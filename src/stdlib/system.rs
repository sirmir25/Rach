use std::fs;
use std::path::Path;
use std::process::Command;

use crate::ast::Value;
use crate::interpreter::{Ctx, RuntimeError};

fn first_str(args: &[Value], line: usize, what: &str) -> Result<String, RuntimeError> {
    args.first()
        .map(|v| v.as_str())
        .ok_or_else(|| RuntimeError::new(400, line, format!("{} requires an argument", what)))
}

fn nth_str(args: &[Value], n: usize, line: usize, what: &str) -> Result<String, RuntimeError> {
    args.get(n)
        .map(|v| v.as_str())
        .ok_or_else(|| RuntimeError::new(400, line, format!("{} requires arg #{}", what, n + 1)))
}

pub fn run_command(args: &[Value], line: usize) -> Result<(), RuntimeError> {
    let cmd = first_str(args, line, "run_command")?;
    println!("$ {}", cmd);
    let (program, shell_arg) = if cfg!(target_os = "windows") {
        ("cmd", "/C")
    } else {
        ("sh", "-c")
    };
    let output = Command::new(program).arg(shell_arg).arg(&cmd).output();
    match output {
        Ok(o) => {
            if !o.stdout.is_empty() {
                print!("{}", String::from_utf8_lossy(&o.stdout));
            }
            if !o.stderr.is_empty() {
                eprint!("{}", String::from_utf8_lossy(&o.stderr));
            }
            if o.status.success() {
                println!("completed");
                Ok(())
            } else {
                let code = o.status.code().unwrap_or(1) as i64;
                eprintln!("error {} string {}", 400 + code, line);
                Ok(())
            }
        }
        Err(e) => Err(RuntimeError::new(500, line, format!("run_command failed: {}", e))),
    }
}

pub fn install_package(args: &[Value], line: usize, ctx: &mut Ctx) -> Result<(), RuntimeError> {
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
                Some("apt-get") | Some("apt") => ("sudo", vec!["apt-get".into(), "install".into(), "-y".into(), pkg.clone()]),
                Some("dnf") => ("sudo", vec!["dnf".into(), "install".into(), "-y".into(), pkg.clone()]),
                Some("yum") => ("sudo", vec!["yum".into(), "install".into(), "-y".into(), pkg.clone()]),
                Some("pacman") => ("sudo", vec!["pacman".into(), "-S".into(), "--noconfirm".into(), pkg.clone()]),
                Some("zypper") => ("sudo", vec!["zypper".into(), "install".into(), "-y".into(), pkg.clone()]),
                Some("apk") => ("sudo", vec!["apk".into(), "add".into(), pkg.clone()]),
                _ => { eprintln!("error 404 string {}  // no package manager found", line); return Ok(()); }
            }
        }
        "windows" => ("winget", vec!["install".into(), "--silent".into(), pkg.clone()]),
        "bsd" => ("pkg", vec!["install".into(), "-y".into(), pkg.clone()]),
        _ => { eprintln!("error 501 string {}  // unsupported OS for install_package", line); return Ok(()); }
    };

    println!("$ {} {}", program, install_args.join(" "));

    let dry_run = std::env::var("RACH_DRY_RUN").map(|v| v == "1" || v.eq_ignore_ascii_case("true")).unwrap_or(false);
    if dry_run {
        println!("// RACH_DRY_RUN=1 — skipped execution");
        println!("completed");
        return Ok(());
    }

    let mut cmd = std::process::Command::new(program);
    cmd.args(&install_args);
    let result = cmd.status();
    match result {
        Ok(s) if s.success() => { println!("completed"); Ok(()) }
        Ok(s) => {
            let code = s.code().unwrap_or(1) as i64;
            eprintln!("error {} string {}  // install_package exited with {}", 400 + code, line, code);
            Ok(())
        }
        Err(e) => {
            eprintln!("error 500 string {}  // install_package: {}", line, e);
            Ok(())
        }
    }
}

pub fn create_file(args: &[Value], line: usize) -> Result<(), RuntimeError> {
    let path = nth_str(args, 0, line, "create_file")?;
    let content = nth_str(args, 1, line, "create_file").unwrap_or_default();
    match fs::write(&path, content.as_bytes()) {
        Ok(_) => { println!("created: {}", path); println!("completed"); Ok(()) }
        Err(e) => { eprintln!("error 500 string {}  // create_file: {}", line, e); Ok(()) }
    }
}

pub fn read_file(args: &[Value], line: usize) -> Result<(), RuntimeError> {
    let path = first_str(args, line, "read_file")?;
    match fs::read_to_string(&path) {
        Ok(s) => { print!("{}", s); if !s.ends_with('\n') { println!(); } println!("completed"); Ok(()) }
        Err(e) => { eprintln!("error 404 string {}  // read_file: {}", line, e); Ok(()) }
    }
}

pub fn edit_file(args: &[Value], line: usize) -> Result<(), RuntimeError> {
    let path = nth_str(args, 0, line, "edit_file")?;
    let content = nth_str(args, 1, line, "edit_file")?;
    match fs::write(&path, content.as_bytes()) {
        Ok(_) => { println!("edited: {}", path); println!("completed"); Ok(()) }
        Err(e) => { eprintln!("error 500 string {}  // edit_file: {}", line, e); Ok(()) }
    }
}

pub fn delete_file(args: &[Value], line: usize) -> Result<(), RuntimeError> {
    let path = first_str(args, line, "delete_file")?;
    match fs::remove_file(&path) {
        Ok(_) => { println!("deleted: {}", path); println!("completed"); Ok(()) }
        Err(e) => { eprintln!("error 404 string {}  // delete_file: {}", line, e); Ok(()) }
    }
}

pub fn check_if_exists(args: &[Value], line: usize) -> Result<(), RuntimeError> {
    let path = first_str(args, line, "check_if_exists")?;
    let exists = Path::new(&path).exists();
    println!("{}: {}", path, if exists { "exists" } else { "missing" });
    println!("completed");
    Ok(())
}

pub fn reboot(line: usize) -> Result<(), RuntimeError> {
    eprintln!("warn: reboot() is a destructive action; interpreter only prints intent");
    println!("would reboot system [line {}]", line);
    println!("completed");
    Ok(())
}

pub fn shutdown(line: usize) -> Result<(), RuntimeError> {
    eprintln!("warn: shutdown() is a destructive action; interpreter only prints intent");
    println!("would shut down system [line {}]", line);
    println!("completed");
    Ok(())
}

/// Tiny inline `which` helper — avoids pulling a crate dependency.
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
