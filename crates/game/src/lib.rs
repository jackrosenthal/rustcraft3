//! The game: object data, world state, and the JASS host that runs map scripts.

pub mod ability;
pub mod actions;
pub mod ai;
pub mod aiferry;
pub mod aimisc;
pub mod aiplace;
pub mod aiposts;
pub mod aitargets;
pub mod autocast;
pub mod blight;
pub mod camera;
pub mod cargo;
pub mod event_ids;
pub mod fog;
pub mod forms;
pub mod handles;
pub mod hero;
pub mod humanspells;
pub mod nightelfspells;
pub mod guard;
pub mod warp;
pub mod deaths;
pub mod deform;
pub mod illusions;
pub mod orcspells;
pub mod undeadspells;
pub mod herospells;
pub mod save;
pub mod w3zscript;
pub mod w3zstate;
mod w3zload;
pub mod targets;
pub mod passives;
pub mod techtree;
pub mod bolts;
pub mod bullets;
pub mod tasks;
pub mod bufftable;
pub mod info;
pub mod mining;
pub mod morph;
pub mod items;
pub mod persist;
pub mod shops;
pub mod spellai;
pub mod spells;
pub mod stampede;
pub mod structupgrade;
pub mod volcano;
pub mod w3zsave;
pub mod unitspells;
pub mod upgrades;
pub mod natives;
pub mod objects;
pub mod cheats;
pub mod fire;
pub mod collections;
pub mod replay;
pub mod orbs;
pub mod orders;
pub mod paint;
pub mod preload;
pub mod spellbook;
pub mod accel;
pub mod accelpath;
pub mod budget;
pub use w3_formats::codes;
pub mod datahash;
pub mod efloat;
mod efloat_recip;
pub mod lrpath;
pub mod motion;
pub mod pathing;
pub mod placement;
pub mod repair;
pub mod keepalive;
pub mod goldrun;
pub mod groupwalk;
pub mod selection;
pub mod techreg;
pub mod rng;
pub mod tags;
pub mod score;
pub mod sim;
pub mod types;
pub mod world;

use crate::collections::HashMap;
use std::sync::Arc;

use anyhow::Context;
use w3_formats::map::{MapInfo, Terrain};
use w3_jass::Vm;
use w3_mpq::Chain;

pub use objects::ObjectDb;
pub use world::World;

use types::{Rect, map_control, slot_state};

/// The turn time one tick of the original's turn loop takes, in milliseconds
/// (`FUN_6f32d670`'s 0x19; `Game::advance`).
pub const TICK_MS: u32 = 25;

pub struct Game {
    pub vm: Vm,
    pub world: World,
    pub info: MapInfo,
    /// The replay being recorded, if any.
    pub recorder: Option<replay::Recorder>,
    /// While a replay plays, the interface's commands are ignored.
    pub locked: bool,
    /// In a network game the interface's commands go out here, to come back in frames
    /// from the host; the last selection sent, so an unchanged one is not sent again.
    pub outbox: Option<Vec<replay::Command>>,
    sent_selection: Option<Vec<u32>>,
    /// Each player's selection as the actions played so far left it.
    codec: actions::Codec,
    /// The local player's selection changes in a game played alone as actions, and as
    /// those actions read back (`apply`).
    local_codec: (actions::Codec, actions::Codec),
    /// A network game's save its turn's actions called for, by a player under a name,
    /// made after the turn's keepalive (`after_keepalive`); and the copies this
    /// computer made, for the program to keep.
    save_due: Option<(u8, String)>,
    pub saves: Vec<NetSaveMade>,
    /// The last keepalive a turn made (`turn_keepalive`).
    last_keepalive: u32,
}

/// A network game's save as this computer made it (`Game::after_keepalive`): its name
/// as the action gave it, and the save (`Game::save_files`), or why it was not made.
pub struct NetSaveMade {
    pub name: String,
    pub file: Result<Vec<u8>, String>,
}

/// How a player slot is filled when the game starts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SlotFill {
    Local,
    /// A human playing on another computer of a network game.
    Remote,
    Computer,
    Empty,
    /// An observer (a slot of the lobby's Observers team): on this computer or not.
    Observer(bool),
}

/// What the engine makes as a game starts, before the map's objects: by default (no
/// computers), nothing beyond what every game makes.
#[derive(Clone, Copy, Debug, Default)]
pub struct Start {
    /// Computer players playing.
    pub computers: usize,
}

impl Start {
    /// As a lobby starts its game.
    pub fn of(lobby: &[LobbySlot]) -> Start {
        Start { computers: lobby.iter().filter(|s| matches!(s.fill, SlotFill::Computer)).count() }
    }

    /// As a game begun without a setup screen starts (`Game::start_without_setup`):
    /// the map's computers, with a campaign's other users, or a second user of a map
    /// with no computer of its own, made computers.
    pub fn without_setup(info: &MapInfo, campaign: bool) -> Start {
        let kinds: Vec<u32> = info.players.iter().map(|p| p.kind).collect();
        let (users, computers) = (kinds.iter().filter(|&&k| k == 1).count(), kinds.iter().filter(|&&k| k == 2).count());
        let extra = if campaign { users.saturating_sub(1) } else { (computers == 0 && users >= 2) as usize };
        Start { computers: computers + extra }
    }
}

/// One player's choices from the game setup screen.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct LobbySlot {
    pub player: u8,
    pub fill: SlotFill,
    pub name: Option<String>,
    /// RACE_HUMAN through RACE_NIGHTELF; `None` for random.
    pub race: Option<i32>,
    pub team: i32,
    pub color: i32,
    /// Hit point percentage (100 down to 50).
    pub handicap: f32,
    /// AI_DIFFICULTY_NEWBIE, _NORMAL, or _INSANE for computer players.
    pub difficulty: i32,
}

fn read_script(chain: &Chain, names: &[&str]) -> anyhow::Result<String> {
    for n in names {
        if let Ok(s) = chain.read_string(n) {
            return Ok(s);
        }
    }
    anyhow::bail!("none of {names:?} found")
}

impl Game {
    /// Loads the mounted map's script and data.  `version` is 0 for Reign of Chaos and 1
    /// for The Frozen Throne.
    pub fn load(chain: Arc<Chain>, version: i32) -> anyhow::Result<Game> {
        Self::load_with_keys(chain, version, None)
    }

    /// As `load`, with the player's CustomKeys.txt (already filtered by
    /// `objects::custom_keys`) laid over the game's strings before the map's own changes.
    pub fn load_with_keys(chain: Arc<Chain>, version: i32, custom_keys: Option<&w3_formats::profile::Profile>) -> anyhow::Result<Game> {
        Self::load_for(chain, version, custom_keys, Start::default())
    }

    /// As `load_with_keys`, for a game started as `start` says: the engine makes objects
    /// for its computers and its interface as the map loads, before the map's own.
    pub fn load_for(chain: Arc<Chain>, version: i32, custom_keys: Option<&w3_formats::profile::Profile>, start: Start) -> anyhow::Result<Game> {
        let info = MapInfo::parse(&chain.read("war3map.w3i")?).context("war3map.w3i")?;
        let common = read_script(&chain, &["Scripts\\common.j"])?;
        let blizzard = read_script(&chain, &["Scripts\\Blizzard.j"])?;
        let map = read_script(&chain, &["war3map.j", "Scripts\\war3map.j"])?;
        let module = w3_jass::compile([("common.j", &*common), ("Blizzard.j", &*blizzard), ("war3map.j", &*map)])?;
        let mut vm = Vm::new(module);
        let mut objects = ObjectDb::load(&chain);
        if let Some(k) = custom_keys {
            objects.apply_custom_keys(k);
        }
        objects.apply_campaign(&chain);
        objects.apply_map(&chain);
        let mut world = World::new(chain.clone(), objects);
        world.version = version;
        world.editor_version = info.editor_version;
        world.melee_map = info.flags & 0x4 != 0;
        world.fixed_player_settings = info.flags & 0x20 != 0;
        world.fixed_start_players = info.players.iter().filter(|p| p.fixed_start && p.id < 32).fold(0, |m, p| m | 1 << p.id);
        // w3i flag 0x2000: the map has terrain fog.
        if info.flags & 0x2000 != 0 {
            let c = info.fog_color;
            let f = (info.fog_style as i32, info.fog_start, info.fog_end, info.fog_density, [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0]);
            world.map_terrain_fog = Some(f);
            world.terrain_fog = Some(f);
        }
        world.bind_natives(&vm);
        // The map's gameplay constants (war3mapMisc.txt) over the game's.
        if let Ok(s) = chain.read_string("war3mapMisc.txt") {
            world.misc.merge(&s);
        }
        // Interface strings (FDF StringList tables) resolve GetLocalizedString keys.
        world.strings = w3_ui::FrameDb::load(&chain).strings.into_iter().collect();
        // The world editor's strings name destructables and doodads (WESTRING_...).
        for f in ["UI\\WorldEditStrings.txt", "UI\\WorldEditGameStrings.txt"] {
            if let Ok(s) = chain.read_string(f) {
                let p = w3_formats::profile::Profile::parse(&s);
                if let Some(sec) = p.sections.get("worldeditstrings") {
                    for (k, v) in sec {
                        world.strings.entry(k.to_ascii_uppercase()).or_insert_with(|| v.trim().trim_matches('"').to_string());
                    }
                }
            }
        }
        if let Ok(s) = chain.read_string("war3map.wts") {
            world.trigger_strings = w3_formats::map::parse_wts(&s).into_iter().collect();
        }
        if let Ok(t) = chain.read("war3map.w3e").map_err(anyhow::Error::from).and_then(|b| Terrain::parse(&b).map_err(Into::into)) {
            let (w, h) = (t.width, t.height);
            world.tileset = t.tileset;
            world.map_bounds = Rect {
                min_x: t.offset[0],
                min_y: t.offset[1],
                max_x: t.offset[0] + (w - 1) as f32 * 128.0,
                max_y: t.offset[1] + (h - 1) as f32 * 128.0,
            };
            world.terrain_heights = Some(world::TerrainHeights {
                width: w,
                height: h,
                offset: t.offset,
                // A middle ramp corner sits half a level up, as the terrain draws it.
                heights: {
                    let letters = w3_formats::map::ramp_letters(&t);
                    t.corners.iter().zip(letters).map(|(c, l)| c.height() + if l == b'R' { 64.0 } else { 0.0 }).collect()
                },
                layers: t.corners.iter().map(|c| c.layer_height).collect(),
                boundary: t.corners.iter().map(|c| c.boundary || c.map_edge).collect(),
            });
            world.blight = t.corners.iter().map(|c| c.blight).collect();
            world.ground = t.corners.iter().map(|c| (c.ground_texture, c.ground_variation)).collect();
            world.ground_tilesets = t.ground_tilesets.clone();
            let [left, right, bottom, top] = info.camera_complements.map(|v| v as f32 * 128.0);
            let b = world.map_bounds;
            world.playable = Rect { min_x: b.min_x + left, max_x: b.max_x - right, min_y: b.min_y + bottom, max_y: b.max_y - top };
            let th = world.terrain_heights.as_ref().unwrap();
            world.fog = Some(fog::FogMap::new(world.map_bounds, |x, y| th.layers.get(y.min(th.height as usize - 1) * th.width as usize + x.min(th.width as usize - 1)).copied().unwrap_or(0)));
        }
        if let (Ok(wpm), Some(t)) = (chain.read("war3map.wpm"), world.terrain_heights.as_ref()) {
            world.pathing = pathing::PathingMap::parse(&wpm, t.offset);
        }
        // The script's globals come first, and the engine's own objects (read under
        // Wine): the game, flags and properties, and the queue after the globals' first
        // (Blizzard.j's first timer); then the pathing and acceleration maps, the
        // pathing system, the game cache, and the AI.
        world.tags.after_next(&[codes::class::GAME_STATE, codes::class::FLAG, codes::class::FLAG, codes::class::REAL, codes::class::REAL, codes::class::FLAG, codes::class::THRESHOLD_LISTENER, codes::class::TRIGGER_QUEUE]);
        vm.init_globals(&mut world).map_err(anyhow::Error::msg)?;
        world.tags.flush();
        world.tags.take_all(&[codes::class::PATHING_MAP, codes::class::PATHING_MAP, codes::class::ACCELERATOR_GRID, codes::class::ACCELERATOR_GRID, codes::class::ACCELERATOR_GRID, codes::class::ACCELERATOR_GRID, codes::class::LONG_RANGE_SYSTEM, codes::class::LONG_RANGE_ACCELERATOR, codes::class::GAME_CACHE_MANAGER]);
        // An AI for each computer playing, then Neutral Hostile's (FUN_6f1e9a40;
        // measured under Wine against one computer).
        // Each starts three timers as it is made (`FUN_6f9c7b20`, its +0x154 and +0x15c;
        // traced on the quiet test map, the game's first six).
        for _ in 0..start.computers + 1 {
            let s = world.tags.take(codes::class::PLAYER_AI);
            let birth = world.tags.live.get(&s).map_or(0, |e| e.1);
            world.tags.player_ais.push((s, birth));
            let counts = [0; 3].map(|_| world.tags.start_timer("9c7b20"));
            world.tags.player_ai_timers.push(counts);
        }
        world.camera_bounds = info.camera_bounds;
        // The map's global weather (w3i) covers the whole map.
        if info.weather != [0; 4] {
            world.add_weather(types::Weather { rect: world.map_bounds, id: info.weather, enabled: true });
        }
        for p in &info.players {
            let name = world.localized(&p.name);
            if let Some(pl) = world.players.get_mut(p.id as usize) {
                pl.name = name;
            }
        }
        Ok(Game { vm, world, info, recorder: None, locked: false, outbox: None, sent_selection: None, codec: Default::default(), local_codec: Default::default(), save_due: None, saves: Vec::new(), last_keepalive: 0 })
    }

    /// Runs the map's `config` function, which declares players and start locations,
    /// then places the map's destructables (read: their handles follow Player(0) and
    /// Player(1), which the config names, in a save's handle table).
    pub fn config(&mut self) -> anyhow::Result<()> {
        let r = self.call("config");
        self.place_destructables();
        r
    }

    /// The destructables war3map.doo places.
    fn place_destructables(&mut self) {
        if let Ok(bytes) = self.world.chain.read("war3map.doo") {
            match w3_formats::map::Doodads::parse(&bytes) {
                Ok(doo) => {
                    for d in &doo.doodads {
                        if self.world.objects.destructables.get(d.id).is_none() {
                            continue;
                        }
                        // One neither visible nor solid is the script's to make: the
                        // editor writes it there too (measured: UndeadX05's four gates
                        // and three trees, flags 0, are made by CreateAllDestructables,
                        // and the original has one of each where ours had two, its
                        // killed gate standing closed over the dead one; every campaign
                        // map has as many such entries as it has destructables made so).
                        if d.flags & 3 == 0 {
                            continue;
                        }
                        // One flagged 3 is scenery: drawn, but not a destructable
                        // (measured: Booty Bay's three such trees are not found by
                        // EnumDestructablesInRect and take no tags in the original,
                        // which draws them).
                        if d.flags == 3 {
                            continue;
                        }
                        let h = natives::objects::create_destructable(
                            &mut self.world,
                            d.id,
                            d.position[0],
                            d.position[1],
                            Some(d.position[2]),
                            d.angle.to_degrees(),
                            d.scale[0],
                            d.variation,
                            d.life == 0,
                        );
                        // The placed life is a percentage of the type's maximum.
                        if let Some(types::Obj::Destructable(x)) = self.world.handles.get_mut(h) {
                            x.life = x.max_life * d.life as f32 / 100.0;
                            x.placed_facing = Some(d.angle);
                        }
                    }
                }
                Err(e) => log::warn!("war3map.doo: {e}"),
            }
        }
    }

    /// Applies the setup screen's choices, then runs the map's `main`.  The setup's
    /// advanced options are the map flags scripts read (IsMapFlagSet): Random Races
    /// (0x40000) draws every race, and the visibility always visible (0x4) or map
    /// explored (0x2) lifts the fog or its mask.
    pub fn start_lobby(&mut self, slots: &[LobbySlot], flags: u32) -> anyhow::Result<()> {
        self.world.map_flags = flags as i32;
        self.world.lobby_teams = true;
        self.seed_generators();
        if flags & 4 != 0 {
            self.world.fog_enabled = false;
            self.world.fog_mask_enabled = false;
        } else if flags & 2 != 0 {
            self.world.fog_mask_enabled = false;
        }
        let random_races = flags & 0x40000 != 0;
        // Observers on defeat (MAP_OBSERVERS_ON_DEATH): each player's
        // PLAYER_STATE_OBSERVER_ON_DEATH.
        if flags & 0x20 != 0 {
            for p in self.world.players.iter_mut() {
                p.state.insert(crate::types::player_state::OBSERVER_ON_DEATH, 1);
            }
        }
        // RACE_PREF_* bits from common.j.
        const PREF_RANDOM: i32 = 32;
        for s in slots {
            let Some(p) = self.world.players.get_mut(s.player as usize) else { continue };
            if let Some(n) = &s.name {
                p.name = n.clone();
            }
            p.team = s.team;
            p.color = s.color;
            p.handicap = s.handicap / 100.0;
            match s.race.filter(|_| !random_races) {
                Some(r) => {
                    p.race = r;
                    p.race_preference = [0, 1, 2, 8, 4].get(r as usize).copied().unwrap_or(0);
                }
                None => p.race_preference = PREF_RANDOM,
            }
            if matches!(s.fill, SlotFill::Computer) {
                self.world.ai_difficulty.insert(s.player, s.difficulty);
            }
        }
        // Random races are drawn as the game starts (FUN_6f1e9dd0): for each of the 12
        // players in turn whose preference is random, the main generator's next number's
        // top two bits plus one.  A player the map does not have is random; one it has
        // whose slot the setup left open or closed draws none (measured: all 12 drew in a
        // two-player Booty Bay, 11 in Forestwalk with its third slot closed; read: the
        // original's replays of Forestwalk with its third slot open and of Lost Temple
        // with its fourth, from the original's seeds).
        for id in 0..12u8 {
            let random = match slots.iter().find(|s| s.player == id) {
                Some(s) if matches!(s.fill, SlotFill::Empty) => !self.info.players.iter().any(|i| i.id == id as u32),
                Some(s) => s.race.is_none() || random_races,
                None => !self.info.players.iter().any(|i| i.id == id as u32),
            };
            if random {
                let r = (self.world.rng.next_u32() >> 30) as i32 + 1;
                let p = &mut self.world.players[id as usize];
                p.race = r;
                p.race_preference = PREF_RANDOM;
            }
        }
        // Observers hold player slots the map leaves unused: they are users who do not
        // play (IsPlayerObserver), with no start location or units.
        for s in slots.iter().filter(|s| matches!(s.fill, SlotFill::Observer(_))) {
            if let Some(p) = self.world.players.get_mut(s.player as usize) {
                p.controller = map_control::USER;
            }
        }
        // A network game's open slots are placed too (read: Mafarazzo TD's LAN save,
        // PLAYER_STATE_PLACED 1 for its nine open user slots, where its game loaded
        // directly places only those playing; provisional: a game played alone's open
        // slots, and a closed slot's, not read).
        if slots.iter().any(|s| matches!(s.fill, SlotFill::Remote)) {
            for s in slots.iter().filter(|s| matches!(s.fill, SlotFill::Empty)) {
                if let Some(p) = self.world.players.get_mut(s.player as usize) {
                    p.state.insert(crate::types::player_state::PLACED, 1);
                }
            }
        }
        let fills: HashMap<u8, SlotFill> = slots.iter().map(|s| (s.player, s.fill.clone())).collect();
        self.start(|id, _| fills.get(&id).cloned().unwrap_or(SlotFill::Empty))
    }

    /// Starts a game begun without a setup screen: a campaign's local player in the
    /// first user slot and computers in the rest; another map, as the original's
    /// -loadfile, the local player in the first user slot and a computer in the second,
    /// each at the start location of its number, and the other user slots empty
    /// (measured: Player(1) a computer at start 1, Player(2) and Player(3) empty).
    pub fn start_without_setup(&mut self, campaign: bool) -> anyhow::Result<()> {
        self.world.fixed_starts = !campaign;
        // A map loaded directly runs with its random seed locked ('war3',
        // MAP_LOCK_RANDOM_SEED; measured: a test map's structures' alarm waits are the
        // locked seed's draws, whatever the setup's seed).
        if !campaign {
            self.world.map_flags |= 0x8000;
        }
        // A map with a computer slot of its own gets no other (measured: a test map
        // whose Player(1) is a computer slot leaves its user slots Player(2) and
        // Player(3) empty).
        let has_computer = self.world.players.iter().any(|p| p.controller == map_control::COMPUTER);
        let mut users = 0;
        self.start(|_, controller| {
            if controller != map_control::USER {
                return SlotFill::Computer;
            }
            users += 1;
            match users {
                1 => SlotFill::Local,
                2 if campaign || !has_computer => SlotFill::Computer,
                _ if campaign => SlotFill::Computer,
                _ => SlotFill::Empty,
            }
        })
    }

    /// The original's record of a map loaded directly (as -loadfile makes it; read: its
    /// replays of Glacial Thaw, a test map, and Mafarazzo TD): a slot for each of the
    /// map's users and computers in order, the local player's with network id 1, a
    /// computer's in a user slot and the user's with the map downloaded (100), the map's
    /// own computers' and the empty slots' unknown (255); each its player's team (0 for
    /// one not playing) and color; races random and selectable, or with fixed player
    /// settings the map's alone; normal computers, no handicap.  The select mode is 3
    /// with fixed player settings, 0 for a melee map; the type 1, with 8 for a map
    /// Blizzard signed (`signed`; read: Glacial Thaw's 9).
    pub fn loaded_record(&self, signed: bool) -> replay::NetRecord {
        use w3_net::slot::{DOWNLOAD_UNKNOWN, layout, race, status};
        const RACES: [u8; 5] = [race::RANDOM, race::HUMAN, race::ORC, race::UNDEAD, race::NIGHT_ELF];
        let fixed = self.info.flags & 0x20 != 0;
        let mut players: Vec<_> = self.info.players.iter().filter(|p| p.kind == 1 || p.kind == 2).collect();
        players.sort_by_key(|p| p.id);
        let slots = players
            .iter()
            .filter_map(|i| {
                let p = self.world.players.get(i.id as usize)?;
                let playing = p.slot_state == slot_state::PLAYING;
                let computer = playing && p.controller == map_control::COMPUTER;
                let local = playing && p.id == self.world.local_player && !computer;
                Some(replay::NetRecordSlot {
                    pid: local as u8,
                    download: if playing && i.kind == 1 { 100 } else { DOWNLOAD_UNKNOWN },
                    status: if playing { status::OCCUPIED } else { status::OPEN },
                    computer,
                    team: if playing { p.team.max(0) as u8 } else { 0 },
                    color: p.color as u8,
                    race: if fixed { RACES[(i.race as usize).min(4)] } else { race::RANDOM | race::SELECTABLE },
                    ai: 1,
                    handicap: 100,
                })
            })
            .collect::<Vec<_>>();
        let forces = self.info.flags & 0x40 != 0;
        let layout = match (forces, fixed) {
            (false, _) => layout::MELEE,
            (true, true) => layout::FIXED_PLAYER_SETTINGS,
            (true, false) => layout::CUSTOM_FORCES,
        };
        replay::NetRecord { game_type: 1 | if signed { 8 } else { 0 }, layout, start_spots: slots.len() as u8, slots, stat: Vec::new() }
    }

    /// Fills player slots as the lobby would, then runs the map's `main`.
    /// The generators start from the game's seed (FUN_6f29e300): the main one seeded with
    /// it, and the other 45 from it.
    fn seed_generators(&mut self) {
        self.last_keepalive = 0;
        let seed = rng::Rng::game_seed(self.world.seed, self.world.map_flags);
        self.world.rng = rng::Rng::new(seed);
        self.world.rand_data = rng::RandData::new(seed);
    }

    pub fn start(&mut self, mut fill: impl FnMut(u8, i32) -> SlotFill) -> anyhow::Result<()> {
        if !self.world.lobby_teams {
            self.seed_generators();
        }
        // Each user and computer slot's fill, in the players' order.
        let fills: Vec<Option<SlotFill>> = self.world.players.iter().map(|p| matches!(p.controller, map_control::USER | map_control::COMPUTER).then(|| fill(p.id, p.controller))).collect();
        // A map loaded directly draws as the game starts, on the main generator: a random
        // race for each of the 12 players whose preference is random, all but a map's slot
        // left open (`FUN_6f1e9dd0`), then a shuffle of the free start locations for each
        // team placed, the largest first (`FUN_6f1e9e60` from `FUN_6f1e9fd0`, for i from
        // 1: a draw each).  Traced under -loadfile, the main generator's alone: the quiet
        // test map's 12 and 1, Forestwalk's 11 (its third slot open) and 3, before the
        // wanderers' waits; with them the undead computer's hero picks agree.  A map with
        // fixed player settings draws neither: its races are its own (a preference not
        // random, RACE_PREF_RANDOM 0x20, or none set), and its placement keeps the
        // locations it set (MAP_PLACEMENT_FIXED and USE_MAP_SETTINGS, 1 and 2), the
        // locations it forced taken first (read: the original's Mafarazzo TD begun so, its
        // main generator undrawn at its first keepalive, where the port had drawn 23).
        if self.world.fixed_starts {
            let fixed = self.info.flags & 0x20 != 0;
            for (p, f) in self.world.players.iter().zip(&fills).take(12) {
                let random = f.is_none() || !fixed || p.race_preference & 0x20 != 0;
                if !matches!(f, Some(SlotFill::Empty)) && random {
                    self.world.rng.next_u32();
                }
            }
            let mut teams: Vec<(i32, usize)> = Vec::new();
            for (p, f) in self.world.players.iter().zip(&fills) {
                if matches!(f, Some(SlotFill::Local | SlotFill::Remote | SlotFill::Computer)) && !p.start_forced {
                    match teams.iter_mut().find(|t| t.0 == p.team) {
                        Some(t) => t.1 += 1,
                        None => teams.push((p.team, 1)),
                    }
                }
            }
            if matches!(self.world.placement, 1 | 2) {
                teams.clear();
            }
            teams.sort_by(|a, b| b.1.cmp(&a.1));
            let forced = self.world.players.iter().take(types::PLAYER_NEUTRAL_AGGRESSIVE as usize).filter(|p| p.start_forced).count();
            let mut free = self.world.start_locations.len().saturating_sub(forced);
            for (_, size) in teams {
                for _ in 1..free {
                    self.world.rng.next_u32();
                }
                free = free.saturating_sub(size);
            }
        }
        // Each player's start (FUN_6f1e7c20): passive toward Neutral Passive and it
        // toward them; food capped at [Misc] FoodCeiling (90 for Reign of Chaos, 100 for
        // The Frozen Throne, kept to 1 to 300: FUN_6f047250); the game's result neutral
        // (PLAYER_GAME_RESULT_NEUTRAL).  The four neutral players give bounty.
        let ceiling = self.world.misc.get_str("Misc", "FoodCeiling").and_then(|v| v.trim().parse::<i32>().ok()).map_or(if self.world.version >= 1 { 100 } else { 90 }, |v| v.clamp(1, 300));
        let passive = types::PLAYER_NEUTRAL_PASSIVE as usize;
        for i in 0..self.world.players.len() {
            self.world.players[i].alliances[passive][0] = true;
            self.world.players[passive].alliances[i][0] = true;
            let p = &mut self.world.players[i];
            p.state.insert(types::player_state::FOOD_CAP_CEILING, ceiling);
            p.state.insert(types::player_state::GAME_RESULT, 3);
        }
        for p in self.world.players.iter_mut().skip(types::PLAYER_NEUTRAL_AGGRESSIVE as usize).take(4) {
            p.state.insert(types::player_state::GIVES_BOUNTY, 1);
        }
        for (p, f) in self.world.players.iter_mut().zip(fills) {
            let controller = p.controller;
            if let Some(f) = f {
                match f {
                    SlotFill::Local => {
                        p.controller = map_control::USER;
                        p.slot_state = slot_state::PLAYING;
                        self.world.local_player = p.id;
                    }
                    SlotFill::Remote => {
                        p.controller = map_control::USER;
                        p.slot_state = slot_state::PLAYING;
                    }
                    SlotFill::Computer => {
                        p.controller = map_control::COMPUTER;
                        p.slot_state = slot_state::PLAYING;
                        // A map loaded directly sets its computer in a user slot to play
                        // Undead (measured on TwistedMeadows and EchoIsles, whose scripts
                        // prefer Orc for it).
                        if self.world.fixed_starts && controller == map_control::USER {
                            p.race = 3;
                        }
                        // Without fixed player settings (w3i flag 0x20) the lobby names
                        // computer slots by difficulty.
                        if self.info.flags & 0x20 == 0 {
                            let key = match self.world.ai_difficulty.get(&p.id).copied().unwrap_or(1) {
                                0 => "COMPUTER_NEWBIE",
                                2 => "COMPUTER_INSANE",
                                _ => "COMPUTER_NORMAL",
                            };
                            p.name = self.world.strings.get(key).cloned().unwrap_or_else(|| key.to_string());
                        }
                    }
                    SlotFill::Empty => p.slot_state = slot_state::EMPTY,
                    SlotFill::Observer(local) => {
                        p.controller = map_control::USER;
                        p.slot_state = slot_state::EMPTY;
                        p.state.insert(crate::types::player_state::OBSERVER, 1);
                        p.start_location = -1;
                        if local {
                            self.world.local_player = p.id;
                        }
                    }
                }
            }
            // Races follow the map's player setup.
            if let Some(info) = self.info.players.iter().find(|i| i.id == p.id as u32) {
                if p.race == 0 {
                    p.race = match info.race {
                        0 => 1,
                        r => r as i32,
                    };
                }
            }
            // Each player in the game is placed at a start (measured:
            // PLAYER_STATE_PLACED is 1 for the user and computer playing and 0 for the
            // rest).
            if p.slot_state == slot_state::PLAYING {
                p.state.insert(crate::types::player_state::PLACED, 1);
            }
            // Players not in the game, the neutrals among them, are on no team
            // (measured: GetPlayerTeam is -1 for them).
            let in_game = p.slot_state == slot_state::PLAYING && (p.controller == map_control::USER || p.controller == map_control::COMPUTER);
            if !in_game && p.get_state(crate::types::player_state::OBSERVER) == 0 {
                p.team = -1;
            }
        }
        // The players take their start locations by the map's placement.
        if self.world.fixed_starts {
            for p in self.world.players.iter_mut().take(types::PLAYER_NEUTRAL_AGGRESSIVE as usize) {
                p.start_location = p.id as i32;
            }
        } else {
            self.world.place_start_locations();
        }
        // Neutral players always play, as creeps (MAP_CONTROL_CREEP, as game.dll sets
        // all four).
        for i in types::PLAYER_NEUTRAL_AGGRESSIVE..types::PLAYER_COUNT as u8 {
            let p = &mut self.world.players[i as usize];
            p.slot_state = slot_state::PLAYING;
            p.controller = map_control::CREEP;
        }
        // Players the map gives a neutral or rescuable controller are passive toward
        // every user and computer player and toward each other, and those toward them
        // (FUN_6f1e9bf0, as the game loads).
        self.ally_teams();
        let calm: Vec<usize> = (0..types::PLAYER_COUNT).filter(|&i| matches!(self.world.players[i].controller, map_control::NEUTRAL | map_control::RESCUABLE)).collect();
        let active: Vec<usize> = (0..types::PLAYER_COUNT).filter(|&i| matches!(self.world.players[i].controller, map_control::USER | map_control::COMPUTER)).collect();
        const PASSIVE: usize = 0;
        for (k, &n) in calm.iter().enumerate() {
            for &u in active.iter().chain(&calm[k..]) {
                self.world.players[n].alliances[u][PASSIVE] = true;
                self.world.players[u].alliances[n][PASSIVE] = true;
            }
        }
        self.call("main")?;
        // The engine's own objects after main, by the first turn (read under Wine, the
        // same with or without the map's units).
        self.world.time_of_day_ready = true;
        self.world.make_time_of_day_listeners();
        // The game's two periodic timers, events 0x80269 and 0x8026a (`FUN_6f28ba80`,
        // `FUN_6f28bac0`, as `FUN_6f1e5a10` ends the setup; traced: the quiet test map's
        // after its script's; provisional: before the wanderers' first waits).
        let clock = self.world.tags.clock;
        self.world.tags.game_timers[0] = self.world.tags.start_timer("28ba80");
        self.world.tags.game_timers[1] = self.world.tags.start_timer("28bac0");
        self.world.tags.game_timers_clock = clock;
        self.world.draw_first_wander_waits();
        Ok(())
    }

    /// Teammates ally as the game begins (FUN_6f29e300): each pair of playing,
    /// non-observer players on one team are passive, help each other, share experience
    /// and spells, share vision, and win together (PLAYER_STATE_ALLIED_VICTORY).  A map
    /// with its own forces (w3i flag 0x40) does each of these only as the team's force
    /// says: allied (0x1), allied victory (0x2), shared vision (0x8), shared units
    /// (0x10), and shared advanced units (0x20).
    fn ally_teams(&mut self) {
        let custom = self.info.flags & 0x40 != 0;
        // Without a setup screen a melee map's players stand alone, as the original
        // plays a map loaded with -loadfile; the map script's teams do not ally them.
        if !custom && !self.world.lobby_teams {
            return;
        }
        let forces: Vec<u32> = self.info.forces.iter().map(|f| f.flags).collect();
        let playing: Vec<usize> = (0..12)
            .filter(|&i| {
                let p = &self.world.players[i];
                p.slot_state == slot_state::PLAYING && p.get_state(types::player_state::OBSERVER) == 0
            })
            .collect();
        for (k, &a) in playing.iter().enumerate() {
            for &b in &playing[k + 1..] {
                let team = self.world.players[a].team;
                if team != self.world.players[b].team {
                    continue;
                }
                let flags = if custom {
                    match forces.get(team.max(0) as usize) {
                        Some(&f) => f,
                        None => continue,
                    }
                } else {
                    u32::MAX
                };
                let mut set = |kinds: &[usize]| {
                    for &t in kinds {
                        self.world.players[a].alliances[b][t] = true;
                        self.world.players[b].alliances[a][t] = true;
                    }
                };
                if flags & 0x1 != 0 {
                    set(&[0, 1, 2, 3, 4]);
                }
                if flags & 0x8 != 0 {
                    set(&[5]);
                }
                if custom && flags & 0x10 != 0 {
                    set(&[6]);
                }
                if custom && flags & 0x20 != 0 {
                    set(&[7]);
                }
                if flags & 0x2 != 0 {
                    for p in [a, b] {
                        self.world.players[p].state.insert(types::player_state::ALLIED_VICTORY, 1);
                    }
                }
            }
        }
    }

    /// Runs a function of the map script on its machine's own context, as the original
    /// runs `config` and `main` (`FUN_6f91c310`).
    fn call(&mut self, name: &str) -> anyhow::Result<()> {
        let f = self.vm.module.function(name).with_context(|| format!("map script has no {name}"))?;
        self.world.run_main(&mut self.vm, f);
        Ok(())
    }

    /// A player clicked a dialog button: the dialog closes for them, then the button's
    /// and the dialog's click events fire (EVENT_DIALOG_BUTTON_CLICK, EVENT_DIALOG_CLICK).
    /// The player's selection from the interface: units leaving it raise the deselect
    /// events and units joining it the select events.
    pub fn set_selection(&mut self, player: u8, units: &[u32]) {
        use event_ids::*;
        let old = self.world.selection.get(&player).cloned().unwrap_or_default();
        if old == units {
            return;
        }
        self.world.selection.insert(player, units.to_vec());
        // These belong to the selecting player, not the unit's owner.
        let fire = |world: &mut World, vm: &mut Vm, u: u32, pe: i32, ue: i32| {
            let ctx = types::EventCtx { unit: u, player, has_player: true, event_id: pe, ..Default::default() };
            world.fire(vm, ctx.clone(), move |_, k| matches!(k, types::EventKind::PlayerUnit { player: p, event, .. } if *p == player && *event == pe));
            world.fire(vm, types::EventCtx { event_id: ue, ..ctx }, move |_, k| matches!(k, types::EventKind::Unit { unit, event } if *unit == u && *event == ue));
        };
        for &u in old.iter().filter(|u| !units.contains(u)) {
            fire(&mut self.world, &mut self.vm, u, EVENT_PLAYER_UNIT_DESELECTED, EVENT_UNIT_DESELECTED);
        }
        for &u in units.iter().filter(|u| !old.contains(u)) {
            fire(&mut self.world, &mut self.vm, u, EVENT_PLAYER_UNIT_SELECTED, EVENT_UNIT_SELECTED);
        }
    }

    /// A player event with no other data: Escape (EVENT_PLAYER_END_CINEMATIC) and the
    /// arrow key events.
    /// A game event for the triggers registered on it (TriggerRegisterGameEvent: the game
    /// loaded or saved, the hero skill or build menu shown).
    pub fn game_event(&mut self, event: i32) {
        let ctx = types::EventCtx { event_id: event, ..Default::default() };
        self.world.fire(&mut self.vm, ctx, move |_, k| matches!(k, types::EventKind::Game(e) if *e == event));
    }

    pub fn player_event(&mut self, player: u8, event: i32) {
        let ctx = types::EventCtx { player, has_player: true, event_id: event, ..Default::default() };
        self.world.fire(&mut self.vm, ctx, move |_, k| matches!(k, types::EventKind::Player { player: p, event: e, .. } if *p == player && *e == event));
    }

    /// A line `player` typed: a cheat (see `cheats`), or chat that player chat events hear
    /// (an exact registration's text matching the line, another's found in it).  Returns
    /// whether the line was chat to show.
    pub fn chat(&mut self, player: u8, text: &str, to: u8) -> bool {
        // Cheats work only outside network games.
        if !self.world.network {
            if let Some(ctx) = self.world.console_command(&mut self.vm, player, text) {
                if ctx.event_id != 0 {
                    self.player_event(player, ctx.event_id);
                }
                return false;
            }
        }
        self.world.requests.push(world::Request::Chat { player, text: text.to_string(), to });
        for e in self.world.chat_hearers(player, text) {
            self.event_heard(player, e, text);
        }
        true
    }

    /// One event registration hears a player's input: a chat line it matches, or its
    /// trackable's hit or track.
    pub fn event_heard(&mut self, player: u8, event: u32, text: &str) {
        let Some(types::Obj::Event(r)) = self.world.handles.get(event) else { return };
        let trig = r.trigger;
        let ctx = match &r.kind {
            types::EventKind::PlayerChat { text: t, .. } => types::EventCtx { player, has_player: true, event_id: event_ids::EVENT_PLAYER_CHAT, chat: text.to_string(), matched_chat: t.clone(), ..Default::default() },
            types::EventKind::Trackable { trackable, hit } => {
                let event_id = if *hit { event_ids::EVENT_GAME_TRACKABLE_HIT } else { event_ids::EVENT_GAME_TRACKABLE_TRACK };
                types::EventCtx { player, has_player: true, trackable: *trackable, event_id, ..Default::default() }
            }
            _ => return,
        };
        self.world.fire_trigger(&mut self.vm, trig, ctx);
    }

    pub fn click_dialog_button(&mut self, player: u8, button: u32) {
        let Some(types::Obj::DialogButton(b)) = self.world.handles.get(button) else { return };
        let (dialog, quit) = (b.dialog, b.quit);
        if let Some(types::Obj::Dialog(d)) = self.world.handles.get_mut(dialog) {
            d.shown &= !(1 << player);
        }
        use event_ids::*;
        let ctx = types::EventCtx { dialog, button, player, has_player: true, event_id: EVENT_DIALOG_BUTTON_CLICK, ..Default::default() };
        self.world.fire(&mut self.vm, ctx, move |_, k| matches!(k, types::EventKind::DialogButton(b) if *b == button));
        let ctx = types::EventCtx { dialog, button, player, has_player: true, event_id: EVENT_DIALOG_CLICK, ..Default::default() };
        self.world.fire(&mut self.vm, ctx, move |_, k| matches!(k, types::EventKind::Dialog(d) if *d == dialog));
        // DialogAddQuitButton: ends the game, with the score screen when it says so.
        if let Some(score) = quit {
            self.world.requests.push(world::Request::EndGame { score });
        }
    }

    pub fn buy_item(&mut self, player: u8, shop: u32, id: objects::Id) -> Result<(), &'static str> {
        self.world.buy_item(&mut self.vm, player, shop, id)
    }

    pub fn hire_unit(&mut self, player: u8, shop: u32, id: objects::Id) -> Result<(), &'static str> {
        self.world.hire_unit(&mut self.vm, player, shop, id)
    }

    pub fn start_revive(&mut self, altar: u32, hero: u32) -> Result<(), &'static str> {
        self.world.start_revive(&mut self.vm, altar, hero)
    }

    /// The local player uses the item in a unit's inventory slot.
    pub fn use_item(&mut self, unit: u32, slot: usize) -> bool {
        self.world.use_item(&mut self.vm, unit, slot, 0, 0.0, 0.0)
    }

    /// Moves an item between inventory slots (swapping with what is there).
    pub fn move_item(&mut self, unit: u32, from: usize, to: usize) {
        if let Some(u) = self.world.unit_mut(unit) {
            if from < u.inventory.len() && to < u.inventory.len() {
                u.inventory.swap(from, to);
            }
        }
    }

    /// Whether an item may leave its carrier's hands (SetItemDroppable, the item's
    /// droppable); if not, "Unable to drop this item." (error 0xd3, FUN_6f545dd0).
    fn droppable(&mut self, item: u32) -> bool {
        let ok = self.world.item(item).is_none_or(|i| i.droppable);
        if !ok {
            self.world.order_error = Some("Cantdropitem");
        }
        ok
    }

    /// Orders a unit to drop the item in a slot at a point.
    pub fn drop_item_at(&mut self, unit: u32, slot: usize, x: f32, y: f32) {
        let Some(item) = self.world.unit(unit).and_then(|u| u.inventory.get(slot).copied()).filter(|&i| i != 0) else { return };
        if !self.droppable(item) {
            return;
        }
        if let Some(u) = self.world.unit_mut(unit) {
            u.action = types::Action::DropItem { item, x, y };
            u.path.clear();
        }
    }

    /// Orders a unit to hand the item in a slot to another.
    pub fn give_item_to(&mut self, unit: u32, slot: usize, target: u32) {
        let Some(item) = self.world.unit(unit).and_then(|u| u.inventory.get(slot).copied()).filter(|&i| i != 0) else { return };
        if !self.droppable(item) {
            return;
        }
        if let Some(u) = self.world.unit_mut(unit) {
            u.action = types::Action::GiveItem { item, target };
            u.path.clear();
        }
    }

    /// The local player uses the item in a slot on a target unit or point.
    pub fn use_item_at(&mut self, unit: u32, slot: usize, target: u32, x: f32, y: f32) -> bool {
        self.world.use_item(&mut self.vm, unit, slot, target, x, y)
    }

    /// Kills a unit (for tools and tests).
    pub fn kill(&mut self, h: u32) {
        natives::units::kill_unit(&mut self.world, &mut self.vm, h, 0);
    }

    /// Real time passes in a game played alone that PauseGame paused: sleeping threads
    /// go on.
    pub fn tick_paused(&mut self, dt: f32) {
        self.world.wake_sleepers(&mut self.vm, dt);
        self.send_net_out();
    }

    /// What the game sends for its player as the script runs (`World::net_out`: a
    /// trigger's resumption, a TriggerSyncReady): to the host in a network game; dropped
    /// where frames come from elsewhere (a replay, the tags example), which carry their
    /// own.
    fn send_net_out(&mut self) {
        let out = std::mem::take(&mut self.world.net_out);
        if let Some(o) = self.outbox.as_mut() {
            o.extend(out);
        }
    }

    /// Advances the simulation: a turn of `dt` seconds (`advance`).
    pub fn tick(&mut self, dt: f32) {
        if self.recorder.is_some() {
            let checksum = self.turn_keepalive(dt);
            if let Some(r) = self.recorder.as_mut() {
                r.step(dt, None, checksum);
            }
        }
        self.advance(dt);
        self.send_net_out();
    }

    /// The keepalive a turn of `dt` seconds makes: none for a turn of no time, which
    /// keeps the last (`FUN_6f32e3f0` takes a turn of 0 ms as one not to answer: no
    /// keepalive made or sent; read: the original's -loadfile Mafarazzo TD replay, its
    /// checksum after a turn of 0 ms carrying a 0x62 the one before it).
    fn turn_keepalive(&mut self, dt: f32) -> u32 {
        if (dt * 1000.0).round() as u32 > 0 {
            self.last_keepalive = self.world.keepalive();
        }
        self.last_keepalive
    }

    /// A turn's time passes as the original's turn loop (`FUN_6f32d670`) runs it: in
    /// ticks of 25 ms of turn time (the net session's +0x284 less 0x19 each), the rest
    /// kept for the next turn's (`world::World::turn_left`), each tick moving the game
    /// three, four, or five steps of 5 ms by the speed (`DAT_6fce54dc`; the game's
    /// +0x1c70 five more a step).  The next turn's message is taken only once less than
    /// a tick's time is left, so a turn's actions come before the ticks its time makes.
    /// (Read: the original's LAN host's turns of 100 and 99 ms left the keepalive's game
    /// time at 175, and the port's of 113, 203, and 314 ms in all at 100, 200, and 300.)
    /// While a network game's save is under way (`world::SaveWait`), only the save's
    /// own turn's first tick moves the game (state 5's, then 6), and the rest pass; a
    /// paused game's likewise (`World::pause_first`; read: a game played alone paused by
    /// its menu in a turn of 0 ms, the keepalive two turns on 25 ms past it).
    fn advance(&mut self, dt: f32) {
        let left = self.world.turn_left + (dt * 1000.0).round() as u32;
        let mut ticks = left / TICK_MS;
        self.world.turn_left = left % TICK_MS;
        if let Some(w) = self.world.net_save.as_mut() {
            if ticks == 0 || !std::mem::take(&mut w.first) {
                return;
            }
            ticks = 1;
        }
        if self.world.net_paused.is_some() {
            if ticks == 0 || !std::mem::take(&mut self.world.pause_first) {
                return;
            }
            ticks = 1;
        }
        if ticks == 0 {
            return;
        }
        let steps = [3, 4, 5][self.world.game_speed.clamp(0, 2) as usize];
        self.world.tick(&mut self.vm, (ticks * steps * 5) as f32 / 1000.0);
    }

    /// Plays one frame (of a replay or from a network game's host): its commands, then
    /// its step; returns what each command answered, with its player.  A network game's
    /// turn has its sleeps end by the network's word (`World::net_sleeps`).
    pub fn play_frame(&mut self, f: &replay::Frame) -> Vec<(u8, replay::Outcome)> {
        if f.turn.is_some() {
            self.world.net_turns = true;
        }
        self.play_leaves(f);
        let out = self.play_commands(f);
        self.save_turn(f.turn.as_ref().map_or((f.dt * 1000.0).round() as u16, |t| t.0));
        self.after_keepalive();
        self.play_step(f);
        out
    }

    /// A network turn as the original plays it: the leaves before it and its message
    /// hashed into the keepalive's net module, the leaves taken and its actions carried
    /// out, the keepalive made (returned), a save it called for made, then its step.
    pub fn play_turn(&mut self, f: &replay::Frame) -> (Vec<(u8, replay::Outcome)>, u32) {
        for l in &f.leaves {
            self.world.net_leave(l.pid, l.word);
        }
        self.play_leaves(f);
        if let Some((ms, blocks)) = &f.turn {
            if blocks.is_empty() {
                self.world.net_turn(*ms);
            } else {
                self.world.net_turn_actions(*ms, blocks);
            }
        }
        let out = self.play_commands(f);
        self.save_turn(f.turn.as_ref().map_or((f.dt * 1000.0).round() as u16, |t| t.0));
        let keepalive = self.turn_keepalive(f.dt);
        log::debug!(target: "keepalive", "{keepalive:08x}: {}", self.world.keepalive_inputs());
        self.after_keepalive();
        self.play_step(f);
        (out, keepalive)
    }

    /// A network game's people who left before a frame's turn, as the host's
    /// PLAYERLEFT said (`replay::Frame::leaves`), each taken as the original's net game
    /// takes its leave message (`FUN_6f32daf0`).
    fn play_leaves(&mut self, f: &replay::Frame) {
        for l in &f.leaves {
            self.player_left(l.player);
        }
    }

    /// A network game's player is gone: a save under way counts their part done
    /// (`FUN_6f32daf0` calls `FUN_6f32c7b0` with 0), their slot is left, and
    /// EVENT_PLAYER_LEAVE fires.
    fn player_left(&mut self, player: u8) {
        self.save_done(player, 0);
        if let Some(p) = self.world.players.get_mut(player as usize) {
            p.slot_state = types::slot_state::LEFT;
        }
        self.player_event(player, event_ids::EVENT_PLAYER_LEAVE);
    }

    /// A turn's time counts against a network game's save under way, after its actions
    /// and before its keepalive (`FUN_6f32e3f0`: +0x2e0 less the turn's milliseconds):
    /// at 8000 ms of turns the save is over, whoever has not said theirs is done.
    pub fn save_turn(&mut self, ms: u16) {
        let Some(w) = self.world.net_save.as_mut() else { return };
        if ms == 0 {
            return;
        }
        w.budget = w.budget.saturating_sub(ms as u32);
        if w.budget == 0 {
            self.world.net_save = None;
        }
    }

    /// What a turn's message does after its keepalive: the save its actions called for
    /// (`FUN_6f327040`, the command kept at +0x1b48 until the message is handled).  The
    /// net module is set back (`FUN_6f32f930`: CSyncData's sum and count, 0); then
    /// (`FUN_6f26f450`), while no save is under way and the player has made none
    /// (`FUN_6f32b800`, over LAN: its +0x3c), the save begins (`world::SaveWait`), the
    /// triggers hear it (EVENT_GAME_SAVE, its data a `` `gsed ``), this computer's copy
    /// is made (`saves`, in a game being recorded, whose record describes it), the
    /// player's saves count one more, and this computer says its save is done (0x07
    /// with 1 for a copy made, `FUN_6f26c130`).
    pub fn after_keepalive(&mut self) {
        let Some((player, name)) = self.save_due.take() else { return };
        self.world.net_sum = 0;
        if self.world.net_save.is_some() || self.world.saves_made[player as usize % 16] > 0 {
            return;
        }
        self.world.net_save = Some(world::SaveWait { budget: 8000, first: true, done: Vec::new() });
        // (The save's event data, held by the triggers' event until its timer lets it go
        // in the next tick: in the save, and born before the next keepalive; read, the
        // Mafarazzo TD save's `gsed` and its keepalives.)
        let data = self.world.tags.take_id(codes::class::SAVE_EVENT_DATA);
        self.world.save_event = Some(data);
        self.game_event(event_ids::EVENT_GAME_SAVE);
        let mut result = 0;
        if self.recorder.is_some() {
            let file = self.save_files().map_err(|e| format!("{e:#}"));
            result = file.is_ok() as u32;
            self.saves.push(NetSaveMade { name, file });
        }
        self.world.save_event = None;
        self.world.tags.release(data.0);
        self.world.saves_made[player as usize % 16] += 1;
        let local = self.world.local_player;
        if let Some(o) = self.outbox.as_mut() {
            o.push(replay::Command::SaveDone { player: local, result });
        }
    }

    /// A player's computer has said its save is done (`FUN_6f32c7b0`, nothing while no
    /// save is under way): once every person in the game has, the save is over
    /// (`FUN_6f328bd0`).
    fn save_done(&mut self, player: u8, result: u32) {
        let w = &self.world;
        let people: Vec<u8> = w.players.iter().take(16).filter(|p| p.controller == types::map_control::USER && p.slot_state == types::slot_state::PLAYING).map(|p| p.id).collect();
        let Some(s) = self.world.net_save.as_mut() else { return };
        if !s.done.iter().any(|d| d.0 == player) {
            s.done.push((player, result));
        }
        if people.iter().all(|p| s.done.iter().any(|d| d.0 == *p)) {
            self.world.net_save = None;
        }
    }

    /// A frame's step: its time passes.
    pub fn play_step(&mut self, f: &replay::Frame) {
        if self.recorder.is_some() {
            let checksum = self.turn_keepalive(f.dt);
            if let Some(r) = self.recorder.as_mut() {
                r.step(f.dt, Some(f), checksum);
            }
        }
        // A paused game takes commands (a resume among them), its turns' time passing
        // without moving it (`advance`).
        self.advance(f.dt);
        self.send_net_out();
    }

    /// A frame's commands without its step: the original carries out a turn's actions
    /// as its message comes in, before the turn's keepalive and its time (read: a
    /// selection is in the game module of the keepalive its turn's message makes).
    pub fn play_commands(&mut self, f: &replay::Frame) -> Vec<(u8, replay::Outcome)> {
        // A frame read from a file has the original's actions, read as they come.
        let mut commands = f.commands.clone();
        if let Some(actions) = f.actions.as_ref().filter(|_| commands.is_empty()) {
            for (p, bytes) in actions {
                match self.codec.decode(&self.world, *p, bytes) {
                    Ok(cs) => commands.extend(cs.into_iter().map(|c| (*p, c))),
                    Err(e) => log::warn!("player {p}'s actions: {e}"),
                }
            }
        }
        let out = commands
            .into_iter()
            .map(|(p, c)| {
                // While a network game's save is under way, pauses, orders, and cancels are
                // not carried out (`world::SaveWait`).
                if self.world.net_save.is_some() && actions::ignored_while_saving(&c) {
                    return (p, replay::Outcome::Done);
                }
                log::debug!("player {p}: {c:?}");
                let o = self.execute(p, c);
                log::debug!("  answered {o:?}");
                (p, o)
            })
            .collect();
        self.send_net_out();
        out
    }

    /// A command the interface carries out by itself, noted for the replay only: a game
    /// played alone's game menu pausing it and letting it go on, as the original sends
    /// Pause and Resume then (read: its replays' 0x01 as the menu opens and 0x02 as it
    /// closes), where the port's menu stops the game by itself.
    pub fn note(&mut self, c: replay::Command) {
        let local = self.world.local_player;
        if let Some(r) = self.recorder.as_mut().filter(|_| self.outbox.is_none() && !self.locked) {
            r.command(&self.world, local, c);
        }
    }

    /// The replay's last frame: the commands noted since the last step, as a turn of no
    /// time (the menu's pause as the game is quit through it).
    pub fn close_record(&mut self) {
        if self.recorder.as_ref().is_some_and(|r| r.pending()) {
            let checksum = self.turn_keepalive(0.0);
            if let Some(r) = self.recorder.as_mut() {
                r.step(0.0, None, checksum);
            }
        }
    }

    /// A command from the interface (noted for the replay); none while a replay plays,
    /// and in a network game sent to the host rather than carried out.
    pub fn apply(&mut self, c: replay::Command) -> replay::Outcome {
        if self.locked {
            return replay::Outcome::Done;
        }
        let local = self.world.local_player;
        if let Some(out) = self.outbox.as_mut() {
            if let replay::Command::Select { units, .. } = &c {
                if self.sent_selection.as_ref() == Some(units) || (self.sent_selection.is_none() && self.world.selection.get(&local).is_some_and(|s| s == units)) {
                    return replay::Outcome::Done;
                }
                self.sent_selection = Some(units.clone());
            }
            out.push(c);
            return replay::Outcome::Done;
        }
        // A cheat typed in a game played alone goes as the original's cheat action, no
        // chat line (read: its replays' 0x22 and 0x32, no 0x20).
        let c = match c {
            replay::Command::Chat { player, ref text, .. } if !self.world.network => match self.world.cheat_action(text) {
                Some((id, data)) => replay::Command::Cheat { player, id, data },
                None => c,
            },
            c => c,
        };
        // A selection's change goes as the original's actions in a game played alone too,
        // which keep the synchronized selection (`selection`) as they do in a network
        // game's turns and in this game's replay.
        if let replay::Command::Select { .. } = &c {
            let bytes = self.local_codec.0.encode(&self.world, local, &c);
            for c in self.local_codec.1.decode(&self.world, local, &bytes).unwrap_or_default() {
                if let replay::Command::SyncSelection { op, .. } = c {
                    self.world.sync_selection(local, op);
                }
            }
        }
        self.execute(local, c)
    }

    /// Units sent together to one point take places in a formation there.
    fn formation(&mut self, player: u8, units: Vec<u32>) {
        let mine: Vec<u32> = units.into_iter().filter(|&u| natives::players::controls(&self.world, player, u)).collect();
        let goals: Vec<(f32, f32)> = mine.iter().filter_map(|&u| self.world.unit(u)).filter_map(|u| match u.action {
            types::Action::Move { x, y } | types::Action::AttackMove { x, y } => Some((x, y)),
            _ => None,
        }).collect();
        if goals.len() == mine.len() && goals.windows(2).all(|w| (w[0].0 - w[1].0).hypot(w[0].1 - w[1].1) < 1.0) {
            if let Some(&g) = goals.first() {
                self.world.take_formation_places(&mine, g);
            }
        }
        self.world.form_group(&mine);
    }

    fn execute(&mut self, player: u8, c: replay::Command) -> replay::Outcome {
        use replay::{Command as C, Outcome as O};
        // A command speaks only for the player who gave it, and orders only the units
        // they command.
        if c.player().is_some_and(|p| p != player) {
            return O::Done;
        }
        if c.unit().is_some_and(|u| !natives::players::controls(&self.world, player, u)) {
            return O::Done;
        }
        // A selection the player already has changes nothing.
        if let C::Select { player, units } = &c {
            if self.world.selection.get(player).is_some_and(|s| s == units) {
                return O::Done;
            }
        }
        if let Some(r) = self.recorder.as_mut() {
            r.command(&self.world, player, c.clone());
        }
        let res = |r: Result<(), &'static str>| r.map_or_else(O::Error, |_| O::Done);
        match c {
            C::Order { unit, order, x, y, target } => {
                self.world.leave_formation(unit);
                self.world.order_unit(unit, order, x, y, target);
                O::Done
            }
            C::Issue { unit, order, x, y, target, queue } => {
                self.world.leave_formation(unit);
                self.world.issue_order(unit, order, x, y, target, queue);
                O::Done
            }
            C::SelectionOrder { order, at, target, queue, formation, .. } => {
                let units: Vec<u32> = self.world.selection.get(&player).cloned().unwrap_or_default().into_iter().filter(|&u| natives::players::controls(&self.world, player, u)).collect();
                let smart = natives::basic::order_id("smart");
                let (x, y) = at.unwrap_or((0.0, 0.0));
                let (mut ordered, mut error, mut moved) = (false, None, Vec::new());
                // An order to more than one unit has two cluster groups of its own while
                // its units take it (read: five peons sent to a gold mine, two `^clg`
                // made before their five orders and freed after them).
                let groups = (units.len() > 1).then(|| [self.world.tags.take(codes::class::CLUSTER_GOAL), self.world.tags.take(codes::class::CLUSTER_GOAL)]);
                // Sent together to a point on foot they walk as one cluster
                // (`crate::groupwalk`).
                let move_order = order == smart || order == natives::basic::order_id("move");
                let group_walk = units.len() > 1 && target == 0 && move_order && self.world.pathing.is_some() && units.iter().all(|&u| self.world.move_kind(u) == pathing::Move::Foot && self.world.unit(u).is_some_and(|v| !v.extra.local_paths));
                if group_walk {
                    self.world.group_walk_members = Some(Vec::new());
                }
                // Its units take it nearest the target first (read: five peons sent to a
                // gold mine took their orders by their distance to it, 8.0 to 12.0 cells).
                let mut units = units;
                if units.len() > 1 {
                    let to = if target != 0 { self.world.widget_pos(target) } else { at };
                    if let Some((tx, ty)) = to {
                        let d2 = |w: &World, u: u32| w.unit(u).map_or(f32::MAX, |u| (u.x - tx) * (u.x - tx) + (u.y - ty) * (u.y - ty));
                        units.sort_by(|&a, &b| d2(&self.world, a).total_cmp(&d2(&self.world, b)));
                    }
                }
                for u in units {
                    // A unit right-clicked does not take its own click, but goes to the
                    // ground there.
                    let target = if order == smart && target == u { 0 } else { target };
                    if order == smart && target == 0 && at.is_none() {
                        continue;
                    }
                    let (x, y) = if target != 0 { (0.0, 0.0) } else { (x, y) };
                    self.world.order_error = None;
                    self.world.leave_formation(u);
                    self.world.ordering_player = true;
                    self.world.issue_order(u, order, x, y, target, queue);
                    self.world.ordering_player = false;
                    match self.world.order_error.take() {
                        Some(e) => error = Some(e),
                        None => ordered = true,
                    }
                    if at.is_some() && target == 0 {
                        moved.push(u);
                    }
                }
                let walked = match self.world.group_walk_members.take() {
                    Some(members) if !members.is_empty() => {
                        self.world.start_group_walk(members, (x, y));
                        true
                    }
                    _ => false,
                };
                for g in groups.into_iter().flatten() {
                    self.world.tags.release(g);
                }
                if formation && moved.len() > 1 && !walked {
                    self.formation(player, moved);
                }
                match (ordered, error) {
                    (false, Some(e)) => O::Error(e),
                    _ => O::Done,
                }
            }
            C::Formation { units } => {
                self.formation(player, units);
                O::Done
            }
            C::Build { worker, id, x, y } => {
                // A place the structure cannot take refuses the order at once, nothing
                // paid (traced over LAN: Mafarazzo TD's peasant sent to build an Arrow
                // Tower beside the road, its order and block made and freed, no walk;
                // the port had taken the gold and refused it as the peasant arrived).
                let r = if self.world.can_place(id, x, y) { self.world.order_build(worker, id, x, y) } else { Err("Cantplace") };
                if r.is_ok() {
                    self.world.player_build_objects(worker);
                } else if self.world.unit(worker).is_some() {
                    // One refused makes its order and a block, both freed in the next
                    // tick, the block first (traced over LAN: Mafarazzo TD's peasant
                    // sent to build the HSBC without its Castle and St George Bank;
                    // provisional: for every refusal), by the main clock as the order
                    // was carried out, after the timers due as that tick begins, as an
                    // accepted one's (traced: the peasant sent to build beside the road
                    // at turn 21, the leaderboard's trigger at 2.0 making its objects
                    // first).
                    let order = self.world.tags.take(codes::class::ORDER_POINT);
                    let block = self.world.tags.take(codes::class::TASK_A);
                    let at = self.world.actions_clock();
                    self.world.tags.doom_at(at, block);
                    self.world.tags.doom_at(at, order);
                }
                res(r)
            }
            C::Train { building, id } => res(self.world.train(building, id)),
            C::Research { building, id } => res(self.world.research_at(building, id)),
            C::Upgrade { building, id } => res(self.world.upgrade_structure_by(building, id, true)),
            C::CancelLast { building } => {
                self.world.cancel_last(building);
                O::Done
            }
            // A cheat's action, outside network games; the defeat and victory cheats fire
            // the player's event.
            C::Cheat { id, data, .. } => {
                if !self.world.network {
                    if let Some(ctx) = self.world.apply_cheat(player, id, &data).filter(|c| c.event_id != 0) {
                        self.player_event(player, ctx.event_id);
                    }
                }
                O::Done
            }
            // The game's speed, not while MAP_LOCK_SPEED is set (as SetGameSpeed).
            C::SetSpeed { speed, .. } => {
                if self.world.map_flags & 0x4000 == 0 {
                    self.world.game_speed = (speed as i32).clamp(0, 2);
                }
                O::Done
            }
            // Three pauses a player; an observer's are not counted, and only an observer
            // lifts one (FUN_6f3719c0, FUN_6f32b7d0).
            C::Pause { player: p, on } => {
                let observer = |w: &world::World, p: u8| w.players.get(p as usize).is_some_and(|pl| pl.get_state(types::player_state::OBSERVER) != 0);
                let w = &mut self.world;
                if p != player {
                    return O::Done;
                }
                // A game played alone's pause is its game menu's, uncounted and unannounced.
                if !w.net_sleeps() {
                    w.pause_first = on && (w.pause_first || w.net_paused.is_none());
                    w.net_paused = on.then_some(p);
                    return O::Done;
                }
                match (on, w.net_paused) {
                    (true, None) => {
                        let free = observer(w, p);
                        if !free && w.pauses_used[p as usize % 16] >= 3 {
                            return O::Done;
                        }
                        if !free {
                            w.pauses_used[p as usize % 16] += 1;
                        }
                        w.net_paused = Some(p);
                        w.pause_first = true;
                        let left = (!free).then(|| 3 - w.pauses_used[p as usize % 16] as i32);
                        w.requests.push(world::Request::PauseNotice { player: p, on: true, left });
                    }
                    (false, Some(by)) => {
                        if observer(w, by) && !observer(w, p) {
                            return O::Done;
                        }
                        w.net_paused = None;
                        w.requests.push(world::Request::PauseNotice { player: p, on: false, left: None });
                    }
                    _ => {}
                }
                O::Done
            }
            C::CacheValue { cache, mission, key, value } => {
                use replay::CacheValue as V;
                let k = (mission, key);
                for (_, o) in self.world.handles.iter_mut() {
                    let types::Obj::GameCache(c) = o else { continue };
                    if !c.name.eq_ignore_ascii_case(&cache) {
                        continue;
                    }
                    fn set<T>(m: &mut collections::HashMap<(String, String), T>, k: &(String, String), v: Option<T>) {
                        match v {
                            Some(v) => _ = m.insert(k.clone(), v),
                            None => _ = m.remove(k),
                        }
                    }
                    match value.clone() {
                        V::Int(v) => set(&mut c.ints, &k, v),
                        V::Real(v) => set(&mut c.reals, &k, v),
                        V::Bool(v) => set(&mut c.bools, &k, v),
                        V::Str(v) => set(&mut c.strings, &k, v),
                        V::Unit(v) => set(&mut c.units, &k, v),
                    }
                }
                O::Done
            }
            // A trigger execution's wait over, by a player's word (`world::Execution`).
            C::ResumeTrigger { execution, count, .. } => {
                self.world.resume_execution(&mut self.vm, execution, count);
                O::Done
            }
            C::TriggerSyncReady { player, execution } => {
                self.world.sync_ready(&mut self.vm, player, execution);
                O::Done
            }
            // A network game's save, made once the turn's message is handled.
            C::Save { name } => {
                self.save_due = Some((player, name));
                O::Done
            }
            C::SaveDone { result, .. } => {
                self.save_done(player, result);
                O::Done
            }
            C::CancelQueue { building, slot } => {
                self.world.cancel_queue_at(building, slot);
                O::Done
            }
            C::LearnSkill { hero, id } => O::Flag(self.world.learn_skill(hero, id)),
            C::BuyItem { player, shop, id } => res(self.buy_item(player, shop, id)),
            C::HireUnit { player, shop, id } => res(self.hire_unit(player, shop, id)),
            C::Awaken { player, tavern, hero } => res(self.world.awaken(player, tavern, hero)),
            C::StartRevive { altar, hero } => res(self.start_revive(altar, hero)),
            C::UseItem { unit, slot } => O::Flag(self.use_item(unit, slot)),
            C::UseItemAt { unit, slot, target, x, y } => O::Flag(self.use_item_at(unit, slot, target, x, y)),
            C::MoveItem { unit, from, to } => {
                self.move_item(unit, from, to);
                O::Done
            }
            C::DropItemAt { unit, slot, x, y } => {
                self.drop_item_at(unit, slot, x, y);
                O::Done
            }
            C::GiveItemTo { unit, slot, target } => {
                self.give_item_to(unit, slot, target);
                O::Done
            }
            C::SyncSelection { player, op } => {
                self.world.sync_selection(player, op);
                O::Done
            }
            C::Select { player, units } => {
                self.set_selection(player, &units);
                O::Done
            }
            C::Trackable { player, trackable, hit } => {
                for e in self.world.trackable_hearers(trackable, hit) {
                    self.event_heard(player, e, "");
                }
                O::Done
            }
            C::EventHeard { player, event, text } => {
                self.event_heard(player, event, &text);
                O::Done
            }
            // A player opened the hero skill menu or the build menu: the game events
            // EVENT_GAME_SHOW_SKILL and EVENT_GAME_BUILD_SUBMENU.
            C::Submenu { build, .. } => {
                self.game_event(if build { event_ids::EVENT_GAME_BUILD_SUBMENU } else { event_ids::EVENT_GAME_SHOW_SKILL });
                O::Done
            }
            // A network game's player is gone, as the port's older replays say it (a
            // leave now comes as the host's PLAYERLEFT, `replay::Frame::leaves`).
            C::PlayerLeft { player } => {
                self.player_left(player);
                O::Done
            }
            C::Signal { player, x, y } => {
                self.world.requests.push(world::Request::AllySignal { player, x, y });
                O::Done
            }
            // An alliance change from the Allies dialog: the kinds set, and the player's
            // EVENT_PLAYER_ALLIANCE_CHANGED.
            C::Alliance { player, other, flags } => {
                if let Some(p) = self.world.players.get_mut(player as usize) {
                    if let Some(a) = p.alliances.get_mut(other as usize) {
                        for t in 0..=6 {
                            a[t] = flags & (1 << t) != 0;
                        }
                    }
                    // Bit 10 is the sender's own allied victory, sent with every row the
                    // dialog changes (measured: ticked, then unticked, in the original's
                    // Allies dialog; FUN_6f3ab370).
                    p.state.insert(types::player_state::ALLIED_VICTORY, (flags & 0x400 != 0) as i32);
                }
                self.player_event(player, event_ids::EVENT_PLAYER_ALLIANCE_CHANGED);
                O::Done
            }
            // Resources sent to another player, as much as the sender has.
            C::Trade { player, other, gold, lumber } => {
                use types::player_state::{RESOURCE_GOLD, RESOURCE_LUMBER};
                let n = self.world.players.len();
                if (player as usize) < n && (other as usize) < n && player != other {
                    for (state, amount) in [(RESOURCE_GOLD, gold), (RESOURCE_LUMBER, lumber)] {
                        let have = self.world.players[player as usize].get_state(state);
                        let v = amount.clamp(0, have.max(0));
                        *self.world.players[player as usize].state.entry(state).or_insert(0) -= v;
                        *self.world.players[other as usize].state.entry(state).or_insert(0) += v;
                    }
                }
                O::Done
            }
            C::PlayerEvent { player, event } => {
                self.player_event(player, event);
                O::Done
            }
            C::DialogButton { player, button } => {
                self.click_dialog_button(player, button);
                O::Done
            }
            C::Chat { player, text, to } => O::Flag(self.chat(player, &text, to)),
        }
    }
}

#[cfg(test)]
mod turn_tests {
    use super::*;

    fn game() -> Game {
        let chain = Arc::new(Chain::new());
        let world = World::new(chain, ObjectDb::default());
        let module = w3_jass::compile([("test.j", "function main takes nothing returns nothing\nendfunction\n")]).unwrap();
        Game {
            vm: Vm::new(module), world, info: MapInfo::default(), recorder: None,
            locked: false, outbox: None, sent_selection: None, codec: Default::default(),
            local_codec: Default::default(), save_due: None, saves: Vec::new(), last_keepalive: 0,
        }
    }

    #[test]
    fn zero_time_turns_keep_the_preceding_checksum_including_the_recorded_close() {
        let mut g = game();
        g.recorder = Some(replay::Recorder::default());
        let (_, previous) = g.play_turn(&replay::Frame { dt: 0.025, ..Default::default() });
        let at = g.world.game_ms;
        let (_, zero) = g.play_turn(&replay::Frame { dt: 0.0, turn: Some((0, vec![])), ..Default::default() });
        assert_eq!(zero, previous);
        assert_eq!(g.world.game_ms, at);
        assert_ne!(g.world.keepalive(), previous, "the live state changed after the preceding turn");
        g.note(replay::Command::Pause { player: 0, on: true });
        g.close_record();
        let frames = &g.recorder.as_ref().unwrap().replay.frames;
        assert_eq!(frames.len(), 3);
        assert!(frames.iter().all(|f| f.checksum == Some(previous)));
        assert_eq!(frames.last().unwrap().dt, 0.0);
        g.seed_generators();
        assert_eq!(g.turn_keepalive(0.0), 0, "a new game cannot reuse the previous game's checksum");
    }

    #[test]
    fn pause_in_a_zero_time_turn_runs_one_tick_then_holds_until_resumed() {
        let mut g = game();
        let frame = |dt, commands| replay::Frame { dt, commands, ..Default::default() };
        g.play_frame(&frame(0.0, vec![(0, replay::Command::Pause { player: 0, on: true })]));
        assert_eq!(g.world.game_ms, 0);
        g.play_frame(&frame(0.0, vec![(0, replay::Command::Pause { player: 0, on: true })]));
        g.play_frame(&frame(0.110, vec![]));
        let tick = [15, 20, 25][g.world.game_speed.clamp(0, 2) as usize];
        assert_eq!(g.world.game_ms, tick);
        g.play_frame(&frame(0.110, vec![]));
        assert_eq!(g.world.game_ms, tick);
        g.play_frame(&frame(0.005, vec![(0, replay::Command::Pause { player: 0, on: false })]));
        assert_eq!(g.world.game_ms, 2 * tick, "paused turn remainders still carry into the resumed turn");
    }

    #[test]
    fn fixed_map_start_only_draws_for_random_race_preferences() {
        for random in [false, true] {
            let mut g = game();
            g.info.flags = 0x20;
            g.world.fixed_starts = true;
            g.world.placement = 2;
            for p in g.world.players.iter_mut().take(12) {
                p.controller = map_control::USER;
                p.start_forced = true;
                p.race_preference = 1;
            }
            if random { g.world.players[0].race_preference = 0x20; }
            let mut expected = rng::Rng::new(g.world.seed);
            if random { expected.next_u32(); }
            g.start(|id, _| if id == 0 { SlotFill::Local } else { SlotFill::Empty }).unwrap();
            assert_eq!(g.world.rng, expected);
        }
    }

    #[test]
    fn melee_direct_start_keeps_race_and_free_start_shuffle_draws() {
        let mut g = game();
        g.world.fixed_starts = true;
        g.world.placement = 0;
        g.world.start_locations = vec![Default::default(); 2];
        for (i, p) in g.world.players.iter_mut().enumerate().take(12) {
            p.controller = if i < 2 { map_control::USER } else { map_control::NONE };
            p.team = i as i32;
        }
        let mut expected = rng::Rng::new(g.world.seed);
        // All 12 race draws, and one shuffle draw for two free start locations.
        for _ in 0..13 { expected.next_u32(); }
        g.start(|id, _| if id == 0 { SlotFill::Local } else { SlotFill::Computer }).unwrap();
        assert_eq!(g.world.rng, expected);
    }
}
