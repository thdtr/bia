//! File operations behind the explorer: create, rename, copy, move, and
//! keeping paths in sync afterwards. None of them ever replaces something
//! that's already there. Deleting is the app's job, because it goes through
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

/// `name` may be a path below `dir`, like `src/ui/menu.rs`. The folders on
/// the way are created, and the last part is what gets checked.
fn nested(dir: &Path, name: &str) -> Result<PathBuf, FsError> {
    let parts: Vec<&str> =
        name.trim().split(['/', '\\']).map(str::trim).filter(|p| !p.is_empty()).collect();
    let Some((last, folders)) = parts.split_last() else { return Err(FsError::InvalidName) };
    if !folders.iter().all(|f| valid_name(f)) {
        return Err(FsError::InvalidName);
    }
    let parent = folders.iter().fold(dir.to_path_buf(), |p, f| p.join(f));
    let path = checked(&parent, last)?;
    fs::create_dir_all(&parent).map_err(FsError::Io)?;
    Ok(path)
}

/// Creates an empty file `name` inside `dir`, along with the folders in
/// `name`, if it has some. Never overwrites.
pub fn create_file(dir: &Path, name: &str) -> Result<PathBuf, FsError> {
    let path = nested(dir, name)?;
    fs::OpenOptions::new().write(true).create_new(true).open(&path).map_err(FsError::Io)?;
    Ok(path)
}

/// Creates folder `name` inside `dir`, and the folders on the way.
pub fn create_dir(dir: &Path, name: &str) -> Result<PathBuf, FsError> {
    let path = nested(dir, name)?;
    fs::create_dir(&path).map_err(FsError::Io)?;
    Ok(path)
}

/// A name for a copy of `name` that `dir` doesn't have yet: the name
/// itself if it's free, then `notes copy.md`, `notes copy 2.md`…
pub fn free_name(dir: &Path, name: &str) -> String {
    if !exists(&dir.join(name)) {
        return name.to_owned();
    }
    // The extension stays at the end, except for names like `.env`.
    let (stem, ext) = match name.rfind('.') {
        Some(i) if i > 0 => (&name[..i], &name[i..]),
        _ => (name, ""),
    };
    (1..)
        .map(|n| if n == 1 { format!("{stem} copy{ext}") } else { format!("{stem} copy {n}{ext}") })
        .find(|candidate| !exists(&dir.join(candidate)))
        .unwrap_or_default()
}

/// Copies the file or folder `from` into `dir`, under a free name. Returns
/// where the copy is.
pub fn copy_into(from: &Path, dir: &Path) -> Result<PathBuf, FsError> {
    let name = from.file_name().ok_or(FsError::InvalidName)?.to_string_lossy();
    if from.is_dir() && dir.starts_with(from) {
        // A folder copied into itself would never end.
        return Err(FsError::InvalidName);
    }
    let to = dir.join(free_name(dir, &name));
    copy_all(from, &to).map_err(FsError::Io)?;
    Ok(to)
}

fn copy_all(from: &Path, to: &Path) -> io::Result<()> {
    if fs::symlink_metadata(from)?.is_dir() {
        fs::create_dir(to)?;
        for entry in fs::read_dir(from)? {
            let entry = entry?;
            copy_all(&entry.path(), &to.join(entry.file_name()))?;
        }
        Ok(())
    } else {
        fs::copy(from, to).map(|_| ())
    }
}

/// Moves the file or folder `from` into `dir`, keeping its name. Returns
/// where it went, or `None` when it already was there.
pub fn move_into(from: &Path, dir: &Path) -> Result<Option<PathBuf>, FsError> {
    let name = from.file_name().ok_or(FsError::InvalidName)?;
    let to = dir.join(name);
    if to == from {
        return Ok(None);
    }
    if dir.starts_with(from) {
        return Err(FsError::InvalidName);
    }
    if exists(&to) {
        return Err(FsError::Exists(name.to_string_lossy().into_owned()));
    }
    fs::rename(from, &to).map_err(FsError::Io)?;
    Ok(Some(to))
}

/// Whether `path`'s folder has an entry spelled exactly like its name. On a
/// case-insensitive file system `A.rs` "exists" when only `a.rs` is there,
/// but it isn't listed.
fn listed(path: &Path) -> bool {
    let (Some(dir), Some(name)) = (path.parent(), path.file_name()) else { return true };
    fs::read_dir(dir).map_or(true, |entries| entries.flatten().any(|e| e.file_name() == name))
}

/// Renames `from` to `new_name` in the same folder. Returns `None` when the
/// name did not change. Never replaces another file.
///
/// Changing only the case (`a.rs` → `A.rs`) works on case-insensitive file
/// systems too, where the target "exists" because it is the same file. On a
/// case-sensitive one, an existing `A.rs` is a different file and is kept.
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
    if exists(&to) && (!only_case_changed || listed(&to)) {
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
    fn create_with_folders_on_the_way() {
        let dir = scratch("nested");
        let file = create_file(&dir, "src/ui/menu.rs").unwrap();
        assert_eq!(file, dir.join("src").join("ui").join("menu.rs"));
        assert!(file.is_file());
        assert!(create_dir(&dir, "docs/notes/").unwrap().is_dir());
        assert!(matches!(create_file(&dir, "src/ui/menu.rs"), Err(FsError::Exists(_))));
        assert!(matches!(create_file(&dir, "src/../../x"), Err(FsError::InvalidName)));
        assert!(matches!(create_file(&dir, "/"), Err(FsError::InvalidName)));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn copies_get_a_free_name() {
        let dir = scratch("copy");
        let notes = create_file(&dir, "notes.md").unwrap();
        fs::write(&notes, "hi").unwrap();
        assert_eq!(free_name(&dir, "other.md"), "other.md");
        let first = copy_into(&notes, &dir).unwrap();
        assert_eq!(first, dir.join("notes copy.md"));
        let second = copy_into(&notes, &dir).unwrap();
        assert_eq!(second, dir.join("notes copy 2.md"));
        assert_eq!(fs::read_to_string(second).unwrap(), "hi");
        create_file(&dir, ".env").unwrap();
        assert_eq!(free_name(&dir, ".env"), ".env copy");

        let src = create_dir(&dir, "src").unwrap();
        create_file(&src, "a/b.rs").unwrap();
        let copy = copy_into(&src, &dir).unwrap();
        assert!(copy.join("a").join("b.rs").is_file());
        assert!(matches!(copy_into(&src, &src.join("a")), Err(FsError::InvalidName)));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn moving_never_replaces() {
        let dir = scratch("move");
        let a = create_file(&dir, "a.txt").unwrap();
        let sub = create_dir(&dir, "sub").unwrap();
        assert!(move_into(&a, &dir).unwrap().is_none());
        let moved = move_into(&a, &sub).unwrap().unwrap();
        assert_eq!(moved, sub.join("a.txt"));
        let again = create_file(&dir, "a.txt").unwrap();
        assert!(matches!(move_into(&again, &sub), Err(FsError::Exists(_))));
        assert!(again.exists());
        assert!(matches!(move_into(&sub, &sub), Err(FsError::InvalidName)));
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
    fn case_rename_never_replaces_another_file() {
        let dir = scratch("case");
        let lower = dir.join("makefile");
        fs::write(&lower, "small").unwrap();
        let case_insensitive = dir.join("MAKEFILE").exists();
        if !case_insensitive {
            fs::write(dir.join("Makefile"), "keep me").unwrap();
            assert!(matches!(rename(&lower, "Makefile"), Err(FsError::Exists(_))));
            assert_eq!(fs::read_to_string(dir.join("Makefile")).unwrap(), "keep me");
            assert_eq!(fs::read_to_string(&lower).unwrap(), "small");
            fs::remove_file(dir.join("Makefile")).unwrap();
        }
        let to = rename(&lower, "Makefile").unwrap().unwrap();
        assert_eq!(fs::read_to_string(&to).unwrap(), "small");
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
