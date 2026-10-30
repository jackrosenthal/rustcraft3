//! Saved games as the original writes them (.w3z, `w3_formats::w3z`), from the port's
//! state: built piece by piece, each checked against the original's save of the same
//! moment (`World::compare_save`).

use w3_formats::w3z::{SaveGame, Tagged};

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
        out.push(match free.chunks(4).zip(m.free[0].chunks(4)).position(|(a, b)| a != b) {
            None if free.len() == m.free[0].len() => format!("free list: the same, {} slots", free.len() / 4 - 1),
            None => format!("free list: the same as far as both go, ours {} the original's {}", free.len() / 4, m.free[0].len() / 4),
            Some(k) => format!("free list: parts at {k}"),
        });
        out
    }
}
