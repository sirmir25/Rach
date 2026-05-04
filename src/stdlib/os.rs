/// Returns the canonical lowercase OS name used internally for `if linux:`,
/// `if macos:`, etc. Set once into `Ctx::current_os` at startup.
pub fn detect_os_name() -> String {
    if cfg!(target_os = "linux") { "linux".into() }
    else if cfg!(target_os = "macos") { "macos".into() }
    else if cfg!(target_os = "windows") { "windows".into() }
    else if cfg!(any(target_os = "freebsd", target_os = "openbsd", target_os = "netbsd", target_os = "dragonfly")) { "bsd".into() }
    else { "unknown".into() }
}

use crate::ast::Value;
use crate::interpreter::{Ctx, RuntimeError};

pub fn listdir(args: &[Value], line: usize, _ctx: &Ctx) -> Result<Value, RuntimeError> {
    let path = args.first().map(|v| v.as_str()).unwrap_or_else(|| ".".into());
    let entries = std::fs::read_dir(path.as_str())
        .map_err(|e| RuntimeError::new(500, line, format!("listdir: {}", e)))?;
    let mut names = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| RuntimeError::new(500, line, format!("listdir: {}", e)))?;
        names.push(Value::Str(entry.file_name().to_string_lossy().to_string()));
    }
    names.sort_by(|a, b| a.as_str().cmp(&b.as_str()));
    Ok(Value::List(names))
}

pub fn mkdir(args: &[Value], line: usize, _ctx: &Ctx) -> Result<Value, RuntimeError> {
    let path = args.first().map(|v| v.as_str()).unwrap_or_default();
    std::fs::create_dir_all(path.as_str())
        .map_err(|e| RuntimeError::new(500, line, format!("mkdir: {}", e)))?;
    Ok(Value::Nil)
}

pub fn isfile(args: &[Value], line: usize, _ctx: &Ctx) -> Result<Value, RuntimeError> {
    let path = args.first().ok_or_else(|| RuntimeError::new(400, line, "isfile: requires a path"))?.as_str();
    Ok(Value::Bool(std::path::Path::new(path.as_str()).is_file()))
}

pub fn isdir(args: &[Value], line: usize, _ctx: &Ctx) -> Result<Value, RuntimeError> {
    let path = args.first().ok_or_else(|| RuntimeError::new(400, line, "isdir: requires a path"))?.as_str();
    Ok(Value::Bool(std::path::Path::new(path.as_str()).is_dir()))
}

pub fn path_join(args: &[Value], line: usize, _ctx: &Ctx) -> Result<Value, RuntimeError> {
    if args.is_empty() { return Err(RuntimeError::new(400, line, "path_join: requires at least one argument")); }
    let mut p = std::path::PathBuf::from(args[0].as_str());
    for part in &args[1..] {
        p.push(part.as_str());
    }
    Ok(Value::Str(p.to_string_lossy().to_string()))
}

pub fn path_basename(args: &[Value], line: usize, _ctx: &Ctx) -> Result<Value, RuntimeError> {
    let path = args.first().ok_or_else(|| RuntimeError::new(400, line, "path_basename: requires a path"))?.as_str();
    let p = std::path::Path::new(path.as_str());
    Ok(Value::Str(p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()))
}

pub fn path_dirname(args: &[Value], line: usize, _ctx: &Ctx) -> Result<Value, RuntimeError> {
    let path = args.first().ok_or_else(|| RuntimeError::new(400, line, "path_dirname: requires a path"))?.as_str();
    let p = std::path::Path::new(path.as_str());
    Ok(Value::Str(p.parent().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| ".".into())))
}

pub fn path_ext(args: &[Value], line: usize, _ctx: &Ctx) -> Result<Value, RuntimeError> {
    let path = args.first().ok_or_else(|| RuntimeError::new(400, line, "path_ext: requires a path"))?.as_str();
    let p = std::path::Path::new(path.as_str());
    Ok(Value::Str(p.extension().map(|e| e.to_string_lossy().to_string()).unwrap_or_default()))
}

pub fn env_get(args: &[Value], line: usize, _ctx: &Ctx) -> Result<Value, RuntimeError> {
    let key = args.first().ok_or_else(|| RuntimeError::new(400, line, "env_get: requires key"))?.as_str();
    Ok(match std::env::var(key.as_str()) {
        Ok(v) => Value::Str(v),
        Err(_) => args.get(1).cloned().unwrap_or(Value::Nil),
    })
}

pub fn env_set(args: &[Value], line: usize, _ctx: &Ctx) -> Result<Value, RuntimeError> {
    let key = args.first().ok_or_else(|| RuntimeError::new(400, line, "env_set: requires key"))?.as_str();
    let val = args.get(1).map(|v| v.as_str()).unwrap_or_default();
    std::env::set_var(key.as_str(), val.as_str());
    Ok(Value::Nil)
}
