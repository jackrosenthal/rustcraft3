//! Tokenizer for JASS source.

use std::rc::Rc;

#[derive(Clone, Debug, PartialEq)]
pub enum Tok {
    Ident(Rc<str>),
    Int(i32),
    Real(f32),
    Str(Rc<str>),
    Newline,
    LParen,
    RParen,
    LBracket,
    RBracket,
    Comma,
    Assign,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Plus,
    Minus,
    Star,
    Slash,
    Eof,
}

#[derive(Clone, Debug)]
pub struct Token {
    pub tok: Tok,
    pub line: u32,
}

#[derive(Debug, thiserror::Error)]
#[error("line {line}: {msg}")]
pub struct SyntaxError {
    pub line: u32,
    pub msg: String,
}

pub fn tokenize(src: &str) -> Result<Vec<Token>, SyntaxError> {
    let b = src.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    let mut line = 1u32;
    let err = |line, msg: &str| SyntaxError { line, msg: msg.to_string() };
    while i < b.len() {
        let c = b[i];
        match c {
            b' ' | b'\t' => i += 1,
            b'\r' if b.get(i + 1) == Some(&b'\n') => i += 1,
            // A lone carriage return also ends a line.
            b'\n' | b'\r' => {
                if !matches!(out.last(), Some(Token { tok: Tok::Newline, .. }) | None) {
                    out.push(Token { tok: Tok::Newline, line });
                }
                line += 1;
                i += 1;
            }
            b'/' if b.get(i + 1) == Some(&b'/') => {
                while i < b.len() && b[i] != b'\n' && b[i] != b'\r' {
                    i += 1;
                }
            }
            b'"' => {
                i += 1;
                let mut s = Vec::new();
                while i < b.len() && b[i] != b'"' {
                    if b[i] == b'\\' && i + 1 < b.len() {
                        i += 1;
                        s.push(match b[i] {
                            b'n' => b'\n',
                            b'r' => b'\r',
                            b't' => b'\t',
                            b'b' => 8,
                            b'f' => 12,
                            other => other,
                        });
                    } else {
                        if b[i] == b'\n' {
                            line += 1;
                        }
                        s.push(b[i]);
                    }
                    i += 1;
                }
                i += 1;
                let s = String::from_utf8_lossy(&s).into_owned();
                out.push(Token { tok: Tok::Str(s.into()), line });
            }
            b'\'' => {
                // Four-character (or one-character) integer constant, big-endian.
                i += 1;
                let mut v: u32 = 0;
                while i < b.len() && b[i] != b'\'' {
                    let ch = if b[i] == b'\\' && i + 1 < b.len() {
                        i += 1;
                        b[i]
                    } else {
                        b[i]
                    };
                    v = (v << 8) | ch as u32;
                    i += 1;
                }
                i += 1;
                out.push(Token { tok: Tok::Int(v as i32), line });
            }
            b'0'..=b'9' | b'.' | b'$' => {
                let start = i;
                if c == b'$' || (c == b'0' && matches!(b.get(i + 1), Some(b'x' | b'X'))) {
                    i += if c == b'$' { 1 } else { 2 };
                    let hs = i;
                    while i < b.len() && b[i].is_ascii_hexdigit() {
                        i += 1;
                    }
                    let v = u32::from_str_radix(&src[hs..i], 16)
                        .map_err(|_| err(line, "bad hex literal"))?;
                    out.push(Token { tok: Tok::Int(v as i32), line });
                    continue;
                }
                while i < b.len() && (b[i].is_ascii_digit() || b[i] == b'.') {
                    i += 1;
                }
                let text = &src[start..i];
                if text.contains('.') {
                    // The whole part converted toward zero, as the original converts
                    // integers (measured: 123456789.0 reads 123456784, not the nearer
                    // 123456792), plus the fraction, in the original's software floats.
                    let v: f32 = if text == "." {
                        0.0
                    } else {
                        let (whole, frac) = text.split_once('.').unwrap_or((text, ""));
                        // The whole part wraps as a 32-bit integer (measured: the literal
                        // 1000000000000000000000.0 reads -559939584).
                        let whole = whole.bytes().fold(0i32, |v, c| v.wrapping_mul(10).wrapping_add((c - b'0') as i32)) as i64;
                        f32::from_bits(crate::softfloat::literal(whole as i32, frac))
                    };
                    out.push(Token { tok: Tok::Real(v), line });
                } else if text.len() > 1 && text.starts_with('0') {
                    let v = u32::from_str_radix(text, 8).map_err(|_| err(line, "bad octal"))?;
                    out.push(Token { tok: Tok::Int(v as i32), line });
                } else {
                    // Out-of-range decimal literals wrap, as the original compiler accepts them.
                    let v = text.parse::<u64>().map_err(|_| err(line, "bad integer"))?;
                    out.push(Token { tok: Tok::Int(v as u32 as i32), line });
                }
            }
            c if c.is_ascii_alphabetic() || c == b'_' => {
                let start = i;
                while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                    i += 1;
                }
                out.push(Token { tok: Tok::Ident(src[start..i].into()), line });
            }
            _ => {
                let two = b.get(i + 1).copied();
                let (tok, n) = match (c, two) {
                    (b'=', Some(b'=')) => (Tok::Eq, 2),
                    (b'!', Some(b'=')) => (Tok::Ne, 2),
                    (b'<', Some(b'=')) => (Tok::Le, 2),
                    (b'>', Some(b'=')) => (Tok::Ge, 2),
                    (b'=', _) => (Tok::Assign, 1),
                    (b'<', _) => (Tok::Lt, 1),
                    (b'>', _) => (Tok::Gt, 1),
                    (b'(', _) => (Tok::LParen, 1),
                    (b')', _) => (Tok::RParen, 1),
                    (b'[', _) => (Tok::LBracket, 1),
                    (b']', _) => (Tok::RBracket, 1),
                    (b',', _) => (Tok::Comma, 1),
                    (b'+', _) => (Tok::Plus, 1),
                    (b'-', _) => (Tok::Minus, 1),
                    (b'*', _) => (Tok::Star, 1),
                    (b'/', _) => (Tok::Slash, 1),
                    _ => {
                        // Stray bytes (e.g. Latin-1 text outside strings) are skipped.
                        i += 1;
                        continue;
                    }
                };
                out.push(Token { tok, line });
                i += n;
            }
        }
    }
    out.push(Token { tok: Tok::Newline, line });
    out.push(Token { tok: Tok::Eof, line });
    Ok(out)
}
