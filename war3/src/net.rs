//! Local area network games, over Warcraft III's own protocol (W3GS, the `w3_net`
//! crate): the lobby's setup and the slot table everyone is sent, and a game under way.
//! In a game the host sends every player each turn: its length and each person's
//! commands.  Everyone, the host too, plays only these turns, and the game runs the
//! same for the same turns, so all play the same game; after each turn a player sends
//! the host its game's checksum, which the host compares with its own.  The commands
//! travel as this game's own (`Command`), not yet as the original's actions, so its
//! games are played with copies of itself.

use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

use w3_game::replay::{Command, Frame};
use w3_net::client::{self, Client};
use w3_net::host::{self, HOST_PID, Host};
use w3_net::packet::leave;
use w3_net::slot::{self, SlotTable, race, status};

/// The game port when the options name none (netgameport).
pub const DEFAULT_PORT: u16 = 6112;

/// Who holds a slot of the game setup.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Holder {
    Open,
    Closed,
    /// AI_DIFFICULTY_NEWBIE, _NORMAL, or _INSANE.
    Computer(i32),
    /// A person, by their player id in the protocol (the host is `HOST_PID`).
    Human(u8),
}

#[derive(Clone, Debug, PartialEq)]
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
    /// The map's own computer under fixed player settings: no one changes it.
    pub locked: bool,
}

/// The game setup everyone sees.  Its slots are the slot table's, in the map's player
/// order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Setup {
    pub game: String,
    pub map: String,
    pub slots: Vec<NetSlot>,
    /// Slow (0), normal (1), or fast (2).
    pub speed: u8,
    /// The advanced options (crate::advanced).
    pub flags: u32,
    /// A saved game is being loaded: only those who were in it may join, each to their
    /// own slot (LOAD_BATTLENET_INFO).
    pub saved: bool,
    /// The map fixes its players' races and colors (w3i flag 0x20).
    pub fixed: bool,
    /// The map's forces' names, each slot's team its force (w3i flag 0x40); empty
    /// without custom forces.
    pub forces: Vec<String>,
}

/// The race bits of RaceMenu's choices.
const RACES: [u8; 5] = [race::RANDOM, race::HUMAN, race::ORC, race::UNDEAD, race::NIGHT_ELF];

/// The settings word GAMEINFO carries: the advanced options are the game's map flags,
/// eight bits up, with the speed below them (1.27b's default fast game, 0x648 and 2, is
/// 0x64802).
pub fn settings(flags: u32, speed: u8) -> u32 {
    flags << 8 | speed as u32 & 3
}

/// The advanced options and speed from a settings word.
pub fn from_settings(word: u32) -> (u32, u8) {
    (word >> 8, (word & 3) as u8)
}

impl Setup {
    /// The setup as the slot table carries it, each slot's map download as `old` has it.
    pub fn table(&self, seed: u32, old: Option<&SlotTable>) -> SlotTable {
        let slots = self
            .slots
            .iter()
            .enumerate()
            .map(|(k, s)| slot::Slot {
                pid: match s.holder {
                    Holder::Human(pid) => pid,
                    _ => 0,
                },
                download: old.and_then(|t| t.slots.get(k)).filter(|o| matches!(s.holder, Holder::Human(pid) if pid == o.pid)).map_or(
                    match s.holder {
                        // As 1.27b sends them: the host has the map; a computer's is unknown.
                        Holder::Human(HOST_PID) => 100,
                        _ => slot::DOWNLOAD_UNKNOWN,
                    },
                    |o| o.download,
                ),
                status: match s.holder {
                    Holder::Open => status::OPEN,
                    Holder::Closed => status::CLOSED,
                    _ => status::OCCUPIED,
                },
                computer: matches!(s.holder, Holder::Computer(_)),
                team: s.team.clamp(0, 12) as u8,
                color: s.color.clamp(0, 12) as u8,
                race: RACES[s.race.min(4)] | if self.fixed { 0 } else { race::SELECTABLE },
                ai: match s.holder {
                    Holder::Computer(d) => d.clamp(0, 2) as u8,
                    _ => 1,
                },
                handicap: 100 - 10 * s.handicap.min(5) as u8,
            })
            .collect();
        let layout = match (self.forces.is_empty(), self.fixed) {
            (true, _) => slot::layout::MELEE,
            (false, true) => slot::layout::FIXED_PLAYER_SETTINGS,
            (false, false) => slot::layout::CUSTOM_FORCES,
        };
        let player_slots = self.slots.iter().filter(|s| s.team != crate::lan::OBSERVERS).count() as u8;
        SlotTable { slots, seed, layout, player_slots }
    }

    /// Takes the slot table's choices into the setup's slots (the same map's, so the
    /// same slots), the people named as `players` names them.
    pub fn take_table(&mut self, table: &SlotTable, players: &[(u8, String)]) {
        for (s, t) in self.slots.iter_mut().zip(&table.slots) {
            s.holder = match t.status {
                status::OPEN => Holder::Open,
                status::CLOSED => Holder::Closed,
                _ if t.computer => Holder::Computer(t.ai as i32),
                _ => Holder::Human(t.pid),
            };
            s.name = match s.holder {
                Holder::Human(pid) => players.iter().find(|p| p.0 == pid).map(|p| p.1.clone()).unwrap_or_default(),
                // A loaded game's open slot is kept for its player, by name.
                Holder::Open if self.saved => std::mem::take(&mut s.name),
                _ => String::new(),
            };
            s.team = t.team as i32;
            s.color = t.color as i32;
            s.race = RACES.iter().position(|&r| t.race & r != 0).unwrap_or(0);
            s.handicap = (100u8.saturating_sub(t.handicap) / 10).min(5) as usize;
        }
    }

    /// The person's slot.
    pub fn slot_of(&self, pid: u8) -> Option<&NetSlot> {
        self.slots.iter().find(|s| s.holder == Holder::Human(pid))
    }
}

/// How a network game plays: as the host, which makes the turns, or as a player.
pub enum Link {
    Host(Box<Host>),
    Client(Box<Client>),
}

/// A network game's connection and its people: each one's protocol id and player.
pub struct Role {
    pub link: Link,
    pub players: Vec<(u8, u8)>,
}

impl std::fmt::Debug for Role {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(match self.link {
            Link::Host(_) => "Host",
            Link::Client(_) => "Client",
        })
    }
}

/// A turn's commands as one person's actions: each with the player who gave it.
fn encode(commands: &[(u8, Command)]) -> Vec<u8> {
    postcard::to_allocvec(commands).unwrap_or_default()
}

fn decode(data: &[u8]) -> Vec<(u8, Command)> {
    postcard::from_bytes(data).unwrap_or_else(|e| {
        log::warn!("net: unreadable actions: {e}");
        Vec::new()
    })
}

/// A network game under way.
pub struct Session {
    pub role: Role,
    /// The host's: each person's actions for the next turn, and each player's turns
    /// played (keepalives).
    pending: Vec<(u8, Vec<u8>)>,
    played_by: HashMap<u8, u32>,
    /// Frames to play, in order, and the number of the next to come.
    pub queue: VecDeque<Frame>,
    pub next: u32,
    pub played: u32,
    /// Game time owed to frames not yet played.
    pub owed: f32,
    /// The host's time toward its next turn.
    clock: f32,
    /// The connection to the host is gone.
    pub lost: bool,
    /// Players who left, for the next turn to say so.
    pub left: Vec<u8>,
    /// Waiting on a player who has fallen behind (or not yet loaded).
    pub waiting: bool,
    /// The players the host says the game waits on (StartLag), for a player's game.
    lag: Vec<u8>,
    /// Frames are played up to this many (for tests that compare games).
    pub limit: Option<u32>,
    /// When the last frame came (a player's game waits on the host after two seconds).
    last_frame: Instant,
    /// The host's own checksums by turn, players' not yet compared, and players dropped
    /// for being out of step.
    own_checks: HashMap<u32, u32>,
    peer_checks: Vec<(u8, u32, u32)>,
    pub desynced: Vec<u8>,
}

/// A turn is 50 ms of game time; the host waits for any player more than 40 behind.
pub const TURN: f32 = 0.05;
const BEHIND: u32 = 40;

impl Session {
    pub fn new(mut role: Role) -> Session {
        match &mut role.link {
            Link::Host(h) => h.loaded(),
            Link::Client(c) => c.loaded(),
        }
        let played_by = role.players.iter().filter(|p| p.0 != HOST_PID).map(|p| (p.0, 0)).collect();
        Session {
            role,
            pending: Vec::new(),
            played_by,
            queue: VecDeque::new(),
            next: 0,
            played: 0,
            owed: 0.0,
            clock: 0.0,
            lost: false,
            left: Vec::new(),
            waiting: false,
            lag: Vec::new(),
            limit: None,
            last_frame: Instant::now(),
            own_checks: HashMap::new(),
            peer_checks: Vec::new(),
            desynced: Vec::new(),
        }
    }

    fn player_of(&self, pid: u8) -> Option<u8> {
        self.role.players.iter().find(|p| p.0 == pid).map(|p| p.1)
    }

    fn name_of(&self, pid: u8) -> String {
        let players = match &self.role.link {
            Link::Host(h) => h.players(),
            Link::Client(c) => c.players.clone(),
        };
        players.into_iter().find(|p| p.0 == pid).map(|p| p.1).unwrap_or_default()
    }

    /// The local player's commands: the host keeps its own for the next turn; a player
    /// sends them to the host.
    pub fn send_commands(&mut self, local: u8, commands: Vec<Command>) {
        if commands.is_empty() {
            return;
        }
        let commands: Vec<(u8, Command)> = commands.into_iter().map(|c| (local, c)).collect();
        match &mut self.role.link {
            Link::Host(_) => self.pending.push((HOST_PID, encode(&commands))),
            Link::Client(c) => c.send_actions(encode(&commands)),
        }
    }

    /// Takes in what has come; the host makes turns as game time passes.  Returns the
    /// chat lines that came (name and text).
    pub fn pump(&mut self, dt: f32) -> Vec<(String, String)> {
        let mut chat = Vec::new();
        match &mut self.role.link {
            Link::Host(h) => {
                let events = h.poll();
                for e in events {
                    match e {
                        host::Event::Actions { pid, data } => self.pending.push((pid, data)),
                        host::Event::Keepalive { pid, checksum } => {
                            let n = self.played_by.entry(pid).or_default();
                            *n += 1;
                            let n = *n;
                            self.peer_checks.push((pid, n, checksum));
                        }
                        host::Event::Left { pid, .. } => {
                            self.played_by.remove(&pid);
                            if let Some(p) = self.player_of(pid) {
                                self.left.push(p);
                            }
                        }
                        host::Event::Chat { from, chat: w3_net::Chat::Game { text, .. } } => chat.push((self.name_of(from), text)),
                        _ => {}
                    }
                }
                self.host_turns(dt);
            }
            Link::Client(c) => {
                for e in c.poll() {
                    match e {
                        client::Event::Turn { interval, actions } => {
                            let mut commands = Vec::new();
                            for (pid, data) in actions {
                                let player = self.role.players.iter().find(|p| p.0 == pid).map(|p| p.1);
                                // A person's commands are their own; the host's may speak for
                                // anyone (a player who left).
                                commands.extend(decode(&data).into_iter().filter(|(p, _)| pid == HOST_PID || Some(*p) == player));
                            }
                            self.queue.push_back(Frame { dt: interval as f32 / 1000.0, commands });
                            self.next += 1;
                            self.last_frame = Instant::now();
                        }
                        client::Event::StartLag(players) => {
                            self.lag = players.iter().filter_map(|&(pid, _)| self.role.players.iter().find(|p| p.0 == pid).map(|p| p.1)).collect();
                        }
                        client::Event::StopLag { pid } => {
                            if let Some(p) = self.role.players.iter().find(|p| p.0 == pid).map(|p| p.1) {
                                self.lag.retain(|&q| q != p);
                            }
                        }
                        client::Event::Chat { from, chat: w3_net::Chat::Game { text, .. } } => {
                            let name = c.players.iter().find(|p| p.0 == from).map(|p| p.1.clone()).unwrap_or_default();
                            chat.push((name, text));
                        }
                        client::Event::Desync(_) => log::warn!("net: the host finds this game out of step"),
                        client::Event::Closed => self.lost = true,
                        _ => {}
                    }
                }
                if !c.alive() {
                    self.lost = true;
                }
            }
        }
        if !self.peer_checks.is_empty() {
            self.compare_checks();
        }
        chat
    }

    /// The host's turns: one each TURN of game time, unless it waits on a player not yet
    /// loaded or fallen too far behind (StartLag tells the players whom, StopLag when it
    /// no longer waits on one).
    fn host_turns(&mut self, dt: f32) {
        let Link::Host(h) = &mut self.role.link else { return };
        let behind: Vec<u8> = self.played_by.iter().filter(|(_, n)| self.next.saturating_sub(**n) > BEHIND).map(|(&pid, _)| pid).collect();
        let waiting = !h.all_loaded() || !behind.is_empty();
        if h.all_loaded() && behind != self.lag {
            if !behind.is_empty() {
                h.broadcast(&w3_net::Packet::StartLag(behind.iter().map(|&pid| (pid, 0)).collect()));
            }
            for &pid in self.lag.iter().filter(|p| !behind.contains(p)) {
                h.broadcast(&w3_net::Packet::StopLag { pid, waited: 0 });
            }
            self.lag = behind;
        }
        self.waiting = waiting;
        if waiting {
            return;
        }
        self.clock += dt;
        while self.clock >= TURN {
            self.clock -= TURN;
            // Those who left: the host's own actions say so.
            let left: Vec<(u8, Command)> = self.left.drain(..).map(|p| (p, Command::PlayerLeft { player: p })).collect();
            if !left.is_empty() {
                self.pending.push((HOST_PID, encode(&left)));
            }
            // Each person's commands as one block.
            let mut blocks: Vec<(u8, Vec<(u8, Command)>)> = Vec::new();
            for (pid, data) in std::mem::take(&mut self.pending) {
                let player = self.role.players.iter().find(|p| p.0 == pid).map(|p| p.1);
                let commands = decode(&data).into_iter().filter(|(p, _)| pid == HOST_PID || Some(*p) == player);
                match blocks.iter_mut().find(|b| b.0 == pid) {
                    Some(b) => b.1.extend(commands),
                    None => blocks.push((pid, commands.collect())),
                }
            }
            let commands: Vec<(u8, Command)> = blocks.iter().flat_map(|b| b.1.iter().cloned()).collect();
            let actions = blocks.iter().map(|(pid, c)| (*pid, encode(c))).collect();
            h.send_turn((TURN * 1000.0) as u16, actions);
            self.queue.push_back(Frame { dt: TURN, commands });
            self.next += 1;
        }
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
        out
    }

    /// The players the game waits on.
    pub fn lagging(&self) -> Vec<u8> {
        match &self.role.link {
            Link::Host(_) => self.lag.iter().filter_map(|&pid| self.player_of(pid)).collect(),
            Link::Client(_) => self.lag.clone(),
        }
    }

    /// A player waits on a host that has sent nothing for two seconds.
    pub fn host_silent(&self) -> bool {
        matches!(self.role.link, Link::Client(_)) && self.queue.is_empty() && self.last_frame.elapsed() > Duration::from_secs(2)
    }

    /// The host drops the players it waits on: they leave the game.
    pub fn drop_lagging(&mut self) {
        let Link::Host(h) = &mut self.role.link else { return };
        for pid in std::mem::take(&mut self.lag) {
            h.drop_player(pid, leave::DISCONNECT);
            self.played_by.remove(&pid);
            if let Some(p) = self.role.players.iter().find(|p| p.0 == pid).map(|p| p.1) {
                self.left.push(p);
            }
        }
    }

    /// Notes this computer's game checksum after turn `frame` (counted from 1): a player
    /// sends it to the host (its keepalive); the host keeps it and drops each player
    /// whose checksum for the turn differs (their games have parted).
    pub fn check(&mut self, frame: u32, sum: u64) {
        let sum = (sum ^ sum >> 32) as u32;
        match &mut self.role.link {
            Link::Client(c) => c.keepalive(sum),
            Link::Host(_) => {
                self.own_checks.insert(frame, sum);
                self.compare_checks();
            }
        }
    }

    fn compare_checks(&mut self) {
        let mut keep = Vec::new();
        for (pid, frame, sum) in std::mem::take(&mut self.peer_checks) {
            match self.own_checks.get(&frame) {
                Some(&mine) if mine != sum => {
                    let player = self.player_of(pid);
                    log::warn!("net: player {player:?} is out of step at turn {frame}");
                    if let Link::Host(h) = &mut self.role.link {
                        h.drop_player(pid, leave::DISCONNECT);
                    }
                    self.played_by.remove(&pid);
                    if let Some(p) = player {
                        self.left.push(p);
                        self.desynced.push(p);
                    }
                }
                Some(_) => {}
                None => keep.push((pid, frame, sum)),
            }
        }
        self.peer_checks = keep;
        // Old checksums are no longer needed once everyone is past them.
        let oldest = self.played_by.values().copied().min().unwrap_or(self.next);
        self.own_checks.retain(|&f, _| f + 1000 >= oldest);
    }

    /// Leaves the game: the host closes it, a player says goodbye.
    pub fn leave(&mut self) {
        match &mut self.role.link {
            Link::Host(h) => h.close(),
            Link::Client(c) => c.leave(leave::DISCONNECT),
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
