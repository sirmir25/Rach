//! Enigma I / M3: the canonical AAAAA→BDZGO vector, the double step, and a real intercepted
//! message from Operation Barbarossa (1941) decrypting to its known German plaintext.

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
fn canonical_vector() {
    assert_eq!(s("enigma(\"AAAAA\")"), "BDZGO");
    assert_eq!(s("enigma(\"BDZGO\")"), "AAAAA", "Enigma is its own inverse");
    assert_eq!(s("enigma(\"Hello, World\")"), "Ilbda, Amwaz", "case kept, punctuation passes through without stepping");
}

#[test]
fn double_step_across_both_notches() {
    // From ADU the middle rotor reaches its notch (E) and double-steps on the next key.
    assert_eq!(s("enigma(\"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\", positions=\"ADU\")"), "CBUPHGFJBWZFCKNFOGBXQCGVIBDZOV");
}

#[test]
fn full_settings_with_numeric_rings() {
    assert_eq!(
        s("enigma(\"THEQUICKBROWNFOXJUMPSOVERTHELAZYDOGTHEQUICKBROWNFOXJUMPSOVER\", rotors=\"IV II V\", reflector=\"C\", rings=\"02 21 12\", positions=\"KDO\", plugboard=\"AV BS CG DL FU HZ IN KM OW RX\")"),
        "ZOUHRHXYVAHZKZRKQYKKANYFFEMJBYUOBHQGBUDMEMOLOVMRKYTWSDGNTOIM"
    );
}

#[test]
fn operation_barbarossa_1941() {
    let ciphertext = "EDPUD NRGYS ZRCXN UYTPO MRMBO FKTBZ REZKM LXLVE FGUEY SIOZV EQMIK UBPMM YLKLT TDEIS MDICA GYKUA CTCDO MOHWX MUUIA UBSTS LRNBZ SZWNR FXWFY SSXJZ VIJHI DISHP RKLKA YUPAD TXQSP INQMA TLPIF SVKDA SCTAC DPBOP VHJK";
    let plain = s(&format!(
        "enigma(\"{ciphertext}\", rotors=\"II IV V\", reflector=\"B\", rings=\"BUL\", positions=\"BLA\", plugboard=\"AV BS CG DL FU HZ IN KM OW RX\")"
    )).replace(' ', "");
    assert!(plain.starts_with("AUFKLXABTEILUNGXVONXKURTINOWAXKURTINOWAXNORDWESTLXSEBEZ"), "got {plain}");
    assert!(plain.ends_with("ANGRIFFXINFXRGTX"), "got {plain}");
}

#[test]
fn bad_settings_are_clean_errors() {
    for (bad, needle) in [
        ("enigma(\"A\", rotors=\"I II\")", "3 names"),
        ("enigma(\"A\", rotors=\"I I II\")", "only be used once"),
        ("enigma(\"A\", rotors=\"I II VI\")", "unknown rotor"),
        ("enigma(\"A\", reflector=\"D\")", "unknown reflector"),
        ("enigma(\"A\", rings=\"AB\")", "exactly 3"),
        ("enigma(\"A\", plugboard=\"AB AC\")", "already cabled"),
        ("enigma(\"A\", plugboard=\"AA\")", "two different letters"),
    ] {
        let err = run(bad).expect_err(bad);
        assert!(err.contains(needle), "{bad}: {err}");
    }
}
