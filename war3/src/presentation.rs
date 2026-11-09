//! Display a precomputed turn on the original's clock, without resimulating it.
//!
//! game.dll: FUN_6f32d670 consumes 25 ms of real time per 15/20/25 ms of
//! game time. FUN_6f684480 -> FUN_6f05a970 -> FUN_6f161040/FUN_6f05bdd0
//! evaluates a unit's position from its last velocity change at that clock.
//! FUN_6f385c10 advances text tags by the consumed game time via FUN_6f9a1910.
use std::collections::HashMap;
use glam::{Vec2, Vec3};
use w3_game::{World, motion::Mover, sim::Missile, types::Unit};

struct UnitPose {
    type_id: [u8; 4],
    position: Vec3,
    facing: f32,
    mover: Option<Mover>,
    wraps: u32,
    walking: bool,
}

#[derive(Default)]
pub(crate) struct Snapshot {
    time: f32,
    ms: u32,
    turn_left: u32,
    units: HashMap<u32, UnitPose>,
    pub missiles: HashMap<u32, Missile>,
}

impl Snapshot {
    pub fn capture(w: &World) -> Self {
        Self {
            time: w.time,
            ms: w.game_ms,
            turn_left: w.turn_left,
            units: w.unit_order.iter().filter_map(|&h| {
                let u = w.unit(h)?;
                Some((h, UnitPose {
                    type_id: u.type_id,
                    position: Vec3::new(u.x, u.y, u.extra.shown_height.unwrap_or(u.fly_height)),
                    facing: u.shown_facing.unwrap_or(u.facing),
                    mover: u.extra.mover,
                    wraps: w.clock_wraps,
                    walking: !u.dead && !u.paused && u.anim.starts_with("walk"),
                }))
            }).collect(),
            missiles: w.missiles.iter().map(|m| (m.id, m.clone())).collect(),
        }
    }
}

#[derive(Default)]
pub(crate) struct Presentation {
    pub previous: Snapshot,
    end_time: f32,
    end_ms: u32,
    tick_ms: u32,
    pub elapsed: f32,
}

impl Presentation {
    pub fn turn(&mut self, mut previous: Snapshot, w: &World, duration: f32) {
        if w.game_ms > previous.ms && duration > 0.0 {
            // Include missiles launched and landed within the computed turn, which
            // otherwise disappear before the renderer ever has a chance to draw them.
            for m in w.render_missiles.values() {
                previous.missiles.entry(m.id).or_insert_with(|| m.clone());
            }
            self.previous = previous;
            self.end_time = w.time;
            self.end_ms = w.game_ms;
            self.tick_ms = [15, 20, 25][w.game_speed.clamp(0, 2) as usize];
            self.elapsed = 0.0;
        } else if duration > 0.0 {
            *self = Self::default();
        }
    }

    fn ms(&self) -> u32 {
        let ticks = (self.previous.turn_left + (self.elapsed.max(0.0) * 1000.0 + 0.0001) as u32) / w3_game::TICK_MS;
        self.previous.ms.saturating_add(ticks.saturating_mul(self.tick_ms)).min(self.end_ms)
    }

    pub fn alpha(&self) -> f32 {
        let span = self.end_ms.saturating_sub(self.previous.ms);
        if span == 0 { 1.0 } else { (self.ms() - self.previous.ms) as f32 / span as f32 }
    }

    pub fn lag(&self) -> f32 {
        (self.end_time - self.previous.time) * (1.0 - self.alpha())
    }

    /// The engine's main clock at the displayed 5 ms tick. Read the already planned
    /// clocks; calling main_clock_at here would mutate authoritative clock state.
    fn clock(&self, w: &World) -> (u32, u32) {
        let tick = (self.ms() as i64 + 4) / 5;
        let i = tick.saturating_sub(w.tick_clocks.0) as usize;
        (w.tick_clocks.1.get(i).copied().unwrap_or(w.main_clock.1),
         w.tick_wraps.get(i).copied().unwrap_or(w.main_wraps))
    }

    pub fn unit(&self, w: &World, u: &Unit) -> (Vec3, f32) {
        let position = Vec3::new(u.x, u.y, u.extra.shown_height.unwrap_or(u.fly_height));
        let facing = u.shown_facing.unwrap_or(u.facing);
        // A mover can outlive its order (normal corpses retain it). It is not
        // permission to keep integrating velocity after death or PauseUnit.
        if u.dead || u.paused {
            return (position, facing);
        }
        let Some(old) = self.previous.units.get(&u.handle) else { return (position, facing) };
        if old.type_id != u.type_id || old.position.distance_squared(position) > 256.0 * 256.0 {
            return (position, facing);
        }
        let samples = w.render_walks.get(&u.handle);
        if samples.is_none() && u.extra.mover.is_none() {
            // Plain flying Move orders still use walk_inner, which computes one
            // endpoint per authoritative turn and has no Mover/history to sample.
            // Present that bounded leg on the same display clock as engine walks.
            if old.walking || u.anim.starts_with("walk") {
                return (old.position.lerp(position, self.alpha()), facing);
            }
            return (position, facing);
        }
        if samples.is_none() && !u.steps_walker() {
            return (position, facing);
        }
        let sample = samples.and_then(|v| v.iter().rev().find(|(tick, _, _)| *tick * 5 <= self.ms() as i64));
        let state = sample.map(|(_, wraps, m)| (*m, *wraps)).or(old.mover.map(|m| (m, old.wraps)));
        let Some((m, wraps)) = state else {
            return if samples.is_some() { (old.position, old.facing) } else { (position, facing) };
        };
        let Some(pathing) = w.pathing.as_ref() else { return (position, facing) };
        let (mut clock, clock_wraps) = self.clock(w);
        if clock_wraps > wraps { clock = w3_game::efloat::add(clock, w3_game::motion::WRAP); }
        let (x, y) = m.at(clock);
        let world = |c, origin: f32| f32::from_bits(w3_game::efloat::add(w3_game::efloat::mul(c, w3_game::pathing::CELL.to_bits()), origin.to_bits()));
        (Vec3::new(world(x, pathing.origin[0]), world(y, pathing.origin[1]), position.z),
         u.shown_facing.unwrap_or(f32::from_bits(m.heading).to_degrees()))
    }

    /// Straight attack flight, as FUN_6f173450/FUN_6f1737d0: launch + velocity
    /// times elapsed clock time, to the fixed intercept point, never retargeted.
    pub fn missile(&self, w: &World, m: &Missile, target: Vec2) -> Option<(Vec2, bool)> {
        if self.tick_ms == 0 { return Some((Vec2::new(m.x, m.y), false)) }
        if let Some((due, aim)) = m.bullet.as_ref().and_then(|b| Some((b.due?, b.seen?))) {
            let start = Vec2::from(m.start);
            let aim = Vec2::from(aim);
            let distance = (aim.x as f64 - start.x as f64).hypot(aim.y as f64 - start.y as f64);
            let flight = (distance / m.speed.max(0.001) as f64).clamp(0.0001, 60.0);
            let sent = due - flight;
            let (clock, wraps) = self.clock(w);
            let now = f32::from_bits(clock) as f64 + wraps as f64 * 300.0;
            if now + 1e-7 < sent { return None }
            let f = ((now - sent) / flight).clamp(0.0, 1.0) as f32;
            return Some((start + (aim - start) * f, now >= due));
        }
        // Spell/legacy missiles already have their simulated endpoints.
        let from = self.previous.missiles.get(&m.id).map_or(Vec2::from(m.start), |old| Vec2::new(old.x, old.y));
        let live = w.missiles.iter().any(|live| live.id == m.id);
        let to = if live { Vec2::new(m.x, m.y) } else { target };
        Some((from.lerp(to, self.alpha()), !live && self.alpha() >= 1.0))
    }

    pub fn text_age(&self, age: f32, suspended: bool) -> f32 {
        if suspended { age } else { (age - self.lag()).max(0.0) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world() -> World {
        World::new(std::sync::Arc::new(w3_mpq::Chain::new()), w3_game::objects::ObjectDb::default())
    }

    fn peasant() -> (World, w3_jass::Vm, u32) {
        let mut w = world();
        w.game_speed = 2;
        w.capture_render_motion = true;
        let mut o = w3_game::objects::Object { id: *b"hpea", ..Default::default() };
        for (key, value) in [("movetp", "fly"), ("spd", "400"), ("turnrate", "0.5"), ("propwin", "360"), ("hp", "100"), ("death", "3"), ("deathtype", "2")] {
            o.fields.insert(key.into(), value.into());
        }
        w.objects.units.objects.insert(o.id, o);
        let mut wpm = b"MP3W".to_vec();
        for v in [0u32, 16, 16] { wpm.extend(v.to_le_bytes()); }
        wpm.extend([0; 256]);
        w.pathing = w3_game::pathing::PathingMap::parse(&wpm, [0.0, 0.0]);
        let h = w3_game::natives::units::create_unit(&mut w, 0, *b"hpea", 48.0, 48.0, 0.0);
        let module = w3_jass::compile([("test.j", "function main takes nothing returns nothing\nendfunction\n")]).unwrap();
        (w, w3_jass::Vm::new(module), h)
    }

    #[test]
    fn flying_peasant_move_orders_progress_between_lan_turns_without_a_mover() {
        let (mut w, mut vm, h) = peasant();
        w.ordering_player = true;
        w.order_unit(h, w3_game::natives::basic::order_id("move"), 448.0, 48.0, 0);
        let mut view = Presentation::default();
        for _ in 0..3 {
            let before = Snapshot::capture(&w);
            let start = w.unit(h).unwrap().x;
            w.tick(&mut vm, 0.1);
            let u = w.unit(h).unwrap();
            assert!(u.extra.mover.is_none(), "exercise the flying Move path that has no engine history");
            assert!(u.x > start);
            let end = u.x;
            let checksum = w.keepalive();
            view.turn(before, &w, 0.1);
            for ms in [0, 25, 50, 75, 100] {
                view.elapsed = ms as f32 / 1000.0;
                let x = view.unit(&w, w.unit(h).unwrap()).0.x;
                assert!((x - (start + (end - start) * ms as f32 / 100.0)).abs() < 0.001);
                assert_eq!(w.keepalive(), checksum);
            }
        }
    }

    #[test]
    fn a_normal_corpse_with_a_retained_mover_never_drifts() {
        let (mut w, mut vm, h) = peasant();
        w.unit_mut(h).unwrap().extra.mover = Some(Mover {
            x: 1.5f32.to_bits(), y: 1.5f32.to_bits(), vx: 10.0f32.to_bits(), ..Default::default()
        });
        w3_game::natives::units::kill_unit(&mut w, &mut vm, h, 0);
        assert!(w.unit(h).unwrap().dead);
        assert!(w.unit(h).unwrap().extra.mover.is_some(), "normal death retains this simulation metadata");
        let position = Vec3::new(48.0, 48.0, 0.0);
        let mut view = Presentation::default();
        for _ in 0..20 {
            let before = Snapshot::capture(&w);
            w.tick(&mut vm, 0.1);
            view.turn(before, &w, 0.1);
            for ms in [0, 25, 50, 75, 100] {
                view.elapsed = ms as f32 / 1000.0;
                assert_eq!(view.unit(&w, w.unit(h).unwrap()).0, position);
            }
        }
    }

    #[test]
    fn a_paused_unit_cannot_move_from_its_retained_velocity() {
        let (mut w, mut vm, h) = peasant();
        let u = w.unit_mut(h).unwrap();
        u.paused = true;
        u.extra.mover = Some(Mover {
            x: 1.5f32.to_bits(), y: 1.5f32.to_bits(), vx: 10.0f32.to_bits(), ..Default::default()
        });
        let before = Snapshot::capture(&w);
        w.tick(&mut vm, 0.1);
        let mut view = Presentation::default();
        view.turn(before, &w, 0.1);
        view.elapsed = 0.075;
        assert_eq!(view.unit(&w, w.unit(h).unwrap()).0, Vec3::new(48.0, 48.0, 0.0));
    }

    #[test]
    fn original_clock_consumes_turn_time_in_25_ms_portions_at_each_speed() {
        for (speed, game_tick) in [(0, 15), (1, 20), (2, 25)] {
            let mut w = world();
            w.game_speed = speed;
            let before = Snapshot::capture(&w);
            w.game_ms = 4 * game_tick;
            w.time = w.game_ms as f32 / 1000.0;
            let mut view = Presentation::default();
            view.turn(before, &w, 0.1);
            for ms in 0..=100 {
                view.elapsed = ms as f32 / 1000.0;
                assert_eq!(view.ms(), ms / 25 * game_tick);
            }
            view.elapsed = 5.0;
            assert_eq!(view.ms(), w.game_ms, "a silent host must not allow extrapolation");
        }
    }

    #[test]
    fn unit_uses_recorded_velocity_changes_and_original_main_clock() {
        let mut w = world();
        w.game_speed = 2;
        w.objects.units.objects.insert(*b"hfoo", w3_game::objects::Object { id: *b"hfoo", ..Default::default() });
        let h = w3_game::natives::units::create_unit(&mut w, 0, *b"hfoo", 0.0, 0.0, 0.0);
        let mut wpm = b"MP3W".to_vec();
        wpm.extend(0u32.to_le_bytes());
        wpm.extend(1u32.to_le_bytes());
        wpm.extend(1u32.to_le_bytes());
        wpm.push(0);
        w.pathing = w3_game::pathing::PathingMap::parse(&wpm, [0.0, 0.0]);
        let moving = Mover { vx: 10.0f32.to_bits(), ..Default::default() };
        w.unit_mut(h).unwrap().extra.mover = Some(moving);
        let before = Snapshot::capture(&w);
        w.game_ms = 100;
        w.time = 0.1;
        let stopped = Mover { x: 0.5f32.to_bits(), placed_at: 0.05f32.to_bits(), ..Default::default() };
        w.render_walks.insert(h, vec![(10, 0, stopped)]);
        w.tick_clocks = (0, (0..=20).map(|i| (i as f32 * 0.005).to_bits()).collect());
        w.tick_wraps = vec![0; 21];
        let u = w.unit_mut(h).unwrap();
        u.x = 16.0;
        u.extra.mover = Some(stopped);
        let checksum = w.keepalive();
        let mut view = Presentation::default();
        view.turn(before, &w, 0.1);
        view.elapsed = 0.025;
        assert!((view.unit(&w, w.unit(h).unwrap()).0.x - 8.0).abs() < 0.001);
        view.elapsed = 0.075;
        assert_eq!(view.unit(&w, w.unit(h).unwrap()).0.x, 16.0);
        assert_eq!(w.unit(h).unwrap().x, 16.0);
        assert_eq!(w.keepalive(), checksum, "render sampling must not change authoritative state");
    }

    #[test]
    fn gold_ages_on_consumed_game_time_and_pauses_do_not_rewind_it() {
        let mut w = world();
        let before = Snapshot::capture(&w);
        w.time = 0.1;
        w.game_ms = 100;
        w.game_speed = 2;
        let mut view = Presentation::default();
        view.turn(before, &w, 0.1);
        view.elapsed = 0.025;
        assert!((view.text_age(0.5, false) - 0.425).abs() < 0.0001);
        assert_eq!(view.text_age(0.5, true), 0.5);
        view.turn(Snapshot::capture(&w), &w, 0.1);
        view.elapsed = 0.0;
        assert_eq!(view.text_age(0.5, false), 0.5);
    }

    #[test]
    fn aimed_projectile_uses_fixed_intercept_and_its_flight_clock() {
        let mut w = world();
        let before = Snapshot::capture(&w);
        w.time = 0.1;
        w.game_ms = 100;
        w.game_speed = 2;
        w.tick_clocks = (0, (0..=20).map(|i| (i as f32 * 0.005).to_bits()).collect());
        w.tick_wraps = vec![0; 21];
        let missile = Missile { id: 1, source: 0, target: 0, model: String::new(),
            x: 100.0, y: 0.0, z: 0.0, speed: 1000.0, arc: 0.0, damage: 0.0,
            attack_type: String::new(), start: (0.0, 0.0), spell: None, weapon: None,
            impact: None, bounces: 0, hit: vec![], stampede: None, rock: None,
            reflected: false, bullet: Some(w3_game::bullets::Bullet {
                due: Some(0.1), aimed: true, seen: Some((100.0, 0.0)), ..Default::default()
            }) };
        let mut view = Presentation::default();
        view.turn(before, &w, 0.1);
        for ms in [0, 25, 50, 75, 100] {
            view.elapsed = ms as f32 / 1000.0;
            let (p, _) = view.missile(&w, &missile, Vec2::new(500.0, 500.0)).unwrap();
            assert!((p.x - ms as f32).abs() < 0.001);
            assert_eq!(p.y, 0.0, "an aimed attack cannot be retargeted by rendering");
        }
    }
}
