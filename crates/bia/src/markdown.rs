//! Markdown on its way to the preview.
//!
//! iced 0.14 only knows fenced code blocks: an indented one comes out as an
//! empty box, with its text as a paragraph after it. Rewriting indented
//! blocks as fenced ones before iced sees them fixes that.

use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use std::borrow::Cow;
use std::ops::Range;

/// The options iced parses with, so both see the same blocks.
fn options() -> Options {
    Options::ENABLE_YAML_STYLE_METADATA_BLOCKS
        | Options::ENABLE_PLUSES_DELIMITED_METADATA_BLOCKS
        | Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
}

pub fn for_preview(text: &str) -> Cow<'_, str> {
    let mut blocks: Vec<(Range<usize>, String)> = Vec::new();
    let mut current: Option<(Range<usize>, String)> = None;
    for (event, range) in Parser::new_ext(text, options()).into_offset_iter() {
        match event {
            Event::Start(Tag::CodeBlock(CodeBlockKind::Indented)) => {
                current = Some((range, String::new()));
            }
            Event::Text(code) => {
                if let Some((_, buffer)) = &mut current {
                    buffer.push_str(&code);
                }
            }
            Event::End(TagEnd::CodeBlock) => {
                // The range starts after the indentation. Only blocks with
                // nothing but that before them on the line: inside a list
                // item, a fence at column 0 would end the list.
                if let Some((range, code)) = current.take() {
                    let line_start = text[..range.start].rfind('\n').map_or(0, |i| i + 1);
                    if text[line_start..range.start].chars().all(|c| c == ' ' || c == '\t') {
                        blocks.push((line_start..range.end, code));
                    }
                }
            }
            _ => {}
        }
    }
    if blocks.is_empty() {
        return Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(text.len() + blocks.len() * 8);
    let mut from = 0;
    for (range, code) in blocks {
        out.push_str(&text[from..range.start]);
        // A fence longer than any run of backticks inside.
        let longest = code.split(|c| c != '`').map(str::len).max().unwrap_or(0);
        let fence = "`".repeat(longest.max(2) + 1);
        out.push_str(&fence);
        out.push('\n');
        out.push_str(&code);
        if !code.ends_with('\n') {
            out.push('\n');
        }
        out.push_str(&fence);
        out.push('\n');
        from = range.end;
    }
    out.push_str(&text[from..]);
    Cow::Owned(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indented_blocks_become_fenced() {
        let text = "# A\n\n    let x = 1;\n\tlet y = 2;\n\nend\n";
        assert_eq!(for_preview(text), "# A\n\n```\nlet x = 1;\nlet y = 2;\n```\n\nend\n");
        let plain = "no code\n\n```rust\nfenced\n```\n";
        assert!(matches!(for_preview(plain), Cow::Borrowed(_)));
        // Backticks inside get a longer fence.
        let ticks = "x\n\n    a ``` b\n";
        assert_eq!(for_preview(ticks), "x\n\n````\na ``` b\n````\n");
    }
}
