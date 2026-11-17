//! Keyboard shortcuts. There are only a few, and the README lists them all.
//!
//! | Keys                  | Action                     |
//! |-----------------------|----------------------------|
//! | Ctrl+O                | Open folder                |
//! | Ctrl+S                | Save                       |
//! | Ctrl+W                | Close tab                  |
//! | Ctrl+Z / Ctrl+Shift+Z | Undo / redo (also Ctrl+Y)  |
//! | Ctrl+F, F3, Shift+F3  | Find, next, previous       |
//! | Ctrl+Shift+F          | Find in files              |
//! | Ctrl+Shift+E          | Show the files             |
//! | Ctrl+H                | Replace (macOS: Cmd+Alt+F) |
//! | Ctrl+P                | Open file by name          |
//! | Ctrl+G                | Go to line                 |
//! | Ctrl+Shift+V          | Markdown preview           |
//! | Ctrl+B                | Toggle sidebar             |
//! | Ctrl+Tab              | Next tab (Shift: previous) |
//! | Ctrl+,                | Open settings.json         |
//! | Ctrl+N / Ctrl+Shift+N | New file / new folder      |
//! | F2 / Delete           | Rename / trash selection   |
//! | Ctrl+= / Ctrl+- / Ctrl+0 | Zoom in / out / reset   |
//! | Ctrl+`                | Show / hide the terminal   |
//!
//! In the editor ([`editor_command`]), besides arrows, Home, End and the
//! clipboard:
//!
//! | Keys                  | Action                     |
//! |-----------------------|----------------------------|
//! | Ctrl+/                | Comment / uncomment lines  |
//! | Alt+Up / Alt+Down     | Move lines                 |
//! | Shift+Alt+Up / Down   | Duplicate lines            |
//! | Ctrl+Alt+Up / Down    | Add a cursor above / below |
//! | Ctrl+D                | Select next occurrence     |
//! | Ctrl+Shift+L          | Select all occurrences     |
//! | Ctrl+Shift+K          | Delete lines               |
//! | Ctrl+L                | Select line (again: more)  |
//! | Alt+click             | Add or remove a cursor     |
//! | F12, Ctrl+click       | Go to definition (LSP)     |
//! | Ctrl+Space            | Complete (LSP)             |
//! | Shift+Alt+F           | Format the file (LSP)      |
//! | F2                    | Rename the symbol (LSP)    |
//! | F8 / Shift+F8         | Next / previous problem    |
//!
//! On macOS, `Cmd` replaces `Ctrl` and `Option` is `Alt`.

use bia_core::editor::Motion;
use iced::keyboard::key::{Named, Physical};
use iced::keyboard::{Key, Modifiers};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shortcut {
    OpenFolder,
    Save,
    CloseTab,
    Undo,
    Redo,
    Find,
    FindNext,
    FindPrevious,
    Replace,
    QuickOpen,
    FindInFiles,
    Explorer,
    GoToLine,
    Preview,
    Sidebar,
    NextTab,
    PreviousTab,
    Settings,
    NewFile,
    NewFolder,
    Rename,
    Delete,
    ZoomIn,
    ZoomOut,
    ZoomReset,
    Terminal,
    Escape,
    Confirm,
    /// Arrow keys nothing else used: they move through lists.
    Up,
    Down,
}

const MAC: bool = cfg!(target_os = "macos");

pub fn shortcut(key: &Key, physical: Physical, mods: Modifiers) -> Option<Shortcut> {
    use Shortcut::*;
    let cmd = mods.command();
    let shift = mods.shift();

    if let Key::Named(named) = key {
        return match named {
            Named::Escape => Some(Escape),
            Named::Enter if !cmd && !shift => Some(Confirm),
            Named::F3 if shift => Some(FindPrevious),
            Named::F3 => Some(FindNext),
            Named::F2 => Some(Rename),
            Named::Delete if !cmd => Some(Delete),
            Named::Tab if mods.control() && shift => Some(PreviousTab),
            Named::Tab if mods.control() => Some(NextTab),
            Named::ArrowUp => Some(Up),
            Named::ArrowDown => Some(Down),
            _ => None,
        };
    }
    if !cmd {
        return None;
    }
    // With Shift held, some platforms report `V` rather than `v`.
    Some(match key.to_latin(physical)?.to_ascii_lowercase() {
        'o' => OpenFolder,
        's' => Save,
        'w' => CloseTab,
        'z' if shift => Redo,
        'z' => Undo,
        'y' => Redo,
        // Cmd+H hides the app on macOS, so Replace is Cmd+Alt+F there.
        'f' if MAC && mods.alt() => Replace,
        'h' if !MAC => Replace,
        'f' if shift => FindInFiles,
        'f' => Find,
        'e' if shift => Explorer,
        'p' => QuickOpen,
        'g' => GoToLine,
        'v' if shift => Preview,
        'b' => Sidebar,
        ',' => Settings,
        'n' if shift => NewFolder,
        'n' => NewFile,
        '=' | '+' => ZoomIn,
        '-' => ZoomOut,
        '0' => ZoomReset,
        '`' => Terminal,
        _ => return None,
    })
}

/// A shortcut as people expect to read it on their platform: `Ctrl+S`,
/// or `⌘S` on macOS.
pub fn label(keys: &str) -> String {
    if cfg!(target_os = "macos") { keys.replace("Ctrl+", "⌘") } else { keys.to_owned() }
}

/// What a key does in the editor, while it has focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    /// Move every cursor; `true` extends the selections.
    Move(Motion, bool),
    Page {
        down: bool,
        extend: bool,
    },
    Backspace,
    Delete,
    DeleteWord {
        forward: bool,
    },
    Newline,
    Indent,
    Outdent,
    SelectAll,
    Copy,
    Cut,
    Paste,
    ToggleComment,
    MoveLines {
        up: bool,
    },
    DuplicateLines {
        down: bool,
    },
    DeleteLines,
    SelectLines,
    SelectNext,
    SelectAllOccurrences,
    AddCursor {
        up: bool,
    },
    /// Escape with several cursors: back to one.
    Collapse,
    // Language server commands.
    GoToDefinition,
    Complete,
    Format,
    RenameSymbol,
    NextProblem {
        back: bool,
    },
}

/// The editor's keys. Anything else (Ctrl+S, Ctrl+F…) isn't the editor's
/// and goes on to [`shortcut`].
pub fn editor_command(key: &Key, physical: Physical, mods: Modifiers) -> Option<Command> {
    use Command::*;
    let (cmd, shift, alt) = (mods.command(), mods.shift(), mods.alt());
    // Word steps: Option on macOS, Ctrl elsewhere.
    let word = if MAC { alt } else { cmd };
    if let Key::Named(named) = key {
        return Some(match named {
            Named::ArrowLeft | Named::ArrowRight => {
                let right = *named == Named::ArrowRight;
                let motion = if MAC && cmd {
                    if right { Motion::LineEnd } else { Motion::LineStart }
                } else if word {
                    if right { Motion::WordRight } else { Motion::WordLeft }
                } else if right {
                    Motion::Right
                } else {
                    Motion::Left
                };
                Move(motion, shift)
            }
            Named::ArrowUp | Named::ArrowDown => {
                let up = *named == Named::ArrowUp;
                match (cmd, alt) {
                    (true, true) => AddCursor { up },
                    (false, true) if shift => DuplicateLines { down: !up },
                    (false, true) => MoveLines { up },
                    (true, false) if MAC => {
                        Move(if up { Motion::DocumentStart } else { Motion::DocumentEnd }, shift)
                    }
                    (true, false) => return None,
                    (false, false) => Move(if up { Motion::Up } else { Motion::Down }, shift),
                }
            }
            Named::Home | Named::End => {
                let end = *named == Named::End;
                let motion = match (cmd, end) {
                    (true, false) => Motion::DocumentStart,
                    (true, true) => Motion::DocumentEnd,
                    (false, false) => Motion::LineStart,
                    (false, true) => Motion::LineEnd,
                };
                Move(motion, shift)
            }
            Named::PageUp | Named::PageDown => {
                Page { down: *named == Named::PageDown, extend: shift }
            }
            Named::Backspace if word => DeleteWord { forward: false },
            Named::Backspace => Backspace,
            Named::Delete if word => DeleteWord { forward: true },
            Named::Delete => Delete,
            Named::Enter if !cmd => Newline,
            Named::Tab if !cmd && !alt => {
                if shift {
                    Outdent
                } else {
                    Indent
                }
            }
            Named::Escape => Collapse,
            Named::F12 => GoToDefinition,
            Named::F2 => RenameSymbol,
            Named::F8 => NextProblem { back: shift },
            Named::Space if cmd && !shift => Complete,
            _ => return None,
        });
    }
    if is_format(key, physical, mods) {
        return Some(Format);
    }
    if !cmd || alt {
        return None;
    }
    Some(match key.to_latin(physical)?.to_ascii_lowercase() {
        'a' if !shift => SelectAll,
        'c' if !shift => Copy,
        'x' if !shift => Cut,
        'v' if !shift => Paste,
        // Layouts that need Shift for `/` (German, for one) still get it.
        '/' => ToggleComment,
        'k' if shift => DeleteLines,
        'l' if shift => SelectAllOccurrences,
        'l' => SelectLines,
        'd' if !shift => SelectNext,
        _ => return None,
    })
}

/// Shift+Alt+F (Shift+Option+F on macOS): format the file.
fn is_format(key: &Key, physical: Physical, mods: Modifiers) -> bool {
    mods.shift()
        && mods.alt()
        && !mods.command()
        && key.to_latin(physical).is_some_and(|c| c.eq_ignore_ascii_case(&'f'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::keyboard::key::Code;

    fn press(c: &str, code: Code, mods: Modifiers) -> Option<Shortcut> {
        shortcut(&Key::Character(c.into()), Physical::Code(code), mods)
    }

    #[test]
    fn shifted_letters_match_either_case() {
        let cmd_shift = Modifiers::COMMAND | Modifiers::SHIFT;
        for c in ["v", "V"] {
            assert_eq!(press(c, Code::KeyV, cmd_shift), Some(Shortcut::Preview));
        }
        for c in ["z", "Z"] {
            assert_eq!(press(c, Code::KeyZ, cmd_shift), Some(Shortcut::Redo));
        }
        assert_eq!(press("N", Code::KeyN, cmd_shift), Some(Shortcut::NewFolder));
    }

    #[test]
    fn plain_bindings() {
        assert_eq!(press("s", Code::KeyS, Modifiers::COMMAND), Some(Shortcut::Save));
        assert_eq!(press("z", Code::KeyZ, Modifiers::COMMAND), Some(Shortcut::Undo));
        assert_eq!(press(",", Code::Comma, Modifiers::COMMAND), Some(Shortcut::Settings));
        assert_eq!(press("p", Code::KeyP, Modifiers::COMMAND), Some(Shortcut::QuickOpen));
        assert_eq!(press("g", Code::KeyG, Modifiers::COMMAND), Some(Shortcut::GoToLine));
        let replace = if MAC {
            press("f", Code::KeyF, Modifiers::COMMAND | Modifiers::ALT)
        } else {
            press("h", Code::KeyH, Modifiers::COMMAND)
        };
        assert_eq!(replace, Some(Shortcut::Replace));
        // Typing letters without Ctrl is never a shortcut.
        assert_eq!(press("s", Code::KeyS, Modifiers::empty()), None);
        assert_eq!(press("S", Code::KeyS, Modifiers::SHIFT), None);
    }

    fn editor_key(key: Key, code: Code, mods: Modifiers) -> Option<Command> {
        editor_command(&key, Physical::Code(code), mods)
    }

    #[test]
    fn editor_keys() {
        let arrow = |n| Key::Named(n);
        let word = if MAC { Modifiers::ALT } else { Modifiers::CTRL };
        assert_eq!(
            editor_key(arrow(Named::ArrowRight), Code::ArrowRight, word | Modifiers::SHIFT),
            Some(Command::Move(Motion::WordRight, true))
        );
        assert_eq!(
            editor_key(arrow(Named::ArrowUp), Code::ArrowUp, Modifiers::ALT),
            Some(Command::MoveLines { up: true })
        );
        assert_eq!(
            editor_key(arrow(Named::ArrowDown), Code::ArrowDown, Modifiers::ALT | Modifiers::SHIFT),
            Some(Command::DuplicateLines { down: true })
        );
        assert_eq!(
            editor_key(
                arrow(Named::ArrowDown),
                Code::ArrowDown,
                Modifiers::COMMAND | Modifiers::ALT
            ),
            Some(Command::AddCursor { up: false })
        );
        let c = |s: &str, code, mods| editor_key(Key::Character(s.into()), code, mods);
        assert_eq!(c("d", Code::KeyD, Modifiers::COMMAND), Some(Command::SelectNext));
        assert_eq!(
            c("L", Code::KeyL, Modifiers::COMMAND | Modifiers::SHIFT),
            Some(Command::SelectAllOccurrences)
        );
        assert_eq!(c("v", Code::KeyV, Modifiers::COMMAND), Some(Command::Paste));
        // Global shortcuts aren't the editor's.
        assert_eq!(c("s", Code::KeyS, Modifiers::COMMAND), None);
        assert_eq!(c("V", Code::KeyV, Modifiers::COMMAND | Modifiers::SHIFT), None);
        // Plain letters are typing.
        assert_eq!(c("a", Code::KeyA, Modifiers::empty()), None);
    }

    #[test]
    fn zoom() {
        assert_eq!(press("=", Code::Equal, Modifiers::COMMAND), Some(Shortcut::ZoomIn));
        assert_eq!(
            press("+", Code::Equal, Modifiers::COMMAND | Modifiers::SHIFT),
            Some(Shortcut::ZoomIn)
        );
        assert_eq!(press("-", Code::Minus, Modifiers::COMMAND), Some(Shortcut::ZoomOut));
        assert_eq!(press("0", Code::Digit0, Modifiers::COMMAND), Some(Shortcut::ZoomReset));
    }

    #[test]
    fn non_latin_layouts_use_the_physical_key() {
        // A Cyrillic layout reports "ы" for the S key; Ctrl+S (Cmd+S) must still save.
        assert_eq!(press("ы", Code::KeyS, Modifiers::COMMAND), Some(Shortcut::Save));
    }
}
