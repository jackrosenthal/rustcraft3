//! Saved games as the original writes them (.w3z, `w3_formats::w3z`), from the port's
//! state: built piece by piece, each checked against the original's save of the same
//! moment (`World::compare_save`).

use w3_formats::w3z::{Array, First, Handlers, Link, ObjectBase, PropertyBase, Ref, SaveGame, Second, SavedObject, Sprite, Tagged, Timing, Widget};

use crate::types::Obj;

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

impl World {
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
        let game = self.tags.live.iter().find(|(_, (c, _))| c == b"+gam").map_or((0, 0), |(&s, (_, b))| (s, *b));
        let mut fixed: [Vec<(Ref, u32)>; 5] = Default::default();
        fixed[0].push((Ref(game.0, game.1), 0x8_0267));
        let handlers = Handlers { keyed: Vec::new(), fixed, events: vec![Vec::new(); 0x400] };
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
        }
        out.push(match free.chunks(4).zip(m.free[0].chunks(4)).position(|(a, b)| a != b) {
            None if free.len() == m.free[0].len() => format!("free list: the same, {} slots", free.len() / 4 - 1),
            None => format!("free list: the same as far as both go, ours {} the original's {}", free.len() / 4, m.free[0].len() / 4),
            Some(k) => format!("free list: parts at {k}"),
        });
        out
    }
}
