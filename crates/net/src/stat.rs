//! The game description a GAMEINFO carries (its "stat string"): the game's settings, the
//! map's size, path, and checksums, and the host's name, coded so that it holds no NUL.

use crate::Error;
use crate::wire::{Reader, Writer};

/// Codes bytes without NULs: each run of up to seven bytes follows a mask byte whose bit
/// `k + 1` says the run's byte `k` was odd (kept); an even byte is sent plus one.
pub fn encode(d: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(d.len() + d.len() / 7 + 1);
    for run in d.chunks(7) {
        let mut mask = 1u8;
        for (k, &c) in run.iter().enumerate() {
            if c % 2 == 1 {
                mask |= 1 << (k + 1);
            }
        }
        out.push(mask);
        out.extend(run.iter().map(|&c| if c % 2 == 1 { c } else { c + 1 }));
    }
    out
}

pub fn decode(b: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(b.len());
    for run in b.chunks(8) {
        let mask = run[0];
        for (k, &c) in run[1..].iter().enumerate() {
            out.push(if mask & (1 << (k + 1)) != 0 { c } else { c.wrapping_sub(1) });
        }
    }
    out
}

/// The game's settings word (captured from 1.27b: a fast melee game with default
/// visibility, teams together, and locked teams is 0x00064802).
pub mod settings {
    /// Speed: 0 slow, 1 normal, 2 fast.
    pub const SPEED_MASK: u32 = 0x3;
    pub const HIDE_TERRAIN: u32 = 0x100;
    pub const MAP_EXPLORED: u32 = 0x200;
    pub const ALWAYS_VISIBLE: u32 = 0x400;
    pub const DEFAULT_VISIBILITY: u32 = 0x800;
    pub const OBSERVERS_ON_DEFEAT: u32 = 0x2000;
    pub const FULL_OBSERVERS: u32 = 0x3000;
    pub const TEAMS_TOGETHER: u32 = 0x4000;
    pub const LOCK_TEAMS: u32 = 0x60000;
    pub const SHARED_CONTROL: u32 = 0x1000000;
    pub const RANDOM_HERO: u32 = 0x2000000;
    pub const RANDOM_RACES: u32 = 0x4000000;
    pub const REFEREES: u32 = 0x40000000;
}

/// A decoded stat string.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GameStat {
    /// The settings word (`settings`).
    pub settings: u32,
    pub map_width: u16,
    pub map_height: u16,
    /// The map's game checksum (`map::Checksums::xoro`).
    pub map_xoro: u32,
    /// The map's path from the game's folder, as `Maps\(2)BootyBay.w3m`.
    pub map_path: String,
    pub host: String,
    pub map_sha1: [u8; 20],
}

impl GameStat {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut w = Writer::default();
        w.u32(self.settings).u8(0).u16(self.map_width).u16(self.map_height).u32(self.map_xoro);
        w.cstr(&self.map_path).cstr(&self.host).u8(0).bytes(&self.map_sha1);
        encode(&w.0)
    }

    pub fn parse(coded: &[u8]) -> Result<GameStat, Error> {
        let d = decode(coded);
        let mut r = Reader::new(&d);
        let settings = r.u32()?;
        r.u8()?;
        let (map_width, map_height, map_xoro) = (r.u16()?, r.u16()?, r.u32()?);
        let map_path = r.cstr()?;
        let host = r.cstr()?;
        r.u8()?;
        let map_sha1 = r.array()?;
        Ok(GameStat { settings, map_width, map_height, map_xoro, map_path, host, map_sha1 })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Booty Bay hosted by the original (1.27b) under Wine.
    const BOOTY_BAY: &str = "01 03 49 07 01 01 a1 01 b9 49 01 13 8f e7 6f 4d 8b 61 71 73 5d 29 33 29 ad 43 6f 6f 75 79 43 61 3b 79 2f 77 33 6d 01 4b 8f 61 63 6b 01 01 77 e3 45 3b 25 fd 81 5d 43 77 1d 39 53 93 41 d1 73 ad 0f cb 3f 77 21";

    fn unhex(s: &str) -> Vec<u8> {
        let s: String = s.split_whitespace().collect();
        (0..s.len()).step_by(2).map(|k| u8::from_str_radix(&s[k..k + 2], 16).unwrap()).collect()
    }

    #[test]
    fn the_originals_stat_string_decodes_and_codes_back() {
        let coded = unhex(BOOTY_BAY);
        let s = GameStat::parse(&coded).unwrap();
        assert_eq!(s.settings, 0x00064802);
        assert_eq!((s.map_width, s.map_height), (160, 72));
        assert_eq!(s.map_xoro, 0x6ee78f13);
        assert_eq!(s.map_path, "Maps\\(2)BootyBay.w3m");
        assert_eq!(s.host, "Jack");
        assert_eq!(s.map_sha1[..4], [0x76, 0xe3, 0x3a, 0x25]);
        assert_eq!(s.to_bytes(), coded);
    }
}
