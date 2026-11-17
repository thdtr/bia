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
use crate::tabs::{Body, Tab, Tabs};
use crate::terminal::{TermEvent, Terminal};
use crate::ui::{self, style::Colors};
use bia_core::backup::{self, Backup};
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
use bia_core::{I18n, Lang, paths};
use iced::widget::{operation, svg};
use iced::{Event, Font, Subscription, Task, Vector, event, keyboard, window};
use std::collections::HashMap;
use std::fs;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub const EDITOR_ID: &str = "bia-editor";
pub const PROMPT_ID: &str = "bia-prompt";
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

/// Unsaved edits are written to `backups/` this long after the last one.
const BACKUP_DELAY: Duration = Duration::from_secs(2);

pub fn run(paths: Vec<PathBuf>) -> iced::Result {
    iced::application(move || Bia::new(paths.clone()), Bia::update, ui::view)
        .default_font(Font::with_name(fonts::UI_FAMILY))
        .title(Bia::title)
        .theme(Bia::theme)
        .subscription(Bia::subscription)
        .window(window::Settings {
            size: iced::Size::new(1180.0, 760.0),
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
    /// A file's text as of the last commit (`None`: not committed).
    GitBase(PathBuf, Option<String>),
    /// The branch and changed files of the folder that's open.
    GitStatus(PathBuf, Arc<git::Status>),
    /// Where language server threads send their events.
    LspChannel(iced::futures::channel::mpsc::UnboundedSender<bia_core::lsp::Event>),
    Lsp(bia_core::lsp::Event),
    /// Time to send edited files to their language servers.
    LspSync,
    TermChannel(iced::futures::channel::mpsc::UnboundedSender<TermEvent>),
    Term(TermEvent),
    TermResize(crate::terminal::Size),
    ToggleTerminal,
    RowPressed(PathBuf),
    RowContext(PathBuf),
    ToggleShelf,
    Shelf(ShelfAction),
    PromptInput(String),
    PromptSubmit,
    SelectTab(usize),
    CloseTab(usize),
    Input(Input),
    Save,
    TogglePreview,
    LinkClicked(String),
    FindInput(String),
    FindSubmit,
    FindOption(FindOption),
    ShowPanel(Panel),
    SearchInput(String),
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
    EditorScrolled(Vector),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShelfAction {
    NewFile,
    NewFolder,
    Rename,
    Delete,
}

/// A question that blocks everything else until it is answered.
pub enum Dialog {
    /// Quitting with unsaved files: save all, discard, cancel.
    Quit,
    /// Closing a tab with unsaved changes: save, discard, cancel.
    Close(PathBuf),
    /// Saving a file that changed on disk since it was opened: overwrite,
    /// load the disk version, cancel.
    Conflict(PathBuf),
}

/// The answer to a [`Dialog`]. What `Save` and `Discard` mean depends on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Choice {
    Save,
    Discard,
    Cancel,
}

/// The bottom strip of the sidebar: explorer actions and their prompts.
pub enum Shelf {
    Hidden,
    Actions,
    Prompt(Prompt),
}

pub struct Prompt {
    pub kind: PromptKind,
    pub value: String,
    pub error: Option<String>,
}

pub enum PromptKind {
    NewFile(PathBuf),
    NewFolder(PathBuf),
    Rename(PathBuf),
    Delete(PathBuf),
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
}

impl Find {
    fn matcher(&self) -> Result<Matcher, QueryError> {
        Matcher::new(&self.query, self.options)
    }
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
    /// Open a file by name. Paths are relative to the workspace, with `/`.
    Files { files: Vec<String>, truncated: bool, results: Vec<String> },
    /// Go to a line of the active file.
    Line,
    /// A new name for the symbol at `at` in `path` (language server).
    Rename { path: PathBuf, at: usize },
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
    pub selected: Option<PathBuf>,
    pub tabs: Tabs,
    pub shelf: Shelf,
    pub find: Option<Find>,
    pub palette: Option<Palette>,
    pub notice: Option<Notice>,
    pub sidebar: bool,
    pub panel: Panel,
    pub project_search: ProjectSearch,
    /// What git says about the open folder; empty outside a repository.
    pub git: Arc<git::Status>,
    pub lsp: Lsp,
    /// The shell in the bottom panel, once opened. It keeps running while
    /// the panel is hidden.
    pub terminal: Option<Terminal>,
    pub terminal_open: bool,
    term_events: Option<iced::futures::channel::mpsc::UnboundedSender<TermEvent>>,
    pub dialog: Option<Dialog>,
    /// `font_size` as settings.json has it, while zoom changes the live one.
    font_size_setting: f32,
    /// Folders opened before, most recent first, for the welcome screen.
    pub recent: Vec<PathBuf>,
    /// The workspace is a folder you chose (rather than a file's folder),
    /// so its open files are remembered.
    remember_root: bool,
    backup_scheduled: bool,
    /// Numbers requests to center the cursor (see `Buffer::center`).
    center_seq: u64,
}

impl Bia {
    /// `paths` come from the command line. The first folder becomes the
    /// workspace (or the first file's folder), and files open in tabs.
    pub fn new(paths: Vec<PathBuf>) -> (Self, Task<Message>) {
        let (settings, settings_error) = Settings::load_or_create(&paths::settings_file());
        let (look, errors) = Look::load(&settings, 0);
        let state = State::load(&paths::state_file());
        let mut app = Self {
            recent: state.recent_folders(),
            remember_root: false,
            backup_scheduled: false,
            center_seq: 0,
            font_size_setting: settings.font_size,
            settings,
            look,
            tree: None,
            selected: None,
            tabs: Tabs::default(),
            shelf: Shelf::Hidden,
            find: None,
            palette: None,
            notice: None,
            sidebar: true,
            panel: Panel::Files,
            project_search: ProjectSearch::default(),
            git: Arc::default(),
            lsp: Lsp::default(),
            terminal: None,
            terminal_open: false,
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

        let mut tasks = Vec::new();
        // With nothing on the command line, BIA reopens the last folder and
        // the files that were open in it.
        if paths.is_empty()
            && let Some(folder) = state.last_folder()
        {
            tasks.push(app.open_root(folder.to_path_buf(), true));
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
            format!("{dot}{}", t.name())
        });
        let root = self.tree.as_ref().map(FileTree::root_name);
        let parts: Vec<String> = tab.into_iter().chain(root).chain(["BIA".to_owned()]).collect();
        parts.join(" — ")
    }

    pub fn theme(&self) -> iced::Theme {
        self.look.iced_theme.clone()
    }

    pub fn subscription(&self) -> Subscription<Message> {
        let keys = event::listen_with(|event, status, _window| match event {
            Event::Window(window::Event::Focused) => Some(Message::WindowFocused),
            Event::Window(window::Event::CloseRequested) => Some(Message::CloseRequested),
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
        Subscription::batch([
            keys,
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
        let first_line = text.lines().next().unwrap_or_default();
        language::detect(&self.look.syntaxes, path, first_line).name.clone()
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
        Task::batch([task, self.schedule_backup()])
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
                self.write_backups();
                self.save_session();
            }

            Message::RowPressed(path) => {
                self.selected = Some(path.clone());
                if path.is_dir() {
                    if let Some(tree) = &mut self.tree {
                        tree.toggle(&path);
                    }
                } else {
                    return self.open(path);
                }
            }
            Message::RowContext(path) => {
                self.selected = Some(path);
                self.shelf = Shelf::Actions;
            }
            Message::ToggleShelf => {
                self.shelf = match self.shelf {
                    Shelf::Hidden => Shelf::Actions,
                    _ => Shelf::Hidden,
                };
            }
            Message::Shelf(action) => return self.shelf_action(action),
            Message::PromptInput(value) => {
                if let Shelf::Prompt(prompt) = &mut self.shelf {
                    prompt.value = value;
                    prompt.error = None;
                }
            }
            Message::PromptSubmit => return self.submit_prompt(),

            Message::SelectTab(index) => {
                if self.tabs.select(index) {
                    self.reveal_active();
                    return operation::focus(EDITOR_ID);
                }
            }
            Message::CloseTab(index) => return self.close_tab(index),

            Message::Input(input) => {
                // The text stays as it was while a dialog asks about it.
                if self.dialog.is_some() {
                    return Task::none();
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
                    self.refresh_find();
                    self.lsp.stale.insert(path.clone());
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
                self.sync_lsp();
            }
            Message::TermChannel(sender) => self.term_events = Some(sender),
            Message::Term(event) => match event {
                // New output: the view redraws after any message.
                TermEvent::Wakeup => {}
                TermEvent::Title(title) => {
                    if let Some(t) = &mut self.terminal {
                        t.title = title;
                    }
                }
                TermEvent::Reply(text) => {
                    if let Some(t) = &self.terminal {
                        t.write(text.into_bytes());
                    }
                }
                TermEvent::Copy(text) => return iced::clipboard::write(text),
                TermEvent::Exit => {
                    self.terminal = None;
                    self.terminal_open = false;
                    return operation::focus(EDITOR_ID);
                }
            },
            Message::TermResize(size) => {
                if let Some(t) = &mut self.terminal {
                    t.resize(size);
                }
            }
            Message::ToggleTerminal => return self.toggle_terminal(),
            Message::EditorScrolled(offset) => {
                if let Some(tab) = self.tabs.active_mut() {
                    tab.scroll = offset;
                }
            }
            Message::Save => {
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
                    buffer.git.borrow_mut().set_base(base);
                }
            }
            Message::GitStatus(root, status) => {
                if self.tree.as_ref().is_some_and(|t| t.root() == root) {
                    self.git = status;
                }
            }
            Message::TogglePreview => {
                let is_md = self.tabs.active().is_some_and(|t| self.is_markdown(t));
                if let Some(buffer) = self.active_buffer_mut().filter(|_| is_md) {
                    let on = buffer.markdown.is_none();
                    buffer.set_preview(on);
                    // The editor moves into a new layout and starts unfocused.
                    return operation::focus(EDITOR_ID);
                }
            }
            Message::LinkClicked(url) => {
                // BIA never opens URLs itself (see docs/SEALED.md). The link is
                // copied so you can open it wherever you choose.
                self.info(url.clone());
                return iced::clipboard::write(url);
            }

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
                });
                self.refresh_find();
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
                }
                self.refresh_find();
                return operation::focus(FIND_ID);
            }
            // Enter in the find field goes to the next match and stays there,
            // so pressing it again goes on; Escape gets back to the text.
            Message::FindSubmit => return self.find_step(true, false),
            Message::ShowPanel(panel) => return self.show_panel(panel),
            Message::SearchInput(query) => self.project_search.query = query,
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
                return operation::focus(EDITOR_ID);
            }

            Message::Shortcut(shortcut) => return self.shortcut(shortcut),
            Message::WindowFocused => {
                self.on_focus();
                // Commits and checkouts happen in other windows.
                return self.refresh_git();
            }
            Message::CloseRequested => {
                if self.tabs.any_dirty() {
                    self.dialog = Some(Dialog::Quit);
                } else {
                    return self.exit();
                }
            }
            Message::Dialog(choice) => return self.answer(choice),
            Message::OpenSettings => return self.open(paths::settings_file()),
        }
        Task::none()
    }

    fn shortcut(&mut self, shortcut: Shortcut) -> Task<Message> {
        if self.dialog.is_some() && shortcut != Shortcut::Escape {
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
                self.refresh_find();
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
                        PaletteMode::Line | PaletteMode::Rename { .. } => 0,
                    };
                    palette.selected = match shortcut {
                        Shortcut::Up => palette.selected.saturating_sub(1),
                        _ => (palette.selected + 1).min(count.saturating_sub(1)),
                    };
                }
            }
            Shortcut::Preview => return self.update(Message::TogglePreview),
            Shortcut::Sidebar => self.sidebar = !self.sidebar,
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
            Shortcut::NewFile => return self.shelf_action(ShelfAction::NewFile),
            Shortcut::NewFolder => return self.shelf_action(ShelfAction::NewFolder),
            Shortcut::Rename => return self.shelf_action(ShelfAction::Rename),
            Shortcut::Delete => return self.shelf_action(ShelfAction::Delete),
            Shortcut::Confirm => {
                if matches!(&self.shelf, Shelf::Prompt(Prompt { kind: PromptKind::Delete(_), .. }))
                {
                    return self.submit_prompt();
                }
            }
            Shortcut::Escape => {
                if self.dialog.is_some() {
                    self.dialog = None;
                } else if self.palette.is_some() {
                    return self.update(Message::PaletteClose);
                } else if !matches!(self.shelf, Shelf::Hidden) {
                    self.shelf = Shelf::Hidden;
                } else if self.find.is_some() {
                    self.find = None;
                    return operation::focus(EDITOR_ID);
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
        self.selected = None;
        self.shelf = Shelf::Hidden;
        self.project_search = ProjectSearch::default();
        self.git = Arc::default();
        if !remember || !self.tabs.is_empty() {
            return self.refresh_git();
        }
        let restore = self.restore_session(&root, session.unwrap_or_default());
        Task::batch([restore, self.refresh_git()])
    }

    // ── Terminal ─────────────────────────────────────────────────────────

    /// Ctrl+`: shows the terminal (starting the shell the first time) or
    /// hides it, moving the focus there and back.
    fn toggle_terminal(&mut self) -> Task<Message> {
        if self.terminal_open {
            self.terminal_open = false;
            return operation::focus(EDITOR_ID);
        }
        if self.terminal.is_none() {
            let Some(events) = self.term_events.clone() else { return Task::none() };
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
            let size = crate::terminal::Size { columns: 80, lines: 12 };
            match Terminal::spawn(&dir, size, events) {
                Ok(terminal) => self.terminal = Some(terminal),
                Err(e) => {
                    let text = self.look.i18n.f("notice.terminal_failed", &[("error", &e)]);
                    self.error(text);
                    return Task::none();
                }
            }
        }
        self.terminal_open = true;
        operation::focus(TERMINAL_ID)
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

    /// Sends edits a moment after typing stops, rather than on every key.
    fn schedule_lsp_sync(&mut self) -> Task<Message> {
        if self.lsp.sync_scheduled || self.lsp.stale.is_empty() {
            return Task::none();
        }
        self.lsp.sync_scheduled = true;
        Task::perform(sleep(Duration::from_millis(150)), |()| Message::LspSync)
    }

    /// Keeps the completion list in step with the cursor, and opens it
    /// while typing a word or after a trigger character (`.`).
    fn follow_completion(&mut self, path: &Path, typed: Option<char>) {
        let start_list = typed.is_some_and(|c| {
            self.lsp.completion.is_none()
                && self.lsp.supports(path, "completionProvider")
                && (c.is_alphanumeric()
                    || c == '_'
                    || self.lsp.triggers(path).iter().any(|t| t.ends_with(c)))
        });
        if start_list {
            self.sync_lsp();
        }
        let Some(buffer) = self.tabs.active().and_then(Tab::buffer) else { return };
        let rope = buffer.editor.rope();
        let cursor = buffer.cursor_offset();
        if self.lsp.completion.is_some() {
            self.lsp.refilter(path, rope, cursor);
        } else if start_list {
            self.lsp.complete(path, rope, word_start(rope, cursor), cursor);
        }
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
                            self.lsp.stale.insert(path.to_owned());
                            self.refresh_find();
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
                    self.lsp.stale.insert(path);
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
            }
        }
        self.refresh_find();
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
        self.reveal_active();
        operation::focus(EDITOR_ID)
    }

    /// Writes the workspace's open files to `state.json`.
    fn save_session(&self) {
        let Some(root) = self.tree.as_ref().map(FileTree::root).filter(|_| self.remember_root)
        else {
            return;
        };
        // Tabs from other folders (settings.json, say) aren't part of it.
        let mine: Vec<&Tab> = self.tabs.iter().filter(|t| t.path.starts_with(root)).collect();
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
    fn write_backups(&mut self) {
        let dir = paths::backups_dir();
        for tab in self.tabs.iter_mut() {
            let path = tab.path.clone();
            let Some(buffer) = tab.buffer() else { continue };
            if buffer.is_dirty() {
                if tab.backup != Some(buffer.revision()) {
                    let b = Backup::new(path, buffer.text(), buffer.modified);
                    // On failure, the next edit tries again.
                    if backup::write(&dir, &b).is_ok() {
                        tab.backup = Some(buffer.revision());
                    }
                }
            } else if tab.backup.take().is_some() {
                backup::remove(&dir, &path);
            }
        }
    }

    /// Quitting: every tab is saved or its edits were discarded, so no
    /// backup is needed any more.
    fn exit(&mut self) -> Task<Message> {
        self.save_session();
        // Servers read EOF on their stdin and stop.
        self.lsp.restart();
        let dir = paths::backups_dir();
        for tab in self.tabs.iter_mut() {
            if tab.backup.take().is_some() {
                backup::remove(&dir, &tab.path);
            }
        }
        iced::exit()
    }

    fn open(&mut self, path: PathBuf) -> Task<Message> {
        if let Some(index) = self.tabs.position(&path) {
            return self.update(Message::SelectTab(index));
        }
        let i18n = &self.look.i18n;
        let body = match Document::load(&path) {
            Ok(doc) => {
                let syntax = self.detect_syntax(&path, &doc.text);
                let indent = Indent::for_file(&path, &doc.text, self.settings.tab_size);
                let mut buffer = Buffer::new(doc, syntax, indent);
                buffer.editor.auto_close = self.settings.auto_close;
                buffer.editor.tab_width = self.settings.tab_size;
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
        self.reveal_active();
        self.sync_lsp();
        Task::batch([operation::focus(EDITOR_ID)].into_iter().chain(base))
    }

    /// Puts back unsaved edits to `path` left in `backups/`.
    fn recover(&mut self, path: &Path, buffer: &mut Buffer) {
        let dir = paths::backups_dir();
        let Some(b) = backup::read(&dir, path) else { return };
        if b.text == buffer.text() {
            backup::remove(&dir, path);
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
    fn reveal_active(&mut self) {
        if let (Some(tab), Some(tree)) = (self.tabs.active(), &mut self.tree)
            && tab.path.starts_with(tree.root())
        {
            tree.reveal(&tab.path);
            self.selected = Some(tab.path.clone());
        }
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
        if let Some(tab) = self.tabs.remove(index)
            && tab.backup.is_some()
        {
            backup::remove(&paths::backups_dir(), &tab.path);
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
            (Dialog::Quit, Choice::Save) => {
                let all_saved = self.tabs.dirty_indices().into_iter().all(|i| self.save(i, false));
                if all_saved {
                    return self.exit();
                }
            }
            (Dialog::Quit, Choice::Discard) => return self.exit(),
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

        if buffer.is_dirty() {
            let on_disk = document::modified(&path);
            if !overwrite && on_disk.is_some() && on_disk != buffer.modified {
                self.dialog = Some(Dialog::Conflict(path));
                return false;
            }
            match document::write_atomic(&path, &buffer.encoded()) {
                Ok(modified) => buffer.mark_saved(modified),
                Err(e) => {
                    let text =
                        self.look.i18n.f("notice.save_failed", &[("name", &name), ("error", &e)]);
                    self.error(text);
                    return false;
                }
            }
        }
        let text = self.look.i18n.f("notice.saved", &[("name", &name)]);
        self.info(text);
        self.after_save(&path);
        if let Some(tree) = &mut self.tree {
            tree.refresh();
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
            Ok(settings) => self.apply_settings(settings),
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
        }
        // Grammars may have been added or removed, and `tab_size` may have
        // changed: detect every file again.
        let detected: Vec<(usize, String, Indent)> = self
            .tabs
            .iter()
            .enumerate()
            .filter_map(|(i, t)| {
                let text = t.buffer()?.text();
                let indent = Indent::for_file(&t.path, &text, settings.tab_size);
                Some((i, self.detect_syntax(&t.path, &text), indent))
            })
            .collect();
        for (i, syntax, indent) in detected {
            if let Some(buffer) = self.tabs.get_mut(i).and_then(Tab::buffer_mut) {
                buffer.editor.syntax = syntax;
                buffer.editor.indent = indent;
                buffer.editor.auto_close = settings.auto_close;
                buffer.editor.tab_width = settings.tab_size;
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
        self.center_seq += 1;
        let seq = self.center_seq;
        let Some(buffer) = self.active_buffer_mut() else { return Task::none() };
        buffer.go_to(line, column);
        buffer.center = seq;
        operation::focus(EDITOR_ID)
    }

    // ── Explorer actions ─────────────────────────────────────────────────

    /// Folder that "new file" / "new folder" act on.
    fn target_dir(&self) -> Option<PathBuf> {
        let root = self.tree.as_ref()?.root().to_path_buf();
        Some(match &self.selected {
            Some(p) if p.is_dir() => p.clone(),
            Some(p) => p.parent().map_or(root.clone(), Path::to_path_buf),
            None => root,
        })
    }

    fn shelf_action(&mut self, action: ShelfAction) -> Task<Message> {
        let Some(root) = self.tree.as_ref().map(|t| t.root().to_path_buf()) else {
            return Task::none();
        };
        let selected = self.selected.clone().filter(|p| p != &root && p.exists());
        let (kind, value) = match action {
            ShelfAction::NewFile => {
                (PromptKind::NewFile(self.target_dir().unwrap_or(root)), String::new())
            }
            ShelfAction::NewFolder => {
                (PromptKind::NewFolder(self.target_dir().unwrap_or(root)), String::new())
            }
            ShelfAction::Rename => match selected {
                Some(p) => {
                    let name = tree::display_name(&p);
                    (PromptKind::Rename(p), name)
                }
                None => return Task::none(),
            },
            ShelfAction::Delete => match selected {
                Some(p) => (PromptKind::Delete(p), String::new()),
                None => return Task::none(),
            },
        };
        let needs_input = !matches!(kind, PromptKind::Delete(_));
        self.shelf = Shelf::Prompt(Prompt { kind, value, error: None });
        if needs_input {
            Task::batch([operation::focus(PROMPT_ID), operation::select_all(PROMPT_ID)])
        } else {
            Task::none()
        }
    }

    fn submit_prompt(&mut self) -> Task<Message> {
        let Shelf::Prompt(prompt) = &mut self.shelf else { return Task::none() };
        let i18n = &self.look.i18n;
        let fs_error = |e: FsError| match e {
            FsError::InvalidName => i18n.t("prompt.invalid").to_owned(),
            FsError::Exists(name) => i18n.f("prompt.exists", &[("name", &name)]),
            FsError::Io(e) => e.to_string(),
        };
        let result: Result<Option<PathBuf>, String> = match &prompt.kind {
            PromptKind::NewFile(dir) => fs_ops::create_file(dir, &prompt.value).map(Some),
            PromptKind::NewFolder(dir) => fs_ops::create_dir(dir, &prompt.value).map(Some),
            PromptKind::Rename(from) => fs_ops::rename(from, &prompt.value),
            PromptKind::Delete(path) => {
                // Trashing a file would throw away edits that only live in a tab.
                if let Some(tab) = self.tabs.dirty_under(path) {
                    prompt.error = Some(i18n.f("prompt.unsaved", &[("name", &tab.name())]));
                    return Task::none();
                }
                trash::delete(path)
                    .map(|()| None)
                    .map_err(|e| FsError::Io(std::io::Error::other(e.to_string())))
            }
        }
        .map_err(fs_error);

        let path = match result {
            Ok(path) => path,
            Err(error) => {
                prompt.error = Some(error);
                return Task::none();
            }
        };

        let Shelf::Prompt(prompt) = std::mem::replace(&mut self.shelf, Shelf::Hidden) else {
            unreachable!("checked at the top")
        };
        match (prompt.kind, path) {
            (PromptKind::NewFile(_), Some(path)) => {
                self.reveal_in_tree(&path);
                return self.open(path);
            }
            (PromptKind::NewFolder(_), Some(path)) => {
                self.reveal_in_tree(&path);
                if let Some(tree) = &mut self.tree {
                    tree.toggle(&path);
                }
            }
            (PromptKind::Rename(from), Some(to)) => {
                if let Some(tree) = &mut self.tree {
                    tree.renamed(&from, &to);
                }
                // A new extension may mean a new language.
                for i in self.tabs.renamed(&from, &to) {
                    let Some(tab) = self.tabs.get_mut(i) else { continue };
                    // The backup is filed under the old path; the next tick
                    // writes it under the new one.
                    if tab.backup.take().is_some()
                        && let Ok(rest) = tab.path.strip_prefix(&to)
                    {
                        backup::remove(&paths::backups_dir(), &from.join(rest));
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
                self.selected = Some(to);
            }
            (PromptKind::Delete(deleted), _) => {
                self.tabs.remove_under(&deleted);
                self.selected = None;
                if let Some(tree) = &mut self.tree {
                    tree.refresh();
                }
            }
            // Renaming to the same name changes nothing.
            _ => {}
        }
        Task::none()
    }

    fn reveal_in_tree(&mut self, path: &Path) {
        if let Some(tree) = &mut self.tree {
            tree.reveal(path);
            tree.refresh();
        }
        self.selected = Some(path.to_path_buf());
    }

    // ── Find ─────────────────────────────────────────────────────────────

    /// Recounts matches after the text or the query changed, so "2 of 5"
    /// stays true.
    fn refresh_find(&mut self) {
        let Some(find) = &mut self.find else { return };
        let Some(buffer) = self.tabs.active().and_then(Tab::buffer) else { return };
        let (matches, error) = match find.matcher() {
            Ok(m) => (m.find_all(&buffer.text()), None),
            Err(QueryError::Empty) => (Vec::new(), None),
            Err(QueryError::Invalid(e)) => (Vec::new(), Some(e)),
        };
        find.matches = matches;
        find.error = error;
        find.current = find.current.filter(|&i| i < find.matches.len());
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
            (Some(_), true) => operation::focus(EDITOR_ID),
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
            self.refresh_find();
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
        self.refresh_find();
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
        Task::perform(
            async move {
                let mut files: Vec<PathBuf> =
                    tree::walk_files(&root, &hidden).take(MAX_FILES).collect();
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
        let mode = if files {
            let Some(tree) = &self.tree else { return Task::none() };
            let (files, truncated) =
                tree::list_files(tree.root(), &self.settings.hidden, MAX_FILES);
            PaletteMode::Files { files, truncated, results: Vec::new() }
        } else {
            if self.tabs.active().and_then(Tab::buffer).is_none() {
                return Task::none();
            }
            PaletteMode::Line
        };
        self.palette = Some(Palette { mode, query: String::new(), selected: 0 });
        self.shelf = Shelf::Hidden;
        self.refresh_palette();
        operation::focus(PALETTE_ID)
    }

    /// Ranks the files again after the query changed. With no query, the
    /// other open files are listed, so Ctrl+P, Enter goes back and forth.
    fn refresh_palette(&mut self) {
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
        let PaletteMode::Files { files, results, .. } = &mut palette.mode else { return };
        let (name, _) = text::split_location(&palette.query);
        *results = if name.trim().is_empty() {
            open.into_iter().take(PALETTE_ROWS).collect()
        } else {
            fuzzy::rank(name, files, PALETTE_ROWS).into_iter().map(|i| files[i].clone()).collect()
        };
    }

    fn submit_palette(&mut self) -> Task<Message> {
        let Some(palette) = &self.palette else { return Task::none() };
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
            return operation::focus(EDITOR_ID);
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
            PaletteMode::Rename { .. } => return Task::none(),
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

/// Events from the terminal's thread, the same way as [`lsp_events`].
fn terminal_events() -> impl iced::futures::Stream<Item = Message> {
    use iced::futures::{SinkExt, StreamExt};
    iced::stream::channel(64, async |mut output| {
        let (sender, mut events) = iced::futures::channel::mpsc::unbounded();
        let _ = output.send(Message::TermChannel(sender)).await;
        while let Some(event) = events.next().await {
            let _ = output.send(Message::Term(event)).await;
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
