<div align="center">
  <img src="assets/logo.svg" width="150" alt="bia">
  <p>A happy minimalist editor.</p>
  <p><a href="README.pt-BR.md">Português</a> · <a href="docs/ARCHITECTURE.md">Architecture</a> · <a href="CONTRIBUTING.md">Contributing</a></p>
  <p>
    <a href="https://github.com/thdtr/bia/actions/workflows/ci.yml"><img src="https://github.com/thdtr/bia/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
    <a href="https://github.com/thdtr/bia/releases/latest"><img src="https://img.shields.io/github/v/release/thdtr/bia" alt="Release"></a>
    <a href="#license"><img src="https://img.shields.io/badge/license-MIT-blue" alt="License"></a>
  </p>
</div>

BIA is a small code editor for Linux, macOS and Windows, written in Rust. File tree,
syntax highlighting, Markdown preview, themes and one settings file. That's it.

![BIA on macOS, with a Next.js project open](docs/screenshot.png)

## Install

Linux and macOS:

```sh
curl -fsSL https://raw.githubusercontent.com/thdtr/bia/main/install.sh | sh
```

Windows (PowerShell):

```powershell
irm https://raw.githubusercontent.com/thdtr/bia/main/install.ps1 | iex
```

No admin rights needed. Open BIA from your app menu, or run `bia ~/code/project`.
`bia src/main.rs:42` opens a file at line 42. Run with no arguments, BIA reopens the
last folder you opened.

You can also grab a build from [releases](https://github.com/thdtr/bia/releases) or run
`cargo install --git https://github.com/thdtr/bia bia`. To uninstall, add `--uninstall`
(`... | sh -s -- --uninstall`). On Windows, set `$env:BIA_UNINSTALL=1` and run the
install command again.

To update, run the install command again. Your settings stay as they are.

### Warning on first launch

BIA builds aren't signed with a paid certificate. If you download one from the
releases page, your system may warn you the first time you open it:

- **Windows:** click *More info*, then *Run anyway*.
- **macOS:** go to *System Settings → Privacy & Security* and click *Open Anyway*.

The install commands above don't trigger this warning.

## Features

- File tree with one folder per line
- Highlighting for ~200 languages, plus any `.sublime-syntax` file you add ([how](docs/LANGUAGES.md))
- Line numbers, tabs, undo and redo
- Find and replace, with match case, whole word and regex
- Find in files, skipping what `.gitignore` lists
- Open a file by name, go to a line
- Several cursors, in every command
- Comment, move, duplicate and delete lines; brackets and quotes close themselves
- Files of any length open at once, line numbers included
- Indentation that follows each file: tabs or spaces, and how many
- Live Markdown preview, in a tab beside the file
- Two files side by side
- Dark and light themes, or your own in JSON ([how](docs/THEMES.md))
- English and Portuguese
- Safe saves: atomic writes, line endings kept, deleted files go to the trash
- Reopens the files you left open, and brings back unsaved edits after a crash
- Git: changed lines beside the line numbers, changed files in the tree, the branch
- Terminals under the editor, in tabs. Ctrl+click on `file:line` opens it
- Language servers you choose: errors as you type, completion, go to definition,
  hover, format, rename

## No network code

BIA never connects to the internet. There's no telemetry and no update check, and CI
fails if network code ever gets into the project ([details](docs/SEALED.md)).

## Shortcuts

| Keys | Action |
|---|---|
| `Ctrl+O` | Open folder |
| `Ctrl+S` | Save |
| `Ctrl+W` | Close tab |
| `Ctrl+P` | Open file by name |
| `Ctrl+G` | Go to line |
| `Ctrl+F` / `F3` | Find / next match |
| `Ctrl+H` | Replace |
| `Ctrl+Shift+F` | Find in files |
| `Ctrl+/` | Comment or uncomment lines |
| `Alt+↑` / `Alt+↓` | Move lines |
| `Shift+Alt+↑` / `Shift+Alt+↓` | Duplicate lines |
| `Ctrl+Shift+K` | Delete lines |
| `Ctrl+L` | Select line (again for the next) |
| `Ctrl+D` | Select the next occurrence (adds a cursor) |
| `Ctrl+Shift+L` | Select every occurrence |
| `Ctrl+Alt+↑` / `Ctrl+Alt+↓` | Add a cursor above / below |
| `Alt+click` | Add or remove a cursor |
| `F12` / `Ctrl+click` | Go to definition (language server) |
| `Ctrl+Space` | Complete (language server) |
| `Shift+Alt+F` | Format the file (language server) |
| `F2` in the editor | Rename the symbol (language server) |
| `F8` / `Shift+F8` | Next / previous problem |
| `Ctrl+Z` / `Ctrl+Shift+Z` | Undo / redo |
| `Ctrl+Shift+V` | Markdown preview |
| `Ctrl+B` | Toggle sidebar |
| `Ctrl+N` / `Ctrl+Shift+N` | New file / folder |
| `F2` / `Delete` | Rename / move to trash |
| `Ctrl+=` / `Ctrl+-` / `Ctrl+0` | Bigger / smaller / reset font |
| `` Ctrl+` `` | Show / hide the terminal |
| `` Ctrl+Shift+` `` | New terminal |
| `Ctrl+\` | Move the tab to the other side |
| `Ctrl+,` | Settings |

On a Mac, use `Cmd` and `Option`. Replace is `Cmd+Alt+F`. `Ctrl+Space` and ``Ctrl+` `` stay `Ctrl`, since macOS takes `Cmd` for those.

## Settings

`Ctrl+,` opens `settings.json`. Changes apply when you save.

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
  "soft_wrap": false
}
```

`font_*` is for the editor and `ui_font_*` for the interface. BIA ships with Fira Sans
and Fira Code, but any installed font works.
`theme` is `noite`, `dia` or one of yours. `language` is `auto`, `en` or `pt-BR`.
`hidden` lists names the file tree skips.
`auto_close` types the closing bracket or quote for you.
`soft_wrap` wraps long lines at the edge of the window.

To use a language server, name its program per language, as the status bar shows
the language. BIA starts none on its own:

```json
"language_servers": {
  "Rust": ["rust-analyzer"],
  "Python": ["pyright-langserver", "--stdio"]
}
```
`tab_size` is how wide a tab looks, and the indent width for files with no
indentation yet. Files that have some keep their own.

## License

[MIT](LICENSE). The bundled Fira fonts are under the
[SIL Open Font License](assets/fonts/OFL-FiraSans.txt).
