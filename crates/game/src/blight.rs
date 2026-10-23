//! Blight: the terrain corners drawn with the tileset's blight texture and the pathing
//! cells flagged blighted (war3map.wpm 0x20), which Undead buildings require
//! (UnitBalance requirePlace "blighted").  Blight Growth (Abli) spreads it from a
//! structure: DataA more radius every Dur seconds up to Area.

use crate::world::World;

pub const BLIGHT_FLAG: u8 = 0x20;

impl World {
    /// Blights the corners and pathing cells within `radius` of a point.
    pub fn blight_circle(&mut self, x: f32, y: f32, radius: f32) {
        let mut changed = false;
        if let Some(t) = self.terrain_heights.as_ref() {
            let (w, h) = (t.width as i32, t.height as i32);
            let (ox, oy) = (t.offset[0], t.offset[1]);
            let r = (radius / 128.0).ceil() as i32 + 1;
            let (cx, cy) = (((x - ox) / 128.0).round() as i32, ((y - oy) / 128.0).round() as i32);
            for j in (cy - r).max(0)..=(cy + r).min(h - 1) {
                for i in (cx - r).max(0)..=(cx + r).min(w - 1) {
                    let (px, py) = (ox + i as f32 * 128.0, oy + j as f32 * 128.0);
                    if (px - x).hypot(py - y) <= radius {
                        let k = (j * w + i) as usize;
                        if let Some(b) = self.blight.get_mut(k) {
                            if !*b {
                                *b = true;
                                changed = true;
                            }
                        }
                    }
                }
            }
        }
        if let Some(p) = self.pathing.as_mut() {
            let r = (radius / crate::pathing::CELL).ceil() as i32 + 1;
            let c = p.cell_of(x, y);
            for j in c.1 - r..=c.1 + r {
                for i in c.0 - r..=c.0 + r {
                    let (px, py) = p.center((i, j));
                    if (px - x).hypot(py - y) <= radius {
                        p.set_flags((i, j), BLIGHT_FLAG, true);
                    }
                }
            }
        }
        if changed {
            self.blight_serial = self.blight_serial.wrapping_add(1);
        }
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
