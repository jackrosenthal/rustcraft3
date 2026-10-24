//! Map setup, game options, and global game state.

use crate::collections::HashMap;

use super::{Args, boolean, int, none, real, reg, string};
use crate::types::*;
use crate::world::NativeFn;

/// Integer and real game states (common.j GAME_STATE_*).
const GAME_STATE_DIVINE_INTERVENTION: i32 = 0;
const GAME_STATE_DISCONNECTED: i32 = 1;
const GAME_STATE_TIME_OF_DAY: i32 = 2;

pub fn register(t: &mut HashMap<&'static str, NativeFn>) {
    reg!(t,
        SetMapName => |w, _, a| { w.map_name = a.s(0); none() },
        SetMapDescription => |w, _, a| { w.map_description = a.s(0); none() },
        SetPlayers => |w, _, a| { w.player_count = a.i(0); none() },
        SetTeams => |w, _, a| { w.team_count = a.i(0); none() },
        GetPlayers => |w, _, _| int(w.player_count),
        GetTeams => |w, _, _| int(w.team_count),
        SetGamePlacement => |w, _, a| { w.placement = a.h(0) as i32; none() },
        GetGamePlacement => |w, _, _| int(w.placement),
        SetGameTypeSupported => |w, _, a| {
            let t = a.h(0) as i32;
            if a.b(1) { w.game_types_supported |= t } else { w.game_types_supported &= !t }
            none()
        },
        IsGameTypeSupported => |w, _, a| boolean(w.game_types_supported & a.h(0) as i32 != 0),
        GetGameTypeSelected => |w, _, _| int(w.game_type_selected),
        SetMapFlag => |w, _, a| {
            let f = a.h(0) as i32;
            if a.b(1) { w.map_flags |= f } else { w.map_flags &= !f }
            none()
        },
        IsMapFlagSet => |w, _, a| boolean(w.map_flags & a.h(0) as i32 != 0),
        // Not while MAP_LOCK_SPEED is set; clamped to fast (as jass_SetGameSpeed).
        SetGameSpeed => |w, _, a| {
            if w.map_flags & 0x4000 == 0 {
                w.game_speed = (a.h(0) as i32).clamp(0, 2);
            }
            none()
        },
        GetGameSpeed => |w, _, _| int(w.game_speed),
        SetGameDifficulty => |w, _, a| { w.difficulty = a.h(0) as i32; none() },
        GetGameDifficulty => |w, _, _| int(w.difficulty),
        SetResourceDensity => |_, _, _| none(),
        SetCreatureDensity => |_, _, _| none(),
        GetResourceDensity => |_, _, _| int(1),
        GetCreatureDensity => |_, _, _| int(1),
        DefineStartLocation => |w, _, a| {
            let i = a.i(0).max(0) as usize;
            if w.start_locations.len() <= i {
                w.start_locations.resize(i + 1, StartLocation::default());
            }
            w.start_locations[i].x = a.r(1);
            w.start_locations[i].y = a.r(2);
            none()
        },
        DefineStartLocationLoc => |w, _, a| {
            let i = a.i(0).max(0) as usize;
            let (x, y) = w.location(a.h(1)).unwrap_or_default();
            if w.start_locations.len() <= i {
                w.start_locations.resize(i + 1, StartLocation::default());
            }
            w.start_locations[i].x = x;
            w.start_locations[i].y = y;
            none()
        },
        SetStartLocPrioCount => |w, _, a| {
            if let Some(s) = w.start_locations.get_mut(a.i(0).max(0) as usize) { s.prio_count = a.i(1); }
            none()
        },
        SetStartLocPrio => |w, _, a| {
            if let Some(s) = w.start_locations.get_mut(a.i(0).max(0) as usize) {
                s.prios.insert(a.i(1), (a.i(2), a.h(3) as i32));
            }
            none()
        },
        GetStartLocPrioSlot => |w, _, a| int(w.start_locations.get(a.i(0).max(0) as usize).and_then(|s| s.prios.get(&a.i(1))).map_or(0, |p| p.0)),
        GetStartLocPrio => |w, _, a| int(w.start_locations.get(a.i(0).max(0) as usize).and_then(|s| s.prios.get(&a.i(1))).map_or(0, |p| p.1)),
        GetStartLocationX => |w, _, a| real(w.start_locations.get(a.i(0).max(0) as usize).map_or(0.0, |s| s.x)),
        GetStartLocationY => |w, _, a| real(w.start_locations.get(a.i(0).max(0) as usize).map_or(0.0, |s| s.y)),
        GetStartLocationLoc => |w, _, a| {
            let (x, y) = w.start_locations.get(a.i(0).max(0) as usize).map_or((0.0, 0.0), |s| (s.x, s.y));
            super::handle(w.handles.insert(Obj::Location(x, y)))
        },
        GetFloatGameState => |w, _, a| real(match a.h(0) as i32 {
            GAME_STATE_TIME_OF_DAY => w.time_of_day,
            _ => 0.0,
        }),
        SetFloatGameState => |w, _, a| {
            if a.h(0) as i32 == GAME_STATE_TIME_OF_DAY { w.time_of_day = a.r(1); }
            none()
        },
        GetIntegerGameState => |_, _, a| int(match a.h(0) as i32 {
            GAME_STATE_DIVINE_INTERVENTION | GAME_STATE_DISCONNECTED => 0,
            _ => 0,
        }),
        SetIntegerGameState => |_, _, _| none(),
        SetTimeOfDayScale => |w, _, a| { w.time_of_day_scale = a.r(0); none() },
        GetTimeOfDayScale => |w, _, _| real(w.time_of_day_scale),
        SuspendTimeOfDay => |w, _, a| { w.time_of_day_suspended = a.b(0); none() },
        VersionGet => |w, _, _| int(w.version),
        VersionCompatible => |w, _, a| boolean(a.h(0) as i32 <= w.version),
        VersionSupported => |w, _, a| boolean(a.h(0) as i32 <= w.version),
        EndGame => |w, _, a| { w.requests.push(crate::world::Request::EndGame { score: a.b(0) }); none() },
        ChangeLevel => |w, _, a| { let map = a.s(0); w.requests.push(crate::world::Request::ChangeLevel { map, score: a.b(1) }); none() },
        SetMissionAvailable => |w, _, a| { w.persist.set_mission(a.i(0).max(0) as usize, a.i(1).max(0) as usize, a.b(2)); none() },
        SetCampaignAvailable => |w, _, a| { w.persist.campaign(a.i(0).max(0) as usize).available = a.b(1); none() },
        SetOpCinematicAvailable => |w, _, a| { w.persist.campaign(a.i(0).max(0) as usize).opening_cinematic = a.b(1); none() },
        SetEdCinematicAvailable => |w, _, a| { w.persist.campaign(a.i(0).max(0) as usize).ending_cinematic = a.b(1); none() },
        RestartGame => |w, _, _| { w.requests.push(crate::world::Request::Restart); none() },
        ReloadGame => |_, _, _| none(),
        SetCampaignMenuRace => |w, _, a| { w.persist.menu_race = a.h(0) as i32; none() },
        SetCampaignMenuRaceEx => |w, _, a| { w.persist.menu_race = a.i(0); none() },
        ForceCampaignSelectScreen => |w, _, _| { w.force_campaign_select = true; none() },
        LoadGame => |w, _, a| { w.requests.push(crate::world::Request::LoadGame(a.s(0))); none() },
        SaveGame => |w, _, a| { w.requests.push(crate::world::Request::SaveGame(a.s(0))); none() },
        RenameSaveDirectory => |_, _, _| boolean(false),
        RemoveSaveDirectory => |_, _, _| boolean(false),
        CopySaveGame => |_, _, _| boolean(false),
        SaveGameExists => |w, _, a| boolean(w.save_path(&a.s(0)).is_some_and(|p| p.exists())),
        SyncSelections => |_, _, _| none(),
        IsNoVictoryCheat => |w, _, _| boolean(w.cheat(crate::cheats::flag::NO_VICTORY)),
        IsNoDefeatCheat => |w, _, _| boolean(w.cheat(crate::cheats::flag::NO_DEFEAT)),
        Cheat => |_, _, _| none(),
        GetMapName => |w, _, _| string(w.map_name.clone()),
        SetEnemyStartLocPrioCount => |_, _, _| none(),
        SetEnemyStartLocPrio => |_, _, _| none(),
        SetAllyColorFilterState => |w, _, a| { w.ally_color_filter = a.i(0).clamp(0, 2); none() },
        GetAllyColorFilterState => |w, _, _| int(w.ally_color_filter),
        SetCreepCampFilterState => |_, _, _| none(),
        GetCreepCampFilterState => |_, _, _| boolean(false),
        EnableMinimapFilterButtons => |_, _, _| none(),
        EnableDragSelect => |w, _, a| { w.select_allowed.1 = a.b(0); none() },
        EnablePreSelect => |w, _, a| { w.select_allowed.2 = a.b(1); none() },
        // Turning selection off also clears the local player's (FUN_6f36c2f0).
        EnableSelect => |w, _, a| {
            w.select_allowed.0 = a.b(0);
            if !a.b(0) {
                let local = w.local_player;
                w.selection.remove(&local);
                w.selection_serial += 1;
            }
            none()
        },
        IsFogEnabled => |w, _, _| boolean(w.fog_enabled),
        IsFogMaskEnabled => |w, _, _| boolean(w.fog_mask_enabled),
        FogEnable => |w, _, a| { w.fog_enabled = a.b(0); none() },
        FogMaskEnable => |w, _, a| { w.fog_mask_enabled = a.b(0); none() },
    );
}
