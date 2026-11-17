//! Moving through text: by grapheme, by word, by line, and to a display
//! column. Offsets are bytes into the rope, always on a char boundary.

use crate::search::is_word;
use ropey::Rope;
use unicode_segmentation::{GraphemeCursor, GraphemeIncomplete};
use unicode_width::UnicodeWidthChar;

/// Start of the grapheme after the one at `at` (`é`, `👍🏽`, `🇧🇷` are one).
pub fn next_grapheme(rope: &Rope, at: usize) -> usize {
    let len = rope.len_bytes();
    if at >= len {
        return len;
    }
    let (mut chunk, mut chunk_start, _, _) = rope.chunk_at_byte(at);
    let mut cursor = GraphemeCursor::new(at, len, true);
    loop {
        match cursor.next_boundary(chunk, chunk_start) {
            Ok(n) => return n.unwrap_or(len),
            Err(GraphemeIncomplete::NextChunk) => {
                chunk_start += chunk.len();
                chunk = rope.chunk_at_byte(chunk_start).0;
            }
            Err(GraphemeIncomplete::PreContext(n)) => {
                let context = rope.chunk_at_byte(n - 1).0;
                cursor.provide_context(context, n - context.len());
            }
            Err(_) => {
                return (at + 1..=len).find(|&i| rope.try_byte_to_char(i).is_ok()).unwrap_or(len);
            }
        }
    }
}

/// Start of the grapheme before `at`.
pub fn prev_grapheme(rope: &Rope, at: usize) -> usize {
    if at == 0 {
        return 0;
    }
    let at = at.min(rope.len_bytes());
    let (mut chunk, mut chunk_start, _, _) = rope.chunk_at_byte(at);
    let mut cursor = GraphemeCursor::new(at, rope.len_bytes(), true);
    loop {
        match cursor.prev_boundary(chunk, chunk_start) {
            Ok(n) => return n.unwrap_or(0),
            Err(GraphemeIncomplete::PrevChunk) => {
                let (c, start, _, _) = rope.chunk_at_byte(chunk_start - 1);
                chunk = c;
                chunk_start = start;
            }
            Err(GraphemeIncomplete::PreContext(n)) => {
                let context = rope.chunk_at_byte(n - 1).0;
                cursor.provide_context(context, n - context.len());
            }
            Err(_) => {
                let c = rope.byte_to_char(at);
                return rope.char_to_byte(c.saturating_sub(1));
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Class {
    Space,
    Newline,
    Word,
    Punctuation,
}

fn class(c: char) -> Class {
    match c {
        '\n' => Class::Newline,
        c if c.is_whitespace() => Class::Space,
        c if is_word(c) => Class::Word,
        _ => Class::Punctuation,
    }
}

pub(super) fn char_after(rope: &Rope, at: usize) -> Option<char> {
    let ci = rope.byte_to_char(at);
    (ci < rope.len_chars()).then(|| rope.char(ci))
}

pub(super) fn char_before(rope: &Rope, at: usize) -> Option<char> {
    let ci = rope.byte_to_char(at);
    (ci > 0).then(|| rope.char(ci - 1))
}

fn skip_right(rope: &Rope, mut at: usize, kind: Class) -> usize {
    while let Some(c) = char_after(rope, at).filter(|&c| class(c) == kind) {
        at += c.len_utf8();
    }
    at
}

fn skip_left(rope: &Rope, mut at: usize, kind: Class) -> usize {
    while let Some(c) = char_before(rope, at).filter(|&c| class(c) == kind) {
        at -= c.len_utf8();
    }
    at
}

/// Ctrl+Right: past the spaces, then past one word or one run of
/// punctuation. At the end of a line, onto the next one.
pub fn word_right(rope: &Rope, at: usize) -> usize {
    let mut pos = skip_right(rope, at, Class::Space);
    if char_after(rope, pos) == Some('\n') {
        if pos != at {
            return pos;
        }
        pos = skip_right(rope, pos + 1, Class::Space);
    }
    match char_after(rope, pos).map(class) {
        Some(Class::Newline) | None => pos,
        Some(kind) => skip_right(rope, pos, kind),
    }
}

/// Ctrl+Left: the mirror of [`word_right`].
pub fn word_left(rope: &Rope, at: usize) -> usize {
    let mut pos = skip_left(rope, at, Class::Space);
    if char_before(rope, pos) == Some('\n') {
        if pos != at {
            return pos;
        }
        pos = skip_left(rope, pos - 1, Class::Space);
    }
    match char_before(rope, pos).map(class) {
        Some(Class::Newline) | None => pos,
        Some(kind) => skip_left(rope, pos, kind),
    }
}

/// The word around `at` (or just before it), as a byte range. Empty when
/// there's no word there.
pub fn word_at(rope: &Rope, at: usize) -> std::ops::Range<usize> {
    let ci = rope.byte_to_char(at);
    let mut start = at;
    let mut back = rope.chars_at(ci);
    while let Some(c) = back.prev() {
        if !is_word(c) {
            break;
        }
        start -= c.len_utf8();
    }
    let mut end = at;
    for c in rope.chars_at(ci) {
        if !is_word(c) {
            break;
        }
        end += c.len_utf8();
    }
    start..end
}

/// Display width of a character at display column `column`: tabs reach the
/// next multiple of `tab_width`, wide characters take two cells.
pub fn char_width(c: char, column: usize, tab_width: usize) -> usize {
    match c {
        '\t' => tab_width - column % tab_width,
        c => c.width().unwrap_or(0).max(usize::from(!c.is_control())),
    }
}

/// Display column of byte `column` in `line`.
pub fn display_column(line: &str, column: usize, tab_width: usize) -> usize {
    line[..column.min(line.len())].chars().fold(0, |col, c| col + char_width(c, col, tab_width))
}

/// Byte column in `line` at display column `goal`. A goal inside a wide
/// character or a tab goes to its nearer side.
pub fn column_for_display(line: &str, goal: usize, tab_width: usize) -> usize {
    let mut col = 0;
    for (i, c) in line.char_indices() {
        if c == '\n' || col >= goal {
            return i;
        }
        let w = char_width(c, col, tab_width);
        if col + w > goal {
            return if goal - col > w / 2 { i + c.len_utf8() } else { i };
        }
        col += w;
    }
    line.trim_end_matches('\n').len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graphemes() {
        let rope = Rope::from_str("aé👍🏽🇧🇷\n");
        let mut stops = vec![0];
        while *stops.last().unwrap() < rope.len_bytes() {
            stops.push(next_grapheme(&rope, *stops.last().unwrap()));
        }
        assert_eq!(stops, [0, 1, 3, 11, 19, 20]);
        let mut back = vec![rope.len_bytes()];
        while *back.last().unwrap() > 0 {
            back.push(prev_grapheme(&rope, *back.last().unwrap()));
        }
        back.reverse();
        assert_eq!(back, stops);
    }

    #[test]
    fn graphemes_across_chunks() {
        // Long enough for ropey to split it into several chunks.
        let text = "é".repeat(3000);
        let rope = Rope::from_str(&text);
        let mut at = 0;
        let mut steps = 0;
        while at < rope.len_bytes() {
            at = next_grapheme(&rope, at);
            steps += 1;
        }
        assert_eq!(steps, 3000);
        while at > 0 {
            at = prev_grapheme(&rope, at);
            steps -= 1;
        }
        assert_eq!(steps, 0);
    }

    #[test]
    fn words() {
        let rope = Rope::from_str("foo.bar  baz\n  qux");
        assert_eq!(word_right(&rope, 0), 3);
        assert_eq!(word_right(&rope, 3), 4);
        assert_eq!(word_right(&rope, 4), 7);
        assert_eq!(word_right(&rope, 7), 12);
        assert_eq!(word_right(&rope, 12), 18);
        assert_eq!(word_left(&rope, 18), 15);
        assert_eq!(word_left(&rope, 15), 13);
        assert_eq!(word_left(&rope, 13), 9);
        assert_eq!(word_left(&rope, 12), 9);
        assert_eq!(word_left(&rope, 9), 4);
        assert_eq!(word_left(&rope, 4), 3);
        assert_eq!(word_left(&rope, 3), 0);
        assert_eq!(word_at(&rope, 5), 4..7);
        assert_eq!(word_at(&rope, 7), 4..7);
        assert_eq!(word_at(&rope, 8), 8..8);
    }

    #[test]
    fn display_columns() {
        assert_eq!(display_column("\tab", 1, 4), 4);
        assert_eq!(display_column("a\tb", 2, 4), 4);
        assert_eq!(display_column("日本", 3, 4), 2);
        assert_eq!(column_for_display("\tab", 4, 4), 1);
        assert_eq!(column_for_display("\tab", 2, 4), 0);
        assert_eq!(column_for_display("ab", 9, 4), 2);
        assert_eq!(column_for_display("日本x", 2, 4), 3);
        assert_eq!(column_for_display("日本x", 4, 4), 6);
    }
}
