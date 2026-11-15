//! Helpers over `\n`-separated text: byte offsets ↔ (line, column) and search.
//!
//! Columns are **byte** offsets inside the line, which is what the editor
//! widget uses. Use [`char_column`] to show a human column number.

/// A (line, byte-column) pair, both zero-based.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct Pos {
    pub line: usize,
    pub column: usize,
}

/// Converts a byte offset into a position. Offsets past the end clamp.
pub fn offset_to_pos(text: &str, offset: usize) -> Pos {
    let offset = floor_boundary(text, offset.min(text.len()));
    let before = &text[..offset];
    let line = before.matches('\n').count();
    let line_start = before.rfind('\n').map_or(0, |i| i + 1);
    Pos { line, column: offset - line_start }
}

/// Converts a position into a byte offset. Positions past the end clamp.
pub fn pos_to_offset(text: &str, pos: Pos) -> usize {
    let mut start = 0;
    for _ in 0..pos.line {
        match text[start..].find('\n') {
            Some(i) => start += i + 1,
            None => return text.len(),
        }
    }
    let line_end = text[start..].find('\n').map_or(text.len(), |i| start + i);
    floor_boundary(text, (start + pos.column).min(line_end))
}

/// 1-based character column for display.
pub fn char_column(line: &str, byte_column: usize) -> usize {
    let end = floor_boundary(line, byte_column.min(line.len()));
    line[..end].chars().count() + 1
}

fn floor_boundary(s: &str, mut i: usize) -> usize {
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

/// Width of `line` in monospace cells: tabs count as 8 and non-ASCII
/// characters as 2. This errs on the wide side, since it sizes the scrollable
/// area and running short would hide the end of a line.
pub fn display_columns(line: &str) -> usize {
    line.chars()
        .map(|c| match c {
            '\t' => 8,
            c if c.is_ascii() => 1,
            _ => 2,
        })
        .sum()
}

/// Widest line of `text`, in [`display_columns`].
pub fn max_columns(text: &str) -> usize {
    text.split('\n').map(display_columns).max().unwrap_or(0)
}

/// Byte offset where line number `n` (1-based) starts in the gutter string
/// `"1\n2\n…"`, computed without building the string.
pub fn gutter_offset(n: usize) -> usize {
    let before = n.saturating_sub(1);
    let mut digits = 0;
    let mut start = 1;
    let mut width = 1;
    while start <= before {
        let end = (start * 10 - 1).min(before);
        digits += (end - start + 1) * width;
        start *= 10;
        width += 1;
    }
    digits + before
}

/// Byte ranges of every case-insensitive (ASCII) match of `needle`.
pub fn find_all(haystack: &str, needle: &str) -> Vec<std::ops::Range<usize>> {
    if needle.is_empty() {
        return Vec::new();
    }
    // ASCII lowercasing keeps byte lengths, so ranges stay valid.
    let hay = haystack.to_ascii_lowercase();
    let needle = needle.to_ascii_lowercase();
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(i) = hay[from..].find(&needle) {
        let start = from + i;
        out.push(start..start + needle.len());
        from = start + needle.len();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_positions() {
        let text = "fn main() {\n    olá();\n}\n";
        for offset in 0..=text.len() {
            if text.is_char_boundary(offset) {
                assert_eq!(pos_to_offset(text, offset_to_pos(text, offset)), offset);
            }
        }
        assert_eq!(offset_to_pos(text, 12), Pos { line: 1, column: 0 });
        assert_eq!(pos_to_offset(text, Pos { line: 9, column: 0 }), text.len());
        assert_eq!(pos_to_offset(text, Pos { line: 0, column: 99 }), 11);
    }

    #[test]
    fn char_columns() {
        assert_eq!(char_column("olá mundo", 4), 4);
        assert_eq!(char_column("abc", 0), 1);
    }

    #[test]
    fn columns() {
        assert_eq!(display_columns("abc"), 3);
        assert_eq!(display_columns("\tx"), 9);
        assert_eq!(display_columns("ação"), 6);
        assert_eq!(max_columns("a\nabcd\n"), 4);
    }

    #[test]
    fn gutter_offsets_match_the_real_string() {
        let gutter: String = (1..=1200).map(|n| n.to_string()).collect::<Vec<_>>().join("\n");
        for n in [1, 2, 9, 10, 11, 99, 100, 101, 1000, 1200] {
            let expected = gutter.find(&format!("\n{n}\n")).map_or(0, |i| i + 1);
            let expected = if n == 1 {
                0
            } else if n == 1200 {
                gutter.rfind('\n').unwrap() + 1
            } else {
                expected
            };
            assert_eq!(gutter_offset(n), expected, "line {n}");
        }
    }

    #[test]
    fn finds_case_insensitive() {
        assert_eq!(find_all("Foo foo FOO", "foo"), vec![0..3, 4..7, 8..11]);
        assert!(find_all("abc", "").is_empty());
        assert_eq!(find_all("ação Ação", "ção"), vec![1..6, 8..13]);
    }
}
