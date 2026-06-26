use std::collections::{BTreeMap, HashMap, HashSet};

use crate::ast::{
    AssignTarget, BashAction, BinOp, CallSegment, Expr, Function, InterpPart, MatchPattern,
    Program, Stmt, StructDef, UnaryOp, Value,
};
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

#[derive(Default)]
pub struct Scope {
    pub vars: HashMap<String, Value>,
    pub consts: HashSet<String>,
}

pub struct Ctx {
    pub imports: HashSet<String>,
    pub current_os: String,
    pub wd: Option<Session>,
    pub wd_unavailable: Option<String>,
    pub headless: bool,
    pub scopes: Vec<Scope>,
    pub functions: HashMap<String, Function>,
    pub structs: HashMap<String, StructDef>,
    pub strict: bool,
    pub capturing: bool,
    pub try_depth: usize,
    /// Current depth of nested user-function calls. Bounded by [`MAX_CALL_DEPTH`] so runaway
    /// recursion in a Rach program yields a diagnostic instead of overflowing the native stack.
    pub call_depth: usize,
    pub source: String,
    pub script_path: String,
    pub log: LogState,
}

impl Ctx {
    pub fn os_matches(&self, want: &str) -> bool {
        let w = want.to_ascii_lowercase();
        self.current_os == w || (w == "macos" && self.current_os == "darwin")
    }

    pub fn lookup(&self, name: &str) -> Option<Value> {
        for scope in self.scopes.iter().rev() {
            if let Some(v) = scope.vars.get(name) { return Some(v.clone()); }
        }
        None
    }

    pub fn set_var(&mut self, name: String, value: Value) -> Result<(), RuntimeError> {
        for scope in self.scopes.iter().rev() {
            if scope.consts.contains(&name) {
                return Err(RuntimeError::new(400, 0, format!("cannot reassign const `{}`", name)));
            }
        }
        for scope in self.scopes.iter_mut().rev() {
            if scope.vars.contains_key(&name) {
                scope.vars.insert(name, value);
                return Ok(());
            }
        }
        if let Some(scope) = self.scopes.last_mut() {
            scope.vars.insert(name, value);
        }
        Ok(())
    }

    pub fn declare_const(&mut self, name: String, value: Value) -> Result<(), RuntimeError> {
        for scope in self.scopes.iter().rev() {
            if scope.consts.contains(&name) {
                return Err(RuntimeError::new(400, 0, format!("const `{}` already defined", name)));
            }
        }
        if let Some(scope) = self.scopes.last_mut() {
            scope.vars.insert(name.clone(), value);
            scope.consts.insert(name);
        }
        Ok(())
    }

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

const RETURN_SIGNAL_CODE:   i64 = -1;
const BREAK_SIGNAL_CODE:    i64 = -2;
const CONTINUE_SIGNAL_CODE: i64 = -3;

/// Maximum depth of nested user-function calls. Runaway recursion (`rach f(): f()`) would
/// otherwise overflow the native stack and abort the process; this turns it into a catchable
/// runtime error. Sized to stay within [`crate::INTERP_STACK_SIZE`], the stack the CLI gives
/// the interpreter thread.
const MAX_CALL_DEPTH: usize = 2000;

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
        // Structs and lambdas are passed by reference through a side-channel
        // to avoid the awful round-trip serialization. We stash the live Value
        // in a thread-local LRU and put a token here.
        Value::Struct { .. } | Value::Lambda { .. } => {
            let tok = stash_value(v.clone());
            format!("N::ref::{}", tok)
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
    if let Some(rest) = s.strip_prefix("N::ref::") {
        if let Ok(tok) = rest.parse::<u64>() {
            if let Some(v) = unstash_value(tok) {
                return v;
            }
        }
        return Value::Nil;
    }
    Value::Str(s.to_string())
}

// ---- Side-channel for non-serializable Values (Struct, Lambda) ----
//
// `return_signal` round-trips through a String, which is fine for primitives
// but loses information for types that contain Stmt or nested Values cheaply.
// Stash them in a thread-local map and return a token instead.
thread_local! {
    static REF_STORE: std::cell::RefCell<std::collections::HashMap<u64, Value>>
        = std::cell::RefCell::new(std::collections::HashMap::new());
    static REF_NEXT: std::cell::Cell<u64> = std::cell::Cell::new(1);
}

fn stash_value(v: Value) -> u64 {
    REF_NEXT.with(|n| {
        let id = n.get();
        n.set(id.wrapping_add(1));
        REF_STORE.with(|s| s.borrow_mut().insert(id, v));
        id
    })
}

fn unstash_value(tok: u64) -> Option<Value> {
    REF_STORE.with(|s| s.borrow_mut().remove(&tok))
}

pub fn make_ctx(strict: bool, source: String, script_path: String) -> Ctx {
    Ctx {
        imports: HashSet::new(),
        current_os: stdlib::os::detect_os_name(),
        wd: None,
        wd_unavailable: None,
        headless: std::env::var("RACH_HEADLESS").map(|v| v == "1" || v.eq_ignore_ascii_case("true")).unwrap_or(false),
        scopes: vec![Scope::default()],
        functions: HashMap::new(),
        structs: HashMap::new(),
        strict,
        capturing: false,
        try_depth: 0,
        call_depth: 0,
        source,
        script_path,
        log: LogState::default(),
    }
}

pub fn run_in_ctx(program: &Program, ctx: &mut Ctx) -> Result<(), RuntimeError> {
    for f in &program.functions {
        ctx.functions.insert(f.name.clone(), f.clone());
    }
    for s in &program.structs {
        ctx.structs.insert(s.name.clone(), s.clone());
    }
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
    let mut structs: HashMap<String, StructDef> = HashMap::new();
    for s in &program.structs {
        structs.insert(s.name.clone(), s.clone());
    }

    let mut ctx = Ctx {
        imports,
        current_os: stdlib::os::detect_os_name(),
        wd: None,
        wd_unavailable: None,
        headless,
        scopes: vec![Scope::default()],
        functions,
        structs,
        strict,
        capturing: false,
        try_depth: 0,
        call_depth: 0,
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
        Stmt::For { vars, iter, body, line } => {
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
                ctx.scopes.push(Scope::default());
                if vars.len() == 1 {
                    ctx.set_var(vars[0].clone(), item)?;
                } else {
                    // tuple unpacking: item must be a list with enough elements
                    let parts = match &item {
                        Value::List(xs) => xs.clone(),
                        other => return Err(RuntimeError::new(400, *line,
                            format!("for: tuple unpack expects a list, got {:?}", other))),
                    };
                    for (i, vname) in vars.iter().enumerate() {
                        let val = parts.get(i).cloned().unwrap_or(Value::Nil);
                        ctx.set_var(vname.clone(), val)?;
                    }
                }
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
        Stmt::Import { path, line } => {
            use std::path::Path;
            let base_path = ctx.script_path.clone();
            let base = if base_path.is_empty() {
                std::path::Path::new(".").to_path_buf()
            } else {
                std::path::Path::new(&base_path).parent()
                    .map(|p| p.to_path_buf())
                    .unwrap_or_else(|| std::path::Path::new(".").to_path_buf())
            };
            let full = base.join(path);
            let source = std::fs::read_to_string(&full).map_err(|e| {
                RuntimeError::new(500, *line, format!("import: cannot read `{}`: {}", full.display(), e))
            })?;
            let tokens = crate::lexer::tokenize(&source).map_err(|e| {
                RuntimeError::new(500, *line, format!("import `{}`: lex error: {}", full.display(), e.message))
            })?;
            let program = crate::parser::parse(tokens).map_err(|e| {
                RuntimeError::new(500, *line, format!("import `{}`: parse error: {}", full.display(), e.message))
            })?;
            // merge structs and functions into current context
            for sd in program.structs {
                ctx.structs.insert(sd.name.clone(), sd);
            }
            for f in &program.functions {
                ctx.functions.insert(f.name.clone(), f.clone());
            }
            // run the top-level body of the imported file
            let main_fn = program.functions.iter().find(|f| f.name == "main").cloned();
            if let Some(m) = main_fn {
                run_block(&m.body, ctx)?;
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
                ctx.scopes.push(Scope::default());
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
        Stmt::Break { line } => Err(RuntimeError::new(BREAK_SIGNAL_CODE, *line, "break")),
        Stmt::Continue { line } => Err(RuntimeError::new(CONTINUE_SIGNAL_CODE, *line, "continue")),
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
                        ctx.set_var(name.clone(), Value::Map(err_map))?;
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
        Stmt::Assign { target, expr, is_const, line } => {
            ctx.capturing = true;
            let v = eval_expr(expr, ctx);
            ctx.capturing = false;
            let v = v?;
            assign_to_target(target, v, *is_const, *line, ctx)
        }
        Stmt::CompoundAssign { target, op, expr, line } => {
            let cur = read_target(target, ctx, *line)?;
            ctx.capturing = true;
            let rhs = eval_expr(expr, ctx);
            ctx.capturing = false;
            let rhs = rhs?;
            let new_val = eval_binary(*op, &cur, &rhs, *line)?;
            assign_to_target(target, new_val, false, *line, ctx)
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
        Stmt::Switch { expr, cases, default, line: _ } => {
            let val = eval_expr(expr, ctx)?;
            let mut matched = false;
            'outer: for (patterns, body) in cases {
                for pat in patterns {
                    let pv = eval_expr(pat, ctx)?;
                    if values_equal(&val, &pv) {
                        ctx.scopes.push(Scope::default());
                        let res = run_block(body, ctx);
                        ctx.scopes.pop();
                        // propagate break/continue/return but swallow them for switch-internal breaks
                        match res {
                            Ok(()) => {}
                            Err(e) if e.code == BREAK_SIGNAL_CODE => {}
                            Err(e) => return Err(e),
                        }
                        matched = true;
                        break 'outer;
                    }
                }
            }
            if !matched {
                if let Some(def) = default {
                    ctx.scopes.push(Scope::default());
                    let res = run_block(def, ctx);
                    ctx.scopes.pop();
                    match res {
                        Ok(()) => {}
                        Err(e) if e.code == BREAK_SIGNAL_CODE => {}
                        Err(e) => return Err(e),
                    }
                }
            }
            Ok(())
        }
        Stmt::DoWhile { body, cond, line: _ } => {
            loop {
                ctx.scopes.push(Scope::default());
                let res = run_block(body, ctx);
                ctx.scopes.pop();
                match res {
                    Ok(()) => {}
                    Err(e) if e.code == BREAK_SIGNAL_CODE => break,
                    Err(e) if e.code == CONTINUE_SIGNAL_CODE => {}
                    Err(e) => return Err(e),
                }
                let v = eval_expr(cond, ctx)?;
                if !v.is_truthy() { break; }
            }
            Ok(())
        }
        Stmt::CFor { init, cond, step, body, line } => {
            ctx.scopes.push(Scope::default());
            if let Some(init_stmt) = init {
                run_stmt(init_stmt, ctx)?;
            }
            loop {
                if let Some(cond_expr) = cond {
                    let v = eval_expr(cond_expr, ctx)?;
                    if !v.is_truthy() { break; }
                }
                ctx.scopes.push(Scope::default());
                let res = run_block(body, ctx);
                ctx.scopes.pop();
                match res {
                    Ok(()) => {}
                    Err(e) if e.code == BREAK_SIGNAL_CODE => { ctx.scopes.pop(); return Ok(()); }
                    Err(e) if e.code == CONTINUE_SIGNAL_CODE => {}
                    Err(e) => { ctx.scopes.pop(); return Err(e); }
                }
                if let Some(step_stmt) = step {
                    run_stmt(step_stmt, ctx).map_err(|e| RuntimeError::new(e.code, *line, e.message))?;
                }
            }
            ctx.scopes.pop();
            Ok(())
        }
        Stmt::Match { expr, arms, line } => {
            let val = eval_expr(expr, ctx)?;
            for arm in arms {
                let mut bindings: HashMap<String, Value> = HashMap::new();
                if match_pattern(&arm.pattern, &val, &mut bindings) {
                    ctx.scopes.push(Scope::default());
                    for (k, v) in bindings {
                        ctx.set_var(k, v)?;
                    }
                    // evaluate guard with bindings in scope
                    let guard_ok = match &arm.guard {
                        None => true,
                        Some(g) => eval_expr(g, ctx)?.is_truthy(),
                    };
                    if guard_ok {
                        let res = run_block(&arm.body, ctx);
                        ctx.scopes.pop();
                        match res {
                            Ok(()) => {}
                            Err(e) if e.code == BREAK_SIGNAL_CODE => {}
                            Err(e) => return Err(e),
                        }
                        return Ok(());
                    }
                    ctx.scopes.pop();
                }
            }
            Ok(())
        }
    }
}

/// Returns true if `val` matches `pattern`, and populates `bindings`.
fn match_pattern(pat: &MatchPattern, val: &Value, bindings: &mut HashMap<String, Value>) -> bool {
    match pat {
        MatchPattern::Wildcard => true,
        MatchPattern::Bind(name) => {
            bindings.insert(name.clone(), val.clone());
            true
        }
        MatchPattern::Literal(lit) => values_equal(val, lit),
        MatchPattern::Range { lo, hi, inclusive } => {
            let v = match val.as_f64() { Some(f) => f, None => return false };
            let l = match lo.as_f64() { Some(f) => f, None => return false };
            let h = match hi.as_f64() { Some(f) => f, None => return false };
            if *inclusive { v >= l && v <= h } else { v >= l && v < h }
        }
        MatchPattern::Or(alts) => {
            alts.iter().any(|a| {
                let mut tmp = HashMap::new();
                if match_pattern(a, val, &mut tmp) { bindings.extend(tmp); true } else { false }
            })
        }
        MatchPattern::List { items, rest } => {
            let elems = match val {
                Value::List(xs) => xs,
                _ => return false,
            };
            if rest.is_none() && elems.len() != items.len() { return false; }
            if rest.is_some() && elems.len() < items.len() { return false; }
            for (i, item_pat) in items.iter().enumerate() {
                if !match_pattern(item_pat, &elems[i], bindings) { return false; }
            }
            if let Some(rest_name) = rest {
                let tail = Value::List(elems[items.len()..].to_vec());
                if !rest_name.is_empty() {
                    bindings.insert(rest_name.clone(), tail);
                }
            }
            true
        }
    }
}

fn read_target(target: &AssignTarget, ctx: &mut Ctx, line: usize) -> Result<Value, RuntimeError> {
    match target {
        AssignTarget::Name(name) => ctx.lookup(name)
            .ok_or_else(|| RuntimeError::new(404, line, format!("undefined variable `{}`", name))),
        AssignTarget::Index { target, key } => {
            let t = eval_expr(target, ctx)?;
            let k = eval_expr(key, ctx)?;
            index_into(&t, &k, line)
        }
        AssignTarget::Field { target, name } => {
            let t = eval_expr(target, ctx)?;
            field_of(&t, name, line)
        }
    }
}

fn assign_to_target(target: &AssignTarget, value: Value, is_const: bool, line: usize, ctx: &mut Ctx) -> Result<(), RuntimeError> {
    match target {
        AssignTarget::Name(name) => {
            if is_const {
                ctx.declare_const(name.clone(), value)?;
            } else {
                ctx.set_var(name.clone(), value)?;
            }
            Ok(())
        }
        AssignTarget::Index { target, key } => {
            let key_val = eval_expr(key, ctx)?;
            mutate_place(target, ctx, line, Box::new(move |container| {
                match container {
                    Value::List(items) => {
                        let i = key_val.as_f64().ok_or_else(|| RuntimeError::new(400, line, "list index must be int"))? as i64;
                        let n = items.len() as i64;
                        let idx = if i < 0 { i + n } else { i };
                        if idx < 0 || idx as usize >= items.len() {
                            return Err(RuntimeError::new(404, line, format!("list index out of range: {}", i)));
                        }
                        items[idx as usize] = value;
                        Ok(())
                    }
                    Value::Map(m) => {
                        m.insert(key_val.as_str(), value);
                        Ok(())
                    }
                    Value::Struct { fields, .. } => {
                        fields.insert(key_val.as_str(), value);
                        Ok(())
                    }
                    other => Err(RuntimeError::new(400, line, format!("cannot index-assign into {:?}", other))),
                }
            }))
        }
        AssignTarget::Field { target, name } => {
            let field_name = name.clone();
            mutate_place(target, ctx, line, Box::new(move |container| {
                match container {
                    Value::Struct { fields, .. } => { fields.insert(field_name, value); Ok(()) }
                    Value::Map(m) => { m.insert(field_name, value); Ok(()) }
                    other => Err(RuntimeError::new(400, line, format!("cannot field-assign on {:?}", other))),
                }
            }))
        }
    }
}

fn mutate_place(expr: &Expr, ctx: &mut Ctx, line: usize, mutator: Box<dyn FnOnce(&mut Value) -> Result<(), RuntimeError>>) -> Result<(), RuntimeError> {
    match expr {
        Expr::Var(name) => {
            let mut v = ctx.lookup(name).ok_or_else(|| RuntimeError::new(404, line, format!("undefined variable `{}`", name)))?;
            mutator(&mut v)?;
            ctx.set_var(name.clone(), v)?;
            Ok(())
        }
        Expr::Field { target, name, line: l } => {
            let field = name.clone();
            let l = *l;
            mutate_place(target, ctx, l, Box::new(move |outer| {
                let inner = field_mut(outer, &field, l)?;
                mutator(inner)
            }))
        }
        Expr::Index { target, key, line: l } => {
            let key_val = eval_expr(key, ctx)?;
            let l = *l;
            mutate_place(target, ctx, l, Box::new(move |outer| {
                let inner = index_mut(outer, &key_val, l)?;
                mutator(inner)
            }))
        }
        _ => Err(RuntimeError::new(400, line, "left-hand side is not assignable")),
    }
}

fn field_mut<'a>(v: &'a mut Value, name: &str, line: usize) -> Result<&'a mut Value, RuntimeError> {
    match v {
        Value::Struct { fields, .. } => fields.get_mut(name).ok_or_else(|| RuntimeError::new(404, line, format!("no field `{}`", name))),
        Value::Map(m) => Ok(m.entry(name.to_string()).or_insert(Value::Nil)),
        other => Err(RuntimeError::new(400, line, format!("cannot access field `{}` on {:?}", name, other))),
    }
}

fn index_mut<'a>(v: &'a mut Value, key: &Value, line: usize) -> Result<&'a mut Value, RuntimeError> {
    match v {
        Value::List(items) => {
            let i = key.as_f64().ok_or_else(|| RuntimeError::new(400, line, "list index must be int"))? as i64;
            let n = items.len() as i64;
            let idx = if i < 0 { i + n } else { i };
            items.get_mut(idx as usize).ok_or_else(|| RuntimeError::new(404, line, format!("list index out of range: {}", i)))
        }
        Value::Map(m) => Ok(m.entry(key.as_str()).or_insert(Value::Nil)),
        Value::Struct { fields, .. } => Ok(fields.entry(key.as_str()).or_insert(Value::Nil)),
        other => Err(RuntimeError::new(400, line, format!("cannot index into {:?}", other))),
    }
}

fn field_of(v: &Value, name: &str, line: usize) -> Result<Value, RuntimeError> {
    match v {
        Value::Struct { fields, .. } => fields.get(name).cloned().ok_or_else(|| RuntimeError::new(404, line, format!("no field `{}`", name))),
        Value::Map(m) => Ok(m.get(name).cloned().unwrap_or(Value::Nil)),
        other => Err(RuntimeError::new(400, line, format!("cannot read field `{}` on {:?}", name, other))),
    }
}

fn index_into(t: &Value, k: &Value, line: usize) -> Result<Value, RuntimeError> {
    match (t, k) {
        (Value::List(items), Value::Int(i)) => {
            let n = items.len() as i64;
            let idx = if *i < 0 { *i + n } else { *i };
            items.get(idx as usize).cloned()
                .ok_or_else(|| RuntimeError::new(404, line, format!("list index out of range: {}", i)))
        }
        (Value::Map(m), _) => {
            let key_str = k.as_str();
            Ok(m.get(&key_str).cloned().unwrap_or(Value::Nil))
        }
        (Value::Struct { fields, .. }, _) => {
            Ok(fields.get(&k.as_str()).cloned().unwrap_or(Value::Nil))
        }
        (Value::Str(s), Value::Int(i)) => {
            let chars: Vec<char> = s.chars().collect();
            let n = chars.len() as i64;
            let idx = if *i < 0 { *i + n } else { *i };
            chars.get(idx as usize).map(|c| Value::Str(c.to_string()))
                .ok_or_else(|| RuntimeError::new(404, line, format!("string index out of range: {}", i)))
        }
        _ => Err(RuntimeError::new(400, line, format!("cannot index {:?} by {:?}", t, k))),
    }
}

/// Dispatch `obj.method(args)`. Returns `Ok(Some(v))` on hit, `Ok(None)` to fall through.
fn call_method(obj: &Value, method: &str, args: &[Value], line: usize, ctx: &mut Ctx) -> Result<Option<Value>, RuntimeError> {
    let result: Option<Value> = match obj {
        Value::Str(s) => match method {
            "upper"       => Some(Value::Str(s.to_uppercase())),
            "lower"       => Some(Value::Str(s.to_lowercase())),
            "trim"        => Some(Value::Str(s.trim().to_string())),
            "len"         => Some(Value::Int(s.chars().count() as i64)),
            "split" => {
                let sep = args.first().map(|v| v.as_str()).unwrap_or_else(|| " ".into());
                let parts: Vec<Value> = if sep.is_empty() {
                    s.chars().map(|c| Value::Str(c.to_string())).collect()
                } else {
                    s.split(sep.as_str()).map(|p| Value::Str(p.to_string())).collect()
                };
                Some(Value::List(parts))
            }
            "replace" => {
                let from = args.first().map(|v| v.as_str()).unwrap_or_default();
                let to   = args.get(1).map(|v| v.as_str()).unwrap_or_default();
                Some(Value::Str(s.replace(from.as_str(), to.as_str())))
            }
            "contains"    => Some(Value::Bool(s.contains(args.first().map(|v| v.as_str()).unwrap_or_default().as_str()))),
            "starts_with" => Some(Value::Bool(s.starts_with(args.first().map(|v| v.as_str()).unwrap_or_default().as_str()))),
            "ends_with"   => Some(Value::Bool(s.ends_with(args.first().map(|v| v.as_str()).unwrap_or_default().as_str()))),
            "find" => {
                let needle = args.first().map(|v| v.as_str()).unwrap_or_default();
                Some(match s.find(needle.as_str()) {
                    Some(i) => Value::Int(s[..i].chars().count() as i64),
                    None    => Value::Int(-1),
                })
            }
            "chars"  => Some(Value::List(s.chars().map(|c| Value::Str(c.to_string())).collect())),
            "lines"  => Some(Value::List(s.lines().map(|l| Value::Str(l.to_string())).collect())),
            "repeat" => {
                let n = args.first().and_then(|v| v.as_f64()).unwrap_or(0.0) as usize;
                Some(Value::Str(s.repeat(n)))
            }
            "is_empty" => Some(Value::Bool(s.is_empty())),
            _ => None,
        },
        Value::List(items) => match method {
            "len"      => Some(Value::Int(items.len() as i64)),
            "is_empty" => Some(Value::Bool(items.is_empty())),
            "contains" => {
                let needle = args.first().unwrap_or(&Value::Nil);
                Some(Value::Bool(items.iter().any(|v| values_equal_pub(v, needle))))
            }
            "append" => {
                let mut new = items.clone();
                if let Some(v) = args.first() { new.push(v.clone()); }
                Some(Value::List(new))
            }
            "pop" => {
                let mut new = items.clone();
                new.pop();
                Some(Value::List(new))
            }
            "reverse" => {
                let mut new = items.clone();
                new.reverse();
                Some(Value::List(new))
            }
            "sort" | "sorted" => {
                let mut new = items.clone();
                new.sort_by(|a, b| a.as_str().cmp(&b.as_str()));
                Some(Value::List(new))
            }
            "join" => {
                let sep = args.first().map(|v| v.as_str()).unwrap_or_default();
                let s = items.iter().map(|v| v.as_str()).collect::<Vec<_>>().join(sep.as_str());
                Some(Value::Str(s))
            }
            "first" => Some(items.first().cloned().unwrap_or(Value::Nil)),
            "last"  => Some(items.last().cloned().unwrap_or(Value::Nil)),
            "slice" => {
                let start = args.first().and_then(|v| v.as_f64()).unwrap_or(0.0) as usize;
                let end   = args.get(1).and_then(|v| v.as_f64()).map(|n| n as usize).unwrap_or(items.len());
                Some(Value::List(items[start.min(items.len())..end.min(items.len())].to_vec()))
            }
            "map" => {
                let f = args.first().cloned().unwrap_or(Value::Nil);
                let mut out = Vec::with_capacity(items.len());
                for item in items {
                    out.push(call_value(&f, vec![item.clone()], line, ctx)?);
                }
                Some(Value::List(out))
            }
            "filter" => {
                let f = args.first().cloned().unwrap_or(Value::Nil);
                let mut out = Vec::new();
                for item in items {
                    if call_value(&f, vec![item.clone()], line, ctx)?.is_truthy() {
                        out.push(item.clone());
                    }
                }
                Some(Value::List(out))
            }
            "reduce" => {
                let f   = args.first().cloned().unwrap_or(Value::Nil);
                let init = args.get(1).cloned();
                let has_init = init.is_some();
                let mut acc = match init {
                    Some(v) => v,
                    None => items.first().cloned().unwrap_or(Value::Nil),
                };
                let start = if has_init { 0 } else { 1 };
                for item in &items[start..] {
                    acc = call_value(&f, vec![acc, item.clone()], line, ctx)?;
                }
                Some(acc)
            }
            _ => None,
        },
        Value::Map(m) => match method {
            "len"      => Some(Value::Int(m.len() as i64)),
            "is_empty" => Some(Value::Bool(m.is_empty())),
            "keys"     => Some(Value::List(m.keys().map(|k| Value::Str(k.clone())).collect())),
            "values"   => Some(Value::List(m.values().cloned().collect())),
            "has" | "contains" => {
                let key = args.first().map(|v| v.as_str()).unwrap_or_default();
                Some(Value::Bool(m.contains_key(key.as_str())))
            }
            "get" => {
                let key = args.first().map(|v| v.as_str()).unwrap_or_default();
                Some(m.get(key.as_str()).cloned().unwrap_or(Value::Nil))
            }
            "set" => {
                let key = args.first().map(|v| v.as_str()).unwrap_or_default();
                let val = args.get(1).cloned().unwrap_or(Value::Nil);
                let mut new = m.clone();
                new.insert(key.to_string(), val);
                Some(Value::Map(new))
            }
            "remove" => {
                let key = args.first().map(|v| v.as_str()).unwrap_or_default();
                let mut new = m.clone();
                new.remove(key.as_str());
                Some(Value::Map(new))
            }
            "items" => {
                let pairs: Vec<Value> = m.iter()
                    .map(|(k, v)| Value::List(vec![Value::Str(k.clone()), v.clone()]))
                    .collect();
                Some(Value::List(pairs))
            }
            _ => None,
        },
        _ => None,
    };
    Ok(result)
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
            for a in args { arg_values.push(eval_expr(a, ctx)?); }
            // First: is `name` a variable holding a Lambda? (e.g. `square = fn(x) -> ...`)
            if let Some(v) = ctx.lookup(name) {
                if matches!(v, Value::Lambda { .. }) {
                    return call_value(&v, arg_values, *line, ctx);
                }
            }
            call_user_function(name, arg_values, *line, ctx)
        }
        Expr::CallValue { callee, args, line } => {
            let mut arg_values = Vec::with_capacity(args.len());
            for a in args { arg_values.push(eval_expr(a, ctx)?); }
            // Method-call syntax: obj.method(args)
            if let Expr::Field { target, name: method, .. } = callee.as_ref() {
                let obj = eval_expr(target, ctx)?;
                if let Some(result) = call_method(&obj, method, &arg_values, *line, ctx)? {
                    return Ok(result);
                }
            }
            // If callee is `Var(name)` and `name` is a known user function, prefer calling
            // it by name (so functions defined later still resolve). Otherwise eval the
            // callee to a Value and call that.
            if let Expr::Var(name) = callee.as_ref() {
                if ctx.functions.contains_key(name) && ctx.lookup(name).is_none() {
                    return call_user_function(name, arg_values, *line, ctx);
                }
            }
            let target = eval_expr(callee, ctx)?;
            call_value(&target, arg_values, *line, ctx)
        }
        Expr::Binary { op, lhs, rhs, line } => {
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
        Expr::Ternary { cond, then_expr, else_expr, line: _ } => {
            let c = eval_expr(cond, ctx)?;
            if c.is_truthy() { eval_expr(then_expr, ctx) } else { eval_expr(else_expr, ctx) }
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
        Expr::StructLit { name, fields, line } => {
            let def = ctx.structs.get(name).cloned()
                .ok_or_else(|| RuntimeError::new(404, *line, format!("undefined struct `{}`", name)))?;
            let mut fmap: BTreeMap<String, Value> = BTreeMap::new();
            for f in &def.fields {
                fmap.insert(f.clone(), Value::Nil);
            }
            for (k, ex) in fields {
                if !def.fields.iter().any(|f| f == k) {
                    return Err(RuntimeError::new(400, *line, format!("struct `{}` has no field `{}`", name, k)));
                }
                let v = eval_expr(ex, ctx)?;
                fmap.insert(k.clone(), v);
            }
            Ok(Value::Struct { name: name.clone(), fields: fmap })
        }
        Expr::Index { target, key, line } => {
            let t = eval_expr(target, ctx)?;
            let k = eval_expr(key, ctx)?;
            index_into(&t, &k, *line)
        }
        Expr::Field { target, name, line } => {
            let t = eval_expr(target, ctx)?;
            field_of(&t, name, *line)
        }
        Expr::Lambda { params, body, line: _ } => {
            // Capture the visible scope by snapshotting all currently-bound names.
            let mut captured: BTreeMap<String, Value> = BTreeMap::new();
            for scope in &ctx.scopes {
                for (k, v) in &scope.vars {
                    captured.insert(k.clone(), v.clone());
                }
            }
            Ok(Value::Lambda { params: params.clone(), body: body.clone(), captured })
        }
        Expr::InterpStr { parts, line: _ } => {
            let mut out = String::new();
            for p in parts {
                match p {
                    InterpPart::Lit(s) => out.push_str(s),
                    InterpPart::Expr(e) => {
                        let v = eval_expr(e, ctx)?;
                        out.push_str(&v.as_str());
                    }
                }
            }
            Ok(Value::Str(out))
        }
    }
}

/// Call any callable Value: `Lambda` (with captured scope) or anything else fails.
pub fn call_value(callee: &Value, args: Vec<Value>, line: usize, ctx: &mut Ctx) -> Result<Value, RuntimeError> {
    match callee {
        Value::Lambda { params, body, captured } => {
            if args.len() != params.len() {
                return Err(RuntimeError::new(400, line, format!(
                    "lambda expects {} arg(s), got {}", params.len(), args.len()
                )));
            }
            let mut frame = Scope::default();
            for (k, v) in captured { frame.vars.insert(k.clone(), v.clone()); }
            for (p, v) in params.iter().zip(args.into_iter()) {
                frame.vars.insert(p.clone(), v);
            }
            ctx.scopes.push(frame);
            let result = run_block(body, ctx);
            ctx.scopes.pop();
            match result {
                Ok(()) => Ok(Value::Nil),
                Err(e) if e.code == RETURN_SIGNAL_CODE => Ok(deserialize_value(&e.message)),
                Err(e) => Err(e),
            }
        }
        other => Err(RuntimeError::new(400, line, format!("not callable: {:?}", other))),
    }
}

fn call_user_function(name: &str, arg_values: Vec<Value>, line: usize, ctx: &mut Ctx) -> Result<Value, RuntimeError> {
    let func = ctx.functions.get(name).cloned()
        .ok_or_else(|| RuntimeError::new(404, line, format!("undefined function `{}`", name)))?;

    let required = func.defaults.iter().filter(|d| d.is_none()).count();
    if arg_values.len() < required || arg_values.len() > func.params.len() {
        return Err(RuntimeError::new(400, line, format!(
            "function `{}` expects {}-{} argument(s), got {}",
            name, required, func.params.len(), arg_values.len()
        )));
    }

    // Evaluate default expressions before pushing the new frame so they see the caller's scope.
    let mut all_args = arg_values;
    for i in all_args.len()..func.params.len() {
        let default_val = match &func.defaults[i] {
            Some(expr) => eval_expr(expr, ctx)?,
            None => return Err(RuntimeError::new(400, line, format!(
                "function `{}`: missing required argument `{}`", name, func.params[i]
            ))),
        };
        all_args.push(default_val);
    }

    if ctx.call_depth >= MAX_CALL_DEPTH {
        return Err(RuntimeError::new(
            500, line,
            format!("call stack exceeded {} frames (infinite recursion?) in `{}`", MAX_CALL_DEPTH, name),
        ));
    }

    let mut frame = Scope::default();
    for (p, v) in func.params.iter().zip(all_args.into_iter()) {
        frame.vars.insert(p.clone(), v);
    }
    ctx.call_depth += 1;
    ctx.scopes.push(frame);
    let result = run_block(&func.body, ctx);
    ctx.scopes.pop();
    ctx.call_depth -= 1;

    match result {
        Ok(()) => Ok(Value::Nil),
        Err(e) if e.code == RETURN_SIGNAL_CODE => Ok(deserialize_value(&e.message)),
        Err(e) => Err(e),
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
        (Value::Struct { name: an, fields: af }, Value::Struct { name: bn, fields: bf }) => {
            an == bn && af.len() == bf.len() && af.iter().all(|(k, v)| bf.get(k).map_or(false, |w| values_equal(v, w)))
        }
        _ => false,
    }
}

fn eval_binary(op: BinOp, l: &Value, r: &Value, line: usize) -> Result<Value, RuntimeError> {
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
        if let (Value::List(a), Value::List(b)) = (l, r) {
            let mut joined = a.clone();
            joined.extend(b.iter().cloned());
            return Ok(Value::List(joined));
        }
    }

    // Bitwise: int-only.
    if matches!(op, BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor | BinOp::Shl | BinOp::Shr) {
        let li = l.as_f64().ok_or_else(|| RuntimeError::new(400, line, format!("bitwise: {:?} is not a number", l)))? as i64;
        let ri = r.as_f64().ok_or_else(|| RuntimeError::new(400, line, format!("bitwise: {:?} is not a number", r)))? as i64;
        let res = match op {
            BinOp::BitAnd => li & ri,
            BinOp::BitOr  => li | ri,
            BinOp::BitXor => li ^ ri,
            BinOp::Shl    => li.checked_shl(ri as u32).unwrap_or(0),
            BinOp::Shr    => li.checked_shr(ri as u32).unwrap_or(0),
            _ => unreachable!(),
        };
        return Ok(Value::Int(res));
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
        UnaryOp::BitNot => {
            let i = v.as_f64().ok_or_else(|| RuntimeError::new(400, line, format!("bit-not: {:?} is not a number", v)))? as i64;
            Ok(Value::Int(!i))
        }
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
