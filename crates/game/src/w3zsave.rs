//! Saved games as the original writes them (.w3z, `w3_formats::w3z`), from the port's
//! state: built piece by piece, each checked against the original's save of the same
//! moment (`World::compare_save`).

use w3_formats::w3z::{Array, First, Link, ObjectBase, Ref, SaveGame, SavedObject, Tagged, Timing};

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

impl World {
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
        out.push(match free.chunks(4).zip(m.free[0].chunks(4)).position(|(a, b)| a != b) {
            None if free.len() == m.free[0].len() => format!("free list: the same, {} slots", free.len() / 4 - 1),
            None => format!("free list: the same as far as both go, ours {} the original's {}", free.len() / 4, m.free[0].len() / 4),
            Some(k) => format!("free list: parts at {k}"),
        });
        out
    }
}
