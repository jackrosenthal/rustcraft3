//! Frame definition (`.fdf`) files: the interface layout language.

use std::rc::Rc;

#[derive(Clone, Debug, PartialEq)]
pub enum Arg {
    Ident(String),
    Str(String),
    Num(f32),
}

impl Arg {
    pub fn as_str(&self) -> &str {
        match self {
            Arg::Ident(s) | Arg::Str(s) => s,
            Arg::Num(_) => "",
        }
    }
    pub fn num(&self) -> f32 {
        match self {
            Arg::Num(n) => *n,
            // Numbers may carry a C float suffix ("-0.002f").
            Arg::Ident(s) | Arg::Str(s) => s.trim_end_matches(['f', 'F']).parse().unwrap_or(0.0),
        }
    }
}

/// One `Key arg, arg, ...` property line.
#[derive(Clone, Debug)]
pub struct Prop {
    pub key: String,
    pub args: Vec<Arg>,
}

/// A `Frame`, `Texture`, or `String` block.
#[derive(Clone, Debug)]
pub struct Def {
    /// "Frame", "Texture", "String", "Layer".
    pub block: String,
    /// Frame type (e.g. "BACKDROP"); empty for texture and string blocks.
    pub kind: String,
    pub name: String,
    pub inherits: Option<String>,
    pub with_children: bool,
    pub props: Vec<Prop>,
    pub children: Vec<Rc<Def>>,
}

impl Def {
    pub fn prop(&self, key: &str) -> Option<&Prop> {
        self.props.iter().rev().find(|p| p.key.eq_ignore_ascii_case(key))
    }
    pub fn has(&self, key: &str) -> bool {
        self.prop(key).is_some()
    }
    pub fn str(&self, key: &str) -> Option<&str> {
        self.prop(key).and_then(|p| p.args.first()).map(Arg::as_str)
    }
    pub fn num(&self, key: &str) -> Option<f32> {
        self.prop(key).and_then(|p| p.args.first()).map(Arg::num)
    }
}

#[derive(Clone, Debug, Default)]
pub struct File {
    pub includes: Vec<String>,
    pub strings: Vec<(String, String)>,
    pub defs: Vec<Rc<Def>>,
}

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Ident(String),
    Str(String),
    Num(f32),
    Comma,
    Open,
    Close,
}

fn tokenize(src: &str) -> Vec<Tok> {
    let b = src.as_bytes();
    let mut i = 0;
    let mut out = Vec::new();
    while i < b.len() {
        let c = b[i];
        if c.is_ascii_whitespace() {
            i += 1;
        } else if c == b'/' && b.get(i + 1) == Some(&b'/') {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
        } else if c == b'/' && b.get(i + 1) == Some(&b'*') {
            i += 2;
            while i + 1 < b.len() && !(b[i] == b'*' && b[i + 1] == b'/') {
                i += 1;
            }
            i += 2;
        } else if c == b'"' {
            i += 1;
            let s = i;
            while i < b.len() && b[i] != b'"' {
                i += 1;
            }
            out.push(Tok::Str(String::from_utf8_lossy(&b[s..i.min(b.len())]).into_owned()));
            i += 1;
        } else if c == b',' || c == b';' {
            out.push(Tok::Comma);
            i += 1;
        } else if c == b'{' {
            out.push(Tok::Open);
            i += 1;
        } else if c == b'}' {
            out.push(Tok::Close);
            i += 1;
        } else {
            let s = i;
            while i < b.len() && !b[i].is_ascii_whitespace() && !b"{},;\"".contains(&b[i]) {
                i += 1;
            }
            let t = String::from_utf8_lossy(&b[s..i]).into_owned();
            let numeric = t.starts_with(|ch: char| ch.is_ascii_digit() || ch == '-' || ch == '.');
            match t.trim_end_matches('f').parse::<f32>() {
                Ok(n) if numeric => out.push(Tok::Num(n)),
                _ => out.push(Tok::Ident(t)),
            }
        }
    }
    out
}

struct Parser {
    toks: Vec<Tok>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos)
    }
    fn next(&mut self) -> Option<Tok> {
        let t = self.toks.get(self.pos).cloned();
        self.pos += 1;
        t
    }

    fn file(&mut self) -> File {
        let mut f = File::default();
        while let Some(t) = self.next() {
            let Tok::Ident(word) = t else { continue };
            match word.as_str() {
                "IncludeFile" => {
                    if let Some(Tok::Str(s)) = self.next() {
                        f.includes.push(s);
                    }
                }
                "StringList" => {
                    if self.peek() == Some(&Tok::Open) {
                        self.pos += 1;
                    }
                    loop {
                        match self.next() {
                            Some(Tok::Close) | None => break,
                            Some(Tok::Ident(k)) => {
                                if let Some(Tok::Str(v)) = self.peek().cloned() {
                                    self.pos += 1;
                                    f.strings.push((k, v));
                                }
                            }
                            _ => {}
                        }
                    }
                }
                "Frame" | "Texture" | "String" | "Layer" => f.defs.push(Rc::new(self.def(word))),
                _ => {}
            }
        }
        f
    }

    /// Parses a block after its keyword: `Frame "TYPE" "Name" [INHERITS [WITHCHILDREN] "Base"] { ... }`.
    fn def(&mut self, block: String) -> Def {
        let mut strs = Vec::new();
        let mut inherits = None;
        let mut with_children = false;
        loop {
            match self.peek().cloned() {
                Some(Tok::Str(s)) => {
                    self.pos += 1;
                    strs.push(s);
                }
                Some(Tok::Ident(w)) if w == "INHERITS" => {
                    self.pos += 1;
                    if let Some(Tok::Ident(w2)) = self.peek().cloned() {
                        if w2 == "WITHCHILDREN" {
                            with_children = true;
                            self.pos += 1;
                        }
                    }
                    if let Some(Tok::Str(s)) = self.next() {
                        inherits = Some(s);
                    }
                }
                Some(Tok::Ident(_)) => {
                    // Unquoted layer names, e.g. `Layer "ARTWORK"` written bare.
                    if let Some(Tok::Ident(s)) = self.next() {
                        strs.push(s);
                    }
                }
                _ => break,
            }
        }
        let (kind, name) = match (block.as_str(), strs.len()) {
            ("Frame", n) if n >= 2 => (strs[0].clone(), strs[1].clone()),
            ("Frame", 1) => (strs[0].clone(), String::new()),
            (_, 0) => (String::new(), String::new()),
            (_, _) => (String::new(), strs[0].clone()),
        };
        let mut d = Def { block, kind, name, inherits, with_children, props: Vec::new(), children: Vec::new() };
        if self.peek() != Some(&Tok::Open) {
            return d;
        }
        self.pos += 1;
        loop {
            match self.next() {
                None | Some(Tok::Close) => break,
                Some(Tok::Ident(key)) => {
                    if matches!(key.as_str(), "Frame" | "Texture" | "String" | "Layer")
                        && matches!(self.peek(), Some(Tok::Str(_)) | Some(Tok::Open))
                        || matches!(key.as_str(), "Frame" | "Texture" | "String" | "Layer")
                            && matches!(self.peek(), Some(Tok::Ident(w)) if w == "INHERITS")
                    {
                        let child = self.def(key);
                        if child.block == "Layer" {
                            // Layers only group a simple frame's textures and strings.
                            d.children.extend(child.children);
                        } else {
                            d.children.push(Rc::new(child));
                        }
                        continue;
                    }
                    let mut args = Vec::new();
                    loop {
                        match self.peek().cloned() {
                            Some(Tok::Comma) => {
                                self.pos += 1;
                                // Commas separate arguments and also end a property; the next
                                // property starts with a CamelCase keyword, while arguments are
                                // numbers, strings, or upper-case names.
                                match self.peek() {
                                    Some(Tok::Num(_)) | Some(Tok::Str(_)) => continue,
                                    Some(Tok::Ident(w)) if is_arg_ident(w) => continue,
                                    _ => break,
                                }
                            }
                            Some(Tok::Num(n)) => {
                                self.pos += 1;
                                args.push(Arg::Num(n));
                            }
                            Some(Tok::Str(s)) => {
                                self.pos += 1;
                                args.push(Arg::Str(s));
                            }
                            Some(Tok::Ident(w)) if is_arg_ident(&w) => {
                                self.pos += 1;
                                args.push(Arg::Ident(w));
                            }
                            _ => break,
                        }
                    }
                    d.props.push(Prop { key, args });
                }
                _ => {}
            }
        }
        d
    }
}

/// Identifiers that appear as property arguments: anchor points, justifications, flags.
fn is_arg_ident(w: &str) -> bool {
    w.len() > 1 && w.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_' || c == '|')
}

pub fn parse(src: &str) -> File {
    Parser { toks: tokenize(src), pos: 0 }.file()
}
