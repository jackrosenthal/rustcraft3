//! The original engine's tags: every object it makes (a game object, an ability, a
//! script handle, and the engine's own regions, positions, and properties) takes a slot
//! in its object manager's table (Tempest's, `DAT_6fd6860c`; `FUN_6f1c53f0`): the last
//! slot given back (`FUN_6f1cbff0` keeps them as a list, the last freed first), else a
//! new one at the table's end (`FUN_6f1cbf40`), and a birth count that only grows
//! (+0x50).  The two name an object in the network's actions, and the next slot and the
//! birth count go into the game's checksum, so this game takes the same tags at the
//! same moments (`docs/reference/bootybay_tags.txt` is the original's sequence for a
//! map's start).

use serde::{Deserialize, Serialize};
use crate::codes;

/// A type code as the original keeps it at +0x10 of each tagged object (`+w3u` a unit,
/// `^pmr` a region, `Amov` an ability, ...).
pub type Code = [u8; 4];

/// A tag given (its object's code and slot), a slot given back, or a turn's start.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TagEvent {
    Take(Code, u32),
    Free(u32),
    Turn,
    /// A movement step's start (its index in the tick), for reading a trace.
    Step(usize),
}

/// While set, a new world records its tags' codes (the `tags` example sets it).
pub static RECORD: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Tags {
    /// The table's slots (given and given back).
    pub slots: u32,
    /// Slots given back, the last one first to be given again.
    #[serde(default)]
    pub free: Vec<u32>,
    /// The birth count: tags given so far.
    #[serde(default)]
    pub born: u32,
    /// The objects tagged and not given back, by slot: each its code and birth (the
    /// original's object table, as a save keeps it).
    #[serde(default)]
    pub live: std::collections::BTreeMap<u32, (Code, u32)>,
    /// The objects' codes in the order tagged, while recording (for comparing with the
    /// original's).
    #[serde(skip)]
    pub log: Option<Vec<Code>>,
    /// While recording, every tag given (code and slot), slot given back, and turn, in
    /// order (for comparing with a trace of the original's).
    #[serde(skip)]
    pub trace: Option<Vec<TagEvent>>,
    /// Where each creep camp's first creep was made (the camps' AI takes tags).
    #[serde(default)]
    pub camps: Vec<(f32, f32)>,
    /// Each camp's AI (`AIcs`), as made with its first creep.
    pub camp_ais: Vec<(u32, u32)>,
    /// The players' AIs (`AI-1`), each computer's and then Neutral Hostile's, in the
    /// order made.
    pub player_ais: Vec<(u32, u32)>,
    /// Each player AI's three timers' start counts (`FUN_6f9c7b20`), in that order.
    #[serde(default)]
    pub player_ai_timers: Vec<[u32; 3]>,
    /// The town sites' AIs (`AIts`), each made with a gold mine, and where: they last
    /// past their mines (a haunted mine's site is the mine's it replaced).
    pub sites: Vec<(f32, f32, (u32, u32))>,
    /// The script functions with a handler, which the original makes the first time a
    /// function is given to an object (an action, a filter, a timer; `FUN_6f2872f0`).
    #[serde(default)]
    pub handlers: std::collections::BTreeSet<u32>,
    /// The timer queue's count of agent timer starts (`FUN_6f0602a0` and its kin, the
    /// queue's +0x50): each start takes the next, which its timer keeps.
    #[serde(default)]
    pub timer_starts: u32,
    /// The agent timers objects keep, by the object's slot: each its count, its
    /// function's name, the game time it started, and its wait (0 for a periodic one's,
    /// which its kind knows).
    #[serde(default)]
    pub timer_objects: std::collections::HashMap<u32, Vec<(u32, String, f32, f32)>>,
    /// Each aura's next sweep due (its periodic timer's, `40b5e0`, on the agent clock),
    /// by the aura's slot.
    #[serde(default)]
    pub aura_dues: std::collections::HashMap<u32, u32>,
    /// The property module's count of timing nodes (its +0x50, at 0x6f15d735,
    /// 0x6f15dc58, 0x6f15e127), which properties' and listeners' blocks keep: taken as
    /// objects are made (`Tags::take`: a modifier `` `rcm `` one, a threshold listener
    /// `` `rtl `` two, an in-range listener `` `pil `` three, an order or task one) and as a
    /// sleeping creep's listeners go (one each).  The count each object's last took.
    #[serde(default)]
    pub prop_count: u32,
    #[serde(default)]
    pub prop_counts: std::collections::BTreeMap<u32, u32>,
    /// Each start's count and kind, while logged (W3_TIMERLOG).
    #[serde(skip)]
    pub timer_log: Option<Vec<(u32, &'static str, u32)>>,
    /// The engine's main clock (its single's bits) as objects are made now (set each
    /// tick: `World::plan_steps`).
    #[serde(default)]
    pub clock: u32,
    /// The main clock at each live object's making, where past 0: a property's time
    /// stamp (`FUN_6f161090`: its clock group's time, +0x70).
    #[serde(default)]
    pub made_at: std::collections::BTreeMap<u32, u32>,
    /// The auto-target spells' checks (their timers started by CAbilityAutoTargetSpell
    /// vf267 and vf270), each due at the next step after its start and every 3 seconds
    /// on (`FUN_6f42f8b0`; traced: the quiet test map's 17 at 0.015 and 3.02 seconds).
    #[serde(default)]
    pub autotarget_due: Vec<f32>,
    /// Each check's unit and spell, and the spells' timers as last started (their count,
    /// due time as the original's queue has it, its first 0.02 after its start, and
    /// period), for saves.
    #[serde(default)]
    pub autotarget_info: Vec<(u32, Code)>,
    #[serde(default)]
    pub autotarget_saved: Vec<f32>,
    #[serde(default)]
    pub autotarget_timers: std::collections::HashMap<(u32, Code), (u32, f32, f32)>,
    /// Each Unholy Aura buff's modifier (`` `rcm ``), by the buff's slot.
    #[serde(default)]
    pub buff_modifiers: std::collections::HashMap<u32, u32>,
    /// Objects to free at the tick's end, after the structures' alarm watch (an AI
    /// worker's walk to its mine slot ended in the tick).
    #[serde(default)]
    pub tick_end: Vec<u32>,
    /// The buffs that listen to their owner's tech tree changes (Command, War Drums, and
    /// Trueshot Aura's), each its slot and its unit's owner, in the order made.
    #[serde(default)]
    pub tech_buffs: Vec<(u32, u8)>,
    /// Each player's object (`+ply`), by number.
    #[serde(default)]
    pub player_slots: Vec<u32>,
    /// The handlers' objects (`hdlr`, CJassFunc): each slot's function (an action's
    /// or filter's, made once a function, or a timer's own).
    #[serde(default)]
    pub handler_objects: std::collections::BTreeMap<u32, u32>,
    /// Each function's number, as handlers save it: the count of functions given a
    /// handler till it, a timer's too (read: the quiet test map's 1, 2, 3, ... by their
    /// handlers' births, a timer's for a function given before keeping that one's).
    #[serde(default)]
    pub function_numbers: std::collections::BTreeMap<u32, u32>,
    /// Objects the engine makes right after the next one tagged (`after_next`).
    #[serde(skip)]
    pending: Vec<Code>,
    /// Slots whose objects were released, freed in this order as the next turn starts
    /// (the original queues each object's destruction as an event due at once, which
    /// its event queue runs at the next frame in the order queued; `FUN_6f15e500`).
    #[serde(default)]
    pub doomed: Vec<u32>,
    /// Freed as the next turn starts.
    #[serde(default)]
    pub next_turn: Vec<u32>,
    /// Freed as the next turn starts, after `doomed`: a removed unit's own objects (traced:
    /// a gold mine's after the objects the haunted mine made since released).
    #[serde(default)]
    pub removed: Vec<u32>,
    /// Freed at the next turn's end, after what that turn released (`later_now`).
    #[serde(default)]
    pub next_turn_end: Vec<u32>,
    #[serde(default)]
    pub later_now: Vec<u32>,
    /// Ended walks' paths (`l^pa`, `^clu`), freed in the next movement step
    /// (`World::first_steps`, `World::step_walkers`).
    #[serde(default)]
    pub step_paths: Vec<([u32; 2], u32)>,
    /// Walk ends' objects released in a tick's first step, while it runs: freed in
    /// order as it ends, after the paths of the last tick's ends (`Tags::end_step`).
    #[serde(default)]
    pub step_frees: Option<Vec<u32>>,
    /// Issued orders' objects freed once the orders due together are all given
    /// (`flush_issued`).
    #[serde(default)]
    pub issued: Vec<u32>,
    /// Slots released after a delay: the game time they fall due, in the order queued.
    #[serde(default)]
    pub timed: Vec<(f64, u32)>,
    /// Script handles' slots (groups, locations, timers, trigger executions, ...), by
    /// handle.
    #[serde(default)]
    pub handle_slots: std::collections::BTreeMap<u32, u32>,
    /// The ids (slot and birth count) the original's actions name script objects by:
    /// dialogs, their buttons, trackables, and event registrations, by handle.
    #[serde(default)]
    pub handle_ids: std::collections::BTreeMap<u32, (u32, u32)>,
    /// Destroyed script objects' agents and handle table words, by handle (what the
    /// original's table keeps while references to them remain).
    #[serde(default)]
    pub dead_handles: std::collections::BTreeMap<u32, ((u32, u32), u32)>,
    /// The game's listeners replaced since it began (or was loaded), whose keys its
    /// handler list keeps, empty (read: the quiet test map's first day change listener
    /// and Blizzard.j's dawn's first).
    #[serde(default)]
    pub game_freed_keys: Vec<(u32, u32)>,
    /// The game's two periodic timers' start counts (events 0x80269 and 0x8026a), and
    /// the clock they started at.
    #[serde(default)]
    pub game_timers: [u32; 2],
    #[serde(default)]
    pub game_timers_clock: u32,
    /// The main clock as SaveGame was called (its tick's).
    #[serde(default)]
    pub save_clock: Option<u32>,
    /// Event registrations' listeners' slots (a game state's `` `rtl ``, an alliance's or
    /// player state's `` `fcl ``), by event handle.
    #[serde(default)]
    pub event_timers: std::collections::BTreeMap<u32, u32>,
    /// The engine's day timer's slot (the `rtl among its objects at load, replaced at
    /// each change between day and night).
    #[serde(default)]
    pub day_timer: Option<u32>,
    /// By day the change's second listener (`FUN_6f283c80`: by day one on the time
    /// falling below dawn and one on its rising past dusk; by night one).
    #[serde(default)]
    pub day_timer_2: Option<u32>,
    /// When the time of day's listeners were last made (the objects born by then):
    /// the script events' replaced since their registration, and the change's.
    #[serde(default)]
    pub listener_made: std::collections::BTreeMap<u32, u64>,
    #[serde(default)]
    pub day_listener_made: u64,
}

impl Tags {
    pub fn new() -> Tags {
        let rec = RECORD.load(std::sync::atomic::Ordering::Relaxed);
        Tags { slots: 0, free: Vec::new(), born: 0, live: Default::default(), trace: rec.then(Vec::new), log: rec.then(Vec::new), camps: Vec::new(), camp_ais: Vec::new(), player_ais: Vec::new(), player_ai_timers: Vec::new(), sites: Vec::new(), handlers: Default::default(), timer_starts: 0, timer_objects: Default::default(), aura_dues: Default::default(), timer_log: std::env::var_os("W3_TIMERLOG").map(|_| Vec::new()), clock: 0, made_at: Default::default(), prop_count: 0, prop_counts: Default::default(), autotarget_due: Vec::new(), autotarget_info: Vec::new(), autotarget_saved: Vec::new(), autotarget_timers: Default::default(), buff_modifiers: Default::default(), tick_end: Vec::new(), tech_buffs: Vec::new(), player_slots: Vec::new(), handler_objects: Default::default(), function_numbers: Default::default(), pending: Vec::new(), doomed: Vec::new(), next_turn: Vec::new(), removed: Vec::new(), next_turn_end: Vec::new(), later_now: Vec::new(), step_paths: Vec::new(), step_frees: None, issued: Vec::new(), timed: Vec::new(), handle_slots: Default::default(), handle_ids: Default::default(), dead_handles: Default::default(), game_freed_keys: Vec::new(), game_timers: [0; 2], game_timers_clock: 0, save_clock: None, event_timers: Default::default(), day_timer: None, day_timer_2: None, listener_made: Default::default(), day_listener_made: 0 }
    }

    /// The next tag, for an object of type `code`: its slot.
    pub fn take(&mut self, code: &Code) -> u32 {
        self.take_id(code).0
    }

    /// The next tag, for an object of type `code`: its slot and its birth count, the two
    /// words that name it in the original's actions.
    pub fn take_id(&mut self, code: &Code) -> (u32, u32) {
        let t = self.free.pop().unwrap_or_else(|| {
            self.slots += 1;
            self.slots - 1
        });
        let birth = self.born;
        self.born += 1;
        // W3_TAGBT=<birth>: where the object of that birth is made.
        if std::env::var("W3_TAGBT").ok().and_then(|v| v.parse::<u32>().ok()) == Some(birth) {
            eprintln!("tag {birth} {}: {}", String::from_utf8_lossy(code), std::backtrace::Backtrace::force_capture());
        }
        self.live.insert(t, (*code, birth));
        let n = match code {
            codes::class::CONTINUOUS_MODIFIER | codes::class::ORDER | codes::class::TASK | codes::class::TASK_A => 1,
            codes::class::THRESHOLD_LISTENER => 2,
            codes::class::IN_RANGE_LISTENER => 3,
            _ => 0,
        };
        if n > 0 {
            self.prop_count += n;
            self.prop_counts.insert(t, self.prop_count);
            if let Some(l) = self.timer_log.as_mut() {
                l.push((self.prop_count, "prop", u32::from_be_bytes(*code)));
            }
        }
        if self.clock != 0 {
            self.made_at.insert(t, self.clock);
        } else {
            self.made_at.remove(&t);
        }
        if let Some(l) = self.log.as_mut() {
            l.push(*code);
        }
        if let Some(l) = self.trace.as_mut() {
            l.push(TagEvent::Take(*code, t));
        }
        self.flush();
        (t, birth)
    }

    /// Gives a slot back.
    pub fn release(&mut self, slot: u32) {
        self.free.push(slot);
        self.live.remove(&slot);
        self.made_at.remove(&slot);
        self.prop_counts.remove(&slot);
        self.tech_buffs.retain(|&(s, _)| s != slot);
        self.timer_objects.remove(&slot);
        self.aura_dues.remove(&slot);
        if let Some(l) = self.trace.as_mut() {
            l.push(TagEvent::Free(slot));
        }
    }

    /// Notes a turn's start in the trace, and frees the objects released before it.
    pub fn turn(&mut self) {
        if let Some(l) = self.trace.as_mut() {
            l.push(TagEvent::Turn);
        }
        for s in std::mem::take(&mut self.next_turn) {
            self.release(s);
        }
        for s in std::mem::take(&mut self.doomed) {
            self.release(s);
        }
        for s in std::mem::take(&mut self.removed) {
            self.release(s);
        }
        self.later_now = std::mem::take(&mut self.next_turn_end);
    }

    /// Queues a slot's release at a time of the main clock (seconds since the game
    /// began, `World::tick_abs`).
    pub fn doom_at(&mut self, due: f64, slot: u32) {
        self.timed.push((due, slot));
    }

    /// A timed free put off to `due`, the slot's earlier one dropped.
    pub fn redoom_at(&mut self, due: f64, slot: u32) {
        self.timed.retain(|&(_, s)| s != slot);
        self.timed.push((due, slot));
    }

    /// Frees what falls due before `now` (strictly after its time, as the original's
    /// events run in the turn after a time they fall due on), earliest first, those due
    /// together in the order queued; then what was released at once.
    pub fn free_due(&mut self, now: f64) {
        self.free_timed(now);
        self.free_doomed();
        for s in std::mem::take(&mut self.later_now) {
            self.release(s);
        }
    }

    /// Frees the timed releases falling due before `now`, earliest first.
    pub fn free_timed(&mut self, now: f64) {
        let mut due: Vec<(f64, u32)> = Vec::new();
        self.timed.retain(|&(t, s)| {
            if t < now - 1e-4 {
                due.push((t, s));
                false
            } else {
                true
            }
        });
        due.sort_by(|a, b| a.0.total_cmp(&b.0));
        for (_, s) in due {
            self.release(s);
        }
    }

    /// A walk end's object released: at once, or as a tick's first step ends.
    pub(crate) fn release_in_step(&mut self, s: u32) {
        match self.step_frees.as_mut() {
            Some(q) => q.push(s),
            None => self.release(s),
        }
    }

    /// A tick's first step begins: its walk ends' objects wait for its end.
    pub fn begin_first_step(&mut self) {
        self.step_frees = Some(Vec::new());
        self.mark_step(0);
    }

    /// Notes a movement step's start in the trace.
    pub fn mark_step(&mut self, k: usize) {
        if let Some(l) = self.trace.as_mut() {
            l.push(TagEvent::Step(k));
        }
    }

    /// A walk ended in this step (a wanderer's give-up or arrival, a worker's arrival at a
    /// structure): its path (`l^pa`, `^clu`) goes in the next step where the walk would
    /// come in the walkers' order, newest first, and at that step's end after the
    /// walkers when older than them all (`World::first_steps`, `World::step_walkers`;
    /// read: a give-up's path freed before an older walker's give-up made its three, an
    /// arrival's after a newer one's; a peon's and a wanderer's paths, ended in the same
    /// step, newest walk first, each way round; a peon's after an order given as the
    /// next step began and before that order's eight).
    pub fn walk_ended(&mut self, path: [u32; 2], serial: u32) {
        self.step_paths.push((path, serial));
    }

    /// A tick's first step's end: its walk ends' objects (read: a give-up's three new
    /// objects made, then the last turn's arrival's path freed, then the give-up's
    /// tree).  In a tick's later steps the orders' objects come first (`end_step`).
    pub fn end_first_step(&mut self) {
        for s in self.step_frees.take().unwrap_or_default() {
            self.release(s);
        }
    }

    /// A tick's later step begins: its walk ends' objects too wait for its end (traced:
    /// two give-ups in a turn's second step make their new objects, then free all).
    pub fn begin_step(&mut self) {
        self.step_frees = Some(Vec::new());
    }

    /// A tick's later step's end: its walk ends' objects freed in order.
    pub fn end_step(&mut self) {
        self.flush_issued();
        for s in self.step_frees.take().unwrap_or_default() {
            self.release(s);
        }
    }

    /// The ended walks' paths waiting for this step.
    pub fn take_step_paths(&mut self) -> Vec<([u32; 2], u32)> {
        std::mem::take(&mut self.step_paths)
    }

    /// Frees a step's waiting paths at once.
    pub fn free_step_paths(&mut self, paths: Vec<([u32; 2], u32)>) {
        for (p, _) in paths {
            for s in p {
                self.release(s);
            }
        }
    }

    /// Frees what has been released so far (what waits for a step waits for the next
    /// tick's first).
    pub fn free_doomed(&mut self) {
        for s in std::mem::take(&mut self.doomed) {
            self.release(s);
        }
    }

    /// Queues an object's destruction: its slot is freed as the next turn starts.
    pub fn doom(&mut self, slot: u32) {
        self.doomed.push(slot);
    }

    /// A listener of the game's replaced: its key kept in the game's handler list.
    pub fn game_key_freed(&mut self, slot: u32) {
        if let Some(&(_, birth)) = self.live.get(&slot) {
            self.game_freed_keys.push((slot, birth));
        }
    }

    /// Queues a script handle's destruction, when it has a slot; its agent (and `word`,
    /// its handle table word) kept for the references scripts may still hold.
    pub fn doom_handle(&mut self, h: u32, word: u32) {
        if let Some(s) = self.handle_slots.remove(&h) {
            if let Some(&(_, birth)) = self.live.get(&s) {
                self.dead_handles.insert(h, ((s, birth), word));
            }
            self.doom(s);
        }
    }

    /// The slot the next object would take (the checksum's presence term).
    pub fn presence(&self) -> u32 {
        self.free.last().copied().unwrap_or(self.slots)
    }

    /// A move order's objects as it is issued (traced with Booty Bay's wandering
    /// critters): its order, task tree, path cluster, and path, eighteen, nine of which
    /// go at once (the cluster's `^clg`, the second, the fifteenth, and six tasks, the
    /// last first).  The slots, by place.
    pub fn move_issued(&mut self) -> Vec<u32> {
        const CODES: [&Code; 18] = [codes::class::ORDER_TASKS, codes::class::TASK_A, codes::class::TASK_A, codes::class::TASK_O, codes::class::TASK, codes::class::TASK, codes::class::TASK, b"tsk.", codes::class::TASK, codes::class::TASK, codes::class::TASK, codes::class::TASK, codes::class::TASK, codes::class::TASK, codes::class::TASK_A, codes::class::CLUSTER_GOAL, codes::class::CLUSTER, codes::class::PATH];
        let slots: Vec<u32> = CODES.iter().map(|c| self.take(c)).collect();
        // Its `^clg` goes at once, the other eight once the orders given with it are
        // (traced: two wanderers' orders between the same steps, each `^clg` freed after
        // its order, then the first's eight and the second's).
        self.release(slots[15]);
        self.issued.extend([1, 14, 13, 12, 11, 10, 9, 8].map(|k| slots[k]));
        slots
    }

    /// A player's order to walk to a point (traced: one peon sent across the ground):
    /// as `move_issued`'s but one task fewer (17), its `^clg` freed at once and its
    /// two blocks and five tasks as the next turn starts, as a player's harvest
    /// order's are.  One given while the unit still walks by such an order (`replaced`,
    /// its objects) frees that order's rest and its path then too: after the new
    /// order's first block, the old order's tree, then the new order's other six, then
    /// the old path (read: two orders to walk given to a peon in one turn).
    pub fn player_move_issued(&mut self, replaced: Option<&[u32]>) -> Vec<u32> {
        const CODES: [&Code; 17] = [codes::class::ORDER_TASKS, codes::class::TASK_A, codes::class::TASK_A, codes::class::TASK_O, codes::class::TASK, codes::class::TASK, codes::class::TASK, b"tsk.", codes::class::TASK, codes::class::TASK, codes::class::TASK, codes::class::TASK, codes::class::TASK, codes::class::TASK_A, codes::class::CLUSTER_GOAL, codes::class::CLUSTER, codes::class::PATH];
        let slots: Vec<u32> = CODES.iter().map(|c| self.take(c)).collect();
        self.release(slots[14]);
        let Some(old) = replaced.filter(|o| o.len() == 17) else {
            for k in [1, 13, 12, 11, 10, 9, 8] {
                self.doom(slots[k]);
            }
            return slots;
        };
        self.doom(slots[1]);
        for k in [7, 6, 5, 4, 3, 2, 0] {
            self.doom(old[k]);
        }
        for k in [13, 12, 11, 10, 9, 8] {
            self.doom(slots[k]);
        }
        for k in [16, 15] {
            self.doom(old[k]);
        }
        slots
    }

    /// A unit's order of several sent together to a point (traced: five peons): as
    /// `player_move_issued`'s without the cluster and path, 14 objects, its blocks and
    /// five tasks as the next turn starts.
    pub fn member_move_issued(&mut self) -> Vec<u32> {
        const CODES: [&Code; 14] = [codes::class::ORDER_TASKS, codes::class::TASK_A, codes::class::TASK_A, codes::class::TASK_O, codes::class::TASK, codes::class::TASK, codes::class::TASK, b"tsk.", codes::class::TASK, codes::class::TASK, codes::class::TASK, codes::class::TASK, codes::class::TASK, codes::class::TASK_A];
        let slots: Vec<u32> = CODES.iter().map(|c| self.take(c)).collect();
        for k in [1, 13, 12, 11, 10, 9, 8] {
            self.doom(slots[k]);
        }
        slots
    }

    /// Frees the objects of the orders just given (`move_issued`).
    pub fn flush_issued(&mut self) {
        for s in std::mem::take(&mut self.issued) {
            self.release(s);
        }
    }

    /// A move order done: three more objects, then the rest of its tree, last made
    /// first in parts, and its path.
    /// The path's (`l^pa`, `^clu`) go as the next movement step starts (traced: in the
    /// next turn when the walk ended in its turn's last step, after an order due ahead
    /// of that turn's first step).
    /// A walk done where it began, never moving (its goal within reach as it started,
    /// done at its first step): no objects made, its tree but the issue's nine
    /// (`move_issued`) freed then, `tsk.` first and `ordt` last, and its path as the
    /// next turn starts (traced: a sheep sent 13.7 away in a turn's last step, its tree
    /// freed in the next turn's first step, before its logic, and its path as the one
    /// after starts).
    /// A walk that reached its goal: no objects made, its tree but the issue's nine
    /// freed at once, its path as the next step starts, as `move_done`'s (traced: sheep
    /// arriving at their goals in turns 119 and 129, the path right after; one arriving
    /// at a turn's last step, the path in the next turn).  `move_done` is for a walk
    /// ended facing a goal it could not come nearer.
    pub fn move_arrived(&mut self, slots: &[u32], serial: u32) {
        for k in [7, 6, 5, 4, 3, 2, 0] {
            self.release_in_step(slots[k]);
        }
        self.walk_ended([slots[slots.len() - 1], slots[slots.len() - 2]], serial);
    }

    pub fn move_done_in_place(&mut self, slots: &[u32]) {
        for k in [7, 6, 5, 4, 3, 2, 0] {
            self.release(slots[k]);
        }
        for k in [slots.len() - 1, slots.len() - 2] {
            self.next_turn.push(slots[k]);
        }
    }

    pub fn move_done(&mut self, slots: &[u32], serial: u32) {
        let a = self.take(codes::class::TASK_A);
        let b = self.take(codes::class::TASK);
        let c = self.take(codes::class::TASK);
        for s in [slots[7], c, b, a, slots[6], slots[5], slots[4], slots[3], slots[2], slots[0]] {
            self.release_in_step(s);
        }
        self.walk_ended([slots[slots.len() - 1], slots[slots.len() - 2]], serial);
    }

    /// Objects the engine makes right after the next object tagged.
    pub fn after_next(&mut self, codes: &[&Code]) {
        self.pending = codes.iter().map(|c| **c).collect();
    }

    /// The objects `after_next` holds, if no object has been tagged since.
    pub fn flush(&mut self) {
        for c in std::mem::take(&mut self.pending) {
            let s = self.take(&c);
            if c == *codes::class::THRESHOLD_LISTENER && self.day_timer.is_none() {
                self.day_timer = Some(s);
            }
        }
    }

    /// Tags for objects of the codes given, in turn.
    pub fn take_all(&mut self, codes: &[&Code]) {
        for c in codes {
            self.take(c);
        }
    }

    /// The handler of a script function given to an object, the first time.
    pub fn handler(&mut self, func: u32) {
        if self.handlers.insert(func) {
            let s = self.take(codes::class::HANDLER);
            self.handler_objects.insert(s, func);
            self.function_number(func);
        }
    }

    /// Counts of the property module's taken without an object made (`prop_count`),
    /// logged with what took them.
    pub fn bump_prop(&mut self, n: u32, what: &'static str) {
        self.prop_count += n;
        if let Some(l) = self.timer_log.as_mut() {
            l.push((self.prop_count, "prop", u32::from_be_bytes(what.as_bytes()[..4].try_into().unwrap_or(*b"????"))));
        }
    }

    /// An agent timer started: the queue's next count, which it keeps.
    pub fn start_timer(&mut self, kind: &'static str) -> u32 {
        self.timer_starts += 1;
        let clock = self.clock;
        if let Some(l) = self.timer_log.as_mut() {
            l.push((self.timer_starts, kind, clock));
        }
        self.timer_starts
    }

    /// A function's handler number, given it the first time.
    pub fn function_number(&mut self, func: u32) -> u32 {
        let n = self.function_numbers.len() as u32 + 1;
        *self.function_numbers.entry(func).or_insert(n)
    }

    /// `n` objects of one code.
    pub fn take_n(&mut self, code: &Code, n: usize) {
        for _ in 0..n {
            self.take(code);
        }
    }
}

/// The objects the original makes for an ability after the ability's own, as measured
/// (`docs/reference/ability_tags.txt`): by the ability's id, and for an ability the
/// measurement lacks (a map's own), by the first measured one of its code.
pub struct AbilityObjects {
    by_id: std::collections::HashMap<Code, (Code, Vec<Code>)>,
    by_code: std::collections::HashMap<Code, Vec<Code>>,
    /// Those the script could not add (the engine's own, measured with nothing made).
    refused: std::collections::HashSet<Code>,
}

impl AbilityObjects {
    pub fn get() -> &'static AbilityObjects {
        static TABLE: std::sync::OnceLock<AbilityObjects> = std::sync::OnceLock::new();
        TABLE.get_or_init(|| {
            let mut t = AbilityObjects { by_id: Default::default(), by_code: Default::default(), refused: Default::default() };
            let code = |s: &str| -> Code { s.as_bytes().try_into().unwrap_or(*b"????") };
            for line in include_str!("../../../docs/reference/ability_tags.txt").lines().filter(|l| !l.starts_with('#')) {
                let mut f = line.split(' ').filter(|s| !s.is_empty());
                let (Some(id), Some(c)) = (f.next(), f.next()) else { continue };
                let words: Vec<&str> = f.collect();
                if words == ["-"] {
                    t.refused.insert(code(id));
                }
                let objects: Vec<Code> = words.iter().filter(|w| **w != "-").map(|w| code(w)).collect();
                t.by_code.entry(code(c)).or_insert_with(|| objects.clone());
                t.by_id.insert(code(id), (code(c), objects));
            }
            t
        })
    }

    /// Whether the script's UnitAddAbility made nothing of it in the original.
    pub fn refused(&self, id: &Code) -> bool {
        self.refused.contains(id)
    }

    /// The objects after an ability of this id and code.
    pub fn objects(&self, id: &Code, code: &Code) -> &[Code] {
        match self.by_id.get(id) {
            Some((c, o)) if c == code => o,
            _ => self.by_code.get(code).map_or(&[], |o| o),
        }
    }
}

/// How many pathing cells apart (CreepCampPathingCellDistance, 26) a creep may be from a
/// camp's first creep and still join it (`FUN_6f9ba6d0`; measured: Booty Bay's 36 creeps
/// found the original's 13 camps).
const CAMP_CELLS: f32 = 26.0;

impl crate::world::World {
    /// The tags of a new unit and of what the original makes with it, in its order
    /// (`FUN_6f68a060`; measured by a census of every unit type, under Wine): the unit
    /// and its own objects, its engine abilities, those of its type's list, the ones
    /// added after, two tasks, and a creep's AI.
    pub(crate) fn tag_unit(&mut self, h: u32) {
        let Some(u) = self.unit(h) else { return };
        let (type_id, owner, x, y) = (u.type_id, u.owner, u.x, u.y);
        let Some(o) = self.objects.units.get(type_id) else { return };
        let field = |f: &str| o.str(f).trim().to_string();
        let ids = |f: &str| -> Vec<Code> { o.list(f).iter().map(|s| crate::objects::id_from_str(s.trim())).filter(|c| c[0] != 0).collect() };
        let is_unit = |c: &Code| self.objects.units.get(*c).is_some();
        let building = o.bool("isbldg");
        let mechanical = field("type").to_ascii_lowercase().split(',').any(|t| t.trim() == "mechanical");
        let race = field("race").to_ascii_lowercase();
        let ability = |id: &Code| self.objects.abilities.get(*id).map(|a| crate::objects::id_from_str(a.str("code").trim()));

        let mut codes: Vec<Code> = vec![*codes::class::UNIT, *codes::class::UNIT_POSITION, *codes::class::PATHING_REGION, *codes::class::PATHING_REGION, *codes::class::REAL, *codes::class::REAL, *codes::class::REAL, *codes::class::REAL, *codes::class::REAL, *codes::class::PATH];
        const BASE: usize = 10;
        let mut blight_rcm: Option<usize> = None;
        let blight_regen = field("regenType").eq_ignore_ascii_case("blight");
        // Life regeneration: a timer, or for regeneration on blight its ability.
        match field("regenType").to_ascii_lowercase().as_str() {
            "always" | "night" | "day" => codes.push(*codes::class::CONTINUOUS_MODIFIER),
            // Its timer too when made on blight (measured: undead made where the
            // test's undead buildings had spread blight had one, those made before
            // them none).
            "blight" => {
                codes.push(*codes::ability::id::REGENERATE_ON_BLIGHT);
                if self.point_blighted(x, y) {
                    blight_rcm = Some(codes.len());
                    codes.push(*codes::class::CONTINUOUS_MODIFIER);
                }
            }
            _ => {}
        }
        codes.push(*codes::class::THRESHOLD_LISTENER);
        if o.real("manaN") > 0.0 {
            codes.push(*codes::class::CONTINUOUS_MODIFIER);
        }
        let mods_end = codes.len();
        let footprint = building.then_some(codes.len());
        if building {
            codes.extend([*codes::class::PATHING_REGION; 3]);
        }

        // The engine's abilities, as the type's lists call for them.
        let trains = ids("Trains").iter().any(is_unit);
        let researches = ids("Researches").iter().any(|c| self.objects.upgrades.get(*c).is_some());
        let revive = o.bool("Revive");
        if ids("Builds").iter().any(is_unit) {
            codes.push(match race.as_str() {
                "human" => *codes::ability::id::BUILD_HUMAN,
                "orc" => *codes::ability::id::BUILD_ORC,
                "undead" => *codes::ability::id::BUILD_UNDEAD,
                "nightelf" => *codes::ability::id::BUILD_NIGHT_ELF,
                "naga" => *codes::ability::id::BUILD_NAGA,
                _ => *codes::ability::id::BUILD_NEUTRAL,
            });
            codes.push(*codes::class::REAL);
        }
        if trains || researches || revive {
            codes.extend([*codes::ability::id::QUEUE, *codes::class::REAL]);
        }
        if ids("Makeitems").iter().any(|c| self.objects.items.get(*c).is_some()) {
            codes.extend([*codes::ability::id::MAKE_ITEMS, *codes::class::REAL]);
        }
        if ids("Sellitems").iter().any(|c| self.objects.items.get(*c).is_some()) {
            codes.extend([*codes::ability::id::SELL_ITEMS, *codes::class::REAL]);
        }
        if ids("Sellunits").iter().any(is_unit) {
            codes.extend([*codes::ability::id::SELL_UNITS, *codes::class::REAL]);
        }
        if ids("Upgrade").iter().any(is_unit) {
            codes.extend([*codes::ability::id::UPGRADE_STRUCTURE, *codes::class::REAL]);
        }
        if o.real("spd") != 0.0 {
            codes.push(*codes::ability::id::MOVE);
        }
        let weapons = o.int("weapsOn");
        if weapons != 0 {
            codes.push(*codes::ability::id::ATTACK);
            // One more object for each splashing weapon, enabled or not.
            for k in 1..=2 {
                if matches!(field(&format!("weapTp{k}")).as_str(), "msplash" | "artillery" | "aline" | "mline" | "mbounce") {
                    codes.push(*codes::class::SPLASH);
                }
            }
        }
        if type_id[0].is_ascii_uppercase() {
            codes.extend([*codes::ability::id::HERO, *codes::class::FLAG, *codes::class::FLAG, *codes::class::CONTINUOUS_MODIFIER, *codes::class::CONTINUOUS_MODIFIER, *codes::class::CONTINUOUS_MODIFIER, *codes::class::CONTINUOUS_MODIFIER]);
        }
        // The type's list, each with its objects.
        let list = ids("abilList");
        let mut spell_timers: Vec<usize> = Vec::new();
        let mut autocast: Option<usize> = None;
        let mut ai: Option<usize> = None;
        let mut blight: Option<usize> = None;
        let mut had: Vec<Code> = Vec::new();
        for id in &list {
            let Some(code) = ability(id) else { continue };
            if had.contains(&code) {
                continue;
            }
            had.push(code);
            let at = codes.len();
            let objects = self.ability_tags(type_id, id);
            if objects.first() == Some(codes::ability::id::BLIGHT_GROWTH) {
                blight = Some(at);
            }
            // A spell's timer (its first `rtl), which sleep and a mana change replace.
            if let Some(k) = objects.iter().position(|c| c == codes::class::THRESHOLD_LISTENER) {
                spell_timers.push(at + k);
            }
            codes.extend(objects);
        }
        // Rally (when it trains or revives), the inventory of a hero of an old map, fire
        // (a structure or a machine), detection of the burrowed (a flyer), the order for
        // its own autocast ability, creep sleep, and the alarm (a structure).
        if (trains || revive) && !had.contains(codes::ability::id::RALLY) {
            codes.push(*codes::ability::id::RALLY);
        }
        if type_id[0].is_ascii_uppercase() && self.editor_version < 6000 && !had.contains(codes::ability::id::INVENTORY) {
            codes.push(*codes::ability::id::INVENTORY);
        }
        if building || mechanical {
            codes.push(match race.as_str() {
                "orc" => *codes::ability::id::ON_FIRE_ORC,
                "undead" => *codes::ability::id::ON_FIRE_UNDEAD,
                "nightelf" => *codes::ability::id::ON_FIRE_NIGHT_ELF,
                _ => *codes::ability::id::ON_FIRE_HUMAN,
            });
        }
        let mut flyer = None;
        if field("movetp").eq_ignore_ascii_case("fly") && !had.contains(codes::ability::id::BURROW_DETECTION_FLYERS) {
            codes.extend([*codes::ability::id::BURROW_DETECTION_FLYERS, *codes::class::IN_RANGE_LISTENER]);
            flyer = Some(codes.len() - 1);
        }
        let auto = crate::objects::id_from_str(field("auto").as_str());
        // (The type's auto names the ability by its code: nfsh's Anhe for its Anh2.)  It
        // is not turned on while its research is missing (FUN_6f48b230 with
        // FUN_6f48f500; measured: a Shaman's Bloodlust, which requires Rost, has none).
        let auto_ability = list.iter().find(|id| ability(id) == Some(auto));
        if auto[0] != 0 && auto_ability.is_some_and(|a| self.ability_enabled(owner, *a)) {
            autocast = Some(codes.len());
            codes.push(*codes::class::ORDER);
        }
        if o.int("canSleep") != 0 && !building {
            codes.push(*codes::ability::id::CREEP_SLEEP);
        }
        if building {
            codes.push(*codes::ability::id::ALARM);
        }
        let tasks = codes.len();
        codes.extend([*codes::class::TASK, *codes::class::TASK]);
        // A gold mine is a town site for computer players (not the one a haunted mine
        // hides: measured, CreateBlightedGoldmine's has none).  Not yet: one whose way
        // from a site made before costs under 70 joins it instead (`FUN_6f9b7c10`,
        // `FUN_6f9ba790`, the cost by the path query `FUN_6f0594f0`; read: Plaguelands'
        // sixteenth mine, 61.2 cells from its fourth, made none, where Ogre Mound's and
        // Twisted Meadows' nearest pairs, 61.1 and 65.3 cells apart, each made two).
        if had.iter().any(|c| c == codes::ability::id::GOLD_MINE || c == codes::ability::id::ENTANGLED_GOLD_MINE) && !self.hidden_mine {
            codes.push(*codes::class::TOWN_SITE);
        }
        // A creep joins the nearest camp whose first creep is near enough, or makes one,
        // and takes its AI.
        // A computer's units, structures among them, take the same AI (measured under
        // Wine against an undead computer: its Necropolis, Haunted Gold Mine, and
        // workers each have it after their tasks).
        let computer = self.players.get(owner as usize).is_some_and(|p| p.controller == crate::types::map_control::COMPUTER && p.slot_state == crate::types::slot_state::PLAYING);
        let mut camp = None;
        if owner == crate::types::PLAYER_NEUTRAL_AGGRESSIVE && !building {
            let reach = CAMP_CELLS * 32.0;
            camp = self.tags.camps.iter().position(|&(cx, cy)| (cx - x).hypot(cy - y) < reach);
            if camp.is_none() {
                camp = Some(self.tags.camps.len());
                self.tags.camps.push((x, y));
                codes.push(*codes::class::CREEP_CAMP);
            }
        }
        // A worker's AI first orders each of its owner's idle workers, itself among
        // them (FUN_6f9c3660 through FUN_6f9c9cc0: FUN_6f6802f0 for each with no order;
        // measured: an undead computer's three acolytes take one, two, and three such
        // orders).  A unit with an inventory then takes its item pickup range (its
        // abilities' AI start, FUN_6f67dff0; AInv's at FUN_6f606a20).
        let mut extra = 0;
        let mut ai_refs = None;
        let mut item_pil = None;
        let mut spell_pils: Vec<(usize, Code)> = Vec::new();
        if (owner == crate::types::PLAYER_NEUTRAL_AGGRESSIVE && !building) || computer {
            ai_refs = Some(codes.len());
            codes.extend([*codes::class::UNIT_AI_REF, *codes::class::UNIT_AI]);
            if self.unit_class(self.unit(h).unwrap(), "peon") {
                extra = self.unit_order.iter().filter_map(|&v| self.unit(v)).filter(|v| v.owner == owner && !v.dead && v.order.id == 0 && self.unit_class(v, "peon")).count();
                for _ in 0..extra {
                    codes.extend([*codes::class::ORDER, *codes::class::TASK_A, *codes::class::TASK]);
                }
            }
            ai = Some(codes.len());
            codes.extend([*codes::class::ORDER, *codes::class::TASK_A, *codes::class::TASK, *codes::class::TASK, *codes::class::TASK, *codes::class::TASK, *codes::class::TASK_A, *codes::class::ORDER, *codes::class::TASK]);
            if codes.contains(codes::ability::id::INVENTORY) {
                item_pil = Some(codes.len());
                codes.push(*codes::class::IN_RANGE_LISTENER);
            }
            // So does each Blizzard and Rain of Fire (their classes' vf 0xc8,
            // `FUN_6f606140`: a listener on event 0xd01bf at its range; read under
            // winedbg, Plaguelands' Doom Guards' after their AI's objects, from the
            // making's hook `FUN_6f9b9230` for an owner with a player AI).
            for c in &had {
                if matches!(c, codes::ability::id::ARCH_MAGE_BLIZZARD | codes::ability::id::PIT_LORD_RAIN_OF_FIRE) {
                    spell_pils.push((codes.len(), *c));
                    codes.push(*codes::class::IN_RANGE_LISTENER);
                }
            }
        }
        let oid = self.tags.take_id(&codes[0]);
        let mut slots = vec![oid.0];
        // A creep falling asleep lets its spells' listeners go before its making's tasks,
        // each taking a count of the property module's (`FUN_6f48c8c0`; read: an Ogre
        // Lord's one, a High Priest's three, its autocast's order none).
        let asleep_now = owner == crate::types::PLAYER_NEUTRAL_AGGRESSIVE
            && !building
            && o.int("canSleep") != 0
            && self.is_night()
            && self.players.get(owner as usize).is_none_or(|p| p.get_state(crate::types::player_state::NO_CREEP_SLEEP) == 0);
        for (k, c) in codes.iter().enumerate().skip(1) {
            if k == tasks && asleep_now {
                self.tags.bump_prop(spell_timers.len() as u32, "slep");
            }
            slots.push(self.tags.take(c));
        }
        // The agent timers its making starts, in its objects' order, named by the
        // function starting each (traced: the quiet test map's, and a census of every
        // unit type made for Neutral Hostile at night): its blight regeneration's
        // (CAbilityBlightRegen); a sold unit's restock each (CAbilitySellUnit vf33, at
        // most twelve); its attack's, a creep's first (CAbilityAttack vf57) and its first
        // look for a target (vf194); an aura's (CAbilityAura vf57); a buff's, an aura's on
        // its bearer or a morph's (CBuff vf211); blight dispel's or growth's; fire's
        // (CAbilityOnFire vf33); creep sleep's check and, falling asleep, its sleep's
        // (vf32); the alarm's (CAbilityAlarm vf32).
        let night_sleeper = owner == crate::types::PLAYER_NEUTRAL_AGGRESSIVE
            && !building
            && o.int("canSleep") != 0
            && self.is_night()
            && self.players.get(owner as usize).is_none_or(|p| p.get_state(crate::types::player_state::NO_CREEP_SLEEP) == 0);
        let sold = ids("Sellunits").iter().filter(|c| is_unit(c)).take(12).count();
        // (Items, only those yet to arrive: read, Forestwalk's shop's eight of eleven.)
        let mut items = ids("Sellitems");
        items.extend(ids("Makeitems"));
        let arriving_items = items.iter().take(12).filter(|i| self.objects.items.get(**i).is_some_and(|o| o.real("stockStart") > 0.0)).count();
        let mut attack_timer: Option<(u32, f32, f32)> = None;
        let mut look_timer: Option<u32> = None;
        let now = self.time;
        let mut last_ability = [0u8; 4];
        for (k, c) in codes.iter().enumerate() {
            if c[0] == b'A' {
                last_ability = *c;
            }
            if c[0] == b'B' && matches!(&last_ability, codes::ability::id::PRIESTESS_TRUESHOT_AURA | codes::ability::id::COMMAND_AURA | codes::ability::id::AURA_WAR_DRUMS) {
                self.tags.tech_buffs.push((slots[k], owner));
            }
            match c {
                codes::ability::id::REGENERATE_ON_BLIGHT => {
                    let n = self.tags.start_timer("436780");
                    self.tags.timer_objects.entry(slots[k]).or_default().push((n, "436780".into(), self.time, 0.0));
                }
                codes::ability::id::SELL_UNITS | codes::ability::id::SELL_ITEMS => {
                    for _ in 0..if c == codes::ability::id::SELL_UNITS { sold } else { arriving_items } {
                        let n = self.tags.start_timer("41c010");
                        self.tags.timer_objects.entry(slots[k]).or_default().push((n, "41c010".into(), self.time, 0.0));
                    }
                }
                codes::ability::id::ATTACK => {
                    if owner == crate::types::PLAYER_NEUTRAL_AGGRESSIVE {
                        let n = self.tags.start_timer("49d220");
                        attack_timer = Some((n, self.time, 2.0));
                    }
                    let n = self.tags.start_timer("4993b0");
                    look_timer = Some(n);
                }
                codes::ability::id::BLIGHT_GROWTH => {
                    self.tags.start_timer("491250");
                }
                codes::ability::id::ON_FIRE_HUMAN | codes::ability::id::ON_FIRE_ORC | codes::ability::id::ON_FIRE_UNDEAD | codes::ability::id::ON_FIRE_NIGHT_ELF => {
                    let n = self.tags.start_timer("41baf0");
                    self.tags.timer_objects.entry(slots[k]).or_default().push((n, "41baf0".into(), self.time, 0.0));
                }
                codes::ability::id::ALARM => {
                    self.tags.start_timer("41ae70");
                }
                codes::ability::id::CREEP_SLEEP => {
                    let n = self.tags.start_timer("524e00");
                    self.tags.timer_objects.entry(slots[k]).or_default().push((n, "524e00".into(), self.time, 0.0));
                    if night_sleeper {
                        let n = self.tags.start_timer("517be0");
                        self.tags.timer_objects.entry(slots[k]).or_default().push((n, "517be0".into(), self.time, 0.0));
                    }
                }
                codes::ability::id::PALADIN_DEVOTION_AURA | codes::ability::id::ARCH_MAGE_BRILLIANCE_AURA | codes::ability::id::DEATH_KNIGHT_UNHOLY_AURA | codes::ability::id::CHIEFTAIN_ENDURANCE_AURA | codes::ability::id::PRIESTESS_TRUESHOT_AURA | codes::ability::id::DREADLORD_VAMPIRIC_AURA | codes::ability::id::KEEPER_THORNS_AURA | codes::ability::id::AURA_WAR_DRUMS | codes::ability::id::COMMAND_AURA | codes::ability::id::AURA_REGENERATION_WARD | codes::ability::id::MANA_REGENERATION_AURA => {
                    let n = self.tags.start_timer("40b5e0");
                    self.tags.timer_objects.entry(slots[k]).or_default().push((n, "40b5e0".into(), self.time, 0.0));
                }
                c if c[0] == b'B' => {
                    let n = self.tags.start_timer("532b50");
                    self.tags.timer_objects.entry(slots[k]).or_default().push((n, "532b50".into(), self.time, crate::ability::AURA_BUFF_TIME));
                }
                _ => {}
            }
        }
        // An armed creep that does not fall asleep: two more of its attack's
        // (`FUN_6f49e880`; read: a golem's, and in the census heroes' and towers').
        if owner == crate::types::PLAYER_NEUTRAL_AGGRESSIVE && codes.contains(codes::ability::id::ATTACK) && !night_sleeper {
            for _ in 0..2 {
                let n = self.tags.start_timer("49d220");
                attack_timer = Some((n, self.time, 2.0));
            }
        }
        // Its unit AI joining (`FUN_6f9b9230`), a computer's unit or a creep awake:
        // each auto-target spell's (CAbilityAutoTargetSpell vf267) and the looks of
        // Cannibalize, Animate Dead, and Mana Burn (vf50: `FUN_6f606220`,
        // `FUN_6f6060e0`, `FUN_6f606c60`), in its list's order (traced: an undead
        // computer's acolytes' Restore and ghoul's Cannibalize; in the census, a creep
        // peasant's Repair; read, Forestwalk's creeps' Curse, Bloodlust, Faerie Fire, and
        // Raise Dead checks, their Animate Dead's and Mana Burn's looks).
        if (computer || (owner == crate::types::PLAYER_NEUTRAL_AGGRESSIVE && !building)) && !night_sleeper {
            for id in &list {
                if let Some(c @ (codes::ability::id::CANNIBALIZE | codes::ability::id::ANIMATE_DEAD_CREEP | codes::ability::id::DEMON_HUNTER_MANA_BURN)) = ability(id).as_ref() {
                    let name = match c {
                        codes::ability::id::CANNIBALIZE => "606220",
                        codes::ability::id::ANIMATE_DEAD_CREEP => "6060e0",
                        _ => "606c60",
                    };
                    let n = self.tags.start_timer(name);
                    if let Some(k) = codes.iter().position(|x| x == c) {
                        self.tags.timer_objects.entry(slots[k]).or_default().push((n, name.into(), self.time, 0.0));
                    }
                    continue;
                }
                match ability(id).as_ref() {
                    Some(c @ (codes::ability::id::REPAIR_ORC | codes::ability::id::RENEW | codes::ability::id::RESTORATION | codes::ability::id::HEAL | codes::ability::id::HEAL_CREEP_NORMAL | codes::ability::id::INNER_FIRE | codes::ability::id::REPLENISH_LIFE_AND_MANA | codes::ability::id::REPLENISH_LIFE | codes::ability::id::REPLENISH_MANA | codes::ability::id::CURSE | codes::ability::id::BLOODLUST | codes::ability::id::FAERIE_FIRE | codes::ability::id::RAISE_DEAD)) => {
                        let n = self.tags.start_timer("422820");
                        self.tags.autotarget_due.push(self.time + 0.015);
                        self.tags.autotarget_info.push((h, *c));
                        self.tags.autotarget_saved.push(f32::from_bits(crate::efloat::add(self.time.to_bits(), 0.02f32.to_bits())));
                        self.tags.autotarget_timers.insert((h, *c), (n, f32::from_bits(crate::efloat::add(self.time.to_bits(), 0.02f32.to_bits())), 0.02));
                    }
                    _ => {}
                }
            }
        }
        let births: Vec<u32> = slots.iter().map(|s| self.tags.live.get(s).map_or(0, |e| e.1)).collect();
        if let Some(k) = codes.iter().position(|c| c == codes::class::CREEP_CAMP) {
            self.tags.camp_ais.push((slots[k], births[k]));
        }
        let camp_ai = camp.and_then(|c| self.tags.camp_ais.get(c).copied());
        let site_ai = codes.iter().position(|c| c == codes::class::TOWN_SITE).map(|k| (slots[k], births[k]));
        if let Some(s) = site_ai {
            self.tags.sites.push((x, y, s));
        }
        // Its spells' listeners, each its ability's (the ability's first `rtl).
        let spell_owners: Vec<(u32, Code)> = spell_timers.iter().filter_map(|&k| codes[..k].iter().rposition(|c| c[0] == b'A').map(|a| (slots[k], codes[a]))).collect();
        // What it releases as it is made, each freed as the next turn starts, in this
        // order (traced in the original's event queue): a creep that falls asleep, its
        // autocast's order and its spells' timers, the last first; then the two tasks
        // of its making, the second first; then a creep's first AI orders and tasks.
        let sleeps = owner == crate::types::PLAYER_NEUTRAL_AGGRESSIVE
            && !building
            && o.int("canSleep") != 0
            && self.is_night()
            && self.players.get(owner as usize).is_none_or(|p| p.get_state(crate::types::player_state::NO_CREEP_SLEEP) == 0);
        // (A flyer's detection object goes with them, after its autocast's order: read,
        // Forestwalk's whelps, each its `` `pil `` then its making's tasks.)
        if sleeps {
            for k in autocast.into_iter().chain(flyer).chain(spell_timers.iter().rev().copied()) {
                self.tags.doom(slots[k]);
            }
        }
        for k in [tasks + 1, tasks] {
            self.tags.doom(slots[k]);
        }
        if let Some(a) = ai {
            for k in 0..extra {
                let e = a - 3 * (extra - k);
                for d in [1, 2, 0] {
                    self.tags.doom(slots[e + d]);
                }
            }
            for d in [1, 6, 5, 4, 3, 2, 0, 8, 7] {
                self.tags.doom(slots[a + d]);
            }
        }
        // Its look at the ground falls due on the main clock, from the tick's first
        // reading (none before the game runs).
        let base = if self.tick_clocks.1.is_empty() { 0.0 } else { self.making_clock.map(|c| c.0).or_else(|| self.tick_abs(0)).unwrap_or(0.0) as f32 };
        if let Some(u) = self.unit_mut(h) {
            u.extra.oid = oid;
            u.extra.spell_timers = spell_timers.iter().map(|&k| slots[k]).collect();
            u.extra.spell_owners = spell_owners;
            u.extra.attack_timer = attack_timer;
            // (Its first look waits the time drawn as it was made.)
            let wait = u.acquire_timer;
            u.extra.look_timer = look_timer.map(|n| (n, f32::from_bits(crate::efloat::add(now.to_bits(), wait.to_bits())), wait));
            u.extra.camp_ai = camp_ai;
            u.extra.site_ai = site_ai;
            // Blight growth's objects (Abli, Xbli, `pos, two ^pmr), released as its
            // dispel ends (`World::update_blight`).
            u.extra.blight_slots = blight.map(|b| slots[b..b + 5].to_vec()).unwrap_or_default();
            u.extra.footprint_slots = footprint.map(|f| slots[f..f + 3].to_vec()).unwrap_or_default();
            u.extra.flyer_pil = flyer.filter(|_| !sleeps).map(|k| slots[k]);
            u.extra.ai_slots = ai_refs.map(|k| vec![(slots[k], births[k]), (slots[k + 1], births[k + 1])]).unwrap_or_default();
            u.extra.item_pil = item_pil.map(|k| (slots[k], births[k]));
            u.extra.spell_pils = spell_pils.iter().map(|&(k, c)| (slots[k], c)).collect();
            u.extra.flyer_sleeping = flyer.is_some() && sleeps;
            let rest_start = mods_end + if building { 3 } else { 0 };
            let pair = |k: usize| (slots[k], births[k]);
            u.extra.object_slots = [(0..BASE).map(pair).collect(), (BASE..mods_end).map(pair).collect(), (rest_start..tasks).map(pair).collect()];
            if blight_regen {
                u.extra.blight_check = Some(base + 2.0);
                u.extra.blight_rcm = blight_rcm.map(|k| slots[k]);
            }
        }
    }
}

impl crate::world::World {
    /// A unit's mana was set: its spells' listeners each make a new one the first time,
    /// replacing those of its making (measured: Booty Bay's two Granite Golems take two
    /// after SetUnitState sets their mana to 0).
    pub(crate) fn mana_changed(&mut self, h: u32) {
        // (Read: the quiet test map's golems' Hurl Boulder and Thunder Clap, Harrow's mud
        // golems' Slow, and Plaguelands' creeps' spells, each their mana set to none by
        // the map's script.)  A creep asleep makes none, its spells' listeners gone with
        // its sleep (read: Emerald Gardens' Mur'gul Snarecasters, made asleep, their Slow
        // the mud golems'; Booty Bay's troll priests and ogre lord and Harrow's gnoll
        // wardens, made asleep, none).
        let Some(u) = self.unit(h).filter(|u| !u.extra.mana_watched && !u.extra.sleeping) else { return };
        let spells: Vec<(Code, f32)> = u.extra.spell_owners.iter().filter(|(s, _)| self.tags.live.contains_key(s)).filter_map(|&(_, c)| u.abilities.iter().find_map(|(a, _)| self.objects.abilities.get(*a).filter(|o| crate::objects::id_from_str(o.str("code").trim()) == c).map(|o| (c, o.level_real("Cost", 1))))).collect();
        if spells.is_empty() {
            return;
        }
        let costs: Vec<f32> = spells.iter().map(|s| s.1).collect();
        let codes: Vec<Code> = spells.iter().map(|s| s.0).collect();
        // The new timers replace the spells' first ones, released the last first.
        let old = u.extra.spell_timers.clone();
        // (Each made by its spell's event, `FUN_6f4256d0`, and the old let go,
        // `FUN_6f161800`: a count of the property module's either side of the new
        // listener's two; traced on the quiet test map's golems.)
        let new: Vec<u32> = costs
            .iter()
            .map(|_| {
                self.tags.bump_prop(1, "evnt");
                let s = self.tags.take(codes::class::THRESHOLD_LISTENER);
                self.tags.bump_prop(1, "lose");
                s
            })
            .collect();
        for &s in old.iter().rev() {
            self.tags.doom(s);
        }
        // They are listeners on the mana rising past each spell's cost, the last spell's
        // in the first slot (read: both golems' Hurl Boulder ones, 75.1 at 1.5 a second,
        // in their second slots, replaced at 50.07 seconds).
        if let Some(u) = self.unit_mut(h) {
            u.extra.mana_watched = true;
            u.extra.mana_listeners = costs.iter().map(|c| c + 0.1).zip(new.iter().rev().copied()).collect();
            u.extra.spell_owners = codes.into_iter().zip(new.iter().rev().copied()).map(|(c, s)| (s, c)).collect();
            u.extra.spell_timers = new;
        }
    }
}

impl crate::world::World {
    /// A new location handle, which the original tags (`+loc`).
    pub fn insert_location(&mut self, x: f32, y: f32) -> u32 {
        let slot = self.tags.take(codes::class::LOCATION);
        let h = self.handles.insert(crate::types::Obj::Location(x, y));
        self.tags.handle_slots.insert(h, slot);
        h
    }
}

impl crate::world::World {
    /// Whether an aura serves a unit of a type: Trueshot one with a ranged weapon,
    /// Vampiric one with a melee weapon, War Drums and Command one with a weapon,
    /// Brilliance one with mana (measured: on a sheep, without attack or mana, those
    /// gave none; read: Trueshot's buff on no wandering critter as it passed).
    pub(crate) fn aura_serves(&self, code: &Code, type_id: Code) -> bool {
        let Some(o) = self.objects.units.get(type_id) else { return false };
        let weapons = o.int("weapsOn");
        let ranged_weapon = |k: &str| !matches!(o.str(k).trim(), "normal" | "instant" | "_" | "");
        let ranged = ranged_weapon("weapTp1");
        match code {
            // Trueshot reaches a unit with a ranged weapon on, its first or its second
            // (read: Harrow's troll warlords' buff on the granite golems of their camps,
            // their second weapon a missile; Twisted Meadows' on the trolls of their
            // camps and not the ogres).
            codes::ability::id::PRIESTESS_TRUESHOT_AURA => weapons & 1 != 0 && ranged || weapons & 2 != 0 && ranged_weapon("weapTp2"),
            codes::ability::id::DREADLORD_VAMPIRIC_AURA => weapons != 0 && !ranged,
            codes::ability::id::AURA_WAR_DRUMS | codes::ability::id::COMMAND_AURA => weapons != 0,
            codes::ability::id::ARCH_MAGE_BRILLIANCE_AURA => o.real("manaN") > 0.0,
            _ => true,
        }
    }

    /// The codes of an ability a unit of a type gains, and of the objects it makes; an
    /// aura's buff on its bearer only when the aura serves it (measured: on a sheep,
    /// without attack or mana, Trueshot, Vampiric, War Drums, Command, and Brilliance
    /// Aura gave none).
    pub(crate) fn ability_tags(&self, type_id: Code, id: &Code) -> Vec<Code> {
        let Some(a) = self.objects.abilities.get(*id) else { return Vec::new() };
        let code = crate::objects::id_from_str(a.str("code").trim());
        if self.objects.units.get(type_id).is_none() {
            return Vec::new();
        }
        let objects = AbilityObjects::get().objects(id, &code);
        let buff = Some(crate::objects::id_from_str(a.level_str("BuffID", 1).trim())).filter(|b| b[0] != 0);
        let serves = self.aura_serves(&code, type_id);
        let aura = matches!(&code, codes::ability::id::PALADIN_DEVOTION_AURA | codes::ability::id::ARCH_MAGE_BRILLIANCE_AURA | codes::ability::id::DEATH_KNIGHT_UNHOLY_AURA | codes::ability::id::CHIEFTAIN_ENDURANCE_AURA | codes::ability::id::PRIESTESS_TRUESHOT_AURA | codes::ability::id::DREADLORD_VAMPIRIC_AURA | codes::ability::id::KEEPER_THORNS_AURA | codes::ability::id::AURA_WAR_DRUMS | codes::ability::id::COMMAND_AURA);
        // A morph's object on the unit it makes (measured: Amil's buff Bmil on a militia,
        // Aphx's Bphx on a phoenix egg, and Stone Form's regeneration timer on a gargoyle
        // of stone).
        let made = match &code {
            codes::ability::id::MILITIA => crate::objects::id_from_str(a.level_str("DataB", 1).trim()) == type_id,
            codes::ability::id::PHOENIX | codes::ability::id::STONE_FORM => crate::objects::id_from_str(a.level_str("UnitID", 1).trim()) == type_id,
            _ => false,
        };
        let mut codes = vec![code];
        let mut k = 0;
        while k < objects.len() {
            let c = objects[k];
            if aura && Some(c) == buff {
                // The buff and its own objects, up to the next of the ability's.
                let end = objects[k + 1..].iter().position(|c| c[0] == b'A' || c[0] == b'X').map_or(objects.len(), |n| k + 1 + n);
                if serves {
                    codes.extend_from_slice(&objects[k..end]);
                }
                k = end;
                continue;
            }
            codes.push(c);
            k += 1;
            // An aura measured without its buff (the sheep it was measured on did not
            // take it) gives it after its own object when it serves the bearer.
            if aura && serves && k == 1 && c == *codes::class::IN_RANGE_LISTENER && !objects.iter().any(|o| Some(*o) == buff) {
                if let Some(b) = buff {
                    codes.push(b);
                    // Brilliance's buff has its own (measured: a dark wizard's).
                    if b == *codes::buff::id::BRILLIANCE_AURA {
                        codes.push(*codes::class::CONTINUOUS_MODIFIER);
                    }
                }
            }
        }
        if made {
            codes.push(match &code {
                codes::ability::id::MILITIA => *codes::buff::id::MILITIA,
                codes::ability::id::PHOENIX => *codes::buff::id::PHOENIX,
                _ => *codes::class::CONTINUOUS_MODIFIER,
            });
        }
        codes
    }
}
