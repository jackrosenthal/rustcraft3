//! Math, strings, type conversion, ids, hashtables, and game caches.

use std::collections::HashMap;

use w3_jass::{NativeResult, Value, Vm};

use super::{Args, boolean, handle, int, none, real, reg, string};
use crate::objects::id_from_int;
use crate::orders::ORDERS;
use crate::types::{GameCache, Hashtable, Obj};
use crate::world::{NativeFn, World};

/// Formats a real the way R2S/R2SW do: fixed-point with the given precision.
pub fn format_real(v: f32, width: i32, precision: i32) -> String {
    let p = precision.max(0) as usize;
    let s = format!("{v:.p$}");
    let w = width.max(0) as usize;
    if s.len() < w { format!("{s:>w$}") } else { s }
}

fn atoi(s: &str) -> i32 {
    let t = s.trim_start();
    let (neg, digits) = match t.as_bytes().first() {
        Some(b'-') => (true, &t[1..]),
        Some(b'+') => (false, &t[1..]),
        _ => (false, t),
    };
    let mut v: i64 = 0;
    for c in digits.bytes().take_while(u8::is_ascii_digit) {
        v = (v * 10 + (c - b'0') as i64).min(i64::from(i32::MAX) + 1);
    }
    let v = if neg { -v } else { v };
    v.clamp(i32::MIN as i64, i32::MAX as i64) as i32
}

fn atof(s: &str) -> f32 {
    let t = s.trim_start();
    let end = t
        .char_indices()
        .take_while(|&(i, c)| c.is_ascii_digit() || c == '.' || ((c == '-' || c == '+') && i == 0) || c == 'e' || c == 'E')
        .map(|(i, c)| i + c.len_utf8())
        .last()
        .unwrap_or(0);
    let mut cut = &t[..end];
    while !cut.is_empty() && cut.parse::<f32>().is_err() {
        cut = &cut[..cut.len() - 1];
    }
    cut.parse().unwrap_or(0.0)
}

pub fn order_id(name: &str) -> i32 {
    ORDERS.iter().find(|(_, n)| n.eq_ignore_ascii_case(name)).map_or(0, |(i, _)| *i)
}

pub fn order_name(id: i32) -> Option<&'static str> {
    ORDERS.iter().find(|(i, _)| *i == id).map(|(_, n)| *n)
}

fn hashtable(w: &mut World, h: u32) -> Option<&mut Hashtable> {
    match w.handles.get_mut(h) {
        Some(Obj::Hashtable(t)) => Some(t),
        _ => None,
    }
}

fn gamecache(w: &mut World, h: u32) -> Option<&mut GameCache> {
    match w.handles.get_mut(h) {
        Some(Obj::GameCache(t)) => Some(t),
        _ => None,
    }
}

fn convert(_: &mut World, _: &mut Vm, a: &[Value]) -> NativeResult {
    // Enumeration handles (races, colors, event ids, ...) carry their integer value.
    NativeResult::Return(Value::Handle(a.i(0) as u32))
}

fn load_handle(w: &mut World, _: &mut Vm, a: &[Value]) -> NativeResult {
    let v = hashtable(w, a.h(0)).and_then(|t| t.handles.get(&(a.i(1), a.i(2))).copied()).unwrap_or(0);
    handle(v)
}

fn save_handle(w: &mut World, _: &mut Vm, a: &[Value]) -> NativeResult {
    let (k, v) = ((a.i(1), a.i(2)), a.h(3));
    if let Some(t) = hashtable(w, a.h(0)) {
        t.handles.insert(k, v);
    }
    boolean(true)
}

pub fn register(t: &mut HashMap<&'static str, NativeFn>) {
    reg!(t,
        Deg2Rad => |_, _, a| real(a.r(0).to_radians()),
        Rad2Deg => |_, _, a| real(a.r(0).to_degrees()),
        Sin => |_, _, a| real(a.r(0).sin()),
        Cos => |_, _, a| real(a.r(0).cos()),
        Tan => |_, _, a| real(a.r(0).tan()),
        Asin => |_, _, a| real(a.r(0).asin()),
        Acos => |_, _, a| real(a.r(0).acos()),
        Atan => |_, _, a| real(a.r(0).atan()),
        Atan2 => |_, _, a| real(a.r(0).atan2(a.r(1))),
        SquareRoot => |_, _, a| real(if a.r(0) <= 0.0 { 0.0 } else { a.r(0).sqrt() }),
        Pow => |_, _, a| real(a.r(0).powf(a.r(1))),
        I2R => |_, _, a| real(a.i(0) as f32),
        R2I => |_, _, a| int(a.r(0) as i32),
        I2S => |_, _, a| string(a.i(0).to_string()),
        R2S => |_, _, a| string(format_real(a.r(0), 0, 3)),
        R2SW => |_, _, a| string(format_real(a.r(0), a.i(1), a.i(2))),
        S2I => |_, _, a| int(atoi(&a.s(0))),
        S2R => |_, _, a| real(atof(&a.s(0))),
        GetHandleId => |_, _, a| int(a.h(0) as i32),
        SubString => |_, _, a| {
            let Some(s) = a[0].str() else { return none() };
            let b = s.as_bytes();
            let start = (a.i(1).max(0) as usize).min(b.len());
            let end = (a.i(2).max(0) as usize).min(b.len());
            if end <= start {
                return string("");
            }
            string(String::from_utf8_lossy(&b[start..end]))
        },
        StringLength => |_, _, a| int(a[0].str().map_or(0, |s| s.len() as i32)),
        StringCase => |_, _, a| {
            let Some(s) = a[0].str() else { return none() };
            let s: String = s.chars().take(1023).collect();
            string(if a.b(1) { s.to_ascii_uppercase() } else { s.to_ascii_lowercase() })
        },
        StringHash => |_, _, a| int(a[0].str().map_or(0, crate::rng::string_hash)),
        GetLocalizedString => |w, _, a| {
            let s = a.s(0);
            string(w.localized(&s))
        },
        GetLocalizedHotkey => |w, _, a| {
            let s = w.localized(&a.s(0));
            int(s.bytes().next().map_or(0, |c| c as i32))
        },
        OrderId => |_, _, a| int(order_id(&a.s(0))),
        OrderId2String => |_, _, a| match order_name(a.i(0)) {
            Some(n) => string(n),
            None => none(),
        },
        UnitId => |w, _, a| {
            let name = a.s(0);
            let id = w.objects.units.objects.iter().find(|(_, o)| o.str("Name").eq_ignore_ascii_case(&name) || o.str("unitName").eq_ignore_ascii_case(&name)).map(|(id, _)| *id);
            int(id.map_or(0, crate::objects::id_to_int))
        },
        UnitId2String => |w, _, a| {
            let id = id_from_int(a.i(0));
            match w.objects.units.get(id) {
                Some(o) if !o.str("unitName").is_empty() => string(o.str("unitName")),
                _ => none(),
            }
        },
        AbilityId => |w, _, a| {
            let name = a.s(0);
            let id = w.objects.abilities.objects.iter().find(|(_, o)| o.str("alias").eq_ignore_ascii_case(&name)).map(|(id, _)| *id);
            int(id.map_or(0, crate::objects::id_to_int))
        },
        AbilityId2String => |w, _, a| {
            let id = id_from_int(a.i(0));
            match w.objects.abilities.get(id) {
                Some(o) => string(o.str("alias")),
                None => none(),
            }
        },
        GetObjectName => |w, _, a| {
            let id = id_from_int(a.i(0));
            let db = &w.objects;
            let name = [&db.units, &db.items, &db.abilities, &db.buffs, &db.upgrades, &db.destructables, &db.doodads]
                .iter()
                .find_map(|t| t.get(id).map(|o| o.str("Name")))
                .unwrap_or_default();
            string(w.localized(&name))
        },
        GetRandomInt => |w, _, a| int(w.rng.int(a.i(0), a.i(1))),
        GetRandomReal => |w, _, a| {
            let (lo, hi) = (a.r(0), a.r(1));
            if (hi - lo).abs() < f32::EPSILON {
                return real(lo);
            }
            let (lo, hi) = if hi < lo { (hi, lo) } else { (lo, hi) };
            // A uniform float in [1, 2) built from the generator's mantissa bits.
            let r = f32::from_bits((w.rng.next_u32() & 0x7f_ffff) | 0x3f80_0000) - 1.0;
            real(lo + r * (hi - lo))
        },
        SetRandomSeed => |w, _, a| {
            w.rng = crate::rng::Rng::new(a.i(0) as u32);
            none()
        },
        InitHashtable => |w, _, _| handle(w.handles.insert(Obj::Hashtable(Box::default()))),
        SaveInteger => |w, _, a| {
            if let Some(t) = hashtable(w, a.h(0)) { t.ints.insert((a.i(1), a.i(2)), a.i(3)); }
            none()
        },
        SaveReal => |w, _, a| {
            if let Some(t) = hashtable(w, a.h(0)) { t.reals.insert((a.i(1), a.i(2)), a.r(3)); }
            none()
        },
        SaveBoolean => |w, _, a| {
            if let Some(t) = hashtable(w, a.h(0)) { t.bools.insert((a.i(1), a.i(2)), a.b(3)); }
            none()
        },
        SaveStr => |w, _, a| {
            let s = a.s(3);
            if let Some(t) = hashtable(w, a.h(0)) { t.strings.insert((a.i(1), a.i(2)), s); }
            boolean(true)
        },
        LoadInteger => |w, _, a| int(hashtable(w, a.h(0)).and_then(|t| t.ints.get(&(a.i(1), a.i(2))).copied()).unwrap_or(0)),
        LoadReal => |w, _, a| real(hashtable(w, a.h(0)).and_then(|t| t.reals.get(&(a.i(1), a.i(2))).copied()).unwrap_or(0.0)),
        LoadBoolean => |w, _, a| boolean(hashtable(w, a.h(0)).and_then(|t| t.bools.get(&(a.i(1), a.i(2))).copied()).unwrap_or(false)),
        LoadStr => |w, _, a| match hashtable(w, a.h(0)).and_then(|t| t.strings.get(&(a.i(1), a.i(2))).cloned()) {
            Some(s) => string(s),
            None => none(),
        },
        HaveSavedInteger => |w, _, a| boolean(hashtable(w, a.h(0)).is_some_and(|t| t.ints.contains_key(&(a.i(1), a.i(2))))),
        HaveSavedReal => |w, _, a| boolean(hashtable(w, a.h(0)).is_some_and(|t| t.reals.contains_key(&(a.i(1), a.i(2))))),
        HaveSavedBoolean => |w, _, a| boolean(hashtable(w, a.h(0)).is_some_and(|t| t.bools.contains_key(&(a.i(1), a.i(2))))),
        HaveSavedString => |w, _, a| boolean(hashtable(w, a.h(0)).is_some_and(|t| t.strings.contains_key(&(a.i(1), a.i(2))))),
        HaveSavedHandle => |w, _, a| boolean(hashtable(w, a.h(0)).is_some_and(|t| t.handles.contains_key(&(a.i(1), a.i(2))))),
        RemoveSavedInteger => |w, _, a| { if let Some(t) = hashtable(w, a.h(0)) { t.ints.remove(&(a.i(1), a.i(2))); } none() },
        RemoveSavedReal => |w, _, a| { if let Some(t) = hashtable(w, a.h(0)) { t.reals.remove(&(a.i(1), a.i(2))); } none() },
        RemoveSavedBoolean => |w, _, a| { if let Some(t) = hashtable(w, a.h(0)) { t.bools.remove(&(a.i(1), a.i(2))); } none() },
        RemoveSavedString => |w, _, a| { if let Some(t) = hashtable(w, a.h(0)) { t.strings.remove(&(a.i(1), a.i(2))); } none() },
        RemoveSavedHandle => |w, _, a| { if let Some(t) = hashtable(w, a.h(0)) { t.handles.remove(&(a.i(1), a.i(2))); } none() },
        FlushParentHashtable => |w, _, a| { if let Some(t) = hashtable(w, a.h(0)) { *t = Hashtable::default(); } none() },
        FlushChildHashtable => |w, _, a| {
            let p = a.i(1);
            if let Some(t) = hashtable(w, a.h(0)) {
                t.ints.retain(|k, _| k.0 != p);
                t.reals.retain(|k, _| k.0 != p);
                t.bools.retain(|k, _| k.0 != p);
                t.strings.retain(|k, _| k.0 != p);
                t.handles.retain(|k, _| k.0 != p);
            }
            none()
        },
        InitGameCache => |w, _, a| {
            // A cache saved by an earlier game comes back with its contents.
            let name = a.s(0);
            let cache = match w.persist.caches.get(&name.to_ascii_lowercase()) {
                Some(saved) => saved.to_cache(&name),
                None => GameCache { name, ..Default::default() },
            };
            handle(w.handles.insert(Obj::GameCache(Box::new(cache))))
        },
        StoreUnit => |w, _, a| {
            let (key, unit) = ((a.s(1), a.s(2)), a.h(3));
            let stored = w.unit(unit).map(|u| crate::types::StoredUnit {
                type_id: u.type_id,
                level: u.level,
                xp: u.hero_xp,
                skill_points: u.skill_points,
                str_: u.str_,
                agi: u.agi,
                int: u.int,
                proper_name: u.proper_name.clone(),
                abilities: u.abilities.clone(),
                items: u.inventory.iter().map(|&i| w.item(i).map(|it| (it.type_id, it.charges))).collect(),
            });
            match (stored, gamecache(w, a.h(0))) {
                (Some(s), Some(c)) => { c.units.insert(key, s); boolean(true) }
                _ => boolean(false),
            }
        },
        RestoreUnit => |w, vm, a| {
            let key = (a.s(1), a.s(2));
            let Some(s) = gamecache(w, a.h(0)).and_then(|c| c.units.get(&key).cloned()) else { return handle(0) };
            let Some(p) = w.player_of(a.h(3)) else { return handle(0) };
            let h = crate::natives::units::create_unit(w, p, s.type_id, a.r(4), a.r(5), a.r(6));
            if let Some(u) = w.unit_mut(h) {
                u.level = s.level;
                u.hero_xp = s.xp;
                u.skill_points = s.skill_points;
                u.str_ = s.str_;
                u.agi = s.agi;
                u.int = s.int;
                if !s.proper_name.is_empty() {
                    u.proper_name = s.proper_name.clone();
                }
                u.abilities = s.abilities.clone();
            }
            for (slot, it) in s.items.iter().enumerate() {
                if let Some((t, charges)) = *it {
                    let item = crate::natives::objects::create_item(w, t, a.r(4), a.r(5));
                    if let Some(i) = w.item_mut(item) {
                        i.charges = charges;
                    }
                    w.give_item(vm, h, item, Some(slot));
                }
            }
            w.apply_hero_stats(h, true);
            handle(h)
        },
        HaveStoredUnit => |w, _, a| { let k = (a.s(1), a.s(2)); boolean(gamecache(w, a.h(0)).is_some_and(|c| c.units.contains_key(&k))) },
        FlushStoredUnit => |w, _, a| { let k = (a.s(1), a.s(2)); if let Some(c) = gamecache(w, a.h(0)) { c.units.remove(&k); } none() },
        FlushStoredMission => |w, _, a| {
            let m = a.s(1);
            if let Some(c) = gamecache(w, a.h(0)) {
                c.ints.retain(|k, _| k.0 != m);
                c.reals.retain(|k, _| k.0 != m);
                c.bools.retain(|k, _| k.0 != m);
                c.strings.retain(|k, _| k.0 != m);
                c.units.retain(|k, _| k.0 != m);
            }
            none()
        },
        StoreInteger => |w, _, a| { let k = (a.s(1), a.s(2)); if let Some(c) = gamecache(w, a.h(0)) { c.ints.insert(k, a.i(3)); } none() },
        StoreReal => |w, _, a| { let k = (a.s(1), a.s(2)); if let Some(c) = gamecache(w, a.h(0)) { c.reals.insert(k, a.r(3)); } none() },
        StoreBoolean => |w, _, a| { let k = (a.s(1), a.s(2)); if let Some(c) = gamecache(w, a.h(0)) { c.bools.insert(k, a.b(3)); } none() },
        StoreString => |w, _, a| { let k = (a.s(1), a.s(2)); let v = a.s(3); if let Some(c) = gamecache(w, a.h(0)) { c.strings.insert(k, v); } boolean(true) },
        GetStoredInteger => |w, _, a| { let k = (a.s(1), a.s(2)); int(gamecache(w, a.h(0)).and_then(|c| c.ints.get(&k).copied()).unwrap_or(0)) },
        GetStoredReal => |w, _, a| { let k = (a.s(1), a.s(2)); real(gamecache(w, a.h(0)).and_then(|c| c.reals.get(&k).copied()).unwrap_or(0.0)) },
        GetStoredBoolean => |w, _, a| { let k = (a.s(1), a.s(2)); boolean(gamecache(w, a.h(0)).and_then(|c| c.bools.get(&k).copied()).unwrap_or(false)) },
        GetStoredString => |w, _, a| { let k = (a.s(1), a.s(2)); match gamecache(w, a.h(0)).and_then(|c| c.strings.get(&k).cloned()) { Some(s) => string(s), None => string("") } },
        HaveStoredInteger => |w, _, a| { let k = (a.s(1), a.s(2)); boolean(gamecache(w, a.h(0)).is_some_and(|c| c.ints.contains_key(&k))) },
        HaveStoredReal => |w, _, a| { let k = (a.s(1), a.s(2)); boolean(gamecache(w, a.h(0)).is_some_and(|c| c.reals.contains_key(&k))) },
        HaveStoredBoolean => |w, _, a| { let k = (a.s(1), a.s(2)); boolean(gamecache(w, a.h(0)).is_some_and(|c| c.bools.contains_key(&k))) },
        HaveStoredString => |w, _, a| { let k = (a.s(1), a.s(2)); boolean(gamecache(w, a.h(0)).is_some_and(|c| c.strings.contains_key(&k))) },
        FlushGameCache => |w, _, a| { if let Some(c) = gamecache(w, a.h(0)) { let n = c.name.clone(); *c = GameCache { name: n, ..Default::default() }; } none() },
        SaveGameCache => |w, _, a| {
            let saved = gamecache(w, a.h(0)).map(|c| (c.name.to_ascii_lowercase(), crate::persist::SavedCache::from_cache(c)));
            match saved {
                Some((name, s)) => { w.persist.caches.insert(name, s); boolean(true) }
                None => boolean(false),
            }
        },
        ReloadGameCachesFromDisk => |_, _, _| boolean(true),
    );
    // Typed handle stores and loads share one representation.
    for name in [
        "SavePlayerHandle", "SaveWidgetHandle", "SaveDestructableHandle", "SaveItemHandle", "SaveUnitHandle",
        "SaveAbilityHandle", "SaveTimerHandle", "SaveTriggerHandle", "SaveTriggerConditionHandle",
        "SaveTriggerActionHandle", "SaveTriggerEventHandle", "SaveForceHandle", "SaveGroupHandle",
        "SaveLocationHandle", "SaveRectHandle", "SaveBooleanExprHandle", "SaveSoundHandle", "SaveEffectHandle",
        "SaveUnitPoolHandle", "SaveItemPoolHandle", "SaveQuestHandle", "SaveQuestItemHandle",
        "SaveDefeatConditionHandle", "SaveTimerDialogHandle", "SaveLeaderboardHandle", "SaveMultiboardHandle",
        "SaveMultiboardItemHandle", "SaveTrackableHandle", "SaveDialogHandle", "SaveButtonHandle",
        "SaveTextTagHandle", "SaveLightningHandle", "SaveImageHandle", "SaveUbersplatHandle", "SaveRegionHandle",
        "SaveFogStateHandle", "SaveFogModifierHandle", "SaveAgentHandle", "SaveHashtableHandle",
    ] {
        t.insert(name, save_handle as NativeFn);
    }
    for name in [
        "LoadPlayerHandle", "LoadWidgetHandle", "LoadDestructableHandle", "LoadItemHandle", "LoadUnitHandle",
        "LoadAbilityHandle", "LoadTimerHandle", "LoadTriggerHandle", "LoadTriggerConditionHandle",
        "LoadTriggerActionHandle", "LoadTriggerEventHandle", "LoadForceHandle", "LoadGroupHandle",
        "LoadLocationHandle", "LoadRectHandle", "LoadBooleanExprHandle", "LoadSoundHandle", "LoadEffectHandle",
        "LoadUnitPoolHandle", "LoadItemPoolHandle", "LoadQuestHandle", "LoadQuestItemHandle",
        "LoadDefeatConditionHandle", "LoadTimerDialogHandle", "LoadLeaderboardHandle", "LoadMultiboardHandle",
        "LoadMultiboardItemHandle", "LoadTrackableHandle", "LoadDialogHandle", "LoadButtonHandle",
        "LoadTextTagHandle", "LoadLightningHandle", "LoadImageHandle", "LoadUbersplatHandle", "LoadRegionHandle",
        "LoadFogStateHandle", "LoadFogModifierHandle", "LoadHashtableHandle",
    ] {
        t.insert(name, load_handle as NativeFn);
    }
    for name in [
        "ConvertRace", "ConvertAllianceType", "ConvertRacePref", "ConvertIGameState", "ConvertFGameState",
        "ConvertPlayerState", "ConvertPlayerScore", "ConvertPlayerGameResult", "ConvertUnitState",
        "ConvertAIDifficulty", "ConvertGameEvent", "ConvertPlayerEvent", "ConvertPlayerUnitEvent",
        "ConvertWidgetEvent", "ConvertDialogEvent", "ConvertUnitEvent", "ConvertLimitOp", "ConvertUnitType",
        "ConvertGameSpeed", "ConvertPlacement", "ConvertStartLocPrio", "ConvertGameDifficulty", "ConvertGameType",
        "ConvertMapFlag", "ConvertMapVisibility", "ConvertMapSetting", "ConvertMapDensity", "ConvertMapControl",
        "ConvertPlayerColor", "ConvertPlayerSlotState", "ConvertVolumeGroup", "ConvertCameraField",
        "ConvertBlendMode", "ConvertRarityControl", "ConvertTexMapFlags", "ConvertFogState", "ConvertEffectType",
        "ConvertVersion", "ConvertItemType", "ConvertAttackType", "ConvertDamageType", "ConvertWeaponType",
        "ConvertSoundType", "ConvertPathingType",
    ] {
        t.insert(name, convert as NativeFn);
    }
}
