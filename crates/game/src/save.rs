//! Saved games, in the original's format (.w3z, its recorded-game container: the
//! port writes them with `Game::save_files`).  Loading starts the same map again with
//! the save's setup and puts the saved state in place of the fresh one
//! (`Game::restore_original`).

use crate::Game;
use crate::types::Obj;

/// A saved game file, read whole.
pub fn read(path: &std::path::Path) -> anyhow::Result<w3_formats::w3z::SaveGame> {
    parse(&std::fs::read(path)?)
}

/// A saved game's bytes, read whole.
pub fn parse(bytes: &[u8]) -> anyhow::Result<w3_formats::w3z::SaveGame> {
    Ok(w3_formats::w3z::SaveGame::read(bytes)?)
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
    /// timers.  The clocks go on from the save's (`World::carry_clocks_to`): its game
    /// time, the engine's main clock, and the movement steps' agent queue.
    /// The units and the walks given again take their saved slots and births
    /// (`tags::RestorePlan`), and the allocator is then the save's, as the original's
    /// loaded game has its object table.
    pub fn restore_original(&mut self, s: &w3_formats::w3z::SaveGame) -> anyhow::Result<()> {
        use crate::natives::units::{create_unit_exact, remove_unit};
        self.world.regions_reloaded = true;
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
        // (The clocks go on from the save's, as the original's loaded game's do.)
        w.carry_clocks_to(s.game_ms);
        let origin = w.pathing.as_ref().map_or([0.0; 2], |p| p.origin);
        // (The threads the map's start left asleep go, with their executions' objects,
        // the save's own asleep coming back below: read, Mafarazzo TD's Initialization
        // trigger's `+tex` at slot 1376, which the port's start had kept for its 20
        // seconds of sleeps where the save holds a walk's cluster, so that walk's cluster
        // was made elsewhere and its frees parted from the original's at turn 39.)
        for s in std::mem::take(&mut w.sleepers) {
            if s.execution != 0 && w.tags.live.contains_key(&s.execution) {
                w.tags.release(s.execution);
            }
        }
        w.executions.clear();
        // (The map's start's units go with their own objects, so the units made anew
        // take their slots: read, Mafarazzo TD's flying peasants, made at its start at
        // slots 1383 and 1406 in both games.  Their orders' objects stay, as others.)
        for h in w.unit_order.clone() {
            let objects: Vec<u32> = w.unit(h).map(|u| {
                let x = &u.extra;
                let live = |&(s, b): &(u32, u32)| w.tags.live.get(&s).is_some_and(|e| e.1 == b);
                let mut v: Vec<u32> = x.object_slots.iter().flatten().chain(&x.ai_slots).chain(&x.ensnare_pil).chain(&x.item_pil).filter(|o| live(o)).map(|o| o.0).collect();
                v.extend(x.flyer_pil.iter().chain(&x.blight_slots).chain(&x.footprint_slots));
                v.extend(x.detected_by.iter().map(|d| d.1).chain(x.mana_listeners.iter().map(|m| m.1)).chain(x.spell_pils.iter().map(|p| p.0)));
                v
            }).unwrap_or_default();
            remove_unit(w, h);
            for s in objects {
                if w.tags.live.contains_key(&s) {
                    w.tags.release(s);
                }
            }
        }
        w.tags.free_doomed();
        // (The units made, by their objects as saved: each unit and the objects made
        // with it at its saved slots and births, `tags::RestorePlan`.)
        let saved_table: Vec<w3_formats::w3z::Tagged> = s.objects.as_ref().map(|m| m.tables[0].clone()).unwrap_or_default();
        w.tags.plan = Some(crate::tags::RestorePlan { saved: saved_table.iter().map(|t| (t.birth, (t.tag, t.kind.to_be_bytes()))).collect(), base: None, next: 0 });
        // (What else the restore makes takes slots past the saved table's, the free list the
        // saved one once it is done.)
        w.tags.free.clear();
        let saved_free = s.objects.as_ref().map_or(&[][..], |m| &m.free[0][..]).chunks(4).map(|c| u32::from_le_bytes(c.try_into().unwrap())).take_while(|&v| v != u32::MAX).max().map_or(0, |f| f + 1);
        w.tags.slots = saved_table.iter().map(|t| t.tag + 1).max().unwrap_or(0).max(saved_free).max(w.tags.slots);
        let mut made: std::collections::HashMap<(u32, u32), u32> = Default::default();
        for u in units {
            let (x, y) = (origin[0] + u.cell[0] * crate::pathing::CELL, origin[1] + u.cell[1] * crate::pathing::CELL);
            if let Some(p) = w.tags.plan.as_mut() {
                (p.base, p.next) = (Some(u.birth), 0);
            }
            let h = create_unit_exact(w, u.owner as u8, u.type_id.to_be_bytes(), x, y, u.facing.to_degrees());
            if let Some(p) = w.tags.plan.as_mut() {
                p.base = None;
            }
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
        self.restore_boards(s, &handles)?;
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
                w.sleepers.push(crate::world::Sleeper { wake_at: w.sleep_time + left.max(0.0) + crate::world::SLEEP_SLACK, thread, ctx, sound: 0, execution: 0, sleep: *sleep, timer_count: 0, held: false });
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
            let mut orders: Vec<(u32, i32, f32, f32, (u32, u32), (u32, u32))> = Vec::new();
            for o in all.iter().flatten().filter(|o| &o.entry.kind.to_be_bytes() == crate::codes::class::UNIT) {
                let Some(&h) = made.get(&(o.entry.tag, o.entry.birth)) else { continue };
                let Ok(Second::Unit { fields, .. }) = o.second() else { continue };
                // Its detection's record (`Cdet`, its second and third words) at its saved
                // slot and birth, found by no player yet: the next detector to find it
                // takes it as the original's goes on with it (`World::detection_look`).
                if let (Some(dt), Some(db)) = (word(fields.get(1)), word(fields.get(2))) {
                    if by.get(&(dt, db)).is_some_and(|d| &d.entry.kind.to_be_bytes() == crate::codes::class::DETECTION) {
                        if let Some(p) = self.world.tags.plan.as_mut() {
                            (p.base, p.next) = (Some(db), 0);
                        }
                        let slot = self.world.tags.take(crate::codes::class::DETECTION);
                        if let Some(p) = self.world.tags.plan.as_mut() {
                            p.base = None;
                        }
                        if let Some(u) = self.world.unit_mut(h) {
                            u.extra.detected_by = vec![(u8::MAX, slot)];
                        }
                    }
                }
                let (Some(t), Some(b)) = (word(fields.get(14)), word(fields.get(15))) else { continue };
                let Some(ord) = by.get(&(t, b)) else { continue };
                let Ok(Second::Agent { own, .. }) = ord.second() else { continue };
                let Some(id) = word(own.get(1)) else { continue };
                let (x, y) = (real(own.get(6)).unwrap_or(0.0), real(own.get(7)).unwrap_or(0.0));
                let target = (word(own.get(8)).unwrap_or(u32::MAX), word(own.get(9)).unwrap_or(u32::MAX));
                orders.push((h, id as i32, x, y, target, (ord.entry.kind, b)));
            }
            for (h, id, x, y, target, (kind, birth)) in orders {
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
                // (An order of a player's kind, `ordt`, as a script or a player gives one
                // (`World::ordering_player`), its objects at the saved order's slots and
                // births by their places from its first, `tags::RestorePlan`: read, a
                // creep's walk's `ordt`, `tskA`, `tskO`, three `task`, `tsk.`, `^clu`, and
                // `l^pa` born 0, 2, 3, 4 to 6, 7, 15, and 16 after it, as
                // `Tags::player_move_issued` makes them.)
                let player = &kind.to_be_bytes() == crate::codes::class::ORDER_TASKS;
                let before = self.world.ordering_player;
                let ordering = std::mem::replace(&mut self.world.ordering_player, player || before);
                if let (true, Some(p)) = (player, self.world.tags.plan.as_mut()) {
                    (p.base, p.next) = (Some(birth), 0);
                }
                self.world.order_unit(h, id, x, y, target);
                if let Some(p) = self.world.tags.plan.as_mut() {
                    p.base = None;
                }
                self.world.ordering_player = ordering;
            }
        }
        // Each unit's place as saved, off the quarter-unit grid a unit is made on, and a
        // walker's motion (its clustered position's, `` `pcu ``, at its saved slot: its
        // place in cells, velocity, and heading, and its stamp, the main clock of its last
        // step; read, Mafarazzo TD's creeps at 103.959236 cells where made at 103.953125).
        {
            use w3_formats::w3z::First;
            let all = s.objects.as_ref().map(|m| m.objects()).transpose()?.unwrap_or_default();
            let by: std::collections::HashMap<(u32, u32), &w3_formats::w3z::SavedObject> = all[0].iter().map(|o| ((o.entry.tag, o.entry.birth), o)).collect();
            let w = &mut self.world;
            for h in w.unit_order.clone() {
                let Some(u) = w.unit_mut(h) else { continue };
                // (A detector's looks go on by its in-range object's timer, each look its
                // half second's on the agent queue (`World::detection_looks`, its clock
                // counted from the start of the tick its object was made in): the clock
                // set so its next falls at the timer's due, taken in the tick after it;
                // read, Mafarazzo TD's flying peasants' due 54.0 as saved at 53.8, the
                // loaded game's two detections at 55.0 after its keepalive there.)
                if let Some(Ok(First::Interference { object, .. })) = u.extra.flyer_pil.and_then(|p| all[0].iter().find(|o| o.entry.tag == p && &o.entry.kind.to_be_bytes() == crate::codes::class::IN_RANGE_LISTENER)).map(|o| o.first()) {
                    if let Some((due, ..)) = object.timings[0].0 {
                        let due_ms = (due as f64 * 1000.0).round() as i64;
                        u.extra.detect_clock = Some((s.game_ms as i64 - 100 - due_ms).rem_euclid(500) as f32);
                        u.extra.detect_ahead = None;
                    }
                }
                // (Its pathing off, SetUnitPathing's, where its own flags, its record's
                // first word (CUnit +0x20), lack 0x40; a structure's never have it.  Read:
                // Mafarazzo TD's creeps', 0x20000406, their walks' clusters' words a
                // poorer guide.)
                if !u.is_building {
                    if let Some(Ok(w3_formats::w3z::Second::Unit { own, .. })) = by.get(&u.extra.oid).map(|o| o.second()) {
                        u.extra.no_pathing = own & 0x40 == 0;
                    }
                }
                let Some(pcu) = u.extra.object_slots[0].get(1).copied() else { continue };
                let Some(Ok(First::Cluster { base, values, words, last, .. })) = by.get(&pcu).filter(|o| &o.entry.kind.to_be_bytes() == crate::codes::class::UNIT_POSITION).map(|o| o.first()) else { continue };
                (u.x, u.y) = (origin[0] + values[0] * crate::pathing::CELL, origin[1] + values[1] * crate::pathing::CELL);
                // (Its facing as saved, in radians, for its records: read, Mafarazzo TD's
                // towers' 4.7123885, made at 270 degrees.)
                u.extra.made_facing = Some((u.facing, values[5]));
                // (Its speed as set, SetUnitMoveSpeed's, in cells a second: one whose type
                // does not move holds none.)
                if values[4] < f32::MAX {
                    u.move_speed = values[4] * crate::pathing::CELL;
                }
                // A walker sent to a point goes on with its walk (`World::walk_steps_inner`):
                // its mover as saved, and its ways as its paths keep them, so it neither
                // plans nor searches anew.  Its own path (`l^pa`, the position's last) its
                // local way, from its local goal (first) to where the search set out, and
                // the node it steers for by its index (read: each of Mafarazzo TD's creeps'
                // at 0, its local goal), and its own coarse way and index (limit 400); its
                // walk's cluster (`^clu`, the position's first words) its path's coarse
                // way and index, the order's (limit 5000, read: 0 to 8 along).
                if u.extra.player_walk && matches!(u.action, crate::types::Action::Move { .. }) {
                    u.extra.mover = Some(crate::motion::Mover { x: values[0].to_bits(), y: values[1].to_bits(), vx: values[2].to_bits(), vy: values[3].to_bits(), heading: values[5].to_bits(), placed_at: base.real.to_bits() });
                    let path_of = |r: (u32, u32)| match by.get(&r).map(|o| o.first()) {
                        Some(Ok(First::Path { ways, .. })) => Some(ways),
                        _ => None,
                    };
                    let coarse_of = |(a, index, ..): &(w3_formats::w3z::Array<[f32; 2]>, u32, u32, u16)| {
                        (*index != u32::MAX && (*index as usize) < a.entries.len()).then(|| crate::accelpath::Coarse { points: a.entries.iter().map(|e| (e[0], e[1])).collect(), index: *index as usize, searched_at: 0 })
                    };
                    if let Some(ways) = path_of((last.0, last.1)) {
                        let (local, index, ..) = &ways[0];
                        if (*index as usize) < local.entries.len() {
                            u.path = local.entries[..=*index as usize].iter().rev().map(|e| (origin[0] + e[0] * crate::pathing::CELL, origin[1] + e[1] * crate::pathing::CELL)).collect();
                        }
                        u.extra.move_coarse = coarse_of(&ways[1]);
                    }
                    if let Some(Ok(First::Cluster2 { refs, .. })) = by.get(&(words[0], words[1])).map(|o| o.first()) {
                        u.extra.coarse = path_of((refs[2].0, refs[2].1)).and_then(|ways| coarse_of(&ways[1]));
                        // (Its place among the walks, which step and free their old paths
                        // newest first: the load links each cluster at the list's head as
                        // it comes in the table, so the highest slot's is the newest, all
                        // older than any walk ordered after; read, four creeps' old
                        // clusters freed in turn 5 of Mafarazzo TD's load, slot 3592's
                        // last, born between the others.)
                        u.extra.walk_serial = words[0];
                    }
                }
            }
        }
        // Who is inside each watched region, as the units stand: the game goes on with
        // them inside, no unit entering one it was in as it was saved (read: Mafarazzo
        // TD's creeps inside their lanes' turning regions, which the original's load
        // ordered on no further, its first tick's births five walks' and its turn 77's
        // 16 walks fewer than the port's whose walkers entered them anew).
        {
            let w = &mut self.world;
            let mut regions: Vec<u32> = w.region_inside.keys().copied().collect();
            regions.sort_unstable();
            for region in regions {
                let Some(r) = w.region(region) else { continue };
                let now: Vec<u32> = w.unit_order.iter().copied().filter(|&h| w.unit(h).is_some_and(|u| r.contains(u.x, u.y))).collect();
                w.region_inside.insert(region, now);
            }
        }
        // The computers' AI scripts, as they were (their threads' sleeps on the main
        // clock).
        let main_clock = s.objects.as_ref().map_or(0.0, |m| m.clocks[0].0);
        if let Err(e) = self.restore_ai_scripts(s, &scripts, main_clock, &made) {
            log::warn!("the AI scripts of the save: {e:#}");
        }
        // The players' tech registrations as saved, in their order: the units made again
        // counted no more (read: Mafarazzo TD's LAN load, its first keepalive's game
        // module the saved game's, each peasant made once).
        if let Ok(techs) = s.player_techs() {
            for (p, regs) in techs.into_iter().enumerate().filter(|(_, r)| !r.is_empty()) {
                if let Some(pl) = self.world.players.get_mut(p) {
                    pl.tech_regs = regs.into_iter().map(|r| crate::techreg::TechReg { id: r[0].to_be_bytes(), max: r[1] as i32, made: r[2] as i32, producing: r[3] as i32, changing: r[4] as i32, present: r[5] as i32, killed: r[6] as i32, flags: r[7] }).collect();
                }
            }
        }
        // Its generators its own, seeded as a game starts from the slot table's seed, no
        // map script drawing (the saves hold none of theirs: read, the load's first
        // keepalive's random data module a fresh one's).
        self.seed_generators();
        // The allocator as saved: its births, slots, and free list (those the port's
        // objects still hold kept out of it), then the load's own objects.
        self.world.tags.plan = None;
        if let Some(m) = s.objects.as_ref() {
            let births = m.header.get(24..28).map_or(0, |b| u32::from_le_bytes(b.try_into().unwrap()));
            let free: Vec<u32> = m.free[0].chunks(4).map(|c| u32::from_le_bytes(c.try_into().unwrap())).take_while(|&v| v != u32::MAX).collect();
            let t = &mut self.world.tags;
            t.slots = m.tables[0].iter().map(|e| e.tag + 1).chain(free.iter().map(|&f| f + 1)).max().unwrap_or(0).max(t.slots);
            t.born = births;
            t.free = free.into_iter().rev().filter(|f| !t.live.contains_key(f)).collect();
            let saved: std::collections::HashMap<u32, (u32, u32)> = m.tables[0].iter().map(|e| (e.tag, (e.kind, e.birth))).collect();
            t.strangers = t.live.iter().filter(|(slot, (code, birth))| saved.get(slot) != Some(&(u32::from_be_bytes(*code), *birth))).map(|(&slot, _)| slot).collect();
            // (The properties made again at their saved slots keep their saved stamps,
            // the main clock as each was made: read, Mafarazzo TD's creeps' reals at
            // 19.997644, made at 20 seconds, not the load's 53.783657.  Provisional: in
            // the main clock's wrap of now.)
            if let Ok(objects) = m.objects() {
                use w3_formats::w3z::First;
                for o in &objects[0] {
                    if t.live.get(&o.entry.tag) != Some(&(o.entry.kind.to_be_bytes(), o.entry.birth)) {
                        continue;
                    }
                    let stamp = match o.first() {
                        Ok(First::Real { base, .. } | First::Flag { base, .. } | First::Cluster { base, .. }) => base.real,
                        _ => continue,
                    };
                    if stamp == 0.0 {
                        t.made_at.remove(&o.entry.tag);
                    } else {
                        t.made_at.insert(o.entry.tag, stamp.to_bits());
                    }
                    t.made_wraps.remove(&o.entry.tag);
                }
            }
        }
        let saved = s.objects.as_ref().map(|m| m.tables[0].clone()).unwrap_or_default();
        self.loaded_objects(&saved);
        // (Each unit comes into its in-range search block as the save loads, in the
        // units' order, `World::note_block`.)
        for h in self.world.unit_order.clone() {
            if let Some(u) = self.world.unit_mut(h) {
                u.extra.block_entry = None;
            }
            self.world.note_block(h);
        }
        Ok(())
    }

    /// What the original makes and lets go as a saved game is loaded, the first born
    /// after the save's.  Each unit's spells' listeners are made anew, as its mana is
    /// set (`World::mana_changed`, in the units' order), their old ones going as the
    /// first tick begins, after the save's own event data (`gsed`), which goes then too
    /// (read: Mafarazzo TD's LAN load, its flying peasants' Repair listeners, threshold
    /// listeners on their mana at births 19013 and 19014 in the saved free list's first
    /// two slots, and the save's `gsed` and their old listeners, slots 9156, 1400, and
    /// 1423, freed 0.0001 after the first step).
    pub fn loaded_objects(&mut self, saved: &[w3_formats::w3z::Tagged]) {
        for e in saved.iter().filter(|e| &e.kind.to_be_bytes() == crate::codes::class::SAVE_EVENT_DATA) {
            self.world.tags.doom(e.tag);
        }
        for h in self.world.unit_order.clone() {
            self.world.mana_changed(h);
        }
    }

    /// An original's save's leaderboards, timer dialogs, and camera setups (made by the
    /// script as it ran, which the map's start run again to load does not make again),
    /// under the handles they had: a leaderboard's title, items (each its label, value,
    /// style, player, and colors, a player's own color left the player's), size, and
    /// whether displayed (`World::leaderboard_records`), and the players shown it (each
    /// `+ply`'s +0x2d4); a timer dialog's timer, title, and speed; a camera setup's
    /// place and values (its reals in the second table, `World::camera_setup_records`).
    fn restore_boards(&mut self, s: &w3_formats::w3z::SaveGame, handles: &std::collections::HashMap<(u32, u32), u32>) -> anyhow::Result<()> {
        use crate::types::{CameraSetup, Leaderboard, LeaderboardItem, Obj, TimerDialog};
        use w3_formats::w3z::{Field, First, Second};
        let all = s.objects.as_ref().map(|m| m.objects()).transpose()?.unwrap_or_default();
        let by: std::collections::HashMap<u32, &w3_formats::w3z::SavedObject> = all.iter().flatten().map(|o| (o.entry.tag, o)).collect();
        let word = |f: Option<&Field>| match f {
            Some(Field::Word(w)) => Some(*w),
            _ => None,
        };
        let real = |f: Option<&Field>| match f {
            Some(Field::Real(r)) => Some(*r),
            _ => None,
        };
        let text = |f: Option<&Field>| match f {
            Some(Field::Text(t)) => Some(t.clone()),
            _ => None,
        };
        let w = &mut self.world;
        let mut boards: std::collections::HashMap<(u32, u32), u32> = Default::default();
        for o in all.iter().flatten() {
            let Some(&jass) = handles.get(&(o.entry.tag, o.entry.birth)) else { continue };
            let Ok(Second::Agent { own, tail, .. }) = o.second() else { continue };
            match &o.entry.kind.to_be_bytes() {
                crate::codes::class::LEADERBOARD => {
                    let Some(Field::List(rows)) = tail.get(1) else { continue };
                    let mut b = Leaderboard { label: text(tail.get(5)).unwrap_or_default(), size: word(tail.get(3)).unwrap_or(0) as usize, displayed: word(tail.get(4)) == Some(1), ..Leaderboard::default() };
                    for r in rows {
                        let player = word(r.get(5)).unwrap_or(0) as u8;
                        let theirs = w.player_argb(player);
                        let own_color = |c: u32| {
                            let [a, r, g, bl] = c.to_be_bytes();
                            (c != theirs).then_some([r, g, bl, a])
                        };
                        let flag = |k: usize| word(r.get(k)) == Some(1);
                        b.items.push(LeaderboardItem { label: text(r.first()).unwrap_or_default(), value: word(r.get(1)).unwrap_or(0) as i32, style: [flag(2), flag(3), flag(4)], player, label_color: word(r.get(6)).and_then(own_color), value_color: word(r.get(7)).and_then(own_color) });
                    }
                    let h = w.handles.insert(Obj::Leaderboard(b));
                    w.handles.alias(jass, h);
                    w.tags.handle_slots.insert(h, o.entry.tag);
                    boards.insert((o.entry.tag, o.entry.birth), h);
                }
                crate::codes::class::TIMER_DIALOG => {
                    let timer = (word(own.get(1)).unwrap_or(u32::MAX), word(own.get(2)).unwrap_or(u32::MAX));
                    let Some(&t) = handles.get(&timer) else { continue };
                    let timer = w.handles.resolve(t);
                    let d = TimerDialog { timer, title: text(tail.get(3)), speed: real(tail.get(4)).unwrap_or(1.0), ..TimerDialog::default() };
                    let h = w.handles.insert(Obj::TimerDialog(d));
                    w.handles.alias(jass, h);
                    w.tags.handle_slots.insert(h, o.entry.tag);
                }
                crate::codes::class::CAMERA_SETUP => {
                    let mut c = CameraSetup { x: real(tail.first()).unwrap_or(0.0), y: real(tail.get(1)).unwrap_or(0.0), ..CameraSetup::default() };
                    if let Ok(First::Agent { refs, .. }) = o.first() {
                        let reals: Vec<f32> = refs.iter().filter_map(|r| by.get(&r.0)).filter_map(|p| match p.first() {
                            Ok(First::Real { values, .. }) => Some(values[0]),
                            _ => None,
                        }).collect();
                        for (k, &f) in crate::w3zsave::CAMERA_SETUP_FIELDS.iter().enumerate() {
                            if let Some(&v) = reals.get(k) {
                                c.fields.insert(f, v);
                            }
                            if let Some(&v) = reals.get(8 + k) {
                                c.durations.insert(f, v);
                            }
                        }
                    }
                    let h = w.handles.insert(Obj::CameraSetup(Box::new(c)));
                    w.handles.alias(jass, h);
                    w.tags.handle_slots.insert(h, o.entry.tag);
                }
                _ => {}
            }
        }
        // The players shown each board.
        for o in all.iter().flatten().filter(|o| &o.entry.kind.to_be_bytes() == crate::codes::class::PLAYER) {
            let Ok(Second::Agent { tail, .. }) = o.second() else { continue };
            let (Some(slot), Some(birth)) = (word(tail.get(139)), word(tail.get(140))) else { continue };
            let (Some(&b), Some(p)) = (boards.get(&(slot, birth)), w.tags.player_slots.iter().position(|&s| s == o.entry.tag)) else { continue };
            w.player_leaderboards.insert(p as u8, b);
        }
        Ok(())
    }
}
