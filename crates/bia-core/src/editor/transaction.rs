//! A set of edits made at once (one per cursor), and how offsets move
//! through them.

use ropey::Rope;
use std::ops::Range;

/// Replace the bytes in `range` (of the text before the transaction) with
/// `text`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edit {
    pub range: Range<usize>,
    pub text: String,
}

impl Edit {
    pub fn new(range: Range<usize>, text: impl Into<String>) -> Self {
        Self { range, text: text.into() }
    }

    fn growth(&self) -> isize {
        self.text.len() as isize - self.range.len() as isize
    }
}

/// Which side of text inserted exactly at an offset the offset sticks to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Assoc {
    Before,
    After,
}

/// Edits sorted by position, none overlapping.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Transaction {
    edits: Vec<Edit>,
}

impl Transaction {
    /// Sorts `edits` and drops the ones that do nothing. An edit that
    /// overlaps an earlier one is dropped too: two cursors can't change the
    /// same text.
    pub fn new(mut edits: Vec<Edit>) -> Self {
        edits.retain(|e| !(e.range.is_empty() && e.text.is_empty()));
        edits.sort_by_key(|e| (e.range.start, e.range.end));
        let mut kept: Vec<Edit> = Vec::with_capacity(edits.len());
        for e in edits {
            if let Some(last) = kept.last() {
                let overlaps = e.range.start < last.range.end
                    || (e.range.start == last.range.start && e.range.start == last.range.end);
                if overlaps {
                    continue;
                }
            }
            kept.push(e);
        }
        Self { edits: kept }
    }

    pub fn is_empty(&self) -> bool {
        self.edits.is_empty()
    }

    pub fn edits(&self) -> &[Edit] {
        &self.edits
    }

    pub fn apply(&self, rope: &mut Rope) {
        for e in self.edits.iter().rev() {
            let start = rope.byte_to_char(e.range.start);
            let end = rope.byte_to_char(e.range.end);
            rope.remove(start..end);
            rope.insert(start, &e.text);
        }
    }

    /// The transaction that takes the text back, given the text as it was
    /// before this one was applied.
    pub fn invert(&self, before: &Rope) -> Self {
        let mut delta = 0isize;
        let edits = self
            .edits
            .iter()
            .map(|e| {
                let start = e.range.start.saturating_add_signed(delta);
                delta += e.growth();
                let removed = before.byte_slice(e.range.clone()).to_string();
                Edit::new(start..start + e.text.len(), removed)
            })
            .collect();
        Self { edits }
    }

    /// Where byte `pos` of the text before ends up after. An offset inside
    /// replaced text goes to the start (`Before`) or end (`After`) of the
    /// new text.
    pub fn map(&self, pos: usize, assoc: Assoc) -> usize {
        let mut delta = 0isize;
        for e in &self.edits {
            if pos < e.range.start {
                break;
            }
            if pos > e.range.start && pos >= e.range.end {
                delta += e.growth();
                continue;
            }
            let start = e.range.start.saturating_add_signed(delta);
            return match assoc {
                Assoc::Before => start,
                Assoc::After => start + e.text.len(),
            };
        }
        pos.saturating_add_signed(delta)
    }

    /// Where each edit starts once all of them are applied.
    pub fn new_starts(&self) -> Vec<usize> {
        let mut delta = 0isize;
        self.edits
            .iter()
            .map(|e| {
                let start = e.range.start.saturating_add_signed(delta);
                delta += e.growth();
                start
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_invert_map() {
        let mut rope = Rope::from_str("hello world");
        let tx = Transaction::new(vec![Edit::new(6..11, "olá"), Edit::new(0..0, ">> ")]);
        let inverse = tx.invert(&rope);
        tx.apply(&mut rope);
        assert_eq!(rope.to_string(), ">> hello olá");
        assert_eq!(tx.map(0, Assoc::Before), 0);
        assert_eq!(tx.map(0, Assoc::After), 3);
        assert_eq!(tx.map(5, Assoc::After), 8);
        assert_eq!(tx.map(8, Assoc::Before), 9);
        assert_eq!(tx.map(11, Assoc::After), 13);
        assert_eq!(tx.new_starts(), [0, 9]);
        inverse.apply(&mut rope);
        assert_eq!(rope.to_string(), "hello world");
    }

    #[test]
    fn overlapping_and_empty_edits_are_dropped() {
        let tx = Transaction::new(vec![
            Edit::new(2..5, "x"),
            Edit::new(3..4, "y"),
            Edit::new(7..7, ""),
            Edit::new(5..5, "z"),
        ]);
        assert_eq!(tx.edits(), [Edit::new(2..5, "x"), Edit::new(5..5, "z")]);
        // Two insertions at one offset: only the first.
        let tx = Transaction::new(vec![Edit::new(1..1, "a"), Edit::new(1..1, "b")]);
        assert_eq!(tx.edits().len(), 1);
    }
}
