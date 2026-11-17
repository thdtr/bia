//! Where text goes on screen. The screen shows rows: without wrapping, one
//! per line; with it, a long line takes several, broken after a space when
//! there is one. Widths are in display cells (see [`char_width`]).

use super::motion::char_width;
use ropey::Rope;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayMap {
    tab_width: usize,
    /// Columns per row, when wrapping.
    wrap: Option<usize>,
    lines: usize,
    /// With wrapping, the rows before each line; one more entry than lines.
    rows_before: Vec<usize>,
}

impl DisplayMap {
    pub fn new(rope: &Rope, tab_width: usize, wrap: Option<usize>) -> Self {
        let lines = rope.len_lines();
        let wrap = wrap.map(|w| w.max(8));
        let mut rows_before = Vec::new();
        if let Some(width) = wrap {
            rows_before.reserve(lines + 1);
            let mut total = 0;
            for line in rope.lines() {
                rows_before.push(total);
                let text = line.to_string();
                total += wrap_points(text.trim_end_matches('\n'), width, tab_width).len();
            }
            rows_before.push(total);
        }
        Self { tab_width, wrap, lines, rows_before }
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

/// Where `line` breaks into rows of at most `width` cells: after the last
/// space that fits, or mid-word when a word is wider than a row.
pub fn wrap_points(line: &str, width: usize, tab_width: usize) -> Vec<usize> {
    let mut starts = vec![0];
    let mut row_start = 0;
    let mut col = 0;
    let mut last_break: Option<usize> = None;
    for (i, c) in line.char_indices() {
        let w = char_width(c, col, tab_width);
        if col + w > width && i > row_start {
            let at = match last_break {
                Some(b) if b > row_start && b <= i => b,
                _ => i,
            };
            starts.push(at);
            row_start = at;
            last_break = None;
            col = line[at..i].chars().fold(0, |col, c| col + char_width(c, col, tab_width));
        }
        col += char_width(c, col, tab_width);
        if c == ' ' || c == '\t' {
            last_break = Some(i + c.len_utf8());
        }
    }
    starts
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
