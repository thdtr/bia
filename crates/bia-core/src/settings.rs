//! `settings.json`: the only configuration file BIA has.
//!
//! Every key is optional. Unknown keys are ignored, so old and new BIA
//! versions can share one file. The file is re-read whenever it is saved
//! from inside BIA; there is no settings screen.

use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Font used by the editor. BIA ships Fira Code; `"monospace"` means the
    /// system's monospace font, and any installed font name works.
    pub font_family: String,
    /// Editor font size, in logical pixels.
    pub font_size: f32,
    /// Font used by the interface. BIA ships Fira Sans; `"sans-serif"` means
    /// the system's UI font.
    pub ui_font_family: String,
    /// Interface font size, in logical pixels.
    pub ui_font_size: f32,
    /// Line height, relative to `font_size`.
    pub line_height: f32,
    /// Spaces per indentation level in files that aren't indented yet.
    /// Files that already are keep their own style.
    pub tab_size: usize,
    /// Theme id: the file name (without `.json`) of a built-in or custom theme.
    pub theme: String,
    /// `"auto"`, `"en"` or `"pt-BR"`.
    pub language: String,
    /// Names hidden from the explorer. `*.ext` hides by extension.
    pub hidden: Vec<String>,
    /// Typing `(`, `[`, `{` or a quote also types the closing half.
    pub auto_close: bool,
    /// Long lines wrap at the window's edge instead of scrolling sideways.
    pub soft_wrap: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            font_family: "Fira Code".into(),
            font_size: 14.0,
            ui_font_family: "Fira Sans".into(),
            ui_font_size: 13.0,
            line_height: 1.55,
            tab_size: 4,
            theme: "noite".into(),
            language: "auto".into(),
            hidden: [".git", ".hg", ".svn", ".DS_Store", "Thumbs.db", "node_modules", "target"]
                .map(String::from)
                .to_vec(),
            auto_close: true,
            soft_wrap: false,
        }
    }
}

#[derive(Debug)]
pub enum SettingsError {
    Io(io::Error),
    Parse(serde_json::Error),
}

impl std::fmt::Display for SettingsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "{e}"),
            Self::Parse(e) => write!(f, "line {}, column {}: {e}", e.line(), e.column()),
        }
    }
}

impl std::error::Error for SettingsError {}

impl Settings {
    /// Parses settings, then clamps values into sane ranges.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        let mut s: Settings = serde_json::from_str(json)?;
        s.sanitize();
        Ok(s)
    }

    pub fn to_json(&self) -> String {
        // Serialization of a plain struct cannot fail.
        serde_json::to_string_pretty(self).unwrap_or_default() + "\n"
    }

    /// Loads `path`, creating it with defaults when it does not exist yet.
    ///
    /// On a parse error the defaults are returned *together with* the error,
    /// so BIA always starts and can tell the user what is wrong.
    pub fn load_or_create(path: &Path) -> (Self, Option<SettingsError>) {
        match fs::read_to_string(path) {
            Ok(json) => match Self::from_json(&json) {
                Ok(s) => (s, None),
                Err(e) => (Self::default(), Some(SettingsError::Parse(e))),
            },
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                let defaults = Self::default();
                let created = path
                    .parent()
                    .map_or(Ok(()), fs::create_dir_all)
                    .and_then(|()| fs::write(path, defaults.to_json()));
                (defaults, created.err().map(SettingsError::Io))
            }
            Err(e) => (Self::default(), Some(SettingsError::Io(e))),
        }
    }

    fn sanitize(&mut self) {
        let d = Self::default();
        let clamp = |v: f32, lo: f32, hi: f32, fallback: f32| {
            if v.is_finite() { v.clamp(lo, hi) } else { fallback }
        };
        self.font_size = clamp(self.font_size, 6.0, 72.0, d.font_size);
        self.ui_font_size = clamp(self.ui_font_size, 8.0, 32.0, d.ui_font_size);
        self.line_height = clamp(self.line_height, 1.0, 3.0, d.line_height);
        self.tab_size = self.tab_size.clamp(1, 8);
        if self.font_family.trim().is_empty() {
            self.font_family = d.font_family;
        }
        if self.ui_font_family.trim().is_empty() {
            self.ui_font_family = d.ui_font_family;
        }
        if self.theme.trim().is_empty() {
            self.theme = d.theme;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_object_is_defaults() {
        assert_eq!(Settings::from_json("{}").unwrap(), Settings::default());
    }

    #[test]
    fn partial_and_unknown_keys() {
        let s = Settings::from_json(r#"{ "font_size": 18, "future_key": true }"#).unwrap();
        assert_eq!(s.font_size, 18.0);
        assert_eq!(s.theme, "noite");
    }

    #[test]
    fn clamps_nonsense() {
        let s = Settings::from_json(
            r#"{ "font_size": 900, "line_height": 0, "theme": " ", "tab_size": 0 }"#,
        )
        .unwrap();
        assert_eq!(s.font_size, 72.0);
        assert_eq!(s.tab_size, 1);
        assert_eq!(s.line_height, 1.0);
        assert_eq!(s.theme, "noite");
    }

    #[test]
    fn roundtrip() {
        let s = Settings::default();
        assert_eq!(Settings::from_json(&s.to_json()).unwrap(), s);
    }

    #[test]
    fn creates_missing_file() {
        let dir = std::env::temp_dir().join(format!("bia-settings-{}", std::process::id()));
        let file = dir.join("settings.json");
        let _ = fs::remove_dir_all(&dir);
        let (s, err) = Settings::load_or_create(&file);
        assert!(err.is_none());
        assert_eq!(s, Settings::default());
        assert!(file.exists());
        let _ = fs::remove_dir_all(&dir);
    }
}
