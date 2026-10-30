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

    /// Night, when creeps sleep: the engine's day and night, which change 0.01 past
    /// dawn and dusk (`time_of_day_moved`).
    pub(crate) fn is_night(&self) -> bool {
        !self.daytime
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

impl World {
    /// The time of day moved from `old` to `new` (the clock, or SetFloatGameState): its
    /// events that became true fire, and at a change between day and night every unit
    /// takes it, the last made first, and sleeping creeps wake with the day.  Measured
    /// in the original's tags: each time-of-day event fired runs its trigger and makes a
    /// timer; the change makes two timers for day, one for night; then each unit a
    /// timer (`rcm), and a creep that wakes one for each spell it casts (Ensnare a
    /// region too), after its own.
    /// An event of the time of day fires: one for equal when the time passes its value
    /// (forward, through midnight too), the others when they become true as the clock
    /// runs (traced: setting the time from 0 to 8 fires Blizzard.j's dawn trigger, equal
    /// to 6, and not its day trigger, greater than or equal to 6).
    fn time_event_fires(op: i32, value: f32, old: f32, new: f32, set: bool) -> bool {
        const EQUAL: i32 = 2;
        if op == EQUAL {
            return if old <= new { old < value && value <= new } else { value > old || value <= new };
        }
        !set && crate::world::compare(op, new, value) && !crate::world::compare(op, old, value)
    }

    /// As the time runs each event is a listener on it (`FUN_6f27a110`): those for
    /// less than or at most fire their triggers as it falls past their values, those
    /// for at least or greater than as it rises past, one for equal as it rises past,
    /// replaced by one the other way before its trigger runs, and as it falls past,
    /// replaced only (`FUN_6f27c5c0`).  Through midnight the time rises to the day's
    /// end and falls back from there (its FloatModifier set back, event 0x80262), where
    /// the night's listener of the change between day and night, on its falling below
    /// dusk, is replaced by one on its rising past dawn, it still being night.  The
    /// listeners passed fire the latest made first; the old listeners and the
    /// triggers' executions go at the tick's end, in the order let go (read: at dusk the
    /// Night trigger's `+tex`, then the Dusk event's `` `rtl `` and `+tex`; at midnight
    /// the change's `` `rtl ``, Dusk's and Dawn's, then Day's and Night's `+tex`).
    fn time_listeners_crossed(&mut self, vm: &mut w3_jass::Vm, old: f32, new: f32) {
        use crate::types::EventKind;
        let hours = self.misc_f32("DayHours", 24.0);
        let wrapped = new < old;
        let rises = |v: f32| if wrapped { (old < v && v <= hours) || v <= new } else { old < v && v <= new };
        let falls = |v: f32| wrapped && new < v && v < hours;
        // (listener's making: re-made ones by when, the rest by their registration; what
        // it does)
        let mut passed: Vec<((u8, u64), Option<(u32, bool, bool)>)> = Vec::new();
        for &e in &self.events {
            let Some(crate::types::Obj::Event(r)) = self.handles.get(e) else { continue };
            let EventKind::GameState { state: 2, op, value } = r.kind else { continue };
            let (remake, trigger) = match op {
                2 => (rises(value) || falls(value), rises(value)),
                0 | 1 => (false, falls(value)),
                3 | 4 => (false, rises(value)),
                _ => (false, false),
            };
            if remake || trigger {
                let made = self.tags.listener_made.get(&e).map_or((0, e as u64), |&b| (1, b));
                passed.push((made, Some((e, remake, trigger))));
            }
        }
        let (dusk, eps) = (self.misc_f32("Dusk", 18.0), 0.01);
        if !self.daytime && falls(dusk - eps) {
            passed.push(((1, self.tags.day_listener_made), None));
        }
        if passed.is_empty() {
            return;
        }
        passed.sort_by(|a, b| b.0.cmp(&a.0));
        // (The orders' objects waiting for the tick's end keep their place.)
        let waiting = std::mem::take(&mut self.tags.issued);
        for (_, what) in passed {
            match what {
                Some((e, remake, trigger)) => {
                    if remake {
                        let s = self.tags.take(b"`rtl");
                        self.tags.listener_made.insert(e, self.tags.born as u64);
                        if let Some(old) = self.tags.event_timers.insert(e, s) {
                            self.tags.issued.push(old);
                        }
                    }
                    if trigger {
                        self.exec_at_tick_end = true;
                        self.fire_registrations_pub(vm, crate::types::EventCtx::default(), vec![e]);
                        self.exec_at_tick_end = false;
                    }
                }
                None => {
                    let s = self.tags.take(b"`rtl");
                    self.tags.day_listener_made = self.tags.born as u64;
                    if let Some(old) = self.tags.day_timer.replace(s) {
                        self.tags.issued.push(old);
                    }
                    if let Some(old) = self.tags.day_timer_2.take() {
                        self.tags.issued.push(old);
                    }
                }
            }
        }
        self.tags.flush_issued();
        self.tags.issued = waiting;
    }

    pub(crate) fn time_of_day_moved(&mut self, vm: &mut w3_jass::Vm, old: f32, new: f32, set: bool) {
        use crate::types::EventKind;
        let fires = |k: &EventKind| matches!(*k, EventKind::GameState { state: 2, op, value } if Self::time_event_fires(op, value, old, new, set));
        let fired: Vec<u32> = self.events.iter().copied().filter(|&e| matches!(self.handles.get(e), Some(crate::types::Obj::Event(r)) if fires(&r.kind))).collect();
        if set {
            self.fire(vm, crate::types::EventCtx::default(), |_, k| fires(k));
            // Each fired event's timer is replaced, the old one released (traced).
            for e in fired {
                let s = self.tags.take(b"`rtl");
                self.tags.listener_made.insert(e, self.tags.born as u64);
                if let Some(old) = self.tags.event_timers.insert(e, s) {
                    self.tags.doom(old);
                }
            }
        } else {
            let _ = fired;
            self.time_listeners_crossed(vm, old, new);
        }
        // Day and night change on listeners of their own, 0.01 past dawn and dusk
        // (`FUN_6f283c80`): by day, night comes as the time passes Dusk and 0.01 or
        // falls below Dawn less 0.01; by night, day comes as it passes Dawn and 0.01 or
        // falls below Dusk less 0.01 (read: the change at main clock 200.199966, the
        // dusk triggers at 200.0).
        let (dawn, dusk, eps) = (self.misc_f32("Dawn", 6.0), self.misc_f32("Dusk", 18.0), 0.01);
        let day = if self.daytime { !(new >= dusk + eps || new < dawn - eps) } else { new >= dawn + eps && new < dusk - eps };
        if day == self.daytime {
            return;
        }
        self.daytime = day;
        // The change's timers; the engine's day timer before them is released (traced:
        // slot 0x287 at Booty Bay's first day; which new one takes its place is not yet
        // measured, the first here).
        let first = self.tags.take(b"`rtl");
        self.tags.day_listener_made = self.tags.born as u64;
        let second = day.then(|| self.tags.take(b"`rtl"));
        // Both of the last change's go as it ends, before the step's walk ends (read:
        // the first dawn's two at nightfall; the night's at dawn, before a walk end's
        // tree in that step).
        let old: Vec<u32> = [self.tags.day_timer.replace(first), std::mem::replace(&mut self.tags.day_timer_2, second)].into_iter().flatten().collect();
        // Dead ones too, while their corpses remain (measured: a creep killed by script).
        let units: Vec<u32> = self.unit_order.iter().rev().copied().filter(|&h| self.unit(h).is_some()).collect();
        // Each unit's object of the change lasts three seconds of the main clock (traced:
        // queued with an event 3.0 seconds on, freed in the turn after, in the order
        // made).
        let now = self.time_of_day_clock;
        for h in units {
            let s = self.tags.take(b"`rcm");
            self.tags.doom_at(now + 3.0, s);
            let Some(u) = self.unit(h).filter(|u| !u.dead) else { continue };
            // With the night an idle creep that may sleep falls asleep at once
            // (measured: a creep made by day, after a night set by script, wakes making
            // its spells' timers when the day is set again).
            if !day {
                let allowed = self.players.get(u.owner as usize).is_none_or(|p| p.get_state(player_state::NO_CREEP_SLEEP) == 0);
                let idle = matches!(u.action, Action::Idle) && !u.extra.returning && u.swing.is_none();
                if u.extra.can_sleep && !u.hidden && allowed && idle && !u.extra.sleeping {
                    // Its spells' listeners go as it sleeps, the last first, Ensnare's
                    // in-range object after the listener made before it (read: at
                    // nightfall, after every unit's object of the change).
                    let (listeners, pil) = self.unit_mut(h).map(|u| (std::mem::take(&mut u.extra.mana_listeners), u.extra.ensnare_pil.take())).unwrap_or_default();
                    for (_, s) in listeners.iter().rev() {
                        self.tags.doom(*s);
                        if let Some((_, p)) = pil.filter(|&(r, _)| r == *s) {
                            self.tags.doom(p);
                        }
                    }
                    if let Some((r, p)) = pil.filter(|&(r, _)| !listeners.iter().any(|l| l.1 == r)) {
                        let _ = r;
                        self.tags.doom(p);
                    }
                    self.fall_asleep(h);
                }
                continue;
            }
            if !u.extra.sleeping || u.extra.sleep_perm {
                continue;
            }
            let spells: Vec<crate::objects::Id> = u.abilities.iter().map(|(a, _)| *a).collect();
            let table = crate::tags::AbilityObjects::get();
            // Each spell's listener on its mana (`` `rtl ``): made below its cost, then,
            // the creep's mana gone to none, again above it in the reverse order, in the
            // slots the first freed (traced: a High Priest's Heal, Inner Fire, and Abolish
            // Magic; its Heal's the last, re-armed as its mana passes 5.1).
            let mut costs = Vec::new();
            let mut slots = Vec::new();
            let mut pil = None;
            for a in spells {
                let Some(code) = self.objects.abilities.get(a).map(|o| crate::objects::id_from_str(o.str("code").trim())) else { continue };
                if table.objects(&a, &code).contains(b"`rtl") {
                    let s = self.tags.take(b"`rtl");
                    slots.push(s);
                    costs.push(self.objects.abilities.get(a).map_or(0.0, |o| o.level_real("Cost", 1)) + 0.1);
                    if code == *b"Aens" {
                        pil = Some((s, self.tags.take(b"`pil")));
                    }
                }
            }
            slots.reverse();
            // One whose cost its mana is past already waits for it to fall (read: creeps
            // waking with full mana at dawn made their listeners, none replaced after).
            let mana = self.unit(h).map_or(0.0, |u| u.mana);
            let costs: Vec<f32> = costs.into_iter().map(|c| if mana >= c { f32::INFINITY } else { c }).collect();
            if let Some(u) = self.unit_mut(h) {
                u.extra.mana_listeners = costs.into_iter().zip(slots).collect();
                u.extra.ensnare_pil = pil;
            }
            self.set_asleep(h, false);
        }
        // One set by script lets them go as the next turn starts (traced: the engine's
        // day timer at load, slot 0x287, at the game's first dawn).
        for s in old {
            if set {
                self.tags.doom(s);
            } else {
                self.tags.release(s);
            }
        }
    }
}
