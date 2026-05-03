#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    Word(String),
    /// String literal — stored as a sequence of parts. Pure-literal strings end
    /// up as a single `StrPart::Lit`. `{expr}` inside the source produces an
    /// `Expr` part; `\{` writes a literal `{`.
    Str(Vec<StrPart>),
    Int(i64),
    Float(f64),
    LParen,
    RParen,
    LBracket,
    RBracket,
    LBrace,
    RBrace,
    Comma,
    Equals,
    Colon,
    Semicolon,
    Dot,
    Newline,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Caret,
    EqEq,
    BangEq,
    Lt,
    Gt,
    LtEq,
    GtEq,
    LtLt,
    GtGt,
    PlusEq,
    MinusEq,
    StarEq,
    SlashEq,
    PercentEq,
    PlusPlus,
    MinusMinus,
    Question,
    Amp,
    Pipe,
    Tilde,
    CaretCaret,
    ColonColon,
    Arrow,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StrPart {
    /// Literal text (already unescaped).
    Lit(String),
    /// Raw source for an interpolated expression (e.g. `name + 1`).
    /// We store the raw text and re-tokenize it later — that keeps the
    /// lexer state machine simple.
    Expr(String),
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
        if c == '{' { push(&mut tokens, Tok::LBrace, line, tok_col); i += 1; col += 1; continue; }
        if c == '}' { push(&mut tokens, Tok::RBrace, line, tok_col); i += 1; col += 1; continue; }
        if c == ',' { push(&mut tokens, Tok::Comma,  line, tok_col); i += 1; col += 1; continue; }
        if c == ';' { push(&mut tokens, Tok::Semicolon, line, tok_col); i += 1; col += 1; continue; }
        if c == '.' { push(&mut tokens, Tok::Dot, line, tok_col); i += 1; col += 1; continue; }
        if c == '?' { push(&mut tokens, Tok::Question, line, tok_col); i += 1; col += 1; continue; }
        if c == '~' { push(&mut tokens, Tok::Tilde, line, tok_col); i += 1; col += 1; continue; }
        if c == '&' { push(&mut tokens, Tok::Amp, line, tok_col); i += 1; col += 1; continue; }
        if c == '|' { push(&mut tokens, Tok::Pipe, line, tok_col); i += 1; col += 1; continue; }

        if c == ':' && i + 1 < chars.len() && chars[i + 1] == ':' {
            push(&mut tokens, Tok::ColonColon, line, tok_col); i += 2; col += 2; continue;
        }
        if c == ':' { push(&mut tokens, Tok::Colon, line, tok_col); i += 1; col += 1; continue; }

        if c == '+' && i + 1 < chars.len() && chars[i + 1] == '+' {
            push(&mut tokens, Tok::PlusPlus, line, tok_col); i += 2; col += 2; continue;
        }
        if c == '+' && i + 1 < chars.len() && chars[i + 1] == '=' {
            push(&mut tokens, Tok::PlusEq, line, tok_col); i += 2; col += 2; continue;
        }
        if c == '+' { push(&mut tokens, Tok::Plus, line, tok_col); i += 1; col += 1; continue; }

        if c == '-' && i + 1 < chars.len() && chars[i + 1] == '-' {
            push(&mut tokens, Tok::MinusMinus, line, tok_col); i += 2; col += 2; continue;
        }
        if c == '-' && i + 1 < chars.len() && chars[i + 1] == '=' {
            push(&mut tokens, Tok::MinusEq, line, tok_col); i += 2; col += 2; continue;
        }
        if c == '-' && i + 1 < chars.len() && chars[i + 1] == '>' {
            push(&mut tokens, Tok::Arrow, line, tok_col); i += 2; col += 2; continue;
        }
        if c == '-' { push(&mut tokens, Tok::Minus, line, tok_col); i += 1; col += 1; continue; }

        if c == '*' && i + 1 < chars.len() && chars[i + 1] == '=' {
            push(&mut tokens, Tok::StarEq, line, tok_col); i += 2; col += 2; continue;
        }
        if c == '*' { push(&mut tokens, Tok::Star, line, tok_col); i += 1; col += 1; continue; }

        if c == '/' && i + 1 < chars.len() && chars[i + 1] == '=' {
            push(&mut tokens, Tok::SlashEq, line, tok_col); i += 2; col += 2; continue;
        }
        if c == '/' { push(&mut tokens, Tok::Slash, line, tok_col); i += 1; col += 1; continue; }

        if c == '%' && i + 1 < chars.len() && chars[i + 1] == '=' {
            push(&mut tokens, Tok::PercentEq, line, tok_col); i += 2; col += 2; continue;
        }
        if c == '%' { push(&mut tokens, Tok::Percent, line, tok_col); i += 1; col += 1; continue; }

        if c == '^' && i + 1 < chars.len() && chars[i + 1] == '^' {
            push(&mut tokens, Tok::CaretCaret, line, tok_col); i += 2; col += 2; continue;
        }
        if c == '^' { push(&mut tokens, Tok::Caret, line, tok_col); i += 1; col += 1; continue; }

        if c == '=' && i + 1 < chars.len() && chars[i + 1] == '=' {
            push(&mut tokens, Tok::EqEq, line, tok_col); i += 2; col += 2; continue;
        }
        if c == '=' { push(&mut tokens, Tok::Equals, line, tok_col); i += 1; col += 1; continue; }

        if c == '!' && i + 1 < chars.len() && chars[i + 1] == '=' {
            push(&mut tokens, Tok::BangEq, line, tok_col); i += 2; col += 2; continue;
        }

        if c == '<' && i + 1 < chars.len() && chars[i + 1] == '<' {
            push(&mut tokens, Tok::LtLt, line, tok_col); i += 2; col += 2; continue;
        }
        if c == '<' && i + 1 < chars.len() && chars[i + 1] == '=' {
            push(&mut tokens, Tok::LtEq, line, tok_col); i += 2; col += 2; continue;
        }
        if c == '<' { push(&mut tokens, Tok::Lt, line, tok_col); i += 1; col += 1; continue; }

        if c == '>' && i + 1 < chars.len() && chars[i + 1] == '>' {
            push(&mut tokens, Tok::GtGt, line, tok_col); i += 2; col += 2; continue;
        }
        if c == '>' && i + 1 < chars.len() && chars[i + 1] == '=' {
            push(&mut tokens, Tok::GtEq, line, tok_col); i += 2; col += 2; continue;
        }
        if c == '>' { push(&mut tokens, Tok::Gt, line, tok_col); i += 1; col += 1; continue; }

        if c == '"' {
            let start_line = line;
            let start_col = tok_col;
            i += 1;
            col += 1;
            let mut parts: Vec<StrPart> = Vec::new();
            let mut buf = String::new();
            while i < chars.len() && chars[i] != '"' {
                if chars[i] == '\\' && i + 1 < chars.len() {
                    match chars[i + 1] {
                        'n' => buf.push('\n'),
                        't' => buf.push('\t'),
                        'r' => buf.push('\r'),
                        '\\' => buf.push('\\'),
                        '"' => buf.push('"'),
                        '{' => buf.push('{'),
                        '}' => buf.push('}'),
                        other => { buf.push('\\'); buf.push(other); }
                    }
                    i += 2;
                    col += 2;
                    continue;
                }
                if chars[i] == '{' {
                    if !buf.is_empty() { parts.push(StrPart::Lit(std::mem::take(&mut buf))); }
                    i += 1;
                    col += 1;
                    let mut depth = 1usize;
                    let mut expr = String::new();
                    while i < chars.len() && depth > 0 {
                        let ch = chars[i];
                        if ch == '{' { depth += 1; }
                        if ch == '}' { depth -= 1; if depth == 0 { break; } }
                        if ch == '\n' { line += 1; col = 1; } else { col += 1; }
                        expr.push(ch);
                        i += 1;
                    }
                    if i >= chars.len() {
                        return Err(LexError { line: start_line, message: "unterminated `{...}` in string".into() });
                    }
                    i += 1; // consume `}`
                    col += 1;
                    parts.push(StrPart::Expr(expr));
                    continue;
                }
                if chars[i] == '\n' {
                    line += 1;
                    col = 1;
                } else {
                    col += 1;
                }
                buf.push(chars[i]);
                i += 1;
            }
            if i >= chars.len() {
                return Err(LexError { line: start_line, message: "unterminated string".into() });
            }
            i += 1;
            col += 1;
            if !buf.is_empty() || parts.is_empty() {
                parts.push(StrPart::Lit(buf));
            }
            tokens.push(Token { tok: Tok::Str(parts), line: start_line, col: start_col });
            continue;
        }

        if c.is_ascii_digit() {
            let start = i;
            let start_col = tok_col;
            while i < chars.len() && chars[i].is_ascii_digit() {
                i += 1;
                col += 1;
            }
            let mut is_float = false;
            if i + 1 < chars.len() && chars[i] == '.' && chars[i + 1].is_ascii_digit() {
                is_float = true;
                i += 1;
                col += 1;
                while i < chars.len() && chars[i].is_ascii_digit() {
                    i += 1;
                    col += 1;
                }
            }
            let lit: String = chars[start..i].iter().collect();
            if is_float {
                let f: f64 = lit.parse().map_err(|_| LexError { line, message: format!("bad float {}", lit) })?;
                tokens.push(Token { tok: Tok::Float(f), line, col: start_col });
            } else {
                let n: i64 = lit.parse().map_err(|_| LexError { line, message: format!("bad number {}", lit) })?;
                tokens.push(Token { tok: Tok::Int(n), line, col: start_col });
            }
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
