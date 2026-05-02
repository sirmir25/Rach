//! Interactive REPL — Python-style. Variables, functions, and the WebDriver
//! session persist across prompts. Multi-line input is collected when an input
//! line ends with `:` (block header) or starts with `rach <name>(...)`
//! (function definition); reading stops on an empty line.

use std::io::{self, BufRead, Write};

use crate::interpreter::{self, report_pretty};
use crate::lexer;
use crate::parser;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const PROMPT: &str = "rach> ";
const CONT:   &str = "...   ";

pub fn run() -> i32 {
    let strict = std::env::var("RACH_STRICT").map(|v| v == "1" || v.eq_ignore_ascii_case("true")).unwrap_or(false);
    let mut ctx = interpreter::make_ctx(strict, String::new(), "<repl>".to_string());

    println!("Rach {} — interactive console. Ctrl-D / `exit` to quit.", VERSION);
    println!("Type any Rach statement; multi-line blocks end at an empty line.");
    println!();

    let stdin = io::stdin();
    let mut stdout = io::stdout();
    let mut stdin_lock = stdin.lock();

    loop {
        print!("{}", PROMPT);
        let _ = stdout.flush();

        let mut buf = String::new();
        match stdin_lock.read_line(&mut buf) {
            Ok(0) => { println!(); break; }   // EOF
            Ok(_) => {}
            Err(_) => break,
        }
        let trimmed = buf.trim_end().to_string();
        if trimmed.is_empty() { continue; }
        if matches!(trimmed.trim(), "exit" | "quit" | ":q") { break; }

        // Collect continuation lines for blocks / multi-line function defs.
        let mut combined = trimmed.clone();
        if needs_continuation(&trimmed) {
            loop {
                print!("{}", CONT);
                let _ = stdout.flush();
                let mut more = String::new();
                match stdin_lock.read_line(&mut more) {
                    Ok(0) => break,
                    Ok(_) => {}
                    Err(_) => break,
                }
                let line = more.trim_end_matches('\n').to_string();
                if line.trim().is_empty() {
                    break;
                }
                combined.push('\n');
                combined.push_str(&line);
            }
        }

        // Update ctx.source so error messages can quote the current input.
        ctx.source = combined.clone();

        let tokens = match lexer::tokenize(&combined) {
            Ok(t) => t,
            Err(e) => { report_pretty("lex", 400, "<repl>", e.line, &e.message, Some(&combined)); continue; }
        };
        let program = match parser::parse(tokens) {
            Ok(p) => p,
            Err(e) => { report_pretty("parse", 422, "<repl>", e.line, &e.message, Some(&combined)); continue; }
        };
        if let Err(e) = interpreter::run_in_ctx(&program, &mut ctx) {
            report_pretty("runtime", e.code, "<repl>", e.line, &e.message, Some(&combined));
        }
    }
    0
}

/// Heuristic: should we keep reading more lines after this one?
/// - Yes for any line whose tail (ignoring comments) is `:` (block header).
/// - Yes for `rach <name>(...)` function declarations (need a matching
///   `return(end)` and `(endN)` later).
/// - Otherwise no.
fn needs_continuation(line: &str) -> bool {
    let cleaned = strip_comment(line).trim_end().to_string();
    if cleaned.ends_with(':') { return true; }
    if cleaned.starts_with("rach ") { return true; }
    false
}

fn strip_comment(s: &str) -> &str {
    // Naive: split on `#` or `//` outside of a string.
    let bytes = s.as_bytes();
    let mut in_str = false;
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if c == b'"' { in_str = !in_str; }
        if !in_str {
            if c == b'#' { return &s[..i]; }
            if c == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
                return &s[..i];
            }
        }
        i += 1;
    }
    s
}
