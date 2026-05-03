use std::collections::BTreeMap;

use crate::ast::{BashAction, BinOp, CallSegment, Expr, Function, Program, Stmt, UnaryOp, Value};
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

    // Two top-level forms accepted:
    //
    //   1. Legacy "wrapped":   `rach main(0) ... return(end) (end0)` blocks. Multiple
    //      functions (main + helpers) all use this form.
    //
    //   2. Top-level script:   bare statements at the file root. The parser
    //      synthesises a `main(0)` wrapper around them. Helper functions can
    //      still be declared after the implicit main body using `rach name(...)`.
    let starts_with_function = matches!(p.peek().map(|t| t.tok.clone()), Some(Tok::Word(w)) if w == "rach");

    if starts_with_function {
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
    } else {
        // Synthesised main: collect top-level stmts until EOF or until we hit
        // a `rach <name>(...)` declaration of a helper.
        let mut main_body: Vec<Stmt> = Vec::new();
        let main_line = p.peek().map(|t| t.line).unwrap_or(1);
        loop {
            p.skip_newlines();
            let tok = match p.peek() { Some(t) => t.clone(), None => break };
            if matches!(&tok.tok, Tok::Word(w) if w == "rach") { break; }
            let stmt = parse_stmt(&mut p)?;
            main_body.push(stmt);
        }
        functions.push(Function {
            name: "main".to_string(),
            params: Vec::new(),
            body: main_body,
            line: main_line,
        });
        // Any trailing helper functions
        while let Some(tok) = p.peek().cloned() {
            match &tok.tok {
                Tok::Word(w) if w == "rach" => {
                    let f = parse_function(&mut p)?;
                    functions.push(f);
                    p.skip_newlines();
                }
                _ => return Err(ParseError::at(Some(&tok), format!("expected `rach` or end of file, got `{:?}`", tok.tok))),
            }
        }
    }

    Ok(Program { imports, functions })
}

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

    // Two forms inside the parens:
    //   `rach main(0)`            — legacy arity (an integer); produces no named params
    //   `rach myfn(a, b)`         — named params (zero or more identifiers)
    //   `rach myfn()`             — empty
    let mut params: Vec<String> = Vec::new();
    match p.peek().map(|t| t.tok.clone()) {
        Some(Tok::RParen) => { p.next(); }
        Some(Tok::Int(_)) => {
            p.next(); // discard legacy arity
            p.expect_tok(&Tok::RParen, "`)`")?;
        }
        Some(Tok::Word(_)) => {
            loop {
                let pt = p.next().ok_or_else(|| ParseError::at(None, "expected param name"))?;
                match pt.tok {
                    Tok::Word(s) => params.push(s),
                    _ => return Err(ParseError::at(Some(&pt), "expected param name")),
                }
                match p.peek().map(|t| t.tok.clone()) {
                    Some(Tok::Comma) => { p.next(); }
                    Some(Tok::RParen) => { p.next(); break; }
                    _ => return Err(ParseError::at(p.peek(), "expected `,` or `)` in param list")),
                }
            }
        }
        _ => return Err(ParseError::at(p.peek(), "expected params or arity")),
    }

    p.expect_newline()?;
    p.skip_newlines();

    let body = parse_block(p, 0)?;

    // `return(end)` — function-end marker (NOT the same as `return <expr>`)
    p.expect_word("return")?;
    p.expect_tok(&Tok::LParen, "`(`")?;
    expect_end_marker(p)?;
    p.expect_tok(&Tok::RParen, "`)`")?;
    p.expect_newline()?;
    p.skip_newlines();

    p.expect_tok(&Tok::LParen, "`(`")?;
    expect_end_marker(p)?;
    if let Some(Tok::Int(_)) = p.peek().map(|t| t.tok.clone()) {
        p.next();
    }
    p.expect_tok(&Tok::RParen, "`)`")?;
    p.expect_newline()?;

    Ok(Function { name, params, body, line: header.line })
}

fn parse_block(p: &mut P, min_indent_col: usize) -> Result<Vec<Stmt>, ParseError> {
    let mut stmts = Vec::new();
    loop {
        p.skip_newlines();
        let tok = match p.peek() {
            Some(t) => t.clone(),
            None => break,
        };

        // Function-end marker `return(end)` — only at function level (min_indent=0)
        if min_indent_col == 0 {
            if matches!(&tok.tok, Tok::Word(w) if w == "return") {
                if let Some(Tok::LParen) = p.peek_at(1).map(|t| t.tok.clone()) {
                    if let Some(Tok::Word(end_w)) = p.peek_at(2).map(|t| t.tok.clone()) {
                        if end_w == "end" || (end_w.starts_with("end") && end_w[3..].chars().all(|c| c.is_ascii_digit())) {
                            break;
                        }
                    }
                }
            }
        }
        if matches!(&tok.tok, Tok::LParen) { break; }

        if min_indent_col > 0 && tok.col <= min_indent_col {
            break;
        }

        // `else:` and `rescue` mark the boundaries of their parent if/try blocks.
        if matches!(&tok.tok, Tok::Word(w) if w == "else") {
            if let Some(Tok::Colon) = p.peek_at(1).map(|t| t.tok.clone()) {
                break;
            }
        }
        if matches!(&tok.tok, Tok::Word(w) if w == "rescue") {
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

    if word == "completed" {
        p.next();
        p.expect_newline()?;
        return Ok(Stmt::Completed { line: head_line });
    }

    if word == "error" {
        p.next();
        let code_tok = p.next().ok_or_else(|| ParseError::at(Some(&head), "expected error code"))?;
        let code = match code_tok.tok {
            Tok::Int(n) => n,
            _ => return Err(ParseError::at(Some(&code_tok), "expected error code (int)")),
        };
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

    if word == "return" {
        p.next();
        // `return` followed by newline → bare return
        if matches!(p.peek().map(|t| t.tok.clone()), Some(Tok::Newline) | None) {
            p.expect_newline()?;
            return Ok(Stmt::Return { expr: None, line: head_line });
        }
        let expr = parse_expr(p)?;
        p.expect_newline()?;
        return Ok(Stmt::Return { expr: Some(expr), line: head_line });
    }

    if word == "set" {
        p.next();
        let name_tok = p.next().ok_or_else(|| ParseError::at(None, "expected variable name"))?;
        let name = match name_tok.tok {
            Tok::Word(s) => s,
            _ => return Err(ParseError::at(Some(&name_tok), "expected variable name")),
        };
        p.expect_tok(&Tok::Equals, "`=`")?;
        let expr = parse_expr(p)?;
        p.expect_newline()?;
        return Ok(Stmt::Set { name, expr, line: head_line });
    }

    if word == "if" {
        p.next();
        // Look ahead one token to decide: legacy `if [not] <os>:` vs general `if <expr>:`.
        // Legacy form: optional `not`, then a bare OS keyword, then `:`.
        let saved = p.pos;
        let mut maybe_negate = false;
        if matches!(p.peek().map(|t| t.tok.clone()), Some(Tok::Word(ref w)) if w == "not") {
            p.next();
            maybe_negate = true;
        }
        let is_legacy_os = matches!(
            (p.peek().map(|t| t.tok.clone()), p.peek_at(1).map(|t| t.tok.clone())),
            (Some(Tok::Word(ref w)), Some(Tok::Colon))
                if matches!(w.as_str(), "linux" | "macos" | "darwin" | "windows" | "bsd")
        );

        if is_legacy_os {
            let os_tok = p.next().unwrap();
            let os = match os_tok.tok { Tok::Word(s) => s, _ => unreachable!() };
            p.expect_tok(&Tok::Colon, "`:`")?;
            p.expect_newline()?;
            let body = parse_block(p, head_col)?;
            let else_body = parse_optional_else(p, head_col)?;
            return Ok(Stmt::IfOs { os, negate: maybe_negate, body, else_body, line: head_line });
        }

        // Reset and parse as general expression-conditioned if.
        p.pos = saved;
        let cond = parse_expr(p)?;
        p.expect_tok(&Tok::Colon, "`:`")?;
        p.expect_newline()?;
        let body = parse_block(p, head_col)?;
        let else_body = parse_optional_else(p, head_col)?;
        return Ok(Stmt::If { cond, body, else_body, line: head_line });
    }

    if word == "while" {
        p.next();
        let cond = parse_expr(p)?;
        p.expect_tok(&Tok::Colon, "`:`")?;
        p.expect_newline()?;
        let body = parse_block(p, head_col)?;
        return Ok(Stmt::While { cond, body, line: head_line });
    }

    if word == "break" {
        p.next();
        p.expect_newline()?;
        return Ok(Stmt::Break { line: head_line });
    }

    if word == "continue" {
        p.next();
        p.expect_newline()?;
        return Ok(Stmt::Continue { line: head_line });
    }

    if word == "try" {
        p.next();
        p.expect_tok(&Tok::Colon, "`:`")?;
        p.expect_newline()?;
        let body = parse_block(p, head_col)?;
        p.skip_newlines();
        // Require `rescue [name]:` at the same column as `try`.
        let rescue_tok = p.peek().cloned()
            .ok_or_else(|| ParseError::at(None, "expected `rescue:` after `try:` block"))?;
        if !matches!(&rescue_tok.tok, Tok::Word(w) if w == "rescue") || rescue_tok.col != head_col {
            return Err(ParseError::at(Some(&rescue_tok), "expected `rescue:` at the same column as `try:`"));
        }
        p.next();
        let rescue_var = if let Some(Tok::Word(name)) = p.peek().map(|t| t.tok.clone()) {
            if name != "as" {
                return Err(ParseError::at(p.peek(), "expected `as <name>:` or just `:`"));
            }
            p.next();
            let name_tok = p.next().ok_or_else(|| ParseError::at(None, "expected name after `as`"))?;
            match name_tok.tok {
                Tok::Word(n) => Some(n),
                _ => return Err(ParseError::at(Some(&name_tok), "expected identifier after `as`")),
            }
        } else {
            None
        };
        p.expect_tok(&Tok::Colon, "`:`")?;
        p.expect_newline()?;
        let rescue_body = parse_block(p, head_col)?;
        return Ok(Stmt::Try { body, rescue_var, rescue_body, line: head_line });
    }

    if word == "assert" {
        p.next();
        p.expect_tok(&Tok::LParen, "`(`")?;
        let cond = parse_expr(p)?;
        let message = if matches!(p.peek().map(|t| t.tok.clone()), Some(Tok::Comma)) {
            p.next();
            Some(parse_expr(p)?)
        } else {
            None
        };
        p.expect_tok(&Tok::RParen, "`)`")?;
        p.expect_newline()?;
        return Ok(Stmt::Assert { cond, message, line: head_line });
    }

    if word == "for" {
        p.next();
        let var_tok = p.next().ok_or_else(|| ParseError::at(None, "expected loop variable"))?;
        let var = match var_tok.tok {
            Tok::Word(s) => s,
            _ => return Err(ParseError::at(Some(&var_tok), "expected loop variable")),
        };
        p.expect_word("in")?;
        let iter = parse_expr(p)?;
        p.expect_tok(&Tok::Colon, "`:`")?;
        p.expect_newline()?;
        let body = parse_block(p, head_col)?;
        return Ok(Stmt::For { var, iter, body, line: head_line });
    }

    // `WORD = ...` — variable assignment OR legacy bash DSL.
    // Bash DSL is opt-in via known head keywords on the RHS (`generate`,
    // `search`, `web`, `complete`). Anything else is treated as `set`,
    // including arithmetic like `y = x * 2 + 1`.
    if let Some(Tok::Equals) = p.peek_at(1).map(|t| t.tok.clone()) {
        let is_bash_dsl = matches!(p.peek_at(2).map(|t| t.tok.clone()),
            Some(Tok::Word(w)) if matches!(w.as_str(), "generate" | "search" | "web" | "complete")
        );

        if is_bash_dsl {
            p.next(); // word
            p.next(); // =
            let (action, argument) = parse_bash_dsl_rhs(p)?;
            p.expect_newline()?;
            return Ok(Stmt::BashDsl { action, argument, line: head_line });
        } else {
            p.next(); // word
            p.next(); // =
            let expr = parse_expr(p)?;
            p.expect_newline()?;
            return Ok(Stmt::Set { name: word, expr, line: head_line });
        }
    }

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

    parse_call_or_fncall_stmt(p, head_line)
}

fn parse_bash_dsl_rhs(p: &mut P) -> Result<(BashAction, String), ParseError> {
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
            if tokens.len() >= 2 && tokens[1] == "search" {
                let rest2 = tokens[2..].join(" ");
                return Ok((BashAction::WebSearch, rest2));
            }
            BashAction::Search
        }
        "complete" => BashAction::CompleteOrError,
        _ => BashAction::Generate,
    };
    Ok((action, rest))
}

fn parse_call_or_fncall_stmt(p: &mut P, line: usize) -> Result<Stmt, ParseError> {
    // Detect bare `name(args)` user-fn call vs. multi-segment command call.
    // Heuristic: a single Word followed immediately by `(` AND nothing more
    // after the matching `)` than newline, AND the name is NOT in the known
    // command registry — treat as user-fn ExprStmt. Otherwise, command-style.
    if let (Some(Tok::Word(w)), Some(Tok::LParen)) = (
        p.peek().map(|t| t.tok.clone()),
        p.peek_at(1).map(|t| t.tok.clone()),
    ) {
        // Don't consume yet — first try command-style. Command-style with
        // multi-word names will work fine; this branch is for single-word fn
        // calls only, and we let the dispatcher decide unknown ones.
        if !crate::stdlib::is_known_command(&w) {
            // Parse as user-fn call
            let _ = p.next();
            p.expect_tok(&Tok::LParen, "`(`")?;
            let mut args = Vec::new();
            loop {
                match p.peek().map(|t| t.tok.clone()) {
                    Some(Tok::RParen) => { p.next(); break; }
                    Some(Tok::Comma) => { p.next(); continue; }
                    _ => {
                        let e = parse_expr(p)?;
                        args.push(e);
                    }
                }
            }
            p.expect_newline()?;
            return Ok(Stmt::ExprStmt { expr: Expr::FnCall { name: w, args, line }, line });
        }
    }

    let segments = parse_call_segments(p)?;
    p.expect_newline()?;
    Ok(Stmt::Call { segments, line })
}

fn parse_call_segments(p: &mut P) -> Result<Vec<CallSegment>, ParseError> {
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
    Ok(segments)
}

fn collect_word_run(p: &mut P) -> Result<Vec<String>, ParseError> {
    let mut out = Vec::new();
    while let Some(Tok::Word(w)) = p.peek().map(|t| t.tok.clone()) {
        if matches!(p.peek_at(1).map(|t| t.tok.clone()), Some(Tok::Equals)) {
            break;
        }
        p.next();
        out.push(w);
    }
    Ok(out)
}

fn parse_arglist(p: &mut P) -> Result<(Vec<Expr>, BTreeMap<String, Expr>), ParseError> {
    p.expect_tok(&Tok::LParen, "`(`")?;
    let mut positional: Vec<Expr> = Vec::new();
    let mut named: BTreeMap<String, Expr> = BTreeMap::new();

    loop {
        match p.peek().map(|t| t.tok.clone()) {
            Some(Tok::RParen) => { p.next(); break; }
            Some(Tok::Comma) => { p.next(); continue; }
            Some(Tok::Word(w)) if matches!(p.peek_at(1).map(|t| t.tok.clone()), Some(Tok::Equals)) => {
                p.next(); // word
                p.next(); // =
                let v = parse_expr(p)?;
                named.insert(w, v);
            }
            _ => {
                let v = parse_expr(p)?;
                positional.push(v);
            }
        }
    }
    Ok((positional, named))
}

/// Pulls an `else:` block off the token stream if one is at `cond_col`.
/// Used by both `if`-OS and `if`-expr.
fn parse_optional_else(p: &mut P, cond_col: usize) -> Result<Option<Vec<Stmt>>, ParseError> {
    p.skip_newlines();
    if let Some(t) = p.peek().cloned() {
        if matches!(&t.tok, Tok::Word(w) if w == "else") && t.col == cond_col {
            p.next();
            p.expect_tok(&Tok::Colon, "`:`")?;
            p.expect_newline()?;
            let body = parse_block(p, cond_col)?;
            return Ok(Some(body));
        }
    }
    Ok(None)
}

/// Parse a full expression with operator precedence (low to high):
///   or  →  and  →  not  →  comparison  →  add/sub  →  mul/div/mod
///   →  pow (right-assoc)  →  unary  →  primary.
pub fn parse_expr(p: &mut P) -> Result<Expr, ParseError> {
    parse_logical_or(p)
}

fn parse_logical_or(p: &mut P) -> Result<Expr, ParseError> {
    let mut left = parse_logical_and(p)?;
    while matches!(p.peek().map(|t| t.tok.clone()), Some(Tok::Word(ref w)) if w == "or") {
        let line = p.peek().map(|t| t.line).unwrap_or(0);
        p.next();
        let right = parse_logical_and(p)?;
        left = Expr::Binary { op: BinOp::Or, lhs: Box::new(left), rhs: Box::new(right), line };
    }
    Ok(left)
}

fn parse_logical_and(p: &mut P) -> Result<Expr, ParseError> {
    let mut left = parse_logical_not(p)?;
    while matches!(p.peek().map(|t| t.tok.clone()), Some(Tok::Word(ref w)) if w == "and") {
        let line = p.peek().map(|t| t.line).unwrap_or(0);
        p.next();
        let right = parse_logical_not(p)?;
        left = Expr::Binary { op: BinOp::And, lhs: Box::new(left), rhs: Box::new(right), line };
    }
    Ok(left)
}

fn parse_logical_not(p: &mut P) -> Result<Expr, ParseError> {
    if matches!(p.peek().map(|t| t.tok.clone()), Some(Tok::Word(ref w)) if w == "not") {
        let line = p.peek().map(|t| t.line).unwrap_or(0);
        p.next();
        let inner = parse_logical_not(p)?;
        return Ok(Expr::Unary { op: UnaryOp::Not, expr: Box::new(inner), line });
    }
    parse_comparison(p)
}

fn parse_comparison(p: &mut P) -> Result<Expr, ParseError> {
    let left = parse_additive(p)?;
    let op = match p.peek().map(|t| t.tok.clone()) {
        Some(Tok::EqEq)  => Some(BinOp::Eq),
        Some(Tok::BangEq) => Some(BinOp::Ne),
        Some(Tok::Lt)    => Some(BinOp::Lt),
        Some(Tok::Gt)    => Some(BinOp::Gt),
        Some(Tok::LtEq)  => Some(BinOp::Le),
        Some(Tok::GtEq)  => Some(BinOp::Ge),
        _ => None,
    };
    if let Some(op) = op {
        let line = p.peek().map(|t| t.line).unwrap_or(0);
        p.next();
        let right = parse_additive(p)?;
        return Ok(Expr::Binary { op, lhs: Box::new(left), rhs: Box::new(right), line });
    }
    Ok(left)
}

fn parse_additive(p: &mut P) -> Result<Expr, ParseError> {
    let mut left = parse_multiplicative(p)?;
    loop {
        let op = match p.peek().map(|t| t.tok.clone()) {
            Some(Tok::Plus) => BinOp::Add,
            Some(Tok::Minus) => BinOp::Sub,
            _ => break,
        };
        let line = p.peek().map(|t| t.line).unwrap_or(0);
        p.next();
        let right = parse_multiplicative(p)?;
        left = Expr::Binary { op, lhs: Box::new(left), rhs: Box::new(right), line };
    }
    Ok(left)
}

fn parse_multiplicative(p: &mut P) -> Result<Expr, ParseError> {
    let mut left = parse_power(p)?;
    loop {
        let op = match p.peek().map(|t| t.tok.clone()) {
            Some(Tok::Star) => BinOp::Mul,
            Some(Tok::Slash) => BinOp::Div,
            Some(Tok::Percent) => BinOp::Mod,
            _ => break,
        };
        let line = p.peek().map(|t| t.line).unwrap_or(0);
        p.next();
        let right = parse_power(p)?;
        left = Expr::Binary { op, lhs: Box::new(left), rhs: Box::new(right), line };
    }
    Ok(left)
}

fn parse_power(p: &mut P) -> Result<Expr, ParseError> {
    let left = parse_unary(p)?;
    if matches!(p.peek().map(|t| t.tok.clone()), Some(Tok::Caret)) {
        let line = p.peek().map(|t| t.line).unwrap_or(0);
        p.next();
        let right = parse_power(p)?;
        return Ok(Expr::Binary { op: BinOp::Pow, lhs: Box::new(left), rhs: Box::new(right), line });
    }
    Ok(left)
}

fn parse_unary(p: &mut P) -> Result<Expr, ParseError> {
    match p.peek().map(|t| t.tok.clone()) {
        Some(Tok::Minus) => {
            let line = p.peek().map(|t| t.line).unwrap_or(0);
            p.next();
            let inner = parse_unary(p)?;
            Ok(Expr::Unary { op: UnaryOp::Neg, expr: Box::new(inner), line })
        }
        Some(Tok::Plus) => { p.next(); parse_unary(p) }
        _ => parse_primary(p),
    }
}

fn parse_primary(p: &mut P) -> Result<Expr, ParseError> {
    let base = parse_atom(p)?;
    parse_indexing(p, base)
}

/// `expr[key][key2]...` — postfix indexing into list/map.
fn parse_indexing(p: &mut P, mut target: Expr) -> Result<Expr, ParseError> {
    while matches!(p.peek().map(|t| t.tok.clone()), Some(Tok::LBracket)) {
        let line = p.peek().map(|t| t.line).unwrap_or(0);
        p.next();
        let key = parse_expr(p)?;
        p.expect_tok(&Tok::RBracket, "`]`")?;
        target = Expr::Index { target: Box::new(target), key: Box::new(key), line };
    }
    Ok(target)
}

fn parse_atom(p: &mut P) -> Result<Expr, ParseError> {
    let head = p.peek().cloned().ok_or_else(|| ParseError::at(None, "expected expression"))?;
    match head.tok.clone() {
        Tok::Str(s) => { p.next(); Ok(Expr::Lit(Value::Str(s))) }
        Tok::Int(n) => { p.next(); Ok(Expr::Lit(Value::Int(n))) }
        Tok::Float(f) => { p.next(); Ok(Expr::Lit(Value::Float(f))) }
        Tok::LBrace => {
            // Map literal: `{"key": value, "k2": v2}`
            let line = head.line;
            p.next();
            let mut entries: Vec<(Expr, Expr)> = Vec::new();
            loop {
                match p.peek().map(|t| t.tok.clone()) {
                    Some(Tok::RBrace) => { p.next(); break; }
                    Some(Tok::Comma) => { p.next(); continue; }
                    _ => {
                        let k = parse_expr(p)?;
                        p.expect_tok(&Tok::Colon, "`:`")?;
                        let v = parse_expr(p)?;
                        entries.push((k, v));
                    }
                }
            }
            Ok(Expr::MapLit { entries, line })
        }
        Tok::LParen => {
            p.next();
            let inner = parse_expr(p)?;
            p.expect_tok(&Tok::RParen, "`)`")?;
            Ok(inner)
        }
        Tok::LBracket => {
            p.next();
            let mut items = Vec::new();
            loop {
                match p.peek().map(|t| t.tok.clone()) {
                    Some(Tok::RBracket) => { p.next(); break; }
                    Some(Tok::Comma) => { p.next(); continue; }
                    _ => {
                        let e = parse_expr(p)?;
                        items.push(e);
                    }
                }
            }
            Ok(Expr::List(items))
        }
        Tok::Word(w) => {
            // Identifier — may be: bool literal, variable, fn call, or command call.
            // - `true` / `false` → bool literal
            // - `name(...)` → user-fn call OR command call (dispatcher decides)
            // - `name` alone → variable reference
            // - multi-word + `(` → command call
            if w == "true" { p.next(); return Ok(Expr::Lit(Value::Bool(true))); }
            if w == "false" { p.next(); return Ok(Expr::Lit(Value::Bool(false))); }
            if w == "nil" { p.next(); return Ok(Expr::Lit(Value::Nil)); }

            // Single word + `(` → call (we decide user-fn vs. command at that level)
            if matches!(p.peek_at(1).map(|t| t.tok.clone()), Some(Tok::LParen)) {
                if crate::stdlib::is_known_command(&w) {
                    let line = head.line;
                    let segments = parse_call_segments(p)?;
                    return Ok(Expr::Call { segments, line });
                } else {
                    // user-fn call
                    let _ = p.next();
                    p.expect_tok(&Tok::LParen, "`(`")?;
                    let mut args = Vec::new();
                    loop {
                        match p.peek().map(|t| t.tok.clone()) {
                            Some(Tok::RParen) => { p.next(); break; }
                            Some(Tok::Comma) => { p.next(); continue; }
                            _ => {
                                let e = parse_expr(p)?;
                                args.push(e);
                            }
                        }
                    }
                    return Ok(Expr::FnCall { name: w, args, line: head.line });
                }
            }

            // Multi-word command call (e.g. `read file("...")`): parse as call segments.
            // Heuristic: peek the longest word-run; if any of those words combined
            // is a known prefix of a multi-word command, parse as command call.
            // Simplest: if next token after the leading word is also a Word, it's
            // a multi-word command; parse via parse_call_segments.
            if matches!(p.peek_at(1).map(|t| t.tok.clone()), Some(Tok::Word(_))) {
                let line = head.line;
                let segments = parse_call_segments(p)?;
                return Ok(Expr::Call { segments, line });
            }

            // Bare identifier → variable
            p.next();
            Ok(Expr::Var(w))
        }
        _ => Err(ParseError::at(Some(&head), format!("unexpected token in expression: {:?}", head.tok))),
    }
}
