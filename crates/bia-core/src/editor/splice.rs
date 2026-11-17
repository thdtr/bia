//! A journal of recent edits as places in the text they were made to, so a
//! language server can be sent just what changed instead of the whole file.
//!
//! Each transaction becomes a list of [`Splice`]s, last edit first: applied
//! in that order, each one's places are still right when it comes, since
//! the edits after it in the text are the ones already made.

use super::transaction::Transaction;
use ropey::Rope;
use std::collections::VecDeque;

/// A place in the text: its line, and its column counted in bytes and in
/// UTF-16 units (servers take one count or the other).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Place {
    pub line: usize,
    pub byte: usize,
    pub utf16: usize,
}

impl Place {
    fn of(rope: &Rope, at: usize) -> Self {
        let line = rope.byte_to_line(at);
        let start = rope.line_to_byte(line);
        let units = |byte: usize| rope.char_to_utf16_cu(rope.byte_to_char(byte));
        Self { line, byte: at - start, utf16: units(at) - units(start) }
    }
}

/// Replace the text from `start` to `end` with `text`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Splice {
    pub start: Place,
    pub end: Place,
    pub text: String,
}

/// Transactions kept, and how much inserted text, before the oldest go.
/// A server further behind than that gets the whole text instead.
const MAX_ENTRIES: usize = 256;
const MAX_BYTES: usize = 1 << 20;

#[derive(Debug, Default)]
pub(super) struct Journal {
    /// The revision each transaction made, and its splices.
    entries: VecDeque<(u64, Vec<Splice>)>,
    bytes: usize,
}

impl Journal {
    /// Notes `tx`, about to be applied to `rope`, as what makes `revision`.
    pub(super) fn record(&mut self, rope: &Rope, tx: &Transaction, revision: u64) {
        let splices: Vec<Splice> = tx
            .edits()
            .iter()
            .rev()
            .map(|e| Splice {
                start: Place::of(rope, e.range.start),
                end: Place::of(rope, e.range.end),
                text: e.text.clone(),
            })
            .collect();
        self.bytes += splices.iter().map(|s| s.text.len()).sum::<usize>();
        self.entries.push_back((revision, splices));
        while self.entries.len() > MAX_ENTRIES || self.bytes > MAX_BYTES {
            let Some((_, gone)) = self.entries.pop_front() else { break };
            self.bytes -= gone.iter().map(|s| s.text.len()).sum::<usize>();
        }
    }

    /// The splices that take the text from revision `since` to `current`,
    /// in order, or `None` if the journal doesn't go back that far.
    pub(super) fn since(&self, since: u64, current: u64) -> Option<Vec<&Splice>> {
        if since > current {
            return None;
        }
        if since < current && self.entries.front().is_none_or(|(r, _)| *r > since + 1) {
            return None;
        }
        Some(self.entries.iter().filter(|(r, _)| *r > since).flat_map(|(_, s)| s).collect())
    }
}

#[cfg(test)]
mod tests {
    use crate::editor::Editor;
    use crate::indent::Indent;

    /// Applies splices to `text` by line and byte column, as a server would.
    fn apply(text: &str, splices: &[&super::Splice]) -> String {
        let mut text = text.to_owned();
        for s in splices {
            let at = |line: usize, byte: usize| {
                text.split_inclusive('\n').take(line).map(str::len).sum::<usize>() + byte
            };
            let (start, end) = (at(s.start.line, s.start.byte), at(s.end.line, s.end.byte));
            text.replace_range(start..end, &s.text);
        }
        text
    }

    #[test]
    fn splices_replay_the_edits() {
        let before = "ação\nb\nc\n";
        let mut ed = Editor::new(before, Indent::spaces(4));
        let start = ed.revision();
        // Two cursors typing, then a line deleted, then undo.
        ed.select(1..1);
        ed.add_cursor_vertically(false);
        ed.type_char('x');
        ed.select(ed.line_start(2)..ed.line_start(2));
        ed.delete_lines();
        ed.undo();
        ed.type_char('😀');
        let splices = ed.splices_since(start).expect("in the journal");
        assert_eq!(apply(before, &splices), ed.text());

        // UTF-16 columns count the emoji as two units.
        let last = splices.last().unwrap();
        assert_eq!(last.text, "😀");
        let line = ed.line(last.start.line);
        let prefix = &line[..last.start.byte];
        assert_eq!(last.start.utf16, prefix.encode_utf16().count());

        assert_eq!(ed.splices_since(ed.revision()).map(|s| s.len()), Some(0));
        assert!(ed.splices_since(ed.revision() + 1).is_none());
    }

    #[test]
    fn old_revisions_fall_out() {
        let mut ed = Editor::new("", Indent::spaces(4));
        let start = ed.revision();
        for _ in 0..300 {
            ed.type_char('a');
            ed.seal();
        }
        assert!(ed.splices_since(start).is_none());
        let recent = ed.revision() - 10;
        assert_eq!(ed.splices_since(recent).map(|s| s.len()), Some(10));
    }
}
