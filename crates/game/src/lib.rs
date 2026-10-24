//! The game: object data, world state, and the JASS host that runs map scripts.

pub mod ability;
pub mod ai;
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
pub mod targets;
pub mod passives;
pub mod techtree;
pub mod bolts;
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
pub mod volcano;
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
pub mod pathing;
pub mod placement;
pub mod repair;
pub mod rng;
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
        let info = MapInfo::parse(&chain.read("war3map.w3i")?).context("war3map.w3i")?;
        let common = read_script(&chain, &["Scripts\\common.j"])?;
        let blizzard = read_script(&chain, &["Scripts\\Blizzard.j"])?;
        let map = read_script(&chain, &["war3map.j", "Scripts\\war3map.j"])?;
        let module = w3_jass::compile([("common.j", &*common), ("Blizzard.j", &*blizzard), ("war3map.j", &*map)])?;
        let vm = Vm::new(module);
        let mut objects = ObjectDb::load(&chain);
        if let Some(k) = custom_keys {
            objects.apply_custom_keys(k);
        }
        objects.apply_campaign(&chain);
        objects.apply_map(&chain);
        let mut world = World::new(chain.clone(), objects);
        world.version = version;
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
            world.fog = Some(fog::FogMap::new(world.map_bounds, |x, y| th.level_at(x, y)));
        }
        if let (Ok(wpm), Some(t)) = (chain.read("war3map.wpm"), world.terrain_heights.as_ref()) {
            world.pathing = pathing::PathingMap::parse(&wpm, t.offset);
        }
        if let Ok(bytes) = chain.read("war3map.doo") {
            match w3_formats::map::Doodads::parse(&bytes) {
                Ok(doo) => {
                    for d in &doo.doodads {
                        if world.objects.destructables.get(d.id).is_none() {
                            continue;
                        }
                        let h = natives::objects::create_destructable(
                            &mut world,
                            d.id,
                            d.position[0],
                            d.position[1],
                            Some(d.position[2]),
                            d.angle.to_degrees(),
                            d.scale[0],
                            d.variation,
                        );
                        // The placed life is a percentage of the type's maximum.
                        if let Some(types::Obj::Destructable(x)) = world.handles.get_mut(h) {
                            x.life = x.max_life * d.life as f32 / 100.0;
                        }
                    }
                }
                Err(e) => log::warn!("war3map.doo: {e}"),
            }
        }
        world.camera_bounds = info.camera_bounds;
        // The map's global weather (w3i) covers the whole map.
        if info.weather != [0; 4] {
            world.handles.insert(types::Obj::Weather(types::Weather { rect: world.map_bounds, id: info.weather, enabled: true }));
        }
        for p in &info.players {
            let name = world.localized(&p.name);
            if let Some(pl) = world.players.get_mut(p.id as usize) {
                pl.name = name;
            }
        }
        Ok(Game { vm, world, info, recorder: None, locked: false, outbox: None, sent_selection: None })
    }

    /// Runs the map's `config` function, which declares players and start locations.
    pub fn config(&mut self) -> anyhow::Result<()> {
        let w = &mut self.world;
        self.vm.init_globals(w).map_err(anyhow::Error::msg)?;
        self.call("config")
    }

    /// Applies the setup screen's choices, then runs the map's `main`.  The setup's
    /// advanced options are the map flags scripts read (IsMapFlagSet): Random Races
    /// (0x40000) draws every race, and the visibility always visible (0x4) or map
    /// explored (0x2) lifts the fog or its mask.
    pub fn start_lobby(&mut self, slots: &[LobbySlot], flags: u32) -> anyhow::Result<()> {
        self.world.map_flags = flags as i32;
        self.world.lobby_teams = true;
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
                None => {
                    // A random race is drawn when the game starts.
                    let r = self.world.rng.int(1, 4);
                    let p = &mut self.world.players[s.player as usize];
                    p.race = r;
                    p.race_preference = PREF_RANDOM;
                }
            }
            if matches!(s.fill, SlotFill::Computer) {
                self.world.ai_difficulty.insert(s.player, s.difficulty);
            }
        }
        // Observers hold player slots the map leaves unused: they are users who do not
        // play (IsPlayerObserver), with no start location or units.
        for s in slots.iter().filter(|s| matches!(s.fill, SlotFill::Observer(_))) {
            if let Some(p) = self.world.players.get_mut(s.player as usize) {
                p.controller = map_control::USER;
            }
        }
        let fills: HashMap<u8, SlotFill> = slots.iter().map(|s| (s.player, s.fill.clone())).collect();
        self.start(|id, _| fills.get(&id).cloned().unwrap_or(SlotFill::Empty))
    }

    /// Fills player slots as the lobby would, then runs the map's `main`.
    pub fn start(&mut self, mut fill: impl FnMut(u8, i32) -> SlotFill) -> anyhow::Result<()> {
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
        for p in self.world.players.iter_mut() {
            let controller = p.controller;
            if controller == map_control::USER || controller == map_control::COMPUTER {
                match fill(p.id, controller) {
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
        }
        // The players take their start locations by the map's placement.
        self.world.place_start_locations();
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
        self.call("main")
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

    fn call(&mut self, name: &str) -> anyhow::Result<()> {
        let f = self.vm.module.function(name).with_context(|| format!("map script has no {name}"))?;
        let t = self.vm.new_thread(f, &[], true);
        self.world.run_thread(&mut self.vm, t, Default::default());
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
        self.world.fire(&mut self.vm, ctx, move |_, k| matches!(k, types::EventKind::Player { player: p, event: e } if *p == player && *e == event));
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
        let regs: Vec<(u32, String)> = self
            .world
            .events
            .iter()
            .filter_map(|&e| match self.world.handles.get(e) {
                Some(types::Obj::Event(r)) => match &r.kind {
                    types::EventKind::PlayerChat { player: p, text: t, exact } if *p == player && if *exact { t == text } else { text.contains(t.as_str()) } => Some((r.trigger, t.clone())),
                    _ => None,
                },
                _ => None,
            })
            .collect();
        for (trig, matched) in regs {
            let ctx = types::EventCtx { player, has_player: true, event_id: event_ids::EVENT_PLAYER_CHAT, chat: text.to_string(), matched_chat: matched, ..Default::default() };
            self.world.fire_trigger(&mut self.vm, trig, ctx);
        }
        true
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

    /// Advances the simulation.
    pub fn tick(&mut self, dt: f32) {
        if let Some(r) = self.recorder.as_mut() {
            r.step(dt);
        }
        self.world.tick(&mut self.vm, dt);
    }

    /// Plays one frame (of a replay or from a network game's host): its commands, then
    /// its step; returns what each command answered, with its player.
    pub fn play_frame(&mut self, f: &replay::Frame) -> Vec<(u8, replay::Outcome)> {
        let out = f.commands.iter().map(|(p, c)| (*p, self.execute(*p, c.clone()))).collect();
        if let Some(r) = self.recorder.as_mut() {
            r.step(f.dt);
        }
        // A paused game takes commands (a resume among them) but no time passes.
        if self.world.net_paused.is_none() {
            self.world.tick(&mut self.vm, f.dt);
        }
        out
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
        self.execute(local, c)
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
            r.command(player, c.clone());
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
            C::Formation { units } => {
                let mine: Vec<u32> = units.into_iter().filter(|&u| natives::players::controls(&self.world, player, u)).collect();
                self.world.form_group(&mine);
                O::Done
            }
            C::Build { worker, id, x, y } => {
                if let Some(u) = self.world.unit_mut(worker) {
                    u.action = types::Action::Build { id, x, y };
                    u.path.clear();
                }
                O::Done
            }
            C::Train { building, id } => res(self.world.train(building, id)),
            C::Research { building, id } => res(self.world.research_at(building, id)),
            C::Upgrade { building, id } => res(self.world.upgrade_structure(building, id)),
            C::CancelLast { building } => {
                self.world.cancel_last(building);
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
            C::Save { name } => {
                // The triggers hear the save first (EVENT_GAME_SAVE).
                self.game_event(event_ids::EVENT_GAME_SAVE);
                self.world.requests.push(world::Request::NetSave(name));
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
            C::Select { player, units } => {
                self.set_selection(player, &units);
                O::Done
            }
            C::Trackable { player, trackable, hit } => {
                let event = if hit { event_ids::EVENT_GAME_TRACKABLE_HIT } else { event_ids::EVENT_GAME_TRACKABLE_TRACK };
                let ctx = types::EventCtx { player, has_player: true, trackable, event_id: event, ..Default::default() };
                self.world.fire(&mut self.vm, ctx, move |_, k| matches!(k, types::EventKind::Trackable { trackable: t, hit: h } if *t == trackable && *h == hit));
                O::Done
            }
            // A player opened the hero skill menu or the build menu: the game events
            // EVENT_GAME_SHOW_SKILL and EVENT_GAME_BUILD_SUBMENU.
            C::Submenu { build, .. } => {
                self.game_event(if build { event_ids::EVENT_GAME_BUILD_SUBMENU } else { event_ids::EVENT_GAME_SHOW_SKILL });
                O::Done
            }
            // A network game's player is gone: their slot is left, and EVENT_PLAYER_LEAVE
            // fires.
            C::PlayerLeft { player } => {
                if let Some(p) = self.world.players.get_mut(player as usize) {
                    p.slot_state = types::slot_state::LEFT;
                }
                self.player_event(player, event_ids::EVENT_PLAYER_LEAVE);
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
