//! Key combinations as people write them in `keybindings`, like
//! `Ctrl+Shift+D`, `Alt+Up` or `F5`.
//!
//! `Ctrl` is the key BIA's own shortcuts use, which is Cmd on a Mac, so a
//! binding written once works on every system. Names are matched without
//! regard to case.

/// A key with its modifiers.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Combo {
    /// Ctrl, or Cmd on a Mac.
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    /// The key: one character (`d`, `/`, `` ` ``) in lower case, or a name
    /// (`up`, `enter`, `f5`).
    pub key: String,
}

/// Names of the keys that aren't characters.
pub const NAMED: [&str; 28] = [
    "up",
    "down",
    "left",
    "right",
    "home",
    "end",
    "pageup",
    "pagedown",
    "enter",
    "tab",
    "escape",
    "backspace",
    "delete",
    "insert",
    "space",
    "f1",
    "f2",
    "f3",
    "f4",
    "f5",
    "f6",
    "f7",
    "f8",
    "f9",
    "f10",
    "f11",
    "f12",
    "menu",
];

impl Combo {
    /// Reads `Ctrl+Shift+D`. `None` when it isn't a key combination.
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        // `Ctrl++` is the + key.
        let (mods, key) = match text.strip_suffix("++") {
            Some(mods) => (mods, "+"),
            None => match text.rsplit_once('+') {
                Some((mods, key)) => (mods, key),
                None => ("", text),
            },
        };
        let mut combo = Combo { ctrl: false, alt: false, shift: false, key: String::new() };
        for part in mods.split('+').map(str::trim).filter(|p| !p.is_empty()) {
            match part.to_ascii_lowercase().as_str() {
                "ctrl" | "control" | "cmd" | "command" | "super" | "meta" => combo.ctrl = true,
                "alt" | "option" | "opt" => combo.alt = true,
                "shift" => combo.shift = true,
                _ => return None,
            }
        }
        let key = key.trim().to_lowercase();
        let key = match key.as_str() {
            "esc" => "escape".to_owned(),
            "return" => "enter".to_owned(),
            "del" => "delete".to_owned(),
            "pgup" => "pageup".to_owned(),
            "pgdown" | "pgdn" => "pagedown".to_owned(),
            "↑" => "up".to_owned(),
            "↓" => "down".to_owned(),
            "←" => "left".to_owned(),
            "→" => "right".to_owned(),
            _ => key,
        };
        let one_char = key.chars().count() == 1;
        if !one_char && !NAMED.contains(&key.as_str()) {
            return None;
        }
        combo.key = key;
        Some(combo)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn combo(ctrl: bool, alt: bool, shift: bool, key: &str) -> Combo {
        Combo { ctrl, alt, shift, key: key.to_owned() }
    }

    #[test]
    fn reads_what_people_write() {
        assert_eq!(Combo::parse("Ctrl+Shift+D"), Some(combo(true, false, true, "d")));
        assert_eq!(Combo::parse("cmd+alt+up"), Some(combo(true, true, false, "up")));
        assert_eq!(Combo::parse("F5"), Some(combo(false, false, false, "f5")));
        assert_eq!(Combo::parse("Ctrl++"), Some(combo(true, false, false, "+")));
        assert_eq!(Combo::parse("Ctrl+`"), Some(combo(true, false, false, "`")));
        assert_eq!(Combo::parse("Alt+←"), Some(combo(false, true, false, "left")));
        assert_eq!(Combo::parse("Ctrl+Esc"), Some(combo(true, false, false, "escape")));
        assert_eq!(Combo::parse("Hyper+X"), None);
        assert_eq!(Combo::parse("Ctrl+Banana"), None);
    }
}
