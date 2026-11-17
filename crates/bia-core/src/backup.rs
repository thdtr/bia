//! Unsaved edits kept on disk, so a crash or a power cut doesn't lose them.
//!
//! One JSON file per edited file in `backups/`, named after a hash of its
//! path. BIA writes it a moment after you stop typing and deletes it once
//! the file is saved or its edits are discarded. Opening a file that has a
//! backup brings the edits back.

use crate::document;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Backup {
    pub path: PathBuf,
    pub text: String,
    /// When the file on disk was last modified as the edits began, in
    /// seconds and nanoseconds since 1970. If the file changed since,
    /// saving will ask before overwriting it.
    modified: Option<(u64, u32)>,
}

impl Backup {
    pub fn new(path: PathBuf, text: String, modified: Option<SystemTime>) -> Self {
        let modified = modified
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| (d.as_secs(), d.subsec_nanos()));
        Self { path, text, modified }
    }

    pub fn modified(&self) -> Option<SystemTime> {
        self.modified.map(|(s, n)| UNIX_EPOCH + Duration::new(s, n))
    }
}

/// FNV-1a: stable across Rust versions, unlike `DefaultHasher`, so a
/// backup written by one BIA build is found by the next.
fn hash(path: &Path) -> u64 {
    path.to_string_lossy()
        .bytes()
        .fold(0xcbf2_9ce4_8422_2325, |h, b| (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3))
}

fn file_for(dir: &Path, path: &Path) -> PathBuf {
    dir.join(format!("{:016x}.json", hash(path)))
}

pub fn write(dir: &Path, backup: &Backup) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    let json = serde_json::to_string(backup).map_err(io::Error::other)?;
    document::write_atomic(&file_for(dir, &backup.path), json.as_bytes()).map(|_| ())
}

pub fn read(dir: &Path, path: &Path) -> Option<Backup> {
    let json = fs::read_to_string(file_for(dir, path)).ok()?;
    let backup: Backup = serde_json::from_str(&json).ok()?;
    // Two paths with the same hash: not ours.
    (backup.path == path).then_some(backup)
}

pub fn remove(dir: &Path, path: &Path) {
    if read(dir, path).is_some() {
        let _ = fs::remove_file(file_for(dir, path));
    }
}

/// Every backup in `dir`.
pub fn all(dir: &Path) -> Vec<Backup> {
    let Ok(entries) = fs::read_dir(dir) else { return Vec::new() };
    entries
        .flatten()
        .filter_map(|e| serde_json::from_str(&fs::read_to_string(e.path()).ok()?).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_read_remove() {
        let dir = std::env::temp_dir().join(format!("bia-backup-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let path = PathBuf::from("/home/x/src/main.rs");
        let when = UNIX_EPOCH + Duration::new(1_700_000_000, 42);
        let backup = Backup::new(path.clone(), "olá\n".into(), Some(when));

        assert_eq!(read(&dir, &path), None);
        write(&dir, &backup).unwrap();
        let back = read(&dir, &path).unwrap();
        assert_eq!(back, backup);
        assert_eq!(back.modified(), Some(when));
        assert_eq!(all(&dir), [backup]);
        assert_eq!(read(&dir, Path::new("/other")), None);

        remove(&dir, &path);
        assert_eq!(read(&dir, &path), None);
        assert!(all(&dir).is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn hash_is_stable() {
        assert_eq!(hash(Path::new("")), 0xcbf2_9ce4_8422_2325);
        assert_eq!(hash(Path::new("a")), 0xaf63_dc4c_8601_ec8c);
    }
}
