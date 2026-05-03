use std::collections::BTreeMap;

use crate::ast::{
    AssignTarget, BashAction, BinOp, CallSegment, Expr, Function, InterpPart, Program, Stmt,
    StructDef, UnaryOp, Value,
};
use crate::lexer::{StrPart, Tok, Token};

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
        while matches!(self.peek().map(|t| &t.tok), Some(Tok::Newline) | Some(Tok::Semicolon)) {
            self.pos += 1;
        }
    }
    fn expect_newline(&mut self) -> Result<(), ParseError> {
        match self.peek().map(|t| &t.tok) {
            Some(Tok::Newline) | Some(Tok::Semicolon) => { self.pos += 1; Ok(()) }
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
    let mut structs: Vec<StructDef> = Vec::new();

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

    // Top-level: `rach name(...)` defs, `struct` defs, or bare statements
    // (which get wrapped into an implicit `main`).
    let starts_with_def = matches!(
        p.peek().map(|t| t.tok.clone()),
        Some(Tok::Word(ref w)) if w == "rach" || w == "struct"
    );

    if starts_with_def {
        loop {
            p.skip_newlines();
            let tok = match p.peek() { Some(t) => t.clone(), None => break };
            match &tok.tok {
                Tok::Word(w) if w == "rach" => {
                    let f = parse_function(&mut p)?;
                    functions.push(f);
                }
                Tok::Word(w) if w == "struct" => {
                    let s = parse_struct(&mut p)?;
                    structs.push(s);
                }
                _ => return Err(ParseError::at(Some(&tok), format!("expected `rach`, `struct`, or `import`, got `{:?}`", tok.tok))),
            }
        }
    } else {
        let mut main_body: Vec<Stmt> = Vec::new();
        let main_line = p.peek().map(|t| t.line).unwrap_or(1);
        loop {
            p.skip_newlines();
            let tok = match p.peek() { Some(t) => t.clone(), None => break };
            if matches!(&tok.tok, Tok::Word(w) if w == "rach" || w == "struct") { break; }
            let stmt = parse_stmt(&mut p)?;
            main_body.push(stmt);
        }
        functions.push(Function {
            name: "main".to_string(),
            params: Vec::new(),
            defaults: Vec::new(),
            body: main_body,
            line: main_line,
        });
        loop {
            p.skip_newlines();
            let tok = match p.peek() { Some(t) => t.clone(), None => break };
            match &tok.tok {
                Tok::Word(w) if w == "rach" => {
                    let f = parse_function(&mut p)?;
                    functions.push(f);
                }
                Tok::Word(w) if w == "struct" => {
                    let s = parse_struct(&mut p)?;
                    structs.push(s);
                }
                _ => return Err(ParseError::at(Some(&tok), format!("expected `rach`, `struct`, or end of file, got `{:?}`", tok.tok))),
            }
        }
    }

    Ok(Program { imports, functions, structs })
}

fn expect_end_marker(p: &mut P) -> Result<(), ParseError> {
    let tok = p.next().ok_or_else(|| ParseError::at(None, "expected `end`"))?;
    match &tok.tok {
        Tok::Word(w) if w == "end" || (w.starts_with("end") && w[3..].chars().all(|c| c.is_ascii_digit())) => Ok(()),
        _ => Err(ParseError::at(Some(&tok), "expected `end`")),
    }
}

fn parse_struct(p: &mut P) -> Result<StructDef, ParseError> {
    let header = p.expect_word("struct")?;
    let name_tok = p.next().ok_or_else(|| ParseError::at(Some(&header), "expected struct name"))?;
    let name = match name_tok.tok {
        Tok::Word(s) => s,
        _ => return Err(ParseError::at(Some(&name_tok), "expected struct name")),
    };
    p.expect_tok(&Tok::LBrace, "`{`")?;
    let mut fields: Vec<String> = Vec::new();
    loop {
        p.skip_newlines();
        match p.peek().map(|t| t.tok.clone()) {
            Some(Tok::RBrace) => { p.next(); break; }
            Some(Tok::Comma) => { p.next(); continue; }
            Some(Tok::Word(f)) => { p.next(); fields.push(f); }
            _ => return Err(ParseError::at(p.peek(), "expected field name or `}`")),
        }
    }
    p.expect_newline()?;
    Ok(StructDef { name, fields, line: header.line })
}

fn parse_function(p: &mut P) -> Result<Function, ParseError> {
    let header = p.expect_word("rach")?;
    let name_tok = p.next().ok_or_else(|| ParseError::at(Some(&header), "expected function name"))?;
    let name = match name_tok.tok {
        Tok::Word(s) => s,
        _ => return Err(ParseError::at(Some(&name_tok), "expected function name")),
    };
    p.expect_tok(&Tok::LParen, "`(`")?;

    let mut params: Vec<String> = Vec::new();
    let mut defaults: Vec<Option<Expr>> = Vec::new();
    match p.peek().map(|t| t.tok.clone()) {
        Some(Tok::RParen) => { p.next(); }
        Some(Tok::Int(_)) => {
            p.next();
            p.expect_tok(&Tok::RParen, "`)`")?;
        }
        Some(Tok::Word(_)) => {
            loop {
                let pt = p.next().ok_or_else(|| ParseError::at(None, "expected param name"))?;
                match pt.tok {
                    Tok::Word(s) => params.push(s),
                    _ => return Err(ParseError::at(Some(&pt), "expected param name")),
                }
                // C++ default param: `name = expr`
                if matches!(p.peek().map(|t| t.tok.clone()), Some(Tok::Equals)) {
                    p.next();
                    defaults.push(Some(parse_expr(p)?));
                } else {
                    defaults.push(None);
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

    // Two body forms:
    //   (a) clean:   `rach name(...):`  ... `end`     (newline)
    //   (b) legacy:  `rach name(...)`   ... `return(end)` `(end0)`
    let clean_form = matches!(p.peek().map(|t| t.tok.clone()), Some(Tok::Colon));

    if clean_form {
        p.next(); // `:`
        p.expect_newline()?;
        let body = parse_block(p, 0)?;
        let end_tok = p.next().ok_or_else(|| ParseError::at(None, "expected `end` to close function"))?;
        match &end_tok.tok {
            Tok::Word(w) if w == "end" => {}
            _ => return Err(ParseError::at(Some(&end_tok), "expected `end` to close function")),
        }
        let _ = p.expect_newline();
        return Ok(Function { name, params, defaults, body, line: header.line });
    }

    p.expect_newline()?;
    p.skip_newlines();

    let body = parse_block(p, 0)?;

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

    Ok(Function { name, params, defaults, body, line: header.line })
}

fn parse_block(p: &mut P, min_indent_col: usize) -> Result<Vec<Stmt>, ParseError> {
    let mut stmts = Vec::new();
    loop {
        p.skip_newlines();
        let tok = match p.peek() {
            Some(t) => t.clone(),
            None => break,
        };

        // Function-end markers (legacy and clean form).
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
            if matches!(&tok.tok, Tok::Word(w) if w == "end") {
                break;
            }
        }
        if matches!(&tok.tok, Tok::LParen) { break; }

        if min_indent_col > 0 && tok.col <= min_indent_col {
            break;
        }

        // `else:`, `rescue`, and `end` mark block boundaries.
        if matches!(&tok.tok, Tok::Word(w) if w == "else") {
            if let Some(Tok::Colon) = p.peek_at(1).map(|t| t.tok.clone()) {
                break;
            }
        }
        if matches!(&tok.tok, Tok::Word(w) if w == "rescue") {
            break;
        }
        if matches!(&tok.tok, Tok::Word(w) if w == "end") {
            break;
        }

        let stmt = parse_stmt(p)?;
        stmts.push(stmt);
    }
    Ok(stmts)
}

/// Parses a mini-statement inside `for (init; cond; step)` — no trailing newline.
/// Handles: `set x = expr`, `x = expr`, `x++`, `x--`, `x += expr`.
fn parse_cfor_mini_stmt(p: &mut P) -> Result<Option<Stmt>, ParseError> {
    let tok = match p.peek().cloned() { Some(t) => t, None => return Ok(None) };
    let line = tok.line;

    // Extract name and consume tokens up to the operator.
    let name = if matches!(&tok.tok, Tok::Word(w) if w == "set") {
        p.next(); // consume "set"
        let nt = p.next().ok_or_else(|| ParseError::at(None, "expected variable name"))?;
        match nt.tok { Tok::Word(s) => s, _ => return Err(ParseError::at(Some(&nt), "expected variable name")) }
    } else if let Tok::Word(w) = &tok.tok {
        let n = w.clone();
        p.next(); // consume the name
        n
    } else {
        return Ok(None);
    };

    // Now p.peek() is the operator.
    match p.peek().map(|t| t.tok.clone()) {
        Some(Tok::PlusPlus) => {
            p.next();
            Ok(Some(Stmt::CompoundAssign { target: AssignTarget::Name(name), op: BinOp::Add, expr: Expr::Lit(Value::Int(1)), line }))
        }
        Some(Tok::MinusMinus) => {
            p.next();
            Ok(Some(Stmt::CompoundAssign { target: AssignTarget::Name(name), op: BinOp::Sub, expr: Expr::Lit(Value::Int(1)), line }))
        }
        Some(Tok::PlusEq)  => { p.next(); Ok(Some(Stmt::CompoundAssign { target: AssignTarget::Name(name), op: BinOp::Add, expr: parse_expr(p)?, line })) }
        Some(Tok::MinusEq) => { p.next(); Ok(Some(Stmt::CompoundAssign { target: AssignTarget::Name(name), op: BinOp::Sub, expr: parse_expr(p)?, line })) }
        Some(Tok::StarEq)  => { p.next(); Ok(Some(Stmt::CompoundAssign { target: AssignTarget::Name(name), op: BinOp::Mul, expr: parse_expr(p)?, line })) }
        Some(Tok::SlashEq) => { p.next(); Ok(Some(Stmt::CompoundAssign { target: AssignTarget::Name(name), op: BinOp::Div, expr: parse_expr(p)?, line })) }
        Some(Tok::Equals)  => { p.next(); Ok(Some(Stmt::Assign { target: AssignTarget::Name(name), expr: parse_expr(p)?, is_const: false, line })) }
        _ => Ok(None),
    }
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
        if matches!(p.peek().map(|t| t.tok.clone()), Some(Tok::Newline) | Some(Tok::Semicolon) | None) {
            p.expect_newline()?;
            return Ok(Stmt::Return { expr: None, line: head_line });
        }
        let expr = parse_expr(p)?;
        p.expect_newline()?;
        return Ok(Stmt::Return { expr: Some(expr), line: head_line });
    }

    if word == "set" || word == "const" {
        let is_const = word == "const";
        p.next();
        let name_tok = p.next().ok_or_else(|| ParseError::at(None, "expected variable name"))?;
        let name = match name_tok.tok {
            Tok::Word(s) => s,
            _ => return Err(ParseError::at(Some(&name_tok), "expected variable name")),
        };
        p.expect_tok(&Tok::Equals, "`=`")?;
        let expr = parse_expr(p)?;
        p.expect_newline()?;
        return Ok(Stmt::Assign { target: AssignTarget::Name(name), expr, is_const, line: head_line });
    }

    if word == "if" {
        p.next();
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
        // C-style: `for (init; cond; step):`
        if matches!(p.peek().map(|t| t.tok.clone()), Some(Tok::LParen)) {
            p.next(); // `(`
            let init = parse_cfor_mini_stmt(p)?;
            p.expect_tok(&Tok::Semicolon, "`;`")?;
            let cond = if matches!(p.peek().map(|t| t.tok.clone()), Some(Tok::Semicolon)) {
                None
            } else {
                Some(parse_expr(p)?)
            };
            p.expect_tok(&Tok::Semicolon, "`;`")?;
            let step = if matches!(p.peek().map(|t| t.tok.clone()), Some(Tok::RParen)) {
                None
            } else {
                parse_cfor_mini_stmt(p)?
            };
            p.expect_tok(&Tok::RParen, "`)`")?;
            p.expect_tok(&Tok::Colon, "`:`")?;
            p.expect_newline()?;
            let body = parse_block(p, head_col)?;
            return Ok(Stmt::CFor { init: init.map(Box::new), cond, step: step.map(Box::new), body, line: head_line });
        }
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

    if word == "switch" {
        p.next();
        let expr = parse_expr(p)?;
        p.expect_tok(&Tok::Colon, "`:`")?;
        p.expect_newline()?;
        let mut cases: Vec<(Vec<Expr>, Vec<Stmt>)> = Vec::new();
        let mut default: Option<Vec<Stmt>> = None;
        loop {
            p.skip_newlines();
            let next = match p.peek() { Some(t) => t.clone(), None => break };
            if next.col <= head_col && next.col < head_col + 4 { break; }
            let kw = match &next.tok { Tok::Word(w) => w.clone(), _ => break };
            if kw == "case" {
                p.next();
                let mut vals = vec![parse_expr(p)?];
                while matches!(p.peek().map(|t| t.tok.clone()), Some(Tok::Comma)) {
                    p.next();
                    vals.push(parse_expr(p)?);
                }
                p.expect_tok(&Tok::Colon, "`:`")?;
                p.expect_newline()?;
                let case_body = parse_block(p, next.col)?;
                cases.push((vals, case_body));
            } else if kw == "default" {
                p.next();
                p.expect_tok(&Tok::Colon, "`:`")?;
                p.expect_newline()?;
                default = Some(parse_block(p, next.col)?);
            } else {
                break;
            }
        }
        return Ok(Stmt::Switch { expr, cases, default, line: head_line });
    }

    if word == "do" {
        p.next();
        p.expect_tok(&Tok::Colon, "`:`")?;
        p.expect_newline()?;
        let body = parse_block(p, head_col)?;
        p.skip_newlines();
        p.expect_word("while")?;
        let cond = parse_expr(p)?;
        p.expect_newline()?;
        return Ok(Stmt::DoWhile { body, cond, line: head_line });
    }

    // `WORD = ...` / `WORD += ...` / `WORD++` / `WORD.field = ...` / `WORD[idx] = ...`
    let next1 = p.peek_at(1).map(|t| t.tok.clone());
    if let Some(Tok::Equals) = &next1 {
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
            p.next();
            p.next();
            let expr = parse_expr(p)?;
            p.expect_newline()?;
            return Ok(Stmt::Assign { target: AssignTarget::Name(word), expr, is_const: false, line: head_line });
        }
    }

    if let Some(op_tok) = next1.as_ref().and_then(compound_op) {
        p.next(); // word
        p.next(); // op=
        let expr = parse_expr(p)?;
        p.expect_newline()?;
        return Ok(Stmt::CompoundAssign {
            target: AssignTarget::Name(word),
            op: op_tok,
            expr,
            line: head_line,
        });
    }

    if matches!(&next1, Some(Tok::PlusPlus) | Some(Tok::MinusMinus)) {
        let op = match next1.unwrap() {
            Tok::PlusPlus => BinOp::Add,
            Tok::MinusMinus => BinOp::Sub,
            _ => unreachable!(),
        };
        p.next(); // word
        p.next(); // ++/--
        p.expect_newline()?;
        return Ok(Stmt::CompoundAssign {
            target: AssignTarget::Name(word),
            op,
            expr: Expr::Lit(Value::Int(1)),
            line: head_line,
        });
    }

    // `WORD.field = ...` / `WORD.field += ...` / `WORD[i] = ...`
    if matches!(&next1, Some(Tok::Dot) | Some(Tok::LBracket)) {
        let saved = p.pos;
        // Try to parse a place-expr (Var + chain of .field / [idx]) and check for `=` or compound after it.
        if let Some(stmt) = try_parse_place_assign(p, &word, head_line)? {
            return Ok(stmt);
        }
        p.pos = saved;
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
                        Tok::Str(parts) => match parts.first() {
                            Some(StrPart::Lit(s)) if parts.len() == 1 => s.clone(),
                            _ => parts.iter().filter_map(|p| if let StrPart::Lit(s) = p { Some(s.clone()) } else { None }).collect::<Vec<_>>().join(""),
                        },
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

fn compound_op(t: &Tok) -> Option<BinOp> {
    match t {
        Tok::PlusEq => Some(BinOp::Add),
        Tok::MinusEq => Some(BinOp::Sub),
        Tok::StarEq => Some(BinOp::Mul),
        Tok::SlashEq => Some(BinOp::Div),
        Tok::PercentEq => Some(BinOp::Mod),
        _ => None,
    }
}

/// Try to parse `WORD (.field | [expr])+ (= | += ...) expr`. Returns Ok(Some) on success,
/// Ok(None) if we don't see an assignment token after the place-expr.
fn try_parse_place_assign(p: &mut P, base_name: &str, line: usize) -> Result<Option<Stmt>, ParseError> {
    p.next(); // consume base word
    let mut target_expr = Expr::Var(base_name.to_string());
    let mut last_field: Option<String> = None;
    let mut last_index: Option<Expr> = None;

    loop {
        match p.peek().map(|t| t.tok.clone()) {
            Some(Tok::Dot) => {
                if let Some(field) = last_field.take() {
                    target_expr = Expr::Field { target: Box::new(target_expr), name: field, line };
                } else if let Some(key) = last_index.take() {
                    target_expr = Expr::Index { target: Box::new(target_expr), key: Box::new(key), line };
                }
                p.next();
                let ft = p.next().ok_or_else(|| ParseError::at(None, "expected field name after `.`"))?;
                let fname = match ft.tok {
                    Tok::Word(s) => s,
                    _ => return Err(ParseError::at(Some(&ft), "expected field name after `.`")),
                };
                last_field = Some(fname);
            }
            Some(Tok::LBracket) => {
                if let Some(field) = last_field.take() {
                    target_expr = Expr::Field { target: Box::new(target_expr), name: field, line };
                } else if let Some(key) = last_index.take() {
                    target_expr = Expr::Index { target: Box::new(target_expr), key: Box::new(key), line };
                }
                p.next();
                let key = parse_expr(p)?;
                p.expect_tok(&Tok::RBracket, "`]`")?;
                last_index = Some(key);
            }
            _ => break,
        }
    }

    let op_tok = p.peek().map(|t| t.tok.clone());
    let target = match (last_field, last_index) {
        (Some(field), None) => AssignTarget::Field { target: Box::new(target_expr), name: field },
        (None, Some(key)) => AssignTarget::Index { target: Box::new(target_expr), key: Box::new(key) },
        _ => return Ok(None), // shouldn't happen — caller guarantees `.` or `[`
    };

    if matches!(op_tok, Some(Tok::Equals)) {
        p.next();
        let expr = parse_expr(p)?;
        p.expect_newline()?;
        return Ok(Some(Stmt::Assign { target, expr, is_const: false, line }));
    }
    if let Some(op) = op_tok.as_ref().and_then(compound_op) {
        p.next();
        let expr = parse_expr(p)?;
        p.expect_newline()?;
        return Ok(Some(Stmt::CompoundAssign { target, op, expr, line }));
    }
    Ok(None)
}

fn parse_bash_dsl_rhs(p: &mut P) -> Result<(BashAction, String), ParseError> {
    let mut tokens: Vec<String> = Vec::new();
    loop {
        match p.peek().map(|t| t.tok.clone()) {
            Some(Tok::Newline) | Some(Tok::Semicolon) | None => break,
            Some(Tok::Word(w)) => { p.next(); tokens.push(w); }
            Some(Tok::Str(parts)) => {
                p.next();
                let s: String = parts.iter().filter_map(|pa| if let StrPart::Lit(s) = pa { Some(s.clone()) } else { None }).collect::<Vec<_>>().join("");
                tokens.push(s);
            }
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
    if let (Some(Tok::Word(w)), Some(Tok::LParen)) = (
        p.peek().map(|t| t.tok.clone()),
        p.peek_at(1).map(|t| t.tok.clone()),
    ) {
        if !crate::stdlib::is_known_command(&w) {
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
            // Could be followed by chained postfix ops (e.g. `f()(x)`, `f().field`).
            let chained = parse_postfix_chain(p, Expr::FnCall { name: w, args, line })?;
            p.expect_newline()?;
            return Ok(Stmt::ExprStmt { expr: chained, line });
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
                p.next();
                p.next();
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

/// Expression precedence (low to high):
///   ternary → or → and → not → comparison → bit-or → bit-xor → bit-and
///   → shift → add/sub → mul/div/mod → power (right-assoc) → unary → postfix → primary.
pub fn parse_expr(p: &mut P) -> Result<Expr, ParseError> {
    parse_ternary(p)
}

fn parse_ternary(p: &mut P) -> Result<Expr, ParseError> {
    let cond = parse_logical_or(p)?;
    if matches!(p.peek().map(|t| t.tok.clone()), Some(Tok::Question)) {
        let line = p.peek().map(|t| t.line).unwrap_or(0);
        p.next();
        let then_expr = parse_ternary(p)?;
        p.expect_tok(&Tok::Colon, "`:` in ternary")?;
        let else_expr = parse_ternary(p)?;
        return Ok(Expr::Ternary {
            cond: Box::new(cond),
            then_expr: Box::new(then_expr),
            else_expr: Box::new(else_expr),
            line,
        });
    }
    Ok(cond)
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
    let left = parse_bit_or(p)?;
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
        let right = parse_bit_or(p)?;
        return Ok(Expr::Binary { op, lhs: Box::new(left), rhs: Box::new(right), line });
    }
    Ok(left)
}

fn parse_bit_or(p: &mut P) -> Result<Expr, ParseError> {
    let mut left = parse_bit_xor(p)?;
    while matches!(p.peek().map(|t| t.tok.clone()), Some(Tok::Pipe)) {
        let line = p.peek().map(|t| t.line).unwrap_or(0);
        p.next();
        let right = parse_bit_xor(p)?;
        left = Expr::Binary { op: BinOp::BitOr, lhs: Box::new(left), rhs: Box::new(right), line };
    }
    Ok(left)
}

fn parse_bit_xor(p: &mut P) -> Result<Expr, ParseError> {
    let mut left = parse_bit_and(p)?;
    while matches!(p.peek().map(|t| t.tok.clone()), Some(Tok::CaretCaret)) {
        let line = p.peek().map(|t| t.line).unwrap_or(0);
        p.next();
        let right = parse_bit_and(p)?;
        left = Expr::Binary { op: BinOp::BitXor, lhs: Box::new(left), rhs: Box::new(right), line };
    }
    Ok(left)
}

fn parse_bit_and(p: &mut P) -> Result<Expr, ParseError> {
    let mut left = parse_shift(p)?;
    while matches!(p.peek().map(|t| t.tok.clone()), Some(Tok::Amp)) {
        let line = p.peek().map(|t| t.line).unwrap_or(0);
        p.next();
        let right = parse_shift(p)?;
        left = Expr::Binary { op: BinOp::BitAnd, lhs: Box::new(left), rhs: Box::new(right), line };
    }
    Ok(left)
}

fn parse_shift(p: &mut P) -> Result<Expr, ParseError> {
    let mut left = parse_additive(p)?;
    loop {
        let op = match p.peek().map(|t| t.tok.clone()) {
            Some(Tok::LtLt) => BinOp::Shl,
            Some(Tok::GtGt) => BinOp::Shr,
            _ => break,
        };
        let line = p.peek().map(|t| t.line).unwrap_or(0);
        p.next();
        let right = parse_additive(p)?;
        left = Expr::Binary { op, lhs: Box::new(left), rhs: Box::new(right), line };
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
        Some(Tok::Tilde) => {
            let line = p.peek().map(|t| t.line).unwrap_or(0);
            p.next();
            let inner = parse_unary(p)?;
            Ok(Expr::Unary { op: UnaryOp::BitNot, expr: Box::new(inner), line })
        }
        _ => parse_primary(p),
    }
}

fn parse_primary(p: &mut P) -> Result<Expr, ParseError> {
    let base = parse_atom(p)?;
    parse_postfix_chain(p, base)
}

/// Postfix chain: `[idx]`, `.name`, `(args)`. Lets us write `f(x).field[0]`.
fn parse_postfix_chain(p: &mut P, mut target: Expr) -> Result<Expr, ParseError> {
    loop {
        match p.peek().map(|t| t.tok.clone()) {
            Some(Tok::LBracket) => {
                let line = p.peek().map(|t| t.line).unwrap_or(0);
                p.next();
                let key = parse_expr(p)?;
                p.expect_tok(&Tok::RBracket, "`]`")?;
                target = Expr::Index { target: Box::new(target), key: Box::new(key), line };
            }
            Some(Tok::Dot) => {
                let line = p.peek().map(|t| t.line).unwrap_or(0);
                p.next();
                let nt = p.next().ok_or_else(|| ParseError::at(None, "expected field name after `.`"))?;
                let name = match nt.tok {
                    Tok::Word(s) => s,
                    _ => return Err(ParseError::at(Some(&nt), "expected field name after `.`")),
                };
                target = Expr::Field { target: Box::new(target), name, line };
            }
            Some(Tok::LParen) => {
                let line = p.peek().map(|t| t.line).unwrap_or(0);
                p.next();
                let mut args: Vec<Expr> = Vec::new();
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
                target = Expr::CallValue { callee: Box::new(target), args, line };
            }
            _ => break,
        }
    }
    Ok(target)
}

fn parse_lambda(p: &mut P) -> Result<Expr, ParseError> {
    let header_line = p.peek().map(|t| t.line).unwrap_or(0);
    p.expect_word("fn")?;
    p.expect_tok(&Tok::LParen, "`(`")?;
    let mut params: Vec<String> = Vec::new();
    loop {
        match p.peek().map(|t| t.tok.clone()) {
            Some(Tok::RParen) => { p.next(); break; }
            Some(Tok::Comma) => { p.next(); continue; }
            Some(Tok::Word(w)) => { p.next(); params.push(w); }
            _ => return Err(ParseError::at(p.peek(), "expected param name or `)`")),
        }
    }
    // Two body forms:
    //   fn(x) -> expr               (single-expr lambda)
    //   fn(x): ... end              (block lambda)
    if matches!(p.peek().map(|t| t.tok.clone()), Some(Tok::Arrow)) {
        p.next();
        let expr = parse_expr(p)?;
        let body = vec![Stmt::Return { expr: Some(expr), line: header_line }];
        return Ok(Expr::Lambda { params, body, line: header_line });
    }
    if matches!(p.peek().map(|t| t.tok.clone()), Some(Tok::Colon)) {
        p.next();
        p.expect_newline()?;
        let body = parse_block(p, 0)?;
        let end_tok = p.next().ok_or_else(|| ParseError::at(None, "expected `end` to close lambda"))?;
        match &end_tok.tok {
            Tok::Word(w) if w == "end" => {}
            _ => return Err(ParseError::at(Some(&end_tok), "expected `end` to close lambda")),
        }
        return Ok(Expr::Lambda { params, body, line: header_line });
    }
    Err(ParseError::at(p.peek(), "expected `->` or `:` after lambda parameters"))
}

fn parse_atom(p: &mut P) -> Result<Expr, ParseError> {
    let head = p.peek().cloned().ok_or_else(|| ParseError::at(None, "expected expression"))?;
    match head.tok.clone() {
        Tok::Str(parts) => { p.next(); Ok(build_string_expr(parts, head.line)?) }
        Tok::Int(n) => { p.next(); Ok(Expr::Lit(Value::Int(n))) }
        Tok::Float(f) => { p.next(); Ok(Expr::Lit(Value::Float(f))) }
        Tok::LBrace => {
            let line = head.line;
            p.next();
            let mut entries: Vec<(Expr, Expr)> = Vec::new();
            loop {
                match p.peek().map(|t| t.tok.clone()) {
                    Some(Tok::RBrace) => { p.next(); break; }
                    Some(Tok::Comma) => { p.next(); continue; }
                    Some(Tok::Newline) => { p.next(); continue; }
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
            if w == "true" { p.next(); return Ok(Expr::Lit(Value::Bool(true))); }
            if w == "false" { p.next(); return Ok(Expr::Lit(Value::Bool(false))); }
            if w == "nil" { p.next(); return Ok(Expr::Lit(Value::Nil)); }
            if w == "fn" { return parse_lambda(p); }

            // `Module::name` — collapse into single dotted name.
            if matches!(p.peek_at(1).map(|t| t.tok.clone()), Some(Tok::ColonColon)) {
                p.next(); // module word
                p.next(); // ::
                let nt = p.next().ok_or_else(|| ParseError::at(None, "expected name after `::`"))?;
                let nm = match nt.tok {
                    Tok::Word(s) => s,
                    _ => return Err(ParseError::at(Some(&nt), "expected name after `::`")),
                };
                let combined = format!("{}_{}", w, nm);
                if matches!(p.peek().map(|t| t.tok.clone()), Some(Tok::LParen)) {
                    let line = head.line;
                    let _ = p.next();
                    let mut args = Vec::new();
                    loop {
                        match p.peek().map(|t| t.tok.clone()) {
                            Some(Tok::RParen) => { p.next(); break; }
                            Some(Tok::Comma) => { p.next(); continue; }
                            _ => { let e = parse_expr(p)?; args.push(e); }
                        }
                    }
                    return Ok(Expr::FnCall { name: combined, args, line });
                }
                return Ok(Expr::Var(combined));
            }

            // `Name { x: 1, y: 2 }` — struct literal. Reserve only when next `{` looks like field list.
            // Heuristic: capitalized identifier OR followed by `Word :` inside braces.
            if matches!(p.peek_at(1).map(|t| t.tok.clone()), Some(Tok::LBrace)) {
                let looks_like_struct = matches!(
                    (p.peek_at(2).map(|t| t.tok.clone()), p.peek_at(3).map(|t| t.tok.clone())),
                    (Some(Tok::Word(_)), Some(Tok::Colon)) | (Some(Tok::RBrace), _)
                );
                if looks_like_struct {
                    p.next(); // word
                    p.next(); // {
                    let mut fields: Vec<(String, Expr)> = Vec::new();
                    loop {
                        p.skip_newlines();
                        match p.peek().map(|t| t.tok.clone()) {
                            Some(Tok::RBrace) => { p.next(); break; }
                            Some(Tok::Comma) => { p.next(); continue; }
                            Some(Tok::Word(fname)) => {
                                p.next();
                                p.expect_tok(&Tok::Colon, "`:`")?;
                                let v = parse_expr(p)?;
                                fields.push((fname, v));
                            }
                            _ => return Err(ParseError::at(p.peek(), "expected field name in struct literal")),
                        }
                    }
                    return Ok(Expr::StructLit { name: w, fields, line: head.line });
                }
            }

            if matches!(p.peek_at(1).map(|t| t.tok.clone()), Some(Tok::LParen)) {
                if crate::stdlib::is_known_command(&w) {
                    let line = head.line;
                    let segments = parse_call_segments(p)?;
                    return Ok(Expr::Call { segments, line });
                } else {
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

            if matches!(p.peek_at(1).map(|t| t.tok.clone()), Some(Tok::Word(_))) {
                let line = head.line;
                let segments = parse_call_segments(p)?;
                return Ok(Expr::Call { segments, line });
            }

            p.next();
            Ok(Expr::Var(w))
        }
        _ => Err(ParseError::at(Some(&head), format!("unexpected token in expression: {:?}", head.tok))),
    }
}

/// Convert lexer's `Tok::Str(parts)` into either a plain string literal (when
/// no `{...}` interpolation is used) or an `InterpStr` expression. Embedded
/// expression sources are re-tokenized + re-parsed here.
fn build_string_expr(parts: Vec<StrPart>, line: usize) -> Result<Expr, ParseError> {
    let has_interp = parts.iter().any(|p| matches!(p, StrPart::Expr(_)));
    if !has_interp {
        let s: String = parts.into_iter().filter_map(|p| if let StrPart::Lit(s) = p { Some(s) } else { None }).collect::<Vec<_>>().join("");
        return Ok(Expr::Lit(Value::Str(s)));
    }
    let mut out: Vec<InterpPart> = Vec::new();
    for part in parts {
        match part {
            StrPart::Lit(s) => out.push(InterpPart::Lit(s)),
            StrPart::Expr(src) => {
                let tokens = crate::lexer::tokenize(&src)
                    .map_err(|e| ParseError { line, message: format!("interp: lex error: {}", e.message) })?;
                let mut sub = P::new(tokens);
                sub.skip_newlines();
                let e = parse_expr(&mut sub)?;
                out.push(InterpPart::Expr(e));
            }
        }
    }
    Ok(Expr::InterpStr { parts: out, line })
}
