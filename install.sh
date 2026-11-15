#!/bin/sh
# Installs BIA on Linux or macOS.
#
#   curl -fsSL https://raw.githubusercontent.com/thdtr/bia/main/install.sh | sh
#
# Options (after `sh -s --` when piping):
#   --uninstall        remove BIA
#   --version vX.Y.Z   install a specific release instead of the latest
#
# Environment:
#   BIA_PREFIX         where to install (default: ~/.local, binary goes in $BIA_PREFIX/bin)
#
# Nothing here needs root. Everything lands in your home folder.

set -eu

REPO="thdtr/bia"
PREFIX="${BIA_PREFIX:-$HOME/.local}"
BIN_DIR="$PREFIX/bin"
VERSION="latest"
ACTION="install"

while [ $# -gt 0 ]; do
    case "$1" in
        --uninstall) ACTION="uninstall" ;;
        --version) shift; VERSION="${1:?--version needs a value, e.g. v0.1.0}" ;;
        -h|--help) sed -n '2,14p' "$0" 2>/dev/null || true; exit 0 ;;
        *) echo "bia: unknown option: $1" >&2; exit 1 ;;
    esac
    shift
done

say() { printf '%s\n' "$*"; }
die() { printf 'bia: %s\n' "$*" >&2; exit 1; }

OS="$(uname -s)"
case "$OS" in
    Linux) PLATFORM="unknown-linux-gnu" ;;
    Darwin) PLATFORM="apple-darwin" ;;
    *) die "this script supports Linux and macOS. On Windows, use install.ps1." ;;
esac

case "$(uname -m)" in
    x86_64|amd64) ARCH="x86_64" ;;
    aarch64|arm64) ARCH="aarch64" ;;
    *) die "no pre-built BIA for $(uname -m) yet. You can build from source: cargo install --git https://github.com/$REPO bia" ;;
esac

TARGET="$ARCH-$PLATFORM"
APPS_DIR="$HOME/.local/share/applications"
ICON_DIR="$HOME/.local/share/icons/hicolor/scalable/apps"
MAC_APP="$HOME/Applications/BIA.app"

uninstall() {
    rm -f "$BIN_DIR/bia"
    if [ "$OS" = "Linux" ]; then
        rm -f "$APPS_DIR/bia.desktop" "$ICON_DIR/bia.svg"
        command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database "$APPS_DIR" 2>/dev/null || true
    else
        rm -rf "$MAC_APP"
    fi
    say "BIA was removed. Your settings are still in ~/.config/bia (Linux) or ~/Library/Application Support/bia (macOS)."
}

if [ "$ACTION" = "uninstall" ]; then
    uninstall
    exit 0
fi

if command -v curl >/dev/null 2>&1; then
    fetch() { curl -fsSL "$1" -o "$2"; }
elif command -v wget >/dev/null 2>&1; then
    fetch() { wget -qO "$2" "$1"; }
else
    die "you need curl or wget"
fi

if command -v sha256sum >/dev/null 2>&1; then
    sha256() { sha256sum "$1" | cut -d' ' -f1; }
else
    sha256() { shasum -a 256 "$1" | cut -d' ' -f1; }
fi

if [ "$VERSION" = "latest" ]; then
    BASE="https://github.com/$REPO/releases/latest/download"
else
    BASE="https://github.com/$REPO/releases/download/$VERSION"
fi
ARCHIVE="bia-$TARGET.tar.gz"

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

say "Downloading BIA ($TARGET)..."
fetch "$BASE/$ARCHIVE" "$TMP/$ARCHIVE" || die "download failed: $BASE/$ARCHIVE"
fetch "$BASE/$ARCHIVE.sha256" "$TMP/$ARCHIVE.sha256" || die "checksum download failed"

expected="$(cut -d' ' -f1 < "$TMP/$ARCHIVE.sha256")"
actual="$(sha256 "$TMP/$ARCHIVE")"
[ "$expected" = "$actual" ] || die "checksum mismatch, refusing to install (expected $expected, got $actual)"

tar -xzf "$TMP/$ARCHIVE" -C "$TMP"
SRC="$TMP/bia-$TARGET"
mkdir -p "$BIN_DIR"

if [ "$OS" = "Linux" ]; then
    install -m 755 "$SRC/bia" "$BIN_DIR/bia"
    mkdir -p "$APPS_DIR" "$ICON_DIR"
    cp "$SRC/bia.svg" "$ICON_DIR/bia.svg"
    # Absolute path, so the menu entry works even if ~/.local/bin is not on PATH.
    sed "s|^Exec=bia|Exec=$BIN_DIR/bia|" "$SRC/bia.desktop" > "$APPS_DIR/bia.desktop"
    command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database "$APPS_DIR" 2>/dev/null || true
    say "Installed $BIN_DIR/bia and added BIA to your app menu."
else
    mkdir -p "$HOME/Applications"
    rm -rf "$MAC_APP"
    cp -R "$SRC/BIA.app" "$MAC_APP"
    xattr -dr com.apple.quarantine "$MAC_APP" 2>/dev/null || true
    ln -sf "$MAC_APP/Contents/MacOS/bia" "$BIN_DIR/bia"
    say "Installed BIA.app in ~/Applications and the bia command in $BIN_DIR."
fi

case ":$PATH:" in
    *":$BIN_DIR:"*) say "Run it with: bia <folder>" ;;
    *)
        say ""
        say "$BIN_DIR isn't on your PATH yet. Add this line to your ~/.bashrc or ~/.zshrc:"
        say "    export PATH=\"$BIN_DIR:\$PATH\""
        ;;
esac
