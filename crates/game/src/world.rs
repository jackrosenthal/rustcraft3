//! Game state and the script host.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use w3_jass::{FuncId, Host, NativeResult, Outcome, Thread, Value, Vm};
use w3_mpq::Chain;

use crate::handles::Handles;
use crate::objects::ObjectDb;
use crate::types::*;

pub type NativeFn = fn(&mut World, &mut Vm, &[Value]) -> NativeResult;

/// Messages for the presentation layer (renderer, audio, interface).
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum Request {
    DisplayText { players: u32, x: f32, y: f32, text: String, duration: f32 },
    ClearText { players: u32 },
    PlaySound(u32),
    StopSound { sound: u32, fade: bool },
    PlayMusic(String),
    /// A sound label at a world position, heard where the local player can see.
    WorldSound { label: String, x: f32, y: f32, z: f32 },
    /// An interface sound label for some players.
    UiSound { players: u32, label: String },
    /// A unit's acknowledgment line (UnitAckSounds `<unitSound><kind>`).
    Ack { unit: u32, kind: &'static str },
    StopMusic(bool),
    SetCameraPosition { players: u32, x: f32, y: f32 },
    PanCamera { players: u32, x: f32, y: f32, z: Option<f32>, duration: f32 },
    SetCameraField { players: u32, field: i32, value: f32, duration: f32 },
    ResetCamera { players: u32, duration: f32 },
    CinematicMode(bool),
    /// ChangeLevel: the next map, and whether the score screen comes first.
    ChangeLevel { map: String, score: bool },
    Fade { players: u32, out: bool, duration: f32 },
    EndGame,
    /// RestartGame: the map starts over.
    Restart,
    /// SaveGame and LoadGame: a saved game's name in the saved games folder.
    SaveGame(String),
    LoadGame(String),
    /// UnitAddIndicator and AddIndicator: a widget's indicator in a color.
    Indicator { widget: u32, color: [u8; 4] },
    /// PlayCinematic: a movie.
    PlayMovie(String),
    /// ForceUIKey: the interface acts as if a hotkey were pressed; ForceUICancel as if
    /// the cancel key were.
    ForceUIKey(String),
    ForceUICancel,
    DisplayLoadDialog,
}

/// A Sentinel owl perched on a tree.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Sentinel {
    pub caster: u32,
    pub owner: u8,
    pub tree: u32,
    pub radius: f32,
    pub effect: u32,
}

/// A suspended trigger thread and the event context it resumes with.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct Sleeper {
    pub wake_at: f32,
    pub thread: Thread,
    pub ctx: EventCtx,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct World {
    #[serde(skip)]
    pub chain: Arc<Chain>,
    #[serde(skip)]
    pub objects: ObjectDb,
    pub handles: Handles<Obj>,
    pub players: Vec<Player>,
    pub start_locations: Vec<StartLocation>,
    pub events: Vec<u32>,
    #[serde(skip)]
    pub natives: Vec<Option<NativeFn>>,
    #[serde(skip)]
    pub native_names: Vec<String>,
    warned: HashSet<u32>,
    /// Game time in seconds.
    pub time: f32,
    pub ctx: EventCtx,
    pub enum_ctx: EnumCtx,
    pub sleepers: Vec<Sleeper>,
    /// Threads started by natives (TriggerExecute, ExecuteFunc) that must run once the
    /// current native returns.
    pub pending: Vec<(Thread, EventCtx)>,
    #[serde(skip)]
    pub requests: Vec<Request>,
    pub map_name: String,
    pub map_description: String,
    pub player_count: i32,
    pub team_count: i32,
    pub placement: i32,
    pub game_type_selected: i32,
    pub game_types_supported: i32,
    pub map_flags: i32,
    pub game_speed: i32,
    pub difficulty: i32,
    pub camera_bounds: [f32; 8],
    pub camera_margins: [f32; 4],
    pub map_bounds: Rect,
    pub playable: Rect,
    pub time_of_day: f32,
    pub time_of_day_scale: f32,
    pub day_night_models: (String, String),
    pub fog_enabled: bool,
    pub fog_mask_enabled: bool,
    #[serde(skip)]
    pub terrain_heights: Option<TerrainHeights>,
    pub global_variables_changed: HashMap<String, f32>,
    /// Units\\MiscData.txt merged with Units\\MiscGame.txt: gameplay constants.
    #[serde(skip)]
    pub misc: w3_formats::profile::Profile,
    pub rng: crate::rng::Rng,
    /// `TRIGSTR_n` strings from war3map.wts.
    #[serde(skip)]
    pub trigger_strings: HashMap<u32, String>,
    /// Localized interface strings (FDF StringList entries).
    #[serde(skip)]
    pub strings: HashMap<String, String>,
    pub local_player: u8,
    /// VERSION_REIGN_OF_CHAOS (0) or VERSION_FROZEN_THRONE (1).
    pub version: i32,
    /// Live units in creation order, the order enumerations visit them.
    pub unit_order: Vec<u32>,
    pub selection: HashMap<u8, Vec<u32>>,
    /// Counts script changes to the selection, so the interface can follow them.
    pub selection_serial: u32,
    /// The leaderboard each player is shown (PlayerSetLeaderboard).
    pub player_leaderboards: HashMap<u8, u32>,
    /// MultiboardSuppressDisplay.
    pub multiboard_suppressed: bool,
    /// Map tileset letter (war3map.w3e).
    pub tileset: u8,
    /// CripplePlayer: players who see each player's units.
    pub crippled: HashMap<u8, u16>,
    /// RemovePlayer: each removed player's PLAYER_GAME_RESULT.
    pub player_results: HashMap<u8, i32>,
    pub restart_disabled: bool,
    /// The saved game this game was loaded from (GetSaveBasicFilename).
    pub load_game_name: String,
    pub reserved_hero_buttons: i32,
    pub alt_minimap_icon: Option<String>,
    /// EnableWorldFogBoundary.
    pub fog_boundary: bool,
    /// PlayModelCinematic.
    pub model_cinematic: Option<String>,
    /// Running terrain deformations: handle, start time, and parameters.
    pub deformations: Vec<(u32, f32, crate::natives::visuals::Deform)>,
    /// Entangle Gold Mine casts finishing: time, caster, and mine.
    pub pending_entangles: Vec<(f32, u32, u32)>,
    /// Blighted terrain corners, and a count that changes when they do.
    pub blight: Vec<bool>,
    pub blight_serial: u32,
    pub autocast_timer: f32,
    pub sentinels: Vec<Sentinel>,
    /// Why the last order was refused (a command error key), for the interface.
    #[serde(skip)]
    pub order_error: Option<&'static str>,
    /// Where saved games are kept (set by the program running the game).
    #[serde(skip)]
    pub saves_dir: Option<std::path::PathBuf>,
    /// Splats\\LightningData.slk, read when first needed.
    #[serde(skip)]
    pub lightning_data: Option<w3_formats::slk::Table>,
    /// W3PROF timings by subsystem.
    #[serde(skip)]
    pub profile: Option<HashMap<&'static str, f64>>,
    pub destructable_order: Vec<u32>,
    pub item_order: Vec<u32>,
    /// Effects destroyed by script, with the time they were destroyed; the presentation
    /// layer plays their death animation before the handle is freed.
    pub dying_effects: Vec<(u32, f32)>,
    pub pathing: Option<crate::pathing::PathingMap>,
    pub missiles: Vec<crate::sim::Missile>,
    #[serde(skip)]
    pathing_textures: HashMap<String, Option<w3_formats::blp::Image>>,
    pub ai: HashMap<u8, crate::ai::AiState>,
    /// Saved games keep their variables and threads apart (see save.rs).
    #[serde(skip)]
    pub ai_vms: HashMap<u8, crate::ai::AiVm>,
    /// The computer player whose AI script is running, while one is.
    pub ai_current: Option<u8>,
    /// AI_DIFFICULTY_* per computer player.
    pub ai_difficulty: HashMap<u8, i32>,
    pub fog: Option<crate::fog::FogMap>,
    pub shops: crate::shops::Shops,
    pub upgrade_cache: crate::upgrades::UpgradeCache,
    /// Spell lightning between units, and jumping spells' next strikes.
    pub bolts: Vec<crate::bolts::Bolt>,
    pub jumps: Vec<crate::bolts::Jump>,
    /// Tech requirements met, by player, requirement, and amount; cleared each tick.
    #[serde(skip)]
    pub tech_cache: std::cell::RefCell<HashMap<(u8, crate::objects::Id, i32), bool>>,
    pub ground_effects: Vec<GroundEffect>,
    /// Quests and defeat conditions in creation order, and whether the quest button
    /// should flash (FlashQuestDialogButton).
    pub quests: Vec<u32>,
    pub defeat_conditions: Vec<u32>,
    pub quest_flash: bool,
    /// The local player's camera.
    pub camera: crate::camera::GameCamera,
    pub cinematic: Cinematic,
    /// Terrain fog: style, start, end, density, and color (0 to 1), from the map (w3i
    /// flag 0x2000) or SetTerrainFogEx; ResetTerrainFog returns to the map's.
    pub terrain_fog: Option<(i32, f32, f32, f32, [f32; 3])>,
    pub map_terrain_fog: Option<(i32, f32, f32, f32, [f32; 3])>,
    /// SetSkyModel's model, if any.
    pub sky_model: Option<String>,
    /// Unit events raised where no VM is at hand, fired at the end of the tick: the
    /// unit, its player and unit event ids, and the context.
    pub queued_events: Vec<(u32, i32, i32, EventCtx)>,
    /// Structures whose construction was cancelled, removed once their event fires.
    pub cancelled_constructions: Vec<u32>,
    /// The player's profile: campaign progress and saved game caches.
    pub persist: crate::persist::Persist,
    /// Handles removed at a game time (temporary fog modifiers).
    pub timed_removals: Vec<(u32, f32)>,
}

/// Ground heights for GetLocationZ and unit placement.
pub struct TerrainHeights {
    pub width: u32,
    pub height: u32,
    pub offset: [f32; 2],
    pub heights: Vec<f32>,
    /// Cliff level of each corner.
    pub layers: Vec<u8>,
}

impl TerrainHeights {
    /// The lowest cliff level among the corners of the tile containing (x, y).
    pub fn level_at(&self, x: f32, y: f32) -> u8 {
        let fx = ((x - self.offset[0]) / 128.0).clamp(0.0, (self.width - 1) as f32);
        let fy = ((y - self.offset[1]) / 128.0).clamp(0.0, (self.height - 1) as f32);
        let (ix, iy) = (fx.floor() as u32, fy.floor() as u32);
        let at = |x: u32, y: u32| self.layers.get((y.min(self.height - 1) * self.width + x.min(self.width - 1)) as usize).copied().unwrap_or(0);
        at(ix, iy).min(at(ix + 1, iy)).min(at(ix, iy + 1)).min(at(ix + 1, iy + 1))
    }

    pub fn at(&self, x: f32, y: f32) -> f32 {
        let fx = ((x - self.offset[0]) / 128.0).clamp(0.0, (self.width - 1) as f32);
        let fy = ((y - self.offset[1]) / 128.0).clamp(0.0, (self.height - 1) as f32);
        let (ix, iy) = (fx.floor() as u32, fy.floor() as u32);
        let (tx, ty) = (fx - ix as f32, fy - iy as f32);
        let at = |x: u32, y: u32| self.heights[(y.min(self.height - 1) * self.width + x.min(self.width - 1)) as usize];
        let a = at(ix, iy) * (1.0 - tx) + at(ix + 1, iy) * tx;
        let b = at(ix, iy + 1) * (1.0 - tx) + at(ix + 1, iy + 1) * tx;
        a * (1.0 - ty) + b * ty
    }
}

impl World {
    /// The race name used in sound labels (JobDoneSoundHuman).
    pub fn race_label(&self, player: u8) -> &'static str {
        match self.players.get(player as usize).map_or(1, |p| p.race) {
            2 => "Orc",
            3 => "Undead",
            4 => "NightElf",
            _ => "Human",
        }
    }

    pub fn new(chain: Arc<Chain>, objects: ObjectDb) -> World {
        let camera = crate::camera::GameCamera::new(&w3_formats::profile::Profile::parse(&chain.read_string("UI\\MiscData.txt").unwrap_or_default()));
        let mut misc = w3_formats::profile::Profile::default();
        for f in ["UI\\MiscData.txt", "UI\\MiscUI.txt", "Units\\MiscData.txt", "Units\\MiscGame.txt"] {
            if let Ok(s) = chain.read_string(f) {
                misc.merge(&s);
            }
        }
        let mut handles = Handles::default();
        let mut players = Vec::new();
        // The sixteen player handles are created first, so Player(0) is the first handle.
        for i in 0..PLAYER_COUNT as u8 {
            let h = handles.insert(Obj::Player(i));
            players.push(Player::new(i, h));
        }
        for (i, p) in players.iter_mut().enumerate() {
            for j in 0..PLAYER_COUNT {
                if i == j {
                    p.alliances[j] = [true; ALLIANCE_TYPES];
                }
            }
        }
        World {
            chain,
            objects,
            handles,
            players,
            start_locations: Vec::new(),
            events: Vec::new(),
            natives: Vec::new(),
            native_names: Vec::new(),
            warned: HashSet::new(),
            time: 0.0,
            ctx: EventCtx::default(),
            enum_ctx: EnumCtx::default(),
            sleepers: Vec::new(),
            pending: Vec::new(),
            requests: Vec::new(),
            map_name: String::new(),
            map_description: String::new(),
            player_count: 0,
            team_count: 0,
            placement: 0,
            game_type_selected: 0,
            game_types_supported: 0,
            map_flags: 0,
            game_speed: 2,
            difficulty: 1,
            camera_bounds: [0.0; 8],
            camera_margins: [0.0; 4],
            map_bounds: Rect::default(),
            playable: Rect::default(),
            time_of_day: 8.0,
            time_of_day_scale: 1.0,
            day_night_models: (String::new(), String::new()),
            fog_enabled: true,
            fog_mask_enabled: true,
            terrain_heights: None,
            global_variables_changed: HashMap::new(),
            misc,
            rng: crate::rng::Rng::new(0),
            trigger_strings: HashMap::new(),
            strings: HashMap::new(),
            local_player: 0,
            version: 1,
            unit_order: Vec::new(),
            selection: HashMap::new(),
            selection_serial: 0,
            player_leaderboards: HashMap::new(),
            multiboard_suppressed: false,
            tileset: 0,
            crippled: HashMap::new(),
            player_results: HashMap::new(),
            restart_disabled: false,
            load_game_name: String::new(),
            reserved_hero_buttons: 0,
            alt_minimap_icon: None,
            fog_boundary: true,
            model_cinematic: None,
            deformations: Vec::new(),
            pending_entangles: Vec::new(),
            blight: Vec::new(),
            blight_serial: 0,
            autocast_timer: 0.0,
            sentinels: Vec::new(),
            profile: None,
            lightning_data: None,
            saves_dir: None,
            order_error: None,
            destructable_order: Vec::new(),
            item_order: Vec::new(),
            dying_effects: Vec::new(),
            pathing: None,
            missiles: Vec::new(),
            pathing_textures: HashMap::new(),
            ai: HashMap::new(),
            ai_vms: HashMap::new(),
            ai_current: None,
            ai_difficulty: HashMap::new(),
            fog: None,
            shops: Default::default(),
            upgrade_cache: Default::default(),
            tech_cache: Default::default(),
            bolts: Vec::new(),
            jumps: Vec::new(),
            ground_effects: Vec::new(),
            quests: Vec::new(),
            defeat_conditions: Vec::new(),
            quest_flash: false,
            camera,
            cinematic: Cinematic::default(),
            terrain_fog: None,
            map_terrain_fog: None,
            sky_model: None,
            queued_events: Vec::new(),
            cancelled_constructions: Vec::new(),
            persist: Default::default(),
            timed_removals: Vec::new(),
        }
    }

    /// Resolves `TRIGSTR_n` references and interface string keys; other text is returned
    /// unchanged.
    /// A saved game's file from the name a script gives (".w3z" when it has no
    /// extension).
    pub fn save_path(&self, name: &str) -> Option<std::path::PathBuf> {
        let dir = self.saves_dir.as_ref()?;
        let name = name.replace('\\', "/");
        let file = std::path::Path::new(&name).file_name()?.to_string_lossy().to_string();
        let file = if file.contains('.') { file } else { format!("{file}.w3z") };
        Some(dir.join(file))
    }

    pub fn localized(&self, s: &str) -> String {
        if let Some(n) = s.strip_prefix("TRIGSTR_") {
            let digits: String = n.chars().take_while(|c| c.is_ascii_digit()).collect();
            if let Some(t) = digits.parse().ok().and_then(|n: u32| self.trigger_strings.get(&n)) {
                return t.clone();
            }
        }
        self.strings.get(s).cloned().unwrap_or_else(|| s.to_string())
    }

    /// A gameplay constant from Units\\MiscData.txt or Units\\MiscGame.txt.
    pub fn misc_f32(&self, key: &str, default: f32) -> f32 {
        self.misc.get_str("Misc", key).and_then(|v| v.trim().parse().ok()).unwrap_or(default)
    }

    /// Resolves each declared native to its implementation.
    pub fn bind_natives(&mut self, vm: &Vm) {
        let table = crate::natives::table();
        self.natives = vm.module.natives.iter().map(|n| table.get(&*n.name).copied()).collect();
        self.native_names = vm.module.natives.iter().map(|n| n.name.to_string()).collect();
        let missing = self.natives.iter().filter(|n| n.is_none()).count();
        log::info!("{} natives bound, {missing} not yet implemented", self.natives.len() - missing);
    }

    // ---- handle accessors ----

    pub fn unit(&self, h: u32) -> Option<&Unit> {
        match self.handles.get(h) {
            Some(Obj::Unit(u)) => Some(u),
            _ => None,
        }
    }
    pub fn unit_mut(&mut self, h: u32) -> Option<&mut Unit> {
        match self.handles.get_mut(h) {
            Some(Obj::Unit(u)) => Some(u),
            _ => None,
        }
    }
    pub fn player_of(&self, h: u32) -> Option<u8> {
        match self.handles.get(h) {
            Some(Obj::Player(p)) => Some(*p),
            _ => None,
        }
    }
    pub fn player_handle(&self, id: u8) -> Value {
        self.players.get(id as usize).map_or(Value::Null, |p| Value::Handle(p.handle))
    }
    pub fn timer_mut(&mut self, h: u32) -> Option<&mut Timer> {
        match self.handles.get_mut(h) {
            Some(Obj::Timer(t)) => Some(t),
            _ => None,
        }
    }
    pub fn trigger(&self, h: u32) -> Option<&Trigger> {
        match self.handles.get(h) {
            Some(Obj::Trigger(t)) => Some(t),
            _ => None,
        }
    }
    pub fn trigger_mut(&mut self, h: u32) -> Option<&mut Trigger> {
        match self.handles.get_mut(h) {
            Some(Obj::Trigger(t)) => Some(t),
            _ => None,
        }
    }
    pub fn group_mut(&mut self, h: u32) -> Option<&mut Group> {
        match self.handles.get_mut(h) {
            Some(Obj::Group(g)) => Some(g),
            _ => None,
        }
    }
    pub fn group(&self, h: u32) -> Option<&Group> {
        match self.handles.get(h) {
            Some(Obj::Group(g)) => Some(g),
            _ => None,
        }
    }
    pub fn force(&self, h: u32) -> Option<&Force> {
        match self.handles.get(h) {
            Some(Obj::Force(f)) => Some(f),
            _ => None,
        }
    }
    pub fn force_mut(&mut self, h: u32) -> Option<&mut Force> {
        match self.handles.get_mut(h) {
            Some(Obj::Force(f)) => Some(f),
            _ => None,
        }
    }
    pub fn rect(&self, h: u32) -> Option<Rect> {
        match self.handles.get(h) {
            Some(Obj::Rect(r)) => Some(*r),
            _ => None,
        }
    }
    pub fn location(&self, h: u32) -> Option<(f32, f32)> {
        match self.handles.get(h) {
            Some(Obj::Location(x, y)) => Some((*x, *y)),
            _ => None,
        }
    }
    pub fn region(&self, h: u32) -> Option<&Region> {
        match self.handles.get(h) {
            Some(Obj::Region(r)) => Some(r),
            _ => None,
        }
    }

    pub fn ground_z(&self, x: f32, y: f32) -> f32 {
        self.terrain_heights.as_ref().map_or(0.0, |t| t.at(x, y))
    }

    // ---- boolean expressions and triggers ----

    /// Evaluates a boolexpr handle; `null` evaluates to true.
    pub fn eval_boolexpr(&mut self, vm: &mut Vm, h: u32) -> bool {
        let e = match self.handles.get(h) {
            Some(Obj::BoolExpr(e)) => e.clone(),
            _ => return true,
        };
        match e {
            BoolExpr::Func(f) => match vm.call(self, f, &[]) {
                Outcome::Finished(v) => v.bool(),
                Outcome::Aborted(err) => {
                    log::warn!("condition aborted: {err}");
                    false
                }
                Outcome::Sleeping(_) => false,
            },
            BoolExpr::And(a, b) => self.eval_boolexpr(vm, a) && self.eval_boolexpr(vm, b),
            BoolExpr::Or(a, b) => self.eval_boolexpr(vm, a) || self.eval_boolexpr(vm, b),
            BoolExpr::Not(a) => !self.eval_boolexpr(vm, a),
        }
    }

    pub fn trigger_evaluate(&mut self, vm: &mut Vm, trig: u32) -> bool {
        let conds = match self.trigger_mut(trig) {
            Some(t) => {
                t.eval_count += 1;
                t.conditions.clone()
            }
            None => return false,
        };
        for c in conds {
            let expr = match self.handles.get(c) {
                Some(Obj::TriggerCondition { expr }) => *expr,
                _ => continue,
            };
            if !self.eval_boolexpr(vm, expr) {
                return false;
            }
        }
        true
    }

    /// Runs a trigger's actions on a new thread that may sleep.
    pub fn trigger_execute(&mut self, vm: &mut Vm, trig: u32, ctx: EventCtx) {
        let actions = match self.trigger_mut(trig) {
            Some(t) => {
                t.exec_count += 1;
                t.actions.clone()
            }
            None => return,
        };
        for a in actions {
            let f = match self.handles.get(a) {
                Some(Obj::TriggerAction { func }) => *func,
                _ => continue,
            };
            let mut ctx = ctx.clone();
            ctx.trigger = trig;
            let t = vm.new_thread(f, &[], true);
            self.run_thread(vm, t, ctx);
        }
    }

    /// Runs a thread with an event context until it finishes or sleeps.
    pub fn run_thread(&mut self, vm: &mut Vm, mut t: Thread, ctx: EventCtx) {
        let saved = std::mem::replace(&mut self.ctx, ctx);
        match vm.run(self, &mut t) {
            Outcome::Finished(_) => {}
            Outcome::Sleeping(secs) => {
                let ctx = self.ctx.clone();
                self.sleepers.push(Sleeper { wake_at: self.time + secs.max(0.0), thread: t, ctx });
            }
            Outcome::Aborted(e) => log::warn!("thread aborted: {e}"),
        }
        self.ctx = saved;
        self.run_pending(vm);
    }

    fn run_pending(&mut self, vm: &mut Vm) {
        while !self.pending.is_empty() {
            let batch = std::mem::take(&mut self.pending);
            for (t, ctx) in batch {
                self.run_thread(vm, t, ctx);
            }
        }
    }

    /// Fires every enabled trigger registered for events matching `pred`, evaluating
    /// conditions first.
    pub fn fire(&mut self, vm: &mut Vm, ctx: EventCtx, pred: impl Fn(&World, &EventKind) -> bool) {
        let regs: Vec<(u32, u32)> = self
            .events
            .iter()
            .filter_map(|&e| match self.handles.get(e) {
                Some(Obj::Event(r)) if pred(self, &r.kind) => Some((e, r.trigger)),
                _ => None,
            })
            .collect();
        for (_event, trig) in regs {
            if !self.trigger(trig).is_some_and(|t| t.enabled) {
                continue;
            }
            let mut c = ctx.clone();
            c.trigger = trig;
            let saved = std::mem::replace(&mut self.ctx, c.clone());
            let pass = self.trigger_evaluate(vm, trig);
            self.ctx = saved;
            if pass {
                self.trigger_execute(vm, trig, c);
            }
        }
    }

    // ---- time ----

    /// Advances game time: timers, sleeping threads, and time-based events.
    /// Rescue (CAbilityRescuable): a unit rescuable by a player, or owned by a player
    /// rescuable by it (ALLIANCE_RESCUABLE), raises EVENT_PLAYER_UNIT_RESCUED once one of
    /// that player's units comes within its rescue range (384 unless set); Blizzard.j
    /// then changes its owner.
    fn update_rescue(&mut self) {
        const ALLIANCE_RESCUABLE: usize = 8;
        let mut rescued = Vec::new();
        for &h in &self.unit_order {
            let Some(u) = self.unit(h).filter(|u| !u.dead) else { continue };
            let by_alliance: u16 = (0..PLAYER_COUNT).filter(|&p| self.players.get(u.owner as usize).is_some_and(|o| o.alliances[p][ALLIANCE_RESCUABLE])).fold(0, |m, p| m | 1 << p);
            let by = u.extra.rescuable_by | by_alliance;
            if by == 0 {
                continue;
            }
            let range = if u.extra.rescue_range > 0.0 { u.extra.rescue_range } else { 384.0 };
            let rescuer = self.unit_order.iter().copied().find(|&r| {
                self.unit(r).is_some_and(|ru| !ru.dead && !ru.hidden && ru.owner != u.owner && by & (1 << ru.owner) != 0 && (ru.x - u.x).hypot(ru.y - u.y) <= range + u.collision + ru.collision)
            });
            if let Some(r) = rescuer {
                rescued.push((h, r));
            }
        }
        for (h, r) in rescued {
            if let Some(u) = self.unit_mut(h) {
                u.extra.rescuable_by = 0;
            }
            use crate::event_ids::*;
            self.queue_unit_event(h, EVENT_PLAYER_UNIT_RESCUED, EVENT_UNIT_RESCUED, EventCtx { other_unit: r, ..Default::default() });
        }
    }

    /// Queues a unit event to fire with the VM (see `queued_events`).
    pub fn queue_unit_event(&mut self, unit: u32, player_event: i32, unit_event: i32, ctx: EventCtx) {
        self.queued_events.push((unit, player_event, unit_event, ctx));
    }

    pub fn flush_events(&mut self, vm: &mut Vm) {
        for (unit, pe, ue, ctx) in std::mem::take(&mut self.queued_events) {
            self.fire_unit_pair(vm, unit, pe, ue, ctx);
        }
        for h in std::mem::take(&mut self.cancelled_constructions) {
            self.uncover_mine(vm, h);
            crate::natives::units::remove_unit(self, h);
        }
    }

    pub fn tick(&mut self, vm: &mut Vm, dt: f32) {
        self.tech_cache.borrow_mut().clear();
        // W3PROF: time per subsystem, printed every 60 game seconds.
        macro_rules! prof {
            ($w:expr, $name:expr, $e:expr) => {{
                if $w.profile.is_some() {
                    let t0 = std::time::Instant::now();
                    let r = $e;
                    *$w.profile.as_mut().unwrap().entry($name).or_insert(0.0) += t0.elapsed().as_secs_f64();
                    r
                } else {
                    $e
                }
            }};
        }
        if self.profile.is_none() && std::env::var("W3PROF").is_ok() {
            self.profile = Some(HashMap::new());
        }
        if let Some(p) = self.profile.as_mut() {
            if (self.time / 5.0).floor() != ((self.time + dt) / 5.0).floor() {
                let mut v: Vec<(&&str, &f64)> = p.iter().collect();
                v.sort_by(|a, b| b.1.total_cmp(a.1));
                eprintln!("profile at {:.0}s: {}", self.time, v.iter().take(8).map(|(k, t)| format!("{k} {:.2}s", t)).collect::<Vec<_>>().join(", "));
            }
        }
        self.time += dt;
        let day_hours = self.misc_f32("DayHours", 24.0);
        let day_length = self.misc_f32("DayLength", 480.0);
        let hours_per_second = day_hours / day_length * self.time_of_day_scale;
        self.time_of_day = (self.time_of_day + dt * hours_per_second) % day_hours;
        self.camera.update(dt);
        prof!(self, "update_text_tags", self.update_text_tags(dt));
        prof!(self, "update_blight", self.update_blight(dt));
        prof!(self, "update_mines", self.update_mines());
        prof!(self, "update_moon_wells", self.update_moon_wells());
        prof!(self, "update_forms", self.update_forms(dt));
        prof!(self, "update_cargo", self.update_cargo(vm));
        prof!(self, "update_morphs", self.update_morphs(dt));
        prof!(self, "update_autocast", self.update_autocast(dt));
        let now = self.time;
        let due: Vec<(f32, u32, u32)> = self.pending_entangles.iter().copied().filter(|e| e.0 <= now).collect();
        self.pending_entangles.retain(|e| e.0 > now);
        for (_, caster, mine) in due {
            self.entangle(caster, mine);
        }
        // TimerDialogSetRealTimeRemaining counts down in the dialog itself.
        for (_, o) in self.handles.iter_mut() {
            if let Obj::TimerDialog(d) = o {
                if let Some(t) = d.real_time.as_mut() {
                    *t -= dt;
                }
            }
        }
        if self.cinematic.scene.as_ref().is_some_and(|s| s.ends <= self.time) {
            self.cinematic.scene = None;
        }

        prof!(self, "update_food", self.update_food());
        prof!(self, "update_units", self.update_units(vm, dt));
        prof!(self, "regenerate", self.regenerate(dt));
        prof!(self, "update_buffs", self.update_buffs(vm, dt));
        prof!(self, "update_area_buffs", self.update_area_buffs(vm, dt));
        prof!(self, "update_traps", self.update_traps(vm));
        prof!(self, "update_splits", self.update_splits(dt));
        self.update_bolts(vm, dt);
        // Owls leave fallen trees.
        let standing: Vec<bool> = self.sentinels.iter().map(|s| self.is_tree_pub(s.tree)).collect();
        let fallen: Vec<u32> = self.sentinels.iter().zip(&standing).filter(|(_, ok)| !**ok).map(|(s, _)| s.effect).collect();
        let mut k = 0;
        self.sentinels.retain(|_| {
            k += 1;
            standing[k - 1]
        });
        for e in fallen {
            if let Some(Obj::Effect(x)) = self.handles.get_mut(e) {
                x.destroyed = true;
            }
        }
        prof!(self, "update_channels", self.update_channels(vm, dt));
        prof!(self, "update_ground_effects", self.update_ground_effects(vm, dt));
        prof!(self, "update_reincarnation", self.update_reincarnation(dt));
        prof!(self, "update_fog", self.update_fog(dt));
        prof!(self, "update_shops", self.update_shops(dt));
        prof!(self, "update_ai", self.update_ai(dt));
        prof!(self, "update_rescue", self.update_rescue());
        self.flush_events(vm);

        // Timers.
        let mut expired = Vec::new();
        for id in self.handles.ids() {
            if let Some(Obj::Timer(t)) = self.handles.get_mut(id) {
                if !t.running {
                    continue;
                }
                t.remaining -= dt;
                if t.remaining <= 0.0 {
                    if t.periodic && t.timeout > 0.0 {
                        t.remaining += t.timeout;
                    } else {
                        t.running = false;
                        t.remaining = 0.0;
                    }
                    expired.push((id, t.callback));
                }
            }
        }
        for (timer, callback) in expired {
            let ctx = EventCtx { timer, ..Default::default() };
            if let Some(f) = callback {
                let t = vm.new_thread(f, &[], false);
                self.run_thread(vm, t, ctx.clone());
            }
            self.fire(vm, ctx, |_, k| matches!(k, EventKind::TimerExpire(t) if *t == timer));
        }

        // Sleeping threads.
        let now = self.time;
        let mut i = 0;
        while i < self.sleepers.len() {
            if self.sleepers[i].wake_at <= now {
                let s = self.sleepers.swap_remove(i);
                self.run_thread(vm, s.thread, s.ctx);
            } else {
                i += 1;
            }
        }

        // Elapsed-time game state events.
        let t = self.time;
        self.fire(vm, EventCtx::default(), |_, k| match k {
            EventKind::GameState { state: 2, op, value } => compare(*op, t, *value) && !compare(*op, t - dt, *value),
            _ => false,
        });
    }

    /// Cells blocked by a pathing texture (PathTextures\\*.tga) centered at a position.
    /// Red marks unwalkable cells; one texel is one 32x32 cell.  The texture turns with the
    /// object in quarter turns.
    pub fn pathing_cells(&mut self, path_tex: &str, x: f32, y: f32, facing_deg: f32) -> Vec<(i32, i32)> {
        if path_tex.is_empty() || path_tex == "_" || path_tex.eq_ignore_ascii_case("none") {
            return Vec::new();
        }
        let key = path_tex.to_ascii_lowercase();
        if !self.pathing_textures.contains_key(&key) {
            let img = self.chain.read(path_tex).ok().and_then(|b| w3_formats::blp::decode_texture(path_tex, &b).ok()).and_then(|v| v.into_iter().next());
            self.pathing_textures.insert(key.clone(), img);
        }
        let (Some(Some(img)), Some(p)) = (self.pathing_textures.get(&key), self.pathing.as_ref()) else { return Vec::new() };
        let (w, h) = (img.width as i32, img.height as i32);
        let quarter = ((facing_deg / 90.0).round() as i32).rem_euclid(4);
        let (cx, cy) = p.cell_of(x, y);
        let mut out = Vec::new();
        for ty in 0..h {
            for tx in 0..w {
                let i = ((ty * w + tx) * 4) as usize;
                if img.rgba[i] == 0 {
                    continue;
                }
                // Texel offsets from the center, with texture rows running north to south.
                let (ox, oy) = (tx - w / 2, (h - 1 - ty) - h / 2);
                let (rx, ry) = match quarter {
                    0 => (ox, oy),
                    1 => (-oy - 1, ox),
                    2 => (-ox - 1, -oy - 1),
                    _ => (oy, -ox - 1),
                };
                out.push((cx + rx, cy + ry));
            }
        }
        out
    }

    /// Recomputes each player's food used and food cap from their living units.
    pub fn update_food(&mut self) {
        let mut used = [0i32; PLAYER_COUNT];
        let mut made = [0i32; PLAYER_COUNT];
        for &h in &self.unit_order {
            let Some(u) = self.unit(h) else { continue };
            if u.dead {
                continue;
            }
            if let Some(o) = self.objects.units.get(u.type_id) {
                if u.extra.use_food {
                    used[u.owner as usize % PLAYER_COUNT] += o.int("fused");
                }
                if u.constructing.is_none() {
                    made[u.owner as usize % PLAYER_COUNT] += o.int("fmade");
                }
                // Queued units hold their food while training.
                for q in &u.queue {
                    used[u.owner as usize % PLAYER_COUNT] += self.objects.units.get(*q).map_or(0, |o| o.int("fused"));
                }
            }
        }
        for (i, p) in self.players.iter_mut().enumerate() {
            p.state.insert(player_state::RESOURCE_FOOD_USED, used[i]);
            let ceiling = p.get_state(player_state::FOOD_CAP_CEILING);
            let cap = if ceiling > 0 { made[i].min(ceiling) } else { made[i] };
            p.state.insert(player_state::RESOURCE_FOOD_CAP, cap);
        }
    }

    pub fn warn_unimplemented(&mut self, native: u32) {
        if self.warned.insert(native) {
            log::warn!(
                "native {} is not implemented yet",
                self.native_names.get(native as usize).map_or("?", |s| s.as_str())
            );
        }
    }
}

/// LIMIT_OP comparison (common.j: LESS_THAN = 0 ... NOT_EQUAL = 5).
pub fn compare(op: i32, a: f32, b: f32) -> bool {
    match op {
        0 => a < b,
        1 => a <= b,
        2 => a == b,
        3 => a >= b,
        4 => a > b,
        _ => a != b,
    }
}

impl Host for World {
    fn call_native(&mut self, vm: &mut Vm, native: u32, args: &[Value]) -> NativeResult {
        if let Some(p) = self.ai_current {
            let f = self.ai.get(&p).and_then(|a| a.natives.get(native as usize).copied().flatten());
            return match f {
                Some(f) => f(self, vm, args),
                None => {
                    let ret = vm.module.natives[native as usize].ret_base;
                    NativeResult::Return(ret.map_or(Value::Null, |b| b.default_value()))
                }
            };
        }
        log::trace!("{}{:?}", self.native_names.get(native as usize).map_or("?", |s| s.as_str()), args);
        match self.natives.get(native as usize).copied().flatten() {
            Some(f) => f(self, vm, args),
            None => {
                self.warn_unimplemented(native);
                let ret = vm.module.natives[native as usize].ret.clone();
                NativeResult::Return(match ret {
                    None => Value::Null,
                    Some(t) => vm.module.base_type(&t).default_value(),
                })
            }
        }
    }
}

pub fn func_of(v: &Value) -> Option<FuncId> {
    v.code()
}

impl World {
    /// The text a timer dialog shows: its timer's remaining time (or its own countdown)
    /// times its speed, rounded to whole seconds, as hours:minutes:seconds; 00:00:00
    /// once it reaches zero.
    pub fn timer_dialog_text(&self, d: &TimerDialog) -> String {
        let remaining = d.real_time.unwrap_or_else(|| match self.handles.get(d.timer) {
            Some(Obj::Timer(t)) => t.remaining,
            _ => 0.0,
        });
        if remaining <= 0.0 {
            return "00:00:00".to_string();
        }
        let t = (d.speed * remaining + 0.5) as i64 as f32;
        let h = (t / 3600.0) as i64;
        let rest = t - (h * 3600) as f32;
        let m = (rest / 60.0) as i64;
        let s = (rest - (m * 60) as f32) as i64;
        format!("{h:02}:{m:02}:{s:02}")
    }
}
