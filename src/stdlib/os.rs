/// Returns the canonical lowercase OS name used internally for `if linux:`,
/// `if macos:`, etc. Set once into `Ctx::current_os` at startup.
pub fn detect_os_name() -> String {
    if cfg!(target_os = "linux") { "linux".into() }
    else if cfg!(target_os = "macos") { "macos".into() }
    else if cfg!(target_os = "windows") { "windows".into() }
    else if cfg!(any(target_os = "freebsd", target_os = "openbsd", target_os = "netbsd", target_os = "dragonfly")) { "bsd".into() }
    else { "unknown".into() }
}
