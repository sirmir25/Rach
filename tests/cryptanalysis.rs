//! Breaking the classical ciphers: Babbage-style Vigenère cracking (key length via index of
//! coincidence, key letters via chi-squared), Caesar cracking, and the statistics behind them —
//! on real text in English (Ada Lovelace's 1843 Notes) and Russian (Pushkin).

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

fn field(expr: &str, key: &str) -> Value {
    match run(expr) {
        Ok(Value::Map(m)) => m.get(key).cloned().unwrap_or_else(|| panic!("no `{key}` in result")),
        other => panic!("`{expr}` -> expected a map, got {other:?}"),
    }
}

const LOVELACE: &str = "The distinctive characteristic of the Analytical Engine, and that which has rendered it \
possible to endow mechanism with such extensive faculties as bid fair to make this engine the executive \
right-hand of abstract algebra, is the introduction into it of the principle which Jacquard devised for \
regulating, by means of punched cards, the most complicated patterns in the fabrication of brocaded stuffs. \
We may say most aptly that the Analytical Engine weaves algebraical patterns just as the Jacquard-loom weaves \
flowers and leaves. The Analytical Engine has no pretensions whatever to originate anything. It can do \
whatever we know how to order it to perform.";

const PUSHKIN: &str = "Мой дядя самых честных правил, Когда не в шутку занемог, Он уважать себя заставил \
И лучше выдумать не мог. Его пример другим наука; Но, боже мой, какая скука С больным сидеть и день и ночь, \
Не отходя ни шагу прочь! Какое низкое коварство Полуживого забавлять, Ему подушки поправлять, Печально \
подносить лекарство, Вздыхать и думать про себя: Когда же черт возьмет тебя!";

#[test]
fn cracks_vigenere_on_lovelace_notes() {
    for key in ["BABBAGE", "ADA", "ANALYTICAL", "LOVELACE"] {
        let ct = s(&format!("vigenere_encrypt(\"{LOVELACE}\", \"{key}\")"));
        let expr = format!("vigenere_crack(\"{ct}\")");
        match field(&expr, "key") {
            Value::Str(found) => assert_eq!(found, key),
            other => panic!("key: {other:?}"),
        }
        match field(&expr, "plaintext") {
            Value::Str(p) => assert_eq!(p, LOVELACE),
            other => panic!("plaintext: {other:?}"),
        }
    }
}

#[test]
fn cracks_vigenere_in_russian() {
    let ct = s(&format!("vigenere_encrypt(\"{PUSHKIN}\", \"КЛЮЧ\", alphabet=\"ru\")"));
    match field(&format!("vigenere_crack(\"{ct}\", alphabet=\"ru\")"), "key") {
        Value::Str(found) => assert_eq!(found, "КЛЮЧ"),
        other => panic!("key: {other:?}"),
    }
}

#[test]
fn cracks_caesar_both_alphabets() {
    let ct = s(&format!("caesar_encrypt(\"{LOVELACE}\", 11)"));
    assert!(matches!(field(&format!("caesar_crack(\"{ct}\")"), "shift"), Value::Int(11)));
    let ct = s(&format!("caesar_encrypt(\"{PUSHKIN}\", 20, alphabet=\"ru\")"));
    match field(&format!("caesar_crack(\"{ct}\", alphabet=\"ru\")"), "plaintext") {
        Value::Str(p) => assert_eq!(p, PUSHKIN),
        other => panic!("{other:?}"),
    }
}

#[test]
fn index_of_coincidence_separates_language_from_noise() {
    let ic = |expr: &str| match run(expr) { Ok(Value::Float(f)) => f, other => panic!("{other:?}") };
    let english = ic(&format!("index_of_coincidence(\"{LOVELACE}\")"));
    let scrambled = ic(&format!("index_of_coincidence(vigenere_encrypt(\"{LOVELACE}\", \"QZXWJKVB\"))"));
    assert!((0.058..0.075).contains(&english), "English IC ≈ 0.066, got {english}");
    assert!(scrambled < 0.050, "polyalphabetic text flattens toward 1/26 ≈ 0.038, got {scrambled}");
}

#[test]
fn letter_frequencies_counts_letters_only() {
    match run("letter_frequencies(\"Hello, World!\")") {
        Ok(Value::Map(m)) => {
            assert!(matches!(m.get("L"), Some(Value::Int(3))));
            assert!(matches!(m.get("O"), Some(Value::Int(2))));
            assert!(!m.contains_key(","), "punctuation is not a letter");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn too_little_text_is_refused_not_guessed() {
    assert!(run("vigenere_crack(\"ABC\")").unwrap_err().contains("at least"));
}
