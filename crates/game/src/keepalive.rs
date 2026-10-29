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
/// two players (read under winedbg at its first turns; the game's, 6, unchanged while
/// nobody acts).
pub const BOOTY_BAY_MODULES: [(u8, u32); 12] = [
    (19, 0), (18, 0xc7d37d14), (17, 0x50bc09d8), (16, 0xa4fc2e24), (14, 0x68b89b88), (13, 0x38eb3c00), (12, 0x38eb3c00),
    (9, 0xd85c9c00), (7, 0x98054236), (6, 0xaa436eb2), (5, 0xa996f046), (4, 0xb5891e1b),
];

impl World {
    /// The net module's part of a turn without actions, as its message comes in.
    pub fn net_turn(&mut self, ms: u16) {
        self.net_sum = self.net_sum.wrapping_add(net_message(0x1f, &ms.to_le_bytes()));
    }

    /// The net module's part of a turn with its players' actions: its message carries
    /// the turn's time, then each player's block (id, length, actions), as a replay's
    /// turn record does.
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
    /// after the one before it.  Modules the port does not compute yet (the game's
    /// players, 6; the databases, 4, 5, 7, and 9; the map's custom data, 12 to 19) are
    /// taken from `keepalive_modules`, values read from the original; module 0 is the
    /// cheat flags, 0 without cheats.
    pub fn keepalive(&self) -> u32 {
        let mut modules = self.keepalive_modules.clone();
        modules.insert(0, 0);
        modules.insert(2, self.net_sum);
        modules.insert(3, self.rand_data.checksum());
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
