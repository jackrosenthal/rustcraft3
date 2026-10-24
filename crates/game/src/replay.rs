//! Replays: what the players did, step by step.  Every command from the interface goes
//! through `Game::apply`, which notes it; each step of the game (`Game::tick`) closes a
//! frame of the step's length and the commands before it.  A replay plays the frames
//! back on the same map with the same setup: the commands of each frame, then the step,
//! so the game, which runs the same way for the same inputs, plays out as it did.

use serde::{Deserialize, Serialize};

use crate::objects::Id;

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
    UseItem { unit: u32, slot: usize },
    UseItemAt { unit: u32, slot: usize, target: u32, x: f32, y: f32 },
    MoveItem { unit: u32, from: usize, to: usize },
    DropItemAt { unit: u32, slot: usize, x: f32, y: f32 },
    GiveItemTo { unit: u32, slot: usize, target: u32 },
    Select { player: u8, units: Vec<u32> },
    PlayerEvent { player: u8, event: i32 },
    DialogButton { player: u8, button: u32 },
    Chat { player: u8, text: String },
}

/// What a command answered: an interface error to show, or whether a chat line is shown
/// or an item was used.
#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    Done,
    Error(&'static str),
    Flag(bool),
}

/// One step of the game: its length and the commands given before it.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Frame {
    pub dt: f32,
        pub commands: Vec<Command>,
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
    pub frames: Vec<Frame>,
}

/// The file's first bytes, before its compressed body.
pub const MAGIC: &[u8; 8] = b"W3RRPLY1";

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
        let body = b.strip_prefix(MAGIC.as_slice()).ok_or_else(|| anyhow::anyhow!("not a replay this game recorded"))?;
        let mut json = Vec::new();
        flate2::read::ZlibDecoder::new(body).read_to_end(&mut json)?;
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
    pending: Vec<Command>,
}

impl Recorder {
    pub fn new(replay: Replay) -> Recorder {
        Recorder { replay, pending: Vec::new() }
    }

    pub fn command(&mut self, c: Command) {
        self.pending.push(c);
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
