//! Spell books (CAbilitySpellBook, Aspb), the unit's own or a carried item's.  When the
//! unit gets a book, it chooses spells from DataA, at most eleven: between DataC and
//! DataD of them (DataC and one die of the difference), each picked by a die among
//! those left, or the list in turn when the count is fixed (FUN_6f501270).  The unit has
//! the chosen spells while it has the book; they show on the book's own page, in the
//! order chosen from the top left, beside its Cancel.

use crate::objects::{Id, id_from_str};
use crate::world::World;

/// The most spells a book holds.
const MOST: usize = 11;

impl World {
    /// The spell books a unit has: its own and its carried items'.
    pub fn spell_books(&self, h: u32) -> Vec<(Id, u32)> {
        let Some(u) = self.unit(h) else { return Vec::new() };
        let mut out: Vec<(Id, u32)> = u.abilities.iter().filter(|(a, _)| self.ability_code(*a) == "Aspb").map(|&(a, l)| (a, l.max(1) as u32)).collect();
        for &it in &u.inventory {
            let Some(t) = self.item(it).map(|i| i.type_id) else { continue };
            for a in self.item_abilities(t) {
                if self.ability_code(a) == "Aspb" && !out.iter().any(|b| b.0 == a) {
                    out.push((a, 1));
                }
            }
        }
        out
    }

    /// The book an ability of a unit is in.
    pub fn book_of(&self, h: u32, ability: Id) -> Option<Id> {
        self.unit(h)?.extra.books.iter().find(|b| b.1.contains(&ability)).map(|b| b.0)
    }

    /// Gives a unit the spells of books it has newly got, and takes away those of books
    /// it no longer has.
    pub fn sync_spell_books(&mut self, h: u32) {
        let have = self.spell_books(h);
        let Some(u) = self.unit(h) else { return };
        let gone: Vec<(Id, Vec<Id>)> = u.extra.books.iter().filter(|b| !have.iter().any(|x| x.0 == b.0)).cloned().collect();
        let new: Vec<(Id, u32)> = have.into_iter().filter(|x| !u.extra.books.iter().any(|b| b.0 == x.0)).collect();
        for (book, spells) in gone {
            if let Some(u) = self.unit_mut(h) {
                u.extra.books.retain(|b| b.0 != book);
                let kept: Vec<Id> = u.extra.books.iter().flat_map(|b| b.1.clone()).collect();
                u.abilities.retain(|(a, _)| !spells.contains(a) || kept.contains(a));
            }
        }
        for (book, level) in new {
            let spells = self.choose_spells(book, level);
            if let Some(u) = self.unit_mut(h) {
                for &s in &spells {
                    if !u.abilities.iter().any(|x| x.0 == s) {
                        u.abilities.push((s, 1));
                    }
                }
                u.extra.books.push((book, spells));
            }
        }
    }

    fn choose_spells(&mut self, book: Id, level: u32) -> Vec<Id> {
        let Some(o) = self.objects.abilities.get(book) else { return Vec::new() };
        let mut list: Vec<Id> = o
            .level_str("DataA", level)
            .split([' ', ',', '-', '_', '\t'])
            .filter(|s| !s.is_empty())
            .map(|s| id_from_str(s.trim()))
            .filter(|&a| self.objects.abilities.get(a).is_some())
            .take(MOST)
            .collect();
        let least = self.data(book, "DataC", level).max(0.0) as usize;
        let most = (self.data(book, "DataD", level).max(0.0) as usize).min(MOST).min(list.len());
        let fixed = least >= most;
        let count = if fixed { most } else { least + self.rng.int(1, (most - least) as i32) as usize };
        let mut out = Vec::new();
        for k in 0..count.min(list.len()) {
            let left = list.len();
            let i = if fixed { 0 } else { self.rng.int(1, left as i32) as usize - 1 };
            out.push(list[i]);
            // The last one left takes the chosen one's place.
            list.swap_remove(i);
            let _ = k;
        }
        out
    }
}
