//! Undo / redo as a list of small text changes.
//!
//! The editor reports the text before and after every edit; [`Change::between`]
//! reduces that to the one span that changed, so memory grows with what you
//! typed, not with file size. Consecutive keystrokes are grouped into words,
//! the way people expect "undo" to behave.

use std::time::{Duration, Instant};

/// Keystrokes further apart than this start a new undo step.
const GROUP_WINDOW: Duration = Duration::from_millis(1200);
const LIMIT: usize = 5_000;

/// "At byte `at`, `removed` was replaced by `inserted`."
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub at: usize,
    pub removed: String,
    pub inserted: String,
}

impl Change {
    /// The minimal change turning `old` into `new`.
    ///
    /// `cursor` (byte offset in `new`, after the edit) resolves ambiguity in
    /// repetitive text: typing `a` in the middle of `aa` is recorded where it
    /// actually happened, not at the end.
    pub fn between(old: &str, new: &str, cursor: Option<usize>) -> Option<Change> {
        if old == new {
            return None;
        }
        let (a, b) = (old.as_bytes(), new.as_bytes());
        let shortest = a.len().min(b.len());

        let suffix_cap = cursor.map_or(shortest, |c| shortest.min(b.len().saturating_sub(c)));
        let mut suffix =
            a.iter().rev().zip(b.iter().rev()).take(suffix_cap).take_while(|(x, y)| x == y).count();
        while suffix > 0 && !old.is_char_boundary(a.len() - suffix) {
            suffix -= 1;
        }

        let prefix_cap = shortest - suffix;
        let mut prefix = a.iter().zip(b).take(prefix_cap).take_while(|(x, y)| x == y).count();
        while prefix > 0 && !old.is_char_boundary(prefix) {
            prefix -= 1;
        }

        Some(Change {
            at: prefix,
            removed: old[prefix..a.len() - suffix].to_owned(),
            inserted: new[prefix..b.len() - suffix].to_owned(),
        })
    }

    pub fn inverse(&self) -> Change {
        Change { at: self.at, removed: self.inserted.clone(), inserted: self.removed.clone() }
    }

    pub fn apply(&self, text: &mut String) {
        text.replace_range(self.at..self.at + self.removed.len(), &self.inserted);
    }

    /// Byte offset just after the inserted text: where the cursor goes.
    pub fn end(&self) -> usize {
        self.at + self.inserted.len()
    }

    /// Folds `next` into `self` when both are part of one "word" of typing
    /// or one run of deleting.
    fn absorb(&mut self, next: &Change) -> bool {
        let typing = next.removed.is_empty()
            && next.at == self.at + self.inserted.len()
            && !self.inserted.is_empty()
            && next.inserted.chars().count() == 1
            && next.inserted != "\n";
        if typing {
            let boundary = next.inserted.chars().all(char::is_whitespace)
                && !self.inserted.ends_with(char::is_whitespace);
            if boundary {
                return false;
            }
            self.inserted.push_str(&next.inserted);
            return true;
        }

        let deleting = self.inserted.is_empty() && next.inserted.is_empty();
        if deleting && next.at + next.removed.len() == self.at {
            // Backspace: the deleted run grows to the left.
            self.at = next.at;
            self.removed.insert_str(0, &next.removed);
            return true;
        }
        if deleting && next.at == self.at {
            // Delete: the deleted run grows to the right.
            self.removed.push_str(&next.removed);
            return true;
        }
        false
    }
}

#[derive(Debug, Default, Clone)]
pub struct History {
    undo: Vec<Change>,
    redo: Vec<Change>,
    last_edit: Option<Instant>,
}

impl History {
    pub fn record(&mut self, change: Change, now: Instant) {
        self.redo.clear();
        let recent =
            self.last_edit.is_some_and(|t| now.saturating_duration_since(t) < GROUP_WINDOW);
        let merged = recent && self.undo.last_mut().is_some_and(|prev| prev.absorb(&change));
        if !merged {
            self.undo.push(change);
            if self.undo.len() > LIMIT {
                self.undo.drain(..self.undo.len() - LIMIT);
            }
        }
        self.last_edit = Some(now);
    }

    /// Ends the current group (e.g. after the cursor was moved by hand).
    pub fn seal(&mut self) {
        self.last_edit = None;
    }

    /// Returns the change to apply to the text to undo one step.
    pub fn undo(&mut self) -> Option<Change> {
        let change = self.undo.pop()?;
        let inverse = change.inverse();
        self.redo.push(change);
        self.last_edit = None;
        Some(inverse)
    }

    /// Returns the change to apply to the text to redo one step.
    pub fn redo(&mut self) -> Option<Change> {
        let change = self.redo.pop()?;
        self.undo.push(change.clone());
        self.last_edit = None;
        Some(change)
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn type_str(history: &mut History, text: &mut String, at: usize, s: &str, t0: Instant) {
        for (i, ch) in s.char_indices() {
            let old = text.clone();
            text.insert(at + i, ch);
            let cursor = at + i + ch.len_utf8();
            history.record(Change::between(&old, text, Some(cursor)).unwrap(), t0);
        }
    }

    #[test]
    fn minimal_diffs() {
        // Typed "brave " with the cursor ending at byte 12.
        let c = Change::between("hello world", "hello brave world", Some(12)).unwrap();
        assert_eq!((c.at, c.removed.as_str(), c.inserted.as_str()), (6, "", "brave "));
        // Without a hint any equivalent minimal change is fine.
        let c = Change::between("hello world", "hello brave world", None).unwrap();
        assert_eq!((c.removed.len(), c.inserted.len()), (0, 6));
        let c = Change::between("aa", "aaa", Some(2)).unwrap();
        assert_eq!(c.at, 1, "cursor hint places the insertion");
        let c = Change::between("olá", "olé", None).unwrap();
        assert_eq!((c.removed.as_str(), c.inserted.as_str()), ("á", "é"));
        assert!(Change::between("same", "same", None).is_none());
    }

    #[test]
    fn diff_then_apply_is_identity() {
        let pairs = [("", "x"), ("x", ""), ("abc", "aXc"), ("ação", "acao"), ("aaaa", "aa")];
        for (old, new) in pairs {
            for cursor in [None, Some(0), Some(new.len())] {
                let c = Change::between(old, new, cursor).unwrap();
                let mut s = old.to_owned();
                c.apply(&mut s);
                assert_eq!(s, new);
                c.inverse().apply(&mut s);
                assert_eq!(s, old);
            }
        }
    }

    #[test]
    fn groups_words() {
        let t0 = Instant::now();
        let mut h = History::default();
        let mut text = String::new();
        type_str(&mut h, &mut text, 0, "hello world", t0);
        assert_eq!(text, "hello world");

        h.undo().unwrap().apply(&mut text);
        assert_eq!(text, "hello");
        h.undo().unwrap().apply(&mut text);
        assert_eq!(text, "");
        assert!(!h.can_undo());

        h.redo().unwrap().apply(&mut text);
        h.redo().unwrap().apply(&mut text);
        assert_eq!(text, "hello world");
    }

    #[test]
    fn pause_splits_groups() {
        let t0 = Instant::now();
        let mut h = History::default();
        let mut text = String::new();
        type_str(&mut h, &mut text, 0, "ab", t0);
        type_str(&mut h, &mut text, 2, "cd", t0 + Duration::from_secs(5));
        h.undo().unwrap().apply(&mut text);
        assert_eq!(text, "ab");
    }

    #[test]
    fn backspace_run_is_one_step() {
        let t0 = Instant::now();
        let mut h = History::default();
        let mut text = String::from("abcdef");
        for _ in 0..3 {
            let old = text.clone();
            text.pop();
            h.record(Change::between(&old, &text, Some(text.len())).unwrap(), t0);
        }
        assert_eq!(text, "abc");
        h.undo().unwrap().apply(&mut text);
        assert_eq!(text, "abcdef");
    }

    #[test]
    fn new_edit_clears_redo() {
        let t0 = Instant::now();
        let mut h = History::default();
        let mut text = String::new();
        type_str(&mut h, &mut text, 0, "a", t0);
        h.undo().unwrap().apply(&mut text);
        type_str(&mut h, &mut text, 0, "b", t0);
        assert!(!h.can_redo());
    }
}
