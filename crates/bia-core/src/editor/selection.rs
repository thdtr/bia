//! Cursors and selections. There is always at least one; with several,
//! every edit happens at each of them.

use super::transaction::{Assoc, Transaction};
use std::ops::Range;

/// A selection from `anchor` to `head` (byte offsets). The cursor is at
/// `head`; when they're equal, nothing is selected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selection {
    pub anchor: usize,
    pub head: usize,
    /// The column (in display cells) that Up and Down aim for, kept across
    /// short lines.
    pub goal: Option<usize>,
}

impl Selection {
    pub fn cursor(at: usize) -> Self {
        Self { anchor: at, head: at, goal: None }
    }

    pub fn new(anchor: usize, head: usize) -> Self {
        Self { anchor, head, goal: None }
    }

    pub fn is_empty(&self) -> bool {
        self.anchor == self.head
    }

    pub fn start(&self) -> usize {
        self.anchor.min(self.head)
    }

    pub fn end(&self) -> usize {
        self.anchor.max(self.head)
    }

    pub fn range(&self) -> Range<usize> {
        self.start()..self.end()
    }

    /// Moves the head, keeping the anchor (`extend`) or bringing it along.
    pub fn moved(self, head: usize, extend: bool) -> Self {
        let anchor = if extend { self.anchor } else { head };
        Self { anchor, head, goal: None }
    }

    fn overlaps(&self, other: &Self) -> bool {
        // Touching counts only for two cursors at the same spot; selections
        // that meet end to end stay separate, as in other editors.
        if self.is_empty() || other.is_empty() {
            self.start() <= other.end() && other.start() <= self.end() && {
                let point = |s: &Self| s.is_empty().then_some(s.head);
                match (point(self), point(other)) {
                    (Some(a), Some(b)) => a == b,
                    (Some(p), None) => p > other.start() && p < other.end(),
                    (None, Some(p)) => p > self.start() && p < self.end(),
                    (None, None) => unreachable!(),
                }
            }
        } else {
            self.start() < other.end() && other.start() < self.end()
        }
    }

    fn merge(self, other: Self) -> Self {
        let (start, end) = (self.start().min(other.start()), self.end().max(other.end()));
        if self.head >= self.anchor { Self::new(start, end) } else { Self::new(end, start) }
    }
}

/// The selections of an editor, sorted and never overlapping. One of them
/// is the primary: the one the view follows and new cursors start from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selections {
    list: Vec<Selection>,
    primary: usize,
}

impl Selections {
    pub fn single(selection: Selection) -> Self {
        Self { list: vec![selection], primary: 0 }
    }

    /// Sorts and merges `list`. `primary` indexes into `list` as given.
    pub fn new(list: Vec<Selection>, primary: usize) -> Self {
        assert!(!list.is_empty(), "there is always a selection");
        let primary_sel = list[primary.min(list.len() - 1)];
        let mut order: Vec<(Selection, bool)> =
            list.into_iter().enumerate().map(|(i, s)| (s, i == primary)).collect();
        order.sort_by_key(|(s, _)| (s.start(), s.end()));
        let mut merged: Vec<(Selection, bool)> = Vec::with_capacity(order.len());
        for (s, is_primary) in order {
            match merged.last_mut() {
                Some((last, p)) if last.overlaps(&s) => {
                    *last = last.merge(s);
                    *p |= is_primary;
                }
                _ => merged.push((s, is_primary)),
            }
        }
        let primary = merged.iter().position(|(_, p)| *p).unwrap_or_else(|| {
            merged.iter().position(|(s, _)| s.range() == primary_sel.range()).unwrap_or(0)
        });
        Self { list: merged.into_iter().map(|(s, _)| s).collect(), primary }
    }

    pub fn iter(&self) -> impl Iterator<Item = &Selection> {
        self.list.iter()
    }

    pub fn as_slice(&self) -> &[Selection] {
        &self.list
    }

    pub fn len(&self) -> usize {
        self.list.len()
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    pub fn primary(&self) -> Selection {
        self.list[self.primary]
    }

    pub fn primary_index(&self) -> usize {
        self.primary
    }

    /// Applies `f` to every selection, then sorts and merges.
    pub fn map(&self, f: impl FnMut(Selection) -> Selection) -> Self {
        Self::new(self.list.iter().copied().map(f).collect(), self.primary)
    }

    /// Where the selections end up after `tx`.
    pub fn after(&self, tx: &Transaction) -> Self {
        self.map(|s| Selection::new(tx.map(s.anchor, Assoc::After), tx.map(s.head, Assoc::After)))
    }

    /// Keeps only the primary selection.
    pub fn only_primary(&self) -> Self {
        Self::single(self.primary())
    }

    /// Adds `selection` and makes it the primary.
    pub fn with(&self, selection: Selection) -> Self {
        let mut list = self.list.clone();
        list.push(selection);
        let primary = list.len() - 1;
        Self::new(list, primary)
    }

    /// Clamps every offset to `len`.
    pub fn clamped(&self, len: usize) -> Self {
        self.map(|s| Selection { anchor: s.anchor.min(len), head: s.head.min(len), goal: s.goal })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sorted_merged_primary_kept() {
        let s = Selections::new(
            vec![
                Selection::new(10, 12),
                Selection::cursor(3),
                Selection::new(11, 15),
                Selection::cursor(3),
            ],
            2,
        );
        assert_eq!(s.as_slice(), [Selection::cursor(3), Selection::new(10, 15)]);
        assert_eq!(s.primary(), Selection::new(10, 15));
    }

    #[test]
    fn touching_selections_stay_apart() {
        let s = Selections::new(vec![Selection::new(0, 3), Selection::new(3, 6)], 0);
        assert_eq!(s.len(), 2);
        // A cursor at the edge of a selection stays too; inside, it merges.
        let s = Selections::new(vec![Selection::new(0, 3), Selection::cursor(3)], 0);
        assert_eq!(s.len(), 2);
        let s = Selections::new(vec![Selection::new(0, 3), Selection::cursor(2)], 1);
        assert_eq!(s.as_slice(), [Selection::new(0, 3)]);
        assert_eq!(s.primary_index(), 0);
    }

    #[test]
    fn backwards_selection_keeps_direction_when_merged() {
        let s = Selections::new(vec![Selection::new(5, 2), Selection::new(4, 1)], 0);
        assert_eq!(s.as_slice(), [Selection::new(5, 1)]);
    }
}
