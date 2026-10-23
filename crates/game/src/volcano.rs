//! Volcano (CAbilityVolcano, CEffectVolcano, CArtilleryVolcano).  The caster channels
//! for DataB waves DataC apart while a volcano (UnitID) stands at the target point.
//! The first wave comes a second after the cast.  A wave (FUN_6f5d0840) hurls
//! 1 + Σ(5 + 2i) rocks over DataA rings (at most 32), in a shuffled order, one after
//! another between 0.5 and 1 times (DataC / 2 / rocks − 0.01) seconds apart
//! (FUN_6f5d8930).  With S = Area / (DataA + 0.5), the first lands within ±0.15 Area of
//! the volcano and ring i's 5 + 2i rocks evenly around it at about (i + 1) S (the
//! outer half-step 0.95 to 1.04 times S / 2), from a random starting angle (turned by
//! π on even rings), each pushed up to 0.3 Area further.  A rock is an artillery
//! missile (the EfctID's Missileart, speed, and arc) that on landing shows its
//! Specialart and deals DataE to what the spell allows within S / 2 and DataE × DataF
//! within S, DataD times as much to structures, stunning units (the first BuffID) for
//! Dur, HeroDur for Heroes.

use w3_jass::Vm;

use crate::objects::{Id, id_from_str};
use crate::sim::Missile;
use crate::types::*;
use crate::world::World;

/// A volcano standing while its caster channels.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Volcano {
    pub caster: u32,
    pub ability: Id,
    pub level: u32,
    pub x: f32,
    pub y: f32,
    pub dest: u32,
    /// Rocks still to throw this wave: where they land, in order, and when the next goes.
    pub rocks: Vec<(f32, f32)>,
    pub next: f32,
    pub every: f32,
}

impl World {
    fn effect_profile(&self, a: Id, lvl: u32, key: &str) -> String {
        let id = self.objects.abilities.get(a).map(|o| o.level_str("EfctID", lvl)).unwrap_or_default();
        self.objects.buffs.get(id_from_str(id.trim())).and_then(|o| o.list(key).first().cloned()).unwrap_or_default()
    }

    /// The volcano rises where the spell was cast.
    pub(crate) fn begin_volcano(&mut self, h: u32, a: Id, lvl: u32, x: f32, y: f32) {
        let kind = id_from_str(self.objects.abilities.get(a).map(|o| o.level_str("UnitID", lvl)).unwrap_or_default().trim());
        let dest = crate::natives::objects::create_destructable(self, kind, x, y, None, 270.0, 1.0, 0);
        self.volcanoes.push(Volcano { caster: h, ability: a, level: lvl, x, y, dest, rocks: Vec::new(), next: 0.0, every: 0.0 });
    }

    /// A wave of rocks (see the module notes).
    pub(crate) fn volcano_wave(&mut self, h: u32, a: Id) {
        let Some(i) = self.volcanoes.iter().position(|v| v.caster == h && v.ability == a) else { return };
        let (lvl, cx, cy) = (self.volcanoes[i].level, self.volcanoes[i].x, self.volcanoes[i].y);
        let rings = self.data(a, "DataA", lvl).max(0.0) as usize;
        let area = self.data(a, "Area", lvl);
        let step = area / (rings as f32 + 0.5);
        let jitter = area * 0.3;
        let count = (1 + (0..rings).map(|i| 5 + 2 * i).sum::<usize>()).min(32);
        let mut order: Vec<usize> = (0..count).collect();
        for k in (1..order.len()).rev() {
            let j = self.rng.int(0, k as i32) as usize;
            order.swap(k, j);
        }
        let mut spots = vec![(cx, cy); count];
        let mut slot = count;
        if slot > 0 {
            slot -= 1;
            spots[order[slot]] = (cx + (self.rng.real(0.0, 1.0) - 0.5) * jitter, cy + (self.rng.real(0.0, 1.0) - 0.5) * jitter);
        }
        let start = self.rng.int(0, 99) as f32 * std::f32::consts::TAU / 100.0;
        for ring in 0..rings {
            let n = 5 + 2 * ring;
            let base = start + if ring % 2 == 0 { std::f32::consts::PI } else { 0.0 };
            for k in 0..n {
                if slot == 0 {
                    break;
                }
                slot -= 1;
                let outer = 0.75 + (self.rng.int(0, 9) + 20) as f32 / 100.0;
                let r = step * 0.5 + step * ring as f32 + step * 0.5 * outer;
                let ang = base + std::f32::consts::TAU / n as f32 * k as f32;
                let (jx, jy) = (self.rng.real(0.0, 1.0) * jitter, self.rng.real(0.0, 1.0) * jitter);
                spots[order[slot]] = (cx + ang.cos() * r + jx, cy + ang.sin() * r + jy);
            }
        }
        let every = self.data(a, "DataC", lvl) * 0.5 / count.max(1) as f32 - 0.01;
        let v = &mut self.volcanoes[i];
        v.rocks = spots;
        v.every = every;
        v.next = self.time;
    }

    /// Throws the waves' rocks, lands them, and lets a volcano die when its channel ends.
    pub(crate) fn update_volcanoes(&mut self, vm: &mut Vm, dt: f32) {
        let mut i = 0;
        while i < self.volcanoes.len() {
            let v = self.volcanoes[i].clone();
            let channeling = self.unit(v.caster).is_some_and(|u| !u.dead && u.channel.as_ref().is_some_and(|c| c.ability == v.ability));
            if !channeling {
                crate::natives::objects::kill_destructable_pub(self, vm, v.dest);
                self.volcanoes.swap_remove(i);
                continue;
            }
            if !v.rocks.is_empty() && self.time >= v.next {
                let mut v = v;
                let (tx, ty) = v.rocks.pop().unwrap();
                let wait = self.rng.int(0, 49) as f32 + 50.0;
                v.next = self.time + wait / 100.0 * v.every;
                let speed = self.effect_profile(v.ability, v.level, "Missilespeed").trim().parse().unwrap_or(0.0f32).max(1.0);
                let arc = self.effect_profile(v.ability, v.level, "Missilearc").trim().parse().unwrap_or(0.0f32);
                let z = self.ground_z(v.x, v.y);
                self.missiles.push(Missile {
                    id: 0,
                    source: v.caster,
                    target: 0,
                    model: self.effect_profile(v.ability, v.level, "Missileart"),
                    x: v.x,
                    y: v.y,
                    z,
                    speed,
                    arc,
                    damage: self.data(v.ability, "DataE", v.level),
                    attack_type: String::new(),
                    start: (v.x, v.y),
                    spell: None,
                    weapon: None,
                    impact: Some((tx, ty)),
                    bounces: 0,
                    hit: Vec::new(),
                    stampede: None,
                    rock: Some((v.ability, v.level)),
                });
                self.volcanoes[i] = v;
            }
            i += 1;
        }
        // Rocks in flight.
        let mut landed = Vec::new();
        let mut k = 0;
        while k < self.missiles.len() {
            let Some((a, lvl)) = self.missiles[k].rock else {
                k += 1;
                continue;
            };
            let m = &mut self.missiles[k];
            let end = m.impact.unwrap_or((m.x, m.y));
            let (dx, dy) = (end.0 - m.x, end.1 - m.y);
            let d = dx.hypot(dy);
            let step = m.speed * dt;
            if d <= step {
                let m = self.missiles.swap_remove(k);
                landed.push((m.source, a, lvl, end, m.damage));
                continue;
            }
            m.x += dx / d * step;
            m.y += dy / d * step;
            k += 1;
        }
        for (source, a, lvl, (x, y), full) in landed {
            let art = self.effect_profile(a, lvl, "Specialart");
            self.spawn_effect(&art, x, y);
            let rings = self.data(a, "DataA", lvl).max(0.0);
            let step = self.data(a, "Area", lvl) / (rings + 0.5);
            let (structures, half) = (self.data(a, "DataD", lvl), self.data(a, "DataF", lvl));
            let stun = id_from_str(self.objects.abilities.get(a).map(|o| o.level_str("BuffID", lvl)).unwrap_or_default().split(',').next().unwrap_or("").trim());
            for t in self.units_in_pub(x, y, step) {
                if self.unit(t).is_none_or(|u| u.dead) || self.cast_target_error(source, a, t).is_some() {
                    continue;
                }
                let (dist, building) = self.unit(t).map_or((f32::MAX, false), |u| ((u.x - x).hypot(u.y - y), u.is_building));
                let mut amount = if dist <= step * 0.5 { full } else { full * half };
                if building {
                    amount *= structures;
                } else {
                    let dur = if self.hero_duration(t) { self.data(a, "HeroDur", lvl) } else { self.data(a, "Dur", lvl) };
                    self.apply_buff(t, Buff { id: stun, ability: a, level: lvl, caster: source, remaining: dur, stun: true, ..Default::default() });
                }
                self.spell_damage(vm, source, t, amount);
            }
            // Trees and debris within the full-damage reach take the full amount.
            let near: Vec<u32> = self
                .handles
                .iter()
                .filter_map(|(h, o)| match o {
                    Obj::Destructable(d) if d.life > 0.0 && (d.x - x).hypot(d.y - y) <= step * 0.5 => Some(*h),
                    _ => None,
                })
                .collect();
            for d in near {
                if self.cast_target_error(source, a, d).is_some() {
                    continue;
                }
                let dead = match self.handles.get_mut(d) {
                    Some(Obj::Destructable(x)) if !x.invulnerable => {
                        x.life -= full;
                        x.life <= 0.0
                    }
                    _ => false,
                };
                if dead {
                    crate::natives::objects::kill_destructable_pub(self, vm, d);
                }
            }
        }
    }
}
