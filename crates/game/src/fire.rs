//! Burning structures (CAbilityOnFire): every structure is given its race's On Fire
//! ability when made (FUN_6f3f5f10: Afio for orc, Afiu undead, Afin night elf, Afih any
//! other).  Every 1.5 s plus a two-thousandth of its maximum life, at most 3 s
//! (FUN_6f41baf0), it weighs its life: at 75% or less it burns small, 50% medium, 25%
//! large (FUN_6f00aca0), each the race's effect buff (Xfhs, Xfhm, Xfhl, and the orc,
//! night elf, and undead ones) with its Targetart at its Targetattach points; above 75%
//! the fire goes out (FUN_6f4249a0).  Dead structures and those being built do not burn.

use crate::types::*;
use crate::world::World;

/// The race's letter in its fire buffs' codes.
fn race_letter(race: &str) -> u8 {
    match race {
        "orc" => b'o',
        "undead" => b'u',
        "nightelf" => b'n',
        _ => b'h',
    }
}

impl World {
    /// The fire buff a structure burns with, if any.
    fn fire_buff(&self, u: &Unit) -> Option<crate::objects::Id> {
        let size = match u.extra.fire {
            1 => b's',
            2 => b'm',
            3 => b'l',
            _ => return None,
        };
        let race = self.objects.units.get(u.type_id).map(|o| o.str("race").to_ascii_lowercase()).unwrap_or_default();
        Some([b'X', b'f', race_letter(&race), size])
    }

    /// The looped sound of a structure's fire (the buff's Effectsoundlooped).
    pub fn fire_sound(&self, h: u32) -> Option<String> {
        let u = self.unit(h)?;
        let o = self.objects.buffs.get(self.fire_buff(u)?)?;
        Some(o.str("Effectsoundlooped")).filter(|s| !s.is_empty())
    }

    pub(crate) fn update_fires(&mut self, dt: f32) {
        let units: Vec<u32> = self.unit_order.iter().copied().filter(|&h| self.unit(h).is_some_and(|u| u.is_building)).collect();
        for h in units {
            let Some(u) = self.unit_mut(h) else { continue };
            if u.dead {
                if u.extra.fire != 0 {
                    u.extra.fire = 0;
                    self.set_fire_art(h);
                }
                continue;
            }
            // The check repeats from when the structure is made (its first after a period).
            let period = (1.5 + u.max_life / 2000.0).clamp(1.5, 3.0);
            if !u.extra.fire_started {
                u.extra.fire_started = true;
                u.extra.fire_timer = period;
            }
            u.extra.fire_timer -= dt;
            if u.extra.fire_timer > 0.0 {
                continue;
            }
            u.extra.fire_timer += period;
            let frac = if u.max_life > 0.0 { u.life / u.max_life } else { 1.0 };
            if u.constructing.is_some() {
                continue;
            }
            let level = [(0.25, 3), (0.5, 2), (0.75, 1)].iter().find(|&&(t, _)| frac <= t).map_or(0, |&(_, l)| l);
            if level != u.extra.fire {
                u.extra.fire = level;
                self.set_fire_art(h);
            }
        }
    }

    /// Replaces a structure's fire art with its current fire's.
    fn set_fire_art(&mut self, h: u32) {
        let Some(u) = self.unit_mut(h) else { return };
        for e in std::mem::take(&mut u.extra.fire_art) {
            if let Some(Obj::Effect(f)) = self.handles.get_mut(e) {
                f.destroyed = true;
            }
        }
        let Some(u) = self.unit(h) else { return };
        let Some(o) = self.fire_buff(u).and_then(|b| self.objects.buffs.get(b)) else { return };
        let models = o.list("Targetart");
        let n = o.str("Targetattachcount").trim().parse::<usize>().unwrap_or(1).max(1);
        let parts: Vec<(String, String)> = (0..n)
            .filter_map(|k| {
                let key = if k == 0 { "Targetattach".to_string() } else { format!("Targetattach{k}") };
                let model = models.get(k).or(models.first()).cloned().unwrap_or_default();
                (!model.is_empty()).then(|| (model, o.str(&key)))
            })
            .collect();
        let handles: Vec<u32> = parts.into_iter().map(|(model, attach)| self.handles.insert_unseen(Obj::Effect(Effect { model, x: 0.0, y: 0.0, z: 0.0, target: Some((h, attach)), destroyed: false, facing: 0.0 }))).collect();
        if let Some(u) = self.unit_mut(h) {
            u.extra.fire_art = handles;
        }
    }
}
