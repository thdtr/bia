//! `settings.json`: the only configuration file BIA has.
//!
//! Every key is optional. Unknown keys are ignored, so old and new BIA
//! versions can share one file. The file is re-read whenever it is saved
//! from inside BIA; there is no settings screen.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
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
    /// Long lines wrap at the window's edge (`true` or `"edge"`), at
    /// `wrap_column` (`"column"`), or not at all (`false` or `"off"`).
    pub soft_wrap: SoftWrap,
    /// Where lines wrap with `"soft_wrap": "column"`, in characters.
    pub wrap_column: usize,
    /// Columns where a thin line is drawn, to see how long lines get.
    pub rulers: Vec<usize>,
    /// Thin lines down each level of indentation.
    pub indent_guides: bool,
    /// Where spaces and tabs are drawn, `"none"`, `"selection"` or `"all"`.
    pub show_whitespace: Whitespace,
    pub line_numbers: bool,
    /// Line numbers count from the cursor's line, for `vim`-style moves.
    pub relative_line_numbers: bool,
    pub cursor_blink: bool,
    /// Indent new files with spaces (`true`) or tabs. Files that are
    /// already indented keep their own style.
    pub insert_spaces: bool,
    /// Save by itself. `"off"`, `"focus_change"` (when the window loses
    /// focus) or `"after_delay"` (a moment after you stop typing).
    pub autosave: Autosave,
    /// Ask the language server to format the file before saving.
    pub format_on_save: bool,
    pub trim_trailing_whitespace_on_save: bool,
    /// Add a line break at the end when there's none. Line breaks that are
    /// already there stay.
    pub ensure_final_newline_on_save: bool,
    /// Reopen the last folder when BIA starts.
    pub restore_on_startup: bool,
    /// Which language a file is, by name or `*.ext`, when BIA guesses
    /// wrong, like `{ "*.env.local": "Bourne Again Shell (bash)" }`.
    pub file_types: BTreeMap<String, String>,
    /// Settings for one language, by its name as the status bar shows it.
    pub languages: BTreeMap<String, LanguageSettings>,
    pub terminal: TerminalSettings,
    /// Language servers, by language name as the status bar shows it:
    /// `{ "Rust": ["rust-analyzer"] }`. None by default: BIA never starts a
    /// program you didn't name here.
    pub language_servers: BTreeMap<String, Vec<String>>,
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
            soft_wrap: SoftWrap::Off,
            wrap_column: 80,
            rulers: Vec::new(),
            indent_guides: true,
            show_whitespace: Whitespace::Selection,
            line_numbers: true,
            relative_line_numbers: false,
            cursor_blink: true,
            insert_spaces: true,
            autosave: Autosave::Off,
            format_on_save: false,
            trim_trailing_whitespace_on_save: false,
            ensure_final_newline_on_save: false,
            restore_on_startup: true,
            file_types: BTreeMap::new(),
            languages: BTreeMap::new(),
            terminal: TerminalSettings::default(),
            language_servers: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SoftWrap {
    #[default]
    Off,
    Edge,
    /// At `wrap_column`, or the edge if that comes first.
    Column,
}

/// `true` and `false` from older settings files still mean edge and off.
impl<'de> Deserialize<'de> for SoftWrap {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Either {
            Flag(bool),
            Name(String),
        }
        Ok(match Either::deserialize(d)? {
            Either::Flag(true) => SoftWrap::Edge,
            Either::Flag(false) => SoftWrap::Off,
            Either::Name(name) => match name.as_str() {
                "edge" | "on" => SoftWrap::Edge,
                "column" => SoftWrap::Column,
                _ => SoftWrap::Off,
            },
        })
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Whitespace {
    None,
    #[default]
    Selection,
    All,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Autosave {
    #[default]
    Off,
    FocusChange,
    AfterDelay,
}

/// What one language may set differently. Anything left out follows the
/// main settings.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct LanguageSettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tab_size: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub insert_spaces: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rulers: Option<Vec<usize>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub soft_wrap: Option<SoftWrap>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wrap_column: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format_on_save: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trim_trailing_whitespace_on_save: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ensure_final_newline_on_save: Option<bool>,
}

/// The terminal panel. Empty means BIA's choice, your login shell and the
/// editor's font.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TerminalSettings {
    /// The program to run, then its arguments, like `["fish"]` or `["bash", "-l"]`.
    pub shell: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_family: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_size: Option<f32>,
}

/// The settings that apply to one file, after its language's overrides.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForLanguage {
    pub tab_size: usize,
    pub insert_spaces: bool,
    pub rulers: Vec<usize>,
    pub soft_wrap: SoftWrap,
    pub wrap_column: usize,
    pub format_on_save: bool,
    pub trim_trailing_whitespace_on_save: bool,
    pub ensure_final_newline_on_save: bool,
}

/// Every key BIA reads, to point out the ones it doesn't know (a typo,
/// usually).
const KEYS: [&str; 28] = [
    "font_family",
    "font_size",
    "ui_font_family",
    "ui_font_size",
    "line_height",
    "tab_size",
    "theme",
    "language",
    "hidden",
    "auto_close",
    "soft_wrap",
    "wrap_column",
    "rulers",
    "indent_guides",
    "show_whitespace",
    "line_numbers",
    "relative_line_numbers",
    "cursor_blink",
    "insert_spaces",
    "autosave",
    "format_on_save",
    "trim_trailing_whitespace_on_save",
    "ensure_final_newline_on_save",
    "restore_on_startup",
    "file_types",
    "languages",
    "terminal",
    "language_servers",
];

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

    /// Top-level keys in `json` that BIA doesn't read.
    pub fn unknown_keys(json: &str) -> Vec<String> {
        let Ok(serde_json::Value::Object(map)) = serde_json::from_str(json) else {
            return Vec::new();
        };
        map.keys().filter(|k| !KEYS.contains(&k.as_str())).cloned().collect()
    }

    /// The settings for a file in `language`, with that language's
    /// overrides applied.
    pub fn for_language(&self, language: &str) -> ForLanguage {
        let o = self.languages.get(language).cloned().unwrap_or_default();
        ForLanguage {
            tab_size: o.tab_size.unwrap_or(self.tab_size).clamp(1, 8),
            insert_spaces: o.insert_spaces.unwrap_or(self.insert_spaces),
            rulers: o.rulers.unwrap_or_else(|| self.rulers.clone()),
            soft_wrap: o.soft_wrap.unwrap_or(self.soft_wrap),
            wrap_column: o.wrap_column.unwrap_or(self.wrap_column).clamp(20, 1000),
            format_on_save: o.format_on_save.unwrap_or(self.format_on_save),
            trim_trailing_whitespace_on_save: o
                .trim_trailing_whitespace_on_save
                .unwrap_or(self.trim_trailing_whitespace_on_save),
            ensure_final_newline_on_save: o
                .ensure_final_newline_on_save
                .unwrap_or(self.ensure_final_newline_on_save),
        }
    }

    /// The language `file_types` names for a file called `name`.
    pub fn file_type(&self, name: &str) -> Option<&str> {
        self.file_types.iter().find_map(|(pattern, language)| {
            let hit = match pattern.strip_prefix('*') {
                Some(suffix) if !suffix.is_empty() => name.ends_with(suffix),
                _ => pattern == name,
            };
            hit.then_some(language.as_str())
        })
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
        self.wrap_column = self.wrap_column.clamp(20, 1000);
        self.rulers.retain(|&c| (1..=1000).contains(&c));
        self.rulers.sort_unstable();
        if let Some(size) = &mut self.terminal.font_size {
            *size = clamp(*size, 6.0, 72.0, d.font_size);
        }
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
    fn soft_wrap_takes_old_and_new_values() {
        let wrap =
            |v: &str| Settings::from_json(&format!("{{\"soft_wrap\": {v}}}")).unwrap().soft_wrap;
        assert_eq!(wrap("true"), SoftWrap::Edge);
        assert_eq!(wrap("false"), SoftWrap::Off);
        assert_eq!(wrap("\"column\""), SoftWrap::Column);
        assert_eq!(wrap("\"edge\""), SoftWrap::Edge);
    }

    #[test]
    fn wrap_column_per_language() {
        let s = Settings::from_json(
            r#"{ "soft_wrap": "column", "wrap_column": 100, "languages": { "Markdown": { "wrap_column": 72 } } }"#,
        )
        .unwrap();
        assert_eq!(s.for_language("Rust").wrap_column, 100);
        assert_eq!(s.for_language("Markdown").wrap_column, 72);
        assert_eq!(Settings::from_json(r#"{ "wrap_column": 3 }"#).unwrap().wrap_column, 20);
    }

    #[test]
    fn languages_override_the_rest() {
        let s = Settings::from_json(
            r#"{ "tab_size": 4, "rulers": [100], "languages": { "Python": { "rulers": [88] } } }"#,
        )
        .unwrap();
        assert_eq!(s.for_language("Python").rulers, [88]);
        assert_eq!(s.for_language("Python").tab_size, 4);
        assert_eq!(s.for_language("Rust").rulers, [100]);
    }

    #[test]
    fn file_types_and_unknown_keys() {
        let s = Settings::from_json(
            r#"{ "file_types": { "*.env.local": "Shell", "Justfile": "Make" } }"#,
        )
        .unwrap();
        assert_eq!(s.file_type("a.env.local"), Some("Shell"));
        assert_eq!(s.file_type("Justfile"), Some("Make"));
        assert_eq!(s.file_type("main.rs"), None);
        assert_eq!(Settings::unknown_keys(r#"{ "font_size": 1, "fontsize": 2 }"#), ["fontsize"]);
        let defaults = Settings::default().to_json();
        assert!(Settings::unknown_keys(&defaults).is_empty());
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
