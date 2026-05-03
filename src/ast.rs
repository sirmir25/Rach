use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub struct Program {
    pub imports: Vec<String>,
    pub functions: Vec<Function>,
    pub structs: Vec<StructDef>,
}

#[derive(Debug, Clone)]
pub struct Function {
    pub name: String,
    pub params: Vec<String>,
    /// Parallel to `params`. `None` = required, `Some(expr)` = C++-style default value.
    pub defaults: Vec<Option<Expr>>,
    pub body: Vec<Stmt>,
    pub line: usize,
}

#[derive(Debug, Clone)]
pub struct StructDef {
    pub name: String,
    pub fields: Vec<String>,
    pub line: usize,
}

#[derive(Debug, Clone)]
pub enum Value {
    Str(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    List(Vec<Value>),
    Map(BTreeMap<String, Value>),
    /// `struct Point { x, y }` instance — keeps the name for display + dispatch.
    Struct {
        name: String,
        fields: BTreeMap<String, Value>,
    },
    /// First-class function — `fn(x) -> x * 2` or `fn(x): ... end`.
    /// Captures the lexical scope at definition time.
    Lambda {
        params: Vec<String>,
        body: Vec<Stmt>,
        captured: BTreeMap<String, Value>,
    },
    Nil,
}

impl Value {
    pub fn as_str(&self) -> String {
        match self {
            Value::Str(s) => s.clone(),
            Value::Int(i) => i.to_string(),
            Value::Float(f) => {
                if f.is_finite() && *f == f.trunc() && f.abs() < 1e16 {
                    format!("{:.1}", f)
                } else {
                    format!("{}", f)
                }
            }
            Value::Bool(b) => b.to_string(),
            Value::Nil => String::new(),
            Value::List(items) => {
                let parts: Vec<String> = items.iter().map(|v| v.as_str()).collect();
                parts.join(", ")
            }
            Value::Map(items) => {
                let parts: Vec<String> = items.iter()
                    .map(|(k, v)| format!("{:?}: {}", k, v.as_str()))
                    .collect();
                format!("{{{}}}", parts.join(", "))
            }
            Value::Struct { name, fields } => {
                let parts: Vec<String> = fields.iter()
                    .map(|(k, v)| format!("{}: {}", k, v.as_str()))
                    .collect();
                format!("{} {{ {} }}", name, parts.join(", "))
            }
            Value::Lambda { params, .. } => {
                format!("<lambda({})>", params.join(", "))
            }
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Int(i) => Some(*i as f64),
            Value::Float(f) => Some(*f),
            Value::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
            Value::Str(s) => s.trim().parse::<f64>().ok(),
            _ => None,
        }
    }

    pub fn is_truthy(&self) -> bool {
        match self {
            Value::Bool(b) => *b,
            Value::Nil => false,
            Value::Int(n) => *n != 0,
            Value::Float(f) => *f != 0.0 && !f.is_nan(),
            Value::Str(s) => !s.is_empty(),
            Value::List(items) => !items.is_empty(),
            Value::Map(m) => !m.is_empty(),
            Value::Struct { fields, .. } => !fields.is_empty(),
            Value::Lambda { .. } => true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BinOp {
    Add, Sub, Mul, Div, Mod, Pow,
    Eq, Ne, Lt, Gt, Le, Ge,
    And, Or,
    BitAnd, BitOr, BitXor, Shl, Shr,
}

#[derive(Debug, Clone, Copy)]
pub enum UnaryOp { Neg, Not, BitNot }

#[derive(Debug, Clone)]
pub enum Expr {
    Lit(Value),
    Var(String),
    List(Vec<Expr>),
    Call {
        segments: Vec<CallSegment>,
        line: usize,
    },
    FnCall {
        name: String,
        args: Vec<Expr>,
        line: usize,
    },
    /// Call an arbitrary expression as a function — `f(x)`, `arr[0](x)`, etc.
    /// `callee` evaluates to a `Value::Lambda` (or a known user-fn name).
    CallValue {
        callee: Box<Expr>,
        args: Vec<Expr>,
        line: usize,
    },
    Binary {
        op: BinOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
        line: usize,
    },
    Unary {
        op: UnaryOp,
        expr: Box<Expr>,
        line: usize,
    },
    /// `cond ? then : else` — ternary.
    Ternary {
        cond: Box<Expr>,
        then_expr: Box<Expr>,
        else_expr: Box<Expr>,
        line: usize,
    },
    /// `{"key": value, ...}` — map literal
    MapLit {
        entries: Vec<(Expr, Expr)>,
        line: usize,
    },
    /// `Point { x: 1, y: 2 }` — struct instantiation.
    StructLit {
        name: String,
        fields: Vec<(String, Expr)>,
        line: usize,
    },
    /// `coll[key]` — indexing into list (int) or map (string)
    Index {
        target: Box<Expr>,
        key: Box<Expr>,
        line: usize,
    },
    /// `target.name` — field access for structs/maps; sugar over Index with string key.
    Field {
        target: Box<Expr>,
        name: String,
        line: usize,
    },
    /// `fn(x, y) -> expr` (single-expr) or `fn(x, y): ... end` (block-form).
    Lambda {
        params: Vec<String>,
        body: Vec<Stmt>,
        line: usize,
    },
    /// Built at parse time from a string literal that contains `{...}`. At eval
    /// time, each Expr part is evaluated and concatenated as a string.
    InterpStr {
        parts: Vec<InterpPart>,
        line: usize,
    },
}

#[derive(Debug, Clone)]
pub enum InterpPart {
    Lit(String),
    Expr(Expr),
}

#[derive(Debug, Clone)]
pub struct CallSegment {
    pub words: Vec<String>,
    pub positional: Vec<Expr>,
    pub named: BTreeMap<String, Expr>,
}

/// Target of an assignment — `x`, `x[k]`, or `x.y`. Compound assignment desugars
/// the rhs but reuses the same place-expression on both sides.
#[derive(Debug, Clone)]
pub enum AssignTarget {
    Name(String),
    Index { target: Box<Expr>, key: Box<Expr> },
    Field { target: Box<Expr>, name: String },
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
    /// `set NAME = <expr>` / `NAME = <expr>` / `obj.field = <expr>` / `arr[i] = <expr>`.
    Assign {
        target: AssignTarget,
        expr: Expr,
        is_const: bool,
        line: usize,
    },
    /// `x += expr` etc — desugared at eval time to `x = x op expr`.
    CompoundAssign {
        target: AssignTarget,
        op: BinOp,
        expr: Expr,
        line: usize,
    },
    /// Legacy OS check: `if linux:` / `if not macos:` / optional `else:`.
    IfOs {
        os: String,
        negate: bool,
        body: Vec<Stmt>,
        else_body: Option<Vec<Stmt>>,
        line: usize,
    },
    /// General-purpose conditional: `if <expr>:` ... `else:`.
    If {
        cond: Expr,
        body: Vec<Stmt>,
        else_body: Option<Vec<Stmt>>,
        line: usize,
    },
    /// `while <expr>:` block.
    While {
        cond: Expr,
        body: Vec<Stmt>,
        line: usize,
    },
    /// `break` / `continue` inside loops.
    Break { line: usize },
    Continue { line: usize },
    /// `try: ... rescue [name]: ...` — catch a runtime error and bind it to a var.
    Try {
        body: Vec<Stmt>,
        rescue_var: Option<String>,
        rescue_body: Vec<Stmt>,
        line: usize,
    },
    /// `assert(<expr> [, "message"])` — abort if expr is falsy.
    Assert {
        cond: Expr,
        message: Option<Expr>,
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
    /// C++ `switch (expr) { case v: ... default: ... }` — Rach uses indent syntax.
    Switch {
        expr: Expr,
        cases: Vec<(Vec<Expr>, Vec<Stmt>)>,
        default: Option<Vec<Stmt>>,
        line: usize,
    },
    /// C++ `do { ... } while (cond)` — Rach: `do: ... while cond`.
    DoWhile {
        body: Vec<Stmt>,
        cond: Expr,
        line: usize,
    },
    /// C++ `for (init; cond; step)` — Rach: `for (init; cond; step):`.
    CFor {
        init: Option<Box<Stmt>>,
        cond: Option<Expr>,
        step: Option<Box<Stmt>>,
        body: Vec<Stmt>,
        line: usize,
    },
}

#[derive(Debug, Clone)]
pub enum BashAction {
    Generate,
    Search,
    WebSearch,
    CompleteOrError,
}
