//! Rects, locations, regions, and map bounds.

use crate::collections::HashMap;

use super::{Args, boolean, handle, none, real, reg};
use crate::types::*;
use crate::world::{NativeFn, World};

fn rect_mut(w: &mut World, h: u32) -> Option<&mut Rect> {
    match w.handles.get_mut(h) {
        Some(Obj::Rect(r)) => Some(r),
        _ => None,
    }
}

fn region_mut(w: &mut World, h: u32) -> Option<&mut Region> {
    match w.handles.get_mut(h) {
        Some(Obj::Region(r)) => Some(r),
        _ => None,
    }
}

fn rect_cells(r: &Rect) -> impl Iterator<Item = (i32, i32)> {
    let (x0, y0) = Region::cell(r.min_x, r.min_y);
    let (x1, y1) = Region::cell(r.max_x - 0.001, r.max_y - 0.001);
    (y0..=y1).flat_map(move |y| (x0..=x1).map(move |x| (x, y)))
}

/// CAMERA_MARGIN_LEFT/RIGHT/TOP/BOTTOM read UI\MiscData.txt [CameraMargins], in cells of
/// 128 units (GetCameraMargin, game.dll 0x6f1ff460).
fn camera_margin(w: &World, which: u32) -> f32 {
    let key = match which {
        0 => "left",
        1 => "right",
        2 => "top",
        3 => "bottom",
        _ => return 0.0,
    };
    let cells: f32 = w.misc.get_str("CameraMargins", key).and_then(|v| v.trim().parse().ok()).unwrap_or(0.0);
    cells * 128.0
}

pub fn register(t: &mut HashMap<&'static str, NativeFn>) {
    reg!(t,
        Rect => |w, _, a| handle(w.handles.insert(Obj::Rect(Rect { min_x: a.r(0), min_y: a.r(1), max_x: a.r(2), max_y: a.r(3) }))),
        RectFromLoc => |w, _, a| {
            let (x0, y0) = w.location(a.h(0)).unwrap_or_default();
            let (x1, y1) = w.location(a.h(1)).unwrap_or_default();
            handle(w.handles.insert(Obj::Rect(Rect { min_x: x0, min_y: y0, max_x: x1, max_y: y1 })))
        },
        RemoveRect => |w, _, a| { if w.rect(a.h(0)).is_some() { w.handles.remove(a.h(0)); } none() },
        SetRect => |w, _, a| {
            let r = Rect { min_x: a.r(1), min_y: a.r(2), max_x: a.r(3), max_y: a.r(4) };
            if let Some(x) = rect_mut(w, a.h(0)) { *x = r; }
            none()
        },
        SetRectFromLoc => |w, _, a| {
            let (x0, y0) = w.location(a.h(1)).unwrap_or_default();
            let (x1, y1) = w.location(a.h(2)).unwrap_or_default();
            if let Some(x) = rect_mut(w, a.h(0)) { *x = Rect { min_x: x0, min_y: y0, max_x: x1, max_y: y1 }; }
            none()
        },
        MoveRectTo => |w, _, a| {
            let (cx, cy) = (a.r(1), a.r(2));
            if let Some(r) = rect_mut(w, a.h(0)) {
                let (hw, hh) = ((r.max_x - r.min_x) / 2.0, (r.max_y - r.min_y) / 2.0);
                *r = Rect { min_x: cx - hw, min_y: cy - hh, max_x: cx + hw, max_y: cy + hh };
            }
            none()
        },
        MoveRectToLoc => |w, _, a| {
            let (cx, cy) = w.location(a.h(1)).unwrap_or_default();
            if let Some(r) = rect_mut(w, a.h(0)) {
                let (hw, hh) = ((r.max_x - r.min_x) / 2.0, (r.max_y - r.min_y) / 2.0);
                *r = Rect { min_x: cx - hw, min_y: cy - hh, max_x: cx + hw, max_y: cy + hh };
            }
            none()
        },
        GetRectCenterX => |w, _, a| real(w.rect(a.h(0)).map_or(0.0, |r| (r.min_x + r.max_x) / 2.0)),
        GetRectCenterY => |w, _, a| real(w.rect(a.h(0)).map_or(0.0, |r| (r.min_y + r.max_y) / 2.0)),
        GetRectMinX => |w, _, a| real(w.rect(a.h(0)).map_or(0.0, |r| r.min_x)),
        GetRectMinY => |w, _, a| real(w.rect(a.h(0)).map_or(0.0, |r| r.min_y)),
        GetRectMaxX => |w, _, a| real(w.rect(a.h(0)).map_or(0.0, |r| r.max_x)),
        GetRectMaxY => |w, _, a| real(w.rect(a.h(0)).map_or(0.0, |r| r.max_y)),
        Location => |w, _, a| handle(w.handles.insert(Obj::Location(a.r(0), a.r(1)))),
        RemoveLocation => |w, _, a| { if w.location(a.h(0)).is_some() { w.handles.remove(a.h(0)); } none() },
        MoveLocation => |w, _, a| {
            if let Some(Obj::Location(x, y)) = w.handles.get_mut(a.h(0)) { *x = a.r(1); *y = a.r(2); }
            none()
        },
        GetLocationX => |w, _, a| real(w.location(a.h(0)).map_or(0.0, |l| l.0)),
        GetLocationY => |w, _, a| real(w.location(a.h(0)).map_or(0.0, |l| l.1)),
        GetLocationZ => |w, _, a| real(w.location(a.h(0)).map_or(0.0, |(x, y)| w.ground_z(x, y))),
        CreateRegion => |w, _, _| handle(w.handles.insert(Obj::Region(Region::default()))),
        RemoveRegion => |w, _, a| { if w.region(a.h(0)).is_some() { w.handles.remove(a.h(0)); } none() },
        RegionAddRect => |w, _, a| {
            let Some(r) = w.rect(a.h(1)) else { return none() };
            if let Some(g) = region_mut(w, a.h(0)) { g.cells.extend(rect_cells(&r)); }
            none()
        },
        RegionClearRect => |w, _, a| {
            let Some(r) = w.rect(a.h(1)) else { return none() };
            if let Some(g) = region_mut(w, a.h(0)) { for c in rect_cells(&r) { g.cells.remove(&c); } }
            none()
        },
        RegionAddCell => |w, _, a| {
            let c = Region::cell(a.r(1), a.r(2));
            if let Some(g) = region_mut(w, a.h(0)) { g.cells.insert(c); }
            none()
        },
        RegionAddCellAtLoc => |w, _, a| {
            let (x, y) = w.location(a.h(1)).unwrap_or_default();
            if let Some(g) = region_mut(w, a.h(0)) { g.cells.insert(Region::cell(x, y)); }
            none()
        },
        RegionClearCell => |w, _, a| {
            let c = Region::cell(a.r(1), a.r(2));
            if let Some(g) = region_mut(w, a.h(0)) { g.cells.remove(&c); }
            none()
        },
        RegionClearCellAtLoc => |w, _, a| {
            let (x, y) = w.location(a.h(1)).unwrap_or_default();
            if let Some(g) = region_mut(w, a.h(0)) { g.cells.remove(&Region::cell(x, y)); }
            none()
        },
        IsPointInRegion => |w, _, a| boolean(w.region(a.h(0)).is_some_and(|r| r.contains(a.r(1), a.r(2)))),
        IsLocationInRegion => |w, _, a| {
            let (x, y) = w.location(a.h(1)).unwrap_or_default();
            boolean(w.region(a.h(0)).is_some_and(|r| r.contains(x, y)))
        },
        IsUnitInRegion => |w, _, a| {
            let Some((x, y)) = w.unit(a.h(1)).map(|u| (u.x, u.y)) else { return boolean(false) };
            boolean(w.region(a.h(0)).is_some_and(|r| r.contains(x, y)))
        },
        GetWorldBounds => |w, _, _| { let r = w.map_bounds; handle(w.handles.insert(Obj::Rect(r))) },
        SetCameraBounds => |w, _, a| {
            for i in 0..8 { w.camera_bounds[i] = a.r(i); }
            none()
        },
        GetCameraMargin => |w, _, a| real(camera_margin(w, a.i(0) as u32)),
        GetCameraBoundMinX => |w, _, _| real(w.camera_bounds.chunks(2).map(|p| p[0]).fold(f32::INFINITY, f32::min)),
        GetCameraBoundMinY => |w, _, _| real(w.camera_bounds.chunks(2).map(|p| p[1]).fold(f32::INFINITY, f32::min)),
        GetCameraBoundMaxX => |w, _, _| real(w.camera_bounds.chunks(2).map(|p| p[0]).fold(f32::NEG_INFINITY, f32::max)),
        GetCameraBoundMaxY => |w, _, _| real(w.camera_bounds.chunks(2).map(|p| p[1]).fold(f32::NEG_INFINITY, f32::max)),
    );
}
