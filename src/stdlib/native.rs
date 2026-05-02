//! C / C++ interop for Rach.
//!
//! Two layers of integration:
//!
//! 1. **Build-time FFI** — `native/util.c` and `native/util.cpp` are compiled
//!    by `build.rs` into static libs and linked into the interpreter. The
//!    `extern "C"` block below binds their symbols. Exposed as Rach commands:
//!      - `native_crc32(text)`        — CRC-32 of input bytes (hex string)
//!      - `native_base64(text)`       — base64 encode (C)
//!      - `native_sort_ints("1,3,2")` — quicksort via std::sort (C++)
//!      - `native_reverse(text)`      — byte-level reverse (C++)
//!
//! 2. **Runtime spawn** — `run_c(code)` / `run_cpp(code)` write a temp file,
//!    invoke `cc` / `c++`, run the binary, capture stdout. Lets Rach scripts
//!    drop into native code on demand without compiling Rach against it.

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::ast::Value;
use crate::interpreter::{Ctx, RuntimeError};

extern "C" {
    fn rach_crc32(data: *const u8, len: usize) -> u32;
    fn rach_base64_encode(input: *const u8, len: usize, output: *mut c_char) -> usize;
    fn rach_sort_csv_ints(inout: *mut c_char) -> c_int;
    fn rach_reverse_bytes(s: *mut c_char);
}

fn first_str(args: &[Value], line: usize, what: &str) -> Result<String, RuntimeError> {
    args.first()
        .map(|v| v.as_str())
        .ok_or_else(|| RuntimeError::new(400, line, format!("{} requires text argument", what)))
}

// ---- Build-time FFI commands ----

pub fn native_crc32(args: &[Value], line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let s = first_str(args, line, "native_crc32")?;
    let bytes = s.as_bytes();
    let crc = unsafe { rach_crc32(bytes.as_ptr(), bytes.len()) };
    let hex = format!("{:08x}", crc);
    if !ctx.capturing {
        println!("crc32: {}", hex);
        println!("completed");
    }
    Ok(Value::Str(hex))
}

pub fn native_base64(args: &[Value], line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let s = first_str(args, line, "native_base64")?;
    let bytes = s.as_bytes();
    let cap = (bytes.len() / 3 + 1) * 4 + 1;
    let mut buf: Vec<u8> = vec![0; cap];
    let written = unsafe {
        rach_base64_encode(bytes.as_ptr(), bytes.len(), buf.as_mut_ptr() as *mut c_char)
    };
    buf.truncate(written);
    let encoded = String::from_utf8(buf)
        .map_err(|e| RuntimeError::new(500, line, format!("base64: bad utf8 from C: {}", e)))?;
    if !ctx.capturing {
        println!("base64: {}", encoded);
        println!("completed");
    }
    Ok(Value::Str(encoded))
}

pub fn native_sort_ints(args: &[Value], line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let s = first_str(args, line, "native_sort_ints")?;
    // Allow extra room — output is always <= input length (sorted ints fit in same chars).
    let mut buf: Vec<u8> = Vec::with_capacity(s.len() + 32);
    buf.extend_from_slice(s.as_bytes());
    buf.resize(s.len() + 32, 0);
    let cstr = CString::new(s.as_bytes())
        .map_err(|_| RuntimeError::new(400, line, "native_sort_ints: input contains NUL byte"))?;
    let mut owned = cstr.into_bytes_with_nul();
    owned.resize(s.len() + 32, 0);

    let rc = unsafe { rach_sort_csv_ints(owned.as_mut_ptr() as *mut c_char) };
    if rc != 0 {
        return Err(RuntimeError::new(400, line, "native_sort_ints: failed to parse comma-separated integers"));
    }
    let result = unsafe { CStr::from_ptr(owned.as_ptr() as *const c_char) }
        .to_string_lossy()
        .into_owned();
    if !ctx.capturing {
        println!("sorted: {}", result);
        println!("completed");
    }
    Ok(Value::Str(result))
}

pub fn native_reverse(args: &[Value], line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let s = first_str(args, line, "native_reverse")?;
    let cstr = CString::new(s.as_bytes())
        .map_err(|_| RuntimeError::new(400, line, "native_reverse: input contains NUL byte"))?;
    let mut owned = cstr.into_bytes_with_nul();
    unsafe { rach_reverse_bytes(owned.as_mut_ptr() as *mut c_char) };
    let result = unsafe { CStr::from_ptr(owned.as_ptr() as *const c_char) }
        .to_string_lossy()
        .into_owned();
    if !ctx.capturing {
        println!("reversed: {}", result);
        println!("completed");
    }
    Ok(Value::Str(result))
}

// ---- Runtime spawn commands ----

pub fn run_c(args: &[Value], line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let code = first_str(args, line, "run_c")?;
    spawn_native("c", &code, line, ctx)
}

pub fn run_cpp(args: &[Value], line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let code = first_str(args, line, "run_cpp")?;
    spawn_native("cpp", &code, line, ctx)
}

fn spawn_native(lang: &str, code: &str, line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmpdir = std::env::temp_dir();
    let (src_ext, compiler) = match lang {
        "c"   => ("c",   c_compiler()),
        "cpp" => ("cpp", cpp_compiler()),
        _ => return Err(RuntimeError::new(400, line, format!("run_native: unknown language `{}`", lang))),
    };
    let src_path = tmpdir.join(format!("rach_run_{}_{}.{}", lang, stamp, src_ext));
    let bin_path = tmpdir.join(format!("rach_run_{}_{}", lang, stamp));

    std::fs::write(&src_path, code)
        .map_err(|e| RuntimeError::new(500, line, format!("write source: {}", e)))?;

    let mut compile = Command::new(&compiler);
    if lang == "cpp" {
        compile.arg("-std=c++17");
    } else {
        compile.arg("-std=c99");
    }
    compile.arg(&src_path).arg("-o").arg(&bin_path);
    let compile_out = compile.output()
        .map_err(|e| RuntimeError::new(500, line, format!("spawn {}: {}", compiler, e)))?;
    if !compile_out.status.success() {
        let _ = std::fs::remove_file(&src_path);
        let stderr = String::from_utf8_lossy(&compile_out.stderr);
        return Err(RuntimeError::new(400, line,
            format!("{} compile failed:\n{}", compiler, stderr.trim())));
    }

    let run = Command::new(&bin_path).output();
    let _ = std::fs::remove_file(&src_path);
    let _ = std::fs::remove_file(&bin_path);

    match run {
        Ok(o) => {
            let stdout = String::from_utf8_lossy(&o.stdout).into_owned();
            if !ctx.capturing {
                if !stdout.is_empty() {
                    print!("{}", stdout);
                    if !stdout.ends_with('\n') { println!(); }
                }
                if !o.stderr.is_empty() {
                    eprint!("{}", String::from_utf8_lossy(&o.stderr));
                }
                if o.status.success() {
                    println!("completed");
                } else {
                    let code = o.status.code().unwrap_or(1) as i64;
                    eprintln!("error {} string {}", 400 + code, line);
                }
            }
            Ok(Value::Str(stdout))
        }
        Err(e) => Err(RuntimeError::new(500, line, format!("run native binary: {}", e))),
    }
}

fn c_compiler() -> String {
    std::env::var("CC").unwrap_or_else(|_| "cc".to_string())
}

fn cpp_compiler() -> String {
    std::env::var("CXX").unwrap_or_else(|_| "c++".to_string())
}
