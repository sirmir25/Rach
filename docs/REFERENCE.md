# Rach Language Reference

A K&R-style reference for the Rach scripting language, version 0.2.

---

## 1. Lexical structure

### 1.1 Source character set
ASCII text. UTF-8 in string literals is supported but never decomposed. Lines end at `\n`. Tab and space are interchangeable for indentation.

### 1.2 Comments
```
# from `#` to end of line
// from `//` to end of line
```

### 1.3 Identifiers
`[A-Za-z_][A-Za-z0-9_]*`. Reserved keywords: `import`, `rach`, `return`, `if`, `else`, `not`, `for`, `in`, `set`, `completed`, `error`, `string`, `ai_generate`, `true`, `false`, `nil`, `linux`, `macos`, `darwin`, `windows`, `bsd`.

### 1.4 Literals
| Kind     | Form                          | Examples                |
|----------|-------------------------------|-------------------------|
| Integer  | optional sign, digits         | `42`, `-1`, `0`         |
| Float    | digits, dot, digits           | `3.14`, `0.5`           |
| String   | double-quoted, escapes `\n \t \r \\ \" \{ \}` | `"hello\n"`   |
| f-string | `f"…{expr}…"`, interpolates expressions | `f"hi, {name}"` |
| Bool     | keyword                       | `true`, `false`         |
| Nil      | keyword                       | `nil`                   |
| List     | bracketed, comma-separated    | `[1, "two", 3.0]`       |
| Map      | brace-comma, string keys      | `{"x": 1, "y": 2}`      |

### 1.5 Operators and punctuation
`( ) [ ] , = : + - * / % ^`. Newline is a statement terminator.

---

## 2. Program structure

A file is one of:

A `.rach` file is a flat sequence of: import lines, top-level statements, function defs, and struct defs — in any order. Top-level statements are collected, in source order, into an implicit `main`.

```
rach square(x):
    return x * x
end

print(square(7))   # 49
```

A file may declare at most one `main`; execution starts there. If both an explicit `rach main(): ... end` and bare top-level statements are present, the bare statements run first, then the explicit body.

The legacy form `rach name(N) ... return(end) (endK)` still parses for back-compat; new code shouldn't use it.

### 2.1 Imports

```
import <module>
```

`import` lines are **declarative** — the standard library is always linked in. Imports document intent only; an unknown module triggers a warning but does not fail.

Recognised modules: `os`, `system`, `web`, `browser`, `linux`, `macos`, `windows`, `bash`, `ai`, `ascii`.

### 2.2 Function declaration

```
rach <name>(<params>):
    <statements>
end
```

- `<params>` is empty, or a comma-separated list of identifiers, optionally with default values: `rach greet(name, greeting = "hello"):`.
- The body is a block of statements indented past the `rach` keyword.

Inside the body:
- `return <expr>` — return a value; control leaves the function.
- `return` (bare) — return `nil`.

### 2.3 Statements

- Expression statement: any command or function call on its own line.
- Variable assignment: `set NAME = <expr>` or shorthand `NAME = <expr>`. Index/field assignment: `m["k"] = v`, `s.field = v`.
- `if <expr>:` / `else:` blocks (the predicate can be any expression, including OS sugar `if linux:`, `if not windows:`).
- `while <expr>:` block; `do: ... while <expr>` for at-least-once loops.
- `for <var> in <expr>:` block; `for k, v in <map>:` to destructure pairs.
- `break`, `continue`.
- `try: ... rescue [as <var>]: ...` — catch a runtime error, bind to `<var>` as `{ code, line, message }`.
- `return <expr>`.
- `error <code> [string <line>]` — print a manual error.
- `completed` — print the literal word `completed` (optional; commands print it themselves on success).
- `<word> = generate ... | search ... | web search ... | complete or error` — bash DSL (legacy).

---

## 3. Expressions

### 3.1 Precedence (low to high)
1. Additive: `+`, `-`
2. Multiplicative: `*`, `/`, `%`
3. Power: `^` (right-associative)
4. Unary: `-`, `+`
5. Primary: literals, identifiers, calls, list, `(<expr>)`

### 3.2 Arithmetic semantics
- `int + int` → `int` if exact, else `float`.
- Any `float` operand promotes the result to `float`.
- `/` is true division (always returns float when result is non-integral).
- `%` is `rem_euclid` (positive result for positive divisor).
- `^` is `f64::powf`.
- Division/modulo by zero is a runtime error (code 400).

### 3.3 Strings and f-strings
`+` between two strings concatenates: `"foo" + "bar"` → `"foobar"`. With a non-string operand, both are coerced to numbers.

Plain `"..."` is fully literal — `{` and `}` are ordinary characters, so embedding C / JSON / regex / SQL needs no escaping. f-strings `f"..."` interpolate `{expr}` at runtime, where `expr` is any Rach expression. Use `\{` / `\}` to embed literal braces in an f-string.

```
name = "rach"
greet = f"hello, {name}"     # "hello, rach"
sum   = f"2+3 = {2 + 3}"     # "2+3 = 5"
json  = "{\"k\": 1}"         # plain string, no interpolation
```

### 3.4 Variables
A variable is referenced by bare identifier. Unknown name → runtime error code 404.

Scoping: function calls push a fresh scope; `for` loops push a scope per iteration body; the implicit `main` runs in the global scope.

### 3.5 Calls
Two call forms:

**(a) Command call** — multiword name + parenthesised args:
```
read_file("/tmp/x")
open in browser("https://...")            # → open_in_browser(...)
fill form id("login") value("ivan")       # → fill_form, kwargs id=..., value=...
```

The parser flattens word runs and finds the longest prefix matching a known stdlib command. Remaining segments become keyword arguments.

**(b) User function call** — exact name + parens:
```
square(7)
greet(name, age)
```

A user function call expression returns the value passed to its `return <expr>`. If no `return` was hit, returns `nil`.

### 3.6 Capturing mode
Inside `set x = <expr>`, the RHS executes in **capturing mode**: stdlib commands skip side-effecting prints (the `path: exists` line, the file content dump, etc.) and only return their value. Use this to feed command output into variables.

```
content = read("/tmp/x.txt")        # captures text without echoing it
hex = native_crc32(content)
```

---

## 4. Standard library

### 4.1 Output
| Command            | Effect                                   | Returns       |
|--------------------|------------------------------------------|---------------|
| `print(x, ...)`    | Print args joined by space + newline     | the line      |
| `echo(x, ...)`     | Alias for `print`                        | the line      |

### 4.2 Files
| Command                          | Effect                              | Returns       |
|----------------------------------|-------------------------------------|---------------|
| `read(path)` / `read_file(path)` | Print file contents                 | content       |
| `write(p, c)` / `create_file`    | Write/overwrite file                | path          |
| `edit_file(p, c)`                | Same as create_file                 | path          |
| `delete_file(p)` / `del` / `rm`  | Delete                              | bool          |
| `exists(p)` / `check_if_exists`  | Print `exists` or `missing`         | bool          |

### 4.3 Shell
| Command                   | Effect                                                 |
|---------------------------|--------------------------------------------------------|
| `run(cmd)` / `sh(cmd)`    | Run via `sh -c` (Win: `cmd /C`); print stdout/stderr   |
| `install_package(name)`   | brew/apt-get/dnf/pacman/zypper/apk/winget/pkg          |
| `reboot()` / `shutdown()` | Print intent only (no execution, for safety)           |

`install_package` honours `RACH_DRY_RUN=1`.

### 4.4 Control flow values

`if linux: / if macos: / if windows: / if bsd:` — each tests `Ctx::current_os` (set once at startup). `macos` is synonymous with `darwin`. Use `if not <os>:` and an optional same-column `else:` block.

```
for url in ["a", "b", "c"]:    # list literal
    visit(url)
for n in 5:                    # 0..N range
    print(n)
for tag in "x,y,z":            # comma-string split
    print(tag)
```

### 4.5 Math
Inputs in radians for trig.

| Group         | Commands                                                              |
|---------------|----------------------------------------------------------------------|
| Trig          | `sin cos tan asin acos atan atan2`                                    |
| Logs/exp      | `exp log log10 log2 pow sqrt`                                         |
| Rounding      | `floor ceil round abs`                                                |
| Aggregates    | `min max sum avg` (each takes a list or varargs)                      |
| Conversions   | `radians(deg) degrees(rad)`                                           |
| Constants     | `pi() e()`                                                            |

### 4.6 Logging
| Command                   | Effect                                                                                           |
|---------------------------|--------------------------------------------------------------------------------------------------|
| `log_debug(msg, ...)`     | Emit at `debug` level                                                                            |
| `log_info(msg, ...)`      | Emit at `info` level                                                                             |
| `log_warn(msg, ...)`      | Emit at `warn` level                                                                             |
| `log_error(msg, ...)`     | Emit at `error` level                                                                            |
| `log(level, msg, ...)`    | Emit by name                                                                                     |
| `log_level(level)`        | Set or query minimum level (`debug` < `info` < `warn` < `error` < `off`)                         |
| `log_to(path)` / `log_to()` | Mirror entries to file / disable                                                              |
| `log_history()`           | Return `List` of formatted entries                                                               |
| `log_filter(level)`       | Return entries at >= level                                                                       |
| `log_count(level?)`       | Return total or per-level count                                                                  |
| `log_clear()`             | Empty the buffer (returns count cleared)                                                         |

Buffer holds the last 1000 entries. `RACH_LOG=debug` sets the initial level.

### 4.7 ASCII art
`ascii_banner(text)`, `ascii_box(text, title=, style=)`, `ascii_pyramid(text)`, `ascii_diamond(text)`, `ascii_mirror(text)`, `ascii_border(text, style=)`, `ascii_table(headers="A,B,C", rows="1,2,3;4,5,6")`.

Styles: `single`, `double`, `bold`, `rounded`, `ascii`, `stars`, `hash`.

### 4.8 Native (C/C++)
Build-time linked:
- `native_crc32(text)` → 8-char hex (CRC-32, C)
- `native_base64(text)` → base64 string (C)
- `native_sort_ints("3,1,2")` → sorted CSV (C++ std::sort)
- `native_reverse(text)` → byte-reversed string (C++)

Runtime spawn:
- `run_c(code)` — write to temp `.c`, compile via `$CC` (default `cc`), run, capture stdout
- `run_cpp(code)` — same with `$CXX` (default `c++`), `-std=c++17`

### 4.9 Browser (W3C WebDriver)
Drivers auto-installed if Chrome or Firefox is present. Set `RACH_HEADLESS=1` for headless.

| Command                                      | Notes                                                |
|----------------------------------------------|------------------------------------------------------|
| `open in browser("url")`                     | First available browser                              |
| `open in chrome / firefox / edge / safari`   | Pin a browser                                         |
| `navigate to(url)`                           | Same tab                                              |
| `open new tab(url)` / `switch tab(N)`        |                                                      |
| `wait seconds(N)` (max 600)                  |                                                      |
| `scroll down pixels(N)`                      |                                                      |
| `take screenshot(path)`                      | PNG via WebDriver                                    |
| `press key("Enter")`                         | Special keys: Tab, Esc, Space, Backspace, Up/Down/Left/Right, Home, End, PageUp, PageDown |
| `click button("Sign in")`                    | Match by visible text                                |
| `click element("#id"/"".cls"/"//xpath")`     | Selector type detected by leading char               |
| `type text(id, text)`                        |                                                      |
| `fill form id("X") value("Y")`               | Clears then types                                    |
| `login user("u") pws("p")`                   | Finds typical login/password fields, presses Enter   |
| `execute js("code")`                         | Returns JS result                                    |
| `download file(url, path)`                   | Via curl                                              |
| `upload file(path, input_id)`                | Via WebDriver send_keys to `<input type=file>`       |

### 4.10 AI
```
ai_generate(language="python", task="parse JSON from stdin")
```
If `ANTHROPIC_API_KEY` is set, calls Claude (`claude-haiku-4-5-20251001` by default; override with `RACH_LLM_MODEL`). Otherwise falls back to a small set of canned templates.

---

## 5. Errors

### 5.1 Format
```
error[<code>]: <message>
  --> <file>:<line>
   |
 N |   <source line>
 ...
   |
// <stage> error <code> string <line>
```
Stage is `lex`, `parse`, or `runtime`. Colours auto-disable when stderr is not a TTY.

### 5.2 Codes
| Code | Meaning                                          |
|------|--------------------------------------------------|
| 400  | Bad input                                        |
| 404  | Not found (file, command, DOM element, variable) |
| 409  | State conflict (no browser session)              |
| 422  | Parser syntax error                              |
| 500  | Internal error (I/O, process spawn)              |
| 501  | Not implemented on this OS                       |
| 502  | Subsystem failure (driver, network)              |
| 503  | Service unavailable (driver bring-up)            |

### 5.3 Strict mode
`RACH_STRICT=1` makes `error N` abort and any runtime command failure terminate the script. Without it, errors are printed and execution continues.

---

## 6. CLI

```
rach                 open the REPL
rach repl            same
rach <file>          run the script (auto-resolves to ./examples/<file>[.rach])
rach run <file>      same
rach check <file>    parse-only
rach version
rach help
```

### 6.1 REPL
- Prompt `rach> `; continuation `...   `.
- A line ending with `:` (block header) or `rach <name>(...)` triggers continuation. Reading stops at an empty line.
- Variables and user functions persist across prompts.
- `exit`, `quit`, `:q`, or Ctrl-D to leave.

### 6.2 Exit codes
| Code | When                       |
|------|----------------------------|
| 0    | Success                    |
| 1    | Runtime error              |
| 2    | Cannot read file           |
| 3    | Lex error                  |
| 4    | Parse error                |

---

## 7. Environment variables

| Name                | Effect                                                            |
|---------------------|--------------------------------------------------------------------|
| `RACH_HEADLESS`     | `1` → browser headless                                            |
| `RACH_DRY_RUN`      | `1` → `install_package` only prints                               |
| `RACH_DRIVER_DIR`   | Cache dir for downloaded WebDrivers                               |
| `RACH_STRICT`       | `1` → errors abort                                                |
| `RACH_LOG`          | Initial log level                                                 |
| `ANTHROPIC_API_KEY` | Enable Claude in `ai_generate`                                    |
| `RACH_LLM_MODEL`    | Override Claude model                                             |
| `CC`, `CXX`         | Compilers used by `run_c` / `run_cpp`                             |

---

## 8. Grammar (formal)

```
program        := { import_line | function | struct | stmt }
import_line    := "import" IDENT NEWLINE

function       := "rach" IDENT "(" [ param_list ] ")" ":" NEWLINE
                    { stmt }
                  "end" NEWLINE
                  // legacy form, still accepted:
                  // "rach" IDENT "(" param_list ")" NEWLINE
                  //   { stmt }
                  // "return" "(" "end" ")" NEWLINE
                  // "(" "end" [ INT ] ")" NEWLINE

struct         := "struct" IDENT "{" field_list "}" NEWLINE
field_list     := IDENT { [ "," ] IDENT }

param_list     := param { "," param }
param          := IDENT [ "=" expr ]

stmt           := if_stmt | for_stmt | set_stmt | bash_dsl
                | "completed" NEWLINE
                | "error" INT [ "string" INT ] NEWLINE
                | "return" [ expr ] NEWLINE
                | call_or_expr NEWLINE

if_stmt        := "if" [ "not" ] IDENT ":" NEWLINE
                    { stmt at indent > if's-column }
                  [ "else" ":" NEWLINE
                    { stmt at indent > if's-column } ]

for_stmt       := "for" IDENT "in" expr ":" NEWLINE
                    { stmt }

set_stmt       := [ "set" ] IDENT "=" expr NEWLINE

expr           := additive
additive       := multiplicative { ( "+" | "-" ) multiplicative }
multiplicative := power          { ( "*" | "/" | "%" ) power }
power          := unary [ "^" power ]
unary          := ( "-" | "+" ) unary | primary
primary        := STRING | INT | FLOAT | "true" | "false" | "nil"
                | "[" [ expr { "," expr } ] "]"
                | "(" expr ")"
                | IDENT                            # variable
                | call                             # command or user fn
call           := segment { segment }
segment        := IDENT { IDENT } "(" arg_list ")"
arg_list       := /* empty */ | arg { "," arg }
arg            := expr | IDENT "=" expr            # named keyword

bash_dsl       := IDENT "=" rhs-of-line NEWLINE    # only when RHS starts with
                                                    # generate|search|web|complete
```

---

## 9. Versioning

Rach follows semver. `0.x` is pre-stable; the language and stdlib may change. Every breaking change is in `CHANGELOG.md` (when one exists).
