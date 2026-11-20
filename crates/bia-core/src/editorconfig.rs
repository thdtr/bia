//! `.editorconfig`: how a project asks every editor to indent and end its
//! files (<https://editorconfig.org>).
//!
//! BIA reads the files from the file's folder up, stopping at one that says
//! `root = true`, and applies the sections whose pattern matches the file.
//! A closer file wins over a farther one, and a later section over an
//! earlier one. BIA uses the keys it has a use for:
//!
//! | Key | What BIA does |
//! |---|---|
//! | `indent_style` | `tab` or `space` for new indentation |
//! | `indent_size` | the width of one level (`tab` means `tab_width`) |
//! | `tab_width` | how wide a tab looks |
//! | `trim_trailing_whitespace` | trims the end of every line on save |
//! | `insert_final_newline` | adds a line break at the end on save |

use std::fs;
use std::path::Path;

/// What the `.editorconfig` files say about one file. `None`: they don't.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Props {
    pub tabs: Option<bool>,
    pub indent_size: Option<usize>,
    pub tab_width: Option<usize>,
    pub trim_trailing_whitespace: Option<bool>,
    pub insert_final_newline: Option<bool>,
}

/// Reads the `.editorconfig` files that apply to `path`.
pub fn for_file(path: &Path) -> Props {
    let mut files = Vec::new();
    let mut dir = path.parent();
    while let Some(d) = dir {
        if let Ok(text) = fs::read_to_string(d.join(".editorconfig")) {
            let root = is_root(&text);
            files.push((d.to_path_buf(), text));
            if root {
                break;
            }
        }
        dir = d.parent();
    }
    let mut props = Props::default();
    // Farthest first, so closer files overwrite what they set.
    for (dir, text) in files.iter().rev() {
        let Ok(relative) = path.strip_prefix(dir) else { continue };
        let relative = relative.to_string_lossy().replace('\\', "/");
        apply(text, &relative, &mut props);
    }
    props
}

fn is_root(text: &str) -> bool {
    text.lines()
        .map(str::trim)
        .take_while(|l| !l.starts_with('['))
        .filter_map(|l| l.split_once('='))
        .any(|(k, v)| {
            k.trim().eq_ignore_ascii_case("root") && v.trim().eq_ignore_ascii_case("true")
        })
}

/// Applies the sections of one file's `text` that match `relative` (the
/// file's path from that `.editorconfig`'s folder, with `/`).
fn apply(text: &str, relative: &str, props: &mut Props) {
    let mut matching = false;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if let Some(section) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            matching = section_matches(section, relative);
            continue;
        }
        if !matching {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else { continue };
        let (key, value) = (key.trim().to_ascii_lowercase(), value.trim().to_ascii_lowercase());
        let flag = match value.as_str() {
            "true" => Some(true),
            "false" => Some(false),
            _ => None,
        };
        let number = value.parse::<usize>().ok().filter(|n| (1..=16).contains(n));
        match key.as_str() {
            "indent_style" => {
                props.tabs =
                    (value == "tab").then_some(true).or((value == "space").then_some(false))
            }
            "indent_size" if value == "tab" => props.indent_size = None,
            "indent_size" => props.indent_size = number,
            "tab_width" => props.tab_width = number,
            "trim_trailing_whitespace" => props.trim_trailing_whitespace = flag,
            "insert_final_newline" => props.insert_final_newline = flag,
            _ => {}
        }
    }
}

/// A section's pattern against a path. Without a `/`, the pattern matches
/// the file's name in any folder; with one, the path from the
/// `.editorconfig`'s folder.
fn section_matches(pattern: &str, relative: &str) -> bool {
    let pattern = pattern.trim();
    if pattern.contains('/') {
        crate::text::glob(pattern.trim_start_matches('/'), relative)
    } else {
        let name = relative.rsplit('/').next().unwrap_or(relative);
        crate::text::glob(pattern, name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::glob;

    #[test]
    fn patterns() {
        assert!(glob("*", "main.rs"));
        assert!(glob("*.{js,ts}", "app.ts"));
        assert!(!glob("*.{js,ts}", "app.rs"));
        assert!(glob("Makefile", "Makefile"));
        assert!(glob("lib/**.js", "lib/a/b/c.js"));
        assert!(!glob("lib/*.js", "lib/a/c.js"));
        assert!(glob("[Mm]akefile", "makefile"));
        assert!(section_matches("*.py", "src/tools/run.py"));
    }

    #[test]
    fn closer_files_and_later_sections_win() {
        let dir = std::env::temp_dir().join(format!("bia-editorconfig-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("web")).unwrap();
        fs::write(
            dir.join(".editorconfig"),
            "root = true\n\n[*]\nindent_style = space\nindent_size = 4\ninsert_final_newline = true\n\n[Makefile]\nindent_style = tab\n",
        )
        .unwrap();
        fs::write(dir.join("web").join(".editorconfig"), "[*.js]\nindent_size = 2\n").unwrap();
        let js = for_file(&dir.join("web").join("app.js"));
        assert_eq!(js.tabs, Some(false));
        assert_eq!(js.indent_size, Some(2));
        assert_eq!(js.insert_final_newline, Some(true));
        let make = for_file(&dir.join("Makefile"));
        assert_eq!(make.tabs, Some(true));
        assert_eq!(for_file(&dir.join("README.md")).indent_size, Some(4));
        let _ = fs::remove_dir_all(&dir);
    }
}
