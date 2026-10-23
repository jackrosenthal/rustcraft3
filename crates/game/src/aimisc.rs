//! The rest of a computer player's commands to its units: suicide attacks
//! (SuicidePlayer, SuicideUnit), Unsummon, ConvertUnits, RemoveInjuries, RemoveSiege, and
//! the target allies share (SetAllianceTarget).

use crate::natives::basic::order_id;
use crate::natives::players::is_ally;
use crate::objects::{Id, id_from_str};
use crate::types::*;
use crate::world::World;

impl World {
    /// SuicidePlayer and SuicidePlayerUnits (FUN_6f9cc850, FUN_6f9d7240): unless
    /// `check` and the assault group is not full, the attack captain goes after the
    /// player's units (and with `buildings` its structures too) until none is left.
    pub(crate) fn suicide_player(&mut self, p: u8, target: u8, check: bool, buildings: bool) -> bool {
        let Some(st) = self.ai.get_mut(&p) else { return false };
        if check && !st.assault_full {
            return false;
        }
        st.attack_player = Some((target, buildings));
        st.attack.target = 0;
        st.attack.goal = None;
        st.attack.retreating = false;
        true
    }

    /// The attack captain against a player: its next target is the player's unit (or
    /// structure, when allowed) nearest the captain.
    pub(crate) fn ai_attack_player_tick(&mut self, p: u8) {
        let Some(st) = self.ai.get(&p) else { return };
        let Some((enemy, buildings)) = st.attack_player else { return };
        let current = st.attack.target;
        if self.unit(current).is_some_and(|u| !u.dead && u.owner == enemy) {
            return;
        }
        let units: Vec<(f32, f32)> = st.attack.units.iter().filter_map(|&h| self.unit(h)).filter(|u| !u.dead).map(|u| (u.x, u.y)).collect();
        if units.is_empty() {
            return;
        }
        let n = units.len() as f32;
        let (cx, cy) = (units.iter().map(|q| q.0).sum::<f32>() / n, units.iter().map(|q| q.1).sum::<f32>() / n);
        let next = self
            .unit_order
            .iter()
            .copied()
            .filter_map(|h| self.unit(h).map(|u| (h, u)))
            .filter(|(_, u)| !u.dead && !u.hidden && u.owner == enemy && (buildings || !u.is_building))
            .map(|(h, u)| ((u.x - cx).hypot(u.y - cy), h))
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|x| x.1);
        if let Some(st) = self.ai.get_mut(&p) {
            match next {
                Some(t) => st.attack.target = t,
                None => st.attack_player = None,
            }
        }
    }

    /// SuicideUnit and SuicideUnitEx (FUN_6f9cc890): up to `qty` of the player's idle
    /// or gathering units of the type leave their captains and go to die against
    /// `target` (a player, or -1 for any enemy).
    pub(crate) fn suicide_units(&mut self, p: u8, qty: i32, id: Id, target: i32) {
        let mut left = qty;
        let units: Vec<u32> = self
            .unit_order
            .iter()
            .copied()
            .filter(|&h| self.unit(h).is_some_and(|u| u.owner == p && !u.dead && u.type_id == id && matches!(u.action, Action::Idle | Action::Harvest { .. } | Action::ReturnResources)))
            .collect();
        for h in units {
            if left < 1 {
                break;
            }
            if let Some(st) = self.ai.get_mut(&p) {
                st.attack.units.retain(|&k| k != h);
                st.defense.units.retain(|&k| k != h);
            }
            self.leave_mine_pub(h);
            if let Some(u) = self.unit_mut(h) {
                u.extra.ai_suicide = Some(target);
            }
            self.suicide_target(h);
            left -= 1;
        }
    }

    /// A suicide unit's next target (FUN_6f9ce5b0, FUN_6f9ce280): one of up to 100 of
    /// the target player's (else any enemy's) structures it can attack, at random, else
    /// one of their units; it attack-moves there.
    pub(crate) fn suicide_target(&mut self, h: u32) {
        let Some(u) = self.unit(h) else { return };
        let (owner, target) = (u.owner, u.extra.ai_suicide.unwrap_or(-1));
        let pick = |w: &World, structures: bool, of_target: bool| -> Vec<u32> {
            w.unit_order
                .iter()
                .copied()
                .filter(|&t| {
                    w.unit(t).is_some_and(|tu| {
                        !tu.dead && !tu.hidden && tu.is_building == structures && if of_target { tu.owner as i32 == target } else { !is_ally(w, owner, tu.owner) && tu.owner != PLAYER_NEUTRAL_PASSIVE }
                    }) && w.can_attack_unit(h, t)
                })
                .take(100)
                .collect()
        };
        let mut found = Vec::new();
        if target >= 0 {
            found = pick(self, true, true);
            if found.is_empty() {
                found = pick(self, false, true);
            }
        }
        if found.is_empty() {
            found = pick(self, true, false);
        }
        if found.is_empty() {
            found = pick(self, false, false);
        }
        if found.is_empty() {
            return;
        }
        let t = found[self.rng.int(0, found.len() as i32 - 1) as usize];
        let Some((x, y)) = self.unit(t).map(|u| (u.x, u.y)) else { return };
        self.order_unit(h, order_id("attack"), x, y, 0);
        self.spell_ai(h, t, h);
    }

    /// Suicide units that have stopped pick their next target (FUN_6f9bd6e0).
    pub(crate) fn ai_suicide_tick(&mut self, p: u8) {
        let idle: Vec<u32> = self.unit_order.iter().copied().filter(|&h| self.unit(h).is_some_and(|u| u.owner == p && !u.dead && u.extra.ai_suicide.is_some() && matches!(u.action, Action::Idle))).collect();
        for h in idle {
            self.suicide_target(h);
        }
    }

    /// Unsummon (FUN_6f9cd7d0): the first of the player's units that can unsummon and
    /// is idle or gathering does so to the structure.
    pub(crate) fn ai_unsummon(&mut self, p: u8, building: u32) {
        let caster = self.unit_order.iter().copied().find(|&h| {
            self.unit(h).is_some_and(|u| u.owner == p && !u.dead && matches!(u.action, Action::Idle | Action::Harvest { .. } | Action::ReturnResources)) && self.ability_with_code(h, "Auns").is_some()
        });
        let Some(h) = caster else { return };
        let Some((a, _)) = self.ability_with_code(h, "Auns") else { return };
        let order = self.objects.abilities.get(a).map(|o| o.str("Order")).unwrap_or_default();
        self.leave_mine_pub(h);
        self.order_unit(h, order_id(&order), 0.0, 0.0, building);
    }

    /// ConvertUnits (FUN_6f9c4bb0): up to `qty` of the player's units of the type that
    /// can morph from it are ordered to.
    pub(crate) fn ai_convert(&mut self, p: u8, qty: i32, id: Id) {
        let mut left = qty;
        let units: Vec<u32> = self.unit_order.iter().copied().filter(|&h| self.unit(h).is_some_and(|u| u.owner == p && !u.dead && u.type_id == id)).collect();
        for h in units {
            if left < 1 {
                break;
            }
            let order = self.unit(h).and_then(|u| {
                u.abilities.iter().find_map(|&(a, lvl)| {
                    let o = self.objects.abilities.get(a)?;
                    (id_from_str(o.level_str("DataA", lvl.max(1) as u32).trim()) == id && !o.str("Order").is_empty() && o.level_str("UnitID", lvl.max(1) as u32).trim().len() == 4).then(|| o.str("Order"))
                })
            });
            let Some(order) = order else { continue };
            if self.unit(h).is_some_and(|u| u.extra.morph.is_none()) {
                self.order_unit(h, order_id(&order), 0.0, 0.0, 0);
            }
            left -= 1;
        }
    }

    /// RemoveInjuries (FUN_6f9d5820): the attack captain's most hurt unit at half
    /// life or less that is not a Hero, illusion, or summoned joins the defense captain.
    pub(crate) fn remove_injuries(&mut self, p: u8) {
        let Some(st) = self.ai.get(&p) else { return };
        let worst = st
            .attack
            .units
            .iter()
            .copied()
            .filter_map(|h| self.unit(h).map(|u| (h, u)))
            .filter(|(h, u)| !u.dead && !u.is_hero && u.extra.illusion.is_none() && !self.is_summoned(*h) && u.max_life > 0.0 && u.life / u.max_life <= 0.5)
            .min_by(|a, b| (a.1.life / a.1.max_life).total_cmp(&(b.1.life / b.1.max_life)))
            .map(|x| x.0);
        if let (Some(h), Some(st)) = (worst, self.ai.get_mut(&p)) {
            st.attack.units.retain(|&k| k != h);
            st.defense.units.push(h);
        }
    }

    /// RemoveSiege (FUN_6f9d5a70): the attack captain's artillery joins the defense
    /// captain.
    pub(crate) fn remove_siege(&mut self, p: u8) {
        let Some(st) = self.ai.get(&p) else { return };
        let siege: Vec<u32> = st
            .attack
            .units
            .iter()
            .copied()
            .filter(|&h| self.unit(h).is_some_and(|u| crate::sim::unit_weapons(self, u).iter().any(|(_, wp)| wp.weapon_type.to_ascii_lowercase().starts_with("artillery"))))
            .collect();
        if let Some(st) = self.ai.get_mut(&p) {
            st.attack.units.retain(|k| !siege.contains(k));
            st.defense.units.extend(siege);
        }
    }

    /// SetAllianceTarget (FUN_6f9cab80): the player's and its allied computers'.
    pub(crate) fn set_alliance_target(&mut self, p: u8, t: u32) {
        let players: Vec<u8> = self.ai.keys().copied().filter(|&q| q == p || t != 0 && is_ally(self, q, p)).collect();
        for q in players {
            if let Some(st) = self.ai.get_mut(&q) {
                st.alliance_target = t;
            }
        }
    }

    /// UnitInvis: invisible to the computer player and not detected by it.
    pub(crate) fn ai_unit_invis(&self, p: u8, h: u32) -> bool {
        self.is_invisible(h) && !self.detected_by(h, p)
    }
}

impl World {
    /// IgnoredUnits (FUN_6f9c7990): the player's units of the type in no captain, at no
    /// guard post, and not sent to die.
    pub(crate) fn ignored_units(&self, p: u8, id: Id) -> i32 {
        let Some(st) = self.ai.get(&p) else { return 0 };
        let guards = self.guards(p);
        self.unit_order
            .iter()
            .filter(|h| !st.attack.units.contains(h) && !st.defense.units.contains(h) && !guards.contains(h))
            .filter_map(|&h| self.unit(h))
            .filter(|u| u.owner == p && !u.dead && u.type_id == id && u.extra.ai_suicide.is_none())
            .count() as i32
    }

    /// MergeUnits (FUN_6f9c8a30): with fewer than `qty` of `make` (counting those
    /// coupling), idle pairs of `a` and `b` are ordered to couple.
    pub(crate) fn merge_units(&mut self, p: u8, qty: i32, a: Id, b: Id, make: Id) -> bool {
        let couple = |w: &World, h: u32| -> Option<String> {
            w.unit(h)?.abilities.iter().find_map(|&(ab, _)| {
                let o = w.objects.abilities.get(ab)?;
                matches!(o.str("code").as_str(), "Acoa" | "Acoh" | "Aco2" | "Aco3").then(|| o.str("Order"))
            })
        };
        let mine = |w: &World, id: Id| -> Vec<u32> { w.unit_order.iter().copied().filter(|&h| w.unit(h).is_some_and(|u| u.owner == p && !u.dead && u.type_id == id)).collect() };
        let coupling = mine(self, a).into_iter().filter(|&h| self.unit(h).is_some_and(|u| matches!(u.action, Action::Cast { .. }))).count() as i32;
        let made = mine(self, make).len() as i32;
        if made + coupling >= qty {
            return true;
        }
        let idle = |w: &World, id: Id| -> Vec<u32> { mine(w, id).into_iter().filter(|&h| w.unit(h).is_some_and(|u| matches!(u.action, Action::Idle | Action::AttackMove { .. } | Action::Move { .. }))).collect() };
        let (left, right) = (idle(self, a), idle(self, b));
        if left.is_empty() || right.is_empty() {
            return false;
        }
        for (&x, &y) in left.iter().zip(right.iter()).take((qty - made - coupling).max(0) as usize) {
            if let Some(order) = couple(self, x) {
                self.order_unit(x, order_id(&order), 0.0, 0.0, y);
            }
        }
        true
    }

    /// ResetCaptainLocs (FUN_6f9c9c10): the attack captain's home 300 beyond the main
    /// hall away from its mine, the defense captain's the town's center.
    pub(crate) fn reset_captain_locs(&mut self, p: u8) {
        let center = self.town_center(p, 0);
        let (hx, hy) = self.town_pos_pub(p, 0);
        let mine = self.mine_near_pub(hx, hy).and_then(|m| self.unit(m)).map(|m| (m.x, m.y));
        let attack = match mine {
            Some((mx, my)) => {
                let (dx, dy) = (hx - mx, hy - my);
                let len = dx.hypot(dy).max(1.0);
                (hx + dx / len * 300.0, hy + dy / len * 300.0)
            }
            None => (hx, hy),
        };
        if let Some(st) = self.ai.get_mut(&p) {
            st.attack.home = attack;
            st.defense.home = center;
        }
    }
}
