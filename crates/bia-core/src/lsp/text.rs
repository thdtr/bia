//! Where things are, the way the Language Server Protocol counts: files as
//! `file://` URIs, positions as a line and a column in UTF-16 code units
//! (or bytes, when the server agrees to UTF-8).

use ropey::Rope;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Encoding {
    Utf8,
    Utf16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Position {
    pub line: u32,
    pub character: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Range {
    pub start: Position,
    pub end: Position,
}

fn units(c: char, encoding: Encoding) -> usize {
    match encoding {
        Encoding::Utf8 => c.len_utf8(),
        Encoding::Utf16 => c.len_utf16(),
    }
}

/// The position of byte `at`.
pub fn position(rope: &Rope, at: usize, encoding: Encoding) -> Position {
    let at = at.min(rope.len_bytes());
    let line = rope.byte_to_line(at);
    let start = rope.line_to_byte(line);
    let character = rope.byte_slice(start..at).chars().map(|c| units(c, encoding)).sum::<usize>();
    Position { line: line as u32, character: character as u32 }
}

/// The byte at `position`, clamped to the text and to its line.
pub fn offset(rope: &Rope, position: Position, encoding: Encoding) -> usize {
    let line = position.line as usize;
    if line >= rope.len_lines() {
        return rope.len_bytes();
    }
    let mut at = rope.line_to_byte(line);
    let mut count = 0;
    for c in rope.line(line).chars() {
        if c == '\n' || count >= position.character as usize {
            break;
        }
        count += units(c, encoding);
        at += c.len_utf8();
    }
    at
}

pub fn range(rope: &Rope, range: std::ops::Range<usize>, encoding: Encoding) -> Range {
    Range { start: position(rope, range.start, encoding), end: position(rope, range.end, encoding) }
}

pub fn offsets(rope: &Rope, range: Range, encoding: Encoding) -> std::ops::Range<usize> {
    let start = offset(rope, range.start, encoding);
    start..offset(rope, range.end, encoding).max(start)
}

/// `file://` URI of an absolute path, with everything but unreserved
/// characters and `/` percent-encoded.
pub fn uri(path: &Path) -> String {
    let path = path.to_string_lossy().replace('\\', "/");
    let mut out = String::from(if path.starts_with('/') { "file://" } else { "file:///" });
    for b in path.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => {
                out.push(b as char)
            }
            // Windows drive letters: `C:`.
            b':' if out.len() <= 10 => out.push(':'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// The path of a `file://` URI.
pub fn path(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    let rest = rest.strip_prefix("localhost").unwrap_or(rest);
    let mut bytes = Vec::with_capacity(rest.len());
    let mut iter = rest.bytes();
    while let Some(b) = iter.next() {
        if b == b'%' {
            let hex = [iter.next()?, iter.next()?];
            bytes.push(u8::from_str_radix(std::str::from_utf8(&hex).ok()?, 16).ok()?);
        } else {
            bytes.push(b);
        }
    }
    let text = String::from_utf8(bytes).ok()?;
    // `/C:/x` on Windows.
    let text = match text.as_bytes() {
        [b'/', drive, b':', ..] if drive.is_ascii_alphabetic() && cfg!(windows) => {
            text[1..].to_owned()
        }
        _ => text,
    };
    Some(PathBuf::from(text))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positions_count_utf16_or_bytes() {
        let rope = Rope::from_str("aé😀b\nx");
        // 😀 is two UTF-16 units and four bytes.
        let b = 1 + 2 + 4;
        assert_eq!(position(&rope, b, Encoding::Utf16), Position { line: 0, character: 4 });
        assert_eq!(position(&rope, b, Encoding::Utf8), Position { line: 0, character: 7 });
        assert_eq!(offset(&rope, Position { line: 0, character: 4 }, Encoding::Utf16), b);
        assert_eq!(offset(&rope, Position { line: 0, character: 99 }, Encoding::Utf16), 8);
        assert_eq!(offset(&rope, Position { line: 1, character: 1 }, Encoding::Utf8), 10);
        assert_eq!(offset(&rope, Position { line: 9, character: 0 }, Encoding::Utf8), 10);
    }

    #[test]
    fn uris() {
        let p = Path::new("/home/a b/ação.rs");
        let u = uri(p);
        assert_eq!(u, "file:///home/a%20b/a%C3%A7%C3%A3o.rs");
        assert_eq!(path(&u).as_deref(), Some(p));
        assert_eq!(path("https://x"), None);
    }
}
