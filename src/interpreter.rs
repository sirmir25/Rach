use std::collections::{HashMap, HashSet};

use crate::ast::{BashAction, BinOp, CallSegment, Expr, Function, Program, Stmt, UnaryOp, Value};
use crate::stdlib;
use crate::stdlib::logging::LogState;
use crate::stdlib::webdriver::Session;
use crate::stdlib::ResolvedSegment;

#[derive(Debug)]
pub struct RuntimeError {
    pub code: i64,
    pub line: usize,
    pub message: String,
}

impl RuntimeError {
    pub fn new(code: i64, line: usize, message: impl Into<String>) -> Self {
        Self { code, line, message: message.into() }
    }
}

pub struct Ctx {
    pub imports: HashSet<String>,
    pub current_os: String,
    pub wd: Option<Session>,
    pub wd_unavailable: Option<String>,
    pub headless: bool,
    /// Variable scopes — pushed on function entry / `for`, popped on exit.
    pub scopes: Vec<HashMap<String, Value>>,
    /// User-defined functions.
    pub functions: HashMap<String, Function>,
    /// `RACH_STRICT=1` — turn `error N` into a hard runtime error that
    /// aborts execution (instead of just printing).
    pub strict: bool,
    /// True while evaluating the RHS of `set x = ...` — stdlib commands check
    /// this to skip side-effecting prints when their result is being captured.
    pub capturing: bool,
    /// Depth of nested `try:` blocks. When > 0, runtime errors propagate
    /// instead of being printed-and-continued.
    pub try_depth: usize,
    /// Source text of the running script, used by the pretty error printer.
    pub source: String,
    /// Path of the running script, shown in error headers.
    pub script_path: String,
    /// Logging state — level, ring buffer, and optional file mirror.
    pub log: LogState,
}

impl Ctx {
    pub fn os_matches(&self, want: &str) -> bool {
        let w = want.to_ascii_lowercase();
        self.current_os == w || (w == "macos" && self.current_os == "darwin")
    }

    pub fn lookup(&self, name: &str) -> Option<Value> {
        for scope in self.scopes.iter().rev() {
            if let Some(v) = scope.get(name) { return Some(v.clone()); }
        }
        None
    }

    pub fn set_var(&mut self, name: String, value: Value) {
        // If the variable already exists in any enclosing scope, update it there.
        // Otherwise, create it in the topmost (current) scope.
        for scope in self.scopes.iter_mut().rev() {
            if scope.contains_key(&name) {
                scope.insert(name, value);
                return;
            }
        }
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name, value);
        }
    }

    /// Print a runtime error in the same multi-line "rustc" format used for
    /// lex / parse errors, with a caret pointing to the offending source line.
    /// `code` echoes the HTTP-ish error code; `line` is 1-based (0 = no line).
    pub fn report_error(&self, code: i64, line: usize, message: &str) {
        report_pretty("runtime", code, &self.script_path, line, message, Some(&self.source));
    }
}

pub fn report_pretty(stage: &str, code: i64, path: &str, line: usize, message: &str, source: Option<&str>) {
    use std::io::IsTerminal;
    let isatty = std::io::stderr().is_terminal();
    let (red, bold, dim, reset) = if isatty {
        ("\x1b[31;1m", "\x1b[1m", "\x1b[2m", "\x1b[0m")
    } else {
        ("", "", "", "")
    };

    eprintln!("{}error[{}]{}: {}{}{}", red, code, reset, bold, message, reset);
    if line > 0 {
        eprintln!("{}  --> {}{}:{}", dim, reset, path, line);
        if let Some(src) = source {
            let lines: Vec<&str> = src.lines().collect();
            let lo = line.saturating_sub(2).max(1);
            let hi = (line + 1).min(lines.len());
            let width = hi.to_string().len();
            eprintln!("{}{:>w$} |{}", dim, "", reset, w = width);
            for n in lo..=hi {
                if n == 0 || n > lines.len() { continue; }
                let mark = if n == line { ">" } else { " " };
                let txt = lines[n - 1];
                if n == line {
                    eprintln!("{}{:>w$} |{} {} {}{}{}", dim, n, reset, mark, red, txt, reset, w = width);
                } else {
                    eprintln!("{}{:>w$} |{} {} {}", dim, n, reset, mark, txt, w = width);
                }
            }
            eprintln!("{}{:>w$} |{}", dim, "", reset, w = width);
        }
    } else {
        eprintln!("{}  --> {}{}", dim, reset, path);
    }
    eprintln!("{}// {} error {} string {}{}", dim, stage, code, line, reset);
}

/// Signal codes carried in `RuntimeError::code` for non-error control flow.
/// All real error codes are >= 0; signals are negative.
const RETURN_SIGNAL_CODE:   i64 = -1;
const BREAK_SIGNAL_CODE:    i64 = -2;
const CONTINUE_SIGNAL_CODE: i64 = -3;

fn return_signal(value: Value) -> RuntimeError {
    RuntimeError { code: RETURN_SIGNAL_CODE, line: 0, message: serialize_value(&value) }
}

fn serialize_value(v: &Value) -> String {
    match v {
        Value::Nil => "N::nil".into(),
        Value::Bool(b) => format!("N::bool::{}", b),
        Value::Int(n) => format!("N::int::{}", n),
        Value::Float(f) => format!("N::float::{}", f),
        Value::Str(s) => format!("N::str::{}", s),
        Value::List(items) => {
            let parts: Vec<String> = items.iter().map(serialize_value).collect();
            format!("N::list::{}", parts.join("\u{1F}"))
        }
        Value::Map(items) => {
            let parts: Vec<String> = items.iter()
                .map(|(k, v)| format!("{}\u{1E}{}", k, serialize_value(v)))
                .collect();
            format!("N::map::{}", parts.join("\u{1F}"))
        }
    }
}

fn deserialize_value(s: &str) -> Value {
    if s == "N::nil" { return Value::Nil; }
    if let Some(rest) = s.strip_prefix("N::bool::") {
        return Value::Bool(rest == "true");
    }
    if let Some(rest) = s.strip_prefix("N::int::") {
        return Value::Int(rest.parse().unwrap_or(0));
    }
    if let Some(rest) = s.strip_prefix("N::float::") {
        return Value::Float(rest.parse().unwrap_or(0.0));
    }
    if let Some(rest) = s.strip_prefix("N::str::") {
        return Value::Str(rest.to_string());
    }
    if let Some(rest) = s.strip_prefix("N::list::") {
        if rest.is_empty() { return Value::List(Vec::new()); }
        let items: Vec<Value> = rest.split('\u{1F}').map(deserialize_value).collect();
        return Value::List(items);
    }
    if let Some(rest) = s.strip_prefix("N::map::") {
        let mut map = std::collections::BTreeMap::new();
        if !rest.is_empty() {
            for entry in rest.split('\u{1F}') {
                if let Some((k, v)) = entry.split_once('\u{1E}') {
                    map.insert(k.to_string(), deserialize_value(v));
                }
            }
        }
        return Value::Map(map);
    }
    Value::Str(s.to_string())
}

/// Build an empty Ctx for use by the REPL or other embedders. Loads no program.
pub fn make_ctx(strict: bool, source: String, script_path: String) -> Ctx {
    Ctx {
        imports: HashSet::new(),
        current_os: stdlib::os::detect_os_name(),
        wd: None,
        wd_unavailable: None,
        headless: std::env::var("RACH_HEADLESS").map(|v| v == "1" || v.eq_ignore_ascii_case("true")).unwrap_or(false),
        scopes: vec![HashMap::new()],
        functions: HashMap::new(),
        strict,
        capturing: false,
        try_depth: 0,
        source,
        script_path,
        log: LogState::default(),
    }
}

/// Run a parsed program against an *existing* Ctx (variables, functions, and
/// browser session persist). Used by the REPL.
pub fn run_in_ctx(program: &Program, ctx: &mut Ctx) -> Result<(), RuntimeError> {
    // Merge any helper functions defined at top level.
    for f in &program.functions {
        ctx.functions.insert(f.name.clone(), f.clone());
    }
    // Then run the implicit/explicit main body, if any, in the persistent scopes.
    if let Some(main) = program.functions.iter().find(|f| f.name == "main") {
        let result = run_block(&main.body, ctx);
        match result {
            Ok(()) => Ok(()),
            Err(e) if e.code == RETURN_SIGNAL_CODE => Ok(()),
            Err(e) => Err(e),
        }
    } else {
        Ok(())
    }
}

pub fn run(program: &Program, source: &str, script_path: &str) -> Result<(), RuntimeError> {
    let known_modules: HashSet<&str> = [
        "web", "browser", "system", "os",
        "linux", "windows", "macos",
        "bash", "ai", "ascii",
    ].into_iter().collect();

    for imp in &program.imports {
        if !known_modules.contains(imp.as_str()) {
            eprintln!("warn: unknown module `{}` (continuing)", imp);
        }
    }

    let imports: HashSet<String> = program.imports.iter().cloned().collect();
    let headless = std::env::var("RACH_HEADLESS").map(|v| v == "1" || v.eq_ignore_ascii_case("true")).unwrap_or(false);
    let strict = std::env::var("RACH_STRICT").map(|v| v == "1" || v.eq_ignore_ascii_case("true")).unwrap_or(false);

    let mut functions: HashMap<String, Function> = HashMap::new();
    for f in &program.functions {
        functions.insert(f.name.clone(), f.clone());
    }

    let mut ctx = Ctx {
        imports,
        current_os: stdlib::os::detect_os_name(),
        wd: None,
        wd_unavailable: None,
        headless,
        scopes: vec![HashMap::new()],
        functions,
        strict,
        capturing: false,
        try_depth: 0,
        source: source.to_string(),
        script_path: script_path.to_string(),
        log: LogState::default(),
    };

    let main = ctx.functions.get("main").cloned()
        .ok_or_else(|| RuntimeError::new(404, 0, "no `main` function defined"))?;

    let result = run_block(&main.body, &mut ctx);
    ctx.wd.take();

    match result {
        Ok(()) => Ok(()),
        Err(e) if e.code == RETURN_SIGNAL_CODE => Ok(()),
        Err(e) => Err(e),
    }
}

fn run_block(stmts: &[Stmt], ctx: &mut Ctx) -> Result<(), RuntimeError> {
    for stmt in stmts {
        run_stmt(stmt, ctx)?;
    }
    Ok(())
}

fn run_stmt(stmt: &Stmt, ctx: &mut Ctx) -> Result<(), RuntimeError> {
    match stmt {
        Stmt::Completed { .. } => {
            println!("completed");
            Ok(())
        }
        Stmt::Error { code, line_ref, line } => {
            eprintln!("error {} string {}", code, line_ref);
            if ctx.strict {
                return Err(RuntimeError::new(*code, *line, "strict mode: error aborts execution"));
            }
            Ok(())
        }
        Stmt::IfOs { os, negate, body, else_body, line } => {
            let mut matched = ctx.os_matches(os);
            if *negate { matched = !matched; }
            if matched {
                run_block(body, ctx)?;
            } else if let Some(eb) = else_body {
                run_block(eb, ctx)?;
            } else {
                println!("// skipped if {}{} block (current os: {}) [line {}]",
                    if *negate { "not " } else { "" }, os, ctx.current_os, line);
            }
            Ok(())
        }
        Stmt::For { var, iter, body, line } => {
            let v = eval_expr(iter, ctx)?;
            let items: Vec<Value> = match v {
                Value::List(xs) => xs,
                Value::Map(m) => m.into_keys().map(Value::Str).collect(),
                Value::Str(s) => s.split(',').map(|x| Value::Str(x.trim().to_string())).collect(),
                Value::Int(n) if n >= 0 => (0..n).map(Value::Int).collect(),
                other => {
                    return Err(RuntimeError::new(400, *line, format!("for: cannot iterate over {:?}", other)));
                }
            };
            for item in items {
                ctx.scopes.push(HashMap::new());
                ctx.set_var(var.clone(), item);
                let res = run_block(body, ctx);
                ctx.scopes.pop();
                match res {
                    Ok(()) => {}
                    Err(e) if e.code == BREAK_SIGNAL_CODE => break,
                    Err(e) if e.code == CONTINUE_SIGNAL_CODE => continue,
                    Err(e) => return Err(e),
                }
            }
            Ok(())
        }
        Stmt::If { cond, body, else_body, line: _ } => {
            let v = eval_expr(cond, ctx)?;
            if v.is_truthy() {
                run_block(body, ctx)?;
            } else if let Some(eb) = else_body {
                run_block(eb, ctx)?;
            }
            Ok(())
        }
        Stmt::While { cond, body, line: _ } => {
            loop {
                let v = eval_expr(cond, ctx)?;
                if !v.is_truthy() { break; }
                ctx.scopes.push(HashMap::new());
                let res = run_block(body, ctx);
                ctx.scopes.pop();
                match res {
                    Ok(()) => {}
                    Err(e) if e.code == BREAK_SIGNAL_CODE => break,
                    Err(e) if e.code == CONTINUE_SIGNAL_CODE => continue,
                    Err(e) => return Err(e),
                }
            }
            Ok(())
        }
        Stmt::Break { line } => {
            Err(RuntimeError::new(BREAK_SIGNAL_CODE, *line, "break"))
        }
        Stmt::Continue { line } => {
            Err(RuntimeError::new(CONTINUE_SIGNAL_CODE, *line, "continue"))
        }
        Stmt::Try { body, rescue_var, rescue_body, line: _ } => {
            ctx.try_depth += 1;
            let res = run_block(body, ctx);
            ctx.try_depth -= 1;
            match res {
                Ok(()) => Ok(()),
                Err(e) if matches!(e.code, RETURN_SIGNAL_CODE | BREAK_SIGNAL_CODE | CONTINUE_SIGNAL_CODE) => Err(e),
                Err(e) => {
                    if let Some(name) = rescue_var {
                        let mut err_map = std::collections::BTreeMap::new();
                        err_map.insert("code".to_string(), Value::Int(e.code));
                        err_map.insert("line".to_string(), Value::Int(e.line as i64));
                        err_map.insert("message".to_string(), Value::Str(e.message));
                        ctx.set_var(name.clone(), Value::Map(err_map));
                    }
                    run_block(rescue_body, ctx)
                }
            }
        }
        Stmt::Assert { cond, message, line } => {
            let v = eval_expr(cond, ctx)?;
            if !v.is_truthy() {
                let msg = match message {
                    Some(e) => eval_expr(e, ctx)?.as_str(),
                    None => "assertion failed".to_string(),
                };
                return Err(RuntimeError::new(400, *line, format!("assert: {}", msg)));
            }
            Ok(())
        }
        Stmt::Set { name, expr, line: _ } => {
            ctx.capturing = true;
            let v = eval_expr(expr, ctx);
            ctx.capturing = false;
            let v = v?;
            ctx.set_var(name.clone(), v);
            Ok(())
        }
        Stmt::BashDsl { action, argument, line } => {
            stdlib::bash::run_bash_dsl(action, argument, *line)?;
            Ok(())
        }
        Stmt::AiGenerate { language, task, line } => {
            stdlib::ai::ai_generate(language, task, *line);
            Ok(())
        }
        Stmt::Call { segments, line } => {
            let resolved = resolve_segments(segments, ctx)?;
            match stdlib::dispatch_resolved(&resolved, *line, ctx) {
                Ok(_) => Ok(()),
                Err(e) if matches!(e.code, RETURN_SIGNAL_CODE | BREAK_SIGNAL_CODE | CONTINUE_SIGNAL_CODE) => Err(e),
                Err(e) => {
                    if ctx.strict || ctx.try_depth > 0 {
                        Err(e)
                    } else {
                        ctx.report_error(e.code, e.line, &e.message);
                        Ok(())
                    }
                }
            }
        }
        Stmt::Return { expr, line: _ } => {
            let v = match expr {
                Some(e) => eval_expr(e, ctx)?,
                None => Value::Nil,
            };
            Err(return_signal(v))
        }
        Stmt::ExprStmt { expr, line: _ } => {
            let _ = eval_expr(expr, ctx)?;
            Ok(())
        }
    }
}

fn resolve_segments(segments: &[CallSegment], ctx: &mut Ctx) -> Result<Vec<ResolvedSegment>, RuntimeError> {
    let mut out = Vec::with_capacity(segments.len());
    for seg in segments {
        let mut positional = Vec::with_capacity(seg.positional.len());
        for e in &seg.positional {
            positional.push(eval_expr(e, ctx)?);
        }
        let mut named = std::collections::BTreeMap::new();
        for (k, e) in &seg.named {
            named.insert(k.clone(), eval_expr(e, ctx)?);
        }
        out.push(ResolvedSegment {
            words: seg.words.clone(),
            positional,
            named,
        });
    }
    Ok(out)
}

fn eval_expr(expr: &Expr, ctx: &mut Ctx) -> Result<Value, RuntimeError> {
    match expr {
        Expr::Lit(v) => Ok(v.clone()),
        Expr::Var(name) => {
            ctx.lookup(name).ok_or_else(|| RuntimeError::new(404, 0, format!("undefined variable `{}`", name)))
        }
        Expr::List(items) => {
            let mut out = Vec::with_capacity(items.len());
            for it in items {
                out.push(eval_expr(it, ctx)?);
            }
            Ok(Value::List(out))
        }
        Expr::Call { segments, line } => {
            let resolved = resolve_segments(segments, ctx)?;
            stdlib::dispatch_resolved(&resolved, *line, ctx)
        }
        Expr::FnCall { name, args, line } => {
            let mut arg_values = Vec::with_capacity(args.len());
            for a in args {
                arg_values.push(eval_expr(a, ctx)?);
            }
            let func = ctx.functions.get(name).cloned()
                .ok_or_else(|| RuntimeError::new(404, *line, format!("undefined function `{}`", name)))?;

            if arg_values.len() != func.params.len() {
                return Err(RuntimeError::new(400, *line, format!(
                    "function `{}` expects {} argument(s), got {}",
                    name, func.params.len(), arg_values.len()
                )));
            }

            let mut frame = HashMap::new();
            for (p, v) in func.params.iter().zip(arg_values.into_iter()) {
                frame.insert(p.clone(), v);
            }
            ctx.scopes.push(frame);
            let result = run_block(&func.body, ctx);
            ctx.scopes.pop();

            match result {
                Ok(()) => Ok(Value::Nil),
                Err(e) if e.code == RETURN_SIGNAL_CODE => Ok(deserialize_value(&e.message)),
                Err(e) => Err(e),
            }
        }
        Expr::Binary { op, lhs, rhs, line } => {
            // Short-circuit `and`/`or` — don't eval rhs unless needed.
            match op {
                BinOp::And => {
                    let l = eval_expr(lhs, ctx)?;
                    if !l.is_truthy() { return Ok(Value::Bool(false)); }
                    let r = eval_expr(rhs, ctx)?;
                    return Ok(Value::Bool(r.is_truthy()));
                }
                BinOp::Or => {
                    let l = eval_expr(lhs, ctx)?;
                    if l.is_truthy() { return Ok(Value::Bool(true)); }
                    let r = eval_expr(rhs, ctx)?;
                    return Ok(Value::Bool(r.is_truthy()));
                }
                _ => {}
            }
            let l = eval_expr(lhs, ctx)?;
            let r = eval_expr(rhs, ctx)?;
            eval_binary(*op, &l, &r, *line)
        }
        Expr::Unary { op, expr, line } => {
            let v = eval_expr(expr, ctx)?;
            eval_unary(*op, &v, *line)
        }
        Expr::MapLit { entries, line } => {
            let mut map = std::collections::BTreeMap::new();
            for (kex, vex) in entries {
                let k = eval_expr(kex, ctx)?.as_str();
                let v = eval_expr(vex, ctx)?;
                map.insert(k, v);
            }
            let _ = line;
            Ok(Value::Map(map))
        }
        Expr::Index { target, key, line } => {
            let t = eval_expr(target, ctx)?;
            let k = eval_expr(key, ctx)?;
            match (&t, &k) {
                (Value::List(items), Value::Int(i)) => {
                    let n = items.len() as i64;
                    let idx = if *i < 0 { *i + n } else { *i };
                    items.get(idx as usize).cloned()
                        .ok_or_else(|| RuntimeError::new(404, *line, format!("list index out of range: {}", i)))
                }
                (Value::Map(m), _) => {
                    let key_str = k.as_str();
                    Ok(m.get(&key_str).cloned().unwrap_or(Value::Nil))
                }
                (Value::Str(s), Value::Int(i)) => {
                    let chars: Vec<char> = s.chars().collect();
                    let n = chars.len() as i64;
                    let idx = if *i < 0 { *i + n } else { *i };
                    chars.get(idx as usize).map(|c| Value::Str(c.to_string()))
                        .ok_or_else(|| RuntimeError::new(404, *line, format!("string index out of range: {}", i)))
                }
                _ => Err(RuntimeError::new(400, *line, format!("cannot index {:?} by {:?}", t, k))),
            }
        }
    }
}

pub fn values_equal_pub(l: &Value, r: &Value) -> bool { values_equal(l, r) }

fn values_equal(l: &Value, r: &Value) -> bool {
    match (l, r) {
        (Value::Nil, Value::Nil) => true,
        (Value::Bool(a), Value::Bool(b)) => a == b,
        (Value::Str(a), Value::Str(b)) => a == b,
        (Value::Int(a), Value::Int(b)) => a == b,
        (Value::Float(a), Value::Float(b)) => a == b,
        (Value::Int(a), Value::Float(b)) | (Value::Float(b), Value::Int(a)) => *a as f64 == *b,
        (Value::List(a), Value::List(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(x, y)| values_equal(x, y))
        }
        (Value::Map(a), Value::Map(b)) => {
            a.len() == b.len() && a.iter().all(|(k, v)| b.get(k).map_or(false, |w| values_equal(v, w)))
        }
        _ => false,
    }
}

fn eval_binary(op: BinOp, l: &Value, r: &Value, line: usize) -> Result<Value, RuntimeError> {
    // Short-circuit logical ops first; these don't coerce to numbers.
    match op {
        BinOp::And => return Ok(Value::Bool(l.is_truthy() && r.is_truthy())),
        BinOp::Or  => return Ok(Value::Bool(l.is_truthy() || r.is_truthy())),
        BinOp::Eq  => return Ok(Value::Bool(values_equal(l, r))),
        BinOp::Ne  => return Ok(Value::Bool(!values_equal(l, r))),
        _ => {}
    }

    if matches!(op, BinOp::Add) {
        if let (Value::Str(a), Value::Str(b)) = (l, r) {
            return Ok(Value::Str(format!("{}{}", a, b)));
        }
    }
    if matches!(op, BinOp::Add) {
        if let (Value::List(a), Value::List(b)) = (l, r) {
            let mut joined = a.clone();
            joined.extend(b.iter().cloned());
            return Ok(Value::List(joined));
        }
    }

    let lf = l.as_f64().ok_or_else(|| RuntimeError::new(400, line, format!("cannot use {:?} as number", l)))?;
    let rf = r.as_f64().ok_or_else(|| RuntimeError::new(400, line, format!("cannot use {:?} as number", r)))?;
    let both_int = matches!((l, r), (Value::Int(_), Value::Int(_)));

    if matches!(op, BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge) {
        return Ok(Value::Bool(match op {
            BinOp::Lt => lf < rf,
            BinOp::Gt => lf > rf,
            BinOp::Le => lf <= rf,
            BinOp::Ge => lf >= rf,
            _ => unreachable!(),
        }));
    }

    let result = match op {
        BinOp::Add => lf + rf,
        BinOp::Sub => lf - rf,
        BinOp::Mul => lf * rf,
        BinOp::Div => {
            if rf == 0.0 { return Err(RuntimeError::new(400, line, "division by zero")); }
            lf / rf
        }
        BinOp::Mod => {
            if rf == 0.0 { return Err(RuntimeError::new(400, line, "modulo by zero")); }
            lf.rem_euclid(rf)
        }
        BinOp::Pow => lf.powf(rf),
        _ => unreachable!(),
    };

    let stay_int = both_int && !matches!(op, BinOp::Div | BinOp::Pow) && result == result.trunc();
    if stay_int {
        Ok(Value::Int(result as i64))
    } else {
        Ok(Value::Float(result))
    }
}

fn eval_unary(op: UnaryOp, v: &Value, line: usize) -> Result<Value, RuntimeError> {
    match op {
        UnaryOp::Neg => match v {
            Value::Int(n) => Ok(Value::Int(-n)),
            Value::Float(f) => Ok(Value::Float(-f)),
            other => {
                let f = other.as_f64().ok_or_else(|| RuntimeError::new(400, line, format!("cannot negate {:?}", other)))?;
                Ok(Value::Float(-f))
            }
        }
        UnaryOp::Not => Ok(Value::Bool(!v.is_truthy())),
    }
}

#[allow(dead_code)]
pub fn _bash_action_label(a: &BashAction) -> &'static str {
    match a {
        BashAction::Generate => "generate",
        BashAction::Search => "search",
        BashAction::WebSearch => "web search",
        BashAction::CompleteOrError => "complete or error",
    }
}
