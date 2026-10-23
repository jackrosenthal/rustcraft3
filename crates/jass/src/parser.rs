//! Recursive-descent parser producing [`ast::Program`].

use crate::ast::*;
use crate::lexer::{SyntaxError, Tok, Token};

pub struct Parser<'a> {
    toks: &'a [Token],
    pos: usize,
}

type R<T> = Result<T, SyntaxError>;

impl<'a> Parser<'a> {
    pub fn new(toks: &'a [Token]) -> Self {
        Parser { toks, pos: 0 }
    }

    fn peek(&self) -> &Tok {
        &self.toks[self.pos].tok
    }
    fn line(&self) -> u32 {
        self.toks[self.pos].line
    }
    fn next(&mut self) -> Tok {
        let t = self.toks[self.pos].tok.clone();
        if self.pos + 1 < self.toks.len() {
            self.pos += 1;
        }
        t
    }
    fn err<T>(&self, msg: impl Into<String>) -> R<T> {
        Err(SyntaxError { line: self.line(), msg: msg.into() })
    }
    fn is_kw(&self, kw: &str) -> bool {
        matches!(self.peek(), Tok::Ident(s) if &**s == kw)
    }
    fn eat_kw(&mut self, kw: &str) -> bool {
        if self.is_kw(kw) {
            self.next();
            true
        } else {
            false
        }
    }
    fn expect_kw(&mut self, kw: &str) -> R<()> {
        if self.eat_kw(kw) { Ok(()) } else { self.err(format!("expected '{kw}'")) }
    }
    fn expect(&mut self, t: Tok) -> R<()> {
        if *self.peek() == t {
            self.next();
            Ok(())
        } else {
            self.err(format!("expected {t:?}, found {:?}", self.peek()))
        }
    }
    fn ident(&mut self) -> R<Name> {
        match self.next() {
            Tok::Ident(s) => Ok(s),
            t => self.err(format!("expected identifier, found {t:?}")),
        }
    }
    fn skip_newlines(&mut self) {
        while *self.peek() == Tok::Newline {
            self.next();
        }
    }
    fn end_line(&mut self) -> R<()> {
        match self.peek() {
            Tok::Newline => {
                self.skip_newlines();
                Ok(())
            }
            Tok::Eof => Ok(()),
            t => self.err(format!("expected end of line, found {t:?}")),
        }
    }

    pub fn program(mut self, prog: &mut Program) -> R<()> {
        self.skip_newlines();
        while *self.peek() != Tok::Eof {
            if self.eat_kw("type") {
                let name = self.ident()?;
                self.expect_kw("extends")?;
                let parent = self.ident()?;
                prog.types.push((name, parent));
                self.end_line()?;
            } else if self.eat_kw("globals") {
                self.end_line()?;
                while !self.eat_kw("endglobals") {
                    let constant = self.eat_kw("constant");
                    let mut d = self.var_decl()?;
                    d.constant = constant;
                    prog.globals.push(d);
                    self.end_line()?;
                }
                self.end_line()?;
            } else {
                let _constant = self.eat_kw("constant");
                if self.eat_kw("native") {
                    prog.natives.push(self.signature()?);
                    self.end_line()?;
                } else if self.eat_kw("function") {
                    let f = self.function()?;
                    prog.functions.push(f);
                } else {
                    return self.err(format!("unexpected {:?} at top level", self.peek()));
                }
            }
        }
        Ok(())
    }

    fn var_decl(&mut self) -> R<VarDecl> {
        let ty = self.ident()?;
        let array = self.eat_kw("array");
        let name = self.ident()?;
        let init = if *self.peek() == Tok::Assign {
            self.next();
            Some(self.expr()?)
        } else {
            None
        };
        Ok(VarDecl { ty, name, array, constant: false, init })
    }

    fn signature(&mut self) -> R<Signature> {
        let name = self.ident()?;
        self.expect_kw("takes")?;
        let mut params = Vec::new();
        if !self.eat_kw("nothing") {
            loop {
                let ty = self.ident()?;
                let n = self.ident()?;
                params.push((n, ty));
                if *self.peek() != Tok::Comma {
                    break;
                }
                self.next();
            }
        }
        self.expect_kw("returns")?;
        let ret = if self.eat_kw("nothing") { None } else { Some(self.ident()?) };
        Ok(Signature { name, params, ret })
    }

    fn function(&mut self) -> R<Function> {
        let sig = self.signature()?;
        self.end_line()?;
        let mut locals = Vec::new();
        while self.eat_kw("local") {
            locals.push(self.var_decl()?);
            self.end_line()?;
        }
        let body = self.block(&["endfunction"])?;
        self.expect_kw("endfunction")?;
        self.end_line()?;
        Ok(Function { sig, locals, body })
    }

    fn block(&mut self, terminators: &[&str]) -> R<Vec<Stmt>> {
        let mut out = Vec::new();
        loop {
            if terminators.iter().any(|t| self.is_kw(t)) {
                return Ok(out);
            }
            if *self.peek() == Tok::Eof {
                return self.err("unexpected end of file");
            }
            out.push(self.stmt()?);
        }
    }

    fn stmt(&mut self) -> R<Stmt> {
        let line = self.line();
        self.eat_kw("debug");
        let s = if self.eat_kw("set") {
            let name = self.ident()?;
            let idx = if *self.peek() == Tok::LBracket {
                self.next();
                let e = self.expr()?;
                self.expect(Tok::RBracket)?;
                Some(e)
            } else {
                None
            };
            self.expect(Tok::Assign)?;
            Stmt::Set(name, idx, self.expr()?, line)
        } else if self.eat_kw("call") {
            let name = self.ident()?;
            let args = self.args()?;
            Stmt::Call(name, args, line)
        } else if self.eat_kw("if") {
            let mut arms = Vec::new();
            let cond = self.expr()?;
            self.expect_kw("then")?;
            self.end_line()?;
            let body = self.block(&["elseif", "else", "endif"])?;
            arms.push((cond, body));
            let mut else_body = Vec::new();
            loop {
                if self.eat_kw("elseif") {
                    let cond = self.expr()?;
                    self.expect_kw("then")?;
                    self.end_line()?;
                    arms.push((cond, self.block(&["elseif", "else", "endif"])?));
                } else if self.eat_kw("else") {
                    self.end_line()?;
                    else_body = self.block(&["endif"])?;
                } else {
                    self.expect_kw("endif")?;
                    break;
                }
            }
            Stmt::If(arms, else_body)
        } else if self.eat_kw("loop") {
            self.end_line()?;
            let body = self.block(&["endloop"])?;
            self.expect_kw("endloop")?;
            Stmt::Loop(body)
        } else if self.eat_kw("exitwhen") {
            Stmt::ExitWhen(self.expr()?)
        } else if self.eat_kw("return") {
            let e = if matches!(self.peek(), Tok::Newline) { None } else { Some(self.expr()?) };
            Stmt::Return(e, line)
        } else {
            return self.err(format!("unexpected {:?} in statement", self.peek()));
        };
        self.end_line()?;
        Ok(s)
    }

    fn args(&mut self) -> R<Vec<Expr>> {
        self.expect(Tok::LParen)?;
        let mut args = Vec::new();
        if *self.peek() != Tok::RParen {
            loop {
                args.push(self.expr()?);
                if *self.peek() != Tok::Comma {
                    break;
                }
                self.next();
            }
        }
        self.expect(Tok::RParen)?;
        Ok(args)
    }

    pub fn expr(&mut self) -> R<Expr> {
        self.or_expr()
    }

    fn or_expr(&mut self) -> R<Expr> {
        let mut l = self.and_expr()?;
        while self.eat_kw("or") {
            let r = self.and_expr()?;
            l = Expr::Bin(BinOp::Or, Box::new(l), Box::new(r));
        }
        Ok(l)
    }

    fn and_expr(&mut self) -> R<Expr> {
        let mut l = self.cmp_expr()?;
        while self.eat_kw("and") {
            let r = self.cmp_expr()?;
            l = Expr::Bin(BinOp::And, Box::new(l), Box::new(r));
        }
        Ok(l)
    }

    fn cmp_expr(&mut self) -> R<Expr> {
        let mut l = self.add_expr()?;
        loop {
            let op = match self.peek() {
                Tok::Eq => BinOp::Eq,
                Tok::Ne => BinOp::Ne,
                Tok::Lt => BinOp::Lt,
                Tok::Le => BinOp::Le,
                Tok::Gt => BinOp::Gt,
                Tok::Ge => BinOp::Ge,
                _ => return Ok(l),
            };
            self.next();
            let r = self.add_expr()?;
            l = Expr::Bin(op, Box::new(l), Box::new(r));
        }
    }

    fn add_expr(&mut self) -> R<Expr> {
        let mut l = self.mul_expr()?;
        loop {
            let op = match self.peek() {
                Tok::Plus => BinOp::Add,
                Tok::Minus => BinOp::Sub,
                _ => return Ok(l),
            };
            self.next();
            let r = self.mul_expr()?;
            l = Expr::Bin(op, Box::new(l), Box::new(r));
        }
    }

    fn mul_expr(&mut self) -> R<Expr> {
        let mut l = self.unary()?;
        loop {
            let op = match self.peek() {
                Tok::Star => BinOp::Mul,
                Tok::Slash => BinOp::Div,
                _ => return Ok(l),
            };
            self.next();
            let r = self.unary()?;
            l = Expr::Bin(op, Box::new(l), Box::new(r));
        }
    }

    fn unary(&mut self) -> R<Expr> {
        match self.peek() {
            Tok::Minus => {
                self.next();
                Ok(match self.unary()? {
                    Expr::Int(v) => Expr::Int(v.wrapping_neg()),
                    Expr::Real(v) => Expr::Real(-v),
                    e => Expr::Neg(Box::new(e)),
                })
            }
            Tok::Plus => {
                self.next();
                self.unary()
            }
            Tok::Ident(s) if &**s == "not" => {
                self.next();
                Ok(Expr::Not(Box::new(self.unary()?)))
            }
            _ => self.primary(),
        }
    }

    fn primary(&mut self) -> R<Expr> {
        match self.next() {
            Tok::Int(v) => Ok(Expr::Int(v)),
            Tok::Real(v) => Ok(Expr::Real(v)),
            Tok::Str(s) => Ok(Expr::Str(s)),
            Tok::LParen => {
                let e = self.expr()?;
                self.expect(Tok::RParen)?;
                Ok(e)
            }
            Tok::Ident(s) => match &*s {
                "true" => Ok(Expr::Bool(true)),
                "false" => Ok(Expr::Bool(false)),
                "null" => Ok(Expr::Null),
                "function" => Ok(Expr::FuncRef(self.ident()?)),
                _ => match self.peek() {
                    Tok::LParen => Ok(Expr::Call(s, self.args()?)),
                    Tok::LBracket => {
                        self.next();
                        let i = self.expr()?;
                        self.expect(Tok::RBracket)?;
                        Ok(Expr::Index(s, Box::new(i)))
                    }
                    _ => Ok(Expr::Var(s)),
                },
            },
            t => self.err(format!("unexpected {t:?} in expression")),
        }
    }
}
