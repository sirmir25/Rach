pub mod ai;
pub mod bash;
pub mod drivers;
pub mod os;
pub mod system;
pub mod web;
pub mod webdriver;

use std::collections::BTreeMap;

use crate::ast::{CallSegment, Value};
use crate::interpreter::{Ctx, RuntimeError};

/// All command names the runtime knows about. Listed longest-first inside
/// `resolve_call` doesn't matter — we explicitly try the longest concatenation
/// of leading words first and walk down. The list IS the registry, kept in
/// sync with the `match` in `dispatch`.
const KNOWN: &[&str] = &[
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
];

/// Turn a list of `(words, args)` segments into a single (name, positional, kwargs)
/// triple by finding the longest concatenation of leading words that matches
/// a known command. Trailing words and segments become kwargs.
///
/// Examples:
///   `wait seconds(5)`              → name=wait_seconds, pos=[5]
///   `open in browser("u")`         → name=open_in_browser, pos=["u"]
///   `fill form id("X") value("Y")` → name=fill_form, kwargs={id:[X], value:[Y]}
///   `login user("U") pws("P")`     → name=login,    kwargs={user:[U], pws:[P]}
fn resolve_call(segments: &[CallSegment]) -> Result<(String, Vec<Value>, BTreeMap<String, Vec<Value>>), String> {
    if segments.is_empty() { return Err("empty call".into()); }

    // Flatten leading words across all segments — but track which word boundary
    // each segment ends at so we can reattach args.
    let mut all_words: Vec<&str> = Vec::new();
    let mut segment_ends: Vec<usize> = Vec::new(); // index in all_words after segment i's words
    for seg in segments {
        for w in &seg.words { all_words.push(w.as_str()); }
        segment_ends.push(all_words.len());
    }

    // Try longest prefix match first.
    for n in (1..=all_words.len()).rev() {
        let candidate: String = all_words[..n].join("_");
        if !KNOWN.iter().any(|k| *k == candidate) { continue; }

        // Find which segment the n-th word belongs to.
        let split_seg = segment_ends.iter().position(|&e| e >= n).unwrap();
        let words_consumed_in_split = n - if split_seg == 0 { 0 } else { segment_ends[split_seg - 1] };

        let mut positional: Vec<Value> = Vec::new();
        let mut kwargs: BTreeMap<String, Vec<Value>> = BTreeMap::new();

        for (k, v) in &segments[split_seg].named {
            kwargs.entry(k.clone()).or_default().push(v.clone());
        }

        // If the split lands exactly at a segment boundary, the segment's args
        // are positional. Otherwise leftover words form the first kwarg name.
        let split_seg_words = &segments[split_seg].words;
        if words_consumed_in_split == split_seg_words.len() {
            positional.extend(segments[split_seg].positional.iter().cloned());
        } else {
            let leftover: Vec<String> = split_seg_words[words_consumed_in_split..].iter().cloned().collect();
            let kname = leftover.join("_");
            kwargs.entry(kname).or_default().extend(segments[split_seg].positional.iter().cloned());
        }

        // Subsequent segments are pure kwargs (their words form the key).
        for seg in &segments[split_seg + 1..] {
            let kname = seg.words.join("_");
            kwargs.entry(kname.clone()).or_default().extend(seg.positional.iter().cloned());
            for (k, v) in &seg.named {
                kwargs.entry(k.clone()).or_default().push(v.clone());
            }
        }

        return Ok((candidate, positional, kwargs));
    }

    // No match — surface the longest concatenation as the unknown command name.
    Err(format!("unknown command `{}`", all_words.join("_")))
}

pub fn dispatch_segments(segments: &[CallSegment], line: usize, ctx: &mut Ctx) -> Result<(), RuntimeError> {
    let (name, positional, kwargs) = resolve_call(segments)
        .map_err(|e| RuntimeError::new(404, line, e))?;
    dispatch(&name, &positional, &kwargs, line, ctx)
}

pub fn dispatch(
    name: &str,
    positional: &[Value],
    kwargs: &BTreeMap<String, Vec<Value>>,
    line: usize,
    ctx: &mut Ctx,
) -> Result<(), RuntimeError> {
    match name {
        // ---- system / os ----
        "detect_os" => os::detect_os(line, ctx),
        "reboot" => system::reboot(line),
        "shutdown" => system::shutdown(line),
        "run_command" => system::run_command(positional, line),
        "install_package" => system::install_package(positional, line, ctx),
        "create_file" => system::create_file(positional, line),
        "read_file" => system::read_file(positional, line),
        "edit_file" => system::edit_file(positional, line),
        "delete_file" => system::delete_file(positional, line),
        "check_if_exists" => system::check_if_exists(positional, line),

        // ---- web / browser (real WebDriver) ----
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

        other => Err(RuntimeError::new(404, line, format!("unknown command `{}`", other))),
    }
}
