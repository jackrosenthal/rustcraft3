//! The original's movement steps (NIpse's, read from game.dll and measured): every 30
//! ms of game time a walking unit's heading turns toward its way, its speed is set,
//! and its velocity changed; its place is kept as where it was when its velocity last
//! changed, carried on by the velocity for the time since.  In cell units (world units
//! over 32, from the map's corner), computed with the engine's own arithmetic.

use crate::efloat as ef;

/// The game clock's tick, 0.005 seconds: its timer queue's clock moves from event to
/// event, and a timer every tick sets it to 0.005 more by the engine's addition each
/// time (watched: the first write 0.005; `FUN_6f053710` reschedules a periodic timer at
/// its time plus its period, `FUN_6f052380` sets the clock to each event's time).
pub const TICK: u32 = 0x3ba3_d70a;

/// The movement steps' period on the agent queue, 30 milliseconds in seconds by the
/// engine's arithmetic (0x3cf5c290, a bit above the single nearest 0.03; read: its
/// clock at all of 2600 steps, 0.03 to 77.997368, the sums of it by the engine's
/// addition, where 0.03's sums part from step 24).  The main clock at the steps runs by
/// the 0.005s ticks instead (read: 5.069871, 8.849516, 11.069143, 32.395412, and
/// 46.017925, each bit for bit the 0.005s summed).
pub const STEP: u32 = 0x3cf5_c290;

/// The span the clocks are set back by as the main clock reaches it (read: the main
/// clock's 0.005s sums at 299.985016 and 300.004913, the latter 0.004913 then; the
/// agent queue's step due 300.018311 at 0.018311, its events due then set back as
/// much, later ones counted from those).
pub const WRAP: u32 = 0x4396_0000;

/// The clock six ticks on.
pub fn next_step(clock: u32) -> u32 {
    (0..6).fold(clock, |c, _| ef::add(c, TICK))
}
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

    /// Whether its heading is on a point already (within 0.2, as `face` judges).
    pub fn faces(&self, to: (u32, u32), now: u32) -> bool {
        let (x, y) = self.at(now);
        let (dx, dy) = (ef::sub(to.0, x), ef::sub(to.1, y));
        let dist = ef::sqrt(ef::add(ef::mul(dx, dx), ef::mul(dy, dy)));
        let angle = turn_to(self.heading, ef::heading(dx, dy, dist));
        !lt(ON_HEADING, abs(angle))
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

    /// `set_velocity` held to a top speed as the engine does it (`FUN_6f1606e0`): one
    /// whose length passes the top is scaled to it by the reciprocal of its length
    /// (`FUN_6f15fc70`, `FUN_6f071570`), which falls a little short (read: peons whose
    /// tuned speeds were 5.907512 and 6.486394 walking at 5.907436 and 6.486344, those at
    /// 5.9375, 7.125, and 6.025506 at theirs), and one too short to measure stops.
    fn set_velocity_within(&mut self, now: u32, vx: u32, vy: u32, top: u32) {
        self.set_velocity(now, vx, vy);
        let len2 = || ef::add(ef::mul(self.vx, self.vx), ef::mul(self.vy, self.vy));
        let l2 = len2();
        if lt(abs(l2), TINY_ANGLE) {
            self.vx = ZERO;
            self.vy = ZERO;
            return;
        }
        if lt(top, ef::sqrt(l2)) {
            let k = ef::mul(top, ef::recip(ef::sqrt(len2())));
            self.vx = ef::mul(self.vx, k);
            self.vy = ef::mul(self.vy, k);
        }
    }

    /// One step toward a waypoint at a clock reading (`FUN_6f16a790`, `FUN_6f16e910`,
    /// `FUN_6f170880`, `FUN_6f16fe20`).
    pub fn step(&mut self, gait: &Gait, to: (u32, u32), now: u32) -> Step {
        self.steer(gait, to, now, false, None)
    }

    /// `step`, its arrival judged against a bearing to another point: an order's goal
    /// beyond the end of a way short of it (`FUN_6f16e910` reads the member's goal).
    pub fn step_facing(&mut self, gait: &Gait, to: (u32, u32), goal: (u32, u32), now: u32) -> Step {
        self.steer(gait, to, now, false, Some(goal))
    }

    /// One step toward the last waypoint of a walk to a structure (`goal` its middle):
    /// once within reach it stops and turns toward the goal, at once when the turn is
    /// within its rate, and is there when on that bearing (read: a peon 0.25 and 0.18 off
    /// the bearings of a gold mine and a great hall turned onto them in the step it came
    /// within reach, stopped, and arrived in the next).
    pub fn arrive(&mut self, gait: &Gait, to: (u32, u32), goal: (u32, u32), now: u32) -> Step {
        let (x, y) = self.at(now);
        let (dx, dy) = (ef::sub(to.0, x), ef::sub(to.1, y));
        let dist = ef::sqrt(ef::add(ef::mul(dx, dx), ef::mul(dy, dy)));
        if lt(gait.reach, dist) {
            return self.steer(gait, to, now, false, None);
        }
        let angle = self.goal_angle(goal, now);
        if angle == ZERO {
            self.set_velocity(now, ZERO, ZERO);
            return Step::Reached;
        }
        // A turn within its rate puts its heading on the bearing itself (read: a peon's
        // headings at a gold mine and a hall each its bearing to them, 5.990847 and
        // 3.327624, also where the turn crossed 0).
        if lt(abs(angle), gait.turn) {
            self.heading = self.goal_bearing(goal, now);
        } else {
            let speed = ef::sqrt(ef::add(ef::mul(self.vx, self.vx), ef::mul(self.vy, self.vy)));
            let speed = if lt(abs(speed), TINY_ANGLE) { ZERO } else { speed };
            let heading = if speed == ZERO { self.heading } else { ef::heading(self.vx, self.vy, speed) };
            self.heading = wrap(ef::add(heading, if lt(angle, ZERO) { gait.turn | 0x8000_0000 } else { gait.turn }));
        }
        self.set_velocity(now, ZERO, ZERO);
        Step::Moving
    }

    /// Stops where it is and turns toward a point, onto its bearing when the turn is
    /// within its rate (as `arrive` does; read: a peon coming within range of a tree
    /// stopped there in that step, its heading the tree's bearing, 0.424 from its
    /// course, its turn 0.6).
    pub fn halt_facing(&mut self, gait: &Gait, goal: (u32, u32), now: u32) {
        let angle = self.goal_angle(goal, now);
        if angle != ZERO {
            if lt(abs(angle), gait.turn) {
                self.heading = self.goal_bearing(goal, now);
            } else {
                let speed = ef::sqrt(ef::add(ef::mul(self.vx, self.vx), ef::mul(self.vy, self.vy)));
                let speed = if lt(abs(speed), TINY_ANGLE) { ZERO } else { speed };
                let heading = if speed == ZERO { self.heading } else { ef::heading(self.vx, self.vy, speed) };
                self.heading = wrap(ef::add(heading, if lt(angle, ZERO) { gait.turn | 0x8000_0000 } else { gait.turn }));
            }
        }
        self.set_velocity(now, ZERO, ZERO);
    }

    /// Whether it faces a point: standing within a step's turn of its bearing, or,
    /// moving, its course within 0.2 of it (read: a peon walking 0.102 off its tree's
    /// bearing arrived in the step it came within range, one 0.424 off stopped and
    /// turned first).
    pub fn stopped_facing(&self, gait: &Gait, goal: (u32, u32), now: u32) -> bool {
        let stopped = self.vx & 0x7fff_ffff == 0 && self.vy & 0x7fff_ffff == 0;
        let angle = self.goal_angle(goal, now);
        if !stopped {
            return !lt(ON_HEADING, abs(angle));
        }
        angle == ZERO || lt(abs(angle), gait.turn)
    }

    /// Whether it is within `range` cells of a point now (`FUN_6f05b580`: the squares
    /// compared, a thousandth over still within).
    pub fn within(&self, point: (u32, u32), range: u32, now: u32) -> bool {
        let (x, y) = self.at(now);
        let (dx, dy) = (ef::sub(point.0, x), ef::sub(point.1, y));
        let d2 = ef::add(ef::mul(dx, dx), ef::mul(dy, dy));
        let r2 = ef::mul(range, range);
        !lt(r2, d2) || lt(ef::sub(d2, r2), 0.001f32.to_bits())
    }

    /// Whether its walk is over there now: within reach on the goal's bearing, or stopped
    /// there (`arrive`) within a step's turn of it (read: a peon stopped at a gold mine
    /// 0.069 short of its bearing, its turn 0.6, went in at the next step with its
    /// heading as it was; one walking home straight at its hall arrived as it came
    /// within reach).
    pub fn arrived(&self, gait: &Gait, to: (u32, u32), goal: (u32, u32), now: u32) -> bool {
        let (x, y) = self.at(now);
        let (dx, dy) = (ef::sub(to.0, x), ef::sub(to.1, y));
        let dist = ef::sqrt(ef::add(ef::mul(dx, dx), ef::mul(dy, dy)));
        let stopped = self.vx & 0x7fff_ffff == 0 && self.vy & 0x7fff_ffff == 0;
        let angle = self.goal_angle(goal, now);
        !lt(gait.reach, dist) && (angle == ZERO || (stopped && lt(abs(angle), gait.turn)))
    }

    /// A point's bearing from its place now.
    fn goal_bearing(&self, goal: (u32, u32), now: u32) -> u32 {
        let (x, y) = self.at(now);
        let (gx, gy) = (ef::sub(goal.0, x), ef::sub(goal.1, y));
        let gd = ef::sqrt(ef::add(ef::mul(gx, gx), ef::mul(gy, gy)));
        ef::heading(gx, gy, gd)
    }

    /// The turn from its heading (or its velocity's, when moving) to a point's bearing,
    /// 0 when tiny.
    fn goal_angle(&self, goal: (u32, u32), now: u32) -> u32 {
        let (x, y) = self.at(now);
        let speed = ef::sqrt(ef::add(ef::mul(self.vx, self.vx), ef::mul(self.vy, self.vy)));
        let speed = if lt(abs(speed), TINY_ANGLE) { ZERO } else { speed };
        let heading = if speed == ZERO { self.heading } else { ef::heading(self.vx, self.vy, speed) };
        let (gx, gy) = (ef::sub(goal.0, x), ef::sub(goal.1, y));
        let gd = ef::sqrt(ef::add(ef::mul(gx, gx), ef::mul(gy, gy)));
        if gd == ZERO {
            return ZERO;
        }
        let angle = turn_to(heading, ef::heading(gx, gy, gd));
        if lt(abs(angle), TINY_ANGLE) {
            ZERO
        } else {
            angle
        }
    }

    /// Its speed squared, as the engine compares two units' (`FUN_6f168360`).
    pub fn speed2(&self) -> u32 {
        ef::add(ef::mul(self.vx, self.vx), ef::mul(self.vy, self.vy))
    }

    /// One step of a unit held back (`FUN_6f16fbd0` with its stop flag): it turns toward
    /// its waypoint and does not move.
    pub fn hold(&mut self, gait: &Gait, to: (u32, u32), now: u32) {
        self.steer(gait, to, now, true, None);
    }

    fn steer(&mut self, gait: &Gait, to: (u32, u32), now: u32, stop: bool, goal: Option<(u32, u32)>) -> Step {
        let (x, y) = self.at(now);
        let speed = ef::sqrt(ef::add(ef::mul(self.vx, self.vx), ef::mul(self.vy, self.vy)));
        let speed = if lt(abs(speed), TINY_ANGLE) { ZERO } else { speed };
        let heading = if speed == ZERO { self.heading } else { ef::heading(self.vx, self.vy, speed) };
        let (dx, dy) = (ef::sub(to.0, x), ef::sub(to.1, y));
        let dist = ef::sqrt(ef::add(ef::mul(dx, dx), ef::mul(dy, dy)));
        let mut angle = turn_to(heading, ef::heading(dx, dy, dist));
        // Within reach it only turns (`FUN_6f16fbd0` steering with the stop flag), and
        // with its heading on the way it is there.
        let within = !lt(gait.reach, dist);
        if let (true, Some(g)) = (within, goal) {
            let (gx, gy) = (ef::sub(g.0, x), ef::sub(g.1, y));
            let gd = ef::sqrt(ef::add(ef::mul(gx, gx), ef::mul(gy, gy)));
            if gd != ZERO {
                angle = turn_to(heading, ef::heading(gx, gy, gd));
            }
        }
        let angle = if lt(abs(angle), TINY_ANGLE) { ZERO } else { angle };
        if within && !stop && !lt(ON_HEADING, abs(angle)) {
            self.heading = heading;
            self.set_velocity(now, ZERO, ZERO);
            return Step::Reached;
        }
        let mut speed = if stop || within || !lt(abs(angle), gait.window) { ZERO } else { ef::add(speed, gait.grow) };
        if lt(gait.top, speed) {
            speed = gait.top;
        }
        let mut heading = heading;
        if angle != ZERO {
            let t = if !lt(abs(angle), gait.turn) { gait.turn } else { abs(angle) };
            heading = ef::add(heading, if lt(angle, ZERO) { t | 0x8000_0000 } else { t });
        }
        self.heading = wrap(heading);
        // The velocity is taken from the heading before it is wrapped (read: sheep
        // 1050117 at 8.85 turned to -0.174075, its velocity (3.077775, -0.541219) that
        // angle's, not 6.1091104's, whose table step truncates the other way).
        if lt(ZERO, speed) {
            let (s, c) = ef::sin_cos(heading);
            self.set_velocity_within(now, ef::mul(speed, c), ef::mul(speed, s), gait.top);
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

    /// The agent queue's clock at movement steps as read under winedbg: the sums of the
    /// 30-millisecond period by the engine's addition.
    #[test]
    fn the_steps_clocks_are_the_originals() {
        let mut s = 0;
        for k in 1..=2600 {
            s = ef::add(s, STEP);
            let want = match k {
                24 => "0.720000",
                180 => "5.399984",
                1151 => "34.529137",
                1540 => "46.198662",
                2600 => "77.997368",
                _ => continue,
            };
            assert_eq!(format!("{:.6}", f32::from_bits(s)), want, "step {k}");
        }
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
            clock = next_step(clock);
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
