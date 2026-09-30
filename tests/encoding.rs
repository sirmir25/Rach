//! Encoding round-trips and RFC 4648 / Bitcoin / Adobe test vectors (cross-checked against
//! Python's `base64` and `urllib.parse`).

use rach::ast::Value;
use rach::interpreter::{self, make_ctx};
use rach::{lexer, parser};

fn run(expr: &str) -> Result<Value, String> {
    let src = format!("r = {expr}\n");
    let program = parser::parse(lexer::tokenize(&src).expect("lex")).expect("parse");
    let mut ctx = make_ctx(false, src.clone(), "<test>".into());
    interpreter::run_in_ctx(&program, &mut ctx).map_err(|e| e.message)?;
    Ok(ctx.lookup("r").expect("r bound"))
}

fn s(expr: &str) -> String {
    match run(expr) {
        Ok(Value::Str(s)) => s,
        other => panic!("`{expr}` -> expected a string, got {other:?}"),
    }
}

#[test]
fn base64_rfc4648_vectors() {
    for (plain, enc) in [("", ""), ("f", "Zg=="), ("fo", "Zm8="), ("foo", "Zm9v"), ("foob", "Zm9vYg=="), ("fooba", "Zm9vYmE="), ("foobar", "Zm9vYmFy")] {
        assert_eq!(s(&format!("base64_encode(\"{plain}\")")), enc);
        assert_eq!(s(&format!("base64_decode(\"{enc}\")")), plain);
    }
    assert_eq!(s("base64_encode(\"привет, мир\")"), "0L/RgNC40LLQtdGCLCDQvNC40YA=");
    assert_eq!(s("base64_encode(\"привет, мир\", url=true)"), "0L_RgNC40LLQtdGCLCDQvNC40YA");
    assert_eq!(s("base64_decode(\"0L_RgNC40LLQtdGCLCDQvNC40YA\")"), "привет, мир");
}

#[test]
fn base32_rfc4648_vectors() {
    for (plain, enc) in [("f", "MY======"), ("fo", "MZXQ===="), ("foo", "MZXW6==="), ("foob", "MZXW6YQ="), ("fooba", "MZXW6YTB"), ("foobar", "MZXW6YTBOI======")] {
        assert_eq!(s(&format!("base32_encode(\"{plain}\")")), enc);
        assert_eq!(s(&format!("base32_decode(\"{enc}\")")), plain);
    }
    assert_eq!(s("base32_decode(\"mzxw6ytboi\")"), "foobar", "lowercase and unpadded input decode too");
}

#[test]
fn base58_bitcoin_vectors() {
    assert_eq!(s("base58_encode(\"Hello World!\")"), "2NEpo7TZRRrLZSi2U");
    assert_eq!(s("base58_decode(\"2NEpo7TZRRrLZSi2U\")"), "Hello World!");
    // Leading zero bytes become leading `1`s and survive the round trip.
    assert_eq!(s("base58_encode(\"0000616263\", input=\"hex\")"), "11ZiCa");
    assert_eq!(s("base58_decode(\"11ZiCa\", output=\"hex\")"), "0000616263");
    assert!(run("base58_decode(\"0OIl\")").unwrap_err().contains("invalid base58"));
}

#[test]
fn ascii85_adobe_vectors() {
    for (plain, enc) in [("f", "Ac"), ("fo", "Ao@"), ("foo", "AoDS"), ("foob", "AoDTs"), ("foobar", "AoDTs@<)")] {
        assert_eq!(s(&format!("ascii85_encode(\"{plain}\")")), enc);
        assert_eq!(s(&format!("ascii85_decode(\"{enc}\")")), plain);
    }
    assert_eq!(s("ascii85_encode(\"0000000068656c6c6f\", input=\"hex\")"), "zBOu!rDZ");
    assert_eq!(s("ascii85_decode(\"<~zBOu!rDZ~>\", output=\"hex\")"), "0000000068656c6c6f");
}

#[test]
fn hex_binary_url() {
    assert_eq!(s("hex_encode(\"Rach\")"), "52616368");
    assert_eq!(s("hex_decode(\"52 61 63 68\")"), "Rach");
    assert_eq!(s("binary_encode(\"Hi\")"), "01001000 01101001");
    assert_eq!(s("binary_decode(\"0100100001101001\")"), "Hi");
    assert_eq!(s("url_encode(\"hello world/ä?&=~\")"), "hello%20world%2F%C3%A4%3F%26%3D~");
    assert_eq!(s("url_decode(\"hello%20world%2F%C3%A4%3F%26%3D~\")"), "hello world/ä?&=~");
    assert_eq!(s("url_decode(\"a+b\")"), "a+b");
    assert_eq!(s("url_decode(\"a+b\", form=true)"), "a b");
}

#[test]
fn binary_results_need_output_hex_not_garbage() {
    let err = run("base64_decode(\"/w==\")").unwrap_err();
    assert!(err.contains("output=\"hex\""), "non-UTF-8 result should say how to get it: {err}");
    assert_eq!(s("base64_decode(\"/w==\", output=\"hex\")"), "ff");
}

#[test]
fn malformed_input_is_a_clean_error() {
    for bad in ["base64_decode(\"A\")", "base64_decode(\"@@@@\")", "base32_decode(\"A\")", "hex_decode(\"abc\")",
                "hex_decode(\"zz\")", "binary_decode(\"0101\")", "url_decode(\"%4\")", "ascii85_decode(\"v\")"] {
        assert!(run(bad).is_err(), "{bad} should fail cleanly");
    }
}

#[test]
fn round_trips_through_every_codec() {
    for text in ["", "a", "Rach!", "The quick brown fox jumps over the lazy dog", "ünïcödé ✓ 日本"] {
        for codec in ["base64", "base32", "base58", "ascii85", "hex", "binary", "url"] {
            let got = s(&format!("{codec}_decode({codec}_encode(\"{text}\"))"));
            assert_eq!(got, text, "{codec} round trip");
        }
    }
}
