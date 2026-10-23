//! Illusions (the unit flag IsUnitIllusion reads): copies of a unit that deal and take
//! damage at their ability's factors, live for its duration, and vanish when they die,
//! their buff's Specialart left where they fell.  Mirror Image (AOmi) makes DataA
//! images of the caster, dealing DataB and taking DataC times damage, the caster and
//! its images trading places at random and the caster's buffs dispelled; the Wand of
//! Illusion (AIil) makes one image of its target, dealing DataA and taking DataB times
//! damage.

use crate::objects::{Id, id_from_str};
use crate::types::*;
use crate::world::World;

impl World {
    /// An illusion of `source` for `owner` at a point.
    pub(crate) fn make_illusion(&mut self, source: u32, owner: u8, (x, y): (f32, f32), a: Id, lvl: u32, dealt: f32, taken: f32) -> Option<u32> {
        let s = self.unit(source)?.clone();
        let dur = self.data(a, "Dur", lvl).max(self.data(a, "HeroDur", lvl));
        let h = crate::natives::units::create_unit(self, owner, s.type_id, x, y, s.facing);
        let buff = self.objects.abilities.get(a).map(|o| id_from_str(o.level_str("BuffID", lvl).split(',').next().unwrap_or("").trim())).unwrap_or([0; 4]);
        if s.is_hero {
            if let Some(u) = self.unit_mut(h) {
                u.level = s.level;
                u.hero_xp = s.hero_xp;
                u.abilities = s.abilities.clone();
            }
            self.apply_hero_stats(h, true);
        }
        let u = self.unit_mut(h)?;
        u.life = (u.max_life * s.life / s.max_life.max(1.0)).max(1.0);
        u.mana = u.max_mana * if s.max_mana > 0.0 { s.mana / s.max_mana } else { 0.0 };
        u.extra.illusion = Some((dealt, taken, buff));
        u.extra.use_food = false;
        u.timed_life = Some(dur);
        u.autocast.clear();
        self.apply_buff(h, Buff { id: buff, ability: a, level: lvl, caster: source, remaining: dur, ..Default::default() });
        self.update_food();
        Some(h)
    }

    /// Mirror Image: the caster and its DataA images around it, in an order chosen at
    /// random; the caster loses its buffs.
    pub(crate) fn mirror_image(&mut self, h: u32, a: Id, lvl: u32) {
        let Some(u) = self.unit(h) else { return };
        let (owner, x, y) = (u.owner, u.x, u.y);
        let n = self.data(a, "DataA", lvl).max(0.0) as usize;
        let (dealt, taken) = (self.data(a, "DataB", lvl), self.data(a, "DataC", lvl));
        let r = self.data(a, "Rng", lvl).max(64.0);
        let mut spots: Vec<(f32, f32)> = (0..=n).map(|k| {
            let ang = k as f32 / (n + 1) as f32 * std::f32::consts::TAU;
            (x + r * ang.cos(), y + r * ang.sin())
        }).collect();
        // The caster takes one spot at random.
        let pick = self.rng.int(0, n as i32).max(0) as usize;
        let mine = spots.remove(pick.min(spots.len() - 1));
        if let Some(u) = self.unit_mut(h) {
            u.buffs.retain(|b| b.invulnerable && b.ability == a);
            u.x = mine.0;
            u.y = mine.1;
            u.action = Action::Idle;
        }
        for spot in spots {
            self.make_illusion(h, owner, spot, a, lvl, dealt, taken);
        }
    }

    /// The damage an illusion deals or takes.
    pub fn illusion_damage(&self, source: u32, target: u32, dmg: f32) -> f32 {
        let dealt = self.unit(source).and_then(|u| u.extra.illusion).map_or(1.0, |i| i.0);
        let taken = self.unit(target).and_then(|u| u.extra.illusion).map_or(1.0, |i| i.1);
        dmg * dealt * taken
    }
}
