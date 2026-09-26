pub mod ai;
pub mod args;
pub mod ascii;
pub mod bash;
pub mod cipher;
pub mod collections;
pub mod drivers;
pub mod encoding;
pub mod enigma;
pub mod hash;
pub mod http;
pub mod io;
pub mod json;
pub mod logging;
pub mod math;
pub mod native;
pub mod os;
pub mod system;
pub mod time;
pub mod web;
pub mod webdriver;

use std::collections::BTreeMap;

use crate::ast::Value;
use crate::interpreter::{Ctx, RuntimeError};

const KNOWN: &[&str] = &[
    // print / convenience
    "print", "echo",
    // short aliases (resolve to longer canonical names below)
    "read", "write", "exists", "del", "run", "sh", "rm",
    // system / os
    "reboot", "shutdown",
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
    // math
    "sin", "cos", "tan", "asin", "acos", "atan", "atan2",
    "sqrt", "exp", "log", "log10", "log2", "pow",
    "abs", "floor", "ceil", "round",
    "min", "max", "sum", "avg",
    "radians", "degrees", "pi", "e",
    // logging  (NB: bare `log` is math's natural log, above)
    "log_message", "log_debug", "log_info", "log_warn", "log_error",
    "log_level", "log_to", "log_history", "log_filter", "log_count", "log_clear",
    // collections (string/list/map)
    "len", "split", "join", "contains", "slice", "append", "pop",
    "sorted", "reverse", "upper", "lower", "trim", "replace",
    "map_keys", "map_values", "map_set", "dict",
    "range", "enumerate", "keys", "values", "zip", "flatten",
    // filesystem
    "listdir", "mkdir", "isfile", "isdir",
    "path_join", "path_basename", "path_dirname", "path_ext",
    // env
    "env_get", "env_set",
    // io
    "input",
    // time
    "now", "now_ms", "sleep_ms", "format_time",
    // json
    "json_parse", "json_stringify",
    // http
    "http_get", "http_post",
    // encoding
    "base64_encode", "base64_decode", "base32_encode", "base32_decode",
    "base58_encode", "base58_decode", "ascii85_encode", "ascii85_decode",
    "hex_encode", "hex_decode", "binary_encode", "binary_decode",
    "url_encode", "url_decode",
    // hashing
    "md5", "sha1", "sha256", "sha512", "crc32", "adler32", "fnv1a", "hmac", "pbkdf2",
    // classical ciphers
    "caesar_encrypt", "caesar_decrypt", "rot13", "rot47", "atbash",
    "affine_encrypt", "affine_decrypt", "vigenere_encrypt", "vigenere_decrypt",
    "beaufort", "autokey_encrypt", "autokey_decrypt",
    "playfair_encrypt", "playfair_decrypt", "rail_fence_encrypt", "rail_fence_decrypt",
    "columnar_encrypt", "columnar_decrypt", "polybius_encrypt", "polybius_decrypt",
    "bifid_encrypt", "bifid_decrypt", "adfgvx_encrypt", "adfgvx_decrypt",
    "bacon_encrypt", "bacon_decrypt", "morse_encode", "morse_decode",
    "enigma",
];

/// Single-word match — used by parser to decide if `name(...)` is a known
/// command vs. a user-fn call. Multi-word commands are matched separately
/// via the longest-prefix algorithm in `resolve_resolved_segments`.
#[must_use]
pub fn is_known_command(word: &str) -> bool {
    KNOWN.iter().any(|k| k.starts_with(word) && (k.len() == word.len() || k.as_bytes()[word.len()] == b'_'))
}

/// Command name, positional args, and keyword args of a resolved call.
type CallParts = (String, Vec<Value>, BTreeMap<String, Vec<Value>>);

/// Resolve segments (already evaluated to Values) into (name, positional, kwargs).
pub fn resolve_resolved_segments(
    segments_resolved: &[ResolvedSegment],
) -> Result<CallParts, String> {
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
            let leftover: Vec<String> = split_seg_words[words_consumed_in_split..].to_vec();
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

/// A `CallSegment` with its Exprs already evaluated to Values.
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
            let s: String = positional.iter().map(Value::as_str).collect::<Vec<_>>().join(" ");
            println!("{s}");
            Ok(Value::Str(s))
        }

        // ---- system / os ----
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

        // ---- math ----
        "sin"     => math::sin(positional, line, ctx),
        "cos"     => math::cos(positional, line, ctx),
        "tan"     => math::tan(positional, line, ctx),
        "asin"    => math::asin(positional, line, ctx),
        "acos"    => math::acos(positional, line, ctx),
        "atan"    => math::atan(positional, line, ctx),
        "atan2"   => math::atan2(positional, line, ctx),
        "sqrt"    => math::sqrt(positional, line, ctx),
        "exp"     => math::exp(positional, line, ctx),
        "log"     => math::log(positional, line, ctx),
        "log10"   => math::log10(positional, line, ctx),
        "log2"    => math::log2(positional, line, ctx),
        "pow"     => math::pow(positional, line, ctx),
        "abs"     => math::abs(positional, line, ctx),
        "floor"   => math::floor(positional, line, ctx),
        "ceil"    => math::ceil(positional, line, ctx),
        "round"   => math::round(positional, line, ctx),
        "min"     => math::min(positional, line, ctx),
        "max"     => math::max(positional, line, ctx),
        "sum"     => math::sum(positional, line, ctx),
        "avg"     => math::avg(positional, line, ctx),
        "radians" => math::radians(positional, line, ctx),
        "degrees" => math::degrees(positional, line, ctx),
        "pi"      => math::pi(positional, line, ctx),
        "e"       => math::e_const(positional, line, ctx),

        // ---- logging ----
        // NB: `log` is math's natural logarithm; the level-tagged logger is `log_message`.
        "log_message" => logging::log(positional, line, ctx),
        "log_debug"   => logging::log_debug(positional, line, ctx),
        "log_info"    => logging::log_info(positional, line, ctx),
        "log_warn"    => logging::log_warn(positional, line, ctx),
        "log_error"   => logging::log_error(positional, line, ctx),
        "log_level"   => logging::log_level(positional, line, ctx),
        "log_to"      => logging::log_to(positional, line, ctx),
        "log_history" => logging::log_history(positional, line, ctx),
        "log_filter"  => logging::log_filter(positional, line, ctx),
        "log_count"   => logging::log_count(positional, line, ctx),
        "log_clear"   => logging::log_clear(positional, line, ctx),

        // ---- collections ----
        "len"        => collections::len(positional, line, ctx),
        "split"      => collections::split(positional, line, ctx),
        "join"       => collections::join(positional, line, ctx),
        "contains"   => collections::contains(positional, line, ctx),
        "slice"      => collections::slice(positional, line, ctx),
        "append"     => collections::append(positional, line, ctx),
        "pop"        => collections::pop(positional, line, ctx),
        "sorted"     => collections::sorted(positional, line, ctx),
        "reverse"    => collections::reverse(positional, line, ctx),
        "upper"      => collections::upper(positional, line, ctx),
        "lower"      => collections::lower(positional, line, ctx),
        "trim"       => collections::trim(positional, line, ctx),
        "replace"    => collections::replace(positional, line, ctx),
        "map_keys"   => collections::map_keys(positional, line, ctx),
        "map_values" => collections::map_values(positional, line, ctx),
        "map_set"    => collections::map_set(positional, line, ctx),
        "dict"       => collections::dict(positional, line, ctx),
        "range"      => collections::range(positional, line, ctx),
        "enumerate"  => collections::enumerate(positional, line, ctx),
        "keys"       => collections::keys(positional, line, ctx),
        "values"     => collections::values(positional, line, ctx),
        "zip"        => collections::zip(positional, line, ctx),
        "flatten"    => collections::flatten(positional, line, ctx),

        // ---- filesystem ----
        "listdir"       => os::listdir(positional, line, ctx),
        "mkdir"         => os::mkdir(positional, line, ctx),
        "isfile"        => os::isfile(positional, line, ctx),
        "isdir"         => os::isdir(positional, line, ctx),
        "path_join"     => os::path_join(positional, line, ctx),
        "path_basename" => os::path_basename(positional, line, ctx),
        "path_dirname"  => os::path_dirname(positional, line, ctx),
        "path_ext"      => os::path_ext(positional, line, ctx),

        // ---- env ----
        "env_get"    => os::env_get(positional, line, ctx),
        "env_set"    => os::env_set(positional, line, ctx),

        // ---- io ----
        "input"      => io::input(positional, line, ctx),

        // ---- time ----
        "now"         => time::now(positional, line, ctx),
        "now_ms"      => time::now_ms(positional, line, ctx),
        "sleep_ms"    => time::sleep_ms(positional, line, ctx),
        "format_time" => time::format_time(positional, line, ctx),

        // ---- json ----
        "json_parse"     => json::json_parse(positional, line, ctx),
        "json_stringify" => json::json_stringify(positional, line, ctx),

        // ---- http ----
        "http_get"  => http::http_get(positional, line, ctx),
        "http_post" => http::http_post(positional, line, ctx),

        // ---- encoding ----
        "base64_encode"  => encoding::base64_encode(positional, kwargs, line, ctx),
        "base64_decode"  => encoding::base64_decode(positional, kwargs, line, ctx),
        "base32_encode"  => encoding::base32_encode(positional, kwargs, line, ctx),
        "base32_decode"  => encoding::base32_decode(positional, kwargs, line, ctx),
        "base58_encode"  => encoding::base58_encode(positional, kwargs, line, ctx),
        "base58_decode"  => encoding::base58_decode(positional, kwargs, line, ctx),
        "ascii85_encode" => encoding::ascii85_encode(positional, kwargs, line, ctx),
        "ascii85_decode" => encoding::ascii85_decode(positional, kwargs, line, ctx),
        "hex_encode"     => encoding::hex_encode(positional, kwargs, line, ctx),
        "hex_decode"     => encoding::hex_decode(positional, kwargs, line, ctx),
        "binary_encode"  => encoding::binary_encode(positional, kwargs, line, ctx),
        "binary_decode"  => encoding::binary_decode(positional, kwargs, line, ctx),
        "url_encode"     => encoding::url_encode(positional, kwargs, line, ctx),
        "url_decode"     => encoding::url_decode(positional, kwargs, line, ctx),

        // ---- hashing ----
        "md5"     => hash::md5(positional, kwargs, line, ctx),
        "sha1"    => hash::sha1(positional, kwargs, line, ctx),
        "sha256"  => hash::sha256(positional, kwargs, line, ctx),
        "sha512"  => hash::sha512(positional, kwargs, line, ctx),
        "crc32"   => hash::crc32(positional, kwargs, line, ctx),
        "adler32" => hash::adler32(positional, kwargs, line, ctx),
        "fnv1a"   => hash::fnv1a(positional, kwargs, line, ctx),
        "hmac"    => hash::hmac(positional, kwargs, line, ctx),
        "pbkdf2"  => hash::pbkdf2(positional, kwargs, line, ctx),

        // ---- classical ciphers ----
        "caesar_encrypt"   => cipher::caesar_encrypt(positional, kwargs, line, ctx),
        "caesar_decrypt"   => cipher::caesar_decrypt(positional, kwargs, line, ctx),
        "rot13"            => cipher::rot13(positional, kwargs, line, ctx),
        "rot47"            => cipher::rot47_cmd(positional, kwargs, line, ctx),
        "atbash"           => cipher::atbash_cmd(positional, kwargs, line, ctx),
        "affine_encrypt"   => cipher::affine_encrypt(positional, kwargs, line, ctx),
        "affine_decrypt"   => cipher::affine_decrypt(positional, kwargs, line, ctx),
        "vigenere_encrypt" => cipher::vigenere_encrypt(positional, kwargs, line, ctx),
        "vigenere_decrypt" => cipher::vigenere_decrypt(positional, kwargs, line, ctx),
        "beaufort"         => cipher::beaufort_cmd(positional, kwargs, line, ctx),
        "autokey_encrypt"  => cipher::autokey_encrypt(positional, kwargs, line, ctx),
        "autokey_decrypt"  => cipher::autokey_decrypt(positional, kwargs, line, ctx),
        "playfair_encrypt"   => cipher::playfair_encrypt(positional, kwargs, line, ctx),
        "playfair_decrypt"   => cipher::playfair_decrypt(positional, kwargs, line, ctx),
        "rail_fence_encrypt" => cipher::rail_fence_encrypt(positional, kwargs, line, ctx),
        "rail_fence_decrypt" => cipher::rail_fence_decrypt(positional, kwargs, line, ctx),
        "columnar_encrypt"   => cipher::columnar_encrypt(positional, kwargs, line, ctx),
        "columnar_decrypt"   => cipher::columnar_decrypt(positional, kwargs, line, ctx),
        "polybius_encrypt"   => cipher::polybius_encrypt(positional, kwargs, line, ctx),
        "polybius_decrypt"   => cipher::polybius_decrypt(positional, kwargs, line, ctx),
        "bifid_encrypt"      => cipher::bifid_encrypt(positional, kwargs, line, ctx),
        "bifid_decrypt"      => cipher::bifid_decrypt(positional, kwargs, line, ctx),
        "adfgvx_encrypt"     => cipher::adfgvx_encrypt(positional, kwargs, line, ctx),
        "adfgvx_decrypt"     => cipher::adfgvx_decrypt(positional, kwargs, line, ctx),
        "bacon_encrypt"      => cipher::bacon_encrypt(positional, kwargs, line, ctx),
        "bacon_decrypt"      => cipher::bacon_decrypt(positional, kwargs, line, ctx),
        "morse_encode"       => cipher::morse_encode(positional, kwargs, line, ctx),
        "morse_decode"       => cipher::morse_decode(positional, kwargs, line, ctx),
        "enigma"             => enigma::enigma(positional, kwargs, line, ctx),

        other => Err(RuntimeError::new(404, line, format!("unknown command `{other}`"))),
    }
}
