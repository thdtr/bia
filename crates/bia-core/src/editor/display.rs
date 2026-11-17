//! Where text goes on screen. The screen shows rows: without wrapping, one
//! per line; with it, a long line takes several, broken after a space when
//! there is one. Widths are in display cells (see [`char_width`]).

use super::motion::char_width;
use super::{Changed, Editor};
use ropey::{Rope, RopeSlice};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayMap {
    tab_width: usize,
    /// Columns per row, when wrapping.
    wrap: Option<usize>,
    lines: usize,
    /// The editor revision this map is for.
    revision: u64,
    /// With wrapping, the rows of each line.
    rows: Vec<usize>,
    /// With wrapping, the rows before each line; one more entry than lines.
    rows_before: Vec<usize>,
}

impl DisplayMap {
    pub fn new(rope: &Rope, tab_width: usize, wrap: Option<usize>) -> Self {
        let lines = rope.len_lines();
        let wrap = wrap.map(|w| w.max(8));
        let rows = match wrap {
            Some(width) => rope.lines().map(|line| line_rows(line, width, tab_width)).collect(),
            None => Vec::new(),
        };
        let mut map = Self { tab_width, wrap, lines, revision: 0, rows, rows_before: Vec::new() };
        map.sum_from(0);
        map
    }

    pub fn for_editor(editor: &Editor, wrap: Option<usize>) -> Self {
        let mut map = Self::new(editor.rope(), editor.tab_width, wrap);
        map.revision = editor.revision();
        map
    }

    /// Brings the map up to date with `editor`, wrapping at `wrap`. After
    /// an edit, only the lines it touched are measured again.
    pub fn sync(&mut self, editor: &Editor, wrap: Option<usize>) {
        let wrap = wrap.map(|w| w.max(8));
        if wrap != self.wrap || editor.tab_width != self.tab_width {
            *self = Self::for_editor(editor, wrap);
            return;
        }
        match editor.changed_lines(self.revision) {
            Changed::Nothing => {}
            Changed::Everything => *self = Self::for_editor(editor, wrap),
            Changed::Lines { first, old_end, new_end } => {
                self.lines = editor.line_count();
                self.revision = editor.revision();
                let Some(width) = wrap else { return };
                let rope = editor.rope();
                let measured: Vec<usize> = (first..new_end)
                    .map(|l| line_rows(rope.line(l), width, self.tab_width))
                    .collect();
                let old_end = old_end.clamp(first, self.rows.len());
                self.rows.splice(first..old_end, measured);
                if self.rows.len() == self.lines {
                    self.sum_from(first);
                } else {
                    *self = Self::for_editor(editor, wrap);
                }
            }
        }
        self.revision = editor.revision();
    }

    /// Whether the map already matches `editor` wrapped at `wrap`.
    pub fn is_current(&self, editor: &Editor, wrap: Option<usize>) -> bool {
        self.revision == editor.revision()
            && self.wrap == wrap.map(|w| w.max(8))
            && self.tab_width == editor.tab_width
    }

    /// Recomputes `rows_before` from line `first` on.
    fn sum_from(&mut self, first: usize) {
        if self.wrap.is_none() {
            return;
        }
        let first = first.min(self.rows_before.len().saturating_sub(1));
        self.rows_before.truncate(first + 1);
        let mut total = self.rows_before.last().copied().unwrap_or(0);
        if self.rows_before.is_empty() {
            self.rows_before.push(0);
        }
        for &rows in &self.rows[first..] {
            total += rows;
            self.rows_before.push(total);
        }
    }

    pub fn wrap(&self) -> Option<usize> {
        self.wrap
    }
    pub fn row_count(&self) -> usize {
        match self.wrap {
            Some(_) => self.rows_before[self.lines],
            None => self.lines,
        }
    }

    /// The first row of `line`.
    pub fn first_row(&self, line: usize) -> usize {
        match self.wrap {
            Some(_) => self.rows_before[line.min(self.lines)],
            None => line,
        }
    }

    /// The line that row `row` belongs to.
    pub fn line_of_row(&self, row: usize) -> usize {
        match self.wrap {
            Some(_) => {
                let row = row.min(self.row_count().saturating_sub(1));
                self.rows_before
                    .partition_point(|&r| r <= row)
                    .saturating_sub(1)
                    .min(self.lines - 1)
            }
            None => row.min(self.lines.saturating_sub(1)),
        }
    }

    /// Byte offsets in `line` (one line's text, no `\n`) where its rows
    /// start. Always begins with 0.
    pub fn row_starts(&self, line: &str) -> Vec<usize> {
        match self.wrap {
            Some(width) => wrap_points(line, width, self.tab_width),
            None => vec![0],
        }
    }

    /// The row of byte `column` in `line`, counted from the line's first row.
    /// An offset right at a break belongs to the row it starts.
    pub fn row_in_line(&self, line: &str, column: usize) -> usize {
        let starts = self.row_starts(line);
        starts.partition_point(|&s| s <= column).saturating_sub(1)
    }
}

/// How many rows `line` (with or without its `\n`) takes, wrapped at
/// `width`. Measuring every line of a long file happens whenever the width
/// changes, so this doesn't allocate, and skips the lines that obviously fit.
fn line_rows(line: RopeSlice<'_>, width: usize, tab_width: usize) -> usize {
    let len = line.len_bytes();
    let len = if len > 0 && line.byte(len - 1) == b'\n' { len - 1 } else { len };
    // Any character but a tab takes no more cells than it has bytes.
    if len <= width && !line.chunks().any(|c| c.contains('\t')) {
        return 1;
    }
    let line = line.byte_slice(..len);
    let mut rows = 1;
    match line.as_str() {
        Some(text) => wrap_each(text, width, tab_width, |_| rows += 1),
        None => wrap_each(&line.to_string(), width, tab_width, |_| rows += 1),
    }
    rows
}

/// Where `line` breaks into rows of at most `width` cells: after the last
/// space that fits, or mid-word when a word is wider than a row.
pub fn wrap_points(line: &str, width: usize, tab_width: usize) -> Vec<usize> {
    let mut starts = vec![0];
    wrap_each(line, width, tab_width, |at| starts.push(at));
    starts
}

/// Calls `row` with the start of every row of `line` after the first.
fn wrap_each(line: &str, width: usize, tab_width: usize, row: impl FnMut(usize)) {
    if line.is_ascii() && !line.contains('\t') {
        wrap_ascii(line.as_bytes(), width, row);
    } else {
        wrap_chars(line, width, tab_width, row);
    }
}

/// [`wrap_chars`] for text where every character is one cell.
fn wrap_ascii(line: &[u8], width: usize, mut row: impl FnMut(usize)) {
    let mut row_start = 0;
    let mut last_break: Option<usize> = None;
    for (i, &b) in line.iter().enumerate() {
        if i - row_start >= width && i > row_start {
            let at = match last_break {
                Some(b) if b > row_start && b <= i => b,
                _ => i,
            };
            row(at);
            row_start = at;
            last_break = None;
        }
        if b == b' ' {
            last_break = Some(i + 1);
        }
    }
}

fn wrap_chars(line: &str, width: usize, tab_width: usize, mut row: impl FnMut(usize)) {
    let mut row_start = 0;
    let mut col = 0;
    let mut last_break: Option<usize> = None;
    for (i, c) in line.char_indices() {
        let mut w = char_width(c, col, tab_width);
        if col + w > width && i > row_start {
            let at = match last_break {
                Some(b) if b > row_start && b <= i => b,
                _ => i,
            };
            row(at);
            row_start = at;
            last_break = None;
            col = line[at..i].chars().fold(0, |col, c| col + char_width(c, col, tab_width));
            // A tab is narrower or wider where the row now puts it.
            w = char_width(c, col, tab_width);
        }
        col += w;
        if c == ' ' || c == '\t' {
            last_break = Some(i + c.len_utf8());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_after_spaces() {
        assert_eq!(wrap_points("aaaa bbbb cccc", 10, 4), [0, 10]);
        assert_eq!(wrap_points("aaaa bbbb cccc", 9, 4), [0, 5]);
        assert_eq!(wrap_points("aaaa bbbb cccc", 8, 4), [0, 5, 10]);
        assert_eq!(wrap_points("abcdefghijkl", 5, 4), [0, 5, 10]);
        assert_eq!(wrap_points("", 5, 4), [0]);
        // Wide characters take two cells.
        assert_eq!(wrap_points("日本語日本", 4, 4), [0, 6, 12]);
    }

    #[test]
    fn counting_rows_matches_the_breaks() {
        let lines = [
            "",
            "short",
            "aaaa bbbb cccc dddd",
            "a\tb\tc\td\te",
            "日本語日本語日本語",
            "x".repeat(40).as_str(),
        ]
        .map(str::to_owned);
        for line in &lines {
            for width in [8, 10, 17] {
                let rope = Rope::from_str(&format!("{line}\n"));
                assert_eq!(
                    line_rows(rope.line(0), width, 4),
                    wrap_points(line, width, 4).len(),
                    "{line:?} at {width}"
                );
            }
        }
    }

    #[test]
    fn ascii_breaks_like_any_text() {
        let lines = [
            "aaaa bbbb cccc dddd",
            "  indented words that go on and on",
            "x".repeat(40).as_str(),
            "a b c d e f g h i j k l",
        ]
        .map(str::to_owned);
        for line in &lines {
            for width in 1..24 {
                let (mut fast, mut slow) = (Vec::new(), Vec::new());
                wrap_ascii(line.as_bytes(), width, |at| fast.push(at));
                wrap_chars(line, width, 4, |at| slow.push(at));
                assert_eq!(fast, slow, "{line:?} at {width}");
            }
        }
    }

    #[test]
    fn rows_and_lines() {
        let rope = Rope::from_str("short\naaaa bbbb cccc dddd\n\nx");
        let flat = DisplayMap::new(&rope, 4, None);
        assert_eq!(flat.row_count(), 4);
        assert_eq!(flat.line_of_row(9), 3);

        let wrapped = DisplayMap::new(&rope, 4, Some(10));
        // "aaaa bbbb " and "cccc dddd".
        assert_eq!(wrapped.row_count(), 5);
        assert_eq!(wrapped.first_row(1), 1);
        assert_eq!(wrapped.first_row(2), 3);
        assert_eq!(wrapped.line_of_row(2), 1);
        assert_eq!(wrapped.line_of_row(3), 2);
        assert_eq!(wrapped.line_of_row(4), 3);
        assert_eq!(wrapped.row_in_line("aaaa bbbb cccc dddd", 10), 1);
        assert_eq!(wrapped.row_in_line("aaaa bbbb cccc dddd", 9), 0);
    }
}
