//! Saved games: the world's state and the scripts' variables and threads.  Loading
//! starts the same map again (its object data, script, and strings) and puts the saved
//! state in place of the fresh one.

use serde::{Deserialize, Serialize};
use w3_jass::{Thread, Value};

use crate::Game;
use crate::world::World;

/// Bumped whenever the saved state's layout changes.
const FORMAT: u32 = 13;

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

/// A player of a saved game, for loading a network game.
#[derive(Clone, Debug)]
pub struct SavedPlayer {
    pub id: u8,
    pub name: String,
    pub human: bool,
    pub computer: bool,
    pub observer: bool,
    pub race: i32,
    pub team: i32,
    pub color: i32,
}

impl SaveGame {
    /// The game's people, computers, and observers.
    pub fn players(&self) -> Vec<SavedPlayer> {
        use crate::types::{map_control, player_state, slot_state};
        self.world
            .players
            .iter()
            .filter(|p| p.id < 12)
            .filter_map(|p| {
                let observer = p.get_state(player_state::OBSERVER) != 0 && p.controller == map_control::USER;
                let playing = p.slot_state == slot_state::PLAYING;
                let human = p.controller == map_control::USER && (playing || observer);
                let computer = p.controller == map_control::COMPUTER && playing;
                (human || computer).then(|| SavedPlayer { id: p.id, name: p.name.clone(), human, computer, observer, race: p.race, team: p.team, color: p.color })
            })
            .collect()
    }

    pub fn parse(bytes: &[u8]) -> anyhow::Result<SaveGame> {
        Ok(postcard::from_bytes(bytes)?)
    }

    /// The saved game's map, read without the rest of the state.
    pub fn peek_map(bytes: &[u8]) -> Option<String> {
        let ((format, map), _) = postcard::take_from_bytes::<(u32, String)>(bytes).ok()?;
        (format == FORMAT).then_some(map)
    }
}

/// A saved game to load: the port's, or the original's (.w3z in its recorded-game
/// container, `w3_formats::w3z`).
pub enum Load {
    Port(SaveGame),
    Original(Box<w3_formats::w3z::SaveGame>),
}

impl Load {
    pub fn parse(bytes: &[u8]) -> anyhow::Result<Load> {
        if bytes.starts_with(w3_formats::container::MAGIC) {
            return Ok(Load::Original(Box::new(w3_formats::w3z::SaveGame::read(bytes)?)));
        }
        Ok(Load::Port(SaveGame::parse(bytes)?))
    }

    /// The map the game is on (an original's from the game's folder).
    pub fn map(&self) -> &str {
        match self {
            Load::Port(s) => &s.map,
            Load::Original(s) => &s.map,
        }
    }
}

impl Game {
    /// Puts an original's saved game in place of a game just started on its map (its
    /// script's main run): its units (their type, owner, place, facing, life, and mana),
    /// the handles scripts had them by, its players' gold and lumber (saved in tenths),
    /// the time of day, and the map script's integer, real, and boolean variables (by
    /// name, arrays too).  Not yet:
    /// the rest (the time, which the port's timers and sleeping threads count from;
    /// orders, buildings in progress, items, destructables, the other variables, the
    /// AI scripts', timers, and threads), which stay as the script just made them.
    pub fn restore_original(&mut self, s: &w3_formats::w3z::SaveGame) -> anyhow::Result<()> {
        use crate::natives::units::{create_unit_exact, remove_unit};
        let units = s.units()?;
        let handles: std::collections::HashMap<(u32, u32), u32> = s.handles()?.into_iter().map(|(r, h)| ((r.0, r.1), h)).collect();
        let players = s.player_states()?;
        let game_reals = s.game_reals()?;
        let scripts = s.script_data()?;
        let w = &mut self.world;
        let origin = w.pathing.as_ref().map_or([0.0; 2], |p| p.origin);
        for h in w.unit_order.clone() {
            remove_unit(w, h);
        }
        for u in units {
            let (x, y) = (origin[0] + u.cell[0] * crate::pathing::CELL, origin[1] + u.cell[1] * crate::pathing::CELL);
            let h = create_unit_exact(w, u.owner as u8, u.type_id.to_be_bytes(), x, y, u.facing.to_degrees());
            // The handle scripts had it by, as the save's handle table has it.
            if let Some(&jass) = handles.get(&(u.tag, u.birth)) {
                w.handles.alias(jass, h);
            }
            if let Some(unit) = w.unit_mut(h) {
                (unit.max_life, unit.max_mana) = (u.life[3], u.mana[3]);
                (unit.life, unit.mana) = (u.life[0].min(u.life[3]), u.mana[0].min(u.mana[3]));
            }
        }
        // Gold, lumber, and what was gathered of each (JASS's player states 1, 2, 15,
        // and 16), the rest as the script set them.
        for (p, states) in players.into_iter().enumerate() {
            let Some(player) = w.players.get_mut(p) else { continue };
            for (k, v) in states {
                if matches!(k, 1 | 2 | 15 | 16) {
                    player.state.insert(k as i32, v / 10);
                }
            }
        }
        if let Some((_, v)) = game_reals.iter().find(|(k, _)| *k == 2) {
            (w.time_of_day, w.time_of_day_exact, w.time_of_day_clock) = (v[0], v[0] as f64, w.time as f64);
        }
        // The map script's machine is the first; its values by type (the original's
        // machine's: 4 integer, 5 real, 8 boolean, and 9, 10, and 13 their arrays; a
        // real as its bits).
        if let Some((_, m)) = scripts.machines.first() {
            use w3_formats::w3z::Contents;
            use w3_jass::{BaseType, Value};
            let value = |kind: u32, v: u32| match kind {
                4 | 9 => Some(Value::Int(v as i32)),
                5 | 10 => Some(Value::Real(f32::from_bits(v))),
                8 | 13 => Some(Value::Bool(v != 0)),
                _ => None,
            };
            let vm = &mut self.vm;
            for (name, v) in &m.globals {
                let Some(&g) = vm.module.global_index.get(name.as_str()) else { continue };
                let decl = &vm.module.globals[g as usize];
                let base_matches = matches!((decl.base, v.kind), (BaseType::Integer, 4 | 9) | (BaseType::Real, 5 | 10) | (BaseType::Boolean, 8 | 13));
                if decl.constant || !base_matches {
                    continue;
                }
                match (&v.value, decl.array) {
                    (Contents::One(x), false) => {
                        if let Some(x) = value(v.kind, *x) {
                            vm.globals[g as usize] = x;
                        }
                    }
                    (Contents::Array(xs), true) => {
                        vm.global_arrays[g as usize] = xs.iter().filter_map(|&x| value(v.kind, x)).collect();
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }
}
