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
├── state.json          written by BIA: the last folder opened
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
  "hidden": [".git", "node_modules", "target"]
}
```

A setting that is omitted keeps its default value.

## Reference

| Setting | Default | Description |
|---|---|---|
| `font_family` | `"Fira Code"` | Font of the editor. Any installed font can be used. |
| `font_size` | `14` | Size of the editor font, from 6 to 72. |
| `line_height` | `1.55` | Line height in the editor, from 1.0 to 3.0. |
| `tab_size` | `4` | Indentation width, from 1 to 8, for files that have no indentation yet. Files that already have indentation keep their own. |
| `ui_font_family` | `"Fira Sans"` | Font of the interface. |
| `ui_font_size` | `13` | Size of the interface font, from 8 to 32. |
| `theme` | `"noite"` | `noite`, `dia` or the name of a custom theme. See [Themes](@/docs/themes.md). |
| `language` | `"auto"` | Interface language: `auto` (follows the system), `en` or `pt-BR`. |
| `hidden` | see below | Names that the file tree does not show. |

The default value of `hidden` is `[".git", ".hg", ".svn", ".DS_Store", "Thumbs.db", "node_modules", "target"]`.

Values outside the accepted range are adjusted to the nearest limit. Fira Sans and Fira Code are bundled with BIA and look the same on every system.

## Errors

If `settings.json` contains invalid JSON, BIA keeps running with the default values and reports the error in the status bar. Correcting the file and saving it again applies the settings.
