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
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct ArmorInfo {
    pub armor: i32,
    pub bonus: f32,
    /// defType, lowercase.
    pub def_type: String,
    pub upgrade: Option<i32>,
    pub invulnerable: bool,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct HeroInfo {
    /// Primary attribute: "STR", "AGI", or "INT".
    pub primary: String,
    /// Base and bonus for strength, agility, and intellect.
    pub attributes: [(i32, i32); 3],
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct InfoStats {
    pub attack: Option<AttackInfo>,
    pub armor: ArmorInfo,
    pub food_made: i32,
    pub gold: Option<i32>,
    pub hero: Option<HeroInfo>,
}

impl World {
    /// The research level of the first upgrade in a unit's `upgrades` list with one of
    /// `effects`.
    fn upgrade_level_for(&self, owner: u8, h_type: crate::objects::Id, effects: &[&str]) -> Option<i32> {
        let o = self.objects.units.get(h_type)?;
        let p = self.players.get(owner as usize)?;
        for up in o.list("upgrades") {
            let id = id_from_str(up.trim());
            let Some(u) = self.objects.upgrades.get(id) else { continue };
            if (1..=4).any(|k| effects.contains(&u.str(&format!("effect{k}")).as_str())) {
                return Some(p.tech_researched.get(&id).copied().unwrap_or(0));
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
            let base = wp.base + ub.damage + self.primary_attribute_pub(h);
            let bonus = u.item_bonus.damage + self.buff_damage_bonus(h);
            s.attack = Some(AttackInfo {
                min: (base + dice as f32) as i32,
                max: (base + (dice * wp.sides) as f32) as i32,
                bonus: bonus as i32,
                attack_type: wp.attack_type.clone(),
                upgrade: self.upgrade_level_for(u.owner, u.type_id, &["ratd", "ratx"]),
            });
        }
        let def = o.map_or(0.0, |o| o.real("def")) + ub.armor;
        let agi = if u.is_hero { (u.agi + u.item_bonus.agi).floor() * self.misc_f32("AgiDefenseBonus", 0.3) + self.misc_f32("AgiDefenseBase", -2.0) } else { 0.0 };
        s.armor = ArmorInfo {
            armor: (def + agi).round() as i32,
            bonus: u.item_bonus.armor + self.buff_mods(h).2,
            def_type: self.defense_type(u),
            upgrade: self.upgrade_level_for(u.owner, u.type_id, &["rarm"]),
            invulnerable: self.unit_invulnerable(u),
        };
        s.food_made = o.map_or(0, |o| o.int("fmade"));
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
