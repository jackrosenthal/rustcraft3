//! Math, strings, type conversion, ids, hashtables, and game caches.

use crate::collections::HashMap;

use w3_jass::softfloat as sf;
use w3_jass::{NativeResult, Value, Vm};

use super::{Args, boolean, handle, int, none, real, reg, string};
use crate::objects::id_from_int;
use crate::orders::ORDERS;
use crate::types::{GameCache, Hashtable, Obj};
use crate::world::{NativeFn, World};

/// Formats a real the way R2S/R2SW do: fixed-point with the given precision.
pub fn format_real(v: f32, width: i32, precision: i32) -> String {
    w3_jass::softfloat::format(v.to_bits(), width, precision)
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

/// S2R, as the original reads it (softfloat.rs; measured: " 2.5" reads 0, "1.5e2" 1.5).
fn atof(s: &str) -> f32 {
    f32::from_bits(w3_jass::softfloat::parse(s))
}

pub fn order_id(name: &str) -> i32 {
    ORDERS.iter().find(|(_, n)| n.eq_ignore_ascii_case(name)).map_or(0, |(i, _)| *i)
}

/// A unit type by its name ("footman"), as UnitId reads it.
pub fn unit_id_by_name(w: &World, name: &str) -> i32 {
    let id = w.objects.units.objects.iter().find(|(_, o)| o.str("Name").eq_ignore_ascii_case(name) || o.str("unitName").eq_ignore_ascii_case(name)).map(|(id, _)| *id);
    id.map_or(0, crate::objects::id_to_int)
}

/// The order an Issue native's string names: an order, else a unit type by name (the
/// one to train or build), as FUN_6f20f320 looks it up.
pub fn issue_order_id(w: &World, name: &str) -> i32 {
    match order_id(name) {
        0 => unit_id_by_name(w, name),
        id => id,
    }
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
    let h = w.cache_alias.get(&h).copied().unwrap_or(h);
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
        // The original's own approximations, in its software floats (softfloat.rs;
        // measured: Sin(1) is 0.841473600 there, SquareRoot(1000) 31.623027776).
        Deg2Rad => |_, _, a| real(f32::from_bits(sf::mul(a.r(0).to_bits(), 0x3c8e_fa35))),
        Rad2Deg => |_, _, a| real(f32::from_bits(sf::mul(a.r(0).to_bits(), 0x4265_2ee1))),
        Sin => |_, _, a| real(f32::from_bits(sf::sin(a.r(0).to_bits()))),
        Cos => |_, _, a| real(f32::from_bits(sf::cos(a.r(0).to_bits()))),
        Tan => |_, _, a| real(f32::from_bits(sf::tan(a.r(0).to_bits()))),
        // Outside -1 to 1, 0 (jass_Asin, jass_Acos; measured: Asin(2) and Acos(-2) are 0).
        Asin => |_, _, a| real(if (-1.0..=1.0).contains(&a.r(0)) { f32::from_bits(sf::asin(a.r(0).to_bits())) } else { 0.0 }),
        Acos => |_, _, a| real(if (-1.0..=1.0).contains(&a.r(0)) { f32::from_bits(sf::acos(a.r(0).to_bits())) } else { 0.0 }),
        Atan => |_, _, a| real(f32::from_bits(sf::atan(a.r(0).to_bits()))),
        // Both within 0.001 of 0, 0 (jass_Atan2).
        Atan2 => |_, _, a| real(if a.r(0).abs() < 0.001 && a.r(1).abs() < 0.001 { 0.0 } else { f32::from_bits(sf::atan2(a.r(0).to_bits(), a.r(1).to_bits())) }),
        // Below 0.001, 0 (jass_SquareRoot).
        SquareRoot => |_, _, a| real(if a.r(0) < 0.001 { 0.0 } else { f32::from_bits(sf::sqrt(a.r(0).to_bits())) }),
        // A base within 0.001 of 0 to a negative power is 0, and anything to a power
        // within 0.001 of 0 is 1 (jass_Pow; measured: Pow(0, -1) is 0); a negative base
        // to a fractional power is taken as its size's (measured: Pow(-8, 0.333) is
        // 1.998613824).
        Pow => |_, _, a| {
            let (x, y) = (a.r(0), a.r(1));
            if x.abs() < 0.001 {
                if y < 0.0 {
                    return real(0.0);
                }
            } else if y.abs() < 0.001 {
                return real(1.0);
            }
            real(f32::from_bits(sf::pow(x.to_bits(), y.to_bits())))
        },
        I2R => |_, _, a| real(w3_jass::value::int_to_real(a.i(0) as i64)),
        // Out of range it wraps, as the original's conversion through 64 bits does
        // (measured: R2I(10000000000.0) is 1410065408).
        R2I => |_, _, a| int(a.r(0) as i64 as i32),
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
            // An end before the start reads to the string's end (measured:
            // SubString("hello", 4, 2) is "o").
            let end = if a.i(2) < a.i(1) { b.len() } else { (a.i(2).max(0) as usize).min(b.len()) };
            if end <= start {
                return string("");
            }
            // A character cut in two shows as nothing, as in the original (measured:
            // SubString("h\u{e9}llo", 0, 2) shows "h").
            string(String::from_utf8_lossy(&b[start..end]).replace('\u{fffd}', ""))
        },
        StringLength => |_, _, a| int(a[0].str().map_or(0, |s| s.len() as i32)),
        StringCase => |_, _, a| {
            let Some(s) = a[0].str() else { return none() };
            let s: String = s.chars().take(1023).collect();
            string(if a.b(1) { s.to_ascii_uppercase() } else { s.to_ascii_lowercase() })
        },
        // Of an empty string, 0 (jass_StringHash hashes only a non-empty string).
        StringHash => |_, _, a| int(a[0].str().filter(|s| !s.is_empty()).map_or(0, crate::rng::string_hash)),
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
        UnitId => |w, _, a| int(unit_id_by_name(w, &a.s(0))),
        UnitId2String => |w, _, a| {
            let id = id_from_int(a.i(0));
            match w.objects.units.get(id) {
                // A unit type the map adds reads "custom_" and its id (measured:
                // custom_nC02 for Human04's goblin merchant copy).
                Some(o) if o.base.is_some() => string(format!("custom_{}", crate::objects::id_str(id))),
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
            real(w.rng.real(a.r(0), a.r(1)))
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
            // A cache already open by that name is shared, under a new handle (measured:
            // two InitGameCache("Test.w3v") differ but read the same values).
            let name = a.s(0);
            let open = w.handles.iter().find_map(|(h, o)| match o {
                Obj::GameCache(c) if c.name.eq_ignore_ascii_case(&name) && !w.cache_alias.contains_key(h) => Some(*h),
                _ => None,
            });
            if let Some(first) = open {
                let alias = w.handles.insert(Obj::GameCache(Box::new(GameCache { name, ..Default::default() })));
                w.cache_alias.insert(alias, first);
                return handle(alias);
            }
            // A cache saved by an earlier game comes back with its contents.
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
        FlushStoredInteger => |w, _, a| { let k = (a.s(1), a.s(2)); if let Some(c) = gamecache(w, a.h(0)) { c.ints.remove(&k); } none() },
        FlushStoredReal => |w, _, a| { let k = (a.s(1), a.s(2)); if let Some(c) = gamecache(w, a.h(0)) { c.reals.remove(&k); } none() },
        FlushStoredBoolean => |w, _, a| { let k = (a.s(1), a.s(2)); if let Some(c) = gamecache(w, a.h(0)) { c.bools.remove(&k); } none() },
        FlushStoredString => |w, _, a| { let k = (a.s(1), a.s(2)); if let Some(c) = gamecache(w, a.h(0)) { c.strings.remove(&k); } none() },
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
