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

/// The files a clipboard names, one per line, either as `file://`
/// addresses, the way file managers copy them, or as absolute paths. Lines
/// that are neither, like GNOME's `copy` header, are skipped.
pub fn clipboard_paths(text: &str) -> Vec<std::path::PathBuf> {
    text.lines()
        .map(str::trim)
        .filter_map(|line| {
            let path = match line.strip_prefix("file://") {
                Some(rest) => {
                    // A host may come first; only this machine's makes sense.
                    let rest = rest.strip_prefix("localhost").unwrap_or(rest);
                    let decoded = percent_decode(rest)?;
                    // `file:///C:/x` on Windows.
                    if cfg!(windows) { decoded.trim_start_matches('/').to_owned() } else { decoded }
                }
                None => line.to_owned(),
            };
            let path = std::path::PathBuf::from(path);
            path.is_absolute().then_some(path)
        })
        .collect()
}

fn percent_decode(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = s.get(i + 1..i + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// `path` as a `file://` address, for programs that take one.
pub fn file_uri(path: &std::path::Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    let mut uri = String::from(if text.starts_with('/') { "file://" } else { "file:///" });
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' | b':' => {
                uri.push(byte as char);
            }
            _ => uri.push_str(&format!("%{byte:02X}")),
        }
    }
    uri
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// The word of `line` that byte `at` touches (a cursor right after a word
/// counts), if any.
pub fn word_at(line: &str, at: usize) -> Option<std::ops::Range<usize>> {
    let mut at = at.min(line.len());
    while !line.is_char_boundary(at) {
        at -= 1;
    }
    let start = line[..at]
        .char_indices()
        .rev()
        .take_while(|(_, c)| is_word(*c))
        .last()
        .map_or(at, |(i, _)| i);
    let end =
        line[at..].char_indices().find(|(_, c)| !is_word(*c)).map_or(line.len(), |(i, _)| at + i);
    (start < end).then_some(start..end)
}

/// Every place `word` appears in `line` as a whole word.
pub fn word_places(line: &str, word: &str) -> Vec<std::ops::Range<usize>> {
    if word.is_empty() {
        return Vec::new();
    }
    line.match_indices(word)
        .filter(|(i, _)| {
            let before = line[..*i].chars().next_back();
            let after = line[i + word.len()..].chars().next();
            !before.is_some_and(is_word) && !after.is_some_and(is_word)
        })
        .map(|(i, _)| i..i + word.len())
        .collect()
}

/// `*` (not across `/`), `**` (across), `?`, `{a,b}` and `[abc]`.
pub fn glob(pattern: &str, text: &str) -> bool {
    // `{a,b}` becomes one pattern per choice.
    if let Some(open) = pattern.find('{')
        && let Some(close) = pattern[open..].find('}').map(|i| open + i)
    {
        let (head, tail) = (&pattern[..open], &pattern[close + 1..]);
        return pattern[open + 1..close]
            .split(',')
            .any(|choice| glob(&format!("{head}{choice}{tail}"), text));
    }
    matches(pattern.as_bytes(), text.as_bytes())
}

fn matches(p: &[u8], t: &[u8]) -> bool {
    match p.first() {
        None => t.is_empty(),
        Some(b'*') if p.get(1) == Some(&b'*') => {
            let rest = &p[2..];
            (0..=t.len()).any(|i| matches(rest, &t[i..]))
        }
        Some(b'*') => {
            let rest = &p[1..];
            (0..=t.len())
                .take_while(|&i| i == 0 || t[i - 1] != b'/')
                .any(|i| matches(rest, &t[i..]))
        }
        Some(b'?') => !t.is_empty() && t[0] != b'/' && matches(&p[1..], &t[1..]),
        Some(b'[') => {
            let Some(close) = p.iter().position(|&c| c == b']') else {
                return t.first() == Some(&b'[') && matches(&p[1..], &t[1..]);
            };
            let set = &p[1..close];
            !t.is_empty() && set.contains(&t[0]) && matches(&p[close + 1..], &t[1..])
        }
        Some(&c) => t.first() == Some(&c) && matches(&p[1..], &t[1..]),
    }
}

/// Which files a search looks in, as patterns separated by commas, like
/// `src, *.rs` or `*.lock, tests`. A pattern matches a file's path from the
/// open folder, its name, or any folder it's in. No `include` means every
/// file; anything `exclude` matches is left out.
pub fn path_filter(include: &str, exclude: &str) -> impl Fn(&str) -> bool + Send + 'static {
    let split = |list: &str| -> Vec<String> {
        list.split(',')
            .map(|p| p.trim().trim_matches('/').to_owned())
            .filter(|p| !p.is_empty())
            .collect()
    };
    let (include, exclude) = (split(include), split(exclude));
    move |relative: &str| {
        let hits = |pattern: &String| {
            glob(pattern, relative)
                || relative.split('/').any(|part| glob(pattern, part))
                || relative.starts_with(&format!("{pattern}/"))
        };
        (include.is_empty() || include.iter().any(hits)) && !exclude.iter().any(hits)
    }
}

/// The anchor GitHub gives a heading, which is what `[link](#anchor)`
/// points to. It's lower case, spaces become `-`, and punctuation goes
/// away, while letters with accents stay ("Configurações" →
/// `configurações`).
pub fn heading_anchor(title: &str) -> String {
    title
        .trim()
        .to_lowercase()
        .chars()
        .filter_map(|c| match c {
            ' ' => Some('-'),
            '-' | '_' => Some(c),
            c if c.is_alphanumeric() => Some(c),
            _ => None,
        })
        .collect()
}

/// A link's `#anchor`, without the `#` and with `%C3%A7` and the like
/// turned back into letters.
pub fn link_anchor(link: &str) -> Option<String> {
    let anchor = link.strip_prefix('#')?;
    Some(percent_decode(anchor).unwrap_or_else(|| anchor.to_owned()).to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heading_anchors() {
        assert_eq!(heading_anchor("Find in files"), "find-in-files");
        assert_eq!(heading_anchor("Configurações"), "configurações");
        assert_eq!(heading_anchor("What's new? (0.5)"), "whats-new-05");
        assert_eq!(heading_anchor(".editorconfig"), "editorconfig");
        assert_eq!(link_anchor("#configura%C3%A7%C3%B5es").as_deref(), Some("configurações"));
        assert_eq!(link_anchor("#Install").as_deref(), Some("install"));
        assert_eq!(link_anchor("notes.md"), None);
    }

    #[test]
    fn search_filters() {
        let rust_in_src = path_filter("src, *.toml", "*_test.rs");
        assert!(rust_in_src("src/main.rs"));
        assert!(rust_in_src("crates/x/src/lib.rs"));
        assert!(rust_in_src("Cargo.toml"));
        assert!(!rust_in_src("README.md"));
        assert!(!rust_in_src("src/parse_test.rs"));
        let everything_but_locks = path_filter("", "*.lock, target");
        assert!(everything_but_locks("a/b.rs"));
        assert!(!everything_but_locks("Cargo.lock"));
        assert!(!everything_but_locks("target/debug/x"));
        assert!(path_filter("docs/**.md", "")("docs/a/b.md"));
    }

    #[test]
    fn words_and_their_places() {
        let line = "let total = total_tax + total;";
        assert_eq!(word_at(line, 6), Some(4..9));
        assert_eq!(word_at(line, 9), Some(4..9));
        assert_eq!(word_at(line, 10), None);
        assert_eq!(word_places(line, "total"), [4..9, 24..29]);
        assert!(word_places(line, "").is_empty());
        assert_eq!(word_at("ação x", 2), Some(0..6));
    }

    #[cfg(unix)]
    #[test]
    fn file_lists_from_the_clipboard() {
        use std::path::PathBuf;
        let gnome = "copy\nfile:///home/me/My%20Notes.md\nfile:///tmp/a.rs";
        assert_eq!(
            clipboard_paths(gnome),
            [PathBuf::from("/home/me/My Notes.md"), PathBuf::from("/tmp/a.rs")]
        );
        assert_eq!(clipboard_paths("/etc/hosts"), [PathBuf::from("/etc/hosts")]);
        assert!(clipboard_paths("just some text").is_empty());
        assert_eq!(file_uri(std::path::Path::new("/a b/ç.md")), "file:///a%20b/%C3%A7.md");
    }

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
