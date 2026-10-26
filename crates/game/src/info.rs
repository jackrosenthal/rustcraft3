//! What the console's info panel shows for a unit (CInfoPanelUnitDetail and its
//! CInfoPanelIcon* parts): attack and armor values with their bonuses, the research
//! level that improves each, food provided, gold left, and hero attributes.

use crate::objects::id_from_str;
use crate::sim::weapon;
use crate::world::World;

/// One attack's line: the white range and the colored bonus.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct AttackInfo {
    pub min: i32,
    pub max: i32,
    pub bonus: i32,
    /// atkType1, lowercase (normal, pierce, siege, magic, chaos, hero).
    pub attack_type: String,
    /// The research level of the upgrade that improves it, when one does.
    pub upgrade: Option<i32>,
    /// That upgrade.
    pub upgrade_id: Option<crate::objects::Id>,
    /// Its range and what improves it, and its cooldown and the attack speed added.
    pub range: i32,
    pub range_bonus: i32,
    pub cooldown: f32,
    pub speed_bonus: f32,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct ArmorInfo {
    pub armor: i32,
    pub bonus: f32,
    /// defType, lowercase.
    pub def_type: String,
    pub upgrade: Option<i32>,
    pub upgrade_id: Option<crate::objects::Id>,
    pub invulnerable: bool,
    /// The share of damage its armor turns away (negative for more).
    pub reduction: f32,
    /// For a unit that moves: its speed now and what it gains on its own.
    pub move_speed: Option<(f32, f32)>,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct HeroInfo {
    /// Primary attribute: "STR", "AGI", or "INT".
    pub primary: String,
    /// Base and bonus for strength, agility, and intellect.
    pub attributes: [(i32, i32); 3],
}

/// A spellcaster's rank (Casterupgradeart, Casterupgradename, Casterupgradetip at its
/// caster training's level).
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct RankInfo {
    pub art: String,
    pub name: String,
    pub tip: String,
    pub level: i32,
    pub upgrade_id: Option<crate::objects::Id>,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct InfoStats {
    pub attack: Option<AttackInfo>,
    pub armor: ArmorInfo,
    pub food_made: i32,
    pub gold: Option<i32>,
    pub hero: Option<HeroInfo>,
    pub rank: Option<RankInfo>,
}

impl World {
    /// The first upgrade in a unit's `upgrades` list of one of `classes` (UpgradeData
    /// class: melee and ranged for its attack, armor, caster for its rank), and its
    /// research level.
    fn upgrade_for(&self, owner: u8, h_type: crate::objects::Id, classes: &[&str]) -> Option<(crate::objects::Id, i32)> {
        let o = self.objects.units.get(h_type)?;
        let p = self.players.get(owner as usize)?;
        for up in o.list("upgrades") {
            let id = id_from_str(up.trim());
            let Some(u) = self.objects.upgrades.get(id) else { continue };
            if classes.contains(&u.str("class").to_ascii_lowercase().as_str()) {
                return Some((id, p.tech_researched.get(&id).copied().unwrap_or(0)));
            }
        }
        None
    }

    pub fn info_stats(&self, h: u32) -> InfoStats {
        let mut s = InfoStats::default();
        let Some(u) = self.unit(h) else { return s };
        let o = self.objects.units.get(u.type_id);
        let ub = self.upgrade_bonus(u.owner, u.type_id);
        if let Some(wp) = weapon(self, u.type_id) {
            let dice = wp.dice + ub.dice;
            // The base takes the hero's own primary attribute; what items and abilities
            // add to it shows in the bonus (measured: OrcX03a's Rexxar, strength 56 + 6,
            // reads 68 - 78 +34 in the original, where ours read 74 - 84 +28).
            let (own, all) = (self.primary_attribute_base(h), self.primary_attribute_pub(h));
            let base = wp.base + ub.damage + own;
            let (min, max) = ((base + dice as f32) as i32, (base + (dice * wp.sides) as f32) as i32);
            // Damage factors (Inner Fire, Trueshot Aura) show as that share of the mean,
            // rounded (measured with Inner Fire's tenth: +1 on a footman's 12 - 13, +2 on
            // a grunt's 18 - 21 and an archer's 16 - 18; Command Aura's tenth +8 on
            // Rexxar's 74 - 84 with his strength's 6).
            let factor = self.buff_mods(h).3 - 1.0;
            let mean = (min + max) as f32 / 2.0 + (all - own);
            // A carried orb's DataA, added as it strikes (orbs.rs), shows there too
            // (measured: HumanX06's Illidan with the Orb of Kil'jaeden, AIgd's 6, reads
            // 37 - 59 +6 in the original).
            let orbs: f32 = u
                .inventory
                .iter()
                .filter_map(|&i| self.item(i))
                .flat_map(|i| self.item_abilities(i.type_id))
                .filter(|&a| matches!(self.ability_code(a).as_str(), "AIob" | "AIcb" | "AIpb" | "AIlb" | "AIsb" | "AIfb"))
                .map(|a| self.data(a, "DataA", 1))
                .sum();
            let bonus = u.item_bonus.damage + orbs + self.buff_damage_bonus(h) + (all - own) + (factor * mean).round();
            let up = self.upgrade_for(u.owner, u.type_id, &["melee", "ranged"]);
            s.attack = Some(AttackInfo {
                min,
                max,
                bonus: bonus as i32,
                attack_type: wp.attack_type.clone(),
                upgrade: up.map(|(_, l)| l),
                upgrade_id: up.map(|(id, _)| id),
                range: wp.range as i32,
                range_bonus: ub.range as i32,
                cooldown: wp.cooldown,
                speed_bonus: u.item_bonus.attack_speed + ub.attack_speed + (self.buff_mods(h).1 - 1.0),
            });
        }
        let def = o.map_or(0.0, |o| o.real("def")) + ub.armor;
        // The base takes the hero's own agility, the bonus what items and abilities add
        // to it (measured: Rexxar, agility 28 + 6, reads 7 +9.8 in the original).
        let per = self.misc_f32("AgiDefenseBonus", 0.3);
        let agi = if u.is_hero { u.agi.floor() * per + self.misc_f32("AgiDefenseBase", -2.0) } else { 0.0 };
        let agi_bonus = if u.is_hero { ((u.agi + u.item_bonus.agi).floor() - u.agi.floor()) * per } else { 0.0 };
        let up = self.upgrade_for(u.owner, u.type_id, &["armor"]);
        let bonus = u.item_bonus.armor + self.buff_mods(h).2 + agi_bonus;
        // DefenseArmor's reduction, or for negative armor the original's table
        // (FUN_6f66d320) as the combat uses it.
        let total = (def + agi).round() + bonus;
        let reduction = if total >= 0.0 {
            let k = self.misc_f32("DefenseArmor", 0.06);
            total * k / (1.0 + total * k)
        } else {
            1.0 - crate::sim::final_damage(self, 1.0, "chaos", u).max(0.0)
        };
        s.armor = ArmorInfo {
            armor: (def + agi).round() as i32,
            bonus,
            def_type: self.defense_type(u),
            upgrade: up.map(|(_, l)| l),
            upgrade_id: up.map(|(id, _)| id),
            invulnerable: self.unit_invulnerable(u),
            reduction,
            move_speed: (!u.is_building && u.move_speed > 0.0).then(|| {
                let now = self.current_move_speed(h);
                (now, now - u.move_speed)
            }),
        };
        s.food_made = o.map_or(0, |o| o.int("fmade"));
        // A spellcaster's rank (CInfoPanelIconRank::vf27, FUN_6f340c30 and FUN_6f340c60).
        if let Some(o) = o {
            let art = o.str("Casterupgradeart");
            let names = o.list("Casterupgradename");
            if !art.is_empty() && !names.is_empty() {
                let up = self.upgrade_for(u.owner, u.type_id, &["caster"]);
                let level = up.map_or(0, |(_, l)| l);
                let entry = |list: Vec<String>| list.get((level.max(0) as usize).min(list.len().saturating_sub(1))).map(|s| s.trim().to_string()).unwrap_or_default();
                s.rank = Some(RankInfo { art, name: entry(names), tip: entry(o.list("Casterupgradetip")), level, upgrade_id: up.map(|(id, _)| id) });
            }
        }
        s.gold = self.ability_with_code(h, "Agld").map(|_| u.resource);
        if u.is_hero {
            let b = u.item_bonus;
            s.hero = Some(HeroInfo {
                primary: o.map(|o| o.str("Primary").to_ascii_uppercase()).unwrap_or_default(),
                attributes: [(u.str_ as i32, b.str_ as i32), (u.agi as i32, b.agi as i32), (u.int as i32, b.int as i32)],
            });
        }
        s
    }
}

/// What a structure is producing, for CInfoPanelBuildingDetail's modes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ProductionKind {
    Training,
    /// A structure's own upgrade (FUN_6f3ccfd0 mode 2: TRAINING without the queue).
    Upgrading,
    Researching,
    Reviving,
    Constructing,
    /// A unit changing form over time (FUN_6f3d2cb0: MORPHING).
    Morphing,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Production {
    pub kind: ProductionKind,
    /// Icons of the queued items, the one in progress first.
    pub icons: Vec<String>,
    /// Fraction of the first item done.
    pub progress: f32,
}

impl World {
    fn upgrade_icon(&self, owner: u8, id: crate::objects::Id) -> Option<String> {
        let up = self.objects.upgrades.get(id)?;
        let level = self.players.get(owner as usize).and_then(|p| p.tech_researched.get(&id).copied()).unwrap_or(0);
        let art = up.list("Art");
        art.get(level as usize).or(art.last()).map(|s| s.trim().to_string())
    }

    pub fn production(&self, h: u32) -> Option<Production> {
        let u = self.unit(h)?;
        if let Some((left, to, _)) = u.extra.morph {
            if left > 0.0 && u.extra.morph_total > 0.0 {
                let art = self.objects.units.get(to).map(|o| o.str("Art")).unwrap_or_default();
                return Some(Production { kind: ProductionKind::Morphing, icons: vec![art], progress: (1.0 - left / u.extra.morph_total).clamp(0.0, 1.0) });
            }
        }
        if let Some(left) = u.constructing {
            let total = u.build_time.max(0.01);
            let art = self.objects.units.get(u.type_id).map(|o| o.str("Art")).unwrap_or_default();
            return Some(Production { kind: ProductionKind::Constructing, icons: vec![art], progress: (1.0 - left / total).clamp(0.0, 1.0) });
        }
        if let Some((hero, done, need)) = u.revive {
            let art = self.unit(hero).and_then(|hu| self.objects.units.get(hu.type_id)).map(|o| o.str("Art")).unwrap_or_default();
            return Some(Production { kind: ProductionKind::Reviving, icons: vec![art], progress: (done / need.max(0.01)).clamp(0.0, 1.0) });
        }
        let &first = u.queue.first()?;
        let icon = |id| self.upgrade_icon(u.owner, id).or_else(|| self.objects.units.get(id).map(|o| o.str("Art"))).unwrap_or_default();
        let icons = u.queue.iter().map(|&id| icon(id)).collect();
        let (kind, total) = if let Some(up) = self.objects.upgrades.get(first) {
            let level = self.players.get(u.owner as usize).and_then(|p| p.tech_researched.get(&first).copied()).unwrap_or(0);
            (ProductionKind::Researching, up.real("timebase") + up.real("timemod") * level as f32)
        } else {
            let id_s = crate::objects::id_str(first);
            let own = self.objects.units.get(u.type_id).is_some_and(|o| o.list("Upgrade").iter().any(|x| x.trim() == id_s));
            let kind = if own { ProductionKind::Upgrading } else { ProductionKind::Training };
            (kind, self.objects.units.get(first).map_or(1.0, |o| o.real("bldtm")))
        };
        Some(Production { kind, icons, progress: (u.queue_progress / total.max(0.01)).clamp(0.0, 1.0) })
    }
}
