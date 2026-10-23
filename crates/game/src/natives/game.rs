//! Map setup, game options, and global game state.

use std::collections::HashMap;

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
        SetGameSpeed => |w, _, a| { w.game_speed = a.h(0) as i32; none() },
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
        SuspendTimeOfDay => |_, _, _| none(),
        VersionGet => |w, _, _| int(w.version),
        VersionCompatible => |w, _, a| boolean(a.h(0) as i32 <= w.version),
        VersionSupported => |w, _, a| boolean(a.h(0) as i32 <= w.version),
        EndGame => |w, _, _| { w.requests.push(crate::world::Request::EndGame); none() },
        ChangeLevel => |_, _, _| none(),
        RestartGame => |_, _, _| none(),
        ReloadGame => |_, _, _| none(),
        SetCampaignMenuRace => |_, _, _| none(),
        SetCampaignMenuRaceEx => |_, _, _| none(),
        ForceCampaignSelectScreen => |_, _, _| none(),
        LoadGame => |_, _, _| none(),
        SaveGame => |_, _, _| none(),
        RenameSaveDirectory => |_, _, _| boolean(false),
        RemoveSaveDirectory => |_, _, _| boolean(false),
        CopySaveGame => |_, _, _| boolean(false),
        SaveGameExists => |_, _, _| boolean(false),
        SyncSelections => |_, _, _| none(),
        IsNoVictoryCheat => |_, _, _| boolean(false),
        IsNoDefeatCheat => |_, _, _| boolean(false),
        Cheat => |_, _, _| none(),
        GetMapName => |w, _, _| string(w.map_name.clone()),
        SetEnemyStartLocPrioCount => |_, _, _| none(),
        SetEnemyStartLocPrio => |_, _, _| none(),
        SetAllyColorFilterState => |_, _, _| none(),
        GetAllyColorFilterState => |_, _, _| int(0),
        SetCreepCampFilterState => |_, _, _| none(),
        GetCreepCampFilterState => |_, _, _| boolean(false),
        EnableMinimapFilterButtons => |_, _, _| none(),
        EnableDragSelect => |_, _, _| none(),
        EnablePreSelect => |_, _, _| none(),
        EnableSelect => |_, _, _| none(),
        IsFogEnabled => |w, _, _| boolean(w.fog_enabled),
        IsFogMaskEnabled => |w, _, _| boolean(w.fog_mask_enabled),
        FogEnable => |w, _, a| { w.fog_enabled = a.b(0); none() },
        FogMaskEnable => |w, _, a| { w.fog_mask_enabled = a.b(0); none() },
    );
}
