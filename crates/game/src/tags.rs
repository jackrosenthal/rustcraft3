//! The original engine's tags: every object it makes (a game object, an ability, a
//! script handle, and the engine's own regions, positions, and properties) takes the
//! next tag from its object manager (Tempest's, `DAT_6fd6860c`).  A tag names an object
//! in the network's actions (as two words), and the tags given so far go into the
//! game's checksum, so this game counts the same tags at the same moments
//! (`docs/reference/bootybay_tags.txt` is the original's sequence for a map's start).

use serde::{Deserialize, Serialize};

/// A type code as the original keeps it at +0x10 of each tagged object (`+w3u` a unit,
/// `^pmr` a region, `Amov` an ability, ...).
pub type Code = [u8; 4];

/// While set, a new world records its tags' codes (the `tags` example sets it).
pub static RECORD: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Tags {
    /// Tags given so far (the next presence tag, while none has been given back).
    pub next: u32,
    /// The objects' codes in the order tagged, while recording (for comparing with the
    /// original's).
    #[serde(skip)]
    pub log: Option<Vec<Code>>,
    /// Where each creep camp's first creep was made (the camps' AI takes tags).
    #[serde(default)]
    pub camps: Vec<(f32, f32)>,
    /// The script functions with a handler, which the original makes the first time a
    /// function is given to an object (an action, a filter, a timer; `FUN_6f2872f0`).
    #[serde(default)]
    pub handlers: std::collections::BTreeSet<u32>,
    /// Those given to timers, which have their own (measured: Melee's alliance-change
    /// action, already a trigger's, takes another handler when a timer is started
    /// with it).
    #[serde(default)]
    pub timer_handlers: std::collections::BTreeSet<u32>,
    /// Objects the engine makes right after the next one tagged (`after_next`).
    #[serde(skip)]
    pending: Vec<Code>,
}

impl Tags {
    pub fn new() -> Tags {
        Tags { next: 0, log: RECORD.load(std::sync::atomic::Ordering::Relaxed).then(Vec::new), camps: Vec::new(), handlers: Default::default(), timer_handlers: Default::default(), pending: Vec::new() }
    }

    /// The next tag, for an object of type `code`.
    pub fn take(&mut self, code: &Code) -> u32 {
        let t = self.next;
        self.next += 1;
        if let Some(l) = self.log.as_mut() {
            l.push(*code);
        }
        self.flush();
        t
    }

    /// Objects the engine makes right after the next object tagged.
    pub fn after_next(&mut self, codes: &[&Code]) {
        self.pending = codes.iter().map(|c| **c).collect();
    }

    /// The objects `after_next` holds, if no object has been tagged since.
    pub fn flush(&mut self) {
        for c in std::mem::take(&mut self.pending) {
            self.take(&c);
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

    /// The handler of a script function given to a timer, the first time.
    pub fn timer_handler(&mut self, func: u32) {
        if self.timer_handlers.insert(func) {
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
        let mut had: Vec<Code> = Vec::new();
        for id in &list {
            let Some(code) = ability(id) else { continue };
            if had.contains(&code) {
                continue;
            }
            had.push(code);
            codes.extend(self.ability_tags(type_id, id));
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
            codes.push(*b"+ord");
        }
        if o.int("canSleep") != 0 && !building {
            codes.push(*b"ACsp");
        }
        if building {
            codes.push(*b"Aalr");
        }
        codes.extend([*b"task", *b"task"]);
        // A gold mine is a town site for computer players.
        if had.iter().any(|c| c == b"Agld" || c == b"Aegm") {
            codes.push(*b"AIts");
        }
        // A creep joins the nearest camp whose first creep is near enough, or makes one,
        // and takes its AI.
        if owner == crate::types::PLAYER_NEUTRAL_AGGRESSIVE && !building {
            let reach = CAMP_CELLS * 32.0;
            if !self.tags.camps.iter().any(|&(cx, cy)| (cx - x).hypot(cy - y) < reach) {
                self.tags.camps.push((x, y));
                codes.push(*b"AIcs");
            }
            codes.extend([*b"uref", *b"AI-U", *b"+ord", *b"tskA", *b"task", *b"task", *b"task", *b"task", *b"tskA", *b"+ord", *b"task"]);
        }
        for c in &codes {
            self.tags.take(c);
        }
    }
}

impl crate::world::World {
    /// A unit's mana was set: its creep Hurl Boulder and Thunder Clap (ACtb, ACtc) each
    /// make a timer the first time (measured: Booty Bay's two Granite Golems take two
    /// after SetUnitState sets their mana to 0, and creeps with other spells none).
    pub(crate) fn mana_changed(&mut self, h: u32) {
        let Some(u) = self.unit(h).filter(|u| !u.extra.mana_watched) else { return };
        let n = u.abilities.iter().filter(|(a, _)| self.objects.abilities.get(*a).is_some_and(|o| matches!(o.str("code").trim(), "ACtb" | "ACtc"))).count();
        if n == 0 {
            return;
        }
        self.tags.take_n(b"`rtl", n);
        if let Some(u) = self.unit_mut(h) {
            u.extra.mana_watched = true;
        }
    }
}

impl crate::world::World {
    /// A new location handle, which the original tags (`+loc`).
    pub fn insert_location(&mut self, x: f32, y: f32) -> u32 {
        self.tags.take(b"+loc");
        self.handles.insert(crate::types::Obj::Location(x, y))
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
