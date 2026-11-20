+++
title = "Getting started"
description = "Open folders and files, edit with several cursors, search, use git and the terminal, and save safely."
weight = 2
+++

## Opening a folder

BIA works on one folder at a time, shown in the file tree on the left. A folder can be opened in three ways:

- with <kbd>Ctrl+O</kbd> (<kbd>⌘O</kbd> on macOS), which shows the system's folder picker;
- from the command line, with `bia ~/code/project`;
- by starting BIA with no arguments, which reopens the last folder used.

Each folder remembers the files that were open in it and where the cursor was. They open again with the folder. The welcome screen lists recent folders.

This is stored in `state.json`, in the configuration folder. BIA never writes to `settings.json` on its own.

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

Folders are listed first, then files, one per line. Names listed in the `hidden` setting are not shown. The file you're editing is selected in the tree, and the tree scrolls to it.

The buttons at the top of the sidebar create a file or a folder, collapse every folder and hide the sidebar. A right click on a file or folder opens a menu with the rest, such as cut, copy, paste, duplicate, copy the path, rename, move to the trash, show it in your file manager or open a terminal there.

Names are typed right in the tree. Enter confirms and Escape cancels. A name with folders in it, like `src/ui/menu.rs`, creates the folders too. When you rename a file, its name comes selected without the extension.

Drag a file or folder onto another folder to move it there, or hold Alt to copy it. Files copied in your file manager can be pasted into the tree. A copy that would take a name already in use gets another one, like `notes copy.md`, and its name field opens so you can change it. BIA never replaces a file when it creates, copies or moves one.

| Action | Keys |
|---|---|
| New file in the selected folder | <kbd>Ctrl+N</kbd> |
| New folder | <kbd>Ctrl+Shift+N</kbd> |
| Rename | <kbd>F2</kbd> |
| Move to the trash | <kbd>Delete</kbd> |
| Cut, copy, paste | <kbd>Ctrl+X</kbd>, <kbd>Ctrl+C</kbd>, <kbd>Ctrl+V</kbd> |
| Show or hide the sidebar | <kbd>Ctrl+B</kbd> |

Cut, copy and paste work in the tree after you click a folder there, since a click on a file takes you to the editor. A right click on the path in the status bar copies it.

Drag the edge of the sidebar to change its width. BIA remembers the width, and whether you hid the sidebar.

Deleted files go to the operating system's trash and can be restored from there. A file with unsaved changes cannot be moved to the trash.

In a git repository, changed files are shown in one color and new files in another, along with the folders that contain them.

## Finding files and lines

<kbd>Ctrl+P</kbd> opens a file by name. Typing part of the name filters the list. Adding `:line` at the end, as in `main:42`, opens the file at that line. Files listed in `.gitignore` are left out.

<kbd>Ctrl+G</kbd> moves the cursor to a line in the current file. A value in the form `line:column` is also accepted.

## Several cursors

Every editing command works at each cursor at once.

| Action | Keys |
|---|---|
| Select the word, then its next occurrence | <kbd>Ctrl+D</kbd> |
| Select every occurrence | <kbd>Ctrl+Shift+L</kbd> |
| Add a cursor above or below | <kbd>Ctrl+Alt+↑</kbd> / <kbd>Ctrl+Alt+↓</kbd> |
| Add or remove a cursor | <kbd>Alt</kbd>+click |
| Back to one cursor | <kbd>Esc</kbd> |

When the clipboard holds as many lines as there are cursors, pasting gives one line to each.

## Lines

| Action | Keys |
|---|---|
| Comment or uncomment | <kbd>Ctrl+/</kbd> |
| Move up or down | <kbd>Alt+↑</kbd> / <kbd>Alt+↓</kbd> |
| Duplicate up or down | <kbd>Shift+Alt+↑</kbd> / <kbd>Shift+Alt+↓</kbd> |
| Delete | <kbd>Ctrl+Shift+K</kbd> |
| Select the line (again for the next one) | <kbd>Ctrl+L</kbd> |

<kbd>Ctrl+C</kbd> and <kbd>Ctrl+X</kbd> with nothing selected take the whole line, and pasting it puts it above the cursor's line.

Typing `(`, `[`, `{` or a quote also types the closing half, and wraps the selection if there is one. The `auto_close` setting turns this off. The bracket matching the one at the cursor is highlighted.

With `soft_wrap` on, long lines wrap at the edge of the window, and the arrow keys move by rows on screen. <kbd>Ctrl+=</kbd> and <kbd>Ctrl+-</kbd> change the font size until BIA closes; <kbd>Ctrl+0</kbd> resets it.

## Find and replace

<kbd>Ctrl+F</kbd> searches the current file and highlights every match. <kbd>Enter</kbd>, <kbd>F3</kbd> and <kbd>Shift+F3</kbd> move to the next and previous match.

<kbd>Ctrl+H</kbd> opens replace (<kbd>⌘⌥F</kbd> on macOS, where <kbd>⌘H</kbd> hides the application). Matches can be replaced one at a time or all at once.

Three buttons next to the field match case, match whole words and read the text as a regular expression. In a regular expression, the replacement can use `$1` for a group, and `\n` for a line break.

## Find in files

<kbd>Ctrl+Shift+F</kbd> searches every file in the folder, in the sidebar, with the same three options. <kbd>Enter</kbd> starts the search. Files listed in `.gitignore`, binary files and files over 16 MB are skipped. Open files are searched with their unsaved changes. Clicking a result opens the file with the match selected.

<kbd>Ctrl+Shift+E</kbd> goes back to the file tree.

## Git

In a git repository, bars beside the line numbers show the lines added and changed since the last commit, and a mark shows where lines were deleted. The status bar shows the branch. All of this refreshes on save and when the window regains focus.

BIA runs your `git` for this, only to read the repository. Without git, or outside a repository, nothing is shown.

## Terminal

<kbd>Ctrl+`</kbd>, or "Terminal" in the status bar, shows the terminal under the editor, running your shell in the open folder. Pressing it again hides the panel, and the shells keep running.

Each terminal has a tab at the top of the panel. <kbd>Ctrl+Shift+`</kbd> or the + button opens another one, and × closes it. Typing `exit` closes it too. On Linux and macOS, BIA asks first when a program is still running in a terminal you close, or when you quit. Drag the top edge of the panel to make it taller or shorter.

Hold <kbd>Ctrl</kbd> and click a file location the terminal printed, such as `src/main.rs:42:7` or `src/app.ts(12,5)`, to open the file at that line.

<kbd>Ctrl+Shift+C</kbd> and <kbd>Ctrl+Shift+V</kbd> copy and paste. <kbd>Ctrl+C</kbd> goes to the program, as in any terminal. Shortcuts such as <kbd>Ctrl+P</kbd> and <kbd>Ctrl+Tab</kbd> still work while the terminal has focus.

## Language servers

With a language server set up, BIA underlines errors as you type, completes code and can go to definitions, show what's under the mouse, format and rename. See [Language servers](@/docs/language-servers.md).

## Indentation

BIA detects whether each file is indented with tabs or spaces, and how wide one level is. It then:

- keeps the current indentation when <kbd>Enter</kbd> is pressed;
- adds one level after `{`, `[` or `(`;
- indents or unindents the selected lines with <kbd>Tab</kbd> and <kbd>Shift+Tab</kbd>;
- removes one level with <kbd>Backspace</kbd> inside leading spaces.

Files with no indentation yet use `tab_size` from the settings. Go files and Makefiles use tabs. The status bar shows the indentation of the current file.

## Two files side by side

<kbd>Ctrl+\</kbd>, or the button at the right of the tabs, moves the current tab to the other side of the window, so you can see two files at once. Each side has its own tabs, and the files you open go to the side you're working on. When the last tab on the right closes, the window goes back to one side.

## Markdown preview

<kbd>Ctrl+Shift+V</kbd> opens a preview of the current Markdown file in a tab beside it, and the preview follows what you type. Pressing it again, or closing the tab, closes the preview. The preview follows CommonMark, with GitHub tables, task lists and strikethrough. It does not load images. Links to other files open them in BIA, and web links are copied to the clipboard instead of being opened.

## Saving

<kbd>Ctrl+S</kbd> saves the current file. Each save:

1. writes the new content to a temporary file in the same folder, with the same permissions;
2. flushes the temporary file to disk;
3. renames it over the original.

An interrupted save therefore never leaves a partially written file. Files with more than one hard link, and files in folders where BIA cannot create new files, are written in place.

Line endings (LF or CRLF) and a UTF-8 byte order mark are written back as they were read. A file that mixes LF and CRLF is saved with the more common of the two, and the status bar reports this before saving.

If a file changed on disk after it was opened, BIA asks before overwriting it. Changes made outside BIA are picked up when its window regains focus.

A dot on a tab marks unsaved changes. Closing the tab or quitting with unsaved changes asks for confirmation.

Unsaved changes are also copied to the `backups` folder, in the configuration folder, two seconds after the last edit. If BIA or the system stops before they are saved, opening the file again brings them back. The copy is deleted once the file is saved or the changes are discarded.

## Files BIA does not open

To avoid damaging them, BIA refuses to open:

- binary files;
- files that are not valid UTF-8;
- files larger than 16 MB;
- files that use a bare CR as the line break.

An explanation is shown in place of the editor.
