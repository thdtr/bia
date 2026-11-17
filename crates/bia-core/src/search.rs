//! Searching text, with the options of the find bar: match case, whole word
//! and regular expression. The find bar and "find in files" share it.
//!
//! Every query goes through the `regex` crate, plain text included (escaped).
//! That gives Unicode case folding for free: `É` finds `é`, which lowercasing
//! bytes by hand would get wrong.

use crate::document::MAX_SIZE;
use regex::{Captures, Regex, RegexBuilder};
use std::collections::HashMap;
use std::fs;
use std::ops::Range;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Options {
    pub case_sensitive: bool,
    pub whole_word: bool,
    pub regex: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueryError {
    Empty,
    /// A regular expression that doesn't compile, with the reason.
    Invalid(String),
}

/// A compiled query.
#[derive(Debug, Clone)]
pub struct Matcher {
    re: Regex,
    options: Options,
}

impl Matcher {
    pub fn new(query: &str, options: Options) -> Result<Self, QueryError> {
        if query.is_empty() {
            return Err(QueryError::Empty);
        }
        let pattern = if options.regex { query.to_owned() } else { regex::escape(query) };
        let re = RegexBuilder::new(&pattern)
            .case_insensitive(!options.case_sensitive)
            // `^` and `$` mean the start and end of a line, as in every editor.
            .multi_line(true)
            .build()
            .map_err(|e| QueryError::Invalid(short_error(&e)))?;
        Ok(Self { re, options })
    }

    /// Byte ranges of every match, in order and not overlapping. Empty
    /// matches (`x*`, `^`) are left out: there would be nothing to select.
    pub fn find_all(&self, text: &str) -> Vec<Range<usize>> {
        self.re.find_iter(text).map(|m| m.range()).filter(|r| self.keep(text, r)).collect()
    }

    /// Every match with the text that replaces it. In regex mode, `with` may
    /// use `$1`, `${name}`, `\n` and `\t`; otherwise it is taken as is.
    pub fn replacements(&self, text: &str, with: &str) -> Vec<(Range<usize>, String)> {
        let template = if self.options.regex { unescape(with) } else { String::new() };
        self.re
            .captures_iter(text)
            .filter_map(|caps| {
                let range = caps.get(0)?.range();
                self.keep(text, &range).then(|| {
                    let new =
                        if self.options.regex { expand(&caps, &template) } else { with.to_owned() };
                    (range, new)
                })
            })
            .collect()
    }

    /// What replaces the match at exactly `range`, if there is one there.
    pub fn replacement_at(&self, text: &str, range: Range<usize>, with: &str) -> Option<String> {
        self.replacements(text, with).into_iter().find(|(r, _)| *r == range).map(|(_, s)| s)
    }

    fn keep(&self, text: &str, range: &Range<usize>) -> bool {
        if range.is_empty() {
            return false;
        }
        if !self.options.whole_word {
            return true;
        }
        // Done by hand rather than with `\b`, which does the wrong thing
        // when the query itself starts or ends with punctuation (`->`).
        let before = text[..range.start].chars().next_back();
        let after = text[range.end..].chars().next();
        !before.is_some_and(is_word) && !after.is_some_and(is_word)
    }
}

/// One match in a file, ready to list and to jump to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    /// 0-based line and character column of the start of the match.
    pub line: usize,
    pub column: usize,
    /// Length in bytes, cut at the end of the line.
    pub len: usize,
    /// The line, shortened to fit a sidebar row.
    pub preview: String,
    /// Where the match is in `preview`.
    pub highlight: Range<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileHits {
    pub path: PathBuf,
    pub hits: Vec<Hit>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Found {
    pub files: Vec<FileHits>,
    /// Number of hits in `files`.
    pub hits: usize,
    /// The search stopped at the limit; there are more.
    pub truncated: bool,
}

/// Searches `files` in order. Text in `open` (unsaved edits) is searched
/// instead of what is on disk. Files that BIA wouldn't open as text
/// (binary, not UTF-8, over 16 MB) are skipped. Stops after `limit` hits.
pub fn search_files(
    files: impl IntoIterator<Item = PathBuf>,
    matcher: &Matcher,
    open: &HashMap<PathBuf, String>,
    limit: usize,
) -> Found {
    let mut found = Found::default();
    if limit == 0 {
        return found;
    }
    for path in files {
        let owned;
        let text = match open.get(&path) {
            Some(text) => text.as_str(),
            None => match read_text(&path) {
                Some(text) => {
                    owned = text;
                    owned.as_str()
                }
                None => continue,
            },
        };
        let ranges = matcher.find_all(text);
        if ranges.is_empty() {
            continue;
        }
        let room = limit - found.hits;
        let hits = to_hits(text, &ranges[..ranges.len().min(room)]);
        found.hits += hits.len();
        found.files.push(FileHits { path, hits });
        // At the limit, there may well be more.
        if found.hits == limit {
            found.truncated = true;
            break;
        }
    }
    found
}

fn read_text(path: &std::path::Path) -> Option<String> {
    if fs::metadata(path).ok()?.len() > MAX_SIZE {
        return None;
    }
    let bytes = fs::read(path).ok()?;
    if bytes[..bytes.len().min(8000)].contains(&0) {
        return None;
    }
    String::from_utf8(bytes).ok()
}

/// Longest preview, in characters, and how much comes before the match:
/// little, since a sidebar row shows about 30 characters.
const PREVIEW: usize = 160;
const PREVIEW_BEFORE: usize = 12;

/// Turns byte ranges (sorted) into hits. Walks the text once.
fn to_hits(text: &str, ranges: &[Range<usize>]) -> Vec<Hit> {
    let mut out = Vec::with_capacity(ranges.len());
    let (mut line, mut line_start, mut scanned) = (0, 0, 0);
    for range in ranges {
        for (i, b) in text[scanned..range.start].bytes().enumerate() {
            if b == b'\n' {
                line += 1;
                line_start = scanned + i + 1;
            }
        }
        scanned = range.start;
        let line_end = text[range.start..].find('\n').map_or(text.len(), |i| range.start + i);
        let full = text[line_start..line_end].trim_end_matches('\r');
        let start = range.start - line_start;
        let end = (range.end - line_start).min(full.len()).max(start);

        // Leading indentation says nothing; a long run before the match is cut.
        let mut from = full.len() - full.trim_start().len();
        from = from.min(start);
        let before_chars = full[from..start].chars().count();
        let mut prefix = "";
        if before_chars > PREVIEW_BEFORE {
            from =
                full[..start].char_indices().rev().nth(PREVIEW_BEFORE - 1).map_or(from, |(i, _)| i);
            prefix = "…";
        }
        let mut preview = format!("{prefix}{}", &full[from..]);
        let highlight = prefix.len() + start - from..prefix.len() + end - from;
        if let Some((cut, _)) = preview.char_indices().nth(PREVIEW) {
            preview.truncate(cut.max(highlight.end));
        }
        out.push(Hit {
            line,
            column: full[..start].chars().count(),
            len: end - start,
            preview,
            highlight,
        });
    }
    out
}

pub fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn expand(caps: &Captures<'_>, template: &str) -> String {
    let mut out = String::new();
    caps.expand(template, &mut out);
    out
}

/// `\n`, `\t` and `\\` in a regex replacement, as other editors allow.
fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('\\') => out.push('\\'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// The regex crate's errors span several lines with a caret diagram. The
/// status bar has room for the last line only, which says what is wrong.
fn short_error(e: &regex::Error) -> String {
    let full = e.to_string();
    full.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or(&full).trim().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn find(query: &str, options: Options, text: &str) -> Vec<Range<usize>> {
        Matcher::new(query, options).unwrap().find_all(text)
    }

    #[test]
    fn plain_text_ignores_case_by_default() {
        let o = Options::default();
        assert_eq!(find("foo", o, "Foo foo FOO"), vec![0..3, 4..7, 8..11]);
        assert_eq!(find("ção", o, "ação AÇÃO"), vec![1..6, 8..13]);
        // Non-ASCII letters fold too.
        assert_eq!(find("é", o, "É é"), vec![0..2, 3..5]);
        // Regex syntax means nothing in plain mode.
        assert_eq!(find("a.b", o, "axb a.b"), vec![4..7]);
    }

    #[test]
    fn match_case() {
        let o = Options { case_sensitive: true, ..Options::default() };
        assert_eq!(find("Foo", o, "Foo foo FOO"), vec![0..3]);
    }

    #[test]
    fn whole_word() {
        let o = Options { whole_word: true, ..Options::default() };
        assert_eq!(find("foo", o, "foo food _foo foo."), vec![0..3, 14..17]);
        assert_eq!(find("->", o, "a->b a -> b"), vec![7..9]);
        assert_eq!(find("ação", o, "ações ação"), vec![8..14]);
    }

    #[test]
    fn regex_and_empty_matches() {
        let o = Options { regex: true, ..Options::default() };
        assert_eq!(find(r"\d+", o, "a1 b22"), vec![1..2, 4..6]);
        assert_eq!(find("^b", o, "a\nb\nab"), vec![2..3]);
        assert_eq!(find("x*", o, "axxb"), vec![1..3]);
        assert!(matches!(Matcher::new("(", o), Err(QueryError::Invalid(_))));
        assert_eq!(Matcher::new("", o).unwrap_err(), QueryError::Empty);
    }

    #[test]
    fn replacements() {
        let regex = Options { regex: true, ..Options::default() };
        let m = Matcher::new(r"(\w+)=(\w+)", regex).unwrap();
        assert_eq!(
            m.replacements("a=1 b=2", "$2=$1"),
            vec![(0..3, "1=a".into()), (4..7, "2=b".into())]
        );
        assert_eq!(m.replacement_at("a=1 b=2", 4..7, r"$1\n"), Some("b\n".into()));
        assert_eq!(m.replacement_at("a=1 b=2", 1..3, "x"), None);

        // Plain mode takes `$1` literally.
        let m = Matcher::new("a", Options::default()).unwrap();
        assert_eq!(m.replacements("a", "$1\\n"), vec![(0..1, "$1\\n".into())]);
    }

    #[test]
    fn hits_carry_line_column_and_preview() {
        let text = "fn main() {\r\n    let olá = 1; // olá\r\n}\r\n";
        let m = Matcher::new("olá", Options::default()).unwrap();
        let hits = to_hits(text, &m.find_all(text));
        assert_eq!(hits.len(), 2);
        assert_eq!((hits[0].line, hits[0].column, hits[0].len), (1, 8, 4));
        assert_eq!(hits[0].preview, "let olá = 1; // olá");
        assert_eq!(&hits[0].preview[hits[0].highlight.clone()], "olá");
        assert_eq!((hits[1].line, hits[1].column), (1, 20));
        assert_eq!(&hits[1].preview[hits[1].highlight.clone()], "olá");

        let long = format!("{}needle", "x".repeat(100));
        let m = Matcher::new("needle", Options::default()).unwrap();
        let h = &to_hits(&long, &m.find_all(&long))[0];
        assert!(h.preview.starts_with('…'));
        assert_eq!(&h.preview[h.highlight.clone()], "needle");
        assert_eq!(h.column, 100);
    }

    #[test]
    fn searches_files_and_open_buffers() {
        let dir = std::env::temp_dir().join(format!("bia-search-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let (a, b, bin) = (dir.join("a.txt"), dir.join("b.txt"), dir.join("c.bin"));
        fs::write(&a, "todo\nTODO\n").unwrap();
        fs::write(&b, "nothing").unwrap();
        fs::write(&bin, b"todo\0").unwrap();
        let m = Matcher::new("todo", Options::default()).unwrap();
        let files = vec![a.clone(), b.clone(), bin];

        let found = search_files(files.clone(), &m, &HashMap::new(), 100);
        assert_eq!(found.files.len(), 1);
        assert_eq!(found.hits, 2);
        assert!(!found.truncated);

        // Unsaved text wins over the disk.
        let open = HashMap::from([(b.clone(), "todo".to_owned())]);
        let found = search_files(files.clone(), &m, &open, 100);
        assert_eq!(found.hits, 3);

        let found = search_files(files, &m, &HashMap::new(), 1);
        assert_eq!(found.hits, 1);
        assert!(found.truncated);
        let _ = fs::remove_dir_all(&dir);
    }
}
