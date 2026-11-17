//! Undo and redo. Each step is a group of transactions: a word of typing,
//! a run of deleting, or one command. Selections are restored with the text.

use super::selection::Selections;
use super::transaction::Transaction;
use ropey::Rope;
use std::time::{Duration, Instant};

/// Keystrokes further apart than this start a new step.
const GROUP_WINDOW: Duration = Duration::from_millis(1200);
const LIMIT: usize = 5_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Typing one character. Groups with the next, until a space follows a
    /// word.
    Typing { space: bool },
    /// Backspace or Delete with nothing selected.
    Deleting,
    /// Everything else: a step of its own.
    Other,
}

#[derive(Debug, Clone)]
struct Entry {
    tx: Transaction,
    inverse: Transaction,
}

#[derive(Debug, Clone)]
struct Group {
    entries: Vec<Entry>,
    before: Selections,
    after: Selections,
    /// Identifies the text before and after the group, to tell whether the
    /// file is back to how it was saved.
    state_before: u64,
    state_after: u64,
}

impl Group {
    /// No edit in the group touches text before this offset, in any of the
    /// texts along the way, so the text before it is the same throughout.
    fn first_offset(&self) -> usize {
        self.entries
            .iter()
            .flat_map(|e| e.tx.edits().iter().chain(e.inverse.edits()))
            .map(|e| e.range.start)
            .min()
            .unwrap_or(0)
    }
}

#[derive(Debug, Clone, Default)]
pub struct Undo {
    done: Vec<Group>,
    undone: Vec<Group>,
    last: Option<(Kind, Instant)>,
    /// The current text's identity (see [`Group::state_before`]).
    state: u64,
    next_state: u64,
}

impl Undo {
    pub fn state(&self) -> u64 {
        self.state
    }

    /// Records `tx`, just applied, which turned `before` into `after`.
    pub fn record(
        &mut self,
        tx: Transaction,
        inverse: Transaction,
        before: &Selections,
        after: &Selections,
        kind: Kind,
        now: Instant,
    ) {
        self.undone.clear();
        self.next_state += 1;
        let state = self.next_state;
        let joins = match (self.last, kind) {
            (Some((Kind::Typing { space: was }, at)), Kind::Typing { space }) => {
                recent(at, now) && (was || !space)
            }
            (Some((Kind::Deleting, at)), Kind::Deleting) => recent(at, now),
            _ => false,
        };
        match self.done.last_mut() {
            Some(group) if joins => {
                group.entries.push(Entry { tx, inverse });
                group.after = after.clone();
                group.state_after = state;
            }
            _ => {
                self.done.push(Group {
                    entries: vec![Entry { tx, inverse }],
                    before: before.clone(),
                    after: after.clone(),
                    state_before: self.state,
                    state_after: state,
                });
                if self.done.len() > LIMIT {
                    self.done.drain(..self.done.len() - LIMIT);
                }
            }
        }
        self.state = state;
        self.last = (kind != Kind::Other).then_some((kind, now));
    }

    /// Ends the current group: the next edit starts a new step.
    pub fn seal(&mut self) {
        self.last = None;
    }

    /// Takes `rope` back one step. Returns the selections to restore and
    /// an offset before which nothing changed.
    pub fn undo(&mut self, rope: &mut Rope) -> Option<(Selections, usize)> {
        let group = self.done.pop()?;
        let from = group.first_offset();
        for e in group.entries.iter().rev() {
            e.inverse.apply(rope);
        }
        self.state = group.state_before;
        self.last = None;
        let selections = group.before.clone();
        self.undone.push(group);
        Some((selections, from))
    }

    pub fn redo(&mut self, rope: &mut Rope) -> Option<(Selections, usize)> {
        let group = self.undone.pop()?;
        let from = group.first_offset();
        for e in &group.entries {
            e.tx.apply(rope);
        }
        self.state = group.state_after;
        self.last = None;
        let selections = group.after.clone();
        self.done.push(group);
        Some((selections, from))
    }
}

fn recent(at: Instant, now: Instant) -> bool {
    now.saturating_duration_since(at) < GROUP_WINDOW
}
