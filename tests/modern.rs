//! Modern cipher conformance: FIPS-197 (AES block), NIST SP 800-38A (CBC/CTR), RFC 8439
//! (ChaCha20, Poly1305, ChaCha20-Poly1305 AEAD), RC4 reference vectors — cross-checked against
//! Python's `cryptography` — plus tamper/wrong-key detection and password round trips.

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

const SUNSCREEN: &str = "Ladies and Gentlemen of the class of '99: If I could offer you only one tip for the future, sunscreen would be it.";

#[test]
fn aes_fips197_block_vectors() {
    // One raw block with ECB + input hex; PKCS#7 then adds a full padding block, so check the
    // first 32 hex digits (the vector's block).
    for (key, expected) in [
        ("000102030405060708090a0b0c0d0e0f", "69c4e0d86a7b0430d8cdb78070b4c55a"),
        ("000102030405060708090a0b0c0d0e0f1011121314151617", "dda97ca4864cdfe06eaf70a0ec0d7191"),
        ("000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f", "8ea2b7ca516745bfeafc49904b496089"),
    ] {
        let ct = s(&format!("aes_encrypt(\"00112233445566778899aabbccddeeff\", \"{key}\", mode=\"ecb\", input=\"hex\")"));
        assert_eq!(&ct[..32], expected, "AES-{}", key.len() * 4);
        assert_eq!(s(&format!("aes_decrypt(\"{ct}\", \"{key}\", mode=\"ecb\", output=\"hex\")")), "00112233445566778899aabbccddeeff");
    }
}

#[test]
fn aes_sp800_38a_modes() {
    let key = "2b7e151628aed2a6abf7158809cf4f3c";
    let pt = "6bc1bee22e409f96e93d7e117393172aae2d8a571e03ac9c9eb76fac45af8e51";
    let cbc = s(&format!("aes_encrypt(\"{pt}\", \"{key}\", iv=\"000102030405060708090a0b0c0d0e0f\", input=\"hex\")"));
    assert_eq!(&cbc[..64], "7649abac8119b246cee98e9b12e9197d5086cb9b507219ee95db113a917678b2");
    let ctr = s(&format!("aes_encrypt(\"{pt}\", \"{key}\", mode=\"ctr\", iv=\"f0f1f2f3f4f5f6f7f8f9fafbfcfdfeff\", input=\"hex\")"));
    assert_eq!(ctr, "874d6191b620e3261bef6864990db6ce9806f66b7970fdff8617187bb9fffdff");
    // PKCS#7 over text, matching `cryptography`'s padder.
    assert_eq!(
        s(&format!("aes_encrypt(\"Ada Lovelace, 1843\", \"{key}\", iv=\"00000000000000000000000000000000\")")),
        "d5cbe465eb4d7681d599dce8335ef2d3ab2eb53a010d7b335f989416f202da50"
    );
}

#[test]
fn aes_round_trips_with_random_iv_and_passphrase() {
    for mode in ["cbc", "ctr", "ecb"] {
        let ct = s(&format!("aes_encrypt(\"Привет, Энигма!\", \"correct horse battery staple\", mode=\"{mode}\")"));
        assert_eq!(s(&format!("aes_decrypt(\"{ct}\", \"correct horse battery staple\", mode=\"{mode}\")")), "Привет, Энигма!");
    }
    let a = s("aes_encrypt(\"same\", \"k\")");
    let b = s("aes_encrypt(\"same\", \"k\")");
    assert_ne!(a, b, "a fresh random IV each time: equal plaintexts must not give equal ciphertexts");
    assert!(run(&format!("aes_decrypt(\"{a}\", \"wrong key\")")).is_err(), "wrong key should fail padding, not return garbage");
}

#[test]
fn chacha20_rfc8439_2_4_2() {
    let key = "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f";
    assert_eq!(
        s(&format!("chacha20_encrypt(\"{SUNSCREEN}\", \"{key}\", nonce=\"000000000000004a00000000\")")),
        "6e2e359a2568f98041ba0728dd0d6981e97e7aec1d4360c20a27afccfd9fae0bf91b65c5524733ab8f593dabcd62b3571639d624e65152ab8f530c359f0861d807ca0dbf500d6a6156a38e088a22b65e52bc514d16ccf806818ce91ab77937365af90bbf74a35be6b40b8eedf2785e42874d"
    );
    let ct = s(&format!("chacha20_encrypt(\"{SUNSCREEN}\", \"passphrase\")"));
    assert_eq!(s(&format!("chacha20_decrypt(\"{ct}\", \"passphrase\")")), SUNSCREEN);
}

#[test]
fn poly1305_rfc8439_2_5_2() {
    assert_eq!(
        s("poly1305(\"Cryptographic Forum Research Group\", \"85d6be7857556d337f4452fe42d506a80103808afb0db2fd4abff6af4149f51b\")"),
        "a8061dc1305136c6c22b8baf0c0127a9"
    );
}

#[test]
fn chacha20_poly1305_rfc8439_2_8_2() {
    let key = "808182838485868788898a8b8c8d8e8f909192939495969798999a9b9c9d9e9f";
    // The RFC's AAD is binary (50515253c0c1c2c3c4c5c6c7), which Rach text can't carry, so check
    // the full vector through the Rust API and the text path via a round trip.
    let sealed = rach::stdlib::modern::chacha20_poly1305_seal(
        &<[u8; 32]>::try_from((0x80u8..=0x9f).collect::<Vec<u8>>()).unwrap(),
        &[0x07, 0, 0, 0, 0x40, 0x41, 0x42, 0x43, 0x44, 0x45, 0x46, 0x47],
        &[0x50, 0x51, 0x52, 0x53, 0xc0, 0xc1, 0xc2, 0xc3, 0xc4, 0xc5, 0xc6, 0xc7],
        SUNSCREEN.as_bytes(),
    );
    let hex: String = sealed.iter().map(|b| format!("{b:02x}")).collect();
    assert_eq!(hex, "d31a8d34648e60db7b86afbc53ef7ec2a4aded51296e08fea9e2b5a736ee62d63dbea45e8ca9671282fafb69da92728b1a71de0a9e060b2905d6a5b67ecd3b3692ddbd7f2d778b8c9803aee328091b58fab324e4fad675945585808b4831d7bc3ff4def08e4b7a9de576d26586cec64b61161ae10b594f09e26a7e902ecbd0600691");

    let ct = s(&format!("chacha20_poly1305_encrypt(\"{SUNSCREEN}\", \"{key}\", aad=\"header\")"));
    assert_eq!(s(&format!("chacha20_poly1305_decrypt(\"{ct}\", \"{key}\", aad=\"header\")")), SUNSCREEN);
    assert!(run(&format!("chacha20_poly1305_decrypt(\"{ct}\", \"{key}\", aad=\"other\")")).unwrap_err().contains("authentication failed"));
}

#[test]
fn aead_detects_every_single_bit_flip() {
    let ct = s("chacha20_poly1305_encrypt(\"attack at dawn\", \"k\")");
    for i in 0..ct.len() {
        let mut bytes: Vec<char> = ct.chars().collect();
        bytes[i] = if bytes[i] == '0' { '1' } else { '0' };
        let tampered: String = bytes.into_iter().collect();
        assert!(run(&format!("chacha20_poly1305_decrypt(\"{tampered}\", \"k\")")).is_err(), "flip at hex digit {i} went unnoticed");
    }
}

#[test]
fn password_encrypt_decrypt() {
    let token = s("encrypt(\"Ada's secret note\", \"hunter2\", iterations=1000)");
    assert!(token.starts_with("rach1$1000$"), "self-describing token: {token}");
    assert_eq!(s(&format!("decrypt(\"{token}\", \"hunter2\")")), "Ada's secret note");
    assert!(run(&format!("decrypt(\"{token}\", \"hunter3\")")).unwrap_err().contains("authentication failed"));
    assert_ne!(token, s("encrypt(\"Ada's secret note\", \"hunter2\", iterations=1000)"), "fresh salt and nonce each time");
    assert!(run("decrypt(\"nonsense\", \"x\")").unwrap_err().contains("not a Rach encrypted token"));
}

#[test]
fn rc4_and_xor() {
    assert_eq!(s("rc4_encrypt(\"Plaintext\", \"Key\")"), "bbf316e8d940af0ad3");
    assert_eq!(s("rc4_encrypt(\"pedia\", \"Wiki\")"), "1021bf0420");
    assert_eq!(s("rc4_encrypt(\"Attack at dawn\", \"Secret\")"), "45a01f645fc35b383552544b9bf5");
    assert_eq!(s("rc4_decrypt(\"45a01f645fc35b383552544b9bf5\", \"Secret\")"), "Attack at dawn");
    assert_eq!(s("xor_encrypt(\"AB\", \"A\")"), "0003");
    assert_eq!(s("xor_decrypt(xor_encrypt(\"hello\", \"key\"), \"key\")"), "hello");
}

#[test]
fn random_bytes_are_random() {
    let a = s("random_bytes(16)");
    assert_eq!(a.len(), 32);
    assert_ne!(a, s("random_bytes(16)"));
    assert!(run("random_bytes(0)").is_err());
}
