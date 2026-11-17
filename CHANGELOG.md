# Changelog

Notable changes to BIA are listed here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Changed

- The editor and the terminal keep only the text drawn in the last two frames. Scrolling
  through a long file used about 50 MB less, and a terminal with a lot of output about
  110 MB less.
- Highlighting parses lines without their line break, which is 15–25% faster in some
  languages.
- In files over 256 KB, find matches, the Markdown preview and the git marks catch up
  when you pause typing, and the git diff and the search run in the background. In a
  45,000-line file, being in a git repository added about 19 ms of work to every
  keystroke; now it adds about 2.
- Highlighting no longer parses everything below the line you edit again. Lines below
  keep their colors unless the edit changes them, as opening a comment does. Typing in
  the middle of the screen in a 200,000-line file went from 20 ms of work per key to 13.
- The file tree builds only the rows in sight. With 3,000 rows open, a keystroke went
  from 28 ms of work to 7, the same as with a small tree, and the window uses about
  25 MB less.
- Backups are written in the background instead of while you type.
- With `soft_wrap` on, measuring where lines wrap is 1.7 to 4 times faster, which shows
  when resizing the window over a long file.
- The git branch and changed files come from one `git` call instead of three. Folders
  opened through a symlink now get their git colors in the tree.
- The cursor stops blinking 10 seconds after you stop typing, so an idle window isn't
  redrawn twice a second.
- Undo keeps fewer steps when they hold more than 128 MB of text, so a few "replace
  all" in a big file can't take memory without limit.
- Language servers get just the edited ranges when they support it (most do), instead
  of the whole file after every pause, and messages to them are written on a thread of
  their own, so a busy server can no longer freeze the window.
- `Ctrl+P` lists the folder in the background and remembers the list, so it no longer
  freezes the window in big folders. Ranking 50,000 paths takes 3 ms instead of 18.

### Fixed

- Language servers got the whole file every 150 ms while you typed, instead of once,
  150 ms after you stopped.
- With the cursor next to a bracket, each frame looked for the matching one once per
  line on screen instead of once.

## [0.3.0] - 2026-11-17

### Added

- Find in files (`Ctrl+Shift+F`), in the sidebar. Files your `.gitignore` lists are
  skipped, and open files are searched with their unsaved edits.
- Match case, whole word and regular expression options in find, replace and find in
  files. In regex mode, the replacement can use `$1`.
- Comment and uncomment lines with `Ctrl+/`.
- Move lines with `Alt+↑`/`Alt+↓`, duplicate them with `Shift+Alt+↑`/`↓`, delete
  them with `Ctrl+Shift+K` and select them with `Ctrl+L`.
- Brackets and quotes close themselves, and wrap the selection. Turn it off with
  `"auto_close": false`.
- `Ctrl+=`, `Ctrl+-` and `Ctrl+0` change the editor font size for the session.
- BIA reopens the files you had open in a folder, with the cursor where it was.
- Unsaved edits are kept in a backup until you save or discard them. After a crash,
  BIA opens the file with them.
- The welcome screen lists recent folders.
- Several cursors: `Ctrl+D` adds the next occurrence, `Ctrl+Shift+L` selects them
  all, `Ctrl+Alt+↑`/`↓` adds one above or below, Alt+click adds or removes one.
  Every command works with all of them.
- Find lights up every match, not just the current one.
- The current line and the bracket pair at the cursor are highlighted.
- Double click selects a word, triple click a line, and dragging after either goes
  by words or lines. Clicking a line number selects the line.
- `Ctrl+C` and `Ctrl+X` with nothing selected take the whole line, and pasting it
  puts it above the cursor's line.
- `Ctrl+Backspace` and `Ctrl+Delete` delete a word.
- A terminal panel (`` Ctrl+` `` or "Terminal" in the status bar) running your shell
  in the open folder, with colors, scrollback and full-screen programs.
  `Ctrl+Shift+C` and `Ctrl+Shift+V` copy and paste.
- Language servers, named per language in `"language_servers"` (BIA starts none
  on its own): errors and warnings underlined as you type, with counts in the
  status bar and the message under the cursor; a hover card when the mouse rests;
  completion as you type or with `Ctrl+Space`; go to definition with `F12` or
  `Ctrl+click`; format with `Shift+Alt+F`; rename with `F2`; `F8` to jump between
  problems. Servers start with the download switches of cargo, go, pip and npm
  turned off.
- Git: bars beside the line numbers show lines added or changed since the last
  commit, and a mark shows where lines were deleted. Changed and new files are
  colored in the tree, and the status bar shows the branch. BIA runs your `git` for
  this, only for reading, with hooks, fsmonitor and network transports turned off.
  Themes can set `added` and `modified` colors.
- `"soft_wrap": true` wraps long lines at the edge of the window. Up and Down then
  move by rows on screen.

### Changed

- BIA draws the editor itself instead of using iced's text editor. Files open at
  once whatever their length, with line numbers (they used to be turned off past
  10,000 lines), and typing no longer compares the whole text on every key.
- Tabs are drawn `tab_size` columns wide instead of always 8.
- Enter in the find field goes to the next match and stays in the field.
- Open file by name (`Ctrl+P`) skips what `.gitignore` lists.
- MIT is now the only license. Versions up to 0.2.0 remain available under MIT or
  Apache 2.0.

### Fixed

- With the CPU renderer (no GPU), text that changed to other text of the same width
  could stay on screen as it was. iced tells paragraphs apart without their text;
  BIA now gives each one bounds of its own.
- The Markdown preview showed indented code blocks as an empty box, with their text
  as a paragraph after it.
- Find ignored case only for ASCII letters: `É` didn't find `é`.
- A file that fits in the editor no longer shows a scrollbar or scrolls a little.

## [0.2.0] - 2026-11-17

### Added

- Indentation follows each file. BIA detects tabs or spaces and the width, keeps the
  indent when you press Enter and adds a level after `{`, `[` or `(`. Tab and
  Shift+Tab indent the selected lines, and Backspace in leading spaces removes one
  level. Files with no indentation yet use `tab_size` from the settings (tabs for Go
  and Makefiles). The status bar shows which one a file uses.
- Replace (`Ctrl+H`, or `Cmd+Alt+F` on macOS), one match at a time or all at once.
- Open a file by name with `Ctrl+P`. Add `:line` to jump to a line.
- Go to line with `Ctrl+G`, and `bia file:line:column` on the command line.
- With no arguments, BIA reopens the last folder you opened. It's saved in
  `state.json`, next to `settings.json`.

### Changed

- Syntax definitions updated to the set from `bat` 0.26 (two-face 0.5).
- File dialogs updated (rfd 0.17). On Linux they still go through the XDG desktop
  portal, and fall back to `zenity` when there's no portal. This also drops 34
  dependencies from the build.

## [0.1.0] - 2026-11-15

First version.

### Added

- File tree with folders first and one folder per line. Create, rename and move to
  trash from the tree.
- File icons: symbols for kinds of files and two-letter tiles for about 35 languages.
- Editor with line numbers, horizontal scrolling, tabs, find, and undo/redo grouped by
  word.
- Syntax highlighting for about 200 languages, plus your own `.sublime-syntax` files.
- Markdown preview next to the text.
- `settings.json`, applied when you save it.
- Two themes, Noite (dark) and Dia (light), and support for your own.
- English and Brazilian Portuguese.
- Fira Sans and Fira Code bundled as the default fonts.
- Atomic saves that keep permissions, hard links, line endings and BOMs. Binary,
  non-UTF-8 and very large files aren't opened, nor files with bare CR line breaks.
- Changes made outside BIA are picked up when you come back to the window, and saving
  over them asks first.
- Closing a tab or quitting with unsaved changes asks what to do. A file with unsaved
  changes can't be moved to the trash.
- `bia` opens a folder or several files from the command line.
- No network code, checked in CI by `deny.toml` and `tests/sealed.rs`.
- Installers for Linux, macOS and Windows.

[Unreleased]: https://github.com/thdtr/bia/compare/v0.3.0...HEAD
[0.3.0]: https://github.com/thdtr/bia/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/thdtr/bia/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/thdtr/bia/releases/tag/v0.1.0
