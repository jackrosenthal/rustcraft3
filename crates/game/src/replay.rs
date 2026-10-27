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
            | C::PlayerEvent { player, .. }
            | C::DialogButton { player, .. }
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
const COMMAND_ACTION: u8 = 0x7F;
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
    /// The replay as the original's file (.w3g): its header and record (the host and
    /// players, the slots, the seed), then a turn per frame with each player's actions
    /// and a checksum.  The players' commands go as private actions, and the setup this
    /// game needs beyond the record in a private record.
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
        let stat = w3_net::stat::encode(&w3_net::GameStat { settings: self.flags, map_width: 0, map_height: 0, map_xoro: 0, map_path: self.map.clone(), host: host.name.clone(), map_sha1: [0; 20] }.to_bytes());
        let setup = Setup { map: self.map.clone(), lobby: self.lobby.clone(), persist: self.persist.clone(), campaign: self.campaign, version: self.version, flags: self.flags, local_name: self.local_name.clone() };
        let setup = postcard::to_allocvec(&setup)?;
        let mut records = vec![
            w3g::Record::Other { id: SETUP_RECORD, body: [&(setup.len() as u32).to_le_bytes()[..], &setup].concat() },
            w3g::Record::Other { id: 0x1A, body: 1u32.to_le_bytes().to_vec() },
            w3g::Record::Other { id: 0x1B, body: 1u32.to_le_bytes().to_vec() },
            w3g::Record::Other { id: 0x1C, body: 1u32.to_le_bytes().to_vec() },
        ];
        for f in &self.frames {
            let mut actions = vec![(0u8, [&[STEP_ACTION][..], &f.dt.to_le_bytes()].concat())];
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
            records.push(w3g::Record::Turn { ms: (f.dt * 1000.0).round() as u16, actions });
            records.push(w3g::Record::Checksum(vec![0; 4]));
        }
        let length_ms = (self.length() * 1000.0).round() as u32;
        let multiplayer = lobby.iter().any(|s| matches!(s.fill, crate::SlotFill::Remote));
        let r = w3g::Replay {
            product: if self.version == 0 { *b"3RAW" } else { *b"PX3W" },
            version: 27,
            build: 6059,
            flags: if multiplayer { 0x8000 } else { 0 },
            length_ms,
            host,
            game_name: if multiplayer { self.map.clone() } else { "Local Game".into() },
            stat,
            player_count: lobby.len().max(1) as u32,
            game_type: 1,
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
            return Ok(Replay { map: r.map, lobby: r.lobby, persist: r.persist, campaign: r.campaign, version: r.version, flags: r.flags, local_name: r.local_name, frames: r.frames, seed: 0 });
        }
        Ok(postcard::from_bytes(&json)?)
    }

    /// A replay this game wrote as the original's file.
    fn from_w3g(b: &[u8]) -> anyhow::Result<Replay> {
        use w3_formats::w3g;
        let r = w3g::Replay::parse(b)?;
        let setup = r
            .records
            .iter()
            .find_map(|rec| match rec {
                w3g::Record::Other { id: SETUP_RECORD, body } => body.get(4..),
                _ => None,
            })
            .ok_or_else(|| anyhow::anyhow!("a replay of the original's, not this game's"))?;
        let s: Setup = postcard::from_bytes(setup)?;
        let mut frames = Vec::new();
        for rec in &r.records {
            let w3g::Record::Turn { ms, actions } = rec else { continue };
            let mut f = Frame { dt: *ms as f32 / 1000.0, commands: Vec::new() };
            for (pid, bytes) in actions {
                let mut k = 0;
                while k < bytes.len() {
                    match bytes[k] {
                        STEP_ACTION if *pid == 0 && k + 5 <= bytes.len() => {
                            f.dt = f32::from_le_bytes(bytes[k + 1..k + 5].try_into().unwrap());
                            k += 5;
                        }
                        COMMAND_ACTION if k + 3 <= bytes.len() => {
                            let n = u16::from_le_bytes([bytes[k + 1], bytes[k + 2]]) as usize;
                            let body = bytes.get(k + 3..k + 3 + n).ok_or_else(|| anyhow::anyhow!("short command"))?;
                            f.commands.push((pid.saturating_sub(1), postcard::from_bytes(body)?));
                            k += 3 + n;
                        }
                        _ => anyhow::bail!("an action this game does not read yet"),
                    }
                }
            }
            frames.push(f);
        }
        Ok(Replay { map: s.map, lobby: s.lobby, persist: s.persist, campaign: s.campaign, version: s.version, flags: s.flags, local_name: s.local_name, frames, seed: r.seed })
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
}

impl Recorder {
    pub fn new(replay: Replay) -> Recorder {
        Recorder { replay, pending: Vec::new() }
    }

    pub fn command(&mut self, player: u8, c: Command) {
        self.pending.push((player, c));
    }

    pub fn step(&mut self, dt: f32) {
        let commands = std::mem::take(&mut self.pending);
        self.replay.frames.push(Frame { dt, commands });
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

    /// A replay written as the original's file reads back as it was: its setup, each
    /// frame's exact length, and each command.
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
                Frame { dt: 0.0166, commands: vec![] },
                Frame { dt: 0.05, commands: vec![(0, Command::Order { unit: 42, order: 851971, x: 1.5, y: -2.0, target: 0 }), (0, Command::Chat { player: 0, text: "hi".into(), to: 0 })] },
                Frame { dt: 0.1, commands: vec![(1, Command::Train { building: 7, id: *b"hpea" })] },
            ],
            ..Default::default()
        };
        let b = r.to_bytes().unwrap();
        assert!(b.starts_with(w3_formats::w3g::MAGIC));
        let back = Replay::parse(&b).unwrap();
        assert_eq!(postcard::to_allocvec(&back).unwrap(), postcard::to_allocvec(&r).unwrap());
        let w = w3_formats::w3g::Replay::parse(&b).unwrap();
        assert_eq!((w.seed, w.slots.len(), w.host.name.as_str()), (0x35000000, 2, "Jack"));
    }
}
