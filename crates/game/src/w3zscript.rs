//! The script data of a saved game in the original's format (`FUN_6f90d350`): the
//! code blocks, each machine's state, and its threads, made from the port's machine.

use w3_formats::w3z::{Code, Contents, Field, Frame, Machine, SaveGame, ScriptData, Thread, Value};
use w3_jass::vm::{Reg, Var};

use crate::Game;

/// What a save's header holds that the world does not: the map's path from the game's
/// folder, its description as a game played alone carries it (the coded stat string),
/// and its checksums over the game's scripts.
#[derive(Clone, Debug, Default)]
pub struct SaveHead {
    pub map: String,
    pub stat: Vec<u8>,
    pub xoro: u32,
    pub sha1: [u8; 20],
    /// A network game's description (none for a game played alone).
    pub network: Option<crate::replay::NetworkHead>,
}

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

/// A context's frames as saved (`FUN_6f91a540`), the current first: each its pushed
/// values, whether it is its run's first, and its locals by name as made.
fn frames_of(vm: &w3_jass::Vm, frames: &[w3_jass::vm::Frame]) -> Vec<Frame> {
    frames
        .iter()
        .rev()
        .map(|f| {
            let names = vm.local_names(f.func);
            Frame {
                arguments: f.stack.iter().map(reg_value).collect(),
                word: f.top as u32,
                locals: f.locals.iter().enumerate().map(|(k, v)| (names.get(k).map_or_else(String::new, |n| n.to_string()), var_value(v))).collect(),
            }
        })
        .collect()
}

/// A machine as saved (`FUN_6f91a0d0`), on its code block (`code`, by place).
fn machine_of(vm: &w3_jass::Vm, code: u32) -> Machine {
    let m = &vm.main;
    let names = vm.global_names();
    Machine {
        code,
        flag: 0,
        real: 0.0,
        word: vm.op_limit,
        at: m.ip.map_or(0, |i| i * 2),
        returns: m.returns.iter().map(|r| r.map_or(0, |i| i * 2)).collect(),
        functions: vm.strings.entries.iter().skip(1).map(|(s, n)| (s.as_deref().unwrap_or("").to_string(), *n)).collect(),
        globals: vm.vars.iter().zip(&names).filter(|(v, _)| v.decl != 0).map(|(v, n)| (n.to_string(), var_value(v))).collect(),
        frames: frames_of(vm, &m.frames),
        stack: m.regs.iter().map(reg_value).collect(),
        strings: vm.callbacks.iter().skip(1).map(|&e| vm.function_at(e).map_or_else(String::new, |n| n.to_string())).collect(),
    }
}

/// A thread as saved (`FUN_6f91a790`): its place, its place in the code, its frames,
/// whether it sleeps, a real, its operation limit, and its registers.
fn thread_of(vm: &w3_jass::Vm, t: &w3_jass::Thread, sleeping: bool) -> Thread {
    Thread {
        machine: t.place,
        at: t.ip.map_or(0, |i| i * 2),
        frames: frames_of(vm, &t.frames),
        flag: sleeping as u32,
        real: 0.0,
        word: vm.op_limit,
        stack: t.regs.iter().map(reg_value).collect(),
    }
}

/// The original's machine's values as the port's: its strings by their text (its own
/// table, `Machine::functions`), a return place outside the code none; the rest as
/// saved (a code value its function's place in the same code).
pub(crate) struct Remap<'a> {
    pub strings: &'a [(String, u32)],
}

impl Remap<'_> {
    fn word(&self, vm: &mut w3_jass::Vm, kind: u32, v: u32) -> u32 {
        use w3_jass::vm::ty;
        match kind as u8 {
            ty::STRING | ty::STRING_ARRAY if v != 0 => self.strings.get(v as usize - 1).map_or(0, |s| vm.intern(&s.0)),
            ty::RETURN if v / 2 >= vm.code_len() => 0,
            _ => v,
        }
    }

    fn reg(&self, vm: &mut w3_jass::Vm, v: &Value) -> Reg {
        let value = match v.value {
            Contents::One(x) => self.word(vm, v.kind, x),
            Contents::Array(_) => 0,
        };
        Reg { kind: v.kind as u8, decl: v.word as u8, value }
    }

    pub(crate) fn var(&self, vm: &mut w3_jass::Vm, v: &Value, param: bool) -> Var {
        match &v.value {
            Contents::One(x) => Var { kind: v.kind as u8, decl: v.word as u8, value: self.word(vm, v.kind, *x), array: None, param },
            Contents::Array(xs) => {
                let xs = xs.iter().map(|&x| self.word(vm, v.kind, x)).collect();
                Var { kind: v.kind as u8, decl: v.word as u8, value: 0, array: Some(xs), param }
            }
        }
    }
}

/// A thread from its saved form (`thread_of`'s inverse): its frames' functions found
/// from the code (the current one's from its place; a caller's from the return place it
/// pushed; a frame below one marked its run's first, or holding a return place outside
/// the code, a run's base, its return place the run's), its locals' arguments by its
/// function.
pub(crate) fn thread_from(vm: &mut w3_jass::Vm, t: &Thread, map: &Remap) -> w3_jass::Thread {
    use w3_jass::vm::ty;
    let mut th = w3_jass::Thread::default();
    (th.place, th.ip, th.can_sleep) = (t.machine, (t.at != 0 && t.at / 2 < vm.code_len()).then_some(t.at / 2), true);
    for (k, v) in t.stack.iter().enumerate().take(th.regs.len()) {
        th.regs[k] = map.reg(vm, v);
    }
    let saved: Vec<&Frame> = t.frames.iter().rev().collect();
    let n = saved.len();
    let ret = |f: &Frame| f.arguments.iter().rev().find(|v| v.kind == ty::RETURN as u32).and_then(|v| match v.value {
        Contents::One(x) => Some(x),
        _ => None,
    });
    for (j, f) in saved.iter().enumerate() {
        let place = ret(f).filter(|&x| x != 0 && x / 2 < vm.code_len());
        let base = j + 1 < n && (saved[j + 1].word != 0 || place.is_none());
        let func = if j + 1 == n {
            th.ip.map_or(u32::MAX, |ip| vm.function_holding(ip))
        } else if base {
            th.returns.push(place.map(|x| x / 2));
            u32::MAX
        } else {
            vm.function_holding(place.unwrap_or(0) / 2)
        };
        let params = vm.param_count(func);
        let locals = f.locals.iter().enumerate().map(|(k, (_, v))| map.var(vm, v, k < params)).collect();
        let stack = f.arguments.iter().map(|v| map.reg(vm, v)).collect();
        th.frames.push(w3_jass::vm::Frame { func, locals, stack, top: f.word != 0 });
    }
    th
}

/// A machine's code block as saved (`FUN_6f910730`).
fn code_of(vm: &w3_jass::Vm) -> Code {
    Code { words: vm.code_words().to_vec(), strings: vm.code_table().iter().map(|s| s.to_string()).collect() }
}

impl Game {
    /// The map script's machine as saved (`FUN_6f91a0d0`).
    pub fn saved_machine(&self) -> Machine {
        machine_of(&self.vm, 1)
    }

    /// The map script's threads as saved, by place: those asleep (flag 1), those kept
    /// within SaveGame (flag 0), and the one that called it last as it was in the call
    /// (a timer's callback's, say, freed after it).
    pub fn saved_threads(&self) -> Vec<Thread> {
        // (A thread asleep records its sleep: read, a trigger's 30.)
        let mut out: Vec<Thread> = self.world.sleepers.iter().map(|s| {
            let mut t = thread_of(&self.vm, &s.thread, s.thread.parked.is_none());
            if s.thread.parked.is_none() {
                t.real = s.sleep;
            }
            t
        }).collect();
        if let Some(t) = self.vm.snapshot.as_ref().filter(|t| !out.iter().any(|o| o.machine == t.place)) {
            out.push(thread_of(&self.vm, t, false));
        }
        out.sort_by_key(|t| t.machine);
        out
    }

    /// The computers' AI scripts as saved, by their places: each machine, on its
    /// script's code block (one a script, after the map script's, in the order the
    /// AIs started), and its threads asleep (read: Forestwalk's undead computer's at
    /// its start, common.ai's code, its machine's limit 10000, its main asleep, the
    /// first frame of its thread).  Each block's
    /// code, then the machines and their threads.  Provisional: a machine's own place
    /// in the code (the original's an uninitialized word), two computers' sharing.
    pub fn saved_ai_scripts(&self) -> (Vec<Code>, Vec<((u32, Machine), Vec<Thread>)>) {
        let mut scripts: Vec<String> = Vec::new();
        let mut codes = Vec::new();
        let mut machines = Vec::new();
        let mut ais: Vec<(&u8, &crate::ai::AiVm)> = self.world.ai_vms.iter().collect();
        ais.sort_by_key(|(_, a)| a.vm.main.place);
        for (p, a) in ais {
            let script = self.world.ai.get(p).map_or_else(String::new, |s| s.script.to_ascii_lowercase());
            let k = match scripts.iter().position(|s| *s == script) {
                Some(k) => k,
                None => {
                    scripts.push(script);
                    codes.push(code_of(&a.vm));
                    scripts.len() - 1
                }
            };
            // (An AI's main has its script's function as its first frame, with none below
            // it to return to: read, Forestwalk's computer's main.  A thread StartThread
            // began keeps the base frame its start made, holding its return place, its
            // function's frame marked its run's first: read, its four others at 2.5
            // seconds.)
            let mut threads: Vec<Thread> = a
                .sleepers
                .iter()
                .map(|(_, t)| {
                    let started = t.frames.first().is_some_and(|f| f.func == u32::MAX) && t.frames.get(1).is_some_and(|f| a.vm.function_name(f.func).is_some_and(|n| &*n != "main"));
                    if started {
                        return thread_of(&a.vm, t, true);
                    }
                    let mut t = t.clone();
                    if t.frames.first().is_some_and(|f| f.func == u32::MAX) {
                        t.frames.remove(0);
                    }
                    // (No frame of it marked its run's first: read, its main's 0.)
                    let mut saved = thread_of(&a.vm, &t, true);
                    for f in &mut saved.frames {
                        f.word = 0;
                    }
                    saved
                })
                .collect();
            threads.sort_by_key(|t| t.machine);
            // (Its machine's own limit 10000, its threads' 300000.)
            let mut machine = machine_of(&a.vm, 2 + k as u32);
            machine.word = 10_000;
            machines.push(((a.vm.main.place, machine), threads));
        }
        (codes, machines)
    }

    /// Each computer's AI threads written as saved and read back (`thread_from`), the
    /// differences from the port's own: none when the two are each other's inverse.
    pub fn check_thread_round_trip(&mut self) -> Vec<String> {
        let mut out = Vec::new();
        let players: Vec<u8> = self.world.ai_vms.keys().copied().collect();
        for p in players {
            let Some(a) = self.world.ai_vms.get_mut(&p) else { continue };
            let m = machine_of(&a.vm, 2);
            let map = Remap { strings: &m.functions };
            let threads: Vec<w3_jass::Thread> = a.sleepers.iter().map(|(_, t)| t.clone()).collect();
            for t in threads {
                let saved = thread_of(&a.vm, &t, true);
                let back = thread_from(&mut a.vm, &saved, &map);
                let fr = |t: &w3_jass::Thread| t.frames.iter().map(|f| (f.func, f.top, f.stack.clone(), f.locals.iter().map(|v| (v.kind, v.decl, v.value, v.array.clone(), v.param)).collect::<Vec<_>>())).collect::<Vec<_>>();
                let same = (t.place, t.ip, &t.returns, &t.regs) == (back.place, back.ip, &back.returns, &back.regs) && fr(&t) == fr(&back);
                if !same {
                    out.push(format!("p{p} place {}: ip {:?} vs {:?}; returns {:?} vs {:?}; regs differ {:?}; frames {:?} vs {:?}", t.place, t.ip, back.ip, t.returns, back.returns, t.regs.iter().zip(&back.regs).enumerate().filter(|(_, (a, b))| a != b).map(|(k, (a, b))| (k, *a, *b)).take(4).collect::<Vec<_>>(), fr(&t).iter().map(|f| (f.0, f.1)).collect::<Vec<_>>(), fr(&back).iter().map(|f| (f.0, f.1)).collect::<Vec<_>>()));
                }
            }
        }
        out
    }

    /// The computers' AI scripts as a save of the original's has them, in place of
    /// those the map's start made: each machine (by its place) its globals and its
    /// threads (`thread_from`), each asleep for what was left of its sleep (its AI
    /// thread object's timer, due on the main clock, its flags without 4: `clock` that
    /// clock as saved; read, the hero map's at 450 seconds, due 150.09998 to 200.09998
    /// with the main clock at 150.0, the agent clock 150.665; no longer than the sleep
    /// it records), and their objects (`AItr`) as saved.
    pub fn restore_ai_scripts(&mut self, s: &SaveGame, scripts: &ScriptData, clock: f32, units: &std::collections::HashMap<(u32, u32), u32>) -> anyhow::Result<()> {
        use w3_formats::w3z::{Second, Timing};
        let objects = s.objects.as_ref().map(|m| m.objects()).transpose()?.unwrap_or_default();
        // Each thread's object (`AItr`) by its thread's place: its due, its last sleep,
        // its number (its own sixth word, the main's 0), and its timer's count.
        let mut due: std::collections::HashMap<u32, (f32, f32)> = Default::default();
        let mut numbers: std::collections::HashMap<u32, (u32, u32)> = Default::default();
        for o in objects.iter().flatten().filter(|o| &o.entry.kind.to_be_bytes() == crate::codes::class::AI_THREAD) {
            if let Ok(Second::Agent { own, tail, .. }) = o.second() {
                let sleep = match own.get(6) {
                    Some(Field::Real(r)) => *r,
                    _ => 0.0,
                };
                let number = match own.get(5) {
                    Some(Field::Word(n)) => *n,
                    _ => 0,
                };
                if let (Some(Field::Timer(_, _, Timing(Some(t)))), Some(Field::Word(place))) = (tail.first(), tail.get(4)) {
                    due.insert(*place, (t.0, sleep));
                    numbers.insert(*place, (number, t.2));
                }
            }
        }
        let now = self.world.time;
        for (k, (place, m)) in scripts.machines.iter().enumerate().skip(1) {
            let Some(p) = self.world.ai_vms.iter().find(|(_, a)| a.vm.main.place == *place).map(|(&p, _)| p) else { continue };
            let map = Remap { strings: &m.functions };
            let Some(a) = self.world.ai_vms.get_mut(&p) else { continue };
            for (name, v) in &m.globals {
                let Some(&g) = a.vm.module.global_index.get(name.as_str()) else { continue };
                let var = map.var(&mut a.vm, v, false);
                if let Some(slot) = a.vm.vars.get_mut(g as usize) {
                    *slot = var;
                }
            }
            let mut sleepers = Vec::new();
            for t in scripts.threads.get(k).into_iter().flatten() {
                let th = thread_from(&mut a.vm, t, &map);
                // (Its place taken in the places all machines share, as the map's
                // threads': a save counts the places by them, and the original finds
                // each thread by its place; read, a save of the port's whose count fell
                // short of its computer's threads' places crashed the original as the
                // first woke.)
                {
                    let mut places = a.vm.places.borrow_mut();
                    places.free.retain(|&p| p != th.place);
                    places.len = places.len.max(th.place + 1).max(scripts.thread_places);
                }
                let (mut left, sleep) = due.get(&t.machine).map_or((0.0, 0.0), |&(d, s)| (d - clock, s));
                while left > sleep + 1.0 {
                    left -= 300.0;
                }
                if left < -1.0 {
                    left += 300.0;
                }
                sleepers.push((now + left.max(0.0), th));
            }
            a.sleepers = sleepers;
            // Its threads' objects anew, one a thread as saved (the restart's slots taken
            // first, the main's for the main), each saying when its thread wakes, as the
            // port's own sleeps set them; none of the restart's threads left to start.
            let restored: Vec<(u32, f32, f32)> = a.sleepers.iter().map(|(at, th)| (th.place, *at, due.get(&th.place).map_or(0.0, |d| d.1))).collect();
            let Some(st) = self.world.ai.get_mut(&p) else { continue };
            st.spawned.clear();
            let mut old: Vec<crate::ai::ThreadObject> = std::mem::take(&mut st.thread_objects).into_values().collect();
            old.sort_by_key(|o| std::cmp::Reverse(o.number));
            let mut objects: std::collections::BTreeMap<u32, crate::ai::ThreadObject> = Default::default();
            let mut order: Vec<(u32, f32, f32)> = restored;
            order.sort_by_key(|&(place, ..)| numbers.get(&place).map_or(u32::MAX, |n| n.0));
            for (place, at, sleep) in order {
                let (number, count) = numbers.get(&place).copied().unwrap_or((objects.len() as u32, 0));
                let slot = match old.pop() {
                    Some(o) => o.slot,
                    None => self.world.tags.take(crate::codes::class::AI_THREAD),
                };
                objects.insert(place, crate::ai::ThreadObject { slot, number, sleep, count, due: at });
            }
            for o in old {
                self.world.tags.doom(o.slot);
            }
            if let Some(st) = self.world.ai.get_mut(&p) {
                st.thread_objects = objects;
            }
        }
        self.restore_captains(&objects, units);
        Ok(())
    }

    /// The computers' captains as saved (CAICaptain, `AI-C`, two a computer in the
    /// computers' order, its kind 2 the attack's and 1 the defense's): each its units
    /// (the owners, on their first lists, of its chain of unit references `uref` from
    /// the head its record names), where it is bound (an attack's goal, the defense's
    /// home) and where it stands, and the unit among its slots (read: the hero map's
    /// undead computer's attack captain at 300 seconds, five fiends and ghouls marching
    /// on the human town hall, its script's target too, the hall's place its goal; its
    /// defense captain's its home).  Provisional: the slots' other meanings, its counts.
    fn restore_captains(&mut self, objects: &[Vec<w3_formats::w3z::SavedObject>], units: &std::collections::HashMap<(u32, u32), u32>) {
        use w3_formats::w3z::{First, Second};
        let all: std::collections::HashMap<u32, &w3_formats::w3z::SavedObject> = objects.iter().flatten().map(|o| (o.entry.tag, o)).collect();
        // (A unit reference's next, and its unit: its owner on its first list.)
        let next = |t: u32| match all.get(&t).map(|o| o.second()) {
            Some(Ok(Second::UnitRefs { words, .. })) => Some((words[1], words[2])),
            _ => None,
        };
        let owner = |t: u32| match all.get(&t).map(|o| o.first()) {
            Some(Ok(First::Agent { object, .. })) => Some((object.links[0].3 .0, object.links[0].3 .1)),
            _ => None,
        };
        let word = |f: &Field| match f {
            Field::Word(w) => Some(*w),
            _ => None,
        };
        let real = |f: &Field| match f {
            Field::Real(r) => Some(*r),
            _ => None,
        };
        let entry = |f: &Field| match f {
            Field::List(g) => g.first().and_then(|g| Some((word(g.first()?)?, word(g.get(1)?)?))),
            _ => None,
        };
        let mut captains: Vec<(u32, Vec<u32>, (f32, f32), (f32, f32), Option<u32>, u32)> = Vec::new();
        for o in objects.iter().flatten().filter(|o| &o.entry.kind.to_be_bytes() == crate::codes::class::AI_CAPTAIN) {
            let Ok(Second::Agent { tail, .. }) = o.second() else { continue };
            let n = tail.len();
            if n < 57 {
                continue;
            }
            let kind = word(&tail[n - 53]).unwrap_or(0);
            let mut members = Vec::new();
            let mut at = entry(&tail[n - 49]);
            while let Some((t, _)) = at.filter(|a| a.0 != u32::MAX) {
                if let Some(h) = owner(t).and_then(|u| units.get(&u)) {
                    members.push(*h);
                }
                at = next(t);
                if members.len() > 1000 {
                    break;
                }
            }
            let bound = (real(&tail[n - 42]).unwrap_or(0.0), real(&tail[n - 41]).unwrap_or(0.0));
            let place = (real(&tail[n - 33]).unwrap_or(bound.0), real(&tail[n - 32]).unwrap_or(bound.1));
            let target = tail[n - 31..n - 12].iter().filter_map(entry).find_map(|r| units.get(&r).copied());
            let order = word(&tail[n - 51]).unwrap_or(0);
            captains.push((kind, members, bound, place, target, order));
        }
        let mut computers: Vec<u8> = self.world.ai.keys().copied().collect();
        computers.sort();
        for (p, pair) in computers.into_iter().zip(captains.chunks(2)) {
            let Some(st) = self.world.ai.get_mut(&p) else { continue };
            for (kind, members, bound, place, target, order) in pair {
                let c = if *kind == 2 { &mut st.attack } else { &mut st.defense };
                c.units = members.clone();
                c.peak = members.len();
                // (An attack captain's order: attacking, AImove, to its goal; going home,
                // move, its goal its home, as CaptainGoHome leaves it: read, the hero
                // map's at 300 and 900 seconds AImove on the human town, at 450 move to
                // where it stands.)
                if *kind == 2 {
                    c.goal = Some(*bound);
                    c.retreating = *order == crate::natives::basic::order_id("move") as u32;
                    if c.retreating {
                        c.home = *bound;
                    }
                } else {
                    c.home = *bound;
                }
                c.place = Some(*place);
                if let Some(t) = target {
                    c.target = *t;
                }
            }
        }
    }

    /// The map script's code block as saved (`FUN_6f910730`).
    pub fn saved_code(&self) -> Code {
        code_of(&self.vm)
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
            // (W3Z_NORETURN: none taken, to test what the original makes of it.)
            let word = old.iter().find(|o| o.machine == t.machine).and_then(|o| o.frames.last()).and_then(|f| f.arguments.last()).map(|v| v.value.clone()).filter(|_| std::env::var_os("W3Z_NORETURN").is_none());
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
        // The AI scripts' (when the port runs any: else the template's stay).
        let (codes, machines) = self.saved_ai_scripts();
        if !machines.is_empty() {
            d.code.truncate(1);
            d.code.extend(codes.into_iter().map(Some));
            d.code_places = d.code.len() as u32 + 1;
            d.machines.truncate(1);
            d.threads.truncate(1);
            for (m, t) in machines {
                d.machines.push(m);
                d.threads.push(t);
            }
        }
        d
    }

    /// A save made from `template` (the original's save of the same moment) with the
    /// pieces the port makes in place of its own: the world's, and the script data.  Its
    /// objects' word is their sum as written (the template's while a record is not read).
    pub fn write_save(&self, template: &SaveGame) -> SaveGame {
        let game = self.world.game_object();
        let mut s = self.world.write_save(template, vec![(game.0, self.game_state_second())]);
        s.objects_word = s.objects_sum().unwrap_or(s.objects_word);
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

    /// The game as a saved game in the original's format (`.w3z`, its recorded-game
    /// container with the header the original's 1.27 writes; a Reign of Chaos game's
    /// product as its replays'), and the port's own state (`port`) as its sidecar
    /// (`save::sidecar`).  Its header from the game's record: the map's path and
    /// checksums, and a network game's description (`Replay::network_head`), else a
    /// game played alone's (the local player's name, the speed fast).
    pub fn save_files(&self, port: Vec<u8>) -> anyhow::Result<(Vec<u8>, Vec<u8>)> {
        use anyhow::Context;
        let record = self.recorder.as_ref().map(|r| &r.replay);
        let ms = record.and_then(|r| r.map_stat.clone()).context("the map's checksums are not known")?;
        let expansion = record.is_none_or(|r| r.version != 0);
        let network = record.and_then(|r| r.network_head());
        let stat = match &network {
            Some(n) => n.stat.clone(),
            None => {
                let host = self.world.players.get(self.world.local_player as usize).map_or_else(String::new, |p| p.name.clone());
                w3_net::GameStat { settings: 2, map_width: ms.width, map_height: ms.height, map_xoro: ms.xoro, map_path: ms.path.clone(), host, map_sha1: ms.sha1 }.to_bytes()
            }
        };
        let head = SaveHead { map: ms.path, stat, xoro: ms.xoro, sha1: ms.sha1, network };
        let header = w3_formats::container::Header { product: if expansion { *b"PX3W" } else { *b"3RAW" }, version: 27, build: 6059, flags: 0, length_ms: 0 };
        let w3z = self.write_save_fresh(&head).to_file(&header);
        let side = crate::save::sidecar(&w3z, &port);
        Ok((w3z, side))
    }

    /// A save from the port's state alone, without a template: the header (`head`'s map
    /// and description, the slot table, the local player), the object manager (its
    /// tables, the main one's from the tags and the second the classes' definitions and
    /// the camera's; its free lists, classes, clocks, the path queues as a game's start
    /// has them, the map's bounds and 522.6667), every object's records, and the game's
    /// state, its script data the map script's and the computers' AI scripts'
    /// (`saved_script_data`), and the objects' word, their sum (`FUN_6f324930`) over
    /// the records written.  Provisional: the clocks' counts (0), the fog's third grid
    /// and the doodads' bytes (0).
    pub fn write_save_fresh(&self, head: &SaveHead) -> SaveGame {
        use w3_formats::w3z::{GameState, ObjectManager, SavedObject};
        let w = &self.world;
        let blank = |entry| SavedObject { entry, first: Vec::new(), second: Vec::new() };
        let mut second: Vec<SavedObject> = crate::w3zsave::class_definitions().into_iter().map(|o| blank(o.entry)).collect();
        second.extend(w.camera_records().into_iter().map(|(e, _, _)| blank(e)));
        second.extend(w.camera_setup_entries().into_iter().map(blank));
        let births = second.iter().map(|o| o.entry.birth + 1).max().unwrap_or(0);
        let objects = [w.saved_table().into_iter().map(blank).collect(), second];
        let time = w.time;
        let mut queues = Vec::new();
        for _ in 0..64 {
            for v in [0u32, 0, 0, u32::MAX, u32::MAX, u32::MAX, u32::MAX] {
                queues.extend(v.to_le_bytes());
            }
        }
        let b = w.map_bounds;
        let mut m = ObjectManager {
            header: w.saved_manager_header(births),
            free: [Vec::new(), u32::MAX.to_le_bytes().to_vec()],
            classes: crate::w3zsave::saved_classes(),
            clocks: w.saved_clocks(time),
            block: vec![0; 256],
            data: [Vec::new(), queues, Vec::new(), Vec::new(), Vec::new(), Vec::new()],
            bounds: [b.min_x, b.min_y, b.max_x, b.max_y, 522.6667],
            ..Default::default()
        };
        m.set_objects(&objects);
        let local = w.local_player;
        let skeleton = SaveGame {
            map: head.map.clone(),
            // (A game played alone's "Local Game", 12, and 1; a network game's its name
            // and words, and its slot table as it came: read, the original's save of a
            // LAN game.)
            info_name: head.network.as_ref().map_or_else(|| "Local Game".into(), |n| n.name.clone()),
            info_stat: head.stat.clone(),
            info_words: head.network.as_ref().map_or([12, 1], |n| n.words),
            slots: head.network.as_ref().and_then(|n| n.slots.clone()).unwrap_or_else(|| w.saved_slot_table()),
            map_xoro: head.xoro,
            map_sha1: head.sha1,
            local_slot: head.network.as_ref().and_then(|n| n.local_slot).unwrap_or_else(|| w.players.iter().take(12).filter(|p| p.slot_state != crate::types::slot_state::EMPTY).position(|p| p.id == local).map_or(-1, |k| k as i32)),
            local_player: local as u32,
            player_name: w.players.get(local as usize).map_or_else(String::new, |p| p.name.clone()),
            preferences: [1, 1],
            game_ms: w.game_ms,
            objects: Some(m),
            ..Default::default()
        };
        let game = w.game_object();
        let mut s = w.write_save(&skeleton, vec![(game.0, self.game_state_second())]);
        // W3Z_COVERAGE: the objects left without records (none made for their kind yet).
        if std::env::var("W3Z_COVERAGE").is_ok() {
            if let Some(Ok(objects)) = s.objects.as_ref().map(|m| m.objects()) {
                let mut empty: std::collections::BTreeMap<String, (u32, u32)> = Default::default();
                for o in objects.iter().flatten() {
                    let k = String::from_utf8_lossy(&o.entry.kind.to_be_bytes()).into_owned();
                    let e = empty.entry(k).or_default();
                    e.0 += u32::from(o.first.is_empty());
                    e.1 += u32::from(o.second.is_empty() && !matches!(&o.entry.family.to_be_bytes(), b"^pro" | b"^lis" | b"^mod" | b"^pma"));
                }
                empty.retain(|_, e| e.0 + e.1 > 0);
                eprintln!("fresh: objects without records by kind (first, second): {empty:?}");
            }
        }
        // The objects the port makes no records for yet (a walk's order, tasks, cluster,
        // and path; a computer's captains): left out, their slots free, their units
        // standing as their records have them (provisional).
        if let Some(m) = s.objects.as_mut() {
            if let Ok(mut objects) = m.objects() {
                let mut freed: Vec<u32> = Vec::new();
                objects[0].retain(|o| {
                    let keep = !o.first.is_empty() || matches!(&o.entry.kind.to_be_bytes(), crate::codes::class::ACCELERATOR_GRID | crate::codes::class::PATHING_MAP);
                    if !keep {
                        freed.push(o.entry.tag);
                    }
                    keep
                });
                if !freed.is_empty() {
                    let mut free = m.free[0].clone();
                    free.truncate(free.len().saturating_sub(4));
                    for slot in freed {
                        free.extend(slot.to_le_bytes());
                    }
                    free.extend(u32::MAX.to_le_bytes());
                    m.free[0] = free;
                    m.set_objects(&objects);
                }
            }
        }
        // The objects' word, their sum over the records as written (0 while a record is
        // not read).
        s.objects_word = s.objects_sum().unwrap_or(0);
        let Ok(script) = self.saved_script_data(&ScriptData::default()).to_bytes() else { return s };
        let parts: Vec<(&'static str, Vec<Field>)> = vec![
            ("game", w.saved_game_part()),
            ("map setup", self.saved_map_setup(head.network.as_ref())),
            ("fog", w.saved_fog(&[]).unwrap_or_default()),
            ("game 2", w.saved_game_2(&[])),
            ("game 3", w.saved_game_3(&[])),
            ("terrain", w.saved_terrain(&[]).unwrap_or_default()),
            ("script data", vec![Field::Bytes(script)]),
            ("hashtables", vec![Field::List(Vec::new()), Field::Word(0)]),
            ("references", w.saved_sites()),
            ("sounds", w.saved_sounds()),
            ("ui", w.saved_ui()),
        ];
        if let Ok(rest) = (GameState { parts, rest: Vec::new() }).to_bytes() {
            s.rest = rest;
        }
        s
    }

    /// The JASS handle table (CGameState's, `FUN_6f282260`), by handle value from
    /// 0x100000: each handle's references, its agent, and a word (1 for events, timers,
    /// locations, and sounds).  Its references (read against the quiet test map's): its
    /// object's own while it lasts; each global holding it, but for a value its
    /// initializer set; each returned function's local that held it (`leak_counts`); and
    /// each local of a thread asleep or saving.  A destroyed object's handle keeps its
    /// agent's slot while references remain.  The engine holds the expired timer of a
    /// callback asleep or saving besides (`context_timers`).  Each row: the count,
    /// the agent (none once destroyed), the word, and the agent's birth.
    pub fn handle_table(&self) -> Vec<(u32, Option<(u32, u32)>, u32, Option<u32>)> {
        use crate::types::Obj;
        use w3_jass::vm::ty;
        let w = &self.world;
        let mut refs: std::collections::HashMap<u32, u32> = Default::default();
        let mut add = |h: u32, n: u32| *refs.entry(h).or_default() += n;
        let var = |v: &Var, add: &mut dyn FnMut(u32, u32)| {
            if v.decl == ty::HANDLE && v.value != 0 {
                add(v.value, 1);
            }
            if v.kind == ty::HANDLE_ARRAY {
                v.array.iter().flatten().filter(|&&h| h != 0).for_each(|&h| add(h, 1));
            }
        };
        for (k, v) in self.vm.vars.iter().enumerate() {
            if !self.vm.init_set.contains(&(k as u32)) {
                var(v, &mut add);
            }
        }
        let mut threads: Vec<&w3_jass::Thread> = w.sleepers.iter().map(|s| &s.thread).collect();
        if let Some(t) = self.vm.snapshot.as_ref().filter(|t| !threads.iter().any(|o| o.place == t.place)) {
            threads.push(t);
        }
        for t in threads {
            for f in &t.frames {
                for v in &f.locals {
                    var(v, &mut add);
                }
            }
        }
        for (&h, &n) in &self.vm.leak_counts {
            add(h, n);
        }
        for h in self.context_timers() {
            add(h, 1);
        }
        let mut by_handle: std::collections::BTreeMap<u32, u32> = Default::default();
        for id in w.handles.ids() {
            if let Some(h) = w.handles.known_handle(id) {
                by_handle.insert(h, id);
            }
        }
        let last = by_handle.keys().chain(refs.keys()).copied().max().unwrap_or(crate::handles::HANDLE_BASE);
        let tagged = |s: u32| (s, w.tags.live.get(&s).map_or(0, |e| e.1));
        (crate::handles::HANDLE_BASE..=last)
            .map(|h| {
                let held = refs.get(&h).copied().unwrap_or(0);
                let Some(&id) = by_handle.get(&h) else {
                    let dead = w.tags.dead_handles.get(&h).filter(|_| held > 0);
                    return (held, None, dead.map_or(0, |d| d.1), dead.map(|d| d.0 .1));
                };
                let o = w.handles.get(id);
                if std::env::var("W3Z_HTOBJ").ok().and_then(|v| u32::from_str_radix(v.trim_start_matches("0x"), 16).ok()) == Some(h) {
                    eprintln!("handle {h:#x}: {o:?}");
                }
                let agent = match o {
                    Some(Obj::Unit(u)) => u.extra.object_slots[0].first().copied(),
                    Some(Obj::Destructable(d)) => Some(d.oid),
                    Some(Obj::Player(p)) => w.tags.player_slots.get(*p as usize).map(|&s| tagged(s)),
                    Some(Obj::Trigger(t)) => t.slots.first().map(|&s| tagged(s)),
                    Some(Obj::TriggerAction { slot, .. }) => Some(tagged(*slot)),
                    Some(Obj::Force(f)) => f.slots.first().map(|&s| tagged(s)),
                    Some(Obj::Region(g)) => g.objects.map(|o| o[0]),
                    _ => w.tags.handle_ids.get(&id).copied().or_else(|| w.tags.handle_slots.get(&id).map(|&s| tagged(s))),
                };
                let word = u32::from(matches!(o, Some(Obj::Event(_) | Obj::Timer(_) | Obj::Location(..) | Obj::Sound(_))));
                (held + 1, agent, word, agent.map(|a| a.1))
            })
            .collect()
    }

    /// The game's own record (CGameState's, `FUN_6f28aad0`, own `FUN_6f28b190`; read
    /// against the quiet test map's): 0, the trigger queue, 0.0; its keyed handlers
    /// (`World::game_keyed_handlers`); ten event lists, the first each neutral
    /// building's (Aneu, event 0xd01a6); fifteen lists of handles, the sixth the
    /// expired timers callbacks hold (`context_timers`); none; 4; no tables; the time
    /// of day's modifier and its day's end listener, the day's change's two; the handle
    /// table (`handle_table`, each a size, 4 and 16 a row, and its rows: the count, the
    /// agent, and the word; then the agent, its birth, and the handle from the first
    /// handle on); none; the trigger queue; 0.0, 0; the game's two periodic timers
    /// (every 0.4 and every agent step, events 0x80269 and 0x8026a, from the setup's
    /// end, due after the clock SaveGame was called at); 0, the hundred words 0 to 99,
    /// 100, 0, 0, and none.  Provisional: the second list, the fifteen lists' others.
    pub fn game_state_second(&self) -> w3_formats::w3z::Second {
        use w3_formats::w3z::{Handlers, Ref, Second, Timing};
        let w = &self.world;
        let game = w.game_object();
        let rf = |r: (u32, u32)| Ref(r.0, r.1);
        let none = Ref(u32::MAX, u32::MAX);
        let qum = w.first_of(crate::codes::class::TRIGGER_QUEUE);
        let own = vec![Field::Word(0), Field::Word(qum.0), Field::Word(qum.1), Field::Real(0.0)];
        let mut shops: Vec<(u32, u32)> = w.tags.live.iter().filter(|(_, (c, _))| c == crate::codes::ability::id::NEUTRAL_BUILDING).map(|(&s, (_, b))| (s, *b)).collect();
        shops.sort_by_key(|r| r.1);
        // (The neutral buildings' on its event list for 0xd01a6 too.)
        let mut events = vec![Vec::new(); 0x400];
        events[0x1a6] = shops.iter().map(|&r| (rf(r), 0xd_01a6)).collect();
        let handlers = Handlers { keyed: w.game_keyed_handlers(), fixed: Default::default(), events };
        let mut tail = Vec::new();
        tail.push(Field::List(shops.iter().map(|&r| vec![Field::Ref(rf(r)), Field::Word(0xd_01a6)]).collect()));
        tail.extend((1..10).map(|_| Field::List(Vec::new())));
        let mut timers = self.context_timers();
        timers.sort();
        timers.dedup();
        tail.push(Field::List((0..15).map(|k| vec![Field::List(if k == 5 { timers.iter().map(|&h| vec![Field::Word(h)]).collect() } else { Vec::new() })]).collect()));
        tail.extend([Field::List(Vec::new()), Field::Word(4), Field::List(Vec::new())]);
        let live = |s: Option<u32>| s.filter(|s| w.tags.live.contains_key(s)).map_or(none, |s| Ref(s, w.tags.live[&s].1));
        tail.extend([live(w.time_of_day_listeners[0]), live(w.time_of_day_listeners[1]), live(w.tags.day_timer), live(w.tags.day_timer_2)].map(Field::Ref));
        let table = self.handle_table();
        tail.push(Field::Word(4 + 16 * table.len() as u32));
        tail.push(Field::List(table.iter().map(|r| vec![Field::Word(r.0), Field::Ref(r.1.map_or(none, rf)), Field::Word(r.2)]).collect()));
        tail.push(Field::Word(4 + 16 * table.len().saturating_sub(1) as u32));
        // (In the order the handles were given.)
        let mut rows: Vec<(u32, &(u32, Option<(u32, u32)>, u32, Option<u32>))> = table.iter().enumerate().skip(1).map(|(k, r)| (crate::handles::HANDLE_BASE + k as u32, r)).collect();
        rows.sort_by_key(|&(h, _)| w.handles.given_order(h));
        tail.push(Field::List(rows.iter().map(|&(h, r)| vec![Field::Ref(r.1.map_or(none, rf)), Field::Word(r.3.unwrap_or(0)), Field::Word(h)]).collect()));
        tail.push(Field::List(Vec::new()));
        // (Then the time of day's suspension, +0x6c: a bit for each who stopped it,
        // SuspendTimeOfDay's 1, `FUN_6f28bb00`; read, Mafarazzo TD's at 30 seconds.)
        tail.extend([Field::Word(qum.0), Field::Word(qum.1), Field::Real(0.0), Field::Word(u32::from(w.time_of_day_suspended))]);
        let now = w.tags.save_clock.unwrap_or(w.tags.clock);
        for (k, (period, event)) in [(0x3ecc_ccce_u32, 0x8_0269), (crate::motion::STEP, 0x8_026a)].into_iter().enumerate() {
            let mut due = crate::efloat::add(w.tags.game_timers_clock, period);
            while f32::from_bits(due) <= f32::from_bits(now) {
                due = crate::efloat::add(due, period);
            }
            tail.push(Field::Timer(rf(game), 2, Timing(Some((f32::from_bits(due), f32::from_bits(period), w.tags.game_timers[k], event, 1)))));
        }
        tail.push(Field::Word(0));
        tail.push(Field::List((0..100).map(|k| vec![Field::Word(k)]).collect()));
        tail.extend([Field::Word(100), Field::Word(0), Field::Word(0), Field::List(Vec::new())]);
        Second::Agent { own, handlers, tail }
    }

    /// The expired timers the engine holds as callbacks' contexts: those of callbacks
    /// asleep or kept within SaveGame (read: the quiet test map's timer whose callback
    /// saves; provisional: one asleep).
    pub fn context_timers(&self) -> Vec<u32> {
        // (Those scripts have had: a timer event's own timer is none of theirs.)
        self.world.sleepers.iter().filter(|s| s.ctx.timer != 0).filter_map(|s| self.world.handles.known_handle(s.ctx.timer)).collect()
    }

    /// The script data's pieces the port makes, against the original's save of the same
    /// moment.
    pub fn compare_script(&self, orig: &SaveGame) -> Vec<String> {
        let mut out = Vec::new();
        // The handle table, against the original's (CGameState's tail 19).
        if let Ok(objects) = orig.objects.as_ref().map_or(Err(()), |m| m.objects().map_err(|_| ())) {
            let kinds: std::collections::HashMap<u32, String> = objects[0].iter().map(|o| (o.entry.tag, String::from_utf8_lossy(&o.entry.kind.to_be_bytes()).to_string())).collect();
            let game = objects[0].iter().find(|o| &o.entry.kind.to_be_bytes() == crate::codes::class::GAME_STATE);
            if let Some(Ok(w3_formats::w3z::Second::Agent { tail, .. })) = game.map(|o| o.second()) {
                if let Some(Field::List(rows)) = tail.get(19) {
                    let ours = self.handle_table();
                    let mut by_kind: std::collections::BTreeMap<String, (u32, u32, u32, u32)> = Default::default();
                    let mut shown = 0;
                    for (k, row) in rows.iter().enumerate() {
                        let [Field::Word(count), Field::Ref(r), Field::Word(word)] = row[..] else { continue };
                        let kind = kinds.get(&r.0).cloned().unwrap_or_else(|| "none".into());
                        let e = by_kind.entry(kind.clone()).or_default();
                        let theirs = (count, (r.0 != u32::MAX).then_some((r.0, r.1)), word);
                        let mine = ours.get(k).map(|r| (r.0, r.1, r.2));
                        if mine == Some(theirs) {
                            e.0 += 1;
                        } else {
                            if mine.map(|m| m.0) != Some(count) { e.1 += 1; }
                            if mine.map(|m| m.1) != Some(theirs.1) { e.2 += 1; }
                            if mine.map(|m| m.2) != Some(word) { e.3 += 1; }
                            if std::env::var("W3Z_ALL").is_ok() && shown < 60 {
                                shown += 1;
                                out.push(format!("  handle {:#x} {kind}: ours {mine:?}, the original's {theirs:?}", 0x10_0000 + k));
                            }
                        }
                    }
                    out.push(format!("handle table: {} ours, {} the original's; by kind (same, count differs, agent differs, word differs) {by_kind:?}", ours.len(), rows.len()));
                    // The game's whole record.
                    if let (Some(o), w3_formats::w3z::Second::Agent { own, handlers, tail: ours_tail }) = (game, self.game_state_second()) {
                        let theirs = o.second().ok();
                        let mine = w3_formats::w3z::Second::Agent { own: own.clone(), handlers: handlers.clone(), tail: ours_tail.clone() };
                        let same = mine.to_bytes(o.entry.kind).is_ok_and(|b| b == o.second);
                        let mut what = Vec::new();
                        if let Some(w3_formats::w3z::Second::Agent { own: o2, handlers: h2, tail: t2 }) = &theirs {
                            if &own != o2 { what.push(format!("own {own:?} vs {o2:?}")); }
                            if handlers.keyed != h2.keyed { what.push(format!("keyed {:?} vs {:?}", handlers.keyed, h2.keyed)); }
                            if handlers.fixed != h2.fixed { what.push("fixed".into()); }
                            if handlers.events != h2.events { what.push(format!("events: theirs non-empty {:?}", h2.events.iter().enumerate().filter(|e| !e.1.is_empty()).map(|e| (e.0, e.1.clone())).collect::<Vec<_>>())); }
                            if let (Some(Field::List(a)), Some(Field::List(b))) = (ours_tail.get(21), t2.get(21)) {
                                for (x, y) in a.iter().zip(b).filter(|(x, y)| x != y).take(8) {
                                    what.push(format!("handle row {x:?} vs {y:?}"));
                                }
                            }
                            for k in 0..ours_tail.len().max(t2.len()) {
                                if ours_tail.get(k) != t2.get(k) {
                                    let show = |f: Option<&Field>| { let t = format!("{f:?}"); t[..t.len().min(300)].to_string() };
                                    what.push(format!("tail {k}: {} vs {}", show(ours_tail.get(k)), show(t2.get(k))));
                                }
                            }
                        }
                        out.push(format!("game's record: {}{}", if same { "the same" } else { "differs" }, what.iter().map(|w| format!("; {w}")).collect::<String>()));
                    }
                    // W3Z_HT: each handle's count, its holders among the globals (by
                    // name), whether a returned function's local kept it, and its kind.
                    if std::env::var("W3Z_HT").is_ok() {
                        let names = self.vm.global_names();
                        let mut holders: std::collections::HashMap<u32, Vec<String>> = Default::default();
                        for (k, v) in self.vm.vars.iter().enumerate() {
                            let name = names.get(k).map(|n| n.to_string()).unwrap_or_default();
                            if v.decl == w3_jass::vm::ty::HANDLE && v.value != 0 {
                                holders.entry(v.value).or_default().push(name.clone());
                            }
                            if v.kind == w3_jass::vm::ty::HANDLE_ARRAY {
                                for (j, &h) in v.array.iter().flatten().enumerate().filter(|x| *x.1 != 0) {
                                    holders.entry(h).or_default().push(format!("{name}[{j}]"));
                                }
                            }
                        }
                        for (k, row) in rows.iter().enumerate() {
                            let [Field::Word(count), Field::Ref(r), Field::Word(word)] = row[..] else { continue };
                            let h = 0x10_0000 + k as u32;
                            let kind = kinds.get(&r.0).cloned().unwrap_or_else(|| "none".into());
                            out.push(format!("ht {h:#x} {kind} count {count} word {word} leaked {} globals {:?} ours {:?}", self.vm.leak_counts.get(&h).copied().unwrap_or(0), holders.get(&h), ours.get(k)));
                        }
                    }
                }
            }
        }
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
        // The AI scripts': code blocks, machines, and threads.
        let (codes, machines) = self.saved_ai_scripts();
        for (k, c) in codes.iter().enumerate() {
            match d.code.get(1 + k) {
                Some(Some(t)) => {
                    let first = c.strings.iter().zip(&t.strings).position(|(a, b)| a != b);
                    let word = c.words.iter().zip(&t.words).position(|(a, b)| a != b);
                    out.push(format!("AI code {}: words {} ours, {} the original's, first differing {word:?}; strings {} and {}, first differing {:?}", k + 2, c.words.len(), t.words.len(), c.strings.len(), t.strings.len(), first.map(|j| (j, &c.strings[j], &t.strings[j]))));
                }
                _ => out.push(format!("AI code {}: none in the original's", k + 2)),
            }
        }
        for (k, ((place, m), threads)) in machines.iter().enumerate() {
            let Some((tp, t)) = d.machines.get(1 + k) else { out.push(format!("AI machine {k}: none in the original's")); continue };
            let dbg = |x: &dyn std::fmt::Debug| format!("{x:?}");
            let globals = m.globals.iter().zip(&t.globals).filter(|(a, b)| dbg(a) != dbg(b)).count();
            let first_global = m.globals.iter().zip(&t.globals).find(|(a, b)| dbg(a) != dbg(b)).map(|(a, b)| format!("{:?} vs {:?}", a, b));
            out.push(format!("AI machine {}: place {place} vs {tp}; code {} vs {}; word {} vs {}; at {} vs {}; functions {} vs {}; globals {} vs {}, {globals} differ{}; stack differs {}; strings {:?} vs {:?}", k + 1, m.code, t.code, m.word, t.word, m.at, t.at, m.functions.len(), t.functions.len(), m.globals.len(), t.globals.len(), first_global.map_or(String::new(), |g| format!(", the first {}", &g[..g.len().min(300)])), dbg(&m.stack) != dbg(&t.stack), m.strings, t.strings));
            if std::env::var("W3Z_ALL").is_ok() {
                for (a, b) in m.globals.iter().zip(&t.globals).filter(|(a, b)| dbg(a) != dbg(b)) {
                    out.push(format!("  AI global {} {:?} vs {:?}", a.0, a.1.value, b.1.value));
                }
            }
            if let Some(theirs) = d.threads.get(1 + k) {
                for (a, b) in threads.iter().zip(theirs) {
                    if std::env::var_os("W3Z_FRAMES").is_some() {
                        for (k, (x, y)) in a.stack.iter().zip(&b.stack).enumerate().filter(|(_, (x, y))| dbg(x) != dbg(y)).take(12) {
                            out.push(format!("    place {} r{k}: ours {x:?} theirs {y:?}", a.machine));
                        }
                    }
                    out.push(format!("  AI thread: place {} vs {}; at {} vs {}; flag {} vs {}; word {} vs {}; frames {} vs {}{}; registers differ {}", a.machine, b.machine, a.at, b.at, a.flag, b.flag, a.word, b.word, a.frames.len(), b.frames.len(), if dbg(&a.frames) == dbg(&b.frames) && std::env::var_os("W3Z_FRAMES").is_none() { String::new() } else { format!(" (ours {:?}, theirs {:?})", a.frames.iter().map(|f| (f.word, f.arguments.clone(), f.locals.clone())).collect::<Vec<_>>(), b.frames.iter().map(|f| (f.word, f.arguments.clone(), f.locals.clone())).collect::<Vec<_>>()) }, a.stack.iter().zip(&b.stack).filter(|(x, y)| dbg(x) != dbg(y)).count()));
                }
                if threads.len() != theirs.len() {
                    out.push(format!("  AI threads: {} ours, {} the original's", threads.len(), theirs.len()));
                }
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
