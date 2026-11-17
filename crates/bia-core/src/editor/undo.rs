//! Undo and redo. Each step is a group of transactions: a word of typing,
//! a run of deleting, or one command. Selections are restored with the text.

use super::selection::Selections;
use super::transaction::Transaction;
use std::time::{Duration, Instant};

/// Keystrokes further apart than this start a new step.
const GROUP_WINDOW: Duration = Duration::from_millis(1200);
const LIMIT: usize = 5_000;
/// Text kept by the steps, before the oldest go. A "replace all" in a
/// 16 MB file keeps two copies of it; a handful of those is plenty.
const MAX_BYTES: usize = if cfg!(test) { 1 << 20 } else { 128 << 20 };

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

impl Entry {
    /// The text it keeps, both ways.
    fn bytes(&self) -> usize {
        let text = |tx: &Transaction| tx.edits().iter().map(|e| e.text.len()).sum::<usize>();
        text(&self.tx) + text(&self.inverse)
    }
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
    /// The text its entries keep.
    bytes: usize,
}

#[derive(Debug, Clone, Default)]
pub struct Undo {
    done: Vec<Group>,
    undone: Vec<Group>,
    last: Option<(Kind, Instant)>,
    /// The current text's identity (see [`Group::state_before`]).
    state: u64,
    next_state: u64,
    /// The text kept by every group, done and undone.
    bytes: usize,
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
        self.bytes -= self.undone.drain(..).map(|g| g.bytes).sum::<usize>();
        self.next_state += 1;
        let state = self.next_state;
        let joins = match (self.last, kind) {
            (Some((Kind::Typing { space: was }, at)), Kind::Typing { space }) => {
                recent(at, now) && (was || !space)
            }
            (Some((Kind::Deleting, at)), Kind::Deleting) => recent(at, now),
            _ => false,
        };
        let entry = Entry { tx, inverse };
        let bytes = entry.bytes();
        self.bytes += bytes;
        match self.done.last_mut() {
            Some(group) if joins => {
                group.entries.push(entry);
                group.after = after.clone();
                group.state_after = state;
                group.bytes += bytes;
            }
            _ => {
                self.done.push(Group {
                    entries: vec![entry],
                    before: before.clone(),
                    after: after.clone(),
                    state_before: self.state,
                    state_after: state,
                    bytes,
                });
            }
        }
        // The oldest steps go first; the newest always stays.
        let mut excess = 0;
        let mut bytes = self.bytes;
        while excess + 1 < self.done.len()
            && (self.done.len() - excess > LIMIT || bytes > MAX_BYTES)
        {
            bytes -= self.done[excess].bytes;
            excess += 1;
        }
        self.done.drain(..excess);
        self.bytes = bytes;
        self.state = state;
        self.last = (kind != Kind::Other).then_some((kind, now));
    }

    /// Ends the current group: the next edit starts a new step.
    pub fn seal(&mut self) {
        self.last = None;
    }

    /// One step back: the transactions to apply, in order, and the
    /// selections to restore.
    pub fn undo(&mut self) -> Option<(Vec<Transaction>, Selections)> {
        let group = self.done.pop()?;
        let txs = group.entries.iter().rev().map(|e| e.inverse.clone()).collect();
        self.state = group.state_before;
        self.last = None;
        let selections = group.before.clone();
        self.undone.push(group);
        Some((txs, selections))
    }

    pub fn redo(&mut self) -> Option<(Vec<Transaction>, Selections)> {
        let group = self.undone.pop()?;
        let txs = group.entries.iter().map(|e| e.tx.clone()).collect();
        self.state = group.state_after;
        self.last = None;
        let selections = group.after.clone();
        self.done.push(group);
        Some((txs, selections))
    }
}

fn recent(at: Instant, now: Instant) -> bool {
    now.saturating_duration_since(at) < GROUP_WINDOW
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::{Edit, Selection};

    fn record(undo: &mut Undo, text: &str) {
        let s = Selections::single(Selection::cursor(0));
        let tx = Transaction::new(vec![Edit::new(0..0, text)]);
        let inverse = Transaction::new(vec![Edit::new(0..text.len(), "")]);
        undo.record(tx, inverse, &s, &s, Kind::Other, Instant::now());
    }

    #[test]
    fn big_steps_push_out_the_oldest() {
        let mut undo = Undo::default();
        record(&mut undo, "small");
        let big = "x".repeat(400 << 10);
        for _ in 0..3 {
            record(&mut undo, &big);
        }
        // 1.2 MB of text is over the limit (1 MB in tests): the small step
        // and the first big one went.
        assert_eq!(undo.done.len(), 2);
        assert_eq!(undo.bytes, 2 * big.len());
        // One step bigger than the limit still stays, alone.
        record(&mut undo, &"y".repeat(2 << 20));
        assert_eq!(undo.done.len(), 1);
        // Undone steps count until something new is recorded.
        assert!(undo.undo().is_some());
        assert_eq!(undo.bytes, 2 << 20);
        record(&mut undo, "z");
        assert_eq!(undo.bytes, 1);
    }
}
