//! The game: object data, world state, and the JASS host that runs map scripts.

pub mod ability;
pub mod ai;
pub mod event_ids;
pub mod fog;
pub mod handles;
pub mod hero;
pub mod natives;
pub mod objects;
pub mod orders;
pub mod pathing;
pub mod rng;
pub mod sim;
pub mod types;
pub mod world;

use std::collections::HashMap;
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
}

/// How a player slot is filled when the game starts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotFill {
    Local,
    Computer,
    Empty,
}

/// One player's choices from the game setup screen.
#[derive(Clone, Debug)]
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
        let info = MapInfo::parse(&chain.read("war3map.w3i")?).context("war3map.w3i")?;
        let common = read_script(&chain, &["Scripts\\common.j"])?;
        let blizzard = read_script(&chain, &["Scripts\\Blizzard.j"])?;
        let map = read_script(&chain, &["war3map.j", "Scripts\\war3map.j"])?;
        let module = w3_jass::compile([("common.j", &*common), ("Blizzard.j", &*blizzard), ("war3map.j", &*map)])?;
        let vm = Vm::new(module);
        let mut objects = ObjectDb::load(&chain);
        objects.apply_map(&chain);
        let mut world = World::new(chain.clone(), objects);
        world.version = version;
        world.bind_natives(&vm);
        // Interface strings (FDF StringList tables) resolve GetLocalizedString keys.
        world.strings = w3_ui::FrameDb::load(&chain).strings;
        if let Ok(s) = chain.read_string("war3map.wts") {
            world.trigger_strings = w3_formats::map::parse_wts(&s);
        }
        if let Ok(t) = chain.read("war3map.w3e").map_err(anyhow::Error::from).and_then(|b| Terrain::parse(&b).map_err(Into::into)) {
            let (w, h) = (t.width, t.height);
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
                heights: t.corners.iter().map(|c| c.height()).collect(),
                layers: t.corners.iter().map(|c| c.layer_height).collect(),
            });
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
        for p in &info.players {
            let name = world.localized(&p.name);
            if let Some(pl) = world.players.get_mut(p.id as usize) {
                pl.name = name;
            }
        }
        Ok(Game { vm, world, info })
    }

    /// Runs the map's `config` function, which declares players and start locations.
    pub fn config(&mut self) -> anyhow::Result<()> {
        let w = &mut self.world;
        self.vm.init_globals(w).map_err(anyhow::Error::msg)?;
        self.call("config")
    }

    /// Applies the setup screen's choices, then runs the map's `main`.
    pub fn start_lobby(&mut self, slots: &[LobbySlot]) -> anyhow::Result<()> {
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
            match s.race {
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
        let fills: HashMap<u8, SlotFill> = slots.iter().map(|s| (s.player, s.fill.clone())).collect();
        self.start(|id, _| fills.get(&id).cloned().unwrap_or(SlotFill::Empty))
    }

    /// Fills player slots as the lobby would, then runs the map's `main`.
    pub fn start(&mut self, mut fill: impl FnMut(u8, i32) -> SlotFill) -> anyhow::Result<()> {
        for p in self.world.players.iter_mut() {
            let controller = p.controller;
            if controller == map_control::USER || controller == map_control::COMPUTER {
                match fill(p.id, controller) {
                    SlotFill::Local => {
                        p.controller = map_control::USER;
                        p.slot_state = slot_state::PLAYING;
                        self.world.local_player = p.id;
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
        // Neutral players always play.
        for i in types::PLAYER_NEUTRAL_AGGRESSIVE..types::PLAYER_COUNT as u8 {
            let p = &mut self.world.players[i as usize];
            p.slot_state = slot_state::PLAYING;
        }
        self.call("main")
    }

    fn call(&mut self, name: &str) -> anyhow::Result<()> {
        let f = self.vm.module.function(name).with_context(|| format!("map script has no {name}"))?;
        let t = self.vm.new_thread(f, &[], true);
        self.world.run_thread(&mut self.vm, t, Default::default());
        Ok(())
    }

    /// A player clicked a dialog button: the dialog closes for them, then the button's
    /// and the dialog's click events fire (EVENT_DIALOG_BUTTON_CLICK, EVENT_DIALOG_CLICK).
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
        if quit.is_some() {
            self.world.requests.push(world::Request::EndGame);
        }
    }

    /// Kills a unit (for tools and tests).
    pub fn kill(&mut self, h: u32) {
        natives::units::kill_unit(&mut self.world, &mut self.vm, h, 0);
    }

    /// Advances the simulation.
    pub fn tick(&mut self, dt: f32) {
        self.world.tick(&mut self.vm, dt);
    }
}
