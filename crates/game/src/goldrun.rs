//! A worker mining gold as the original's engine does it (CAbilityHarvest with a gold
//! mine's CAbilityGoldMine, traced with a Booty Bay peon): its walks are the engine's
//! (`World::walk_steps`, its way searched to the mine's or the hall's footprint), it
//! goes into the mine as its walk ends there, comes out at the same place when the
//! mine's timer is up, walks home, and as that walk ends gives its gold and heads back,
//! each order with its objects (`HARVEST_PLAYER`, `HARVEST_OWN`, `RETURN`).  On each
//! arrival at the mine its speed is tuned toward a five-second round trip
//! (`FUN_6f437ae0`).

use serde::{Deserialize, Serialize};

use crate::efloat as ef;
use crate::tags::Code;
use crate::types::*;
use crate::world::World;

/// A harvest order given by a player (a smart order on a mine): its order, a block, a
/// second block, four tasks, the three target tasks, a task, the move to the mine (a
/// target task), three tasks, a block (`FUN_6f419380`), and its move's path cluster
/// group, cluster, and path.
const HARVEST_PLAYER: [&Code; 19] = [b"ordt", b"tskA", b"tskA", b"task", b"task", b"task", b"task", b"tskt", b"tskt", b"tskt", b"task", b"tskt", b"task", b"task", b"task", b"tskA", b"^clg", b"^clu", b"l^pa"];
/// The worker's own harvest order from the hall: one block at its head and four tasks
/// at its tail (its flag 0x40000000 adds 0xd0150).
const HARVEST_OWN: [&Code; 19] = [b"ordt", b"tskA", b"task", b"task", b"task", b"task", b"tskt", b"tskt", b"tskt", b"task", b"tskt", b"task", b"task", b"task", b"task", b"tskA", b"^clg", b"^clu", b"l^pa"];
/// The order home with the gold: its order, a block, four tasks, two target tasks, four
/// tasks, a block, and its move's objects.
const RETURN: [&Code; 16] = [b"ordt", b"tskA", b"task", b"task", b"task", b"task", b"tskt", b"tskt", b"task", b"task", b"task", b"task", b"tskA", b"^clg", b"^clu", b"l^pa"];

/// Where a gold run stands.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Phase {
    /// Walking to the mine.
    ToMine,
    /// In the mine until the agent queue's clock reaches this (f32 bits).
    InMine(u32),
    /// Walking home with the gold to this structure.
    ToHall(u32),
}

/// Which order holds the run's objects now.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Kind {
    Player,
    Own,
    Return,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GoldRun {
    pub mine: u32,
    pub phase: Phase,
    pub kind: Kind,
    /// The order's objects' slots, by place in its list (those freed already kept).
    pub order: Vec<u32>,
    /// Whether this worker has been in a mine (its first entry makes a `+fgm`).
    pub entered: bool,
    /// The round trip's timer (CAgentTimer at +0x204): its due on the agent queue.
    pub timer_due: Option<u32>,
    /// The speed bonus now (+0x21c), the last trip's travel time (+0x224), and the bonus
    /// before (+0x22c), as f32 bits.
    pub bonus: u32,
    pub travel: u32,
    pub bonus_before: u32,
    /// The worker's speed factor (its movement ability's +0x78), 1 and the bonuses added.
    pub factor: u32,
}

impl GoldRun {
    /// The round trip's speed tuning (`FUN_6f437ae0`): the time since the last arrival
    /// (its 10-second timer's), less the mine's time, is the trip's travel; the factor
    /// that would have made it and the last trip's (when there was one) take five
    /// seconds less the mine's, averaged, clamped to 0.5 to 1.2, is the new bonus plus 1
    /// (read: a peon's speeds 6.041100 and 6.025844 cells a second, after trips of
    /// 5.069793 and 4.979798 seconds).
    pub fn tune(&mut self, now: u32, mine_time: u32) {
        let ten = 10f32.to_bits();
        let one = 1f32.to_bits();
        let left = match self.timer_due {
            Some(due) if f32::from_bits(ef::sub(due, now)) > 0.0 => ef::sub(due, now),
            _ => 0,
        };
        let travel = ef::sub(ef::sub(ten, left), mine_time);
        let old = self.bonus;
        let (bonus, before) = if f32::from_bits(left) <= 0.0 || f32::from_bits(ten) <= f32::from_bits(mine_time) || f32::from_bits(travel) <= 0.0 {
            self.travel = 0;
            (0, 0)
        } else {
            let mut num = ef::mul(travel, ef::add(old, one));
            let mut n = one;
            if f32::from_bits(self.travel) > 0.0 {
                num = ef::add(num, ef::mul(self.travel, ef::add(self.bonus_before, one)));
                n = ef::add(n, one);
            }
            let f = ef::div(num, ef::mul(n, ef::sub(5f32.to_bits(), mine_time)));
            let f = f32::from_bits(f).clamp(0.5, 1.2).to_bits();
            self.travel = travel;
            (ef::sub(f, one), old)
        };
        self.bonus_before = before;
        self.factor = ef::add(self.factor, ef::sub(bonus, old));
        self.bonus = bonus;
        self.timer_due = Some(ef::add(now, ten));
    }
}

impl World {
    /// Whether a worker's harvest of `target` runs as the original's engine does: a
    /// ground worker gathering with Ahar from a gold mine (Agld).
    pub fn gold_run_eligible(&self, h: u32, target: u32) -> bool {
        self.pathing.is_some()
            && self.ability_with_code(h, "Ahar").is_some()
            && self.ability_with_code(h, "Aaha").is_none()
            && self.ability_with_code(h, "Awha").is_none()
            && self.unit(target).is_some_and(|m| !m.dead && !m.hidden && !m.pathing_cells.is_empty())
            && self.ability_with_code(target, "Agld").is_some()
            && self.move_kind(h) == crate::pathing::Move::Foot
    }

    /// A harvest order on a gold mine: its objects, the walk to the mine from the next
    /// step.  `player`: given by a player's action (its five objects go as the next turn
    /// starts, as the action's do); else the worker's own (they go with the tick's
    /// orders).
    pub fn start_gold_run(&mut self, h: u32, mine: u32, player: bool) {
        let codes: &[&Code] = if player { &HARVEST_PLAYER } else { &HARVEST_OWN };
        let order: Vec<u32> = codes.iter().map(|c| self.tags.take(c)).collect();
        self.tags.release(order[16]);
        let five: [usize; 5] = if player { [1, 15, 14, 13, 12] } else { [15, 14, 13, 12, 11] };
        for k in five {
            if player {
                self.tags.doom(order[k]);
            } else {
                self.tags.issued.push(order[k]);
            }
        }
        let serial = self.tags.born;
        let old = self.unit(h).and_then(|u| u.extra.gold_run.clone());
        let run = match old {
            Some(mut r) => {
                r.mine = mine;
                r.phase = Phase::ToMine;
                r.kind = if player { Kind::Player } else { Kind::Own };
                r.order = order;
                r
            }
            None => GoldRun { mine, phase: Phase::ToMine, kind: if player { Kind::Player } else { Kind::Own }, order, entered: false, timer_due: None, bonus: 0, travel: 0, bonus_before: 0, factor: 1f32.to_bits() },
        };
        let Some(u) = self.unit_mut(h) else { return };
        u.extra.gold_run = Some(run);
        u.action = Action::Harvest { target: mine };
        u.harvest_source = mine;
        u.path.clear();
        u.extra.mover = None;
        u.extra.walk_serial = serial;
    }

    /// The structure a gold run walks to now, if walking.
    pub fn gold_run_target(&self, h: u32) -> Option<u32> {
        let r = self.unit(h)?.extra.gold_run.as_ref()?;
        match r.phase {
            Phase::ToMine => Some(r.mine),
            Phase::ToHall(hall) => Some(hall),
            Phase::InMine(_) => None,
        }
    }

    /// Its walk's goal in world units: the middle of the structure it walks to.
    pub fn gold_run_goal(&self, h: u32) -> Option<(f32, f32)> {
        self.gold_run_target(h).and_then(|t| self.unit(t)).map(|t| (t.x, t.y))
    }

    /// Its speed: the unit's base speed and flat bonuses times its factor, by the
    /// engine's arithmetic (`FUN_6f5fc900`).
    pub fn gold_run_speed(&self, h: u32) -> Option<f32> {
        let factor = self.unit(h)?.extra.gold_run.as_ref()?.factor;
        let u = self.unit(h)?;
        let ub = self.upgrade_bonus(u.owner, u.type_id);
        let base = (u.move_speed + u.item_bonus.move_speed + ub.move_speed).to_bits();
        Some(f32::from_bits(ef::mul(base, factor)))
    }

    /// Its walk ended at the structure it walked to, in step `k` (found in the step's
    /// search, before the walkers move: the provider's answer 3).
    pub fn gold_run_arrive(&mut self, h: u32, k: usize) {
        let Some(run) = self.unit(h).and_then(|u| u.extra.gold_run.clone()) else { return };
        let path = [run.order[run.order.len() - 1], run.order[run.order.len() - 2]];
        let agent = self.step_agent.get(k).copied().unwrap_or(self.agent_step);
        match run.phase {
            Phase::ToMine => self.enter_mine(h, run, path, agent),
            Phase::ToHall(hall) => self.drop_gold(h, run, hall, path),
            Phase::InMine(_) => {}
        }
    }

    /// At the mine (`FUN_6f437ae0`): a worker's first entry makes a `+fgm`; the move's
    /// target task and the task before it go at once and its path as the next step ends
    /// (`Tags::late_path`); the round trip's timing tunes its speed; it goes in for the
    /// mine's time (Agld DataB), taking its load.
    fn enter_mine(&mut self, h: u32, mut run: GoldRun, path: [u32; 2], agent: u32) {
        if !run.entered {
            self.tags.take(b"+fgm");
            run.entered = true;
        }
        let (target_task, task) = match run.kind {
            Kind::Player => (11, 10),
            _ => (10, 9),
        };
        self.tags.release(run.order[target_task]);
        self.tags.release(run.order[task]);
        self.tags.late_path(path);
        let mine = run.mine;
        let (dur, gold_cap) = match (self.ability_with_code(mine, "Agld"), self.ability_with_code(h, "Ahar")) {
            (Some((m, ml)), Some((a, al))) => (self.ability_real(m, "DataB", ml), self.ability_real(a, "DataC", al) as i32),
            _ => (1.0, 10),
        };
        run.tune(agent, dur.to_bits());
        run.phase = Phase::InMine(ef::add(agent, dur.to_bits()));
        let before = self.unit(mine).map_or(0, |m| m.resource);
        let take = gold_cap.min(before);
        if let Some(m) = self.unit_mut(mine) {
            m.resource -= take;
            m.inside += 1;
        }
        self.gold_warning(h, before, before - take);
        if let Some(u) = self.unit_mut(h) {
            u.carry_gold = take;
            u.inside = mine;
            u.hidden = true;
            u.extra.gold_run = Some(run);
        }
    }

    /// The mine's timer is up (`h`'s due on the agent queue): out at the place it went
    /// in, its order home given (`RETURN`: its `^clg` at once, the harvest order's tree
    /// after, its own five with the tick's orders), walking from step `k`.
    pub fn leave_gold_mine(&mut self, h: u32, k: usize) {
        let Some(mut run) = self.unit(h).and_then(|u| u.extra.gold_run.clone()) else { return };
        let order: Vec<u32> = RETURN.iter().map(|c| self.tags.take(c)).collect();
        self.tags.release(order[13]);
        let rest: &[usize] = match run.kind {
            Kind::Player => &[9, 8, 7, 6, 5, 4, 3, 2, 0],
            _ => &[8, 7, 6, 5, 4, 3, 2, 1, 0],
        };
        for &i in rest {
            self.tags.release(run.order[i]);
        }
        for i in [12, 11, 10, 9, 8] {
            self.tags.issued.push(order[i]);
        }
        let mine = run.mine;
        let hall = self.drop_off(h).unwrap_or(0);
        run.phase = Phase::ToHall(hall);
        run.kind = Kind::Return;
        run.order = order;
        let serial = self.tags.born;
        if let Some(m) = self.unit_mut(mine) {
            m.inside = m.inside.saturating_sub(1);
        }
        if let Some(u) = self.unit_mut(h) {
            u.inside = 0;
            u.hidden = false;
            u.extra.gold_run = Some(run);
            u.extra.walk_from = Some(k);
            u.extra.walk_serial = serial;
            u.path.clear();
            u.order = Order { id: crate::natives::basic::order_id("resumeharvesting"), target_x: 0.0, target_y: 0.0, target: 0 };
        }
    }

    /// Home with the gold (`FUN_6f438880`): given, and its harvest order back to the mine
    /// at once (`HARVEST_OWN`, its `^clg` freed then); the order home's tree goes as the
    /// step ends, after its walk ends' objects (`Tags::after_step`), then the new order's
    /// five; the walk home's path as the next step ends.
    fn drop_gold(&mut self, h: u32, run: GoldRun, _hall: u32, path: [u32; 2]) {
        let (owner, gold) = self.unit(h).map_or((0, 0), |u| (u.owner, u.carry_gold));
        if gold > 0 {
            self.gather(owner, h, gold, 0, 0.0);
        }
        if let Some(u) = self.unit_mut(h) {
            u.carry_gold = 0;
        }
        let codes = &HARVEST_OWN;
        let order: Vec<u32> = codes.iter().map(|c| self.tags.take(c)).collect();
        self.tags.release(order[16]);
        for i in [7, 6, 5, 4, 3, 2, 1, 0] {
            self.tags.after_step(run.order[i]);
        }
        for i in [15, 14, 13, 12, 11] {
            self.tags.issued.push(order[i]);
        }
        self.tags.late_path(path);
        let mut run = run;
        run.phase = Phase::ToMine;
        run.kind = Kind::Own;
        run.order = order;
        let serial = self.tags.born;
        if let Some(u) = self.unit_mut(h) {
            u.extra.gold_run = Some(run);
            u.extra.walk_serial = serial;
            u.path.clear();
            u.order = Order { id: crate::natives::basic::order_id("harvest"), target_x: 0.0, target_y: 0.0, target: 0 };
        }
    }

    /// Where a walker walks: a move order's point, or a gold run's structure's middle.
    pub fn walk_goal(&self, h: u32) -> Option<(f32, f32)> {
        match self.unit(h)?.action {
            Action::Move { x, y } => Some((x, y)),
            Action::Harvest { .. } => self.gold_run_goal(h),
            _ => None,
        }
    }

    /// The walkers of step `k` whose walks to a structure end as it starts (their way's
    /// last point within reach, on the structure's bearing: the provider's answer 3, in
    /// its pass before the walkers move) arrive; the others are left.
    pub(crate) fn gold_run_step_ends(&mut self, walkers: Vec<u32>, k: usize) -> Vec<u32> {
        let Some(origin) = self.pathing.as_ref().map(|p| p.origin) else { return walkers };
        let Some(&clock) = self.step_clocks.get(k) else { return walkers };
        let cell = |w: f32, o: f32| ef::div(ef::sub(w.to_bits(), o.to_bits()), crate::pathing::CELL.to_bits());
        let mut left = Vec::with_capacity(walkers.len());
        for h in walkers {
            let ends = (|| {
                let u = self.unit(h)?;
                u.extra.gold_run.as_ref()?;
                let m = u.extra.mover?;
                let to = *u.path.first().filter(|_| u.path.len() == 1)?;
                let g = self.gold_run_goal(h)?;
                let gait = crate::motion::Gait { top: 0, grow: 0, turn: 0, window: 0, reach: 0.49f32.to_bits() };
                m.arrived(&gait, (cell(to.0, origin[0]), cell(to.1, origin[1])), (cell(g.0, origin[0]), cell(g.1, origin[1])), clock).then_some(m)
            })();
            if let (Some(u), Some(m)) = (self.unit(h), self.unit(h).and_then(|u| u.extra.mover)) {
                if u.extra.gold_run.is_some() {
                    let (x, y) = m.at(clock);
                    log::debug!("gold run {h} step {k} clock {:.6} at {:.6} {:.6} heading {:.6} v {:.6} {:.6} path {:?} ends {}", f32::from_bits(clock), f32::from_bits(x), f32::from_bits(y), f32::from_bits(m.heading), f32::from_bits(m.vx), f32::from_bits(m.vy), u.path, ends.is_some());
                }
            }
            match ends {
                Some(m) => {
                    if let Some(u) = self.unit_mut(h) {
                        u.extra.walk_heading = Some(m.heading);
                        u.extra.mover = None;
                        u.extra.step_from = None;
                        u.path.clear();
                        crate::sim::set_anim(u, "stand");
                    }
                    self.gold_run_arrive(h, k);
                }
                None => left.push(h),
            }
        }
        left
    }

    /// A gold run's way to its structure (`World::local_path`'s search, 700 nodes deep,
    /// to the first node whose ring touches the structure's footprint, toward its
    /// middle).
    pub(crate) fn gold_run_search(&self, h: u32, cells: &impl crate::lrpath::Cells, start: (i32, i32), end: (i32, i32), class: u8) -> Option<(crate::lrpath::Outcome, bool, u32)> {
        let t = self.gold_run_target(h)?;
        let foot: std::collections::HashSet<(i32, i32)> = self.unit(t)?.pathing_cells.iter().copied().collect();
        Some(crate::lrpath::search_to_region(cells, start, end, class, 700, |x, y| foot.contains(&(x, y))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A Booty Bay peon's arrivals at its gold mine (the agent queue's clocks read under
    /// winedbg at `FUN_6f437ae0`): no tuning the first time, then its speeds as the
    /// original set them (`FUN_6f15ff40`'s writes, cells a second).
    #[test]
    fn a_peons_speed_is_tuned_as_the_originals() {
        let mut run = GoldRun { mine: 0, phase: Phase::ToMine, kind: Kind::Player, order: Vec::new(), entered: false, timer_due: None, bonus: 0, travel: 0, bonus_before: 0, factor: 1f32.to_bits() };
        // As winedbg printed them, to six places.
        let speed = |r: &GoldRun| (f32::from_bits(ef::mul(190f32.to_bits(), r.factor)) as f64 / 32.0 * 1e6).round() / 1e6;
        run.tune(42.838799f32.to_bits(), 1f32.to_bits());
        assert_eq!(speed(&run), 5.9375);
        run.tune(47.908592f32.to_bits(), 1f32.to_bits());
        assert_eq!(speed(&run), 6.041100);
        run.tune(52.888390f32.to_bits(), 1f32.to_bits());
        assert_eq!(speed(&run), 6.025844);
    }
}
