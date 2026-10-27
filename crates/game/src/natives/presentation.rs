//! Text, sound, music, camera, and other presentation natives.  These record state or
//! queue requests for the presentation layer; they do not affect the simulation.

use crate::collections::HashMap;

use w3_jass::Value;

use super::{Args, boolean, handle, int, none, real, reg};
use crate::fog::{FOG_FOGGED, FOG_MASKED, FOG_VISIBLE, FogModifier, FogShape};
use crate::types::*;
use crate::world::{NativeFn, Request, World};

fn player_mask(w: &World, a: &[Value], n: usize) -> u32 {
    w.player_of(a.h(n)).map_or(0, |p| 1 << p)
}

fn stacked_place(w: &mut World, h: u32, by_position: bool, width: f32, height: f32) -> Option<crate::types::StackedSound> {
    let s = sound_mut(w, h)?;
    if s.path.is_empty() {
        return None;
    }
    let (file, unit, pos) = (s.path.clone(), s.unit, s.position);
    match (by_position, pos, unit) {
        (true, Some((x, y, _)), _) => Some(crate::types::StackedSound { file, sound: h, unit: None, x, y, width, height }),
        (_, _, Some(u)) => Some(crate::types::StackedSound { file, sound: h, unit: Some(u), x: 0.0, y: 0.0, width: 0.0, height: 0.0 }),
        _ => {
            log::warn!("Error, registering stacked sound for HLOOPINGSND that has no unitPtr or position specified!");
            None
        }
    }
}

/// A sound's length in seconds: SetSoundDuration's, else its file's (as the original
/// reads it, FUN_6f2331d0), kept once found.
fn sound_length(w: &mut World, h: u32) -> f32 {
    let Some(Obj::Sound(s)) = w.handles.get(h) else { return 0.0 };
    if s.duration_ms == 0 && !s.path.is_empty() {
        let ms = w.chain.read(&s.path).ok().and_then(|d| sound_file_ms(&d)).unwrap_or(0) as i32;
        if let Some(s) = sound_mut(w, h) {
            s.duration_ms = ms;
        }
    }
    sound_mut(w, h).map_or(0.0, |s| s.duration_ms as f32 / 1000.0)
}

/// How far a playing sound has played, in seconds; None once it has ended or when it
/// does not play.
pub fn sound_position(w: &mut World, h: u32) -> Option<f32> {
    let len = sound_length(w, h);
    let now = w.sleep_time;
    let s = sound_mut(w, h)?;
    let at = now - s.started?;
    (s.looping || at < len).then_some(if s.looping && len > 0.0 { at % len } else { at })
}

/// TriggerWaitForSound: until `offset` seconds before the sound ends, or no time when
/// it does not play (FUN_6f2975c0: the sound's length less the offset less where it
/// is).
pub fn sound_wait(w: &mut World, h: u32, offset: f32) -> f32 {
    let len = sound_length(w, h);
    let at = sound_position(w, h).unwrap_or(len);
    (len - offset - at).max(0.0)
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
            let name = w.player_of(a.h(0)).map(|p| w.player_name(p)).unwrap_or_default();
            let text = w.localized(&a.s(4)).replace("%s", &name);
            w.requests.push(Request::DisplayText { players, x: a.r(1), y: a.r(2), text, duration: a.r(3) });
            none()
        },
        ClearTextMessages => |w, _, _| { w.requests.push(Request::ClearText { players: u32::MAX }); none() },
        SetDayNightModels => |w, _, a| { w.day_night_models = (a.s(0), a.s(1)); none() },
        SetTerrainFogEx => |w, _, a| { w.terrain_fog = Some((a.i(0), a.r(1), a.r(2), a.r(3), [a.r(4), a.r(5), a.r(6)])); none() },
        ResetTerrainFog => |w, _, _| { w.terrain_fog = w.map_terrain_fog; none() },
        SetSkyModel => |w, _, a| { let m = a.s(0); w.sky_model = (!m.is_empty()).then_some(m); none() },
        NewSoundEnvironment => |w, _, a| { w.requests.push(Request::SoundEnvironment(a.s(0))); none() },
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
        SetMusicPlayPosition => |w, _, a| { w.requests.push(Request::MusicOffset(a.i(0))); none() },
        SetThematicMusicPlayPosition => |w, _, a| { w.requests.push(Request::MusicOffset(a.i(0))); none() },
        CreateSound => |w, _, a| {
            w.tags.take(b"+snd");
            let s = Sound { path: a.s(0), looping: a.b(1), is_3d: a.b(2), stop_when_out_of_range: a.b(3), fade_in: a.i(4), fade_out: a.i(5), eax: a.s(6), volume: 127, pitch: 1.0, ..Default::default() };
            handle(w.handles.insert(Obj::Sound(Box::new(s))))
        },
        CreateSoundFilenameWithLabel => |w, _, a| {
            w.tags.take(b"+snd");
            let s = Sound { path: a.s(0), looping: a.b(1), is_3d: a.b(2), stop_when_out_of_range: a.b(3), fade_in: a.i(4), fade_out: a.i(5), label: Some(a.s(6)), volume: 127, pitch: 1.0, ..Default::default() };
            handle(w.handles.insert(Obj::Sound(Box::new(s))))
        },
        CreateSoundFromLabel => |w, _, a| {
            w.tags.take(b"+snd");
            let s = Sound { label: Some(a.s(0)), looping: a.b(1), is_3d: a.b(2), stop_when_out_of_range: a.b(3), fade_in: a.i(4), fade_out: a.i(5), volume: 127, pitch: 1.0, ..Default::default() };
            handle(w.handles.insert(Obj::Sound(Box::new(s))))
        },
        CreateMIDISound => |w, _, a| {
            w.tags.take(b"+snd");
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
        GetSoundFileDuration => |w, _, a| {
            let path = a.s(0);
            int(w.chain.read(&path).ok().and_then(|d| sound_file_ms(&d)).map_or(0, |ms| ms as i32))
        },
        SetSoundDistances => |w, _, a| { let (lo, hi) = (a.r(1), a.r(2)); if let Some(s) = sound_mut(w, a.h(0)) { s.min_dist = lo; s.max_dist = hi; } none() },
        SetSoundConeAngles => |w, _, a| {
            let (h, cone) = (a.h(0), (a.r(1), a.r(2), a.i(3)));
            if let Some(s) = sound_mut(w, h) {
                s.cone = Some(cone);
            }
            w.requests.push(Request::SoundCone(h));
            none()
        },
        SetSoundConeOrientation => |w, _, a| {
            let (h, d) = (a.h(0), (a.r(1), a.r(2), a.r(3)));
            if let Some(s) = sound_mut(w, h) {
                s.cone_dir = Some(d);
            }
            w.requests.push(Request::SoundCone(h));
            none()
        },
        SetSoundPosition => |w, _, a| { let p = (a.r(1), a.r(2), a.r(3)); if let Some(s) = sound_mut(w, a.h(0)) { s.position = Some(p); } none() },
        SetSoundVelocity => |_, _, _| none(),
        AttachSoundToUnit => |w, _, a| {
            let p = w.unit(a.h(1)).map(|u| (u.x, u.y, 0.0));
            let u = a.h(1);
            if let (Some(p), Some(s)) = (p, sound_mut(w, a.h(0))) { s.position = Some(p); s.unit = Some(u); }
            none()
        },
        StartSound => |w, _, a| {
            let now = w.sleep_time;
            if let Some(s) = sound_mut(w, a.h(0)) { s.playing = true; s.started = Some(now); }
            w.requests.push(Request::PlaySound(a.h(0)));
            none()
        },
        StopSound => |w, _, a| {
            if let Some(s) = sound_mut(w, a.h(0)) { s.playing = false; s.started = None; }
            w.requests.push(Request::StopSound { sound: a.h(0), fade: a.b(2) });
            none()
        },
        KillSoundWhenDone => |_, _, _| none(),
        GetSoundIsPlaying => |w, _, a| boolean(sound_position(w, a.h(0)).is_some()),
        GetSoundIsLoading => |_, _, _| boolean(false),
        SetSoundPlayPosition => |w, _, a| {
            let (now, ms) = (w.sleep_time, a.i(1));
            if let Some(s) = sound_mut(w, a.h(0)).filter(|s| s.started.is_some()) {
                s.started = Some(now - ms as f32 / 1000.0);
            }
            w.requests.push(Request::SoundOffset { sound: a.h(0), ms });
            none()
        },
        // By position, the sound's place with the rect's size (FUN_6f359680); else the
        // unit it is attached to.  Keyed by the sound's file.
        RegisterStackedSound => |w, _, a| {
            let (h, by_position, width, height) = (a.h(0), a.b(1), a.r(2).abs(), a.r(3).abs());
            if let Some(st) = stacked_place(w, h, by_position, width, height) {
                if !w.stacked_sounds.contains(&st) {
                    w.stacked_sounds.push(st);
                }
            }
            none()
        },
        UnregisterStackedSound => |w, _, a| {
            let (h, by_position, width, height) = (a.h(0), a.b(1), a.r(2).abs(), a.r(3).abs());
            if let Some(st) = stacked_place(w, h, by_position, width, height) {
                w.stacked_sounds.retain(|s| !(s.file.eq_ignore_ascii_case(&st.file) && s.unit == st.unit && s.x == st.x && s.y == st.y));
            }
            none()
        },
        VolumeGroupSetVolume => |w, _, a| {
            let g = a.i(0);
            if (0..8).contains(&g) {
                let now = w.time;
                let level = w.volume_group(g as usize);
                w.volume_groups[g as usize] = (level, a.r(1), now);
            }
            none()
        },
        // Each group not at full volume, or moving, goes back to it.
        VolumeGroupReset => |w, _, _| {
            let now = w.time;
            for g in 0..8 {
                let (from, to, start) = w.volume_groups[g];
                if to != 1.0 || from != to && now - start < 1.0 {
                    let level = w.volume_group(g);
                    w.volume_groups[g] = (level, 1.0, now);
                }
            }
            none()
        },
        SetCameraPosition => |w, _, a| { w.pan_camera(a.r(0), a.r(1), 0.0); none() },
        SetCameraQuickPosition => |w, _, a| { w.requests.push(Request::QuickPosition { x: a.r(0), y: a.r(1) }); none() },
        PanCameraTo => |w, _, a| { w.pan_camera(a.r(0), a.r(1), 0.0); none() },
        PanCameraToTimed => |w, _, a| { w.pan_camera(a.r(0), a.r(1), a.r(2)); none() },
        PanCameraToWithZ => |w, _, a| { w.pan_camera(a.r(0), a.r(1), 0.0); none() },
        PanCameraToTimedWithZ => |w, _, a| { w.pan_camera(a.r(0), a.r(1), a.r(3)); none() },
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
            w.camera.follow_orient = a.b(3);
            none()
        },
        // The target controller with its orientation flags (FUN_6f3635e0 with 1, 1).
        SetCameraOrientController => |w, _, a| {
            let u = a.h(0);
            w.camera.follow = (u != 0).then(|| (u, a.r(1), a.r(2)));
            w.camera.follow_orient = true;
            none()
        },
        SetCameraRotateMode => |w, _, a| {
            let (x, y, sweep, duration) = (a.r(0), a.r(1), a.r(2), a.r(3));
            w.camera.rotate_about(x, y, sweep, duration);
            none()
        },
        SetCinematicCamera => |w, _, a| {
            let model = a.s(0);
            w.camera.cinematic = (!model.is_empty()).then(|| (model, w.time));
            none()
        },
        // The rect's corners as SetCameraBounds takes them: lower left, upper right,
        // upper left, lower right.
        SetCameraBoundsToRect => |w, _, a| {
            if let Some(Obj::Rect(r)) = w.handles.get(a.h(0)).cloned() {
                w.camera_bounds = [r.min_x, r.min_y, r.max_x, r.max_y, r.min_x, r.max_y, r.max_x, r.min_y];
            }
            none()
        },
        CameraSetSmoothingFactor => |w, _, a| {
            let f = a.r(0);
            w.camera.smoothing = if f > 0.0 && f < 10.0 { f } else if f > 0.0 { 10.0 } else { 0.0 };
            none()
        },
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
        GetCameraTargetPositionLoc => |w, _, _| { let (x, y) = (w.camera.x, w.camera.y); handle(w.insert_location(x, y)) },
        GetCameraEyePositionLoc => |w, _, _| { let (x, y, _) = camera_eye(w); handle(w.insert_location(x, y)) },
        GetCameraEyePositionX => |w, _, _| real(camera_eye(w).0),
        GetCameraEyePositionY => |w, _, _| real(camera_eye(w).1),
        GetCameraEyePositionZ => |w, _, _| real(camera_eye(w).2),
        CreateCameraSetup => |w, _, _| {
            w.tags.take(b"+cst");
            handle(w.handles.insert(Obj::CameraSetup(Box::default())))
        },
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
            handle(w.insert_location(x, y))
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
            w.tags.take(b"+fgm");
            let (Some(p), Some(r)) = (w.player_of(a.h(0)), w.rect(a.h(2))) else { return none() };
            fog_modifier(w, p, a.i(1), FogShape::Rect(r), a.b(3), a.b(4))
        },
        CreateFogModifierRadius => |w, _, a| {
            w.tags.take(b"+fgm");
            let Some(p) = w.player_of(a.h(0)) else { return none() };
            fog_modifier(w, p, a.i(1), FogShape::Circle(a.r(2), a.r(3), a.r(4)), a.b(5), a.b(6))
        },
        CreateFogModifierRadiusLoc => |w, _, a| {
            w.tags.take(b"+fgm");
            let (Some(p), Some((x, y))) = (w.player_of(a.h(0)), w.location(a.h(2))) else { return none() };
            fog_modifier(w, p, a.i(1), FogShape::Circle(x, y, a.r(3)), a.b(4), a.b(5))
        },
        DestroyFogModifier => |w, _, a| {
            if matches!(w.handles.get(a.h(0)), Some(Obj::FogModifier(_))) { w.handles.remove(a.h(0)); }
            none()
        },
        FogModifierStart => |w, _, a| {
            // Starting a modifier applies it at once, so the area is explored even if it
            // stops before the next update, or a masked one unexplored (measured:
            // UndeadX07a's intro cleanup "wipes visibility" with a masked modifier over
            // the map made and destroyed at once, and in the original what its intro
            // showed is black again; units see their own ground again next update).
            let m = match w.handles.get_mut(a.h(0)) {
                Some(Obj::FogModifier(m)) => {
                    m.active = true;
                    Some((m.player, m.state, m.shape.clone()))
                }
                _ => None,
            };
            if let (Some((p, state, shape)), Some(f)) = (m, w.fog.as_mut()) {
                f.set_state(p, state, &shape);
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
        // Whether the player has the unit in its detection (FUN_6f68bae0).
        IsUnitDetected => |w, _, a| { let (h, p) = (a.h(0), w.player_of(a.h(1)).unwrap_or(0)); boolean(w.detected_by(h, p)) },
        IsUnitInvisible => |w, _, a| { let (h, p) = (a.h(0), w.player_of(a.h(1)).unwrap_or(0)); boolean(w.is_invisible(h) && !w.detected_by(h, p)) },
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
        DialogCreate => |w, _, _| {
            w.tags.take(b"+dlg");
            handle(w.handles.insert(Obj::Dialog(Dialog::default())))
        },
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
            w.tags.take(b"+dlb");
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
            w.tags.take(b"+que");
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
            w.tags.take(b"+qui");
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
            w.tags.take(b"+dfc");
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
        // A trackable: its model standing at a point, which the mouse hits and tracks
        // (TriggerRegisterTrackableHitEvent, ...TrackEvent).
        CreateTrackable => |w, _, a| {
            w.tags.take(b"+trk");
            let (x, y) = (a.r(1), a.r(2));
            let z = w.ground_z(x, y);
            let h = w.handles.insert(Obj::Effect(crate::types::Effect { model: a.s(0), x, y, z, target: None, destroyed: false, facing: a.r(3) }));
            w.trackables.push(h);
            handle(h)
        },
        // The files preloaded since PreloadGenClear, for PreloadGenEnd (FUN_6f2407c0).
        Preload => |w, _, a| { let f = a.s(0); if !f.is_empty() { w.preload.files.push(f); } none() },
        PreloadEnd => |_, _, _| none(),
        PreloadStart => |_, _, _| none(),
        PreloadRefresh => |_, _, _| none(),
        PreloadEndEx => |_, _, _| none(),
        PreloadGenClear => |w, _, _| { w.preload.files.clear(); none() },
        PreloadGenStart => |w, _, _| { w.preload.started = Some(std::time::Instant::now()); none() },
        PreloadGenEnd => |w, _, a| { w.write_preload_file(&a.s(0)); none() },
        Preloader => |w, _, a| { w.run_preload_file(&a.s(0)); none() },
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
        SetTerrainType => |w, _, a| {
            let (x, y, t, variation, area, shape) = (a.r(0), a.r(1), a.i(2), a.i(3), a.i(4), a.i(5));
            w.set_terrain_type(x, y, (t as u32).to_be_bytes(), variation, area, shape != 0);
            none()
        },
        GetTerrainType => |w, _, a| int(w.terrain_corner(a.r(0), a.r(1)).and_then(|i| w.ground_tilesets.get(w.ground[i].0 as usize & 0xf)).map_or(0, |t| u32::from_be_bytes(*t) as i32)),
        GetTerrainVariance => |w, _, a| int(w.terrain_corner(a.r(0), a.r(1)).map_or(0, |i| (w.ground[i].1 & 0x1f) as i32)),
        SetBlight => |w, _, a| {
            let (x, y, r, add) = (a.r(1), a.r(2), a.r(3), a.b(4));
            w.blight_disc(add, x, y, r);
            none()
        },
        SetBlightRect => |w, _, a| {
            let Some(Obj::Rect(r)) = w.handles.get(a.h(1)).cloned() else { return none() };
            w.blight_rect(a.b(2), r.min_x, r.min_y, r.max_x, r.max_y);
            none()
        },
        SetBlightPoint => |w, _, a| {
            let (x, y, add) = (a.r(1), a.r(2), a.b(3));
            w.blight_point(add, x, y);
            none()
        },
        SetBlightLoc => |w, _, a| {
            let Some((x, y)) = w.location(a.h(1)) else { return none() };
            let (r, add) = (a.r(2), a.b(3));
            w.blight_disc(add, x, y, r);
            none()
        },
        IsPointBlighted => |w, _, a| boolean(w.point_blighted(a.r(0), a.r(1))),
        SetWaterBaseColor => |w, _, a| { w.water_color = [a.i(0), a.i(1), a.i(2), a.i(3)].map(|v| v.clamp(0, 255) as u8); none() },
        SetWaterDeforms => |w, _, a| { w.water_deforms = a.b(0); none() },
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
        PauseGame => |w, _, a| { w.script_paused = a.b(0); none() },
        SetPortraitLight => |_, _, _| none(),
        // Empty in the original (0x6f215310).
        SetUnitFog => |_, _, _| none(),
    );
}

/// A sound file's length in milliseconds (GetSoundFileDuration): a WAV's data over its
/// byte rate, or an MP3's frames counted from their headers.
pub fn sound_file_ms(data: &[u8]) -> Option<u32> {
    if data.len() > 12 && &data[0..4] == b"RIFF" && &data[8..12] == b"WAVE" {
        let (mut at, mut rate, mut size) = (12usize, 0u32, 0u32);
        while at + 8 <= data.len() {
            let id = &data[at..at + 4];
            let len = u32::from_le_bytes(data[at + 4..at + 8].try_into().ok()?) as usize;
            if id == b"fmt " && at + 16 <= data.len() {
                rate = u32::from_le_bytes(data[at + 16..at + 20].try_into().ok()?);
            } else if id == b"data" {
                size = len as u32;
            }
            at += 8 + len + (len & 1);
        }
        return (rate > 0).then(|| (size as u64 * 1000 / rate as u64) as u32);
    }
    // MP3: skip an ID3v2 tag, then walk the frames.
    let mut at = 0usize;
    if data.len() > 10 && &data[0..3] == b"ID3" {
        let n = data[6..10].iter().fold(0usize, |acc, &b| (acc << 7) | (b & 0x7f) as usize);
        at = 10 + n;
    }
    const RATES: [[u32; 3]; 4] = [[11025, 12000, 8000], [0, 0, 0], [22050, 24000, 16000], [44100, 48000, 32000]];
    const KBPS_V1: [u32; 16] = [0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 0];
    const KBPS_V2: [u32; 16] = [0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160, 0];
    let (mut samples, mut rate) = (0u64, 0u32);
    while at + 4 <= data.len() {
        if data[at] != 0xff || data[at + 1] & 0xe0 != 0xe0 {
            at += 1;
            continue;
        }
        let version = (data[at + 1] >> 3) & 3;
        let layer = (data[at + 1] >> 1) & 3;
        let br = (data[at + 2] >> 4) as usize;
        let sr = ((data[at + 2] >> 2) & 3) as usize;
        let pad = ((data[at + 2] >> 1) & 1) as u32;
        if version == 1 || layer != 1 || sr == 3 || br == 0 || br == 15 {
            at += 1;
            continue;
        }
        let r = RATES[version as usize][sr];
        let (kbps, per) = if version == 3 { (KBPS_V1[br], 1152) } else { (KBPS_V2[br], 576) };
        let len = (per / 8 * kbps * 1000 / r + pad) as usize;
        if len < 4 {
            break;
        }
        samples += per as u64;
        rate = r;
        at += len;
    }
    (rate > 0).then(|| (samples * 1000 / rate as u64) as u32)
}
