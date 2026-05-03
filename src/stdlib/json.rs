//! JSON parse / stringify via serde_json (already a dependency).

use std::collections::BTreeMap;

use serde_json::Value as JsonValue;

use crate::ast::Value;
use crate::interpreter::{Ctx, RuntimeError};

fn first(args: &[Value], line: usize, what: &str) -> Result<String, RuntimeError> {
    args.first()
        .map(|v| v.as_str())
        .ok_or_else(|| RuntimeError::new(400, line, format!("{} requires text argument", what)))
}

pub fn json_parse(args: &[Value], line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let raw = first(args, line, "json_parse")?;
    let parsed: JsonValue = serde_json::from_str(&raw)
        .map_err(|e| RuntimeError::new(400, line, format!("json_parse: {}", e)))?;
    let value = json_to_value(&parsed);
    if !ctx.capturing {
        println!("json_parse: {}", value.as_str());
        println!("completed");
    }
    Ok(value)
}

pub fn json_stringify(args: &[Value], line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let v = args.first()
        .ok_or_else(|| RuntimeError::new(400, line, "json_stringify requires a value"))?;
    let pretty = args.get(1).map(|v| v.is_truthy()).unwrap_or(false);
    let json = value_to_json(v);
    let s = if pretty {
        serde_json::to_string_pretty(&json)
            .map_err(|e| RuntimeError::new(500, line, format!("json_stringify: {}", e)))?
    } else {
        serde_json::to_string(&json)
            .map_err(|e| RuntimeError::new(500, line, format!("json_stringify: {}", e)))?
    };
    if !ctx.capturing {
        println!("{}", s);
        println!("completed");
    }
    Ok(Value::Str(s))
}

fn json_to_value(j: &JsonValue) -> Value {
    match j {
        JsonValue::Null => Value::Nil,
        JsonValue::Bool(b) => Value::Bool(*b),
        JsonValue::Number(n) => {
            if let Some(i) = n.as_i64() { Value::Int(i) }
            else { Value::Float(n.as_f64().unwrap_or(0.0)) }
        }
        JsonValue::String(s) => Value::Str(s.clone()),
        JsonValue::Array(items) => Value::List(items.iter().map(json_to_value).collect()),
        JsonValue::Object(obj) => {
            let mut map = BTreeMap::new();
            for (k, v) in obj.iter() {
                map.insert(k.clone(), json_to_value(v));
            }
            Value::Map(map)
        }
    }
}

fn value_to_json(v: &Value) -> JsonValue {
    match v {
        Value::Nil => JsonValue::Null,
        Value::Bool(b) => JsonValue::Bool(*b),
        Value::Int(n) => JsonValue::from(*n),
        Value::Float(f) => JsonValue::from(*f),
        Value::Str(s) => JsonValue::String(s.clone()),
        Value::List(items) => JsonValue::Array(items.iter().map(value_to_json).collect()),
        Value::Map(m) => {
            let mut obj = serde_json::Map::new();
            for (k, val) in m {
                obj.insert(k.clone(), value_to_json(val));
            }
            JsonValue::Object(obj)
        }
    }
}
