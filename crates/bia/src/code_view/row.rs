//! One row on screen: a line, or part of one when wrapped, with tabs turned
//! into spaces up to the next tab stop.

use bia_core::editor::{char_width, display_column};

pub struct RowText {
    /// What is drawn.
    pub text: String,
    /// `(byte in the line, byte in text)` at every character boundary,
    /// ending with the row's end.
    map: Vec<(usize, usize)>,
}

impl RowText {
    /// Bytes `start..end` of `line` (a whole line's text, no `\n`).
    pub fn new(line: &str, start: usize, end: usize, tab_width: usize) -> Self {
        let mut text = String::with_capacity(end - start);
        let mut map = Vec::with_capacity(end - start + 1);
        let mut col = display_column(line, start, tab_width);
        for (i, c) in line[start..end].char_indices() {
            map.push((start + i, text.len()));
            let w = char_width(c, col, tab_width);
            if c == '\t' {
                text.extend(std::iter::repeat_n(' ', w));
            } else {
                text.push(c);
            }
            col += w;
        }
        map.push((end, text.len()));
        Self { text, map }
    }

    #[cfg(test)]
    pub fn start(&self) -> usize {
        self.map[0].0
    }

    #[cfg(test)]
    pub fn end(&self) -> usize {
        self.map[self.map.len() - 1].0
    }

    /// Where byte `at` of the line is in [`Self::text`]. Offsets outside
    /// the row clamp to its ends.
    pub fn to_text(&self, at: usize) -> usize {
        let i = self.map.partition_point(|&(src, _)| src <= at).saturating_sub(1);
        self.map[i].1
    }

    /// The line byte nearest to byte `at` of [`Self::text`]: inside a tab,
    /// the nearer side of it.
    pub fn to_line(&self, at: usize) -> usize {
        let i = self.map.partition_point(|&(_, d)| d <= at).saturating_sub(1);
        let (src, disp) = self.map[i];
        match self.map.get(i + 1) {
            Some(&(next_src, next_disp)) if at > disp && (at - disp) * 2 >= next_disp - disp => {
                next_src
            }
            _ => src,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tabs_become_spaces_to_the_next_stop() {
        let row = RowText::new("a\tb\tc", 0, 5, 4);
        assert_eq!(row.text, "a   b   c");
        assert_eq!(row.to_text(2), 4);
        assert_eq!(row.to_line(4), 2);
        // Inside a tab: the nearer side.
        assert_eq!(row.to_line(2), 1);
        assert_eq!(row.to_line(3), 2);
        assert_eq!(row.to_text(99), 9);
    }

    #[test]
    fn wrapped_rows_keep_tab_stops() {
        let line = "abcdef\tx";
        let row = RowText::new(line, 4, line.len(), 4);
        // The tab starts at column 6, so it is 2 wide.
        assert_eq!(row.text, "ef  x");
        assert_eq!((row.start(), row.end()), (4, 8));
        assert_eq!(row.to_text(4), 0);
        assert_eq!(row.to_line(5), 8);
    }
}
