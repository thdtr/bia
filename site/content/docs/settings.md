+++
title = "Settings"
description = "The settings.json file: where it is, what each setting does and its default value."
weight = 4
+++

All of BIA's settings are kept in a single file, `settings.json`. There is no settings screen. <kbd>Ctrl+,</kbd> (<kbd>⌘,</kbd> on macOS) opens the file in the editor, and the changes take effect when it is saved.

## Location

| System | Configuration folder |
|---|---|
| Linux | `~/.config/bia/` (or `$XDG_CONFIG_HOME/bia/`) |
| macOS | `~/Library/Application Support/bia/` |
| Windows | `%APPDATA%\bia\` |

The `BIA_CONFIG_DIR` environment variable replaces this folder. It is useful for tests and portable installations.

The folder contains:

```text
bia/
├── settings.json       your settings
├── state.json          written by BIA: folders opened and their open files
├── backups/            written by BIA: unsaved changes, until saved or discarded
├── history/            written by BIA: the last 30 saved versions of each file
├── themes/             custom themes (*.json)
└── languages/          additional grammars (*.sublime-syntax)
```

If `settings.json` does not exist, BIA creates it with the default values.

## Example

```json
{
  "font_family": "Fira Code",
  "font_size": 14,
  "line_height": 1.55,
  "tab_size": 4,
  "ui_font_family": "Fira Sans",
  "ui_font_size": 13,
  "theme": "noite",
  "language": "auto",
  "hidden": [".git", "node_modules", "target"],
  "auto_close": true,
  "soft_wrap": "off",
  "rulers": [100],
  "show_whitespace": "selection",
  "trim_trailing_whitespace_on_save": true,
  "languages": {
    "Python": { "rulers": [88] },
    "Go": { "insert_spaces": false }
  },
  "language_servers": {
    "Rust": ["rust-analyzer"]
  }
}
```

"Default settings", in the menu or with <kbd>Ctrl+Shift+P</kbd>, opens every setting with its default value, to read next to your own.

A setting that is omitted keeps its default value.

## Reference

| Setting | Default | Description |
|---|---|---|
| `font_family` | `"Fira Code"` | Font of the editor. Any installed font can be used. |
| `font_size` | `14` | Size of the editor font, from 6 to 72. |
| `line_height` | `1.55` | Line height in the editor, from 1.0 to 3.0. |
| `tab_size` | `4` | How many columns a tab takes on screen, from 1 to 8, and the indentation width for files that have no indentation yet. Files that already have indentation keep their own. |
| `ui_font_family` | `"Fira Sans"` | Font of the interface. |
| `ui_font_size` | `13` | Size of the interface font, from 8 to 32. |
| `theme` | `"noite"` | `noite`, `dia` or the name of a custom theme. See [Themes](@/docs/themes.md). |
| `language` | `"auto"` | Interface language: `auto` (follows the system), `en` or `pt-BR`. |
| `hidden` | see below | Names that the file tree does not show. |
| `auto_close` | `true` | Typing `(`, `[`, `{` or a quote also types the closing half. |
| `soft_wrap` | `"off"` | Where long lines wrap instead of scrolling sideways. `"edge"` wraps at the edge of the editor, `"ruler"` at the first ruler (or the edge, if that comes first), and `"off"` doesn't wrap. `true` and `false` still work. |
| `rulers` | `[]` | Columns with a thin line down the editor, such as `[80, 120]`. |
| `indent_guides` | `true` | Thin lines down each level of indentation. |
| `show_whitespace` | `"selection"` | Where spaces show as dots and tabs as dashes. `"none"`, `"selection"` or `"all"`. |
| `line_numbers` | `true` | Line numbers beside the text. |
| `relative_line_numbers` | `false` | Numbers count from the cursor's line, which helps with moves like `5↓`. |
| `cursor_blink` | `true` | The cursor blinks for a few seconds after each key. |
| `insert_spaces` | `true` | Files with no indentation yet are indented with spaces. `false` uses tabs. |
| `autosave` | `"off"` | Saves on its own. `"focus_change"` saves when the window loses the focus, `"after_delay"` two seconds after you stop typing. Files never saved are left for you to name. |
| `format_on_save` | `false` | Asks the language server to format the file before saving. |
| `trim_trailing_whitespace_on_save` | `false` | Removes the spaces and tabs at the end of every line when saving. |
| `ensure_final_newline_on_save` | `false` | Adds a line break at the end of the file when saving, if it has none. Line breaks that are already there stay. |
| `restore_on_startup` | `true` | Opens the last folder when BIA starts. |
| `file_types` | `{}` | Which language a file is, by name or `*.ext`, when BIA guesses wrong, such as `{ "*.env.local": "Bourne Again Shell (bash)" }`. The language name is the one in the status bar. |
| `languages` | `{}` | Settings for one language, by its name in the status bar. See below. |
| `terminal` | `{}` | The shell and font of the terminal. See below. |
| `language_servers` | `{}` | The language server for each language, as a command: program, then arguments. The key is the language name shown in the status bar. See [Language servers](@/docs/language-servers.md). |

The default value of `hidden` is `[".git", ".hg", ".svn", ".DS_Store", "Thumbs.db", "node_modules", "target"]`.

Values outside the accepted range are adjusted to the nearest limit. Fira Sans and Fira Code are bundled with BIA and look the same on every system.

## One language at a time

`languages` changes some settings for the files of one language, and leaves the rest as they are. It takes `tab_size`, `insert_spaces`, `rulers`, `soft_wrap`, `format_on_save`, `trim_trailing_whitespace_on_save` and `ensure_final_newline_on_save`.

```json
"languages": {
  "Python": { "rulers": [88], "format_on_save": true },
  "Makefile": { "insert_spaces": false }
}
```

## The terminal

```json
"terminal": {
  "shell": ["fish"],
  "font_family": "JetBrains Mono",
  "font_size": 13
}
```

`shell` is the program and its arguments. Left out, the terminal runs your login shell, in the editor's font and size.

## .editorconfig

When a project has `.editorconfig` files, BIA follows them over these settings for the files they cover. It reads `indent_style`, `indent_size`, `tab_width`, `trim_trailing_whitespace` and `insert_final_newline`.

## Errors

If `settings.json` contains invalid JSON, BIA keeps running with the default values and reports the error in the status bar. Correcting the file and saving it again applies the settings. A key BIA doesn't know, often a typo, is pointed out in the status bar too.
