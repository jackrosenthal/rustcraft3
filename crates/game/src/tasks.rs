//! A unit's tasks as the original's objects: the task stack (CUnit +0x174, each task's
//! +0x24 the one below; a task pushed by `FUN_6f691e60`, run top first by `FUN_6f67df00`
//! until one waits, clearing the unit's flag 1, and popped and destroyed as it is done,
//! `FUN_6f691260`), for what the port runs otherwise: a structure's attack, and a
//! pause's order.
//!
//! A structure's attack on a target it takes up of its own accord: CAbilityAttack's
//! take-up (`FUN_6f4943f0`, from its look `FUN_6f493ff0`) pushes its tasks by the unit's
//! state (+0x194, a block's `tskA` setting it as it runs, event 0xd0162
//! `FUN_6f697ed0`): for one in state 0 a block back to 0 (`FUN_6f6927a0`), task 0xd014b
//! (`FUN_6f691f20`), the walk back to where it stood (`tsk.` 0xd016c, `FUN_6f692120`),
//! a block to 7, tasks 0xd0148 and 0xd016a, the walk into range (`tskt` 0xd016f,
//! `FUN_6f6926b0`), task 0xd0163, the attack (`tskt` 0xd0168), and a block to 1, which
//! runs and goes at once; the attack then waits (`FUN_6f49e510`).  Traced: Mafarazzo
//! TD's Plague Tower taking up a fort runner, its ten objects in that order and the
//! last freed with them.

use crate::codes;
use crate::world::World;

/// A block (`tskA`): sets the unit's state as it runs.
const BLOCK: u32 = 0xd0162;
/// Task 0xd014b (`FUN_6f498ff0`).
const TASK_14B: u32 = 0xd014b;
/// The walk back to where it stood (CAbilityMove's).
const MOVE_HOME: u32 = 0xd016c;
/// Task 0xd0148 (`FUN_6f49ed20`): the look again in 0.01 (`FUN_6f49c3a0`, `FUN_6f49d000`).
const LOOK_AGAIN: u32 = 0xd0148;
/// Task 0xd016a (`FUN_6f49e6b0`): waits for a backswing under way.
const BACKSWING: u32 = 0xd016a;
/// The walk into range (CAbilityMove's).
const MOVE_TO: u32 = 0xd016f;
/// Task 0xd0163 (`FUN_6f49ea80`): a wait of ReactionDelay times a draw on stream 9,
/// after an attack done (event 0xd01af, `FUN_6f49b3d0`).
const REACT: u32 = 0xd0163;
/// The attack (`FUN_6f49e510`).
const ATTACK: u32 = 0xd0168;

/// A structure's first attack on a target it takes up begins this long after (the
/// attack task's `FUN_6f49d130` with DAT_6fcd53c4, for a unit that cannot move; one that
/// can tries at once, event 0xd0196).  Read: the Plague Tower's look at 0.216, its
/// damage points 0.32 apart from 0.616.
pub(crate) const FIRST_ATTACK_WAIT: f64 = 0.1;

/// The look for a target again after an attack is given up (`FUN_6f49d000`: its timer,
/// event 0xd01c1, restarted at DAT_6fcd53a4).
pub(crate) const LOOK_AFTER_DROP: f32 = 0.01;

/// A structure that can move (its move ability, CUnit +0x1ec, `FUN_6f66f030`) and has
/// taken up what lies out of its reach: its attack tried at once and done, it waits its
/// reaction (task 0xd0163), then walks into range (task 0xd016f) from inside its
/// footprint, held there until it gives up (`World::reach_walk_step`).
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum ReachWalk {
    /// Its reaction wait, due then on the agent queue (seconds since the game began).
    React { due: f64 },
    /// Its walk, held: the steps left of its blocked count (0 before its first step,
    /// which draws it), and its path and cluster.
    Walk { steps: u32, path: [u32; 2] },
}

impl ReachWalk {
    /// Whether it walks (with the walkers, in their order).
    pub fn walking(&self) -> bool {
        matches!(self, ReachWalk::Walk { .. })
    }
}

impl World {
    /// Whether a unit's attacks are a structure's: it cannot walk into range
    /// (`FUN_6f66f030`, no move ability or its move disabled), so it takes up only
    /// what it reaches (`FUN_6f4959f0`) and gives up what leaves its reach
    /// (`FUN_6f495180`).
    pub(crate) fn attacks_in_place(&self, h: u32) -> bool {
        self.unit(h).is_some_and(|u| u.is_building && !u.extra.uprooted)
    }

    /// Whether such a structure can move all the same (`FUN_6f66f030` →
    /// `FUN_6f5fb040`): it has a move ability (`Amov`, made for a type with a speed), as
    /// Mafarazzo TD's Towers of Vengence, made from the Ancient Protector without its
    /// Root, have.  Its look then takes up what is within its acquisition range out of
    /// its weapons' reach (`FUN_6f498e50` from 0x6f498ec3; the reach test `FUN_6f4959f0`
    /// from 0x6f495ab4).  (Provisional: the ability's disabling, its +0x7c and its +0x20
    /// flag 0x200, not yet read.)
    pub(crate) fn moves_to_attack(&self, h: u32) -> bool {
        self.attacks_in_place(h) && self.unit(h).is_some_and(|u| u.extra.object_slots.iter().flatten().any(|&(s, b)| self.tags.live.get(&s).is_some_and(|e| &e.0 == codes::ability::id::MOVE && e.1 == b)))
    }

    /// A structure takes up a target on its own at `at` (the main clock): its tasks
    /// made, or, with a target already, the attack's (the old attack, its reaction
    /// wait, and its walk popped, `FUN_6f497190`; a walk into range and an attack
    /// pushed); its first attack on it begins `FIRST_ATTACK_WAIT` on.
    pub(crate) fn attack_taken_up(&mut self, h: u32, at: f64) {
        let Some(u) = self.unit(h) else { return };
        let mut tasks = u.extra.attack_tasks.clone();
        if tasks.is_empty() {
            let made: [(&crate::tags::Code, u32); 10] = [(codes::class::TASK_A, BLOCK), (codes::class::TASK, TASK_14B), (codes::class::TASK_POINT, MOVE_HOME), (codes::class::TASK_A, BLOCK), (codes::class::TASK, LOOK_AGAIN), (codes::class::TASK, BACKSWING), (codes::class::TASK_T, MOVE_TO), (codes::class::TASK, REACT), (codes::class::TASK_T, ATTACK), (codes::class::TASK_A, BLOCK)];
            tasks = made.iter().map(|&(c, e)| (self.tags.take(c), e)).collect();
            // (The last block runs and goes as the main tick ends, `World::free_as_tick_ends`;
            // traced: the upgrade game's take-up at turn 503, its slot the next step's
            // region trigger's `+tex`.  One that can move tries its attack at once,
            // `World::reach_tried`.)
            let (top, _) = tasks.pop().unwrap();
            // (What a backswing held as it gave its last target up goes after, top
            // first: traced, the towers-vs-waves game's Cutter at turn 551, its six
            // freed after its new tasks' last block as that main tick ended.)
            let held = self.unit_mut(h).map(|u| std::mem::take(&mut u.extra.held_tasks)).unwrap_or_default();
            if self.moves_to_attack(h) {
                if let Some(u) = self.unit_mut(h) {
                    u.extra.attack_tasks = tasks;
                }
                self.reach_tried(h, at, top);
                for &(s, _) in held.iter().rev() {
                    self.free_as_tick_ends(s, at);
                }
                return;
            }
            self.free_as_tick_ends(top, at);
            for &(s, _) in held.iter().rev() {
                self.free_as_tick_ends(s, at);
            }
        } else {
            self.pop_attack(&mut tasks);
            tasks.push((self.tags.take(codes::class::TASK_T), MOVE_TO));
            tasks.push((self.tags.take(codes::class::TASK_T), ATTACK));
        }
        if let Some(u) = self.unit_mut(h) {
            u.extra.attack_tasks = tasks;
            u.extra.first_swing_at = Some(at + FIRST_ATTACK_WAIT);
        }
    }

    /// A structure that can move takes up a target at `at`, its tasks' last block `top`
    /// run: its attack task tells its move ability to stop (`FUN_6f49e510`: event 0xd0196
    /// through the unit, `FUN_6f0561f0`, where one that cannot move waits 0.1 for its
    /// try, `FUN_6f49d130`) and, leaving the unit's flag 1 set, is done at once, near or
    /// far; it and the block go as the main tick ends (`World::free_as_tick_ends`), and
    /// its reaction wait runs (task 0xd0163, `FUN_6f49ea80`: ReactionDelay times a draw
    /// on stream 9, `FUN_6f6936a0`, from the agent queue's clock, event 0xd01af).  Its
    /// try (`FUN_6f499310` → `FUN_6f495180`) comes only from its move ability's 0xd0196,
    /// as a walk arrives.  Measured: the upgrade game's Mega Tower of Vengence taking up
    /// a creep 1875 off at its look due 47.909035, its block and attack freed at once,
    /// its reaction wait from 0x6f49a6b8, its end due 48.028042; at 49.778515 and
    /// 50.108501, the creep 1559 and 1485 off center to center against its 1500 range,
    /// its attack freed at once again, with no stop at `FUN_6f495180` or
    /// `FUN_6f49a240` (49.7 to 50.13 seconds).
    fn reach_tried(&mut self, h: u32, at: f64, top: u32) {
        use crate::efloat as ef;
        let t = self.unit(h).map_or(0, |u| u.extra.acquired);
        self.free_as_tick_ends(top, at);
        let attack = self.unit_mut(h).and_then(|u| if u.extra.attack_tasks.last().is_some_and(|t| t.1 == ATTACK) { u.extra.attack_tasks.pop() } else { None });
        if let Some((s, _)) = attack {
            self.free_as_tick_ends(s, at);
        }
        let draw = self.rand_data.streams[crate::rng::stream::REACTION].real(0.0, 1.0);
        let wait = ef::mul(draw.to_bits(), self.misc_f32("ReactionDelay", 0.25).to_bits());
        let due = f32::from_bits(ef::add((at as f32).to_bits(), wait)) as f64;
        log::trace!("{h} reacts to {t} from {at:.6}: draw {draw:.6}, due {due:.6}");
        if let Some(u) = self.unit_mut(h) {
            u.extra.first_swing_at = None;
            u.extra.reach_walk = Some(ReachWalk::React { due });
        }
    }

    /// An object destroyed by an event on the agent queue due at `at`, freed as that main
    /// tick ends (traced: the upgrade game's tower's reaction task, its wait ending at
    /// 48.028042, freed after the region trigger's objects of the step after in the same
    /// main tick; ending at 48.864094, a main tick before the next step's, freed before
    /// that step's trigger took its slot).  After the steps, with the turn's releases
    /// (traced: the flyer game's Arrow Tower's take-up after its first creep's death,
    /// as the turn ends, its block freed in that turn).
    fn free_as_tick_ends(&mut self, s: u32, at: f64) {
        let kept = ((at - self.clock_wraps as f64 * 300.0) as f32).to_bits();
        match self.agent_tick(kept).and_then(|t| self.tick_main(t)).filter(|_| !self.after_steps) {
            Some(c) => self.tags.doom_at(c, s),
            None => self.tags.doom(s),
        }
    }

    /// A structure's reaction wait done at `at` (event 0xd01af): the task popped, freed
    /// as the main tick ends, then its walk into range begins (`FUN_6f5fc640`): its
    /// cluster's goal, cluster, and path made, the goal let go at once, as a walk's; from
    /// inside its footprint it is blocked from the start, its count drawn then
    /// (`FUN_6f167290` → `FUN_6f1689d0`: 7 or 8, `FUN_6f1d62b0(2)` on the main
    /// generator, its goal far).  Read: the upgrade game's tower at 48.026821 (step
    /// 0xa41), the only main draw through 48.25; traced, its three objects in turn 481.
    pub(crate) fn reaction_ended(&mut self, h: u32, at: f64) {
        let react = self.unit_mut(h).and_then(|u| if u.extra.attack_tasks.last().is_some_and(|t| t.1 == REACT) { u.extra.attack_tasks.pop() } else { None });
        if let Some((s, _)) = react {
            self.free_as_tick_ends(s, at);
        }
        let goal = self.tags.take(codes::class::CLUSTER_GOAL);
        let cluster = self.tags.take(codes::class::CLUSTER);
        let path = self.tags.take(codes::class::PATH);
        self.tags.release(goal);
        log::trace!("{h} walks into range at {at:.6}");
        let serial = self.tags.born;
        if let Some(u) = self.unit_mut(h) {
            u.extra.walk_serial = serial;
            u.extra.reach_walk = Some(ReachWalk::Walk { steps: 0, path: [path, cluster] });
        }
    }

    /// A structure's walk into range in movement step `k`, in its turn among the walkers:
    /// held, it counts its blocked count down, and as it runs out the walk ends
    /// (CAbilityMove's event 0xd0198, `FUN_6f603110`): its task popped, a block and two
    /// tasks 0xd0144 pushed, all freed top first as the step ends, its path and cluster
    /// at the next step (`Tags::walk_ended`), and its look comes again `LOOK_AFTER_DROP`
    /// after the step's agent clock (`FUN_6f49d000`).  Measured: the upgrade game's
    /// tower's walks begun at 48.028042 and 48.427876, each ending in the eighth step
    /// after, 48.238579 and 48.658562, its looks at 48.248577 and 48.668560 taking the
    /// creep up again; traced, its walk's ten objects freed in the step's order, its path
    /// and cluster after the next look's.  False for any other walker.
    pub(crate) fn reach_walk_step(&mut self, h: u32, k: usize) -> bool {
        use crate::efloat as ef;
        let Some(ReachWalk::Walk { mut steps, path }) = self.unit(h).and_then(|u| u.extra.reach_walk.clone()) else { return false };
        // Its first step searches and finds it blocked: its count, 7 or 8, is drawn
        // then on the main generator (`FUN_6f167290` → `FUN_6f1689d0`,
        // `FUN_6f1d62b0(2)`), and it holds the count's steps, then the one it searches
        // again in, finding nothing from inside its footprint (measured: a count of 7,
        // its end in the eighth step).  (Keepalives: the upgrade game's tower's walk
        // begun at 51.178535, after that main tick's step, its draw not in that turn's
        // keepalive, as the step that draws it was in the next turn.)
        if steps == 0 {
            let r = f32::from_bits((self.rng.next_u32() & 0x7f_ffff) | 0x3f80_0000) - 1.0;
            let count = 7 + (r * 2.0) as u32;
            log::trace!("{h} is blocked in step {k}: {count}");
            steps = count + 1;
        }
        if steps > 1 {
            if let Some(u) = self.unit_mut(h) {
                u.extra.reach_walk = Some(ReachWalk::Walk { steps: steps - 1, path });
            }
            return true;
        }
        let Some(u) = self.unit_mut(h) else { return true };
        u.extra.reach_walk = None;
        let mut tasks = std::mem::take(&mut u.extra.attack_tasks);
        let serial = u.extra.walk_serial;
        if tasks.last().is_some_and(|t| t.1 == MOVE_TO) {
            let (s, _) = tasks.pop().unwrap();
            self.tags.release_in_step(s);
        }
        let pushed: Vec<u32> = [codes::class::TASK_A, codes::class::TASK, codes::class::TASK].iter().map(|c| self.tags.take(c)).collect();
        for &s in pushed.iter().rev() {
            self.tags.release_in_step(s);
        }
        for &(s, _) in tasks.iter().rev() {
            self.tags.release_in_step(s);
        }
        self.tags.walk_ended(path, serial);
        let at = self.step_agent.get(k).copied().unwrap_or(0);
        let due = f32::from_bits(ef::add(at, LOOK_AFTER_DROP.to_bits()));
        let n = self.tags.start_timer("49d000");
        let (now, dt) = (self.time_exact, self.tick_dt);
        log::trace!("{h} gives up its walk at step {k} ({:.6}), looking at {due:.6}", f32::from_bits(at));
        if let Some(u) = self.unit_mut(h) {
            u.action = crate::types::Action::Idle;
            u.extra.first_swing_at = None;
            u.extra.acquired = 0;
            // (As the units' logic, still to come this tick, will count it down.)
            u.acquire_timer = (due as f64 - now) as f32 + dt;
            u.extra.look_timer = Some((n, due, LOOK_AFTER_DROP));
        }
        true
    }

    /// The attack's own tasks popped (`FUN_6f497190`): the attack, its reaction wait
    /// (not run: no draw), and its walk into range, each as it is on top.
    fn pop_attack(&mut self, tasks: &mut Vec<(u32, u32)>) {
        for e in [ATTACK, REACT, MOVE_TO] {
            if tasks.last().is_some_and(|t| t.1 == e) {
                let (s, _) = tasks.pop().unwrap();
                self.tags.doom(s);
            }
        }
    }

    /// A structure gives its target up at `at` (`FUN_6f49d280`, from the attack's try
    /// as its cooldown runs out, `FUN_6f495180`, with the target out of reach): the
    /// attack's tasks popped, then the rest run and freed, top first (none waits: the
    /// backswing is over by the cooldown's end, and a structure's walks end at once),
    /// and the look for a target again `LOOK_AFTER_DROP` on.  Traced: the Plague Tower's
    /// nine tasks freed in the turn of its last attack, its runner fled.
    pub(crate) fn attack_given_up(&mut self, h: u32, at: f64) {
        self.attack_given_up_held(h, at, false);
    }

    /// As `attack_given_up`; in a backswing (`hold`), the rest wait, from task 0xd016a
    /// (`FUN_6f49e6b0`) down, kept for its next take-up (`World::attack_taken_up`).
    fn attack_given_up_held(&mut self, h: u32, at: f64, hold: bool) {
        // (A walk into range under way: its path and cluster go with the turn's
        // releases; provisional.)
        if let Some(ReachWalk::Walk { path, .. }) = self.unit_mut(h).and_then(|u| u.extra.reach_walk.take()) {
            for s in path {
                self.tags.doom(s);
            }
        }
        let Some(u) = self.unit(h) else { return };
        let mut tasks = u.extra.attack_tasks.clone();
        self.pop_attack(&mut tasks);
        if hold && tasks.last().is_some_and(|t| t.1 == BACKSWING) {
            if let Some(u) = self.unit_mut(h) {
                u.extra.held_tasks = std::mem::take(&mut tasks);
            }
        }
        for &(s, _) in tasks.iter().rev() {
            self.tags.doom(s);
        }
        let n = self.tags.start_timer("49d000");
        let due = (at as f32 + LOOK_AFTER_DROP).max(self.time);
        let now = self.time_exact;
        if let Some(u) = self.unit_mut(h) {
            u.extra.attack_tasks.clear();
            u.extra.first_swing_at = None;
            u.extra.acquired = 0;
            u.acquire_timer = (due as f64 - now) as f32;
            u.extra.look_timer = Some((n, due, LOOK_AFTER_DROP));
        }
    }

    /// A structure gives its target up at its try among the steps, at `at` on the agent
    /// queue (`FUN_6f49d280`, `World::first_try`): as `attack_given_up`, its tasks freed
    /// as the main tick ends (`World::free_as_tick_ends`), and its look again
    /// `LOOK_AFTER_DROP` on, among the steps too.
    pub(crate) fn try_given_up(&mut self, h: u32, at: f64) {
        use crate::efloat as ef;
        let Some(u) = self.unit_mut(h) else { return };
        let mut tasks = std::mem::take(&mut u.extra.attack_tasks);
        for e in [ATTACK, REACT, MOVE_TO] {
            if tasks.last().is_some_and(|t| t.1 == e) {
                let (s, _) = tasks.pop().unwrap();
                self.free_as_tick_ends(s, at);
            }
        }
        for &(s, _) in tasks.iter().rev() {
            self.free_as_tick_ends(s, at);
        }
        let n = self.tags.start_timer("49d000");
        let due = f32::from_bits(ef::add((at as f32).to_bits(), LOOK_AFTER_DROP.to_bits()));
        let (now, dt) = (self.time_exact, self.tick_dt);
        if let Some(u) = self.unit_mut(h) {
            u.action = crate::types::Action::Idle;
            u.extra.first_swing_at = None;
            u.extra.acquired = 0;
            // (As the units' logic, still to come this tick, will count it down.)
            u.acquire_timer = (due as f64 - now) as f32 + dt;
            u.extra.look_timer = Some((n, due, LOOK_AFTER_DROP));
        }
    }

    /// Whether a structure reaches a target walking by the engine's steps at game time
    /// `at` (`FUN_6f05b580` with its places carried on to the main clock then,
    /// `FUN_6f161040`: the range over 32 and both collisions over 32, squared, against
    /// the cells between, a thousandth over still within), or none for a target not so
    /// walking.  Read: the Plague Tower's fourteenth try, its runner 33.54 cells off at
    /// its cooldown's end, past its 31.25 and 2.25, where the runner's last step had it
    /// 33.42 off.
    pub(crate) fn reaches_walker(&self, h: u32, target: u32, range: f32, at: f64) -> Option<bool> {
        use crate::efloat as ef;
        let (u, t) = (self.unit(h)?, self.unit(target)?);
        let m = t.extra.mover.filter(|_| t.steps_walker())?;
        let origin = self.pathing.as_ref()?.origin;
        let clock = self.main_tick_clock(at)?;
        let cell = |w: f32, o: f32| ef::div(ef::sub(w.to_bits(), o.to_bits()), crate::pathing::CELL.to_bits());
        let radius = |c: f32| ef::div(c.to_bits(), crate::pathing::CELL.to_bits());
        let r = ef::add(ef::add(ef::div(range.to_bits(), crate::pathing::CELL.to_bits()), radius(t.collision)), radius(u.collision));
        Some(m.within((cell(u.x, origin[0]), cell(u.y, origin[1])), r, clock))
    }

    /// The structures attacking a unit as its death begins give it up (`FUN_6f49d280`),
    /// each looking again `LOOK_AFTER_DROP` after `at` (`World::death_begun`), in the
    /// order they took it up (traced: the upgrade game's two towers on a creep the
    /// h00X's shell killed at turn 512, the one that took it up at turn 503 first, then
    /// the one at 507); not one walking into range, its attack done (a Tower of
    /// Vengence's, `ReachWalk`).
    pub(crate) fn attackers_give_up(&mut self, h: u32, at: f64) {
        let mut attackers: Vec<u32> = self.unit_order.iter().copied().filter(|&a| self.attacks_in_place(a) && self.unit(a).is_some_and(|u| u.extra.acquired == h && !u.extra.attack_tasks.is_empty() && u.extra.reach_walk.is_none())).collect();
        let taken = |w: &World, a: u32| w.unit(a).and_then(|u| u.extra.attack_tasks.first()).and_then(|t| w.tags.live.get(&t.0)).map_or(u32::MAX, |e| e.1);
        attackers.sort_by_key(|&a| taken(self, a));
        let after = self.after_steps;
        for a in attackers {
            // (In its backswing, past its damage point, the rest of its tasks wait:
            // traced, the towers-vs-waves game's Cutter at turn 551, three freed at its
            // creep's death, six at its next take-up; the Arrow Tower's all nine.)
            let backswing = self.unit(a).and_then(|u| u.swing.as_ref()).is_some_and(|s| s.done || s.landed);
            self.attack_given_up_held(a, at, backswing);
            let due = (at + LOOK_AFTER_DROP as f64) as f32;
            let now = self.time_exact;
            if let Some(u) = self.unit_mut(a) {
                u.action = crate::types::Action::Idle;
                // (A damage point still to come is called off, and the cooldown with it,
                // `FUN_6f497e20`: its timers at +0x1ec and +0x1d8 stopped.)
                if u.swing.take().is_some_and(|s| !s.done && !s.landed) {
                    u.cooldown = 0.0;
                }
                // (One whose target died among the steps looks again among them, as
                // its timer falls due: traced, the towers-vs-waves game's two at turn
                // 551, taking up creeps after the next step; one after the steps, as
                // the turn ends, `World::looks_after_deaths`.)
                u.extra.look_after_death = after;
                if let Some(l) = u.extra.look_timer.as_mut() {
                    l.1 = due;
                }
                u.acquire_timer = (due as f64 - now) as f32;
            }
        }
    }

    /// The tasks a unit's removal destroys after its own first (`FUN_6f673fe0`: a
    /// block made and destroyed, then the stack top first), as their slots.
    /// A unit's death begun (`FUN_6f679bb0`), its objects as the original's: the
    /// structures attacking it give it up (their tasks freed, the last first, and each
    /// looks again `LOOK_AFTER_DROP` after `at`, the main clock), its AI's objects go,
    /// its orders are cleared (`FUN_6f673610`: a block made and destroyed, its task
    /// stack destroyed top first, then its order), its death's course is put on its
    /// stack (`FUN_6f678280`: its removal 0xd015a, a corpse's two, 0xd0159 and 0xd0158,
    /// 0xd0154, and the corpse's wait 0xd0155), then the death 0xd0153, 0xd0178, 0xd0144,
    /// and a block, and run, the last put first: the block, 0xd0144, and 0xd0178 go.
    /// Traced: the flyer game's creep killed by the Arrow Tower at turn 274 (the tower's
    /// nine tasks, `uref`, `AI-U`, the block, its walk's seven, the block, 0xd0144,
    /// 0xd0178; then, as 0xd0153 runs, its `` `rtl ``, its death trigger's `+tex` and
    /// `pudd`, 0xd0153, and its walk's path and cluster, `World::death_ended`).
    /// Provisional: a corpse's two for a type whose corpse decays (its flag 0x200, not
    /// read), and a current order other than a walk's not freed.
    pub(crate) fn death_begun(&mut self, h: u32, at: f64) -> Option<(u32, Vec<u32>)> {
        self.attackers_give_up(h, at);
        let live = |w: &World, s: u32, b: u32| w.tags.live.get(&s).is_some_and(|e| e.1 == b);
        let ai = self.unit_mut(h).map(|u| std::mem::take(&mut u.extra.ai_slots)).unwrap_or_default();
        for (s, b) in ai {
            if live(self, s, b) {
                self.tags.doom(s);
            }
        }
        let block = self.tags.take(codes::class::TASK_A);
        self.tags.doom(block);
        // (Its walk's: the issue's tree, made at once, its births in a row.)
        let order = self.unit_mut(h).map(|u| std::mem::take(&mut u.extra.order_tags)).unwrap_or_default();
        let base = order.first().and_then(|&s| self.tags.live.get(&s)).filter(|e| &e.0 == codes::class::ORDER_TASKS).map(|e| e.1);
        let mut path = Vec::new();
        if let Some(b0) = base.filter(|_| order.len() >= 17) {
            for k in [7, 6, 5, 4, 3, 2, 0] {
                if live(self, order[k], b0 + k as u32) {
                    self.tags.doom(order[k]);
                }
            }
            let n = order.len();
            for k in [n - 1, n - 2] {
                if live(self, order[k], b0 + k as u32) {
                    path.push(order[k]);
                }
            }
        }
        let decays = self.unit(h).and_then(|u| self.objects.units.get(u.type_id)).is_some_and(|o| o.int("deathType") & 2 != 0);
        let course = if decays { 5 } else { 3 };
        let kept: Vec<u32> = (0..course).map(|_| self.tags.take(codes::class::TASK)).collect();
        let die = self.tags.take(codes::class::TASK);
        let run: Vec<u32> = [codes::class::TASK, codes::class::TASK, codes::class::TASK_A].iter().map(|c| self.tags.take(c)).collect();
        for &s in run.iter().rev() {
            self.tags.doom(s);
        }
        // The corpse's wait (0xd0155) ends as its death has played (its type's death
        // time on), and goes with 0xd0154 (traced: the flyer game's creep, dead at
        // 27.381405, the two freed in turn 304, 3 seconds on); then its flesh decays,
        // 0xd0158 going DecayTime on (traced: the second flyer game's, freed in turn
        // 324, 5 seconds on).  (Provisional: 0xd0159 and the removal kept, the bones'
        // decay not yet read.)
        let mut left = kept.len().min(3);
        if kept.len() == 5 {
            let played = self.unit(h).and_then(|u| self.objects.units.get(u.type_id)).map_or(0.0, |o| o.real("death")) as f64;
            for &s in [kept[4], kept[3]].iter() {
                self.tags.doom_at(at + played, s);
            }
            // (Its timer started as the wait ends, it goes after those due with it that
            // were started before: traced, after its killer's missile, removed BulletDeathTime
            // after the landing that killed it.)
            self.tags.doom_at(at + played + self.misc_f32("DecayTime", 2.0) as f64 + 1e-6, kept[2]);
            left = 2;
        }
        if let Some(u) = self.unit_mut(h) {
            u.extra.death_tasks = kept[..left].to_vec();
        }
        Some((die, path))
    }

    /// A unit KillUnit kills that bursts (SetUnitExploded; not a structure or a hero),
    /// as its death begins, before its death's events: its AI's `uref` and `AI-U`, its
    /// kill's first task, its walk's tree (top first, then its `ordt`; the walk's path
    /// and cluster at the next step, as an ended walk's), its kill's ninth to seventh,
    /// and its regeneration's listener go, in that order with the step's releases.
    /// Read: Mafarazzo TD's creep exploded by WalkEnd's trigger in its LAN load's turn
    /// 144, all freed 0.0001 after the trigger (`World::burst_ends` for the rest).
    pub(crate) fn burst_begun(&mut self, h: u32, kill: &[u32]) {
        let live = |w: &World, s: u32, b: u32| w.tags.live.get(&s).is_some_and(|e| e.1 == b);
        let ai = self.unit_mut(h).map(|u| std::mem::take(&mut u.extra.ai_slots)).unwrap_or_default();
        for (s, b) in ai {
            if live(self, s, b) {
                self.tags.doom(s);
            }
        }
        self.tags.doom(kill[0]);
        let order = self.unit_mut(h).map(|u| {
            u.extra.player_walk = false;
            std::mem::take(&mut u.extra.order_tags)
        }).unwrap_or_default();
        let base = order.first().and_then(|&s| self.tags.live.get(&s)).filter(|e| &e.0 == codes::class::ORDER_TASKS).map(|e| e.1);
        if let Some(b0) = base.filter(|_| order.len() >= 17) {
            for k in [7, 6, 5, 4, 3, 2, 0] {
                if live(self, order[k], b0 + k as u32) {
                    self.tags.doom(order[k]);
                }
            }
            let n = order.len();
            if live(self, order[n - 1], b0 + n as u32 - 1) && live(self, order[n - 2], b0 + n as u32 - 2) {
                let serial = self.unit(h).map_or(0, |u| u.extra.walk_serial);
                self.tags.walk_ended([order[n - 1], order[n - 2]], serial);
            }
        }
        for k in [8, 7, 6] {
            self.tags.doom(kill[k]);
        }
        let listeners = self.unit_mut(h).map(|u| std::mem::take(&mut u.extra.object_slots[1])).unwrap_or_default();
        for (s, b) in listeners {
            if live(self, s, b) {
                self.tags.doom(s);
            }
        }
    }

    /// The bursts of a step's region triggers end after those triggers have all run
    /// (`World::burst_end`): read, Mafarazzo TD's exploded creep's after the Losing life
    /// trigger's execution, made after WalkEnd's.
    pub(crate) fn burst_ends(&mut self) {
        for (h, t) in std::mem::take(&mut self.pending_bursts) {
            self.burst_end(h, t);
        }
    }

    /// A burst unit's end (`World::burst_begun`), its death's effect made (`+EIP`, its
    /// `` `pos ``, two `^pmr`, the second table's): its kill's fifth, fourth, and third
    /// tasks (`kill` those and its second), its abilities (the last first), its own
    /// objects (its reals, path, regions, position, and itself), its kill's second
    /// task, and its detection's record go with the step's releases, and it is gone,
    /// leaving no body.
    pub(crate) fn burst_end(&mut self, h: u32, kill: [u32; 4]) {
        let Some(u) = self.unit(h) else { return };
        let objects = u.extra.object_slots.clone();
        let detection = u.extra.detected_by.first().map(|d| d.1);
        let live = |w: &World, (s, b): (u32, u32)| w.tags.live.get(&s).is_some_and(|e| e.1 == b) && !w.tags.doomed.contains(&s);
        let mut gone = kill[..3].to_vec();
        gone.extend(objects[2].iter().rev().copied().filter(|&o| live(self, o)).map(|o| o.0));
        gone.extend([8, 7, 6, 5, 4, 9, 2, 3, 1, 0].iter().filter_map(|&j| objects[0].get(j).filter(|&&o| live(self, o)).map(|o| o.0)));
        gone.push(kill[3]);
        if let Some(d) = detection.filter(|d| self.tags.live.get(d).is_some_and(|e| &e.0 == codes::class::DETECTION)) {
            gone.push(d);
        }
        for s in gone {
            self.tags.doom(s);
        }
        if let Some(u) = self.unit_mut(h) {
            u.extra.object_slots = Default::default();
            u.extra.detected_by.clear();
        }
        crate::natives::units::remove_unit(self, h);
    }

    /// A unit's death's 0xd0153 done (`World::death_begun`): it goes, then the walk's
    /// path and cluster as the corpse's wait begins.
    pub(crate) fn death_ended(&mut self, die: u32, path: Vec<u32>) {
        self.tags.doom(die);
        for s in path {
            self.tags.doom(s);
        }
    }

    /// The structures whose targets died this turn look again as it ends, after its
    /// frees (`World::death_begun`; traced: the flyer game's Arrow Tower taking up its
    /// next creep after its first's death's 28 objects were freed, its new tasks' last
    /// block freed in that turn too).
    pub(crate) fn looks_after_deaths(&mut self) {
        let towers: Vec<u32> = self.unit_order.iter().copied().filter(|&a| self.unit(a).is_some_and(|u| u.extra.look_after_death)).collect();
        for a in towers {
            if let Some(u) = self.unit_mut(a) {
                u.extra.look_after_death = false;
            }
            let Some(u) = self.unit(a).filter(|u| !u.dead && matches!(u.action, crate::types::Action::Idle)) else { continue };
            let (range, creeps) = (u.acquire_range, u.owner == crate::types::PLAYER_NEUTRAL_AGGRESSIVE);
            let found = self.find_target_ex(a, range, creeps);
            match found {
                Some(t) => {
                    if let Some(u) = self.unit_mut(a) {
                        u.action = crate::types::Action::Attack { target: t };
                    }
                    self.acquired(a, t);
                    self.look_restarted(a, self.tick_dt, 0.5);
                }
                None => self.look_restarted(a, self.tick_dt, 0.5),
            }
        }
        self.tags.free_doomed();
    }

    pub(crate) fn take_attack_tasks(&mut self, h: u32) -> Vec<u32> {
        let Some(u) = self.unit_mut(h) else { return Vec::new() };
        u.extra.first_swing_at = None;
        std::mem::take(&mut u.extra.attack_tasks).into_iter().rev().map(|t| t.0).collect()
    }

    /// PauseUnit's order (`FUN_6f697770`, `FUN_6f688d30`: the first of a unit's pauses,
    /// its count at +0x54, issues order 0xd0005, `FUN_6f69bd80` and `FUN_6f680320`): its
    /// `ordt`, two blocks, seven tasks, and a block, the second, the last, and the three
    /// tasks before it freed with the turn's releases, the rest kept while it is paused
    /// (traced: Mafarazzo TD's test runners, paused as made, their rest freed before
    /// their removals' tasks).  Its end (`FUN_6f6977c0`, `FUN_6f678590`) frees them
    /// (provisional: in what order).
    pub(crate) fn pause_objects(&mut self, h: u32, pause: bool) {
        let Some(paused) = self.unit(h).map(|u| u.paused) else { return };
        if pause && !paused {
            const CODES: [&crate::tags::Code; 11] = [codes::class::ORDER_TASKS, codes::class::TASK_A, codes::class::TASK_A, codes::class::TASK, codes::class::TASK, codes::class::TASK, codes::class::TASK, codes::class::TASK, codes::class::TASK, codes::class::TASK, codes::class::TASK_A];
            let slots: Vec<u32> = CODES.iter().map(|c| self.tags.take(c)).collect();
            for k in [1, 10, 9, 8, 7] {
                self.tags.doom(slots[k]);
            }
            if let Some(u) = self.unit_mut(h) {
                u.extra.pause_order = [0, 2, 3, 4, 5, 6].map(|k| slots[k]).to_vec();
            }
        } else if !pause && paused {
            let rest = self.unit_mut(h).map(|u| std::mem::take(&mut u.extra.pause_order)).unwrap_or_default();
            for &s in rest.iter().rev() {
                self.tags.doom(s);
            }
        }
    }

    /// A computer's unit runs from its attacker (`World::flee_from`) by the engine's walk:
    /// its move order's objects as a player's (`Tags::player_move_issued`), the seven it
    /// lets go at once destroyed on the frame after a missile's landing that made it
    /// run, then the `+ord` of the order 0xd0006 queued after it (`FUN_6f693450`), taking
    /// the freed `^clg`'s slot (traced: Mafarazzo TD's fort runner, struck by the Plague
    /// Tower's first missile at the end of turn 9: its walk's seventeen, the `^clg`
    /// freed and the `+ord` in its slot, the seven freed in turn 10 after the missile's
    /// `Atrg`; struck again at its post in turn 141, the seven freed in that turn).
    pub(crate) fn flee_walk(&mut self, h: u32) {
        let n = self.tags.doomed.len();
        self.start_player_walk(h);
        if let Some(at) = self.landing_clock {
            let held = self.tags.doomed.split_off(n.min(self.tags.doomed.len()));
            for s in held {
                self.tags.doom_at(at + crate::bullets::DESTROYED_AFTER, s);
            }
        }
        let ord = self.tags.take(codes::class::ORDER);
        if let Some(u) = self.unit_mut(h) {
            u.extra.ai_idle_ord = Some(ord);
        }
    }

    /// The walk of a computer's unit with its look (order 0xd0006) queued after it ends:
    /// the look runs at once in its step (`FUN_6f9bd6e0`, its task made), and a unit
    /// keeping a guard post more than 50 from it attack-moves back there
    /// (`FUN_6f9c0b00`): that order's `ordt`, the `+ord` of its own look, a block, a
    /// `tskO`, two tasks, a `tsk.`, four tasks, a block, and its walk's `^clg` (freed at
    /// once), `^clu`, and `l^pa`.  Then the walk's order's tree, the look's task and
    /// `+ord`, the new order's last block and four tasks before it, the last first, and
    /// the walk's path go as the step ends.  Traced: the fort runner's flight ending at
    /// turn 84, its walk back's at 139 (the order's tree, the look's task and `+ord`,
    /// its path).  Whether the unit walks on.  (Provisional: only for a unit without a
    /// weapon, whose walk back takes up nothing.)
    pub(crate) fn ai_walk_ended(&mut self, h: u32) -> bool {
        let Some(u) = self.unit(h) else { return false };
        let (owner, x, y, ord) = (u.owner, u.x, u.y, u.extra.ai_idle_ord);
        let armed = !crate::sim::unit_weapons(self, u).is_empty();
        let slots = self.unit_mut(h).map(|u| std::mem::take(&mut u.extra.order_tags)).unwrap_or_default();
        let post = self.posts(owner).iter().find(|g| g.unit == h).map(|g| (g.x, g.y));
        let look = self.tags.take(codes::class::TASK);
        let back = post.filter(|p| (x - p.0).hypot(y - p.1) > 50.0 && !armed);
        let serial = self.tags.born;
        let new: Option<Vec<u32>> = back.map(|_| {
            const CODES: [&crate::tags::Code; 15] = [codes::class::ORDER_TASKS, codes::class::ORDER, codes::class::TASK_A, codes::class::TASK_O, codes::class::TASK, codes::class::TASK, codes::class::TASK_POINT, codes::class::TASK, codes::class::TASK, codes::class::TASK, codes::class::TASK, codes::class::TASK_A, codes::class::CLUSTER_GOAL, codes::class::CLUSTER, codes::class::PATH];
            let n: Vec<u32> = CODES.iter().map(|c| self.tags.take(c)).collect();
            self.tags.release(n[12]);
            n
        });
        let (tree, path): (&[usize], [usize; 2]) = match slots.len() {
            17 => (&[7, 6, 5, 4, 3, 2, 0], [16, 15]),
            15 => (&[6, 5, 4, 3, 2, 0], [14, 13]),
            _ => (&[], [usize::MAX; 2]),
        };
        for &k in tree {
            self.tags.release_in_step(slots[k]);
        }
        self.tags.release_in_step(look);
        if let Some(o) = ord {
            self.tags.release_in_step(o);
        }
        if let Some(n) = &new {
            for k in [11, 10, 9, 8, 7] {
                self.tags.release_in_step(n[k]);
            }
        }
        for k in path {
            if let Some(&s) = slots.get(k) {
                self.tags.release_in_step(s);
            }
        }
        let Some(u) = self.unit_mut(h) else { return false };
        u.extra.order_queue.retain(|e| e.0 != crate::aiposts::AI_IDLE_ORDER);
        match (new, back) {
            (Some(n), Some((px, py))) => {
                u.action = crate::types::Action::AttackMove { x: px, y: py };
                u.order = crate::types::Order { id: crate::natives::basic::order_id("attack"), target_x: px, target_y: py, target: 0 };
                u.extra.ai_idle_ord = Some(n[1]);
                u.extra.order_tags = n;
                u.extra.walk_serial = serial;
                u.extra.player_walk = true;
                u.extra.coarse = None;
                u.extra.move_coarse = None;
                u.extra.plan_at[0] = None;
                u.extra.coarse_done = false;
                true
            }
            _ => {
                u.extra.ai_idle_ord = None;
                false
            }
        }
    }
}
