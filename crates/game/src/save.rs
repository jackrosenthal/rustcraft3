//! Saved games: the world's state and the scripts' variables and threads.  Loading
//! starts the same map again (its object data, script, and strings) and puts the saved
//! state in place of the fresh one.

use serde::{Deserialize, Serialize};
use w3_jass::{State, Thread};

use crate::Game;
use crate::types::Obj;
use crate::world::World;

/// Bumped whenever the saved state's layout changes.
const FORMAT: u32 = 14;

/// A script machine's state.
type Variables = State;

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
            vm.places = self.vm.places.clone();
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

/// The setup a save of the original's was played with, for the map to start so before
/// it loads (`Game::restore_original`): its slot table's slots, each occupied one
/// played by its computer or, its local player's, by this computer (the rest empty),
/// with the race the game drew for it (the table keeps the choice: random), its name,
/// team, color, handicap, and computer level.
pub fn original_lobby(s: &w3_formats::w3z::SaveGame) -> Vec<crate::LobbySlot> {
    let races = s.player_races().unwrap_or_default();
    let names = s.player_names().unwrap_or_default();
    s.slot_list()
        .iter()
        .map(|&[_, _, status, computer, team, color, _, ai, handicap]| {
            let p = color;
            let fill = match (status, computer != 0) {
                (2, true) => crate::SlotFill::Computer,
                (2, false) if p as u32 == s.local_player => crate::SlotFill::Local,
                _ => crate::SlotFill::Empty,
            };
            crate::LobbySlot {
                player: p,
                fill,
                name: names.get(p as usize).filter(|n| !n.is_empty() && computer == 0).cloned(),
                race: races.get(p as usize).copied().filter(|r| (1..=4).contains(r)).map(|r| r as i32),
                team: team as i32,
                color: color as i32,
                handicap: handicap as f32,
                difficulty: ai as i32,
            }
        })
        .collect()
}

/// Where the port keeps its own state beside a saved game in the original's format:
/// `<save>.port`.
pub fn sidecar_path(path: &std::path::Path) -> std::path::PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(".port");
    std::path::PathBuf::from(s)
}

/// The original's file's length and CRC-32, with which its sidecar begins: a save
/// written over since (by the original, or another) is not taken for its sidecar's.
fn stamp(w3z: &[u8]) -> [u8; 8] {
    let mut crc = flate2::Crc::new();
    crc.update(w3z);
    let mut out = [0; 8];
    out[..4].copy_from_slice(&(w3z.len() as u32).to_le_bytes());
    out[4..].copy_from_slice(&crc.sum().to_le_bytes());
    out
}

/// The sidecar of a saved game in the original's format (`w3z`): its stamp, then the
/// port's own state (`Game::save`).
pub fn sidecar(w3z: &[u8], port: &[u8]) -> Vec<u8> {
    [&stamp(w3z)[..], port].concat()
}

/// The port's own state of a saved game file: its sidecar's, when it is this file's,
/// or the file itself in the port's format (as saves were before the original's
/// format).
pub fn port_state(path: &std::path::Path) -> Option<Vec<u8>> {
    let bytes = std::fs::read(path).ok()?;
    if !bytes.starts_with(w3_formats::container::MAGIC) {
        return Some(bytes);
    }
    let side = std::fs::read(sidecar_path(path)).ok()?;
    (side.len() > 8 && side[..8] == stamp(&bytes)).then(|| side[8..].to_vec())
}

impl Load {
    /// A saved game file: the port's own state when its sidecar is this file's, else
    /// the file as it reads.
    pub fn read(path: &std::path::Path) -> anyhow::Result<Load> {
        let bytes = std::fs::read(path)?;
        if bytes.starts_with(w3_formats::container::MAGIC) {
            if let Some(port) = port_state(path) {
                match SaveGame::parse(&port) {
                    Ok(s) => return Ok(Load::Port(s)),
                    Err(e) => log::warn!("{}: the port's state beside it does not read ({e:#}); loading the save alone", path.display()),
                }
            }
        }
        Load::parse(&bytes)
    }

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

/// A handle's object's kind as the original's handle table names it, for those the
/// port tells apart.
fn obj_kind(o: &Obj) -> Option<[u8; 4]> {
    use crate::codes::class;
    Some(*match o {
        Obj::Unit(_) => class::UNIT,
        Obj::Destructable(_) => class::DESTRUCTABLE,
        Obj::Item(_) => class::ITEM,
        Obj::Player(_) => class::PLAYER,
        Obj::Trigger(_) => class::TRIGGER,
        Obj::Timer(_) => class::TIMER,
        Obj::TimerDialog(_) => class::TIMER_DIALOG,
        Obj::Group(_) => class::GROUP,
        Obj::Location(..) => class::LOCATION,
        Obj::Rect(_) => class::RECT,
        Obj::Force(_) => class::FORCE,
        Obj::Sound(_) => class::SOUND,
        _ => return None,
    })
}

impl Game {
    /// Puts an original's saved game in place of a game just started on its map (its
    /// script's main run): its units (their type, owner, place, facing, life, and mana,
    /// a hero's experience, level, skill points, and skills),
    /// the handles scripts had them by, its players' states and alliances,
    /// the time of day, the map script's variables (by name, arrays too) and its threads
    /// asleep, its timers, its destructables' life, its items (those carried in their
    /// units' slots), its players' upgrades researched, the computers' AI scripts and
    /// captains (`restore_ai_scripts`), the mines' gold, the structures being built, the
    /// queues at work, each unit's order, the triggers turned off, and the timer events'
    /// timers.  The time played before (`World::played_before_ms`), as its saves and
    /// score show it; the clocks start again from 0, all the rest counting from them.
    pub fn restore_original(&mut self, s: &w3_formats::w3z::SaveGame) -> anyhow::Result<()> {
        use crate::natives::units::{create_unit_exact, remove_unit};
        let units = s.units()?;
        let handles: std::collections::HashMap<(u32, u32), u32> = s.handles()?.into_iter().map(|(r, h)| ((r.0, r.1), h)).collect();
        let players = s.player_states()?;
        let game_reals = s.game_reals()?;
        let timers = s.timers()?;
        let destructables = s.destructables()?;
        let items = s.items()?;
        let agent_clock = s.objects.as_ref().map_or(0.0, |m| m.clocks[1].0);
        let scripts = s.script_data()?;
        let w = &mut self.world;
        w.played_before_ms = s.game_ms;
        let origin = w.pathing.as_ref().map_or([0.0; 2], |p| p.origin);
        for h in w.unit_order.clone() {
            remove_unit(w, h);
        }
        // (The units made, by their objects as saved.)
        let mut made: std::collections::HashMap<(u32, u32), u32> = Default::default();
        for u in units {
            let (x, y) = (origin[0] + u.cell[0] * crate::pathing::CELL, origin[1] + u.cell[1] * crate::pathing::CELL);
            let h = create_unit_exact(w, u.owner as u8, u.type_id.to_be_bytes(), x, y, u.facing.to_degrees());
            made.insert((u.tag, u.birth), h);
            // The handle scripts had it by, as the save's handle table has it.
            if let Some(&jass) = handles.get(&(u.tag, u.birth)) {
                w.handles.alias(jass, h);
            }
            let time = w.time;
            if let Some(unit) = w.unit_mut(h) {
                (unit.max_life, unit.max_mana) = (u.life[3], u.mana[3]);
                use w3_formats::w3z::SavedUnit;
                (unit.life, unit.mana) = (SavedUnit::now(u.life, u.life_at, agent_clock), SavedUnit::now(u.mana, u.mana_at, agent_clock));
                // A corpse (none of its life left; read, the hero map's ghouls at 300
                // seconds, their owner 15, bones' with the top bit) lies dead, as
                // CreateCorpse makes one (provisional: its decay from now).
                if u.life[0] <= 0.0 {
                    unit.dead = true;
                    unit.life = 0.0;
                    unit.death_time = time;
                }
            }
        }
        // A hero's experience and skill points (its hero ability's first two words), its
        // level (its level flag's value, the first of the ability's two, less one), and
        // its skills' levels (each its ability's word at +0x50, the level less one, as
        // GetUnitAbilityLevel reads; read, the hero map's Dreadlord at 900 seconds: 633,
        // level 3, three skills at level 1); its life and mana as saved after.
        {
            use w3_formats::w3z::{Field, First, Second};
            let all = s.objects.as_ref().map(|m| m.objects()).transpose()?.unwrap_or_default();
            let by: std::collections::HashMap<(u32, u32), &w3_formats::w3z::SavedObject> = all.iter().flatten().map(|o| ((o.entry.tag, o.entry.birth), o)).collect();
            let word = |f: Option<&Field>| match f {
                Some(Field::Word(w)) => Some(*w),
                _ => None,
            };
            let mut heroes: Vec<u32> = Vec::new();
            for o in all.iter().flatten().filter(|o| o.entry.kind.to_be_bytes()[0] == b'A') {
                let Ok(First::Agent { object, refs, .. }) = o.first() else { continue };
                let Some(&h) = made.get(&(object.links[0].3 .0, object.links[0].3 .1)) else { continue };
                let Some((type_id, hero)) = w.unit(h).map(|u| (u.type_id, u.is_hero)) else { continue };
                if !hero {
                    continue;
                }
                let Ok(Second::Agent { own, tail, .. }) = o.second() else { continue };
                if &o.entry.kind.to_be_bytes() == crate::codes::ability::id::HERO {
                    let flag = |k: u32| refs.iter().find_map(|r| match by.get(&(r.0, r.1)).map(|p| p.first()) {
                        Some(Ok(First::Flag { base, value })) if base.object.words[0] == k => Some(value),
                        _ => None,
                    });
                    if let Some(u) = w.unit_mut(h) {
                        u.hero_xp = word(tail.first()).unwrap_or(0) as i32;
                        u.skill_points = word(tail.get(1)).unwrap_or(0) as i32;
                        if let Some(l) = flag(0) {
                            u.level = l as i32 + 1;
                        }
                    }
                    // (Its base strength, agility, and intelligence, without its items'
                    // or its levels' gains: the record's third and tenth words and its
                    // second flag; read, the hero map's Dreadlord at level 2, 20, 18,
                    // and 19, its totals 22, 19, and 21 as its 550 life, 3.7 armor,
                    // and 315 mana from them say.)
                    w.refresh_hero_attrs(h);
                    if let (Some(st), Some(ag), Some(it)) = (word(tail.get(2)), word(tail.get(9)), flag(1)) {
                        w.set_hero_base_attrs(h, [st as f32, ag as f32, it as f32]);
                    }
                    heroes.push(h);
                    continue;
                }
                // A skill learned (one of its type's hero abilities).
                let (Some(id), Some(level)) = (word(own.get(9)), word(own.get(10))) else { continue };
                let id = id.to_be_bytes();
                let skills = w.objects.units.get(type_id).map(|o| o.list("heroAbilList").iter().map(|x| crate::objects::id_from_str(x.trim())).collect::<Vec<_>>()).unwrap_or_default();
                if !skills.contains(&id) {
                    continue;
                }
                let level = level as i32 + 1;
                let had = w.unit(h).is_some_and(|u| u.abilities.iter().any(|(a, _)| *a == id));
                if let Some(u) = w.unit_mut(h) {
                    match u.abilities.iter_mut().find(|(a, _)| *a == id) {
                        Some(e) => e.1 = level,
                        None => u.abilities.push((id, level)),
                    }
                }
                if !had {
                    w.learned_objects(h, id);
                }
            }
            for h in heroes {
                let kept = w.unit(h).map(|u| (u.life, u.mana, u.max_life, u.max_mana));
                w.apply_hero_stats(h, false);
                if let (Some((life, mana, max_life, max_mana)), Some(u)) = (kept, w.unit_mut(h)) {
                    (u.life, u.mana, u.max_life, u.max_mana) = (life, mana, max_life, max_mana);
                }
            }
        }
        // Each gold mine's gold (its Gold Mine ability's first own field after the
        // ability's, `Agld`), and a haunted mine's (`Abgm`: its gold, then the mine it
        // covers, hidden under it as haunting leaves it; read, the hero map at 300
        // seconds, its mines' 11000 and 14000, its undead computer's haunted mine's 9760
        // over its mine), and an entangled one's so (`Aegm`, provisional).
        {
            use w3_formats::w3z::{Field, First, Second};
            use crate::codes::ability::id::{BLIGHTED_GOLD_MINE, ENTANGLED_GOLD_MINE, GOLD_MINE};
            let all = s.objects.as_ref().map(|m| m.objects()).transpose()?.unwrap_or_default();
            for o in all.iter().flatten() {
                let kind = o.entry.kind.to_be_bytes();
                if &kind != GOLD_MINE && &kind != BLIGHTED_GOLD_MINE && &kind != ENTANGLED_GOLD_MINE {
                    continue;
                }
                let Ok(First::Agent { object, .. }) = o.first() else { continue };
                let Some(&h) = made.get(&(object.links[0].3 .0, object.links[0].3 .1)) else { continue };
                let Ok(Second::Agent { own, .. }) = o.second() else { continue };
                let Some(&Field::Word(gold)) = own.get(13) else { continue };
                if &kind == BLIGHTED_GOLD_MINE || &kind == ENTANGLED_GOLD_MINE {
                    if let (Some(&Field::Word(t)), Some(&Field::Word(b))) = (own.get(14), own.get(15)) {
                        if let Some(&mine) = made.get(&(t, b)) {
                            w.cover_mine(h, mine);
                        }
                    }
                }
                if let Some(u) = w.unit_mut(h) {
                    u.resource = gold as i32;
                }
            }
        }
        // A structure being built (its Build In Progress, `ABnP`, the unit's ability, its
        // owner on its first list): the time left its real's (the ability's reference,
        // its build time falling 1 a second from its stamp on the agent clock; read, the
        // hero map's computer's two at 450 seconds, 60 and 50, 36.8 and 23.8 left).  Its
        // objects as a start makes them, stamped as long ago as it has been building,
        // for the port's own saves of it.  Provisional: its builder, still at work.
        {
            use w3_formats::w3z::First;
            let all = s.objects.as_ref().map(|m| m.objects()).transpose()?.unwrap_or_default();
            let by: std::collections::HashMap<(u32, u32), &w3_formats::w3z::SavedObject> = all.iter().flatten().map(|o| ((o.entry.tag, o.entry.birth), o)).collect();
            // (A stamp before the agent clock's last wrap at 300 seconds counts from
            // there: read, a research's at 279.76 with the clock at 1.21.)
            let since = |stamp: f32| {
                let mut e = agent_clock - stamp;
                while e < 0.0 {
                    e += 300.0;
                }
                e
            };
            // An ability's real (its first reference's): its first value and what it is
            // now (falling at its rate since its stamp).
            let real_of = |refs: &[w3_formats::w3z::Ref]| refs.iter().find_map(|r| match by.get(&(r.0, r.1)).map(|p| p.first()) {
                Some(Ok(First::Real { base, values })) => Some((values[0], values[0] + values[1] * since(base.real))),
                _ => None,
            });
            for o in all.iter().flatten().filter(|o| &o.entry.kind.to_be_bytes() == crate::codes::ability::id::BUILD_IN_PROGRESS) {
                let Ok(First::Agent { object, refs, .. }) = o.first() else { continue };
                let Some(&h) = made.get(&(object.links[0].3 .0, object.links[0].3 .1)) else { continue };
                let left = real_of(&refs).map(|(total, now)| (now, total));
                let Some((left, total)) = left else { continue };
                let left = left.clamp(0.0, total);
                let objects: Vec<u32> = [crate::codes::ability::id::BUILD_IN_PROGRESS, crate::codes::class::REAL, crate::codes::class::CONTINUOUS_MODIFIER, crate::codes::class::THRESHOLD_LISTENER, crate::codes::class::CONTINUOUS_MODIFIER].iter().map(|c| w.tags.take(c)).collect();
                let begun = crate::efloat::sub(w.main_clock_now().to_bits(), (total - left).to_bits());
                for &o in &objects {
                    w.tags.made_at.insert(o, begun);
                    w.tags.made_wraps.remove(&o);
                }
                if let Some(u) = w.unit_mut(h) {
                    u.build_time = total;
                    u.constructing = Some(left);
                    u.extra.build_objects = objects;
                    crate::sim::restart_anim(u, "birth");
                }
            }
            // A queue at work (its Queue, `Aque`: its seven places' ids after the
            // ability's fields and two reals and a word, the first's time its real's,
            // falling 1 a second from its time; read, the hero map's computer's
            // research of Rura at 900 seconds, 21.45 of 75 done): its ids, the time
            // done on the first, each begun in its owner's tech tree, the first's
            // objects as training takes them; its start's events not again.
            for o in all.iter().flatten().filter(|o| &o.entry.kind.to_be_bytes() == crate::codes::ability::id::QUEUE) {
                use w3_formats::w3z::{Field, Second};
                let Ok(First::Agent { object, refs, .. }) = o.first() else { continue };
                let Some(&h) = made.get(&(object.links[0].3 .0, object.links[0].3 .1)) else { continue };
                let Ok(Second::Agent { own, .. }) = o.second() else { continue };
                let Some(Field::List(places)) = own.iter().skip(13).find(|f| matches!(f, Field::List(_))) else { continue };
                let ids: Vec<crate::objects::Id> = places.iter().filter_map(|g| match g.first() {
                    Some(Field::Word(id)) if *id != 0 => Some(id.to_be_bytes()),
                    _ => None,
                }).collect();
                if ids.is_empty() {
                    continue;
                }
                let done = real_of(&refs).map_or(0.0, |(total, now)| (total - now).max(0.0));
                let Some(owner) = w.unit(h).map(|u| u.owner) else { continue };
                for &id in &ids {
                    w.tech_reg(owner, id).producing += 1;
                }
                w.tech_changed(owner);
                let rcm = w.tags.take(crate::codes::class::CONTINUOUS_MODIFIER);
                let rtl = w.tags.take(crate::codes::class::THRESHOLD_LISTENER);
                if let Some(u) = w.unit_mut(h) {
                    u.extra.queue_tokens = vec![false; ids.len()];
                    u.queue = ids;
                    u.queue_progress = done;
                    u.queue_started = true;
                    u.extra.training_tags = Some((rcm, rtl));
                }
            }
        }
        // The upgrades researched (each a player's `+Wup`, CPlayerUpgrade: its player,
        // the upgrade, 0, its level, and 3, found by the first two as the engine looks
        // them up, `FUN_6f6c9b40`, its level the fourth as `FUN_6f6c9b00` reads; read,
        // the hero map's undead computer's nine at 900 seconds, Unholy Strength at
        // level 1), their effects on the player's units as research gives them.
        // Provisional: the third and fifth words.
        {
            use w3_formats::w3z::{Field, Second};
            let all = s.objects.as_ref().map(|m| m.objects()).transpose()?.unwrap_or_default();
            let word = |f: Option<&Field>| match f {
                Some(Field::Word(w)) => Some(*w),
                _ => None,
            };
            let mut changed: Vec<u8> = Vec::new();
            for o in all.iter().flatten().filter(|o| &o.entry.kind.to_be_bytes() == crate::codes::class::PLAYER_UPGRADE) {
                let Ok(Second::Agent { tail, .. }) = o.second() else { continue };
                let (Some(p), Some(id), Some(level)) = (word(tail.first()), word(tail.get(1)), word(tail.get(3))) else { continue };
                let Some(player) = w.players.get_mut(p as usize) else { continue };
                if level > 0 {
                    player.tech_researched.insert(id.to_be_bytes(), level as i32);
                    changed.push(p as u8);
                }
            }
            changed.sort();
            changed.dedup();
            for p in changed {
                w.research_changed(p);
            }
        }
        // The players' states by their JASS numbers (gold, lumber, and what was
        // gathered of each, 1, 2, 15, and 16, in tenths), but those the game reckons
        // (hero tokens, food, and upkeep: 3, 4, 5, 13, and 14).
        for (p, states) in players.into_iter().enumerate() {
            let Some(player) = w.players.get_mut(p) else { continue };
            for (k, v) in states {
                match k {
                    1 | 2 | 15 | 16 => {
                        player.state.insert(k as i32, v / 10);
                    }
                    3 | 4 | 5 | 13 | 14 => {}
                    _ => {
                        player.state.insert(k as i32, v);
                    }
                }
            }
        }
        // The alliances (each player's `ally` object's flags: its player's number, then
        // each alliance type's mask of the players it is set toward), as the script
        // left them.
        {
            use w3_formats::w3z::First;
            let all = s.objects.as_ref().map(|m| m.objects()).transpose()?.unwrap_or_default();
            let by: std::collections::HashMap<(u32, u32), &w3_formats::w3z::SavedObject> = all.iter().flatten().map(|o| ((o.entry.tag, o.entry.birth), o)).collect();
            for o in all.iter().flatten().filter(|o| &o.entry.kind.to_be_bytes() == crate::codes::class::ALLIANCE) {
                let Ok(First::Agent { refs, .. }) = o.first() else { continue };
                let flags: Vec<(u32, u32)> = refs.iter().filter_map(|r| match by.get(&(r.0, r.1)).map(|p| p.first()) {
                    Some(Ok(First::Flag { base, value })) => Some((base.object.words[0], value)),
                    _ => None,
                }).collect();
                let Some(&(_, me)) = flags.iter().find(|f| f.0 == 0) else { continue };
                let Some(player) = w.players.get_mut(me as usize) else { continue };
                for &(k, mask) in flags.iter().filter(|f| f.0 >= 1 && (f.0 as usize) <= crate::types::ALLIANCE_TYPES) {
                    for (j, a) in player.alliances.iter_mut().enumerate() {
                        a[k as usize - 1] = mask & (1 << j) != 0;
                    }
                }
            }
        }
        // The items in place of the script's, under the handles they had; one a unit
        // carries in its inventory's slot (its Inventory's six, `AInv`, each an item's
        // tag and birth; its unit the ability's owner on its first list; read, the hero
        // map's heroes at 300 and 900 seconds).
        let mut carried: Vec<((u32, u32), (u32, u32), usize)> = Vec::new();
        {
            use w3_formats::w3z::{Field, First, Second};
            let all = s.objects.as_ref().map(|m| m.objects()).transpose()?.unwrap_or_default();
            for o in all.iter().flatten().filter(|o| &o.entry.kind.to_be_bytes() == crate::codes::ability::id::INVENTORY) {
                let Ok(First::Agent { object, .. }) = o.first() else { continue };
                let unit = (object.links[0].3 .0, object.links[0].3 .1);
                let Ok(Second::Agent { tail, .. }) = o.second() else { continue };
                let Some(Field::List(slots)) = tail.first() else { continue };
                for (k, g) in slots.iter().enumerate() {
                    if let (Some(Field::Word(t)), Some(Field::Word(b))) = (g.first(), g.get(1)) {
                        if *t != u32::MAX {
                            carried.push(((*t, *b), unit, k));
                        }
                    }
                }
            }
        }
        for h in w.item_order.clone() {
            w.remove_item(h);
        }
        let mut made_items: std::collections::HashMap<(u32, u32), u32> = Default::default();
        for i in items {
            let (x, y) = (origin[0] + i.pos[0] * crate::pathing::CELL, origin[1] + i.pos[1] * crate::pathing::CELL);
            let h = crate::natives::objects::create_item(w, i.type_id.to_be_bytes(), x, y);
            made_items.insert((i.tag, i.birth), h);
            if let Some(&jass) = handles.get(&(i.tag, i.birth)) {
                w.handles.alias(jass, h);
            }
            if let Some(Obj::Item(item)) = w.handles.get_mut(h) {
                (item.life, item.player) = (i.life, i.owner as u8);
            }
        }
        for (item, unit, slot) in carried {
            if let (Some(&ih), Some(&uh)) = (made_items.get(&item), made.get(&unit)) {
                w.carry_item(uh, ih, slot);
            }
        }
        // Each unit's abilities beyond its type's that a script gave it (an ability object
        // it owns, on its first list, that its making here gave it none of: read, Echo
        // Isles' town hall's Militia Conversion, which the melee setup adds), at their
        // levels (the ability's word at +0x50, the level less one); not an item's, which
        // came with the item (an item ability, or one its carried items give: read, the
        // hero map's Death Knight's Sentry Ward).
        {
            use w3_formats::w3z::{Field, First, Second};
            let all = s.objects.as_ref().map(|m| m.objects()).transpose()?.unwrap_or_default();
            for o in all.iter().flatten().filter(|o| o.entry.kind.to_be_bytes()[0] == b'A') {
                // (Its object's kind the ability's code; its id, which a map's own may
                // build on the code, its own tenth word, as a skill's.)
                let code = o.entry.kind.to_be_bytes();
                let Ok(First::Agent { object, .. }) = o.first() else { continue };
                let Some(&h) = made.get(&(object.links[0].3 .0, object.links[0].3 .1)) else { continue };
                let Ok(Second::Agent { own, .. }) = o.second() else { continue };
                let (Some(&Field::Word(id)), Some(&Field::Word(level))) = (own.get(9), own.get(10)) else { continue };
                let id = id.to_be_bytes();
                if w.objects.abilities.get(id).is_none_or(|a| a.int("item") != 0) {
                    continue;
                }
                let Some(u) = w.unit(h) else { continue };
                let carried = u.inventory.iter().filter_map(|&i| w.item(i)).any(|it| w.item_abilities(it.type_id).iter().any(|a| *a == id));
                let has = carried || u.abilities.iter().any(|(a, _)| *a == id) || u.extra.object_slots.iter().flatten().any(|&(s, _)| w.tags.live.get(&s).is_some_and(|e| e.0 == code));
                if has || !w.add_unit_ability(h, id) {
                    continue;
                }
                let level = level as i32 + 1;
                if let Some(e) = w.unit_mut(h).and_then(|u| u.abilities.iter_mut().find(|(a, _)| *a == id)) {
                    e.1 = level.max(1);
                }
            }
        }
        // The destructables' life, by the handles they had (the map placed them in the
        // same order), those at none dead.
        for d in destructables {
            let Some(&jass) = handles.get(&(d.tag, d.birth)) else { continue };
            let id = w.handles.resolve(jass);
            if matches!(w.handles.get(id), Some(Obj::Destructable(x)) if x.type_id == d.type_id.to_be_bytes()) {
                crate::natives::objects::set_saved_destructable_life(w, id, d.life);
            }
        }
        // The script's timers, by the handles they had (the script made them in the
        // same order): running ones with the time left to their node's due on the main
        // clock (in its current wrap: read, the original's 320-second timer fired as its
        // main clock read 20.0 in its second wrap), and for one in its 120-second chunks
        // (flag 0x10) the chunks left after it and the rest; its run's length its
        // chunks' and the rest, or its node's period; repeating by its flag 2.
        let main_clock = s.objects.as_ref().map_or(0.0, |m| m.clocks[0].0);
        let (clock_now, clock_wraps) = (f32::from_bits(w.tags.clock) as f64, w.tags.clock_wraps);
        for t in timers {
            let Some(&jass) = handles.get(&(t.tag, t.birth)) else { continue };
            let id = w.handles.resolve(jass);
            let Some(Obj::Timer(timer)) = w.handles.get_mut(id) else { continue };
            timer.timeout = t.timeout;
            match t.run {
                Some((due, length, node_periodic)) => {
                    let mut left = due - main_clock;
                    if left < -1.0 {
                        left += 300.0;
                    }
                    let left = left.max(0.0) as f64;
                    let chunked = t.flags & 0x10 != 0;
                    let (left, run) = if chunked {
                        (left + 120.0 * t.chunks.1.saturating_sub(1) as f64 + t.rest as f64, 120.0 * t.chunks.0 as f64 + t.rest as f64)
                    } else if t.chunks.0 > 0 {
                        (left, 120.0 * t.chunks.0 as f64 + t.rest as f64)
                    } else {
                        (left, length as f64)
                    };
                    let periodic = t.flags & 2 != 0 || (node_periodic && !chunked);
                    (timer.running, timer.periodic, timer.remaining, timer.period) = (true, periodic, left, run);
                    // (Its run begun its length before its expiry, on the port's clock,
                    // as a save reckons its node from.)
                    (timer.started_clock, timer.started_wraps) = (((clock_now + left - run) as f32).to_bits(), clock_wraps);
                }
                None => timer.running = false,
            }
        }
        if let Some((_, v)) = game_reals.iter().find(|(k, _)| *k == 2) {
            (w.time_of_day, w.time_of_day_exact, w.time_of_day_clock) = (v[0], v[0] as f64, w.time as f64);
        }
        // The map script's machine is the first: its globals as they were (its strings
        // by their text; a handle kept where the port holds an object of the kind the
        // save's handle table names, else none; its constants as the script makes them).
        if let Some((_, m)) = scripts.machines.first() {
            use w3_jass::vm::ty;
            let all = s.objects.as_ref().map(|m| m.objects()).transpose()?.unwrap_or_default();
            let kind_of: std::collections::HashMap<(u32, u32), [u8; 4]> = all.iter().flatten().map(|o| ((o.entry.tag, o.entry.birth), o.entry.kind.to_be_bytes())).collect();
            let saved_kind: std::collections::HashMap<u32, [u8; 4]> = s.handles()?.into_iter().filter_map(|(r, h)| kind_of.get(&(r.0, r.1)).map(|k| (h, *k))).collect();
            let w = &self.world;
            let handle_ok = |h: u32| -> bool {
                if h == 0 {
                    return true;
                }
                let Some(o) = w.handles.get(w.handles.resolve(h)) else { return false };
                match (saved_kind.get(&h), obj_kind(o)) {
                    (Some(a), Some(b)) => *a == b,
                    _ => true,
                }
            };
            let map = crate::w3zscript::Remap { strings: &m.functions };
            let mut kept: Vec<(u32, w3_jass::vm::Var)> = Vec::new();
            for (name, v) in &m.globals {
                let Some(&g) = self.vm.module.global_index.get(name.as_str()) else { continue };
                if self.vm.module.globals[g as usize].constant {
                    continue;
                }
                let mut var = map.var(&mut self.vm, v, false);
                if var.decl == ty::HANDLE {
                    if !handle_ok(var.value) {
                        var.value = 0;
                    }
                } else if var.decl == ty::HANDLE_ARRAY {
                    for x in var.array.iter_mut().flatten() {
                        if !handle_ok(*x) {
                            *x = 0;
                        }
                    }
                }
                kept.push((g, var));
            }
            for (g, var) in kept {
                if let Some(slot) = self.vm.vars.get_mut(g as usize) {
                    *slot = var;
                }
            }
        }
        // The map script's threads asleep (TriggerSleepAction, flag 1), each by its
        // execution's object (`+tex`, CTriggerExecution vf14): its context's slots (0
        // the triggering unit, 1 its widget, 2 the trigger, 3 the player, 6 the event's
        // id: the event natives' indices, `FUN_6f2892d0`), its wake timer on the agent
        // clock, and its place (read: a trigger waiting 30 seconds from 50, saved at
        // 60).  Provisional: the event's own data (slots 4 and 5: an order's, a spell's,
        // damage), and a sleep past the clock's wrap.
        if let (Some((_, m)), Some(threads)) = (scripts.machines.first(), scripts.threads.first()) {
            use w3_formats::w3z::{Field, Second, Timing};
            let all = s.objects.as_ref().map(|m| m.objects()).transpose()?.unwrap_or_default();
            let mut execs: std::collections::HashMap<u32, (f32, f32, Vec<u32>)> = Default::default();
            for o in all.iter().flatten().filter(|o| &o.entry.kind.to_be_bytes() == crate::codes::class::TRIGGER_EXECUTION) {
                let Ok(Second::Agent { tail, .. }) = o.second() else { continue };
                let words: Vec<u32> = match tail.first() {
                    Some(Field::List(g)) => g.iter().filter_map(|g| match g.first() {
                        Some(Field::Word(w)) => Some(*w),
                        _ => None,
                    }).collect(),
                    _ => Vec::new(),
                };
                let (Some(Field::Timer(_, _, Timing(Some(t)))), Some(Field::Word(place))) = (tail.get(1), tail.get(5)) else { continue };
                execs.insert(*place, (t.0, t.1, words));
            }
            let map = crate::w3zscript::Remap { strings: &m.functions };
            for t in threads.iter().filter(|t| t.flag == 1) {
                let Some((due, sleep, words)) = execs.get(&t.machine) else { continue };
                let thread = crate::w3zscript::thread_from(&mut self.vm, t, &map);
                let w = &mut self.world;
                let word = |k: usize| words.get(k).copied().unwrap_or(0);
                let res = |w: &crate::world::World, h: u32| if h == 0 { 0 } else { w.handles.resolve(h) };
                let player = match w.handles.get(res(w, word(3))) {
                    Some(Obj::Player(p)) => Some(*p),
                    _ => None,
                };
                let ctx = crate::types::EventCtx { trigger: res(w, word(2)), unit: res(w, word(0)), widget: res(w, word(1)), event_id: word(6) as i32, player: player.unwrap_or(0), has_player: player.is_some(), ..Default::default() };
                let mut left = due - agent_clock;
                while left > sleep + 1.0 {
                    left -= 300.0;
                }
                if left < -1.0 {
                    left += 300.0;
                }
                // (Its place taken in the places all machines share.)
                {
                    let mut places = self.vm.places.borrow_mut();
                    places.free.retain(|&p| p != thread.place);
                    places.len = places.len.max(thread.place + 1);
                }
                w.sleepers.push(crate::world::Sleeper { wake_at: w.sleep_time + left.max(0.0) + crate::world::SLEEP_SLACK, thread, ctx, sound: 0, execution: 0, sleep: *sleep, timer_count: 0 });
            }
        }
        // Each trigger's enabled flag (its second, kind 1; read, a trigger the original
        // disabled as it ran, 0), by the handle it had: one turned off as it ran stays
        // off though the map's start made it anew.
        {
            use w3_formats::w3z::First;
            let all = s.objects.as_ref().map(|m| m.objects()).transpose()?.unwrap_or_default();
            let by: std::collections::HashMap<(u32, u32), &w3_formats::w3z::SavedObject> = all.iter().flatten().map(|o| ((o.entry.tag, o.entry.birth), o)).collect();
            for o in all.iter().flatten().filter(|o| &o.entry.kind.to_be_bytes() == crate::codes::class::TRIGGER) {
                let Some(&jass) = handles.get(&(o.entry.tag, o.entry.birth)) else { continue };
                let Ok(First::Agent { refs, .. }) = o.first() else { continue };
                let enabled = refs.iter().find_map(|r| match by.get(&(r.0, r.1)).map(|p| p.first()) {
                    Some(Ok(First::Flag { base, value })) if base.object.words[0] == 1 => Some(value != 0),
                    _ => None,
                });
                let w = &mut self.world;
                let id = w.handles.resolve(jass);
                if let (Some(enabled), Some(Obj::Trigger(t))) = (enabled, w.handles.get_mut(id)) {
                    t.enabled = enabled;
                }
            }
        }
        // Each timer event's registration (`tmvt`, TriggerRegisterTimerEvent's, by the
        // event handle it had): its timer as saved, on the main clock (kind 0): running
        // to its due, or stopped once it has fired, so that the map's start, run again
        // to load, fires none twice (read: the original's at 40 seconds, due 50, and at
        // 60, stopped).
        {
            use w3_formats::w3z::{Field, Second, Timing};
            let main_clock = s.objects.as_ref().map_or(0.0, |m| m.clocks[0].0);
            let all = s.objects.as_ref().map(|m| m.objects()).transpose()?.unwrap_or_default();
            for o in all.iter().flatten().filter(|o| &o.entry.kind.to_be_bytes() == crate::codes::class::TIMER_EVENT) {
                let Some(&jass) = handles.get(&(o.entry.tag, o.entry.birth)) else { continue };
                let Ok(Second::Agent { tail, .. }) = o.second() else { continue };
                let Some(Field::Timer(_, _, Timing(timing))) = tail.first() else { continue };
                let w = &mut self.world;
                let Some(Obj::Event(r)) = w.handles.get(w.handles.resolve(jass)) else { continue };
                let crate::types::EventKind::TimerExpire(timer) = r.kind else { continue };
                let Some(Obj::Timer(t)) = w.handles.get_mut(timer) else { continue };
                match timing {
                    Some((due, period, _, _, rep)) => {
                        let mut left = due - main_clock;
                        while left < -1.0 {
                            left += 300.0;
                        }
                        (t.running, t.remaining, t.period, t.periodic) = (true, left.max(0.0) as f64, *period as f64, *rep != 0);
                    }
                    None => t.running = false,
                }
            }
        }
        // Each unit's order (its own record's fifteenth and sixteenth fields name it: an
        // order's id, `+ord`; one with a target or a point, `ordt`, its point and its
        // target's tag and birth after; read, the hero map's at 900 seconds, workers
        // harvesting their mine and trees, a captain's units moving), given again as
        // the unit's order without its events; one the engine names none of, none.
        // Provisional: a target not a unit or a destructable.
        {
            use w3_formats::w3z::{Field, Second};
            let all = s.objects.as_ref().map(|m| m.objects()).transpose()?.unwrap_or_default();
            let by: std::collections::HashMap<(u32, u32), &w3_formats::w3z::SavedObject> = all.iter().flatten().map(|o| ((o.entry.tag, o.entry.birth), o)).collect();
            let word = |f: Option<&Field>| match f {
                Some(Field::Word(w)) => Some(*w),
                _ => None,
            };
            let real = |f: Option<&Field>| match f {
                Some(Field::Real(r)) => Some(*r),
                _ => None,
            };
            let mut orders: Vec<(u32, i32, f32, f32, (u32, u32))> = Vec::new();
            for o in all.iter().flatten().filter(|o| &o.entry.kind.to_be_bytes() == crate::codes::class::UNIT) {
                let Some(&h) = made.get(&(o.entry.tag, o.entry.birth)) else { continue };
                let Ok(Second::Unit { fields, .. }) = o.second() else { continue };
                let (Some(t), Some(b)) = (word(fields.get(14)), word(fields.get(15))) else { continue };
                let Some(ord) = by.get(&(t, b)) else { continue };
                let Ok(Second::Agent { own, .. }) = ord.second() else { continue };
                let Some(id) = word(own.get(1)) else { continue };
                let (x, y) = (real(own.get(6)).unwrap_or(0.0), real(own.get(7)).unwrap_or(0.0));
                let target = (word(own.get(8)).unwrap_or(u32::MAX), word(own.get(9)).unwrap_or(u32::MAX));
                orders.push((h, id as i32, x, y, target));
            }
            for (h, id, x, y, target) in orders {
                if crate::natives::basic::order_name(id).is_none_or(|n| n == "stop") {
                    continue;
                }
                let target = if target.0 == u32::MAX {
                    0
                } else if let Some(&u) = made.get(&target) {
                    u
                } else if let Some(&jass) = handles.get(&target) {
                    self.world.handles.resolve(jass)
                } else {
                    continue;
                };
                self.world.order_unit(h, id, x, y, target);
            }
        }
        // The computers' AI scripts, as they were (their threads' sleeps on the main
        // clock).
        let main_clock = s.objects.as_ref().map_or(0.0, |m| m.clocks[0].0);
        if let Err(e) = self.restore_ai_scripts(s, &scripts, main_clock, &made) {
            log::warn!("the AI scripts of the save: {e:#}");
        }
        Ok(())
    }
}
