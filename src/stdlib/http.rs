//! HTTP client via curl. Returns a map { status, body, ok }.

use std::collections::BTreeMap;
use std::process::Command;

use crate::ast::Value;
use crate::interpreter::{Ctx, RuntimeError};

const HTTP_OK_LOW: i64 = 200;
const HTTP_OK_HIGH: i64 = 299;

fn first_str(args: &[Value], line: usize, what: &str) -> Result<String, RuntimeError> {
    args.first()
        .map(|v| v.as_str())
        .ok_or_else(|| RuntimeError::new(400, line, format!("{} requires url", what)))
}

fn run_curl(args: &[&str], line: usize) -> Result<(i64, String), RuntimeError> {
    let output = Command::new("curl")
        .args(args)
        .arg("-sS")
        .arg("--max-time").arg("60")
        .arg("-w").arg("\n%{http_code}")
        .output()
        .map_err(|e| RuntimeError::new(502, line, format!("spawn curl: {}", e)))?;
    if !output.status.success() {
        return Err(RuntimeError::new(502, line,
            format!("curl exit {:?}: {}", output.status.code(), String::from_utf8_lossy(&output.stderr))));
    }
    let combined = String::from_utf8_lossy(&output.stdout).into_owned();
    let split_at = combined.rfind('\n').unwrap_or(combined.len());
    let body = combined[..split_at].to_string();
    let status_str = combined[split_at..].trim();
    let status: i64 = status_str.parse().unwrap_or(0);
    Ok((status, body))
}

fn build_response(status: i64, body: String) -> Value {
    let mut map: BTreeMap<String, Value> = BTreeMap::new();
    map.insert("status".to_string(), Value::Int(status));
    map.insert("body".to_string(), Value::Str(body));
    map.insert("ok".to_string(), Value::Bool((HTTP_OK_LOW..=HTTP_OK_HIGH).contains(&status)));
    Value::Map(map)
}

fn emit(ctx: &Ctx, label: &str, value: &Value) {
    if !ctx.capturing {
        println!("{}: {}", label, value.as_str());
        println!("completed");
    }
}

pub fn http_get(args: &[Value], line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let url = first_str(args, line, "http_get")?;
    let (status, body) = run_curl(&["-X", "GET", &url], line)?;
    let result = build_response(status, body);
    emit(ctx, "http_get", &result);
    Ok(result)
}

pub fn http_post(args: &[Value], line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let url = first_str(args, line, "http_post")?;
    let body = args.get(1).map(|v| v.as_str()).unwrap_or_default();
    let content_type = args.get(2).map(|v| v.as_str())
        .unwrap_or_else(|| "application/json".to_string());

    let (status, response_body) = run_curl(
        &["-X", "POST", &url, "-d", &body, "-H", &format!("Content-Type: {}", content_type)],
        line,
    )?;
    let result = build_response(status, response_body);
    emit(ctx, "http_post", &result);
    Ok(result)
}
