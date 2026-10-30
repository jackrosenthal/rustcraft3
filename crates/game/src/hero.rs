//! Heroes: attributes, experience, levels, and skills, using the hero constants in
//! Units\\MiscGame.txt.

use crate::objects::{Id, id_from_str};
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

    /// A table of experience by level: its listed values, then each further level the
    /// one before times A, plus B times the level, plus C (FUN_6f048140).
    fn xp_table(&self, key: &str, defaults: (f32, f32, f32), level: i32) -> f32 {
        let table = self.misc_list_f32(key);
        let (a, b, c) = (self.misc_f32(&format!("{key}FormulaA"), defaults.0), self.misc_f32(&format!("{key}FormulaB"), defaults.1), self.misc_f32(&format!("{key}FormulaC"), defaults.2));
        let mut v = 0.0;
        for l in 1..=level.max(1) {
            v = match table.get((l - 1) as usize) {
                Some(&t) => t,
                None => a * v + b * l as f32 + c,
            };
        }
        v
    }

    /// The experience a unit is worth killed (FUN_6f6848b0): a Hero's GrantHeroXP at its
    /// level (up to MaxHeroLevel), anything else's GrantNormalXP at its level (up to
    /// MaxUnitLevel), nothing below level 1; a summoned unit's times SummonedKillFactor.
    pub fn kill_xp_value(&self, victim: u32) -> f32 {
        let Some(u) = self.unit(victim) else { return 0.0 };
        let hero = u.is_hero && u.extra.illusion.is_none();
        let level = if u.is_hero { u.level } else { self.objects.units.get(u.type_id).map_or(u.level, |o| o.int("level")) };
        if level < 1 {
            return 0.0;
        }
        let cap = if hero { self.misc_f32("MaxHeroLevel", 10.0) } else { self.misc_f32("MaxUnitLevel", 20.0) } as i32;
        let level = level.min(cap.max(1));
        let mut v = if hero { self.xp_table("GrantHeroXP", (1.0, 0.0, 100.0), level) } else { self.xp_table("GrantNormalXP", (1.0, 5.0, 5.0), level) };
        if self.is_summoned(victim) {
            v *= self.misc_f32("SummonedKillFactor", 0.5);
        }
        v
    }

    /// Recomputes a hero's maximum life and mana from its attributes.
    pub fn apply_hero_stats(&mut self, h: u32, fill: bool) {
        let mana_per = self.misc_f32("IntManaBonus", 15.0);
        let Some(u) = self.unit(h) else { return };
        let Some(o) = self.objects.units.get(u.type_id) else { return };
        let base_mana = o.real("manaN");
        let b = u.item_bonus;
        let int = u.int + b.int;
        let max_life = self.handicapped_life(h);
        let u = self.unit_mut(h).unwrap();
        let (old_hp, old_mana) = (u.max_life, u.max_mana);
        u.max_life = max_life;
        u.max_mana = base_mana + int.floor() * mana_per + b.mana;
        if fill {
            u.life = u.max_life;
            u.mana = u.max_mana;
        } else if !u.dead {
            u.life += u.max_life - old_hp;
            u.mana = (u.mana + u.max_mana - old_mana).max(0.0);
        }
    }

    pub fn add_hero_xp(&mut self, h: u32, xp: i32) {
        let Some(u) = self.unit_mut(h) else { return };
        // Experience is never taken away (measured: AddHeroXP of -50 leaves it); a dead
        // hero takes it too (measured: SetHeroXP to 500 reads 500 after its death).
        if !u.is_hero || xp <= 0 {
            return;
        }
        let before = u.hero_xp;
        u.hero_xp += xp;
        let owner = u.owner;
        // No more than the top level needs (measured: 51,200 given reads 5,400, level
        // 10's need, as MaxHeroLevel is 10).
        let top = self.hero_xp_needed(self.misc_f32("MaxHeroLevel", 10.0) as i32);
        let u = self.unit_mut(h).unwrap();
        u.hero_xp = u.hero_xp.min(top.max(before));
        let gained = u.hero_xp - before;
        self.score_experience(owner, gained);
        self.level_up_from_xp(h);
    }

    /// A hero's strength, agility, and intelligence for its type and level: the base and
    /// the gain times the levels past the first, at once (measured: a level 6 Paladin
    /// has 26 intelligence, 17 and 1.8 times 5, where adding 1.8 five times floors to 25).
    fn type_attrs(&self, h: u32) -> [f32; 3] {
        let Some(u) = self.unit(h) else { return [0.0; 3] };
        let Some(o) = self.objects.units.get(u.type_id) else { return [u.str_, u.agi, u.int] };
        let n = (u.level - 1).max(0) as f32;
        [o.real("STR") + o.real("STRplus") * n, o.real("AGI") + o.real("AGIplus") * n, o.real("INT") + o.real("INTplus") * n]
    }

    /// Sets a hero's attributes from its type and level and what it has gained beyond.
    pub(crate) fn refresh_hero_attrs(&mut self, h: u32) {
        let t = self.type_attrs(h);
        let Some(u) = self.unit_mut(h) else { return };
        let b = u.extra.attr_bonus;
        u.str_ = t[0] + b[0];
        u.agi = t[1] + b[1];
        u.int = t[2] + b[2];
    }

    /// Sets a hero's attributes outright, keeping what lies beyond its type's as bonus.
    pub(crate) fn set_hero_attrs(&mut self, h: u32, v: [f32; 3]) {
        let t = self.type_attrs(h);
        let Some(u) = self.unit_mut(h) else { return };
        u.extra.attr_bonus = [v[0] - t[0], v[1] - t[1], v[2] - t[2]];
        u.str_ = v[0];
        u.agi = v[1];
        u.int = v[2];
    }

    /// Levels a hero up as far as its experience reaches (FUN_6f528110's last loop).
    fn level_up_from_xp(&mut self, h: u32) {
        let max_level = self.misc_f32("MaxHeroLevel", 10.0) as i32;
        loop {
            let (level, xp) = (self.unit(h).unwrap().level, self.unit(h).unwrap().hero_xp);
            if level >= max_level || xp < self.hero_xp_needed(level + 1) {
                break;
            }
            let u = self.unit_mut(h).unwrap();
            u.level += 1;
            u.skill_points += 1;
            self.refresh_hero_attrs(h);
            self.apply_hero_stats(h, false);
            use crate::event_ids::*;
            self.queue_unit_event(h, EVENT_PLAYER_HERO_LEVEL, EVENT_UNIT_HERO_LEVEL, EventCtx::default());
        }
    }

    /// Kill experience (FUN_6f660500, FUN_6f6881b0, FUN_6f67dd60): not for a kill by a
    /// structure unless BuildingKillsGiveExp, nor an ally's.  The victim's worth is split
    /// among the heroes that gain from the killer's player (theirs, or a player's who
    /// shares experience with them) within HeroExpRange, or with GlobalExperience among
    /// all of them when none are near.  Illusions take none; heroes at the top level
    /// share only under MaxLevelHeroesDrainExp, and suspended ones share without gaining.
    pub fn grant_kill_xp(&mut self, victim: u32, killer: u32) {
        let (Some(v), Some(k)) = (self.unit(victim), self.unit(killer)) else { return };
        let (vx, vy, vowner, kowner, by_building) = (v.x, v.y, v.owner, k.owner, k.is_building);
        if crate::natives::players::is_ally(self, kowner, vowner) {
            return;
        }
        if by_building && self.misc_f32("BuildingKillsGiveExp", 0.0) == 0.0 {
            return;
        }
        let max_level = self.misc_f32("MaxHeroLevel", 10.0) as i32;
        let drain = self.misc_f32("MaxLevelHeroesDrainExp", 1.0) != 0.0;
        let range = self.misc_f32("HeroExpRange", 1200.0);
        let shares = |o: u8| o == kowner || self.players.get(kowner as usize).is_some_and(|p| p.alliances[o as usize % crate::types::PLAYER_COUNT][3]);
        let eligible: Vec<(u32, bool)> = self
            .unit_order
            .iter()
            .filter_map(|&h| self.unit(h).map(|u| (h, u)))
            .filter(|(_, u)| u.is_hero && !u.dead && u.extra.illusion.is_none() && shares(u.owner) && (drain || u.level < max_level))
            .map(|(h, u)| (h, (u.x - vx).hypot(u.y - vy) <= range))
            .collect();
        let near: Vec<u32> = eligible.iter().filter(|e| e.1).map(|e| e.0).collect();
        let heroes = if !near.is_empty() || self.misc_f32("GlobalExperience", 0.0) == 0.0 { near } else { eligible.iter().map(|e| e.0).collect() };
        if heroes.is_empty() {
            return;
        }
        let share = self.kill_xp_value(victim) / heroes.len() as f32;
        if share <= 0.0 {
            return;
        }
        let creep = vowner >= crate::types::PLAYER_NEUTRAL_AGGRESSIVE;
        for h in heroes {
            self.gain_kill_xp(h, share, creep);
        }
    }

    /// One hero's part of kill experience (FUN_6f528110): times its player's XP handicap,
    /// less the XPFactor of the highest hall tier its requirements meet (TWN1 to TWN9,
    /// by the player's hero count); from a creep, taken level by level at HeroFactorXP's
    /// percent for each level, so none past the table's zero.
    fn gain_kill_xp(&mut self, h: u32, share: f32, creep: bool) {
        let Some(u) = self.unit(h) else { return };
        if u.xp_suspended || !u.is_hero || u.dead {
            return;
        }
        let (owner, type_id, level, current) = (u.owner, u.type_id, u.level, u.hero_xp);
        let total = current as f32;
        let mut raw = share * self.players.get(owner as usize).map_or(1.0, |p| p.handicap_xp);
        let heroes = self.hero_count(owner);
        if heroes > 0 {
            let mut factor = 0.0;
            for i in 0..9u32 {
                if !self.missing_requirements(owner, type_id, i + 1).is_empty() {
                    break;
                }
                let twn = crate::objects::id_from_str(&format!("TWN{}", i + 1));
                let list: Vec<f32> = self.dependency_group(twn).map(|o| o.list("XPFactor")).unwrap_or_default().iter().filter_map(|v| v.trim().parse().ok()).collect();
                if !list.is_empty() {
                    factor = list[(heroes as usize).min(list.len()) - 1];
                }
            }
            raw *= 1.0 - factor;
        }
        let max_level = self.misc_f32("MaxHeroLevel", 10.0) as i32;
        let new_total = if !creep {
            total + raw
        } else {
            let factors = self.misc_list_f32("HeroFactorXP");
            let (mut total, mut level) = (total, level.max(1));
            while level < max_level {
                let f = factors.get((level - 1) as usize).or(factors.last()).copied().unwrap_or(100.0) / 100.0;
                if f <= 0.0 {
                    break;
                }
                let next = self.hero_xp_needed(level + 1) as f32;
                let gained = total + raw * f;
                if gained <= next {
                    total = gained;
                    break;
                }
                raw = (gained - next) / f;
                total = next;
                level += 1;
            }
            total
        };
        let top = self.hero_xp_needed(max_level);
        let added = (new_total.round() as i32).min(top.max(current)) - current;
        if added <= 0 {
            return;
        }
        if let Some(u) = self.unit_mut(h) {
            u.hero_xp += added;
        }
        self.score_experience(owner, added);
        self.level_up_from_xp(h);
    }

    /// Whether a hero can learn the next level of an ability now.
    /// What stands for a hero skill now: the skill, or what Engineering Upgrade made
    /// of it.
    /// Whether a skill is one of the hero's own (FUN_6f5294b0): its heroAbilList, where
    /// AUfa also answers for AUfu, or a skill put in place of one of them.
    fn in_hero_list(&self, h: u32, ability: Id) -> bool {
        let Some(u) = self.unit(h) else { return false };
        let list: Vec<Id> = self.objects.units.get(u.type_id).map(|o| o.list("heroAbilList")).unwrap_or_default().iter().map(|s| crate::objects::id_from_str(s.trim())).collect();
        let listed = |a: Id| list.iter().any(|&x| x == a || (a == *b"AUfa" && x == *b"AUfu"));
        listed(ability) || u.extra.swapped.iter().any(|s| s.1 == ability && listed(s.0))
    }

    pub fn learned_id(&self, h: u32, ability: Id) -> Id {
        self.unit(h).and_then(|u| u.extra.swapped.iter().find(|s| s.0 == ability)).map_or(ability, |s| s.1)
    }

    /// Engineering Upgrade (CAbilityEngineeringUpgrade) at `level`: each of its DataC to
    /// DataF pairs ("old,new") puts the new ability in place of the old at the old's
    /// level, now or when the old is learned later.
    fn engineering_upgrade(&mut self, h: u32, a: Id, level: u32) {
        for field in ["DataC", "DataD", "DataE", "DataF"] {
            let pair = self.objects.abilities.get(a).map(|o| o.level_str(field, level)).unwrap_or_default();
            let mut it = pair.split(',').map(|s| id_from_str(s.trim()));
            let (Some(old), Some(new)) = (it.next(), it.next()) else { continue };
            if old[0] == 0 || new[0] == 0 {
                continue;
            }
            let Some(u) = self.unit_mut(h) else { return };
            match u.extra.swapped.iter_mut().find(|s| s.1 == old) {
                Some(s) => s.1 = new,
                None => u.extra.swapped.push((old, new)),
            }
            if let Some(e) = u.abilities.iter_mut().find(|e| e.0 == old) {
                e.0 = new;
            }
        }
        self.detach_ability_art(h);
        self.attach_ability_art(h);
    }

    /// The hero level the next level of a hero's skill needs: its reqLevel, and
    /// levelSkip more for each level it has (HeroAbilityLevelSkip when that is 0)
    /// (FUN_6f529fc0).
    pub fn learn_level(&self, h: u32, ability: Id) -> i32 {
        let ability = self.learned_id(h, ability);
        let current = self.unit(h).and_then(|u| u.abilities.iter().find(|(a, _)| *a == ability)).map_or(0, |a| a.1);
        let ab = self.objects.abilities.get(ability);
        let req = ab.map_or(1, |a| a.int("reqLevel").max(1));
        let skip = match ab.map_or(0, |a| a.int("levelSkip")) {
            0 => self.misc_f32("HeroAbilityLevelSkip", 2.0) as i32,
            v => v,
        };
        req + current as i32 * skip
    }

    /// Whether a hero may take the next level of a skill, skill points aside: below its
    /// top level, with its requirements met and the hero level it needs, which
    /// whoisjohngalt waives (FUN_6f528d20).
    pub fn learn_allowed(&self, h: u32, ability: Id) -> bool {
        if !self.in_hero_list(h, ability) {
            return false;
        }
        let ability = self.learned_id(h, ability);
        let Some(u) = self.unit(h) else { return false };
        let current = u.abilities.iter().find(|(a, _)| *a == ability).map_or(0, |a| a.1);
        let max = self.objects.abilities.get(ability).map_or(3, |a| a.int("levels").max(1));
        current < max && self.missing_requirements(u.owner, ability, 0).is_empty() && (u.level >= self.learn_level(h, ability) || self.cheat(crate::cheats::flag::HERO_LEVEL))
    }

    /// Whether every skill of a hero's list stands at its top level.
    pub fn all_skills_learned(&self, h: u32) -> bool {
        let Some(u) = self.unit(h) else { return false };
        let list: Vec<Id> = self.objects.units.get(u.type_id).map(|o| o.list("heroAbilList")).unwrap_or_default().iter().map(|s| crate::objects::id_from_str(s.trim())).filter(|a| a[0] != 0).collect();
        !list.is_empty()
            && list.iter().all(|&a| {
                let a = self.learned_id(h, a);
                let current = u.abilities.iter().find(|(x, _)| *x == a).map_or(0, |x| x.1);
                current >= self.objects.abilities.get(a).map_or(3, |o| o.int("levels").max(1))
            })
    }

    pub fn can_learn(&self, h: u32, ability: Id) -> bool {
        self.unit(h).is_some_and(|u| u.skill_points > 0) && self.learn_allowed(h, ability)
    }

    /// Learns or improves a hero ability, spending a skill point.
    pub fn learn_skill(&mut self, h: u32, ability: Id) -> bool {
        if !self.can_learn(h, ability) {
            return false;
        }
        let ability = self.learned_id(h, ability);
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
        if self.ability_code(ability) == "ANeg" {
            self.engineering_upgrade(h, ability, level as u32);
        }
        use crate::event_ids::*;
        let ctx = EventCtx { spell_ability: crate::objects::id_to_int(ability), state: level, ..Default::default() };
        self.queue_unit_event(h, EVENT_PLAYER_HERO_SKILL, EVENT_UNIT_HERO_SKILL, ctx);
        true
    }

    /// A unit's life and mana regeneration a second.
    fn regen_rates(&self, h: u32) -> Option<(f32, f32)> {
        let u = self.unit(h)?;
        if u.dead || u.constructing.is_some() {
            return None;
        }
        let o = self.objects.units.get(u.type_id)?;
        let mut hp = o.real("regenHP");
        let mut mp = o.real("regenMana");
        // regenType: "always", "night", "day", "blight", or "none".
        let rt = o.str("regenType").to_ascii_lowercase();
        let day = (6.0..18.0).contains(&self.time_of_day);
        // An Undead's life comes back only on blight (measured: a ghoul off blight
        // stays at 100 for ten seconds).
        let off_blight = rt == "blight" && !self.point_blighted(u.x, u.y);
        if rt == "none" || (rt == "night" && day) || (rt == "day" && !day) || off_blight {
            hp = 0.0;
        }
        let ub = self.upgrade_bonus(u.owner, u.type_id);
        hp += ub.hp_regen + u.item_bonus.life_regen;
        if u.is_hero {
            hp += (u.str_ + u.item_bonus.str_).floor() * self.misc_f32("StrRegenBonus", 0.05);
            // A hero's mana comes from its intelligence alone, without its type's
            // regenMana (measured: a Paladin of 17 intelligence regains 8.50 in 10
            // seconds, its regenMana 0.01 not added; its life 13.50, regenHP 0.25
            // with 22 strength's 1.1).
            mp = (u.int + u.item_bonus.int).floor() * self.misc_f32("IntRegenBonus", 0.05);
        }
        // Mana regeneration items and research add a fraction of that rate (measured:
        // a Sobi Mask, half again, gives a Paladin of 17 intelligence 4.3 more in 10
        // seconds; ours added half of its regenMana, 0.05).
        mp *= 1.0 + u.item_bonus.mana_regen + ub.mana_regen;
        Some((hp, mp))
    }

    /// The listeners on mana (`UnitExtra::mana_listeners`) whose thresholds the mana
    /// reaches in this tick, by the main tick they fall in: their due times are on the
    /// game clock, which trails exact time, so each falls in the main tick whose clock
    /// first reaches it (read: an Ogre Lord's 100.1 at 1.25 a second, due at 80.08,
    /// re-armed in turn 803, the clock then 0.025 short, before that turn's first step;
    /// a creep at 0.5 a second reaching 5.1 at 10.2, in turn 104).  The port's mana
    /// passes a threshold before the clock does, so a listener stays due until then.
    pub fn plan_mana_listeners(&mut self, dt: f32) {
        let start = (self.game_ms as f64 - (dt as f64 * 1000.0).round()) / 1000.0;
        let mut due = Vec::new();
        for &h in &self.unit_order {
            let Some((_, mp)) = self.regen_rates(h).filter(|r| r.1 > 0.0) else { continue };
            let Some(u) = self.unit(h) else { continue };
            let (first, n) = (self.tick_clocks.0, self.tick_clocks.1.len());
            for &(thr, slot) in &u.extra.mana_listeners {
                let at = (thr - u.mana) / mp;
                let when = (start + at as f64) as f32 as f64;
                if let Some(i) = (0..n).position(|i| self.tick_abs(i).is_some_and(|c| c >= when)).filter(|&i| i + 1 < n) {
                    due.push((first + i as i64, at, h, slot));
                }
            }
        }
        // (Events falling due together run in the order queued: the listeners' making.)
        due.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)).then(a.3.cmp(&b.3)));
        self.mana_due = due;
    }

    /// The planned listeners falling in main ticks up to `upto` (all, without it), each
    /// replaced by one waiting for the mana to fall back below the cost (not yet
    /// followed), the old released once those of their main tick are made (read: an
    /// Ogre Lord's re-armed in turn 803, the old freed right after the new was made;
    /// two High Priests' in turn 70, both new made, then both old freed).
    pub fn fire_mana_listeners(&mut self, upto: Option<i64>) {
        let n = self.mana_due.iter().take_while(|e| upto.is_none_or(|t| e.0 <= t)).count();
        let fired: Vec<(i64, f32, u32, u32)> = self.mana_due.drain(..n).collect();
        for group in fired.chunk_by(|a, b| a.0 == b.0) {
            let new: Vec<u32> = group.iter().map(|_| self.tags.take(b"`rtl")).collect();
            for (&(_, _, h, slot), &n) in group.iter().zip(&new) {
                self.tags.release(slot);
                // The new one waits for the mana to fall (not yet followed: it never
                // fires here), kept in the old one's place, to go as the creep sleeps.
                if let Some(l) = self.unit_mut(h).and_then(|u| u.extra.mana_listeners.iter_mut().find(|l| l.1 == slot)) {
                    *l = (f32::INFINITY, n);
                }
            }
        }
    }

    /// Structures' alarms: their wait over, a computer player's structure watches for
    /// enemies, a position listener for event 0xd01c8 (traced: an undead computer's
    /// Necropolis at 0.72 seconds and Haunted Gold Mine at 0.745, `FUN_6f436510` on event
    /// 0xd01c1; a user's Town Hall and the neutral gold mines none; provisional: the
    /// condition, the unit's flags 4 and not 0x100 at +0x5c, read as its owner a
    /// computer).
    pub fn alarms(&mut self, dt: f32) {
        let hs: Vec<u32> = self.unit_order.clone();
        for h in hs {
            let Some(u) = self.unit_mut(h) else { continue };
            let Some(w) = u.extra.alarm_wait.as_mut() else { continue };
            *w -= dt;
            if *w > 0.0 {
                continue;
            }
            u.extra.alarm_wait = None;
            let owner = u.owner;
            let computer = self.players.get(owner as usize).is_some_and(|p| p.controller == crate::types::map_control::COMPUTER && p.slot_state == crate::types::slot_state::PLAYING);
            if computer && self.unit(h).is_some_and(|u| !u.dead) {
                self.tags.take(b"`pil");
            }
        }
    }

    /// Hero health and mana regeneration and unit regeneration.
    pub fn regenerate(&mut self, dt: f32) {
        let hs: Vec<u32> = self.unit_order.clone();
        // Regeneration on blight looks at the ground every two seconds from the unit's
        // making, its timer made as it finds blight (traced: an undead start's ghoul and
        // three acolytes each take one at 2.0 seconds, the haunted mine's blight under
        // them since its making; event 0xd01c1, `FUN_6f43a200`; provisional: the period,
        // and leaving blight frees it).
        for &h in &hs {
            let Some(u) = self.unit_mut(h) else { continue };
            let Some(c) = u.extra.blight_check.as_mut() else { continue };
            *c -= dt;
            if *c > 1e-4 {
                continue;
            }
            *c += 2.0;
            let (x, y, had) = (u.x, u.y, u.extra.blight_rcm);
            match (self.point_blighted(x, y), had) {
                (true, None) => {
                    let s = self.tags.take(b"`rcm");
                    if let Some(u) = self.unit_mut(h) {
                        u.extra.blight_rcm = Some(s);
                    }
                }
                (false, Some(s)) => {
                    self.tags.release(s);
                    if let Some(u) = self.unit_mut(h) {
                        u.extra.blight_rcm = None;
                    }
                }
                _ => {}
            }
        }
        for h in hs {
            let Some((hp, mp)) = self.regen_rates(h) else { continue };
            let u = self.unit_mut(h).unwrap();
            u.life = (u.life + hp * dt).min(u.max_life);
            u.mana = (u.mana + mp * dt).min(u.max_mana);
        }
    }
}


