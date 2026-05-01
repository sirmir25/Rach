use crate::interpreter::{Ctx, RuntimeError};

/// Returns the canonical lowercase os name: linux | macos | windows | bsd | unknown
pub fn detect_os_name() -> String {
    if cfg!(target_os = "linux") { "linux".into() }
    else if cfg!(target_os = "macos") { "macos".into() }
    else if cfg!(target_os = "windows") { "windows".into() }
    else if cfg!(any(target_os = "freebsd", target_os = "openbsd", target_os = "netbsd", target_os = "dragonfly")) { "bsd".into() }
    else { "unknown".into() }
}

pub fn detect_os(_line: usize, ctx: &mut Ctx) -> Result<(), RuntimeError> {
    ctx.current_os = detect_os_name();
    println!("os: {}", ctx.current_os);
    println!("completed");
    Ok(())
}
