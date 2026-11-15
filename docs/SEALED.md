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

Pull requests that add networking won't be merged, whatever the feature
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
* **Links in Markdown.** Clicking a link in the preview copies the URL. BIA doesn't
  open a browser.
* **Images in Markdown.** The preview doesn't load images, local or remote. It shows
  the alt text instead.
* **Network drives.** If you open a folder on NFS or SMB, your OS does the network
  I/O. BIA only reads the files.
* **Builds from somewhere else.** This only holds for builds of this code. Build it
  yourself, or check that a release file came from this repository's CI:

  ```sh
  gh attestation verify bia-x86_64-unknown-linux-gnu.tar.gz --repo thdtr/bia
  ```

## Adding network features

This isn't going to change. A feature that needs the network is better off as a
separate tool.
