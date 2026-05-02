# Rach installers

Five interchangeable installers — pick the one your environment already has. All do the same thing: check `cargo`, build `--release`, copy the binary to a system path, verify with `rach version`.

| Installer        | Platform              | Run                                                |
|------------------|-----------------------|----------------------------------------------------|
| `install.sh`     | Linux / macOS / BSD   | `./installers/install.sh [PREFIX]` (default `/usr/local`) |
| `install.bat`    | Windows (cmd.exe)     | `installers\install.bat [INSTALL_DIR]` (default `%ProgramFiles%\rach`) |
| `install.py`     | Cross-platform        | `python3 installers/install.py [DIR]`              |
| `install.c`      | Cross-platform (C99)  | `cc installers/install.c -o /tmp/ri && /tmp/ri [DIR]` |
| `install.cpp`    | Cross-platform (C++17)| `c++ -std=c++17 installers/install.cpp -o /tmp/ri && /tmp/ri [DIR]` |

Prerequisites: Rust toolchain on PATH (`cargo`). Get it from <https://rustup.rs>.

If the install path needs root (`/usr/local/bin`, `Program Files`), the installer will retry with `sudo` (Unix) or print a hint to re-run as Administrator (Windows).
