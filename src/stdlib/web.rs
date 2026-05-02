use std::collections::BTreeMap;
use std::process::Command;
use std::thread;
use std::time::Duration;

use crate::ast::Value;
use crate::interpreter::{Ctx, RuntimeError};
use crate::stdlib::webdriver;

fn first_str(args: &[Value], line: usize, what: &str) -> Result<String, RuntimeError> {
    args.first()
        .map(|v| v.as_str())
        .ok_or_else(|| RuntimeError::new(400, line, format!("{} requires an argument", what)))
}

fn one_kw(kwargs: &BTreeMap<String, Vec<Value>>, key: &str) -> Option<String> {
    kwargs.get(key).and_then(|v| v.first()).map(|v| v.as_str())
}

/// Ensure a live WebDriver session, optionally pinning a browser by name.
fn ensure_session<'a>(ctx: &'a mut Ctx, preferred: Option<&str>, line: usize) -> Result<&'a webdriver::Session, RuntimeError> {
    if ctx.wd.is_none() {
        match webdriver::start(preferred, ctx.headless) {
            Ok(s) => {
                eprintln!("// webdriver: {:?} session started on port {}", s.browser, s.port);
                ctx.wd = Some(s);
            }
            Err(e) => {
                ctx.wd_unavailable = Some(e.to_string());
                return Err(RuntimeError::new(503, line, format!("webdriver: {}", e)));
            }
        }
    }
    Ok(ctx.wd.as_ref().unwrap())
}

fn session(ctx: &Ctx, line: usize) -> Result<&webdriver::Session, RuntimeError> {
    if let Some(s) = ctx.wd.as_ref() { return Ok(s); }
    let msg = match &ctx.wd_unavailable {
        Some(reason) => format!("no active browser session — driver unavailable: {}", reason),
        None => "no active browser session — call `open in browser(...)` (or `open in chrome/firefox/edge`) first".into(),
    };
    Err(RuntimeError::new(409, line, msg))
}

// ---------- Public entry points ----------

pub fn open_in_browser(args: &[Value], line: usize, ctx: &mut Ctx) -> Result<Value, RuntimeError> {
    let url = first_str(args, line, "open_in_browser")?;
    match ensure_session(ctx, None, line) {
        Ok(s) => {
            s.navigate(&url).map_err(|e| RuntimeError::new(502, line, format!("navigate: {}", e)))?;
            println!("opened: {}", url);
            println!("completed");
            Ok(Value::Nil)
        }
        Err(driver_err) => {
            // Fallback: best-effort OS-default open. Lets simple "just visit a URL"
            // scripts work even without a driver installed.
            eprintln!("// {}", driver_err.message);
            eprintln!("// falling back to OS default `open`");
            os_open(&url, line)
        }
    }
}

pub fn open_in(browser: &str, args: &[Value], line: usize, ctx: &mut Ctx) -> Result<Value, RuntimeError> {
    let url = first_str(args, line, "open_in_<browser>")?;
    let s = ensure_session(ctx, Some(browser), line)?;
    s.navigate(&url).map_err(|e| RuntimeError::new(502, line, format!("navigate: {}", e)))?;
    println!("opened in {}: {}", browser, url);
    println!("completed");
    Ok(Value::Nil)
}

pub fn navigate_to(args: &[Value], line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let url = first_str(args, line, "navigate_to")?;
    let s = session(ctx, line)?;
    s.navigate(&url).map_err(|e| RuntimeError::new(502, line, format!("navigate: {}", e)))?;
    println!("navigated: {}", url);
    println!("completed");
    Ok(Value::Nil)
}

pub fn open_new_tab(args: &[Value], line: usize, ctx: &mut Ctx) -> Result<Value, RuntimeError> {
    let url = first_str(args, line, "open_new_tab")?;
    let s = ensure_session(ctx, None, line)?;
    s.new_window(&url).map_err(|e| RuntimeError::new(502, line, format!("new tab: {}", e)))?;
    println!("new tab: {}", url);
    println!("completed");
    Ok(Value::Nil)
}

pub fn switch_tab(args: &[Value], line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let n_str = first_str(args, line, "switch_tab")?;
    let n: usize = n_str.parse().map_err(|_| RuntimeError::new(400, line, format!("switch_tab: bad index `{}`", n_str)))?;
    let s = session(ctx, line)?;
    let handles = s.list_windows().map_err(|e| RuntimeError::new(502, line, format!("list tabs: {}", e)))?;
    let idx = if n == 0 { 0 } else { n.saturating_sub(1) };
    let handle = handles.get(idx)
        .ok_or_else(|| RuntimeError::new(404, line, format!("tab #{} does not exist (have {})", n, handles.len())))?;
    s.switch_to_window(handle).map_err(|e| RuntimeError::new(502, line, format!("switch: {}", e)))?;
    println!("switched to tab {}", n);
    println!("completed");
    Ok(Value::Nil)
}

pub fn wait_seconds(args: &[Value], line: usize) -> Result<Value, RuntimeError> {
    let s = first_str(args, line, "wait_seconds")?;
    let secs: u64 = s.parse().map_err(|_| RuntimeError::new(400, line, format!("wait_seconds: bad number `{}`", s)))?;
    if secs > 600 {
        return Err(RuntimeError::new(400, line, "wait_seconds: refusing to sleep > 600s"));
    }
    thread::sleep(Duration::from_secs(secs));
    println!("waited {}s", secs);
    println!("completed");
    Ok(Value::Nil)
}

pub fn scroll_down_pixels(args: &[Value], line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let p_str = first_str(args, line, "scroll_down_pixels")?;
    let s = session(ctx, line)?;
    let script = format!("window.scrollBy(0, {});", p_str.parse::<i64>().unwrap_or(0));
    s.execute_script(&script).map_err(|e| RuntimeError::new(502, line, format!("scroll: {}", e)))?;
    println!("scrolled {} px", p_str);
    println!("completed");
    Ok(Value::Nil)
}

pub fn take_screenshot(args: &[Value], line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let path = first_str(args, line, "take_screenshot")?;
    if let Some(s) = ctx.wd.as_ref() {
        s.screenshot(&path).map_err(|e| RuntimeError::new(502, line, format!("screenshot: {}", e)))?;
        println!("screenshot: {}", path);
        println!("completed");
        return Ok(Value::Nil);
    }
    // No driver session — fall back to OS-level screenshot if available.
    if cfg!(target_os = "macos") {
        let r = Command::new("screencapture").arg("-x").arg(&path).status();
        if matches!(r, Ok(s) if s.success()) {
            println!("screenshot: {}", path);
            println!("completed");
            return Ok(Value::Nil);
        }
    }
    Err(RuntimeError::new(502, line, "no browser session and no OS screenshot tool"))
}

pub fn press_key(args: &[Value], line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let k = first_str(args, line, "press_key")?;
    let s = session(ctx, line)?;
    let unicode = key_to_unicode(&k)
        .ok_or_else(|| RuntimeError::new(400, line, format!("press_key: unknown key `{}`", k)))?;
    s.active_element_send_keys(unicode).map_err(|e| RuntimeError::new(502, line, format!("press_key: {}", e)))?;
    println!("pressed: {}", k);
    println!("completed");
    Ok(Value::Nil)
}

pub fn click_button(args: &[Value], line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let label = first_str(args, line, "click_button")?;
    let s = session(ctx, line)?;
    // Match buttons by visible text. Covers <button>X</button>, <input type=submit value=X>, and aria-labelled controls.
    let escaped = xpath_escape(&label);
    let xpath = format!(
        "//button[normalize-space()={lit}] | \
         //input[(@type='submit' or @type='button') and (@value={lit} or @aria-label={lit})] | \
         //*[@role='button' and (normalize-space()={lit} or @aria-label={lit})] | \
         //a[normalize-space()={lit}]",
        lit = escaped
    );
    let eid = s.find_element("xpath", &xpath)
        .map_err(|e| RuntimeError::new(404, line, format!("click_button({}): {}", label, e)))?;
    s.click(&eid).map_err(|e| RuntimeError::new(502, line, format!("click: {}", e)))?;
    println!("clicked button: {}", label);
    println!("completed");
    Ok(Value::Nil)
}

pub fn click_element(args: &[Value], line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let sel = first_str(args, line, "click_element")?;
    let s = session(ctx, line)?;
    let (by, value) = guess_locator(&sel);
    let eid = s.find_element(by, value)
        .map_err(|e| RuntimeError::new(404, line, format!("click_element({}): {}", sel, e)))?;
    s.click(&eid).map_err(|e| RuntimeError::new(502, line, format!("click: {}", e)))?;
    println!("clicked: {}", sel);
    println!("completed");
    Ok(Value::Nil)
}

pub fn type_text(args: &[Value], line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let id = args.first().map(|v| v.as_str()).unwrap_or_default();
    let text = args.get(1).map(|v| v.as_str()).unwrap_or_default();
    if id.is_empty() { return Err(RuntimeError::new(400, line, "type_text(id, text) needs id")); }
    let s = session(ctx, line)?;
    let (by, value) = guess_locator(&id);
    let eid = s.find_element(by, value)
        .map_err(|e| RuntimeError::new(404, line, format!("type_text({}): {}", id, e)))?;
    s.send_keys(&eid, &text).map_err(|e| RuntimeError::new(502, line, format!("send_keys: {}", e)))?;
    println!("typed into #{}", id);
    println!("completed");
    Ok(Value::Nil)
}

pub fn fill_form(kwargs: &BTreeMap<String, Vec<Value>>, line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let id = one_kw(kwargs, "id").ok_or_else(|| RuntimeError::new(400, line, "fill_form requires id(...)"))?;
    let value = one_kw(kwargs, "value").ok_or_else(|| RuntimeError::new(400, line, "fill_form requires value(...)"))?;
    let s = session(ctx, line)?;
    let (by, lookup) = guess_locator(&id);
    let eid = s.find_element(by, lookup)
        .map_err(|e| RuntimeError::new(404, line, format!("fill_form({}): {}", id, e)))?;
    let _ = s.clear(&eid); // best-effort; ignore if not clearable
    s.send_keys(&eid, &value).map_err(|e| RuntimeError::new(502, line, format!("send_keys: {}", e)))?;
    println!("filled #{} = {:?}", id, value);
    println!("completed");
    Ok(Value::Nil)
}

pub fn login(kwargs: &BTreeMap<String, Vec<Value>>, line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let user = one_kw(kwargs, "user").ok_or_else(|| RuntimeError::new(400, line, "login requires user(...)"))?;
    let pwd = one_kw(kwargs, "pws").or_else(|| one_kw(kwargs, "password"))
        .ok_or_else(|| RuntimeError::new(400, line, "login requires pws(...)"))?;
    let s = session(ctx, line)?;

    let user_eid = s.find_element("css selector",
        "input[type=email], input[name=login], input[name=username], input[id=login], input[id=username], input[autocomplete=username]")
        .map_err(|e| RuntimeError::new(404, line, format!("login: username field not found: {}", e)))?;
    let _ = s.clear(&user_eid);
    s.send_keys(&user_eid, &user).map_err(|e| RuntimeError::new(502, line, format!("login (user): {}", e)))?;

    let pwd_eid = s.find_element("css selector", "input[type=password]")
        .map_err(|e| RuntimeError::new(404, line, format!("login: password field not found: {}", e)))?;
    let _ = s.clear(&pwd_eid);
    s.send_keys(&pwd_eid, &pwd).map_err(|e| RuntimeError::new(502, line, format!("login (pwd): {}", e)))?;

    // Submit by Enter — works without guessing the submit button selector.
    s.send_keys(&pwd_eid, "\u{E007}").map_err(|e| RuntimeError::new(502, line, format!("login (submit): {}", e)))?;
    println!("(login) user={:?} password=***", user);
    println!("completed");
    Ok(Value::Nil)
}

pub fn execute_js(args: &[Value], line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let code = first_str(args, line, "execute_js")?;
    let s = session(ctx, line)?;
    let v = s.execute_script(&code).map_err(|e| RuntimeError::new(502, line, format!("execute_js: {}", e)))?;
    println!("(js) -> {}", v);
    println!("completed");
    Ok(Value::Nil)
}

pub fn download_file(args: &[Value], line: usize) -> Result<Value, RuntimeError> {
    let url = args.first().map(|v| v.as_str()).unwrap_or_default();
    let path = args.get(1).map(|v| v.as_str()).unwrap_or_default();
    if url.is_empty() || path.is_empty() {
        return Err(RuntimeError::new(400, line, "download_file(url, path) requires both args"));
    }
    let cmd = format!("curl -L -o {} {}", shell_quote(&path), shell_quote(&url));
    println!("$ {}", cmd);
    let r = Command::new("sh").arg("-c").arg(&cmd).status();
    match r {
        Ok(s) if s.success() => { println!("completed"); Ok(Value::Nil) }
        _ => { eprintln!("error 502 string {}  // download failed", line); Ok(Value::Nil) }
    }
}

pub fn upload_file(args: &[Value], line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let path = args.first().map(|v| v.as_str()).unwrap_or_default();
    let id = args.get(1).map(|v| v.as_str()).unwrap_or_default();
    if path.is_empty() || id.is_empty() {
        return Err(RuntimeError::new(400, line, "upload_file(path, input_id) requires both"));
    }
    let s = session(ctx, line)?;
    let (by, lookup) = guess_locator(&id);
    let eid = s.find_element(by, lookup)
        .map_err(|e| RuntimeError::new(404, line, format!("upload_file({}): {}", id, e)))?;
    s.send_keys(&eid, &path).map_err(|e| RuntimeError::new(502, line, format!("upload: {}", e)))?;
    println!("uploaded {} -> #{}", path, id);
    println!("completed");
    Ok(Value::Nil)
}

// ---------- Helpers ----------

/// Heuristic: figure out what locator strategy the user gave us.
/// - starts with `#` → CSS id; `.` → CSS class; `/` → XPath; otherwise treated as id-or-name.
fn guess_locator(s: &str) -> (&'static str, &str) {
    if s.starts_with('/') {
        ("xpath", s)
    } else if s.starts_with('#') || s.starts_with('.') || s.contains(' ') || s.contains('[') {
        ("css selector", s)
    } else {
        // Bare token — most likely an id or name attribute.
        ("css selector", leak_combined(s))
    }
}

/// We need a `&'static str` for the matcher when we synthesize a CSS expression
/// like `[id=foo],[name=foo]`. Leak it — these strings live for the program's
/// lifetime and the count is bounded by the number of unique element references.
fn leak_combined(s: &str) -> &'static str {
    let combined = format!("[id='{0}'],[name='{0}']", s.replace('\'', "\\'"));
    Box::leak(combined.into_boxed_str())
}

fn xpath_escape(s: &str) -> String {
    if !s.contains('\'') { return format!("'{}'", s); }
    if !s.contains('"')  { return format!("\"{}\"", s); }
    // Both kinds of quotes — use concat()
    let parts: Vec<String> = s.split('\'').map(|p| format!("'{}'", p)).collect();
    format!("concat({}, \"'\", {})", parts[0], parts[1..].join(", \"'\", "))
}

/// Map common key names to the W3C WebDriver Unicode private-use codepoints.
fn key_to_unicode(name: &str) -> Option<&'static str> {
    Some(match name.to_ascii_lowercase().as_str() {
        "enter" | "return" => "\u{E007}",
        "tab" => "\u{E004}",
        "escape" | "esc" => "\u{E00C}",
        "space" => "\u{E00D}",
        "backspace" => "\u{E003}",
        "delete" => "\u{E017}",
        "arrow_up" | "up" => "\u{E013}",
        "arrow_down" | "down" => "\u{E015}",
        "arrow_left" | "left" => "\u{E012}",
        "arrow_right" | "right" => "\u{E014}",
        "home" => "\u{E011}",
        "end" => "\u{E010}",
        "pageup" | "page_up" => "\u{E00E}",
        "pagedown" | "page_down" => "\u{E00F}",
        _ => return None,
    })
}

/// OS-default URL opener, used only when no WebDriver is installed.
fn os_open(url: &str, line: usize) -> Result<Value, RuntimeError> {
    let result = if cfg!(target_os = "macos") {
        Command::new("open").arg(url).status()
    } else if cfg!(target_os = "windows") {
        Command::new("cmd").args(["/C", "start", "", url]).status()
    } else {
        Command::new("xdg-open").arg(url).status()
    };
    match result {
        Ok(s) if s.success() => { println!("opened: {}", url); println!("completed"); Ok(Value::Nil) }
        Ok(_) => Err(RuntimeError::new(500, line, "browser launch returned non-zero")),
        Err(e) => Err(RuntimeError::new(500, line, format!("browser launch: {}", e))),
    }
}

fn shell_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('\'');
    for c in s.chars() {
        if c == '\'' { out.push_str("'\\''"); } else { out.push(c); }
    }
    out.push('\'');
    out
}
