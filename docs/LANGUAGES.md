# Languages

BIA highlights code with [Sublime Text syntax definitions](https://www.sublimetext.com/docs/syntax.html)
(`.sublime-syntax`). About 200 languages are built in: the set curated by the
[`bat`](https://github.com/sharkdp/bat) project, through the
[`two-face`](https://github.com/CosmicHorrorDev/two-face) crate.

## Adding a language, no rebuild needed

1. Get or write a `.sublime-syntax` file. Most languages already have one: search
   GitHub for `<language> sublime-syntax`, or look in Sublime Text packages.
2. Put it in your languages folder:
   * Linux: `~/.config/bia/languages/`
   * macOS: `~/Library/Application Support/bia/languages/`
   * Windows: `%APPDATA%\bia\languages\`
3. Restart BIA, or open `settings.json` and save it.

A user grammar wins over a built-in one for the same file extension, so you can also
use this to replace a built-in grammar. A broken grammar file is skipped and reported
in the status bar. It never stops BIA from starting.

### Minimal example

`~/.config/bia/languages/todo.sublime-syntax`:

```yaml
%YAML 1.2
---
name: Todo
file_extensions: [todo]
scope: text.todo
contexts:
  main:
    - match: '^\s*\[x\].*$'
      scope: comment.done.todo
    - match: '^\s*\[ \]'
      scope: keyword.open.todo
    - match: '@\w+'
      scope: entity.name.tag.todo
```

The colors come from your theme's `syntax` map, matched by scope (see
[THEMES.md](THEMES.md)).

## How BIA picks a grammar

In order:

1. The exact file name (`Makefile`, `Dockerfile`, `CMakeLists.txt`)
2. The extension (`.rs`, `.py`, `.tsx`)
3. The first line (`#!/usr/bin/env python3`, `<?xml`, `-*- mode -*-`)
4. Plain text

The status bar shows which grammar is active.

## Comments

`Ctrl+/` needs to know how a language writes comments, and the built-in grammars don't
say (Sublime keeps that in separate metadata files). BIA has a table of its own for
about 100 languages, in `crates/bia-core/src/editing.rs`, keyed by grammar name. A
grammar you add isn't in it: `Ctrl+/` says so in the status bar. A line in that table
fixes it for everyone.

## Why Sublime grammars and not tree-sitter?

Tree-sitter parses more precisely, but every grammar is C code that has to be compiled
into the editor (or loaded as WASM through a runtime). Sublime grammars are data
files. We'd rather let anyone add a language by dropping in a file than have the more
precise parser. See [ARCHITECTURE.md](ARCHITECTURE.md).
