//! Local history: the versions of a file you saved, kept on your disk, so
//! you can go back to one even without git.
//!
//! ```text
//! <config>/history/<hash of the path>/<milliseconds since 1970>.txt
//! ```
//!
//! Each save adds the text that was saved, unless it's the same as the
//! last version. Only the newest [`KEEP`] stay, and files over
//! [`MAX_SIZE`] aren't kept at all.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Versions kept per file.
pub const KEEP: usize = 30;
/// Bigger files aren't kept: their history would fill the disk.
pub const MAX_SIZE: usize = 1024 * 1024;

/// One saved version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    pub saved: SystemTime,
    pub file: PathBuf,
}

/// FNV-1a, as in `backup.rs`: the same path always lands in one folder.
fn folder(dir: &Path, path: &Path) -> PathBuf {
    let hash = path
        .to_string_lossy()
        .bytes()
        .fold(0xcbf2_9ce4_8422_2325_u64, |h, b| (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3));
    dir.join(format!("{hash:016x}"))
}

/// The versions of `path`, newest first.
pub fn list(dir: &Path, path: &Path) -> Vec<Version> {
    let Ok(entries) = fs::read_dir(folder(dir, path)) else { return Vec::new() };
    let mut versions: Vec<Version> = entries
        .flatten()
        .filter_map(|e| {
            let file = e.path();
            let millis: u64 = file.file_stem()?.to_str()?.parse().ok()?;
            Some(Version { saved: UNIX_EPOCH + Duration::from_millis(millis), file })
        })
        .collect();
    versions.sort_by_key(|v| std::cmp::Reverse(v.saved));
    versions
}

/// Keeps `text` as the newest version of `path`.
pub fn record(dir: &Path, path: &Path, text: &str) -> io::Result<()> {
    if text.len() > MAX_SIZE {
        return Ok(());
    }
    let versions = list(dir, path);
    if let Some(last) = versions.first()
        && fs::read_to_string(&last.file).is_ok_and(|t| t == text)
    {
        return Ok(());
    }
    let folder = folder(dir, path);
    fs::create_dir_all(&folder)?;
    let mut millis = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis());
    // Two saves in the same millisecond still get a file each.
    while folder.join(format!("{millis}.txt")).exists() {
        millis += 1;
    }
    crate::document::write_atomic(&folder.join(format!("{millis}.txt")), text.as_bytes())?;
    for old in versions.iter().skip(KEEP - 1) {
        let _ = fs::remove_file(&old.file);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_new_versions_only() {
        let dir = std::env::temp_dir().join(format!("bia-history-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let file = Path::new("/project/notes.md");
        record(&dir, file, "one").unwrap();
        record(&dir, file, "one").unwrap();
        record(&dir, file, "two").unwrap();
        let versions = list(&dir, file);
        assert_eq!(versions.len(), 2);
        assert_eq!(fs::read_to_string(&versions[0].file).unwrap(), "two");
        assert!(list(&dir, Path::new("/other")).is_empty());
        for n in 0..KEEP + 5 {
            record(&dir, file, &n.to_string()).unwrap();
        }
        assert_eq!(list(&dir, file).len(), KEEP);
        let _ = fs::remove_dir_all(&dir);
    }
}
