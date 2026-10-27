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
            | C::Trade { player, .. } => Some(*player),
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

/// The file's first bytes, before its compressed body.
pub const MAGIC: &[u8; 8] = b"W3RRPLY5";
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

impl Replay {
    /// The replay as a file: the magic, then the replay compressed.
    pub fn to_bytes(&self) -> anyhow::Result<Vec<u8>> {
        use std::io::Write;
        let json = postcard::to_allocvec(self)?;
        let mut e = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        e.write_all(&json)?;
        let mut out = MAGIC.to_vec();
        out.extend(e.finish()?);
        Ok(out)
    }

    pub fn parse(b: &[u8]) -> anyhow::Result<Replay> {
        use std::io::Read;
        let (body, old) = match (b.strip_prefix(MAGIC.as_slice()), b.strip_prefix(MAGIC_4.as_slice())) {
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
