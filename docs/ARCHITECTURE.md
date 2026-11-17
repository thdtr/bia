# Architecture

BIA is two Rust crates, a folder of assets and two scripts that generate some of them.

```text
bia/
├── crates/
│   ├── bia-core/        no GUI: settings, themes, i18n, tree, documents, history,
│   │   │                languages, file operations (fs_ops), indentation, fuzzy match
│   │   └── tests/sealed.rs   fails the build if network code appears
│   └── bia/             the iced application
│       ├── build.rs        embeds the icon in bia.exe (Windows only)
│       └── src/
│           ├── app.rs          state + update loop; orchestrates the modules below
│           ├── tabs.rs         open files, the active one, rename/delete bookkeeping
│           ├── buffer.rs       one open file: widget content, undo, save metadata
│           ├── highlighter.rs  syntect → iced incremental highlighting bridge
│           ├── icons.rs        which icon each file gets (name, then extension)
│           ├── keys.rs         every keyboard shortcut, in one table
│           ├── fonts.rs        bundled fonts; font names from settings → iced fonts
│           └── ui/
│               ├── layout.rs   editor geometry shared by the view and scrolling
│               ├── style.rs    every color decision
│               └── …           pure view functions
├── assets/
│   ├── fonts/           Fira Sans and Fira Code, with their OFL licenses
│   ├── icons/           app icon (svg/png/ico/rgba), logo template, explorer icons
│   ├── themes/          built-in themes (JSON), compiled into the binary
│   └── locales/         en.json, pt-BR.json, compiled into the binary
├── tools/
│   ├── make_icons.py    generates the explorer icons
│   ├── make_logo.py     generates the logo and every app icon format
│   ├── make_social_preview.py   the image shown when a link to BIA is shared
│   └── third_party_licenses.sh   licenses of everything the binary includes
├── about.toml, about.hbs    cargo-about settings for the script above
└── deny.toml            the dependency-level half of the "sealed" guarantee
```

Anything that can be tested without a window goes in `bia-core` (file I/O, undo,
text math, file operations) or in a plain module of the app (`tabs.rs`,
`ui/layout.rs`, `icons.rs`, `keys.rs`). All of these have unit tests. `app.rs` just
connects them to iced.

## Guidelines

1. **One way to do each thing.** One config file, one theme format, one shortcut per
   action.
2. **Keep the core free of GUI code.** `bia-core` builds in seconds and has its own
   tests. If we ever changed GUI toolkit, it wouldn't need to change.
3. **Customize with files, not code.** Themes and languages are files you drop in a
   folder, with no Rust and no rebuild. Translations are files too, but they're
   compiled in.
4. **Don't lose or damage files.** This is where BIA does more than the bare minimum.
5. **No network code.** See [SEALED.md](SEALED.md).

## Dependencies

| Need | Crate | Why |
|---|---|---|
| GUI | **[iced](https://iced.rs) 0.14** | Written in Rust, runs on Linux, macOS and Windows (GPU rendering with a CPU fallback), and comes with a multi-line text editor that supports incremental highlighting, plus a Markdown widget. We also looked at GPUI (used by Zed and GRAM), which is faster but has no stable API and no editor outside Zed; egui, which redraws everything every frame; and Tauri, which needs a browser engine. |
| Text rendering | cosmic-text (through iced) | Unicode, ligatures, emoji and font fallback. |
| Fonts | **Fira Sans** and **Fira Code**, bundled | One family, so BIA looks the same on every machine. Similar in style to GRAM's defaults. SIL Open Font License, about 2.5 MB of the binary. |
| Syntax highlighting | **syntect** + **two-face** | Uses Sublime Text grammars. About 200 come built in (the set `bat` maintains), and users can add more by dropping a `.sublime-syntax` file in a folder. Tree-sitter grammars are C code that has to be compiled in, so they don't allow that. syntect is built with `regex-fancy`, so there's no C toolchain involved. |
| Markdown | pulldown-cmark (through iced) | CommonMark with GitHub's tables, task lists and strikethrough. |
| Settings, themes, translations | serde + serde_json | `settings.json` was a given, so everything else is JSON too. |
| System language | sys-locale | macOS apps don't get `$LANG`, and Windows has no equivalent. |
| Folder picker | rfd | Native dialogs. On Linux it talks to the XDG desktop portal instead of linking GTK, and runs `zenity` if there's no portal. |
| Deleting files | trash | Files deleted from the tree go to the OS trash. |

Things we chose not to add: an async runtime (iced's thread pool is enough), a file
watcher (BIA refreshes when the window gets focus back, so it uses no CPU while you're
in another app), a crate for config folders (`paths.rs` is 15 lines) and an image
decoder (the window icon is stored as raw pixels and the logo is an SVG).

The `svg` feature of iced adds resvg, which draws the logo and the file tree icons.
The icons follow the same rules as GRAM's (16×16 grid, drawing in the central 10 px,
1.2 px strokes with round ends, one color), but they're our own, since GRAM's are GPL.
Kinds of files get a symbol and languages get a tile with two letters, so a new
language doesn't need a new drawing. `tools/make_icons.py` generates all of them.

## How data flows

```text
keyboard / mouse ──► iced ──► Message ──► Bia::update ──► state
                                                     │
                       ui::view(&Bia) ◄──────────────┘
```

* **Editing.** The `text_editor` widget sends `Action`s. `Buffer::perform` applies
  them, compares the text before and after (`history::Change::between`) and stores
  only the part that changed. Undo applies the reverse change through the widget, so
  the scroll position and highlighting stay as they were.
* **Highlighting.** `highlighter.rs` connects syntect to iced's `Highlighter` trait and
  saves the parser state every 64 lines. After an edit on line 9,000, parsing restarts
  at line 8,960 instead of line 0.
* **Saving.** `document::write_atomic` writes a temporary file next to the original,
  with the same permissions, flushes it to disk and renames it over the original. It
  follows symlinks. Files with more than one hard link, and files in folders where BIA
  can't create files, are written in place instead. If the file changed on disk since
  it was opened, BIA asks before overwriting it. Line endings (LF or CRLF) and a UTF-8
  BOM are written back the way they were read.
* **Settings.** Saving `settings.json`, or a file in `themes/` or `languages/`, from
  inside BIA reloads all of them. There's no settings screen; you edit the file.

## Beyond the basics

Besides the file tree, highlighting, Markdown preview, settings and themes, BIA has a
few things an editor can't really do without:

| Feature | Why |
|---|---|
| Undo and redo, grouped by word | iced's editor doesn't have undo. |
| Find and replace (`Ctrl+F`, `Ctrl+H`) | You need to search inside a file, and renaming by hand is slow. |
| Indentation that follows the file | iced's editor always indents with 8 spaces, and Enter goes back to column 0. BIA detects tabs or spaces and the width, keeps the indent on Enter and adds a level after `{`, `[` or `(`. |
| Open file by name (`Ctrl+P`) | In a real project, the tree alone is too slow. |
| Go to line (`Ctrl+G`, `bia file:line`) | Compilers, stack traces and CI logs point to lines. |
| Tabs | You usually work on more than one file. |
| Unsaved changes warnings | A dot on the tab, and a question before closing or quitting. |
| Atomic saves, line endings and BOM kept | Saving never corrupts a file or turns every line into a diff. |
| Binary, non-UTF-8, CR-only and >16 MB files refused | Opening them as text would corrupt them or freeze the editor. |
| Outside changes noticed on focus and on save | After a `git checkout`, you don't overwrite the new version with an old one. |
| Create, rename and trash from the tree | Organizing files means more than looking at them. |
| `bia <path>` on the command line | That's how most developers open an editor. |
| A test for missing translations | A missing string fails CI instead of showing up as a key. |

## Known limitations

* **Long lines don't wrap.** The line numbers and the text share one scroll area so
  they always line up, and iced's editor only supports that without wrapping. Long
  lines scroll sideways.
* **Very long files are slow to open.** iced lays out the whole file up front: about
  0.3 s for 3,000 lines and 0.8 s for 10,000. Files over 10,000 lines open without line
  numbers, which keeps scrolling and typing fast (`LARGE_FILE_LINES` in
  `ui/layout.rs`).
* **Mixed line endings aren't kept.** A file that mixes LF and CRLF is saved with the
  more common one. The status bar says so before you save.
* **Bare CR line breaks aren't supported.** The editor widget treats `\r` as a line
  break and BIA's copy of the text doesn't, so these files aren't opened.
* **Tabs are drawn 8 columns wide.** iced doesn't let BIA change that. `tab_size` only
  sets what the Tab key inserts.
* **Each keystroke compares the whole text.** That's a byte comparison of at most
  16 MB, which you won't notice, and it keeps the undo code simple.

## Out of scope

Terminal, git, debugger, LSP, plugins, AI, telemetry, auto-update, accounts and sync.
These either need the network (see [SEALED.md](SEALED.md)) or are better as separate
tools. LSP is the one we may revisit: it runs local programs, so it doesn't conflict
with having no network code, but it would roughly double the size of the code. It
needs a discussion in an issue before anyone writes code for it.
