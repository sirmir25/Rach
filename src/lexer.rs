#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    Word(String),
    Str(String),
    Int(i64),
    LParen,
    RParen,
    LBracket,
    RBracket,
    Comma,
    Equals,
    Colon,
    Newline,
}

#[derive(Debug, Clone)]
pub struct Token {
    pub tok: Tok,
    pub line: usize,
    pub col: usize,
}

#[derive(Debug)]
pub struct LexError {
    pub line: usize,
    pub message: String,
}

pub fn tokenize(source: &str) -> Result<Vec<Token>, LexError> {
    let mut tokens = Vec::new();
    let mut line = 1usize;
    let mut col = 1usize;
    let chars: Vec<char> = source.chars().collect();
    let mut i = 0usize;

    let push = |tokens: &mut Vec<Token>, tok: Tok, line: usize, col: usize| {
        tokens.push(Token { tok, line, col });
    };

    while i < chars.len() {
        let c = chars[i];

        if c == '\n' {
            push(&mut tokens, Tok::Newline, line, col);
            line += 1;
            col = 1;
            i += 1;
            continue;
        }

        if c == ' ' || c == '\t' || c == '\r' {
            col += 1;
            i += 1;
            continue;
        }

        if c == '#' || (c == '/' && i + 1 < chars.len() && chars[i + 1] == '/') {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
                col += 1;
            }
            continue;
        }

        let tok_col = col;

        if c == '(' { push(&mut tokens, Tok::LParen, line, tok_col); i += 1; col += 1; continue; }
        if c == ')' { push(&mut tokens, Tok::RParen, line, tok_col); i += 1; col += 1; continue; }
        if c == '[' { push(&mut tokens, Tok::LBracket, line, tok_col); i += 1; col += 1; continue; }
        if c == ']' { push(&mut tokens, Tok::RBracket, line, tok_col); i += 1; col += 1; continue; }
        if c == ',' { push(&mut tokens, Tok::Comma,  line, tok_col); i += 1; col += 1; continue; }
        if c == '=' { push(&mut tokens, Tok::Equals, line, tok_col); i += 1; col += 1; continue; }
        if c == ':' { push(&mut tokens, Tok::Colon,  line, tok_col); i += 1; col += 1; continue; }

        if c == '"' {
            let start_line = line;
            let start_col = tok_col;
            i += 1;
            col += 1;
            let mut s = String::new();
            while i < chars.len() && chars[i] != '"' {
                if chars[i] == '\\' && i + 1 < chars.len() {
                    match chars[i + 1] {
                        'n' => s.push('\n'),
                        't' => s.push('\t'),
                        'r' => s.push('\r'),
                        '\\' => s.push('\\'),
                        '"' => s.push('"'),
                        other => { s.push('\\'); s.push(other); }
                    }
                    i += 2;
                    col += 2;
                    continue;
                }
                if chars[i] == '\n' {
                    line += 1;
                    col = 1;
                } else {
                    col += 1;
                }
                s.push(chars[i]);
                i += 1;
            }
            if i >= chars.len() {
                return Err(LexError { line: start_line, message: "unterminated string".into() });
            }
            i += 1;
            col += 1;
            tokens.push(Token { tok: Tok::Str(s), line: start_line, col: start_col });
            continue;
        }

        if c.is_ascii_digit() || (c == '-' && i + 1 < chars.len() && chars[i + 1].is_ascii_digit()) {
            let start = i;
            let start_col = tok_col;
            if chars[i] == '-' { i += 1; col += 1; }
            while i < chars.len() && chars[i].is_ascii_digit() {
                i += 1;
                col += 1;
            }
            let lit: String = chars[start..i].iter().collect();
            let n: i64 = lit.parse().map_err(|_| LexError { line, message: format!("bad number {}", lit) })?;
            tokens.push(Token { tok: Tok::Int(n), line, col: start_col });
            continue;
        }

        if is_word_start(c) {
            let start = i;
            let start_col = tok_col;
            while i < chars.len() && is_word_cont(chars[i]) {
                i += 1;
                col += 1;
            }
            let w: String = chars[start..i].iter().collect();
            tokens.push(Token { tok: Tok::Word(w), line, col: start_col });
            continue;
        }

        return Err(LexError { line, message: format!("unexpected character '{}'", c) });
    }

    tokens.push(Token { tok: Tok::Newline, line, col });
    Ok(tokens)
}

fn is_word_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

fn is_word_cont(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}
