//! Spell lightning: bolts from one unit to another (an ability's LightningEffect)
//! that follow both ends, from the source's launch height to the target's impact
//! height, show for a while and fade; and jumping spells (Chain Lightning, Healing
//! Wave) that strike their next target a moment after the last.

use w3_jass::Vm;

use crate::objects::Id;
use crate::types::*;
use crate::world::World;

/// How long a bolt shows before it fades, and how long it fades (measured from the
/// original at 30 frames a second).
const BOLT_HOLD: f32 = 0.45;
const BOLT_FADE: f32 = 0.5;

/// Time between a jumping spell's strikes (measured: 0.23 seconds).
const JUMP_DELAY: f32 = 0.25;

/// A bolt between two units, by its lightning handle.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Bolt {
    pub lightning: u32,
    pub from: u32,
    pub to: u32,
    pub age: f32,
    /// A timed bolt (Lightning Attack): hidden for `delay`, then shown for `life`;
    /// zero life is a spell bolt's hold and fade.
    #[serde(default)]
    pub delay: f32,
    #[serde(default)]
    pub life: f32,
}

/// A jumping spell's next strike.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Jump {
    pub ability: Id,
    pub level: u32,
    pub caster: u32,
    pub owner: u8,
    /// The unit the next bolt starts from.
    pub from: u32,
    pub amount: f32,
    pub jumps_left: usize,
    pub hit: Vec<u32>,
    pub heal: bool,
    pub timer: f32,
    /// The first strike's target, waiting for the timer.
    #[serde(default)]
    pub first: u32,
    /// Mana Burn: the strike drains up to `amount` mana and deals as much damage.
    #[serde(default)]
    pub burn: bool,
}

impl World {
    /// The ability's LightningEffect codes: the first bolt's, then the others'.
    fn lightning_codes(&self, a: Id) -> Vec<String> {
        self.objects.abilities.get(a).map(|o| o.list("LightningEffect")).unwrap_or_default().iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
    }

    fn bolt_ends(&self, from: u32, to: u32) -> Option<([f32; 3], [f32; 3])> {
        let (f, t) = (self.unit(from)?, self.unit(to)?);
        let launch = self.objects.units.get(f.type_id).map_or(60.0, |o| o.real("launchZ"));
        let impact = self.objects.units.get(t.type_id).map_or(60.0, |o| o.real("impactZ"));
        Some(([f.x, f.y, self.ground_z(f.x, f.y) + f.fly_height + launch], [t.x, t.y, self.ground_z(t.x, t.y) + t.fly_height + impact]))
    }

    /// A bolt of lightning type `code` from unit `from` to unit `to`.
    pub fn spell_bolt(&mut self, code: &str, from: u32, to: u32) {
        self.timed_bolt(code, from, to, 0.0, 0.0);
    }

    /// A bolt shown after `delay` for `life` (zero life: held, then faded).
    pub fn timed_bolt(&mut self, code: &str, from: u32, to: u32, delay: f32, life: f32) {
        let Some((f, t)) = self.bolt_ends(from, to) else { return };
        let chain = self.chain.clone();
        let table = self.lightning_data.get_or_insert_with(|| chain.read_string("Splats\\LightningData.slk").map(|s| w3_formats::slk::Table::parse(&s)).unwrap_or_default());
        if !table.rows.contains_key(code) {
            return;
        }
        let n = |c: &str| table.get(code, c).map(|v| v.as_str()).unwrap_or_default().trim().parse::<f32>().unwrap_or(255.0) / 255.0;
        let mut color = [n("R"), n("G"), n("B"), n("A")];
        if delay > 0.0 {
            color[3] = 0.0;
        }
        let h = self.handles.insert(Obj::Lightning(Lightning { code: code.to_string(), check_visibility: false, from: f, to: t, color }));
        self.bolts.push(Bolt { lightning: h, from, to, age: 0.0, delay, life });
    }

    /// A bolt from the ability's LightningEffect (the first or a later one).
    pub fn ability_bolt(&mut self, a: Id, first: bool, from: u32, to: u32) {
        let codes = self.lightning_codes(a);
        let code = if first { codes.first() } else { codes.get(1).or(codes.first()) };
        if let Some(code) = code.cloned() {
            self.spell_bolt(&code, from, to);
        }
    }

    /// Starts a jumping spell at its first target.
    /// A jumping spell's first strike comes a jump's time after the cast, as the others
    /// after it (measured: Chain Lightning's first 85 at 0.23 seconds after its cast).
    pub fn start_jumps(&mut self, _vm: &mut Vm, a: Id, level: u32, caster: u32, target: u32, amount: f32, jumps: usize, heal: bool) {
        let owner = self.unit(caster).map_or(0, |u| u.owner);
        self.jumps.push(Jump { ability: a, level, caster, owner, from: caster, amount, jumps_left: jumps, hit: Vec::new(), heal, timer: JUMP_DELAY, first: target, burn: false });
    }

    /// Mana Burn's bolt strikes after its delay (DataB, "bolt delay"; measured: 50 at
    /// 0.25 seconds after the cast).
    pub fn start_mana_burn(&mut self, a: Id, level: u32, caster: u32, target: u32, amount: f32, delay: f32) {
        let owner = self.unit(caster).map_or(0, |u| u.owner);
        self.jumps.push(Jump { ability: a, level, caster, owner, from: caster, amount, jumps_left: 1, hit: Vec::new(), heal: false, timer: delay, first: target, burn: true });
    }

    fn strike(&mut self, vm: &mut Vm, j: &mut Jump, target: u32) {
        let first = j.hit.is_empty();
        self.ability_bolt(j.ability, first, j.from, target);
        if j.heal {
            self.magic_heal(target, j.amount);
        } else if j.burn {
            let drained = self.unit(target).map_or(0.0, |t| t.mana.min(j.amount));
            if let Some(t) = self.unit_mut(target) {
                t.mana -= drained;
            }
            self.spell_damage(vm, j.caster, target, drained);
        } else {
            self.spell_damage(vm, j.caster, target, j.amount);
        }
        let loss = self.data(j.ability, "DataC", j.level);
        j.amount *= 1.0 - loss;
        j.hit.push(target);
        j.from = target;
        j.jumps_left = j.jumps_left.saturating_sub(1);
        j.timer = JUMP_DELAY;
    }

    /// Moves bolts with their units, fades and removes them, and makes the next
    /// strikes of jumping spells.
    pub fn update_bolts(&mut self, vm: &mut Vm, dt: f32) {
        let mut bolts = std::mem::take(&mut self.bolts);
        bolts.retain_mut(|b| {
            b.age += dt;
            let ends = self.bolt_ends(b.from, b.to);
            let Some(Obj::Lightning(l)) = self.handles.get_mut(b.lightning) else { return false };
            if let Some((f, t)) = ends {
                l.from = f;
                l.to = t;
            }
            if b.life > 0.0 {
                if b.age >= b.delay + b.life {
                    self.handles.remove(b.lightning);
                    return false;
                }
                if b.age >= b.delay && l.color[3] == 0.0 {
                    l.color[3] = 1.0;
                }
                return true;
            }
            if b.age >= BOLT_HOLD + BOLT_FADE {
                self.handles.remove(b.lightning);
                return false;
            }
            if b.age > BOLT_HOLD {
                let a = 1.0 - (b.age - BOLT_HOLD) / BOLT_FADE;
                l.color[3] = l.color[3].min(a.max(0.0));
            }
            true
        });
        self.bolts.extend(bolts);
        let mut jumps = std::mem::take(&mut self.jumps);
        let mut keep = Vec::new();
        for mut j in jumps.drain(..) {
            j.timer -= dt;
            if j.timer > 0.0 {
                keep.push(j);
                continue;
            }
            if j.first != 0 {
                let t = std::mem::take(&mut j.first);
                if self.unit(t).is_some_and(|u| !u.dead) {
                    self.strike(vm, &mut j, t);
                    if j.jumps_left > 0 {
                        keep.push(j);
                    }
                }
                continue;
            }
            let area = self.data(j.ability, "Area", j.level);
            let range = if area > 0.0 { area } else { 500.0 };
            let Some((px, py)) = self.widget_pos(j.from) else { continue };
            let next = self
                .units_in_pub(px, py, range)
                .into_iter()
                .filter(|t| !j.hit.contains(t))
                // The ability's own allowed targets choose the next one.
                .filter(|&t| self.cast_target_error(j.caster, j.ability, t).is_none())
                .filter(|&t| !j.heal || self.unit(t).is_some_and(|u| u.life < u.max_life))
                .min_by(|&p, &q| {
                    let dd = |t: u32| self.widget_pos(t).map_or(f32::MAX, |w| (w.0 - px).hypot(w.1 - py));
                    dd(p).total_cmp(&dd(q))
                });
            let Some(n) = next else { continue };
            self.strike(vm, &mut j, n);
            if j.jumps_left > 0 {
                keep.push(j);
            }
        }
        self.jumps.extend(keep);
    }
}
