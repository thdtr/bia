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
mod fonts;
mod icons;
mod keys;
mod markdown;
mod syntax;
mod tabs;
mod ui;

use std::path::PathBuf;

const HELP: &str = "\
BIA, a happy minimalist editor

Usage: bia [PATH]...

  PATH            a folder to open, or files to open in tabs
                  (without a folder, the first file's folder is the workspace)

Options:
  -h, --help      print this help
  -V, --version   print the version

Configuration lives in settings.json (Ctrl+, inside BIA).
BIA has no network code, so it never connects to anything.";

fn main() -> iced::Result {
    let mut paths = Vec::new();
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
            _ => paths.push(PathBuf::from(arg)),
        }
    }
    fonts::register_bundled();
    app::run(paths)
}
