//! Images, ubersplats, lightning, and terrain deformations.

use std::collections::HashMap;

use super::{Args, boolean, handle, none, real};
use crate::types::*;
use crate::world::{NativeFn, World};

fn image(w: &mut World, h: u32) -> Option<&mut Image> {
    match w.handles.get_mut(h) {
        Some(Obj::Image(i)) => Some(i),
        _ => None,
    }
}

fn ubersplat(w: &mut World, h: u32) -> Option<&mut Ubersplat> {
    match w.handles.get_mut(h) {
        Some(Obj::Ubersplat(u)) => Some(u),
        _ => None,
    }
}

fn lightning(w: &mut World, h: u32) -> Option<&mut Lightning> {
    match w.handles.get_mut(h) {
        Some(Obj::Lightning(l)) => Some(l),
        _ => None,
    }
}

fn lightning_color(w: &mut World, h: u32, k: usize) -> f32 {
    lightning(w, h).map_or(0.0, |l| l.color[k])
}

/// A lightning type's color (R, G, B, A from 0 to 1), or None if it has no row.
fn lightning_type_color(w: &mut World, code: &str) -> Option<[f32; 4]> {
    let chain = w.chain.clone();
    let t = w.lightning_data.get_or_insert_with(|| chain.read_string("Splats\\LightningData.slk").map(|s| w3_formats::slk::Table::parse(&s)).unwrap_or_default());
    if !t.rows.contains_key(code) {
        return None;
    }
    let n = |c: &str| t.get(code, c).map(|v| v.as_str()).unwrap_or_default().trim().parse::<f32>().unwrap_or(255.0) / 255.0;
    Some([n("R"), n("G"), n("B"), n("A")])
}

/// Whether the local player sees either end (checkVisibility).
fn ends_visible(w: &World, from: [f32; 3], to: [f32; 3]) -> bool {
    let p = w.local_player;
    w.fog_state(from[0], from[1], p) == crate::fog::FOG_VISIBLE || w.fog_state(to[0], to[1], p) == crate::fog::FOG_VISIBLE
}

/// AddLightning and AddLightningEx: nothing for an unknown type, or when checking
/// visibility and neither end is visible.
fn add_lightning(w: &mut World, code: String, check: bool, from: [f32; 3], to: [f32; 3]) -> super::NativeResult {
    if check && !ends_visible(w, from, to) {
        return handle(0);
    }
    let Some(color) = lightning_type_color(w, &code) else { return handle(0) };
    handle(w.handles.insert(Obj::Lightning(Lightning { code, check_visibility: check, from, to, color })))
}

fn move_lightning(w: &mut World, h: u32, check: bool, from: [f32; 3], to: [f32; 3]) -> super::NativeResult {
    if check && !ends_visible(w, from, to) {
        return boolean(false);
    }
    boolean(lightning(w, h).map(|l| { l.from = from; l.to = to; }).is_some())
}

/// A terrain deformation (TerrainDeform*): its kind and parameters, kept for drawing.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum Deform {
    Crater { x: f32, y: f32, radius: f32, depth: f32, duration: f32, permanent: bool },
    Ripple { x: f32, y: f32, radius: f32, depth: f32, duration: f32, count: i32, space_wave: f32, time_wave: f32, radius_start_pct: f32, limit_neg: bool },
    Wave { x: f32, y: f32, dir_x: f32, dir_y: f32, distance: f32, speed: f32, radius: f32, depth: f32, trail_time: f32, count: i32 },
    Random { x: f32, y: f32, radius: f32, min_delta: f32, max_delta: f32, duration: f32, update_interval: f32 },
}

pub fn register(t: &mut HashMap<&'static str, NativeFn>) {
    reg!(t,
        CreateImage => |w, _, a| {
            let i = Image {
                file: a.s(0),
                size: [a.r(1), a.r(2), a.r(3)],
                pos: [a.r(4), a.r(5), a.r(6)],
                origin: [a.r(7), a.r(8), a.r(9)],
                kind: a.i(10),
                // Made shown but not rendered (flags 1 and 2; FUN_6f7309a0).
                shown: true,
                render: false,
                render_always: false,
                above_water: (false, false),
                constant_height: None,
                color: [255; 4],
            };
            handle(w.handles.insert(Obj::Image(i)))
        },
        DestroyImage => |w, _, a| { if image(w, a.h(0)).is_some() { w.handles.remove(a.h(0)); } none() },
        ShowImage => |w, _, a| { let on = a.b(1); if let Some(i) = image(w, a.h(0)) { i.shown = on; } none() },
        SetImageConstantHeight => |w, _, a| { let (on, z) = (a.b(1), a.r(2)); if let Some(i) = image(w, a.h(0)) { i.constant_height = on.then_some(z); } none() },
        SetImagePosition => |w, _, a| { let p = [a.r(1), a.r(2), a.r(3)]; if let Some(i) = image(w, a.h(0)) { i.pos = p; } none() },
        SetImageColor => |w, _, a| { let c = [a.i(1), a.i(2), a.i(3), a.i(4)].map(|v| v.clamp(0, 255) as u8); if let Some(i) = image(w, a.h(0)) { i.color = c; } none() },
        SetImageRender => |w, _, a| { let on = a.b(1); if let Some(i) = image(w, a.h(0)) { i.render = on; } none() },
        SetImageRenderAlways => |w, _, a| { let on = a.b(1); if let Some(i) = image(w, a.h(0)) { i.render_always = on; } none() },
        SetImageAboveWater => |w, _, a| { let v = (a.b(1), a.b(2)); if let Some(i) = image(w, a.h(0)) { i.above_water = v; } none() },
        SetImageType => |w, _, a| { let k = a.i(1); if let Some(i) = image(w, a.h(0)) { i.kind = k; } none() },

        CreateUbersplat => |w, _, a| {
            let u = Ubersplat {
                x: a.r(0),
                y: a.r(1),
                name: a.s(2),
                color: [a.i(3), a.i(4), a.i(5), a.i(6)].map(|v| v.clamp(0, 255) as u8),
                paused: a.b(7),
                no_birth: a.b(8),
                shown: true,
                render: true,
                render_always: false,
                age: Some(0.0),
            };
            handle(w.handles.insert(Obj::Ubersplat(u)))
        },
        DestroyUbersplat => |w, _, a| { if ubersplat(w, a.h(0)).is_some() { w.handles.remove(a.h(0)); } none() },
        ResetUbersplat => |w, _, a| { if let Some(u) = ubersplat(w, a.h(0)) { u.age = Some(0.0); } none() },
        FinishUbersplat => |w, _, a| { if let Some(u) = ubersplat(w, a.h(0)) { u.age = None; } none() },
        ShowUbersplat => |w, _, a| { let on = a.b(1); if let Some(u) = ubersplat(w, a.h(0)) { u.shown = on; } none() },
        SetUbersplatRender => |w, _, a| { let on = a.b(1); if let Some(u) = ubersplat(w, a.h(0)) { u.render = on; } none() },
        SetUbersplatRenderAlways => |w, _, a| { let on = a.b(1); if let Some(u) = ubersplat(w, a.h(0)) { u.render_always = on; } none() },

        // AddLightning and MoveLightning put the ends on the ground.
        AddLightning => |w, _, a| {
            let (f, t) = ([a.r(2), a.r(3), w.ground_z(a.r(2), a.r(3))], [a.r(4), a.r(5), w.ground_z(a.r(4), a.r(5))]);
            add_lightning(w, a.s(0), a.b(1), f, t)
        },
        AddLightningEx => |w, _, a| add_lightning(w, a.s(0), a.b(1), [a.r(2), a.r(3), a.r(4)], [a.r(5), a.r(6), a.r(7)]),
        DestroyLightning => |w, _, a| boolean(lightning(w, a.h(0)).is_some() && w.handles.remove(a.h(0)).is_some()),
        MoveLightning => |w, _, a| {
            let (f, t) = ([a.r(2), a.r(3), w.ground_z(a.r(2), a.r(3))], [a.r(4), a.r(5), w.ground_z(a.r(4), a.r(5))]);
            move_lightning(w, a.h(0), a.b(1), f, t)
        },
        MoveLightningEx => |w, _, a| move_lightning(w, a.h(0), a.b(1), [a.r(2), a.r(3), a.r(4)], [a.r(5), a.r(6), a.r(7)]),
        // SetLightningColor keeps each channel as a byte (value times 255).
        SetLightningColor => |w, _, a| {
            let c = [a.r(1), a.r(2), a.r(3), a.r(4)].map(|v| ((v * 255.0) as i32 & 0xff) as f32 / 255.0);
            boolean(lightning(w, a.h(0)).map(|l| l.color = c).is_some())
        },
        GetLightningColorR => |w, _, a| real(lightning_color(w, a.h(0), 0)),
        GetLightningColorG => |w, _, a| real(lightning_color(w, a.h(0), 1)),
        GetLightningColorB => |w, _, a| real(lightning_color(w, a.h(0), 2)),
        GetLightningColorA => |w, _, a| real(lightning_color(w, a.h(0), 3)),

        TerrainDeformCrater => |w, _, a| {
            let d = Deform::Crater { x: a.r(0), y: a.r(1), radius: a.r(2), depth: a.r(3), duration: a.i(4) as f32 / 1000.0, permanent: a.b(5) };
            handle(w.add_deformation(d))
        },
        TerrainDeformRipple => |w, _, a| {
            let d = Deform::Ripple { x: a.r(0), y: a.r(1), radius: a.r(2), depth: a.r(3), duration: a.i(4) as f32 / 1000.0, count: a.i(5), space_wave: a.r(6), time_wave: a.r(7), radius_start_pct: a.r(8), limit_neg: a.b(9) };
            handle(w.add_deformation(d))
        },
        TerrainDeformWave => |w, _, a| {
            let d = Deform::Wave { x: a.r(0), y: a.r(1), dir_x: a.r(2), dir_y: a.r(3), distance: a.r(4), speed: a.r(5), radius: a.r(6), depth: a.r(7), trail_time: a.i(8) as f32 / 1000.0, count: a.i(9) };
            handle(w.add_deformation(d))
        },
        TerrainDeformRandom => |w, _, a| {
            let d = Deform::Random { x: a.r(0), y: a.r(1), radius: a.r(2), min_delta: a.r(3), max_delta: a.r(4), duration: a.i(5) as f32 / 1000.0, update_interval: a.i(6) as f32 / 1000.0 };
            handle(w.add_deformation(d))
        },
        TerrainDeformStop => |w, _, a| { let h = a.h(0); w.deformations.retain(|d| d.0 != h); none() },
        TerrainDeformStopAll => |w, _, _| { w.deformations.clear(); none() },
    );
}

impl World {
    /// Starts a terrain deformation and returns its handle.
    pub fn add_deformation(&mut self, d: Deform) -> u32 {
        let h = self.handles.insert(Obj::Opaque("terraindeformation".to_string()));
        self.deformations.push((h, self.time, d));
        h
    }
}

use super::reg;
