+++
title = "Syntax highlighting"
description = "The built-in grammars, how to add a language and how BIA chooses a grammar."
weight = 7
+++

BIA highlights code with [Sublime Text syntax definitions](https://www.sublimetext.com/docs/syntax.html) (`.sublime-syntax` files). About 200 languages are included, from the set maintained by the [`bat`](https://github.com/sharkdp/bat) project.

## Adding a language

No rebuild is required.

1. Obtain or write a `.sublime-syntax` file. Most languages already have one, published in Sublime Text packages or on GitHub.
2. Place it in the `languages` folder inside the configuration folder (see [Settings](@/docs/settings.md#location)).
3. Restart BIA, or open `settings.json` and save it.

A custom grammar takes precedence over a built-in one for the same file extension, which also allows a built-in grammar to be replaced. An invalid grammar file is skipped and reported in the status bar. It never prevents BIA from starting.

### Example

The file `languages/todo.sublime-syntax` highlights task lists with the `.todo` extension:

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

The colors come from the `syntax` map of the active theme, matched by scope (see [Themes](@/docs/themes.md)).

## How a grammar is chosen

BIA tries the following, in order:

1. the exact file name, such as `Makefile`, `Dockerfile` or `CMakeLists.txt`;
2. the extension, such as `.rs`, `.py` or `.tsx`;
3. the first line, such as `#!/usr/bin/env python3` or `<?xml`;
4. plain text.

The status bar shows the grammar in use.

## Comments

<kbd>Ctrl+/</kbd> comments lines with the language's own marker: `//` in Rust, `#` in Python, `<!-- -->` in HTML. The built-in grammars don't carry these markers, so BIA keeps them in a table of its own, for about 100 languages. An added grammar isn't in it, and <kbd>Ctrl+/</kbd> says so in the status bar.

The language name in the status bar is also the key for `language_servers` (see [Language servers](@/docs/language-servers.md)).

## Why not tree-sitter

Tree-sitter parses code more precisely, but each grammar is C code that must be compiled into the editor. Sublime Text grammars are data files, so anyone can add a language by placing a file in a folder.
