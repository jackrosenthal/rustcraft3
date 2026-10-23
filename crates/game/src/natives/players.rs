//! Players, forces, alliances, and player state.

use std::collections::HashMap;

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
            Some(p) => string(w.localized(&w.players[p as usize].name.clone())),
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
        SetPlayerHandicap => |w, _, a| { let v = a.r(1); with_player(w, a, |p| p.handicap = v) },
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
        SetPlayerState => |w, _, a| {
            let (s, v) = (a.h(1) as i32, a.i(2));
            with_player(w, a, |p| { p.state.insert(s, v); })
        },
        GetPlayerState => |w, _, a| int(player_arg(w, a, 0).map_or(0, |p| w.players[p as usize].get_state(a.h(1) as i32))),
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
            with_player(w, a, |p| { p.tech_max.insert(id, v); })
        },
        GetPlayerTechMaxAllowed => |w, _, a| {
            let id = id_from_int(a.i(1));
            int(player_arg(w, a, 0).and_then(|p| w.players[p as usize].tech_max.get(&id).copied()).unwrap_or(-1))
        },
        AddPlayerTechResearched => |w, _, a| {
            let (id, v) = (id_from_int(a.i(1)), a.i(2));
            let r = with_player(w, a, |p| { *p.tech_researched.entry(id).or_insert(0) += v; });
            if let Some(p) = player_arg(w, a, 0) { w.research_changed(p); }
            r
        },
        SetPlayerTechResearched => |w, _, a| {
            let (id, v) = (id_from_int(a.i(1)), a.i(2));
            let r = with_player(w, a, |p| { p.tech_researched.insert(id, v); });
            if let Some(p) = player_arg(w, a, 0) { w.research_changed(p); }
            r
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
        GetPlayerScore => |_, _, _| int(0),
        CreateForce => |w, _, _| handle(w.handles.insert(Obj::Force(Force::default()))),
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
        ForForce => |w, vm, a| for_force(w, vm, a.h(0), a.code(1)),
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
        PauseCompAI => |_, _, _| none(),
        GetAIDifficulty => |w, _, a| int(w.player_of(a.h(0)).and_then(|p| w.ai_difficulty.get(&p).copied()).unwrap_or(0)),
        RemoveGuardPosition => |_, _, _| none(),
        RecycleGuardPosition => |_, _, _| none(),
        RemoveAllGuardPositions => |_, _, _| none(),
        GetEnumPlayer => |w, _, _| NativeResult::Return(w.player_handle(w.enum_ctx.enum_player)),
        GetFilterPlayer => |w, _, _| NativeResult::Return(w.player_handle(w.enum_ctx.filter_player)),
    );
}
