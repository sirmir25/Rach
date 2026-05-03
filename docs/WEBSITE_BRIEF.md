# Website Brief: rach-lang.dev

A one-page landing site for the Rach programming language. Drop this file into Claude Designer / v0 / Lovable / Bolt with the prompt: *"Build a single-page marketing site for a programming language using this brief. Use the structure, copy, and code examples below verbatim."*

---

## Brand

- **Name**: Rach
- **One-liner**: Write simply, run anywhere.
- **Long pitch**: Rach is a tiny scripting language for automation — files, shell, browsers, math, logs, AI — written in Rust and shipped as one statically-linked binary. Read like English, run like a script.
- **Tone**: Confident, technical, no fluff. Think Rust + curl docs, not marketing copy.
- **Color palette**: Deep slate `#0F172A` background, accent `#F97316` (orange) for CTAs and highlights, muted `#94A3B8` for secondary text, `#22D3EE` for code-token highlights.
- **Typography**: Inter or Geist for body, JetBrains Mono / Geist Mono for code.
- **Logo idea**: Lowercase `rach` in monospace; the `r` has a small chevron above it (`>`) suggesting a prompt.

---

## Sections (in order)

### 1. Hero
- Title: **Rach**
- Subtitle: **Write simply, run anywhere.**
- Description: *A tiny scripting language for automation, in one statically-linked binary. Files, shell, browsers, math, logs, AI — all from a script that reads like English.*
- Primary CTA: `Install →` (anchors to the install section)
- Secondary CTA: `View on GitHub` → https://github.com/sirmir25/Rach
- Visual: rotating terminal showing 3-second snippets from `examples/short.rach`, `examples/native.rach`, and a REPL session.

### 2. "Hello, Rach" — minimal example
Three lines, full width, monospace, syntax-highlighted. Caption: *That's the whole script — no `main()`, no imports, no boilerplate.*

```
print("hello, rach")
write("/tmp/hi.txt", "data")
read("/tmp/hi.txt")
```

### 3. Feature grid (six cards, 3×2)

Each card is an icon + 2-word title + one-line subtitle.

1. **One binary** — Statically-linked Rust. No runtime, no venv, no JVM.
2. **Real WebDriver** — Chrome and Firefox automation built into the language. Driver auto-installed.
3. **C / C++ inline** — `run_c("…")`, `run_cpp("…")`, plus native helpers linked at build time.
4. **Built-in AI** — `ai_generate(...)` calls Claude when an API key is set. Falls back to templates offline.
5. **Math + logging** — `sin`, `pi()`, `sqrt`, `log_info`, `log_filter("warn")` — proper stdlib, not fake.
6. **Python-style REPL** — `rach` with no args opens an interactive console. State persists across prompts.

### 4. Code tour (tabs)

Three tabs sharing one editor frame. Each tab swaps the visible code block.

**Tab "Files & shell"**
```
write("/tmp/x.txt", "data")
content = read("/tmp/x.txt")
sh("ls -la /tmp")

for url in ["https://a", "https://b"]:
    download_file(url, "/tmp/" + url)
```

**Tab "Browser"**
```
open in browser("https://github.com/login")
fill form id("login_field") value("ilia")
fill form id("password") value("hunter2")
click button("Sign in")
take screenshot("/tmp/after-login.png")
```

**Tab "Math + logs + AI"**
```
log_level("debug")
log_info("starting")

x = 7
y = sqrt(x * x + 1)
print(sin(pi() / 2))            # 1.0

ai_generate(language="python", task="parse JSON from stdin")
log_filter("warn")
```

### 5. Install section

Big monospace block with one-tab-each. Each tab uses **only the snippet from `INSTALL_SNIPPETS.md`** that matches it. Tabs in this order:

1. macOS / Linux (bash)
2. Windows (cmd)
3. Cross-platform (Python)
4. C99
5. C++17

Below the tabs, a short note:
> *Each installer just runs `cargo build --release` and copies the binary to a system path. Requires Rust toolchain — get it from rustup.rs.*

### 6. REPL teaser

Inline terminal mock:

```
$ rach
Rach 0.2.0 — interactive console. Ctrl-D / `exit` to quit.

rach> x = 7
rach> y = x * x + 1
rach> print(y)
50
rach> sqrt(99)
9.9498...
rach> exit
$
```

### 7. Cheatsheet (collapsible)

A compact two-column reference with anchors. Source: pull section 4 of `REFERENCE.md` and lay it out as a cheatsheet, four groups: **Files / Shell**, **Math**, **Logging**, **Browser**.

### 8. Footer

- "MIT-licensed • Built with Rust" (left)
- Links: GitHub, Reference, Changelog (right)
- Tiny line: "Made by sirmir25"

---

## Copywriting rules

- No emoji, no exclamation marks.
- Sentences stay short. One thought per sentence.
- Code is the hero. Marketing copy is decoration.
- Never say "powerful", "amazing", or "next-generation". Just describe what it does.
- Russian quotes from the README author, if used as testimonials, stay in Russian and untranslated.

---

## SEO / Meta

- `<title>`: Rach — Write simply, run anywhere
- `<meta name="description">`: Tiny Rust-based scripting language for automation. Files, shell, browsers, math, logs, AI in one binary.
- Open Graph: square card with `rach` logo on slate, tagline below.

---

## Asset checklist

- Hero terminal frame (PNG or animated SVG cycling 3 examples)
- 6 feature icons (line-art, monochrome, accent on hover)
- Favicon: `R` glyph on slate

---

## What NOT to include

- No "Why Rach?" paragraph that lists buzzwords. The feature grid does that job.
- No newsletter signup. No cookie banner unless legally required.
- No comparison tables vs. Python/Bash. Show the code; let the reader decide.
