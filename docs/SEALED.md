# No network code

BIA has no code that opens a network connection. It doesn't send telemetry, check for
updates or talk to any online service. In the code this is called being *sealed*.

## Why

An editor sees everything you write, including private code, `.env` files and test
data. Most editors today connect to several services and let you turn some of them
off in the settings. We didn't want BIA to depend on a setting, so the code isn't
there at all.

## How we check it

Two checks run in CI on every pull request:

1. [`deny.toml`](../deny.toml) makes `cargo deny check bans` fail if an HTTP client or
   server (`reqwest`, `hyper`, `ureq`, `curl` and others), a TLS library (`rustls`,
   `native-tls`, `openssl`), a WebSocket, QUIC or gRPC crate, or a telemetry SDK shows
   up anywhere in the dependency tree, including indirect dependencies.
2. [`crates/bia-core/tests/sealed.rs`](../crates/bia-core/tests/sealed.rs) searches every
   `.rs` file in the repository for `std::net`, `TcpStream`, `UdpSocket`, async runtime
   networking modules and the names of networking crates. It also checks `Cargo.lock`.

That's why a pull request that adds networking can't be merged, even for a good idea.
Open an issue first and we'll look for another way
([CONTRIBUTING.md](../CONTRIBUTING.md)).

## Checking it yourself

```sh
cargo install cargo-deny
cargo deny check bans
cargo test -p bia-core --test sealed
```

On Linux you can also start BIA with no network at all and see that it works the
same:

```sh
unshare -rn bia ~/code/project
```

## What this covers and what it doesn't

BIA's own code never opens a network connection, makes an HTTP request or does a DNS
lookup.

Some things are outside that:

* **The folder picker.** "Open folder" asks your desktop to show its own dialog (the
  XDG portal on Linux, AppKit on macOS, the shell on Windows). That dialog is part of
  your OS. Opening folders from the command line skips it.
* **Local communication.** On Linux the request for that dialog goes over D-Bus, which
  uses a Unix socket. That's communication between programs on your own machine, not
  networking.
* **Remote X11 or D-Bus.** If `DISPLAY` or `DBUS_SESSION_BUS_ADDRESS` point to another
  machine (X forwarding, a remote session bus), the libraries that talk to your
  desktop follow them over TCP. On a normal desktop both are local.
* **Links in Markdown.** Clicking a web link in the preview copies the URL. BIA
  doesn't open a browser. Links to files on your disk open them in BIA.
* **Images in Markdown.** The preview doesn't load images, local or remote. It shows
  the alt text instead.
* **Network drives.** If you open a folder on NFS or SMB, your OS does the network
  I/O. BIA only reads the files.
* **Programs BIA runs for you.** BIA runs `git` to show the branch and what changed.
  Those calls only read the repository on your disk, and every one turns off what
  could make git start other programs or reach a server: hooks, the fsmonitor
  daemon and every transport (`protocol.allow=never`). Programs are separate from
  BIA, so what they do is up to them, but none of these calls asks git to use the
  network.
* **Your file manager.** "Show in file manager" asks it to show the file: through
  D-Bus on Linux (`dbus-send`, or `xdg-open` on the folder when that fails), with
  `open -R` on macOS and `explorer /select` on Windows. That's all BIA asks it.
* **The terminal.** It runs your shell, and what you run there is yours, network
  included. BIA only shows it.
* **Language servers.** They're programs you install and name in
  `language_servers`; BIA starts none on its own and downloads none. It talks to them
  over their stdin and stdout. What a server does is up to it, and some reach the
  network on their own: rust-analyzer runs cargo, gopls runs go. BIA starts them
  with `CARGO_NET_OFFLINE=true`, `GOPROXY=off`, `GOTOOLCHAIN=local`,
  `PIP_NO_INDEX=1` and `NPM_CONFIG_OFFLINE=true`, which turn off the downloads those
  tools know about. For a hard guarantee on Linux, run BIA with `unshare -rn`, as
  above: its servers inherit having no network.
* **Builds from somewhere else.** This only holds for builds of this code. Build it
  yourself, or check that a release file came from this repository's CI:

  ```sh
  gh attestation verify bia-x86_64-unknown-linux-gnu.tar.gz --repo thdtr/bia
  ```

## Features that need the network

BIA is built around having no network code, so these features don't fit inside the
editor. Many of them work well as a separate tool next to BIA. If you have one in mind,
open an issue and we'll think it through together.
