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

    /// The map script's threads as saved, by place: those asleep (flag 1), those kept
    /// within SaveGame (flag 0), and the one that called it last as it was in the call
    /// (a timer's callback's, say, freed after it).
    pub fn saved_threads(&self) -> Vec<Thread> {
        let mut out: Vec<Thread> = self.world.sleepers.iter().map(|s| self.saved_thread(&s.thread, s.thread.parked.is_none())).collect();
        if let Some(t) = self.vm.snapshot.as_ref().filter(|t| !out.iter().any(|o| o.machine == t.place)) {
            out.push(self.saved_thread(t, false));
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

    /// The JASS handle table (CGameState's, `FUN_6f282260`), by handle value from
    /// 0x100000: each handle's references, its agent, and a word (1 for events, timers,
    /// locations, and sounds).  Its references (read against the quiet test map's): its
    /// object's own while it lasts; each global holding it, but for a value its
    /// initializer set; each returned function's local that held it (`leak_counts`); and
    /// each local of a thread asleep or saving.  A destroyed object's handle keeps its
    /// agent's slot while references remain.  The engine holds the expired timer of a
    /// callback asleep or saving besides (`context_timers`).  Each row: the count,
    /// the agent (none once destroyed), the word, and the agent's slot.
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
                    return (held, None, dead.map_or(0, |d| d.1), dead.map(|d| d.0 .0));
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
                    _ => w.tags.handle_ids.get(&id).copied().or_else(|| w.tags.handle_slots.get(&id).map(|&s| tagged(s))),
                };
                let word = u32::from(matches!(o, Some(Obj::Event(_) | Obj::Timer(_) | Obj::Location(..) | Obj::Sound(_))));
                (held + 1, agent, word, agent.map(|a| a.0))
            })
            .collect()
    }

    /// The expired timers the engine holds as callbacks' contexts: those of callbacks
    /// asleep or kept within SaveGame (read: the quiet test map's timer whose callback
    /// saves; provisional: one asleep).
    pub fn context_timers(&self) -> Vec<u32> {
        self.world.sleepers.iter().map(|s| s.ctx.timer).filter(|&t| t != 0).collect()
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
