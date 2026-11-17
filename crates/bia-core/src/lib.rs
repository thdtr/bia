//! # bia-core
//!
//! Everything in BIA that does not draw pixels: settings, themes, i18n, the
//! file tree, document I/O, the editor model (text, cursors, undo),
//! indentation, language detection, search and what BIA remembers between
//! runs.
//!
//! Keeping this crate free of GUI types means it is fast to compile, easy to
//! test, and would survive a change of UI toolkit untouched.
//!
//! BIA has no network code: nothing in this crate (or in the app) opens a network
//! socket. `tests/sealed.rs` and `deny.toml` enforce it on every commit.

pub mod backup;
pub mod color;
pub mod document;
pub mod editing;
pub mod editor;
pub mod fs_ops;
pub mod fuzzy;
pub mod git;
pub mod i18n;
pub mod indent;
pub mod language;
pub mod paths;
pub mod search;
pub mod settings;
pub mod state;
pub mod text;
pub mod theme;
pub mod tree;

pub use color::Rgba;
pub use i18n::{I18n, Lang};
pub use settings::Settings;
pub use theme::Theme;
