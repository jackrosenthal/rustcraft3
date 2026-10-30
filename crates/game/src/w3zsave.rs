//! Saved games as the original writes them (.w3z, `w3_formats::w3z`), from the port's
//! state: built piece by piece, each checked against the original's save of the same
//! moment (`World::compare_save`).

use w3_formats::w3z::{Array, Field, First, Handlers, Link, ObjectBase, PropertyBase, Ref, SaveGame, Second, SavedObject, Sprite, Tagged, Timing, Widget};

use crate::types::{BoolExpr, EventKind, Obj};

use crate::tags::Code;
use crate::world::World;

/// The family a kind of engine object belongs to, as the object tables hold it (read
/// from every save on hand: properties, listeners, timers, and pathing regions under
/// their families, a few kinds their own, the rest agents).
pub fn family_of(kind: &Code) -> Code {
    match kind {
        b"`flg" | b"`pcu" | b"`pos" | b"`rel" => *b"^pro",
        b"`fcl" | b"`pil" | b"`rtl" => *b"^lis",
        b"`rcm" => *b"^mod",
        b"^pma" | b"^pmr" => *b"^pma",
        b"^ama" | b"^clu" | b"l^ac" | b"l^pa" | b"l^sy" | b"^adf" => *kind,
        _ => *b"+agl",
    }
}

/// The engine's agent classes (`docs/reference/w3z_classes.txt`): each its code, its
/// parent's, and its definition's tag and birth, and that definition's array.
struct Class {
    code: u32,
    parent: u32,
    tag: u32,
    birth: u32,
    array: Array<[u32; 11]>,
}

fn classes() -> Vec<Class> {
    let hex = |s: &str| u32::from_str_radix(s, 16).unwrap_or(0);
    let code = |s: &str| u32::from_be_bytes(s.as_bytes().try_into().unwrap_or([0; 4]));
    include_str!("../../../docs/reference/w3z_classes.txt")
        .lines()
        .filter(|l| !l.starts_with('#'))
        .map(|l| {
            let w: Vec<&str> = l.split(' ').collect();
            let mut array = Array { words: [0, 0, 8, 0], entries: Vec::new() };
            if let Some(words) = w.get(4) {
                let v: Vec<u32> = words.split(',').map(hex).collect();
                array.words = [v[0], v[1], v[2], v[3]];
                array.entries = w[5..].iter().map(|e| e.split(',').map(hex).collect::<Vec<u32>>().try_into().unwrap_or([0; 11])).collect();
            }
            Class { code: code(w[0]), parent: code(w[1]), tag: hex(w[2]), birth: w[3].parse().unwrap_or(0), array }
        })
        .collect()
}

/// An object's base as saved for one with no links or timings: its own tag in each
/// of its two link sets.
fn plain_base(tag: u32, birth: u32) -> ObjectBase {
    let none = Ref(u32::MAX, u32::MAX);
    let set = (Link(none, 0), Link(none, 0), Ref(tag, birth), none);
    ObjectBase { real: 0.0, words: [u32::MAX, 0], links: [set, set], timings: [Timing(None), Timing(None)] }
}

/// A class's definition's tag and birth (an agent's first record names it).
fn class_ref(code: &[u8; 4]) -> Ref {
    let c = u32::from_be_bytes(*code);
    classes().into_iter().find(|k| k.code == c).map_or(Ref(u32::MAX, u32::MAX), |k| Ref(k.tag, k.birth))
}

/// The registry as saved: each class's code, parent, and definition's tag and birth.
pub fn saved_classes() -> Vec<[u32; 4]> {
    classes().iter().map(|c| [c.code, c.parent, c.tag, c.birth]).collect()
}

/// The classes' definitions (`^adf`, CRlAgentDef), as the second table holds them.
pub fn class_definitions() -> Vec<SavedObject> {
    let adf = u32::from_be_bytes(*b"^adf");
    let mut defs: Vec<SavedObject> = classes()
        .into_iter()
        .map(|c| SavedObject {
            entry: Tagged { family: adf, kind: adf, tag: c.tag, birth: c.birth },
            first: First::AgentDef { object: plain_base(c.tag, c.birth), array: c.array }.to_bytes(),
            second: Vec::new(),
        })
        .collect();
    defs.sort_by_key(|d| d.entry.tag);
    defs
}

/// A property's base as saved: its object's words 0 and 0x80000000, its own tag and
/// its owner's in its first link set, its own in its second.
fn property_base(tag: (u32, u32), owner: (u32, u32)) -> PropertyBase {
    let none = Ref(u32::MAX, u32::MAX);
    let object = ObjectBase {
        real: 0.0,
        words: [0, 0x8000_0000],
        links: [(Link(none, 0), Link(none, 0), Ref(tag.0, tag.1), Ref(owner.0, owner.1)), (Link(none, 0), Link(none, 0), Ref(tag.0, tag.1), none)],
        timings: [Timing(None), Timing(None)],
    };
    PropertyBase { object, links: [Link(none, 0), Link(none, 0)], real: 0.0, word: 0 }
}

const NONE: Ref = Ref(u32::MAX, u32::MAX);

fn rf(t: (u32, u32)) -> Ref {
    Ref(t.0, t.1)
}

/// An agent's first record (CAgentBaseAbs, `FUN_6f146480`): its base, its class's
/// definition, its properties in order, and the last of them as its second link
/// (read: a tree's position, a trigger's two flags).
fn agent_first(own: (u32, u32), class: &[u8; 4], props: &[(u32, u32)]) -> First {
    let last = props.last().map_or(Link(NONE, 0), |&p| Link(rf(p), 0));
    First::Agent { object: plain_base(own.0, own.1), agent: class_ref(class), refs: props.iter().map(|&p| rf(p)).collect(), links: [Link(NONE, 0), last, Link(NONE, 0), Link(NONE, 0)] }
}

/// The base of an agent's `k`th property: its index, the next and the one before in its
/// first link set with itself and its owner, itself in its second.
fn listed_property_base(props: &[(u32, u32)], k: usize, owner: (u32, u32)) -> PropertyBase {
    let next = props.get(k + 1).map_or(NONE, |&p| rf(p));
    let prev = if k > 0 { rf(props[k - 1]) } else { NONE };
    let mut base = property_base(props[k], owner);
    base.object.words[0] = k as u32;
    base.object.links[0].0 = Link(next, 0);
    base.object.links[0].1 = Link(prev, 0);
    base
}

/// An agent's handler lists with only the game's for its event 0x80267 (read: a tree's,
/// a handler's).
fn game_handlers(game: (u32, u32)) -> Handlers {
    let mut fixed: [Vec<(Ref, u32)>; 5] = Default::default();
    fixed[0].push((rf(game), 0x8_0267));
    Handlers { keyed: Vec::new(), fixed, events: vec![Vec::new(); 0x400] }
}

/// A flag listener (`` `fcl ``): itself, its owner, the flag it listens to, and its word.
#[derive(Clone, Copy, Debug)]
pub struct Listener {
    pub me: (u32, u32),
    pub owner: (u32, u32),
    pub flag: (u32, u32),
    pub word: u32,
}

/// The local player's interface's flag listeners, in the second table (the resource
/// bar's), as a save holds them, in the order made.
pub fn client_listeners(save: &SaveGame) -> Vec<Listener> {
    let Some(m) = save.objects.as_ref() else { return Vec::new() };
    let Ok(objects) = m.objects() else { return Vec::new() };
    let mut out: Vec<Listener> = objects[1]
        .iter()
        .filter(|o| &o.entry.kind.to_be_bytes() == b"`fcl")
        .filter_map(|o| match o.first() {
            Ok(First::FlagListener { object, word }) => Some(Listener { me: (o.entry.tag, o.entry.birth), owner: (object.links[0].3 .0, object.links[0].3 .1), flag: (object.links[1].3 .0, object.links[1].3 .1), word }),
            _ => None,
        })
        .collect();
    out.sort_by_key(|l| l.me.1);
    out
}

/// A chain of listeners, newest first from its head: each one's link to the one made
/// after it and to the one before, in its link set `set`.
fn chain_links(chain: &[(u32, u32)], me: (u32, u32), set: u32) -> (Link, Link) {
    let k = chain.iter().position(|&c| c == me).unwrap_or(0);
    let after = chain.get(k + 1).map_or(Link(NONE, 0), |&c| Link(rf(c), set));
    let before = if k > 0 { Link(rf(chain[k - 1]), set) } else { Link(NONE, 0) };
    (after, before)
}

impl World {
    /// The players' records and their properties' (read against the quiet test map's,
    /// at 5 seconds): each player's 26 flags, its states by number (gold and lumber in
    /// tenths), its alliances' 11 (its number, then each alliance type's mask of the
    /// players it is set toward), and the flag listeners: each player's own on its food
    /// used, the registrations' of alliance changes (owned by the alliances, on the
    /// type's flag) and player states (owned by the player, on the state's flag), and
    /// the interface's, `client`, made after the rest (provisional).  An owner's
    /// listeners and a flag's chain newest first, the owner's agent's fourth link and
    /// the flag's second its head.
    pub fn player_records(&self, client: &[Listener]) -> Vec<(u32, First, Option<Second>)> {
        let mut out = Vec::new();
        let mut listeners: Vec<Listener> = Vec::new();
        let mut alliance_regs: Vec<((u32, u32), Listener)> = Vec::new();
        for (i, &ps) in self.tags.player_slots.iter().enumerate() {
            let _ = i;
            listeners.push(Listener { me: self.tagged(ps + 27), owner: self.tagged(ps), flag: self.tagged(ps + 6), word: 6 });
        }
        for id in self.handles.ids() {
            let Some(Obj::Event(r)) = self.handles.get(id) else { continue };
            let Some(&l) = self.tags.event_timers.get(&id) else { continue };
            let Some(&ps) = (match r.kind {
                EventKind::Player { player, .. } | EventKind::PlayerState { player, .. } => self.tags.player_slots.get(player as usize),
                _ => None,
            }) else {
                continue;
            };
            match r.kind {
                EventKind::Player { event, alliance, .. } if event == crate::event_ids::EVENT_PLAYER_ALLIANCE_CHANGED => {
                    let li = Listener { me: self.tagged(l), owner: self.tagged(ps + 28), flag: self.tagged(ps + 29 + 1 + alliance as u32), word: 0x8_0268 };
                    listeners.push(li);
                    if let Some(&reg) = self.tags.handle_ids.get(&id) {
                        alliance_regs.push((reg, li));
                    }
                }
                EventKind::PlayerState { state, .. } => {
                    listeners.push(Listener { me: self.tagged(l), owner: self.tagged(ps), flag: self.tagged(ps + 1 + state as u32), word: 0 });
                }
                _ => {}
            }
        }
        listeners.sort_by_key(|l| l.me.1);
        listeners.extend_from_slice(client);
        let owned = |o: (u32, u32)| listeners.iter().filter(|l| l.owner == o).map(|l| l.me).collect::<Vec<_>>();
        let on = |f: (u32, u32)| listeners.iter().filter(|l| l.flag == f).map(|l| l.me).collect::<Vec<_>>();
        for l in &listeners {
            if l.me.0 & 0x8000_0000 != 0 {
                continue;
            }
            let (a0, b0) = chain_links(&owned(l.owner), l.me, 0);
            let (a1, b1) = chain_links(&on(l.flag), l.me, 1);
            let object = ObjectBase {
                real: 0.0,
                words: [l.word, 0x2000_0400],
                links: [(a0, b0, rf(l.me), rf(l.owner)), (a1, b1, rf(l.me), rf(l.flag))],
                timings: [Timing(None), Timing(None)],
            };
            out.push((l.me.0, First::FlagListener { object, word: u32::MAX }, None));
        }
        let flag_records = |out: &mut Vec<(u32, First, Option<Second>)>, owner: (u32, u32), flags: &[(u32, u32)], values: &[u32]| {
            for (k, &f) in flags.iter().enumerate() {
                let mut base = listed_property_base(flags, k, owner);
                if let Some(&head) = on(f).last() {
                    base.links[1] = Link(rf(head), 1);
                }
                out.push((f.0, First::Flag { base, value: values.get(k).copied().unwrap_or(0) }, None));
            }
        };
        for (i, &ps) in self.tags.player_slots.iter().enumerate() {
            let Some(p) = self.players.get(i) else { continue };
            let me = self.tagged(ps);
            let flags: Vec<(u32, u32)> = (1..=26).map(|k| self.tagged(ps + k)).collect();
            // (The upkeep rates the port derives from food used as they are asked for.)
            let (gold_tax, lumber_tax) = self.upkeep_tax(i as u8);
            let values: Vec<u32> = (0..26)
                .map(|k| match k {
                    1 | 2 => p.get_state(k) * 10,
                    crate::types::player_state::GOLD_UPKEEP_RATE => gold_tax,
                    crate::types::player_state::LUMBER_UPKEEP_RATE => lumber_tax,
                    _ => p.get_state(k),
                } as u32)
                .collect();
            flag_records(&mut out, me, &flags, &values);
            let mut first = agent_first(me, b"+ply", &flags);
            if let (First::Agent { links, .. }, Some(&head)) = (&mut first, owned(me).last()) {
                links[3] = Link(rf(head), 0);
            }
            out.push((me.0, first, None));
            let ally = self.tagged(ps + 28);
            let aflags: Vec<(u32, u32)> = (29..=39).map(|k| self.tagged(ps + k)).collect();
            let mut avalues = vec![i as u32];
            for t in 0..crate::types::ALLIANCE_TYPES {
                avalues.push(p.alliances.iter().enumerate().filter(|(_, a)| a[t]).fold(0u32, |m, (j, _)| m | 1 << j));
            }
            flag_records(&mut out, ally, &aflags, &avalues);
            let mut first = agent_first(ally, b"ally", &aflags);
            if let (First::Agent { links, .. }, Some(&head)) = (&mut first, owned(ally).last()) {
                links[3] = Link(rf(head), 0);
            }
            // (Its handlers: each alliance registration's on event 0x80268, keyed by its
            // listener, and not the game's.)
            let keyed = alliance_regs.iter().filter(|(_, l)| l.owner == ally).map(|(reg, l)| (l.me.0, vec![(rf(*reg), 0x8_0268)])).collect();
            let handlers = Handlers { keyed, fixed: Default::default(), events: vec![Vec::new(); 0x400] };
            out.push((ally.0, first, Some(Second::Agent { own: vec![Field::Word(0)], handlers, tail: Vec::new() })));
        }
        out
    }

    /// The records of the script's triggers, their flags, actions, and handlers, by
    /// slot (read against the quiet test map's): a trigger's agent (its two flags, the
    /// second linked) and its own (CTriggerWar3: its word, its evaluations and
    /// executions, two words, and whether it waits on sleeps; its events', conditions',
    /// and actions' lists); its flags (2, then 1 while enabled); an action's its
    /// function's handler; a handler's its number.
    pub fn trigger_records(&self) -> Vec<(u32, First, Option<Second>)> {
        let mut out = Vec::new();
        let no_handlers = || game_handlers(self.game_object());
        // (A timer's own handler has no game registration: read, the test map's.)
        let timers: std::collections::HashSet<u32> = self.handles.ids().into_iter().filter_map(|id| match self.handles.get(id) {
            Some(Obj::Timer(t)) => t.handler,
            _ => None,
        }).collect();
        for (slot, func) in &self.tags.handler_objects {
            if self.tags.live.get(slot).is_none_or(|e| &e.0 != b"hdlr") {
                continue;
            }
            let own = self.tagged(*slot);
            let k = self.tags.function_numbers.get(func).copied().unwrap_or(0);
            let handlers = if timers.contains(slot) { Handlers { keyed: Vec::new(), fixed: Default::default(), events: vec![Vec::new(); 0x400] } } else { no_handlers() };
            out.push((own.0, agent_first(own, b"hdlr", &[]), Some(Second::Agent { own: vec![Field::Word(0)], handlers, tail: vec![Field::Word(k)] })));
        }
        for id in self.handles.ids() {
            match self.handles.get(id) {
                Some(Obj::Trigger(t)) if t.slots.len() == 3 => {
                    let own = self.tagged(t.slots[0]);
                    let flags = [self.tagged(t.slots[1]), self.tagged(t.slots[2])];
                    // (Registrations of timers' expiry, dialogs, their buttons, and
                    // trackables are not on their triggers' lists: their natives make them
                    // without `FUN_6f296670`.)
                    let unlisted = |h: &u32| matches!(self.handles.get(*h), Some(Obj::Event(r)) if matches!(r.kind, EventKind::TimerExpire(_) | EventKind::Dialog(_) | EventKind::DialogButton(_) | EventKind::Trackable { .. }));
                    let listed = |hs: &[u32]| Field::List(hs.iter().filter(|h| !unlisted(h)).filter_map(|h| self.tags.handle_ids.get(h)).map(|&(s, b)| vec![Field::Ref(Ref(s, b))]).collect());
                    let actions = Field::List(
                        t.actions
                            .iter()
                            .filter_map(|&a| match self.handles.get(a) {
                                Some(Obj::TriggerAction { slot, .. }) if *slot != 0 => Some(vec![Field::Ref(rf(self.tagged(*slot)))]),
                                _ => None,
                            })
                            .collect(),
                    );
                    // (Provisional: the fifth word, 1 for the trigger the map ran at its
                    // start, taken as its executions.)
                    let words = [0, t.eval_count as u32, t.exec_count as u32, 0, t.exec_count as u32, u32::from(t.wait_on_sleeps)];
                    let second = Second::Agent { own: words.iter().map(|&w| Field::Word(w)).collect(), handlers: no_handlers(), tail: vec![listed(&t.events), listed(&t.conditions), actions] };
                    out.push((own.0, agent_first(own, b"+trg", &flags), Some(second)));
                    // (Provisional: which of the two is the enabled flag, every trigger read
                    // being enabled.)
                    for (k, v) in [2, u32::from(t.enabled)].into_iter().enumerate() {
                        out.push((flags[k].0, First::Flag { base: listed_property_base(&flags, k, own), value: v }, None));
                    }
                }
                Some(Obj::TriggerAction { func, slot }) if *slot != 0 => {
                    let own = self.tagged(*slot);
                    let handler = self.tags.handler_objects.iter().find(|(_, f)| *f == func).map_or(NONE, |(&s, _)| rf(self.tagged(s)));
                    out.push((own.0, agent_first(own, b"+tac", &[]), Some(Second::Agent { own: vec![Field::Word(0)], handlers: no_handlers(), tail: vec![Field::Ref(handler)] })));
                }
                _ => {}
            }
        }
        out
    }

    /// The game's object (`+gam`).
    fn game_object(&self) -> (u32, u32) {
        self.tags.live.iter().find(|(_, (c, _))| c == b"+gam").map_or((0, 0), |(&s, (_, b))| (s, *b))
    }

    /// The records of the script's event registrations, filters, and locations, by
    /// slot (read against the quiet test map's): a registration's own starts with four
    /// words (0, -1, -1, 0: provisional, all read so), then its kind's: the engine's
    /// event id (the script's plus 0x80200), the unit or player named by its slot and
    /// birth (+0x14, +0x18), its filter, and its handler list, its trigger with the
    /// event.  A game state's registration of the time of day (`FUN_6f27a110`) holds
    /// its limit's op (0x80101 on), value, and state, its listeners (one for equal by
    /// the time's side of its value: the first while past it), and its handler lists:
    /// itself on 0x80106 and its trigger on 0x80203; an alliance's its player, the
    /// alliance type, its flag listener, and the same lists, its trigger's on 0x8020c; a
    /// player state's its op, player, state, and value, and its trigger's on 0x8020b.
    /// A filter names its function's handler; a location keeps its point.
    pub fn script_object_records(&self) -> Vec<(u32, First, Option<Second>)> {
        let mut out = Vec::new();
        let words = |ws: &[u32]| ws.iter().map(|&w| Field::Word(w)).collect::<Vec<_>>();
        let handler_list = |pairs: Vec<((u32, u32), u32)>| Field::List(pairs.into_iter().map(|(r, e)| vec![Field::Ref(rf(r)), Field::Word(e)]).collect());
        let player = |p: u8| self.tags.player_slots.get(p as usize).map_or((u32::MAX, u32::MAX), |&s| self.tagged(s));
        let slot_of = |h: u32| self.tags.handle_slots.get(&h).map(|&s| self.tagged(s));
        let hours = self.time_of_day;
        for id in self.handles.ids() {
            match self.handles.get(id) {
                Some(Obj::Event(r)) => {
                    let Some(&own) = self.tags.handle_ids.get(&id) else { continue };
                    let Some(Obj::Trigger(t)) = self.handles.get(r.trigger) else { continue };
                    let Some(&trig) = t.slots.first() else { continue };
                    let trig = self.tagged(trig);
                    let base = [0, u32::MAX, u32::MAX, 0];
                    let (code, own_words, tail): (&[u8; 4], Vec<u32>, Vec<Field>) = match &r.kind {
                        EventKind::Unit { unit, event } => {
                            let e = *event as u32 + 0x8_0200;
                            let u = self.unit(*unit).and_then(|u| u.extra.object_slots[0].first().copied()).unwrap_or((u32::MAX, u32::MAX));
                            (b"uevt", vec![0], vec![Field::Word(e), Field::Word(u.0), Field::Word(u.1), Field::Word(0), Field::Ref(NONE), handler_list(vec![(trig, e)])])
                        }
                        EventKind::Player { player: p, event, alliance } if *event == crate::event_ids::EVENT_PLAYER_ALLIANCE_CHANGED => {
                            let pl = player(*p);
                            let listener = self.tags.event_timers.get(&id).map_or(NONE, |&s| rf(self.tagged(s)));
                            let mut ws = words(&base);
                            ws.extend([Field::Word(0), Field::Word(0x8_0106), Field::Real(0.0), Field::Word(0), Field::Word(pl.0), Field::Word(pl.1), Field::Word(*alliance as u32)]);
                            let tail = vec![Field::List(Vec::new()), Field::Ref(listener), handler_list(vec![(own, 0x8_0106)]), handler_list(vec![(trig, 0x8_020c)])];
                            out.push((own.0, agent_first(own, b"alvt", &[]), Some(Second::Agent { own: ws, handlers: game_handlers(self.game_object()), tail })));
                            continue;
                        }
                        EventKind::Player { player: p, event, .. } | EventKind::PlayerUnit { player: p, event, filter: None } => {
                            let e = *event as u32 + 0x8_0200;
                            let pl = player(*p);
                            (b"pevt", vec![0, e, pl.0, pl.1, 0, 0], vec![Field::Ref(NONE), handler_list(vec![(trig, e)])])
                        }
                        EventKind::PlayerUnit { player: p, event, filter: Some(f) } => {
                            let e = *event as u32 + 0x8_0200;
                            let pl = player(*p);
                            let f = slot_of(*f).map_or(NONE, rf);
                            (b"pevt", vec![0, e, pl.0, pl.1, 0, 0], vec![Field::Ref(f), handler_list(vec![(trig, e)])])
                        }
                        EventKind::PlayerState { player: p, state, op, value } => {
                            let pl = player(*p);
                            let listener = self.tags.event_timers.get(&id).map_or(NONE, |&s| rf(self.tagged(s)));
                            let op = *op as u32 + 0x8_0101;
                            let mut ws = words(&base);
                            ws.extend(words(&[0, op, pl.0, pl.1, 0, *state as u32, op]));
                            ws.push(Field::Real(*value));
                            let tail = vec![Field::Ref(NONE), Field::List(Vec::new()), Field::Ref(listener), handler_list(vec![(trig, 0x8_020b)])];
                            out.push((own.0, agent_first(own, b"psvt", &[]), Some(Second::Agent { own: ws, handlers: game_handlers(self.game_object()), tail })));
                            continue;
                        }
                        EventKind::Game(event) => {
                            let e = *event as u32 + 0x8_0200;
                            (b"gevt", vec![e], vec![handler_list(vec![(trig, e)])])
                        }
                        EventKind::TimerExpire(timer) => {
                            let tm = slot_of(*timer).map_or(NONE, rf);
                            (b"tmet", vec![0], vec![Field::Ref(tm), handler_list(vec![(trig, 0x8_0204)])])
                        }
                        EventKind::GameState { state: 2, op, value } => {
                            let listener = self.tags.event_timers.get(&id).map_or(NONE, |&s| rf(self.tagged(s)));
                            let first = *op != 2 || hours > *value;
                            let (l1, l2) = if first { (listener, NONE) } else { (NONE, listener) };
                            let mut ws = words(&base);
                            ws.extend([Field::Word(0x8_0106), Field::Word(*op as u32 + 0x8_0101), Field::Real(*value), Field::Word(2)]);
                            let tail = vec![Field::List(Vec::new()), Field::Ref(l1), Field::Ref(l2), handler_list(vec![(own, 0x8_0106)]), handler_list(vec![(trig, 0x8_0203)])];
                            out.push((own.0, agent_first(own, b"gfvt", &[]), Some(Second::Agent { own: ws, handlers: game_handlers(self.game_object()), tail })));
                            continue;
                        }
                        _ => continue,
                    };
                    let mut ws = words(&base);
                    ws.extend(words(&own_words));
                    out.push((own.0, agent_first(own, code, &[]), Some(Second::Agent { own: ws, handlers: game_handlers(self.game_object()), tail })));
                }
                Some(Obj::BoolExpr(BoolExpr::Func(f))) => {
                    let Some(own) = slot_of(id) else { continue };
                    let handler = self.tags.handler_objects.iter().find(|(_, g)| *g == f).map_or(NONE, |(&s, _)| rf(self.tagged(s)));
                    out.push((own.0, agent_first(own, b"+flt", &[]), Some(Second::Agent { own: vec![Field::Word(0)], handlers: game_handlers(self.game_object()), tail: vec![Field::Ref(handler)] })));
                }
                Some(Obj::Location(x, y)) => {
                    let Some(own) = slot_of(id) else { continue };
                    out.push((own.0, agent_first(own, b"+loc", &[]), Some(Second::Agent { own: vec![Field::Word(0)], handlers: game_handlers(self.game_object()), tail: vec![Field::Real(*x), Field::Real(*y)] })));
                }
                _ => {}
            }
        }
        out
    }

    /// An object's slot with its birth, as references name it.
    fn tagged(&self, slot: u32) -> (u32, u32) {
        (slot, self.tags.live.get(&slot).map_or(0, |e| e.1))
    }

    /// The two pathing maps' objects (`^pma`): the coarse one of 8-cell blocks, then the
    /// pathing map itself.
    fn pathing_map_objects(&self) -> [(u32, u32); 2] {
        let mut maps = self.tags.live.iter().filter(|(_, (c, _))| c == b"^pma").map(|(&s, (_, b))| (s, *b));
        let a = maps.next().unwrap_or_default();
        [a, maps.next().unwrap_or_default()]
    }

    /// A destructable's records besides its own, by slot (read against the quiet test
    /// map's trees): its position (`` `pos ``: its cell, x then y, as reals, 0, 0, the
    /// largest real, its facing in radians, 0, and its two regions), the regions of its
    /// position, a cell each (the coarse map's of 8-cell blocks, kind 4, and the
    /// pathing map's, kind 1), and its footprint's three (kind 1, flags 0xc2, 0x10, and
    /// 0x08 for a tree, bounds its cells' first row and column and their last plus 2,
    /// the count of its cells and 0x10000000).  A region's first word is its map's
    /// search mark when it was last met, the running game's (left 0).
    pub fn destructable_records(&self, h: u32) -> Vec<(u32, First)> {
        let Some(Obj::Destructable(d)) = self.handles.get(h) else { return Vec::new() };
        let Some(p) = self.pathing.as_ref() else { return Vec::new() };
        if d.object_slots.len() < 3 {
            return Vec::new();
        }
        let [coarse, fine] = self.pathing_map_objects();
        let own = self.tagged(d.oid.0);
        let pos = self.tagged(d.object_slots[0]);
        let (cx, cy) = ((d.x - p.origin[0]) / crate::pathing::CELL, (d.y - p.origin[1]) / crate::pathing::CELL);
        let (row, col) = (cy.floor() as u32, cx.floor() as u32);
        let r = |s: u32| Ref(self.tagged(s).0, self.tagged(s).1);
        let mut out = vec![(
            pos.0,
            First::Pos {
                base: property_base(pos, own),
                values: [cx, cy, 0.0, 0.0, f32::MAX, self.saved_destructable_facing(d), 0.0],
                refs: [r(d.object_slots[1]), r(d.object_slots[2])],
            },
        )];
        let region = |map: (u32, u32), owner: (u32, u32), word: u32, bounds: [u32; 4], words: [u32; 3]| First::Region { refs: [Ref(map.0, map.1), Ref(owner.0, owner.1)], word, bounds, words };
        out.push((d.object_slots[1], region(coarse, pos, 0x0400_0000, [row >> 3, col >> 3, (row >> 3) + 1, (col >> 3) + 1], [0, 0x2000_0001, 0])));
        out.push((d.object_slots[2], region(fine, pos, 0x0100_0000, [row, col, row + 1, col + 1], [0, 1, 0])));
        if d.object_slots.len() == 6 && !d.pathing_cells.is_empty() {
            let rows = d.pathing_cells.iter().map(|c| c.1 as u32);
            let cols = d.pathing_cells.iter().map(|c| c.0 as u32);
            let bounds = [rows.clone().min().unwrap_or(0), cols.clone().min().unwrap_or(0), rows.max().unwrap_or(0) + 2, cols.max().unwrap_or(0) + 2];
            let n = d.pathing_cells.len() as u32;
            for (k, flags) in [0xc2, 0x10, 0x08].into_iter().enumerate() {
                out.push((d.object_slots[3 + k], region(fine, own, 0x0100_0000 | flags, bounds, [0, n, 0x1000_0000])));
            }
        }
        out
    }

    /// A destructable's own records (`+w3d`; read against the quiet test map's trees):
    /// the agent's (its class's definition, its position, its position again as its
    /// second link), and its own (CDestructable: a word, 6, its life, 0, and its
    /// maximum; its handlers, the game's for event 0x80267; the widget, its sprite's
    /// model the type's file, variation, and "S" for a lightweight type, its scale,
    /// its owner 0, its type, its footprint's regions and its place, its occlusion
    /// height; then its variation, its z, -2, and 1.0).  What the local player sees of
    /// it (the sprite's tint and a word and short, the widget's shorts and first byte,
    /// and for some the word's 0x100) is the running game's: these the most of Booty
    /// Bay's trees have (provisional: whether the original minds them).
    pub fn destructable_own_records(&self, h: u32) -> Option<(u32, First, Second)> {
        let Some(Obj::Destructable(d)) = self.handles.get(h) else { return None };
        if d.object_slots.len() < 3 {
            return None;
        }
        let o = self.objects.destructables.get(d.type_id)?;
        let own = self.tagged(d.oid.0);
        let pos = self.tagged(d.object_slots[0]);
        let none = Ref(u32::MAX, u32::MAX);
        let first = First::Agent { object: plain_base(own.0, own.1), agent: class_ref(b"+w3d"), refs: vec![Ref(pos.0, pos.1)], links: [Link(none, 0), Link(Ref(pos.0, pos.1), 0), Link(none, 0), Link(none, 0)] };
        let handlers = game_handlers(self.game_object());
        let dead = d.life <= 0.0;
        let model = format!("{}{}{}.mdl", o.str("file"), d.variation, if dead { "D" } else if o.int("lightweight") != 0 { "S" } else { "" });
        let sprite = Sprite { model, real: f32::from_bits(0x038c_0025), short: 0xfffe, byte: 255, word: 0x0020_2030, reals: [d.scale, 1.0], words: [u32::MAX, 0], last_real: 0.0, last_byte: 0, flags: [0; 3], entries: vec![(0.0, 4, Vec::new())] };
        let footprint: Vec<Ref> = d.object_slots[3..].iter().map(|&s| self.tagged(s)).map(|t| Ref(t.0, t.1)).collect();
        let widget = Widget {
            sprite: Some(sprite),
            owner: 0,
            word: u32::from_be_bytes(d.type_id),
            shorts: [4095, 0x8001],
            refs: Some((footprint, [d.x, d.y])),
            bytes: [1, 0],
            last_real: o.real("occH"),
            byte: 0,
        };
        let second = Second::Destructable { own: (6, [d.life, 0.0, d.max_life]), handlers, widget, bytes: [d.variation as u8, 0], words: [d.z.to_bits(), 0xffff_fffe, 1f32.to_bits()] };
        Some((own.0, first, second))
    }

    /// A destructable's facing as its position saves it: its type's fixed rotation if
    /// it has one, else as placed, in radians (read: every tree of Booty Bay and
    /// Forestwalk, fixedRot 270, saves 0x4096cbe3, which 270 times 0.01745329 gives;
    /// provisional: that constant, which the binary does not hold as such, and the
    /// placed facing of a type without one).
    fn saved_destructable_facing(&self, d: &crate::types::Destructable) -> f32 {
        let fixed = self.objects.destructables.get(d.type_id).map_or(-1.0, |o| o.real("fixedRot"));
        if fixed >= 0.0 {
            fixed * 0.017_453_29
        } else {
            d.placed_facing.unwrap_or(d.facing * 0.017_453_292)
        }
    }

    /// The object manager's header as saved (`FUN_6f15c9b0`): two words, two reals (25
    /// and 0.2), its update counter and that's parity, each table's births, four
    /// groups of a clock (a time, a word, its wrap 300, a word, a count), and "espi".
    /// The two words, the counter, and the counts are the running game's (they differ
    /// between saves of one moment); the original loads a save with them 0, the
    /// counter 1024 (read: the quiet test map's save so changed, `w3zedit header0`).
    pub fn saved_manager_header(&self, second_births: u32) -> Vec<u8> {
        let mut o = Vec::new();
        let mut word = |v: u32| o.extend(v.to_le_bytes());
        let main = f32::from_bits(self.main_clock.1);
        word(0);
        word(0);
        word(25f32.to_bits());
        word(0.2f32.to_bits());
        word(1024);
        word(0);
        word(self.tags.born);
        word(second_births);
        let agent = self.step_agent.last().copied().unwrap_or(main.to_bits());
        for (t, w) in [(main.to_bits(), 4096), (agent, 0), (main.to_bits(), 4096), (agent, 0)] {
            word(t);
            word(0);
            word(300f32.to_bits());
            word(w);
            word(0);
        }
        o.extend(b"espi");
        o
    }

    /// A save made from `template` (the original's save of the same moment) with the
    /// pieces the port makes in place of its own.
    pub fn write_save(&self, template: &SaveGame) -> SaveGame {
        let mut s = template.clone();
        let Some(m) = s.objects.as_mut() else { return s };
        let Ok(mut objects) = m.objects() else { return s };
        let births1 = m.header.get(28..32).map_or(0, |b| u32::from_le_bytes(b.try_into().unwrap()));
        m.header = self.saved_manager_header(births1);
        m.classes = saved_classes();
        let defs = class_definitions();
        for o in objects[1].iter_mut() {
            if let Some(d) = defs.iter().find(|d| d.entry.tag == o.entry.tag) {
                *o = d.clone();
            }
        }
        let ours = self.saved_table();
        if ours.len() == objects[0].len() {
            for (o, e) in objects[0].iter_mut().zip(ours) {
                o.entry = e;
            }
        }
        let mut records: std::collections::HashMap<u32, First> = std::collections::HashMap::new();
        for &h in &self.destructable_order {
            records.extend(self.destructable_records(h));
        }
        let mut seconds: std::collections::HashMap<u32, Second> = std::collections::HashMap::new();
        for &h in &self.destructable_order {
            if let Some((slot, f, sec)) = self.destructable_own_records(h) {
                records.insert(slot, f);
                seconds.insert(slot, sec);
            }
        }
        for (slot, f, sec) in self.trigger_records().into_iter().chain(self.script_object_records()).chain(self.player_records(&client_listeners(template))) {
            records.insert(slot, f);
            if let Some(sec) = sec {
                seconds.insert(slot, sec);
            }
        }
        for o in objects[0].iter_mut() {
            if let Some(r) = records.get(&o.entry.tag) {
                o.first = r.to_bytes();
            }
            if let Some(Ok(b)) = seconds.get(&o.entry.tag).map(|sec| sec.to_bytes(o.entry.kind)) {
                o.second = b;
            }
        }
        m.set_objects(&objects);
        m.free[0] = self.saved_free_list();
        s
    }

    /// The first object table as saved (`FUN_6f15c810`): each live object by slot, its
    /// family's code, its own, its slot, and its birth.
    pub fn saved_table(&self) -> Vec<Tagged> {
        self.tags
            .live
            .iter()
            .map(|(&slot, (code, birth))| Tagged { family: u32::from_be_bytes(family_of(code)), kind: u32::from_be_bytes(*code), tag: slot, birth: *birth })
            .collect()
    }

    /// The first table's free list as saved (`FUN_6f15c710`): its head, each slot's
    /// next in turn, and -1.
    pub fn saved_free_list(&self) -> Vec<u8> {
        let mut out = Vec::new();
        for &s in self.tags.free.iter().rev() {
            out.extend(s.to_le_bytes());
        }
        out.extend(u32::MAX.to_le_bytes());
        out
    }

    /// The pieces of a save the port makes, each against the original's save `orig`:
    /// a line each, the same or where they first part.
    pub fn compare_save(&self, orig: &SaveGame) -> Vec<String> {
        let mut out = Vec::new();
        let Some(m) = orig.objects.as_ref() else { return vec!["no object manager".into()] };
        let ours = self.saved_table();
        let line = match ours.iter().zip(&m.tables[0]).position(|(a, b)| a != b) {
            None if ours.len() == m.tables[0].len() => format!("table: the same, {} objects", ours.len()),
            None => format!("table: the same as far as both go, ours {} the original's {}", ours.len(), m.tables[0].len()),
            Some(k) => format!("table: parts at {k}: ours {:?}, the original's {:?}", ours[k], m.tables[0][k]),
        };
        out.push(line);
        let free = self.saved_free_list();
        out.push(if saved_classes() == m.classes { format!("classes: the same, {}", m.classes.len()) } else { "classes: differ".into() });
        if let Ok(objects) = m.objects() {
            let defs = class_definitions();
            let theirs: Vec<&SavedObject> = objects[1].iter().filter(|o| o.entry.kind == u32::from_be_bytes(*b"^adf")).collect();
            let k = defs.iter().zip(&theirs).position(|(a, b)| a.entry != b.entry || a.first != b.first || a.second != b.second);
            out.push(match k {
                None if defs.len() == theirs.len() => format!("class definitions: the same, {}", defs.len()),
                None => format!("class definitions: ours {}, the original's {}", defs.len(), theirs.len()),
                Some(k) => format!("class definitions: part at {k}, tag {:x}", defs[k].entry.tag),
            });
        }
        if let Ok(objects) = m.objects() {
            // The destructables' positions and regions, the regions' marks set aside.
            let by_slot: std::collections::HashMap<u32, &SavedObject> = objects[0].iter().map(|o| (o.entry.tag, o)).collect();
            let (mut same, mut differ, mut first) = (0, 0, None);
            for &h in &self.destructable_order {
                for (slot, rec) in self.destructable_records(h) {
                    let ours = match &rec {
                        First::Region { refs, word, bounds, words } => First::Region { refs: *refs, word: *word, bounds: *bounds, words: [0, words[1], words[2]] },
                        other => other.clone(),
                    };
                    let theirs = by_slot.get(&slot).and_then(|o| o.first().ok()).map(|f| match f {
                        First::Region { refs, word, bounds, words } => First::Region { refs, word, bounds, words: [0, words[1], words[2]] },
                        other => other,
                    });
                    if theirs.as_ref() == Some(&ours) {
                        same += 1;
                    } else {
                        differ += 1;
                        if first.is_none() {
                            first = Some(format!("slot {slot}: ours {ours:?}, the original's {theirs:?}"));
                        }
                    }
                }
            }
            out.push(format!("destructables' positions and regions: {same} the same, {differ} differ{}", first.map_or(String::new(), |f| format!("; first {f}"))));
            // Their own records, with what the local player sees set as ours.
            let (mut same, mut seen, mut differ, mut first) = (0, 0, 0, None);
            for &h in &self.destructable_order {
                let Some((slot, f, sec)) = self.destructable_own_records(h) else { continue };
                let Some(o) = by_slot.get(&slot) else { continue };
                let theirs = (o.first().ok(), o.second().ok());
                if theirs.0.as_ref() != Some(&f) {
                    differ += 1;
                    first.get_or_insert(format!("slot {slot} first: ours {f:?}, the original's {:?}", theirs.0));
                    continue;
                }
                match theirs.1 {
                    Some(t) if t == sec => same += 1,
                    Some(Second::Destructable { own, handlers, mut widget, bytes, words }) => {
                        if let (Some(ws), Second::Destructable { widget: ours, .. }) = (widget.sprite.as_mut(), &sec) {
                            let os = ours.sprite.as_ref().unwrap();
                            (ws.real, ws.short, ws.word) = (os.real, os.short, os.word);
                            (widget.shorts, widget.bytes[0]) = (ours.shorts, ours.bytes[0]);
                        }
                        if (Second::Destructable { own: (own.0 & !0x100, own.1), handlers, widget, bytes, words }) == sec {
                            seen += 1;
                        } else {
                            differ += 1;
                            first.get_or_insert(format!("slot {slot} second differs"));
                        }
                    }
                    other => {
                        differ += 1;
                        first.get_or_insert(format!("slot {slot}: the original's {other:?}"));
                    }
                }
            }
            out.push(format!("destructables: {same} the same, {seen} but for what the local player sees, {differ} differ{}", first.map_or(String::new(), |f| format!("; first {f}"))));
            // The triggers' objects.
            let (mut same, mut differ, mut first) = (0, 0, None);
            for (slot, f, sec) in self.trigger_records() {
                let Some(o) = by_slot.get(&slot) else { continue };
                let ok1 = o.first().ok().as_ref() == Some(&f);
                let ok2 = sec.as_ref().is_none_or(|s| o.second().ok().as_ref() == Some(s));
                if ok1 && ok2 {
                    same += 1;
                } else {
                    differ += 1;
                    let brief = |s: Option<Second>| match s {
                        Some(Second::Agent { own, handlers, tail }) => format!("own {own:?} keyed {:?} fixed {:?} events {} tail {tail:?}", handlers.keyed, handlers.fixed, handlers.events.len()),
                        other => format!("{other:?}"),
                    };
                    let detail = if ok1 { format!("ours {}; the original's {}", brief(sec.clone()), brief(o.second().ok())) } else { format!("ours {f:?}; the original's {:?}", o.first()) };
                    let line = format!("slot {slot} ({}): first {ok1} second {ok2}; {detail}", String::from_utf8_lossy(&o.entry.kind.to_be_bytes()));
                    if std::env::var("W3Z_ALL").is_ok() {
                        out.push(line.clone());
                    }
                    first.get_or_insert(line);
                }
            }
            if std::env::var("W3Z_HANDLERS").is_ok() {
                for (slot, func) in &self.tags.handler_objects {
                    let k = self.tags.function_numbers.get(func).copied().unwrap_or(0);
                    let theirs = by_slot.get(slot).and_then(|o| o.second().ok()).and_then(|s| match s {
                        Second::Agent { tail, .. } => tail.first().cloned(),
                        _ => None,
                    });
                    out.push(format!("handler slot {slot} birth {} func {func} ours {k} theirs {theirs:?}", self.tagged(*slot).1));
                }
            }
            out.push(format!("triggers' objects: {same} the same, {differ} differ{}", first.map_or(String::new(), |f| format!("; first {f}"))));
            // The script's other objects.
            let (mut same, mut differ, mut first) = (0, 0, None);
            for (slot, f, sec) in self.script_object_records() {
                let Some(o) = by_slot.get(&slot) else { continue };
                let ok1 = o.first().ok().as_ref() == Some(&f);
                let ok2 = sec.as_ref().is_none_or(|s| o.second().ok().as_ref() == Some(s));
                if ok1 && ok2 {
                    same += 1;
                } else {
                    differ += 1;
                    let brief = |s: Option<Second>| match s {
                        Some(Second::Agent { own, handlers, tail }) => format!("own {own:?} keyed {:?} fixed {:?} events {} tail {tail:?}", handlers.keyed, handlers.fixed, handlers.events.len()),
                        other => format!("{other:?}"),
                    };
                    let detail = if ok1 { format!("ours {}; the original's {}", brief(sec.clone()), brief(o.second().ok())) } else { format!("ours {f:?}; the original's {:?}", o.first()) };
                    let line = format!("slot {slot} ({}): first {ok1} second {ok2}; {detail}", String::from_utf8_lossy(&o.entry.kind.to_be_bytes()));
                    if std::env::var("W3Z_ALL").is_ok() {
                        out.push(line.clone());
                    }
                    first.get_or_insert(line);
                }
            }
            if false {
                for (slot, func) in &self.tags.handler_objects {
                    let k = self.tags.function_numbers.get(func).copied().unwrap_or(0);
                    let theirs = by_slot.get(slot).and_then(|o| o.second().ok()).and_then(|s| match s {
                        Second::Agent { tail, .. } => tail.first().cloned(),
                        _ => None,
                    });
                    out.push(format!("handler slot {slot} birth {} func {func} ours {k} theirs {theirs:?}", self.tagged(*slot).1));
                }
            }
            out.push(format!("script's objects: {same} the same, {differ} differ{}", first.map_or(String::new(), |f| format!("; first {f}"))));
            // The players' and their properties'.
            let (mut same, mut differ, mut first) = (0, 0, None);
            for (slot, f, sec) in self.player_records(&client_listeners(orig)) {
                let Some(o) = by_slot.get(&slot) else { continue };
                let ok1 = o.first().ok().as_ref() == Some(&f);
                let ok2 = sec.as_ref().is_none_or(|s| o.second().ok().as_ref() == Some(s));
                if ok1 && ok2 {
                    same += 1;
                } else {
                    differ += 1;
                    let brief = |s: Option<Second>| match s {
                        Some(Second::Agent { own, handlers, tail }) => format!("own {own:?} keyed {:?} fixed {:?} events {} tail {tail:?}", handlers.keyed, handlers.fixed, handlers.events.len()),
                        other => format!("{other:?}"),
                    };
                    let detail = if ok1 { format!("ours {}; the original's {}", brief(sec.clone()), brief(o.second().ok())) } else { format!("ours {f:?}; the original's {:?}", o.first()) };
                    let line = format!("slot {slot} ({}): first {ok1} second {ok2}; {detail}", String::from_utf8_lossy(&o.entry.kind.to_be_bytes()));
                    if std::env::var("W3Z_ALL").is_ok() {
                        out.push(line.clone());
                    }
                    first.get_or_insert(line);
                }
            }
            if false {
                for (slot, func) in &self.tags.handler_objects {
                    let k = self.tags.function_numbers.get(func).copied().unwrap_or(0);
                    let theirs = by_slot.get(slot).and_then(|o| o.second().ok()).and_then(|s| match s {
                        Second::Agent { tail, .. } => tail.first().cloned(),
                        _ => None,
                    });
                    out.push(format!("handler slot {slot} birth {} func {func} ours {k} theirs {theirs:?}", self.tagged(*slot).1));
                }
            }
            out.push(format!("players' objects: {same} the same, {differ} differ{}", first.map_or(String::new(), |f| format!("; first {f}"))));
        }
        out.push(match free.chunks(4).zip(m.free[0].chunks(4)).position(|(a, b)| a != b) {
            None if free.len() == m.free[0].len() => format!("free list: the same, {} slots", free.len() / 4 - 1),
            None => format!("free list: the same as far as both go, ours {} the original's {}", free.len() / 4, m.free[0].len() / 4),
            Some(k) => format!("free list: parts at {k}"),
        });
        out
    }
}
