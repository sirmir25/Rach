//! WebDriver auto-installer.
//!
//! Resolves a usable driver binary in this priority order:
//!   1. `$PATH`        — user-installed (`brew install --cask chromedriver` etc.)
//!   2. Rach cache     — previously auto-installed by us (`~/Library/Caches/rach/drivers`)
//!   3. Auto-download  — geckodriver from GitHub, chromedriver from Chrome for Testing
//!
//! Cache location is overridable with `$RACH_DRIVER_DIR`.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value as Json;

const GECKODRIVER_VERSION: &str = "0.36.0";

pub fn driver_dir() -> PathBuf {
    if let Ok(d) = std::env::var("RACH_DRIVER_DIR") {
        return PathBuf::from(d);
    }
    if cfg!(target_os = "windows") {
        let local = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| ".".into());
        PathBuf::from(local).join("rach").join("drivers")
    } else if cfg!(target_os = "macos") {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        PathBuf::from(home).join("Library/Caches/rach/drivers")
    } else {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        PathBuf::from(home).join(".cache/rach/drivers")
    }
}

pub fn firefox_installed() -> bool {
    if which("firefox").is_some() { return true; }
    if cfg!(target_os = "macos") {
        return Path::new("/Applications/Firefox.app/Contents/MacOS/firefox").exists();
    }
    if cfg!(target_os = "windows") {
        return Path::new(r"C:\Program Files\Mozilla Firefox\firefox.exe").exists()
            || Path::new(r"C:\Program Files (x86)\Mozilla Firefox\firefox.exe").exists();
    }
    false
}

pub fn chrome_installed() -> bool {
    for n in &["google-chrome", "google-chrome-stable", "chromium-browser", "chromium", "chrome"] {
        if which(n).is_some() { return true; }
    }
    if cfg!(target_os = "macos") {
        for p in &[
            "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
            "/Applications/Chromium.app/Contents/MacOS/Chromium",
        ] {
            if Path::new(p).exists() { return true; }
        }
    }
    if cfg!(target_os = "windows") {
        for p in &[
            r"C:\Program Files\Google\Chrome\Application\chrome.exe",
            r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe",
        ] {
            if Path::new(p).exists() { return true; }
        }
    }
    false
}

pub fn edge_installed() -> bool {
    if which("microsoft-edge").is_some() { return true; }
    if cfg!(target_os = "macos") {
        return Path::new("/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge").exists();
    }
    if cfg!(target_os = "windows") {
        return Path::new(r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe").exists();
    }
    false
}

/// Get geckodriver path: cache → download.
pub fn ensure_geckodriver() -> Result<PathBuf, String> {
    let dir = driver_dir();
    let bin = dir.join(if cfg!(windows) { "geckodriver.exe" } else { "geckodriver" });
    if bin.is_file() { return Ok(bin); }

    std::fs::create_dir_all(&dir).map_err(|e| format!("mkdir {}: {}", dir.display(), e))?;
    let asset = geckodriver_asset()?;
    let url = format!(
        "https://github.com/mozilla/geckodriver/releases/download/v{}/{}",
        GECKODRIVER_VERSION, asset
    );
    let archive = dir.join(&asset);
    eprintln!(
        "// rach: downloading geckodriver v{} ({}/{})...",
        GECKODRIVER_VERSION, std::env::consts::OS, std::env::consts::ARCH
    );
    run_curl(&url, &archive)?;
    extract(&archive, &dir)?;
    let _ = std::fs::remove_file(&archive);

    if !bin.is_file() {
        return Err(format!("geckodriver missing after extract at {}", bin.display()));
    }
    chmod_exec(&bin)?;
    Ok(bin)
}

/// Get chromedriver path: cache → Chrome-for-Testing download.
pub fn ensure_chromedriver() -> Result<PathBuf, String> {
    let dir = driver_dir();
    let bin = dir.join(if cfg!(windows) { "chromedriver.exe" } else { "chromedriver" });
    if bin.is_file() { return Ok(bin); }

    std::fs::create_dir_all(&dir).map_err(|e| format!("mkdir {}: {}", dir.display(), e))?;

    // Look up the latest stable chromedriver URL via Chrome for Testing's metadata.
    let api = "https://googlechromelabs.github.io/chrome-for-testing/last-known-good-versions-with-downloads.json";
    eprintln!("// rach: querying Chrome for Testing for latest stable chromedriver...");
    let json = curl_to_string(api)?;
    let v: Json = serde_json::from_str(&json).map_err(|e| format!("parse CfT json: {}", e))?;

    let plat = chrome_platform()?;
    let downloads = v.pointer("/channels/Stable/downloads/chromedriver")
        .and_then(|x| x.as_array())
        .ok_or_else(|| "Chrome-for-Testing JSON: no chromedriver list".to_string())?;

    let url = downloads.iter()
        .find(|d| d.get("platform").and_then(|p| p.as_str()) == Some(plat))
        .and_then(|d| d.get("url").and_then(|u| u.as_str()))
        .ok_or_else(|| format!("no chromedriver download for platform `{}`", plat))?
        .to_string();

    let archive = dir.join("chromedriver.zip");
    eprintln!("// rach: downloading chromedriver from {}...", url);
    run_curl(&url, &archive)?;
    extract(&archive, &dir)?;
    let _ = std::fs::remove_file(&archive);

    // CfT zip extracts to `chromedriver-<plat>/chromedriver(.exe)` — promote it.
    let want = if cfg!(windows) { "chromedriver.exe" } else { "chromedriver" };
    let found = find_file_recursive(&dir, want)
        .ok_or_else(|| "chromedriver binary not found after extract".to_string())?;
    if found != bin {
        if let Err(_) = std::fs::rename(&found, &bin) {
            std::fs::copy(&found, &bin).map_err(|e| format!("copy chromedriver: {}", e))?;
        }
    }
    chmod_exec(&bin)?;
    Ok(bin)
}

// ---------- internals ----------

fn chrome_platform() -> Result<&'static str, String> {
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;
    Ok(match (os, arch) {
        ("macos", "aarch64") => "mac-arm64",
        ("macos", "x86_64")  => "mac-x64",
        ("linux", "x86_64")  => "linux64",
        ("windows", "x86_64") => "win64",
        ("windows", "x86")   => "win32",
        _ => return Err(format!("unsupported platform: {}/{}", os, arch)),
    })
}

fn geckodriver_asset() -> Result<String, String> {
    let v = GECKODRIVER_VERSION;
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;
    Ok(match (os, arch) {
        ("macos",   "aarch64") => format!("geckodriver-v{}-macos-aarch64.tar.gz", v),
        ("macos",   "x86_64")  => format!("geckodriver-v{}-macos.tar.gz", v),
        ("linux",   "x86_64")  => format!("geckodriver-v{}-linux64.tar.gz", v),
        ("linux",   "aarch64") => format!("geckodriver-v{}-linux-aarch64.tar.gz", v),
        ("windows", "x86_64")  => format!("geckodriver-v{}-win64.zip", v),
        _ => return Err(format!("unsupported platform: {}/{}", os, arch)),
    })
}

fn run_curl(url: &str, out: &Path) -> Result<(), String> {
    let s = Command::new("curl").args(["-fSL", "--retry", "2", "-o"]).arg(out).arg(url).status()
        .map_err(|e| format!("spawn curl: {}", e))?;
    if !s.success() { return Err(format!("curl failed for {}", url)); }
    Ok(())
}

fn curl_to_string(url: &str) -> Result<String, String> {
    let out = Command::new("curl").args(["-fsSL", "--retry", "2", url]).output()
        .map_err(|e| format!("spawn curl: {}", e))?;
    if !out.status.success() {
        return Err(format!("curl failed for {} (stderr: {})", url, String::from_utf8_lossy(&out.stderr)));
    }
    String::from_utf8(out.stdout).map_err(|e| format!("curl stdout not utf-8: {}", e))
}

fn extract(archive: &Path, dest: &Path) -> Result<(), String> {
    let path_str = archive.to_string_lossy().to_lowercase();
    if path_str.ends_with(".tar.gz") || path_str.ends_with(".tgz") {
        let s = Command::new("tar").args(["-xzf"]).arg(archive).arg("-C").arg(dest).status()
            .map_err(|e| format!("spawn tar: {}", e))?;
        if !s.success() { return Err("tar extract failed".into()); }
    } else if path_str.ends_with(".zip") {
        if cfg!(target_os = "windows") {
            let cmd = format!(
                "Expand-Archive -Path '{}' -DestinationPath '{}' -Force",
                archive.display(), dest.display()
            );
            let s = Command::new("powershell").args(["-NoProfile", "-Command", &cmd]).status()
                .map_err(|e| format!("spawn powershell: {}", e))?;
            if !s.success() { return Err("Expand-Archive failed".into()); }
        } else {
            let s = Command::new("unzip").args(["-oq"]).arg(archive).arg("-d").arg(dest).status()
                .map_err(|e| format!("spawn unzip: {}", e))?;
            if !s.success() { return Err("unzip failed".into()); }
        }
    } else {
        return Err(format!("unknown archive type: {}", archive.display()));
    }
    Ok(())
}

fn find_file_recursive(root: &Path, name: &str) -> Option<PathBuf> {
    let entries = std::fs::read_dir(root).ok()?;
    for entry in entries.flatten() {
        let p = entry.path();
        if p.file_name().map(|n| n == name).unwrap_or(false) && p.is_file() {
            return Some(p);
        }
        if p.is_dir() {
            if let Some(r) = find_file_recursive(&p, name) { return Some(r); }
        }
    }
    None
}

fn chmod_exec(_p: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(_p).map_err(|e| format!("stat: {}", e))?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(_p, perms).map_err(|e| format!("chmod: {}", e))?;
    }
    Ok(())
}

pub fn which(name: &str) -> Option<PathBuf> {
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
