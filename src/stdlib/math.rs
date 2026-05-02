//! Math functions: trig, exp/log, rounding, constants.
//!
//! All take and return floats. Trigonometric inputs are radians; for degrees
//! use `radians(x)` to convert.

use crate::ast::Value;
use crate::interpreter::{Ctx, RuntimeError};

const HALF_TURN: f64 = 180.0;

fn one_f64(args: &[Value], line: usize, what: &str) -> Result<f64, RuntimeError> {
    args.first()
        .and_then(|v| v.as_f64())
        .ok_or_else(|| RuntimeError::new(400, line, format!("{} requires a numeric argument", what)))
}

fn two_f64(args: &[Value], line: usize, what: &str) -> Result<(f64, f64), RuntimeError> {
    let a = args.first().and_then(|v| v.as_f64())
        .ok_or_else(|| RuntimeError::new(400, line, format!("{} requires arg #1 numeric", what)))?;
    let b = args.get(1).and_then(|v| v.as_f64())
        .ok_or_else(|| RuntimeError::new(400, line, format!("{} requires arg #2 numeric", what)))?;
    Ok((a, b))
}

fn emit(ctx: &Ctx, label: &str, value: f64) -> Value {
    if !ctx.capturing {
        println!("{}: {}", label, value);
        println!("completed");
    }
    Value::Float(value)
}

pub fn sin(args: &[Value], line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    Ok(emit(ctx, "sin", one_f64(args, line, "sin")?.sin()))
}
pub fn cos(args: &[Value], line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    Ok(emit(ctx, "cos", one_f64(args, line, "cos")?.cos()))
}
pub fn tan(args: &[Value], line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    Ok(emit(ctx, "tan", one_f64(args, line, "tan")?.tan()))
}
pub fn asin(args: &[Value], line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    Ok(emit(ctx, "asin", one_f64(args, line, "asin")?.asin()))
}
pub fn acos(args: &[Value], line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    Ok(emit(ctx, "acos", one_f64(args, line, "acos")?.acos()))
}
pub fn atan(args: &[Value], line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    Ok(emit(ctx, "atan", one_f64(args, line, "atan")?.atan()))
}
pub fn atan2(args: &[Value], line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let (y, x) = two_f64(args, line, "atan2")?;
    Ok(emit(ctx, "atan2", y.atan2(x)))
}

pub fn sqrt(args: &[Value], line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let v = one_f64(args, line, "sqrt")?;
    if v < 0.0 { return Err(RuntimeError::new(400, line, "sqrt of negative number")); }
    Ok(emit(ctx, "sqrt", v.sqrt()))
}
pub fn exp(args: &[Value], line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    Ok(emit(ctx, "exp", one_f64(args, line, "exp")?.exp()))
}
pub fn log(args: &[Value], line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    Ok(emit(ctx, "log", one_f64(args, line, "log")?.ln()))
}
pub fn log10(args: &[Value], line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    Ok(emit(ctx, "log10", one_f64(args, line, "log10")?.log10()))
}
pub fn log2(args: &[Value], line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    Ok(emit(ctx, "log2", one_f64(args, line, "log2")?.log2()))
}
pub fn pow(args: &[Value], line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let (b, e) = two_f64(args, line, "pow")?;
    Ok(emit(ctx, "pow", b.powf(e)))
}

pub fn abs(args: &[Value], line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let v = args.first().ok_or_else(|| RuntimeError::new(400, line, "abs requires arg"))?;
    let result = match v {
        Value::Int(n) => Value::Int(n.abs()),
        Value::Float(f) => Value::Float(f.abs()),
        other => Value::Float(other.as_f64()
            .ok_or_else(|| RuntimeError::new(400, line, format!("abs: not a number: {:?}", other)))?
            .abs()),
    };
    if !ctx.capturing {
        println!("abs: {}", result.as_str());
        println!("completed");
    }
    Ok(result)
}

pub fn floor(args: &[Value], line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let f = one_f64(args, line, "floor")?.floor();
    let v = Value::Int(f as i64);
    if !ctx.capturing { println!("floor: {}", v.as_str()); println!("completed"); }
    Ok(v)
}
pub fn ceil(args: &[Value], line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let f = one_f64(args, line, "ceil")?.ceil();
    let v = Value::Int(f as i64);
    if !ctx.capturing { println!("ceil: {}", v.as_str()); println!("completed"); }
    Ok(v)
}
pub fn round(args: &[Value], line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let f = one_f64(args, line, "round")?.round();
    let v = Value::Int(f as i64);
    if !ctx.capturing { println!("round: {}", v.as_str()); println!("completed"); }
    Ok(v)
}

pub fn min(args: &[Value], line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let nums = collect_numbers(args, line, "min")?;
    let m = nums.iter().cloned().fold(f64::INFINITY, f64::min);
    Ok(emit(ctx, "min", m))
}
pub fn max(args: &[Value], line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let nums = collect_numbers(args, line, "max")?;
    let m = nums.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    Ok(emit(ctx, "max", m))
}
pub fn sum(args: &[Value], line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let nums = collect_numbers(args, line, "sum")?;
    let total: f64 = nums.iter().sum();
    Ok(emit(ctx, "sum", total))
}
pub fn avg(args: &[Value], line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let nums = collect_numbers(args, line, "avg")?;
    if nums.is_empty() { return Err(RuntimeError::new(400, line, "avg of empty input")); }
    let total: f64 = nums.iter().sum();
    Ok(emit(ctx, "avg", total / nums.len() as f64))
}

pub fn radians(args: &[Value], line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let v = one_f64(args, line, "radians")?;
    Ok(emit(ctx, "radians", v * std::f64::consts::PI / HALF_TURN))
}
pub fn degrees(args: &[Value], line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let v = one_f64(args, line, "degrees")?;
    Ok(emit(ctx, "degrees", v * HALF_TURN / std::f64::consts::PI))
}

pub fn pi(_args: &[Value], _line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    Ok(emit(ctx, "pi", std::f64::consts::PI))
}
pub fn e_const(_args: &[Value], _line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    Ok(emit(ctx, "e", std::f64::consts::E))
}

fn collect_numbers(args: &[Value], line: usize, what: &str) -> Result<Vec<f64>, RuntimeError> {
    let mut out = Vec::new();
    for v in args {
        if let Value::List(items) = v {
            for it in items {
                out.push(it.as_f64()
                    .ok_or_else(|| RuntimeError::new(400, line, format!("{}: list item is not a number: {:?}", what, it)))?);
            }
        } else {
            out.push(v.as_f64()
                .ok_or_else(|| RuntimeError::new(400, line, format!("{}: arg is not a number: {:?}", what, v)))?);
        }
    }
    Ok(out)
}
