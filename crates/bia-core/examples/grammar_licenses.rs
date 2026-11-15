//! Prints the licenses of the syntax definitions built into BIA. Used by
//! `tools/third_party_licenses.sh` for the file that ships in release
//! packages.
//!
//! ```sh
//! cargo run -p bia-core --example grammar_licenses
//! ```

use std::io::{self, Write};

fn main() -> io::Result<()> {
    let mut out = io::stdout().lock();
    for license in two_face::acknowledgement::listing().for_syntaxes() {
        writeln!(out, "{}\n", license.rel_path.display())?;
        writeln!(out, "{}\n", license.text.trim_end())?;
        writeln!(out, "{}\n", "-".repeat(80))?;
    }
    Ok(())
}
