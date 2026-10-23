//! Saved games: the world's state and the scripts' variables and threads.  Loading
//! starts the same map again (its object data, script, and strings) and puts the saved
//! state in place of the fresh one.

use serde::{Deserialize, Serialize};
use w3_jass::{Thread, Value};

use crate::Game;
use crate::world::World;

/// Bumped whenever the saved state's layout changes.
const FORMAT: u32 = 1;

type Variables = (Vec<Value>, Vec<Vec<Value>>);

#[derive(Serialize)]
struct SaveRef<'a> {
    format: u32,
    map: &'a str,
    world: &'a World,
    variables: Variables,
    ai: Vec<(u8, Variables, Vec<(f32, Thread)>)>,
}

#[derive(Deserialize)]
pub struct SaveGame {
    format: u32,
    /// The map the game is on, as the program named it when starting it.
    pub map: String,
    world: World,
    variables: Variables,
    ai: Vec<(u8, Variables, Vec<(f32, Thread)>)>,
}

impl Game {
    /// The game's state as a saved game of map `map`.
    pub fn save(&self, map: &str) -> anyhow::Result<Vec<u8>> {
        let ai = self.world.ai_vms.iter().map(|(&p, a)| (p, a.vm.variables(), a.sleepers.clone())).collect();
        let s = SaveRef { format: FORMAT, map, world: &self.world, variables: self.vm.variables(), ai };
        Ok(postcard::to_stdvec(&s)?)
    }

    /// Restores a saved game into a game freshly loaded from the same map (before its
    /// script has run).
    pub fn restore(&mut self, save: SaveGame) -> anyhow::Result<()> {
        if save.format != FORMAT {
            anyhow::bail!("saved game format {} is not {FORMAT}", save.format);
        }
        let mut world = save.world;
        let fresh = &mut self.world;
        // What comes from the map rather than the game.
        std::mem::swap(&mut world.chain, &mut fresh.chain);
        std::mem::swap(&mut world.objects, &mut fresh.objects);
        std::mem::swap(&mut world.natives, &mut fresh.natives);
        std::mem::swap(&mut world.native_names, &mut fresh.native_names);
        std::mem::swap(&mut world.trigger_strings, &mut fresh.trigger_strings);
        std::mem::swap(&mut world.strings, &mut fresh.strings);
        std::mem::swap(&mut world.misc, &mut fresh.misc);
        std::mem::swap(&mut world.terrain_heights, &mut fresh.terrain_heights);
        self.world = world;
        self.vm.set_variables(save.variables);
        // AI scripts compile again and take back their variables and threads.
        for (player, vars, sleepers) in save.ai {
            let Some(script) = self.world.ai.get(&player).map(|a| a.script.clone()) else { continue };
            let Some((mut vm, natives, names)) = self.world.compile_ai(&script) else { continue };
            vm.set_variables(vars);
            if let Some(a) = self.world.ai.get_mut(&player) {
                a.natives = natives;
                a.native_names = names;
            }
            self.world.ai_vms.insert(player, crate::ai::AiVm { vm, sleepers });
        }
        Ok(())
    }
}

impl SaveGame {
    pub fn parse(bytes: &[u8]) -> anyhow::Result<SaveGame> {
        Ok(postcard::from_bytes(bytes)?)
    }
}
