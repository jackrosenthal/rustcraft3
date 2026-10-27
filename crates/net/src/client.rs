//! A player joined to a host, as 1.27b's `Net::NetClient` behaves on the wire: it asks
//! to join, answers pings, reports and downloads the map, sends chat and setup choices,
//! says when it has loaded, and once the game is under way plays the host's turns,
//! sending its actions and a checksum after each turn.

use std::net::SocketAddr;

use crate::conn::{Conn, Discovery};
use crate::packet::{Chat, GameInfo, Packet, SockAddr, leave};
use crate::slot::SlotTable;

/// What the player heard.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// Seated, as player `pid`.
    Joined { pid: u8 },
    Rejected { reason: u32 },
    /// The slots changed (`Client::slots`).
    Slots,
    PlayerJoined { pid: u8, name: String },
    PlayerLeft { pid: u8, reason: u32 },
    Chat { from: u8, chat: Chat },
    /// The host's map: the program says whether it has it (`Client::map_status`).
    MapCheck { path: String, size: u32, crc32: u32, xoro: u32, sha1: [u8; 20] },
    /// The map came from the host, whole and checked.
    MapDownloaded(Vec<u8>),
    CountdownStart,
    /// The game loads.
    CountdownEnd,
    Loaded { pid: u8 },
    /// A turn: its length in milliseconds and each player's actions.
    Turn { interval: u16, actions: Vec<(u8, Vec<u8>)> },
    /// The game waits on players, or stops waiting on one.
    StartLag(Vec<(u8, u32)>),
    StopLag { pid: u8 },
    Desync(Vec<u8>),
    /// The connection to the host is gone.
    Closed,
}

pub struct Client {
    conn: Conn,
    /// This player's id, once seated.
    pub pid: u8,
    pub slots: SlotTable,
    /// The other players' ids and names.
    pub players: Vec<(u8, String)>,
    map_size: u32,
    map_crc32: u32,
    download: Option<Vec<u8>>,
    received: u32,
    /// A turn's actions so far, while its parts come (0x48).
    partial: Vec<(u8, Vec<u8>)>,
    closed: bool,
}

impl Client {
    /// Asks to join the game a GAMEINFO described, from `from` (the host's address).
    pub fn join(from: SocketAddr, game: &GameInfo, name: &str, listen_port: u16) -> std::io::Result<Client> {
        let mut conn = Conn::connect(SocketAddr::new(from.ip(), game.port))?;
        let internal = match conn.local_addr() {
            Some(SocketAddr::V4(a)) => SockAddr::new(a),
            _ => SockAddr::default(),
        };
        conn.send(&Packet::ReqJoin { host_counter: game.host_counter, entry_key: game.entry_key, listen_port, peer_key: 0, name: name.to_string(), internal });
        Ok(Client { conn, pid: 0, slots: SlotTable { slots: Vec::new(), seed: 0, layout: 0, player_slots: 0 }, players: Vec::new(), map_size: 0, map_crc32: 0, download: None, received: 0, partial: Vec::new(), closed: false })
    }

    pub fn poll(&mut self) -> Vec<Event> {
        let mut events = Vec::new();
        for p in self.conn.poll() {
            match p {
                Packet::PingFromHost(t) => self.conn.send(&Packet::PongToHost(t)),
                Packet::SlotInfoJoin { slots, pid, .. } => {
                    self.slots = slots;
                    self.pid = pid;
                    events.push(Event::Joined { pid });
                    events.push(Event::Slots);
                }
                Packet::RejectJoin { reason } => {
                    self.closed = true;
                    events.push(Event::Rejected { reason });
                }
                Packet::PlayerInfo { pid, name, .. } => {
                    self.players.retain(|p| p.0 != pid);
                    self.players.push((pid, name.clone()));
                    events.push(Event::PlayerJoined { pid, name });
                }
                Packet::PlayerLeft { pid, reason } => {
                    self.players.retain(|p| p.0 != pid);
                    events.push(Event::PlayerLeft { pid, reason });
                }
                Packet::PlayerLoaded { pid } => events.push(Event::Loaded { pid }),
                Packet::SlotInfo(slots) => {
                    self.slots = slots;
                    events.push(Event::Slots);
                }
                Packet::CountdownStart => events.push(Event::CountdownStart),
                Packet::CountdownEnd => events.push(Event::CountdownEnd),
                Packet::IncomingAction { interval, actions, more } => {
                    self.partial.extend(actions);
                    if !more {
                        events.push(Event::Turn { interval, actions: std::mem::take(&mut self.partial) });
                    }
                }
                Packet::Desync(b) => events.push(Event::Desync(b)),
                Packet::ChatFromHost { from, chat, .. } => events.push(Event::Chat { from, chat }),
                Packet::StartLag(l) => events.push(Event::StartLag(l)),
                Packet::StopLag { pid, .. } => events.push(Event::StopLag { pid }),
                Packet::MapCheck { path, size, crc32, xoro, sha1 } => {
                    self.map_size = size;
                    self.map_crc32 = crc32;
                    events.push(Event::MapCheck { path, size, crc32, xoro, sha1 });
                }
                Packet::StartDownload { .. } => {
                    self.download = Some(vec![0; self.map_size as usize]);
                    self.received = 0;
                }
                Packet::MapPart { from, offset, data, .. } => self.map_part(from, offset, &data, &mut events),
                other => log::debug!("w3gs: client: unexpected {other:?}"),
            }
        }
        if !self.conn.alive && !self.closed {
            self.closed = true;
            events.push(Event::Closed);
        }
        events
    }

    fn map_part(&mut self, from: u8, offset: u32, data: &[u8], events: &mut Vec<Event>) {
        let Some(buf) = self.download.as_mut() else { return };
        let (start, end) = (offset as usize, offset as usize + data.len());
        if offset != self.received || end > buf.len() {
            return;
        }
        buf[start..end].copy_from_slice(data);
        self.received = end as u32;
        self.conn.send(&Packet::MapPartOk { from, to: self.pid, received: self.received });
        if self.received == self.map_size {
            let map = self.download.take().unwrap_or_default();
            if crc32fast::hash(&map) == self.map_crc32 {
                self.conn.send(&Packet::MapSize { flag: 1, size: self.map_size });
                events.push(Event::MapDownloaded(map));
            } else {
                log::warn!("w3gs: the downloaded map's checksum differs");
            }
        }
    }

    /// Whether this computer has the host's map (as MAPCHECK described it); without it,
    /// the host sends it.
    pub fn map_status(&mut self, have: bool) {
        self.conn.send(&Packet::MapSize { flag: 1, size: if have { self.map_size } else { 0 } });
    }

    /// A chat line to everyone else (or a setup choice, to the host).
    pub fn chat(&mut self, chat: Chat) {
        let to: Vec<u8> = match chat {
            Chat::Lobby(_) | Chat::Game { .. } => self.players.iter().map(|p| p.0).collect(),
            _ => vec![crate::host::HOST_PID],
        };
        self.conn.send(&Packet::ChatToHost { to, from: self.pid, chat });
    }

    pub fn loaded(&mut self) {
        self.conn.send(&Packet::GameLoadedSelf);
    }

    pub fn send_actions(&mut self, data: Vec<u8>) {
        self.conn.send(&Packet::OutgoingAction(data));
    }

    /// A turn played, and the game's checksum after it.
    pub fn keepalive(&mut self, checksum: u32) {
        self.conn.send(&Packet::OutgoingKeepalive { checksum });
    }

    pub fn leave(&mut self, reason: u32) {
        self.conn.send(&Packet::LeaveGame { reason });
        self.conn.close();
        self.closed = true;
    }

    pub fn alive(&self) -> bool {
        !self.closed && self.conn.alive
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        if !self.closed {
            self.leave(leave::DISCONNECT);
        }
    }
}

/// Games found on the network: SEARCHGAME every few seconds, the GAMEINFOs that come
/// back, and those heard offered.  A game no longer heard of for ten seconds is gone.
pub struct Browser {
    discovery: Discovery,
    product: [u8; 4],
    version: u32,
    last_search: Option<std::time::Instant>,
    pub games: Vec<(SocketAddr, GameInfo, std::time::Instant)>,
}

impl Browser {
    pub fn new(port: u16, product: [u8; 4], version: u32) -> std::io::Result<Browser> {
        Ok(Browser { discovery: Discovery::seeker(port)?, product, version, last_search: None, games: Vec::new() })
    }

    /// Returns whether the list changed.
    pub fn poll(&mut self) -> bool {
        let now = std::time::Instant::now();
        if self.last_search.is_none_or(|t| now - t >= std::time::Duration::from_secs(3)) {
            self.last_search = Some(now);
            self.discovery.broadcast(&Packet::SearchGame { product: self.product, version: self.version });
        }
        let before = self.games.len();
        let mut changed = false;
        for (from, p) in self.discovery.poll() {
            match p {
                Packet::GameInfo(g) if g.product == self.product && g.version == self.version => {
                    match self.games.iter_mut().find(|e| e.0.ip() == from.ip() && e.1.host_counter == g.host_counter) {
                        Some(e) => {
                            changed |= e.1 != g;
                            *e = (from, g, now);
                        }
                        None => {
                            self.games.push((from, g, now));
                            changed = true;
                        }
                    }
                }
                Packet::CreateGame { .. } | Packet::RefreshGame { .. } => {
                    self.discovery.send_to(&Packet::SearchGame { product: self.product, version: self.version }, from);
                }
                Packet::DecreateGame { host_counter } => self.games.retain(|e| !(e.0.ip() == from.ip() && e.1.host_counter == host_counter)),
                _ => {}
            }
        }
        self.games.retain(|e| now - e.2 < std::time::Duration::from_secs(10));
        changed || self.games.len() != before
    }
}
