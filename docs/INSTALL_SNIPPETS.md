# Install Snippets

Copy-paste ready blocks for the install section of the website. Each block is what the user actually pastes into their terminal. Show them in tabs, in this order. Every snippet assumes the user has already run `git clone https://github.com/sirmir25/Rach.git && cd Rach` (the very first tab makes that explicit; later tabs can omit it).

---

## Tab 1 — macOS / Linux / BSD (bash)

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

## Tab 2 — Windows (cmd.exe, run as Administrator)

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

## Tab 3 — Cross-platform (Python 3)

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

---

## Tab 4 — C99 (any POSIX or MSVC)

```bash
cc installers/install.c -o /tmp/rach-install
/tmp/rach-install
```

Windows MSVC:
```bat
cl installers\install.c /Fe:rach-install.exe
rach-install.exe
```

---

## Tab 5 — C++17

```bash
c++ -std=c++17 installers/install.cpp -o /tmp/rach-install
/tmp/rach-install
```

Windows MSVC:
```bat
cl /std:c++17 installers\install.cpp /Fe:rach-install.exe
rach-install.exe
```

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

> All five installers do the same thing: build the interpreter from source via `cargo build --release` and copy the resulting binary to a system path. They require the Rust toolchain — install from <https://rustup.rs>.
>
> Once installed, **the binary has no runtime dependencies**. End users never need Rust to run `.rach` scripts.
