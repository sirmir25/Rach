use crate::ast::Value;
use crate::interpreter::{Ctx, RuntimeError};

pub fn detect_os_name() -> String {
    if cfg!(target_os = "linux") { "linux".into() }
    else if cfg!(target_os = "macos") { "macos".into() }
    else if cfg!(target_os = "windows") { "windows".into() }
    else if cfg!(any(target_os = "freebsd", target_os = "openbsd", target_os = "netbsd", target_os = "dragonfly")) { "bsd".into() }
    else { "unknown".into() }
}

pub fn detect_os(_line: usize, ctx: &mut Ctx) -> Result<Value, RuntimeError> {
    ctx.current_os = detect_os_name();
    if !ctx.capturing {
        println!("os: {}", ctx.current_os);
        println!("completed");
    }
    Ok(Value::Str(ctx.current_os.clone()))
}
