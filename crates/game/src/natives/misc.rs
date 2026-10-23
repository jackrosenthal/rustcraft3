//! Unit and item pools, random creeps and buildings, terrain queries, shop stock
//! across shops, player removal, and game settings.

use std::collections::HashMap;

use super::{Args, boolean, handle, int, none, string};
use crate::objects::{Id, id_from_int, id_to_int};
use crate::types::*;
use crate::world::{NativeFn, World};

fn pool(w: &mut World, h: u32) -> Option<&mut Vec<(Id, f32)>> {
    match w.handles.get_mut(h) {
        Some(Obj::UnitPool(p)) | Some(Obj::ItemPool(p)) => Some(p),
        _ => None,
    }
}

fn pool_add(w: &mut World, h: u32, id: Id, weight: f32) {
    if let Some(p) = pool(w, h) {
        match p.iter_mut().find(|e| e.0 == id) {
            Some(e) => e.1 = weight,
            None => p.push((id, weight)),
        }
    }
}

/// A type from a pool, chosen with chance in proportion to its weight.
fn pool_pick(w: &mut World, h: u32) -> Option<Id> {
    let entries = pool(w, h)?.clone();
    let total: f32 = entries.iter().map(|e| e.1.max(0.0)).sum();
    if total <= 0.0 {
        return None;
    }
    let mut r = w.rng.next_u32() as f32 / u32::MAX as f32 * total;
    for (id, weight) in &entries {
        r -= weight.max(0.0);
        if r <= 0.0 {
            return Some(*id);
        }
    }
    entries.last().map(|e| e.0)
}

/// Whether a unit type may appear on this map's tileset (UnitUI tilesets: "*" or letters).
fn on_tileset(w: &World, o: &crate::objects::Object) -> bool {
    let t = o.str("tilesets");
    t.contains('*') || w.tileset == 0 || t.bytes().any(|b| b.eq_ignore_ascii_case(&w.tileset))
}

/// ChooseRandomCreep (0x6f672c50): a non-hero creep type of the level (any for -1),
/// not a building or special, on the map's tileset.
fn random_creep(w: &mut World, level: i32) -> i32 {
    let mut ids: Vec<Id> = w.objects.units.ids().into_iter().filter(|&id| {
        !id[0].is_ascii_uppercase()
            && w.objects.units.get(id).is_some_and(|o| {
                o.str("race").eq_ignore_ascii_case("creeps") && (level == -1 || o.int("level") == level) && !o.bool("isbldg") && !o.bool("special") && on_tileset(w, o)
            })
    }).collect();
    ids.sort();
    if ids.is_empty() {
        return 0;
    }
    let k = w.rng.int(0, ids.len() as i32 - 1) as usize;
    id_to_int(ids[k])
}

/// ChooseRandomNPBuilding (0x6f672f50): a neutral ("other" race) building placeable in
/// the editor, not special or campaign-only, on the map's tileset.
fn random_np_building(w: &mut World) -> i32 {
    let mut ids: Vec<Id> = w.objects.units.ids().into_iter().filter(|&id| {
        w.objects.units.get(id).is_some_and(|o| {
            o.str("race").eq_ignore_ascii_case("other") && o.bool("isbldg") && !o.bool("special") && !o.bool("campaign") && o.bool("inEditor") && on_tileset(w, o)
        })
    }).collect();
    ids.sort();
    if ids.is_empty() {
        return 0;
    }
    let k = w.rng.int(0, ids.len() as i32 - 1) as usize;
    id_to_int(ids[k])
}

/// PATHING_TYPE_* as war3map.wpm flag bits (0x6f201630).
fn pathing_mask(t: i32) -> u8 {
    match t {
        0 => 0xff,
        1 => 0x02,
        2 => 0x04,
        3 => 0x08,
        4 => 0x10,
        5 => 0x20,
        6 => 0x40,
        7 => 0x80,
        _ => 0,
    }
}

/// Shops with the Sell Items ability ('Asid'), which the *AllStock natives change.
fn all_shops(w: &World) -> Vec<u32> {
    w.unit_order.iter().copied().filter(|&h| w.unit(h).is_some_and(|u| !u.dead) && w.ability_with_code(h, "Asid").is_some()).collect()
}

/// EFFECT_TYPE_* art of an ability (or buff): Effectart, Targetart, Casterart,
/// Specialart, Areaeffectart, Missileart, or LightningEffect, the index'th entry.
fn ability_effect(w: &World, id: Id, kind: i32, index: i32) -> String {
    let field = match kind {
        1 => "Targetart",
        2 => "Casterart",
        3 => "Specialart",
        4 => "Areaeffectart",
        5 => "Missileart",
        6 => "LightningEffect",
        _ => "Effectart",
    };
    let o = w.objects.abilities.get(id).or_else(|| w.objects.buffs.get(id));
    o.and_then(|o| o.list(field).get(index.max(0) as usize).cloned()).unwrap_or_default()
}

/// SOUND_TYPE_EFFECT (Effectsound) or SOUND_TYPE_EFFECT_LOOPED (Effectsoundlooped).
fn ability_sound(w: &World, id: Id, kind: i32) -> String {
    let field = if kind == 1 { "Effectsoundlooped" } else { "Effectsound" };
    let o = w.objects.abilities.get(id).or_else(|| w.objects.buffs.get(id));
    o.map(|o| o.str(field)).unwrap_or_default()
}

pub fn register(t: &mut HashMap<&'static str, NativeFn>) {
    reg!(t,
        CreateUnitPool => |w, _, _| handle(w.handles.insert(Obj::UnitPool(Vec::new()))),
        DestroyUnitPool => |w, _, a| { if matches!(w.handles.get(a.h(0)), Some(Obj::UnitPool(_))) { w.handles.remove(a.h(0)); } none() },
        UnitPoolAddUnitType => |w, _, a| { pool_add(w, a.h(0), id_from_int(a.i(1)), a.r(2)); none() },
        UnitPoolRemoveUnitType => |w, _, a| { let id = id_from_int(a.i(1)); if let Some(p) = pool(w, a.h(0)) { p.retain(|e| e.0 != id); } none() },
        PlaceRandomUnit => |w, vm, a| {
            let (owner, x, y, facing) = (w.player_of(a.h(1)).unwrap_or(0), a.r(2), a.r(3), a.r(4));
            let Some(id) = pool_pick(w, a.h(0)) else { return handle(0) };
            let u = super::units::create_unit(w, owner, id, x, y, facing);
            let _ = vm;
            handle(u)
        },
        CreateItemPool => |w, _, _| handle(w.handles.insert(Obj::ItemPool(Vec::new()))),
        DestroyItemPool => |w, _, a| { if matches!(w.handles.get(a.h(0)), Some(Obj::ItemPool(_))) { w.handles.remove(a.h(0)); } none() },
        ItemPoolAddItemType => |w, _, a| { pool_add(w, a.h(0), id_from_int(a.i(1)), a.r(2)); none() },
        ItemPoolRemoveItemType => |w, _, a| { let id = id_from_int(a.i(1)); if let Some(p) = pool(w, a.h(0)) { p.retain(|e| e.0 != id); } none() },
        PlaceRandomItem => |w, _, a| {
            let (x, y) = (a.r(1), a.r(2));
            let Some(id) = pool_pick(w, a.h(0)) else { return handle(0) };
            handle(super::objects::create_item(w, id, x, y))
        },
        ChooseRandomCreep => |w, _, a| int(random_creep(w, a.i(0))),
        ChooseRandomNPBuilding => |w, _, _| int(random_np_building(w)),

        // True where the type's flag is set: where the point is NOT pathable that way.
        IsTerrainPathable => |w, _, a| {
            let (x, y, m) = (a.r(0), a.r(1), pathing_mask(a.h(2) as i32));
            let Some(p) = w.pathing.as_ref() else { return boolean(false) };
            let c = p.cell_of(x, y);
            let blocked = m & crate::pathing::NO_WALK != 0 && !p.walkable(c) && p.flags_at(c).is_some_and(|f| f & crate::pathing::NO_WALK == 0);
            boolean(p.flags_at(c).is_none_or(|f| f & m != 0) || blocked)
        },
        SetTerrainPathable => |w, _, a| {
            let (x, y, m, on) = (a.r(0), a.r(1), pathing_mask(a.h(2) as i32), a.b(3));
            if let Some(p) = w.pathing.as_mut() {
                let c = p.cell_of(x, y);
                p.set_flags(c, m, !on);
            }
            none()
        },
        GetTerrainCliffLevel => |w, _, a| { let (x, y) = (a.r(0), a.r(1)); int(w.terrain_heights.as_ref().map_or(0, |t| t.level_at(x, y) as i32)) },

        AddItemToAllStock => |w, _, a| { let (id, c, m) = (id_from_int(a.i(0)), a.i(1), a.i(2)); for s in all_shops(w) { super::objects::add_stock(w, s, id, c, m); } none() },
        AddUnitToAllStock => |w, _, a| { let (id, c, m) = (id_from_int(a.i(0)), a.i(1), a.i(2)); for s in all_shops(w) { super::objects::add_stock(w, s, id, c, m); } none() },
        RemoveItemFromAllStock => |w, _, a| { let id = id_from_int(a.i(0)); w.shops.added.retain(|k, _| k.1 != id); none() },
        RemoveUnitFromAllStock => |w, _, a| { let id = id_from_int(a.i(0)); w.shops.added.retain(|k, _| k.1 != id); none() },
        SetAllItemTypeSlots => |w, _, a| { w.shops.item_slots = Some(a.i(0)); none() },
        SetAllUnitTypeSlots => |w, _, a| { w.shops.unit_slots = Some(a.i(0)); none() },
        SetItemTypeSlots => |w, _, a| { let n = a.i(1); w.shops.shop_item_slots.insert(a.h(0), n); none() },
        SetUnitTypeSlots => |w, _, a| { let n = a.i(1); w.shops.shop_unit_slots.insert(a.h(0), n); none() },

        // The player's RACE_PREF_USER_SELECTABLE bit.
        GetPlayerSelectable => |w, _, a| boolean(w.player_of(a.h(0)).is_some_and(|p| w.players.get(p as usize).is_some_and(|pl| pl.race_selectable))),
        // CripplePlayer: the force sees the player's units.
        CripplePlayer => |w, _, a| {
            let (Some(p), on) = (w.player_of(a.h(0)), a.b(2)) else { return none() };
            let mask = match w.handles.get(a.h(1)) { Some(Obj::Force(f)) => f.players as u16, _ => 0 };
            let e = w.crippled.entry(p).or_insert(0);
            if on { *e |= mask } else { *e &= !mask }
            none()
        },
        // RemovePlayer: the player leaves the game with a result.
        RemovePlayer => |w, _, a| {
            let (Some(p), result) = (w.player_of(a.h(0)), a.h(1) as i32) else { return none() };
            if let Some(pl) = w.players.get_mut(p as usize) {
                pl.slot_state = slot_state::LEFT;
            }
            w.player_results.insert(p, result);
            none()
        },
        // An empty function in the original (0x6f25c2a0).
        SetPlayerUnitsOwner => |_, _, _| none(),
        CachePlayerHeroData => |_, _, _| none(),

        SetTutorialCleared => |w, _, a| { w.persist.tutorial_cleared = a.b(0); none() },
        GetDefaultDifficulty => |w, _, _| handle(w.persist.difficulty as u32),
        SetDefaultDifficulty => |w, _, a| { w.persist.difficulty = a.h(0) as i32; none() },
        SetCustomCampaignButtonVisible => |w, _, a| { let (i, on) = (a.i(0), a.b(1)); w.persist.custom_campaign_buttons.insert(i, on); none() },
        GetCustomCampaignButtonVisible => |w, _, a| boolean(w.persist.custom_campaign_buttons.get(&a.i(0)).copied().unwrap_or(true)),
        DoNotSaveReplay => |_, _, _| none(),
        DisableRestartMission => |w, _, a| { w.restart_disabled = a.b(0); none() },
        GetSaveBasicFilename => |w, _, _| string(w.load_game_name.clone()),
        SetReservedLocalHeroButtons => |w, _, a| { w.reserved_hero_buttons = a.i(0); none() },
        SetAltMinimapIcon => |w, _, a| { w.alt_minimap_icon = Some(a.s(0)); none() },
        EnableWorldFogBoundary => |w, _, a| { w.fog_boundary = a.b(0); none() },

        // Single player: nothing to synchronize.
        SyncStoredInteger => |_, _, _| none(),
        SyncStoredReal => |_, _, _| none(),
        SyncStoredBoolean => |_, _, _| none(),
        SyncStoredUnit => |_, _, _| none(),
        SyncStoredString => |_, _, _| none(),
        // An empty function in the original.
        SetTerrainFog => |_, _, _| none(),
        UnitAddIndicator => |w, _, a| { let c = [a.i(1), a.i(2), a.i(3), a.i(4)].map(|v| v as u8); w.requests.push(crate::world::Request::Indicator { widget: a.h(0), color: c }); none() },
        AddIndicator => |w, _, a| { let c = [a.i(1), a.i(2), a.i(3), a.i(4)].map(|v| v as u8); w.requests.push(crate::world::Request::Indicator { widget: a.h(0), color: c }); none() },
        PlayModelCinematic => |w, _, a| { w.model_cinematic = Some(a.s(0)); none() },
        PlayCinematic => |w, _, a| { w.requests.push(crate::world::Request::PlayMovie(a.s(0))); none() },
        ForceUIKey => |w, _, a| { w.requests.push(crate::world::Request::ForceUIKey(a.s(0))); none() },
        ForceUICancel => |w, _, _| { w.requests.push(crate::world::Request::ForceUICancel); none() },
        DisplayLoadDialog => |w, _, _| { w.requests.push(crate::world::Request::DisplayLoadDialog); none() },
        GetAbilityEffect => |w, _, a| string(ability_effect(w, crate::objects::id_from_str(&a.s(0)), a.h(1) as i32, a.i(2))),
        GetAbilityEffectById => |w, _, a| string(ability_effect(w, id_from_int(a.i(0)), a.h(1) as i32, a.i(2))),
        GetAbilitySound => |w, _, a| string(ability_sound(w, crate::objects::id_from_str(&a.s(0)), a.h(1) as i32)),
        GetAbilitySoundById => |w, _, a| string(ability_sound(w, id_from_int(a.i(0)), a.h(1) as i32)),
        // A neutral passive gold mine ('ngol', hidden) under the player's haunted mine ('ugol').
        CreateBlightedGoldmine => |w, _, a| {
            let (Some(p), x, y, face) = (w.player_of(a.h(0)), a.r(1), a.r(2), a.r(3)) else { return handle(0) };
            let mine = super::units::create_unit(w, 15, *b"ngol", x, y, face);
            if let Some(m) = w.unit_mut(mine) {
                m.hidden = true;
            }
            let h = super::units::create_unit(w, p, *b"ugol", x, y, face);
            let gold = w.unit(mine).map_or(0, |m| m.resource);
            if let Some(u) = w.unit_mut(h) {
                u.extra.mine = Some(mine);
                u.resource = gold;
            }
            handle(h)
        },
        GetTournamentFinishSoonTimeRemaining => |_, _, _| int(0),
        GetTournamentFinishNowRule => |_, _, _| int(0),
        GetTournamentFinishNowPlayer => |_, _, _| handle(0),
        GetTournamentScore => |_, _, _| int(0),
    );
}

use super::reg;
