//! Replays: what the players did, step by step.  Every command from the interface goes
//! through `Game::apply`, which notes it; each step of the game (`Game::tick`) closes a
//! frame of the step's length and the commands before it.  A replay plays the frames
//! back on the same map with the same setup: the commands of each frame, then the step,
//! so the game, which runs the same way for the same inputs, plays out as it did.

use serde::{Deserialize, Serialize};

use crate::objects::Id;

/// A game cache entry's value, as SyncStored* sends it (none when the sender lacked it).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum CacheValue {
    Int(Option<i32>),
    Real(Option<f32>),
    Bool(Option<bool>),
    Str(Option<String>),
    Unit(Option<crate::types::StoredUnit>),
}

/// A player's command, as the interface gives it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Command {
    Order { unit: u32, order: i32, x: f32, y: f32, target: u32 },
    /// An order that queues behind the current ones when `queue`.
    Issue { unit: u32, order: i32, x: f32, y: f32, target: u32, queue: bool },
    Build { worker: u32, id: Id, x: f32, y: f32 },
    Train { building: u32, id: Id },
    Research { building: u32, id: Id },
    Upgrade { building: u32, id: Id },
    CancelLast { building: u32 },
    LearnSkill { hero: u32, id: Id },
    BuyItem { player: u8, shop: u32, id: Id },
    HireUnit { player: u8, shop: u32, id: Id },
    StartRevive { altar: u32, hero: u32 },
    /// A tavern's Awaken of a player's dead hero.
    Awaken { player: u8, tavern: u32, hero: u32 },
    UseItem { unit: u32, slot: usize },
    UseItemAt { unit: u32, slot: usize, target: u32, x: f32, y: f32 },
    MoveItem { unit: u32, from: usize, to: usize },
    DropItemAt { unit: u32, slot: usize, x: f32, y: f32 },
    GiveItemTo { unit: u32, slot: usize, target: u32 },
    Select { player: u8, units: Vec<u32> },
    /// A selection action as the original keeps it in step (`crate::selection`).
    SyncSelection { player: u8, op: crate::selection::SelOp },
    PlayerEvent { player: u8, event: i32 },
    DialogButton { player: u8, button: u32 },
    /// A chat line, to everyone (0), allies (1), observers (2), or a player (3 on).
    Chat { player: u8, text: String, to: u8 },
    /// A network game's player left.
    PlayerLeft { player: u8 },
    /// A player's mouse hit (clicked) or tracked (moved onto) a trackable.
    Trackable { player: u8, trackable: u32, hit: bool },
    /// A player opened the build menu (or the hero skill menu).
    Submenu { player: u8, build: bool },
    /// A player's minimap signal to their allies (CNetCommandAllyPing, 5 seconds).
    Signal { player: u8, x: f32, y: f32 },
    /// A player's alliance kinds toward another (CNetCommandTeamChangeAlliance: bits 0
    /// to 6 of the alliance types).
    Alliance { player: u8, other: u8, flags: u32 },
    /// A player sends another gold and lumber (CNetCommandTeamTradeResources).
    Trade { player: u8, other: u8, gold: i32, lumber: i32 },
    /// The units just ordered to a point move as a group in formation (the order flag
    /// 0x10 of FUN_6f394600): at the speed of the slowest.
    Formation { units: Vec<u32> },
    /// Cancels one item of a structure's queue, by its place (0 the one in progress).
    CancelQueue { building: u32, slot: usize },
    /// Saves a network game on every computer at once, under this name (the game
    /// menu's Save Game in a network game, kept in Save\\Multiplayer).
    Save { name: String },
    /// A player pauses or resumes a network game (FUN_6f3719c0).
    Pause { player: u8, on: bool },
    /// A game cache value one computer had, for all (SyncStoredInteger and the rest).
    CacheValue { cache: String, mission: String, key: String, value: CacheValue },
    /// An order to the player's selection, as the original sends one (its order
    /// actions): each selected unit the player commands takes it, at a point (`at`), on
    /// a target, or neither; queued behind its orders when `queue`; those sent to a
    /// point in formation when `formation`.
    SelectionOrder { player: u8, order: i32, at: Option<(f32, f32)>, target: u32, queue: bool, formation: bool },
    /// One event registration hears a player's input, as the original sends it: a chat
    /// line (`text`) a chat event registration matches, or a trackable's hit or track.
    EventHeard { player: u8, event: u32, text: String },
}

impl Command {
    /// The unit (or structure) a command orders, when it orders one.
    pub fn unit(&self) -> Option<u32> {
        use Command as C;
        match self {
            C::Order { unit, .. }
            | C::Issue { unit, .. }
            | C::UseItem { unit, .. }
            | C::UseItemAt { unit, .. }
            | C::MoveItem { unit, .. }
            | C::DropItemAt { unit, .. }
            | C::GiveItemTo { unit, .. } => Some(*unit),
            C::Build { worker, .. } => Some(*worker),
            C::Train { building, .. } | C::Research { building, .. } | C::Upgrade { building, .. } | C::CancelLast { building } | C::CancelQueue { building, .. } => Some(*building),
            C::LearnSkill { hero, .. } => Some(*hero),
            C::StartRevive { altar, .. } => Some(*altar),
            _ => None,
        }
    }

    /// The player a command speaks for, when it names one.
    pub fn player(&self) -> Option<u8> {
        use Command as C;
        match self {
            C::BuyItem { player, .. }
            | C::HireUnit { player, .. }
            | C::Awaken { player, .. }
            | C::Select { player, .. }
            | C::SyncSelection { player, .. }
            | C::PlayerEvent { player, .. }
            | C::DialogButton { player, .. }
            | C::EventHeard { player, .. }
            | C::Chat { player, .. }
            | C::PlayerLeft { player }
            | C::Submenu { player, .. }
            | C::Trackable { player, .. }
            | C::Signal { player, .. }
            | C::Alliance { player, .. }
            | C::Trade { player, .. }
            | C::SelectionOrder { player, .. } => Some(*player),
            _ => None,
        }
    }
}

/// What a command answered: an interface error to show, or whether a chat line is shown
/// or an item was used.
#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    Done,
    Error(&'static str),
    Flag(bool),
}

/// One step of the game: its length and the commands given before it, each with the
/// player who gave it.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Frame {
    pub dt: f32,
    pub commands: Vec<(u8, Command)>,
    /// The commands as the original's actions, each player's block (`actions::Codec`),
    /// when they were recorded so; a replay read from a file plays from them.
    #[serde(default)]
    pub actions: Option<Vec<(u8, Vec<u8>)>>,
    /// A network game's turn message as the host sent it: its time in milliseconds and
    /// each player's block by network id, as the keepalive's net module hashes it.
    #[serde(default)]
    pub turn: Option<(u16, Vec<(u8, Vec<u8>)>)>,
    /// The game's checksum for the frame, as the keepalive has it (the state its
    /// actions left, before its time): the replay's checksum record after it.
    #[serde(default)]
    pub checksum: Option<u32>,
}

/// The map as the original's record names and checks it (its game description's
/// path, playable size, and checksums: w3-net's `stat`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MapStat {
    pub path: String,
    pub width: u16,
    pub height: u16,
    pub xoro: u32,
    pub sha1: [u8; 20],
}

/// A replay: the game it was, and its frames.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Replay {
    /// The map, as a file or a path in the game's archives.
    pub map: String,
    /// The setup screen's choices (none for a campaign mission).
    pub lobby: Option<Vec<crate::LobbySlot>>,
    /// The profile's progress when the game began (campaign caches and difficulty).
    pub persist: crate::persist::Persist,
    pub campaign: bool,
    /// 0 for Reign of Chaos, 1 for The Frozen Throne.
    pub version: i32,
    /// The setup's advanced options (the map flags it began with).
    pub flags: u32,
    /// The local player's name in a game begun without a setup screen (the profile's).
    pub local_name: Option<String>,
    pub frames: Vec<Frame>,
    /// The seed the game's generators started from (a network game's slot table's).
    pub seed: u32,
    /// The map as the original's record has it, when known.
    #[serde(default)]
    pub map_stat: Option<MapStat>,
    /// A network game's name, as its record has it.
    #[serde(default)]
    pub game_name: Option<String>,
}

/// The first bytes of replays this game recorded in its own format before it wrote the
/// original's (.w3g); they still play.
const MAGIC_5: &[u8; 8] = b"W3RRPLY5";
/// Replays recorded before the seed was kept, which played from seed 0.
const MAGIC_4: &[u8; 8] = b"W3RRPLY4";

/// A replay as format 4 laid it out: all but the seed.
#[derive(Deserialize)]
struct Replay4 {
    map: String,
    lobby: Option<Vec<crate::LobbySlot>>,
    persist: crate::persist::Persist,
    campaign: bool,
    version: i32,
    flags: u32,
    local_name: Option<String>,
    frames: Vec<Frame>,
}

/// The private record (after the start records) holding what the original's record has
/// no place for: the setup as this game keeps it and the campaign's progress.
const SETUP_RECORD: u8 = 0x7E;
/// The private action carrying one of this game's commands, until each has its
/// original action (`Command` in postcard).
const COMMAND_ACTION: u8 = crate::actions::COMMAND_ACTION;
/// The private action giving a turn's exact length (an f32), from player 0, as this
/// game's steps need not be whole milliseconds.
const STEP_ACTION: u8 = 0x7D;

/// The setup kept in the private record.
#[derive(Serialize, Deserialize)]
struct Setup {
    map: String,
    lobby: Option<Vec<crate::LobbySlot>>,
    persist: crate::persist::Persist,
    campaign: bool,
    version: i32,
    flags: u32,
    local_name: Option<String>,
}

impl Replay {
    /// The replay as the original's file (.w3g), as it writes them: its header and
    /// record (the host and players, the game's description with the map and its
    /// checksums, the slots, the seed), the start records, then each frame's turn (a
    /// network game's message as it came, else its players' actions) and the game's
    /// checksum after it, and the local player's leaving.  Only a frame not of whole
    /// milliseconds or a command without an action of the original's, and a campaign
    /// mission's setup, go privately.
    pub fn to_bytes(&self) -> anyhow::Result<Vec<u8>> {
        use w3_formats::w3g;
        let lobby = self.lobby.clone().unwrap_or_default();
        let name_of = |s: &crate::LobbySlot| s.name.clone().unwrap_or_else(|| format!("Player {}", s.player + 1));
        let host_slot = lobby.iter().find(|s| matches!(s.fill, crate::SlotFill::Local)).or(lobby.first());
        let host = w3g::PlayerRecord { pid: host_slot.map_or(1, |s| s.player + 1), name: host_slot.map(name_of).or(self.local_name.clone()).unwrap_or_default(), extra: vec![0] };
        let players = lobby
            .iter()
            .filter(|s| matches!(s.fill, crate::SlotFill::Remote) || matches!(s.fill, crate::SlotFill::Observer(false)))
            .map(|s| w3g::PlayerRecord { pid: s.player + 1, name: name_of(s), extra: vec![0] })
            .collect();
        // Races as the slot table has them: human 1, orc 2, night elf 4, undead 8, random
        // 0x20, each selectable (0x40).
        let race = |r: Option<i32>| 0x40 | match r {
            Some(1) => 1,
            Some(2) => 2,
            Some(3) => 8,
            Some(4) => 4,
            _ => 0x20,
        };
        let slots = lobby
            .iter()
            .map(|s| {
                let used = !matches!(s.fill, crate::SlotFill::Empty);
                let computer = matches!(s.fill, crate::SlotFill::Computer);
                w3g::Slot { pid: if used && !computer { s.player + 1 } else { 0 }, download: 100, status: if used { 2 } else { 0 }, computer, team: s.team as u8, color: s.color as u8, race: race(s.race), ai: s.difficulty as u8, handicap: s.handicap as u8 }
            })
            .collect::<Vec<_>>();
        let multiplayer = lobby.iter().any(|s| matches!(s.fill, crate::SlotFill::Remote));
        // The settings word holds the advanced options eight bits up and the speed (fast,
        // 2) below them, as the original's games are.
        let ms = self.map_stat.clone().unwrap_or_else(|| MapStat { path: self.map.clone(), ..Default::default() });
        let stat = w3_net::GameStat { settings: self.flags << 8 | 2, map_width: ms.width, map_height: ms.height, map_xoro: ms.xoro, map_path: ms.path.clone(), host: host.name.clone(), map_sha1: ms.sha1 }.to_bytes();
        let mut records = Vec::new();
        // What the original's record has no place for (a campaign mission's setup and
        // progress) goes in a private record, which only this game reads; a game from a
        // setup screen is all in the original's.
        if self.campaign || self.lobby.is_none() {
            let setup = Setup { map: self.map.clone(), lobby: self.lobby.clone(), persist: self.persist.clone(), campaign: self.campaign, version: self.version, flags: self.flags, local_name: self.local_name.clone() };
            let setup = postcard::to_allocvec(&setup)?;
            records.push(w3g::Record::Other { id: SETUP_RECORD, body: [&(setup.len() as u32).to_le_bytes()[..], &setup].concat() });
        }
        for id in [0x1A, 0x1B, 0x1C] {
            records.push(w3g::Record::Other { id, body: 1u32.to_le_bytes().to_vec() });
        }
        for f in &self.frames {
            // A network turn as its message came; else the frame's actions, and its exact
            // length privately only when it is not whole milliseconds.
            let (ms, mut actions) = match &f.turn {
                Some((ms, blocks)) => (*ms, blocks.clone()),
                None => {
                    let ms = (f.dt * 1000.0).round() as u16;
                    let mut actions: Vec<(u8, Vec<u8>)> = Vec::new();
                    match &f.actions {
                        Some(acts) => actions.extend(acts.iter().filter(|(_, b)| !b.is_empty()).map(|(p, b)| (p + 1, b.clone()))),
                        None => {
                            for (player, c) in &f.commands {
                                let body = postcard::to_allocvec(c)?;
                                let mut a = vec![COMMAND_ACTION];
                                a.extend((body.len() as u16).to_le_bytes());
                                a.extend(body);
                                match actions.iter_mut().find(|(p, _)| *p == player + 1) {
                                    Some((_, bytes)) => bytes.extend(a),
                                    None => actions.push((player + 1, a)),
                                }
                            }
                        }
                    }
                    (ms, actions)
                }
            };
            if f.turn.is_none() && (ms as f32 / 1000.0 - f.dt).abs() > 1e-6 {
                actions.insert(0, (0u8, [&[STEP_ACTION][..], &f.dt.to_le_bytes()].concat()));
            }
            records.push(w3g::Record::Turn { ms, actions });
            records.push(w3g::Record::Checksum(f.checksum.unwrap_or(0).to_le_bytes().to_vec()));
        }
        // The game's record ends with the local player leaving (read: a game played
        // alone, reason 12, result 7).
        records.push(w3g::Record::Leave { reason: 12, pid: host.pid, result: 7, unknown: 1 });
        let length_ms = (self.length() * 1000.0).round() as u32;
        let humans = lobby.iter().filter(|s| matches!(s.fill, crate::SlotFill::Local | crate::SlotFill::Remote)).count();
        let r = w3g::Replay {
            product: if self.version == 0 { *b"3RAW" } else { *b"PX3W" },
            version: 27,
            build: 6059,
            flags: if multiplayer { 0x8000 } else { 0 },
            length_ms,
            host,
            game_name: if multiplayer { self.game_name.clone().unwrap_or_else(|| self.map.clone()) } else { "Local Game".into() },
            stat,
            // As the original writes them: a network game's players and game type 9, a
            // game played alone 12 and 1.
            player_count: if multiplayer { humans.max(1) as u32 } else { 12 },
            game_type: if multiplayer { 9 } else { 1 },
            language: 0,
            players,
            start_spots: slots.len() as u8,
            slots,
            seed: self.seed,
            select_mode: 0,
            records,
        };
        Ok(r.to_bytes())
    }

    pub fn parse(b: &[u8]) -> anyhow::Result<Replay> {
        use std::io::Read;
        if b.starts_with(w3_formats::w3g::MAGIC) {
            return Self::from_w3g(b);
        }
        let (body, old) = match (b.strip_prefix(MAGIC_5.as_slice()), b.strip_prefix(MAGIC_4.as_slice())) {
            (Some(body), _) => (body, false),
            (None, Some(body)) => (body, true),
            _ => anyhow::bail!("not a replay this game recorded"),
        };
        let mut json = Vec::new();
        flate2::read::ZlibDecoder::new(body).read_to_end(&mut json)?;
        if old {
            let r: Replay4 = postcard::from_bytes(&json)?;
            return Ok(Replay { map: r.map, lobby: r.lobby, persist: r.persist, campaign: r.campaign, version: r.version, flags: r.flags, local_name: r.local_name, frames: r.frames, seed: 0, map_stat: None, game_name: None });
        }
        Ok(postcard::from_bytes(&json)?)
    }

    /// A replay as the original's file, this game's or the original's: its setup from
    /// the private record when there is one, else from the original's record.
    fn from_w3g(b: &[u8]) -> anyhow::Result<Replay> {
        use w3_formats::w3g;
        let r = w3g::Replay::parse(b)?;
        let stat = w3_net::GameStat::parse(&r.stat)?;
        let private = r.records.iter().find_map(|rec| match rec {
            w3g::Record::Other { id: SETUP_RECORD, body } => body.get(4..),
            _ => None,
        });
        let s: Setup = match private {
            Some(setup) => postcard::from_bytes(setup)?,
            None => Self::setup_of(&r, &stat),
        };
        let multiplayer = r.flags & 0x8000 != 0;
        let mut frames: Vec<Frame> = Vec::new();
        for rec in &r.records {
            if let w3g::Record::Checksum(c) = rec {
                if let (Some(f), true) = (frames.last_mut(), c.len() == 4) {
                    f.checksum = Some(u32::from_le_bytes([c[0], c[1], c[2], c[3]]));
                }
                continue;
            }
            let w3g::Record::Turn { ms, actions } = rec else { continue };
            // A network game's turn is played as its message came (its net module part).
            let turn = multiplayer.then(|| (*ms, actions.iter().filter(|(p, _)| *p != 0).cloned().collect()));
            let mut f = Frame { dt: *ms as f32 / 1000.0, commands: Vec::new(), actions: Some(Vec::new()), turn, checksum: None };
            for (pid, bytes) in actions {
                if *pid == 0 {
                    if bytes.len() == 5 && bytes[0] == STEP_ACTION {
                        f.dt = f32::from_le_bytes(bytes[1..5].try_into().unwrap());
                    }
                    continue;
                }
                f.actions.get_or_insert_with(Vec::new).push((pid - 1, bytes.clone()));
            }
            frames.push(f);
        }
        let map_stat = Some(MapStat { path: stat.map_path.clone(), width: stat.map_width, height: stat.map_height, xoro: stat.map_xoro, sha1: stat.map_sha1 });
        let game_name = multiplayer.then(|| r.game_name.clone());
        Ok(Replay { map: s.map, lobby: s.lobby, persist: s.persist, campaign: s.campaign, version: s.version, flags: s.flags, local_name: s.local_name, frames, seed: r.seed, map_stat, game_name })
    }

    /// A game's setup from the original's record: its map as its description names it,
    /// its slots (the host's on this computer; races by their bits, human 1, orc 2,
    /// night elf 4, undead 8, random 0x20), and its advanced options eight bits up in
    /// the settings word.
    fn setup_of(r: &w3_formats::w3g::Replay, stat: &w3_net::GameStat) -> Setup {
        let lobby = r
            .slots
            .iter()
            .enumerate()
            .map(|(k, s)| {
                let name = std::iter::once(&r.host).chain(&r.players).find(|p| p.pid == s.pid && s.pid != 0).map(|p| p.name.clone());
                let fill = if s.status != 2 {
                    crate::SlotFill::Empty
                } else if s.computer {
                    crate::SlotFill::Computer
                } else if s.pid == r.host.pid {
                    crate::SlotFill::Local
                } else {
                    crate::SlotFill::Remote
                };
                let race = match s.race & 0x3f {
                    1 => Some(1),
                    2 => Some(2),
                    8 => Some(3),
                    4 => Some(4),
                    _ => None,
                };
                crate::LobbySlot { player: k as u8, fill, name, race, team: s.team as i32, color: s.color as i32, handicap: s.handicap as f32, difficulty: s.ai as i32 }
            })
            .collect();
        Setup { map: stat.map_path.clone(), lobby: Some(lobby), persist: Default::default(), campaign: false, version: if &r.product == b"PX3W" { 1 } else { 0 }, flags: stat.settings >> 8, local_name: Some(r.host.name.clone()) }
    }

    /// The game's length in seconds.
    pub fn length(&self) -> f32 {
        self.frames.iter().map(|f| f.dt).sum()
    }
}

/// A replay being made.
#[derive(Clone, Debug, Default)]
pub struct Recorder {
    pub replay: Replay,
    /// The commands since the last step.
    pending: Vec<(u8, Command)>,
    /// Those commands as actions.
    actions: Vec<(u8, Vec<u8>)>,
    codec: crate::actions::Codec,
}

impl Recorder {
    pub fn new(replay: Replay) -> Recorder {
        Recorder { replay, pending: Vec::new(), actions: Vec::new(), codec: Default::default() }
    }

    /// Notes a command, as the world stands before it is carried out.
    /// (A network game's turn is noted whole as it is played, `step`.)
    pub fn command(&mut self, w: &crate::world::World, player: u8, c: Command) {
        let bytes = self.codec.encode(w, player, &c);
        if !bytes.is_empty() {
            match self.actions.iter_mut().find(|(p, _)| *p == player) {
                Some((_, b)) => b.extend(bytes),
                None => self.actions.push((player, bytes)),
            }
        }
        self.pending.push((player, c));
    }

    /// Closes a frame of `dt` seconds: a network turn's message as it came (`turn`), and
    /// the game's checksum for it.
    pub fn step(&mut self, dt: f32, turn: Option<(u16, Vec<(u8, Vec<u8>)>)>, checksum: u32) {
        let commands = std::mem::take(&mut self.pending);
        let actions = std::mem::take(&mut self.actions);
        self.replay.frames.push(Frame { dt, commands, actions: Some(actions), turn, checksum: Some(checksum) });
    }
}

/// A replay playing back: the next frame to play.
#[derive(Clone, Debug, Default)]
pub struct Playback {
    pub replay: Replay,
    pub next: usize,
    /// Game time owed to frames not yet played.
    pub owed: f32,
}

impl Playback {
    pub fn finished(&self) -> bool {
        self.next >= self.replay.frames.len()
    }
}

impl Outcome {
    /// The interface error, if any.
    pub fn ok(self) -> Result<(), &'static str> {
        match self {
            Outcome::Error(e) => Err(e),
            _ => Ok(()),
        }
    }

    pub fn flag(self) -> bool {
        matches!(self, Outcome::Flag(true))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A replay written as the original's file reads back as it was: its setup (from
    /// the original's record), each frame's exact length, and each command.
    #[test]
    fn replays_keep_everything_in_the_originals_file() {
        let lobby = vec![
            crate::LobbySlot { player: 0, fill: crate::SlotFill::Local, name: Some("Jack".into()), race: Some(1), team: 0, color: 0, handicap: 100.0, difficulty: 1 },
            crate::LobbySlot { player: 1, fill: crate::SlotFill::Computer, name: None, race: None, team: 1, color: 1, handicap: 90.0, difficulty: 2 },
        ];
        let r = Replay {
            map: "Maps\\(2)BootyBay.w3m".into(),
            lobby: Some(lobby),
            version: 1,
            flags: 0x64802,
            seed: 0x35000000,
            frames: vec![
                Frame { dt: 0.0166, commands: vec![], actions: None, turn: None, checksum: None },
                Frame { dt: 0.05, commands: vec![(0, Command::Order { unit: 42, order: 851971, x: 1.5, y: -2.0, target: 0 }), (0, Command::Chat { player: 0, text: "hi".into(), to: 0 })], actions: None, turn: None, checksum: None },
                Frame { dt: 0.1, commands: vec![(1, Command::Train { building: 7, id: *b"hpea" })], actions: None, turn: None, checksum: None },
            ],
            ..Default::default()
        };
        let b = r.to_bytes().unwrap();
        assert!(b.starts_with(w3_formats::w3g::MAGIC));
        let mut back = Replay::parse(&b).unwrap();
        // The commands come back as their private actions; read them as play does.
        for f in &mut back.frames {
            for (p, bytes) in f.actions.take().unwrap_or_default() {
                let mut k = 0;
                while k < bytes.len() {
                    assert_eq!(bytes[k], COMMAND_ACTION);
                    let n = u16::from_le_bytes([bytes[k + 1], bytes[k + 2]]) as usize;
                    f.commands.push((p, postcard::from_bytes(&bytes[k + 3..k + 3 + n]).unwrap()));
                    k += 3 + n;
                }
            }
        }
        // The setup comes back from the original's record: the host's name as the local
        // one, each frame's checksum, and the map's description.
        assert_eq!(back.local_name.take().as_deref(), Some("Jack"));
        assert_eq!(back.map_stat.take().map(|m| m.path), Some("Maps\\(2)BootyBay.w3m".to_string()));
        for f in &mut back.frames {
            assert_eq!(f.checksum.take(), Some(0));
        }
        assert_eq!(postcard::to_allocvec(&back).unwrap(), postcard::to_allocvec(&r).unwrap());
        let w = w3_formats::w3g::Replay::parse(&b).unwrap();
        assert_eq!((w.seed, w.slots.len(), w.host.name.as_str(), w.game_type, w.player_count), (0x35000000, 2, "Jack", 1, 12));
        // No private setup record for a game from a setup screen; the game ends with the
        // local player's leaving.
        assert!(!w.records.iter().any(|r| matches!(r, w3_formats::w3g::Record::Other { id: SETUP_RECORD, .. })));
        assert!(matches!(w.records.last(), Some(w3_formats::w3g::Record::Leave { reason: 12, pid: 1, result: 7, unknown: 1 })));
    }
}
