//! User input and stdin helpers.

use std::io::{self, BufRead, Write};

use crate::ast::Value;
use crate::interpreter::{Ctx, RuntimeError};

pub fn input(args: &[Value], _line: usize, _ctx: &Ctx) -> Result<Value, RuntimeError> {
    if let Some(prompt) = args.first() {
        print!("{}", prompt.as_str());
        let _ = io::stdout().flush();
    }
    let mut line = String::new();
    let stdin = io::stdin();
    let mut lock = stdin.lock();
    match lock.read_line(&mut line) {
        Ok(0) => Ok(Value::Nil),
        Ok(_) => {
            let trimmed = line.trim_end_matches(&['\r', '\n'][..]).to_string();
            Ok(Value::Str(trimmed))
        }
        Err(e) => Err(RuntimeError::new(500, 0, format!("input: {}", e))),
    }
}
