//! A game's host, as 1.27b's `Net::NetRouter` behaves on the wire: it offers the game,
//! seats joining players (SLOTINFOJOIN, PLAYERINFO, MAPCHECK, then the slots), pings
//! them, sends the map to those without it, passes chat on, counts down, and once the
//! game is under way sends everyone each turn's actions.  What the setup allows (a
//! player's team, color, race, or handicap) is the program's to decide: it hears the
//! choice (`Event::Choice`), changes `slots`, and calls `send_slots`.

use std::net::{SocketAddr, SocketAddrV4};
use std::time::{Duration, Instant};

use crate::conn::{Conn, Discovery, Listener};
use crate::packet::{self, Chat, GameInfo, Packet, SockAddr, leave, reject};
use crate::slot::{DOWNLOAD_UNKNOWN, SlotTable, status};

/// The host's own player id.
pub const HOST_PID: u8 = 1;
/// Map pieces' size (as 1.27b sends them).
pub const MAP_PART: usize = 1442;
/// Map pieces sent ahead of the player's acknowledgment.
const MAP_WINDOW: u32 = 16;
const PING_EVERY: Duration = Duration::from_secs(5);
const ANNOUNCE_EVERY: Duration = Duration::from_secs(3);

/// What the host heard.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// A player was seated.
    Joined { pid: u8, name: String },
    Left { pid: u8, reason: u32 },
    /// A chat line, to the host among others.
    Chat { from: u8, chat: Chat },
    /// A player's setup choice (`Chat::Team` and the like).
    Choice { pid: u8, chat: Chat },
    /// A player has the whole map.
    MapReady { pid: u8 },
    /// A player has loaded the game.
    Loaded { pid: u8 },
    /// A player's actions for the next turn.
    Actions { pid: u8, data: Vec<u8> },
    /// A player played a turn; their game's checksum after it.
    Keepalive { pid: u8, checksum: u32 },
}

/// The map the game is played on.
pub struct HostMap {
    /// Its path as the players' games name it (`Maps\...`).
    pub path: String,
    pub bytes: Vec<u8>,
    pub checksums: crate::map::Checksums,
}

struct Player {
    pid: u8,
    name: String,
    conn: Conn,
    internal: SockAddr,
    external: SockAddr,
    /// Map bytes sent, and acknowledged.
    sent: Option<u32>,
    acked: u32,
    loaded: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Lobby,
    Loading,
    Playing,
}

pub struct Host {
    listener: Listener,
    discovery: Option<Discovery>,
    pub info: GameInfo,
    pub map: HostMap,
    pub slots: SlotTable,
    /// The host's own name, for PLAYERINFO.
    pub name: String,
    players: Vec<Player>,
    joining: Vec<(Conn, Instant)>,
    joins: u32,
    pub phase: Phase,
    created: Instant,
    last_ping: Instant,
    last_announce: Instant,
}

impl Host {
    /// Offers a game on `port` (TCP), its description in `info` (whose players, uptime,
    /// port, host counter, and entry key the host fills in).
    pub fn new(port: u16, mut info: GameInfo, map: HostMap, slots: SlotTable, name: String) -> std::io::Result<Host> {
        let discovery = Discovery::new(port).map_err(|e| log::warn!("w3gs: discovery: {e}")).ok();
        // Another game on this computer holding the game port is at this address on
        // that port, so this one is offered on another (1.27b will not join a game at
        // its own address and port, taking it for its own).
        let shared = !discovery.as_ref().is_some_and(|d| d.listening);
        let listener = if shared { Listener::bind(0)? } else { Listener::bind(port).or_else(|_| Listener::bind(0))? };
        info.port = listener.port();
        info.entry_key = random_u32();
        if info.host_counter == 0 {
            info.host_counter = 1;
        }
        let now = Instant::now();
        let h = Host { listener, discovery, info, map, slots, name, players: Vec::new(), joining: Vec::new(), joins: 0, phase: Phase::Lobby, created: now, last_ping: now, last_announce: now - ANNOUNCE_EVERY };
        if let Some(d) = h.discovery.as_ref() {
            d.broadcast(&Packet::CreateGame { product: h.info.product, version: h.info.version, host_counter: h.info.host_counter });
        }
        Ok(h)
    }

    pub fn port(&self) -> u16 {
        self.listener.port()
    }

    /// The players' ids and names, the host first.
    pub fn players(&self) -> Vec<(u8, String)> {
        std::iter::once((HOST_PID, self.name.clone())).chain(self.players.iter().map(|p| (p.pid, p.name.clone()))).collect()
    }

    fn game_info(&self) -> Packet {
        let mut g = self.info.clone();
        g.players = 1 + self.players.len() as u32;
        g.uptime = self.created.elapsed().as_secs() as u32;
        g.port = self.listener.port();
        Packet::GameInfo(g)
    }

    /// Takes in what has come and answers what the protocol answers by itself.
    pub fn poll(&mut self) -> Vec<Event> {
        let mut events = Vec::new();
        self.discover();
        while let Some(c) = self.listener.accept() {
            log::debug!("w3gs: host: connection from {:?}", c.peer);
            self.joining.push((c, Instant::now()));
        }
        // Connections that have not asked to join after ten seconds are dropped.
        for (mut conn, since) in std::mem::take(&mut self.joining) {
            let req = conn.poll().into_iter().find_map(|p| match p {
                Packet::ReqJoin { name, internal, .. } => Some((name, internal)),
                _ => None,
            });
            match req {
                Some((name, internal)) => events.extend(self.join(conn, name, internal)),
                None if conn.alive && since.elapsed() < Duration::from_secs(10) => self.joining.push((conn, since)),
                None => {}
            }
        }
        let mut gone = Vec::new();
        for k in 0..self.players.len() {
            for p in self.players[k].conn.poll() {
                self.handle(k, p, &mut events, &mut gone);
            }
            if !self.players[k].conn.alive {
                gone.push((self.players[k].pid, leave::DISCONNECT));
            }
            self.send_map_parts(k);
        }
        for (pid, reason) in gone {
            if self.players.iter().any(|p| p.pid == pid) {
                self.drop_player(pid, reason);
                events.push(Event::Left { pid, reason });
            }
        }
        if self.last_ping.elapsed() >= PING_EVERY {
            self.last_ping = Instant::now();
            let t = self.created.elapsed().as_millis() as u32;
            self.broadcast(&Packet::PingFromHost(t));
        }
        events
    }

    /// Answers searches for this game, and offers it now and then (to programs that
    /// cannot hear searches, sharing this computer's game port).
    fn discover(&mut self) {
        let Some(d) = self.discovery.as_ref() else { return };
        if self.phase != Phase::Lobby {
            return;
        }
        for (from, p) in d.poll() {
            if let Packet::SearchGame { product, version } = p
                && product == self.info.product
                && version == self.info.version
            {
                d.send_to(&self.game_info(), from);
            }
        }
        if self.last_announce.elapsed() >= ANNOUNCE_EVERY {
            self.last_announce = Instant::now();
            d.broadcast(&self.game_info());
        }
    }

    fn join(&mut self, mut conn: Conn, name: String, internal: SockAddr) -> Option<Event> {
        log::debug!("w3gs: host: {name} asks to join from {:?}", conn.peer);
        if self.phase != Phase::Lobby {
            conn.send(&Packet::RejectJoin { reason: reject::STARTED });
            return None;
        }
        let Some(slot) = self.slots.slots.iter().position(|s| s.status == status::OPEN && !s.computer) else {
            conn.send(&Packet::RejectJoin { reason: reject::FULL });
            return None;
        };
        let pid = (2..=12u8).find(|id| !self.players.iter().any(|p| p.pid == *id))?;
        self.joins += 1;
        let marks = if self.info.game_type & 0x200 != 0 { 0x0101 } else { 0x0001 };
        let external = match conn.peer {
            Some(SocketAddr::V4(a)) => SockAddr::new(a),
            _ => SockAddr::default(),
        };
        conn.send(&Packet::SlotInfoJoin { slots: self.slots.clone(), pid, addr: external });
        conn.send(&Packet::PlayerInfo { join_counter: self.joins, pid: HOST_PID, name: self.name.clone(), marks, external: SockAddr::default(), internal: SockAddr::default() });
        for p in &self.players {
            conn.send(&Packet::PlayerInfo { join_counter: self.joins, pid: p.pid, name: p.name.clone(), marks, external: p.external, internal: p.internal });
        }
        let info = Packet::PlayerInfo { join_counter: self.joins, pid, name: name.clone(), marks, external, internal };
        for p in self.players.iter_mut() {
            p.conn.send(&info);
        }
        let c = &self.map.checksums;
        conn.send(&Packet::MapCheck { path: self.map.path.clone(), size: c.size, crc32: c.crc32, xoro: c.xoro, sha1: c.sha1 });
        self.players.push(Player { pid, name: name.clone(), conn, internal, external, sent: None, acked: 0, loaded: false });
        let s = &mut self.slots.slots[slot];
        s.pid = pid;
        s.status = status::OCCUPIED;
        s.download = DOWNLOAD_UNKNOWN;
        self.send_slots();
        Some(Event::Joined { pid, name })
    }

    fn handle(&mut self, k: usize, p: Packet, events: &mut Vec<Event>, gone: &mut Vec<(u8, u32)>) {
        let pid = self.players[k].pid;
        log::trace!("w3gs: host: from {pid}: {p:?}");
        match p {
            Packet::PongToHost(_) => {}
            Packet::LeaveGame { reason } => gone.push((pid, reason)),
            Packet::MapSize { flag, size } => {
                if flag == 1 && size == self.map.checksums.size {
                    self.players[k].sent = None;
                    self.set_download(pid, 100);
                    events.push(Event::MapReady { pid });
                } else if self.players[k].sent.is_none() && self.phase == Phase::Lobby {
                    // The map comes from the host.
                    self.players[k].conn.send(&Packet::StartDownload { from: HOST_PID });
                    self.players[k].sent = Some(0);
                    self.players[k].acked = 0;
                    self.set_download(pid, 0);
                }
            }
            Packet::MapPartOk { received, .. } => {
                let size = self.map.checksums.size.max(1);
                let pl = &mut self.players[k];
                let before = pl.acked as u64 * 10 / size as u64;
                pl.acked = received.max(pl.acked);
                if pl.acked as u64 * 10 / size as u64 != before {
                    let percent = (pl.acked as u64 * 100 / size as u64).min(99) as u8;
                    self.set_download(pid, percent);
                }
            }
            Packet::ChatToHost { to, chat, .. } => match chat {
                Chat::Lobby(_) | Chat::Game { .. } => {
                    for q in self.players.iter_mut().filter(|q| to.contains(&q.pid)) {
                        q.conn.send(&Packet::ChatFromHost { to: vec![q.pid], from: pid, chat: chat.clone() });
                    }
                    if to.contains(&HOST_PID) {
                        events.push(Event::Chat { from: pid, chat });
                    }
                }
                choice => events.push(Event::Choice { pid, chat: choice }),
            },
            Packet::GameLoadedSelf if self.phase != Phase::Lobby => {
                self.players[k].loaded = true;
                self.broadcast(&Packet::PlayerLoaded { pid });
                events.push(Event::Loaded { pid });
            }
            Packet::OutgoingAction(data) => events.push(Event::Actions { pid, data }),
            Packet::OutgoingKeepalive { checksum } => events.push(Event::Keepalive { pid, checksum }),
            other => log::debug!("w3gs: host: unexpected {other:?} from {pid}"),
        }
    }

    fn set_download(&mut self, pid: u8, percent: u8) {
        if let Some(s) = self.slots.slots.iter_mut().find(|s| s.pid == pid) {
            s.download = percent;
            self.send_slots();
        }
    }

    /// Keeps a downloading player's map pieces coming, a window ahead of what they have.
    fn send_map_parts(&mut self, k: usize) {
        let size = self.map.bytes.len() as u32;
        let p = &mut self.players[k];
        let Some(mut sent) = p.sent else { return };
        while sent < size && sent < p.acked + MAP_WINDOW * MAP_PART as u32 {
            let end = (sent as usize + MAP_PART).min(size as usize);
            let data = self.map.bytes[sent as usize..end].to_vec();
            p.conn.send(&Packet::MapPart { to: p.pid, from: HOST_PID, offset: sent, data });
            sent = end as u32;
        }
        p.sent = Some(sent);
    }

    /// Everyone has the slots as they stand.
    pub fn send_slots(&mut self) {
        let p = Packet::SlotInfo(self.slots.clone());
        self.broadcast(&p);
    }

    pub fn broadcast(&mut self, p: &Packet) {
        for q in self.players.iter_mut() {
            q.conn.send(p);
        }
    }

    /// The host's chat line, to the players `to`.
    pub fn chat(&mut self, to: &[u8], chat: Chat) {
        for q in self.players.iter_mut().filter(|q| to.contains(&q.pid)) {
            q.conn.send(&Packet::ChatFromHost { to: vec![q.pid], from: HOST_PID, chat: chat.clone() });
        }
    }

    /// Removes a player: the others are told, and in the setup their slot opens.
    pub fn drop_player(&mut self, pid: u8, reason: u32) {
        let Some(k) = self.players.iter().position(|p| p.pid == pid) else { return };
        let mut p = self.players.remove(k);
        p.conn.close();
        let reason = if self.phase == Phase::Lobby { leave::LOBBY } else { reason };
        self.broadcast(&Packet::PlayerLeft { pid, reason });
        if self.phase == Phase::Lobby {
            if let Some(s) = self.slots.slots.iter_mut().find(|s| s.pid == pid) {
                s.pid = 0;
                s.status = status::OPEN;
                s.download = DOWNLOAD_UNKNOWN;
            }
            self.send_slots();
        }
    }

    pub fn start_countdown(&mut self) {
        self.broadcast(&Packet::CountdownStart);
    }

    /// The countdown is over: everyone loads the game.
    pub fn end_countdown(&mut self) {
        self.broadcast(&Packet::CountdownEnd);
        self.phase = Phase::Loading;
        if let Some(d) = self.discovery.as_ref() {
            d.broadcast(&Packet::DecreateGame { host_counter: self.info.host_counter });
        }
    }

    /// The host has loaded the game.
    pub fn loaded(&mut self) {
        self.broadcast(&Packet::PlayerLoaded { pid: HOST_PID });
    }

    /// Whether every player has loaded.
    pub fn all_loaded(&self) -> bool {
        self.players.iter().all(|p| p.loaded)
    }

    /// A turn: its length and each player's actions.
    pub fn send_turn(&mut self, interval: u16, actions: Vec<(u8, Vec<u8>)>) {
        self.phase = Phase::Playing;
        // A packet holds at most 1460 bytes; a longer turn goes in parts (0x48), the
        // last a 0x0C.
        let mut parts: Vec<Vec<(u8, Vec<u8>)>> = vec![Vec::new()];
        let mut size = 0;
        for a in actions {
            if size + a.1.len() + 3 > 1450 && !parts.last().unwrap().is_empty() {
                parts.push(Vec::new());
                size = 0;
            }
            size += a.1.len() + 3;
            parts.last_mut().unwrap().push(a);
        }
        let n = parts.len();
        for (k, actions) in parts.into_iter().enumerate() {
            let more = k + 1 < n;
            self.broadcast(&Packet::IncomingAction { interval: if more { 0 } else { interval }, actions, more });
        }
    }

    /// The game is over for everyone: it is no longer offered.
    pub fn close(&mut self) {
        if let Some(d) = self.discovery.as_ref() {
            d.broadcast(&Packet::DecreateGame { host_counter: self.info.host_counter });
        }
        for p in self.players.iter_mut() {
            p.conn.close();
        }
        self.players.clear();
    }

    pub fn has_player(&self, pid: u8) -> bool {
        self.players.iter().any(|p| p.pid == pid)
    }

    /// The address a player's connection came from.
    pub fn player_addr(&self, pid: u8) -> Option<SocketAddrV4> {
        self.players.iter().find(|p| p.pid == pid).and_then(|p| p.external.addr())
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        self.close();
    }
}

/// An entry key for the game: what a joining player must quote (from the clock, which
/// is enough to tell one game from another).
fn random_u32() -> u32 {
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    (t.as_nanos() as u64).wrapping_mul(0x9E3779B97F4A7C15).rotate_left(17) as u32
}

pub use packet::{ROC, TFT, VERSION};
