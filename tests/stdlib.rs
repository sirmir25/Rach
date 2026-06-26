//! Stdlib dispatch regression tests.

use rach::ast::Value;
use rach::interpreter::make_ctx;
use rach::stdlib;
use std::collections::BTreeMap;

/// `log` must resolve to math's natural logarithm, not the logger. These shared a dispatch
/// key, leaving the logger's `log` unreachable; `log_message` is now the logger entry point.
#[test]
fn log_is_natural_log_and_logger_is_reachable() {
    let mut ctx = make_ctx(false, String::new(), "<test>".into());
    let kwargs: BTreeMap<String, Vec<Value>> = BTreeMap::new();

    let out = stdlib::dispatch("log", &[Value::Float(std::f64::consts::E)], &kwargs, 0, &mut ctx)
        .expect("log(e) must dispatch to math::log");
    match out {
        Value::Float(f) => assert!((f - 1.0).abs() < 1e-9, "ln(e) should be 1, got {f}"),
        other => panic!("expected Float from log(e), got {other:?}"),
    }

    // The logger is now reachable under `log_message` (no longer shadowed by math's `log`).
    stdlib::dispatch(
        "log_message",
        &[Value::Str("info".into()), Value::Str("hello".into())],
        &kwargs,
        0,
        &mut ctx,
    )
    .expect("log_message must dispatch to the logger");
}
