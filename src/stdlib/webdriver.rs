//! Minimal W3C WebDriver client.
//!
//! Talks to `chromedriver`, `geckodriver`, or `msedgedriver` running on localhost
//! over plain HTTP/1.1. No external HTTP/TLS deps — we only ever connect to
//! 127.0.0.1, so a tiny hand-rolled client is sufficient and dependency-free.

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{json, Value as Json};

use crate::stdlib::drivers;

#[derive(Debug)]
pub struct WdError(pub String);

impl std::fmt::Display for WdError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{}", self.0) }
}

impl<E: std::error::Error> From<E> for WdError {
    fn from(e: E) -> Self { WdError(e.to_string()) }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Browser { Chrome, Firefox, Edge, Safari }

impl Browser {
    fn driver_bin(self) -> &'static str {
        match self {
            Browser::Chrome => "chromedriver",
            Browser::Firefox => "geckodriver",
            Browser::Edge => "msedgedriver",
            Browser::Safari => "safaridriver",
        }
    }
    fn capability_name(self) -> &'static str {
        match self {
            Browser::Chrome => "chrome",
            Browser::Firefox => "firefox",
            Browser::Edge => "MicrosoftEdge",
            Browser::Safari => "safari",
        }
    }
}

pub struct Session {
    pub browser: Browser,
    pub port: u16,
    pub session_id: String,
    /// Driver child process. `Some` when we spawned it ourselves; `None` if the
    /// user is running their own driver and we just connected.
    child: Option<Child>,
}

impl Drop for Session {
    fn drop(&mut self) {
        // Best-effort: end the WebDriver session, then reap the driver process.
        let _ = http_request(self.port, "DELETE", &format!("/session/{}", self.session_id), None);
        if let Some(child) = &mut self.child {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// Resolve driver: PATH → cache → auto-install. Returns the chosen browser
/// kind plus a path to its driver binary.
fn find_or_install_driver(preferred: Option<&str>) -> Result<(Browser, PathBuf), WdError> {
    let pref = preferred.map(|s| s.to_ascii_lowercase());
    let order: Vec<Browser> = match pref.as_deref() {
        Some("chrome")  => vec![Browser::Chrome],
        Some("firefox") => vec![Browser::Firefox],
        Some("edge")    => vec![Browser::Edge],
        Some("safari")  => vec![Browser::Safari],
        _ => {
            // Auto-pick: prefer browsers whose desktop app is actually installed.
            // Safari is intentionally excluded here — it requires a manual
            // `safaridriver --enable` + Develop-menu toggle that we can't
            // automate, so picking it silently confuses users. Users who want
            // it can write `open in safari("...")` explicitly.
            let mut o: Vec<Browser> = Vec::new();
            if drivers::chrome_installed()  { o.push(Browser::Chrome); }
            if drivers::firefox_installed() { o.push(Browser::Firefox); }
            if drivers::edge_installed()    { o.push(Browser::Edge); }
            if o.is_empty() { o.push(Browser::Firefox); } // last-resort
            o
        }
    };

    // Phase 1: PATH or cache — instant if anything's already there.
    for b in &order {
        if let Some(p) = drivers::which(b.driver_bin()) { return Ok((*b, p)); }
        let cached = drivers::driver_dir().join(b.driver_bin());
        if cached.is_file() { return Ok((*b, cached)); }
        if *b == Browser::Safari && cfg!(target_os = "macos") {
            let sys = PathBuf::from("/usr/bin/safaridriver");
            if sys.exists() { return Ok((Browser::Safari, sys)); }
        }
    }

    // Phase 2: auto-install for browsers that are installed but missing a driver.
    let mut last_err: Option<String> = None;
    for b in &order {
        match b {
            Browser::Chrome if drivers::chrome_installed() => {
                match drivers::ensure_chromedriver() {
                    Ok(p) => return Ok((Browser::Chrome, p)),
                    Err(e) => last_err = Some(format!("chromedriver auto-install: {}", e)),
                }
            }
            Browser::Firefox if drivers::firefox_installed() => {
                match drivers::ensure_geckodriver() {
                    Ok(p) => return Ok((Browser::Firefox, p)),
                    Err(e) => last_err = Some(format!("geckodriver auto-install: {}", e)),
                }
            }
            _ => {}
        }
    }

    Err(WdError(install_hint(preferred, last_err)))
}

fn install_hint(preferred: Option<&str>, last_err: Option<String>) -> String {
    let want = preferred.unwrap_or("any");
    let mut msg = format!(
        "no usable WebDriver for `{}`.\n  \
         tried: $PATH, ~/.cache/rach/drivers, and auto-install.\n  \
         install a browser so we can auto-download a driver, or install the driver yourself:\n    \
         brew install --cask google-chrome   # macOS — Chrome (auto-fetches chromedriver)\n    \
         brew install --cask firefox         # macOS — Firefox (auto-fetches geckodriver)\n    \
         apt install chromium-browser        # Linux — Chromium (driver via auto-install)\n    \
         apt install firefox                 # Linux — Firefox\n    \
         /usr/bin/safaridriver --enable      # macOS Safari — one-time, then enable Remote Automation in Develop menu",
        want
    );
    if let Some(e) = last_err { msg.push_str(&format!("\n  last error: {}", e)); }
    msg
}

/// Start a fresh driver subprocess on a free port and create a session.
pub fn start(preferred: Option<&str>, headless: bool) -> Result<Session, WdError> {
    let (browser, driver_path) = find_or_install_driver(preferred)?;
    let port = pick_free_port()?;

    let mut cmd = Command::new(&driver_path);
    match browser {
        Browser::Chrome | Browser::Edge => { cmd.arg(format!("--port={}", port)); }
        Browser::Firefox => { cmd.arg("--port").arg(port.to_string()); }
        Browser::Safari => { cmd.arg("--port").arg(port.to_string()); }
    }
    // Pipe driver stderr to /tmp so failures are diagnosable. stdout is
    // chatty and unhelpful — we leave it null.
    let log_path = std::env::temp_dir().join(format!("rach-driver-{}.log", port));
    let log_file = std::fs::File::create(&log_path).map_err(|e| WdError(format!("create driver log: {}", e)))?;
    let log_dup = log_file.try_clone().map_err(|e| WdError(format!("dup log fd: {}", e)))?;
    cmd.stdout(Stdio::from(log_file)).stderr(Stdio::from(log_dup));

    let child = cmd.spawn().map_err(|e| WdError(format!("failed to spawn `{}`: {}", driver_path.display(), e)))?;

    // Wait until the driver is listening (up to 10s).
    if !wait_for_port(port, Duration::from_secs(15)) {
        let mut child = child;
        let _ = child.kill();
        let log = std::fs::read_to_string(&log_path).unwrap_or_default();
        return Err(WdError(format!(
            "`{}` did not become ready on port {}; driver log:\n{}",
            driver_path.display(), port, log
        )));
    }

    let caps = build_capabilities(browser, headless);
    let resp = http_request(port, "POST", "/session", Some(&caps.to_string()))?;
    let body: Json = serde_json::from_str(&resp.body).map_err(|e| WdError(format!("bad session response: {} — body: {}", e, resp.body)))?;
    let session_id = body.pointer("/value/sessionId")
        .and_then(|v| v.as_str())
        .ok_or_else(|| WdError(format!("session response missing sessionId: {}", body)))?
        .to_string();

    Ok(Session { browser, port, session_id, child: Some(child) })
}

fn build_capabilities(browser: Browser, headless: bool) -> Json {
    let mut always_match = serde_json::Map::new();
    always_match.insert("browserName".into(), json!(browser.capability_name()));

    if headless {
        match browser {
            Browser::Chrome | Browser::Edge => {
                let key = if matches!(browser, Browser::Chrome) { "goog:chromeOptions" } else { "ms:edgeOptions" };
                always_match.insert(key.into(), json!({
                    "args": ["--headless=new", "--no-sandbox", "--disable-dev-shm-usage"]
                }));
            }
            Browser::Firefox => {
                always_match.insert("moz:firefoxOptions".into(), json!({
                    "args": ["-headless"]
                }));
            }
            Browser::Safari => {
                // Safari has no headless mode.
            }
        }
    }

    json!({ "capabilities": { "alwaysMatch": Json::Object(always_match) } })
}

// ------- High-level WebDriver actions -------

impl Session {
    pub fn navigate(&self, url: &str) -> Result<(), WdError> {
        let body = json!({ "url": url }).to_string();
        let r = http_request(self.port, "POST", &format!("/session/{}/url", self.session_id), Some(&body))?;
        ensure_ok(&r.body)
    }

    pub fn find_element(&self, by: &str, value: &str) -> Result<String, WdError> {
        let body = json!({ "using": by, "value": value }).to_string();
        let r = http_request(self.port, "POST", &format!("/session/{}/element", self.session_id), Some(&body))?;
        let v: Json = serde_json::from_str(&r.body).map_err(|e| WdError(format!("find_element: {} — {}", e, r.body)))?;
        if let Some(err) = v.pointer("/value/error").and_then(|x| x.as_str()) {
            return Err(WdError(format!("find_element({}, {}): {}", by, value, err)));
        }
        let inner = v.get("value").ok_or_else(|| WdError(format!("find_element: no value: {}", v)))?;
        if let Json::Object(map) = inner {
            // Element ref key is `element-6066-11e4-a52e-4f735466cecf` per W3C
            for val in map.values() {
                if let Some(s) = val.as_str() { return Ok(s.to_string()); }
            }
        }
        Err(WdError(format!("find_element: no element id in response: {}", v)))
    }

    pub fn click(&self, element_id: &str) -> Result<(), WdError> {
        let path = format!("/session/{}/element/{}/click", self.session_id, element_id);
        let r = http_request(self.port, "POST", &path, Some("{}"))?;
        ensure_ok(&r.body)
    }

    pub fn clear(&self, element_id: &str) -> Result<(), WdError> {
        let path = format!("/session/{}/element/{}/clear", self.session_id, element_id);
        let r = http_request(self.port, "POST", &path, Some("{}"))?;
        ensure_ok(&r.body)
    }

    pub fn send_keys(&self, element_id: &str, text: &str) -> Result<(), WdError> {
        let body = json!({ "text": text }).to_string();
        let path = format!("/session/{}/element/{}/value", self.session_id, element_id);
        let r = http_request(self.port, "POST", &path, Some(&body))?;
        ensure_ok(&r.body)
    }

    pub fn execute_script(&self, script: &str) -> Result<Json, WdError> {
        let body = json!({ "script": script, "args": [] }).to_string();
        let path = format!("/session/{}/execute/sync", self.session_id);
        let r = http_request(self.port, "POST", &path, Some(&body))?;
        let v: Json = serde_json::from_str(&r.body).map_err(|e| WdError(format!("execute_script: {} — {}", e, r.body)))?;
        Ok(v.get("value").cloned().unwrap_or(Json::Null))
    }

    pub fn screenshot(&self, path: &str) -> Result<(), WdError> {
        let url = format!("/session/{}/screenshot", self.session_id);
        let r = http_request(self.port, "GET", &url, None)?;
        let v: Json = serde_json::from_str(&r.body).map_err(|e| WdError(format!("screenshot: {}", e)))?;
        let b64 = v.get("value").and_then(|x| x.as_str())
            .ok_or_else(|| WdError("screenshot: no base64 payload".into()))?;
        let bytes = b64_decode(b64)?;
        std::fs::write(path, bytes).map_err(|e| WdError(format!("write screenshot: {}", e)))?;
        Ok(())
    }

    pub fn new_window(&self, url: &str) -> Result<(), WdError> {
        let path = format!("/session/{}/window/new", self.session_id);
        let body = json!({ "type": "tab" }).to_string();
        let r = http_request(self.port, "POST", &path, Some(&body))?;
        let v: Json = serde_json::from_str(&r.body).map_err(|e| WdError(format!("new_window: {}", e)))?;
        if let Some(handle) = v.pointer("/value/handle").and_then(|x| x.as_str()) {
            self.switch_to_window(handle)?;
        }
        if !url.is_empty() { self.navigate(url)?; }
        Ok(())
    }

    pub fn list_windows(&self) -> Result<Vec<String>, WdError> {
        let path = format!("/session/{}/window/handles", self.session_id);
        let r = http_request(self.port, "GET", &path, None)?;
        let v: Json = serde_json::from_str(&r.body).map_err(|e| WdError(format!("list_windows: {}", e)))?;
        let arr = v.get("value").and_then(|x| x.as_array())
            .ok_or_else(|| WdError("list_windows: no array".into()))?;
        Ok(arr.iter().filter_map(|x| x.as_str().map(String::from)).collect())
    }

    pub fn switch_to_window(&self, handle: &str) -> Result<(), WdError> {
        let path = format!("/session/{}/window", self.session_id);
        let body = json!({ "handle": handle }).to_string();
        let r = http_request(self.port, "POST", &path, Some(&body))?;
        ensure_ok(&r.body)
    }

    pub fn active_element_send_keys(&self, text: &str) -> Result<(), WdError> {
        // Get the active element id, then send keys to it.
        let r = http_request(self.port, "GET", &format!("/session/{}/element/active", self.session_id), None)?;
        let v: Json = serde_json::from_str(&r.body).map_err(|e| WdError(format!("active_element: {}", e)))?;
        let inner = v.get("value").ok_or_else(|| WdError("active_element: no value".into()))?;
        if let Json::Object(map) = inner {
            for val in map.values() {
                if let Some(s) = val.as_str() {
                    return self.send_keys(s, text);
                }
            }
        }
        Err(WdError("active_element: no id".into()))
    }
}

fn ensure_ok(body: &str) -> Result<(), WdError> {
    let v: Json = serde_json::from_str(body).map_err(|e| WdError(format!("bad response: {} — {}", e, body)))?;
    if let Some(err) = v.pointer("/value/error").and_then(|x| x.as_str()) {
        let msg = v.pointer("/value/message").and_then(|x| x.as_str()).unwrap_or("");
        return Err(WdError(format!("WebDriver error: {} — {}", err, msg)));
    }
    Ok(())
}

// ------- Tiny HTTP client (localhost only, plain HTTP/1.1) -------

struct HttpResp {
    #[allow(dead_code)]
    status: u16,
    body: String,
}

fn http_request(port: u16, method: &str, path: &str, body: Option<&str>) -> Result<HttpResp, WdError> {
    let addr_str = format!("127.0.0.1:{}", port);
    let addr = addr_str.to_socket_addrs()?.next().ok_or_else(|| WdError("bad addr".into()))?;

    // macOS quirk: `TcpStream::connect_timeout` can return WouldBlock (errno 35)
    // when the kernel hasn't fully established the connection yet. Retry the
    // plain blocking `connect` a few times — it's cheap on localhost.
    let mut stream = None;
    let mut last_err: Option<std::io::Error> = None;
    for _ in 0..20 {
        match TcpStream::connect(&addr) {
            Ok(s) => { stream = Some(s); break; }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock
                   || e.kind() == std::io::ErrorKind::ConnectionRefused => {
                last_err = Some(e);
                thread::sleep(Duration::from_millis(100));
            }
            Err(e) => return Err(WdError(format!("connect {}: {}", addr_str, e))),
        }
    }
    let mut stream = stream.ok_or_else(|| WdError(format!(
        "connect {}: {}", addr_str, last_err.map(|e| e.to_string()).unwrap_or_else(|| "no connection".into())
    )))?;

    stream.set_read_timeout(Some(Duration::from_secs(60)))?;
    stream.set_write_timeout(Some(Duration::from_secs(30)))?;

    let body_bytes = body.unwrap_or("");
    let mut req = format!(
        "{} {} HTTP/1.1\r\nHost: 127.0.0.1\r\nUser-Agent: rach/0.2\r\nAccept: application/json\r\nConnection: close\r\n",
        method, path
    );
    if body.is_some() {
        req.push_str("Content-Type: application/json\r\n");
        req.push_str(&format!("Content-Length: {}\r\n", body_bytes.len()));
    }
    req.push_str("\r\n");
    stream.write_all(req.as_bytes())?;
    if !body_bytes.is_empty() { stream.write_all(body_bytes.as_bytes())?; }
    stream.flush()?;

    let mut raw = Vec::new();
    stream.read_to_end(&mut raw)?;

    let (status, body_str) = parse_http_response(&raw)?;
    Ok(HttpResp { status, body: body_str })
}

fn parse_http_response(raw: &[u8]) -> Result<(u16, String), WdError> {
    let sep = b"\r\n\r\n";
    let split = raw.windows(sep.len()).position(|w| w == sep)
        .ok_or_else(|| WdError("malformed http response (no header/body separator)".into()))?;
    let head = &raw[..split];
    let body = &raw[split + sep.len()..];

    let head_str = std::str::from_utf8(head).map_err(|_| WdError("non-utf8 headers".into()))?;
    let status_line = head_str.lines().next().ok_or_else(|| WdError("empty status line".into()))?;
    let status: u16 = status_line.split_whitespace().nth(1).unwrap_or("0").parse().unwrap_or(0);

    // Check for chunked transfer encoding (chromedriver sometimes uses it).
    let lower = head_str.to_ascii_lowercase();
    let body_str = if lower.contains("transfer-encoding: chunked") {
        decode_chunked(body)?
    } else {
        String::from_utf8(body.to_vec()).map_err(|_| WdError("non-utf8 body".into()))?
    };
    Ok((status, body_str))
}

fn decode_chunked(input: &[u8]) -> Result<String, WdError> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < input.len() {
        // Read size line
        let line_end = input[i..].windows(2).position(|w| w == b"\r\n")
            .ok_or_else(|| WdError("chunked: no size line terminator".into()))?;
        let size_line = std::str::from_utf8(&input[i..i + line_end])
            .map_err(|_| WdError("chunked: non-utf8 size".into()))?;
        let size_hex = size_line.split(';').next().unwrap_or("0").trim();
        let size = usize::from_str_radix(size_hex, 16).map_err(|_| WdError("chunked: bad size".into()))?;
        i += line_end + 2;
        if size == 0 { break; }
        if i + size > input.len() { return Err(WdError("chunked: short chunk".into())); }
        out.extend_from_slice(&input[i..i + size]);
        i += size;
        if input.get(i..i + 2) == Some(b"\r\n") { i += 2; }
    }
    String::from_utf8(out).map_err(|_| WdError("chunked: non-utf8 body".into()))
}

// ------- Helpers -------

fn pick_free_port() -> Result<u16, WdError> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    drop(listener);
    Ok(port)
}

fn wait_for_port(port: u16, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        // Probe `/status`: any non-zero HTTP response proves the driver is
        // serving HTTP. We don't insist on 200 because some drivers return
        // odd statuses for `/status` until a session exists.
        if let Ok(r) = http_request(port, "GET", "/status", None) {
            if r.status > 0 { return true; }
        }
        thread::sleep(Duration::from_millis(200));
    }
    false
}

// Minimal base64 decoder so we don't need a `base64` crate dep.
fn b64_decode(input: &str) -> Result<Vec<u8>, WdError> {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut lookup = [255u8; 256];
    for (i, &c) in TABLE.iter().enumerate() { lookup[c as usize] = i as u8; }
    let bytes: Vec<u8> = input.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    let mut chunk = [0u8; 4];
    let mut idx = 0;
    for &b in &bytes {
        if b == b'=' { break; }
        let v = lookup[b as usize];
        if v == 255 { return Err(WdError(format!("base64: bad char {}", b as char))); }
        chunk[idx] = v;
        idx += 1;
        if idx == 4 {
            out.push((chunk[0] << 2) | (chunk[1] >> 4));
            out.push((chunk[1] << 4) | (chunk[2] >> 2));
            out.push((chunk[2] << 6) | chunk[3]);
            idx = 0;
        }
    }
    if idx == 2 { out.push((chunk[0] << 2) | (chunk[1] >> 4)); }
    else if idx == 3 {
        out.push((chunk[0] << 2) | (chunk[1] >> 4));
        out.push((chunk[1] << 4) | (chunk[2] >> 2));
    }
    Ok(out)
}
