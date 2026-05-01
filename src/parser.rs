use std::collections::BTreeMap;

use crate::ast::{BashAction, CallSegment, Function, Program, Stmt, Value};
use crate::lexer::{Tok, Token};

#[derive(Debug)]
pub struct ParseError {
    pub line: usize,
    pub message: String,
}

impl ParseError {
    fn at(t: Option<&Token>, msg: impl Into<String>) -> Self {
        let line = t.map(|t| t.line).unwrap_or(0);
        ParseError { line, message: msg.into() }
    }
}

struct P {
    tokens: Vec<Token>,
    pos: usize,
}

impl P {
    fn new(tokens: Vec<Token>) -> Self { Self { tokens, pos: 0 } }

    fn peek(&self) -> Option<&Token> { self.tokens.get(self.pos) }

    fn peek_at(&self, off: usize) -> Option<&Token> { self.tokens.get(self.pos + off) }

    fn next(&mut self) -> Option<Token> {
        let t = self.tokens.get(self.pos).cloned();
        if t.is_some() { self.pos += 1; }
        t
    }

    fn skip_newlines(&mut self) {
        while matches!(self.peek().map(|t| &t.tok), Some(Tok::Newline)) {
            self.pos += 1;
        }
    }

    fn expect_newline(&mut self) -> Result<(), ParseError> {
        match self.peek().map(|t| &t.tok) {
            Some(Tok::Newline) => { self.pos += 1; Ok(()) }
            None => Ok(()),
            _ => Err(ParseError::at(self.peek(), "expected end of line")),
        }
    }

    fn expect_word(&mut self, w: &str) -> Result<Token, ParseError> {
        match self.peek().cloned() {
            Some(t) => match &t.tok {
                Tok::Word(s) if s == w => { self.pos += 1; Ok(t) }
                _ => Err(ParseError::at(Some(&t), format!("expected `{}`", w))),
            },
            None => Err(ParseError::at(None, format!("expected `{}`", w))),
        }
    }

    fn expect_tok(&mut self, expected: &Tok, label: &str) -> Result<Token, ParseError> {
        match self.peek().cloned() {
            Some(t) if std::mem::discriminant(&t.tok) == std::mem::discriminant(expected) => {
                self.pos += 1; Ok(t)
            }
            other => Err(ParseError::at(other.as_ref(), format!("expected {}", label))),
        }
    }
}

pub fn parse(tokens: Vec<Token>) -> Result<Program, ParseError> {
    let mut p = P::new(tokens);
    let mut imports = Vec::new();
    let mut functions = Vec::new();

    p.skip_newlines();

    // Imports
    while let Some(tok) = p.peek().cloned() {
        if let Tok::Word(w) = &tok.tok {
            if w == "import" {
                p.next();
                let name_tok = p.next().ok_or_else(|| ParseError::at(Some(&tok), "expected module name"))?;
                let name = match name_tok.tok {
                    Tok::Word(s) => s,
                    _ => return Err(ParseError::at(Some(&name_tok), "expected module name")),
                };
                imports.push(name);
                p.expect_newline()?;
                p.skip_newlines();
                continue;
            }
        }
        break;
    }

    // Functions
    while let Some(tok) = p.peek().cloned() {
        match &tok.tok {
            Tok::Word(w) if w == "rach" => {
                let f = parse_function(&mut p)?;
                functions.push(f);
                p.skip_newlines();
            }
            _ => return Err(ParseError::at(Some(&tok), format!("expected `rach` or `import`, got `{:?}`", tok.tok))),
        }
    }

    Ok(Program { imports, functions })
}

/// Accept either `end` or `endN` (where N is one or more digits glued to the keyword
/// because the lexer permits digits in identifier continuations).
fn expect_end_marker(p: &mut P) -> Result<(), ParseError> {
    let tok = p.next().ok_or_else(|| ParseError::at(None, "expected `end`"))?;
    match &tok.tok {
        Tok::Word(w) if w == "end" || (w.starts_with("end") && w[3..].chars().all(|c| c.is_ascii_digit())) => Ok(()),
        _ => Err(ParseError::at(Some(&tok), "expected `end`")),
    }
}

fn parse_function(p: &mut P) -> Result<Function, ParseError> {
    let header = p.expect_word("rach")?;
    let name_tok = p.next().ok_or_else(|| ParseError::at(Some(&header), "expected function name"))?;
    let name = match name_tok.tok {
        Tok::Word(s) => s,
        _ => return Err(ParseError::at(Some(&name_tok), "expected function name")),
    };
    p.expect_tok(&Tok::LParen, "`(`")?;
    let arity_tok = p.next().ok_or_else(|| ParseError::at(None, "expected arity"))?;
    let arity = match arity_tok.tok {
        Tok::Int(n) => n as u32,
        _ => return Err(ParseError::at(Some(&arity_tok), "expected integer arity")),
    };
    p.expect_tok(&Tok::RParen, "`)`")?;
    p.expect_newline()?;
    p.skip_newlines();

    let body = parse_block(p, 0)?; // 0 = no minimum indent (only stops on `return`)

    // return(end)  — `end` may also tokenize as `endN` (digits run together with the word)
    p.expect_word("return")?;
    p.expect_tok(&Tok::LParen, "`(`")?;
    expect_end_marker(p)?;
    p.expect_tok(&Tok::RParen, "`)`")?;
    p.expect_newline()?;
    p.skip_newlines();

    // (endN)
    p.expect_tok(&Tok::LParen, "`(`")?;
    expect_end_marker(p)?;
    if let Some(Tok::Int(_)) = p.peek().map(|t| t.tok.clone()) {
        p.next();
    }
    p.expect_tok(&Tok::RParen, "`)`")?;
    p.expect_newline()?;

    Ok(Function { name, arity, body, line: header.line })
}

/// Parse a sequence of statements. `min_indent_col` is the minimum starting column;
/// we stop when the next stmt starts at col <= min_indent_col, or on `return`/`(`
/// at the function level, or end-of-file.
fn parse_block(p: &mut P, min_indent_col: usize) -> Result<Vec<Stmt>, ParseError> {
    let mut stmts = Vec::new();
    loop {
        p.skip_newlines();
        let tok = match p.peek() {
            Some(t) => t.clone(),
            None => break,
        };

        // End of function-level block
        if matches!(&tok.tok, Tok::Word(w) if w == "return") { break; }
        if matches!(&tok.tok, Tok::LParen) { break; } // `(end0)` sentinel

        // Indent check (only matters for nested blocks; min_indent_col=0 disables)
        if min_indent_col > 0 && tok.col <= min_indent_col {
            break;
        }

        let stmt = parse_stmt(p)?;
        stmts.push(stmt);
    }
    Ok(stmts)
}

fn parse_stmt(p: &mut P) -> Result<Stmt, ParseError> {
    let head = p.peek().cloned().ok_or_else(|| ParseError::at(None, "unexpected end of input"))?;
    let head_col = head.col;
    let head_line = head.line;

    let word = match &head.tok {
        Tok::Word(w) => w.clone(),
        _ => return Err(ParseError::at(Some(&head), format!("unexpected token {:?}", head.tok))),
    };

    // `completed`
    if word == "completed" {
        p.next();
        p.expect_newline()?;
        return Ok(Stmt::Completed { line: head_line });
    }

    // `error N string M`
    if word == "error" {
        p.next();
        let code_tok = p.next().ok_or_else(|| ParseError::at(Some(&head), "expected error code"))?;
        let code = match code_tok.tok {
            Tok::Int(n) => n,
            _ => return Err(ParseError::at(Some(&code_tok), "expected error code (int)")),
        };
        // optional `string N`
        let mut line_ref = 0i64;
        if let Some(Tok::Word(w)) = p.peek().map(|t| t.tok.clone()) {
            if w == "string" {
                p.next();
                let n_tok = p.next().ok_or_else(|| ParseError::at(Some(&head), "expected line ref"))?;
                if let Tok::Int(n) = n_tok.tok { line_ref = n; }
            }
        }
        p.expect_newline()?;
        return Ok(Stmt::Error { code, line_ref, line: head_line });
    }

    // `if WORD :`
    if word == "if" {
        p.next();
        let os_tok = p.next().ok_or_else(|| ParseError::at(Some(&head), "expected os name"))?;
        let os = match os_tok.tok {
            Tok::Word(s) => s,
            _ => return Err(ParseError::at(Some(&os_tok), "expected os name")),
        };
        p.expect_tok(&Tok::Colon, "`:`")?;
        p.expect_newline()?;
        let body = parse_block(p, head_col)?;
        return Ok(Stmt::IfOs { os, body, line: head_line });
    }

    // `bash = ...` or any `WORD = ...` bash DSL line
    // Detect by checking if next token is `=` (Equals)
    if let Some(Tok::Equals) = p.peek_at(1).map(|t| t.tok.clone()) {
        // word can be anything; we treat it as a bash-DSL assignment
        p.next(); // word
        p.next(); // =
        // Consume the rest of the line as raw words/strings
        let (action, argument) = parse_bash_dsl_rhs(p)?;
        p.expect_newline()?;
        return Ok(Stmt::BashDsl { action, argument, line: head_line });
    }

    // ai_generate(...)
    if word == "ai_generate" {
        p.next();
        p.expect_tok(&Tok::LParen, "`(`")?;
        let mut language = String::new();
        let mut task = String::new();
        loop {
            match p.peek().map(|t| t.tok.clone()) {
                Some(Tok::RParen) => { p.next(); break; }
                Some(Tok::Comma) => { p.next(); continue; }
                Some(Tok::Word(k)) => {
                    p.next();
                    p.expect_tok(&Tok::Equals, "`=`")?;
                    let v_tok = p.next().ok_or_else(|| ParseError::at(Some(&head), "expected value"))?;
                    let v = match v_tok.tok {
                        Tok::Str(s) => s,
                        Tok::Int(n) => n.to_string(),
                        Tok::Word(w) => w,
                        _ => return Err(ParseError::at(Some(&v_tok), "expected value")),
                    };
                    if k == "language" { language = v; }
                    else if k == "task" { task = v; }
                }
                other => return Err(ParseError::at(other.as_ref().and_then(|_| p.peek()), "bad ai_generate args")),
            }
        }
        p.expect_newline()?;
        return Ok(Stmt::AiGenerate { language, task, line: head_line });
    }

    // Otherwise: a "command call" — sequence of words then `(`...`)`, optionally repeated.
    parse_call_stmt(p, head_line)
}

fn parse_bash_dsl_rhs(p: &mut P) -> Result<(BashAction, String), ParseError> {
    // Consume words/strings until newline
    let mut tokens: Vec<String> = Vec::new();
    loop {
        match p.peek().map(|t| t.tok.clone()) {
            Some(Tok::Newline) | None => break,
            Some(Tok::Word(w)) => { p.next(); tokens.push(w); }
            Some(Tok::Str(s)) => { p.next(); tokens.push(s); }
            Some(Tok::Int(n)) => { p.next(); tokens.push(n.to_string()); }
            Some(_) => break,
        }
    }
    if tokens.is_empty() {
        return Err(ParseError::at(None, "empty bash DSL line"));
    }
    let head = tokens[0].as_str();
    let rest = tokens[1..].join(" ");
    let action = match head {
        "generate" => BashAction::Generate,
        "search" => BashAction::Search,
        "web" => {
            // expect `web search ...`
            if tokens.len() >= 2 && tokens[1] == "search" {
                let rest2 = tokens[2..].join(" ");
                return Ok((BashAction::WebSearch, rest2));
            }
            BashAction::Search
        }
        "complete" => BashAction::CompleteOrError,
        _ => BashAction::Generate, // forgiving default
    };
    Ok((action, rest))
}

fn parse_call_stmt(p: &mut P, line: usize) -> Result<Stmt, ParseError> {
    let mut segments: Vec<CallSegment> = Vec::new();

    loop {
        let words = collect_word_run(p)?;
        if words.is_empty() { break; }
        if !matches!(p.peek().map(|t| t.tok.clone()), Some(Tok::LParen)) {
            return Err(ParseError::at(p.peek(), format!("expected `(` after `{}`", words.join(" "))));
        }
        let (positional, named) = parse_arglist(p)?;
        segments.push(CallSegment { words, positional, named });
    }

    if segments.is_empty() {
        return Err(ParseError::at(p.peek(), "expected command name"));
    }

    p.expect_newline()?;
    Ok(Stmt::Call { segments, line })
}

fn collect_word_run(p: &mut P) -> Result<Vec<String>, ParseError> {
    let mut out = Vec::new();
    while let Some(Tok::Word(w)) = p.peek().map(|t| t.tok.clone()) {
        // stop if next-next is `=` (means key=value, not a word run)
        if matches!(p.peek_at(1).map(|t| t.tok.clone()), Some(Tok::Equals)) {
            break;
        }
        p.next();
        out.push(w);
    }
    Ok(out)
}

fn parse_arglist(p: &mut P) -> Result<(Vec<Value>, BTreeMap<String, Value>), ParseError> {
    p.expect_tok(&Tok::LParen, "`(`")?;
    let mut positional = Vec::new();
    let mut named: BTreeMap<String, Value> = BTreeMap::new();

    loop {
        match p.peek().map(|t| t.tok.clone()) {
            Some(Tok::RParen) => { p.next(); break; }
            Some(Tok::Comma) => { p.next(); continue; }
            Some(Tok::Str(s)) => { p.next(); positional.push(Value::Str(s)); }
            Some(Tok::Int(n)) => { p.next(); positional.push(Value::Int(n)); }
            Some(Tok::Word(w)) => {
                // Could be `key = value` or a bare word value.
                if matches!(p.peek_at(1).map(|t| t.tok.clone()), Some(Tok::Equals)) {
                    p.next(); // word
                    p.next(); // =
                    let v_tok = p.next().ok_or_else(|| ParseError::at(None, "expected value"))?;
                    let v = match v_tok.tok {
                        Tok::Str(s) => Value::Str(s),
                        Tok::Int(n) => Value::Int(n),
                        Tok::Word(s) => Value::Str(s),
                        _ => return Err(ParseError::at(Some(&v_tok), "bad value")),
                    };
                    named.insert(w, v);
                } else {
                    p.next();
                    positional.push(Value::Str(w));
                }
            }
            other => return Err(ParseError::at(other.as_ref().and_then(|_| p.peek()), "unexpected token in argument list")),
        }
    }

    Ok((positional, named))
}
