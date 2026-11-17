//! What git knows about the open folder: the branch, which files changed,
//! and the committed text of a file, for the marks in the editor's gutter.
//!
//! BIA runs the `git` program for this, with commands that only read the
//! repository on disk. Settings that could make git run other programs or
//! reach the network are turned off on every call (see [`command`]). Without
//! git installed, or outside a repository, every function returns nothing.

use similar::{DiffOp, TextDiff};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// A `git` command that can't run hooks, the fsmonitor daemon or any
/// transport, and never asks for input.
fn command(dir: &Path) -> Command {
    let mut cmd = Command::new("git");
    cmd.arg("-C")
        .arg(dir)
        .args(["--no-pager", "--no-optional-locks"])
        // A repository's own config could name programs for these.
        .args(["-c", "core.fsmonitor=false", "-c", "core.hooksPath=/dev/null"])
        .args(["-c", "protocol.allow=never"])
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // No console window flashing up.
        cmd.creation_flags(0x0800_0000);
    }
    cmd
}

fn run(dir: &Path, args: &[&str]) -> Option<Vec<u8>> {
    let output = command(dir).args(args).output().ok()?;
    output.status.success().then_some(output.stdout)
}

/// The text of `file` as of the last commit, with `\n` line breaks. `None`
/// when it isn't committed (or there's no repository or no git).
pub fn head_text(file: &Path) -> Option<String> {
    let dir = file.parent()?;
    let name = file.file_name()?.to_str()?;
    let bytes = run(dir, &["cat-file", "blob", &format!("HEAD:./{name}")])?;
    let text = String::from_utf8(bytes).ok()?;
    Some(if text.contains('\r') { text.replace("\r\n", "\n") } else { text })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileStatus {
    /// Changed since the last commit.
    Modified,
    /// New: untracked, or added but never committed.
    Added,
    /// Both sides of a merge changed it.
    Conflict,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Status {
    /// The current branch, or the short commit when detached.
    pub branch: Option<String>,
    /// Changed files, by absolute path. Folders that contain one are listed
    /// as modified too, so the tree can show where the changes are.
    pub files: HashMap<PathBuf, FileStatus>,
}

/// The branch and changed files of the repository `dir` is in. One `git`
/// process: starting one takes long on Windows, and this runs every time
/// the window gets focus back.
pub fn status(dir: &Path) -> Status {
    // Where git's paths start. Found here rather than asked of git, and kept
    // as the tree spells it (git would resolve symlinks, and the tree's
    // paths wouldn't match).
    let Some(top) = dir.ancestors().find(|d| d.join(".git").exists()) else {
        return Status::default();
    };
    let args = ["status", "--porcelain=v2", "--branch", "-z", "--untracked-files=all"];
    let Some(out) = run(dir, &args) else { return Status::default() };
    let (branch, entries) = parse_porcelain(&out);
    let mut files = HashMap::new();
    for (path, status) in entries {
        let path = top.join(path);
        for dir in path.ancestors().skip(1).take_while(|d| d.starts_with(top) && *d != top) {
            files.entry(dir.to_path_buf()).or_insert(FileStatus::Modified);
        }
        files.insert(path, status);
    }
    Status { branch, files }
}

/// `git status --porcelain=v2 --branch -z`: the branch, then one entry per
/// changed file, each ending in `\0`. Renames are followed by the old path
/// as an entry of its own.
fn parse_porcelain(out: &[u8]) -> (Option<String>, Vec<(String, FileStatus)>) {
    let (mut head, mut oid) = (None, None);
    let mut entries = Vec::new();
    let mut records = out.split(|&b| b == 0).map(String::from_utf8_lossy);
    // The path comes after this many fields, which hold no spaces.
    let path_after =
        |record: &str, fields: usize| record.splitn(fields + 1, ' ').nth(fields).map(str::to_owned);
    while let Some(record) = records.next() {
        let xy = record.get(2..4).unwrap_or("").as_bytes();
        let (x, y) = (xy.first().copied().unwrap_or(b'.'), xy.get(1).copied().unwrap_or(b'.'));
        let entry = match record.as_bytes().first() {
            Some(b'#') => {
                if let Some(name) = record.strip_prefix("# branch.head ") {
                    head = Some(name.to_owned()).filter(|n| n != "(detached)");
                } else if let Some(id) = record.strip_prefix("# branch.oid ") {
                    oid = (id != "(initial)").then(|| id.chars().take(7).collect::<String>());
                }
                continue;
            }
            Some(b'?') => record.get(2..).map(|p| (p.to_owned(), FileStatus::Added)),
            Some(b'u') => path_after(&record, 10).map(|p| (p, FileStatus::Conflict)),
            Some(kind @ (b'1' | b'2')) => {
                let path = path_after(&record, if *kind == b'2' { 9 } else { 8 });
                if *kind == b'2' {
                    records.next();
                }
                match (x, y) {
                    // Deleted files aren't in the tree to mark.
                    (b'D', _) | (_, b'D') => None,
                    (b'A', _) => path.map(|p| (p, FileStatus::Added)),
                    _ => path.map(|p| (p, FileStatus::Modified)),
                }
            }
            _ => None,
        };
        entries.extend(entry);
    }
    (head.or(oid), entries)
}

/// A gutter mark: lines added or changed since the commit, or a spot where
/// lines were deleted (just before line `start`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    Added { start: usize, end: usize },
    Modified { start: usize, end: usize },
    Deleted { at: usize },
}

/// How `text` differs from `base`, line by line, in lines of `text`. Gives
/// up after a moment on huge diffs rather than freeze typing.
pub fn marks(base: &str, text: &str) -> Vec<Mark> {
    let diff = TextDiff::configure()
        .deadline(Instant::now() + Duration::from_millis(50))
        .diff_lines(base, text);
    diff.ops()
        .iter()
        .filter_map(|op| match *op {
            DiffOp::Equal { .. } => None,
            DiffOp::Insert { new_index, new_len, .. } => {
                Some(Mark::Added { start: new_index, end: new_index + new_len })
            }
            DiffOp::Replace { new_index, new_len, .. } => {
                Some(Mark::Modified { start: new_index, end: new_index + new_len })
            }
            DiffOp::Delete { new_index, .. } => Some(Mark::Deleted { at: new_index }),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_marks() {
        let base = "a\nb\nc\nd\n";
        assert!(marks(base, base).is_empty());
        assert_eq!(marks(base, "a\nb\nx\nc\nd\n"), [Mark::Added { start: 2, end: 3 }]);
        assert_eq!(marks(base, "a\nB\nc\nd\n"), [Mark::Modified { start: 1, end: 2 }]);
        assert_eq!(marks(base, "a\nd\n"), [Mark::Deleted { at: 1 }]);
    }

    #[test]
    fn porcelain() {
        let (a, b, c) = ("1".repeat(40), "2".repeat(40), "3".repeat(40));
        let out = format!(
            "# branch.oid 0123456789abcdef0123456789abcdef01234567\0# branch.head main\0\
             1 .M N... 100644 100644 100644 {a} {b} src/a b.rs\0? new file.txt\0\
             2 R. N... 100644 100644 100644 {a} {b} R100 b.rs\0old.rs\0\
             u UU N... 100644 100644 100644 100644 {a} {b} {c} c.rs\0\
             1 .D N... 100644 100644 000000 {a} {b} gone.rs\0\
             1 A. N... 000000 100644 100644 {a} {b} added.rs\0"
        );
        let (branch, entries) = parse_porcelain(out.as_bytes());
        assert_eq!(branch.as_deref(), Some("main"));
        assert_eq!(
            entries,
            [
                ("src/a b.rs".to_owned(), FileStatus::Modified),
                ("new file.txt".to_owned(), FileStatus::Added),
                ("b.rs".to_owned(), FileStatus::Modified),
                ("c.rs".to_owned(), FileStatus::Conflict),
                ("added.rs".to_owned(), FileStatus::Added),
            ]
        );
        // Detached: the short commit.
        let out = b"# branch.oid 0123456789abcdef\0# branch.head (detached)\0";
        assert_eq!(parse_porcelain(out).0.as_deref(), Some("0123456"));
    }

    /// Against a real repository, when git is installed.
    #[test]
    fn real_repository() {
        let dir = std::env::temp_dir().join(format!("bia-git-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src")).unwrap();
        let git = |args: &[&str]| {
            Command::new("git")
                .arg("-C")
                .arg(&dir)
                .args(["-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false"])
                .args(args)
                .output()
                .is_ok_and(|o| o.status.success())
        };
        if !git(&["init", "-q", "-b", "main"]) {
            return; // no git here
        }
        std::fs::write(dir.join("src/a.rs"), "one\r\ntwo\r\n").unwrap();
        assert!(git(&["add", "."]));
        assert!(git(&["commit", "-q", "-m", "x"]));
        std::fs::write(dir.join("src/a.rs"), "one\nTWO\n").unwrap();
        std::fs::write(dir.join("new.txt"), "").unwrap();

        assert_eq!(head_text(&dir.join("src/a.rs")).as_deref(), Some("one\ntwo\n"));
        assert_eq!(head_text(&dir.join("new.txt")), None);
        let status = status(&dir.join("src"));
        assert_eq!(status.branch.as_deref(), Some("main"));
        // Paths as given, the way the file tree has them.
        assert_eq!(status.files.get(&dir.join("src/a.rs")), Some(&FileStatus::Modified));
        assert_eq!(status.files.get(&dir.join("src")), Some(&FileStatus::Modified));
        assert_eq!(status.files.get(&dir.join("new.txt")), Some(&FileStatus::Added));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
