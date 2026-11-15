# Changelog

Notable changes to BIA are listed here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/).

## [Unreleased]

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
- Atomic saves that keep line endings and BOMs. Binary, non-UTF-8 and very large files
  aren't opened. Changes made outside BIA are picked up when you come back to the
  window. Unsaved work is protected when closing tabs, quitting and deleting.
- No network code, checked in CI by `deny.toml` and `tests/sealed.rs`.
- Installers for Linux, macOS and Windows.
