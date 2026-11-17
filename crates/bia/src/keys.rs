//! Keyboard shortcuts. There are only a few, and the README lists them all.
//!
//! | Keys                  | Action                     |
//! |-----------------------|----------------------------|
//! | Ctrl+O                | Open folder                |
//! | Ctrl+S                | Save                       |
//! | Ctrl+W                | Close tab                  |
//! | Ctrl+Z / Ctrl+Shift+Z | Undo / redo (also Ctrl+Y)  |
//! | Ctrl+F, F3, Shift+F3  | Find, next, previous       |
//! | Ctrl+H                | Replace (macOS: Cmd+Alt+F) |
//! | Ctrl+P                | Open file by name          |
//! | Ctrl+G                | Go to line                 |
//! | Ctrl+Shift+V          | Markdown preview           |
//! | Ctrl+B                | Toggle sidebar             |
//! | Ctrl+Tab              | Next tab (Shift: previous) |
//! | Ctrl+,                | Open settings.json         |
//! | Ctrl+N / Ctrl+Shift+N | New file / new folder      |
//! | F2 / Delete           | Rename / trash selection   |
//!
//! On macOS, `Cmd` replaces `Ctrl`.

use crate::app::Message;
use iced::keyboard::key::{Named, Physical};
use iced::keyboard::{Key, Modifiers};
use iced::widget::text_editor::{Action, Binding, Edit, KeyPress, Status};

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
        'f' => Find,
        'p' => QuickOpen,
        'g' => GoToLine,
        'v' if shift => Preview,
        'b' => Sidebar,
        ',' => Settings,
        'n' if shift => NewFolder,
        'n' => NewFile,
        _ => return None,
    })
}

/// A shortcut as people expect to read it on their platform: `Ctrl+S`,
/// or `⌘S` on macOS.
pub fn label(keys: &str) -> String {
    if cfg!(target_os = "macos") { keys.replace("Ctrl+", "⌘") } else { keys.to_owned() }
}

/// Key handling inside the editor. Clipboard and motion keys keep iced's
/// defaults; `Tab` indents; every other `Ctrl` combination is *not* consumed,
/// so it reaches the global shortcut handler above.
pub fn editor_binding(press: KeyPress) -> Option<Binding<Message>> {
    if !matches!(press.status, Status::Focused { .. }) {
        return None;
    }
    let mods = press.modifiers;
    // The editor lives in an outer scroll area, so it can't know how tall a
    // "page" is. BIA handles paging itself.
    if let Key::Named(named @ (Named::PageUp | Named::PageDown)) = press.key {
        let down = named == Named::PageDown;
        return Some(Binding::Custom(Message::Page { down, select: mods.shift() }));
    }
    if let Key::Named(Named::Tab) = press.key {
        if mods.control() || mods.alt() || mods.logo() {
            return None;
        }
        let edit = if mods.shift() { Edit::Unindent } else { Edit::Indent };
        return Some(Binding::Custom(Message::Edit(Action::Edit(edit))));
    }
    if mods.command() {
        let clipboard_or_select_all = matches!(
            press.key.to_latin(press.physical_key).map(|c| c.to_ascii_lowercase()),
            Some('c' | 'x' | 'v' | 'a')
        ) && !mods.shift();
        let is_motion = matches!(press.key, Key::Named(_));
        if !clipboard_or_select_all && !is_motion {
            return None;
        }
    }
    Binding::from_key_press(press)
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

    #[test]
    fn non_latin_layouts_use_the_physical_key() {
        // A Cyrillic layout reports "ы" for the S key; Ctrl+S (Cmd+S) must still save.
        assert_eq!(press("ы", Code::KeyS, Modifiers::COMMAND), Some(Shortcut::Save));
    }
}
