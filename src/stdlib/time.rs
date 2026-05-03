//! Time utilities: `now()`, `sleep(ms)`, `format_time(ts, fmt)`.

use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::ast::Value;
use crate::interpreter::{Ctx, RuntimeError};

const SECS_PER_MIN: u64 = 60;
const SECS_PER_HOUR: u64 = SECS_PER_MIN * 60;
const SECS_PER_DAY: u64 = SECS_PER_HOUR * 24;

pub fn now(_args: &[Value], _line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    if !ctx.capturing {
        println!("now: {}", secs);
        println!("completed");
    }
    Ok(Value::Int(secs))
}

pub fn now_ms(_args: &[Value], _line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    if !ctx.capturing {
        println!("now_ms: {}", ms);
        println!("completed");
    }
    Ok(Value::Int(ms))
}

pub fn sleep_ms(args: &[Value], line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let ms = args.first().and_then(|v| v.as_f64())
        .ok_or_else(|| RuntimeError::new(400, line, "sleep_ms requires a number"))? as u64;
    if ms > 600_000 {
        return Err(RuntimeError::new(400, line, "sleep_ms: refusing to sleep > 600000 ms"));
    }
    thread::sleep(Duration::from_millis(ms));
    if !ctx.capturing {
        println!("slept {} ms", ms);
        println!("completed");
    }
    Ok(Value::Int(ms as i64))
}

/// Format a unix timestamp (seconds) as `YYYY-MM-DD HH:MM:SS` (UTC) or with
/// a custom strftime-lite spec supporting %Y %m %d %H %M %S.
pub fn format_time(args: &[Value], line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let ts = args.first().and_then(|v| v.as_f64())
        .ok_or_else(|| RuntimeError::new(400, line, "format_time requires a unix timestamp"))? as u64;
    let fmt = args.get(1).map(|v| v.as_str()).unwrap_or_else(|| "%Y-%m-%d %H:%M:%S".to_string());

    let (y, m, d) = unix_to_ymd(ts);
    let s_of_day = ts % SECS_PER_DAY;
    let h = (s_of_day / SECS_PER_HOUR) as u32;
    let mi = ((s_of_day % SECS_PER_HOUR) / SECS_PER_MIN) as u32;
    let s = (s_of_day % SECS_PER_MIN) as u32;

    let formatted = fmt
        .replace("%Y", &format!("{:04}", y))
        .replace("%m", &format!("{:02}", m))
        .replace("%d", &format!("{:02}", d))
        .replace("%H", &format!("{:02}", h))
        .replace("%M", &format!("{:02}", mi))
        .replace("%S", &format!("{:02}", s));

    if !ctx.capturing {
        println!("format_time: {}", formatted);
        println!("completed");
    }
    Ok(Value::Str(formatted))
}

/// Converts a unix timestamp (seconds since 1970-01-01 UTC) to (year, month, day).
/// Algorithm from Howard Hinnant's date library — civil_from_days.
fn unix_to_ymd(secs: u64) -> (i64, u32, u32) {
    let days = (secs / SECS_PER_DAY) as i64;
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z.rem_euclid(146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m as u32, d as u32)
}
