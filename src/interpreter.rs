use std::collections::{HashMap, HashSet};

use crate::ast::{BashAction, CallSegment, Expr, Function, Program, Stmt, Value};
use crate::stdlib;
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
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name, value);
        }
    }
}

/// Signal value carried in `Result::Err` to mean "non-error early return"
/// from a user function. Distinguished from real runtime errors by code 0.
const RETURN_SIGNAL_CODE: i64 = -1;

fn return_signal(value: Value) -> RuntimeError {
    RuntimeError { code: RETURN_SIGNAL_CODE, line: 0, message: serialize_value(&value) }
}

fn serialize_value(v: &Value) -> String {
    // Use a sentinel-tagged form so we can round-trip exactly the value.
    match v {
        Value::Nil => "N::nil".into(),
        Value::Bool(b) => format!("N::bool::{}", b),
        Value::Int(n) => format!("N::int::{}", n),
        Value::Str(s) => format!("N::str::{}", s),
        Value::List(items) => {
            let parts: Vec<String> = items.iter().map(serialize_value).collect();
            format!("N::list::{}", parts.join("\u{1F}"))
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
    if let Some(rest) = s.strip_prefix("N::str::") {
        return Value::Str(rest.to_string());
    }
    if let Some(rest) = s.strip_prefix("N::list::") {
        if rest.is_empty() { return Value::List(Vec::new()); }
        let items: Vec<Value> = rest.split('\u{1F}').map(deserialize_value).collect();
        return Value::List(items);
    }
    Value::Str(s.to_string())
}

pub fn run(program: &Program) -> Result<(), RuntimeError> {
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
            let items = match v {
                Value::List(xs) => xs,
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
                res?;
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
            stdlib::dispatch_resolved(&resolved, *line, ctx)?;
            Ok(())
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
