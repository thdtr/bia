//! BIA, a happy minimalist editor.
//!
//! ```text
//! bia [PATH]...  open a folder, or files and the first one's folder
//! ```

// No console window behind the editor on Windows release builds.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod buffer;
mod code_view;
mod commands;
mod fonts;
mod frame_cache;
mod icons;
mod keys;
mod lsp;
mod markdown;
mod syntax;
mod tabs;
mod terminal;
mod terminal_view;
mod ui;

use std::path::PathBuf;

const HELP: &str = "\
BIA, a happy minimalist editor

Usage: bia [PATH]...

  PATH              a folder to open, or files to open in tabs
                    (without a folder, the first file's folder is the workspace)

Options:
  -n, --new         open a new window with nothing in it
  -f, --foreground  stay attached to the terminal instead of giving it back
  -h, --help        print this help
  -V, --version     print the version

Configuration lives in settings.json (Ctrl+, inside BIA).
BIA has no network code, so it never connects to anything.";

fn main() -> iced::Result {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let mut paths = Vec::new();
    let mut new = false;
    let mut foreground = false;
    for arg in args.iter().cloned() {
        match arg.to_str() {
            Some("-h" | "--help") => {
                println!("{HELP}");
                return Ok(());
            }
            Some("-n" | "--new") => new = true,
            Some("-f" | "--foreground") => foreground = true,
            Some("-V" | "--version") => {
                println!("bia {} beta", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            _ => paths.push(PathBuf::from(arg)),
        }
    }
    if !foreground && detach(&args) {
        return Ok(());
    }
    fonts::register_bundled();
    app::run(paths, new)
}

/// Started from a terminal, BIA starts itself again on its own and gives the
/// prompt back, like `code .` does. The copy runs in its own process group,
/// so Ctrl+C or closing the terminal leaves it alone. False when BIA should
/// run here: no terminal to give back, or the copy could not start.
///
/// Windows needs none of this: release builds are GUI programs, and the
/// shell does not wait for them. Debug builds stay in the terminal, so
/// `cargo run` shows panics and stops with Ctrl+C.
#[cfg(all(unix, not(debug_assertions)))]
fn detach(args: &[std::ffi::OsString]) -> bool {
    use std::io::IsTerminal;
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};

    let in_terminal = std::io::stdin().is_terminal()
        || std::io::stdout().is_terminal()
        || std::io::stderr().is_terminal();
    if !in_terminal {
        return false;
    }
    std::env::current_exe()
        .and_then(|exe| {
            Command::new(exe)
                .args(args)
                .arg("--foreground")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .process_group(0)
                .spawn()
        })
        .is_ok()
}

#[cfg(not(all(unix, not(debug_assertions))))]
fn detach(_args: &[std::ffi::OsString]) -> bool {
    false
}
