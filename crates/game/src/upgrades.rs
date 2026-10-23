//! Research effects: each UpgradeData upgrade has up to four effects (codes from
//! UpgradeEffectMetaData) worth `base + mod * (level - 1)` at a level, applied to the
//! unit types whose UnitBalance `upgrades` list names it.

use std::collections::HashMap;

use crate::objects::{Id, id_from_str};
use crate::world::World;

/// What a player's research adds to one unit type.
#[derive(Clone, Copy, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct UpgradeBonus {
    /// ratd: attack dice.
    pub dice: i32,
    /// ratx: attack damage.
    pub damage: f32,
    /// rarm: armor (the unit's defUp per level when the upgrade gives no value).
    pub armor: f32,
    /// rhpx and rhpo: hit points, flat and as a fraction of the base.
    pub hp: f32,
    pub hp_pct: f32,
    /// rmnx and rman: mana, flat and as a fraction.
    pub mana: f32,
    pub mana_pct: f32,
    /// rhpr: hit point regeneration per second.
    pub hp_regen: f32,
    /// rmnr: fraction added to mana regeneration.
    pub mana_regen: f32,
    /// rmvx and rmov: movement speed, flat and as a fraction.
    pub move_speed: f32,
    pub move_pct: f32,
    /// ratr: attack range.
    pub range: f32,
    /// rats: fraction added to attack rate.
    pub attack_speed: f32,
    /// rsig: sight range.
    pub sight: f32,
    /// rspi: damage Spiked Barricades deal to melee attackers.
    pub spikes: f32,
    /// rasd: attack spill distance.
    pub spill_dist: f32,
    /// ratc: more attack targets (bounces).
    pub targets: i32,
    /// renw: the weapons enabled (weapsOn bits).
    pub weapons: Option<i32>,
    /// rlum: more lumber a trip.
    pub lumber: i32,
    /// rauv: ultravision.
    pub ultravision: bool,
}

/// One upgrade's value at a level, or `None` when the effect gives no value ("-").
fn value(base: &str, md: &str, level: i32) -> Option<f32> {
    let b: f32 = base.trim().parse().ok()?;
    let m: f32 = md.trim().parse().unwrap_or(0.0);
    Some(b + m * (level - 1) as f32)
}

impl World {
    /// The research bonuses of `owner` for unit type `id`, cached until research changes.
    pub fn upgrade_bonus(&self, owner: u8, id: Id) -> UpgradeBonus {
        if let Some(b) = self.upgrade_cache.borrow().get(&(owner, id)) {
            return *b;
        }
        let b = self.compute_upgrade_bonus(owner, id);
        self.upgrade_cache.borrow_mut().insert((owner, id), b);
        b
    }

    fn compute_upgrade_bonus(&self, owner: u8, id: Id) -> UpgradeBonus {
        let mut b = UpgradeBonus::default();
        let Some(o) = self.objects.units.get(id) else { return b };
        let Some(p) = self.players.get(owner as usize) else { return b };
        for up in o.list("upgrades") {
            let up = id_from_str(up.trim());
            let level = p.tech_researched.get(&up).copied().unwrap_or(0);
            if level <= 0 {
                continue;
            }
            let Some(u) = self.objects.upgrades.get(up) else { continue };
            for k in 1..=4 {
                let effect = u.str(&format!("effect{k}"));
                let v = value(&u.str(&format!("base{k}")), &u.str(&format!("mod{k}")), level);
                match effect.as_str() {
                    "ratd" => b.dice += v.unwrap_or(0.0) as i32,
                    "ratx" => b.damage += v.unwrap_or(0.0),
                    "rarm" => b.armor += v.unwrap_or(o.real("defUp") * level as f32),
                    "rhpx" => b.hp += v.unwrap_or(0.0),
                    "rhpo" => b.hp_pct += v.unwrap_or(0.0),
                    "rmnx" => b.mana += v.unwrap_or(0.0),
                    "rman" => b.mana_pct += v.unwrap_or(0.0),
                    "rhpr" => b.hp_regen += v.unwrap_or(0.0),
                    "rmnr" => b.mana_regen += v.unwrap_or(0.0),
                    "rmvx" => b.move_speed += v.unwrap_or(0.0),
                    "rmov" => b.move_pct += v.unwrap_or(0.0),
                    "ratr" => b.range += v.unwrap_or(0.0),
                    "rats" => b.attack_speed += v.unwrap_or(0.0),
                    "rsig" => b.sight += v.unwrap_or(0.0),
                    "rspi" => b.spikes += v.unwrap_or(0.0),
                    "rasd" => b.spill_dist += v.unwrap_or(0.0),
                    "ratc" => b.targets += v.unwrap_or(0.0) as i32,
                    "renw" => b.weapons = v.map(|m| m as i32),
                    "rlum" => b.lumber += v.unwrap_or(0.0) as i32,
                    "rauv" => b.ultravision = true,
                    _ => {}
                }
            }
        }
        b
    }

    /// rlev: research that raises the level of a unit's ability with code `code`.
    pub fn upgrade_spell_levels(&self, owner: u8, id: Id, code: &str) -> i32 {
        let Some(o) = self.objects.units.get(id) else { return 0 };
        let Some(p) = self.players.get(owner as usize) else { return 0 };
        let mut n = 0;
        for up in o.list("upgrades") {
            let up = id_from_str(up.trim());
            let level = p.tech_researched.get(&up).copied().unwrap_or(0);
            let Some(u) = self.objects.upgrades.get(up).filter(|_| level > 0) else { continue };
            for k in 1..=4 {
                if u.str(&format!("effect{k}")) == "rlev" && u.str(&format!("code{k}")).trim().eq_ignore_ascii_case(code) {
                    n += value(&u.str(&format!("base{k}")), &u.str(&format!("mod{k}")), level).unwrap_or(1.0) as i32;
                }
            }
        }
        n
    }

    /// After research finishes: forget cached bonuses and bring every unit of the player
    /// to its new maximum life and mana.
    pub fn research_changed(&mut self, owner: u8) {
        self.upgrade_cache.borrow_mut().retain(|(p, _), _| *p != owner);
        let units: Vec<u32> = self.unit_order.iter().copied().filter(|&h| self.unit(h).is_some_and(|u| u.owner == owner)).collect();
        for h in units {
            self.apply_upgrades(h);
        }
    }

    /// Sets a unit's maximum life and mana from its type, research, and items, keeping
    /// the damage it has taken.
    pub fn apply_upgrades(&mut self, h: u32) {
        let Some(u) = self.unit(h) else { return };
        if u.is_hero {
            self.apply_hero_stats(h, false);
            return;
        }
        let b = self.upgrade_bonus(u.owner, u.type_id);
        let Some(o) = self.objects.units.get(u.type_id) else { return };
        let (hp, mp) = (o.real("HP"), o.real("manaN"));
        let u = self.unit_mut(h).unwrap();
        let max_life = hp * (1.0 + b.hp_pct) + b.hp + u.item_bonus.life;
        let max_mana = if mp > 0.0 { mp * (1.0 + b.mana_pct) + b.mana + u.item_bonus.mana } else { 0.0 };
        let (dl, dm) = (max_life - u.max_life, max_mana - u.max_mana);
        u.max_life = max_life;
        u.max_mana = max_mana;
        if !u.dead {
            u.life = (u.life + dl.max(0.0)).min(u.max_life);
            u.mana = (u.mana + dm.max(0.0)).clamp(0.0, u.max_mana);
        }
    }
}

pub type UpgradeCache = std::cell::RefCell<HashMap<(u8, Id), UpgradeBonus>>;
