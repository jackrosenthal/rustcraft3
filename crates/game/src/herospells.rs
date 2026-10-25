//! Hero spells of the Frozen Throne and neutral heroes, from their AbilityData fields as
//! AbilityMetaData names them: Acid Bomb, Breath of Fire, Charm, Cluster Rockets, Dark
//! Conversion, Drunken Haze, Doom, Dark Portal, Storm Earth and Fire, Finger of Death,
//! Forked Lightning, Healing Spray, Howl of Terror, Summon Lava Spawn, Mana Shield, Rain
//! of Chaos, the Beast Master's summons, Silence, Soul Burn, Transmute, Tornado, and Hex.

use w3_jass::Vm;

use crate::objects::{Id, id_from_str};
use crate::types::*;
use crate::world::World;

impl World {
    fn lstr(&self, a: Id, field: &str, lvl: u32) -> String {
        self.objects.abilities.get(a).map(|o| o.level_str(field, lvl)).unwrap_or_default().trim().to_string()
    }

    fn buff0(&self, a: Id, lvl: u32) -> Id {
        id_from_str(self.lstr(a, "BuffID", lvl).split(',').next().unwrap_or("").trim())
    }

    /// Dark Conversion takes only units of its DataA race (commoner).
    pub(crate) fn converts_race(&self, a: Id, lvl: u32, target: u32) -> bool {
        let race = self.lstr(a, "DataA", lvl).to_ascii_lowercase();
        race.is_empty() || race == "-" || self.unit(target).and_then(|u| self.objects.units.get(u.type_id)).is_some_and(|o| o.str("race").eq_ignore_ascii_case(&race))
    }

    pub(crate) fn hero_spell_effect(&mut self, vm: &mut Vm, h: u32, a: Id, lvl: u32, code: &str, target: u32, (x, y): (f32, f32)) -> bool {
        let d = |w: &World, f: &str| w.data(a, f, lvl);
        let owner = self.unit(h).map_or(0, |u| u.owner);
        let buff = self.buff0(a, lvl);
        let dur = if self.hero_duration(target) { d(self, "HeroDur") } else { d(self, "Dur") };
        let (cx, cy) = self.unit(h).map(|u| (u.x, u.y)).unwrap_or((x, y));
        let (tx, ty) = self.widget_pos(target).unwrap_or((x, y));
        let area = d(self, "Area");
        match code {
            // Acid Bomb: the target loses DataC armor and takes DataD every DataF; others
            // within Area take DataE every DataF.
            "ANab" => {
                let every = d(self, "DataF").max(0.01);
                for t in self.enemies_in(owner, tx, ty, area) {
                    let dps = if t == target { d(self, "DataD") } else { d(self, "DataE") } / every;
                    let armor = if t == target { -d(self, "DataC") } else { 0.0 };
                    let dur = if self.hero_duration(t) { d(self, "HeroDur") } else { d(self, "Dur") };
                    self.apply_buff(t, Buff { id: buff, ability: a, level: lvl, caster: h, remaining: dur, dps, armor, ..Default::default() });
                }
            }
            // Breath of Fire: a cone DataC long widening from Area to DataD; DataA each up to
            // DataB in all, then DataE a second for Dur.
            "ANbf" => {
                let len = d(self, "DataC");
                let dir = (x - cx, y - cy);
                let n = dir.0.hypot(dir.1).max(1.0);
                let end = (cx + dir.0 / n * len, cy + dir.1 / n * len);
                let targets = self.line_units_pub(owner, (cx, cy), end, area * 2.0, d(self, "DataD") * 2.0);
                self.capped_damage_pub(vm, h, &targets, d(self, "DataA"), d(self, "DataB"));
                for t in targets {
                    self.apply_buff(t, Buff { id: buff, ability: a, level: lvl, caster: h, remaining: dur, dps: d(self, "DataE"), ..Default::default() });
                }
            }
            // Charm and Dark Conversion: the target becomes the caster's (Dark Conversion
            // also turns it into UnitID).
            "ANch" => {
                if let Some(u) = self.unit_mut(target).filter(|u| !u.is_hero) {
                    u.owner = owner;
                    u.action = Action::Idle;
                }
            }
            // Soul Preservation (CAbilitySoulPreservation, CBuffSoulPreservation): the
            // target is held, untouchable, for Dur (HeroDur); then it is the caster's
            // (update_buffs).
            "ANsl" => {
                if self.unit(target).is_some_and(|u| !u.is_hero) {
                    let dur = self.duration_for(a, lvl, target);
                    self.apply_buff(target, Buff { id: buff, ability: a, level: lvl, caster: h, remaining: dur, stun: true, invulnerable: true, magic_immune: true, ..Default::default() });
                    if let Some(u) = self.unit_mut(target) {
                        u.action = Action::Idle;
                    }
                }
            }
            // Dark Conversion (CAbilityDarkConversion, CBuffDarkConversion): the target
            // sleeps under the buff for Dur (HeroDur); when it ends with the target alive
            // the target becomes UnitID for the caster (update_buffs).
            "ANdc" => {
                if self.unit(target).is_some_and(|u| !u.is_hero) && self.converts_race(a, lvl, target) {
                    let dur = self.duration_for(a, lvl, target);
                    self.apply_buff(target, Buff { id: buff, ability: a, level: lvl, caster: h, remaining: dur, stun: true, ..Default::default() });
                    if let Some(u) = self.unit_mut(target) {
                        u.action = Action::Idle;
                    }
                }
            }
            // Cluster Rockets: DataA every DataB for DataF within Area (at most DataD each),
            // structures taking DataE of it, and a stun for Dur.
            "ANcs" => {
                let total = d(self, "DataA") * (d(self, "DataF") / d(self, "DataB").max(0.01)).floor();
                let each = total.min(d(self, "DataD"));
                for t in self.enemies_in(owner, x, y, area) {
                    let f = if self.unit(t).is_some_and(|u| u.is_building) { d(self, "DataE") } else { 1.0 };
                    self.spell_damage(vm, h, t, each * f);
                    let dur = if self.hero_duration(t) { d(self, "HeroDur") } else { d(self, "Dur") };
                    self.apply_buff(t, Buff { id: buff, ability: a, level: lvl, caster: h, remaining: dur, stun: true, ..Default::default() });
                }
            }
            // Drunken Haze: within Area, DataB more misses and DataC slower movement.
            "ANdh" => {
                for t in self.enemies_in(owner, tx, ty, area) {
                    let dur = if self.hero_duration(t) { d(self, "HeroDur") } else { d(self, "Dur") };
                    self.apply_buff(t, Buff { id: buff, ability: a, level: lvl, caster: h, remaining: dur, miss_chance: d(self, "DataB"), move_factor: -d(self, "DataC"), attack_factor: -d(self, "DataD"), ..Default::default() });
                }
            }
            // Doom: DataA a second, silenced, until death; then DataB of UnitID rise for
            // DataC (see on_death_buffs).
            "ANdo" => self.apply_buff(target, Buff { id: buff, ability: a, level: lvl, caster: h, remaining: f32::INFINITY, dps: d(self, "DataA"), silence: true, ..Default::default() }),
            // Dark Portal: between DataB and DataC of the DataA types come through.
            "ANdp" => {
                let types: Vec<Id> = self.lstr(a, "DataA", lvl).split(',').map(|s| id_from_str(s.trim())).filter(|i| i[0] != 0).collect();
                let (lo, hi) = (d(self, "DataB") as i32, d(self, "DataC") as i32);
                let n = self.rng.int(lo, hi.max(lo));
                for k in 0..n {
                    let t = types[(k as usize) % types.len().max(1)];
                    self.summon(h, owner, t, 1, x, y, 0.0);
                }
            }
            // Storm, Earth and Fire: the hero splits into the DataA spirits for Dur.
            "ANef" => {
                let types: Vec<Id> = self.lstr(a, "DataA", lvl).split(',').map(|s| id_from_str(s.trim())).filter(|i| i[0] != 0).collect();
                let life = d(self, "Dur");
                let mut spirits = Vec::new();
                for t in types {
                    spirits.extend(self.summon(h, owner, t, 1, cx, cy, life));
                }
                if let Some(u) = self.unit_mut(h) {
                    u.hidden = true;
                    u.action = Action::Idle;
                    u.extra.split = Some((life, spirits));
                }
            }
            // Finger of Death: DataC damage.
            "ANfd" => {
                self.ability_bolt(a, true, h, target);
                self.spell_damage(vm, h, target, d(self, "DataC"))
            }
            // Forked Lightning: DataA to at most DataB in a cone DataC long ending DataD wide.
            "ANfl" => {
                let len = d(self, "DataC");
                let dir = (tx - cx, ty - cy);
                let n = dir.0.hypot(dir.1).max(1.0);
                let end = (cx + dir.0 / n * len, cy + dir.1 / n * len);
                let mut targets = self.line_units_pub(owner, (cx, cy), end, area * 2.0, d(self, "DataD") * 2.0);
                targets.retain(|&t| t != target);
                targets.insert(0, target);
                targets.truncate(d(self, "DataB").max(1.0) as usize);
                for t in targets {
                    self.ability_bolt(a, true, h, t);
                    self.spell_damage(vm, h, t, d(self, "DataA"));
                }
            }
            // Healing Spray: DataF waves of DataA within Area.
            "ANhs" => {
                let total = d(self, "DataA") * d(self, "DataF").max(1.0);
                for t in self.allies_in_pub(owner, x, y, area) {
                    self.magic_heal(t, total.min(d(self, "DataD").max(total)));
                }
            }
            // Howl of Terror: enemies within Area deal DataA less damage.
            "ANht" => {
                for t in self.enemies_in(owner, cx, cy, area) {
                    let dur = if self.hero_duration(t) { d(self, "HeroDur") } else { d(self, "Dur") };
                    self.apply_buff(t, Buff { id: buff, ability: a, level: lvl, caster: h, remaining: dur, damage_factor: -d(self, "DataA"), ..Default::default() });
                }
            }
            // The Beast Master's and Firelord's summons: DataA of UnitID for Dur.
            "ANsg" | "ANsq" | "ANsw" | "ANlm" => {
                let unit = id_from_str(&self.lstr(a, "UnitID", lvl));
                let n = d(self, "DataA").max(1.0) as i32;
                self.summon(h, owner, unit, n, cx, cy, d(self, "Dur"));
            }
            // Mana Shield: damage taken comes off mana, DataA mana a point, DataB of it.
            "ANms" => {
                let on = !self.unit(h).is_some_and(|u| u.buffs.iter().any(|b| b.ability == a));
                if let Some(u) = self.unit_mut(h) {
                    u.buffs.retain(|b| b.ability != a);
                }
                if on {
                    self.apply_buff(h, Buff { id: buff, ability: a, level: lvl, caster: h, remaining: f32::INFINITY, mana_shield: (d(self, "DataA"), d(self, "DataB")), ..Default::default() });
                }
            }
            // Rain of Chaos: DataB units of the DataA ability's UnitID land within Area.
            "ANrc" => {
                let inf = id_from_str(&self.lstr(a, "DataA", lvl));
                let unit = id_from_str(self.objects.abilities.get(inf).map(|o| o.level_str("UnitID", 1)).unwrap_or_default().trim());
                let life = self.objects.abilities.get(inf).map_or(0.0, |o| o.level_real("Dur", 1));
                for _ in 0..d(self, "DataB") as i32 {
                    let ang = self.rng.int(0, 359) as f32;
                    let r = self.rng.int(0, area as i32) as f32;
                    let (px, py) = (x + r * ang.to_radians().cos(), y + r * ang.to_radians().sin());
                    self.summon(h, owner, unit, 1, px, py, life);
                }
            }
            // Silence: no spells within Area for Dur.
            "ANsi" => {
                for t in self.enemies_in(owner, x, y, area) {
                    let dur = if self.hero_duration(t) { d(self, "HeroDur") } else { d(self, "Dur") };
                    self.apply_buff(t, Buff { id: buff, ability: a, level: lvl, caster: h, remaining: dur, silence: true, ..Default::default() });
                }
            }
            // Soul Burn: DataA every DataB, DataC less damage, silenced.
            "ANso" => self.apply_buff(target, Buff { id: buff, ability: a, level: lvl, caster: h, remaining: dur, dps: d(self, "DataA") / d(self, "DataB").max(0.01), damage_factor: -d(self, "DataC"), silence: true, ..Default::default() }),
            // Transmute: a unit up to level DataC dies for DataA of its gold cost.
            "ANtm" => {
                let Some(u) = self.unit(target).filter(|u| !u.is_hero && u.level <= d(self, "DataC") as i32) else { return true };
                let gold = self.objects.units.get(u.type_id).map_or(0.0, |o| o.real("goldcost")) * d(self, "DataA");
                *self.players[owner as usize].state.entry(player_state::RESOURCE_GOLD).or_insert(0) += gold as i32;
                crate::natives::units::kill_unit(self, vm, target, h);
            }
            // Tornado: UnitID at the point for Dur.
            "ANto" => {
                let unit = id_from_str(&self.lstr(a, "UnitID", lvl));
                self.summon(h, owner, unit, 1, x, y, d(self, "Dur"));
            }
            // Hex: the target becomes one of the critters for its movement (DataB ground,
            // DataC air, DataD amphibious, DataE water) for Dur.
            "AOhx" => {
                let field = match self.move_kind(target) {
                    crate::pathing::Move::Fly => "DataC",
                    crate::pathing::Move::Amphibious => "DataD",
                    crate::pathing::Move::Float => "DataE",
                    _ => "DataB",
                };
                let list: Vec<Id> = self.lstr(a, field, lvl).split(',').map(|s| id_from_str(s.trim())).filter(|i| i[0] != 0).collect();
                if list.is_empty() {
                    return true;
                }
                let to = list[self.rng.int(0, list.len() as i32 - 1) as usize];
                // As Polymorph: the unit stays what it was, shown as the critter.
                let speed = self.objects.units.get(to).map_or(0.0, |o| o.real("spd"));
                self.apply_buff(target, Buff { id: buff, ability: a, level: lvl, caster: h, remaining: dur, shown_as: Some(to), move_set: speed, disarm: 3, silence: true, ..Default::default() });
            }
            _ => return false,
        }
        true
    }

    /// Storm, Earth and Fire ends: the hero returns where a spirit stands (or where it
    /// split).
    pub fn update_splits(&mut self, dt: f32) {
        let split: Vec<u32> = self.unit_order.iter().copied().filter(|&h| self.unit(h).is_some_and(|u| u.extra.split.is_some())).collect();
        for h in split {
            let (left, spirits) = self.unit(h).and_then(|u| u.extra.split.clone()).unwrap();
            let alive: Vec<u32> = spirits.iter().copied().filter(|&s| self.unit(s).is_some_and(|u| !u.dead)).collect();
            let left = left - dt;
            if left > 0.0 && !alive.is_empty() {
                self.unit_mut(h).unwrap().extra.split = Some((left, spirits));
                continue;
            }
            let at = alive.first().and_then(|&s| self.unit(s).map(|u| (u.x, u.y)));
            for s in alive {
                crate::natives::units::remove_unit(self, s);
            }
            if let Some(u) = self.unit_mut(h) {
                u.extra.split = None;
                u.hidden = false;
                if let Some((x, y)) = at {
                    u.x = x;
                    u.y = y;
                }
            }
        }
    }
}
