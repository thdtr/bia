//! Where BIA keeps its files.
//!
//! ```text
//! <config>/bia/
//! ├── settings.json
//! ├── state.json   (written by BIA: folders opened and their open files)
//! ├── backups/     (written by BIA: unsaved edits, until saved or discarded)
//! ├── history/     (written by BIA, the last 30 versions you saved of each file)
//! ├── themes/      *.json      (custom themes)
//! └── languages/   *.sublime-syntax  (extra grammars)
//! ```
//!
//! `<config>` is `$XDG_CONFIG_HOME` (or `~/.config`) on Linux/BSD,
//! `~/Library/Application Support` on macOS and `%APPDATA%` on Windows.
//! Set `BIA_CONFIG_DIR` to override it (handy for tests and portable installs).

use std::env;
use std::path::PathBuf;

/// Root of BIA's configuration folder.
pub fn config_dir() -> PathBuf {
    if let Some(dir) = env::var_os("BIA_CONFIG_DIR").filter(|v| !v.is_empty()) {
        return PathBuf::from(dir);
    }
    platform_config_dir().join("bia")
}

pub fn settings_file() -> PathBuf {
    config_dir().join("settings.json")
}

pub fn state_file() -> PathBuf {
    config_dir().join("state.json")
}

pub fn backups_dir() -> PathBuf {
    config_dir().join("backups")
}

/// Versions of the files you saved (see `history.rs`).
pub fn history_dir() -> PathBuf {
    config_dir().join("history")
}

/// Where files that were never saved would live. Nothing is written here:
/// it only gives them a path, so their backups have a name.
pub fn untitled_dir() -> PathBuf {
    config_dir().join("untitled")
}

pub fn themes_dir() -> PathBuf {
    config_dir().join("themes")
}

pub fn languages_dir() -> PathBuf {
    config_dir().join("languages")
}

fn home() -> PathBuf {
    env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

#[cfg(target_os = "windows")]
fn platform_config_dir() -> PathBuf {
    env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join("AppData").join("Roaming"))
}

#[cfg(target_os = "macos")]
fn platform_config_dir() -> PathBuf {
    home().join("Library").join("Application Support")
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn platform_config_dir() -> PathBuf {
    env::var_os("XDG_CONFIG_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".config"))
}
