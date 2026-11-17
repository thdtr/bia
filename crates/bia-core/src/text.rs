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

#[cfg(test)]
mod tests {
    use super::*;

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
