//! Creeps keep to their camps, and units call their friends to a fight (MiscGame):
//! a creep that has strayed GuardDistance from home and chased GuardReturnTime
//! seconds without being attacked heads home, as does one beyond MaxGuardDistance
//! after that time whatever attacks it, or one left idle away from home (measured: an
//! ogre, its camp's or one made by a script, chases a fleeing footman some 2000 from
//! home and turns back 5 to 6 seconds after passing 600).  A unit attacked calls the
//! idle friends within CallForHelp (CreepCallForHelp for creeps) to it.

use crate::types::*;
use crate::world::World;

impl World {
    pub fn update_guards(&mut self) {
        let (guard, max, wait) = (self.misc_f32("GuardDistance", 600.0), self.misc_f32("MaxGuardDistance", 1000.0), self.misc_f32("GuardReturnTime", 5.0));
        let now = self.time;
        let units: Vec<u32> = self.unit_order.iter().copied().filter(|&h| self.unit(h).is_some_and(|u| !u.dead && !u.hidden && u.extra.home.is_some())).collect();
        for h in units {
            let u = self.unit_mut(h).unwrap();
            if u.extra.home.is_some_and(|p| p.0.is_nan()) {
                u.extra.home = Some((u.x, u.y));
            }
            let (hx, hy) = u.extra.home.unwrap();
            let d = (u.x - hx).hypot(u.y - hy);
            let u = self.unit_mut(h).unwrap();
            if u.extra.returning {
                if !matches!(u.action, Action::Move { .. }) || d < 32.0 {
                    u.extra.returning = false;
                    u.extra.strayed = None;
                }
                continue;
            }
            if d <= guard {
                u.extra.strayed = None;
                u.extra.idle_away = None;
                continue;
            }
            let strayed = *u.extra.strayed.get_or_insert(now);
            let chasing = matches!(u.action, Action::Attack { .. });
            let idle = matches!(u.action, Action::Idle);
            // One standing idle away from home waits GuardReturnTime first (measured: an
            // ogre that fled 971 from home stands there five seconds, then goes back).
            let idle_since = if idle { *u.extra.idle_away.get_or_insert(now) } else {
                u.extra.idle_away = None;
                now
            };
            let go = idle && now - idle_since >= wait || chasing && now - strayed >= wait && (d > max || now - u.extra.last_attacked >= wait);
            if go {
                u.extra.returning = true;
                u.extra.idle_away = None;
                // Its order reads "move" on the way (measured).
                u.order = Order { id: crate::natives::basic::order_id("move"), target_x: hx, target_y: hy, target: 0 };
                u.action = Action::Move { x: hx, y: hy };
                u.path.clear();
                u.repath_timer = 0.0;
                u.swing = None;
            }
        }
    }

    /// The idle armed friends of an attacked unit within call-for-help range join in.
    pub(crate) fn call_for_help(&mut self, target: u32, attacker: u32) {
        let Some(t) = self.unit(target) else { return };
        let (owner, x, y) = (t.owner, t.x, t.y);
        if self.unit(attacker).is_none_or(|a| crate::natives::players::is_ally(self, owner, a.owner)) {
            return;
        }
        let key = if owner == PLAYER_NEUTRAL_AGGRESSIVE { "CreepCallForHelp" } else { "CallForHelp" };
        let range = self.misc_f32(key, 600.0);
        let friends: Vec<u32> = self
            .unit_order
            .iter()
            .copied()
            .filter(|&f| f != target)
            .filter(|&f| {
                self.unit(f).is_some_and(|u| {
                    u.owner == owner && !u.dead && !u.hidden && !u.is_building && !u.extra.returning && matches!(u.action, Action::Idle) && (u.x - x).hypot(u.y - y) <= range && !crate::sim::unit_weapons(self, u).is_empty()
                })
            })
            .collect();
        for f in friends {
            self.set_asleep(f, false);
            self.unit_mut(f).unwrap().action = Action::Attack { target: attacker };
            self.spell_ai(f, attacker, target);
        }
    }
}

impl World {
    /// Creep sleep (CAbilityCreepSleep): at night an idle creep at its camp that may
    /// sleep falls asleep, CreepSleepTarget over its head, unless its owner has
    /// PLAYER_STATE_NO_CREEP_SLEEP; one that sleeps always (UnitAddSleepPerm) sleeps
    /// whenever idle.  Day, damage, or a friend's call wakes it.  One falling asleep
    /// shows it (its effect) after a random wait (`fall_asleep`).
    pub fn update_creep_sleep(&mut self, dt: f32) {
        let showing: Vec<u32> = self.unit_order.iter().copied().filter(|&h| self.unit(h).is_some_and(|u| u.extra.sleeping && u.extra.sleep_show_wait.is_some())).collect();
        for h in showing {
            let u = self.unit_mut(h).unwrap();
            let left = u.extra.sleep_show_wait.unwrap_or(0.0) - dt;
            if left > 0.0 {
                u.extra.sleep_show_wait = Some(left);
            } else {
                u.extra.sleep_show_wait = None;
                self.show_asleep(h);
            }
        }
        let night = self.is_night();
        let units: Vec<u32> = self.unit_order.iter().copied().filter(|&h| self.unit(h).is_some_and(|u| !u.dead && !u.hidden && (u.extra.can_sleep || u.extra.sleep_perm))).collect();
        for h in units {
            let u = self.unit(h).unwrap();
            let allowed = self.players.get(u.owner as usize).is_none_or(|p| p.get_state(player_state::NO_CREEP_SLEEP) == 0);
            let idle = matches!(u.action, Action::Idle) && !u.extra.returning && u.swing.is_none();
            let want = idle && (u.extra.sleep_perm || night && allowed);
            if want && !u.extra.sleeping {
                self.fall_asleep(h);
            } else if u.extra.sleeping && !u.extra.sleep_perm && !(night && allowed) {
                self.set_asleep(h, false);
            }
        }
    }

    /// Night, when creeps sleep.
    pub(crate) fn is_night(&self) -> bool {
        !(6.0..18.0).contains(&self.time_of_day)
    }

    /// A newly made creep that may sleep falls asleep if it is night (the original's
    /// creep sleep checks as it starts; units made as the map starts, at midnight before
    /// the melee setup's 8:00, fall asleep and wake with the day).
    pub(crate) fn creep_made(&mut self, h: u32) {
        let Some(u) = self.unit(h) else { return };
        let allowed = self.players.get(u.owner as usize).is_none_or(|p| p.get_state(player_state::NO_CREEP_SLEEP) == 0);
        if u.extra.can_sleep && !u.extra.sleeping && self.is_night() && allowed {
            self.fall_asleep(h);
        }
    }

    /// A creep falls asleep at once and shows it after a random wait of up to three
    /// seconds, drawn on creep sleep's own generator (FUN_6f524e00, stream 37, times the
    /// 3.0 at 0x6fd3c750).
    pub(crate) fn fall_asleep(&mut self, h: u32) {
        if self.unit(h).is_none_or(|u| u.extra.sleeping) {
            return;
        }
        let wait = self.rand_data.streams[crate::rng::stream::CREEP_SLEEP].real(0.0, 3.0);
        let u = self.unit_mut(h).unwrap();
        u.extra.sleeping = true;
        u.extra.sleep_show_wait = Some(wait);
    }

    pub(crate) fn set_asleep(&mut self, h: u32, on: bool) {
        let Some(u) = self.unit_mut(h) else { return };
        if u.extra.sleeping == on {
            return;
        }
        u.extra.sleeping = on;
        u.extra.sleep_show_wait = None;
        let old = std::mem::take(&mut u.extra.sleep_effect);
        if old != 0 {
            self.handles.remove(old);
        }
        if on {
            self.show_asleep(h);
        }
    }

    /// The sleep effect over a sleeping unit's head.
    fn show_asleep(&mut self, h: u32) {
        if self.unit(h).is_some_and(|u| u.extra.sleeping && u.extra.sleep_effect == 0) {
            let art = self.objects.abilities.get(*b"ACsp").map(|o| (o.list("Casterart").first().cloned().unwrap_or_default(), o.list("Casterattach").first().cloned().unwrap_or_default()));
            if let Some((model, attach)) = art.filter(|a| !a.0.is_empty()) {
                let e = self.handles.insert(Obj::Effect(Effect { model, x: 0.0, y: 0.0, z: 0.0, target: Some((h, attach)), destroyed: false, facing: 0.0 }));
                if let Some(u) = self.unit_mut(h) {
                    u.extra.sleep_effect = e;
                }
            }
        }
    }
}

impl World {
    /// A computer player's unit hurt by an enemy may run from it instead of answering,
    /// when the player's AI lets its units (SetUnitsFlee) or its Heroes (SetHeroesFlee)
    /// flee (FUN_6f5fa400, FUN_6f607ba0).  An illusion, one disabled or moving already,
    /// or one whose acquisition range is under 100 stays; one without a weapon runs,
    /// and from an attacker without one none does.  Of their weapons' reaches at each
    /// other: an attacker not free to fight (disabled) is run from when the unit reaches
    /// farther, or when it is ensnared and not the unit's target.  The unit runs as its
    /// life falls into 600 to 610, 500 to 510, and so on to 100 to 110 (only 200 to 210
    /// and 100 to 110 for a unit reaching 200 or less hurt by one reaching 200 or more);
    /// a unit not a Hero below 150; a Hero in 400 to 410 or 300 to 310, from a Hero
    /// reaching under 200 that it outreaches past 200, below 200, or below a third of
    /// its most.  It runs 300 straight away (a Hero that low, or from an attacker
    /// without a weapon), or else the attacker's reach and 100 more (FUN_6f5fc350),
    /// ordered to move.  Measured: a computer's ghoul struck to 103 and to 47 in a
    /// creep camp is ordered to move, and comes back.
    pub(crate) fn ai_flees(&mut self, h: u32, attacker: u32) -> bool {
        let (Some(u), Some(a)) = (self.unit(h), self.unit(attacker)) else { return false };
        if u.dead || u.is_building || u.paused || u.extra.illusion.is_some() || crate::natives::players::is_ally(self, u.owner, a.owner) {
            return false;
        }
        let Some(st) = self.ai.get(&u.owner) else { return false };
        let flag = if u.is_hero { "heroes_flee" } else { "units_flee" };
        if !st.flags.get(flag).copied().unwrap_or(false) || u.order.id == crate::natives::basic::order_id("move") || self.buff_mods(h).4 {
            return false;
        }
        let mine = crate::sim::unit_weapons(self, u);
        let theirs = crate::sim::unit_weapons(self, a);
        let reach = |ws: &[(u8, crate::sim::Weapon)], from: &Unit, to: &Unit| ws.iter().find(|(_, w)| crate::sim::can_target(self, w, from, to)).map_or(0.0, |(_, w)| w.range);
        let (mine_r, theirs_r) = (reach(&mine, u, a), reach(&theirs, a, u));
        let life = u.life;
        let run = if mine.is_empty() {
            true
        } else if u.acquire_range < 100.0 || theirs.is_empty() {
            false
        } else {
            let attacker_free = !a.paused && !self.buff_mods(attacker).4;
            let ensnared = a.buffs.iter().any(|b| b.id == *b"Bens");
            let targeting = matches!(u.action, Action::Attack { target } if target == attacker);
            let band = |n: f32| life >= n && life <= n + 10.0;
            let bands: &[f32] = if theirs_r >= 200.0 && mine_r <= 200.0 { &[200.0, 100.0] } else { &[600.0, 500.0, 400.0, 300.0, 200.0, 100.0] };
            if !attacker_free && (mine_r > theirs_r || (!targeting && ensnared)) {
                true
            } else if bands.iter().any(|&n| band(n)) {
                true
            } else if !u.is_hero {
                life < 150.0
            } else {
                band(400.0) || band(300.0) || (a.is_hero && a.extra.illusion.is_none() && theirs_r < 200.0 && mine_r > 200.0) || life < 200.0 || life < u.max_life / 3.0
            }
        };
        if !run {
            return false;
        }
        let low_hero = u.is_hero && (life < 200.0 || life < u.max_life / 3.0);
        let dist = if low_hero || theirs.is_empty() { 300.0 } else { theirs_r + 100.0 };
        let (dx, dy) = (u.x - a.x, u.y - a.y);
        let d = dx.hypot(dy);
        let (nx, ny) = if d > 0.001 { (dx / d, dy / d) } else { (u.facing.to_radians().cos(), u.facing.to_radians().sin()) };
        let (fx, fy) = self.clamp_to_map(u.x + nx * dist, u.y + ny * dist);
        let u = self.unit_mut(h).unwrap();
        u.action = Action::Move { x: fx, y: fy };
        u.order = Order { id: crate::natives::basic::order_id("move"), target_x: fx, target_y: fy, target: 0 };
        u.path.clear();
        u.swing = None;
        true
    }
}
