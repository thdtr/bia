//! Application state and the update loop.
//!
//! BIA follows iced's Elm architecture: [`Bia`] is the whole state,
//! [`Message`] every event, [`Bia::update`] the only place state changes and
//! `ui::view` a pure function of the state.
//!
//! This module only orchestrates. The pieces it coordinates live elsewhere:
//! open files in `tabs.rs`, one file's text in `buffer.rs`, file operations
//! in `bia_core::fs_ops`, the editor widget in `code_view`.

use crate::buffer::{Buffer, LspAction, Reply};
use crate::code_view::{Input, Pick};
use crate::fonts;
use crate::keys::{self, Shortcut};
use crate::lsp::{Effect, Lsp};
use crate::syntax;
use crate::tabs::{Body, Pane, Tab, Tabs};
use crate::terminal::{TermEvent, Terminal, Terminals};
use crate::ui::{self, style::Colors};
use bia_core::backup;
use bia_core::document::{self, Document, LoadError};
use bia_core::fs_ops::{self, FsError};
use bia_core::fuzzy;
use bia_core::git;
use bia_core::indent::Indent;
use bia_core::language::{
    self,
    syntect::{highlighting::Theme as SyntaxTheme, parsing::SyntaxSet},
};
use bia_core::search::{self, Found, Matcher, QueryError};
use bia_core::settings::{Settings, SettingsError};
use bia_core::state::{OpenFile, Session, State};
use bia_core::text;
use bia_core::theme::Theme as BiaTheme;
use bia_core::tree::{self, FileTree};
use bia_core::{I18n, Lang, editing, paths};
use iced::widget::{operation, svg};
use iced::{Event, Font, Subscription, Task, Vector, event, keyboard, window};
use std::collections::HashMap;
use std::fs;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub const EDITOR_ID: &str = "bia-editor";
/// The editor on the right side of a split.
pub const EDITOR_RIGHT_ID: &str = "bia-editor-right";
pub const PROMPT_ID: &str = "bia-prompt";
pub const TREE_ID: &str = "bia-tree";
pub const FIND_ID: &str = "bia-find";
pub const REPLACE_ID: &str = "bia-replace";
pub const PALETTE_ID: &str = "bia-palette";
pub const SEARCH_ID: &str = "bia-search";
pub const TERMINAL_ID: &str = "bia-terminal";

/// Notices fade on the next interaction after this long.
const NOTICE_TTL: Duration = Duration::from_secs(4);

/// Rows shown by "open file by name".
pub const PALETTE_ROWS: usize = 12;

/// "Open file by name" and "find in files" stop listing a workspace after
/// this many files.
const MAX_FILES: usize = 50_000;

/// "Find in files" stops after this many matches.
const MAX_HITS: usize = 2_000;

/// Edits go to the language server this long after typing stops.
const LSP_DELAY: Duration = Duration::from_millis(150);

/// Unsaved edits are written to `backups/` this long after the last one.
const BACKUP_DELAY: Duration = Duration::from_secs(2);

pub fn run(paths: Vec<PathBuf>, new: bool) -> iced::Result {
    // The window comes back the size, place and shape it was.
    let layout = State::load(&paths::state_file()).layout;
    let size = layout.window_size.map_or(iced::Size::new(1180.0, 760.0), |(w, h)| {
        iced::Size::new(w.max(520.0), h.max(320.0))
    });
    // A second window lands a little lower and to the right of the first.
    let nudge = if new { 32.0 } else { 0.0 };
    let position = match layout.window_position {
        Some((x, y)) => window::Position::Specific(iced::Point::new(x + nudge, y + nudge)),
        None => window::Position::default(),
    };
    iced::application(move || Bia::new(paths.clone(), new), Bia::update, ui::view)
        .default_font(Font::with_name(fonts::UI_FAMILY))
        .title(Bia::title)
        .theme(Bia::theme)
        .subscription(Bia::subscription)
        .window(window::Settings {
            size,
            position,
            maximized: layout.maximized,
            min_size: Some(iced::Size::new(520.0, 320.0)),
            icon: ui::style::window_icon(),
            exit_on_close_request: false,
            #[cfg(target_os = "linux")]
            platform_specific: window::settings::PlatformSpecific {
                application_id: "bia".into(),
                ..Default::default()
            },
            ..window::Settings::default()
        })
        .run()
}

#[derive(Debug, Clone)]
pub enum Message {
    OpenFolder,
    FolderPicked(Option<PathBuf>),
    OpenRecent(PathBuf),
    BackupTick,
    /// A backup of a file at a revision was written, or failed to be.
    BackupWritten(PathBuf, u64, bool),
    /// A file's text as of the last commit (`None`: not committed).
    GitBase(PathBuf, Option<String>),
    /// The branch and changed files of the folder that's open.
    GitStatus(PathBuf, Arc<git::Status>),
    /// Where language server threads send their events.
    LspChannel(iced::futures::channel::mpsc::UnboundedSender<bia_core::lsp::Event>),
    Lsp(bia_core::lsp::Event),
    /// Time to send edited files to their language servers.
    LspSync,
    /// Typing paused: time to bring find matches, the preview and the git
    /// marks up to date (see [`Bia::follow_edits`]).
    FollowEdits,
    /// A git diff made in the background: buffer id, revision, marks.
    GitMarks(u64, u64, Vec<git::Mark>),
    /// Find matches counted in the background.
    FindDone(Arc<FindResult>),
    /// The open folder's files, listed in the background.
    FilesListed(Arc<FileList>),
    TermChannel(iced::futures::channel::mpsc::UnboundedSender<(u64, TermEvent)>),
    Term(u64, TermEvent),
    TermResize(u64, crate::terminal::Size),
    /// Time to see which program runs in front of each terminal.
    LookAtTerminals,
    ToggleTerminal,
    NewTerminal,
    SelectTerminal(u64),
    CloseTerminal(u64),
    /// Ctrl+click on a file location in terminal `id`.
    TermOpen(u64, String),
    ToggleSidebar,
    /// A divider was pressed, to drag it.
    DragStart(Splitter),
    Dragged(iced::Point),
    DragEnd,
    WindowResized(window::Id, iced::Size),
    WindowMoved(iced::Point),
    /// Whether the window is maximized, asked after it changed size.
    Maximized(bool),
    /// A file or folder dropped on the window.
    FileDropped(PathBuf),
    /// A command from the palette, the menu or a button.
    Run(crate::commands::Command),
    /// Where "Save as" should write tab `.0` (`None`: cancelled), and
    /// whether to close the tab after.
    SaveAsPicked(PathBuf, Option<PathBuf>, bool),
    /// Files chosen with "Open file".
    FilesPicked(Option<Vec<PathBuf>>),
    /// A right click on tab `.0`. A [`Message::TabMenu`] follows.
    TabTarget(usize),
    TabMenu(iced::Point),
    /// Closes tabs on the side of tab `.0`.
    CloseTabs(usize, CloseWhich),
    /// The menu button in the status bar.
    MainMenu,
    CloseOverlay,
    /// Puts this text on the clipboard and says so.
    Copy(String),
    /// The language server didn't format this file in time to save it.
    FormatTimedOut(PathBuf),
    /// The window lost the focus.
    WindowUnfocused,
    /// The file tree scrolled: offset and height of what's in sight.
    TreeScrolled(f32, f32),
    /// The mouse went down on a row of the tree, so a click or a drag starts.
    RowDown(PathBuf),
    /// The mouse is over a row of the tree.
    RowEnter(PathBuf),
    /// The mouse left the tree.
    TreeLeave,
    /// The mouse button came up after a [`Message::RowDown`].
    TreeRelease,
    /// A right click on a row. A [`Message::TreeMenu`] follows.
    RowTarget(PathBuf),
    /// A right click in the sidebar, at this point of the window.
    TreeMenu(iced::Point),
    CloseMenu,
    Tree(TreeAction),
    /// What the clipboard had when you pasted into folder `.0`.
    PasteFiles(PathBuf, Option<String>),
    ModifiersChanged(keyboard::Modifiers),
    /// Something done in the background failed, and says why.
    Failed(Option<String>),
    PromptInput(String),
    PromptSubmit,
    SelectTab(usize),
    CloseTab(usize),
    /// What the editor on one side of the split was told to do.
    Input(Pane, Input),
    Save,
    TogglePreview,
    LinkClicked(String),
    FindInput(String),
    FindSubmit,
    FindOption(FindOption),
    ShowPanel(Panel),
    SearchInput(String),
    SearchInclude(String),
    SearchExclude(String),
    SearchSubmit,
    SearchOption(FindOption),
    SearchDone(u64, Arc<Found>),
    SearchOpen(usize, usize),
    ReplaceInput(String),
    ReplaceOne,
    ReplaceAll,
    PaletteInput(String),
    PaletteSubmit,
    PalettePick(usize),
    PaletteClose,
    Shortcut(Shortcut),
    WindowFocused,
    CloseRequested,
    Dialog(Choice),
    OpenSettings,
    EditorScrolled(Pane, Vector),
    /// Ctrl+\: moves a tab to the other side of the split.
    MoveToOtherSide(usize),
}

/// What you can do to a file or folder in the tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TreeAction {
    NewFile(PathBuf),
    NewFolder(PathBuf),
    Rename(PathBuf),
    Duplicate(PathBuf),
    Trash(PathBuf),
    Cut(PathBuf),
    Copy(PathBuf),
    /// Pastes into this folder.
    Paste(PathBuf),
    CopyPath(PathBuf),
    CopyRelativePath(PathBuf),
    /// Shows it in the system's file manager.
    Reveal(PathBuf),
    /// A new terminal in this folder.
    OpenTerminal(PathBuf),
    CollapseAll,
    Refresh,
}

/// A row being dragged in the tree.
pub struct TreeDrag {
    pub from: PathBuf,
    /// The row under the mouse now.
    pub over: Option<PathBuf>,
    /// The mouse went over another row, so this is a drag, not a click.
    pub moved: bool,
}

/// Which tabs "close others" and its kin close, next to a tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseWhich {
    Others,
    ToTheRight,
    Saved,
    All,
}

/// What a card over everything shows, about BIA or the shortcuts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Overlay {
    About,
    Shortcuts,
}

/// What happens after unsaved files are saved or discarded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum After {
    Quit,
    CloseFolder,
}

/// The dividers you can drag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Splitter {
    /// Between the sidebar and the editor.
    Sidebar,
    /// Above the terminal panel.
    Terminal,
}

/// A divider being dragged, with where the mouse was when it started to move
/// and the size it had then.
struct Drag {
    splitter: Splitter,
    from: Option<iced::Point>,
    size: f32,
}

pub const SIDEBAR_WIDTH: f32 = 248.0;
pub const TERMINAL_HEIGHT: f32 = 260.0;

/// A question that blocks everything else until it is answered.
pub enum Dialog {
    /// A program is running in a terminal that is about to close.
    Busy(Closing),
    /// Quitting or closing the folder with unsaved files. Save all,
    /// discard, cancel.
    Unsaved(After),
    /// Closing a tab with unsaved changes: save, discard, cancel.
    Close(PathBuf),
    /// Moving a file or folder to the trash. Trash or cancel.
    Trash(PathBuf),
    /// Saving a file that changed on disk since it was opened: overwrite,
    /// load the disk version, cancel.
    Conflict(PathBuf),
}

/// What would stop a program running in a terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Closing {
    Quit,
    Terminal(u64),
}

/// The answer to a [`Dialog`]. What `Save` and `Discard` mean depends on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Choice {
    Save,
    Discard,
    Cancel,
}

/// The name field that opens in the tree to create or rename something.
pub struct Prompt {
    pub kind: PromptKind,
    pub value: String,
    pub error: Option<String>,
}

pub enum PromptKind {
    /// A new file in this folder.
    NewFile(PathBuf),
    NewFolder(PathBuf),
    Rename(PathBuf),
}

pub struct Find {
    pub query: String,
    pub options: search::Options,
    pub matches: Vec<Range<usize>>,
    pub current: Option<usize>,
    /// Why the query can't run: an invalid regular expression.
    pub error: Option<String>,
    /// The replacement text, while the replace field is shown (Ctrl+H).
    pub replace: Option<String>,
    /// The buffer and revision `matches` were counted in, and those of a
    /// search still running.
    searched: Option<(u64, u64)>,
    running: Option<(u64, u64)>,
}

impl Find {
    fn matcher(&self) -> Result<Matcher, QueryError> {
        Matcher::new(&self.query, self.options)
    }

    /// The query or its options changed: count again.
    fn changed(&mut self) {
        self.searched = None;
        self.running = None;
    }
}

/// What a find in the background found, and what it looked for.
#[derive(Debug)]
pub struct FindResult {
    buffer: u64,
    revision: u64,
    query: String,
    options: search::Options,
    matches: Vec<Range<usize>>,
}

/// Up to this size, find matches, the Markdown preview and the git marks
/// follow every keystroke. Bigger files get them once typing pauses, and
/// the slow parts run off the UI thread.
const LIVE_LIMIT: usize = 256 * 1024;
const FOLLOW_DELAY: Duration = Duration::from_millis(150);

/// When the active file was last edited, to wait for a pause in typing.
struct Follow {
    /// The buffer and revision seen after the last message.
    seen: Option<(u64, u64)>,
    edited_at: Instant,
    scheduled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FindOption {
    Case,
    Word,
    Regex,
}

/// What the sidebar shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    Files,
    Search,
}

/// Find in files (Ctrl+Shift+F).
#[derive(Default)]
pub struct ProjectSearch {
    pub query: String,
    /// Files to look in, and files to leave out (see `text::path_filter`).
    pub include: String,
    pub exclude: String,
    pub options: search::Options,
    pub found: Option<Arc<Found>>,
    pub running: bool,
    pub error: Option<String>,
    /// Bumped by every search, so a slow one that finishes late is dropped.
    generation: u64,
}

/// The box at the top of the window for Ctrl+P and Ctrl+G.
pub struct Palette {
    pub mode: PaletteMode,
    pub query: String,
    /// The highlighted row of `results`.
    pub selected: usize,
}

pub enum PaletteMode {
    /// Open a file by name. `files` is `None` while they're being listed.
    Files { files: Option<Arc<FileList>>, results: Vec<String> },
    /// Go to a line of the active file.
    Line,
    /// A new name for the symbol at `at` in `path` (language server).
    Rename { path: PathBuf, at: usize },
    /// Ctrl+Shift+P runs a command by name.
    Commands { results: Vec<crate::commands::Command> },
    /// A folder opened before.
    Recent { results: Vec<PathBuf> },
    /// A saved version of `path` to bring back.
    History { path: PathBuf, versions: Vec<bia_core::history::Version> },
}

/// The files of the open folder, for "open file by name". Listed in the
/// background and kept until something may have changed them: a file
/// operation, or coming back from another app.
#[derive(Debug)]
pub struct FileList {
    pub root: PathBuf,
    /// Relative to `root`, with `/` separators, sorted.
    pub files: Vec<String>,
    /// The folder has more than [`MAX_FILES`].
    pub truncated: bool,
}

pub struct Notice {
    pub text: String,
    pub error: bool,
    at: Instant,
}

/// Everything derived from settings, rebuilt as a whole when they change.
pub struct Look {
    pub i18n: I18n,
    pub colors: Colors,
    pub iced_theme: iced::Theme,
    pub syntax_theme: Arc<SyntaxTheme>,
    pub syntaxes: Arc<SyntaxSet>,
    pub editor_font: Font,
    pub ui_font: Font,
    /// The terminal's font, its own or the editor's.
    pub terminal_font: Font,
    /// The logo, painted in the theme's colors.
    pub logo: svg::Handle,
    /// Bumped on every rebuild so highlighters know to start over.
    pub generation: u64,
}

impl Look {
    fn load(settings: &Settings, generation: u64) -> (Self, Vec<String>) {
        let mut errors = Vec::new();
        let i18n = I18n::new(Lang::from_setting(&settings.language));
        let (theme, theme_error) = BiaTheme::load(&settings.theme, &paths::themes_dir());
        if let Some(e) = theme_error {
            errors.push(i18n.f("notice.theme_error", &[("error", &e)]));
        }
        let (syntaxes, grammar_errors) = language::load_syntaxes(&paths::languages_dir());
        errors.extend(grammar_errors);
        let colors = Colors::from_theme(&theme);
        let editor_font = fonts::from_setting(&settings.font_family);
        let look = Self {
            iced_theme: colors.iced_theme(&theme.name),
            syntax_theme: Arc::new(language::syntax_theme(&theme)),
            syntaxes: Arc::new(syntaxes),
            colors,
            editor_font,
            ui_font: fonts::from_setting(&settings.ui_font_family),
            terminal_font: settings
                .terminal
                .font_family
                .as_deref()
                .map_or(editor_font, fonts::from_setting),
            logo: ui::style::logo(colors),
            generation,
            i18n,
        };
        (look, errors)
    }
}

pub struct Bia {
    pub settings: Settings,
    pub look: Look,
    pub tree: Option<FileTree>,
    /// Where the file tree is scrolled and how tall it is, to build only
    /// the rows in sight.
    pub tree_viewport: (f32, f32),
    pub selected: Option<PathBuf>,
    pub tabs: Tabs,
    /// The name field open in the tree, if any.
    pub prompt: Option<Prompt>,
    pub menu: Option<ui::menu::Menu>,
    /// The row a right click landed on, until the menu opens for it.
    menu_target: Option<PathBuf>,
    pub tree_drag: Option<TreeDrag>,
    /// The row under the mouse.
    pub tree_hover: Option<PathBuf>,
    /// A file or folder cut (`true`) or copied in the tree, until pasted.
    file_clipboard: Option<(PathBuf, bool)>,
    modifiers: keyboard::Modifiers,
    pub find: Option<Find>,
    pub palette: Option<Palette>,
    pub notice: Option<Notice>,
    pub sidebar: bool,
    pub panel: Panel,
    pub project_search: ProjectSearch,
    /// What git says about the open folder; empty outside a repository.
    pub git: Arc<git::Status>,
    pub lsp: Lsp,
    /// The shells in the bottom panel. They keep running while the panel
    /// is hidden.
    pub terminals: Terminals,
    term_events: Option<iced::futures::channel::mpsc::UnboundedSender<(u64, TermEvent)>>,
    pub sidebar_width: f32,
    pub terminal_height: f32,
    drag: Option<Drag>,
    /// The size of the window's content.
    window: iced::Size,
    window_id: Option<window::Id>,
    /// The size before the window was maximized, which is what it comes
    /// back to next time.
    normal_size: iced::Size,
    window_position: Option<iced::Point>,
    maximized: bool,
    pub overlay: Option<Overlay>,
    /// Files waiting for the language server to format them before they're
    /// saved.
    format_then_save: std::collections::HashSet<PathBuf>,
    /// An autosave is running (see [`Bia::autosave`]).
    saving_quietly: bool,
    /// Places the cursor was before a jump, for Back, and after going back,
    /// for Forward.
    back: Vec<(PathBuf, usize)>,
    forward: Vec<(PathBuf, usize)>,
    /// When the last place was remembered. A jump that goes through two
    /// steps (open, then go to a line) is remembered once.
    remembered_at: Option<Instant>,
    navigating: bool,
    /// Tabs closed lately, newest last, with their cursor, for Ctrl+Shift+T.
    closed: Vec<(PathBuf, usize, usize)>,
    /// The tab a right click landed on, until its menu opens.
    tab_target: Option<usize>,
    pub dialog: Option<Dialog>,
    /// `font_size` as settings.json has it, while zoom changes the live one.
    font_size_setting: f32,
    /// Folders opened before, most recent first, for the welcome screen.
    pub recent: Vec<PathBuf>,
    /// The workspace is a folder you chose (rather than a file's folder),
    /// so its open files are remembered.
    remember_root: bool,
    backup_scheduled: bool,
    backups: backup::Writer,
    files: Option<Arc<FileList>>,
    /// A listing of this folder is running.
    listing: Option<PathBuf>,
    follow: Follow,
    /// Numbers requests to center the cursor (see `Buffer::center`).
    center_seq: u64,
}

impl Bia {
    /// `paths` come from the command line. The first folder becomes the
    /// workspace (or the first file's folder), and files open in tabs.
    pub fn new(paths: Vec<PathBuf>, new_window: bool) -> (Self, Task<Message>) {
        let (settings, settings_error) = Settings::load_or_create(&paths::settings_file());
        let (look, errors) = Look::load(&settings, 0);
        let state = State::load(&paths::state_file());
        let mut app = Self {
            recent: state.recent_folders(),
            remember_root: false,
            backup_scheduled: false,
            backups: backup::Writer::new(paths::backups_dir()),
            files: None,
            listing: None,
            follow: Follow { seen: None, edited_at: Instant::now(), scheduled: false },
            center_seq: 0,
            font_size_setting: settings.font_size,
            settings,
            look,
            tree: None,
            // Until the sidebar reports its size, as much as a tall window shows.
            tree_viewport: (0.0, 1_200.0),
            selected: None,
            tabs: Tabs::default(),
            prompt: None,
            menu: None,
            menu_target: None,
            tree_drag: None,
            tree_hover: None,
            file_clipboard: None,
            modifiers: keyboard::Modifiers::empty(),
            find: None,
            palette: None,
            notice: None,
            sidebar: !state.layout.sidebar_hidden,
            sidebar_width: state.layout.sidebar_width.unwrap_or(SIDEBAR_WIDTH),
            terminal_height: state.layout.terminal_height.unwrap_or(TERMINAL_HEIGHT),
            drag: None,
            window: iced::Size::new(1200.0, 800.0),
            window_id: None,
            normal_size: state
                .layout
                .window_size
                .map_or(iced::Size::new(1180.0, 760.0), |(w, h)| iced::Size::new(w, h)),
            window_position: state.layout.window_position.map(|(x, y)| iced::Point::new(x, y)),
            maximized: state.layout.maximized,
            overlay: None,
            closed: Vec::new(),
            format_then_save: Default::default(),
            saving_quietly: false,
            back: Vec::new(),
            forward: Vec::new(),
            remembered_at: None,
            navigating: false,
            tab_target: None,
            panel: Panel::Files,
            project_search: ProjectSearch::default(),
            git: Arc::default(),
            lsp: Lsp::default(),
            terminals: Terminals::default(),
            term_events: None,
            dialog: None,
        };
        if let Some(e) = settings_error {
            let text = app.settings_error_text(&e);
            app.error(text);
        }
        if let Some(e) = errors.into_iter().next() {
            app.error(e);
        }
        app.warn_unknown_keys();

        let mut tasks = Vec::new();
        // With nothing on the command line, BIA reopens the last folder and
        // the files that were open in it.
        if paths.is_empty()
            && !new_window
            && app.settings.restore_on_startup
            && let Some(folder) = state.last_folder()
        {
            tasks.push(app.open_root(folder.to_path_buf(), true));
        }
        // Files never saved come back with what you typed. A second window
        // leaves them to the first.
        if !new_window {
            app.restore_untitled();
        }

        for arg in paths {
            // `bia src/main.rs:42:7`, the way compilers print locations.
            let located = arg.to_str().and_then(|s| {
                let (file, at) = text::split_location(s);
                let at = at?;
                (!arg.exists() && Path::new(file).is_file()).then(|| (PathBuf::from(file), at))
            });
            let (path, at) = match located {
                Some((path, at)) => (path, Some(at)),
                None => (arg, None),
            };
            let path =
                fs::canonicalize(&path).or_else(|_| std::path::absolute(&path)).unwrap_or(path);
            if path.is_dir() {
                if app.tree.is_none() {
                    tasks.push(app.open_root(path, true));
                }
                continue;
            }
            if app.tree.is_none()
                && let Some(parent) = path.parent().filter(|p| p.is_dir())
            {
                let _ = app.open_root(parent.to_path_buf(), false);
            }
            if path.is_file() {
                tasks.push(app.open_at(path, at));
            } else {
                let reason = fs::metadata(&path).err().map(|e| e.to_string()).unwrap_or_default();
                let error = format!("{}: {reason}", path.display());
                let text = app.look.i18n.f("editor.read_error", &[("error", &error)]);
                app.error(text);
            }
        }
        (app, Task::batch(tasks))
    }

    pub fn title(&self) -> String {
        let tab = self.tabs.active().map(|t| {
            let dot = if t.is_dirty() { "● " } else { "" };
            format!("{dot}{}", self.tab_name(t))
        });
        let root = self.tree.as_ref().map(FileTree::root_name);
        let parts: Vec<String> = tab.into_iter().chain(root).chain(["BIA".to_owned()]).collect();
        parts.join(" — ")
    }

    pub fn theme(&self) -> iced::Theme {
        self.look.iced_theme.clone()
    }

    pub fn subscription(&self) -> Subscription<Message> {
        let keys = event::listen_with(|event, status, window| match event {
            Event::Window(window::Event::Focused) => Some(Message::WindowFocused),
            Event::Window(window::Event::Unfocused) => Some(Message::WindowUnfocused),
            Event::Window(window::Event::CloseRequested) => Some(Message::CloseRequested),
            Event::Window(window::Event::Opened { size, .. } | window::Event::Resized(size)) => {
                Some(Message::WindowResized(window, size))
            }
            Event::Window(window::Event::Moved(at)) => Some(Message::WindowMoved(at)),
            Event::Window(window::Event::FileDropped(path)) => Some(Message::FileDropped(path)),
            Event::Keyboard(keyboard::Event::ModifiersChanged(m)) => {
                Some(Message::ModifiersChanged(m))
            }
            Event::Keyboard(keyboard::Event::KeyPressed {
                key, physical_key, modifiers, ..
            }) => {
                let shortcut = keys::shortcut(&key, physical_key, modifiers)?;
                // Text fields swallow Escape, but Escape must always close things.
                let free = status == event::Status::Ignored || shortcut == Shortcut::Escape;
                free.then_some(Message::Shortcut(shortcut))
            }
            _ => None,
        });
        // The mouse is followed only while a divider is being dragged.
        let drag = if self.drag.is_some() {
            event::listen_with(|event, _, _| match event {
                Event::Mouse(iced::mouse::Event::CursorMoved { position }) => {
                    Some(Message::Dragged(position))
                }
                Event::Mouse(iced::mouse::Event::ButtonReleased(_)) => Some(Message::DragEnd),
                _ => None,
            })
        } else {
            Subscription::none()
        };
        // While a row of the tree is pressed, its release is a click or a drop,
        // wherever the mouse is by then.
        let tree_drag = if self.tree_drag.is_some() {
            event::listen_with(|event, _, _| match event {
                Event::Mouse(iced::mouse::Event::ButtonReleased(iced::mouse::Button::Left)) => {
                    Some(Message::TreeRelease)
                }
                _ => None,
            })
        } else {
            Subscription::none()
        };
        Subscription::batch([
            keys,
            drag,
            tree_drag,
            Subscription::run(lsp_events),
            Subscription::run(terminal_events),
        ])
    }

    // ── Queries used by the view ─────────────────────────────────────────

    pub fn syntax_context(&self) -> syntax::Context<'_> {
        syntax::Context {
            syntaxes: &self.look.syntaxes,
            theme: &self.look.syntax_theme,
            generation: self.look.generation,
        }
    }

    pub fn is_markdown(&self, tab: &Tab) -> bool {
        tab.buffer().is_some() && language::is_markdown(&tab.path)
    }

    fn active_buffer_mut(&mut self) -> Option<&mut Buffer> {
        self.tabs.active_mut()?.buffer_mut()
    }

    fn detect_syntax(&self, path: &Path, text: &str) -> String {
        self.detect_syntax_with(&self.settings, path, text)
    }

    /// The language of `path`: what `file_types` says, if it names one BIA
    /// knows, or else what the name and first line tell.
    fn detect_syntax_with(&self, settings: &Settings, path: &Path, text: &str) -> String {
        let named = path
            .file_name()
            .and_then(|n| settings.file_type(&n.to_string_lossy()).map(str::to_owned))
            .and_then(|name| self.look.syntaxes.find_syntax_by_name(&name).map(|s| s.name.clone()));
        if let Some(name) = named {
            return name;
        }
        let first_line = text.lines().next().unwrap_or_default();
        language::detect(&self.look.syntaxes, path, first_line).name.clone()
    }

    /// How a file in `syntax` is indented, and how wide its tabs look. The
    /// project's `.editorconfig` comes first, then what the file already
    /// does, then the settings.
    fn indent_for(
        settings: &Settings,
        path: &Path,
        syntax: &str,
        text: &str,
        config: bia_core::editorconfig::Props,
    ) -> (Indent, usize) {
        let own = settings.for_language(syntax);
        let tab_width =
            config.tab_width.or(config.indent_size).unwrap_or(own.tab_size).clamp(1, 16);
        let size = config.indent_size.unwrap_or(tab_width);
        let indent = match config.tabs {
            Some(true) => Indent::tabs(tab_width),
            Some(false) => Indent::spaces(size),
            None => Indent::for_file(path, text, size, !own.insert_spaces),
        };
        (indent, tab_width)
    }

    // ── Notices ──────────────────────────────────────────────────────────

    fn info(&mut self, text: String) {
        self.notice = Some(Notice { text, error: false, at: Instant::now() });
    }

    fn error(&mut self, text: String) {
        self.notice = Some(Notice { text, error: true, at: Instant::now() });
    }

    fn expire_notice(&mut self) {
        if self.notice.as_ref().is_some_and(|n| n.at.elapsed() > NOTICE_TTL) {
            self.notice = None;
        }
    }

    /// Points out a key settings.json has that BIA doesn't read. It's a typo,
    /// usually, that would otherwise do nothing without a word.
    fn warn_unknown_keys(&mut self) {
        let json = fs::read_to_string(paths::settings_file()).unwrap_or_default();
        if let Some(key) = Settings::unknown_keys(&json).into_iter().next() {
            let text = self.look.i18n.f("notice.unknown_key", &[("key", &key)]);
            self.error(text);
        }
    }

    fn settings_error_text(&self, error: &SettingsError) -> String {
        let i18n = &self.look.i18n;
        match error {
            SettingsError::Parse(e) => i18n.f(
                "notice.settings_error_at",
                &[("line", &e.line()), ("column", &e.column()), ("error", e)],
            ),
            SettingsError::Io(e) => i18n.f("notice.settings_error", &[("error", e)]),
        }
    }

    // ── Update ───────────────────────────────────────────────────────────

    pub fn update(&mut self, message: Message) -> Task<Message> {
        let task = self.handle(message);
        Task::batch([task, self.schedule_backup(), self.follow_edits()])
    }

    fn handle(&mut self, message: Message) -> Task<Message> {
        if !matches!(message, Message::WindowFocused) {
            self.expire_notice();
        }
        match message {
            Message::OpenFolder => {
                let start = self.tree.as_ref().map(|t| t.root().to_path_buf());
                return Task::perform(pick_folder(start), Message::FolderPicked);
            }
            Message::FolderPicked(Some(path)) | Message::OpenRecent(path) => {
                return self.open_root(path, true);
            }
            Message::FolderPicked(None) => {}
            Message::BackupTick => {
                self.backup_scheduled = false;
                self.save_session();
                // Two seconds after the last key, "after_delay" saves here.
                if self.settings.autosave == bia_core::settings::Autosave::AfterDelay
                    && !self.dirty_files().is_empty()
                    && self.dialog.is_none()
                {
                    let save = self.autosave();
                    return Task::batch([save, self.write_backups()]);
                }
                return self.write_backups();
            }

            Message::TreeScrolled(offset, height) => self.tree_viewport = (offset, height),
            Message::RowDown(path) => {
                self.menu = None;
                self.prompt = None;
                self.selected = Some(path.clone());
                self.tree_drag = Some(TreeDrag { from: path, over: None, moved: false });
            }
            Message::RowEnter(path) => {
                if let Some(drag) = &mut self.tree_drag {
                    drag.moved |= path != drag.from;
                    drag.over = Some(path.clone());
                }
                self.tree_hover = Some(path);
            }
            Message::TreeLeave => {
                self.tree_hover = None;
                if let Some(drag) = &mut self.tree_drag {
                    drag.over = None;
                }
            }
            Message::TreeRelease => return self.tree_release(),
            Message::RowTarget(path) => self.menu_target = Some(path),
            Message::TreeMenu(at) => self.tree_menu(at),
            Message::CloseMenu => self.menu = None,
            Message::Tree(action) => {
                self.menu = None;
                return self.tree_action(action);
            }
            Message::PasteFiles(dir, text) => return self.paste_files(&dir, text.as_deref()),
            Message::ModifiersChanged(m) => self.modifiers = m,
            Message::Failed(error) => {
                if let Some(error) = error {
                    let text = self.look.i18n.f("notice.fs_error", &[("error", &error)]);
                    self.error(text);
                }
            }
            Message::PromptInput(value) => {
                if let Some(prompt) = &mut self.prompt {
                    prompt.value = value;
                    prompt.error = None;
                }
            }
            Message::PromptSubmit => return self.submit_prompt(),

            Message::SelectTab(index) => {
                if index != self.tabs.active_index() {
                    self.remember_place();
                }
                if self.tabs.select(index) {
                    let reveal = self.reveal_active();
                    return Task::batch([reveal, self.focus_editor()]);
                }
            }
            Message::CloseTab(index) => return self.close_tab(index),

            Message::Input(pane, input) => {
                // The text stays as it was while a dialog asks about it.
                if self.dialog.is_some() {
                    return Task::none();
                }
                // Resting the mouse on the other side doesn't move the focus.
                if pane != self.tabs.focused() {
                    if matches!(input, Input::Hover(_)) {
                        return Task::none();
                    }
                    self.tabs.focus(pane);
                    self.find = None;
                    let _ = self.reveal_active();
                }
                let typed = match &input {
                    Input::Text(text) => text.chars().last(),
                    _ => None,
                };
                let Some(path) = self.tabs.active().map(|t| t.path.clone()) else {
                    return Task::none();
                };
                let Some(buffer) = self.active_buffer_mut() else { return Task::none() };
                let revision = buffer.revision();
                let reply = buffer.input(input);
                let edited = buffer.revision() != revision;
                let syntax = buffer.editor.syntax.clone();
                if edited {
                    self.lsp.edited(path.clone());
                    self.lsp.hover = None;
                }
                self.follow_completion(&path, typed);
                let sync = self.schedule_lsp_sync();
                let task = match reply {
                    Reply::None => Task::none(),
                    Reply::Clipboard(text) => iced::clipboard::write(text),
                    Reply::NoComments => {
                        let text = self.look.i18n.f("notice.no_comments", &[("language", &syntax)]);
                        self.error(text);
                        Task::none()
                    }
                    Reply::Lsp(action) => self.lsp_action(&path, action),
                };
                return Task::batch([task, sync]);
            }
            Message::LspChannel(sender) => {
                self.lsp.sink = Some(sender);
                self.sync_lsp();
            }
            Message::Lsp(event) => {
                let ready = matches!(event, bia_core::lsp::Event::Ready { .. });
                let active = self
                    .tabs
                    .active()
                    .and_then(|t| Some((t.path.as_path(), t.buffer()?)))
                    .map(|(p, b)| (p, b.editor.rope(), b.cursor_offset()));
                let effects = self.lsp.event(event, active);
                if ready {
                    self.sync_lsp();
                }
                return self.apply_lsp(effects);
            }
            Message::LspSync => {
                self.lsp.sync_scheduled = false;
                // Still typing: wait for the pause.
                let quiet = self.lsp.edited_at.map_or(LSP_DELAY, |at| at.elapsed());
                if quiet < LSP_DELAY {
                    return self.schedule_lsp_sync_in(LSP_DELAY - quiet);
                }
                self.sync_lsp();
            }
            Message::BackupWritten(path, revision, ok) => {
                // Failed: the next tick tries again.
                if let Some(tab) = self.tabs.iter_mut().find(|t| t.path == path)
                    && !ok
                    && tab.backup == Some(revision)
                {
                    tab.backup = None;
                }
            }
            Message::FollowEdits => self.follow.scheduled = false,
            Message::GitMarks(id, revision, marks) => {
                if let Some(buffer) =
                    self.tabs.iter_mut().find_map(|t| t.buffer_mut().filter(|b| b.id == id))
                {
                    buffer.git.finish(&buffer.editor, revision, marks);
                }
            }
            Message::FindDone(found) => self.find_done(&found),
            Message::FilesListed(list) => {
                if self.listing.as_ref() == Some(&list.root) {
                    self.listing = None;
                }
                if self.tree.as_ref().is_some_and(|t| t.root() == list.root) {
                    self.files = Some(Arc::clone(&list));
                    if let Some(Palette { mode: PaletteMode::Files { files, .. }, .. }) =
                        &mut self.palette
                    {
                        *files = Some(list);
                        self.refresh_palette();
                    }
                }
            }
            Message::TermChannel(sender) => self.term_events = Some(sender),
            Message::Term(id, event) => match event {
                // New output: the view redraws after any message. The program
                // in front may have changed too.
                TermEvent::Wakeup => {
                    let later = self.terminals.get_mut(id).is_some_and(|t| t.look_at_program());
                    if later && !self.terminals.look_scheduled {
                        self.terminals.look_scheduled = true;
                        return Task::perform(sleep(crate::terminal::LOOK_EVERY), |()| {
                            Message::LookAtTerminals
                        });
                    }
                }
                TermEvent::Title(title) => {
                    if let Some(t) = self.terminals.get_mut(id) {
                        t.title = title;
                    }
                }
                TermEvent::Reply(text) => {
                    if let Some(t) = self.terminals.get(id) {
                        t.write(text.into_bytes());
                    }
                }
                TermEvent::Copy(text) => return iced::clipboard::write(text),
                // After `exit` or Ctrl+D, the tab goes away with its shell.
                TermEvent::Exit => return self.close_terminal(id),
            },
            Message::TermResize(id, size) => {
                if let Some(t) = self.terminals.get_mut(id) {
                    t.resize(size);
                }
            }
            Message::LookAtTerminals => {
                self.terminals.look_scheduled = false;
                for t in &mut self.terminals.list {
                    t.look_again();
                }
            }
            Message::ToggleTerminal => return self.toggle_terminal(),
            Message::NewTerminal => return self.new_terminal(),
            Message::SelectTerminal(id) => {
                self.terminals.select(id);
                return operation::focus(TERMINAL_ID);
            }
            Message::CloseTerminal(id) => {
                if self.terminals.get(id).is_some_and(Terminal::busy) {
                    self.dialog = Some(Dialog::Busy(Closing::Terminal(id)));
                    return Task::none();
                }
                return self.close_terminal(id);
            }
            Message::TermOpen(id, location) => return self.open_location(id, &location),
            Message::ToggleSidebar => self.toggle_sidebar(),
            Message::DragStart(splitter) => {
                let size = match splitter {
                    Splitter::Sidebar => self.sidebar_width,
                    Splitter::Terminal => self.terminal_height,
                };
                self.drag = Some(Drag { splitter, from: None, size });
            }
            Message::Dragged(at) => self.dragged(at),
            Message::DragEnd => {
                self.drag = None;
                self.save_layout();
            }
            Message::WindowResized(id, size) => {
                self.window = size;
                self.window_id = Some(id);
                return window::is_maximized(id).map(Message::Maximized);
            }
            Message::WindowMoved(at) => {
                if !self.maximized {
                    self.window_position = Some(at);
                }
            }
            Message::Maximized(maximized) => {
                self.maximized = maximized;
                if !maximized {
                    self.normal_size = self.window;
                }
            }
            Message::FileDropped(path) => {
                if path.is_dir() {
                    return self.open_root(path, true);
                }
                return self.open(path);
            }
            Message::Run(command) => {
                self.menu = None;
                self.palette = None;
                return self.run(command);
            }
            Message::SaveAsPicked(from, to, close) => return self.save_as_picked(&from, to, close),
            Message::FilesPicked(files) => {
                let opened: Vec<_> =
                    files.unwrap_or_default().into_iter().map(|f| self.open(f)).collect();
                return Task::batch(opened);
            }
            Message::TabTarget(index) => self.tab_target = Some(index),
            Message::TabMenu(at) => self.tab_menu(at),
            Message::CloseTabs(index, which) => return self.close_tabs(index, which),
            Message::MainMenu => self.main_menu(),
            Message::CloseOverlay => self.overlay = None,
            Message::FormatTimedOut(path) => return self.save_after_format(&path),
            Message::WindowUnfocused => {
                if self.settings.autosave == bia_core::settings::Autosave::FocusChange {
                    return self.autosave();
                }
            }
            Message::Copy(text) => {
                let notice = self.look.i18n.f("notice.copied", &[("text", &text)]);
                self.info(notice);
                return iced::clipboard::write(text);
            }
            Message::EditorScrolled(pane, offset) => {
                if let Some(tab) = self.tabs.active_in_mut(pane) {
                    tab.scroll = offset;
                }
            }
            Message::MoveToOtherSide(index) => {
                if self.tabs.move_to_other(index) {
                    let reveal = self.reveal_active();
                    return Task::batch([reveal, self.focus_editor()]);
                }
            }
            Message::Save => {
                self.lsp.completion = None;
                if self.tabs.active().is_some_and(is_untitled) {
                    return self.save_as(self.tabs.active_index(), false);
                }
                if let Some(wait) = self.format_before_save() {
                    return wait;
                }
                self.save(self.tabs.active_index(), false);
                if let Some(path) = self.tabs.active().map(|t| t.path.clone()) {
                    self.sync_lsp();
                    self.lsp.saved(&path);
                }
                return self.refresh_git();
            }
            Message::GitBase(path, base) => {
                if let Some(buffer) =
                    self.tabs.position(&path).and_then(|i| self.tabs.get_mut(i)?.buffer_mut())
                {
                    buffer.git.set_base(base);
                }
            }
            Message::GitStatus(root, status) => {
                if self.tree.as_ref().is_some_and(|t| t.root() == root) {
                    self.git = status;
                }
            }
            Message::TogglePreview => return self.toggle_preview(),
            Message::LinkClicked(url) => return self.link_clicked(url),

            Message::FindInput(query) => {
                let old = self.find.take();
                let replace = old.as_ref().and_then(|f| f.replace.clone());
                let options = old.map(|f| f.options).unwrap_or_default();
                self.find = Some(Find {
                    query,
                    options,
                    matches: Vec::new(),
                    current: None,
                    error: None,
                    replace,
                    searched: None,
                    running: None,
                });
            }
            Message::FindOption(option) => {
                if let Some(find) = &mut self.find {
                    let o = &mut find.options;
                    match option {
                        FindOption::Case => o.case_sensitive = !o.case_sensitive,
                        FindOption::Word => o.whole_word = !o.whole_word,
                        FindOption::Regex => o.regex = !o.regex,
                    }
                    find.current = None;
                    find.changed();
                }
                return operation::focus(FIND_ID);
            }
            // Enter in the find field goes to the next match and stays there,
            // so pressing it again goes on; Escape gets back to the text.
            Message::FindSubmit => return self.find_step(true, false),
            Message::ShowPanel(panel) => return self.show_panel(panel),
            Message::SearchInput(query) => self.project_search.query = query,
            Message::SearchInclude(include) => self.project_search.include = include,
            Message::SearchExclude(exclude) => self.project_search.exclude = exclude,
            Message::SearchSubmit => return self.run_search(),
            Message::SearchOption(option) => {
                let o = &mut self.project_search.options;
                match option {
                    FindOption::Case => o.case_sensitive = !o.case_sensitive,
                    FindOption::Word => o.whole_word = !o.whole_word,
                    FindOption::Regex => o.regex = !o.regex,
                }
                // Same results for a different question would mislead.
                let task = if self.project_search.found.is_some() {
                    self.run_search()
                } else {
                    Task::none()
                };
                return Task::batch([task, operation::focus(SEARCH_ID)]);
            }
            Message::SearchDone(generation, found) => {
                let search = &mut self.project_search;
                if generation == search.generation {
                    search.found = Some(found);
                    search.running = false;
                }
            }
            Message::SearchOpen(file, hit) => return self.open_hit(file, hit),
            Message::ReplaceInput(value) => {
                if let Some(find) = &mut self.find {
                    find.replace = Some(value);
                }
            }
            Message::ReplaceOne => return self.replace_one(),
            Message::ReplaceAll => return self.replace_all(),

            Message::PaletteInput(query) => {
                if let Some(palette) = &mut self.palette {
                    palette.query = query;
                }
                self.refresh_palette();
            }
            Message::PaletteSubmit => return self.submit_palette(),
            Message::PalettePick(index) => {
                if let Some(palette) = &mut self.palette {
                    palette.selected = index;
                }
                return self.submit_palette();
            }
            Message::PaletteClose => {
                self.palette = None;
                return self.focus_editor();
            }

            Message::Shortcut(shortcut) => return self.shortcut(shortcut),
            Message::WindowFocused => {
                self.on_focus();
                // Commits and checkouts happen in other windows.
                return self.refresh_git();
            }
            Message::CloseRequested => return self.quit(true),
            Message::Dialog(choice) => return self.answer(choice),
            Message::OpenSettings => return self.open(paths::settings_file()),
        }
        Task::none()
    }

    fn shortcut(&mut self, shortcut: Shortcut) -> Task<Message> {
        let confirms =
            shortcut == Shortcut::Confirm && matches!(self.dialog, Some(Dialog::Trash(_)));
        if self.dialog.is_some() && shortcut != Shortcut::Escape && !confirms {
            return Task::none();
        }
        match shortcut {
            Shortcut::OpenFolder => return self.update(Message::OpenFolder),
            Shortcut::Save => return self.update(Message::Save),
            Shortcut::CloseTab => return self.close_tab(self.tabs.active_index()),
            Shortcut::Undo | Shortcut::Redo => {
                if let Some(b) = self.active_buffer_mut() {
                    if shortcut == Shortcut::Undo {
                        b.undo()
                    } else {
                        b.redo()
                    };
                }
            }
            Shortcut::Find | Shortcut::Replace => {
                if self.tabs.active().and_then(Tab::buffer).is_some() {
                    let query = self.find.as_ref().map(|f| f.query.clone()).unwrap_or_default();
                    let _ = self.update(Message::FindInput(query.clone()));
                    let mut field = FIND_ID;
                    if shortcut == Shortcut::Replace
                        && let Some(find) = &mut self.find
                    {
                        find.replace.get_or_insert_default();
                        if !query.is_empty() {
                            field = REPLACE_ID;
                        }
                    }
                    return Task::batch([operation::focus(field), operation::select_all(field)]);
                }
            }
            Shortcut::FindNext => return self.find_step(true, true),
            Shortcut::FindPrevious => return self.find_step(false, true),
            Shortcut::QuickOpen => return self.open_palette(true),
            Shortcut::FindInFiles => return self.show_panel(Panel::Search),
            Shortcut::Explorer => return self.show_panel(Panel::Files),
            Shortcut::GoToLine => return self.open_palette(false),
            Shortcut::Up | Shortcut::Down => {
                if let Some(palette) = &mut self.palette {
                    let count = match &palette.mode {
                        PaletteMode::Files { results, .. } => results.len(),
                        PaletteMode::Commands { results } => results.len(),
                        PaletteMode::Recent { results } => results.len(),
                        PaletteMode::History { versions, .. } => versions.len().min(12),
                        PaletteMode::Line | PaletteMode::Rename { .. } => 0,
                    };
                    palette.selected = match shortcut {
                        Shortcut::Up => palette.selected.saturating_sub(1),
                        _ => (palette.selected + 1).min(count.saturating_sub(1)),
                    };
                }
            }
            Shortcut::Preview => return self.update(Message::TogglePreview),
            Shortcut::Sidebar => self.toggle_sidebar(),
            Shortcut::NextTab | Shortcut::PreviousTab => {
                if let Some(i) = self.tabs.neighbor(shortcut == Shortcut::NextTab) {
                    return self.update(Message::SelectTab(i));
                }
            }
            Shortcut::Settings => return self.update(Message::OpenSettings),
            Shortcut::ZoomIn => self.zoom(Some(1.0)),
            Shortcut::ZoomOut => self.zoom(Some(-1.0)),
            Shortcut::ZoomReset => self.zoom(None),
            Shortcut::Terminal => return self.toggle_terminal(),
            Shortcut::NewTerminal => return self.new_terminal(),
            Shortcut::Split => {
                return self.update(Message::MoveToOtherSide(self.tabs.active_index()));
            }
            Shortcut::NewFile => return self.new_untitled(),
            Shortcut::NewWindow => return self.new_window(),
            Shortcut::SaveAs => return self.save_as(self.tabs.active_index(), false),
            Shortcut::ReopenTab => return self.reopen_tab(),
            Shortcut::Commands => return self.open_commands(),
            Shortcut::Quit => return self.quit(true),
            Shortcut::Back => return self.go_back(),
            Shortcut::Forward => return self.go_forward(),
            // These reach the app only when no editor, terminal or field took
            // them, so they act on what's selected in the tree.
            Shortcut::Rename | Shortcut::Delete | Shortcut::Cut | Shortcut::Copy => {
                let Some(path) = self.selected_in_tree() else { return Task::none() };
                return self.tree_action(match shortcut {
                    Shortcut::Rename => TreeAction::Rename(path),
                    Shortcut::Delete => TreeAction::Trash(path),
                    Shortcut::Cut => TreeAction::Cut(path),
                    _ => TreeAction::Copy(path),
                });
            }
            Shortcut::Paste => {
                let Some(dir) = self.target_dir() else { return Task::none() };
                return self.tree_action(TreeAction::Paste(dir));
            }
            Shortcut::Confirm => {
                if matches!(self.dialog, Some(Dialog::Trash(_))) {
                    return self.answer(Choice::Discard);
                }
            }
            Shortcut::Escape => {
                if self.dialog.is_some() {
                    self.dialog = None;
                } else if self.overlay.is_some() {
                    self.overlay = None;
                } else if self.menu.is_some() {
                    self.menu = None;
                } else if self.palette.is_some() {
                    return self.update(Message::PaletteClose);
                } else if self.prompt.is_some() {
                    self.prompt = None;
                } else if self.find.is_some() {
                    self.find = None;
                    return self.focus_editor();
                }
            }
        }
        Task::none()
    }

    /// Ctrl+= and Ctrl+-: the editor font, one pixel at a time, for this
    /// session. `None` goes back to `font_size` from settings.json.
    fn zoom(&mut self, step: Option<f32>) {
        let size = match step {
            Some(step) => (self.settings.font_size + step).clamp(6.0, 72.0),
            None => self.font_size_setting,
        };
        self.settings.font_size = size;
        let text = self.look.i18n.f("notice.zoom", &[("size", &size)]);
        self.info(text);
    }

    // ── Files and tabs ───────────────────────────────────────────────────

    /// Makes `root` the workspace. `remember` is true for folders you chose
    /// and false for a file's folder: a chosen folder opens next time, and
    /// its open files are remembered and, with no tabs open, reopened.
    fn open_root(&mut self, root: PathBuf, remember: bool) -> Task<Message> {
        self.save_session();
        let mut session = None;
        if remember {
            // Not worth an error: the worst case is the welcome screen next time.
            let _ = State::update(&paths::state_file(), |state| {
                state.last_folder = Some(root.clone());
                session = state.session(&root).cloned();
                state.touch(&root);
                self.recent = state.recent_folders();
            });
        }
        self.remember_root = remember;
        self.palette = None;
        self.tree = Some(FileTree::new(root.clone(), self.settings.hidden.clone()));
        self.files = None;
        self.selected = None;
        self.prompt = None;
        self.project_search = ProjectSearch::default();
        self.git = Arc::default();
        if !remember || !self.tabs.is_empty() {
            return self.refresh_git();
        }
        let restore = self.restore_session(&root, session.unwrap_or_default());
        Task::batch([restore, self.refresh_git()])
    }

    /// Puts the keyboard in the editor of the focused side.
    fn focus_editor(&self) -> Task<Message> {
        operation::focus(match self.tabs.focused() {
            Pane::Left => EDITOR_ID,
            Pane::Right => EDITOR_RIGHT_ID,
        })
    }

    // ── Terminal ─────────────────────────────────────────────────────────

    /// Ctrl+`: shows the terminal panel (starting a shell the first time)
    /// or hides it, moving the focus there and back.
    fn toggle_terminal(&mut self) -> Task<Message> {
        if self.terminals.open {
            self.terminals.open = false;
            return self.focus_editor();
        }
        if self.terminals.list.is_empty() {
            return self.new_terminal();
        }
        self.terminals.open = true;
        operation::focus(TERMINAL_ID)
    }

    /// Ctrl+Shift+` or the + button starts another shell, in a new tab.
    fn new_terminal(&mut self) -> Task<Message> {
        let dir = self
            .tree
            .as_ref()
            .map(|t| t.root().to_path_buf())
            .or_else(|| {
                std::env::var_os("HOME")
                    .or_else(|| std::env::var_os("USERPROFILE"))
                    .map(PathBuf::from)
            })
            .unwrap_or_else(|| PathBuf::from("."));
        self.new_terminal_in(dir)
    }

    fn new_terminal_in(&mut self, dir: PathBuf) -> Task<Message> {
        let Some(events) = self.term_events.clone() else { return Task::none() };
        let size = crate::terminal::Size { columns: 80, lines: 12 };
        let id = self.terminals.next_id();
        let shell = self
            .settings
            .terminal
            .shell
            .split_first()
            .map(|(program, args)| (program.clone(), args.to_vec()));
        match Terminal::spawn(id, &dir, size, events, shell) {
            Ok(terminal) => self.terminals.push(terminal),
            Err(e) => {
                let text = self.look.i18n.f("notice.terminal_failed", &[("error", &e)]);
                self.error(text);
                return Task::none();
            }
        }
        operation::focus(TERMINAL_ID)
    }

    fn close_terminal(&mut self, id: u64) -> Task<Message> {
        self.terminals.remove(id);
        if self.terminals.open { operation::focus(TERMINAL_ID) } else { self.focus_editor() }
    }

    /// Opens a location clicked in a terminal. Relative paths start from
    /// the folder the shell is in, then from the open folder.
    fn open_location(&mut self, id: u64, location: &str) -> Task<Message> {
        let (path, at) = text::split_location(location);
        let path = Path::new(path);
        let mut bases = Vec::new();
        if let Some(t) = self.terminals.get(id) {
            bases.push(t.cwd());
        }
        if let Some(tree) = &self.tree {
            bases.push(tree.root().to_path_buf());
        }
        let found = if path.is_absolute() {
            Some(path.to_path_buf()).filter(|p| p.is_file())
        } else {
            bases.iter().map(|base| base.join(path)).find(|p| p.is_file())
        };
        match found {
            Some(file) => {
                let file = fs::canonicalize(&file).unwrap_or(file);
                self.open_at(file, at)
            }
            None => {
                let text = self.look.i18n.f("notice.not_found", &[("path", &location)]);
                self.error(text);
                Task::none()
            }
        }
    }

    // ── Layout ───────────────────────────────────────────────────────────

    fn toggle_sidebar(&mut self) {
        self.sidebar = !self.sidebar;
        self.save_layout();
    }

    /// The mouse moved while a divider is held. The panel follows it, but
    /// leaves the editor some room.
    fn dragged(&mut self, at: iced::Point) {
        let window = self.window;
        let Some(drag) = &mut self.drag else { return };
        let Some(from) = drag.from else {
            drag.from = Some(at);
            return;
        };
        match drag.splitter {
            Splitter::Sidebar => {
                let most = (window.width - 320.0).max(160.0);
                self.sidebar_width = (drag.size + at.x - from.x).clamp(160.0, most);
            }
            Splitter::Terminal => {
                let most = (window.height - 180.0).max(90.0);
                self.terminal_height = (drag.size + from.y - at.y).clamp(90.0, most);
            }
        }
    }

    /// The sizes the sidebar and the terminal take on screen, kept inside
    /// a window that may have shrunk since they were set.
    /// The rulers for a file in `language`.
    pub fn rulers_for(&self, language: &str) -> &[usize] {
        self.settings
            .languages
            .get(language)
            .and_then(|l| l.rulers.as_deref())
            .unwrap_or(&self.settings.rulers)
    }

    /// Where long lines wrap in a file in `language`.
    pub fn wrap_for(&self, language: &str) -> crate::code_view::Wrap {
        use crate::code_view::Wrap;
        use bia_core::settings::SoftWrap;
        let own = self.settings.for_language(language);
        match own.soft_wrap {
            SoftWrap::Off => Wrap::Off,
            SoftWrap::Edge => Wrap::Edge,
            SoftWrap::Column => Wrap::Column(own.wrap_column),
        }
    }

    pub fn window_size(&self) -> iced::Size {
        self.window
    }

    pub fn sidebar_width(&self) -> f32 {
        self.sidebar_width.min((self.window.width - 320.0).max(160.0))
    }

    pub fn terminal_height(&self) -> f32 {
        self.terminal_height.min((self.window.height - 180.0).max(90.0))
    }

    fn save_layout(&self) {
        let (width, hidden, height) = (self.sidebar_width, !self.sidebar, self.terminal_height);
        // Not worth an error: the worst case is the default sizes next time.
        let _ = State::update(&paths::state_file(), |state| {
            state.layout.sidebar_width = Some(width);
            state.layout.sidebar_hidden = hidden;
            state.layout.terminal_height = Some(height);
        });
    }

    // ── Quitting ─────────────────────────────────────────────────────────

    /// Closing the window. First a program running in a terminal, then
    /// unsaved files, each asks before anything is lost.
    fn quit(&mut self, check_terminals: bool) -> Task<Message> {
        if check_terminals && self.terminals.any_busy() {
            self.dialog = Some(Dialog::Busy(Closing::Quit));
        } else if !self.dirty_files().is_empty() {
            self.dialog = Some(Dialog::Unsaved(After::Quit));
        } else {
            return self.exit();
        }
        Task::none()
    }

    /// Tabs with unsaved changes to a file. Files never saved aren't
    /// among them, since they're kept for next time instead.
    pub fn dirty_files(&self) -> Vec<usize> {
        self.tabs
            .dirty_indices()
            .into_iter()
            .filter(|&i| !self.tabs.iter().nth(i).is_some_and(is_untitled))
            .collect()
    }

    /// "Close folder": back to the welcome screen, after asking about
    /// unsaved files.
    fn close_folder(&mut self, ask: bool) -> Task<Message> {
        if ask && !self.dirty_files().is_empty() {
            self.dialog = Some(Dialog::Unsaved(After::CloseFolder));
            return Task::none();
        }
        self.save_session();
        let named: Vec<usize> = (0..self.tabs.iter().count())
            .rev()
            .filter(|&i| !self.tabs.iter().nth(i).is_some_and(is_untitled))
            .collect();
        for i in named {
            if let Some(tab) = self.tabs.remove(i)
                && tab.backup.is_some()
            {
                self.backups.remove(tab.path);
            }
        }
        self.tree = None;
        self.selected = None;
        self.prompt = None;
        self.find = None;
        self.git = Arc::default();
        self.project_search = ProjectSearch::default();
        self.remember_root = false;
        // Next time BIA starts empty too.
        let _ = State::update(&paths::state_file(), |state| state.last_folder = None);
        self.sync_lsp();
        Task::none()
    }

    // ── Commands ─────────────────────────────────────────────────────────

    fn run(&mut self, command: crate::commands::Command) -> Task<Message> {
        use crate::commands::Command as C;
        let shortcut = |s| Message::Shortcut(s);
        match command {
            C::NewFile => return self.new_untitled(),
            C::NewWindow => return self.new_window(),
            C::OpenFile => {
                let start = self.tree.as_ref().map(|t| t.root().to_path_buf());
                return Task::perform(pick_files(start), Message::FilesPicked);
            }
            C::OpenFolder => return self.update(Message::OpenFolder),
            C::OpenRecent => return self.open_recent(),
            C::Save => return self.update(Message::Save),
            C::SaveAs => return self.save_as(self.tabs.active_index(), false),
            C::SaveAll => return self.save_all(),
            C::CloseTab => return self.close_tab(self.tabs.active_index()),
            C::ReopenTab => return self.reopen_tab(),
            C::CloseFolder => return self.close_folder(true),
            C::QuickOpen => return self.update(shortcut(Shortcut::QuickOpen)),
            C::GoToLine => return self.update(shortcut(Shortcut::GoToLine)),
            C::Find => return self.update(shortcut(Shortcut::Find)),
            C::Replace => return self.update(shortcut(Shortcut::Replace)),
            C::FindInFiles => return self.update(shortcut(Shortcut::FindInFiles)),
            C::Sidebar => self.toggle_sidebar(),
            C::Terminal => return self.toggle_terminal(),
            C::NewTerminal => return self.new_terminal(),
            C::Split => return self.update(shortcut(Shortcut::Split)),
            C::Preview => return self.toggle_preview(),
            C::ZoomIn => self.zoom(Some(1.0)),
            C::ZoomOut => self.zoom(Some(-1.0)),
            C::ZoomReset => self.zoom(None),
            C::CollapseTree => return self.tree_action(TreeAction::CollapseAll),
            C::Settings => return self.update(Message::OpenSettings),
            C::Commands => return self.open_commands(),
            C::Shortcuts => self.overlay = Some(Overlay::Shortcuts),
            C::About => self.overlay = Some(Overlay::About),
            C::Quit => return self.quit(true),
            C::Back => return self.go_back(),
            C::Forward => return self.go_forward(),
            C::LocalHistory => return self.open_history(),
            C::DefaultSettings => return self.open_default_settings(),
        }
        Task::none()
    }

    // ── Back and forward ─────────────────────────────────────────────────

    /// Where the cursor is, as the file and the offset in it.
    fn here(&self) -> Option<(PathBuf, usize)> {
        let tab = self.tabs.active().filter(|t| !is_untitled(t))?;
        Some((tab.path.clone(), tab.buffer()?.cursor_offset()))
    }

    /// Notes where the cursor is before it jumps somewhere else, so Back
    /// can return there.
    fn remember_place(&mut self) {
        if self.navigating
            || self.remembered_at.is_some_and(|t| t.elapsed() < Duration::from_millis(80))
        {
            return;
        }
        let Some(place) = self.here() else { return };
        if self.back.last() != Some(&place) {
            self.back.push(place);
            if self.back.len() > 100 {
                self.back.remove(0);
            }
        }
        self.forward.clear();
        self.remembered_at = Some(Instant::now());
    }

    fn go_back(&mut self) -> Task<Message> {
        let Some(place) = self.back.pop() else { return Task::none() };
        if let Some(here) = self.here() {
            self.forward.push(here);
        }
        self.jump_to(place)
    }

    fn go_forward(&mut self) -> Task<Message> {
        let Some(place) = self.forward.pop() else { return Task::none() };
        if let Some(here) = self.here() {
            self.back.push(here);
        }
        self.jump_to(place)
    }

    fn jump_to(&mut self, (path, at): (PathBuf, usize)) -> Task<Message> {
        if !path.is_file() {
            return Task::none();
        }
        self.navigating = true;
        let open = self.open(path);
        self.navigating = false;
        self.center_seq += 1;
        let seq = self.center_seq;
        if let Some(buffer) = self.active_buffer_mut() {
            let at = at.min(buffer.editor.len());
            buffer.select(at..at);
            buffer.center = seq;
        }
        Task::batch([open, self.focus_editor()])
    }

    // ── Local history, default settings ──────────────────────────────────

    fn open_history(&mut self) -> Task<Message> {
        let Some(tab) = self.tabs.active().filter(|t| t.buffer().is_some() && !is_untitled(t))
        else {
            return Task::none();
        };
        let path = tab.path.clone();
        let versions = bia_core::history::list(&paths::history_dir(), &path);
        let mode = PaletteMode::History { path, versions };
        self.palette = Some(Palette { mode, query: String::new(), selected: 0 });
        self.menu = None;
        operation::focus(PALETTE_ID)
    }

    /// When a version was saved, the way people say it, like "5 min ago".
    pub fn ago(&self, when: std::time::SystemTime) -> String {
        let t = &self.look.i18n;
        let minutes = when.elapsed().map_or(0, |d| d.as_secs() / 60);
        match minutes {
            0 => t.t("history.now").to_owned(),
            1..60 => t.f("history.minutes", &[("count", &minutes)]),
            60..1440 => t.f("history.hours", &[("count", &(minutes / 60))]),
            _ => t.f("history.days", &[("count", &(minutes / 1440))]),
        }
    }

    /// Puts a saved version's text in the tab, as an edit Ctrl+Z takes back.
    /// Nothing is written until you save.
    fn restore_version(
        &mut self,
        path: &Path,
        version: &bia_core::history::Version,
    ) -> Task<Message> {
        let Ok(text) = fs::read_to_string(&version.file) else { return Task::none() };
        let when = self.ago(version.saved);
        let Some(buffer) =
            self.tabs.position(path).and_then(|i| self.tabs.get_mut(i)?.buffer_mut())
        else {
            return Task::none();
        };
        let len = buffer.editor.len();
        buffer.replace_all(&[(0..len, text)]);
        self.lsp.edited(path.to_path_buf());
        let notice = self.look.i18n.f("notice.restored", &[("when", &when)]);
        self.info(notice);
        self.focus_editor()
    }

    /// Every setting with its default value, in a file of its own, to read
    /// next to settings.json.
    fn open_default_settings(&mut self) -> Task<Message> {
        let path = paths::config_dir().join("default-settings.json");
        if let Err(e) = fs::write(&path, Settings::default().to_json()) {
            let text = self.look.i18n.f("notice.fs_error", &[("error", &e)]);
            self.error(text);
            return Task::none();
        }
        if let Some(i) = self.tabs.position(&path) {
            self.reload(&path);
            return self.update(Message::SelectTab(i));
        }
        self.open(path)
    }

    /// Ctrl+Shift+N starts another BIA, with nothing open.
    fn new_window(&mut self) -> Task<Message> {
        let started = std::env::current_exe().and_then(|exe| {
            std::process::Command::new(exe)
                .arg("--new")
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
        });
        if let Err(e) = started {
            let text = self.look.i18n.f("notice.fs_error", &[("error", &e)]);
            self.error(text);
        }
        Task::none()
    }

    /// The menu button shows the commands you look for in a menu bar.
    fn main_menu(&mut self) {
        use crate::commands::Command;
        use ui::menu::Item;
        let items = Command::MENU
            .into_iter()
            .map(|c| match c {
                Some(c) => Item::action(c.label(), c.keys(), Message::Run(c)),
                None => Item::Separator,
            })
            .collect();
        // It opens above the button, at the left of the status bar.
        let at = iced::Point::new(8.0, self.window.height - ui::STATUS_HEIGHT);
        self.menu = Some(ui::menu::Menu { at, items });
    }

    // ── Files never saved ────────────────────────────────────────────────

    /// Ctrl+N opens an empty tab with no file yet. Saving asks where to put it.
    fn new_untitled(&mut self) -> Task<Message> {
        let dir = paths::untitled_dir();
        let number = (1..)
            .find(|n| self.tabs.position(&dir.join(format!("untitled-{n}"))).is_none())
            .unwrap_or(1);
        self.open_untitled(dir.join(format!("untitled-{number}")), "")
    }

    fn open_untitled(&mut self, path: PathBuf, text: &str) -> Task<Message> {
        let Ok(doc) = Document::from_bytes(b"") else { return Task::none() };
        let (indent, tab_width) =
            Self::indent_for(&self.settings, &path, "Plain Text", "", Default::default());
        let mut buffer = Buffer::new(doc, "Plain Text".to_owned(), indent);
        buffer.editor.auto_close = self.settings.auto_close;
        buffer.editor.tab_width = tab_width;
        if !text.is_empty() {
            buffer.recover(text);
        }
        self.tabs.insert(Tab::new(path, Body::Text(buffer)));
        self.focus_editor()
    }

    /// The files never saved that were open last time, from their backups.
    fn restore_untitled(&mut self) {
        let dir = paths::untitled_dir();
        let mut found: Vec<_> = backup::all(&paths::backups_dir())
            .into_iter()
            .filter(|b| b.path.parent() == Some(dir.as_path()))
            .collect();
        found.sort_by(|a, b| a.path.cmp(&b.path));
        for b in found {
            let _ = self.open_untitled(b.path, &b.text);
        }
    }

    /// The name a tab shows, like "Untitled 2", "Preview README.md" or the
    /// file's name.
    pub fn tab_name(&self, tab: &Tab) -> String {
        let t = &self.look.i18n;
        if let Some(n) = untitled_number(&tab.path) {
            return t.f("untitled", &[("number", &n)]);
        }
        if tab.is_preview() {
            return t.f("tabs.preview", &[("name", &tab.name())]);
        }
        tab.name()
    }

    // ── Save as, save all, reopen ────────────────────────────────────────

    /// With `format_on_save`, asks the language server to format the
    /// active file and returns a task that saves it anyway if the server
    /// takes too long. The save itself happens when the answer comes.
    fn format_before_save(&mut self) -> Option<Task<Message>> {
        let tab = self.tabs.active()?;
        let buffer = tab.buffer()?;
        let path = tab.path.clone();
        if !self.settings.for_language(&buffer.editor.syntax).format_on_save
            || !self.lsp.supports(&path, "documentFormattingProvider")
        {
            return None;
        }
        let (revision, indent) = (buffer.revision(), buffer.editor.indent);
        self.sync_lsp();
        if !self.lsp.format(&path, revision, indent.width, !indent.tabs) {
            return None;
        }
        self.format_then_save.insert(path.clone());
        Some(Task::perform(sleep(Duration::from_secs(2)), move |()| {
            Message::FormatTimedOut(path.clone())
        }))
    }

    /// The server formatted `path` (or never answered), so now it's saved.
    fn save_after_format(&mut self, path: &Path) -> Task<Message> {
        if !self.format_then_save.remove(path) {
            return Task::none();
        }
        let Some(index) = self.tabs.position(path) else { return Task::none() };
        self.save(index, false);
        self.sync_lsp();
        self.lsp.saved(path);
        self.refresh_git()
    }

    /// Asks where to save tab `index`, then saves it there, and closes it
    /// if `close`.
    fn save_as(&mut self, index: usize, close: bool) -> Task<Message> {
        let Some(tab) = self.tabs.iter().nth(index).filter(|t| t.buffer().is_some()) else {
            return Task::none();
        };
        let path = tab.path.clone();
        let (dir, name) = if is_untitled(tab) {
            (self.tree.as_ref().map(|t| t.root().to_path_buf()), String::new())
        } else {
            (path.parent().map(Path::to_path_buf), tab.name())
        };
        Task::perform(pick_save(dir, name), move |to| {
            Message::SaveAsPicked(path.clone(), to, close)
        })
    }

    fn save_as_picked(&mut self, from: &Path, to: Option<PathBuf>, close: bool) -> Task<Message> {
        let Some(to) = to else { return Task::none() };
        let Some(index) = self.tabs.position(from) else { return Task::none() };
        // Another tab showing the file being replaced would show old text.
        if let Some(other) = self.tabs.position(&to).filter(|&i| i != index) {
            if self.tabs.iter().nth(other).is_some_and(Tab::is_dirty) {
                let text =
                    self.look.i18n.f("prompt.unsaved", &[("name", &tree::display_name(&to))]);
                self.error(text);
                return Task::none();
            }
            let _ = self.remove_tab(other);
        }
        let Some(index) = self.tabs.position(from) else { return Task::none() };
        let Some(tab) = self.tabs.get_mut(index) else { return Task::none() };
        let Some(buffer) = tab.buffer_mut() else { return Task::none() };
        match document::write_atomic(&to, &buffer.encoded()) {
            Ok(modified) => buffer.mark_saved(modified),
            Err(e) => {
                let name = tree::display_name(&to);
                let text =
                    self.look.i18n.f("notice.save_failed", &[("name", &name), ("error", &e)]);
                self.error(text);
                return Task::none();
            }
        }
        if tab.backup.take().is_some() {
            self.backups.remove(from.to_path_buf());
        }
        tab.path = to.clone();
        let text = tab.buffer().map(Buffer::text).unwrap_or_default();
        let syntax = self.detect_syntax(&to, &text);
        if let Some(buffer) = self.tabs.get_mut(index).and_then(Tab::buffer_mut) {
            buffer.editor.syntax = syntax;
        }
        let name = tree::display_name(&to);
        let notice = self.look.i18n.f("notice.saved", &[("name", &name)]);
        self.info(notice);
        self.after_save(&to);
        if let Some(tree) = &mut self.tree {
            tree.refresh();
            self.files = None;
        }
        self.sync_lsp();
        if close {
            return self.remove_tab(index);
        }
        Task::batch([self.reveal_active(), git_base(to), self.refresh_git()])
    }

    /// Saves every tab with changes. Files never saved are left for you to
    /// save one by one, since each needs a name.
    fn save_all(&mut self) -> Task<Message> {
        for i in self.dirty_files() {
            self.save(i, false);
        }
        self.sync_lsp();
        self.refresh_git()
    }

    /// Saving on its own, with `autosave`. It leaves out the cleanups
    /// (trailing spaces, the last line break), since it happens while you type
    /// and would take away the space you just typed.
    fn autosave(&mut self) -> Task<Message> {
        self.saving_quietly = true;
        let task = self.save_all();
        self.saving_quietly = false;
        task
    }

    /// Ctrl+Shift+T reopens the tab closed last, at the line it was on.
    fn reopen_tab(&mut self) -> Task<Message> {
        while let Some((path, line, column)) = self.closed.pop() {
            if !path.is_file() || self.tabs.position(&path).is_some() {
                continue;
            }
            let open = self.open(path);
            let go = self.go_to_line(line, column);
            return Task::batch([open, go]);
        }
        let text = self.look.i18n.t("notice.reopened_none").to_owned();
        self.info(text);
        Task::none()
    }

    // ── Tab menu ─────────────────────────────────────────────────────────

    fn tab_menu(&mut self, at: iced::Point) {
        use ui::menu::Item;
        let Some(index) = self.tab_target.take() else { return };
        let Some(tab) = self.tabs.iter().nth(index) else { return };
        let path = tab.path.clone();
        let close = |which| Message::CloseTabs(index, which);
        let mut items = vec![
            Item::action("tabs.close", Some("Ctrl+W"), Message::CloseTab(index)),
            Item::action("tabs.close_others", None, close(CloseWhich::Others)),
            Item::action("tabs.close_right", None, close(CloseWhich::ToTheRight)),
            Item::action("tabs.close_saved", None, close(CloseWhich::Saved)),
            Item::action("tabs.close_all", None, close(CloseWhich::All)),
        ];
        if !is_untitled(tab) {
            items.extend([
                Item::Separator,
                Item::action(
                    "tree.copy_path",
                    None,
                    Message::Tree(TreeAction::CopyPath(path.clone())),
                ),
                Item::action(
                    "tree.copy_relative_path",
                    None,
                    Message::Tree(TreeAction::CopyRelativePath(path.clone())),
                ),
                Item::action("tree.reveal", None, Message::Tree(TreeAction::Reveal(path))),
                Item::action(
                    "cmd.local_history",
                    None,
                    Message::Run(crate::commands::Command::LocalHistory),
                ),
            ]);
        }
        items.extend([
            Item::Separator,
            Item::action("cmd.split", Some("Ctrl+\\"), Message::MoveToOtherSide(index)),
        ]);
        self.menu = Some(ui::menu::Menu { at, items });
    }

    /// Closes the tabs `which` names, on tab `index`'s side. Tabs with
    /// unsaved changes stay, and the first one asks what to do.
    fn close_tabs(&mut self, index: usize, which: CloseWhich) -> Task<Message> {
        let Some(pane) = self.tabs.iter().nth(index).map(|t| t.pane) else { return Task::none() };
        let targets: Vec<PathBuf> = self
            .tabs
            .in_pane(pane)
            .filter(|(i, t)| match which {
                CloseWhich::Others => *i != index,
                CloseWhich::ToTheRight => *i > index,
                CloseWhich::Saved => !t.is_dirty(),
                CloseWhich::All => true,
            })
            .map(|(_, t)| t.path.clone())
            .collect();
        let mut tasks = Vec::new();
        let mut asked = false;
        for path in targets {
            let Some(i) = self.tabs.iter().position(|t| t.path == path) else { continue };
            if self.tabs.iter().nth(i).is_some_and(Tab::is_dirty) {
                if !asked {
                    asked = true;
                    self.tabs.select(i);
                    tasks.push(self.close_tab(i));
                }
                continue;
            }
            tasks.push(self.remove_tab(i));
        }
        Task::batch(tasks)
    }

    // ── Language servers ─────────────────────────────────────────────────

    /// Starts the servers open files need and sends them edited text.
    fn sync_lsp(&mut self) {
        let root =
            self.tree.as_ref().map(|t| t.root().to_path_buf()).or_else(|| {
                self.tabs.active().and_then(|t| t.path.parent().map(Path::to_path_buf))
            });
        let Some(root) = root else { return };
        if let Some(error) = self.lsp.sync(&self.tabs, &root, &self.settings.language_servers) {
            let text = self.look.i18n.f("notice.lsp_failed", &[("error", &error)]);
            self.error(text);
        }
    }

    /// Sends edits [`LSP_DELAY`] after typing stops, rather than on every
    /// key. One timer at a time: when it fires during typing, it waits again.
    fn schedule_lsp_sync(&mut self) -> Task<Message> {
        self.schedule_lsp_sync_in(LSP_DELAY)
    }

    fn schedule_lsp_sync_in(&mut self, delay: Duration) -> Task<Message> {
        if self.lsp.sync_scheduled || self.lsp.stale.is_empty() {
            return Task::none();
        }
        self.lsp.sync_scheduled = true;
        Task::perform(sleep(delay), |()| Message::LspSync)
    }

    /// Keeps the completion list in step with the cursor, and opens it
    /// while typing a word or after a trigger character (`.`).
    fn follow_completion(&mut self, path: &Path, typed: Option<char>) {
        // A trigger character starts a new list, for what comes after it.
        let trigger = typed.is_some_and(|c| self.lsp.triggers(path).iter().any(|t| t.ends_with(c)));
        if trigger {
            self.lsp.completion = None;
        }
        if self.lsp.completion.is_some() {
            let Some(buffer) = self.tabs.active().and_then(Tab::buffer) else { return };
            self.lsp.refilter(path, buffer.editor.rope(), buffer.cursor_offset());
            return;
        }
        let start_list = typed.is_some_and(|c| {
            self.lsp.supports(path, "completionProvider")
                && (trigger || c.is_alphanumeric() || c == '_')
        });
        if !start_list {
            return;
        }
        self.sync_lsp();
        let Some(buffer) = self.tabs.active().and_then(Tab::buffer) else { return };
        let rope = buffer.editor.rope();
        let cursor = buffer.cursor_offset();
        self.lsp.complete(path, rope, word_start(rope, cursor), cursor);
    }

    fn lsp_action(&mut self, path: &Path, action: LspAction) -> Task<Message> {
        match action {
            LspAction::Hover(None) => self.lsp.hover = None,
            LspAction::Hover(Some(at)) => {
                let Some(buffer) = self.tabs.active().and_then(Tab::buffer) else {
                    return Task::none();
                };
                // Problems at that spot go on the card first.
                let problems: Vec<String> = self
                    .lsp
                    .problems(path, buffer.editor.rope())
                    .into_iter()
                    .filter(|(r, ..)| r.start <= at && at <= r.end.max(r.start + 1))
                    .map(|(_, _, m)| m.to_owned())
                    .collect();
                self.lsp.hover =
                    (!problems.is_empty()).then(|| (path.to_owned(), at, problems.join("\n\n")));
                if self.lsp.supports(path, "hoverProvider") {
                    self.sync_lsp();
                    if let Some(buffer) = self.tabs.active().and_then(Tab::buffer) {
                        self.lsp.hover(path, buffer.editor.rope(), at);
                    }
                }
            }
            LspAction::Definition(at) => {
                self.sync_lsp();
                let Some(buffer) = self.tabs.active().and_then(Tab::buffer) else {
                    return Task::none();
                };
                if !self.lsp.definition(path, buffer.editor.rope(), at) {
                    self.no_server();
                }
            }
            LspAction::Complete => {
                self.sync_lsp();
                let Some(buffer) = self.tabs.active().and_then(Tab::buffer) else {
                    return Task::none();
                };
                let (rope, cursor) = (buffer.editor.rope(), buffer.cursor_offset());
                if !self.lsp.complete(path, rope, word_start(rope, cursor), cursor) {
                    self.no_server();
                }
            }
            LspAction::Pick(pick) => {
                let Some(list) = &mut self.lsp.completion else { return Task::none() };
                let n = list.shown.len().max(1);
                match pick {
                    Pick::Up => list.selected = (list.selected + n - 1) % n,
                    Pick::Down => list.selected = (list.selected + 1) % n,
                    Pick::Cancel => self.lsp.completion = None,
                    Pick::Accept => {
                        let list = self.lsp.completion.take();
                        let Some(buffer) = self.active_buffer_mut() else { return Task::none() };
                        let cursor = buffer.cursor_offset();
                        if let Some((range, insert)) =
                            list.and_then(|l| l.chosen(buffer.editor.rope(), cursor))
                        {
                            buffer.replace_range(range, &insert);
                            self.lsp.edited(path.to_owned());
                            return self.schedule_lsp_sync();
                        }
                    }
                }
            }
            LspAction::Format => {
                self.sync_lsp();
                let Some(buffer) = self.tabs.active().and_then(Tab::buffer) else {
                    return Task::none();
                };
                let indent = buffer.editor.indent;
                if !self.lsp.format(path, buffer.revision(), indent.width, !indent.tabs) {
                    self.no_server();
                }
            }
            LspAction::Rename(at) => {
                let Some(buffer) = self.tabs.active().and_then(Tab::buffer) else {
                    return Task::none();
                };
                if !self.lsp.supports(path, "renameProvider") {
                    self.no_server();
                    return Task::none();
                }
                let word = buffer.editor.word_range(at);
                let query = buffer.editor.slice(word);
                self.palette = Some(Palette {
                    mode: PaletteMode::Rename { path: path.to_owned(), at },
                    query,
                    selected: 0,
                });
                return Task::batch([
                    operation::focus(PALETTE_ID),
                    operation::select_all(PALETTE_ID),
                ]);
            }
            LspAction::NextProblem { back } => {
                let Some(buffer) = self.tabs.active().and_then(Tab::buffer) else {
                    return Task::none();
                };
                let cursor = buffer.cursor_offset();
                let starts: Vec<usize> = self
                    .lsp
                    .problems(path, buffer.editor.rope())
                    .iter()
                    .map(|(r, ..)| r.start)
                    .collect();
                let next = if back {
                    starts.iter().rev().find(|&&s| s < cursor).or(starts.last())
                } else {
                    starts.iter().find(|&&s| s > cursor).or(starts.first())
                };
                let Some(&at) = next else {
                    let text = self.look.i18n.t("notice.no_problems").to_owned();
                    self.info(text);
                    return Task::none();
                };
                self.center_seq += 1;
                let seq = self.center_seq;
                if let Some(buffer) = self.active_buffer_mut() {
                    buffer.select(at..at);
                    buffer.center = seq;
                }
            }
        }
        Task::none()
    }

    fn no_server(&mut self) {
        let language = self
            .tabs
            .active()
            .and_then(Tab::buffer)
            .map(|b| b.editor.syntax.clone())
            .unwrap_or_default();
        let text = self.look.i18n.f("notice.no_lsp", &[("language", &language)]);
        self.error(text);
    }

    /// Does what server replies call for.
    fn apply_lsp(&mut self, effects: Vec<Effect>) -> Task<Message> {
        let mut tasks = Vec::new();
        for effect in effects {
            match effect {
                Effect::Open { path, range, encoding } => {
                    self.remember_place();
                    tasks.push(self.open(path));
                    self.center_seq += 1;
                    let seq = self.center_seq;
                    if let Some(buffer) = self.active_buffer_mut() {
                        let at = bia_core::lsp::text::offset(
                            buffer.editor.rope(),
                            range.start,
                            encoding,
                        );
                        buffer.select(at..at);
                        buffer.center = seq;
                    }
                }
                Effect::Edit { path, edits, encoding, revision } => {
                    if self.tabs.position(&path).is_none() {
                        tasks.push(self.open(path.clone()));
                    }
                    let Some(buffer) =
                        self.tabs.position(&path).and_then(|i| self.tabs.get_mut(i)?.buffer_mut())
                    else {
                        continue;
                    };
                    if revision.is_some_and(|r| r != buffer.revision()) {
                        continue;
                    }
                    let rope = buffer.editor.rope();
                    let mut ranges: Vec<(Range<usize>, String)> = edits
                        .iter()
                        .map(|(r, t)| (bia_core::lsp::text::offsets(rope, *r, encoding), t.clone()))
                        .collect();
                    ranges.sort_by_key(|(r, _)| (r.start, r.end));
                    ranges.dedup_by(|b, a| b.0.start < a.0.end);
                    buffer.replace_all(&ranges);
                    self.lsp.edited(path);
                    self.lsp.hover = None;
                }
                Effect::Notice(count) => {
                    let text = self.look.i18n.f("notice.renamed", &[("count", &count)]);
                    self.info(text);
                }
                Effect::Error(error) => {
                    let text = self.look.i18n.f("notice.lsp_error", &[("error", &error)]);
                    self.error(text);
                }
                Effect::Formatted(path) => tasks.push(self.save_after_format(&path)),
            }
        }
        tasks.push(self.schedule_lsp_sync());
        Task::batch(tasks)
    }

    // ── Git ──────────────────────────────────────────────────────────────

    /// Asks git, in the background, for the folder's status and for the
    /// committed text of every open file.
    fn refresh_git(&self) -> Task<Message> {
        let mut tasks: Vec<Task<Message>> = self
            .tabs
            .iter()
            .filter(|t| t.buffer().is_some())
            .map(|t| git_base(t.path.clone()))
            .collect();
        if let Some(root) = self.tree.as_ref().map(|t| t.root().to_path_buf()) {
            tasks.push(Task::perform(
                {
                    let root = root.clone();
                    async move { Arc::new(git::status(&root)) }
                },
                move |status| Message::GitStatus(root.clone(), status),
            ));
        }
        Task::batch(tasks)
    }

    /// Reopens a folder's files where their cursors were, then any file in
    /// it with unsaved edits in `backups/` (from a crash).
    fn restore_session(&mut self, root: &Path, session: Session) -> Task<Message> {
        for file in &session.files {
            if !file.path.is_file() || self.tabs.position(&file.path).is_some() {
                continue;
            }
            let _ = self.open(file.path.clone());
            self.center_seq += 1;
            let seq = self.center_seq;
            if let Some(buffer) = self.active_buffer_mut() {
                buffer.go_to(file.line, file.column);
                buffer.center = seq;
            }
        }
        let active = session.files.get(session.active).and_then(|f| self.tabs.position(&f.path));
        for b in backup::all(&paths::backups_dir()) {
            if b.path.starts_with(root) && b.path.is_file() && self.tabs.position(&b.path).is_none()
            {
                let _ = self.open(b.path);
            }
        }
        if let Some(i) = active {
            self.tabs.select(i);
        }
        let reveal = self.reveal_active();
        Task::batch([reveal, self.focus_editor()])
    }

    /// Writes the workspace's open files to `state.json`.
    fn save_session(&self) {
        let Some(root) = self.tree.as_ref().map(FileTree::root).filter(|_| self.remember_root)
        else {
            return;
        };
        // Tabs from other folders (settings.json, say) aren't part of it.
        let mine: Vec<&Tab> =
            self.tabs.iter().filter(|t| t.path.starts_with(root) && !t.is_preview()).collect();
        let files: Vec<OpenFile> = mine
            .iter()
            .map(|t| {
                let (line, column) = t.buffer().map_or((1, 1), Buffer::line_col);
                OpenFile { path: t.path.clone(), line: line - 1, column: column - 1 }
            })
            .collect();
        let front = self.tabs.active().map(|t| &t.path);
        let active = mine.iter().position(|t| Some(&t.path) == front).unwrap_or(0);
        let session = Session { folder: root.to_path_buf(), files, active };
        let _ = State::update(&paths::state_file(), |state| {
            if state.session(root) != Some(&session) {
                state.remember(session);
            }
        });
    }

    // ── Backups of unsaved edits ─────────────────────────────────────────

    /// Asks for a [`Message::BackupTick`] shortly, if some tab's backup is
    /// out of date and none is coming yet.
    fn schedule_backup(&mut self) -> Task<Message> {
        let stale = |t: &Tab| match t.buffer() {
            Some(b) if b.is_dirty() => t.backup != Some(b.revision()),
            _ => t.backup.is_some(),
        };
        if self.backup_scheduled || !self.tabs.iter().any(stale) {
            return Task::none();
        }
        self.backup_scheduled = true;
        Task::perform(sleep(BACKUP_DELAY), |()| Message::BackupTick)
    }

    /// Brings `backups/` in line with the tabs: one backup per file with
    /// unsaved edits, none for the rest.
    /// The writing happens on the backup thread; `tab.backup` says what
    /// was asked of it, and goes back to `None` if writing fails.
    fn write_backups(&mut self) -> Task<Message> {
        let mut tasks = Vec::new();
        for tab in self.tabs.iter_mut() {
            let path = tab.path.clone();
            let Some(buffer) = tab.buffer() else { continue };
            if buffer.is_dirty() {
                let revision = buffer.revision();
                if tab.backup != Some(revision) {
                    let (done, written) = iced::futures::channel::oneshot::channel();
                    let text = buffer.editor.rope().clone();
                    self.backups.write(path.clone(), text, buffer.modified, move |ok| {
                        let _ = done.send(ok);
                    });
                    tab.backup = Some(revision);
                    tasks.push(Task::perform(
                        async move { written.await.unwrap_or(false) },
                        move |ok| Message::BackupWritten(path.clone(), revision, ok),
                    ));
                }
            } else if tab.backup.take().is_some() {
                self.backups.remove(path);
            }
        }
        Task::batch(tasks)
    }

    /// Quitting: every tab is saved or its edits were discarded, so no
    /// backup is needed any more.
    fn exit(&mut self) -> Task<Message> {
        self.save_session();
        self.save_window();
        // Servers read EOF on their stdin and stop.
        self.lsp.restart();
        let dir = paths::backups_dir();
        for tab in self.tabs.iter_mut() {
            // Files never saved stay in their backups, up to the last key,
            // and come back next time.
            if untitled_number(&tab.path).is_some() {
                if let Some(buffer) = tab.buffer().filter(|b| b.is_dirty()) {
                    let _ = backup::write(
                        &dir,
                        &backup::Backup::new(tab.path.clone(), buffer.text(), None),
                    );
                }
                continue;
            }
            if tab.backup.take().is_some() {
                self.backups.remove(tab.path.clone());
            }
        }
        self.backups.flush();
        iced::exit()
    }

    /// The window's size, place and whether it was maximized, for next time.
    fn save_window(&self) {
        let (size, at, maximized) = (self.normal_size, self.window_position, self.maximized);
        let _ = State::update(&paths::state_file(), |state| {
            state.layout.window_size = Some((size.width, size.height));
            state.layout.window_position = at.map(|p| (p.x, p.y));
            state.layout.maximized = maximized;
        });
    }

    fn open(&mut self, path: PathBuf) -> Task<Message> {
        if self.tabs.active().is_none_or(|t| t.path != path) {
            self.remember_place();
        }
        if let Some(index) = self.tabs.position(&path) {
            return self.update(Message::SelectTab(index));
        }
        let i18n = &self.look.i18n;
        let body = match Document::load(&path) {
            Ok(doc) => {
                let syntax = self.detect_syntax(&path, &doc.text);
                let config = bia_core::editorconfig::for_file(&path);
                let (indent, tab_width) =
                    Self::indent_for(&self.settings, &path, &syntax, &doc.text, config);
                let mut buffer = Buffer::new(doc, syntax, indent);
                buffer.editorconfig = config;
                buffer.editor.auto_close = self.settings.auto_close;
                buffer.editor.tab_width = tab_width;
                self.recover(&path, &mut buffer);
                Body::Text(buffer)
            }
            Err(LoadError::Binary) => Body::Unsupported(i18n.t("editor.binary").into()),
            Err(LoadError::NotUtf8) => Body::Unsupported(i18n.t("editor.not_utf8").into()),
            Err(LoadError::BareCr) => Body::Unsupported(i18n.t("editor.bare_cr").into()),
            Err(LoadError::TooLarge(size)) => Body::Unsupported(i18n.f(
                "editor.too_large",
                &[
                    ("size", &document::human_size(size)),
                    ("max", &document::human_size(document::MAX_SIZE)),
                ],
            )),
            Err(LoadError::Io(e)) => {
                Body::Unsupported(i18n.f("editor.read_error", &[("error", &e)]))
            }
        };
        let backup = match &body {
            Body::Text(b) if b.is_dirty() => Some(b.revision()),
            _ => None,
        };
        let mut tab = Tab::new(path, body);
        tab.backup = backup;
        let is_text = matches!(tab.body, Body::Text(_));
        let base = is_text.then(|| git_base(tab.path.clone()));
        self.tabs.insert(tab);
        let reveal = self.reveal_active();
        self.sync_lsp();
        Task::batch([reveal, self.focus_editor()].into_iter().chain(base))
    }

    /// Ctrl+Shift+V on a Markdown file opens its preview on the other side
    /// of the split, or closes it. On a preview tab, closes it.
    fn toggle_preview(&mut self) -> Task<Message> {
        let Some(tab) = self.tabs.active() else { return Task::none() };
        if tab.is_preview() {
            return self.remove_tab(self.tabs.active_index());
        }
        if !self.is_markdown(tab) {
            return Task::none();
        }
        let path = tab.path.clone();
        if let Some(preview) = self.tabs.preview_of(&path) {
            return self.remove_tab(preview);
        }
        if let Some(buffer) = self.active_buffer_mut() {
            buffer.set_preview(true);
        }
        self.tabs.insert_beside(Tab::new(path, Body::Preview));
        Task::none()
    }

    /// A link in a preview. A link to another file opens it here; BIA never
    /// opens web addresses itself (see docs/SEALED.md), so those are
    /// copied for you to open wherever you like.
    fn link_clicked(&mut self, url: String) -> Task<Message> {
        if url.starts_with('#') {
            return Task::none();
        }
        let is_web = url.contains("://") || url.starts_with("mailto:");
        if !is_web {
            let target = url.split(['#', '?']).next().unwrap_or_default().replace("%20", " ");
            // The previews in sight are the ones the click could come from.
            let found = [Pane::Left, Pane::Right]
                .into_iter()
                .filter_map(|pane| self.tabs.active_in(pane))
                .filter(|(_, t)| t.is_preview())
                .filter_map(|(_, t)| t.path.parent().map(|dir| dir.join(&target)))
                .find(|p| p.is_file());
            if let Some(file) = found {
                let file = fs::canonicalize(&file).unwrap_or(file);
                return self.open(file);
            }
        }
        self.info(url.clone());
        iced::clipboard::write(url)
    }

    /// Puts back unsaved edits to `path` left in `backups/`.
    fn recover(&mut self, path: &Path, buffer: &mut Buffer) {
        let dir = paths::backups_dir();
        let Some(b) = backup::read(&dir, path) else { return };
        if b.text == buffer.text() {
            self.backups.remove(path.to_owned());
            return;
        }
        buffer.recover(&b.text);
        // If the file changed on disk after the edits began, saving asks
        // before overwriting it.
        if b.modified() != buffer.modified {
            buffer.modified = b.modified();
        }
        let text = self.look.i18n.f("notice.recovered", &[("name", &tree::display_name(path))]);
        self.info(text);
    }

    /// Opens `path`, at a 1-based line and column if given.
    fn open_at(&mut self, path: PathBuf, at: Option<(usize, Option<usize>)>) -> Task<Message> {
        let task = self.open(path);
        match at {
            Some((line, column)) => {
                self.go_to_line(line.saturating_sub(1), column.unwrap_or(1).saturating_sub(1))
            }
            None => task,
        }
    }

    /// Keeps the explorer in step with the active tab.
    /// Keeps the explorer in step with the active tab. It opens the folders
    /// down to it, selects it, and scrolls to it if it's out of sight.
    fn reveal_active(&mut self) -> Task<Message> {
        let Some(path) = self.tabs.active().map(|t| t.path.clone()) else { return Task::none() };
        let Some(tree) = &mut self.tree else { return Task::none() };
        if !path.starts_with(tree.root()) {
            return Task::none();
        }
        tree.reveal(&path);
        self.selected = Some(path.clone());
        let Some(index) = tree.rows().iter().position(|r| r.path == path) else {
            return Task::none();
        };
        let height = ui::row_height(self);
        let (offset, in_sight) = self.tree_viewport;
        let y = index as f32 * height;
        if y >= offset && y + height <= offset + in_sight {
            return Task::none();
        }
        let y = (y - in_sight / 3.0).max(0.0);
        operation::scroll_to(TREE_ID, iced::widget::scrollable::AbsoluteOffset { x: 0.0, y })
    }

    fn close_tab(&mut self, index: usize) -> Task<Message> {
        let Some(tab) = self.tabs.get_mut(index) else { return Task::none() };
        if tab.is_dirty() {
            self.dialog = Some(Dialog::Close(tab.path.clone()));
            return Task::none();
        }
        self.remove_tab(index)
    }

    fn remove_tab(&mut self, index: usize) -> Task<Message> {
        let Some(tab) = self.tabs.remove(index) else { return Task::none() };
        if let Some(buffer) = tab.buffer().filter(|_| !is_untitled(&tab)) {
            let (line, column) = buffer.line_col();
            self.closed.retain(|(p, ..)| p != &tab.path);
            self.closed.push((tab.path.clone(), line - 1, column - 1));
            if self.closed.len() > 30 {
                self.closed.remove(0);
            }
        }
        if tab.is_preview() {
            // Nobody looks at the rendered Markdown anymore.
            if let Some(i) = self.tabs.position(&tab.path)
                && let Some(buffer) = self.tabs.get_mut(i).and_then(Tab::buffer_mut)
            {
                buffer.set_preview(false);
            }
        } else if let Some(preview) = self.tabs.preview_of(&tab.path) {
            // A preview goes with its file.
            self.tabs.remove(preview);
        }
        if tab.backup.is_some() {
            self.backups.remove(tab.path);
        }
        if self.tabs.is_empty() {
            self.find = None;
        }
        self.sync_lsp();
        Task::none()
    }

    fn answer(&mut self, choice: Choice) -> Task<Message> {
        let Some(dialog) = self.dialog.take() else { return Task::none() };
        match (dialog, choice) {
            (_, Choice::Cancel) => {}
            (Dialog::Busy(Closing::Quit), _) => return self.quit(false),
            (Dialog::Busy(Closing::Terminal(id)), _) => return self.close_terminal(id),
            (Dialog::Trash(path), _) => return self.trash(&path),
            (Dialog::Unsaved(after), Choice::Save) => {
                let all_saved = self.dirty_files().into_iter().all(|i| self.save(i, false));
                if all_saved {
                    return self.after_unsaved(after);
                }
            }
            (Dialog::Unsaved(after), Choice::Discard) => return self.after_unsaved(after),
            (Dialog::Close(path), Choice::Save) if untitled_number(&path).is_some() => {
                if let Some(i) = self.tabs.position(&path) {
                    return self.save_as(i, true);
                }
            }
            (Dialog::Close(path), choice) => {
                if let Some(i) = self.tabs.position(&path)
                    && (choice == Choice::Discard || self.save(i, false))
                {
                    return self.remove_tab(i);
                }
            }
            (Dialog::Conflict(path), Choice::Save) => {
                if let Some(i) = self.tabs.position(&path) {
                    self.save(i, true);
                }
            }
            (Dialog::Conflict(path), Choice::Discard) => self.reload(&path),
        }
        Task::none()
    }

    /// Unsaved files were saved, or you chose to drop their edits.
    fn after_unsaved(&mut self, after: After) -> Task<Message> {
        match after {
            After::Quit => self.exit(),
            After::CloseFolder => self.close_folder(false),
        }
    }

    /// Replaces a tab's text with the file on disk, dropping unsaved edits.
    fn reload(&mut self, path: &Path) {
        let Some(buffer) =
            self.tabs.position(path).and_then(|i| self.tabs.get_mut(i)?.buffer_mut())
        else {
            return;
        };
        match Document::load(path) {
            Ok(doc) => buffer.reload(doc),
            Err(e) => {
                let text = self.look.i18n.f("editor.read_error", &[("error", &e)]);
                self.error(text);
            }
        }
    }

    /// Writes tab `index` to disk. Returns whether it is now saved.
    ///
    /// If the file changed on disk since it was opened, nothing is written
    /// and the conflict dialog opens, unless `overwrite` is set.
    fn save(&mut self, index: usize, overwrite: bool) -> bool {
        let Some(tab) = self.tabs.get_mut(index) else { return false };
        let name = tab.name();
        let path = tab.path.clone();
        let Some(buffer) = tab.buffer_mut() else { return false };
        let own = self.settings.for_language(&buffer.editor.syntax);
        let config = buffer.editorconfig;
        let trim = config.trim_trailing_whitespace.unwrap_or(own.trim_trailing_whitespace_on_save);
        let final_newline = config.insert_final_newline.unwrap_or(own.ensure_final_newline_on_save);
        // The cleanups are edits like any other, and Ctrl+Z takes them back.
        if (trim || final_newline) && !self.saving_quietly {
            let text = buffer.text();
            let mut edits: Vec<(Range<usize>, String)> = Vec::new();
            if trim {
                edits.extend(
                    editing::trailing_whitespace(&text).into_iter().map(|r| (r, String::new())),
                );
            }
            if final_newline && editing::needs_final_newline(&text) {
                edits.push((text.len()..text.len(), buffer.line_ending.as_str().to_owned()));
            }
            if !edits.is_empty() {
                buffer.replace_all(&edits);
            }
        }

        if buffer.is_dirty() {
            let on_disk = document::modified(&path);
            if !overwrite && on_disk.is_some() && on_disk != buffer.modified {
                self.dialog = Some(Dialog::Conflict(path));
                return false;
            }
            match document::write_atomic(&path, &buffer.encoded()) {
                Ok(modified) => {
                    buffer.mark_saved(modified);
                    // Not worth an error, since the file itself is saved.
                    let _ = bia_core::history::record(&paths::history_dir(), &path, &buffer.text());
                }
                Err(e) => {
                    let text =
                        self.look.i18n.f("notice.save_failed", &[("name", &name), ("error", &e)]);
                    self.error(text);
                    return false;
                }
            }
        }
        if !self.saving_quietly {
            let text = self.look.i18n.f("notice.saved", &[("name", &name)]);
            self.info(text);
        }
        self.after_save(&path);
        if let Some(tree) = &mut self.tree {
            tree.refresh();
            self.files = None;
        }
        true
    }

    /// Saving BIA's own config files applies them immediately.
    fn after_save(&mut self, path: &Path) {
        let is_config = path == paths::settings_file()
            || path.starts_with(paths::themes_dir())
            || path.starts_with(paths::languages_dir());
        if !is_config {
            return;
        }
        let json = fs::read_to_string(paths::settings_file()).unwrap_or_default();
        match Settings::from_json(&json) {
            Ok(settings) => {
                self.apply_settings(settings);
                self.warn_unknown_keys();
            }
            Err(e) => {
                let text = self.settings_error_text(&SettingsError::Parse(e));
                self.error(text);
            }
        }
    }

    fn apply_settings(&mut self, settings: Settings) {
        if settings.language_servers != self.settings.language_servers {
            self.lsp.restart();
        }
        let (look, errors) = Look::load(&settings, self.look.generation + 1);
        self.look = look;
        if let Some(tree) = &mut self.tree {
            tree.set_hidden(settings.hidden.clone());
            self.files = None;
        }
        // Grammars may have been added or removed, and `tab_size` may have
        // changed: detect every file again.
        let detected: Vec<(usize, String, Indent, usize)> = self
            .tabs
            .iter()
            .enumerate()
            .filter_map(|(i, t)| {
                let text = t.buffer()?.text();
                let syntax = self.detect_syntax_with(&settings, &t.path, &text);
                let config = t.buffer()?.editorconfig;
                let (indent, tab_width) =
                    Self::indent_for(&settings, &t.path, &syntax, &text, config);
                Some((i, syntax, indent, tab_width))
            })
            .collect();
        for (i, syntax, indent, tab_width) in detected {
            if let Some(buffer) = self.tabs.get_mut(i).and_then(Tab::buffer_mut) {
                buffer.editor.syntax = syntax;
                buffer.editor.indent = indent;
                buffer.editor.auto_close = settings.auto_close;
                buffer.editor.tab_width = tab_width;
            }
        }
        self.font_size_setting = settings.font_size;
        self.settings = settings;
        match errors.into_iter().next() {
            Some(e) => self.error(e),
            None => {
                let text = self.look.i18n.t("notice.reloaded").to_owned();
                self.info(text);
            }
        }
    }

    /// Back from another app: pick up outside changes to files and folders.
    fn on_focus(&mut self) {
        if let Some(tree) = &mut self.tree {
            tree.refresh();
            self.files = None;
        }
        let mut conflicts = Vec::new();
        for tab in self.tabs.iter_mut() {
            let path = tab.path.clone();
            let Some(buffer) = tab.buffer_mut() else { continue };
            let on_disk = document::modified(&path);
            if on_disk.is_none() || on_disk == buffer.modified {
                continue;
            }
            if buffer.is_dirty() {
                // Kept as is. Saving will ask before overwriting.
                conflicts.push(tree::display_name(&path));
            } else if let Ok(doc) = Document::load(&path) {
                buffer.reload(doc);
            }
        }
        if let Some(name) = conflicts.first() {
            let text = self.look.i18n.f("notice.changed_on_disk", &[("name", name)]);
            self.error(text);
        }
    }

    // ── Editor scrolling ─────────────────────────────────────────────────

    /// Moves the cursor to a 0-based line and column of the active file and
    /// has the editor center it.
    fn go_to_line(&mut self, line: usize, column: usize) -> Task<Message> {
        self.remember_place();
        self.center_seq += 1;
        let seq = self.center_seq;
        let Some(buffer) = self.active_buffer_mut() else { return Task::none() };
        buffer.go_to(line, column);
        buffer.center = seq;
        self.focus_editor()
    }

    // ── Explorer actions ─────────────────────────────────────────────────

    /// The folder "new file" and "new folder" act on, or the open folder.
    pub fn target_dir_or_root(&self) -> PathBuf {
        self.target_dir().unwrap_or_default()
    }

    /// The folder a dragged row would land in, while one is dragged.
    pub fn drop_target(&self) -> Option<PathBuf> {
        let drag = self.tree_drag.as_ref().filter(|d| d.moved)?;
        let over = drag.over.as_ref()?;
        let dir = if over.is_dir() { over.clone() } else { over.parent()?.to_path_buf() };
        (!dir.starts_with(&drag.from) && Some(dir.as_path()) != drag.from.parent()).then_some(dir)
    }

    /// Folder that "new file" / "new folder" act on.
    fn target_dir(&self) -> Option<PathBuf> {
        let root = self.tree.as_ref()?.root().to_path_buf();
        Some(match &self.selected {
            Some(p) if p.is_dir() => p.clone(),
            Some(p) => p.parent().map_or(root.clone(), Path::to_path_buf),
            None => root,
        })
    }

    /// What's selected in the tree, other than the root folder.
    fn selected_in_tree(&self) -> Option<PathBuf> {
        let root = self.tree.as_ref()?.root();
        self.selected.clone().filter(|p| p != root && p.exists())
    }

    /// The mouse came up after pressing a row. A click opens the file or
    /// the folder, a drag onto another folder moves (or with Alt, copies)
    /// what was dragged there.
    fn tree_release(&mut self) -> Task<Message> {
        let Some(drag) = self.tree_drag.take() else { return Task::none() };
        if !drag.moved {
            let path = drag.from;
            if path.is_dir() {
                if let Some(tree) = &mut self.tree {
                    tree.toggle(&path);
                }
                return Task::none();
            }
            return self.open(path);
        }
        self.tree_drag = Some(drag);
        let target = self.drop_target();
        let Some(drag) = self.tree_drag.take() else { return Task::none() };
        let Some(dir) = target else { return Task::none() };
        if self.modifiers.alt() {
            return self.copy_into(&drag.from, &dir);
        }
        self.move_into(&drag.from, &dir);
        Task::none()
    }

    /// The menu for a right click in the sidebar, for the row it landed
    /// on, or for the open folder.
    fn tree_menu(&mut self, at: iced::Point) {
        use ui::menu::Item;
        let Some(root) = self.tree.as_ref().map(|t| t.root().to_path_buf()) else { return };
        let target = self.menu_target.take();
        self.prompt = None;
        let path = target.clone().unwrap_or_else(|| root.clone());
        let dir = if path.is_dir() {
            path.clone()
        } else {
            path.parent().map_or(root.clone(), Path::to_path_buf)
        };
        let tree = |a: TreeAction| Message::Tree(a);
        let mut items = vec![
            Item::action("tree.new_file", None, tree(TreeAction::NewFile(dir.clone()))),
            Item::action("tree.new_folder", None, tree(TreeAction::NewFolder(dir.clone()))),
            Item::Separator,
        ];
        if let Some(p) = &target {
            self.selected = Some(p.clone());
            items.extend([
                Item::action("tree.cut", Some("Ctrl+X"), tree(TreeAction::Cut(p.clone()))),
                Item::action("tree.copy", Some("Ctrl+C"), tree(TreeAction::Copy(p.clone()))),
            ]);
        }
        items.extend([Item::action(
            "tree.paste",
            Some("Ctrl+V"),
            tree(TreeAction::Paste(dir.clone())),
        )]);
        if let Some(p) = &target {
            items.push(Item::action(
                "tree.duplicate",
                None,
                tree(TreeAction::Duplicate(p.clone())),
            ));
        }
        items.extend([
            Item::Separator,
            Item::action("tree.copy_path", None, tree(TreeAction::CopyPath(path.clone()))),
            Item::action(
                "tree.copy_relative_path",
                None,
                tree(TreeAction::CopyRelativePath(path.clone())),
            ),
        ]);
        if let Some(p) = &target {
            items.extend([
                Item::Separator,
                Item::action("tree.rename", Some("F2"), tree(TreeAction::Rename(p.clone()))),
                Item::danger("tree.trash", Some("Delete"), tree(TreeAction::Trash(p.clone()))),
            ]);
        }
        items.extend([
            Item::Separator,
            Item::action("tree.reveal", None, tree(TreeAction::Reveal(path.clone()))),
            Item::action("tree.open_terminal", None, tree(TreeAction::OpenTerminal(dir))),
            Item::Separator,
            Item::action("tree.collapse_all", None, tree(TreeAction::CollapseAll)),
            Item::action("tree.refresh", None, tree(TreeAction::Refresh)),
        ]);
        self.menu = Some(ui::menu::Menu { at, items });
    }

    fn tree_action(&mut self, action: TreeAction) -> Task<Message> {
        let root = self.tree.as_ref().map(|t| t.root().to_path_buf());
        let new_file = matches!(action, TreeAction::NewFile(_));
        let cut = matches!(action, TreeAction::Cut(_));
        let relative = matches!(action, TreeAction::CopyRelativePath(_));
        match action {
            TreeAction::NewFile(dir) | TreeAction::NewFolder(dir) if self.tree.is_some() => {
                if let Some(tree) = &mut self.tree
                    && Some(&dir) != root.as_ref()
                {
                    tree.reveal(&dir.join("x"));
                    tree.expand(&dir);
                }
                let kind =
                    if new_file { PromptKind::NewFile(dir) } else { PromptKind::NewFolder(dir) };
                self.prompt = Some(Prompt { kind, value: String::new(), error: None });
                return operation::focus(PROMPT_ID);
            }
            TreeAction::NewFile(_) | TreeAction::NewFolder(_) => {}
            TreeAction::Rename(path) => return self.start_rename(path),
            TreeAction::Duplicate(path) => {
                if let Some(dir) = path.parent().map(Path::to_path_buf) {
                    return self.copy_into(&path, &dir);
                }
            }
            TreeAction::Trash(path) => {
                // Trashing a file would throw away edits that only live in a tab.
                if let Some(tab) = self.tabs.dirty_under(&path) {
                    let text = self.look.i18n.f("prompt.unsaved", &[("name", &tab.name())]);
                    self.error(text);
                } else {
                    self.dialog = Some(Dialog::Trash(path));
                }
            }
            TreeAction::Cut(path) | TreeAction::Copy(path) => {
                let text = path.display().to_string();
                self.file_clipboard = Some((path, cut));
                // The path goes to the system clipboard too, so pasting it
                // anywhere works, and so BIA can tell, when you paste, whether
                // something newer was copied since.
                return iced::clipboard::write(text);
            }
            TreeAction::Paste(dir) => {
                return iced::clipboard::read()
                    .map(move |text| Message::PasteFiles(dir.clone(), text));
            }
            TreeAction::CopyPath(path) | TreeAction::CopyRelativePath(path) => {
                let text = match root.as_deref().and_then(|r| path.strip_prefix(r).ok()) {
                    Some(rel) if relative => rel.display().to_string(),
                    _ => path.display().to_string(),
                };
                let notice = self.look.i18n.f("notice.copied", &[("text", &text)]);
                self.info(notice);
                return iced::clipboard::write(text);
            }
            TreeAction::Reveal(path) => {
                // Asking the file manager can take a moment, so not on this thread.
                return Task::perform(
                    async move { reveal_in_file_manager(&path).err().map(|e| e.to_string()) },
                    Message::Failed,
                );
            }
            TreeAction::OpenTerminal(dir) => return self.new_terminal_in(dir),
            TreeAction::CollapseAll => {
                if let Some(tree) = &mut self.tree {
                    tree.collapse_all();
                }
            }
            TreeAction::Refresh => {
                if let Some(tree) = &mut self.tree {
                    tree.refresh();
                    self.files = None;
                }
                return self.refresh_git();
            }
        }
        Task::none()
    }

    /// Opens the name field on the row of `path`, with the name selected
    /// up to its extension, so typing replaces just the name.
    fn start_rename(&mut self, path: PathBuf) -> Task<Message> {
        let name = tree::display_name(&path);
        let stem = match name.rfind('.') {
            Some(i) if i > 0 && path.is_file() => name[..i].chars().count(),
            _ => name.chars().count(),
        };
        self.reveal_in_tree(&path);
        self.prompt = Some(Prompt { kind: PromptKind::Rename(path), value: name, error: None });
        Task::batch([operation::focus(PROMPT_ID), operation::select_range(PROMPT_ID, 0, stem)])
    }

    fn submit_prompt(&mut self) -> Task<Message> {
        let Some(prompt) = &mut self.prompt else { return Task::none() };
        let i18n = &self.look.i18n;
        let result: Result<Option<PathBuf>, FsError> = match &prompt.kind {
            PromptKind::NewFile(dir) => fs_ops::create_file(dir, &prompt.value).map(Some),
            PromptKind::NewFolder(dir) => fs_ops::create_dir(dir, &prompt.value).map(Some),
            PromptKind::Rename(from) => fs_ops::rename(from, &prompt.value),
        };
        let path = match result {
            Ok(path) => path,
            Err(e) => {
                // The field stays open, so you can fix the name.
                let text = fs_error_text(i18n, e);
                prompt.error = Some(text.clone());
                self.error(text);
                return Task::none();
            }
        };
        let Some(prompt) = self.prompt.take() else { return Task::none() };
        match (prompt.kind, path) {
            (PromptKind::NewFile(_), Some(path)) => {
                self.reveal_in_tree(&path);
                return self.open(path);
            }
            (PromptKind::NewFolder(_), Some(path)) => {
                self.reveal_in_tree(&path);
                if let Some(tree) = &mut self.tree {
                    tree.expand(&path);
                }
            }
            (PromptKind::Rename(from), Some(to)) => self.moved(&from, &to),
            // Renaming to the same name changes nothing.
            _ => {}
        }
        Task::none()
    }

    /// `from` is now at `to`, renamed or moved, so the tree, the tabs and the
    /// backups follow it.
    fn moved(&mut self, from: &Path, to: &Path) {
        if let Some(tree) = &mut self.tree {
            tree.renamed(from, to);
            self.files = None;
        }
        // A new extension may mean a new language.
        for i in self.tabs.renamed(from, to) {
            let Some(tab) = self.tabs.get_mut(i) else { continue };
            // The backup is filed under the old path; the next tick
            // writes it under the new one.
            if tab.backup.take().is_some()
                && let Ok(rest) = tab.path.strip_prefix(to)
            {
                self.backups.remove(from.join(rest));
            }
            let (path, text) = match tab.buffer() {
                Some(b) => (tab.path.clone(), b.text()),
                None => continue,
            };
            let syntax = self.detect_syntax(&path, &text);
            if let Some(buffer) = self.tabs.get_mut(i).and_then(Tab::buffer_mut) {
                buffer.editor.syntax = syntax;
            }
        }
        self.reveal_in_tree(to);
    }

    fn move_into(&mut self, from: &Path, dir: &Path) {
        match fs_ops::move_into(from, dir) {
            Ok(Some(to)) => self.moved(from, &to),
            Ok(None) => {}
            Err(e) => {
                let text = fs_error_text(&self.look.i18n, e);
                self.error(text);
            }
        }
    }

    /// Copies `from` into `dir`. When the copy had to get another name
    /// (`notes copy.md`), its name field opens so you can pick a better one.
    fn copy_into(&mut self, from: &Path, dir: &Path) -> Task<Message> {
        match fs_ops::copy_into(from, dir) {
            Ok(to) => {
                if to.file_name() != from.file_name() {
                    return self.start_rename(to);
                }
                self.reveal_in_tree(&to);
            }
            Err(e) => {
                let text = fs_error_text(&self.look.i18n, e);
                self.error(text);
            }
        }
        Task::none()
    }

    /// Pastes into `dir`: what was cut or copied in the tree, if the
    /// clipboard still holds its path, or else the files the clipboard
    /// names (copied in a file manager, say).
    fn paste_files(&mut self, dir: &Path, text: Option<&str>) -> Task<Message> {
        let text = text.unwrap_or_default();
        if let Some((path, cut)) = self.file_clipboard.clone()
            && text.trim() == path.display().to_string()
        {
            if cut {
                self.file_clipboard = None;
                self.move_into(&path, dir);
                return Task::none();
            }
            return self.copy_into(&path, dir);
        }
        let paths = text::clipboard_paths(text);
        if let [single] = paths.as_slice() {
            return self.copy_into(single, dir);
        }
        for path in paths.iter().filter(|p| p.exists()) {
            if let Err(e) = fs_ops::copy_into(path, dir) {
                let text = fs_error_text(&self.look.i18n, e);
                self.error(text);
            }
        }
        if let Some(tree) = &mut self.tree {
            tree.refresh();
            self.files = None;
        }
        Task::none()
    }

    /// Moves `path` to the system's trash, after the dialog said yes.
    fn trash(&mut self, path: &Path) -> Task<Message> {
        if let Err(e) = trash::delete(path) {
            let text = self.look.i18n.f("notice.fs_error", &[("error", &e)]);
            self.error(text);
            return Task::none();
        }
        let mut tasks = Vec::new();
        while let Some(i) = self.tabs.iter().position(|t| t.path.starts_with(path)) {
            tasks.push(self.remove_tab(i));
        }
        self.selected = None;
        if let Some(tree) = &mut self.tree {
            tree.refresh();
            self.files = None;
        }
        Task::batch(tasks)
    }

    fn reveal_in_tree(&mut self, path: &Path) {
        if let Some(tree) = &mut self.tree {
            tree.reveal(path);
            tree.refresh();
            self.files = None;
        }
        self.selected = Some(path.to_path_buf());
    }

    // ── Find ─────────────────────────────────────────────────────────────

    /// Counts matches now, if the text or the query changed since they
    /// were last counted, so "2 of 5" stays true.
    fn refresh_find(&mut self) {
        let Some(find) = &mut self.find else { return };
        let Some(buffer) = self.tabs.active().and_then(Tab::buffer) else { return };
        let key = (buffer.id, buffer.revision());
        if find.searched == Some(key) {
            return;
        }
        let (matches, error) = match find.matcher() {
            Ok(m) => (m.find_all(&buffer.text()), None),
            Err(QueryError::Empty) => (Vec::new(), None),
            Err(QueryError::Invalid(e)) => (Vec::new(), Some(e)),
        };
        find.matches = matches;
        find.error = error;
        find.current = find.current.filter(|&i| i < find.matches.len());
        find.searched = Some(key);
    }

    /// Matches counted in the background, kept if they're still for what
    /// the find bar and the text are now.
    fn find_done(&mut self, found: &FindResult) {
        let Some(find) = &mut self.find else { return };
        if find.running == Some((found.buffer, found.revision)) {
            find.running = None;
        }
        let current = self.tabs.active().and_then(Tab::buffer).map(|b| (b.id, b.revision()));
        if current != Some((found.buffer, found.revision))
            || find.query != found.query
            || find.options != found.options
        {
            return;
        }
        find.matches.clone_from(&found.matches);
        find.error = None;
        find.current = find.current.filter(|&i| i < find.matches.len());
        find.searched = current;
    }

    /// Brings what follows the active file's text up to date: find
    /// matches, the Markdown preview and the git marks. Runs after every
    /// message. Up to [`LIVE_LIMIT`], it all happens at once; bigger files
    /// wait for typing to pause, and the diff and the search run in the
    /// background.
    fn follow_edits(&mut self) -> Task<Message> {
        let Some(buffer) = self.tabs.active_mut().and_then(Tab::buffer_mut) else {
            return Task::none();
        };
        let key = (buffer.id, buffer.revision());
        // An edit (not a switch to another file) starts the wait over.
        if self.follow.seen.is_some_and(|(id, revision)| id == key.0 && revision != key.1) {
            self.follow.edited_at = Instant::now();
        }
        self.follow.seen = Some(key);
        buffer.git.follow(&buffer.editor);

        let find_stale =
            self.find.as_ref().is_some_and(|f| f.searched != Some(key) && f.running != Some(key));
        let base = buffer.git.wanted(&buffer.editor);
        if !find_stale && base.is_none() && !buffer.markdown_stale() {
            return Task::none();
        }
        let live = buffer.editor.len() <= LIVE_LIMIT;
        if !live {
            let quiet = self.follow.edited_at.elapsed();
            if quiet < FOLLOW_DELAY {
                if self.follow.scheduled {
                    return Task::none();
                }
                self.follow.scheduled = true;
                return Task::perform(sleep(FOLLOW_DELAY - quiet), |()| Message::FollowEdits);
            }
        }

        buffer.refresh_markdown();
        let mut tasks = Vec::new();
        if let Some(base) = base {
            let (id, revision) = key;
            buffer.git.start(revision);
            if live {
                let marks = git::marks(&base, &buffer.text());
                buffer.git.finish(&buffer.editor, revision, marks);
            } else {
                let rope = buffer.editor.rope().clone();
                tasks.push(Task::perform(
                    async move { git::marks(&base, &rope.to_string()) },
                    move |marks| Message::GitMarks(id, revision, marks),
                ));
            }
        }
        if find_stale {
            if live {
                self.refresh_find();
            } else if let Some(find) = &mut self.find {
                match find.matcher() {
                    Ok(matcher) => {
                        find.running = Some(key);
                        let rope = buffer.editor.rope().clone();
                        let (query, options) = (find.query.clone(), find.options);
                        tasks.push(Task::perform(
                            async move {
                                let matches = matcher.find_all(&rope.to_string());
                                let (buffer, revision) = key;
                                Arc::new(FindResult { buffer, revision, query, options, matches })
                            },
                            Message::FindDone,
                        ));
                    }
                    // An empty or invalid query costs nothing to count.
                    Err(_) => self.refresh_find(),
                }
            }
        }
        Task::batch(tasks)
    }

    /// Selects the next (or previous) match. `to_editor` moves the focus to
    /// the editor; replacing keeps it in the replace field instead.
    fn find_step(&mut self, forward: bool, to_editor: bool) -> Task<Message> {
        self.refresh_find();
        let Some(find) = &self.find else { return Task::none() };
        let (matches, previous) = (find.matches.clone(), find.current);
        let Some(buffer) = self.active_buffer_mut() else { return Task::none() };
        let current = if matches.is_empty() {
            None
        } else if forward {
            let cursor = buffer.cursor_offset();
            Some(matches.iter().position(|m| m.start >= cursor).unwrap_or(0))
        } else {
            let n = matches.len();
            Some(previous.map_or(n - 1, |i| (i.min(n - 1) + n - 1) % n))
        };
        if let Some(i) = current {
            buffer.select(matches[i].clone());
        }
        if let Some(find) = &mut self.find {
            find.current = current;
        }
        match (current, to_editor) {
            (None, _) => Task::none(),
            (Some(_), true) => self.focus_editor(),
            (Some(_), false) => Task::none(),
        }
    }

    /// Replaces the selected match, then selects the next one. The first
    /// press only selects a match, so you see what will change.
    fn replace_one(&mut self) -> Task<Message> {
        let Some(find) = &self.find else { return Task::none() };
        let with = find.replace.clone().unwrap_or_default();
        let target = find.current.and_then(|i| find.matches.get(i).cloned());
        let matcher = find.matcher().ok();
        if let (Some(range), Some(matcher), Some(buffer)) =
            (target, matcher, self.active_buffer_mut())
            && buffer.selection_range().as_ref() == Some(&range)
            && let Some(new) = matcher.replacement_at(&buffer.text(), range.clone(), &with)
        {
            buffer.replace_range(range, &new);
        }
        self.find_step(true, false)
    }

    fn replace_all(&mut self) -> Task<Message> {
        let Some(find) = &self.find else { return Task::none() };
        let with = find.replace.clone().unwrap_or_default();
        let Ok(matcher) = find.matcher() else { return Task::none() };
        let Some(buffer) = self.active_buffer_mut() else { return Task::none() };
        let edits = matcher.replacements(&buffer.text(), &with);
        if edits.is_empty() {
            return Task::none();
        }
        buffer.replace_all(&edits);
        if let Some(find) = &mut self.find {
            find.current = None;
        }
        let text = self.look.i18n.f("find.replaced", &[("count", &edits.len())]);
        self.info(text);
        Task::none()
    }

    // ── Find in files ────────────────────────────────────────────────────

    fn show_panel(&mut self, panel: Panel) -> Task<Message> {
        if self.tree.is_none() {
            return Task::none();
        }
        self.sidebar = true;
        self.panel = panel;
        if panel != Panel::Search {
            return Task::none();
        }
        // Start from the selected text, as other editors do.
        if let Some(selected) = self
            .tabs
            .active()
            .and_then(Tab::buffer)
            .and_then(|b| Some(b.editor.slice(b.selection_range()?)))
            .filter(|s| !s.contains('\n'))
        {
            self.project_search.query = selected;
        }
        Task::batch([operation::focus(SEARCH_ID), operation::select_all(SEARCH_ID)])
    }

    /// Searches the workspace in the background. Open files with unsaved
    /// changes are searched as they are in the editor.
    fn run_search(&mut self) -> Task<Message> {
        let Some(root) = self.tree.as_ref().map(|t| t.root().to_path_buf()) else {
            return Task::none();
        };
        let open: HashMap<PathBuf, String> = self
            .tabs
            .iter()
            .filter_map(|t| t.buffer().filter(|b| b.is_dirty()).map(|b| (t.path.clone(), b.text())))
            .collect();
        let hidden = self.settings.hidden.clone();
        let search = &mut self.project_search;
        search.generation += 1;
        search.found = None;
        search.running = false;
        search.error = None;
        let matcher = match Matcher::new(&search.query, search.options) {
            Ok(m) => m,
            Err(QueryError::Empty) => return Task::none(),
            Err(QueryError::Invalid(e)) => {
                search.error = Some(e);
                return Task::none();
            }
        };
        search.running = true;
        let generation = search.generation;
        let wanted = text::path_filter(&search.include, &search.exclude);
        Task::perform(
            async move {
                let mut files: Vec<PathBuf> = tree::walk_files(&root, &hidden)
                    .filter(|p| {
                        let relative = p.strip_prefix(&root).unwrap_or(p);
                        wanted(&relative.to_string_lossy().replace('\\', "/"))
                    })
                    .take(MAX_FILES)
                    .collect();
                files.sort();
                Arc::new(search::search_files(files, &matcher, &open, MAX_HITS))
            },
            move |found| Message::SearchDone(generation, found),
        )
    }

    /// Opens a result's file with the match selected.
    fn open_hit(&mut self, file: usize, hit: usize) -> Task<Message> {
        let Some(found) = self.project_search.found.clone() else { return Task::none() };
        let Some(file) = found.files.get(file) else { return Task::none() };
        let Some(hit) = file.hits.get(hit) else { return Task::none() };
        let _ = self.open(file.path.clone());
        if self.tabs.active().map(|t| &t.path) != Some(&file.path) {
            return Task::none();
        }
        let task = self.go_to_line(hit.line, hit.column);
        if let Some(buffer) = self.active_buffer_mut() {
            let at = buffer.cursor_offset();
            buffer.select(at..at + hit.len);
        }
        task
    }

    // ── Open file by name, go to line ────────────────────────────────────

    fn open_palette(&mut self, files: bool) -> Task<Message> {
        let mut list = Task::none();
        let mode = if files {
            let Some(tree) = &self.tree else { return Task::none() };
            let root = tree.root().to_path_buf();
            let files = self.files.clone().filter(|f| f.root == root);
            if files.is_none() && self.listing.as_ref() != Some(&root) {
                self.listing = Some(root.clone());
                let hidden = self.settings.hidden.clone();
                list = Task::perform(
                    async move {
                        let (files, truncated) = tree::list_files(&root, &hidden, MAX_FILES);
                        Arc::new(FileList { root, files, truncated })
                    },
                    Message::FilesListed,
                );
            }
            PaletteMode::Files { files, results: Vec::new() }
        } else {
            if self.tabs.active().and_then(Tab::buffer).is_none() {
                return Task::none();
            }
            PaletteMode::Line
        };
        self.palette = Some(Palette { mode, query: String::new(), selected: 0 });
        self.prompt = None;
        self.refresh_palette();
        Task::batch([operation::focus(PALETTE_ID), list])
    }

    /// Ctrl+Shift+P lists every command, found by typing part of its name.
    fn open_commands(&mut self) -> Task<Message> {
        let mode = PaletteMode::Commands { results: Vec::new() };
        self.palette = Some(Palette { mode, query: String::new(), selected: 0 });
        self.prompt = None;
        self.menu = None;
        self.refresh_palette();
        operation::focus(PALETTE_ID)
    }

    /// The folders opened before, found by typing part of their path.
    fn open_recent(&mut self) -> Task<Message> {
        let mode = PaletteMode::Recent { results: Vec::new() };
        self.palette = Some(Palette { mode, query: String::new(), selected: 0 });
        self.prompt = None;
        self.menu = None;
        self.refresh_palette();
        operation::focus(PALETTE_ID)
    }

    /// Ranks the files again after the query changed. With no query, the
    /// other open files are listed, so Ctrl+P, Enter goes back and forth.
    fn refresh_palette(&mut self) {
        let query = self.palette.as_ref().map(|p| p.query.clone()).unwrap_or_default();
        let ranked = |names: &[String], limit: usize| -> Vec<usize> {
            if query.trim().is_empty() {
                (0..names.len().min(limit)).collect()
            } else {
                fuzzy::rank(&query, names, limit)
            }
        };
        match self.palette.as_ref().map(|p| &p.mode) {
            Some(PaletteMode::Commands { .. }) => {
                use crate::commands::Command;
                let names: Vec<String> =
                    Command::ALL.iter().map(|c| self.look.i18n.t(c.label()).to_owned()).collect();
                let found = ranked(&names, Command::ALL.len());
                if let Some(Palette { mode: PaletteMode::Commands { results }, selected, .. }) =
                    &mut self.palette
                {
                    *results = found.into_iter().map(|i| Command::ALL[i]).collect();
                    *selected = 0;
                }
                return;
            }
            Some(PaletteMode::Recent { .. }) => {
                let names: Vec<String> =
                    self.recent.iter().map(|p| p.display().to_string()).collect();
                let found = ranked(&names, PALETTE_ROWS);
                let paths: Vec<PathBuf> =
                    found.into_iter().map(|i| self.recent[i].clone()).collect();
                if let Some(Palette { mode: PaletteMode::Recent { results }, selected, .. }) =
                    &mut self.palette
                {
                    *results = paths;
                    *selected = 0;
                }
                return;
            }
            _ => {}
        }
        let root = self.tree.as_ref().map(|t| t.root().to_path_buf());
        let active = self.tabs.active_index();
        let open: Vec<String> = self
            .tabs
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != active)
            .filter_map(|(_, t)| relative_path(root.as_deref()?, &t.path))
            .collect();
        let Some(palette) = &mut self.palette else { return };
        palette.selected = 0;
        let PaletteMode::Files { files, results } = &mut palette.mode else { return };
        let (name, _) = text::split_location(&palette.query);
        *results = match files {
            _ if name.trim().is_empty() => open.into_iter().take(PALETTE_ROWS).collect(),
            Some(list) => fuzzy::rank(name, &list.files, PALETTE_ROWS)
                .into_iter()
                .map(|i| list.files[i].clone())
                .collect(),
            None => Vec::new(),
        };
    }

    fn submit_palette(&mut self) -> Task<Message> {
        let Some(palette) = &self.palette else { return Task::none() };
        match &palette.mode {
            PaletteMode::Commands { results } => {
                let Some(&command) = results.get(palette.selected) else { return Task::none() };
                self.palette = None;
                return self.run(command);
            }
            PaletteMode::Recent { results } => {
                let Some(folder) = results.get(palette.selected).cloned() else {
                    return Task::none();
                };
                self.palette = None;
                return self.open_root(folder, true);
            }
            PaletteMode::History { path, versions } => {
                let Some(version) = versions.get(palette.selected).cloned() else {
                    return Task::none();
                };
                let path = path.clone();
                self.palette = None;
                return self.restore_version(&path, &version);
            }
            _ => {}
        }
        if let PaletteMode::Rename { path, at } = &palette.mode {
            let (path, at, name) = (path.clone(), *at, palette.query.trim().to_owned());
            self.palette = None;
            if name.is_empty() {
                return Task::none();
            }
            self.sync_lsp();
            if let Some(buffer) =
                self.tabs.position(&path).and_then(|i| self.tabs.get_mut(i)?.buffer())
            {
                self.lsp.rename(&path, buffer.editor.rope(), at, &name);
            }
            return self.focus_editor();
        }
        let (line, column) = match &palette.mode {
            // `42` or `42:7`
            PaletteMode::Line => {
                let query = palette.query.trim();
                let (line, column) =
                    query.split_once(':').map_or((query, None), |(l, c)| (l, Some(c)));
                let Ok(line) = line.trim().parse::<usize>() else { return Task::none() };
                let column = column.and_then(|c| c.trim().parse().ok());
                (line, column)
            }
            // A file, maybe with `:line:column`, or just `:line` for the open file.
            PaletteMode::Files { results, .. } => {
                let (name, at) = text::split_location(&palette.query);
                let file = results.get(palette.selected).cloned();
                match (file, at) {
                    (Some(file), at) => {
                        let Some(root) = self.tree.as_ref().map(|t| t.root().to_path_buf()) else {
                            return Task::none();
                        };
                        self.palette = None;
                        return self.open_at(root.join(file), at);
                    }
                    (None, Some(at)) if name.trim().is_empty() => at,
                    (None, _) => return Task::none(),
                }
            }
            PaletteMode::Rename { .. }
            | PaletteMode::Commands { .. }
            | PaletteMode::Recent { .. }
            | PaletteMode::History { .. } => return Task::none(),
        };
        self.palette = None;
        self.go_to_line(line.saturating_sub(1), column.unwrap_or(1).saturating_sub(1))
    }
}

/// `path` relative to `root`, with `/` separators, if it's inside it.
fn relative_path(root: &Path, path: &Path) -> Option<String> {
    let relative = path.strip_prefix(root).ok()?;
    Some(relative.iter().map(|p| p.to_string_lossy()).collect::<Vec<_>>().join("/"))
}

/// Start of the word that ends at `cursor`.
fn word_start(rope: &ropey::Rope, cursor: usize) -> usize {
    let mut start = cursor;
    let mut chars = rope.chars_at(rope.byte_to_char(cursor));
    while let Some(c) = chars.prev() {
        if !(c.is_alphanumeric() || c == '_') {
            break;
        }
        start -= c.len_utf8();
    }
    start
}

/// The number of a file never saved, from its stand-in path.
pub fn untitled_number(path: &Path) -> Option<u32> {
    if path.parent() != Some(paths::untitled_dir().as_path()) {
        return None;
    }
    path.file_name()?.to_str()?.strip_prefix("untitled-")?.parse().ok()
}

pub fn is_untitled(tab: &Tab) -> bool {
    untitled_number(&tab.path).is_some()
}

async fn pick_files(start: Option<PathBuf>) -> Option<Vec<PathBuf>> {
    let mut dialog = rfd::AsyncFileDialog::new();
    if let Some(dir) = start {
        dialog = dialog.set_directory(dir);
    }
    let files = dialog.pick_files().await?;
    Some(files.into_iter().map(|f| f.path().to_path_buf()).collect())
}

async fn pick_save(dir: Option<PathBuf>, name: String) -> Option<PathBuf> {
    let mut dialog = rfd::AsyncFileDialog::new().set_file_name(name);
    if let Some(dir) = dir {
        dialog = dialog.set_directory(dir);
    }
    dialog.save_file().await.map(|f| f.path().to_path_buf())
}

fn fs_error_text(i18n: &I18n, e: FsError) -> String {
    match e {
        FsError::InvalidName => i18n.t("prompt.invalid").to_owned(),
        FsError::Exists(name) => i18n.f("prompt.exists", &[("name", &name)]),
        FsError::Io(e) => e.to_string(),
    }
}

/// Shows `path` in the system's file manager, selected where the system
/// allows it. These are programs on your machine, started and left alone
/// (see docs/SEALED.md).
fn reveal_in_file_manager(path: &Path) -> std::io::Result<()> {
    use std::process::{Command, Stdio};
    let spawn = |command: &mut Command| {
        command.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().map(drop)
    };
    if cfg!(target_os = "macos") {
        return spawn(Command::new("open").arg("-R").arg(path));
    }
    if cfg!(windows) {
        let mut select = std::ffi::OsString::from("/select,");
        select.push(path);
        return spawn(Command::new("explorer").arg(select));
    }
    // The freedesktop file manager interface selects the file, in the
    // file managers that have it (GNOME Files, Dolphin, Nemo…).
    let shown = Command::new("dbus-send")
        .args([
            "--session",
            "--print-reply",
            "--dest=org.freedesktop.FileManager1",
            "/org/freedesktop/FileManager1",
            "org.freedesktop.FileManager1.ShowItems",
        ])
        .arg(format!("array:string:{}", text::file_uri(path)))
        .arg("string:")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success());
    if shown {
        return Ok(());
    }
    let folder = if path.is_dir() { path } else { path.parent().unwrap_or(path) };
    spawn(Command::new("xdg-open").arg(folder))
}

/// Events from the terminal's thread, the same way as [`lsp_events`].
fn terminal_events() -> impl iced::futures::Stream<Item = Message> {
    use iced::futures::{SinkExt, StreamExt};
    iced::stream::channel(64, async |mut output| {
        let (sender, mut events) = iced::futures::channel::mpsc::unbounded();
        let _ = output.send(Message::TermChannel(sender)).await;
        while let Some((id, event)) = events.next().await {
            let _ = output.send(Message::Term(id, event)).await;
        }
    })
}

/// Events from language servers. The first message hands the app the
/// sender that server threads write to.
fn lsp_events() -> impl iced::futures::Stream<Item = Message> {
    use iced::futures::{SinkExt, StreamExt};
    iced::stream::channel(64, async |mut output| {
        let (sender, mut events) = iced::futures::channel::mpsc::unbounded();
        let _ = output.send(Message::LspChannel(sender)).await;
        while let Some(event) = events.next().await {
            let _ = output.send(Message::Lsp(event)).await;
        }
    })
}

/// The committed text of `path`, read in the background.
fn git_base(path: PathBuf) -> Task<Message> {
    Task::perform(
        {
            let path = path.clone();
            async move { git::head_text(&path) }
        },
        move |base| Message::GitBase(path.clone(), base),
    )
}

/// Waits without blocking the executor's threads.
async fn sleep(duration: Duration) {
    let (done, wait) = iced::futures::channel::oneshot::channel();
    std::thread::spawn(move || {
        std::thread::sleep(duration);
        let _ = done.send(());
    });
    let _ = wait.await;
}

async fn pick_folder(start: Option<PathBuf>) -> Option<PathBuf> {
    let mut dialog = rfd::AsyncFileDialog::new();
    if let Some(dir) = start {
        dialog = dialog.set_directory(dir);
    }
    dialog.pick_folder().await.map(|handle| handle.path().to_path_buf())
}
