//! Checks that BIA contains no network code.
//!
//! This test scans every Rust source file in the repository for the APIs
//! and crates that open network connections. `deny.toml` enforces the same
//! rule for the whole dependency graph. If you are reading this because the
//! test failed: features that need the network are out of scope for BIA.
//! See docs/SEALED.md.

use std::fs;
use std::path::{Path, PathBuf};

const FORBIDDEN: &[&str] = &[
    "std::net",
    "TcpStream",
    "TcpListener",
    "UdpSocket",
    "ToSocketAddrs",
    "reqwest",
    "hyper",
    "ureq",
    "isahc",
    "surf::",
    "curl::",
    "tungstenite",
    "rustls",
    "native_tls",
    "openssl",
    "tokio::net",
    "async_std::net",
    "smol::net",
    "mio::net",
    "socket2",
];

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        if path.is_dir() {
            if name != "target" && name != ".git" {
                rust_files(&path, out);
            }
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// `needle` appears in `line` and is not part of a longer identifier
/// (so "hyperlink" does not match "hyper").
fn contains_word(line: &str, needle: &str) -> bool {
    let is_ident = |c: char| c.is_alphanumeric() || c == '_';
    line.match_indices(needle).any(|(i, _)| {
        let before = line[..i].chars().next_back();
        let after = line[i + needle.len()..].chars().next();
        let starts_word = !needle.starts_with(is_ident) || !before.is_some_and(is_ident);
        let ends_word = !needle.ends_with(is_ident) || !after.is_some_and(is_ident);
        starts_word && ends_word
    })
}

#[test]
fn no_network_code_in_bia() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let this_file = Path::new(file!()).file_name().unwrap();
    let mut files = Vec::new();
    rust_files(&root.join("crates"), &mut files);
    assert!(files.len() > 5, "scanner found too few files; wrong root?");

    let mut violations = Vec::new();
    for file in files.iter().filter(|f| f.file_name() != Some(this_file)) {
        let source = fs::read_to_string(file).unwrap();
        for (n, line) in source.lines().enumerate() {
            for needle in FORBIDDEN {
                if contains_word(line, needle) {
                    violations.push(format!("{}:{}: `{needle}`", file.display(), n + 1));
                }
            }
        }
    }
    assert!(violations.is_empty(), "network code found:\n{}", violations.join("\n"));
}

#[test]
fn no_network_crates_in_manifests() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let lock = fs::read_to_string(root.join("Cargo.lock")).unwrap_or_default();
    let banned = [
        "reqwest",
        "hyper",
        "ureq",
        "isahc",
        "surf",
        "curl",
        "tungstenite",
        "rustls",
        "native-tls",
        "openssl",
        "h2",
        "quinn",
    ];
    let present: Vec<&str> = banned
        .iter()
        .copied()
        .filter(|name| lock.contains(&format!("name = \"{name}\"\n")))
        .collect();
    assert!(present.is_empty(), "network crates in Cargo.lock: {present:?}");
}
