//! Destructables, items, and special effects.

use crate::collections::HashMap;

use w3_jass::{NativeResult, Value, Vm};

use super::{Args, boolean, handle, int, none, real, reg};
use crate::objects::{Id, id_from_int, id_to_int};
use crate::types::*;
use crate::world::{NativeFn, World};

pub fn create_destructable(w: &mut World, id: Id, x: f32, y: f32, z: Option<f32>, facing: f32, scale: f32, variation: u32) -> u32 {
    let Some(o) = w.objects.destructables.get(id) else { return 0 };
    let hp = o.real("HP");
    let nvar = o.int("numVar").max(1) as u32;
    let z = z.unwrap_or_else(|| w.ground_z(x, y));
    let d = Destructable {
        oid: (0, 0),
        handle: 0,
        type_id: id,
        x,
        y,
        z,
        facing,
        scale,
        variation: variation % nvar,
        life: hp,
        max_life: hp,
        invulnerable: false,
        visible: true,
        animation: None,
        anim_serial: 0,
        anim_speed: 1.0,
        pathing_cells: Vec::new(),
        occluder_height: None,
    };
    let path_tex = o.str("pathTex");
    let h = w.handles.insert(Obj::Destructable(Box::new(d)));
    // Its tags (read under Wine): itself, its position, five regions.
    let oid = w.tags.take_id(b"+w3d");
    w.tags.take(b"`pos");
    w.tags.take_n(b"^pmr", 5);
    if let Some(Obj::Destructable(d)) = w.handles.get_mut(h) {
        d.handle = h;
        d.oid = oid;
    }
    w.destructable_order.push(h);
    // A pathing texture is drawn for the facing structures stand at (270, as
    // BuildingAngle), and turns from there.
    let cells = w.pathing_cells(&path_tex, x, y, facing - 270.0);
    if let Some(p) = w.pathing.as_mut() {
        p.stamp_destructable(&cells, 1);
    }
    if let Some(Obj::Destructable(d)) = w.handles.get_mut(h) {
        d.pathing_cells = cells;
    }
    h
}

/// Puts back a revived destructable's pathing blockers.
fn restamp_destructable(w: &mut World, h: u32) {
    let Some((type_id, x, y, facing)) = dest(w, h).filter(|d| d.pathing_cells.is_empty()).map(|d| (d.type_id, d.x, d.y, d.facing)) else { return };
    let path_tex = w.objects.destructables.get(type_id).map(|o| o.str("pathTex")).unwrap_or_default();
    let cells = w.pathing_cells(&path_tex, x, y, facing - 270.0);
    if let Some(p) = w.pathing.as_mut() {
        p.stamp_destructable(&cells, 1);
    }
    if let Some(d) = dest_mut(w, h) {
        d.pathing_cells = cells;
    }
}

/// Where a script's destructable stands: snapped as a structure is, by its pathing
/// texture's size (measured: trees asked for at 1380, 930 and 1395.5, 947.3 stand at
/// 1344, 896, at -1380, -930 at -1344, -896; 2 by 2 barrels at 1530 at 1504, a gate,
/// rocks, and a sight blocker at 1472).
fn snapped(w: &mut World, id: i32, x: f32, y: f32) -> (f32, f32) {
    let tex = w.objects.destructables.get(id_from_int(id)).map(|o| o.str("pathTex")).unwrap_or_default();
    w.snap_to_texture(&tex, x, y)
}

/// Clears a destructable's pathing blockers (on death or removal).
pub fn unstamp_destructable(w: &mut World, h: u32) {
    let cells = match w.handles.get_mut(h) {
        Some(Obj::Destructable(d)) => std::mem::take(&mut d.pathing_cells),
        _ => return,
    };
    if let Some(p) = w.pathing.as_mut() {
        p.stamp_destructable(&cells, -1);
    }
}

fn dest(w: &World, h: u32) -> Option<&Destructable> {
    match w.handles.get(h) {
        Some(Obj::Destructable(d)) => Some(d),
        _ => None,
    }
}

fn dest_mut(w: &mut World, h: u32) -> Option<&mut Destructable> {
    match w.handles.get_mut(h) {
        Some(Obj::Destructable(d)) => Some(d),
        _ => None,
    }
}

pub fn create_item(w: &mut World, id: Id, x: f32, y: f32) -> u32 {
    let Some(o) = w.objects.items.get(id) else { return 0 };
    // Its tags (measured: CreateItem and UnitAddItemById each make these).
    let oid = w.tags.take_id(b"item");
    w.tags.take_all(&[b"`pos", b"^pmr", b"^pmr"]);
    let item = Item {
        handle: 0,
        oid,
        type_id: id,
        x,
        y,
        life: o.real("HP"),
        charges: o.int("uses"),
        player: crate::types::PLAYER_NEUTRAL_PASSIVE,
        owner_unit: 0,
        visible: true,
        invulnerable: false,
        user_data: 0,
        pawnable: o.bool("pawnable"),
        drop_on_death: false,
        droppable: o.bool("droppable"),
        drop_type: None,
    };
    let h = w.handles.insert(Obj::Item(Box::new(item)));
    if let Some(Obj::Item(i)) = w.handles.get_mut(h) {
        i.handle = h;
    }
    w.item_order.push(h);
    h
}

pub(crate) fn add_stock(w: &mut World, shop: u32, id: Id, current: i32, max: i32) {
    let regen = w.objects.items.get(id).or_else(|| w.objects.units.get(id)).map_or(0.0, |o| o.real("stockRegen"));
    w.shops.added.insert((shop, id), crate::shops::Stock { count: current, max, timer: regen, regen, arriving: false });
}

/// A random item type of a level (and class: ITEM_TYPE_* from common.j), among those
/// marked pickRandom in ItemData; -1 for any level.
fn choose_random_item(w: &mut World, class: Option<i32>, level: i32) -> i32 {
    // ITEM_TYPE_PERMANENT, CHARGED, POWERUP, ARTIFACT, PURCHASABLE, CAMPAIGN, MISCELLANEOUS.
    const CLASSES: [&str; 7] = ["permanent", "charged", "powerup", "artifact", "purchasable", "campaign", "miscellaneous"];
    let mut ids: Vec<Id> = w
        .objects
        .items
        .ids()
        .into_iter()
        .filter(|&id| {
            w.objects.items.get(id).is_some_and(|o| {
                o.bool("pickRandom")
                    && (level < 0 || o.int("Level") == level)
                    && class.is_none_or(|c| c == 8 || CLASSES.get(c as usize).is_some_and(|k| o.str("class").eq_ignore_ascii_case(k)))
            })
        })
        .collect();
    ids.sort();
    if ids.is_empty() {
        return 0;
    }
    let k = w.rng.int(0, ids.len() as i32 - 1) as usize;
    id_to_int(ids[k])
}

fn item(w: &World, h: u32) -> Option<&Item> {
    match w.handles.get(h) {
        Some(Obj::Item(i)) => Some(i),
        _ => None,
    }
}

fn item_mut(w: &mut World, h: u32) -> Option<&mut Item> {
    match w.handles.get_mut(h) {
        Some(Obj::Item(i)) => Some(i),
        _ => None,
    }
}

pub fn kill_destructable_pub(w: &mut World, vm: &mut Vm, h: u32) {
    kill_destructable(w, vm, h);
}

fn kill_destructable(w: &mut World, vm: &mut Vm, h: u32) {
    let Some(d) = dest_mut(w, h) else { return };
    if d.life <= 0.0 {
        return;
    }
    d.life = 0.0;
    unstamp_destructable(w, h);
    let ctx = EventCtx { destructable: h, widget: h, ..Default::default() };
    w.fire(vm, ctx, |_, k| matches!(k, EventKind::Death(x) if *x == h));
}

fn enum_destructables(w: &mut World, vm: &mut Vm, rect: u32, filter: u32, func: Option<w3_jass::FuncId>) -> NativeResult {
    let Some(r) = w.rect(rect) else { return none() };
    let list: Vec<u32> = w.destructable_order.iter().copied().filter(|&h| dest(w, h).is_some_and(|d| r.contains(d.x, d.y))).collect();
    for h in list {
        let saved = w.enum_ctx;
        w.enum_ctx.filter_destructable = h;
        let ok = filter == 0 || w.eval_boolexpr(vm, filter);
        w.enum_ctx = saved;
        if ok {
            if let Some(f) = func {
                let saved = w.enum_ctx;
                w.enum_ctx.enum_destructable = h;
                let _ = vm.call(w, f, &[]);
                w.enum_ctx = saved;
            }
        }
    }
    none()
}

fn enum_items(w: &mut World, vm: &mut Vm, rect: u32, filter: u32, func: Option<w3_jass::FuncId>) -> NativeResult {
    let Some(r) = w.rect(rect) else { return none() };
    let list: Vec<u32> = w.item_order.iter().copied().filter(|&h| item(w, h).is_some_and(|i| i.owner_unit == 0 && r.contains(i.x, i.y))).collect();
    for h in list {
        let saved = w.enum_ctx;
        w.enum_ctx.filter_item = h;
        let ok = filter == 0 || w.eval_boolexpr(vm, filter);
        w.enum_ctx = saved;
        if ok {
            if let Some(f) = func {
                let saved = w.enum_ctx;
                w.enum_ctx.enum_item = h;
                let _ = vm.call(w, f, &[]);
                w.enum_ctx = saved;
            }
        }
    }
    none()
}

fn add_effect(w: &mut World, model: String, x: f32, y: f32, target: Option<(u32, String)>) -> NativeResult {
    let z = w.ground_z(x, y);
    handle(w.handles.insert(Obj::Effect(Effect { model, x, y, z, target, destroyed: false, facing: 0.0 })))
}

pub fn register(t: &mut HashMap<&'static str, NativeFn>) {
    reg!(t,
        CreateDestructable => |w, _, a| { let (x, y) = snapped(w, a.i(0), a.r(1), a.r(2)); handle(create_destructable(w, id_from_int(a.i(0)), x, y, None, a.r(3), a.r(4), a.i(5) as u32)) },
        CreateDestructableZ => |w, _, a| { let (x, y) = snapped(w, a.i(0), a.r(1), a.r(2)); handle(create_destructable(w, id_from_int(a.i(0)), x, y, Some(a.r(3)), a.r(4), a.r(5), a.i(6) as u32)) },
        CreateDeadDestructable => |w, _, a| {
            let (x, y) = snapped(w, a.i(0), a.r(1), a.r(2));
            let h = create_destructable(w, id_from_int(a.i(0)), x, y, None, a.r(3), a.r(4), a.i(5) as u32);
            if let Some(d) = dest_mut(w, h) { d.life = 0.0; }
            handle(h)
        },
        CreateDeadDestructableZ => |w, _, a| {
            let (x, y) = snapped(w, a.i(0), a.r(1), a.r(2));
            let h = create_destructable(w, id_from_int(a.i(0)), x, y, Some(a.r(3)), a.r(4), a.r(5), a.i(6) as u32);
            if let Some(d) = dest_mut(w, h) { d.life = 0.0; }
            handle(h)
        },
        RemoveDestructable => |w, _, a| {
            let h = a.h(0);
            unstamp_destructable(w, h);
            if dest(w, h).is_some() { w.handles.remove(h); w.destructable_order.retain(|x| *x != h); }
            none()
        },
        KillDestructable => |w, vm, a| { kill_destructable(w, vm, a.h(0)); none() },
        // Brings one back to life: none given restores all of it, any other amount
        // at least half a point (measured: a dead 50-life tree given 0 has 50, given
        // -5 or 0.3 has 0.5, given 0.6 or 1 has that, given 100 has 50; UndeadX07a's
        // intro cleanup gives its sight blockers GetDestructableMaxLife(null), 0, and
        // they block sight again there, where ours stayed dead).
        DestructableRestoreLife => |w, _, a| {
            let (h, life, _birth) = (a.h(0), a.r(1), a.b(2));
            if let Some(d) = dest_mut(w, h) {
                d.life = if life == 0.0 { d.max_life } else { life.max(0.5).min(d.max_life) };
            }
            restamp_destructable(w, h);
            none()
        },
        SetDestructableInvulnerable => |w, _, a| { let v = a.b(1); if let Some(d) = dest_mut(w, a.h(0)) { d.invulnerable = v; } none() },
        IsDestructableInvulnerable => |w, _, a| boolean(dest(w, a.h(0)).is_some_and(|d| d.invulnerable)),
        GetDestructableTypeId => |w, _, a| int(dest(w, a.h(0)).map_or(0, |d| id_to_int(d.type_id))),
        GetDestructableX => |w, _, a| real(dest(w, a.h(0)).map_or(0.0, |d| d.x)),
        GetDestructableY => |w, _, a| real(dest(w, a.h(0)).map_or(0.0, |d| d.y)),
        GetDestructableLife => |w, _, a| real(dest(w, a.h(0)).map_or(0.0, |d| d.life)),
        SetDestructableLife => |w, _, a| { let v = a.r(1); if let Some(d) = dest_mut(w, a.h(0)) { d.life = v.min(d.max_life); } none() },
        GetDestructableMaxLife => |w, _, a| real(dest(w, a.h(0)).map_or(0.0, |d| d.max_life)),
        SetDestructableMaxLife => |w, _, a| { let v = a.r(1); if let Some(d) = dest_mut(w, a.h(0)) { d.max_life = v; } none() },
        GetDestructableOccluderHeight => |w, _, a| real(dest(w, a.h(0)).and_then(|d| w.objects.destructables.get(d.type_id)).map_or(0.0, |o| o.real("occH"))),
        SetDestructableOccluderHeight => |w, _, a| {
            if let Some(crate::types::Obj::Destructable(d)) = w.handles.get_mut(a.h(0)) {
                d.occluder_height = Some(a.r(1));
            }
            none()
        },
        ShowDestructable => |w, _, a| { let v = a.b(1); if let Some(d) = dest_mut(w, a.h(0)) { d.visible = v; } none() },
        SetDestructableAnimation => |w, _, a| {
            let s = a.s(1).to_ascii_lowercase();
            if let Some(d) = dest_mut(w, a.h(0)) {
                d.animation = Some(s);
                d.anim_serial = d.anim_serial.wrapping_add(1);
            }
            none()
        },
        QueueDestructableAnimation => |w, _, a| {
            let s = a.s(1).to_ascii_lowercase();
            if let Some(d) = dest_mut(w, a.h(0)) {
                d.animation = Some(s);
                d.anim_serial = d.anim_serial.wrapping_add(1);
            }
            none()
        },
        SetDestructableAnimationSpeed => |w, _, a| {
            let speed = a.r(1);
            if let Some(d) = dest_mut(w, a.h(0)) {
                d.anim_speed = speed;
            }
            none()
        },
        GetDestructableName => |w, _, a| {
            let n = dest(w, a.h(0)).and_then(|d| w.objects.destructables.get(d.type_id)).map(|o| o.str("Name")).unwrap_or_default();
            super::string(w.localized(&n))
        },
        EnumDestructablesInRect => |w, vm, a| {
            if let Some(f) = a.code(2) {
                w.tags.handler(f);
            }
            enum_destructables(w, vm, a.h(0), a.h(1), a.code(2))
        },
        GetEnumDestructable => |w, _, _| handle(w.enum_ctx.enum_destructable),
        GetFilterDestructable => |w, _, _| handle(w.enum_ctx.filter_destructable),
        CreateItem => |w, _, a| handle(create_item(w, id_from_int(a.i(0)), a.r(1), a.r(2))),
        GetManipulatedItem => |w, _, _| handle(w.ctx.item),
        GetSoldItem => |w, _, _| handle(w.ctx.item),
        GetSoldUnit => |w, _, _| handle(w.ctx.other_unit),
        GetBuyingUnit => |w, _, _| handle(w.ctx.other_unit),
        GetSellingUnit => |w, _, _| handle(w.ctx.unit),
        GetRevivingUnit => |w, _, _| handle(w.ctx.unit),
        GetRevivableUnit => |w, _, _| handle(w.ctx.unit),
        ReviveHero => |w, _, a| {
            let (h, x, y) = (a.h(0), a.r(1), a.r(2));
            let ok = w.unit(h).is_some_and(|u| u.is_hero && u.dead);
            if ok { w.revive_hero(h, x, y); }
            boolean(ok)
        },
        ReviveHeroLoc => |w, _, a| {
            let h = a.h(0);
            let Some((x, y)) = w.location(a.h(1)) else { return boolean(false) };
            let ok = w.unit(h).is_some_and(|u| u.is_hero && u.dead);
            if ok { w.revive_hero(h, x, y); }
            boolean(ok)
        },
        AddItemToStock => |w, _, a| { add_stock(w, a.h(0), id_from_int(a.i(1)), a.i(2), a.i(3)); none() },
        AddUnitToStock => |w, _, a| { add_stock(w, a.h(0), id_from_int(a.i(1)), a.i(2), a.i(3)); none() },
        RemoveItemFromStock => |w, _, a| { w.shops.added.remove(&(a.h(0), id_from_int(a.i(1)))); none() },
        RemoveUnitFromStock => |w, _, a| { w.shops.added.remove(&(a.h(0), id_from_int(a.i(1)))); none() },
        UnitInventorySize => |w, _, a| int(w.inventory_size(a.h(0)) as i32),
        UnitItemInSlot => |w, _, a| {
            let s = a.i(1);
            handle(w.unit(a.h(0)).and_then(|u| u.inventory.get(s as usize).copied()).filter(|_| s >= 0).unwrap_or(0))
        },
        UnitHasItem => |w, _, a| boolean(a.h(1) != 0 && w.unit(a.h(0)).is_some_and(|u| u.inventory.contains(&a.h(1)))),
        UnitAddItem => |w, vm, a| boolean(w.give_item(vm, a.h(0), a.h(1), None)),
        UnitAddItemById => |w, vm, a| {
            let (u, id) = (a.h(0), id_from_int(a.i(1)));
            let Some((x, y)) = w.unit(u).map(|u| (u.x, u.y)) else { return none() };
            let it = create_item(w, id, x, y);
            if !w.give_item(vm, u, it, None) {
                // A full inventory leaves the item at the unit's feet.
            }
            handle(it)
        },
        UnitAddItemToSlotById => |w, vm, a| {
            let (u, id, slot) = (a.h(0), id_from_int(a.i(1)), a.i(2));
            let Some((x, y)) = w.unit(u).map(|u| (u.x, u.y)) else { return boolean(false) };
            let it = create_item(w, id, x, y);
            let ok = slot >= 0 && w.give_item(vm, u, it, Some(slot as usize));
            if !ok {
                w.remove_item(it);
            }
            boolean(ok)
        },
        UnitRemoveItem => |w, vm, a| {
            let (u, it) = (a.h(0), a.h(1));
            if let Some((x, y)) = w.unit(u).filter(|u| u.inventory.contains(&it)).map(|u| (u.x, u.y)) {
                w.drop_item(vm, it, x, y);
            }
            none()
        },
        UnitRemoveItemFromSlot => |w, vm, a| {
            let (u, s) = (a.h(0), a.i(1));
            let Some((it, x, y)) = w.unit(u).and_then(|u| u.inventory.get(s as usize).map(|&i| (i, u.x, u.y))).filter(|t| t.0 != 0 && s >= 0) else { return none() };
            w.drop_item(vm, it, x, y);
            handle(it)
        },
        UnitDropItemPoint => |w, _, a| {
            let (u, it, x, y) = (a.h(0), a.h(1), a.r(2), a.r(3));
            let ok = w.unit(u).is_some_and(|u| u.inventory.contains(&it));
            if ok {
                if let Some(u) = w.unit_mut(u) { u.action = Action::DropItem { item: it, x, y }; u.path.clear(); }
            }
            boolean(ok)
        },
        UnitDropItemSlot => |w, _, a| {
            // Moves an item between inventory slots.
            let (u, it, slot) = (a.h(0), a.h(1), a.i(2));
            let Some(un) = w.unit_mut(u) else { return boolean(false) };
            let Some(from) = un.inventory.iter().position(|&i| i == it) else { return boolean(false) };
            if slot < 0 || slot as usize >= 6 {
                return boolean(false);
            }
            un.inventory.swap(from, slot as usize);
            boolean(true)
        },
        UnitDropItemTarget => |w, vm, a| {
            // Gives an item to another unit.
            let (u, it, target) = (a.h(0), a.h(1), a.h(2));
            let Some((x, y)) = w.unit(u).filter(|u| u.inventory.contains(&it)).map(|u| (u.x, u.y)) else { return boolean(false) };
            w.drop_item(vm, it, x, y);
            boolean(w.give_item(vm, target, it, None))
        },
        UnitUseItem => |w, vm, a| {
            let (u, it) = (a.h(0), a.h(1));
            let Some(slot) = w.unit(u).and_then(|un| un.inventory.iter().position(|&i| i == it)) else { return boolean(false) };
            boolean(w.use_item(vm, u, slot, 0, 0.0, 0.0))
        },
        UnitUseItemPoint => |w, vm, a| {
            let (u, it, x, y) = (a.h(0), a.h(1), a.r(2), a.r(3));
            let Some(slot) = w.unit(u).and_then(|un| un.inventory.iter().position(|&i| i == it)) else { return boolean(false) };
            boolean(w.use_item(vm, u, slot, 0, x, y))
        },
        UnitUseItemTarget => |w, vm, a| {
            let (u, it, t) = (a.h(0), a.h(1), a.h(2));
            let Some(slot) = w.unit(u).and_then(|un| un.inventory.iter().position(|&i| i == it)) else { return boolean(false) };
            boolean(w.use_item(vm, u, slot, t, 0.0, 0.0))
        },
        ChooseRandomItem => |w, _, a| int(choose_random_item(w, None, a.i(0))),
        ChooseRandomItemEx => |w, _, a| int(choose_random_item(w, Some(a.i(0)), a.i(1))),
        RemoveItem => |w, _, a| {
            let h = a.h(0);
            if item(w, h).is_some() { w.handles.remove(h); w.item_order.retain(|x| *x != h); }
            none()
        },
        GetItemTypeId => |w, _, a| int(item(w, a.h(0)).map_or(0, |i| id_to_int(i.type_id))),
        GetItemX => |w, _, a| real(item(w, a.h(0)).map_or(0.0, |i| i.x)),
        GetItemY => |w, _, a| real(item(w, a.h(0)).map_or(0.0, |i| i.y)),
        SetItemPosition => |w, _, a| { let (x, y) = w.clamp_to_map(a.r(1), a.r(2)); if let Some(i) = item_mut(w, a.h(0)) { i.x = x; i.y = y; i.owner_unit = 0; } none() },
        SetItemVisible => |w, _, a| { let v = a.b(1); if let Some(i) = item_mut(w, a.h(0)) { i.visible = v; } none() },
        IsItemVisible => |w, _, a| boolean(item(w, a.h(0)).is_some_and(|i| i.visible)),
        SetItemInvulnerable => |w, _, a| { let v = a.b(1); if let Some(i) = item_mut(w, a.h(0)) { i.invulnerable = v; } none() },
        IsItemInvulnerable => |w, _, a| boolean(item(w, a.h(0)).is_some_and(|i| i.invulnerable)),
        GetItemCharges => |w, _, a| int(item(w, a.h(0)).map_or(0, |i| i.charges)),
        // Never below 0 (measured: -3 reads 0).
        SetItemCharges => |w, _, a| { let v = a.i(1).max(0); if let Some(i) = item_mut(w, a.h(0)) { i.charges = v; } none() },
        GetItemUserData => |w, _, a| int(item(w, a.h(0)).map_or(0, |i| i.user_data)),
        SetItemUserData => |w, _, a| { let v = a.i(1); if let Some(i) = item_mut(w, a.h(0)) { i.user_data = v; } none() },
        GetItemPlayer => |w, _, a| match item(w, a.h(0)) { Some(i) => NativeResult::Return(w.player_handle(i.player)), None => none() },
        SetItemPlayer => |w, _, a| {
            let Some(p) = w.player_of(a.h(1)) else { return none() };
            if let Some(i) = item_mut(w, a.h(0)) { i.player = p; }
            none()
        },
        SetItemDropOnDeath => |w, _, a| { let v = a.b(1); if let Some(i) = item_mut(w, a.h(0)) { i.drop_on_death = v; } none() },
        SetItemDropID => |w, _, a| { let id = id_from_int(a.i(1)); if let Some(i) = item_mut(w, a.h(0)) { i.drop_type = (id[0] != 0).then_some(id); } none() },
        SetItemDroppable => |w, _, a| { let v = a.b(1); if let Some(i) = item_mut(w, a.h(0)) { i.droppable = v; } none() },
        SetItemPawnable => |w, _, a| { let v = a.b(1); if let Some(i) = item_mut(w, a.h(0)) { i.pawnable = v; } none() },
        IsItemPawnable => |w, _, a| boolean(item(w, a.h(0)).is_some_and(|i| i.pawnable)),
        IsItemOwned => |w, _, a| boolean(item(w, a.h(0)).is_some_and(|i| i.owner_unit != 0)),
        IsItemPowerup => |w, _, a| boolean(item(w, a.h(0)).and_then(|i| w.objects.items.get(i.type_id)).is_some_and(|o| o.bool("powerup"))),
        IsItemSellable => |w, _, a| boolean(item(w, a.h(0)).and_then(|i| w.objects.items.get(i.type_id)).is_some_and(|o| o.bool("sellable"))),
        IsItemIdPowerup => |w, _, a| boolean(w.objects.items.get(id_from_int(a.i(0))).is_some_and(|o| o.bool("powerup"))),
        IsItemIdSellable => |w, _, a| boolean(w.objects.items.get(id_from_int(a.i(0))).is_some_and(|o| o.bool("sellable"))),
        IsItemIdPawnable => |w, _, a| boolean(w.objects.items.get(id_from_int(a.i(0))).is_some_and(|o| o.bool("pawnable"))),
        GetItemLevel => |w, _, a| int(item(w, a.h(0)).and_then(|i| w.objects.items.get(i.type_id)).map_or(0, |o| o.int("Level"))),
        GetItemType => |w, _, a| {
            let class = item(w, a.h(0)).and_then(|i| w.objects.items.get(i.type_id)).map(|o| o.str("class").to_ascii_lowercase()).unwrap_or_default();
            NativeResult::Return(Value::Handle(item_class(&class)))
        },
        GetItemName => |w, _, a| {
            let n = item(w, a.h(0)).and_then(|i| w.objects.items.get(i.type_id)).map(|o| o.str("Name")).unwrap_or_default();
            super::string(w.localized(&n))
        },
        EnumItemsInRect => |w, vm, a| {
            if let Some(f) = a.code(2) {
                w.tags.handler(f);
            }
            enum_items(w, vm, a.h(0), a.h(1), a.code(2))
        },
        GetEnumItem => |w, _, _| handle(w.enum_ctx.enum_item),
        GetFilterItem => |w, _, _| handle(w.enum_ctx.filter_item),
        AddSpecialEffect => |w, _, a| {
            w.tags.take_all(&[b"+EIP", b"`pos", b"^pmr", b"^pmr"]);
            add_effect(w, a.s(0), a.r(1), a.r(2), None)
        },
        AddSpecialEffectLoc => |w, _, a| { w.tags.take_all(&[b"+EIP", b"`pos", b"^pmr", b"^pmr"]);  let (x, y) = w.location(a.h(1)).unwrap_or_default(); add_effect(w, a.s(0), x, y, None) },
        AddSpecialEffectTarget => |w, _, a| add_effect(w, a.s(0), 0.0, 0.0, Some((a.h(1), a.s(2)))),
        DestroyEffect => |w, _, a| {
            if let Some(Obj::Effect(e)) = w.handles.get_mut(a.h(0)) { e.destroyed = true; }
            w.dying_effects.push((a.h(0), w.time));
            none()
        },
        AddSpellEffect => |w, _, a| add_effect(w, a.s(0), a.r(2), a.r(3), None),
        AddSpellEffectLoc => |w, _, a| { let (x, y) = w.location(a.h(2)).unwrap_or_default(); add_effect(w, a.s(0), x, y, None) },
        AddSpellEffectById => |w, _, a| add_effect(w, String::new(), a.r(2), a.r(3), None),
        AddSpellEffectByIdLoc => |w, _, a| { let (x, y) = w.location(a.h(2)).unwrap_or_default(); add_effect(w, String::new(), x, y, None) },
        AddSpellEffectTarget => |w, _, a| add_effect(w, a.s(0), 0.0, 0.0, Some((a.h(2), a.s(3)))),
        AddSpellEffectTargetById => |w, _, a| add_effect(w, String::new(), 0.0, 0.0, Some((a.h(2), a.s(3)))),
    );
}

/// ITEM_TYPE_* values for the `class` field.
fn item_class(c: &str) -> u32 {
    match c {
        "permanent" => 0,
        "charged" => 1,
        "powerup" => 2,
        "artifact" => 3,
        "purchasable" => 4,
        "campaign" => 5,
        "miscellaneous" => 6,
        "unknown" => 7,
        _ => 8,
    }
}
