//! Small helpers over text that the editor model doesn't cover.

/// Splits a `:line` or `:line:column` suffix (1-based, as compilers print
/// them) off `s`: `src/main.rs:42:7` gives `("src/main.rs", Some((42, Some(7))))`.
pub fn split_location(s: &str) -> (&str, Option<(usize, Option<usize>)>) {
    let number =
        |t: &str| t.parse::<usize>().ok().filter(|_| t.bytes().all(|b| b.is_ascii_digit()));
    let Some((rest, last)) = s.rsplit_once(':') else { return (s, None) };
    let Some(last) = number(last) else { return (s, None) };
    if let Some((path, line)) = rest.rsplit_once(':')
        && let Some(line) = number(line)
    {
        return (path, Some((line, Some(last))));
    }
    (rest, Some((last, None)))
}

/// The word of `line` around character `at` when it looks like a file
/// location, as the terminal shows them: `src/main.rs:42:7`, `--> a.rs:3`,
/// `src/app.ts(12,5)`, `./notes.md`. Gives the range of characters to
/// underline and the location written as `path:line:column`.
pub fn location_at(line: &[char], at: usize) -> Option<(std::ops::Range<usize>, String)> {
    let breaks = |c: char| c.is_whitespace() || matches!(c, '"' | '\'' | '`' | '<' | '>' | '|');
    if at >= line.len() || breaks(line[at]) {
        return None;
    }
    let mut start = at;
    while start > 0 && !breaks(line[start - 1]) {
        start -= 1;
    }
    let mut end = at + 1;
    while end < line.len() && !breaks(line[end]) {
        end += 1;
    }
    // Punctuation around the word belongs to the sentence, not the path.
    while start < end && matches!(line[start], '(' | '[' | '{' | ',' | ';') {
        start += 1;
    }
    let word: String = line[start..end].iter().collect();
    let mut word = word.as_str();
    // `file.ts(12,5)` is how tsc and MSBuild print a place in a file.
    let mut paren = None;
    if let Some(open) = word.find('(')
        && let Some(close) = word[open..].find(')').map(|i| open + i)
    {
        let inside = &word[open + 1..close];
        let mut parts = inside.split(',').map(|n| n.trim().parse::<usize>());
        if let Some(Ok(line_no)) = parts.next() {
            let column = parts.next().and_then(Result::ok);
            paren = Some((line_no, column));
            word = &word[..close + 1];
        }
    }
    let trimmed = word.trim_end_matches(['.', ',', ';', ':', ')', ']', '}']);
    let trimmed = if paren.is_some() { word } else { trimmed };
    if trimmed.is_empty() {
        return None;
    }
    let end = start + trimmed.chars().count();
    let (path, place) = match paren {
        Some(place) => (&trimmed[..trimmed.find('(').unwrap_or(trimmed.len())], Some(place)),
        None => split_location(trimmed),
    };
    // A bare word ("error", "42") is not a path. A path has a folder or an
    // extension, and isn't a URL.
    let looks_like_path = (path.contains('/') || path.contains('\\') || path.contains('.'))
        && !path.contains("://")
        && path.chars().any(char::is_alphanumeric);
    if !looks_like_path {
        return None;
    }
    let location = match place {
        Some((line, Some(column))) => format!("{path}:{line}:{column}"),
        Some((line, None)) => format!("{path}:{line}"),
        None => path.to_owned(),
    };
    Some((start..end, location))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn location(line: &str, at: usize) -> Option<(std::ops::Range<usize>, String)> {
        location_at(&line.chars().collect::<Vec<_>>(), at)
    }

    #[test]
    fn locations_in_terminal_lines() {
        let rustc = "  --> crates/bia/src/app.rs:42:5";
        assert_eq!(location(rustc, 10), Some((6..32, "crates/bia/src/app.rs:42:5".into())));
        assert_eq!(location(rustc, 3), None);
        let tsc = "src/app.ts(12,5): error TS2304";
        assert_eq!(location(tsc, 0), Some((0..16, "src/app.ts:12:5".into())));
        assert_eq!(location("see ./notes.md.", 6), Some((4..14, "./notes.md".into())));
        assert_eq!(location("error: expected", 2), None);
        assert_eq!(location("open https://example.com/a.html", 10), None);
        assert_eq!(location("at (lib/x.js:3:9)", 6), Some((4..16, "lib/x.js:3:9".into())));
        assert_eq!(location("", 0), None);
    }

    #[test]
    fn locations() {
        assert_eq!(split_location("src/main.rs:42:7"), ("src/main.rs", Some((42, Some(7)))));
        assert_eq!(split_location("main.rs:42"), ("main.rs", Some((42, None))));
        assert_eq!(split_location("main.rs:"), ("main.rs:", None));
        assert_eq!(split_location("main.rs"), ("main.rs", None));
        assert_eq!(split_location(":12"), ("", Some((12, None))));
        assert_eq!(split_location("C:\\x\\a.rs:3"), ("C:\\x\\a.rs", Some((3, None))));
        assert_eq!(split_location("a:+3"), ("a:+3", None));
    }
}
