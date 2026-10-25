//! Autocast: abilities a unit casts on its own while their autocast is on (orderon /
//! orderoff, the right-click toggle).  Each moment each idle or fighting unit looks
//! for a target its spell suits: the most hurt living ally for Heal, an ally in combat
//! without the buff for Inner Fire, Bloodlust, and Frost Armor, an enemy attacking
//! without the buff for Slow, Curse, Faerie Fire, and Web (flyers), a corpse for Raise
//! Dead, and an enemy summon for Abolish Magic.  The interrupted order resumes after.

use crate::objects::Id;
use crate::types::*;
use crate::world::World;

/// Ability codes with autocast, by what they look for.
const HEAL: [&str; 2] = ["Ahea", "Anhe"];
const ALLY_BUFF: [&str; 4] = ["Ainf", "Ablo", "AUfu", "AUfa"];
const ENEMY_DEBUFF: [&str; 4] = ["Aslo", "Acrs", "Afae", "Aweb"];

impl World {
    /// Turns a unit's autocast on or off by an ability's orderon / orderoff name.
    pub fn autocast_order(&mut self, h: u32, name: &str) -> bool {
        let Some(u) = self.unit(h) else { return false };
        let hit = u.abilities.iter().find_map(|&(a, _)| {
            let o = self.objects.abilities.get(a)?;
            if o.str("Orderon").eq_ignore_ascii_case(name) {
                Some((a, true))
            } else if o.str("Orderoff").eq_ignore_ascii_case(name) {
                Some((a, false))
            } else {
                None
            }
        });
        let Some((a, on)) = hit else { return false };
        let u = self.unit_mut(h).unwrap();
        u.autocast.retain(|&x| x != a);
        if on {
            u.autocast.push(a);
        }
        true
    }

    fn in_combat(&self, h: u32) -> bool {
        self.unit(h).is_some_and(|u| matches!(u.action, Action::Attack { .. }) || u.swing.is_some() || self.time - u.extra.last_attacked < 3.0)
    }

    fn has_buff_of(&self, h: u32, a: Id) -> bool {
        self.unit(h).is_some_and(|u| u.buffs.iter().any(|b| b.ability == a))
    }

    fn autocast_target(&self, h: u32, a: Id, lvl: u32) -> Option<u32> {
        let u = self.unit(h)?;
        let code = self.ability_code(a);
        let range = self.data(a, "Rng", lvl).max(0.0) + u.collision;
        let near = |t: &Unit| (t.x - u.x).hypot(t.y - u.y) <= range;
        let alive = |t: &Unit| !t.dead && !t.hidden && !t.is_building;
        let ally = |t: &Unit| crate::natives::players::is_ally(self, u.owner, t.owner);
        let organic = |t: &Unit| self.objects.units.get(t.type_id).is_none_or(|o| !o.str("type").to_ascii_lowercase().contains("mechanical"));
        let units = || self.unit_order.iter().filter_map(|&t| self.unit(t));
        if HEAL.contains(&code.as_str()) {
            let amount = self.data(a, "DataA", lvl);
            return units()
                .filter(|t| alive(t) && ally(t) && near(t) && organic(t) && t.max_life - t.life >= amount.min(t.max_life * 0.5))
                .min_by(|x, y| (x.life / x.max_life).total_cmp(&(y.life / y.max_life)))
                .map(|t| t.handle);
        }
        if ALLY_BUFF.contains(&code.as_str()) {
            return units().find(|t| alive(t) && ally(t) && near(t) && self.in_combat(t.handle) && !self.has_buff_of(t.handle, a)).map(|t| t.handle);
        }
        // An enemy fighting itself (measured: a Sorceress slows a footman attacking her but
        // not one on hold position she attacks).
        if ENEMY_DEBUFF.contains(&code.as_str()) {
            let air_only = code == "Aweb";
            return units()
                .find(|t| {
                    alive(t) && !ally(t) && near(t) && !self.has_buff_of(t.handle, a) && !self.is_magic_immune(t.handle) && self.attacking(t.handle) && (!air_only || self.move_kind(t.handle) == crate::pathing::Move::Fly)
                })
                .map(|t| t.handle);
        }
        match code.as_str() {
            // Repair on autocast: the nearest damaged structure or machine in sight.
            "Arep" | "Arst" | "Aren" => units()
                .filter(|t| (t.x - u.x).hypot(t.y - u.y) <= u.acquire_range.max(500.0) && self.can_repair(h, t.handle))
                .min_by(|x, y| (x.x - u.x).hypot(x.y - u.y).total_cmp(&(y.x - u.x).hypot(y.y - u.y)))
                .map(|t| t.handle),
            "Arai" => self.corpses_in(u.x, u.y, self.data(a, "Area", lvl)).first().map(|_| h),
            // Carrion Scarabs: the nearest corpse in range, in a fight.
            "AUcb" => self.in_combat(h).then(|| self.corpses_in(u.x, u.y, self.data(a, "Rng", lvl)).first().copied()).flatten(),
            "Aadm" => units().find(|t| alive(t) && !ally(t) && near(t) && self.is_summoned(t.handle)).map(|t| t.handle),
            // Frenzy: the caster itself, in a fight, without it.
            "Afzy" => (self.in_combat(h) && !self.has_buff_of(h, a)).then_some(h),
            _ => None,
        }
    }

    /// Each moment (measured: a Priest heals at 0.54 seconds after it is made, its cast
    /// point 0.5, and again 0.02 to 0.04 seconds after each cooldown runs out).
    pub fn update_autocast(&mut self, _dt: f32) {
        // Illusions autocast only under [Misc] IllusionsGetAutocast (0 in the game's data).
        let illusions_cast = self.misc_f32("IllusionsGetAutocast", 0.0) != 0.0;
        let casters: Vec<u32> = self.unit_order.iter().copied().filter(|&h| self.unit(h).is_some_and(|u| !u.dead && !u.hidden && !u.autocast.is_empty() && u.casting.is_none() && (illusions_cast || u.extra.illusion.is_none()) && matches!(u.action, Action::Idle | Action::Attack { .. } | Action::AttackMove { .. } | Action::Hold))).collect();
        for h in casters {
            let list = self.unit(h).map(|u| u.autocast.clone()).unwrap_or_default();
            for a in list {
                let lvl = self.ability_level(h, a);
                if lvl == 0 || self.can_cast(h, a).is_err() {
                    continue;
                }
                let Some(target) = self.autocast_target(h, a, lvl) else { continue };
                if matches!(self.ability_code(a).as_str(), "Arep" | "Arst" | "Aren") {
                    if matches!(self.unit(h).map(|u| &u.action), Some(Action::Idle)) {
                        self.unit_mut(h).unwrap().action = Action::Repair { target };
                    }
                    break;
                }
                let target = if target == h && self.targeting(a) == crate::ability::Targeting::None { 0 } else { target };
                let (x, y) = self.widget_pos(target).or_else(|| self.unit(h).map(|u| (u.x, u.y))).unwrap_or_default();
                let u = self.unit_mut(h).unwrap();
                u.extra.resume = Some(u.action.clone());
                u.action = Action::Cast { ability: a, target, x, y };
                break;
            }
        }
    }
}
