use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use crate::ast::Value;
use crate::interpreter::{Ctx, RuntimeError};
use crate::stdlib::args::{kw_bool, kw_str, Kwargs};

fn first_str(args: &[Value], line: usize, what: &str) -> Result<String, RuntimeError> {
    args.first()
        .map(Value::as_str)
        .ok_or_else(|| RuntimeError::new(400, line, format!("{what} requires an argument")))
}

fn nth_str(args: &[Value], n: usize, line: usize, what: &str) -> Result<String, RuntimeError> {
    args.get(n)
        .map(Value::as_str)
        .ok_or_else(|| RuntimeError::new(400, line, format!("{} requires arg #{}", what, n + 1)))
}

pub fn run_command(args: &[Value], line: usize) -> Result<Value, RuntimeError> {
    let cmd = first_str(args, line, "run_command")?;
    println!("$ {cmd}");
    let (program, shell_arg) = if cfg!(target_os = "windows") {
        ("cmd", "/C")
    } else {
        ("sh", "-c")
    };
    let output = Command::new(program).arg(shell_arg).arg(&cmd).output();
    match output {
        Ok(o) => {
            let stdout_str = String::from_utf8_lossy(&o.stdout).to_string();
            if !o.stdout.is_empty() {
                print!("{stdout_str}");
            }
            if !o.stderr.is_empty() {
                eprint!("{}", String::from_utf8_lossy(&o.stderr));
            }
            if o.status.success() {
                println!("completed");
                Ok(Value::Str(stdout_str))
            } else {
                let code = i64::from(o.status.code().unwrap_or(1));
                Err(RuntimeError::new(400 + code, line, format!("run_command exited with {code}")))
            }
        }
        Err(e) => Err(RuntimeError::new(500, line, format!("run_command failed: {e}"))),
    }
}

/// `exec(cmd, quiet=, check=, cwd=, env=, input=)` — like `run`, but hands the outcome back
/// as `{code, ok, stdout, stderr}` instead of failing on a non-zero exit, so scripts can
/// branch on it. `check=true` restores `run`'s fail-fast behaviour.
pub fn exec(args: &[Value], kwargs: &Kwargs, line: usize) -> Result<Value, RuntimeError> {
    let cmd = first_str(args, line, "exec")?;
    let quiet = kw_bool(kwargs, "quiet");
    let (program, shell_arg) = if cfg!(target_os = "windows") {
        ("cmd", "/C")
    } else {
        ("sh", "-c")
    };
    let mut command = Command::new(program);
    command.arg(shell_arg).arg(&cmd);
    if let Some(dir) = kw_str(kwargs, "cwd") {
        command.current_dir(dir);
    }
    match kwargs.get("env").and_then(|v| v.first()) {
        None => {}
        Some(Value::Map(vars)) => {
            for (k, v) in vars {
                command.env(k, v.as_str());
            }
        }
        Some(other) => {
            return Err(RuntimeError::new(400, line, format!("exec: `env` must be a map, got {}", other.describe())));
        }
    }
    let input = kw_str(kwargs, "input");
    command.stdin(if input.is_some() { Stdio::piped() } else { Stdio::inherit() });
    command.stdout(Stdio::piped()).stderr(Stdio::piped());

    if !quiet {
        println!("$ {cmd}");
    }
    let mut child = command
        .spawn()
        .map_err(|e| RuntimeError::new(500, line, format!("exec failed: {e}")))?;
    if let (Some(text), Some(mut stdin)) = (input, child.stdin.take()) {
        // A child that exits without reading stdin closes the pipe; that's its choice, not an error.
        if let Err(e) = stdin.write_all(text.as_bytes()) {
            if e.kind() != std::io::ErrorKind::BrokenPipe {
                return Err(RuntimeError::new(500, line, format!("exec: writing stdin failed: {e}")));
            }
        }
    }
    let output = child
        .wait_with_output()
        .map_err(|e| RuntimeError::new(500, line, format!("exec failed: {e}")))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    if !quiet {
        print!("{stdout}");
        eprint!("{stderr}");
    }
    // A process killed by a signal has no exit code; -1 keeps `code` an int and `ok` false.
    let code = output.status.code().map_or(-1, i64::from);
    if kw_bool(kwargs, "check") && !output.status.success() {
        return Err(RuntimeError::new(400 + code.max(1), line, format!("exec: `{cmd}` exited with {code}")));
    }

    let mut result = BTreeMap::new();
    result.insert("code".to_string(), Value::Int(code));
    result.insert("ok".to_string(), Value::Bool(output.status.success()));
    result.insert("stdout".to_string(), Value::Str(stdout));
    result.insert("stderr".to_string(), Value::Str(stderr));
    Ok(Value::Map(result))
}

pub fn install_package(args: &[Value], line: usize, ctx: &mut Ctx) -> Result<Value, RuntimeError> {
    let pkg = first_str(args, line, "install_package")?;
    let (program, install_args): (&str, Vec<String>) = match ctx.current_os.as_str() {
        "macos" => ("brew", vec!["install".into(), pkg.clone()]),
        "linux" => {
            let candidates = ["apt-get", "apt", "dnf", "yum", "pacman", "zypper", "apk"];
            let mut chosen = None;
            for c in candidates {
                if which::which_path(c).is_some() { chosen = Some(c); break; }
            }
            match chosen {
                Some("apt-get" | "apt") => ("sudo", vec!["apt-get".into(), "install".into(), "-y".into(), pkg.clone()]),
                Some("dnf") => ("sudo", vec!["dnf".into(), "install".into(), "-y".into(), pkg.clone()]),
                Some("yum") => ("sudo", vec!["yum".into(), "install".into(), "-y".into(), pkg.clone()]),
                Some("pacman") => ("sudo", vec!["pacman".into(), "-S".into(), "--noconfirm".into(), pkg.clone()]),
                Some("zypper") => ("sudo", vec!["zypper".into(), "install".into(), "-y".into(), pkg.clone()]),
                Some("apk") => ("sudo", vec!["apk".into(), "add".into(), pkg.clone()]),
                _ => { eprintln!("error 404 string {line}  // no package manager found"); return Ok(Value::Nil); }
            }
        }
        "windows" => ("winget", vec!["install".into(), "--silent".into(), pkg.clone()]),
        "bsd" => ("pkg", vec!["install".into(), "-y".into(), pkg.clone()]),
        _ => { eprintln!("error 501 string {line}  // unsupported OS for install_package"); return Ok(Value::Nil); }
    };

    println!("$ {} {}", program, install_args.join(" "));

    let dry_run = std::env::var("RACH_DRY_RUN").is_ok_and(|v| v == "1" || v.eq_ignore_ascii_case("true"));
    if dry_run {
        println!("// RACH_DRY_RUN=1 — skipped execution");
        println!("completed");
        return Ok(Value::Bool(true));
    }

    let mut cmd = Command::new(program);
    cmd.args(&install_args);
    let result = cmd.status();
    match result {
        Ok(s) if s.success() => { println!("completed"); Ok(Value::Bool(true)) }
        Ok(s) => {
            let code = i64::from(s.code().unwrap_or(1));
            eprintln!("error {} string {}  // install_package exited with {}", 400 + code, line, code);
            Ok(Value::Bool(false))
        }
        Err(e) => {
            eprintln!("error 500 string {line}  // install_package: {e}");
            Ok(Value::Bool(false))
        }
    }
}

pub fn create_file(args: &[Value], line: usize) -> Result<Value, RuntimeError> {
    let path = nth_str(args, 0, line, "create_file")?;
    let content = nth_str(args, 1, line, "create_file").unwrap_or_default();
    match fs::write(&path, content.as_bytes()) {
        Ok(()) => { println!("created: {path}"); println!("completed"); Ok(Value::Str(path)) }
        Err(e) => Err(RuntimeError::new(500, line, format!("create_file({path}): {e}"))),
    }
}

pub fn read_file(args: &[Value], line: usize, capturing: bool) -> Result<Value, RuntimeError> {
    let path = first_str(args, line, "read_file")?;
    match fs::read_to_string(&path) {
        Ok(s) => {
            if !capturing {
                print!("{s}");
                if !s.ends_with('\n') { println!(); }
                println!("completed");
            }
            Ok(Value::Str(s))
        }
        Err(e) => Err(RuntimeError::new(404, line, format!("read_file({path}): {e}"))),
    }
}

pub fn edit_file(args: &[Value], line: usize) -> Result<Value, RuntimeError> {
    let path = nth_str(args, 0, line, "edit_file")?;
    let content = nth_str(args, 1, line, "edit_file")?;
    match fs::write(&path, content.as_bytes()) {
        Ok(()) => { println!("edited: {path}"); println!("completed"); Ok(Value::Str(path)) }
        Err(e) => Err(RuntimeError::new(500, line, format!("edit_file({path}): {e}"))),
    }
}

pub fn delete_file(args: &[Value], line: usize) -> Result<Value, RuntimeError> {
    let path = first_str(args, line, "delete_file")?;
    match fs::remove_file(&path) {
        Ok(()) => { println!("deleted: {path}"); println!("completed"); Ok(Value::Bool(true)) }
        Err(e) => Err(RuntimeError::new(404, line, format!("delete_file({path}): {e}"))),
    }
}

pub fn check_if_exists(args: &[Value], line: usize, capturing: bool) -> Result<Value, RuntimeError> {
    let path = first_str(args, line, "check_if_exists")?;
    let exists = Path::new(&path).exists();
    if !capturing {
        println!("{}: {}", path, if exists { "exists" } else { "missing" });
        println!("completed");
    }
    let _ = line;
    Ok(Value::Bool(exists))
}

pub fn reboot(line: usize) -> Result<Value, RuntimeError> {
    eprintln!("warn: reboot() is a destructive action; interpreter only prints intent");
    println!("would reboot system [line {line}]");
    println!("completed");
    Ok(Value::Nil)
}

pub fn shutdown(line: usize) -> Result<Value, RuntimeError> {
    eprintln!("warn: shutdown() is a destructive action; interpreter only prints intent");
    println!("would shut down system [line {line}]");
    println!("completed");
    Ok(Value::Nil)
}

mod which {
    use std::path::PathBuf;

    pub fn which_path(name: &str) -> Option<PathBuf> {
        let path = std::env::var_os("PATH")?;
        for dir in std::env::split_paths(&path) {
            let candidate = dir.join(name);
            if candidate.is_file() { return Some(candidate); }
            #[cfg(target_os = "windows")]
            {
                let exe = dir.join(format!("{}.exe", name));
                if exe.is_file() { return Some(exe); }
            }
        }
        None
    }
}
