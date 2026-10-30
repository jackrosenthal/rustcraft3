//! JASS compiled as the original compiles it (Jass2's Compile.cpp, `FUN_6f9146f0`), into
//! the code a saved game keeps (`w3_formats::w3z::Code`): 8-byte instructions (three
//! register bytes, an opcode, a 32-bit argument) and a string table, read in saves'
//! code blocks and the original's VM (`FUN_6f91b460`).
//!
//! As read: each file in turn gives its types (`extends` the parent, `type` the
//! child), its globals in a function `<init>` (each a `global` or `constant` with its
//! base type, its initializer, and a store), then its functions; `endprogram` closes
//! the program.  A value takes the next register of a counter running through the
//! program (1 to 255, then 1 again; 0 holds results); labels are numbered through the
//! program from 1.  Names and string literals (as written, escapes kept) take string
//! ids in the order the source first has them, "main" first; the built-in type names
//! are left out; after them come the strings the code first uses that the source did
//! not have as such ("handle", "<init>", and a literal's value with its escapes
//! resolved), in the order used.

use std::collections::HashMap;

use crate::ast::*;

/// A compiled program: its words (two to an instruction) and strings (from id 1).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Code {
    pub words: Vec<u32>,
    pub strings: Vec<String>,
}

pub mod op {
    pub const ENDPROGRAM: u8 = 1;
    pub const OLDJUMP: u8 = 2;
    pub const FUNCTION: u8 = 3;
    pub const ENDFUNCTION: u8 = 4;
    pub const LOCAL: u8 = 5;
    pub const GLOBAL: u8 = 6;
    pub const CONSTANT: u8 = 7;
    pub const FUNCARG: u8 = 8;
    pub const EXTENDS: u8 = 9;
    pub const TYPE: u8 = 0xa;
    pub const POPN: u8 = 0xb;
    pub const LITERAL: u8 = 0xc;
    pub const MOVRR: u8 = 0xd;
    pub const MOVRV: u8 = 0xe;
    pub const CODE: u8 = 0xf;
    pub const MOVRA: u8 = 0x10;
    pub const MOVVR: u8 = 0x11;
    pub const MOVAR: u8 = 0x12;
    pub const PUSH: u8 = 0x13;
    pub const POP: u8 = 0x14;
    pub const NATIVE: u8 = 0x15;
    pub const CALL: u8 = 0x16;
    pub const I2R: u8 = 0x17;
    pub const AND: u8 = 0x18;
    pub const OR: u8 = 0x19;
    pub const EQ: u8 = 0x1a;
    pub const NE: u8 = 0x1b;
    pub const LE: u8 = 0x1c;
    pub const GE: u8 = 0x1d;
    pub const LT: u8 = 0x1e;
    pub const GT: u8 = 0x1f;
    pub const ADD: u8 = 0x20;
    pub const SUB: u8 = 0x21;
    pub const MUL: u8 = 0x22;
    pub const DIV: u8 = 0x23;
    pub const NEG: u8 = 0x25;
    pub const NOT: u8 = 0x26;
    pub const RETURN: u8 = 0x27;
    pub const LABEL: u8 = 0x28;
    pub const JUMPIFTRUE: u8 = 0x29;
    pub const JUMPIFFALSE: u8 = 0x2a;
    pub const JUMP: u8 = 0x2b;
}

/// The VM's types: 0 nothing, 2 null, 3 code, 4 integer, 5 real, 6 string, 7 handle,
/// 8 boolean; an array five more than its element's.
type Ty = u8;
const NOTHING: Ty = 0;
const NULL: Ty = 2;
const CODE: Ty = 3;
const INTEGER: Ty = 4;
const REAL: Ty = 5;
const STRING: Ty = 6;
const HANDLE: Ty = 7;
const BOOLEAN: Ty = 8;

fn base(name: &str) -> Ty {
    match name {
        "nothing" => NOTHING,
        "code" => CODE,
        "integer" => INTEGER,
        "real" => REAL,
        "string" => STRING,
        "boolean" => BOOLEAN,
        _ => HANDLE,
    }
}

const KEYWORDS: &[&str] = &[
    "function", "takes", "returns", "endfunction", "nothing", "local", "set", "call", "if", "then", "else", "elseif", "endif", "loop", "endloop", "exitwhen",
    "return", "constant", "native", "type", "extends", "globals", "endglobals", "array", "and", "or", "not", "true", "false", "null", "debug", "integer", "real",
    "string", "boolean", "code", "handle",
];

/// The names and string literals of a source in order, string literals as written.
fn source_names(src: &str, out: &mut Vec<(String, Option<String>)>) {
    let b = src.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c == b'/' && b.get(i + 1) == Some(&b'/') {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
        } else if c == b'"' {
            let start = i + 1;
            i += 1;
            while i < b.len() && b[i] != b'"' {
                if b[i] == b'\\' {
                    i += 1;
                }
                i += 1;
            }
            let raw = String::from_utf8_lossy(&b[start..i.min(b.len())]).into_owned();
            out.push((raw.clone(), Some(unescape(&raw))));
            i += 1;
        } else if c == b'\'' {
            i += 1;
            while i < b.len() && b[i] != b'\'' {
                if b[i] == b'\\' {
                    i += 1;
                }
                i += 1;
            }
            i += 1;
        } else if c.is_ascii_alphabetic() || c == b'_' {
            let start = i;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                i += 1;
            }
            let w = &src[start..i];
            if !KEYWORDS.contains(&w) {
                out.push((w.to_string(), None));
            }
        } else if c.is_ascii_digit() || c == b'$' {
            // Numbers, hex ones too (0x1F, $1F).
            i += 1;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'.') {
                i += 1;
            }
        } else {
            i += 1;
        }
    }
}

/// A string literal's value, as the port's lexer reads it.
fn unescape(raw: &str) -> String {
    let b = raw.as_bytes();
    let mut s = Vec::new();
    let mut i = 0;
    while i < b.len() {
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
            s.push(b[i]);
        }
        i += 1;
    }
    String::from_utf8_lossy(&s).into_owned()
}

#[derive(Default)]
struct Gen {
    words: Vec<u32>,
    strings: Vec<String>,
    ids: HashMap<String, u32>,
    reg: u8,
    label: u32,
    globals: HashMap<Name, (Ty, bool)>,
    funcs: HashMap<Name, (Ty, Vec<Ty>, bool)>,
    locals: HashMap<Name, (Ty, bool)>,
    ret: Ty,
    /// The innermost loop's exit label.
    exit: Vec<u32>,
}

impl Gen {
    fn id(&mut self, name: &str) -> u32 {
        if let Some(&i) = self.ids.get(name) {
            return i;
        }
        self.strings.push(name.to_string());
        let i = self.strings.len() as u32;
        self.ids.insert(name.to_string(), i);
        i
    }

    fn emit(&mut self, b0: u8, b1: u8, b2: u8, op: u8, arg: u32) {
        self.words.push(u32::from_le_bytes([b0, b1, b2, op]));
        self.words.push(arg);
    }

    fn alloc(&mut self) -> u8 {
        self.reg = if self.reg == 255 { 1 } else { self.reg + 1 };
        self.reg
    }

    fn new_label(&mut self) -> u32 {
        self.label += 1;
        self.label
    }

    fn var(&self, name: &str) -> (Ty, bool) {
        self.locals.get(name).or_else(|| self.globals.get(name)).copied().unwrap_or((HANDLE, false))
    }

    fn ty_of(t: &Name) -> Ty {
        base(t)
    }

    /// An expression's value in a register (0 for a call's), and its type; `want` the
    /// type it is wanted as (an integer for a real is converted).
    fn expr(&mut self, e: &Expr, want: Option<Ty>) -> (u8, Ty) {
        let (r, t) = match e {
            Expr::Int(v) => {
                let r = self.alloc();
                self.emit(0, INTEGER, r, op::LITERAL, *v as u32);
                (r, INTEGER)
            }
            Expr::Real(v) => {
                let r = self.alloc();
                self.emit(0, REAL, r, op::LITERAL, v.to_bits());
                (r, REAL)
            }
            Expr::Str(s) => {
                // The value, escapes resolved (one with escapes taken after the source's
                // names, its text as written being among those).
                let id = self.id(s);
                let r = self.alloc();
                self.emit(0, STRING, r, op::LITERAL, id);
                (r, STRING)
            }
            Expr::Bool(v) => {
                let r = self.alloc();
                self.emit(0, BOOLEAN, r, op::LITERAL, *v as u32);
                (r, BOOLEAN)
            }
            Expr::Null => {
                let r = self.alloc();
                self.emit(0, NULL, r, op::LITERAL, 0);
                (r, NULL)
            }
            Expr::Var(n) => {
                let (t, _) = self.var(n);
                let id = self.id(n);
                let r = self.alloc();
                self.emit(0, t, r, op::MOVRV, id);
                (r, t)
            }
            Expr::Index(n, i) => {
                let (t, _) = self.var(n);
                let (ir, _) = self.expr(i, Some(INTEGER));
                let id = self.id(n);
                let r = self.alloc();
                self.emit(t, ir, r, op::MOVRA, id);
                (r, t)
            }
            Expr::Call(n, args) => {
                self.call(n, args);
                (0, self.funcs.get(n).map_or(NOTHING, |f| f.0))
            }
            Expr::FuncRef(n) => {
                let id = self.id(n);
                let r = self.alloc();
                self.emit(0, CODE, r, op::CODE, id);
                (r, CODE)
            }
            Expr::Neg(x) => {
                // Negated, then converted (read: `set scale = -1` for a real is 1, neg,
                // i2r).
                let (r, t) = self.expr(x, None);
                self.emit(0, 0, r, op::NEG, 0);
                (r, t)
            }
            Expr::Not(x) => {
                let (r, _) = self.expr(x, Some(BOOLEAN));
                self.emit(0, 0, r, op::NOT, 0);
                (r, BOOLEAN)
            }
            Expr::Paren(x) => return self.expr(x, want),
            Expr::Bin(o @ (BinOp::And | BinOp::Or), _, _) => {
                let mut operands = Vec::new();
                chain(e, *o, &mut operands);
                (self.short_circuit(*o, &operands), BOOLEAN)
            }
            Expr::Bin(o, l, rhs) => self.binary(*o, l, rhs),
        };
        if want == Some(REAL) && t == INTEGER {
            self.emit(0, 0, r, op::I2R, 0);
            return (r, REAL);
        }
        (r, t)
    }

    /// `and` and `or`, short-circuit, a chain of either nested to the right (read: `A and
    /// B and C` is A, a jump on false, then `B and C`, then each one's false into the
    /// last's register).
    fn short_circuit(&mut self, o: BinOp, operands: &[&Expr]) -> u8 {
        let (l, _) = self.expr(operands[0], Some(BOOLEAN));
        if operands.len() == 1 {
            return l;
        }
        let skip = self.new_label();
        self.emit(0, 0, l, if o == BinOp::And { op::JUMPIFFALSE } else { op::JUMPIFTRUE }, skip);
        let r = self.short_circuit(o, &operands[1..]);
        let end = self.new_label();
        self.emit(0, 0, 0, op::JUMP, end);
        self.emit(0, 0, 0, op::LABEL, skip);
        self.emit(0, BOOLEAN, r, op::LITERAL, (o == BinOp::Or) as u32);
        self.emit(0, 0, 0, op::LABEL, end);
        r
    }

    /// An expression's type, without compiling it.
    fn type_of(&self, e: &Expr) -> Ty {
        match e {
            Expr::Int(_) => INTEGER,
            Expr::Real(_) => REAL,
            Expr::Str(_) => STRING,
            Expr::Bool(_) | Expr::Not(_) => BOOLEAN,
            Expr::Null => NULL,
            Expr::Var(n) | Expr::Index(n, _) => self.var(n).0,
            Expr::Call(n, _) => self.funcs.get(n).map_or(NOTHING, |f| f.0),
            Expr::FuncRef(_) => CODE,
            Expr::Neg(x) | Expr::Paren(x) => self.type_of(x),
            Expr::Bin(o, l, r) => match o {
                BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge | BinOp::And | BinOp::Or => BOOLEAN,
                _ => match (self.type_of(l), self.type_of(r)) {
                    (REAL, _) | (_, REAL) => REAL,
                    (t, _) => t,
                },
            },
        }
    }

    fn binary(&mut self, o: BinOp, l: &Expr, rhs: &Expr) -> (u8, Ty) {
        // An integer beside a real becomes a real as it is computed (read: `2 *
        // bj_CELLWIDTH` is 2, i2r, push, ...); a null compared takes the other side's type.
        let (lt0, rt0) = (self.type_of(l), self.type_of(rhs));
        let mixed = (lt0 == REAL && rt0 == INTEGER) || (lt0 == INTEGER && rt0 == REAL);
        let (lr, lt) = self.expr(l, if mixed { Some(REAL) } else { None });
        self.emit(0, 0, lr, op::PUSH, 0);
        let want = match (o, lt) {
            (BinOp::Eq | BinOp::Ne, t) if t != INTEGER && t != REAL => Some(t),
            _ if mixed => Some(REAL),
            _ => None,
        };
        // A null compared takes the other side's type.
        let (rr, rt) = match (rhs, want) {
            (Expr::Null, Some(t)) if matches!(o, BinOp::Eq | BinOp::Ne) => {
                let r = self.alloc();
                self.emit(0, t, r, op::LITERAL, 0);
                (r, t)
            }
            _ => self.expr(rhs, want),
        };
        let p = self.alloc();
        self.emit(0, 0, p, op::POP, 0);
        let code = match o {
            BinOp::Add => op::ADD,
            BinOp::Sub => op::SUB,
            BinOp::Mul => op::MUL,
            BinOp::Div => op::DIV,
            BinOp::Eq => op::EQ,
            BinOp::Ne => op::NE,
            BinOp::Lt => op::LT,
            BinOp::Le => op::LE,
            BinOp::Gt => op::GT,
            BinOp::Ge => op::GE,
            BinOp::And => op::AND,
            BinOp::Or => op::OR,
        };
        self.emit(rr, p, p, code, 0);
        let t = match o {
            BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge | BinOp::And | BinOp::Or => BOOLEAN,
            _ if lt == REAL || rt == REAL => REAL,
            _ => lt,
        };
        (p, t)
    }

    fn call(&mut self, n: &Name, args: &[Expr]) {
        let (params, native) = self.funcs.get(n).map_or((Vec::new(), true), |f| (f.1.clone(), f.2));
        for (k, a) in args.iter().enumerate() {
            let (r, _) = self.expr(a, params.get(k).copied());
            self.emit(0, 0, r, op::PUSH, 0);
        }
        let id = self.id(n);
        self.emit(0, 0, 0, if native { op::NATIVE } else { op::CALL }, id);
        // A function's arguments come off the stack after it (a native takes its own).
        if !native && !args.is_empty() {
            self.emit(0, 0, args.len() as u8, op::POPN, 0);
        }
    }

    fn stmts(&mut self, body: &[Stmt]) {
        for s in body {
            self.stmt(s);
        }
    }

    fn stmt(&mut self, s: &Stmt) {
        match s {
            Stmt::Set(n, None, e, _) => {
                let (t, _) = self.var(n);
                let (r, _) = self.expr(e, Some(t));
                let id = self.id(n);
                self.emit(0, 0, r, op::MOVVR, id);
            }
            Stmt::Set(n, Some(i), e, _) => {
                let (t, _) = self.var(n);
                let (ir, _) = self.expr(i, Some(INTEGER));
                self.emit(0, 0, ir, op::PUSH, 0);
                let (vr, _) = self.expr(e, Some(t));
                let p = self.alloc();
                self.emit(0, 0, p, op::POP, 0);
                let id = self.id(n);
                self.emit(0, vr, p, op::MOVAR, id);
            }
            Stmt::Call(n, args, _) => self.call(n, args),
            Stmt::Debug(s) => {
                let past = self.new_label();
                self.emit(0, 0, 0, op::OLDJUMP, past);
                self.stmt(s);
                self.emit(0, 0, 0, op::LABEL, past);
            }
            Stmt::If(branches, otherwise) => self.if_chain(branches, otherwise),
            Stmt::Loop(body) => {
                let (start, end) = (self.new_label(), self.new_label());
                self.emit(0, 0, 0, op::LABEL, start);
                self.exit.push(end);
                self.stmts(body);
                self.exit.pop();
                self.emit(0, 0, 0, op::JUMP, start);
                self.emit(0, 0, 0, op::LABEL, end);
            }
            Stmt::ExitWhen(c) => {
                let (r, _) = self.expr(c, Some(BOOLEAN));
                let end = *self.exit.last().unwrap_or(&0);
                self.emit(0, 0, r, op::JUMPIFTRUE, end);
            }
            Stmt::Return(e, _) => {
                if let Some(e) = e {
                    let ret = self.ret;
                    let (r, _) = self.expr(e, Some(ret));
                    self.emit(0, r, 0, op::MOVRR, 0);
                }
                self.emit(0, 0, 0, op::RETURN, 0);
            }
        }
    }

    /// An if and its elseifs, each elseif an if within the else.
    fn if_chain(&mut self, branches: &[(Expr, Vec<Stmt>)], otherwise: &[Stmt]) {
        let Some(((cond, body), rest)) = branches.split_first() else {
            self.stmts(otherwise);
            return;
        };
        let (r, _) = self.expr(cond, Some(BOOLEAN));
        // An if with nothing in it is its condition alone (read: Blizzard.j's empty `if
        // (doPreload) then endif`).
        if body.is_empty() && rest.is_empty() && otherwise.is_empty() {
            return;
        }
        let (no, end) = (self.new_label(), self.new_label());
        self.emit(0, 0, r, op::JUMPIFFALSE, no);
        self.stmts(body);
        self.emit(0, 0, 0, op::JUMP, end);
        self.emit(0, 0, 0, op::LABEL, no);
        self.if_chain(rest, otherwise);
        self.emit(0, 0, 0, op::LABEL, end);
    }

    fn decl_ty(d: &VarDecl) -> Ty {
        Self::ty_of(&d.ty) + if d.array { 5 } else { 0 }
    }

    fn file(&mut self, p: &Program) {
        for (child, parent) in &p.types {
            let pid = self.id(parent);
            self.emit(0, 0, 0, op::EXTENDS, pid);
            let cid = self.id(child);
            self.emit(0, 0, 0, op::TYPE, cid);
        }
        for n in &p.natives {
            let ret = n.ret.as_ref().map_or(NOTHING, Self::ty_of);
            self.funcs.insert(n.name.clone(), (ret, n.params.iter().map(|(_, t)| Self::ty_of(t)).collect(), true));
        }
        if !p.globals.is_empty() {
            let id = self.id("<init>");
            self.emit(0, 0, 0, op::FUNCTION, id);
            for g in &p.globals {
                let t = Self::decl_ty(g);
                self.globals.insert(g.name.clone(), (Self::ty_of(&g.ty), g.array));
                let id = self.id(&g.name);
                self.emit(0, 0, t, if g.constant { op::CONSTANT } else { op::GLOBAL }, id);
                if let Some(e) = &g.init {
                    let (r, _) = self.expr(e, Some(Self::ty_of(&g.ty)));
                    self.emit(0, 0, r, op::MOVVR, id);
                }
            }
            self.emit(0, 0, 0, op::RETURN, 0);
            self.emit(0, 0, 0, op::ENDFUNCTION, 0);
        }
        for f in &p.functions {
            let ret = f.sig.ret.as_ref().map_or(NOTHING, Self::ty_of);
            self.funcs.insert(f.sig.name.clone(), (ret, f.sig.params.iter().map(|(_, t)| Self::ty_of(t)).collect(), false));
            let id = self.id(&f.sig.name);
            self.emit(0, 0, ret, op::FUNCTION, id);
            self.locals.clear();
            self.ret = ret;
            let n = f.sig.params.len();
            for (k, (name, t)) in f.sig.params.iter().enumerate() {
                let ty = Self::ty_of(t);
                self.locals.insert(name.clone(), (ty, false));
                let id = self.id(name);
                self.emit(0, (n - k) as u8, ty, op::FUNCARG, id);
            }
            for l in &f.locals {
                let t = Self::decl_ty(l);
                self.locals.insert(l.name.clone(), (Self::ty_of(&l.ty), l.array));
                let id = self.id(&l.name);
                self.emit(0, 0, t, op::LOCAL, id);
                if let Some(e) = &l.init {
                    let (r, _) = self.expr(e, Some(Self::ty_of(&l.ty)));
                    self.emit(0, 0, r, op::MOVVR, id);
                }
            }
            self.stmts(&f.body);
            self.emit(0, 0, 0, op::LITERAL, 0);
            self.emit(0, 0, 0, op::RETURN, 0);
            self.emit(0, 0, 0, op::ENDFUNCTION, 0);
        }
    }
}

/// The operands of a chain of one operator, as the port's parser nests it (to the left).
fn chain<'e>(e: &'e Expr, o: BinOp, out: &mut Vec<&'e Expr>) {
    match e {
        Expr::Bin(x, l, r) if *x == o => {
            chain(l, o, out);
            out.push(r);
        }
        e => out.push(e),
    }
}

/// Compiles source files in order (as common.j, Blizzard.j, and a map's war3map.j)
/// into the original's code.
pub fn compile<'a>(files: impl IntoIterator<Item = (&'a str, &'a str)>) -> Result<Code, crate::Error> {
    let files: Vec<(&str, &str)> = files.into_iter().collect();
    let mut g = Gen::default();
    g.id("main");
    for (_, src) in &files {
        let mut names = Vec::new();
        source_names(src, &mut names);
        for (raw, _) in names {
            g.id(&raw);
        }
    }
    for (name, src) in &files {
        let prog = crate::parse(src).map_err(|err| crate::Error::Syntax { file: name.to_string(), err })?;
        g.file(&prog);
    }
    g.emit(0, 0, 0, op::ENDPROGRAM, 0);
    Ok(Code { words: g.words, strings: g.strings })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A program in the shapes read in the original's code (Blizzard.j's RMinBJ, its
    /// globals' `<init>`).
    #[test]
    fn compiles_as_the_original() {
        let src = "type agent extends handle\nglobals\n    integer g = 0\nendglobals\nfunction F takes real a, real b returns real\n    if (a < b) then\n        return a\n    else\n        return b\n    endif\nendfunction\n";
        let c = compile([("t.j", src)]).unwrap();
        assert_eq!(c.strings, ["main", "agent", "g", "F", "a", "b", "handle", "<init>"]);
        let w = |b0: u8, b1: u8, b2: u8, op: u8, arg: u32| [u32::from_le_bytes([b0, b1, b2, op]), arg];
        let want: Vec<u32> = [
            w(0, 0, 0, op::EXTENDS, 7),
            w(0, 0, 0, op::TYPE, 2),
            w(0, 0, 0, op::FUNCTION, 8),
            w(0, 0, INTEGER, op::GLOBAL, 3),
            w(0, INTEGER, 1, op::LITERAL, 0),
            w(0, 0, 1, op::MOVVR, 3),
            w(0, 0, 0, op::RETURN, 0),
            w(0, 0, 0, op::ENDFUNCTION, 0),
            w(0, 0, REAL, op::FUNCTION, 4),
            w(0, 2, REAL, op::FUNCARG, 5),
            w(0, 1, REAL, op::FUNCARG, 6),
            w(0, REAL, 2, op::MOVRV, 5),
            w(0, 0, 2, op::PUSH, 0),
            w(0, REAL, 3, op::MOVRV, 6),
            w(0, 0, 4, op::POP, 0),
            w(3, 4, 4, op::LT, 0),
            w(0, 0, 4, op::JUMPIFFALSE, 1),
            w(0, REAL, 5, op::MOVRV, 5),
            w(0, 5, 0, op::MOVRR, 0),
            w(0, 0, 0, op::RETURN, 0),
            w(0, 0, 0, op::JUMP, 2),
            w(0, 0, 0, op::LABEL, 1),
            w(0, REAL, 6, op::MOVRV, 6),
            w(0, 6, 0, op::MOVRR, 0),
            w(0, 0, 0, op::RETURN, 0),
            w(0, 0, 0, op::LABEL, 2),
            w(0, 0, 0, op::LITERAL, 0),
            w(0, 0, 0, op::RETURN, 0),
            w(0, 0, 0, op::ENDFUNCTION, 0),
            w(0, 0, 0, op::ENDPROGRAM, 0),
        ]
        .concat();
        assert_eq!(c.words, want);
    }
}
