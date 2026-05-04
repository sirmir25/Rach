# Rach installers

All installers download a pre-built binary from GitHub Releases for your platform — **no Rust toolchain required**. They fall back to building from source via `cargo` only if the download fails (or if you set `RACH_FROM_SOURCE=1`).

## One-liners

```bash
# macOS / Linux
curl -fsSL https://raw.githubusercontent.com/sirmir25/Rach/main/installers/install.sh | bash
```

```powershell
# Windows (PowerShell)
iwr -useb https://raw.githubusercontent.com/sirmir25/Rach/main/installers/install.ps1 | iex
```

## From a checkout

| Installer        | Platform              | Run                                                                      |
|------------------|-----------------------|--------------------------------------------------------------------------|
| `install.sh`     | Linux / macOS / BSD   | `./installers/install.sh [PREFIX]` (default `/usr/local`)               |
| `install.ps1`    | Windows (PowerShell)  | `powershell -ExecutionPolicy Bypass -File installers\install.ps1`       |
| `install.bat`    | Windows (cmd.exe)     | `installers\install.bat [INSTALL_DIR]` (default `%LOCALAPPDATA%\Programs\rach`) |
| `install.py`     | Cross-platform        | `python3 installers/install.py [DIR]`                                   |
| `install.c`      | Cross-platform (C99)  | `cc installers/install.c -o /tmp/ri && /tmp/ri [DIR]`                   |
| `install.cpp`    | Cross-platform (C++17)| `c++ -std=c++17 installers/install.cpp -o /tmp/ri && /tmp/ri [DIR]`     |

## Pre-built binary targets

Releases publish archives for:

- `x86_64-unknown-linux-musl` (statically linked, glibc-free)
- `aarch64-unknown-linux-musl`
- `x86_64-apple-darwin`
- `aarch64-apple-darwin`
- `x86_64-pc-windows-msvc`

## Environment variables

| Variable             | Effect                                                       |
|----------------------|--------------------------------------------------------------|
| `RACH_VERSION`       | Pin a release tag (e.g. `v0.2.0`). Default: `latest`.       |
| `RACH_REPO`          | Override the GitHub repo (default `sirmir25/Rach`).         |
| `RACH_FROM_SOURCE=1` | Force `cargo build --release` instead of downloading a binary. Requires a checkout and `cargo` on PATH. |
| `RACH_INSTALL_DIR`   | Windows PowerShell installer: override the install directory. |

## Permissions

If the install path needs root (`/usr/local/bin`, `Program Files`), the Unix installers re-run the copy step under `sudo`. The Windows PowerShell installer defaults to `%LOCALAPPDATA%\Programs\rach`, which doesn't require Administrator rights, and it auto-adds that directory to your user PATH.
