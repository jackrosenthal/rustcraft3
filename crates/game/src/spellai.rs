//! Computer players' and creeps' spellcasting (each ability class's vf64, run for a
//! unit by FUN_6f695030).  A unit owned by a computer or creep player runs its
//! abilities' AI when it takes damage from an enemy (CUnit vf72), when it answers a
//! friend's call for help (FUN_6f66e9e0), and when it engages an enemy (FUN_6f9ce5b0,
//! FUN_6f9d35f0), with the enemy and the unit the fight is about (the "victim": the
//! damaged or called-for unit, or the unit itself).  Each ability decides whether to
//! cast and at what.
//!
//! Most start with the same gate (FUN_6f6095f0): no enemy, the ability's AI still
//! resting, the ability unable to cast now, a neutral enemy when the ability does not
//! allow one, an enemy that is not one, a victim not on the caster's side, a target
//! that already bears the spell's buff, or a target the spell cannot take all mean no.
//! An ability that weighs a cast rests its AI for half a second (FUN_6f432b20).  Many
//! count the units around them (FUN_6f602e30 with FUN_6f602f70): wards, and unless
//! asked magic-immune units, are not counted; of the caster's side, all, those with
//! more than 20 mana, those not invisible, and those attacking from beyond
//! 200; of its enemies, all, the disabled, the flying, the structures, those that can
//! fight (armed, not workers), those with more than 20 mana, and those not unseen
//! (invisible and undetected).  Constants are game.dll's static floats (400 at 0x6fd77f34, 450 at
//! 0x6fd77f38, 1000 at 0x6fd3c7f8, 9999 at 0x6fd3c804, and so on).

use w3_jass::Vm;

use crate::objects::{Id, id_from_str};
use crate::types::*;
use crate::world::World;

/// The units around a point, as FUN_6f602f70 counts them.
#[derive(Default, Debug, Clone, Copy)]
pub(crate) struct Census {
    /// The caster's side: all (0x18), ranged beyond 200 (0x1c), not invisible (0x3c),
    /// with more than 20 mana (0x44).
    pub allies: i32,
    pub ally_ranged: i32,
    pub ally_seen: i32,
    pub ally_mana: i32,
    /// Its enemies: all (0x20), flying (0x24), structures (0x28), able to fight (0x2c),
    /// disabled (0x34), not unseen (0x38), with more than 20 mana (0x40).
    pub enemies: i32,
    pub enemy_flying: i32,
    pub enemy_structures: i32,
    pub enemy_fighters: i32,
    pub enemy_disabled: i32,
    pub enemy_seen: i32,
    pub enemy_mana: i32,
}

/// Illidan's demon form ('Eidm'), which Sleep leaves alone.
const ILLIDAN_DEMON: Id = *b"Eidm";

impl World {
    /// Whether a player's units help another's (the caster's side in the census).
    fn helps(&self, a: u8, b: u8) -> bool {
        const HELP_RESPONSE: usize = 2;
        a == b || self.players.get(a as usize).is_some_and(|p| p.alliances[b as usize % PLAYER_COUNT][HELP_RESPONSE])
    }

    fn is_classed(&self, u: &Unit, class: &str) -> bool {
        self.objects.units.get(u.type_id).is_some_and(|o| o.str("type").to_ascii_lowercase().split(',').any(|c| c.trim() == class))
    }

    fn real_hero(u: &Unit) -> bool {
        u.is_hero && u.extra.illusion.is_none()
    }

    /// Whether a unit's owner is a computer or the creeps (the units whose spells the
    /// game casts).
    pub(crate) fn ai_controlled(&self, h: u32) -> bool {
        self.unit(h).and_then(|u| self.players.get(u.owner as usize)).is_some_and(|p| matches!(p.controller, map_control::COMPUTER | map_control::CREEP))
    }

    /// FUN_6f602e30: the census within `radius` of a point for `caster`'s side.
    pub(crate) fn census(&self, caster: u32, x: f32, y: f32, radius: f32, with_immune: bool) -> Census {
        let mut c = Census::default();
        let Some(owner) = self.unit(caster).map(|u| u.owner) else { return c };
        for &h in &self.unit_order {
            let Some(u) = self.unit(h) else { continue };
            if u.dead || u.hidden || (u.x - x).hypot(u.y - y) > radius || self.is_classed(u, "ward") {
                continue;
            }
            if !with_immune && self.is_magic_immune(h) {
                continue;
            }
            let mana = u.mana > 20.0;
            if self.helps(owner, u.owner) {
                c.allies += 1;
                c.ally_mana += mana as i32;
                c.ally_seen += !self.is_invisible(h) as i32;
                if crate::sim::unit_weapons(self, u).iter().map(|w| w.1.range).fold(0.0f32, f32::max) > 200.0 {
                    c.ally_ranged += 1;
                }
            } else if !crate::natives::players::is_ally(self, owner, u.owner) {
                c.enemies += 1;
                c.enemy_disabled += self.buff_mods(h).4 as i32;
                c.enemy_structures += u.is_building as i32;
                c.enemy_fighters += (!crate::sim::unit_weapons(self, u).is_empty() && !self.is_classed(u, "peon")) as i32;
                c.enemy_flying += (u.fly_height > 1.0) as i32;
                c.enemy_mana += mana as i32;
                c.enemy_seen += self.unit_visible_to(h, owner) as i32;
            }
        }
        c
    }

    fn ai_resting(&self, h: u32, a: Id) -> bool {
        self.unit(h).is_some_and(|u| u.extra.ai_rest.iter().any(|&(x, t)| x == a && t > self.time))
    }

    /// FUN_6f432b20: the ability's AI rests half a second.
    fn ai_rest(&mut self, h: u32, a: Id) {
        let until = self.time + 0.5;
        if let Some(u) = self.unit_mut(h) {
            u.extra.ai_rest.retain(|&(x, _)| x != a);
            u.extra.ai_rest.push((a, until));
        }
    }

    /// FUN_6f6095f0: whether the ability's AI should not cast (see the module notes).
    fn ai_gate(&self, h: u32, a: Id, enemy: u32, victim: u32, allow_neutral: bool, target: Option<u32>, buff: Option<Id>) -> bool {
        let Some(e) = self.unit(enemy) else { return true };
        if self.ai_resting(h, a) || self.can_cast(h, a).is_err() {
            return true;
        }
        if !allow_neutral && e.owner >= PLAYER_NEUTRAL_AGGRESSIVE {
            return true;
        }
        let Some(owner) = self.unit(h).map(|u| u.owner) else { return true };
        if crate::natives::players::is_ally(self, owner, e.owner) {
            return true;
        }
        let Some(v) = self.unit(victim) else { return true };
        if !crate::natives::players::is_ally(self, owner, v.owner) {
            return true;
        }
        if let Some(b) = buff {
            let on = target.unwrap_or(enemy);
            if self.unit(on).is_some_and(|t| t.buffs.iter().any(|x| x.id == b)) {
                return true;
            }
        }
        match target {
            Some(t) => self.cast_target_error(h, a, t).is_some(),
            None => false,
        }
    }

    /// FUN_6f6033e0 and FUN_6f40eaa0: orders the cast (at a unit, or with none), the
    /// unit going back to what it did after (order 0xd0006).
    fn ai_cast(&mut self, h: u32, a: Id, target: u32) {
        let (x, y) = self.widget_pos(target).unwrap_or_else(|| self.unit(h).map_or((0.0, 0.0), |u| (u.x, u.y)));
        if let Some(u) = self.unit_mut(h) {
            if !matches!(u.action, Action::Cast { .. }) {
                u.extra.resume = Some(u.action.clone());
            }
            u.action = Action::Cast { ability: a, target, x, y };
            u.path.clear();
        }
    }

    fn buff_id(&self, a: Id, lvl: u32) -> Option<Id> {
        let s = self.objects.abilities.get(a).map(|o| o.level_str("BuffID", lvl)).unwrap_or_default();
        let id = id_from_str(s.split(',').next().unwrap_or("").trim());
        (id[0] != 0).then_some(id)
    }

    fn life(&self, h: u32) -> f32 {
        self.unit(h).map_or(0.0, |u| u.life)
    }

    /// FUN_6f695030: each of the unit's abilities with an AI weighs a cast.
    pub(crate) fn spell_ai(&mut self, h: u32, enemy: u32, victim: u32) {
        if !self.ai_controlled(h) || self.unit(h).is_none_or(|u| u.dead || u.paused) {
            return;
        }
        self.item_ai(h, enemy, victim);
        let abilities: Vec<(Id, i32)> = self.unit(h).map(|u| u.abilities.clone()).unwrap_or_default();
        for (a, lvl) in abilities {
            if lvl <= 0 {
                continue;
            }
            // A cast already ordered this round ends the round.
            if self.unit(h).is_some_and(|u| matches!(u.action, Action::Cast { .. })) {
                return;
            }
            let lvl = lvl as u32;
            match self.ability_code(a).as_str() {
                "AHtc" | "ACtc" | "AOws" => self.ai_thunder_clap(h, a, lvl, enemy, victim),
                "AHtb" | "ACtb" | "ANfb" => self.ai_storm_bolt(h, a, lvl, enemy, victim),
                "AHhb" => self.ai_holy_light(h, a, lvl, enemy, victim),
                "AUdc" => self.ai_death_coil(h, a, lvl, enemy, victim),
                "AUsl" => self.ai_sleep(h, a, lvl, enemy, victim),
                "AUfn" => self.ai_frost_nova(h, a, lvl, enemy, victim),
                "AOcl" => self.ai_chain_lightning(h, a, lvl, enemy, victim),
                "AOhw" => self.ai_healing_wave(h, a, victim),
                "AUcs" => self.ai_carrion_swarm(h, a, lvl, enemy, victim),
                "AOsh" => self.ai_shockwave(h, a, lvl, enemy, victim),
                "AEer" => self.ai_entangling_roots(h, a, lvl, enemy, victim),
                "AEmb" => self.ai_mana_burn(h, a, lvl, enemy, victim),
                "AOhx" => self.ai_hex(h, a, enemy, victim),
                "Aprg" => self.ai_purge(h, a, lvl, enemy, victim),
                "Aply" => self.ai_polymorph(h, a, enemy, victim),
                "ANsi" | "ANdh" => self.ai_silence(h, a, lvl, enemy, victim),
                "AHbn" => self.ai_banish(h, a, enemy, victim),
                "AUim" => self.ai_impale(h, a, lvl, enemy, victim),
                "AHfs" => self.ai_flame_strike(h, a, lvl, enemy, victim),
                "ANdo" => self.ai_doom(h, a, lvl, enemy, victim),
                "Acri" => self.ai_cripple(h, a, enemy, victim),
                "AEfk" => self.ai_fan_of_knives(h, a, lvl, enemy, victim),
                "ANbf" | "ACbf" => self.ai_breath(h, a, lvl, enemy, victim),
                "AHwe" | "ANsg" | "ANsq" | "ANsw" | "ANlm" => self.ai_summon(h, a, enemy, victim),
                "AOsf" | "AIsh" => self.ai_feral_spirit(h, a, victim),
                "AHpx" => self.ai_phoenix(h, a, lvl, enemy, victim),
                "ANwm" => self.ai_watery_minion(h, a, enemy, victim),
                "ANsy" | "AOwd" => self.ai_ward(h, a, lvl, enemy, victim),
                "ANht" => self.ai_howl(h, a, lvl, enemy, victim),
                "Auhf" => self.ai_unholy_frenzy(h, a, lvl, enemy, victim),
                "Aspl" => self.ai_spirit_link(h, a, lvl, enemy, victim),
                "Alsh" => self.ai_lightning_shield(h, a, lvl, enemy, victim),
                "AUls" => self.ai_locust_swarm(h, a, lvl, enemy, victim),
                "AEsf" => self.ai_starfall(h, a, lvl, enemy, victim),
                "AOvd" => self.ai_voodoo(h, a, lvl, enemy, victim),
                "Arai" | "AUcb" | "AIrd" | "Avng" => self.ai_raise_dead(h, a, lvl, enemy, victim),
                "AUin" | "ANin" => self.ai_inferno(h, a, lvl, enemy, victim),
                "ANrc" => {
                    if !self.ai_gate(h, a, enemy, victim, false, None, None) {
                        self.ai_cast(h, a, enemy);
                    }
                }
                "AHdr" => self.ai_drain(h, a, lvl, enemy, victim),
                "Aens" => self.ai_ensnare(h, a, enemy, victim),
                "Aweb" => {
                    if !self.ai_gate(h, a, enemy, victim, true, Some(enemy), None) {
                        self.ai_cast(h, a, enemy);
                    }
                }
                "Acyc" => self.ai_cyclone(h, a, enemy),
                "Atau" => self.ai_taunt(h, a, lvl, enemy, victim),
                "ANtm" => self.ai_transmute(h, a, enemy, victim),
                "ANmo" => self.ai_monsoon(h, a, lvl, enemy, victim),
                "ANst" => self.ai_stampede(h, a, lvl, enemy, victim),
                "ANms" => self.ai_mana_shield(h, a, lvl, enemy, victim),
                "Arej" => self.ai_rejuvenation(h, a, lvl, enemy, victim),
                "ANcs" => self.ai_cluster_rockets(h, a, lvl, enemy, victim),
                "Aabs" => self.ai_absorb(h, a, enemy, victim),
                "ANch" => self.ai_charm(h, a, enemy, victim),
                "Acmg" => {
                    if !self.ai_gate(h, a, enemy, victim, true, Some(enemy), None) {
                        self.ai_cast(h, a, enemy);
                    }
                }
                "Advm" => self.ai_devour_magic(h, a, enemy, victim),
                "ANfl" => self.ai_forked_lightning(h, a, lvl, enemy, victim),
                "Apos" | "Aps2" => self.ai_possession(h, a, enemy, victim),
                "AEsh" => self.ai_lingering(h, a, lvl, enemy, victim, false),
                "ANso" => self.ai_lingering(h, a, lvl, enemy, victim, true),
                "Aams" => {
                    if !self.ai_gate(h, a, enemy, victim, true, Some(victim), None) && self.unit(victim).is_some_and(Self::real_hero) {
                        self.ai_cast(h, a, victim);
                    }
                }
                _ => {}
            }
        }
    }

    /// The abilities' "hurt" AI (their vf63), when a computer's or creep's unit takes
    /// damage from an enemy `attacker` (`attack` when by an attack).
    pub(crate) fn spell_ai_hurt(&mut self, vm: &mut Vm, h: u32, attacker: u32, attack: bool) {
        if !self.ai_controlled(h) || self.unit(h).is_none_or(|u| u.dead || u.paused) {
            return;
        }
        let abilities: Vec<(Id, i32)> = self.unit(h).map(|u| u.abilities.clone()).unwrap_or_default();
        for (a, lvl) in abilities {
            if lvl <= 0 || self.unit(h).is_some_and(|u| matches!(u.action, Action::Cast { .. })) {
                continue;
            }
            let lvl = lvl as u32;
            match self.ability_code(a).as_str() {
                // FUN_6f602930: Thunder Clap's own test, easier when hard pressed.
                "AHtc" | "ACtc" | "AOws" => self.thunder_clap_test(h, a, lvl, true),
                // FUN_6f602950: a summon past the gate.
                "AHwe" | "ANsg" | "ANsq" | "ANsw" | "ANlm" | "AHpx" | "AEsv" | "ANwm" => {
                    if !self.ai_gate(h, a, attacker, h, true, None, None) {
                        self.ai_cast(h, a, 0);
                    }
                }
                // FUN_6f6028d0: Feral Spirit against a seen enemy.
                "AOsf" | "AIsh" => {
                    let owner = self.unit(h).map_or(0, |u| u.owner);
                    let ok = self.unit(attacker).is_some_and(|e| !crate::natives::players::is_ally(self, owner, e.owner)) && self.unit_visible_to(attacker, owner);
                    if ok && self.can_cast(h, a).is_ok() {
                        self.ai_cast(h, a, 0);
                    }
                }
                // CAbilityWindWalk vf63: past the gate with less than a third of its life or
                // 200.
                "AOwk" => {
                    if !self.ai_gate(h, a, attacker, h, true, None, None) && (self.life_fraction(h) < 1.0 / 3.0 || self.life(h) < 200.0) {
                        self.ai_cast(h, a, 0);
                    }
                }
                "AOmi" => self.ai_mirror_image(h, a, attacker),
                // CAbilityAvatar vf63: able to cast, armed, with half its life or less.
                "AHav" => {
                    if self.can_cast(h, a).is_ok() && self.unit(h).is_some_and(|u| !crate::sim::unit_weapons(self, u).is_empty()) && self.life_fraction(h) <= 0.5 {
                        self.ai_cast(h, a, 0);
                    }
                }
                // CAbilityDivineShield vf63: struck by an attack, at once.
                "AHds" => {
                    if attack && self.can_cast(h, a).is_ok() {
                        self.ai_cast(h, a, 0);
                    }
                }
                // CAbilityMetamorphosis vf63: under half its life, struck by an enemy.
                "AEme" => {
                    let owner = self.unit(h).map_or(0, |u| u.owner);
                    let enemy = self.unit(attacker).is_some_and(|e| !crate::natives::players::is_ally(self, owner, e.owner));
                    if enemy && self.can_cast(h, a).is_ok() && self.life_fraction(h) < 0.5 {
                        self.ai_cast(h, a, 0);
                    }
                }
                _ => {}
            }
        }
        self.ai_use_potions(vm, h, attacker);
    }

    fn life_fraction(&self, h: u32) -> f32 {
        self.unit(h).map_or(1.0, |u| if u.max_life > 0.0 { u.life / u.max_life } else { 0.0 })
    }

    /// Mirror Image (CAbilityMirrorImage vf63): past the gate; at once when its mana is
    /// full or it has half its life or less; otherwise with 3 enemies able to fight
    /// within 500 of the attacker.
    fn ai_mirror_image(&mut self, h: u32, a: Id, attacker: u32) {
        if self.ai_gate(h, a, attacker, h, true, None, None) {
            return;
        }
        let full = self.unit(h).is_some_and(|u| u.mana == u.max_mana);
        if !full && self.life_fraction(h) > 0.5 {
            self.ai_rest(h, a);
            let (x, y) = self.pos(attacker);
            if self.census(h, x, y, 500.0, true).enemy_fighters < 3 {
                return;
            }
        }
        self.ai_cast(h, a, 0);
    }

    /// Held potions (CAbilityItemHeal, ItemHealAoe, ItemRestore, ItemInvul, ItemInvis
    /// vf63): healing once missing as much as it heals; restoration at less than 100
    /// life; invulnerability at 100 life or less against an enemy; invisibility with
    /// less than a third of its life or 200 against an enemy.
    fn ai_use_potions(&mut self, vm: &mut Vm, h: u32, attacker: u32) {
        let Some(u) = self.unit(h) else { return };
        let (life, max, owner) = (u.life, u.max_life, u.owner);
        let enemy = self.unit(attacker).is_some_and(|e| !crate::natives::players::is_ally(self, owner, e.owner));
        let slots: Vec<(usize, u32)> = u.inventory.iter().copied().enumerate().filter(|&(_, i)| i != 0).collect();
        for (slot, item) in slots {
            let Some(t) = self.item(item).map(|i| i.type_id) else { continue };
            for a in self.item_abilities(t) {
                let use_it = match self.ability_code(a).as_str() {
                    "AIhe" | "AIha" => max - life >= self.data(a, "DataA", 1),
                    "AIre" => life < 100.0,
                    "AIvu" => enemy && life <= 100.0,
                    "AIvi" => enemy && (self.life_fraction(h) < 1.0 / 3.0 || life < 200.0),
                    _ => false,
                };
                if use_it && self.unit(h).is_some_and(|u| u.cooldowns.get(&a).copied().unwrap_or(0.0) <= 0.0) {
                    self.use_item(vm, h, slot, 0, 0.0, 0.0);
                    return;
                }
            }
        }
    }

    /// Thunder Clap, War Stomp (FUN_6f60fca0, FUN_6f605650): with the victim on the
    /// caster's side and the AI ready, it counts within Area (no magic-immune units):
    /// at least 3 seen enemies besides structures, and more enemies able to fight,
    /// besides structures, than units on its side.
    fn ai_thunder_clap(&mut self, h: u32, a: Id, lvl: u32, enemy: u32, victim: u32) {
        let Some(owner) = self.unit(h).map(|u| u.owner) else { return };
        if enemy == 0 || self.unit(victim).is_none_or(|v| !crate::natives::players::is_ally(self, owner, v.owner)) {
            return;
        }
        self.thunder_clap_test(h, a, lvl, false);
    }

    /// FUN_6f605650: the census test; when hurt and below 400 life, 2 seen enemies
    /// within Area suffice.
    fn thunder_clap_test(&mut self, h: u32, a: Id, lvl: u32, hurt: bool) {
        if self.ai_resting(h, a) || self.can_cast(h, a).is_err() {
            return;
        }
        self.ai_rest(h, a);
        let (x, y) = self.pos(h);
        let c = self.census(h, x, y, self.data(a, "Area", lvl), false);
        if hurt && self.life(h) < 400.0 {
            if c.enemy_seen < 2 {
                return;
            }
        } else if c.enemy_seen - c.enemy_structures < 3 || c.enemy_fighters - c.enemy_structures <= c.allies {
            return;
        }
        self.ai_cast(h, a, 0);
    }

    /// Storm Bolt, Firebolt (FUN_6f60fb90): past the gate, at once on a Hero; otherwise
    /// only when the caster is the one fighting, at an enemy with between DataA + 50 and
    /// DataA + 150 life (between 0.6 × DataA and 9999 when the caster has less than
    /// 400 life).
    fn ai_storm_bolt(&mut self, h: u32, a: Id, lvl: u32, enemy: u32, victim: u32) {
        if self.ai_gate(h, a, enemy, victim, true, Some(enemy), None) {
            return;
        }
        if self.unit(enemy).is_some_and(Self::real_hero) {
            self.ai_cast(h, a, enemy);
            return;
        }
        if h != victim {
            return;
        }
        let dmg = self.data(a, "DataA", lvl);
        let (lo, hi) = if self.life(h) >= 400.0 { (dmg + 50.0, dmg + 150.0) } else { (dmg * 0.6, 9999.0) };
        let life = self.life(enemy);
        if lo <= life && life <= hi {
            self.ai_cast(h, a, enemy);
        }
    }

    /// How many more casts the caster's mana pays for (10 when the spell is free).
    fn casts_left(&self, h: u32, a: Id, lvl: u32) -> i32 {
        let cost = self.data(a, "Cost", lvl);
        if cost <= 0.0 { 10 } else { (self.unit(h).map_or(0.0, |u| u.mana) / cost) as i32 }
    }

    fn race_of(&self, h: u32) -> String {
        self.unit(h).and_then(|u| self.objects.units.get(u.type_id)).map(|o| o.str("race").to_ascii_lowercase()).unwrap_or_default()
    }

    /// FUN_6f608c60: whom Holy Light suits: a friend that has 100 life or less or is
    /// missing at least `amount`, or an undead or demon enemy (when `other` is on the
    /// caster's side) that is a Hero or has between `amount` + 20 and `amount` + 50
    /// life.
    fn holy_light_target(&self, h: u32, cand: u32, other: u32, amount: f32) -> Option<u32> {
        let owner = self.unit(h)?.owner;
        let c = self.unit(cand)?;
        if crate::natives::players::is_ally(self, owner, c.owner) {
            if c.life > 100.0 && c.max_life - c.life < amount {
                return None;
            }
            return Some(cand);
        }
        if other != 0 && self.unit(other).is_some_and(|o| !crate::natives::players::is_ally(self, owner, o.owner)) {
            return None;
        }
        if !matches!(self.race_of(cand).as_str(), "undead" | "demon") {
            return None;
        }
        if Self::real_hero(c) {
            return Some(cand);
        }
        (amount <= c.life - 20.0 && amount >= c.life - 50.0).then_some(cand)
    }

    /// Holy Light (CAbilityHolyBolt vf64): heals the victim when it suits; else, with
    /// fewer than 3 casts' mana and the victim not itself, nothing; else the enemy when
    /// half the heal suits it.
    fn ai_holy_light(&mut self, h: u32, a: Id, lvl: u32, enemy: u32, victim: u32) {
        let Some(owner) = self.unit(h).map(|u| u.owner) else { return };
        if self.unit(enemy).is_some_and(|e| crate::natives::players::is_ally(self, owner, e.owner)) {
            return;
        }
        if self.can_cast(h, a).is_err() {
            return;
        }
        let amount = self.data(a, "DataA", lvl);
        let casts = self.casts_left(h, a, lvl);
        let target = match self.holy_light_target(h, victim, enemy, amount) {
            Some(t) => t,
            None => {
                if casts < 3 && victim != h {
                    return;
                }
                let Some(t) = self.holy_light_target(h, enemy, victim, amount * 0.5) else { return };
                t
            }
        };
        if self.cast_target_error(h, a, target).is_none() {
            self.ai_cast(h, a, target);
        }
    }

    /// FUN_6f603e00: whom Death Coil suits: an enemy (with `other` on the caster's
    /// side) that is a Hero or has between `amount` + 20 and `amount` + 50 life, or an
    /// undead or demon friend with 70 life or less or missing at least `amount`.
    fn death_coil_target(&self, h: u32, cand: u32, other: u32, amount: f32) -> Option<u32> {
        let owner = self.unit(h)?.owner;
        let c = self.unit(cand)?;
        if !crate::natives::players::is_ally(self, owner, c.owner) {
            if self.unit(other).is_none_or(|o| !crate::natives::players::is_ally(self, owner, o.owner)) {
                return None;
            }
            if !Self::real_hero(c) && (c.life - 20.0 < amount || c.life - 50.0 > amount) {
                return None;
            }
            return Some(cand);
        }
        if !matches!(self.race_of(cand).as_str(), "undead" | "demon") {
            return None;
        }
        if c.life > 70.0 && c.max_life - c.life < amount {
            return None;
        }
        Some(cand)
    }

    /// Death Coil (CAbilityDeathCoil vf64): past the gate, heals the victim when it
    /// suits; else, with fewer than 3 casts' mana and the victim not itself, nothing;
    /// else the enemy when half of DataA suits it.
    fn ai_death_coil(&mut self, h: u32, a: Id, lvl: u32, enemy: u32, victim: u32) {
        if self.ai_gate(h, a, enemy, victim, true, None, None) {
            return;
        }
        let amount = self.data(a, "DataA", lvl);
        let casts = self.casts_left(h, a, lvl);
        let target = match self.death_coil_target(h, victim, enemy, amount) {
            Some(t) => t,
            None => {
                if casts < 3 && victim != h {
                    return;
                }
                let Some(t) = self.death_coil_target(h, enemy, victim, amount * 0.5) else { return };
                t
            }
        };
        if self.cast_target_error(h, a, target).is_none() {
            self.ai_cast(h, a, target);
        }
    }

    /// Sleep (CAbilitySleep vf64): past the gate (and not on one already asleep), not
    /// on Illidan's demon form, on an enemy with at least 100 life that is a Hero, or
    /// has at least 200 (and 450 unless the caster is the victim), and, above the first
    /// level, not a creep of level 5 or less; then with more than one enemy able to
    /// fight and not disabled within 1000.
    fn ai_sleep(&mut self, h: u32, a: Id, lvl: u32, enemy: u32, victim: u32) {
        let buff = self.buff_id(a, lvl);
        if self.ai_gate(h, a, enemy, victim, true, Some(enemy), buff) {
            return;
        }
        let Some(e) = self.unit(enemy) else { return };
        if e.type_id == ILLIDAN_DEMON || e.life < 100.0 {
            return;
        }
        let worthy = Self::real_hero(e) || (e.life >= 200.0 && (victim == h || e.life >= 450.0));
        let creep_ok = lvl <= 1 || e.owner < PLAYER_NEUTRAL_AGGRESSIVE || e.level > 5;
        if !worthy || !creep_ok {
            return;
        }
        self.ai_rest(h, a);
        let (x, y) = self.unit(h).map(|u| (u.x, u.y)).unwrap();
        let c = self.census(h, x, y, 1000.0, true);
        if c.enemy_fighters - c.enemy_disabled > 1 {
            self.ai_cast(h, a, enemy);
        }
    }

    /// The caster fighting for itself with less than 400 life asks less of a spell.
    fn hard_pressed(&self, h: u32, victim: u32) -> bool {
        victim == h && self.life(h) < 400.0
    }

    fn pos(&self, h: u32) -> (f32, f32) {
        self.unit(h).map_or((0.0, 0.0), |u| (u.x, u.y))
    }

    /// Frost Nova (CAbilityFrostNova vf64): past the gate, at once on a Hero; else on
    /// an enemy with at least DataA life when, within Area of it (no magic-immune units),
    /// enemies able to fight besides structures number 3 (1 when hard pressed).
    fn ai_frost_nova(&mut self, h: u32, a: Id, lvl: u32, enemy: u32, victim: u32) {
        if self.ai_gate(h, a, enemy, victim, true, Some(enemy), None) {
            return;
        }
        if !self.unit(enemy).is_some_and(Self::real_hero) {
            if self.life(enemy) < self.data(a, "DataA", lvl) {
                return;
            }
            self.ai_rest(h, a);
            let (x, y) = self.pos(enemy);
            let c = self.census(h, x, y, self.data(a, "Area", lvl), false);
            let need = if self.hard_pressed(h, victim) { 1 } else { 3 };
            if c.enemy_fighters - c.enemy_structures < need {
                return;
            }
        }
        self.ai_cast(h, a, enemy);
    }

    /// Chain Lightning (CAbilityChainLightning vf64): past the gate, on an enemy with at
    /// least DataA life when, within Area of it (no magic-immune units), there are as
    /// many enemies as DataB targets (1 for a Hero or when hard pressed; at most 3, 4
    /// above the first level).
    fn ai_chain_lightning(&mut self, h: u32, a: Id, lvl: u32, enemy: u32, victim: u32) {
        if self.ai_gate(h, a, enemy, victim, true, Some(enemy), None) {
            return;
        }
        if self.life(enemy) < self.data(a, "DataA", lvl) {
            return;
        }
        let mut need = self.data(a, "DataB", lvl) as i32;
        if self.unit(enemy).is_some_and(Self::real_hero) || self.hard_pressed(h, victim) {
            need = 1;
        }
        need = need.min(if lvl > 1 { 4 } else { 3 });
        self.ai_rest(h, a);
        let (x, y) = self.pos(enemy);
        let c = self.census(h, x, y, self.data(a, "Area", lvl), false);
        if need <= c.enemies {
            self.ai_cast(h, a, enemy);
        }
    }

    /// Healing Wave (CAbilityHealingWave vf64): with its AI ready, on a friendly victim
    /// it may target.
    fn ai_healing_wave(&mut self, h: u32, a: Id, victim: u32) {
        let Some(owner) = self.unit(h).map(|u| u.owner) else { return };
        if victim == 0 || self.ai_resting(h, a) || self.can_cast(h, a).is_err() {
            return;
        }
        if self.unit(victim).is_some_and(|v| crate::natives::players::is_ally(self, owner, v.owner)) && self.cast_target_error(h, a, victim).is_none() {
            self.ai_cast(h, a, victim);
        }
    }

    /// Carrion Swarm (CAbilityCarrionSwarm vf64): past the gate, at the enemy when,
    /// within the wave's width where it reaches the enemy (Area widening to DataD over
    /// DataC; no magic-immune units), enemies able to fight besides structures number 3
    /// (2 when hard pressed) and none is disabled.
    fn ai_carrion_swarm(&mut self, h: u32, a: Id, lvl: u32, enemy: u32, victim: u32) {
        if self.ai_gate(h, a, enemy, victim, true, Some(enemy), None) {
            return;
        }
        let need = if self.hard_pressed(h, victim) { 2 } else { 3 };
        self.ai_rest(h, a);
        let ((cx, cy), (ex, ey)) = (self.pos(h), self.pos(enemy));
        let along = (ex - cx).hypot(ey - cy) / self.data(a, "DataC", lvl).max(1.0);
        let (start, end) = (self.data(a, "Area", lvl), self.data(a, "DataD", lvl));
        let c = self.census(h, ex, ey, start + (end - start) * along, false);
        if need <= c.enemy_fighters - c.enemy_structures && c.enemy_disabled < 1 {
            self.ai_cast(h, a, enemy);
        }
    }

    /// Shockwave (CAbilityShockwave vf64): past the gate, at the enemy when, within Area
    /// of it (no magic-immune units), 3 enemies (2 when hard pressed) are seen.
    fn ai_shockwave(&mut self, h: u32, a: Id, lvl: u32, enemy: u32, victim: u32) {
        if self.ai_gate(h, a, enemy, victim, true, Some(enemy), None) {
            return;
        }
        self.ai_rest(h, a);
        let need = if self.hard_pressed(h, victim) { 2 } else { 3 };
        let (x, y) = self.pos(enemy);
        let c = self.census(h, x, y, self.data(a, "Area", lvl), false);
        if need <= c.enemy_seen {
            self.ai_cast(h, a, enemy);
        }
    }

    /// Entangling Roots (CAbilityEntanglingRoots vf64): able to cast, on an enemy not
    /// already rooted (BEer); a structure at once, as when the caster fights for itself
    /// with 200 life or less and a third of its life or less; else only an enemy with
    /// between half and all of the roots' damage (DataA a second for Dur) as life.
    fn ai_entangling_roots(&mut self, h: u32, a: Id, lvl: u32, enemy: u32, victim: u32) {
        let Some(owner) = self.unit(h).map(|u| u.owner) else { return };
        let Some(e) = self.unit(enemy) else { return };
        if self.can_cast(h, a).is_err() || crate::natives::players::is_ally(self, owner, e.owner) || e.buffs.iter().any(|b| b.id == *b"BEer") {
            return;
        }
        let low = victim == h && self.life(h) <= 200.0 && self.unit(h).is_some_and(|u| u.max_life > 0.0 && u.life / u.max_life <= 1.0 / 3.0);
        if !e.is_building && !low {
            let dur = if self.hero_duration(enemy) { self.data(a, "HeroDur", lvl) } else { self.data(a, "Dur", lvl) };
            let total = self.data(a, "DataA", lvl) * dur;
            if e.life < total * 0.5 || total < e.life {
                return;
            }
        }
        self.ai_cast(h, a, enemy);
    }

    /// Mana Burn (CAbilityManaBurn vf64, FUN_6f608e10): able to cast, with the victim on
    /// its side, at an enemy player's unit (no creep) with at least 50 mana for a Hero
    /// or DataA for others.
    fn ai_mana_burn(&mut self, h: u32, a: Id, lvl: u32, enemy: u32, victim: u32) {
        let Some(owner) = self.unit(h).map(|u| u.owner) else { return };
        let Some(e) = self.unit(enemy) else { return };
        if self.can_cast(h, a).is_err() || crate::natives::players::is_ally(self, owner, e.owner) {
            return;
        }
        if self.unit(victim).is_none_or(|v| !crate::natives::players::is_ally(self, owner, v.owner)) {
            return;
        }
        let need = if Self::real_hero(e) { 50.0 } else { self.data(a, "DataA", lvl) };
        if e.owner < PLAYER_NEUTRAL_AGGRESSIVE && e.mana >= need {
            self.ai_cast(h, a, enemy);
        }
    }

    /// Hex (CAbilityHex vf64): past the gate (no creeps), on a unit that is not a Hero
    /// with more than 450 life.
    fn ai_hex(&mut self, h: u32, a: Id, enemy: u32, victim: u32) {
        if self.ai_gate(h, a, enemy, victim, false, Some(enemy), None) {
            return;
        }
        if self.unit(enemy).is_some_and(|e| !Self::real_hero(e) && e.life > 450.0) {
            self.ai_cast(h, a, enemy);
        }
    }

    /// Whether a unit bears a buff an enemy laid on it (UnitHasBuffsEx's negative
    /// buffs, as Purge's AI asks).
    fn has_hostile_buff(&self, h: u32) -> bool {
        let Some(u) = self.unit(h) else { return false };
        u.buffs.iter().any(|b| b.remaining > 0.5 && self.unit(b.caster).is_some_and(|c| !crate::natives::players::is_ally(self, u.owner, c.owner)))
    }

    /// Purge (FUN_6f60eb10): past the gate (no creeps): a summoned unit with at least
    /// 0.6 × DataC life; another only when no enemy spell is on it already, it is not
    /// purged (Bprg), and it attacks from closer than 200 or not at all.
    fn ai_purge(&mut self, h: u32, a: Id, lvl: u32, enemy: u32, victim: u32) {
        if self.ai_gate(h, a, enemy, victim, false, Some(enemy), None) {
            return;
        }
        let Some(e) = self.unit(enemy) else { return };
        if self.is_classed(e, "summoned") {
            if e.life < 0.6 * self.data(a, "DataC", lvl) {
                return;
            }
        } else {
            if self.has_hostile_buff(enemy) || e.buffs.iter().any(|b| b.id == *b"Bprg") {
                return;
            }
            let reach = crate::sim::unit_weapons(self, e).iter().map(|w| w.1.range).fold(0.0f32, f32::max);
            if reach >= 200.0 {
                return;
            }
        }
        self.ai_cast(h, a, enemy);
    }

    /// Polymorph (CAbilityPolymorph vf64): past the gate (no creeps), on a unit with at
    /// least 100 mana; else only one with more than 450 life while the caster's mana
    /// is full.
    fn ai_polymorph(&mut self, h: u32, a: Id, enemy: u32, victim: u32) {
        if self.ai_gate(h, a, enemy, victim, false, Some(enemy), None) {
            return;
        }
        let Some(e) = self.unit(enemy) else { return };
        if e.mana < 100.0 {
            if e.life <= 450.0 {
                return;
            }
            if self.unit(h).is_some_and(|u| u.mana != u.max_mana) {
                return;
            }
        }
        self.ai_cast(h, a, enemy);
    }

    /// Silence, Drunken Haze (FUN_6f60f1a0): past the gate (no creeps, not on one
    /// bearing it), on a unit that is not a Hero with at least 75 mana and 100 life.
    fn ai_silence(&mut self, h: u32, a: Id, lvl: u32, enemy: u32, victim: u32) {
        let buff = self.buff_id(a, lvl);
        if self.ai_gate(h, a, enemy, victim, false, Some(enemy), buff) {
            return;
        }
        if self.unit(enemy).is_some_and(|e| !Self::real_hero(e) && e.mana >= 75.0 && e.life >= 100.0) {
            self.ai_cast(h, a, enemy);
        }
    }

    /// Banish (CAbilityBanish vf64): past the gate (no creeps), on a unit that is not a
    /// Hero with more than 450 life.
    fn ai_banish(&mut self, h: u32, a: Id, enemy: u32, victim: u32) {
        if self.ai_gate(h, a, enemy, victim, false, Some(enemy), None) {
            return;
        }
        if self.unit(enemy).is_some_and(|e| !Self::real_hero(e) && e.life > 450.0) {
            self.ai_cast(h, a, enemy);
        }
    }

    /// Impale (CAbilityImpale vf64): past the gate, at the enemy when within Area of it
    /// (no magic-immune units) 3 enemies (2 when hard pressed) are seen.
    fn ai_impale(&mut self, h: u32, a: Id, lvl: u32, enemy: u32, victim: u32) {
        if self.ai_gate(h, a, enemy, victim, true, Some(enemy), None) {
            return;
        }
        self.ai_rest(h, a);
        let need = if self.hard_pressed(h, victim) { 2 } else { 3 };
        let (x, y) = self.pos(enemy);
        let c = self.census(h, x, y, self.data(a, "Area", lvl), false);
        if need <= c.enemy_seen {
            self.ai_cast(h, a, enemy);
        }
    }

    /// Flame Strike (CAbilityFlameStrike vf64): past the gate, at the enemy when within
    /// Area of it (no magic-immune units) more than one enemy is on the ground and the
    /// enemies are at least twice the caster's side.
    fn ai_flame_strike(&mut self, h: u32, a: Id, lvl: u32, enemy: u32, victim: u32) {
        if self.ai_gate(h, a, enemy, victim, true, Some(enemy), None) {
            return;
        }
        self.ai_rest(h, a);
        let (x, y) = self.pos(enemy);
        let c = self.census(h, x, y, self.data(a, "Area", lvl), false);
        if c.enemies - c.enemy_flying > 1 && c.allies * 2 <= c.enemies {
            self.ai_cast(h, a, enemy);
        }
    }

    /// Doom (CAbilityDoom vf64): past the gate (no creeps, not on one doomed), at once
    /// when hard pressed; else on a unit with at least 400 maximum life and at most
    /// 5 × DataA life, with at least 3 seen enemies within 800 of the caster.
    fn ai_doom(&mut self, h: u32, a: Id, lvl: u32, enemy: u32, victim: u32) {
        let buff = self.buff_id(a, lvl);
        if self.ai_gate(h, a, enemy, victim, false, Some(enemy), buff) {
            return;
        }
        if !(victim == h && self.life(h) <= 400.0) {
            let Some(e) = self.unit(enemy) else { return };
            if e.max_life < 400.0 || self.data(a, "DataA", lvl) * 5.0 < e.life {
                return;
            }
            let (x, y) = self.pos(h);
            if self.census(h, x, y, 800.0, true).enemy_seen < 3 {
                return;
            }
        }
        self.ai_cast(h, a, enemy);
    }

    /// Cripple (CAbilityCripple vf64): past the gate (no creeps), on a Hero with 100 life
    /// or less or another with more than 50, not crippled (Bcri), that deals at least
    /// 14 damage a second (its average damage over its cooldown).
    fn ai_cripple(&mut self, h: u32, a: Id, enemy: u32, victim: u32) {
        if self.ai_gate(h, a, enemy, victim, false, Some(enemy), None) {
            return;
        }
        let Some(e) = self.unit(enemy) else { return };
        if if Self::real_hero(e) { e.life > 100.0 } else { e.life <= 50.0 } {
            return;
        }
        if e.buffs.iter().any(|b| b.id == *b"Bcri") {
            return;
        }
        let Some((_, w)) = crate::sim::unit_weapons(self, e).into_iter().next() else { return };
        if w.cooldown <= 0.0 {
            return;
        }
        let (min, max) = (w.base + w.dice as f32, w.base + (w.dice * w.sides) as f32);
        if ((max - min) / 2.0 + min) / w.cooldown >= 14.0 {
            self.ai_cast(h, a, enemy);
        }
    }

    /// Fan of Knives (CAbilityFanOfKnives vf64): with its AI ready, an enemy, and the
    /// victim on its side: at once on a Hero with DataA life or less; else when within
    /// Area of the caster (no magic-immune units) there are 3 enemies (1 when hard
    /// pressed).
    fn ai_fan_of_knives(&mut self, h: u32, a: Id, lvl: u32, enemy: u32, victim: u32) {
        let Some(owner) = self.unit(h).map(|u| u.owner) else { return };
        let Some(e) = self.unit(enemy) else { return };
        if self.ai_resting(h, a) || self.can_cast(h, a).is_err() || crate::natives::players::is_ally(self, owner, e.owner) {
            return;
        }
        if self.unit(victim).is_none_or(|v| !crate::natives::players::is_ally(self, owner, v.owner)) {
            return;
        }
        if !(Self::real_hero(e) && e.life <= self.data(a, "DataA", lvl)) {
            self.ai_rest(h, a);
            let (x, y) = self.pos(h);
            let need = if self.hard_pressed(h, victim) { 1 } else { 3 };
            if self.census(h, x, y, self.data(a, "Area", lvl), false).enemies < need {
                return;
            }
        }
        self.ai_cast(h, a, 0);
    }

    /// Breath of Fire, Breath of Frost (FUN_6f60ba10): with Drunken Haze too and mana
    /// for both, the haze goes first on an enemy without it; otherwise as Carrion
    /// Swarm.
    fn ai_breath(&mut self, h: u32, a: Id, lvl: u32, enemy: u32, victim: u32) {
        if self.can_cast(h, a).is_err() {
            return;
        }
        if let Some((haze, hl)) = self.ability_with_code(h, "ANdh") {
            let hl = hl.max(1) as u32;
            let both = self.data(a, "Cost", lvl) + self.data(haze, "Cost", hl);
            if self.unit(h).is_some_and(|u| u.mana < both) {
                return;
            }
            let hazed = self.unit(enemy).is_some_and(|e| e.buffs.iter().any(|b| b.ability == haze));
            if !hazed {
                self.ai_silence(h, haze, hl, enemy, victim);
                if self.unit(h).is_some_and(|u| matches!(u.action, Action::Cast { .. })) {
                    return;
                }
            }
        }
        self.ai_carrion_swarm(h, a, lvl, enemy, victim);
    }

    /// The area spells' watch (their vf50, FUN_6f05c020): every half second a
    /// computer's or creep's unit with one of them looks at the units within the
    /// spell's cast range (1000 for Death and Decay), and each enemy that has come
    /// within it since lets the spell weigh it (their vf3 on the watch event).
    pub(crate) fn update_spell_watch(&mut self) {
        let now = self.time;
        let casters: Vec<u32> = self.unit_order.iter().copied().filter(|&h| self.unit(h).is_some_and(|u| !u.dead && !u.paused && u.extra.ai_watch <= now && u.abilities.iter().any(|&(a, l)| l > 0 && (WATCHED.contains(&self.ability_code(a).as_str()) || EVERY_2S.contains(&self.ability_code(a).as_str()))))).collect();
        for h in casters {
            if let Some(u) = self.unit_mut(h) {
                u.extra.ai_watch = now + 0.5;
            }
            if !self.ai_controlled(h) || self.unit(h).is_some_and(|u| matches!(u.action, Action::Cast { .. })) {
                continue;
            }
            let abilities: Vec<(Id, i32)> = self.unit(h).map(|u| u.abilities.clone()).unwrap_or_default();
            for (a, lvl) in abilities {
                let code = self.ability_code(a);
                if lvl > 0 && EVERY_2S.contains(&code.as_str()) {
                    let due = self.unit(h).is_some_and(|u| u.extra.ai_rest.iter().find(|e| e.0 == a).is_none_or(|e| e.1 <= now));
                    if due {
                        if let Some(u) = self.unit_mut(h) {
                            u.extra.ai_rest.retain(|e| e.0 != a);
                            u.extra.ai_rest.push((a, now + 2.0));
                        }
                        self.ai_raise_corpses(h, a, lvl as u32, code == "AHre");
                    }
                    continue;
                }
                if lvl <= 0 || !WATCHED.contains(&code.as_str()) {
                    continue;
                }
                let lvl = lvl as u32;
                let radius = match code.as_str() {
                    "AUdd" => 1000.0,
                    "ANsl" => self.data(a, "Rng", lvl) + 400.0,
                    _ => self.data(a, "Rng", lvl),
                };
                // Dark Conversion and Soul Preservation weigh every unit, not only enemies.
                let any = matches!(code.as_str(), "ANdc" | "ANsl");
                let (x, y, owner) = self.unit(h).map(|u| (u.x, u.y, u.owner)).unwrap();
                let near: Vec<u32> = self.unit_order.iter().copied().filter(|&t| t != h && self.unit(t).is_some_and(|u| !u.dead && !u.hidden && (u.x - x).hypot(u.y - y) <= radius && (any || !crate::natives::players::is_ally(self, owner, u.owner)))).collect();
                let before = self.unit(h).and_then(|u| u.extra.ai_inside.iter().find(|e| e.0 == a).map(|e| e.1.clone())).unwrap_or_default();
                if let Some(u) = self.unit_mut(h) {
                    u.extra.ai_inside.retain(|e| e.0 != a);
                    u.extra.ai_inside.push((a, near.clone()));
                }
                for t in near.into_iter().filter(|t| !before.contains(t)) {
                    match code.as_str() {
                        "AHbz" | "ANrf" => self.ai_blizzard(h, a, lvl, t),
                        "AUdd" => self.ai_death_and_decay(h, a, lvl, t),
                        // Dark Conversion, Soul Preservation (their vf3 on the watch): on a
                        // unit they may target that does not bear them already.
                        "ANdc" | "ANsl" => {
                            let buff = self.buff_id(a, lvl);
                            let bears = self.unit(t).is_some_and(|u| u.buffs.iter().any(|b| Some(b.id) == buff));
                            let fits = if code == "ANdc" { self.converts_race(a, lvl, t) } else { self.unit(t).is_some_and(|u| !crate::natives::players::is_ally(self, owner, u.owner)) };
                            if fits && self.can_cast(h, a).is_ok() && self.cast_target_error(h, a, t).is_none() && !bears {
                                self.ai_cast(h, a, t);
                            }
                        }
                        // Finger of Death (its vf3 on the watch): at an enemy that is not
                        // within 300 of the caster.
                        "ANfd" => {
                            if self.can_cast(h, a).is_ok() && (self.pos(t).0 - x).hypot(self.pos(t).1 - y) > 300.0 {
                                self.ai_cast(h, a, t);
                            }
                        }
                        _ => {}
                    }
                    if self.unit(h).is_some_and(|u| matches!(u.action, Action::Cast { .. })) {
                        break;
                    }
                }
            }
        }
    }

    /// FUN_6f603610 and the like: the census at the spell's point within its Area.
    fn point_census(&self, h: u32, a: Id, lvl: u32, x: f32, y: f32) -> Census {
        self.census(h, x, y, self.data(a, "Area", lvl), false)
    }

    /// Blizzard, Rain of Fire (FUN_6f604060): with its AI ready, on an enemy when
    /// around it more than one enemy is on the ground and the enemies are at least
    /// twice the caster's side (FUN_6f603610).
    fn ai_blizzard(&mut self, h: u32, a: Id, lvl: u32, t: u32) {
        if self.ai_resting(h, a) || self.can_cast(h, a).is_err() {
            return;
        }
        self.ai_rest(h, a);
        let (x, y) = self.pos(t);
        let c = self.point_census(h, a, lvl, x, y);
        if c.enemies - c.enemy_flying > 1 && c.allies * 2 <= c.enemies {
            self.ai_cast(h, a, t);
        }
    }

    /// Death and Decay (FUN_6f6042d0, its vf267): on an armed enemy structure when at
    /// least 2 structures stand within Area of it.
    fn ai_death_and_decay(&mut self, h: u32, a: Id, lvl: u32, t: u32) {
        if self.can_cast(h, a).is_err() {
            return;
        }
        let Some(u) = self.unit(t) else { return };
        if !u.is_building || crate::sim::unit_weapons(self, u).is_empty() {
            return;
        }
        let (x, y) = (u.x, u.y);
        if self.point_census(h, a, lvl, x, y).enemy_structures >= 2 {
            self.ai_cast(h, a, t);
        }
    }

    /// A computer's area channel re-checks its spot each tick (FUN_6f6109b0,
    /// FUN_6f610a30): Blizzard and Rain of Fire while the enemies there outnumber the
    /// caster's side; Death and Decay while structures there are not outnumbered by
    /// its side, or, with none, enemies remain and none of its side.  After 3 failures
    /// in a row it stops; returns whether it did.
    pub(crate) fn ai_channel_recheck(&mut self, h: u32, c: &mut Channel) -> bool {
        if !self.ai_controlled(h) {
            return false;
        }
        let code = self.ability_code(c.ability);
        let census = || self.point_census(h, c.ability, c.level, c.x, c.y);
        let ok = match code.as_str() {
            "AHbz" | "ANrf" => {
                let n = census();
                n.allies < n.enemies
            }
            "AUdd" => {
                let n = census();
                if n.enemy_structures == 0 { n.allies == 0 && n.enemies != 0 } else { n.enemies >= n.allies }
            }
            _ => return false,
        };
        c.ai_fails = if ok { 0 } else { c.ai_fails + 1 };
        if c.ai_fails > 2 {
            c.elapsed = c.duration + 1.0;
            if let Some(u) = self.unit_mut(h) {
                u.action = Action::Idle;
            }
            return true;
        }
        false
    }

    /// Summons (FUN_6f610120: Water Elemental, Summon Bear, Quilbeast, Hawk, Lava
    /// Spawn): past the gate, when more than 2 enemies are within 500 of the victim or
    /// the caster has half its life or less.
    fn ai_summon(&mut self, h: u32, a: Id, enemy: u32, victim: u32) {
        if self.ai_gate(h, a, enemy, victim, true, None, None) {
            return;
        }
        self.ai_rest(h, a);
        let (x, y) = self.pos(victim);
        if self.census(h, x, y, 500.0, true).enemies > 2 || self.life_fraction(h) <= 0.5 {
            self.ai_cast(h, a, 0);
        }
    }

    /// Feral Spirit (FUN_6f60f690): with its AI ready, when within 500 of the victim
    /// more than one enemy is seen and the caster's side is not the larger.
    fn ai_feral_spirit(&mut self, h: u32, a: Id, victim: u32) {
        if self.ai_resting(h, a) || self.can_cast(h, a).is_err() {
            return;
        }
        self.ai_rest(h, a);
        let (x, y) = self.pos(victim);
        let c = self.census(h, x, y, 500.0, true);
        if c.enemy_seen > 1 && c.allies <= c.enemies {
            self.ai_cast(h, a, 0);
        }
    }

    /// Phoenix (CAbilitySummonPhoenix vf64): past the gate, unless one of its phoenixes
    /// still lives.
    fn ai_phoenix(&mut self, h: u32, a: Id, lvl: u32, enemy: u32, victim: u32) {
        if self.ai_gate(h, a, enemy, victim, true, None, None) {
            return;
        }
        self.ai_rest(h, a);
        let kind = id_from_str(self.objects.abilities.get(a).map(|o| o.level_str("UnitID", lvl)).unwrap_or_default().trim());
        let owner = self.unit(h).map_or(0, |u| u.owner);
        if self.unit_order.iter().any(|&t| self.unit(t).is_some_and(|u| u.owner == owner && u.type_id == kind && !u.dead)) {
            return;
        }
        self.ai_cast(h, a, 0);
    }

    /// Watery Minion (CAbilityWateryMinion vf64): past the gate, at the enemy; when the
    /// victim is another, only with 3 enemies within 500 of it or the caster at half
    /// its life or less.
    fn ai_watery_minion(&mut self, h: u32, a: Id, enemy: u32, victim: u32) {
        if self.ai_gate(h, a, enemy, victim, true, None, None) {
            return;
        }
        if victim != h {
            self.ai_rest(h, a);
            let (x, y) = self.pos(victim);
            if self.census(h, x, y, 500.0, true).enemies < 3 && self.life_fraction(h) > 0.5 {
                return;
            }
        }
        self.ai_cast(h, a, enemy);
    }

    /// Pocket Factory, Serpent Ward (CAbilitySummonFactory, CAbilityWard vf64): past the
    /// gate, at the enemy when it costs under 11 mana; else at the victim when 5
    /// enemies are within 600 of it or the caster has half its life or less.
    fn ai_ward(&mut self, h: u32, a: Id, lvl: u32, enemy: u32, victim: u32) {
        if self.ai_gate(h, a, enemy, victim, true, None, None) {
            return;
        }
        if self.data(a, "Cost", lvl) < 11.0 {
            self.ai_cast(h, a, enemy);
            return;
        }
        self.ai_rest(h, a);
        let (x, y) = self.pos(victim);
        if self.census(h, x, y, 600.0, true).enemies < 5 && self.life_fraction(h) > 0.5 {
            return;
        }
        self.ai_cast(h, a, victim);
    }

    /// Howl of Terror (CAbilityHowlOfTerror vf64): past the gate (no creeps), at once
    /// when hard pressed; else with 3 seen enemies besides structures within Area.
    fn ai_howl(&mut self, h: u32, a: Id, lvl: u32, enemy: u32, victim: u32) {
        let buff = self.buff_id(a, lvl);
        if self.ai_gate(h, a, enemy, victim, false, None, buff) {
            return;
        }
        if !(victim == h && self.life(h) <= 400.0) {
            self.ai_rest(h, a);
            let (x, y) = self.pos(h);
            let c = self.census(h, x, y, self.data(a, "Area", lvl), false);
            if c.enemy_seen - c.enemy_structures < 3 {
                return;
            }
        }
        self.ai_cast(h, a, 0);
    }

    /// Unholy Frenzy (CAbilityUnholyFrenzy vf64): past the gate (no creeps, not on one
    /// bearing it), on the armed victim when it has twice the life the frenzy drains
    /// over its time (DataB a second for Dur) and deals at least 14 damage a second.
    fn ai_unholy_frenzy(&mut self, h: u32, a: Id, lvl: u32, enemy: u32, victim: u32) {
        let buff = self.buff_id(a, lvl);
        if self.ai_gate(h, a, enemy, victim, false, Some(victim), buff) {
            return;
        }
        let Some(v) = self.unit(victim) else { return };
        let Some((_, w)) = crate::sim::unit_weapons(self, v).into_iter().next() else { return };
        let drain = self.data(a, "Dur", lvl) * self.data(a, "DataB", lvl);
        if v.life < 2.0 * drain || w.cooldown <= 0.0 {
            return;
        }
        let (min, max) = (w.base + w.dice as f32, w.base + (w.dice * w.sides) as f32);
        if ((max - min) / 2.0 + min) / w.cooldown >= 14.0 {
            self.ai_cast(h, a, victim);
        }
    }

    /// Spirit Link (CAbilitySpiritLink vf64): past the gate (not on one linked), on a
    /// victim other than itself that is a Hero or has at least 100 life.
    fn ai_spirit_link(&mut self, h: u32, a: Id, lvl: u32, enemy: u32, victim: u32) {
        let buff = self.buff_id(a, lvl);
        if self.ai_gate(h, a, enemy, victim, true, Some(victim), buff) || victim == h {
            return;
        }
        if self.unit(victim).is_some_and(|v| !Self::real_hero(v) && v.life < 100.0) {
            return;
        }
        self.ai_cast(h, a, victim);
    }

    /// FUN_6f604b80: Lightning Shield on `t` when within Area of it 3 enemies are seen
    /// (2 when the caster has under 100 life) and the caster's side there numbers no
    /// more than `t` itself when a friend.
    fn lightning_shield_at(&mut self, h: u32, a: Id, lvl: u32, t: u32) -> bool {
        self.ai_rest(h, a);
        let (x, y) = self.pos(t);
        let c = self.census(h, x, y, self.data(a, "Area", lvl), false);
        let need = if self.life(h) < 100.0 { 2 } else { 3 };
        let owner = self.unit(h).map_or(0, |u| u.owner);
        let friend = self.unit(t).is_some_and(|u| crate::natives::players::is_ally(self, owner, u.owner)) as i32;
        if need <= c.enemy_seen && c.allies <= friend {
            self.ai_cast(h, a, t);
            return true;
        }
        false
    }

    /// Lightning Shield (CAbilityLightningShield vf64): on the victim, else on the
    /// enemy, past the gate (no creeps, not on one bearing it).
    fn ai_lightning_shield(&mut self, h: u32, a: Id, lvl: u32, enemy: u32, victim: u32) {
        let buff = self.buff_id(a, lvl);
        if !self.ai_gate(h, a, enemy, victim, false, Some(victim), buff) && self.lightning_shield_at(h, a, lvl, victim) {
            return;
        }
        if !self.ai_gate(h, a, enemy, victim, false, Some(enemy), buff) {
            self.lightning_shield_at(h, a, lvl, enemy);
        }
    }

    /// Locust Swarm (CAbilityLocustSwarm vf64): past the gate (no creeps), with 8
    /// enemies able to fight within Area (2 when the caster has 400 life or less).
    fn ai_locust_swarm(&mut self, h: u32, a: Id, lvl: u32, enemy: u32, victim: u32) {
        if self.ai_gate(h, a, enemy, victim, false, None, None) {
            return;
        }
        self.ai_rest(h, a);
        let (x, y) = self.pos(h);
        let need = if self.life(h) > 400.0 { 8 } else { 2 };
        if need <= self.census(h, x, y, self.data(a, "Area", lvl), true).enemy_fighters {
            self.ai_cast(h, a, 0);
        }
    }

    /// Starfall (CAbilityStarfall vf64, FUN_6f603520): past the gate (no creeps), with 5
    /// enemies within Area.
    fn ai_starfall(&mut self, h: u32, a: Id, lvl: u32, enemy: u32, victim: u32) {
        if self.ai_gate(h, a, enemy, victim, false, None, None) {
            return;
        }
        self.ai_rest(h, a);
        let (x, y) = self.pos(h);
        if self.census(h, x, y, self.data(a, "Area", lvl), false).enemies >= 5 {
            self.ai_cast(h, a, 0);
        }
    }

    /// Big Bad Voodoo (CAbilityVoodoo vf64, FUN_6f603580): past the gate (no creeps),
    /// with 4 of the caster's side within Area and an enemy structure there.
    fn ai_voodoo(&mut self, h: u32, a: Id, lvl: u32, enemy: u32, victim: u32) {
        if self.ai_gate(h, a, enemy, victim, false, None, None) {
            return;
        }
        self.ai_rest(h, a);
        let (x, y) = self.pos(h);
        let c = self.census(h, x, y, self.data(a, "Area", lvl), false);
        if c.allies >= 4 && c.enemy_structures > 0 {
            self.ai_cast(h, a, 0);
        }
    }

    /// Raise Dead, Carrion Scarabs, Vengeance (FUN_6f60ec20): past the gate, against a
    /// seen enemy, a Hero raises at once when a corpse lies within the spell's reach;
    /// other casters leave it to their autocast.
    fn ai_raise_dead(&mut self, h: u32, a: Id, lvl: u32, enemy: u32, victim: u32) {
        if self.ai_gate(h, a, enemy, victim, true, None, None) {
            return;
        }
        let owner = self.unit(h).map_or(0, |u| u.owner);
        if !self.unit_visible_to(enemy, owner) || !self.unit(h).is_some_and(Self::real_hero) {
            return;
        }
        self.ai_rest(h, a);
        let (x, y) = self.pos(h);
        let reach = self.data(a, "Area", lvl).max(self.data(a, "Rng", lvl));
        if !self.corpses_in(x, y, reach).is_empty() {
            self.ai_cast(h, a, 0);
        }
    }

    /// Inferno (FUN_6f60dad0, FUN_6f607470): past the gate (no creeps), at an enemy
    /// player's unit when more than one enemy on the ground is seen within Area of it.
    fn ai_inferno(&mut self, h: u32, a: Id, lvl: u32, enemy: u32, victim: u32) {
        if self.ai_gate(h, a, enemy, victim, false, None, None) || self.unit(enemy).is_none_or(|e| e.owner >= PLAYER_NEUTRAL_AGGRESSIVE) {
            return;
        }
        self.ai_rest(h, a);
        let (x, y) = self.pos(enemy);
        let c = self.census(h, x, y, self.data(a, "Area", lvl), true);
        if c.enemy_seen - c.enemy_flying > 1 {
            self.ai_cast(h, a, enemy);
        }
    }

    /// Drain (CAbilityDrain vf64): past the gate, on an enemy it may target: draining
    /// life (DataA a second for Dur) when the enemy has between half and all of it, or
    /// the caster lacks all of it and the enemy has it; else draining mana (DataB) when
    /// the caster lacks all of it and the enemy has it.
    fn ai_drain(&mut self, h: u32, a: Id, lvl: u32, enemy: u32, victim: u32) {
        if self.ai_gate(h, a, enemy, victim, true, None, None) || self.cast_target_error(h, a, enemy).is_some() {
            return;
        }
        let (Some(c), Some(e)) = (self.unit(h), self.unit(enemy)) else { return };
        let dur = self.data(a, "Dur", lvl);
        let life = self.data(a, "DataA", lvl) * dur;
        if life != 0.0 {
            let missing = c.max_life - c.life;
            if (life * 0.5 <= e.life && e.life <= life) || (life <= missing && life <= e.life) {
                self.ai_cast(h, a, enemy);
                return;
            }
        }
        let mana = self.data(a, "DataB", lvl) * dur;
        if mana == 0.0 || c.max_mana - c.mana < mana || e.mana < mana {
            return;
        }
        self.ai_cast(h, a, enemy);
    }

    /// Ensnare (CAbilityEnsnare vf64): past the gate, on a unit that is not a Hero or has
    /// under 300 life; a seen one only of an enemy player, and with 700 life or less
    /// only at 200 or less and not attacking, or above 700 only attacking from 200 or
    /// closer.
    fn ai_ensnare(&mut self, h: u32, a: Id, enemy: u32, victim: u32) {
        if self.ai_gate(h, a, enemy, victim, true, Some(enemy), None) {
            return;
        }
        let Some(e) = self.unit(enemy) else { return };
        if Self::real_hero(e) && e.life >= 300.0 {
            return;
        }
        let owner = self.unit(h).map_or(0, |u| u.owner);
        if self.unit_visible_to(enemy, owner) {
            if e.owner >= PLAYER_NEUTRAL_AGGRESSIVE {
                return;
            }
            let reach = crate::sim::unit_weapons(self, e).iter().map(|w| w.1.range).fold(-1.0f32, f32::max);
            if e.life <= 700.0 {
                if e.life > 200.0 || matches!(e.action, Action::Attack { .. }) {
                    return;
                }
            } else if reach < 0.0 || reach > 200.0 {
                return;
            }
        }
        self.ai_cast(h, a, enemy);
    }

    /// Cyclone (CAbilityCyclone vf64): able to cast, on an enemy player's unit that is
    /// not a Hero, has more than 400 life, and may be targeted.
    fn ai_cyclone(&mut self, h: u32, a: Id, enemy: u32) {
        let owner = self.unit(h).map_or(0, |u| u.owner);
        let Some(e) = self.unit(enemy) else { return };
        if self.can_cast(h, a).is_err() || Self::real_hero(e) || e.life <= 400.0 || e.owner >= PLAYER_NEUTRAL_AGGRESSIVE {
            return;
        }
        if self.cast_target_error(h, a, enemy).is_none() && !crate::natives::players::is_ally(self, owner, e.owner) {
            self.ai_cast(h, a, enemy);
        }
    }

    /// Animate Dead, Resurrection (FUN_6f603f10, FUN_6f6052f0): every 2 seconds, able to
    /// cast, it counts the corpses within Area that were not Heroes or workers (the
    /// caster's side's for Resurrection), and those of units with more than 450 maximum
    /// life (at least 450 for Resurrection): with 400 life or less one large corpse or
    /// 2 corpses do; otherwise as many as it raises (DataA).
    fn ai_raise_corpses(&mut self, h: u32, a: Id, lvl: u32, own: bool) {
        if !self.ai_controlled(h) || self.can_cast(h, a).is_err() {
            return;
        }
        let Some(owner) = self.unit(h).map(|u| u.owner) else { return };
        let (x, y) = self.pos(h);
        let (mut count, mut large) = (0, 0);
        for c in self.corpses_in(x, y, self.data(a, "Area", lvl)) {
            let Some(u) = self.unit(c) else { continue };
            if u.is_hero || self.is_classed(u, "peon") || (own && !crate::natives::players::is_ally(self, owner, u.owner)) {
                continue;
            }
            count += 1;
            if if own { u.max_life >= 450.0 } else { u.max_life > 450.0 } {
                large += 1;
            }
        }
        let go = if self.life(h) <= 400.0 { large > 0 || count >= 2 } else { count >= self.data(a, "DataA", lvl) as i32 };
        if go {
            self.ai_cast(h, a, 0);
        }
    }

    /// Taunt (CAbilityTaunt vf64): past the gate, for a victim other than itself
    /// against a unit that is no structure, with at least 300 life, and an enemy
    /// within Area.
    fn ai_taunt(&mut self, h: u32, a: Id, lvl: u32, enemy: u32, victim: u32) {
        if self.ai_gate(h, a, enemy, victim, true, None, None) || victim == h || self.unit(enemy).is_some_and(|e| e.is_building) || self.life(h) < 300.0 {
            return;
        }
        self.ai_rest(h, a);
        let (x, y) = self.pos(h);
        if self.census(h, x, y, self.data(a, "Area", lvl), true).enemies > 0 {
            self.ai_cast(h, a, 0);
        }
    }

    /// Transmute (CAbilityTransmute vf64): past the gate, on an enemy it may target and
    /// not in militia (Bmil), with at least 600 life unless hard pressed.
    fn ai_transmute(&mut self, h: u32, a: Id, enemy: u32, victim: u32) {
        if self.ai_gate(h, a, enemy, victim, true, None, None) || self.cast_target_error(h, a, enemy).is_some() {
            return;
        }
        let Some(e) = self.unit(enemy) else { return };
        if e.buffs.iter().any(|b| b.id == *b"Bmil") {
            return;
        }
        if !(victim == h && self.life(h) <= 400.0) && e.life < 600.0 {
            return;
        }
        self.ai_cast(h, a, enemy);
    }

    /// Monsoon (CAbilityMonsoon vf64): past the gate (no creeps), at the enemy when more
    /// than 4 enemies on the ground are within Area of it.
    fn ai_monsoon(&mut self, h: u32, a: Id, lvl: u32, enemy: u32, victim: u32) {
        if self.ai_gate(h, a, enemy, victim, false, None, None) {
            return;
        }
        self.ai_rest(h, a);
        let (x, y) = self.pos(enemy);
        let c = self.census(h, x, y, self.data(a, "Area", lvl), false);
        if c.enemies - c.enemy_flying > 4 {
            self.ai_cast(h, a, enemy);
        }
    }

    /// Stampede (CAbilityStampede vf64, FUN_6f6034c0): past the gate (no creeps), at the
    /// enemy with 5 enemies within Area of the caster.
    fn ai_stampede(&mut self, h: u32, a: Id, lvl: u32, enemy: u32, victim: u32) {
        if self.ai_gate(h, a, enemy, victim, false, None, None) {
            return;
        }
        self.ai_rest(h, a);
        let (x, y) = self.pos(h);
        if self.census(h, x, y, self.data(a, "Area", lvl), true).enemies >= 5 {
            self.ai_cast(h, a, enemy);
        }
    }

    /// Mana Shield (CAbilityManaShield vf64): past the gate, for itself: with S the life
    /// its mana beyond the cost would shield ((1 − DataB) × (mana − cost) ÷ (DataB ÷
    /// DataA), at least 100), raised once its life falls to S, lowered once above
    /// S + 100.
    fn ai_mana_shield(&mut self, h: u32, a: Id, lvl: u32, enemy: u32, victim: u32) {
        if self.ai_gate(h, a, enemy, victim, true, None, None) || victim != h {
            return;
        }
        let (per, part) = (self.data(a, "DataA", lvl), self.data(a, "DataB", lvl));
        if per == 0.0 || part == 0.0 {
            return;
        }
        let cost = self.data(a, "Cost", lvl);
        let mana = self.unit(h).map_or(0.0, |u| u.mana);
        let shield = ((1.0 - part) * (mana - cost) / (part / per)).max(100.0);
        let on = self.unit(h).is_some_and(|u| u.buffs.iter().any(|b| b.ability == a));
        let life = self.life(h);
        // Casting it raises or lowers it.
        if (!on && life <= shield) || (on && life > shield + 100.0) {
            self.ai_cast(h, a, 0);
        }
    }

    /// Rejuvenation (CAbilityRejuvination vf64, FUN_6f603af0): past the gate, on a
    /// victim without it that lacks at least DataA life, or has 200 or less of between
    /// 400 and 600.
    fn ai_rejuvenation(&mut self, h: u32, a: Id, lvl: u32, enemy: u32, victim: u32) {
        if self.ai_gate(h, a, enemy, victim, true, Some(victim), None) {
            return;
        }
        self.ai_rest(h, a);
        let Some(v) = self.unit(victim) else { return };
        if v.buffs.iter().any(|b| b.id == *b"Brej") {
            return;
        }
        let small = v.life <= 200.0 && (400.0..=600.0).contains(&v.max_life);
        if small || v.max_life - v.life >= self.data(a, "DataA", lvl) {
            self.ai_cast(h, a, victim);
        }
    }

    /// Cluster Rockets (CAbilityClusterRockets vf64): past the gate, at the enemy when
    /// within Area of it 3 enemies (2 when hard pressed) are seen.
    fn ai_cluster_rockets(&mut self, h: u32, a: Id, lvl: u32, enemy: u32, victim: u32) {
        if self.ai_gate(h, a, enemy, victim, true, Some(enemy), None) {
            return;
        }
        self.ai_rest(h, a);
        let need = if self.hard_pressed(h, victim) { 2 } else { 3 };
        let (x, y) = self.pos(enemy);
        if need <= self.census(h, x, y, self.data(a, "Area", lvl), false).enemy_seen {
            self.ai_cast(h, a, enemy);
        }
    }

    /// Absorb Mana (CAbilityAbsorb vf64): past the gate, from a victim that is no Hero
    /// with 100 life or less and at least 10 mana.
    fn ai_absorb(&mut self, h: u32, a: Id, enemy: u32, victim: u32) {
        if self.ai_gate(h, a, enemy, victim, true, Some(victim), None) {
            return;
        }
        if self.unit(victim).is_some_and(|v| !Self::real_hero(v) && v.life <= 100.0 && v.mana >= 10.0) {
            self.ai_cast(h, a, victim);
        }
    }

    /// Charm (CAbilityCharm vf64): past the gate, on an enemy it may target and not in
    /// militia (Bmil), with more than 400 life unless hard pressed.
    fn ai_charm(&mut self, h: u32, a: Id, enemy: u32, victim: u32) {
        if self.ai_gate(h, a, enemy, victim, true, None, None) || self.cast_target_error(h, a, enemy).is_some() {
            return;
        }
        let Some(e) = self.unit(enemy) else { return };
        if e.buffs.iter().any(|b| b.id == *b"Bmil") || (!(victim == h && self.life(h) <= 400.0) && e.life <= 400.0) {
            return;
        }
        self.ai_cast(h, a, enemy);
    }

    /// Whether a unit bears buffs its friends laid (UnitHasBuffsEx's positive buffs).
    fn has_friendly_buff(&self, h: u32) -> bool {
        let Some(u) = self.unit(h) else { return false };
        u.buffs.iter().any(|b| b.remaining > 0.5 && b.caster != 0 && self.unit(b.caster).is_some_and(|c| crate::natives::players::is_ally(self, u.owner, c.owner)))
    }

    /// Devour Magic (CAbilityDevourMagic vf64): past the gate, cleansing the victim when
    /// only enemy spells are on it; else on an enemy that is summoned or bears its
    /// friends' spells, unless the caster has 200 life and 100 mana while the enemy
    /// bears enemy spells already.
    fn ai_devour_magic(&mut self, h: u32, a: Id, enemy: u32, victim: u32) {
        if self.ai_gate(h, a, enemy, victim, true, None, None) {
            return;
        }
        if self.has_hostile_buff(victim) && !self.has_friendly_buff(victim) {
            self.ai_cast(h, a, victim);
            return;
        }
        let summoned = self.unit(enemy).is_some_and(|e| self.is_classed(e, "summoned"));
        if !summoned && !self.has_friendly_buff(enemy) {
            return;
        }
        let strong = self.life(h) >= 200.0 && self.unit(h).is_some_and(|u| u.mana >= 100.0);
        if strong && self.has_hostile_buff(enemy) {
            return;
        }
        self.ai_cast(h, a, enemy);
    }

    /// Forked Lightning (CAbilityForkedLightning vf64): past the gate, on an enemy it may
    /// target with at least DataA life when its fork would reach DataB enemies (1 for a
    /// Hero or when hard pressed) around it.
    fn ai_forked_lightning(&mut self, h: u32, a: Id, lvl: u32, enemy: u32, victim: u32) {
        if self.ai_gate(h, a, enemy, victim, true, None, None) || self.life(enemy) < self.data(a, "DataA", lvl) || self.cast_target_error(h, a, enemy).is_some() {
            return;
        }
        let mut need = self.data(a, "DataB", lvl) as i32;
        if self.unit(enemy).is_some_and(Self::real_hero) || self.hard_pressed(h, victim) {
            need = 1;
        }
        self.ai_rest(h, a);
        let (x, y) = self.pos(enemy);
        if need <= self.census(h, x, y, self.data(a, "Area", lvl), false).enemies {
            self.ai_cast(h, a, enemy);
        }
    }

    /// Possession (FUN_6f60e810): past the gate, on an enemy not in militia (Bmil), by a
    /// caster without Curse and Anti-magic Shell that is not at full life.
    fn ai_possession(&mut self, h: u32, a: Id, enemy: u32, victim: u32) {
        if self.ai_gate(h, a, enemy, victim, true, Some(enemy), None) || self.unit(enemy).is_some_and(|e| e.buffs.iter().any(|b| b.id == *b"Bmil")) {
            return;
        }
        if self.ability_with_code(h, "Acrs").is_none() && self.ability_with_code(h, "Aams").is_none() && self.unit(h).is_some_and(|u| u.life != u.max_life) {
            self.ai_cast(h, a, enemy);
        }
    }

    /// Shadow Strike, Soul Burn (their vf64): able to cast, against an enemy (Soul Burn
    /// only a Hero) without the spell's buff, with the victim on its side: the damage
    /// over the spell's time (Shadow Strike DataA every Cast seconds plus DataE, Soul
    /// Burn DataA every DataB seconds; over HeroDur for Heroes) must leave the enemy
    /// between 0 and 2000 life (200 and 400 for Shadow Strike on others) unless it is
    /// an unseen flyer, and it must be within the cast range plus 100 (200 for Shadow
    /// Strike on others).
    fn ai_lingering(&mut self, h: u32, a: Id, lvl: u32, enemy: u32, victim: u32, soul_burn: bool) {
        let Some(owner) = self.unit(h).map(|u| u.owner) else { return };
        let Some(e) = self.unit(enemy) else { return };
        if self.can_cast(h, a).is_err() || crate::natives::players::is_ally(self, owner, e.owner) {
            return;
        }
        if self.unit(victim).is_none_or(|v| !crate::natives::players::is_ally(self, owner, v.owner)) {
            return;
        }
        if let Some(b) = self.buff_id(a, lvl) {
            if e.buffs.iter().any(|x| x.id == b) {
                return;
            }
        }
        let hero = Self::real_hero(e);
        if soul_burn && !hero {
            return;
        }
        let unseen_flyer = e.fly_height > 1.0 && !self.unit_visible_to(enemy, owner);
        if !unseen_flyer {
            let dur = if self.hero_duration(enemy) { self.data(a, "HeroDur", lvl) } else { self.data(a, "Dur", lvl) };
            let every = if soul_burn { self.data(a, "DataB", lvl) } else { self.data(a, "Cast", lvl) };
            let ticks = if every > 0.0 { (dur / every) as i32 } else { 0 };
            let mut total = self.data(a, "DataA", lvl) * ticks as f32;
            if !soul_burn {
                total += self.data(a, "DataE", lvl);
            }
            let (lo, hi) = if soul_burn || hero { (total, total + 2000.0) } else { (total + 200.0, total + 400.0) };
            if e.life < lo || hi < e.life {
                return;
            }
        }
        let reach = self.data(a, "Rng", lvl) + if !soul_burn && !hero { 200.0 } else { 100.0 };
        let ((cx, cy), (ex, ey)) = (self.pos(h), self.pos(enemy));
        if (ex - cx).hypot(ey - cy) <= reach {
            self.ai_cast(h, a, enemy);
        }
    }

    /// Held items' AI (their abilities' vf64), each use queued for the next tick:
    /// figurines with more than 2 seen enemies (any enemies for flyers) within 500 of
    /// the victim and the caster's side not the larger; Web past the gate; dispel
    /// wands on the victim, else on the enemy; the defense scroll against a
    /// player's unit fighting another of the caster's units; the Moonstone by a night
    /// elf by day.
    fn item_ai(&mut self, h: u32, enemy: u32, victim: u32) {
        let Some(u) = self.unit(h) else { return };
        let owner = u.owner;
        let slots: Vec<(usize, u32)> = u.inventory.iter().copied().enumerate().filter(|&(_, i)| i != 0).collect();
        for (slot, item) in slots {
            if self.ai_item_uses.iter().any(|q| q.0 == h) {
                return;
            }
            let Some(t) = self.item(item).map(|i| i.type_id) else { continue };
            for a in self.item_abilities(t) {
                if self.unit(h).is_some_and(|u| u.cooldowns.get(&a).copied().unwrap_or(0.0) > 0.0) {
                    continue;
                }
                let enemy_player = self.unit(enemy).is_some_and(|e| e.owner < PLAYER_NEUTRAL_AGGRESSIVE && !crate::natives::players::is_ally(self, owner, e.owner));
                let target = match self.ability_code(a).as_str() {
                    "AIfg" | "AIfu" | "AIfh" | "AIff" | "AIfd" | "AIfr" | "AIfs" if enemy_player && !self.ai_resting(h, a) => {
                        self.ai_rest(h, a);
                        let (x, y) = self.pos(victim);
                        let c = self.census(h, x, y, 500.0, true);
                        let flyer = self.ability_code(a) == "AIfd";
                        let seen = if flyer { c.enemies } else { c.enemy_seen };
                        (seen > 2 && c.allies <= c.enemies).then_some(0)
                    }
                    "AIwb" => (!self.ai_gate(h, a, enemy, victim, true, Some(enemy), None)).then_some(enemy),
                    "AIdc" => {
                        if !self.ai_gate(h, a, enemy, victim, true, Some(victim), None) {
                            Some(victim)
                        } else {
                            (!self.ai_gate(h, a, enemy, victim, false, Some(enemy), None)).then_some(enemy)
                        }
                    }
                    "AIda" => {
                        let other = victim != h && self.unit(victim).zip(self.unit(enemy)).is_some_and(|(v, e)| !crate::natives::players::is_ally(self, v.owner, e.owner));
                        (enemy_player && other).then_some(0)
                    }
                    "AIct" => {
                        let elf = self.players.get(owner as usize).is_some_and(|p| p.race == 4);
                        let day = (6.0..18.0).contains(&self.time_of_day);
                        (enemy_player && elf && day && self.unit(victim).is_some_and(|v| crate::natives::players::is_ally(self, owner, v.owner))).then_some(0)
                    }
                    _ => None,
                };
                if let Some(target) = target {
                    self.ai_item_uses.push((h, slot, target));
                    return;
                }
            }
        }
    }
}

/// Abilities whose AI watches the units about the caster.
const WATCHED: [&str; 6] = ["AHbz", "ANrf", "AUdd", "ANfd", "ANdc", "ANsl"];

/// Abilities whose AI looks every 2 seconds (their vf50 timer, 0x6fd6fbb0).
const EVERY_2S: [&str; 2] = ["AUan", "AHre"];
