//! `state.json`: what BIA remembers between runs, which for now is only the
//! last folder you opened. BIA writes this file itself; it isn't a setting.
//!
//! A missing or broken file is the same as an empty one: forgetting the last
//! folder is never worth an error.

use crate::document;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct State {
    pub last_folder: Option<PathBuf>,
}

impl State {
    pub fn load(path: &Path) -> Self {
        fs::read_to_string(path)
            .ok()
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> io::Result<()> {
        let json = serde_json::to_string_pretty(self).map_err(io::Error::other)? + "\n";
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        document::write_atomic(path, json.as_bytes()).map(|_| ())
    }

    /// The last folder, if it still exists.
    pub fn last_folder(&self) -> Option<&Path> {
        self.last_folder.as_deref().filter(|p| p.is_dir())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("bia-state-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn roundtrip() {
        let dir = scratch("roundtrip");
        let file = dir.join("state.json");
        assert_eq!(State::load(&file), State::default());
        let state = State { last_folder: Some(dir.clone()) };
        state.save(&file).unwrap();
        assert_eq!(State::load(&file), state);
        assert_eq!(State::load(&file).last_folder(), Some(dir.as_path()));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn broken_or_stale_is_forgotten() {
        let dir = scratch("broken");
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("state.json");
        fs::write(&file, "{ not json").unwrap();
        assert_eq!(State::load(&file), State::default());

        let gone = State { last_folder: Some(dir.join("deleted")) };
        assert_eq!(gone.last_folder(), None);
        let _ = fs::remove_dir_all(&dir);
    }
}
