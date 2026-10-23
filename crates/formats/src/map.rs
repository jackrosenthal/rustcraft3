//! Files inside a map archive: terrain (`war3map.w3e`), map info (`war3map.w3i`),
//! placed doodads (`war3map.doo`), placed units (`war3mapUnits.doo`), and trigger strings
//! (`war3map.wts`).

use std::collections::HashMap;

use crate::{Error, Reader};

pub type FourCC = [u8; 4];

pub fn fourcc_str(id: FourCC) -> String {
    String::from_utf8_lossy(&id).into_owned()
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Corner {
    pub ground_height: i16,
    pub water_level: u16,
    pub map_edge: bool,
    pub ramp: bool,
    pub blight: bool,
    pub water: bool,
    pub boundary: bool,
    pub ground_texture: u8,
    pub ground_variation: u8,
    pub cliff_variation: u8,
    pub cliff_texture: u8,
    pub layer_height: u8,
}

impl Corner {
    /// World-space height of the ground at this corner.
    pub fn height(&self) -> f32 {
        (self.ground_height as f32 - 8192.0) / 4.0 + (self.layer_height as f32 - 2.0) * 128.0
    }
    /// World-space height of the water surface, before the tileset's water offset.
    pub fn water_height(&self) -> f32 {
        (self.water_level as f32 - 8192.0) / 4.0
    }
}

#[derive(Clone, Debug, Default)]
pub struct Terrain {
    pub version: u32,
    pub tileset: u8,
    pub custom_tilesets: bool,
    pub ground_tilesets: Vec<FourCC>,
    pub cliff_tilesets: Vec<FourCC>,
    /// Corners per row (one more than the number of tiles).
    pub width: u32,
    pub height: u32,
    pub offset: [f32; 2],
    pub corners: Vec<Corner>,
}

impl Terrain {
    pub fn parse(data: &[u8]) -> Result<Terrain, Error> {
        let mut r = Reader::new(data);
        if &r.tag()? != b"W3E!" {
            return Err(Error::Format("not a W3E terrain file"));
        }
        let version = r.u32()?;
        let tileset = r.u8()?;
        let custom_tilesets = r.u32()? != 0;
        let n = r.u32()?;
        let ground_tilesets = (0..n).map(|_| r.tag()).collect::<Result<_, _>>()?;
        let n = r.u32()?;
        let cliff_tilesets = (0..n).map(|_| r.tag()).collect::<Result<_, _>>()?;
        let width = r.u32()?;
        let height = r.u32()?;
        let offset = [r.f32()?, r.f32()?];
        let mut corners = Vec::with_capacity((width * height) as usize);
        for _ in 0..width * height {
            let ground_height = r.u16()? as i16;
            let w = r.u16()?;
            let flags_tex = r.u8()?;
            let variation = r.u8()?;
            let cliff = r.u8()?;
            corners.push(Corner {
                ground_height,
                water_level: w & 0x3FFF,
                map_edge: w & 0x4000 != 0,
                ramp: flags_tex & 0x10 != 0,
                blight: flags_tex & 0x20 != 0,
                water: flags_tex & 0x40 != 0,
                boundary: flags_tex & 0x80 != 0,
                ground_texture: flags_tex & 0x0F,
                ground_variation: variation & 0x1F,
                cliff_variation: variation >> 5,
                cliff_texture: cliff >> 4,
                layer_height: cliff & 0x0F,
            });
        }
        Ok(Terrain {
            version,
            tileset,
            custom_tilesets,
            ground_tilesets,
            cliff_tilesets,
            width,
            height,
            offset,
            corners,
        })
    }

    pub fn corner(&self, x: u32, y: u32) -> &Corner {
        &self.corners[(y * self.width + x) as usize]
    }
}

#[derive(Clone, Debug, Default)]
pub struct PlayerInfo {
    pub id: u32,
    pub kind: u32,
    pub race: u32,
    pub fixed_start: bool,
    pub name: String,
    pub start: [f32; 2],
    pub ally_low: u32,
    pub ally_high: u32,
}

#[derive(Clone, Debug, Default)]
pub struct ForceInfo {
    pub flags: u32,
    pub players: u32,
    pub name: String,
}

#[derive(Clone, Debug, Default)]
pub struct MapInfo {
    pub version: u32,
    pub saves: u32,
    pub editor_version: u32,
    pub name: String,
    pub author: String,
    pub description: String,
    pub recommended_players: String,
    pub camera_bounds: [f32; 8],
    pub camera_complements: [u32; 4],
    pub playable_width: u32,
    pub playable_height: u32,
    pub flags: u32,
    pub tileset: u8,
    pub loading_screen: i32,
    pub loading_model: String,
    pub loading_text: String,
    pub loading_title: String,
    pub loading_subtitle: String,
    pub game_data_set: u32,
    pub prologue_screen: i32,
    pub prologue_path: String,
    pub prologue_text: String,
    pub prologue_title: String,
    pub prologue_subtitle: String,
    pub fog_style: u32,
    pub fog_start: f32,
    pub fog_end: f32,
    pub fog_density: f32,
    pub fog_color: [u8; 4],
    pub weather: FourCC,
    pub sound_environment: String,
    pub light_environment: u8,
    pub water_color: [u8; 4],
    pub players: Vec<PlayerInfo>,
    pub forces: Vec<ForceInfo>,
}

impl MapInfo {
    pub fn parse(data: &[u8]) -> Result<MapInfo, Error> {
        let mut r = Reader::new(data);
        let mut m = MapInfo { version: r.u32()?, ..Default::default() };
        let tft = m.version >= 25;
        m.saves = r.u32()?;
        m.editor_version = r.u32()?;
        m.name = r.cstr()?;
        m.author = r.cstr()?;
        m.description = r.cstr()?;
        m.recommended_players = r.cstr()?;
        for v in &mut m.camera_bounds {
            *v = r.f32()?;
        }
        for v in &mut m.camera_complements {
            *v = r.u32()?;
        }
        m.playable_width = r.u32()?;
        m.playable_height = r.u32()?;
        m.flags = r.u32()?;
        m.tileset = r.u8()?;
        m.loading_screen = r.i32()?;
        if tft {
            m.loading_model = r.cstr()?;
        }
        m.loading_text = r.cstr()?;
        m.loading_title = r.cstr()?;
        m.loading_subtitle = r.cstr()?;
        if tft {
            m.game_data_set = r.u32()?;
            m.prologue_path = r.cstr()?;
        } else {
            m.prologue_screen = r.i32()?;
        }
        m.prologue_text = r.cstr()?;
        m.prologue_title = r.cstr()?;
        m.prologue_subtitle = r.cstr()?;
        if tft {
            m.fog_style = r.u32()?;
            m.fog_start = r.f32()?;
            m.fog_end = r.f32()?;
            m.fog_density = r.f32()?;
            m.fog_color = r.bytes(4)?.try_into().unwrap();
            m.weather = r.tag()?;
            m.sound_environment = r.cstr()?;
            m.light_environment = r.u8()?;
            m.water_color = r.bytes(4)?.try_into().unwrap();
        }
        let n = r.u32()?;
        for _ in 0..n {
            m.players.push(PlayerInfo {
                id: r.u32()?,
                kind: r.u32()?,
                race: r.u32()?,
                fixed_start: r.u32()? != 0,
                name: r.cstr()?,
                start: [r.f32()?, r.f32()?],
                ally_low: r.u32()?,
                ally_high: r.u32()?,
            });
        }
        let n = r.u32()?;
        for _ in 0..n {
            m.forces.push(ForceInfo { flags: r.u32()?, players: r.u32()?, name: r.cstr()? });
        }
        Ok(m)
    }
}

#[derive(Clone, Debug, Default)]
pub struct DroppedItemSet {
    pub items: Vec<(FourCC, u32)>,
}

fn item_sets(r: &mut Reader) -> Result<Vec<DroppedItemSet>, Error> {
    let n = r.u32()?;
    let mut sets = Vec::new();
    for _ in 0..n {
        let m = r.u32()?;
        let mut items = Vec::new();
        for _ in 0..m {
            items.push((r.tag()?, r.u32()?));
        }
        sets.push(DroppedItemSet { items });
    }
    Ok(sets)
}

#[derive(Clone, Debug, Default)]
pub struct PlacedDoodad {
    pub id: FourCC,
    pub variation: u32,
    pub position: [f32; 3],
    pub angle: f32,
    pub scale: [f32; 3],
    pub flags: u8,
    pub life: u8,
    pub item_table: i32,
    pub item_sets: Vec<DroppedItemSet>,
    pub editor_id: u32,
}

#[derive(Clone, Debug, Default)]
pub struct SpecialDoodad {
    pub id: FourCC,
    pub z: u32,
    pub x: u32,
    pub y: u32,
}

#[derive(Clone, Debug, Default)]
pub struct Doodads {
    pub version: u32,
    pub subversion: u32,
    pub doodads: Vec<PlacedDoodad>,
    pub special: Vec<SpecialDoodad>,
}

impl Doodads {
    pub fn parse(data: &[u8]) -> Result<Doodads, Error> {
        let mut r = Reader::new(data);
        if &r.tag()? != b"W3do" {
            return Err(Error::Format("not a doodad file"));
        }
        let version = r.u32()?;
        let subversion = r.u32()?;
        let tft = version >= 8;
        let n = r.u32()?;
        let mut doodads = Vec::with_capacity(n as usize);
        for _ in 0..n {
            let mut d = PlacedDoodad {
                id: r.tag()?,
                variation: r.u32()?,
                position: r.vec3()?,
                angle: r.f32()?,
                scale: r.vec3()?,
                flags: r.u8()?,
                life: r.u8()?,
                ..Default::default()
            };
            if tft {
                d.item_table = r.i32()?;
                d.item_sets = item_sets(&mut r)?;
            }
            d.editor_id = r.u32()?;
            doodads.push(d);
        }
        let mut special = Vec::new();
        if r.remaining() >= 8 {
            let _v = r.u32()?;
            let n = r.u32()?;
            for _ in 0..n {
                special.push(SpecialDoodad { id: r.tag()?, z: r.u32()?, x: r.u32()?, y: r.u32()? });
            }
        }
        Ok(Doodads { version, subversion, doodads, special })
    }
}

#[derive(Clone, Debug, Default)]
pub struct RandomUnit {
    /// 0: random from level, 1: random from map table, 2: random from custom table.
    pub kind: u32,
    pub level_or_group: i32,
    pub class_or_position: i32,
    pub custom: Vec<(FourCC, u32)>,
}

#[derive(Clone, Debug, Default)]
pub struct PlacedUnit {
    pub id: FourCC,
    pub variation: u32,
    pub position: [f32; 3],
    pub angle: f32,
    pub scale: [f32; 3],
    pub flags: u8,
    pub player: u32,
    pub life: i32,
    pub mana: i32,
    pub item_table: i32,
    pub item_sets: Vec<DroppedItemSet>,
    pub gold: u32,
    pub target_acquisition: f32,
    pub hero_level: u32,
    pub hero_stats: [u32; 3],
    pub inventory: Vec<(u32, FourCC)>,
    pub abilities: Vec<(FourCC, bool, u32)>,
    pub random: Option<RandomUnit>,
    pub color: i32,
    pub waygate: i32,
    pub creation_number: u32,
}

impl PlacedUnit {
    /// Parses `war3mapUnits.doo`.  Only the World Editor reads this file; the game creates
    /// pre-placed units by running the map script's `CreateAllUnits`.
    pub fn parse_all(data: &[u8]) -> Result<Vec<PlacedUnit>, Error> {
        let mut r = Reader::new(data);
        if &r.tag()? != b"W3do" {
            return Err(Error::Format("not a units file"));
        }
        let version = r.u32()?;
        let _sub = r.u32()?;
        let tft = version >= 8;
        let n = r.u32()?;
        let mut out = Vec::with_capacity(n as usize);
        for _ in 0..n {
            let mut u = PlacedUnit {
                id: r.tag()?,
                variation: r.u32()?,
                position: r.vec3()?,
                angle: r.f32()?,
                scale: r.vec3()?,
                flags: r.u8()?,
                player: r.u32()?,
                ..Default::default()
            };
            r.skip(2)?;
            u.life = r.i32()?;
            u.mana = r.i32()?;
            u.item_table = r.i32()?;
            if tft {
                u.item_sets = item_sets(&mut r)?;
            }
            u.gold = r.u32()?;
            u.target_acquisition = r.f32()?;
            u.hero_level = r.u32()?;
            if tft {
                u.hero_stats = [r.u32()?, r.u32()?, r.u32()?];
            }
            let n = r.u32()?;
            for _ in 0..n {
                u.inventory.push((r.u32()?, r.tag()?));
            }
            let n = r.u32()?;
            for _ in 0..n {
                u.abilities.push((r.tag()?, r.u32()? != 0, r.u32()?));
            }
            let kind = r.u32()?;
            u.random = match kind {
                0 => {
                    let v = r.u32()?;
                    Some(RandomUnit {
                        kind,
                        level_or_group: (v & 0x00FF_FFFF) as i32,
                        class_or_position: (v >> 24) as i32,
                        custom: vec![],
                    })
                }
                1 => Some(RandomUnit {
                    kind,
                    level_or_group: r.i32()?,
                    class_or_position: r.i32()?,
                    custom: vec![],
                }),
                2 => {
                    let n = r.u32()?;
                    let mut custom = Vec::new();
                    for _ in 0..n {
                        custom.push((r.tag()?, r.u32()?));
                    }
                    Some(RandomUnit { kind, level_or_group: 0, class_or_position: 0, custom })
                }
                _ => None,
            };
            u.color = r.i32()?;
            u.waygate = r.i32()?;
            u.creation_number = r.u32()?;
            out.push(u);
        }
        Ok(out)
    }
}

/// `war3map.wts`: `STRING n { ... }` blocks referenced as `TRIGSTR_n`.
pub fn parse_wts(text: &str) -> HashMap<u32, String> {
    let mut out = HashMap::new();
    let mut lines = text.lines();
    while let Some(line) = lines.next() {
        let line = line.trim_start_matches('\u{feff}').trim();
        let Some(n) = line.strip_prefix("STRING ") else { continue };
        let Ok(id) = n.trim().parse::<u32>() else { continue };
        let mut body = Vec::new();
        let mut started = false;
        for l in lines.by_ref() {
            if !started {
                if l.trim() == "{" {
                    started = true;
                }
                continue;
            }
            if l.trim_end() == "}" {
                break;
            }
            body.push(l.trim_end_matches('\r'));
        }
        out.insert(id, body.join("\n"));
    }
    out
}
