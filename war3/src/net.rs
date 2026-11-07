//! Local area network games, over Warcraft III's own protocol (W3GS, the `w3_net`
//! crate): the lobby's setup and the slot table everyone is sent, and a game under way.
//! In a game the host sends every player each turn: its length and each person's
//! actions, as the original's (`w3_game::actions::Codec`).  Everyone, the host too,
//! plays only these turns, and the game runs the same for the same turns, so all play
//! the same game; after each turn a player sends the host its game's checksum (the
//! original's keepalive), which the host compares with its own and the others'.  A
//! player who leaves is said by the host's PLAYERLEFT, which every computer takes
//! before the next turn; chat goes to its recipients only, as the original sends it.

use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

use w3_game::replay::{Command, Frame, Leave};
use w3_game::world::World;
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

/// A slot's race flags as the race menu's index (random where none is set).
pub fn race_index(flags: u8) -> usize {
    RACES.iter().position(|&r| flags & r != 0).unwrap_or(0)
}

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
            s.race = race_index(t.race);
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

/// A network game's connection, its people (each one's protocol id and player), and the
/// slot table's seed, which every computer's game starts its generators from.
pub struct Role {
    pub link: Link,
    pub players: Vec<(u8, u8)>,
    pub seed: u32,
    /// The game's name, as its replay's record keeps it.
    pub game: String,
    /// The game's setup, as its replay's record copies it.
    pub record: w3_game::replay::NetRecord,
}

impl std::fmt::Debug for Role {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(match self.link {
            Link::Host(_) => "Host",
            Link::Client(_) => "Client",
        })
    }
}


/// The keepalive's data modules for a game on the archives as mounted (the map's over
/// the data set's), as the original loads them (`w3_game::datahash::modules`).
pub fn data_modules(chain: &w3_mpq::Chain) -> std::collections::BTreeMap<u8, u32> {
    w3_game::datahash::modules(|n| chain.read(n).unwrap_or_default()).into_iter().collect()
}

/// A turn's blocks as a frame: each person's actions as their player's, which the game
/// reads as it plays the frame.
fn frame(players: &[(u8, u8)], ms: u16, blocks: &[(u8, Vec<u8>)]) -> Frame {
    let actions = blocks.iter().filter_map(|(pid, data)| players.iter().find(|p| p.0 == *pid).map(|p| (p.1, data.clone()))).collect();
    Frame { dt: ms as f32 / 1000.0, actions: Some(actions), turn: Some((ms, blocks.to_vec())), ..Default::default() }
}

/// The people a chat line goes to, by protocol id, as the original's sender picks them
/// (`FUN_6f36ee10`, `FUN_6f32ca60`): to all (scope 0), everyone else in the game; to
/// allies (1), the players still playing whom the speaker is allied with (IsPlayerAlly's
/// alliance); to observers (2), the observers; to a player (3 + the player), that one.
/// The speaker and players without a person (computers) are left out.
pub fn recipients(w: &World, people: &[(u8, u8)], speaker: u8, scope: u8) -> Vec<u8> {
    use w3_game::types::{player_state, slot_state};
    let other = |p: &u8| *p != speaker && (*p as usize) < 12;
    let state = |p: u8| w.players.get(p as usize).map_or(slot_state::EMPTY, |pl| pl.slot_state);
    let players: Vec<u8> = match scope {
        0 => people.iter().map(|x| x.1).filter(other).collect(),
        1 => (0..12).filter(other).filter(|&p| state(p) == slot_state::PLAYING && w3_game::natives::players::is_ally(w, speaker, p)).collect(),
        2 => (0..12).filter(other).filter(|&p| state(p) != slot_state::EMPTY && w.players[p as usize].get_state(player_state::OBSERVER) != 0).collect(),
        n => vec![n - 3],
    };
    players.into_iter().filter_map(|p| people.iter().find(|x| x.1 == p).map(|x| x.0)).collect()
}

/// What came, to the host or to a player.
enum Heard {
    Host(host::Event),
    Player(client::Event),
}

/// A network game under way.
pub struct Session {
    pub role: Role,
    /// The host's: each person's actions for the next turn, and each player's turns
    /// played (keepalives).
    pending: Vec<(u8, Vec<u8>)>,
    /// The local player's commands as the original's actions (`actions::Codec`).
    codec: w3_game::actions::Codec,
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
    /// The people who left (protocol ids).
    gone: Vec<u8>,
    /// The leaves and chat lines that came since the last turn, for the next turn's
    /// frame (the host's next, or the next the host sends a player): each computer takes
    /// them before that turn, as the original's net client does its messages.
    leaves: Vec<Leave>,
    chat: Vec<(u8, u32, String)>,
    /// Waiting on a player who has fallen behind (or not yet loaded).
    pub waiting: bool,
    /// The players the host says the game waits on (StartLag), for a player's game.
    lag: Vec<u8>,
    /// Frames are played up to this many (for tests that compare games).
    pub limit: Option<u32>,
    /// When the last frame came (a player's game waits on the host after two seconds).
    last_frame: Instant,
    /// The host's keepalives by turn, each person's (the host's own among them) as they
    /// come, and the players found out of step.
    checks: std::collections::BTreeMap<u32, Vec<(u8, u32)>>,
    pub desynced: Vec<u8>,
    /// The word this computer leaves with (`leave_word`), and whether the script ended
    /// the game for its player (EndGame: the victory or defeat dialog).
    pub word: u32,
    pub over: bool,
}

/// The word a player leaves the game with (LEAVEGAME), as the original's game gives it
/// (`FUN_6f32c130`'s first argument): LOST through the game menu (`FUN_6f29f430` and
/// the menu's others), even after a victory or a defeat, and OBSERVER through the
/// victory or defeat dialog (the script's EndGame: `over`) or as an observer
/// (`FUN_6f1de050`).  Measured: both, after a victory and after a defeat, as the
/// original's replays' closing leave record has them; as an observer provisional.
pub fn leave_word(w: &World, over: bool) -> u32 {
    use w3_game::types::player_state;
    let observer = w.players.get(w.local_player as usize).is_some_and(|p| p.get_state(player_state::OBSERVER) != 0);
    if over || observer { leave::OBSERVER } else { leave::LOST }
}

/// The host makes a turn each 100 ms (the LAN provider's, `FUN_6f974710`; 250 over
/// Battle.net).  The original's each say the time since the last (`FUN_6f99e310`: the
/// tick count, 1 to 400 ms; measured 100, 99, 95, 86), but its game takes game time in
/// whole 25 ms ticks, the rest kept for the next turn (measured: its keepalives' time,
/// 175 after turns of 100 and 99, and 100, 200, 300 where the port's host sent 113, 203,
/// 314), which the port's does not yet, so the port's turns are 100 ms each.
pub const TURN: f32 = 0.1;
/// The players a desync leaves out go as anything more comes from them, or after 2 s
/// (`FUN_6f998880`, `Host::drop_after`).
const DESYNC_DROP: Duration = Duration::from_millis(2000);

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
            codec: Default::default(),
            played_by,
            queue: VecDeque::new(),
            next: 0,
            played: 0,
            owed: 0.0,
            clock: 0.0,
            lost: false,
            gone: Vec::new(),
            leaves: Vec::new(),
            chat: Vec::new(),
            waiting: false,
            lag: Vec::new(),
            limit: None,
            last_frame: Instant::now(),
            checks: Default::default(),
            desynced: Vec::new(),
            word: leave::LOST,
            over: false,
        }
    }

    fn player_of(&self, pid: u8) -> Option<u8> {
        self.role.players.iter().find(|p| p.0 == pid).map(|p| p.1)
    }

    /// This computer's protocol id.
    fn local_pid(&self) -> u8 {
        match &self.role.link {
            Link::Host(_) => HOST_PID,
            Link::Client(c) => c.pid,
        }
    }

    /// The people still in the game, by protocol id and player.
    fn people(&self) -> Vec<(u8, u8)> {
        self.role.players.iter().copied().filter(|p| !self.gone.contains(&p.0)).collect()
    }

    /// A person left, as the host's PLAYERLEFT said (with its word): the next turn's
    /// frame says so.
    fn left(&mut self, pid: u8, word: u32) {
        if self.gone.contains(&pid) {
            return;
        }
        self.gone.push(pid);
        self.played_by.remove(&pid);
        self.lag.retain(|&p| p != pid);
        if let Some(player) = self.player_of(pid) {
            self.leaves.push(Leave { pid, player, word });
        }
    }

    /// The local player's commands: the host keeps its own for the next turn; a player
    /// sends them to the host.
    /// They go as the original's actions (`actions::Codec`), as the world stands.
    /// A chat line goes as the game's chat to its recipients only (`recipients`), its
    /// trigger events as actions; the lines are returned for this computer to show.
    pub fn send_commands(&mut self, w: &World, local: u8, commands: Vec<Command>) -> Vec<(u8, String, u8)> {
        let mut said = Vec::new();
        for c in &commands {
            if let Command::Chat { text, to, .. } = c {
                let chat = w3_net::Chat::Game { scope: *to as u32, text: text.clone() };
                let to_pids = recipients(w, &self.people(), local, *to);
                if !to_pids.is_empty() {
                    match &mut self.role.link {
                        Link::Host(h) => h.chat(&to_pids, chat),
                        Link::Client(c) => c.chat_to(to_pids, chat),
                    }
                }
                self.chat.push((self.local_pid(), *to as u32, text.clone()));
                said.push((local, text.clone(), *to));
            }
        }
        let data: Vec<u8> = commands.iter().flat_map(|c| self.codec.encode(w, local, c)).collect();
        if data.is_empty() {
            return said;
        }
        match &mut self.role.link {
            Link::Host(_) => self.pending.push((HOST_PID, data)),
            Link::Client(c) => c.send_actions(data),
        }
        said
    }

    /// Takes in what has come; the host makes turns as game time passes.  Returns the
    /// chat lines that came (player, text, and scope).
    pub fn pump(&mut self, dt: f32) -> Vec<(u8, String, u8)> {
        let mut chat = Vec::new();
        let events: Vec<Heard> = match &mut self.role.link {
            Link::Host(h) => h.poll().into_iter().map(Heard::Host).collect(),
            Link::Client(c) => c.poll().into_iter().map(Heard::Player).collect(),
        };
        for e in events {
            match e {
                Heard::Host(host::Event::Actions { pid, data }) => self.pending.push((pid, data)),
                Heard::Host(host::Event::Keepalive { pid, checksum }) => {
                    let n = self.played_by.entry(pid).or_default();
                    *n += 1;
                    let n = *n;
                    self.checks.entry(n).or_default().push((pid, checksum));
                }
                Heard::Host(host::Event::Left { pid, reason }) | Heard::Player(client::Event::PlayerLeft { pid, reason }) => self.left(pid, reason),
                Heard::Host(host::Event::Chat { from, chat: w3_net::Chat::Game { text, scope } }) | Heard::Player(client::Event::Chat { from, chat: w3_net::Chat::Game { text, scope } }) => {
                    if let Some(p) = self.player_of(from) {
                        self.chat.push((from, scope, text.clone()));
                        chat.push((p, text, scope as u8));
                    }
                }
                Heard::Player(client::Event::Turn { interval, actions }) => {
                    let mut f = frame(&self.role.players, interval, &actions);
                    f.leaves = std::mem::take(&mut self.leaves);
                    f.chat = std::mem::take(&mut self.chat);
                    self.queue.push_back(f);
                    self.next += 1;
                    self.last_frame = Instant::now();
                }
                Heard::Player(client::Event::StartLag(players)) => self.lag = players.iter().map(|&(pid, _)| pid).collect(),
                Heard::Player(client::Event::StopLag { pid }) => self.lag.retain(|&q| q != pid),
                Heard::Player(client::Event::Desync { turn, players }) => log::warn!("net: the host finds the games out of step at turn {turn}; this one's agrees with {players:?}"),
                Heard::Player(client::Event::Kicked { reason }) => log::warn!("net: the host turned this player out ({reason:#x})"),
                Heard::Player(client::Event::Closed) => self.lost = true,
                _ => {}
            }
        }
        if matches!(self.role.link, Link::Host(_)) {
            self.host_turns(dt);
        } else if let Link::Client(c) = &self.role.link {
            self.lost |= !c.alive();
        }
        if !self.checks.is_empty() {
            self.compare_checks();
        }
        chat
    }

    /// The host's turns: one each TURN of game time, unless it waits on players not yet
    /// loaded or fallen behind (the host's lag screen, `Host::lagging`).
    fn host_turns(&mut self, dt: f32) {
        let Link::Host(h) = &mut self.role.link else { return };
        self.waiting = !h.all_loaded() || !h.lagging().is_empty();
        if self.waiting {
            return;
        }
        self.clock += dt;
        if self.clock >= TURN {
            let ms = (TURN * 1000.0) as u16;
            // (Time past a turn more is not made up.)
            self.clock = (self.clock - TURN).min(TURN);
            // Each person's actions as one block.
            let mut blocks: Vec<(u8, Vec<u8>)> = Vec::new();
            for (pid, data) in std::mem::take(&mut self.pending) {
                match blocks.iter_mut().find(|b| b.0 == pid) {
                    Some(b) => b.1.extend(data),
                    None => blocks.push((pid, data)),
                }
            }
            let mut f = frame(&self.role.players, ms, &blocks);
            f.leaves = std::mem::take(&mut self.leaves);
            f.chat = std::mem::take(&mut self.chat);
            h.send_turn(ms, blocks);
            self.queue.push_back(f);
            self.next += 1;
        }
    }

    /// The frames whose time has come, each as its length of game time passes, faster
    /// while frames pile up.
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
        let pids = match &self.role.link {
            Link::Host(h) => h.lagging(),
            Link::Client(_) => self.lag.clone(),
        };
        pids.into_iter().filter_map(|pid| self.player_of(pid)).collect()
    }

    /// A player waits on a host that has sent nothing for two seconds.
    pub fn host_silent(&self) -> bool {
        matches!(self.role.link, Link::Client(_)) && self.queue.is_empty() && self.last_frame.elapsed() > Duration::from_secs(2)
    }

    /// The lag screen's Drop: the host's own vote, or a player's sent to the host
    /// (DROPREQ); enough votes drop the players waited on (`Host::vote`).
    pub fn drop_lagging(&mut self) {
        match &mut self.role.link {
            Link::Host(h) => h.vote(HOST_PID),
            Link::Client(c) => c.drop_vote(),
        }
    }

    /// Notes this computer's game checksum after turn `frame` (counted from 1): a player
    /// sends it to the host (its keepalive); the host keeps it with the players' to
    /// compare. A turn of zero milliseconds has no keepalive on the wire.
    pub fn check(&mut self, frame: u32, dt: f32, sum: u64) {
        if (dt * 1000.0).round() as u32 == 0 {
            return;
        }
        let sum = (sum ^ sum >> 32) as u32;
        log::debug!("net: keepalive {frame} {sum:08x}");
        match &mut self.role.link {
            Link::Client(c) => c.keepalive(sum),
            Link::Host(_) => {
                self.checks.entry(frame).or_default().push((HOST_PID, sum));
                self.compare_checks();
            }
        }
    }

    /// Compares each turn's keepalives once every person still in the game has sent
    /// theirs.  Games that have parted are grouped by their checksum, as the original's
    /// router does (`FUN_6f999b50`): each player is told (DESYNC) the others in their
    /// group, themselves first in the host's (the group of the router's first player,
    /// however small: measured, two of the port's players agreeing were told apart from
    /// the original host), and the other groups' players go (`DESYNC_DROP`).
    fn compare_checks(&mut self) {
        // (Each turn that differs is told, until those out of step are gone: measured,
        // the original's 0x0E for turns 1 and 2 to a player answering checksum 0.)
        let people: Vec<u8> = std::iter::once(HOST_PID).chain(self.played_by.keys().copied()).collect();
        let complete: Vec<u32> = self.checks.iter().filter(|(_, sums)| people.iter().all(|p| sums.iter().any(|s| s.0 == *p))).map(|(&n, _)| n).collect();
        for n in complete {
            let sums: Vec<(u8, u32)> = self.checks.remove(&n).unwrap_or_default().into_iter().filter(|s| people.contains(&s.0)).collect();
            if sums.iter().all(|s| s.1 == sums[0].1) {
                continue;
            }
            let mut groups: Vec<(u32, Vec<u8>)> = Vec::new();
            for &(pid, sum) in &sums {
                match groups.iter_mut().find(|g| g.0 == sum) {
                    Some(g) => g.1.push(pid),
                    None => groups.push((sum, vec![pid])),
                }
            }
            let main = groups.iter().position(|g| g.1.contains(&HOST_PID)).unwrap_or(0);
            log::warn!("net: the games are out of step at turn {n}: {groups:x?}");
            if let Link::Host(h) = &mut self.role.link {
                for (k, (sum, members)) in groups.iter().enumerate() {
                    for &pid in members.iter().filter(|&&p| p != HOST_PID) {
                        let others = members.iter().copied().filter(|&p| p != pid);
                        let players: Vec<u8> = if k == main { std::iter::once(pid).chain(others).collect() } else { others.collect() };
                        h.send(pid, &w3_net::Packet::Desync { turn: n, checksum: sum.to_le_bytes().to_vec(), players });
                        if k != main {
                            h.drop_after(pid, DESYNC_DROP);
                        }
                    }
                }
            }
            let out: Vec<u8> = groups.iter().enumerate().filter(|(k, _)| *k != main).flat_map(|(_, g)| g.1.clone()).filter_map(|pid| self.player_of(pid)).collect();
            for p in out {
                if !self.desynced.contains(&p) {
                    self.desynced.push(p);
                }
            }
        }
        // Turns no one still in the game has yet to send are not kept.
        let oldest = self.played_by.values().copied().min().unwrap_or(self.next);
        self.checks.retain(|&f, _| f + 1000 >= oldest);
    }

    /// Leaves the game: the host closes it, a player says goodbye with `word`.
    pub fn leave(&mut self) {
        let word = self.word;
        match &mut self.role.link {
            Link::Host(h) => h.close(),
            Link::Client(c) => c.leave(word),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_time_turns_send_no_keepalive() {
        use w3_net::{GameInfo, GameStat, Packet, client::Client, conn::Conn};
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let addr = listener.local_addr().unwrap();
        let info = GameInfo {
            product: *b"PX3W", version: 27, host_counter: 0, entry_key: 0,
            name: "test".into(),
            stat: GameStat { settings: 0, map_width: 0, map_height: 0, map_xoro: 0,
                map_path: String::new(), host: String::new(), map_sha1: [0; 20] },
            slots_total: 2, game_type: 1, players: 1, player_slots: 2, uptime: 0, port: addr.port(),
        };
        let client = Client::join(addr, &info, "test", 0).unwrap();
        let mut server = Conn::new(listener.accept().unwrap().0).unwrap();
        let mut session = Session::new(Role { link: Link::Client(Box::new(client)),
            players: Vec::new(), seed: 0, game: "test".into(), record: Default::default() });
        session.check(1, 0.0, 0xdead);
        session.check(2, 0.1, 0x1234);
        session.check(3, 0.0, 0xbeef);
        session.check(4, 0.1, 0x5678);
        // A packet after the checks delimits the wire observation without a sleep
        // or an assertion based on the absence of traffic within a short timeout.
        if let Link::Client(c) = &mut session.role.link {
            c.send_actions(vec![0x02]);
        }
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut sums = Vec::new();
        let mut finished = false;
        while !finished && Instant::now() < deadline {
            for packet in server.poll_wait(deadline.saturating_duration_since(Instant::now())) {
                match packet {
                    Packet::OutgoingKeepalive { leftover, checksum } => sums.push((leftover, checksum)),
                    Packet::OutgoingAction(_) => finished = true,
                    _ => {}
                }
            }
        }
        server.close();
        assert!(finished, "the final packet did not arrive");
        assert_eq!(sums, [(0, 0x1234), (4, 0x5678)]);
    }
}
