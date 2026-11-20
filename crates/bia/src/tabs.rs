//! The open files, which side of the split each is on, and which one is
//! active.
//!
//! All index bookkeeping lives here (where a new tab goes, which tab
//! becomes active when one closes, what happens to paths after a rename),
//! so `app.rs` never touches indices directly.

use crate::buffer::Buffer;
use bia_core::{fs_ops, tree};
use iced::Vector;
use std::path::{Path, PathBuf};

// A handful of tabs at most, so boxing the large variant buys nothing.
#[allow(clippy::large_enum_variant)]
pub enum Body {
    Text(Buffer),
    /// Shown instead of an editor for files BIA refuses to edit.
    Unsupported(String),
    /// The rendered Markdown of the tab open for the same path, which
    /// keeps it up to date.
    Preview,
}

pub struct Tab {
    pub path: PathBuf,
    pub body: Body,
    /// Where the editor was scrolled to, restored when the tab comes back.
    pub scroll: Vector,
    /// The buffer revision last written to `backups/`, if there's a backup.
    pub backup: Option<u64>,
    pub pane: Pane,
}

impl Tab {
    pub fn new(path: PathBuf, body: Body) -> Self {
        Self { path, body, scroll: Vector::ZERO, backup: None, pane: Pane::Left }
    }

    pub fn name(&self) -> String {
        tree::display_name(&self.path)
    }

    pub fn buffer(&self) -> Option<&Buffer> {
        match &self.body {
            Body::Text(b) => Some(b),
            Body::Unsupported(_) | Body::Preview => None,
        }
    }

    pub fn buffer_mut(&mut self) -> Option<&mut Buffer> {
        match &mut self.body {
            Body::Text(b) => Some(b),
            Body::Unsupported(_) | Body::Preview => None,
        }
    }

    pub fn is_preview(&self) -> bool {
        matches!(self.body, Body::Preview)
    }

    pub fn is_dirty(&self) -> bool {
        self.buffer().is_some_and(Buffer::is_dirty)
    }
}

/// The two sides the editor can split into. Without a split, every tab
/// is on the left.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Pane {
    #[default]
    Left,
    Right,
}

impl Pane {
    fn slot(self) -> usize {
        match self {
            Pane::Left => 0,
            Pane::Right => 1,
        }
    }

    pub fn other(self) -> Pane {
        match self {
            Pane::Left => Pane::Right,
            Pane::Right => Pane::Left,
        }
    }
}

/// Every open tab, in one list, each on a side of the split. Each side has
/// its own active tab, and one side has the focus: "the active tab" is the
/// focused side's.
///
/// Two rules always hold. The right side has tabs only while the left
/// does, and the focus is on a side that has tabs (when there are any).
#[derive(Default)]
pub struct Tabs {
    list: Vec<Tab>,
    /// The active tab of each side, as an index into `list`. Only means
    /// something for a side that has tabs.
    active: [usize; 2],
    focused: Pane,
}

impl Tabs {
    pub fn iter(&self) -> std::slice::Iter<'_, Tab> {
        self.list.iter()
    }

    pub fn iter_mut(&mut self) -> std::slice::IterMut<'_, Tab> {
        self.list.iter_mut()
    }

    /// The tabs of one side, with their indices, in order.
    pub fn in_pane(&self, pane: Pane) -> impl Iterator<Item = (usize, &Tab)> {
        self.list.iter().enumerate().filter(move |(_, t)| t.pane == pane)
    }

    fn has(&self, pane: Pane) -> bool {
        self.list.iter().any(|t| t.pane == pane)
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    pub fn is_split(&self) -> bool {
        self.has(Pane::Right)
    }

    pub fn focused(&self) -> Pane {
        self.focused
    }

    pub fn get_mut(&mut self, index: usize) -> Option<&mut Tab> {
        self.list.get_mut(index)
    }

    pub fn active_index(&self) -> usize {
        self.active[self.focused.slot()]
    }

    /// The active tab of `pane`, with its index.
    pub fn active_in(&self, pane: Pane) -> Option<(usize, &Tab)> {
        let i = self.active[pane.slot()];
        self.list.get(i).filter(|t| t.pane == pane).map(|t| (i, t))
    }

    pub fn active(&self) -> Option<&Tab> {
        self.active_in(self.focused).map(|(_, t)| t)
    }

    pub fn active_in_mut(&mut self, pane: Pane) -> Option<&mut Tab> {
        let i = self.active[pane.slot()];
        self.list.get_mut(i).filter(|t| t.pane == pane)
    }

    pub fn active_mut(&mut self) -> Option<&mut Tab> {
        let i = self.active_index();
        self.list.get_mut(i).filter(|t| t.pane == self.focused)
    }

    /// The tab where `path` is open for editing (not its preview).
    pub fn position(&self, path: &Path) -> Option<usize> {
        self.list.iter().position(|t| t.path == path && !t.is_preview())
    }

    /// The preview tab of `path`, if one is open.
    pub fn preview_of(&self, path: &Path) -> Option<usize> {
        self.list.iter().position(|t| t.path == path && t.is_preview())
    }

    /// Opens `tab` as the active tab of the side that isn't focused, and
    /// leaves the focus where it is.
    pub fn insert_beside(&mut self, mut tab: Tab) {
        let pane = self.focused.other();
        tab.pane = pane;
        let at = match self.active_in(pane) {
            Some((i, _)) => i + 1,
            None => self.list.len(),
        };
        self.list.insert(at, tab);
        for slot in &mut self.active {
            if *slot >= at {
                *slot += 1;
            }
        }
        self.active[pane.slot()] = at;
        self.settle();
    }

    /// Moves the focus to `pane`, if it has tabs.
    pub fn focus(&mut self, pane: Pane) {
        if self.has(pane) {
            self.focused = pane;
        }
    }

    /// Makes tab `index` active, and focuses its side. Returns false if
    /// there is no such tab.
    pub fn select(&mut self, index: usize) -> bool {
        let Some(tab) = self.list.get(index) else { return false };
        self.focused = tab.pane;
        self.active[tab.pane.slot()] = index;
        true
    }

    /// The next (or previous) tab on the focused side, wrapping around.
    pub fn neighbor(&self, forward: bool) -> Option<usize> {
        let side: Vec<usize> = self.in_pane(self.focused).map(|(i, _)| i).collect();
        let n = side.len();
        let at = side.iter().position(|&i| i == self.active_index())?;
        Some(side[if forward { (at + 1) % n } else { (at + n - 1) % n }])
    }

    /// Opens `tab` on the focused side, right after its active tab, and
    /// makes it active.
    pub fn insert(&mut self, mut tab: Tab) {
        let pane = self.focused;
        tab.pane = pane;
        let at = if self.has(pane) { self.active_index() + 1 } else { self.list.len() };
        self.list.insert(at, tab);
        for slot in &mut self.active {
            if *slot >= at {
                *slot += 1;
            }
        }
        self.active[pane.slot()] = at;
    }

    /// Closes tab `index`. Each side keeps its active tab; closing an
    /// active tab activates the one that slides into its place on that
    /// side, or else the one before it.
    pub fn remove(&mut self, index: usize) -> Option<Tab> {
        if index >= self.list.len() {
            return None;
        }
        let tab = self.list.remove(index);
        let slot = tab.pane.slot();
        let was_active = self.active[slot] == index;
        for active in &mut self.active {
            if *active > index {
                *active -= 1;
            }
        }
        if was_active {
            let side: Vec<usize> = self.in_pane(tab.pane).map(|(i, _)| i).collect();
            if let Some(&next) = side.iter().find(|&&i| i >= index).or(side.last()) {
                self.active[slot] = next;
            }
        }
        self.settle();
        Some(tab)
    }

    /// Closes every tab for `path` or anything inside it.
    pub fn remove_under(&mut self, path: &Path) {
        let kept: Vec<Option<PathBuf>> = [Pane::Left, Pane::Right]
            .map(|p| self.active_in(p).map(|(_, t)| t.path.clone()))
            .into();
        self.list.retain(|t| !t.path.starts_with(path));
        for (slot, pane) in [Pane::Left, Pane::Right].into_iter().enumerate() {
            let same = kept[slot]
                .as_ref()
                .and_then(|p| self.list.iter().position(|t| t.pane == pane && &t.path == p));
            let first = self.in_pane(pane).map(|(i, _)| i).next();
            self.active[slot] = same.or(first).unwrap_or(0);
        }
        self.settle();
    }

    /// Ctrl+\: moves tab `index` to the other side. The last tab on the
    /// left stays put, since the left side is never empty while the right
    /// has tabs.
    pub fn move_to_other(&mut self, index: usize) -> bool {
        let Some(tab) = self.list.get(index) else { return false };
        let from = tab.pane;
        if from == Pane::Left && self.in_pane(Pane::Left).count() == 1 {
            return false;
        }
        self.list[index].pane = from.other();
        let side: Vec<usize> = self.in_pane(from).map(|(i, _)| i).collect();
        if let Some(&next) = side.iter().find(|&&i| i >= index).or(side.last()) {
            self.active[from.slot()] = next;
        }
        self.focused = from.other();
        self.active[self.focused.slot()] = index;
        self.settle();
        true
    }

    /// Puts the two rules back after tabs closed or moved.
    fn settle(&mut self) {
        if !self.has(Pane::Left) && self.has(Pane::Right) {
            for tab in &mut self.list {
                tab.pane = Pane::Left;
            }
            self.active[0] = self.active[1];
        }
        if !self.has(self.focused) {
            self.focused = Pane::Left;
        }
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

    fn side(tabs: &Tabs, pane: Pane) -> Vec<String> {
        tabs.in_pane(pane).map(|(_, t)| t.name()).collect()
    }

    #[test]
    fn splitting_moves_tabs_between_sides() {
        let mut t = tabs(&["/a", "/b", "/c"]);
        // The last tab on the left can't leave it.
        assert!(t.move_to_other(2));
        assert_eq!(side(&t, Pane::Left), ["a", "b"]);
        assert_eq!(side(&t, Pane::Right), ["c"]);
        assert_eq!(t.focused(), Pane::Right);
        assert_eq!(t.active().unwrap().name(), "c");
        assert_eq!(t.active_in(Pane::Left).unwrap().1.name(), "b");

        // New tabs open on the focused side.
        t.insert(Tab::new("/d".into(), Body::Unsupported(String::new())));
        assert_eq!(side(&t, Pane::Right), ["c", "d"]);
        assert_eq!(t.neighbor(true).map(|i| t.iter().nth(i).unwrap().name()), Some("c".into()));

        // Closing the right side's tabs brings the focus back left.
        let d = t.position(Path::new("/d")).unwrap();
        t.remove(d);
        let c = t.position(Path::new("/c")).unwrap();
        t.remove(c);
        assert!(!t.is_split());
        assert_eq!(t.focused(), Pane::Left);
        assert_eq!(t.active().unwrap().name(), "b");

        // Emptying the left side moves the right side over.
        let mut t = tabs(&["/a", "/b"]);
        t.move_to_other(1);
        t.remove(0);
        assert!(!t.is_split());
        assert_eq!(t.active().unwrap().name(), "b");

        let mut t = tabs(&["/a"]);
        assert!(!t.move_to_other(0));
    }

    #[test]
    fn previews_open_beside_their_file() {
        let mut t = tabs(&["/a.md", "/b.md"]);
        t.insert_beside(Tab::new("/b.md".into(), Body::Preview));
        assert_eq!(side(&t, Pane::Right), ["b.md"]);
        assert_eq!(t.focused(), Pane::Left);
        assert_eq!(t.active().unwrap().name(), "b.md");
        assert!(!t.active().unwrap().is_preview());
        // The file and its preview are told apart.
        assert_eq!(t.position(Path::new("/b.md")), Some(1));
        assert_eq!(t.preview_of(Path::new("/b.md")), Some(2));
        assert_eq!(t.preview_of(Path::new("/a.md")), None);
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
