//! Local area network games.  A host announces its game over UDP to whoever searches
//! the game port, takes players over TCP on the same port, runs the game setup
//! (GameChatroom), and once the game starts sends every player the frames all play:
//! each a step of the game and the commands its players gave, in the order the host
//! took them.  Everyone, the host too, plays only these frames, and the game runs the
//! same for the same frames, so all play the same game.  The messages are this game's
//! own; it plays with copies of itself.

use std::collections::VecDeque;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use w3_game::replay::{Command, Frame};

/// Messages of this protocol's version only are understood.
pub const VERSION: u32 = 1;
/// The game port when the options name none (netgameport).
pub const DEFAULT_PORT: u16 = 6112;
const MAGIC: &[u8; 4] = b"W3RN";

/// A game on offer, as its host announces it.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct GameAd {
    pub name: String,
    pub host: String,
    /// The map, as a file or a path in the game's archives.
    pub map: String,
    pub players: u8,
    pub slots: u8,
    pub port: u16,
    pub expansion: bool,
    /// Slow (0), normal (1), or fast (2).
    pub speed: u8,
}

/// Who holds a slot of the game setup.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum Holder {
    Open,
    Closed,
    /// AI_DIFFICULTY_NEWBIE, _NORMAL, or _INSANE.
    Computer(i32),
    /// A person: 0 the host, else a joined player's connection.
    Human(u32),
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct NetSlot {
    pub player: u8,
    pub holder: Holder,
    pub name: String,
    /// Index into RaceMenu: random, human, orc, undead, night elf.
    pub race: usize,
    pub team: i32,
    pub color: i32,
    /// Index into HandicapMenu: 100% down to 50%.
    pub handicap: usize,
}

/// The game setup everyone sees.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Setup {
    pub game: String,
    pub map: String,
    pub slots: Vec<NetSlot>,
    /// Slow (0), normal (1), or fast (2).
    pub speed: u8,
    /// The advanced options (crate::advanced).
    pub flags: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Msg {
    /// Discovery, over UDP.
    Search,
    Announce(GameAd),
    /// A player asks to join, and is let in (with their connection number) or not (a
    /// GlobalStrings key saying why).
    Hello { name: String, version: u32 },
    Welcome { id: u32 },
    Refused(String),
    Setup(Setup),
    Chat { from: String, text: String },
    /// A lobby notice: a NetworkStrings key and the player it is about.
    Notice { key: String, name: String },
    /// A joined player's choices for their own slot.
    Choose { race: usize, team: i32, color: i32, handicap: usize },
    /// The game begins with this setup.
    Start(Setup),
    /// A player's commands, to the host.
    Commands(Vec<Command>),
    /// A frame, from the host.
    Frame(u32, Frame),
    /// The frames a player has played, to the host.
    Played(u32),
    Leave,
}

fn encode(m: &Msg) -> Vec<u8> {
    let body = postcard::to_allocvec(m).unwrap_or_default();
    let mut out = MAGIC.to_vec();
    out.extend((body.len() as u32).to_le_bytes());
    out.extend(body);
    out
}

/// One connection's messages: a thread reads them into a channel (`None` once the
/// connection is gone); writes go straight out.
pub struct Conn {
    stream: TcpStream,
    rx: Receiver<Option<Msg>>,
    pub alive: bool,
}

impl Conn {
    pub fn new(stream: TcpStream) -> std::io::Result<Conn> {
        stream.set_nodelay(true)?;
        let mut reader = stream.try_clone()?;
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            loop {
                let mut head = [0u8; 8];
                if reader.read_exact(&mut head).is_err() || &head[..4] != MAGIC {
                    break;
                }
                let n = u32::from_le_bytes(head[4..].try_into().unwrap()) as usize;
                if n > 16 << 20 {
                    break;
                }
                let mut body = vec![0u8; n];
                if reader.read_exact(&mut body).is_err() {
                    break;
                }
                match postcard::from_bytes::<Msg>(&body) {
                    Ok(m) => {
                        if tx.send(Some(m)).is_err() {
                            return;
                        }
                    }
                    Err(e) => log::warn!("net: bad message: {e}"),
                }
            }
            let _ = tx.send(None);
        });
        Ok(Conn { stream, rx, alive: true })
    }

    pub fn connect(addr: SocketAddr) -> std::io::Result<Conn> {
        Conn::new(TcpStream::connect_timeout(&addr, Duration::from_secs(3))?)
    }

    pub fn send(&mut self, m: &Msg) {
        if self.alive && self.stream.write_all(&encode(m)).is_err() {
            self.alive = false;
        }
    }

    /// The messages that have come in.
    pub fn poll(&mut self) -> Vec<Msg> {
        let mut out = Vec::new();
        while let Ok(m) = self.rx.try_recv() {
            match m {
                Some(m) => out.push(m),
                None => self.alive = false,
            }
        }
        out
    }
}

/// Searching the network for games: a Search broadcast every two seconds, and the
/// announcements that answer it.
pub struct Browser {
    socket: UdpSocket,
    port: u16,
    last: Option<Instant>,
    pub games: Vec<(SocketAddr, GameAd, Instant)>,
}

impl Browser {
    pub fn new(port: u16) -> std::io::Result<Browser> {
        let socket = UdpSocket::bind("0.0.0.0:0")?;
        socket.set_broadcast(true)?;
        socket.set_nonblocking(true)?;
        Ok(Browser { socket, port, last: None, games: Vec::new() })
    }

    pub fn poll(&mut self) {
        if self.last.is_none_or(|t| t.elapsed() > Duration::from_secs(2)) {
            self.last = Some(Instant::now());
            let m = encode(&Msg::Search);
            let _ = self.socket.send_to(&m, ("255.255.255.255", self.port));
            let _ = self.socket.send_to(&m, ("127.0.0.1", self.port));
        }
        let mut buf = [0u8; 2048];
        while let Ok((n, from)) = self.socket.recv_from(&mut buf) {
            if let Some(Msg::Announce(ad)) = decode(&buf[..n]) {
                let addr = SocketAddr::new(from.ip(), ad.port);
                // A game answering at several addresses (the broadcast and the loopback)
                // is listed once, at the first.
                match self.games.iter_mut().find(|g| g.0 == addr || g.1 == ad) {
                    Some(g) => {
                        g.1 = ad;
                        g.2 = Instant::now();
                    }
                    None => self.games.push((addr, ad, Instant::now())),
                }
            }
        }
        // Games not heard from in six seconds are gone.
        self.games.retain(|g| g.2.elapsed() < Duration::from_secs(6));
    }
}

fn decode(b: &[u8]) -> Option<Msg> {
    if b.len() < 8 || &b[..4] != MAGIC {
        return None;
    }
    let n = u32::from_le_bytes(b[4..8].try_into().ok()?) as usize;
    postcard::from_bytes(b.get(8..8 + n)?).ok()
}

/// A hosted game: the announcement, the players, and once begun the frames.
pub struct Host {
    udp: Option<UdpSocket>,
    listener: TcpListener,
    pub ad: GameAd,
    /// Joined players: connection number, connection, and name (`None` until they
    /// have said hello).
    pub peers: Vec<(u32, Conn, Option<String>)>,
    next_id: u32,
}

impl Host {
    pub fn new(ad: GameAd) -> std::io::Result<Host> {
        let listener = TcpListener::bind(("0.0.0.0", ad.port))?;
        listener.set_nonblocking(true)?;
        // Another game on this computer may hold the UDP port; this one is then found
        // only by its address.
        let udp = UdpSocket::bind(("0.0.0.0", ad.port)).and_then(|s| s.set_nonblocking(true).map(|_| s)).ok();
        Ok(Host { udp, listener, ad, peers: Vec::new(), next_id: 1 })
    }

    /// Answers searches, takes new connections, and returns each peer's messages (and
    /// those gone).
    pub fn poll(&mut self) -> (Vec<(u32, Msg)>, Vec<u32>) {
        if let Some(udp) = &self.udp {
            let mut buf = [0u8; 2048];
            while let Ok((n, from)) = udp.recv_from(&mut buf) {
                if let Some(Msg::Search) = decode(&buf[..n]) {
                    let _ = udp.send_to(&encode(&Msg::Announce(self.ad.clone())), from);
                }
            }
        }
        while let Ok((s, _)) = self.listener.accept() {
            let _ = s.set_nonblocking(false);
            if let Ok(c) = Conn::new(s) {
                self.peers.push((self.next_id, c, None));
                self.next_id += 1;
            }
        }
        let mut msgs = Vec::new();
        let mut gone = Vec::new();
        for (id, c, _) in &mut self.peers {
            for m in c.poll() {
                msgs.push((*id, m));
            }
            if !c.alive {
                gone.push(*id);
            }
        }
        self.peers.retain(|p| p.1.alive);
        (msgs, gone)
    }

    pub fn send(&mut self, id: u32, m: &Msg) {
        if let Some(p) = self.peers.iter_mut().find(|p| p.0 == id) {
            p.1.send(m);
        }
    }

    pub fn broadcast(&mut self, m: &Msg) {
        let bytes = encode(m);
        for p in &mut self.peers {
            if p.2.is_some() && p.1.alive && p.1.stream.write_all(&bytes).is_err() {
                p.1.alive = false;
            }
        }
    }

    pub fn drop_peer(&mut self, id: u32) {
        self.peers.retain(|p| p.0 != id);
    }
}

/// How a network game plays: the host makes the frames; everyone plays them.
pub enum Role {
    Host {
        host: Host,
        /// Players' commands for the next frame, with the player each came from.
        pending: Vec<(u8, Command)>,
        /// Each peer's player and the frames it has played.
        peers: Vec<(u32, u8, u32)>,
    },
    Client {
        conn: Conn,
    },
}

impl std::fmt::Debug for Role {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(match self {
            Role::Host { .. } => "Host",
            Role::Client { .. } => "Client",
        })
    }
}

/// A network game under way.
pub struct Session {
    pub role: Role,
    /// Frames to play, in order, and the number of the next to come.
    pub queue: VecDeque<Frame>,
    pub next: u32,
    pub played: u32,
    /// Game time owed to frames not yet played.
    pub owed: f32,
    /// The host's time toward its next frame.
    clock: f32,
    /// The connection to the host is gone (or every peer of the host).
    pub lost: bool,
    /// Players who left, for the frames to say so.
    pub left: Vec<u8>,
    /// Waiting on a player who has fallen behind.
    pub waiting: bool,
    /// Frames are played up to this many (for tests that compare games).
    pub limit: Option<u32>,
    /// When the last frame came (a player's game waits on the host after two seconds).
    last_frame: Instant,
}

/// The host sends a frame every 50 ms of game time and waits for any player more
/// than 40 behind.
pub const TURN: f32 = 0.05;
const BEHIND: u32 = 40;

impl Session {
    pub fn new(role: Role) -> Session {
        Session { role, queue: VecDeque::new(), next: 0, played: 0, owed: 0.0, clock: 0.0, lost: false, left: Vec::new(), waiting: false, limit: None, last_frame: Instant::now() }
    }

    /// The local player's commands, to the host (the host takes its own at once).
    pub fn send_commands(&mut self, local: u8, commands: Vec<Command>) {
        if commands.is_empty() {
            return;
        }
        match &mut self.role {
            Role::Host { pending, .. } => pending.extend(commands.into_iter().map(|c| (local, c))),
            Role::Client { conn } => conn.send(&Msg::Commands(commands)),
        }
    }

    /// Takes in messages; the host makes frames as game time passes. Returns the chat
    /// lines that came.
    pub fn pump(&mut self, dt: f32) -> Vec<(String, String)> {
        let mut chat = Vec::new();
        match &mut self.role {
            Role::Host { host, pending, peers } => {
                let (msgs, gone) = host.poll();
                for (id, m) in msgs {
                    let player = peers.iter().find(|p| p.0 == id).map(|p| p.1);
                    match (m, player) {
                        (Msg::Commands(cs), Some(p)) => pending.extend(cs.into_iter().map(|c| (p, c))),
                        (Msg::Played(n), Some(_)) => {
                            if let Some(p) = peers.iter_mut().find(|p| p.0 == id) {
                                p.2 = p.2.max(n);
                            }
                        }
                        (Msg::Chat { from, text }, _) => chat.push((from, text)),
                        _ => {}
                    }
                }
                for id in gone {
                    if let Some(k) = peers.iter().position(|p| p.0 == id) {
                        self.left.push(peers[k].1);
                        peers.remove(k);
                    }
                }
                // A frame each turn, unless someone has fallen too far behind.
                self.waiting = peers.iter().any(|p| self.next.saturating_sub(p.2) > BEHIND);
                if !self.waiting {
                    self.clock += dt;
                    while self.clock >= TURN {
                        self.clock -= TURN;
                        let mut commands = std::mem::take(pending);
                        commands.extend(self.left.drain(..).map(|p| (p, Command::PlayerLeft { player: p })));
                        let f = Frame { dt: TURN, commands };
                        host.broadcast(&Msg::Frame(self.next, f.clone()));
                        self.queue.push_back(f);
                        self.next += 1;
                    }
                }
            }
            Role::Client { conn } => {
                for m in conn.poll() {
                    match m {
                        Msg::Frame(n, f) if n == self.next => {
                            self.queue.push_back(f);
                            self.next += 1;
                            self.last_frame = Instant::now();
                        }
                        Msg::Chat { from, text } => chat.push((from, text)),
                        _ => {}
                    }
                }
                if !conn.alive {
                    self.lost = true;
                }
            }
        }
        chat
    }

    /// The frames whose time has come: one per TURN of game time, faster while frames
    /// pile up.
    pub fn due(&mut self, dt: f32) -> Vec<Frame> {
        self.owed = (self.owed + dt).min(TURN * 4.0);
        let mut out = Vec::new();
        while let Some(f) = self.queue.front() {
            if self.limit.is_some_and(|l| self.played + out.len() as u32 >= l) {
                break;
            }
            let behind = self.queue.len() > 3;
            if f.dt > self.owed && !behind {
                break;
            }
            self.owed = (self.owed - f.dt).max(0.0);
            out.push(self.queue.pop_front().unwrap());
        }
        self.played += out.len() as u32;
        if !out.is_empty() {
            if let Role::Client { conn } = &mut self.role {
                conn.send(&Msg::Played(self.played));
            }
        }
        out
    }

    /// The players the host waits on.
    pub fn lagging(&self) -> Vec<u8> {
        match &self.role {
            Role::Host { peers, .. } => peers.iter().filter(|p| self.next.saturating_sub(p.2) > BEHIND).map(|p| p.1).collect(),
            Role::Client { .. } => Vec::new(),
        }
    }

    /// A player waits on a host that has sent nothing for two seconds.
    pub fn host_silent(&self) -> bool {
        matches!(self.role, Role::Client { .. }) && self.queue.is_empty() && self.last_frame.elapsed() > Duration::from_secs(2)
    }

    /// The host drops the players it waits on: they leave the game.
    pub fn drop_lagging(&mut self) {
        let next = self.next;
        if let Role::Host { host, peers, .. } = &mut self.role {
            let gone: Vec<(u32, u8)> = peers.iter().filter(|p| next.saturating_sub(p.2) > BEHIND).map(|p| (p.0, p.1)).collect();
            for (id, player) in gone {
                host.drop_peer(id);
                peers.retain(|p| p.0 != id);
                self.left.push(player);
            }
        }
    }

    /// Leaves the game: the host closes it, a player says goodbye.
    pub fn leave(&mut self) {
        match &mut self.role {
            Role::Host { host, .. } => host.broadcast(&Msg::Leave),
            Role::Client { conn } => conn.send(&Msg::Leave),
        }
    }
}

/// Leaving the game (the scene's end) says so.
impl Drop for Session {
    fn drop(&mut self) {
        self.leave();
    }
}

/// The game port from the options.
pub fn port(options: &crate::options::Options) -> u16 {
    u16::try_from(options.netgameport).ok().filter(|&p| p > 0).unwrap_or(DEFAULT_PORT)
}
