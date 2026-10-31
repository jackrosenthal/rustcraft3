//! Saved games as the original writes them (.w3z, `w3_formats::w3z`), from the port's
//! state: built piece by piece, each checked against the original's save of the same
//! moment (`World::compare_save`).

use w3_formats::w3z::{Array, Field, First, Handlers, Link, ObjectBase, PropertyBase, Ref, SaveGame, Second, SavedObject, Sprite, Tagged, Timing, Widget};

use crate::types::{BoolExpr, EventKind, Obj};

use crate::tags::Code;
use crate::world::World;

mod unit;

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

/// The rally points (`rlyp`, the second table's agents), each with the structure it
/// serves (its first link's owner), as a save holds them.
pub fn rally_points(save: &SaveGame) -> Vec<(Ref, Ref)> {
    let Some(m) = save.objects.as_ref() else { return Vec::new() };
    let Ok(objects) = m.objects() else { return Vec::new() };
    objects[1]
        .iter()
        .filter(|o| &o.entry.kind.to_be_bytes() == b"rlyp")
        .filter_map(|o| match o.first() {
            Ok(First::Agent { object, .. }) => Some((Ref(o.entry.tag, o.entry.birth), object.links[0].3)),
            _ => None,
        })
        .collect()
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
        let code_of = |s: u32| self.tags.live.get(&s).map(|e| e.0);
        let mods: Vec<(u32, u32)> = u.extra.object_slots[1].clone();
        let rcms: Vec<(u32, u32)> = mods.iter().copied().filter(|&(s, _)| code_of(s) == Some(*b"`rcm")).collect();
        let rtl = mods.iter().copied().find(|&(s, _)| code_of(s) == Some(*b"`rtl"));
        let (hp_rate, mp_rate) = self.regen_rates(h).unwrap_or((0.0, 0.0));
        let regen_life = matches!(o.str("regenType").to_ascii_lowercase().as_str(), "always" | "night" | "day");
        // (Regeneration on blight: its `` `rcm `` made as the unit stands on blight, the
        // life stamped then; read, the undead's at 2 seconds.)
        let blight_rcm = u.extra.blight_rcm.filter(|s| self.tags.live.contains_key(s)).map(|s| self.tagged(s));
        let life_rcm = if regen_life { rcms.first().copied() } else { blight_rcm };
        let mana_rcm = if o.real("manaN") > 0.0 { rcms.get(usize::from(regen_life)).copied() } else { None };
        let night = self.night_sight;
        let sight = if self.ultravision(u) { o.real("sight") } else { o.real("sight") + (o.real("nsight") - o.real("sight")) * night };
        // Mana regenerating is kept as its value at its stamp (read: 0 for the creeps'
        // mana at the start, their rates beside it; provisional: the port's mana, a
        // tick's regeneration ahead, taken back by its rate since then).
        let mana_stamp = self.tags.made_at.get(&props[2].0).map_or(0.0, |&c| f32::from_bits(c));
        let mana = if mp_rate > 0.0 && u.mana < u.max_mana {
            let base = u.mana - mp_rate * (self.time - mana_stamp);
            if base.abs() < mp_rate * 0.05 { 0.0 } else { base.max(0.0) }
        } else {
            u.mana
        };
        // (Its newest listener on mana: a creep's made again as it woke, else its
        // spells' own.)
        let mana_heard = u.extra.mana_listeners.iter().map(|&(_, s)| s).chain(u.extra.spell_timers.iter().copied()).filter(|s| self.tags.live.contains_key(s)).max_by_key(|s| self.tagged(*s).1);
        // (Mana without a pool is -0, read: every unit without mana's.)
        let mana = if u.max_mana == 0.0 && mana == 0.0 { -0.0 } else { mana };
        let values: [[f32; 4]; 5] = [
            [u.life, hp_rate, 0.0, u.max_life],
            [mana, mp_rate, 0.0, u.max_mana],
            [-0.0, 0.0, 0.0, 2048.0],
            [-0.0, 0.0, 0.0, 1.0],
            // (A sight of none never changed is -0, as made: read, the haunted mine's
            // hidden gold mine, made after the day's change set the others' to 0.)
            [if sight == 0.0 && u.extra.sight_stamp.is_none() { -0.0 } else { sight }, 0.0, -f32::MAX, f32::MAX],
        ];
        for k in 1..=5 {
            let mut base = self.stamped(listed_property_base(&props, k, me));
            if k == 1 {
                if let Some(r) = life_rcm.filter(|_| !regen_life) {
                    base.real = self.tags.made_at.get(&r.0).map_or(0.0, |&c| f32::from_bits(c));
                }
            }
            match k {
                1 => base.links = [life_rcm.map_or(Link(NONE, 0), |r| Link(rf(r), 1)), rtl.map_or(Link(NONE, 0), |r| Link(rf(r), 1))],
                2 => base.links = [mana_rcm.map_or(Link(NONE, 0), |r| Link(rf(r), 1)), mana_heard.map_or(Link(NONE, 0), |s| Link(rf(self.tagged(s)), 1))],
                5 => base.real = u.extra.sight_stamp.unwrap_or(0.0),
                _ => {}
            }
            out.push((props[k].0, First::Real { base, values: values[k - 1] }, None));
        }
        out
    }

    /// A unit's continuous modifiers (`` `rcm ``, read against the quiet test map's): its
    /// life's and mana's regeneration (and on blight its life's), each two words
    /// (-1, 0x60050044), its rate and no limit, a timing node due 150 seconds after it
    /// was made, every 150, with the count it took (`Tags::prop_counts`); chained on
    /// the unit newest first, and on its real.
    pub fn unit_modifier_records(&self, h: u32) -> Vec<(u32, First, Option<Second>)> {
        let mut out = Vec::new();
        let Some(u) = self.unit(h) else { return out };
        let Some(o) = self.objects.units.get(u.type_id) else { return out };
        let first = &u.extra.object_slots[0];
        if first.len() < 10 {
            return out;
        }
        let me = first[0];
        let (life, mana) = (first[4], first[5]);
        let live = |s: u32| self.tags.live.get(&s).is_some_and(|e| &e.0 == b"`rcm");
        let rcms: Vec<(u32, u32)> = u.extra.object_slots[1].iter().copied().filter(|&(s, _)| live(s)).collect();
        let (hp_rate, mp_rate) = self.regen_rates(h).unwrap_or((0.0, 0.0));
        let regen_life = matches!(o.str("regenType").to_ascii_lowercase().as_str(), "always" | "night" | "day");
        // (Each with its real and rate.)
        let mut mods: Vec<((u32, u32), (u32, u32), f32)> = Vec::new();
        let mut k = 0;
        if regen_life {
            if let Some(&r) = rcms.get(k) {
                mods.push((r, life, hp_rate));
                k += 1;
            }
        }
        if o.real("manaN") > 0.0 {
            if let Some(&r) = rcms.get(k) {
                mods.push((r, mana, mp_rate));
            }
        }
        if let Some(s) = u.extra.blight_rcm.filter(|&s| live(s)) {
            mods.push((self.tagged(s), life, hp_rate));
        }
        mods.sort_by_key(|m| m.0 .1);
        let chain: Vec<(u32, u32)> = mods.iter().map(|m| m.0).collect();
        for &(r, prop, rate) in &mods {
            let (after, before) = chain_links(&chain, r, 0);
            let made = self.tags.made_at.get(&r.0).copied().unwrap_or(0);
            let due = f32::from_bits(crate::efloat::add(made, 150f32.to_bits()));
            let count = self.tags.prop_counts.get(&r.0).copied().unwrap_or(0);
            let object = ObjectBase {
                real: 0.0,
                words: [u32::MAX, 0x6005_0044],
                links: [(after, before, rf(r), rf(me)), (Link(NONE, 0), Link(NONE, 0), rf(r), rf(prop))],
                timings: [Timing(Some((due, 150.0, count, 0, 1))), Timing(None)],
            };
            out.push((r.0, First::Continuous { object, values: [rate, f32::MAX] }, None));
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
        let (life, mana) = (first[4], first[5]);
        let is_rtl = |s: u32| self.tags.live.get(&s).is_some_and(|e| &e.0 == b"`rtl");
        let death = u.extra.object_slots[1].iter().copied().find(|&(s, _)| is_rtl(s));
        let (_, mp_rate) = self.regen_rates(h).unwrap_or((0.0, 0.0));
        // (Each: itself, its real, event, threshold, rising, and its node.)
        let mut ls: Vec<((u32, u32), (u32, u32), u32, f32, u32, Timing)> = Vec::new();
        if let Some(d) = death {
            ls.push((d, life, 0xd_019f, 0.4, 0, Timing(None)));
        }
        let woken: Vec<(f32, u32)> = u.extra.mana_listeners.iter().copied().filter(|&(_, s)| is_rtl(s)).collect();
        for &s in u.extra.spell_timers.iter().filter(|&&s| is_rtl(s) && !woken.iter().any(|w| w.1 == s)) {
            ls.push((self.tagged(s), mana, 0xd_01b7, -0.1, 0, Timing(None)));
        }
        for &(thr, s) in &woken {
            let me_s = self.tagged(s);
            let made = self.tags.made_at.get(&s).map_or(0.0, |&c| f32::from_bits(c));
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
        // (The unit's chain holds its in-range listeners too, its abilities' and its
        // AI's, by their makings.)
        let is_pil = |s: u32| self.tags.live.get(&s).is_some_and(|e| &e.0 == b"`pil");
        let mut on_unit: Vec<(u32, u32)> = ls.iter().map(|l| l.0).collect();
        let pils = u.extra.object_slots[1].iter().chain(&u.extra.object_slots[2]).map(|&(s, _)| s).chain(u.extra.flyer_pil).chain(u.extra.item_pil.map(|p| p.0)).chain(u.extra.ensnare_pil.map(|p| p.1)).chain(u.extra.alarm_pil);
        for s in pils {
            if is_pil(s) && !on_unit.iter().any(|x| x.0 == s) {
                on_unit.push(self.tagged(s));
            }
        }
        on_unit.sort_by_key(|x| x.1);
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
        let mut rec = agent_first(me, b"+w3u", &props);
        let live = |s: u32, c: &[u8; 4]| self.tags.live.get(&s).is_some_and(|e| &e.0 == c);
        let all = || u.extra.object_slots[1].iter().chain(&u.extra.object_slots[2]).map(|&(s, _)| s).chain(u.extra.flyer_pil).chain(u.extra.item_pil.map(|p| p.0)).chain(u.extra.ensnare_pil.map(|p| p.1)).chain(u.extra.alarm_pil).chain(u.extra.spell_timers.iter().copied()).chain(u.extra.mana_listeners.iter().map(|l| l.1)).chain(u.extra.blight_rcm);
        // (Not a kept slot another object has taken since: read, the town hall's
        // upgrade's modifiers, gone, their slots the acolytes' blight's.)
        let stale = |s: u32| u.extra.object_slots.iter().flatten().any(|&(x, b)| x == s && self.tagged(s).1 != b);
        let newest = |codes: &[&[u8; 4]]| all().filter(|&s| !stale(s) && codes.iter().any(|c| live(s, c))).max_by_key(|&s| self.tagged(s).1).map(|s| Link(rf(self.tagged(s)), 0));
        if let First::Agent { links, .. } = &mut rec {
            // (Its abilities' chain, newest first: its abilities and an AI's unit list,
            // not the AI itself.)
            // (Buffs among them, a buff being an ability: read, the aura scans' at 2
            // seconds.)
            let ability = |s: u32| self.tags.live.get(&s).is_some_and(|e| (e.0[0] == b'A' && &e.0 != b"AI-U") || e.0[0] == b'B' || &e.0 == b"uref");
            let newest_ability = u.extra.object_slots[2].iter().map(|&(s, _)| s).chain(u.extra.ai_slots.iter().map(|&(s, _)| s)).chain(u.buffs.iter().map(|b| b.slot).filter(|&s| s != 0)).filter(|&s| ability(s)).max_by_key(|&s| self.tagged(s).1);
            if let Some(s) = newest_ability {
                links[0] = Link(rf(self.tagged(s)), 0);
            }
            links[2] = newest(&[b"`rcm"]).unwrap_or(Link(NONE, 0));
            links[3] = newest(&[b"`rtl", b"`pil"]).unwrap_or(Link(NONE, 0));
        }
        vec![(me.0, rec, None)]
    }

    /// A unit's clustered position (`` `pcu ``, read against the quiet test map's): its
    /// place in pathing cells, two zeros, its speed in cells a second (none moving
    /// unbounded), facing in radians, and collision in cells; its two regions; two
    /// words of -1 and one of 0; 0.8, its speed again (a structure's 0.75), its turn
    /// rate (a structure's 0.001), pi / 3, two zeros, its facing, and 0; its cells; a
    /// flag word (not yet read: 0 written); and its path.  Its property's second head
    /// is its newest in-range listener.
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
        let is_pil = |s: u32| self.tags.live.get(&s).is_some_and(|e| &e.0 == b"`pil");
        let newest = u.extra.object_slots[1].iter().chain(&u.extra.object_slots[2]).map(|&(s, _)| s).chain(u.extra.flyer_pil).chain(u.extra.item_pil.map(|p| p.0)).chain(u.extra.ensnare_pil.map(|p| p.1)).chain(u.extra.alarm_pil).filter(|&s| is_pil(s)).max_by_key(|&s| self.tagged(s).1);
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
        let speed = o.real("spd");
        let moving = speed > 0.0;
        let cells_speed = if moving { speed / 32.0 } else { f32::MAX };
        let turn = if moving { o.real("turnRate") } else { 0.001 };
        let values = [cx, cy, 0.0, 0.0, cells_speed, facing, o.real("collision") / 32.0];
        let reals = [0.8, if moving { speed / 32.0 } else { 0.75 }, turn, 1.047_197_5, 0.0, 0.0, facing, 0.0];
        let first_rec = First::Cluster {
            base,
            values,
            refs: [rf(first[2]), rf(first[3])],
            words: [u32::MAX, u32::MAX, 0],
            reals,
            more: [cx as u32, cy as u32, 0],
            flags: [0, 0],
            last: rf(first[9]),
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
                        out.push((flags[k].0, First::Flag { base: self.stamped(listed_property_base(&flags, k, own)), value: v }, None));
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

    /// A property's base with its time stamp: the main clock at its making.
    fn stamped(&self, mut base: PropertyBase) -> PropertyBase {
        let slot = base.object.links[0].2 .0;
        base.real = self.tags.made_at.get(&slot).map_or(0.0, |&c| f32::from_bits(c));
        base
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
                    out.push((own.0, agent_first(own, b"+frc", &flags), Some(Second::Agent { own: vec![Field::Word(0)], handlers: game_handlers(self.game_object()), tail })));
                }
                // A timer (CTimerWar3, read against the quiet test map's): its own word,
                // timeout, and a real; its agent timer (CAgentTimerExtended,
                // `FUN_6f0602a0`): while running itself, its flags, and its block (due
                // on the clock its run started by, its period, its start's count,
                // event 0x80204, a periodic node); the count of 120-second chunks a
                // timeout of 120 or more runs in (DAT_6fd3c894, set to 120 at load:
                // such a timer runs as a periodic node of 120, flag 0x10) twice; the
                // timeout beyond the chunks; its function's handler, and the
                // registrations of its expiry.  (Provisional: a periodic one's flag 2,
                // and the shorts as chunks pass.)
                Some(Obj::Timer(t)) => {
                    let Some(own) = slot_of(id) else { continue };
                    const CHUNK: f32 = 120.0;
                    let chunks = (t.timeout / CHUNK) as u32;
                    let rest = if chunks > 0 { t.timeout - chunks as f32 * CHUNK } else { t.timeout };
                    let flags = if chunks > 0 { 0x10 } else { 0 } | if t.periodic { 2 } else { 0 };
                    let node_periodic = u32::from(chunks > 0 || t.periodic);
                    let agent = if t.running {
                        let first = if chunks > 0 { CHUNK } else { t.timeout };
                        let due = f32::from_bits(crate::efloat::add(t.started_clock, first.to_bits()));
                        Field::Timer(rf(own), flags, Timing(Some((due, t.timeout, t.agent_seq, 0x8_0204, node_periodic))))
                    } else {
                        Field::Timer(NONE, 0, Timing(None))
                    };
                    let periodic = chunks.min(0xffff);
                    let handler = t.handler.map_or(NONE, |s| rf(self.tagged(s)));
                    let expiries = self
                        .events
                        .iter()
                        .filter(|&&e| matches!(self.handles.get(e), Some(Obj::Event(r)) if matches!(r.kind, EventKind::TimerExpire(x) if x == id)))
                        .filter_map(|e| self.tags.handle_ids.get(e))
                        .map(|&r| ((r.0, r.1), 0x8_0204))
                        .collect();
                    let tail = vec![agent, Field::Short(periodic as u16), Field::Short(periodic as u16), Field::Real(rest), Field::Ref(handler), handler_list(expiries)];
                    out.push((own.0, agent_first(own, b"+tmr", &[]), Some(Second::Agent { own: vec![Field::Word(0), Field::Real(t.timeout), Field::Real(0.0)], handlers: game_handlers(self.game_object()), tail })));
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
        let unit_reals: Vec<(u32, First, Option<Second>)> = self.unit_order.iter().flat_map(|&h| self.unit_real_records(h).into_iter().chain(self.unit_modifier_records(h)).chain(self.unit_listener_records(h)).chain(self.unit_position_records(h)).chain(self.unit_agent_record(h))).collect();
        for (slot, f, sec) in self.trigger_records().into_iter().chain(self.script_object_records()).chain(self.player_records(&client_listeners(template))).chain(unit_reals) {
            records.insert(slot, f);
            if let Some(sec) = sec {
                seconds.insert(slot, sec);
            }
        }
        // The units' own records (their second table's rally points as the template has
        // them).
        let rallies = rally_points(template);
        for &h in &self.unit_order {
            if let Some((slot, sec)) = self.unit_own_record(h, &rallies) {
                seconds.insert(slot, sec);
            }
            // Its abilities' agents, and the own records of the kinds written.
            // (The AI lists' after.)
            records.extend(self.ability_agent_records(h, &rallies));
            let chain = self.ability_chain(h, &rallies);
            for (k, &(t, code)) in chain.iter().enumerate() {
                let older = chain[..k].iter().rev().find(|c| c.1[0] == b'A').map(|c| c.0);
                if let Some(sec) = self.ability_own_record(h, t, &code, older) {
                    seconds.insert(t.0, sec);
                }
            }
        }
        for (slot, sec) in self.unit_ref_records().into_iter().chain(self.unit_ai_records()) {
            seconds.insert(slot, sec);
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
                for &h in &self.unit_order {
                    let chain = self.ability_chain(h, &rallies);
                    for (k, &(t, code)) in chain.iter().enumerate() {
                        // (The ability made before it, not a buff.)
                        let older = chain[..k].iter().rev().find(|c| c.1[0] == b'A').map(|c| c.0);
                        let Some(sec) = self.ability_own_record(h, t, &code, older) else { continue };
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
                            first.get_or_insert(line);
                        }
                    }
                }
                out.push(format!("units' abilities' own records: {same} the same; by kind (same, differ) {kinds:?}{}", first.map_or(String::new(), |f| format!("; first {}", &f[..f.len().min(600)]))));
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
