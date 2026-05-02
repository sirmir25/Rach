pub mod ai;
pub mod ascii;
pub mod bash;
pub mod drivers;
pub mod native;
pub mod os;
pub mod system;
pub mod web;
pub mod webdriver;

use std::collections::BTreeMap;

use crate::ast::{CallSegment, Value};
use crate::interpreter::{Ctx, RuntimeError};

const KNOWN: &[&str] = &[
    // print / convenience
    "print", "echo",
    // short aliases (resolve to longer canonical names below)
    "os", "read", "write", "exists", "del", "run", "sh", "rm",
    // system / os
    "detect_os", "reboot", "shutdown",
    "run_command", "install_package",
    "create_file", "read_file", "edit_file", "delete_file", "check_if_exists",
    // web / browser
    "open_in_browser", "open_in_firefox", "open_in_chrome", "open_in_edge", "open_in_safari",
    "navigate_to", "open_new_tab", "switch_tab",
    "wait_seconds", "scroll_down_pixels",
    "take_screenshot", "press_key",
    "click_button", "click_element",
    "type_text", "fill_form", "login",
    "execute_js", "download_file", "upload_file",
    // ascii art
    "ascii_banner", "ascii_box", "ascii_pyramid", "ascii_diamond",
    "ascii_border", "ascii_mirror", "ascii_table",
    // native (C / C++)
    "native_crc32", "native_base64", "native_sort_ints", "native_reverse",
    "run_c", "run_cpp",
];

/// Single-word match — used by parser to decide if `name(...)` is a known
/// command vs. a user-fn call. Multi-word commands are matched separately
/// via the longest-prefix algorithm in `resolve_call`.
pub fn is_known_command(word: &str) -> bool {
    KNOWN.iter().any(|k| k.starts_with(word) && (k.len() == word.len() || k.as_bytes()[word.len()] == b'_'))
}

fn resolve_call(segments: &[CallSegment]) -> Result<(String, Vec<Value>, BTreeMap<String, Vec<Value>>, Vec<&CallSegment>), String> {
    if segments.is_empty() { return Err("empty call".into()); }

    let mut all_words: Vec<&str> = Vec::new();
    let mut segment_ends: Vec<usize> = Vec::new();
    for seg in segments {
        for w in &seg.words { all_words.push(w.as_str()); }
        segment_ends.push(all_words.len());
    }

    for n in (1..=all_words.len()).rev() {
        let candidate: String = all_words[..n].join("_");
        if !KNOWN.iter().any(|k| *k == candidate) { continue; }

        let split_seg = segment_ends.iter().position(|&e| e >= n).unwrap();
        let words_consumed_in_split = n - if split_seg == 0 { 0 } else { segment_ends[split_seg - 1] };

        // Return the segments *unchanged* — the caller resolves Exprs to Values
        // before this function is called, so segments here already hold Values.
        let _ = (words_consumed_in_split, split_seg);
        return Ok((candidate, Vec::new(), BTreeMap::new(), segments.iter().collect()));
    }

    Err(format!("unknown command `{}`", all_words.join("_")))
}

/// Resolve segments (already evaluated to Values) into (name, positional, kwargs).
pub fn resolve_resolved_segments(
    segments_resolved: &[ResolvedSegment],
) -> Result<(String, Vec<Value>, BTreeMap<String, Vec<Value>>), String> {
    if segments_resolved.is_empty() { return Err("empty call".into()); }

    let mut all_words: Vec<&str> = Vec::new();
    let mut segment_ends: Vec<usize> = Vec::new();
    for seg in segments_resolved {
        for w in &seg.words { all_words.push(w.as_str()); }
        segment_ends.push(all_words.len());
    }

    for n in (1..=all_words.len()).rev() {
        let candidate: String = all_words[..n].join("_");
        if !KNOWN.iter().any(|k| *k == candidate) { continue; }

        let split_seg = segment_ends.iter().position(|&e| e >= n).unwrap();
        let words_consumed_in_split = n - if split_seg == 0 { 0 } else { segment_ends[split_seg - 1] };

        let mut positional: Vec<Value> = Vec::new();
        let mut kwargs: BTreeMap<String, Vec<Value>> = BTreeMap::new();

        for (k, v) in &segments_resolved[split_seg].named {
            kwargs.entry(k.clone()).or_default().push(v.clone());
        }

        let split_seg_words = &segments_resolved[split_seg].words;
        if words_consumed_in_split == split_seg_words.len() {
            positional.extend(segments_resolved[split_seg].positional.iter().cloned());
        } else {
            let leftover: Vec<String> = split_seg_words[words_consumed_in_split..].iter().cloned().collect();
            let kname = leftover.join("_");
            kwargs.entry(kname).or_default().extend(segments_resolved[split_seg].positional.iter().cloned());
        }

        for seg in &segments_resolved[split_seg + 1..] {
            let kname = seg.words.join("_");
            kwargs.entry(kname.clone()).or_default().extend(seg.positional.iter().cloned());
            for (k, v) in &seg.named {
                kwargs.entry(k.clone()).or_default().push(v.clone());
            }
        }

        return Ok((candidate, positional, kwargs));
    }

    Err(format!("unknown command `{}`", all_words.join("_")))
}

/// A CallSegment with its Exprs already evaluated to Values.
pub struct ResolvedSegment {
    pub words: Vec<String>,
    pub positional: Vec<Value>,
    pub named: BTreeMap<String, Value>,
}

pub fn dispatch_resolved(
    segments_resolved: &[ResolvedSegment],
    line: usize,
    ctx: &mut Ctx,
) -> Result<Value, RuntimeError> {
    let (name, positional, kwargs) = resolve_resolved_segments(segments_resolved)
        .map_err(|e| RuntimeError::new(404, line, e))?;
    dispatch(&name, &positional, &kwargs, line, ctx)
}

pub fn dispatch(
    name: &str,
    positional: &[Value],
    kwargs: &BTreeMap<String, Vec<Value>>,
    line: usize,
    ctx: &mut Ctx,
) -> Result<Value, RuntimeError> {
    // Short aliases — rewritten to canonical names so the rest of the
    // dispatcher stays a single match statement.
    let canonical: &str = match name {
        "os" => "detect_os",
        "read" => "read_file",
        "write" => "create_file",
        "exists" => "check_if_exists",
        "del" | "rm" => "delete_file",
        "run" | "sh" => "run_command",
        "echo" => "print",
        other => other,
    };

    match canonical {
        // ---- print ----
        "print" => {
            let s: String = positional.iter().map(|v| v.as_str()).collect::<Vec<_>>().join(" ");
            println!("{}", s);
            Ok(Value::Str(s))
        }

        // ---- system / os ----
        "detect_os" => os::detect_os(line, ctx),
        "reboot" => system::reboot(line),
        "shutdown" => system::shutdown(line),
        "run_command" => system::run_command(positional, line),
        "install_package" => system::install_package(positional, line, ctx),
        "create_file" => system::create_file(positional, line),
        "read_file" => system::read_file(positional, line, ctx.capturing),
        "edit_file" => system::edit_file(positional, line),
        "delete_file" => system::delete_file(positional, line),
        "check_if_exists" => system::check_if_exists(positional, line, ctx.capturing),

        // ---- web / browser ----
        "open_in_browser" => web::open_in_browser(positional, line, ctx),
        "open_in_firefox" => web::open_in("firefox", positional, line, ctx),
        "open_in_chrome"  => web::open_in("chrome",  positional, line, ctx),
        "open_in_edge"    => web::open_in("edge",    positional, line, ctx),
        "open_in_safari"  => web::open_in("safari",  positional, line, ctx),
        "navigate_to"     => web::navigate_to(positional, line, ctx),
        "open_new_tab"    => web::open_new_tab(positional, line, ctx),
        "switch_tab"      => web::switch_tab(positional, line, ctx),
        "wait_seconds"    => web::wait_seconds(positional, line),
        "scroll_down_pixels" => web::scroll_down_pixels(positional, line, ctx),
        "take_screenshot" => web::take_screenshot(positional, line, ctx),
        "press_key"       => web::press_key(positional, line, ctx),
        "click_button"    => web::click_button(positional, line, ctx),
        "click_element"   => web::click_element(positional, line, ctx),
        "type_text"       => web::type_text(positional, line, ctx),
        "fill_form"       => web::fill_form(kwargs, line, ctx),
        "login"           => web::login(kwargs, line, ctx),
        "execute_js"      => web::execute_js(positional, line, ctx),
        "download_file"   => web::download_file(positional, line),
        "upload_file"     => web::upload_file(positional, line, ctx),

        // ---- ascii art ----
        "ascii_banner"  => ascii::banner(positional, kwargs, line),
        "ascii_box"     => ascii::box_around(positional, kwargs, line),
        "ascii_pyramid" => ascii::pyramid(positional, line),
        "ascii_diamond" => ascii::diamond(positional, line),
        "ascii_border"  => ascii::border(positional, kwargs, line),
        "ascii_mirror"  => ascii::mirror(positional, line),
        "ascii_table"   => ascii::table(positional, kwargs, line),

        // ---- native (C / C++) ----
        "native_crc32"     => native::native_crc32(positional, line, ctx),
        "native_base64"    => native::native_base64(positional, line, ctx),
        "native_sort_ints" => native::native_sort_ints(positional, line, ctx),
        "native_reverse"   => native::native_reverse(positional, line, ctx),
        "run_c"            => native::run_c(positional, line, ctx),
        "run_cpp"          => native::run_cpp(positional, line, ctx),

        other => Err(RuntimeError::new(404, line, format!("unknown command `{}`", other))),
    }
}

// Suppress unused warning on the legacy resolve_call (kept in case future
// refactors want it).
#[allow(dead_code)]
fn _legacy_resolve_call(segments: &[CallSegment]) -> Result<(String, Vec<Value>, BTreeMap<String, Vec<Value>>, Vec<&CallSegment>), String> {
    resolve_call(segments)
}
