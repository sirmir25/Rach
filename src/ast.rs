use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub struct Program {
    pub imports: Vec<String>,
    pub functions: Vec<Function>,
}

#[derive(Debug, Clone)]
pub struct Function {
    pub name: String,
    pub params: Vec<String>,
    pub body: Vec<Stmt>,
    pub line: usize,
}

#[derive(Debug, Clone)]
pub enum Value {
    Str(String),
    Int(i64),
    Bool(bool),
    List(Vec<Value>),
    Nil,
}

impl Value {
    pub fn as_str(&self) -> String {
        match self {
            Value::Str(s) => s.clone(),
            Value::Int(i) => i.to_string(),
            Value::Bool(b) => b.to_string(),
            Value::Nil => String::new(),
            Value::List(items) => {
                let parts: Vec<String> = items.iter().map(|v| v.as_str()).collect();
                parts.join(", ")
            }
        }
    }

    pub fn is_truthy(&self) -> bool {
        match self {
            Value::Bool(b) => *b,
            Value::Nil => false,
            Value::Int(n) => *n != 0,
            Value::Str(s) => !s.is_empty(),
            Value::List(items) => !items.is_empty(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum Expr {
    Lit(Value),
    Var(String),
    List(Vec<Expr>),
    /// A command call as a value-producing expression: `read_file("/tmp/x")`
    Call {
        segments: Vec<CallSegment>,
        line: usize,
    },
    /// User function call: `my_fn(x, y)`
    FnCall {
        name: String,
        args: Vec<Expr>,
        line: usize,
    },
}

#[derive(Debug, Clone)]
pub struct CallSegment {
    pub words: Vec<String>,
    pub positional: Vec<Expr>,
    pub named: BTreeMap<String, Expr>,
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
    /// `set NAME = <expr>` — variable assignment / capture command output
    Set {
        name: String,
        expr: Expr,
        line: usize,
    },
    /// Old form: `if linux:` / `if windows:` / `if macos:` / new: `if not linux:` /
    /// optionally followed by `else:`
    IfOs {
        os: String,
        negate: bool,
        body: Vec<Stmt>,
        else_body: Option<Vec<Stmt>>,
        line: usize,
    },
    /// `for x in <expr>:`
    For {
        var: String,
        iter: Expr,
        body: Vec<Stmt>,
        line: usize,
    },
    /// `bash = generate ...` etc.
    BashDsl {
        action: BashAction,
        argument: String,
        line: usize,
    },
    /// `completed`
    Completed { line: usize },
    /// `error 409 string 12`
    Error { code: i64, line_ref: i64, line: usize },
    /// `ai_generate(language="bash", task="...")`
    AiGenerate {
        language: String,
        task: String,
        line: usize,
    },
    /// `return <expr>` inside a user function
    Return { expr: Option<Expr>, line: usize },
    /// Bare expression statement: a user-function call producing side effects.
    ExprStmt { expr: Expr, line: usize },
}

#[derive(Debug, Clone)]
pub enum BashAction {
    Generate,
    Search,
    WebSearch,
    CompleteOrError,
}
