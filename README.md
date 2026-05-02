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

Requires Rust 1.70+ and `cargo`. Build:

```bash
git clone https://github.com/<USER>/rach.git
cd rach
cargo build --release
sudo ln -s "$PWD/target/release/rach" /usr/local/bin/rach
rach version
```

For browser automation you also need one of:
- Chrome or Chromium (then `chromedriver` is downloaded automatically)
- Firefox (then `geckodriver` is downloaded automatically)
- Microsoft Edge with `msedgedriver` already installed
- Safari + `safaridriver --enable` + "Allow Remote Automation" checkbox in the Develop menu

`curl`, `tar`, `unzip` must be in PATH (available out of the box on macOS/Linux).

---

## Hello, world

`hello.rach`:

```
import os
import system

rach main(0)
    detect os()
    create_file("/tmp/hello.txt", "hello from Rach")
    read_file("/tmp/hello.txt")
    completed
return(end)
(end0)
```

Run:

```bash
rach hello.rach
```

Output:

```
os: macos
completed
created: /tmp/hello.txt
completed
hello from Rach
completed
completed
```

---

## Script structure

Each `.rach` file is:

```
import <module1>
import <module2>
...

rach <name>(<arity>)
    <commands>
return(end)
(end<N>)
```

Rules:

- A file must contain a function named `main` — execution starts there.
- `<arity>` is an integer; always `0` for now (function arguments are not yet implemented, but the syntax already supports them).
- `return(end)` marks the end of the function body, `(end0)` — the end of the file. `(end0)`, `(end1)` etc. are equivalent: the trailing digit carries no meaning, it's just part of the syntactic label.
- Comments: `#` or `//` until end of line.
- Indentation is significant only inside `if linux/macos/windows` blocks.

---

## Modules (import)

Imports are declarative. They don't load code (the standard library is always compiled into the interpreter), but serve as documentation of intent. Unknown modules trigger a warning, but not an error.

| Module       | What it declares                                  |
|--------------|---------------------------------------------------|
| `os`         | `detect_os`, `if linux/macos/windows` checks      |
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
set os_name = detect_os()
set message = read_file("/tmp/notes.txt")
set urls = ["https://a", "https://b", "https://c"]
```

When a variable is on the RHS of `set`, the command runs in **capturing mode** — it returns its result without printing the usual side-effect output.

User functions are declared with named params and may `return <expr>`:

```
rach square(x)
    set result = run_command("echo squared")
    return result
return(end)
(end0)
```

Call them like commands the parser doesn't already know: `set y = square(7)`.

---

## Loops: `for x in <expr>:`

Iterate over a list literal, a captured list variable, or a non-negative integer (which yields `0..N`):

```
for url in ["https://one", "https://two"]:
    open in browser(url)

set urls = ["a", "b", "c"]
for u in urls:
    run_command("echo visited")

for i in 3:
    create_file("/tmp/file", "x")
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

### os / system

| Command                  | What it does                                                    |
|--------------------------|------------------------------------------------------------------|
| `detect os()`            | Prints the current OS: `linux`, `macos`, `windows`, `bsd`       |
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

## Flow control: `if linux/macos/windows`

The only conditional operator — an OS check:

```
rach main(0)
    detect os()
    if linux:
        run_command("apt-get update")
    if macos:
        run_command("brew update")
    if windows:
        run_command("winget upgrade --all")
    completed
return(end)
(end0)
```

The block body — all lines indented more than the `if`. There are no empty `else` blocks — write multiple separate `if`s.

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

```bash
cargo build --release
./target/release/rach examples/hello.rach
```

Dependencies: only `serde_json`. Everything else — `std`.

Cross-compilation:

```bash
rustup target add x86_64-unknown-linux-musl
cargo build --release --target x86_64-unknown-linux-musl
```

You get a statically-linked binary that runs on any Linux without glibc concerns.

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

## Grammar (formal)

```
program       := { import_line } { function }
import_line   := "import" IDENT NEWLINE
function      := "rach" IDENT "(" INT ")" NEWLINE
                   { stmt }
                 "return" "(" "end" ")" NEWLINE
                 "(" "end" [ INT ] ")" NEWLINE

stmt          := if_stmt
              | bash_dsl
              | ai_generate_call
              | "completed" NEWLINE
              | "error" INT [ "string" INT ] NEWLINE
              | call

if_stmt       := "if" IDENT ":" NEWLINE
                   { stmt at indent > if's-column }

bash_dsl      := IDENT "=" rest-of-line NEWLINE
ai_generate_call := "ai_generate" "(" kw_args ")" NEWLINE

call          := segment { segment } NEWLINE
segment       := IDENT { IDENT } "(" arg_list ")"
arg_list      := arg { "," arg }
arg           := STRING | INT | IDENT | IDENT "=" (STRING | INT | IDENT)
```

The lexer treats `\n` as a significant separator. Identifiers — `[A-Za-z_][A-Za-z0-9_]*`. Strings — double-quoted with support for `\\`, `\"`, `\n`, `\t`, `\r`. Numbers — signed integers.

---

## Limitations and non-goals

Currently Rach has **no**:

- Arithmetic, string operations, comparisons.
- User-defined conditions beyond OS checks (`if linux/macos/windows`, `if not <os>`, `else`).
- Importing your own files.
- Try/catch — errors are printed by default; with `RACH_STRICT=1` they abort.

This is intentional: the language is designed as a declarative script DSL for automation, not as general-purpose. If you need logic — write `run_command("python3 -c '...'")` or `ai_generate(language="python", task="...")` and let Python do the work.

---

## License

TBD. MIT or Apache-2.0 recommended. Without a `LICENSE` in the repo the code is not legally reusable.
