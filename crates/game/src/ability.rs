//! Abilities: casting (orders, range, cast point, mana, cooldown), buffs and auras, and
//! the effects of each ability code.  Field meanings come from AbilityMetaData.slk.

use w3_jass::Vm;

use crate::event_ids::*;
use crate::objects::{Id, id_from_str};
use crate::types::*;
use crate::world::World;

/// How an ability's order is aimed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Targeting {
    None,
    Unit,
    Point,
    UnitOrPoint,
    Passive,
}

/// Buffs abilities give when their BuffID names none (their classes' defaults: 'Bpxf'
/// for Phoenix Fire, 'Bdef' for Defend's CBuffDefense).
pub fn default_buff(code: &str) -> Option<&'static str> {
    match code {
        "Apxf" => Some("Bpxf"),
        "Adef" => Some("Bdef"),
        _ => None,
    }
}

/// The orders built into ability classes (each class's vf194, its code its vf7) whose
/// abilities have no order string in their func data, or one the class does not answer
/// to (measured: a Dread Lord's Inferno is refused as "inferno", its func data's, and
/// taken as "dreadlordinferno", its class's).
const BUILT_IN_ORDERS: &[(&str, i32)] = &[
    ("AUin", 852224), // CAbilityDreadLordInferno
    ("ACad", 852246), // CAbilityCreepAnimateDead
    ("ACbf", 852560), // CAbilityBreathOfFrost
    ("AIsh", 852573), // CAbilitySpiritTroll
    ("Aabs", 852529), // CAbilityAbsorb
    ("Aave", 852531), // CAbilityAvengerForm
    ("Alam", 852201), // CAbilitySacrificialLamb
    ("ACrn", 852251), // CAbilityCreepReincarnation
    ("AEev", 852172), // CAbilityEvasion
    ("AHca", 852244), // CAbilityColdArrows
    ("AHdr", 852487), // CAbilityDrain
    ("ANbs", 852577), // CAbilityBlackArrowSpell
    ("ANca", 852582), // CAbilityCleavingAttack
    ("ANdb", 852584), // CAbilityDrunkenBrawler
    ("ANdp", 852229), // CAbilityDarkPortal
    ("ANfb", 852231), // CAbilityFireBolt
    ("ANfd", 852230), // CAbilityFingerOfDeath
    ("ANfy", 852655), // CAbilityFactory
    ("ANgl", 852233), // CAbilityNeutralG2L
    ("ANlg", 852234), // CAbilityNeutralL2G
    ("ANpi", 852236), // CAbilityPermImmolation
    ("ANpr", 852568), // CAbilityPreservation
    ("ANrc", 852237), // CAbilityRainOfChaos
    ("ANsa", 852569), // CAbilitySanctuary
    ("ANse", 852572), // CAbilitySpellShieldAoe
    ("ANsp", 852235), // CAbilityNeutralSpies
    ("ANss", 852571), // CAbilitySpellShield
    ("ANsu", 852604), // CAbilitySubmerge
    ("AOcr", 852120), // CAbilityCriticalStrike
    ("AOre", 852124), // CAbilityReincarnation
    ("AOwd", 852504), // CAbilityWard
    ("AUds", 852220), // CAbilityDarkSummoning
    ("AUts", 852559), // CAbilityThornyShield
    ("Aami", 852186), // CAbilityAntiMagicShellInstant
    ("Aast", 852490), // CAbilityAncestralSpirit
    ("Abar", 852136), // CAbilityBarkskin
    ("Abof", 852492), // CAbilityBallsOfFire
    ("Achd", 852111), // CAbilityCargoHoldDeath
    ("Acoa", 852143), // CAbilityCoupleArcher
    ("Acoh", 852142), // CAbilityCoupleHippogryph
    ("Acoi", 852508), // CAbilityCoupleInstant
    ("Adch", 852495), // CAbilityDisenchant
    ("Adta", 852015), // CAbilityDetectAoe
    ("Adts", 852070), // CAbilityMagicSentry
    ("Advm", 852536), // CAbilityDevourMagic
    ("Aetl", 852510), // CAbilityEthereal
    ("Aexh", 852538), // CAbilityExhume
    ("Afak", 852540), // CAbilityFlamingAttack
    ("Afbk", 852475), // CAbilityFeedback
    ("Aflk", 852476), // CAbilityFlakCannon
    ("Afsh", 852477), // CAbilityFragShards
    ("Agyb", 852061), // CAbilityGyroBombs
    ("Agyv", 852062), // CAbilityGyroVision
    ("Aimp", 852152), // CAbilityImpalingBolt
    ("Aliq", 852498), // CAbilityLiquidFire
    ("Amec", 852564), // CAbilityMechanicalCritter
    ("Amgl", 852154), // CAbilityMoonGlaive
    ("Amim", 852153), // CAbilityMagicImmunity
    ("Andt", 852023), // CAbilityNeutralDetectAoe
    ("Anei", 852566), // CAbilityNeutralInteract
    ("Aphx", 852482), // CAbilityPhoenix
    ("Apig", 852567), // CAbilityPandaImmolation
    ("Apo2", 852661), // CAbilityPoisonDamage
    ("Apoi", 852016), // CAbilityPoisonAttack
    ("Apts", 852194), // CAbilityPlagueToss
    ("Apxf", 852481), // CAbilityPhoenixFire
    ("Aret", 852471), // CAbilityRetrain
    ("Arng", 852241), // CAbilityRevenge
    ("Aroc", 852472), // CAbilityBarrage
    ("Arsk", 852518), // CAbilityResistantSkin
    ("Asal", 852112), // CAbilitySalvage
    ("Ashs", 852570), // CAbilityShadowSight
    ("Aspo", 852167), // CAbilitySlowPoison
    ("Assk", 852519), // CAbilityStoneSkin
    ("Aste", 852574), // CAbilitySteal
    ("Asth", 852078), // CAbilityStormHammers
    ("Atlp", 852080), // CAbilityTankLoadPilot
    ("Atpi", 852081), // CAbilityTankPilot
    ("Atru", 852208), // CAbilityTrueSight
    ("Aven", 852115), // CAbilityVenomSpear
    ("Awar", 852116), // CAbilityWarStomp
];

/// The orders abilities carry in their code rather than in an order string
/// (CAbilityItemDetectAoe::vf194 for Reveal, CAbilityMeatDrop's for Drop All Corpses,
/// whose order string names no order, and the classes in BUILT_IN_ORDERS).
pub fn intrinsic_order(code: &str) -> Option<i32> {
    match code {
        "AIta" => Some(852270),
        "Amed" => Some(852054),
        _ => BUILT_IN_ORDERS.iter().find(|(c, _)| *c == code).map(|(_, o)| *o),
    }
}

/// Spells that fly to their target as a missile and take hold as it lands.
pub(crate) fn spell_flies(code: &str) -> bool {
    matches!(code, "ANdh" | "AEsh" | "Aens" | "ANab" | "ACtb" | "ANfb" | "ANpa" | "ANtm")
}

/// Abilities whose code takes a point rather than a unit target.
fn point_target(code: &str) -> bool {
    matches!(code, "AIpm" | "AImo" | "Ablp" | "AIbl" | "AIdi" | "ANsy" | "ANmo" | "Adtn" | "AIta" | "Afla" | "Aclf" | "AHbz" | "AHfs" | "AOsh" | "AUcs" | "AUim" | "ANrf" | "AEbl" | "AUin" | "AOeq" | "AOsw" | "AOfs" | "AUdd" | "AEfn" | "ANst" | "ANvc" | "Adro" | "Aroo" | "Adis" | "Advm" | "Ahwd" | "Aeye" | "Asta" | "ANbf" | "ANcs" | "ANdp" | "ANrc" | "ANhs" | "ANsi" | "ANto" | "ANdh")
}

/// Abilities with no target (cast instantly around the caster).
fn no_target(code: &str) -> bool {
    matches!(code, "Adef" | "Amdf" | "ANth" | "AUls" | "Afzy" | "Amed" | "Amic" | "Adec" | "Amfl" | "AHtc" | "AHds" | "AOws" | "AOwk" | "AOmi" | "AHav" | "AUan" | "AHre" | "AEim" | "AEme" | "AOww" | "AEtq" | "AEsf" | "AOvd" | "AOsf" | "AHpx" | "AEst" | "AEsv" | "AUcb" | "AHwe" | "AEfk" | "ANdr" | "Astd" | "Abtl" | "Acan" | "Aroa" | "Atau" | "Absk" | "ANef" | "ANht" | "ANms" | "ANlm" | "ANsg" | "ANsq" | "ANsw")
}

/// Passive abilities and auras.
fn passive(code: &str) -> bool {
    matches!(code, "AHbh" | "AOcr" | "AEev" | "AHad" | "AHab" | "AUav" | "AOae" | "AEar" | "AEah" | "AOr2" | "AUts" | "ANba" | "AUau" | "Aamk" | "AOre")
}

impl World {
    pub fn ability_code(&self, id: Id) -> String {
        let code = self.objects.abilities.get(id).map(|a| a.str("code")).unwrap_or_default();
        // Creep copies of hero and unit spells, with the same fields as the originals.
        match code.as_str() {
            "ACtc" => "AHtc",
            "ACbf" => "ANbf",
            "ACdv" => "Adev",
            "ACrn" => "AOre",
            "ACad" => "AUan",
            // Akama's Shadow Meld.
            "Ahid" => "Ashm",
            // Inferno and Watery Minion, with Inferno's and Summon Water Elemental's
            // fields.
            "ANin" => "AUin",
            "ANwm" => "AHwe",
            // Items with their spells' fields: Rod of Necromancy, Scepter of the Dead
            // (Animate Dead), Scroll of Resurrection, Spider Silk Broach, Flare Gun.
            "AIrd" => "Arai",
            "AIan" => "AUan",
            "AIrs" => "AHre",
            "AIwb" => "Aweb",
            "AIfa" => "Afla",
            _ => return code,
        }
        .to_string()
    }

    pub fn ability_level(&self, h: u32, id: Id) -> u32 {
        let Some(u) = self.unit(h) else { return 0 };
        let base = u.abilities.iter().find(|(a, _)| *a == id).map_or(0, |a| a.1.max(0) as u32);
        // An item's ability is its holder's, at level 1.
        if base == 0 && u.inventory.iter().any(|&it| it != 0 && self.item(it).is_some_and(|i| self.item_abilities(i.type_id).contains(&id))) {
            return 1;
        }
        // An ability whose research is not done does nothing yet.
        if base > 0 && !self.ability_enabled(u.owner, id) {
            return 0;
        }
        // Research raises unit spells (rlev); hero skills level by learning.
        if base > 0 && !u.is_hero {
            let code = self.ability_code(id);
            return base + self.upgrade_spell_levels(u.owner, u.type_id, &code).max(0) as u32;
        }
        base
    }

    pub fn data(&self, id: Id, field: &str, level: u32) -> f32 {
        self.objects.abilities.get(id).map_or(0.0, |a| a.level_real(field, level.max(1)))
    }

    pub fn targeting(&self, id: Id) -> Targeting {
        let code = self.ability_code(id);
        // Channel (ANcl): its target type, DataB (none, unit, point, either).
        if code == "ANcl" {
            return match self.data(id, "DataB", 1) as i32 {
                1 => Targeting::Unit,
                2 => Targeting::Point,
                3 => Targeting::UnitOrPoint,
                _ => Targeting::None,
            };
        }
        if passive(&code) {
            Targeting::Passive
        } else if no_target(&code) {
            Targeting::None
        } else if point_target(&code) {
            Targeting::Point
        } else {
            Targeting::Unit
        }
    }

    /// The order an ability answers to: Channel's (ANcl) is its DataF, Base Order ID.
    pub fn order_string(&self, a: Id) -> String {
        let Some(o) = self.objects.abilities.get(a) else { return String::new() };
        if o.str("code") == "ANcl" {
            let base = o.level_str("DataF", 1).trim().to_string();
            if !base.is_empty() && base != "-" && base != "_" {
                return base;
            }
        }
        // A class's own order stands for its func data's.
        if let Some(name) = intrinsic_order(&o.str("code")).and_then(crate::natives::basic::order_name) {
            return name.to_string();
        }
        o.str("Order")
    }

    /// Whether the ability shows a button: Channel (ANcl) only with its Visible option
    /// (DataC 1).
    pub fn ability_visible(&self, a: Id) -> bool {
        self.ability_code(a) != "ANcl" || self.data(a, "DataC", 1) as i32 & 1 != 0
    }

    /// The unit's ability whose order string matches `order`.
    pub fn ability_for_order(&self, h: u32, order: i32) -> Option<Id> {
        if let Some(a) = self.ability_for_intrinsic_order(h, order) {
            return Some(a);
        }
        let name = crate::natives::basic::order_name(order)?;
        let u = self.unit(h)?;
        // Gathering is an action of its own, never a cast.
        u.abilities.iter().map(|(a, _)| *a).find(|&a| {
            self.objects.abilities.get(a).is_some_and(|o| !matches!(o.str("code").as_str(), "Ahar" | "Ahrl" | "Aaha" | "Awha") && (self.order_string(a).eq_ignore_ascii_case(name) || o.str("Orderon").eq_ignore_ascii_case(name)))
        })
    }

    /// How far a unit reaches from its center: its collision, or for a structure half
    /// its pathing footprint (a PathTextures\\NxN image's N cells of 32) when larger.
    pub fn unit_size(&self, u: &Unit) -> f32 {
        if !u.is_building {
            return u.collision;
        }
        let tex = self.objects.units.get(u.type_id).map(|o| o.str("pathTex")).unwrap_or_default();
        let n = tex.rsplit('\\').next().and_then(|f| f.split('x').next()).and_then(|v| v.trim().parse::<f32>().ok()).unwrap_or(0.0);
        u.collision.max(n * 16.0)
    }

    /// How far a widget reaches from its center: a unit's size, or half a
    /// destructable's pathing footprint (pathTex NxN, 32 a cell).
    pub fn widget_size(&self, h: u32) -> f32 {
        match self.handles.get(h) {
            Some(Obj::Unit(u)) => self.unit_size(u),
            Some(Obj::Destructable(d)) => {
                let tex = self.objects.destructables.get(d.type_id).map(|o| o.str("pathTex")).unwrap_or_default();
                tex.rsplit('\\').next().and_then(|f| f.split('x').next()).and_then(|v| v.trim().parse::<f32>().ok()).unwrap_or(0.0) * 16.0
            }
            _ => 0.0,
        }
    }

    /// The unit's ability with `order` built into its code.
    pub fn ability_for_intrinsic_order(&self, h: u32, order: i32) -> Option<Id> {
        let u = self.unit(h)?;
        u.abilities.iter().map(|(a, _)| *a).find(|&a| self.intrinsic_order_of(a) == Some(order))
    }

    /// An ability's built-in order.
    pub fn intrinsic_order_of(&self, a: Id) -> Option<i32> {
        intrinsic_order(&self.ability_code(a))
    }

    /// Movement, attack-speed, armor, and damage factors from a unit's buffs.
    pub fn buff_mods(&self, h: u32) -> (f32, f32, f32, f32, bool, bool) {
        let Some(u) = self.unit(h) else { return (1.0, 1.0, 0.0, 1.0, false, false) };
        let (mut mv, mut atk, mut armor, mut dmg, mut stun, mut inv) = (1.0, 1.0, 0.0, 1.0, false, false);
        for b in &u.buffs {
            if b.move_factor != 0.0 {
                mv *= 1.0 + b.move_factor;
            }
            if b.attack_factor != 0.0 {
                atk *= 1.0 + b.attack_factor;
            }
            armor += b.armor;
            if b.damage_factor != 0.0 {
                dmg *= 1.0 + b.damage_factor;
            }
            stun |= b.stun;
            inv |= b.invulnerable;
        }
        (mv.max(0.0), atk.max(0.0), armor, dmg, stun, inv)
    }

    /// Whether spells last their Hero duration on a unit: Heroes, and units with
    /// Resistant Skin (Arsk), which "reduces the duration of negative spells".
    pub fn hero_duration(&self, h: u32) -> bool {
        self.unit(h).is_some_and(|u| u.is_hero || u.abilities.iter().any(|(a, _)| self.ability_code(*a) == "Arsk"))
    }

    /// Locust (Aloc): a unit nobody selects, targets, or finds in an area.
    pub fn is_locust(&self, u: &Unit) -> bool {
        u.abilities.iter().any(|(a, _)| self.ability_code(*a) == "Aloc")
    }

    /// Invulnerable: SetUnitInvulnerable, or the Invulnerable ability (Avul).
    pub fn unit_invulnerable(&self, u: &Unit) -> bool {
        u.invulnerable || u.abilities.iter().any(|(a, _)| self.ability_code(*a) == "Avul")
    }

    pub fn has_buff_flag(&self, h: u32, f: impl Fn(&Buff) -> bool) -> bool {
        self.unit(h).is_some_and(|u| u.buffs.iter().any(|b| f(b)))
    }

    /// Flat attack bonus from buffs (Avatar) and the life fraction stolen (Vampiric Aura).
    pub fn buff_damage_bonus(&self, h: u32) -> f32 {
        self.unit(h).map_or(0.0, |u| u.buffs.iter().map(|b| b.damage_bonus).sum())
    }

    pub fn is_magic_immune(&self, h: u32) -> bool {
        self.has_buff_flag(h, |b| b.magic_immune) || self.ability_with_code(h, "Amim").is_some() || self.ability_with_code(h, "ACmi").is_some()
    }

    pub fn is_invisible(&self, h: u32) -> bool {
        self.cloaked(h) || self.unit(h).is_some_and(|u| self.permanently_invisible(u))
    }

    /// Hidden by a buff that has finished fading in (Wind Walk, Invisibility).
    pub fn cloaked(&self, h: u32) -> bool {
        self.has_buff_flag(h, |b| b.invisible && b.fade <= 0.0)
    }

    pub fn is_ethereal(&self, h: u32) -> bool {
        // A buff (Banish), or the Ethereal ability itself (Aetl; measured: a footman
        // given it takes nothing from piercing attacks and 1.66 times from magic ones).
        self.has_buff_flag(h, |b| b.ethereal) || self.ability_with_code(h, "Aetl").is_some()
    }

    /// The factor on damage of an attack type to an ethereal unit: its entry in
    /// EtherealDamageBonus (NORMAL, PIERCE, SIEGE, MAGIC, CHAOS, SPELLS, HERO), applied
    /// to damage from an enemy, or from anyone under EtherealDamageBonusAlly
    /// (FUN_6f69b380).  1 for a unit that is not ethereal.
    pub fn ethereal_factor(&self, source: u32, target: u32, attack_type: &str) -> f32 {
        if !self.is_ethereal(target) {
            return 1.0;
        }
        const TYPES: [&str; 7] = ["normal", "pierce", "siege", "magic", "chaos", "spells", "hero"];
        let Some(i) = TYPES.iter().position(|t| t.eq_ignore_ascii_case(attack_type)) else { return 1.0 };
        let enemy = match (self.unit(source), self.unit(target)) {
            (Some(s), Some(t)) => !crate::natives::players::is_ally(self, t.owner, s.owner),
            _ => true,
        };
        if !enemy && self.misc_f32("EtherealDamageBonusAlly", 0.0) == 0.0 {
            return 1.0;
        }
        self.misc.get_list("Misc", "EtherealDamageBonus").get(i).and_then(|v| v.trim().parse().ok()).unwrap_or(1.0)
    }

    /// Damage wakes sleeping units.
    pub fn wake(&mut self, h: u32) {
        if let Some(u) = self.unit_mut(h) {
            u.buffs.retain(|b| !b.sleep && !b.ends_on_damage);
        }
        self.set_asleep(h, false);
    }

    /// Adds or refreshes a buff of the same type from the same ability.
    pub fn apply_buff(&mut self, target: u32, b: Buff) {
        self.apply_buff_tagged(target, b, true);
    }

    /// As `apply_buff`; a new buff takes the original's tags when `tagged` (its own and,
    /// with target art, its effect's: traced as the auras' first scan reaches others;
    /// an aura's bearer took its own as it was made).
    pub fn apply_buff_tagged(&mut self, target: u32, mut b: Buff, tagged: bool) {
        if self.unit(target).is_none_or(|u| u.dead) {
            return;
        }
        // Buff art attaches to the target (the buff's Targetart and Targetattach).
        let art = self.objects.buffs.get(b.id).map(|o| (o.list("Targetart").first().cloned().unwrap_or_default(), o.list("Targetattach").first().cloned().unwrap_or_default()));
        let u = self.unit_mut(target).unwrap();
        // An effect without a buff type (a fountain's) is told apart by its ability.
        if let Some(existing) = u.buffs.iter_mut().find(|x| x.id == b.id && (b.id != [0; 4] || x.ability == b.ability)) {
            existing.remaining = existing.remaining.max(b.remaining);
            if b.aura_left.is_some() {
                existing.aura_left = b.aura_left;
            }
            existing.level = b.level;
            return;
        }
        if tagged && b.id != [0; 4] {
            self.tags.take(&b.id);
            if art.as_ref().is_some_and(|(m, _)| !m.is_empty()) {
                self.tags.take(b"+EIm");
            }
        }
        if let Some((model, attach)) = art {
            if !model.is_empty() {
                let e = self.handles.insert(Obj::Effect(Effect { model, x: 0.0, y: 0.0, z: 0.0, target: Some((target, attach)), destroyed: false, facing: 0.0 }));
                b.effect = e;
            }
        }
        self.unit_mut(target).unwrap().buffs.push(b);
    }

    pub(crate) fn remove_buff_effect(&mut self, e: u32) {
        if e != 0 {
            if let Some(Obj::Effect(x)) = self.handles.get_mut(e) {
                x.destroyed = true;
            }
            self.dying_effects.push((e, self.time));
        }
    }

    /// Ages buffs, applies damage and healing over time, and refreshes auras.
    pub fn update_buffs(&mut self, vm: &mut Vm, dt: f32) {
        let units = self.unit_order.clone();
        let mut expired_effects = Vec::new();
        let mut dots = Vec::new();
        let mut conversions = Vec::new();
        let mut released: Vec<(u32, u32, Id, bool)> = Vec::new();
        let ethereal_heal = self.misc_f32("EtherealHealBonus", 1.66);
        for &h in &units {
            let heal_factor = if self.is_ethereal(h) { ethereal_heal } else { 1.0 };
            let Some(u) = self.unit_mut(h) else { continue };
            if u.dead {
                for b in u.buffs.drain(..) {
                    expired_effects.push(b.effect);
                }
                continue;
            }
            // A paused unit's buffs hold: they neither run out nor act (measured: a
            // paused footman under Rain of Fire keeps its burn buffs and loses no life).
            if u.paused {
                continue;
            }
            for b in &mut u.buffs {
                b.remaining -= dt;
                b.fade -= dt;
                if let Some(l) = b.aura_left.as_mut() {
                    *l -= dt;
                }
                // Strikes once a second end with the buff: none falls as it runs out
                // (measured: Breath of Fire's 5 seconds burn 5 times, the first at once).
                if b.dps != 0.0 && b.dps_every > 0.0 {
                    if b.remaining > 0.0 {
                        b.dps_next -= dt;
                        while b.dps_next <= 0.0 {
                            let f = if b.dps < 0.0 && b.ethereal_heal { heal_factor } else { 1.0 };
                            dots.push((b.caster, h, b.dps * b.dps_every * f));
                            b.dps_next += b.dps_every;
                        }
                    }
                } else if b.dps != 0.0 {
                    dots.push((b.caster, h, b.dps * dt));
                }
                u.mana = (u.mana + (b.mana_regen + b.mana_regen_pct * u.max_mana) * dt).min(u.max_mana);
                let f = if b.ethereal_heal { heal_factor } else { 1.0 };
                u.life = (u.life + (b.life_regen * f + b.life_regen_pct * u.max_life) * dt).min(u.max_life);
            }
            let mut lost_life = 0.0;
            let ended: Vec<(u32, Id, u32, bool)> = u.buffs.iter().filter(|b| b.remaining <= 0.0).map(|b| (b.caster, b.ability, b.level, b.stun)).collect();
            for (caster, a, lvl, stun) in ended {
                conversions.push((h, caster, a, lvl));
                released.push((h, caster, a, stun));
            }
            u.buffs.retain(|b| {
                if b.remaining <= 0.0 {
                    expired_effects.push(b.effect);
                    lost_life += b.life_bonus;
                    false
                } else {
                    true
                }
            });
            if lost_life > 0.0 {
                u.max_life -= lost_life;
                u.life = u.life.min(u.max_life).max(1.0);
            }
            for c in u.cooldowns.values_mut() {
                *c = (*c - dt).max(0.0);
            }
            if let Some(t) = u.timed_life.as_mut().filter(|_| !u.extra.timed_life_paused) {
                *t -= dt;
            }
        }
        for e in expired_effects {
            self.remove_buff_effect(e);
        }
        // Dark Conversion runs its course (CBuffDarkConversion vf41): the sleeper becomes
        // UnitID for the caster's player, the caster raising the summon events
        // (FUN_6f67b0b0).
        // Soul Preservation's end (CBuffSoulPreservation vf41): the held unit becomes the
        // caster's player's, the caster raising the summon events.
        for (h, caster, a, lvl) in conversions {
            let code = self.ability_code(a);
            if !matches!(code.as_str(), "ANdc" | "ANsl") || self.unit(h).is_none_or(|u| u.dead) {
                continue;
            }
            let Some(owner) = self.unit(caster).map(|c| c.owner) else { continue };
            if code == "ANdc" {
                let to = id_from_str(self.objects.abilities.get(a).map(|o| o.level_str("UnitID", lvl)).unwrap_or_default().trim());
                self.change_type(h, to);
            }
            if let Some(u) = self.unit_mut(h) {
                u.owner = owner;
                u.action = Action::Idle;
            }
            use crate::event_ids::*;
            self.queue_unit_event(h, EVENT_PLAYER_UNIT_SUMMON, EVENT_UNIT_SUMMON, EventCtx { other_unit: caster, ..Default::default() });
        }
        // A stun or Entangling Roots running out touches the unit with a hit of nothing
        // (measured: a footman stunned by Storm Bolt for 5 seconds shows a damage event of
        // 0 at their end, one rooted for 9 at theirs; one ensnared none).
        for (h, caster, a, stun) in released {
            if (stun || self.ability_code(a) == "AEer") && self.unit(h).is_some_and(|u| !u.dead) {
                self.spell_damage(vm, caster, h, 0.0);
            }
        }
        for (src, h, amount) in dots {
            if amount >= 0.0 {
                self.spell_damage(vm, src, h, amount);
            } else if let Some(u) = self.unit_mut(h) {
                u.life = (u.life - amount).min(u.max_life);
            }
        }
        // Summons whose timed life ran out die.
        let expired: Vec<u32> = units.iter().copied().filter(|&h| self.unit(h).is_some_and(|u| !u.dead && u.timed_life.is_some_and(|t| t <= 0.0))).collect();
        for h in expired {
            crate::natives::units::kill_unit(self, vm, h, 0);
        }
        self.update_auras(dt);
    }

    /// Whether a toggled ability is on: Immolation while it burns, the others while
    /// their buff lasts.
    pub fn toggle_active(&self, h: u32, a: Id) -> bool {
        if self.ability_code(a) == "AEim" {
            return self.ground_effects.iter().any(|e| e.caster == h && e.ability == a && e.elapsed < e.duration);
        }
        self.unit(h).is_some_and(|u| u.buffs.iter().any(|b| b.ability == a))
    }

    /// Whether a toggled ability that is on may be turned off now: all but Divine Shield,
    /// which only under CanDeactivateDivineShield (CAbilityDivineShield::vf198).
    pub fn toggle_can_end(&self, a: Id) -> bool {
        self.ability_code(a) != "AHds" || self.misc_f32("CanDeactivateDivineShield", 0.0) != 0.0
    }

    /// An off order (an ability's Unorder) for a toggled ability that is on: ends it.
    pub fn toggle_off_order(&mut self, h: u32, name: &str) -> bool {
        let Some(u) = self.unit(h) else { return false };
        let found = u.abilities.iter().map(|&(a, _)| a).find(|&a| {
            self.objects.abilities.get(a).is_some_and(|o| o.str("Unorder").eq_ignore_ascii_case(name)) && matches!(self.ability_code(a).as_str(), "AEim" | "AHds" | "Amfl" | "ANth" | "Amdf")
        });
        let Some(a) = found else { return false };
        if !self.toggle_active(h, a) || !self.toggle_can_end(a) {
            return true;
        }
        if self.ability_code(a) == "AEim" {
            for e in self.ground_effects.iter_mut().filter(|e| e.caster == h && e.ability == a) {
                e.elapsed = e.duration;
            }
        }
        if let Some(u) = self.unit_mut(h) {
            u.buffs.retain(|b| b.ability != a);
            if u.channel.as_ref().is_some_and(|c| c.ability == a) {
                u.channel = None;
            }
        }
        true
    }

    /// Whether a unit's auras reach others, under MiscGame's rules: illusions, invisible
    /// units, polymorphed ones, burrowed ones, animated dead, and flying heroes bestow
    /// their auras only where IllusionsBestowAuras (and the rest) say so.
    fn bestows_auras(&self, h: u32) -> bool {
        let Some(u) = self.unit(h) else { return false };
        let allowed = |key: &str| self.misc_f32(key, 1.0) != 0.0;
        let polymorphed = u.buffs.iter().any(|b| matches!(self.ability_code(b.ability).as_str(), "Aply" | "AOhx"));
        let burrowed = u.abilities.iter().any(|&(a, l)| self.ability_code(a) == "Abur" && self.objects.abilities.get(a).is_some_and(|o| crate::objects::id_from_str(o.level_str("UnitID", l.max(1) as u32).trim()) == u.type_id));
        let flying_hero = u.is_hero && self.move_kind(h) == crate::pathing::Move::Fly;
        !(u.extra.illusion.is_some() && !allowed("IllusionsBestowAuras")
            || self.is_invisible(h) && !allowed("InvisibleUnitsBestowAuras")
            || polymorphed && !allowed("PolymorphedUnitsBestowAuras")
            || burrowed && !allowed("BurrowedUnitsBestowAuras")
            || u.extra.animated && !allowed("AnimatedUnitsBestowAuras")
            || flying_hero && !allowed("FlyingHeroesBestowAuras"))
    }

    /// Auras give their buff to qualifying units in range, refreshed every update.
    /// Auras, as the original runs them (measured with Devotion Aura): each bearer
    /// scans every half second from when it began to bear one, the first scan half a
    /// second in, giving those in range its buff for 2 seconds; every 2 seconds it
    /// drops the buffs it gave whose 2 seconds have run out (a footman leaving at 6.13,
    /// 6.38, or 6.63 seconds loses the buff at 10.01).
    pub(crate) fn update_auras(&mut self, dt: f32) {
        let mut grants = Vec::new();
        let mut sweeps = Vec::new();
        let bearers: Vec<u32> = self
            .unit_order
            .iter()
            .copied()
            .filter(|&h| self.aura_sources(h).iter().any(|(a, _)| matches!(self.ability_code(*a).as_str(), "AHad" | "AHab" | "AUau" | "AOae" | "AEar" | "AUav" | "AEah" | "Aoar" | "Aarm" | "Aakb" | "AOac" | "Aasl")))
            .collect();
        let mut scanning = crate::collections::HashSet::default();
        // A new bearer has its own aura at once (measured: an Archmage that learns
        // Brilliance Aura regains its 0.75 a second from then, 17.00 mana in 10 seconds
        // with its 0.95 by intelligence; those near it from the first scan, 13.79 for a
        // Sorceress).
        let mut fresh = crate::collections::HashSet::default();
        for h in bearers {
            let u = self.unit_mut(h).unwrap();
            let prev = u.extra.aura_clock;
            let now = prev.map_or(0.0, |c| c + dt);
            u.extra.aura_clock = Some(now);
            let Some(prev) = prev else {
                fresh.insert(h);
                continue;
            };
            if (now / 0.5).floor() > (prev / 0.5).floor() {
                scanning.insert(h);
            }
            if (now / 2.0).floor() > (prev / 2.0).floor() {
                sweeps.push(h);
            }
        }
        for &h in &self.unit_order {
            let Some(u) = self.unit(h) else { continue };
            if u.dead {
                continue;
            }
            let bestows = self.bestows_auras(h);
            for (a, lvl) in self.aura_sources(h) {
                let code = self.ability_code(a);
                let lvl = lvl.max(1) as u32;
                let buff = self.objects.abilities.get(a).map(|o| o.level_str("BuffID", lvl)).unwrap_or_default();
                let mut b = Buff { id: id_from_str(buff.trim()), ability: a, level: lvl, caster: h, remaining: 0.5, ..Default::default() };
                match code.as_str() {
                    // Devotion Aura: armor bonus (DataA).
                    "AHad" => b.armor = self.data(a, "DataA", lvl),
                    // Brilliance Aura: mana regeneration (DataA).
                    "AHab" => b.mana_regen = self.data(a, "DataA", lvl),
                    // Unholy Aura: movement speed factor (DataA) and life regeneration (DataB).
                    "AUau" => {
                        b.move_factor = self.data(a, "DataA", lvl);
                        b.life_regen = self.data(a, "DataB", lvl);
                    }
                    // Endurance Aura: movement (DataA) and attack speed (DataB) factors.
                    "AOae" => {
                        b.move_factor = self.data(a, "DataA", lvl);
                        b.attack_factor = self.data(a, "DataB", lvl);
                    }
                    // Trueshot Aura: damage bonus factor (DataA).
                    "AEar" => b.damage_factor = self.data(a, "DataA", lvl),
                    // Vampiric Aura: a share of attack damage returns as life (DataA).
                    "AUav" => b.life_steal = self.data(a, "DataA", lvl),
                    // Thorns Aura: melee attackers take damage (DataA, a fraction when DataB).
                    "AEah" => {
                        b.return_damage = self.data(a, "DataA", lvl);
                        b.return_pct = self.data(a, "DataB", lvl) != 0.0;
                    }
                    // Regeneration (a healing ward): DataA life a second, of maximum life
                    // when DataB.
                    "Aoar" => {
                        if self.data(a, "DataB", lvl) != 0.0 {
                            b.life_regen_pct = self.data(a, "DataA", lvl);
                        } else {
                            b.life_regen = self.data(a, "DataA", lvl);
                        }
                    }
                    // Neutral Regen (mana only, a fountain of mana): DataA mana a second, of
                    // maximum mana when DataB.
                    "Aarm" => {
                        if self.data(a, "DataB", lvl) != 0.0 {
                            b.mana_regen_pct = self.data(a, "DataA", lvl);
                        } else {
                            b.mana_regen = self.data(a, "DataA", lvl);
                        }
                    }
                    // Engineering Upgrade: its bearer moves DataA faster and deals DataB
                    // more damage (BNeg, on itself only).
                    "ANeg" => {
                        b.move_factor = self.data(a, "DataA", lvl);
                        b.damage_bonus = self.data(a, "DataB", lvl);
                    }
                    // War Drums and Command Aura: attack damage increase (DataA).
                    "Aakb" | "AOac" => b.damage_factor = self.data(a, "DataA", lvl) + self.upgrade_ability_bonus(u.owner, u.type_id, "raud", &code),
                    // Slow Aura (a tornado): enemies move (DataA) and attack (DataB) slower.
                    "Aasl" => {
                        b.move_factor = self.data(a, "DataA", lvl);
                        b.attack_factor = self.data(a, "DataB", lvl);
                    }
                    _ => continue,
                }
                // Engineering Upgrade is its bearer's own; the rest are auras.
                if code != "ANeg" && !bestows {
                    continue;
                }
                let area = if code == "ANeg" { 0.0 } else { self.data(a, "Area", lvl) };
                // Whom it reaches: enemies (a slow aura), allies, or, for a regeneration
                // whose targets name no side (a fountain), everyone.
                let targs = self.objects.abilities.get(a).map(|o| o.level_str("targs", lvl).to_ascii_lowercase()).unwrap_or_default();
                let sided = ["friend", "allies", "player", "enemy", "neutral"].iter().any(|s| targs.split(',').any(|t| t.trim() == *s));
                let reach = if code == "Aasl" {
                    Some(true)
                } else if matches!(code.as_str(), "Aoar" | "Aarm") && !sided {
                    None
                } else {
                    Some(false)
                };
                let mut area = area;
                if code != "ANeg" {
                    if fresh.contains(&h) {
                        area = 0.0;
                    } else if !scanning.contains(&h) {
                        continue;
                    }
                    b.remaining = f32::INFINITY;
                    b.aura_left = Some(2.0);
                }
                grants.push((h, u.owner, u.x, u.y, area, b, reach));
            }
        }
        for (src, owner, x, y, area, b, reach) in grants {
            let targets: Vec<u32> = self
                .unit_order
                .iter()
                .copied()
                .filter(|&t| {
                    // Without an area (Engineering Upgrade) only the bearer.
                    (area > 0.0 || t == src) && self.unit(t).is_some_and(|u| !u.dead && !u.is_building && (u.x - x).hypot(u.y - y) <= area && reach.is_none_or(|enemies| crate::natives::players::is_ally(self, owner, u.owner) != enemies))
                })
                .collect();
            for t in targets {
                self.apply_buff_tagged(t, b.clone(), t != src || area > 0.0 && false);
            }
        }
        // The sweeps, and the buffs of bearers that are gone once lapsed.
        let lapsed: Vec<(u32, u32)> = self
            .unit_order
            .iter()
            .flat_map(|&t| self.unit(t).into_iter().flat_map(move |u| u.buffs.iter().map(move |b| (t, b))))
            .filter(|(_, b)| b.aura_left.is_some_and(|l| l < 0.0) && (sweeps.contains(&b.caster) || self.unit(b.caster).is_none()))
            .map(|(t, b)| (t, b.effect))
            .collect();
        for (t, effect) in lapsed {
            if let Some(u) = self.unit_mut(t) {
                u.buffs.retain(|b| !(b.effect == effect && b.aura_left.is_some_and(|l| l < 0.0)));
            }
            if effect != 0 {
                self.remove_buff_effect(effect);
            }
        }
    }

    /// A unit's abilities and those of the items it carries, at their first level: an
    /// item's aura is its bearer's (measured: OrcX03a's Rexxar holds BHad, BHab, and
    /// BOac in the original, from the Devotion, Brilliance, and Command Aura of the
    /// heroes' blba, mnsf, and shhn; ours gave none).
    fn aura_sources(&self, h: u32) -> Vec<(Id, i32)> {
        let Some(u) = self.unit(h) else { return Vec::new() };
        let items = u.inventory.iter().filter_map(|&i| self.item(i)).flat_map(|i| self.item_abilities(i.type_id)).map(|a| (a, 1));
        u.abilities.iter().copied().chain(items).collect()
    }

    /// Spell damage: the DamageBonusSpells table applies, armor value does not.
    pub fn spell_damage(&mut self, vm: &mut Vm, source: u32, target: u32, amount: f32) {
        let Some(t) = self.unit(target) else { return };
        if t.dead || self.unit_invulnerable(t) || self.buff_mods(target).5 {
            return;
        }
        // Magic immune units ignore spell damage (MagicImmunesResistDamage).
        if self.is_magic_immune(target) && self.misc_f32("MagicImmunesResistDamage", 1.0) != 0.0 {
            return;
        }
        let def_type = self.defense_type(t);
        let col = crate::upgrades::DEFENSE_TYPES.iter().position(|c| *c == def_type).unwrap_or(7);
        let mult: f32 = self.misc.get_list("Misc", "DamageBonusSpells").get(col).and_then(|v| v.trim().parse().ok()).unwrap_or(1.0);
        let ethereal = self.ethereal_factor(source, target, "spells");
        let reduction = self.unit(target).map_or(0.0, |u| u.item_bonus.spell_reduction).clamp(0.0, 1.0);
        let dmg = self.illusion_damage(source, target, self.elunes_grace(target, "spells", amount * mult * ethereal)) * (1.0 - reduction);
        let owner = t.owner;
        // A spell shows its caster to the target's owner (FUN_6f67fde0).
        self.reveal_attacker(source, target, 0.0);
        self.wake(target);
        let linked_dead = self.take_life(target, dmg);
        let dead = self.unit(target).is_some_and(|t| t.life <= 0.405);
        if !dead {
            self.phase_shift(target);
        }
        for d in linked_dead {
            crate::natives::units::kill_unit(self, vm, d, source);
        }
        let ctx = EventCtx { unit: target, other_unit: source, damage: dmg, event_id: EVENT_UNIT_DAMAGED, player: owner, has_player: true, ..Default::default() };
        self.fire(vm, ctx, |_, k| matches!(k, EventKind::Unit { unit, event } if *unit == target && *event == EVENT_UNIT_DAMAGED));
        if !dead && source != target {
            if !self.ai_flees(target, source) {
                self.retaliate(target, source, true);
            }
        }
        if dead {
            crate::natives::units::kill_unit(self, vm, target, source);
        }
    }

    /// A magical heal: EtherealHealBonus times as much on an ethereal unit, as Holy
    /// Light, Heal, Death Coil, Healing Wave, Healing Spray, and Tranquility give
    /// (their abilities' vf267, FUN_6f5d7700, FUN_6f54d8a0); healing items do not.
    pub(crate) fn magic_heal(&mut self, target: u32, amount: f32) {
        let f = if self.is_ethereal(target) { self.misc_f32("EtherealHealBonus", 1.66) } else { 1.0 };
        self.heal(target, amount * f);
    }

    pub(crate) fn heal(&mut self, target: u32, amount: f32) {
        if let Some(u) = self.unit_mut(target) {
            if !u.dead {
                u.life = (u.life + amount).min(u.max_life);
            }
        }
    }

    pub(crate) fn spawn_effect(&mut self, model: &str, x: f32, y: f32) {
        if model.is_empty() {
            return;
        }
        let z = self.ground_z(x, y);
        let e = self.handles.insert(Obj::Effect(Effect { model: model.to_string(), x, y, z, target: None, destroyed: true, facing: 0.0 }));
        self.dying_effects.push((e, self.time));
    }

    pub(crate) fn profile_art(&self, id: Id, key: &str) -> String {
        self.objects.abilities.get(id).and_then(|o| o.list(key).first().cloned()).unwrap_or_default()
    }

    pub(crate) fn is_undead(&self, h: u32) -> bool {
        self.unit(h).and_then(|u| self.objects.units.get(u.type_id)).is_some_and(|o| o.str("type").to_ascii_lowercase().split(',').any(|c| c.trim() == "undead"))
    }

    pub(crate) fn enemies_in(&self, owner: u8, x: f32, y: f32, r: f32) -> Vec<u32> {
        self.unit_order
            .iter()
            .copied()
            .filter(|&h| self.unit(h).is_some_and(|u| !u.dead && !u.hidden && (u.x - x).hypot(u.y - y) <= r && !crate::natives::players::is_ally(self, owner, u.owner)))
            .collect()
    }

    /// Checks mana and cooldown; returns the interface error key when the cast cannot start.
    pub fn can_cast(&self, h: u32, a: Id) -> Result<(), &'static str> {
        let Some(u) = self.unit(h) else { return Err("") };
        let lvl = self.ability_level(h, a);
        if lvl == 0 {
            return Err("");
        }
        if u.cooldowns.get(&a).copied().unwrap_or(0.0) > 0.0 {
            return Err("Cooldown");
        }
        if u.mana < self.data(a, "Cost", lvl) && !self.cheat(crate::cheats::flag::MANA) {
            return Err("Nomana");
        }
        if u.buffs.iter().any(|b| b.silence) {
            return Err("");
        }
        // Shadowmeld works only at night (measured: by day the original shows its
        // button dimmed, Tyrande's in NightElf02's opening and a new one's at noon).
        if self.ability_code(a) == "Ashm" && (6.0..18.0).contains(&self.time_of_day) {
            return Err("");
        }
        Ok(())
    }

    /// One step of a cast order: approach, face, wait for the cast point, then fire.
    pub fn cast_step(&mut self, vm: &mut Vm, h: u32, a: Id, target: u32, x: f32, y: f32, dt: f32) {
        let lvl = self.ability_level(h, a);
        if lvl == 0 || self.can_cast(h, a).is_err() && self.unit(h).is_some_and(|u| u.casting.is_none()) {
            if let Some(u) = self.unit_mut(h) {
                u.action = Action::Idle;
            }
            return;
        }
        let range = self.data(a, "Rng", lvl);
        let goal = if target != 0 { self.widget_pos(target) } else { Some((x, y)) };
        let Some(goal) = goal else {
            let u = self.unit_mut(h).unwrap();
            u.action = Action::Idle;
            u.casting = None;
            return;
        };
        // A dead target ends the cast, unless the ability is for corpses.
        let for_corpses = self.objects.abilities.get(a).is_some_and(|o| o.level_str("targs", lvl).split(',').any(|t| t.trim().eq_ignore_ascii_case("dead")));
        if target != 0 && self.unit(target).is_some_and(|t| t.dead != for_corpses || t.hidden && for_corpses) {
            self.unit_mut(h).unwrap().action = Action::Idle;
            return;
        }
        let target_size = if target != 0 { self.widget_size(target) } else { 0.0 };
        let Some(u) = self.unit(h) else { return };
        let began = u.casting.is_none();
        if began {
            // Range runs between the caster's and the target unit's edges.
            let d = (goal.0 - u.x).hypot(goal.1 - u.y) - u.collision - target_size;
            // One without a target goes to a unit given it all the same (Cannibalize's
            // corpse, as a computer's ghoul does).
            if (self.targeting(a) != Targeting::None || target != 0) && d > range {
                if u.path.is_empty() || u.repath_timer <= 0.0 {
                    self.path_to(h, goal);
                }
                self.walk(h, dt);
                return;
            }
            let u = self.unit_mut(h).unwrap();
            u.path.clear();
            // An item's spell goes off without the holder turning (measured: a Staff of
            // Teleportation used toward the hero's back takes 3.06 seconds, its 3 second
            // delay and no more).
            let from_item = u.extra.casting_item.is_some();
            if self.targeting(a) != Targeting::None && !from_item {
                let u = self.unit_mut(h).unwrap();
                let want = crate::sim::angle_to((u.x, u.y), goal);
                let left = crate::sim::turn(u, want, dt);
                if left.abs() > 15.0 {
                    return;
                }
            }
            let anim = self.objects.abilities.get(a).map(|o| o.str("Animnames").replace(',', " ")).unwrap_or_default();
            let u = self.unit_mut(h).unwrap();
            u.casting = Some((0.0, false));
            u.anim = if anim.is_empty() { "spell".into() } else { anim };
            u.anim_serial = u.anim_serial.wrapping_add(1);
            // The cast begins: channel, then cast.
            let ctx = EventCtx { target_unit: target, spell_ability: crate::objects::id_to_int(a), order_x: goal.0, order_y: goal.1, ..Default::default() };
            self.fire_unit_pair(vm, h, EVENT_PLAYER_UNIT_SPELL_CHANNEL, EVENT_UNIT_SPELL_CHANNEL, ctx.clone());
            self.fire_unit_pair(vm, h, EVENT_PLAYER_UNIT_SPELL_CAST, EVENT_UNIT_SPELL_CAST, ctx);
            // It goes off its cast point after it begins, in that step if that is none
            // (measured: a Mountain King's Storm Bolt, cast point 0.4, begins as ordered
            // and goes off 0.40 seconds after; Wind Walk's buff shows 0.02 after).
        }
        let Some(u) = self.unit(h) else { return };
        let Some((elapsed, fired)) = u.casting else { return };
        // Begun, a cast reaches SpellCastRangeBuffer further; a target that gets
        // beyond that before the spell goes off is followed again (FUN_6f415270).
        if !fired && target != 0 && self.targeting(a) != Targeting::None {
            let d = (goal.0 - u.x).hypot(goal.1 - u.y) - u.collision - target_size;
            if d > range + self.misc_f32("SpellCastRangeBuffer", 300.0) {
                let u = self.unit_mut(h).unwrap();
                u.casting = None;
                crate::sim::set_anim(u, "stand");
                return;
            }
        }
        let Some(u) = self.unit(h) else { return };
        let elapsed = if began { elapsed } else { elapsed + dt };
        let o = self.objects.units.get(u.type_id);
        let (castpt, backsw) = o.map_or((0.3, 0.5), |o| (o.real("castpt"), o.real("castbsw")));
        // Nor does it wait for the holder's cast point.
        let (castpt, backsw) = if u.extra.casting_item.is_some() { (0.0, 0.0) } else { (castpt, backsw) };
        // Possession takes hold its casting time (Cast) after the cast point (measured:
        // a ghost's touches a footman 1.73 seconds after the order, a short walk, its
        // cast point 0.5, and Cast 1 later; ours, without, 0.70).
        let castpt = if self.ability_code(a) == "Apos" { castpt + self.data(a, "Cast", lvl) } else { castpt };
        // Bladestorm, Immolation, and Wind Walk begin at once, without a backswing
        // (measured: Bladestorm's first strike 0.20 seconds after the order, its
        // interval, Immolation's 0.02, and Wind Walk's buff 0.02, though their heroes'
        // cast point is 0.3; each hero's first attack lands 0.35 seconds in, its damage
        // point).
        let instant = matches!(self.ability_code(a).as_str(), "AOww" | "AEim" | "AOwk");
        let (castpt, backsw) = if instant { (0.0, 0.0) } else { (castpt, backsw) };
        // Steps of f32 fall short of whole times (twenty 0.02s make 0.39999998).
        if !fired && elapsed >= castpt - 1e-4 {
            // thereisnospoon and thedudeabides.
            let cost = if self.cheat(crate::cheats::flag::MANA) { 0.0 } else { self.data(a, "Cost", lvl) };
            let cool = if self.cheat(crate::cheats::flag::COOLDOWN) { 0.0 } else { self.data(a, "Cool", lvl) };
            let u = self.unit_mut(h).unwrap();
            u.mana -= cost;
            u.cooldowns.insert(a, cool);
            u.casting = Some((elapsed, true));
            let ctx = EventCtx { target_unit: target, spell_ability: crate::objects::id_to_int(a), order_x: goal.0, order_y: goal.1, ..Default::default() };
            self.fire_unit_pair(vm, h, EVENT_PLAYER_UNIT_SPELL_EFFECT, EVENT_UNIT_SPELL_EFFECT, ctx);
            if cost > 0.0 {
                self.mana_flare(vm, h, cost);
            }
            // CasterArt at the caster and TargetArt at the target, as the ability lists.
            let (cx, cy) = self.unit(h).map_or(goal, |u| (u.x, u.y));
            let caster_art = self.profile_art(a, "CasterArt");
            self.spawn_effect(&caster_art, cx, cy);
            if target != 0 {
                let target_art = self.profile_art(a, "TargetArt");
                self.spawn_effect(&target_art, goal.0, goal.1);
            }
            // An item's spell does what the item does (a Wand of Illusion's image),
            // else what the ability does as a unit's.
            let item = self.unit_mut(h).and_then(|u| u.extra.casting_item.take());
            let item_type = item.and_then(|it| self.item(it).map(|i| i.type_id));
            let handled = match (item, item_type) {
                (Some(it), Some(ty)) => self.item_effect(vm, h, it, ty, target, goal.0, goal.1),
                _ => false,
            };
            if !handled {
                self.ability_effect(vm, h, a, lvl, target, goal);
            }
            if let (Some(it), Some(ty)) = (item, item_type) {
                self.spend_item_use(vm, h, it, ty);
            }
            return;
        }
        let u = self.unit_mut(h).unwrap();
        // A channeled spell holds the caster until it ends (update_channels).
        if u.channel.is_some() {
            u.casting = Some((elapsed, fired));
            return;
        }
        if fired && elapsed >= castpt + backsw - 1e-4 {
            u.casting = None;
            // An autocast resumes what it interrupted; otherwise the order is done
            // (measured: a Paladin that has cast Divine Shield shows Stop lit, as an
            // idle unit does).
            match u.extra.resume.take() {
                Some(r) => u.action = r,
                None => {
                    u.action = Action::Idle;
                    u.order = Order::default();
                }
            }
            let ctx = EventCtx { target_unit: target, spell_ability: crate::objects::id_to_int(a), order_x: goal.0, order_y: goal.1, ..Default::default() };
            self.queue_unit_event(h, EVENT_PLAYER_UNIT_SPELL_FINISH, EVENT_UNIT_SPELL_FINISH, ctx.clone());
            self.queue_unit_event(h, EVENT_PLAYER_UNIT_SPELL_ENDCAST, EVENT_UNIT_SPELL_ENDCAST, ctx);
            let u = self.unit_mut(h).unwrap();
            u.anim = "stand".into();
            u.anim_serial = u.anim_serial.wrapping_add(1);
        } else {
            u.casting = Some((elapsed, fired));
        }
    }

    /// What each ability code does when cast.
    /// An ability's effect without a cast (an orb's effect ability).
    pub(crate) fn ability_effect_pub(&mut self, vm: &mut Vm, h: u32, a: Id, lvl: u32, target: u32, at: (f32, f32)) {
        self.ability_effect(vm, h, a, lvl, target, at);
    }

    /// Spell Shield (ANss, an item's): a spell an enemy aims at the bearer is spent
    /// on the shield instead, which then rests for its Cool and shows its Casterart.
    fn spell_shield_blocks(&mut self, caster: u32, target: u32) -> bool {
        let Some(t) = self.unit(target) else { return false };
        let enemy = self.unit(caster).is_some_and(|c| !crate::natives::players::is_ally(self, t.owner, c.owner));
        if !enemy || caster == target {
            return false;
        }
        let shield = t.inventory.iter().filter_map(|&i| self.item(i)).flat_map(|i| self.item_abilities(i.type_id)).find(|&a| self.ability_code(a) == "ANss");
        let Some(a) = shield else { return false };
        if self.unit(target).is_some_and(|u| u.cooldowns.get(&a).copied().unwrap_or(0.0) > 0.0) {
            return false;
        }
        let cool = self.data(a, "Cool", 1);
        self.unit_mut(target).unwrap().cooldowns.insert(a, cool);
        let (model, attach) = (self.profile_art(a, "Casterart"), self.profile_art(a, "Casterattach"));
        if !model.is_empty() {
            // Shown for a second, its stand's length.
            let e = self.handles.insert(Obj::Effect(Effect { model, x: 0.0, y: 0.0, z: 0.0, target: Some((target, attach)), destroyed: false, facing: 0.0 }));
            self.effect_destroys.push((e, self.time + 1.0));
        }
        true
    }

    /// A spell's missile from the caster to its target unit (or point), whose effect
    /// takes hold as it lands (spell_landed).
    fn launch_spell_missile(&mut self, h: u32, a: Id, lvl: u32, target: u32, at: (f32, f32)) {
        let Some((sx, sy)) = self.unit(h).map(|u| (u.x, u.y)) else { return };
        let model = self.profile_art(a, "Missileart");
        let speed = self.profile_art(a, "Missilespeed").trim().parse().unwrap_or(1000.0f32).max(1.0);
        let arc = self.profile_art(a, "Missilearc").trim().parse().unwrap_or(0.0);
        let z = self.ground_z(sx, sy) + 60.0;
        let target = if self.unit(target).is_some() { target } else { 0 };
        self.missiles.push(crate::sim::Missile {
            id: 0,
            source: h,
            target,
            model,
            x: sx,
            y: sy,
            z,
            speed,
            arc,
            damage: 0.0,
            attack_type: String::new(),
            start: (sx, sy),
            spell: Some((a, lvl, 0.0, [0; 4])),
            weapon: None,
            impact: (target == 0).then_some(at),
            bounces: 0,
            hit: Vec::new(),
            stampede: None,
            rock: None,
            reflected: false,
        });
    }

    /// A spell's missile lands: its effect takes hold, the target touched with a hit of
    /// nothing as it does, and again after for those that lay a lasting hold (measured:
    /// Shadow Strike shows damage events of 0, 75, and 0 at once; Drunken Haze, Ensnare,
    /// Acid Bomb, and Parasite two of 0; Storm Bolt and Firebolt 0 and their damage only,
    /// Transmute one of 0).
    pub(crate) fn spell_landed(&mut self, vm: &mut Vm, h: u32, a: Id, lvl: u32, target: u32, at: (f32, f32)) {
        self.spell_landing = true;
        self.ability_effect(vm, h, a, lvl, target, at);
        self.spell_landing = false;
        let enemy = self.unit(target).is_some_and(|t| !t.dead && self.unit(h).is_some_and(|c| !crate::natives::players::is_ally(self, t.owner, c.owner)));
        if enemy && !matches!(self.ability_code(a).as_str(), "ACtb" | "ANfb" | "ANtm") {
            self.spell_damage(vm, h, target, 0.0);
        }
    }

    fn ability_effect(&mut self, vm: &mut Vm, h: u32, a: Id, lvl: u32, target: u32, (x, y): (f32, f32)) {
        let code = self.ability_code(a);
        // These fly to their target (Missileart, Missilespeed, Missilearc) and take hold
        // as they land (measured on footmen about 300 away: a Brewmaster's Drunken Haze
        // lands 0.21 seconds after its cast point, a Warden's Shadow Strike 0.25, and a
        // Raider's Ensnare, an Alchemist's Acid Bomb, a granite golem's Storm Bolt, and a
        // salamander lord's Firebolt at 0.81, 0.83, 0.73, and 0.72 seconds after the order,
        // Parasite at 0.26 and 0.43, and an Alchemist's Transmute at 0.83).
        if spell_flies(&code) && !self.spell_landing {
            self.launch_spell_missile(h, a, lvl, target, (x, y));
            return;
        }
        if target != 0 && self.unit(target).is_some() && self.spell_shield_blocks(h, target) {
            return;
        }
        // A spell aimed at an enemy unit touches it with nothing first (measured: Sleep,
        // Entangling Roots, Slow, Faerie Fire, Hex, Polymorph, Curse, and Frost Nova each
        // give their target a damage event of 0 as they take hold; a missile's, as Storm
        // Bolt's and Death Coil's, when it lands; Chain Lightning gives none).
        let enemy_target = target != 0 && self.unit(target).is_some_and(|t| self.unit(h).is_some_and(|c| !crate::natives::players::is_ally(self, t.owner, c.owner)));
        if enemy_target && !matches!(code.as_str(), "AOcl" | "AHtb" | "AUdc") {
            self.spell_damage(vm, h, target, 0.0);
        }
        let owner = self.unit(h).map_or(0, |u| u.owner);
        let dur = if self.hero_duration(target) { self.data(a, "HeroDur", lvl) } else { self.data(a, "Dur", lvl) };
        let buff_id = id_from_str(self.objects.abilities.get(a).map(|o| o.level_str("BuffID", lvl)).unwrap_or_default().trim());
        let area = self.data(a, "Area", lvl);
        match code.as_str() {
            // Entangle Gold Mine: the mine is entangled after the casting time.
            "Aent" => {
                let cast = self.data(a, "Cast", lvl);
                self.pending_entangles.push((self.time + cast, h, target));
            }
            // Storm Bolt: a missile that deals DataA damage and stuns.
            "AHtb" => {
                let model = self.profile_art(a, "Missileart");
                let speed = self.profile_art(a, "Missilespeed").parse().unwrap_or(1000.0);
                let src = self.unit(h).unwrap();
                let (sx, sy) = (src.x, src.y);
                let z = self.ground_z(sx, sy) + 60.0;
                self.missiles.push(crate::sim::Missile {
                    id: 0,
                    source: h,
                    target,
                    model,
                    x: sx,
                    y: sy,
                    z,
                    speed,
                    arc: 0.0,
                    damage: self.data(a, "DataA", lvl),
                    attack_type: String::new(),
                    start: (sx, sy),
                    spell: Some((a, lvl, dur, buff_id)),
                    weapon: None,
                    impact: None,
                    bounces: 0,
                    hit: Vec::new(),
                    stampede: None,
                    rock: None,
                    reflected: false,
                });
            }
            // Thunder Clap: area damage (DataA) and slow (DataC movement, DataD attack).
            "AHtc" | "AOws" => {
                let (cx, cy) = self.unit(h).map_or((x, y), |u| (u.x, u.y));
                let dmg = self.data(a, "DataA", lvl);
                for t in self.enemies_in(owner, cx, cy, area) {
                    self.spell_damage(vm, h, t, dmg);
                    let d = if self.hero_duration(t) { self.data(a, "HeroDur", lvl) } else { self.data(a, "Dur", lvl) };
                    let mut b = Buff { id: buff_id, ability: a, level: lvl, caster: h, remaining: d, ..Default::default() };
                    if code == "AOws" {
                        b.stun = true;
                    } else {
                        b.move_factor = -self.data(a, "DataC", lvl);
                        b.attack_factor = -self.data(a, "DataD", lvl);
                    }
                    self.apply_buff(t, b);
                }
                let art = self.profile_art(a, "CasterArt");
                self.spawn_effect(&art, cx, cy);
            }
            // Holy Light: heals living allies (DataA) or damages undead enemies (half).
            "AHhb" => {
                let amount = self.data(a, "DataA", lvl);
                if self.is_undead(target) {
                    self.spell_damage(vm, h, target, amount / 2.0);
                } else {
                    self.magic_heal(target, amount);
                }
                let art = self.profile_art(a, "TargetArt");
                if let Some((tx, ty)) = self.widget_pos(target) {
                    self.spawn_effect(&art, tx, ty);
                }
            }
            // Divine Shield: invulnerable for the duration.
            "AHds" => {
                let d = self.data(a, "Dur", lvl).max(self.data(a, "HeroDur", lvl));
                self.apply_buff(h, Buff { id: buff_id, ability: a, level: lvl, caster: h, remaining: d, invulnerable: true, ..Default::default() });
            }
            // Heal: restores DataA hit points.
            "Ahea" | "Anhe" => self.magic_heal(target, self.data(a, "DataA", lvl)),
            // Death Coil: heals undead allies (DataA) or damages living enemies (half).
            // Death Coil: a missile (Missilespeed) that heals an undead target DataA or
            // harms another half that as it lands (measured: 100 to a footman 0.67
            // seconds after the order, with its touch).
            "AUdc" => {
                let model = self.profile_art(a, "Missileart");
                let speed = self.profile_art(a, "Missilespeed").parse().unwrap_or(1100.0);
                let src = self.unit(h).unwrap();
                let (sx, sy) = (src.x, src.y);
                let z = self.ground_z(sx, sy) + 60.0;
                self.missiles.push(crate::sim::Missile {
                    id: 0,
                    source: h,
                    target,
                    model,
                    x: sx,
                    y: sy,
                    z,
                    speed,
                    arc: 0.0,
                    damage: self.data(a, "DataA", lvl),
                    attack_type: String::new(),
                    start: (sx, sy),
                    spell: Some((a, lvl, 0.0, [0; 4])),
                    weapon: None,
                    impact: None,
                    bounces: 0,
                    hit: Vec::new(),
                    stampede: None,
                    rock: None,
                    reflected: false,
                });
            }
            // Frost Nova: target damage (DataB), area damage (DataA), and a slow.
            // Frost Nova: a touch of nothing on the target, then DataA to each enemy in
            // the area, then DataB to the target (measured: the target's damage events
            // read 0, then 50 with its neighbor's, then 100).
            "AUfn" => {
                let (tx, ty) = self.widget_pos(target).unwrap_or((x, y));
                for t in self.enemies_in(owner, tx, ty, area) {
                    self.spell_damage(vm, h, t, self.data(a, "DataA", lvl));
                    let d = if self.hero_duration(t) { self.data(a, "HeroDur", lvl) } else { self.data(a, "Dur", lvl) };
                    self.apply_buff(t, Buff { id: buff_id, ability: a, level: lvl, caster: h, remaining: d, move_factor: -0.5, attack_factor: -0.25, ..Default::default() });
                }
                self.spell_damage(vm, h, target, self.data(a, "DataB", lvl));
            }
            // Slow: movement (DataA) and attack speed (DataB) factors.
            "Aslo" => self.apply_buff(target, Buff { id: buff_id, ability: a, level: lvl, caster: h, remaining: dur, move_factor: -self.data(a, "DataA", lvl), attack_factor: -self.data(a, "DataB", lvl), ..Default::default() }),
            // Bloodlust: attack speed (DataA) and movement (DataB) factors.
            // Frenzy: the caster attacks DataA faster for Dur.
            "Afzy" => self.apply_buff(h, Buff { id: buff_id, ability: a, level: lvl, caster: h, remaining: dur.max(self.data(a, "Dur", lvl)), attack_factor: self.data(a, "DataA", lvl), ..Default::default() }),
            "Ablo" => self.apply_buff(target, Buff { id: buff_id, ability: a, level: lvl, caster: h, remaining: dur, attack_factor: self.data(a, "DataA", lvl), move_factor: self.data(a, "DataB", lvl), ..Default::default() }),
            // Inner Fire: damage factor (DataA) and armor (DataB).
            "Ainf" => self.apply_buff(target, Buff { id: buff_id, ability: a, level: lvl, caster: h, remaining: dur, damage_factor: self.data(a, "DataA", lvl), armor: self.data(a, "DataB", lvl), ..Default::default() }),
            // Ensnare, Web, and Entangling Roots hold the target in place.
            // Entangling Roots strikes DataA once a second, the first at once (measured: a
            // footman rooted at 0.70 seconds takes 15 at 0.71, 1.71, and on to 8.71).
            "Aens" | "Aweb" | "AEer" => self.apply_buff(target, Buff { id: buff_id, ability: a, level: lvl, caster: h, remaining: dur, move_factor: -1.0, dps: if code == "AEer" { self.data(a, "DataA", lvl) } else { 0.0 }, dps_every: 1.0, dps_next: 0.0, ..Default::default() }),
            // Hurl Boulder, Firebolt, and similar: damage and stun.
            "ACtb" | "ANfb" => {
                self.spell_damage(vm, h, target, self.data(a, "DataA", lvl));
                self.apply_buff(target, Buff { id: buff_id, ability: a, level: lvl, caster: h, remaining: dur, stun: true, ..Default::default() });
            }
            other => {
                if !self.unit_spell_effect(vm, h, a, lvl, other, target, (x, y)) && !self.hero_spell_effect(vm, h, a, lvl, other, target, (x, y)) && !self.spell_effect(vm, h, a, lvl, other, target, (x, y)) {
                    log::debug!("ability {other} has no effect yet");
                }
            }
        }
    }

    /// A chance ability's roll by pseudo-random distribution (FUN_6f600930): the chance,
    /// to the nearest 5%, picks a constant C from the original's table, and the ability
    /// goes off when a uniform roll is at most C times the attempts since it last did.
    pub(crate) fn prd_roll(&mut self, h: u32, a: Id, chance_pct: f32) -> bool {
        const C: [u32; 21] = [0, 38, 147, 320, 547, 819, 1126, 1461, 1815, 2184, 2562, 2953, 3333, 3781, 4204, 4607, 5000, 5799, 6667, 7714, 10000];
        let i = ((chance_pct * 0.2 + 0.5) as i32).clamp(0, 20) as usize;
        let c = C[i] as f32 / 10000.0;
        let n = self.unit(h).and_then(|u| u.extra.prd.iter().find(|e| e.0 == a).map(|e| e.1)).unwrap_or(1);
        let hit = c * n as f32 >= self.rng.real(0.0, 1.0);
        if let Some(u) = self.unit_mut(h) {
            let next = if hit { 1 } else { n + 1 };
            match u.extra.prd.iter_mut().find(|e| e.0 == a) {
                Some(e) => e.1 = next,
                None => u.extra.prd.push((a, next)),
            }
        }
        hit
    }

    /// Passive attack modifiers: Bash, Critical Strike, and Demolish.  Returns the extra damage factor
    /// and flat bonus for this hit, and applies stuns.
    pub fn on_attack_passives(&mut self, attacker: u32, target: u32, damage: f32) -> f32 {
        let Some(u) = self.unit(attacker) else { return damage };
        let abilities = u.abilities.clone();
        let mut damage = damage;
        for (a, lvl) in abilities {
            let lvl = lvl.max(1) as u32;
            match self.ability_code(a).as_str() {
                // Bash: DataA% chance; DataC bonus damage; stun for Dur/HeroDur.
                "AHbh" => {
                    if self.prd_roll(attacker, a, self.data(a, "DataA", lvl)) {
                        damage += self.data(a, "DataC", lvl);
                        let d = if self.hero_duration(target) { self.data(a, "HeroDur", lvl) } else { self.data(a, "Dur", lvl) };
                        let buff = id_from_str(self.objects.abilities.get(a).map(|o| o.level_str("BuffID", lvl)).unwrap_or_default().trim());
                        self.apply_buff(target, Buff { id: buff, ability: a, level: lvl, caster: attacker, remaining: d, stun: true, ..Default::default() });
                    }
                }
                // Drunken Brawler: DataA% chance to deal DataB times damage.
                "ANdb" => {
                    if self.prd_roll(attacker, a, self.data(a, "DataA", lvl)) {
                        damage = damage * self.data(a, "DataB", lvl) + self.data(a, "DataC", lvl);
                    }
                }
                // Demolish: on what it allows, a DataA% chance to deal DataB times damage
                // to structures, DataC to units, and DataD to Heroes (none when zero).
                "ANde" if self.cast_target_error(attacker, a, target).is_none() => {
                    let (building, hero) = self.unit(target).map_or((false, false), |t| (t.is_building, t.is_hero));
                    let factor = self.data(a, if building { "DataB" } else if hero { "DataD" } else { "DataC" }, lvl);
                    if factor > 0.0 && self.prd_roll(attacker, a, self.data(a, "DataA", lvl)) {
                        damage *= factor;
                    }
                }
                // Critical Strike: DataA% chance to deal DataB times damage.
                "AOcr" => {
                    if self.prd_roll(attacker, a, self.data(a, "DataA", lvl)) {
                        damage = damage * self.data(a, "DataB", lvl) + self.data(a, "DataC", lvl);
                    }
                }
                _ => {}
            }
        }
        damage
    }

    /// Evasion: whether the target dodges this attack.
    pub fn evades(&mut self, target: u32) -> bool {
        let Some(u) = self.unit(target) else { return false };
        let abilities = u.abilities.clone();
        for (a, lvl) in abilities {
            let code = self.ability_code(a);
            if code == "AEev" || code == "ANdb" {
                let chance = self.data(a, if code == "ANdb" { "DataD" } else { "DataA" }, lvl.max(1) as u32) * 100.0;
                if (self.rng.int(1, 100) as f32) <= chance {
                    return true;
                }
            }
        }
        false
    }
}

/// Mana Shield: the shielded share of damage comes off mana while it lasts; returns the
/// damage left for life.
pub(crate) fn mana_shield(u: &mut Unit, dmg: f32) -> f32 {
    let Some((per, frac)) = u.buffs.iter().find(|b| b.mana_shield.1 > 0.0).map(|b| b.mana_shield) else { return dmg };
    let shielded = dmg * frac;
    let cost = shielded * per.max(0.0);
    let paid = cost.min(u.mana);
    u.mana -= paid;
    let absorbed = if cost > 0.0 { shielded * paid / cost } else { shielded };
    dmg - absorbed
}
