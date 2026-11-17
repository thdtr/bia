//! An open text file: the editor widget's content plus BIA's own copy of the
//! text, undo history and the bookkeeping needed to save it faithfully.
//!
//! BIA's copy and the widget must always hold the same text, split into the
//! same lines. The widget treats a bare `\r` as a line break and BIA does
//! not, so no `\r` ever reaches either of them: files with one aren't opened
//! and pasted text is normalized.

use bia_core::document::{Document, LineEnding};
use bia_core::editing::{self, LineEdit};
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
    /// Typing an opening bracket or quote also types the closing one.
    pub auto_close: bool,
    /// Parsed Markdown, kept only while the preview is open.
    pub markdown: Option<markdown::Content>,
    /// `"1\n2\n3…"` for the line-number gutter, rebuilt when the count changes.
    pub gutter: String,
    pub line_count: usize,
    /// Widest line in monospace cells, to size the horizontal scroll area.
    pub max_columns: usize,
    /// Goes up with every change to the text, so callers can tell whether
    /// it changed since they last looked.
    pub revision: u64,
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
            auto_close: true,
            markdown: None,
            gutter: String::new(),
            line_count: 0,
            max_columns: 0,
            revision: 0,
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
        let auto_close = self.auto_close;
        *self = Self::new(doc, syntax, self.indent);
        self.auto_close = auto_close;
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
            Action::Edit(Edit::Insert(c)) if self.auto_close && self.type_pair(c) => return,
            Action::Edit(Edit::Backspace) if self.auto_close && self.delete_pair() => return,
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
            self.revision += 1;
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
        self.revision += 1;
        // The widget is the source of truth for what is on screen. Should the
        // two ever disagree, adopt the widget's text rather than drift.
        let shown = self.content.text();
        if shown != self.text {
            self.text = shown;
        }
        self.refresh_metrics();
        self.refresh_markdown();
    }

    /// Puts back edits recovered from a backup, as one undo step on top of
    /// the file as it is on disk.
    pub fn recover(&mut self, text: &str) {
        self.history.seal();
        self.replace(0..self.text.len(), text);
        self.history.seal();
        self.place(Pos::default(), None);
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

    /// Replaces every range in `edits` (sorted, not overlapping) with its
    /// text, as one undo step. The cursor stays where it was in the text.
    pub fn replace_all(&mut self, edits: &[(Range<usize>, String)]) {
        let (Some((first, _)), Some((last, _))) = (edits.first(), edits.last()) else { return };
        let cursor = self.cursor_offset();
        let mut middle = String::new();
        let mut from = first.start;
        // How far the cursor moves: by every replacement before it, and to
        // the end of the replacement it was inside of.
        let mut new_cursor = cursor as isize;
        for (range, with) in edits {
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
        let edits: Vec<LineEdit> = (start.line..=last)
            .enumerate()
            .filter_map(|(i, n)| {
                let line = self.line_text(n);
                let (remove, insert) = if add {
                    (0, if line.is_empty() { return None } else { unit.clone() })
                } else {
                    (self.indent.outdent(&line), String::new())
                };
                (remove > 0 || !insert.is_empty()).then_some(LineEdit {
                    line: i,
                    at: 0,
                    remove,
                    insert,
                })
            })
            .collect();
        self.edit_lines(start.line, last, &edits);
    }

    /// Lines the selection covers, or the cursor's line. A selection that
    /// ends at the start of a line doesn't include that line.
    fn selected_lines(&self) -> (usize, usize) {
        let cursor = self.cursor_pos();
        let (start, end) = self.selection().unwrap_or((cursor, cursor));
        let last = if end.column == 0 && end.line > start.line { end.line - 1 } else { end.line };
        (start.line, last)
    }

    /// Applies `edits` (indexed from `first`) to lines `first..=last` as one
    /// change, keeping the cursor and selection on the same text.
    fn edit_lines(&mut self, first: usize, last: usize, edits: &[LineEdit]) {
        if edits.is_empty() {
            return;
        }
        let lines: Vec<String> = (first..=last).map(|n| self.line_text(n)).collect();
        let block: Vec<String> =
            lines.iter().enumerate().map(|(i, l)| editing::apply_line(l, i, edits)).collect();
        let cursor = self.content.cursor();
        let start = self.offset(Pos { line: first, column: 0 });
        let end = self.offset(Pos { line: last, column: lines[lines.len() - 1].len() });
        self.replace(start..end, &block.join("\n"));

        let shift = |p: Position| {
            let p = from_widget(p);
            if p.line < first || p.line > last {
                return p;
            }
            Pos { line: p.line, column: editing::shift_column(p.column, p.line - first, edits) }
        };
        self.place(shift(cursor.position), cursor.selection.map(shift));
    }

    /// Puts back a cursor and selection saved before an edit, `by` lines
    /// further down (or up), in the same columns.
    fn restore_cursor(&mut self, cursor: Cursor, by: isize) {
        // Past the last line means the end of the text: a selection of
        // whole lines that moved down onto the last one.
        let last = self.line_count - 1;
        let last_len = self.line_text(last).len();
        let shift = |p: Position| {
            let line = p.line.saturating_add_signed(by);
            if line > last {
                Pos { line: last, column: last_len }
            } else {
                Pos { line, column: p.column }
            }
        };
        let (position, anchor) = (shift(cursor.position), cursor.selection.map(shift));
        self.place(position, anchor);
    }

    fn line_range(&self, first: usize, last: usize) -> Range<usize> {
        let end = Pos { line: last, column: self.line_text(last).len() };
        self.offset(Pos { line: first, column: 0 })..self.offset(end)
    }

    // ── Commands on whole lines ─────────────────────────────────────────

    /// Ctrl+/. Returns false when BIA doesn't know how this language
    /// writes comments.
    pub fn toggle_comment(&mut self) -> bool {
        let Some(comments) = editing::comments(&self.syntax) else { return false };
        let (first, last) = self.selected_lines();
        let lines: Vec<String> = (first..=last).map(|n| self.line_text(n)).collect();
        let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
        let edits = editing::toggle_comment(&refs, comments);
        self.history.seal();
        self.edit_lines(first, last, &edits);
        self.history.seal();
        true
    }

    /// Shift+Alt+Down / Up: copies the lines below them. Down moves the
    /// cursor to the copy; Up leaves it on the original, which is now above.
    pub fn duplicate_lines(&mut self, down: bool) {
        let (first, last) = self.selected_lines();
        let block = self.text[self.line_range(first, last)].to_owned();
        let end = self.line_range(first, last).end;
        let cursor = self.content.cursor();
        self.history.seal();
        self.replace(end..end, &format!("\n{block}"));
        self.restore_cursor(cursor, if down { (last - first + 1) as isize } else { 0 });
        self.history.seal();
    }

    /// Alt+Up / Down: swaps the lines with the one above or below.
    pub fn move_lines(&mut self, up: bool) {
        let (first, last) = self.selected_lines();
        if (up && first == 0) || (!up && last + 1 >= self.line_count) {
            return;
        }
        let block = self.text[self.line_range(first, last)].to_owned();
        let (range, new) = if up {
            let above = self.line_text(first - 1);
            (self.line_range(first - 1, last), format!("{block}\n{above}"))
        } else {
            let below = self.line_text(last + 1);
            (self.line_range(first, last + 1), format!("{below}\n{block}"))
        };
        let cursor = self.content.cursor();
        self.history.seal();
        self.replace(range, &new);
        self.restore_cursor(cursor, if up { -1 } else { 1 });
        self.history.seal();
    }

    /// Ctrl+Shift+K: deletes the lines, leaving the cursor in its column on
    /// the line that takes their place.
    pub fn delete_lines(&mut self) {
        let (first, last) = self.selected_lines();
        let column = self.cursor_pos().column;
        let range = if last + 1 < self.line_count {
            self.offset(Pos { line: first, column: 0 })
                ..self.offset(Pos { line: last + 1, column: 0 })
        } else if first > 0 {
            self.line_range(first - 1, last).start + self.line_text(first - 1).len()
                ..self.text.len()
        } else {
            0..self.text.len()
        };
        self.history.seal();
        self.replace(range, "");
        let line = first.min(self.line_count - 1);
        let offset = self.offset(Pos { line, column });
        self.place(text::offset_to_pos(&self.text, offset), None);
        self.history.seal();
    }

    /// Ctrl+L: selects the cursor's line, or one more line each time.
    pub fn select_lines(&mut self) {
        let (first, last) = match self.selection() {
            Some((s, e)) if s.column == 0 && e.column == 0 => (s.line, e.line),
            _ => self.selected_lines(),
        };
        let end = if last + 1 < self.line_count {
            Pos { line: last + 1, column: 0 }
        } else {
            Pos { line: last, column: self.line_text(last).len() }
        };
        self.place(end, Some(Pos { line: first, column: 0 }));
        self.history.seal();
    }

    // ── Brackets and quotes ─────────────────────────────────────────────

    fn around_cursor(&self) -> (Option<char>, Option<char>) {
        let at = self.cursor_offset();
        (self.text[..at].chars().next_back(), self.text[at..].chars().next())
    }

    /// Typing a bracket or quote: wraps a selection, steps over the closing
    /// half when it is next, or types both halves. Returns false when
    /// `c` should be typed as usual.
    fn type_pair(&mut self, c: char) -> bool {
        let close = editing::closing(c);
        if let (Some(close), Some((start, end))) = (close, self.selection()) {
            let (s, e) = (self.offset(start), self.offset(end));
            let wrapped = format!("{c}{}{close}", &self.text[s..e]);
            self.history.seal();
            self.replace(s..e, &wrapped);
            let n = c.len_utf8();
            let anchor = text::offset_to_pos(&self.text, s + n);
            self.place(text::offset_to_pos(&self.text, e + n), Some(anchor));
            self.history.seal();
            return true;
        }
        if self.selection().is_some() {
            return false;
        }
        let (before, after) = self.around_cursor();
        // Step over a closing half instead of doubling it. For quotes, only
        // right after text, so a quote typed before a string opens a new one.
        let step_over = after == Some(c)
            && (")]}".contains(c)
                || (editing::is_quote(c) && before.is_some_and(|b| !b.is_whitespace())));
        if step_over {
            self.content.perform(Action::Move(Motion::Right));
            return true;
        }
        let Some(close) = close.filter(|_| editing::should_close(c, before, after, &self.syntax))
        else {
            return false;
        };
        let at = self.cursor_offset();
        self.replace(at..at, &format!("{c}{close}"));
        self.place(text::offset_to_pos(&self.text, at + c.len_utf8()), None);
        true
    }

    /// Backspace between `(` and `)` (or two quotes) deletes both.
    fn delete_pair(&mut self) -> bool {
        if self.selection().is_some() {
            return false;
        }
        let (Some(before), Some(after)) = self.around_cursor() else { return false };
        if editing::closing(before) != Some(after) {
            return false;
        }
        let at = self.cursor_offset();
        self.replace(at - before.len_utf8()..at + after.len_utf8(), "");
        true
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
        let m = bia_core::search::Matcher::new("foo", Default::default()).unwrap();
        b.replace_all(&m.replacements(b.text(), "x"));
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
    fn recovered_text_is_one_undo_step() {
        let mut b = buffer("a\nb");
        let before = b.revision;
        b.recover("a\nb\nc");
        assert!(b.is_dirty());
        assert!(b.revision > before);
        assert_in_sync(&b, &"recover");
        b.undo();
        assert_eq!(b.text(), "a\nb");
        assert!(!b.is_dirty());
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

    fn rust(text: &str) -> Buffer {
        let mut b = buffer(text);
        b.syntax = "Rust".into();
        b
    }

    #[test]
    fn toggle_comment_keeps_the_selection() {
        let mut b = rust("fn a() {\n    x();\n}\n");
        b.perform(Action::Move(Motion::Down));
        b.perform(Action::Move(Motion::End));
        assert!(b.toggle_comment());
        assert_eq!(b.text(), "fn a() {\n    // x();\n}\n");
        assert_eq!(b.cursor_pos(), Pos { line: 1, column: 11 });
        assert!(b.toggle_comment());
        assert_eq!(b.text(), "fn a() {\n    x();\n}\n");
        assert_in_sync(&b, &"comment");

        b.perform(Action::SelectAll);
        b.toggle_comment();
        assert_eq!(b.text(), "// fn a() {\n//     x();\n// }\n");
        b.undo();
        assert_eq!(b.text(), "fn a() {\n    x();\n}\n");
        assert!(!buffer("x").toggle_comment());
    }

    #[test]
    fn line_commands() {
        let mut b = buffer("a\nb\nc");
        b.perform(Action::Move(Motion::Down));
        b.move_lines(true);
        assert_eq!(b.text(), "b\na\nc");
        assert_eq!(b.cursor_pos().line, 0);
        b.move_lines(true);
        assert_eq!(b.text(), "b\na\nc");
        b.move_lines(false);
        b.move_lines(false);
        assert_eq!(b.text(), "a\nc\nb");
        assert_eq!(b.cursor_pos().line, 2);
        b.move_lines(false);
        assert_eq!(b.text(), "a\nc\nb");
        assert_in_sync(&b, &"move");

        b.duplicate_lines(true);
        assert_eq!(b.text(), "a\nc\nb\nb");
        assert_eq!(b.cursor_pos().line, 3);
        b.delete_lines();
        b.delete_lines();
        assert_eq!(b.text(), "a\nc");
        assert_eq!(b.cursor_pos().line, 1);
        b.perform(Action::Move(Motion::DocumentStart));
        b.delete_lines();
        assert_eq!(b.text(), "c");
        b.delete_lines();
        assert_eq!(b.text(), "");
        assert_in_sync(&b, &"delete");
        while b.undo() {}
        assert_eq!(b.text(), "a\nb\nc");
    }

    #[test]
    fn select_lines_grows() {
        let mut b = buffer("a\nb\nc");
        b.select_lines();
        assert_eq!(b.selection_range(), Some(0..2));
        b.select_lines();
        assert_eq!(b.selection_range(), Some(0..4));
        b.select_lines();
        assert_eq!(b.selection_range(), Some(0..5));
        // A selected block moves as a whole.
        b.perform(Action::Move(Motion::DocumentStart));
        b.select_lines();
        b.move_lines(false);
        assert_eq!(b.text(), "b\na\nc");
        assert_eq!(b.selection_range(), Some(2..4));
    }

    #[test]
    fn brackets_and_quotes_close() {
        let mut b = rust("");
        type_text(&mut b, "f(\"x");
        assert_eq!(b.text(), "f(\"x\")");
        type_text(&mut b, "\")");
        assert_eq!(b.text(), "f(\"x\")");
        assert_eq!(b.cursor_offset(), b.text().len());
        // Lifetimes stay single.
        type_text(&mut b, " 'a");
        assert_eq!(b.text(), "f(\"x\") 'a");
        assert_in_sync(&b, &"pairs");

        let mut b = rust("");
        type_text(&mut b, "[");
        b.perform(Action::Edit(Edit::Backspace));
        assert_eq!(b.text(), "");
        // Before a word, a bracket is just a bracket.
        let mut b = rust("x");
        type_text(&mut b, "(");
        assert_eq!(b.text(), "(x");

        // A selection gets wrapped and stays selected.
        let mut b = rust("a b");
        b.perform(Action::Select(Motion::WordRight));
        type_text(&mut b, "[");
        assert_eq!(b.text(), "[a] b");
        assert_eq!(b.selection_range(), Some(1..2));
        b.undo();
        assert_eq!(b.text(), "a b");

        let mut b = rust("");
        b.auto_close = false;
        type_text(&mut b, "(");
        assert_eq!(b.text(), "(");
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
        let chars = ['a', ' ', 'é', '日', '(', ')', '"', '\'', '[', '\t'];
        let pastes = ["foo\nbar", "ç", "", "    indented\n\n", "win\r\ndows", "old\rmac"];

        for seed in 0..150 {
            let mut rng = Rng(seed * 7919 + 1);
            let mut b = rust(original);
            let mut log = Vec::new();
            for _ in 0..60 {
                let command = rng.below(100);
                if command < 12 {
                    let name = match command {
                        0..=2 => {
                            b.toggle_comment();
                            "comment"
                        }
                        3..=5 => {
                            b.move_lines(command == 3);
                            "move"
                        }
                        6..=7 => {
                            b.duplicate_lines(command == 6);
                            "duplicate"
                        }
                        8..=9 => {
                            b.delete_lines();
                            "delete lines"
                        }
                        _ => {
                            b.select_lines();
                            "select lines"
                        }
                    };
                    log.push(name.to_owned());
                    assert_in_sync(&b, &(seed, &log));
                    continue;
                }
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
