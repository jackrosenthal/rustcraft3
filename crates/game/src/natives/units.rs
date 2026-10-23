//! Units and unit groups.

use std::collections::HashMap;

use w3_jass::{NativeResult, Value, Vm};

use super::{Args, boolean, handle, int, none, real, reg, string};
use crate::event_ids::*;
use crate::natives::players::is_ally;
use crate::objects::{Id, id_from_int, id_to_int};
use crate::types::*;
use crate::world::{NativeFn, World};

/// Creates a unit of a type for a player, returning its handle (0 if the type is unknown).
pub fn create_unit(w: &mut World, owner: u8, type_id: Id, x: f32, y: f32, facing: f32) -> u32 {
    let Some(o) = w.objects.units.get(type_id) else {
        log::warn!("CreateUnit: unknown unit type {}", crate::objects::id_str(type_id));
        return 0;
    };
    let hp = o.real("HP").max(1.0);
    let max_mana = o.real("manaN");
    let mana = o.real("mana0").min(max_mana.max(0.0));
    let mut abilities: Vec<(Id, i32)> = o
        .list("abilList")
        .iter()
        .map(|s| (crate::objects::id_from_str(s.trim()), 1))
        .collect();
    abilities.retain(|(id, _)| id[0] != 0);
    let is_hero = type_id[0].is_ascii_uppercase();
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
        acquire_range: o.real("acquire"),
        move_speed: o.real("spd"),
        turn_speed: o.real("turnRate"),
        prop_window: o.real("propWin"),
        paused: false,
        hidden: false,
        invulnerable: false,
        dead: false,
        level: o.int("level"),
        hero_xp: 0,
        resource: 0,
        abilities,
        order: Order::default(),
        animation: None,
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
    };
    let path_tex = if unit.is_building { o.str("pathTex") } else { String::new() };
    let mut unit = unit;
    // Gold mines start with their ability's gold (Agld DataA).
    for (a, _) in &unit.abilities {
        if let Some(ab) = w.objects.abilities.get(*a) {
            if ab.str("code") == "Agld" {
                unit.resource = ab.int("DataA1");
            }
        }
    }
    let h = w.handles.insert(Obj::Unit(Box::new(unit)));
    if let Some(u) = w.unit_mut(h) {
        u.handle = h;
    }
    if is_hero {
        w.apply_hero_stats(h, true);
    }
    w.unit_order.push(h);
    if !path_tex.is_empty() {
        // Structures are always placed facing BuildingAngle; their footprint does not turn.
        let cells = w.pathing_cells(&path_tex, x, y, 0.0);
        if let Some(p) = w.pathing.as_mut() {
            p.stamp(&cells, 1);
        }
        if let Some(u) = w.unit_mut(h) {
            u.pathing_cells = cells;
        }
    }
    h
}

/// Kills a unit and fires its death events.
pub fn kill_unit(w: &mut World, vm: &mut Vm, h: u32, killer: u32) {
    if w.unit(h).is_some_and(|u| !u.dead) && killer != 0 {
        w.grant_kill_xp(h, killer);
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
    let owner = u.owner;
    w.unit_mut(h).unwrap().death_time = w.time;
    let ctx = EventCtx {
        unit: h,
        widget: h,
        other_unit: killer,
        player: owner,
        has_player: true,
        ..Default::default()
    };
    w.fire(vm, EventCtx { event_id: EVENT_UNIT_DEATH, ..ctx.clone() }, |_, k| match k {
        EventKind::Unit { unit, event } => *unit == h && *event == EVENT_UNIT_DEATH,
        EventKind::Death(widget) => *widget == h,
        _ => false,
    });
    w.fire(vm, EventCtx { event_id: EVENT_PLAYER_UNIT_DEATH, ..ctx }, |_, k| match k {
        EventKind::PlayerUnit { player, event, .. } => *player == owner && *event == EVENT_PLAYER_UNIT_DEATH,
        _ => false,
    });
}

pub fn remove_unit(w: &mut World, h: u32) {
    if let Some(cells) = w.unit_mut(h).map(|u| std::mem::take(&mut u.pathing_cells)) {
        if let Some(p) = w.pathing.as_mut() {
            p.stamp(&cells, -1);
        }
    }
    if w.unit(h).is_some() {
        w.handles.remove(h);
        w.unit_order.retain(|x| *x != h);
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

fn with_unit(w: &mut World, a: &[Value], f: impl FnOnce(&mut Unit)) -> NativeResult {
    if let Some(u) = w.unit_mut(a.h(0)) {
        f(u);
    }
    none()
}

/// UNIT_TYPE_* (common.j ConvertUnitType values).  Classifications that depend on status
/// effects the simulation does not model yet read as false.
fn is_unit_type(w: &World, u: &Unit, t: i32) -> bool {
    let o = w.objects.units.get(u.type_id);
    let field = |f: &str| o.map(|o| o.str(f).to_ascii_lowercase()).unwrap_or_default();
    let classes = field("type");
    let has = |c: &str| classes.split(',').any(|x| x.trim() == c);
    let targets = format!("{},{}", field("targs1"), field("targs2"));
    match t {
        0 => u.is_hero,
        1 => u.dead,
        2 => u.is_building,
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

fn issue_order(w: &mut World, vm: &mut Vm, unit: u32, order: i32, x: f32, y: f32, target: u32) -> NativeResult {
    let Some(owner) = w.unit(unit).map(|u| u.owner) else { return boolean(false) };
    w.order_unit(unit, order, x, y, target);
    let (event, ctx) = if target != 0 {
        (EVENT_PLAYER_UNIT_ISSUED_TARGET_ORDER, EventCtx { target_unit: target, ..Default::default() })
    } else if x != 0.0 || y != 0.0 {
        (EVENT_PLAYER_UNIT_ISSUED_POINT_ORDER, EventCtx::default())
    } else {
        (EVENT_PLAYER_UNIT_ISSUED_ORDER, EventCtx::default())
    };
    let ctx = EventCtx { unit, event_id: event, order_id: order, order_x: x, order_y: y, player: owner, has_player: true, ..ctx };
    w.fire(vm, ctx, move |_, k| matches!(k, EventKind::PlayerUnit { player, event: e, .. } if *player == owner && *e == event));
    boolean(true)
}

pub fn register(t: &mut HashMap<&'static str, NativeFn>) {
    reg!(t,
        CreateUnit => |w, _, a| {
            let Some(p) = w.player_of(a.h(0)) else { return none() };
            handle(create_unit(w, p, id_from_int(a.i(1)), a.r(2), a.r(3), a.r(4)))
        },
        CreateUnitByName => |w, _, a| {
            let Some(p) = w.player_of(a.h(0)) else { return none() };
            let name = a.s(1);
            let id = w.objects.units.objects.iter().find(|(_, o)| o.str("unitName").eq_ignore_ascii_case(&name)).map(|(id, _)| *id);
            match id { Some(id) => handle(create_unit(w, p, id, a.r(2), a.r(3), a.r(4))), None => none() }
        },
        CreateUnitAtLoc => |w, _, a| {
            let Some(p) = w.player_of(a.h(0)) else { return none() };
            let (x, y) = w.location(a.h(2)).unwrap_or_default();
            handle(create_unit(w, p, id_from_int(a.i(1)), x, y, a.r(3)))
        },
        CreateUnitAtLocByName => |w, _, a| {
            let Some(p) = w.player_of(a.h(0)) else { return none() };
            let (x, y) = w.location(a.h(2)).unwrap_or_default();
            let name = a.s(1);
            let id = w.objects.units.objects.iter().find(|(_, o)| o.str("unitName").eq_ignore_ascii_case(&name)).map(|(id, _)| *id);
            match id { Some(id) => handle(create_unit(w, p, id, x, y, a.r(3))), None => none() }
        },
        CreateCorpse => |w, _, a| {
            let Some(p) = w.player_of(a.h(0)) else { return none() };
            let h = create_unit(w, p, id_from_int(a.i(1)), a.r(2), a.r(3), a.r(4));
            if let Some(u) = w.unit_mut(h) { u.dead = true; u.life = 0.0; }
            handle(h)
        },
        KillUnit => |w, vm, a| { kill_unit(w, vm, a.h(0), 0); none() },
        RemoveUnit => |w, _, a| { remove_unit(w, a.h(0)); none() },
        ShowUnit => |w, _, a| { let v = a.b(1); with_unit(w, a, |u| u.hidden = !v) },
        IsUnitHidden => |w, _, a| boolean(unit_arg(w, a, 0).is_some_and(|u| u.hidden)),
        SetUnitState => |w, _, a| {
            let (s, v) = (a.h(1) as i32, a.r(2));
            with_unit(w, a, |u| match s {
                unit_state::LIFE => u.life = v.min(u.max_life),
                unit_state::MAX_LIFE => u.max_life = v,
                unit_state::MANA => u.mana = v.clamp(0.0, u.max_mana),
                unit_state::MAX_MANA => u.max_mana = v,
                _ => {}
            })
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
            handle(w.handles.insert(Obj::Location(x, y)))
        },
        GetUnitFacing => |w, _, a| real(unit_arg(w, a, 0).map_or(0.0, |u| u.facing)),
        GetUnitMoveSpeed => |w, _, a| real(unit_arg(w, a, 0).map_or(0.0, |u| u.move_speed)),
        GetUnitDefaultMoveSpeed => |w, _, a| real(unit_arg(w, a, 0).and_then(|u| w.objects.units.get(u.type_id)).map_or(0.0, |o| o.real("spd"))),
        GetUnitTurnSpeed => |w, _, a| real(unit_arg(w, a, 0).map_or(0.0, |u| u.turn_speed)),
        GetUnitPropWindow => |w, _, a| real(unit_arg(w, a, 0).map_or(0.0, |u| u.prop_window.to_radians())),
        GetUnitFlyHeight => |w, _, a| real(unit_arg(w, a, 0).map_or(0.0, |u| u.fly_height)),
        GetUnitAcquireRange => |w, _, a| real(unit_arg(w, a, 0).map_or(0.0, |u| u.acquire_range)),
        SetUnitX => |w, _, a| { let v = a.r(1); with_unit(w, a, |u| u.x = v) },
        SetUnitY => |w, _, a| { let v = a.r(1); with_unit(w, a, |u| u.y = v) },
        SetUnitPosition => |w, _, a| { let (x, y) = (a.r(1), a.r(2)); with_unit(w, a, |u| { u.x = x; u.y = y; }) },
        SetUnitPositionLoc => |w, _, a| {
            let (x, y) = w.location(a.h(1)).unwrap_or_default();
            with_unit(w, a, |u| { u.x = x; u.y = y; })
        },
        SetUnitFacing => |w, _, a| { let v = a.r(1); with_unit(w, a, |u| u.facing = v) },
        SetUnitFacingTimed => |w, _, a| { let v = a.r(1); with_unit(w, a, |u| u.facing = v) },
        SetUnitMoveSpeed => |w, _, a| { let v = a.r(1); with_unit(w, a, |u| u.move_speed = v) },
        SetUnitFlyHeight => |w, _, a| { let v = a.r(1); with_unit(w, a, |u| u.fly_height = v) },
        SetUnitTurnSpeed => |w, _, a| { let v = a.r(1); with_unit(w, a, |u| u.turn_speed = v) },
        SetUnitPropWindow => |w, _, a| { let v = a.r(1).to_degrees(); with_unit(w, a, |u| u.prop_window = v) },
        SetUnitAcquireRange => |w, _, a| { let v = a.r(1); with_unit(w, a, |u| u.acquire_range = v) },
        SetUnitScale => |w, _, a| { let v = a.r(1); with_unit(w, a, |u| u.scale = v) },
        SetUnitTimeScale => |w, _, a| { let v = a.r(1); with_unit(w, a, |u| u.time_scale = v) },
        SetUnitVertexColor => |w, _, a| {
            let c = [a.i(1) as u8, a.i(2) as u8, a.i(3) as u8, a.i(4) as u8];
            with_unit(w, a, |u| u.vertex_color = c)
        },
        SetUnitColor => |w, _, a| { let c = a.h(1) as i32; with_unit(w, a, |u| u.color = c) },
        SetUnitAnimation => |w, _, a| { let s = a.s(1); with_unit(w, a, |u| u.animation = Some(s)) },
        SetUnitAnimationByIndex => |_, _, _| none(),
        SetUnitAnimationWithRarity => |w, _, a| { let s = a.s(1); with_unit(w, a, |u| u.animation = Some(s)) },
        QueueUnitAnimation => |w, _, a| { let s = a.s(1); with_unit(w, a, |u| u.animation = Some(s)) },
        AddUnitAnimationProperties => |_, _, _| none(),
        SetUnitBlendTime => |_, _, _| none(),
        SetUnitLookAt => |_, _, _| none(),
        ResetUnitLookAt => |_, _, _| none(),
        SetUnitInvulnerable => |w, _, a| { let v = a.b(1); with_unit(w, a, |u| u.invulnerable = v) },
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
        AddHeroXP => |w, _, a| { w.add_hero_xp(a.h(0), a.i(1)); none() },
        SetHeroXP => |w, _, a| {
            let cur = unit_arg(w, a, 0).map_or(0, |u| u.hero_xp);
            if a.i(1) > cur { w.add_hero_xp(a.h(0), a.i(1) - cur); }
            none()
        },
        SetHeroLevel => |w, _, a| {
            let (h, l) = (a.h(0), a.i(1));
            while w.unit(h).is_some_and(|u| u.is_hero && u.level < l) {
                let need = w.hero_xp_needed(w.unit(h).unwrap().level + 1) - w.unit(h).unwrap().hero_xp;
                w.add_hero_xp(h, need.max(1));
            }
            none()
        },
        GetHeroStr => |w, _, a| int(unit_arg(w, a, 0).map_or(0, |u| u.str_ as i32)),
        GetHeroAgi => |w, _, a| int(unit_arg(w, a, 0).map_or(0, |u| u.agi as i32)),
        GetHeroInt => |w, _, a| int(unit_arg(w, a, 0).map_or(0, |u| u.int as i32)),
        GetHeroSkillPoints => |w, _, a| int(unit_arg(w, a, 0).map_or(0, |u| u.skill_points)),
        UnitModifySkillPoints => |w, _, a| {
            let d = a.i(1);
            let Some(u) = w.unit_mut(a.h(0)) else { return boolean(false) };
            u.skill_points = (u.skill_points + d).max(0);
            boolean(true)
        },
        SelectHeroSkill => |w, _, a| { w.learn_skill(a.h(0), id_from_int(a.i(1))); none() },
        IsHeroUnitId => |_, _, a| boolean(id_from_int(a.i(0))[0].is_ascii_uppercase()),
        IsUnitIdType => |w, _, a| {
            let id = id_from_int(a.i(0));
            let probe = Unit { type_id: id, is_hero: id[0].is_ascii_uppercase(), is_building: w.objects.units.get(id).is_some_and(|o| o.bool("isbldg")), ..dummy_unit() };
            boolean(is_unit_type(w, &probe, a.h(1) as i32))
        },
        IsUnitType => |w, _, a| boolean(unit_arg(w, a, 0).is_some_and(|u| is_unit_type(w, u, a.h(1) as i32))),
        GetOwningPlayer => |w, _, a| match unit_arg(w, a, 0) { Some(u) => NativeResult::Return(w.player_handle(u.owner)), None => none() },
        SetUnitOwner => |w, _, a| {
            let Some(p) = w.player_of(a.h(1)) else { return none() };
            let change_color = a.b(2);
            let color = w.players[p as usize].color;
            with_unit(w, a, |u| { u.owner = p; if change_color { u.color = color; } })
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
            boolean(true)
        },
        UnitRemoveAbility => |w, _, a| {
            let id = id_from_int(a.i(1));
            let Some(u) = w.unit_mut(a.h(0)) else { return boolean(false) };
            let n = u.abilities.len();
            u.abilities.retain(|(x, _)| *x != id);
            boolean(u.abilities.len() != n)
        },
        GetUnitAbilityLevel => |w, _, a| {
            let id = id_from_int(a.i(1));
            int(unit_arg(w, a, 0).and_then(|u| u.abilities.iter().find(|(x, _)| *x == id)).map_or(0, |(_, l)| *l))
        },
        SetUnitAbilityLevel => |w, _, a| {
            let (id, l) = (id_from_int(a.i(1)), a.i(2));
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
        UnitMakeAbilityPermanent => |_, _, _| boolean(true),
        GetUnitCurrentOrder => |w, _, a| int(unit_arg(w, a, 0).map_or(0, |u| u.order.id)),
        IssueImmediateOrder => |w, vm, a| issue_order(w, vm, a.h(0), crate::natives::basic::order_id(&a.s(1)), 0.0, 0.0, 0),
        IssueImmediateOrderById => |w, vm, a| issue_order(w, vm, a.h(0), a.i(1), 0.0, 0.0, 0),
        IssuePointOrder => |w, vm, a| issue_order(w, vm, a.h(0), crate::natives::basic::order_id(&a.s(1)), a.r(2), a.r(3), 0),
        IssuePointOrderById => |w, vm, a| issue_order(w, vm, a.h(0), a.i(1), a.r(2), a.r(3), 0),
        IssuePointOrderLoc => |w, vm, a| {
            let (x, y) = w.location(a.h(2)).unwrap_or_default();
            issue_order(w, vm, a.h(0), crate::natives::basic::order_id(&a.s(1)), x, y, 0)
        },
        IssuePointOrderByIdLoc => |w, vm, a| {
            let (x, y) = w.location(a.h(2)).unwrap_or_default();
            issue_order(w, vm, a.h(0), a.i(1), x, y, 0)
        },
        IssueTargetOrder => |w, vm, a| issue_order(w, vm, a.h(0), crate::natives::basic::order_id(&a.s(1)), 0.0, 0.0, a.h(2)),
        IssueTargetOrderById => |w, vm, a| issue_order(w, vm, a.h(0), a.i(1), 0.0, 0.0, a.h(2)),
        GetIssuedOrderId => |w, _, _| int(w.ctx.order_id),
        GetOrderPointX => |w, _, _| real(w.ctx.order_x),
        GetOrderPointY => |w, _, _| real(w.ctx.order_y),
        GetOrderPointLoc => |w, _, _| { let (x, y) = (w.ctx.order_x, w.ctx.order_y); handle(w.handles.insert(Obj::Location(x, y))) },
        GetOrderTarget => |w, _, _| handle(w.ctx.target_unit),
        GetOrderTargetUnit => |w, _, _| handle(w.ctx.target_unit),
        GetOrderedUnit => |w, _, _| handle(w.ctx.unit),
        GetDyingUnit => |w, _, _| handle(w.ctx.unit),
        GetKillingUnit => |w, _, _| handle(w.ctx.other_unit),
        GetAttacker => |w, _, _| handle(w.ctx.other_unit),
        GetEnumUnit => |w, _, _| handle(w.enum_ctx.enum_unit),
        GetFilterUnit => |w, _, _| handle(w.enum_ctx.filter_unit),
        CreateGroup => |w, _, _| handle(w.handles.insert(Obj::Group(Group::default()))),
        DestroyGroup => |w, _, a| { if w.group(a.h(0)).is_some() { w.handles.remove(a.h(0)); } none() },
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
            let first = w.group(a.h(0)).and_then(|g| g.units.iter().copied().find(|&u| w.unit(u).is_some()));
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
            enum_units(w, vm, a.h(0), a.h(2), -1, |_, u| r.contains(u.x, u.y));
            none()
        },
        GroupEnumUnitsInRectCounted => |w, vm, a| {
            let Some(r) = w.rect(a.h(1)) else { return none() };
            enum_units(w, vm, a.h(0), a.h(2), a.i(3), |_, u| r.contains(u.x, u.y));
            none()
        },
        GroupEnumUnitsInRange => |w, vm, a| {
            let (x, y, r) = (a.r(1), a.r(2), a.r(3));
            enum_units(w, vm, a.h(0), a.h(4), -1, |_, u| (u.x - x).hypot(u.y - y) <= r);
            none()
        },
        GroupEnumUnitsInRangeOfLoc => |w, vm, a| {
            let (x, y) = w.location(a.h(1)).unwrap_or_default();
            let r = a.r(2);
            enum_units(w, vm, a.h(0), a.h(3), -1, |_, u| (u.x - x).hypot(u.y - y) <= r);
            none()
        },
        GroupEnumUnitsInRangeCounted => |w, vm, a| {
            let (x, y, r) = (a.r(1), a.r(2), a.r(3));
            enum_units(w, vm, a.h(0), a.h(4), a.i(5), |_, u| (u.x - x).hypot(u.y - y) <= r);
            none()
        },
        GroupEnumUnitsSelected => |w, vm, a| {
            let Some(p) = w.player_of(a.h(1)) else { return none() };
            let sel = w.selection.get(&p).cloned().unwrap_or_default();
            enum_units(w, vm, a.h(0), a.h(2), -1, |_, u| sel.contains(&u.handle));
            none()
        },
        GroupImmediateOrder => |w, vm, a| {
            let order = crate::natives::basic::order_id(&a.s(1));
            for u in w.group(a.h(0)).map(|g| g.units.clone()).unwrap_or_default() { issue_order(w, vm, u, order, 0.0, 0.0, 0); }
            boolean(true)
        },
        GroupImmediateOrderById => |w, vm, a| {
            for u in w.group(a.h(0)).map(|g| g.units.clone()).unwrap_or_default() { issue_order(w, vm, u, a.i(1), 0.0, 0.0, 0); }
            boolean(true)
        },
        GroupPointOrder => |w, vm, a| {
            let order = crate::natives::basic::order_id(&a.s(1));
            for u in w.group(a.h(0)).map(|g| g.units.clone()).unwrap_or_default() { issue_order(w, vm, u, order, a.r(2), a.r(3), 0); }
            boolean(true)
        },
        GroupPointOrderById => |w, vm, a| {
            for u in w.group(a.h(0)).map(|g| g.units.clone()).unwrap_or_default() { issue_order(w, vm, u, a.i(1), a.r(2), a.r(3), 0); }
            boolean(true)
        },
        GroupPointOrderLoc => |w, vm, a| {
            let order = crate::natives::basic::order_id(&a.s(1));
            let (x, y) = w.location(a.h(2)).unwrap_or_default();
            for u in w.group(a.h(0)).map(|g| g.units.clone()).unwrap_or_default() { issue_order(w, vm, u, order, x, y, 0); }
            boolean(true)
        },
        GroupPointOrderByIdLoc => |w, vm, a| {
            let (x, y) = w.location(a.h(2)).unwrap_or_default();
            for u in w.group(a.h(0)).map(|g| g.units.clone()).unwrap_or_default() { issue_order(w, vm, u, a.i(1), x, y, 0); }
            boolean(true)
        },
        GroupTargetOrder => |w, vm, a| {
            let order = crate::natives::basic::order_id(&a.s(1));
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
            none()
        },
        ClearSelection => |w, _, _| { let local = w.local_player; w.selection.remove(&local); none() },
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
            handle(w.handles.insert(Obj::Location(x, y)))
        },
    );
}

fn dummy_unit() -> Unit {
    Unit {
        handle: 0, type_id: [0; 4], owner: 0, x: 0.0, y: 0.0, fly_height: 0.0, facing: 0.0, life: 0.0, max_life: 0.0,
        mana: 0.0, max_mana: 0.0, color: 0, scale: 1.0, vertex_color: [255; 4], user_data: 0, acquire_range: 0.0,
        move_speed: 0.0, turn_speed: 0.0, prop_window: 0.0, paused: false, hidden: false, invulnerable: false,
        dead: false, level: 0, hero_xp: 0, resource: 0, abilities: vec![], order: Order::default(), animation: None,
        time_scale: 1.0, inventory: [0; 6], rally: None, explode_on_death: false, is_building: false, is_hero: false,
        collision: 0.0, action: Action::Idle, path: vec![], repath_timer: 0.0, cooldown: 0.0, swing: None,
        acquire_timer: 0.0, anim: String::new(), anim_serial: 0, death_time: 0.0, pathing_cells: vec![],
        carry_gold: 0, carry_lumber: 0, work_timer: 0.0, harvest_source: 0, inside: 0,
        queue: vec![], queue_progress: 0.0, constructing: None, build_time: 0.0, building: 0,
        str_: 0.0, agi: 0.0, int: 0.0, skill_points: 0, regen_timer: 0.0,
    }
}
