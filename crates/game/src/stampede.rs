//! Stampede (CAbilityStampede, CMissileStampede).  While the caster channels for Dur, a
//! beast leaves every 1/DataA seconds (FUN_6f50e9b0): it runs in the direction the
//! spell was cast across a square 2 × Area wide centered on the caster, from −Area to
//! +Area along that direction, kept to the map's bounds, at the profile's Missilespeed.
//! The square's width is five lanes of 2 × Area / 5; each beast takes a lane not yet run
//! (the first the middle one), at a random place within it, and once all five have run
//! they are free again (FUN_6f51e610).  Every DataE seconds a beast looks for what the
//! spell may hit within DataB of it; on finding something it deals DataC to all of that
//! within DataD and dies (CMissileStampede::Tick).

use w3_jass::Vm;

use crate::objects::Id;
use crate::sim::Missile;
use crate::types::*;
use crate::world::World;

impl World {
    /// One beast of a Stampede channel.
    pub(crate) fn stampede_beast(&mut self, h: u32, c: &mut Channel) {
        let (a, lvl) = (c.ability, c.level);
        let Some((cx, cy)) = self.unit(h).map(|u| (u.x, u.y)) else { return };
        let area = self.data(a, "Area", lvl);
        // The lanes run (bits 0 to 4), and whether the first beast has left (bit 7).
        let lane = if c.lanes & 0x80 == 0 {
            2
        } else {
            let free: Vec<u8> = (0..5).filter(|k| c.lanes & (1 << k) == 0).collect();
            if free.is_empty() {
                c.lanes &= 0x80;
                self.rng.int(0, 4) as u8
            } else {
                free[self.rng.int(0, free.len() as i32 - 1) as usize]
            }
        };
        c.lanes |= 0x80 | (1 << lane);
        let width = area * 2.0 / 5.0;
        let side = -area + (lane as f32 + self.rng.real(0.0, 1.0)) * width;
        let angle = (c.y - cy).atan2(c.x - cx);
        let (dx, dy) = (angle.cos(), angle.sin());
        let at = |along: f32| (cx + dx * along - dy * side, cy + dy * along + dx * side);
        let clamp = |p: (f32, f32), r: &Rect| (p.0.clamp(r.min_x, r.max_x), p.1.clamp(r.min_y, r.max_y));
        let start = clamp(at(-area), &self.map_bounds);
        let end = clamp(at(area), &self.map_bounds);
        let speed = self.profile_art(a, "Missilespeed").trim().parse().unwrap_or(0.0f32).max(1.0);
        let z = self.ground_z(start.0, start.1);
        let every = self.data(a, "DataE", lvl);
        self.missiles.push(Missile {
            id: 0,
            source: h,
            target: 0,
            model: self.profile_art(a, "Missileart"),
            x: start.0,
            y: start.1,
            z,
            speed,
            arc: 0.0,
            damage: self.data(a, "DataC", lvl),
            attack_type: String::new(),
            start,
            spell: None,
            weapon: None,
            impact: Some(end),
            bounces: 0,
            hit: Vec::new(),
            stampede: Some((a, lvl, every)),
            rock: None,
            reflected: false,
        });
    }

    /// Moves the running beasts, each striking what it meets (see the module notes).
    pub(crate) fn update_stampede(&mut self, vm: &mut Vm, dt: f32) {
        let mut strikes: Vec<(u32, Id, u32, f32, f32, f32)> = Vec::new();
        let mut i = 0;
        while i < self.missiles.len() {
            let Some((a, lvl, wait)) = self.missiles[i].stampede else {
                i += 1;
                continue;
            };
            let every = self.data(a, "DataE", lvl).max(0.01);
            let m = &mut self.missiles[i];
            let end = m.impact.unwrap_or((m.x, m.y));
            let (dx, dy) = (end.0 - m.x, end.1 - m.y);
            let d = dx.hypot(dy);
            let step = m.speed * dt;
            if d <= step {
                let m = self.missiles.swap_remove(i);
                self.art_at(a, "Specialart", m.x, m.y);
                continue;
            }
            m.x += dx / d * step;
            m.y += dy / d * step;
            let wait = wait - dt;
            if wait > 0.0 {
                m.stampede = Some((a, lvl, wait));
                i += 1;
                continue;
            }
            let (source, x, y, damage) = (m.source, m.x, m.y, m.damage);
            m.stampede = Some((a, lvl, wait + every));
            let reach = self.data(a, "DataB", lvl);
            let met = self.units_in_pub(x, y, reach + 64.0).into_iter().any(|t| {
                self.unit(t).is_some_and(|u| (u.x - x).hypot(u.y - y) - u.collision <= reach) && self.cast_target_error(source, a, t).is_none()
            });
            if met {
                self.missiles.swap_remove(i);
                strikes.push((source, a, lvl, x, y, damage));
                continue;
            }
            i += 1;
        }
        for (source, a, lvl, x, y, damage) in strikes {
            self.art_at(a, "Specialart", x, y);
            let radius = self.data(a, "DataD", lvl);
            for t in self.units_in_pub(x, y, radius) {
                if self.cast_target_error(source, a, t).is_none() {
                    self.spell_damage(vm, source, t, damage);
                }
            }
        }
    }
}
