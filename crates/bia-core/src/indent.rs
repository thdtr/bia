//! Indentation: what one level is in a file, and the text that Tab,
//! Shift+Tab, Enter and Backspace produce with it.
//!
//! A file's own indentation wins. Only files with no indented lines yet fall
//! back to the language's convention (tabs for Go and Makefiles) or to
//! `tab_size` spaces from the settings.
//!
//! Columns are byte offsets inside a line, like everywhere else in BIA.

use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Indent {
    /// Indent with `\t` instead of spaces.
    pub tabs: bool,
    /// Spaces per level. With `tabs`, it's how many spaces Shift+Tab removes
    /// from a line indented with spaces anyway.
    pub width: usize,
}

impl Indent {
    pub const fn spaces(width: usize) -> Self {
        Self { tabs: false, width }
    }

    pub const fn tabs(width: usize) -> Self {
        Self { tabs: true, width }
    }

    /// The indentation for `path`: detected from `text`, else the language's
    /// convention, else `tab_size` spaces (or tabs, with `tabs`).
    pub fn for_file(path: &Path, text: &str, tab_size: usize, tabs: bool) -> Self {
        let tab_size = tab_size.clamp(1, 8);
        match detect(text) {
            Some(Detected::Tabs) => Self::tabs(tab_size),
            Some(Detected::Spaces(n)) => Self::spaces(n),
            None if tabs || wants_tabs(path) => Self::tabs(tab_size),
            None => Self::spaces(tab_size),
        }
    }

    /// One level of indentation.
    pub fn unit(self) -> String {
        if self.tabs { "\t".to_owned() } else { " ".repeat(self.width) }
    }

    /// What Tab inserts at the end of `before` (the line up to the cursor):
    /// a tab, or spaces up to the next tab stop.
    pub fn insert_at(self, before: &str) -> String {
        if self.tabs {
            return "\t".to_owned();
        }
        let column = before.chars().fold(0, |col, c| match c {
            '\t' => col + self.width - col % self.width,
            _ => col + 1,
        });
        " ".repeat(self.width - column % self.width)
    }

    /// What Enter inserts at `column` of `line`, and where the cursor ends up
    /// (a byte offset into the returned text).
    ///
    /// The new line keeps the current indentation. After an opening bracket
    /// it gets one more level, and if the matching bracket comes right after
    /// the cursor, it moves to a line of its own: `{|}` becomes
    /// `{` / `····|` / `}`.
    pub fn newline(self, line: &str, column: usize) -> (String, usize) {
        let column = floor_boundary(line, column.min(line.len()));
        let (before, after) = line.split_at(column);
        let leading = &before[..before.len() - before.trim_start().len()];
        let mut text = format!("\n{leading}");

        let opener = before.trim_end().chars().last();
        let Some(close) = opener.and_then(closing) else {
            let cursor = text.len();
            return (text, cursor);
        };
        text.push_str(&self.unit());
        let cursor = text.len();
        if after.trim_start().starts_with(close) {
            text.push('\n');
            text.push_str(leading);
        }
        (text, cursor)
    }

    /// How many bytes Backspace deletes when `before` is the line up to the
    /// cursor. Inside leading spaces it goes back to the previous tab stop;
    /// everywhere else, `None` (the usual one character).
    pub fn backspace(self, before: &str) -> Option<usize> {
        if self.tabs || before.is_empty() || !before.bytes().all(|b| b == b' ') {
            return None;
        }
        let n = (before.len() - 1) % self.width + 1;
        (n > 1).then_some(n)
    }

    /// How many bytes Shift+Tab removes from the start of `line`.
    pub fn outdent(self, line: &str) -> usize {
        if line.starts_with('\t') {
            return 1;
        }
        line.bytes().take(self.width).take_while(|&b| b == b' ').count()
    }
}

fn closing(open: char) -> Option<char> {
    match open {
        '{' => Some('}'),
        '[' => Some(']'),
        '(' => Some(')'),
        _ => None,
    }
}

fn floor_boundary(s: &str, mut i: usize) -> usize {
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

/// Go and Makefiles are indented with tabs by convention (Makefiles need them).
fn wants_tabs(path: &Path) -> bool {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default();
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or_default();
    matches!(name, "Makefile" | "makefile" | "GNUmakefile") || matches!(ext, "go" | "mk" | "mak")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Detected {
    Tabs,
    Spaces(usize),
}

/// How `text` is indented, or `None` if no line is.
///
/// Spaces are measured by how much the indentation grows from one line to
/// the next: a file whose lines step in by 2 is a 2-space file, even if some
/// lines sit 8 deep. Steps of one space are ignored, since they are usually
/// alignment (` * ` in block comments, for example).
pub fn detect(text: &str) -> Option<Detected> {
    let mut tab_lines = 0;
    let mut space_lines = 0;
    let mut steps = [0usize; 9];
    let mut previous = 0;
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        if line.starts_with('\t') {
            tab_lines += 1;
            continue;
        }
        let spaces = line.bytes().take_while(|&b| b == b' ').count();
        if spaces >= 2 {
            space_lines += 1;
        }
        if let Some(step) = spaces.checked_sub(previous).filter(|s| (2..=8).contains(s)) {
            steps[step] += 1;
        }
        previous = spaces;
    }
    if tab_lines == 0 && space_lines == 0 {
        return None;
    }
    if tab_lines >= space_lines {
        return Some(Detected::Tabs);
    }
    // The most common step; ties go to the smaller one.
    let best = (2..=8).rev().max_by_key(|&s| steps[s]).filter(|&s| steps[s] > 0)?;
    Some(Detected::Spaces(best))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_spaces_by_step() {
        let two = "a:\n  b:\n    c: 1\n  d: 2\n";
        assert_eq!(detect(two), Some(Detected::Spaces(2)));
        let four = "fn main() {\n    if x {\n        y();\n    }\n    z(a,\n      b);\n}\n";
        assert_eq!(detect(four), Some(Detected::Spaces(4)));
        let comments = "/**\n * one\n * two\n */\nint x;\n";
        assert_eq!(detect(comments), None);
    }

    #[test]
    fn detects_tabs() {
        assert_eq!(detect("func f() {\n\tx()\n\tif y {\n\t\tz()\n\t}\n}\n"), Some(Detected::Tabs));
        assert_eq!(detect("no indentation\nat all\n"), None);
    }

    #[test]
    fn falls_back_to_language_then_settings() {
        assert_eq!(Indent::for_file(Path::new("Makefile"), "", 4, false), Indent::tabs(4));
        assert_eq!(Indent::for_file(Path::new("main.go"), "", 2, false), Indent::tabs(2));
        assert_eq!(Indent::for_file(Path::new("a.py"), "", 4, false), Indent::spaces(4));
        assert_eq!(Indent::for_file(Path::new("a.py"), "x\n  y\n", 4, false), Indent::spaces(2));
        assert_eq!(Indent::for_file(Path::new("a.go"), "x\n    y\n", 4, false), Indent::spaces(4));
        assert_eq!(Indent::for_file(Path::new("a.py"), "", 4, true), Indent::tabs(4));
    }

    #[test]
    fn tab_goes_to_the_next_stop() {
        let four = Indent::spaces(4);
        assert_eq!(four.insert_at(""), "    ");
        assert_eq!(four.insert_at("ab"), "  ");
        assert_eq!(four.insert_at("abcd"), "    ");
        assert_eq!(four.insert_at("\tx"), "   ");
        assert_eq!(Indent::tabs(4).insert_at("  "), "\t");
    }

    #[test]
    fn enter_keeps_indentation() {
        let four = Indent::spaces(4);
        assert_eq!(four.newline("    let x = 1;", 14), ("\n    ".to_owned(), 5));
        // Inside the indentation, only what's before the cursor carries over.
        assert_eq!(four.newline("        x", 4), ("\n    ".to_owned(), 5));
        assert_eq!(Indent::tabs(4).newline("\tfoo", 4), ("\n\t".to_owned(), 2));
    }

    #[test]
    fn enter_after_a_bracket() {
        let four = Indent::spaces(4);
        assert_eq!(four.newline("fn main() {", 11), ("\n    ".to_owned(), 5));
        assert_eq!(four.newline("  f(", 4), ("\n      ".to_owned(), 7));
        // Between a pair, the closing bracket gets its own line.
        assert_eq!(four.newline("  x = {}", 7), ("\n      \n  ".to_owned(), 7));
        assert_eq!(four.newline("[]", 1), ("\n    \n".to_owned(), 5));
        // Not after a closing bracket or in the middle of text.
        assert_eq!(four.newline("f()", 3), ("\n".to_owned(), 1));
    }

    #[test]
    fn backspace_in_leading_spaces() {
        let four = Indent::spaces(4);
        assert_eq!(four.backspace("        "), Some(4));
        assert_eq!(four.backspace("      "), Some(2));
        assert_eq!(four.backspace(" "), None);
        assert_eq!(four.backspace("    x "), None);
        assert_eq!(four.backspace(""), None);
        assert_eq!(Indent::tabs(4).backspace("    "), None);
    }

    #[test]
    fn outdent_removes_one_level() {
        let four = Indent::spaces(4);
        assert_eq!(four.outdent("        x"), 4);
        assert_eq!(four.outdent("  x"), 2);
        assert_eq!(four.outdent("\t\tx"), 1);
        assert_eq!(four.outdent("x"), 0);
    }
}
