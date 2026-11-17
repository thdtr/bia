# Changelog

Notable changes to BIA are listed here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/).

## [Unreleased]

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

### Changed

- Open file by name (`Ctrl+P`) skips what `.gitignore` lists.
- MIT is now the only license. Versions up to 0.2.0 remain available under MIT or
  Apache 2.0.

### Fixed

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

[Unreleased]: https://github.com/thdtr/bia/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/thdtr/bia/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/thdtr/bia/releases/tag/v0.1.0
