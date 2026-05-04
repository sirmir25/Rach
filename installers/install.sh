#!/usr/bin/env bash
# Rach installer for Linux / macOS / BSD.
#
# Downloads a pre-built binary from GitHub Releases — no Rust toolchain required.
#
# Usage (one-liner):
#   curl -fsSL https://raw.githubusercontent.com/sirmir25/Rach/main/installers/install.sh | bash
#
# Usage (from a checkout):
#   ./installers/install.sh [PREFIX]   (default PREFIX=/usr/local)
#
# Env:
#   RACH_VERSION   pin a release tag (e.g. v0.2.0). Default: latest.
#   RACH_REPO      override repo (default sirmir25/Rach).
#   RACH_FROM_SOURCE=1   force `cargo build` instead of downloading a binary.

set -euo pipefail

REPO="${RACH_REPO:-sirmir25/Rach}"
VERSION="${RACH_VERSION:-latest}"
PREFIX="${1:-/usr/local}"
BIN_DIR="$PREFIX/bin"

say()  { printf '\033[1;36m==>\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33m!!\033[0m  %s\n' "$*" >&2; }
die()  { printf '\033[1;31mxx\033[0m  %s\n' "$*" >&2; exit 1; }

detect_target() {
    local os arch
    case "$(uname -s)" in
        Linux)   os=linux ;;
        Darwin)  os=darwin ;;
        FreeBSD|OpenBSD|NetBSD)
            warn "no pre-built binary for $(uname -s); falling back to source build"
            RACH_FROM_SOURCE=1
            return
            ;;
        *) die "unsupported OS: $(uname -s)" ;;
    esac
    case "$(uname -m)" in
        x86_64|amd64)    arch=x86_64 ;;
        arm64|aarch64)   arch=aarch64 ;;
        *) die "unsupported arch: $(uname -m)" ;;
    esac
    case "$os-$arch" in
        linux-x86_64)   TARGET=x86_64-unknown-linux-musl ;;
        linux-aarch64)  TARGET=aarch64-unknown-linux-musl ;;
        darwin-x86_64)  TARGET=x86_64-apple-darwin ;;
        darwin-aarch64) TARGET=aarch64-apple-darwin ;;
        *) die "unsupported combo: $os/$arch" ;;
    esac
}

fetch_to() {
    local url="$1" out="$2"
    if command -v curl >/dev/null 2>&1; then
        curl -fsSL --proto '=https' --tlsv1.2 "$url" -o "$out"
    elif command -v wget >/dev/null 2>&1; then
        wget -qO "$out" "$url"
    else
        die "need curl or wget to download the binary"
    fi
}

install_binary() {
    local src="$1"
    say "Installing to $BIN_DIR/rach"
    if [ -w "$BIN_DIR" ] || [ "$(id -u)" -eq 0 ]; then
        install -d "$BIN_DIR"
        install -m 0755 "$src" "$BIN_DIR/rach"
    else
        warn "$BIN_DIR is not writable — using sudo"
        sudo install -d "$BIN_DIR"
        sudo install -m 0755 "$src" "$BIN_DIR/rach"
    fi
}

install_from_release() {
    detect_target
    [ -n "${TARGET:-}" ] || return 1

    local asset="rach-$TARGET.tar.gz"
    local url
    if [ "$VERSION" = "latest" ]; then
        url="https://github.com/$REPO/releases/latest/download/$asset"
    else
        url="https://github.com/$REPO/releases/download/$VERSION/$asset"
    fi

    local tmp
    tmp="$(mktemp -d)"
    trap 'rm -rf "$tmp"' EXIT

    say "Downloading $asset…"
    if ! fetch_to "$url" "$tmp/$asset"; then
        warn "download failed: $url"
        return 1
    fi

    say "Extracting…"
    tar xzf "$tmp/$asset" -C "$tmp"
    local bin
    bin="$(find "$tmp" -type f -name rach -perm -u+x | head -n1)"
    [ -n "$bin" ] && [ -x "$bin" ] || { warn "archive missing rach binary"; return 1; }

    install_binary "$bin"
    return 0
}

install_from_source() {
    command -v cargo >/dev/null 2>&1 || die "no pre-built binary available and cargo not on PATH (install Rust from https://rustup.rs or set RACH_VERSION)"
    local repo_root
    repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." 2>/dev/null && pwd)" \
        || die "RACH_FROM_SOURCE=1 requires running from a checkout, not via curl|bash"
    [ -f "$repo_root/Cargo.toml" ] || die "Cargo.toml not found at $repo_root — run from a checkout"

    say "Building Rach from source (release)…"
    ( cd "$repo_root" && cargo build --release )
    local src="$repo_root/target/release/rach"
    [ -x "$src" ] || die "build did not produce $src"
    install_binary "$src"
}

main() {
    if [ "${RACH_FROM_SOURCE:-0}" = "1" ]; then
        install_from_source
    else
        if ! install_from_release; then
            warn "falling back to source build"
            install_from_source
        fi
    fi

    say "Verifying…"
    "$BIN_DIR/rach" version
    say "Installed. Try:  rach examples/hello.rach"
}

main "$@"
