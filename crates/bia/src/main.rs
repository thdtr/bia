//! BIA, a happy minimalist editor.
//!
//! ```text
//! bia [PATH]     open a folder, or a file and its folder
//! ```

// No console window behind the editor on Windows release builds.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod buffer;
mod fonts;
mod highlighter;
mod icons;
mod keys;
mod tabs;
mod ui;

use std::path::PathBuf;

const HELP: &str = "\
BIA, a happy minimalist editor

Usage: bia [PATH]

  PATH            a folder to open, or a file (its folder becomes the workspace)

Options:
  -h, --help      print this help
  -V, --version   print the version

Configuration lives in settings.json (Ctrl+, inside BIA).
BIA has no network code, so it never connects to anything.";

fn main() -> iced::Result {
    let mut path = None;
    for arg in std::env::args_os().skip(1) {
        match arg.to_str() {
            Some("-h" | "--help") => {
                println!("{HELP}");
                return Ok(());
            }
            Some("-V" | "--version") => {
                println!("bia {}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            _ => path = Some(PathBuf::from(arg)),
        }
    }
    fonts::register_bundled();
    app::run(path)
}
