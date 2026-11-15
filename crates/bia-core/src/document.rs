//! Reading and writing files without ever damaging them.
//!
//! * Line endings are detected and written back the same way.
//! * A UTF-8 byte-order mark is preserved.
//! * Binary, non-UTF-8 and oversized files are refused instead of mangled,
//!   and so are files with bare CR line breaks (see [`LoadError::BareCr`]).
//! * Saves are atomic: write a sibling temp file, flush it to disk, then
//!   rename it over the original. A crash mid-save never leaves half a file.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

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
    /// The file mixes LF and CRLF. It is saved with `line_ending`, the more
    /// common of the two.
    pub mixed_endings: bool,
    pub bom: bool,
    pub modified: Option<SystemTime>,
}

#[derive(Debug)]
pub enum LoadError {
    Io(io::Error),
    Binary,
    NotUtf8,
    TooLarge(u64),
    /// A CR that is not part of a CRLF. The editor widget treats it as a
    /// line break and BIA's copy of the text does not, so editing such a
    /// file could change the wrong text.
    BareCr,
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "{e}"),
            Self::Binary => write!(f, "binary file"),
            Self::NotUtf8 => write!(f, "not UTF-8"),
            Self::TooLarge(n) => write!(f, "too large ({})", human_size(*n)),
            Self::BareCr => write!(f, "CR line breaks"),
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
        if text.contains('\r') {
            return Err(LoadError::BareCr);
        }
        Ok(Self { text, line_ending, mixed_endings: crlf > 0 && lf > 0, bom, modified: None })
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

/// Replaces `path` with `bytes`, keeping its permissions. Follows symlinks.
/// Returns the new modification time.
///
/// Normally this writes a temp file next to the original and renames it over
/// it, so a crash never leaves half a file. Two cases write in place instead:
/// files with more than one hard link (a rename would split them apart) and
/// folders where BIA may change the file but not create new ones.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<Option<SystemTime>> {
    let target = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let meta = fs::metadata(&target).ok();
    if meta.as_ref().is_some_and(has_other_links) {
        write_in_place(&target, bytes)?;
    } else {
        match replace(&target, bytes, meta.as_ref()) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::PermissionDenied && meta.is_some() => {
                write_in_place(&target, bytes)?;
            }
            Err(e) => return Err(e),
        }
    }
    Ok(modified(&target))
}

fn replace(target: &Path, bytes: &[u8], meta: Option<&fs::Metadata>) -> io::Result<()> {
    let dir = target.parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let (tmp, mut file) = create_temp(dir, target, meta)?;
    let result: io::Result<()> = (|| {
        file.write_all(bytes)?;
        file.sync_all()?;
        if let Some(meta) = meta {
            file.set_permissions(meta.permissions())?;
        }
        drop(file);
        fs::rename(&tmp, target)?;
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
    result
}

/// Creates `.<name>.bia~<unique>` next to `target`. `create_new` refuses
/// anything already there, symlinks included, so the write can't be
/// redirected. On Unix the file starts with the target's permissions, so a
/// private file is never readable by others, not even for a moment.
fn create_temp(
    dir: &Path,
    target: &Path,
    meta: Option<&fs::Metadata>,
) -> io::Result<(PathBuf, File)> {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let name = target.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    // Leave room for the prefix and suffix within the usual 255-byte limit.
    let mut cut = name.len().min(200);
    while !name.is_char_boundary(cut) {
        cut -= 1;
    }
    let stem = &name[..cut];
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.subsec_nanos());

    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        options.mode(meta.map_or(0o666, |m| m.permissions().mode() & 0o777));
    }
    #[cfg(not(unix))]
    let _ = meta;

    for _ in 0..16 {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let tmp = dir.join(format!(".{stem}.bia~{:x}{:x}{n:x}", std::process::id(), nanos));
        match options.open(&tmp) {
            Ok(file) => return Ok((tmp, file)),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    Err(io::Error::new(io::ErrorKind::AlreadyExists, "no free name for a temporary file"))
}

fn write_in_place(target: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = OpenOptions::new().write(true).truncate(true).open(target)?;
    file.write_all(bytes)?;
    file.sync_all()
}

#[cfg(unix)]
fn has_other_links(meta: &fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    meta.is_file() && meta.nlink() > 1
}

#[cfg(not(unix))]
fn has_other_links(_: &fs::Metadata) -> bool {
    false
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
    fn refuses_bare_cr() {
        for bytes in [&b"a\rb"[..], b"a\n\rb", b"a\r\nb\r"] {
            assert!(matches!(Document::from_bytes(bytes), Err(LoadError::BareCr)), "{bytes:?}");
        }
        assert!(Document::from_bytes(b"a\r\n\r\nb").is_ok());
    }

    #[test]
    fn mixed_endings_are_flagged() {
        let doc = Document::from_bytes(b"a\r\nb\r\nc\n").unwrap();
        assert!(doc.mixed_endings);
        assert_eq!(doc.line_ending, LineEnding::CrLf);
        assert!(!Document::from_bytes(b"a\r\nb\r\n").unwrap().mixed_endings);
        assert!(!Document::from_bytes(b"a\nb\n").unwrap().mixed_endings);
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("bia-doc-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn atomic_write_replaces_content() {
        let dir = scratch("replace");
        let file = dir.join("f.txt");
        fs::write(&file, "old").unwrap();
        write_atomic(&file, b"new").unwrap();
        assert_eq!(fs::read_to_string(&file).unwrap(), "new");
        let leftovers = fs::read_dir(&dir).unwrap().count();
        assert_eq!(leftovers, 1, "temp file left behind");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn long_file_names_save() {
        let dir = scratch("long");
        let file = dir.join(format!("{}.txt", "n".repeat(245)));
        fs::write(&file, "old").unwrap();
        write_atomic(&file, b"new").unwrap();
        assert_eq!(fs::read_to_string(&file).unwrap(), "new");
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn unix_permissions_and_links_survive() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let dir = scratch("unix");

        // Permissions are kept.
        let private = dir.join("private.txt");
        fs::write(&private, "old").unwrap();
        fs::set_permissions(&private, fs::Permissions::from_mode(0o600)).unwrap();
        write_atomic(&private, b"new").unwrap();
        assert_eq!(fs::metadata(&private).unwrap().permissions().mode() & 0o777, 0o600);

        // Saving through a symlink changes the file it points to.
        let link = dir.join("link.txt");
        symlink(&private, &link).unwrap();
        write_atomic(&link, b"via link").unwrap();
        assert!(fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
        assert_eq!(fs::read_to_string(&private).unwrap(), "via link");

        // Hard links still share the content.
        let (a, b) = (dir.join("a.txt"), dir.join("b.txt"));
        fs::write(&a, "one").unwrap();
        fs::hard_link(&a, &b).unwrap();
        write_atomic(&a, b"two").unwrap();
        assert_eq!(fs::read_to_string(&b).unwrap(), "two");

        // A writable file in a read-only folder can still be saved.
        let ro = dir.join("ro");
        fs::create_dir(&ro).unwrap();
        let file = ro.join("f.txt");
        fs::write(&file, "old").unwrap();
        fs::set_permissions(&ro, fs::Permissions::from_mode(0o555)).unwrap();
        let saved = write_atomic(&file, b"new");
        fs::set_permissions(&ro, fs::Permissions::from_mode(0o755)).unwrap();
        saved.unwrap();
        assert_eq!(fs::read_to_string(&file).unwrap(), "new");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn sizes() {
        assert_eq!(human_size(512), "512 B");
        assert_eq!(human_size(16 * 1024 * 1024), "16.0 MB");
    }
}
