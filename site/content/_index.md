+++
title = "BIA, a happy minimalist editor"
description = "A small code editor for Linux, macOS and Windows, written in Rust. It has no network code, so it never connects to anything."
template = "index.html"

[extra]
headline = "A happy minimalist editor."
lede = "A small code editor for Linux, macOS and Windows, written in Rust."
tab_unix = "macOS and Linux"
tab_windows = "Windows"
install_note = "No admin rights needed. You can also [download a build](#download)."
shot_alt = "BIA in a macOS window, with a Next.js project in the sidebar and an API route open in the editor."
shot_caption = "BIA on macOS, open on a Next.js API route."

features_title = "Everything BIA does"
features = [
  { name = "Several cursors", text = "<kbd data-mod>Ctrl+D</kbd> adds the next match. Every command works on all of them." },
  { name = "Find in files", text = "Match case, whole word and regex. Skips what `.gitignore` lists." },
  { name = "Language servers", text = "Errors as you type, completion, definition, rename. You pick the server." },
  { name = "Git", text = "Changed lines beside the numbers, changed files in the tree, the branch." },
  { name = "Terminal", text = "Your shell in a panel, with <kbd data-mod>Ctrl+`</kbd>." },
  { name = "Line commands", text = "Comment, move, duplicate and delete lines. Brackets close themselves." },
  { name = "Syntax highlighting", text = "About 200 languages, plus your own `.sublime-syntax` files." },
  { name = "Files of any size", text = "300,000 lines open at once, line numbers included." },
  { name = "Open by name", text = "<kbd data-mod>Ctrl+P</kbd> and part of the name. Add `:42` for line 42." },
  { name = "Markdown preview", text = "Live, next to the text." },
  { name = "Themes", text = "Noite and Dia, or your own in JSON." },
  { name = "English and Portuguese", text = "Follows your system, or your settings." },
]

left_out_title = "What it doesn’t do yet"
left_out = ["Debugger", "Plugins", "AI"]
left_out_note = "Some may come later. Only what needs the network stays out."

saves_title = "Saving never breaks a file"
saves_lede = "BIA writes a copy first and swaps it in once it’s safely on disk."
saves_steps = ["Writes a hidden copy", "Flushes it to disk", "Renames it over the original"]
saves_notes = [
  "Unsaved edits come back after a crash.",
  "Line endings and the BOM stay as they were.",
  "If the file changed on disk, BIA asks first.",
  "Deleted files go to the trash.",
]

settings_title = "One settings file"
settings_lede = "<kbd data-mod>Ctrl+,</kbd> opens `settings.json`. Save it and the change shows up right away."
settings_link = "Every setting and its default"

sealed_title = "BIA never connects to the internet"
sealed_lede = "No telemetry and no update check. CI fails if network code gets in."
sealed_code_label = "Check it yourself"
sealed_code = """
cargo deny check bans
cargo test -p bia-core --test sealed
unshare -rn bia ~/code/project"""
sealed_site = "This site plays by the same rule. No analytics and no requests to other servers."
sealed_link = "How the check works"

download_title = "Builds for Linux, macOS and Windows"
download_head = ["System", "Processor", "File"]
download_note = "These builds aren’t signed with a paid certificate, so your system may warn you the first time. The install command doesn’t trigger it."
download_link = "What to do about the warning"
download_anchor = "warning-on-first-launch"
+++
