//! Lowers parsed JASS into stack-machine bytecode.

use std::collections::HashMap;
use std::rc::Rc;

use crate::ast::*;
use crate::value::{BaseType, FuncId, Value};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Op {
    Const(u32),
    LoadLocal(u16),
    StoreLocal(u16),
    LoadGlobal(u32),
    StoreGlobal(u32),
    /// Index on the stack.
    LoadLocalArray(u16),
    /// Index then value on the stack.
    StoreLocalArray(u16),
    LoadGlobalArray(u32),
    StoreGlobalArray(u32),
    IntToReal,
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
    Neg,
    Not,
    Jump(u32),
    JumpIfFalse(u32),
    /// If the top of stack is false, jump leaving it; otherwise pop it.
    AndJump(u32),
    /// If the top of stack is true, jump leaving it; otherwise pop it.
    OrJump(u32),
    Call(FuncId, u8),
    Native(u32, u8),
    Pop,
    Return,
}

#[derive(Debug, Clone)]
pub struct CompiledFunction {
    pub name: Name,
    pub params: Vec<BaseType>,
    pub ret: Option<BaseType>,
    /// Scalar local slots, parameters first.
    pub locals: Vec<BaseType>,
    /// Element types of local arrays.
    pub arrays: Vec<BaseType>,
    pub code: Vec<Op>,
    pub lines: Vec<u32>,
}

#[derive(Debug, Clone)]
pub struct NativeDecl {
    pub name: Name,
    pub params: Vec<Name>,
    pub ret: Option<Name>,
}

#[derive(Debug, Clone)]
pub struct GlobalDecl {
    pub name: Name,
    pub ty: Name,
    pub base: BaseType,
    pub array: bool,
    pub constant: bool,
}

#[derive(Debug, thiserror::Error)]
#[error("{func}: line {line}: {msg}")]
pub struct CompileError {
    pub func: String,
    pub line: u32,
    pub msg: String,
}

/// A whole compiled program: every source file linked together.
#[derive(Default, Debug)]
pub struct Module {
    pub types: HashMap<Name, Name>,
    pub globals: Vec<GlobalDecl>,
    pub global_index: HashMap<Name, u32>,
    pub natives: Vec<NativeDecl>,
    pub native_index: HashMap<Name, u32>,
    pub functions: Vec<CompiledFunction>,
    pub function_index: HashMap<Name, FuncId>,
    pub constants: Vec<Value>,
    /// Functions that initialize each source file's globals, in order.
    pub global_inits: Vec<FuncId>,
}

impl Module {
    pub fn base_type(&self, ty: &str) -> BaseType {
        let mut t = ty;
        for _ in 0..64 {
            match t {
                "integer" => return BaseType::Integer,
                "real" => return BaseType::Real,
                "boolean" => return BaseType::Boolean,
                "string" => return BaseType::String,
                "code" => return BaseType::Code,
                "handle" => return BaseType::Handle,
                _ => match self.types.get(t) {
                    Some(p) => t = p,
                    None => return BaseType::Handle,
                },
            }
        }
        BaseType::Handle
    }

    /// Whether `ty` is `ancestor` or derives from it.
    pub fn extends(&self, ty: &str, ancestor: &str) -> bool {
        let mut t = ty;
        for _ in 0..64 {
            if t == ancestor {
                return true;
            }
            match self.types.get(t) {
                Some(p) => t = p,
                None => return false,
            }
        }
        false
    }

    pub fn function(&self, name: &str) -> Option<FuncId> {
        self.function_index.get(name).copied()
    }

    /// Adds a parsed source file.  Files are linked in the order given, as the game
    /// loads common.j, Blizzard.j, then the map script.
    pub fn add(&mut self, prog: &Program) -> Result<(), CompileError> {
        for (t, p) in &prog.types {
            self.types.insert(t.clone(), p.clone());
        }
        for n in &prog.natives {
            let id = self.natives.len() as u32;
            self.natives.push(NativeDecl {
                name: n.name.clone(),
                params: n.params.iter().map(|p| p.1.clone()).collect(),
                ret: n.ret.clone(),
            });
            self.native_index.insert(n.name.clone(), id);
        }
        let first_global = self.globals.len();
        for g in &prog.globals {
            let id = self.globals.len() as u32;
            self.globals.push(GlobalDecl {
                name: g.name.clone(),
                ty: g.ty.clone(),
                base: self.base_type(&g.ty),
                array: g.array,
                constant: g.constant,
            });
            self.global_index.insert(g.name.clone(), id);
        }
        // Declare every function first so forward references and `function X` resolve.
        let first_fn = self.functions.len();
        for f in &prog.functions {
            let id = self.functions.len() as FuncId;
            let params: Vec<BaseType> = f.sig.params.iter().map(|p| self.base_type(&p.1)).collect();
            self.functions.push(CompiledFunction {
                name: f.sig.name.clone(),
                params: params.clone(),
                ret: f.sig.ret.as_deref().map(|t| self.base_type(t)),
                locals: params,
                arrays: Vec::new(),
                code: Vec::new(),
                lines: Vec::new(),
            });
            self.function_index.insert(f.sig.name.clone(), id);
        }
        // Global initializers become a synthetic function.
        let init_id = self.functions.len() as FuncId;
        let mut init = FnCompiler::new(self, "<globals>".into(), None);
        for (i, g) in prog.globals.iter().enumerate() {
            if let Some(e) = &g.init {
                let gid = (first_global + i) as u32;
                let base = init.m.globals[gid as usize].base;
                init.expr_as(e, base, 0)?;
                init.emit(Op::StoreGlobal(gid), 0);
            }
        }
        let null = init.constant(Value::Null);
        init.emit(Op::Const(null), 0);
        init.emit(Op::Return, 0);
        let (code, lines, consts) = (init.code, init.lines, init.new_consts);
        self.constants.extend(consts);
        self.functions.push(CompiledFunction {
            name: "<globals>".into(),
            params: vec![],
            ret: None,
            locals: vec![],
            arrays: vec![],
            code,
            lines,
        });
        self.global_inits.push(init_id);

        for (i, f) in prog.functions.iter().enumerate() {
            let id = first_fn + i;
            let ret = self.functions[id].ret;
            let mut c = FnCompiler::new(self, f.sig.name.clone(), ret);
            for (n, p) in &f.sig.params {
                let b = c.m.base_type(p);
                c.declare_local(n, b, false);
            }
            for l in &f.locals {
                let b = c.m.base_type(&l.ty);
                let slot = c.declare_local(&l.name, b, l.array);
                if let Some(e) = &l.init {
                    c.expr_as(e, b, 0)?;
                    c.emit(Op::StoreLocal(slot), 0);
                }
            }
            c.block(&f.body)?;
            if ret.is_some() {
                // Falling off the end of a function that returns a value yields its default.
                let v = c.constant(ret.unwrap().default_value());
                c.emit(Op::Const(v), 0);
            } else {
                let v = c.constant(Value::Null);
                c.emit(Op::Const(v), 0);
            }
            c.emit(Op::Return, 0);
            let FnCompiler { code, lines, local_types, array_types, new_consts, .. } = c;
            self.constants.extend(new_consts);
            let cf = &mut self.functions[id];
            cf.code = code;
            cf.lines = lines;
            cf.locals = local_types;
            cf.arrays = array_types;
        }
        Ok(())
    }
}

struct FnCompiler<'m> {
    m: &'m Module,
    name: Name,
    ret: Option<BaseType>,
    code: Vec<Op>,
    lines: Vec<u32>,
    locals: HashMap<Name, (u16, bool)>,
    local_types: Vec<BaseType>,
    array_types: Vec<BaseType>,
    new_consts: Vec<Value>,
    loop_exits: Vec<Vec<usize>>,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Ty {
    Known(BaseType),
    Null,
}

impl<'m> FnCompiler<'m> {
    fn new(m: &'m Module, name: Name, ret: Option<BaseType>) -> Self {
        FnCompiler {
            m,
            name,
            ret,
            code: Vec::new(),
            lines: Vec::new(),
            locals: HashMap::new(),
            local_types: Vec::new(),
            array_types: Vec::new(),
            new_consts: Vec::new(),
            loop_exits: Vec::new(),
        }
    }

    fn err<T>(&self, line: u32, msg: impl Into<String>) -> Result<T, CompileError> {
        Err(CompileError { func: self.name.to_string(), line, msg: msg.into() })
    }

    fn emit(&mut self, op: Op, line: u32) -> usize {
        self.code.push(op);
        self.lines.push(line);
        self.code.len() - 1
    }

    fn constant(&mut self, v: Value) -> u32 {
        let id = self.m.constants.len() + self.new_consts.len();
        self.new_consts.push(v);
        id as u32
    }

    fn declare_local(&mut self, name: &Name, ty: BaseType, array: bool) -> u16 {
        let slot = if array {
            self.array_types.push(ty);
            self.array_types.len() - 1
        } else {
            self.local_types.push(ty);
            self.local_types.len() - 1
        } as u16;
        self.locals.insert(name.clone(), (slot, array));
        slot
    }

    fn patch(&mut self, at: usize, target: usize) {
        let t = target as u32;
        self.code[at] = match self.code[at] {
            Op::Jump(_) => Op::Jump(t),
            Op::JumpIfFalse(_) => Op::JumpIfFalse(t),
            Op::AndJump(_) => Op::AndJump(t),
            Op::OrJump(_) => Op::OrJump(t),
            op => op,
        };
    }

    fn block(&mut self, stmts: &[Stmt]) -> Result<(), CompileError> {
        for s in stmts {
            self.stmt(s)?;
        }
        Ok(())
    }

    fn stmt(&mut self, s: &Stmt) -> Result<(), CompileError> {
        match s {
            Stmt::Set(name, idx, e, line) => {
                let line = *line;
                if let Some(&(slot, array)) = self.locals.get(name) {
                    if array {
                        let ty = self.array_types[slot as usize];
                        let Some(i) = idx else { return self.err(line, "array needs index") };
                        self.expr_as(i, BaseType::Integer, line)?;
                        self.expr_as(e, ty, line)?;
                        self.emit(Op::StoreLocalArray(slot), line);
                    } else {
                        let ty = self.local_types[slot as usize];
                        self.expr_as(e, ty, line)?;
                        self.emit(Op::StoreLocal(slot), line);
                    }
                } else if let Some(&g) = self.m.global_index.get(name) {
                    let decl = &self.m.globals[g as usize];
                    let (ty, array) = (decl.base, decl.array);
                    if array {
                        let Some(i) = idx else { return self.err(line, "array needs index") };
                        self.expr_as(i, BaseType::Integer, line)?;
                        self.expr_as(e, ty, line)?;
                        self.emit(Op::StoreGlobalArray(g), line);
                    } else {
                        self.expr_as(e, ty, line)?;
                        self.emit(Op::StoreGlobal(g), line);
                    }
                } else {
                    return self.err(line, format!("undeclared variable {name}"));
                }
            }
            Stmt::Call(name, args, line) => {
                self.call(name, args, *line)?;
                self.emit(Op::Pop, *line);
            }
            Stmt::If(arms, else_body) => {
                let mut ends = Vec::new();
                for (cond, body) in arms {
                    self.expr_as(cond, BaseType::Boolean, 0)?;
                    let jf = self.emit(Op::JumpIfFalse(0), 0);
                    self.block(body)?;
                    ends.push(self.emit(Op::Jump(0), 0));
                    let here = self.code.len();
                    self.patch(jf, here);
                }
                self.block(else_body)?;
                let here = self.code.len();
                for e in ends {
                    self.patch(e, here);
                }
            }
            Stmt::Loop(body) => {
                let top = self.code.len();
                self.loop_exits.push(Vec::new());
                self.block(body)?;
                self.emit(Op::Jump(top as u32), 0);
                let here = self.code.len();
                for e in self.loop_exits.pop().unwrap() {
                    self.patch(e, here);
                }
            }
            Stmt::ExitWhen(cond) => {
                // exitwhen c  ==>  if c then jump to loop end
                self.expr_as(cond, BaseType::Boolean, 0)?;
                let skip = self.emit(Op::JumpIfFalse(0), 0);
                let j = self.emit(Op::Jump(0), 0);
                match self.loop_exits.last_mut() {
                    Some(v) => v.push(j),
                    None => return self.err(0, "exitwhen outside loop"),
                }
                let here = self.code.len();
                self.patch(skip, here);
            }
            Stmt::Return(e, line) => {
                match (e, self.ret) {
                    (Some(e), Some(rt)) => self.expr_as(e, rt, *line)?,
                    (Some(e), None) => {
                        self.expr(e, *line)?;
                    }
                    (None, rt) => {
                        let v = self.constant(rt.map_or(Value::Null, BaseType::default_value));
                        self.emit(Op::Const(v), *line);
                    }
                }
                self.emit(Op::Return, *line);
            }
        }
        Ok(())
    }

    /// Compiles `e` and converts integers to reals when the destination is real.
    fn expr_as(&mut self, e: &Expr, want: BaseType, line: u32) -> Result<(), CompileError> {
        let t = self.expr(e, line)?;
        if want == BaseType::Real && t == Ty::Known(BaseType::Integer) {
            self.emit(Op::IntToReal, line);
        }
        Ok(())
    }

    fn call(&mut self, name: &Name, args: &[Expr], line: u32) -> Result<Ty, CompileError> {
        if let Some(&f) = self.m.function_index.get(name) {
            let params = self.m.functions[f as usize].params.clone();
            if params.len() != args.len() {
                return self.err(line, format!("{name} takes {} arguments", params.len()));
            }
            for (a, p) in args.iter().zip(&params) {
                self.expr_as(a, *p, line)?;
            }
            self.emit(Op::Call(f, args.len() as u8), line);
            Ok(self.m.functions[f as usize].ret.map_or(Ty::Null, Ty::Known))
        } else if let Some(&n) = self.m.native_index.get(name) {
            let decl = &self.m.natives[n as usize];
            if decl.params.len() != args.len() {
                return self.err(line, format!("{name} takes {} arguments", decl.params.len()));
            }
            let params: Vec<BaseType> = decl.params.iter().map(|p| self.m.base_type(p)).collect();
            let ret = decl.ret.as_deref().map(|r| self.m.base_type(r));
            for (a, p) in args.iter().zip(&params) {
                self.expr_as(a, *p, line)?;
            }
            self.emit(Op::Native(n, args.len() as u8), line);
            Ok(ret.map_or(Ty::Null, Ty::Known))
        } else {
            self.err(line, format!("undeclared function {name}"))
        }
    }

    fn expr(&mut self, e: &Expr, line: u32) -> Result<Ty, CompileError> {
        use BaseType::*;
        Ok(match e {
            Expr::Int(v) => {
                let c = self.constant(Value::Int(*v));
                self.emit(Op::Const(c), line);
                Ty::Known(Integer)
            }
            Expr::Real(v) => {
                let c = self.constant(Value::Real(*v));
                self.emit(Op::Const(c), line);
                Ty::Known(Real)
            }
            Expr::Str(s) => {
                let c = self.constant(Value::Str(Rc::clone(s)));
                self.emit(Op::Const(c), line);
                Ty::Known(String)
            }
            Expr::Bool(b) => {
                let c = self.constant(Value::Bool(*b));
                self.emit(Op::Const(c), line);
                Ty::Known(Boolean)
            }
            Expr::Null => {
                let c = self.constant(Value::Null);
                self.emit(Op::Const(c), line);
                Ty::Null
            }
            Expr::FuncRef(name) => {
                let Some(&f) = self.m.function_index.get(name) else {
                    return self.err(line, format!("undeclared function {name}"));
                };
                let c = self.constant(Value::Code(f));
                self.emit(Op::Const(c), line);
                Ty::Known(Code)
            }
            Expr::Var(name) => {
                if let Some(&(slot, array)) = self.locals.get(name) {
                    if array {
                        return self.err(line, format!("{name} is an array"));
                    }
                    self.emit(Op::LoadLocal(slot), line);
                    Ty::Known(self.local_types[slot as usize])
                } else if let Some(&g) = self.m.global_index.get(name) {
                    self.emit(Op::LoadGlobal(g), line);
                    Ty::Known(self.m.globals[g as usize].base)
                } else {
                    return self.err(line, format!("undeclared variable {name}"));
                }
            }
            Expr::Index(name, i) => {
                self.expr_as(i, Integer, line)?;
                if let Some(&(slot, true)) = self.locals.get(name) {
                    self.emit(Op::LoadLocalArray(slot), line);
                    Ty::Known(self.array_types[slot as usize])
                } else if let Some(&g) = self.m.global_index.get(name) {
                    self.emit(Op::LoadGlobalArray(g), line);
                    Ty::Known(self.m.globals[g as usize].base)
                } else {
                    return self.err(line, format!("undeclared array {name}"));
                }
            }
            Expr::Call(name, args) => self.call(name, args, line)?,
            Expr::Neg(e) => {
                let t = self.expr(e, line)?;
                self.emit(Op::Neg, line);
                t
            }
            Expr::Not(e) => {
                self.expr(e, line)?;
                self.emit(Op::Not, line);
                Ty::Known(Boolean)
            }
            Expr::Bin(op @ (BinOp::And | BinOp::Or), l, r) => {
                self.expr(l, line)?;
                let j = self.emit(if *op == BinOp::And { Op::AndJump(0) } else { Op::OrJump(0) }, line);
                self.expr(r, line)?;
                let here = self.code.len();
                self.patch(j, here);
                Ty::Known(Boolean)
            }
            Expr::Bin(op, l, r) => {
                let lt = self.expr(l, line)?;
                let rt = self.expr(r, line)?;
                self.emit(
                    match op {
                        BinOp::Add => Op::Add,
                        BinOp::Sub => Op::Sub,
                        BinOp::Mul => Op::Mul,
                        BinOp::Div => Op::Div,
                        BinOp::Eq => Op::Eq,
                        BinOp::Ne => Op::Ne,
                        BinOp::Lt => Op::Lt,
                        BinOp::Le => Op::Le,
                        BinOp::Gt => Op::Gt,
                        BinOp::Ge => Op::Ge,
                        BinOp::And | BinOp::Or => unreachable!(),
                    },
                    line,
                );
                match op {
                    BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div => {
                        if lt == Ty::Known(Real) || rt == Ty::Known(Real) {
                            Ty::Known(Real)
                        } else {
                            lt
                        }
                    }
                    _ => Ty::Known(Boolean),
                }
            }
        })
    }
}
