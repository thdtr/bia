//! An open text file: the editor widget's content plus BIA's own copy of the
//! text, undo history and the bookkeeping needed to save it faithfully.
//!
//! BIA's copy and the widget must always hold the same text, split into the
//! same lines. The widget treats a bare `\r` as a line break and BIA does
//! not, so no `\r` ever reaches either of them: files with one aren't opened
//! and pasted text is normalized.

use bia_core::document::{Document, LineEnding};
use bia_core::history::{Change, History};
use bia_core::indent::Indent;
use bia_core::text::{self, Pos};
use iced::widget::markdown;
use iced::widget::text_editor::{Action, Content, Cursor, Edit, Motion, Position};
use std::ops::Range;
use std::sync::Arc;
use std::time::{Instant, SystemTime};

pub struct Buffer {
    pub content: Content,
    /// Mirror of `content`, normalized to `\n`. Diffed after every edit.
    text: String,
    /// Text as of the last load/save, for the "unsaved" dot.
    saved: String,
    history: History,
    pub line_ending: LineEnding,
    /// The file mixed LF and CRLF when it was opened.
    pub mixed_endings: bool,
    pub bom: bool,
    /// Disk modification time as of the last load or save, to notice
    /// outside changes.
    pub modified: Option<SystemTime>,
    /// Grammar name used for highlighting (also shown in the status bar).
    pub syntax: String,
    /// What Tab, Shift+Tab, Enter and Backspace indent with.
    pub indent: Indent,
    /// Parsed Markdown, kept only while the preview is open.
    pub markdown: Option<markdown::Content>,
    /// `"1\n2\n3…"` for the line-number gutter, rebuilt when the count changes.
    pub gutter: String,
    pub line_count: usize,
    /// Widest line in monospace cells, to size the horizontal scroll area.
    pub max_columns: usize,
}

fn to_widget(p: Pos) -> Position {
    Position { line: p.line, column: p.column }
}

fn from_widget(p: Position) -> Pos {
    Pos { line: p.line, column: p.column }
}

impl Buffer {
    pub fn new(doc: Document, syntax: String, indent: Indent) -> Self {
        let mut buffer = Self {
            content: Content::with_text(&doc.text),
            saved: doc.text.clone(),
            text: doc.text,
            history: History::default(),
            line_ending: doc.line_ending,
            mixed_endings: doc.mixed_endings,
            bom: doc.bom,
            modified: doc.modified,
            syntax,
            indent,
            markdown: None,
            gutter: String::new(),
            line_count: 0,
            max_columns: 0,
        };
        buffer.refresh_metrics();
        buffer
    }

    fn refresh_metrics(&mut self) {
        let lines = self.text.matches('\n').count() + 1;
        if lines != self.line_count {
            self.line_count = lines;
            self.gutter = (1..=lines).map(|n| n.to_string()).collect::<Vec<_>>().join("\n");
        }
        self.max_columns = text::max_columns(&self.text);
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn is_dirty(&self) -> bool {
        self.text != self.saved
    }

    pub fn encoded(&self) -> Vec<u8> {
        Document::encode(&self.text, self.line_ending, self.bom)
    }

    pub fn mark_saved(&mut self, modified: Option<SystemTime>) {
        self.saved.clone_from(&self.text);
        self.modified = modified;
    }

    /// Replaces everything with a fresh copy from disk (outside change).
    pub fn reload(&mut self, doc: Document) {
        let syntax = std::mem::take(&mut self.syntax);
        let preview = self.markdown.is_some();
        *self = Self::new(doc, syntax, self.indent);
        if preview {
            self.set_preview(true);
        }
    }

    pub fn perform(&mut self, action: Action) {
        let action = match action {
            Action::Edit(Edit::Paste(text)) if text.contains('\r') => {
                Action::Edit(Edit::Paste(Arc::new(text.replace("\r\n", "\n").replace('\r', "\n"))))
            }
            Action::Edit(Edit::Insert('\r')) => Action::Edit(Edit::Enter),
            other => other,
        };
        // iced's own versions of these always use 8 spaces and never carry
        // indentation to a new line, so BIA does them itself.
        match action {
            Action::Edit(Edit::Enter) => return self.newline(),
            Action::Edit(Edit::Indent) => return self.indent(),
            Action::Edit(Edit::Unindent) => return self.outdent(),
            Action::Edit(Edit::Backspace) if self.backspace_indent() => return,
            _ => {}
        }
        let is_edit = action.is_edit();
        let is_pointer = matches!(action, Action::Click(_) | Action::Drag(_));
        self.content.perform(action);
        if is_edit {
            self.sync();
        } else if is_pointer {
            // Clicking elsewhere ends the current undo group.
            self.history.seal();
        }
    }

    fn sync(&mut self) {
        let new = self.content.text();
        let cursor = text::pos_to_offset(&new, self.cursor_pos());
        if let Some(change) = Change::between(&self.text, &new, Some(cursor)) {
            self.history.record(change, Instant::now());
            self.text = new;
            self.refresh_metrics();
            self.refresh_markdown();
        }
    }

    pub fn undo(&mut self) -> bool {
        self.history.undo().map(|c| self.apply(&c)).is_some()
    }

    pub fn redo(&mut self) -> bool {
        self.history.redo().map(|c| self.apply(&c)).is_some()
    }

    /// Applies `change` to the widget by selecting the old span and pasting
    /// the new text over it. This keeps scroll position and highlighting.
    fn apply(&mut self, change: &Change) {
        let start = text::offset_to_pos(&self.text, change.at);
        let end = text::offset_to_pos(&self.text, change.at + change.removed.len());
        self.content
            .move_to(Cursor { position: to_widget(end), selection: Some(to_widget(start)) });
        if start != end || !change.inserted.is_empty() {
            self.content.perform(Action::Edit(Edit::Paste(Arc::new(change.inserted.clone()))));
        }
        change.apply(&mut self.text);
        // The widget is the source of truth for what is on screen. Should the
        // two ever disagree, adopt the widget's text rather than drift.
        let shown = self.content.text();
        if shown != self.text {
            self.text = shown;
        }
        self.refresh_metrics();
        self.refresh_markdown();
    }

    pub fn cursor_pos(&self) -> Pos {
        let p = self.content.cursor().position;
        Pos { line: p.line, column: p.column }
    }

    /// 1-based line and character column, for the status bar.
    pub fn line_col(&self) -> (usize, usize) {
        let p = self.cursor_pos();
        let line = self.content.line(p.line).map(|l| l.text.into_owned()).unwrap_or_default();
        (p.line + 1, text::char_column(&line, p.column))
    }

    /// Cursor column in monospace cells, for keeping it scrolled into view.
    pub fn cursor_columns(&self) -> usize {
        let p = self.cursor_pos();
        let line = self.content.line(p.line).map(|l| l.text.into_owned()).unwrap_or_default();
        let end = p.column.min(line.len());
        text::display_columns(line.get(..end).unwrap_or(&line))
    }

    pub fn cursor_offset(&self) -> usize {
        text::pos_to_offset(&self.text, self.cursor_pos())
    }

    pub fn select(&mut self, range: Range<usize>) {
        let start = text::offset_to_pos(&self.text, range.start);
        let end = text::offset_to_pos(&self.text, range.end);
        self.content
            .move_to(Cursor { position: to_widget(end), selection: Some(to_widget(start)) });
        self.history.seal();
    }

    /// The selected byte range, if anything is selected.
    pub fn selection_range(&self) -> Option<Range<usize>> {
        let (start, end) = self.selection()?;
        Some(text::pos_to_offset(&self.text, start)..text::pos_to_offset(&self.text, end))
    }

    /// Moves the cursor to `line` (0-based) and character `column` (0-based),
    /// both clamped to the text.
    pub fn go_to(&mut self, line: usize, column: usize) {
        let line = line.min(self.line_count.saturating_sub(1));
        let text = self.line_text(line);
        let column = text.char_indices().nth(column).map_or(text.len(), |(i, _)| i);
        self.place(Pos { line, column }, None);
        self.history.seal();
    }

    /// Replaces `range` with `with` as one undo step and leaves the
    /// cursor after it.
    pub fn replace_range(&mut self, range: Range<usize>, with: &str) {
        self.history.seal();
        self.replace(range, with);
        self.history.seal();
    }

    /// Replaces every range in `ranges` (sorted, not overlapping) with
    /// `with`, as one undo step. The cursor stays where it was in the text.
    pub fn replace_all(&mut self, ranges: &[Range<usize>], with: &str) {
        let (Some(first), Some(last)) = (ranges.first(), ranges.last()) else { return };
        let cursor = self.cursor_offset();
        let mut middle = String::new();
        let mut from = first.start;
        // How far the cursor moves: by every replacement before it, and to
        // the end of the replacement it was inside of.
        let mut new_cursor = cursor as isize;
        for range in ranges {
            middle.push_str(&self.text[from..range.start]);
            middle.push_str(with);
            from = range.end;
            let growth = with.len() as isize - range.len() as isize;
            if range.end <= cursor {
                new_cursor += growth;
            } else if range.start < cursor {
                new_cursor += range.end as isize - cursor as isize + growth;
            }
        }
        self.replace_range(first.start..last.end, &middle);
        let pos = text::offset_to_pos(&self.text, new_cursor.max(0) as usize);
        self.place(pos, None);
    }

    // ── Editing helpers ──────────────────────────────────────────────────

    /// Start and end of the selection, in order. `None` when nothing is
    /// selected (an empty selection counts as nothing).
    fn selection(&self) -> Option<(Pos, Pos)> {
        let cursor = self.content.cursor();
        let a = from_widget(cursor.position);
        let b = from_widget(cursor.selection?);
        (a != b).then(|| (a.min(b), a.max(b)))
    }

    fn line_text(&self, line: usize) -> String {
        self.content.line(line).map(|l| l.text.into_owned()).unwrap_or_default()
    }

    fn offset(&self, pos: Pos) -> usize {
        text::pos_to_offset(&self.text, pos)
    }

    /// Replaces a byte range of the text through the widget, so scrolling
    /// and highlighting carry on, and records it for undo.
    fn replace(&mut self, range: Range<usize>, with: &str) {
        let start = text::offset_to_pos(&self.text, range.start);
        let end = text::offset_to_pos(&self.text, range.end);
        self.content
            .move_to(Cursor { position: to_widget(end), selection: Some(to_widget(start)) });
        self.content.perform(Action::Edit(Edit::Paste(Arc::new(with.to_owned()))));
        self.sync();
    }

    /// Puts the cursor at `position`, selecting from `anchor` if given.
    fn place(&mut self, position: Pos, anchor: Option<Pos>) {
        let position = to_widget(position);
        match anchor.map(to_widget).filter(|a| *a != position) {
            Some(anchor) => self.content.move_to(Cursor { position, selection: Some(anchor) }),
            None => {
                // `move_to` keeps an existing selection, and an empty one
                // makes the next Backspace do nothing. A motion clears it.
                self.content.move_to(Cursor { position, selection: Some(position) });
                self.content.perform(Action::Move(Motion::Right));
            }
        }
    }

    /// Enter: a new line with the current indentation (see
    /// [`Indent::newline`]).
    fn newline(&mut self) {
        let cursor = self.cursor_pos();
        let (start, end) = self.selection().unwrap_or((cursor, cursor));
        let mut line = self.line_text(start.line);
        line.truncate(start.column);
        line.push_str(self.line_text(end.line).get(end.column..).unwrap_or_default());
        let (insert, caret) = self.indent.newline(&line, start.column);
        let at = self.offset(start);
        self.replace(at..self.offset(end), &insert);
        if caret != insert.len() {
            let pos = text::offset_to_pos(&self.text, at + caret);
            self.place(pos, None);
        }
    }

    /// Tab: one level at the cursor, or on every selected line.
    fn indent(&mut self) {
        self.history.seal();
        match self.selection() {
            Some((start, end)) => self.shift_lines(start, end, true),
            None => {
                let cursor = self.cursor_pos();
                let line = self.line_text(cursor.line);
                let insert = self.indent.insert_at(line.get(..cursor.column).unwrap_or_default());
                let at = self.offset(cursor);
                self.replace(at..at, &insert);
            }
        }
        self.history.seal();
    }

    /// Shift+Tab: one level less on the cursor's line or the selected lines.
    fn outdent(&mut self) {
        let cursor = self.cursor_pos();
        let (start, end) = self.selection().unwrap_or((cursor, cursor));
        self.history.seal();
        self.shift_lines(start, end, false);
        self.history.seal();
    }

    /// Adds or removes one level of indentation on the lines from `start`
    /// to `end`, keeping the cursor and selection on the same text.
    fn shift_lines(&mut self, start: Pos, end: Pos, add: bool) {
        // A selection that ends at the start of a line doesn't include it.
        let last = if end.column == 0 && end.line > start.line { end.line - 1 } else { end.line };
        let unit = self.indent.unit();
        let mut block = String::new();
        let mut deltas = Vec::new();
        for (i, n) in (start.line..=last).enumerate() {
            let line = self.line_text(n);
            if i > 0 {
                block.push('\n');
            }
            let delta = if !add {
                let remove = self.indent.outdent(&line);
                block.push_str(&line[remove..]);
                -(remove as isize)
            } else if line.is_empty() {
                0
            } else {
                block.push_str(&unit);
                block.push_str(&line);
                unit.len() as isize
            };
            deltas.push(delta);
        }
        if deltas.iter().all(|&d| d == 0) {
            return;
        }
        let cursor = self.content.cursor();
        let range_start = self.offset(Pos { line: start.line, column: 0 });
        let last_len = self.line_text(last).len();
        let range_end = self.offset(Pos { line: last, column: last_len });
        self.replace(range_start..range_end, &block);

        let shift = |p: Position| {
            let p = from_widget(p);
            let Some(&delta) = p.line.checked_sub(start.line).and_then(|i| deltas.get(i)) else {
                return p;
            };
            let column = match delta {
                d if d < 0 => p.column.saturating_sub(d.unsigned_abs()),
                // Something selected from the start of a line still is.
                _ if p.column == 0 => 0,
                d => p.column + d as usize,
            };
            Pos { line: p.line, column }
        };
        self.place(shift(cursor.position), cursor.selection.map(shift));
    }

    /// Backspace in leading spaces deletes back to the previous tab stop.
    /// Returns false when the usual Backspace applies.
    fn backspace_indent(&mut self) -> bool {
        if self.selection().is_some() {
            return false;
        }
        let cursor = self.cursor_pos();
        let line = self.line_text(cursor.line);
        let Some(n) = self.indent.backspace(line.get(..cursor.column).unwrap_or_default()) else {
            return false;
        };
        let at = self.offset(cursor);
        self.replace(at - n..at, "");
        true
    }

    pub fn set_preview(&mut self, on: bool) {
        self.markdown = on.then(|| markdown::Content::parse(&self.text));
    }

    fn refresh_markdown(&mut self) {
        if self.markdown.is_some() {
            self.markdown = Some(markdown::Content::parse(&self.text));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::widget::text_editor::Motion;

    fn buffer(text: &str) -> Buffer {
        Buffer::new(
            Document::from_bytes(text.as_bytes()).unwrap(),
            "Plain Text".into(),
            Indent::spaces(4),
        )
    }

    /// BIA's copy of the text matches what the widget shows.
    fn assert_in_sync(b: &Buffer, context: &dyn std::fmt::Debug) {
        assert_eq!(b.text(), b.content.text(), "text differs after {context:?}");
        assert_eq!(b.line_count, b.content.line_count(), "line count differs after {context:?}");
        assert!(!b.text().contains('\r'), "CR got in after {context:?}");
    }

    #[test]
    fn shift_tab_inside_the_indentation() {
        let mut b = buffer("    x = 1\n      y\n");
        b.perform(Action::Move(Motion::Right));
        b.perform(Action::Move(Motion::Right));
        b.perform(Action::Edit(Edit::Unindent));
        b.perform(Action::Edit(Edit::Insert('a')));
        assert_eq!(b.text(), "ax = 1\n      y\n");
        assert_in_sync(&b, &"unindent");

        // A selection that ends inside the next line's indentation.
        b.perform(Action::Select(Motion::Down));
        b.perform(Action::Edit(Edit::Unindent));
        b.perform(Action::Edit(Edit::Insert('b')));
        assert_in_sync(&b, &"unindent with selection");
    }

    fn type_text(b: &mut Buffer, text: &str) {
        for c in text.chars() {
            b.perform(Action::Edit(Edit::Insert(c)));
        }
    }

    #[test]
    fn enter_keeps_indentation() {
        let mut b = buffer("    foo");
        b.perform(Action::Move(Motion::End));
        b.perform(Action::Edit(Edit::Enter));
        type_text(&mut b, "x");
        assert_eq!(b.text(), "    foo\n    x");
        assert_in_sync(&b, &"enter");
    }

    #[test]
    fn enter_between_brackets() {
        let mut b = buffer("fn f() {}");
        b.perform(Action::Move(Motion::End));
        b.perform(Action::Move(Motion::Left));
        b.perform(Action::Edit(Edit::Enter));
        type_text(&mut b, "x");
        assert_eq!(b.text(), "fn f() {\n    x\n}");
        assert_in_sync(&b, &"enter between brackets");
        while b.undo() {}
        assert_eq!(b.text(), "fn f() {}");
    }

    #[test]
    fn tab_and_backspace_use_the_indent_width() {
        let mut b = buffer("ab");
        b.perform(Action::Edit(Edit::Indent));
        assert_eq!(b.text(), "    ab");
        b.perform(Action::Move(Motion::Right));
        b.perform(Action::Edit(Edit::Indent));
        assert_eq!(b.text(), "    a   b");
        b.perform(Action::Move(Motion::Home));
        b.perform(Action::Move(Motion::Right));
        b.perform(Action::Move(Motion::Right));
        b.perform(Action::Move(Motion::Right));
        b.perform(Action::Move(Motion::Right));
        b.perform(Action::Edit(Edit::Backspace));
        assert_eq!(b.text(), "a   b");
        assert_in_sync(&b, &"backspace");

        let mut tabs = buffer("x");
        tabs.indent = Indent::tabs(4);
        tabs.perform(Action::Edit(Edit::Indent));
        assert_eq!(tabs.text(), "\tx");
    }

    #[test]
    fn tab_on_a_selection_shifts_whole_lines() {
        let mut b = buffer("a\nb\n\nc");
        b.perform(Action::SelectAll);
        b.perform(Action::Edit(Edit::Indent));
        assert_eq!(b.text(), "    a\n    b\n\n    c");
        assert_in_sync(&b, &"indent selection");
        b.perform(Action::Edit(Edit::Unindent));
        assert_eq!(b.text(), "a\nb\n\nc");
        // Still selected: typing replaces everything.
        type_text(&mut b, "z");
        assert_eq!(b.text(), "z");
        assert_in_sync(&b, &"unindent selection");
    }

    #[test]
    fn replace_all_keeps_the_cursor() {
        let mut b = buffer("foo bar foo baz");
        b.perform(Action::Move(Motion::DocumentEnd));
        let ranges = text::find_all(b.text(), "foo");
        b.replace_all(&ranges, "x");
        assert_eq!(b.text(), "x bar x baz");
        assert_eq!(b.cursor_offset(), b.text().len());
        assert_in_sync(&b, &"replace all");
        b.undo();
        assert_eq!(b.text(), "foo bar foo baz");
    }

    #[test]
    fn go_to_moves_without_selecting() {
        let mut b = buffer("a\nbcd\ne");
        b.perform(Action::SelectAll);
        b.go_to(1, 2);
        assert_eq!(b.cursor_pos(), Pos { line: 1, column: 2 });
        b.perform(Action::Edit(Edit::Backspace));
        assert_eq!(b.text(), "a\nbd\ne");
        b.go_to(99, 99);
        assert_eq!(b.cursor_pos(), Pos { line: 2, column: 1 });
    }

    #[test]
    fn pasted_line_breaks_are_normalized() {
        let mut b = buffer("");
        b.perform(Action::Edit(Edit::Paste(Arc::new("a\r\nb\rc".to_owned()))));
        assert_eq!(b.text(), "a\nb\nc");
        assert_in_sync(&b, &"paste");
        b.undo();
        assert_eq!(b.text(), "");
    }

    struct Rng(u64);

    impl Rng {
        fn below(&mut self, n: usize) -> usize {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            (self.0 >> 33) as usize % n
        }
    }

    /// Random typing, pasting, indenting, moving and undoing. The text and
    /// the widget never drift apart, and undoing everything gets back the
    /// original.
    #[test]
    fn random_edits_stay_in_sync_and_undo_fully() {
        let original = "fn main() {\n    olá();\n\tlet x = \"日本\";\n}\n\nfim";
        let motions = [
            Motion::Left,
            Motion::Right,
            Motion::Up,
            Motion::Down,
            Motion::Home,
            Motion::End,
            Motion::WordLeft,
            Motion::WordRight,
            Motion::DocumentStart,
            Motion::DocumentEnd,
        ];
        let chars = ['a', ' ', 'é', '日', '(', '\t'];
        let pastes = ["foo\nbar", "ç", "", "    indented\n\n", "win\r\ndows", "old\rmac"];

        for seed in 0..150 {
            let mut rng = Rng(seed * 7919 + 1);
            let mut b = buffer(original);
            let mut log = Vec::new();
            for _ in 0..60 {
                let action = match rng.below(100) {
                    0..=24 => Action::Edit(Edit::Insert(chars[rng.below(chars.len())])),
                    25..=31 => Action::Edit(Edit::Enter),
                    32..=41 => Action::Edit(Edit::Backspace),
                    42..=47 => Action::Edit(Edit::Delete),
                    48..=53 => Action::Edit(Edit::Paste(Arc::new(
                        pastes[rng.below(pastes.len())].to_owned(),
                    ))),
                    54..=57 => Action::Edit(Edit::Indent),
                    58..=63 => Action::Edit(Edit::Unindent),
                    64..=77 => Action::Move(motions[rng.below(motions.len())]),
                    78..=87 => Action::Select(motions[rng.below(motions.len())]),
                    88..=89 => Action::SelectAll,
                    90..=95 => {
                        log.push("undo".to_owned());
                        b.undo();
                        continue;
                    }
                    _ => {
                        log.push("redo".to_owned());
                        b.redo();
                        continue;
                    }
                };
                log.push(format!("{action:?}"));
                b.perform(action);
                assert_in_sync(&b, &(seed, &log));
            }

            while b.redo() {}
            let edited = b.text().to_owned();
            while b.undo() {}
            assert_eq!(b.text(), original, "seed {seed}: undo all, after {log:?}");
            assert!(!b.is_dirty());
            while b.redo() {}
            assert_eq!(b.text(), edited, "seed {seed}: redo all, after {log:?}");
        }
    }
}
