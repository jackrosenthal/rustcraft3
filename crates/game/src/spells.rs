//! Spell effects by ability code, with field meanings from AbilityMetaData: channeled
//! waves, ground effects, lines, chains, summons, corpses, and buffs.

use w3_jass::Vm;

use crate::objects::{Id, id_from_str};
use crate::types::*;
use crate::world::World;

/// Codes whose effect keeps the caster channeling.
pub fn channeled(code: &str) -> bool {
    matches!(code, "AHbz" | "ANrf" | "AEsf" | "AEtq" | "AUdd" | "AHdr" | "AOvd" | "AOeq" | "ANst" | "ANvc" | "AHmt")
}

impl World {
    fn d(&self, a: Id, field: &str, lvl: u32) -> f32 {
        self.data(a, field, lvl)
    }

    fn buff_of(&self, a: Id, lvl: u32) -> Id {
        id_from_str(self.objects.abilities.get(a).map(|o| o.level_str("BuffID", lvl)).unwrap_or_default().split(',').next().unwrap_or("").trim())
    }

    pub(crate) fn duration_for(&self, a: Id, lvl: u32, target: u32) -> f32 {
        if self.hero_duration(target) { self.d(a, "HeroDur", lvl) } else { self.d(a, "Dur", lvl) }
    }

    fn owner_of(&self, h: u32) -> u8 {
        self.unit(h).map_or(0, |u| u.owner)
    }

    pub(crate) fn units_in_pub(&self, x: f32, y: f32, r: f32) -> Vec<u32> {
        self.units_in(x, y, r)
    }

    pub(crate) fn allies_in_pub(&self, owner: u8, x: f32, y: f32, r: f32) -> Vec<u32> {
        self.allies_in(owner, x, y, r)
    }

    fn units_in(&self, x: f32, y: f32, r: f32) -> Vec<u32> {
        self.unit_order.iter().copied().filter(|&h| self.unit(h).is_some_and(|u| !u.dead && !u.hidden && (u.x - x).hypot(u.y - y) <= r)).collect()
    }

    pub(crate) fn allies_in(&self, owner: u8, x: f32, y: f32, r: f32) -> Vec<u32> {
        self.units_in(x, y, r).into_iter().filter(|&h| crate::natives::players::is_ally(self, owner, self.owner_of(h))).collect()
    }

    /// Corpses (dead non-hero, non-structure units) near a point, nearest first.
    pub fn corpses_in(&self, x: f32, y: f32, r: f32) -> Vec<u32> {
        let mut c: Vec<(f32, u32)> = self
            .unit_order
            .iter()
            .copied()
            .filter_map(|h| self.unit(h).filter(|u| u.dead && !u.is_hero && !u.is_building && !u.hidden).map(|u| ((u.x - x).hypot(u.y - y), h)))
            .filter(|(d, _)| *d <= r)
            .collect();
        c.sort_by(|a, b| a.0.total_cmp(&b.0));
        c.into_iter().map(|c| c.1).collect()
    }

    /// Creates summoned units with a timed life around a point; each raises the summon
    /// event (the summoned unit triggering, the summoner as GetSummoningUnit).
    pub fn summon(&mut self, summoner: u32, owner: u8, unit: Id, n: i32, x: f32, y: f32, life: f32) -> Vec<u32> {
        let mut out = Vec::new();
        for i in 0..n.max(0) {
            let ang = i as f32 / n.max(1) as f32 * std::f32::consts::TAU;
            let r = if n > 1 { 100.0 } else { 0.0 };
            let s = crate::natives::units::create_unit(self, owner, unit, x + r * ang.cos(), y + r * ang.sin(), 270.0);
            if s != 0 {
                if let Some(u) = self.unit_mut(s) {
                    if life > 0.0 {
                        u.timed_life = Some(life);
                    }
                }
                use crate::event_ids::*;
                self.queue_unit_event(s, EVENT_PLAYER_UNIT_SUMMON, EVENT_UNIT_SUMMON, EventCtx { other_unit: summoner, ..Default::default() });
                out.push(s);
            }
        }
        out
    }

    /// Units along a line from `a` toward `b`, within a width growing from `w0` to `w1`.
    pub(crate) fn line_units_pub(&self, owner: u8, a: (f32, f32), b: (f32, f32), w0: f32, w1: f32) -> Vec<u32> {
        self.line_units(owner, a, b, w0, w1)
    }

    pub(crate) fn capped_damage_pub(&mut self, vm: &mut Vm, source: u32, targets: &[u32], each: f32, max: f32) {
        self.capped_damage(vm, source, targets, each, max)
    }

    fn line_units(&self, owner: u8, a: (f32, f32), b: (f32, f32), w0: f32, w1: f32) -> Vec<u32> {
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let len = dx.hypot(dy).max(1.0);
        let (ux, uy) = (dx / len, dy / len);
        self.unit_order
            .iter()
            .copied()
            .filter(|&h| {
                let Some(u) = self.unit(h) else { return false };
                if u.dead || u.hidden || crate::natives::players::is_ally(self, owner, u.owner) {
                    return false;
                }
                let (px, py) = (u.x - a.0, u.y - a.1);
                let t = px * ux + py * uy;
                if !(0.0..=len).contains(&t) {
                    return false;
                }
                let side = (px * uy - py * ux).abs();
                side <= (w0 + (w1 - w0) * t / len) / 2.0 + u.collision
            })
            .collect()
    }

    /// Damage to several targets with a cap on the total: each takes `each`, or an even
    /// share of `max` when that is less.
    fn capped_damage(&mut self, vm: &mut Vm, source: u32, targets: &[u32], each: f32, max: f32) {
        if targets.is_empty() {
            return;
        }
        let per = if max > 0.0 { each.min(max / targets.len() as f32) } else { each };
        for &t in targets {
            self.spell_damage(vm, source, t, per);
        }
    }

    pub(crate) fn art_at(&mut self, a: Id, key: &str, x: f32, y: f32) {
        let m = self.profile_art(a, key);
        self.spawn_effect(&m, x, y);
    }

    /// The effect of a spell this module knows; false when the code is not handled here.
    pub(crate) fn spell_effect(&mut self, vm: &mut Vm, h: u32, a: Id, lvl: u32, code: &str, target: u32, (x, y): (f32, f32)) -> bool {
        let owner = self.owner_of(h);
        let area = self.d(a, "Area", lvl);
        let buff = self.buff_of(a, lvl);
        let dur = self.duration_for(a, lvl, target);
        let (cx, cy) = self.unit(h).map_or((x, y), |u| (u.x, u.y));
        let (tx, ty) = self.widget_pos(target).unwrap_or((x, y));
        match code {
            // Channeled waves.
            "AHbz" | "ANrf" | "AEsf" | "AEtq" | "AUdd" | "AHdr" | "AOvd" | "AOeq" | "ANst" => {
                let waves = match code {
                    "AHbz" | "ANrf" => self.d(a, "DataA", lvl).max(1.0) as u32,
                    _ => 0,
                };
                // Self-sustained spells last the caster's duration (HeroDur for heroes);
                // Blizzard's waves come one per Cast time.
                let cast = self.d(a, "Cast", lvl).max(0.1);
                let duration = if code == "AHbz" || code == "ANrf" { waves as f32 * cast } else { self.duration_for(a, lvl, h) };
                let interval = match code {
                    "AHbz" | "ANrf" => cast,
                    "AEsf" | "AEtq" => self.d(a, "DataB", lvl).max(0.1),
                    "AHdr" => self.d(a, "DataC", lvl).max(0.1),
                    // Stampede: DataA beasts a second.
                    "ANst" => 1.0 / self.d(a, "DataA", lvl).max(0.01),
                    _ => 1.0,
                };
                let (px, py) = if code == "AOvd" || code == "AEtq" || code == "AEsf" { (cx, cy) } else { (x, y) };
                // Drain: the lightning of what it takes (LightningEffect: both, life,
                // mana).
                let link = if code == "AHdr" {
                    let (life, mana) = (self.d(a, "DataA", lvl) > 0.0, self.d(a, "DataB", lvl) > 0.0);
                    self.channel_link_nth(a, if life && mana { 0 } else if life { 1 } else { 2 }, h, target)
                } else {
                    0
                };
                // Drain's first transfer comes an interval after the cast, as in the
                // original.
                let next = if code == "AHdr" { interval } else { 0.0 };
                if let Some(u) = self.unit_mut(h) {
                    u.channel = Some(Channel { ability: a, level: lvl, target, x: px, y: py, elapsed: 0.0, duration, interval, next, waves, link, held: [0; 4], lanes: 0, ai_fails: 0, strikes: Vec::new(), struck: Vec::new() });
                }
                true
            }
            // Volcano: DataB waves DataC apart, the first a second after the cast
            // (volcano.rs).
            "ANvc" => {
                let every = self.d(a, "DataC", lvl).max(0.1);
                let duration = self.d(a, "DataB", lvl) * every;
                self.begin_volcano(h, a, lvl, x, y);
                if let Some(u) = self.unit_mut(h) {
                    u.channel = Some(Channel { ability: a, level: lvl, target: 0, x, y, elapsed: 0.0, duration, interval: every, next: 1.0, waves: 0, link: 0, held: [0; 4], lanes: 0, ai_fails: 0, strikes: Vec::new(), struck: Vec::new() });
                }
                true
            }
            // Cloud: while the caster channels, for Dur, structures within Area stop
            // their DataA attacks (2: ranged).
            "Aclf" => {
                let duration = self.d(a, "Dur", lvl);
                self.effect_at(a, lvl, x, y, duration);
                if let Some(u) = self.unit_mut(h) {
                    u.channel = Some(Channel { ability: a, level: lvl, target: 0, x, y, elapsed: 0.0, duration, interval: 0.25, next: 0.0, waves: 0, link: 0, held: [0; 4], lanes: 0, ai_fails: 0, strikes: Vec::new(), struck: Vec::new() });
                }
                true
            }
            // Aerial Shackles: the target is held and takes DataA a second while the
            // caster channels, for Dur (HeroDur on Heroes), tied by its lightning.
            "Amls" => {
                let duration = self.duration_for(a, lvl, target);
                // BuffID: the caster's buff (Bmlc), then the target's (Bmlt).
                let buffs = self.objects.abilities.get(a).map(|o| o.level_str("BuffID", lvl)).unwrap_or_default();
                let ids: Vec<Id> = buffs.split(',').map(|b| crate::objects::id_from_str(b.trim())).collect();
                let held = ids.get(1).or(ids.first()).copied().unwrap_or([0; 4]);
                if let Some(&mine) = ids.first().filter(|_| ids.len() > 1) {
                    self.apply_buff(h, Buff { id: mine, ability: a, level: lvl, caster: h, remaining: duration, ..Default::default() });
                }
                self.apply_buff(target, Buff { id: held, ability: a, level: lvl, caster: h, remaining: duration, stun: true, dps: self.d(a, "DataA", lvl), ..Default::default() });
                let link = self.channel_link(a, h, target);
                if let Some(u) = self.unit_mut(h) {
                    u.channel = Some(Channel { ability: a, level: lvl, target, x, y, elapsed: 0.0, duration, interval: duration.max(0.1) + 1.0, next: duration.max(0.1) + 1.0, waves: 0, link, held, lanes: 0, ai_fails: 0, strikes: Vec::new(), struck: Vec::new() });
                }
                true
            }
            // Flame Strike: DataA every DataB seconds for HeroDur, then DataC every DataD
            // seconds until Dur.
            // Monsoon: DataA every DataB for Dur to what it allows within Area,
            // structures taking DataC of it.
            "ANmo" => {
                let duration = self.d(a, "Dur", lvl);
                let interval = self.d(a, "DataB", lvl).max(0.05);
                self.ground_effects.push(GroundEffect { caster: h, owner, ability: a, level: lvl, x, y, elapsed: 0.0, duration, interval, next: 0.0, effect: 0 });
                self.art_at(a, "EffectArt", x, y);
                true
            }
            "AHfs" => {
                let duration = self.d(a, "Dur", lvl);
                let interval = self.d(a, "DataB", lvl).max(0.05);
                self.ground_effects.push(GroundEffect { caster: h, owner, ability: a, level: lvl, x, y, elapsed: 0.0, duration, interval, next: 0.0, effect: 0 });
                self.art_at(a, "EffectArt", x, y);
                true
            }
            // Bladestorm and Immolation follow the caster.
            "AOww" | "AEim" => {
                let duration = if code == "AEim" { f32::MAX } else { self.duration_for(a, lvl, h) };
                self.ground_effects.push(GroundEffect { caster: h, owner, ability: a, level: lvl, x: cx, y: cy, elapsed: 0.0, duration, interval: 1.0, next: 0.0, effect: 0 });
                if code == "AOww" {
                    self.apply_buff(h, Buff { id: buff, ability: a, level: lvl, caster: h, remaining: duration, magic_immune: true, ..Default::default() });
                }
                true
            }
            // Lines: Shockwave, Carrion Swarm, Impale.
            "AOsh" | "AUcs" | "AUim" => {
                let (len, w1, each, max) = match code {
                    "AUim" => (self.d(a, "DataA", lvl), area, self.d(a, "DataC", lvl), 0.0),
                    _ => (self.d(a, "DataC", lvl), self.d(a, "DataD", lvl), self.d(a, "DataA", lvl), self.d(a, "DataB", lvl)),
                };
                let (dx, dy) = (tx - cx, ty - cy);
                let l = dx.hypot(dy).max(1.0);
                let end = (cx + dx / l * len, cy + dy / l * len);
                let hit = self.line_units(owner, (cx, cy), end, area, w1.max(area));
                self.capped_damage(vm, h, &hit, each, max);
                if code == "AUim" {
                    // Impaled units are thrown up, stunned for the air time plus Dur.
                    let air = self.d(a, "DataD", lvl);
                    for t in hit {
                        let d = self.duration_for(a, lvl, t) + air;
                        self.apply_buff(t, Buff { id: buff, ability: a, level: lvl, caster: h, remaining: d, stun: true, ..Default::default() });
                    }
                }
                for k in 1..=4 {
                    let f = k as f32 / 4.0;
                    self.art_at(a, "Missileart", cx + (end.0 - cx) * f, cy + (end.1 - cy) * f);
                }
                true
            }
            // Chains: Chain Lightning and Healing Wave jump to the nearest new target,
            // losing a fraction per jump.
            // Jumps come a moment apart, each with its bolt (bolts.rs).
            "AOcl" | "AOhw" => {
                let (amount, jumps) = (self.d(a, "DataA", lvl), self.d(a, "DataB", lvl).max(1.0) as usize);
                self.start_jumps(vm, a, lvl, h, target, amount, jumps, code == "AOhw");
                true
            }
            // Mana Burn: drains up to DataA mana and deals as much damage.
            "AEmb" => {
                self.ability_bolt(a, true, h, target);
                let drained = self.unit(target).map_or(0.0, |t| t.mana.min(self.d(a, "DataA", lvl)));
                if let Some(t) = self.unit_mut(target) {
                    t.mana -= drained;
                }
                self.spell_damage(vm, h, target, drained);
                true
            }
            // Shadow Strike: initial damage, then decaying damage over time with a slow.
            "AEsh" => {
                self.spell_damage(vm, h, target, self.d(a, "DataE", lvl));
                let dps = self.d(a, "DataA", lvl) / 3.0;
                let b = Buff { id: buff, ability: a, level: lvl, caster: h, remaining: dur, dps, move_factor: -self.d(a, "DataB", lvl), attack_factor: -self.d(a, "DataC", lvl), ..Default::default() };
                self.apply_buff(target, b);
                true
            }
            // Blink: to the point, no farther than the maximum range.
            "AEbl" => {
                let max = self.d(a, "DataA", lvl);
                let (dx, dy) = (x - cx, y - cy);
                let dd = dx.hypot(dy);
                let k = if dd > max && dd > 0.0 { max / dd } else { 1.0 };
                self.art_at(a, "CasterArt", cx, cy);
                if let Some(u) = self.unit_mut(h) {
                    u.x = cx + dx * k;
                    u.y = cy + dy * k;
                    u.path.clear();
                }
                self.art_at(a, "TargetArt", cx + dx * k, cy + dy * k);
                true
            }
            // Fan of Knives: damage to enemies around, capped in total and count.
            "AEfk" => {
                let mut t = self.enemies_in(owner, cx, cy, area);
                let n = self.d(a, "DataC", lvl) as usize;
                if n > 0 {
                    t.truncate(n);
                }
                self.capped_damage(vm, h, &t, self.d(a, "DataA", lvl), self.d(a, "DataB", lvl));
                true
            }
            // Dark Ritual and Death Pact: sacrifice a friendly unit for mana or life.
            "AUdr" | "AUdp" => {
                let life = self.unit(target).map_or(0.0, |t| t.life);
                let (to_mana, to_life) = (self.d(a, "DataA", lvl), self.d(a, "DataB", lvl));
                let (mana_pct, life_pct) = (self.d(a, "DataC", lvl) != 0.0, self.d(a, "DataD", lvl) != 0.0);
                let mana_gain = if mana_pct { life * to_mana } else { to_mana };
                let life_gain = if life_pct { life * to_life } else { to_life };
                crate::natives::units::kill_unit(self, vm, target, h);
                if let Some(u) = self.unit_mut(h) {
                    u.mana = (u.mana + mana_gain).min(u.max_mana);
                    u.life = (u.life + life_gain).min(u.max_life);
                }
                true
            }
            "AUsl" => {
                self.apply_buff(target, Buff { id: buff, ability: a, level: lvl, caster: h, remaining: dur, sleep: true, stun: true, ..Default::default() });
                true
            }
            "AHbn" => {
                let b = Buff { id: buff, ability: a, level: lvl, caster: h, remaining: dur, ethereal: true, move_factor: -self.d(a, "DataA", lvl), attack_factor: -self.d(a, "DataB", lvl), ..Default::default() };
                self.apply_buff(target, b);
                true
            }
            "AHav" => {
                let d = self.d(a, "Dur", lvl).max(self.d(a, "HeroDur", lvl));
                let b = Buff { id: buff, ability: a, level: lvl, caster: h, remaining: d, armor: self.d(a, "DataA", lvl), life_bonus: self.d(a, "DataB", lvl), damage_bonus: self.d(a, "DataC", lvl), magic_immune: true, ..Default::default() };
                let gain = b.life_bonus;
                self.apply_buff(h, b);
                if let Some(u) = self.unit_mut(h) {
                    u.max_life += gain;
                    u.life += gain;
                }
                true
            }
            "AOwk" => {
                let d = self.d(a, "Dur", lvl).max(self.d(a, "HeroDur", lvl));
                let b = Buff { id: buff, ability: a, level: lvl, caster: h, remaining: d, invisible: true, move_factor: self.d(a, "DataB", lvl), backstab: self.d(a, "DataC", lvl), ..Default::default() };
                self.apply_buff(h, b);
                true
            }
            "AUfa" | "AUfu" => {
                let b = Buff { id: buff, ability: a, level: lvl, caster: h, remaining: self.d(a, "DataA", lvl), armor: self.d(a, "DataB", lvl), ..Default::default() };
                self.apply_buff(target, b);
                true
            }
            // Summons.
            "AOsf" | "AHwe" | "AHpx" | "AEst" | "AEsv" | "AOwd" | "AOsw" => {
                let unit = id_from_str(self.objects.abilities.get(a).map(|o| o.level_str("UnitID", lvl)).unwrap_or_default().trim());
                let n = match code {
                    "AOsf" => self.d(a, "DataB", lvl),
                    _ => self.d(a, "DataA", lvl),
                }
                .max(1.0) as i32;
                let (sx, sy) = if code == "AOsw" || code == "AOwd" { (x, y) } else { (cx, cy) };
                let life = self.d(a, "Dur", lvl);
                self.summon(h, owner, unit, n, sx, sy, life);
                true
            }
            // Force of Nature: trees in the area become treants.
            "AEfn" => {
                let unit = id_from_str(self.objects.abilities.get(a).map(|o| o.level_str("UnitID", lvl)).unwrap_or_default().trim());
                let n = self.d(a, "DataA", lvl).max(1.0) as usize;
                let trees: Vec<u32> = self.destructable_order.iter().copied().filter(|&t| self.is_tree_pub(t) && self.widget_pos(t).is_some_and(|p| (p.0 - x).hypot(p.1 - y) <= area)).take(n).collect();
                let life = self.d(a, "Dur", lvl);
                for t in trees {
                    let (px, py) = self.widget_pos(t).unwrap();
                    crate::natives::objects::kill_destructable_pub(self, vm, t);
                    self.summon(h, owner, unit, 1, px, py, life);
                }
                true
            }
            // Inferno: damage and a stun where it lands, then the infernal.
            "AUin" => {
                for t in self.enemies_in(owner, x, y, area) {
                    self.spell_damage(vm, h, t, self.d(a, "DataA", lvl));
                    let d = self.duration_for(a, lvl, t);
                    self.apply_buff(t, Buff { id: buff, ability: a, level: lvl, caster: h, remaining: d, stun: true, ..Default::default() });
                }
                let unit = id_from_str(self.objects.abilities.get(a).map(|o| o.level_str("UnitID", lvl)).unwrap_or_default().trim());
                let life = self.d(a, "DataB", lvl);
                self.summon(h, owner, unit, 1, x, y, life);
                true
            }
            // Corpses: Animate Dead raises any, Resurrection friendly ones.
            "AUan" | "AHre" => {
                let n = self.d(a, "DataA", lvl).max(1.0) as usize;
                let corpses: Vec<u32> = self
                    .corpses_in(cx, cy, area.max(900.0))
                    .into_iter()
                    .filter(|&c| code == "AUan" || crate::natives::players::is_ally(self, owner, self.owner_of(c)))
                    .take(n)
                    .collect();
                let life = if code == "AUan" { self.d(a, "Dur", lvl) } else { 0.0 };
                // DataB: the raised are invulnerable, under the ability's buff (BUan).
                let invulnerable = self.d(a, "DataB", lvl) != 0.0;
                let buff = self.buff_of(a, lvl);
                for c in corpses {
                    let Some(u) = self.unit(c) else { continue };
                    let (t, px, py, f) = (u.type_id, u.x, u.y, u.facing);
                    crate::natives::units::remove_unit(self, c);
                    let r = crate::natives::units::create_unit(self, owner, t, px, py, f);
                    if let Some(ru) = self.unit_mut(r) {
                        if life > 0.0 {
                            ru.timed_life = Some(life);
                        }
                        ru.extra.animated = invulnerable;
                    }
                    if invulnerable && buff != [0; 4] && r != 0 {
                        let remaining = if life > 0.0 { life } else { f32::INFINITY };
                        self.apply_buff(r, Buff { id: buff, ability: a, level: lvl, caster: h, remaining, invulnerable: true, ..Default::default() });
                    }
                    self.art_at(a, "TargetArt", px, py);
                }
                true
            }
            // Mass Teleport: the caster and units around it to a friendly unit.
            // Mass Teleport: the caster channels DataB seconds, then it and up to DataA of
            // its owner's units within Area go to the target (measured: a Staff of
            // Teleportation's hero arrives 3.06 seconds after it is used).
            "AHmt" => {
                let delay = self.d(a, "DataB", lvl).max(0.01);
                if let Some(u) = self.unit_mut(h) {
                    // The teleport comes when the delay is up; the channel lasts a step
                    // longer so that its last tick is not cut off.
                    u.channel = Some(Channel { ability: a, level: lvl, target, x, y, elapsed: 0.0, duration: delay + 0.05, interval: delay + 1.0, next: delay, waves: 0, link: 0, held: [0; 4], lanes: 0, ai_fails: 0, strikes: Vec::new(), struck: Vec::new() });
                }
                true
            }
            // Far Sight reveals the area for its duration.
            "AOfs" => {
                let modifier = crate::fog::FogModifier { player: owner, state: crate::fog::FOG_VISIBLE, shape: crate::fog::FogShape::Circle(x, y, area), shared: true, after_units: false, active: true };
                let m = self.handles.insert(Obj::FogModifier(modifier));
                self.timed_removals.push((m, self.time + self.d(a, "Dur", lvl).max(1.0)));
                true
            }
            // Reveal (the Town Hall's, AIta): the area shows, and its invisible units, for
            // Dur.
            "AIta" => {
                let dur = self.d(a, "Dur", lvl).max(0.1);
                self.reveal_area(owner, x, y, area, dur, 0.0);
                self.effect_at(a, lvl, x, y, dur);
                true
            }
            // Flare: after DataB, the area shows, and its invisible units, for Dur.
            "Afla" => {
                let (dur, delay) = (self.d(a, "Dur", lvl).max(0.1), self.d(a, "DataB", lvl));
                self.reveal_area(owner, x, y, area, dur, delay);
                self.effect_at(a, lvl, x, y, dur + delay);
                true
            }
            "Amil" => {
                self.take_up_arms(h, a, lvl);
                true
            }
            "Acmg" => {
                self.control_magic(vm, h, a, lvl, target);
                true
            }
            "Asps" => {
                self.spell_steal(h, a, lvl, target);
                true
            }
            "Aspl" => {
                self.spirit_link(h, a, lvl, target);
                true
            }
            "Auco" => {
                self.unstable_concoction(vm, h, a, lvl, target);
                true
            }
            "Adev" => {
                self.devour(h, a, lvl, target);
                true
            }
            "Amel" => {
                self.load_corpse(h, target);
                true
            }
            "Amed" => {
                self.drop_corpses(h);
                true
            }
            // Parasite: DataA a second for Dur, attacks DataB and moves DataC slower.
            "ANpa" => {
                let buff = self.buff_of(a, lvl);
                let dur = self.duration_for(a, lvl, target);
                self.apply_buff(target, Buff { id: buff, ability: a, level: lvl, caster: h, remaining: dur, dps: self.d(a, "DataA", lvl), attack_factor: -self.d(a, "DataB", lvl), move_factor: -self.d(a, "DataC", lvl), ..Default::default() });
                true
            }
            // Pocket Factory: a factory (UnitID) for Dur at the point.
            "ANsy" => {
                let kind = crate::objects::id_from_str(self.objects.abilities.get(a).map(|o| o.level_str("UnitID", lvl)).unwrap_or_default().trim());
                let dur = self.duration_for(a, lvl, h);
                let every = self.d(a, "DataA", lvl);
                for f in self.summon(h, owner, kind, 1, x, y, dur) {
                    if let Some(u) = self.unit_mut(f) {
                        u.extra.factory = Some((a, lvl, every));
                    }
                }
                true
            }
            "AUcb" => {
                self.carrion_scarabs(h, a, lvl, target);
                true
            }
            "AOmi" => {
                self.mirror_image(h, a, lvl);
                true
            }
            "AUls" => {
                self.begin_swarm(h, a, lvl);
                true
            }
            "Aps2" => {
                self.begin_possession(h, a, lvl, target);
                true
            }
            "Acoi" => {
                self.couple(h, a, lvl, target);
                true
            }
            "Adec" => {
                self.decouple(h, a, lvl);
                true
            }
            "Adtn" => {
                self.detonate(vm, h, a, lvl, x, y);
                true
            }
            "Agra" => {
                self.grab_tree(vm, h, a, lvl, target);
                true
            }
            "Aeat" => {
                self.eat_tree(vm, h, a, lvl, target);
                true
            }
            "Avng" => {
                self.vengeance(h, a, lvl, target);
                true
            }
            // Mana Flare: the caster channels its flare for Dur.
            "Amfl" => {
                let duration = self.d(a, "Dur", lvl);
                let buff = self.buff_of(a, lvl);
                self.apply_buff(h, Buff { id: buff, ability: a, level: lvl, caster: h, remaining: duration, ..Default::default() });
                if let Some(u) = self.unit_mut(h) {
                    u.channel = Some(Channel { ability: a, level: lvl, target: 0, x, y, elapsed: 0.0, duration, interval: duration + 1.0, next: duration + 1.0, waves: 0, link: 0, held: [0; 4], lanes: 0, ai_fails: 0, strikes: Vec::new(), struck: Vec::new() });
                }
                true
            }
            // Channel: the caster channels for DataA, Follow Through Time, with no effect
            // of its own (maps answer its spell events).
            "ANcl" => {
                let duration = self.d(a, "DataA", lvl);
                if duration > 0.0 {
                    if let Some(u) = self.unit_mut(h) {
                        u.channel = Some(Channel { ability: a, level: lvl, target, x, y, elapsed: 0.0, duration, interval: duration + 1.0, next: duration + 1.0, waves: 0, link: 0, held: [0; 4], lanes: 0, ai_fails: 0, strikes: Vec::new(), struck: Vec::new() });
                    }
                }
                true
            }
            "AOre" | "AEev" | "AOcr" | "AHbh" | "AUts" | "AEah" | "AUav" => true,
            _ => false,
        }
    }

    /// Advances channeled spells: waves at their interval; moving, stun, or death stop
    /// them.
    pub fn update_channels(&mut self, vm: &mut Vm, dt: f32) {
        let casters: Vec<u32> = self.unit_order.iter().copied().filter(|&h| self.unit(h).is_some_and(|u| u.channel.is_some())).collect();
        for h in casters {
            let Some(u) = self.unit(h) else { continue };
            let Some(mut c) = u.channel.clone() else { continue };
            let stop = u.dead || !matches!(u.action, Action::Cast { ability, .. } if ability == c.ability) || self.buff_mods(h).4;
            if let Some(t) = (c.link != 0).then(|| self.channel_link_ends(h, c.target)).flatten() {
                if let Some(Obj::Lightning(l)) = self.handles.get_mut(c.link) {
                    l.from = t.0;
                    l.to = t.1;
                }
            }
            let target_gone = c.link != 0 && self.unit(c.target).is_none_or(|t| t.dead);
            if stop || target_gone {
                // An interrupted channel ends without finishing.
                self.release_channel(&c);
                self.unit_mut(h).unwrap().channel = None;
                use crate::event_ids::*;
                let ctx = EventCtx { target_unit: c.target, spell_ability: crate::objects::id_to_int(c.ability), order_x: c.x, order_y: c.y, ..Default::default() };
                self.queue_unit_event(h, EVENT_PLAYER_UNIT_SPELL_ENDCAST, EVENT_UNIT_SPELL_ENDCAST, ctx);
                continue;
            }
            c.elapsed += dt;
            c.next -= dt;
            if !c.strikes.is_empty() {
                for s in c.strikes.iter_mut() {
                    *s -= dt;
                }
                while c.strikes.first().is_some_and(|&s| s <= 0.0) {
                    c.strikes.remove(0);
                    self.blizzard_strike(vm, h, &mut c);
                }
            }
            while c.next <= 0.0 && c.elapsed <= c.duration + 1e-3 {
                self.channel_tick(vm, h, &mut c);
                c.next += c.interval;
                if c.waves > 0 {
                    c.waves -= 1;
                    if c.waves == 0 {
                        // Blizzard channels on through its last wave's second (measured:
                        // the Archmage's order is blizzard for 6.3 seconds, its cast
                        // point and six waves a second apart); the others end with
                        // their last wave.
                        if self.ability_code(c.ability) == "AHbz" {
                            c.next = f32::INFINITY;
                        } else {
                            c.elapsed = c.duration + 1.0;
                        }
                    }
                }
            }
            let done = c.elapsed >= c.duration;
            if done {
                self.release_channel(&c);
                if self.ability_code(c.ability) == "Aps2" {
                    self.finish_possession(vm, h, c.target);
                    continue;
                }
            }
            let u = self.unit_mut(h).unwrap();
            if done {
                u.channel = None;
                u.casting = None;
                u.action = Action::Idle;
                use crate::event_ids::*;
                let ctx = EventCtx { target_unit: c.target, spell_ability: crate::objects::id_to_int(c.ability), order_x: c.x, order_y: c.y, ..Default::default() };
                self.queue_unit_event(h, EVENT_PLAYER_UNIT_SPELL_FINISH, EVENT_UNIT_SPELL_FINISH, ctx.clone());
                self.queue_unit_event(h, EVENT_PLAYER_UNIT_SPELL_ENDCAST, EVENT_UNIT_SPELL_ENDCAST, ctx);
            } else {
                u.channel = Some(c);
            }
        }
    }

    /// Mass Teleport's end: the caster and up to DataA of its owner's units within Area
    /// go to the target, or its point if it is gone.
    fn mass_teleport(&mut self, h: u32, a: Id, lvl: u32, target: u32, at: (f32, f32)) {
        let owner = self.owner_of(h);
        let area = self.d(a, "Area", lvl);
        let (cx, cy) = self.unit(h).map_or(at, |u| (u.x, u.y));
        let (tx, ty) = self.widget_pos(target).unwrap_or(at);
        let n = self.d(a, "DataA", lvl) as usize;
        let mut group: Vec<u32> = self.allies_in(owner, cx, cy, area).into_iter().filter(|&u| u != h && self.owner_of(u) == owner && self.unit(u).is_some_and(|u| !u.is_building)).take(n).collect();
        group.push(h);
        for (k, u) in group.into_iter().enumerate() {
            let ang = k as f32 * 0.9;
            let r = 40.0 * (k as f32).sqrt();
            if let Some(un) = self.unit_mut(u) {
                un.x = tx + r * ang.cos();
                un.y = ty + r * ang.sin();
                un.path.clear();
                if u != h {
                    un.action = Action::Idle;
                }
            }
        }
    }

    /// A Blizzard wave strikes: DataB to each unit in Area not struck before by the
    /// cast, a share of DataF at most when that is less, and DataD of it to
    /// structures.
    fn blizzard_strike(&mut self, vm: &mut Vm, h: u32, c: &mut Channel) {
        let (a, lvl) = (c.ability, c.level);
        let area = self.d(a, "Area", lvl);
        let targets: Vec<u32> = self.units_in(c.x, c.y, area).into_iter().filter(|t| !c.struck.contains(t)).collect();
        let reduction = self.d(a, "DataD", lvl);
        let max = self.d(a, "DataF", lvl);
        let each = self.d(a, "DataB", lvl);
        let per = if max > 0.0 && !targets.is_empty() { each.min(max / targets.len() as f32) } else { each };
        for t in targets {
            let k = if self.unit(t).is_some_and(|u| u.is_building) { reduction } else { 1.0 };
            self.spell_damage(vm, h, t, per * if k > 0.0 { k } else { 1.0 });
            c.struck.push(t);
        }
    }

    fn channel_tick(&mut self, vm: &mut Vm, h: u32, c: &mut Channel) {
        // A computer's channel goes on only while its AI still likes the spot.
        if self.ai_channel_recheck(h, c) {
            return;
        }
        let (a, lvl) = (c.ability, c.level);
        let code = self.ability_code(a);
        let owner = self.owner_of(h);
        let area = self.d(a, "Area", lvl);
        match code.as_str() {
            "AHmt" => self.mass_teleport(h, a, lvl, c.target, (c.x, c.y)),
            "ANst" => self.stampede_beast(h, c),
            "ANvc" => self.volcano_wave(h, a),
            "Aclf" => {
                let buff = self.buff_of(a, lvl);
                let disarm = self.d(a, "DataA", lvl) as u8;
                let targs = self.objects.abilities.get(a).map(|o| o.level_str("targs", lvl).to_ascii_lowercase()).unwrap_or_default();
                for t in self.units_in(c.x, c.y, area) {
                    let ok = self.unit(t).is_some_and(|u| u.is_building || !targs.contains("structure"));
                    if ok && self.unit(t).is_some_and(|u| u.owner != owner) {
                        self.apply_buff(t, Buff { id: buff, ability: a, level: lvl, caster: h, remaining: 0.5, disarm, ..Default::default() });
                    }
                }
            }
            // Blizzard: a wave of shards over the area; friends are hit too.
            // Rain of Fire: a Blizzard wave that also leaves what it hits burning for DataE
            // a second for Dur.
            "ANrf" => {
                let targets: Vec<u32> = self.units_in(c.x, c.y, area).into_iter().filter(|&t| self.cast_target_error(h, a, t).is_none()).collect();
                let (reduction, max, each) = (self.d(a, "DataD", lvl), self.d(a, "DataF", lvl), self.d(a, "DataB", lvl));
                let per = if max > 0.0 && !targets.is_empty() { each.min(max / targets.len() as f32) } else { each };
                let buff = self.buff_of(a, lvl);
                let (burn, dur) = (self.d(a, "DataE", lvl), self.d(a, "Dur", lvl));
                for t in targets {
                    let k = if self.unit(t).is_some_and(|u| u.is_building) && reduction > 0.0 { reduction } else { 1.0 };
                    self.spell_damage(vm, h, t, per * k);
                    if burn > 0.0 && self.unit(t).is_some_and(|u| !u.is_building) {
                        self.apply_buff(t, Buff { id: buff, ability: a, level: lvl, caster: h, remaining: dur, dps: burn, ..Default::default() });
                    }
                }
                self.art_at(a, "EffectArt", c.x, c.y);
            }
            // Blizzard: a wave's shards fall now and strike 0.9 seconds later
            // (FUN_6f4daef0's DAT_6fcd5414), each unit once a cast (measured: a footman
            // moved into the area during the cast is struck by the next wave and no
            // other, and one moved out after the first wave is not struck again).
            "AHbz" => {
                c.strikes.push(0.9);
                self.art_at(a, "EffectArt", c.x, c.y);
            }
            "AEsf" => {
                let reduction = self.d(a, "DataC", lvl);
                for t in self.enemies_in(owner, c.x, c.y, area) {
                    let k = if self.unit(t).is_some_and(|u| u.is_building) && reduction > 0.0 { reduction } else { 1.0 };
                    self.spell_damage(vm, h, t, self.d(a, "DataA", lvl) * k);
                }
            }
            "AEtq" => {
                for t in self.allies_in(owner, c.x, c.y, area) {
                    if self.unit(t).is_some_and(|u| !u.is_building) {
                        self.magic_heal(t, self.d(a, "DataA", lvl));
                    }
                }
            }
            // Death and Decay: a share of maximum life every second, to everything.
            "AUdd" => {
                let pct = self.d(a, "DataA", lvl);
                for t in self.units_in(c.x, c.y, area) {
                    let max = self.unit(t).map_or(0.0, |u| u.max_life);
                    self.spell_damage(vm, h, t, max * pct * c.interval);
                }
            }
            // Drain (Siphon Mana, Life Drain): each interval DataA life and DataB mana
            // from an enemy to the caster; to an ally the caster gives DataD life and
            // DataE mana a second, under DrainTransfersLife and DrainTransfersMana
            // (CAbilityDrain::vf265), paying all of it though the ally fills, and it
            // ends once the ally is full.
            "AHdr" if self.unit(c.target).is_some_and(|t| crate::natives::players::is_ally(self, owner, t.owner)) => {
                let life = if self.misc_f32("DrainTransfersLife", 1.0) != 0.0 { self.d(a, "DataD", lvl) * c.interval } else { 0.0 };
                let mana = if self.misc_f32("DrainTransfersMana", 1.0) != 0.0 { self.d(a, "DataE", lvl) * c.interval } else { 0.0 };
                let (life, mana) = self.unit(h).map_or((0.0, 0.0), |u| (life.min((u.life - 1.0).max(0.0)), mana.min(u.mana)));
                if let Some(u) = self.unit_mut(h) {
                    u.life -= life;
                    u.mana -= mana;
                }
                let mut full = true;
                if let Some(t) = self.unit_mut(c.target) {
                    t.life = (t.life + life).min(t.max_life);
                    t.mana = (t.mana + mana).min(t.max_mana);
                    full = (life == 0.0 || t.life >= t.max_life) && (mana == 0.0 || t.mana >= t.max_mana);
                }
                if full {
                    c.elapsed = c.duration + 1.0;
                }
            }
            "AHdr" => {
                let take = self.unit(c.target).map_or(0.0, |t| t.mana.min(self.d(a, "DataB", lvl)));
                if let Some(t) = self.unit_mut(c.target) {
                    t.mana -= take;
                }
                if let Some(u) = self.unit_mut(h) {
                    u.mana = (u.mana + take).min(u.max_mana);
                }
                let life = self.d(a, "DataA", lvl);
                if life > 0.0 {
                    let before = self.unit(c.target).map_or(0.0, |t| t.life);
                    self.spell_damage(vm, h, c.target, life);
                    let taken = (before - self.unit(c.target).map_or(0.0, |t| t.life.max(0.0))).max(0.0);
                    if let Some(u) = self.unit_mut(h) {
                        u.life = (u.life + taken).min(u.max_life);
                    }
                }
            }
            // Big Bad Voodoo: allies around the caster cannot be hurt.
            "AOvd" => {
                let buff = self.buff_of(a, lvl);
                let (x, y) = self.unit(h).map_or((c.x, c.y), |u| (u.x, u.y));
                for t in self.allies_in(owner, x, y, area) {
                    if t != h {
                        self.apply_buff(t, Buff { id: buff, ability: a, level: lvl, caster: h, remaining: c.interval + 0.1, invulnerable: true, ..Default::default() });
                    }
                }
            }
            // Earthquake: buildings take damage; units slow.
            "AOeq" => {
                let buff = self.buff_of(a, lvl);
                for t in self.enemies_in(owner, c.x, c.y, area) {
                    if self.unit(t).is_some_and(|u| u.is_building) {
                        self.spell_damage(vm, h, t, self.d(a, "DataB", lvl) * c.interval);
                    } else {
                        self.apply_buff(t, Buff { id: buff, ability: a, level: lvl, caster: h, remaining: c.interval + 0.1, move_factor: -self.d(a, "DataC", lvl), ..Default::default() });
                    }
                }
            }
            _ => {}
        }
    }

    /// Reincarnation: a hero with AOre rises where it fell after DataA seconds, when the
    /// ability was ready as it died.
    pub fn update_reincarnation(&mut self, dt: f32) {
        let heroes: Vec<u32> = self.unit_order.iter().copied().filter(|&h| self.unit(h).is_some_and(|u| u.dead && u.reincarnate.is_some())).collect();
        for h in heroes {
            let u = self.unit_mut(h).unwrap();
            let t = u.reincarnate.unwrap() - dt;
            if t > 0.0 {
                u.reincarnate = Some(t);
                continue;
            }
            u.reincarnate = None;
            let (x, y) = (u.x, u.y);
            let restore = u.extra.restore.take();
            let mana_before = u.mana;
            self.revive_hero(h, x, y);
            if let (Some((life, mana)), Some(u)) = (restore, self.unit_mut(h)) {
                if life > 0.0 {
                    u.life = life.min(u.max_life);
                }
                u.mana = if mana < 0.0 { mana_before.min(u.max_mana) } else { mana.min(u.max_mana) };
            }
        }
    }

    /// Called as a hero dies: starts Reincarnation if it has it ready.
    pub fn try_reincarnate(&mut self, h: u32) -> bool {
        // An Ankh of Reincarnation (AIrc) in the inventory: back after DataA with DataB
        // life and DataC mana (-1 keeps it), the ankh spent.
        if self.ability_with_code(h, "AOre").is_none() {
            let held = self.unit(h).map(|u| u.inventory.clone()).unwrap_or_default();
            for item in held {
                let Some(t) = self.item(item).map(|i| i.type_id) else { continue };
                let Some(a) = self.item_abilities(t).into_iter().find(|&a| self.ability_code(a) == "AIrc") else { continue };
                let (delay, life, mana) = (self.d(a, "DataA", 1), self.d(a, "DataB", 1), self.d(a, "DataC", 1));
                self.remove_item(item);
                let u = self.unit_mut(h).unwrap();
                u.reincarnate = Some(delay);
                u.extra.restore = Some((life, mana));
                return true;
            }
            return false;
        }
        let Some((a, lvl)) = self.ability_with_code(h, "AOre") else { return false };
        if self.unit(h).is_some_and(|u| u.cooldowns.get(&a).copied().unwrap_or(0.0) > 0.0) {
            return false;
        }
        let delay = self.d(a, "DataA", lvl);
        let cool = self.d(a, "Cool", lvl);
        let u = self.unit_mut(h).unwrap();
        u.cooldowns.insert(a, cool);
        u.reincarnate = Some(delay);
        true
    }

    /// Advances spells left on the ground.
    pub fn update_ground_effects(&mut self, vm: &mut Vm, dt: f32) {
        let mut effects = std::mem::take(&mut self.ground_effects);
        for e in &mut effects {
            e.elapsed += dt;
            e.next -= dt;
            // Effects that follow their caster end with it.
            let code = self.ability_code(e.ability);
            if matches!(code.as_str(), "AOww" | "AEim") {
                match self.unit(e.caster).filter(|u| !u.dead) {
                    Some(u) => {
                        e.x = u.x;
                        e.y = u.y;
                    }
                    None => e.elapsed = e.duration,
                }
            }
            while e.next <= 0.0 && e.elapsed < e.duration {
                e.next += e.interval;
                let area = self.d(e.ability, "Area", e.level);
                match code.as_str() {
                    "AHfs" => {
                        let full = self.d(e.ability, "HeroDur", e.level);
                        let damage = if e.elapsed <= full {
                            self.d(e.ability, "DataA", e.level)
                        } else {
                            // The half-damage phase ticks every DataD seconds.
                            e.interval = self.d(e.ability, "DataD", e.level).max(0.05);
                            self.d(e.ability, "DataC", e.level)
                        };
                        for t in self.enemies_in(e.owner, e.x, e.y, area) {
                            self.spell_damage(vm, e.caster, t, damage);
                        }
                    }
                    // Burning Oil: DataA every DataB to what it allows within Area,
                        // structures taking DataE of it.
                    "Abof" | "ANmo" => {
                        let reduction_key = if code == "ANmo" { "DataC" } else { "DataE" };
                        let (dmg, reduction) = (self.d(e.ability, "DataA", e.level), self.d(e.ability, reduction_key, e.level));
                        for t in self.units_in(e.x, e.y, area) {
                            if self.cast_target_error(e.caster, e.ability, t).is_some() {
                                continue;
                            }
                            let k = if self.unit(t).is_some_and(|u| u.is_building) && reduction > 0.0 { reduction } else { 1.0 };
                            self.spell_damage(vm, e.caster, t, dmg * k);
                        }
                    }
                    "AOww" => {
                        let dps = self.d(e.ability, "DataA", e.level);
                        for t in self.enemies_in(e.owner, e.x, e.y, area) {
                            self.spell_damage(vm, e.caster, t, dps * e.interval);
                        }
                    }
                    "AEim" => {
                        // Immolation burns while the caster can pay its mana each second.
                        let drain = self.d(e.ability, "DataB", e.level);
                        let mana = self.unit(e.caster).map_or(0.0, |u| u.mana);
                        if mana < drain {
                            e.elapsed = e.duration;
                            break;
                        }
                        if let Some(u) = self.unit_mut(e.caster) {
                            u.mana -= drain;
                        }
                        for t in self.enemies_in(e.owner, e.x, e.y, area) {
                            self.spell_damage(vm, e.caster, t, self.d(e.ability, "DataA", e.level));
                        }
                    }
                    _ => {}
                }
            }
        }
        effects.retain(|e| e.elapsed < e.duration);
        effects.append(&mut self.ground_effects);
        self.ground_effects = effects;
        // Handles with a set lifetime (Far Sight's reveal).
        let now = self.time;
        let expired: Vec<u32> = self.timed_removals.iter().filter(|(_, t)| *t <= now).map(|(h, _)| *h).collect();
        self.timed_removals.retain(|(_, t)| *t > now);
        for h in expired {
            self.handles.remove(h);
        }
        let due: Vec<u32> = self.timed_activations.iter().filter(|(_, t)| *t <= now).map(|(h, _)| *h).collect();
        self.timed_activations.retain(|(_, t)| *t > now);
        for h in due {
            if let Some(Obj::FogModifier(m)) = self.handles.get_mut(h) {
                m.active = true;
            }
        }
        self.detect_areas.retain(|a| a.4 > now);
    }

    /// Shows a circle to a player, and the invisible units in it, `delay` from now for
    /// `dur`.
    pub(crate) fn reveal_area(&mut self, player: u8, x: f32, y: f32, r: f32, dur: f32, delay: f32) {
        let modifier = crate::fog::FogModifier { player, state: crate::fog::FOG_VISIBLE, shape: crate::fog::FogShape::Circle(x, y, r), shared: true, after_units: false, active: delay <= 0.0 };
        let m = self.handles.insert(Obj::FogModifier(modifier));
        if delay > 0.0 {
            self.timed_activations.push((m, self.time + delay));
        }
        self.timed_removals.push((m, self.time + delay + dur));
        self.detect_areas.push((player, x, y, r, self.time + delay + dur));
    }

    /// The ability's EfctID effect (its Effectart) at a point for a while.
    fn effect_at(&mut self, a: Id, lvl: u32, x: f32, y: f32, secs: f32) {
        let Some(id) = self.objects.abilities.get(a).map(|o| o.level_str("EfctID", lvl)).filter(|s| !s.trim().is_empty() && s.trim() != "_") else { return };
        let Some(model) = self.objects.buffs.get(crate::objects::id_from_str(id.trim())).and_then(|o| o.list("Effectart").first().cloned()).filter(|m| !m.is_empty()) else { return };
        let z = self.ground_z(x, y);
        let e = self.handles.insert(Obj::Effect(Effect { model, x, y, z, target: None, destroyed: false, facing: 0.0 }));
        self.timed_removals.push((e, self.time + secs));
    }

    /// The ability's channel lightning (its LightningEffect) from caster to target.
    fn channel_link(&mut self, a: Id, from: u32, to: u32) -> u32 {
        self.channel_link_nth(a, 0, from, to)
    }

    /// A lightning from `from` to `to` of the ability's `n`th LightningEffect.
    fn channel_link_nth(&mut self, a: Id, n: usize, from: u32, to: u32) -> u32 {
        let code = self.objects.abilities.get(a).map(|o| o.str("LightningEffect")).unwrap_or_default();
        let codes: Vec<&str> = code.split(',').map(str::trim).collect();
        let code = codes.get(n).or(codes.first()).copied().unwrap_or("").to_string();
        let Some((f, t)) = self.channel_link_ends(from, to) else { return 0 };
        if code.is_empty() {
            return 0;
        }
        let chain = self.chain.clone();
        let table = self.lightning_data.get_or_insert_with(|| chain.read_string("Splats\\LightningData.slk").map(|s| w3_formats::slk::Table::parse(&s)).unwrap_or_default());
        let n = |c: &str| table.get(&code, c).map(|v| v.as_str()).unwrap_or_default().trim().parse::<f32>().unwrap_or(255.0) / 255.0;
        let color = [n("R"), n("G"), n("B"), n("A")];
        self.handles.insert(Obj::Lightning(Lightning { code, check_visibility: false, from: f, to: t, color }))
    }

    fn channel_link_ends(&self, from: u32, to: u32) -> Option<([f32; 3], [f32; 3])> {
        let (f, t) = (self.unit(from)?, self.unit(to)?);
        let launch = self.objects.units.get(f.type_id).map_or(60.0, |o| o.real("launchZ"));
        let impact = self.objects.units.get(t.type_id).map_or(60.0, |o| o.real("impactZ"));
        Some(([f.x, f.y, self.ground_z(f.x, f.y) + f.fly_height + launch], [t.x, t.y, self.ground_z(t.x, t.y) + t.fly_height + impact]))
    }

    /// A channel's end lets its target go and takes down its lightning.
    fn release_channel(&mut self, c: &Channel) {
        if c.link != 0 {
            self.handles.remove(c.link);
        }
        // The caster's own channel buff ends too.
        if let Some(caster) = self.unit_order.iter().copied().find(|&u| self.unit(u).is_some_and(|x| x.channel.as_ref().is_some_and(|ch| ch.ability == c.ability && ch.target == c.target))) {
            if let Some(u) = self.unit_mut(caster) {
                for b in u.buffs.iter_mut().filter(|b| b.ability == c.ability) {
                    b.remaining = 0.0;
                }
            }
        }
        if c.held != [0; 4] {
            if let Some(t) = self.unit_mut(c.target) {
                for b in t.buffs.iter_mut().filter(|b| b.id == c.held) {
                    b.remaining = 0.0;
                }
            }
        }
    }
}
