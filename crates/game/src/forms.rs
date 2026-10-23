//! Rooting and uprooting (Aroo): a rooted ancient is a structure using its rooted
//! weapons (DataA, weapsOn bits); uprooting plays its morph for Dur seconds and leaves a
//! unit that walks at its UnitBalance speed with its uprooted weapons (DataB) and
//! DataD's defense type, in its alternate animations; rooting walks to a spot, plays the
//! alternate morph, and makes it a structure there again.

use crate::types::*;
use crate::world::World;

/// Defense types by index (DEFENSE_TYPE_*), as UnitBalance names them.
const DEFENSE: [&str; 8] = ["small", "medium", "large", "fort", "normal", "hero", "divine", "none"];

impl World {
    fn root_ability(&self, h: u32) -> Option<(crate::objects::Id, u32)> {
        self.ability_with_code(h, "Aroo")
    }

    fn root_data(&self, h: u32, field: &str) -> f32 {
        self.root_ability(h).and_then(|(a, l)| self.objects.abilities.get(a).map(|o| o.level_real(field, l))).unwrap_or(0.0)
    }

    /// Starts uprooting a rooted ancient (order "unroot").
    pub fn uproot(&mut self, h: u32) -> bool {
        if self.root_ability(h).is_none() || self.unit(h).is_none_or(|u| u.extra.uprooted || u.extra.rooting.is_some() || u.constructing.is_some()) {
            return false;
        }
        let dur = self.root_data(h, "Dur");
        let u = self.unit_mut(h).unwrap();
        u.extra.rooting = Some((dur, None));
        u.queue.clear();
        crate::sim::set_anim(u, "morph");
        true
    }

    /// Sends an uprooted ancient to root at a spot (order "root").
    pub fn root_at(&mut self, h: u32, x: f32, y: f32) -> bool {
        if self.root_ability(h).is_none() || self.unit(h).is_none_or(|u| !u.extra.uprooted || u.extra.rooting.is_some()) {
            return false;
        }
        let (x, y) = self.snap_placement(x, y);
        let u = self.unit_mut(h).unwrap();
        u.extra.root_target = Some((x, y));
        u.action = Action::Move { x, y };
        u.path.clear();
        true
    }

    pub fn update_forms(&mut self, dt: f32) {
        let units: Vec<u32> = self.unit_order.iter().copied().filter(|&h| self.unit(h).is_some_and(|u| !u.dead && (u.extra.rooting.is_some() || u.extra.root_target.is_some()))).collect();
        for h in units {
            // Arrived where it roots: begin the morph if the spot is free.
            if let Some((x, y)) = self.unit(h).and_then(|u| u.extra.rooting.is_none().then_some(u.extra.root_target).flatten()) {
                let arrived = self.unit(h).is_some_and(|u| (u.x - x).hypot(u.y - y) <= 16.0 || matches!(u.action, Action::Idle));
                if !arrived {
                    continue;
                }
                let id = self.unit(h).unwrap().type_id;
                let near = self.unit(h).is_some_and(|u| (u.x - x).hypot(u.y - y) <= 64.0);
                let u = self.unit_mut(h).unwrap();
                u.extra.root_target = None;
                if !near {
                    continue;
                }
                // The ancient stands aside from its own footprint check.
                if !self.can_place(id, x, y) {
                    continue;
                }
                let dur = self.root_data(h, "Dur");
                let u = self.unit_mut(h).unwrap();
                u.x = x;
                u.y = y;
                u.action = Action::Hold;
                u.path.clear();
                u.extra.rooting = Some((dur, Some((x, y))));
                crate::sim::set_anim(u, "morph alternate");
                continue;
            }
            let Some((left, spot)) = self.unit(h).and_then(|u| u.extra.rooting) else { continue };
            let left = left - dt;
            if left > 0.0 {
                self.unit_mut(h).unwrap().extra.rooting = Some((left, spot));
                continue;
            }
            match spot {
                None => self.finish_uproot(h),
                Some((x, y)) => self.finish_root(h, x, y),
            }
        }
    }

    fn finish_uproot(&mut self, h: u32) {
        let weapons = self.root_data(h, "DataB") as i32;
        let defense = DEFENSE.get(self.root_data(h, "DataD") as usize).copied().unwrap_or("large");
        let speed = self.unit(h).and_then(|u| self.objects.units.get(u.type_id)).map_or(0.0, |o| o.real("spd"));
        let cells = self.unit_mut(h).map(|u| std::mem::take(&mut u.pathing_cells)).unwrap_or_default();
        if let Some(p) = self.pathing.as_mut() {
            p.stamp(&cells, -1);
        }
        let u = self.unit_mut(h).unwrap();
        u.extra.rooting = None;
        u.extra.uprooted = true;
        u.extra.weapons = Some(weapons);
        u.extra.defense = Some(defense.to_string());
        u.is_building = false;
        u.move_speed = speed;
        u.action = Action::Idle;
        crate::sim::set_anim(u, "stand");
    }

    fn finish_root(&mut self, h: u32, x: f32, y: f32) {
        let weapons = self.root_data(h, "DataA") as i32;
        let tex = self.unit(h).and_then(|u| self.objects.units.get(u.type_id)).map(|o| o.str("pathTex")).unwrap_or_default();
        let cells = self.pathing_cells(&tex, x, y, 0.0);
        if let Some(p) = self.pathing.as_mut() {
            p.stamp(&cells, 1);
        }
        let u = self.unit_mut(h).unwrap();
        u.pathing_cells = cells;
        u.extra.rooting = None;
        u.extra.uprooted = false;
        u.extra.weapons = Some(weapons);
        u.extra.defense = None;
        u.is_building = true;
        u.move_speed = 0.0;
        u.action = Action::Idle;
        crate::sim::set_anim(u, "stand");
    }
}
