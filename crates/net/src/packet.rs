//! W3GS packets: `F7`, the packet's id, its length (header included, u16), and its body.
//! Every layout here was captured from 1.27b under Wine or read from game.dll's
//! handlers (the host's dispatch is `FUN_6f99b870`, the player's `FUN_6f98a8e0`).

use crate::Error;
use crate::slot::SlotTable;
use crate::stat::GameStat;
use crate::wire::{Reader, Writer};

pub const HEADER: u8 = 0xF7;

/// The Frozen Throne's product code ("W3XP" backwards); Reign of Chaos's is `ROC`.
pub const TFT: [u8; 4] = *b"PX3W";
pub const ROC: [u8; 4] = *b"3RAW";
/// 1.27's version, which SEARCHGAME and GAMEINFO carry.
pub const VERSION: u32 = 27;

/// A `sockaddr_in` as the packets carry it: family 2, port big-endian, address, zeros.
/// The host's own appears as all zeros.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SockAddr(pub [u8; 16]);

impl SockAddr {
    pub fn new(addr: std::net::SocketAddrV4) -> SockAddr {
        let mut b = [0u8; 16];
        b[0] = 2;
        b[2..4].copy_from_slice(&addr.port().to_be_bytes());
        b[4..8].copy_from_slice(&addr.ip().octets());
        SockAddr(b)
    }

    pub fn addr(&self) -> Option<std::net::SocketAddrV4> {
        let b = self.0;
        (b[0] == 2).then(|| std::net::SocketAddrV4::new([b[4], b[5], b[6], b[7]].into(), u16::from_be_bytes([b[2], b[3]])))
    }
}

/// What a chat packet says: a line, or a player's choice in the setup.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Chat {
    /// A line in the setup's chat (flag 0x10).
    Lobby(String),
    /// Setup choices (flags 0x11 to 0x14): a team, color, race bits, or handicap.
    Team(u8),
    Color(u8),
    Race(u8),
    Handicap(u8),
    /// A line in the game (flag 0x20): to all (0), allies (1), observers (2), or a
    /// player (3 + the player).
    Game { scope: u32, text: String },
}

/// Why a player left (PLAYERLEFT, LEAVEGAME).
pub mod leave {
    pub const DISCONNECT: u32 = 0x01;
    pub const LOST: u32 = 0x07;
    pub const LOST_BUILDINGS: u32 = 0x08;
    pub const WON: u32 = 0x09;
    pub const DRAW: u32 = 0x0A;
    pub const OBSERVER: u32 = 0x0B;
    pub const LOBBY: u32 = 0x0D;
}

/// Why a join was refused (REJECTJOIN).
pub mod reject {
    pub const FULL: u32 = 0x09;
    pub const STARTED: u32 = 0x0A;
    pub const WRONG_PASSWORD: u32 = 0x1B;
}

/// A packet, either way.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Packet {
    // Host to player.
    /// 0x01: the host's tick count, to be sent back (PONGTOHOST).
    PingFromHost(u32),
    /// 0x04: a joining player's welcome: the slots, their player id, and their address
    /// as the host sees it.
    SlotInfoJoin { slots: SlotTable, pid: u8, addr: SockAddr },
    /// 0x05
    RejectJoin { reason: u32 },
    /// 0x06: another player in the game.  `marks` is 0x0001, or 0x0101 in a loaded
    /// game's setup (as 1.27b sends them).
    PlayerInfo { join_counter: u32, pid: u8, name: String, marks: u16, external: SockAddr, internal: SockAddr },
    /// 0x07
    PlayerLeft { pid: u8, reason: u32 },
    /// 0x08: a player has loaded the map.
    PlayerLoaded { pid: u8 },
    /// 0x09
    SlotInfo(SlotTable),
    /// 0x0A, 0x0B: the countdown begins; it is over and the game loads.
    CountdownStart,
    CountdownEnd,
    /// 0x0C: a turn: its length in milliseconds and each player's actions.  0x48 is the
    /// same with more of the turn's actions to come.
    IncomingAction { interval: u16, actions: Vec<(u8, Vec<u8>)>, more: bool },
    /// 0x0E: players found out of step (game.dll `FUN_6f98b5d0`); kept raw.
    Desync(Vec<u8>),
    /// 0x0F
    ChatFromHost { to: Vec<u8>, from: u8, chat: Chat },
    /// 0x10: players the game waits on, with how long it has waited (ms).
    StartLag(Vec<(u8, u32)>),
    /// 0x11
    StopLag { pid: u8, waited: u32 },
    /// 0x3D: the map a joining player must have.
    MapCheck { path: String, size: u32, crc32: u32, xoro: u32, sha1: [u8; 20] },
    /// 0x3F: the map comes from player `from`.
    StartDownload { from: u8 },
    /// 0x43: a piece of the map, at `offset`.
    MapPart { to: u8, from: u8, offset: u32, data: Vec<u8> },

    // Player to host.
    /// 0x1E
    ReqJoin { host_counter: u32, entry_key: u32, listen_port: u16, peer_key: u32, name: String, internal: SockAddr },
    /// 0x21
    LeaveGame { reason: u32 },
    /// 0x23
    GameLoadedSelf,
    /// 0x26: the player's actions for the next turn.
    OutgoingAction(Vec<u8>),
    /// 0x27: after each turn, the player's game checksum.
    OutgoingKeepalive { checksum: u32 },
    /// 0x28
    ChatToHost { to: Vec<u8>, from: u8, chat: Chat },
    /// 0x42: how much of the checked map the player has (flag 1, its size; 0 for none).
    MapSize { flag: u8, size: u32 },
    /// 0x44: the map pieces received so far.
    MapPartOk { from: u8, to: u8, received: u32 },
    /// 0x46
    PongToHost(u32),

    // Over UDP.
    /// 0x2F: who hosts a game of this product and version?
    SearchGame { product: [u8; 4], version: u32 },
    /// 0x30: a game on offer.
    GameInfo(GameInfo),
    /// 0x31, 0x32, 0x33: a game is created, its count of players changes, it is gone.
    CreateGame { product: [u8; 4], version: u32, host_counter: u32 },
    RefreshGame { host_counter: u32, players: u32, player_slots: u32 },
    DecreateGame { host_counter: u32 },

    /// Anything else, unread.
    Other { id: u8, body: Vec<u8> },
}

/// A game on offer, as GAMEINFO describes it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GameInfo {
    pub product: [u8; 4],
    pub version: u32,
    pub host_counter: u32,
    pub entry_key: u32,
    pub name: String,
    pub stat: GameStat,
    pub slots_total: u32,
    /// 1 for a custom map, 9 for one of Blizzard's.
    pub game_type: u32,
    /// People in the game.
    pub players: u32,
    /// The map's slots for people.
    pub player_slots: u32,
    /// Seconds since the game was created.
    pub uptime: u32,
    /// The host's TCP port.
    pub port: u16,
}

fn write_chat(w: &mut Writer, to: &[u8], from: u8, chat: &Chat) {
    w.u8(to.len() as u8).bytes(to).u8(from);
    match chat {
        Chat::Lobby(t) => w.u8(0x10).cstr(t),
        Chat::Team(v) => w.u8(0x11).u8(*v),
        Chat::Color(v) => w.u8(0x12).u8(*v),
        Chat::Race(v) => w.u8(0x13).u8(*v),
        Chat::Handicap(v) => w.u8(0x14).u8(*v),
        Chat::Game { scope, text } => w.u8(0x20).u32(*scope).cstr(text),
    };
}

fn read_chat(r: &mut Reader) -> Result<(Vec<u8>, u8, Chat), Error> {
    let n = r.u8()? as usize;
    let to = r.bytes(n)?.to_vec();
    let from = r.u8()?;
    let chat = match r.u8()? {
        0x10 => Chat::Lobby(r.cstr()?),
        0x11 => Chat::Team(r.u8()?),
        0x12 => Chat::Color(r.u8()?),
        0x13 => Chat::Race(r.u8()?),
        0x14 => Chat::Handicap(r.u8()?),
        0x20 => Chat::Game { scope: r.u32()?, text: r.cstr()? },
        f => return Err(Error::Bad(format!("chat flag {f:#x}"))),
    };
    Ok((to, from, chat))
}

impl Packet {
    pub fn id(&self) -> u8 {
        use Packet as P;
        match self {
            P::PingFromHost(_) => 0x01,
            P::SlotInfoJoin { .. } => 0x04,
            P::RejectJoin { .. } => 0x05,
            P::PlayerInfo { .. } => 0x06,
            P::PlayerLeft { .. } => 0x07,
            P::PlayerLoaded { .. } => 0x08,
            P::SlotInfo(_) => 0x09,
            P::CountdownStart => 0x0A,
            P::CountdownEnd => 0x0B,
            P::IncomingAction { more: false, .. } => 0x0C,
            P::IncomingAction { more: true, .. } => 0x48,
            P::Desync(_) => 0x0E,
            P::ChatFromHost { .. } => 0x0F,
            P::StartLag(_) => 0x10,
            P::StopLag { .. } => 0x11,
            P::MapCheck { .. } => 0x3D,
            P::StartDownload { .. } => 0x3F,
            P::MapPart { .. } => 0x43,
            P::ReqJoin { .. } => 0x1E,
            P::LeaveGame { .. } => 0x21,
            P::GameLoadedSelf => 0x23,
            P::OutgoingAction(_) => 0x26,
            P::OutgoingKeepalive { .. } => 0x27,
            P::ChatToHost { .. } => 0x28,
            P::MapSize { .. } => 0x42,
            P::MapPartOk { .. } => 0x44,
            P::PongToHost(_) => 0x46,
            P::SearchGame { .. } => 0x2F,
            P::GameInfo(_) => 0x30,
            P::CreateGame { .. } => 0x31,
            P::RefreshGame { .. } => 0x32,
            P::DecreateGame { .. } => 0x33,
            P::Other { id, .. } => *id,
        }
    }

    /// The packet with its header.
    pub fn to_bytes(&self) -> Vec<u8> {
        use Packet as P;
        let mut w = Writer::default();
        match self {
            P::PingFromHost(t) | P::PongToHost(t) => {
                w.u32(*t);
            }
            P::SlotInfoJoin { slots, pid, addr } => {
                slots.write_sized(&mut w);
                w.u8(*pid).bytes(&addr.0);
            }
            P::RejectJoin { reason } | P::LeaveGame { reason } => {
                w.u32(*reason);
            }
            P::PlayerInfo { join_counter, pid, name, marks, external, internal } => {
                w.u32(*join_counter).u8(*pid).cstr(name).u16(*marks).bytes(&external.0).bytes(&internal.0);
            }
            P::PlayerLeft { pid, reason } => {
                w.u8(*pid).u32(*reason);
            }
            P::PlayerLoaded { pid } => {
                w.u8(*pid);
            }
            P::SlotInfo(slots) => slots.write_sized(&mut w),
            P::CountdownStart | P::CountdownEnd | P::GameLoadedSelf => {}
            P::IncomingAction { interval, actions, .. } => {
                w.u16(*interval);
                if !actions.is_empty() {
                    let mut a = Writer::default();
                    for (pid, data) in actions {
                        a.u8(*pid).u16(data.len() as u16).bytes(data);
                    }
                    w.u16(crc32fast::hash(&a.0) as u16).bytes(&a.0);
                }
            }
            P::Desync(b) => {
                w.bytes(b);
            }
            P::ChatFromHost { to, from, chat } | P::ChatToHost { to, from, chat } => write_chat(&mut w, to, *from, chat),
            P::StartLag(lagging) => {
                w.u8(lagging.len() as u8);
                for (pid, ms) in lagging {
                    w.u8(*pid).u32(*ms);
                }
            }
            P::StopLag { pid, waited } => {
                w.u8(*pid).u32(*waited);
            }
            P::MapCheck { path, size, crc32, xoro, sha1 } => {
                w.u32(1).cstr(path).u32(*size).u32(*crc32).u32(*xoro).bytes(sha1);
            }
            P::StartDownload { from } => {
                w.u32(1).u8(*from);
            }
            P::MapPart { to, from, offset, data } => {
                w.u8(*to).u8(*from).u32(1).u32(*offset).u32(crc32fast::hash(data)).bytes(data);
            }
            P::ReqJoin { host_counter, entry_key, listen_port, peer_key, name, internal } => {
                w.u32(*host_counter).u32(*entry_key).u8(0).u16(*listen_port).u32(*peer_key).cstr(name).u16(1).bytes(&internal.0);
            }
            P::OutgoingAction(data) => {
                w.u32(crc32fast::hash(data)).bytes(data);
            }
            P::OutgoingKeepalive { checksum } => {
                w.u8(0).u32(*checksum);
            }
            P::MapSize { flag, size } => {
                w.u32(1).u8(*flag).u32(*size);
            }
            P::MapPartOk { from, to, received } => {
                w.u8(*from).u8(*to).u32(1).u32(*received);
            }
            P::SearchGame { product, version } => {
                w.bytes(product).u32(*version).u32(0);
            }
            P::GameInfo(g) => {
                w.bytes(&g.product).u32(g.version).u32(g.host_counter).u32(g.entry_key).cstr(&g.name).u8(0);
                w.bytes(&g.stat.to_bytes()).u8(0);
                w.u32(g.slots_total).u32(g.game_type).u32(g.players).u32(g.player_slots).u32(g.uptime).u16(g.port);
            }
            P::CreateGame { product, version, host_counter } => {
                w.bytes(product).u32(*version).u32(*host_counter);
            }
            P::RefreshGame { host_counter, players, player_slots } => {
                w.u32(*host_counter).u32(*players).u32(*player_slots);
            }
            P::DecreateGame { host_counter } => {
                w.u32(*host_counter);
            }
            P::Other { body, .. } => {
                w.bytes(body);
            }
        }
        let mut out = vec![HEADER, self.id()];
        out.extend(((w.0.len() + 4) as u16).to_le_bytes());
        out.extend(w.0);
        out
    }

    /// A packet's length from its first four bytes, if they are a header.
    pub fn length(head: &[u8]) -> Result<Option<usize>, Error> {
        if head.len() < 4 {
            return Ok(None);
        }
        if head[0] != HEADER {
            return Err(Error::Bad(format!("header {:#x}", head[0])));
        }
        let n = u16::from_le_bytes([head[2], head[3]]) as usize;
        if n < 4 {
            return Err(Error::Bad(format!("length {n}")));
        }
        Ok(Some(n))
    }

    /// Reads a whole packet, header included.
    pub fn parse(b: &[u8]) -> Result<Packet, Error> {
        use Packet as P;
        let n = Packet::length(b)?.ok_or(Error::Short)?;
        let body = b.get(4..n).ok_or(Error::Short)?;
        let id = b[1];
        let mut r = Reader::new(body);
        let p = match id {
            0x01 => P::PingFromHost(r.u32()?),
            0x04 => P::SlotInfoJoin { slots: SlotTable::read_sized(&mut r)?, pid: r.u8()?, addr: SockAddr(r.array()?) },
            0x05 => P::RejectJoin { reason: r.u32()? },
            0x06 => {
                let (join_counter, pid, name, marks) = (r.u32()?, r.u8()?, r.cstr()?, r.u16()?);
                P::PlayerInfo { join_counter, pid, name, marks, external: SockAddr(r.array()?), internal: SockAddr(r.array()?) }
            }
            0x07 => P::PlayerLeft { pid: r.u8()?, reason: r.u32()? },
            0x08 => P::PlayerLoaded { pid: r.u8()? },
            0x09 => P::SlotInfo(SlotTable::read_sized(&mut r)?),
            0x0A => P::CountdownStart,
            0x0B => P::CountdownEnd,
            0x0C | 0x48 => {
                let interval = r.u16()?;
                let mut actions = Vec::new();
                if !r.is_empty() {
                    r.u16()?;
                    while !r.is_empty() {
                        let pid = r.u8()?;
                        let len = r.u16()? as usize;
                        actions.push((pid, r.bytes(len)?.to_vec()));
                    }
                }
                P::IncomingAction { interval, actions, more: id == 0x48 }
            }
            0x0E => P::Desync(r.rest().to_vec()),
            0x0F => {
                let (to, from, chat) = read_chat(&mut r)?;
                P::ChatFromHost { to, from, chat }
            }
            0x10 => {
                let n = r.u8()?;
                P::StartLag((0..n).map(|_| Ok((r.u8()?, r.u32()?))).collect::<Result<_, Error>>()?)
            }
            0x11 => P::StopLag { pid: r.u8()?, waited: r.u32()? },
            0x3D => {
                r.u32()?;
                P::MapCheck { path: r.cstr()?, size: r.u32()?, crc32: r.u32()?, xoro: r.u32()?, sha1: r.array()? }
            }
            0x3F => {
                r.u32()?;
                P::StartDownload { from: r.u8()? }
            }
            0x43 => {
                let (to, from) = (r.u8()?, r.u8()?);
                r.u32()?;
                let offset = r.u32()?;
                r.u32()?;
                P::MapPart { to, from, offset, data: r.rest().to_vec() }
            }
            0x1E => {
                let (host_counter, entry_key) = (r.u32()?, r.u32()?);
                r.u8()?;
                let (listen_port, peer_key, name) = (r.u16()?, r.u32()?, r.cstr()?);
                r.u16()?;
                P::ReqJoin { host_counter, entry_key, listen_port, peer_key, name, internal: SockAddr(r.array()?) }
            }
            0x21 => P::LeaveGame { reason: r.u32()? },
            0x23 => P::GameLoadedSelf,
            0x26 => {
                r.u32()?;
                P::OutgoingAction(r.rest().to_vec())
            }
            0x27 => {
                r.u8()?;
                P::OutgoingKeepalive { checksum: r.u32()? }
            }
            0x28 => {
                let (to, from, chat) = read_chat(&mut r)?;
                P::ChatToHost { to, from, chat }
            }
            0x42 => {
                r.u32()?;
                P::MapSize { flag: r.u8()?, size: r.u32()? }
            }
            0x44 => {
                let (from, to) = (r.u8()?, r.u8()?);
                r.u32()?;
                P::MapPartOk { from, to, received: r.u32()? }
            }
            0x46 => P::PongToHost(r.u32()?),
            0x2F => {
                let p = P::SearchGame { product: r.array()?, version: r.u32()? };
                r.u32()?;
                p
            }
            0x30 => {
                let (product, version, host_counter, entry_key) = (r.array()?, r.u32()?, r.u32()?, r.u32()?);
                let name = r.cstr()?;
                r.cstr()?;
                let rest = r.rest();
                let end = rest.iter().position(|&c| c == 0).ok_or(Error::Short)?;
                let stat = GameStat::parse(&rest[..end])?;
                let mut r = Reader::new(&rest[end + 1..]);
                P::GameInfo(GameInfo {
                    product,
                    version,
                    host_counter,
                    entry_key,
                    name,
                    stat,
                    slots_total: r.u32()?,
                    game_type: r.u32()?,
                    players: r.u32()?,
                    player_slots: r.u32()?,
                    uptime: r.u32()?,
                    port: r.u16()?,
                })
            }
            0x31 => P::CreateGame { product: r.array()?, version: r.u32()?, host_counter: r.u32()? },
            0x32 => P::RefreshGame { host_counter: r.u32()?, players: r.u32()?, player_slots: r.u32()? },
            0x33 => P::DecreateGame { host_counter: r.u32()? },
            id => P::Other { id, body: body.to_vec() },
        };
        Ok(p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unhex(s: &str) -> Vec<u8> {
        let s: String = s.split_whitespace().collect();
        (0..s.len()).step_by(2).map(|k| u8::from_str_radix(&s[k..k + 2], 16).unwrap()).collect()
    }

    /// Each packet captured from 1.27b under Wine reads, and writes back the same.
    #[test]
    fn captured_packets_read_and_write_back() {
        for hex in [
            // GAMEINFO for Booty Bay.
            "f7 30 83 00 50 58 33 57 1b 00 00 00 01 00 00 00 ac 6a 32 6b 4c 6f 63 61 6c 20 47 61 6d 65 20 28 4a 61 63 6b 29 00 00 01 03 49 07 01 01 a1 01 b9 49 01 13 8f e7 6f 4d 8b 61 71 73 5d 29 33 29 ad 43 6f 6f 75 79 43 61 3b 79 2f 77 33 6d 01 4b 8f 61 63 6b 01 01 77 e3 45 3b 25 fd 81 5d 43 77 1d 39 53 93 41 d1 73 ad 0f cb 3f 77 21 00 02 00 00 00 09 00 00 00 01 00 00 00 02 00 00 00 08 00 00 00 e0 17",
            // REQJOIN from the original.
            "f7 1e 2a 00 01 00 00 00 ac 6a 32 6b 00 e0 17 02 00 00 00 4a 61 63 6b 00 01 00 02 00 18 38 7f 00 00 01 00 00 00 00 00 00 00 00",
            // SLOTINFOJOIN, PLAYERINFO, MAPCHECK, SLOTINFO for Booty Bay.
            "f7 04 30 00 19 00 02 01 64 02 00 00 00 60 01 64 00 ff 00 00 01 0c 60 01 64 e3 44 9b 35 00 02 02 02 00 d9 d4 7f 00 00 01 00 00 00 00 00 00 00 00",
            "f7 06 30 00 03 00 00 00 01 4a 61 63 6b 00 01 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00",
            // A loaded game's PLAYERINFO.
            "f7 06 30 00 01 00 00 00 01 4a 61 63 6b 00 01 01 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00",
            "f7 3d 3d 00 01 00 00 00 4d 61 70 73 5c 28 32 29 42 6f 6f 74 79 42 61 79 2e 77 33 6d 00 d1 e7 01 00 00 5a ef d8 13 8f e7 6e 76 e3 3a 25 fc 80 5c 43 76 38 53 93 41 d0 72 ac cb 3f 77 20",
            "f7 09 1f 00 19 00 02 01 64 02 00 00 00 60 01 64 02 ff 02 00 01 01 60 01 64 7d 60 9b 35 00 02",
            // Chat, ping, countdown, loaded.
            "f7 0f 14 00 01 02 01 10 68 65 6c 6c 6f 20 74 68 65 72 65 00",
            "f7 01 08 00 c7 88 9d 35",
            "f7 0a 04 00",
            "f7 08 05 00 02",
            "f7 3f 09 00 01 00 00 00 01",
            // The original's desync notice.
            "f7 0e 0e 00 01 00 00 00 04 00 00 00 00 00",
            // Packets the original accepted from us.
            "f7 42 0d 00 01 00 00 00 01 d1 e7 01 00",
            "f7 28 16 00 01 01 02 10 68 69 20 66 72 6f 6d 20 70 72 6f 62 65 00",
            "f7 44 0e 00 01 02 01 00 00 00 a2 05 00 00",
        ] {
            let b = unhex(hex);
            let p = Packet::parse(&b).unwrap();
            assert!(!matches!(p, Packet::Other { .. }), "{p:?}");
            assert_eq!(p.to_bytes(), b, "{p:?}");
        }
    }

    #[test]
    fn the_originals_slots_read() {
        let b = unhex("f7 04 30 00 19 00 02 01 64 02 00 00 00 60 01 64 00 ff 00 00 01 0c 60 01 64 e3 44 9b 35 00 02 02 02 00 d9 d4 7f 00 00 01 00 00 00 00 00 00 00 00");
        let Packet::SlotInfoJoin { slots, pid, addr } = Packet::parse(&b).unwrap() else { panic!() };
        assert_eq!(pid, 2);
        assert_eq!(addr.addr(), Some("127.0.0.1:55764".parse().unwrap()));
        assert_eq!(slots.slots.len(), 2);
        assert_eq!(slots.slots[0].pid, 1);
        assert_eq!(slots.slots[0].race, crate::slot::race::RANDOM | crate::slot::race::SELECTABLE);
        assert_eq!((slots.seed, slots.layout, slots.player_slots), (0x359b44e3, 0, 2));
    }
}
