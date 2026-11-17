+++
title = "Known limitations"
description = "Current limitations of BIA, and features it does not have yet."
weight = 8
+++

## Current limitations

- **Long lines do not wrap.** The line numbers and the text share one scroll area so that they always stay aligned, and the editor widget only supports this without wrapping. Long lines scroll horizontally.
- **Very long files are slow to open.** The whole file is laid out when it is opened: about 0.3 seconds for 3,000 lines and 0.8 seconds for 10,000. Files with more than 10,000 lines open without line numbers, which keeps scrolling and typing fast.
- **Mixed line endings are not preserved.** A file that mixes LF and CRLF is saved with the more common of the two. The status bar reports this before saving.
- **Tabs are drawn eight columns wide.** The editor widget does not allow this to be changed. `tab_size` only sets what the <kbd>Tab</kbd> key inserts.

## Not available yet

BIA does not currently include a terminal, git integration, a debugger, Language Server Protocol (LSP) support, plugins or AI features. Some of these may be added in the future. Because each one would add significantly to the code (LSP support alone would roughly double it), a proposal should start as an [issue](https://github.com/thdtr/bia/issues), so that it can be discussed before any code is written.

Features that require the network, such as telemetry, automatic updates, accounts or sync, will not be added. See [No network code](@/docs/network.md).
