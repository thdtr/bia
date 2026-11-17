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

/// The branch and changed files of the repository `dir` is in.
pub fn status(dir: &Path) -> Status {
    let Some(top) = run(dir, &["rev-parse", "--show-toplevel"]) else {
        return Status::default();
    };
    let top = PathBuf::from(String::from_utf8_lossy(&top).trim());
    let branch = run(dir, &["branch", "--show-current"])
        .map(|b| String::from_utf8_lossy(&b).trim().to_owned())
        .filter(|b| !b.is_empty())
        .or_else(|| {
            run(dir, &["rev-parse", "--short", "HEAD"])
                .map(|b| String::from_utf8_lossy(&b).trim().to_owned())
        });
    let porcelain =
        run(dir, &["status", "--porcelain=v1", "-z", "--untracked-files=all"]).unwrap_or_default();
    let mut files = HashMap::new();
    for (path, status) in parse_porcelain(&porcelain) {
        let path = top.join(path);
        for dir in path.ancestors().skip(1).take_while(|d| d.starts_with(&top) && *d != top) {
            files.entry(dir.to_path_buf()).or_insert(FileStatus::Modified);
        }
        files.insert(path, status);
    }
    Status { branch, files }
}

/// `git status --porcelain=v1 -z`: `XY path\0`, renames followed by the old
/// path as another entry.
fn parse_porcelain(out: &[u8]) -> Vec<(String, FileStatus)> {
    let mut entries = Vec::new();
    let mut parts = out.split(|&b| b == 0).filter(|p| p.len() > 3);
    while let Some(entry) = parts.next() {
        let (x, y) = (entry[0], entry[1]);
        let path = String::from_utf8_lossy(&entry[3..]).into_owned();
        if x == b'R' || x == b'C' {
            parts.next();
        }
        let status = match (x, y) {
            (b'U', _) | (_, b'U') | (b'A', b'A') | (b'D', b'D') => FileStatus::Conflict,
            (b'?', _) | (b'A', _) => FileStatus::Added,
            // Deleted files aren't in the tree to mark.
            (_, b'D') | (b'D', _) => continue,
            _ => FileStatus::Modified,
        };
        entries.push((path, status));
    }
    entries
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
        let out = b" M src/a.rs\0?? new.txt\0R  b.rs\0old.rs\0UU c.rs\0 D gone.rs\0A  added.rs\0";
        assert_eq!(
            parse_porcelain(out),
            [
                ("src/a.rs".to_owned(), FileStatus::Modified),
                ("new.txt".to_owned(), FileStatus::Added),
                ("b.rs".to_owned(), FileStatus::Modified),
                ("c.rs".to_owned(), FileStatus::Conflict),
                ("added.rs".to_owned(), FileStatus::Added),
            ]
        );
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
        let top = PathBuf::from(
            String::from_utf8(run(&dir, &["rev-parse", "--show-toplevel"]).unwrap())
                .unwrap()
                .trim(),
        );
        assert_eq!(status.files.get(&top.join("src/a.rs")), Some(&FileStatus::Modified));
        assert_eq!(status.files.get(&top.join("src")), Some(&FileStatus::Modified));
        assert_eq!(status.files.get(&top.join("new.txt")), Some(&FileStatus::Added));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
