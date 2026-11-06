//! A structure's upgrade into a type of its Upgrade list, as the original's engine
//! carries it out: the order goes to the structure's Upgrade ability (`Aupg`,
//! CAbilityUpgrade's vf3, 0x6f5a5990, the target among the type's list), which gives it
//! an Upgrade In Progress (`AUnP`, `FUN_6f56c660`, made with its real) and starts it
//! (CAbilityUpgradeInProgress's vf212, 0x6f59cf30): the cost paid, the type changed at
//! once (`FUN_6f670950`, the unit's id changed with its tech tree, abilities, and
//! sight), and its rise begun (`FUN_6f41b790`).  It is done on the main clock at the new
//! type's build time on (vf205, 0x6f5a75f0), its objects going and the new type made in
//! its owner's tech tree (`CAbilityInProgress`'s vf205, 0x6f427f10).

use crate::codes;
use crate::event_ids::*;
use crate::objects::Id;
use crate::types::*;
use crate::world::{Request, World};
use w3_jass::Vm;

impl World {
    /// Starts upgrading a structure into another type from its Upgrade list.
    pub fn upgrade_structure(&mut self, h: u32, id: Id) -> Result<(), &'static str> {
        self.upgrade_structure_by(h, id, false)
    }

    /// `upgrade_structure`, by a player's order (`player`) or otherwise.
    ///
    /// A player's order makes an order and a task (`+ord`, `tskA`) as it is carried out,
    /// with the turn's actions, then the Upgrade In Progress and its real, the change's
    /// sight modifier, and the rise's modifier and threshold listener (`FUN_6f436b00`,
    /// event 0xd01c4).  The task, what the change lets go, and the order go in that
    /// order as the next turn's first step begins, after the turn's timers due before
    /// (traced: Mafarazzo TD's host player's Tower of Vengeance upgraded at turn 21 and
    /// again at 200, over LAN with the original: the order's task and order freed after
    /// the leaderboard trigger's objects at 2.0, the second's around its Upgrade
    /// ability's real and itself).  (Provisional: an order by script or by a computer's
    /// AI, none of its own objects here.)
    pub fn upgrade_structure_by(&mut self, h: u32, id: Id, player: bool) -> Result<(), &'static str> {
        let Some(u) = self.unit(h) else { return Err("") };
        if u.dead || u.constructing.is_some() || !u.queue.is_empty() || u.extra.upgrading.is_some() {
            return Err("");
        }
        let (owner, from) = (u.owner, u.type_id);
        // The target is one of the type's (CAbilityUpgrade's vf200, 0x6f582970).
        let id_s = crate::objects::id_str(id);
        if !self.objects.units.get(from).is_some_and(|o| o.list("Upgrade").iter().any(|x| x.trim() == id_s)) {
            return Err("");
        }
        if !self.missing_requirements(owner, id, 1).is_empty() || !self.tech_allowed(owner, id, 1) {
            return Err("");
        }
        // The new type's whole cost (measured: 6000 for Super Tower of Vengence, Red's
        // gold 30150 to 24150 as the order was carried out; vf212 takes the old's back
        // unless RelativeUpgradeCost reads as set, provisional, and the engine's read of
        // UpgradeInProgressIdChange, 0 in MiscGame.txt, behaves as set below.)
        let (g, l, _) = self.cost(id);
        self.spend(owner, id, false)?;
        // On the main clock as the turn's actions are carried out (measured: the type
        // changed at 2.0000 for the order of turn 21, at 19.8977 for that of turn 200).
        let at = self.actions_clock();
        log::debug!("t={:.1} upgrade to {id_s} at main clock {at:.6}", self.time);
        let order = player.then(|| (self.tags.take(codes::class::ORDER), self.tags.take(codes::class::TASK_A)));
        if let Some((_, task)) = order {
            self.tags.doom_at(at, task);
        }
        let mut objects: Vec<u32> = [codes::ability::id::UPGRADE_IN_PROGRESS, codes::class::REAL].iter().map(|c| self.tags.take(c)).collect();
        self.upgrade_type_change(h, from, id, at);
        objects.extend([codes::class::CONTINUOUS_MODIFIER, codes::class::THRESHOLD_LISTENER].iter().map(|c| self.tags.take(c)));
        if let Some((order, _)) = order {
            self.tags.doom_at(at, order);
        }
        // Done at the build time on, on the main clock (measured: the tech tree's
        // finish at 17.0000 and 34.8977 for builds of 15 begun at 2.0000 and 19.8977).
        let time = self.objects.units.get(id).map_or(1.0, |o| o.real("bldtm")).max(0.01);
        let due = f32::from_bits(crate::efloat::add((at as f32).to_bits(), time.to_bits()));
        if let Some(u) = self.unit_mut(h) {
            u.extra.upgrading = Some(Upgrading { from, to: id, due, time, objects, paid: (g, l) });
        }
        // (Provisional: its event's data agent, `FUN_6f67da80` with event 0x80242, made
        // when something listens, not yet; when its triggers run.)
        self.queue_unit_event(h, EVENT_PLAYER_UNIT_UPGRADE_START, EVENT_UNIT_UPGRADE_START, EventCtx::default());
        Ok(())
    }

    /// The main clock as a turn's actions are carried out: its last main tick's.
    pub(crate) fn actions_clock(&self) -> f64 {
        self.tick_clocks.1.len().checked_sub(1).and_then(|i| self.tick_abs(i)).unwrap_or(self.time_exact)
    }

    /// The unit's change of type as its upgrade starts (`FUN_6f670950` with 0x101): in
    /// its owner's tech tree the old type taken from present and the new one begun
    /// (`FUN_6f6703b0`; measured: e000 to e001 at 2.0, the tech tree's start of e001
    /// then), its type's stats and engine abilities (its list's at the upgrade's end,
    /// `World::replace_type_abilities`), the engine's Upgrade ability let go when the
    /// new type upgrades to nothing (its real, then itself, freed with the order's
    /// objects: traced, e001 to e002), and its sight's modifier over 3 seconds, as a
    /// change of day makes (`FUN_6f6708a0`, the unit's +0x110; traced: freed 3 seconds
    /// on).
    fn upgrade_type_change(&mut self, h: u32, from: Id, to: Id, at: f64) {
        let Some(owner) = self.unit(h).map(|u| u.owner) else { return };
        self.tech_reg(owner, from).present -= 1;
        self.tech_changed(owner);
        self.tech_reg(owner, to).producing += 1;
        self.tech_changed(owner);
        self.transform_unit(h, to);
        let upgrades = self.objects.units.get(to).is_some_and(|o| o.list("Upgrade").iter().any(|s| self.objects.units.get(crate::objects::id_from_str(s.trim())).is_some()));
        if !upgrades {
            let k = self.unit(h).and_then(|u| u.extra.object_slots[2].iter().position(|&(s, b)| self.tags.live.get(&s).is_some_and(|e| e.0 == *codes::ability::id::UPGRADE_STRUCTURE && e.1 == b)));
            let gone: Vec<(u32, u32)> = match (k, self.unit_mut(h)) {
                (Some(k), Some(u)) if k + 1 < u.extra.object_slots[2].len() => u.extra.object_slots[2].drain(k..k + 2).collect(),
                _ => Vec::new(),
            };
            for &(s, _) in gone.iter().rev() {
                self.tags.doom_at(at, s);
            }
        }
        let s = self.tags.take(codes::class::CONTINUOUS_MODIFIER);
        self.tags.doom_at(at + 3.0, s);
        let birth = self.tags.live.get(&s).map_or(0, |e| e.1);
        if let Some(u) = self.unit_mut(h) {
            u.extra.day_rcm = Some(s);
            u.extra.day_ref = Some((s, birth));
            u.extra.sight_stamp = Some((at + 3.0) as f32);
        }
        self.retype_splashes(h, from, to);
    }

    /// The attack's splash objects as its weapons become the new type's (`FUN_6f49cab0`
    /// from the change of type, after its sight's modifier): for each of its two weapons
    /// one made as its kind becomes a splashing one (artillery, msplash, mbounce, mline,
    /// or aline; `FUN_6f4977e0`), kept while it stays one, let go at once as it becomes
    /// another (its vf 0x5c, then its last reference).  (Measured: Mafarazzo TD's Arrow
    /// Tower, a missile's, becoming the Cannon Tower, an artillery's, made one more
    /// object as its upgrade began, turn 212 of the maft3 game; provisional: the
    /// order, a let go one's.)
    fn retype_splashes(&mut self, h: u32, from: Id, to: Id) {
        let splash = |t: &str| matches!(t.trim(), "msplash" | "artillery" | "aline" | "mline" | "mbounce");
        let kinds = |id: Id| self.objects.units.get(id).map(|o| [o.str("weapTp1"), o.str("weapTp2")]).unwrap_or_default();
        let (was, now) = (kinds(from), kinds(to));
        let splashes = |w: &World| w.unit(h).map(|u| u.extra.object_slots[2].iter().enumerate().filter(|(_, o)| w.tags.live.get(&o.0).is_some_and(|e| &e.0 == codes::class::SPLASH && e.1 == o.1)).map(|(k, _)| k).collect::<Vec<usize>>()).unwrap_or_default();
        // Its splash objects stand after its attack, the first weapon's first.
        let mut held = 0;
        for k in 0..2 {
            match (splash(&was[k]), splash(&now[k])) {
                (true, false) => {
                    let at = splashes(self).get(held).copied();
                    if let Some((s, _)) = at.and_then(|i| self.unit_mut(h).map(|u| u.extra.object_slots[2].remove(i))) {
                        self.tags.release(s);
                    }
                }
                (false, true) => {
                    let after = splashes(self).get(held.wrapping_sub(1)).copied().or_else(|| self.unit(h).and_then(|u| u.extra.object_slots[2].iter().position(|&(s, b)| self.tags.live.get(&s).is_some_and(|e| &e.0 == codes::ability::id::ATTACK && e.1 == b))));
                    let made = self.tags.take_id(codes::class::SPLASH);
                    if let Some(u) = self.unit_mut(h) {
                        let k = after.map_or(u.extra.object_slots[2].len(), |i| i + 1);
                        u.extra.object_slots[2].insert(k, made);
                    }
                    held += 1;
                }
                (true, true) => held += 1,
                (false, false) => {}
            }
        }
    }

    /// The upgrades due on the main clock by main tick `tick` (by the turn's last with
    /// none), done as their step begins.
    pub(crate) fn upgrades_to(&mut self, vm: &mut Vm, tick: Option<i64>) {
        let at = match tick {
            Some(t) => self.tick_main(t),
            None => self.tick_clocks.1.len().checked_sub(1).and_then(|i| self.tick_abs(i)),
        };
        let Some(at) = at else { return };
        let due: Vec<u32> = self.unit_order.iter().copied().filter(|&h| self.unit(h).is_some_and(|u| !u.dead && u.extra.upgrading.as_ref().is_some_and(|g| g.due as f64 <= at + 1e-6))).collect();
        for h in due {
            self.complete_upgrade(vm, h);
        }
    }

    /// An upgrade done (CAbilityUpgradeInProgress's vf205, `FUN_6f5a75f0`): its
    /// abilities the new type's (`World::replace_type_abilities`), then its objects go
    /// at once, the rise's listener, its modifier, the real, and Upgrade In Progress
    /// (traced; `FUN_6f427f10`, by `FUN_6f41dd90` and `FUN_6f48e8a0`), the new type made
    /// in its owner's tech tree (`FUN_6f262f30`, `FUN_6f22f6a0`; measured at 17.0000),
    /// then its events (`FUN_6f67d960`, event 0x80244).
    fn complete_upgrade(&mut self, vm: &mut Vm, h: u32) {
        let Some(to) = self.unit(h).and_then(|u| u.extra.upgrading.as_ref()).map(|g| g.to) else { return };
        self.replace_type_abilities(h, to);
        let Some(u) = self.unit_mut(h) else { return };
        let Some(g) = u.extra.upgrading.take() else { return };
        let owner = u.owner;
        if g.objects.len() == 4 {
            for k in [3, 2, 1, 0] {
                self.tags.release(g.objects[k]);
            }
        }
        self.tech_built(owner, g.to);
        let sound = format!("UpgradeComplete{}", self.race_label(owner));
        self.requests.push(Request::Completed { unit: h, id: g.to, level: 1, sound });
        self.fire_unit_pair(vm, h, EVENT_PLAYER_UNIT_UPGRADE_FINISH, EVENT_UNIT_UPGRADE_FINISH, EventCtx::default());
    }

    /// An upgrade cancelled (CAbilityUpgradeInProgress's vf206): UpgradeRefundRate of
    /// what was paid back, and the old type again (`FUN_6f670950` with 0x102: the new
    /// type's production cancelled, the old made and present).  (Provisional: its
    /// objects' order, as its end's; the Upgrade ability not made again.)
    pub(crate) fn cancel_upgrade(&mut self, h: u32) {
        let Some(u) = self.unit_mut(h) else { return };
        let Some(g) = u.extra.upgrading.take() else { return };
        let owner = u.owner;
        if g.objects.len() == 4 {
            for k in [3, 2, 1, 0] {
                self.tags.release(g.objects[k]);
            }
        }
        let rate = self.misc_f32("UpgradeRefundRate", 0.75);
        if let Some(p) = self.players.get_mut(owner as usize) {
            *p.state.entry(player_state::RESOURCE_GOLD).or_insert(0) += (g.paid.0 as f32 * rate) as i32;
            *p.state.entry(player_state::RESOURCE_LUMBER).or_insert(0) += (g.paid.1 as f32 * rate) as i32;
        }
        self.tech_reg(owner, g.to).producing -= 1;
        self.tech_changed(owner);
        self.tech_add(owner, g.from);
        self.transform_unit(h, g.from);
        self.queue_unit_event(h, EVENT_PLAYER_UNIT_UPGRADE_CANCEL, EVENT_UNIT_UPGRADE_CANCEL, EventCtx::default());
    }
}
