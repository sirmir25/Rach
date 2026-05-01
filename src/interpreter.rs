use std::collections::HashSet;

use crate::ast::{BashAction, Program, Stmt};
use crate::stdlib;
use crate::stdlib::webdriver::Session;

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
    /// Active WebDriver session, lazily started on the first browser command.
    pub wd: Option<Session>,
    /// Last WebDriver startup failure (so subsequent commands can surface it).
    pub wd_unavailable: Option<String>,
    /// True when RACH_HEADLESS=1 is set; passed to WebDriver capabilities.
    pub headless: bool,
}

impl Ctx {
    pub fn os_matches(&self, want: &str) -> bool {
        let w = want.to_ascii_lowercase();
        self.current_os == w || (w == "macos" && self.current_os == "darwin")
    }
}

pub fn run(program: &Program) -> Result<(), RuntimeError> {
    let known_modules: HashSet<&str> = [
        "web", "browser", "system", "os",
        "linux", "windows", "macos",
        "bash", "ai",
    ].into_iter().collect();

    for imp in &program.imports {
        if !known_modules.contains(imp.as_str()) {
            eprintln!("warn: unknown module `{}` (continuing)", imp);
        }
    }

    let imports: HashSet<String> = program.imports.iter().cloned().collect();
    let headless = std::env::var("RACH_HEADLESS").map(|v| v == "1" || v.eq_ignore_ascii_case("true")).unwrap_or(false);
    let mut ctx = Ctx {
        imports,
        current_os: stdlib::os::detect_os_name(),
        wd: None,
        wd_unavailable: None,
        headless,
    };

    let main = program.functions.iter().find(|f| f.name == "main")
        .ok_or_else(|| RuntimeError::new(404, 0, "no `main` function defined"))?;

    let result = run_block(&main.body, &mut ctx);
    // Drop WebDriver session deterministically before returning so the
    // browser closes even if the script errored out.
    ctx.wd.take();
    result
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
        Stmt::Error { code, line_ref, .. } => {
            eprintln!("error {} string {}", code, line_ref);
            Ok(())
        }
        Stmt::IfOs { os, body, line } => {
            if ctx.os_matches(os) {
                run_block(body, ctx)?;
            } else {
                println!("// skipped if {} block (current os: {}) [line {}]", os, ctx.current_os, line);
            }
            Ok(())
        }
        Stmt::BashDsl { action, argument, line } => {
            stdlib::bash::run_bash_dsl(action, argument, *line)
        }
        Stmt::AiGenerate { language, task, line } => {
            stdlib::ai::ai_generate(language, task, *line);
            Ok(())
        }
        Stmt::Call { segments, line } => {
            stdlib::dispatch_segments(segments, *line, ctx)
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
