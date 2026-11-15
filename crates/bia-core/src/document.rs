//! Reading and writing files without ever damaging them.
//!
//! * Line endings are detected and written back the same way.
//! * A UTF-8 byte-order mark is preserved.
//! * Binary, non-UTF-8 and oversized files are refused instead of mangled.
//! * Saves are atomic: write a sibling temp file, flush it to disk, then
//!   rename it over the original. A crash mid-save never leaves half a file.

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Files bigger than this are not opened (the editor would crawl).
pub const MAX_SIZE: u64 = 16 * 1024 * 1024;

const BOM: &str = "\u{feff}";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineEnding {
    #[default]
    Lf,
    CrLf,
}

impl LineEnding {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Lf => "\n",
            Self::CrLf => "\r\n",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Lf => "LF",
            Self::CrLf => "CRLF",
        }
    }
}

/// A file's text, normalized to `\n`, plus what is needed to write it back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document {
    pub text: String,
    pub line_ending: LineEnding,
    pub bom: bool,
    pub modified: Option<SystemTime>,
}

#[derive(Debug)]
pub enum LoadError {
    Io(io::Error),
    Binary,
    NotUtf8,
    TooLarge(u64),
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "{e}"),
            Self::Binary => write!(f, "binary file"),
            Self::NotUtf8 => write!(f, "not UTF-8"),
            Self::TooLarge(n) => write!(f, "too large ({})", human_size(*n)),
        }
    }
}

impl std::error::Error for LoadError {}

impl Document {
    pub fn load(path: &Path) -> Result<Self, LoadError> {
        let meta = fs::metadata(path).map_err(LoadError::Io)?;
        if meta.len() > MAX_SIZE {
            return Err(LoadError::TooLarge(meta.len()));
        }
        let bytes = fs::read(path).map_err(LoadError::Io)?;
        let mut doc = Self::from_bytes(&bytes)?;
        doc.modified = meta.modified().ok();
        Ok(doc)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, LoadError> {
        // A NUL byte in the first 8 KiB is the classic "this is binary" signal.
        if bytes.iter().take(8192).any(|&b| b == 0) {
            return Err(LoadError::Binary);
        }
        let raw = std::str::from_utf8(bytes).map_err(|_| LoadError::NotUtf8)?;
        let (raw, bom) = match raw.strip_prefix(BOM) {
            Some(rest) => (rest, true),
            None => (raw, false),
        };
        let crlf = raw.matches("\r\n").count();
        let lf = raw.matches('\n').count() - crlf;
        let line_ending = if crlf > lf { LineEnding::CrLf } else { LineEnding::Lf };
        let text = if crlf > 0 { raw.replace("\r\n", "\n") } else { raw.to_owned() };
        Ok(Self { text, line_ending, bom, modified: None })
    }

    /// The exact bytes to write for `text` (which uses `\n`).
    pub fn encode(text: &str, line_ending: LineEnding, bom: bool) -> Vec<u8> {
        let mut out = String::with_capacity(text.len() + 3);
        if bom {
            out.push_str(BOM);
        }
        match line_ending {
            LineEnding::Lf => out.push_str(text),
            LineEnding::CrLf => out.push_str(&text.replace('\n', "\r\n")),
        }
        out.into_bytes()
    }
}

/// Atomically replaces `path` with `bytes`, keeping its permissions.
/// Returns the new modification time.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<Option<SystemTime>> {
    // Write through symlinks instead of replacing the link with a file.
    let target = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let dir = target.parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let name = target.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let tmp: PathBuf = dir.join(format!(".{name}.bia~{}", std::process::id()));

    let result: io::Result<()> = (|| {
        let mut file = File::create(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        if let Ok(meta) = fs::metadata(&target) {
            fs::set_permissions(&tmp, meta.permissions())?;
        }
        fs::rename(&tmp, &target)?;
        // Make the rename itself durable. Windows has no directory handles
        // to sync, and a failure here is not worth failing the save over.
        #[cfg(unix)]
        if let Ok(dir) = File::open(dir) {
            let _ = dir.sync_all();
        }
        Ok(())
    })();

    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result?;
    Ok(fs::metadata(&target).and_then(|m| m.modified()).ok())
}

/// Modification time, if the file still exists.
pub fn modified(path: &Path) -> Option<SystemTime> {
    fs::metadata(path).and_then(|m| m.modified()).ok()
}

pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 { format!("{bytes} B") } else { format!("{value:.1} {}", UNITS[unit]) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crlf_and_bom_roundtrip() {
        let original = "\u{feff}a\r\nb\r\n".as_bytes();
        let doc = Document::from_bytes(original).unwrap();
        assert_eq!(doc.text, "a\nb\n");
        assert_eq!(doc.line_ending, LineEnding::CrLf);
        assert!(doc.bom);
        assert_eq!(Document::encode(&doc.text, doc.line_ending, doc.bom), original);
    }

    #[test]
    fn lf_untouched() {
        let doc = Document::from_bytes(b"x\ny").unwrap();
        assert_eq!((doc.text.as_str(), doc.line_ending, doc.bom), ("x\ny", LineEnding::Lf, false));
    }

    #[test]
    fn refuses_binary_and_latin1() {
        assert!(matches!(Document::from_bytes(b"PNG\0\x01"), Err(LoadError::Binary)));
        assert!(matches!(Document::from_bytes(b"caf\xe9"), Err(LoadError::NotUtf8)));
    }

    #[test]
    fn atomic_write_replaces_content() {
        let dir = std::env::temp_dir().join(format!("bia-doc-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("f.txt");
        fs::write(&file, "old").unwrap();
        write_atomic(&file, b"new").unwrap();
        assert_eq!(fs::read_to_string(&file).unwrap(), "new");
        let leftovers = fs::read_dir(&dir).unwrap().count();
        assert_eq!(leftovers, 1, "temp file left behind");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn sizes() {
        assert_eq!(human_size(512), "512 B");
        assert_eq!(human_size(16 * 1024 * 1024), "16.0 MB");
    }
}
