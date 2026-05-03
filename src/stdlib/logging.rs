//! Structured logging for Rach scripts.
//!
//! Every `log_*` call appends an entry to an in-memory ring buffer (capped at
//! 1000) and prints it with a timestamp and coloured level tag. The minimum
//! level is set via `log_level("info")`; entries below the threshold are
//! dropped. Optional file mirroring via `log_to("/path/to/file.log")`.
//!
//! Read back what's been logged with `log_history()` / `log_filter("warn")`,
//! count by level with `log_count("error")`, and reset with `log_clear()`.

use std::collections::VecDeque;
use std::fs::OpenOptions;
use std::io::{IsTerminal, Write};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::ast::Value;
use crate::interpreter::{Ctx, RuntimeError};

const RING_BUFFER_LIMIT: usize = 1000;
const SECS_PER_MIN: u64 = 60;
const SECS_PER_HOUR: u64 = SECS_PER_MIN * 60;
const SECS_PER_DAY: u64 = SECS_PER_HOUR * 24;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel { Debug = 0, Info = 1, Warn = 2, Error = 3, Off = 4 }

impl LogLevel {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "debug" | "trace" => Some(LogLevel::Debug),
            "info" => Some(LogLevel::Info),
            "warn" | "warning" => Some(LogLevel::Warn),
            "error" | "err" => Some(LogLevel::Error),
            "off" | "none" => Some(LogLevel::Off),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            LogLevel::Debug => "DEBUG",
            LogLevel::Info  => "INFO ",
            LogLevel::Warn  => "WARN ",
            LogLevel::Error => "ERROR",
            LogLevel::Off   => "OFF  ",
        }
    }

    pub fn ansi(self) -> &'static str {
        match self {
            LogLevel::Debug => "\x1b[2m",
            LogLevel::Info  => "\x1b[36m",
            LogLevel::Warn  => "\x1b[33m",
            LogLevel::Error => "\x1b[31;1m",
            LogLevel::Off   => "",
        }
    }
}

#[derive(Debug, Clone)]
pub struct LogEntry {
    pub level: LogLevel,
    pub message: String,
    pub timestamp: u64,
}

pub struct LogState {
    pub level: LogLevel,
    pub buffer: VecDeque<LogEntry>,
    pub file: Option<PathBuf>,
}

impl Default for LogState {
    fn default() -> Self {
        let level_from_env = std::env::var("RACH_LOG")
            .ok()
            .and_then(|s| LogLevel::parse(&s))
            .unwrap_or(LogLevel::Info);
        Self {
            level: level_from_env,
            buffer: VecDeque::with_capacity(RING_BUFFER_LIMIT),
            file: None,
        }
    }
}

fn now_unix_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn format_clock(unix_secs: u64) -> String {
    let s = unix_secs % SECS_PER_DAY;
    let h = s / SECS_PER_HOUR;
    let m = (s % SECS_PER_HOUR) / SECS_PER_MIN;
    let s = s % SECS_PER_MIN;
    format!("{:02}:{:02}:{:02}", h, m, s)
}

fn format_entry(entry: &LogEntry, with_color: bool) -> String {
    let clock = format_clock(entry.timestamp);
    if with_color {
        format!("{}[{} {}]\x1b[0m {}", entry.level.ansi(), entry.level.label(), clock, entry.message)
    } else {
        format!("[{} {}] {}", entry.level.label(), clock, entry.message)
    }
}

fn first_str(args: &[Value], line: usize, what: &str) -> Result<String, RuntimeError> {
    args.first()
        .map(|v| v.as_str())
        .ok_or_else(|| RuntimeError::new(400, line, format!("{} requires text argument", what)))
}

fn parse_level(s: &str, line: usize) -> Result<LogLevel, RuntimeError> {
    LogLevel::parse(s).ok_or_else(|| RuntimeError::new(400, line, format!("unknown log level `{}`", s)))
}

fn append_to_file(path: &PathBuf, line: &str) {
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "{}", line);
    }
}

fn emit(ctx: &mut Ctx, level: LogLevel, message: String) -> Value {
    if level < ctx.log.level || ctx.log.level == LogLevel::Off {
        return Value::Bool(false);
    }
    let entry = LogEntry { level, message: message.clone(), timestamp: now_unix_secs() };

    if !ctx.capturing {
        let with_color = std::io::stderr().is_terminal();
        eprintln!("{}", format_entry(&entry, with_color));
    }
    if let Some(path) = ctx.log.file.clone() {
        append_to_file(&path, &format_entry(&entry, false));
    }

    if ctx.log.buffer.len() == RING_BUFFER_LIMIT {
        ctx.log.buffer.pop_front();
    }
    ctx.log.buffer.push_back(entry);
    Value::Bool(true)
}

pub fn log(args: &[Value], line: usize, ctx: &mut Ctx) -> Result<Value, RuntimeError> {
    if args.len() < 2 {
        return Err(RuntimeError::new(400, line, "log(level, message) requires two arguments"));
    }
    let level = parse_level(&args[0].as_str(), line)?;
    let message = args[1..].iter().map(|v| v.as_str()).collect::<Vec<_>>().join(" ");
    Ok(emit(ctx, level, message))
}

pub fn log_debug(args: &[Value], _line: usize, ctx: &mut Ctx) -> Result<Value, RuntimeError> {
    let message = args.iter().map(|v| v.as_str()).collect::<Vec<_>>().join(" ");
    Ok(emit(ctx, LogLevel::Debug, message))
}
pub fn log_info(args: &[Value], _line: usize, ctx: &mut Ctx) -> Result<Value, RuntimeError> {
    let message = args.iter().map(|v| v.as_str()).collect::<Vec<_>>().join(" ");
    Ok(emit(ctx, LogLevel::Info, message))
}
pub fn log_warn(args: &[Value], _line: usize, ctx: &mut Ctx) -> Result<Value, RuntimeError> {
    let message = args.iter().map(|v| v.as_str()).collect::<Vec<_>>().join(" ");
    Ok(emit(ctx, LogLevel::Warn, message))
}
pub fn log_error(args: &[Value], _line: usize, ctx: &mut Ctx) -> Result<Value, RuntimeError> {
    let message = args.iter().map(|v| v.as_str()).collect::<Vec<_>>().join(" ");
    Ok(emit(ctx, LogLevel::Error, message))
}

pub fn log_level(args: &[Value], line: usize, ctx: &mut Ctx) -> Result<Value, RuntimeError> {
    if args.is_empty() {
        let label = ctx.log.level.label().trim().to_string();
        if !ctx.capturing {
            println!("log_level: {}", label);
            println!("completed");
        }
        return Ok(Value::Str(label));
    }
    let level = parse_level(&first_str(args, line, "log_level")?, line)?;
    ctx.log.level = level;
    if !ctx.capturing {
        println!("log_level set to {}", level.label().trim());
        println!("completed");
    }
    Ok(Value::Str(level.label().trim().to_string()))
}

pub fn log_to(args: &[Value], line: usize, ctx: &mut Ctx) -> Result<Value, RuntimeError> {
    if args.is_empty() {
        ctx.log.file = None;
        if !ctx.capturing { println!("log file disabled"); println!("completed"); }
        return Ok(Value::Bool(false));
    }
    let path = first_str(args, line, "log_to")?;
    ctx.log.file = Some(PathBuf::from(&path));
    if !ctx.capturing {
        println!("log file: {}", path);
        println!("completed");
    }
    Ok(Value::Str(path))
}

pub fn log_history(_args: &[Value], _line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let entries: Vec<Value> = ctx.log.buffer.iter()
        .map(|e| Value::Str(format_entry(e, false)))
        .collect();
    if !ctx.capturing {
        for entry in &ctx.log.buffer {
            println!("{}", format_entry(entry, false));
        }
        println!("completed");
    }
    Ok(Value::List(entries))
}

pub fn log_filter(args: &[Value], line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let level = parse_level(&first_str(args, line, "log_filter")?, line)?;
    let entries: Vec<Value> = ctx.log.buffer.iter()
        .filter(|e| e.level >= level)
        .map(|e| Value::Str(format_entry(e, false)))
        .collect();
    if !ctx.capturing {
        for entry in ctx.log.buffer.iter().filter(|e| e.level >= level) {
            println!("{}", format_entry(entry, false));
        }
        println!("completed");
    }
    Ok(Value::List(entries))
}

pub fn log_count(args: &[Value], line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let count = if args.is_empty() {
        ctx.log.buffer.len() as i64
    } else {
        let level = parse_level(&first_str(args, line, "log_count")?, line)?;
        ctx.log.buffer.iter().filter(|e| e.level == level).count() as i64
    };
    if !ctx.capturing {
        println!("log_count: {}", count);
        println!("completed");
    }
    Ok(Value::Int(count))
}

pub fn log_clear(_args: &[Value], _line: usize, ctx: &mut Ctx) -> Result<Value, RuntimeError> {
    let cleared = ctx.log.buffer.len() as i64;
    ctx.log.buffer.clear();
    if !ctx.capturing {
        println!("log_clear: removed {} entries", cleared);
        println!("completed");
    }
    Ok(Value::Int(cleared))
}
