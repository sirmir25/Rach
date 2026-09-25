# Rach

> Write simply — run anywhere.

Rach is a small scripting language focused on automation: system commands, files, browsers (via real W3C WebDriver), generation of bash and code in other languages. The interpreter is written in Rust, a single statically-linked binary, with no runtime dependencies except `curl`/`tar`/`unzip` for auto-installing web drivers.

---

## Contents

- [Installation](#installation)
- [Hello, world](#hello-world)
- [Script structure](#script-structure)
- [Modules (import)](#modules-import)
- [Standard library commands](#standard-library-commands)
  - [os / system](#os--system)
  - [Files](#files)
  - [run_command / install_package](#run_command--install_package)
  - [Browser (WebDriver)](#browser-webdriver)
  - [bash DSL](#bash-dsl)
  - [ai_generate](#ai_generate)
- [Flow control: `if linux/macos/windows`](#flow-control-if-linuxmacoswindows)
- [Error convention](#error-convention)
- [Environment variables](#environment-variables)
- [Building from source](#building-from-source)
- [CLI](#cli)
- [Grammar (formal)](#grammar-formal)
- [Limitations and non-goals](#limitations-and-non-goals)
- [License](#license)

---

## Installation

**No Rust toolchain required.** The installers download a pre-built binary from GitHub Releases for your platform.

One-liners:

```bash
# macOS / Linux
curl -fsSL https://raw.githubusercontent.com/sirmir25/Rach/main/installers/install.sh | bash
```

```powershell
# Windows (PowerShell)
iwr -useb https://raw.githubusercontent.com/sirmir25/Rach/main/installers/install.ps1 | iex
```

From a checkout — pick whichever installer matches your environment:

```bash
git clone https://github.com/sirmir25/Rach.git
cd Rach

# Linux / macOS / BSD
./installers/install.sh

# Windows (cmd.exe — defaults to %LOCALAPPDATA%\Programs\rach, no Admin needed)
installers\install.bat

# Cross-platform (Python 3, no extra deps)
python3 installers/install.py

# Cross-platform (C99 or C++17)
cc installers/install.c -o /tmp/ri && /tmp/ri
c++ -std=c++17 installers/install.cpp -o /tmp/ri && /tmp/ri
```

Each installer takes an optional install dir as its first argument. Pin a specific version with `RACH_VERSION=v0.2.0`. See [`installers/README.md`](installers/README.md) for details.

Pre-built archives are published for `x86_64`/`aarch64` Linux (musl), `x86_64`/`aarch64` macOS, and `x86_64` Windows.

Building from source (only if you want to hack on the interpreter, or you're on a platform we don't ship binaries for):

```bash
cargo build --release
sudo ln -s "$PWD/target/release/rach" /usr/local/bin/rach
rach version
```

Or force any installer to build from source: `RACH_FROM_SOURCE=1 ./installers/install.sh`.

For browser automation you also need one of:
- Chrome or Chromium (then `chromedriver` is downloaded automatically)
- Firefox (then `geckodriver` is downloaded automatically)
- Microsoft Edge with `msedgedriver` already installed
- Safari + `safaridriver --enable` + "Allow Remote Automation" checkbox in the Develop menu

`curl`, `tar`, `unzip` must be in PATH (available out of the box on macOS/Linux).

---

## Hello, world

The minimal `.rach` file is just commands, top to bottom — no `main`, no `import`, no end markers:

```
print("hello, rach")
write("/tmp/hello.txt", "hi")
read("/tmp/hello.txt")
```

Run: `rach hello.rach`.

Helper functions can sit before or after the body:

```
rach square(x):
    return x * x
end

print(square(7))   # 49
```

The legacy wrapped form `rach main(0) ... return(end) (end0)` still parses for back-compat, but new code shouldn't use it.

---

## Script structure

A `.rach` file is a mix of:

- **Top-level statements** — bare commands and expressions; collected, in source order, into an implicit `main`.
- **Function defs** — `rach name(params): ... end`. May appear anywhere at the top level.
- **Struct defs** — `struct Name { field1, field2 }`.
- **Imports** — declarative-only, see below.

Rules:

- `import` lines are optional; the stdlib is always available. Imports serve as documentation of intent.
- Comments: `#` or `//` until end of line.
- Indentation is significant only inside `if`/`for`/`while`/`else`/function blocks.
- The `set` keyword is optional: `x = 5` works the same as `set x = 5`.
- The `completed` keyword is also optional — every command prints `completed` on success automatically.
- Strings: `"..."` is a plain literal — `{` and `}` are literal characters. Use `f"hello, {name}"` to interpolate expressions, Python-style.

---

## Modules (import)

Imports are declarative. They don't load code (the standard library is always compiled into the interpreter), but serve as documentation of intent. Unknown modules trigger a warning, but not an error.

| Module       | What it declares                                  |
|--------------|---------------------------------------------------|
| `os`         | `if linux/macos/windows` checks                   |
| `system`     | files, `run_command`, `install_package`, `reboot` |
| `web`        | browser automation                                |
| `browser`    | alias for `web` (semantic alias)                  |
| `linux`      | OS-specific namespace                             |
| `windows`    | OS-specific namespace                             |
| `macos`      | OS-specific namespace                             |
| `bash`       | `bash = generate ...` DSL                         |
| `ai`         | `ai_generate(...)`                                |
| `ascii`      | ASCII-art generators (banner, box, table, etc.)   |

---

## Variables and user functions

`set NAME = <expr>` captures a value. Right-hand side can be a literal, a list, a command call, or another variable.

```
set message = read_file("/tmp/notes.txt")
set urls = ["https://a", "https://b", "https://c"]
```

When a variable is on the RHS of `set`, the command runs in **capturing mode** — it returns its result without printing the usual side-effect output.

User functions are declared with named params and may `return <expr>`:

```
rach square(x):
    return x * x
end
```

Call them like commands the parser doesn't already know: `y = square(7)`.

Default parameters use the C++ `name = expr` syntax:

```
rach greet(name, greeting = "hello"):
    return greeting + ", " + name
end

greet("rach")               # "hello, rach"
greet("rach", "hi")         # "hi, rach"
```

---

## Loops: `for x in <expr>:`

Iterate over a list literal, a captured list variable, or a non-negative integer (which yields `0..N`):

```
for url in ["https://one", "https://two"]:
    open in browser(url)

urls = ["a", "b", "c"]
for u in urls:
    run("echo visited")

for i in 3:
    write("/tmp/file", "x")
```

Strings split on commas: `for tag in "a,b,c":` yields `a`, `b`, `c`.

---

## Conditionals: `if`, `if not`, `else`

```
if linux:
    run_command("apt-get update")
else:
    run_command("brew update")

if not windows:
    run_command("uname -a")
```

Only OS checks are supported (still no general-purpose conditions). Combine `if` + `else` and `if not`/`else` to cover all branches.

---

## Standard library commands

Commands in Rach are written "in English": several words in a row form the command name, parentheses are arguments. Example:

```
open in browser("https://example.com")   // → open_in_browser("https://...")
fill form id("login") value("ilia")      // → fill_form, kwargs id=..., value=...
wait seconds(3)                          // → wait_seconds(3)
```

The command name is resolved by the interpreter: it searches for the longest prefix of words matching a known command. The remaining words + their `(...)` become keyword arguments.

### Short aliases

The most common stdlib commands have one-word aliases — pick whichever feels shorter:

| Short          | Canonical                |
|----------------|--------------------------|
| `read(path)`   | `read_file(path)`        |
| `write(p, c)`  | `create_file(p, c)`      |
| `exists(path)` | `check_if_exists(path)`  |
| `del(path)` / `rm(path)` | `delete_file(path)` |
| `run(cmd)` / `sh(cmd)`   | `run_command(cmd)`  |
| `print(x)` / `echo(x)`   | line print to stdout |

### system

| Command                  | What it does                                                    |
|--------------------------|------------------------------------------------------------------|
| `reboot()`               | Prints reboot intent (without executing — for safety)           |
| `shutdown()`             | Same, no execution                                              |

### Files

| Command                                     | Effect                                |
|---------------------------------------------|----------------------------------------|
| `create_file("/path", "content")`           | Creates a file, overwrites if exists  |
| `read_file("/path")`                        | Prints the contents                   |
| `edit_file("/path", "new content")`         | Overwrites                            |
| `delete_file("/path")`                      | Deletes                               |
| `check_if_exists("/path")`                  | Prints `exists` or `missing`          |

### run_command / install_package

```
run_command("ls -la /tmp")
install_package("htop")
```

`run_command` runs the command via `sh -c` (on Windows — `cmd /C`) and prints stdout/stderr.

`install_package` picks a package manager based on the OS:

| OS      | Command                                       |
|---------|-----------------------------------------------|
| macOS   | `brew install <pkg>`                          |
| Linux   | `apt-get` / `dnf` / `pacman` / `zypper` / `apk` (under sudo) |
| Windows | `winget install --silent <pkg>`               |
| BSD     | `pkg install -y <pkg>`                        |

Installation actually runs. To run only in "what would be done" mode:

```bash
RACH_DRY_RUN=1 rach install.rach
```

### Browser (WebDriver)

All browser commands go through real [W3C WebDriver](https://www.w3.org/TR/webdriver2/) (HTTP protocol). The driver is started automatically on the first browser command:

1. If `chromedriver`/`geckodriver`/`msedgedriver` is in PATH — it is used.
2. Otherwise it's looked for in the cache `~/Library/Caches/rach/drivers/` (on Linux — `~/.cache/rach/drivers/`).
3. Otherwise it is downloaded:
   - `chromedriver` — via the [Chrome for Testing API](https://googlechromelabs.github.io/chrome-for-testing/), if Chrome/Chromium is installed.
   - `geckodriver` v0.36.0 from GitHub Releases, if Firefox is installed.

Command list:

| Command                                              | What it does                                        |
|------------------------------------------------------|------------------------------------------------------|
| `open in browser("url")`                             | Launch any available browser and open URL           |
| `open in chrome("url")` / `firefox` / `edge` / `safari` | Force a specific browser                         |
| `navigate to("url")`                                 | Go to URL in the current tab                        |
| `open new tab("url")`                                | New tab                                             |
| `switch tab(2)`                                      | Switch to tab #N (1-indexed)                        |
| `wait seconds(N)`                                    | Wait (max 600)                                      |
| `scroll down pixels(600)`                            | Scroll via `window.scrollBy`                        |
| `take screenshot("/tmp/x.png")`                      | Screenshot via WebDriver, PNG                       |
| `press key("Enter")`                                 | Send a special key to the active element            |
| `click button("Sign in")`                            | Find a button by text and click                     |
| `click element("#submit")` / `(".cls")` / `("//xpath")` | Click by selector                                |
| `type text("input_id", "text")`                      | Type into element                                   |
| `fill form id("login") value("ivan")`                | Find by id/name, clear, type                        |
| `login user("ivan") pws("secret")`                   | Find typical login+password fields, press Enter     |
| `execute js("return document.title")`                | Execute JS, print result                            |
| `download file("url", "/path")`                      | Download via `curl -L`                              |
| `upload file("/local/path", "input_id")`             | Via `send_keys` into `<input type=file>`            |

Supported key names for `press key`: `Enter/Return`, `Tab`, `Escape/Esc`, `Space`, `Backspace`, `Delete`, `Up/Down/Left/Right` (with/without `Arrow_` prefix), `Home`, `End`, `PageUp`, `PageDown`.

Selector strategies in `click_element`/`type_text`/`fill_form`:

- starts with `/` — XPath
- starts with `#`, `.`, or contains a space/`[` — CSS selector
- otherwise — `[id='X'],[name='X']`

Headless mode:

```bash
RACH_HEADLESS=1 rach script.rach
```

(Supported for Chrome/Edge/Firefox; Safari can't do headless.)

### bash DSL

Inside the body of `main` you can encounter assignments of the form `<anything> = <action> <text>`:

```
bash = generate install oh my zsh           # Generates a one-liner
bash = search curl or wget                  # Briefly "what's in the system for X"
bash = web search site ohmyzsh              # Logs intent of a web search (sends no requests)
bash = complete or error                    # Just prints `completed`
```

This is intentionally a weak, heuristic DSL — for short notes and hints. For real bash execution use `run_command("...")`.

Recognized tasks in `generate`: oh-my-zsh, homebrew, curl/wget, apt update, disk/memory/CPU. Anything else yields a `# TODO: ...` stub.

### ai_generate

Two backends:

1. **Live LLM (Claude)**: if `ANTHROPIC_API_KEY` is set, the call goes to `https://api.anthropic.com/v1/messages` via `curl`. Default model is `claude-haiku-4-5-20251001`; override with `RACH_LLM_MODEL`.
2. **Templates** (offline fallback): canned snippets for canonical tasks.

```
ai_generate(language="bash", task="install oh-my-zsh on Linux")
ai_generate(language="rust", task="simple TCP server")
```

Supported languages (templates): `bash` (alias `sh`), `python` (`py`), `rust` (`rs`), `c++` (`cpp`/`cxx`), `c`, `zig`. With an API key, any language the model can produce works.

### Native C / C++ interop

The interpreter links C and C++ object code at build time (`native/util.c`, `native/util.cpp` → static libs via `cc-rs` in `build.rs`). Four native functions are exposed as Rach commands:

```
native_crc32("hello world")          # CRC-32 (C), prints hex string
native_base64("hello, rach")         # base64 encode (C)
native_sort_ints("3,1,4,1,5,9,2,6")  # std::sort over signed ints (C++)
native_reverse("rach")               # byte-level reverse (C++)
```

For ad-hoc native code, two runtime commands write a temp file, compile via the system `cc` / `c++`, run the binary, and capture stdout:

```
run_c("#include <stdio.h>\nint main(){ printf(\"hi from C %d\\n\", 42); }")
run_cpp("#include <iostream>\nint main(){ std::cout << \"hi from C++\\n\"; }")
```

`CC` / `CXX` env vars override the compiler. Build-time FFI doesn't need `cc` at runtime; `run_c`/`run_cpp` do.

### ascii (ASCII art)

```
ascii banner("HELLO")                                    # 5-line block letters
ascii box("text", title="WARN", style="rounded")         # bordered box
ascii pyramid("X")                                       # pyramid of repeated chars
ascii diamond("X")                                       # diamond shape
ascii mirror("text")                                     # text + reversed copy
ascii table(headers="Name,Age", rows="Ivan,25;Maria,30") # formatted table
```

Border styles for `ascii box`: `single` (default), `double`, `bold`, `rounded`, `ascii`, `stars`, `hash`.

---

## Flow control

General `if` / `else` / `while` work on any expression. `if linux` / `if macos` / `if windows` are sugar for OS checks:

```
if linux:
    run("apt-get update")
else:
    run("brew update")

i = 0
while i < 5:
    print(i)
    i = i + 1
```

The block body is anything indented more than the header line.

`macos` is synonymous with `darwin`.

---

## Error convention

Every command after success prints `completed`. After failure — a line of the form:

```
error <code> string <line_number>  // <explanation>
```

Codes are close in meaning to HTTP:

| Code | Meaning                                         |
|------|--------------------------------------------------|
| 400  | Bad input (invalid arguments)                   |
| 404  | Not found (file, command, DOM element)          |
| 409  | State conflict (no active browser session)      |
| 422  | Parser syntax error                             |
| 500  | Internal error (I/O, process spawn)             |
| 501  | Not implemented on this OS                      |
| 502  | Subsystem failure (external driver, network)    |
| 503  | Service unavailable (couldn't start WebDriver)  |

You can manually raise an error:

```
error 409 string 12
```

— this just prints the line (does not interrupt execution).

---

## Environment variables

| Variable               | What it does                                                |
|------------------------|-------------------------------------------------------------|
| `RACH_HEADLESS`        | `1` — start the browser in headless mode                    |
| `RACH_DRY_RUN`         | `1` — `install_package` only prints the command, doesn't run it |
| `RACH_DRIVER_DIR`      | Directory for the cache of downloaded WebDriver binaries    |
| `RACH_STRICT`          | `1` — `error N` aborts execution (otherwise just printed)   |
| `ANTHROPIC_API_KEY`    | If set, `ai_generate` calls Claude via `curl` instead of using templates |
| `RACH_LLM_MODEL`       | Override the Claude model used by `ai_generate` (default: `claude-haiku-4-5-20251001`) |

---

## Building from source

End users don't need this — pre-built binaries are published on every release. Build from source only if you're hacking on the interpreter or running on a platform we don't ship a binary for.

```bash
cargo build --release
./target/release/rach examples/hello.rach
```

Cross-compilation:

```bash
rustup target add x86_64-unknown-linux-musl
cargo build --release --target x86_64-unknown-linux-musl
```

You get a statically-linked binary that runs on any Linux without glibc concerns.

Releases are produced by the GitHub Actions workflow at `.github/workflows/release.yml` — push a `vX.Y.Z` tag and the matrix builds and uploads archives for all supported targets.

---

## CLI

```
rach <file.rach>          run the script
rach run <file.rach>      same thing
rach check <file.rach>    only check syntax, no execution
rach version              print version
rach help                 brief help
```

Exit codes:

| Code | When                                  |
|------|----------------------------------------|
| 0    | Success                               |
| 1    | Runtime error                         |
| 2    | Failed to read file                   |
| 3    | Lexical error                         |
| 4    | Parse error                           |

---

## Grammar (informal)

```
program        := { import_line | top_item }
top_item       := function | struct | stmt
import_line    := "import" IDENT NEWLINE

function       := "rach" IDENT "(" [ params ] ")" ":" NEWLINE
                    block
                  "end" NEWLINE
struct         := "struct" IDENT "{" { IDENT [ "," ] } "}" NEWLINE

params         := param { "," param }
param          := IDENT [ "=" expr ]

block          := { stmt }
stmt           := assign | if | while | for | return | try | call_stmt
                | "break" | "continue" | "completed" | error_stmt

assign         := [ "set" ] IDENT [ "[" expr "]" | "." IDENT ]* "=" expr
if             := "if" expr ":" NEWLINE block [ "else" ":" NEWLINE block ]
while          := "while" expr ":" NEWLINE block
for            := "for" IDENT [ "," IDENT ] "in" expr ":" NEWLINE block
return         := "return" [ expr ]
try            := "try:" NEWLINE block "rescue" [ "as" IDENT ] ":" NEWLINE block

call_stmt      := segment { segment }
segment        := IDENT { IDENT } "(" [ arg_list ] ")"
arg_list       := arg { "," arg }
arg            := expr | IDENT "=" expr

expr           := … standard precedence climbing over
                  + - * / % ^ == != < <= > >=  and or not
                  with literals (int, float, string, f-string, list, map),
                  variables, indexing `e[k]`, calls, ranges `a..b` / `a..=b`,
                  postfix method calls `e.method(...)`, lambdas `\x -> expr`.
```

The lexer treats `\n` as a significant separator. Identifiers — `[A-Za-z_][A-Za-z0-9_]*`. Strings — double-quoted; supported escapes `\\ \" \n \t \r \{ \}`. f-strings (`f"..."`) interpolate `{expr}`. Numbers — int (`42`) or float (`3.14`).

---

## Limitations and non-goals

What Rach intentionally doesn't try to be:

- A type system. Values are dynamically typed; runtime errors stay rustc-style.
- A module system for user code. `import` lines are declarative; the stdlib is always linked. To split a project, run `rach` on multiple files via shell pipelines.
- A package manager. The interpreter is one statically-linked binary.
- An async runtime. Use `run("…")` to shell out to whatever does async well in your environment.

If you need a feature that isn't in the stdlib, the practical answer is `run("python3 -c '...'")` or `ai_generate(language="...", task="...")` — Rach is for orchestration, not for replacing real ecosystems.

---

## License

TBD. MIT or Apache-2.0 recommended. Without a `LICENSE` in the repo the code is not legally reusable.
