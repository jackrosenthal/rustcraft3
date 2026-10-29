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
    /// The script functions with a handler, which the original makes the first time a
    /// function is given to an object (an action, a filter, a timer; `FUN_6f2872f0`).
    #[serde(default)]
    pub handlers: std::collections::BTreeSet<u32>,
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
    /// Game state events' timers' slots, by event handle.
    #[serde(default)]
    pub event_timers: std::collections::BTreeMap<u32, u32>,
    /// The engine's day timer's slot (the `rtl among its objects at load, replaced at
    /// each change between day and night).
    #[serde(default)]
    pub day_timer: Option<u32>,
}

impl Tags {
    pub fn new() -> Tags {
        let rec = RECORD.load(std::sync::atomic::Ordering::Relaxed);
        Tags { slots: 0, free: Vec::new(), born: 0, trace: rec.then(Vec::new), log: rec.then(Vec::new), camps: Vec::new(), handlers: Default::default(), pending: Vec::new(), doomed: Vec::new(), next_turn: Vec::new(), step_paths: Vec::new(), step_frees: None, issued: Vec::new(), timed: Vec::new(), handle_slots: Default::default(), handle_ids: Default::default(), event_timers: Default::default(), day_timer: None }
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
    }

    /// Queues a slot's release at a game time.
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
        self.free_doomed();
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

    /// Queues a script handle's destruction, when it has a slot.
    pub fn doom_handle(&mut self, h: u32) {
        if let Some(s) = self.handle_slots.remove(&h) {
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
        const CODES: [&Code; 18] = [b"ordt", b"tskA", b"tskA", b"tskO", b"task", b"task", b"task", b"tsk.", b"task", b"task", b"task", b"task", b"task", b"task", b"tskA", b"^clg", b"^clu", b"l^pa"];
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
    /// order's are.
    pub fn player_move_issued(&mut self) -> Vec<u32> {
        const CODES: [&Code; 17] = [b"ordt", b"tskA", b"tskA", b"tskO", b"task", b"task", b"task", b"tsk.", b"task", b"task", b"task", b"task", b"task", b"tskA", b"^clg", b"^clu", b"l^pa"];
        let slots: Vec<u32> = CODES.iter().map(|c| self.take(c)).collect();
        self.release(slots[14]);
        for k in [1, 13, 12, 11, 10, 9, 8] {
            self.doom(slots[k]);
        }
        slots
    }

    /// A unit's order of several sent together to a point (traced: five peons): as
    /// `player_move_issued`'s without the cluster and path, 14 objects, its blocks and
    /// five tasks as the next turn starts.
    pub fn member_move_issued(&mut self) -> Vec<u32> {
        const CODES: [&Code; 14] = [b"ordt", b"tskA", b"tskA", b"tskO", b"task", b"task", b"task", b"tsk.", b"task", b"task", b"task", b"task", b"task", b"tskA"];
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
        let a = self.take(b"tskA");
        let b = self.take(b"task");
        let c = self.take(b"task");
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
            if c == *b"`rtl" && self.day_timer.is_none() {
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
            self.take(b"hdlr");
        }
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

        let mut codes: Vec<Code> = vec![*b"+w3u", *b"`pcu", *b"^pmr", *b"^pmr", *b"`rel", *b"`rel", *b"`rel", *b"`rel", *b"`rel", *b"l^pa"];
        // Life regeneration: a timer, or for regeneration on blight its ability.
        match field("regenType").to_ascii_lowercase().as_str() {
            "always" | "night" | "day" => codes.push(*b"`rcm"),
            // Its timer too when made on blight (measured: undead made where the
            // test's undead buildings had spread blight had one, those made before
            // them none).
            "blight" => {
                codes.push(*b"Ablr");
                if self.point_blighted(x, y) {
                    codes.push(*b"`rcm");
                }
            }
            _ => {}
        }
        codes.push(*b"`rtl");
        if o.real("manaN") > 0.0 {
            codes.push(*b"`rcm");
        }
        let footprint = building.then_some(codes.len());
        if building {
            codes.extend([*b"^pmr"; 3]);
        }

        // The engine's abilities, as the type's lists call for them.
        let trains = ids("Trains").iter().any(is_unit);
        let researches = ids("Researches").iter().any(|c| self.objects.upgrades.get(*c).is_some());
        let revive = o.bool("Revive");
        if ids("Builds").iter().any(is_unit) {
            codes.push(match race.as_str() {
                "human" => *b"AHbu",
                "orc" => *b"AObu",
                "undead" => *b"AUbu",
                "nightelf" => *b"AEbu",
                "naga" => *b"AGbu",
                _ => *b"ANbu",
            });
            codes.push(*b"`rel");
        }
        if trains || researches || revive {
            codes.extend([*b"Aque", *b"`rel"]);
        }
        if ids("Makeitems").iter().any(|c| self.objects.items.get(*c).is_some()) {
            codes.extend([*b"Amai", *b"`rel"]);
        }
        if ids("Sellitems").iter().any(|c| self.objects.items.get(*c).is_some()) {
            codes.extend([*b"Asei", *b"`rel"]);
        }
        if ids("Sellunits").iter().any(is_unit) {
            codes.extend([*b"Asel", *b"`rel"]);
        }
        if ids("Upgrade").iter().any(is_unit) {
            codes.extend([*b"Aupg", *b"`rel"]);
        }
        if o.real("spd") != 0.0 {
            codes.push(*b"Amov");
        }
        let weapons = o.int("weapsOn");
        if weapons != 0 {
            codes.push(*b"Aatk");
            // One more object for each splashing weapon, enabled or not.
            for k in 1..=2 {
                if matches!(field(&format!("weapTp{k}")).as_str(), "msplash" | "artillery" | "aline" | "mline" | "mbounce") {
                    codes.push(*b"spla");
                }
            }
        }
        if type_id[0].is_ascii_uppercase() {
            codes.extend([*b"AHer", *b"`flg", *b"`flg", *b"`rcm", *b"`rcm", *b"`rcm", *b"`rcm"]);
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
            if objects.first() == Some(b"Abli") {
                blight = Some(at);
            }
            // A spell's timer (its first `rtl), which sleep and a mana change replace.
            if let Some(k) = objects.iter().position(|c| c == b"`rtl") {
                spell_timers.push(at + k);
            }
            codes.extend(objects);
        }
        // Rally (when it trains or revives), the inventory of a hero of an old map, fire
        // (a structure or a machine), detection of the burrowed (a flyer), the order for
        // its own autocast ability, creep sleep, and the alarm (a structure).
        if (trains || revive) && !had.contains(b"ARal") {
            codes.push(*b"ARal");
        }
        if type_id[0].is_ascii_uppercase() && self.editor_version < 6000 && !had.contains(b"AInv") {
            codes.push(*b"AInv");
        }
        if building || mechanical {
            codes.push(match race.as_str() {
                "orc" => *b"Afio",
                "undead" => *b"Afiu",
                "nightelf" => *b"Afin",
                _ => *b"Afih",
            });
        }
        if field("movetp").eq_ignore_ascii_case("fly") && !had.contains(b"Abdt") {
            codes.extend([*b"Abdt", *b"`pil"]);
        }
        let auto = crate::objects::id_from_str(field("auto").as_str());
        // (The type's auto names the ability by its code: nfsh's Anhe for its Anh2.)  It
        // is not turned on while its research is missing (FUN_6f48b230 with
        // FUN_6f48f500; measured: a Shaman's Bloodlust, which requires Rost, has none).
        let auto_ability = list.iter().find(|id| ability(id) == Some(auto));
        if auto[0] != 0 && auto_ability.is_some_and(|a| self.ability_enabled(owner, *a)) {
            autocast = Some(codes.len());
            codes.push(*b"+ord");
        }
        if o.int("canSleep") != 0 && !building {
            codes.push(*b"ACsp");
        }
        if building {
            codes.push(*b"Aalr");
        }
        let tasks = codes.len();
        codes.extend([*b"task", *b"task"]);
        // A gold mine is a town site for computer players (not the one a haunted mine
        // hides: measured, CreateBlightedGoldmine's has none).
        if had.iter().any(|c| c == b"Agld" || c == b"Aegm") && !self.hidden_mine {
            codes.push(*b"AIts");
        }
        // A creep joins the nearest camp whose first creep is near enough, or makes one,
        // and takes its AI.
        // A computer's units, structures among them, take the same AI (measured under
        // Wine against an undead computer: its Necropolis, Haunted Gold Mine, and
        // workers each have it after their tasks).
        let computer = self.players.get(owner as usize).is_some_and(|p| p.controller == crate::types::map_control::COMPUTER && p.slot_state == crate::types::slot_state::PLAYING);
        if owner == crate::types::PLAYER_NEUTRAL_AGGRESSIVE && !building {
            let reach = CAMP_CELLS * 32.0;
            if !self.tags.camps.iter().any(|&(cx, cy)| (cx - x).hypot(cy - y) < reach) {
                self.tags.camps.push((x, y));
                codes.push(*b"AIcs");
            }
        }
        // A worker's AI first orders each of its owner's idle workers, itself among
        // them (FUN_6f9c3660 through FUN_6f9c9cc0: FUN_6f6802f0 for each with no order;
        // measured: an undead computer's three acolytes take one, two, and three such
        // orders).  A unit with an inventory then takes its item pickup range (its
        // abilities' AI start, FUN_6f67dff0; AInv's at FUN_6f606a20).
        let mut extra = 0;
        if (owner == crate::types::PLAYER_NEUTRAL_AGGRESSIVE && !building) || computer {
            codes.extend([*b"uref", *b"AI-U"]);
            if self.unit_class(self.unit(h).unwrap(), "peon") {
                extra = self.unit_order.iter().filter_map(|&v| self.unit(v)).filter(|v| v.owner == owner && !v.dead && v.order.id == 0 && self.unit_class(v, "peon")).count();
                for _ in 0..extra {
                    codes.extend([*b"+ord", *b"tskA", *b"task"]);
                }
            }
            ai = Some(codes.len());
            codes.extend([*b"+ord", *b"tskA", *b"task", *b"task", *b"task", *b"task", *b"tskA", *b"+ord", *b"task"]);
            if codes.contains(b"AInv") {
                codes.push(*b"`pil");
            }
        }
        let oid = self.tags.take_id(&codes[0]);
        let mut slots = vec![oid.0];
        for c in &codes[1..] {
            slots.push(self.tags.take(c));
        }
        // What it releases as it is made, each freed as the next turn starts, in this
        // order (traced in the original's event queue): a creep that falls asleep, its
        // autocast's order and its spells' timers, the last first; then the two tasks
        // of its making, the second first; then a creep's first AI orders and tasks.
        let sleeps = owner == crate::types::PLAYER_NEUTRAL_AGGRESSIVE
            && !building
            && o.int("canSleep") != 0
            && self.is_night()
            && self.players.get(owner as usize).is_none_or(|p| p.get_state(crate::types::player_state::NO_CREEP_SLEEP) == 0);
        if sleeps {
            for k in autocast.into_iter().chain(spell_timers.iter().rev().copied()) {
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
        if let Some(u) = self.unit_mut(h) {
            u.extra.oid = oid;
            u.extra.spell_timers = spell_timers.iter().map(|&k| slots[k]).collect();
            // Blight growth's objects (Abli, Xbli, `pos, two ^pmr), released as its
            // dispel ends (`World::update_blight`).
            u.extra.blight_slots = blight.map(|b| slots[b..b + 5].to_vec()).unwrap_or_default();
            u.extra.footprint_slots = footprint.map(|f| slots[f..f + 3].to_vec()).unwrap_or_default();
        }
    }
}

impl crate::world::World {
    /// A unit's mana was set: its creep Hurl Boulder and Thunder Clap (ACtb, ACtc) each
    /// make a timer the first time (measured: Booty Bay's two Granite Golems take two
    /// after SetUnitState sets their mana to 0, and creeps with other spells none).
    pub(crate) fn mana_changed(&mut self, h: u32) {
        let Some(u) = self.unit(h).filter(|u| !u.extra.mana_watched) else { return };
        let costs: Vec<f32> = u.abilities.iter().filter_map(|(a, _)| self.objects.abilities.get(*a).filter(|o| matches!(o.str("code").trim(), "ACtb" | "ACtc")).map(|o| o.level_real("Cost", 1))).collect();
        if costs.is_empty() {
            return;
        }
        // The new timers replace the spells' first ones, released the last first.
        let old = u.extra.spell_timers.clone();
        let new: Vec<u32> = costs.iter().map(|_| self.tags.take(b"`rtl")).collect();
        for &s in old.iter().rev() {
            self.tags.doom(s);
        }
        // They are listeners on the mana rising past each spell's cost, the last spell's
        // in the first slot (read: both golems' Hurl Boulder ones, 75.1 at 1.5 a second,
        // in their second slots, replaced at 50.07 seconds).
        if let Some(u) = self.unit_mut(h) {
            u.extra.mana_watched = true;
            u.extra.mana_listeners = costs.iter().map(|c| c + 0.1).zip(new.iter().rev().copied()).collect();
            u.extra.spell_timers = new;
        }
    }
}

impl crate::world::World {
    /// A new location handle, which the original tags (`+loc`).
    pub fn insert_location(&mut self, x: f32, y: f32) -> u32 {
        let slot = self.tags.take(b"+loc");
        let h = self.handles.insert(crate::types::Obj::Location(x, y));
        self.tags.handle_slots.insert(h, slot);
        h
    }
}

impl crate::world::World {
    /// The codes of an ability a unit of a type gains, and of the objects it makes; an
    /// aura's buff on its bearer only when the aura serves it (measured: on a sheep,
    /// without attack or mana, Trueshot, Vampiric, War Drums, Command, and Brilliance
    /// Aura gave none).
    pub(crate) fn ability_tags(&self, type_id: Code, id: &Code) -> Vec<Code> {
        let Some(a) = self.objects.abilities.get(*id) else { return Vec::new() };
        let code = crate::objects::id_from_str(a.str("code").trim());
        let Some(o) = self.objects.units.get(type_id) else { return Vec::new() };
        let weapons = o.int("weapsOn");
        let ranged = !matches!(o.str("weapTp1").trim(), "normal" | "instant" | "_" | "");
        let objects = AbilityObjects::get().objects(id, &code);
        let buff = Some(crate::objects::id_from_str(a.level_str("BuffID", 1).trim())).filter(|b| b[0] != 0);
        let serves = match &code {
            b"AEar" => weapons != 0 && ranged,
            b"AUav" => weapons != 0 && !ranged,
            b"Aakb" | b"AOac" => weapons != 0,
            b"AHab" => o.real("manaN") > 0.0,
            _ => true,
        };
        let aura = matches!(&code, b"AHad" | b"AHab" | b"AUau" | b"AOae" | b"AEar" | b"AUav" | b"AEah" | b"Aakb" | b"AOac");
        // A morph's object on the unit it makes (measured: Amil's buff Bmil on a militia,
        // Aphx's Bphx on a phoenix egg, and Stone Form's regeneration timer on a gargoyle
        // of stone).
        let made = match &code {
            b"Amil" => crate::objects::id_from_str(a.level_str("DataB", 1).trim()) == type_id,
            b"Aphx" | b"Astn" => crate::objects::id_from_str(a.level_str("UnitID", 1).trim()) == type_id,
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
            if aura && serves && k == 1 && c == *b"`pil" && !objects.iter().any(|o| Some(*o) == buff) {
                if let Some(b) = buff {
                    codes.push(b);
                    // Brilliance's buff has its own (measured: a dark wizard's).
                    if b == *b"BHab" {
                        codes.push(*b"`rcm");
                    }
                }
            }
        }
        if made {
            codes.push(match &code {
                b"Amil" => *b"Bmil",
                b"Aphx" => *b"Bphx",
                _ => *b"`rcm",
            });
        }
        codes
    }
}
