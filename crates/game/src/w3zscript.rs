//! The script data of a saved game in the original's format (`FUN_6f90d350`): the
//! code blocks, each machine's state, and its threads, made from the port's machine.

use w3_formats::w3z::{Code, Contents, Field, Frame, Machine, SaveGame, ScriptData, Thread, Value};
use w3_jass::vm::{Reg, Var};

use crate::Game;

/// A register or a pushed value as saved (`FUN_6f91a6e0`): its type, the type given,
/// and its word.
fn reg_value(r: &Reg) -> Value {
    Value { kind: r.kind as u32, word: r.decl as u32, length: 0, value: Contents::One(r.value) }
}

/// A variable as saved: an array its element count and words.
fn var_value(v: &Var) -> Value {
    match (v.kind > 8, &v.array) {
        (true, Some(a)) => Value { kind: v.kind as u32, word: v.decl as u32, length: a.len() as u32, value: Contents::Array(a.clone()) },
        (true, None) => Value { kind: v.kind as u32, word: v.decl as u32, length: 0, value: Contents::Array(Vec::new()) },
        _ => Value { kind: v.kind as u32, word: v.decl as u32, length: 0, value: Contents::One(v.value) },
    }
}

impl Game {
    /// A context's frames as saved (`FUN_6f91a540`), the current first: each its pushed
    /// values, whether it is its run's first, and its locals by name as made.
    fn saved_frames(&self, frames: &[w3_jass::vm::Frame]) -> Vec<Frame> {
        frames
            .iter()
            .rev()
            .map(|f| {
                let names = self.vm.local_names(f.func);
                Frame {
                    arguments: f.stack.iter().map(reg_value).collect(),
                    word: f.top as u32,
                    locals: f.locals.iter().enumerate().map(|(k, v)| (names.get(k).map_or_else(String::new, |n| n.to_string()), var_value(v))).collect(),
                }
            })
            .collect()
    }

    /// The map script's machine as saved (`FUN_6f91a0d0`).
    pub fn saved_machine(&self) -> Machine {
        let vm = &self.vm;
        let m = &vm.main;
        let names = vm.global_names();
        Machine {
            code: 1,
            flag: 0,
            real: 0.0,
            word: vm.op_limit,
            at: m.ip.map_or(0, |i| i * 2),
            returns: m.returns.iter().map(|r| r.map_or(0, |i| i * 2)).collect(),
            functions: vm.strings.entries.iter().skip(1).map(|(s, n)| (s.as_deref().unwrap_or("").to_string(), *n)).collect(),
            globals: vm.vars.iter().zip(&names).filter(|(v, _)| v.decl != 0).map(|(v, n)| (n.to_string(), var_value(v))).collect(),
            frames: self.saved_frames(&m.frames),
            stack: m.regs.iter().map(reg_value).collect(),
            strings: vm.callbacks.iter().skip(1).map(|&e| vm.function_at(e).map_or_else(String::new, |n| n.to_string())).collect(),
        }
    }

    /// A thread as saved (`FUN_6f91a790`): its place, its place in the code, its frames,
    /// whether it sleeps, a real, its operation limit, and its registers.
    fn saved_thread(&self, t: &w3_jass::Thread, sleeping: bool) -> Thread {
        Thread {
            machine: t.place,
            at: t.ip.map_or(0, |i| i * 2),
            frames: self.saved_frames(&t.frames),
            flag: sleeping as u32,
            real: 0.0,
            word: self.vm.op_limit,
            stack: t.regs.iter().map(reg_value).collect(),
        }
    }

    /// The map script's threads as saved: the one that called SaveGame (as it was in
    /// the call), then those asleep (provisional: their order the places').
    pub fn saved_threads(&self) -> Vec<Thread> {
        let mut out: Vec<Thread> = self.vm.snapshot.iter().map(|t| self.saved_thread(t, false)).collect();
        for s in &self.world.sleepers {
            out.push(self.saved_thread(&s.thread, true));
        }
        out.sort_by_key(|t| t.machine);
        out
    }

    /// The map script's code block as saved (`FUN_6f910730`).
    pub fn saved_code(&self) -> Code {
        Code { words: self.vm.code_words().to_vec(), strings: self.vm.code_table().iter().map(|s| s.to_string()).collect() }
    }

    /// The script data as saved (`FUN_6f90d350`): the map script's code block, machine,
    /// and threads, the rest (the AI scripts') the template's.  A thread's first frame's
    /// return place (the original's code address from none, a heap address) the
    /// template's thread's at the same place where it has one (provisional).
    pub fn saved_script_data(&self, template: &ScriptData) -> ScriptData {
        let mut d = template.clone();
        d.thread_places = d.thread_places.max(self.vm.places.borrow().len);
        match d.code.first_mut() {
            Some(c) => *c = Some(self.saved_code()),
            None => d.code.push(Some(self.saved_code())),
        }
        d.code_places = d.code_places.max(d.code.len() as u32 + 1);
        let machine = (self.vm.main.place, self.saved_machine());
        match d.machines.first_mut() {
            Some(m) => *m = machine,
            None => d.machines.push(machine),
        }
        let mut threads = self.saved_threads();
        let old = d.threads.first().cloned().unwrap_or_default();
        for t in &mut threads {
            let word = old.iter().find(|o| o.machine == t.machine).and_then(|o| o.frames.last()).and_then(|f| f.arguments.last()).map(|v| v.value.clone());
            if let (Some(f), Some(w)) = (t.frames.last_mut(), word) {
                if let Some(v) = f.arguments.last_mut().filter(|v| v.kind == w3_jass::vm::ty::RETURN as u32 && v.value == Contents::One(0)) {
                    v.value = w;
                }
            }
        }
        match d.threads.first_mut() {
            Some(t) => *t = threads,
            None => d.threads.push(threads),
        }
        d
    }

    /// A save made from `template` (the original's save of the same moment) with the
    /// pieces the port makes in place of its own: the world's, and the script data.
    pub fn write_save(&self, template: &SaveGame) -> SaveGame {
        let mut s = self.world.write_save(template);
        let (Ok(state), Ok(d)) = (s.state(), s.script_data()) else { return s };
        let mut state = self.saved_state(&state);
        let Ok(bytes) = self.saved_script_data(&d).to_bytes() else { return s };
        if let Some((_, f)) = state.parts.iter_mut().find(|(n, _)| *n == "script data") {
            *f = vec![Field::Bytes(bytes)];
        }
        if let Ok(rest) = state.to_bytes() {
            s.rest = rest;
        }
        s
    }

    /// The script data's pieces the port makes, against the original's save of the same
    /// moment.
    pub fn compare_script(&self, orig: &SaveGame) -> Vec<String> {
        let mut out = Vec::new();
        let Ok(d) = orig.script_data() else { return vec!["script data: not read".into()] };
        if let Some(Some(c)) = d.code.first() {
            let ours = self.saved_code();
            let words = ours.words == c.words;
            let first = ours.strings.iter().zip(&c.strings).position(|(a, b)| a != b);
            out.push(format!("code: words {}; strings {} ours, {} the original's, {}", if words { "the same" } else { "differ" }, ours.strings.len(), c.strings.len(), first.map_or("agreeing".into(), |k| format!("first differing {k}: {:?} vs {:?}", ours.strings[k], c.strings[k]))));
            if first.is_none() && ours.strings.len() != c.strings.len() {
                out.push(format!("  the original's last {:?}, ours {:?}", &c.strings[c.strings.len().saturating_sub(3)..], &ours.strings[ours.strings.len().saturating_sub(3)..]));
            }
        }
        let Some((_, theirs)) = d.machines.first() else { return out };
        let ours = self.saved_machine();
        out.push(format!("machine: at {} vs {}; word {} vs {}; returns {:?} vs {:?}", ours.at, theirs.at, ours.word, theirs.word, ours.returns, theirs.returns));
        let (a, b) = (&ours.functions, &theirs.functions);
        let first = a.iter().zip(b).position(|(x, y)| x != y);
        out.push(format!("strings: {} ours, {} the original's; {}", a.len(), b.len(), first.map_or("agreeing".into(), |k| format!("first differing {k}: {:?} vs {:?}", a[k], b[k]))));
        if std::env::var("W3Z_ALL").is_ok() {
            for k in 0..a.len().max(b.len()) {
                if a.get(k) != b.get(k) {
                    out.push(format!("  string {}: {:?} vs {:?}", k + 1, a.get(k), b.get(k)));
                }
            }
        }
        let (a, b) = (&ours.globals, &theirs.globals);
        let differ: Vec<usize> = (0..a.len().max(b.len())).filter(|&k| a.get(k).map(|x| (&x.0, x.1.kind, x.1.word, x.1.length, &x.1.value)) != b.get(k).map(|x| (&x.0, x.1.kind, x.1.word, x.1.length, &x.1.value))).collect();
        out.push(format!("globals: {} ours, {} the original's; {} differ", a.len(), b.len(), differ.len()));
        for &k in differ.iter().take(if std::env::var("W3Z_ALL").is_ok() { usize::MAX } else { 12 }) {
            let show = |g: Option<&(String, Value)>| g.map(|(n, v)| format!("{n} {} {} {} {:?}", v.kind, v.word, v.length, v.value)).unwrap_or_default();
            out.push(format!("  global {k}: {} vs {}", show(a.get(k)), show(b.get(k))));
        }
        let (a, b) = (&ours.stack, &theirs.stack);
        let differ: Vec<usize> = (0..a.len().max(b.len())).filter(|&k| a.get(k).map(|v| (v.kind, v.word, &v.value)) != b.get(k).map(|v| (v.kind, v.word, &v.value))).collect();
        out.push(format!("registers: {} differ{}", differ.len(), differ.iter().take(12).map(|&k| format!("; r{k} {:?} vs {:?}", a.get(k).map(|v| (v.kind, v.word, &v.value)), b.get(k).map(|v| (v.kind, v.word, &v.value)))).collect::<String>()));
        out.push(format!("frames: {} ours, {} the original's", ours.frames.len(), theirs.frames.len()));
        out.push(format!("places: {} ours, {} the original's", self.vm.places.borrow().len, d.thread_places));
        let (a, b) = (self.saved_threads(), d.threads.first().cloned().unwrap_or_default());
        out.push(format!("threads: {} ours, {} the original's", a.len(), b.len()));
        for (x, y) in a.iter().zip(&b) {
            let regs = (0..256).filter(|&k| x.stack.get(k).map(|v| (v.kind, v.word, &v.value)) != y.stack.get(k).map(|v| (v.kind, v.word, &v.value))).count();
            let fr = |t: &Thread| t.frames.iter().map(|f| format!("[{} {:?} {:?}]", f.word, f.arguments.iter().map(|v| (v.kind, v.word, v.value.clone())).collect::<Vec<_>>(), f.locals.iter().map(|(n, v)| (n.clone(), v.kind, v.value.clone())).collect::<Vec<_>>())).collect::<String>();
            out.push(format!("  thread {} vs {}: at {} vs {}; flag {} vs {}; word {} vs {}; registers differ {regs}", x.machine, y.machine, x.at, y.at, x.flag, y.flag, x.word, y.word));
            let (fa, fb) = (fr(x), fr(y));
            if fa != fb {
                out.push(format!("    frames {fa} vs {fb}"));
            }
        }
        let first = ours.strings.iter().zip(&theirs.strings).position(|(x, y)| x != y);
        out.push(format!("code references: {} ours, {} the original's; {}", ours.strings.len(), theirs.strings.len(), first.map_or("agreeing".into(), |k| format!("first differing {k}: {:?} vs {:?}", ours.strings[k], theirs.strings[k]))));
        // The whole, as written.
        if let (Ok(a), Some(Field::Bytes(b))) = (self.saved_script_data(&d).to_bytes(), orig.state().ok().and_then(|g| g.parts.into_iter().find(|(n, _)| *n == "script data")).and_then(|(_, f)| f.into_iter().next())) {
            let first = a.iter().zip(&b).position(|(x, y)| x != y);
            out.push(format!("written: {} bytes ours, {} the original's, {}", a.len(), b.len(), first.map_or("the same".into(), |k| format!("first differing byte {k}"))));
        }
        out
    }
}
