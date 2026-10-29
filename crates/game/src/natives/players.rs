//! Players, forces, alliances, and player state.

use crate::collections::HashMap;

use w3_jass::{NativeResult, Value, Vm};

use super::{Args, boolean, handle, int, none, real, reg, string};
use crate::objects::id_from_int;
use crate::types::*;
use crate::world::{NativeFn, World};

/// ALLIANCE_PASSIVE: the alliance flag IsPlayerAlly and IsPlayerEnemy read.
pub(crate) const ALLIANCE_PASSIVE: usize = 0;

pub(crate) fn player_arg(w: &World, a: &[Value], n: usize) -> Option<u8> {
    w.player_of(a.h(n))
}

fn with_player(w: &mut World, a: &[Value], f: impl FnOnce(&mut Player)) -> NativeResult {
    if let Some(p) = player_arg(w, a, 0) {
        f(&mut w.players[p as usize]);
    }
    none()
}

/// Whether a player commands a unit: their own, or one whose owner shares advanced
/// control with them, or shares control of a unit not a structure (structures too with
/// the setup's full shared unit control, 0x10000: FUN_6f1ea8b0, FUN_6f1e0fe0).
pub fn controls(w: &World, player: u8, h: u32) -> bool {
    let Some(u) = w.unit(h) else { return false };
    if u.owner == player {
        return true;
    }
    let Some(a) = w.players.get(u.owner as usize).and_then(|o| o.alliances.get(player as usize)) else { return false };
    const SHARED_CONTROL: usize = 6;
    const SHARED_ADVANCED_CONTROL: usize = 7;
    a[SHARED_ADVANCED_CONTROL] || (a[SHARED_CONTROL] && (!u.is_building || w.map_flags & 0x10000 != 0))
}

/// A player's color under the ally color filter (FUN_6f33c820), from UI\MiscData.txt
/// [TeamColorFilter]: the local player's ColorIndexPlayer, allies' ColorIndexAlly,
/// others' ColorIndexEnemy (ColorIndexCreep past the twelve players); an observer sees
/// each team in the color of its first player.  Neutral passive and beyond keep
/// `color`.
pub fn filter_color(w: &World, player: u8, color: i32) -> i32 {
    let me = w.local_player;
    if player >= 15 || me >= 15 {
        return color;
    }
    let index = |k: &str, d: i32| w.misc.get_str("TeamColorFilter", k).and_then(|v| v.trim().parse().ok()).unwrap_or(d);
    if w.players[me as usize].get_state(crate::types::player_state::OBSERVER) != 0 {
        if let Some(u) = (0..12u8).find(|&u| is_ally(w, player, u) && is_ally(w, u, player)) {
            return w.players[u as usize].color;
        }
    }
    if player == me {
        index("ColorIndexPlayer", 1)
    } else if is_ally(w, me, player) {
        index("ColorIndexAlly", 2)
    } else if player > 11 {
        index("ColorIndexCreep", 12)
    } else {
        index("ColorIndexEnemy", 0)
    }
}

pub fn is_ally(w: &World, a: u8, b: u8) -> bool {
    w.players.get(a as usize).is_some_and(|p| p.alliances[b as usize % PLAYER_COUNT][ALLIANCE_PASSIVE])
}

fn force_contains(w: &World, f: u32, p: u8) -> bool {
    w.force(f).is_some_and(|f| f.players & (1 << p) != 0)
}

/// Runs `func` for each player in a force with GetEnumPlayer set.
fn for_force(w: &mut World, vm: &mut Vm, force: u32, func: Option<w3_jass::FuncId>) -> NativeResult {
    let (Some(mask), Some(f)) = (w.force(force).map(|f| f.players), func) else { return none() };
    for p in 0..PLAYER_COUNT as u8 {
        if mask & (1 << p) != 0 {
            let saved = w.enum_ctx;
            w.enum_ctx.enum_player = p;
            let _ = vm.call(w, f, &[]);
            w.enum_ctx = saved;
        }
    }
    none()
}

fn enum_players(w: &mut World, vm: &mut Vm, force: u32, filter: u32, pred: impl Fn(&World, u8) -> bool) -> NativeResult {
    for p in 0..PLAYER_COUNT as u8 {
        if !pred(w, p) {
            continue;
        }
        let saved = w.enum_ctx;
        w.enum_ctx.filter_player = p;
        let ok = filter == 0 || w.eval_boolexpr(vm, filter);
        w.enum_ctx = saved;
        if ok {
            if let Some(f) = w.force_mut(force) {
                f.players |= 1 << p;
            }
        }
    }
    none()
}

pub fn register(t: &mut HashMap<&'static str, NativeFn>) {
    reg!(t,
        Player => |w, _, a| {
            let i = a.i(0);
            if (0..PLAYER_COUNT as i32).contains(&i) { NativeResult::Return(w.player_handle(i as u8)) } else { none() }
        },
        GetLocalPlayer => |w, _, _| NativeResult::Return(w.player_handle(w.local_player)),
        GetPlayerId => |w, _, a| int(player_arg(w, a, 0).map_or(0, |p| p as i32)),
        GetPlayerName => |w, _, a| match player_arg(w, a, 0) {
            Some(p) => string(w.player_name(p)),
            None => none(),
        },
        SetPlayerName => |w, _, a| { let n = a.s(1); with_player(w, a, |p| p.name = n) },
        GetPlayerColor => |w, _, a| int(player_arg(w, a, 0).map_or(0, |p| w.players[p as usize].color)),
        SetPlayerColor => |w, _, a| { let c = a.h(1) as i32; with_player(w, a, |p| p.color = c) },
        GetPlayerRace => |w, _, a| int(player_arg(w, a, 0).map_or(1, |p| w.players[p as usize].race)),
        SetPlayerRacePreference => |w, _, a| { let r = a.h(1) as i32; with_player(w, a, |p| p.race_preference = r) },
        SetPlayerRaceSelectable => |w, _, a| { let v = a.b(1); with_player(w, a, |p| p.race_selectable = v) },
        GetPlayerRaceSelectable => |w, _, a| boolean(player_arg(w, a, 0).is_some_and(|p| w.players[p as usize].race_selectable)),
        IsPlayerRacePrefSet => |w, _, a| {
            let pref = a.h(1) as i32;
            boolean(player_arg(w, a, 0).is_some_and(|p| w.players[p as usize].race_preference & pref != 0))
        },
        SetPlayerController => |w, _, a| { let c = a.h(1) as i32; with_player(w, a, |p| p.controller = c) },
        GetPlayerController => |w, _, a| int(player_arg(w, a, 0).map_or(map_control::NONE, |p| w.players[p as usize].controller)),
        GetPlayerSlotState => |w, _, a| int(player_arg(w, a, 0).map_or(0, |p| w.players[p as usize].slot_state)),
        GetPlayerStartLocation => |w, _, a| int(player_arg(w, a, 0).map_or(-1, |p| w.players[p as usize].start_location)),
        SetPlayerStartLocation => |w, _, a| { let s = a.i(1); with_player(w, a, |p| p.start_location = s) },
        ForcePlayerStartLocation => |w, _, a| { let s = a.i(1); with_player(w, a, |p| p.start_location = s) },
        SetPlayerTeam => |w, _, a| { let s = a.i(1); with_player(w, a, |p| p.team = s) },
        GetPlayerTeam => |w, _, a| int(player_arg(w, a, 0).map_or(0, |p| w.players[p as usize].team)),
        // Kept within 0.01 and 100 (measured: -1 and 0 read 0.01, 200 reads 100).  The
        // player's units' maximum life goes with what was asked, unclamped, kept whole,
        // their life in proportion; any left with none die (jass_SetPlayerHandicap passes
        // the raw value to its units; measured: a footman at 300 of 420 reads 150 of 210
        // at 0.5; at 0.01 3 of 4, and from there at 2, 630 of 840; at 0.005 2 of 2; at
        // 0 or -1 it dies).
        SetPlayerHandicap => |w, vm, a| {
            let raw = a.r(1);
            let Some(p) = player_arg(w, a, 0) else { return none() };
            let old = std::mem::replace(&mut w.players[p as usize].handicap, raw.clamp(0.01, 100.0));
            if (raw - old).abs() > 0.0 {
                let units: Vec<u32> = w.unit_order.iter().copied().filter(|&h| w.unit(h).is_some_and(|u| u.owner == p && !u.dead)).collect();
                let mut dying = Vec::new();
                for h in units {
                    let exact = w.base_max_life(h) * raw;
                    let u = w.unit_mut(h).unwrap();
                    if u.max_life > 0.0 {
                        u.life *= exact / u.max_life;
                    }
                    u.max_life = if raw == 1.0 { exact } else { exact.trunc() };
                    u.life = u.life.min(u.max_life);
                    if u.life <= 0.405 {
                        dying.push(h);
                    }
                }
                for h in dying {
                    super::units::kill_unit(w, vm, h, 0);
                }
            }
            none()
        },
        GetPlayerHandicap => |w, _, a| real(player_arg(w, a, 0).map_or(1.0, |p| w.players[p as usize].handicap)),
        SetPlayerHandicapXP => |w, _, a| { let v = a.r(1); with_player(w, a, |p| p.handicap_xp = v) },
        GetPlayerHandicapXP => |w, _, a| real(player_arg(w, a, 0).map_or(1.0, |p| w.players[p as usize].handicap_xp)),
        SetPlayerOnScoreScreen => |w, _, a| { let v = a.b(1); with_player(w, a, |p| p.on_score_screen = v) },
        IsPlayerObserver => |w, _, a| boolean(player_arg(w, a, 0).is_some_and(|p| w.players[p as usize].get_state(player_state::OBSERVER) != 0)),
        SetPlayerAlliance => |w, _, a| {
            let (Some(p), Some(o)) = (player_arg(w, a, 0), player_arg(w, a, 1)) else { return none() };
            let t = a.h(2) as usize;
            if t < ALLIANCE_TYPES {
                w.players[p as usize].alliances[o as usize][t] = a.b(3);
            }
            none()
        },
        GetPlayerAlliance => |w, _, a| {
            let (Some(p), Some(o)) = (player_arg(w, a, 0), player_arg(w, a, 1)) else { return boolean(false) };
            let t = a.h(2) as usize;
            boolean(t < ALLIANCE_TYPES && w.players[p as usize].alliances[o as usize][t])
        },
        IsPlayerAlly => |w, _, a| {
            let (Some(p), Some(o)) = (player_arg(w, a, 0), player_arg(w, a, 1)) else { return boolean(false) };
            boolean(is_ally(w, p, o))
        },
        IsPlayerEnemy => |w, _, a| {
            let (Some(p), Some(o)) = (player_arg(w, a, 0), player_arg(w, a, 1)) else { return boolean(false) };
            boolean(!is_ally(w, p, o))
        },
        IsPlayerInForce => |w, _, a| {
            let Some(p) = player_arg(w, a, 0) else { return boolean(false) };
            boolean(force_contains(w, a.h(1), p))
        },
        // Gold and lumber stay within 0 and 1,000,000 (FUN_6f1e98e0 keeps them in tenths
        // up to 10,000,000, a negative amount reading as too many and so as none).
        SetPlayerState => |w, _, a| {
            let (s, mut v) = (a.h(1) as i32, a.i(2));
            if matches!(s, crate::types::player_state::RESOURCE_GOLD | crate::types::player_state::RESOURCE_LUMBER) && !(0..=1_000_000).contains(&v) {
                v = if v < 0 { 0 } else { 1_000_000 };
            }
            with_player(w, a, |p| { p.state.insert(s, v); })
        },
        // The upkeep rates are the upkeep level's taxes (measured: 30 at 55 food used).
        GetPlayerState => |w, _, a| {
            let Some(p) = player_arg(w, a, 0) else { return int(0) };
            // Food counts units as they come and go (measured: five footmen made just
            // before read 10 more food used, and the upkeep they bring).
            use crate::types::player_state as ps;
            if matches!(a.h(1) as i32, ps::RESOURCE_FOOD_USED | ps::RESOURCE_FOOD_CAP | ps::GOLD_UPKEEP_RATE | ps::LUMBER_UPKEEP_RATE) {
                w.update_food();
            }
            int(match a.h(1) as i32 {
                crate::types::player_state::GOLD_UPKEEP_RATE => w.upkeep_tax(p).0,
                crate::types::player_state::LUMBER_UPKEEP_RATE => w.upkeep_tax(p).1,
                s => w.players[p as usize].get_state(s),
            })
        },
        SetPlayerTaxRate => |w, _, a| {
            let (Some(p), Some(o)) = (player_arg(w, a, 0), player_arg(w, a, 1)) else { return none() };
            let (r, v) = (a.h(2) as i32, a.i(3));
            w.players[p as usize].tax_rates.insert((o, r), v);
            none()
        },
        GetPlayerTaxRate => |w, _, a| {
            let (Some(p), Some(o)) = (player_arg(w, a, 0), player_arg(w, a, 1)) else { return int(0) };
            int(w.players[p as usize].tax_rates.get(&(o, a.h(2) as i32)).copied().unwrap_or(0))
        },
        SetPlayerTechMaxAllowed => |w, _, a| {
            let (id, v) = (id_from_int(a.i(1)), a.i(2));
            if let Some(p) = player_arg(w, a, 0) {
                w.tech_reg(p, id).max = v;
            }
            with_player(w, a, |p| { p.tech_max.insert(id, v); })
        },
        GetPlayerTechMaxAllowed => |w, _, a| {
            let id = id_from_int(a.i(1));
            let Some(p) = player_arg(w, a, 0) else { return int(0) };
            int(tech_max_allowed(w, p, id))
        },
        AddPlayerTechResearched => |w, _, a| {
            let (id, v) = (id_from_int(a.i(1)), a.i(2));
            let Some(p) = player_arg(w, a, 0) else { return none() };
            add_tech_researched(w, p, id, v);
            none()
        },
        // Only ever raises the level (jass_SetPlayerTechResearched adds the difference).
        SetPlayerTechResearched => |w, _, a| {
            let (id, v) = (id_from_int(a.i(1)), a.i(2));
            let Some(p) = player_arg(w, a, 0) else { return none() };
            let have = w.players[p as usize].tech_researched.get(&id).copied().unwrap_or(0);
            add_tech_researched(w, p, id, (v - have).max(0));
            none()
        },
        GetPlayerTechResearched => |w, _, a| {
            let id = id_from_int(a.i(1));
            boolean(player_arg(w, a, 0).is_some_and(|p| w.players[p as usize].tech_researched.get(&id).copied().unwrap_or(0) > 0))
        },
        // Research levels, or finished units of the type (and its equivalents unless
        // specificonly).
        GetPlayerTechCount => |w, _, a| {
            let id = id_from_int(a.i(1));
            int(player_arg(w, a, 0).map_or(0, |p| w.tech_count(p, id, !a.b(2))))
        },
        SetPlayerAbilityAvailable => |w, _, a| {
            let (id, on) = (id_from_int(a.i(1)), a.b(2));
            with_player(w, a, |p| { if on { p.abilities_unavailable.remove(&id); } else { p.abilities_unavailable.insert(id); } })
        },
        GetPlayerScore => |w, _, a| int(player_arg(w, a, 0).map_or(0, |p| w.player_score(p, a.h(1) as i32))),
        CreateForce => |w, _, _| {
            w.tags.take_all(&[b"+frc", b"`flg", b"`flg"]);
            handle(w.handles.insert(Obj::Force(Force::default())))
        },
        DestroyForce => |w, _, a| { if w.force(a.h(0)).is_some() { w.handles.remove(a.h(0)); } none() },
        ForceAddPlayer => |w, _, a| {
            let Some(p) = player_arg(w, a, 1) else { return none() };
            if let Some(f) = w.force_mut(a.h(0)) { f.players |= 1 << p; }
            none()
        },
        ForceRemovePlayer => |w, _, a| {
            let Some(p) = player_arg(w, a, 1) else { return none() };
            if let Some(f) = w.force_mut(a.h(0)) { f.players &= !(1 << p); }
            none()
        },
        ForceClear => |w, _, a| { if let Some(f) = w.force_mut(a.h(0)) { f.players = 0; } none() },
        ForceEnumPlayers => |w, vm, a| enum_players(w, vm, a.h(0), a.h(1), |_, _| true),
        ForceEnumPlayersCounted => |w, vm, a| enum_players(w, vm, a.h(0), a.h(1), |_, _| true),
        ForceEnumAllies => |w, vm, a| {
            let Some(p) = player_arg(w, a, 1) else { return none() };
            enum_players(w, vm, a.h(0), a.h(2), move |w, o| is_ally(w, p, o))
        },
        ForceEnumEnemies => |w, vm, a| {
            let Some(p) = player_arg(w, a, 1) else { return none() };
            enum_players(w, vm, a.h(0), a.h(2), move |w, o| !is_ally(w, p, o))
        },
        // Its function takes a handler, as ForGroup's does (traced: Melee's alliance
        // check's ForForce calls).
        ForForce => |w, vm, a| {
            if let Some(f) = a.code(1) {
                w.tags.handler(f);
            }
            for_force(w, vm, a.h(0), a.code(1))
        },
        StartMeleeAI => |w, _, a| {
            if let Some(p) = w.player_of(a.h(0)) { let s = a.s(1); w.start_ai(p, &s, true); }
            none()
        },
        StartCampaignAI => |w, _, a| {
            if let Some(p) = w.player_of(a.h(0)) { let s = a.s(1); w.start_ai(p, &s, false); }
            none()
        },
        CommandAI => |w, _, a| {
            if let Some(p) = w.player_of(a.h(0)) {
                if let Some(s) = w.ai.get_mut(&p) { s.commands.push_front((a.i(1), a.i(2))); }
            }
            none()
        },
        PauseCompAI => |w, _, a| {
            if let Some(p) = w.player_of(a.h(0)) {
                if let Some(s) = w.ai.get_mut(&p) {
                    s.paused += if a.b(1) { 1 } else { -1 };
                }
            }
            none()
        },
        GetAIDifficulty => |w, _, a| int(w.player_of(a.h(0)).and_then(|p| w.ai_difficulty.get(&p).copied()).unwrap_or(0)),
        RemoveGuardPosition => |w, _, a| {
            w.remove_guard_position(a.h(0));
            none()
        },
        RecycleGuardPosition => |w, _, a| {
            w.recycle_guard_position(a.h(0));
            none()
        },
        RemoveAllGuardPositions => |w, _, a| {
            if let Some(p) = w.player_of(a.h(0)) {
                w.remove_all_guard_positions(p);
            }
            none()
        },
        GetEnumPlayer => |w, _, _| NativeResult::Return(w.player_handle(w.enum_ctx.enum_player)),
        GetFilterPlayer => |w, _, _| NativeResult::Return(w.player_handle(w.enum_ctx.filter_player)),
    );
}

/// GetPlayerTechMaxAllowed: the set limit, compared unsigned against the type's own (an
/// upgrade's level count, else 2147483647), so an unset or negative limit reads that
/// (jass_GetPlayerTechMaxAllowed; measured: a footman unset reads 2147483647, Iron
/// Forged Swords 3, and Iron Forged Swords set to 10 reads 3).
fn tech_max_allowed(w: &World, p: u8, id: crate::objects::Id) -> i32 {
    let own = w.objects.upgrades.get(id).map_or(i32::MAX, |o| o.int("maxlevel").max(0)) as u32;
    let set = w.players[p as usize].tech_max.get(&id).copied().unwrap_or(-1) as u32;
    own.min(set) as i32
}

/// AddPlayerTechResearched: a level at a time while another is allowed, so a negative
/// amount does nothing and an upgrade stops at its level count and its maximum allowed
/// (measured: set to 3 with a maximum of 1 reads 1; adding -1 leaves it).
fn add_tech_researched(w: &mut World, p: u8, id: crate::objects::Id, n: i32) {
    if n <= 0 {
        return;
    }
    let have = w.players[p as usize].tech_researched.get(&id).copied().unwrap_or(0);
    let to = if w.objects.upgrades.get(id).is_some() { have.max((have + n).min(tech_max_allowed(w, p, id))) } else { have + n };
    if to != have {
        w.players[p as usize].tech_researched.insert(id, to);
        w.research_changed(p);
    }
}
