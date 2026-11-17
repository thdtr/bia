+++
title = "Known limitations"
description = "What BIA does not do yet, and what it will not do."
weight = 8
+++

## Current limitations

- **Long lines do not wrap.** The line numbers and the text share one scroll area so that they always stay aligned, and the editor widget only supports this without wrapping. Long lines scroll horizontally.
- **Very long files are slow to open.** The whole file is laid out when it is opened: about 0.3 seconds for 3,000 lines and 0.8 seconds for 10,000. Files with more than 10,000 lines open without line numbers, which keeps scrolling and typing fast.
- **Mixed line endings are not preserved.** A file that mixes LF and CRLF is saved with the more common of the two. The status bar reports this before saving.
- **Tabs are drawn eight columns wide.** The editor widget does not allow this to be changed. `tab_size` only sets what the <kbd>Tab</kbd> key inserts.

## Out of scope

BIA does not include, and does not plan to include, a terminal, git integration, a debugger, plugins, AI features, telemetry, automatic updates, accounts or sync. These features either need the network (see [No network code](@/docs/network.md)) or work better as separate tools.

Language Server Protocol (LSP) support is the one exception that may be reconsidered. It runs local programs and therefore does not conflict with the network policy, but it would roughly double the size of the code. It requires a discussion in an issue before any code is written.
