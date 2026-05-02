mod ast;
mod lexer;
mod parser;
mod interpreter;
mod stdlib;

use std::env;
use std::fs;
use std::process::ExitCode;

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn print_usage() {
    println!("Rach {} — пиши просто, запускай везде", VERSION);
    println!();
    println!("Usage:");
    println!("  rach <file.rach>        run a Rach script");
    println!("  rach run <file.rach>    same as above");
    println!("  rach check <file.rach>  parse only (no execution)");
    println!("  rach version            print version");
    println!("  rach help               show this help");
}

/// Resolve a script path with friendly fallbacks. Tries, in order:
///   1. The path as given
///   2. <path>.rach (if no .rach extension)
///   3. examples/<path>
///   4. examples/<path>.rach
/// Returns the first path that exists, or None.
fn resolve_script_path(path: &str) -> Option<String> {
    use std::path::Path;
    let candidates: Vec<String> = {
        let mut v = vec![path.to_string()];
        if !path.ends_with(".rach") {
            v.push(format!("{}.rach", path));
        }
        let bare = Path::new(path).file_name().and_then(|s| s.to_str()).unwrap_or(path);
        v.push(format!("examples/{}", bare));
        if !bare.ends_with(".rach") {
            v.push(format!("examples/{}.rach", bare));
        }
        v
    };
    candidates.into_iter().find(|p| Path::new(p).is_file())
}

use interpreter::report_pretty;

fn run_file(path: &str, check_only: bool) -> ExitCode {
    let resolved = resolve_script_path(path);
    let read_path: String = match &resolved {
        Some(p) => p.clone(),
        None => path.to_string(),
    };
    if let Some(p) = &resolved {
        if p != path {
            eprintln!("// using {} (not found at {})", p, path);
        }
    }
    let source = match fs::read_to_string(&read_path) {
        Ok(s) => s,
        Err(e) => {
            report_pretty("io", 404, path, 0, &format!("cannot read {}: {}", path, e), None);
            // Suggest examples/ if any .rach file there matches the basename
            if let Ok(entries) = fs::read_dir("examples") {
                let stem = std::path::Path::new(path)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or(path);
                let hits: Vec<String> = entries
                    .filter_map(|e| e.ok())
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .filter(|name| name.ends_with(".rach") && name.contains(stem))
                    .collect();
                if !hits.is_empty() {
                    eprintln!("// did you mean: {}", hits.iter().map(|h| format!("examples/{}", h)).collect::<Vec<_>>().join(", "));
                }
            }
            return ExitCode::from(2);
        }
    };

    let tokens = match lexer::tokenize(&source) {
        Ok(t) => t,
        Err(e) => {
            report_pretty("lex", 400, &read_path, e.line, &e.message, Some(&source));
            return ExitCode::from(3);
        }
    };

    let program = match parser::parse(tokens) {
        Ok(p) => p,
        Err(e) => {
            report_pretty("parse", 422, &read_path, e.line, &e.message, Some(&source));
            return ExitCode::from(4);
        }
    };

    if check_only {
        println!("completed");
        println!("script ok: {} import(s), {} function(s)", program.imports.len(), program.functions.len());
        return ExitCode::SUCCESS;
    }

    match interpreter::run(&program, &source, &read_path) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            report_pretty("runtime", e.code, &read_path, e.line, &e.message, Some(&source));
            ExitCode::from(1)
        }
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        print_usage();
        return ExitCode::SUCCESS;
    }

    match args[1].as_str() {
        "help" | "-h" | "--help" => {
            print_usage();
            ExitCode::SUCCESS
        }
        "version" | "-v" | "--version" => {
            println!("rach {}", VERSION);
            ExitCode::SUCCESS
        }
        "check" => {
            if args.len() < 3 {
                eprintln!("error 400 string 0  // missing file argument");
                return ExitCode::from(2);
            }
            run_file(&args[2], true)
        }
        "run" => {
            if args.len() < 3 {
                eprintln!("error 400 string 0  // missing file argument");
                return ExitCode::from(2);
            }
            run_file(&args[2], false)
        }
        other => run_file(other, false),
    }
}
