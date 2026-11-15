//! An open text file: the editor widget's content plus BIA's own copy of the
//! text, undo history and the bookkeeping needed to save it faithfully.

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
    pub bom: bool,
    /// Disk modification time we last saw, to notice outside changes.
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

    pub fn set_preview(&mut self, on: bool) {
        self.markdown = on.then(|| markdown::Content::parse(&self.text));
    }

    fn refresh_markdown(&mut self) {
        if self.markdown.is_some() {
            self.markdown = Some(markdown::Content::parse(&self.text));
        }
    }
}
