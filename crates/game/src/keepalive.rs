//! The keepalive checksum each player sends the host after a turn (`FUN_6f330830`), by
//! which the original finds games that have parted.
//!
//! It is made before turn n runs, from the state after turn n - 1: `t ^ rol(m ^ 0x17ab,
//! 3)`, then rotated by 3 and xored with the next tag (presence), the birth count, and
//! the byte hash of the main generator's value and taps (`FUN_6f330910`), and rotated
//! by 3 once more.  `t` is the game time in milliseconds (the game's +0x1c70), `m` the
//! game state's modules combined from last to first, `m = rol(m ^ module, 3)`, an empty
//! one skipped (`FUN_6f042dd0`).  Read: with these, the port's state gives the
//! original's keepalives for Booty Bay's turns 1 to 431 exactly
//! (`docs/reference/bootybay_keepalives.txt`).

use crate::world::World;

/// `b * 0x7a2d ^ b` summed over a word's four bytes.
pub fn byte_hash(v: u32) -> u32 {
    v.to_le_bytes().iter().fold(0u32, |s, &b| s.wrapping_add((b as u32).wrapping_mul(0x7a2d) ^ b as u32))
}

/// A network message's payload hash (`FUN_6f328550`): its words, then its last bytes,
/// each xored in and rotated by 3 (for a byte, the rotation's high bits from before it).
pub fn payload_hash(data: &[u8]) -> u32 {
    let mut u = 0u32;
    let words = data.len() / 4;
    for w in data[..words * 4].chunks(4) {
        u = (u ^ u32::from_le_bytes([w[0], w[1], w[2], w[3]])).rotate_left(3);
    }
    for &b in &data[words * 4..] {
        u = ((u ^ b as u32) << 3) | (u >> 29);
    }
    u
}

/// What the net module (CNetData's +0x227c) adds for a message of `kind` with `payload`:
/// the byte hash of the kind plus, for kinds 0x1e and 0x1f, its payload's hash's byte
/// hash.  A turn without actions is one message of kind 0x1f carrying the turn's time
/// in milliseconds as two bytes (read: 0x00c0cec0 a turn, for 100 ms turns).
pub fn net_message(kind: u8, payload: &[u8]) -> u32 {
    let x = kind as u32 + if matches!(kind, 0x1e | 0x1f) { byte_hash(payload_hash(payload)) } else { 0 };
    byte_hash(x)
}

/// The modules the port does not compute, as the original had them in Booty Bay with
/// two players (read under winedbg at its first turns; the same on Harrow): the
/// databases and the map's custom data, made as the data loads.  A map with its own
/// object data or tables would have others.
pub const BOOTY_BAY_MODULES: [(u8, u32); 11] = [
    (19, 0), (18, 0xc7d37d14), (17, 0x50bc09d8), (16, 0xa4fc2e24), (14, 0x68b89b88), (13, 0x38eb3c00), (12, 0x38eb3c00),
    (9, 0xd85c9c00), (7, 0x98054236), (5, 0xa996f046), (4, 0xb5891e1b),
];

/// An object as the game module names it: its slot ^ birth (the handles in a player's
/// lists carry the unit's tag at +0xc and +0x10), 0 for one in a slot never freed.
pub type Tagged = u32;

/// A player's selection as the game module reads it (CPlayerWar3 +0x34).
#[derive(Clone, Debug, Default)]
pub struct Selection {
    /// The units selected (+0xc), in order.
    pub units: Vec<Tagged>,
    /// The subgroups (+0x1b8), each a type's units, and the one selected (+0x1c0).
    pub subgroups: Vec<Vec<Tagged>>,
    pub current: Option<usize>,
    /// The 10 control groups (+0x20, 0x14 apart).
    pub groups: [Vec<Tagged>; 10],
    /// The units at +0x1a4 (set with the first selection) and +0x1a8.
    pub active: Option<Tagged>,
    pub other: Option<Tagged>,
}

impl Selection {
    /// Its hash (`FUN_6f28e740`): the count, each unit, the selected subgroup's index
    /// (-1 with none), each subgroup's count and units, the control groups' units, and
    /// the two units, each `u = rol(u ^ rol(x, 3), 3)` (a count or index `rol(u ^ n, 3)`).
    pub fn hash(&self) -> u32 {
        let unit = |u: u32, x: Tagged| (u ^ x.rotate_left(3)).rotate_left(3);
        let mut u = self.units.iter().fold(self.units.len() as u32, |u, &x| unit(u, x));
        u = (self.current.map_or(u32::MAX, |i| i as u32) ^ u).rotate_left(3);
        for g in &self.subgroups {
            u = g.iter().fold((u ^ g.len() as u32).rotate_left(3), |u, &x| unit(u, x));
        }
        for g in &self.groups {
            u = g.iter().fold(u, |u, &x| unit(u, x));
        }
        self.active.iter().chain(self.other.iter()).fold(u, |u, &x| unit(u, x))
    }
}

impl World {
    /// The game module (6, CGameWar3, `FUN_6f1de210`): from 0xc, each of the 12
    /// players' parts, `m = rol(m ^ part, 3)`.  A player's part (`FUN_6f25c560`): its
    /// handicap (+0x29c) and tech tree hash, then its experience handicap (+0x2a4), its
    /// 7 slots at +0x2b4 (empty in the games read), and its selection
    /// (`crate::selection`).
    pub fn game_module(&self) -> u32 {
        (0..12u8).fold(0xc, |m, p| {
            let Some(pl) = self.players.get(p as usize) else { return m };
            let mut u = (pl.handicap.to_bits() ^ self.tech_hash(p)).rotate_left(3);
            u = (u ^ pl.handicap_xp.to_bits()).rotate_left(3);
            let part = (self.sync_selection_hash(p) ^ u).rotate_left(3);
            (m ^ part).rotate_left(3)
        })
    }

    /// The net module's part of a turn without actions, as its message comes in.
    pub fn net_turn(&mut self, ms: u16) {
        self.net_sum = self.net_sum.wrapping_add(net_message(0x1f, &ms.to_le_bytes()));
    }

    /// The net module's part of a turn with its players' actions: its message carries
    /// the turn's time, then each player's block (id, length, actions), as a replay's
    /// turn record does (read: the original's messages and sums for turns with a
    /// selection, a control group, a training order, and a rally point).
    pub fn net_turn_actions(&mut self, ms: u16, actions: &[(u8, Vec<u8>)]) {
        let mut payload = ms.to_le_bytes().to_vec();
        for (pid, a) in actions {
            payload.push(*pid);
            payload.extend((a.len() as u16).to_le_bytes());
            payload.extend(a);
        }
        self.net_sum = self.net_sum.wrapping_add(net_message(0x1f, &payload));
    }

    /// The keepalive checksum for the turn whose message came in last, from the state
    /// after the one before it.  Modules the port does not compute yet (the databases,
    /// 4, 5, 7, and 9; the map's custom data, 12 to 19) are taken from
    /// `keepalive_modules`, values read from the original; module 0 is the cheat
    /// flags, 0 without cheats.
    /// The game module (6) that would make this keepalive `want`, the other inputs as
    /// they are: for reading which part of a differing keepalive differs.
    pub fn game_module_for(&self, want: u32) -> u32 {
        let state = self.rng.state();
        let h = byte_hash((state >> 32) as u32).wrapping_add(byte_hash(state as u32));
        let presence = self.tags.free.last().copied().unwrap_or(self.tags.slots);
        let mut c = want.rotate_right(3) ^ h;
        c = c.rotate_right(3) ^ self.tags.born;
        c = c.rotate_right(3) ^ presence;
        let m = c.rotate_right(3) ^ self.game_ms;
        let mut a = m.rotate_right(3) ^ 0x17ab;
        let mut modules = self.keepalive_modules.clone();
        modules.insert(0, 0);
        modules.insert(2, self.net_sum);
        modules.insert(3, self.rand_data.checksum());
        // Peel the modules below 6 off the fold (it runs from the last to the first).
        for (_, &v) in modules.range(..6) {
            a = a.rotate_right(3) ^ v;
        }
        let above = modules.range(7..).rev().fold(0u32, |m, (_, &v)| (m ^ v).rotate_left(3));
        a.rotate_right(3) ^ above
    }

    pub fn keepalive(&self) -> u32 {
        let mut modules = self.keepalive_modules.clone();
        modules.insert(0, 0);
        modules.insert(2, self.net_sum);
        modules.insert(3, self.rand_data.checksum());
        modules.insert(6, self.game_module());
        let m = modules.iter().rev().fold(0u32, |m, (_, &v)| (m ^ v).rotate_left(3));
        let t = self.game_ms;
        let presence = self.tags.free.last().copied().unwrap_or(self.tags.slots);
        let state = self.rng.state();
        let h = byte_hash((state >> 32) as u32).wrapping_add(byte_hash(state as u32));
        let mut c = t ^ (m ^ 0x17ab).rotate_left(3);
        c = c.rotate_left(3) ^ presence;
        c = c.rotate_left(3) ^ self.tags.born;
        c = c.rotate_left(3) ^ h;
        c.rotate_left(3)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 100 ms turn without actions adds what the original's net module grew by each
    /// turn of Booty Bay.
    #[test]
    fn an_empty_turn_adds_what_the_original_adds() {
        assert_eq!(net_message(0x1f, &100u16.to_le_bytes()), 0x00c0_cec0);
    }

    /// What the original's net module grew by for turns with a player's actions: a
    /// selection (`16 ... 1a 19 ...`) and a control group (`17 ...`), from player 2.
    #[test]
    fn a_turn_with_actions_adds_what_the_original_adds() {
        let select = [
            0x16, 0x01, 0x01, 0x00, 0x38, 0x31, 0x00, 0x00, 0x38, 0x31, 0x00, 0x00, 0x1a, 0x19, 0x6f, 0x65, 0x70, 0x6f,
            0x38, 0x31, 0x00, 0x00, 0x38, 0x31, 0x00, 0x00,
        ];
        let group = [0x17, 0x00, 0x01, 0x00, 0x18, 0x31, 0x00, 0x00, 0x18, 0x31, 0x00, 0x00];
        for (actions, grew) in [(&select[..], 0x0097_c380), (&group[..], 0x008f_2c30)] {
            let mut payload = 100u16.to_le_bytes().to_vec();
            payload.push(2);
            payload.extend((actions.len() as u16).to_le_bytes());
            payload.extend(actions);
            assert_eq!(net_message(0x1f, &payload), grew);
        }
    }

    /// The selection hash for the states read from the original (player 1 of Booty
    /// Bay: a peon selected, then put in group 1, then three peons and group 2, then
    /// group 1 recalled), as their game module values show them: each the start's
    /// 0xaa436eb2 with player 1's part changed by `rol(before ^ after, 4)`.
    #[test]
    fn selections_hash_as_the_originals() {
        let module = |s: &Selection| 0xaa43_6eb2 ^ (Selection::default().hash() ^ s.hash()).rotate_left(4);
        let mut s = Selection { units: vec![0], subgroups: vec![vec![0]], current: Some(0), active: Some(0), ..Default::default() };
        assert_eq!(module(&s), 0x55b4_b14d);
        s.groups[0] = vec![0];
        assert_eq!(module(&s), 0x55fd_914d);
        s.units = vec![0; 3];
        s.subgroups = vec![vec![0; 3]];
        s.groups[1] = vec![0; 3];
        assert_eq!(module(&s), 0xd5bc_894c);
        s.units = vec![0];
        s.subgroups = vec![vec![0]];
        assert_eq!(module(&s), 0xd7bc_914d);
    }

    /// Booty Bay's first keepalive from the original's module values there.
    #[test]
    fn the_first_keepalive_is_the_originals() {
        let modules = [
            (19u32, 0u32), (18, 0xc7d37d14), (17, 0x50bc09d8), (16, 0xa4fc2e24), (14, 0x68b89b88), (13, 0x38eb3c00),
            (12, 0x38eb3c00), (9, 0xd85c9c00), (7, 0x98054236), (6, 0xaa436eb2), (5, 0xa996f046), (4, 0xb5891e1b),
            (3, 0x4c0afd48), (2, 0x00c0cec0), (0, 0),
        ];
        let m = modules.iter().fold(0u32, |m, &(_, v)| (m ^ v).rotate_left(3));
        let h = byte_hash(0x0d5a5d0a).wrapping_add(byte_hash(0x8064a444));
        let mut c = (m ^ 0x17ab).rotate_left(3);
        for v in [0x3205, 0x3205, h] {
            c = c.rotate_left(3) ^ v;
        }
        assert_eq!(c.rotate_left(3), 0x610c6f3a);
    }
}
