+++
title = "Known limitations"
description = "Current limitations of BIA, and features it does not have yet."
weight = 9
+++

## Current limitations

- **Mixed line endings are not preserved.** A file that mixes LF and CRLF is saved with the more common of the two. The status bar reports this before saving.
- **Bare CR line breaks are not supported.** Files that use them are not opened.
- **Find in files does not replace.** Each result opens its file with the match selected, where <kbd>Ctrl+H</kbd> works.
- **Highlighting far into a long file can be wrong.** Jumping more than 5,000 lines past what was already highlighted starts highlighting a little above the screen, which is wrong only inside a comment or string longer than that.
- **Language servers get the whole file after each pause in typing.** This is simple and reliable, but slower than sending only the changes on very large files.
- **Input methods are untested.** Composed characters and input methods for languages such as Japanese should work, but haven't been tested yet.

## Not available yet

BIA does not currently include a debugger, plugins or AI features. Some of these may be added in the future. Because each one adds significantly to the code, a proposal should start as an [issue](https://github.com/thdtr/bia/issues), so that it can be discussed before any code is written.

Features that require the network, such as telemetry, automatic updates, accounts or sync, do not fit BIA’s network policy. They may be better served by a separate tool. See [No network code](@/docs/network.md).
