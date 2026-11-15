//! Languages: which grammar highlights which file.
//!
//! BIA ships ~200 grammars (the set curated by the `bat` project) compiled
//! into the binary. Adding a language needs **no rebuild**: drop a Sublime
//! Text `.sublime-syntax` file into the `languages/` config folder and it is
//! picked up on the next start (or when settings are re-applied).
//! User grammars take precedence over built-in ones for the same extension.

use crate::theme::Theme;
use std::fs;
use std::path::Path;
use std::str::FromStr;
use syntect::highlighting::{
    Color, FontStyle, ScopeSelectors, StyleModifier, Theme as SyntaxTheme, ThemeItem, ThemeSettings,
};
use syntect::parsing::{SyntaxDefinition, SyntaxReference, SyntaxSet};

pub use syntect;

/// Builds the grammar set: built-ins plus every `*.sublime-syntax` in `user_dir`.
///
/// Broken user grammars are skipped and reported, never fatal.
pub fn load_syntaxes(user_dir: &Path) -> (SyntaxSet, Vec<String>) {
    let built_in = two_face::syntax::extra_no_newlines();
    let mut files: Vec<_> = fs::read_dir(user_dir)
        .map(|entries| {
            entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|e| e == "sublime-syntax"))
                .collect()
        })
        .unwrap_or_default();
    if files.is_empty() {
        return (built_in, Vec::new());
    }
    files.sort();

    let mut errors = Vec::new();
    let mut builder = built_in.into_builder();
    for file in files {
        let name = file.file_stem().map(|s| s.to_string_lossy().into_owned());
        let parsed = fs::read_to_string(&file).map_err(|e| e.to_string()).and_then(|src| {
            SyntaxDefinition::load_from_str(&src, false, name.as_deref()).map_err(|e| e.to_string())
        });
        match parsed {
            Ok(def) => builder.add(def),
            Err(e) => errors.push(format!("{}: {e}", file.display())),
        }
    }
    (builder.build(), errors)
}

/// Picks the grammar for `path`: exact file name (`Makefile`, `Dockerfile`),
/// then extension, then the first line (shebangs, `<?xml`, modelines).
pub fn detect<'a>(set: &'a SyntaxSet, path: &Path, first_line: &str) -> &'a SyntaxReference {
    let file_name = path.file_name().and_then(|n| n.to_str());
    let extension = path.extension().and_then(|e| e.to_str());
    file_name
        .and_then(|n| set.find_syntax_by_extension(n))
        .or_else(|| extension.and_then(|e| set.find_syntax_by_extension(e)))
        .or_else(|| set.find_syntax_by_first_line(first_line))
        .unwrap_or_else(|| set.find_syntax_plain_text())
}

pub fn is_markdown(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| {
        matches!(e.to_ascii_lowercase().as_str(), "md" | "markdown" | "mdown" | "mkd")
    })
}

/// Translates a BIA theme into the form the syntax highlighter understands.
pub fn syntax_theme(theme: &Theme) -> SyntaxTheme {
    let color = |c: crate::Rgba| Color { r: c.r, g: c.g, b: c.b, a: c.a };
    let scopes = theme
        .syntax
        .iter()
        .filter_map(|(selector, style)| {
            let scope = ScopeSelectors::from_str(selector).ok()?;
            let mut font_style = FontStyle::empty();
            if style.bold() {
                font_style.insert(FontStyle::BOLD);
            }
            if style.italic() {
                font_style.insert(FontStyle::ITALIC);
            }
            Some(ThemeItem {
                scope,
                style: StyleModifier {
                    foreground: style.color().map(color),
                    background: None,
                    font_style: (!font_style.is_empty()).then_some(font_style),
                },
            })
        })
        .collect();

    SyntaxTheme {
        name: Some(theme.name.clone()),
        author: None,
        settings: ThemeSettings {
            foreground: Some(color(theme.ui.text)),
            background: Some(color(theme.ui.background)),
            ..ThemeSettings::default()
        },
        scopes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn detects_common_files() {
        let (set, errors) = load_syntaxes(Path::new("/nonexistent"));
        assert!(errors.is_empty());
        let name = |p: &str, first: &str| detect(&set, &PathBuf::from(p), first).name.clone();
        assert_eq!(name("main.rs", ""), "Rust");
        assert_eq!(name("README.md", ""), "Markdown");
        assert_eq!(name("Cargo.toml", ""), "TOML");
        assert_eq!(name("script", "#!/usr/bin/env python3"), "Python");
        assert_eq!(name("notes.unknownext", ""), "Plain Text");
    }

    #[test]
    fn user_grammar_is_loaded() {
        let dir = std::env::temp_dir().join(format!("bia-lang-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("bialang.sublime-syntax"),
            "%YAML 1.2\n---\nname: BiaLang\nfile_extensions: [bia]\nscope: source.bia\n\
             contexts:\n  main:\n    - match: '\\bbia\\b'\n      scope: keyword.bia\n",
        )
        .unwrap();
        fs::write(dir.join("broken.sublime-syntax"), "this is: [not valid").unwrap();
        let (set, errors) = load_syntaxes(&dir);
        assert_eq!(detect(&set, Path::new("x.bia"), "").name, "BiaLang");
        assert_eq!(errors.len(), 1, "broken grammar reported, not fatal");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn theme_translates() {
        let t = syntax_theme(&Theme::fallback());
        assert!(!t.scopes.is_empty());
    }

    #[test]
    fn markdown_detection() {
        assert!(is_markdown(Path::new("README.MD")));
        assert!(!is_markdown(Path::new("main.rs")));
    }
}
