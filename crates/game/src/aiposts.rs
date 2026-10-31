//! A computer player's guard posts (CGuardPost): AddGuardPost names a unit type to
//! keep at a point.  A new unit of that type that is not a Hero takes the first empty
//! post of its type whose replacements (SetReplacementCount) are not used up, leaves
//! its captain, and attack-moves there (FUN_6f9c9d60, FUN_6f9bfc90).  FillGuardPosts
//! asks for a unit for every such empty post (FUN_6f9c5ef0); ReturnGuardPosts sends
//! idle guards more than 50 from their posts back (FUN_6f9ca090, FUN_6f9c0b00).

use crate::objects::Id;
use crate::types::*;
use crate::world::World;
use crate::codes;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct GuardPost {
    pub id: Id,
    pub x: f32,
    pub y: f32,
    /// The unit keeping the post.
    pub unit: u32,
    /// Units assigned to the post so far.
    pub filled: i32,
}

impl World {
    fn post_empty(&self, post: &GuardPost) -> bool {
        self.unit(post.unit).is_none_or(|u| u.dead)
    }

    /// AddGuardPost (FUN_6f9c2ee0): a post, unless an empty one of that type is
    /// already at that point.
    pub(crate) fn add_guard_post(&mut self, p: u8, id: Id, x: f32, y: f32) {
        let exists = self.ai.get(&p).is_some_and(|s| s.posts.iter().any(|g| g.id == id && g.x == x && g.y == y && self.post_empty(g)));
        if !exists {
            if let Some(s) = self.ai.get_mut(&p) {
                s.posts.push(GuardPost { id, x, y, unit: 0, filled: 0 });
            }
        }
    }

    /// A campaign AI's units each keep a guard post of their own where they stand
    /// (FUN_6f9c3660 and FUN_6f9c72d0 give every unit the AI takes up a post at its place,
    /// heroes, structures, workers, and summoned units aside; measured: Human02's Orcs
    /// count none of their headhunters and grunts as IgnoredUnits from the start, and
    /// train a headhunter to fill the post the defender they take leaves).  Units the AI
    /// trains later take empty posts instead (it trains no more than it lost).
    pub(crate) fn post_campaign_units(&mut self, p: u8) {
        let units: Vec<(u32, Id, f32, f32)> = self
            .unit_order
            .iter()
            .filter_map(|&h| self.unit(h).map(|u| (h, u)))
            .filter(|(h, u)| u.owner == p && !u.dead && !u.is_building && !u.is_hero && !self.is_summoned(*h))
            .filter(|(_, u)| !self.objects.units.get(u.type_id).is_some_and(|o| o.list("type").iter().any(|t| t.trim().eq_ignore_ascii_case("peon"))))
            .map(|(h, u)| (h, u.type_id, u.x, u.y))
            .collect();
        let Some(st) = self.ai.get_mut(&p) else { return };
        for (h, id, x, y) in units {
            if st.posts.iter().any(|g| g.unit == h) {
                continue;
            }
            st.posts.push(GuardPost { id, x, y, unit: h, filled: 1 });
            st.attack.units.retain(|&k| k != h);
            st.defense.units.retain(|&k| k != h);
        }
    }

    /// A computer player's new unit takes an empty guard post of its type.
    pub(crate) fn ai_new_unit(&mut self, h: u32) {
        let Some(u) = self.unit(h) else { return };
        let (p, type_id) = (u.owner, u.type_id);
        if u.is_building || u.is_hero && u.extra.illusion.is_none() || self.objects.units.get(type_id).is_some_and(|o| o.list("type").iter().any(|t| t.trim().eq_ignore_ascii_case("peon"))) {
            return;
        }
        let Some(st) = self.ai.get(&p) else { return };
        let limit = st.replacements;
        let Some(i) = st.posts.iter().position(|g| g.id == type_id && self.post_empty(g) && g.filled <= limit) else { return };
        let Some(st) = self.ai.get_mut(&p) else { return };
        let post = &mut st.posts[i];
        post.unit = h;
        post.filled += 1;
        let (x, y) = (post.x, post.y);
        st.attack.units.retain(|&k| k != h);
        st.defense.units.retain(|&k| k != h);
        let attack = crate::natives::basic::order_id("attack");
        self.order_unit(h, attack, x, y, 0);
    }

    /// FillGuardPosts: a unit asked for each empty post whose replacements remain.
    pub(crate) fn fill_guard_posts(&mut self, p: u8) {
        let Some(st) = self.ai.get(&p) else { return };
        let limit = st.replacements;
        let wanted: Vec<Id> = st.posts.iter().filter(|g| self.post_empty(g) && g.filled <= limit).map(|g| g.id).collect();
        for id in wanted {
            self.ai_produce(p, id, -1);
        }
    }

    /// ReturnGuardPosts: idle guards more than 50 from their posts attack-move back.
    pub(crate) fn return_guard_posts(&mut self, p: u8) {
        let Some(st) = self.ai.get(&p) else { return };
        let away: Vec<(u32, f32, f32)> = st
            .posts
            .iter()
            .filter_map(|g| self.unit(g.unit).filter(|u| !u.dead && matches!(u.action, Action::Idle) && (u.x - g.x).hypot(u.y - g.y) > 50.0).map(|_| (g.unit, g.x, g.y)))
            .collect();
        let attack = crate::natives::basic::order_id("attack");
        for (h, x, y) in away {
            self.order_unit(h, attack, x, y, 0);
        }
    }

    /// Units keeping a guard post, which the captains leave alone.
    pub(crate) fn guards(&self, p: u8) -> Vec<u32> {
        self.ai.get(&p).map(|s| s.posts.iter().filter(|g| !self.post_empty(g)).map(|g| g.unit).collect()).unwrap_or_default()
    }
}

impl World {
    /// RemoveGuardPosition (FUN_6f9be910, FUN_6f9c9b30): a computer player's unit
    /// leaves its captain and stops, and its guard post is never filled again.
    pub fn remove_guard_position(&mut self, h: u32) {
        let Some(p) = self.unit(h).map(|u| u.owner) else { return };
        let Some(st) = self.ai.get_mut(&p) else { return };
        st.attack.units.retain(|&k| k != h);
        st.defense.units.retain(|&k| k != h);
        for g in st.posts.iter_mut().filter(|g| g.unit == h) {
            g.unit = 0;
            g.filled = 99999;
        }
        self.order_unit(h, crate::natives::basic::order_id("stop"), 0.0, 0.0, 0);
    }

    /// RecycleGuardPosition (FUN_6f9bde20): as RemoveGuardPosition, then the unit is
    /// taken in afresh and may keep another post.
    pub fn recycle_guard_position(&mut self, h: u32) {
        let joined = self.unit(h).is_some_and(|u| self.ai.contains_key(&u.owner));
        if joined {
            self.recycle_objects(h);
        }
        self.remove_guard_position(h);
        self.ai_new_unit(h);
        if joined {
            self.captain_walk(h);
        }
    }

    /// A computer's unit taken in walks to its captain's home by the steps from the
    /// game's first (`FUN_6f9d87d0`).
    fn captain_walk(&mut self, h: u32) {
        // Its goal the defense captain's home, midway between its town hall and gold
        // mine; within 300 of it, it only turns to face its way's first point, done once
        // within 0.2 of its bearing, its walk's objects kept until its next order (read
        // in single-player saves: the undead start's ghoul, 101 from home, turning 0.5 a
        // step six times to 1.712389, 0.116 off its way's first point's 1.5958, from 0.2
        // on, never moving until 1.1; provisional: the radius, the captains' own for
        // being home, and the point faced).
        let owner = self.unit(h).map_or(0, |u| u.owner);
        let home = self.captain_homes(owner).1;
        let near = self.unit(h).is_some_and(|u| (u.x - home.0).hypot(u.y - home.1) <= 300.0);
        let serial = self.tags.born;
        if let Some(u) = self.unit_mut(h) {
            u.extra.captain_walk = Some(home);
        }
        let face = if near { self.local_path(h, home).and_then(|(way, _, _)| way.first().copied()).unwrap_or(home) } else { home };
        if let Some(u) = self.unit_mut(h) {
            u.extra.captain_walk = Some(home);
            u.extra.walk_serial = serial;
            u.extra.mover = None;
            u.path.clear();
            if near {
                u.path.push(face);
                u.extra.facing_goal = true;
            }
        }
    }

    /// The objects a computer's unit's recycling makes and frees (traced on Forestwalk,
    /// the undead start's ghoul, `FUN_6f9bde20`): RemoveGuardPosition's stop order
    /// (`+ord`, `tskA`, four `task`, `tskA`; `FUN_6f9be910`); its unit AI made anew
    /// (`FUN_6f9c3660`: `uref`, `AI-U`, a stop order as above, `FUN_6f6803f0(1)`, a
    /// `uref` as it joins the AI's lists); its AI's order (`FUN_6f693450`: `+ord` and the
    /// idle unit's task, then the walk to its captain, `FUN_6f9d87d0`: `ordt`, `tskA`,
    /// `tskO`, three `task`, `tskt`, five `task`, `tskA`, its path's `^clg` freed at
    /// once, `^clu`, `l^pa`); and its abilities' AI (`FUN_6f67dff0`: its inventory's
    /// pickup range, `` `pil ``).  As the next turn starts: its old `uref` and `AI-U`, the
    /// first stop's (its `tskA`s, its tasks the last first, its `+ord`), its old
    /// `` `pil ``, the second stop's so, the walk's task and `+ord`, its last `tskA`,
    /// and its five tasks the last first.
    fn recycle_objects(&mut self, h: u32) {
        let stop: [&crate::tags::Code; 7] = [codes::class::ORDER, codes::class::TASK_A, codes::class::TASK, codes::class::TASK, codes::class::TASK, codes::class::TASK, codes::class::TASK_A];
        let first: Vec<u32> = stop.iter().map(|c| self.tags.take(c)).collect();
        let fresh = [self.tags.take(codes::class::UNIT_AI_REF), self.tags.take(codes::class::UNIT_AI)];
        let second: Vec<u32> = stop.iter().map(|c| self.tags.take(c)).collect();
        let listed = self.tags.take(codes::class::UNIT_AI_REF);
        let walk_codes: [&crate::tags::Code; 18] = [codes::class::ORDER, codes::class::TASK, codes::class::ORDER_TASKS, codes::class::TASK_A, codes::class::TASK_O, codes::class::TASK, codes::class::TASK, codes::class::TASK, codes::class::TASK_T, codes::class::TASK, codes::class::TASK, codes::class::TASK, codes::class::TASK, codes::class::TASK, codes::class::TASK_A, codes::class::CLUSTER_GOAL, codes::class::CLUSTER, codes::class::PATH];
        let walk: Vec<u32> = walk_codes.iter().map(|c| self.tags.take(c)).collect();
        self.tags.release(walk[15]);
        let pil = self.tags.take(codes::class::IN_RANGE_LISTENER);
        let live = |w: &World, (s, b): (u32, u32)| w.tags.live.get(&s).is_some_and(|e| e.1 == b);
        let (old_ai, old_pil) = self.unit(h).map(|u| (u.extra.ai_slots.clone(), u.extra.item_pil)).unwrap_or_default();
        let old_ai: Vec<u32> = old_ai.into_iter().filter(|&o| live(self, o)).map(|o| o.0).collect();
        for o in old_ai {
            self.tags.doom(o);
        }
        let stopped = |w: &mut World, s: &[u32]| {
            for k in [1, 6, 5, 4, 3, 2, 0] {
                w.tags.doom(s[k]);
            }
        };
        stopped(self, &first);
        if let Some(o) = old_pil.filter(|&o| live(self, o)) {
            self.tags.doom(o.0);
        }
        stopped(self, &second);
        for k in [1, 0, 14, 13, 12, 11, 10, 9] {
            self.tags.doom(walk[k]);
        }
        let births = |w: &World, s: u32| (s, w.tags.live.get(&s).map_or(0, |e| e.1));
        let ai = vec![births(self, fresh[0]), births(self, fresh[1])];
        let item = births(self, pil);
        let base = if self.tick_clocks.1.is_empty() { 0.0 } else { self.tick_abs(0).unwrap_or(0.0) as f32 };
        if let Some(u) = self.unit_mut(h) {
            u.extra.ai_slots = ai;
            u.extra.item_pil = Some(item);
            u.extra.captain_order = walk;
            u.extra.captain_uref = Some(listed);
            u.extra.captain_check = Some(base + 1.0);
        }
    }

    /// The captains' looks at their units due by `now` on the main clock: one idle, not
    /// walking, is sent home again (event 0xd01c9 by an agent timer a second after it
    /// was taken in, `FUN_6f9d9020` through `FUN_6f9d4600`; read with call stacks under
    /// winedbg: Forestwalk's undead computer's ghoul at 1.0, its path query, `^clg`,
    /// `^clu`, `l^pa`, the `^clg` freed, a `^clp`; a move order of sixteen, its `^clg`
    /// freed and its `+ord` taking the slot; freed then its third, its walk's tree
    /// ninth back to third, its fourteenth to ninth, its walk's path; in single-player
    /// saves walking at 1.03 on its way's first point's bearing).  Provisional: the
    /// period after the first.
    pub(crate) fn captain_checks_to(&mut self, now: f64) {
        let due: Vec<u32> = self.unit_order.iter().copied().filter(|&h| self.unit(h).is_some_and(|u| !u.dead && u.extra.captain_check.is_some_and(|t| t as f64 <= now))).collect();
        for h in due {
            let Some(u) = self.unit_mut(h) else { continue };
            let at = u.extra.captain_check.unwrap_or(0.0);
            u.extra.captain_check = Some(at + 1.0);
            let idle = matches!(u.action, Action::Idle) && u.extra.captain_walk.is_none() && u.extra.captain_order.len() == 18;
            if !idle {
                continue;
            }
            let old = std::mem::take(&mut u.extra.captain_order);
            let query: Vec<u32> = [codes::class::CLUSTER_GOAL, codes::class::CLUSTER, codes::class::PATH].iter().map(|c| self.tags.take(c)).collect();
            self.tags.release(query[0]);
            let clp = self.tags.take(b"^clp");
            const MOVE: [&crate::tags::Code; 16] = [codes::class::CLUSTER_GOAL, codes::class::ORDER_TASKS, codes::class::TASK_A, codes::class::TASK_A, codes::class::TASK_O, codes::class::TASK, codes::class::TASK, b"tsk.", codes::class::TASK, codes::class::TASK, codes::class::TASK, codes::class::TASK, codes::class::TASK, codes::class::TASK_A, codes::class::CLUSTER, codes::class::PATH];
            let order: Vec<u32> = MOVE.iter().map(|c| self.tags.take(c)).collect();
            self.tags.release(order[0]);
            let ord = self.tags.take(codes::class::ORDER);
            self.tags.release(order[2]);
            for k in [8, 7, 6, 5, 4, 3, 2] {
                self.tags.release(old[k]);
            }
            for k in [13, 12, 11, 10, 9, 8] {
                self.tags.release(order[k]);
            }
            for k in [17, 16] {
                self.tags.release(old[k]);
            }
            let owner = self.unit(h).map_or(0, |u| u.owner);
            let home = self.captain_homes(owner).1;
            let serial = self.tags.born;
            if let Some(u) = self.unit_mut(h) {
                let mut kept = vec![ord];
                kept.extend([1, 3, 4, 5, 6, 7, 14, 15].map(|k| order[k]));
                kept.extend([query[1], query[2], clp]);
                u.extra.captain_order = kept;
                u.extra.captain_walk = Some(home);
                u.extra.walk_serial = serial;
                u.extra.mover = None;
                u.extra.facing_goal = false;
                u.path.clear();
            }
        }
    }

    /// RemoveAllGuardPositions (FUN_6f9c9a60): the computer player's guards leave
    /// their captains and every post is gone.
    pub fn remove_all_guard_positions(&mut self, p: u8) {
        let Some(st) = self.ai.get_mut(&p) else { return };
        let guards: Vec<u32> = st.posts.iter().map(|g| g.unit).filter(|&u| u != 0).collect();
        st.attack.units.retain(|k| !guards.contains(k));
        st.defense.units.retain(|k| !guards.contains(k));
        st.posts.clear();
    }
}
