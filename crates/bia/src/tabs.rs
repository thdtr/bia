//! The open files and which one is active.
//!
//! All index bookkeeping lives here (where a new tab goes, which tab
//! becomes active when one closes, what happens to paths after a rename),
//! so `app.rs` never touches indices directly.

use crate::buffer::Buffer;
use bia_core::{fs_ops, tree};
use iced::widget::scrollable::AbsoluteOffset;
use std::path::{Path, PathBuf};

// A handful of tabs at most, so boxing the large variant buys nothing.
#[allow(clippy::large_enum_variant)]
pub enum Body {
    Text(Buffer),
    /// Shown instead of an editor for files BIA refuses to edit.
    Unsupported(String),
}

pub struct Tab {
    pub path: PathBuf,
    pub body: Body,
    /// Where the editor was scrolled to, restored when the tab comes back.
    pub scroll: AbsoluteOffset,
}

impl Tab {
    pub fn new(path: PathBuf, body: Body) -> Self {
        Self { path, body, scroll: AbsoluteOffset::default() }
    }

    pub fn name(&self) -> String {
        tree::display_name(&self.path)
    }

    pub fn buffer(&self) -> Option<&Buffer> {
        match &self.body {
            Body::Text(b) => Some(b),
            Body::Unsupported(_) => None,
        }
    }

    pub fn buffer_mut(&mut self) -> Option<&mut Buffer> {
        match &mut self.body {
            Body::Text(b) => Some(b),
            Body::Unsupported(_) => None,
        }
    }

    pub fn is_dirty(&self) -> bool {
        self.buffer().is_some_and(Buffer::is_dirty)
    }
}

#[derive(Default)]
pub struct Tabs {
    list: Vec<Tab>,
    active: usize,
}

impl Tabs {
    pub fn iter(&self) -> std::slice::Iter<'_, Tab> {
        self.list.iter()
    }

    pub fn iter_mut(&mut self) -> std::slice::IterMut<'_, Tab> {
        self.list.iter_mut()
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    pub fn get_mut(&mut self, index: usize) -> Option<&mut Tab> {
        self.list.get_mut(index)
    }

    pub fn active_index(&self) -> usize {
        self.active
    }

    pub fn active(&self) -> Option<&Tab> {
        self.list.get(self.active)
    }

    pub fn active_mut(&mut self) -> Option<&mut Tab> {
        self.list.get_mut(self.active)
    }

    pub fn position(&self, path: &Path) -> Option<usize> {
        self.list.iter().position(|t| t.path == path)
    }

    /// Makes tab `index` active. Returns false if there is no such tab.
    pub fn select(&mut self, index: usize) -> bool {
        let exists = index < self.list.len();
        if exists {
            self.active = index;
        }
        exists
    }

    /// The next (or previous) tab, wrapping around.
    pub fn neighbor(&self, forward: bool) -> Option<usize> {
        let n = self.list.len();
        (n > 0).then(|| if forward { (self.active + 1) % n } else { (self.active + n - 1) % n })
    }

    /// Opens `tab` right after the active one and makes it active.
    pub fn insert(&mut self, tab: Tab) {
        let at = if self.list.is_empty() { 0 } else { self.active + 1 };
        self.list.insert(at, tab);
        self.active = at;
    }

    /// Closes tab `index`. The active tab stays active; closing the active
    /// tab activates the one that slides into its place (or the new last).
    pub fn remove(&mut self, index: usize) -> Option<Tab> {
        if index >= self.list.len() {
            return None;
        }
        let tab = self.list.remove(index);
        if index < self.active || self.active >= self.list.len() {
            self.active = self.active.saturating_sub(1);
        }
        Some(tab)
    }

    /// Closes every tab for `path` or anything inside it.
    pub fn remove_under(&mut self, path: &Path) {
        let active_path = self.active().map(|t| t.path.clone());
        self.list.retain(|t| !t.path.starts_with(path));
        self.active = active_path.and_then(|p| self.position(&p)).unwrap_or(0);
    }

    pub fn any_dirty(&self) -> bool {
        self.list.iter().any(Tab::is_dirty)
    }

    pub fn dirty_indices(&self) -> Vec<usize> {
        (0..self.list.len()).filter(|&i| self.list[i].is_dirty()).collect()
    }

    /// The first tab under `path` with unsaved changes, if any.
    pub fn dirty_under(&self, path: &Path) -> Option<&Tab> {
        self.list.iter().find(|t| t.path.starts_with(path) && t.is_dirty())
    }

    /// Follows a rename of `from` to `to`. Returns the tabs that moved.
    pub fn renamed(&mut self, from: &Path, to: &Path) -> Vec<usize> {
        let mut changed = Vec::new();
        for (i, tab) in self.list.iter_mut().enumerate() {
            if let Some(new) = fs_ops::moved(&tab.path, from, to) {
                tab.path = new;
                changed.push(i);
            }
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tabs(paths: &[&str]) -> Tabs {
        let mut tabs = Tabs::default();
        for p in paths {
            tabs.insert(Tab::new(PathBuf::from(p), Body::Unsupported(String::new())));
        }
        tabs
    }

    fn names(tabs: &Tabs) -> Vec<String> {
        tabs.iter().map(Tab::name).collect()
    }

    #[test]
    fn new_tabs_open_after_the_active_one() {
        let mut t = tabs(&["/a", "/b"]);
        t.select(0);
        t.insert(Tab::new("/c".into(), Body::Unsupported(String::new())));
        assert_eq!(names(&t), ["a", "c", "b"]);
        assert_eq!(t.active().unwrap().name(), "c");
    }

    #[test]
    fn closing_keeps_a_sensible_active_tab() {
        // Close a tab before the active one: the active tab stays active.
        let mut t = tabs(&["/a", "/b", "/c"]);
        t.remove(0);
        assert_eq!(t.active().unwrap().name(), "c");

        // Close the active tab in the middle: its right neighbor takes over.
        let mut t = tabs(&["/a", "/b", "/c"]);
        t.select(1);
        t.remove(1);
        assert_eq!(t.active().unwrap().name(), "c");

        // Close the last, active tab: the new last becomes active.
        let mut t = tabs(&["/a", "/b"]);
        t.remove(1);
        assert_eq!(t.active().unwrap().name(), "a");

        let mut t = tabs(&["/a"]);
        t.remove(0);
        assert!(t.active().is_none());
        assert!(t.remove(5).is_none());
    }

    #[test]
    fn neighbors_wrap() {
        let mut t = tabs(&["/a", "/b", "/c"]);
        assert_eq!(t.neighbor(true), Some(0));
        t.select(0);
        assert_eq!(t.neighbor(false), Some(2));
        assert_eq!(Tabs::default().neighbor(true), None);
    }

    #[test]
    fn rename_and_delete_follow_folders() {
        let mut t = tabs(&["/w/src/main.rs", "/w/src/ui/mod.rs", "/w/README.md"]);
        t.select(0);
        assert_eq!(t.renamed(Path::new("/w/src"), Path::new("/w/lib")), vec![0, 1]);
        assert_eq!(t.iter().next().unwrap().path, PathBuf::from("/w/lib/main.rs"));

        t.remove_under(Path::new("/w/lib"));
        assert_eq!(names(&t), ["README.md"]);
        assert_eq!(t.active().unwrap().name(), "README.md");
    }
}
