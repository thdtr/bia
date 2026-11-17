//! The text of an open file and its cursors, with every editing command.
//!
//! No GUI types here: the editor widget turns keys and clicks into these
//! calls and draws what [`Editor`] holds. Offsets are bytes into the text,
//! always on a char boundary. Every command works with several cursors.

mod display;
mod motion;
mod selection;
mod transaction;
mod undo;

pub use display::DisplayMap;

pub use motion::{char_width, column_for_display, display_column, next_grapheme, prev_grapheme};
pub use selection::{Selection, Selections};
pub use transaction::{Assoc, Edit, Transaction};

use crate::editing::{self, LineEdit};
use crate::indent::Indent;
use crate::search::is_word;
use ropey::Rope;
use std::ops::Range;
use std::time::Instant;
use undo::{Kind, Undo};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Motion {
    Left,
    Right,
    WordLeft,
    WordRight,
    Up,
    Down,
    /// Up or down by this many lines.
    PageUp(usize),
    PageDown(usize),
    /// To the first non-blank character, or to column 0 if already there.
    LineStart,
    LineEnd,
    DocumentStart,
    DocumentEnd,
}

/// What a command does at one selection.
enum Outcome {
    /// Make `edit`. The selection goes to `anchor` and `head`, offsets into
    /// the edit's new text.
    Edit {
        edit: Edit,
        anchor: usize,
        head: usize,
    },
    /// Only move the selection (offsets in the text before the command).
    Move(Selection),
    Keep,
}

impl Outcome {
    fn replace(range: Range<usize>, text: impl Into<String>) -> Self {
        let text = text.into();
        let end = text.len();
        Self::Edit { edit: Edit::new(range, text), anchor: end, head: end }
    }
}

pub struct Editor {
    rope: Rope,
    selections: Selections,
    undo: Undo,
    saved_state: u64,
    revision: u64,
    pub indent: Indent,
    /// Grammar name, for comment tokens and quote pairs.
    pub syntax: String,
    pub auto_close: bool,
    /// Columns a tab takes on screen.
    pub tab_width: usize,
    /// What the last copy put on the clipboard, if it was whole lines
    /// (nothing selected). Pasting exactly that goes above the cursor's line.
    copied_lines: Option<String>,
    /// Ctrl+D started from a cursor in a word, so it matches whole words.
    next_whole_word: bool,
    /// Recent changes, so caches (highlighting, wrapping) redo only what
    /// changed.
    changes: Vec<Change>,
}

#[derive(Debug, Clone, Copy)]
struct Change {
    revision: u64,
    /// First line touched; the text before it is as it was.
    first: usize,
    /// The line after the last one touched, in the new text.
    end: usize,
    lines_before: usize,
    lines_after: usize,
}

/// Which lines changed since some revision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Changed {
    Nothing,
    /// Lines `first..old_end` of the old text are now `first..new_end`;
    /// the lines after them only moved.
    Lines {
        first: usize,
        old_end: usize,
        new_end: usize,
    },
    /// Older than the log: assume everything.
    Everything,
}

/// Recent changes kept in [`Editor::first_changed_line`]'s log.
const CHANGE_LOG: usize = 256;

/// What a mouse drag selects by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unit {
    Char,
    Word,
    Line,
}

impl Editor {
    /// `text` must use `\n` line breaks only.
    pub fn new(text: &str, indent: Indent) -> Self {
        Self {
            rope: Rope::from_str(text),
            selections: Selections::single(Selection::cursor(0)),
            undo: Undo::default(),
            saved_state: 0,
            revision: 0,
            indent,
            syntax: String::new(),
            auto_close: true,
            tab_width: 4,
            copied_lines: None,
            next_whole_word: false,
            changes: Vec::new(),
        }
    }

    /// The first line that may differ from the text at revision `since`,
    /// or `None` if nothing changed. Too old a revision gives line 0.
    pub fn first_changed_line(&self, since: u64) -> Option<usize> {
        match self.changed_lines(since) {
            Changed::Nothing => None,
            Changed::Lines { first, .. } => Some(first),
            Changed::Everything => Some(0),
        }
    }

    /// Which lines changed since revision `since`.
    pub fn changed_lines(&self, since: u64) -> Changed {
        if since >= self.revision {
            return Changed::Nothing;
        }
        let covered = self.changes.first().is_some_and(|c| c.revision <= since + 1);
        if !covered {
            return Changed::Everything;
        }
        let recent: Vec<&Change> = self.changes.iter().filter(|c| c.revision > since).collect();
        match recent.as_slice() {
            [] => Changed::Nothing,
            [c] => Changed::Lines {
                first: c.first,
                old_end: (c.end + c.lines_before).saturating_sub(c.lines_after).max(c.first),
                new_end: c.end,
            },
            // Several: one range covering them all, in the newest text.
            [c, rest @ ..] => {
                let (mut first, mut end) = (c.first, c.end);
                for c in rest {
                    let growth = c.lines_after as isize - c.lines_before as isize;
                    // What's after this change's lines moved with it.
                    let moved = if end > c.first { end.saturating_add_signed(growth) } else { end };
                    end = moved.max(c.end);
                    first = first.min(c.first);
                }
                let before = c.lines_before;
                let after = self.line_count();
                Changed::Lines {
                    first,
                    old_end: (end + before).saturating_sub(after).max(first),
                    new_end: end.max(first),
                }
            }
        }
    }

    /// Logs a change to lines `first..end` (of the new text).
    fn changed(&mut self, first: usize, end: usize, lines_before: usize) {
        self.revision += 1;
        let lines_after = self.line_count();
        let first = first.min(lines_after);
        self.changes.push(Change {
            revision: self.revision,
            first,
            end: end.clamp(first, lines_after),
            lines_before,
            lines_after,
        });
        if self.changes.len() > CHANGE_LOG {
            self.changes.drain(..self.changes.len() - CHANGE_LOG);
        }
    }

    /// A mouse drag that started on `origin` (a point, word or line) and
    /// is now at `at`: selects from one to the other, by `unit`.
    pub fn drag(&mut self, origin: Range<usize>, at: usize, unit: Unit) {
        let at = at.min(self.len());
        let target = match unit {
            Unit::Char => at..at,
            Unit::Word => self.word_range(at),
            Unit::Line => self.line_range(at),
        };
        let selection = if target.start < origin.start {
            Selection::new(origin.end, target.start)
        } else {
            Selection::new(origin.start, target.end.max(origin.end))
        };
        let primary = self.selections.primary_index();
        let mut list = self.selections.as_slice().to_vec();
        list[primary] = selection;
        self.selections = Selections::new(list, primary).clamped(self.len());
    }

    /// The bracket matching the one just before or just after `at`, as the
    /// offsets of both. Strings and comments aren't told apart.
    pub fn matching_bracket(&self, at: usize) -> Option<(usize, usize)> {
        const LIMIT: usize = 20_000;
        let pairs = [('(', ')'), ('[', ']'), ('{', '}')];
        let candidates = [
            motion::char_after(&self.rope, at).map(|c| (at, c)),
            motion::char_before(&self.rope, at).map(|c| (at - c.len_utf8(), c)),
        ];
        for (pos, c) in candidates.into_iter().flatten() {
            if let Some(&(open, close)) = pairs.iter().find(|(o, _)| *o == c) {
                let mut depth = 0usize;
                let mut offset = pos;
                for ch in self.rope.byte_slice(pos..).chars().take(LIMIT) {
                    if ch == open {
                        depth += 1;
                    } else if ch == close {
                        depth -= 1;
                        if depth == 0 {
                            return Some((pos, offset));
                        }
                    }
                    offset += ch.len_utf8();
                }
            } else if let Some(&(open, close)) = pairs.iter().find(|(_, cl)| *cl == c) {
                let mut depth = 0usize;
                let mut chars = self.rope.chars_at(self.rope.byte_to_char(pos + 1));
                let mut offset = pos + 1;
                for _ in 0..LIMIT {
                    let Some(ch) = chars.prev() else { break };
                    offset -= ch.len_utf8();
                    if ch == close {
                        depth += 1;
                    } else if ch == open {
                        depth -= 1;
                        if depth == 0 {
                            return Some((offset, pos));
                        }
                    }
                }
            }
        }
        None
    }

    // ── Reading ──────────────────────────────────────────────────────────

    pub fn rope(&self) -> &Rope {
        &self.rope
    }

    pub fn text(&self) -> String {
        self.rope.to_string()
    }

    pub fn len(&self) -> usize {
        self.rope.len_bytes()
    }

    pub fn is_empty(&self) -> bool {
        self.rope.len_bytes() == 0
    }

    pub fn slice(&self, range: Range<usize>) -> String {
        self.rope.byte_slice(range).to_string()
    }

    /// Lines, counting the empty one after a final `\n`.
    pub fn line_count(&self) -> usize {
        self.rope.len_lines()
    }

    pub fn line_of(&self, at: usize) -> usize {
        self.rope.byte_to_line(at.min(self.len()))
    }

    pub fn line_start(&self, line: usize) -> usize {
        self.rope.line_to_byte(line.min(self.line_count()))
    }

    /// Line `line` without its `\n`.
    pub fn line(&self, line: usize) -> String {
        if line >= self.line_count() {
            return String::new();
        }
        let mut s = self.rope.line(line).to_string();
        if s.ends_with('\n') {
            s.pop();
        }
        s
    }

    /// Offset of the end of line `line`, before its `\n`.
    pub fn line_end(&self, line: usize) -> usize {
        self.line_start(line) + self.line(line).len()
    }

    /// Line and byte column of `at`.
    pub fn line_col(&self, at: usize) -> (usize, usize) {
        let line = self.line_of(at);
        (line, at.min(self.len()) - self.line_start(line))
    }

    /// Offset of `line` and byte `column`, clamped to the text.
    pub fn offset(&self, line: usize, column: usize) -> usize {
        if line >= self.line_count() {
            return self.len();
        }
        let content = self.line(line);
        let mut column = column.min(content.len());
        while !content.is_char_boundary(column) {
            column -= 1;
        }
        self.line_start(line) + column
    }

    pub fn selections(&self) -> &Selections {
        &self.selections
    }

    /// Goes up with every change to the text, undo included.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn is_dirty(&self) -> bool {
        self.undo.state() != self.saved_state
    }

    pub fn mark_saved(&mut self) {
        self.saved_state = self.undo.state();
    }

    /// 1-based line and character column of the primary cursor.
    pub fn cursor_line_col(&self) -> (usize, usize) {
        let (line, col) = self.line_col(self.selections.primary().head);
        (line + 1, self.line(line)[..col].chars().count() + 1)
    }

    // ── Selections ───────────────────────────────────────────────────────

    pub fn set_selections(&mut self, selections: Selections) {
        self.undo.seal();
        self.selections = selections.clamped(self.len());
    }

    /// One selection over `range`.
    pub fn select(&mut self, range: Range<usize>) {
        self.set_selections(Selections::single(Selection::new(range.start, range.end)));
    }

    /// Moves every cursor. With `extend`, the selections grow instead.
    pub fn move_cursors(&mut self, motion: Motion, extend: bool) {
        self.undo.seal();
        self.next_whole_word = false;
        let next = self.selections.map(|s| self.moved(s, motion, extend));
        self.selections = next;
    }

    fn moved(&self, s: Selection, motion: Motion, extend: bool) -> Selection {
        // Left and Right first collapse a selection to the side they point to.
        if !extend && !s.is_empty() {
            match motion {
                Motion::Left => return Selection::cursor(s.start()),
                Motion::Right => return Selection::cursor(s.end()),
                _ => {}
            }
        }
        let head = s.head;
        let to = |at: usize| s.moved(at, extend);
        match motion {
            Motion::Left => to(prev_grapheme(&self.rope, head)),
            Motion::Right => to(next_grapheme(&self.rope, head)),
            Motion::WordLeft => to(motion::word_left(&self.rope, head)),
            Motion::WordRight => to(motion::word_right(&self.rope, head)),
            Motion::Up => self.vertical(s, -1, extend),
            Motion::Down => self.vertical(s, 1, extend),
            Motion::PageUp(n) => self.vertical(s, -(n as isize), extend),
            Motion::PageDown(n) => self.vertical(s, n as isize, extend),
            Motion::LineStart => {
                let (line, col) = self.line_col(head);
                let content = self.line(line);
                let first = content.len() - content.trim_start_matches([' ', '\t']).len();
                let column = if col == first { 0 } else { first };
                to(self.line_start(line) + column)
            }
            Motion::LineEnd => to(self.line_end(self.line_of(head))),
            Motion::DocumentStart => to(0),
            Motion::DocumentEnd => to(self.len()),
        }
    }

    /// `s` moved `delta` lines, aiming for its goal column. Past the first
    /// or last line, the cursor goes to the start or end of the text.
    fn vertical(&self, s: Selection, delta: isize, extend: bool) -> Selection {
        let (line, col) = self.line_col(s.head);
        let goal = s.goal.unwrap_or_else(|| display_column(&self.line(line), col, self.tab_width));
        let target = line as isize + delta;
        let head = if target < 0 {
            0
        } else if target as usize >= self.line_count() {
            self.len()
        } else {
            let t = target as usize;
            self.line_start(t) + column_for_display(&self.line(t), goal, self.tab_width)
        };
        let anchor = if extend { s.anchor } else { head };
        Selection { anchor, head, goal: Some(goal) }
    }

    /// Up and Down (and by pages) when long lines wrap: by rows on screen,
    /// keeping the column within the row. Without wrapping, by lines.
    pub fn move_rows(&mut self, map: &DisplayMap, delta: isize, extend: bool) {
        self.undo.seal();
        self.next_whole_word = false;
        let next = self.selections.map(|s| match map.wrap() {
            Some(_) => self.vertical_rows(map, s, delta, extend),
            None => self.vertical(s, delta, extend),
        });
        self.selections = next;
    }

    fn vertical_rows(
        &self,
        map: &DisplayMap,
        s: Selection,
        delta: isize,
        extend: bool,
    ) -> Selection {
        let tab = self.tab_width;
        let (line, col) = self.line_col(s.head);
        let text = self.line(line);
        let starts = map.row_starts(&text);
        let k = starts.partition_point(|&x| x <= col).saturating_sub(1);
        let base = display_column(&text, starts[k], tab);
        let goal = s.goal.unwrap_or_else(|| display_column(&text, col, tab) - base);
        let target = (map.first_row(line) + k) as isize + delta;
        let head = if target < 0 {
            0
        } else if target as usize >= map.row_count() {
            self.len()
        } else {
            let row = target as usize;
            let line = map.line_of_row(row);
            let text = self.line(line);
            let starts = map.row_starts(&text);
            let k = (row - map.first_row(line)).min(starts.len() - 1);
            let start = starts[k];
            let base = display_column(&text, start, tab);
            let mut column = column_for_display(&text, base + goal, tab).max(start);
            // The end of a row that wraps is the start of the next one.
            if let Some(&end) = starts.get(k + 1)
                && column >= end
            {
                column = text[..end].char_indices().next_back().map_or(start, |(i, _)| i);
            }
            self.line_start(line) + column
        };
        let anchor = if extend { s.anchor } else { head };
        Selection { anchor, head, goal: Some(goal) }
    }

    /// The display column of `at`: where it is on screen in its line.
    pub fn display_column(&self, at: usize) -> usize {
        let (line, col) = self.line_col(at);
        display_column(&self.line(line), col, self.tab_width)
    }

    pub fn select_all(&mut self) {
        self.select(0..self.len());
    }

    /// A click: one cursor at `at`, or with `extend` (Shift), the primary
    /// selection stretched to it.
    pub fn click(&mut self, at: usize, extend: bool) {
        let at = at.min(self.len());
        let primary = self.selections.primary();
        let anchor = if extend { primary.anchor } else { at };
        self.set_selections(Selections::single(Selection::new(anchor, at)));
    }

    /// Alt+click: adds a cursor at `at`, or removes the one already there.
    pub fn toggle_cursor(&mut self, at: usize) {
        let at = at.min(self.len());
        let list = self.selections.as_slice();
        if list.len() > 1
            && let Some(i) = list.iter().position(|s| s.is_empty() && s.head == at)
        {
            let mut rest = list.to_vec();
            rest.remove(i);
            self.set_selections(Selections::new(rest, 0));
            return;
        }
        let next = self.selections.with(Selection::cursor(at));
        self.set_selections(next);
    }

    /// Double click: the word at `at` (or the run of spaces or punctuation).
    pub fn word_range(&self, at: usize) -> Range<usize> {
        let word = motion::word_at(&self.rope, at);
        if !word.is_empty() {
            return word;
        }
        let end = next_grapheme(&self.rope, at);
        if end > at && motion::char_after(&self.rope, at) != Some('\n') { at..end } else { at..at }
    }

    /// Triple click: the whole line at `at`, with its `\n`.
    pub fn line_range(&self, at: usize) -> Range<usize> {
        let line = self.line_of(at);
        self.line_start(line)..self.line_start(line + 1)
    }

    /// Escape: back to one cursor, or with one, to no selection. Returns
    /// whether anything changed.
    pub fn collapse(&mut self) -> bool {
        let primary = self.selections.primary();
        let next = if self.selections.len() > 1 {
            self.selections.only_primary()
        } else if !primary.is_empty() {
            Selections::single(Selection::cursor(primary.head))
        } else {
            return false;
        };
        self.set_selections(next);
        true
    }

    /// Ctrl+Alt+Up / Down: a new cursor on the line above the first cursor
    /// (or below the last), in the same column.
    pub fn add_cursor_vertically(&mut self, up: bool) {
        let list = self.selections.as_slice();
        let from = if up { list[0] } else { list[list.len() - 1] };
        let from = Selection { anchor: from.head, ..from };
        let moved = self.vertical(from, if up { -1 } else { 1 }, false);
        let (line, _) = self.line_col(from.head);
        if self.line_of(moved.head) == line {
            return;
        }
        let next = self.selections.with(moved);
        self.selections = next;
        self.undo.seal();
    }

    /// Ctrl+D: selects the word at the cursor; again, the next place the
    /// selected text appears, adding a selection there.
    pub fn select_next_occurrence(&mut self) {
        let primary = self.selections.primary();
        if self.selections.len() == 1 && primary.is_empty() {
            let word = motion::word_at(&self.rope, primary.head);
            if !word.is_empty() {
                self.select(word);
                self.next_whole_word = true;
            }
            return;
        }
        let needle = self.slice(primary.range());
        if needle.is_empty() {
            return;
        }
        let text = self.text();
        let after = self.selections.iter().map(Selection::end).max().unwrap_or(0);
        let taken: Vec<Range<usize>> = self.selections.iter().map(Selection::range).collect();
        let found = self
            .occurrences(&text, &needle)
            .into_iter()
            .filter(|r| !taken.contains(r))
            .min_by_key(|r| (r.start < after, r.start));
        if let Some(r) = found {
            let next = self.selections.with(Selection::new(r.start, r.end));
            self.selections = next;
            self.undo.seal();
        }
    }

    /// Ctrl+Shift+L: a selection on every place the selected text appears.
    pub fn select_all_occurrences(&mut self) {
        let primary = self.selections.primary();
        let mut range = primary.range();
        if primary.is_empty() {
            range = motion::word_at(&self.rope, primary.head);
            self.next_whole_word = true;
        }
        let needle = self.slice(range.clone());
        if needle.is_empty() {
            return;
        }
        let text = self.text();
        let all: Vec<Selection> = self
            .occurrences(&text, &needle)
            .into_iter()
            .map(|r| Selection::new(r.start, r.end))
            .collect();
        let primary = all.iter().position(|s| s.range() == range).unwrap_or(0);
        if !all.is_empty() {
            self.set_selections(Selections::new(all, primary));
        }
    }

    fn occurrences(&self, text: &str, needle: &str) -> Vec<Range<usize>> {
        let whole = self.next_whole_word;
        text.match_indices(needle)
            .map(|(i, m)| i..i + m.len())
            .filter(|r| {
                !whole
                    || (!text[..r.start].chars().next_back().is_some_and(is_word)
                        && !text[r.end..].chars().next().is_some_and(is_word))
            })
            .collect()
    }

    // ── Undo ─────────────────────────────────────────────────────────────

    pub fn undo(&mut self) -> bool {
        let Some((txs, selections)) = self.undo.undo() else { return false };
        for tx in &txs {
            self.apply_logged(tx);
        }
        self.selections = selections.clamped(self.len());
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some((txs, selections)) = self.undo.redo() else { return false };
        for tx in &txs {
            self.apply_logged(tx);
        }
        self.selections = selections.clamped(self.len());
        true
    }

    /// Applies `tx` to the text and logs which lines it changed.
    fn apply_logged(&mut self, tx: &Transaction) {
        let Some(last) = tx.edits().len().checked_sub(1) else { return };
        let lines = self.line_count();
        tx.apply(&mut self.rope);
        let end = tx.new_starts()[last] + tx.edits()[last].text.len();
        self.changed(self.line_of(tx.edits()[0].range.start), self.line_of(end) + 1, lines);
    }

    /// Ends the current undo step.
    pub fn seal(&mut self) {
        self.undo.seal();
    }

    // ── Applying changes ─────────────────────────────────────────────────

    /// Applies `tx` as one undo step. `after` gives the new selections,
    /// looking at the text once changed.
    fn commit(
        &mut self,
        tx: Transaction,
        kind: Kind,
        after: impl FnOnce(&Self, &Transaction) -> Selections,
    ) {
        if tx.is_empty() {
            let next = after(self, &tx).clamped(self.len());
            self.selections = next;
            return;
        }
        let before = self.selections.clone();
        let inverse = tx.invert(&self.rope);
        self.apply_logged(&tx);
        let next = after(self, &tx).clamped(self.len());
        self.undo.record(tx, inverse, &before, &next, kind, Instant::now());
        self.selections = next;
    }

    /// Runs `f` at every selection and applies the edits together.
    fn change_each(&mut self, kind: Kind, mut f: impl FnMut(&Self, Selection) -> Outcome) {
        self.next_whole_word = false;
        let list: Vec<Selection> = self.selections.as_slice().to_vec();
        let outcomes: Vec<Outcome> = list.iter().map(|&s| f(self, s)).collect();
        let edits: Vec<Edit> = outcomes
            .iter()
            .filter_map(|o| match o {
                Outcome::Edit { edit, .. } => Some(edit.clone()),
                _ => None,
            })
            .collect();
        let tx = Transaction::new(edits);
        let starts = tx.new_starts();
        let primary = self.selections.primary_index();
        self.commit(tx, kind, |_, tx| {
            let mut kept = tx.edits().iter().zip(starts).peekable();
            let next = list
                .iter()
                .zip(&outcomes)
                .map(|(s, o)| {
                    let map = |s: &Selection| {
                        Selection::new(tx.map(s.anchor, Assoc::After), tx.map(s.head, Assoc::After))
                    };
                    match o {
                        Outcome::Edit { edit, anchor, head } => {
                            match kept.peek() {
                                Some(&(e, start)) if e == edit => {
                                    kept.next();
                                    Selection::new(start + anchor, start + head)
                                }
                                // Dropped for overlapping another cursor's edit.
                                _ => map(s),
                            }
                        }
                        Outcome::Move(m) => map(m),
                        Outcome::Keep => map(s),
                    }
                })
                .collect();
            Selections::new(next, primary)
        });
    }

    /// Lines each selection covers (a selection ending at the start of a
    /// line doesn't cover it), merged when they touch: `(first, last)`.
    fn line_blocks(&self) -> Vec<(usize, usize)> {
        let mut blocks: Vec<(usize, usize)> = Vec::new();
        for s in self.selections.iter() {
            let first = self.line_of(s.start());
            let (end_line, end_col) = self.line_col(s.end());
            let last = if end_col == 0 && end_line > first { end_line - 1 } else { end_line };
            match blocks.last_mut() {
                Some(b) if first <= b.1 + 1 => b.1 = b.1.max(last),
                _ => blocks.push((first, last)),
            }
        }
        blocks
    }

    /// Edits inside lines (no line breaks added or removed), computed for
    /// each block of selected lines. Selections stay on the same text.
    fn edit_lines(&mut self, per_block: impl Fn(&Self, &[String]) -> Vec<LineEdit>) {
        let blocks = self.line_blocks();
        let mut edits = Vec::new();
        let mut per_line: Vec<(usize, usize, Vec<LineEdit>)> = Vec::new();
        for (first, last) in blocks {
            let lines: Vec<String> = (first..=last).map(|n| self.line(n)).collect();
            let line_edits = per_block(self, &lines);
            for e in &line_edits {
                let at = self.line_start(first + e.line) + e.at;
                edits.push(Edit::new(at..at + e.remove, e.insert.clone()));
            }
            per_line.push((first, last, line_edits));
        }
        let points: Vec<(usize, usize)> = self
            .selections
            .iter()
            .flat_map(|s| [s.anchor, s.head])
            .map(|p| self.line_col(p))
            .collect();
        let old_starts: Vec<usize> =
            points.iter().map(|&(line, _)| self.line_start(line)).collect();
        let primary = self.selections.primary_index();
        self.undo.seal();
        self.commit(Transaction::new(edits), Kind::Other, |_, tx| {
            let new = |i: usize| {
                let (line, col) = points[i];
                let block = per_line.iter().find(|(f, l, _)| (*f..=*l).contains(&line));
                match block {
                    Some((first, _, le)) => {
                        tx.map(old_starts[i], Assoc::Before)
                            + editing::shift_column(col, line - first, le)
                    }
                    None => tx.map(old_starts[i] + col, Assoc::After),
                }
            };
            let list = (0..points.len() / 2).map(|k| Selection::new(new(2 * k), new(2 * k + 1)));
            Selections::new(list.collect(), primary)
        });
        self.undo.seal();
    }

    /// Edits whole blocks of lines; each selection point moves by the
    /// offset `place` gives for it (the block it's in, its old offset and
    /// the transaction).
    fn edit_blocks(
        &mut self,
        edits: Vec<Edit>,
        place: impl Fn(&Self, &Transaction, usize) -> usize,
    ) {
        let primary = self.selections.primary_index();
        let points: Vec<(usize, usize)> =
            self.selections.iter().map(|s| (s.anchor, s.head)).collect();
        self.undo.seal();
        self.commit(Transaction::new(edits), Kind::Other, |ed, tx| {
            let list = points
                .iter()
                .map(|&(a, h)| Selection::new(place(ed, tx, a), place(ed, tx, h)))
                .collect();
            Selections::new(list, primary)
        });
        self.undo.seal();
    }

    // ── Typing ───────────────────────────────────────────────────────────

    /// A character typed. Brackets and quotes may close themselves, wrap
    /// the selection, or step over a closing one.
    pub fn type_char(&mut self, c: char) {
        if c == '\n' || c == '\r' {
            return self.newline();
        }
        let n = c.len_utf8();
        self.change_each(Kind::Typing { space: c.is_whitespace() }, |ed, s| {
            let close = editing::closing(c).filter(|_| ed.auto_close);
            if let Some(close) = close
                && !s.is_empty()
            {
                let inner = ed.slice(s.range());
                let len = inner.len();
                let (anchor, head) = if s.head >= s.anchor { (n, n + len) } else { (n + len, n) };
                let edit = Edit::new(s.range(), format!("{c}{inner}{close}"));
                return Outcome::Edit { edit, anchor, head };
            }
            if ed.auto_close && s.is_empty() {
                let before = motion::char_before(&ed.rope, s.head);
                let after = motion::char_after(&ed.rope, s.head);
                // Step over a closing half instead of doubling it. Quotes
                // only right after text, so a quote before a string opens one.
                let step = after == Some(c)
                    && (")]}".contains(c)
                        || (editing::is_quote(c) && before.is_some_and(|b| !b.is_whitespace())));
                if step {
                    return Outcome::Move(Selection::cursor(s.head + n));
                }
                if let Some(close) =
                    close.filter(|_| editing::should_close(c, before, after, &ed.syntax))
                {
                    let edit = Edit::new(s.head..s.head, format!("{c}{close}"));
                    return Outcome::Edit { edit, anchor: n, head: n };
                }
            }
            Outcome::replace(s.range(), c.to_string())
        });
    }

    /// Text from the clipboard or an input method. With as many lines as
    /// cursors, each cursor gets one. Lines copied whole go above the line.
    pub fn paste(&mut self, text: &str) {
        let text = text.replace("\r\n", "\n").replace('\r', "\n");
        let whole_lines = self.copied_lines.as_deref() == Some(text.as_str())
            && self.selections.iter().all(Selection::is_empty);
        let parts: Vec<&str> = text.strip_suffix('\n').unwrap_or(&text).split('\n').collect();
        let spread = self.selections.len() > 1 && parts.len() == self.selections.len();
        let mut index = 0;
        self.undo.seal();
        self.change_each(Kind::Other, |ed, s| {
            let piece = match (spread, whole_lines) {
                (true, true) => format!("{}\n", parts[index]),
                (true, false) => parts[index].to_owned(),
                (false, _) => text.clone(),
            };
            index += 1;
            if whole_lines {
                let start = ed.line_start(ed.line_of(s.head));
                let at = piece.len() + s.head - start;
                return Outcome::Edit {
                    edit: Edit::new(start..start, piece),
                    anchor: at,
                    head: at,
                };
            }
            Outcome::replace(s.range(), piece)
        });
        self.undo.seal();
    }

    /// Text inserted as is at every selection (no pairs, no indentation).
    pub fn insert(&mut self, text: &str) {
        self.undo.seal();
        self.change_each(Kind::Other, |_, s| Outcome::replace(s.range(), text));
        self.undo.seal();
    }

    /// Ctrl+C. With nothing selected, the cursors' whole lines.
    pub fn copy(&mut self) -> String {
        if self.selections.iter().all(Selection::is_empty) {
            let mut lines: Vec<usize> =
                self.selections.iter().map(|s| self.line_of(s.head)).collect();
            lines.dedup();
            let text: String = lines.iter().map(|&l| self.line(l) + "\n").collect();
            self.copied_lines = Some(text.clone());
            return text;
        }
        self.copied_lines = None;
        let parts: Vec<String> = self
            .selections
            .iter()
            .filter(|s| !s.is_empty())
            .map(|s| self.slice(s.range()))
            .collect();
        parts.join("\n")
    }

    /// Ctrl+X. With nothing selected, cuts the cursors' lines.
    pub fn cut(&mut self) -> String {
        let text = self.copy();
        if self.copied_lines.is_some() {
            self.delete_lines();
        } else {
            self.undo.seal();
            self.change_each(Kind::Other, |_, s| Outcome::replace(s.range(), ""));
            self.undo.seal();
        }
        text
    }

    /// Enter: a new line with the indentation (see [`Indent::newline`]).
    pub fn newline(&mut self) {
        self.undo.seal();
        self.change_each(Kind::Other, |ed, s| {
            let (start_line, start_col) = ed.line_col(s.start());
            let (end_line, end_col) = ed.line_col(s.end());
            let mut line = ed.line(start_line);
            line.truncate(start_col);
            line.push_str(&ed.line(end_line)[end_col..]);
            let (insert, caret) = ed.indent.newline(&line, start_col);
            Outcome::Edit { edit: Edit::new(s.range(), insert), anchor: caret, head: caret }
        });
    }

    /// Backspace: the selection, a pair of brackets, one indentation level
    /// in leading spaces, or one grapheme.
    pub fn backspace(&mut self) {
        let kind = self.deleting_kind();
        self.change_each(kind, |ed, s| {
            if !s.is_empty() {
                return Outcome::replace(s.range(), "");
            }
            let at = s.head;
            if at == 0 {
                return Outcome::Keep;
            }
            let before = motion::char_before(&ed.rope, at);
            let after = motion::char_after(&ed.rope, at);
            if ed.auto_close
                && let (Some(b), Some(a)) = (before, after)
                && editing::closing(b) == Some(a)
            {
                return Outcome::replace(at - b.len_utf8()..at + a.len_utf8(), "");
            }
            let (line, col) = ed.line_col(at);
            if let Some(n) = ed.indent.backspace(&ed.line(line)[..col]) {
                return Outcome::replace(at - n..at, "");
            }
            Outcome::replace(prev_grapheme(&ed.rope, at)..at, "")
        });
    }

    /// Delete: the selection or the next grapheme.
    pub fn delete(&mut self) {
        let kind = self.deleting_kind();
        self.change_each(kind, |ed, s| {
            if !s.is_empty() {
                return Outcome::replace(s.range(), "");
            }
            let end = next_grapheme(&ed.rope, s.head);
            if end == s.head { Outcome::Keep } else { Outcome::replace(s.head..end, "") }
        });
    }

    /// Ctrl+Backspace / Ctrl+Delete: to the previous or next word stop.
    pub fn delete_word(&mut self, forward: bool) {
        let kind = self.deleting_kind();
        self.change_each(kind, |ed, s| {
            if !s.is_empty() {
                return Outcome::replace(s.range(), "");
            }
            let range = if forward {
                s.head..motion::word_right(&ed.rope, s.head)
            } else {
                motion::word_left(&ed.rope, s.head)..s.head
            };
            if range.is_empty() { Outcome::Keep } else { Outcome::replace(range, "") }
        });
    }

    fn deleting_kind(&self) -> Kind {
        if self.selections.iter().all(Selection::is_empty) { Kind::Deleting } else { Kind::Other }
    }

    // ── Indentation and comments ─────────────────────────────────────────

    /// Tab: one level at each cursor, or on every selected line.
    pub fn indent(&mut self) {
        if self.selections.iter().all(Selection::is_empty) {
            self.undo.seal();
            self.change_each(Kind::Other, |ed, s| {
                let (line, col) = ed.line_col(s.head);
                let insert = ed.indent.insert_at(&ed.line(line)[..col]);
                Outcome::replace(s.head..s.head, insert)
            });
            self.undo.seal();
        } else {
            self.shift_lines(true);
        }
    }

    /// Shift+Tab: one level less on the selected lines.
    pub fn outdent(&mut self) {
        self.shift_lines(false);
    }

    fn shift_lines(&mut self, add: bool) {
        self.edit_lines(|ed, lines| {
            let unit = ed.indent.unit();
            lines
                .iter()
                .enumerate()
                .filter_map(|(i, line)| {
                    let (remove, insert) = if add {
                        (0, if line.is_empty() { return None } else { unit.clone() })
                    } else {
                        (ed.indent.outdent(line), String::new())
                    };
                    (remove > 0 || !insert.is_empty()).then_some(LineEdit {
                        line: i,
                        at: 0,
                        remove,
                        insert,
                    })
                })
                .collect()
        });
    }

    /// Ctrl+/. Returns false when BIA doesn't know how this language writes
    /// comments.
    pub fn toggle_comment(&mut self) -> bool {
        let Some(comments) = editing::comments(&self.syntax) else { return false };
        self.edit_lines(|_, lines| {
            let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
            editing::toggle_comment(&refs, comments)
        });
        true
    }

    // ── Whole lines ──────────────────────────────────────────────────────

    /// Alt+Up / Down: swaps the selected lines with the line above or below.
    pub fn move_lines(&mut self, up: bool) {
        let blocks = self.line_blocks();
        let (first, last) = (blocks[0].0, blocks[blocks.len() - 1].1);
        if (up && first == 0) || (!up && last + 1 >= self.line_count()) {
            return;
        }
        let mut edits = Vec::new();
        // Each block's points, up to the start of the line after it.
        let mut shifts: Vec<(std::ops::RangeInclusive<usize>, isize)> = Vec::new();
        for &(first, last) in &blocks {
            let (start, end) = (self.line_start(first), self.line_end(last));
            let block = self.slice(start..end);
            if up {
                let above = self.line(first - 1);
                edits.push(Edit::new(self.line_start(first - 1)..end, format!("{block}\n{above}")));
                shifts.push((start..=end + 1, -(above.len() as isize + 1)));
            } else {
                let below = self.line(last + 1);
                edits.push(Edit::new(start..self.line_end(last + 1), format!("{below}\n{block}")));
                shifts.push((start..=end + 1, below.len() as isize + 1));
            }
        }
        self.edit_blocks(edits, |_, _, p| {
            match shifts.iter().find(|(r, _)| r.contains(&p)) {
                Some((_, delta)) => p.saturating_add_signed(*delta),
                // Each edit keeps its length, so nothing else moves.
                None => p,
            }
        });
    }

    /// Shift+Alt+Down / Up: copies the selected lines below themselves.
    /// Down moves the selections to the copy.
    pub fn duplicate_lines(&mut self, down: bool) {
        let blocks: Vec<(usize, usize)> = self
            .line_blocks()
            .into_iter()
            .map(|(f, l)| (self.line_start(f), self.line_end(l)))
            .collect();
        let edits = blocks
            .iter()
            .map(|&(s, e)| Edit::new(e..e, format!("\n{}", self.slice(s..e))))
            .collect();
        self.edit_blocks(edits, |_, tx, p| {
            let Some(&(start, end)) = blocks.iter().find(|(s, e)| (*s..=*e + 1).contains(&p))
            else {
                return tx.map(p, Assoc::After);
            };
            let copy = end - start + 1;
            match (down, p == end + 1) {
                (true, true) => tx.map(p, Assoc::After),
                (true, false) => tx.map(p, Assoc::Before) + copy,
                (false, true) => tx.map(p - 1, Assoc::Before) + 1,
                (false, false) => tx.map(p, Assoc::Before),
            }
        });
    }

    /// Ctrl+Shift+K: deletes the selected lines. Each cursor lands in its
    /// column on the line that takes their place.
    pub fn delete_lines(&mut self) {
        let count = self.line_count();
        let len = self.len();
        let blocks = self.line_blocks();
        let edits: Vec<Edit> = blocks
            .iter()
            .map(|&(first, last)| {
                let range = if last + 1 < count {
                    self.line_start(first)..self.line_start(last + 1)
                } else if first > 0 {
                    self.line_end(first - 1)..len
                } else {
                    0..len
                };
                Edit::new(range, "")
            })
            .collect();
        let goals: Vec<usize> =
            self.selections.iter().map(|s| self.display_column(s.head)).collect();
        let starts: Vec<usize> = edits.iter().map(|e| e.range.start).collect();
        let primary = self.selections.primary_index();
        let heads: Vec<usize> = self.selections.iter().map(|s| s.head).collect();
        self.undo.seal();
        self.commit(Transaction::new(edits), Kind::Other, |ed, tx| {
            let list = heads
                .iter()
                .zip(&goals)
                .map(|(&head, &goal)| {
                    // The line now where the deleted block began (or, at the
                    // end of the text, the line before it).
                    let block = starts.iter().rposition(|&s| s <= head).unwrap_or(0);
                    let line = ed.line_of(tx.map(starts[block], Assoc::Before));
                    let column = column_for_display(&ed.line(line), goal, ed.tab_width);
                    Selection::cursor(ed.line_start(line) + column)
                })
                .collect();
            Selections::new(list, primary)
        });
        self.undo.seal();
    }

    /// Ctrl+L: selects each cursor's line; again, one line more.
    pub fn select_lines(&mut self) {
        let next = self.selections.map(|s| {
            let (start_line, start_col) = self.line_col(s.start());
            let (end_line, end_col) = self.line_col(s.end());
            // Whole lines selected already: take one more.
            let whole = !s.is_empty() && start_col == 0 && end_col == 0;
            let last = if !whole && end_col == 0 && end_line > start_line {
                end_line - 1
            } else {
                end_line
            };
            let end =
                if last + 1 < self.line_count() { self.line_start(last + 1) } else { self.len() };
            Selection::new(self.line_start(start_line), end)
        });
        self.set_selections(next);
    }

    // ── Whole-text changes ───────────────────────────────────────────────

    /// Replaces each range (sorted, not overlapping) as one undo step:
    /// replace all, recovered backups.
    pub fn replace_ranges(&mut self, edits: Vec<(Range<usize>, String)>) {
        let tx = Transaction::new(edits.into_iter().map(|(r, t)| Edit::new(r, t)).collect());
        let before = self.selections.clone();
        self.undo.seal();
        self.commit(tx, Kind::Other, |_, tx| before.after(tx));
        self.undo.seal();
    }
}

#[cfg(test)]
mod tests;
