use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub struct Program {
    pub imports: Vec<String>,
    pub functions: Vec<Function>,
}

#[derive(Debug, Clone)]
pub struct Function {
    pub name: String,
    pub arity: u32,
    pub body: Vec<Stmt>,
    pub line: usize,
}

#[derive(Debug, Clone)]
pub enum Value {
    Str(String),
    Int(i64),
    Bool(bool),
    Nil,
}

impl Value {
    pub fn as_str(&self) -> String {
        match self {
            Value::Str(s) => s.clone(),
            Value::Int(i) => i.to_string(),
            Value::Bool(b) => b.to_string(),
            Value::Nil => String::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CallSegment {
    pub words: Vec<String>,
    pub positional: Vec<Value>,
    pub named: BTreeMap<String, Value>,
}

#[derive(Debug, Clone)]
pub enum Stmt {
    /// A "command-style" call. Stored as raw segments because the boundary
    /// between command name and first kwarg is ambiguous at parse time:
    /// `fill form id("X") value("Y")` could be `fill_form_id` + value-kwarg, or
    /// `fill_form` + id-kwarg + value-kwarg. The dispatcher resolves it by
    /// longest-prefix match against the known-command registry.
    Call {
        segments: Vec<CallSegment>,
        line: usize,
    },
    /// `if linux:` / `if windows:` / `if macos:` — conditional block on detected OS
    IfOs {
        os: String,
        body: Vec<Stmt>,
        line: usize,
    },
    /// `bash = generate ...` / `bash = search ...` / `bash = web search site ...`
    /// `bash = complete or error`
    BashDsl {
        action: BashAction,
        argument: String,
        line: usize,
    },
    /// `completed`
    Completed { line: usize },
    /// `error 409 string 12`
    Error { code: i64, line_ref: i64, line: usize },
    /// AI generation: ai_generate(language="bash", task="...")
    AiGenerate {
        language: String,
        task: String,
        line: usize,
    },
}

#[derive(Debug, Clone)]
pub enum BashAction {
    /// Generate a bash snippet via the built-in generator
    Generate,
    /// Search for a tool / command
    Search,
    /// Web search (e.g. `web search site ohmyzsh`)
    WebSearch,
    /// Status: `complete or error`
    CompleteOrError,
}
