//! Text, sound, music, camera, and other presentation natives.  These record state or
//! queue requests for the presentation layer; they do not affect the simulation.

use std::collections::HashMap;

use w3_jass::Value;

use super::{Args, boolean, handle, int, none, real, reg};
use crate::fog::{FOG_FOGGED, FOG_MASKED, FOG_VISIBLE, FogModifier, FogShape};
use crate::types::*;
use crate::world::{NativeFn, Request, World};

fn player_mask(w: &World, a: &[Value], n: usize) -> u32 {
    w.player_of(a.h(n)).map_or(0, |p| 1 << p)
}

fn sound_mut(w: &mut World, h: u32) -> Option<&mut Sound> {
    match w.handles.get_mut(h) {
        Some(Obj::Sound(s)) => Some(s),
        _ => None,
    }
}

fn tag_mut(w: &mut World, h: u32) -> Option<&mut TextTag> {
    match w.handles.get_mut(h) {
        Some(Obj::TextTag(t)) => Some(t),
        _ => None,
    }
}

/// The camera's eye: back from the target along its angle of attack and rotation.
fn camera_eye(w: &World) -> (f32, f32, f32) {
    use crate::camera::*;
    let c = &w.camera;
    let (aoa, rot, d) = (c.fields[FIELD_ANGLE_OF_ATTACK].to_radians(), c.fields[FIELD_ROTATION].to_radians(), c.fields[FIELD_TARGET_DISTANCE]);
    let z = w.ground_z(c.x, c.y) + c.fields[FIELD_ZOFFSET];
    (c.x - rot.cos() * aoa.cos() * d, c.y - rot.sin() * aoa.cos() * d, z - aoa.sin() * d)
}

fn quest(w: &mut World, h: u32, f: impl FnOnce(&mut Quest)) -> w3_jass::NativeResult {
    if let Some(Obj::Quest(q)) = w.handles.get_mut(h) {
        f(q);
    }
    none()
}

fn quest_get(w: &World, h: u32) -> Option<&Quest> {
    match w.handles.get(h) {
        Some(Obj::Quest(q)) => Some(q),
        _ => None,
    }
}

fn opaque(w: &mut World, kind: &'static str) -> w3_jass::NativeResult {
    handle(w.handles.insert(Obj::Opaque(kind.to_string())))
}

fn fog_modifier(w: &mut World, player: u8, state: i32, shape: FogShape, shared: bool, after_units: bool) -> w3_jass::NativeResult {
    handle(w.handles.insert(Obj::FogModifier(FogModifier { player, state, shape, shared, after_units, active: false })))
}

fn set_fog_state(w: &mut World, player: u8, state: i32, shape: FogShape, _shared: bool) -> w3_jass::NativeResult {
    if let Some(f) = w.fog.as_mut() {
        f.set_state(player, state, &shape);
    }
    none()
}

fn add_dialog_button(w: &mut World, b: DialogButton) -> w3_jass::NativeResult {
    let dialog = b.dialog;
    if !matches!(w.handles.get(dialog), Some(Obj::Dialog(_))) {
        return none();
    }
    let h = w.handles.insert(Obj::DialogButton(b));
    if let Some(Obj::Dialog(d)) = w.handles.get_mut(dialog) {
        d.buttons.push(h);
    }
    handle(h)
}


pub fn register(t: &mut HashMap<&'static str, NativeFn>) {
    reg!(t,
        DisplayTextToPlayer => |w, _, a| {
            let (players, text) = (player_mask(w, a, 0), w.localized(&a.s(3)));
            w.requests.push(Request::DisplayText { players, x: a.r(1), y: a.r(2), text, duration: -1.0 });
            none()
        },
        DisplayTimedTextToPlayer => |w, _, a| {
            let (players, text) = (player_mask(w, a, 0), w.localized(&a.s(4)));
            w.requests.push(Request::DisplayText { players, x: a.r(1), y: a.r(2), text, duration: a.r(3) });
            none()
        },
        DisplayTimedTextFromPlayer => |w, _, a| {
            let players = 1 << w.local_player;
            let name = w.player_of(a.h(0)).map(|p| w.players[p as usize].name.clone()).unwrap_or_default();
            let text = w.localized(&a.s(4)).replace("%s", &name);
            w.requests.push(Request::DisplayText { players, x: a.r(1), y: a.r(2), text, duration: a.r(3) });
            none()
        },
        ClearTextMessages => |w, _, _| { w.requests.push(Request::ClearText { players: u32::MAX }); none() },
        SetDayNightModels => |w, _, a| { w.day_night_models = (a.s(0), a.s(1)); none() },
        SetTerrainFogEx => |w, _, a| { w.terrain_fog = Some((a.i(0), a.r(1), a.r(2), a.r(3), [a.r(4), a.r(5), a.r(6)])); none() },
        ResetTerrainFog => |w, _, _| { w.terrain_fog = w.map_terrain_fog; none() },
        SetSkyModel => |w, _, a| { let m = a.s(0); w.sky_model = (!m.is_empty()).then_some(m); none() },
        NewSoundEnvironment => |_, _, _| none(),
        SetMapMusic => |w, _, a| { w.requests.push(Request::PlayMusic(a.s(0))); none() },
        PlayMusic => |w, _, a| { w.requests.push(Request::PlayMusic(a.s(0))); none() },
        PlayMusicEx => |w, _, a| { w.requests.push(Request::PlayMusic(a.s(0))); none() },
        PlayThematicMusic => |w, _, a| { w.requests.push(Request::PlayThematic(a.s(0))); none() },
        PlayThematicMusicEx => |w, _, a| { w.requests.push(Request::PlayThematic(a.s(0))); none() },
        StopMusic => |w, _, a| { w.requests.push(Request::StopMusic(a.b(0))); none() },
        ClearMapMusic => |w, _, _| { w.requests.push(Request::StopMusic(false)); none() },
        ResumeMusic => |w, _, _| { w.requests.push(Request::ResumeMusic); none() },
        EndThematicMusic => |w, _, _| { w.requests.push(Request::EndThematic); none() },
        SetMusicVolume => |w, _, a| { w.requests.push(Request::MusicVolume(a.i(0) as f32 / 127.0)); none() },
        SetMusicPlayPosition => |_, _, _| none(),
        SetThematicMusicPlayPosition => |_, _, _| none(),
        CreateSound => |w, _, a| {
            let s = Sound { path: a.s(0), looping: a.b(1), is_3d: a.b(2), stop_when_out_of_range: a.b(3), fade_in: a.i(4), fade_out: a.i(5), eax: a.s(6), volume: 127, pitch: 1.0, ..Default::default() };
            handle(w.handles.insert(Obj::Sound(Box::new(s))))
        },
        CreateSoundFilenameWithLabel => |w, _, a| {
            let s = Sound { path: a.s(0), looping: a.b(1), is_3d: a.b(2), stop_when_out_of_range: a.b(3), fade_in: a.i(4), fade_out: a.i(5), label: Some(a.s(6)), volume: 127, pitch: 1.0, ..Default::default() };
            handle(w.handles.insert(Obj::Sound(Box::new(s))))
        },
        CreateSoundFromLabel => |w, _, a| {
            let s = Sound { label: Some(a.s(0)), looping: a.b(1), is_3d: a.b(2), stop_when_out_of_range: a.b(3), fade_in: a.i(4), fade_out: a.i(5), volume: 127, pitch: 1.0, ..Default::default() };
            handle(w.handles.insert(Obj::Sound(Box::new(s))))
        },
        CreateMIDISound => |w, _, a| {
            let s = Sound { label: Some(a.s(0)), fade_in: a.i(1), fade_out: a.i(2), volume: 127, pitch: 1.0, ..Default::default() };
            handle(w.handles.insert(Obj::Sound(Box::new(s))))
        },
        SetSoundParamsFromLabel => |w, _, a| { let l = a.s(1); if let Some(s) = sound_mut(w, a.h(0)) { s.label = Some(l); } none() },
        SetSoundDistanceCutoff => |w, _, a| { let v = a.r(1); if let Some(s) = sound_mut(w, a.h(0)) { s.cutoff = v; } none() },
        SetSoundChannel => |w, _, a| { let v = a.i(1); if let Some(s) = sound_mut(w, a.h(0)) { s.channel = v; } none() },
        SetSoundVolume => |w, _, a| { let v = a.i(1); if let Some(s) = sound_mut(w, a.h(0)) { s.volume = v; } none() },
        SetSoundPitch => |w, _, a| { let v = a.r(1); if let Some(s) = sound_mut(w, a.h(0)) { s.pitch = v; } none() },
        SetSoundDuration => |w, _, a| { let v = a.i(1); if let Some(s) = sound_mut(w, a.h(0)) { s.duration_ms = v; } none() },
        GetSoundDuration => |w, _, a| int(sound_mut(w, a.h(0)).map_or(0, |s| s.duration_ms)),
        GetSoundFileDuration => |_, _, _| int(0),
        SetSoundDistances => |w, _, a| { let (lo, hi) = (a.r(1), a.r(2)); if let Some(s) = sound_mut(w, a.h(0)) { s.min_dist = lo; s.max_dist = hi; } none() },
        SetSoundConeAngles => |_, _, _| none(),
        SetSoundConeOrientation => |_, _, _| none(),
        SetSoundPosition => |w, _, a| { let p = (a.r(1), a.r(2), a.r(3)); if let Some(s) = sound_mut(w, a.h(0)) { s.position = Some(p); } none() },
        SetSoundVelocity => |_, _, _| none(),
        AttachSoundToUnit => |w, _, a| {
            let p = w.unit(a.h(1)).map(|u| (u.x, u.y, 0.0));
            if let (Some(p), Some(s)) = (p, sound_mut(w, a.h(0))) { s.position = Some(p); }
            none()
        },
        StartSound => |w, _, a| {
            if let Some(s) = sound_mut(w, a.h(0)) { s.playing = true; }
            w.requests.push(Request::PlaySound(a.h(0)));
            none()
        },
        StopSound => |w, _, a| {
            if let Some(s) = sound_mut(w, a.h(0)) { s.playing = false; }
            w.requests.push(Request::StopSound { sound: a.h(0), fade: a.b(2) });
            none()
        },
        KillSoundWhenDone => |_, _, _| none(),
        GetSoundIsPlaying => |w, _, a| boolean(sound_mut(w, a.h(0)).is_some_and(|s| s.playing)),
        GetSoundIsLoading => |_, _, _| boolean(false),
        SetSoundPlayPosition => |_, _, _| none(),
        RegisterStackedSound => |_, _, _| none(),
        UnregisterStackedSound => |_, _, _| none(),
        VolumeGroupSetVolume => |_, _, _| none(),
        VolumeGroupReset => |_, _, _| none(),
        SetCameraPosition => |w, _, a| { w.camera.pan_to(a.r(0), a.r(1), 0.0); none() },
        SetCameraQuickPosition => |_, _, _| none(),
        PanCameraTo => |w, _, a| { w.camera.pan_to(a.r(0), a.r(1), 0.0); none() },
        PanCameraToTimed => |w, _, a| { w.camera.pan_to(a.r(0), a.r(1), a.r(2)); none() },
        PanCameraToWithZ => |w, _, a| { w.camera.pan_to(a.r(0), a.r(1), 0.0); none() },
        PanCameraToTimedWithZ => |w, _, a| { w.camera.pan_to(a.r(0), a.r(1), a.r(3)); none() },
        SetCameraField => |w, _, a| { w.camera.set_field(a.h(0) as usize, a.r(1), a.r(2)); none() },
        AdjustCameraField => |w, _, a| {
            let f = a.h(0) as usize;
            let v = w.camera.fields.get(f).copied().unwrap_or(0.0) + a.r(1);
            w.camera.set_field(f, v, a.r(2));
            none()
        },
        ResetToGameCamera => |w, _, a| {
            let d = a.r(0);
            w.camera.follow = None;
            w.camera.cinematic = None;
            for f in 0..crate::camera::FIELD_COUNT {
                let v = w.camera.defaults[f];
                w.camera.set_field(f, v, d);
            }
            none()
        },
        StopCamera => |w, _, _| { w.camera.stop(); none() },
        SetCameraTargetController => |w, _, a| {
            let u = a.h(0);
            w.camera.follow = (u != 0).then(|| (u, a.r(1), a.r(2)));
            none()
        },
        SetCameraOrientController => |_, _, _| none(),
        SetCameraRotateMode => |_, _, _| none(),
        SetCinematicCamera => |w, _, a| {
            let model = a.s(0);
            w.camera.cinematic = (!model.is_empty()).then(|| (model, w.time));
            none()
        },
        SetCameraBoundsToRect => |_, _, _| none(),
        CameraSetSmoothingFactor => |_, _, _| none(),
        CameraSetSourceNoise => |w, _, a| { w.camera.source_noise = (a.r(0), a.r(1), false); none() },
        CameraSetTargetNoise => |w, _, a| { w.camera.target_noise = (a.r(0), a.r(1), false); none() },
        CameraSetSourceNoiseEx => |w, _, a| { w.camera.source_noise = (a.r(0), a.r(1), a.b(2)); none() },
        CameraSetTargetNoiseEx => |w, _, a| { w.camera.target_noise = (a.r(0), a.r(1), a.b(2)); none() },
        GetCameraField => |w, _, a| {
            let f = a.h(0) as usize;
            let v = w.camera.fields.get(f).copied().unwrap_or(0.0);
            real(if crate::camera::is_angle(f) { v.to_radians() } else { v })
        },
        GetCameraTargetPositionX => |w, _, _| real(w.camera.x),
        GetCameraTargetPositionY => |w, _, _| real(w.camera.y),
        GetCameraTargetPositionZ => |w, _, _| { let (x, y) = (w.camera.x, w.camera.y); real(w.ground_z(x, y) + w.camera.fields[crate::camera::FIELD_ZOFFSET]) },
        GetCameraTargetPositionLoc => |w, _, _| { let (x, y) = (w.camera.x, w.camera.y); handle(w.handles.insert(Obj::Location(x, y))) },
        GetCameraEyePositionLoc => |w, _, _| { let (x, y, _) = camera_eye(w); handle(w.handles.insert(Obj::Location(x, y))) },
        GetCameraEyePositionX => |w, _, _| real(camera_eye(w).0),
        GetCameraEyePositionY => |w, _, _| real(camera_eye(w).1),
        GetCameraEyePositionZ => |w, _, _| real(camera_eye(w).2),
        CreateCameraSetup => |w, _, _| handle(w.handles.insert(Obj::CameraSetup(Box::default()))),
        CameraSetupSetField => |w, _, a| {
            if let Some(Obj::CameraSetup(c)) = w.handles.get_mut(a.h(0)) { c.fields.insert(a.h(1) as i32, a.r(2)); c.durations.insert(a.h(1) as i32, a.r(3)); }
            none()
        },
        CameraSetupGetField => |w, _, a| real(match w.handles.get(a.h(0)) {
            Some(Obj::CameraSetup(c)) => c.fields.get(&(a.h(1) as i32)).copied().unwrap_or(0.0),
            _ => 0.0,
        }),
        CameraSetupSetDestPosition => |w, _, a| {
            if let Some(Obj::CameraSetup(c)) = w.handles.get_mut(a.h(0)) { c.x = a.r(1); c.y = a.r(2); }
            none()
        },
        CameraSetupGetDestPositionX => |w, _, a| real(match w.handles.get(a.h(0)) { Some(Obj::CameraSetup(c)) => c.x, _ => 0.0 }),
        CameraSetupGetDestPositionY => |w, _, a| real(match w.handles.get(a.h(0)) { Some(Obj::CameraSetup(c)) => c.y, _ => 0.0 }),
        CameraSetupGetDestPositionLoc => |w, _, a| {
            let (x, y) = match w.handles.get(a.h(0)) { Some(Obj::CameraSetup(c)) => (c.x, c.y), _ => (0.0, 0.0) };
            handle(w.handles.insert(Obj::Location(x, y)))
        },
        CameraSetupApply => |w, _, a| { w.apply_camera_setup(a.h(0), a.b(1), a.b(2), None); none() },
        CameraSetupApplyWithZ => |w, _, a| { w.apply_camera_setup(a.h(0), true, false, None); none() },
        CameraSetupApplyForceDuration => |w, _, a| { w.apply_camera_setup(a.h(0), a.b(1), true, Some(a.r(2))); none() },
        CameraSetupApplyForceDurationWithZ => |w, _, a| { w.apply_camera_setup(a.h(0), true, true, Some(a.r(2))); none() },
        SetCinematicScene => |w, _, a| {
            let now = w.time;
            let scene = CineScene {
                unit_type: crate::objects::id_from_int(a.i(0)),
                color: a.h(1) as i32,
                speaker: w.localized(&a.s(2)),
                text: w.localized(&a.s(3)),
                ends: now + a.r(4),
                voice_ends: now + a.r(5),
            };
            w.cinematic.scene = Some(scene);
            none()
        },
        EndCinematicScene => |w, _, _| { w.cinematic.scene = None; none() },
        ForceCinematicSubtitles => |w, _, a| { w.cinematic.subtitles = a.b(0); none() },
        DisplayCineFilter => |w, _, a| { w.cinematic.filter.shown = a.b(0).then_some(w.time); none() },
        IsCineFilterDisplayed => |w, _, _| boolean(w.cinematic.filter.shown.is_some()),
        SetCineFilterTexture => |w, _, a| { w.cinematic.filter.texture = a.s(0); none() },
        SetCineFilterBlendMode => |w, _, a| { w.cinematic.filter.blend = a.h(0) as i32; none() },
        SetCineFilterTexMapFlags => |w, _, a| { w.cinematic.filter.tex_flags = a.h(0) as i32; none() },
        SetCineFilterStartUV => |w, _, a| { w.cinematic.filter.start_uv = [a.r(0), a.r(1), a.r(2), a.r(3)]; none() },
        SetCineFilterEndUV => |w, _, a| { w.cinematic.filter.end_uv = [a.r(0), a.r(1), a.r(2), a.r(3)]; none() },
        SetCineFilterStartColor => |w, _, a| { w.cinematic.filter.start_color = [a.i(0), a.i(1), a.i(2), a.i(3)]; none() },
        SetCineFilterEndColor => |w, _, a| { w.cinematic.filter.end_color = [a.i(0), a.i(1), a.i(2), a.i(3)]; none() },
        SetCineFilterDuration => |w, _, a| { w.cinematic.filter.duration = a.r(0); none() },
        // Weather effects start off until enabled.
        AddWeatherEffect => |w, _, a| {
            let Some(Obj::Rect(r)) = w.handles.get(a.h(0)).cloned() else { return handle(0) };
            let id = crate::objects::id_from_int(a.i(1));
            handle(w.handles.insert(Obj::Weather(Weather { rect: r, id, enabled: false })))
        },
        RemoveWeatherEffect => |w, _, a| { if matches!(w.handles.get(a.h(0)), Some(Obj::Weather(_))) { w.handles.remove(a.h(0)); } none() },
        EnableWeatherEffect => |w, _, a| { let on = a.b(1); if let Some(Obj::Weather(x)) = w.handles.get_mut(a.h(0)) { x.enabled = on; } none() },
        CreateFogModifierRect => |w, _, a| {
            let (Some(p), Some(r)) = (w.player_of(a.h(0)), w.rect(a.h(2))) else { return none() };
            fog_modifier(w, p, a.i(1), FogShape::Rect(r), a.b(3), a.b(4))
        },
        CreateFogModifierRadius => |w, _, a| {
            let Some(p) = w.player_of(a.h(0)) else { return none() };
            fog_modifier(w, p, a.i(1), FogShape::Circle(a.r(2), a.r(3), a.r(4)), a.b(5), a.b(6))
        },
        CreateFogModifierRadiusLoc => |w, _, a| {
            let (Some(p), Some((x, y))) = (w.player_of(a.h(0)), w.location(a.h(2))) else { return none() };
            fog_modifier(w, p, a.i(1), FogShape::Circle(x, y, a.r(3)), a.b(4), a.b(5))
        },
        DestroyFogModifier => |w, _, a| {
            if matches!(w.handles.get(a.h(0)), Some(Obj::FogModifier(_))) { w.handles.remove(a.h(0)); }
            none()
        },
        FogModifierStart => |w, _, a| {
            // Starting a modifier applies it at once, so the area is explored even if it
            // stops before the next update.
            let m = match w.handles.get_mut(a.h(0)) {
                Some(Obj::FogModifier(m)) => {
                    m.active = true;
                    Some((m.player, m.state, m.shape.clone()))
                }
                _ => None,
            };
            if let (Some((p, state, shape)), Some(f)) = (m, w.fog.as_mut()) {
                if state != FOG_MASKED {
                    f.set_state(p, state, &shape);
                }
            }
            none()
        },
        FogModifierStop => |w, _, a| { if let Some(Obj::FogModifier(m)) = w.handles.get_mut(a.h(0)) { m.active = false; } none() },
        SetFogStateRect => |w, _, a| {
            let (Some(p), Some(r)) = (w.player_of(a.h(0)), w.rect(a.h(2))) else { return none() };
            set_fog_state(w, p, a.i(1), FogShape::Rect(r), a.b(3))
        },
        SetFogStateRadius => |w, _, a| {
            let Some(p) = w.player_of(a.h(0)) else { return none() };
            set_fog_state(w, p, a.i(1), FogShape::Circle(a.r(2), a.r(3), a.r(4)), a.b(5))
        },
        SetFogStateRadiusLoc => |w, _, a| {
            let (Some(p), Some((x, y))) = (w.player_of(a.h(0)), w.location(a.h(2))) else { return none() };
            set_fog_state(w, p, a.i(1), FogShape::Circle(x, y, a.r(3)), a.b(4))
        },
        IsVisibleToPlayer => |w, _, a| { let p = w.player_of(a.h(2)).unwrap_or(0); boolean(w.fog_state(a.r(0), a.r(1), p) == FOG_VISIBLE) },
        IsLocationVisibleToPlayer => |w, _, a| {
            let p = w.player_of(a.h(1)).unwrap_or(0);
            boolean(w.location(a.h(0)).is_some_and(|(x, y)| w.fog_state(x, y, p) == FOG_VISIBLE))
        },
        IsFoggedToPlayer => |w, _, a| { let p = w.player_of(a.h(2)).unwrap_or(0); boolean(w.fog_state(a.r(0), a.r(1), p) == FOG_FOGGED) },
        IsLocationFoggedToPlayer => |w, _, a| {
            let p = w.player_of(a.h(1)).unwrap_or(0);
            boolean(w.location(a.h(0)).is_some_and(|(x, y)| w.fog_state(x, y, p) == FOG_FOGGED))
        },
        IsMaskedToPlayer => |w, _, a| { let p = w.player_of(a.h(2)).unwrap_or(0); boolean(w.fog_state(a.r(0), a.r(1), p) == FOG_MASKED) },
        IsLocationMaskedToPlayer => |w, _, a| {
            let p = w.player_of(a.h(1)).unwrap_or(0);
            boolean(w.location(a.h(0)).is_some_and(|(x, y)| w.fog_state(x, y, p) == FOG_MASKED))
        },
        IsUnitVisible => |w, _, a| { let p = w.player_of(a.h(1)).unwrap_or(0); boolean(w.unit_visible_to(a.h(0), p)) },
        IsUnitFogged => |w, _, a| {
            let p = w.player_of(a.h(1)).unwrap_or(0);
            boolean(w.unit(a.h(0)).is_some_and(|u| w.fog_state(u.x, u.y, p) == FOG_FOGGED) && !w.unit_visible_to(a.h(0), p))
        },
        IsUnitMasked => |w, _, a| {
            let p = w.player_of(a.h(1)).unwrap_or(0);
            boolean(w.unit(a.h(0)).is_some_and(|u| w.fog_state(u.x, u.y, p) == FOG_MASKED) && !w.unit_visible_to(a.h(0), p))
        },
        IsUnitDetected => |_, _, _| boolean(true),
        IsUnitInvisible => |_, _, _| boolean(false),
        CreateTextTag => |w, _, _| handle(w.handles.insert(Obj::TextTag(Box::new(TextTag { visible: true, color: [255; 4], permanent: true, ..Default::default() })))),
        DestroyTextTag => |w, _, a| { if tag_mut(w, a.h(0)).is_some() { w.handles.remove(a.h(0)); } none() },
        SetTextTagText => |w, _, a| { let (s, h) = (w.localized(&a.s(1)), a.r(2)); if let Some(t) = tag_mut(w, a.h(0)) { t.text = s; t.height = h; } none() },
        SetTextTagPos => |w, _, a| { let p = (a.r(1), a.r(2), a.r(3)); if let Some(t) = tag_mut(w, a.h(0)) { t.x = p.0; t.y = p.1; t.z = p.2; t.unit = None; } none() },
        SetTextTagPosUnit => |w, _, a| { let (u, z) = (a.h(1), a.r(2)); if let Some(t) = tag_mut(w, a.h(0)) { t.unit = Some((u, z)); } none() },
        SetTextTagColor => |w, _, a| { let c = [a.i(1) as u8, a.i(2) as u8, a.i(3) as u8, a.i(4) as u8]; if let Some(t) = tag_mut(w, a.h(0)) { t.color = c; } none() },
        SetTextTagVelocity => |w, _, a| { let v = (a.r(1), a.r(2)); if let Some(t) = tag_mut(w, a.h(0)) { t.velocity = v; } none() },
        SetTextTagVisibility => |w, _, a| { let v = a.b(1); if let Some(t) = tag_mut(w, a.h(0)) { t.visible = v; } none() },
        SetTextTagSuspended => |w, _, a| { let v = a.b(1); if let Some(t) = tag_mut(w, a.h(0)) { t.suspended = v; } none() },
        SetTextTagPermanent => |w, _, a| { let v = a.b(1); if let Some(t) = tag_mut(w, a.h(0)) { t.permanent = v; } none() },
        SetTextTagAge => |w, _, a| { let v = a.r(1); if let Some(t) = tag_mut(w, a.h(0)) { t.age = v; } none() },
        SetTextTagLifespan => |w, _, a| { let v = a.r(1); if let Some(t) = tag_mut(w, a.h(0)) { t.lifespan = v; } none() },
        SetTextTagFadepoint => |w, _, a| { let v = a.r(1); if let Some(t) = tag_mut(w, a.h(0)) { t.fadepoint = v; } none() },
        DialogCreate => |w, _, _| handle(w.handles.insert(Obj::Dialog(Dialog::default()))),
        DialogDestroy => |w, _, a| {
            if let Some(Obj::Dialog(d)) = w.handles.get(a.h(0)) {
                for b in d.buttons.clone() {
                    w.handles.remove(b);
                }
                w.handles.remove(a.h(0));
            }
            none()
        },
        DialogClear => |w, _, a| {
            let buttons = match w.handles.get_mut(a.h(0)) {
                Some(Obj::Dialog(d)) => { d.message.clear(); std::mem::take(&mut d.buttons) }
                _ => Vec::new(),
            };
            for b in buttons {
                w.handles.remove(b);
            }
            none()
        },
        DialogSetMessage => |w, _, a| {
            let text = w.localized(&a.s(1));
            if let Some(Obj::Dialog(d)) = w.handles.get_mut(a.h(0)) { d.message = text; }
            none()
        },
        DialogAddButton => |w, _, a| {
            let (dialog, text, hotkey) = (a.h(0), w.localized(&a.s(1)), a.i(2));
            add_dialog_button(w, DialogButton { dialog, text, hotkey, quit: None })
        },
        DialogAddQuitButton => |w, _, a| {
            let (dialog, score, text, hotkey) = (a.h(0), a.b(1), w.localized(&a.s(2)), a.i(3));
            add_dialog_button(w, DialogButton { dialog, text, hotkey, quit: Some(score) })
        },
        DialogDisplay => |w, _, a| {
            let (p, show) = (w.player_of(a.h(0)), a.b(2));
            if let (Some(p), Some(Obj::Dialog(d))) = (p, w.handles.get_mut(a.h(1))) {
                if show { d.shown |= 1 << p; } else { d.shown &= !(1 << p); }
            }
            none()
        },
        CreateQuest => |w, _, _| {
            let q = w.handles.insert(Obj::Quest(Quest::default()));
            w.quests.push(q);
            handle(q)
        },
        DestroyQuest => |w, _, a| {
            let q = a.h(0);
            w.quests.retain(|&x| x != q);
            if let Some(Obj::Quest(qu)) = w.handles.get(q) {
                for i in qu.items.clone() {
                    w.handles.remove(i);
                }
            }
            w.handles.remove(q);
            none()
        },
        QuestSetTitle => |w, _, a| { let t = w.localized(&a.s(1)); quest(w, a.h(0), |q| q.title = t) },
        QuestSetDescription => |w, _, a| { let t = w.localized(&a.s(1)); quest(w, a.h(0), |q| q.description = t) },
        QuestSetIconPath => |w, _, a| { let t = a.s(1); quest(w, a.h(0), |q| q.icon = t) },
        QuestSetRequired => |w, _, a| { let v = a.b(1); quest(w, a.h(0), |q| q.required = v) },
        QuestSetCompleted => |w, _, a| { let v = a.b(1); quest(w, a.h(0), |q| q.completed = v) },
        QuestSetDiscovered => |w, _, a| { let v = a.b(1); quest(w, a.h(0), |q| q.discovered = v) },
        QuestSetFailed => |w, _, a| { let v = a.b(1); quest(w, a.h(0), |q| q.failed = v) },
        QuestSetEnabled => |w, _, a| { let v = a.b(1); quest(w, a.h(0), |q| q.enabled = v) },
        IsQuestRequired => |w, _, a| boolean(quest_get(w, a.h(0)).is_some_and(|q| q.required)),
        IsQuestCompleted => |w, _, a| boolean(quest_get(w, a.h(0)).is_some_and(|q| q.completed)),
        IsQuestDiscovered => |w, _, a| boolean(quest_get(w, a.h(0)).is_some_and(|q| q.discovered)),
        IsQuestFailed => |w, _, a| boolean(quest_get(w, a.h(0)).is_some_and(|q| q.failed)),
        IsQuestEnabled => |w, _, a| boolean(quest_get(w, a.h(0)).is_some_and(|q| q.enabled)),
        QuestCreateItem => |w, _, a| {
            let q = a.h(0);
            let i = w.handles.insert(Obj::QuestItem(QuestItem { quest: q, ..Default::default() }));
            if let Some(Obj::Quest(qu)) = w.handles.get_mut(q) {
                qu.items.push(i);
            }
            handle(i)
        },
        QuestItemSetDescription => |w, _, a| {
            let t = w.localized(&a.s(1));
            if let Some(Obj::QuestItem(i)) = w.handles.get_mut(a.h(0)) { i.description = t; }
            none()
        },
        QuestItemSetCompleted => |w, _, a| {
            let v = a.b(1);
            if let Some(Obj::QuestItem(i)) = w.handles.get_mut(a.h(0)) { i.completed = v; }
            none()
        },
        IsQuestItemCompleted => |w, _, a| boolean(matches!(w.handles.get(a.h(0)), Some(Obj::QuestItem(i)) if i.completed)),
        CreateDefeatCondition => |w, _, _| {
            let d = w.handles.insert(Obj::DefeatCondition(String::new()));
            w.defeat_conditions.push(d);
            handle(d)
        },
        DestroyDefeatCondition => |w, _, a| {
            let d = a.h(0);
            w.defeat_conditions.retain(|&x| x != d);
            w.handles.remove(d);
            none()
        },
        DefeatConditionSetDescription => |w, _, a| {
            let t = w.localized(&a.s(1));
            if let Some(Obj::DefeatCondition(s)) = w.handles.get_mut(a.h(0)) { *s = t; }
            none()
        },
        FlashQuestDialogButton => |w, _, _| { w.quest_flash = true; none() },
        ForceQuestDialogUpdate => |_, _, _| none(),
        CreateTrackable => |w, _, _| opaque(w, "trackable"),
        Preload => |_, _, _| none(),
        PreloadEnd => |_, _, _| none(),
        PreloadStart => |_, _, _| none(),
        PreloadRefresh => |_, _, _| none(),
        PreloadEndEx => |_, _, _| none(),
        PreloadGenClear => |_, _, _| none(),
        PreloadGenStart => |_, _, _| none(),
        PreloadGenEnd => |_, _, _| none(),
        Preloader => |_, _, _| none(),
        PingMinimap => |w, _, a| { w.requests.push(Request::Ping { x: a.r(0), y: a.r(1), duration: a.r(2), color: None }); none() },
        PingMinimapEx => |w, _, a| {
            let c = [a.i(3), a.i(4), a.i(5)].map(|v| v.clamp(0, 255) as u8);
            w.requests.push(Request::Ping { x: a.r(0), y: a.r(1), duration: a.r(2), color: Some(c) });
            none()
        },
        SetDoodadAnimation => |w, _, a| {
            let area = crate::world::DoodadArea::Circle { x: a.r(0), y: a.r(1), radius: a.r(2), nearest: a.b(4) };
            w.requests.push(Request::DoodadAnimation { id: crate::objects::id_from_int(a.i(3)), area, anim: a.s(5), random: a.b(6) });
            none()
        },
        SetDoodadAnimationRect => |w, _, a| {
            let Some(Obj::Rect(r)) = w.handles.get(a.h(0)).cloned() else { return none() };
            let area = crate::world::DoodadArea::Rect { min_x: r.min_x, min_y: r.min_y, max_x: r.max_x, max_y: r.max_y };
            w.requests.push(Request::DoodadAnimation { id: crate::objects::id_from_int(a.i(1)), area, anim: a.s(2), random: a.b(3) });
            none()
        },
        SetTerrainType => |_, _, _| none(),
        GetTerrainType => |_, _, _| int(0),
        GetTerrainVariance => |_, _, _| int(0),
        SetBlight => |_, _, _| none(),
        SetBlightRect => |_, _, _| none(),
        SetBlightPoint => |_, _, _| none(),
        SetBlightLoc => |_, _, _| none(),
        IsPointBlighted => |_, _, _| boolean(false),
        SetWaterBaseColor => |_, _, _| none(),
        SetWaterDeforms => |_, _, _| none(),
        EnableOcclusion => |_, _, _| none(),
        SetIntroShotText => |_, _, _| none(),
        SetIntroShotModel => |_, _, _| none(),
        EnableUserControl => |w, _, a| { w.cinematic.user_control = a.b(0); none() },
        EnableUserUI => |w, _, a| { w.cinematic.user_ui = a.b(0); none() },
        ShowInterface => |w, _, a| {
            let now = w.time;
            let c = &mut w.cinematic;
            c.interface = a.b(0);
            c.interface_changed = now;
            c.interface_fade = a.r(1);
            none()
        },
        PauseGame => |_, _, _| none(),
        SetPortraitLight => |_, _, _| none(),
        SetUnitFog => |_, _, _| none(),
    );
}
