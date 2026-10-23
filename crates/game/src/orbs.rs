//! Attack modifiers: Searing and Cold Arrows and Black Arrow while their autocast is on
//! and mana lasts (each attack costs the ability's mana), Poison and Venom, Frost
//! Attack (MiscGame FrostMoveSpeedDecrease and FrostAttackSpeedDecrease), Feedback,
//! Liquid Fire on structures, and Pillage, which pays for damage done to structures.

use w3_jass::Vm;

use crate::objects::{Id, id_from_str};
use crate::types::*;
use crate::world::World;

impl World {
    fn buff_ids(&self, a: Id, lvl: u32) -> Vec<Id> {
        self.objects.abilities.get(a).map(|o| o.level_str("BuffID", lvl)).unwrap_or_default().split(',').map(|s| id_from_str(s.trim())).filter(|i| i[0] != 0).collect()
    }

    /// Applies a unit's attack modifiers to one of its hits on `target`; returns the
    /// damage after them.
    pub fn attack_modifiers(&mut self, vm: &mut Vm, attacker: u32, target: u32, damage: f32) -> f32 {
        let Some(u) = self.unit(attacker) else { return damage };
        let (abilities, autocast, mana) = (u.abilities.clone(), u.autocast.clone(), u.mana);
        let target_building = self.unit(target).is_some_and(|t| t.is_building);
        let target_hero = self.unit(target).is_some_and(|t| t.is_hero);
        let mut damage = damage;
        let mut mana_left = mana;
        for (a, lvl) in abilities {
            let lvl = lvl.max(1) as u32;
            let code = self.ability_code(a);
            let d = |w: &World, f: &str| w.data(a, f, lvl);
            let buffs = self.buff_ids(a, lvl);
            let hero = target_hero;
            let dur = if hero { d(self, "HeroDur") } else { d(self, "Dur") };
            match code.as_str() {
                // Arrows with a mana cost per attack while autocast.
                "AHfa" | "AHca" | "ANba" if autocast.contains(&a) && !target_building => {
                    let cost = d(self, "Cost");
                    if mana_left < cost {
                        continue;
                    }
                    mana_left -= cost;
                    damage += d(self, "DataA");
                    if code == "AHca" {
                        self.apply_buff(target, Buff { id: buffs.first().copied().unwrap_or_default(), ability: a, level: lvl, caster: attacker, remaining: dur, move_factor: -d(self, "DataB"), attack_factor: -d(self, "DataC"), ..Default::default() });
                    }
                    if code == "ANba" {
                        self.apply_buff(target, Buff { id: buffs.first().copied().unwrap_or_default(), ability: a, level: lvl, caster: attacker, remaining: d(self, "Dur").max(0.5), ..Default::default() });
                    }
                }
                // Poison and Venom: damage a second and slows for Dur.
                "Apoi" | "Aven" | "Apo2" if !target_building => {
                    self.apply_buff(target, Buff { id: buffs.first().copied().unwrap_or_default(), ability: a, level: lvl, caster: attacker, remaining: dur, dps: d(self, "DataA"), attack_factor: -d(self, "DataB"), move_factor: -d(self, "DataC"), ..Default::default() });
                }
                // Frost Attack: the MiscGame frost slow.
                "Afra" if !target_building => {
                    let (mv, at) = (self.misc_f32("FrostMoveSpeedDecrease", 0.5), self.misc_f32("FrostAttackSpeedDecrease", 0.25));
                    self.apply_buff(target, Buff { id: buffs.first().copied().unwrap_or_default(), ability: a, level: lvl, caster: attacker, remaining: dur, move_factor: -mv, attack_factor: -at, ..Default::default() });
                }
                // Feedback: burns mana (DataA, DataC for heroes) and deals it as damage
                // (DataB, DataD); summons take DataE.
                "Afbk" | "AIfb" => {
                    let (max, ratio) = if hero { (d(self, "DataC"), d(self, "DataD")) } else { (d(self, "DataA"), d(self, "DataB")) };
                    let burned = self.unit(target).map_or(0.0, |t| t.mana.min(max));
                    if let Some(t) = self.unit_mut(target) {
                        t.mana -= burned;
                    }
                    damage += burned * ratio;
                    if self.is_summoned(target) {
                        damage += d(self, "DataE");
                    }
                }
                // Liquid Fire: burning structures take DataA a second and attack slower.
                "Aliq" if target_building => {
                    self.apply_buff(target, Buff { id: buffs.first().copied().unwrap_or_default(), ability: a, level: lvl, caster: attacker, remaining: dur, dps: d(self, "DataA"), attack_factor: -d(self, "DataC"), no_repair: d(self, "DataD") == 0.0, ..Default::default() });
                }
                // Pillage: structures hit pay their cost times DataA per share of life.
                "Asal" if target_building => {
                    let Some(t) = self.unit(target) else { continue };
                    let Some(o) = self.objects.units.get(t.type_id) else { continue };
                    let share = (damage / t.max_life.max(1.0)) * d(self, "DataA");
                    let (gold, lumber) = ((o.real("goldcost") * share) as i32, (o.real("lumbercost") * share) as i32);
                    let owner = self.unit(attacker).map_or(0, |u| u.owner);
                    let p = &mut self.players[owner as usize];
                    *p.state.entry(player_state::RESOURCE_GOLD).or_insert(0) += gold;
                    *p.state.entry(player_state::RESOURCE_LUMBER).or_insert(0) += lumber;
                }
                _ => {}
            }
        }
        if mana_left != mana {
            if let Some(u) = self.unit_mut(attacker) {
                u.mana = mana_left;
            }
        }
        let _ = vm;
        damage
    }

    /// Black Arrow: a unit dying under it raises DataB of the arrow's UnitID for its
    /// archer, lasting DataC.
    pub fn on_death_buffs(&mut self, h: u32) {
        let Some(u) = self.unit(h) else { return };
        if u.is_hero || u.is_building {
            return;
        }
        let (x, y) = (u.x, u.y);
        let arrows: Vec<(Id, u32, u32)> = u.buffs.iter().filter(|b| self.ability_code(b.ability) == "ANba").map(|b| (b.ability, b.level, b.caster)).collect();
        if let Some((a, lvl, caster)) = arrows.first().copied() {
            let n = self.data(a, "DataB", lvl) as i32;
            let life = self.data(a, "DataC", lvl);
            let unit = id_from_str(self.objects.abilities.get(a).map(|o| o.level_str("UnitID", lvl)).unwrap_or_default().trim());
            let owner = self.unit(caster).map_or(0, |c| c.owner);
            self.summon(caster, owner, unit, n, x, y, life);
        }
    }
}
