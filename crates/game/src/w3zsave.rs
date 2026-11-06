//! Saved games as the original writes them (.w3z, `w3_formats::w3z`), from the port's
//! state: built piece by piece, each checked against the original's save of the same
//! moment (`World::compare_save`).

use w3_formats::w3z::{Array, Field, First, Handlers, Link, ObjectBase, PropertyBase, Ref, SaveGame, Second, SavedObject, Sprite, Tagged, Timing, Widget};

use crate::types::{BoolExpr, EventKind, Obj};

use crate::tags::Code;
use crate::world::World;
use crate::codes;

mod unit;
mod kinds;
mod kind_layouts;
mod script;

pub(crate) use script::CAMERA_SETUP_FIELDS;

/// The family a kind of engine object belongs to, as the object tables hold it (read
/// from every save on hand: properties, listeners, timers, and pathing regions under
/// their families, a few kinds their own, the rest agents).
pub fn family_of(kind: &Code) -> Code {
    match kind {
        codes::class::FLAG | codes::class::UNIT_POSITION | codes::class::POSITION | codes::class::REAL => *codes::class::PROPERTY_FAMILY,
        codes::class::FLAG_LISTENER | codes::class::IN_RANGE_LISTENER | codes::class::THRESHOLD_LISTENER => *codes::class::LISTENER_FAMILY,
        codes::class::CONTINUOUS_MODIFIER => *codes::class::MODIFIER_FAMILY,
        codes::class::PATHING_MAP | codes::class::PATHING_REGION => *codes::class::PATHING_MAP,
        codes::class::ACCELERATOR_GRID | codes::class::CLUSTER | codes::class::LONG_RANGE_ACCELERATOR | codes::class::PATH | codes::class::LONG_RANGE_SYSTEM | codes::class::AGENT_DEFINITION => *kind,
        _ => *codes::class::AGENT,
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
    let adf = u32::from_be_bytes(*codes::class::AGENT_DEFINITION);
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

/// A unit's state words as its order sets them (read against Forestwalk's at 2.5 and
/// 12 seconds): its kind of work (6 an acolyte at its haunted mine, 5 a peasant's
/// harvest, 2 a summoning), 1 in a gold mine, a byte while it walks, the three words
/// after its tint (2 carrying or at the haunted mine, else 1; 5 walking, else 4; 35
/// harvesting, 37 summoning), and 0x80 on its last word inside a mine.
#[derive(Clone, Copy)]
pub(crate) struct OrderState {
    pub kind: u32,
    pub inside_word: u32,
    pub walking: bool,
    pub after_tint: [u32; 4],
    pub inside: bool,
}

impl Default for OrderState {
    fn default() -> Self {
        OrderState { kind: 0, inside_word: 0, walking: false, after_tint: [1, 4, 0, 0], inside: false }
    }
}

/// A unit's order as `World::saved_order` writes it.
pub(crate) struct SavedOrder {
    pub records: Vec<(u32, First, Option<Second>)>,
    pub task: (u32, u32),
    pub order: (u32, u32),
    pub next: (u32, u32),
    pub mine: bool,
    /// Its timer (the summoning's end).
    pub timer: Option<Field>,
    /// Whether it summons a structure.
    pub summoning: bool,
    /// Whether it is inside a gold mine.
    pub in_gold_mine: bool,
    /// Whether it walks.
    pub walking: bool,
    pub state: OrderState,
    /// Its walk's cluster and path, and the unit it walks to.
    pub walk: Option<((u32, u32), (u32, u32), u32)>,
    /// Its place in a gold mine's queue.
    pub queue: Option<(u32, u32)>,
}

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
        .filter(|o| &o.entry.kind.to_be_bytes() == codes::class::FLAG_LISTENER)
        .filter_map(|o| match o.first() {
            Ok(First::FlagListener { object, word }) => Some(Listener { me: (o.entry.tag, o.entry.birth), owner: (object.links[0].3 .0, object.links[0].3 .1), flag: (object.links[1].3 .0, object.links[1].3 .1), word }),
            _ => None,
        })
        .collect();
    out.sort_by_key(|l| l.me.1);
    out
}

/// The span an extended agent timer runs in (DAT_6fd3c894, set to 120 at load by
/// `FUN_6f070d80`).
pub(crate) const TIMER_CHUNK: f32 = 120.0;

/// An extended agent timer's (CAgentTimerExtended's) timeout as it runs: the count of
/// whole chunks in it (as a periodic node of `TIMER_CHUNK`), and the time beyond them,
/// the fraction of a chunk in the engine's arithmetic (read: a shop's 440 seconds,
/// three chunks and 80.00001; 120, one and 0).  Under a chunk, none and the timeout.
impl World {
    /// A running timer's node as the original would have it now: its due (on its clock,
    /// in the clock's current wrap), its period, and the chunks left of its run (none
    /// once at the rest).  From its start on its clock, each node's due its last's and
    /// its period, added in single precision: each chunk's 120, then the rest's (as the
    /// last chunk passes, vf3 `FUN_6f060d40`), then, a periodic one, another run; each
    /// set back 300 as the clock wraps while it waits (read: the hero map's 30-second
    /// timer due 300.0001 with the clock at 300.0 in its third wrap).  Provisional: the
    /// rest's node from the last chunk's due, not the clock's step.
    fn timer_node(&self, t: &crate::types::Timer, chunks: u32, rest: f32, run: f32) -> Option<(f32, f32, u32)> {
        use crate::efloat::{add, sub};
        let wrap = f32::from_bits(crate::motion::WRAP);
        let now = f32::from_bits(self.tags.clock) as f64 + wrap as f64 * self.tags.clock_wraps as f64;
        let (mut d, mut w, mut j) = (t.started_clock, t.started_wraps, 0u32);
        for _ in 0..1_000_000 {
            let period = if chunks == 0 { run } else if j < chunks { TIMER_CHUNK } else { rest };
            if period <= 0.0 {
                return Some((f32::from_bits(self.tags.clock), period, 0));
            }
            let mut due = add(d, period.to_bits());
            let mut dw = w;
            while dw < self.tags.clock_wraps && f32::from_bits(due) > wrap {
                due = sub(due, crate::motion::WRAP);
                dw += 1;
            }
            if f32::from_bits(due) as f64 + wrap as f64 * dw as f64 > now {
                return Some((f32::from_bits(due), period, chunks - j.min(chunks)));
            }
            (d, w) = (due, dw);
            if chunks > 0 && j < chunks {
                j += 1;
            } else if t.periodic {
                j = 0;
            } else {
                return None;
            }
        }
        None
    }
}

pub(crate) fn timer_chunks(timeout: f32) -> (u32, f32) {
    use crate::efloat::{div, floor, from_int, mul, sub, to_int};
    let chunk = TIMER_CHUNK.to_bits();
    let q = div(timeout.to_bits(), chunk);
    let chunks = to_int(floor(q)).max(0) as u32;
    if chunks == 0 {
        return (0, timeout);
    }
    (chunks, f32::from_bits(mul(sub(q, from_int(chunks as i32)), chunk)))
}

/// The art (the second table's effects) the auras and their buffs hold first in their
/// records, by slot, as a save holds them.
pub fn client_art(save: &SaveGame) -> std::collections::HashMap<u32, Field> {
    let Some(m) = save.objects.as_ref() else { return Default::default() };
    let Ok(objects) = m.objects() else { return Default::default() };
    objects[0]
        .iter()
        .filter(|o| {
            let k = o.entry.kind.to_be_bytes();
            k[0] == b'B' || matches!(&k, codes::ability::id::PRIESTESS_TRUESHOT_AURA | codes::ability::id::PALADIN_DEVOTION_AURA | codes::ability::id::MANA_REGENERATION_AURA | codes::ability::id::ARCH_MAGE_BRILLIANCE_AURA | codes::ability::id::DEATH_KNIGHT_UNHOLY_AURA | codes::ability::id::CHIEFTAIN_ENDURANCE_AURA | codes::ability::id::DREADLORD_VAMPIRIC_AURA | codes::ability::id::KEEPER_THORNS_AURA | codes::ability::id::AURA_WAR_DRUMS | codes::ability::id::COMMAND_AURA | codes::ability::id::AURA_REGENERATION_WARD)
        })
        .filter_map(|o| match o.second() {
            Ok(Second::Agent { tail, .. }) => match tail.first() {
                Some(f @ Field::List(rows)) if !rows.is_empty() && rows.iter().all(|r| matches!(r[..], [Field::Ref(x)] if x.0 & 0x8000_0000 != 0)) => Some((o.entry.tag, f.clone())),
                _ => None,
            },
            _ => None,
        })
        .collect()
}

/// The rally points (`rlyp`, the second table's agents), each with the structure it
/// serves (its first link's owner), as a save holds them.
pub fn rally_points(save: &SaveGame) -> Vec<(Ref, Ref)> {
    let Some(m) = save.objects.as_ref() else { return Vec::new() };
    let Ok(objects) = m.objects() else { return Vec::new() };
    objects[1]
        .iter()
        .filter(|o| &o.entry.kind.to_be_bytes() == codes::class::RALLY_POINT)
        .filter_map(|o| match o.first() {
            Ok(First::Agent { object, .. }) => Some((Ref(o.entry.tag, o.entry.birth), object.links[0].3)),
            _ => None,
        })
        .collect()
}

/// A chain of listeners, newest first from its head: each one's link to the one made
/// after it and to the one before, in its link set `set`.
/// A unit's continuous modifier as a save holds it (`` `rcm ``): its slot and birth, the
/// real it moves (1 life, 2 mana, 5 sight, as `unit_real_records` numbers them), its
/// kind's word, its values (a rate and a limit), and its timing node.
struct UnitModifier {
    r: (u32, u32),
    prop: usize,
    word: u32,
    values: [f32; 2],
    timing: Timing,
}

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
                let mut base = self.stamped(listed_property_base(flags, k, owner));
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
                    // (Gold, lumber, and the gold and lumber gathered are kept in tenths:
                    // GetPlayerState, `6f201ad0`, divides these four by ten.)
                    1 | 2 | crate::types::player_state::GOLD_GATHERED | crate::types::player_state::LUMBER_GATHERED => p.get_state(k) * 10,
                    crate::types::player_state::GOLD_UPKEEP_RATE => gold_tax,
                    crate::types::player_state::LUMBER_UPKEEP_RATE => lumber_tax,
                    _ => p.get_state(k),
                } as u32)
                .collect();
            flag_records(&mut out, me, &flags, &values);
            let mut first = agent_first(me, codes::class::PLAYER, &flags);
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
            let mut first = agent_first(ally, codes::class::ALLIANCE, &aflags);
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

    /// The players' AIs' own records (CCompAI, `FUN_6f9ca100`; read against the quiet
    /// test map's, whose computer runs no script): 0, its player, the newest list of
    /// its units' AIs (`uref`) twice, none-valued places, eight none and eight 0.0, 7,
    /// a computer's newest unit not a structure, four towns (a
    /// computer's first 1), 3, twelve and twelve 0, six none, a flags word (0x180183 a
    /// computer's, 0x100002 Neutral Hostile's), and its three timers, every 2, 10, and
    /// 0.5 seconds from its making (events 0xd01c1, 0xd01c3, 0xd01c5).  Provisional:
    /// what a running script sets in them, the flags.
    /// A computer player's AI object (`AI-1`), in the order computers' are made.
    fn player_ai_object(&self, player: u8) -> Option<(u32, u32)> {
        let computers: Vec<u8> = (0..12u8).filter(|&i| self.players.get(i as usize).is_some_and(|p| p.controller == 1)).collect();
        let k = computers.iter().position(|&p| p == player)?;
        self.tags.player_ais.get(k).copied()
    }

    /// A computer's captains' objects as saved (CAICaptain and its parts, read against
    /// Forestwalk's undead computer's at 2.5 and 160 seconds): for each, attack then
    /// defense, its agent (its position its property, its newest listener its head) and
    /// own record (6; its four listeners' handlers keyed by them, the defense
    /// captain's two fixed; its VillagerKid sprite, 200, its listeners, its look every
    /// second, its kind 2 or 1, its home thrice, and 200); its position cluster (its
    /// place in cells, never moving, 0.8 and 0.75, 0.4, 0.1); its regions (one 8-cell
    /// block 0x08000000, one cell 0x01000002); its path, idle; and its four in-range
    /// listeners (400, 1500, 1100, and 1300 units, the first two every half second,
    /// events 0xd01aa, 0xd01ab, 0xd01c9, 0xd01ca), the units within.  Provisional: its
    /// units, its group, its counts, and its timers' counts (none written).
    pub fn captain_records(&self) -> Vec<(u32, First, Option<Second>)> {
        let mut out = Vec::new();
        let Some(p) = self.pathing.as_ref() else { return out };
        let [coarse, fine] = self.pathing_map_objects();
        let now = self.time;
        let w = Field::Word;
        let mut ais: Vec<(&u8, &crate::ai::AiState)> = self.ai.iter().collect();
        ais.sort_by_key(|(p, _)| **p);
        for (&player, st) in ais {
            for (k, c) in [&st.attack, &st.defense].into_iter().enumerate() {
                if c.objects.len() != 9 || !c.objects.iter().all(|s| self.tags.live.contains_key(s)) {
                    continue;
                }
                let t: Vec<(u32, u32)> = c.objects.iter().map(|&s| self.tagged(s)).collect();
                let (aic, pcu, pmrc, pmrf, pa) = (t[0], t[1], t[2], t[3], t[4]);
                let pils = &t[5..9];
                let (x, y) = c.place.unwrap_or(c.home);
                let (cx, cy) = ((x - p.origin[0]) / 32.0, (y - p.origin[1]) / 32.0);
                // A timer's next due from the captain's making, by its period.
                let due = |period: f32| {
                    let mut d = crate::efloat::add(c.made_at, period.to_bits());
                    while f32::from_bits(d) < now - 0.0205 {
                        d = crate::efloat::add(d, period.to_bits());
                    }
                    f32::from_bits(d)
                };
                // Its agent.
                let mut first = agent_first(aic, codes::class::AI_CAPTAIN, &[pcu]);
                if let First::Agent { links, .. } = &mut first {
                    links[3] = Link(rf(pils[3]), 0);
                }
                let events = [0xd_01aa, 0xd_01ab, 0xd_01c9, 0xd_01ca];
                let keyed: Vec<(u32, Vec<(Ref, u32)>)> = pils.iter().zip(events).map(|(p, e)| (p.0, vec![(rf(aic), e)])).collect();
                let mut fixed: [Vec<(Ref, u32)>; 5] = Default::default();
                if k == 1 {
                    fixed[0].push((rf(aic), 0xd_0196));
                    fixed[1].push((rf(aic), 0xd_0198));
                }
                let sprite = Sprite { model: "units\\critters\\VillagerKid\\VillagerKid.mdl".into(), real: f32::from_bits(0x0344_0000), short: 0xfffe, byte: 255, word: if k == 0 { u32::MAX } else { 0xff00_ff00 }, reals: [0.0001, 1.0], words: [u32::MAX, 0xffff_f2fb], last_real: f32::from_bits(0x96), last_byte: 0, flags: [1, 0, 0], entries: vec![(0.0, 4, vec![1, 2, 3])] };
                let e = || Field::List(vec![vec![w(u32::MAX), w(u32::MAX), Field::Real(0.0)]]);
                let (hx, hy) = c.home;
                let mut tail = vec![Field::Sprite(Some(sprite)), w(0), w(0), Field::Short(0), Field::Short(0), Field::Byte(0), Field::Byte(0), Field::Byte(0), Field::Real(200.0)];
                tail.extend(pils.iter().map(|&p| Field::Ref(rf(p))));
                tail.push(Field::Timer(rf(aic), 2, Timing(Some((due(1.0), 1.0, 0, 0xd_01c1, 1)))));
                tail.push(Field::Timer(NONE, 0, Timing(None)));
                tail.push(Field::List(Vec::new()));
                tail.extend([w(0), w(0), w(0), w(0), w(if k == 0 { 2 } else { 1 }), w(1), w(0xd_0014), w(4096), e()]);
                tail.extend([w(k as u32), w(0), w(0), w(0), w(0), w(0)]);
                tail.extend([Field::Real(hx), Field::Real(hy), Field::Real(hx), Field::Real(hy), Field::Real(-1.0), Field::Real(-1.0), Field::Real(200.0), w(u32::MAX), w(0), Field::Real(hx), Field::Real(hy)]);
                tail.extend((0..19).map(|_| e()));
                tail.extend((0..12).map(|_| w(0)));
                out.push((aic.0, first, Some(Second::Agent { own: vec![w(6)], handlers: Handlers { keyed, fixed, events: vec![Vec::new(); 0x400] }, tail })));
                // Its position.
                let mut base = property_base(pcu, aic);
                base.links[1] = Link(rf(pils[3]), 1);
                out.push((pcu.0, First::Cluster { base, values: [cx, cy, 0.0, 0.0, f32::MAX, 0.0, 0.0], refs: [rf(pmrc), rf(pmrf)], words: [u32::MAX, u32::MAX, 0], reals: [0.8, 0.75, f32::from_bits(0x3ecc_ccce), 0.1, 0.0, 0.0, 0.0, 0.0], more: [cx as u32, cy as u32, 0], flags: [0, 0], last: rf(pa), separate: None }, None));
                // Its regions.
                let (bx, by) = ((cx as u32) >> 3, (cy as u32) >> 3);
                out.push((pmrc.0, First::Region { refs: [rf(coarse), rf(pcu)], word: 0x0800_0000, bounds: [by, bx, by + 1, bx + 1], words: [0, 0xc000_0001, 0] }, None));
                out.push((pmrf.0, First::Region { refs: [rf(fine), rf(pcu)], word: 0x0100_0002, bounds: [cy as u32, cx as u32, cy as u32 + 1, cx as u32 + 1], words: [0, 1, 0] }, None));
                // Its path, idle (a computer's mobile unit's word).
                let way = |limit: u16| (Array { words: [0, 0, 128, 0], entries: Vec::new() }, u32::MAX, 0, limit);
                out.push((pa.0, First::Path { reals: [-128000.0; 6], words: [(0x30 | (player as u32 & 0xf)) << 16, 0, 0, u32::MAX, u32::MAX, 0], real: 0.0, word: 0x0200_0002, ways: [way(700), way(400)], refs: [NONE, NONE, rf(pmrf), NONE] }, None));
                // Its listeners.
                let ranges = [400.0f32, 1500.0, 1100.0, 1300.0];
                let periods = [0.5f32, 0.5, 1.0, 1.0];
                for i in 0..4 {
                    let newer = pils.get(i + 1).map_or(NONE, |&p| rf(p));
                    let older = if i > 0 { rf(pils[i - 1]) } else { NONE };
                    let near: Vec<u32> = self.unit_order.iter().copied().filter(|&v| self.unit(v).is_some_and(|w| !w.dead && !w.hidden && (w.x - x).hypot(w.y - y) - self.objects.units.get(w.type_id).map_or(0.0, |o| o.real("collision")) <= ranges[i])).collect();
                    let near = crate::natives::units::spatial_order(self, near);
                    let entries: Vec<[u32; 3]> = near.iter().filter_map(|&v| self.unit(v).and_then(|w| w.extra.object_slots[0].get(1).copied())).map(|p| [p.0, p.1, 0]).collect();
                    let object = ObjectBase {
                        real: periods[i],
                        words: [events[i], 0x2019_0400],
                        links: [(Link(newer, 0), Link(older, 0), rf(pils[i]), rf(aic)), (Link(newer, u32::from(newer != NONE)), Link(older, u32::from(older != NONE)), rf(pils[i]), rf(pcu))],
                        timings: [Timing(Some((due(periods[i]), periods[i], 0, 0, 1))), Timing(None)],
                    };
                    out.push((pils[i].0, First::Interference { object, real: ranges[i] / 32.0, word: 0, array: in_range_array(entries) }, None));
                }
            }
        }
        out
    }

    /// A town site's mine as the original names it (its mine made with it, else one
    /// made at its place since: a melee undead start's haunting replaces the mine, read,
    /// Echo Isles'; a haunted or entangled mine over it standing for it), the mine's
    /// place, and a computer's town on it: its town hall the nearest to the mine, within
    /// 1500.
    pub(crate) fn site_town(&self, index: usize) -> (Option<u32>, (f32, f32), Option<u32>) {
        let Some(&(x, y, (slot, _))) = self.tags.sites.get(index) else { return (None, (0.0, 0.0), None) };
        let is_mine = |h: u32| [codes::ability::GOLD_MINE, codes::ability::BLIGHTED_GOLD_MINE, codes::ability::ENTANGLED_GOLD_MINE].iter().any(|c| self.ability_with_code(h, c).is_some());
        let site_mine = self.unit_order.iter().copied().find(|&h| self.unit(h).is_some_and(|u| u.extra.site_ai.is_some_and(|s| s.0 == slot))).or_else(|| {
            self.unit_order.iter().copied().find(|&h| self.unit(h).is_some_and(|u| !u.dead && u.extra.mine.is_none() && (u.x - x).hypot(u.y - y) < 64.0) && is_mine(h))
        });
        let cover = site_mine.and_then(|m| self.unit_order.iter().copied().find(|&v| self.unit(v).is_some_and(|w| !w.dead && w.extra.mine == Some(m))));
        let at = site_mine.and_then(|h| self.unit(h)).map_or((x, y), |u| (u.x, u.y));
        let town = self
            .unit_order
            .iter()
            .copied()
            .filter_map(|h| self.unit(h).map(|u| (h, u)))
            .filter(|(_, u)| !u.dead && u.constructing.is_none() && self.ai.contains_key(&u.owner) && self.is_town_hall(u))
            .map(|(h, u)| ((u.x - at.0).hypot(u.y - at.1), h))
            .filter(|(d, _)| *d <= 1500.0)
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, h)| h);
        (cover.or(site_mine), at, town)
    }

    pub fn player_ai_seconds(&self) -> Vec<(u32, Second)> {
        let mut out = Vec::new();
        let computers: Vec<u8> = (0..12u8).filter(|&i| self.players.get(i as usize).is_some_and(|p| p.controller == 1)).collect();
        let clock = self.agent_step;
        for (k, &(slot, birth)) in self.tags.player_ais.iter().enumerate() {
            let me = (slot, birth);
            let player = computers.get(k).copied().unwrap_or(12);
            let computer = player < 12;
            let head = self.unit_order.iter().filter_map(|&h| self.unit(h)).filter(|u| u.owner == player).filter_map(|u| u.extra.ai_slots.first().copied()).filter(|s| self.tags.live.get(&s.0).is_some_and(|e| e.1 == s.1)).max_by_key(|s| s.1).unwrap_or((u32::MAX, u32::MAX));
            // (The unit its repair logic chose last (CCompAI +0x17c, `FUN_6f9c3920` and
            // `FUN_6f9cbba0`): one with Repair or Restore, which that logic, run as a
            // loaded game begins, takes as it is when idle (state 0, 5, or 6) and asks
            // for that ability; taken as the newest such unit.  Read: the quiet test
            // map's last acolyte; Forestwalk's computer's an acolyte at 2.5 and 160
            // seconds, the port's idle ghoul there crashing the original as it loaded.
            // Provisional: which of them.)
            let repairs = |u: &crate::types::Unit| !u.is_building && u.abilities.iter().any(|(a, _)| matches!(self.objects.abilities.get(*a).map(|o| crate::objects::id_from_str(o.str("code").trim())), Some(c) if c == *codes::ability::id::REPAIR_ORC || c == *codes::ability::id::RESTORATION));
            let guard = if computer { self.unit_order.iter().filter_map(|&h| self.unit(h)).filter(|u| u.owner == player && !u.dead && repairs(u)).filter_map(|u| u.extra.object_slots[0].first().copied()).max_by_key(|s| s.1) } else { None }.unwrap_or((u32::MAX, u32::MAX));
            let w = |v: u32| Field::Word(v);
            let none = || [Field::Word(u32::MAX), Field::Word(u32::MAX)];
            let mut own = vec![w(0), w(player as u32), w(head.0), w(head.1), Field::Real(0.0)];
            for _ in 0..3 {
                own.extend(none());
                own.push(Field::Real(0.0));
            }
            // (A computer's 1, eight trees and their reals (read: the hero map's at 450
            // seconds, eight trees, 26 to 29; provisional, none), 7, and its town's place,
            // where its workers' mine is looked for (read: there, 1024 and 5888, the
            // defense captain's home; the port's 0 and 0 left an orc computer loaded
            // from its save mining nothing).)
            own.push(w(u32::from(computer && self.ai.contains_key(&player))));
            own.push(Field::List((0..8).map(|_| vec![w(u32::MAX), w(u32::MAX), Field::Real(0.0)]).collect()));
            own.push(Field::List((0..8).map(|_| vec![Field::Real(0.0)]).collect()));
            let town = if self.ai.contains_key(&player) { self.town_pos_pub(player, 0) } else { (0.0, 0.0) };
            own.extend([w(7), Field::Real(town.0), Field::Real(town.1)]);
            for _ in 0..2 {
                own.extend(none());
                own.push(Field::Real(0.0));
            }
            own.extend([w(guard.0), w(guard.1), Field::Real(0.0)]);
            // (Its towns, by their sites' indices, four places: the sites holding its town
            // halls, which TownWithMine looks through (`FUN_6f9ccf50`: a human's or orc's
            // site needing its hall, a night elf's its entangled mine).  Read: the hero
            // map's computer's 4, its Necropolis's site; the port's 1 for every computer
            // left an orc's and a night elf's without a town after the original loaded its
            // save, their gold peons sent to the trees.)
            let towns: Vec<u32> = if computer { (0..self.tags.sites.len()).filter(|&i| self.site_town(i).2.and_then(|h| self.unit(h)).is_some_and(|u| u.owner == player)).map(|i| i as u32).take(4).collect() } else { Vec::new() };
            own.push(Field::List((0..4).map(|t| vec![w(towns.get(t).copied().unwrap_or(u32::MAX))]).collect()));
            // (Its guard posts' list's head, the newest: read, Mafarazzo TD's computer's
            // 180th.)
            let post = self.last_guard_post(player);
            own.extend([w(post.0), w(post.1)]);
            own.extend([Field::Real(0.0), w(3), w(u32::MAX), w(0), w(0)]);
            own.push(Field::List((0..12).map(|_| vec![w(0)]).collect()));
            own.push(Field::List((0..12).map(|_| vec![w(0)]).collect()));
            // (Its threads' objects by their numbers, six places: `ai_Sleep` takes the
            // running thread's here, by its number at +0x29c; read, Forestwalk's
            // computer's main and its four begun at 2.5 seconds.)
            let mut threads: Vec<(u32, (u32, u32))> = self.ai.get(&player).map(|st| st.thread_objects.values().map(|o| (o.number, self.tagged(o.slot))).collect()).unwrap_or_default();
            threads.sort();
            own.push(Field::List((0..6).map(|k| match threads.get(k) {
                Some(&(_, r)) => vec![w(r.0), w(r.1), Field::Real(0.0)],
                None => vec![w(u32::MAX), w(u32::MAX), Field::Real(0.0)],
            }).collect()));
            own.push(w(0));
            // (Its attack and defense captains, as saved: `captain_records`.)
            for c in self.ai.get(&player).map(|st| [&st.attack, &st.defense]).into_iter().flatten() {
                match c.objects.first().filter(|s| c.objects.len() == 9 && self.tags.live.contains_key(s)) {
                    Some(&s) => {
                        let r = self.tagged(s);
                        own.extend([w(r.0), w(r.1)]);
                    }
                    None => own.extend(none()),
                }
                own.push(Field::Real(0.0));
            }
            if !self.ai.contains_key(&player) {
                for _ in 0..2 {
                    own.extend(none());
                    own.push(Field::Real(0.0));
                }
            }
            own.extend([w(if computer { 0x18_0183 } else { 0x10_0002 }), w(0)]);
            for _ in 0..2 {
                own.extend(none());
                own.push(Field::Real(0.0));
            }
            own.extend([w(0), w(0)]);
            let counts = self.tags.player_ai_timers.get(k).copied().unwrap_or([0; 3]);
            let timer = |period: f32, count: u32, event: u32| {
                let mut due = period.to_bits();
                while f32::from_bits(due) <= f32::from_bits(clock) {
                    due = crate::efloat::add(due, period.to_bits());
                }
                Field::Timer(rf(me), 2, Timing(Some((f32::from_bits(due), period, count, event, 1))))
            };
            let mut tail = vec![w(0), w(0), Field::List(Vec::new()), Field::List(Vec::new())];
            tail.push(timer(2.0, counts[0], 0xd_01c1));
            tail.push(timer(10.0, counts[1], 0xd_01c3));
            for _ in 0..5 {
                tail.push(Field::Timer(NONE, 0, Timing(None)));
            }
            tail.push(timer(0.5, counts[2], 0xd_01c5));
            out.push((slot, Second::Agent { own, handlers: Handlers { keyed: Vec::new(), fixed: Default::default(), events: vec![Vec::new(); 0x400] }, tail }));
        }
        out
    }

    /// The players' agents' own records (CPlayerWar3, `FUN_6f269d70`; read against the
    /// quiet test map's sixteen): the game's handler, and keyed by each of its flags'
    /// listeners (its own at slot +27, word 6; a player state registration's, word 0; the
    /// local player's interface's `` `fcl `` in the second table, after, by the flag)
    /// in four buckets by slot; its 63 event lists (0x8020b to 0x80233, 0x80305 to
    /// 0x80318, 0x80260, 0x80261: the script's registrations by event, and on 0x80261 the
    /// tech-change listeners of its units' queues, repair, restore, inventory, creep
    /// sleep, auras and their buffs, and abolish, heal, and inner fire, as made); its name
    /// and number; an empty selection; its alliance object; food cap and used, 96, race,
    /// color, controller, whether playing (the neutral players too), team and start or
    /// -1; its flag listener and the interface's; handicaps 1, 1; its tech table (each
    /// type's limit, made, in production two ways, present, killed, and kind); its
    /// structures' and other units' counts; its AI, twice, or -1; its bit; and constants.
    /// Provisional: the selection, what an AI or cinematics set, the counts' rules.
    pub fn player_agent_seconds(&self, client: &[Listener], rallies: &[(Ref, Ref)]) -> Vec<(u32, Second)> {
        let mut out = Vec::new();
        const TECH_LISTENERS: [&[u8; 4]; 12] = [codes::ability::id::QUEUE, codes::ability::id::REPAIR_ORC, codes::ability::id::RESTORATION, codes::ability::id::INVENTORY, codes::ability::id::CREEP_SLEEP, codes::ability::id::PRIESTESS_TRUESHOT_AURA, codes::buff::id::TRUESHOT_AURA, codes::ability::id::PALADIN_DEVOTION_AURA, codes::ability::id::MANA_REGENERATION_AURA, codes::ability::id::ABOLISH_MAGIC, codes::ability::id::HEAL_CREEP_NORMAL, codes::ability::id::INNER_FIRE];
        let list_index = |e: u32| match e {
            0x8_020b..=0x8_0233 => Some((e - 0x8_020b) as usize),
            0x8_0305..=0x8_0318 => Some(41 + (e - 0x8_0305) as usize),
            0x8_0260 => Some(61),
            0x8_0261 => Some(62),
            _ => None,
        };
        let ais: Vec<(u32, u32)> = self.tags.player_ais.clone();
        let computers: Vec<usize> = (0..self.players.len()).filter(|&i| self.players[i].controller == 1).collect();
        for (i, &ps) in self.tags.player_slots.iter().enumerate() {
            let Some(p) = self.players.get(i) else { continue };
            let me = self.tagged(ps);
            let mut events: Vec<Vec<Vec<Field>>> = vec![Vec::new(); 63];
            // The script's registrations of this player's events.
            for &id in &self.events {
                let Some(Obj::Event(r)) = self.handles.get(id) else { continue };
                let (player, event) = match r.kind {
                    EventKind::Player { player, event, .. } | EventKind::PlayerUnit { player, event, .. } => (player, event as u32 + 0x8_0200),
                    // (A chat registration on events 0x80210 and 0x80260: read, Mafarazzo
                    // TD's at 30 seconds.)
                    EventKind::PlayerChat { player, .. } => {
                        if player as usize == i {
                            if let Some(&reg) = self.tags.handle_ids.get(&id) {
                                for (k, e) in [(5, 0x8_0210), (61, 0x8_0260)] {
                                    events[k].push(vec![Field::Ref(rf(reg)), Field::Word(e)]);
                                }
                            }
                        }
                        continue;
                    }
                    _ => continue,
                };
                if player as usize != i || event == 0x8_0200 + crate::event_ids::EVENT_PLAYER_ALLIANCE_CHANGED as u32 {
                    continue;
                }
                let (Some(k), Some(&reg)) = (list_index(event), self.tags.handle_ids.get(&id)) else { continue };
                events[k].push(vec![Field::Ref(rf(reg)), Field::Word(event)]);
            }
            // Its units' tech-change listeners, as they registered: first those made with
            // their units (an aura's buff on its own caster among them), by birth; then
            // the creeps' autocast spells as they woke, the last creep first, each's spells
            // from its last; then the auras' buffs given later, as made.
            let (mut made, mut woke, mut later): (Vec<(u32, u32)>, Vec<(u32, u32)>, Vec<(u32, u32)>) = Default::default();
            for &h in self.unit_order.iter().rev() {
                let Some(u) = self.unit(h).filter(|u| u.owner as usize == i) else { continue };
                let chain = self.ability_chain(h, rallies);
                let creep = chain.iter().any(|(_, c)| c == codes::ability::id::CREEP_SLEEP);
                let own: Vec<u32> = u.extra.object_slots.iter().flatten().map(|s| s.0).collect();
                let mut spells = Vec::new();
                for (t, code) in chain {
                    if !TECH_LISTENERS.contains(&&code) {
                        continue;
                    }
                    if creep && matches!(&code, codes::ability::id::ABOLISH_MAGIC | codes::ability::id::HEAL_CREEP_NORMAL | codes::ability::id::INNER_FIRE) {
                        spells.push(t);
                    } else if code[0] == b'B' && !own.contains(&t.0) {
                        later.push(t);
                    } else {
                        made.push(t);
                    }
                }
                spells.sort_by_key(|t| std::cmp::Reverse(t.1));
                woke.extend(spells);
            }
            made.sort_by_key(|t| t.1);
            later.sort_by_key(|t| t.1);
            events[62].extend(made.iter().chain(&woke).chain(&later).map(|&t| vec![Field::Ref(rf(t)), Field::Word(0x8_0261)]));
            // Keyed by its flags' listeners.
            let mut keyed: Vec<((u32, u32), u32, u32)> = vec![(self.tagged(ps + 27), 6, 0)];
            for &id in &self.events {
                let Some(Obj::Event(r)) = self.handles.get(id) else { continue };
                if !matches!(r.kind, EventKind::PlayerState { player, .. } if player as usize == i) {
                    continue;
                }
                let (Some(&l), Some(&reg)) = (self.tags.event_timers.get(&id), self.tags.handle_ids.get(&id)) else { continue };
                keyed.push((self.tagged(l), 0, reg.0 | 0x4000_0000));
            }
            let mut keyed: Vec<(u32, Vec<(Ref, u32)>)> = {
                keyed.sort_by_key(|k| (k.0 .0 & 3, k.0 .1));
                keyed.into_iter().map(|(l, w, reg)| if reg & 0x4000_0000 != 0 { (l.0, vec![(rf(self.tagged(reg & !0x4000_0000)), w)]) } else { (l.0, vec![(rf(me), w)]) }).collect()
            };
            // (The interface's, by the state each watches: gold, lumber, the two upkeep
            // rates, the food cap, the food used.)
            const WATCHED: [u32; 6] = [1, 2, 13, 14, 4, 5];
            let mut mine: Vec<&Listener> = client.iter().filter(|l| l.owner == me).collect();
            mine.sort_by_key(|l| WATCHED.iter().position(|&w| w == l.flag.0.wrapping_sub(ps + 1)).unwrap_or(9));
            for (k, l) in mine.iter().enumerate() {
                let pos = keyed.iter().rposition(|(t, _)| t & 3 <= l.me.0 & 3).map_or(0, |x| x + 1);
                keyed.insert(pos, (l.me.0, vec![(rf(me), k as u32)]));
            }
            let handlers = Handlers { keyed, fixed: game_handlers(self.game_object()).fixed, events: vec![Vec::new(); 0x400] };
            let mut tail: Vec<Field> = events.into_iter().map(Field::List).collect();
            let none = || Field::Ref(NONE);
            let empty = || Field::List(Vec::new());
            let words = |n: usize, w: u32| Field::List((0..n).map(|_| vec![Field::Word(w)]).collect());
            // (An empty slot of the map's without a name: read, Mafarazzo TD's.)
            let name = if i < 12 && p.slot_state == crate::types::slot_state::EMPTY { String::new() } else { p.name.clone() };
            tail.extend([Field::Text(name), Field::Byte(i as u8), empty(), Field::Word(10)]);
            tail.extend((0..20).map(|_| empty()));
            tail.extend([none(), none(), empty(), empty(), Field::Word(u32::MAX), empty(), none(), none(), empty(), Field::Word(0), Field::Word(u32::MAX), Field::Word(i as u32), none(), Field::Word(0), Field::Ref(rf(self.tagged(ps + 28)))]);
            tail.extend([words(12, 0), words(12, 0)]);
            let playing = p.slot_state == 1 || i >= 12;
            let or_none = |v: i32| if playing && v >= 0 && i < 12 { v as u32 } else if i < 12 && playing { v as u32 } else { u32::MAX };
            // (Its race preference, +0x25c, `FUN_6f1e9dd0`'s: a map fixing its players'
            // settings its player's race alone, else random or the race chosen, with
            // 0x40, chosen in the lobby; the neutral players 0x60.  Read: Mafarazzo TD's
            // 1 for its twelve players, Booty Bay's 0x60.)
            let preference = if i >= 12 {
                0x60
            } else if self.fixed_player_settings {
                [0, 1, 2, 8, 4].get(p.race as usize).copied().unwrap_or(0x20)
            } else {
                0x40 | if p.race_preference & 0x3f != 0 { p.race_preference as u32 & 0x3f } else { 0x20 }
            };
            // (Its start location, +0x274, kept for an empty slot of the map's, then its
            // team, +0x278, none for one not playing: read, Mafarazzo TD's computer at
            // start 11 in team 1, its empty slots each at its own start in no team.)
            let start = if i < 12 && p.start_location >= 0 { p.start_location as u32 } else { u32::MAX };
            tail.extend([
                Field::Word(p.get_state(crate::types::player_state::RESOURCE_FOOD_CAP) as u32),
                Field::Word(p.get_state(crate::types::player_state::RESOURCE_FOOD_USED) as u32),
                Field::Word(0),
                Field::Word(0),
                Field::Word(0),
                Field::Word(preference),
                // (An unused slot's race: beyond the map's players the first six 4 and
                // the rest 3, an empty one of the map's 3; read on Booty Bay and
                // Forestwalk alone: provisional.)
                Field::Word(if p.race != 0 || i >= 12 {
                    p.race as u32
                } else if (i as i32) < self.player_count {
                    3
                } else if (i as i32) - self.player_count < 6 {
                    4
                } else {
                    3
                }),
                Field::Word(p.color as u32),
                Field::Word(p.controller as u32),
                Field::Word(playing as u32),
                Field::Word(start),
                Field::Word(or_none(p.team)),
            ]);
            tail.push(Field::Ref(rf(self.tagged(ps + 27))));
            // (The interface's listeners again, by birth.)
            let mut by_birth = mine.clone();
            by_birth.sort_by_key(|l| l.me.1);
            for k in 0..6 {
                tail.push(Field::Ref(by_birth.get(k).map_or(NONE, |l| rf(l.me))));
            }
            tail.extend([Field::Real(p.handicap), Field::Real(p.handicap_xp)]);
            tail.push(Field::List((0..7).map(|_| vec![none()]).collect()));
            tail.push(words(7, 0));
            tail.push(Field::Word(0));
            tail.push(Field::List(p.tech_regs.iter().map(|t| {
                let max = t.max as u32;
                let mut e = vec![Field::Word(u32::from_be_bytes(t.id))];
                e.extend([max, t.made as u32, t.producing as u32, t.changing as u32, t.present as u32, t.killed as u32, t.flags].map(|v| Field::Real(f32::from_bits(v))));
                e
            }).collect()));
            tail.push(Field::List(Vec::new()));
            let (mut structures, mut others) = (0u32, 0u32);
            for &h in &self.unit_order {
                if let Some(u) = self.unit(h).filter(|u| u.owner as usize == i && !u.dead) {
                    if u.is_building { structures += 1 } else { others += 1 }
                }
            }
            let ai = if i < 12 { computers.iter().position(|&c| c == i).and_then(|k| ais.get(k)) } else if i == 12 { ais.last() } else { None };
            let ai = ai.map_or(u32::MAX, |a| a.0);
            // (The players whose sight it shares, +0x2e0, `FUN_6f26b3d0`: itself and each
            // sharing vision with it; read, Mafarazzo TD's eleven allies' 0x7ff.)
            let sight = (0..crate::types::PLAYER_COUNT).filter(|&k| k == i || self.players.get(k).is_some_and(|o| o.alliances[i][crate::fog::ALLIANCE_SHARED_VISION])).fold(0u32, |m, k| m | 1 << k);
            tail.extend([Field::Word(0), Field::Word(structures), Field::Word(0), Field::Word(others), Field::Word(ai), Field::Word(ai), Field::Word(0), Field::Word(sight)]);
            tail.push(Field::List(Vec::new()));
            // (Its leaderboard, +0x2d4: PlayerSetLeaderboard's.)
            let board = self.player_leaderboard_object(i as u8).map_or(NONE, rf);
            tail.extend([Field::Word(board.0), Field::Word(board.1), Field::Word(0), Field::Timer(NONE, 0, Timing(None)), Field::Word(1), Field::Word(1), Field::Word(1), Field::Text(String::new()), Field::Word(1), Field::Word(0), Field::Word(0), Field::Word(0), Field::Word(0), Field::Word(0)]);
            // (Its own word 0 for the local player and an empty slot of the map's, 2 for
            // the others: read, Mafarazzo TD's.)
            let own = if i as u8 == self.local_player || (i < 12 && p.slot_state == crate::types::slot_state::EMPTY) { 0 } else { 2 };
            out.push((ps, Second::Agent { own: vec![Field::Word(own)], handlers, tail }));
        }
        out
    }

    /// A unit's five reals (`` `rel ``, read against the quiet test map's): its life,
    /// mana, two of -0 (to 2048 and to 1), and sight, each its value, rate, and bounds,
    /// listed after its `` `pcu `` (the unit's properties); life and mana with their
    /// modifiers' and listeners' heads (its regeneration's `` `rcm ``, its death's
    /// `` `rtl ``).
    pub fn unit_real_records(&self, h: u32) -> Vec<(u32, First, Option<Second>)> {
        let mut out = Vec::new();
        let Some(u) = self.unit(h) else { return out };
        let Some(o) = self.objects.units.get(u.type_id) else { return out };
        let first = &u.extra.object_slots[0];
        if first.len() < 10 {
            return out;
        }
        let me = first[0];
        let props: Vec<(u32, u32)> = vec![first[1], first[4], first[5], first[6], first[7], first[8]];
        // (Each real's newest listener its head, as the listeners chain them.)
        let thresholds = self.unit_thresholds(h);
        let newest_on = |p: (u32, u32)| thresholds.iter().filter(|l| l.1 == p).map(|l| l.0).last();
        let (rtl, mana_heard) = (newest_on(first[4]), newest_on(first[5]));
        let (hp_rate, mp_rate) = self.regen_rates(h).unwrap_or((0.0, 0.0));
        let regen_life = matches!(o.str("regenType").to_ascii_lowercase().as_str(), "always" | "night" | "day") && !u.extra.no_life_rcm;
        // (Regeneration on blight: its `` `rcm `` made as the unit stands on blight, the
        // life stamped then; read, the undead's at 2 seconds.)
        let blight_rcm = u.extra.blight_rcm.filter(|s| self.tags.live.contains_key(s)).map(|s| self.tagged(s));
        // (Each real's head its newest modifier, its rate theirs together.)
        let all_mods = self.unit_modifiers(h);
        let newest = |k: usize| all_mods.iter().filter(|m| m.prop == k).map(|m| m.r).last();
        let life_rcm = newest(1);
        let mana_rcm = newest(2);
        // (A structure being built's its rise.)
        let rising = self.build_parts(h).map(|p| p[4]);
        let life_rate: f32 = if all_mods.iter().any(|m| m.prop == 1) { all_mods.iter().filter(|m| m.prop == 1 && (m.word == 0x6005_0044 || Some(m.r) == rising)).map(|m| m.values[0]).sum() } else { hp_rate };

        let night = self.night_sight;
        let sight = if self.ultravision(u) { o.real("sight") } else { o.real("sight") + (o.real("nsight") - o.real("sight")) * night };
        // Mana regenerating is kept as its value at its stamp (read: 0 for the creeps'
        // mana at the start, their rates beside it; provisional: the port's mana, a
        // tick's regeneration ahead, taken back by its rate since then).
        let mana_stamp = self.made_abs(props[2].0).map_or(0.0, f32::from_bits);
        let mana = if mp_rate > 0.0 && u.mana < u.max_mana {
            let base = u.mana - mp_rate * (self.time - mana_stamp);
            if base.abs() < mp_rate * 0.05 { 0.0 } else { base.max(0.0) }
        } else {
            u.mana
        };
        // (Mana without a pool is -0, read: every unit without mana's.)
        let mana = if u.max_mana == 0.0 && mana == 0.0 { -0.0 } else { mana };
        // (While its day change's modifier lasts, its sight as the change began, at that
        // rate, stamped as it was made.)
        let ramp = all_mods.iter().find(|m| m.prop == 5).map(|m| (m.r, m.values[0]));
        let sight_now = if let Some(_) = ramp { self.day_change_sight(h).0 } else if sight == 0.0 && u.extra.sight_stamp.is_none() { -0.0 } else { sight };
        let values: [[f32; 4]; 5] = [
            [u.life, life_rate, 0.0, u.max_life],
            [mana, mp_rate, 0.0, u.max_mana],
            [-0.0, 0.0, 0.0, 2048.0],
            [-0.0, 0.0, 0.0, 1.0],
            // (A sight of none never changed is -0, as made: read, the haunted mine's
            // hidden gold mine, made after the day's change set the others' to 0.)
            [sight_now, ramp.map_or(0.0, |r| r.1), -f32::MAX, f32::MAX],
        ];
        for k in 1..=5 {
            let mut base = self.stamped(listed_property_base(&props, k, me));
            if k == 1 {
                if let Some(r) = blight_rcm.filter(|_| !regen_life) {
                    base.real = self.tags.made_at.get(&r.0).map_or(0.0, |&c| f32::from_bits(c));
                }
            }
            match k {
                1 => base.links = [life_rcm.map_or(Link(NONE, 0), |r| Link(rf(r), 1)), rtl.map_or(Link(NONE, 0), |r| Link(rf(r), 1))],
                2 => base.links = [mana_rcm.map_or(Link(NONE, 0), |r| Link(rf(r), 1)), mana_heard.map_or(Link(NONE, 0), |s| Link(rf(s), 1))],
                5 => match ramp {
                    Some((r, _)) => {
                        base.real = self.tags.made_at.get(&r.0).map_or(0.0, |&c| f32::from_bits(c));
                        base.links = [Link(rf(r), 1), Link(NONE, 0)];
                    }
                    // (Else stamped as made: read, Mafarazzo TD's creeps made at 20
                    // seconds, no day's change since.)
                    None => {
                        if let Some(s) = u.extra.sight_stamp {
                            base.real = s;
                        }
                    }
                },
                _ => {}
            }
            out.push((props[k].0, First::Real { base, values: values[k - 1] }, None));
        }
        out
    }

    /// A unit's continuous modifiers (`UnitModifier`), oldest first: its life's and
    /// mana's regeneration (and on blight its life's), each Unholy Aura buff's on its
    /// life, and its last day change's on its sight while that lasts.
    fn unit_modifiers(&self, h: u32) -> Vec<UnitModifier> {
        let mut out = Vec::new();
        let Some(u) = self.unit(h) else { return out };
        let Some(o) = self.objects.units.get(u.type_id) else { return out };
        if u.extra.object_slots[0].len() < 10 {
            return out;
        }
        let live = |s: u32| self.tags.live.get(&s).is_some_and(|e| &e.0 == codes::class::CONTINUOUS_MODIFIER);
        let rcms: Vec<(u32, u32)> = u.extra.object_slots[1].iter().copied().filter(|&(s, _)| live(s)).collect();
        // (Not a kept slot another object has taken since: read, the town hall's
        // upgrade's modifiers, gone, their slots the acolytes' blight's.)
        let fresh = |r: &&(u32, u32)| self.tagged(r.0).1 == r.1;
        let (hp_base, hero_hp, mp_rate) = self.regen_parts(h).unwrap_or((0.0, 0.0, 0.0));
        let hp_rate = hp_base + hero_hp;
        let hero = self.hero_parts(h);
        let regen_life = matches!(o.str("regenType").to_ascii_lowercase().as_str(), "always" | "night" | "day") && !u.extra.no_life_rcm;
        // (A regeneration's node due 150 seconds after it was made, every 150.)
        let periodic = |r: (u32, u32)| {
            let made = self.made_abs(r.0).unwrap_or(0);
            let due = f32::from_bits(crate::efloat::add(made, 150f32.to_bits()));
            Timing(Some((due, 150.0, self.tags.prop_counts.get(&r.0).copied().unwrap_or(0), 0, 1)))
        };
        let regen = |r: (u32, u32), prop: usize, rate: f32| UnitModifier { r, prop, word: 0x6005_0044, values: [rate, f32::MAX], timing: periodic(r) };
        let mut k = 0;
        if regen_life {
            if let Some(&r) = rcms.get(k).filter(fresh) {
                out.push(regen(r, 1, if hero.is_some() { hp_base } else { hp_rate }));
                k += 1;
            }
        }
        // A hero's strength's on its life and intelligence's on its mana (its AHer's).
        if let Some((_, _, [s, i])) = hero {
            out.extend(s.map(|r| regen(r, 1, hero_hp)));
            out.extend(i.map(|r| regen(r, 2, mp_rate)));
        }
        // A structure being built's on its life: its rise from a tenth over the build
        // time, for that and a second more (0x60040044, `construction_records`).
        if let Some(p) = self.build_parts(h) {
            let bt = u.build_time.max(0.01);
            let span = bt + 1.0;
            let made = self.made_abs(p[4].0).unwrap_or(0);
            let due = f32::from_bits(crate::efloat::add(made, span.to_bits()));
            let count = self.tags.prop_counts.get(&p[4].0).copied().unwrap_or(0);
            // (In the engine's arithmetic: read, a Crypt's 1300 over 60, 19.499998.)
            let rate = f32::from_bits(crate::efloat::div(crate::efloat::mul(u.max_life.to_bits(), 0.9f32.to_bits()), bt.to_bits()));
            out.push(UnitModifier { r: p[4], prop: 1, word: 0x6004_0044, values: [rate, span], timing: Timing(Some((due, span, count, 0, 0))) });
        }
        if o.real("manaN") > 0.0 {
            if let Some(&r) = rcms.get(k).filter(fresh) {
                out.push(regen(r, 2, mp_rate));
            }
        }
        if let Some(s) = u.extra.blight_rcm.filter(|&s| live(s)) {
            out.push(regen(self.tagged(s), 1, if hero.is_some() { hp_base } else { hp_rate }));
        }
        // Each Unholy Aura buff's, its regeneration (read: Forestwalk's test map's
        // creeps by a Satyr Hellcaller at 2.5 seconds, 0.5 each, due 150.5).
        for b in u.buffs.iter().filter(|b| self.buff_kept(b)) {
            if let Some(&s) = self.tags.buff_modifiers.get(&b.slot).filter(|&&s| live(s)) {
                let r = self.tagged(s);
                out.push(regen(r, 1, b.life_regen));
            }
        }
        // The day's change's, its sight moving to the new over three seconds: its rate a
        // second and its three, one node due as it ends (read: the creeps' at the
        // melee setup's 8:00, 200 and 3.0, due 3.0, its sight real 800 then with rate
        // 200; a deer's 0).
        // (Its own still: freed three seconds on, its slot may be another's since.)
        if let Some(s) = u.extra.day_ref.filter(|&(s, b)| live(s) && self.tagged(s).1 == b).map(|x| x.0) {
            let r = self.tagged(s);
            let made = self.made_abs(s).unwrap_or(0);
            let due = f32::from_bits(crate::efloat::add(made, 3f32.to_bits()));
            let count = self.tags.prop_counts.get(&s).copied().unwrap_or(0);
            let (from, to) = self.day_change_sight(h);
            out.push(UnitModifier { r, prop: 5, word: 0x6004_0044, values: [(to - from) / 3.0, 3.0], timing: Timing(Some((due, 3.0, count, 0, 0))) });
        }
        out.sort_by_key(|m| m.r.1);
        out
    }

    /// A unit's order as saved: its objects' records, its current task, its order, and
    /// the order after it, and whether it works in a haunted mine.  So far an AI's
    /// harvest at a haunted mine with its acolyte in the mine (read against Forestwalk's
    /// undead computer's at 2.5 seconds): its order (`ordt`: harvest, 852018, the mine)
    /// and the harvest's tasks (`tskA` 852322, three tasks 852326, 852298, 852296, and
    /// the target task 852355 on the mine), each naming the one before, the last the
    /// unit's current task; the order its own next.  (`AI_HARVEST`'s fourth to ninth,
    /// as `mining.rs` keeps them.)
    pub(crate) fn saved_order(&self, h: u32) -> Option<SavedOrder> {
        self.harvest_order(h).or_else(|| self.summoning_order(h)).or_else(|| self.gold_run_order(h)).or_else(|| self.move_order(h))
    }

    /// A player's (or the map script's) order to walk to a point, walking (read against
    /// Forestwalk's peasants sent off by script, 3 seconds in): its order (`ordt`: 0,
    /// move 851986, 0, none, 0, the point, none, five 0, 0.0, 0.0; none, 0, two none),
    /// `tskA` 852322, `tskO` 852326 with the order, tasks 852296, 852298, 852325, and the
    /// point task `tsk.` 852331 (the point, 0.0; a none), and its walk (`walk_records`
    /// to the point).  (`Tags::player_move_issued`'s first and third to eighth, the
    /// walk's cluster and path its last two.)
    fn move_order(&self, h: u32) -> Option<SavedOrder> {
        let u = self.unit(h)?;
        let crate::types::Action::Move { x, y } = u.action else { return None };
        let o = &u.extra.order_tags;
        if !u.extra.player_walk || !matches!(o.len(), 17 | 18) {
            return None;
        }
        let places = [0usize, 2, 3, 4, 5, 6, 7];
        let kinds: [&[u8; 4]; 7] = [codes::class::ORDER_TASKS, codes::class::TASK_A, codes::class::TASK_O, codes::class::TASK, codes::class::TASK, codes::class::TASK, b"tsk."];
        let t0 = self.tagged(o[0]);
        let at = |k: usize, c: &[u8; 4]| o.get(k).map(|&s| self.tagged(s)).filter(|t| t.1 == t0.1 + k as u32 && self.tags.live.get(&t.0).is_some_and(|e| &e.0 == c));
        let t: Vec<(u32, u32)> = places.iter().zip(kinds).map(|(&k, c)| at(k, c)).collect::<Option<_>>()?;
        const MOVE: u32 = 851_986;
        let w = Field::Word;
        let r = Field::Real;
        let handlers = || Handlers { keyed: Vec::new(), fixed: Default::default(), events: vec![Vec::new(); 0x400] };
        let mut records = Vec::new();
        // (Its third word its unit's owner: read, Mafarazzo TD's creeps' 11, Forestwalk's
        // peasants' 0; provisional, the owner or the player ordering.)
        records.push((t[0].0, agent_first(t[0], codes::class::ORDER_TASKS, &[]), Some(Second::Agent { own: vec![w(0), w(MOVE), w(u.owner as u32), w(u32::MAX), w(u32::MAX), w(0), r(x), r(y), w(u32::MAX), w(u32::MAX), w(0), w(0), w(0), w(0), w(0), r(0.0), r(0.0)], handlers: handlers(), tail: vec![w(u32::MAX), w(u32::MAX), w(0), Field::Ref(NONE), Field::Ref(NONE)] })));
        let events = [852_322, 852_326, 852_296, 852_298, 852_325, 852_331];
        for k in 1..t.len() {
            let prev = if k == 1 { (u32::MAX, u32::MAX) } else { t[k - 1] };
            let mut own = vec![w(0), w(prev.0), w(prev.1), w(0), w(events[k - 1])];
            let mut tail = Vec::new();
            match k {
                1 => own.push(w(0)),
                2 => own.push(w(MOVE)),
                6 => {
                    own.extend([r(x), r(y), r(0.0)]);
                    tail.push(Field::Ref(NONE));
                }
                _ => {}
            }
            records.push((t[k].0, agent_first(t[k], kinds[k], &[]), Some(Second::Agent { own, handlers: handlers(), tail })));
        }
        let n = o.len();
        let walk = at(n - 2, codes::class::CLUSTER).zip(at(n - 1, codes::class::PATH)).filter(|_| u.extra.mover.is_some());
        if let Some((c, p)) = walk {
            records.extend(self.point_walk_records(h, c, p, (x, y)));
        }
        // (Its work 7: read, a peasant sent to a point.)
        let state = OrderState { kind: 7, walking: walk.is_some(), after_tint: [1, if walk.is_some() { 5 } else { 4 }, 0, 0], ..Default::default() };
        Some(SavedOrder { records, task: t[6], order: t[0], next: t[0], mine: false, timer: None, summoning: false, in_gold_mine: false, walking: walk.is_some(), state, walk: walk.map(|(c, p)| (c, p, 0)), queue: None })
    }

    /// A walk to a point's cluster and path (read against Forestwalk's peasants sent off
    /// by script): the cluster of its walker's position with its local goal (where its
    /// own path leads now) and heading, no target, the goal and the local goal, 0x20000,
    /// and the path; the path the goal thrice, its owner word ((0x206 | 0x8 local) << 4 |
    /// owner) << 16, its collision, no short way, its long way (the coarse way's corners
    /// in half cells, else the goal and where it stands).  Provisional: the counters,
    /// the coarse goal (the goal written), the cluster's speed (the walker's top
    /// moving).
    fn point_walk_records(&self, h: u32, clu: (u32, u32), path: (u32, u32), goal: (f32, f32)) -> Vec<(u32, First, Option<Second>)> {
        let mut out = Vec::new();
        let (Some(u), Some(p)) = (self.unit(h), self.pathing.as_ref()) else { return out };
        let Some(&pcu) = u.extra.object_slots[0].get(1) else { return out };
        let cells = |(x, y): (f32, f32)| ((x - p.origin[0]) / 32.0, (y - p.origin[1]) / 32.0);
        let (gx, gy) = cells(goal);
        // (Its local goal the point itself while its way is direct.)
        let (lx, ly) = if u.path.len() > 1 { cells(u.path[u.path.len() - 1]) } else { (gx, gy) };
        let (cx, cy) = cells((u.x, u.y));
        let heading = u.extra.mover.as_ref().map_or(0.0, |m| f32::from_bits(m.heading));
        let moving = u.extra.mover.as_ref().is_some_and(|m| f32::from_bits(m.vx) != 0.0 || f32::from_bits(m.vy) != 0.0);
        let speed = if moving { self.walk_speed_cells(h) } else { 0.0 };
        let array = Array { words: [528, 528, 0, 12], entries: vec![([pcu.0, pcu.1, 0], [0.0, 0.0, lx, ly, speed, heading], 0)] };
        // (A walker whose pathing a script turned off: its cluster's word 0, and its
        // path's owner word without 0x200; read, Mafarazzo TD's creeps; provisional.)
        let pathing = !u.extra.no_pathing;
        out.push((clu.0, First::Cluster2 { array, words: [u32::MAX, u32::MAX, 0], reals: [gx, gy, lx, ly], more: [0, 0, 0, 0, 0], last_reals: [heading, f32::MAX], word: if pathing { 0x2_0000 } else { 0 }, refs: [NONE, NONE, rf(path)] }, None));
        // (A collision of none as one: read, Mafarazzo TD's creeps' 0.03125.)
        let collision = self.objects.units.get(u.type_id).map_or(0.0, |o| o.real("collision")).max(1.0) / 32.0;
        let local = if u.owner == self.local_player { 0x8 } else { 0 };
        let word = (((if pathing { 0x206 } else { 0x006 }) | local) << 4 | (u.owner as u32 & 0xf)) << 16;
        let corners: Vec<[f32; 2]> = match u.extra.coarse.as_ref() {
            Some(c) => c.points.iter().map(|&(x, y)| [x, y]).collect(),
            None => vec![[gx / 2.0, gy / 2.0], [cx / 2.0, cy / 2.0]],
        };
        let short = (Array { words: [0, 0, 128, 0], entries: Vec::new() }, u32::MAX, 0, 700);
        // (Its long way's corner it walks to, from the goal's 0: read, Mafarazzo TD's
        // creeps' 3 of 8 halfway.)
        let index = u.extra.coarse.as_ref().map_or(0, |c| c.index as u32);
        let long = (path_array(corners), index, 0, 5000);
        out.push((path.0, First::Path { reals: [gx, gy, gx, gy, gx, gy], words: [word, 0, 0, u32::MAX, u32::MAX, 0], real: collision, word: 0, ways: [short, long], refs: [NONE, NONE, NONE, NONE] }, None));
        out
    }

    /// A worker's gold run's order (read against Forestwalk's human peasants at 12
    /// seconds, sent to mine by script): its own harvest order from the hall (`ordt`:
    /// 0, harvest 852018, 0, none, 0, the mine's place, the mine, five 0, 0.0, 0.0;
    /// none, 0, two none) with `tskA` 852322, tasks 852326, 852298, 852296, 852306,
    /// and target tasks 852369, 852355, 852354 on the mine, so far in the mine; walking
    /// to it a task 852305 and target task 852337 more.  Its order home (`ordt` 852017
    /// at the hall's place, the hall) with `tskA` 852322, tasks 852326, 852296,
    /// 852298, 852306, and target tasks 852358 and 852340 on the hall.  The order its
    /// own next.  (`goldrun.rs`'s `HARVEST_OWN` and `RETURN`, by place, each born that
    /// many after its order.)  Provisional: a walk's cluster and path not written.
    fn gold_run_order(&self, h: u32) -> Option<SavedOrder> {
        use crate::goldrun::{Kind, Phase};
        let u = self.unit(h)?;
        let r = u.extra.gold_run.as_ref()?;
        // (Chopping lumber: its harvest order on the tree with `tskA` 852322, tasks 852326,
        // 852296, 852298, 852306, and target tasks 852357 and 852356 on the tree; read,
        // Forestwalk's peasants sent to a tree by script, at 12 seconds.)
        const CHOP: [(&[u8; 4], u32); 8] = [(codes::class::ORDER_TASKS, 852_018), (codes::class::TASK_A, 852_322), (codes::class::TASK, 852_326), (codes::class::TASK, 852_296), (codes::class::TASK, 852_298), (codes::class::TASK, 852_306), (codes::class::TASK_T, 852_357), (codes::class::TASK_T, 852_356)];
        // (Walking to its tree, the walk's target task 852337 on it more, 116.0.)
        const CHOP_WALK: [(&[u8; 4], u32); 9] = [(codes::class::ORDER_TASKS, 852_018), (codes::class::TASK_A, 852_322), (codes::class::TASK, 852_326), (codes::class::TASK, 852_296), (codes::class::TASK, 852_298), (codes::class::TASK, 852_306), (codes::class::TASK_T, 852_357), (codes::class::TASK_T, 852_356), (codes::class::TASK_T, 852_337)];
        if r.lumber && !matches!(r.phase, Phase::Chopping(_) | Phase::ToMine | Phase::ToHall(_)) {
            return None;
        }
        const OWN: [(&[u8; 4], u32); 11] = [(codes::class::ORDER_TASKS, 852_018), (codes::class::TASK_A, 852_322), (codes::class::TASK, 852_326), (codes::class::TASK, 852_298), (codes::class::TASK, 852_296), (codes::class::TASK, 852_306), (codes::class::TASK_T, 852_369), (codes::class::TASK_T, 852_355), (codes::class::TASK_T, 852_354), (codes::class::TASK, 852_305), (codes::class::TASK_T, 852_337)];
        const HOME: [(&[u8; 4], u32); 8] = [(codes::class::ORDER_TASKS, 852_017), (codes::class::TASK_A, 852_322), (codes::class::TASK, 852_326), (codes::class::TASK, 852_296), (codes::class::TASK, 852_298), (codes::class::TASK, 852_306), (codes::class::TASK_T, 852_358), (codes::class::TASK_T, 852_340)];
        // (Waiting at the mine as in it; the player's order the worker's own a place on,
        // its first block gone: read, Forestwalk's peasants at 3 seconds.)
        let (list, target): (&[(&[u8; 4], u32)], u32) = match (r.kind, r.phase) {
            (Kind::Own | Kind::Player, Phase::Chopping(_)) => (&CHOP[..], r.mine),
            (Kind::Own | Kind::Player, Phase::ToMine) if r.lumber => (&CHOP_WALK[..], r.mine),
            (Kind::Own | Kind::Player, Phase::InMine(_) | Phase::Waiting(_)) => (&OWN[..9], r.mine),
            (Kind::Own | Kind::Player, Phase::ToMine) => (&OWN[..], r.mine),
            (Kind::Return, Phase::ToHall(hall)) => (&HOME[..], hall),
            _ => return None,
        };
        let places: Vec<usize> = (0..list.len()).map(|k| if r.kind == Kind::Player && k > 0 { k + 1 } else { k }).collect();
        if r.order.len() <= *places.last()? {
            return None;
        }
        let t0 = self.tagged(r.order[0]);
        let t: Vec<(u32, u32)> = places.iter().map(|&k| self.tagged(r.order[k])).collect();
        if !t.iter().zip(list).zip(&places).all(|((x, (c, _)), &k)| x.1 == t0.1 + k as u32 && self.tags.live.get(&x.0).is_some_and(|e| &e.0 == *c)) {
            return None;
        }
        // (Its target a unit, or a tree.)
        let (target_ref, tx, ty) = match self.unit(target) {
            Some(tu) => (tu.extra.object_slots[0].first().copied()?, tu.x, tu.y),
            None => match self.handles.get(target) {
                Some(crate::types::Obj::Destructable(d)) => (self.tagged(d.oid.0), d.x, d.y),
                _ => return None,
            },
        };
        let w = Field::Word;
        let handlers = || Handlers { keyed: Vec::new(), fixed: Default::default(), events: vec![Vec::new(); 0x400] };
        let mut records = Vec::new();
        let mut own = vec![w(0), w(list[0].1), w(0), w(u32::MAX), w(u32::MAX), w(0), Field::Real(tx), Field::Real(ty), w(target_ref.0), w(target_ref.1)];
        own.extend([w(0), w(0), w(0), w(0), w(0), Field::Real(0.0), Field::Real(0.0)]);
        records.push((t[0].0, agent_first(t[0], codes::class::ORDER_TASKS, &[]), Some(Second::Agent { own, handlers: handlers(), tail: vec![w(u32::MAX), w(u32::MAX), w(0), Field::Ref(NONE), Field::Ref(NONE)] })));
        for k in 1..t.len() {
            let prev = if k == 1 { (u32::MAX, u32::MAX) } else { t[k - 1] };
            let (kind, event) = list[k];
            let mut own = vec![w(0), w(prev.0), w(prev.1), w(0), w(event)];
            let mut tail = Vec::new();
            if kind == codes::class::TASK_A {
                own.push(w(0));
            } else if kind == codes::class::TASK_T {
                // (A walk to a tree's reach 116.0: read, a peasant's and a ghoul's.)
                let reach = if r.lumber && event == 852_337 { 116.0 } else { 0.0 };
                own.extend([w(target_ref.0), w(target_ref.1), w(0), Field::Real(reach)]);
                tail.push(Field::Ref(NONE));
            }
            records.push((t[k].0, agent_first(t[k], kind, &[]), Some(Second::Agent { own, handlers: handlers(), tail })));
        }
        let last = *t.last()?;
        let in_gold_mine = matches!(r.phase, Phase::InMine(_));
        let waiting = matches!(r.phase, Phase::Waiting(_)) || (r.lumber && matches!(r.phase, Phase::Chopping(_)));
        let home = matches!(r.phase, Phase::ToHall(_));
        // Its walk's cluster and path (the order's, by place).
        let at = |k: usize, c: &[u8; 4]| r.order.get(k).map(|&s| self.tagged(s)).filter(|x| x.1 == t0.1 + k as u32 && self.tags.live.get(&x.0).is_some_and(|e| &e.0 == c));
        let walk = if in_gold_mine || waiting {
            None
        } else {
            let (c, p) = if home { (14, 15) } else if r.lumber { (15, 16) } else { (17, 18) };

            at(c, codes::class::CLUSTER).zip(at(p, codes::class::PATH)).map(|(c, p)| (c, p, target))
        };
        if let Some((c, p, _)) = walk {
            records.extend(self.walk_records(h, c, p, target));
        }
        let walking = !in_gold_mine && !waiting;
        // (Chopping: 3, 4, 19, 36 after its tint and 0x80 on its last word, read.)
        // (Walking to its tree: 1, 5, 36, 36, no walking byte, read.)
        // (Home with its lumber: 2, 5, 36, 36, read.)
        let state = if r.lumber && walking {
            OrderState { kind: 5, inside_word: 0, walking: false, after_tint: [if home { 2 } else { 1 }, 5, 36, 36], inside: false }
        } else if r.lumber {
            OrderState { kind: 5, inside_word: 0, walking: false, after_tint: [3, 4, 19, 36], inside: true }
        } else {
            OrderState { kind: 5, inside_word: in_gold_mine as u32, walking, after_tint: [if home { 2 } else { 1 }, if walking { 5 } else { 4 }, 35, 0], inside: in_gold_mine }
        };
        // Waiting, its place in the mine's queue (a `uref`).
        let queue = match r.phase {
            Phase::Waiting(s) => Some(s).filter(|s| self.tags.live.get(s).is_some_and(|e| &e.0 == codes::class::UNIT_AI_REF)).map(|s| self.tagged(s)),
            _ => None,
        };
        Some(SavedOrder { records, task: last, order: t[0], next: t[0], mine: false, timer: None, summoning: false, in_gold_mine, walking, state, walk, queue })
    }

    /// An AI's build order on a worker summoning its structure (read against
    /// Forestwalk's undead computer's acolyte and its Crypt at 2.5 seconds): its
    /// order (`ord.`: 0, the structure's type, 1, its next order, 0, the site), that
    /// next order (`+ord`, 851974), and the build's tasks (`tskA` 852322, three tasks
    /// 852296, 852298, 852292, and the site task `tsk.`: the type, the site, 0.0), the
    /// last its current task; its order's end on its timer (event 0xd019d, a second
    /// from its arrival).  (`AI_BUILD`'s first and third to seventh with the last order
    /// made, as `ai_build_objects` keeps them.)
    fn summoning_order(&self, h: u32) -> Option<SavedOrder> {
        let u = self.unit(h)?;
        let end = u.extra.build_end?;
        let o = &u.extra.ai_order;
        if o.len() != crate::ai::AI_BUILD.len() {
            return None;
        }
        let idx = [0usize, 2, 3, 4, 5, 6];
        let kinds: [&[u8; 4]; 6] = [b"ord.", codes::class::TASK_A, codes::class::TASK, codes::class::TASK, codes::class::TASK, b"tsk."];
        if !idx.iter().zip(kinds).all(|(&k, c)| self.tags.live.get(&o[k]).is_some_and(|e| &e.0 == c)) || self.tags.live.get(&o[14]).is_none_or(|e| &e.0 != codes::class::ORDER) {
            return None;
        }
        let t: Vec<(u32, u32)> = idx.iter().map(|&k| self.tagged(o[k])).collect();
        let next = self.tagged(o[14]);
        let w = Field::Word;
        let r = Field::Real;
        let handlers = || Handlers { keyed: Vec::new(), fixed: Default::default(), events: vec![Vec::new(); 0x400] };
        let (kind, x, y) = (u.order.id as u32, u.order.target_x, u.order.target_y);
        let mut records = Vec::new();
        records.push((t[0].0, agent_first(t[0], b"ord.", &[]), Some(Second::Agent { own: vec![w(0), w(kind), w(1), w(next.0), w(next.1), w(0), r(x), r(y)], handlers: handlers(), tail: vec![w(u32::MAX), w(u32::MAX), w(0), Field::Ref(NONE)] })));
        records.push((next.0, agent_first(next, codes::class::ORDER, &[]), Some(Second::Agent { own: vec![w(0), w(851_974), w(1), w(u32::MAX), w(u32::MAX), w(0)], handlers: handlers(), tail: vec![w(u32::MAX), w(u32::MAX), w(0)] })));
        let events = [852_322, 852_296, 852_298, 852_292];
        for k in 1..t.len() {
            let prev = if k == 1 { (u32::MAX, u32::MAX) } else { t[k - 1] };
            let mut own = vec![w(0), w(prev.0), w(prev.1), w(0)];
            let mut tail = Vec::new();
            if k == t.len() - 1 {
                own.extend([w(kind), r(x), r(y), r(0.0)]);
                tail.push(Field::Ref(NONE));
            } else {
                own.push(w(events[k - 1]));
                if kinds[k] == codes::class::TASK_A {
                    own.push(w(0));
                }
            }
            records.push((t[k].0, agent_first(t[k], kinds[k], &[]), Some(Second::Agent { own, handlers: handlers(), tail })));
        }
        let me = u.extra.object_slots[0].first().copied()?;
        let count = self.tags.prop_counts.get(&me.0).copied().unwrap_or(0);
        let timer = Field::Timer(rf(me), 0, Timing(Some((end, 1.0, count, 0xd_019d, 0))));
        let state = OrderState { kind: 2, after_tint: [1, 4, 37, 0], ..Default::default() };
        Some(SavedOrder { records, task: t[5], order: t[0], next, mine: false, timer: Some(timer), summoning: true, in_gold_mine: false, walking: false, state, walk: None, queue: None })
    }

    /// A walk's cluster and path (read against Forestwalk's human peasants walking to
    /// their mine and home at 12 seconds): the cluster (`^clu`) of its walker's
    /// position with the goal (the target's place in cells), its speed, and its
    /// heading; the target's position; the goal; counters; a heading; 0x21000; the
    /// target's footprint region and the path.  The path (`l^pa`): the goal thrice, the
    /// walker's owner word, its collision in cells, its short way none and its long
    /// way from the goal to where it stands in half cells, and the target's region.
    /// Provisional: the counters (written 0, 1, 0), the cluster's headings (the
    /// walker's), the long way's start (where it stands, not where it set out).
    fn walk_records(&self, h: u32, clu: (u32, u32), path: (u32, u32), target: u32) -> Vec<(u32, First, Option<Second>)> {
        let mut out = Vec::new();
        let (Some(u), Some(p)) = (self.unit(h), self.pathing.as_ref()) else { return out };
        let Some((gx, gy, tpcu, region)) = self.run_target(target) else { return out };
        let Some(&pcu) = u.extra.object_slots[0].get(1) else { return out };
        let (cx, cy) = ((u.x - p.origin[0]) / 32.0, (u.y - p.origin[1]) / 32.0);
        let speed = self.walk_speed_cells(h);
        let heading = u.extra.mover.as_ref().map_or(0.0, |m| f32::from_bits(m.heading));
        let array = Array { words: [528, 528, 0, 12], entries: vec![([pcu.0, pcu.1, 0], [0.0, 0.0, gx, gy, speed, heading], 0)] };
        out.push((clu.0, First::Cluster2 { array, words: [tpcu.0, tpcu.1, 0], reals: [0.0, 0.0, gx, gy], more: [0, 1, 0, 0, 0], last_reals: [heading, f32::MAX], word: 0x2_1000, refs: [region, NONE, rf(path)] }, None));
        let o = self.objects.units.get(u.type_id);
        let collision = o.map_or(0.0, |o| o.real("collision")) / 32.0;
        let local = if u.owner == self.local_player { 0x8 } else { 0 };
        let word = ((0x46 | local) << 4 | (u.owner as u32 & 0xf)) << 16;
        let short = (Array { words: [0, 0, 128, 0], entries: Vec::new() }, u32::MAX, 0, 700);
        let long = (path_array(vec![[gx / 2.0, gy / 2.0], [cx / 2.0, cy / 2.0]]), 0, 0, 5000);
        out.push((path.0, First::Path { reals: [gx, gy, gx, gy, gx, gy], words: [word, 0, 0, u32::MAX, u32::MAX, 0], real: collision, word: 0, ways: [short, long], refs: [NONE, NONE, NONE, region] }, None));
        out
    }

    /// A worker's run's target as walks and paths name it: its place in cells, its
    /// position, and its region (a structure's second footprint region; a tree's
    /// fourth region: read, Forestwalk's mine and hall, and its trees).
    pub(crate) fn run_target(&self, target: u32) -> Option<(f32, f32, (u32, u32), Ref)> {
        let p = self.pathing.as_ref()?;
        if let Some(t) = self.unit(target) {
            let pcu = *t.extra.object_slots[0].get(1)?;
            let region = t.extra.footprint_slots.get(1).map_or(NONE, |&s| rf(self.tagged(s)));
            return Some(((t.x - p.origin[0]) / 32.0, (t.y - p.origin[1]) / 32.0, pcu, region));
        }
        match self.handles.get(target) {
            Some(crate::types::Obj::Destructable(d)) => {
                let pos = self.tagged(*d.object_slots.first()?);
                let region = d.object_slots.get(4).map_or(NONE, |&s| rf(self.tagged(s)));
                Some(((d.x - p.origin[0]) / 32.0, (d.y - p.origin[1]) / 32.0, pos, region))
            }
            _ => None,
        }
    }

    /// A walker's top speed in cells a second: its type's, by its gold run's factor.
    fn walk_speed_cells(&self, h: u32) -> f32 {
        let Some(u) = self.unit(h) else { return 0.0 };
        // (Its speed as set, SetUnitMoveSpeed's: read, Mafarazzo TD's creeps'.)
        let speed = u.move_speed;
        let factor = u.extra.gold_run.as_ref().map_or(1.0, |r| f32::from_bits(r.factor));
        speed * factor / 32.0
    }

    /// An AI's harvest at a haunted mine with its acolyte in the mine.
    fn harvest_order(&self, h: u32) -> Option<SavedOrder> {
        let u = self.unit(h)?;
        let kinds = [codes::class::ORDER_TASKS, codes::class::TASK_A, codes::class::TASK, codes::class::TASK, codes::class::TASK, codes::class::TASK_T];
        if u.extra.ai_order.len() != kinds.len() || !u.extra.ai_order.iter().zip(kinds).all(|(s, c)| self.tags.live.get(s).is_some_and(|e| &e.0 == c)) {
            return None;
        }
        let mine = self.unit(u.order.target).filter(|m| m.abilities.iter().any(|(a, _)| a == codes::ability::id::BLIGHTED_GOLD_MINE)).and_then(|m| m.extra.object_slots[0].first().copied())?;
        let t: Vec<(u32, u32)> = u.extra.ai_order.iter().map(|&s| self.tagged(s)).collect();
        let w = Field::Word;
        let handlers = || Handlers { keyed: Vec::new(), fixed: Default::default(), events: vec![Vec::new(); 0x400] };
        let mut records = Vec::new();
        let mut own = vec![w(0), w(852_018), w(1), w(u32::MAX), w(u32::MAX), w(0), Field::Real(0.0), Field::Real(0.0), w(mine.0), w(mine.1)];
        own.extend([w(0), w(0), w(0), w(0), w(0), Field::Real(0.0), Field::Real(0.0)]);
        records.push((t[0].0, agent_first(t[0], codes::class::ORDER_TASKS, &[]), Some(Second::Agent { own, handlers: handlers(), tail: vec![w(u32::MAX), w(u32::MAX), w(0), Field::Ref(NONE), Field::Ref(NONE)] })));
        let events = [852_322, 852_326, 852_298, 852_296, 852_355];
        for k in 1..t.len() {
            let prev = if k == 1 { (u32::MAX, u32::MAX) } else { t[k - 1] };
            let mut own = vec![w(0), w(prev.0), w(prev.1), w(0), w(events[k - 1])];
            let mut tail = Vec::new();
            match kinds[k] {
                c if c == codes::class::TASK_A => own.push(w(0)),
                c if c == codes::class::TASK_T => {
                    own.extend([w(mine.0), w(mine.1), w(0), Field::Real(0.0)]);
                    tail.push(Field::Ref(NONE));
                }
                _ => {}
            }
            records.push((t[k].0, agent_first(t[k], kinds[k], &[]), Some(Second::Agent { own, handlers: handlers(), tail })));
        }
        let state = OrderState { kind: 6, after_tint: [2, 4, 35, 0], inside: true, ..Default::default() };
        Some(SavedOrder { records, task: t[5], order: t[0], next: t[0], mine: true, timer: None, summoning: false, in_gold_mine: false, walking: false, state, walk: None, queue: None })
    }

    /// The orders' records (`saved_order`).
    pub fn saved_order_records(&self) -> Vec<(u32, First, Option<Second>)> {
        self.unit_order.iter().filter_map(|&h| self.saved_order(h)).flat_map(|o| o.records).collect()
    }

    /// A gold mine's fog modifier (`+fgm`, made for the player whose worker went in):
    /// its agent and 72 bytes: 0x1d4, 1, 600.0, ten 0, the mine's place, 0, and a
    /// reference none (read: Forestwalk's human peasants' mine at 12 seconds;
    /// provisional, not yet read field by field, the player's own place in it).
    pub fn mine_fog_records(&self) -> Vec<(u32, First, Option<Second>)> {
        let mut out = Vec::new();
        for &h in &self.unit_order {
            let Some(u) = self.unit(h) else { continue };
            let Some((s, _)) = u.extra.mine_fog.filter(|(s, _)| self.tags.live.get(s).is_some_and(|e| &e.0 == codes::class::FOG_MODIFIER)) else { continue };
            let me = self.tagged(s);
            let mut b: Vec<u8> = Vec::new();
            for v in [0x1d4u32, 1, 600f32.to_bits()] {
                b.extend(v.to_le_bytes());
            }
            b.extend([0u8; 40]);
            for v in [u.x.to_bits(), u.y.to_bits(), 0, u32::MAX, u32::MAX] {
                b.extend(v.to_le_bytes());
            }
            out.push((me.0, agent_first(me, codes::class::FOG_MODIFIER, &[]), Some(Second::Raw(b))));
        }
        out
    }

    /// The worker in a gold mine's shaft and its timer to come out.
    fn shaft_worker(&self, shaft: u32) -> Option<((u32, u32), Field)> {
        let mine = self.unit_order.iter().copied().find(|&m| self.unit(m).is_some_and(|u| u.extra.object_slots[2].iter().any(|&(s, _)| s == shaft)))?;
        let m = self.unit(mine)?;
        let agld = m.extra.object_slots[2].iter().copied().find(|&(s, b)| self.tags.live.get(&s).is_some_and(|e| &e.0 == codes::ability::id::GOLD_MINE && e.1 == b))?;
        let (w, due) = self.unit_order.iter().filter_map(|&h| self.unit(h)).find_map(|w| match w.extra.gold_run.as_ref() {
            Some(r) if r.mine == mine => match r.phase {
                crate::goldrun::Phase::InMine(due) => Some((w, due)),
                _ => None,
            },
            _ => None,
        })?;
        let who = w.extra.object_slots[0].first().copied()?;
        let (dur, _) = self.mine_time_and_room(mine);
        Some((who, Field::Timer(rf(agld), 0, Timing(Some((f32::from_bits(due), dur, 0, 0xd_01c1, 0))))))
    }

    /// The objects of the items a unit holds, in its slots' order: each its tag, kind,
    /// and item.
    pub(crate) fn held_item_objects(&self, h: u32) -> Vec<((u32, u32), [u8; 4], u32)> {
        let Some(u) = self.unit(h) else { return Vec::new() };
        u.inventory.iter().filter_map(|&i| self.item(i).map(|it| (i, it))).flat_map(|(i, it)| it.ability_slots.iter().filter_map(move |&s| self.tags.live.get(&s).map(|e| ((s, e.1), e.0, i)))).collect()
    }

    /// The items' records (CItem, read against the melee start's Town Portal scroll on
    /// Forestwalk's computer's Dreadlord and a scripted item on Booty Bay's ground): its
    /// agent (its position its property); its own flags (held 0x2757, on the ground
    /// 0x3416); the game's handler; its sprite (its model, hidden while held), its
    /// owner, type, shorts (0, 0x8001 held), -99999, its life, 0, its HP, its charges,
    /// first ability, holder, 0, 15, its abilities' objects (four places), two timers;
    /// its position (its place in cells as it was made or dropped, facing 270°, 1/32)
    /// and regions (held: its block flagged 0x0c000000, the cell none, 0x18; on the
    /// ground: its block and cell, 0x01000018).  Provisional: the flags' meanings,
    /// the second 15, the sprite's fields, the search marks.
    pub fn item_records(&self) -> Vec<(u32, First, Option<Second>)> {
        let mut out = Vec::new();
        let Some(p) = self.pathing.as_ref() else { return out };
        let [coarse, fine] = self.pathing_map_objects();
        let game = self.game_object();
        let w = Field::Word;
        let timer = || Field::Timer(NONE, 0, Timing(None));
        for &i in &self.item_order {
            let Some(it) = self.item(i) else { continue };
            let Some(o) = self.objects.items.get(it.type_id) else { continue };
            if it.parts.len() != 3 || !self.tags.live.get(&it.oid.0).is_some_and(|e| e.1 == it.oid.1) || !it.parts.iter().all(|s| self.tags.live.contains_key(s)) {
                continue;
            }
            let me = it.oid;
            let (pos, r1, r2) = (self.tagged(it.parts[0]), self.tagged(it.parts[1]), self.tagged(it.parts[2]));
            let holder = self.unit(it.owner_unit).and_then(|u| u.extra.object_slots[0].first().copied()).filter(|_| it.owner_unit != 0);
            let held = holder.is_some();
            let model = o.str("file");
            let stands = self.named_sequences(&model, "stand");
            let births = self.named_sequences(&model, "birth");
            let sprite = Sprite {
                model,
                real: f32::from_bits(0x0384_0000 | if held { 0x0008_0000 } else { 0 }),
                short: if held { 0xfffe } else { stands.first().copied().unwrap_or(0) as u16 },
                byte: 255,
                word: if held { 0x0020_2020 } else { 0x00ff_ffff },
                reals: [1.0, 1.0],
                words: if held { [u32::MAX, 0xffff_feb3] } else { [4, 0] },
                last_real: f32::from_bits(150),
                last_byte: 0,
                flags: if held { [2, 0, 1] } else { [1, 1, 1] },
                entries: vec![(0.0, 0, births), (0.0, 4, stands)],
            };
            // (Its first ability, not its cooldown group: read, Claws of Attack's AItf
            // where its cooldownID is AIat.)
            let first_ability = self.item_abilities(it.type_id).first().map_or(0, |a| u32::from_be_bytes(*a));
            let (ht, hb) = holder.unwrap_or((u32::MAX, u32::MAX));
            let mut abilities: Vec<Vec<Field>> = it.ability_slots.iter().filter(|s| self.tags.live.get(s).is_some_and(|e| e.0[0] == b'A')).map(|&s| self.tagged(s)).map(|t| vec![w(t.0), w(t.1), Field::Real(0.0)]).collect();
            abilities.resize_with(4, || vec![w(u32::MAX), w(u32::MAX), Field::Real(0.0)]);
            let tail = vec![
                Field::Sprite(Some(sprite)),
                w(it.player as u32),
                w(u32::from_be_bytes(it.type_id)),
                Field::Short(0),
                Field::Short(if held { 0x8001 } else { 0 }),
                Field::Byte(0),
                Field::Byte(0),
                Field::Byte(0),
                Field::Real(-99999.0),
                Field::Byte(0),
                Field::Real(it.life),
                Field::Real(0.0),
                Field::Real(o.real("HP")),
                w(it.charges.max(0) as u32),
                w(first_ability),
                w(ht),
                w(hb),
                Field::Real(0.0),
                w(15),
                Field::List(abilities),
                timer(),
                w(0),
                w(0),
                timer(),
            ];
            let own = vec![w(if held { 0x2757 } else { 0x3416 })];
            out.push((me.0, agent_first(me, codes::class::ITEM, &[pos]), Some(Second::Agent { own, handlers: game_handlers(game), tail })));
            let (cx, cy) = ((it.x - p.origin[0]) / 32.0, (it.y - p.origin[1]) / 32.0);
            let (col, row) = (cx.max(0.0) as u32, cy.max(0.0) as u32);
            out.push((pos.0, First::Pos { base: self.stamped(property_base(pos, me)), values: [cx, cy, 0.0, 0.0, f32::MAX, 4.712_388_5, 0.03125], refs: [rf(r1), rf(r2)] }, None));
            out.push((r1.0, First::Region { refs: [rf(coarse), rf(pos)], word: if held { 0x0c00_0000 } else { 0 }, bounds: [row >> 3, col >> 3, (row >> 3) + 1, (col >> 3) + 1], words: [0, 0x3000_0001, 0] }, None));
            let unplaced = 0xfffe_0c00;
            out.push((r2.0, First::Region { refs: [rf(fine), rf(pos)], word: if held { 0x18 } else { 0x0100_0018 }, bounds: if held { [unplaced; 4] } else { [row, col, row + 1, col + 1] }, words: [0, u32::from(!held), 0] }, None));
        }
        out
    }

    /// A structure being built's objects (`FUN_6f533490`, `FUN_6f41b380`): its Build In
    /// Progress ability, that's real (the time left), the rise's modifier and threshold
    /// listener on it, and its life's modifier.
    pub(crate) fn build_parts(&self, h: u32) -> Option<[(u32, u32); 5]> {
        let u = self.unit(h)?;
        u.constructing?;
        let codes = [codes::ability::id::BUILD_IN_PROGRESS, codes::class::REAL, codes::class::CONTINUOUS_MODIFIER, codes::class::THRESHOLD_LISTENER, codes::class::CONTINUOUS_MODIFIER];
        if u.extra.build_objects.len() != 5 || !u.extra.build_objects.iter().zip(codes).all(|(s, c)| self.tags.live.get(s).is_some_and(|e| &e.0 == c)) {
            return None;
        }
        let p: Vec<(u32, u32)> = u.extra.build_objects.iter().map(|&s| self.tagged(s)).collect();
        Some([p[0], p[1], p[2], p[3], p[4]])
    }

    /// A structure being built's real and its rise's modifier and listener (read
    /// against Forestwalk's undead computer's Crypt at 2.5 seconds and Graveyard at 160):
    /// the real its build time at its start, falling 1 a second; the modifier -1 for
    /// the build time and a second more (0x60040044, its node due as that ends); the
    /// listener on event 0xd01c4 at 0 (0x20090400), its node checking every 4
    /// seconds from the start, the last the time left.  Provisional: a peasant's
    /// structure without its builder (its rate 0).
    pub fn construction_records(&self, h: u32) -> Vec<(u32, First, Option<Second>)> {
        let mut out = Vec::new();
        let Some(u) = self.unit(h) else { return out };
        let Some([abnp, rel, rise, rtl, _]) = self.build_parts(h) else { return out };
        let bt = u.build_time.max(0.01);
        let made = |r: (u32, u32)| self.made_abs(r.0).unwrap_or(0);
        let count = |r: (u32, u32)| self.tags.prop_counts.get(&r.0).copied().unwrap_or(0);
        let mut base = self.stamped(listed_property_base(&[rel], 0, abnp));
        base.links = [Link(rf(rise), 1), Link(rf(rtl), 1)];
        out.push((rel.0, First::Real { base, values: [bt, -1.0, 0.0, f32::MAX] }, None));
        let span = bt + 1.0;
        let owned = |r: (u32, u32)| [(Link(NONE, 0), Link(NONE, 0), rf(r), rf(abnp)), (Link(NONE, 0), Link(NONE, 0), rf(r), rf(rel))];
        let due = f32::from_bits(crate::efloat::add(made(rise), span.to_bits()));
        out.push((rise.0, First::Continuous { object: ObjectBase { real: 0.0, words: [u32::MAX, 0x6004_0044], links: owned(rise), timings: [Timing(Some((due, span, count(rise), 0, 0))), Timing(None)] }, values: [-1.0, span] }, None));
        // (Its checks every 4 seconds, the last the time left.)
        let start = made(rtl);
        let (mut at, mut period) = (start, 4f32.min(bt));
        let mut due = crate::efloat::add(at, period.to_bits());
        while f32::from_bits(due) <= self.time {
            at = due;
            period = 4f32.min(bt - (f32::from_bits(at) - f32::from_bits(start)));
            // (None left: its last check due, not stepped again.)
            if period <= 0.0 {
                break;
            }
            due = crate::efloat::add(at, period.to_bits());
        }
        out.push((rtl.0, First::Threshold { object: ObjectBase { real: 0.0, words: [0xd_01c4, 0x2009_0400], links: owned(rtl), timings: [Timing(Some((f32::from_bits(due), period, count(rtl), 0, 1))), Timing(None)] }, real: 0.0, word: 0 }, None));
        out
    }

    /// A hero's hero ability (AHer) and its objects: its two flags (its level less one,
    /// its intelligence) and its strength's and intelligence's regeneration modifiers,
    /// those still its (read: Forestwalk's Dreadlord's, after two `rcm freed).
    pub(crate) fn hero_parts(&self, h: u32) -> Option<((u32, u32), [(u32, u32); 2], [Option<(u32, u32)>; 2])> {
        let u = self.unit(h)?;
        let rest = &u.extra.object_slots[2];
        let is = |&(s, b): &(u32, u32), code: &[u8; 4]| self.tags.live.get(&s).is_some_and(|e| &e.0 == code && e.1 == b);
        let k = rest.iter().position(|x| is(x, codes::ability::id::HERO))?;
        let at = |d: usize| rest.get(k + d).copied();
        let flags = [at(1)?, at(2)?];
        if !flags.iter().all(|f| is(f, codes::class::FLAG)) {
            return None;
        }
        let rcm = |d: usize| at(d).filter(|x| is(x, codes::class::CONTINUOUS_MODIFIER));
        Some((rest[k], flags, [rcm(5), rcm(6)]))
    }

    /// A hero ability's two flags' records: its level less one and its base
    /// intelligence (`World::hero_base_attrs`), each stamped as made (read against
    /// Forestwalk's Dreadlord's at level 1, 0 and 18).
    pub fn hero_flag_records(&self, h: u32) -> Vec<(u32, First, Option<Second>)> {
        let Some(u) = self.unit(h) else { return Vec::new() };
        let Some((aher, flags, _)) = self.hero_parts(h) else { return Vec::new() };
        let values = [u.level.max(1) as u32 - 1, self.hero_base_attrs(h)[2].round().max(0.0) as u32];
        (0..2).map(|k| (flags[k].0, First::Flag { base: self.stamped(listed_property_base(&flags, k, aher)), value: values[k] }, None)).collect()
    }

    /// A unit's sight as its last day change began and where it goes (night sight to
    /// day sight at dawn), the same for one with ultravision.
    fn day_change_sight(&self, h: u32) -> (f32, f32) {
        let Some(u) = self.unit(h) else { return (0.0, 0.0) };
        let Some(o) = self.objects.units.get(u.type_id) else { return (0.0, 0.0) };
        let (day, night) = (o.real("sight"), if self.ultravision(u) { o.real("sight") } else { o.real("nsight") });
        if (6.0..18.0).contains(&self.time_of_day) { (night, day) } else { (day, night) }
    }

    /// A unit's continuous modifiers' records (`` `rcm ``, read against the quiet test
    /// map's and Forestwalk's): each two words (-1, its kind), its values, and its
    /// timing; chained on the unit newest first and on its real newest first, the
    /// real's links flagged 1.
    pub fn unit_modifier_records(&self, h: u32) -> Vec<(u32, First, Option<Second>)> {
        let mut out = Vec::new();
        let Some(u) = self.unit(h) else { return out };
        let first = &u.extra.object_slots[0];
        if first.len() < 10 {
            return out;
        }
        let me = first[0];
        let props = [first[1], first[4], first[5], first[6], first[7], first[8]];
        let mods = self.unit_modifiers(h);
        let chain: Vec<(u32, u32)> = mods.iter().map(|m| m.r).collect();
        for m in &mods {
            let (after, before) = chain_links(&chain, m.r, 0);
            let on_real: Vec<(u32, u32)> = mods.iter().filter(|n| n.prop == m.prop).map(|n| n.r).collect();
            let (r_after, r_before) = chain_links(&on_real, m.r, 1);
            let object = ObjectBase {
                real: 0.0,
                words: [u32::MAX, m.word],
                links: [(after, before, rf(m.r), rf(me)), (r_after, r_before, rf(m.r), rf(props[m.prop]))],
                timings: [m.timing, Timing(None)],
            };
            out.push((m.r.0, First::Continuous { object, values: m.values }, None));
        }
        out
    }

    /// A unit's threshold listeners (`` `rtl ``, read against the quiet test map's): its
    /// death's on its life (event 0xd019f, below 0.4); its spells' on its mana, as
    /// made (0xd01b7, below -0.1) or as a creep woke (0xd01b6, rising past each cost
    /// and 0.1, a timing node that checks again in the time the rate takes to reach
    /// it, at most 4 seconds: read, 5.1 at 0.75 a second from 0 checked at 4.0 and then
    /// 2.8 on); each two words (its event, 0x20090400), chained on the unit and on its
    /// real.  (Provisional: a re-armed node's count, the running game's.)
    pub fn unit_listener_records(&self, h: u32) -> Vec<(u32, First, Option<Second>)> {
        let mut out = Vec::new();
        let Some(u) = self.unit(h) else { return out };
        let first = &u.extra.object_slots[0];
        if first.len() < 10 {
            return out;
        }
        let me = first[0];
        let ls = self.unit_thresholds(h);
        // (The unit's chain holds its in-range listeners too, its abilities' and its
        // AI's, by their makings.)
        let on_unit = self.unit_listener_chain(h);
        for &(me_s, prop, event, thr, rising, node) in &ls {
            let on_prop: Vec<(u32, u32)> = ls.iter().filter(|l| l.1 == prop).map(|l| l.0).collect();
            let (a0, b0) = chain_links(&on_unit, me_s, 0);
            let (a1, b1) = chain_links(&on_prop, me_s, 1);
            let object = ObjectBase {
                real: 0.0,
                words: [event, 0x2009_0400],
                links: [(a0, b0, rf(me_s), rf(me)), (a1, b1, rf(me_s), rf(prop))],
                timings: [node, Timing(None)],
            };
            out.push((me_s.0, First::Threshold { object, real: thr, word: rising }, None));
        }
        out
    }

    /// A unit's threshold listeners (`unit_listener_records`), by birth: each itself,
    /// its real, event, threshold, rising, and its node.
    fn unit_thresholds(&self, h: u32) -> Vec<((u32, u32), (u32, u32), u32, f32, u32, Timing)> {
        let Some(u) = self.unit(h) else { return Vec::new() };
        let first = &u.extra.object_slots[0];
        if first.len() < 10 {
            return Vec::new();
        }
        let (life, mana) = (first[4], first[5]);
        let is_rtl = |s: u32| self.tags.live.get(&s).is_some_and(|e| &e.0 == codes::class::THRESHOLD_LISTENER);
        // (Not a kept slot another object has taken since.)
        let kept = |&&(s, b): &&(u32, u32)| self.tagged(s).1 == b;
        let death = u.extra.object_slots[1].iter().filter(kept).copied().find(|&(s, _)| is_rtl(s));
        let (_, mp_rate) = self.regen_rates(h).unwrap_or((0.0, 0.0));
        // (Each: itself, its real, event, threshold, rising, and its node.)
        let mut ls: Vec<((u32, u32), (u32, u32), u32, f32, u32, Timing)> = Vec::new();
        if let Some(d) = death {
            ls.push((d, life, 0xd_019f, 0.4, 0, Timing(None)));
        }
        let woken: Vec<(f32, u32)> = u.extra.mana_listeners.iter().copied().filter(|&(_, s)| is_rtl(s)).collect();
        // (A hero's learned skill's waits for its mana to fall below its cost, less
        // 0.1 in the engine's arithmetic: read, a Dreadlord's Sleep at 270 mana,
        // 99.899994.  Provisional: one learned with less mana than its cost.)
        let learned = |s: u32| -> Option<f32> {
            let code = u.extra.spell_owners.iter().find(|o| o.0 == s)?.1;
            let (a, l) = u.abilities.iter().copied().find(|(a, _)| self.objects.abilities.get(*a).is_some_and(|o| crate::objects::id_from_str(o.str("code").trim()) == code))?;
            let ab = self.objects.abilities.get(a).filter(|o| o.int("hero") != 0)?;
            let cost = ab.level_real("Cost", l.max(1) as u32);
            (u.mana >= cost).then(|| f32::from_bits(crate::efloat::sub(cost.to_bits(), 0.1f32.to_bits())))
        };
        for &s in u.extra.spell_timers.iter().filter(|&&s| is_rtl(s) && !woken.iter().any(|w| w.1 == s)) {
            ls.push((self.tagged(s), mana, 0xd_01b7, learned(s).unwrap_or(-0.1), 0, Timing(None)));
        }
        // (A held item's ability's, below its cost less 0.1: read, a Town Portal
        // scroll's -0.1.)
        let held = self.held_item_objects(h);
        for (k, x) in held.iter().enumerate().filter(|(_, x)| &x.1 == codes::class::THRESHOLD_LISTENER) {
            let cost = held[..k].iter().rev().find(|y| y.2 == x.2 && y.1[0] == b'A').and_then(|y| self.objects.abilities.get(y.1)).map_or(0.0, |a| a.level_real("Cost", 1));
            ls.push((x.0, mana, 0xd_01b7, f32::from_bits(crate::efloat::sub(cost.to_bits(), 0.1f32.to_bits())), 0, Timing(None)));
        }
        for &(thr, s) in &woken {
            let me_s = self.tagged(s);
            let made = self.made_abs(s).map_or(0.0, f32::from_bits);
            // Its checks from its making: the mana from 0 at its rate (the creeps' at the
            // start).
            // (Due at most 4 seconds on, or as the mana reaches it, that time reckoned
            // from 0 in single precision: read, 5.1 at 0.75 due 6.7999997.)
            let reach = made + thr / mp_rate.max(f32::MIN_POSITIVE);
            let (mut at, mut due) = (made, (made + 4.0).min(reach));
            if mp_rate > 0.0 && thr.is_finite() {
                while due <= self.time {
                    at = due;
                    due = (at + 4.0).min(reach);
                }
            }
            let count = self.tags.prop_counts.get(&s).copied().unwrap_or(0);
            if thr.is_finite() {
                // (Its period the time left at its last check, from the mana then.)
                let period = ((thr - mp_rate * (at - made)) / mp_rate.max(f32::MIN_POSITIVE)).min(4.0);
                ls.push((me_s, mana, 0xd_01b6, thr, 1, Timing(Some((due, period, count, 0, 1)))));
            } else {
                // One past its cost waits for the mana to fall (provisional: below -0.1,
                // read only of spells costing nothing).
                ls.push((me_s, mana, 0xd_01b7, -0.1, 0, Timing(None)));
            }
        }
        ls.sort_by_key(|l| l.0 .1);
        ls
    }

    /// A unit's own first record (`+w3u`'s agent, read against the quiet test map's): its
    /// properties (its position and five reals), the last its second link; its first
    /// link its AI's unit list (`uref`), its third its newest modifier, its fourth its
    /// newest listener.
    pub fn unit_agent_record(&self, h: u32) -> Vec<(u32, First, Option<Second>)> {
        let Some(u) = self.unit(h) else { return Vec::new() };
        let first = &u.extra.object_slots[0];
        if first.len() < 10 {
            return Vec::new();
        }
        let me = first[0];
        let props: Vec<(u32, u32)> = vec![first[1], first[4], first[5], first[6], first[7], first[8]];
        let mut rec = agent_first(me, codes::class::UNIT, &props);
        if let First::Agent { links, .. } = &mut rec {
            // (Its abilities' chain, newest first: its abilities and an AI's unit list,
            // not the AI itself.)
            // (Buffs among them, a buff being an ability: read, the aura scans' at 2
            // seconds.)
            // (And what else its chain holds: a structure's Build In Progress, its held
            // items' abilities, its order's tasks.)
            if let Some((t, _)) = self.ability_chain(h, &[]).into_iter().max_by_key(|(t, _)| t.1) {
                links[0] = Link(rf(t), 0);
            }
            // (Each chain's head its newest, as its records chain them.)
            links[2] = self.unit_modifiers(h).last().map_or(Link(NONE, 0), |m| Link(rf(m.r), 0));
            links[3] = self.unit_listener_chain(h).last().map_or(Link(NONE, 0), |&s| Link(rf(s), 0));
        }
        vec![(me.0, rec, None)]
    }

    /// A unit's clustered position (`` `pcu ``, read against the quiet test map's): its
    /// place in pathing cells, two zeros, its speed in cells a second (none moving
    /// unbounded), facing in radians, and collision in cells; its two regions; two
    /// words of -1 and one of 0; 0.8, its speed again (a structure's 0.75), its turn
    /// rate (a structure's 0.001), pi / 3, two zeros, its facing, and 0; its cells; a
    /// flag word (its type's look and formation, below); and its path.  Its property's
    /// second head is its newest in-range listener.
    pub fn unit_position_records(&self, h: u32) -> Vec<(u32, First, Option<Second>)> {
        let mut out = Vec::new();
        let Some(u) = self.unit(h) else { return out };
        let Some(o) = self.objects.units.get(u.type_id) else { return out };
        let Some(p) = self.pathing.as_ref() else { return out };
        let first = &u.extra.object_slots[0];
        if first.len() < 10 {
            return out;
        }
        let me = first[0];
        let props: Vec<(u32, u32)> = vec![first[1], first[4], first[5], first[6], first[7], first[8]];
        let mut base = self.stamped(listed_property_base(&props, 0, me));
        let newest = self.unit_pils(h).into_iter().map(|p| self.tagged(p.0)).max_by_key(|x| x.1).map(|x| x.0);
        if let Some(s) = newest {
            base.links[1] = Link(rf(self.tagged(s)), 1);
        }
        let cx = (u.x - p.origin[0]) / 32.0;
        let cy = (u.y - p.origin[1]) / 32.0;
        // (Degrees times pi / 180 in the engine's arithmetic, a negative angle then two
        // pi on (read: every facing of the quiet test map's script's creeps, from its
        // literals, 57 of 57), and its pi / 3 as 1.0471975.)
        let facing = match u.extra.made_facing {
            Some((deg, rad)) if deg == u.facing => rad,
            _ => f32::from_bits(crate::efloat::mul(u.facing.to_bits(), 0x3c8e_fa35)),
        };
        // (Its speed as set, SetUnitMoveSpeed's: read, Mafarazzo TD's creeps'; one whose
        // type moves not moving all the same.)
        let moving = o.real("spd") > 0.0;
        let speed = if moving { u.move_speed } else { 0.0 };
        let cells_speed = if moving { speed / 32.0 } else { f32::MAX };
        let turn = if moving { o.real("turnRate") } else { 0.001 };
        // (A collision of none as one: read, Mafarazzo TD's units, 0.03125; provisional.)
        let mut values = [cx, cy, 0.0, 0.0, cells_speed, facing, o.real("collision").max(1.0) / 32.0];
        let mut reals = [0.8, if moving { speed / 32.0 } else { 0.75 }, turn, 1.047_197_5, 0.0, 0.0, facing, 0.0];
        let mut words = [u32::MAX, u32::MAX, 0];
        // Its flag word (+0xd8): its type's orientInterp in bits 8 to 11 and formation in
        // 12 to 15 (`FUN_6f653800` from the type's data: CUnit vf50 `FUN_6f698c80`, then
        // vf51 `FUN_6f698750`; read, every unit of the quiet test map's at 5 seconds and
        // of Booty Bay's network save).  Provisional: bit 16 (`FUN_6f171060`) and bit 24
        // (an attack's search, CAbilityAttack `FUN_6f49bc40` by `FUN_6f05c350`), not set.
        let look = (o.int("orientInterp") as u32 & 0xff) << 8;
        let flags_word = (o.int("formation") as u32 & 0xff) << 12 | look & 0xffff_0fff;
        let more = [cx as u32, cy as u32, flags_word];
        let mut flags = [0, 0];
        // A worker on a gold run: its speed by its run's factor and its first real
        // 2.0625 (6.0 on its way home); walking, its walk's cluster, its velocity,
        // and its heading (read: Forestwalk's peasants at 12 seconds; provisional, the
        // first real, the turn's last two).
        // (On a lumber run, its first real 4.125: read, Forestwalk's peasants
        // chopping and walking to their trees; walking home, 6.0 as with gold.)
        let lumber = u.extra.gold_run.as_ref().is_some_and(|r| r.lumber);
        if lumber {
            let home = u.extra.gold_run.as_ref().is_some_and(|r| matches!(r.phase, crate::goldrun::Phase::ToHall(_)));
            reals[0] = if home { 6.0 } else { 4.125 };
        }
        if let Some(r) = u.extra.gold_run.as_ref().filter(|r| !r.lumber) {
            let top = self.walk_speed_cells(h);
            values[4] = top;
            reals[1] = top;
            reals[0] = if matches!(r.phase, crate::goldrun::Phase::ToHall(_)) { 6.0 } else { 2.0625 };
        }
        // Walking (any walk: read too, peasants sent to a point), its walk's cluster,
        // velocity, and heading.
        {
            if let (Some((c, _, _)), Some(m)) = (self.saved_order(h).and_then(|o| o.walk), u.extra.mover.as_ref()) {
                let f = f32::from_bits;
                // (Stamped as it last moved: the main clock of the last step's tick; read,
                // Mafarazzo TD's LAN save at 53.8 seconds, its 180 walkers' 53.753674, the
                // main clock then 53.783657.)
                base.real = if self.last_step_clock != 0 { f32::from_bits(self.last_step_clock) } else { self.main_clock_now() };
                values[0] = f(m.x);
                values[1] = f(m.y);
                values[2] = f(m.vx);
                values[3] = f(m.vy);
                values[5] = f(m.heading);
                words = [c.0, c.1, 0];
                reals[4] = f(m.vx).hypot(f(m.vy));
                reals[6] = f(m.heading);
                // (A lumber walker's 0, 0: read.)
                flags = if lumber { [0, 0] } else { [1, 0] };
                // (One whose pathing a script turned off: 0.49 and 0, 0; read, Mafarazzo
                // TD's creeps; provisional.)
                if u.extra.no_pathing {
                    reals[0] = 0.49;
                    flags = [0, 0];
                }
            }
        }
        let first_rec = First::Cluster {
            base,
            values,
            refs: [rf(first[2]), rf(first[3])],
            words,
            reals,
            more,
            flags,
            last: rf(first[9]),
            separate: None,
        };
        out.push((first[1].0, first_rec, None));
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
            if self.tags.live.get(slot).is_none_or(|e| &e.0 != codes::class::HANDLER) {
                continue;
            }
            let own = self.tagged(*slot);
            let k = self.tags.function_numbers.get(func).copied().unwrap_or(0);
            let handlers = if timers.contains(slot) { Handlers { keyed: Vec::new(), fixed: Default::default(), events: vec![Vec::new(); 0x400] } } else { no_handlers() };
            out.push((own.0, agent_first(own, codes::class::HANDLER, &[]), Some(Second::Agent { own: vec![Field::Word(0)], handlers, tail: vec![Field::Word(k)] })));
        }
        for id in self.handles.ids() {
            match self.handles.get(id) {
                Some(Obj::Trigger(t)) if t.slots.len() == 3 => {
                    let own = self.tagged(t.slots[0]);
                    let flags = [self.tagged(t.slots[1]), self.tagged(t.slots[2])];
                    // (Registrations of timers' expiry, dialogs, their buttons, and
                    // trackables are not on their triggers' lists: their natives make them
                    // without `FUN_6f296670`.)
                    // (A timer event's, TriggerRegisterTimerEvent's `tmvt`, is: read, the
                    // original's save of a trigger waiting.)
                    let timer_event = |h: &u32| self.tags.handle_ids.get(h).and_then(|(s, _)| self.tags.live.get(s)).is_some_and(|e| &e.0 == codes::class::TIMER_EVENT);
                    let unlisted = |h: &u32| !timer_event(h) && matches!(self.handles.get(*h), Some(Obj::Event(r)) if matches!(r.kind, EventKind::TimerExpire(_) | EventKind::Dialog(_) | EventKind::DialogButton(_) | EventKind::Trackable { .. }));
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
                    // Its words: its evaluations and executions counted, its executions
                    // still running, a thread of each asleep (read, the original's save of
                    // a trigger waiting, 1), its conditions' last evaluation (+0x74, none
                    // by TriggerExecute: read, Mafarazzo TD's triggers run by another's
                    // TriggerExecute 0, those run by ConditionalTriggerExecute 1), and
                    // whether it waits on sleeps.
                    let running = {
                        let mut ex: Vec<u32> = self.sleepers.iter().filter(|s| s.execution != 0 && s.ctx.trigger == id).map(|s| s.execution).collect();
                        ex.sort();
                        ex.dedup();
                        ex.len() as u32
                    };
                    let words = [0, t.eval_count as u32, t.exec_count as u32, running, u32::from(t.last_eval), u32::from(t.wait_on_sleeps)];
                    // (Its conditions, `tcnd`, by their handles' slots.)
                    let conditions = Field::List(t.conditions.iter().filter_map(|c| self.tags.handle_slots.get(c)).map(|&s| vec![Field::Ref(rf(self.tagged(s)))]).collect());
                    let second = Second::Agent { own: words.iter().map(|&w| Field::Word(w)).collect(), handlers: no_handlers(), tail: vec![listed(&t.events), conditions, actions] };
                    out.push((own.0, agent_first(own, codes::class::TRIGGER, &flags), Some(second)));
                    // (Provisional: which of the two is the enabled flag, every trigger read
                    // being enabled.)
                    for (k, v) in [2, u32::from(t.enabled)].into_iter().enumerate() {
                        out.push((flags[k].0, First::Flag { base: self.stamped(listed_property_base(&flags, k, own)), value: v }, None));
                    }
                }
                Some(Obj::TriggerAction { func, slot }) if *slot != 0 => {
                    let own = self.tagged(*slot);
                    let handler = self.tags.handler_objects.iter().find(|(_, f)| *f == func).map_or(NONE, |(&s, _)| rf(self.tagged(s)));
                    out.push((own.0, agent_first(own, codes::class::TRIGGER_ACTION, &[]), Some(Second::Agent { own: vec![Field::Word(0)], handlers: no_handlers(), tail: vec![Field::Ref(handler)] })));
                }
                _ => {}
            }
        }
        out
    }

    /// The players' upgrades researched (CPlayerUpgrade vf14, `+Wup`; read against the
    /// hero map's undead computer's at 900 seconds): no handlers; its player, the
    /// upgrade, 0, its level, and 3, then its four effects' places (each an object, or
    /// none); each effect's object (CUpgradeArmor and the rest) 0, its base and its
    /// amount a level as the upgrade's data gives them, and 0.  Provisional: the third
    /// and fifth words (3 in every one read, of one, two, or three levels).
    pub fn upgrade_records(&self) -> Vec<(u32, First, Option<Second>)> {
        let mut out = Vec::new();
        let quiet = || Handlers { keyed: Vec::new(), fixed: Default::default(), events: vec![Vec::new(); 0x400] };
        for p in &self.players {
            for (id, slot, effects) in &p.upgrade_objects {
                if self.tags.live.get(slot).is_none_or(|e| &e.0 != codes::class::PLAYER_UPGRADE) {
                    continue;
                }
                let me = self.tagged(*slot);
                let Some(o) = self.objects.upgrades.get(*id) else { continue };
                let level = p.tech_researched.get(id).copied().unwrap_or(0).max(0) as u32;
                let places = Field::List((0..4).map(|k| match effects.get(k) {
                    Some(&e) => {
                        let t = self.tagged(e);
                        vec![Field::Word(t.0), Field::Word(t.1), Field::Word(0)]
                    }
                    None => vec![Field::Word(u32::MAX), Field::Word(u32::MAX), Field::Word(0)],
                }).collect());
                let tail = vec![Field::Word(p.id as u32), Field::Word(u32::from_be_bytes(*id)), Field::Word(0), Field::Word(level), Field::Word(3), places];
                out.push((me.0, agent_first(me, codes::class::PLAYER_UPGRADE, &[]), Some(Second::Agent { own: vec![Field::Word(0)], handlers: quiet(), tail })));
                // Its effects', in its data's order, those named.
                let named = (1..=4).filter(|k| crate::objects::id_from_str(o.str(&format!("effect{k}")).trim())[0].is_ascii_lowercase());
                for (k, &e) in named.zip(effects) {
                    let Some(code) = self.tags.live.get(&e).map(|x| x.0) else { continue };
                    let t = self.tagged(e);
                    let own = vec![Field::Word(0), Field::Real(o.real(&format!("base{k}"))), Field::Real(o.real(&format!("mod{k}"))), Field::Word(0)];
                    out.push((t.0, agent_first(t, &code, &[]), Some(Second::Agent { own, handlers: quiet(), tail: Vec::new() })));
                }
            }
        }
        out
    }

    /// The trigger executions alive (`+tex`, CTriggerExecution vf14; read against the
    /// original's save of a trigger waiting 30 seconds): its context's fifteen words
    /// (slot 0 the triggering unit, 1 its widget, 2 the trigger, 3 the player, 6 the
    /// event's id; the event's own objects, 4 and 5, none); its timer to its thread's
    /// waking (kind 4, its sleep, once), the sleep again in chunks; the thread's place;
    /// the action it runs, and the actions listed; 0.0, none, 0, 0; its trigger on event
    /// 0x80274; none; and its count of resumptions plus one (+0x90, 1 until a network's
    /// word resumed it: `world::Execution`).  One whose thread does not sleep (within
    /// SaveGame, say) has no timer, nor one TriggerSyncStart stopped.  Provisional: the
    /// actions listed (the one).
    pub fn execution_records(&self) -> Vec<(u32, First, Option<Second>)> {
        let mut out = Vec::new();
        let live: Vec<u32> = self.tags.live.iter().filter(|(_, (c, _))| c == codes::class::TRIGGER_EXECUTION).map(|(&s, _)| s).collect();
        for slot in live {
            let me = self.tagged(slot);
            let sleeper = self.sleepers.iter().find(|s| s.execution == slot);
            let jass = |id: u32| if id == 0 { 0 } else { self.handles.known_handle(id).unwrap_or(id) };
            let mut words = [0u32; 15];
            let mut tail_timer = (Field::Timer(NONE, 0, Timing(None)), 0u32, 0.0f32);
            let (mut place, mut action, mut trigger, mut asleep) = (0, None, None, false);
            if let Some(s) = sleeper {
                let player = self.players.get(s.ctx.player as usize).map_or(0, |p| p.handle);
                words[0] = jass(s.ctx.unit);
                words[1] = jass(s.ctx.widget);
                words[2] = jass(s.ctx.trigger);
                words[3] = if s.ctx.has_player { jass(player) } else { 0 };
                words[6] = s.ctx.event_id as u32;
                if s.wake_at.is_finite() {
                    let left = (s.wake_at - crate::world::SLEEP_SLACK - self.sleep_time).max(0.0);
                    let (chunks, rest) = timer_chunks(s.sleep);
                    tail_timer = (Field::Timer(rf(me), 4, Timing(Some((self.time + left, s.sleep, s.timer_count, 0, 0)))), chunks, rest);
                }
                place = s.thread.place;
                if let Some(Obj::Trigger(t)) = self.handles.get(s.ctx.trigger) {
                    // (The action its thread runs: the one whose function is its run's.)
                    let top = s.thread.frames.iter().find(|f| f.top).map(|f| f.func);
                    let mine = t.actions.iter().find(|&&a| matches!(self.handles.get(a), Some(Obj::TriggerAction { func, .. }) if Some(*func) == top)).or(t.actions.first());
                    action = mine.and_then(|&a| match self.handles.get(a) {
                        Some(Obj::TriggerAction { slot, .. }) if *slot != 0 => Some(self.tagged(*slot)),
                        _ => None,
                    });
                    asleep = s.wake_at.is_finite();
                    trigger = t.slots.first().map(|&s| self.tagged(s));
                }
            }
            let (timer, chunks, rest) = tail_timer;
            let tail = vec![
                Field::List(words.iter().map(|&w| vec![Field::Word(w)]).collect()),
                timer,
                Field::Short(chunks.min(0xffff) as u16),
                Field::Short(chunks.min(0xffff) as u16),
                Field::Real(rest),
                Field::Word(place),
                // (Its action under way while it sleeps, none kept within SaveGame: read,
                // Mafarazzo TD's; its actions listed either way.)
                Field::Ref(action.filter(|_| asleep).map_or(NONE, rf)),
                Field::List(action.into_iter().map(|a| vec![Field::Ref(rf(a))]).collect()),
                Field::Real(0.0),
                Field::Ref(NONE),
                Field::Word(0),
                Field::Word(0),
                Field::List(trigger.into_iter().map(|t| vec![Field::Ref(rf(t)), Field::Word(0x8_0274)]).collect()),
                Field::List(Vec::new()),
                Field::Word(self.executions.get(&slot).map_or(1, |e| e.count)),
            ];
            let handlers = Handlers { keyed: Vec::new(), fixed: Default::default(), events: vec![Vec::new(); 0x400] };
            out.push((me.0, agent_first(me, codes::class::TRIGGER_EXECUTION, &[]), Some(Second::Agent { own: vec![Field::Word(0)], handlers, tail })));
        }
        out
    }

    /// A property's base with its time stamp: the main clock at its making.
    fn stamped(&self, mut base: PropertyBase) -> PropertyBase {
        let slot = base.object.links[0].2 .0;
        base.real = self.tags.made_at.get(&slot).map_or(0.0, |&c| f32::from_bits(c));
        base
    }

    /// The game's object (`+gam`).
    pub(crate) fn game_object(&self) -> (u32, u32) {
        self.tags.live.iter().find(|(_, (c, _))| c == codes::class::GAME_STATE).map_or((0, 0), |(&s, (_, b))| (s, *b))
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
                            (codes::class::UNIT_EVENT, vec![0], vec![Field::Word(e), Field::Word(u.0), Field::Word(u.1), Field::Word(0), Field::Ref(NONE), handler_list(vec![(trig, e)])])
                        }
                        EventKind::Player { player: p, event, alliance } if *event == crate::event_ids::EVENT_PLAYER_ALLIANCE_CHANGED => {
                            let pl = player(*p);
                            let listener = self.tags.event_timers.get(&id).map_or(NONE, |&s| rf(self.tagged(s)));
                            let mut ws = words(&base);
                            ws.extend([Field::Word(0), Field::Word(0x8_0106), Field::Real(0.0), Field::Word(0), Field::Word(pl.0), Field::Word(pl.1), Field::Word(*alliance as u32)]);
                            let tail = vec![Field::List(Vec::new()), Field::Ref(listener), handler_list(vec![(own, 0x8_0106)]), handler_list(vec![(trig, 0x8_020c)])];
                            out.push((own.0, agent_first(own, codes::class::ALLIANCE_EVENT, &[]), Some(Second::Agent { own: ws, handlers: game_handlers(self.game_object()), tail })));
                            continue;
                        }
                        EventKind::Player { player: p, event, .. } | EventKind::PlayerUnit { player: p, event, filter: None } => {
                            let e = *event as u32 + 0x8_0200;
                            let pl = player(*p);
                            (codes::class::PLAYER_EVENT, vec![0, e, pl.0, pl.1, 0, 0], vec![Field::Ref(NONE), handler_list(vec![(trig, e)])])
                        }
                        EventKind::PlayerUnit { player: p, event, filter: Some(f) } => {
                            let e = *event as u32 + 0x8_0200;
                            let pl = player(*p);
                            let f = slot_of(*f).map_or(NONE, rf);
                            (codes::class::PLAYER_EVENT, vec![0, e, pl.0, pl.1, 0, 0], vec![Field::Ref(f), handler_list(vec![(trig, e)])])
                        }
                        EventKind::PlayerState { player: p, state, op, value } => {
                            let pl = player(*p);
                            let listener = self.tags.event_timers.get(&id).map_or(NONE, |&s| rf(self.tagged(s)));
                            let op = *op as u32 + 0x8_0101;
                            let mut ws = words(&base);
                            ws.extend(words(&[0, op, pl.0, pl.1, 0, *state as u32, op]));
                            ws.push(Field::Real(*value));
                            let tail = vec![Field::Ref(NONE), Field::List(Vec::new()), Field::Ref(listener), handler_list(vec![(trig, 0x8_020b)])];
                            out.push((own.0, agent_first(own, codes::class::PLAYER_STATE_EVENT, &[]), Some(Second::Agent { own: ws, handlers: game_handlers(self.game_object()), tail })));
                            continue;
                        }
                        EventKind::Game(event) => {
                            let e = *event as u32 + 0x8_0200;
                            (codes::class::GAME_EVENT, vec![e], vec![handler_list(vec![(trig, e)])])
                        }
                        // A timer event's (CTimerEventReg, TriggerRegisterTimerEvent's
                        // `tmvt`): the registration's words, then its timer, the
                        // trigger's, to its due (event 0x80204, once unless periodic),
                        // stopped once it has fired, its timeout in chunks (read: the
                        // original's at 40 seconds, due 50, and at 60).
                        EventKind::TimerExpire(timer) if self.tags.live.get(&own.0).is_some_and(|e| &e.0 == codes::class::TIMER_EVENT) => {
                            let Some(Obj::Timer(tm)) = self.handles.get(*timer) else { continue };
                            let field = if tm.running {
                                Field::Timer(rf(trig), 0, Timing(Some(((self.time as f64 + tm.remaining) as f32, tm.period as f32, tm.agent_seq, 0x8_0204, u32::from(tm.periodic)))))
                            } else {
                                Field::Timer(NONE, 0, Timing(None))
                            };
                            let (chunks, rest) = timer_chunks(tm.timeout);
                            let mut ws = words(&base);
                            ws.push(Field::Word(0));
                            let tail = vec![field, Field::Short(chunks.min(0xffff) as u16), Field::Short(chunks.min(0xffff) as u16), Field::Real(rest)];
                            out.push((own.0, agent_first(own, codes::class::TIMER_EVENT, &[]), Some(Second::Agent { own: ws, handlers: game_handlers(self.game_object()), tail })));
                            continue;
                        }
                        EventKind::TimerExpire(timer) => {
                            let tm = slot_of(*timer).map_or(NONE, rf);
                            (codes::class::TIMER_EXPIRED_EVENT, vec![0], vec![Field::Ref(tm), handler_list(vec![(trig, 0x8_0204)])])
                        }
                        // A dialog button's (read: the melee defeat dialog's): its button.
                        EventKind::DialogButton(button) => {
                            let b = self.tags.handle_ids.get(button).map_or(NONE, |&r| rf(r));
                            (codes::class::DIALOG_BUTTON_EVENT, vec![0], vec![Field::Ref(b), handler_list(vec![(trig, 0x8_025a)])])
                        }
                        EventKind::GameState { state: 2, op, value } => {
                            let listener = self.tags.event_timers.get(&id).map_or(NONE, |&s| rf(self.tagged(s)));
                            let first = *op != 2 || hours > *value;
                            let (l1, l2) = if first { (listener, NONE) } else { (NONE, listener) };
                            let mut ws = words(&base);
                            ws.extend([Field::Word(0x8_0106), Field::Word(*op as u32 + 0x8_0101), Field::Real(*value), Field::Word(2)]);
                            let tail = vec![Field::List(Vec::new()), Field::Ref(l1), Field::Ref(l2), handler_list(vec![(own, 0x8_0106)]), handler_list(vec![(trig, 0x8_0203)])];
                            out.push((own.0, agent_first(own, codes::class::GAME_STATE_EVENT, &[]), Some(Second::Agent { own: ws, handlers: game_handlers(self.game_object()), tail })));
                            continue;
                        }
                        // A region's enter or leave registration and a chat registration
                        // (`script.rs`).
                        EventKind::EnterRegion { region, filter } | EventKind::LeaveRegion { region, filter } => {
                            let enter = matches!(r.kind, EventKind::EnterRegion { .. });
                            out.extend(self.region_event_record(own, *region, *filter, trig, enter));
                            continue;
                        }
                        EventKind::PlayerChat { player: p, text, exact } => {
                            out.push(self.chat_event_record(own, player(*p), text, *exact, trig));
                            continue;
                        }
                        _ => continue,
                    };
                    let mut ws = words(&base);
                    ws.extend(words(&own_words));
                    out.push((own.0, agent_first(own, code, &[]), Some(Second::Agent { own: ws, handlers: game_handlers(self.game_object()), tail })));
                }
                // A dialog (read: the melee defeat dialogs at 900 seconds): a word, its
                // buttons, and three words.
                Some(Obj::Dialog(d)) => {
                    let Some(&own) = self.tags.handle_ids.get(&id) else { continue };
                    let buttons: Vec<Vec<Field>> = d.buttons.iter().filter_map(|b| self.tags.handle_ids.get(b)).map(|&r| vec![Field::Ref(rf(r))]).collect();
                    out.push((own.0, agent_first(own, codes::class::DIALOG, &[]), Some(Second::Agent { own: vec![Field::Word(0)], handlers: game_handlers(self.game_object()), tail: vec![Field::List(buttons), Field::Word(0), Field::Word(0), Field::Word(0)] })));
                }
                // A dialog button: its text, its hotkey, the registrations of its clicks
                // (event 0x8025b), and whether it quits and to the score screen.
                Some(Obj::DialogButton(b)) => {
                    let Some(&own) = self.tags.handle_ids.get(&id) else { continue };
                    let text = b.text.clone();
                    let regs: Vec<((u32, u32), u32)> = self.handles.ids().into_iter().filter_map(|e| match self.handles.get(e) {
                        Some(Obj::Event(r)) if matches!(r.kind, EventKind::DialogButton(x) if x == id) => self.tags.handle_ids.get(&e).map(|&s| (s, 0x8_025b)),
                        _ => None,
                    }).collect();
                    let (quit, score) = b.quit.map_or((0, 0), |s| (1, u32::from(s)));
                    let tail = vec![Field::Text(text), Field::Word(b.hotkey as u32), handler_list(regs), Field::Word(quit), Field::Word(score)];
                    out.push((own.0, agent_first(own, codes::class::DIALOG_BUTTON, &[]), Some(Second::Agent { own: vec![Field::Word(0)], handlers: game_handlers(self.game_object()), tail })));
                }
                Some(Obj::BoolExpr(BoolExpr::Func(f))) => {
                    let Some(own) = slot_of(id) else { continue };
                    let handler = self.tags.handler_objects.iter().find(|(_, g)| *g == f).map_or(NONE, |(&s, _)| rf(self.tagged(s)));
                    out.push((own.0, agent_first(own, codes::class::FILTER, &[]), Some(Second::Agent { own: vec![Field::Word(0)], handlers: game_handlers(self.game_object()), tail: vec![Field::Ref(handler)] })));
                }
                Some(Obj::Force(f)) if f.slots.len() == 3 => {
                    let own = self.tagged(f.slots[0]);
                    let flags = [self.tagged(f.slots[1]), self.tagged(f.slots[2])];
                    for (k, v) in [f.players, f.enumerated].into_iter().enumerate() {
                        out.push((flags[k].0, First::Flag { base: self.stamped(listed_property_base(&flags, k, own)), value: v }, None));
                    }
                    // (Its last ForForce's function's handler, its last enumeration's
                    // filter's, provisional, and a player, none read.)
                    let handler = |func: Option<u32>| func.and_then(|func| self.tags.handler_objects.iter().find(|(_, g)| **g == func)).map_or(NONE, |(&s, _)| rf(self.tagged(s)));
                    let tail = vec![Field::Ref(handler(f.callback)), Field::Ref(handler(f.filter)), Field::Ref(NONE)];
                    out.push((own.0, agent_first(own, codes::class::FORCE, &flags), Some(Second::Agent { own: vec![Field::Word(0)], handlers: game_handlers(self.game_object()), tail })));
                }
                // A timer (CTimerWar3, read against the quiet test map's): its own word,
                // timeout, and a real; its agent timer (CAgentTimerExtended,
                // `FUN_6f0602a0`): while running itself, its flags, and its block (its
                // node's next due, on its clock in the clock's current wrap, its period,
                // its start's count, event 0x80204, a periodic node); the count of
                // 120-second chunks a run of 120 or more goes in (`timer_chunks`: such
                // a run is a periodic node of 120, flag 0x10, then, as the last chunk
                // passes, a node for the rest, the flag dropped, vf3 `FUN_6f060d40`)
                // and the chunks left (none once expired: read, the hero map's save
                // timers of 300, 450, and 900 seconds, 2, 3, and 7 and none); the run
                // beyond the chunks; its function's handler, and the registrations of
                // its expiry.  A periodic one's node keeps its count (the hero map's
                // 30-second timer's at 450 and 900 seconds, due 150.00009 and
                // 300.0001).  (Provisional: a periodic one's flag 2; the rest's node's
                // count, its start's.)
                Some(Obj::Timer(t)) => {
                    let Some(own) = slot_of(id) else { continue };
                    let run = if t.period > 0.0 { t.period as f32 } else { t.timeout };
                    let (chunks, rest) = timer_chunks(run);
                    let periodic_flag = if t.periodic { 2 } else { 0 };
                    let (agent, left) = match t.running.then(|| self.timer_node(t, chunks, rest, run)).flatten() {
                        Some((due, period, left)) => {
                            let flags = if left > 0 { 0x10 } else { 0 } | periodic_flag;
                            let node_periodic = u32::from(left > 0 || (t.periodic && chunks == 0));
                            (Field::Timer(rf(own), flags, Timing(Some((due, period, t.agent_seq, 0x8_0204, node_periodic)))), left)
                        }
                        None => (Field::Timer(NONE, 0, Timing(None)), 0),
                    };
                    let periodic = chunks.min(0xffff);
                    let left = left.min(0xffff);
                    let handler = t.handler.map_or(NONE, |s| rf(self.tagged(s)));
                    let expiries = self
                        .events
                        .iter()
                        .filter(|&&e| matches!(self.handles.get(e), Some(Obj::Event(r)) if matches!(r.kind, EventKind::TimerExpire(x) if x == id)))
                        .filter_map(|e| self.tags.handle_ids.get(e))
                        .map(|&r| ((r.0, r.1), 0x8_0204))
                        .collect();
                    let tail = vec![agent, Field::Short(periodic as u16), Field::Short(left as u16), Field::Real(rest), Field::Ref(handler), handler_list(expiries)];
                    out.push((own.0, agent_first(own, codes::class::TIMER, &[]), Some(Second::Agent { own: vec![Field::Word(0), Field::Real(t.timeout), Field::Real(0.0)], handlers: game_handlers(self.game_object()), tail })));
                }
                Some(Obj::Location(x, y)) => {
                    let Some(own) = slot_of(id) else { continue };
                    out.push((own.0, agent_first(own, codes::class::LOCATION, &[]), Some(Second::Agent { own: vec![Field::Word(0)], handlers: game_handlers(self.game_object()), tail: vec![Field::Real(*x), Field::Real(*y)] })));
                }
                // A sound (CSoundScript?): its timer (none), 0, 0, 0, 0, and its labels: one
                // made from a label its entry (the label, none, 0, its fades in and out as
                // words, 128, 3, "", 0, 0), else (a MIDI sound's too) none.  (Provisional: a sound playing, the
                // 128 and 3.)
                Some(Obj::Sound(snd)) => {
                    let Some(own) = slot_of(id) else { continue };
                    let labels = match snd.label.as_ref().filter(|_| !snd.midi) {
                        Some(l) => {
                            let mut entry = vec![Field::Text(l.clone()), Field::Ref(NONE), Field::Word(0), Field::Real(f32::from_bits(snd.fade_in as u32)), Field::Real(f32::from_bits(snd.fade_out as u32)), Field::Real(f32::from_bits(128)), Field::Word(3)];
                            match self.sound_label_row(l) {
                                // (A label with an EAX preset (UI\SoundInfo's tables): the
                                // preset, 0, a flags word, its least and most distance, and
                                // two lists, as the day's rooster and the night's wolf have
                                // them; provisional: the flags word, 289 for theirs.)
                                Some((eax, min, max)) if eax != "0" && !eax.is_empty() => {
                                    entry.extend([Field::Text(eax), Field::Word(0), Field::Word(289), Field::List(vec![vec![Field::Word(min.to_bits()), Field::Word(max.to_bits())]]), Field::List(vec![vec![Field::Word(0)]]), Field::List(vec![vec![Field::Word(0)]])]);
                                }
                                _ => entry.extend([Field::Text(String::new()), Field::Word(0), Field::Word(0)]),
                            }
                            vec![vec![Field::Word(1), Field::List(vec![entry])]]
                        }
                        None => vec![vec![Field::Word(0)]],
                    };
                    let tail = vec![Field::Timer(NONE, 0, Timing(None)), Field::Short(0), Field::Short(0), Field::Real(0.0), Field::Word(0), Field::List(labels)];
                    out.push((own.0, agent_first(own, codes::class::SOUND, &[]), Some(Second::Agent { own: vec![Field::Word(0)], handlers: game_handlers(self.game_object()), tail })));
                }
                // A timer dialog: its timer, 0; then 0, its colors (as made), its title (as
                // set, else DEFAULTTIMERDIALOGTEXT: read, Mafarazzo TD's "Next round in:"
                // at 30 seconds), and 1.  (Provisional: the colors and speed set.)
                Some(Obj::TimerDialog(d)) => {
                    let Some(own) = slot_of(id) else { continue };
                    let timer = slot_of(d.timer).map_or(NONE, rf);
                    let title = d.title.clone().unwrap_or_else(|| self.localized("DEFAULTTIMERDIALOGTEXT"));
                    let tail = vec![Field::Word(0), Field::Word(0xfffc_d211), Field::Word(0xfffc_d211), Field::Text(title), Field::Real(1.0)];
                    out.push((own.0, agent_first(own, codes::class::TIMER_DIALOG, &[]), Some(Second::Agent { own: vec![Field::Word(0), Field::Word(timer.0), Field::Word(timer.1), Field::Real(0.0)], handlers: game_handlers(self.game_object()), tail })));
                }
                // A rect (CScriptRect): its bounds, bottom, left, top, right.
                Some(Obj::Rect(r)) => {
                    let Some(own) = slot_of(id) else { continue };
                    let tail = vec![Field::Real(r.min_y), Field::Real(r.min_x), Field::Real(r.max_y), Field::Real(r.max_x)];
                    out.push((own.0, agent_first(own, codes::class::RECT, &[]), Some(Second::Agent { own: vec![Field::Word(0)], handlers: game_handlers(self.game_object()), tail })));
                }
                // A group (CGroup, `FUN_6f2464b0`, its class writing its whole record): its
                // units; its last enumeration's filter (+0x38) and its last ForGroup's
                // function's handler (+0x3c, `FUN_6f22ff30`); -1 (+0x40), the player its
                // last enumeration took or -1, and its limit or -1 (+0x44, +0x48,
                // `FUN_6f240890`; before any 0 and 0), and 0 (read: the quiet test map's
                // empty groups, Mafarazzo TD's of a rect's and of a player's units).
                Some(Obj::Group(g)) => {
                    let Some(own) = slot_of(id) else { continue };
                    let units = g.units.iter().filter_map(|&u| self.unit(u).and_then(|u| u.extra.object_slots[0].first().copied())).map(|s| vec![Field::Ref(rf(s))]).collect();
                    let filter = Some(g.filter).filter(|&f| f != 0).and_then(|f| slot_of(f)).map_or(NONE, rf);
                    let handler = g.callback.and_then(|func| self.tags.handler_objects.iter().find(|(_, f)| **f == func)).map_or(NONE, |(&s, _)| rf(self.tagged(s)));
                    let (player, limit) = g.enumerated.map_or((0, 0), |(p, l)| (p as u32, l as u32));
                    out.push((own.0, agent_first(own, codes::class::GROUP, &[]), Some(Second::Whole(vec![Field::List(units), Field::Ref(filter), Field::Ref(handler), Field::Word(u32::MAX), Field::Word(player), Field::Word(limit), Field::Word(0)]))));
                }
                _ => {}
            }
        }
        out.extend(self.region_records());
        out.extend(self.condition_records());
        out.extend(self.leaderboard_records());
        out.extend(self.camera_setup_records().0);
        out.extend(self.guard_post_records());
        out
    }

    /// An object's slot with its birth, as references name it.
    /// Whether a buff's object is still its own (not let go, its slot another's since).
    pub(crate) fn buff_kept(&self, b: &crate::types::Buff) -> bool {
        b.slot != 0 && self.tags.live.get(&b.slot).is_some_and(|e| e.1 == b.birth)
    }

    fn tagged(&self, slot: u32) -> (u32, u32) {
        (slot, self.tags.live.get(&slot).map_or(0, |e| e.1))
    }

    /// The two pathing maps' objects (`^pma`): the coarse one of 8-cell blocks, then the
    /// pathing map itself.
    fn pathing_map_objects(&self) -> [(u32, u32); 2] {
        let mut maps = self.tags.live.iter().filter(|(_, (c, _))| c == codes::class::PATHING_MAP).map(|(&s, (_, b))| (s, *b));
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
        let first = First::Agent { object: plain_base(own.0, own.1), agent: class_ref(codes::class::DESTRUCTABLE), refs: vec![Ref(pos.0, pos.1)], links: [Link(none, 0), Link(Ref(pos.0, pos.1), 0), Link(none, 0), Link(none, 0)] };
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

    /// A destructable's facing as its position saves it: in degrees, its type's fixed
    /// rotation if it has one, else as placed (the map's radians times 57.29578), times
    /// 0.01745329 (read: every tree of Booty Bay and Forestwalk, fixedRot 270, saves
    /// 0x4096cbe3, which 270 times 0.01745329 gives; Mafarazzo TD's six placed at
    /// 0x40963ceb save 0x40963cea, their 269.00003 degrees so; provisional: those
    /// constants, which the binary does not hold as such).
    fn saved_destructable_facing(&self, d: &crate::types::Destructable) -> f32 {
        let fixed = self.objects.destructables.get(d.type_id).map_or(-1.0, |o| o.real("fixedRot"));
        let degrees = if fixed >= 0.0 { fixed } else { d.placed_facing.map_or(d.facing, |r| r * 57.295_78) };
        degrees * 0.017_453_29
    }

    /// The W3GS slot table as a save holds it: the slot count; each map slot (its players
    /// but the neutral ones, those in use) its player's number from 1 (a computer's 0),
    /// 100 (its download), 2 (in use), whether a computer, its team, color, race (its
    /// preference, selectable), the computer's difficulty, and its handicap; the seed, 0
    /// (the layout), and the start spots (read: the quiet test map's two).
    pub fn saved_slot_table(&self) -> Vec<u8> {
        let slots: Vec<&crate::types::Player> = self.players.iter().take(12).filter(|p| p.slot_state != crate::types::slot_state::EMPTY).collect();
        let mut o = vec![slots.len() as u8];
        for p in &slots {
            let computer = p.controller == crate::types::map_control::COMPUTER;
            let race = 0x40 | if p.race_preference & 0x3f != 0 { p.race_preference as u8 & 0x3f } else { 0x20 };
            let ai = self.ai_difficulty.get(&p.id).copied().unwrap_or(1) as u8;
            o.extend([if computer { 0 } else { p.id + 1 }, 100, 2, computer as u8, p.team as u8, p.color as u8, race, ai, (p.handicap * 100.0).round() as u8]);
        }
        o.extend(self.seed.to_le_bytes());
        o.extend([0, slots.len() as u8]);
        o
    }

    /// The object manager's header as saved (`FUN_6f15c9b0`): two words, two reals (25
    /// and 0.2), its update counter and that's parity, each table's births, four
    /// groups of a clock (a time, a word, its wrap 300, a word, a count), and "espi".
    /// The two words and the counts are the running game's (they differ between saves
    /// of one moment); the original loads a save with them 0 (read: the quiet test
    /// map's save so changed, `w3zedit header0`).  Provisional: the counts, written 0.
    pub fn saved_manager_header(&self, second_births: u32) -> Vec<u8> {
        let mut o = Vec::new();
        let mut word = |v: u32| o.extend(v.to_le_bytes());
        let main = f32::from_bits(self.main_clock.1);
        word(0);
        word(0);
        word(25f32.to_bits());
        word(0.2f32.to_bits());
        // (The movement steps' counter, the path manager's +0x538: 1024 at the start
        // and one more as each step begins, `FUN_6f15aa80`, the game's first step at
        // its clock 0 not counted; read, 2816 at Mafarazzo TD's 53.8 seconds, the port's
        // 1793rd step done, and 3070 at its load's turn 77.  Provisional: which step
        // goes uncounted.)
        word(1024u32.wrapping_add((self.steps_total as u32).saturating_sub(1)));
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
    pub fn write_save(&self, template: &SaveGame, extra: Vec<(u32, Second)>) -> SaveGame {
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
        let mut seconds: std::collections::HashMap<u32, Second> = extra.into_iter().collect();
        for &h in &self.destructable_order {
            if let Some((slot, f, sec)) = self.destructable_own_records(h) {
                records.insert(slot, f);
                seconds.insert(slot, sec);
            }
        }
        let trace = std::env::var("W3Z_TRACE").is_ok();
        let tr = |what: &str, h: u32| {
            if trace {
                eprintln!("w3z: {what} {h}");
            }
        };
        tr("destructables done", 0);
        let unit_reals: Vec<(u32, First, Option<Second>)> = self.unit_order.iter().flat_map(|&h| {
            tr("unit reals", h);
            let mut v = self.unit_real_records(h);
            tr("modifiers", h);
            v.extend(self.unit_modifier_records(h));
            tr("hero flags", h);
            v.extend(self.hero_flag_records(h));
            tr("construction", h);
            v.extend(self.construction_records(h));
            tr("listeners", h);
            v.extend(self.unit_listener_records(h));
            v.extend(self.mine_payout_records(h));
            tr("position", h);
            v.extend(self.unit_position_records(h));
            tr("agent", h);
            v.extend(self.unit_agent_record(h));
            v
        }).collect();
        tr("unit reals done", 0);
        for (slot, f, sec) in self.trigger_records().into_iter().chain(self.execution_records()).chain(self.upgrade_records()).chain(self.script_object_records()).chain(self.player_records(&client_listeners(template))).chain(unit_reals).chain(self.captain_records()).chain(self.item_records()).chain(self.saved_order_records()).chain(self.mine_fog_records()).chain(self.save_event_records()) {
            records.insert(slot, f);
            if let Some(sec) = sec {
                seconds.insert(slot, sec);
            }
        }
        // The units' own records (their second table's rally points as the template has
        // them).
        let rallies = rally_points(template);
        let art = client_art(template);
        tr("first records done", 0);
        for &h in &self.unit_order {
            tr("unit own", h);
            if let Some((slot, sec)) = self.unit_own_record(h, &rallies) {
                seconds.insert(slot, sec);
            }
            // Its abilities' agents, and the own records of the kinds written.
            // (The AI lists' after.)
            records.extend(self.ability_agent_records(h, &rallies));
            for (slot, f, sec) in self.splash_records(h) {
                records.insert(slot, f);
                seconds.insert(slot, sec);
            }
            let chain = self.ability_chain(h, &rallies);
            let olders = self.chain_older(h, &chain);
            tr("unit abilities", h);
            for (k, &(t, code)) in chain.iter().enumerate() {
                let older = olders[k];
                if let Some(sec) = self.ability_own_record(h, t, &code, older, &rallies, &art) {
                    seconds.insert(t.0, sec);
                }
            }
        }
        tr("units' own done", 0);
        for (slot, sec) in self.unit_ref_records().into_iter().chain(self.unit_ai_records()) {
            seconds.insert(slot, sec);
        }
        // The units' regions (their search marks the template's, the running game's).
        let marks: std::collections::HashMap<u32, u32> = objects[0].iter().filter_map(|o| match o.first() { Ok(First::Region { words, .. }) => Some((o.entry.tag, words[0])), _ => None }).collect();
        tr("refs and ai done", 0);
        for &h in &self.unit_order {
            tr("unit path", h);
            if let Some((slot, f)) = self.unit_path_record(h) {
                records.insert(slot, f);
            }
            records.extend(self.unit_pil_records(h));
            for (slot, mut f) in self.unit_region_records(h) {
                if let (First::Region { words, .. }, Some(&m)) = (&mut f, marks.get(&slot)) {
                    words[0] = m;
                }
                records.insert(slot, f);
            }
        }
        // The agents with no properties, each its class's definition alone.
        tr("unit paths done", 0);
        for (slot, f) in self.plain_agent_records() {
            records.entry(slot).or_insert(f);
        }
        // The pathing system's objects (their counters the template's).
        let counters = |code: &[u8; 4]| objects[0].iter().find(|o| &o.entry.kind.to_be_bytes() == code).and_then(|o| o.first().ok());
        let accel_word = match counters(codes::class::LONG_RANGE_ACCELERATOR) {
            Some(First::Accelerator { word, .. }) => word,
            _ => 48,
        };
        let system_short = match counters(codes::class::LONG_RANGE_SYSTEM) {
            Some(First::System { short, .. }) => short,
            _ => 0,
        };
        tr("plain agents done", 0);
        for (slot, f) in self.pathing_system_records(accel_word, system_short) {
            records.entry(slot).or_insert(f);
        }
        for (slot, f) in self.game_object_records().into_iter().chain(self.time_of_day_records()) {
            records.entry(slot).or_insert(f);
        }
        tr("pathing done", 0);
        for (slot, sec) in self.plain_agent_seconds() {
            seconds.entry(slot).or_insert(sec);
        }
        tr("plain seconds done", 0);
        for (slot, sec) in self.player_agent_seconds(&client_listeners(template), &rallies).into_iter().chain(self.player_ai_seconds()) {
            seconds.entry(slot).or_insert(sec);
        }
        tr("player seconds done", 0);
        for &h in &self.unit_order {
            tr("ability reals", h);
            for (slot, f) in self.ability_real_records(h, &rallies).into_iter().chain(self.structure_region_records(h)) {
                records.entry(slot).or_insert(f);
            }
            let (mut firsts, mut secs) = self.mine_circle_records(h);
            let (bf, bs) = self.blight_growth_records(h);
            firsts.extend(bf);
            secs.extend(bs);
            for (slot, f) in firsts {
                records.entry(slot).or_insert(f);
            }
            for (slot, sec) in secs {
                seconds.entry(slot).or_insert(sec);
            }
        }
        // (A haunted mine's ring slots without their mine, gone and its slots kept by
        // the port, are left out, their slots freed: their records would be empty.)
        for (slot, (code, _)) in &self.tags.live {
            if code == codes::class::MINE_SLOT && !seconds.contains_key(slot) {
                records.remove(slot);
            }
        }
        // The main clock set back every 300 seconds, the dues as the original's, its
        // agents' timers' and its properties' set back as many times (read: Forestwalk's
        // computer game at 450 seconds, both from 150; at 600, saved by a timer due as
        // the clock was set back the second time, the agents' from 0.06 and the
        // properties' from 300, theirs set back as the next step begins).  Only those
        // the port reckons on the game's time are set back (due now or after; a gold
        // run's are on the agent queue's clock already).
        let wraps = self.tick_wraps.last().copied().unwrap_or(0);
        if wraps > 0 {
            let from = self.time - 1.0;
            for (slot, f) in records.iter_mut() {
                let _ = slot;
                set_back_first(f, wraps, from);
            }
            for sec in seconds.values_mut() {
                set_back_second(sec, wraps, from);
            }
        }
        // W3Z_COVERAGE: by kind, the objects whose first and second records are the
        // template's still.
        let mut kept: std::collections::BTreeMap<String, (u32, u32, u32)> = Default::default();
        for o in objects[0].iter_mut() {
            let e = kept.entry(String::from_utf8_lossy(&o.entry.kind.to_be_bytes()).into_owned()).or_default();
            e.0 += 1;
            if let Some(r) = records.get(&o.entry.tag) {
                o.first = r.to_bytes();
            } else {
                e.1 += 1;
                if std::env::var("W3Z_COVERAGE").is_ok_and(|v| v == "list") {
                    let owner = match o.first() {
                        Ok(First::Real { base, .. } | First::Flag { base, .. } | First::Pos { base, .. }) => format!("{:?}", base.object.links[0].3),
                        _ => String::new(),
                    };
                    eprintln!("  kept first {} {} {owner}", o.entry.tag, String::from_utf8_lossy(&o.entry.kind.to_be_bytes()));
                }
            }
            if let Some(Ok(b)) = seconds.get(&o.entry.tag).map(|sec| sec.to_bytes(o.entry.kind)) {
                o.second = b;
            } else if !o.second.is_empty() {
                e.2 += 1;
            }
        }
        if std::env::var("W3Z_COVERAGE").is_ok() {
            let first: u32 = kept.values().map(|e| e.1).sum();
            let second: u32 = kept.values().map(|e| e.2).sum();
            eprintln!("coverage: of {} objects, {first} first and {second} second records the template's; by kind (objects, first kept, second kept):", objects[0].len());
            for (k, e) in kept.iter().filter(|(_, e)| e.1 + e.2 > 0) {
                eprintln!("  {k}: {e:?}");
            }
        }
        // The camera's objects in the second table, and the camera setups' reals.
        for (e, f, sec) in self.camera_records().into_iter().chain(self.camera_setup_records().1.into_iter().map(|(e, f)| (e, f, None))) {
            if let Some(o) = objects[1].iter_mut().find(|o| o.entry.tag == e.tag) {
                o.first = f.to_bytes();
                if let Some(Ok(b)) = sec.map(|sec| sec.to_bytes(e.kind)) {
                    o.second = b;
                }
            }
        }
        m.set_objects(&objects);
        m.free[0] = self.saved_free_list();
        // The regions section (the pathing maps and grids; their search marks the
        // template's, the running game's, else 0).
        let marks = w3_formats::w3z::PathingMaps::parse(&m.data[0]).map_or([0, 0], |maps| [maps.proximity[0].words[0], maps.proximity[1].words[0]]);
        if let Some(ours) = self.saved_pathing_maps(marks) {
            m.data[0] = ours.to_bytes();
        }
        s
    }

    /// The game interface's camera (the second table's `+cam`, made as the slots are
    /// assigned, after the classes' definitions; read against the quiet test map's and
    /// Forestwalk's): its fourteen properties, each its index (eleven reals, the target's
    /// eye's place and the target's in pathing cells, each with its coarse and fine
    /// regions, and a real), and its own record (CCameraWar3: 120 as a real's bits, an
    /// identity matrix, 1 and 2; its points, the local player's start and the target with
    /// the ground's height; the start again and the camera bounds' sides and corners,
    /// 1.0 twice, and four matrices).  The reals: 1700, 85 degrees, the distance, the far
    /// clip, 100, the field of view, the roll, the rotation, the angle of attack, the
    /// height offset, 0 and 0 (angles in radians as `deg * pi / 180` in single
    /// precision but the 85 degrees', in double; a zero -0.0).  The eye's place is the distance's run along the ground
    /// behind the target by the rotation.  Provisional: the stamp (the client's clock in
    /// the original), the regions' search marks (0).
    pub fn camera_records(&self) -> Vec<(Tagged, First, Option<Second>)> {
        let c = &self.camera;
        let rad = |deg: f32| deg * std::f32::consts::PI / 180.0;
        let zero = |v: f32| if v == 0.0 { -0.0 } else { v };
        let tag = |k: u32| (0x8000_0000 | (1077 + k), 1077 + k);
        let entry = |k: u32, code: &[u8; 4], family: &[u8; 4]| Tagged { family: u32::from_be_bytes(*family), kind: u32::from_be_bytes(*code), tag: tag(k).0, birth: tag(k).1 };
        let cam = tag(0);
        // (Its properties: the reals 1 to 11, the places 12 and 15, the last real 18.)
        let props: Vec<(u32, u32)> = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 15, 18].iter().map(|&k| tag(k)).collect();
        let (distance, far, aoa, fov, roll, rotation, z) = (c.fields[0], c.fields[1], c.fields[2], c.fields[3], c.fields[4], c.fields[5], c.fields[6]);
        let reals: [[f32; 4]; 11] = [
            [1700.0, 0.0, 100.0, 10000.0],
            // (This one in double precision: read, 1.4835299.)
            [(85.0 * std::f32::consts::PI as f64 / 180.0) as f32, 0.0, 0.0, rad(90.0)],
            [distance, 0.0, 100.0, 10000.0],
            [far, 0.0, 100.0, 10000.0],
            [100.0, 0.0, 0.0, 100.0],
            [rad(fov), 0.0, rad(20.0), rad(120.0)],
            [zero(rad(roll)), 0.0, f32::NEG_INFINITY, f32::INFINITY],
            [zero(rad(rotation)), 0.0, f32::NEG_INFINITY, f32::INFINITY],
            [zero(rad(aoa)), 0.0, f32::NEG_INFINITY, f32::INFINITY],
            [zero(z), 0.0, -4096.0, 4096.0],
            [-0.0, 0.0, 0.0, f32::MAX],
        ];
        let mut out = vec![(entry(0, codes::class::CAMERA, codes::class::AGENT), agent_first(cam, codes::class::CAMERA, &props), None)];
        for (k, v) in reals.iter().enumerate() {
            out.push((entry(1 + k as u32, codes::class::REAL, codes::class::PROPERTY_FAMILY), First::Real { base: listed_property_base(&props, k, cam), values: *v }, None));
        }
        // The eye's place and the target's, in pathing cells.
        let run = distance * rad(aoa).cos();
        let (rx, ry) = (rad(rotation).cos(), rad(rotation).sin());
        let origin = self.pathing.as_ref().map_or([0.0; 2], |p| p.origin);
        let maps = self.pathing_map_objects();
        // (The target in cells, the eye's the run in cells back from it: read, to the bit.)
        let (tx, ty) = ((c.x - origin[0]) / 32.0, (c.y - origin[1]) / 32.0);
        for (j, (cx, cy)) in [(tx - run * rx / 32.0, ty - run * ry / 32.0), (tx, ty)].into_iter().enumerate() {
            let k = 12 + 3 * j as u32;
            let (row, col) = (cy.floor().max(0.0) as u32, cx.floor().max(0.0) as u32);
            let mut base = listed_property_base(&props, 11 + j, cam);
            base.real = self.main_clock_now();
            out.push((entry(k, codes::class::POSITION, codes::class::PROPERTY_FAMILY), First::Pos { base, values: [cx, cy, 0.0, 0.0, f32::MAX, 0.0, 0.0], refs: [rf(tag(k + 1)), rf(tag(k + 2))] }, None));
            out.push((entry(k + 1, codes::class::PATHING_REGION, codes::class::PATHING_MAP), First::Region { refs: [rf(maps[0]), rf(tag(k))], word: 0, bounds: [row >> 3, col >> 3, (row >> 3) + 1, (col >> 3) + 1], words: [0, 1, 0] }, None));
            out.push((entry(k + 2, codes::class::PATHING_REGION, codes::class::PATHING_MAP), First::Region { refs: [rf(maps[1]), rf(tag(k))], word: 0x0100_0000, bounds: [row, col, row + 1, col + 1], words: [0, 1, 0] }, None));
        }
        out.push((entry(18, codes::class::REAL, codes::class::PROPERTY_FAMILY), First::Real { base: listed_property_base(&props, 13, cam), values: [-0.0, 0.0, f32::NEG_INFINITY, f32::INFINITY] }, None));
        // Its own record.
        let r = Field::Real;
        let start = self.players.get(self.local_player as usize).and_then(|p| self.start_locations.get(p.start_location.max(0) as usize)).map_or((c.x, c.y), |s| (s.x, s.y));
        let b = self.camera_bounds;
        let (xmin, xmax) = (b.iter().step_by(2).copied().fold(f32::MAX, f32::min), b.iter().step_by(2).copied().fold(f32::MIN, f32::max));
        let (ymin, ymax) = (b.iter().skip(1).step_by(2).copied().fold(f32::MAX, f32::min), b.iter().skip(1).step_by(2).copied().fold(f32::MIN, f32::max));
        let identity = || [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0f32];
        let matrix = |m: [f32; 9]| Field::List(vec![m.iter().map(|&v| r(v)).collect()]);
        let mut tail = vec![Field::Word(0), r(f32::from_bits(120)), Field::Word(0), Field::Ref(NONE), r(0.0), r(0.0), r(0.0)];
        tail.extend(identity().map(r));
        tail.extend([Field::Timer(NONE, 0, Timing(None)), Field::Word(1), Field::Word(2)]);
        let mut points = vec![vec![r(start.0), r(start.1), r(0.0)], vec![r(c.x), r(c.y), r(self.ground_z(c.x, c.y))]];
        points.extend((0..6).map(|_| vec![r(0.0), r(0.0), r(0.0)]));
        tail.push(Field::List(points));
        tail.extend((0..20).map(|_| Field::Ref(NONE)));
        tail.push(Field::Byte(0));
        tail.extend((0..5).map(|_| Field::Ref(NONE)));
        tail.extend([start.0, start.1, ymin, xmin, ymax, xmax, xmin, ymin, xmax, ymax, xmin, ymax, xmax, ymin, 1.0, 1.0].map(r));
        tail.extend([identity(), identity(), [1.0, -0.0, 0.0, -0.0, 1.0, -0.0, 0.0, -0.0, 1.0], [2.0, 3.0, 2.0, 3.0, 2.0, 3.0, 2.0, 3.0, 2.0]].map(matrix));
        out[0].2 = Some(Second::Agent { own: Vec::new(), handlers: Handlers { keyed: Vec::new(), fixed: Default::default(), events: Vec::new() }, tail });
        out
    }

    /// A sound label's EAX preset and least and most distances, from the game's sound
    /// tables (UI\SoundInfo).
    fn sound_label_row(&self, label: &str) -> Option<(String, f32, f32)> {
        for t in ["UISounds", "AmbienceSounds", "AbilitySounds", "AnimSounds", "MIDISounds", "DialogSounds", "UnitAckSounds", "UnitCombatSounds", "EnvironmentSounds"] {
            let Ok(text) = self.chain.read_string(&format!("UI\\SoundInfo\\{t}.slk")) else { continue };
            let slk = w3_formats::slk::Table::parse(&text);
            if let Some(eax) = slk.get(label, "EAXFlags") {
                let num = |c: &str| slk.get(label, c).and_then(|v| v.as_str().parse::<f32>().ok()).unwrap_or(0.0);
                return Some((eax.as_str(), num("MinDistance"), num("MaxDistance")));
            }
        }
        None
    }

    /// The game's object (`+gam`, CGameWar3's agent) and its five properties: two flags
    /// (0), the time of day (a continuous real: the time as last set and the main
    /// clock then, its rate, 0 and 24; its two listeners, the `` `rcm `` and `` `rtl ``
    /// of the day's change, linked), the time's scale, and a flag (1).  The agent's last
    /// two links name the listeners too.  Provisional: the flags.
    /// The time of day's modifier and threshold listeners (read against the quiet test
    /// map's, at 8:15 by day): the `` `rcm `` of its rate (two words, -1 and 0x60050044,
    /// its rate and no limit, a node due 150 seconds after it was made, every 150); its
    /// listeners, chained on the game and on the time's real by their making: the
    /// script's (TriggerRegisterGameStateEvent; an equal one listening rising, at least,
    /// while the time is below its value, else falling, at most: Blizzard.j's dawn
    /// remade so by the melee setup's 8:00), the day's change's (by day 0.01 below dawn,
    /// falling, and 0.01 past dusk, rising; provisional by night, 0.01 past dawn,
    /// rising), and the day's end (24, rising).  Each two words (its event: 0x80101 and
    /// its comparison the script's, 0x80263 the change's, 0x80262 the day's end; and
    /// 0x20090400), its threshold, and whether rising; one rising ahead of the time a
    /// node checking every 4 seconds from its making (as a unit's).  Provisional: the
    /// nodes' counts (the running game's re-armed ones), a stopped time's.
    pub fn time_of_day_records(&self) -> Vec<(u32, First)> {
        let game = self.game_object();
        let mut props: Vec<(u32, u32)> = self.tags.live.iter().filter(|&(&s, (c, _))| s > game.0 && s <= game.0 + 5 && matches!(c, codes::class::FLAG | codes::class::REAL)).map(|(&s, (_, b))| (s, *b)).collect();
        props.sort();
        let Some(&real) = props.get(2) else { return Vec::new() };
        let mut out = Vec::new();
        let hours = self.misc_f32("DayHours", 24.0);
        let [rcm, _] = self.time_of_day_listeners;
        // (No modifier while suspended, the time standing: its listeners check nothing,
        // read, Mafarazzo TD's dusk listeners without a node.)
        let rate = if rcm.is_some() { hours / self.misc_f32("DayLength", 480.0) * self.time_of_day_scale } else { 0.0 };
        let tod = self.time_of_day;
        if let Some(r) = rcm.filter(|s| self.tags.live.contains_key(s)).map(|s| self.tagged(s)) {
            let made = self.made_abs(r.0).unwrap_or(0);
            let due = f32::from_bits(crate::efloat::add(made, 150f32.to_bits()));
            let count = self.tags.prop_counts.get(&r.0).copied().unwrap_or(0);
            let object = ObjectBase { real: 0.0, words: [u32::MAX, 0x6005_0044], links: [(Link(NONE, 0), Link(NONE, 0), rf(r), rf(game)), (Link(NONE, 0), Link(NONE, 0), rf(r), rf(real))], timings: [Timing(Some((due, 150.0, count, 0, 1))), Timing(None)] };
            out.push((r.0, First::Continuous { object, values: [rate, f32::MAX] }));
        }
        let ls = self.time_of_day_listener_list();
        let chain: Vec<(u32, u32)> = ls.iter().map(|l| l.0).collect();
        let now = self.time;
        for &(me, event, thr, rising, _) in &ls {
            let node = if rising && tod < thr && rate > 0.0 {
                // (Checked every 4 seconds from its making, or as the time reaches it.)
                let made = self.made_abs(me.0).map_or(0.0, f32::from_bits);
                let at_time = |t: f32| tod - rate * (now - t);
                let mut at = made;
                let mut due = made + 4f32.min((thr - at_time(made)) / rate);
                while due <= now {
                    at = due;
                    due = at + 4f32.min((thr - at_time(at)) / rate);
                }
                let period = 4f32.min((thr - at_time(at)) / rate);
                Timing(Some((due, period, self.tags.prop_counts.get(&me.0).copied().unwrap_or(0), 0, 1)))
            } else {
                Timing(None)
            };
            let (a0, b0) = chain_links(&chain, me, 0);
            let (a1, b1) = chain_links(&chain, me, 1);
            let object = ObjectBase { real: 0.0, words: [event, 0x2009_0400], links: [(a0, b0, rf(me), rf(game)), (a1, b1, rf(me), rf(real))], timings: [node, Timing(None)] };
            out.push((me.0, First::Threshold { object, real: thr, word: u32::from(rising) }));
        }
        out
    }

    /// The time of day's threshold listeners (as `time_of_day_records` has them), by
    /// their making: each itself, its event, threshold, whether rising, and the agent
    /// its event goes to (the script's registration's, else the game).
    fn time_of_day_listener_list(&self) -> Vec<((u32, u32), u32, f32, bool, Ref)> {
        let game = self.game_object();
        let hours = self.misc_f32("DayHours", 24.0);
        let (dawn, dusk, eps) = (self.misc_f32("Dawn", 6.0), self.misc_f32("Dusk", 18.0), 0.01f32);
        let tod = self.time_of_day;
        let mut ls = Vec::new();
        for &e in &self.events {
            let Some(Obj::Event(r)) = self.handles.get(e) else { continue };
            let EventKind::GameState { state: 2, op, value } = r.kind else { continue };
            let Some(&s) = self.tags.event_timers.get(&e).filter(|s| self.tags.live.contains_key(s)) else { continue };
            let reg = self.tags.handle_ids.get(&e).map_or(NONE, |&r| rf(r));
            // (Less, at most, equal, at least, greater: 0 to 4.)
            let op = if op == 2 { if tod < value { 3 } else { 1 } } else { op };
            ls.push((self.tagged(s), 0x8_0101 + op as u32, value, matches!(op, 3 | 4), reg));
        }
        let change = |s: Option<u32>| s.filter(|s| self.tags.live.contains_key(s)).map(|s| self.tagged(s));
        // (The day change's thresholds in the engine's arithmetic, `FUN_6f283c80` by
        // `FUN_6f06fa90` and `FUN_6f06fbb0`: read, dusk's 18.009998 in the quiet test
        // map's and Mafarazzo TD's saves.)
        let less = |a: f32| f32::from_bits(crate::efloat::sub(a.to_bits(), eps.to_bits()));
        let more = |a: f32| f32::from_bits(crate::efloat::add(a.to_bits(), eps.to_bits()));
        if self.daytime {
            ls.extend(change(self.tags.day_timer).map(|r| (r, 0x8_0263, less(dawn), false, rf(game))));
            ls.extend(change(self.tags.day_timer_2).map(|r| (r, 0x8_0263, more(dusk), true, rf(game))));
        } else {
            ls.extend(change(self.tags.day_timer).map(|r| (r, 0x8_0263, more(dawn), true, rf(game))));
        }
        ls.extend(change(self.time_of_day_listeners[1]).map(|r| (r, 0x8_0262, hours, true, rf(game))));
        ls.sort_by_key(|l| l.0 .1);
        ls
    }

    /// The game's keyed handlers (its record's): each listener of the time of day's
    /// with its event's agent and event, and those replaced since the game began,
    /// empty; by a key's low two bits, then its making.
    pub fn game_keyed_handlers(&self) -> Vec<(u32, Vec<(Ref, u32)>)> {
        let mut keys: Vec<((u32, u32), Vec<(Ref, u32)>)> = self.time_of_day_listener_list().into_iter().map(|(me, event, _, _, to)| (me, vec![(to, event)])).collect();
        keys.extend(self.tags.game_freed_keys.iter().map(|&k| (k, Vec::new())));
        keys.sort_by_key(|k| (k.0 .0 & 3, k.0 .1));
        keys.into_iter().map(|(k, v)| (k.0, v)).collect()
    }

    /// The newest of the time of day's listeners (its chains' head).
    fn time_of_day_head(&self) -> Option<(u32, u32)> {
        self.time_of_day_records().into_iter().filter(|(_, f)| matches!(f, First::Threshold { .. })).map(|(s, _)| self.tagged(s)).max_by_key(|r| r.1)
    }

    pub fn game_object_records(&self) -> Vec<(u32, First)> {
        let game = self.game_object();
        let mut props: Vec<(u32, u32)> = self.tags.live.iter().filter(|&(&s, (c, _))| s > game.0 && s <= game.0 + 5 && matches!(c, codes::class::FLAG | codes::class::REAL)).map(|(&s, (_, b))| (s, *b)).collect();
        props.sort();
        if props.len() != 5 {
            return Vec::new();
        }
        // (Its modifier and its newest listener, each in its own place: no modifier while
        // the time of day is suspended, the time's rate 0 then; read, Mafarazzo TD's at
        // 30 seconds, its rate 0 and its listener in the second place.)
        let modifier = self.time_of_day_listeners[0].filter(|s| self.tags.live.contains_key(s)).map(|s| self.tagged(s));
        let listeners = [modifier, self.time_of_day_head()];
        let link = |k: usize| listeners[k].map_or(Link(NONE, 0), |r| Link(rf(r), 0));
        let mut out = Vec::new();
        let hours = self.misc_f32("DayHours", 24.0);
        let rate = if modifier.is_some() { hours / self.misc_f32("DayLength", 480.0) * self.time_of_day_scale } else { 0.0 };
        for k in 0..5 {
            let mut base = listed_property_base(&props, k, game);
            let f = match k {
                2 => {
                    base.real = f32::from_bits(self.time_of_day_set.1);
                    base.links = listeners.map(|l| l.map_or(Link(NONE, 0), |r| Link(rf(r), 1)));
                    First::Real { base, values: [self.time_of_day_set.0, rate, 0.0, hours] }
                }
                3 => First::Real { base, values: [self.time_of_day_scale, 0.0, -f32::MAX, f32::MAX] },
                _ => First::Flag { base, value: u32::from(k == 4) },
            };
            out.push((props[k].0, f));
        }
        let mut first = agent_first(game, codes::class::GAME_STATE, &props);
        if let First::Agent { links, .. } = &mut first {
            links[2] = link(0);
            links[3] = link(1);
        }
        out.push((game.0, first));
        out
    }

    /// The game cache manager's own fields (`gccm`, read against the quiet test map's,
    /// whose profile held one saved cache): its file's name, and the saved caches as
    /// that file holds them (`Persist::caches_w3v`): their counter, each cache's name,
    /// counter, and missions, each its name, five counters, and its integers, reals,
    /// flags, units (their records' fields), and strings.  (Provisional: the order, the
    /// file's, name order; the caches the game makes and fills.)
    fn game_cache_fields(&self) -> Vec<Field> {
        const UNIT: &str = "w [www] wwwww f w fff w f [ww] fff [w] f s";
        let c = self.persist.caches_w3v();
        let text = |s: &str| Field::Text(s.to_string());
        let caches = c
            .caches
            .iter()
            .map(|cache| {
                let missions = cache
                    .missions
                    .iter()
                    .map(|m| {
                        let mut row = vec![text(&m.name)];
                        row.extend(m.counters.iter().map(|&w| Field::Word(w)));
                        row.push(Field::List(m.ints.iter().map(|(k, v)| vec![text(k), Field::Word(*v as u32)]).collect()));
                        row.push(Field::List(m.reals.iter().map(|(k, v)| vec![text(k), Field::Real(*v)]).collect()));
                        row.push(Field::List(m.bools.iter().map(|(k, v)| vec![text(k), Field::Word(*v)]).collect()));
                        row.push(Field::List(m.units.iter().filter_map(|(k, v)| Some(std::iter::once(text(k)).chain(w3_formats::w3z::parse_fields(UNIT, v).ok()?).collect())).collect()));
                        row.push(Field::List(m.strings.iter().map(|(k, v)| vec![text(k), text(v)]).collect()));
                        row
                    })
                    .collect();
                vec![text(&cache.name), Field::Word(cache.counter), Field::List(missions)]
            })
            .collect();
        vec![text("Campaigns.w3v"), Field::Word(c.counter), Field::List(caches)]
    }

    /// Second records of the AI's and engine's plain objects: each camp's AI (`AIcs`: 0,
    /// its creeps' count, and twelve places for them, its creeps by slot then none), each
    /// town site's (`AIts`: 0, its index, its place twice, none, its gold mine, none),
    /// the `shft` (none, 0, 0, 0, and no timer), and the trigger queue (`+qum`: none
    /// queued).  Provisional: what a running AI keeps in them; a site a town has taken
    /// (its town hall after its place, its center and a second place computed: read,
    /// the quiet test map's computer's, 8148.0054 and 8832), a `shft` at work, a queue
    /// in use.
    pub fn plain_agent_seconds(&self) -> Vec<(u32, Second)> {
        let events = || Handlers { keyed: Vec::new(), fixed: Default::default(), events: vec![Vec::new(); 0x400] };
        let none3 = || vec![Field::Word(u32::MAX), Field::Word(u32::MAX), Field::Word(0)];
        let mut out = Vec::new();
        for (&slot, (code, _)) in &self.tags.live {
            match code {
                codes::class::CREEP_CAMP => {
                    let mut members: Vec<(u32, u32)> = self.unit_order.iter().filter_map(|&h| self.unit(h)).filter(|u| u.extra.camp_ai.is_some_and(|c| c.0 == slot)).filter_map(|u| u.extra.object_slots[0].first().copied()).collect();
                    members.sort();
                    let list = (0..12).map(|k| members.get(k).map_or_else(none3, |m| vec![Field::Word(m.0), Field::Word(m.1), Field::Word(0)])).collect();
                    out.push((slot, Second::Agent { own: vec![Field::Word(0), Field::Word(members.len() as u32), Field::List(list)], handlers: events(), tail: Vec::new() }));
                }
                // A town site (CAITownSite): 0, its index, where its town gathers and its
                // own place, its town's hall, 0, its mine, 0, and none.  A computer's town
                // on it (its hall the nearest to the mine, within 1500) gives the hall and
                // its attack captain's home, and a haunted or entangled mine over the
                // site's stands for it (read: Echo Isles' undead computer at 60 seconds,
                // its Necropolis and haunted mine on site 2, gathering at 4499.96,
                // 2698.23; the hero map's at 450 seconds at its attack captain's home;
                // the human's town on none).  Without the town an orc computer loaded
                // from the port's save sent its gold peons to the trees.
                codes::class::TOWN_SITE => {
                    let Some(index) = self.tags.sites.iter().position(|s| s.2 .0 == slot) else { continue };
                    let (x, y, _) = self.tags.sites[index];
                    let (mine, at, town) = self.site_town(index);
                    let mine = mine.and_then(|h| self.unit(h)).and_then(|u| u.extra.object_slots[0].first().copied()).unwrap_or((u32::MAX, u32::MAX));
                    let town = town.and_then(|h| self.unit(h));
                    let hall = town.and_then(|u| u.extra.object_slots[0].first().copied());
                    let home = town.and_then(|u| self.ai.get(&u.owner)).map_or((x, y), |st| st.attack.home);
                    // (Its own place, a town's, midway from its hall to its mine: read, Echo
                    // Isles' 4896, 3264, the Necropolis at 4672 and the mine at 5120.)
                    let place = town.map_or((x, y), |u| ((u.x + at.0) / 2.0, (u.y + at.1) / 2.0));
                    let mut own = vec![Field::Word(0), Field::Word(index as u32), Field::Real(home.0), Field::Real(home.1), Field::Real(place.0), Field::Real(place.1)];
                    match hall {
                        Some(r) => own.extend([Field::Word(r.0), Field::Word(r.1), Field::Word(0)]),
                        None => own.extend(none3()),
                    }
                    own.extend([Field::Word(mine.0), Field::Word(mine.1), Field::Word(0)]);
                    own.extend(none3());
                    out.push((slot, Second::Agent { own, handlers: events(), tail: Vec::new() }));
                }
                // A gold mine's shaft: the worker in it, 1, and its time to come out on
                // the mine's ability (its time in, event 0xd01c1; read: Forestwalk's
                // peasant at 12 seconds), else none.
                codes::class::MINE_SHAFT => {
                    let (who, timer) = self.shaft_worker(slot).unwrap_or(((u32::MAX, u32::MAX), Field::Timer(NONE, 0, Timing(None))));
                    let inside = who.0 != u32::MAX;
                    out.push((slot, Second::Agent { own: vec![Field::Word(who.0), Field::Word(who.1), Field::Word(0), Field::Word(inside as u32), Field::Real(0.0)], handlers: Handlers { keyed: Vec::new(), fixed: Default::default(), events: Vec::new() }, tail: vec![timer] }))
                }
                codes::class::TRIGGER_QUEUE => out.push((slot, Second::Agent { own: vec![Field::Word(0)], handlers: events(), tail: vec![Field::List(Vec::new()), Field::List(Vec::new())] })),
                codes::class::GAME_CACHE_MANAGER => out.push((slot, Second::Agent { own: vec![Field::Word(0)], handlers: events(), tail: self.game_cache_fields() })),
                // An AI script's thread (CThreadAI, `FUN_6f9bef30`): 0, its player's AI,
                // 0, its number's successor and its number (its main's both 0), its last
                // sleep; that sleep's timer (event 0xd01bf, once), its length again, and
                // the thread's place (read: Forestwalk's undead computer's five at 5
                // seconds; provisional: the two numbers, read as the threads in turn).
                codes::class::AI_THREAD => {
                    let Some((player, o)) = self.ai.iter().find_map(|(&p, st)| st.thread_objects.iter().find(|(_, o)| o.slot == slot).map(|(&place, o)| (p, (place, *o)))) else { continue };
                    let (place, o) = o;
                    let ai = self.player_ai_object(player).unwrap_or((u32::MAX, u32::MAX));
                    let (a, b) = if o.number == 0 { (0, 0) } else { (o.number + 1, o.number) };
                    let me = self.tagged(slot);
                    let timer = if o.count == 0 { Field::Timer(NONE, 0, Timing(None)) } else { Field::Timer(rf(me), 0, Timing(Some((o.due, o.sleep, o.count, 0xd_01bf, 0)))) };
                    let (chunks, rest) = timer_chunks(o.sleep);
                    let own = vec![Field::Word(0), Field::Word(ai.0), Field::Word(ai.1), Field::Word(0), Field::Word(a), Field::Word(b), Field::Real(o.sleep)];
                    out.push((slot, Second::Agent { own, handlers: events(), tail: vec![timer, Field::Short(chunks.min(0xffff) as u16), Field::Short(chunks.min(0xffff) as u16), Field::Real(rest), Field::Word(place)] }));
                }
                // A unit's detection by a player (CDetectedData): 0; 0 and sixteen 0,
                // twice (read: Forestwalk's eighteen, none yet counted).
                codes::class::DETECTION => {
                    let zeros = || Field::List((0..16).map(|_| vec![Field::Word(0)]).collect());
                    out.push((slot, Second::Agent { own: vec![Field::Word(0)], handlers: events(), tail: vec![Field::Word(0), zeros(), Field::Word(0), zeros()] }));
                }
                // A unit's shared vision (CSharedVisionData): 0; the players it is shown
                // to, and sixteen counts, each player's of the buffs showing it
                // (`FUN_6f688b80`, `FUN_6f678210`).
                codes::class::SHARED_VISION => {
                    let mut counts = [0u32; 16];
                    if let Some(u) = self.unit_order.iter().filter_map(|&h| self.unit(h)).find(|u| u.extra.vision_record.is_some_and(|v| v.0 == slot)) {
                        for p in u.buffs.iter().filter(|b| self.buff_kept(b)).filter_map(|b| b.vision_player) {
                            counts[p as usize % 16] += 1;
                        }
                    }
                    let mask = (0..16).filter(|&p| counts[p] > 0).fold(0u32, |m, p| m | 1 << p);
                    let list = Field::List(counts.iter().map(|&c| vec![Field::Word(c)]).collect());
                    out.push((slot, Second::Agent { own: vec![Field::Word(0)], handlers: events(), tail: vec![Field::Word(mask), list] }));
                }
                _ => {}
            }
        }
        out
    }

    /// The pathing system's objects' first records: the two pathing maps' and the four
    /// accelerator grids' (`^pma`, `^ama`: none), the long-range system's (`l^sy`: the
    /// pathing map, and a short counting its work), and the accelerator's (`l^ac`: its
    /// grids, an empty queue of 256, and a word counting its work).  Provisional: the
    /// two counters (48 and 0 on the quiet test map, more as units path).
    pub fn pathing_system_records(&self, accel_word: u32, system_short: u16) -> Vec<(u32, First)> {
        let of = |code: &[u8; 4]| {
            let mut v: Vec<(u32, u32)> = self.tags.live.iter().filter(|(_, (c, _))| c == code).map(|(&s, (_, b))| (s, *b)).collect();
            v.sort();
            v
        };
        let (pma, ama) = (of(codes::class::PATHING_MAP), of(codes::class::ACCELERATOR_GRID));
        let mut out: Vec<(u32, First)> = pma.iter().chain(&ama).map(|&(s, _)| (s, First::Empty)).collect();
        if let (Some(&sy), Some(&map)) = (of(codes::class::LONG_RANGE_SYSTEM).first(), pma.get(1)) {
            out.push((sy.0, First::System { owner: rf(map), short: system_short }));
        }
        if let (Some(&ac), true) = (of(codes::class::LONG_RANGE_ACCELERATOR).first(), ama.len() == 4) {
            let array = w3_formats::w3z::Array { words: [3072, 3072, 0, 256], entries: vec![[0u32; 3]; 256] };
            out.push((ac.0, First::Accelerator { refs: [rf(ama[0]), rf(ama[1]), rf(ama[2]), rf(ama[3])], array, word: accel_word }));
        }
        out
    }

    /// The save's event data while a network game's save is made (`` `gsed ``, read: the
    /// original's LAN save of Mafarazzo TD): its first record its class's definition
    /// alone and its second timer running 0.0001 s from the main clock, the queue's next
    /// count its (provisional: the port's count of the starts); its second the game's
    /// handler for event 0x80267 in the first fixed list, and no event lists.
    pub fn save_event_records(&self) -> Vec<(u32, First, Option<Second>)> {
        let Some(me) = self.save_event else { return Vec::new() };
        let First::Agent { mut object, agent, refs, links } = agent_first(me, codes::class::SAVE_EVENT_DATA, &[]) else { return Vec::new() };
        let now = self.main_clock_now();
        object.timings[1] = Timing(Some((f32::from_bits(crate::efloat::add(now.to_bits(), 0.0001f32.to_bits())), 0.0001, self.tags.timer_starts + 1, 0, 0)));
        let game = self.game_object();
        let mut second = Vec::new();
        for w in [0, 1, game.0, game.1, 0x8_0267, 0, 0, 0, 0] {
            second.extend(u32::to_le_bytes(w));
        }
        vec![(me.0, First::Agent { object, agent, refs, links }, Some(Second::Raw(second)))]
    }

    /// The first records of the agents that have no properties (read: the AIs of units,
    /// camps, town sites, and players, the groups, sounds, timer dialogs, results,
    /// shifts, rects, the queue, and the game cache manager): each its class's definition
    /// alone.
    pub fn plain_agent_records(&self) -> Vec<(u32, First)> {
        const PLAIN: [&[u8; 4]; 15] = [codes::class::UNIT_AI, codes::class::CREEP_CAMP, codes::class::TOWN_SITE, codes::class::PLAYER_AI, codes::class::GROUP, codes::class::SOUND, codes::class::TIMER_DIALOG, codes::class::MINE_SLOT, codes::class::MINE_SHAFT, codes::class::RECT, codes::class::TRIGGER_QUEUE, codes::class::GAME_CACHE_MANAGER, codes::class::DETECTION, codes::class::SHARED_VISION, codes::class::AI_THREAD];
        // (A unit's AI without its unit, the unit gone and its AI kept by the port, is
        // left out, its slot freed: the original holds none such, and its reader
        // would meet an empty record.)
        let held: std::collections::HashSet<u32> = self.unit_order.iter().filter_map(|&h| self.unit(h)).flat_map(|u| u.extra.ai_slots.iter().map(|&(s, _)| s)).collect();
        self.tags.live.iter().filter(|(_, (c, _))| PLAIN.contains(&c)).filter(|(s, (c, _))| c != codes::class::UNIT_AI || held.contains(s)).map(|(&slot, (c, birth))| (slot, agent_first((slot, *birth), c, &[]))).collect()
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
        let mut out = vec![format!("time {:.6}, agent clock {:.6}", self.time, f32::from_bits(self.agent_step))];
        let Some(m) = orig.objects.as_ref() else { return vec!["no object manager".into()] };
        let ours = self.saved_table();
        // W3Z_BIRTHS=<from>-<to>: our live objects born in that range, by birth.
        if let Some((lo, hi)) = std::env::var("W3Z_BIRTHS").ok().and_then(|v| v.split_once('-').and_then(|(a, b)| Some((a.parse::<u32>().ok()?, b.parse::<u32>().ok()?)))) {
            let mut v: Vec<&Tagged> = ours.iter().filter(|t| (lo..=hi).contains(&t.birth)).collect();
            v.sort_by_key(|t| t.birth);
            for t in v {
                out.push(format!("birth {} {} {}", t.birth, t.tag, String::from_utf8_lossy(&t.kind.to_be_bytes())));
            }
        }
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
            let theirs: Vec<&SavedObject> = objects[1].iter().filter(|o| o.entry.kind == u32::from_be_bytes(*codes::class::AGENT_DEFINITION)).collect();
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
                // (By the bytes written: -0 is not 0.)
                let ok1 = f.to_bytes() == o.first;
                let ok2 = sec.as_ref().is_none_or(|s| s.to_bytes(o.entry.kind).is_ok_and(|b| b == o.second));
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
                // (By the bytes written: -0 is not 0.)
                let ok1 = f.to_bytes() == o.first;
                let ok2 = sec.as_ref().is_none_or(|s| s.to_bytes(o.entry.kind).is_ok_and(|b| b == o.second));
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
                // (By the bytes written: -0 is not 0.)
                let ok1 = f.to_bytes() == o.first;
                let ok2 = sec.as_ref().is_none_or(|s| s.to_bytes(o.entry.kind).is_ok_and(|b| b == o.second));
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
            // The units' reals.
            let (mut same, mut differ, mut first) = (0, 0, None);
            for (slot, f, sec) in self.unit_order.iter().flat_map(|&h| self.unit_real_records(h)).collect::<Vec<_>>() {
                let Some(o) = by_slot.get(&slot) else { continue };
                // (By the bytes written: -0 is not 0.)
                let ok1 = f.to_bytes() == o.first;
                let ok2 = sec.as_ref().is_none_or(|s| s.to_bytes(o.entry.kind).is_ok_and(|b| b == o.second));
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
            out.push(format!("units' reals: {same} the same, {differ} differ{}", first.map_or(String::new(), |f| format!("; first {f}"))));
            // The units' modifiers.
            let (mut same, mut differ, mut first) = (0, 0, None);
            for (slot, f, sec) in self.unit_order.iter().flat_map(|&h| self.unit_modifier_records(h)).collect::<Vec<_>>() {
                let Some(o) = by_slot.get(&slot) else { continue };
                // (By the bytes written: -0 is not 0.)
                let ok1 = f.to_bytes() == o.first;
                let ok2 = sec.as_ref().is_none_or(|s| s.to_bytes(o.entry.kind).is_ok_and(|b| b == o.second));
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
            out.push(format!("units' modifiers: {same} the same, {differ} differ{}", first.map_or(String::new(), |f| format!("; first {f}"))));
            // The units' listeners.
            let (mut same, mut differ, mut first) = (0, 0, None);
            for (slot, f, sec) in self.unit_order.iter().flat_map(|&h| self.unit_listener_records(h)).collect::<Vec<_>>() {
                let Some(o) = by_slot.get(&slot) else { continue };
                // (By the bytes written: -0 is not 0.)
                let ok1 = f.to_bytes() == o.first;
                let ok2 = sec.as_ref().is_none_or(|s| s.to_bytes(o.entry.kind).is_ok_and(|b| b == o.second));
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
            out.push(format!("units' listeners: {same} the same, {differ} differ{}", first.map_or(String::new(), |f| format!("; first {f}"))));
            // The units' positions.
            let (mut same, mut differ, mut first) = (0, 0, None);
            for (slot, f, sec) in self.unit_order.iter().flat_map(|&h| self.unit_position_records(h)).collect::<Vec<_>>() {
                let Some(o) = by_slot.get(&slot) else { continue };
                // (By the bytes written: -0 is not 0.)
                let ok1 = f.to_bytes() == o.first;
                let ok2 = sec.as_ref().is_none_or(|s| s.to_bytes(o.entry.kind).is_ok_and(|b| b == o.second));
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
            out.push(format!("units' positions: {same} the same, {differ} differ{}", first.map_or(String::new(), |f| format!("; first {f}"))));
            // The units' own records, by part.
            {
                let rallies = rally_points(orig);
                let (mut same, mut first) = (0, None);
                let mut parts: std::collections::BTreeMap<&str, u32> = Default::default();
                let mut fields_off: std::collections::BTreeMap<usize, (u32, String)> = Default::default();
                for &h in &self.unit_order {
                    let Some((slot, sec)) = self.unit_own_record(h, &rallies) else { continue };
                    let Some(o) = by_slot.get(&slot) else { continue };
                    if sec.to_bytes(o.entry.kind).is_ok_and(|b| b == o.second) {
                        same += 1;
                        continue;
                    }
                    let (Second::Unit { own, handlers, widget, lists, fields }, Ok(Second::Unit { own: own2, handlers: h2, widget: w2, lists: l2, fields: f2 })) = (&sec, o.second()) else { continue };
                    let mut differs = Vec::new();
                    if *own != own2 {
                        differs.push(("own", format!("{own:#x} vs {own2:#x}")));
                    }
                    if handlers.keyed != h2.keyed {
                        differs.push(("keyed", format!("{:?} vs {:?}", handlers.keyed, h2.keyed)));
                    }
                    if handlers.fixed != h2.fixed {
                        differs.push(("fixed", format!("{:?} vs {:?}", handlers.fixed, h2.fixed)));
                    }
                    if let Some(k) = (0..0x400).find(|&k| handlers.events.get(k) != h2.events.get(k)) {
                        differs.push(("events", format!("{k:#x}: {:?} vs {:?}", handlers.events.get(k), h2.events.get(k))));
                    }
                    let strip = |w: &Widget| Widget { sprite: None, shorts: [0; 2], ..w.clone() };
                    if strip(widget) != strip(&w2) {
                        differs.push(("widget", format!("{:?} vs {:?}", strip(widget), strip(&w2))));
                    }
                    // (What the local player sees aside: the animation's choice and time,
                    // the fog's tint.)
                    let seen = |w: &Widget| w.sprite.clone().map(|s| Sprite { real: 0.0, short: 0, word: 0, words: [0; 2], flags: [0; 3], entries: Vec::new(), ..s });
                    if seen(widget) != seen(&w2) {
                        differs.push(("sprite", format!("{:?} vs {:?}", seen(widget), seen(&w2))));
                    }
                    let stands = |w: &Widget| w.sprite.as_ref().and_then(|s| s.entries.first().map(|e| e.2.clone()));
                    if stands(widget) != stands(&w2) {
                        differs.push(("stands", format!("{:?} vs {:?}", stands(widget), stands(&w2))));
                    }
                    if *lists != l2 {
                        differs.push(("lists", format!("{lists:?} vs {l2:?}")));
                    }
                    let shown: Option<usize> = std::env::var("W3Z_FIELD").ok().and_then(|v| v.parse().ok());
                    for (k, (a, b)) in fields.iter().zip(f2.iter()).enumerate() {
                        if a != b {
                            if shown == Some(k) {
                                out.push(format!("unit slot {slot} {} field {k}: ours {a:?}, the original's {b:?}", String::from_utf8_lossy(&widget.word.to_be_bytes())));
                            }
                            let e = fields_off.entry(k).or_insert((0, format!("slot {slot}: {a:?} vs {b:?}")));
                            e.0 += 1;
                        }
                    }
                    for (p, d) in &differs {
                        *parts.entry(p).or_insert(0) += 1;
                        if std::env::var("W3Z_ALL").is_ok() {
                            out.push(format!("unit slot {slot} {p}: ours {d}"));
                        }
                    }
                    first.get_or_insert(format!("slot {slot}: {:?}", differs.iter().map(|d| d.0).collect::<Vec<_>>()));
                }
                out.push(format!("units' own records: {same} the same; parts differing {parts:?}; fields differing {:?}{}", fields_off.iter().map(|(k, v)| (k, v.0)).collect::<Vec<_>>(), first.map_or(String::new(), |f| format!("; first {f}"))));
                if std::env::var("W3Z_ALL").is_ok() {
                    for (k, (n, ex)) in &fields_off {
                        out.push(format!("unit field {k}: {n} differ, first {ex}"));
                    }
                }
            }
            // The units' abilities' agents.
            {
                let rallies = rally_points(orig);
                let (mut same, mut differ, mut first) = (0, 0, None);
                let mut kinds: std::collections::BTreeMap<String, u32> = Default::default();
                for &h in &self.unit_order {
                    for (slot, f) in self.ability_agent_records(h, &rallies) {
                        let Some(o) = by_slot.get(&slot) else { continue };
                        if f.to_bytes() == o.first {
                            same += 1;
                        } else {
                            differ += 1;
                            *kinds.entry(String::from_utf8_lossy(&o.entry.kind.to_be_bytes()).to_string()).or_insert(0) += 1;
                            let line = format!("slot {slot}: ours {f:?}, the original's {:?}", o.first());
                            if std::env::var("W3Z_ALL").is_ok() {
                                out.push(format!("ability {line}"));
                            }
                            first.get_or_insert(line);
                        }
                    }
                }
                out.push(format!("units' abilities' agents: {same} the same, {differ} differ {kinds:?}{}", first.map_or(String::new(), |f| format!("; first {f}"))));
                // Their own records, for the kinds the port writes.
                let (mut same, mut first) = (0, None);
                let mut kinds: std::collections::BTreeMap<String, (u32, u32)> = Default::default();
                let mut fields: std::collections::BTreeMap<String, (u32, String)> = Default::default();
                let art = client_art(orig);
                for &h in &self.unit_order {
                    let chain = self.ability_chain(h, &rallies);
                    let olders = self.chain_older(h, &chain);
                    for (k, &(t, code)) in chain.iter().enumerate() {
                        let older = olders[k];
                        let Some(sec) = self.ability_own_record(h, t, &code, older, &rallies, &art) else { continue };
                        let Some(o) = by_slot.get(&t.0) else { continue };
                        let e = kinds.entry(String::from_utf8_lossy(&code).to_string()).or_insert((0, 0));
                        if sec.to_bytes(o.entry.kind).is_ok_and(|b| b == o.second) {
                            same += 1;
                            e.0 += 1;
                        } else {
                            e.1 += 1;
                            let line = format!("slot {}: ours {sec:?}, the original's {:?}", t.0, o.second());
                            if std::env::var("W3Z_ALL").is_ok() {
                                out.push(format!("ability own {line}"));
                            }
                            if let Ok(theirs) = o.second() {
                                for d in second_diffs(&sec, &theirs) {
                                    let e = fields.entry(format!("{} {}", String::from_utf8_lossy(&code), d.0)).or_insert((0, String::new()));
                                    e.0 += 1;
                                    if e.1.is_empty() {
                                        e.1 = format!("slot {}: {}", t.0, d.1);
                                    }
                                }
                            }
                            first.get_or_insert(line);
                        }
                    }
                }
                out.push(format!("units' abilities' own records: {same} the same; by kind (same, differ) {kinds:?}{}", first.map_or(String::new(), |f| format!("; first {}", &f[..f.len().min(600)]))));
                for (k, (n, example)) in &fields {
                    out.push(format!("  ability field {k}: {n} differ, e.g., {example}"));
                }
                // Their orders' and tasks' records.
                let (mut same, mut differ) = (0, Vec::new());
                for &h in &self.unit_order {
                    for (slot, f, sec) in self.order_records(h) {
                        let Some(o) = by_slot.get(&slot) else { differ.push(format!("{slot} missing")); continue };
                        let first_ok = f.as_ref().is_none_or(|f| f.to_bytes() == o.first);
                        let second_ok = sec.to_bytes(o.entry.kind).is_ok_and(|b| b == o.second);
                        if first_ok && second_ok {
                            same += 1;
                        } else {
                            differ.push(format!("{slot} {}: ours {:?} {sec:?}, the original's {:?} {:?}", String::from_utf8_lossy(&o.entry.kind.to_be_bytes()), f, o.first(), o.second()));
                        }
                    }
                }
                out.push(format!("units' orders and tasks: {same} the same, {} differ{}", differ.len(), differ.iter().take(4).map(|d| format!("\n  {}", &d[..d.len().min(1500)])).collect::<String>()));
                // The units' AI lists.
                let (mut same, mut differ, mut first) = (0, 0, None);
                for (slot, sec) in self.unit_ref_records() {
                    let Some(o) = by_slot.get(&slot) else { continue };
                    if sec.to_bytes(o.entry.kind).is_ok_and(|b| b == o.second) {
                        same += 1;
                    } else {
                        differ += 1;
                        let words = |s: &Second| match s { Second::UnitRefs { words, .. } => *words, _ => [0; 4] };
                        first.get_or_insert(format!("slot {slot}: ours {:?}, the original's {:?}", words(&sec), o.second().ok().map(|s| words(&s))));
                    }
                }
                out.push(format!("units' AI lists: {same} the same, {differ} differ{}", first.map_or(String::new(), |f| format!("; first {f}"))));
                let (mut same, mut differ, mut first) = (0, 0, None);
                for (slot, sec) in self.unit_ai_records() {
                    let Some(o) = by_slot.get(&slot) else { continue };
                    if sec.to_bytes(o.entry.kind).is_ok_and(|b| b == o.second) {
                        same += 1;
                    } else {
                        differ += 1;
                        let own = |s: &Second| match s { Second::Agent { own, .. } => format!("{own:?}"), _ => String::new() };
                        let line = format!("slot {slot}: ours {}, the original's {:?}", own(&sec), o.second().ok().map(|s| own(&s)));
                        if std::env::var("W3Z_ALL").is_ok() {
                            out.push(format!("ai {line}"));
                        }
                        first.get_or_insert(line);
                    }
                }
                out.push(format!("units' AIs: {same} the same, {differ} differ{}", first.map_or(String::new(), |f| format!("; first {f}"))));
                // The units' regions, the search marks aside.
                let (mut same, mut marks, mut differ, mut first) = (0, 0, 0, None);
                for &h in &self.unit_order {
                    for (slot, f) in self.unit_region_records(h) {
                        let Some(o) = by_slot.get(&slot) else { continue };
                        if f.to_bytes() == o.first {
                            same += 1;
                            continue;
                        }
                        match o.first() {
                            Ok(First::Region { refs, word, bounds, words }) if (First::Region { refs, word, bounds, words: [0, words[1], words[2]] }) == f => marks += 1,
                            theirs => {
                                differ += 1;
                                let line = format!("slot {slot}: ours {f:?}, the original's {theirs:?}");
                                if std::env::var("W3Z_ALL").is_ok() {
                                    out.push(format!("region {line}"));
                                }
                                first.get_or_insert(line);
                            }
                        }
                    }
                }
                out.push(format!("units' regions: {same} the same, {marks} but for the search mark, {differ} differ{}", first.map_or(String::new(), |f| format!("; first {f}"))));
                let (mut same, mut differ, mut first) = (0, 0, None);
                for &h in &self.unit_order {
                    let Some((slot, f)) = self.unit_path_record(h) else { continue };
                    let Some(o) = by_slot.get(&slot) else { continue };
                    if f.to_bytes() == o.first {
                        same += 1;
                    } else {
                        differ += 1;
                        first.get_or_insert(format!("slot {slot}: ours {f:?}, the original's {:?}", o.first()));
                    }
                }
                out.push(format!("units' paths: {same} the same, {differ} differ{}", first.map_or(String::new(), |f| format!("; first {f}"))));
                let (mut same, mut differ, mut first) = (0, 0, None);
                for &h in &self.unit_order {
                    for (slot, f) in self.unit_pil_records(h) {
                        let Some(o) = by_slot.get(&slot) else { continue };
                        if f.to_bytes() == o.first {
                            same += 1;
                        } else {
                            differ += 1;
                            let line = format!("slot {slot}: ours {f:?}, the original's {:?}", o.first());
                            if std::env::var("W3Z_ALL").is_ok() {
                                out.push(format!("pil {line}"));
                            }
                            first.get_or_insert(line);
                        }
                    }
                }
                out.push(format!("units' in-range listeners: {same} the same, {differ} differ{}", first.map_or(String::new(), |f| format!("; first {f}"))));
            }
            // The accelerator map's grids.
            if let Ok(maps) = w3_formats::w3z::PathingMaps::parse(&m.data[0]) {
                let ours = self.saved_grids();
                for (k, g) in maps.grids.iter().enumerate() {
                    let Some(o) = ours.get(k) else { continue };
                    let differ = o.iter().zip(&g.cells).filter(|(a, b)| a != b).count();
                    let first = o.iter().zip(&g.cells).position(|(a, b)| a != b).map(|i| format!("; first cell {} ({}, {}): ours {:#x}, the original's {:#x}", i, i % g.frame.width as usize, i / g.frame.width as usize, o[i], g.cells[i]));
                    out.push(format!("grid {k}: {} cells, {} ours, {differ} differ{}", g.cells.len(), o.len(), first.unwrap_or_default()));
                }
            }
            // The agents with no properties, and the pathing system's objects.
            let rallies = rally_points(orig);
            let ability_reals: Vec<(u32, First)> = self.unit_order.iter().flat_map(|&h| self.ability_real_records(h, &rallies)).collect();
            let footprints: Vec<(u32, First)> = self.unit_order.iter().flat_map(|&h| self.structure_region_records(h)).collect();
            let circles: Vec<(u32, First)> = self.unit_order.iter().flat_map(|&h| self.mine_circle_records(h).0.into_iter().chain(self.blight_growth_records(h).0)).collect();
            {
                let by_slot: std::collections::HashMap<u32, &SavedObject> = objects[0].iter().map(|o| (o.entry.tag, o)).collect();
                let secs: Vec<(u32, Second)> = self.unit_order.iter().flat_map(|&h| self.mine_circle_records(h).1.into_iter().chain(self.blight_growth_records(h).1)).chain(self.plain_agent_seconds()).chain(self.player_agent_seconds(&client_listeners(orig), &rally_points(orig))).chain(self.player_ai_seconds()).collect();
                let (mut same, mut differ, mut first) = (0, 0, None);
                for (slot, sec) in secs {
                    match by_slot.get(&slot) {
                        Some(o) if sec.to_bytes(o.entry.kind).is_ok_and(|b| b == o.second) => same += 1,
                        Some(o) => {
                            differ += 1;
                            if std::env::var("W3Z_ALL").is_ok() {
                                if let (Second::Agent { own, handlers, tail }, Ok(Second::Agent { own: o2, handlers: h2, tail: t2 })) = (&sec, o.second()) {
                                    let where_ = if *own != o2 {
                                        format!("own {own:?} vs {o2:?}")
                                    } else if handlers.keyed != h2.keyed {
                                        format!("keyed {:?} vs {:?}", handlers.keyed, h2.keyed)
                                    } else if handlers.fixed != h2.fixed || handlers.events.len() != h2.events.len() {
                                        format!("fixed/events {:?} {} vs {:?} {}", handlers.fixed, handlers.events.len(), h2.fixed, h2.events.len())
                                    } else {
                                        // (By bits: a NaN equals itself.)
                                        let bits = |f: Option<&Field>| format!("{f:?}").replace("NaN", "nan");
                                        let k = (0..tail.len().max(t2.len())).find(|&k| bits(tail.get(k)) != bits(t2.get(k))).unwrap_or(0);
                                        if let (Some(Field::List(x)), Some(Field::List(y))) = (tail.get(k), t2.get(k)) {
                                            let j = (0..x.len().max(y.len())).find(|&j| format!("{:?}", x.get(j)) != format!("{:?}", y.get(j))).unwrap_or(0);
                                            out.push(format!("  slot {slot} tail {k} entry {j} of {} vs {}: {:?} vs {:?}", x.len(), y.len(), x.get(j), y.get(j)));
                                        }
                                        let show = |f: Option<&Field>| { let t = format!("{f:?}"); t[..t.len().min(240)].to_string() };
                                        format!("tail {k} ({} vs {}): {} vs {}", tail.len(), t2.len(), show(tail.get(k)), show(t2.get(k)))
                                    };
                                    out.push(format!("  slot {slot}: {where_}"));
                                    if let Ok(b) = sec.to_bytes(o.entry.kind) {
                                        if let Some(k) = b.iter().zip(&o.second).position(|(x, y)| x != y) {
                                            let hex = |v: &[u8]| v.iter().map(|x| format!("{x:02x}")).collect::<String>();
                                            out.push(format!("  slot {slot} first differing byte {k} of {} vs {}: {} vs {}", b.len(), o.second.len(), hex(&b[k.saturating_sub(8)..(k + 16).min(b.len())]), hex(&o.second[k.saturating_sub(8)..(k + 16).min(o.second.len())])));
                                        }
                                    }
                                }
                            }
                            first.get_or_insert(format!("slot {slot}: ours {sec:?}, the original's {:?}", o.second()));
                        }
                        None => differ += 1,
                    }
                }
                out.push(format!("own records (mines' slots, circles, blight growth, camps, sites, shifts, the queue, players): {same} the same, {differ} differ{}", first.map_or(String::new(), |f| format!("; first {f}"))));
            }
            // The camera's objects, in the second table.
            {
                let by_tag: std::collections::HashMap<u32, &SavedObject> = objects[1].iter().map(|o| (o.entry.tag, o)).collect();
                let (mut same, mut differ) = (0, Vec::new());
                for (e, f, sec) in self.camera_records() {
                    let Some(o) = by_tag.get(&e.tag) else { differ.push(format!("{} missing", e.tag)); continue };
                    let first_ok = f.to_bytes() == o.first;
                    let second_ok = sec.as_ref().is_none_or(|x| x.to_bytes(e.kind).is_ok_and(|b| b == o.second));
                    if first_ok && second_ok && o.entry == e {
                        same += 1;
                    } else {
                        let theirs = if first_ok { format!("{:?}", o.second()) } else { format!("{:?}", o.first()) };
                        let ours = if first_ok { format!("{sec:?}") } else { format!("{f:?}") };
                        differ.push(format!("{} ({}): ours {}, the original's {}", e.tag - 0x8000_0000, if first_ok { "second" } else { "first" }, &ours[..ours.len().min(900)], &theirs[..theirs.len().min(900)]));
                    }
                }
                out.push(format!("camera: {same} the same, {} differ{}", differ.len(), differ.iter().map(|d| format!("\n  {d}")).collect::<String>()));
            }
            for (name, recs) in [("plain agents", self.plain_agent_records()), ("pathing system objects", self.pathing_system_records(48, 0)), ("game object", self.game_object_records()), ("time of day's modifier and listeners", self.time_of_day_records()), ("abilities' reals", ability_reals), ("structures' footprints (but for the search mark)", footprints), ("mines' circles and blight growth (but for the search mark)", circles)] {
                let by_slot: std::collections::HashMap<u32, &SavedObject> = objects[0].iter().map(|o| (o.entry.tag, o)).collect();
                let (mut same, mut differ, mut first) = (0, 0, None);
                for (slot, f) in recs {
                    let unmarked = |f: &First| match f.clone() {
                        First::Region { refs, word, bounds, mut words } => {
                            words[0] = 0;
                            First::Region { refs, word, bounds, words }.to_bytes()
                        }
                        other => other.to_bytes(),
                    };
                    match by_slot.get(&slot) {
                        Some(o) if o.first().is_ok_and(|t| unmarked(&t) == unmarked(&f)) => same += 1,
                        Some(o) => {
                            differ += 1;
                            first.get_or_insert(format!("slot {slot} ({}): ours {f:?}, the original's {:?}", String::from_utf8_lossy(&o.entry.kind.to_be_bytes()), o.first()));
                        }
                        None => {
                            differ += 1;
                            first.get_or_insert(format!("slot {slot}: not in the original's"));
                        }
                    }
                }
                out.push(format!("{name}: {same} the same, {differ} differ{}", first.map_or(String::new(), |f| format!("; first {f}"))));
            }
            // The regions section whole.
            if let Ok(maps) = w3_formats::w3z::PathingMaps::parse(&m.data[0]) {
                match self.saved_pathing_maps([maps.proximity[0].words[0], maps.proximity[1].words[0]]) {
                    Some(ours) => {
                        let b = ours.to_bytes();
                        let first = b.iter().zip(&m.data[0]).position(|(x, y)| x != y);
                        out.push(format!("regions section: {} bytes ours, {} the original's, {}", b.len(), m.data[0].len(), first.map_or("the same".into(), |k| format!("first differing byte {k}"))));
                        if ours.refs != maps.refs || ours.tail != maps.tail || ours.proximity[0].frame != maps.proximity[0].frame || ours.proximity[1].frame != maps.proximity[1].frame {
                            out.push(format!("  refs {:?} {:?} vs {:?} {:?}; frames {:?} vs {:?}", ours.refs, ours.tail, maps.refs, maps.tail, ours.proximity.iter().map(|p| p.frame).collect::<Vec<_>>(), maps.proximity.iter().map(|p| p.frame).collect::<Vec<_>>()));
                        }
                    }
                    None => out.push("regions section: not made".into()),
                }
            }
            // The pathing map's cells.
            if let Ok(maps) = w3_formats::w3z::PathingMaps::parse(&m.data[0]) {
                let ours = self.saved_pathing_cells();
                let theirs = &maps.proximity[1].cells;
                let (mut flags_off, mut lists_off, mut sets_off) = (0, 0, 0);
                let (mut first_flag, mut first_list) = (None, None);
                for (a, b) in ours.iter().zip(theirs) {
                    if (a.0, a.1, a.2) != (b.0, b.1, b.2) {
                        flags_off += 1;
                        first_flag.get_or_insert(format!("({}, {}): ours {:#x}, the original's {:#x}", b.0, b.1, a.2, b.2));
                    }
                    if a.3 != b.3 {
                        lists_off += 1;
                        let mut x = a.3.clone();
                        let mut y = b.3.clone();
                        x.sort_by_key(|r| (r.0, r.1));
                        y.sort_by_key(|r| (r.0, r.1));
                        if x != y {
                            sets_off += 1;
                        }
                        first_list.get_or_insert(format!("({}, {}): ours {:?}, the original's {:?}", b.0, b.1, a.3, b.3));
                    }
                }
                if std::env::var("W3Z_FIND").is_ok() {
                    let mut extra: std::collections::BTreeMap<u32, u32> = Default::default();
                    let mut missing: std::collections::BTreeMap<u32, u32> = Default::default();
                    for (a, b) in ours.iter().zip(theirs) {
                        for r in &a.3 {
                            if !b.3.contains(r) {
                                *extra.entry(r.0).or_insert(0) += 1;
                            }
                        }
                        for r in &b.3 {
                            if !a.3.contains(r) {
                                *missing.entry(r.0).or_insert(0) += 1;
                            }
                        }
                    }
                    out.push(format!("cells: ours alone {extra:?}; the original's alone {missing:?}"));
                }
                out.push(format!("pathing cells: {} ours, {} the original's; flags differ {flags_off}{}; lists differ {lists_off} ({sets_off} in their members){}", ours.len(), theirs.len(), first_flag.map_or(String::new(), |f| format!(" first {f}")), first_list.map_or(String::new(), |f| format!(" first {f}"))));
            }
            // The units' agents.
            let (mut same, mut differ, mut first) = (0, 0, None);
            for (slot, f, sec) in self.unit_order.iter().flat_map(|&h| self.unit_agent_record(h)).collect::<Vec<_>>() {
                let Some(o) = by_slot.get(&slot) else { continue };
                // (By the bytes written: -0 is not 0.)
                let ok1 = f.to_bytes() == o.first;
                let ok2 = sec.as_ref().is_none_or(|s| s.to_bytes(o.entry.kind).is_ok_and(|b| b == o.second));
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
            out.push(format!("units' agents: {same} the same, {differ} differ{}", first.map_or(String::new(), |f| format!("; first {f}"))));
        }
        out.push(match free.chunks(4).zip(m.free[0].chunks(4)).position(|(a, b)| a != b) {
            None if free.len() == m.free[0].len() => format!("free list: the same, {} slots", free.len() / 4 - 1),
            None => format!("free list: the same as far as both go, ours {} the original's {}", free.len() / 4, m.free[0].len() / 4),
            Some(k) => format!("free list: parts at {k}"),
        });
        out
    }
}

/// Where two agents' second records differ, field by field: each place ("own 3", "tail
/// 2", "keyed", "events 0x1a2") and the two values.
fn second_diffs(ours: &Second, theirs: &Second) -> Vec<(String, String)> {
    let mut out = Vec::new();
    match (ours, theirs) {
        (Second::Agent { own: a, handlers: ha, tail: ta }, Second::Agent { own: b, handlers: hb, tail: tb }) => {
            for (name, x, y) in [("own", a, b), ("tail", ta, tb)] {
                for k in 0..x.len().max(y.len()) {
                    let (p, q) = (x.get(k).map(|f| format!("{f:?}")), y.get(k).map(|f| format!("{f:?}")));
                    if p != q {
                        out.push((format!("{name} {k}"), format!("ours {} vs {}", p.unwrap_or_default(), q.unwrap_or_default())));
                    }
                }
            }
            if format!("{:?}", ha.keyed) != format!("{:?}", hb.keyed) {
                out.push(("keyed".into(), format!("ours {:?} vs {:?}", ha.keyed, hb.keyed)));
            }
            if format!("{:?}", ha.fixed) != format!("{:?}", hb.fixed) {
                out.push(("fixed".into(), format!("ours {:?} vs {:?}", ha.fixed, hb.fixed)));
            }
            for k in 0..ha.events.len().max(hb.events.len()) {
                let (p, q) = (ha.events.get(k), hb.events.get(k));
                if format!("{p:?}") != format!("{q:?}") {
                    out.push((format!("events {:#x}", k), format!("ours {p:?} vs {q:?}")));
                }
            }
        }
        _ => out.push(("whole".into(), format!("ours {ours:?} vs {theirs:?}"))),
    }
    out
}

impl World {
    /// The object manager's clocks: the main clock's and the agent queue's, each with
    /// the times it has been set back, the span, and two words (read: Forestwalk's
    /// computer game saved at 450 seconds, main 150.0 and agent 150.66518, each set back
    /// once; at 300, the main 300.0 not yet).  The main clock as the port keeps it (its
    /// properties' dues reckoned from their makings' clocks); the agent queue's the
    /// game's time set back as often (its timers' dues reckoned on that time).
    /// Provisional: their last words, the queues' counts, 0.
    pub(crate) fn saved_clocks(&self, time: f32) -> [(f32, u32, f32, u32, u32); 2] {
        let wraps = self.tick_wraps.last().copied().unwrap_or(0);
        if wraps == 0 {
            return [(time, 0, 300.0, 4096, 0), (time, 0, 300.0, 0, 0)];
        }
        let mut agent = time.to_bits();
        for _ in 0..wraps {
            agent = crate::efloat::sub(agent, crate::motion::WRAP);
        }
        [(self.main_clock_now(), wraps, 300.0, 4096, 0), (f32::from_bits(agent), wraps, 300.0, 0, 0)]
    }
}

/// A path's way as saved: its array's room in bytes twice, 128, and its places, 128
/// doubled as needed (the original's grows as an in-range listener's does; more
/// points than its room would be read past it).
pub(crate) fn path_array(entries: Vec<[f32; 2]>) -> Array<[f32; 2]> {
    let mut room = 128u32;
    while (room as usize) < entries.len() {
        room *= 2;
    }
    Array { words: [room * 8, room * 8, 128, room], entries }
}

/// An in-range listener's units as saved: its array's room in bytes twice, its
/// growth, and its room, 32 places doubled as needed (read: [768, 768, 32, 64] in the
/// original's with more than 32 units met).  The original reads the units into that
/// room: more than it, as the port once wrote, overran its heap and hung its loader.
pub(crate) fn in_range_array(entries: Vec<[u32; 3]>) -> Array<[u32; 3]> {
    let mut room = 32u32;
    while (room as usize) < entries.len() {
        room *= 2;
    }
    Array { words: [room * 12, room * 12, 32, room], entries }
}

/// A due on the game's time set back `n` times by 300 seconds, as the engine's
/// subtraction does, where it is due by `from` or after.
fn set_back_timing(t: &mut Timing, n: u32, from: f32) {
    if let Some((due, ..)) = &mut t.0 {
        if *due >= from {
            for _ in 0..n {
                *due = f32::from_bits(crate::efloat::sub(due.to_bits(), crate::motion::WRAP));
            }
        }
    }
}

fn set_back_fields(fields: &mut [Field], n: u32, from: f32) {
    for f in fields {
        match f {
            Field::Timer(_, _, t) => set_back_timing(t, n, from),
            Field::List(l) => {
                for g in l {
                    set_back_fields(g, n, from);
                }
            }
            _ => {}
        }
    }
}

/// A first record's timings.
fn set_back_first(f: &mut First, wraps: u32, from: f32) {
    let object = match f {
        First::Flag { base, .. } | First::Real { base, .. } | First::Pos { base, .. } | First::Cluster { base, .. } => &mut base.object,
        First::Threshold { object, .. } | First::FlagListener { object, .. } | First::Interference { object, .. } | First::Continuous { object, .. } | First::Agent { object, .. } | First::AgentDef { object, .. } => object,
        _ => return,
    };
    for t in &mut object.timings {
        set_back_timing(t, wraps, from);
    }
}

fn set_back_second(sec: &mut Second, wraps: u32, from: f32) {
    match sec {
        Second::Agent { own, tail, .. } => {
            set_back_fields(own, wraps, from);
            set_back_fields(tail, wraps, from);
        }
        Second::Unit { fields, .. } | Second::Whole(fields) => set_back_fields(fields, wraps, from),
        _ => {}
    }
}
