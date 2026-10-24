//! Repair (Ahrp, Arep), Restoration (Arst), and Renew (Aren): a worker restores a damaged
//! structure or mechanical unit at a rate that would take its build time times the
//! Repair Time Ratio (DataB) from nothing to full, paying its cost times the Repair Cost
//! Ratio (DataA) in proportion to the life restored.  Helping build a peasant's
//! structure (powerbuild) speeds it by the Powerbuild Rate (DataD) per extra builder.

use crate::types::*;
use crate::world::World;

const REPAIRS: [&str; 3] = ["Arep", "Arst", "Aren"];

impl World {
    fn repair_ability(&self, h: u32) -> Option<(crate::objects::Id, u32)> {
        REPAIRS.iter().find_map(|c| self.ability_with_code(h, c))
    }

    /// Whether a worker can repair a target: a friendly structure or mechanical unit
    /// (Renew also takes ancients) that is damaged.
    pub fn can_repair(&self, h: u32, target: u32) -> bool {
        let Some((a, _)) = self.repair_ability(h) else { return false };
        let (Some(u), Some(t)) = (self.unit(h), self.unit(target)) else { return false };
        if t.dead || h == target || !crate::natives::players::is_ally(self, u.owner, t.owner) || t.life >= t.max_life || t.constructing.is_some() || t.buffs.iter().any(|b| b.no_repair) {
            return false;
        }
        let o = self.objects.units.get(t.type_id);
        let kinds = o.map(|o| o.str("type").to_ascii_lowercase()).unwrap_or_default();
        let ancient = kinds.contains("ancient");
        let code = self.ability_code(a);
        if ancient && code != "Aren" {
            return false;
        }
        t.is_building || kinds.contains("mechanical")
    }

    /// The repair rate (life a second) and cost (gold and lumber per point of life), from
    /// the target's repair gold, lumber, and time (UnitBalance goldRep, lumberRep, and
    /// reptm; its build costs and time where it has none) and the ability's ratios.
    fn repair_terms(&self, h: u32, target: u32) -> Option<(f32, f32, f32)> {
        let (a, l) = self.repair_ability(h)?;
        let t = self.unit(target)?;
        let o = self.objects.units.get(t.type_id)?;
        let field = |rep: &str, build: &str| if o.str(rep).trim().is_empty() { o.real(build) } else { o.real(rep) };
        let (cost_ratio, time_ratio) = (self.data(a, "DataA", l), self.data(a, "DataB", l));
        let time = (field("reptm", "bldtm") * time_ratio).max(0.1);
        let per_life = 1.0 / t.max_life.max(1.0);
        Some((t.max_life / time, field("goldRep", "goldcost") * cost_ratio * per_life, field("lumberRep", "lumbercost") * cost_ratio * per_life))
    }

    pub(crate) fn repair_step(&mut self, h: u32, target: u32, dt: f32) {
        if !self.can_repair(h, target) {
            let u = self.unit_mut(h).unwrap();
            u.action = Action::Idle;
            crate::sim::set_anim(u, "stand");
            return;
        }
        let range = self.repair_ability(h).map_or(50.0, |(a, l)| self.data(a, "Rng", l));
        if !self.approach(h, target, range, dt) {
            return;
        }
        let Some((rate, gold_per, lumber_per)) = self.repair_terms(h, target) else { return };
        let owner = self.unit(h).unwrap().owner;
        let missing = self.unit(target).map_or(0.0, |t| t.max_life - t.life);
        let gain = (rate * dt).min(missing);
        // Paid as it goes; the fractions carry over.
        let (gold, lumber) = (gain * gold_per, gain * lumber_per);
        let u = self.unit_mut(h).unwrap();
        u.extra.repair_debt.0 += gold;
        u.extra.repair_debt.1 += lumber;
        // Paid rounded up, ahead of the work (measured: 90 life of a farm costs 6 gold
        // and 2 lumber in the original, where its share of the price is 5.04 and 1.26).
        let (pay_gold, pay_lumber) = (u.extra.repair_debt.0.ceil().max(0.0) as i32, u.extra.repair_debt.1.ceil().max(0.0) as i32);
        let p = &self.players[owner as usize];
        if p.get_state(player_state::RESOURCE_GOLD) < pay_gold || p.get_state(player_state::RESOURCE_LUMBER) < pay_lumber {
            let u = self.unit_mut(h).unwrap();
            u.action = Action::Idle;
            crate::sim::set_anim(u, "stand");
            return;
        }
        let p = &mut self.players[owner as usize];
        *p.state.entry(player_state::RESOURCE_GOLD).or_insert(0) -= pay_gold;
        *p.state.entry(player_state::RESOURCE_LUMBER).or_insert(0) -= pay_lumber;
        let u = self.unit_mut(h).unwrap();
        u.extra.repair_debt.0 -= pay_gold as f32;
        u.extra.repair_debt.1 -= pay_lumber as f32;
        let tp = self.widget_pos(target).unwrap_or_default();
        let u = self.unit_mut(h).unwrap();
        let want = crate::sim::angle_to((u.x, u.y), tp);
        crate::sim::turn(u, want, dt);
        crate::sim::set_anim(u, "stand work");
        if let Some(t) = self.unit_mut(target) {
            t.life = (t.life + gain).min(t.max_life);
        }
    }

    /// How fast a peasant's structure goes up with its builders: one, plus the
    /// Powerbuild Rate for each other.
    pub fn build_speed(&self, structure: u32) -> f32 {
        let builders: Vec<u32> = self.unit_order.iter().copied().filter(|&o| self.unit(o).is_some_and(|w| !w.dead && matches!(w.action, Action::Construct { target } if target == structure))).collect();
        let Some(&first) = builders.first() else { return 0.0 };
        let rate = self.repair_ability(first).map_or(0.0, |(a, l)| self.data(a, "DataD", l));
        1.0 + rate * (builders.len() as f32 - 1.0).max(0.0)
    }

    /// Extra builders on a peasant's structure each cost the Powerbuild Cost (DataC) of
    /// its price over its build time.
    pub(crate) fn charge_powerbuild(&mut self, structure: u32, dt: f32) {
        let builders: Vec<u32> = self.unit_order.iter().copied().filter(|&o| self.unit(o).is_some_and(|w| !w.dead && matches!(w.action, Action::Construct { target } if target == structure))).collect();
        let Some(&first) = builders.first() else { return };
        let Some((a, l)) = self.repair_ability(first) else { return };
        let share = self.data(a, "DataC", l) * (builders.len() as f32 - 1.0);
        let Some(t) = self.unit(structure) else { return };
        let Some(o) = self.objects.units.get(t.type_id) else { return };
        let time = o.real("bldtm").max(0.1);
        let (gold, lumber) = (o.real("goldcost") * share * dt / time, o.real("lumbercost") * share * dt / time);
        let owner = t.owner;
        let t = self.unit_mut(structure).unwrap();
        t.extra.repair_debt.0 += gold;
        t.extra.repair_debt.1 += lumber;
        let (g, lu) = (t.extra.repair_debt.0.floor() as i32, t.extra.repair_debt.1.floor() as i32);
        t.extra.repair_debt.0 -= g as f32;
        t.extra.repair_debt.1 -= lu as f32;
        let p = &mut self.players[owner as usize];
        *p.state.entry(player_state::RESOURCE_GOLD).or_insert(0) -= g;
        *p.state.entry(player_state::RESOURCE_LUMBER).or_insert(0) -= lu;
    }
}
