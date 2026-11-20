//! The terminal panel. Shells on pseudo-terminals, each read and parsed by
//! `alacritty_terminal` on its own thread, and the bytes keys turn into.

use alacritty_terminal::event::{Event, EventListener, WindowSize};
use alacritty_terminal::event_loop::{EventLoop, EventLoopSender, Msg};
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::{Config, Term, TermMode};
use alacritty_terminal::tty;
use iced::futures::channel::mpsc::UnboundedSender;
use iced::keyboard::key::Named;
use iced::keyboard::{Key, Modifiers};
use std::borrow::Cow;
use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// What the terminal thread tells the app.
#[derive(Debug, Clone)]
pub enum TermEvent {
    /// New output to draw.
    Wakeup,
    Title(String),
    /// Bytes the terminal answers the program with (cursor reports…).
    Reply(String),
    /// A program asked to put text on the clipboard (OSC 52).
    Copy(String),
    /// The shell exited.
    Exit,
}

/// Sends a terminal's events to the app, tagged with the terminal's id.
#[derive(Clone)]
pub struct Listener {
    id: u64,
    events: UnboundedSender<(u64, TermEvent)>,
}

impl EventListener for Listener {
    fn send_event(&self, event: Event) {
        let event = match event {
            Event::Wakeup => TermEvent::Wakeup,
            Event::Title(title) => TermEvent::Title(title),
            Event::PtyWrite(text) => TermEvent::Reply(text),
            Event::ClipboardStore(_, text) => TermEvent::Copy(text),
            Event::Exit | Event::ChildExit(_) => TermEvent::Exit,
            _ => return,
        };
        let _ = self.events.unbounded_send((self.id, event));
    }
}

/// Rows and columns, for alacritty.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Size {
    pub columns: usize,
    pub lines: usize,
}

impl Dimensions for Size {
    fn total_lines(&self) -> usize {
        self.lines
    }

    fn screen_lines(&self) -> usize {
        self.lines
    }

    fn columns(&self) -> usize {
        self.columns
    }
}

/// How often a terminal's tab looks at which program runs in front.
pub const LOOK_EVERY: std::time::Duration = std::time::Duration::from_millis(400);

/// Lines kept above the screen.
const SCROLLBACK: usize = 10_000;

pub struct Terminal {
    pub id: u64,
    pub term: Arc<FairMutex<Term<Listener>>>,
    sender: EventLoopSender,
    pub size: Size,
    /// The title a program set, or the shell's name.
    pub title: String,
    /// The program running in front (`zsh`, `cargo`), where the system
    /// tells. The tab shows it instead of the title.
    pub program: Option<String>,
    looked_at: std::time::Instant,
    stale: bool,
    /// The folder the shell started in.
    pub dir: PathBuf,
    /// The shell's process id, where the system lets us look at it.
    pid: Option<u32>,
}

impl Terminal {
    /// Starts `shell` (a program and its arguments), or the user's login
    /// shell, in `dir`.
    pub fn spawn(
        id: u64,
        dir: &Path,
        size: Size,
        events: UnboundedSender<(u64, TermEvent)>,
        shell: Option<(String, Vec<String>)>,
    ) -> io::Result<Self> {
        let env = HashMap::from([
            ("TERM".to_owned(), "xterm-256color".to_owned()),
            ("COLORTERM".to_owned(), "truecolor".to_owned()),
        ]);
        let title = shell.as_ref().map(|(program, _)| program.clone());
        let options = tty::Options {
            shell: shell.map(|(program, args)| tty::Shell::new(program, args)),
            working_directory: Some(dir.to_path_buf()),
            drain_on_exit: false,
            env,
            #[cfg(windows)]
            escape_args: true,
        };
        let pty = tty::new(&options, window_size(size), id)?;
        #[cfg(unix)]
        let pid = Some(pty.child().id());
        #[cfg(not(unix))]
        let pid = None;
        let listener = Listener { id, events };
        let config = Config { scrolling_history: SCROLLBACK, ..Config::default() };
        let term = Arc::new(FairMutex::new(Term::new(config, &size, listener.clone())));
        let event_loop = EventLoop::new(Arc::clone(&term), listener, pty, false, false)?;
        let sender = event_loop.channel();
        event_loop.spawn();
        Ok(Self {
            id,
            term,
            sender,
            size,
            title: title
                .as_deref()
                .map(Path::new)
                .and_then(Path::file_stem)
                .map_or_else(shell_name, |s| s.to_string_lossy().into_owned()),
            program: None,
            looked_at: std::time::Instant::now() - LOOK_EVERY,
            stale: false,
            dir: dir.to_path_buf(),
            pid,
        })
    }

    /// The name for the tab.
    pub fn name(&self) -> &str {
        self.program.as_deref().unwrap_or(&self.title)
    }

    /// Looks again at which program runs in front, at most a few times a
    /// second, since output can come thousands of times a second. Returns
    /// `true` when it's too soon, and the app should ask again in a moment.
    pub fn look_at_program(&mut self) -> bool {
        if self.looked_at.elapsed() < LOOK_EVERY {
            self.stale = true;
            return true;
        }
        self.looked_at = std::time::Instant::now();
        self.stale = false;
        self.program = self.pid.and_then(front_program);
        false
    }

    /// Output came while it was too soon to look, so look now.
    pub fn look_again(&mut self) {
        if self.stale {
            self.looked_at = std::time::Instant::now() - LOOK_EVERY;
            self.look_at_program();
        }
    }

    /// The folder the shell is in now. Only Linux tells; elsewhere it's
    /// the folder it started in.
    pub fn cwd(&self) -> PathBuf {
        #[cfg(target_os = "linux")]
        if let Some(pid) = self.pid
            && let Ok(dir) = std::fs::read_link(format!("/proc/{pid}/cwd"))
        {
            return dir;
        }
        self.dir.clone()
    }

    /// A program other than the shell is running in front (a build, a
    /// server, an editor), so closing the terminal would stop it. Windows
    /// doesn't say, so there it's always `false`.
    pub fn busy(&self) -> bool {
        let Some(pid) = self.pid else { return false };
        foreground_job(pid)
    }

    pub fn write(&self, bytes: impl Into<Cow<'static, [u8]>>) {
        let _ = self.sender.send(Msg::Input(bytes.into()));
    }

    /// Typing scrolls back to the bottom, as in every terminal.
    pub fn input(&self, bytes: Vec<u8>) {
        self.term.lock().scroll_display(Scroll::Bottom);
        self.write(bytes);
    }

    pub fn resize(&mut self, size: Size) {
        if size == self.size || size.columns == 0 || size.lines == 0 {
            return;
        }
        self.size = size;
        self.term.lock().resize(size);
        let _ = self.sender.send(Msg::Resize(window_size(size)));
    }

    pub fn scroll(&self, lines: i32) {
        self.term.lock().scroll_display(Scroll::Delta(lines));
    }

    /// Text pasted: bracketed when the program asked for it, so a shell
    /// doesn't run pasted lines one by one.
    pub fn paste(&self, text: &str) {
        let text = text.replace("\r\n", "\r").replace('\n', "\r");
        let bracketed = self.term.lock().mode().contains(TermMode::BRACKETED_PASTE);
        let bytes = if bracketed {
            format!("\x1b[200~{}\x1b[201~", text.replace('\x1b', "")).into_bytes()
        } else {
            text.into_bytes()
        };
        self.input(bytes);
    }

    pub fn app_cursor(&self) -> bool {
        self.term.lock().mode().contains(TermMode::APP_CURSOR)
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        let _ = self.sender.send(Msg::Shutdown);
    }
}

/// The shell's name, for the tab before any program sets a title.
fn shell_name() -> String {
    let shell = std::env::var_os("SHELL").or_else(|| std::env::var_os("ComSpec"));
    shell
        .as_deref()
        .map(Path::new)
        .and_then(Path::file_stem)
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "shell".to_owned())
}

/// Whether the terminal of shell `pid` has a foreground process group
/// other than the shell's own.
#[cfg(target_os = "linux")]
fn foreground_job(pid: u32) -> bool {
    // After the command name come state, ppid, pgrp, session, tty and tpgid.
    let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) else { return false };
    let Some((_, fields)) = stat.rsplit_once(')') else { return false };
    let fields: Vec<&str> = fields.split_whitespace().collect();
    match (fields.get(2), fields.get(5)) {
        (Some(group), Some(front)) => front.parse::<i64>().is_ok_and(|f| f > 0) && group != front,
        _ => false,
    }
}

/// The name of the program in front of shell `pid`'s terminal.
#[cfg(target_os = "linux")]
fn front_program(pid: u32) -> Option<String> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let (_, fields) = stat.rsplit_once(')')?;
    let front: i64 = fields.split_whitespace().nth(5)?.parse().ok()?;
    let target = if front > 0 { front.to_string() } else { pid.to_string() };
    let name = std::fs::read_to_string(format!("/proc/{target}/comm")).ok()?;
    Some(name.trim().to_owned()).filter(|n| !n.is_empty())
}

#[cfg(not(target_os = "linux"))]
fn front_program(_pid: u32) -> Option<String> {
    None
}

#[cfg(target_os = "macos")]
fn foreground_job(pid: u32) -> bool {
    let out = std::process::Command::new("ps")
        .args(["-o", "pgid=,tpgid=", "-p", &pid.to_string()])
        .output();
    let Ok(out) = out else { return false };
    let text = String::from_utf8_lossy(&out.stdout);
    let mut numbers = text.split_whitespace().map(|n| n.parse::<i64>().unwrap_or(0));
    match (numbers.next(), numbers.next()) {
        (Some(group), Some(front)) => front > 0 && group != front,
        _ => false,
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn foreground_job(_pid: u32) -> bool {
    false
}

/// The terminals in the panel, in the order of their tabs.
#[derive(Default)]
pub struct Terminals {
    pub list: Vec<Terminal>,
    /// Index of the terminal in front.
    pub active: usize,
    /// The panel is showing. Hidden terminals keep running.
    pub open: bool,
    /// A [`Terminal::look_again`] is on its way.
    pub look_scheduled: bool,
    next_id: u64,
}

impl Terminals {
    pub fn active(&self) -> Option<&Terminal> {
        self.list.get(self.active)
    }

    pub fn get_mut(&mut self, id: u64) -> Option<&mut Terminal> {
        self.list.iter_mut().find(|t| t.id == id)
    }

    pub fn get(&self, id: u64) -> Option<&Terminal> {
        self.list.iter().find(|t| t.id == id)
    }

    pub fn next_id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }

    /// Adds a terminal and brings it to the front.
    pub fn push(&mut self, terminal: Terminal) {
        self.list.push(terminal);
        self.active = self.list.len() - 1;
        self.open = true;
    }

    /// Closes terminal `id`. The one to its left comes to the front, and
    /// the panel hides when none are left.
    pub fn remove(&mut self, id: u64) {
        let Some(i) = self.list.iter().position(|t| t.id == id) else { return };
        self.list.remove(i);
        if self.active >= i && self.active > 0 {
            self.active -= 1;
        }
        if self.list.is_empty() {
            self.open = false;
        }
    }

    pub fn select(&mut self, id: u64) {
        if let Some(i) = self.list.iter().position(|t| t.id == id) {
            self.active = i;
        }
    }

    /// Some terminal runs a program that closing BIA would stop.
    pub fn any_busy(&self) -> bool {
        self.list.iter().any(Terminal::busy)
    }
}

fn window_size(size: Size) -> WindowSize {
    WindowSize {
        num_lines: size.lines as u16,
        num_cols: size.columns as u16,
        // Only programs that draw images use these.
        cell_width: 8,
        cell_height: 16,
    }
}

/// The bytes a key sends to the program, or `None` for keys the terminal
/// leaves alone (typed text comes separately, with its `text`).
pub fn key_bytes(key: &Key, mods: Modifiers, app_cursor: bool) -> Option<Vec<u8>> {
    let modifier =
        1 + u8::from(mods.shift()) + 2 * u8::from(mods.alt()) + 4 * u8::from(mods.control());
    let csi = |code: &str| -> Vec<u8> {
        if modifier > 1 {
            format!("\x1b[1;{modifier}{code}").into_bytes()
        } else if app_cursor {
            format!("\x1bO{code}").into_bytes()
        } else {
            format!("\x1b[{code}").into_bytes()
        }
    };
    let tilde = |n: u8| -> Vec<u8> {
        if modifier > 1 { format!("\x1b[{n};{modifier}~") } else { format!("\x1b[{n}~") }
            .into_bytes()
    };
    match key {
        Key::Named(named) => Some(match named {
            Named::Enter => {
                if mods.alt() {
                    b"\x1b\r".to_vec()
                } else {
                    b"\r".to_vec()
                }
            }
            Named::Backspace => {
                if mods.alt() {
                    b"\x1b\x7f".to_vec()
                } else if mods.control() {
                    b"\x08".to_vec()
                } else {
                    b"\x7f".to_vec()
                }
            }
            Named::Tab if mods.shift() => b"\x1b[Z".to_vec(),
            Named::Tab => b"\t".to_vec(),
            Named::Escape => b"\x1b".to_vec(),
            Named::ArrowUp => csi("A"),
            Named::ArrowDown => csi("B"),
            Named::ArrowRight => csi("C"),
            Named::ArrowLeft => csi("D"),
            Named::Home => csi("H"),
            Named::End => csi("F"),
            Named::Insert => tilde(2),
            Named::Delete => tilde(3),
            Named::PageUp => tilde(5),
            Named::PageDown => tilde(6),
            Named::F1 => b"\x1bOP".to_vec(),
            Named::F2 => b"\x1bOQ".to_vec(),
            Named::F3 => b"\x1bOR".to_vec(),
            Named::F4 => b"\x1bOS".to_vec(),
            Named::F5 => tilde(15),
            Named::F6 => tilde(17),
            Named::F7 => tilde(18),
            Named::F8 => tilde(19),
            Named::F9 => tilde(20),
            Named::F10 => tilde(21),
            Named::F11 => tilde(23),
            Named::F12 => tilde(24),
            Named::Space if mods.control() => vec![0],
            _ => return None,
        }),
        Key::Character(c) if mods.control() => {
            let c = c.chars().next()?.to_ascii_lowercase();
            let byte = match c {
                'a'..='z' => c as u8 - b'a' + 1,
                '@' | '2' | ' ' => 0,
                '[' | '3' => 0x1b,
                '\\' | '4' => 0x1c,
                ']' | '5' => 0x1d,
                '^' | '6' => 0x1e,
                '_' | '7' | '-' | '/' => 0x1f,
                _ => return None,
            };
            Some(if mods.alt() { vec![0x1b, byte] } else { vec![byte] })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named(n: Named) -> Key {
        Key::Named(n)
    }

    #[test]
    fn keys() {
        let none = Modifiers::empty();
        assert_eq!(key_bytes(&named(Named::ArrowUp), none, false).unwrap(), b"\x1b[A");
        assert_eq!(key_bytes(&named(Named::ArrowUp), none, true).unwrap(), b"\x1bOA");
        assert_eq!(
            key_bytes(&named(Named::ArrowLeft), Modifiers::CTRL, false).unwrap(),
            b"\x1b[1;5D"
        );
        assert_eq!(key_bytes(&named(Named::Enter), none, false).unwrap(), b"\r");
        assert_eq!(key_bytes(&named(Named::Backspace), none, false).unwrap(), b"\x7f");
        assert_eq!(key_bytes(&named(Named::Delete), none, false).unwrap(), b"\x1b[3~");
        assert_eq!(key_bytes(&Key::Character("c".into()), Modifiers::CTRL, false).unwrap(), [3]);
        assert_eq!(key_bytes(&Key::Character("[".into()), Modifiers::CTRL, false).unwrap(), [0x1b]);
        assert_eq!(key_bytes(&Key::Character("c".into()), none, false), None);
    }
}
