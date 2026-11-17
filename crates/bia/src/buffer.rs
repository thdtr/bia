//! An open text file: its [`Editor`] (text, cursors, undo), what saving it
//! faithfully needs, and the caches that drawing it uses.

use crate::code_view::Input;
use crate::keys::Command;
use crate::syntax::Highlights;
use bia_core::document::{Document, LineEnding};
use bia_core::editor::{DisplayMap, Editor, Motion};
use bia_core::git::{self, Mark};
use bia_core::indent::Indent;
use iced::widget::markdown;
use std::cell::RefCell;
use std::ops::Range;
use std::time::SystemTime;

pub struct Buffer {
    pub editor: Editor,
    pub line_ending: LineEnding,
    /// The file mixed LF and CRLF when it was opened.
    pub mixed_endings: bool,
    pub bom: bool,
    /// Disk modification time as of the last load or save, to notice
    /// outside changes.
    pub modified: Option<SystemTime>,
    /// Parsed Markdown, kept only while the preview is open.
    pub markdown: Option<markdown::Content>,
    markdown_revision: u64,
    pub highlights: RefCell<Highlights>,
    /// The rows the text takes on screen, kept up to date by the view
    /// (which knows the width) and used here for Up and Down.
    pub display: RefCell<DisplayMap>,
    /// A new value asks the view to center the cursor (go to line).
    pub center: u64,
    /// The text as of the last commit, for the gutter's change marks.
    pub git: RefCell<GitMarks>,
}

/// Lines changed since the last commit, worked out again when the text
/// changes.
#[derive(Default)]
pub struct GitMarks {
    base: Option<String>,
    revision: Option<u64>,
    marks: Vec<Mark>,
}

/// Past this many lines, the gutter goes without git marks: the diff would
/// slow down typing.
const GIT_MARK_LINES: usize = 50_000;

impl GitMarks {
    pub fn set_base(&mut self, base: Option<String>) {
        if self.base != base {
            self.base = base;
            self.revision = None;
        }
    }

    pub fn marks(&mut self, editor: &Editor) -> &[Mark] {
        if self.revision != Some(editor.revision()) {
            self.revision = Some(editor.revision());
            self.marks = match &self.base {
                Some(base) if editor.line_count() <= GIT_MARK_LINES => {
                    git::marks(base, &editor.text())
                }
                _ => Vec::new(),
            };
        }
        &self.marks
    }
}

/// What an input asks of the app besides changing the text.
#[derive(Debug, PartialEq, Eq)]
pub enum Reply {
    None,
    /// Put this on the clipboard (copy, cut).
    Clipboard(String),
    /// Ctrl+/ in a language BIA has no comment tokens for.
    NoComments,
}

impl Buffer {
    pub fn new(doc: Document, syntax: String, indent: Indent) -> Self {
        let mut editor = Editor::new(&doc.text, indent);
        editor.syntax = syntax;
        let display = RefCell::new(DisplayMap::for_editor(&editor, None));
        Self {
            display,
            editor,
            line_ending: doc.line_ending,
            mixed_endings: doc.mixed_endings,
            bom: doc.bom,
            modified: doc.modified,
            markdown: None,
            markdown_revision: 0,
            highlights: RefCell::default(),
            center: 0,
            git: RefCell::default(),
        }
    }

    pub fn text(&self) -> String {
        self.editor.text()
    }

    pub fn is_dirty(&self) -> bool {
        self.editor.is_dirty()
    }

    pub fn revision(&self) -> u64 {
        self.editor.revision()
    }

    pub fn encoded(&self) -> Vec<u8> {
        Document::encode(&self.text(), self.line_ending, self.bom)
    }

    pub fn mark_saved(&mut self, modified: Option<SystemTime>) {
        self.editor.mark_saved();
        self.modified = modified;
    }

    /// Replaces everything with a fresh copy from disk (outside change).
    pub fn reload(&mut self, doc: Document) {
        let old = &self.editor;
        let (syntax, indent) = (old.syntax.clone(), old.indent);
        let (auto_close, tab_width) = (old.auto_close, old.tab_width);
        let preview = self.markdown.is_some();
        let git = self.git.take();
        *self = Self::new(doc, syntax, indent);
        self.git = RefCell::new(git);
        self.editor.auto_close = auto_close;
        self.editor.tab_width = tab_width;
        if preview {
            self.set_preview(true);
        }
    }

    /// Does what the editor widget asks.
    pub fn input(&mut self, input: Input) -> Reply {
        let ed = &mut self.editor;
        let reply = match input {
            Input::Click { at, extend } => {
                ed.click(at, extend);
                Reply::None
            }
            Input::ToggleCursor(at) => {
                ed.toggle_cursor(at);
                Reply::None
            }
            Input::Select(range) => {
                ed.select(range);
                Reply::None
            }
            Input::Drag { origin, at, unit } => {
                ed.drag(origin, at, unit);
                Reply::None
            }
            Input::Text(text) => {
                for c in text.chars() {
                    ed.type_char(c);
                }
                Reply::None
            }
            Input::Paste(text) => {
                ed.paste(&text);
                Reply::None
            }
            Input::Command(command) => self.command(command),
        };
        self.refresh_markdown();
        reply
    }

    fn command(&mut self, command: Command) -> Reply {
        // With long lines wrapped, Up and Down go by rows on screen.
        let rows = match command {
            Command::Move(Motion::Up, extend) => Some((-1, extend)),
            Command::Move(Motion::Down, extend) => Some((1, extend)),
            Command::Move(Motion::PageUp(n), extend) => Some((-(n as isize), extend)),
            Command::Move(Motion::PageDown(n), extend) => Some((n as isize, extend)),
            _ => None,
        };
        if let Some((delta, extend)) = rows {
            let mut map = self.display.borrow_mut();
            let wrap = map.wrap();
            map.sync(&self.editor, wrap);
            self.editor.move_rows(&map, delta, extend);
            return Reply::None;
        }
        let ed = &mut self.editor;
        match command {
            Command::Move(motion, extend) => ed.move_cursors(motion, extend),
            // The view turns these into moves by its own height, and reads
            // the clipboard itself.
            Command::Page { .. } | Command::Paste => {}
            Command::Backspace => ed.backspace(),
            Command::Delete => ed.delete(),
            Command::DeleteWord { forward } => ed.delete_word(forward),
            Command::Newline => ed.newline(),
            Command::Indent => ed.indent(),
            Command::Outdent => ed.outdent(),
            Command::SelectAll => ed.select_all(),
            Command::Copy => return Reply::Clipboard(ed.copy()),
            Command::Cut => return Reply::Clipboard(ed.cut()),
            Command::ToggleComment => {
                if !ed.toggle_comment() {
                    return Reply::NoComments;
                }
            }
            Command::MoveLines { up } => ed.move_lines(up),
            Command::DuplicateLines { down } => ed.duplicate_lines(down),
            Command::DeleteLines => ed.delete_lines(),
            Command::SelectLines => ed.select_lines(),
            Command::SelectNext => ed.select_next_occurrence(),
            Command::SelectAllOccurrences => ed.select_all_occurrences(),
            Command::AddCursor { up } => ed.add_cursor_vertically(up),
            Command::Collapse => {
                ed.collapse();
            }
        }
        Reply::None
    }

    pub fn undo(&mut self) -> bool {
        let done = self.editor.undo();
        self.refresh_markdown();
        done
    }

    pub fn redo(&mut self) -> bool {
        let done = self.editor.redo();
        self.refresh_markdown();
        done
    }

    /// 1-based line and character column of the cursor, for the status bar.
    pub fn line_col(&self) -> (usize, usize) {
        self.editor.cursor_line_col()
    }

    pub fn cursor_offset(&self) -> usize {
        self.editor.selections().primary().head
    }

    pub fn select(&mut self, range: Range<usize>) {
        self.editor.select(range);
    }

    /// The primary selection, if it isn't empty.
    pub fn selection_range(&self) -> Option<Range<usize>> {
        let s = self.editor.selections().primary();
        (!s.is_empty()).then(|| s.range())
    }

    /// Moves the cursor to `line` (0-based) and character `column`
    /// (0-based), both clamped to the text.
    pub fn go_to(&mut self, line: usize, column: usize) {
        let line = line.min(self.editor.line_count() - 1);
        let text = self.editor.line(line);
        let column = text.char_indices().nth(column).map_or(text.len(), |(i, _)| i);
        let at = self.editor.offset(line, column);
        self.editor.select(at..at);
    }

    /// Replaces `range` as one undo step, leaving the cursor after it.
    pub fn replace_range(&mut self, range: Range<usize>, with: &str) {
        self.editor.select(range.clone());
        self.editor.replace_ranges(vec![(range, with.to_owned())]);
        self.refresh_markdown();
    }

    /// Replaces every range (sorted, not overlapping) as one undo step.
    pub fn replace_all(&mut self, edits: &[(Range<usize>, String)]) {
        self.editor.replace_ranges(edits.to_vec());
        self.refresh_markdown();
    }

    /// Puts back edits recovered from a backup, as one undo step on top of
    /// the file as it is on disk.
    pub fn recover(&mut self, text: &str) {
        let len = self.editor.len();
        self.editor.replace_ranges(vec![(0..len, text.to_owned())]);
        self.editor.select(0..0);
    }

    pub fn set_preview(&mut self, on: bool) {
        self.markdown =
            on.then(|| markdown::Content::parse(&crate::markdown::for_preview(&self.text())));
        self.markdown_revision = self.revision();
    }

    fn refresh_markdown(&mut self) {
        if self.markdown.is_some() && self.markdown_revision != self.revision() {
            self.set_preview(true);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bia_core::editor::Motion;

    fn buffer(text: &str) -> Buffer {
        let mut b = Buffer::new(
            Document::from_bytes(text.as_bytes()).unwrap(),
            "Rust".into(),
            Indent::spaces(4),
        );
        b.editor.select(0..0);
        b
    }

    #[test]
    fn recovered_text_is_one_undo_step() {
        let mut b = buffer("a\nb");
        b.recover("a\nb\nc");
        assert!(b.is_dirty());
        b.undo();
        assert_eq!(b.text(), "a\nb");
        assert!(!b.is_dirty());
    }

    #[test]
    fn inputs() {
        let mut b = buffer("x");
        // Before a word, a bracket stays single.
        b.input(Input::Text("f(".into()));
        assert_eq!(b.text(), "f(x");
        assert_eq!(b.input(Input::Command(Command::Copy)), Reply::Clipboard("f(x\n".into()));
        b.input(Input::Command(Command::Move(Motion::DocumentEnd, false)));
        b.input(Input::Paste("!".into()));
        assert_eq!(b.text(), "f(x!");

        let mut plain = buffer("x");
        plain.editor.syntax = "Plain Text".into();
        assert_eq!(plain.input(Input::Command(Command::ToggleComment)), Reply::NoComments);
    }

    #[test]
    fn go_to_counts_characters() {
        let mut b = buffer("ação\nb");
        b.go_to(0, 2);
        assert_eq!(b.cursor_offset(), 3);
        b.go_to(9, 9);
        assert_eq!(b.line_col(), (2, 2));
    }

    #[test]
    fn replace_leaves_the_cursor_after() {
        let mut b = buffer("foo bar");
        b.replace_range(0..3, "x");
        assert_eq!(b.text(), "x bar");
        assert_eq!(b.cursor_offset(), 1);
    }
}
