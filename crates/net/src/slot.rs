//! The game setup's slot table, as SLOTINFO and SLOTINFOJOIN carry it.

use crate::Error;
use crate::wire::{Reader, Writer};

/// A slot's state.
pub mod status {
    pub const OPEN: u8 = 0;
    pub const CLOSED: u8 = 1;
    pub const OCCUPIED: u8 = 2;
}

/// A slot's race bits: one race, or random; SELECTABLE when its player may change it
/// (the map does not fix it).
pub mod race {
    pub const HUMAN: u8 = 0x01;
    pub const ORC: u8 = 0x02;
    pub const NIGHT_ELF: u8 = 0x04;
    pub const UNDEAD: u8 = 0x08;
    pub const RANDOM: u8 = 0x20;
    pub const SELECTABLE: u8 = 0x40;
}

/// The slot table's layout: melee, custom forces, or custom forces with fixed player
/// settings (w3i flags 0x40 and 0x20).
pub mod layout {
    pub const MELEE: u8 = 0;
    pub const CUSTOM_FORCES: u8 = 1;
    pub const FIXED_PLAYER_SETTINGS: u8 = 3;
}

/// Nobody has reported the map yet (`Slot::download`).
pub const DOWNLOAD_UNKNOWN: u8 = 255;

/// A slot of the game setup.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Slot {
    /// The connection's player id (the host is 1), or 0 for no person.
    pub pid: u8,
    /// How much of the map the slot's player has, in percent (`DOWNLOAD_UNKNOWN`).
    pub download: u8,
    pub status: u8,
    pub computer: bool,
    pub team: u8,
    pub color: u8,
    pub race: u8,
    /// A computer's difficulty: 0 easy, 1 normal, 2 insane.
    pub ai: u8,
    /// 50 to 100.
    pub handicap: u8,
}

/// The setup's slots, the seed the game starts from, and its layout.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SlotTable {
    pub slots: Vec<Slot>,
    pub seed: u32,
    pub layout: u8,
    /// The map's playable slots (its users' and computers').
    pub player_slots: u8,
}

impl SlotTable {
    pub fn write(&self, w: &mut Writer) {
        w.u8(self.slots.len() as u8);
        for s in &self.slots {
            w.u8(s.pid).u8(s.download).u8(s.status).u8(s.computer as u8).u8(s.team).u8(s.color).u8(s.race).u8(s.ai).u8(s.handicap);
        }
        w.u32(self.seed).u8(self.layout).u8(self.player_slots);
    }

    pub fn read(r: &mut Reader) -> Result<SlotTable, Error> {
        let n = r.u8()?;
        let mut slots = Vec::with_capacity(n as usize);
        for _ in 0..n {
            let [pid, download, status, computer, team, color, race, ai, handicap] = r.array()?;
            slots.push(Slot { pid, download, status, computer: computer != 0, team, color, race, ai, handicap });
        }
        Ok(SlotTable { slots, seed: r.u32()?, layout: r.u8()?, player_slots: r.u8()? })
    }

    /// The table as a packet carries it: its length, then itself.
    pub fn write_sized(&self, w: &mut Writer) {
        let mut t = Writer::default();
        self.write(&mut t);
        w.u16(t.0.len() as u16).bytes(&t.0);
    }

    pub fn read_sized(r: &mut Reader) -> Result<SlotTable, Error> {
        let n = r.u16()? as usize;
        SlotTable::read(&mut Reader::new(r.bytes(n)?))
    }
}
