//! The original's movement steps (NIpse's, read from game.dll and measured): every 30
//! ms of game time a walking unit's heading turns toward its way, its speed is set,
//! and its velocity changed; its place is kept as where it was when its velocity last
//! changed, carried on by the velocity for the time since.  In cell units (world units
//! over 32, from the map's corner), computed with the engine's own arithmetic.

use crate::efloat as ef;

/// The game clock's step, 0.03 seconds.
pub const STEP: u32 = 0x3cf5_c28f;
/// Angles under this count as none (0x6fcd5470).
const TINY_ANGLE: u32 = 0x3456_bf95;
/// A heading this near its way's counts as on it (0x6fcd53d4).
const ON_HEADING: u32 = 0x3e4c_cccd;
/// Times under this count as none (0x6fcd539c).
const TINY_TIME: u32 = 0x38d1_b717;
const ZERO: u32 = 0;

/// How a unit moves, in cells and radians: its top speed a second, how much its speed
/// may grow a step, how far its heading may turn a step, the angle off its way past
/// which it stops to turn, and how near a waypoint counts as there.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Gait {
    pub top: u32,
    pub grow: u32,
    pub turn: u32,
    pub window: u32,
    pub reach: u32,
}

/// A moving unit's state: its place when last placed and the clock then, its velocity,
/// and its heading when still.
#[derive(Clone, Copy, Debug, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct Mover {
    pub x: u32,
    pub y: u32,
    pub vx: u32,
    pub vy: u32,
    pub heading: u32,
    pub placed_at: u32,
}

fn abs(a: u32) -> u32 {
    a & 0x7fff_ffff
}

fn lt(a: u32, b: u32) -> bool {
    f32::from_bits(a) < f32::from_bits(b)
}

/// The time since a clock reading (`FUN_6f15cea0`, one epoch).
pub fn since(now: u32, then: u32) -> u32 {
    let d = ef::sub(now, then);
    if lt(abs(d), TINY_TIME) {
        0
    } else {
        d
    }
}

/// The angle to turn from one heading to another, -pi to pi (`FUN_6f173720`).
fn turn_to(from: u32, to: u32) -> u32 {
    let d = ef::sub(from, to);
    let neg = d ^ 0x8000_0000;
    if !lt(d, ZERO) {
        if lt(crate::efloat_recip::PI, d) {
            return ef::sub(crate::efloat_recip::TWO_PI, d);
        }
        neg
    } else {
        if lt(crate::efloat_recip::PI, neg) {
            return ef::sub(crate::efloat_recip::TWO_PI ^ 0x8000_0000, d);
        }
        neg
    }
}

/// A heading wrapped into 0 to two pi, as the steering keeps it (`FUN_6f16fe20`).
fn wrap(h: u32) -> u32 {
    let two_pi = crate::efloat_recip::TWO_PI;
    let mut h = h;
    if !lt(abs(h), two_pi) {
        h = ef::sub(h, ef::mul(ef::floor(ef::div(h, two_pi)), two_pi));
    }
    if lt(h, ZERO) {
        ef::add(h, two_pi)
    } else {
        h
    }
}

/// What a step did.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Step {
    Moving,
    /// Within reach of the waypoint, heading on it: stopped.
    Reached,
}

impl Mover {
    /// One step of turning in place toward a point, its search having found no way
    /// (traced: a pig boxed in by its herd and Booty Bay's first sheep, their speeds 0,
    /// turn toward their orders' goals and are done once within 0.2 of them).
    pub fn face(&mut self, gait: &Gait, to: (u32, u32), now: u32) -> Step {
        let (x, y) = self.at(now);
        let (dx, dy) = (ef::sub(to.0, x), ef::sub(to.1, y));
        let dist = ef::sqrt(ef::add(ef::mul(dx, dx), ef::mul(dy, dy)));
        let angle = turn_to(self.heading, ef::heading(dx, dy, dist));
        let angle = if lt(abs(angle), TINY_ANGLE) { ZERO } else { angle };
        if !lt(ON_HEADING, abs(angle)) {
            self.set_velocity(now, ZERO, ZERO);
            return Step::Reached;
        }
        let t = if !lt(abs(angle), gait.turn) { gait.turn } else { abs(angle) };
        self.heading = wrap(ef::add(self.heading, if lt(angle, ZERO) { t | 0x8000_0000 } else { t }));
        self.set_velocity(now, ZERO, ZERO);
        Step::Moving
    }

    /// Its place at a clock reading.
    pub fn at(&self, now: u32) -> (u32, u32) {
        let dt = since(now, self.placed_at);
        (ef::add(self.x, ef::mul(self.vx, dt)), ef::add(self.y, ef::mul(self.vy, dt)))
    }

    /// Sets its velocity at a clock reading (`FUN_6f15f7e0`): its place moved on to
    /// then, the change added.
    fn set_velocity(&mut self, now: u32, vx: u32, vy: u32) {
        let (x, y) = self.at(now);
        self.x = x;
        self.y = y;
        self.placed_at = now;
        self.vx = ef::add(self.vx, ef::sub(vx, self.vx));
        self.vy = ef::add(self.vy, ef::sub(vy, self.vy));
    }

    /// One step toward a waypoint at a clock reading (`FUN_6f16a790`, `FUN_6f16e910`,
    /// `FUN_6f170880`, `FUN_6f16fe20`).
    pub fn step(&mut self, gait: &Gait, to: (u32, u32), now: u32) -> Step {
        let (x, y) = self.at(now);
        let speed = ef::sqrt(ef::add(ef::mul(self.vx, self.vx), ef::mul(self.vy, self.vy)));
        let speed = if lt(abs(speed), TINY_ANGLE) { ZERO } else { speed };
        let heading = if speed == ZERO { self.heading } else { ef::heading(self.vx, self.vy, speed) };
        let (dx, dy) = (ef::sub(to.0, x), ef::sub(to.1, y));
        let dist = ef::sqrt(ef::add(ef::mul(dx, dx), ef::mul(dy, dy)));
        let angle = turn_to(heading, ef::heading(dx, dy, dist));
        let angle = if lt(abs(angle), TINY_ANGLE) { ZERO } else { angle };
        // Within reach it only turns (`FUN_6f16fbd0` steering with the stop flag), and
        // with its heading on the way it is there.
        let within = !lt(gait.reach, dist);
        if within && !lt(ON_HEADING, abs(angle)) {
            self.heading = heading;
            self.set_velocity(now, ZERO, ZERO);
            return Step::Reached;
        }
        let mut speed = if within || !lt(abs(angle), gait.window) { ZERO } else { ef::add(speed, gait.grow) };
        if lt(gait.top, speed) {
            speed = gait.top;
        }
        let mut heading = heading;
        if angle != ZERO {
            let t = if !lt(abs(angle), gait.turn) { gait.turn } else { abs(angle) };
            heading = ef::add(heading, if lt(angle, ZERO) { t | 0x8000_0000 } else { t });
        }
        self.heading = wrap(heading);
        if lt(ZERO, speed) {
            let (s, c) = ef::sin_cos(self.heading);
            self.set_velocity(now, ef::mul(speed, c), ef::mul(speed, s));
        } else {
            self.set_velocity(now, ZERO, ZERO);
        }
        Step::Moving
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(v: f32) -> u32 {
        v.to_bits()
    }

    /// Booty Bay's first wandering sheep ordered south by script, sampled each step 10
    /// ms after it (so a third of a step on), in world units: still while it turns from
    /// 116.2 degrees, then 3 units a step bending to its waypoint (the middle of cell
    /// 430, 45), stopped within reach of it.
    #[test]
    fn the_sheep_walks_as_measured() {
        let (ox, oy) = (-12544.0f32, -6400.0f32);
        let cell = |w: f32, o: f32| f((w - o) / 32.0);
        let gait = Gait { top: f(3.125), grow: f(3.125), turn: f(0.5), window: f(1.047197), reach: f(0.49) };
        let mut clock = f(1.0);
        let mut m = Mover { x: cell(1232.0, ox), y: cell(-4912.0, oy), heading: f(116.2f32.to_radians()), placed_at: clock, ..Default::default() };
        let to = (f(430.5), f(45.5));
        let samples = [
            (1232.00, -4912.00),
            (1232.00, -4912.00),
            (1232.00, -4912.00),
            (1232.00, -4912.00),
            (1231.82, -4912.98),
            (1231.47, -4915.95),
            (1231.52, -4918.95),
            (1231.58, -4921.95),
            (1231.64, -4924.95),
            (1231.69, -4927.95),
            (1231.73, -4929.95),
        ];
        let mut reached = None;
        for (k, &(sx, sy)) in samples.iter().enumerate() {
            clock = ef::add(clock, STEP);
            if reached.is_none() && m.step(&gait, to, clock) == Step::Reached {
                reached = Some(k);
            }
            let (x, y) = m.at(ef::add(clock, f(0.01)));
            let (wx, wy) = (f32::from_bits(x) * 32.0 + ox, f32::from_bits(y) * 32.0 + oy);
            assert!((wx - sx).abs() < 0.02 && (wy - sy).abs() < 0.02, "step {k}: {wx:.2}, {wy:.2} against {sx}, {sy}");
        }
        assert_eq!(reached, Some(10));
    }
}
