//! Attack modifiers: Searing and Cold Arrows and Black Arrow while their autocast is on
//! and mana lasts (each attack costs the ability's mana), Frost
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
        let (mut abilities, autocast, mana) = (u.abilities.clone(), u.autocast.clone(), u.mana);
        // Held items' abilities modify attacks too (orbs, Orb of Venom's poison).
        for item in u.inventory.clone() {
            if let Some(t) = self.item(item).map(|i| i.type_id) {
                abilities.extend(self.item_abilities(t).into_iter().map(|a| (a, 1)));
            }
        }
        let target_building = self.unit(target).is_some_and(|t| t.is_building);
        let target_hero = self.unit(target).is_some_and(|t| t.is_hero);
        let mut damage = damage;
        let mut mana_left = mana;
        let owner = u.owner;
        for (a, lvl) in abilities {
            if !self.ability_enabled(owner, a) {
                continue;
            }
            let lvl = lvl.max(1) as u32;
            let code = self.ability_code(a);
            let d = |w: &World, f: &str| w.data(a, f, lvl);
            let buffs = self.buff_ids(a, lvl);
            let hero = target_hero;
            let dur = if self.hero_duration(target) { d(self, "HeroDur") } else { d(self, "Dur") };
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
                // Incinerate (CAbilityIncinerateArrow, CBuffIncinerate): while autocast
                // and mana last, each hit on what it allows lays or renews its buff for
                // Dur, one more time each hit, and deals that many times DataA more.
                "ANia" | "ANic" if (code == "ANic" || autocast.contains(&a)) && self.cast_target_error(attacker, a, target).is_none() => {
                    let cost = if code == "ANia" { d(self, "Cost") } else { 0.0 };
                    if mana_left < cost {
                        continue;
                    }
                    mana_left -= cost;
                    let id = buffs.first().copied().unwrap_or_default();
                    self.apply_buff(target, Buff { id, ability: a, level: lvl, caster: attacker, remaining: d(self, "Dur"), ..Default::default() });
                    let mut stacks = 0;
                    if let Some(b) = self.unit_mut(target).and_then(|t| t.buffs.iter_mut().find(|b| b.id == id && b.ability == a)) {
                        b.stacks += 1;
                        b.caster = attacker;
                        stacks = b.stacks;
                    }
                    damage += stacks as f32 * d(self, "DataA");
                }
                // Orb of Lightning (CAbilityLightningPurge, an arrow without cost): each
                // hit purges what it may target.
                "AIlp" if self.cast_target_error(attacker, a, target).is_none() => {
                    if let Some(at) = self.widget_pos(target) {
                        self.ability_effect_pub(vm, attacker, a, 1, target, at);
                    }
                }
                // Freezing Breath: a structure hit stops working for Dur.
                "Afrz" if target_building => {
                    self.apply_buff(target, Buff { id: buffs.first().copied().unwrap_or_default(), ability: a, level: lvl, caster: attacker, remaining: d(self, "Dur"), stun: true, ..Default::default() });
                }
                // Orb of Annihilation: while autocast and mana last, DataA more damage, and
                // the hit spreads: all of it within DataD, DataB within DataE, and DataC
                // within Area.
                "Afak" if autocast.contains(&a) => {
                    let cost = d(self, "Cost");
                    if mana_left < cost {
                        continue;
                    }
                    mana_left -= cost;
                    damage += d(self, "DataA");
                    let (full_r, half_r, small_r, half, small) = (d(self, "DataD"), d(self, "DataE"), d(self, "Area"), d(self, "DataB"), d(self, "DataC"));
                    if let Some((tx, ty)) = self.unit(target).map(|t| (t.x, t.y)) {
                        for t in self.enemies_in(owner, tx, ty, small_r.max(half_r)) {
                            if t == target {
                                continue;
                            }
                            let dist = self.unit(t).map_or(f32::MAX, |u| (u.x - tx).hypot(u.y - ty));
                            let f = if dist <= full_r { 1.0 } else if dist <= half_r { half } else if dist <= small_r { small } else { 0.0 };
                            if f > 0.0 {
                                self.spell_damage(vm, attacker, t, damage * f);
                            }
                        }
                    }
                }
                // Frost Attack (and the Frost Wyrm's Frost Breath): the MiscGame frost slow.
                "Afra" | "Afrb" if !target_building => {
                    let (mv, at) = (self.misc_f32("FrostMoveSpeedDecrease", 0.5), self.misc_f32("FrostAttackSpeedDecrease", 0.25));
                    self.apply_buff(target, Buff { id: buffs.first().copied().unwrap_or_default(), ability: a, level: lvl, caster: attacker, remaining: dur, move_factor: -mv, attack_factor: -at, ..Default::default() });
                }
                // Feedback: burns mana (DataA, DataC for heroes) and deals it as damage
                // (DataB, DataD); summons take DataE.
                "Afbk" => {
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
                // Orbs (CAbilityAttackMod): DataA more damage, and their own effects.
                "AIob" | "AIcb" | "AIpb" | "AIlb" | "AIsb" | "AIfb" => {
                    damage += d(self, "DataA");
                    match code.as_str() {
                        // Orb of Frost: the frost slow for Dur (HeroDur on Heroes).
                        "AIob" if !target_building => {
                            let (mv, at) = (self.misc_f32("FrostMoveSpeedDecrease", 0.5), self.misc_f32("FrostAttackSpeedDecrease", 0.25));
                            self.apply_buff(target, Buff { id: buffs.first().copied().unwrap_or_default(), ability: a, level: lvl, caster: attacker, remaining: dur, move_factor: -mv, attack_factor: -at, ..Default::default() });
                        }
                        // Orb of Corruption: DataB less armor for Dur.
                        "AIcb" => {
                            self.apply_buff(target, Buff { id: buffs.first().copied().unwrap_or_default(), ability: a, level: lvl, caster: attacker, remaining: dur, armor: -d(self, "DataB"), ..Default::default() });
                        }
                        // Orb of Fire: the bonus burns others within Area too.
                        "AIfb" => {
                            let (tx, ty) = self.unit(target).map(|t| (t.x, t.y)).unwrap_or_default();
                            for t in self.enemies_in(owner, tx, ty, d(self, "Area")) {
                                if t != target {
                                    self.spell_damage(vm, attacker, t, d(self, "DataA"));
                                }
                            }
                        }
                        // Orbs that cast: their effect ability (UnitID) with a chance of
                        // DataB on units, DataC on Heroes, and DataD on summons.
                        "AIsb" => {
                            let chance = if target_hero { d(self, "DataC") } else if self.is_summoned(target) { d(self, "DataD") } else { d(self, "DataB") };
                            if (self.rng.int(1, 100) as f32) <= chance {
                                let effect = id_from_str(self.objects.abilities.get(a).map(|o| o.level_str("UnitID", lvl)).unwrap_or_default().trim());
                                if self.objects.abilities.get(effect).is_some() {
                                    let pos = self.unit(target).map(|t| (t.x, t.y)).unwrap_or_default();
                                    self.ability_effect_pub(vm, attacker, effect, 1, target, pos);
                                }
                            }
                        }
                        _ => {}
                    }
                }
                // Cleaving Attack: DataA of the damage to other enemies within Area.
                "ANca" => {
                    let (tx, ty) = self.unit(target).map(|t| (t.x, t.y)).unwrap_or_default();
                    let owner = self.unit(attacker).map_or(0, |u| u.owner);
                    for t in self.enemies_in(owner, tx, ty, d(self, "Area")) {
                        if t != target {
                            self.deal_damage_pub(vm, attacker, t, damage * d(self, "DataA"), "normal");
                        }
                    }
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

    /// Pillage: structures hit pay their cost times DataA per share of life dealt,
    /// gathered until it makes a step of DataB, after the damage event (measured: a
    /// grunt's five hits of about 10 on a 500-life farm pay 4 gold and 1 lumber, 0.82 and
    /// 0.2 each, the gold read at each hit's event the hits' before).
    pub(crate) fn pillage(&mut self, attacker: u32, target: u32, dealt: f32) {
        let Some(t) = self.unit(target).filter(|t| t.is_building) else { return };
        let Some(o) = self.objects.units.get(t.type_id) else { return };
        let (cost_gold, cost_lumber, max_life) = (o.real("goldcost"), o.real("lumbercost"), t.max_life.max(1.0));
        let Some(u) = self.unit(attacker) else { return };
        let owner = u.owner;
        let Some((a, lvl)) = u.abilities.iter().copied().find(|&(a, _)| self.ability_code(a) == "Asal" && self.ability_enabled(owner, a)) else { return };
        let lvl = lvl.max(1) as u32;
        let share = dealt / max_life * self.data(a, "DataA", lvl);
        let step = self.data(a, "DataB", lvl).max(1.0);
        let Some(u) = self.unit_mut(attacker) else { return };
        let acc = &mut u.extra.pillaged;
        acc[0] += cost_gold * share;
        acc[1] += cost_lumber * share;
        let gold = ((acc[0] / step).floor() * step) as i32;
        let lumber = ((acc[1] / step).floor() * step) as i32;
        acc[0] -= gold as f32;
        acc[1] -= lumber as f32;
        let p = &mut self.players[owner as usize];
        *p.state.entry(player_state::RESOURCE_GOLD).or_insert(0) += gold;
        *p.state.entry(player_state::RESOURCE_LUMBER).or_insert(0) += lumber;
    }

    /// Black Arrow: a unit dying under it raises DataB of the arrow's UnitID for its
    /// archer, lasting DataC.
    pub fn on_death_buffs(&mut self, h: u32) {
        let Some(u) = self.unit(h) else { return };
        if u.is_hero || u.is_building {
            return;
        }
        let (x, y) = (u.x, u.y);
        // Doom: DataB of UnitID rise for DataC.
        let doom: Vec<(Id, u32, u32)> = u.buffs.iter().filter(|b| self.ability_code(b.ability) == "ANdo").map(|b| (b.ability, b.level, b.caster)).collect();
        if let Some((a, lvl, caster)) = doom.first().copied() {
            let n = self.data(a, "DataB", lvl) as i32;
            let life = self.data(a, "DataC", lvl);
            let unit = id_from_str(self.objects.abilities.get(a).map(|o| o.level_str("UnitID", lvl)).unwrap_or_default().trim());
            let owner = self.unit(caster).map_or(0, |c| c.owner);
            self.summon(caster, owner, unit, n, x, y, life);
            return;
        }
        let Some(u) = self.unit(h) else { return };
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
