//! The terminal panel: a shell on a pseudo-terminal, read and parsed by
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
use std::path::Path;
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

#[derive(Clone)]
pub struct Listener(UnboundedSender<TermEvent>);

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
        let _ = self.0.unbounded_send(event);
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

/// Lines kept above the screen.
const SCROLLBACK: usize = 10_000;

pub struct Terminal {
    pub term: Arc<FairMutex<Term<Listener>>>,
    sender: EventLoopSender,
    pub size: Size,
    pub title: String,
}

impl Terminal {
    /// Starts the user's shell in `dir`.
    pub fn spawn(dir: &Path, size: Size, events: UnboundedSender<TermEvent>) -> io::Result<Self> {
        let env = HashMap::from([
            ("TERM".to_owned(), "xterm-256color".to_owned()),
            ("COLORTERM".to_owned(), "truecolor".to_owned()),
        ]);
        let options = tty::Options {
            shell: None,
            working_directory: Some(dir.to_path_buf()),
            drain_on_exit: false,
            env,
            #[cfg(windows)]
            escape_args: true,
        };
        let pty = tty::new(&options, window_size(size), 0)?;
        let listener = Listener(events);
        let config = Config { scrolling_history: SCROLLBACK, ..Config::default() };
        let term = Arc::new(FairMutex::new(Term::new(config, &size, listener.clone())));
        let event_loop = EventLoop::new(Arc::clone(&term), listener, pty, false, false)?;
        let sender = event_loop.channel();
        event_loop.spawn();
        Ok(Self { term, sender, size, title: String::new() })
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
