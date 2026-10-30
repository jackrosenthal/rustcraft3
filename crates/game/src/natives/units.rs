//! Units and unit groups.

use crate::collections::HashMap;

use w3_jass::{NativeResult, Value, Vm};

use super::{Args, boolean, handle, int, none, real, reg, string};
use crate::event_ids::*;
use crate::natives::players::is_ally;
use crate::objects::{Id, id_from_int, id_to_int};
use crate::types::*;
use crate::world::{NativeFn, World};

/// Creates a unit of a type for a player, returning its handle (0 if the type is unknown).
/// Where a new unit stands: a structure snapped to the build grid, and if destructables
/// stand there, the nearest place on the grid clear of them; a walker among
/// destructables, the nearest pathing cell's middle where it fits clear of them
/// (measured: farms made at 3700, 2700 and 1301, 1301 stand at 3648, 2688 and 1280,
/// 1280, forges at 1728, 1216 and 1760, 1600 at 1760, 1248 and 1760, 1632, all as
/// snapped; barracks made among trees at 3400, 2400 stand clear of them, at 3776, 2112
/// there, where the search here finds 3200, 2560; a footman among them moves 76 out).
/// The cells of one ring of the placement search, in the order the original's walks it
/// (FUN_6f14b070 and its three turns): the ring is ymin, xmin, ymax, xmax (the maxima
/// past its last cells).
fn ring_order(mode: i32, r0: i32, r1: i32, r2: i32, r3: i32) -> Vec<(i32, i32)> {
    let top: Vec<(i32, i32)> = (r1..=r3 - 2).rev().map(|x| (x, r2 - 1)).collect();
    let left: Vec<(i32, i32)> = (r0..=r2 - 2).rev().map(|y| (r1, y)).collect();
    let bottom: Vec<(i32, i32)> = (r1 + 1..=r3 - 1).map(|x| (x, r0)).collect();
    let right: Vec<(i32, i32)> = (r0 + 1..=r2 - 1).map(|y| (r3 - 1, y)).collect();
    let sides = [top, left, bottom, right];
    let start = mode.rem_euclid(4) as usize;
    (0..4).flat_map(|i| sides[(start + i) % 4].clone()).collect()
}

pub(crate) fn placed_at(w: &mut World, type_id: Id, x: f32, y: f32, facing: f32) -> (f32, f32) {
    let Some(o) = w.objects.units.get(type_id) else { return (x, y) };
    if w.pathing.is_none() {
        return (x, y);
    }
    // A structure by isbldg, ancients that walk among them (measured: an Ancient of
    // War made on unbuildable ground stands 181 away).
    let building = !o.str("pathTex").is_empty() && (o.real("spd") <= 0.0 || o.int("isbldg") == 1);
    let (movetp, collision, tex) = (o.str("movetp"), o.real("collision"), o.str("pathTex"));
    if building {
        if w.builds_on(type_id) {
            return (x, y);
        }
        let (sx, sy) = w.snap_placement(type_id, x, y);
        // One others are built on (canBuildOn: a gold mine) stands where asked
        // (measured: gold mines made for Neutral Passive and Player 1 on wholly
        // unbuildable ground stay there, where farms are moved; NightElfX08's, whose
        // footprint's corner is unbuildable, stays at its 6976, 6016 where ours stood
        // 1900 away, its slaves mining nothing).
        if w.objects.units.get(type_id).is_some_and(|o| o.int("canBuildOn") == 1) {
            return (sx, sy);
        }
        // Destructables in the way move it, and ground its preventPlace forbids
        // (measured: a farm, preventPlace unbuildable, made on unbuildable ground stands
        // 770 away; Human01's city buildings, with none, stay on theirs), not other
        // structures.
        let prevent = w.objects.units.get(type_id).map(|o| o.str("preventPlace").to_ascii_lowercase()).unwrap_or_default();
        let mut forbid = 0u8;
        // One that must stand on blight makes its own, and its ground's buildability
        // goes unasked (measured: a Ziggurat and a Crypt made where unbuildable cells
        // lie under their footprints stand there, at 992 and 960 for 1000, where a farm
        // and an Ancient Protector move off).
        let blight = w.objects.units.get(type_id).is_some_and(|o| o.str("requirePlace").to_ascii_lowercase().split(',').any(|t| t.trim() == "blighted"));
        for t in prevent.split(',').map(str::trim) {
            match t {
                "unbuildable" if blight => {}
                "unbuildable" => forbid |= crate::pathing::NO_BUILD,
                "unwalkable" => forbid |= crate::pathing::NO_WALK,
                _ => {}
            }
        }
        // Destructables by its walking footprint (red), the ground by its whole
        // footprint (blue; measured: an Ancient of War stands where none of its 12x12
        // cells is unbuildable).  Other structures don't count yet: the original moves
        // a town hall made on a start location by them, where to is not yet known.
        // A structure that takes gold in (Artn with DataA: a Town Hall's or Great Hall's
        // Argl) stands at least 768 from a gold mine (measured: one asked for 512 to 704
        // north of a mine is moved 768 away, a barracks not; one west of a mine lands 781
        // off; a Tree of Life or Eternity and a Necropolis, taking lumber alone (Arlm),
        // stay where asked 448 and 512 from one, their footprints clear of its, as
        // Orc04's trees do 590 from their mines).
        let abil_list = w.objects.units.get(type_id).map(|o| o.list("abilList")).unwrap_or_default();
        let hall = abil_list
            .iter()
            .filter_map(|a| w.objects.abilities.get(crate::objects::id_from_str(a.trim())))
            .any(|a| a.str("code") == "Artn" && a.level_real("DataA", 1) != 0.0);
        let mines: Vec<(f32, f32)> = if hall {
            w.unit_order.iter().filter_map(|&h| w.unit(h)).filter(|u| !u.dead && w.ability_with_code(u.handle, "Agld").is_some()).map(|u| (u.x, u.y)).collect()
        } else {
            Vec::new()
        };
        let clear = |w: &mut World, px: f32, py: f32| {
            if mines.iter().any(|&(mx, my)| (mx - px).hypot(my - py) < 768.0) {
                return false;
            }
            let whole = w.build_cells(&tex, px, py, 0.0);
            let p = w.pathing.as_ref().unwrap();
            whole.iter().all(|&c| p.clear_of_destructables(c) && p.index_pub(c).is_some_and(|i| p.flags[i] & forbid == 0) && (forbid & crate::pathing::NO_BUILD == 0 || !p.under_structure(c)))
        };
        if clear(w, sx, sy) {
            return (sx, sy);
        }
        // The search turns with the facing asked for, a quarter turn a side (measured:
        // structures made facing 0 around a stand of trees and facing 270 around a gold
        // mine land where the original's do, each walk only with its own facing).
        let mode = ((facing / 90.0).round() as i32).rem_euclid(4);
        let (cx, cy) = ((x / 32.0).floor() as i32, (y / 32.0).floor() as i32);
        for k in 1..=64i32 {
            let (r0, r1, r2, r3) = (cy - k, cx - k, cy + 1 + k, cx + 1 + k);
            for (px, py) in ring_order(mode, r0, r1, r2, r3) {
                let (ax, ay) = w.snap_placement(type_id, px as f32 * 32.0 + 16.0, py as f32 * 32.0 + 16.0);
                if clear(w, ax, ay) {
                    return w.clamp_to_map(ax, ay);
                }
            }
        }
        return (sx, sy);
    }
    ground_place(w, &movetp, collision, x, y, true, None, 0x02)
}

/// A unit shown again where it stood (out of a gold mine; `FUN_6f699c00`'s placing): as
/// one made there, without the room its size class asks for about it (read: peons out
/// of a gold mine beside its footprint stay there, and one whose place another stood on
/// came out at the next cell but one, by the mine too).
/// Kept clear of the units whose regions its mask meets (`Unit::region_mask`).
pub(crate) fn shown_at(w: &World, h: u32) -> (f32, f32) {
    let Some(u) = w.unit(h) else { return (0.0, 0.0) };
    let Some(o) = w.objects.units.get(u.type_id) else { return (u.x, u.y) };
    if w.pathing.is_none() {
        return (u.x, u.y);
    }
    let (movetp, collision) = (o.str("movetp"), o.real("collision"));
    ground_place(w, &movetp, collision, u.x, u.y, false, Some(h), u.region_mask())
}

/// A ground unit's place at or near x, y (`placed_at`); `room`: the cells about it must
/// be walkable as far as its size class reaches; `except`: a unit not to keep clear of;
/// `mask`: the region flags of the units it keeps clear of (`Unit::region_flags`).
#[allow(clippy::too_many_arguments)]
fn ground_place(w: &World, movetp: &str, collision: f32, x: f32, y: f32, room_about: bool, except: Option<u32>, mask: u32) -> (f32, f32) {
    let kind = crate::pathing::Move::from_movetp(movetp);
    let radius = collision * 0.5;
    if kind == crate::pathing::Move::Fly || radius <= 0.0 {
        return (x, y);
    }
    // One made too near another ground unit stands at the first pathing cell's middle
    // clear of them, searched ring by ring outward from its cell: each ring's bottom
    // row left to right, its right column upward, its top row right to left, and its
    // left column downward (FUN_6f170080 walks FUN_6f14a1e0's rings this way for every
    // unit, whatever its facing).  A cell is clear of a unit when their cells are more
    // than the two sizes over a cell apart in either axis (measured: peasants and
    // footmen, 16 and 31, made on one spot stand 2 cells apart, knights, 32, 3, and meat
    // wagons, 48, 4, the second each time at the ring's first cell; Human01's bandits
    // made 2 cells apart keep their places).
    let others: Vec<(f32, f32, f32)> = w.unit_order.iter().filter(|&&o| Some(o) != except).filter_map(|&o| w.unit(o)).filter(|u| u.region_flags() & mask != 0).filter(|u| !u.dead && !u.hidden && !u.is_building && u.collision > 0.0 && (u.x - x).abs() < 1024.0 && (u.y - y).abs() < 1024.0).filter(|u| w.move_kind(u.handle) != crate::pathing::Move::Fly).map(|u| (u.x, u.y, u.collision)).collect();
    let p = w.pathing.as_ref().unwrap();
    let near: Vec<((i32, i32), i32)> = others.iter().map(|&(ox, oy, oc)| (p.cell_of(ox, oy), ((collision + oc) / crate::pathing::CELL).floor() as i32)).collect();
    let apart = |c: (i32, i32)| near.iter().all(|&(o, t)| (c.0 - o.0).abs().max((c.1 - o.1).abs()) > t);
    // Destructables under the cells it covers beyond its own move it too (measured: a
    // footman made at 500,1600 among trees stands at 464,1520), the ground's own
    // pathing only once it is moved (the original leaves units a map stands on ground
    // it marks unwalkable).
    let span = ((radius - 16.0) / 32.0).ceil().max(0.0) as i32;
    let open = |c: (i32, i32)| (-span..=span).all(|ey| (-span..=span).all(|ex| p.clear_of_destructables((c.0 + ex, c.1 + ey)) || p.index_pub((c.0 + ex, c.1 + ey)).is_none()));
    let (cx, cy) = p.cell_of(x, y);
    // On walkable ground the cells about it must be walkable too, as far as its size
    // class reaches (FUN_6f170080 asks FUN_6f149370 with the class: collision in cells
    // below 0.5, below 1, below 1.5, or more; measured with sheep, collision 16, made
    // above unwalkable ground in Booty Bay: in the next cell row they move up to the
    // row above, one row higher they stay; the fifth pig so stands at -5456,-4432).
    // One asked on ground a map marks unwalkable stays.
    let class = {
        let c = collision / crate::pathing::CELL;
        if c < 0.5 { 0.0 } else if c < 1.0 { 1.0 } else if c < 1.5 { 2.0 } else { 3.0 }
    };
    let reach = class * crate::pathing::CELL;
    let room = |c: (i32, i32)| p.fits_kind(c, reach, kind);
    if apart((cx, cy)) && open((cx, cy)) && (!room_about || !p.walkable((cx, cy)) || room((cx, cy))) {
        return (x, y);
    }
    for k in 1..=64i32 {
        for c in ring_order(2, cy - k, cx - k, cy + 1 + k, cx + 1 + k) {
            if apart(c) && open(c) && p.fits_kind(c, radius, kind) && (!room_about || room(c)) {
                return p.center(c);
            }
        }
    }
    (x, y)
}

pub fn create_unit(w: &mut World, owner: u8, type_id: Id, x: f32, y: f32, facing: f32) -> u32 {
    // On the map, facing within a turn (measured: 400 reads 40, -90 reads 270).
    let (x, y) = w.clamp_to_map(x, y);
    let (x, y) = placed_at(w, type_id, x, y, facing.rem_euclid(360.0));
    create_unit_exact(w, owner, type_id, x, y, facing)
}

/// A unit made just where asked, without looking for room.
pub fn create_unit_exact(w: &mut World, owner: u8, type_id: Id, x: f32, y: f32, facing: f32) -> u32 {
    let (x, y) = w.clamp_to_map(x, y);
    // Placed on a quarter-unit grid, rounding down (measured: pigs made at -5719.4,
    // -4560.2 and -5627.1, -4500.0 stand at -5719.5, -4560.25 and -5627.25, -4500.0).
    let (x, y) = ((x * 4.0).floor() / 4.0, (y * 4.0).floor() / 4.0);
    let facing = facing.rem_euclid(360.0);
    let Some(o) = w.objects.units.get(type_id) else {
        log::warn!("CreateUnit: unknown unit type {}", crate::objects::id_str(type_id));
        return 0;
    };

    // Times its owner's handicap (measured: a footman made at 0.5 has 210 of 210).
    let hp = w.handicap_life(owner, o.real("HP").max(1.0));
    let max_mana = o.real("manaN");
    let mana = o.real("mana0").min(max_mana.max(0.0));
    let mut abilities: Vec<(Id, i32)> = o
        .list("abilList")
        .iter()
        .map(|s| (crate::objects::id_from_str(s.trim()), 1))
        .collect();
    abilities.retain(|(id, _)| id[0] != 0);
    let is_hero = type_id[0].is_ascii_uppercase();
    // A hero of a map saved by an editor before 6000 has its inventory whatever its
    // ability list says (measured: NightElf02, editor version 4436, lists Tyrande's
    // abilities as Sshm alone and her heroes have six slots; at 5999 too, at 6000 and
    // Booty Bay's 6030 none).
    let inventory = *b"AInv";
    if is_hero && w.editor_version < 6000 && !abilities.iter().any(|(a, _)| *a == inventory) {
        abilities.insert(0, (inventory, 1));
    }
    let unit = Unit {
        handle: 0,
        type_id,
        owner,
        x,
        y,
        fly_height: o.real("moveHeight"),
        facing,
        life: hp,
        max_life: hp,
        mana,
        max_mana,
        color: w.players.get(owner as usize).map_or(owner as i32, |p| p.color),
        scale: o.real("modelScale").max(0.0001),
        vertex_color: [o.int("red") as u8, o.int("green") as u8, o.int("blue") as u8, 255],
        user_data: 0,
        // Without a weapon enabled it acquires nothing (measured: a Goblin Sapper,
        // weapsOn 0 and acquire 500, reads 0).
        acquire_range: if o.int("weapsOn") == 0 { 0.0 } else { o.real("acquire") },
        move_speed: o.real("spd"),
        turn_speed: o.real("turnRate"),
        prop_window: o.real("propWin"),
        shown_facing: None,
        paused: false,
        hidden: false,
        invulnerable: false,
        dead: false,
        // Heroes start at level 1; for other units "level" is the kill-experience level.
        level: if is_hero { 1 } else { o.int("level") },
        hero_xp: 0,
        resource: 0,
        abilities,
        order: Order::default(),
        time_scale: 1.0,
        inventory: [0; 6],
        rally: None,
        explode_on_death: false,
        is_building: o.bool("isbldg"),
        is_hero,
        collision: o.real("collision"),
        action: Action::Idle,
        path: Vec::new(),
        repath_timer: 0.0,
        cooldown: 0.0,
        swing: None,
        acquire_timer: 0.0,
        anim: "stand".into(),
        anim_serial: 0,
        death_time: 0.0,
        pathing_cells: Vec::new(),
        build_cells: Vec::new(),
        carry_gold: 0,
        carry_lumber: 0,
        work_timer: 0.0,
        harvest_source: 0,
        inside: 0,
        queue: Vec::new(),
        queue_progress: 0.0,
        constructing: None,
        build_time: o.real("bldtm"),
        building: 0,
        str_: o.real("STR"),
        agi: o.real("AGI"),
        int: o.real("INT"),
        skill_points: if is_hero { 1 } else { 0 },
        regen_timer: 0.0,
        cooldowns: HashMap::default(),
        autocast: Vec::new(),
        buffs: Vec::new(),
        casting: None,
        timed_life: None,
        item_bonus: Default::default(),
        revive: None,
        reviving: false,
        channel: None,
        reincarnate: None,
        proper_name: String::new(),
        xp_suspended: false,
        queue_started: false,
        extra: Default::default(),
    };
    let path_tex = if unit.is_building { o.str("pathTex") } else { String::new() };
    let mut unit = unit;
    // Heroes take one of their type's proper names, preferring one no living hero of
    // the type has.
    if is_hero {
        let names = o.list("Propernames");
        let free: Vec<&String> = names.iter().filter(|n| !w.unit_order.iter().any(|&h| w.unit(h).is_some_and(|u| u.type_id == type_id && &u.proper_name == *n))).collect();
        let pool: Vec<&String> = if free.is_empty() { names.iter().collect() } else { free };
        if !pool.is_empty() {
            let k = w.rng.int(0, pool.len() as i32 - 1) as usize;
            unit.proper_name = pool[k].trim().to_string();
        }
    }
    // Gold mines start with their ability's gold (Agld DataA).
    for (a, _) in &unit.abilities {
        if let Some(ab) = w.objects.abilities.get(*a) {
            if ab.str("code") == "Agld" {
                unit.resource = ab.int("DataA1");
            }
        }
    }
    let mut unit = unit;
    // Creeps keep to where they settle once made (NaN until the first update).  Those
    // whose type may sleep (UnitData canSleep) sleep at night: the unit takes the creep
    // sleep ability only then (FUN_6f670950).
    if owner == crate::types::PLAYER_NEUTRAL_AGGRESSIVE && !unit.is_building {
        unit.extra.home = Some((f32::NAN, f32::NAN));
        unit.extra.can_sleep = w.objects.units.get(type_id).is_some_and(|o| o.int("canSleep") != 0);
    }
    unit.extra.born = w.time;
    // A unit takes its handle as a script first has it (`Handles::handle`; the hidden
    // gold mine under a haunted mine never does).
    let h = w.handles.insert_unseen(Obj::Unit(Box::new(unit)));
    if let Some(u) = w.unit_mut(h) {
        u.handle = h;
    }
    if is_hero {
        w.apply_hero_stats(h, true);
    } else {
        // Research already done applies to new units, at full life.
        w.apply_upgrades(h);
        if let Some(u) = w.unit_mut(h) {
            u.life = u.max_life;
            u.mana = u.mana.min(u.max_mana);
        }
    }
    w.unit_order.push(h);
    // Made finished, it adds itself to its owner's tech tree (FUN_6f677870 with 2).
    w.tech_add(owner, type_id);
    // A structure that spreads blight blights the four cells about it at once, its
    // growth following (measured: a Necropolis made by script has blighted 0 to 96 to
    // its right before the script's next line, and 768 a second later).
    if let Some((a, lvl)) = w.ability_with_code(h, "Abli") {
        if w.objects.abilities.get(a).is_some_and(|o| o.level_real("DataB", lvl) != 0.0) {
            w.blight_circle(x, y, 0.0);
        }
    }
    w.tag_unit(h);
    w.wander_paths(h);
    w.start_stock(h);
    w.attach_ability_art(h);
    w.sync_spell_books(h);
    w.score_made(owner, type_id, h, is_hero);
    if !path_tex.is_empty() {
        // Structures are always placed facing BuildingAngle; their footprint does not turn.
        let cells = w.pathing_cells(&path_tex, x, y, 0.0);
        let whole = if w.unit(h).is_some_and(|u| u.is_building) { w.build_cells(&path_tex, x, y, 0.0) } else { Vec::new() };
        if let Some(p) = w.pathing.as_mut() {
            p.stamp(&cells, 1);
            p.stamp_no_build(&whole, 1);
        }
        if let Some(u) = w.unit_mut(h) {
            u.pathing_cells = cells;
            u.build_cells = whole;
        }
    }
    // An armed unit first looks for a target after a random moment of up to a second
    // (CAbilityAttack::Initialize's timer, the random times 1.0 at 0x6fd6bf44; measured:
    // five riflemen made beside paused footmen first strike 0.16 to 0.73 seconds later
    // than an immediate look allows), drawn on the attack's own generator (vf194,
    // FUN_6f4993b0, stream 2).
    if w.unit(h).is_some_and(|u| !crate::sim::unit_weapons(w, u).is_empty()) {
        let t = w.rand_data.streams[crate::rng::stream::ATTACK].real(0.0, 1.0);
        if let Some(u) = w.unit_mut(h) {
            u.acquire_timer = t;
        }
    }
    w.creep_made(h);
    // Every structure has an alarm (CAbilityAlarm, measured: town halls, gold mines, and
    // shops each start one), which waits a random moment, drawn on its own generator,
    // before watching for enemies.  The watch is not played here; the draw is kept, so
    // the generator stays the original's.
    if w.unit(h).is_some_and(|u| u.is_building) {
        w.rand_data.streams[crate::rng::stream::ALARM].next_u32();
    }
    // Computer players' and creeps' units start with their autocast on.
    if w.players.get(owner as usize).is_some_and(|p| p.controller == map_control::COMPUTER || p.controller == map_control::CREEP) {
        let auto: Vec<Id> = w.unit(h).map(|u| u.abilities.iter().map(|a| a.0).filter(|&a| w.objects.abilities.get(a).is_some_and(|o| !o.str("Orderon").is_empty())).collect()).unwrap_or_default();
        if let Some(u) = w.unit_mut(h) {
            u.autocast = auto;
        }
    } else {
        // Others' start with their type's own (UnitAbilities auto) on (measured: a
        // user's and an empty slot's Priests heal, a user's Sorceress slows, and a Druid
        // of the Talon casts Faerie Fire unbidden; a Necromancer raises no dead).
        let own = w.unit(h).and_then(|u| w.objects.units.get(u.type_id)).map(|o| o.str("auto")).unwrap_or_default();
        let auto: Vec<Id> = own.split(',').map(|c| crate::objects::id_from_str(c.trim())).filter(|c| c[0] != 0).filter(|c| w.unit(h).is_some_and(|u| u.abilities.iter().any(|a| a.0 == *c))).collect();
        if let Some(u) = w.unit_mut(h) {
            u.autocast = auto;
        }
    }
    // Food it provides counts at once (measured: two farms made by a script read a food
    // cap 12 higher straight after).
    if w.unit(h).and_then(|u| w.objects.units.get(u.type_id)).is_some_and(|o| o.int("fmade") > 0) {
        w.update_food();
    }
    // A computer player's new unit may take a guard post.
    w.ai_new_unit(h);
    h
}

/// Kills a unit and fires its death events.
pub fn kill_unit(w: &mut World, vm: &mut Vm, h: u32, killer: u32) {
    w.set_asleep(h, false);
    let illusion = w.unit(h).and_then(|u| u.extra.illusion);
    // A unit set to explode (SetUnitExploded) or dying under Incinerate bursts, unless a
    // Hero (FUN_6f678280).
    let explodes = w.unit(h).is_some_and(|u| !u.is_hero && (u.explode_on_death || u.buffs.iter().any(|b| matches!(w.ability_code(b.ability).as_str(), "ANia" | "ANic"))));
    if w.unit(h).is_some_and(|u| !u.dead) && killer != 0 && illusion.is_none() {
        w.grant_bounty(h, killer);
        w.grant_kill_xp(h, killer);
    }
    // The Phoenix falls into its egg instead of dying.
    if w.phoenix_egg(h) {
        return;
    }
    // Reincarnation: the hero falls and rises again without dying.
    if w.unit(h).is_some_and(|u| !u.dead && u.is_hero) && w.try_reincarnate(h) {
        if let Some(u) = w.unit_mut(h) {
            u.dead = true;
            u.life = 0.0;
            u.action = Action::Idle;
            u.anim = "death".into();
            u.anim_serial = u.anim_serial.wrapping_add(1);
        }
        return;
    }
    if w.unit(h).is_some_and(|u| !u.dead) {
        w.detach_ability_art(h);
        w.death_abilities(vm, h);
        w.on_death_buffs(h);
        w.drop_on_death(vm, h);
        w.leave_mine(h);
        w.uncover_mine(vm, h);
    }
    let Some(u) = w.unit_mut(h) else { return };
    if u.dead {
        return;
    }
    u.dead = true;
    u.life = 0.0;
    u.action = Action::Idle;
    u.swing = None;
    u.path.clear();
    u.anim = "death".into();
    u.anim_serial = u.anim_serial.wrapping_add(1);
    let (owner, type_id, hero) = (u.owner, u.type_id, u.is_hero);
    w.unit_mut(h).unwrap().death_time = w.time;
    if let Some(k) = w.unit(killer).map(|k| k.owner) {
        w.score_killed(k, owner, type_id, hero);
    }
    let ctx = EventCtx {
        unit: h,
        widget: h,
        other_unit: killer,
        player: owner,
        has_player: true,
        ..Default::default()
    };
    // The owner's death triggers go first, then the unit's (measured: triggers made in
    // the order player A, unit B, player D, players E, unit F fire A D E B F).
    w.fire(vm, EventCtx { event_id: EVENT_PLAYER_UNIT_DEATH, ..ctx.clone() }, |_, k| match k {
        EventKind::PlayerUnit { player, event, .. } => *player == owner && *event == EVENT_PLAYER_UNIT_DEATH,
        _ => false,
    });
    w.fire(vm, EventCtx { event_id: EVENT_UNIT_DEATH, ..ctx }, |_, k| match k {
        EventKind::Unit { unit, event } => *unit == h && *event == EVENT_UNIT_DEATH,
        EventKind::Death(widget) => *widget == h,
        _ => false,
    });
    // A fallen hero can be revived (EVENT_PLAYER_HERO_REVIVABLE).
    if w.unit(h).is_some_and(|u| u.is_hero && u.dead) && illusion.is_none() {
        w.queue_unit_event(h, EVENT_PLAYER_HERO_REVIVABLE, EVENT_UNIT_HERO_REVIVABLE, EventCtx { unit: h, ..Default::default() });
    }
    // An illusion vanishes, its buff's Specialart left where it fell.
    if let Some((_, _, buff)) = illusion {
        if let Some((x, y)) = w.unit(h).map(|u| (u.x, u.y)) {
            let art = w.objects.buffs.get(buff).map(|o| o.list("Specialart").first().cloned().unwrap_or_default()).unwrap_or_default();
            w.spawn_effect(&art, x, y);
        }
        remove_unit(w, h);
    } else if explodes {
        // Into its type's Specialart, leaving no corpse.
        if let Some((x, y, type_id)) = w.unit(h).map(|u| (u.x, u.y, u.type_id)) {
            let art = w.objects.units.get(type_id).map(|o| o.list("Specialart").first().cloned().unwrap_or_default()).unwrap_or_default();
            w.spawn_effect(&art, x, y);
        }
        if let Some(u) = w.unit_mut(h) {
            u.hidden = true;
            u.extra.exploded = true;
        }
    }
}

/// A group entry left by a unit a script removed: skipped by everything but
/// FirstOfGroup, which reads null while it is first.
pub const GROUP_GHOST: u32 = u32::MAX;

/// RemoveUnit: the unit goes at the end of the moment (measured: it still counts in
/// its groups and has its type straight after), leaving its place in each group as a
/// ghost (measured: the group then counts and enumerates without it, yet FirstOfGroup
/// reads null while its place is first, even after other units are added).
pub fn script_remove_unit(w: &mut World, h: u32) {
    if w.unit(h).is_some() && !w.pending_removals.contains(&h) {
        w.pending_removals.push(h);
        // Its ground is free at once (Blizzard.j removes a gold mine and makes the
        // haunted mine in its place straight after; measured: it stands there).
        if let Some((cells, whole)) = w.unit_mut(h).map(|u| (std::mem::take(&mut u.pathing_cells), std::mem::take(&mut u.build_cells))) {
            if let Some(p) = w.pathing.as_mut() {
                p.stamp(&cells, -1);
                p.stamp_no_build(&whole, -1);
            }
        }
    }
}

/// The units RemoveUnit asked for, gone now.
pub fn finish_removals(w: &mut World) {
    for h in std::mem::take(&mut w.pending_removals) {
        if w.unit(h).is_none() {
            continue;
        }
        for id in w.handles.ids() {
            if let Some(Obj::Group(g)) = w.handles.get_mut(id) {
                for u in g.units.iter_mut().filter(|u| **u == h) {
                    *u = GROUP_GHOST;
                }
            }
        }
        remove_unit(w, h);
    }
}

pub fn remove_unit(w: &mut World, h: u32) {
    w.detach_ability_art(h);
    if let Some((cells, whole)) = w.unit_mut(h).map(|u| (std::mem::take(&mut u.pathing_cells), std::mem::take(&mut u.build_cells))) {
        if let Some(p) = w.pathing.as_mut() {
            p.stamp(&cells, -1);
            p.stamp_no_build(&whole, -1);
        }
    }
    if w.unit(h).is_some() {
        w.handles.remove(h);
        w.unit_order.retain(|x| *x != h);
        // Creep camps forget it, as its handle may come to name another unit.
        for camp in w.creep_camps.iter_mut() {
            camp.retain(|c| *c != h);
        }
        for id in w.handles.ids() {
            if let Some(Obj::Group(g)) = w.handles.get_mut(id) {
                g.units.retain(|u| *u != h);
            }
        }
    }
}

fn unit_arg<'a>(w: &'a World, a: &[Value], n: usize) -> Option<&'a Unit> {
    w.unit(a.h(n))
}

/// Removes a unit's buffs: positive ones (from allied casters), negative ones, or both.
/// UnitRemoveBuffs and UnitRemoveBuffsEx (FUN_6f48eb10).
fn remove_buffs(w: &mut World, h: u32, f: crate::bufftable::Filter) {
    let Some(u) = w.unit(h) else { return };
    let take: Vec<bool> = u.buffs.iter().map(|b| w.buff_removable(b, u.owner, f)).collect();
    let effects: Vec<u32> = u.buffs.iter().zip(&take).filter(|(_, t)| **t).map(|(b, _)| b.effect).collect();
    let u = w.unit_mut(h).unwrap();
    let mut k = 0;
    u.buffs.retain(|_| {
        k += 1;
        !take[k - 1]
    });
    for e in effects {
        w.remove_buff_effect(e);
    }
}

/// UnitHasBuffsEx and UnitCountBuffsEx (FUN_6f48bd70).
fn count_buffs(w: &World, h: u32, f: crate::bufftable::Filter) -> i32 {
    let Some(u) = w.unit(h) else { return 0 };
    u.buffs.iter().filter(|b| w.buff_counted(b, u.owner, f)).count() as i32
}

/// The Ex natives' arguments: positive, negative, magic, physical, timed life, aura,
/// auto dispel.
fn buff_filter(a: &[Value]) -> crate::bufftable::Filter {
    crate::bufftable::Filter { positive: a.b(1), negative: a.b(2), magic: a.b(3), physical: a.b(4), timed_life: a.b(5), aura: a.b(6), auto_dispel: a.b(7) }
}

/// Damage dealt by a script with an attack type (ATTACK_TYPE_* index).
/// Damage from UnitDamageTarget or UnitDamagePoint.  The attack type's DamageBonus
/// table applies; armor reduces only DAMAGE_TYPE_NORMAL (measured in the original:
/// 100 piercing damage of DAMAGE_TYPE_NORMAL to a farm is 26.9, of DAMAGE_TYPE_MAGIC or
/// UNIVERSAL 35, and ATTACK_TYPE_NORMAL's spells table is reduced by armor only with
/// DAMAGE_TYPE_NORMAL: 56.7 to a Paladin, 70 otherwise).
fn script_damage(w: &mut World, vm: &mut Vm, src: u32, target: u32, amount: f32, attack_type: i32, damage_type: i32) {
    const DAMAGE_TYPE_NORMAL: i32 = 4;
    const ATTACK_TYPE_MAGIC: i32 = 4;
    let physical = damage_type == DAMAGE_TYPE_NORMAL;
    // Spell immunity turns away magic attacks and these damage types: fire, cold,
    // lightning, divine, magic, sonic, force, death, mind, plant, defensive, spirit
    // link, and shadow strike (measured against a Dryad, each type in turn; unknown,
    // normal, enhanced, poison, disease, acid, demolition, slow poison, and universal
    // damage it takes).
    const RESISTED: [i32; 13] = [8, 9, 10, 13, 14, 15, 17, 18, 19, 20, 21, 24, 25];
    if w.is_magic_immune(target) && w.misc_f32("MagicImmunesResistDamage", 1.0) != 0.0 && (attack_type == ATTACK_TYPE_MAGIC || RESISTED.contains(&damage_type)) {
        return;
    }
    if attack_type == 0 && !physical {
        return w.spell_damage(vm, src, target, amount);
    }
    // Spells of DAMAGE_TYPE_NORMAL meet an ethereal unit as a normal attack would
    // (measured: nothing, where of DAMAGE_TYPE_MAGIC they deal 1.66 times).
    if attack_type == 0 && w.is_ethereal(target) && w.ethereal_factor(src, target, "normal") <= 0.0 {
        return;
    }
    let name = match attack_type {
        0 => "spells",
        1 => "normal",
        2 => "pierce",
        3 => "siege",
        4 => "magic",
        5 => "chaos",
        6 => "hero",
        _ => "normal",
    };
    w.damage_ignores_armor = !physical;
    w.deal_damage_pub(vm, src, target, amount, name);
    w.damage_ignores_armor = false;
}

/// A worker's build order at a point.
fn issue_build(w: &mut World, h: u32, id: crate::objects::Id, x: f32, y: f32) -> NativeResult {
    let ok = w.unit(h).and_then(|u| w.objects.units.get(u.type_id)).is_some_and(|o| o.list("Builds").iter().any(|b| crate::objects::id_from_str(b.trim()) == id));
    if !ok {
        return boolean(false);
    }
    // A spot the structure cannot stand on is refused at once (measured: the original's
    // IssueBuildOrderById returns false for a farm on a blocked spot, true otherwise).
    let (x, y) = w.snap_placement(id, x, y);
    if !w.can_place(id, x, y) {
        return boolean(false);
    }
    boolean(w.order_build(h, id, x, y).is_ok())
}

fn play_anim(u: &mut Unit, name: String) {
    u.extra.anim_queue.clear();
    u.anim = name;
    u.anim_serial = u.anim_serial.wrapping_add(1);
}

fn with_unit(w: &mut World, a: &[Value], f: impl FnOnce(&mut Unit)) -> NativeResult {
    if let Some(u) = w.unit_mut(a.h(0)) {
        f(u);
    }
    none()
}

/// A group sent to a point: each unit ordered, then those that went taking places in a
/// formation there.
fn group_point_order(w: &mut World, vm: &mut Vm, g: u32, order: i32, x: f32, y: f32) -> NativeResult {
    let units = w.group(g).map(|g| g.units.clone()).unwrap_or_default();
    for &u in &units {
        issue_order(w, vm, u, order, x, y, 0);
    }
    let going: Vec<u32> = units.into_iter().filter(|&u| w.unit(u).is_some_and(|u| matches!(u.action, Action::Move { .. } | Action::AttackMove { .. }))).collect();
    w.take_formation_places(&going, (x, y));
    boolean(true)
}

/// UNIT_TYPE_* (common.j ConvertUnitType values).  Classifications that depend on status
/// effects the simulation does not model yet read as false.
fn is_unit_type(w: &World, u: &Unit, t: i32) -> bool {
    // UnitAddType and UnitRemoveType override the type's classifications.
    if u.extra.added_types.contains(&t) {
        return true;
    }
    if u.extra.removed_types.contains(&t) {
        return false;
    }
    is_unit_type_base(w, u, t)
}

fn is_unit_type_base(w: &World, u: &Unit, t: i32) -> bool {
    let o = w.objects.units.get(u.type_id);
    let field = |f: &str| o.map(|o| o.str(f).to_ascii_lowercase()).unwrap_or_default();
    let classes = field("type");
    let has = |c: &str| classes.split(',').any(|x| x.trim() == c);
    let targets = format!("{},{}", field("targs1"), field("targs2"));
    match t {
        0 => u.is_hero,
        1 => u.dead,
        // An uprooted ancient is still a structure (measured).
        2 => u.is_building || u.extra.uprooted,
        3 => field("movetp") == "fly",
        4 => field("movetp") != "fly",
        5 => targets.split(',').any(|x| x.trim() == "air"),
        6 => targets.split(',').any(|x| x.trim() == "ground"),
        7 => field("weaptp1") == "normal",
        8 => !field("weaptp1").is_empty() && field("weaptp1") != "normal",
        9 => has("giant"),
        10 => has("summoned"),
        14 => has("undead"),
        15 => has("mechanical"),
        16 => has("peon"),
        17 => has("sapper"),
        18 => has("townhall"),
        19 => has("ancient"),
        20 => has("tauren"),
        _ => false,
    }
}

/// Clears a group and fills it with units passing `pred` and the filter.
fn enum_units(w: &mut World, vm: &mut Vm, group: u32, filter: u32, limit: i32, pred: impl Fn(&World, &Unit) -> bool) {
    if let Some(g) = w.group_mut(group) {
        g.units.clear();
    } else {
        return;
    }
    let candidates: Vec<u32> = w.unit_order.iter().copied().filter(|&h| w.unit(h).is_some_and(|u| pred(w, u))).collect();
    let mut count = 0;
    for h in candidates {
        if limit >= 0 && count >= limit {
            break;
        }
        let saved = w.enum_ctx;
        w.enum_ctx.filter_unit = h;
        let ok = filter == 0 || w.eval_boolexpr(vm, filter);
        w.enum_ctx = saved;
        if ok {
            if let Some(g) = w.group_mut(group) {
                g.units.push(h);
            }
            count += 1;
        }
    }
}

fn for_group(w: &mut World, vm: &mut Vm, a: &[Value]) -> NativeResult {
    let Some(f) = a.code(1) else { return none() };
    w.tags.handler(f);
    let units = w.group(a.h(0)).map(|g| g.units.clone()).unwrap_or_default();
    for u in units {
        if w.unit(u).is_none() {
            continue;
        }
        let saved = w.enum_ctx;
        w.enum_ctx.enum_unit = u;
        let _ = vm.call(w, f, &[]);
        w.enum_ctx = saved;
    }
    none()
}

/// A player's order to a neutral building: an item type it sells is bought for the
/// player, and a unit type hired; the rest go to the building as orders (measured: the
/// original's IssueNeutralImmediateOrderById(player, a Goblin Merchant, 'dust') buys
/// the one in stock, 75 gold, then returns false).
fn issue_neutral(w: &mut World, vm: &mut Vm, a: &[Value], order: i32, x: f32, y: f32, target: u32) -> NativeResult {
    let (Some(player), shop) = (super::players::player_arg(w, a, 0), a.h(1)) else { return boolean(false) };
    let id = id_from_int(order);
    if w.objects.items.get(id).is_some() {
        return boolean(w.buy_item(vm, player, shop, id).is_ok());
    }
    let sells_unit = w.unit(shop).and_then(|u| w.objects.units.get(u.type_id)).is_some_and(|o| o.list("Sellunits").iter().any(|b| crate::objects::id_from_str(b.trim()) == id));
    if sells_unit {
        return boolean(w.hire_unit(vm, player, shop, id).is_ok());
    }
    issue_order(w, vm, shop, order, x, y, target)
}

/// The objects an immediate order makes (measured: a creep ordered to stop makes an
/// order and its tasks; a Shadow Priest's autodispeloff, which only turns its
/// autocast off, none).
fn tag_immediate_order(w: &mut World, unit: u32, order: i32) {
    let Some(u) = w.unit(unit) else { return };
    let name = crate::natives::basic::order_name(order).unwrap_or_default();
    let toggle = u.abilities.iter().any(|(a, _)| w.objects.abilities.get(*a).is_some_and(|o| o.str("Orderon") == name || o.str("Orderoff") == name));
    if !toggle {
        w.tags.take_all(&[b"+ord", b"tskA", b"task", b"task", b"task", b"task", b"tskA"]);
    }
}

fn issue_order(w: &mut World, vm: &mut Vm, unit: u32, order: i32, x: f32, y: f32, target: u32) -> NativeResult {
    let Some(owner) = w.unit(unit).map(|u| u.owner) else { return boolean(false) };
    // An order that is a unit type or a research: a structure trains it, researches it,
    // or becomes it (a town hall to a keep); a worker builds it at the point.
    let id = id_from_int(order);
    let is_unit_type = w.objects.units.get(id).is_some();
    let is_research = w.objects.upgrades.get(id).is_some();
    // An id that is no order (nor a unit, research, or item type) is refused (measured:
    // the original's IssueImmediateOrderById with a handle id returns false).
    if !is_unit_type && !is_research && w.objects.items.get(id).is_none() && crate::natives::basic::order_name(order).is_none() {
        return boolean(false);
    }
    if is_unit_type || is_research {
        let ok = if is_research {
            w.research_at(unit, id).is_ok()
        } else if w.unit(unit).is_some_and(|u| u.is_building) {
            let upgrades_to = w.unit(unit).and_then(|u| w.objects.units.get(u.type_id)).is_some_and(|o| o.list("Upgrade").iter().any(|b| crate::objects::id_from_str(b.trim()) == id));
            if upgrades_to { w.upgrade_structure(unit, id).is_ok() } else { w.train(unit, id).is_ok() }
        } else {
            // At its target when it has one: Blizzard.j's IssueHauntOrderAtLocBJ orders
            // a haunted gold mine as a target order on the mine (measured: Undead03's
            // acolyte so ordered pays 255 gold and 220 lumber and haunts the mine).
            let (x, y) = if target != 0 { w.widget_pos(target).unwrap_or((x, y)) } else { (x, y) };
            return issue_build(w, unit, id, x, y);
        };
        // What it starts on, it starts at once, its event firing within the order
        // (measured: a barracks ordered to train two footmen fires its first train
        // start before the second is paid for, 865 gold of 1000 left; ours fired it
        // on the next update, 730 left).
        if ok {
            w.start_production(vm, unit);
        }
        return boolean(ok);
    }
    w.order_error = None;
    w.order_unit(unit, order, x, y, target);
    // A refused order (a target the ability does not allow) is not issued.
    if w.order_error.take().is_some() {
        return boolean(false);
    }
    let _ = owner;
    let (pe, ue, ctx) = w.order_event(unit, order, x, y, target);
    w.fire_unit_pair(vm, unit, pe, ue, ctx);
    boolean(true)
}

pub fn register(t: &mut HashMap<&'static str, NativeFn>) {
    reg!(t,
        CreateUnit => |w, vm, a| {
            let Some(p) = w.player_of(a.h(0)) else { return none() };
            let h = create_unit(w, p, id_from_int(a.i(1)), a.r(2), a.r(3), a.r(4));
            w.unit_placed(vm, h);
            handle(h)
        },
        CreateUnitByName => |w, vm, a| {
            let Some(p) = w.player_of(a.h(0)) else { return none() };
            let name = a.s(1);
            let id = w.objects.units.objects.iter().find(|(_, o)| o.str("unitName").eq_ignore_ascii_case(&name)).map(|(id, _)| *id);
            match id {
                Some(id) => {
                    let h = create_unit(w, p, id, a.r(2), a.r(3), a.r(4));
                    w.unit_placed(vm, h);
                    handle(h)
                }
                None => none(),
            }
        },
        CreateUnitAtLoc => |w, vm, a| {
            let Some(p) = w.player_of(a.h(0)) else { return none() };
            let (x, y) = w.location(a.h(2)).unwrap_or_default();
            let h = create_unit(w, p, id_from_int(a.i(1)), x, y, a.r(3));
            w.unit_placed(vm, h);
            handle(h)
        },
        CreateUnitAtLocByName => |w, vm, a| {
            let Some(p) = w.player_of(a.h(0)) else { return none() };
            let (x, y) = w.location(a.h(2)).unwrap_or_default();
            let name = a.s(1);
            let id = w.objects.units.objects.iter().find(|(_, o)| o.str("unitName").eq_ignore_ascii_case(&name)).map(|(id, _)| *id);
            match id {
                Some(id) => {
                    let h = create_unit(w, p, id, x, y, a.r(3));
                    w.unit_placed(vm, h);
                    handle(h)
                }
                None => none(),
            }
        },
        CreateCorpse => |w, _, a| {
            let Some(p) = w.player_of(a.h(0)) else { return none() };
            let h = create_unit(w, p, id_from_int(a.i(1)), a.r(2), a.r(3), a.r(4));
            if let Some(u) = w.unit_mut(h) { u.dead = true; u.life = 0.0; }
            handle(h)
        },
        // Each makes its tasks (measured by script: a creep's).
        KillUnit => |w, vm, a| {
            if w.unit(a.h(0)).is_some_and(|u| !u.dead) {
                w.tags.take(b"tskA");
                w.tags.take_n(b"task", 7);
                w.tags.take(b"tskA");
            }
            kill_unit(w, vm, a.h(0), 0);
            none()
        },
        RemoveUnit => |w, _, a| {
            // A structure's footprint goes at once (measured: a gold mine's three
            // regions freed before the removal's tasks).
            if let Some(footprint) = w.unit_mut(a.h(0)).map(|u| std::mem::take(&mut u.extra.footprint_slots)) {
                for s in footprint {
                    w.tags.release(s);
                }
                let t: Vec<u32> = [b"tskA", b"task", b"task", b"task", b"task", b"task", b"tskA"].iter().map(|c| w.tags.take(c)).collect();
                // The removal's tasks and the unit's own objects freed as the next turn
                // starts, in this order (traced for a gold mine Blizzard.j removes): the
                // first and last tasks, the sixth and fifth; then, after what is
                // released meanwhile, the fourth, its listener (provisional: its
                // modifiers with it, the last first), the third task, its abilities'
                // objects (the last first), its five reals (the last first), its path,
                // its two regions, its position, itself, the second task.
                let objects = w.unit(a.h(0)).map(|u| u.extra.object_slots.clone()).unwrap_or_default();
                let live = |w: &World, (s, b): (u32, u32)| w.tags.live.get(&s).is_some_and(|e| e.1 == b);
                for k in [0, 6, 5, 4] {
                    w.tags.doom(t[k]);
                }
                let mut later = vec![t[3]];
                later.extend(objects[1].iter().rev().filter(|&&o| live(w, o)).map(|o| o.0));
                later.push(t[2]);
                later.extend(objects[2].iter().rev().filter(|&&o| live(w, o)).map(|o| o.0));
                later.extend([8, 7, 6, 5, 4, 9, 2, 3, 1, 0].iter().filter_map(|&k| objects[0].get(k).filter(|&&o| live(w, o)).map(|o| o.0)));
                later.push(t[1]);
                w.tags.removed.extend(later);
                // Its day change's object goes on the frame after (traced: the gold
                // mine's at 0.0002, after its own at 0.0001).
                if let Some(s) = w.unit_mut(a.h(0)).and_then(|u| u.extra.day_rcm.take()) {
                    if w.tags.timed.iter().any(|&(_, x)| x == s) {
                        let at = w.time_of_day_clock + 1e-4;
                        w.tags.redoom_at(at, s);
                    }
                }
            }
            script_remove_unit(w, a.h(0));
            none()
        },
        ShowUnit => |w, _, a| { let v = a.b(1); with_unit(w, a, |u| u.hidden = !v) },
        IsUnitHidden => |w, _, a| boolean(unit_arg(w, a, 0).is_some_and(|u| u.hidden)),
        SetUnitState => |w, vm, a| {
            let (s, v) = (a.h(1) as i32, a.r(2));
            let r = with_unit(w, a, |u| match s {
                unit_state::LIFE => u.life = v.clamp(0.0, u.max_life),
                unit_state::MAX_LIFE => u.max_life = v,
                unit_state::MANA => u.mana = v.clamp(0.0, u.max_mana),
                unit_state::MAX_MANA => u.max_mana = v,
                _ => {}
            });
            if s == unit_state::MANA {
                w.mana_changed(a.h(0));
            }
            // Life set to none kills (measured: -5 reads 0, and the unit is dead).
            if s == unit_state::LIFE && w.unit(a.h(0)).is_some_and(|u| !u.dead && u.life <= 0.405) {
                kill_unit(w, vm, a.h(0), 0);
            }
            r
        },
        GetUnitState => |w, _, a| real(unit_arg(w, a, 0).map_or(0.0, |u| match a.h(1) as i32 {
            unit_state::LIFE => u.life,
            unit_state::MAX_LIFE => u.max_life,
            unit_state::MANA => u.mana,
            unit_state::MAX_MANA => u.max_mana,
            _ => 0.0,
        })),
        GetWidgetLife => |w, _, a| real(unit_arg(w, a, 0).map(|u| u.life).or_else(|| match w.handles.get(a.h(0)) {
            Some(Obj::Destructable(d)) => Some(d.life),
            Some(Obj::Item(i)) => Some(i.life),
            _ => None,
        }).unwrap_or(0.0)),
        SetWidgetLife => |w, _, a| {
            let v = a.r(1);
            match w.handles.get_mut(a.h(0)) {
                Some(Obj::Unit(u)) => u.life = v.min(u.max_life),
                Some(Obj::Destructable(d)) => d.life = v.min(d.max_life),
                Some(Obj::Item(i)) => i.life = v,
                _ => {}
            }
            none()
        },
        GetWidgetX => |w, _, a| real(match w.handles.get(a.h(0)) {
            Some(Obj::Unit(u)) => u.x, Some(Obj::Destructable(d)) => d.x, Some(Obj::Item(i)) => i.x, _ => 0.0,
        }),
        GetWidgetY => |w, _, a| real(match w.handles.get(a.h(0)) {
            Some(Obj::Unit(u)) => u.y, Some(Obj::Destructable(d)) => d.y, Some(Obj::Item(i)) => i.y, _ => 0.0,
        }),
        GetUnitX => |w, _, a| real(unit_arg(w, a, 0).map_or(0.0, |u| u.x)),
        GetUnitY => |w, _, a| real(unit_arg(w, a, 0).map_or(0.0, |u| u.y)),
        GetUnitLoc => |w, _, a| {
            let (x, y) = unit_arg(w, a, 0).map_or((0.0, 0.0), |u| (u.x, u.y));
            handle(w.insert_location(x, y))
        },
        GetUnitFacing => |w, _, a| real(unit_arg(w, a, 0).map_or(0.0, |u| u.shown_facing.unwrap_or(u.facing))),
        // The speed it moves at now, with its bonuses (measured: 337.5 for a grunt, 270,
        // under Bloodlust's 25 percent).
        GetUnitMoveSpeed => |w, _, a| real(unit_arg(w, a, 0).map_or(0.0, |u| w.current_move_speed(u.handle))),
        GetUnitDefaultMoveSpeed => |w, _, a| real(unit_arg(w, a, 0).and_then(|u| w.objects.units.get(u.type_id)).map_or(0.0, |o| o.real("spd"))),
        GetUnitTurnSpeed => |w, _, a| real(unit_arg(w, a, 0).map_or(0.0, |u| u.turn_speed)),
        GetUnitPropWindow => |w, _, a| real(unit_arg(w, a, 0).map_or(0.0, |u| u.prop_window.to_radians())),
        GetUnitFlyHeight => |w, _, a| real(unit_arg(w, a, 0).map_or(0.0, |u| u.fly_height)),
        GetUnitAcquireRange => |w, _, a| real(unit_arg(w, a, 0).map_or(0.0, |u| u.acquire_range)),
        // A unit moved by a script plans its way again from where it stands (measured:
        // NightElf02's opening, skipped, moves a walking Tyrande ahead along her way; ours
        // walked her back to the waypoints she had planned, into the rect that starts
        // the Doom Guards' scene, which the original never shows there).
        SetUnitX => |w, vm, a| { let v = a.r(1); let r = with_unit(w, a, |u| { u.x = v; u.path.clear(); }); w.unit_placed(vm, a.h(0)); r },
        SetUnitY => |w, vm, a| { let v = a.r(1); let r = with_unit(w, a, |u| { u.y = v; u.path.clear(); }); w.unit_placed(vm, a.h(0)); r },
        // Kept on the map (measured: 99999 places at 10208), unlike SetUnitX and SetUnitY,
        // and it stops the unit (measured: a footman walking east, so placed, has no order
        // after and stays where it was put; one moved by SetUnitX and SetUnitY walks on).
        SetUnitPosition => |w, vm, a| {
            let (x, y) = w.clamp_to_map(a.r(1), a.r(2));
            let r = with_unit(w, a, |u| { u.x = x; u.y = y; placed_stop(u); });
            w.unit_placed(vm, a.h(0));
            r
        },
        SetUnitPositionLoc => |w, vm, a| {
            let (x, y) = w.location(a.h(1)).unwrap_or_default();
            let (x, y) = w.clamp_to_map(x, y);
            let r = with_unit(w, a, |u| { u.x = x; u.y = y; placed_stop(u); });
            w.unit_placed(vm, a.h(0));
            r
        },
        // Within a turn (measured: 720 reads 0).
        SetUnitFacing => |w, _, a| { let v = a.r(1).rem_euclid(360.0); with_unit(w, a, |u| u.facing = v) },
        SetUnitFacingTimed => |w, _, a| { let v = a.r(1).rem_euclid(360.0); with_unit(w, a, |u| u.facing = v) },
        // A unit that can move cannot be stopped this way: its speed stays within its
        // limits (measured: SetUnitMoveSpeed(footman, 0) leaves it walking at 150,
        // MinUnitSpeed).
        SetUnitMoveSpeed => |w, _, a| {
            let v = a.r(1);
            let mobile = unit_arg(w, a, 0).and_then(|u| w.objects.units.get(u.type_id)).is_some_and(|o| o.real("spd") > 0.0);
            with_unit(w, a, |u| u.move_speed = if mobile { v.max(1.0) } else { v })
        },
        // Never below 0, nor below the type's moveFloor (measured: a gryphon rider set
        // to 50, 0, or -50 reads 90; a footman set to -50 reads 0).
        SetUnitFlyHeight => |w, _, a| {
            let floor = unit_arg(w, a, 0).and_then(|u| w.objects.units.get(u.type_id)).map_or(0.0, |o| o.real("moveFloor"));
            let (v, rate) = (a.r(1).max(0.0).max(floor), a.r(2));
            // GetUnitFlyHeight reads the new height at once; the model gets there at the
            // rate (0 for at once).
            with_unit(w, a, |u| {
                u.extra.shown_height = (rate > 0.0).then(|| u.extra.shown_height.unwrap_or(u.fly_height));
                u.extra.height_rate = rate;
                u.fly_height = v;
            })
        },
        // Both are kept as angles within a turn (measured: -1 reads 5.283, 7 reads 0.717).
        SetUnitTurnSpeed => |w, _, a| { let v = a.r(1).rem_euclid(std::f32::consts::TAU); with_unit(w, a, |u| u.turn_speed = v) },
        SetUnitPropWindow => |w, _, a| { let v = a.r(1).rem_euclid(std::f32::consts::TAU).to_degrees(); with_unit(w, a, |u| u.prop_window = v) },
        // Only above 50, and only for a unit that can attack (FUN_6f697ea0: a setting of
        // 0 leaves a footman's 500).
        SetUnitAcquireRange => |w, _, a| {
            let v = a.r(1);
            let armed = unit_arg(w, a, 0).and_then(|u| w.objects.units.get(u.type_id)).is_some_and(|o| o.int("weapsOn") != 0);
            with_unit(w, a, |u| if v > 50.0 && armed { u.acquire_range = v })
        },
        SetUnitScale => |w, _, a| { let v = a.r(1); with_unit(w, a, |u| u.scale = v) },
        SetUnitTimeScale => |w, _, a| { let v = a.r(1); with_unit(w, a, |u| u.time_scale = v) },
        SetUnitVertexColor => |w, _, a| {
            let c = [a.i(1) as u8, a.i(2) as u8, a.i(3) as u8, a.i(4) as u8];
            with_unit(w, a, |u| u.vertex_color = c)
        },
        SetUnitColor => |w, _, a| { let c = a.h(1) as i32; with_unit(w, a, |u| u.color = c) },
        // A script's animation plays once (the unit then stands); by index, the model's
        // sequence of that number ("#n" to the presentation layer).
        SetUnitAnimation => |w, _, a| { let s = a.s(1).to_ascii_lowercase(); with_unit(w, a, |u| play_anim(u, s)) },
        SetUnitAnimationByIndex => |w, _, a| { let s = format!("#{}", a.i(1)); with_unit(w, a, |u| play_anim(u, s)) },
        SetUnitAnimationWithRarity => |w, _, a| { let s = a.s(1).to_ascii_lowercase(); with_unit(w, a, |u| play_anim(u, s)) },
        QueueUnitAnimation => |w, _, a| { let s = a.s(1).to_ascii_lowercase(); with_unit(w, a, |u| u.extra.anim_queue.push(s)) },
        AddUnitAnimationProperties => |w, _, a| {
            let (props, add) = (a.s(1).to_ascii_lowercase(), a.b(2));
            with_unit(w, a, |u| {
                for p in props.split([',', ' ']).filter(|p| !p.is_empty()) {
                    u.extra.anim_props.retain(|x| x != p);
                    if add {
                        u.extra.anim_props.push(p.to_string());
                    }
                }
            })
        },
        SetUnitBlendTime => |w, _, a| { let v = a.r(1); with_unit(w, a, |u| u.extra.blend_time = Some(v.max(0.0))) },
        SetUnitLookAt => |w, _, a| {
            let (bone, target, off) = (a.s(1), a.h(2), [a.r(3), a.r(4), a.r(5)]);
            with_unit(w, a, |u| u.extra.look_at = Some((bone, target, off)))
        },
        ResetUnitLookAt => |w, _, a| with_unit(w, a, |u| u.extra.look_at = None),
        // The original grants and takes away the Invulnerable ability ('Avul').
        SetUnitInvulnerable => |w, _, a| {
            let v = a.b(1);
            let avul = crate::objects::id_from_str("Avul");
            with_unit(w, a, |u| {
                u.invulnerable = v;
                u.abilities.retain(|(x, _)| *x != avul);
                if v {
                    u.abilities.push((avul, 1));
                }
            })
        },
        PauseUnit => |w, _, a| { let v = a.b(1); with_unit(w, a, |u| u.paused = v) },
        IsUnitPaused => |w, _, a| boolean(unit_arg(w, a, 0).is_some_and(|u| u.paused)),
        SetUnitExploded => |w, _, a| { let v = a.b(1); with_unit(w, a, |u| u.explode_on_death = v) },
        SetUnitUserData => |w, _, a| { let v = a.i(1); with_unit(w, a, |u| u.user_data = v) },
        GetUnitUserData => |w, _, a| int(unit_arg(w, a, 0).map_or(0, |u| u.user_data)),
        GetUnitTypeId => |w, _, a| int(unit_arg(w, a, 0).map_or(0, |u| id_to_int(u.type_id))),
        GetUnitRace => |w, _, a| {
            let race = unit_arg(w, a, 0).and_then(|u| w.objects.units.get(u.type_id)).map(|o| o.str("race").to_ascii_lowercase()).unwrap_or_default();
            NativeResult::Return(Value::Handle(match race.as_str() {
                "human" => 1, "orc" => 2, "undead" => 3, "nightelf" => 4, "demon" => 5, "other" => 7, _ => 7,
            }))
        },
        GetUnitName => |w, _, a| {
            let name = unit_arg(w, a, 0).and_then(|u| w.objects.units.get(u.type_id)).map(|o| o.str("Name")).unwrap_or_default();
            string(w.localized(&name))
        },
        GetUnitFoodUsed => |w, _, a| int(unit_arg(w, a, 0).and_then(|u| w.objects.units.get(u.type_id)).map_or(0, |o| o.int("fused"))),
        GetUnitFoodMade => |w, _, a| int(unit_arg(w, a, 0).and_then(|u| w.objects.units.get(u.type_id)).map_or(0, |o| o.int("fmade"))),
        GetFoodUsed => |w, _, a| int(w.objects.units.get(id_from_int(a.i(0))).map_or(0, |o| o.int("fused"))),
        GetFoodMade => |w, _, a| int(w.objects.units.get(id_from_int(a.i(0))).map_or(0, |o| o.int("fmade"))),
        GetUnitPointValue => |w, _, a| int(unit_arg(w, a, 0).and_then(|u| w.objects.units.get(u.type_id)).map_or(0, |o| o.int("points"))),
        GetUnitPointValueByType => |w, _, a| int(w.objects.units.get(id_from_int(a.i(0))).map_or(0, |o| o.int("points"))),
        GetUnitLevel => |w, _, a| int(unit_arg(w, a, 0).map_or(0, |u| u.level)),
        GetHeroLevel => |w, _, a| int(unit_arg(w, a, 0).map_or(0, |u| u.level)),
        GetHeroXP => |w, _, a| int(unit_arg(w, a, 0).map_or(0, |u| u.hero_xp)),
        GetHeroProperName => |w, _, a| { let n = unit_arg(w, a, 0).map(|u| u.proper_name.clone()).unwrap_or_default(); string(n) },
        AddHeroXP => |w, vm, a| { w.add_hero_xp(a.h(0), a.i(1)); w.flush_events(vm); none() },
        SetHeroXP => |w, vm, a| {
            let cur = unit_arg(w, a, 0).map_or(0, |u| u.hero_xp);
            if a.i(1) > cur { w.add_hero_xp(a.h(0), a.i(1) - cur); }
            w.flush_events(vm);
            none()
        },
        SetHeroLevel => |w, vm, a| {
            // Up to MaxHeroLevel.  The original looks the experience up at the level less
            // one, clamped into its table, so level 1 reads the first entry, level 2's, and
            // a level 1 hero set to level 1 rises to 2 (measured: 200 experience).
            let (h, l) = (a.h(0), a.i(1).min(w.misc_f32("MaxHeroLevel", 10.0) as i32));
            let l = if l == 1 { 2 } else { l };
            while w.unit(h).is_some_and(|u| u.is_hero && !u.dead && u.level < l) {
                let need = w.hero_xp_needed(w.unit(h).unwrap().level + 1) - w.unit(h).unwrap().hero_xp;
                w.add_hero_xp(h, need.max(1));
            }
            w.flush_events(vm);
            none()
        },
        // With includeBonuses, its items' too (measured: a Paladin of 22 strength with
        // Gauntlets of Ogre Strength reads 25, and 22 without).
        GetHeroStr => |w, _, a| { let b = a.b(1); int(unit_arg(w, a, 0).map_or(0, |u| (u.str_ + if b { u.item_bonus.str_ } else { 0.0 }) as i32)) },
        GetHeroAgi => |w, _, a| { let b = a.b(1); int(unit_arg(w, a, 0).map_or(0, |u| (u.agi + if b { u.item_bonus.agi } else { 0.0 }) as i32)) },
        GetHeroInt => |w, _, a| { let b = a.b(1); int(unit_arg(w, a, 0).map_or(0, |u| (u.int + if b { u.item_bonus.int } else { 0.0 }) as i32)) },
        GetHeroSkillPoints => |w, _, a| int(unit_arg(w, a, 0).map_or(0, |u| u.skill_points)),
        UnitModifySkillPoints => |w, _, a| {
            let d = a.i(1);
            let Some(u) = w.unit_mut(a.h(0)) else { return boolean(false) };
            u.skill_points = (u.skill_points + d).max(0);
            boolean(true)
        },
        SelectHeroSkill => |w, vm, a| { w.learn_skill(a.h(0), id_from_int(a.i(1))); w.flush_events(vm); none() },
        SetHeroStr => |w, _, a| { if let Some(u) = w.unit(a.h(0)) { let v = [a.i(1) as f32, u.agi, u.int]; w.set_hero_attrs(a.h(0), v); } w.apply_hero_stats(a.h(0), false); none() },
        SetHeroAgi => |w, _, a| { if let Some(u) = w.unit(a.h(0)) { let v = [u.str_, a.i(1) as f32, u.int]; w.set_hero_attrs(a.h(0), v); } w.apply_hero_stats(a.h(0), false); none() },
        SetHeroInt => |w, _, a| { if let Some(u) = w.unit(a.h(0)) { let v = [u.str_, u.agi, a.i(1) as f32]; w.set_hero_attrs(a.h(0), v); } w.apply_hero_stats(a.h(0), false); none() },
        // Levels lost take their attribute gains and skill points with them; the
        // experience drops to the new level's.
        UnitStripHeroLevel => |w, _, a| {
            let h = a.h(0);
            let Some(u) = w.unit(h).filter(|u| u.is_hero) else { return boolean(false) };
            let n = a.i(1).min(u.level - 1).max(0);
            if n == 0 { return boolean(false); }
            let level = u.level - n;
            let xp = w.hero_xp_needed(level);
            let u = w.unit_mut(h).unwrap();
            u.level = level;
            u.hero_xp = xp;
            u.skill_points = (u.skill_points - n).max(0);
            w.refresh_hero_attrs(h);
            w.apply_hero_stats(h, false);
            boolean(true)
        },
        // Defaults from the unit's type.
        GetUnitDefaultAcquireRange => |w, _, a| real(unit_arg(w, a, 0).and_then(|u| w.objects.units.get(u.type_id)).map_or(0.0, |o| o.real("acquire"))),
        GetUnitDefaultTurnSpeed => |w, _, a| real(unit_arg(w, a, 0).and_then(|u| w.objects.units.get(u.type_id)).map_or(0.0, |o| o.real("turnRate"))),
        GetUnitDefaultPropWindow => |w, _, a| real(unit_arg(w, a, 0).and_then(|u| w.objects.units.get(u.type_id)).map_or(0.0, |o| o.real("propWin"))),
        GetUnitDefaultFlyHeight => |w, _, a| real(unit_arg(w, a, 0).and_then(|u| w.objects.units.get(u.type_id)).map_or(0.0, |o| o.real("moveHeight"))),
        SetUnitRescuable => |w, _, a| {
            let (Some(p), on) = (w.player_of(a.h(1)), a.b(2)) else { return none() };
            with_unit(w, a, |u| if on { u.extra.rescuable_by |= 1 << p } else { u.extra.rescuable_by &= !(1 << p) })
        },
        SetUnitRescueRange => |w, _, a| { let r = a.r(1); with_unit(w, a, |u| u.extra.rescue_range = r) },
        // A unit whose pathing is off goes straight over everything; a structure's
        // footprint no longer blocks.
        SetUnitPathing => |w, _, a| {
            let (h, on) = (a.h(0), a.b(1));
            let Some(u) = w.unit_mut(h) else { return none() };
            if u.extra.no_pathing == !on {
                return none();
            }
            u.extra.no_pathing = !on;
            let (type_id, x, y) = (u.type_id, u.x, u.y);
            let cells = if on {
                let tex = w.objects.units.get(type_id).map(|o| o.str("pathTex")).unwrap_or_default();
                w.pathing_cells(&tex, x, y, 0.0)
            } else {
                std::mem::take(&mut w.unit_mut(h).unwrap().pathing_cells)
            };
            if let Some(p) = w.pathing.as_mut() {
                p.stamp(&cells, if on { 1 } else { -1 });
            }
            if on {
                w.unit_mut(h).unwrap().pathing_cells = cells;
            }
            none()
        },
        SetUnitUseFood => |w, _, a| { let on = a.b(1); with_unit(w, a, |u| u.extra.use_food = on); w.update_food(); none() },
        SetUnitCreepGuard => |w, _, a| { let on = a.b(1); with_unit(w, a, |u| u.extra.creep_guard = on) },
        UnitSetUsesAltIcon => |w, _, a| { let on = a.b(1); with_unit(w, a, |u| u.extra.alt_icon = on) },
        GetUnitRallyUnit => |w, _, a| handle(w.unit(a.h(0)).map(|u| u.extra.rally_target).filter(|&t| w.unit(t).is_some()).unwrap_or(0)),
        GetUnitRallyDestructable => |w, _, a| handle(w.unit(a.h(0)).map(|u| u.extra.rally_target).filter(|&t| matches!(w.handles.get(t), Some(crate::types::Obj::Destructable(_)))).unwrap_or(0)),
        IsUnitInForce => |w, _, a| {
            let Some(owner) = unit_arg(w, a, 0).map(|u| u.owner) else { return boolean(false) };
            boolean(matches!(w.handles.get(a.h(1)), Some(Obj::Force(f)) if f.players & (1 << owner) != 0))
        },
        IsUnitRace => |w, _, a| {
            let race = unit_arg(w, a, 0).and_then(|u| w.objects.units.get(u.type_id)).map(|o| o.str("race").to_ascii_lowercase()).unwrap_or_default();
            // RACE_HUMAN 1, ORC 2, UNDEAD 3, NIGHTELF 4, DEMON 5, OTHER 7, CREEP 8, CRITTER 9, NAGA 12.
            let want = match a.h(1) { 1 => "human", 2 => "orc", 3 => "undead", 4 => "nightelf", 5 => "demon", 8 => "creeps", 9 => "critters", 12 => "naga", _ => "other" };
            boolean(race == want)
        },
        IsUnitIllusion => |w, _, a| boolean(unit_arg(w, a, 0).is_some_and(|u| u.extra.illusion.is_some())),
        IsUnitInTransport => |w, _, a| { let t = a.h(1); boolean(unit_arg(w, a, 0).is_some_and(|u| u.extra.transport == t && t != 0)) },
        IsUnitLoaded => |w, _, a| boolean(unit_arg(w, a, 0).is_some_and(|u| u.extra.transport != 0)),
        UnitShareVision => |w, _, a| {
            let (Some(p), on) = (w.player_of(a.h(1)), a.b(2)) else { return none() };
            with_unit(w, a, |u| if on { u.extra.shared_vision |= 1 << p } else { u.extra.shared_vision &= !(1 << p) })
        },
        UnitSuspendDecay => |w, _, a| { let on = a.b(1); with_unit(w, a, |u| u.extra.suspend_decay = on) },
        UnitAddType => |w, _, a| {
            let t = a.h(1) as i32;
            let r = with_unit(w, a, |u| { u.extra.removed_types.retain(|&x| x != t); if !u.extra.added_types.contains(&t) { u.extra.added_types.push(t); } });
            let _ = r;
            boolean(true)
        },
        UnitRemoveType => |w, _, a| {
            let t = a.h(1) as i32;
            let _ = with_unit(w, a, |u| { u.extra.added_types.retain(|&x| x != t); if !u.extra.removed_types.contains(&t) { u.extra.removed_types.push(t); } });
            boolean(true)
        },
        // Buffs from an allied caster count as positive, others as negative.
        // Any helpful or harmful buff, auras' and timed lives too.
        UnitRemoveBuffs => |w, _, a| { remove_buffs(w, a.h(0), crate::bufftable::Filter { positive: a.b(1), negative: a.b(2), timed_life: true, aura: true, ..Default::default() }); none() },
        UnitRemoveBuffsEx => |w, _, a| { remove_buffs(w, a.h(0), buff_filter(a)); none() },
        UnitHasBuffsEx => |w, _, a| boolean(count_buffs(w, a.h(0), buff_filter(a)) > 0),
        UnitCountBuffsEx => |w, _, a| int(count_buffs(w, a.h(0), buff_filter(a))),
        UnitAddSleep => |w, _, a| { let on = a.b(1); with_unit(w, a, |u| u.extra.can_sleep = on) },
        UnitCanSleep => |w, _, a| boolean(unit_arg(w, a, 0).is_some_and(|u| u.extra.can_sleep)),
        UnitAddSleepPerm => |w, _, a| { let on = a.b(1); with_unit(w, a, |u| { u.extra.sleep_perm = on; if on { u.extra.sleeping = true; } }) },
        UnitCanSleepPerm => |w, _, a| boolean(unit_arg(w, a, 0).is_some_and(|u| u.extra.sleep_perm)),
        UnitIsSleeping => |w, _, a| boolean(unit_arg(w, a, 0).is_some_and(|u| u.extra.sleeping)),
        UnitWakeUp => |w, _, a| with_unit(w, a, |u| u.extra.sleeping = false),
        UnitApplyTimedLife => |w, _, a| { let d = a.r(2); with_unit(w, a, |u| u.timed_life = Some(d)) },
        UnitPauseTimedLife => |w, _, a| { let on = a.b(1); with_unit(w, a, |u| u.extra.timed_life_paused = on) },
        UnitIgnoreAlarm => |w, _, a| { let on = a.b(1); let _ = with_unit(w, a, |u| u.extra.ignore_alarm = on); boolean(true) },
        UnitIgnoreAlarmToggled => |w, _, a| boolean(unit_arg(w, a, 0).is_some_and(|u| u.extra.ignore_alarm)),
        UnitResetCooldown => |w, _, a| with_unit(w, a, |u| { u.cooldowns.clear(); u.cooldown = 0.0; }),
        UnitSetConstructionProgress => |w, _, a| {
            let p = (a.i(1) as f32 / 100.0).clamp(0.0, 1.0);
            with_unit(w, a, |u| if u.constructing.is_some() { u.constructing = Some(u.build_time * (1.0 - p)); })
        },
        UnitSetUpgradeProgress => |w, _, a| {
            let p = (a.i(1) as f32 / 100.0).clamp(0.0, 1.0);
            let id = unit_arg(w, a, 0).and_then(|u| u.queue.first().copied());
            let t = id.and_then(|id| w.objects.units.get(id)).map_or(0.0, |o| o.real("bldtm"));
            with_unit(w, a, |u| u.queue_progress = t * p)
        },
        // Damage from a script: ATTACK_TYPE_NORMAL (0) is spell damage, the rest take the
        // attack type's bonus against the target's armor type.
        UnitDamageTarget => |w, vm, a| {
            let (src, target, amount, atk, dt) = (a.h(0), a.h(1), a.r(2), a.h(5) as i32, a.h(6) as i32);
            if w.unit(target).is_some() {
                script_damage(w, vm, src, target, amount, atk, dt);
                boolean(true)
            } else if let Some(Obj::Destructable(d)) = w.handles.get_mut(target) {
                d.life -= amount;
                boolean(true)
            } else {
                boolean(false)
            }
        },
        UnitDamagePoint => |w, vm, a| {
            let (src, radius, x, y, amount, atk, dt) = (a.h(0), a.r(2), a.r(3), a.r(4), a.r(5), a.h(8) as i32, a.h(9) as i32);
            let targets: Vec<u32> = w.unit_order.iter().copied().filter(|&t| w.unit(t).is_some_and(|u| !u.dead && (u.x - x).hypot(u.y - y) <= radius)).collect();
            for t in targets {
                script_damage(w, vm, src, t, amount, atk, dt);
            }
            boolean(true)
        },
        IssueInstantPointOrder => |w, vm, a| issue_order(w, vm, a.h(0), crate::natives::basic::issue_order_id(w, &a.s(1)), a.r(2), a.r(3), 0),
        IssueInstantPointOrderById => |w, vm, a| issue_order(w, vm, a.h(0), a.i(1), a.r(2), a.r(3), 0),
        IssueInstantTargetOrder => |w, vm, a| issue_order(w, vm, a.h(0), crate::natives::basic::issue_order_id(w, &a.s(1)), 0.0, 0.0, a.h(2)),
        IssueInstantTargetOrderById => |w, vm, a| issue_order(w, vm, a.h(0), a.i(1), 0.0, 0.0, a.h(2)),
        IssueNeutralImmediateOrder => |w, vm, a| { let o = crate::natives::basic::issue_order_id(w, &a.s(2)); issue_neutral(w, vm, a, o, 0.0, 0.0, 0) },
        IssueNeutralImmediateOrderById => |w, vm, a| issue_neutral(w, vm, a, a.i(2), 0.0, 0.0, 0),
        IssueNeutralPointOrder => |w, vm, a| { let o = crate::natives::basic::issue_order_id(w, &a.s(2)); issue_neutral(w, vm, a, o, a.r(3), a.r(4), 0) },
        IssueNeutralPointOrderById => |w, vm, a| issue_neutral(w, vm, a, a.i(2), a.r(3), a.r(4), 0),
        IssueNeutralTargetOrder => |w, vm, a| { let o = crate::natives::basic::issue_order_id(w, &a.s(2)); issue_neutral(w, vm, a, o, 0.0, 0.0, a.h(3)) },
        IssueNeutralTargetOrderById => |w, vm, a| issue_neutral(w, vm, a, a.i(2), 0.0, 0.0, a.h(3)),
        // The structure by its name (UnitId), as jass_IssueBuildOrder looks it up.
        IssueBuildOrder => |w, _, a| {
            let id = crate::natives::basic::unit_id_by_name(w, &a.s(1));
            if id == 0 {
                return boolean(false);
            }
            issue_build(w, a.h(0), crate::objects::id_from_int(id), a.r(2), a.r(3))
        },
        IssueBuildOrderById => |w, _, a| { let id = id_from_int(a.i(1)); issue_build(w, a.h(0), id, a.r(2), a.r(3)) },
        WaygateGetDestinationX => |w, _, a| real(unit_arg(w, a, 0).and_then(|u| u.extra.waygate).map_or(0.0, |g| g.0)),
        WaygateGetDestinationY => |w, _, a| real(unit_arg(w, a, 0).and_then(|u| u.extra.waygate).map_or(0.0, |g| g.1)),
        WaygateSetDestination => |w, _, a| { let (x, y) = (a.r(1), a.r(2)); with_unit(w, a, |u| { let on = u.extra.waygate.is_some_and(|g| g.2); u.extra.waygate = Some((x, y, on)); }) },
        WaygateActivate => |w, _, a| { let on = a.b(1); with_unit(w, a, |u| { let (x, y, _) = u.extra.waygate.unwrap_or((u.x, u.y, false)); u.extra.waygate = Some((x, y, on)); }) },
        WaygateIsActive => |w, _, a| boolean(unit_arg(w, a, 0).and_then(|u| u.extra.waygate).is_some_and(|g| g.2)),
        SuspendHeroXP => |w, _, a| { if let Some(u) = w.unit_mut(a.h(0)) { u.xp_suspended = a.b(1); } none() },
        IsSuspendedXP => |w, _, a| boolean(unit_arg(w, a, 0).is_some_and(|u| u.xp_suspended)),
        IsHeroUnitId => |_, _, a| boolean(id_from_int(a.i(0))[0].is_ascii_uppercase()),
        IsUnitIdType => |w, _, a| {
            let id = id_from_int(a.i(0));
            let probe = Unit { type_id: id, is_hero: id[0].is_ascii_uppercase(), is_building: w.objects.units.get(id).is_some_and(|o| o.bool("isbldg")), ..dummy_unit() };
            boolean(is_unit_type(w, &probe, a.h(1) as i32))
        },
        IsUnitType => |w, _, a| boolean(unit_arg(w, a, 0).is_some_and(|u| is_unit_type(w, u, a.h(1) as i32))),
        GetOwningPlayer => |w, _, a| match unit_arg(w, a, 0) { Some(u) => NativeResult::Return(w.player_handle(u.owner)), None => none() },
        SetUnitOwner => |w, vm, a| {
            let Some(p) = w.player_of(a.h(1)) else { return none() };
            let change_color = a.b(2);
            let color = w.players[p as usize].color;
            let prev = unit_arg(w, a, 0).map(|u| u.owner);
            with_unit(w, a, |u| { u.owner = p; if change_color { u.color = color; } });
            w.update_food();
            if let Some(prev) = prev.filter(|&o| o != p) {
                let ctx = EventCtx { state: prev as i32, ..Default::default() };
                w.fire_unit_pair(vm, a.h(0), EVENT_PLAYER_UNIT_CHANGE_OWNER, EVENT_UNIT_CHANGE_OWNER, ctx);
            }
            none()
        },
        IsUnitOwnedByPlayer => |w, _, a| boolean(unit_arg(w, a, 0).is_some_and(|u| Some(u.owner) == w.player_of(a.h(1)))),
        IsUnitAlly => |w, _, a| {
            let (Some(u), Some(p)) = (unit_arg(w, a, 0).map(|u| u.owner), w.player_of(a.h(1))) else { return boolean(false) };
            boolean(is_ally(w, u, p))
        },
        IsUnitEnemy => |w, _, a| {
            let (Some(u), Some(p)) = (unit_arg(w, a, 0).map(|u| u.owner), w.player_of(a.h(1))) else { return boolean(false) };
            boolean(!is_ally(w, u, p))
        },
        IsUnitInRange => |w, _, a| {
            let (Some(u), Some(o)) = (unit_arg(w, a, 0), unit_arg(w, a, 1)) else { return boolean(false) };
            boolean((u.x - o.x).hypot(u.y - o.y) <= a.r(2))
        },
        IsUnitInRangeXY => |w, _, a| boolean(unit_arg(w, a, 0).is_some_and(|u| (u.x - a.r(1)).hypot(u.y - a.r(2)) <= a.r(3))),
        IsUnitInRangeLoc => |w, _, a| {
            let (x, y) = w.location(a.h(1)).unwrap_or_default();
            boolean(unit_arg(w, a, 0).is_some_and(|u| (u.x - x).hypot(u.y - y) <= a.r(2)))
        },
        IsUnit => |_, _, a| boolean(a.h(0) == a.h(1)),
        SetResourceAmount => |w, _, a| { let v = a.i(1); with_unit(w, a, |u| u.resource = v) },
        AddResourceAmount => |w, _, a| { let v = a.i(1); with_unit(w, a, |u| u.resource += v) },
        GetResourceAmount => |w, _, a| int(unit_arg(w, a, 0).map_or(0, |u| u.resource)),
        UnitAddAbility => |w, _, a| {
            let id = id_from_int(a.i(1));
            let Some(u) = w.unit_mut(a.h(0)) else { return boolean(false) };
            if u.abilities.iter().any(|(x, _)| *x == id) { return boolean(false); }
            u.abilities.push((id, 1));
            let type_id = u.type_id;
            if !crate::tags::AbilityObjects::get().refused(&id) {
                for c in w.ability_tags(type_id, &id) {
                    w.tags.take(&c);
                }
            }
            w.sync_spell_books(a.h(0));
            w.wander_paths(a.h(0));
            boolean(true)
        },
        UnitRemoveAbility => |w, _, a| {
            let id = id_from_int(a.i(1));
            let Some(u) = w.unit_mut(a.h(0)) else { return boolean(false) };
            let n = u.abilities.len();
            u.abilities.retain(|(x, _)| *x != id);
            let mut removed = u.abilities.len() != n;
            // Buffs are abilities to the original: removing a buff's id takes the buff.
            if !removed {
                let effects: Vec<u32> = u.buffs.iter().filter(|b| b.id == id).map(|b| b.effect).collect();
                u.buffs.retain(|b| b.id != id);
                removed = !effects.is_empty();
                for e in effects {
                    w.remove_buff_effect(e);
                }
            }
            w.sync_spell_books(a.h(0));
            boolean(removed)
        },
        // A buff's id reads level 1 while the unit has it, as buffs are abilities to the
        // original.
        // A buff counts as level 1, unless it is a class's own (Defend's: measured, a
        // defending footman's GetUnitAbilityLevel for 'Bdef' is 0).
        GetUnitAbilityLevel => |w, _, a| {
            let id = id_from_int(a.i(1));
            let hidden = |b: &crate::types::Buff| crate::ability::default_buff(&w.ability_code(b.ability)).is_some_and(|d| crate::objects::id_from_str(d) == b.id);
            int(unit_arg(w, a, 0).map_or(0, |u| u.abilities.iter().find(|(x, _)| *x == id).map_or_else(|| u.buffs.iter().any(|b| b.id == id && !hidden(b)) as i32, |(_, l)| *l)))
        },
        // Kept within 1 and the ability's level count (measured: 9 reads 3 for Holy Light,
        // 0 reads 1).
        SetUnitAbilityLevel => |w, _, a| {
            let id = id_from_int(a.i(1));
            let levels = w.objects.abilities.get(id).map_or(1, |o| o.int("levels").max(1));
            let l = a.i(2).clamp(1, levels);
            let Some(u) = w.unit_mut(a.h(0)) else { return int(0) };
            match u.abilities.iter_mut().find(|(x, _)| *x == id) { Some(e) => { e.1 = l; int(l) } None => int(0) }
        },
        IncUnitAbilityLevel => |w, _, a| {
            let id = id_from_int(a.i(1));
            let Some(u) = w.unit_mut(a.h(0)) else { return int(0) };
            match u.abilities.iter_mut().find(|(x, _)| *x == id) { Some(e) => { e.1 += 1; int(e.1) } None => int(0) }
        },
        DecUnitAbilityLevel => |w, _, a| {
            let id = id_from_int(a.i(1));
            let Some(u) = w.unit_mut(a.h(0)) else { return int(0) };
            match u.abilities.iter_mut().find(|(x, _)| *x == id) { Some(e) => { e.1 = (e.1 - 1).max(1); int(e.1) } None => int(0) }
        },
        UnitMakeAbilityPermanent => |w, _, a| {
            let (id, permanent) = (id_from_int(a.i(2)), a.b(1));
            let Some(u) = w.unit_mut(a.h(0)) else { return boolean(false) };
            if !u.abilities.iter().any(|x| x.0 == id) {
                return boolean(false);
            }
            let p = &mut u.extra.permanent;
            match p.iter_mut().find(|x| x.0 == id) {
                Some(x) => x.1 += if permanent { 1 } else { -1 },
                None => p.push((id, if permanent { 1 } else { -1 })),
            }
            boolean(true)
        },
        GetUnitCurrentOrder => |w, _, a| int(unit_arg(w, a, 0).map_or(0, |u| u.order.id)),
        IssueImmediateOrder => |w, vm, a| {
            let order = crate::natives::basic::issue_order_id(w, &a.s(1));
            tag_immediate_order(w, a.h(0), order);
            issue_order(w, vm, a.h(0), order, 0.0, 0.0, 0)
        },
        IssueImmediateOrderById => |w, vm, a| {
            tag_immediate_order(w, a.h(0), a.i(1));
            issue_order(w, vm, a.h(0), a.i(1), 0.0, 0.0, 0)
        },
        IssuePointOrder => |w, vm, a| issue_order(w, vm, a.h(0), crate::natives::basic::issue_order_id(w, &a.s(1)), a.r(2), a.r(3), 0),
        IssuePointOrderById => |w, vm, a| issue_order(w, vm, a.h(0), a.i(1), a.r(2), a.r(3), 0),
        IssuePointOrderLoc => |w, vm, a| {
            let (x, y) = w.location(a.h(2)).unwrap_or_default();
            issue_order(w, vm, a.h(0), crate::natives::basic::issue_order_id(w, &a.s(1)), x, y, 0)
        },
        IssuePointOrderByIdLoc => |w, vm, a| {
            let (x, y) = w.location(a.h(2)).unwrap_or_default();
            issue_order(w, vm, a.h(0), a.i(1), x, y, 0)
        },
        IssueTargetOrder => |w, vm, a| issue_order(w, vm, a.h(0), crate::natives::basic::issue_order_id(w, &a.s(1)), 0.0, 0.0, a.h(2)),
        IssueTargetOrderById => |w, vm, a| issue_order(w, vm, a.h(0), a.i(1), 0.0, 0.0, a.h(2)),
        GetIssuedOrderId => |w, _, _| int(w.ctx.order_id),
        GetOrderPointX => |w, _, _| real(w.ctx.order_x),
        GetOrderPointY => |w, _, _| real(w.ctx.order_y),
        GetOrderPointLoc => |w, _, _| { let (x, y) = (w.ctx.order_x, w.ctx.order_y); handle(w.insert_location(x, y)) },
        GetOrderTarget => |w, _, _| handle(w.ctx.target_unit),
        GetOrderTargetUnit => |w, _, _| handle(w.ctx.target_unit),
        GetOrderedUnit => |w, _, _| handle(w.ctx.unit),
        GetDyingUnit => |w, _, _| handle(w.ctx.unit),
        GetKillingUnit => |w, _, _| handle(w.ctx.other_unit),
        GetAttacker => |w, _, _| handle(w.ctx.other_unit),
        GetEnumUnit => |w, _, _| handle(w.enum_ctx.enum_unit),
        GetFilterUnit => |w, _, _| handle(w.enum_ctx.filter_unit),
        CreateGroup => |w, _, _| {
            let slot = w.tags.take(b"+grp");
            let h = w.handles.insert(Obj::Group(Group::default()));
            w.tags.handle_slots.insert(h, slot);
            handle(h)
        },
        DestroyGroup => |w, _, a| {
            if w.group(a.h(0)).is_some() {
                w.handles.remove(a.h(0));
                w.tags.doom_handle(a.h(0));
            }
            none()
        },
        GroupAddUnit => |w, _, a| {
            let u = a.h(1);
            if w.unit(u).is_none() { return none(); }
            if let Some(g) = w.group_mut(a.h(0)) { if !g.units.contains(&u) { g.units.push(u); } }
            none()
        },
        GroupRemoveUnit => |w, _, a| { let u = a.h(1); if let Some(g) = w.group_mut(a.h(0)) { g.units.retain(|x| *x != u); } none() },
        GroupClear => |w, _, a| { if let Some(g) = w.group_mut(a.h(0)) { g.units.clear(); } none() },
        IsUnitInGroup => |w, _, a| boolean(w.group(a.h(1)).is_some_and(|g| g.units.contains(&a.h(0)))),
        FirstOfGroup => |w, _, a| {
            let first = w.group(a.h(0)).and_then(|g| {
                if g.units.first() == Some(&GROUP_GHOST) {
                    return None;
                }
                g.units.iter().copied().find(|&u| w.unit(u).is_some())
            });
            handle(first.unwrap_or(0))
        },
        ForGroup => for_group,
        GroupEnumUnitsOfType => |w, vm, a| {
            let name = a.s(1);
            enum_units(w, vm, a.h(0), a.h(2), -1, |w, u| w.objects.units.get(u.type_id).is_some_and(|o| o.str("unitName").eq_ignore_ascii_case(&name)));
            none()
        },
        GroupEnumUnitsOfTypeCounted => |w, vm, a| {
            let name = a.s(1);
            enum_units(w, vm, a.h(0), a.h(2), a.i(3), |w, u| w.objects.units.get(u.type_id).is_some_and(|o| o.str("unitName").eq_ignore_ascii_case(&name)));
            none()
        },
        GroupEnumUnitsOfPlayer => |w, vm, a| {
            let Some(p) = w.player_of(a.h(1)) else { return none() };
            enum_units(w, vm, a.h(0), a.h(2), -1, |_, u| u.owner == p);
            none()
        },
        GroupEnumUnitsInRect => |w, vm, a| {
            let Some(r) = w.rect(a.h(1)) else { return none() };
            enum_units(w, vm, a.h(0), a.h(2), -1, |w, u| r.contains(u.x, u.y) && !w.is_locust(u));
            none()
        },
        GroupEnumUnitsInRectCounted => |w, vm, a| {
            let Some(r) = w.rect(a.h(1)) else { return none() };
            enum_units(w, vm, a.h(0), a.h(2), a.i(3), |w, u| r.contains(u.x, u.y) && !w.is_locust(u));
            none()
        },
        GroupEnumUnitsInRange => |w, vm, a| {
            let (x, y, r) = (a.r(1), a.r(2), a.r(3));
            enum_units(w, vm, a.h(0), a.h(4), -1, |w, u| (u.x - x).hypot(u.y - y) <= r && !w.is_locust(u));
            none()
        },
        GroupEnumUnitsInRangeOfLoc => |w, vm, a| {
            let (x, y) = w.location(a.h(1)).unwrap_or_default();
            let r = a.r(2);
            enum_units(w, vm, a.h(0), a.h(3), -1, |w, u| (u.x - x).hypot(u.y - y) <= r && !w.is_locust(u));
            none()
        },
        GroupEnumUnitsInRangeOfLocCounted => |w, vm, a| {
            let (x, y) = w.location(a.h(1)).unwrap_or_default();
            let r = a.r(2);
            enum_units(w, vm, a.h(0), a.h(3), a.i(4), |w, u| (u.x - x).hypot(u.y - y) <= r && !w.is_locust(u));
            none()
        },
        GroupEnumUnitsInRangeCounted => |w, vm, a| {
            let (x, y, r) = (a.r(1), a.r(2), a.r(3));
            enum_units(w, vm, a.h(0), a.h(4), a.i(5), |w, u| (u.x - x).hypot(u.y - y) <= r && !w.is_locust(u));
            none()
        },
        GroupEnumUnitsSelected => |w, vm, a| {
            let Some(p) = w.player_of(a.h(1)) else { return none() };
            let sel = w.selection.get(&p).cloned().unwrap_or_default();
            enum_units(w, vm, a.h(0), a.h(2), -1, |_, u| sel.contains(&u.handle));
            none()
        },
        GroupImmediateOrder => |w, vm, a| {
            let order = crate::natives::basic::issue_order_id(w, &a.s(1));
            for u in w.group(a.h(0)).map(|g| g.units.clone()).unwrap_or_default() { issue_order(w, vm, u, order, 0.0, 0.0, 0); }
            boolean(true)
        },
        GroupImmediateOrderById => |w, vm, a| {
            for u in w.group(a.h(0)).map(|g| g.units.clone()).unwrap_or_default() { issue_order(w, vm, u, a.i(1), 0.0, 0.0, 0); }
            boolean(true)
        },
        GroupPointOrder => |w, vm, a| {
            let order = crate::natives::basic::issue_order_id(w, &a.s(1));
            group_point_order(w, vm, a.h(0), order, a.r(2), a.r(3))
        },
        GroupPointOrderById => |w, vm, a| group_point_order(w, vm, a.h(0), a.i(1), a.r(2), a.r(3)),
        GroupPointOrderLoc => |w, vm, a| {
            let order = crate::natives::basic::issue_order_id(w, &a.s(1));
            let (x, y) = w.location(a.h(2)).unwrap_or_default();
            group_point_order(w, vm, a.h(0), order, x, y)
        },
        GroupPointOrderByIdLoc => |w, vm, a| {
            let (x, y) = w.location(a.h(2)).unwrap_or_default();
            group_point_order(w, vm, a.h(0), a.i(1), x, y)
        },
        GroupTargetOrder => |w, vm, a| {
            let order = crate::natives::basic::issue_order_id(w, &a.s(1));
            for u in w.group(a.h(0)).map(|g| g.units.clone()).unwrap_or_default() { issue_order(w, vm, u, order, 0.0, 0.0, a.h(2)); }
            boolean(true)
        },
        GroupTargetOrderById => |w, vm, a| {
            for u in w.group(a.h(0)).map(|g| g.units.clone()).unwrap_or_default() { issue_order(w, vm, u, a.i(1), 0.0, 0.0, a.h(2)); }
            boolean(true)
        },
        SelectUnit => |w, _, a| {
            let (u, on) = (a.h(0), a.b(1));
            let Some(owner) = w.unit(u).map(|x| x.owner) else { return none() };
            let _ = owner;
            let local = w.local_player;
            let sel = w.selection.entry(local).or_default();
            if on { if !sel.contains(&u) { sel.push(u); } } else { sel.retain(|x| *x != u); }
            w.selection_serial += 1;
            none()
        },
        ClearSelection => |w, _, _| { let local = w.local_player; w.selection.remove(&local); w.selection_serial += 1; none() },
        IsUnitSelected => |w, _, a| {
            let Some(p) = w.player_of(a.h(1)) else { return boolean(false) };
            boolean(w.selection.get(&p).is_some_and(|s| s.contains(&a.h(0))))
        },
        GetPlayerUnitCount => |w, _, a| {
            let Some(p) = w.player_of(a.h(0)) else { return int(0) };
            int(w.unit_order.iter().filter(|&&h| w.unit(h).is_some_and(|u| u.owner == p && !u.dead)).count() as i32)
        },
        GetPlayerTypedUnitCount => |w, _, a| {
            let Some(p) = w.player_of(a.h(0)) else { return int(0) };
            let name = a.s(1);
            int(w.unit_order.iter().filter(|&&h| w.unit(h).is_some_and(|u| u.owner == p && !u.dead && w.objects.units.get(u.type_id).is_some_and(|o| o.str("unitName").eq_ignore_ascii_case(&name)))).count() as i32)
        },
        GetPlayerStructureCount => |w, _, a| {
            let Some(p) = w.player_of(a.h(0)) else { return int(0) };
            int(w.unit_order.iter().filter(|&&h| w.unit(h).is_some_and(|u| u.owner == p && !u.dead && u.is_building)).count() as i32)
        },
        GetUnitRallyPoint => |w, _, a| {
            let Some((x, y)) = unit_arg(w, a, 0).and_then(|u| u.rally) else { return none() };
            handle(w.insert_location(x, y))
        },
    );
}

fn dummy_unit() -> Unit {
    Unit {
        handle: 0, type_id: [0; 4], owner: 0, x: 0.0, y: 0.0, fly_height: 0.0, facing: 0.0, life: 0.0, max_life: 0.0,
        mana: 0.0, max_mana: 0.0, color: 0, scale: 1.0, vertex_color: [255; 4], user_data: 0, acquire_range: 0.0,
        move_speed: 0.0, turn_speed: 0.0, prop_window: 0.0, shown_facing: None, paused: false, hidden: false, invulnerable: false,
        dead: false, level: 0, hero_xp: 0, resource: 0, abilities: vec![], order: Order::default(),
        time_scale: 1.0, inventory: [0; 6], rally: None, explode_on_death: false, is_building: false, is_hero: false,
        collision: 0.0, action: Action::Idle, path: vec![], repath_timer: 0.0, cooldown: 0.0, swing: None,
        acquire_timer: 0.0, anim: String::new(), anim_serial: 0, death_time: 0.0, pathing_cells: vec![], build_cells: vec![],
        carry_gold: 0, carry_lumber: 0, work_timer: 0.0, harvest_source: 0, inside: 0,
        queue: vec![], queue_progress: 0.0, constructing: None, build_time: 0.0, building: 0,
        str_: 0.0, agi: 0.0, int: 0.0, skill_points: 0, regen_timer: 0.0,
        cooldowns: HashMap::default(), autocast: vec![], buffs: vec![], casting: None, timed_life: None,
        item_bonus: Default::default(), revive: None, reviving: false, channel: None, reincarnate: None, proper_name: String::new(), xp_suspended: false, queue_started: false, extra: Default::default(),
    }
}

/// A unit SetUnitPosition places stops what it was doing.
fn placed_stop(u: &mut crate::types::Unit) {
    u.path.clear();
    u.queue.clear();
    u.extra.queue_tokens.clear();
    if matches!(u.action, crate::types::Action::Move { .. } | crate::types::Action::AttackMove { .. } | crate::types::Action::Attack { .. } | crate::types::Action::Follow { .. } | crate::types::Action::Patrol { .. }) {
        u.action = crate::types::Action::Idle;
        u.order = crate::types::Order::default();
        if u.anim.starts_with("walk") {
            crate::sim::set_anim(u, "stand");
        }
    }
}
