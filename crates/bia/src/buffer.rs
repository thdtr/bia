//! An open text file: the editor widget's content plus BIA's own copy of the
//! text, undo history and the bookkeeping needed to save it faithfully.
//!
//! BIA's copy and the widget must always hold the same text, split into the
//! same lines. The widget treats a bare `\r` as a line break and BIA does
//! not, so no `\r` ever reaches either of them: files with one aren't opened
//! and pasted text is normalized.

use bia_core::document::{Document, LineEnding};
use bia_core::history::{Change, History};
use bia_core::text::{self, Pos};
use iced::widget::markdown;
use iced::widget::text_editor::{Action, Content, Cursor, Edit, Position};
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

impl Buffer {
    pub fn new(doc: Document, syntax: String) -> Self {
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
        *self = Self::new(doc, syntax);
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
            Action::Edit(Edit::Unindent) => {
                self.prepare_unindent();
                action
            }
            other => other,
        };
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

    /// cosmic-text (under iced) miscounts when Shift+Tab runs with the cursor,
    /// or the other end of the selection, inside a line's leading
    /// whitespace: the cursor ends up past the end of the line and the next
    /// keystroke crashes the editor. Moving those ends to the first
    /// non-blank character first avoids it.
    fn prepare_unindent(&mut self) {
        let indent = |line: usize| {
            self.content.line(line).map_or(0, |l| l.text.len() - l.text.trim_start().len())
        };
        let fix = |p: Position| Position { line: p.line, column: p.column.max(indent(p.line)) };
        let cursor = self.content.cursor();
        let fixed = Cursor { position: fix(cursor.position), selection: cursor.selection.map(fix) };
        if fixed != cursor {
            self.content.move_to(fixed);
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
        Buffer::new(Document::from_bytes(text.as_bytes()).unwrap(), "Plain Text".into())
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
