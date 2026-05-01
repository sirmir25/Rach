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

fn run_file(path: &str, check_only: bool) -> ExitCode {
    let source = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error 404 string 0  // cannot read {}: {}", path, e);
            return ExitCode::from(2);
        }
    };

    let tokens = match lexer::tokenize(&source) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("error 400 string {}  // lex: {}", e.line, e.message);
            return ExitCode::from(3);
        }
    };

    let program = match parser::parse(tokens) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error 422 string {}  // parse: {}", e.line, e.message);
            return ExitCode::from(4);
        }
    };

    if check_only {
        println!("completed");
        println!("script ok: {} import(s), {} function(s)", program.imports.len(), program.functions.len());
        return ExitCode::SUCCESS;
    }

    match interpreter::run(&program) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error {} string {}  // {}", e.code, e.line, e.message);
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
