//! Fuzzy matching for "open file by name" (Ctrl+P).
//!
//! A path matches when the query's characters appear in it in order, ignoring
//! case and spaces. Matches score higher when they fall in the file name,
//! start words (after `/`, `_`, `-`, `.` or at a capital) and run together.

/// Indices of the best `limit` matches for `query` among `paths`, best
/// first. Ties go to the shorter path.
///
/// Runs on every key in a list of up to 50,000 paths, so most paths are
/// turned down by a quick in-order check, and the ones that pass reuse the
/// same buffers instead of allocating.
pub fn rank(query: &str, paths: &[String], limit: usize) -> Vec<usize> {
    let query: Vec<char> =
        query.chars().filter(|c| !c.is_whitespace()).map(|c| c.to_ascii_lowercase()).collect();
    let mut buffers = Buffers::default();
    let mut scored: Vec<(i64, usize)> = paths
        .iter()
        .enumerate()
        .filter(|(_, path)| contains_in_order(&query, path))
        .filter_map(|(i, path)| buffers.score(&query, path).map(|s| (s, i)))
        .collect();
    let order = |a: &(i64, usize), b: &(i64, usize)| {
        b.0.cmp(&a.0).then(paths[a.1].len().cmp(&paths[b.1].len())).then(a.1.cmp(&b.1))
    };
    if scored.len() > limit && limit > 0 {
        scored.select_nth_unstable_by(limit - 1, order);
        scored.truncate(limit);
    }
    scored.sort_by(order);
    scored.into_iter().take(limit).map(|(_, i)| i).collect()
}

/// Whether the (lowercased) query's characters appear in `path` in order.
fn contains_in_order(query: &[char], path: &str) -> bool {
    let mut rest = query.iter().peekable();
    for c in path.chars() {
        match rest.peek() {
            None => return true,
            Some(&&q) if c.to_ascii_lowercase() == q => {
                rest.next();
            }
            Some(_) => {}
        }
    }
    rest.peek().is_none()
}

/// Space for one path's characters, kept from one path to the next.
#[derive(Default)]
struct Buffers {
    chars: Vec<char>,
    lower: Vec<char>,
}

impl Buffers {
    /// The score of `path` for an already lowercased `query`, or `None` if
    /// it doesn't match.
    fn score(&mut self, query: &[char], path: &str) -> Option<i64> {
        if query.is_empty() {
            return Some(0);
        }
        self.chars.clear();
        self.chars.extend(path.chars());
        self.lower.clear();
        self.lower.extend(self.chars.iter().map(char::to_ascii_lowercase));
        let (chars, lower) = (&self.chars, &self.lower);
        let name_start = chars.iter().rposition(|&c| c == '/' || c == '\\').map_or(0, |i| i + 1);
        // The whole query inside the file name beats a match spread over folders.
        let in_name = match_from(query, chars, lower, name_start).map(|s| s + 100);
        let anywhere = match_from(query, chars, lower, 0);
        in_name.max(anywhere)
    }
}

/// Matches `query` against `chars[start..]` (`lower` is the same,
/// lowercased). Each character goes to its first occurrence, or to a later
/// one that starts a word when the rest of the query still fits after it.
fn match_from(query: &[char], chars: &[char], lower: &[char], start: usize) -> Option<i64> {
    let fits = |rest: &[char], from: usize| {
        let mut it = lower.get(from..).unwrap_or_default().iter();
        rest.iter().all(|q| it.any(|c| c == q))
    };
    let mut score = 0;
    let mut at = start;
    let mut previous: Option<usize> = None;
    for (n, &q) in query.iter().enumerate() {
        let first = (at..lower.len()).find(|&i| lower[i] == q)?;
        let continues = previous.is_some_and(|p| p + 1 == first);
        let pick = if continues || is_word_start(chars, first) {
            first
        } else {
            (first..lower.len())
                .find(|&i| lower[i] == q && is_word_start(chars, i) && fits(&query[n + 1..], i + 1))
                .unwrap_or(first)
        };
        score += 1;
        if previous.is_some_and(|p| p + 1 == pick) {
            score += 5;
        }
        if is_word_start(chars, pick) {
            score += 10;
        }
        if let Some(p) = previous {
            score -= (pick - p - 1).min(3) as i64;
        }
        previous = Some(pick);
        at = pick + 1;
    }
    Some(score)
}

fn is_word_start(chars: &[char], i: usize) -> bool {
    let Some(prev) = i.checked_sub(1).map(|p| chars[p]) else { return true };
    matches!(prev, '/' | '\\' | '_' | '-' | '.' | ' ')
        || (prev.is_lowercase() && chars[i].is_uppercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn best(query: &str, paths: &[&str]) -> Vec<String> {
        let paths: Vec<String> = paths.iter().map(|p| (*p).to_owned()).collect();
        rank(query, &paths, 10).into_iter().map(|i| paths[i].clone()).collect()
    }

    #[test]
    fn matches_in_order_ignoring_case() {
        let found = best("APrs", &["src/app.rs", "README.md", "src/main.rs"]);
        assert_eq!(found, ["src/app.rs"]);
        assert!(best("xyz", &["src/app.rs"]).is_empty());
    }

    #[test]
    fn file_names_beat_folders() {
        let found = best("main", &["src/domain/lib.rs", "src/main.rs", "main/x.rs"]);
        assert_eq!(found[0], "src/main.rs");
    }

    #[test]
    fn word_starts_beat_scattered_letters() {
        let found = best("ws", &["src/ui/news.rs", "src/ui/workspace.rs"]);
        assert_eq!(found[0], "src/ui/workspace.rs");
        let found = best("dmd", &["docs/random.md", "docs/Design.md"]);
        assert_eq!(found[0], "docs/Design.md");
    }

    #[test]
    fn paths_and_spaces() {
        let found = best("core text", &["crates/bia-core/src/text.rs", "crates/bia/src/tabs.rs"]);
        assert_eq!(found, ["crates/bia-core/src/text.rs"]);
        // An empty query matches everything, shortest first.
        assert_eq!(best("", &["a/bb", "c"]), ["c", "a/bb"]);
    }
}
