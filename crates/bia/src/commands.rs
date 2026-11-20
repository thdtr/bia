//! Everything you can ask BIA to do by name. The command palette
//! (Ctrl+Shift+P) lists them all, the menu button shows the common ones,
//! and the shortcuts list is built from the same table, so a name or a key
//! is written down once.

/// A command, with its name (a translation key) and its keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    NewFile,
    NewWindow,
    OpenFile,
    OpenFolder,
    OpenRecent,
    Save,
    SaveAs,
    SaveAll,
    CloseTab,
    ReopenTab,
    CloseFolder,
    QuickOpen,
    GoToLine,
    Find,
    Replace,
    FindInFiles,
    Sidebar,
    Terminal,
    NewTerminal,
    Split,
    Preview,
    ZoomIn,
    ZoomOut,
    ZoomReset,
    CollapseTree,
    Settings,
    Commands,
    Shortcuts,
    About,
    Quit,
}

impl Command {
    pub const ALL: [Command; 30] = [
        Command::NewFile,
        Command::NewWindow,
        Command::OpenFile,
        Command::OpenFolder,
        Command::OpenRecent,
        Command::Save,
        Command::SaveAs,
        Command::SaveAll,
        Command::CloseTab,
        Command::ReopenTab,
        Command::CloseFolder,
        Command::QuickOpen,
        Command::GoToLine,
        Command::Find,
        Command::Replace,
        Command::FindInFiles,
        Command::Sidebar,
        Command::Terminal,
        Command::NewTerminal,
        Command::Split,
        Command::Preview,
        Command::ZoomIn,
        Command::ZoomOut,
        Command::ZoomReset,
        Command::CollapseTree,
        Command::Settings,
        Command::Commands,
        Command::Shortcuts,
        Command::About,
        Command::Quit,
    ];

    /// What the menu button shows, `None` standing for a separator.
    pub const MENU: [Option<Command>; 22] = [
        Some(Command::NewFile),
        Some(Command::NewWindow),
        Some(Command::OpenFile),
        Some(Command::OpenFolder),
        Some(Command::OpenRecent),
        None,
        Some(Command::Save),
        Some(Command::SaveAs),
        Some(Command::SaveAll),
        None,
        Some(Command::CloseTab),
        Some(Command::CloseFolder),
        None,
        Some(Command::Commands),
        Some(Command::Sidebar),
        Some(Command::Terminal),
        None,
        Some(Command::Settings),
        Some(Command::Shortcuts),
        Some(Command::About),
        None,
        Some(Command::Quit),
    ];

    /// The translation key of its name.
    pub fn label(self) -> &'static str {
        match self {
            Command::NewFile => "cmd.new_file",
            Command::NewWindow => "cmd.new_window",
            Command::OpenFile => "cmd.open_file",
            Command::OpenFolder => "cmd.open_folder",
            Command::OpenRecent => "cmd.open_recent",
            Command::Save => "cmd.save",
            Command::SaveAs => "cmd.save_as",
            Command::SaveAll => "cmd.save_all",
            Command::CloseTab => "cmd.close_tab",
            Command::ReopenTab => "cmd.reopen_tab",
            Command::CloseFolder => "cmd.close_folder",
            Command::QuickOpen => "cmd.quick_open",
            Command::GoToLine => "cmd.go_to_line",
            Command::Find => "cmd.find",
            Command::Replace => "cmd.replace",
            Command::FindInFiles => "cmd.find_in_files",
            Command::Sidebar => "cmd.sidebar",
            Command::Terminal => "cmd.terminal",
            Command::NewTerminal => "cmd.new_terminal",
            Command::Split => "cmd.split",
            Command::Preview => "cmd.preview",
            Command::ZoomIn => "cmd.zoom_in",
            Command::ZoomOut => "cmd.zoom_out",
            Command::ZoomReset => "cmd.zoom_reset",
            Command::CollapseTree => "cmd.collapse_tree",
            Command::Settings => "cmd.settings",
            Command::Commands => "cmd.commands",
            Command::Shortcuts => "cmd.shortcuts",
            Command::About => "cmd.about",
            Command::Quit => "cmd.quit",
        }
    }

    /// Its keys, written for Linux and Windows (`keys::label` turns them
    /// into the Mac's).
    pub fn keys(self) -> Option<&'static str> {
        Some(match self {
            Command::NewFile => "Ctrl+N",
            Command::NewWindow => "Ctrl+Shift+N",
            Command::OpenFolder => "Ctrl+O",
            Command::Save => "Ctrl+S",
            Command::SaveAs => "Ctrl+Shift+S",
            Command::CloseTab => "Ctrl+W",
            Command::ReopenTab => "Ctrl+Shift+T",
            Command::QuickOpen => "Ctrl+P",
            Command::GoToLine => "Ctrl+G",
            Command::Find => "Ctrl+F",
            Command::Replace => {
                if cfg!(target_os = "macos") {
                    "Ctrl+Alt+F"
                } else {
                    "Ctrl+H"
                }
            }
            Command::FindInFiles => "Ctrl+Shift+F",
            Command::Sidebar => "Ctrl+B",
            Command::Terminal => "Ctrl+`",
            Command::NewTerminal => "Ctrl+Shift+`",
            Command::Split => "Ctrl+\\",
            Command::Preview => "Ctrl+Shift+V",
            Command::ZoomIn => "Ctrl+=",
            Command::ZoomOut => "Ctrl+-",
            Command::ZoomReset => "Ctrl+0",
            Command::Settings => "Ctrl+,",
            Command::Commands => "Ctrl+Shift+P",
            Command::Quit => "Ctrl+Q",
            Command::OpenFile
            | Command::OpenRecent
            | Command::SaveAll
            | Command::CloseFolder
            | Command::CollapseTree
            | Command::Shortcuts
            | Command::About => return None,
        })
    }
}

/// Keys that work in the editor, for the shortcuts list. They aren't
/// commands: they act where the cursors are.
pub const EDITOR_KEYS: [(&str, &str); 16] = [
    ("Ctrl+Z", "keys.undo"),
    ("Ctrl+Shift+Z", "keys.redo"),
    ("Ctrl+/", "keys.comment"),
    ("Alt+↑ / Alt+↓", "keys.move_lines"),
    ("Shift+Alt+↑ / Shift+Alt+↓", "keys.duplicate_lines"),
    ("Ctrl+Shift+K", "keys.delete_lines"),
    ("Ctrl+L", "keys.select_line"),
    ("Ctrl+D", "keys.next_occurrence"),
    ("Ctrl+Shift+L", "keys.all_occurrences"),
    ("Ctrl+Alt+↑ / Ctrl+Alt+↓", "keys.add_cursor"),
    ("Alt+click", "keys.toggle_cursor"),
    ("F12", "keys.definition"),
    ("Ctrl+Space", "keys.complete"),
    ("Shift+Alt+F", "keys.format"),
    ("F2", "keys.rename_symbol"),
    ("F8 / Shift+F8", "keys.problems"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_command_is_listed_once() {
        for command in Command::ALL {
            assert_eq!(Command::ALL.iter().filter(|c| **c == command).count(), 1);
        }
        for command in Command::MENU.into_iter().flatten() {
            assert!(Command::ALL.contains(&command));
        }
    }

    #[test]
    fn every_name_is_translated() {
        let en: serde_json::Value =
            serde_json::from_str(include_str!("../../../assets/locales/en.json")).unwrap();
        let pt: serde_json::Value =
            serde_json::from_str(include_str!("../../../assets/locales/pt-BR.json")).unwrap();
        let keys = Command::ALL.map(Command::label).into_iter();
        for key in keys.chain(EDITOR_KEYS.map(|(_, k)| k)) {
            assert!(en.get(key).is_some(), "en.json lacks {key}");
            assert!(pt.get(key).is_some(), "pt-BR.json lacks {key}");
        }
    }
}
