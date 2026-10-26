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
        let mut changed = false;
        if let Some(p) = self.pathing.as_mut() {
            for &(row, c0, c1) in &rows {
                for col in c0..=c1 {
                    let (x0, y0) = (origin.0 + col as f32 * CELL, origin.1 + row as f32 * CELL);
                    for k in 0..16 {
                        let c = p.cell_of(x0 + (k % 4) as f32 * 32.0 + 16.0, y0 + (k / 4) as f32 * 32.0 + 16.0);
                        p.set_flags(c, BLIGHT_FLAG, add);
                    }
                }
            }
        }
        // The terrain's corners take the blight of the cells about them: a corner is
        // blighted while any of its four cells is.
        if let (Some(t), Some(p)) = (self.terrain_heights.as_ref(), self.pathing.as_ref()) {
            let (w, h) = (t.width as i32, t.height as i32);
            let (ox, oy) = (t.offset[0], t.offset[1]);
            let cell_blighted = |col: i32, row: i32| {
                let (px, py) = (origin.0 + col as f32 * CELL + 16.0, origin.1 + row as f32 * CELL + 16.0);
                p.flags_at(p.cell_of(px, py)).is_some_and(|f| f & BLIGHT_FLAG != 0)
            };
            for &(row, c0, c1) in &rows {
                for col in c0 - 1..=c1 + 1 {
                    for drow in 0..=1 {
                        let (i, j) = (((origin.0 + col as f32 * CELL - ox) / CELL).round() as i32, ((origin.1 + (row + drow) as f32 * CELL - oy) / CELL).round() as i32);
                        if i < 0 || j < 0 || i >= w || j >= h {
                            continue;
                        }
                        let (ci, cj) = (col, row + drow);
                        let on = cell_blighted(ci - 1, cj - 1) || cell_blighted(ci, cj - 1) || cell_blighted(ci - 1, cj) || cell_blighted(ci, cj);
                        if let Some(b) = self.blight.get_mut((j * w + i) as usize) {
                            if *b != on {
                                *b = on;
                                changed = true;
                            }
                        }
                    }
                }
            }
        }
        if changed {
            self.blight_serial = self.blight_serial.wrapping_add(1);
        }
    }

    /// Blights or clears the corners and pathing cells inside `bounds` that `inside`
    /// takes (SetBlight, SetBlightRect, SetBlightPoint).
    pub fn set_blight(&mut self, add: bool, bounds: (f32, f32, f32, f32), inside: impl Fn(f32, f32) -> bool) {
        let mut changed = false;
        if let Some(t) = self.terrain_heights.as_ref() {
            let (w, h) = (t.width as i32, t.height as i32);
            let (ox, oy) = (t.offset[0], t.offset[1]);
            let i0 = (((bounds.0 - ox) / 128.0).floor() as i32 - 1).max(0);
            let i1 = (((bounds.2 - ox) / 128.0).ceil() as i32 + 1).min(w - 1);
            let j0 = (((bounds.1 - oy) / 128.0).floor() as i32 - 1).max(0);
            let j1 = (((bounds.3 - oy) / 128.0).ceil() as i32 + 1).min(h - 1);
            for j in j0..=j1 {
                for i in i0..=i1 {
                    let (px, py) = (ox + i as f32 * 128.0, oy + j as f32 * 128.0);
                    if inside(px, py) {
                        let k = (j * w + i) as usize;
                        if let Some(b) = self.blight.get_mut(k) {
                            if *b != add {
                                *b = add;
                                changed = true;
                            }
                        }
                    }
                }
            }
        }
        if let Some(p) = self.pathing.as_mut() {
            let (c0, c1) = (p.cell_of(bounds.0, bounds.1), p.cell_of(bounds.2, bounds.3));
            for j in c0.1 - 1..=c1.1 + 1 {
                for i in c0.0 - 1..=c1.0 + 1 {
                    let (px, py) = p.center((i, j));
                    if inside(px, py) {
                        p.set_flags((i, j), BLIGHT_FLAG, add);
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
