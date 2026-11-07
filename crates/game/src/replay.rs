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
    /// menu's Save Game in a network game, kept in Save\\Multiplayer): the file's
    /// name with its extension, as the original's action carries it ("lansave.w3z").
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
    /// A trigger execution's thread may go on, as the sender's sleep in it ended (the
    /// original's action 0x62, CNetCommandResumeTriggerExec): the execution by its id
    /// (tag and birth) and its count of resumptions so far plus one.
    ResumeTrigger { player: u8, execution: (u32, u32), count: u32 },
    /// A player's TriggerSyncReady in a trigger execution (action 0x63,
    /// CNetCommandTriggerSyncReady).
    TriggerSyncReady { player: u8, execution: (u32, u32) },
    /// A computer's save of a network game is done (action 0x07, CNetCommandSaveDone):
    /// its player and the save's result (1 written).
    SaveDone { player: u8, result: u32 },
    /// A cheat typed in a game played alone, as the original's action carries it (0x20 to
    /// 0x32, `cheats`): its id and its data.
    Cheat { player: u8, id: u8, data: Vec<u8> },
    /// A game played alone's speed set (0x03), as the options' Game Speed sends it.
    SetSpeed { player: u8, speed: u8 },
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
            | C::SelectionOrder { player, .. }
            | C::ResumeTrigger { player, .. }
            | C::TriggerSyncReady { player, .. }
            | C::SaveDone { player, .. }
            | C::Cheat { player, .. }
            | C::SetSpeed { player, .. } => Some(*player),
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

/// A player who left a network game, as the host's PLAYERLEFT (0x07) said: the
/// network's id for them, their player, and the word it carried (the leaver's own
/// LEAVEGAME reason, unchanged: `FUN_6f99cdc0`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Leave {
    pub pid: u8,
    pub player: u8,
    pub word: u32,
}

/// One step of the game: its length and the commands given before it, each with the
/// player who gave it.
#[derive(Clone, Debug, Default)]
pub struct Frame {
    pub dt: f32,
    pub commands: Vec<(u8, Command)>,
    /// The commands as the original's actions, each player's block (`actions::Codec`),
    /// when they were recorded so; a replay read from a file plays from them.
    pub actions: Option<Vec<(u8, Vec<u8>)>>,
    /// A network game's turn message as the host sent it: its time in milliseconds and
    /// each player's block by network id, as the keepalive's net module hashes it.
    pub turn: Option<(u16, Vec<(u8, Vec<u8>)>)>,
    /// The game's checksum for the frame, as the keepalive has it (the state its
    /// actions left, before its time): the replay's checksum record after it.
    pub checksum: Option<u32>,
    /// The players who left before the frame's turn (a network game's PLAYERLEFT, a
    /// replay's leave records).
    pub leaves: Vec<Leave>,
    /// The chat lines that came before the frame's turn: the sender's network id, the
    /// line's recipients as W3GS chat carries them (0 all, 1 allies, 2 observers, 3 on a
    /// player), and the line.
    pub chat: Vec<(u8, u32, String)>,
}

/// The map as the original's record names and checks it (its game description's
/// path, playable size, and checksums: w3-net's `stat`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MapStat {
    pub path: String,
    pub width: u16,
    pub height: u16,
    pub xoro: u32,
    pub sha1: [u8; 20],
}

/// A replay: the game it was, and its frames.
#[derive(Clone, Debug, Default)]
pub struct Replay {
    /// The map, as a file or a path in the game's archives.
    pub map: String,
    /// The setup screen's choices (none for a game begun without one: a campaign
    /// mission, or a map loaded directly).
    pub lobby: Option<Vec<crate::LobbySlot>>,
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
    pub map_stat: Option<MapStat>,
    /// A network game's name, as its record has it.
    pub game_name: Option<String>,
    /// The game's setup as the original's record copies it: a network game's, or a map
    /// loaded directly's (`Game::loaded_record`).
    pub net: Option<NetRecord>,
    /// Whether the map is strongly signed (Blizzard's): a game played alone from a setup
    /// screen is of type 9 then, else 1 (read: the original's of Glacial Thaw, 9).
    pub signed: bool,
    /// The word the local player left with, as the record's closing leave has it (the
    /// LEAVEGAME reason: 7 through the game menu, 0 standing for it; 11 through the
    /// victory or defeat dialog).
    pub left: u32,
    /// The games the program had begun when this one began, this one among them, as the
    /// leave records' last word counts them (the original's net layer's +0xc; read: 1, 2,
    /// 4, and 5 for a quit, a quit, a defeat, and a quit in one run of the original, 2
    /// for a defeat in a fresh one); 0 as 1.
    pub games: u32,
    /// Whether the local player's game was over as they left (a victory or a defeat),
    /// which the games count once more from then (read: the original's LAN host left by
    /// the menu after its victory, its leave 7 and 2 after the guest's 1).
    pub over: bool,
}

/// A network game's setup as the original's record copies it (read: its replay of a
/// Mafarazzo TD game on LAN, against Booty Bay's): the game type its host offered (1
/// for a custom map's, 9 for a Blizzard map's), the slot table's layout as the
/// record's select mode (3 with fixed player settings), and its slots as they were,
/// their count the record's players' and start spots' (12 there, 2 for Booty Bay),
/// fixed races with no selectable flag, and empty slots' download unknown (255).
/// The game's description as GAMEINFO offered it (`stat`, coded: a joined game's its
/// host's, with the host's name and the size it gave) and the slot table's start spots
/// go into its saves.  A map loaded directly has one too, as the original's -loadfile
/// makes it (`Game::loaded_record`), with no description.
#[derive(Clone, Debug, Default)]
pub struct NetRecord {
    pub game_type: u32,
    pub layout: u8,
    pub slots: Vec<NetRecordSlot>,
    pub stat: Vec<u8>,
    pub start_spots: u8,
}

/// A network game's description as its saves carry it (`Replay::network_head`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NetworkHead {
    /// The game's name and its coded stat, as GAMEINFO carried them.
    pub name: String,
    pub stat: Vec<u8>,
    /// The description's two words (`FUN_6f968cf0`'s +0xb0 and +0xb4, copied by
    /// `FUN_6f96bef0`, which keeps the second only for a first of 1 to 16): GAMEINFO's
    /// slots and game type.
    pub words: [u32; 2],
    /// The game type and the advanced options, as its map setup keeps them.
    pub game_type: u32,
    pub flags: u32,
    /// The slot table as W3GS carried it, when it is known, and the local player's
    /// slot in it.
    pub slots: Option<Vec<u8>>,
    pub local_slot: Option<i32>,
}

/// A slot of the slot table, as W3GS has it.
#[derive(Clone, Copy, Debug, Default)]
pub struct NetRecordSlot {
    pub pid: u8,
    pub download: u8,
    pub status: u8,
    pub computer: bool,
    pub team: u8,
    pub color: u8,
    pub race: u8,
    pub ai: u8,
    pub handicap: u8,
}

/// The record's word after the player counts, as every replay the original wrote has
/// it (its "language"; the original fills it from memory, but each of its replays read
/// holds this).
const LANGUAGE: u32 = 0x26fb68;
/// The setup's advanced option a map loaded directly runs with (MAP_LOCK_RANDOM_SEED),
/// which the original's record of such a game has alone (its settings word 0x800002).
const LOCK_RANDOM_SEED: u32 = 0x8000;
/// The network id of a network game's host, as W3GS numbers it.
const HOST_PID: u8 = 1;
/// A leave record's reasons: the leave of another player (`FUN_6f98c680`, after the
/// host's PLAYERLEFT), and those as the local player leaves (`FUN_6f985030`).
const LEFT_REMOTE: u32 = 1;
const LEFT_LOCAL: u32 = 12;
/// The LEAVEGAME words of a player who left through the game menu and of one who left
/// through the victory or defeat dialog (read: the original's game played alone quit
/// through the menu, its closing leave 7, even after its victory; left by the defeat
/// dialog's Quit Game or the victory dialog's Continue, 11).
pub const LEFT_PLAYING: u32 = 7;
pub const LEFT_OVER: u32 = 11;

impl Replay {
    /// A network game's description as its saves carry it (read: the original's save
    /// of a LAN game on Booty Bay, its GAMEINFO's name and stat, the host's own, with
    /// the lobby's options, the size the host gave, and the host's name, then the words
    /// 2 and 9, its slots and game type; its map setup the type and options; and the
    /// slot table as it came): none for a game played alone.  The game's record
    /// (`NetRecord`) gives the stat, words, and table; without one, the stat is made
    /// from the map's (the local player's name as the host's) and the words are the
    /// people and 9 (provisional both).
    pub fn network_head(&self) -> Option<NetworkHead> {
        let lobby = self.lobby.as_ref()?;
        if !lobby.iter().any(|s| matches!(s.fill, crate::SlotFill::Remote)) {
            return None;
        }
        let name = self.game_name.clone().unwrap_or_else(|| self.map.clone());
        let stat = match self.net.as_ref().filter(|n| !n.stat.is_empty()) {
            Some(n) => n.stat.clone(),
            None => {
                let host_slot = lobby.iter().find(|s| matches!(s.fill, crate::SlotFill::Local)).or(lobby.first());
                let host = host_slot.map(|s| s.name.clone().unwrap_or_else(|| format!("Player {}", s.player + 1))).unwrap_or_default();
                let ms = self.map_stat.clone().unwrap_or_else(|| MapStat { path: self.map.clone(), ..Default::default() });
                w3_net::GameStat { settings: self.flags << 8 | 2, map_width: ms.width, map_height: ms.height, map_xoro: ms.xoro, map_path: ms.path.clone(), host, map_sha1: ms.sha1 }.to_bytes()
            }
        };
        let Some(n) = self.net.as_ref().filter(|n| !n.slots.is_empty()) else {
            let people = lobby.iter().filter(|s| matches!(s.fill, crate::SlotFill::Local | crate::SlotFill::Remote)).count() as u32;
            return Some(NetworkHead { name, stat, words: [people.max(1), 9], game_type: 9, flags: self.flags, slots: None, local_slot: None });
        };
        let table = w3_net::SlotTable {
            slots: n.slots.iter().map(|s| w3_net::Slot { pid: s.pid, download: s.download, status: s.status, computer: s.computer, team: s.team, color: s.color, race: s.race, ai: s.ai, handicap: s.handicap }).collect(),
            seed: self.seed,
            layout: n.layout,
            player_slots: if n.start_spots == 0 { n.slots.len() as u8 } else { n.start_spots },
        };
        let mut w = w3_net::wire::Writer::default();
        table.write(&mut w);
        // (The lobby's slots are the table's, in order.)
        let local_slot = lobby.iter().position(|s| matches!(s.fill, crate::SlotFill::Local | crate::SlotFill::Observer(true))).map(|k| k as i32);
        Some(NetworkHead { name, stat, words: [n.slots.len() as u32, n.game_type], game_type: n.game_type, flags: self.flags, slots: Some(w.0), local_slot })
    }

    /// The replay as the original's file (.w3g), as it writes them: its header and
    /// record (the host and players, the game's description with the map and its
    /// checksums, the slots, the seed), the start records, then for each frame the
    /// leaves and chat lines that came before its turn, its turn (a network game's
    /// message as it came, else its players' actions), and the game's checksum after
    /// it, and the leaves as the local player leaves.
    pub fn to_bytes(&self) -> anyhow::Result<Vec<u8>> {
        use w3_formats::w3g;
        let lobby = self.lobby.clone().unwrap_or_default();
        let multiplayer = lobby.iter().any(|s| matches!(s.fill, crate::SlotFill::Remote));
        let name_of = |s: &crate::LobbySlot| s.name.clone().unwrap_or_else(|| format!("Player {}", s.player + 1));
        // The people by their network ids (the slot table's, the lobby's slots being the
        // table's in order; a game played alone's its slots' numbers from 1).
        let pid_of = |k: usize, s: &crate::LobbySlot| self.net.as_ref().and_then(|n| n.slots.get(k)).map(|t| t.pid).filter(|&p| p != 0).unwrap_or(s.player + 1);
        let people: Vec<(w3g::PlayerRecord, bool)> = lobby
            .iter()
            .enumerate()
            .filter(|(_, s)| matches!(s.fill, crate::SlotFill::Local | crate::SlotFill::Remote | crate::SlotFill::Observer(_)))
            .map(|(k, s)| (w3g::PlayerRecord { pid: pid_of(k, s), name: name_of(s), extra: vec![0] }, matches!(s.fill, crate::SlotFill::Local | crate::SlotFill::Observer(true))))
            .collect();
        // The record's first player is a network game's host (read: the original's replay
        // of a LAN game it joined as its second player, the host's first), else the local
        // player; a game begun without a setup screen has its user in the record's slots.
        let loaded_pid = self.net.as_ref().and_then(|n| n.slots.iter().find(|s| s.pid != 0 && !s.computer)).map(|s| s.pid);
        let first = if multiplayer { people.iter().position(|p| p.0.pid == HOST_PID) } else { None }.or_else(|| people.iter().position(|p| p.1));
        let host = match first {
            Some(k) => people[k].0.clone(),
            None => w3g::PlayerRecord { pid: loaded_pid.unwrap_or(1), name: self.local_name.clone().unwrap_or_default(), extra: vec![0] },
        };
        let players = people.iter().enumerate().filter(|&(k, _)| Some(k) != first && multiplayer).map(|(_, p)| p.0.clone()).collect::<Vec<_>>();
        // Races as the slot table has them: human 1, orc 2, night elf 4, undead 8, random
        // 0x20, each selectable (0x40).
        let race = |r: Option<i32>| 0x40 | match r {
            Some(1) => 1,
            Some(2) => 2,
            Some(3) => 8,
            Some(4) => 4,
            _ => 0x20,
        };
        let slots = match &self.net {
            Some(n) => n.slots.iter().map(|s| w3g::Slot { pid: s.pid, download: s.download, status: s.status, computer: s.computer, team: s.team, color: s.color, race: s.race, ai: s.ai, handicap: s.handicap }).collect(),
            None => lobby
                .iter()
                .map(|s| {
                    let used = !matches!(s.fill, crate::SlotFill::Empty);
                    let computer = matches!(s.fill, crate::SlotFill::Computer);
                    w3g::Slot { pid: if used && !computer { s.player + 1 } else { 0 }, download: 100, status: if used { 2 } else { 0 }, computer, team: s.team as u8, color: s.color as u8, race: race(s.race), ai: s.difficulty as u8, handicap: s.handicap as u8 }
                })
                .collect::<Vec<_>>(),
        };
        // The game's description: a network game's as its host offered it; else the
        // settings word holding the advanced options eight bits up and the speed (fast,
        // 2) below them, as the original's games are.
        let ms = self.map_stat.clone().unwrap_or_else(|| MapStat { path: self.map.clone(), ..Default::default() });
        let stat = match self.net.as_ref().filter(|n| multiplayer && !n.stat.is_empty()) {
            Some(n) => n.stat.clone(),
            None => w3_net::GameStat { settings: self.flags << 8 | 2, map_width: ms.width, map_height: ms.height, map_xoro: ms.xoro, map_path: ms.path.clone(), host: host.name.clone(), map_sha1: ms.sha1 }.to_bytes(),
        };
        let mut records = Vec::new();
        for id in [0x1A, 0x1B, 0x1C] {
            records.push(w3g::Record::Other { id, body: 1u32.to_le_bytes().to_vec() });
        }
        // The program's games so far (`games`), one more once the local player's game is
        // over (provisional: another's leave after that).
        let word = if self.left == 0 { LEFT_PLAYING } else { self.left };
        let unknown = self.games.max(1);
        for f in &self.frames {
            // Those who left and what was said before the turn, as they came (each
            // computer writes another's leave with reason 1 and their word, as the
            // net layer makes it: `FUN_6f98c680`).
            for l in &f.leaves {
                records.push(w3g::Record::Leave { reason: LEFT_REMOTE, pid: l.pid, result: l.word, unknown });
            }
            for (pid, to, text) in &f.chat {
                records.push(w3g::Record::Chat { pid: *pid, scope: Some(*to), text: text.clone() });
            }
            // A network turn as its message came; else the frame's actions, by network id.
            let (ms, actions) = match &f.turn {
                Some((ms, blocks)) => (*ms, blocks.clone()),
                None => ((f.dt * 1000.0).round() as u16, f.actions.iter().flatten().filter(|(_, b)| !b.is_empty()).map(|(p, b)| (p + 1, b.clone())).collect()),
            };
            records.push(w3g::Record::Turn { ms, actions });
            records.push(w3g::Record::Checksum(f.checksum.unwrap_or(0).to_le_bytes().to_vec()));
        }
        // The game's record ends with the local player leaving: a leave for each person
        // still in the game by network id, each with the word the local player left with
        // (`FUN_6f985030`; read: a game played alone, reason 12, result 7; the original
        // joined to a LAN game as its second player, the host's then its own, both 7).
        let gone: Vec<u8> = self.frames.iter().flat_map(|f| f.leaves.iter().map(|l| l.pid)).collect();
        let unknown = unknown + self.over as u32;
        let mut staying: Vec<u8> = std::iter::once(host.pid).chain(players.iter().map(|p| p.pid)).filter(|p| !gone.contains(p)).collect();
        staying.sort();
        for pid in staying {
            records.push(w3g::Record::Leave { reason: LEFT_LOCAL, pid, result: word, unknown });
        }
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
            // As the original writes them: a network game's its slot table's slots and
            // its offered type (`NetRecord`); a game played alone 12, and 1, with 8 for a
            // Blizzard map (`signed`).
            player_count: match &self.net {
                _ if !multiplayer => 12,
                Some(n) => n.slots.len() as u32,
                None => humans.max(1) as u32,
            },
            game_type: match &self.net {
                Some(n) => n.game_type,
                None if multiplayer => 9,
                None => 1 | if self.signed { 8 } else { 0 },
            },
            language: LANGUAGE,
            players,
            start_spots: self.net.as_ref().map_or(slots.len() as u8, |n| if n.start_spots == 0 { n.slots.len() as u8 } else { n.start_spots }),
            slots,
            seed: self.seed,
            select_mode: self.net.as_ref().map_or(0, |n| n.layout),
            records,
        };
        Ok(r.to_bytes())
    }

    /// A replay as the original writes them (.w3g), this game's or the original's: its
    /// setup from the record (a game played alone whose setup locked the random seed as
    /// a map loaded directly, from the map as such a game begins), its frames from its
    /// turns, with the leaves and chat lines between them; the leaves as the record ends,
    /// after the last turn, play no part.
    pub fn parse(b: &[u8]) -> anyhow::Result<Replay> {
        use w3_formats::w3g;
        let r = w3g::Replay::parse(b)?;
        let stat = w3_net::GameStat::parse(&r.stat)?;
        let multiplayer = r.flags & 0x8000 != 0;
        let flags = stat.settings >> 8;
        let loaded = !multiplayer && flags & LOCK_RANDOM_SEED != 0;
        let mut frames: Vec<Frame> = Vec::new();
        let (mut leaves, mut chat) = (Vec::new(), Vec::new());
        for rec in &r.records {
            match rec {
                w3g::Record::Checksum(c) => {
                    if let (Some(f), true) = (frames.last_mut(), c.len() == 4) {
                        f.checksum = Some(u32::from_le_bytes([c[0], c[1], c[2], c[3]]));
                    }
                }
                w3g::Record::Leave { pid, result, .. } => {
                    let player = r.slots.iter().position(|s| s.pid == *pid).unwrap_or(0) as u8;
                    leaves.push(Leave { pid: *pid, player, word: *result });
                }
                // (A line of the setup's chat has no scope and no part in the game.)
                w3g::Record::Chat { pid, scope: Some(to), text } => chat.push((*pid, *to, text.clone())),
                w3g::Record::Turn { ms, actions } => {
                    // A network game's turn is played as its message came (its net module part).
                    let turn = multiplayer.then(|| (*ms, actions.clone()));
                    let actions = actions.iter().filter(|(p, _)| *p != 0).map(|(pid, bytes)| (pid - 1, bytes.clone())).collect();
                    frames.push(Frame { dt: *ms as f32 / 1000.0, commands: Vec::new(), actions: Some(actions), turn, checksum: None, leaves: std::mem::take(&mut leaves), chat: std::mem::take(&mut chat) });
                }
                _ => {}
            }
        }
        let map_stat = Some(MapStat { path: stat.map_path.clone(), width: stat.map_width, height: stat.map_height, xoro: stat.map_xoro, sha1: stat.map_sha1 });
        let slots = r.slots.iter().map(|s| NetRecordSlot { pid: s.pid, download: s.download, status: s.status, computer: s.computer, team: s.team, color: s.color, race: s.race, ai: s.ai, handicap: s.handicap }).collect();
        let net = NetRecord { game_type: r.game_type, layout: r.select_mode, slots, stat: if multiplayer { r.stat.clone() } else { Vec::new() }, start_spots: r.start_spots };
        // The local player's leaving as the record closes: its word and the games counted
        // (a game over when the word says so; one left by the menu after a victory looks
        // as though it were not, but for its count).
        let (left, games) = r
            .records
            .iter()
            .rev()
            .find_map(|rec| match rec {
                w3g::Record::Leave { reason: LEFT_LOCAL, pid, result, unknown } if *pid == r.host.pid => Some((*result, unknown.saturating_sub((*result == LEFT_OVER) as u32))),
                _ => None,
            })
            .unwrap_or((0, 0));
        let over = left == LEFT_OVER;
        Ok(Replay {
            map: stat.map_path.clone(),
            lobby: (!loaded).then(|| Self::lobby_of(&r)),
            campaign: false,
            version: if &r.product == b"PX3W" { 1 } else { 0 },
            flags,
            local_name: Some(r.host.name.clone()),
            frames,
            seed: r.seed,
            map_stat,
            game_name: multiplayer.then(|| r.game_name.clone()),
            net: Some(net),
            signed: r.game_type & 8 != 0,
            left,
            games,
            over,
        })
    }

    /// A game's setup screen from the original's record: its slots (the host's on this
    /// computer; races by their bits, human 1, orc 2, night elf 4, undead 8, random
    /// 0x20).
    fn lobby_of(r: &w3_formats::w3g::Replay) -> Vec<crate::LobbySlot> {
        r.slots
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
            .collect()
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
    /// The chat lines since the last step, in a game played alone.
    chat: Vec<(u8, u32, String)>,
    codec: crate::actions::Codec,
}

impl Recorder {
    pub fn new(replay: Replay) -> Recorder {
        Recorder { replay, pending: Vec::new(), actions: Vec::new(), chat: Vec::new(), codec: Default::default() }
    }

    /// Notes a command, as the world stands before it is carried out: its actions, and a
    /// chat line (one carried out here, in a game played alone) as the record's line
    /// from the player's network id.
    /// (A network game's turn is noted whole as it is played, `step`.)
    pub fn command(&mut self, w: &crate::world::World, player: u8, c: Command) {
        let bytes = self.codec.encode(w, player, &c);
        if !bytes.is_empty() {
            match self.actions.iter_mut().find(|(p, _)| *p == player) {
                Some((_, b)) => b.extend(bytes),
                None => self.actions.push((player, bytes)),
            }
        }
        if let Command::Chat { text, to, .. } = &c {
            self.chat.push((player + 1, *to as u32, text.clone()));
        }
        self.pending.push((player, c));
    }

    /// Whether commands are noted that no frame has closed yet.
    pub fn pending(&self) -> bool {
        !self.pending.is_empty() || !self.chat.is_empty()
    }

    /// Closes a frame of `dt` seconds and the game's checksum for it: the frame played
    /// (`from`, a network turn's or a replay's) as it came, its actions, turn, leaves,
    /// and chat; else the commands noted since the last.
    pub fn step(&mut self, dt: f32, from: Option<&Frame>, checksum: u32) {
        let commands = std::mem::take(&mut self.pending);
        let mut actions = std::mem::take(&mut self.actions);
        let mut chat = std::mem::take(&mut self.chat);
        let (mut turn, mut leaves) = (None, Vec::new());
        if let Some(f) = from {
            if let Some(a) = &f.actions {
                actions = a.clone();
            }
            chat.extend(f.chat.iter().cloned());
            (turn, leaves) = (f.turn.clone(), f.leaves.clone());
        }
        self.replay.frames.push(Frame { dt, commands, actions: Some(actions), turn, checksum: Some(checksum), leaves, chat });
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

    /// A network game's saves carry its name, its stat with the lobby's options and
    /// the host's name, and its people (as the original's save of a LAN game); a game
    /// played alone's carry none of it.
    #[test]
    fn network_games_describe_themselves_in_saves() {
        let slot = |player: u8, fill: crate::SlotFill, name: Option<&str>| crate::LobbySlot { player, fill, name: name.map(Into::into), race: None, team: player as i32, color: player as i32, handicap: 100.0, difficulty: 1 };
        let mut r = Replay {
            map: "Maps\\(2)BootyBay.w3m".into(),
            lobby: Some(vec![slot(0, crate::SlotFill::Local, Some("Host")), slot(1, crate::SlotFill::Remote, Some("Jack"))]),
            flags: 0x648,
            game_name: Some("w3-net host".into()),
            ..Default::default()
        };
        let head = r.network_head().expect("a network game");
        assert_eq!((head.name.as_str(), head.words), ("w3-net host", [2, 9]));
        let stat = w3_net::GameStat::parse(&head.stat).unwrap();
        assert_eq!((stat.settings, stat.host.as_str()), (0x64802, "Host"));
        // With the game's record: its stat as offered (a joined game's host's name and
        // size), its slots and type, and its slot table as the original's save of Booty
        // Bay has it.
        let offered = w3_net::GameStat { settings: 0x64802, map_width: 160, map_height: 72, map_xoro: 0x6ee78f13, map_path: "Maps\\(2)BootyBay.w3m".into(), host: "Host".into(), map_sha1: [0; 20] }.to_bytes();
        let s = |pid: u8, team: u8| NetRecordSlot { pid, download: 100, status: 2, computer: false, team, color: team, race: 0x60, ai: 1, handicap: 100 };
        r.lobby = Some(vec![slot(0, crate::SlotFill::Remote, Some("Host")), slot(1, crate::SlotFill::Local, Some("Jack"))]);
        r.seed = 0x35000000;
        r.net = Some(NetRecord { game_type: 9, layout: 0, slots: vec![s(1, 0), s(2, 1)], stat: offered.clone(), start_spots: 2 });
        let head = r.network_head().unwrap();
        assert_eq!((head.stat, head.words, head.game_type, head.flags), (offered, [2, 9], 9, 0x648));
        assert_eq!(head.local_slot, Some(1));
        assert_eq!(head.slots.unwrap(), [2, 1, 100, 2, 0, 0, 0, 0x60, 1, 100, 2, 100, 2, 0, 1, 1, 0x60, 1, 100, 0, 0, 0, 0x35, 0, 2]);
        r.lobby = Some(vec![slot(0, crate::SlotFill::Local, Some("Host")), slot(1, crate::SlotFill::Computer, None)]);
        assert!(r.network_head().is_none());
    }

    /// A game played alone is written as the original writes one, all of it in the
    /// original's records, and reads back as it was: its setup from the record, each
    /// frame's length and actions, its chat line before the turn after it, and the local
    /// player's leaving at the end.
    #[test]
    fn replays_keep_everything_in_the_originals_file() {
        let lobby = vec![
            crate::LobbySlot { player: 0, fill: crate::SlotFill::Local, name: Some("Jack".into()), race: Some(1), team: 0, color: 0, handicap: 100.0, difficulty: 1 },
            crate::LobbySlot { player: 1, fill: crate::SlotFill::Computer, name: None, race: None, team: 1, color: 1, handicap: 90.0, difficulty: 2 },
        ];
        let train = vec![0x10, 0x42, 0, 0x61, 0x65, 0x70, 0x68, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff];
        let r = Replay {
            map: "Maps\\(2)BootyBay.w3m".into(),
            lobby: Some(lobby),
            version: 1,
            flags: 0x648,
            seed: 0x35000000,
            signed: true,
            frames: vec![
                Frame { dt: 0.017, actions: Some(vec![]), checksum: Some(5), ..Default::default() },
                Frame { dt: 0.016, actions: Some(vec![(0, train.clone())]), chat: vec![(1, 0, "hi".into())], checksum: Some(6), ..Default::default() },
            ],
            ..Default::default()
        };
        let b = r.to_bytes().unwrap();
        let w = w3_formats::w3g::Replay::parse(&b).unwrap();
        assert_eq!((w.seed, w.slots.len(), w.host.name.as_str(), w.game_type, w.player_count, w.language), (0x35000000, 2, "Jack", 9, 12, LANGUAGE));
        use w3_formats::w3g::Record as R;
        assert!(matches!(&w.records[3..], [R::Turn { ms: 17, .. }, R::Checksum(_), R::Chat { pid: 1, scope: Some(0), .. }, R::Turn { ms: 16, .. }, R::Checksum(_), R::Leave { reason: 12, pid: 1, result: 7, unknown: 1 }]));
        let back = Replay::parse(&b).unwrap();
        assert_eq!((back.local_name.as_deref(), back.map_stat.map(|m| m.path)), (Some("Jack"), Some("Maps\\(2)BootyBay.w3m".to_string())));
        assert_eq!((back.flags, back.signed, back.lobby.map(|l| l.len())), (0x648, true, Some(2)));
        let f = &back.frames[1];
        assert_eq!((f.dt, &f.actions, &f.chat, f.checksum), (0.016, &Some(vec![(0, train)]), &vec![(1, 0, "hi".to_string())], Some(6)));
        assert!(back.frames.iter().all(|f| f.leaves.is_empty()));
    }

    /// A map loaded directly is written with the original's record of one (no setup
    /// screen: its settings MAP_LOCK_RANDOM_SEED alone, the record's slots), and reads
    /// back as such a game, without a setup screen and by the record's local name.
    #[test]
    fn a_map_loaded_directly_reads_back_so() {
        let slot = |pid: u8, computer: bool| NetRecordSlot { pid, download: 100, status: 2, computer, team: computer as u8, color: computer as u8, race: 0x60, ai: 1, handicap: 100 };
        let r = Replay {
            map: "Maps\\Test\\bbq.w3x".into(),
            flags: LOCK_RANDOM_SEED,
            local_name: Some("WorldEdit".into()),
            net: Some(NetRecord { game_type: 1, layout: 0, slots: vec![slot(1, false), slot(0, true)], stat: Vec::new(), start_spots: 2 }),
            frames: vec![Frame { dt: 0.1, actions: Some(vec![]), ..Default::default() }],
            ..Default::default()
        };
        let b = r.to_bytes().unwrap();
        let w = w3_formats::w3g::Replay::parse(&b).unwrap();
        assert_eq!((w.host.pid, w.host.name.as_str(), w.game_type, w.player_count, w.slots.len()), (1, "WorldEdit", 1, 12, 2));
        assert_eq!(w3_net::GameStat::parse(&w.stat).unwrap().settings, 0x800002);
        let back = Replay::parse(&b).unwrap();
        assert!(back.lobby.is_none());
        assert_eq!(back.local_name.as_deref(), Some("WorldEdit"));
    }

    /// A network game's leaves come before the turn after them, as the other player's
    /// (reason 1 and the word PLAYERLEFT carried), and read back so; as the record ends,
    /// a leave for each player still in it.
    #[test]
    fn a_network_games_leaves_keep() {
        let slot = |player: u8, fill: crate::SlotFill| crate::LobbySlot { player, fill, name: Some(format!("P{player}")), race: None, team: player as i32, color: player as i32, handicap: 100.0, difficulty: 1 };
        let r = Replay {
            map: "Maps\\(2)BootyBay.w3m".into(),
            lobby: Some(vec![slot(0, crate::SlotFill::Local), slot(1, crate::SlotFill::Remote), slot(2, crate::SlotFill::Remote)]),
            frames: vec![
                Frame { dt: 0.1, actions: Some(vec![]), turn: Some((100, vec![])), ..Default::default() },
                Frame { dt: 0.1, actions: Some(vec![]), turn: Some((100, vec![])), leaves: vec![Leave { pid: 2, player: 1, word: 7 }], ..Default::default() },
            ],
            ..Default::default()
        };
        let b = r.to_bytes().unwrap();
        let w = w3_formats::w3g::Replay::parse(&b).unwrap();
        use w3_formats::w3g::Record as R;
        assert!(matches!(&w.records[5..], [R::Leave { reason: 1, pid: 2, result: 7, .. }, R::Turn { .. }, R::Checksum(_), R::Leave { reason: 12, pid: 1, .. }, R::Leave { reason: 12, pid: 3, .. }]));
        let back = Replay::parse(&b).unwrap();
        assert_eq!(back.frames[1].leaves, vec![Leave { pid: 2, player: 1, word: 7 }]);
        // A game joined: the host's first in the record, by the slot table's network ids,
        // the local player among the others (as the original's replay of a LAN game it
        // joined as the second player).
        let s = |pid: u8| NetRecordSlot { pid, download: 100, status: 2, computer: false, team: pid, color: pid, race: 0x60, ai: 1, handicap: 100 };
        let r = Replay { lobby: Some(vec![slot(0, crate::SlotFill::Remote), slot(1, crate::SlotFill::Local)]), net: Some(NetRecord { game_type: 1, layout: 0, slots: vec![s(1), s(3)], stat: Vec::new(), start_spots: 2 }), ..r };
        let w = w3_formats::w3g::Replay::parse(&r.to_bytes().unwrap()).unwrap();
        assert_eq!((w.host.pid, w.host.name.as_str(), w.players.iter().map(|p| p.pid).collect::<Vec<_>>()), (1, "P0", vec![3]));
    }

    /// A network game's turn with a trigger's resumption (the original's 0x62 from its
    /// second player, as Mafarazzo TD's came) is written as it came and reads back as a
    /// turn whose block plays as that player's resumption.
    #[test]
    fn a_network_turns_resumption_keeps() {
        let slot = |player: u8, fill: crate::SlotFill| crate::LobbySlot { player, fill, name: Some(format!("P{player}")), race: None, team: player as i32, color: player as i32, handicap: 100.0, difficulty: 1 };
        let block = vec![0x62, 0x60, 5, 0, 0, 0x60, 5, 0, 0, 1, 0, 0, 0];
        let r = Replay {
            map: "Maps\\(2)BootyBay.w3m".into(),
            lobby: Some(vec![slot(0, crate::SlotFill::Local), slot(1, crate::SlotFill::Remote)]),
            frames: vec![Frame { dt: 0.1, actions: Some(vec![(1, block.clone())]), turn: Some((100, vec![(2, block.clone())])), ..Default::default() }],
            ..Default::default()
        };
        let back = Replay::parse(&r.to_bytes().unwrap()).unwrap();
        let f = &back.frames[0];
        assert_eq!((&f.turn, &f.actions), (&Some((100, vec![(2, block.clone())])), &Some(vec![(1, block.clone())])));
        let w = crate::world::World::new(std::sync::Arc::new(w3_mpq::Chain::default()), crate::objects::ObjectDb::default());
        let cs = crate::actions::Codec::default().decode(&w, 1, &block).unwrap();
        assert_eq!(cs, vec![Command::ResumeTrigger { player: 1, execution: (0x560, 0x560), count: 1 }]);
    }
}
