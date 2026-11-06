//! An attack's missile as the original's objects: each missile a weapon sends (a
//! CBullet: CMissile, CArtillery, and their kin) is an agent of its own, made at the
//! damage point (`FUN_6f4946b0`) with its position (`` `pbh ``, NIpse::CPoPosBh, the
//! CBullet's +0x78), that position's two regions (`^pmr`), and its flight (`>prj`,
//! NIpse::CBhPoProjectile, started by `FUN_6f6d3e50`), in that order.  One an ability
//! sends counts itself on its target in a CAbilityTarget (`Atrg`) kept for that ability
//! and the attacker's owner, made with the first in flight and let go as the last
//! lands.  A missile lands, and BulletDeathTime (MiscData, 5) later it is gone with its
//! objects.

use crate::codes;
use crate::objects::Id;
use crate::sim::Weapon;
use crate::tags::Code;
use crate::world::World;

/// A missile's objects: their slots in the order made (the missile, its position, its
/// two regions, its flight), and the ability it counts itself on its target for, with
/// the attacker's owner (`Atrg`'s key, its +0x6c and +0x70); the game time its place is
/// as of (its sending, then each tick's end, then its landing), and its target's place
/// then.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Bullet {
    pub slots: [u32; 5],
    pub counted: Option<(Id, u8)>,
    #[serde(default)]
    pub at: f64,
    #[serde(default)]
    pub seen: Option<(f32, f32)>,
}

/// A unit's CAbilityTarget: the ability and owner it counts the missiles of, its slot,
/// and the missiles in flight.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct BulletTarget {
    pub unit: u32,
    pub ability: Id,
    pub owner: u8,
    pub slot: u32,
    pub count: u32,
}

/// The ability hooks' order (CAbility vf78: each kind's place, from 0x1b to 0x29, asked
/// in turn by `FUN_6f67fbd0` and its kin, the first to take the missile making it).
/// Slow Poison, Poison Attack, and Envenomed Spears take a missile weapon's at 0x28.
const POISON_PLACE: u32 = 0x28;
/// Frost Attack and Frost Breath take an artillery's, a splash's, and a missile's at 0x29.
const FROST_PLACE: u32 = 0x29;

/// An object let go is destroyed on the engine's next frame (its destruction an event
/// due at once): provisionally a main tick on (read: Mafarazzo TD's Plague Tower's
/// missiles, each landing a main tick or so before a turn's end, its `Atrg` and its
/// objects 5 seconds on freed in the next turn).
const DESTROYED_AFTER: f64 = 0.005;

impl World {
    /// The class of the missile a unit's weapon sends at `target` now, and the ability
    /// it counts itself on its target for, if any.  A weapon's own: CMissile (`B-Mi`,
    /// `FUN_6f497520`) for a missile, CArtillery (`B-Ar`, `FUN_6f4973c0`),
    /// CMissileSplash (`B-Ms`, `FUN_6f497730`), CMissileBounce (`B-Mb`, `FUN_6f4975d0`),
    /// CMissileLine (`B-Ml`, `FUN_6f497680`), and CArtilleryLine (`B-Al`,
    /// `FUN_6f497470`).  But first its abilities are asked, by their place (CAbility
    /// vf78, from 0x1b to 0x29), each kind's hook for the weapon's kind (`FUN_6f67fbd0`
    /// a missile's, vf84; `FUN_6f67f950` artillery's, vf80; `FUN_6f67fb30` a splash's,
    /// vf83; `FUN_6f67f9f0` a bounce's, vf82; `FUN_6f67fa90` a line's, vf81;
    /// `FUN_6f67f8b0` a line artillery's, vf79), the first to take it sending its own,
    /// while not disabled by the tech tree (vf154): at 0x28 Slow Poison, Poison Attack,
    /// and Envenomed Spears a missile weapon's at a unit their targets allow (vf84
    /// `FUN_6f632790`; Slow Poison's CMissileSlowPoison, `Mspo`, vf213 `FUN_6f62b870`,
    /// the others' CMissilePoisonAttack, `Mpoi`, `FUN_6f62b460`); at 0x29 Frost Attack
    /// and Frost Breath an artillery's (CArtilleryFrostBreath, `Rfrb`, vf80
    /// `FUN_6f590260`), a splash's (CMissileFrostBreath, `Mfrb`, vf83 `FUN_6f590350`,
    /// counted on nothing: `FUN_6f59e0b0`), and a missile's at a unit their targets
    /// allow (CMissileFrost, `Mfro`, vf84 `FUN_6f590440`).  (Provisional: Frost's own
    /// check of the target, `FUN_6f671e70`, is not read; the other kinds' hooks, an
    /// orb's, an arrow's, a critical strike's, Barrage's, Lightning Attack's, are not
    /// yet modelled: their weapons' own are taken.)
    pub(crate) fn bullet_class(&self, h: u32, target: u32, wp: &Weapon) -> (&'static Code, Option<Id>) {
        let kind = wp.weapon_type.as_str();
        let splash = kind.starts_with("msplash");
        let own = match kind {
            "artillery" => codes::class::ARTILLERY,
            "aline" => codes::class::ARTILLERY_LINE,
            "mbounce" => codes::class::MISSILE_BOUNCE,
            "mline" => codes::class::MISSILE_LINE,
            _ if splash => codes::class::MISSILE_SPLASH,
            _ => codes::class::MISSILE,
        };
        let Some(u) = self.unit(h) else { return (own, None) };
        let owner = u.owner;
        let at_unit = self.unit(target).is_some();
        // Each ability taking this weapon's kind: its place, whether its targets are
        // checked, its class, and whether it counts itself on the target.
        let mut asked: Vec<(u32, Id, bool, &'static Code, bool)> = Vec::new();
        for &(a, _) in &u.abilities {
            let take = match (self.ability_code(a).as_str(), kind) {
                (codes::ability::SLOW_POISON, "missile") => (POISON_PLACE, true, codes::class::SLOW_POISON_MISSILE, true),
                (codes::ability::POISON_ATTACK | codes::ability::VENOM_SPEARS, "missile") => (POISON_PLACE, true, codes::class::POISON_ATTACK_MISSILE, true),
                (codes::ability::FROST_ATTACK | codes::ability::FROST_BREATH, "missile") => (FROST_PLACE, true, codes::class::FROST_MISSILE, true),
                (codes::ability::FROST_ATTACK | codes::ability::FROST_BREATH, "artillery") => (FROST_PLACE, false, codes::class::FROST_ARTILLERY, false),
                (codes::ability::FROST_ATTACK | codes::ability::FROST_BREATH, _) if splash => (FROST_PLACE, false, codes::class::FROST_SPLASH_MISSILE, false),
                _ => continue,
            };
            asked.push((take.0, a, take.1, take.2, take.3));
        }
        // (Each place in turn; within one, the abilities in the unit's order.)
        asked.sort_by_key(|x| x.0);
        for (_, a, checked, class, counts) in asked {
            if !self.ability_enabled(owner, a) || checked && !(at_unit && self.cast_target_error(h, a, target).is_none()) {
                continue;
            }
            return (class, counts.then_some(a));
        }
        (own, None)
    }

    /// A missile's objects as it is sent at `at` (game time): the missile, its position,
    /// the position's two regions, and its flight (traced: Mafarazzo TD's Plague Tower's
    /// `Mspo`, `` `pbh ``, `^pmr`, `^pmr`, `>prj` at each damage point; a test map's
    /// Arrow, Cannon, Ice, and Cutter towers' `B-Mi`, `B-Ar`, `Mfrb`, and `B-Mb` so);
    /// then, for one an ability sends, its count on its target (`FUN_6f6d2c50`,
    /// `FUN_6f48dfa0`): the target's CAbilityTarget for that ability and the attacker's
    /// owner, made now if it has none (traced: `Atrg` after the `>prj`, with the first of
    /// the Plague Tower's missiles in flight at its target).
    pub(crate) fn bullet_sent(&mut self, h: u32, target: u32, class: &Code, ability: Option<Id>, at: f64) -> crate::bullets::Bullet {
        let mut slots = [0u32; 5];
        for (k, c) in [class, codes::class::PROJECTILE_POSITION, codes::class::PATHING_REGION, codes::class::PATHING_REGION, codes::class::PROJECTILE].into_iter().enumerate() {
            slots[k] = self.tags.take(c);
        }
        let owner = self.unit(h).map_or(0, |u| u.owner);
        let counted = ability.filter(|_| self.unit(target).is_some()).map(|a| (a, owner));
        if let Some((a, p)) = counted {
            match self.bullet_targets.iter_mut().find(|t| t.unit == target && t.ability == a && t.owner == p) {
                Some(t) => t.count += 1,
                None => {
                    let slot = self.tags.take(codes::ability::id::TARGET_CLASS);
                    self.bullet_targets.push(BulletTarget { unit: target, ability: a, owner: p, slot, count: 1 });
                }
            }
        }
        if class == codes::class::MISSILE_BOUNCE {
            self.bounce_marked(target);
        }
        let seen = self.unit(target).map(|t| (t.x, t.y));
        Bullet { slots, counted, at, seen }
    }

    /// A bouncing missile marks each unit it is sent at, the first as it is sent and
    /// each next as it bounces on (CMissileBounce vf50 `FUN_6f6d3790` and vf47
    /// `FUN_6f6cfbc0`, `FUN_6f6d4700`: a buff, `BTAR`, given to the unit; traced: a test
    /// map's Cutter tower's after each missile's `>prj`).  (Provisional: how long it
    /// lasts; traced kept 2.7 seconds, then freed with its unit's other buffs as the
    /// unit was removed, the newest first.)
    pub(crate) fn bounce_marked(&mut self, target: u32) {
        if self.unit(target).is_none() {
            return;
        }
        let (slot, birth) = self.tags.take_id(codes::class::BOUNCE_TARGET);
        if let Some(u) = self.unit_mut(target) {
            u.extra.object_slots[2].insert(0, (slot, birth));
        }
    }

    /// The main clock (seconds since the game began) at game time `t` of this tick.
    pub(crate) fn bullet_clock(&self, t: f64) -> f64 {
        let end = self.tick_abs(self.tick_clocks.1.len().saturating_sub(1)).unwrap_or(self.time_exact);
        end + (t - self.time_exact)
    }

    /// A missile's count on its target let go, as it lands or its target goes
    /// (`FUN_6f48c7f0`, `FUN_6f589120`): with the last, the target's CAbilityTarget is
    /// let go (`FUN_6f48e8a0`) and destroyed (read: the Plague Tower's first `Atrg`, its
    /// missile landing at the end of turn 9, freed first thing in turn 10; the third's
    /// kept while later missiles flew, freed the turn after the last of them landed).
    pub(crate) fn bullet_uncounted(&mut self, b: &mut Bullet, target: u32) {
        let Some((a, p)) = b.counted.take() else { return };
        let Some(i) = self.bullet_targets.iter().position(|t| t.unit == target && t.ability == a && t.owner == p) else { return };
        let t = &mut self.bullet_targets[i];
        t.count = t.count.saturating_sub(1);
        if t.count == 0 {
            let t = self.bullet_targets.remove(i);
            let due = self.bullet_clock(b.at) + DESTROYED_AFTER;
            self.tags.doom_at(due, t.slot);
        }
    }

    /// A missile has landed (or lost its target) at its `at`: its count on its target
    /// let go, and its removal timer started (CBullet vf42, `FUN_6f6d0c90`:
    /// BulletDeathTime, event 0xd019f), at whose end it is destroyed (vf3
    /// `FUN_6f6d4e30`, vf23) with its objects: its flight, its regions, its position,
    /// then itself (traced: `>prj`, `^pmr`, `^pmr`, `` `pbh ``, `Mspo`, in the turn
    /// after the one 5 seconds past its landing, or later in it: the Arrow tower's
    /// after the Cannon tower's next missile was sent).
    pub(crate) fn bullet_landed(&mut self, mut b: Bullet, target: u32) {
        self.bullet_uncounted(&mut b, target);
        let wait = self.misc_f32("BulletDeathTime", 5.0).max(0.0) as f64;
        let due = self.bullet_clock(b.at) + wait + DESTROYED_AFTER;
        let s = b.slots;
        for k in [4, 2, 3, 1, 0] {
            self.tags.doom_at(due, s[k]);
        }
    }

    /// A missile's way on to `end` (game time): toward its target, at `to` then, at its
    /// speed; it lands on reaching the target's edge.  The
    /// original's flight (NIpse::CBhPoProjectile, `FUN_6f173450`) falls due at its
    /// distance over its speed, aimed anew as its target moves (`FUN_6f1739e0`); here the
    /// target is taken to move evenly through the tick from where it was last seen.
    /// Returns the game time it lands, if it does this tick (its place then moved
    /// there), else moves it on.
    pub(crate) fn bullet_flies(m: &mut crate::sim::Missile, to: (f32, f32), edge: f32, end: f64) -> Option<f64> {
        let b = m.bullet.as_mut()?;
        // (From its sending, its last landing for a bounce, or where it was last flown.)
        let t0 = b.at.clamp(end - 1.0, end);
        let avail = end - t0;
        let seen = b.seen.unwrap_or(to);
        let v = if avail > 1e-6 { (((to.0 - seen.0) as f64) / avail, ((to.1 - seen.1) as f64) / avail) } else { (0.0, 0.0) };
        let (dx, dy) = ((seen.0 - m.x) as f64, (seen.1 - m.y) as f64);
        let (s, e) = (m.speed as f64, edge as f64);
        // |D + v τ| = s τ + e: (v·v − s²) τ² + 2 (D·v − s e) τ + (D·D − e²) = 0.
        let (a, bb, c) = (v.0 * v.0 + v.1 * v.1 - s * s, 2.0 * (dx * v.0 + dy * v.1 - s * e), dx * dx + dy * dy - e * e);
        let tau = if c <= 0.0 {
            Some(0.0)
        } else if a.abs() < 1e-9 {
            (bb < 0.0).then(|| -c / bb)
        } else {
            let disc = bb * bb - 4.0 * a * c;
            (disc >= 0.0).then(|| {
                let r = disc.sqrt();
                let (x, y) = ((-bb - r) / (2.0 * a), (-bb + r) / (2.0 * a));
                [x, y].into_iter().filter(|t| *t >= 0.0).fold(f64::INFINITY, f64::min)
            }).filter(|t| t.is_finite())
        };
        if let Some(tau) = tau.filter(|&t| t <= avail + 1e-9) {
            let (px, py) = (seen.0 as f64 + v.0 * tau, seen.1 as f64 + v.1 * tau);
            m.x = px as f32;
            m.y = py as f32;
            b.at = t0 + tau;
            return Some(t0 + tau);
        }
        let (tx, ty) = ((to.0 - m.x) as f64, (to.1 - m.y) as f64);
        let d = (tx * tx + ty * ty).sqrt();
        let step = (s * avail).min(d);
        if d > 0.0 {
            m.x += (tx / d * step) as f32;
            m.y += (ty / d * step) as f32;
        }
        b.at = end;
        b.seen = Some(to);
        None
    }
}
