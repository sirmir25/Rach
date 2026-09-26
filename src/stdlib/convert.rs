//! Type conversions and inspection: int, float, str, bool, type_of.

use crate::ast::Value;
use crate::interpreter::{Ctx, RuntimeError};
use crate::stdlib::args::Kwargs;

fn arg<'a>(args: &'a [Value], line: usize, what: &str) -> Result<&'a Value, RuntimeError> {
    args.first().ok_or_else(|| RuntimeError::new(400, line, format!("{what} requires an argument")))
}

fn emit(ctx: &Ctx, what: &str, v: Value) -> Value {
    if !ctx.capturing {
        println!("{what}: {}", v.as_str());
        println!("completed");
    }
    v
}

/// Truncates toward zero, like C casts and Python's `int()`. Strings may hold an integer or a
/// decimal ("42", "-3.9", " 7 "); anything else — or a value outside i64 — is an error rather
/// than a silently saturated number.
pub fn to_int(v: &Value) -> Result<i64, String> {
    let from_f64 = |f: f64| {
        if f.is_finite() && f.trunc() >= i64::MIN as f64 && f.trunc() < i64::MAX as f64 {
            Ok(f.trunc() as i64)
        } else {
            Err(format!("{f:e} does not fit in a 64-bit integer"))
        }
    };
    match v {
        Value::Int(n) => Ok(*n),
        Value::Float(f) => from_f64(*f),
        Value::Bool(b) => Ok(i64::from(*b)),
        Value::Str(s) => {
            let t = s.trim();
            t.parse::<i64>().or_else(|_| t.parse::<f64>().map_err(|_| format!("\"{s}\" is not a number")).and_then(from_f64))
        }
        other => Err(format!("cannot convert {} to int", type_name(other))),
    }
}

pub fn type_name(v: &Value) -> String {
    match v {
        Value::Int(_) => "int".into(),
        Value::Float(_) => "float".into(),
        Value::Str(_) => "str".into(),
        Value::Bool(_) => "bool".into(),
        Value::List(_) => "list".into(),
        Value::Map(_) => "map".into(),
        Value::Struct { name, .. } => name.clone(),
        Value::Lambda { .. } => "fn".into(),
        Value::Nil => "nil".into(),
    }
}

pub fn int(args: &[Value], _kwargs: &Kwargs, line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let n = to_int(arg(args, line, "int")?).map_err(|e| RuntimeError::new(400, line, format!("int: {e}")))?;
    Ok(emit(ctx, "int", Value::Int(n)))
}

pub fn float(args: &[Value], _kwargs: &Kwargs, line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let v = arg(args, line, "float")?;
    let f = match v {
        Value::Int(_) | Value::Float(_) | Value::Bool(_) | Value::Str(_) => v.as_f64(),
        _ => None,
    }.ok_or_else(|| RuntimeError::new(400, line, format!("float: cannot convert {} `{}` to float", type_name(v), v.as_str())))?;
    Ok(emit(ctx, "float", Value::Float(f)))
}

pub fn str(args: &[Value], _kwargs: &Kwargs, line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    Ok(emit(ctx, "str", Value::Str(arg(args, line, "str")?.as_str())))
}

pub fn bool(args: &[Value], _kwargs: &Kwargs, line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    Ok(emit(ctx, "bool", Value::Bool(arg(args, line, "bool")?.is_truthy())))
}

pub fn type_of(args: &[Value], _kwargs: &Kwargs, line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    Ok(emit(ctx, "type_of", Value::Str(type_name(arg(args, line, "type_of")?))))
}
