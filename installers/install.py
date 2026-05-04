#!/usr/bin/env python3
"""Rach installer (cross-platform).

Downloads a pre-built binary from GitHub Releases. No Rust toolchain required.

Usage:
    python3 installers/install.py            # default install dir
    python3 installers/install.py /opt       # custom prefix on Unix
    python3 installers/install.py "C:\\Tools\\rach"   # custom dir on Windows

Env:
    RACH_VERSION       pin a release tag (e.g. v0.2.0). Default: latest.
    RACH_REPO          override repo (default sirmir25/Rach).
    RACH_FROM_SOURCE=1 force `cargo build` instead of downloading a binary.
"""

from __future__ import annotations

import io
import os
import platform
import shutil
import subprocess
import sys
import tarfile
import tempfile
import urllib.error
import urllib.request
import zipfile
from pathlib import Path

REPO    = os.environ.get("RACH_REPO", "sirmir25/Rach")
VERSION = os.environ.get("RACH_VERSION", "latest")
FROM_SOURCE = os.environ.get("RACH_FROM_SOURCE") == "1"


def say(msg: str) -> None:
    print(f"\033[1;36m==>\033[0m {msg}")


def warn(msg: str) -> None:
    print(f"\033[1;33m!!\033[0m  {msg}", file=sys.stderr)


def die(msg: str, code: int = 1) -> None:
    print(f"\033[1;31mxx\033[0m  {msg}", file=sys.stderr)
    sys.exit(code)


def detect_target() -> str | None:
    sysname = platform.system()
    machine = platform.machine().lower()

    if machine in ("x86_64", "amd64"):
        arch = "x86_64"
    elif machine in ("arm64", "aarch64"):
        arch = "aarch64"
    else:
        return None

    if sysname == "Linux":
        return f"{arch}-unknown-linux-musl"
    if sysname == "Darwin":
        return f"{arch}-apple-darwin"
    if sysname == "Windows":
        # No native ARM64 build yet — the x86_64 binary works under emulation.
        return "x86_64-pc-windows-msvc"
    return None


def asset_url(target: str) -> tuple[str, str]:
    is_zip = target.endswith("pc-windows-msvc")
    name = f"rach-{target}.{'zip' if is_zip else 'tar.gz'}"
    if VERSION == "latest":
        url = f"https://github.com/{REPO}/releases/latest/download/{name}"
    else:
        url = f"https://github.com/{REPO}/releases/download/{VERSION}/{name}"
    return url, name


def download(url: str, dst: Path) -> None:
    say(f"Downloading {dst.name}…")
    req = urllib.request.Request(url, headers={"User-Agent": "rach-installer"})
    with urllib.request.urlopen(req) as r, dst.open("wb") as f:
        shutil.copyfileobj(r, f)


def extract(archive: Path, into: Path) -> Path:
    say("Extracting…")
    if archive.suffix == ".zip":
        with zipfile.ZipFile(archive) as z:
            z.extractall(into)
    else:
        with tarfile.open(archive, "r:gz") as t:
            t.extractall(into)
    bin_name = "rach.exe" if os.name == "nt" else "rach"
    for path in into.rglob(bin_name):
        if path.is_file():
            return path
    raise FileNotFoundError(f"archive missing {bin_name}")


def default_install_dir() -> Path:
    if os.name == "nt":
        base = os.environ.get("LOCALAPPDATA") or os.path.expanduser("~")
        return Path(base) / "Programs" / "rach"
    return Path("/usr/local/bin")


def install_binary(src: Path, dst_dir: Path) -> Path:
    is_win = os.name == "nt"
    bin_name = "rach.exe" if is_win else "rach"
    dst = dst_dir / bin_name
    say(f"Installing to {dst}")
    dst_dir.mkdir(parents=True, exist_ok=True)

    try:
        shutil.copy2(src, dst)
        if not is_win:
            dst.chmod(0o755)
    except PermissionError:
        if is_win:
            die("permission denied — try running from an Administrator cmd, "
                "or pick a writable directory like %LOCALAPPDATA%\\Programs\\rach")
        warn(f"{dst_dir} is not writable — retrying with sudo")
        subprocess.run(["sudo", "install", "-m", "0755", str(src), str(dst)], check=True)

    return dst


def install_from_release(dst_dir: Path) -> Path | None:
    target = detect_target()
    if not target:
        warn(f"no pre-built binary for {platform.system()}/{platform.machine()}")
        return None

    url, asset = asset_url(target)
    with tempfile.TemporaryDirectory() as tmp:
        tmp_path = Path(tmp)
        archive = tmp_path / asset
        try:
            download(url, archive)
        except urllib.error.URLError as e:
            warn(f"download failed: {url} ({e})")
            return None
        try:
            bin_path = extract(archive, tmp_path)
        except (FileNotFoundError, tarfile.TarError, zipfile.BadZipFile) as e:
            warn(f"extract failed: {e}")
            return None
        return install_binary(bin_path, dst_dir)


def install_from_source(dst_dir: Path) -> Path:
    if shutil.which("cargo") is None:
        die("no pre-built binary available and cargo not on PATH "
            "(install Rust from https://rustup.rs or pin RACH_VERSION)")
    repo_root = Path(__file__).resolve().parent.parent
    if not (repo_root / "Cargo.toml").exists():
        die("RACH_FROM_SOURCE=1 requires running from a checkout, "
            "not via a piped one-liner")
    say("Building Rach from source (release)…")
    subprocess.run(["cargo", "build", "--release"], cwd=repo_root, check=True)
    bin_name = "rach.exe" if os.name == "nt" else "rach"
    src = repo_root / "target" / "release" / bin_name
    if not src.exists():
        die(f"build did not produce {src}")
    return install_binary(src, dst_dir)


def main() -> None:
    install_dir = Path(sys.argv[1]) if len(sys.argv) > 1 else default_install_dir()

    dst: Path | None = None
    if not FROM_SOURCE:
        dst = install_from_release(install_dir)
        if dst is None:
            warn("falling back to source build")
    if dst is None:
        dst = install_from_source(install_dir)

    say("Verifying…")
    subprocess.run([str(dst), "version"], check=True)
    sep = os.sep
    say(f"Installed. Try:  rach examples{sep}hello.rach")


if __name__ == "__main__":
    try:
        main()
    except subprocess.CalledProcessError as e:
        die(f"command failed: {e}")
    except KeyboardInterrupt:
        die("interrupted", 130)
