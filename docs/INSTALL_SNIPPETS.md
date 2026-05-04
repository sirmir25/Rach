# Install Snippets

Copy-paste ready blocks for the install section of the website. **No Rust toolchain required** — every installer downloads a pre-built binary from GitHub Releases. Show them in tabs, in this order.

---

## Tab 1 — One-liner (macOS / Linux)

```bash
curl -fsSL https://raw.githubusercontent.com/sirmir25/Rach/main/installers/install.sh | bash
```

Custom prefix:
```bash
curl -fsSL https://raw.githubusercontent.com/sirmir25/Rach/main/installers/install.sh | bash -s -- /opt
```

Pin a version:
```bash
curl -fsSL https://raw.githubusercontent.com/sirmir25/Rach/main/installers/install.sh | RACH_VERSION=v0.2.0 bash
```

---

## Tab 2 — One-liner (Windows / PowerShell)

```powershell
iwr -useb https://raw.githubusercontent.com/sirmir25/Rach/main/installers/install.ps1 | iex
```

Default install dir is `%LOCALAPPDATA%\Programs\rach` — no Administrator required, and the script adds it to your user `PATH`.

---

## Tab 3 — From a checkout (bash)

```bash
git clone https://github.com/sirmir25/Rach.git
cd Rach
./installers/install.sh
```

Custom prefix:
```bash
./installers/install.sh /opt
```

---

## Tab 4 — Windows (cmd.exe)

```bat
git clone https://github.com/sirmir25/Rach.git
cd Rach
installers\install.bat
```

Custom directory:
```bat
installers\install.bat "C:\Tools\rach"
```

---

## Tab 5 — Cross-platform (Python 3)

```bash
python3 installers/install.py
```

Custom directory:
```bash
python3 installers/install.py /opt
```

```bat
python3 installers\install.py "C:\Tools\rach"
```

Uses only the Python standard library — no `pip install` step.

---

## Tab 6 — C99 / C++17 (any POSIX or MSVC)

```bash
cc installers/install.c -o /tmp/rach-install
/tmp/rach-install
```

```bash
c++ -std=c++17 installers/install.cpp -o /tmp/rach-install
/tmp/rach-install
```

Windows MSVC:
```bat
cl installers\install.c /Fe:rach-install.exe && rach-install.exe
cl /std:c++17 installers\install.cpp /Fe:rach-install.exe && rach-install.exe
```

These shell out to system `curl` (POSIX) or `Invoke-WebRequest` (Windows) for the download.

---

## Verify

```bash
rach version
# rach 0.2.0
```

Open the REPL:

```bash
rach
```

Or run a script:

```bash
rach examples/short.rach
```

---

## Prerequisites note (sidebar copy)

> Every installer downloads a pre-built, statically-linked binary from GitHub Releases for your platform — Linux (x86_64/aarch64, musl), macOS (x86_64/aarch64), and Windows (x86_64). No Rust toolchain is needed, and the binary has no runtime dependencies.
>
> If you're on a platform we don't ship a binary for, set `RACH_FROM_SOURCE=1` to build via `cargo` instead (requires the Rust toolchain from <https://rustup.rs>).
