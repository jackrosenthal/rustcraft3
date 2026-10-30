//! A worker mining gold as the original's engine does it (CAbilityHarvest with a gold
//! mine's CAbilityGoldMine, traced with a Booty Bay peon): its walks are the engine's
//! (`World::walk_steps`, its way searched to the mine's or the hall's footprint), it
//! goes into the mine as its walk ends there, comes out at the same place when the
//! mine's timer is up, walks home, and as that walk ends gives its gold and heads back,
//! each order with its objects (`HARVEST_PLAYER`, `HARVEST_OWN`, `RETURN`).  On each
//! arrival at the mine its speed is tuned toward a five-second round trip
//! (`FUN_6f437ae0`).
//!
//! Lumber runs the same way with a tree for the mine (read: a Booty Bay peon sent to a
//! tree): its orders lack the move to the mine and the task before it (`LUMBER_PLAYER`,
//! `LUMBER_OWN`); at the tree it walks to reach it (`REACH`) and then chops until its
//! load is full, going home as its last swing's backswing ends.

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
/// A harvest order on a tree given by a player: the gold order without the move to the
/// mine and the task before it, its move to the tree the third target task.
const LUMBER_PLAYER: [&Code; 17] = [b"ordt", b"tskA", b"tskA", b"task", b"task", b"task", b"task", b"tskt", b"tskt", b"tskt", b"task", b"task", b"task", b"tskA", b"^clg", b"^clu", b"l^pa"];
/// The worker's own harvest order on a tree from the hall.
const LUMBER_OWN: [&Code; 17] = [b"ordt", b"tskA", b"task", b"task", b"task", b"task", b"tskt", b"tskt", b"tskt", b"task", b"task", b"task", b"task", b"tskA", b"^clg", b"^clu", b"l^pa"];
/// At the tree, short of chopping (`FUN_6f4377d0`, event 0xd0184 as the move ends): its
/// walk to reach the tree (0xd0171, `FUN_6f6926b0`), the end of its ghosting (0xd0151,
/// `FUN_6f691f20`, which lets it chop: `FUN_6f43b7e0` sets the harvest's 0x1000), a
/// block (`FUN_6f6927a0(5)`), and the walk's path cluster group, cluster, and path.
const REACH: [&Code; 6] = [b"tskt", b"task", b"tskA", b"^clg", b"^clu", b"l^pa"];

/// Where a gold run stands.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Phase {
    /// Walking to the mine.
    ToMine,
    /// In the mine until the agent queue's clock reaches this (f32 bits).
    InMine(u32),
    /// At the mine, waiting in its queue for room, with its place there (the `uref`'s
    /// slot).
    Waiting(u32),
    /// Walking home with the gold to this structure.
    ToHall(u32),
    /// At its tree, walking to reach it: the walk's target task, its path (`l^pa`,
    /// `^clu`), and its serial; it reaches it in the next step.
    Reaching { task: u32, path: [u32; 2], serial: u32 },
    /// Chopping until the agent queue's clock reaches this (f32 bits): its last swing's
    /// backswing over, its load full.
    Chopping(u32),
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
    /// The gold mine, or the tree.
    pub mine: u32,
    /// Whether it gathers lumber from a tree.
    #[serde(default)]
    pub lumber: bool,
    pub phase: Phase,
    pub kind: Kind,
    /// The order's objects' slots, by place in its list (those freed already kept).
    pub order: Vec<u32>,
    /// Whether this worker has been in a mine (its harvest's flag 0x40000000: its walks
    /// since ghost).
    pub entered: bool,
    /// Whether a walk of its run has ghosted (its region's 0xd6 cleared:
    /// `Unit::region_flags`).
    #[serde(default)]
    pub ghosted: bool,
    /// Whether its ghosting ended at the mine since its last walk began (its region's
    /// 0x10: `Unit::region_flags`).
    #[serde(default)]
    pub at_mine: bool,
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

/// The agent queue's clock as `chops` swings begun at `start` are over: each chop at the
/// damage point `point`, its backswing `back` (at least 0.01) after, and the next swing
/// `dur` less both (at least 0.01) after that, each sum by the engine's arithmetic.
fn chops_end(start: u32, point: u32, back: u32, dur: u32, chops: u32) -> u32 {
    let least = 0.01f32.to_bits();
    let at_least = |t: u32| if f32::from_bits(t) < f32::from_bits(least) { least } else { t };
    let back = at_least(back);
    let gap = at_least(ef::sub(ef::sub(dur, point), back));
    let mut t = start;
    for i in 0..chops {
        let out = ef::add(ef::add(t, point), back);
        if i + 1 == chops {
            return out;
        }
        t = ef::add(out, gap);
    }
    t
}

impl World {
    /// Whether a worker's harvest of `target` runs as the original's engine does: a
    /// ground worker gathering with Ahar from a gold mine (Agld) or a tree.
    pub fn gold_run_eligible(&self, h: u32, target: u32) -> bool {
        self.pathing.is_some()
            && self.ability_with_code(h, "Ahar").is_some()
            && self.ability_with_code(h, "Aaha").is_none()
            && self.ability_with_code(h, "Awha").is_none()
            && (self.unit(target).is_some_and(|m| !m.dead && !m.hidden && !m.pathing_cells.is_empty()) && self.ability_with_code(target, "Agld").is_some()
                || self.is_tree_pub(target) && !self.footprint(target).is_empty())
            && self.move_kind(h) == crate::pathing::Move::Foot
    }

    /// A structure's or a tree's pathing footprint.
    fn footprint(&self, t: u32) -> &[(i32, i32)] {
        match self.handles.get(t) {
            Some(Obj::Unit(u)) => &u.pathing_cells,
            Some(Obj::Destructable(d)) => &d.pathing_cells,
            _ => &[],
        }
    }

    /// A harvest order on a gold mine: its objects, the walk to the mine from the next
    /// step.  `player`: given by a player's action (its five objects go as the next turn
    /// starts, as the action's do); else the worker's own (they go with the tick's
    /// orders).
    pub fn start_gold_run(&mut self, h: u32, mine: u32, player: bool) {
        let lumber = self.is_tree_pub(mine);
        let codes: &[&Code] = match (lumber, player) {
            (false, true) => &HARVEST_PLAYER,
            (false, false) => &HARVEST_OWN,
            (true, true) => &LUMBER_PLAYER,
            (true, false) => &LUMBER_OWN,
        };
        // Its walk is newer than any before it: its serial is taken before its order's
        // objects, as a wanderer's is (read: a wanderer's path, its walk begun at the
        // tick a worker left the mine and ended before the worker's walk home, freed
        // before the worker's arrival).
        let serial = self.tags.born;
        let order: Vec<u32> = codes.iter().map(|c| self.tags.take(c)).collect();
        // Its `^clg` at once; the block at its head (a player's) and the last five
        // before it, the first of them its closing block, as the next turn starts or
        // with the tick's orders (read: a tree's order 1, 13, 12, 11, and 10).
        let n = order.len();
        self.tags.release(order[n - 3]);
        let five: [usize; 5] = if player { [1, n - 4, n - 5, n - 6, n - 7] } else { [n - 4, n - 5, n - 6, n - 7, n - 8] };
        for k in five {
            if player {
                self.tags.doom(order[k]);
            } else {
                self.tags.issued.push(order[k]);
            }
        }
        let old = self.unit(h).and_then(|u| u.extra.gold_run.clone());
        let run = match old {
            Some(mut r) => {
                r.mine = mine;
                r.lumber = lumber;
                r.phase = Phase::ToMine;
                r.kind = if player { Kind::Player } else { Kind::Own };
                r.order = order;
                r.ghosted |= r.entered;
                r.at_mine &= !r.entered;
                r
            }
            None => GoldRun { mine, lumber, phase: Phase::ToMine, kind: if player { Kind::Player } else { Kind::Own }, order, entered: false, ghosted: false, at_mine: false, timer_due: None, bonus: 0, travel: 0, bonus_before: 0, factor: 1f32.to_bits() },
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
            Phase::InMine(_) | Phase::Waiting(_) | Phase::Reaching { .. } | Phase::Chopping(_) => None,
        }
    }

    /// Its walk's goal in world units: the middle of the structure or tree it walks to.
    pub fn gold_run_goal(&self, h: u32) -> Option<(f32, f32)> {
        self.gold_run_target(h).and_then(|t| self.widget_pos(t))
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
            Phase::ToMine => {
                let mut run = run;
                run.at_mine = true;
                if let Some(u) = self.unit_mut(h) {
                    u.extra.gold_run = Some(run.clone());
                }
                if run.lumber {
                    return self.reach_tree(h, run, path);
                }
                self.push_off_ghosts(h);
                self.enter_mine(h, run, path, agent)
            }
            Phase::ToHall(hall) => self.drop_gold(h, run, hall, path),
            Phase::InMine(_) | Phase::Waiting(_) | Phase::Reaching { .. } | Phase::Chopping(_) => {}
        }
    }

    /// At the mine (`FUN_6f437ae0`): a worker's first entry makes a `+fgm`; the move's
    /// target task and the task before it go as the step ends, in the walkers' order, and
    /// its path in the next step (`Tags::walk_ended`); the round trip's timing tunes its
    /// speed; it goes in for the mine's time (Agld DataB), taking its load, when the mine
    /// has room (Agld DataC workers), and else waits where it stands at the end of the
    /// mine's queue, a `uref` its place there (`FUN_6f5c72c0`, `FUN_6f5c75e0`).
    fn enter_mine(&mut self, h: u32, mut run: GoldRun, path: [u32; 2], agent: u32) {
        // The mine's first worker in makes its `+fgm` (read: one for five peons).
        if !self.unit(run.mine).is_some_and(|m| m.extra.mine_shaft) {
            self.tags.take(b"+fgm");
            if let Some(m) = self.unit_mut(run.mine) {
                m.extra.mine_shaft = true;
            }
        }
        run.entered = true;
        let (target_task, task) = match run.kind {
            Kind::Player => (11, 10),
            _ => (10, 9),
        };
        self.tags.release_in_step(run.order[target_task]);
        self.tags.release_in_step(run.order[task]);
        let serial = self.unit(h).map_or(0, |u| u.extra.walk_serial);
        self.tags.walk_ended(path, serial);
        let mine = run.mine;
        let (dur, room) = self.mine_time_and_room(mine);
        run.tune(agent, dur.to_bits());
        if self.unit(mine).is_some_and(|m| m.inside >= room) {
            run.phase = Phase::Waiting(self.tags.take(b"uref"));
            if let Some(m) = self.unit_mut(mine) {
                m.extra.mine_queue.push(h);
            }
            if let Some(u) = self.unit_mut(h) {
                u.extra.gold_run = Some(run);
            }
            return;
        }
        self.into_mine(h, run, ef::add(agent, dur.to_bits()));
    }

    /// As its walk to the mine ends its ghosting ends (task 0xd0151, `FUN_6f43b7e0`,
    /// whether it ghosted or not): its owner's units within 0.1 of touching it that
    /// ghost, or have, are each placed again where they stand, clear of the units about
    /// them (`FUN_6f427510`, `FUN_6f427870`; read: a peon walking home through the cell
    /// where another stopped at the mine moved, still walking, to the next cell clear of
    /// that one and the one waiting beside it).
    fn push_off_ghosts(&mut self, h: u32) {
        let Some((owner, x, y, c)) = self.unit(h).map(|u| (u.owner, u.x, u.y, u.collision)) else { return };
        let Some(origin) = self.pathing.as_ref().map(|p| p.origin) else { return };
        let near: Vec<u32> = self
            .unit_order
            .iter()
            .copied()
            .filter(|&o| o != h)
            .filter(|&o| self.unit(o).is_some_and(|v| !v.dead && !v.hidden && v.owner == owner && v.extra.gold_run.as_ref().is_some_and(|r| r.entered) && (v.x - x).hypot(v.y - y) <= 0.1 + c + v.collision))
            .collect();
        for o in near {
            let Some((ox, oy)) = self.unit(o).map(|v| (v.x, v.y)) else { continue };
            let (nx, ny) = crate::natives::units::shown_at(self, o);
            if (nx, ny) == (ox, oy) {
                continue;
            }
            let cell = |w: f32, o: f32| ef::div(ef::sub(w.to_bits(), o.to_bits()), crate::pathing::CELL.to_bits());
            let (cx, cy) = (cell(nx, origin[0]), cell(ny, origin[1]));
            if let Some(v) = self.unit_mut(o) {
                v.x = nx;
                v.y = ny;
                v.extra.cell_at = Some((cx, cy, nx, ny));
                if let Some(m) = v.extra.mover.as_mut() {
                    m.x = cx;
                    m.y = cy;
                }
            }
        }
    }

    /// At its tree as its move ends (`FUN_6f4377d0`, event 0xd0184): within range but
    /// not yet free to chop, it walks to reach the tree (`REACH`, its `^clg` at once),
    /// its ghosting ending with that walk's tasks (0xd0151: `World::push_off_ghosts`).
    /// The move's target task, then the new block and task, go as the step ends; the
    /// move's path in the next step (read: at turns 853 and 990, the reach's tasks freed
    /// in the step it began).
    fn reach_tree(&mut self, h: u32, mut run: GoldRun, path: [u32; 2]) {
        let walked = self.unit(h).map_or(0, |u| u.extra.walk_serial);
        let serial = self.tags.born;
        let reach: Vec<u32> = REACH.iter().map(|c| self.tags.take(c)).collect();
        self.tags.release(reach[3]);
        let mv = match run.kind {
            Kind::Player => 9,
            _ => 8,
        };
        self.tags.release_in_step(run.order[mv]);
        self.tags.release_in_step(reach[2]);
        self.tags.release_in_step(reach[1]);
        self.tags.walk_ended(path, walked);
        self.push_off_ghosts(h);
        run.entered = true;
        run.phase = Phase::Reaching { task: reach[0], path: [reach[5], reach[4]], serial };
        if let Some(u) = self.unit_mut(h) {
            u.extra.gold_run = Some(run);
            u.extra.walk_serial = serial;
        }
    }

    /// Its walk to reach the tree ends in the step after it began, the tree within
    /// range (event 0xd0184 again, now free to chop: `FUN_6f436400`): its target task
    /// goes as the step ends and its path in the next; it starts to swing at the agent
    /// queue's clock (read: the reach's task and path freed at turns 854 and 990).
    fn tree_reached(&mut self, h: u32, k: usize) {
        let Some(mut run) = self.unit(h).and_then(|u| u.extra.gold_run.clone()) else { return };
        let Phase::Reaching { task, path, serial } = run.phase else { return };
        self.tags.release_in_step(task);
        self.tags.walk_ended(path, serial);
        let agent = self.step_agent.get(k).copied().unwrap_or(self.agent_step);
        run.phase = Phase::Chopping(self.chopped_at(h, agent));
        if let Some(u) = self.unit_mut(h) {
            u.extra.gold_run = Some(run);
        }
    }

    /// When a worker that starts to swing at `start` (the agent queue's clock) has its
    /// load: each swing chops at its damage point (`FUN_6f436a90`, event 0xd01b1), its
    /// backswing (at least 0.01) runs out (`FUN_6f405110`, 0xd01b2), and the next swing
    /// starts once Ahar's Dur less both has passed (`FUN_6f40bce0`, 0xd01be, at least
    /// 0.01), each by the engine's arithmetic; full, it stops as its last backswing runs
    /// out (`FUN_6f436ca0`).  The times are the unit's weapon against trees (a peon's
    /// 0.5 and 0.5, Dur 1.1: 10.9 seconds for ten chops).
    fn chopped_at(&self, h: u32, start: u32) -> u32 {
        let Some(u) = self.unit(h) else { return start };
        let (point, back) = (1..=2)
            .filter_map(|n| crate::sim::weapon_n(self, u.type_id, n))
            .find(|w| w.targets.contains("tree"))
            .map_or((0.5, 0.5), |w| (w.damage_point, w.backswing));
        let (dur, per, cap) = match self.ability_with_code(h, "Ahar") {
            Some((a, l)) => (self.ability_real(a, "Dur", l), self.ability_real(a, "DataA", l) as i32, self.ability_real(a, "DataB", l) as i32 + self.upgrade_bonus(u.owner, u.type_id).lumber),
            None => (1.1, 1, 10),
        };
        let chops = ((cap - u.carry_lumber).max(1) + per.max(1) - 1) / per.max(1);
        chops_end(start, point.to_bits(), back.to_bits(), dur.to_bits(), chops as u32)
    }

    /// Its load full (`h`'s due on the agent queue): the lumber it chopped taken from
    /// the tree, and its order home given as from a mine (`RETURN`: its `^clg` at once,
    /// the harvest order's rest and its own five with the tick's orders' objects),
    /// walking from step `k` (read: turn 963, the harvest order's 8 to 2 and 0, then
    /// the order home's 12 to 8).
    pub fn leave_tree(&mut self, h: u32, k: usize) {
        let Some(mut run) = self.unit(h).and_then(|u| u.extra.gold_run.clone()) else { return };
        let (per, cap) = match self.ability_with_code(h, "Ahar") {
            Some((a, l)) => (self.ability_real(a, "DataA", l) as i32, self.ability_real(a, "DataB", l) as i32),
            None => (1, 10),
        };
        let cap = cap + self.unit(h).map_or(0, |u| self.upgrade_bonus(u.owner, u.type_id).lumber);
        let taken = cap - self.unit(h).map_or(0, |u| u.carry_lumber);
        if let Some(Obj::Destructable(d)) = self.handles.get_mut(run.mine) {
            d.life -= taken as f32 * per.signum() as f32;
        }
        let serial = self.tags.born;
        let order: Vec<u32> = RETURN.iter().map(|c| self.tags.take(c)).collect();
        self.tags.release(order[13]);
        let rest: &[usize] = match run.kind {
            Kind::Player => &[8, 7, 6, 5, 4, 3, 2, 0],
            _ => &[7, 6, 5, 4, 3, 2, 1, 0],
        };
        for &i in rest {
            self.tags.issued.push(run.order[i]);
        }
        for i in [12, 11, 10, 9, 8] {
            self.tags.issued.push(order[i]);
        }
        if let Some(u) = self.unit_mut(h) {
            u.carry_gold = 0;
            u.carry_lumber = cap;
        }
        let hall = self.drop_off(h).unwrap_or(0);
        run.phase = Phase::ToHall(hall);
        run.kind = Kind::Return;
        run.ghosted = true;
        run.at_mine = false;
        run.order = order;
        if let Some(u) = self.unit_mut(h) {
            u.extra.gold_run = Some(run);
            u.extra.walk_from = Some(k);
            u.extra.walk_serial = serial;
            u.path.clear();
            u.order = Order { id: crate::natives::basic::order_id("resumeharvesting"), target_x: 0.0, target_y: 0.0, target: 0 };
        }
    }

    /// A gold mine's time inside (Agld DataB) and the workers it holds at once (DataC).
    fn mine_time_and_room(&self, mine: u32) -> (f32, u32) {
        match self.ability_with_code(mine, "Agld") {
            Some((m, ml)) => (self.ability_real(m, "DataB", ml), self.ability_real(m, "DataC", ml).max(1.0) as u32),
            None => (1.0, 1),
        }
    }

    /// Into its mine, out of sight, until the agent queue's clock reaches `due`, with its
    /// load (Ahar DataC) taken from the mine's gold.
    fn into_mine(&mut self, h: u32, mut run: GoldRun, due: u32) {
        let mine = run.mine;
        let gold_cap = match self.ability_with_code(h, "Ahar") {
            Some((a, al)) => self.ability_real(a, "DataC", al) as i32,
            None => 10,
        };
        run.phase = Phase::InMine(due);
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

    /// The mine's timer is up (`h`'s due on the agent queue; `FUN_6f5eee60`): out where
    /// it went in, or at the nearest place clear of the units about it when one stands
    /// there (`FUN_6f699c00`'s placing; read: a peon out at the next cell but one when
    /// the next in the queue stood where it went in); the queue's first worker in, its
    /// time from this one's due (read: the agent clock at each let-in a whole second
    /// past the first entry's) and its `uref` going with the tick's orders' objects; and
    /// its order home given (`RETURN`: its `^clg` at once, the harvest order's tree and
    /// its own five with the tick's orders' objects), walking from step `k`.
    pub fn leave_gold_mine(&mut self, h: u32, k: usize) {
        let Some(mut run) = self.unit(h).and_then(|u| u.extra.gold_run.clone()) else { return };
        let mine = run.mine;
        let due = match run.phase {
            Phase::InMine(at) => at,
            _ => self.step_agent.get(k).copied().unwrap_or(self.agent_step),
        };
        if self.unit(h).is_some() {
            let (x, y) = crate::natives::units::shown_at(self, h);
            if let Some(u) = self.unit_mut(h) {
                u.x = x;
                u.y = y;
                u.inside = 0;
                u.hidden = false;
            }
        }
        if let Some(m) = self.unit_mut(mine) {
            m.inside = m.inside.saturating_sub(1);
        }
        self.let_in_next(mine, due);
        let now = self.step_clocks.get(k).map_or(self.time_exact, |&c| f32::from_bits(c) as f64);
        self.mine_still_queued(mine, now);
        let serial = self.tags.born;
        let order: Vec<u32> = RETURN.iter().map(|c| self.tags.take(c)).collect();
        self.tags.release(order[13]);
        let rest: &[usize] = match run.kind {
            Kind::Player => &[9, 8, 7, 6, 5, 4, 3, 2, 0],
            _ => &[8, 7, 6, 5, 4, 3, 2, 1, 0],
        };
        // The harvest order's tree goes with the new order's five, with the objects of
        // the orders given at the same tick (read: after a wanderer's order given at
        // that tick and before its eight).
        for &i in rest {
            self.tags.issued.push(run.order[i]);
        }
        for i in [12, 11, 10, 9, 8] {
            self.tags.issued.push(order[i]);
        }
        let hall = self.drop_off(h).unwrap_or(0);
        run.phase = Phase::ToHall(hall);
        run.kind = Kind::Return;
        run.ghosted = true;
        run.at_mine = false;
        run.order = order;
        if let Some(u) = self.unit_mut(h) {
            u.extra.gold_run = Some(run);
            u.extra.walk_from = Some(k);
            u.extra.walk_serial = serial;
            u.path.clear();
            u.order = Order { id: crate::natives::basic::order_id("resumeharvesting"), target_x: 0.0, target_y: 0.0, target: 0 };
        }
    }

    /// The gold taken out as a worker leaves with others still queued (`FUN_6f4382c0`,
    /// `FUN_6f5e64b0`): the mine's FloatModifier (`FUN_6f058120`, its `` `rcm ``) made,
    /// or its end put off, to a second on; it is freed ten seconds after its end (read:
    /// made at turn 602 and put off at 612 and 622, freed at turn 732), `now` the leave's
    /// step's clock.
    fn mine_still_queued(&mut self, mine: u32, now: f64) {
        let Some(m) = self.unit(mine).filter(|m| !m.extra.mine_queue.is_empty()) else { return };
        let slot = match m.extra.mine_busy {
            Some((s, end)) if now <= end + 10.0 => s,
            _ => self.tags.take(b"`rcm"),
        };
        self.tags.redoom_at(now + 1.0 + 10.0, slot);
        if let Some(m) = self.unit_mut(mine) {
            m.extra.mine_busy = Some((slot, now + 1.0));
        }
    }

    /// The first worker still waiting in a mine's queue goes in, its time from `from`
    /// (`FUN_6f5eed80`, `FUN_6f5f4810`).
    fn let_in_next(&mut self, mine: u32, from: u32) {
        loop {
            let Some(w) = self.unit_mut(mine).and_then(|m| (!m.extra.mine_queue.is_empty()).then(|| m.extra.mine_queue.remove(0))) else { return };
            let Some(run) = self.unit(w).filter(|u| !u.dead).and_then(|u| u.extra.gold_run.clone()).filter(|r| r.mine == mine) else { continue };
            let Phase::Waiting(uref) = run.phase else { continue };
            self.tags.issued.push(uref);
            let (dur, _) = self.mine_time_and_room(mine);
            self.into_mine(w, run, ef::add(from, dur.to_bits()));
            return;
        }
    }

    /// Home with the gold (`FUN_6f438880`): given, and its harvest order back to the mine
    /// at once (`HARVEST_OWN`, its `^clg` freed then); the order home's tree, then the new
    /// order's five, go as the step ends in the walkers' order; the walk home's path as
    /// the next step ends.
    fn drop_gold(&mut self, h: u32, run: GoldRun, _hall: u32, path: [u32; 2]) {
        let (owner, gold, lumber) = self.unit(h).map_or((0, 0, 0), |u| (u.owner, u.carry_gold, u.carry_lumber));
        if gold > 0 || lumber > 0 {
            self.gather(owner, h, gold, lumber, 0.0);
        }
        if let Some(u) = self.unit_mut(h) {
            u.carry_gold = 0;
            u.carry_lumber = 0;
        }
        let codes: &[&Code] = if run.lumber { &LUMBER_OWN } else { &HARVEST_OWN };
        let next_serial = self.tags.born;
        let order: Vec<u32> = codes.iter().map(|c| self.tags.take(c)).collect();
        let n = order.len();
        self.tags.release(order[n - 3]);
        for i in [7, 6, 5, 4, 3, 2, 1, 0] {
            self.tags.release_in_step(run.order[i]);
        }
        for i in [n - 4, n - 5, n - 6, n - 7, n - 8] {
            self.tags.release_in_step(order[i]);
        }
        let serial = self.unit(h).map_or(0, |u| u.extra.walk_serial);
        self.tags.walk_ended(path, serial);
        let mut run = run;
        run.phase = Phase::ToMine;
        run.kind = Kind::Own;
        run.ghosted = true;
        run.at_mine = false;
        run.order = order;
        if let Some(u) = self.unit_mut(h) {
            u.extra.gold_run = Some(run);
            u.extra.walk_serial = next_serial;
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

    /// Whether a walker's walk to a structure ends in step `k` (its way's last point within
    /// reach, on the structure's bearing: the provider's answer 3), and if so its
    /// arrival, taken in the walkers' order (read: a hall arrival's order freed before a
    /// newer wanderer's walk end's objects, after an older one's).
    pub(crate) fn gold_run_step_end(&mut self, h: u32, k: usize) -> bool {
        if self.unit(h).and_then(|u| u.extra.gold_run.as_ref()).is_some_and(|r| matches!(r.phase, Phase::Reaching { .. })) {
            self.tree_reached(h, k);
            return true;
        }
        let Some(origin) = self.pathing.as_ref().map(|p| p.origin) else { return false };
        let Some(&clock) = self.step_clocks.get(k) else { return false };
        let cell = |w: f32, o: f32| ef::div(ef::sub(w.to_bits(), o.to_bits()), crate::pathing::CELL.to_bits());
        match self.tree_in_range(h, clock) {
            Some((m, true)) => {
                if let Some(u) = self.unit_mut(h) {
                    u.extra.walk_heading = Some(m.heading);
                    u.extra.mover = None;
                    u.extra.step_from = None;
                    u.path.clear();
                    crate::sim::set_anim(u, "stand");
                }
                self.gold_run_arrive(h, k);
                return true;
            }
            Some((m, false)) => {
                // Where it stopped is its place (as `World::walk_steps` keeps it).
                let (x, y) = m.at(clock);
                if let Some(u) = self.unit_mut(h) {
                    let was = (u.x, u.y);
                    u.x = f32::from_bits(ef::add(ef::mul(x, crate::pathing::CELL.to_bits()), origin[0].to_bits()));
                    u.y = f32::from_bits(ef::add(ef::mul(y, crate::pathing::CELL.to_bits()), origin[1].to_bits()));
                    u.extra.cell_at = Some((x, y, u.x, u.y));
                    if (u.x, u.y) != was {
                        u.extra.step_from = Some(was);
                    }
                    u.facing = f32::from_bits(m.heading).to_degrees();
                    u.extra.mover = Some(m);
                }
                return true;
            }
            None => {}
        }
        let ends = (|| {
            let u = self.unit(h)?;
            u.extra.gold_run.as_ref()?;
            let m = u.extra.mover?;
            let to = *u.path.first().filter(|_| u.path.len() == 1)?;
            let g = self.gold_run_goal(h)?;
            let turn = self.objects.units.get(u.type_id).map_or(0.5, |o| o.real("turnRate"));
            let gait = crate::motion::Gait { top: 0, grow: 0, turn: turn.to_bits(), window: 0, reach: 0.49f32.to_bits() };
            m.arrived(&gait, (cell(to.0, origin[0]), cell(to.1, origin[1])), (cell(g.0, origin[0]), cell(g.1, origin[1])), clock).then_some(m)
        })();
        let Some(m) = ends else { return false };
        if let Some(u) = self.unit_mut(h) {
            u.extra.walk_heading = Some(m.heading);
            u.extra.mover = None;
            u.extra.step_from = None;
            u.path.clear();
            crate::sim::set_anim(u, "stand");
        }
        self.gold_run_arrive(h, k);
        true
    }

    /// A walk to a tree at `clock`, the walker within the harvest's range of the tree's
    /// middle (Ahar Rng over 32 and the walker's radius, the tree's none): it stops there
    /// and turns toward the tree in that step, and arrives in the next, stopped and
    /// facing it (read: a peon 4.134 cells from its tree walking on, at 3.972 stopped on
    /// its bearing, Rng 116, its arrival's event 0xd0184 in the next step).  The walker
    /// as it is then, and whether it arrives.
    fn tree_in_range(&self, h: u32, clock: u32) -> Option<(crate::motion::Mover, bool)> {
        let u = self.unit(h)?;
        let r = u.extra.gold_run.as_ref().filter(|r| r.lumber && r.phase == Phase::ToMine)?;
        let mut m = u.extra.mover?;
        let origin = self.pathing.as_ref()?.origin;
        let cell = |w: f32, o: f32| ef::div(ef::sub(w.to_bits(), o.to_bits()), crate::pathing::CELL.to_bits());
        let (tx, ty) = self.widget_pos(r.mine)?;
        let tree = (cell(tx, origin[0]), cell(ty, origin[1]));
        let (a, l) = self.ability_with_code(h, "Ahar")?;
        let range = ef::add(ef::div(self.ability_real(a, "Rng", l).to_bits(), crate::pathing::CELL.to_bits()), ef::div(u.collision.to_bits(), crate::pathing::CELL.to_bits()));
        if !m.within(tree, range, clock) {
            return None;
        }
        let turn = self.objects.units.get(u.type_id).map_or(0.5, |o| o.real("turnRate"));
        let gait = crate::motion::Gait { top: 0, grow: 0, turn: turn.to_bits(), window: 0, reach: 0.49f32.to_bits() };
        if m.stopped_facing(&gait, tree, clock) {
            return Some((m, true));
        }
        m.halt_facing(&gait, tree, clock);
        Some((m, false))
    }

    /// A gold run's way to its structure (`World::local_path`'s search, 700 nodes deep,
    /// to the first node whose ring touches the structure's footprint, toward its
    /// middle).
    pub(crate) fn gold_run_search(&self, h: u32, cells: &impl crate::lrpath::Cells, start: (i32, i32), end: (i32, i32), class: u8) -> Option<(crate::lrpath::Outcome, bool, u32)> {
        let t = self.gold_run_target(h)?;
        let foot: std::collections::HashSet<(i32, i32)> = self.footprint(t).iter().copied().collect();
        log::debug!("gold run {h} searches from {start:?} to {end:?}, footprint {:?}", self.footprint(t));
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
        let mut run = GoldRun { mine: 0, lumber: false, phase: Phase::ToMine, kind: Kind::Player, order: Vec::new(), entered: false, ghosted: false, at_mine: false, timer_due: None, bonus: 0, travel: 0, bonus_before: 0, factor: 1f32.to_bits() };
        // As winedbg printed them, to six places.
        let speed = |r: &GoldRun| (f32::from_bits(ef::mul(190f32.to_bits(), r.factor)) as f64 / 32.0 * 1e6).round() / 1e6;
        run.tune(42.838799f32.to_bits(), 1f32.to_bits());
        assert_eq!(speed(&run), 5.9375);
        run.tune(47.908592f32.to_bits(), 1f32.to_bits());
        assert_eq!(speed(&run), 6.041100);
        run.tune(52.888390f32.to_bits(), 1f32.to_bits());
        assert_eq!(speed(&run), 6.025844);
        // One of five peons at a mine, its trips longer and shorter (read: its speeds
        // after, at its walks' steps).
        let mut run = GoldRun { factor: 1f32.to_bits(), ..run };
        (run.timer_due, run.bonus, run.travel, run.bonus_before) = (None, 0, 0, 0);
        let speeds: Vec<f64> = [51.688438f32, 56.668236, 61.828026, 66.957817].iter().map(|c| { run.tune(c.to_bits(), 1f32.to_bits()); speed(&run) }).collect();
        assert_eq!(speeds, [5.9375, 5.907512, 6.025506, 6.182261]);
    }

    /// A Booty Bay peon's trips at its tree (the agent queue's clocks read under winedbg
    /// at `FUN_6f436400` and `FUN_6f436ca0`): ten chops, its weapon's 0.5 and 0.5, Ahar's
    /// Dur 1.1.
    #[test]
    fn a_peons_chopping_ends_as_the_originals() {
        let end = |start: f32| {
            let t = chops_end(start.to_bits(), 0.5f32.to_bits(), 0.5f32.to_bits(), 1.1f32.to_bits(), 10);
            format!("{:.6}", f32::from_bits(t))
        };
        assert_eq!(end(212.181900), "223.081818");
        assert_eq!(end(225.831345), "236.731262");
        assert_eq!(end(239.390793), "250.290710");
    }
}
