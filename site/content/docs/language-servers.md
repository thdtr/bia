+++
title = "Language servers"
description = "Errors as you type, completion, go to definition, hover, format and rename, from a language server you choose."
weight = 5
+++

A language server is a program that understands one language: it finds errors, knows what each name refers to and suggests completions. BIA talks to it over the [Language Server Protocol](https://microsoft.github.io/language-server-protocol/).

BIA doesn't include, download or look for any server. It starts only the programs named in the `language_servers` setting.

## Setting up a server

1. Install the server, for example `rust-analyzer` for Rust (`rustup component add rust-analyzer`) or `pyright` for Python.
2. In `settings.json`, add its command under the language's name, as the status bar shows it:

   ```json
   "language_servers": {
     "Rust": ["rust-analyzer"],
     "Python": ["pyright-langserver", "--stdio"],
     "Go": ["gopls"]
   }
   ```

3. Save `settings.json`. The server starts the next time a file in that language is open.

The command is a list: the program, then its arguments. When the program isn't on the `PATH` of the session that started BIA, use its full path. If it can't start, the status bar says so.

## What it does

| Feature | How |
|---|---|
| Errors and warnings | Underlined as you type. The status bar counts them and shows the message under the cursor. |
| Hover | Rest the mouse over a name to see its type and documentation, with the problem at that spot first. |
| Completion | Opens while typing a word or after `.`, or with <kbd>Ctrl+Space</kbd>. <kbd>↑</kbd> and <kbd>↓</kbd> choose, <kbd>Enter</kbd> or <kbd>Tab</kbd> accept, <kbd>Esc</kbd> closes. |
| Go to definition | <kbd>F12</kbd>, or <kbd>Ctrl</kbd>+click. |
| Format the file | <kbd>Shift+Alt+F</kbd>. |
| Rename | <kbd>F2</kbd> in the editor asks for the new name and renames it everywhere the server finds it. Files that weren't open open with the change, unsaved. |
| Next problem | <kbd>F8</kbd>, and <kbd>Shift+F8</kbd> for the previous one. |

One server runs per language, for the open folder. Changing `language_servers` and saving restarts them.

## The network

Language servers are separate programs, and some reach the network on their own: rust-analyzer runs `cargo`, which can download dependencies, and gopls runs `go`. BIA starts every server with these variables, which turn off the downloads those tools know about:

| Variable | Effect |
|---|---|
| `CARGO_NET_OFFLINE=true` | cargo uses only what's already downloaded. |
| `GOPROXY=off`, `GOTOOLCHAIN=local` | go downloads neither modules nor toolchains. |
| `PIP_NO_INDEX=1` | pip doesn't use the package index. |
| `NPM_CONFIG_OFFLINE=true` | npm uses only its cache. |

With dependencies that aren't downloaded yet, the server knows less about them instead of fetching them. On Linux, `unshare -rn bia` takes the network away from BIA and every program it starts. See [No network code](@/docs/network.md).
