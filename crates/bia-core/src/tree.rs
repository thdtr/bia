//! The explorer model: a lazily-read folder tree flattened into rows.
//!
//! Only expanded folders are ever read from disk, so opening a huge
//! repository costs one `read_dir`. Every folder gets its own row (BIA
//! never merges single-child folders into "compact folders"), and folders are
//! listed before files, each group sorted case-insensitively.

use std::cmp::Ordering;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub path: PathBuf,
    pub name: String,
    pub is_dir: bool,
    /// 0 for direct children of the root.
    pub depth: usize,
    /// For folders: whether it is expanded.
    pub expanded: bool,
}

#[derive(Debug, Clone)]
pub struct FileTree {
    root: PathBuf,
    expanded: HashSet<PathBuf>,
    hidden: Vec<String>,
    rows: Vec<Row>,
}

impl FileTree {
    pub fn new(root: impl Into<PathBuf>, hidden: Vec<String>) -> Self {
        let mut tree =
            Self { root: root.into(), expanded: HashSet::new(), hidden, rows: Vec::new() };
        tree.refresh();
        tree
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Display name of the workspace (the root folder's name).
    pub fn root_name(&self) -> String {
        display_name(&self.root)
    }

    pub fn rows(&self) -> &[Row] {
        &self.rows
    }

    pub fn is_expanded(&self, dir: &Path) -> bool {
        self.expanded.contains(dir)
    }

    pub fn set_hidden(&mut self, hidden: Vec<String>) {
        if hidden != self.hidden {
            self.hidden = hidden;
            self.refresh();
        }
    }

    pub fn toggle(&mut self, dir: &Path) {
        if !self.expanded.remove(dir) {
            self.expanded.insert(dir.to_path_buf());
        }
        self.refresh();
    }

    /// Expands every folder between the root and `path` so it becomes visible.
    pub fn reveal(&mut self, path: &Path) {
        let mut changed = false;
        let mut current = path.parent();
        while let Some(dir) = current {
            if !dir.starts_with(&self.root) || dir == self.root {
                break;
            }
            changed |= self.expanded.insert(dir.to_path_buf());
            current = dir.parent();
        }
        if changed {
            self.refresh();
        }
    }

    /// Re-reads every expanded folder from disk. Call after file operations
    /// or when the window regains focus.
    pub fn refresh(&mut self) {
        self.expanded.retain(|p| p.is_dir());
        let mut rows = Vec::with_capacity(self.rows.len());
        self.walk(&self.root, 0, &mut rows);
        self.rows = rows;
    }

    /// Keeps expansion state in sync after a rename.
    pub fn renamed(&mut self, from: &Path, to: &Path) {
        let moved: Vec<PathBuf> =
            self.expanded.iter().filter(|p| p.starts_with(from)).cloned().collect();
        for old in moved {
            self.expanded.remove(&old);
            if let Ok(rest) = old.strip_prefix(from) {
                self.expanded.insert(to.join(rest));
            }
        }
        self.refresh();
    }

    fn walk(&self, dir: &Path, depth: usize, out: &mut Vec<Row>) {
        for row in self.read(dir, depth) {
            let recurse = row.is_dir && row.expanded;
            let path = row.path.clone();
            out.push(row);
            if recurse {
                self.walk(&path, depth + 1, out);
            }
        }
    }

    fn read(&self, dir: &Path, depth: usize) -> Vec<Row> {
        let Ok(entries) = fs::read_dir(dir) else { return Vec::new() };
        let mut rows: Vec<Row> = entries
            .flatten()
            .filter_map(|entry| {
                let name = entry.file_name().to_string_lossy().into_owned();
                if is_hidden(&name, &self.hidden) {
                    return None;
                }
                let path = entry.path();
                // `metadata` follows symlinks, so a link to a folder is a folder.
                let is_dir = fs::metadata(&path).map(|m| m.is_dir()).unwrap_or(false);
                let expanded = is_dir && self.expanded.contains(&path);
                Some(Row { path, name, is_dir, depth, expanded })
            })
            .collect();
        rows.sort_by(compare);
        rows
    }
}

fn compare(a: &Row, b: &Row) -> Ordering {
    b.is_dir
        .cmp(&a.is_dir)
        .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        .then_with(|| a.name.cmp(&b.name))
}

/// Every file under `root` that git wouldn't ignore: `.gitignore`,
/// `.ignore`, `.git/info/exclude` and the global excludes file all apply,
/// and so do the `hidden` names. Dotfiles are listed (`.github`, `.env`).
/// Links to folders aren't followed, since they can loop.
pub fn walk_files(root: &Path, hidden: &[String]) -> impl Iterator<Item = PathBuf> {
    let hidden = hidden.to_vec();
    ignore::WalkBuilder::new(root)
        .hidden(false)
        // A `.gitignore` counts even outside a git repository, as it does
        // in other editors.
        .require_git(false)
        .follow_links(false)
        .filter_entry(move |e| {
            e.depth() == 0 || !is_hidden(&e.file_name().to_string_lossy(), &hidden)
        })
        .build()
        .flatten()
        .filter(|e| match e.file_type() {
            Some(t) if t.is_file() => true,
            // A link to a file counts; a link to a folder doesn't.
            Some(t) if t.is_symlink() => fs::metadata(e.path()).is_ok_and(|m| m.is_file()),
            _ => false,
        })
        .map(ignore::DirEntry::into_path)
}

/// [`walk_files`] as paths relative to `root` with `/` separators, sorted.
/// Stops after `limit` files; the flag says whether it did.
pub fn list_files(root: &Path, hidden: &[String], limit: usize) -> (Vec<String>, bool) {
    let mut files = Vec::new();
    for path in walk_files(root, hidden) {
        if files.len() == limit {
            files.sort();
            return (files, true);
        }
        if let Ok(relative) = path.strip_prefix(root) {
            let parts: Vec<_> = relative.iter().map(|p| p.to_string_lossy()).collect();
            files.push(parts.join("/"));
        }
    }
    files.sort();
    (files, false)
}

/// `name` matches when equal to a pattern, or when the pattern is `*.ext`
/// and the name ends with `.ext`.
pub fn is_hidden(name: &str, patterns: &[String]) -> bool {
    patterns.iter().any(|p| match p.strip_prefix('*') {
        Some(suffix) if !suffix.is_empty() => name.ends_with(suffix),
        _ => p == name,
    })
}

/// Last path component, or the whole path for roots like `/`.
pub fn display_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

/// Rejects names that would escape the folder or that no OS accepts.
pub fn valid_name(name: &str) -> bool {
    let name = name.trim();
    !name.is_empty()
        && name != "."
        && name != ".."
        && !name.contains(['/', '\\', '\0'])
        && !(cfg!(windows) && name.contains([':', '*', '?', '"', '<', '>', '|']))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("bia-tree-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn folders_first_no_compaction_lazy() {
        let root = scratch("order");
        fs::create_dir_all(root.join("src/deep/er")).unwrap();
        fs::write(root.join("b.txt"), "").unwrap();
        fs::write(root.join("A.md"), "").unwrap();
        fs::create_dir(root.join(".git")).unwrap();

        let mut tree = FileTree::new(&root, vec![".git".into()]);
        let names: Vec<_> = tree.rows().iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, ["src", "A.md", "b.txt"]);

        tree.toggle(&root.join("src"));
        tree.toggle(&root.join("src/deep"));
        let rows: Vec<_> = tree.rows().iter().map(|r| (r.name.as_str(), r.depth)).collect();
        // `src/deep/er` is three rows, never one "src/deep/er" row.
        assert_eq!(rows, [("src", 0), ("deep", 1), ("er", 2), ("A.md", 0), ("b.txt", 0)]);

        tree.toggle(&root.join("src"));
        assert_eq!(tree.rows().len(), 3);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn reveal_expands_ancestors() {
        let root = scratch("reveal");
        fs::create_dir_all(root.join("a/b")).unwrap();
        fs::write(root.join("a/b/c.rs"), "").unwrap();
        let mut tree = FileTree::new(&root, vec![]);
        tree.reveal(&root.join("a/b/c.rs"));
        assert!(tree.rows().iter().any(|r| r.name == "c.rs" && r.depth == 2));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn lists_files_recursively() {
        let root = scratch("list");
        fs::create_dir_all(root.join("src/ui")).unwrap();
        fs::create_dir_all(root.join("target/debug")).unwrap();
        fs::write(root.join("src/ui/mod.rs"), "").unwrap();
        fs::write(root.join("src/main.rs"), "").unwrap();
        fs::write(root.join("target/debug/bia"), "").unwrap();
        fs::write(root.join("README.md"), "").unwrap();
        let hidden = vec!["target".to_string()];
        let (files, truncated) = list_files(&root, &hidden, 100);
        assert_eq!(files, ["README.md", "src/main.rs", "src/ui/mod.rs"]);
        assert!(!truncated);
        let (files, truncated) = list_files(&root, &hidden, 2);
        assert_eq!(files.len(), 2);
        assert!(truncated);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn listing_follows_gitignore() {
        let root = scratch("gitignore");
        fs::create_dir_all(root.join("dist")).unwrap();
        fs::create_dir_all(root.join(".github")).unwrap();
        fs::write(root.join(".gitignore"), "dist/\n*.log\n").unwrap();
        fs::write(root.join("dist/app.js"), "").unwrap();
        fs::write(root.join("debug.log"), "").unwrap();
        fs::write(root.join(".github/ci.yml"), "").unwrap();
        fs::write(root.join("main.rs"), "").unwrap();
        let (files, _) = list_files(&root, &[], 100);
        assert_eq!(files, [".github/ci.yml", ".gitignore", "main.rs"]);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn hidden_patterns() {
        let p = vec!["target".to_string(), "*.lock".to_string()];
        assert!(is_hidden("target", &p));
        assert!(is_hidden("Cargo.lock", &p));
        assert!(!is_hidden("targets", &p));
        assert!(!is_hidden("lock", &p));
    }

    #[test]
    fn names() {
        assert!(valid_name("main.rs"));
        assert!(valid_name(".env"));
        for bad in ["", " ", ".", "..", "a/b", "a\\b"] {
            assert!(!valid_name(bad), "{bad:?}");
        }
    }
}
