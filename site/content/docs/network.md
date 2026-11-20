+++
title = "No network code"
description = "BIA contains no code that opens a network connection. This page explains how that is verified and what it covers."
weight = 8
+++

BIA contains no code that opens a network connection. It does not send telemetry, check for updates or communicate with any online service. In the source code, this property is called being *sealed*.

## Rationale

An editor has access to everything written in it, including private code, `.env` files and test data. Most editors connect to several services and allow some of them to be disabled in their settings. BIA does not depend on a setting: the code is not there.

## Verification

Two checks run in CI on every pull request:

1. [`deny.toml`](https://github.com/thdtr/bia/blob/main/deny.toml) makes `cargo deny check bans` fail if an HTTP client or server, a TLS library, a WebSocket, QUIC or gRPC crate, or a telemetry SDK appears anywhere in the dependency tree, including indirect dependencies.
2. [`crates/bia-core/tests/sealed.rs`](https://github.com/thdtr/bia/blob/main/crates/bia-core/tests/sealed.rs) searches every `.rs` file in the repository for `std::net`, `TcpStream`, `UdpSocket`, networking modules of async runtimes and the names of networking crates. It also checks `Cargo.lock`.

For this reason, pull requests that add network access cannot be merged. Proposals that involve the network are best discussed in an [issue](https://github.com/thdtr/bia/issues) first, where alternatives can be considered.

### Running the checks

```sh
cargo install cargo-deny
cargo deny check bans
cargo test -p bia-core --test sealed
```

On Linux, BIA can also be started without any network access, and it works the same way:

```sh
unshare -rn bia ~/code/project
```

## Scope

BIA's own code never opens a network connection, makes an HTTP request or performs a DNS lookup. The following cases fall outside that guarantee:

- **The folder picker.** <kbd>Ctrl+O</kbd> asks the operating system to show its own dialog (the XDG portal on Linux, AppKit on macOS, the shell on Windows). That dialog belongs to the operating system. Opening folders from the command line does not use it.
- **Local communication.** On Linux, the request for that dialog travels over D-Bus, which uses a Unix socket. This is communication between programs on the same machine, not networking.
- **Remote X11 or D-Bus sessions.** If `DISPLAY` or `DBUS_SESSION_BUS_ADDRESS` point to another machine, the libraries that talk to the desktop follow them over TCP. On a regular desktop, both are local.
- **Links in Markdown.** Clicking a web link in the preview copies its URL. BIA does not open a browser. Links to files on your disk open them in BIA.
- **Images in Markdown.** The preview does not load images, local or remote. It shows their alternative text.
- **Network drives.** When a folder on NFS or SMB is opened, the operating system performs the network access. BIA only reads files.
- **Programs BIA runs.** Some features run other programs, which are separate from BIA:
  - **git**, to show changed lines and files and the branch. BIA only asks it to read the repository, and turns off what could make it start other programs or reach a server: hooks, the fsmonitor daemon and every transport (`protocol.allow=never`).
  - **Language servers**, only the ones named in `language_servers`. BIA downloads none, and starts them with the download switches of cargo, go, pip and npm turned off. See [Language servers](@/docs/language-servers.md#the-network).
  - **Your file manager**, when you choose "Show in file manager". BIA only asks it to show the file.
  - **Your shell**, in the terminal. What runs there is up to you, network included.

  For a hard guarantee on Linux, `unshare -rn bia` also takes the network away from every program BIA starts.
- **Builds from other sources.** The guarantee applies to builds of this repository. Build BIA yourself, or verify that a release file was produced by the repository's CI:

  ```sh
  gh attestation verify bia-x86_64-unknown-linux-gnu.tar.gz --repo thdtr/bia
  ```

## Features that need the network

The absence of network code is a core principle of BIA, so features that depend on the network are not added to the editor. Many of them can work well as separate tools used alongside BIA, and such ideas are welcome in an [issue](https://github.com/thdtr/bia/issues).
