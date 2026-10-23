//! Spell effects by ability code, with field meanings from AbilityMetaData: channeled
//! waves, ground effects, lines, chains, summons, corpses, and buffs.

use w3_jass::Vm;

use crate::objects::{Id, id_from_str};
use crate::types::*;
use crate::world::World;

/// Codes whose effect keeps the caster channeling.
pub fn channeled(code: &str) -> bool {
    matches!(code, "AHbz" | "AEsf" | "AEtq" | "AUdd" | "AHdr" | "AOvd" | "AOeq")
}

impl World {
    fn d(&self, a: Id, field: &str, lvl: u32) -> f32 {
        self.data(a, field, lvl)
    }

    fn buff_of(&self, a: Id, lvl: u32) -> Id {
        id_from_str(self.objects.abilities.get(a).map(|o| o.level_str("BuffID", lvl)).unwrap_or_default().split(',').next().unwrap_or("").trim())
    }

    fn duration_for(&self, a: Id, lvl: u32, target: u32) -> f32 {
        if self.unit(target).is_some_and(|t| t.is_hero) { self.d(a, "HeroDur", lvl) } else { self.d(a, "Dur", lvl) }
    }

    fn owner_of(&self, h: u32) -> u8 {
        self.unit(h).map_or(0, |u| u.owner)
    }

    fn units_in(&self, x: f32, y: f32, r: f32) -> Vec<u32> {
        self.unit_order.iter().copied().filter(|&h| self.unit(h).is_some_and(|u| !u.dead && !u.hidden && (u.x - x).hypot(u.y - y) <= r)).collect()
    }

    fn allies_in(&self, owner: u8, x: f32, y: f32, r: f32) -> Vec<u32> {
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

    fn art_at(&mut self, a: Id, key: &str, x: f32, y: f32) {
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
            "AHbz" | "AEsf" | "AEtq" | "AUdd" | "AHdr" | "AOvd" | "AOeq" => {
                let waves = match code {
                    "AHbz" => self.d(a, "DataA", lvl).max(1.0) as u32,
                    _ => 0,
                };
                // Self-sustained spells last the caster's duration (HeroDur for heroes);
                // Blizzard's waves come one per Cast time.
                let cast = self.d(a, "Cast", lvl).max(0.1);
                let duration = if code == "AHbz" { waves as f32 * cast } else { self.duration_for(a, lvl, h) };
                let interval = match code {
                    "AHbz" => cast,
                    "AEsf" | "AEtq" => self.d(a, "DataB", lvl).max(0.1),
                    "AHdr" => self.d(a, "DataC", lvl).max(0.1),
                    _ => 1.0,
                };
                let (px, py) = if code == "AOvd" || code == "AEtq" || code == "AEsf" { (cx, cy) } else { (x, y) };
                if let Some(u) = self.unit_mut(h) {
                    u.channel = Some(Channel { ability: a, level: lvl, target, x: px, y: py, elapsed: 0.0, duration, interval, next: 0.0, waves });
                }
                true
            }
            // Flame Strike: DataA every DataB seconds for HeroDur, then DataC every DataD
            // seconds until Dur.
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
            "AOcl" | "AOhw" => {
                let (mut amount, jumps, loss) = (self.d(a, "DataA", lvl), self.d(a, "DataB", lvl).max(1.0) as usize, self.d(a, "DataC", lvl));
                let heal = code == "AOhw";
                let mut hit = vec![target];
                let mut cur = target;
                for _ in 0..jumps {
                    if heal { self.heal(cur, amount); } else { self.spell_damage(vm, h, cur, amount); }
                    amount *= 1.0 - loss;
                    let Some((px, py)) = self.widget_pos(cur) else { break };
                    let range = if area > 0.0 { area } else { 500.0 };
                    let next = self
                        .units_in(px, py, range)
                        .into_iter()
                        .filter(|t| !hit.contains(t))
                        .filter(|&t| crate::natives::players::is_ally(self, owner, self.owner_of(t)) == heal)
                        .filter(|&t| !heal || self.unit(t).is_some_and(|u| u.life < u.max_life))
                        .min_by(|&p, &q| {
                            let dd = |t: u32| self.widget_pos(t).map_or(f32::MAX, |w| (w.0 - px).hypot(w.1 - py));
                            dd(p).total_cmp(&dd(q))
                        });
                    let Some(n) = next else { break };
                    hit.push(n);
                    cur = n;
                }
                true
            }
            // Mana Burn: drains up to DataA mana and deals as much damage.
            "AEmb" => {
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
                for c in corpses {
                    let Some(u) = self.unit(c) else { continue };
                    let (t, px, py, f) = (u.type_id, u.x, u.y, u.facing);
                    crate::natives::units::remove_unit(self, c);
                    let r = crate::natives::units::create_unit(self, owner, t, px, py, f);
                    if let Some(ru) = self.unit_mut(r) {
                        if life > 0.0 {
                            ru.timed_life = Some(life);
                        }
                    }
                    self.art_at(a, "TargetArt", px, py);
                }
                true
            }
            // Mass Teleport: the caster and units around it to a friendly unit.
            "AHmt" => {
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
                        un.action = Action::Idle;
                    }
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
            let stop = u.dead || !matches!(u.action, Action::Cast { .. }) || self.buff_mods(h).4;
            let Some(mut c) = u.channel.clone() else { continue };
            if stop {
                // An interrupted channel ends without finishing.
                self.unit_mut(h).unwrap().channel = None;
                use crate::event_ids::*;
                let ctx = EventCtx { target_unit: c.target, spell_ability: crate::objects::id_to_int(c.ability), order_x: c.x, order_y: c.y, ..Default::default() };
                self.queue_unit_event(h, EVENT_PLAYER_UNIT_SPELL_ENDCAST, EVENT_UNIT_SPELL_ENDCAST, ctx);
                continue;
            }
            c.elapsed += dt;
            c.next -= dt;
            while c.next <= 0.0 && c.elapsed <= c.duration + 1e-3 {
                self.channel_tick(vm, h, &c);
                c.next += c.interval;
                if c.waves > 0 {
                    c.waves -= 1;
                    if c.waves == 0 {
                        c.elapsed = c.duration + 1.0;
                    }
                }
            }
            let done = c.elapsed >= c.duration;
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

    fn channel_tick(&mut self, vm: &mut Vm, h: u32, c: &Channel) {
        let (a, lvl) = (c.ability, c.level);
        let code = self.ability_code(a);
        let owner = self.owner_of(h);
        let area = self.d(a, "Area", lvl);
        match code.as_str() {
            // Blizzard: a wave of shards over the area; friends are hit too.
            "AHbz" => {
                let targets = self.units_in(c.x, c.y, area);
                let reduction = self.d(a, "DataD", lvl);
                let max = self.d(a, "DataF", lvl);
                let each = self.d(a, "DataB", lvl);
                let per = if max > 0.0 && !targets.is_empty() { each.min(max / targets.len() as f32) } else { each };
                for t in targets {
                    let k = if self.unit(t).is_some_and(|u| u.is_building) { reduction } else { 1.0 };
                    self.spell_damage(vm, h, t, per * if k > 0.0 { k } else { 1.0 });
                }
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
                        self.heal(t, self.d(a, "DataA", lvl));
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
            // Siphon Mana: mana from the target to the caster.
            "AHdr" => {
                let take = self.unit(c.target).map_or(0.0, |t| t.mana.min(self.d(a, "DataB", lvl) * c.interval));
                if let Some(t) = self.unit_mut(c.target) {
                    t.mana -= take;
                }
                if let Some(u) = self.unit_mut(h) {
                    u.mana = (u.mana + take).min(u.max_mana);
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
            self.revive_hero(h, x, y);
        }
    }

    /// Called as a hero dies: starts Reincarnation if it has it ready.
    pub fn try_reincarnate(&mut self, h: u32) -> bool {
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
    }
}
