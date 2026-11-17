+++
title = "Getting started"
description = "Open folders and files, work with the file tree and save safely."
weight = 2
+++

## Opening a folder

BIA works on one folder at a time, shown in the file tree on the left. A folder can be opened in three ways:

- with <kbd>Ctrl+O</kbd> (<kbd>⌘O</kbd> on macOS), which shows the system's folder picker;
- from the command line, with `bia ~/code/project`;
- by starting BIA with no arguments, which reopens the last folder used.

The last folder is stored in `state.json`, in the configuration folder. BIA never writes to `settings.json` on its own.

## Command line

```text
bia [PATH]...
```

| Argument | Effect |
|---|---|
| `bia ~/code/project` | Opens the folder. |
| `bia main.rs lib.rs` | Opens the files in tabs. The folder of the first file becomes the workspace. |
| `bia src/main.rs:42` | Opens the file at line 42. |
| `bia src/main.rs:42:7` | Opens the file at line 42, column 7. |
| `bia --version` | Prints the version. |
| `bia --help` | Prints a summary of the usage. |

## File tree

Folders are listed first, then files, one per line. Names listed in the `hidden` setting are not shown.

| Action | Keys |
|---|---|
| New file in the selected folder | <kbd>Ctrl+N</kbd> |
| New folder | <kbd>Ctrl+Shift+N</kbd> |
| Rename | <kbd>F2</kbd> |
| Move to the trash | <kbd>Delete</kbd> |
| Show or hide the sidebar | <kbd>Ctrl+B</kbd> |

Deleted files go to the operating system's trash and can be restored from there. A file with unsaved changes cannot be moved to the trash.

## Finding files and lines

<kbd>Ctrl+P</kbd> opens a file by name. Typing part of the name filters the list. Adding `:line` at the end, as in `main:42`, opens the file at that line.

<kbd>Ctrl+G</kbd> moves the cursor to a line in the current file. A value in the form `line:column` is also accepted.

## Find and replace

<kbd>Ctrl+F</kbd> searches the current file. <kbd>F3</kbd> and <kbd>Shift+F3</kbd> move to the next and previous match.

<kbd>Ctrl+H</kbd> opens replace (<kbd>⌘⌥F</kbd> on macOS, where <kbd>⌘H</kbd> hides the application). Matches can be replaced one at a time or all at once.

## Indentation

BIA detects whether each file is indented with tabs or spaces, and how wide one level is. It then:

- keeps the current indentation when <kbd>Enter</kbd> is pressed;
- adds one level after `{`, `[` or `(`;
- indents or unindents the selected lines with <kbd>Tab</kbd> and <kbd>Shift+Tab</kbd>;
- removes one level with <kbd>Backspace</kbd> inside leading spaces.

Files with no indentation yet use `tab_size` from the settings. Go files and Makefiles use tabs. The status bar shows the indentation of the current file.

## Markdown preview

<kbd>Ctrl+Shift+V</kbd> shows a preview of the current Markdown file next to the text. The preview follows CommonMark, with GitHub tables, task lists and strikethrough. It does not load images, and links are copied to the clipboard instead of being opened.

## Saving

<kbd>Ctrl+S</kbd> saves the current file. Each save:

1. writes the new content to a temporary file in the same folder, with the same permissions;
2. flushes the temporary file to disk;
3. renames it over the original.

An interrupted save therefore never leaves a partially written file. Files with more than one hard link, and files in folders where BIA cannot create new files, are written in place.

Line endings (LF or CRLF) and a UTF-8 byte order mark are written back as they were read. A file that mixes LF and CRLF is saved with the more common of the two, and the status bar reports this before saving.

If a file changed on disk after it was opened, BIA asks before overwriting it. Changes made outside BIA are picked up when its window regains focus.

A dot on a tab marks unsaved changes. Closing the tab or quitting with unsaved changes asks for confirmation.

## Files BIA does not open

To avoid damaging them, BIA refuses to open:

- binary files;
- files that are not valid UTF-8;
- files larger than 16 MB;
- files that use a bare CR as the line break.

An explanation is shown in place of the editor.
