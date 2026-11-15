//! Application state and the update loop.
//!
//! BIA follows iced's Elm architecture: [`Bia`] is the whole state,
//! [`Message`] every event, [`Bia::update`] the only place state changes and
//! `ui::view` a pure function of the state.
//!
//! This module only orchestrates. The pieces it coordinates live elsewhere:
//! open files in `tabs.rs`, one file's text in `buffer.rs`, file operations
//! in `bia_core::fs_ops`, editor geometry in `ui::layout`.

use crate::buffer::Buffer;
use crate::fonts;
use crate::highlighter;
use crate::keys::{self, Shortcut};
use crate::tabs::{Body, Tab, Tabs};
use crate::ui::{self, layout::Geometry, layout::LARGE_FILE_LINES, style::Colors};
use bia_core::document::{self, Document, LoadError};
use bia_core::fs_ops::{self, FsError};
use bia_core::language::{
    self,
    syntect::{highlighting::Theme as SyntaxTheme, parsing::SyntaxSet},
};
use bia_core::settings::{Settings, SettingsError};
use bia_core::text;
use bia_core::theme::Theme as BiaTheme;
use bia_core::tree::{self, FileTree};
use bia_core::{I18n, Lang, paths};
use iced::widget::scrollable::{AbsoluteOffset, Viewport};
use iced::widget::text_editor::{Action, Motion};
use iced::widget::{operation, svg, text_editor};
use iced::{Event, Font, Subscription, Task, event, keyboard, window};
use std::fs;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub const EDITOR_ID: &str = "bia-editor";
pub const PROMPT_ID: &str = "bia-prompt";
pub const FIND_ID: &str = "bia-find";
pub const EDITOR_SCROLL_ID: &str = "bia-editor-scroll";

/// Notices fade on the next interaction after this long.
const NOTICE_TTL: Duration = Duration::from_secs(4);

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
    RowPressed(PathBuf),
    RowContext(PathBuf),
    ToggleShelf,
    Shelf(ShelfAction),
    PromptInput(String),
    PromptSubmit,
    SelectTab(usize),
    CloseTab(usize),
    Edit(text_editor::Action),
    Save,
    TogglePreview,
    LinkClicked(String),
    FindInput(String),
    FindSubmit,
    Shortcut(Shortcut),
    WindowFocused,
    CloseRequested,
    Dialog(Choice),
    OpenSettings,
    EditorScrolled(Viewport),
    Page { down: bool, select: bool },
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
    pub matches: Vec<Range<usize>>,
    pub current: Option<usize>,
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
    pub geometry: Geometry,
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
            geometry: Geometry::new(
                settings.font_size,
                settings.line_height,
                fonts::char_width(editor_font, settings.font_size, settings.line_height),
            ),
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
    pub notice: Option<Notice>,
    pub sidebar: bool,
    pub dialog: Option<Dialog>,
    editor_viewport: Option<Viewport>,
}

impl Bia {
    /// `paths` come from the command line. The first folder becomes the
    /// workspace (or the first file's folder), and files open in tabs.
    pub fn new(paths: Vec<PathBuf>) -> (Self, Task<Message>) {
        let (settings, settings_error) = Settings::load_or_create(&paths::settings_file());
        let (look, errors) = Look::load(&settings, 0);
        let mut app = Self {
            settings,
            look,
            tree: None,
            selected: None,
            tabs: Tabs::default(),
            shelf: Shelf::Hidden,
            find: None,
            notice: None,
            sidebar: true,
            dialog: None,
            editor_viewport: None,
        };
        if let Some(e) = settings_error {
            let text = app.settings_error_text(&e);
            app.error(text);
        }
        if let Some(e) = errors.into_iter().next() {
            app.error(e);
        }

        let mut tasks = Vec::new();
        for path in paths {
            let path =
                fs::canonicalize(&path).or_else(|_| std::path::absolute(&path)).unwrap_or(path);
            if path.is_dir() {
                if app.tree.is_none() {
                    app.open_root(path);
                }
                continue;
            }
            if app.tree.is_none()
                && let Some(parent) = path.parent().filter(|p| p.is_dir())
            {
                app.open_root(parent.to_path_buf());
            }
            if path.is_file() {
                tasks.push(app.open(path));
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
        event::listen_with(|event, status, _window| match event {
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
        })
    }

    // ── Queries used by the view ─────────────────────────────────────────

    pub fn highlighter_settings(&self, buffer: &Buffer) -> highlighter::Settings {
        highlighter::Settings {
            syntax: buffer.syntax.clone(),
            syntaxes: Arc::clone(&self.look.syntaxes),
            theme: Arc::clone(&self.look.syntax_theme),
            font: self.look.editor_font,
            generation: self.look.generation,
        }
    }

    pub fn is_markdown(&self, tab: &Tab) -> bool {
        tab.buffer().is_some() && language::is_markdown(&tab.path)
    }

    pub fn is_large(&self, buffer: &Buffer) -> bool {
        buffer.line_count > LARGE_FILE_LINES
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
        if !matches!(message, Message::WindowFocused) {
            self.expire_notice();
        }
        match message {
            Message::OpenFolder => {
                let start = self.tree.as_ref().map(|t| t.root().to_path_buf());
                return Task::perform(pick_folder(start), Message::FolderPicked);
            }
            Message::FolderPicked(Some(path)) => self.open_root(path),
            Message::FolderPicked(None) => {}

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
                    return Task::batch([operation::focus(EDITOR_ID), self.restore_scroll()]);
                }
            }
            Message::CloseTab(index) => return self.close_tab(index),

            Message::Edit(action) => {
                // The text stays as it was while a dialog asks about it.
                if self.dialog.is_some() && action.is_edit() {
                    return Task::none();
                }
                // Clicks and drags happen where the user is looking already.
                let follow = !matches!(action, Action::Click(_) | Action::Drag(_));
                let edits = action.is_edit();
                if let Some(buffer) = self.active_buffer_mut() {
                    buffer.perform(action);
                }
                if edits {
                    self.refresh_find();
                }
                if follow {
                    return self.follow_cursor();
                }
            }
            Message::EditorScrolled(viewport) => {
                if let Some(tab) = self.tabs.active_mut() {
                    tab.scroll = viewport.absolute_offset();
                }
                self.editor_viewport = Some(viewport);
            }
            Message::Page { down, select } => {
                let rows = self
                    .editor_viewport
                    .map_or(20, |v| (v.bounds().height / self.look.geometry.line) as usize)
                    .saturating_sub(2)
                    .max(1);
                let motion = if down { Motion::Down } else { Motion::Up };
                let action = if select { Action::Select(motion) } else { Action::Move(motion) };
                if let Some(buffer) = self.active_buffer_mut() {
                    for _ in 0..rows {
                        buffer.perform(action.clone());
                    }
                }
                return self.follow_cursor();
            }
            Message::Save => {
                self.save(self.tabs.active_index(), false);
            }
            Message::TogglePreview => {
                let is_md = self.tabs.active().is_some_and(|t| self.is_markdown(t));
                if let Some(buffer) = self.active_buffer_mut().filter(|_| is_md) {
                    let on = buffer.markdown.is_none();
                    buffer.set_preview(on);
                }
            }
            Message::LinkClicked(url) => {
                // BIA never opens URLs itself (see docs/SEALED.md). The link is
                // copied so you can open it wherever you choose.
                self.info(url.clone());
                return iced::clipboard::write(url);
            }

            Message::FindInput(query) => {
                self.find = Some(Find { query, matches: Vec::new(), current: None });
                self.refresh_find();
            }
            Message::FindSubmit => return self.find_step(true),

            Message::Shortcut(shortcut) => return self.shortcut(shortcut),
            Message::WindowFocused => self.on_focus(),
            Message::CloseRequested => {
                if self.tabs.any_dirty() {
                    self.dialog = Some(Dialog::Quit);
                } else {
                    return iced::exit();
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
                return self.follow_cursor();
            }
            Shortcut::Find => {
                if self.tabs.active().and_then(Tab::buffer).is_some() {
                    let query = self.find.as_ref().map(|f| f.query.clone()).unwrap_or_default();
                    let _ = self.update(Message::FindInput(query));
                    return Task::batch([
                        operation::focus(FIND_ID),
                        operation::select_all(FIND_ID),
                    ]);
                }
            }
            Shortcut::FindNext => return self.find_step(true),
            Shortcut::FindPrevious => return self.find_step(false),
            Shortcut::Preview => return self.update(Message::TogglePreview),
            Shortcut::Sidebar => self.sidebar = !self.sidebar,
            Shortcut::NextTab | Shortcut::PreviousTab => {
                if let Some(i) = self.tabs.neighbor(shortcut == Shortcut::NextTab) {
                    return self.update(Message::SelectTab(i));
                }
            }
            Shortcut::Settings => return self.update(Message::OpenSettings),
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

    // ── Files and tabs ───────────────────────────────────────────────────

    fn open_root(&mut self, root: PathBuf) {
        self.tree = Some(FileTree::new(root, self.settings.hidden.clone()));
        self.selected = None;
        self.shelf = Shelf::Hidden;
    }

    fn open(&mut self, path: PathBuf) -> Task<Message> {
        if let Some(index) = self.tabs.position(&path) {
            return self.update(Message::SelectTab(index));
        }
        let i18n = &self.look.i18n;
        let body = match Document::load(&path) {
            Ok(doc) => {
                let syntax = self.detect_syntax(&path, &doc.text);
                Body::Text(Buffer::new(doc, syntax))
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
        self.tabs.insert(Tab::new(path, body));
        self.reveal_active();
        Task::batch([operation::focus(EDITOR_ID), self.restore_scroll()])
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
        self.tabs.remove(index);
        if self.tabs.is_empty() {
            self.find = None;
        }
        self.restore_scroll()
    }

    fn answer(&mut self, choice: Choice) -> Task<Message> {
        let Some(dialog) = self.dialog.take() else { return Task::none() };
        match (dialog, choice) {
            (_, Choice::Cancel) => {}
            (Dialog::Quit, Choice::Save) => {
                let all_saved = self.tabs.dirty_indices().into_iter().all(|i| self.save(i, false));
                if all_saved {
                    return iced::exit();
                }
            }
            (Dialog::Quit, Choice::Discard) => return iced::exit(),
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
        let (look, errors) = Look::load(&settings, self.look.generation + 1);
        self.look = look;
        if let Some(tree) = &mut self.tree {
            tree.set_hidden(settings.hidden.clone());
        }
        // Grammars may have been added or removed: detect every file again.
        let syntaxes: Vec<(usize, String)> = self
            .tabs
            .iter()
            .enumerate()
            .filter_map(|(i, t)| Some((i, self.detect_syntax(&t.path, t.buffer()?.text()))))
            .collect();
        for (i, syntax) in syntaxes {
            if let Some(buffer) = self.tabs.get_mut(i).and_then(Tab::buffer_mut) {
                buffer.syntax = syntax;
            }
        }
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
    //
    // The editor sits in one scroll area with the line numbers, so both move
    // as one. That scroll area knows nothing about the cursor, hence this.

    fn follow_cursor(&self) -> Task<Message> {
        let (Some(viewport), Some(buffer)) =
            (self.editor_viewport, self.tabs.active().and_then(Tab::buffer))
        else {
            return Task::none();
        };
        if self.is_large(buffer) {
            return Task::none(); // the editor scrolls itself
        }
        let offset = viewport.absolute_offset();
        let bounds = viewport.bounds();
        let (x, y) = self.look.geometry.reveal(
            (buffer.cursor_pos().line, buffer.cursor_columns()),
            buffer.line_count,
            (offset.x, offset.y),
            (bounds.width, bounds.height),
        );
        if x.is_none() && y.is_none() {
            return Task::none();
        }
        operation::scroll_to(EDITOR_SCROLL_ID, AbsoluteOffset { x, y })
    }

    /// Every tab shares one scroll area, so switching tabs restores the
    /// position that tab was left at.
    fn restore_scroll(&self) -> Task<Message> {
        let offset = self.tabs.active().map(|t| t.scroll).unwrap_or_default();
        operation::scroll_to(
            EDITOR_SCROLL_ID,
            AbsoluteOffset { x: Some(offset.x), y: Some(offset.y) },
        )
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
                    let (path, text) = match tab.buffer() {
                        Some(b) => (tab.path.clone(), b.text().to_owned()),
                        None => continue,
                    };
                    let syntax = self.detect_syntax(&path, &text);
                    if let Some(buffer) = self.tabs.get_mut(i).and_then(Tab::buffer_mut) {
                        buffer.syntax = syntax;
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

    /// Recounts matches after the text changed, so "2 of 5" stays true.
    fn refresh_find(&mut self) {
        let Some(find) = &mut self.find else { return };
        let Some(buffer) = self.tabs.active().and_then(Tab::buffer) else { return };
        find.matches = text::find_all(buffer.text(), &find.query);
        find.current = find.current.filter(|&i| i < find.matches.len());
    }

    fn find_step(&mut self, forward: bool) -> Task<Message> {
        let Some(query) = self.find.as_ref().map(|f| f.query.clone()) else { return Task::none() };
        let previous = self.find.as_ref().and_then(|f| f.current);
        let Some(buffer) = self.active_buffer_mut() else { return Task::none() };
        let matches = text::find_all(buffer.text(), &query);
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
        self.find = Some(Find { query, matches, current });
        if current.is_some() {
            Task::batch([operation::focus(EDITOR_ID), self.follow_cursor()])
        } else {
            Task::none()
        }
    }
}

async fn pick_folder(start: Option<PathBuf>) -> Option<PathBuf> {
    let mut dialog = rfd::AsyncFileDialog::new();
    if let Some(dir) = start {
        dialog = dialog.set_directory(dir);
    }
    dialog.pick_folder().await.map(|handle| handle.path().to_path_buf())
}
