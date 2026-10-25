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
            "Arai" => self.corpses_in(u.x, u.y, self.data(a, "Area", lvl)).first().copied(),
            // Carrion Scarabs: the nearest corpse in range, in a fight.
            "AUcb" => self.in_combat(h).then(|| self.corpses_in(u.x, u.y, self.data(a, "Rng", lvl)).first().copied()).flatten(),
            "Aadm" => units().find(|t| alive(t) && !ally(t) && near(t) && self.is_summoned(t.handle)).map(|t| t.handle),
            // Frenzy: the caster itself, in a fight, without it.
            "Afzy" => (self.in_combat(h) && !self.has_buff_of(h, a)).then_some(h),
            _ => None,
        }
    }

    /// A computer's or creep's unit with Cannibalize eats a corpse on its own
    /// (CAbilityCannibalize: its 2 second timer, and its going idle, both to
    /// FUN_6f608370): able to cast, not moving or eating already, and at half its life
    /// or less; on the timer, only at a quarter or less, unarmed, or not attacking.  It
    /// goes for the nearest corpse within 800 whose death has played (measured: a creep
    /// ghoul at 120 of 340 goes to eat corpses 200 to 800 away, but not 850, beginning
    /// between 3 and 4 seconds after it is made, near a footman killed at 0.3).
    pub fn update_cannibalize(&mut self, dt: f32) {
        let units: Vec<u32> = self.unit_order.iter().copied().filter(|&h| self.unit(h).is_some_and(|u| !u.dead && !u.hidden && u.owner >= PLAYER_NEUTRAL_AGGRESSIVE || self.unit(h).is_some_and(|u| !u.dead && !u.hidden && self.players.get(u.owner as usize).is_some_and(|p| p.controller == map_control::COMPUTER)))).collect();
        for h in units {
            let Some((a, lvl)) = self.ability_with_code(h, "Acan") else { continue };
            let Some(u) = self.unit_mut(h) else { continue };
            let idle = matches!(u.action, Action::Idle);
            let became_idle = idle && !u.extra.was_idle;
            u.extra.was_idle = idle;
            let wait = u.extra.cannibal_wait.get_or_insert(2.0);
            *wait -= dt;
            let timed = *wait <= 0.0;
            if timed {
                *wait += 2.0;
            }
            if !timed && !became_idle {
                continue;
            }
            let Some(u) = self.unit(h) else { continue };
            let moving = u.order.id == crate::natives::basic::order_id("move");
            let eating = matches!(u.action, Action::Cast { ability, .. } if ability == a) || u.buffs.iter().any(|b| b.ability == a);
            let frac = u.life / u.max_life.max(1.0);
            if moving || eating || frac > 0.5 || lvl == 0 || self.can_cast(h, a).is_err() {
                continue;
            }
            let fighting = matches!(u.action, Action::Attack { .. }) || u.swing.is_some();
            if timed && !became_idle && !(frac <= 0.25 || crate::sim::unit_weapons(self, u).is_empty() || !fighting) {
                continue;
            }
            let (x, y) = (u.x, u.y);
            let Some(corpse) = self.corpses_in(x, y, 800.0).into_iter().min_by(|&p, &q| {
                let d = |c: u32| self.widget_pos(c).map_or(f32::MAX, |(cx, cy)| (cx - x).hypot(cy - y));
                d(p).total_cmp(&d(q))
            }) else { continue };
            let Some((cx, cy)) = self.widget_pos(corpse) else { continue };
            let u = self.unit_mut(h).unwrap();
            u.action = Action::Cast { ability: a, target: corpse, x: cx, y: cy };
            u.order = Order { id: crate::natives::basic::order_id("cannibalize"), target_x: cx, target_y: cy, target: corpse };
            u.path.clear();
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
