//! Hash / MAC / KDF test vectors (NIST, RFC 2104 / 6070-style, cross-checked against Python's
//! hashlib, hmac and zlib).

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

const FOX: &str = "The quick brown fox jumps over the lazy dog";

#[test]
fn md5_vectors() {
    assert_eq!(s("md5(\"\")"), "d41d8cd98f00b204e9800998ecf8427e");
    assert_eq!(s(&format!("md5(\"{FOX}\")")), "9e107d9d372bb6826bd81d3542a419d6");
}

#[test]
fn sha1_vectors() {
    assert_eq!(s("sha1(\"\")"), "da39a3ee5e6b4b0d3255bfef95601890afd80709");
    assert_eq!(s(&format!("sha1(\"{FOX}\")")), "2fd4e1c67a2d28fced849ee1bb76e7391b93eb12");
}

#[test]
fn sha256_vectors() {
    assert_eq!(s("sha256(\"\")"), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
    assert_eq!(s(&format!("sha256(\"{FOX}\")")), "d7a8fbb307d7809469ca9abcb0082e4f8d5651e46d3cdb762d02d0bf37c9e592");
    assert_eq!(s("sha256(\"привет\")"), "e58f1e8c55fa105bdd3f40e5037eb0b039b5998d52c05e6cd98878dd2da5cab2");
    // NIST's one-million-`a` vector: exercises thousands of blocks and the length field.
    assert_eq!(s("sha256(\"a\".repeat(1000000))"), "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0");
}

#[test]
fn sha512_vectors() {
    assert_eq!(s("sha512(\"\")"), "cf83e1357eefb8bdf1542850d66d8007d620e4050b5715dc83f4a921d36ce9ce47d0d13c5d85f2b0ff8318d2877eec2f63b931bd47417a81a538327af927da3e");
    assert_eq!(s(&format!("sha512(\"{FOX}\")")), "07e547d9586f6a73f73fbac0435ed76951218fb7d0c8d788a309d785436bbb642e93a252a954f23912547d1e8a3b5ed6e1bfd7097821233fa0538f3db854fee6");
}

#[test]
fn block_boundary_lengths_match_between_hashes() {
    // Messages of 55/56/63/64/65 bytes straddle the padding edge cases; hashing them via
    // text and via hex input must agree.
    for n in [55, 56, 63, 64, 65, 111, 112, 127, 128, 129] {
        let text = s(&format!("sha256(\"x\".repeat({n}))"));
        let hex = s(&format!("sha256(hex_encode(\"x\".repeat({n})), input=\"hex\")"));
        assert_eq!(text, hex, "length {n}");
        assert_eq!(s(&format!("sha512(\"x\".repeat({n}))")).len(), 128);
    }
}

#[test]
fn checksums() {
    assert_eq!(s(&format!("crc32(\"{FOX}\")")), "414fa339");
    match run(&format!("crc32(\"{FOX}\", format=\"int\")")) {
        Ok(Value::Int(n)) => assert_eq!(n, 1_095_738_169),
        other => panic!("expected int, got {other:?}"),
    }
    assert_eq!(s("adler32(\"Wikipedia\")"), "11e60398");
    assert_eq!(s("fnv1a(\"a\")"), "e40c292c");
    assert_eq!(s("fnv1a(\"a\", bits=64)"), "af63dc4c8601ec8c");
}

#[test]
fn hmac_vectors() {
    assert_eq!(s(&format!("hmac(\"{FOX}\", \"key\")")), "f7bc83f430538424b13298e6aa6fb143ef4d59a14946175997479dbc2d1a3cd8");
    assert_eq!(s(&format!("hmac(\"{FOX}\", \"key\", algo=\"md5\")")), "80070713463e7749b90c2dc24911e275");
    // Key longer than the block size gets hashed first (RFC 2104 §2).
    assert_eq!(
        s(&format!("hmac(\"{FOX}\", \"k\".repeat(200), algo=\"sha512\")")),
        "2ec850d56a434619da67d65f350b4a2caad666d274cf844ee9ac03f73e14d2012bc00387fc44ee2404aa91155181ae98ee75b0497788ca045997ef2462e82f91"
    );
}

#[test]
fn pbkdf2_vectors() {
    assert_eq!(s("pbkdf2(\"password\", \"salt\", 1, 32)"), "120fb6cffcf8b32c43e7225256c4f837a86548c92ccc35480805987cb70be17b");
    // 40 bytes > one SHA-256 block of output: checks the multi-block (T1 || T2) path.
    assert_eq!(
        s("pbkdf2(\"password\", \"salt\", iterations=4096, length=40)"),
        "c5e478d59288c841aa530db6845c4c8d962893a001ce4e11a4963873aa98134af7ad98c1b458ce3f"
    );
    assert!(run("pbkdf2(\"p\", \"s\", 0)").is_err());
}

#[test]
fn base64_output_and_bad_options() {
    assert_eq!(s("sha256(\"\", format=\"base64\")"), "47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU=");
    assert!(run("hmac(\"m\", \"k\", algo=\"sha3\")").unwrap_err().contains("unknown algo"));
    assert!(run("md5(\"x\", format=\"octal\")").is_err());
}
