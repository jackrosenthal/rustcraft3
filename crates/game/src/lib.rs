//! The game: object data, world state, and the JASS host that runs map scripts.

pub mod ability;
pub mod ai;
pub mod event_ids;
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
            });
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
        for (i, p) in info.players.iter().enumerate() {
            if let Some(pl) = world.players.get_mut(p.id as usize) {
                pl.name = p.name.clone();
                let _ = i;
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

    /// Advances the simulation.
    pub fn tick(&mut self, dt: f32) {
        self.world.tick(&mut self.vm, dt);
    }
}
