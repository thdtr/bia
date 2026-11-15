#!/bin/sh
# Writes the licenses of everything BIA's binary includes from other projects:
# Rust crates, syntax definitions and fonts. Release packages ship the result.
#
#   tools/third_party_licenses.sh [OUTPUT]    (default: THIRD-PARTY-LICENSES.txt)
#
# Needs cargo-about: cargo install --locked --features cli cargo-about

set -eu
cd "$(dirname "$0")/.."
out="${1:-THIRD-PARTY-LICENSES.txt}"

heading() {
    printf '\n%s\n%s\n%s\n\n' \
        "================================================================================" \
        "$1" \
        "================================================================================"
}

{
    echo "BIA includes work from other projects. Their licenses follow."
    heading "Rust crates"
    cargo about generate --locked --fail about.hbs
    heading "Syntax definitions (from bat, through the two-face crate)"
    cargo run --locked --quiet -p bia-core --example grammar_licenses
    heading "Fonts"
    cat assets/fonts/OFL-FiraSans.txt
    echo
    cat assets/fonts/OFL-FiraCode.txt
} > "$out"

echo "Wrote $out"
