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

impl World {
    /// Whether a unit's attacks are a structure's: it cannot walk into range
    /// (`FUN_6f66f030`, no move ability or its move disabled), so it takes up only
    /// what it reaches (`FUN_6f4959f0`) and gives up what leaves its reach
    /// (`FUN_6f495180`).
    pub(crate) fn attacks_in_place(&self, h: u32) -> bool {
        self.unit(h).is_some_and(|u| u.is_building && !u.extra.uprooted)
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
            // (The last block runs and goes at once.)
            let (top, _) = tasks.pop().unwrap();
            self.tags.doom(top);
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
        let Some(u) = self.unit(h) else { return };
        let mut tasks = u.extra.attack_tasks.clone();
        self.pop_attack(&mut tasks);
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
        let attackers: Vec<u32> = self.unit_order.iter().copied().filter(|&a| self.attacks_in_place(a) && self.unit(a).is_some_and(|u| u.extra.acquired == h && !u.extra.attack_tasks.is_empty())).collect();
        for a in attackers {
            self.attack_given_up(a, at);
            let due = (at + LOOK_AFTER_DROP as f64) as f32;
            let now = self.time_exact;
            if let Some(u) = self.unit_mut(a) {
                u.action = crate::types::Action::Idle;
                // (A damage point still to come is called off, and the cooldown with it,
                // `FUN_6f497e20`: its timers at +0x1ec and +0x1d8 stopped.)
                if u.swing.take().is_some_and(|s| !s.done && !s.landed) {
                    u.cooldown = 0.0;
                }
                u.extra.look_after_death = true;
                if let Some(l) = u.extra.look_timer.as_mut() {
                    l.1 = due;
                }
                u.acquire_timer = (due as f64 - now) as f32;
            }
        }
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
        // 27.381405, the two freed in turn 304, 3 seconds on).
        if kept.len() == 5 {
            let played = self.unit(h).and_then(|u| self.objects.units.get(u.type_id)).map_or(0.0, |o| o.real("death")) as f64;
            for &s in [kept[4], kept[3]].iter() {
                self.tags.doom_at(at + played, s);
            }
        }
        if let Some(u) = self.unit_mut(h) {
            u.extra.death_tasks = kept[..kept.len().min(3)].to_vec();
        }
        Some((die, path))
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
