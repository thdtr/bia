//! Editing rules that depend on the language but not on the editor widget:
//! comment tokens, bracket and quote pairs, and edits that touch several
//! lines at once while keeping the cursor on the same text.
//!
//! The built-in grammars don't carry comment tokens (Sublime keeps those in
//! separate metadata files that `two-face` leaves out), so they live in a
//! table here, keyed by grammar name.

/// How a language writes comments.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Comments {
    /// `//`, `#`, `--`…: preferred when there is one.
    pub line: Option<&'static str>,
    /// `/*` and `*/`, `<!--` and `-->`…: for languages with no line comment.
    pub block: Option<(&'static str, &'static str)>,
}

const C_BLOCK: (&str, &str) = ("/*", "*/");
const HTML_BLOCK: (&str, &str) = ("<!--", "-->");
const ML_BLOCK: (&str, &str) = ("(*", "*)");

/// Comment tokens for a grammar, by its name as the status bar shows it.
pub fn comments(syntax: &str) -> Option<Comments> {
    let line = |token| Some(Comments { line: Some(token), block: None });
    let both = |token, block| Some(Comments { line: Some(token), block: Some(block) });
    let block = |block| Some(Comments { line: None, block: Some(block) });
    match syntax {
        "C" | "C++" | "C#" | "D" | "Java" | "JavaScript" | "TypeScript" | "TypeScriptReact"
        | "Go" | "Rust" | "Swift" | "Kotlin" | "Scala" | "Dart" | "Groovy" | "Objective-C"
        | "Objective-C++" | "PHP" | "PHP Source" | "ActionScript" | "Less" | "SCSS" | "Stylus"
        | "Solidity" | "GLSL" | "WGSL" | "QML" | "Verilog" | "SystemVerilog" | "Graphviz (DOT)"
        | "Protocol Buffer" | "jsonnet" | "Pascal" | "JavaScript (Rails)" | "CMake C Header"
        | "CMake C++ Header" | "Typst" | "Typst Code" => both("//", C_BLOCK),
        "Zig" | "Odin" | "Gomod" => line("//"),
        "F#" => both("//", ML_BLOCK),
        "Python"
        | "Ruby"
        | "Perl"
        | "R"
        | "Bourne Again Shell (bash)"
        | "Shell-Unix-Generic"
        | "Fish"
        | "Makefile"
        | "CMake"
        | "CMakeCache"
        | "Dockerfile"
        | "Dockerfile (with bash)"
        | "YAML"
        | "TOML"
        | "DotENV"
        | "Elixir"
        | "Julia"
        | "Nim"
        | "Nix"
        | "Tcl"
        | "AWK"
        | "Crystal"
        | "GDScript (Godot Engine)"
        | "Puppet"
        | "Requirements.txt"
        | "Git Ignore"
        | "Git Attributes"
        | "Git Config"
        | "Git Commit"
        | "Apache Conf"
        | "nginx"
        | "Ninja"
        | "Crontab"
        | "fstab"
        | "Robot Framework"
        | "Terraform"
        | "Protocol Buffer (TEXT)"
        | "Vyper"
        | "Rego"
        | "JQ"
        | "SSH Config"
        | "SSHD Config"
        | "CoffeeScript"
        | "LiveScript"
        | "Java Properties"
        | "GraphQL"
        | "gnuplot"
        | "Authorized Keys"
        | "Known Hosts" => line("#"),
        "Haskell" | "Elm" | "PureScript" | "Idris" => both("--", ("{-", "-}")),
        "Lua" => both("--", ("--[[", "]]")),
        "SQL" | "SQL (Rails)" => both("--", C_BLOCK),
        "Ada" | "VHDL" | "AppleScript" | "Lean 4" | "Cabal" => line("--"),
        "Lisp" | "Clojure" | "Racket" | "INI" | "x86_64 Assembly" | "LLVM" => line(";"),
        "Erlang" | "LaTeX" | "TeX" | "MATLAB" => line("%"),
        "Fortran (Modern)" | "Fortran Namelist" => line("!"),
        "VimL" => line("\""),
        "Batch File" => line("REM"),
        "OCaml" | "SML" | "OCamllex" | "OCamlyacc" => block(ML_BLOCK),
        "CSS" => block(C_BLOCK),
        "HTML" | "XML" | "Markdown" | "MultiMarkdown" | "Vue Component" | "Svelte"
        | "MediaWiki" | "HTML (Rails)" | "HTML (EEx)" | "HTML (ASP)" => block(HTML_BLOCK),
        "Jinja2" | "HTML (Jinja2)" | "HTML (Twig)" => block(("{#", "#}")),
        _ => None,
    }
}

/// One change on one line: remove `remove` bytes at byte column `at`, then
/// insert `insert` there. Edits on the same line must not overlap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineEdit {
    /// Index into the lines the edits were computed for.
    pub line: usize,
    pub at: usize,
    pub remove: usize,
    pub insert: String,
}

impl LineEdit {
    fn new(line: usize, at: usize, remove: usize, insert: impl Into<String>) -> Self {
        Self { line, at, remove, insert: insert.into() }
    }
}

/// Applies the edits for line `index` to `line`.
pub fn apply_line(line: &str, index: usize, edits: &[LineEdit]) -> String {
    let mut mine: Vec<&LineEdit> = edits.iter().filter(|e| e.line == index).collect();
    mine.sort_by_key(|e| std::cmp::Reverse(e.at));
    let mut out = line.to_owned();
    for e in mine {
        out.replace_range(e.at..e.at + e.remove, &e.insert);
    }
    out
}

/// Where byte `column` of line `index` ends up after the edits. Text that
/// was removed collapses to where it was; text inserted at the very start
/// of a line goes after a cursor at column 0, so a selection of whole lines
/// still covers them.
pub fn shift_column(column: usize, index: usize, edits: &[LineEdit]) -> usize {
    let mut mine: Vec<&LineEdit> = edits.iter().filter(|e| e.line == index).collect();
    mine.sort_by_key(|e| e.at);
    let mut delta: isize = 0;
    for e in mine {
        if column < e.at || (column == 0 && e.at == 0 && e.remove == 0) {
            break;
        }
        if column < e.at + e.remove {
            return (e.at as isize + delta) as usize;
        }
        delta += e.insert.len() as isize - e.remove as isize;
    }
    (column as isize + delta) as usize
}

fn indent_len(line: &str) -> usize {
    line.len() - line.trim_start_matches([' ', '\t']).len()
}

fn is_blank(line: &str) -> bool {
    line.trim().is_empty()
}

/// Comments or uncomments `lines`, the way Ctrl+/ does everywhere: if every
/// non-blank line is commented, the comments come off; otherwise every
/// non-blank line gets one, lined up at the smallest indentation.
///
/// Languages with only block comments get one block around the lines.
pub fn toggle_comment(lines: &[&str], comments: Comments) -> Vec<LineEdit> {
    match (comments.line, comments.block) {
        (Some(token), _) => toggle_line_comment(lines, token),
        (None, Some(block)) => toggle_block_comment(lines, block),
        (None, None) => Vec::new(),
    }
}

fn toggle_line_comment(lines: &[&str], token: &str) -> Vec<LineEdit> {
    let filled: Vec<(usize, &str)> =
        lines.iter().copied().enumerate().filter(|(_, l)| !is_blank(l)).collect();
    if filled.is_empty() {
        return Vec::new();
    }
    let commented = filled.iter().all(|(_, l)| l.trim_start().starts_with(token));
    if commented {
        filled
            .iter()
            .map(|&(i, l)| {
                let at = indent_len(l);
                let space = l[at + token.len()..].starts_with(' ');
                LineEdit::new(i, at, token.len() + usize::from(space), "")
            })
            .collect()
    } else {
        let at = filled.iter().map(|(_, l)| indent_len(l)).min().unwrap_or(0);
        filled.iter().map(|&(i, _)| LineEdit::new(i, at, 0, format!("{token} "))).collect()
    }
}

fn toggle_block_comment(lines: &[&str], (open, close): (&str, &str)) -> Vec<LineEdit> {
    let first = lines.iter().position(|l| !is_blank(l));
    let last = lines.iter().rposition(|l| !is_blank(l));
    let (Some(first), Some(last)) = (first, last) else { return Vec::new() };
    let (head, tail) = (lines[first], lines[last]);
    let at = indent_len(head);
    let end = tail.trim_end().len();
    let wrapped = head[at..].starts_with(open)
        && tail[..end].ends_with(close)
        && (first != last || end - at >= open.len() + close.len());
    if wrapped {
        let open_len = open.len() + usize::from(head[at + open.len()..].starts_with(' '));
        let close_at = end - close.len();
        // On one line, the space after the opening token isn't also the one
        // before the closing token.
        let floor = if first == last { at + open_len } else { 0 };
        let close_start = if close_at > floor && tail[..close_at].ends_with(' ') {
            close_at - 1
        } else {
            close_at
        };
        vec![
            LineEdit::new(first, at, open_len, ""),
            LineEdit::new(last, close_start, end - close_start, ""),
        ]
    } else {
        vec![
            LineEdit::new(first, at, 0, format!("{open} ")),
            LineEdit::new(last, end, 0, format!(" {close}")),
        ]
    }
}

/// The closing half of a bracket or quote typed by itself.
pub fn closing(open: char) -> Option<char> {
    Some(match open {
        '(' => ')',
        '[' => ']',
        '{' => '}',
        '"' | '\'' | '`' => open,
        _ => return None,
    })
}

pub fn is_quote(c: char) -> bool {
    matches!(c, '"' | '\'' | '`')
}

/// Whether typing `open` between `before` and `after` should also type its
/// closing half. Brackets close unless they come right before a word;
/// quotes also stay single after a word (`don't`) and in languages where
/// that quote isn't a string delimiter (Rust lifetimes, Lisp quoting).
pub fn should_close(open: char, before: Option<char>, after: Option<char>, syntax: &str) -> bool {
    if closing(open).is_none() {
        return false;
    }
    let after_ok = after.is_none_or(|c| c.is_whitespace() || ";:.,=}])>".contains(c));
    if !after_ok {
        return false;
    }
    if !is_quote(open) {
        return true;
    }
    if before.is_some_and(|c| crate::search::is_word(c) || c == open) {
        return false;
    }
    let skip_single = matches!(
        syntax,
        "Rust"
            | "Lisp"
            | "Clojure"
            | "Racket"
            | "OCaml"
            | "SML"
            | "Haskell"
            | "Elm"
            | "PureScript"
            | "Idris"
            | "Lean 4"
            | "Plain Text"
            | "Markdown"
            | "MultiMarkdown"
    );
    let skip_double = matches!(syntax, "VimL");
    !(open == '\'' && skip_single || open == '"' && skip_double)
}

/// Words in comments that mark work left to do, highlighted so they stand
/// out from the comment around them.
pub const MARKERS: [&str; 4] = ["TODO", "FIXME", "HACK", "XXX"];

/// Where `line` has a [`MARKERS`] word inside a comment, as byte ranges. A
/// comment starts at the line token, at the block opener, or at a leading
/// `*` (the middle of a `/* … */` block); a marker before that is code.
pub fn markers(line: &str, comments: Option<Comments>) -> Vec<std::ops::Range<usize>> {
    let Some(comments) = comments else { return Vec::new() };
    let trimmed = line.trim_start();
    let starts = [comments.line, comments.block.map(|b| b.0)]
        .into_iter()
        .flatten()
        .filter_map(|token| line.find(token))
        .chain(trimmed.starts_with('*').then_some(line.len() - trimmed.len()))
        .min();
    let Some(start) = starts else { return Vec::new() };
    let word = |c: char| c.is_alphanumeric() || c == '_';
    let mut found = Vec::new();
    for marker in MARKERS {
        for (i, _) in line[start..].match_indices(marker) {
            let at = start + i;
            let before = line[..at].chars().next_back();
            let after = line[at + marker.len()..].chars().next();
            if !before.is_some_and(word) && !after.is_some_and(word) {
                found.push(at..at + marker.len());
            }
        }
    }
    found.sort_by_key(|r| r.start);
    found
}

/// The spaces and tabs at the end of each line of `text`, as byte
/// ranges, for "trim trailing whitespace on save". Line breaks stay.
pub fn trailing_whitespace(text: &str) -> Vec<std::ops::Range<usize>> {
    let mut ranges = Vec::new();
    let mut start = 0;
    for line in text.split_inclusive('\n') {
        let body = line.trim_end_matches(['\n', '\r']);
        let kept = body.trim_end_matches([' ', '\t']).len();
        if kept < body.len() {
            ranges.push(start + kept..start + body.len());
        }
        start += line.len();
    }
    ranges
}

/// Whether `text` needs a line break added at its end. An empty file
/// doesn't, and neither does one that already ends with one or more.
pub fn needs_final_newline(text: &str) -> bool {
    !text.is_empty() && !text.ends_with('\n')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_cleanups() {
        assert_eq!(trailing_whitespace("a  \nb\t\r\nc\n  "), [1..3, 5..6, 10..12]);
        assert!(trailing_whitespace("clean\n").is_empty());
        assert!(needs_final_newline("x"));
        assert!(!needs_final_newline("x\n\n\n"));
        assert!(!needs_final_newline(""));
    }

    #[test]
    fn markers_only_in_comments() {
        let rust = comments("Rust");
        let one = |line: &str, c| {
            markers(line, c).into_iter().map(|r| (r.start, r.end)).collect::<Vec<_>>()
        };
        assert_eq!(one("let x = 1; // TODO: less", rust), [(14, 18)]);
        assert_eq!(markers("// FIXME and XXX", rust), [3..8, 13..16]);
        assert_eq!(one(" * HACK", rust), [(3, 7)]);
        assert!(markers("let TODO = 1;", rust).is_empty());
        assert!(markers("// TODOS", rust).is_empty());
        assert_eq!(one("# TODO", comments("Python")), [(2, 6)]);
        assert!(markers("// TODO", None).is_empty());
    }

    fn apply(lines: &[&str], edits: &[LineEdit]) -> Vec<String> {
        lines.iter().enumerate().map(|(i, l)| apply_line(l, i, edits)).collect()
    }

    const SLASHES: Comments = Comments { line: Some("//"), block: Some(C_BLOCK) };
    const CSS: Comments = Comments { line: None, block: Some(C_BLOCK) };

    #[test]
    fn line_comments_line_up_and_toggle_back() {
        let lines = ["    a();", "", "  b();"];
        let edits = toggle_comment(&lines, SLASHES);
        let done = apply(&lines, &edits);
        assert_eq!(done, ["  //   a();", "", "  // b();"]);
        let back: Vec<&str> = done.iter().map(String::as_str).collect();
        let edits = toggle_comment(&back, SLASHES);
        assert_eq!(apply(&back, &edits), ["    a();", "", "  b();"]);
    }

    #[test]
    fn mixed_lines_get_commented() {
        let lines = ["// a", "b"];
        assert_eq!(apply(&lines, &toggle_comment(&lines, SLASHES)), ["// // a", "// b"]);
        // No space after the token is fine too.
        let lines = ["//a"];
        assert_eq!(apply(&lines, &toggle_comment(&lines, SLASHES)), ["a"]);
        assert!(toggle_comment(&["", "  "], SLASHES).is_empty());
    }

    #[test]
    fn block_comments() {
        let lines = ["  a {", "  }"];
        let done = apply(&lines, &toggle_comment(&lines, CSS));
        assert_eq!(done, ["  /* a {", "  } */"]);
        let back: Vec<&str> = done.iter().map(String::as_str).collect();
        assert_eq!(apply(&back, &toggle_comment(&back, CSS)), ["  a {", "  }"]);

        let one = ["x: 1;"];
        let done = apply(&one, &toggle_comment(&one, CSS));
        assert_eq!(done, ["/* x: 1; */"]);
        let back = [done[0].as_str()];
        assert_eq!(apply(&back, &toggle_comment(&back, CSS)), ["x: 1;"]);
        assert_eq!(apply(&["/**/"], &toggle_comment(&["/**/"], CSS)), [""]);
    }

    #[test]
    fn columns_follow_the_text() {
        let edits = vec![LineEdit::new(0, 2, 0, "// ")];
        assert_eq!(shift_column(0, 0, &edits), 0);
        assert_eq!(shift_column(2, 0, &edits), 5);
        assert_eq!(shift_column(4, 0, &edits), 7);
        assert_eq!(shift_column(4, 1, &edits), 4);
        // Inserted at column 0: a cursor there stays before it.
        let edits = vec![LineEdit::new(0, 0, 0, "    ")];
        assert_eq!(shift_column(0, 0, &edits), 0);
        assert_eq!(shift_column(1, 0, &edits), 5);
        // Removed text collapses.
        let edits = vec![LineEdit::new(0, 2, 3, "")];
        assert_eq!(shift_column(3, 0, &edits), 2);
        assert_eq!(shift_column(6, 0, &edits), 3);
        // Two edits on one line.
        let edits = vec![LineEdit::new(0, 0, 0, "/* "), LineEdit::new(0, 4, 0, " */")];
        assert_eq!(shift_column(2, 0, &edits), 5);
        assert_eq!(shift_column(4, 0, &edits), 10);
    }

    #[test]
    fn pairs() {
        assert!(should_close('(', None, None, "Rust"));
        assert!(should_close('(', Some('f'), Some(')'), "Rust"));
        assert!(!should_close('(', None, Some('x'), "Rust"));
        assert!(should_close('"', Some(' '), None, "Rust"));
        assert!(!should_close('\'', Some(' '), None, "Rust"));
        assert!(should_close('\'', Some(' '), None, "Python"));
        assert!(!should_close('\'', Some('n'), None, "Python"));
        assert!(!should_close('a', None, None, "Python"));
    }

    #[test]
    fn known_grammars_have_comments() {
        for name in ["Rust", "Python", "CSS", "HTML", "SQL", "YAML", "Cabal", "OCaml"] {
            assert!(comments(name).is_some(), "{name}");
        }
        assert_eq!(comments("Cabal").unwrap().line, Some("--"));
        assert_eq!(comments("Plain Text"), None);
    }
}
