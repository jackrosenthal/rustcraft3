//! The original's tech tree registrations, as the game module of its checksum reads
//! them: each player's CTechTree (CPlayerWar3 +0x2d0) holds a registration per type it
//! has touched (TTechTreeReg, keyed by the type's code), in the order first touched
//! (`FUN_6f22f5c0` makes one on first use).  SetPlayerTechMaxAllowed sets its limit
//! (`FUN_6f2498a0`); a unit made finished adds to it (`FUN_6f227b40`); queueing to train
//! or research, or placing a structure, starts one (`FUN_6f249cf0`), which then
//! finishes (`FUN_6f22f6a0`) or is cancelled (`FUN_6f228ed0`); a unit removed takes
//! itself back (`FUN_6f23b030`); and a kill counts for the killer's owner
//! (`FUN_6f23b060`).  The port keeps the same counts in the same order so as to give
//! the original's hash (`FUN_6f2292c0`, `FUN_6f229360`).

use serde::{Deserialize, Serialize};

use crate::objects::Id;
use crate::world::World;

/// One type's registration (+0x18 to +0x30 of TTechTreeReg).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TechReg {
    pub id: Id,
    /// SetPlayerTechMaxAllowed's limit (+0x18), `i32::MAX` until set.
    pub max: i32,
    /// Made finished (+0x1c): never taken back.
    pub made: i32,
    /// In production (+0x20): queued to train or research, or being built.
    pub producing: i32,
    /// In production the other way (+0x24): a start of the second kind (a change of type).
    pub changing: i32,
    /// Present (+0x28): made or finished, and not yet removed.
    pub present: i32,
    /// Killed by the player's units (+0x2c).
    pub killed: i32,
    /// What the type is (+0x30): 2 research (`FUN_6f6ca5a0`), 4 a structure (the unit
    /// data's +0xb4, `FUN_6f34b6c0`), 8 another unit (`FUN_6f68c360`), 0 neither ('HERO').
    pub flags: u32,
}

impl TechReg {
    /// Its part of the tech tree's hash (`FUN_6f229360`).
    pub fn hash(&self) -> u32 {
        let pair = |a: i32, b: i32| (a ^ b) as u32;
        let mut x = pair(self.producing, self.made).rotate_left(3) ^ pair(self.present, self.max).rotate_left(3);
        x = x.rotate_left(3) ^ pair(self.killed, self.changing).rotate_left(3);
        x = x.rotate_left(3) ^ self.flags;
        x.rotate_left(3)
    }
}

impl World {
    /// `owner`'s registration of `id`, made (last in its order) if it has none.
    pub fn tech_reg(&mut self, owner: u8, id: Id) -> &mut TechReg {
        let flags = if self.objects.upgrades.get(id).is_some() {
            2
        } else if let Some(o) = self.objects.units.get(id) {
            if o.int("isbldg") != 0 { 4 } else { 8 }
        } else {
            0
        };
        let regs = &mut self.players[owner as usize].tech_regs;
        let k = match regs.iter().position(|r| r.id == id) {
            Some(k) => k,
            None => {
                regs.push(TechReg { id, max: i32::MAX, made: 0, producing: 0, changing: 0, present: 0, killed: 0, flags });
                regs.len() - 1
            }
        };
        &mut regs[k]
    }

    /// A unit of `owner`'s made finished (`FUN_6f227b40`, kind 0).
    pub fn tech_add(&mut self, owner: u8, id: Id) {
        let r = self.tech_reg(owner, id);
        r.present += 1;
        r.made += 1;
    }

    /// `owner`'s tech tree hash (`FUN_6f2292c0`): its registrations in order, `t =
    /// rol(t ^ reg, 3)` (its second list, at +0x38, empty in the games read).
    pub fn tech_hash(&self, owner: u8) -> u32 {
        self.players.get(owner as usize).map_or(0, |p| p.tech_regs.iter().fold(0u32, |t, r| (t ^ r.hash()).rotate_left(3)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reg(max: i32, made: i32, present: i32, flags: u32) -> TechReg {
        TechReg { id: *b"xxxx", max, made, producing: 0, changing: 0, present, killed: 0, flags }
    }

    /// The registrations the original had at Booty Bay's first keepalive (read under
    /// winedbg at `FUN_6f229360`): an empty slot's 28 limits, and a playing slot's with
    /// its town hall and five workers, hashing as the original's did.
    #[test]
    fn booty_bay_tech_trees_hash_as_the_originals() {
        let mut limits = vec![reg(0, 0, 0, 8), reg(0, 0, 0, 8), reg(25, 0, 0, 8), reg(3, 0, 0, 0)];
        limits.extend((0..24).map(|_| reg(1, 0, 0, 8)));
        let hash = |regs: &[TechReg]| regs.iter().fold(0u32, |t, r| (t ^ r.hash()).rotate_left(3));
        assert_eq!(hash(&limits), 0x6102_1200);
        let mut playing = limits.clone();
        playing.push(reg(i32::MAX, 1, 1, 4));
        playing.push(reg(i32::MAX, 5, 5, 8));
        assert_eq!(hash(&playing), 0x4086_ca18);
    }
}
