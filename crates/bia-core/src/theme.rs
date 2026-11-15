//! Themes are small JSON files: nine interface colors plus syntax colors
//! keyed by TextMate scope selectors.
//!
//! Built-in themes live in `assets/themes/` and are compiled into the binary.
//! Users add their own by dropping `<id>.json` into the `themes/` config
//! folder; a user theme with the same id as a built-in one replaces it.

use crate::color::Rgba;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

pub const DEFAULT_ID: &str = "noite";

/// `(id, json)` for every theme compiled into BIA.
pub const BUILT_IN: &[(&str, &str)] = &[
    ("noite", include_str!("../../../assets/themes/noite.json")),
    ("dia", include_str!("../../../assets/themes/dia.json")),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    #[default]
    Dark,
    Light,
}

/// Interface colors. Every theme must define all of them.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct Ui {
    /// Editor background.
    pub background: Rgba,
    /// Sidebar, tab strip and status bar.
    pub panel: Rgba,
    /// Hairlines between areas.
    pub border: Rgba,
    /// Primary text.
    pub text: Rgba,
    /// Secondary text: hints, inactive tabs, explorer glyphs.
    pub muted: Rgba,
    /// The accent color, used only to show where you are.
    pub accent: Rgba,
    /// Text selection.
    pub selection: Rgba,
    /// Hovered and selected rows.
    pub hover: Rgba,
    /// Errors and destructive actions.
    pub danger: Rgba,
}

/// A syntax color, either `"#rrggbb"` or `{ "color": "#rrggbb", "italic": true }`.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(untagged)]
pub enum TokenStyle {
    Color(Rgba),
    Styled {
        #[serde(default)]
        color: Option<Rgba>,
        #[serde(default)]
        bold: bool,
        #[serde(default)]
        italic: bool,
    },
}

impl TokenStyle {
    pub fn color(&self) -> Option<Rgba> {
        match *self {
            Self::Color(c) => Some(c),
            Self::Styled { color, .. } => color,
        }
    }

    pub fn bold(&self) -> bool {
        matches!(self, Self::Styled { bold: true, .. })
    }

    pub fn italic(&self) -> bool {
        matches!(self, Self::Styled { italic: true, .. })
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Theme {
    /// File name without extension. Filled in by the loader.
    #[serde(skip)]
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub kind: Kind,
    pub ui: Ui,
    /// Scope selector (e.g. `"comment"`, `"entity.name.function"`) → style.
    #[serde(default)]
    pub syntax: BTreeMap<String, TokenStyle>,
}

impl Theme {
    pub fn from_json(id: &str, json: &str) -> Result<Self, serde_json::Error> {
        let mut theme: Theme = serde_json::from_str(json)?;
        theme.id = id.to_owned();
        Ok(theme)
    }

    /// The default theme. Built-in themes are covered by tests, so this
    /// cannot fail at runtime.
    pub fn fallback() -> Self {
        Self::from_json(DEFAULT_ID, BUILT_IN[0].1).expect("built-in theme is valid")
    }

    /// Loads theme `id`, preferring a user theme over a built-in one.
    ///
    /// Never fails: on any problem it returns the default theme and a
    /// human-readable error to show in the status bar.
    pub fn load(id: &str, user_dir: &Path) -> (Self, Option<String>) {
        let id = id.trim().to_lowercase();
        let user_file = user_dir.join(format!("{id}.json"));
        if let Ok(json) = fs::read_to_string(&user_file) {
            return match Self::from_json(&id, &json) {
                Ok(t) => (t, None),
                Err(e) => (Self::fallback(), Some(format!("{}: {e}", user_file.display()))),
            };
        }
        match BUILT_IN.iter().find(|(bid, _)| *bid == id) {
            Some((bid, json)) => (Self::from_json(bid, json).expect("built-in theme"), None),
            None => (Self::fallback(), Some(format!("theme `{id}` not found"))),
        }
    }

    /// Ids of every theme the user can pick, built-in first.
    pub fn available(user_dir: &Path) -> Vec<String> {
        let mut ids: Vec<String> = BUILT_IN.iter().map(|(id, _)| (*id).to_owned()).collect();
        if let Ok(entries) = fs::read_dir(user_dir) {
            let mut user: Vec<String> = entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|e| e == "json"))
                .filter_map(|p| p.file_stem().map(|s| s.to_string_lossy().to_lowercase()))
                .filter(|id| !ids.contains(id))
                .collect();
            user.sort();
            ids.extend(user);
        }
        ids
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_themes_parse() {
        for (id, json) in BUILT_IN {
            let t = Theme::from_json(id, json).unwrap_or_else(|e| panic!("{id}: {e}"));
            assert!(!t.syntax.is_empty(), "{id} has no syntax colors");
        }
    }

    #[test]
    fn token_style_forms() {
        let t: BTreeMap<String, TokenStyle> =
            serde_json::from_str(r##"{ "a": "#fff", "b": { "color": "#000", "italic": true } }"##)
                .unwrap();
        assert_eq!(t["a"].color(), Some(Rgba::rgb(255, 255, 255)));
        assert!(t["b"].italic() && !t["b"].bold());
    }

    #[test]
    fn unknown_theme_falls_back() {
        let (t, err) = Theme::load("does-not-exist", Path::new("/nonexistent"));
        assert_eq!(t.id, DEFAULT_ID);
        assert!(err.is_some());
    }
}
