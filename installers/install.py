#!/usr/bin/env python3
"""Rach installer (cross-platform).

Usage:
    python3 installers/install.py            # default prefix
    python3 installers/install.py /opt       # custom prefix on Unix
    python3 installers/install.py "C:\\Tools\\rach"   # custom dir on Windows

Requires: cargo on PATH (https://rustup.rs).
"""

from __future__ import annotations

import os
import shutil
import subprocess
import sys
from pathlib import Path


def say(msg: str) -> None:
    print(f"\033[1;36m==>\033[0m {msg}")


def warn(msg: str) -> None:
    print(f"\033[1;33m!!\033[0m  {msg}", file=sys.stderr)


def die(msg: str, code: int = 1) -> None:
    print(f"\033[1;31mxx\033[0m  {msg}", file=sys.stderr)
    sys.exit(code)


def main() -> None:
    repo_root = Path(__file__).resolve().parent.parent
    is_win = os.name == "nt"

    if shutil.which("cargo") is None:
        die("cargo not found in PATH. Install Rust from https://rustup.rs")

    if len(sys.argv) > 1:
        install_dir = Path(sys.argv[1])
    elif is_win:
        install_dir = Path(os.environ.get("ProgramFiles", "C:\\Program Files")) / "rach"
    else:
        install_dir = Path("/usr/local/bin")

    say("Building Rach (release)...")
    subprocess.run(["cargo", "build", "--release"], cwd=repo_root, check=True)

    bin_name = "rach.exe" if is_win else "rach"
    src_bin = repo_root / "target" / "release" / bin_name
    if not src_bin.exists():
        die(f"build did not produce {src_bin}")

    dst_bin = install_dir / bin_name
    say(f"Installing to {dst_bin}")
    install_dir.mkdir(parents=True, exist_ok=True)

    try:
        shutil.copy2(src_bin, dst_bin)
        if not is_win:
            dst_bin.chmod(0o755)
    except PermissionError:
        if is_win:
            die("permission denied — try running from an Administrator cmd")
        warn(f"{install_dir} is not writable — retrying with sudo")
        subprocess.run(["sudo", "install", "-m", "0755", str(src_bin), str(dst_bin)], check=True)

    say("Verifying...")
    subprocess.run([str(dst_bin), "version"], check=True)
    say(f"Installed. Try:  rach examples{os.sep}hello.rach")


if __name__ == "__main__":
    try:
        main()
    except subprocess.CalledProcessError as e:
        die(f"command failed: {e}")
