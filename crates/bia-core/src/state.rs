//! `state.json`: what BIA remembers between runs. For each folder you
//! opened (the last 20), the files that were open in it and where the
//! cursor was; and which folder was open last. BIA writes this file itself;
//! it isn't a setting.
//!
//! A missing or broken file is the same as an empty one: forgetting is never
//! worth an error.

use crate::document;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Folders remembered, most recent first.
const MAX_SESSIONS: usize = 20;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct State {
    pub last_folder: Option<PathBuf>,
    /// Most recent first.
    pub sessions: Vec<Session>,
}

/// The open files of one folder.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Session {
    pub folder: PathBuf,
    pub files: Vec<OpenFile>,
    /// Index into `files` of the tab that was in front.
    pub active: usize,
}

/// An open file and its cursor: 0-based line and character column.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct OpenFile {
    pub path: PathBuf,
    pub line: usize,
    pub column: usize,
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

    /// Loads the file, changes it and writes it back.
    pub fn update(path: &Path, change: impl FnOnce(&mut Self)) -> io::Result<()> {
        let mut state = Self::load(path);
        change(&mut state);
        state.save(path)
    }

    /// The last folder, if it still exists.
    pub fn last_folder(&self) -> Option<&Path> {
        self.last_folder.as_deref().filter(|p| p.is_dir())
    }

    pub fn session(&self, folder: &Path) -> Option<&Session> {
        self.sessions.iter().find(|s| s.folder == folder)
    }

    /// Stores `session`, replacing the one for the same folder, in front.
    pub fn remember(&mut self, session: Session) {
        self.sessions.retain(|s| s.folder != session.folder);
        self.sessions.insert(0, session);
        self.sessions.truncate(MAX_SESSIONS);
    }

    /// Puts `folder` in front, keeping its files if it had a session.
    pub fn touch(&mut self, folder: &Path) {
        let session = self
            .session(folder)
            .cloned()
            .unwrap_or_else(|| Session { folder: folder.to_path_buf(), ..Session::default() });
        self.remember(session);
    }

    /// Folders opened before that still exist, most recent first.
    pub fn recent_folders(&self) -> Vec<PathBuf> {
        self.sessions.iter().map(|s| s.folder.clone()).filter(|f| f.is_dir()).collect()
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
        let state = State { last_folder: Some(dir.clone()), sessions: Vec::new() };
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

        let gone = State { last_folder: Some(dir.join("deleted")), sessions: Vec::new() };
        assert_eq!(gone.last_folder(), None);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn sessions_most_recent_first() {
        let dir = scratch("sessions");
        fs::create_dir_all(dir.join("a")).unwrap();
        fs::create_dir_all(dir.join("b")).unwrap();
        let file = dir.join("state.json");
        let open = OpenFile { path: dir.join("a/x.rs"), line: 3, column: 1 };
        State::update(&file, |s| {
            s.remember(Session { folder: dir.join("a"), files: vec![open.clone()], active: 0 });
            s.touch(&dir.join("b"));
            s.touch(&dir.join("gone"));
            s.touch(&dir.join("a"));
        })
        .unwrap();
        let state = State::load(&file);
        assert_eq!(state.recent_folders(), [dir.join("a"), dir.join("b")]);
        assert_eq!(state.session(&dir.join("a")).unwrap().files, [open]);

        // A file from before sessions existed still loads.
        fs::write(&file, r#"{ "last_folder": null }"#).unwrap();
        assert_eq!(State::load(&file), State::default());
        let _ = fs::remove_dir_all(&dir);
    }
}
