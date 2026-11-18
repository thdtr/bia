+++
title = "Installation"
description = "Install, update and remove BIA on Linux, macOS and Windows."
weight = 1
+++

BIA runs on Linux (x86_64 and ARM64), macOS (Apple silicon and Intel) and Windows (x86_64). Installation does not require administrator rights.

## Install script

On Linux and macOS, run the following command in a terminal:

```sh
curl -fsSL https://raw.githubusercontent.com/thdtr/bia/main/install.sh | sh
```

On Windows, run the following command in PowerShell:

```powershell
irm https://raw.githubusercontent.com/thdtr/bia/main/install.ps1 | iex
```

The script downloads the latest release for your system, verifies its SHA-256 checksum and installs it in your user folder:

| System | Location |
|---|---|
| Linux | `~/.local/bin/bia`, with an entry in the application menu |
| macOS | `~/Applications/BIA.app`, with the `bia` command in `~/.local/bin` |
| Windows | `%LOCALAPPDATA%\Programs\bia`, with a Start menu shortcut |

If `~/.local/bin` is not on your `PATH`, the script prints the line to add to your shell configuration.

### Options

On Linux and macOS, options are passed after `sh -s --`:

```sh
curl -fsSL https://raw.githubusercontent.com/thdtr/bia/main/install.sh | sh -s -- --version v0.4.0
```

| Option | Effect |
|---|---|
| `--version vX.Y.Z` | Installs a specific release instead of the latest one. |
| `--uninstall` | Removes BIA. |
| `BIA_PREFIX` (environment variable) | Changes the installation prefix. The default is `~/.local`. |

## Updating

To update BIA, run the install command again. Settings, themes and grammar files are not modified.

## Removing

On Linux and macOS:

```sh
curl -fsSL https://raw.githubusercontent.com/thdtr/bia/main/install.sh | sh -s -- --uninstall
```

On Windows:

```powershell
$env:BIA_UNINSTALL = 1; irm https://raw.githubusercontent.com/thdtr/bia/main/install.ps1 | iex
```

Removing BIA does not delete its configuration folder (see [Settings](@/docs/settings.md#location)).

## Manual download

Each release on [GitHub](https://github.com/thdtr/bia/releases) includes an archive per platform and a `.sha256` file with its checksum. Every archive is built by the repository's CI, and its origin can be verified with the GitHub CLI:

```sh
gh attestation verify bia-x86_64-unknown-linux-gnu.tar.gz --repo thdtr/bia
```

### Warning on first launch

Release builds are not signed with a paid certificate. When BIA is downloaded from the releases page, the operating system may block it the first time it is opened:

- **macOS:** open *System Settings → Privacy & Security* and click *Open Anyway*.
- **Windows:** click *More info*, then *Run anyway*.

The install script does not trigger this warning.

## Building from source

With a Rust toolchain (1.88 or later) installed:

```sh
cargo install --git https://github.com/thdtr/bia bia
```
