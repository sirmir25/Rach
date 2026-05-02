#!/usr/bin/env bash
# Rach installer for Linux / macOS / BSD.
# Usage: ./installers/install.sh [PREFIX]   (default PREFIX=/usr/local)

set -euo pipefail

PREFIX="${1:-/usr/local}"
BIN_DIR="$PREFIX/bin"
REPO_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"

say()  { printf '\033[1;36m==>\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33m!!\033[0m  %s\n' "$*" >&2; }
die()  { printf '\033[1;31mxx\033[0m  %s\n' "$*" >&2; exit 1; }

command -v cargo >/dev/null 2>&1 || die "cargo not found in PATH. Install Rust from https://rustup.rs"

say "Building Rach (release)…"
( cd "$REPO_ROOT" && cargo build --release )

SRC_BIN="$REPO_ROOT/target/release/rach"
[ -x "$SRC_BIN" ] || die "build did not produce $SRC_BIN"

say "Installing to $BIN_DIR/rach"
if [ -w "$BIN_DIR" ] || [ "$(id -u)" -eq 0 ]; then
    install -d "$BIN_DIR"
    install -m 0755 "$SRC_BIN" "$BIN_DIR/rach"
else
    warn "$BIN_DIR is not writable — using sudo"
    sudo install -d "$BIN_DIR"
    sudo install -m 0755 "$SRC_BIN" "$BIN_DIR/rach"
fi

say "Verifying…"
"$BIN_DIR/rach" version
say "Installed. Try:  rach examples/hello.rach"
