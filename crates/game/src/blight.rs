//! Blight: the terrain corners drawn with the tileset's blight texture and the pathing
//! cells flagged blighted (war3map.wpm 0x20), which Undead buildings require
//! (UnitBalance requirePlace "blighted").  Blight Growth (Abli) spreads it from a
//! structure: DataA more radius every Dur seconds up to Area.

use crate::world::World;

pub const BLIGHT_FLAG: u8 = 0x20;

impl World {
    /// Blights the corners and pathing cells within `radius` of a point, as SetBlight and
    /// Blight Growth do.
    pub fn blight_circle(&mut self, x: f32, y: f32, radius: f32) {
        self.blight_disc(true, x, y, radius);
    }

    /// The original's blight disc (FUN_6f48f8a0, FUN_6f4908c0): the point and radius
    /// rounded to the 128 grid (halves away from zero), then 128-unit cells of the map
    /// blighted or cleared by the radius in cells.  Below 2, the four cells about the
    /// point; at 2, a fixed shape; beyond, each row of the midpoint circle the sight masks
    /// use (FUN_6f24fad0) blights that row and the one below, from a cell left of its span
    /// to its right end.  Measured: SetBlight of 256 at 1024,1024 blights x 640 to 1280
    /// from y 896 to 1152, 768 to 1280 a row above and below, and 896 to 1152 the next;
    /// a melee Necropolis's blight, grown to 960 (1024 rounded), reaches 1152 below it
    /// and its haunted mine's SetBlightLoc of 768 first reaches 544 below it, as here.
    pub fn blight_disc(&mut self, add: bool, x: f32, y: f32, radius: f32) {
        const CELL: f32 = 128.0;
        let (cxw, cyw) = ((x / CELL).round() * CELL, (y / CELL).round() * CELL);
        let r = ((radius / CELL).round() * CELL / CELL) as i32;
        let origin = (self.map_bounds.min_x, self.map_bounds.min_y);
        let (cx, cy) = (((cxw - origin.0) / CELL).floor() as i32, ((cyw - origin.1) / CELL).floor() as i32);
        // Rows of cells, each with its first and last column.
        let mut rows: Vec<(i32, i32, i32)> = Vec::new();
        match r {
            0 | 1 => {
                rows.push((cy - 1, cx - 1, cx));
                rows.push((cy, cx - 1, cx));
            }
            2 => {
                rows.push((cy - 1, cx - 3, cx + 1));
                rows.push((cy, cx - 3, cx + 1));
                rows.push((cy + 1, cx - 2, cx + 1));
                rows.push((cy - 2, cx - 2, cx + 1));
                rows.push((cy + 2, cx - 1, cx));
                rows.push((cy - 3, cx - 1, cx));
            }
            _ => {
                let spans = crate::fog::circle_spans(r);
                // Clearing leaves out the last row (FUN_6f4908c0's loop stops short).
                let last = if add { 2 * r } else { 2 * r - 1 };
                for j in 0..=last {
                    let row = cy - r + j;
                    let hw = spans[j as usize];
                    rows.push((row - 1, cx - hw - 1, cx + hw));
                    rows.push((row, cx - hw - 1, cx + hw));
                }
            }
        }
        for (row, c0, c1) in rows {
            let (x0, y0) = (origin.0 + c0 as f32 * CELL, origin.1 + row as f32 * CELL);
            self.blight_area(add, x0, y0, origin.0 + (c1 + 1) as f32 * CELL, y0 + CELL);
        }
    }

    /// SetBlightRect (FUN_6f48fc70): its edges rounded to the 128 grid, halves away from
    /// zero, and the cells between blighted or cleared.
    pub fn blight_rect(&mut self, add: bool, x0: f32, y0: f32, x1: f32, y1: f32) {
        let snap = |v: f32| (v / 128.0).round() * 128.0;
        self.blight_area(add, snap(x0), snap(y0), snap(x1), snap(y1));
    }

    /// SetBlightPoint (FUN_6f48fa60): the point moved 64 up and right and cut to the 128
    /// grid toward zero, and the 256 square about it blighted or cleared.
    pub fn blight_point(&mut self, add: bool, x: f32, y: f32) {
        let snap = |v: f32| ((v + 64.0) / 128.0).trunc() * 128.0;
        let (x, y) = (snap(x), snap(y));
        self.blight_area(add, x - 128.0, y - 128.0, x + 128.0, y + 128.0);
    }

    /// Blights or clears the pathing cells whose middles lie in [x0, x1) by [y0, y1)
    /// (FUN_6f04d6d0 with 0x20), and sets the terrain corners about them by the 128
    /// cells around each: a corner is blighted while any of its four cells is.
    fn blight_area(&mut self, add: bool, x0: f32, y0: f32, x1: f32, y1: f32) {
        if x1 <= x0 || y1 <= y0 {
            return;
        }
        if let Some(p) = self.pathing.as_mut() {
            let (c0, c1) = (p.cell_of(x0 + 16.0, y0 + 16.0), p.cell_of(x1 - 16.0, y1 - 16.0));
            for j in c0.1..=c1.1 {
                for i in c0.0..=c1.0 {
                    p.set_flags((i, j), BLIGHT_FLAG, add);
                }
            }
        }
        let mut changed = false;
        if let (Some(t), Some(p)) = (self.terrain_heights.as_ref(), self.pathing.as_ref()) {
            let (w, h) = (t.width as i32, t.height as i32);
            let (ox, oy) = (t.offset[0], t.offset[1]);
            let blighted = |cx: f32, cy: f32| p.flags_at(p.cell_of(cx, cy)).is_some_and(|f| f & BLIGHT_FLAG != 0);
            let i0 = (((x0 - ox) / 128.0).floor() as i32 - 1).max(0);
            let i1 = (((x1 - ox) / 128.0).ceil() as i32 + 1).min(w - 1);
            let j0 = (((y0 - oy) / 128.0).floor() as i32 - 1).max(0);
            let j1 = (((y1 - oy) / 128.0).ceil() as i32 + 1).min(h - 1);
            for j in j0..=j1 {
                for i in i0..=i1 {
                    let (cx, cy) = (ox + i as f32 * 128.0, oy + j as f32 * 128.0);
                    let on = blighted(cx - 64.0, cy - 64.0) || blighted(cx + 64.0, cy - 64.0) || blighted(cx - 64.0, cy + 64.0) || blighted(cx + 64.0, cy + 64.0);
                    if let Some(b) = self.blight.get_mut((j * w + i) as usize) {
                        if *b != on {
                            *b = on;
                            changed = true;
                        }
                    }
                }
            }
        }
        if changed {
            self.blight_serial = self.blight_serial.wrapping_add(1);
        }
    }

    /// Whether the pathing cell at a point is blighted (IsPointBlighted).
    pub fn point_blighted(&self, x: f32, y: f32) -> bool {
        self.pathing.as_ref().is_some_and(|p| p.flags_at(p.cell_of(x, y)).is_some_and(|f| f & BLIGHT_FLAG != 0))
    }

    /// Whether every cell a structure would cover is blighted.
    pub fn footprint_blighted(&mut self, id: crate::objects::Id, x: f32, y: f32) -> bool {
        let tex = self.objects.units.get(id).map(|o| o.str("pathTex")).unwrap_or_default();
        let cells = self.pathing_cells(&tex, x, y, 0.0);
        let Some(p) = &self.pathing else { return true };
        !cells.is_empty() && cells.iter().all(|&c| p.flags_at(c).is_some_and(|f| f & BLIGHT_FLAG != 0))
    }

    /// Blight Growth: each structure with Abli spreads blight outward.
    pub fn update_blight(&mut self, dt: f32) {
        let mut grow = Vec::new();
        for &h in &self.unit_order {
            let Some(u) = self.unit(h).filter(|u| !u.dead && !u.hidden) else { continue };
            let Some((a, lvl)) = self.ability_with_code(h, "Abli") else { continue };
            let Some(o) = self.objects.abilities.get(a) else { continue };
            if o.level_real("DataB", lvl) == 0.0 {
                continue;
            }
            let area = o.level_real("Area", lvl);
            if u.extra.blight_radius >= area {
                continue;
            }
            grow.push((h, u.x, u.y, o.level_real("DataA", lvl).max(1.0), o.level_real("Dur", lvl).max(0.01), area));
        }
        for (h, x, y, step, interval, area) in grow {
            let mut radius = None;
            if let Some(u) = self.unit_mut(h) {
                u.extra.blight_timer -= dt;
                while u.extra.blight_timer <= 0.0 && u.extra.blight_radius < area {
                    u.extra.blight_timer += interval;
                    u.extra.blight_radius = (u.extra.blight_radius + step).min(area);
                    radius = Some(u.extra.blight_radius);
                }
            }
            if let Some(r) = radius {
                self.blight_circle(x, y, r);
            }
        }
    }
}
