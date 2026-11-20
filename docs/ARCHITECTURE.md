# Architecture

BIA is two Rust crates, a folder of assets and two scripts that generate some of them.

```text
bia/
├── crates/
│   ├── bia-core/        no GUI: settings, themes, i18n, tree, documents, languages,
│   │   │                file operations (fs_ops), indentation, fuzzy match, search,
│   │   │                editing rules (comments, pairs), backups, state
│   │   ├── src/editor/  the text model: rope, cursors, commands, undo, display rows
│   │   └── tests/sealed.rs   fails the build if network code appears
│   └── bia/             the iced application
│       ├── build.rs        embeds the icon in bia.exe (Windows only)
│       └── src/
│           ├── app.rs          state + update loop; orchestrates the modules below
│           ├── tabs.rs         open files, the active one, rename/delete bookkeeping
│           ├── buffer.rs       one open file: its editor, save metadata, caches
│           ├── code_view/      the editor widget: draws an editor, sends input
│           ├── syntax.rs       syntect highlighting, line by line, cached
│           ├── icons.rs        which icon each file gets (name, then extension)
│           ├── keys.rs         every keyboard shortcut, in one table
│           ├── fonts.rs        bundled fonts; font names from settings → iced fonts
│           └── ui/
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
│   ├── third_party_licenses.sh   licenses of everything the binary includes
│   └── measure.sh       memory and CPU per key, GPU and CPU renderers, old vs new
├── about.toml, about.hbs    cargo-about settings for third_party_licenses.sh
└── deny.toml            the dependency-level half of the "sealed" guarantee
```

Anything that can be tested without a window goes in `bia-core` (file I/O, the
editor model, file operations) or in a plain module of the app (`tabs.rs`,
`syntax.rs`, `icons.rs`, `keys.rs`). All of these have unit tests. `app.rs` just
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
| GUI | **[iced](https://iced.rs) 0.14** | Written in Rust, runs on Linux, macOS and Windows (GPU rendering with a CPU fallback), has a Markdown widget, and lets a custom widget lay out and draw text itself, which BIA's editor does. We also looked at GPUI (used by Zed and GRAM), which is faster but has no stable API and no editor outside Zed; egui, which redraws everything every frame; and Tauri, which needs a browser engine. |
| Text of open files | ropey | A rope: an edit costs the same anywhere in a 16 MB file. Built without `unicode_lines`, so only `\n` breaks a line. |
| Graphemes, widths | unicode-segmentation, unicode-width | The cursor steps over `é` or 👍🏽 at once, and CJK characters take two columns. |
| Text rendering | cosmic-text (through iced) | Unicode, ligatures, emoji and font fallback. |
| Fonts | **Fira Sans** and **Fira Code**, bundled | One family, so BIA looks the same on every machine. Similar in style to GRAM's defaults. SIL Open Font License, about 2.5 MB of the binary. |
| Syntax highlighting | **syntect** + **two-face** | Uses Sublime Text grammars. About 200 come built in (the set `bat` maintains), and users can add more by dropping a `.sublime-syntax` file in a folder. Tree-sitter grammars are C code that has to be compiled in, so they don't allow that. syntect is built with `regex-fancy`, so there's no C toolchain involved. |
| Markdown | pulldown-cmark (through iced) | CommonMark with GitHub's tables, task lists and strikethrough. |
| Settings, themes, translations | serde + serde_json | `settings.json` was a given, so everything else is JSON too. |
| System language | sys-locale | macOS apps don't get `$LANG`, and Windows has no equivalent. |
| Folder picker | rfd | Native dialogs. On Linux it talks to the XDG desktop portal instead of linking GTK, and runs `zenity` if there's no portal. |
| Deleting files | trash | Files deleted from the tree go to the OS trash. |
| Search | regex | Find, replace and find in files. Plain text goes through it too, escaped, for Unicode case folding. Its engine was already in the build through syntect. |
| Walking a folder | ignore | From ripgrep. Applies `.gitignore`, `.ignore` and the global excludes for `Ctrl+P` and find in files. |
| Terminal | alacritty_terminal | The terminal core of Alacritty: the pseudo-terminal on Linux, macOS and Windows, escape sequences, the grid and scrollback. BIA draws the grid itself, one cached `Paragraph` per run of cells with the same look, placed at its column. |
| Git marks | similar, and the `git` program | `git cat-file` gives a file's committed text, `similar` diffs it line by line. A git library would add dozens of crates; the `git` you already have reads the repository the same way. See [SEALED.md](SEALED.md) for how it's called. |

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

* **Editing.** `code_view` turns keys and the mouse into `Input` messages; it never
  changes the text itself. `Buffer::input` hands them to `bia_core::editor::Editor`,
  where every command runs at every cursor as one transaction (edits sorted, none
  overlapping). Undo keeps the inverse transactions and the selections, grouped by
  word, and knows when the text is back to how it was saved.
* **Drawing.** The widget lays out only the rows on screen, one `Paragraph` each,
  cached by content. `DisplayMap` turns lines into rows: one per line, or several when
  `soft_wrap` is on. It lives in the buffer, so Up and Down move by rows too, and
  after an edit it measures again only the lines the editor's change log names. Tabs become spaces up to the next stop of `tab_size`.
  Selections, the current line, search matches and the bracket pair are rectangles
  under the text; cursors are drawn over it. The widget scrolls by itself, follows
  the cursor and remembers each tab's place.
  The view is rebuilt after every message, so the file tree, which can have
  thousands of rows, only builds the ones in sight (`ui/explorer.rs`).
* **Highlighting.** `syntax.rs` keeps syntect's state every 16 lines, and the styles of
  lines already drawn with the state each one started from. An edit sets aside what
  comes after the changed lines (the editor logs them), moved to where it is now;
  parsing takes it back from the first line that starts in the same state as before,
  so typing inside a line reparses a line or two. Drawing parses for 12 ms at most
  per frame; until it catches up,
  lines on screen are parsed from their own start. A jump more than 5,000 lines past
  what was parsed starts parsing 200 lines above the screen instead.
* **What follows the text.** Find matches, the Markdown preview and the git marks
  are brought up to date after every message (`Bia::follow_edits`). Up to 256 KB
  that happens at once; bigger files wait until typing pauses for 150 ms, and the
  diff and the search run on a copy of the rope in the background (a rope clone
  shares its contents, so the copy is free). Meanwhile the git marks move with the
  lines added or removed above them. Backups are written by a thread of their own,
  in order, and `Ctrl+P` lists the folder in the background and keeps the list
  until a file operation or the window getting focus back.
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
| Undo and redo, grouped by word | Restores the selections too. Keeps the last 5,000 steps, or fewer when they hold more than 128 MB of text. |
| Several cursors | `Ctrl+D`, `Ctrl+Shift+L`, `Ctrl+Alt+↑/↓`, Alt+click. Every command works with all of them. |
| Find and replace (`Ctrl+F`, `Ctrl+H`) | You need to search inside a file, and renaming by hand is slow. |
| Indentation that follows the file | BIA detects tabs or spaces and the width, keeps the indent on Enter and adds a level after `{`, `[` or `(`. |
| Open file by name (`Ctrl+P`) | In a real project, the tree alone is too slow. |
| Go to line (`Ctrl+G`, `bia file:line`) | Compilers, stack traces and CI logs point to lines. |
| Tabs | You usually work on more than one file. |
| Unsaved changes warnings | A dot on the tab, and a question before closing or quitting. |
| Atomic saves, line endings and BOM kept | Saving never corrupts a file or turns every line into a diff. |
| Binary, non-UTF-8, CR-only and >16 MB files refused | Opening them as text would corrupt them or freeze the editor. |
| Outside changes noticed on focus and on save | After a `git checkout`, you don't overwrite the new version with an old one. |
| Create, rename, move, copy and trash from the tree | Organizing files means more than looking at them. Names are typed in the tree itself, a right click opens a menu (`ui/menu.rs` reports where the click was, which `mouse_area` doesn't), and rows can be dragged onto folders. `fs_ops` never replaces anything: a copy gets a free name, and a move onto a taken name fails. |
| `bia <path>` on the command line | That's how most developers open an editor. |
| Reopen the last folder | Opened from the app menu, BIA would otherwise start empty every time. The folder is kept in `state.json`, next to `settings.json`, so BIA never rewrites the file you edit. |
| A test for missing translations | A missing string fails CI instead of showing up as a key. |
| Find options and find in files | Searching a project means case, whole words and regex, in every file. |
| Comment, move, duplicate, delete lines; closing pairs | What hands expect in any code editor. Comment tokens are a table in `editing.rs`, since the built-in grammars don't carry them. |
| Reopen the open files | `state.json` keeps each folder's tabs and cursors. |
| Language servers | Errors and warnings underlined as you type, hover cards, completion, go to definition, format, rename, next problem. `bia-core/lsp` speaks JSON-RPC over the server's stdio, reading on one thread and writing on another, so a busy server never blocks the window. The app keeps one server per language and sends the edits 150 ms after typing stops: just the changed ranges when the server takes them (the editor keeps a journal of recent edits, `editor/splice.rs`), the whole text otherwise. Positions are UTF-8 when the server agrees, UTF-16 otherwise. Servers are named in `language_servers`, never found or downloaded. |
| Terminal (`` Ctrl+` ``) | Shells in tabs, in a panel under the editor (not under the sidebar), starting in the open folder. They keep running while hidden. `Ctrl+Shift+C` and `Ctrl+Shift+V` copy and paste, and `Ctrl+C` goes to the program. Ctrl+click on a location the terminal printed opens the file there. On Linux and macOS, BIA reads the shell's foreground process group to name the tab after the running program and to ask before closing one that's busy. |
| Two sides (`Ctrl+\`) | Tabs keep one list, each tab on the left or right side, and each side has its own active tab (`tabs.rs`). A file is open on one side at a time, so there's never a second buffer of the same file to keep in sync. The Markdown preview is a tab on the other side that reads its file's buffer. |
| Sizes you drag | The sidebar's width and the terminal's height live in `state.json`, not in the settings. |
| Git: changed lines, changed files, the branch | Marks beside the line numbers, colors in the tree, the branch in the status bar. Refreshed on save and when the window gets focus back. |
| Backups of unsaved edits | Written to `backups/` two seconds after the last edit, removed on save or discard. A crash loses at most those two seconds. |

## Known limitations

* **Mixed line endings aren't kept.** A file that mixes LF and CRLF is saved with the
  more common one. The status bar says so before you save.
* **Bare CR line breaks aren't supported.** These files aren't opened.
* **Find in files doesn't replace yet.** Results open the file with the match
  selected, where `Ctrl+H` works.
* **Servers that only take whole files get the whole file** 150 ms after each pause in
  typing. So does any server after more than 256 edits or 1 MB of typed text without
  a pause.
* **Input methods are untested.** Composed characters and IMEs for languages such as
  Japanese go through iced's input method support, but no one has tried them yet.
* **Highlighting far into a long file can be wrong** inside a comment or string that
  started more than 5,000 lines above (see *Highlighting* above).

## Not in BIA yet

There's no debugger, plugins or AI in BIA today. Some
of them may come. Each one adds a lot to a small codebase (LSP alone would roughly
double it), so open an issue to talk it through before writing code.

Telemetry, auto-update, accounts and sync are a different case. They need the network,
and BIA has no network code ([SEALED.md](SEALED.md)).
