//! Syntax tree for JASS programs.

use std::rc::Rc;

pub type Name = Rc<str>;

#[derive(Clone, Debug)]
pub enum Expr {
    Int(i32),
    Real(f32),
    Str(Rc<str>),
    Bool(bool),
    Null,
    Var(Name),
    Index(Name, Box<Expr>),
    Call(Name, Vec<Expr>),
    FuncRef(Name),
    Neg(Box<Expr>),
    Not(Box<Expr>),
    Bin(BinOp, Box<Expr>, Box<Expr>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}

#[derive(Clone, Debug)]
pub enum Stmt {
    Set(Name, Option<Expr>, Expr, u32),
    Call(Name, Vec<Expr>, u32),
    If(Vec<(Expr, Vec<Stmt>)>, Vec<Stmt>),
    Loop(Vec<Stmt>),
    ExitWhen(Expr),
    Return(Option<Expr>, u32),
}

#[derive(Clone, Debug)]
pub struct VarDecl {
    pub ty: Name,
    pub name: Name,
    pub array: bool,
    pub constant: bool,
    pub init: Option<Expr>,
}

#[derive(Clone, Debug)]
pub struct Signature {
    pub name: Name,
    pub params: Vec<(Name, Name)>,
    pub ret: Option<Name>,
}

#[derive(Clone, Debug)]
pub struct Function {
    pub sig: Signature,
    pub locals: Vec<VarDecl>,
    pub body: Vec<Stmt>,
}

#[derive(Clone, Debug, Default)]
pub struct Program {
    /// `type child extends parent`
    pub types: Vec<(Name, Name)>,
    pub globals: Vec<VarDecl>,
    pub natives: Vec<Signature>,
    pub functions: Vec<Function>,
}
