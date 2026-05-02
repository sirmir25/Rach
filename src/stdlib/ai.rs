/// AI code generator with two backends:
///
/// 1. **Live LLM** (preferred): if `ANTHROPIC_API_KEY` is set, call the Claude
///    Messages API via `curl` and serde_json. No new Rust deps. The model is
///    `claude-haiku-4-5-20251001` by default (fast, cheap); override with
///    `RACH_LLM_MODEL`. Limited to 1024 output tokens.
/// 2. **Templates** (fallback): heuristic snippets for a few canonical tasks.
///    Always available, even offline.
pub fn ai_generate(language: &str, task: &str, line: usize) {
    let lang = language.to_ascii_lowercase();
    println!("# ---- ai_generate({}, {:?}) [line {}] ----", lang, task, line);

    if let Ok(key) = std::env::var("ANTHROPIC_API_KEY") {
        if !key.is_empty() {
            match call_claude(&key, &lang, task) {
                Ok(text) => {
                    println!("{}", text);
                    println!("# ---- end ai_generate (claude) ----");
                    println!("completed");
                    return;
                }
                Err(e) => {
                    eprintln!("// claude call failed: {} — falling back to templates", e);
                }
            }
        }
    }

    let body = generate(&lang, task);
    println!("{}", body);
    println!("# ---- end ai_generate (templates) ----");
    println!("completed");
}

fn call_claude(api_key: &str, lang: &str, task: &str) -> Result<String, String> {
    use std::process::Command;

    let model = std::env::var("RACH_LLM_MODEL").unwrap_or_else(|_| "claude-haiku-4-5-20251001".to_string());
    let prompt = format!(
        "Generate idiomatic, runnable {} code for this task:\n\n{}\n\nRespond with code only — no markdown fences, no commentary. Keep it minimal but complete.",
        lang, task
    );

    let body = serde_json::json!({
        "model": model,
        "max_tokens": 1024,
        "messages": [{ "role": "user", "content": prompt }],
    });
    let body_str = body.to_string();

    let output = Command::new("curl")
        .arg("-sS")
        .arg("-X").arg("POST")
        .arg("https://api.anthropic.com/v1/messages")
        .arg("-H").arg(format!("x-api-key: {}", api_key))
        .arg("-H").arg("anthropic-version: 2023-06-01")
        .arg("-H").arg("content-type: application/json")
        .arg("--max-time").arg("60")
        .arg("-d").arg(&body_str)
        .output()
        .map_err(|e| format!("spawn curl: {}", e))?;

    if !output.status.success() {
        return Err(format!("curl exit {:?}: {}", output.status.code(), String::from_utf8_lossy(&output.stderr)));
    }

    let resp_text = String::from_utf8_lossy(&output.stdout);
    let resp: serde_json::Value = serde_json::from_str(&resp_text)
        .map_err(|e| format!("parse json: {} — body was: {}", e, resp_text))?;

    if let Some(err) = resp.get("error") {
        return Err(format!("api error: {}", err));
    }

    let text = resp.get("content")
        .and_then(|c| c.as_array())
        .and_then(|arr| arr.iter().find_map(|b| b.get("text").and_then(|t| t.as_str())))
        .ok_or_else(|| format!("unexpected response shape: {}", resp_text))?;

    Ok(text.to_string())
}

fn generate(lang: &str, task: &str) -> String {
    let t = task.to_ascii_lowercase();
    match lang {
        "bash" | "sh" => bash_for(&t, task),
        "python" | "py" => python_for(&t, task),
        "rust" | "rs" => rust_for(&t, task),
        "c++" | "cpp" | "cxx" => cpp_for(&t, task),
        "c" => c_for(&t, task),
        "zig" => zig_for(&t, task),
        other => format!("// language `{}` not in the built-in ai_generate catalogue\n// task: {}", other, task),
    }
}

fn bash_for(t: &str, raw: &str) -> String {
    if t.contains("oh-my-zsh") || t.contains("oh my zsh") || t.contains("ohmyzsh") {
        return r#"#!/usr/bin/env bash
set -euo pipefail
if [ ! -d "$HOME/.oh-my-zsh" ]; then
  if command -v zsh >/dev/null 2>&1; then
    sh -c "$(curl -fsSL https://raw.githubusercontent.com/ohmyzsh/ohmyzsh/master/tools/install.sh)" "" --unattended
  else
    echo "zsh missing — install it first (apt/dnf/pacman/brew)" >&2
    exit 1
  fi
else
  echo "oh-my-zsh already installed at $HOME/.oh-my-zsh"
fi"#.to_string();
    }
    if t.contains("update") && (t.contains("system") || t.contains("packages")) {
        return r#"#!/usr/bin/env bash
set -euo pipefail
if   command -v apt-get >/dev/null; then sudo apt-get update -y && sudo apt-get upgrade -y
elif command -v dnf     >/dev/null; then sudo dnf upgrade -y
elif command -v pacman  >/dev/null; then sudo pacman -Syu --noconfirm
elif command -v brew    >/dev/null; then brew update && brew upgrade
else echo "no known package manager" >&2; exit 1
fi"#.to_string();
    }
    format!("#!/usr/bin/env bash\nset -euo pipefail\n# task: {}\necho 'TODO: implement task'\n", raw)
}

fn python_for(t: &str, raw: &str) -> String {
    if t.contains("tcp") && t.contains("server") {
        return r#"import socket

def main():
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.bind(("127.0.0.1", 9000))
        s.listen()
        print("listening on 127.0.0.1:9000")
        while True:
            conn, addr = s.accept()
            with conn:
                print("client:", addr)
                data = conn.recv(4096)
                conn.sendall(data)

if __name__ == "__main__":
    main()
"#.to_string();
    }
    if t.contains("json") && t.contains("parse") {
        return r#"import json, sys
data = json.load(sys.stdin)
print(json.dumps(data, indent=2, ensure_ascii=False))
"#.to_string();
    }
    format!("# task: {}\nprint('TODO: implement task')\n", raw)
}

fn rust_for(t: &str, raw: &str) -> String {
    if t.contains("tcp") && t.contains("server") {
        return r#"use std::io::{Read, Write};
use std::net::TcpListener;

fn main() -> std::io::Result<()> {
    let listener = TcpListener::bind("127.0.0.1:9000")?;
    println!("listening on 127.0.0.1:9000");
    for stream in listener.incoming() {
        let mut s = stream?;
        let mut buf = [0u8; 4096];
        let n = s.read(&mut buf)?;
        s.write_all(&buf[..n])?;
    }
    Ok(())
}
"#.to_string();
    }
    if t.contains("copy") && t.contains("file") {
        return r#"fn main() -> std::io::Result<()> {
    let mut args = std::env::args().skip(1);
    let src = args.next().expect("usage: copy <src> <dst>");
    let dst = args.next().expect("usage: copy <src> <dst>");
    std::fs::copy(&src, &dst)?;
    println!("copied {} -> {}", src, dst);
    Ok(())
}
"#.to_string();
    }
    format!("// task: {}\nfn main() {{ println!(\"TODO: implement task\"); }}\n", raw)
}

fn cpp_for(t: &str, raw: &str) -> String {
    if t.contains("copy") && t.contains("file") {
        return r#"#include <filesystem>
#include <iostream>

int main(int argc, char** argv) {
    if (argc != 3) { std::cerr << "usage: copy <src> <dst>\n"; return 1; }
    std::filesystem::copy_file(argv[1], argv[2],
        std::filesystem::copy_options::overwrite_existing);
    std::cout << "copied " << argv[1] << " -> " << argv[2] << "\n";
    return 0;
}
"#.to_string();
    }
    format!("#include <iostream>\nint main() {{\n    // task: {}\n    std::cout << \"TODO\\n\";\n}}\n", raw)
}

fn c_for(t: &str, raw: &str) -> String {
    if t.contains("copy") && t.contains("file") {
        return r#"#include <stdio.h>

int main(int argc, char** argv) {
    if (argc != 3) { fprintf(stderr, "usage: copy <src> <dst>\n"); return 1; }
    FILE* in = fopen(argv[1], "rb");
    FILE* out = fopen(argv[2], "wb");
    if (!in || !out) { perror("fopen"); return 1; }
    char buf[8192];
    size_t n;
    while ((n = fread(buf, 1, sizeof buf, in)) > 0) fwrite(buf, 1, n, out);
    fclose(in); fclose(out);
    return 0;
}
"#.to_string();
    }
    format!("#include <stdio.h>\nint main(void) {{\n    /* task: {} */\n    puts(\"TODO\");\n    return 0;\n}}\n", raw)
}

fn zig_for(t: &str, raw: &str) -> String {
    if t.contains("json") && t.contains("parse") {
        return r#"const std = @import("std");

pub fn main() !void {
    var gpa = std.heap.GeneralPurposeAllocator(.{}){};
    defer _ = gpa.deinit();
    const allocator = gpa.allocator();

    const stdin = std.io.getStdIn().reader();
    const input = try stdin.readAllAlloc(allocator, 1 << 20);
    defer allocator.free(input);

    var parsed = try std.json.parseFromSlice(std.json.Value, allocator, input, .{});
    defer parsed.deinit();

    const stdout = std.io.getStdOut().writer();
    try std.json.stringify(parsed.value, .{ .whitespace = .indent_2 }, stdout);
}
"#.to_string();
    }
    format!("const std = @import(\"std\");\npub fn main() !void {{\n    // task: {}\n    try std.io.getStdOut().writer().print(\"TODO\\n\", .{{}});\n}}\n", raw)
}
