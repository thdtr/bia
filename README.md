<div align="center">
  <img src="assets/logo.svg" width="150" alt="bia">
  <p>A happy minimalist editor.</p>
  <p><a href="README.pt-BR.md">Português</a> · <a href="docs/ARCHITECTURE.md">Architecture</a> · <a href="CONTRIBUTING.md">Contributing</a></p>
</div>

BIA is a small code editor written in Rust. File tree, syntax highlighting, Markdown
preview, themes and one settings file. That's it.

![BIA editing one of its own source files](docs/screenshot.png)

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

You can also grab a build from [releases](https://github.com/thdtr/bia/releases) or run
`cargo install --git https://github.com/thdtr/bia bia`. To uninstall, add `--uninstall`
(`... | sh -s -- --uninstall`). On Windows, set `$env:BIA_UNINSTALL=1` and run the
install command again.

## Features

- File tree with one folder per line
- Highlighting for ~200 languages, plus any `.sublime-syntax` file you add ([how](docs/LANGUAGES.md))
- Line numbers, tabs, find, undo and redo
- Live Markdown preview
- Dark and light themes, or your own in JSON ([how](docs/THEMES.md))
- English and Portuguese
- Safe saves: atomic writes, line endings kept, deleted files go to the trash

## No network code

BIA never connects to the internet. There's no telemetry and no update check, and CI
fails if network code ever gets into the project ([details](docs/SEALED.md)).

## Shortcuts

| Keys | Action |
|---|---|
| `Ctrl+O` | Open folder |
| `Ctrl+S` | Save |
| `Ctrl+W` | Close tab |
| `Ctrl+F` / `F3` | Find / next match |
| `Ctrl+Z` / `Ctrl+Shift+Z` | Undo / redo |
| `Ctrl+Shift+V` | Markdown preview |
| `Ctrl+B` | Toggle sidebar |
| `Ctrl+N` / `Ctrl+Shift+N` | New file / folder |
| `F2` / `Delete` | Rename / move to trash |
| `Ctrl+,` | Settings |

On a Mac, use `Cmd`.

## Settings

`Ctrl+,` opens `settings.json`. Changes apply when you save.

```json
{
  "font_family": "Fira Code",
  "font_size": 14,
  "line_height": 1.55,
  "theme": "noite",
  "language": "auto",
  "hidden": [".git", "node_modules", "target"]
}
```

BIA ships with Fira Sans and Fira Code, but any installed font works.
`theme` is `noite`, `dia` or one of yours. `language` is `auto`, `en` or `pt-BR`.
`hidden` lists names the file tree skips.

## License

MIT or Apache 2.0, your choice. The bundled Fira fonts are under the
[SIL Open Font License](assets/fonts/OFL-FiraSans.txt).
