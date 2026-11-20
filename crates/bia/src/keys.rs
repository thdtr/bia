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
//! | Ctrl+N / Ctrl+Shift+N | New file / new window      |
//! | Ctrl+Shift+S          | Save as                    |
//! | Ctrl+Shift+T          | Reopen the closed tab      |
//! | Ctrl+Shift+P          | All commands               |
//! | Ctrl+Q                | Quit                       |
//! | F2 / Delete           | Rename / trash selection   |
//! | Ctrl+= / Ctrl+- / Ctrl+0 | Zoom in / out / reset   |
//! | Ctrl+`                | Show / hide the terminal   |
//! | Ctrl+Shift+`          | New terminal               |
//! | Ctrl+\                 | Move tab to the other side |
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
    NewWindow,
    SaveAs,
    ReopenTab,
    Commands,
    Quit,
    /// Back to where the cursor was before the last jump, and forward again.
    Back,
    Forward,
    Rename,
    Delete,
    ZoomIn,
    ZoomOut,
    ZoomReset,
    Terminal,
    NewTerminal,
    Split,
    /// Ctrl+X, Ctrl+C and Ctrl+V, when no editor, terminal or text field
    /// took them, act on the file tree.
    Cut,
    Copy,
    Paste,
    Escape,
    Confirm,
    /// Arrow keys nothing else used: they move through lists.
    Up,
    Down,
}

const MAC: bool = cfg!(target_os = "macos");

/// The key left of 1, whatever it types. That's ` on a US keyboard and ' on a
/// Brazilian one, where ` is a dead key. Shift turns it into ~ or ".
pub fn is_backquote(key: &Key, physical: Physical) -> bool {
    use iced::keyboard::key::Code;
    physical == Physical::Code(Code::Backquote) || matches!(key.to_latin(physical), Some('`' | '~'))
}

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
            Named::ArrowLeft if mods.alt() && !MAC && !cmd => Some(Back),
            Named::ArrowRight if mods.alt() && !MAC && !cmd => Some(Forward),
            _ => None,
        };
    }
    // Ctrl+` everywhere, as in other editors: on macOS, Cmd+` switches
    // windows.
    if mods.control() && is_backquote(key, physical) {
        return Some(if shift { NewTerminal } else { Terminal });
    }
    // Ctrl+- and Ctrl+Shift+- on a Mac, where Option+Left moves by word.
    if MAC && mods.control() && !mods.logo() && key.to_latin(physical) == Some('-') {
        return Some(if shift { Forward } else { Back });
    }
    if !cmd {
        return None;
    }
    // With Shift held, some platforms report `V` rather than `v`.
    Some(match key.to_latin(physical)?.to_ascii_lowercase() {
        's' if shift => SaveAs,
        't' if shift => ReopenTab,
        'p' if shift => Commands,
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
        'v' => Paste,
        'x' => Cut,
        'c' if !shift => Copy,
        'b' => Sidebar,
        '\\' => Split,
        ',' => Settings,
        'n' if shift => NewWindow,
        'n' => NewFile,
        'q' => Quit,
        '=' | '+' => ZoomIn,
        '-' => ZoomOut,
        '0' => ZoomReset,
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
            // Alt+Left and Alt+Right go back and forward (see `shortcut`).
            Named::ArrowLeft | Named::ArrowRight if alt && !MAC && !cmd && !shift => return None,
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
            // Ctrl on macOS too: Cmd+Space opens Spotlight.
            Named::Space if mods.control() && !shift => Complete,
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

// ── Your own shortcuts ───────────────────────────────────────────────────

use bia_core::keybind::Combo;

/// What a key in `keybindings` does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    App(crate::commands::Command),
    Editor(Command),
    /// `"none"`: the key does nothing, not even what it did before.
    Nothing,
}

/// The editor actions `keybindings` can name, with those names.
const EDITOR_ACTIONS: [(&str, Command); 20] = [
    ("comment", Command::ToggleComment),
    ("move_lines_up", Command::MoveLines { up: true }),
    ("move_lines_down", Command::MoveLines { up: false }),
    ("duplicate_lines_up", Command::DuplicateLines { down: false }),
    ("duplicate_lines_down", Command::DuplicateLines { down: true }),
    ("delete_lines", Command::DeleteLines),
    ("select_line", Command::SelectLines),
    ("select_next", Command::SelectNext),
    ("select_all_occurrences", Command::SelectAllOccurrences),
    ("select_all", Command::SelectAll),
    ("add_cursor_above", Command::AddCursor { up: true }),
    ("add_cursor_below", Command::AddCursor { up: false }),
    ("indent", Command::Indent),
    ("outdent", Command::Outdent),
    ("go_to_definition", Command::GoToDefinition),
    ("complete", Command::Complete),
    ("format", Command::Format),
    ("rename_symbol", Command::RenameSymbol),
    ("next_problem", Command::NextProblem { back: false }),
    ("previous_problem", Command::NextProblem { back: true }),
];

/// The action a name in `keybindings` stands for.
pub fn action(name: &str) -> Option<Action> {
    let name = name.trim();
    if name == "none" {
        return Some(Action::Nothing);
    }
    if let Some(command) = crate::commands::Command::ALL.into_iter().find(|c| c.id() == name) {
        return Some(Action::App(command));
    }
    EDITOR_ACTIONS.iter().find(|(n, _)| *n == name).map(|(_, c)| Action::Editor(*c))
}

/// Every name `keybindings` takes.
#[cfg(test)]
fn action_names() -> Vec<&'static str> {
    crate::commands::Command::ALL
        .into_iter()
        .map(crate::commands::Command::id)
        .chain(EDITOR_ACTIONS.iter().map(|(n, _)| *n))
        .collect()
}

/// The keys of `keybindings`, read once when the settings load.
#[derive(Debug, Default)]
pub struct Bindings {
    list: Vec<(Combo, Action, String)>,
}

pub static NO_BINDINGS: Bindings = Bindings { list: Vec::new() };

impl Bindings {
    /// Reads `keybindings`. Entries it can't read come back by their keys,
    /// to be pointed out.
    pub fn new(map: &std::collections::BTreeMap<String, String>) -> (Self, Vec<String>) {
        let mut list = Vec::new();
        let mut errors = Vec::new();
        for (keys, name) in map {
            match (Combo::parse(keys), action(name)) {
                (Some(combo), Some(action)) => list.push((combo, action, keys.trim().to_owned())),
                _ => errors.push(keys.clone()),
            }
        }
        (Self { list }, errors)
    }

    /// What the key pressed is bound to, if you bound it.
    pub fn find(&self, key: &Key, physical: Physical, mods: Modifiers) -> Option<Action> {
        if self.list.is_empty() {
            return None;
        }
        let pressed = combo_of(key, physical, mods)?;
        self.list.iter().find(|(c, ..)| *c == pressed).map(|(_, a, _)| *a)
    }

    /// The keys shown next to a command: yours if you bound some, else
    /// BIA's own, unless you gave those to something else.
    pub fn keys_for(&self, command: crate::commands::Command) -> Option<String> {
        if let Some((.., keys)) = self.list.iter().find(|(_, a, _)| *a == Action::App(command)) {
            return Some(keys.clone());
        }
        let default = command.keys()?;
        let taken = Combo::parse(default).is_some_and(|d| self.list.iter().any(|(c, ..)| *c == d));
        (!taken).then(|| default.to_owned())
    }
}

/// The key pressed, as `keybindings` writes it.
fn combo_of(key: &Key, physical: Physical, mods: Modifiers) -> Option<Combo> {
    use iced::keyboard::key::Code;
    let name = match key {
        Key::Named(named) => match named {
            Named::ArrowUp => "up",
            Named::ArrowDown => "down",
            Named::ArrowLeft => "left",
            Named::ArrowRight => "right",
            Named::Home => "home",
            Named::End => "end",
            Named::PageUp => "pageup",
            Named::PageDown => "pagedown",
            Named::Enter => "enter",
            Named::Tab => "tab",
            Named::Escape => "escape",
            Named::Backspace => "backspace",
            Named::Delete => "delete",
            Named::Insert => "insert",
            Named::Space => "space",
            Named::F1 => "f1",
            Named::F2 => "f2",
            Named::F3 => "f3",
            Named::F4 => "f4",
            Named::F5 => "f5",
            Named::F6 => "f6",
            Named::F7 => "f7",
            Named::F8 => "f8",
            Named::F9 => "f9",
            Named::F10 => "f10",
            Named::F11 => "f11",
            Named::F12 => "f12",
            Named::ContextMenu => "menu",
            _ => return None,
        }
        .to_owned(),
        _ => {
            // With Shift, `1` types `!` and `` ` `` types `~`: the key is the
            // one written without Shift.
            let base = match physical {
                Physical::Code(code) if mods.shift() => match code {
                    Code::Backquote => Some('`'),
                    Code::Minus => Some('-'),
                    Code::Equal => Some('='),
                    Code::BracketLeft => Some('['),
                    Code::BracketRight => Some(']'),
                    Code::Backslash => Some('\\'),
                    Code::Semicolon => Some(';'),
                    Code::Quote => Some('\''),
                    Code::Comma => Some(','),
                    Code::Period => Some('.'),
                    Code::Slash => Some('/'),
                    Code::Digit0 => Some('0'),
                    Code::Digit1 => Some('1'),
                    Code::Digit2 => Some('2'),
                    Code::Digit3 => Some('3'),
                    Code::Digit4 => Some('4'),
                    Code::Digit5 => Some('5'),
                    Code::Digit6 => Some('6'),
                    Code::Digit7 => Some('7'),
                    Code::Digit8 => Some('8'),
                    Code::Digit9 => Some('9'),
                    _ => None,
                },
                _ => None,
            };
            let c = base.or_else(|| key.to_latin(physical))?;
            c.to_lowercase().to_string()
        }
    };
    Some(Combo { ctrl: mods.command(), alt: mods.alt(), shift: mods.shift(), key: name })
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::keyboard::key::Code;

    #[test]
    fn your_keys_come_first() {
        let map = std::collections::BTreeMap::from([
            ("Ctrl+Shift+D".to_owned(), "duplicate_lines_down".to_owned()),
            ("Ctrl+S".to_owned(), "none".to_owned()),
            ("Ctrl+Shift+`".to_owned(), "save".to_owned()),
            ("Ctrl+Nope".to_owned(), "save".to_owned()),
            ("F9".to_owned(), "fly".to_owned()),
        ]);
        let (bindings, errors) = Bindings::new(&map);
        assert_eq!(errors, ["Ctrl+Nope", "F9"]);
        let cmd_shift = Modifiers::COMMAND | Modifiers::SHIFT;
        let d = bindings.find(&Key::Character("D".into()), Physical::Code(Code::KeyD), cmd_shift);
        assert_eq!(d, Some(Action::Editor(Command::DuplicateLines { down: true })));
        let s = bindings.find(
            &Key::Character("s".into()),
            Physical::Code(Code::KeyS),
            Modifiers::COMMAND,
        );
        assert_eq!(s, Some(Action::Nothing));
        // Shift+` types ~, but it's still the ` key.
        let tilde =
            bindings.find(&Key::Character("~".into()), Physical::Code(Code::Backquote), cmd_shift);
        assert_eq!(tilde, Some(Action::App(crate::commands::Command::Save)));
        // Save shows the keys you gave it, and New terminal lost its own.
        assert_eq!(
            bindings.keys_for(crate::commands::Command::Save).as_deref(),
            Some("Ctrl+Shift+`")
        );
        assert_eq!(bindings.keys_for(crate::commands::Command::NewTerminal), None);
        assert!(action_names().contains(&"select_next"));
    }

    #[test]
    fn the_settings_page_lists_every_action() {
        let en = include_str!("../../../site/content/docs/settings.md");
        let pt = include_str!("../../../site/content/docs/settings.pt-br.md");
        for name in action_names() {
            let quoted = format!("`{name}`");
            assert!(en.contains(&quoted), "settings.md doesn't list {name}");
            assert!(pt.contains(&quoted), "settings.pt-br.md doesn't list {name}");
        }
    }

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
        assert_eq!(press("N", Code::KeyN, cmd_shift), Some(Shortcut::NewWindow));
        assert_eq!(press("P", Code::KeyP, cmd_shift), Some(Shortcut::Commands));
        assert_eq!(press("S", Code::KeyS, cmd_shift), Some(Shortcut::SaveAs));
        assert_eq!(press("T", Code::KeyT, cmd_shift), Some(Shortcut::ReopenTab));
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
