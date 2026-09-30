//! Classical cipher vectors (Wikipedia's worked examples, cross-checked in Python) and
//! encrypt/decrypt round trips over both the English and Russian alphabets.

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
fn caesar_rot_atbash() {
    assert_eq!(s("caesar_encrypt(\"Hello, World!\", 3)"), "Khoor, Zruog!");
    assert_eq!(s("caesar_decrypt(\"Khoor, Zruog!\", 3)"), "Hello, World!");
    assert_eq!(s("caesar_encrypt(\"xyz\", -29)"), "uvw", "negative and >26 shifts wrap");
    assert_eq!(s("rot13(\"Why did the chicken cross the road?\")"), "Jul qvq gur puvpxra pebff gur ebnq?");
    assert_eq!(s("rot13(rot13(\"Rach\"))"), "Rach");
    assert_eq!(s("rot47(\"Hello\")"), "w6==@");
    assert_eq!(s("rot47(rot47(\"p@ss w0rd!\"))"), "p@ss w0rd!");
    assert_eq!(s("atbash(\"Hello\")"), "Svool");
}

#[test]
fn russian_alphabet() {
    assert_eq!(s("caesar_encrypt(\"ПРИВЕТ, МИР\", 3, alphabet=\"ru\")"), "ТУЛЕЗХ, ПЛУ");
    assert_eq!(s("caesar_decrypt(\"тулезх\", 3, alphabet=\"ru\")"), "привет", "lowercase keeps its case");
    assert_eq!(s("vigenere_encrypt(\"ШИФРОВАНИЕ\", \"КЛЮЧ\", alphabet=\"ru\")"), "ГФТЗЩНЮЕУР");
    assert_eq!(s("vigenere_decrypt(\"ГФТЗЩНЮЕУР\", \"КЛЮЧ\", alphabet=\"ru\")"), "ШИФРОВАНИЕ");
    assert_eq!(s("atbash(atbash(\"Ёжик в тумане\", alphabet=\"ru\"), alphabet=\"ru\")"), "Ёжик в тумане");
}

#[test]
fn affine() {
    assert_eq!(s("affine_encrypt(\"AFFINE CIPHER\", 5, 8)"), "IHHWVC SWFRCP");
    assert_eq!(s("affine_decrypt(\"IHHWVC SWFRCP\", 5, 8)"), "AFFINE CIPHER");
    assert!(run("affine_encrypt(\"x\", 13, 1)").unwrap_err().contains("coprime"));
}

#[test]
fn vigenere_family() {
    assert_eq!(s("vigenere_encrypt(\"ATTACKATDAWN\", \"LEMON\")"), "LXFOPVEFRNHR");
    assert_eq!(s("vigenere_decrypt(\"LXFOPVEFRNHR\", \"lemon\")"), "ATTACKATDAWN");
    // Non-letters pass through and don't advance the key.
    assert_eq!(s("vigenere_encrypt(\"Attack at dawn!\", \"LEMON\")"), "Lxfopv ef rnhr!");
    assert_eq!(s("beaufort(\"DEFENDTHEEASTWALLOFTHECASTLE\", \"FORTIFICATION\")"), "CKMPVCPVWPIWUJOGIUAPVWRIWUUK");
    assert_eq!(s("beaufort(\"CKMPVCPVWPIWUJOGIUAPVWRIWUUK\", \"FORTIFICATION\")"), "DEFENDTHEEASTWALLOFTHECASTLE", "Beaufort is self-inverse");
    assert_eq!(s("autokey_encrypt(\"ATTACKATDAWN\", \"QUEENLY\")"), "QNXEPVYTWTWP");
    assert_eq!(s("autokey_decrypt(\"QNXEPVYTWTWP\", \"QUEENLY\")"), "ATTACKATDAWN");
    assert!(run("vigenere_encrypt(\"x\", \"123\")").unwrap_err().contains("at least one letter"));
}

#[test]
fn round_trips() {
    let text = "Ada Lovelace wrote the first program, 1843.";
    for (enc, dec) in [
        ("caesar_encrypt(@T, 7)", "caesar_decrypt(@E, 7)"),
        ("affine_encrypt(@T, 7, 3)", "affine_decrypt(@E, 7, 3)"),
        ("vigenere_encrypt(@T, \"BABBAGE\")", "vigenere_decrypt(@E, \"BABBAGE\")"),
        ("autokey_encrypt(@T, \"ENGINE\")", "autokey_decrypt(@E, \"ENGINE\")"),
        ("beaufort(@T, \"ANALYTICAL\")", "beaufort(@E, \"ANALYTICAL\")"),
    ] {
        let enc_expr = enc.replace("@T", &format!("\"{text}\""));
        let e = s(&enc_expr);
        assert_ne!(e, text, "{enc} should change the text");
        let dec_expr = dec.replace("@E", &format!("\"{e}\""));
        assert_eq!(s(&dec_expr), text, "{enc} -> {dec}");
    }
}

#[test]
fn playfair_wheatstone_1854() {
    assert_eq!(s("playfair_encrypt(\"Hide the gold in the tree stump\", \"playfair example\")"), "BMODZBXDNABEKUDMUIXMMOUVIF");
    // The filler X between the doubled E's of "tree" can't be told apart from a real X.
    assert_eq!(s("playfair_decrypt(\"BMODZBXDNABEKUDMUIXMMOUVIF\", \"playfair example\")"), "HIDETHEGOLDINTHETREXESTUMP");
    assert!(run("playfair_decrypt(\"ABC\", \"k\")").unwrap_err().contains("even"));
}

#[test]
fn transposition_ciphers() {
    assert_eq!(s("rail_fence_encrypt(\"WEAREDISCOVEREDFLEEATONCE\", 3)"), "WECRLTEERDSOEEFEAOCAIVDEN");
    assert_eq!(s("rail_fence_decrypt(\"WECRLTEERDSOEEFEAOCAIVDEN\", 3)"), "WEAREDISCOVEREDFLEEATONCE");
    for rails in 1..=9 {
        assert_eq!(s(&format!("rail_fence_decrypt(rail_fence_encrypt(\"Ada, 1843!\", {rails}), {rails})")), "Ada, 1843!");
    }
    assert_eq!(s("columnar_encrypt(\"WEAREDISCOVEREDFLEEATONCE\", \"ZEBRAS\")"), "EVLNACDTESEAROFODEECWIREE");
    assert_eq!(s("columnar_decrypt(\"EVLNACDTESEAROFODEECWIREE\", \"ZEBRAS\")"), "WEAREDISCOVEREDFLEEATONCE");
    assert_eq!(s("columnar_decrypt(columnar_encrypt(\"short\", \"LONGERKEY\"), \"LONGERKEY\")"), "short");
}

#[test]
fn polybius_bifid_adfgvx() {
    assert_eq!(s("polybius_encrypt(\"Hello world\")"), "23 15 31 31 34 / 52 34 42 31 14");
    assert_eq!(s("polybius_decrypt(\"23 15 31 31 34 / 52 34 42 31 14\")"), "HELLO WORLD");
    assert_eq!(s("polybius_decrypt(polybius_encrypt(\"secret\", \"KEYWORD\"), \"KEYWORD\")"), "SECRET");
    assert!(run("polybius_decrypt(\"66\")").is_err());

    assert_eq!(s("bifid_encrypt(\"FLEEATONCE\", \"BGWKZQPNDSIOAXEFCLUMTHYVR\")"), "UAEOLWRINS");
    assert_eq!(s("bifid_decrypt(\"UAEOLWRINS\", \"BGWKZQPNDSIOAXEFCLUMTHYVR\")"), "FLEEATONCE");

    assert_eq!(s("adfgvx_encrypt(\"attack at 1200am\", \"NA1C3H8TB2OME5WRPD4F6G7I9J0KLQSUVXYZ\", \"PRIVACY\")"), "DGDDDAGDDGAFADDFDADVDVFAADVX");
    assert_eq!(s("adfgvx_decrypt(\"DGDD DAGD DGAF ADDF DADV DVFA ADVX\", \"NA1C3H8TB2OME5WRPD4F6G7I9J0KLQSUVXYZ\", \"PRIVACY\")"), "attackat1200am");
}

#[test]
fn bacon_and_morse() {
    assert_eq!(s("bacon_encrypt(\"Hi\")"), "AABBB ABAAA");
    assert_eq!(s("bacon_decrypt(\"aabbb abaaa\")"), "HI");
    assert!(run("bacon_decrypt(\"AAB\")").is_err());

    assert_eq!(s("morse_encode(\"SOS\")"), "... --- ...");
    assert_eq!(s("morse_encode(\"Hello World\")"), ".... . .-.. .-.. --- / .-- --- .-. .-.. -..");
    assert_eq!(s("morse_decode(\".... . .-.. .-.. --- / .-- --- .-. .-.. -..\")"), "HELLO WORLD");
    assert_eq!(s("morse_decode(\"··· −−− ···\")"), "SOS", "typographic dots and dashes are accepted");
    assert_eq!(s("morse_encode(\"Привет мир\", alphabet=\"ru\")"), ".--. .-. .. .-- . - / -- .. .-.");
    assert_eq!(s("morse_decode(\".--. .-. .. .-- . - / -- .. .-.\", alphabet=\"ru\")"), "ПРИВЕТ МИР");
    assert!(run("morse_encode(\"Ж\")").unwrap_err().contains("no Morse code"));
}
