//! String, list, and map operations.

use std::collections::BTreeMap;

use crate::ast::Value;
use crate::interpreter::{Ctx, RuntimeError};

fn first<'a>(args: &'a [Value], line: usize, what: &str) -> Result<&'a Value, RuntimeError> {
    args.first().ok_or_else(|| RuntimeError::new(400, line, format!("{what} requires an argument")))
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
        other => return Err(RuntimeError::new(400, line, format!("len: not measurable: {}", other.describe()))),
    };
    let result = Value::Int(n);
    emit_value(ctx, "len", &result);
    Ok(result)
}

pub fn split(args: &[Value], line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let s = first(args, line, "split")?.as_str();
    let sep = args.get(1).map_or_else(|| " ".to_string(), Value::as_str);
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
    let sep = args.get(1).map(Value::as_str).unwrap_or_default();
    let items = match list {
        Value::List(xs) => xs,
        other => return Err(RuntimeError::new(400, line, format!("join: first arg must be a list, got {}", other.describe()))),
    };
    let joined = items.iter().map(Value::as_str).collect::<Vec<_>>().join(&sep);
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
        other => return Err(RuntimeError::new(400, line, format!("contains: cannot search in {}", other.describe()))),
    };
    let result = Value::Bool(found);
    emit_value(ctx, "contains", &result);
    Ok(result)
}

pub fn slice(args: &[Value], line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let target = first(args, line, "slice")?;
    let start = nth(args, 1, line, "slice")?.as_f64()
        .ok_or_else(|| RuntimeError::new(400, line, "slice: start must be int"))? as i64;
    let end_opt = args.get(2).and_then(Value::as_f64).map(|f| f as i64);

    let result = match target {
        Value::Str(s) => {
            let chars: Vec<char> = s.chars().collect();
            let (lo, hi) = slice_bounds(chars.len(), start, end_opt);
            Value::Str(chars[lo..hi].iter().collect())
        }
        Value::List(items) => {
            let (lo, hi) = slice_bounds(items.len(), start, end_opt);
            Value::List(items[lo..hi].to_vec())
        }
        other => return Err(RuntimeError::new(400, line, format!("slice: cannot slice {}", other.describe()))),
    };
    emit_value(ctx, "slice", &result);
    Ok(result)
}

/// `lo..hi` for slicing `len` items: negative indices count from the end, both ends clamp
/// into range, and a start past the end gives an empty slice rather than a panic. Shared
/// with the `.slice()` list method so the two agree.
pub(crate) fn slice_bounds(len: usize, start: i64, end: Option<i64>) -> (usize, usize) {
    let n = len as i64;
    let lo = clamp_index(start, n);
    let hi = end.map_or(len, |e| clamp_index(e, n));
    (lo, hi.max(lo))
}

fn clamp_index(i: i64, n: i64) -> usize {
    let adjusted = if i < 0 { i + n } else { i };
    adjusted.clamp(0, n) as usize
}

pub fn append(args: &[Value], line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let list = first(args, line, "append")?;
    let items = match list {
        Value::List(xs) => xs.clone(),
        other => return Err(RuntimeError::new(400, line, format!("append: first arg must be a list, got {}", other.describe()))),
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
        other => return Err(RuntimeError::new(400, line, format!("pop: not a list: {}", other.describe()))),
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
        other => return Err(RuntimeError::new(400, line, format!("sorted: not a list: {}", other.describe()))),
    };
    // Numbers by value, strings alphabetically (this used to treat every non-number as 0,
    // leaving a list of words unsorted).
    let mut items = items;
    items.sort_by(crate::interpreter::compare_values);
    let result = Value::List(items);
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
        other => return Err(RuntimeError::new(400, line, format!("reverse: cannot reverse {}", other.describe()))),
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
        other => return Err(RuntimeError::new(400, line, format!("map_keys: not a map: {}", other.describe()))),
    };
    let result = Value::List(map.keys().map(|k| Value::Str(k.clone())).collect());
    emit_value(ctx, "map_keys", &result);
    Ok(result)
}

pub fn map_values(args: &[Value], line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let m = first(args, line, "map_values")?;
    let map = match m {
        Value::Map(m) => m,
        other => return Err(RuntimeError::new(400, line, format!("map_values: not a map: {}", other.describe()))),
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
        other => return Err(RuntimeError::new(400, line, format!("map_set: not a map: {}", other.describe()))),
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

pub fn range(args: &[Value], line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let (start, end, step) = match args.len() {
        1 => {
            let n = first(args, line, "range")?.as_f64()
                .ok_or_else(|| RuntimeError::new(400, line, "range: argument must be a number"))? as i64;
            (0i64, n, 1i64)
        }
        2 | 3 => {
            let s = first(args, line, "range")?.as_f64()
                .ok_or_else(|| RuntimeError::new(400, line, "range: start must be a number"))? as i64;
            let e = nth(args, 1, line, "range")?.as_f64()
                .ok_or_else(|| RuntimeError::new(400, line, "range: end must be a number"))? as i64;
            let st = args.get(2).and_then(Value::as_f64).unwrap_or(1.0) as i64;
            (s, e, st)
        }
        _ => return Err(RuntimeError::new(400, line, "range: requires 1, 2, or 3 arguments")),
    };
    if step == 0 { return Err(RuntimeError::new(400, line, "range: step cannot be zero")); }
    // Count in i128 first: stepping an i64 up to `end` could overflow near i64::MAX, and an
    // allocation too big to satisfy must be a Rach error, not a process abort.
    let span = if step > 0 { i128::from(end) - i128::from(start) } else { i128::from(start) - i128::from(end) };
    let stride = i128::from(step.unsigned_abs());
    let count = if span > 0 { (span + stride - 1) / stride } else { 0 };
    let mut items: Vec<Value> = Vec::new();
    usize::try_from(count).ok()
        .and_then(|n| items.try_reserve_exact(n).ok())
        .ok_or_else(|| RuntimeError::new(400, line, format!("range: {count} numbers don't fit in memory; `for i in N:` counts without building a list")))?;
    // Every element lies in [start, end), so the i128 -> i64 cast is exact.
    items.extend((0..count).map(|k| Value::Int((i128::from(start) + k * i128::from(step)) as i64)));
    let result = Value::List(items);
    emit_value(ctx, "range", &result);
    Ok(result)
}

pub fn enumerate(args: &[Value], line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let list = first(args, line, "enumerate")?;
    let start = args.get(1).and_then(Value::as_f64).unwrap_or(0.0) as i64;
    let items = match list {
        Value::List(xs) => xs,
        other => return Err(RuntimeError::new(400, line, format!("enumerate: expected list, got {}", other.describe()))),
    };
    let result = Value::List(
        items.iter().enumerate()
            .map(|(i, v)| Value::List(vec![Value::Int(start + i as i64), v.clone()]))
            .collect()
    );
    emit_value(ctx, "enumerate", &result);
    Ok(result)
}

pub fn keys(args: &[Value], line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let m = first(args, line, "keys")?;
    let result = match m {
        Value::Map(map) => Value::List(map.keys().map(|k| Value::Str(k.clone())).collect()),
        Value::Struct { fields, .. } => Value::List(fields.keys().map(|k| Value::Str(k.clone())).collect()),
        other => return Err(RuntimeError::new(400, line, format!("keys: expected map, got {}", other.describe()))),
    };
    emit_value(ctx, "keys", &result);
    Ok(result)
}

pub fn values(args: &[Value], line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let m = first(args, line, "values")?;
    let result = match m {
        Value::Map(map) => Value::List(map.values().cloned().collect()),
        Value::Struct { fields, .. } => Value::List(fields.values().cloned().collect()),
        other => return Err(RuntimeError::new(400, line, format!("values: expected map, got {}", other.describe()))),
    };
    emit_value(ctx, "values", &result);
    Ok(result)
}

pub fn zip(args: &[Value], line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    if args.len() < 2 {
        return Err(RuntimeError::new(400, line, "zip: requires at least 2 lists"));
    }
    let lists: Vec<&Vec<Value>> = args.iter().map(|a| match a {
        Value::List(xs) => Ok(xs),
        other => Err(RuntimeError::new(400, line, format!("zip: expected list, got {}", other.describe()))),
    }).collect::<Result<_, _>>()?;
    let min_len = lists.iter().map(|l| l.len()).min().unwrap_or(0);
    let result = Value::List(
        (0..min_len).map(|i| Value::List(lists.iter().map(|l| l[i].clone()).collect())).collect()
    );
    emit_value(ctx, "zip", &result);
    Ok(result)
}

pub fn flatten(args: &[Value], line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let list = first(args, line, "flatten")?;
    let items = match list {
        Value::List(xs) => xs,
        other => return Err(RuntimeError::new(400, line, format!("flatten: expected list, got {}", other.describe()))),
    };
    let result = Value::List(
        items.iter().flat_map(|v| match v {
            Value::List(inner) => inner.clone(),
            other => vec![other.clone()],
        }).collect()
    );
    emit_value(ctx, "flatten", &result);
    Ok(result)
}
