//! Object data: units, items, abilities, buffs, upgrades, destructables, and doodads, merged
//! from the SLK tables, the profile text files, and a map's object modifications.

use std::collections::HashMap;

use w3_formats::profile::{Profile, split_list, unquote};
use w3_formats::slk::Table;
use w3_formats::{Error, Reader};
use w3_mpq::Chain;

/// Object ids are four-character codes.
pub type Id = [u8; 4];

pub fn id_str(id: Id) -> String {
    String::from_utf8_lossy(&id).into_owned()
}

pub fn id_from_str(s: &str) -> Id {
    let mut out = [0u8; 4];
    for (o, b) in out.iter_mut().zip(s.bytes()) {
        *o = b;
    }
    out
}

pub fn id_from_int(v: i32) -> Id {
    (v as u32).to_be_bytes()
}

pub fn id_to_int(id: Id) -> i32 {
    u32::from_be_bytes(id) as i32
}

#[derive(Clone, Debug, Default)]
pub struct Object {
    pub id: Id,
    /// The object this one was copied from, for custom objects.
    pub base: Option<Id>,
    /// Lower-cased field name -> raw value.
    pub fields: HashMap<String, String>,
}

/// "-" and "_" mark empty cells in the tables.
fn is_empty_cell(v: &str) -> bool {
    v.is_empty() || v == "-" || v == "_"
}

impl Object {
    pub fn raw(&self, field: &str) -> Option<&str> {
        self.fields.get(&field.to_ascii_lowercase()).map(String::as_str)
    }

    pub fn str(&self, field: &str) -> String {
        match self.raw(field) {
            Some(v) if !is_empty_cell(v) => unquote(v).to_string(),
            _ => String::new(),
        }
    }

    pub fn list(&self, field: &str) -> Vec<String> {
        match self.raw(field) {
            Some(v) if !is_empty_cell(v) => split_list(v).into_iter().filter(|s| !s.is_empty()).collect(),
            _ => Vec::new(),
        }
    }

    pub fn int(&self, field: &str) -> i32 {
        let s = self.str(field);
        let t = s.trim();
        t.parse::<i32>()
            .or_else(|_| t.parse::<f64>().map(|f| f as i32))
            .unwrap_or_else(|_| match t.to_ascii_lowercase().as_str() {
                "true" => 1,
                _ => 0,
            })
    }

    pub fn real(&self, field: &str) -> f32 {
        self.str(field).trim().parse().unwrap_or(0.0)
    }

    pub fn bool(&self, field: &str) -> bool {
        self.int(field) != 0
    }

    /// A per-level field such as `Cool1` or `DataA2`; levels start at 1.
    pub fn level_str(&self, field: &str, level: u32) -> String {
        self.str(&format!("{field}{level}"))
    }

    pub fn level_real(&self, field: &str, level: u32) -> f32 {
        self.real(&format!("{field}{level}"))
    }

    pub fn level_int(&self, field: &str, level: u32) -> i32 {
        self.int(&format!("{field}{level}"))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Unit,
    Item,
    Destructable,
    Doodad,
    Ability,
    Buff,
    Upgrade,
}

#[derive(Default)]
pub struct ObjectTable {
    pub objects: HashMap<Id, Object>,
    /// Object-editor field id -> (field name, slk, index, repeat/data info).
    meta: HashMap<Id, Meta>,
}

#[derive(Clone, Debug)]
struct Meta {
    field: String,
    slk: String,
    index: i32,
    repeat: i32,
    data: i32,
}

impl ObjectTable {
    pub fn get(&self, id: Id) -> Option<&Object> {
        self.objects.get(&id)
    }

    fn merge_slk(&mut self, table: &Table) {
        for (key, row) in &table.rows {
            let id = id_from_str(key);
            let obj = self.objects.entry(id).or_insert_with(|| Object { id, ..Default::default() });
            for (col, v) in table.columns.iter().zip(row) {
                if let Some(v) = v {
                    if col.is_empty() {
                        continue;
                    }
                    obj.fields.insert(col.to_ascii_lowercase(), v.as_str());
                }
            }
        }
    }

    fn merge_profile(&mut self, profile: &Profile, only_existing: bool) {
        // Profile section names are object ids; the profile parser lower-cases them, so
        // match case-insensitively against known objects.
        let mut by_lower: HashMap<String, Vec<Id>> = HashMap::new();
        for id in self.objects.keys() {
            by_lower.entry(id_str(*id).to_ascii_lowercase()).or_default().push(*id);
        }
        for (section, keys) in &profile.sections {
            if section.len() != 4 {
                continue;
            }
            let candidates: Vec<Id> = by_lower.get(section).cloned().unwrap_or_default();
            let ids = if candidates.is_empty() && !only_existing {
                vec![id_from_str(section)]
            } else {
                candidates
            };
            for id in ids {
                let obj = self.objects.entry(id).or_insert_with(|| Object { id, ..Default::default() });
                for (k, v) in keys {
                    obj.fields.insert(k.clone(), v.clone());
                }
            }
        }
    }

    fn load_meta(&mut self, table: &Table) {
        for (key, _) in &table.rows {
            let get = |c: &str| table.get(key, c).map(|v| v.as_str()).unwrap_or_default();
            self.meta.insert(
                id_from_str(key),
                Meta {
                    field: get("field"),
                    slk: get("slk"),
                    index: get("index").parse().unwrap_or(-1),
                    repeat: get("repeat").parse().unwrap_or(0),
                    data: get("data").parse().unwrap_or(0),
                },
            );
        }
    }

    /// Applies a map's object modification file (`war3map.w3u` and friends).  `leveled`
    /// is true for the formats that carry level and data-column fields (abilities,
    /// doodads, upgrades).
    pub fn apply_mods(&mut self, data: &[u8], leveled: bool) -> Result<(), Error> {
        let mut r = Reader::new(data);
        let _version = r.u32()?;
        for custom in [false, true] {
            if r.remaining() < 4 {
                break;
            }
            let n = r.u32()?;
            for _ in 0..n {
                let original = r.tag()?;
                let new = r.tag()?;
                let id = if custom { new } else { original };
                let mut obj = if custom {
                    let mut o = self.objects.get(&original).cloned().unwrap_or_default();
                    o.base = Some(original);
                    o
                } else {
                    self.objects.get(&original).cloned().unwrap_or_default()
                };
                obj.id = id;
                let mods = r.u32()?;
                for _ in 0..mods {
                    let field = r.tag()?;
                    let (level, column) = if leveled { (r.u32()?, r.u32()?) } else { (0, 0) };
                    let ty = r.u32()?;
                    let value = match ty {
                        0 => r.i32()?.to_string(),
                        1 | 2 => r.f32()?.to_string(),
                        _ => r.cstr()?,
                    };
                    let _end = r.u32()?;
                    let Some(meta) = self.meta.get(&field) else { continue };
                    let mut name = meta.field.clone();
                    if meta.data > 0 {
                        name = format!("{name}{}", (b'A' + (column as u8).saturating_sub(1)) as char);
                    }
                    if meta.repeat > 0 && level > 0 {
                        name = format!("{name}{level}");
                    }
                    let key = name.to_ascii_lowercase();
                    if meta.index >= 0 && meta.slk.eq_ignore_ascii_case("Profile") {
                        // A single entry of a comma-separated profile list.
                        let mut items = obj.fields.get(&key).map(|v| split_list(v)).unwrap_or_default();
                        let i = meta.index as usize;
                        if items.len() <= i {
                            items.resize(i + 1, String::new());
                        }
                        items[i] = value;
                        obj.fields.insert(key, items.join(","));
                    } else {
                        obj.fields.insert(key, value);
                    }
                }
                self.objects.insert(id, obj);
            }
        }
        Ok(())
    }
}

/// All object tables for a game.
#[derive(Default)]
pub struct ObjectDb {
    pub units: ObjectTable,
    pub items: ObjectTable,
    pub destructables: ObjectTable,
    pub doodads: ObjectTable,
    pub abilities: ObjectTable,
    pub buffs: ObjectTable,
    pub upgrades: ObjectTable,
}

fn slk(chain: &Chain, path: &str) -> Table {
    match chain.read_string(path) {
        Ok(s) => Table::parse(&s),
        Err(_) => Table::default(),
    }
}

/// Profile files in the order game.dll lists them (file offsets 0xab4f3c-0xab5474).
pub const PROFILE_FILES: &[&str] = &[
    "Units\\CampaignUnitStrings.txt",
    "Units\\HumanUnitStrings.txt",
    "Units\\NeutralUnitStrings.txt",
    "Units\\NightElfUnitStrings.txt",
    "Units\\OrcUnitStrings.txt",
    "Units\\UndeadUnitStrings.txt",
    "Units\\UnitGlobalStrings.txt",
    "Units\\CampaignUnitFunc.txt",
    "Units\\HumanUnitFunc.txt",
    "Units\\NeutralUnitFunc.txt",
    "Units\\NightElfUnitFunc.txt",
    "Units\\OrcUnitFunc.txt",
    "Units\\UndeadUnitFunc.txt",
    "Units\\CampaignAbilityStrings.txt",
    "Units\\CommonAbilityStrings.txt",
    "Units\\HumanAbilityStrings.txt",
    "Units\\NeutralAbilityStrings.txt",
    "Units\\NightElfAbilityStrings.txt",
    "Units\\OrcAbilityStrings.txt",
    "Units\\UndeadAbilityStrings.txt",
    "Units\\ItemAbilityStrings.txt",
    "Units\\CampaignAbilityFunc.txt",
    "Units\\CommonAbilityFunc.txt",
    "Units\\HumanAbilityFunc.txt",
    "Units\\NeutralAbilityFunc.txt",
    "Units\\NightElfAbilityFunc.txt",
    "Units\\OrcAbilityFunc.txt",
    "Units\\UndeadAbilityFunc.txt",
    "Units\\ItemAbilityFunc.txt",
    "Units\\CampaignUpgradeStrings.txt",
    "Units\\HumanUpgradeStrings.txt",
    "Units\\NightElfUpgradeStrings.txt",
    "Units\\OrcUpgradeStrings.txt",
    "Units\\UndeadUpgradeStrings.txt",
    "Units\\NeutralUpgradeStrings.txt",
    "Units\\CampaignUpgradeFunc.txt",
    "Units\\HumanUpgradeFunc.txt",
    "Units\\NightElfUpgradeFunc.txt",
    "Units\\OrcUpgradeFunc.txt",
    "Units\\UndeadUpgradeFunc.txt",
    "Units\\NeutralUpgradeFunc.txt",
    "Units\\CommandStrings.txt",
    "Units\\ItemStrings.txt",
    "Units\\CommandFunc.txt",
    "Units\\ItemFunc.txt",
];

impl ObjectDb {
    pub fn load(chain: &Chain) -> ObjectDb {
        let mut profile = Profile::default();
        for f in PROFILE_FILES {
            if let Ok(s) = chain.read_string(f) {
                profile.merge(&s);
            }
        }
        // A map's war3map.wts-resolved overrides arrive later through apply_mods.
        let mut db = ObjectDb::default();
        for t in ["Units\\UnitData.slk", "Units\\UnitUI.slk", "Units\\UnitBalance.slk", "Units\\UnitAbilities.slk", "Units\\UnitWeapons.slk"] {
            db.units.merge_slk(&slk(chain, t));
        }
        db.items.merge_slk(&slk(chain, "Units\\ItemData.slk"));
        db.destructables.merge_slk(&slk(chain, "Units\\DestructableData.slk"));
        db.doodads.merge_slk(&slk(chain, "Doodads\\Doodads.slk"));
        db.abilities.merge_slk(&slk(chain, "Units\\AbilityData.slk"));
        db.buffs.merge_slk(&slk(chain, "Units\\AbilityBuffData.slk"));
        db.upgrades.merge_slk(&slk(chain, "Units\\UpgradeData.slk"));
        for t in [&mut db.units, &mut db.items, &mut db.abilities, &mut db.buffs, &mut db.upgrades] {
            t.merge_profile(&profile, true);
        }
        let unit_meta = slk(chain, "Units\\UnitMetaData.slk");
        db.units.load_meta(&unit_meta);
        db.items.load_meta(&unit_meta);
        db.abilities.load_meta(&slk(chain, "Units\\AbilityMetaData.slk"));
        db.buffs.load_meta(&slk(chain, "Units\\AbilityBuffMetaData.slk"));
        db.upgrades.load_meta(&slk(chain, "Units\\UpgradeMetaData.slk"));
        db.destructables.load_meta(&slk(chain, "Units\\DestructableMetaData.slk"));
        db.doodads.load_meta(&slk(chain, "Doodads\\DoodadMetaData.slk"));
        db
    }

    /// Applies the map's object modification files, if present.
    pub fn apply_map(&mut self, chain: &Chain) {
        let files: [(&str, &mut ObjectTable, bool); 7] = [
            ("war3map.w3u", &mut self.units, false),
            ("war3map.w3t", &mut self.items, false),
            ("war3map.w3b", &mut self.destructables, false),
            ("war3map.w3d", &mut self.doodads, true),
            ("war3map.w3a", &mut self.abilities, true),
            ("war3map.w3h", &mut self.buffs, false),
            ("war3map.w3q", &mut self.upgrades, true),
        ];
        for (name, table, leveled) in files {
            if let Ok(data) = chain.read(name) {
                if let Err(e) = table.apply_mods(&data, leveled) {
                    log::warn!("{name}: {e}");
                }
            }
        }
    }

    pub fn table(&self, kind: Kind) -> &ObjectTable {
        match kind {
            Kind::Unit => &self.units,
            Kind::Item => &self.items,
            Kind::Destructable => &self.destructables,
            Kind::Doodad => &self.doodads,
            Kind::Ability => &self.abilities,
            Kind::Buff => &self.buffs,
            Kind::Upgrade => &self.upgrades,
        }
    }
}
