//! The original's replays (.w3g): a header, then zlib blocks holding the game's record
//! (its host, name, description, players, and slots) and what happened, turn by turn:
//! each turn's length and each player's actions as the game sent them over the network
//! (W3GS), and after each turn the game's checksum.

use crate::{Error, Reader};

pub const MAGIC: &[u8; 28] = b"Warcraft III recorded game\x1a\0";

/// A player's record: the host's first, then one per other player.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayerRecord {
    pub pid: u8,
    pub name: String,
    /// The record's extra bytes (2 for a LAN game: a 1, then 0).
    pub extra: Vec<u8>,
}

/// A slot of the setup, as W3GS's slot table has it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Slot {
    pub pid: u8,
    pub download: u8,
    pub status: u8,
    pub computer: bool,
    pub team: u8,
    pub color: u8,
    pub race: u8,
    pub ai: u8,
    pub handicap: u8,
}

/// What happened, in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Record {
    /// 0x1F (0x1E in older replays): a turn of `ms` milliseconds and each player's
    /// actions.
    Turn { ms: u16, actions: Vec<(u8, Vec<u8>)> },
    /// 0x22: the game's checksum after the turn before.
    Checksum(Vec<u8>),
    /// 0x20: a chat line; `scope` for one in the game (all 0, allies 1, observers 2, a
    /// player 3 on), none in the setup's chat.
    Chat { pid: u8, scope: Option<u32>, text: String },
    /// 0x17: a player left, why (as LEAVEGAME says), and with what result.
    Leave { reason: u32, pid: u8, result: u32, unknown: u32 },
    /// Anything else (0x1A to 0x1C at the start, 0x23, 0x2F), unread.
    Other { id: u8, body: Vec<u8> },
}

#[derive(Clone, Debug)]
pub struct Replay {
    pub product: [u8; 4],
    pub version: u32,
    pub build: u16,
    pub flags: u16,
    pub length_ms: u32,
    pub host: PlayerRecord,
    pub game_name: String,
    /// The game's description as GAMEINFO carries it, still coded (w3-net's `stat`).
    pub stat: Vec<u8>,
    pub player_count: u32,
    pub game_type: u32,
    pub language: u32,
    pub players: Vec<PlayerRecord>,
    pub slots: Vec<Slot>,
    pub seed: u32,
    pub select_mode: u8,
    pub start_spots: u8,
    pub records: Vec<Record>,
}

impl Replay {
    pub fn parse(file: &[u8]) -> Result<Replay, Error> {
        let mut r = Reader::new(file);
        if r.bytes(28)? != MAGIC {
            return Err(Error::Format("not a recorded game"));
        }
        let header_size = r.u32()? as usize;
        let _compressed = r.u32()?;
        let _header_version = r.u32()?;
        let size = r.u32()? as usize;
        let blocks = r.u32()?;
        let product = r.tag()?;
        let version = r.u32()?;
        let build = r.u16()?;
        let flags = r.u16()?;
        let length_ms = r.u32()?;
        r.pos = header_size;
        // Each block is its own zlib stream, flushed but not finished.
        let mut data = Vec::with_capacity(size);
        for _ in 0..blocks {
            let packed = r.u16()? as usize;
            let unpacked = r.u16()? as usize;
            r.u32()?;
            let mut out = Vec::with_capacity(unpacked);
            let mut z = flate2::Decompress::new(true);
            let _ = z.decompress_vec(r.bytes(packed)?, &mut out, flate2::FlushDecompress::Sync);
            if out.len() < unpacked {
                return Err(Error::Format("short replay block"));
            }
            data.extend_from_slice(&out[..unpacked]);
        }
        data.truncate(size);
        Replay::read_data(&data, product, version, build, flags, length_ms)
    }

    fn read_data(data: &[u8], product: [u8; 4], version: u32, build: u16, flags: u16, length_ms: u32) -> Result<Replay, Error> {
        let mut r = Reader::new(data);
        r.u32()?;
        let player = |r: &mut Reader| -> Result<PlayerRecord, Error> {
            let _kind = r.u8()?;
            let pid = r.u8()?;
            let name = r.cstr()?;
            let n = r.u8()? as usize;
            Ok(PlayerRecord { pid, name, extra: r.bytes(n)?.to_vec() })
        };
        let host = player(&mut r)?;
        let game_name = r.cstr()?;
        r.cstr()?;
        let stat = {
            let rest = &r.data[r.pos..];
            let n = rest.iter().position(|&c| c == 0).ok_or(Error::Eof)?;
            r.pos += n + 1;
            rest[..n].to_vec()
        };
        let player_count = r.u32()?;
        let game_type = r.u32()?;
        let language = r.u32()?;
        let mut players = Vec::new();
        while r.data.get(r.pos) == Some(&0x16) {
            players.push(player(&mut r)?);
            r.u32()?;
        }
        if r.u8()? != 0x19 {
            return Err(Error::Format("no game start record"));
        }
        let n = r.u16()? as usize;
        let mut start = r.sub(n)?;
        let n = start.u8()?;
        let mut slots = Vec::with_capacity(n as usize);
        for _ in 0..n {
            let b = start.bytes(9)?;
            slots.push(Slot { pid: b[0], download: b[1], status: b[2], computer: b[3] != 0, team: b[4], color: b[5], race: b[6], ai: b[7], handicap: b[8] });
        }
        let seed = start.u32()?;
        let select_mode = start.u8()?;
        let start_spots = start.u8()?;
        let mut records = Vec::new();
        while r.remaining() > 0 {
            let id = r.u8()?;
            let rec = match id {
                0 => break,
                0x1E | 0x1F => {
                    let n = r.u16()? as usize;
                    let ms = r.u16()?;
                    let mut body = r.sub(n.saturating_sub(2))?;
                    let mut actions = Vec::new();
                    while body.remaining() > 0 {
                        let pid = body.u8()?;
                        let len = body.u16()? as usize;
                        actions.push((pid, body.bytes(len)?.to_vec()));
                    }
                    Record::Turn { ms, actions }
                }
                0x22 => {
                    let n = r.u8()? as usize;
                    Record::Checksum(r.bytes(n)?.to_vec())
                }
                0x20 => {
                    let pid = r.u8()?;
                    let n = r.u16()? as usize;
                    let mut body = r.sub(n)?;
                    let scope = match body.u8()? {
                        0x20 => Some(body.u32()?),
                        _ => None,
                    };
                    Record::Chat { pid, scope, text: body.cstr()? }
                }
                0x17 => Record::Leave { reason: r.u32()?, pid: r.u8()?, result: r.u32()?, unknown: r.u32()? },
                0x1A..=0x1C => Record::Other { id, body: r.bytes(4)?.to_vec() },
                0x23 => Record::Other { id, body: r.bytes(10)?.to_vec() },
                0x2F => Record::Other { id, body: r.bytes(8)?.to_vec() },
                _ => return Err(Error::Format("unknown replay record")),
            };
            records.push(rec);
        }
        Ok(Replay { product, version, build, flags, length_ms, host, game_name, stat, player_count, game_type, language, players, slots, seed, select_mode, start_spots, records })
    }

    /// The checksums after each turn, in order.
    pub fn checksums(&self) -> impl Iterator<Item = u32> + '_ {
        self.records.iter().filter_map(|r| match r {
            Record::Checksum(b) if b.len() >= 4 => Some(u32::from_le_bytes(b[..4].try_into().unwrap())),
            _ => None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A replay the original wrote (in the game's folder, which the repository does not
    /// hold): its record and turns read.
    #[test]
    fn the_originals_replay_reads() {
        let Ok(b) = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../Warcraft III/Replay/LastReplay.w3g")) else { return };
        let r = Replay::parse(&b).unwrap();
        assert_eq!((&r.product, r.version, r.build), (b"PX3W", 27, 6059));
        let turns = r.records.iter().filter(|r| matches!(r, Record::Turn { .. })).count();
        assert!(turns > 0);
        assert_eq!(r.checksums().count(), turns);
        assert_eq!(r.slots.len(), r.start_spots as usize);
    }
}
