# Rach VS Code Extension

Syntax highlighting and basic language config for `.rach` files.

## Install (local)

```bash
cp -r vscode-rach ~/.vscode/extensions/rach-lang.rach-0.1.0
```

Restart VS Code. Open any `.rach` file — keywords, strings, builtins, and OS names will be coloured.

## What it covers

- Keywords: `import`, `rach`, `return`, `if`, `else`, `for`, `in`, `set`, `not`, `completed`, `error`, `ai_generate`
- OS names: `linux`, `macos`, `darwin`, `windows`, `bsd`
- All stdlib commands (file, system, web, ascii)
- Strings, numbers, comments (`#` or `//`)

This is a TextMate grammar — no LSP, no autocomplete. For autocomplete write a separate extension that runs `rach check <file>` on save.
