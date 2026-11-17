//! The language servers of the open folder and what they said.
//!
//! One server per language, started the first time a file in that language
//! is open and the user named a program for it in `language_servers`.
//! Documents are kept in sync by sending their whole text a moment after
//! edits stop ([`Lsp::sync`]); replies come back as [`Event`]s through the
//! app's subscription and are turned into [`Effect`]s for the app to apply.

use crate::tabs::{Tab, Tabs};
use bia_core::fuzzy;
use bia_core::lsp::types::{self, Completion, Diagnostic, Severity};
use bia_core::lsp::{Client, Encoding, Event, Range, ServerId, text};
use iced::futures::channel::mpsc::UnboundedSender;
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

struct Server {
    client: Client,
    language: String,
    ready: bool,
    encoding: Encoding,
    capabilities: Value,
    /// Documents open on the server, with the version last sent.
    open: HashMap<PathBuf, i32>,
}

enum Pending {
    Definition,
    Hover { path: PathBuf, at: usize },
    Completion { path: PathBuf, start: usize },
    Format { path: PathBuf, revision: u64 },
    Rename,
}

pub struct CompletionState {
    pub path: PathBuf,
    /// Where the word being completed starts.
    pub start: usize,
    items: Vec<Completion>,
    /// Indices into `items` that match what's typed, best first.
    pub shown: Vec<usize>,
    pub selected: usize,
    encoding: Encoding,
}

impl CompletionState {
    pub fn labels(&self) -> impl Iterator<Item = (&str, Option<&str>)> {
        self.shown.iter().map(|&i| (self.items[i].label.as_str(), self.items[i].detail.as_deref()))
    }

    /// The item picked, and the range it replaces given the cursor.
    pub fn chosen(
        &self,
        rope: &ropey::Rope,
        cursor: usize,
    ) -> Option<(std::ops::Range<usize>, String)> {
        let item = &self.items[*self.shown.get(self.selected)?];
        let start = match item.edit_range {
            Some(r) => text::offset(rope, r.start, self.encoding).min(cursor),
            None => self.start,
        };
        Some((start..cursor, item.insert.clone()))
    }
}

/// What the app should do with a reply.
pub enum Effect {
    /// Open `path` with the cursor at `range`.
    Open {
        path: PathBuf,
        range: Range,
        encoding: Encoding,
    },
    /// Replace ranges of `path`, if it's still at `revision` (`None`: any).
    Edit {
        path: PathBuf,
        edits: Vec<(Range, String)>,
        encoding: Encoding,
        revision: Option<u64>,
    },
    Notice(String),
    Error(String),
}

#[derive(Default)]
pub struct Lsp {
    /// Where server threads send their events (from the subscription).
    pub sink: Option<UnboundedSender<Event>>,
    servers: BTreeMap<ServerId, Server>,
    next_id: ServerId,
    /// Languages whose server couldn't start: not tried again until the
    /// settings change.
    failed: HashSet<String>,
    /// Diagnostics by file, with the encoding their positions are in.
    diagnostics: HashMap<PathBuf, (Encoding, Vec<Diagnostic>)>,
    pending: HashMap<(ServerId, u64), Pending>,
    /// Files edited since the server last got their text.
    pub stale: HashSet<PathBuf>,
    /// When the last of those edits was made.
    pub edited_at: Option<std::time::Instant>,
    pub sync_scheduled: bool,
    pub hover: Option<(PathBuf, usize, String)>,
    pub completion: Option<CompletionState>,
    /// The newest completion request: replies to older ones, which can
    /// come later, are dropped.
    last_completion: Option<(ServerId, u64)>,
}

/// The `languageId` LSP expects, from a grammar name.
fn language_id(syntax: &str) -> String {
    match syntax {
        "C++" => "cpp".into(),
        "C#" => "csharp".into(),
        "Bourne Again Shell (bash)" | "Shell-Unix-Generic" => "shellscript".into(),
        "TypeScriptReact" => "typescriptreact".into(),
        "JavaScript (Rails)" | "JavaScript" => "javascript".into(),
        "Objective-C" => "objective-c".into(),
        "Objective-C++" => "objective-cpp".into(),
        "Vue Component" => "vue".into(),
        "Dockerfile" => "dockerfile".into(),
        other => other.to_lowercase().replace(' ', ""),
    }
}

impl Lsp {
    /// Notes that `path` was edited: its text goes to the server once
    /// typing pauses.
    pub fn edited(&mut self, path: PathBuf) {
        self.stale.insert(path);
        self.edited_at = Some(std::time::Instant::now());
    }

    /// Stops every server; they start again as files need them.
    pub fn restart(&mut self) {
        self.servers.clear();
        self.failed.clear();
        self.pending.clear();
        self.diagnostics.clear();
        self.hover = None;
        self.completion = None;
    }

    fn server_of(&self, path: &Path) -> Option<(ServerId, &Server)> {
        self.servers.iter().find(|(_, s)| s.open.contains_key(path)).map(|(id, s)| (*id, s))
    }

    /// Starts servers the open files need, and tells each server which
    /// files are open and what's in them now. Returns an error to show
    /// when a server couldn't start.
    pub fn sync(
        &mut self,
        tabs: &Tabs,
        root: &Path,
        servers: &BTreeMap<String, Vec<String>>,
    ) -> Option<String> {
        let mut error = None;
        let texts: Vec<&Tab> = tabs.iter().filter(|t| t.buffer().is_some()).collect();
        for tab in &texts {
            let language = tab.buffer().map(|b| b.editor.syntax.clone()).unwrap_or_default();
            let Some(command) = servers.get(&language).filter(|c| !c.is_empty()) else { continue };
            let running = self.servers.values().any(|s| s.language == language);
            if running || self.failed.contains(&language) {
                continue;
            }
            let Some(sink) = self.sink.clone() else { continue };
            let id = self.next_id;
            self.next_id += 1;
            match Client::spawn(id, command, root, move |e| {
                let _ = sink.unbounded_send(e);
            }) {
                Ok(client) => {
                    let server = Server {
                        client,
                        language: language.clone(),
                        ready: false,
                        encoding: Encoding::Utf16,
                        capabilities: Value::Null,
                        open: HashMap::new(),
                    };
                    self.servers.insert(id, server);
                }
                Err(e) => {
                    self.failed.insert(language.clone());
                    error = Some(format!("{}: {e}", command[0]));
                }
            }
        }
        for server in self.servers.values_mut().filter(|s| s.ready) {
            // Closed tabs.
            let gone: Vec<PathBuf> = server
                .open
                .keys()
                .filter(|p| !texts.iter().any(|t| &t.path == *p))
                .cloned()
                .collect();
            for path in gone {
                server.open.remove(&path);
                server.client.notify(
                    "textDocument/didClose",
                    json!({ "textDocument": { "uri": text::uri(&path) } }),
                );
            }
            for tab in &texts {
                let Some(buffer) = tab.buffer() else { continue };
                if buffer.editor.syntax != server.language {
                    continue;
                }
                let uri = text::uri(&tab.path);
                match server.open.get_mut(&tab.path) {
                    None => {
                        server.open.insert(tab.path.clone(), 1);
                        server.client.notify(
                            "textDocument/didOpen",
                            json!({ "textDocument": {
                                "uri": uri,
                                "languageId": language_id(&server.language),
                                "version": 1,
                                "text": buffer.text(),
                            }}),
                        );
                    }
                    Some(version) if self.stale.contains(&tab.path) => {
                        *version += 1;
                        server.client.notify(
                            "textDocument/didChange",
                            json!({
                                "textDocument": { "uri": uri, "version": *version },
                                "contentChanges": [{ "text": buffer.text() }],
                            }),
                        );
                    }
                    Some(_) => {}
                }
            }
        }
        self.stale.clear();
        error
    }

    pub fn saved(&self, path: &Path) {
        if let Some((_, server)) = self.server_of(path) {
            server.client.notify(
                "textDocument/didSave",
                json!({ "textDocument": { "uri": text::uri(path) } }),
            );
        }
    }

    /// Whether the server of `path` offers `capability` (`hoverProvider`…).
    pub fn supports(&self, path: &Path, capability: &str) -> bool {
        self.server_of(path).is_some_and(|(_, s)| {
            !matches!(s.capabilities.get(capability), None | Some(Value::Null | Value::Bool(false)))
        })
    }

    /// Characters that open the completion list right away (`.`, `::`).
    pub fn triggers(&self, path: &Path) -> Vec<String> {
        self.server_of(path)
            .and_then(|(_, s)| {
                s.capabilities["completionProvider"]["triggerCharacters"].as_array().cloned()
            })
            .unwrap_or_default()
            .iter()
            .filter_map(|v| v.as_str().map(str::to_owned))
            .collect()
    }

    fn request(&mut self, path: &Path, method: &str, params: Value, pending: Pending) -> bool {
        let Some((id, server)) = self.server_of(path) else { return false };
        let request = server.client.request(method, params);
        if matches!(pending, Pending::Completion { .. }) {
            self.last_completion = Some((id, request));
        }
        self.pending.insert((id, request), pending);
        true
    }

    fn at(&self, path: &Path, rope: &ropey::Rope, at: usize) -> Option<Value> {
        let (_, server) = self.server_of(path)?;
        let position = text::position(rope, at, server.encoding);
        Some(json!({ "textDocument": { "uri": text::uri(path) }, "position": position }))
    }

    pub fn definition(&mut self, path: &Path, rope: &ropey::Rope, at: usize) -> bool {
        let Some(params) = self.at(path, rope, at) else { return false };
        self.request(path, "textDocument/definition", params, Pending::Definition)
    }

    pub fn hover(&mut self, path: &Path, rope: &ropey::Rope, at: usize) -> bool {
        let Some(params) = self.at(path, rope, at) else { return false };
        self.request(
            path,
            "textDocument/hover",
            params,
            Pending::Hover { path: path.to_owned(), at },
        )
    }

    /// Asks for completions at `cursor` for the word starting at `start`.
    pub fn complete(
        &mut self,
        path: &Path,
        rope: &ropey::Rope,
        start: usize,
        cursor: usize,
    ) -> bool {
        let Some(params) = self.at(path, rope, cursor) else { return false };
        self.request(
            path,
            "textDocument/completion",
            params,
            Pending::Completion { path: path.to_owned(), start },
        )
    }

    pub fn format(&mut self, path: &Path, revision: u64, tab_size: usize, spaces: bool) -> bool {
        let params = json!({
            "textDocument": { "uri": text::uri(path) },
            "options": { "tabSize": tab_size, "insertSpaces": spaces },
        });
        self.request(
            path,
            "textDocument/formatting",
            params,
            Pending::Format { path: path.to_owned(), revision },
        )
    }

    pub fn rename(&mut self, path: &Path, rope: &ropey::Rope, at: usize, name: &str) -> bool {
        let Some(mut params) = self.at(path, rope, at) else { return false };
        params["newName"] = json!(name);
        self.request(path, "textDocument/rename", params, Pending::Rename)
    }

    /// Diagnostics of `path` as byte ranges of `rope`, sorted.
    pub fn problems(
        &self,
        path: &Path,
        rope: &ropey::Rope,
    ) -> Vec<(std::ops::Range<usize>, Severity, &str)> {
        let Some((encoding, list)) = self.diagnostics.get(path) else { return Vec::new() };
        let mut out: Vec<_> = list
            .iter()
            .map(|d| (text::offsets(rope, d.range, *encoding), d.severity, d.message.as_str()))
            .collect();
        out.sort_by_key(|(r, s, _)| (r.start, *s));
        out
    }

    /// Error and warning counts of `path`.
    pub fn counts(&self, path: &Path) -> (usize, usize) {
        self.diagnostics.get(path).map_or((0, 0), |(_, list)| {
            let errors = list.iter().filter(|d| d.severity == Severity::Error).count();
            let warnings = list.iter().filter(|d| d.severity == Severity::Warning).count();
            (errors, warnings)
        })
    }

    /// Narrows the completion list to what's typed between its start and
    /// the cursor. Closes it when nothing matches or the cursor left the
    /// word.
    pub fn refilter(&mut self, path: &Path, rope: &ropey::Rope, cursor: usize) {
        let Some(c) = &mut self.completion else { return };
        let same_line =
            c.start <= cursor && rope.byte_to_line(c.start) == rope.byte_to_line(cursor);
        if c.path != path || !same_line {
            self.completion = None;
            return;
        }
        let typed = rope.byte_slice(c.start..cursor).to_string();
        if typed.chars().any(|ch| !(ch.is_alphanumeric() || ch == '_')) {
            self.completion = None;
            return;
        }
        c.shown = if typed.is_empty() {
            let mut all: Vec<usize> = (0..c.items.len()).collect();
            all.sort_by(|&a, &b| c.items[a].sort.cmp(&c.items[b].sort));
            all
        } else {
            let filters: Vec<String> = c.items.iter().map(|i| i.filter.clone()).collect();
            fuzzy::rank(&typed, &filters, 200)
        };
        c.selected = c.selected.min(c.shown.len().saturating_sub(1));
        // Nothing left, or only what's already typed in full.
        let only_itself = c.shown.iter().all(|&i| c.items[i].insert == typed);
        if c.shown.is_empty() || only_itself {
            self.completion = None;
        }
    }

    /// Handles what a server sent. `cursor` says where the active file's
    /// cursor is now, to drop replies that came too late.
    pub fn event(
        &mut self,
        event: Event,
        active: Option<(&Path, &ropey::Rope, usize)>,
    ) -> Vec<Effect> {
        match event {
            Event::Ready { server, encoding, capabilities } => {
                if let Some(s) = self.servers.get_mut(&server) {
                    s.ready = true;
                    s.encoding = encoding;
                    s.capabilities = capabilities;
                }
                Vec::new()
            }
            Event::Exited { server, message } => {
                let Some(s) = self.servers.remove(&server) else { return Vec::new() };
                self.failed.insert(s.language.clone());
                self.diagnostics.retain(|p, _| !s.open.contains_key(p));
                vec![Effect::Error(format!("{}: {message}", s.language))]
            }
            Event::Notification { server, method, params } => match method.as_str() {
                "textDocument/publishDiagnostics" => {
                    let encoding =
                        self.servers.get(&server).map_or(Encoding::Utf16, |s| s.encoding);
                    if let Some((uri, list)) = types::diagnostics(&params)
                        && let Some(path) = text::path(&uri)
                    {
                        self.diagnostics.insert(path, (encoding, list));
                    }
                    Vec::new()
                }
                "window/showMessage" if params["type"].as_u64() == Some(1) => params["message"]
                    .as_str()
                    .map(|m| vec![Effect::Error(m.to_owned())])
                    .unwrap_or_default(),
                _ => Vec::new(),
            },
            Event::Response { server, id, result } => {
                let Some(pending) = self.pending.remove(&(server, id)) else { return Vec::new() };
                let encoding = self.servers.get(&server).map_or(Encoding::Utf16, |s| s.encoding);
                let result = match result {
                    Ok(v) => v,
                    Err(e) => return vec![Effect::Error(e)],
                };
                match pending {
                    Pending::Definition => {
                        let Some((uri, range)) = types::locations(&result).into_iter().next()
                        else {
                            return Vec::new();
                        };
                        text::path(&uri)
                            .map(|path| Effect::Open { path, range, encoding })
                            .into_iter()
                            .collect()
                    }
                    Pending::Hover { path, at } => {
                        if let Some(text) = types::hover(&result) {
                            let shown =
                                self.hover.take().filter(|(p, a, _)| *p == path && *a == at);
                            let text = match shown {
                                // A diagnostic already on the card stays above.
                                Some((_, _, before)) if !before.is_empty() => {
                                    format!("{before}\n\n{text}")
                                }
                                _ => text,
                            };
                            self.hover = Some((path, at, text));
                        }
                        Vec::new()
                    }
                    Pending::Completion { path, start } => {
                        if self.last_completion != Some((server, id)) {
                            return Vec::new();
                        }
                        let items = types::completions(&result);
                        let Some((active_path, rope, cursor)) = active else { return Vec::new() };
                        if active_path != path || items.is_empty() {
                            return Vec::new();
                        }
                        self.completion = Some(CompletionState {
                            path: path.clone(),
                            start,
                            items,
                            shown: Vec::new(),
                            selected: 0,
                            encoding,
                        });
                        self.refilter(&path, rope, cursor);
                        Vec::new()
                    }
                    Pending::Format { path, revision } => {
                        let edits = types::text_edits(&result);
                        if edits.is_empty() {
                            return Vec::new();
                        }
                        vec![Effect::Edit { path, edits, encoding, revision: Some(revision) }]
                    }
                    Pending::Rename => {
                        let changes = types::workspace_edit(&result);
                        let files = changes.len();
                        let mut effects: Vec<Effect> = changes
                            .into_iter()
                            .filter_map(|(uri, edits)| {
                                Some(Effect::Edit {
                                    path: text::path(&uri)?,
                                    edits,
                                    encoding,
                                    revision: None,
                                })
                            })
                            .collect();
                        effects.push(Effect::Notice(format!("{files}")));
                        effects
                    }
                }
            }
        }
    }
}
