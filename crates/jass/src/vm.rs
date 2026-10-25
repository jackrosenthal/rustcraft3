//! Bytecode interpreter.  Threads keep their own call stacks so a native such as
//! `TriggerSleepAction` can suspend one and the host can resume it later.

use std::rc::Rc;

use crate::compile::{Module, Op};
use crate::value::{BaseType, FuncId, Value};

/// Size of a JASS array as allocated by the game.
pub const ARRAY_SIZE: usize = 8192;

/// What a native call asks the interpreter to do next.
pub enum NativeResult {
    Return(Value),
    /// Suspend the calling thread for this many game seconds; the native's return value
    /// (always nothing for the sleeping natives) is supplied on resume.
    Sleep(f32),
    /// Abort the calling thread, as the game does on a fatal script error.
    Abort(String),
}

pub trait Host {
    fn call_native(&mut self, vm: &mut Vm, native: u32, args: &[Value]) -> NativeResult;
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct Frame {
    func: FuncId,
    pc: usize,
    base: usize,
    locals: Vec<Value>,
    arrays: Vec<Vec<Value>>,
}

/// A suspendable JASS thread.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Thread {
    frames: Vec<Frame>,
    stack: Vec<Value>,
    /// Instructions executed since the thread last started or resumed.
    pub ops: u32,
    /// Whether this thread may sleep (trigger actions may; conditions, callbacks may not).
    pub can_sleep: bool,
}

impl Thread {
    /// Every handle the thread's locals, local arrays, and stack hold.
    pub fn handles(&self, out: &mut impl FnMut(u32)) {
        let mut take = |v: &Value| {
            if let Value::Handle(h) = v {
                out(*h);
            }
        };
        for f in &self.frames {
            f.locals.iter().for_each(&mut take);
            f.arrays.iter().flatten().for_each(&mut take);
        }
        self.stack.iter().for_each(&mut take);
    }

    /// The functions on the thread's call stack, outermost first, each with the source
    /// line it is at.
    pub fn backtrace(&self, module: &Module) -> Vec<String> {
        self.frames
            .iter()
            .map(|f| {
                let func = &module.functions[f.func as usize];
                let line = func.lines.get(f.pc.saturating_sub(1)).copied().unwrap_or(0);
                format!("{}:{line}", func.name)
            })
            .collect()
    }
}

#[derive(Debug)]
pub enum Outcome {
    Finished(Value),
    Sleeping(f32),
    Aborted(String),
}

impl Vm {
    /// Every handle the globals and global arrays hold.
    pub fn global_handles(&self, out: &mut impl FnMut(u32)) {
        for v in self.globals.iter().chain(self.global_arrays.iter().flatten()) {
            if let Value::Handle(h) = v {
                out(*h);
            }
        }
    }
}

pub struct Vm {
    pub module: Module,
    pub globals: Vec<Value>,
    pub global_arrays: Vec<Vec<Value>>,
    /// Instruction budget per thread run; exceeding it ends the thread silently.
    pub op_limit: u32,
}

impl Vm {
    pub fn new(module: Module) -> Self {
        let globals = module
            .globals
            .iter()
            .map(|g| if g.array { Value::Null } else { Value::Uninit })
            .collect();
        let global_arrays = module
            .globals
            .iter()
            .map(|g| if g.array { vec![g.base.default_value(); 0] } else { Vec::new() })
            .collect();
        Vm { module, globals, global_arrays, op_limit: 300_000 }
    }

    /// The variables a saved game keeps: globals and global arrays.
    pub fn variables(&self) -> (Vec<Value>, Vec<Vec<Value>>) {
        (self.globals.clone(), self.global_arrays.clone())
    }

    /// Restores variables from [`Vm::variables`] of the same script.
    pub fn set_variables(&mut self, (globals, arrays): (Vec<Value>, Vec<Vec<Value>>)) {
        if globals.len() == self.globals.len() && arrays.len() == self.global_arrays.len() {
            self.globals = globals;
            self.global_arrays = arrays;
        }
    }

    /// Runs every source file's global initializers.
    pub fn init_globals(&mut self, host: &mut dyn Host) -> Result<(), String> {
        for f in self.module.global_inits.clone() {
            if let Outcome::Aborted(e) = self.call(host, f, &[]) {
                return Err(e);
            }
        }
        Ok(())
    }

    pub fn new_thread(&self, func: FuncId, args: &[Value], can_sleep: bool) -> Thread {
        let f = &self.module.functions[func as usize];
        let mut locals: Vec<Value> = vec![Value::Uninit; f.locals.len()];
        for (i, a) in args.iter().enumerate().take(f.params.len()) {
            locals[i] = coerce(a.clone(), f.params[i] == crate::value::BaseType::Real);
        }
        let arrays = f.arrays.iter().map(|_| Vec::new()).collect();
        Thread {
            frames: vec![Frame { func, pc: 0, base: 0, locals, arrays }],
            stack: Vec::new(),
            ops: 0,
            can_sleep,
        }
    }

    /// Calls a function to completion on a fresh thread that may not sleep; used for
    /// conditions, filters, and enumeration callbacks.
    pub fn call(&mut self, host: &mut dyn Host, func: FuncId, args: &[Value]) -> Outcome {
        let mut t = self.new_thread(func, args, false);
        self.run(host, &mut t)
    }

    /// Runs (or resumes) a thread until it finishes, sleeps, or aborts.
    pub fn run(&mut self, host: &mut dyn Host, t: &mut Thread) -> Outcome {
        t.ops = 0;
        loop {
            let depth = t.frames.len();
            let Some(frame) = t.frames.last_mut() else {
                return Outcome::Finished(t.stack.pop().unwrap_or(Value::Null));
            };
            let func = &self.module.functions[frame.func as usize];
            let op = func.code[frame.pc];
            frame.pc += 1;
            t.ops += 1;
            if t.ops > self.op_limit {
                return Outcome::Aborted("operation limit reached".into());
            }
            macro_rules! abort {
                ($($a:tt)*) => {{
                    let line = func.lines.get(frame.pc - 1).copied().unwrap_or(0);
                    return Outcome::Aborted(format!("{} line {}: {}", func.name, line, format!($($a)*)));
                }};
            }
            match op {
                Op::Const(c) => t.stack.push(self.module.constants[c as usize].clone()),
                Op::LoadLocal(s) => {
                    let v = frame.locals[s as usize].clone();
                    if v == Value::Uninit {
                        abort!("read of uninitialized variable");
                    }
                    t.stack.push(v);
                }
                Op::StoreLocal(s) => frame.locals[s as usize] = t.stack.pop().unwrap(),
                Op::LoadGlobal(g) => {
                    let v = self.globals[g as usize].clone();
                    if v == Value::Uninit {
                        abort!("read of uninitialized global {}", self.module.globals[g as usize].name);
                    }
                    t.stack.push(v);
                }
                Op::StoreGlobal(g) => self.globals[g as usize] = t.stack.pop().unwrap(),
                Op::LoadLocalArray(s) => {
                    let i = t.stack.pop().unwrap().int();
                    if !(0..ARRAY_SIZE as i32).contains(&i) {
                        abort!("array index {i} out of bounds");
                    }
                    let ty = func.arrays[s as usize];
                    let arr = &frame.arrays[s as usize];
                    t.stack.push(arr.get(i as usize).cloned().unwrap_or_else(|| ty.default_value()));
                }
                Op::StoreLocalArray(s) => {
                    let v = t.stack.pop().unwrap();
                    let i = t.stack.pop().unwrap().int();
                    if !(0..ARRAY_SIZE as i32).contains(&i) {
                        abort!("array index {i} out of bounds");
                    }
                    let ty = func.arrays[s as usize];
                    store(&mut frame.arrays[s as usize], i as usize, v, ty.default_value());
                }
                Op::LoadGlobalArray(g) => {
                    let i = t.stack.pop().unwrap().int();
                    if !(0..ARRAY_SIZE as i32).contains(&i) {
                        abort!("array index {i} out of bounds");
                    }
                    let ty = self.module.globals[g as usize].base;
                    let arr = &self.global_arrays[g as usize];
                    t.stack.push(arr.get(i as usize).cloned().unwrap_or_else(|| ty.default_value()));
                }
                Op::StoreGlobalArray(g) => {
                    let v = t.stack.pop().unwrap();
                    let i = t.stack.pop().unwrap().int();
                    if !(0..ARRAY_SIZE as i32).contains(&i) {
                        abort!("array index {i} out of bounds");
                    }
                    let d = self.module.globals[g as usize].base.default_value();
                    store(&mut self.global_arrays[g as usize], i as usize, v, d);
                }
                Op::IntToReal => {
                    let v = t.stack.pop().unwrap();
                    t.stack.push(coerce(v, true));
                }
                Op::Add | Op::Sub | Op::Mul | Op::Div => {
                    let r = t.stack.pop().unwrap();
                    let l = t.stack.pop().unwrap();
                    let v = match (&l, &r) {
                        (Value::Int(a), Value::Int(b)) => Value::Int(match op {
                            Op::Add => a.wrapping_add(*b),
                            Op::Sub => a.wrapping_sub(*b),
                            Op::Mul => a.wrapping_mul(*b),
                            _ => {
                                if *b == 0 {
                                    abort!("integer division by zero");
                                }
                                a.wrapping_div(*b)
                            }
                        }),
                        (Value::Int(_) | Value::Real(_), Value::Int(_) | Value::Real(_)) => {
                            // In the original's software floats (softfloat.rs).
                            use crate::softfloat as sf;
                            let (a, b) = (l.real().to_bits(), r.real().to_bits());
                            Value::Real(f32::from_bits(match op {
                                Op::Add => sf::add(a, b),
                                Op::Sub => sf::sub(a, b),
                                Op::Mul => sf::mul(a, b),
                                _ => {
                                    if b & 0x7fff_ffff == 0 {
                                        abort!("real division by zero");
                                    }
                                    sf::div(a, b)
                                }
                            }))
                        }
                        (Value::Str(_) | Value::Null, Value::Str(_) | Value::Null) if op == Op::Add => {
                            match (l.str(), r.str()) {
                                (None, None) => Value::Null,
                                (a, b) => {
                                    let mut s = String::from(a.unwrap_or(""));
                                    s.push_str(b.unwrap_or(""));
                                    Value::Str(Rc::from(s))
                                }
                            }
                        }
                        _ => abort!("bad operands {l:?} and {r:?}"),
                    };
                    t.stack.push(v);
                }
                Op::Eq | Op::Ne => {
                    let r = t.stack.pop().unwrap();
                    let l = t.stack.pop().unwrap();
                    // A real `==` holds within 0.001, while `!=` stays exact (measured in
                    // the original: 1.0 == 1.001 and 1.0 != 1.0001 are both true).
                    let eq = match (op, &l, &r) {
                        (Op::Eq, Value::Real(_) | Value::Int(_), Value::Real(_)) | (Op::Eq, Value::Real(_), Value::Int(_)) => {
                            let f = |v: &Value| match v { Value::Real(x) => *x, Value::Int(i) => *i as f32, _ => 0.0 };
                            // A float step past 0.001 allowed for literals rounded up.
                            (f(&l) - f(&r)).abs() <= 0.001 + 1e-7
                        }
                        _ => values_equal(&l, &r),
                    };
                    t.stack.push(Value::Bool(if op == Op::Eq { eq } else { !eq }));
                }
                Op::Lt | Op::Le | Op::Gt | Op::Ge => {
                    let r = t.stack.pop().unwrap();
                    let l = t.stack.pop().unwrap();
                    let b = match (&l, &r) {
                        (Value::Int(a), Value::Int(b)) => match op {
                            Op::Lt => a < b,
                            Op::Le => a <= b,
                            Op::Gt => a > b,
                            _ => a >= b,
                        },
                        _ => {
                            let (a, b) = (l.real(), r.real());
                            match op {
                                Op::Lt => a < b,
                                Op::Le => a <= b,
                                Op::Gt => a > b,
                                _ => a >= b,
                            }
                        }
                    };
                    t.stack.push(Value::Bool(b));
                }
                Op::Neg => {
                    let v = match t.stack.pop().unwrap() {
                        Value::Int(i) => Value::Int(i.wrapping_neg()),
                        Value::Real(r) => Value::Real(-r),
                        v => abort!("cannot negate {v:?}"),
                    };
                    t.stack.push(v);
                }
                Op::Not => {
                    let v = t.stack.pop().unwrap().bool();
                    t.stack.push(Value::Bool(!v));
                }
                Op::Jump(to) => frame.pc = to as usize,
                Op::JumpIfFalse(to) => {
                    if !t.stack.pop().unwrap().bool() {
                        frame.pc = to as usize;
                    }
                }
                Op::AndJump(to) => {
                    if !t.stack.last().unwrap().bool() {
                        frame.pc = to as usize;
                    } else {
                        t.stack.pop();
                    }
                }
                Op::OrJump(to) => {
                    if t.stack.last().unwrap().bool() {
                        frame.pc = to as usize;
                    } else {
                        t.stack.pop();
                    }
                }
                Op::Call(f, argc) => {
                    if depth > 4096 {
                        abort!("call stack overflow");
                    }
                    let callee = &self.module.functions[f as usize];
                    let base = t.stack.len() - argc as usize;
                    let mut locals = vec![Value::Uninit; callee.locals.len()];
                    for (i, a) in t.stack.drain(base..).enumerate() {
                        locals[i] = a;
                    }
                    let arrays = callee.arrays.iter().map(|_| Vec::new()).collect();
                    t.frames.push(Frame { func: f, pc: 0, base, locals, arrays });
                }
                Op::Native(n, argc) => {
                    let base = t.stack.len() - argc as usize;
                    let args: Vec<Value> = t.stack.drain(base..).collect();
                    if args.iter().any(|a| *a == Value::Uninit) {
                        abort!("uninitialized argument");
                    }
                    match host.call_native(self, n, &args) {
                        NativeResult::Return(v) => {
                            // Hosts may return enumeration handles as plain integers.
                            let v = match (self.module.natives[n as usize].ret_base, v) {
                                (Some(BaseType::Handle), Value::Int(i)) => Value::Handle(i as u32),
                                (Some(BaseType::Integer), Value::Handle(h)) => Value::Int(h as i32),
                                (Some(BaseType::Real), Value::Int(i)) => Value::Real(i as f32),
                                (_, v) => v,
                            };
                            t.stack.push(v)
                        }
                        NativeResult::Sleep(secs) => {
                            t.stack.push(Value::Null);
                            if t.can_sleep {
                                return Outcome::Sleeping(secs);
                            }
                            // A thread that may not sleep (a timer's callback, an
                            // enumeration's, a filter) ends there, as the original's
                            // does (measured: a timer callback's text after its sleep
                            // never shows).
                            return Outcome::Finished(Value::Null);
                        }
                        NativeResult::Abort(e) => return Outcome::Aborted(e),
                    }
                }
                Op::Pop => {
                    t.stack.pop();
                }
                Op::Return => {
                    let v = t.stack.pop().unwrap_or(Value::Null);
                    let fr = t.frames.pop().unwrap();
                    t.stack.truncate(fr.base);
                    t.stack.push(v);
                }
            }
        }
    }

    pub fn global(&self, name: &str) -> Option<&Value> {
        self.module.global_index.get(name).map(|&i| &self.globals[i as usize])
    }

    pub fn set_global(&mut self, name: &str, v: Value) -> bool {
        match self.module.global_index.get(name) {
            Some(&i) => {
                self.globals[i as usize] = v;
                true
            }
            None => false,
        }
    }
}

fn store(arr: &mut Vec<Value>, i: usize, v: Value, default: Value) {
    if arr.len() <= i {
        arr.resize(i + 1, default);
    }
    arr[i] = v;
}

fn coerce(v: Value, to_real: bool) -> Value {
    match v {
        Value::Int(i) if to_real => Value::Real(i as f32),
        v => v,
    }
}

pub fn values_equal(l: &Value, r: &Value) -> bool {
    match (l, r) {
        (Value::Int(a), Value::Real(b)) | (Value::Real(b), Value::Int(a)) => *a as f32 == *b,
        (Value::Null, Value::Handle(0)) | (Value::Handle(0), Value::Null) => true,
        (Value::Str(a), Value::Str(b)) => a == b,
        (a, b) => a == b,
    }
}
