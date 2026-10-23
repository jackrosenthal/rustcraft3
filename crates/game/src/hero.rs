//! Heroes: attributes, experience, levels, and skills, using the hero constants in
//! Units\\MiscGame.txt.

use crate::objects::Id;
use crate::types::EventCtx;

use crate::world::World;

impl World {
    fn misc_list_f32(&self, key: &str) -> Vec<f32> {
        self.misc.get_list("Misc", key).iter().filter_map(|v| v.trim().parse().ok()).collect()
    }

    /// Experience a hero needs to reach `level` (NeedHeroXP and NeedHeroXPFormulaA/B/C:
    /// each level needs the previous total times A, plus B times the level, plus C).
    pub fn hero_xp_needed(&self, level: i32) -> i32 {
        if level <= 1 {
            return 0;
        }
        let base = self.misc_list_f32("NeedHeroXP");
        let (a, b, c) = (self.misc_f32("NeedHeroXPFormulaA", 1.0), self.misc_f32("NeedHeroXPFormulaB", 100.0), self.misc_f32("NeedHeroXPFormulaC", 0.0));
        let mut need = 0.0;
        for l in 2..=level {
            need = match base.get((l - 2) as usize) {
                Some(&v) => v,
                None => a * need + b * l as f32 + c,
            };
        }
        need as i32
    }

    /// Experience granted for killing a unit of a level (GrantHeroXP table, then
    /// GrantHeroXPFormulaA/B/C past its end).
    pub fn xp_for_kill(&self, level: i32) -> i32 {
        let table = self.misc_list_f32("GrantHeroXP");
        let (a, b, c) = (self.misc_f32("GrantHeroXPFormulaA", 1.0), self.misc_f32("GrantHeroXPFormulaB", 0.0), self.misc_f32("GrantHeroXPFormulaC", 100.0));
        let mut v = 0.0;
        for l in 1..=level.max(1) {
            v = match table.get((l - 1) as usize) {
                Some(&t) => t,
                None => a * v + b * l as f32 + c,
            };
        }
        v as i32
    }

    /// Recomputes a hero's maximum life and mana from its attributes.
    pub fn apply_hero_stats(&mut self, h: u32, fill: bool) {
        let hp_per = self.misc_f32("StrHitPointBonus", 25.0);
        let mana_per = self.misc_f32("IntManaBonus", 15.0);
        let Some(u) = self.unit(h) else { return };
        let Some(o) = self.objects.units.get(u.type_id) else { return };
        let (base_hp, base_mana) = (o.real("HP"), o.real("manaN"));
        let b = u.item_bonus;
        let (str_, int) = (u.str_ + b.str_, u.int + b.int);
        let u = self.unit_mut(h).unwrap();
        let (old_hp, old_mana) = (u.max_life, u.max_mana);
        u.max_life = base_hp + str_.floor() * hp_per + b.life;
        u.max_mana = base_mana + int.floor() * mana_per + b.mana;
        if fill {
            u.life = u.max_life;
            u.mana = u.max_mana;
        } else {
            u.life += u.max_life - old_hp;
            u.mana = (u.mana + u.max_mana - old_mana).max(0.0);
        }
    }

    pub fn add_hero_xp(&mut self, h: u32, xp: i32) {
        let max_level = self.misc_f32("MaxHeroLevel", 10.0) as i32;
        let Some(u) = self.unit_mut(h) else { return };
        if !u.is_hero || u.dead {
            return;
        }
        u.hero_xp += xp;
        let owner = u.owner;
        self.score_experience(owner, xp);
        loop {
            let (level, xp) = (self.unit(h).unwrap().level, self.unit(h).unwrap().hero_xp);
            if level >= max_level || xp < self.hero_xp_needed(level + 1) {
                break;
            }
            let o = self.unit(h).and_then(|u| self.objects.units.get(u.type_id));
            let (ds, da, di) = o.map_or((0.0, 0.0, 0.0), |o| (o.real("STRplus"), o.real("AGIplus"), o.real("INTplus")));
            let u = self.unit_mut(h).unwrap();
            u.level += 1;
            u.skill_points += 1;
            u.str_ += ds;
            u.agi += da;
            u.int += di;
            self.apply_hero_stats(h, false);
            use crate::event_ids::*;
            self.queue_unit_event(h, EVENT_PLAYER_HERO_LEVEL, EVENT_UNIT_HERO_LEVEL, EventCtx::default());
        }
    }

    /// Splits kill experience among the killer's heroes in range (HeroExpRange).
    pub fn grant_kill_xp(&mut self, victim: u32, killer: u32) {
        let (Some(v), Some(k)) = (self.unit(victim), self.unit(killer)) else { return };
        let (vx, vy, vlevel, vowner, kowner) = (v.x, v.y, v.level.max(1), v.owner, k.owner);
        if crate::natives::players::is_ally(self, kowner, vowner) {
            return;
        }
        let range = self.misc_f32("HeroExpRange", 1200.0);
        let heroes: Vec<u32> = self
            .unit_order
            .iter()
            .copied()
            .filter(|&h| self.unit(h).is_some_and(|u| u.is_hero && !u.dead && !u.xp_suspended && u.owner == kowner && (u.x - vx).hypot(u.y - vy) <= range))
            .collect();
        if heroes.is_empty() {
            return;
        }
        let xp = self.xp_for_kill(vlevel) / heroes.len() as i32;
        for h in heroes {
            self.add_hero_xp(h, xp);
        }
    }

    /// Whether a hero can learn the next level of an ability now.
    pub fn can_learn(&self, h: u32, ability: Id) -> bool {
        let Some(u) = self.unit(h) else { return false };
        let current = u.abilities.iter().find(|(a, _)| *a == ability).map_or(0, |a| a.1);
        let ab = self.objects.abilities.get(ability);
        let max = ab.map_or(3, |a| a.int("levels").max(1));
        let req = ab.map_or(1, |a| a.int("reqLevel").max(1));
        let skip = ab.map_or(2, |a| a.int("levelSkip").max(1));
        u.skill_points > 0 && current < max && u.level >= req + current * skip
    }

    /// Learns or improves a hero ability, spending a skill point.
    pub fn learn_skill(&mut self, h: u32, ability: Id) -> bool {
        if !self.can_learn(h, ability) {
            return false;
        }
        let Some(u) = self.unit(h) else { return false };
        let _ = u;
        // Each ability level needs a hero level: reqLevel, then every levelSkip levels.
        let u = self.unit_mut(h).unwrap();
        u.skill_points -= 1;
        let level = match u.abilities.iter_mut().find(|(a, _)| *a == ability) {
            Some(e) => {
                e.1 += 1;
                e.1
            }
            None => {
                u.abilities.push((ability, 1));
                1
            }
        };
        if self.ability_code(ability) == "Aamk" {
            self.apply_item_bonuses(h);
        }
        use crate::event_ids::*;
        let ctx = EventCtx { spell_ability: crate::objects::id_to_int(ability), state: level, ..Default::default() };
        self.queue_unit_event(h, EVENT_PLAYER_HERO_SKILL, EVENT_UNIT_HERO_SKILL, ctx);
        true
    }

    /// Hero health and mana regeneration and unit regeneration.
    pub fn regenerate(&mut self, dt: f32) {
        let str_regen = self.misc_f32("StrRegenBonus", 0.05);
        let int_regen = self.misc_f32("IntRegenBonus", 0.05);
        let hs: Vec<u32> = self.unit_order.clone();
        for h in hs {
            let Some(u) = self.unit(h) else { continue };
            if u.dead || u.constructing.is_some() {
                continue;
            }
            let Some(o) = self.objects.units.get(u.type_id) else { continue };
            let mut hp = o.real("regenHP");
            let mut mp = o.real("regenMana");
            // regenType: "always", "night", "day", "blight", or "none".
            let rt = o.str("regenType").to_ascii_lowercase();
            let day = (6.0..18.0).contains(&self.time_of_day);
            if rt == "none" || (rt == "night" && day) || (rt == "day" && !day) {
                hp = 0.0;
            }
            // Mana regeneration items and research add a fraction of the normal rate.
            let ub = self.upgrade_bonus(u.owner, u.type_id);
            mp *= 1.0 + u.item_bonus.mana_regen + ub.mana_regen;
            hp += ub.hp_regen;
            if u.is_hero {
                hp += (u.str_ + u.item_bonus.str_).floor() * str_regen;
                mp += (u.int + u.item_bonus.int).floor() * int_regen;
            }
            let u = self.unit_mut(h).unwrap();
            u.life = (u.life + hp * dt).min(u.max_life);
            u.mana = (u.mana + mp * dt).min(u.max_mana);
        }
    }
}


