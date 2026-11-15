//! File operations behind the explorer: create, rename, and keeping paths
//! in sync afterwards. Deleting is the app's job, because it goes through
//! the OS trash.

use crate::tree::valid_name;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub enum FsError {
    /// Empty, `.`/`..`, or contains a path separator.
    InvalidName,
    /// Something with that name is already there.
    Exists(String),
    Io(io::Error),
}

impl fmt::Display for FsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidName => write!(f, "invalid name"),
            Self::Exists(name) => write!(f, "{name} already exists"),
            Self::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for FsError {}

fn exists(path: &Path) -> bool {
    path.symlink_metadata().is_ok()
}

fn checked(dir: &Path, name: &str) -> Result<PathBuf, FsError> {
    let name = name.trim();
    if !valid_name(name) {
        return Err(FsError::InvalidName);
    }
    let path = dir.join(name);
    if exists(&path) {
        return Err(FsError::Exists(name.to_owned()));
    }
    Ok(path)
}

/// Creates an empty file `name` inside `dir`. Never overwrites.
pub fn create_file(dir: &Path, name: &str) -> Result<PathBuf, FsError> {
    let path = checked(dir, name)?;
    fs::OpenOptions::new().write(true).create_new(true).open(&path).map_err(FsError::Io)?;
    Ok(path)
}

/// Creates folder `name` inside `dir`.
pub fn create_dir(dir: &Path, name: &str) -> Result<PathBuf, FsError> {
    let path = checked(dir, name)?;
    fs::create_dir(&path).map_err(FsError::Io)?;
    Ok(path)
}

/// Renames `from` to `new_name` in the same folder. Returns `None` when the
/// name did not change. Changing only the case (`a.rs` → `A.rs`) is allowed
/// even on case-insensitive file systems, where the target "exists".
pub fn rename(from: &Path, new_name: &str) -> Result<Option<PathBuf>, FsError> {
    let new_name = new_name.trim();
    if !valid_name(new_name) {
        return Err(FsError::InvalidName);
    }
    let to = from.with_file_name(new_name);
    if to == from {
        return Ok(None);
    }
    let only_case_changed =
        to.to_string_lossy().to_lowercase() == from.to_string_lossy().to_lowercase();
    if exists(&to) && !only_case_changed {
        return Err(FsError::Exists(new_name.to_owned()));
    }
    fs::rename(from, &to).map_err(FsError::Io)?;
    Ok(Some(to))
}

/// Where `path` ends up after `from` was renamed to `to`. `None` when
/// `path` is not `from` and not inside it.
pub fn moved(path: &Path, from: &Path, to: &Path) -> Option<PathBuf> {
    let rest = path.strip_prefix(from).ok()?;
    Some(if rest.as_os_str().is_empty() { to.to_path_buf() } else { to.join(rest) })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("bia-fsops-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn create_never_overwrites() {
        let dir = scratch("create");
        let file = create_file(&dir, " notes.md ").unwrap();
        assert_eq!(file, dir.join("notes.md"));
        fs::write(&file, "keep me").unwrap();
        assert!(matches!(create_file(&dir, "notes.md"), Err(FsError::Exists(_))));
        assert!(matches!(create_dir(&dir, "notes.md"), Err(FsError::Exists(_))));
        assert_eq!(fs::read_to_string(&file).unwrap(), "keep me");
        assert!(matches!(create_file(&dir, "../escape"), Err(FsError::InvalidName)));
        assert!(create_dir(&dir, "src").unwrap().is_dir());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn rename_rules() {
        let dir = scratch("rename");
        let a = create_file(&dir, "a.txt").unwrap();
        create_file(&dir, "b.txt").unwrap();
        assert!(rename(&a, "a.txt").unwrap().is_none());
        assert!(matches!(rename(&a, "b.txt"), Err(FsError::Exists(_))));
        assert!(matches!(rename(&a, "x/y"), Err(FsError::InvalidName)));
        let to = rename(&a, "A.txt").unwrap().unwrap();
        assert_eq!(to.file_name().unwrap(), "A.txt");
        assert!(to.exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn moved_paths() {
        let (from, to) = (Path::new("/w/src"), Path::new("/w/lib"));
        assert_eq!(moved(Path::new("/w/src"), from, to), Some(PathBuf::from("/w/lib")));
        assert_eq!(
            moved(Path::new("/w/src/a/b.rs"), from, to),
            Some(PathBuf::from("/w/lib/a/b.rs"))
        );
        assert_eq!(moved(Path::new("/w/srcx/a.rs"), from, to), None);
        assert_eq!(moved(Path::new("/w/other.rs"), from, to), None);
    }
}
