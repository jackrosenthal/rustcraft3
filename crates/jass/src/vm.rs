//! The original's JASS machine (Jass2's Instance.cpp; its interpreter `FUN_6f91b460`),
//! running the code [`crate::bytecode`] compiles word for word as the original does.
//!
//! As read: a context (the map script's machine, or a thread) has 256 registers, each a
//! type, the type it was given, and a word; a chain of frames, each with its locals by
//! name (`local` and `funcarg` make them as they run) and a stack of pushed registers; and
//! its place in the code.  The machine's globals, strings, and code references are
//! shared by its threads.  A call pushes its return place (a word offset, type 1) on the
//! caller's frame's stack and starts a frame; `funcarg` takes its argument from the
//! caller's stack below the return place; `return` frees the frame (releasing its
//! arguments, not its locals) and pops the return place, or, from a thread's first
//! frame, ends the run.  Every callback the engine runs (a condition, a filter, an
//! enumeration's or a timer's function, a trigger's action) runs on a new thread, its
//! first frame below a frame holding its return place; the machine's own context runs
//! the globals' initializers, `config`, and `main`.
//!
//! Strings are interned in the machine's table, each with a count: a literal, a
//! concatenation, or a native's string result counts one more (never less: registers
//! hold no counts); a variable set releases its old string and counts its new one; a
//! function's arguments are released as it returns, its locals never.  A count at zero
//! frees its place, the last freed taken first.  A string interned anew also joins the
//! code's own string table.

use std::collections::{BTreeSet, HashMap};
use std::rc::Rc;

use crate::bytecode::op;
use crate::compile::Module;
use crate::value::{BaseType, FuncId, Value};

/// Size of a JASS array as allocated by the game.
pub const ARRAY_SIZE: usize = 8192;

/// The machine's types: 1 a return place, 2 null, 3 code, 4 integer, 5 real, 6 string,
/// 7 handle, 8 boolean; an array five more than its element's.
pub mod ty {
    pub const NOTHING: u8 = 0;
    pub const RETURN: u8 = 1;
    pub const NULL: u8 = 2;
    pub const CODE: u8 = 3;
    pub const INTEGER: u8 = 4;
    pub const REAL: u8 = 5;
    pub const STRING: u8 = 6;
    pub const HANDLE: u8 = 7;
    pub const BOOLEAN: u8 = 8;
    pub const STRING_ARRAY: u8 = 11;
    pub const HANDLE_ARRAY: u8 = 12;
}

/// What a native call asks the interpreter to do next.
pub enum NativeResult {
    Return(Value),
    /// Suspend the calling thread for this many game seconds.
    Sleep(f32),
    /// Abort the calling thread, as the game does on a fatal script error.
    Abort(String),
}

pub trait Host {
    fn call_native(&mut self, vm: &mut Vm, native: u32, args: &[Value]) -> NativeResult;
}

/// A register, or a value on a frame's stack (`FUN_6f904e30`'s 0x28 bytes): its type
/// (+0x18), the type it was given (+0x1c), and its word (+0x20).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Reg {
    pub kind: u8,
    pub decl: u8,
    pub value: u32,
}

impl Reg {
    fn of(kind: u8, value: u32) -> Reg {
        Reg { kind, decl: kind, value }
    }
}

/// A variable: its type (0 until first set; an array's from its declaration), its
/// declared type, its word, an array's words once first set (at most 8192), and whether
/// it is a function's argument (+0x24).
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Var {
    pub kind: u8,
    pub decl: u8,
    pub value: u32,
    pub array: Option<Vec<u32>>,
    pub param: bool,
}

impl Var {
    fn declared(decl: u8) -> Var {
        Var { kind: if decl > 8 { decl } else { 0 }, decl, ..Default::default() }
    }
}

/// A frame (`AUJassFrame`): its function (the port's id, `NO_FUNC` for a thread's base),
/// its locals as made (by slot, its function's declarations in order), the registers
/// pushed on it, and whether it is a run's first (+0xb8: its `return` ends the run).
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Frame {
    pub func: FuncId,
    pub locals: Vec<Var>,
    pub stack: Vec<Reg>,
    pub top: bool,
}

const NO_FUNC: FuncId = u32::MAX;

impl Frame {
    fn new(func: FuncId, top: bool) -> Frame {
        Frame { func, locals: Vec::new(), stack: Vec::new(), top }
    }
}

/// A context: a thread, or the machine's own.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Thread {
    pub regs: Vec<Reg>,
    pub frames: Vec<Frame>,
    /// Its place (an instruction's index), none before it first runs.
    pub ip: Option<u32>,
    /// The places it was at as each run on it began (+0x28).
    pub returns: Vec<Option<u32>>,
    /// Instructions executed since the thread last started or resumed.
    pub ops: u32,
    /// Whether this thread may sleep (trigger actions may; conditions, callbacks may not).
    pub can_sleep: bool,
    /// A function to call as it first runs, with its arguments.
    entry: Option<(FuncId, Vec<Value>)>,
}

impl Default for Thread {
    fn default() -> Self {
        Thread { regs: vec![Reg::default(); 256], frames: Vec::new(), ip: None, returns: Vec::new(), ops: 0, can_sleep: false, entry: None }
    }
}

impl Thread {
    /// Every handle the thread's variables (its frames' arguments, locals, and local
    /// arrays) hold; the original counts a handle's references by its variables alone.
    pub fn handles(&self, out: &mut impl FnMut(u32)) {
        for f in &self.frames {
            for v in &f.locals {
                var_handles(v, out);
            }
        }
    }

    /// The functions on the thread's call stack, outermost first.
    pub fn backtrace(&self, module: &Module) -> Vec<String> {
        self.frames.iter().filter(|f| f.func != NO_FUNC).map(|f| module.functions.get(f.func as usize).map_or_else(|| format!("#{}", f.func), |x| x.name.to_string())).collect()
    }
}

fn var_handles(v: &Var, out: &mut impl FnMut(u32)) {
    if v.decl == ty::HANDLE && v.value != 0 {
        out(v.value);
    }
    if v.kind == ty::HANDLE_ARRAY {
        v.array.iter().flatten().filter(|&&h| h != 0).for_each(|&h| out(h));
    }
}

#[derive(Debug)]
pub enum Outcome {
    Finished(Value),
    Sleeping(f32),
    Aborted(String),
}

/// The machine's strings (+0x2874): each place's text and count, the first place
/// unused; freed places (+0x2878), the last freed taken first.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Strings {
    pub entries: Vec<(Option<Rc<str>>, u32)>,
    pub free: Vec<u32>,
    #[serde(skip)]
    index: HashMap<Rc<str>, u32>,
}

impl Default for Strings {
    fn default() -> Self {
        Strings { entries: vec![(None, 0)], free: Vec::new(), index: HashMap::new() }
    }
}

impl Strings {
    fn rebuild(&mut self) {
        self.index = self.entries.iter().enumerate().filter_map(|(k, (s, _))| s.clone().map(|s| (s, k as u32))).collect();
    }

    /// The place of `s`, counted once more (`FUN_6f91af80`); whether it is new.
    fn intern(&mut self, s: &str) -> (u32, bool) {
        if let Some(&k) = self.index.get(s) {
            self.entries[k as usize].1 += 1;
            return (k, false);
        }
        let s: Rc<str> = Rc::from(s);
        let k = match self.free.pop() {
            Some(k) => {
                self.entries[k as usize] = (Some(s.clone()), 1);
                k
            }
            None => {
                self.entries.push((Some(s.clone()), 1));
                self.entries.len() as u32 - 1
            }
        };
        self.index.insert(s, k);
        (k, true)
    }

    /// One more count (`FUN_6f917540`).
    fn add_ref(&mut self, k: u32) {
        if let Some(e) = self.entries.get_mut(k as usize).filter(|_| k != 0) {
            e.1 += 1;
        }
    }

    /// One fewer, freeing the place at none (`FUN_6f9163e0`).
    fn release(&mut self, k: u32) {
        if k == 0 {
            return;
        }
        let Some(e) = self.entries.get_mut(k as usize) else { return };
        e.1 = e.1.wrapping_sub(1);
        if e.1 == 0 {
            if let Some(s) = e.0.take() {
                self.index.remove(&s);
            }
            self.free.push(k);
        }
    }

    pub fn get(&self, k: u32) -> Option<&Rc<str>> {
        self.entries.get(k as usize).and_then(|e| e.0.as_ref())
    }
}

/// An instruction: three register bytes, its opcode, and its argument.
#[derive(Clone, Copy, Debug)]
struct Insn {
    a: u8,
    b: u8,
    c: u8,
    op: u8,
    arg: u32,
}

/// A variable's resolution: a local's slot, or a global's index.
const LOCAL: u32 = 0x8000_0000;
const UNRESOLVED: u32 = u32::MAX;

/// A native's signature: each argument's base type and its result's machine type.
#[derive(Clone, Debug)]
struct Sig {
    params: Vec<BaseType>,
    ret: u8,
}

/// A function as the code declares it: its name, entry, and result type.
#[derive(Clone, Debug)]
struct Func {
    name: Rc<str>,
    entry: u32,
    ret: u8,
    port: FuncId,
    locals: Vec<u32>,
}

/// The code, decoded and resolved.
#[derive(Debug, Default)]
struct Prog {
    code: Vec<Insn>,
    /// Each instruction's resolution: a variable's, a native's index, a function's, a
    /// jump's target.
    res: Vec<u32>,
    strings: Vec<Rc<str>>,
    funcs: Vec<Func>,
    /// Functions by entry, and by the port's id.
    at_entry: HashMap<u32, u32>,
    of_port: HashMap<FuncId, u32>,
    inits: Vec<u32>,
    sigs: Vec<Sig>,
}

fn machine_type(b: Option<BaseType>) -> u8 {
    match b {
        None => ty::NOTHING,
        Some(BaseType::Integer) => ty::INTEGER,
        Some(BaseType::Real) => ty::REAL,
        Some(BaseType::Boolean) => ty::BOOLEAN,
        Some(BaseType::String) => ty::STRING,
        Some(BaseType::Code) => ty::CODE,
        Some(BaseType::Handle) => ty::HANDLE,
    }
}

impl Prog {
    fn new(m: &Module) -> Prog {
        let c = &m.code;
        let strings: Vec<Rc<str>> = c.strings.iter().map(|s| Rc::from(s.as_str())).collect();
        let code: Vec<Insn> = c
            .words
            .chunks(2)
            .map(|w| {
                let [a, b, cc, o] = w[0].to_le_bytes();
                Insn { a, b, c: cc, op: o, arg: w[1] }
            })
            .collect();
        let name = |id: u32| strings.get((id as usize).wrapping_sub(1)).cloned().unwrap_or_else(|| Rc::from(""));
        // Labels' targets (`FUN_6f917a20`: the instruction after each label) and the
        // functions (each entered after its `function`).
        let mut labels: HashMap<u32, u32> = HashMap::new();
        let mut funcs: Vec<Func> = Vec::new();
        let mut owner: Vec<u32> = vec![u32::MAX; code.len()];
        let mut cur = u32::MAX;
        for (k, i) in code.iter().enumerate() {
            match i.op {
                op::FUNCTION => {
                    let n = name(i.arg);
                    let port = m.function_index.get(&*n).copied().unwrap_or(NO_FUNC);
                    funcs.push(Func { name: n, entry: k as u32 + 1, ret: i.c, port, locals: Vec::new() });
                    cur = funcs.len() as u32 - 1;
                }
                op::LABEL => {
                    labels.insert(i.arg, k as u32 + 1);
                }
                op::LOCAL | op::FUNCARG if cur != u32::MAX => funcs[cur as usize].locals.push(i.arg),
                _ => {}
            }
            owner[k] = cur;
            if i.op == op::ENDFUNCTION {
                cur = u32::MAX;
            }
        }
        let by_name: HashMap<Rc<str>, u32> = funcs.iter().enumerate().filter(|(_, f)| &*f.name != "<init>").map(|(k, f)| (f.name.clone(), k as u32)).collect();
        let mut res = vec![UNRESOLVED; code.len()];
        for (k, i) in code.iter().enumerate() {
            res[k] = match i.op {
                op::MOVRV | op::MOVVR | op::MOVRA | op::MOVAR | op::LOCAL | op::FUNCARG | op::GLOBAL | op::CONSTANT => {
                    let local = owner[k] != u32::MAX && matches!(i.op, op::MOVRV | op::MOVVR | op::MOVRA | op::MOVAR | op::LOCAL | op::FUNCARG);
                    match funcs.get(owner[k] as usize).and_then(|f| f.locals.iter().position(|&l| l == i.arg)).filter(|_| local && !matches!(i.op, op::GLOBAL | op::CONSTANT)) {
                        Some(slot) => LOCAL | slot as u32,
                        None => m.global_index.get(&*name(i.arg)).copied().unwrap_or(UNRESOLVED),
                    }
                }
                op::NATIVE => m.native_index.get(&*name(i.arg)).copied().unwrap_or(UNRESOLVED),
                op::CALL | op::CODE => by_name.get(&name(i.arg)).copied().unwrap_or(UNRESOLVED),
                op::JUMP | op::OLDJUMP | op::JUMPIFTRUE | op::JUMPIFFALSE => labels.get(&i.arg).copied().unwrap_or(UNRESOLVED),
                _ => UNRESOLVED,
            };
        }
        let at_entry = funcs.iter().enumerate().map(|(k, f)| (f.entry, k as u32)).collect();
        let of_port = funcs.iter().enumerate().filter(|(_, f)| f.port != NO_FUNC).map(|(k, f)| (f.port, k as u32)).collect();
        let inits = funcs.iter().filter(|f| &*f.name == "<init>").map(|f| f.entry).collect();
        let sigs = m.natives.iter().map(|n| Sig { params: n.params.iter().map(|p| m.base_type(p)).collect(), ret: machine_type(n.ret_base) }).collect();
        Prog { code, res, strings, funcs, at_entry, of_port, inits, sigs }
    }
}

/// How an interpreter run ended (`FUN_6f91b460`'s result).
enum Status {
    /// 1: the run's first frame returned (or its last frame).
    Finished,
    /// 2: its operation limit reached.
    Limit,
    /// 3: a native asked it to sleep.
    Sleep(f32),
    /// 6 and 7 (an unset variable read, a division by zero), or a native's abort.
    Error(String),
}

pub struct Vm {
    pub module: Module,
    prog: Rc<Prog>,
    /// The globals (+0x285c), by the module's index; made as their declarations run.
    pub vars: Vec<Var>,
    pub strings: Strings,
    /// Strings the code's table gained as they were first interned (after its own).
    pub code_strings: Vec<Rc<str>>,
    /// The code references natives were given (+0x2888), each a function's index, the
    /// first place unused; and their places by function.
    pub callbacks: Vec<u32>,
    callback_index: HashMap<u32, u32>,
    /// The machine's own context.
    pub main: Thread,
    /// Instruction budget per run (`FUN_6f91b460`'s 300000); past it the run ends.
    pub op_limit: u32,
    /// Handles a function's locals (not its arguments) held as it returned, which the
    /// original keeps for good (read: the group MeleeFindNearestMine destroys without
    /// clearing its local stays taken in a save's handle table, the ids after it not
    /// reusing it).
    pub leaked: BTreeSet<u32>,
}

/// The machine's state as the port saves it.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct State {
    pub vars: Vec<Var>,
    pub strings: Strings,
    pub code_strings: Vec<Rc<str>>,
    pub callbacks: Vec<u32>,
    pub main: Thread,
    pub leaked: BTreeSet<u32>,
}

impl Vm {
    pub fn new(module: Module) -> Self {
        let prog = Rc::new(Prog::new(&module));
        let vars = module.globals.iter().map(|_| Var::default()).collect();
        Vm { module, prog, vars, strings: Strings::default(), code_strings: Vec::new(), callbacks: vec![0], callback_index: HashMap::new(), main: Thread::default(), op_limit: 300_000, leaked: Default::default() }
    }

    /// The machine's state, for a saved game.
    pub fn variables(&self) -> State {
        State { vars: self.vars.clone(), strings: self.strings.clone(), code_strings: self.code_strings.clone(), callbacks: self.callbacks.clone(), main: self.main.clone(), leaked: self.leaked.clone() }
    }

    /// Restores the state [`Vm::variables`] gave, of the same script.
    pub fn set_variables(&mut self, s: State) {
        if s.vars.len() != self.vars.len() {
            return;
        }
        (self.vars, self.strings, self.code_strings, self.callbacks, self.main, self.leaked) = (s.vars, s.strings, s.code_strings, s.callbacks, s.main, s.leaked);
        self.strings.rebuild();
        self.callback_index = self.callbacks.iter().enumerate().skip(1).map(|(k, &f)| (f, k as u32)).collect();
    }

    /// Every handle the globals and global arrays hold, and those returned functions'
    /// locals kept (`leaked`).
    pub fn global_handles(&self, out: &mut impl FnMut(u32)) {
        for v in &self.vars {
            var_handles(v, out);
        }
        for &h in &self.leaked {
            out(h);
        }
    }

    /// Runs every source file's global initializers on the machine.
    pub fn init_globals(&mut self, host: &mut dyn Host) -> Result<(), String> {
        let prog = self.prog.clone();
        let mut t = std::mem::take(&mut self.main);
        let mut r = Ok(());
        for &entry in &prog.inits {
            t.frames.push(Frame::new(NO_FUNC, false));
            t.ip = Some(entry);
            if let Status::Error(e) = self.interpret(host, &mut t) {
                r = Err(e);
                break;
            }
        }
        self.main = t;
        r
    }

    /// Runs a function on the machine's own context (`FUN_6f91c310`), as the engine runs
    /// `config` and `main`.
    pub fn run_main(&mut self, host: &mut dyn Host, func: FuncId) -> Outcome {
        let Some(&f) = self.prog.of_port.get(&func) else { return Outcome::Aborted(format!("no function #{func}")) };
        let mut t = std::mem::take(&mut self.main);
        t.frames.push(Frame::new(func, false));
        t.ip = Some(self.prog.funcs[f as usize].entry);
        let s = self.interpret(host, &mut t);
        let out = self.outcome(&t, f, s);
        self.main = t;
        out
    }

    pub fn new_thread(&self, func: FuncId, args: &[Value], can_sleep: bool) -> Thread {
        Thread { can_sleep, entry: Some((func, args.to_vec())), ..Default::default() }
    }

    /// Calls a function to completion on a fresh thread that may not sleep; used for
    /// conditions, filters, and enumeration callbacks.
    pub fn call(&mut self, host: &mut dyn Host, func: FuncId, args: &[Value]) -> Outcome {
        let mut t = self.new_thread(func, args, false);
        self.run(host, &mut t)
    }

    /// Runs (or resumes) a thread until it finishes, sleeps, or aborts.
    pub fn run(&mut self, host: &mut dyn Host, t: &mut Thread) -> Outcome {
        let prog = self.prog.clone();
        let mut f = t.frames.iter().rev().find(|f| f.top).and_then(|f| prog.of_port.get(&f.func).copied()).unwrap_or(u32::MAX);
        if let Some((func, args)) = t.entry.take() {
            // FUN_6f91c510: a base frame holding the arguments (each through register
            // 0) and the return place, then the function's frame, its run's first.
            let Some(&k) = prog.of_port.get(&func) else { return Outcome::Aborted(format!("no function #{func}")) };
            f = k;
            t.returns.push(t.ip);
            let mut base = Frame::new(NO_FUNC, false);
            for a in &args {
                let r = self.reg_of(a);
                t.regs[0] = r;
                base.stack.push(r);
            }
            base.stack.push(Reg::of(ty::RETURN, return_place(t.ip)));
            t.frames.push(base);
            t.frames.push(Frame::new(func, true));
            t.ip = Some(prog.funcs[k as usize].entry);
        }
        let s = self.interpret(host, t);
        let finished = matches!(s, Status::Finished);
        let out = self.outcome(t, f, s);
        if !matches!(out, Outcome::Sleeping(_)) {
            // The run's frames freed, its place restored.
            for fr in std::mem::take(&mut t.frames).into_iter().rev() {
                self.free_frame(fr, finished);
            }
            t.ip = t.returns.pop().flatten();
        }
        out
    }

    fn outcome(&mut self, t: &Thread, f: u32, s: Status) -> Outcome {
        match s {
            Status::Finished => Outcome::Finished(self.value_of(t.regs[0], self.prog.funcs.get(f as usize).map_or(ty::NOTHING, |f| f.ret))),
            Status::Limit => Outcome::Aborted("operation limit reached".into()),
            Status::Sleep(secs) => {
                if t.can_sleep {
                    Outcome::Sleeping(secs)
                } else {
                    // A thread that may not sleep (a timer's callback, an enumeration's,
                    // a filter) ends there, as the original's does (measured: a timer
                    // callback's text after its sleep never shows).
                    Outcome::Finished(Value::Null)
                }
            }
            Status::Error(e) => Outcome::Aborted(e),
        }
    }

    /// A frame freed (`FUN_6f9161b0`): its arguments released; with `leak`, the handles
    /// its locals hold kept for good.
    fn free_frame(&mut self, fr: Frame, leak: bool) {
        for v in &fr.locals {
            if v.param {
                self.release(v);
            } else if leak && v.decl == ty::HANDLE && v.value != 0 {
                self.leaked.insert(v.value);
            }
        }
    }

    /// A variable's reference released (`FUN_6f916370`): a string's count.
    fn release(&mut self, v: &Var) {
        if v.decl != ty::HANDLE && v.kind == ty::STRING {
            self.strings.release(v.value);
        }
    }

    /// Interns a string, the code's table taking it if new.
    pub fn intern(&mut self, s: &str) -> u32 {
        let (k, new) = self.strings.intern(s);
        if new && !self.prog.strings.iter().any(|x| &**x == s) && !self.code_strings.iter().any(|x| &**x == s) {
            self.code_strings.push(Rc::from(s));
        }
        k
    }

    fn text(&self, k: u32) -> Option<Rc<str>> {
        self.strings.get(k).cloned()
    }

    /// A code reference's place among those natives were given (`FUN_6f916880`).
    fn callback(&mut self, f: u32) -> u32 {
        if let Some(&k) = self.callback_index.get(&f) {
            return k;
        }
        self.callbacks.push(f);
        let k = self.callbacks.len() as u32 - 1;
        self.callback_index.insert(f, k);
        k
    }

    /// A register's value as a native or the host sees it, by type.
    fn value_of(&mut self, r: Reg, t: u8) -> Value {
        match t {
            ty::INTEGER => Value::Int(r.value as i32),
            ty::REAL => Value::Real(f32::from_bits(r.value)),
            ty::BOOLEAN => Value::Bool(r.value != 0),
            ty::STRING => self.text(r.value).map_or(Value::Null, Value::Str),
            ty::HANDLE => Value::opt_handle(r.value),
            ty::CODE => match self.prog.at_entry.get(&(r.value / 2)).map(|&k| self.prog.funcs[k as usize].port) {
                Some(f) if r.kind == ty::CODE && f != NO_FUNC => Value::Code(f),
                _ => Value::Null,
            },
            _ => Value::Null,
        }
    }

    /// A host's value as a register.
    fn reg_of(&mut self, v: &Value) -> Reg {
        match v {
            Value::Int(i) => Reg::of(ty::INTEGER, *i as u32),
            Value::Real(x) => Reg::of(ty::REAL, x.to_bits()),
            Value::Bool(b) => Reg::of(ty::BOOLEAN, *b as u32),
            Value::Str(s) => Reg::of(ty::STRING, self.intern(s)),
            Value::Handle(h) => Reg::of(ty::HANDLE, *h),
            Value::Code(f) => Reg::of(ty::CODE, self.prog.of_port.get(f).map_or(0, |&k| self.prog.funcs[k as usize].entry * 2)),
            Value::Null | Value::Uninit => Reg::of(ty::NULL, 0),
        }
    }

    /// A native's result as the word register 0 takes, by the native's result type.
    fn word_of(&mut self, v: Value, t: u8) -> u32 {
        match (t, v) {
            (_, Value::Null | Value::Uninit) => 0,
            (ty::REAL, Value::Int(i)) => crate::softfloat::itof(i),
            (_, Value::Int(i)) => i as u32,
            (ty::INTEGER, Value::Real(x)) => x as i32 as u32,
            (_, Value::Real(x)) => x.to_bits(),
            (_, Value::Bool(b)) => b as u32,
            (_, Value::Str(s)) => self.intern(&s),
            (_, Value::Handle(h)) => h,
            (_, Value::Code(f)) => self.prog.of_port.get(&f).map_or(0, |&k| self.prog.funcs[k as usize].entry * 2),
        }
    }

    /// A variable by its resolution: a local of the context's current frame, or a global.
    fn var<'a>(vars: &'a mut [Var], t: &'a mut Thread, res: u32) -> Option<&'a mut Var> {
        if res & LOCAL != 0 && res != UNRESOLVED {
            t.frames.last_mut()?.locals.get_mut((res & !LOCAL) as usize)
        } else {
            vars.get_mut(res as usize)
        }
    }

    /// A variable set with its references (`FUN_6f916080` with the machine): the old
    /// string released and the new counted; its type becomes its declared type.
    fn set_var(strings: &mut Strings, v: &mut Var, kind: u8, value: u32) {
        if kind > 8 {
            return;
        }
        let changed = v.value != value;
        if changed && v.decl != ty::HANDLE && v.kind == ty::STRING {
            strings.release(v.value);
        }
        v.kind = v.decl;
        v.value = value;
        if changed && v.decl != ty::HANDLE && v.kind == ty::STRING {
            strings.add_ref(value);
        }
    }

    /// The interpreter (`FUN_6f91b460`): runs the context from its place until its run
    /// ends, at most the operation limit's instructions.
    fn interpret(&mut self, host: &mut dyn Host, t: &mut Thread) -> Status {
        let prog = self.prog.clone();
        let Some(mut ip) = t.ip.map(|i| i as usize) else { return Status::Finished };
        t.ops = 0;
        let limit = self.op_limit;
        macro_rules! fail {
            ($($a:tt)*) => {{
                t.ip = Some(ip as u32);
                return Status::Error(format!($($a)*));
            }};
        }
        loop {
            if t.ops >= limit {
                t.ip = Some(ip as u32);
                return Status::Limit;
            }
            t.ops += 1;
            let Some(&i) = prog.code.get(ip) else { fail!("ran past the code") };
            let res = prog.res[ip];
            ip += 1;
            let (a, b, c) = (i.a as usize, i.b as usize, i.c as usize);
            match i.op {
                op::JUMP | op::OLDJUMP => ip = res as usize,
                op::JUMPIFTRUE => {
                    if t.regs[c].value != 0 {
                        ip = res as usize;
                    }
                }
                op::JUMPIFFALSE => {
                    if t.regs[c].value == 0 {
                        ip = res as usize;
                    }
                }
                op::LOCAL => {
                    if let Some(f) = t.frames.last_mut() {
                        f.locals.push(Var::declared(i.c));
                    }
                }
                op::GLOBAL | op::CONSTANT => {
                    if let Some(v) = self.vars.get_mut(res as usize) {
                        *v = Var::declared(i.c);
                    }
                }
                op::FUNCARG => {
                    // The argument b from the stack's top of the caller's frame, below
                    // its return place.
                    let n = t.frames.len();
                    let r = n.checked_sub(2).and_then(|p| {
                        let s = &t.frames[p].stack;
                        s.len().checked_sub(b + 1).map(|k| s[k])
                    });
                    let mut v = Var { param: true, decl: i.c, ..Default::default() };
                    if let Some(r) = r {
                        Self::set_var(&mut self.strings, &mut v, i.c, r.value);
                    }
                    if let Some(f) = t.frames.last_mut() {
                        f.locals.push(v);
                    }
                }
                op::POPN => {
                    if let Some(f) = t.frames.last_mut() {
                        let n = f.stack.len().saturating_sub(c);
                        f.stack.truncate(n);
                    }
                }
                op::LITERAL => {
                    let value = if i.b == ty::STRING && i.arg != 0 {
                        let s = prog.strings.get(i.arg as usize - 1).cloned().unwrap_or_else(|| Rc::from(""));
                        self.intern(&s)
                    } else {
                        i.arg
                    };
                    if i.b < 9 {
                        t.regs[c] = Reg::of(i.b, value);
                    }
                }
                op::MOVRR => t.regs[c] = t.regs[b],
                op::MOVRV => {
                    let Some(v) = Self::var(&mut self.vars, t, res) else { fail!("no variable {}", name(&prog, i.arg)) };
                    if v.kind == 0 {
                        fail!("read of uninitialized variable {}", name(&prog, i.arg));
                    }
                    let value = v.value;
                    t.regs[c] = Reg::of(i.b, value);
                }
                op::CODE => {
                    let e = prog.funcs.get(res as usize).map_or(0, |f| f.entry * 2);
                    t.regs[c] = Reg::of(ty::CODE, e);
                }
                op::MOVRA => {
                    let k = t.regs[b].value as usize;
                    let Some(v) = Self::var(&mut self.vars, t, res) else { fail!("no variable {}", name(&prog, i.arg)) };
                    let value = v.array.as_ref().and_then(|x| x.get(k).copied()).unwrap_or(0);
                    if i.a < 9 {
                        t.regs[c] = Reg::of(i.a, value);
                    }
                }
                op::MOVVR => {
                    let r = t.regs[c];
                    let Some(v) = Self::var(&mut self.vars, t, res) else { fail!("no variable {}", name(&prog, i.arg)) };
                    let kind = if r.kind == ty::NULL { v.decl } else { r.kind };
                    Self::set_var(&mut self.strings, v, kind, r.value);
                }
                op::MOVAR => {
                    // FUN_6f916110: the array grows to the index (at most 8192), a string
                    // array's old element released and its new counted.
                    let k = t.regs[c].value;
                    let r = t.regs[b];
                    let Some(v) = Self::var(&mut self.vars, t, res) else { fail!("no variable {}", name(&prog, i.arg)) };
                    let old = v.array.as_ref().and_then(|x| x.get(k as usize).copied()).unwrap_or(0);
                    let changed = old != r.value;
                    let strings = v.kind == ty::STRING_ARRAY;
                    let n = (k.wrapping_add(1) as usize).min(ARRAY_SIZE);
                    if n != 0 {
                        let x = v.array.get_or_insert_with(Vec::new);
                        if x.len() < n {
                            x.resize(n, 0);
                        }
                        if let Some(e) = x.get_mut(k as usize) {
                            *e = r.value;
                        }
                    }
                    if changed && strings {
                        self.strings.release(old);
                        self.strings.add_ref(r.value);
                    }
                }
                op::PUSH => {
                    let r = t.regs[c];
                    if let Some(f) = t.frames.last_mut() {
                        f.stack.push(r);
                    }
                }
                op::POP => {
                    if let Some(r) = t.frames.last_mut().and_then(|f| f.stack.pop()) {
                        t.regs[c] = r;
                    }
                }
                op::NATIVE => {
                    let Some(sig) = prog.sigs.get(res as usize) else { fail!("no native {}", name(&prog, i.arg)) };
                    let n = sig.params.len();
                    let Some(f) = t.frames.last() else { fail!("no frame") };
                    let Some(base) = f.stack.len().checked_sub(n) else { fail!("too few arguments to {}", name(&prog, i.arg)) };
                    let regs: Vec<Reg> = f.stack[base..].to_vec();
                    let mut args = Vec::with_capacity(n);
                    for (r, p) in regs.iter().zip(&sig.params) {
                        args.push(match p {
                            // A code argument is a reference the machine keeps (by its
                            // place among the references); not code, none.
                            BaseType::Code => match self.value_of(*r, ty::CODE) {
                                Value::Code(fid) => {
                                    self.callback(r.value / 2);
                                    Value::Code(fid)
                                }
                                v => v,
                            },
                            BaseType::Integer => self.value_of(*r, ty::INTEGER),
                            BaseType::Real => self.value_of(*r, ty::REAL),
                            BaseType::Boolean => self.value_of(*r, ty::BOOLEAN),
                            BaseType::String => self.value_of(*r, ty::STRING),
                            BaseType::Handle => self.value_of(*r, ty::HANDLE),
                        });
                    }
                    t.ip = Some(ip as u32);
                    let result = host.call_native(self, res, &args);
                    if let Some(f) = t.frames.last_mut() {
                        f.stack.truncate(base);
                    }
                    match result {
                        NativeResult::Return(v) => {
                            let rt = sig.ret;
                            let w = self.word_of(v, rt);
                            t.regs[0] = Reg::of(rt, w);
                        }
                        NativeResult::Sleep(secs) => {
                            t.regs[0] = Reg::default();
                            t.ip = Some(ip as u32);
                            return Status::Sleep(secs);
                        }
                        NativeResult::Abort(e) => fail!("{e}"),
                    }
                }
                op::CALL => {
                    let Some(f) = prog.funcs.get(res as usize) else { fail!("no function {}", name(&prog, i.arg)) };
                    if t.frames.len() > 4096 {
                        fail!("call stack overflow");
                    }
                    let place = return_place(Some(ip as u32));
                    if let Some(fr) = t.frames.last_mut() {
                        fr.stack.push(Reg::of(ty::RETURN, place));
                    }
                    t.frames.push(Frame::new(f.port, false));
                    ip = f.entry as usize;
                }
                op::I2R => {
                    let v = crate::softfloat::itof(t.regs[c].value as i32);
                    t.regs[c] = Reg::of(ty::REAL, v);
                }
                op::AND | op::OR => {
                    let (l, r) = (t.regs[b].value != 0, t.regs[a].value != 0);
                    t.regs[c] = Reg::of(ty::BOOLEAN, if i.op == op::AND { l && r } else { l || r } as u32);
                }
                op::EQ | op::NE => {
                    let (l, r) = (t.regs[b], t.regs[a]);
                    let eq = match l.kind {
                        // Within 0.001 (`FUN_6f05cb40`); `!=` exact.
                        ty::REAL if i.op == op::EQ => f32::from_bits(crate::softfloat::sub(l.value, r.value)).abs() < f32::from_bits(0x3a83_126f),
                        // By text; `!=` by place (the original's compares the strings'
                        // pointers, one to a text as interned).
                        ty::STRING if i.op == op::EQ => self.text(l.value) == self.text(r.value),
                        ty::BOOLEAN => (l.value != 0) == (r.value != 0),
                        _ => l.value == r.value,
                    };
                    t.regs[c] = Reg::of(ty::BOOLEAN, (eq == (i.op == op::EQ)) as u32);
                }
                op::LE | op::GE | op::LT | op::GT => {
                    let (l, r) = (t.regs[b], t.regs[a]);
                    let v = if l.kind == ty::INTEGER {
                        let (x, y) = (l.value as i32, r.value as i32);
                        match i.op {
                            op::LE => x <= y,
                            op::GE => x >= y,
                            op::LT => x < y,
                            _ => x > y,
                        }
                    } else {
                        let (x, y) = (f32::from_bits(l.value), f32::from_bits(r.value));
                        match i.op {
                            op::LE => x <= y,
                            op::GE => y <= x,
                            op::LT => x < y,
                            _ => y < x,
                        }
                    };
                    t.regs[c] = Reg::of(ty::BOOLEAN, v as u32);
                }
                op::ADD | op::SUB | op::MUL | op::DIV => {
                    use crate::softfloat as sf;
                    let (l, r) = (t.regs[b], t.regs[a]);
                    let (x, y) = (l.value, r.value);
                    let v = match (i.op, l.kind, r.kind) {
                        (op::ADD, ty::STRING, ty::STRING) => {
                            let mut s = self.text(x).map_or(String::new(), |s| s.to_string());
                            s.push_str(self.text(y).as_deref().unwrap_or(""));
                            Some((ty::STRING, self.intern(&s)))
                        }
                        (op::ADD | op::SUB, ty::INTEGER | ty::REAL, ty::INTEGER) => Some((l.kind, if i.op == op::ADD { x.wrapping_add(y) } else { x.wrapping_sub(y) })),
                        (op::ADD, ty::INTEGER | ty::REAL, ty::REAL) => Some((l.kind, sf::add(x, y))),
                        (op::SUB, ty::INTEGER | ty::REAL, ty::REAL) => Some((l.kind, sf::sub(x, y))),
                        (op::ADD | op::SUB, _, _) => None,
                        (op::MUL, _, ty::INTEGER) => Some((l.kind, (x as i32).wrapping_mul(y as i32) as u32)),
                        (op::MUL, _, ty::REAL) => Some((l.kind, sf::mul(x, y))),
                        (op::MUL, _, _) => Some((l.kind, 0)),
                        (_, _, ty::INTEGER) => {
                            if y == 0 {
                                fail!("integer division by zero");
                            }
                            Some((l.kind, (x as i32).wrapping_div(y as i32) as u32))
                        }
                        (_, _, ty::REAL) => {
                            if y & 0x7fff_ffff == 0 {
                                fail!("real division by zero");
                            }
                            Some((l.kind, sf::div(x, y)))
                        }
                        _ => Some((l.kind, 0)),
                    };
                    if let Some((k, v)) = v.filter(|(k, _)| *k < 9) {
                        t.regs[c] = Reg::of(k, v);
                    }
                }
                op::NEG => {
                    let r = &mut t.regs[c];
                    match r.kind {
                        ty::INTEGER => (r.kind, r.value) = (r.decl, (r.value as i32).wrapping_neg() as u32),
                        ty::REAL => (r.kind, r.value) = (r.decl, r.value ^ 0x8000_0000),
                        _ => {}
                    }
                }
                op::NOT => {
                    let r = &mut t.regs[c];
                    (r.kind, r.value) = (r.decl, ((r.value as i32) < 1) as u32);
                }
                op::RETURN => {
                    let Some(fr) = t.frames.last() else { fail!("no frame") };
                    if fr.top {
                        t.ip = Some(ip as u32);
                        return Status::Finished;
                    }
                    let fr = t.frames.pop().unwrap();
                    self.free_frame(fr, true);
                    let Some(top) = t.frames.last_mut() else {
                        t.ip = Some(ip as u32);
                        return Status::Finished;
                    };
                    let place = top.stack.pop().map_or(0, |r| r.value);
                    ip = (place / 2) as usize;
                }
                _ => {}
            }
        }
    }

    /// The code's words, as compiled.
    pub fn code_words(&self) -> &[u32] {
        &self.module.code.words
    }

    /// The code's string table (from id 1): as compiled, then the strings interned since
    /// that it lacked.
    pub fn code_table(&self) -> Vec<Rc<str>> {
        self.prog.strings.iter().chain(&self.code_strings).cloned().collect()
    }

    /// The name of the function entered at an instruction.
    pub fn function_at(&self, entry: u32) -> Option<Rc<str>> {
        self.prog.at_entry.get(&entry).map(|&k| self.prog.funcs[k as usize].name.clone())
    }

    /// A function's locals' names (its arguments first), in their slots' order.
    pub fn local_names(&self, func: FuncId) -> Vec<Rc<str>> {
        let Some(&k) = self.prog.of_port.get(&func) else { return Vec::new() };
        self.prog.funcs[k as usize].locals.iter().map(|&id| name(&self.prog, id)).collect()
    }

    /// The globals' names, by the module's index.
    pub fn global_names(&self) -> Vec<Rc<str>> {
        self.module.globals.iter().map(|g| Rc::from(&*g.name)).collect()
    }

    /// A global's value as the host sees it.
    pub fn global(&self, name: &str) -> Option<Value> {
        let g = *self.module.global_index.get(name)?;
        let v = self.vars.get(g as usize)?;
        Some(self.var_value(v))
    }

    fn var_value(&self, v: &Var) -> Value {
        if v.kind == 0 {
            return Value::Uninit;
        }
        let r = Reg::of(v.kind, v.value);
        match v.kind {
            ty::INTEGER => Value::Int(r.value as i32),
            ty::REAL => Value::Real(f32::from_bits(r.value)),
            ty::BOOLEAN => Value::Bool(r.value != 0),
            ty::STRING => self.strings.get(r.value).cloned().map_or(Value::Null, Value::Str),
            ty::HANDLE => Value::opt_handle(r.value),
            ty::CODE => self.prog.at_entry.get(&(r.value / 2)).map(|&k| self.prog.funcs[k as usize].port).filter(|&f| f != NO_FUNC).map_or(Value::Null, Value::Code),
            _ => Value::Null,
        }
    }

    /// Sets a global by index, as a script's `set` would.
    pub fn set_global_value(&mut self, g: u32, v: &Value) {
        let r = self.reg_of(v);
        if let Some(var) = self.vars.get_mut(g as usize) {
            if var.decl == 0 {
                return;
            }
            let kind = if r.kind == ty::NULL { var.decl } else { r.kind };
            Self::set_var(&mut self.strings, var, kind, r.value);
        }
    }

    /// Sets a global array's elements by index, as a script's `set`s would.
    pub fn set_global_array(&mut self, g: u32, vs: &[Value]) {
        let regs: Vec<Reg> = vs.iter().map(|v| self.reg_of(v)).collect();
        if let Some(var) = self.vars.get_mut(g as usize) {
            if var.kind <= 8 {
                return;
            }
            let x = var.array.get_or_insert_with(Vec::new);
            x.clear();
            x.extend(regs.iter().take(ARRAY_SIZE).map(|r| r.value));
        }
    }

    pub fn set_global(&mut self, name: &str, v: Value) -> bool {
        match self.module.global_index.get(name) {
            Some(&g) => {
                self.set_global_value(g, &v);
                true
            }
            None => false,
        }
    }
}

/// A return place as a word offset (`FUN_6f91c9f0`): none for a context not yet run
/// (provisional: the original's is its code's address taken from none, a word no saved
/// game can reproduce).
fn return_place(ip: Option<u32>) -> u32 {
    ip.map_or(0, |i| i * 2)
}

fn name(prog: &Prog, id: u32) -> Rc<str> {
    prog.strings.get((id as usize).wrapping_sub(1)).cloned().unwrap_or_else(|| Rc::from("?"))
}

pub fn values_equal(l: &Value, r: &Value) -> bool {
    match (l, r) {
        (Value::Int(a), Value::Real(b)) | (Value::Real(b), Value::Int(a)) => *a as f32 == *b,
        (Value::Null, Value::Handle(0)) | (Value::Handle(0), Value::Null) => true,
        (Value::Str(a), Value::Str(b)) => a == b,
        (a, b) => a == b,
    }
}
