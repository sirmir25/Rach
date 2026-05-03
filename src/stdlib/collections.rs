//! String, list, and map operations.

use std::collections::BTreeMap;

use crate::ast::Value;
use crate::interpreter::{Ctx, RuntimeError};

fn first<'a>(args: &'a [Value], line: usize, what: &str) -> Result<&'a Value, RuntimeError> {
    args.first().ok_or_else(|| RuntimeError::new(400, line, format!("{} requires an argument", what)))
}

fn nth<'a>(args: &'a [Value], n: usize, line: usize, what: &str) -> Result<&'a Value, RuntimeError> {
    args.get(n).ok_or_else(|| RuntimeError::new(400, line, format!("{} requires arg #{}", what, n + 1)))
}

fn emit_value(ctx: &Ctx, label: &str, value: &Value) {
    if !ctx.capturing {
        println!("{}: {}", label, value.as_str());
        println!("completed");
    }
}

pub fn len(args: &[Value], line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let v = first(args, line, "len")?;
    let n = match v {
        Value::Str(s) => s.chars().count() as i64,
        Value::List(items) => items.len() as i64,
        Value::Map(m) => m.len() as i64,
        Value::Nil => 0,
        other => return Err(RuntimeError::new(400, line, format!("len: not measurable: {:?}", other))),
    };
    let result = Value::Int(n);
    emit_value(ctx, "len", &result);
    Ok(result)
}

pub fn split(args: &[Value], line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let s = first(args, line, "split")?.as_str();
    let sep = args.get(1).map(|v| v.as_str()).unwrap_or_else(|| " ".to_string());
    let parts: Vec<Value> = if sep.is_empty() {
        s.chars().map(|c| Value::Str(c.to_string())).collect()
    } else {
        s.split(&sep).map(|p| Value::Str(p.to_string())).collect()
    };
    let result = Value::List(parts);
    emit_value(ctx, "split", &result);
    Ok(result)
}

pub fn join(args: &[Value], line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let list = first(args, line, "join")?;
    let sep = args.get(1).map(|v| v.as_str()).unwrap_or_default();
    let items = match list {
        Value::List(xs) => xs,
        other => return Err(RuntimeError::new(400, line, format!("join: first arg must be a list, got {:?}", other))),
    };
    let joined = items.iter().map(|v| v.as_str()).collect::<Vec<_>>().join(&sep);
    let result = Value::Str(joined);
    emit_value(ctx, "join", &result);
    Ok(result)
}

pub fn contains(args: &[Value], line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let coll = first(args, line, "contains")?;
    let needle = nth(args, 1, line, "contains")?;
    let found = match coll {
        Value::Str(s) => s.contains(&needle.as_str()),
        Value::List(items) => items.iter().any(|v| crate::interpreter::values_equal_pub(v, needle)),
        Value::Map(m) => m.contains_key(&needle.as_str()),
        other => return Err(RuntimeError::new(400, line, format!("contains: cannot search in {:?}", other))),
    };
    let result = Value::Bool(found);
    emit_value(ctx, "contains", &result);
    Ok(result)
}

pub fn slice(args: &[Value], line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let target = first(args, line, "slice")?;
    let start = nth(args, 1, line, "slice")?.as_f64()
        .ok_or_else(|| RuntimeError::new(400, line, "slice: start must be int"))? as i64;
    let end_opt = args.get(2).and_then(|v| v.as_f64()).map(|f| f as i64);

    let result = match target {
        Value::Str(s) => {
            let chars: Vec<char> = s.chars().collect();
            let n = chars.len() as i64;
            let lo = clamp_index(start, n);
            let hi = end_opt.map(|e| clamp_index(e, n)).unwrap_or(n as usize);
            Value::Str(chars[lo..hi.max(lo)].iter().collect())
        }
        Value::List(items) => {
            let n = items.len() as i64;
            let lo = clamp_index(start, n);
            let hi = end_opt.map(|e| clamp_index(e, n)).unwrap_or(n as usize);
            Value::List(items[lo..hi.max(lo)].to_vec())
        }
        other => return Err(RuntimeError::new(400, line, format!("slice: cannot slice {:?}", other))),
    };
    emit_value(ctx, "slice", &result);
    Ok(result)
}

fn clamp_index(i: i64, n: i64) -> usize {
    let adjusted = if i < 0 { i + n } else { i };
    adjusted.clamp(0, n) as usize
}

pub fn append(args: &[Value], line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let list = first(args, line, "append")?;
    let items = match list {
        Value::List(xs) => xs.clone(),
        other => return Err(RuntimeError::new(400, line, format!("append: first arg must be a list, got {:?}", other))),
    };
    let mut new_list = items;
    for v in &args[1..] {
        new_list.push(v.clone());
    }
    let result = Value::List(new_list);
    emit_value(ctx, "append", &result);
    Ok(result)
}

pub fn pop(args: &[Value], line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let list = first(args, line, "pop")?;
    let items = match list {
        Value::List(xs) => xs,
        other => return Err(RuntimeError::new(400, line, format!("pop: not a list: {:?}", other))),
    };
    if items.is_empty() {
        return Err(RuntimeError::new(400, line, "pop: empty list"));
    }
    let result = items.last().cloned().unwrap();
    emit_value(ctx, "pop", &result);
    Ok(result)
}

pub fn sorted(args: &[Value], line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let list = first(args, line, "sorted")?;
    let items = match list {
        Value::List(xs) => xs.clone(),
        other => return Err(RuntimeError::new(400, line, format!("sorted: not a list: {:?}", other))),
    };
    let mut nums: Vec<(f64, Value)> = items.into_iter()
        .map(|v| (v.as_f64().unwrap_or(0.0), v))
        .collect();
    nums.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let result = Value::List(nums.into_iter().map(|(_, v)| v).collect());
    emit_value(ctx, "sorted", &result);
    Ok(result)
}

pub fn reverse(args: &[Value], line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let target = first(args, line, "reverse")?;
    let result = match target {
        Value::Str(s) => Value::Str(s.chars().rev().collect()),
        Value::List(xs) => {
            let mut copy = xs.clone();
            copy.reverse();
            Value::List(copy)
        }
        other => return Err(RuntimeError::new(400, line, format!("reverse: cannot reverse {:?}", other))),
    };
    emit_value(ctx, "reverse", &result);
    Ok(result)
}

pub fn upper(args: &[Value], line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let result = Value::Str(first(args, line, "upper")?.as_str().to_uppercase());
    emit_value(ctx, "upper", &result);
    Ok(result)
}

pub fn lower(args: &[Value], line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let result = Value::Str(first(args, line, "lower")?.as_str().to_lowercase());
    emit_value(ctx, "lower", &result);
    Ok(result)
}

pub fn trim(args: &[Value], line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let result = Value::Str(first(args, line, "trim")?.as_str().trim().to_string());
    emit_value(ctx, "trim", &result);
    Ok(result)
}

pub fn replace(args: &[Value], line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let s = first(args, line, "replace")?.as_str();
    let from = nth(args, 1, line, "replace")?.as_str();
    let to = nth(args, 2, line, "replace")?.as_str();
    let result = Value::Str(s.replace(&from, &to));
    emit_value(ctx, "replace", &result);
    Ok(result)
}

pub fn map_keys(args: &[Value], line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let m = first(args, line, "map_keys")?;
    let map = match m {
        Value::Map(m) => m,
        other => return Err(RuntimeError::new(400, line, format!("map_keys: not a map: {:?}", other))),
    };
    let result = Value::List(map.keys().map(|k| Value::Str(k.clone())).collect());
    emit_value(ctx, "map_keys", &result);
    Ok(result)
}

pub fn map_values(args: &[Value], line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let m = first(args, line, "map_values")?;
    let map = match m {
        Value::Map(m) => m,
        other => return Err(RuntimeError::new(400, line, format!("map_values: not a map: {:?}", other))),
    };
    let result = Value::List(map.values().cloned().collect());
    emit_value(ctx, "map_values", &result);
    Ok(result)
}

pub fn map_set(args: &[Value], line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let m = first(args, line, "map_set")?;
    let key = nth(args, 1, line, "map_set")?.as_str();
    let value = nth(args, 2, line, "map_set")?.clone();
    let mut map: BTreeMap<String, Value> = match m {
        Value::Map(m) => m.clone(),
        Value::Nil => BTreeMap::new(),
        other => return Err(RuntimeError::new(400, line, format!("map_set: not a map: {:?}", other))),
    };
    map.insert(key, value);
    let result = Value::Map(map);
    emit_value(ctx, "map_set", &result);
    Ok(result)
}

pub fn dict(_args: &[Value], _line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    // Build an empty map. Args ignored — for paired construction, use map literal `{...}` or json_parse.
    let result = Value::Map(BTreeMap::new());
    emit_value(ctx, "dict", &result);
    Ok(result)
}
