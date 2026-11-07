//! Saved games (.w3z), as the original writes them (`FUN_6f1e8630`): the recorded-game
//! container (`container`), its record a data store of little-endian bytes, words,
//! dwords, and NUL-terminated strings (`FUN_6f06b170`, `FUN_6f06b220`, `FUN_6f06b3a0`,
//! `FUN_6f06bc20`).  Read so far: the header and the object manager's layout (its
//! tables, classes, clocks, and data sections, and each object's records, `first` and
//! `second`), and the game's own state after it (`SaveGame::state`), all but its
//! script data, kept whole.

use crate::{Error, Reader};
use crate::codes;

/// A saved game's record.
#[derive(Clone, Debug, Default)]
pub struct SaveGame {
    /// The map's path, from the game's folder.
    pub map: String,
    /// The network game's name (empty for a game played alone).
    pub game_name: String,
    /// The network game's description, as its GAMEINFO carries it (`FUN_6f968cf0`): its
    /// name, password, and coded description, and two words.
    pub info_name: String,
    pub info_password: String,
    pub info_stat: Vec<u8>,
    pub info_words: [u32; 2],
    /// The slot table as W3GS carries it (its slots, seed, layout, and players).
    pub slots: Vec<u8>,
    /// A word the game's objects sum (`FUN_6f324930`).
    pub objects_word: u32,
    /// The map's checksums: its xoro and SHA-1 over the game's scripts.
    pub map_xoro: u32,
    pub map_sha1: [u8; 20],
    /// The local player's slot index (-1 for none) and player number.
    pub local_slot: i32,
    pub local_player: u32,
    /// The local player's name.
    pub player_name: String,
    /// Two preferences (`FUN_6f0425d0`: 6 and 0x38).
    pub preferences: [u32; 2],
    /// The game time in milliseconds and the word after it (`FUN_6f32fa00`: the net
    /// game's +0x1c70 and +0x1c74).
    pub game_ms: u32,
    pub game_word: u32,
    /// The engine's object manager (`FUN_6f04dd30`), when the record has 1.27b's layout
    /// (1.22's differs, and is kept whole in `rest`, from the game time on).
    pub objects: Option<ObjectManager>,
    /// The rest of the record, the game's own state (read by `state`).
    pub rest: Vec<u8>,
}

/// The engine's object manager as saved (`FUN_6f04dd30`, its tables at DAT_6fd6860c).
#[derive(Clone, Debug, Default)]
pub struct ObjectManager {
    /// Its first sections (`FUN_6f15b3e0`), each its size first: a header
    /// (`FUN_6f15c9b0`), the two tag tables (`FUN_6f15c810`), and their free lists
    /// (`FUN_6f15c710`).
    pub header: Vec<u8>,
    pub tables: [Vec<Tagged>; 2],
    pub free: [Vec<u8>; 2],
    /// The classes registered (code, parent's code, and tags), the agents' types.
    pub classes: Vec<[u32; 4]>,
    /// The main and agent clocks: time, a word, the wrap (300), and two words.
    pub clocks: [(f32, u32, f32, u32, u32); 2],
    pub block: Vec<u8>,
    pub word: u32,
    /// The objects' data (`FUN_6f15b4f0`), each section its size first: regions
    /// (`FUN_6f15c750`), the pathing (`FUN_6f167500`), and each table's objects twice
    /// (`FUN_6f15c8b0`, `FUN_6f15c930`).
    pub data: [Vec<u8>; 6],
    pub words: [u32; 2],
    /// The map's bounds and a fifth value.
    pub bounds: [f32; 5],
}

/// A saved object: its entry in its table and its two records, each its size first and
/// "espi" after (`FUN_6f15c8b0`, the object's vf 0x24; `FUN_6f15c930`, its vf 0x2c), in
/// the table's order.
#[derive(Clone, Debug, Default)]
pub struct SavedObject {
    pub entry: Tagged,
    pub first: Vec<u8>,
    pub second: Vec<u8>,
}

impl ObjectManager {
    /// Each table's objects with their records.
    pub fn objects(&self) -> Result<[Vec<SavedObject>; 2], Error> {
        let records = |b: &[u8]| -> Result<Vec<Vec<u8>>, Error> {
            let mut r = Reader::new(b);
            let mut out = Vec::new();
            while r.remaining() > 0 {
                let n = r.u32()? as usize;
                out.push(r.bytes(n)?.to_vec());
                if r.bytes(4)? != b"espi" {
                    return Err(Error::Format("an object's record not ended by espi"));
                }
            }
            Ok(out)
        };
        let mut out: [Vec<SavedObject>; 2] = Default::default();
        for (k, o) in out.iter_mut().enumerate() {
            let (first, second) = (records(&self.data[2 + k])?, records(&self.data[4 + k])?);
            if first.len() != self.tables[k].len() || second.len() != self.tables[k].len() {
                return Err(Error::Format("records not one per object"));
            }
            *o = self.tables[k].iter().zip(first).zip(second).map(|((&entry, first), second)| SavedObject { entry, first, second }).collect();
        }
        Ok(out)
    }
}

/// The objects' first data section (`FUN_6f15c750`): the engine's pathing maps, each
/// referred to by its tags first, then the two proximity maps' cells and the four
/// grids' (`FUN_6f14d620`, `FUN_6f163120`), then two more references.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PathingMaps {
    pub refs: [Ref; 6],
    pub proximity: [ProximityMap; 2],
    pub grids: [Grid; 4],
    pub tail: [Ref; 2],
}

/// A map's frame (`FUN_6f14d590`, `FUN_6f163090`): its size in cells, its bounds as
/// reals and as words, and its cell's size and that's inverse (in the pathing map's
/// cells: 8 and 0.125 for a map of a quarter of a tile).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MapFrame {
    pub width: u32,
    pub height: u32,
    pub bounds: [f32; 4],
    pub cells: [u32; 4],
    pub scale: [f32; 2],
}

/// A proximity map (`FUN_6f14d620`): its frame, three words, and each cell with flags
/// or objects (`FUN_6f14d4c0`), in rows: its column, row, flags (the cell's top byte;
/// in the pathing map its pathing), and its objects.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ProximityMap {
    pub frame: MapFrame,
    pub words: [u32; 3],
    pub cells: Vec<(u32, u32, u32, Vec<Ref>)>,
}

/// A grid (`FUN_6f163120`): its frame and each cell's word, in rows.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Grid {
    pub frame: MapFrame,
    pub cells: Vec<u16>,
}

impl MapFrame {
    fn read(r: &mut Reader) -> Result<MapFrame, Error> {
        Ok(MapFrame { width: r.u32()?, height: r.u32()?, bounds: reals(r)?, cells: words(r)?, scale: reals(r)? })
    }
}

impl PathingMaps {
    pub fn parse(b: &[u8]) -> Result<PathingMaps, Error> {
        let mut r = Reader::new(b);
        let mut m = PathingMaps::default();
        for x in &mut m.refs {
            *x = Ref::read(&mut r)?;
        }
        for p in &mut m.proximity {
            p.frame = MapFrame::read(&mut r)?;
            p.words = words(&mut r)?;
            let n = r.u32()?;
            for _ in 0..n {
                let (x, y, flags, k) = (r.u32()?, r.u32()?, r.u32()?, r.u32()?);
                let refs = (0..k).map(|_| Ref::read(&mut r)).collect::<Result<_, _>>()?;
                p.cells.push((x, y, flags, refs));
            }
        }
        for g in &mut m.grids {
            g.frame = MapFrame::read(&mut r)?;
            g.cells = (0..g.frame.width * g.frame.height).map(|_| r.u16()).collect::<Result<_, _>>()?;
        }
        for x in &mut m.tail {
            *x = Ref::read(&mut r)?;
        }
        if r.remaining() > 0 {
            return Err(Error::Format("the pathing maps not read to their end"));
        }
        Ok(m)
    }
}

/// A tagged object's entry: its family's code and its kind's (as the engine holds
/// them: "+agl" and "+ply" for a player's agent, "^pma" and "^pmr" for a pathing
/// region), and its tags.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Tagged {
    pub family: u32,
    pub kind: u32,
    pub tag: u32,
    pub birth: u32,
}

impl ObjectManager {
    fn parse(r: &mut Reader) -> Result<ObjectManager, Error> {
        let mut m = ObjectManager::default();
        let section = |r: &mut Reader| -> Result<Vec<u8>, Error> {
            let n = r.u32()? as usize;
            Ok(r.bytes(n)?.to_vec())
        };
        m.header = section(r)?;
        for t in &mut m.tables {
            let b = section(r)?;
            let mut s = Reader::new(&b);
            let n = s.u32()?;
            for _ in 0..n {
                t.push(Tagged { family: s.u32()?, kind: s.u32()?, tag: s.u32()?, birth: s.u32()? });
            }
        }
        m.free = [section(r)?, section(r)?];
        let n = r.u16()?;
        for _ in 0..n {
            m.classes.push([r.u32()?, r.u32()?, r.u32()?, r.u32()?]);
        }
        for c in &mut m.clocks {
            *c = (r.f32()?, r.u32()?, r.f32()?, r.u32()?, r.u32()?);
        }
        let n = r.u32()? as usize;
        m.block = r.bytes(n)?.to_vec();
        m.word = r.u32()?;
        for d in &mut m.data {
            *d = section(r)?;
        }
        m.words = [r.u32()?, r.u32()?];
        for b in &mut m.bounds {
            *b = r.f32()?;
        }
        Ok(m)
    }
}

impl SaveGame {
    pub fn parse(record: &[u8]) -> Result<SaveGame, Error> {
        let mut r = Reader::new(record);
        let mut s = SaveGame { map: r.cstr()?, game_name: r.cstr()?, ..Default::default() };
        s.info_name = r.cstr()?;
        s.info_password = r.cstr()?;
        let rest = &r.data[r.pos..];
        let n = rest.iter().position(|&c| c == 0).ok_or(Error::Eof)?;
        s.info_stat = r.bytes(n)?.to_vec();
        r.u8()?;
        s.info_words = [r.u32()?, r.u32()?];
        let n = r.u16()? as usize;
        s.slots = r.bytes(n)?.to_vec();
        s.objects_word = r.u32()?;
        s.map_xoro = r.u32()?;
        s.map_sha1 = r.bytes(20)?.try_into().unwrap();
        s.local_slot = r.i32()?;
        s.local_player = r.u32()?;
        s.player_name = r.cstr()?;
        s.preferences = [r.u32()?, r.u32()?];
        // 1.27b's layout from here (1.22's differs: kept whole).
        let mut t = r.clone();
        if let (Ok(ms), Ok(word), Ok(objects)) = (t.u32(), t.u32(), ObjectManager::parse(&mut t)) {
            (s.game_ms, s.game_word, s.objects) = (ms, word, Some(objects));
            r = t;
        }
        s.rest = r.data[r.pos..].to_vec();
        Ok(s)
    }

    /// Reads a .w3z file: its container, then its record.
    pub fn read(file: &[u8]) -> Result<SaveGame, Error> {
        let (_, record) = crate::container::unpack(file)?;
        SaveGame::parse(&record)
    }
}

/// The game's own state after the object manager (`FUN_6f1e8630`), as far as read:
/// each part's fields by name, and the bytes after.
#[derive(Clone, Debug, Default)]
pub struct GameState {
    pub parts: Vec<(&'static str, Vec<Field>)>,
    pub rest: Vec<u8>,
}

/// The game's own state's parts in order, as `fields` layouts ("fog" read by `fog`).
const STATE: &[(&str, &str)] = &[
    // CGameWar3's (its vf 0x10, `FUN_6f1e8460`): two words, a reference, three counted
    // lists of a name and a word (`FUN_6f1d6c60`), and two shorts.
    ("game", "ww r [zw] [zw] [zw] ss"),
    // Its CMapSetupWar3's (vf 4, `FUN_6f2465b0`): two strings, a word, a byte, a
    // string, five words, and six bytes.
    ("map setup", "zz w b z wwwww bbbbbb"),
    // Its CFogOfWarMap's (vf 8, `FUN_6f250d00`).
    ("fog", ""),
    // Then its own: a counted list (`FUN_6f1dbec0`: two reals, a counted list of two
    // words, five words), three words, a counted list of references, a word, a counted
    // list of counted lists of references (`FUN_6f1d6ec0`), a word, a reference
    // (`FUN_6f366820`), and five words.
    ("game 2", "[ff [ww] wwwww] www [r] w [[r]] w r wwwww"),
    // The game UI's (`FUN_6f377330`, its vf 0xe4: nothing in the saves read), then a
    // counted list of strings (`FUN_6f1d6f90`), a reference, and a counted list of
    // words.
    ("game 3", "[z] r [w]"),
    // The terrain's (`FUN_6f78c900`): a word (1); a word (0) and a counted list of
    // bytes (`FUN_6f757c80`); a word (2), a counted list of two words, a string, and a
    // word, and a counted list of bytes (`FUN_6f778f90`); a word (1), two words, and a
    // counted list of three words, four reals, a word, three reals, nine words, and two
    // counted lists of words (`FUN_6f7576a0`); a word (0) and a counted list of two
    // words, four reals, and a word (`FUN_6f7642d0`).
    ("terrain", "w w [b] w [ww z w] [b] ww w [www ffff w fff w ww wwww www [w] [w]] w [ww ffff w]"),
    // `FUN_6f90d350`'s, its size first.
    ("script data", "n"),
    // The hashtables' (CGameHashTableManager, `FUN_6f246430`): a counted list of a key
    // and its counted list of a key and five counted lists (`FUN_6f246d90`: integers,
    // reals, booleans, strings, and handles, each by its key), then a word.
    ("hashtables", "[w [w [ww] [wf] [ww] [wz] [www]]] w"),
    // `FUN_6f9ca1e0`'s: a counted list of references (`FUN_6f9c1560`) and a word.
    ("references", "[r] w"),
    // The sounds' (`FUN_6f359b50`): a counted list of reals (eight), a string, a word,
    // and a counted list of a name and its counted list of a reference, four reals,
    // three words, and three reals.
    ("sounds", "[f] z w [z [r ff ff ww w fff]]"),
    // The game UI's: a byte, a counted list of references (`FUN_6f376ce0`), two words;
    // a word and with it two words, two strings, and four reals (`FUN_6f3cb6b0`); a
    // word, three reals, a word, and a string (`FUN_6f389ec0`); and a string
    // (`FUN_6f3cb860`).
    ("ui", "b [r] ww ?(ww zz ffff) w fff w z z"),
];

/// The fog of war's map (`FUN_6f250d00`): two bytes, four words (the second and third
/// its size), four maps of that many shorts, six reals, three shorts, and a word.
fn fog(r: &mut Reader) -> Result<Vec<Field>, Error> {
    let mut out = fields(r, "bb wwww")?;
    let n = match (&out[3], &out[4]) {
        (Field::Word(a), Field::Word(b)) => *a as usize * *b as usize,
        _ => unreachable!(),
    };
    for _ in 0..4 {
        out.push(Field::List((0..n).map(|_| Ok(vec![Field::Short(r.u16()?)])).collect::<Result<_, Error>>()?));
    }
    out.extend(fields(r, "ffffff sss w")?);
    Ok(out)
}

impl SaveGame {
    /// The game's own state, as far as read.
    pub fn state(&self) -> Result<GameState, Error> {
        let mut r = Reader::new(&self.rest);
        let mut g = GameState::default();
        for &(name, layout) in STATE {
            let f = if name == "fog" { fog(&mut r)? } else { fields(&mut r, layout)? };
            g.parts.push((name, f));
        }
        g.rest = r.data[r.pos..].to_vec();
        Ok(g)
    }
}

/// A reference to a tagged object as saved (`FUN_6f1c5630`): its two tags, or -1 and
/// -1 for none.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Ref(pub u32, pub u32);

impl Ref {
    fn read(r: &mut Reader) -> Result<Ref, Error> {
        Ok(Ref(r.u32()?, r.u32()?))
    }
}

/// A reference with a word (`FUN_6f15df60`: 1 when it is the owner's own link).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Link(pub Ref, pub u32);

impl Link {
    fn read(r: &mut Reader) -> Result<Link, Error> {
        Ok(Link(Ref::read(r)?, r.u32()?))
    }
}

/// An optional block (`FUN_6f15de50`): present, then two reals and three words.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Timing(pub Option<(f32, f32, u32, u32, u32)>);

impl Timing {
    fn read(r: &mut Reader) -> Result<Timing, Error> {
        Ok(Timing(match r.u32()? {
            0 => None,
            _ => Some((r.f32()?, r.f32()?, r.u32()?, r.u32()?, r.u32()?)),
        }))
    }
}

/// Every engine object's first fields (`FUN_6f15dec0`): a real, two words, two pairs
/// of links with two references (`FUN_6f15df20`), and two optional blocks.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ObjectBase {
    pub real: f32,
    pub words: [u32; 2],
    pub links: [(Link, Link, Ref, Ref); 2],
    pub timings: [Timing; 2],
}

impl ObjectBase {
    fn read(r: &mut Reader) -> Result<ObjectBase, Error> {
        let mut b = ObjectBase { real: r.f32()?, words: [r.u32()?, r.u32()?], ..Default::default() };
        for l in &mut b.links {
            *l = (Link::read(r)?, Link::read(r)?, Ref::read(r)?, Ref::read(r)?);
        }
        for t in &mut b.timings {
            *t = Timing::read(r)?;
        }
        Ok(b)
    }
}

/// A property's first fields (`FUN_6f160ff0`): the object's, two links, a real and a
/// word.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PropertyBase {
    pub object: ObjectBase,
    pub links: [Link; 2],
    pub real: f32,
    pub word: u32,
}

impl PropertyBase {
    fn read(r: &mut Reader) -> Result<PropertyBase, Error> {
        Ok(PropertyBase { object: ObjectBase::read(r)?, links: [Link::read(r)?, Link::read(r)?], real: r.f32()?, word: r.u32()? })
    }
}

/// An engine object's first record, as far as its class is read.
#[derive(Clone, Debug, PartialEq)]
pub enum First {
    /// A pathing map region (`^pmr`, CPmRegion's `FUN_6f14d430`): two references, a
    /// word, four words (its bounds), and three words.
    Region { refs: [Ref; 2], word: u32, bounds: [u32; 4], words: [u32; 3] },
    /// A flag property (`` `flg ``, CPoFlag's `FUN_6f160ca0`): its value.
    Flag { base: PropertyBase, value: u32 },
    /// A real property (`` `rel ``, CPoReal's `FUN_6f161710`): four reals.
    Real { base: PropertyBase, values: [f32; 4] },
    /// A position property (`` `pos ``, CPoPos's `FUN_6f15f180`): seven reals and two
    /// references.
    Pos { base: PropertyBase, values: [f32; 7], refs: [Ref; 2] },
    /// An agent's wrapper (CAgentBaseAbs's `FUN_6f146480`): the object's fields, its
    /// agent, a counted list of references, and four links.
    Agent { object: ObjectBase, agent: Ref, refs: Vec<Ref>, links: [Link; 4] },
    /// A real's threshold listener (`` `rtl ``, CPoReThresholdLis's `FUN_6f1616e0`): a real
    /// and a word.
    Threshold { object: ObjectBase, real: f32, word: u32 },
    /// A flag listener (`` `fcl ``, CPoFgLis's `FUN_6f160c30`): a word.
    FlagListener { object: ObjectBase, word: u32 },
    /// A position interference listener (`` `pil ``, CPoPoInterfLis's `FUN_6f15f100`): a
    /// real, a word, and a counted array (`FUN_6f15e9b0`).
    Interference { object: ObjectBase, real: f32, word: u32, array: Array<[u32; 3]> },
    /// A real's continuous modifier (`` `rcm ``, CPoReContinuousMod's `FUN_6f161660`): two
    /// reals.
    Continuous { object: ObjectBase, values: [f32; 2] },
    /// A clustered position (`` `pcu ``, CPoPosCl's `FUN_6f16f890`): the position's
    /// record, three words, eight reals, three words, two flags, a reference, and with
    /// the second flag its separation's record (CPoSeparate's `FUN_6f16f9d0`, the
    /// object at 0xac: two reals and a word; read, the hero map's moving dragons).
    Cluster { base: PropertyBase, values: [f32; 7], refs: [Ref; 2], words: [u32; 3], reals: [f32; 8], more: [u32; 3], flags: [u32; 2], last: Ref, separate: Option<(f32, f32, u32)> },
    /// A long-range path (`l^pa`, CLrPath's `FUN_6f167780`): six reals, six words, a
    /// real, a word, two waypoint lists each with two words and a short, and four
    /// references.
    Path { reals: [f32; 6], words: [u32; 6], real: f32, word: u32, ways: [(Array<[f32; 2]>, u32, u32, u16); 2], refs: [Ref; 4] },
    /// The long-range pathing accelerator (`l^ac`, `FUN_6f162fd0`): four references, a
    /// counted array, and a word.
    Accelerator { refs: [Ref; 4], array: Array<[u32; 3]>, word: u32 },
    /// The long-range pathing system (`l^sy`, `FUN_6f148210`): a reference and a short.
    System { owner: Ref, short: u16 },
    /// An agent class's definition (`^adf`, CRlAgentDef's `FUN_6f146500`): the object's
    /// fields and a counted array of eleven words each (`FUN_6f145840`).
    AgentDef { object: ObjectBase, array: Array<[u32; 11]> },
    /// A proximity cluster (`^clu`, CPrCluster's `FUN_6f16ace0`): a counted array
    /// (`FUN_6f1690d0`: three words, six reals, a word each), then three words, four
    /// reals, five words, two reals, a word, and three references.
    Cluster2 { array: Array<([u32; 3], [f32; 6], u32)>, words: [u32; 3], reals: [f32; 4], more: [u32; 5], last_reals: [f32; 2], word: u32, refs: [Ref; 3] },
    /// A cluster group (`^clp`, CPrClusterGroup's `FUN_6f16aec0`): a word and three reals.
    ClusterGroup { word: u32, reals: [f32; 3] },
    /// A missile's position (`` `pbh ``, CPoPosBh's `FUN_6f172550`): the position's record,
    /// nine reals, a counted list of references, and an optional block.
    ProjectilePos { base: PropertyBase, values: [f32; 7], refs: [Ref; 2], reals: [f32; 9], list: Vec<Ref>, timing: Timing },
    /// A missile's flight (`>prj`, CBhPoProjectile's `FUN_6f173160`): a behavior's five
    /// words and two optional blocks (`FUN_6f171c00`), eleven reals, four words, and two
    /// reals.
    Projectile { words: [u32; 5], timings: [Timing; 2], reals: [f32; 11], more: [u32; 4], last: [f32; 2] },
    /// An object whose first record is empty (`^ama`, CAcceleratorMap; `^pma`,
    /// CProximityMap).
    Empty,
    /// Not yet read.
    Raw(Vec<u8>),
}

/// A counted array as the engine saves its arrays (`FUN_6f15e9b0`, `FUN_6f165610`):
/// four words, the count, then each entry.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Array<T> {
    pub words: [u32; 4],
    pub entries: Vec<T>,
}

impl<T> Array<T> {
    fn read(r: &mut Reader, entry: impl Fn(&mut Reader) -> Result<T, Error>) -> Result<Array<T>, Error> {
        let words = [r.u32()?, r.u32()?, r.u32()?, r.u32()?];
        let n = r.u32()?;
        let entries = (0..n).map(|_| entry(r)).collect::<Result<_, _>>()?;
        Ok(Array { words, entries })
    }
}

fn words<const N: usize>(r: &mut Reader) -> Result<[u32; N], Error> {
    let mut w = [0; N];
    for v in &mut w {
        *v = r.u32()?;
    }
    Ok(w)
}

fn reals<const N: usize>(r: &mut Reader) -> Result<[f32; N], Error> {
    let mut w = [0.0; N];
    for v in &mut w {
        *v = r.f32()?;
    }
    Ok(w)
}

impl SavedObject {
    /// Its first record read by its kind.
    pub fn first(&self) -> Result<First, Error> {
        let mut r = Reader::new(&self.first);
        let (kind, family) = (&self.entry.kind.to_be_bytes(), &self.entry.family.to_be_bytes());
        let out = match (kind, family) {
            (codes::class::PATHING_REGION, _) => {
                let refs = [Ref::read(&mut r)?, Ref::read(&mut r)?];
                let word = r.u32()?;
                let bounds = [r.u32()?, r.u32()?, r.u32()?, r.u32()?];
                First::Region { refs, word, bounds, words: [r.u32()?, r.u32()?, r.u32()?] }
            }
            (codes::class::FLAG, _) => First::Flag { base: PropertyBase::read(&mut r)?, value: r.u32()? },
            (codes::class::REAL, _) => First::Real { base: PropertyBase::read(&mut r)?, values: [r.f32()?, r.f32()?, r.f32()?, r.f32()?] },
            (codes::class::POSITION, _) => {
                let base = PropertyBase::read(&mut r)?;
                let mut values = [0f32; 7];
                for v in &mut values {
                    *v = r.f32()?;
                }
                First::Pos { base, values, refs: [Ref::read(&mut r)?, Ref::read(&mut r)?] }
            }
            (_, codes::class::AGENT) => {
                let object = ObjectBase::read(&mut r)?;
                let agent = Ref::read(&mut r)?;
                let refs = (0..r.u32()?).map(|_| Ref::read(&mut r)).collect::<Result<_, _>>()?;
                First::Agent { object, agent, refs, links: [Link::read(&mut r)?, Link::read(&mut r)?, Link::read(&mut r)?, Link::read(&mut r)?] }
            }
            (codes::class::THRESHOLD_LISTENER, _) => First::Threshold { object: ObjectBase::read(&mut r)?, real: r.f32()?, word: r.u32()? },
            (codes::class::FLAG_LISTENER, _) => First::FlagListener { object: ObjectBase::read(&mut r)?, word: r.u32()? },
            (codes::class::IN_RANGE_LISTENER, _) => First::Interference { object: ObjectBase::read(&mut r)?, real: r.f32()?, word: r.u32()?, array: Array::read(&mut r, |r| words::<3>(r))? },
            (codes::class::CONTINUOUS_MODIFIER, _) => First::Continuous { object: ObjectBase::read(&mut r)?, values: reals(&mut r)? },
            (codes::class::UNIT_POSITION, _) => {
                let base = PropertyBase::read(&mut r)?;
                let (values, refs) = (reals(&mut r)?, [Ref::read(&mut r)?, Ref::read(&mut r)?]);
                let (words3, reals8, more) = (words(&mut r)?, reals(&mut r)?, words(&mut r)?);
                let (flags, last) = (words(&mut r)?, Ref::read(&mut r)?);
                let separate = if flags[1] != 0 { Some((r.f32()?, r.f32()?, r.u32()?)) } else { None };
                First::Cluster { base, values, refs, words: words3, reals: reals8, more, flags, last, separate }
            }
            (codes::class::PATH, _) => {
                let (reals6, words5, real, word) = (reals(&mut r)?, words(&mut r)?, r.f32()?, r.u32()?);
                let way = |r: &mut Reader| -> Result<_, Error> { Ok((Array::read(r, |r| reals::<2>(r))?, r.u32()?, r.u32()?, r.u16()?)) };
                let ways = [way(&mut r)?, way(&mut r)?];
                First::Path { reals: reals6, words: words5, real, word, ways, refs: [Ref::read(&mut r)?, Ref::read(&mut r)?, Ref::read(&mut r)?, Ref::read(&mut r)?] }
            }
            (codes::class::LONG_RANGE_ACCELERATOR, _) => First::Accelerator { refs: [Ref::read(&mut r)?, Ref::read(&mut r)?, Ref::read(&mut r)?, Ref::read(&mut r)?], array: Array::read(&mut r, |r| words::<3>(r))?, word: r.u32()? },
            (codes::class::LONG_RANGE_SYSTEM, _) => First::System { owner: Ref::read(&mut r)?, short: r.u16()? },
            (codes::class::AGENT_DEFINITION, _) => First::AgentDef { object: ObjectBase::read(&mut r)?, array: Array::read(&mut r, |r| words::<11>(r))? },
            (codes::class::CLUSTER, _) => {
                let array = Array::read(&mut r, |r| Ok((words::<3>(r)?, reals::<6>(r)?, r.u32()?)))?;
                let (words3, reals4, more, last_reals, word) = (words(&mut r)?, reals(&mut r)?, words(&mut r)?, reals(&mut r)?, r.u32()?);
                First::Cluster2 { array, words: words3, reals: reals4, more, last_reals, word, refs: [Ref::read(&mut r)?, Ref::read(&mut r)?, Ref::read(&mut r)?] }
            }
            (codes::class::CLUSTER_GROUP, _) => First::ClusterGroup { word: r.u32()?, reals: reals(&mut r)? },
            (codes::class::PROJECTILE_POSITION, _) => {
                let base = PropertyBase::read(&mut r)?;
                let (values, refs, reals9) = (reals(&mut r)?, [Ref::read(&mut r)?, Ref::read(&mut r)?], reals(&mut r)?);
                let list = (0..r.u32()?).map(|_| Ref::read(&mut r)).collect::<Result<_, _>>()?;
                First::ProjectilePos { base, values, refs, reals: reals9, list, timing: Timing::read(&mut r)? }
            }
            (codes::class::PROJECTILE, _) => {
                let (words5, timings) = (words(&mut r)?, [Timing::read(&mut r)?, Timing::read(&mut r)?]);
                First::Projectile { words: words5, timings, reals: reals(&mut r)?, more: words(&mut r)?, last: reals(&mut r)? }
            }
            (codes::class::ACCELERATOR_GRID | codes::class::PATHING_MAP, _) => First::Empty,
            _ => return Ok(First::Raw(self.first.clone())),
        };
        if r.remaining() != 0 {
            return Err(Error::Format("an object's first record longer than its class's fields"));
        }
        Ok(out)
    }

    /// Its part of the objects' sum (`FUN_6f324930`, the object's vf 0x3c): its tags and
    /// the fields its class adds, each as `sum_word` takes it.  Every field it adds is
    /// in its first record, so the sum is the save's own.
    pub fn sum(&self) -> Result<u32, Error> {
        let mut s = Sum(sum_word(self.entry.tag).wrapping_add(sum_word(self.entry.birth)));
        match self.first()? {
            // CPmRegion `FUN_6f14cc20`: its bounds and its word (+0x1c to +0x28, +0x34).
            First::Region { word, bounds, .. } => {
                s.words(&bounds).word(word);
            }
            // CPoFlag `FUN_6f152720`, CPoReal `FUN_6f153310`, CPoPos `FUN_6f152a50`: a
            // property's (CRlProperty `FUN_6f153c90`: the relation's, its real and word)
            // and each value.
            First::Flag { base, value } => {
                s.property(&base).word(value);
            }
            First::Real { base, values } => {
                s.property(&base).reals(&values);
            }
            First::Pos { base, values, .. } => {
                s.property(&base).reals(&values);
            }
            // A unit's position (CPoPosCl `FUN_6f16f0a0`): the position's, then all its own
            // but its third word, its flags, and its reference, and its separation's three
            // (CPoSeparate `FUN_6f159ef0`).
            First::Cluster { base, values, words, reals, more, separate, .. } => {
                s.property(&base).reals(&values).words(&words[..2]).reals(&reals).words(&more);
                if let Some((a, b, w)) = separate {
                    s.reals(&[a, b]).word(w);
                }
            }
            // An agent's wrapper (CAgentBaseAbs), a class's definition (CRlAgentDef): the
            // relation's alone (CPrRelation `FUN_6f15d960`).
            First::Agent { object, .. } | First::AgentDef { object, .. } => {
                s.relation(&object);
            }
            // The listeners and the modifier (`FUN_6f153240`, `FUN_6f1524f0`, `FUN_6f152860`,
            // `FUN_6f153000`): the relation's and their first two fields (one, a flag's).
            First::Threshold { object, real, word } | First::Interference { object, real, word, .. } => {
                s.relation(&object).reals(&[real]).word(word);
            }
            First::FlagListener { object, word } => {
                s.relation(&object).word(word);
            }
            First::Continuous { object, values } => {
                s.relation(&object).reals(&values);
            }
            // A path (CLrPath `FUN_6f166510`): its six reals; each way's count, points, and
            // first word, and its short; its first three words, its own word, and its real.
            First::Path { reals, words, real, word, ways, .. } => {
                s.reals(&reals);
                for (a, w, _, short) in &ways {
                    s.word(a.entries.len() as u32);
                    for p in &a.entries {
                        s.reals(p);
                    }
                    s.word(*w).word(*short as u32);
                }
                s.words(&words[..3]).word(word).reals(&[real]);
            }
            // A walking cluster (CPrCluster `FUN_6f153480`): each entry's two words, two
            // reals, and last word, then its first two words, four reals, five words, two
            // reals, and last word.
            First::Cluster2 { array, words, reals, more, last_reals, word, .. } => {
                for (w, r, last) in &array.entries {
                    s.words(&w[..2]).reals(&r[..2]).word(*last);
                }
                s.words(&words[..2]).reals(&reals).words(&more).reals(&last_reals).word(word);
            }
            // A cluster group (CPrClusterGroup `FUN_6f153b20`): its word and reals.
            First::ClusterGroup { word, reals } => {
                s.word(word).reals(&reals);
            }
            // A missile's position (CPoPosBh `FUN_6f152cd0`): the position's and its nine
            // reals.
            First::ProjectilePos { base, values, reals, .. } => {
                s.property(&base).reals(&values).reals(&reals);
            }
            // A missile's flight (CBhPoProjectile `FUN_6f152230`, past its tags): its third,
            // fourth, and sixth to ninth reals, its eleventh, and its first word after them
            // (+0x40, +0x44, +0x50 to +0x5c, +0x60, +0x64).
            First::Projectile { reals, more, .. } => {
                s.reals(&reals[2..4]).reals(&reals[5..9]).reals(&reals[10..]).word(more[0]);
            }
            // The pathing maps, the accelerator grids, the long-range system and its
            // accelerator: their tags alone (CPresenceTagged `FUN_6f1c54e0`).
            First::Accelerator { .. } | First::System { .. } | First::Empty => {}
            First::Raw(_) => return Err(Error::Format("an object's first record not read, for the objects' sum")),
        }
        Ok(s.0)
    }
}

/// A word as the objects' sum takes it (`FUN_6f1c54e0` and the classes' vf 0x3c): each
/// of its four bytes `b` as `b * 0x7a2d ^ b`, added (so the order of the words does not
/// matter).
pub fn sum_word(v: u32) -> u32 {
    v.to_le_bytes().iter().fold(0u32, |s, &b| s.wrapping_add((b as u32).wrapping_mul(0x7a2d) ^ b as u32))
}

/// The objects' sum as it grows.
struct Sum(u32);

impl Sum {
    fn word(&mut self, v: u32) -> &mut Sum {
        self.0 = self.0.wrapping_add(sum_word(v));
        self
    }

    fn words(&mut self, v: &[u32]) -> &mut Sum {
        v.iter().for_each(|&v| self.0 = self.0.wrapping_add(sum_word(v)));
        self
    }

    fn reals(&mut self, v: &[f32]) -> &mut Sum {
        v.iter().for_each(|v| self.0 = self.0.wrapping_add(sum_word(v.to_bits())));
        self
    }

    /// A relation's (CPrRelation `FUN_6f15d960`): its real and its first word, the object
    /// its first pair's second reference names (that's tags, twice: read so), and its
    /// second word.
    fn relation(&mut self, o: &ObjectBase) -> &mut Sum {
        self.reals(&[o.real]).words(&o.words);
        let r = o.links[0].3;
        if r != Ref(u32::MAX, u32::MAX) {
            self.words(&[r.0, r.1, r.0, r.1]);
        }
        self
    }

    /// A property's (CRlProperty `FUN_6f153c90`): the relation's, its real and its word.
    fn property(&mut self, p: &PropertyBase) -> &mut Sum {
        self.relation(&p.object).reals(&[p.real]).word(p.word)
    }
}

impl SaveGame {
    /// The objects' sum (`FUN_6f324930`, the word after the slot table): each live object
    /// of the main table's (the second's are not summed).
    pub fn objects_sum(&self) -> Result<u32, Error> {
        let m = self.objects.as_ref().ok_or(Error::Format("no object manager read"))?;
        let [main, _] = m.objects()?;
        main.iter().try_fold(0u32, |s, o| Ok(s.wrapping_add(o.sum()?)))
    }
}

/// An agent's handler lists as its record ends (CAgent's `FUN_6f0562b0` and
/// `FUN_6f03eda0`): a counted table of keyed lists, the lists of events 0x40190064 to
/// 0x40190068, and of events 0xd0000 to 0xd03ff (`FUN_6f0564e0`).  A list
/// (`FUN_6f0563d0`): a count, then each handler's reference and its word (read: a
/// destructable's ref 0x281 with event 0x80267).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Handlers {
    pub keyed: Vec<(u32, Vec<(Ref, u32)>)>,
    pub fixed: [Vec<(Ref, u32)>; 5],
    pub events: Vec<Vec<(Ref, u32)>>,
}

impl Handlers {
    /// The handlers of a class whose record has no lists for events 0xd0000 to
    /// 0xd03ff (its vf 0x38 writes the agent base's `FUN_6f0562b0` alone).
    fn read_base(r: &mut Reader) -> Result<Handlers, Error> {
        let list = |r: &mut Reader| -> Result<Vec<(Ref, u32)>, Error> { (0..r.u32()?).map(|_| Ok((Ref::read(r)?, r.u32()?))).collect() };
        let n = r.u32()?;
        let keyed = (0..n).map(|_| Ok((r.u32()?, list(r)?))).collect::<Result<_, Error>>()?;
        Ok(Handlers { keyed, fixed: [list(r)?, list(r)?, list(r)?, list(r)?, list(r)?], events: Vec::new() })
    }

    fn read(r: &mut Reader) -> Result<Handlers, Error> {
        let list = |r: &mut Reader| -> Result<Vec<(Ref, u32)>, Error> { (0..r.u32()?).map(|_| Ok((Ref::read(r)?, r.u32()?))).collect() };
        let n = r.u32()?;
        let keyed = (0..n).map(|_| Ok((r.u32()?, list(r)?))).collect::<Result<_, Error>>()?;
        let fixed = [list(r)?, list(r)?, list(r)?, list(r)?, list(r)?];
        let events = (0..0x400).map(|_| list(r)).collect::<Result<_, _>>()?;
        Ok(Handlers { keyed, fixed, events })
    }
}

/// An agent's second record, as far as its class is read: its own fields (its vf
/// 0x48), then its handler lists.
#[derive(Clone, Debug, PartialEq)]
pub enum Second {
    /// A list of unit references (`uref`, CUnitRefList's `FUN_6f42f860`): four words.
    UnitRefs { words: [u32; 4], handlers: Handlers },
    /// A destructable (`+w3d`, CDestructable's `FUN_6f6c3940`): its own fields (a word and
    /// three reals, `FUN_6f6c39b0`), its handlers, the widget's state, and two bytes and
    /// three words.
    Destructable { own: (u32, [f32; 3]), handlers: Handlers, widget: Widget, bytes: [u8; 2], words: [u32; 3] },
    /// A unit (`+w3u`, CUnit's `FUN_6f696490`), not yet named field by field: its own
    /// word, its handlers, the widget's state, its event lists 0x80234 to 0x80258 and
    /// 0x80319 to 0x80329, then its fields in the order written (timers, references,
    /// words, and reals).
    Unit { own: u32, handlers: Handlers, widget: Widget, lists: Vec<Vec<(Ref, u32)>>, fields: Vec<Field> },
    /// An agent whose record is its own fields, its handlers, and its class's fields
    /// after them, read by layout (`AGENTS`), the fields not yet named.
    Agent { own: Vec<Field>, handlers: Handlers, tail: Vec<Field> },
    /// An agent whose class writes its whole record itself (`WHOLE`).
    Whole(Vec<Field>),
    /// No record (an engine object, or an agent without one).
    Empty,
    /// Not yet read.
    Raw(Vec<u8>),
}

/// A field of a record not yet named, as written: a word, a real, a byte, a short, a
/// reference, or a timer (CAgentTimer's `FUN_6f060ba0`: a reference, a word, and an
/// optional block).
#[derive(Clone, Debug, PartialEq)]
pub enum Field {
    Word(u32),
    Real(f32),
    Byte(u8),
    Short(u16),
    Ref(Ref),
    Timer(Ref, u32, Timing),
    /// A count, then that many groups.
    List(Vec<Vec<Field>>),
    /// A NUL-terminated string.
    Text(String),
    /// A widget's sprite (`FUN_6f1a4050`).
    Sprite(Option<Sprite>),
    /// A section kept whole: its size, then its bytes.
    Bytes(Vec<u8>),
}

/// A layout's fields at rest: read from zeros, each reference none and each timer
/// stopped (none running, no owner), the lists empty.
pub fn default_fields(layout: &str) -> Vec<Field> {
    fn fix(f: Field) -> Field {
        match f {
            Field::Ref(Ref(0, 0)) => Field::Ref(Ref(u32::MAX, u32::MAX)),
            Field::Timer(Ref(0, 0), k, t) => Field::Timer(Ref(u32::MAX, u32::MAX), k, t),
            Field::List(l) => Field::List(l.into_iter().map(|g| g.into_iter().map(fix).collect()).collect()),
            f => f,
        }
    }
    let zeros = vec![0u8; 1 << 16];
    let mut r = Reader::new(&zeros);
    fields(&mut r, layout).unwrap_or_default().into_iter().map(fix).collect()
}

/// The fields of a layout read from bytes, and how many bytes are left after them (to
/// fit a layout to records read raw).
pub fn fit_fields(b: &[u8], layout: &str) -> Result<(Vec<Field>, usize), Error> {
    let mut r = Reader::new(b);
    let f = fields(&mut r, layout)?;
    Ok((f, r.remaining()))
}

/// An agent's second record read as far as its own fields (by `own`'s layout) and its
/// handler lists go, and the bytes left after them (to find an unread class's layout).
pub fn probe_agent(b: &[u8], own: &str) -> Result<(Vec<Field>, Handlers, Vec<u8>), Error> {
    let mut r = Reader::new(b);
    let own = fields(&mut r, own)?;
    let handlers = Handlers::read(&mut r)?;
    Ok((own, handlers, r.data[r.pos..].to_vec()))
}

/// Reads the whole of `b` as fields by a layout (as `fields` does).
pub fn parse_fields(layout: &str, b: &[u8]) -> Result<Vec<Field>, Error> {
    let mut r = Reader::new(b);
    let out = fields(&mut r, layout)?;
    if r.remaining() != 0 {
        return Err(Error::Format("bytes left after the layout"));
    }
    Ok(out)
}

/// Reads fields by a layout: 'w' a word, 'f' a real, 'b' a byte, 's' a short, 'r' a
/// reference, 't' a timer, 'z' a string, 'p' a sprite, 'e' an event's handler list (as
/// "[rw]"), "[...]" a word count and that many groups, "{...}" a short count and that
/// many, "(...)" a group, "?(X)" a word and X when it is not 0, "?(X)(Y)" Y when it is,
/// "?b(X)" the same with a byte, 'n' a size and that many bytes kept whole,
/// 'm' a word kept as a mask and "&h(X)" X when the mask has the bits h (hex), "#(X)" X
/// as many times as the last word read, "~(X)" X when the last word read is not 0; a
/// number before a token repeats it.
fn fields(r: &mut Reader, layout: &str) -> Result<Vec<Field>, Error> {
    let chars: Vec<char> = layout.chars().filter(|c| !c.is_whitespace()).collect();
    // The index of the bracket closing the one at `k`.
    let close = |k: usize| {
        let (open, close) = match chars[k] {
            '[' => ('[', ']'),
            '{' => ('{', '}'),
            _ => ('(', ')'),
        };
        let (mut depth, mut j) = (0, k);
        loop {
            if chars[j] == open {
                depth += 1;
            } else if chars[j] == close {
                depth -= 1;
                if depth == 0 {
                    return j;
                }
            }
            j += 1;
        }
    };
    let inner = |a: usize, b: usize| -> String { chars[a + 1..b].iter().collect() };
    let mut out = Vec::new();
    let (mut mask, mut last) = (0u32, 0u32);
    let mut k = 0;
    while k < chars.len() {
        let mut times = 0usize;
        while chars[k].is_ascii_digit() {
            times = times * 10 + chars[k].to_digit(10).unwrap() as usize;
            k += 1;
        }
        match chars[k] {
            '?' => {
                let byte = chars[k + 1] == 'b';
                let open = if byte { k + 2 } else { k + 1 };
                let x = close(open);
                let y = (chars.get(x + 1) == Some(&'(')).then(|| close(x + 1));
                let flag = if byte {
                    let b = r.u8()?;
                    out.push(Field::Byte(b));
                    b as u32
                } else {
                    let w = r.u32()?;
                    out.push(Field::Word(w));
                    w
                };
                if flag != 0 {
                    out.push(Field::List(vec![fields(r, &inner(open, x))?]));
                } else if let Some(y) = y {
                    out.push(Field::List(vec![fields(r, &inner(x + 1, y))?]));
                }
                k = y.unwrap_or(x) + 1;
                continue;
            }
            '~' => {
                let end = close(k + 1);
                if last != 0 {
                    out.push(Field::List(vec![fields(r, &inner(k + 1, end))?]));
                }
                k = end + 1;
                continue;
            }
            '#' => {
                let end = close(k + 1);
                let group = inner(k + 1, end);
                out.push(Field::List((0..last).map(|_| fields(r, &group)).collect::<Result<_, _>>()?));
                k = end + 1;
                continue;
            }
            '&' => {
                let open = k + 1 + chars[k + 1..].iter().position(|&c| c == '(').expect("layout bits");
                let bits = u32::from_str_radix(&chars[k + 1..open].iter().collect::<String>(), 16).expect("layout bits");
                let end = close(open);
                if mask & bits != 0 {
                    out.push(Field::List(vec![fields(r, &inner(open, end))?]));
                }
                k = end + 1;
                continue;
            }
            _ => {}
        }
        let start = k;
        if matches!(chars[k], '[' | '{' | '(') {
            k = close(k);
        }
        let token: String = chars[start..=k].iter().collect();
        for _ in 0..times.max(1) {
            out.push(match token.chars().next().unwrap() {
                'w' => {
                    last = r.u32()?;
                    Field::Word(last)
                }
                'm' => {
                    mask = r.u32()?;
                    Field::Word(mask)
                }
                'f' => Field::Real(r.f32()?),
                'b' => Field::Byte(r.u8()?),
                's' => Field::Short(r.u16()?),
                'r' => Field::Ref(Ref::read(r)?),
                't' => Field::Timer(Ref::read(r)?, r.u32()?, Timing::read(r)?),
                'z' => Field::Text(r.cstr()?),
                'n' => {
                    let n = r.u32()? as usize;
                    Field::Bytes(r.bytes(n)?.to_vec())
                }
                'p' => Field::Sprite(Sprite::read(r)?),
                'e' => Field::List((0..r.u32()?).map(|_| fields(r, "rw")).collect::<Result<_, _>>()?),
                '[' => Field::List((0..r.u32()?).map(|_| fields(r, &token[1..token.len() - 1])).collect::<Result<_, _>>()?),
                '{' => Field::List((0..r.u16()?).map(|_| fields(r, &token[1..token.len() - 1])).collect::<Result<_, _>>()?),
                '(' => Field::List(vec![fields(r, &token[1..token.len() - 1])?]),
                c => unreachable!("layout {c}"),
            });
        }
        k += 1;
    }
    Ok(out)
}

/// An ability's own fields (CAbility's vf 0x48, `FUN_6f42e830`): four words, seven
/// words, its timer (+0x58), and a word.
const ABILITY_OWN: &str = "wwww wwwwwww t w";

/// Agents whose second record is their own fields (vf 0x48), their handlers, and their
/// class's fields after (vf 0x38 after `FUN_6f0562b0` and the 1024 event lists), by
/// kind: (own, tail) as `fields` layouts.
const AGENTS: &[(&[u8; 4], &str, &str)] = &[
    // CAbilityAlarm (`FUN_6f42d040`): a timer and a reference.
    (codes::ability::id::ALARM, ABILITY_OWN, "t r"),
    // CAbilityWander (`FUN_6f5ab430`): a timer.
    (codes::ability::id::WANDER, ABILITY_OWN, "t"),
    // `FUN_6f42dea0`: three words and a timer.
    (codes::ability::id::ON_FIRE_HUMAN, ABILITY_OWN, "www t"),
    (codes::ability::id::ON_FIRE_UNDEAD, ABILITY_OWN, "www t"),
    // Those whose class's fields are all their own (CAgent's vf 0x38, `FUN_6f03eda0`).
    // CAlliances: a word (`FUN_6f03edf0`).
    (codes::class::ALLIANCE, "w", ""),
    (codes::ability::id::RETURN, ABILITY_OWN, ""),
    // CUnitAI (`FUN_6f9ce510`): fourteen words.
    (codes::class::UNIT_AI, "wwwwwwwwwwwwww", ""),
    // CTask (`FUN_6f696dd0`): five words; `tskA` (`FUN_6f696ec0`): six.
    (codes::class::TASK, "wwwww", ""),
    (codes::class::TASK_A, "wwwwww", ""),
    // CTaskOrderParam: CTaskAction's methods (vf 0x38 `FUN_6f03eda0`, own `FUN_6f696ec0`).
    (codes::class::TASK_O, "wwwwww", ""),
    // CCreepSpot (`FUN_6f9bef80`): two words and a counted list of three words each.
    (codes::class::CREEP_CAMP, "ww [www]", ""),
    // `FUN_6f9ca630`: two words, four reals, nine words.
    (codes::class::TOWN_SITE, "ww ffff wwwwwwwww", ""),
    // `FUN_6f649220`: the ability's and two words; `FUN_6f42e9b0`: a real and three words.
    (codes::ability::id::MILITIA, "wwww wwwwwww t w ww", ""),
    (codes::ability::id::BLIGHT_GROWTH, "wwww wwwwwww t w f www", ""),
    // CAbilityMove (`FUN_6f5fec20`, own `FUN_6f5fec80`: the ability's, then words and
    // reals, three words from build 6000, a word from 6029): two timers.
    (codes::ability::id::MOVE, "wwww wwwwwww t w www ff wwwww ff www w", "t t"),
    // CAbilityCreepSleep (`FUN_6f5224c0`, own `FUN_6f5230a0`: the ability's and a real):
    // two timers and a counted list (`FUN_6f3e2ce0` each, a reference).
    (codes::ability::id::CREEP_SLEEP, "wwww wwwwwww t w f", "t t [r]"),
    // CJassFunc (`FUN_6f211180`): a word.
    (codes::class::HANDLER, "w", "w"),
    // CMapLocation (`FUN_6f27e7d0`): two reals; CRect (`FUN_6f246870`): four.
    (codes::class::LOCATION, "w", "ff"),
    // A hashtable (InitHashtable's): its number among the game's hashtables, from 1, by
    // which the hashtables part names it (read: a test map's two, 1 and 2).
    (codes::class::HASHTABLE, "w", "w"),
    (codes::class::RECT, "w", "ffff"),
    // CTriggerAction (`FUN_6f299650`) and a filter (`FUN_6f211140`): a reference.
    (codes::class::TRIGGER_ACTION, "w", "r"),
    (codes::class::FILTER, "w", "r"),
    // CForce (`FUN_6f2462d0`): three references.
    (codes::class::FORCE, "w", "rrr"),
    // CTimerWar3 (`FUN_6f246c10`, its own `FUN_6f247300`: a word and two reals): its
    // timer (CAgentTimerExtended, `FUN_6f060be0`: a timer, two shorts, a real), a
    // reference, and the list of event 0x80204.
    (codes::class::TIMER, "w ff", "t ss f r [rw]"),
    // CTriggerWar3 (`FUN_6f299840`, its own `FUN_6f2998a0`: six words): three counted
    // lists (`FUN_6f294ba0`, `FUN_6f294c80`, `FUN_6f294c10`).
    (codes::class::TRIGGER, "wwwwww", "[r] [r] [r]"),
    // CTriggerExecution (`FUN_6f2996d0`, own `FUN_6f03edf0`: a word): a counted list of
    // words, an extended timer, a word, a reference, a counted list of references
    // (`FUN_6f294c10`), a real, a reference, two words, events 0x80274's and 0x80273's
    // lists, and a word.
    (codes::class::TRIGGER_EXECUTION, "w", "[w] t ss f w r [r] f r w w e e w"),
    // Event registrations, each ending with its event's handler list: a unit's
    // (`FUN_6f27ede0`, own `FUN_6f27f140`: five words) four words and a reference, a
    // player's (`FUN_6f27e900`, own `FUN_6f27f1b0`: ten words) a reference, a game's
    // (`FUN_6f27e6a0`) only its list.
    (codes::class::UNIT_EVENT, "wwwww", "wwww r [rw]"),
    (codes::class::PLAYER_EVENT, "wwwwwwwwww", "r [rw]"),
    (codes::class::GAME_EVENT, "wwwww", "[rw]"),
    // Registrations on the event-registration base (`FUN_6f27ef60`: event 0x80202's
    // list): a game state's (`FUN_6f27e630`, own `FUN_6f27f260`) two references and two
    // lists; an alliance's (`FUN_6f27e330`, own `FUN_6f27eff0`) a reference and two
    // lists; a player state's (`FUN_6f27e9a0`, own `FUN_6f27f230`) the player
    // registration's, a reference, and event 0x8020b's list; a timer's (`FUN_6f27ec50`)
    // a reference and event 0x80204's list.
    (codes::class::GAME_STATE_EVENT, "wwwwww f w", "e r r e e"),
    (codes::class::ALLIANCE_EVENT, "wwwwww f w www", "e r e e"),
    (codes::class::PLAYER_STATE_EVENT, "wwwwwwwwww w f", "r e r e"),
    (codes::class::TIMER_EXPIRED_EVENT, "wwwww", "r e"),
    // A timer event's registration (CTimerEventReg, TriggerRegisterTimerEvent's): the
    // registration's words, then its timer.
    (codes::class::TIMER_EVENT, "wwwww", "t ss f"),
    // A dialog button's event (CDialogButtonEventReg `vf14_6f27e430`): its button
    // (`FUN_6f21b520`) and event 0x8025a's list.
    (codes::class::DIALOG_BUTTON_EVENT, "wwwww", "r e"),
    // A dialog button (CDialogButton `vf14_6f246090`): its text, hotkey, event
    // 0x8025b's list, and two words (from build 4482); a dialog its buttons and three
    // words (read: the melee defeat dialog; provisional, the dialog's words).
    (codes::class::DIALOG_BUTTON, "w", "z w e w w"),
    (codes::class::DIALOG, "w", "[r] www"),
    // Orders (`FUN_6f6960f0`, own `FUN_6f696b40`: six words): three words from build
    // 6000; a targeted one (`FUN_6f696220`, own `FUN_6f696cf0`) two references more.
    (codes::class::ORDER, "wwwwww", "www"),
    (codes::class::ORDER_TASKS, "wwwwww ff www wwww ff", "www r r"),
    // Tasks: `FUN_6f696330` (own `FUN_6f696f50`) and `FUN_6f696410` (own
    // `FUN_6f6970b0`), a reference each.
    // The buff base's tail (`FUN_6f42d5f0`: from build 6001 a word and two pairs of
    // words): CAbilityAvenger's own a word more from 6003 (`FUN_6f567cf0`).
    (codes::ability::id::VENOM_SPEARS, "wwww wwwwwww t w w", "wwwww"),
    (codes::ability::id::MAGIC_IMMUNITY, "wwww wwwwwww t w", "wwwww"),
    // On it (`FUN_6f42e2b0`): a reference and two timers; (`FUN_6f42d2a0`) three; with
    // own fields `FUN_6f42f3e0` (on `FUN_6f42f4b0`: the ability's, four reals, four words,
    // a real; then four words, two reals, four words, two reals).
    (codes::ability::id::SACRIFICE_ACOLYTE, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"),
    (codes::ability::id::UNSUMMON, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"),
    (codes::ability::id::THUNDER_BOLT_CREEP, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"),
    (codes::ability::id::THUNDER_CLAP_CREEP, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"),
    (codes::ability::id::CHIEFTAIN_SHOCK_WAVE, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"),
    (codes::ability::id::ABOLISH_MAGIC, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t t"),
    (codes::ability::id::HEAL_CREEP_NORMAL, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t t"),
    (codes::ability::id::INNER_FIRE, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t t"),
    (codes::ability::id::CURSE, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t t"),
    (codes::ability::id::BLOODLUST, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t t"),
    (codes::ability::id::FAERIE_FIRE, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t t"),
    (codes::ability::id::WEB, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t t"),
    // The Meat Wagon's: Exhume (`vf14_6f5f5380`) and Meat Load (`FUN_6f42d2a0`) the
    // spell's with three timers, Meat Drop (own `FUN_6f42f4b0`, save `FUN_6f42e2b0`)
    // the spell's first fields and two timers, Plague Toss the buff base's tail
    // (`FUN_6f42d5f0`), its cargo hold (`FUN_6f5aabb0`) the Devour cargo hold's.
    (codes::ability::id::EXHUME, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t t"),
    (codes::ability::id::MEAT_LOAD, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t t"),
    (codes::ability::id::MEAT_DROP, "wwww wwwwwww t w ffff wwww f", "wwwww r t t"),
    (codes::ability::id::PLAGUE_TOSS, "wwww wwwwwww t w", "wwwww"),
    (codes::ability::id::CARGO_HOLD_AMTC, "wwww wwwwwww t w", "f www [wwf] [wwf] [w] [w] t t [ff] [ff]"),
    // Kinds whose layouts were translated from their classes' own (vf18) and save
    // (vf14) functions and fitted to the original's records (`abtest`'s xlate.py and
    // fitall.py, against a map making one of every melee unit, `fwall`).
    (codes::ability::id::WARDEN_BLINK, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t t t"), // CAbilityBlink
    (codes::ability::id::DEMON_HUNTER_IMMOLATION, "wwww wwwwwww t w ffff wwww f w w w", "wwwww r t t t"), // CAbilityImmolation
    (codes::ability::id::DEMON_HUNTER_METAMORPHOSIS, "wwww wwwwwww t w ffff wwww f f f w w f w w f", "wwwww r t t t t t wwwww f w fff w f [ww] f f f [w] f s"), // CAbilityMetamorphosis
    (codes::ability::id::PRIESTESS_STARFALL, "wwww wwwwwww t w ffff wwww f w w w w", "wwwww r t t t"), // CAbilityStarfall
    (codes::ability::id::PRIESTESS_SCOUT, "wwww wwwwwww t w ffff wwww f", "wwwww r t t t"), // CAbilityScout
    (codes::ability::id::KEEPER_TRANQUILITY, "wwww wwwwwww t w ffff wwww f w w w w", "wwwww r t t t"), // CAbilityTranquility
    (codes::ability::id::MOUNTAIN_KING_AVATAR, "wwww wwwwwww t w ffff wwww f w w w", "wwwww r t t t"), // CAbilityAvatar
    (codes::ability::id::MOUNTAIN_KING_BASH, "wwww wwwwwww t w w", "wwwww"), // CAbilityBash
    (codes::ability::id::ARCH_MAGE_BLIZZARD, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t t r [w] w w w"), // CAbilityBlizzard
    (codes::ability::id::BLOOD_MAGE_SIPHON_MANA, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff w w w w w w w w w w w w w", "wwwww r t t t t"), // CAbilityDrain
    (codes::ability::id::PALADIN_DIVINE_SHIELD, "wwww wwwwwww t w ffff wwww f w w w", "wwwww r t t"), // CAbilityDivineShield
    (codes::ability::id::BLOOD_MAGE_FLAME_STRIKE, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff w", "wwwww r t t t t"), // CAbilityFlameStrike
    (codes::ability::id::PALADIN_HOLY_LIGHT, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t t"), // CAbilityHolyBolt
    (codes::ability::id::ARCH_MAGE_MASS_TELEPORT, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff w w w", "wwwww r t t t r r w"), // CAbilityMassTeleport
    (codes::ability::id::PALADIN_RESURRECTION, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t t t"), // CAbilityResurrection
    (codes::ability::id::DEFEND_ITEM, "wwww wwwwwww t w w", "wwwww"), // CAbilityPassiveDefense
    (codes::ability::id::ILLIDAN_CHANNEL, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff w", "wwwww r t t t"), // CAbilityChannel
    (codes::ability::id::ALCHEMIST_CHEMICAL_RAGE, "wwww wwwwwww t w ffff wwww f f f w w f w w", "wwwww r t t t t t wwwww f w fff w f [ww] f f f [w] f s t"), // CAbilityChemicalRage
    (codes::ability::id::TINKERER_CLUSTER_ROCKETS_LEVEL_0, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff w w w w f f f w w w w", "wwwww r t t t t t r"), // CAbilityClusterRockets
    (codes::ability::id::BREWMASTER_DRUNKEN_BRAWLER, "wwww wwwwwww t w w", "wwwww"), // CAbilityDrunkenBrawler
    (codes::ability::id::MALGANIS_DARK_CONVERSION, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff w w", "wwwww r t t r t"), // CAbilityDarkConversion
    (codes::ability::id::TINKERER_DEMOLISH_LEVEL_0, "wwww wwwwwww t w f w", "wwwww"), // CAbilityDemolish
    (codes::ability::id::BREWMASTER_DRUNKEN_HAZE, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t r"), // CAbilityDrunkenHaze
    (codes::ability::id::BREWMASTER_STORM_EARTH_AND_FIRE, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff w w w w", "wwwww r t t t"), // CAbilityElementalFury
    (codes::ability::id::TINKERER_ENGINEERING_UPGRADE, "wwww wwwwwww t w w w w f f w", "wwwww"), // CAbilityEngineeringUpgrade
    (codes::ability::id::FINGER_OF_DEATH, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff w w w", "wwwww r t t t r"), // CAbilityFingerOfDeath
    (codes::ability::id::ALCHEMIST_HEALING_SPRAY, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff w w w w f f f w w w w", "wwwww r t t t t t r"), // CAbilityHealingSpray
    (codes::ability::id::INFERNO, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t r"), // CAbilityInferno
    (codes::ability::id::MONSOON, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff w w w", "wwwww r t t"), // CAbilityMonsoon
    (codes::ability::id::SEA_WITCH_MANA_SHIELD, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff w w w", "wwwww r t t"), // CAbilityManaShield
    (codes::ability::id::RAIN_OF_CHAOS, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff w w w w", "wwwww r t t"), // CAbilityRainOfChaos
    (codes::ability::id::PIT_LORD_RAIN_OF_FIRE, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t t r [w] w w w"), // CAbilityRainOfFire
    (codes::ability::id::DARK_RANGER_SILENCE, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t r"), // CAbilitySilence
    (codes::ability::id::MALGANIS_SOUL_PRESERVATION, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff w", "wwwww r t t r t"), // CAbilitySoulPreservation
    (codes::ability::id::BEAST_MASTER_STAMPEDE, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff f w", "wwwww r t t t t t"), // CAbilityStampede
    (codes::ability::id::SEA_WITCH_TORNADO, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff w w w w", "wwwww r t t r t t"), // CAbilityTornado
    (codes::ability::id::FIRELORD_VOLCANO, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff w w w w", "wwwww r t t t r"), // CAbilityVolcano
    (codes::ability::id::FARSEER_CHAIN_LIGHTNING, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t w f [w w w] [w w w]"), // CAbilityChainLightning
    (codes::ability::id::FARSEER_EARTHQUAKE, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff f f w w w w", "wwwww r t t r t t"), // CAbilityEarthquake
    (codes::ability::id::SHADOW_HUNTER_HEALING_WAVE, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t w f [w w w] [w w w]"), // CAbilityHealingWave
    (codes::ability::id::SHADOW_HUNTER_HEX, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff w", "wwwww r t t r"), // CAbilityHex
    (codes::ability::id::BLADE_MASTER_MIRROR_IMAGE, "wwww wwwwwww t w ffff wwww f", "wwwww r t t w w w [w w w] t w r"), // CAbilityMirrorImage
    (codes::ability::id::SHADOW_HUNTER_VOODOOO, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff w w w w", "wwwww r t t t r t t"), // CAbilityVoodoo
    (codes::ability::id::BLADE_MASTER_BLADESTORM, "wwww wwwwwww t w ffff wwww f w w f f", "wwwww r t t t"), // CAbilityWhirlwind
    (codes::ability::id::DEATH_KNIGHT_ANIMATE_DEAD, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t t"), // CAbilityAnimateDead
    (codes::ability::id::CRYPT_LORD_CARRION_SCARABS, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff w w", "wwwww r t t t t"), // CAbilityCarrionScarabs
    (codes::ability::id::LICH_DEATH_AND_DECAY, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff w w w w", "wwwww r t t r t"), // CAbilityDeathAndDecay
    (codes::ability::id::DEATH_KNIGHT_DEATH_PACT, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t t"), // CAbilityDeathPact
    (codes::ability::id::LICH_DARK_RITUAL, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t t"), // CAbilityDarkRitual
    (codes::ability::id::CRYPT_LORD_IMPALE, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff f f f f w f", "wwwww r t t t"), // CAbilityImpale
    (codes::ability::id::DREADLORD_INFERNO, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t r"), // CAbilityDreadLordInferno
    (codes::ability::id::CRYPT_LORD_LOCUST_SWARM, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t [w w w] w w w t t"), // CAbilityLocustSwarm
    (codes::ability::id::CRYPT_LORD_SPIKED_CARAPACE, "wwww wwwwwww t w f w w w", "wwwww t"), // CAbilityThornyShield
    (codes::ability::id::DISEASE_CLOUD, "wwww wwwwwww t w w f f w w w", "[r] w r t w w w w w"), // CAbilityAuraPlague
    (codes::ability::id::ANCESTRAL_SPIRIT, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t t"), // CAbilityAncestralSpirit
    (codes::ability::id::AVENGER_FORM, "wwww wwwwwww t w ffff wwww f f f w w f w w", "wwwww r t t t t t wwwww f w fff w f [ww] f f f [w] f s"), // CAbilityAvengerForm
    (codes::ability::id::BEARFORM, "wwww wwwwwww t w ffff wwww f f f w w f w w", "wwwww r t t t t t wwwww f w fff w f [ww] f f f [w] f s t"), // CAbilityBearForm
    (codes::ability::id::BESERK, "wwww wwwwwww t w ffff wwww f w", "wwwww r t t t"), // CAbilityBerserkerRage
    (codes::ability::id::BATTLESTATIONS, "wwww wwwwwww t w ffff wwww f w", "wwwww r t t t"), // CAbilityBattlestations
    (codes::ability::id::CARGO_HOLD_BURROW, "wwww wwwwwww t w", "f w w w [w w w] [w w w] [w] [w] t t [f f] [f f]"), // CAbilityBunker
    (codes::ability::id::CARGO_HOLD_ACAR, "wwww wwwwwww t w", "f w w w [w w w] [w w w] [w] [w] t t [f f] [f f]"), // CAbilityCargoHold
    (codes::ability::id::CHAOS, "wwww wwwwwww t w w", "t t"), // CAbilityChaos
    (codes::ability::id::CLOUD_OF_FOG, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff w w w", "wwwww r t t r"), // CAbilityCloudOfFog
    (codes::ability::id::CORPOREAL_FORM, "wwww wwwwwww t w ffff wwww f f f w w f w w", "wwwww r t t t t t wwwww f w fff w f [ww] f f f [w] f s t"), // CAbilityCorporealForm
    (codes::ability::id::CYCLONE, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t r"), // CAbilityCyclone
    (codes::ability::id::DEFEND, "wwww wwwwwww t w w", "wwwww t"), // CAbilityDefend
    (codes::ability::id::DETECTOR, "wwww wwwwwww t w f w", "wwwww r r"), // CAbilityDetector
    (codes::ability::id::DEVOUR, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff w", "wwwww r t t t"), // CAbilityDevour
    (codes::ability::id::DISPEL_MAGIC, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t t"), // CAbilityDispelMagic
    (codes::ability::id::DROP, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff f f f f f f", "wwwww r t t t t"), // CAbilityCargoDrop
    (codes::ability::id::DETONATE, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t t"), // CAbilityDetonate
    (codes::ability::id::DETECT_MAGIC_SENTINEL, "wwww wwwwwww t w f w", "wwwww r r"), // CAbilityMagicSentry
    (codes::ability::id::ENTANGLED_GOLD_MINE, "wwww wwwwwww t w w w w w w f f", "r r r t"), // CAbilityEntangledGoldMine
    (codes::ability::id::CARGO_HOLD_GOLD_MINE, "wwww wwwwwww t w", "f w w w [w w w] [w w w] [w] [w] t t [f f] [f f]"), // CAbilityEntangleCargo
    (codes::ability::id::ENTANGLE, "wwww wwwwwww t w f w w w w w w", "r t"), // CAbilityEntangle
    (codes::ability::id::SENTINEL, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff w", "wwwww r t t r t"), // CAbilitySentinel
    (codes::ability::id::ON_FIRE_NIGHT_ELF, "wwww wwwwwww t w", "w w w t"), // CAbilityOnFireNightElf
    (codes::ability::id::ON_FIRE_ORC, "wwww wwwwwww t w", "w w w t"), // CAbilityOnFireOrc
    (codes::ability::id::FLARE, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff w", "wwwww r t t"), // CAbilityFlare
    (codes::ability::id::GHOST, "wwww wwwwwww t w w", "[r] w"), // CAbilityGhost
    (codes::ability::id::DETECT_GYROCOPTER, "wwww wwwwwww t w f w", "wwwww r r"), // CAbilityGyroVision
    (codes::ability::id::HEALING_WARD, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t t"), // CAbilityHealingWard
    (codes::ability::id::LIGHTNING_ATTACK, "wwww wwwwwww t w f w w w", "w"), // CAbilityLightningAttack
    (codes::ability::id::LOAD, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff w", "wwwww r t t t"), // CAbilityCargoLoad
    (codes::ability::id::LOCUST, "wwww wwwwwww t w w w w f w f f f w w w w", "t"), // CAbilityLocust
    (codes::ability::id::MANA_BATTERY, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff w w w f", "wwwww r t t t r t r w f f f"), // CAbilityManaBattery
    (codes::ability::id::MANA_FLARE, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff f w", "wwwww r t t t"), // CAbilityManaFlare
    (codes::ability::id::MINE, "wwww wwwwwww t w f", "t r"), // CAbilityLandMine
    (codes::ability::id::AERIAL_SHACKLES, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff w w w w w w w w w", "wwwww r t t t t"), // CAbilityMagicLeash
    (codes::ability::id::PHOENIX, "wwww wwwwwww t w ffff wwww f f f w w f w w w w w w", "wwwww r t t t t t wwwww f w fff w f [ww] f f f [w] f s"), // CAbilityPhoenix
    (codes::ability::id::PERMANENT_IMMOLATION_GRAPHIC, "wwww wwwwwww t w w w w", "wwwww"), // CAbilityPandaImmolation
    (codes::ability::id::PERMANENT_INVISIBILITY, "wwww wwwwwww t w w", "[r] w"), // CAbilityPermInvis
    (codes::ability::id::POLYMORPH, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff w", "wwwww r t t r"), // CAbilityPolymorph
    (codes::ability::id::POSSESSION_CHANNELING, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff w w w w", "wwwww r t t t"), // CAbilityPossessionTwo
    (codes::ability::id::PHASE_SHIFT, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff w w w", "wwwww r t t"), // CAbilityPhaseShift
    (codes::ability::id::PHOENIX_FIRE, "wwww wwwwwww t w", "t"), // CAbilityPhoenixFire
    (codes::ability::id::RAVEN_FORM_DRUID, "wwww wwwwwww t w ffff wwww f f f w w f w w", "wwwww r t t t t t wwwww f w fff w f [ww] f f f [w] f s"), // CAbilityRavenForm
    (codes::ability::id::REINFORCED_BURROWS, "wwww wwwwwww t w w f w w", "[r] w"), // CAbilityReinforce
    (codes::ability::id::RENEW, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff w w w f f f w w w f w w w", "wwwww r t t t t t t t r"), // CAbilityRenew
    (codes::ability::id::ROOT, "wwww wwwwwww t w w w f f f f w", "wwwww t t"), // CAbilityRoot
    (codes::ability::id::REPLENISH_LIFE, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff f", "wwwww r t t t"), // CAbilityReplenishLife
    (codes::ability::id::REPLENISH_MANA, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff f", "wwwww r t t t"), // CAbilityReplenishMana
    (codes::ability::id::SACRIFICE_SACRIFICIAL_PIT, "wwww wwwwwww t w f f", "r r r r [w] w f [w w w]"), // CAbilitySacrifice
    (codes::ability::id::PILLAGE, "wwww wwwwwww t w f f", "wwwww"), // CAbilitySalvage
    (codes::ability::id::SELF_DESTRUCT, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff w w w", "wwwww r t t t"), // CAbilitySelfDestruct
    (codes::ability::id::SPHERE, "wwww wwwwwww t w w w w w f f w f", "[r] w t t"), // CAbilitySphere
    (codes::ability::id::SPIKED_BARRICADES, "wwww wwwwwww t w w f w", "[r] w"), // CAbilitySpiked
    (codes::ability::id::SPIRIT_LINK, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff w", "wwwww r t t"), // CAbilitySpiritLink
    (codes::ability::id::SLOW_POISON, "wwww wwwwwww t w w", "wwwww"), // CAbilitySlowPoison
    (codes::ability::id::HARDENED_SKIN, "wwww wwwwwww t w w", "wwwww"), // CAbilityStoneSkin
    (codes::ability::id::STONE_FORM, "wwww wwwwwww t w ffff wwww f f f w w f w w", "wwwww r t t t t t wwwww f w fff w f [ww] f f f [w] f s r t"), // CAbilityStoneForm
    (codes::ability::id::TREE_OF_LIFE_FOR_ATTACHING_ART, "wwww wwwwwww t w w", "[r] w"), // CAbilityTreeOfLife
    (codes::ability::id::DETECT_SHADE, "wwww wwwwwww t w f w", "wwwww r r"), // CAbilityTrueSight
    (codes::ability::id::UNSTABLE_CONCOCTION, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff w w w w w", "wwwww r t t t"), // CAbilityUnstableConcoction
    (codes::ability::id::ULTRAVISION, "wwww wwwwwww t w f", ""), // CAbilityUltraVision
    (codes::ability::id::VENGEANCE, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff w w", "wwwww r t t t t"), // CAbilityVengeance
    (codes::ability::id::PULVERIZE, "wwww wwwwwww t w f w", "wwwww"), // CAbilityWarStomp
    (codes::buff::id::BRILLIANCE_AURA, "wwww wwwwwww t w w f www fff f w w w w", "[r] w t ss f r t"), // CBuffAuraBrilliance
    (codes::buff::id::STUNNED_BPSE, "wwww wwwwwww t w w f www fff w w w", "[r] w t ss f"), // CBuffPause
    (codes::buff::id::THORNY_SHIELD, "wwww wwwwwww t w w", "[r] w"), // CBuffThornyShield
    (codes::buff::id::CURSE, "wwww wwwwwww t w w f www fff f w", "[r] w t ss f"), // CBuffCurse
    (codes::buff::id::SLOWED, "wwww wwwwwww t w w f www fff f f", "[r] w t ss f t"), // CBuffFrost
    (codes::buff::id::MILITIA, "wwww wwwwwww t w w f www fff f w w w", "[r] w t ss f"), // CBuffMilitia
    (codes::buff::id::PHOENIX, "wwww wwwwwww t w w f www fff f", "[r] w t ss f"), // CBuffPhoenix
    (codes::buff::id::PANDA_IMMOLATION, "wwww wwwwwww t w w f www fff w f f f w w w w w w w f", "[r] w t ss f t r r"), // CBuffPandaImmolation
    // Wisp Harvest (the harvest base's save, `vf14_6f42d950`, and its timer) and Dark
    // Portal (`vf14_6f4ea1e0`: a counted list of its unit types, a word, two reals, a
    // reference), fitted to the original's; Robo-Goblin (a morph, two fields more),
    // the regeneration aura's buff (`FUN_6f42e550`: as Unholy Aura's tail), and the
    // poison buff, as their classes' functions read (no record of the original's yet).
    (codes::ability::id::WISP_HARVEST, "wwww wwwwwww t w f f w f f w w w w w w", "[www] [t] wwwwww f t"),
    (codes::ability::id::DARK_PORTAL, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t t [w] w f f r"),
    (codes::ability::id::TINKERER_ROBO_GOBLIN_LEVEL_0, "wwww wwwwwww t w ffff wwww f f f w w f w w w f", "wwwww r t t t t t wwwww f w fff w f [ww] f f f [w] f s t"),
    (codes::buff::id::HEALING_WARD_AURA, "wwww wwwwwww t w w f www fff f www", "[r] w t ss f r t"),
    (codes::buff::id::POISON_BPOI, "wwww wwwwwww t w w f www fff f w w w f f f", "[r] w t ss f t ss f"),
    // BEGIN genlayouts.py: the classes read from the decompile alone (tools/saves).
    (codes::ability::id::AUTO_TARGET_SPELL, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t t"), // CAbilityAutoTargetSpell
    (codes::ability::id::BUTTON, "wwww wwwwwww t w", "wwwww"), // CAbilityButton
    (codes::ability::id::CLOSEST_TARGET_SPELL, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"), // CAbilityClosestTargetSpell
    (codes::ability::id::INTERFACED, "wwww wwwwwww t w", ""), // CAbilityInterfaced
    (codes::ability::id::MODAL_SPELL, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"), // CAbilityModalSpell
    (codes::ability::id::NEUTRAL_SPELL, "wwww wwwwwww t w w", "wwwww"), // CAbilityNeutralSpell
    (codes::ability::id::SIMPLE_SPELL, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"), // CAbilitySimpleSpell
    (codes::ability::id::SPELL, "wwww wwwwwww t w ffff wwww f", "wwwww r t t"), // CAbilitySpell
    (codes::ability::id::BASE_BUILD, "wwww wwwwwww t w f f", "r r"), // CAbilityBaseBuild
    (codes::ability::id::BONUS_BASE, "wwww wwwwwww t w w", "[r] w"), // CBonusBase
    (codes::ability::id::BUILD_QUEUE, "wwww wwwwwww t w f f", "r r r r"), // CAbilityBuildQueue
    (codes::ability::id::POISON_ARROWS, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t t"), // CAbilityPoisonArrows
    (codes::ability::id::STARFALL_DRAIN, "wwww wwwwwww t w w w w w f f", "t t t"), // CAbilityStarfallDrain
    (codes::ability::id::TRANQUILITY_REGEN, "wwww wwwwwww t w w f w", "[r] w r r"), // CAbilityTranquilityRegen
    (codes::ability::id::RANGER_ARROW, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t t"), // CAbilityRangerArrow
    (codes::ability::id::ATTACK_MOD, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff w", "wwwww r t t"), // CAbilityAttackMod
    (codes::ability::id::ITEM_HERO_STAT_BONUS, "wwww wwwwwww t w w f w w w", "[r] w"), // CAbilityAttributeBonus
    (codes::ability::id::AGILITY_MOD, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff w w w", "wwwww r t t"), // CAbilityAgilityMod
    (codes::ability::id::ANIMATE_DEAD, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t t"), // CAbilityItemAnimateDead
    (codes::ability::id::ITEM_ATTACK_SPEED_BONUS, "wwww wwwwwww t w w f", "[r] w t"), // CAbilityAttackSpeedBonus
    (codes::ability::id::ATTACK_BONUS, "wwww wwwwwww t w w f", "[r] w t"), // CAbilityAttackBonus
    (codes::ability::id::BUILD_TINY_CASTLE, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff w", "wwwww r t t"), // CAbilityItemBuild
    (codes::ability::id::ITEM_CURE_AOE, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"), // CAbilityItemCureAoe
    (codes::ability::id::ORB_OF_CORRUPTION, "wwww wwwwwww t w w f w", "[r] w t t"), // CAbilityCorruptionDamageBonus
    (codes::ability::id::ITEM_CLOAK_OF_FLAMES, "wwww wwwwwww t w w w w", "wwwww"), // CAbilityItemCloakOfFlames
    (codes::ability::id::ITEM_COMMAND, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"), // CAbilityItemCommand
    (codes::ability::id::ITEM_CHANGE_TOD, "wwww wwwwwww t w ffff wwww f", "wwwww r t t"), // CAbilityItemChangeTOD
    (codes::ability::id::ITEM_DEFENSE_AOE, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"), // CAbilityItemDefenseAoe
    (codes::ability::id::ITEM_ARMOR_BONUS, "wwww wwwwwww t w w f", "[r] w t"), // CAbilityDefenseBonus
    (codes::ability::id::ITEM_DISPEL_AOE, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t t"), // CAbilityItemDispelAoe
    (codes::ability::id::ITEM_DAMAGE_AOE, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"), // CAbilityItemDamageAoe
    (codes::ability::id::EXPERIENCE_MOD, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff w", "wwwww r t t"), // CAbilityExperienceMod
    (codes::ability::id::FLARE_GUN, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"), // CAbilityItemFlare
    (codes::ability::id::ORB_OF_FIRE, "wwww wwwwwww t w w f w", "[r] w t t"), // CAbilityFireDamageBonus
    (codes::ability::id::ITEM_FLYING_CARPET, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"), // CAbilityItemFlyingCarpet
    (codes::ability::id::FIGURINE_RED_DRAKE, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"), // CAbilityFigurineRedDrake
    (codes::ability::id::FLAG_UNDEAD, "wwww wwwwwww t w w", "[r] w"), // CAbilityFlagUndead
    (codes::ability::id::FIGURINE_FURBOLG, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"), // CAbilityFigurineFurbolg
    (codes::ability::id::CLOUD_OF_FOG_ITEM, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"), // CAbilityFigurine
    (codes::ability::id::FIGURINE_FEL_HOUND, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"), // CAbilityFigurineFelHound
    (codes::ability::id::FLAG, "wwww wwwwwww t w w", "[r] w"), // CAbilityFlag
    (codes::ability::id::FLAG_HUMAN, "wwww wwwwwww t w w", "[r] w"), // CAbilityFlagHuman
    (codes::ability::id::FLAG_NIGHT_ELF, "wwww wwwwwww t w w", "[r] w"), // CAbilityFlagNightElf
    (codes::ability::id::FLAG_ORC, "wwww wwwwwww t w w", "[r] w"), // CAbilityFlagOrc
    (codes::ability::id::FIGURINE_ROCK_GOLEM, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"), // CAbilityFigurineRockGolem
    (codes::ability::id::FIGURINE_SKELETON, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"), // CAbilityFigurineSkeleton
    (codes::ability::id::FIGURINE_DOOM_GUARD, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"), // CAbilityFigurineDoomGuard
    (codes::ability::id::FORTIFICATION_GLYPH, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff w", "wwwww r t t"), // CAbilityGlyph
    (codes::ability::id::GIVE_GOLD, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff w", "wwwww r t t"), // CAbilityItemGiveGold
    (codes::ability::id::ITEM_HEAL_AOE, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"), // CAbilityItemHealAoe
    (codes::ability::id::ITEM_HEALING, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff f", "wwwww r t t"), // CAbilityItemHeal
    (codes::ability::id::ITEM_ILLUSION, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t r"), // CAbilityItemIllusion
    (codes::ability::id::INTELLIGENCE_MOD, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff w w w", "wwwww r t t"), // CAbilityIntelligenceMod
    (codes::ability::id::ORB_OF_LIGHTNING_OLD, "wwww wwwwwww t w w f w", "[r] w t t"), // CAbilityLightningDamageBonus
    (codes::ability::id::LEVEL_MOD, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff w", "wwwww r t t"), // CAbilityLevelMod
    (codes::ability::id::LIGHTNING_PURGE, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"), // CAbilityLightningPurge
    (codes::ability::id::GIVE_LUMBER, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff w", "wwwww r t t"), // CAbilityItemGiveLumber
    (codes::ability::id::ITEM_MANA_REGAIN, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff f f", "wwwww r t t"), // CAbilityItemManaRestore
    (codes::ability::id::ITEM_PERMANENT_LIFE_GAIN, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff w", "wwwww r t t"), // CAbilityMaxLifeMod
    (codes::ability::id::ITEM_LIFE_BONUS, "wwww wwwwwww t w w f", "[r] w"), // CAbilityMaxLifeBonus
    (codes::ability::id::ITEM_MANA_BONUS, "wwww wwwwwww t w w f", "[r] w"), // CAbilityMaxManaBonus
    (codes::ability::id::MAX_MANA_MOD, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff w", "wwwww r t t"), // CAbilityMaxManaMod
    (codes::ability::id::ITEM_MONSTER_LURE, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"), // CAbilityItemMonsterLure
    (codes::ability::id::ITEM_MANA_RESTORE_AOE, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff f f f", "wwwww r t t"), // CAbilityItemManaRestoreAoe
    (codes::ability::id::MOVE_SPEED_BONUS, "wwww wwwwwww t w w f", "[r] w t"), // CAbilityMoveSpeedBonus
    (codes::ability::id::ORB_OF_FROST, "wwww wwwwwww t w w f w", "[r] w t t"), // CAbilityFrostDamageBonus
    (codes::ability::id::ORB_OF_VENOM, "wwww wwwwwww t w w f w", "[r] w t t"), // CAbilityPoisonDamageBonus
    (codes::ability::id::ITEM_PERM_INVIS, "wwww wwwwwww t w w", "[r] w"), // CAbilityItemPermInvis
    (codes::ability::id::ITEM_PLACE_MINE, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"), // CAbilityItemPlaceMine
    (codes::ability::id::ITEM_POTION_VAMPIRISM, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"), // CAbilityItemVampirePotion
    (codes::ability::id::ITEM_RESTORE_AOE, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff f f f f", "wwwww r t t"), // CAbilityItemRestoreAoe
    (codes::ability::id::RUNE_OF_REBIRTH, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff w", "wwwww r t t"), // CAbilityRebirth
    (codes::ability::id::ITEM_REINCARNATION, "wwww wwwwwww t w w w w", ""), // CAbilityItemReincarnation
    (codes::ability::id::RAISE_DEAD_ITEM, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff w w", "wwwww r t t t t"), // CAbilityItemRaiseDead
    (codes::ability::id::ITEM_RESTORE, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff f f f", "wwwww r t t"), // CAbilityItemRestore
    (codes::ability::id::POTION_OF_LIFE_REGEN, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t t"), // CAbilityRegeneration
    (codes::ability::id::ITEM_RANDOM_ITEM, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"), // CAbilityItemRandomItem
    (codes::ability::id::ITEM_REGEN_MANA, "wwww wwwwwww t w w f", "[r] w r r r r"), // CAbilityItemRegenMana
    (codes::ability::id::RESURRECTION, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t t t"), // CAbilityItemResurrection
    (codes::ability::id::ITEM_RECALL, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t w w"), // CAbilityItemRecall
    (codes::ability::id::ITEM_REVEAL_MAP, "wwww wwwwwww t w ffff wwww f", "wwwww r t t"), // CAbilityItemRevealMap
    (codes::ability::id::ITEM_SPEED_AOE, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"), // CAbilityItemSpeedAoe
    (codes::ability::id::ORB_OF_SPELLS, "wwww wwwwwww t w w f w w w w w w w w w w", "[r] w t t"), // CAbilitySpellEffectBonus
    (codes::ability::id::SUMMON_HEADHUNTER_ITEM, "wwww wwwwwww t w ffff wwww f", "wwwww r t t"), // CAbilitySpiritTroll
    (codes::ability::id::SIGHT_BONUS, "wwww wwwwwww t w w f", "[r] w"), // CAbilitySightBonus
    (codes::ability::id::STRENGTH_MOD, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff w w w", "wwwww r t t"), // CAbilityStrengthMod
    (codes::ability::id::SOUL_TRAP, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t r"), // CAbilitySoulTrap
    (codes::ability::id::ITEM_SPEED, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"), // CAbilityItemSpeed
    (codes::ability::id::RUNED_BRACERS, "wwww wwwwwww t w w f", "[r] w t"), // CAbilityItemSpellReduction
    (codes::ability::id::DUST_OF_APPEARANCE, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"), // CAbilityItemDetectByBuff
    (codes::ability::id::ITEM_TELEPORT, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t w w r r"), // CAbilityItemTeleport
    (codes::ability::id::TOME, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff w", "wwwww r t t"), // CAbilityTome
    (codes::ability::id::VAMPIRIC_ATTACK, "wwww wwwwwww t w w f w", "[r] w"), // CAbilityVampireBonus
    (codes::ability::id::ITEM_TEMPORARY_INVISIBILITY, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"), // CAbilityItemInvis
    (codes::ability::id::ITEM_INVUL, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"), // CAbilityItemInvul
    (codes::ability::id::ITEM_WEB, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"), // CAbilityItemWeb
    (codes::ability::id::PERMANENT_ALL_1, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff w w w", "wwwww r t t"), // CAbilityAttributeMod
    (codes::ability::id::BATTLE_ROAR_CLASS, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"), // CAbilityBattleRoar
    (codes::ability::id::ORB_OF_DARKNESS_BLACK_ARROW, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"), // CAbilityBlackArrowSpell
    (codes::ability::id::FACTORY, "wwww wwwwwww t w", "wwwww t"), // CAbilityFactory
    (codes::ability::id::EXCHANGE_GOLD_FOR_LUMBER, "wwww wwwwwww t w w", "wwwww"), // CAbilityNeutralG2L
    (codes::ability::id::FIRELORD_INCINERATE_ANIC, "wwww wwwwwww t w", "wwwww"), // CAbilityIncinerate
    (codes::ability::id::EXCHANGE_LUMBER_FOR_GOLD, "wwww wwwwwww t w w", "wwwww"), // CAbilityNeutralL2G
    (codes::ability::id::MONSOON_DRAIN, "wwww wwwwwww t w w w w w f f", "t t t"), // CAbilityMonsoonDrain
    (codes::ability::id::MIND_ROT_CLASS, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"), // CAbilityMindRot
    (codes::ability::id::PERMANENT_IMMOLATION, "wwww wwwwwww t w w w w", "wwwww"), // CAbilityPermImmolation
    (codes::ability::id::PRESERVATION, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"), // CAbilityPreservation
    (codes::ability::id::NEUTRAL_REGEN, "wwww wwwwwww t w", "r"), // CAbilityNeutralRegen
    (codes::ability::id::NEUTRAL_REGEN_LIFE, "wwww wwwwwww t w w f", "[r] w r r"), // CAbilityNeutralRegenLife
    (codes::ability::id::NEUTRAL_REGEN_MANA, "wwww wwwwwww t w w f", "[r] w r r"), // CAbilityNeutralRegenMana
    (codes::ability::id::SANCTUARY, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"), // CAbilitySanctuary
    (codes::ability::id::SPELL_SHIELD_AOE, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"), // CAbilitySpellShieldAoe
    (codes::ability::id::SPY, "wwww wwwwwww t w w", "wwwww"), // CAbilityNeutralSpies
    (codes::ability::id::SPELL_SHIELD, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"), // CAbilitySpellShield
    (codes::ability::id::SUBMERGE_MYRMIDON, "wwww wwwwwww t w ffff wwww f f f w w f w w", "wwwww r t t t t t wwwww f w fff w f [ww] f f f [w] f s t"), // CAbilitySubmerge
    (codes::ability::id::THORNY_SHIELD_CREEP, "wwww wwwwwww t w", "wwwww"), // CAbilityThorns
    (codes::ability::id::PASSIVE, "wwww wwwwwww t w", "wwwww"), // CAbilityPassive
    (codes::ability::id::PERSISTENT_BONUS, "wwww wwwwwww t w w f", "[r] w"), // CAbilityPersistentBonus
    (codes::ability::id::REGEN_BASE, "wwww wwwwwww t w w f", "[r] w r r"), // CAbilityRegenBase
    (codes::ability::id::RESEARCH, "wwww wwwwwww t w f f w w", "r r"), // CAbilityResearch
    (codes::ability::id::SACRIFICE_IN_PROGRESS, "wwww wwwwwww t w f f", "r r"), // CAbilitySacrificeInProgress
    (codes::ability::id::TRAIN, "wwww wwwwwww t w f f", "r r r r [w] w"), // CAbilityTrain
    (codes::ability::id::DARK_SUMMONING, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff w [w w w]", "wwwww r t t [r] w w"), // CAbilityDarkSummoning
    (codes::ability::id::LICH_FROST_ARMOR, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"), // CAbilityFrostArmor
    (codes::ability::id::AURA_BLIGHT_REGEN, "wwww wwwwwww t w w f f", "[r] w r t w w w w w"), // CAbilityAuraBlightRegen
    (codes::ability::id::ITEM_ANTI_MAGIC_SHELL, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"), // CAbilityAntiMagicShellInstant
    (codes::ability::id::ATTRIBUTE_MODIFIER_SKILL, "wwww wwwwwww t w w w w w", "wwwww"), // CAbilityAttributeModSkill
    (codes::ability::id::AURA_SLOW, "wwww wwwwwww t w w f f w w w", "[r] w r t w w w w w"), // CAbilityAuraSlow
    (codes::ability::id::AURA_SPELL, "wwww wwwwwww t w ffff wwww f", "wwwww r t t"), // CAbilityAuraSpell
    (codes::ability::id::BARKSKIN, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"), // CAbilityBarkskin
    (codes::ability::id::BLIGHT_PLACEMENT, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"), // CAbilityBlightPlacement
    (codes::ability::id::BOUNCE, "wwww wwwwwww t w ffff wwww f", "wwwww r t t"), // CAbilityBounce
    (codes::ability::id::CALL_TO_ARMS, "wwww wwwwwww t w", ""), // CAbilityCallToArms
    (codes::ability::id::CHAOS_BASE, "wwww wwwwwww t w w", "t t"), // CAbilityChaosBase
    (codes::ability::id::CHAOS_CARGO_LOAD, "wwww wwwwwww t w w", "t t"), // CAbilityChaosCargoLoad
    (codes::ability::id::COUPLE_ARCHER, "wwww wwwwwww t w f w w w", ""), // CAbilityCoupleArcher
    (codes::ability::id::COUPLE_HIPPOGRYPH, "wwww wwwwwww t w f w w w", ""), // CAbilityCoupleHippogryph
    (codes::ability::id::COUPLE, "wwww wwwwwww t w f w w w", ""), // CAbilityCouple
    (codes::ability::id::CARGO, "wwww wwwwwww t w w w w", ""), // CAbilityCargo
    (codes::ability::id::DISENCHANT_OLD, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t t"), // CAbilityDisenchant
    (codes::ability::id::NOTIFY_DAMAGE, "wwww wwwwwww t w w w w", ""), // CAbilityNotifyDamage
    (codes::ability::id::REVEAL, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"), // CAbilityDetectAoe
    (codes::ability::id::ETHEREAL_FORM, "wwww wwwwwww t w ffff wwww f f f w w f w w", "wwwww r t t t t t wwwww f w fff w f [ww] f f f [w] f s"), // CAbilityEtherealForm
    (codes::ability::id::ETHEREAL, "wwww wwwwwww t w", "wwwww"), // CAbilityEthereal
    (codes::ability::id::ON_FIRE_BASE, "wwww wwwwwww t w w", "[r] w"), // COnFireBase
    (codes::ability::id::ON_FIRE, "wwww wwwwwww t w", "w w w t"), // CAbilityOnFire
    (codes::ability::id::GOLD_MINE_BASE, "wwww wwwwwww t w w", "r"), // CAbilityGoldMineBase
    (codes::ability::id::GRAVEYARD_CORPSE, "wwww wwwwwww t w w w w", ""), // CAbilityGraveyardCorpse
    (codes::ability::id::LEASH, "wwww wwwwwww t w f f w", "t"), // CAbilityLeash
    (codes::ability::id::MAGIC_DEFENSE, "wwww wwwwwww t w w", "wwwww t"), // CAbilityMagicDefense
    (codes::ability::id::MECHANICAL_CRITTER, "wwww wwwwwww t w ffff wwww f", "wwwww r t t"), // CAbilityMechanicalCritter
    (codes::ability::id::MORPH, "wwww wwwwwww t w ffff wwww f f f w w f w w", "wwwww r t t t t t wwwww f w fff w f [ww] f f f [w] f s"), // CAbilityMorph
    (codes::ability::id::MOUNT, "wwww wwwwwww t w ffff wwww f f f w w f w w w", "wwwww r t t t t t wwwww f w fff w f [ww] f f f [w] f s"), // CAbilityMount
    (codes::ability::id::NIGHT_VISION, "wwww wwwwwww t w f", ""), // CAbilityNightVision
    (codes::ability::id::ORB_OF_VENOM_POISON_ATTACK, "wwww wwwwwww t w w", "wwwww"), // CAbilityPoisonDamage
    (codes::ability::id::POISON_ATTACK, "wwww wwwwwww t w w", "wwwww"), // CAbilityPoisonAttack
    (codes::ability::id::REGEN_LIFE, "wwww wwwwwww t w w f", "[r] w r r"), // CAbilityRegenLife
    (codes::ability::id::REGEN_MANA, "wwww wwwwwww t w w f", "[r] w r r"), // CAbilityRegenMana
    (codes::ability::id::RETRAIN, "wwww wwwwwww t w ffff wwww f", "wwwww r t t"), // CAbilityRetrain
    (codes::ability::id::REPLENISH_LIFE_AND_MANA, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff f", "wwwww r t t t"), // CAbilityReplenish
    (codes::ability::id::RESCUABLE, "wwww wwwwwww t w w f", "r"), // CAbilityRescuable
    (codes::ability::id::SHADOW_SIGHT, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"), // CAbilityShadowSight
    (codes::ability::id::SLEEP_ALWAYS, "wwww wwwwwww t w f", "t t [r]"), // CAbilitySleepAlways
    (codes::ability::id::DEATH_SPAWN, "wwww wwwwwww t w", ""), // CAbilityDeathSpawn
    (codes::ability::id::SOUL_TRAPPED, "wwww wwwwwww t w w w w w w w w", ""), // CAbilitySoulTrapped
    (codes::ability::id::SPELL_BOOK, "wwww wwwwwww t w ffff wwww f [w] [w w w] w", "wwwww r t t"), // CAbilitySpellBook
    (codes::ability::id::MANA_STEAL, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t t"), // CAbilitySteal
    (codes::ability::id::TORNADO_DAMAGE, "wwww wwwwwww t w w w w", "t"), // CAbilityTornadoDamage
    (codes::ability::id::TANK_DROP_PILOT, "wwww wwwwwww t w ffff wwww f", "wwwww r t t"), // CAbilityTankDropPilot
    (codes::ability::id::LOAD_PILOT, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"), // CAbilityTankLoadPilot
    (codes::ability::id::TANK_PILOT, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff w w", "wwwww r t t"), // CAbilityTankPilot
    (codes::ability::id::TARGET_CLASS, "wwww wwwwwww t w", "w w w"), // CAbilityTarget
    (codes::ability::id::TORNADO_SPIN, "wwww wwwwwww t w w w w", "t"), // CAbilityTornadoSpin
    (codes::ability::id::TANK_TURRET, "wwww wwwwwww t w", ""), // CAbilityTankTurret
    (codes::ability::id::TORNADO_WANDER, "wwww wwwwwww t w f f", "t t"), // CAbilityTornadoWander
    (codes::ability::id::WARP, "wwww wwwwwww t w w f f", ""), // CAbilityWarp
    (codes::buff::id::AOE, "wwww wwwwwww t w w f www fff f", "[r] w t ss f"), // CBuffAoe
    (codes::buff::id::AURA, "wwww wwwwwww t w w f www fff f www", "[r] w t ss f"), // CBuffAura
    (codes::buff::id::PROGRESS_BAR, "wwww wwwwwww t w w f www fff f", "[r] w t ss f"), // CBuffProgressBar
    (codes::buff::id::BREATH_OF_FROST, "wwww wwwwwww t w w f www fff f w w w", "[r] w t ss f t ss f"), // CBuffBreathOfFrost
    (codes::buff::id::SLAM, "wwww wwwwwww t w w f www fff f f", "[r] w t ss f t"), // CBuffCreepThunderClap
    (codes::buff::id::DOT, "wwww wwwwwww t w w f www fff f w w w", "[r] w t ss f t ss f"), // CBuffDoT
    (codes::buff::id::ENTANGLING_ROOTS, "wwww wwwwwww t w w f www fff f w w w f w", "[r] w t ss f t ss f"), // CBuffEntanglingRoots
    (codes::buff::id::FORCE_OF_NATURE, "wwww wwwwwww t w w f www fff f", "[r] w t ss f"), // CBuffForceOfNature
    (codes::buff::id::IMMOLATION_AOE, "wwww wwwwwww t w w f www fff f", "[r] w t ss f"), // CBuffImmolationAoe
    (codes::buff::id::IMMOLATION, "wwww wwwwwww t w w f www fff w f f f w w w w w w w f", "[r] w t ss f t r r"), // CBuffImmolation
    (codes::buff::id::METAMORPHOSIS, "wwww wwwwwww t w w f www fff f w w w", "[r] w t ss f"), // CBuffMetamorphosis
    (codes::buff::id::SHADOW_STRIKE, "wwww wwwwwww t w w f www fff f w w w f f f f f f f", "[r] w t ss f t ss f t"), // CBuffShadowStrike
    (codes::buff::id::SCOUT, "wwww wwwwwww t w w f www fff f", "[r] w t ss f"), // CBuffScout
    (codes::buff::id::VENGEANCE, "wwww wwwwwww t w w f www fff f", "[r] w t ss f"), // CBuffSpiritOfVengeance
    (codes::buff::id::SUMMONED_UNIT, "wwww wwwwwww t w w f www fff f", "[r] w t ss f"), // CBuffFigurine
    (codes::buff::id::AVATAR, "wwww wwwwwww t w w f www fff f f f w f", "[r] w t ss f"), // CBuffAvatar
    (codes::buff::id::BLIZZARD, "wwww wwwwwww t w w f www fff f w w w", "[r] w t ss f t ss f"), // CBuffBlizzard
    (codes::buff::id::BANISH, "wwww wwwwwww t w w f www fff f f w w w w", "[r] w t ss f t"), // CBuffBanish
    (codes::buff::id::BLIZZARD_AOE, "wwww wwwwwww t w w f www fff f", "[r] w t ss f"), // CBuffBlizzardAoe
    (codes::buff::id::COLD_ARROWS_BHCA, "wwww wwwwwww t w w f www fff f w w w f f f", "[r] w t ss f t ss f"), // CBuffColdArrow
    (codes::buff::id::DIVINE_SHIELD, "wwww wwwwwww t w w f www fff", "[r] w t ss f"), // CBuffDivineShield
    (codes::buff::id::FLAME_STRIKE_AOE, "wwww wwwwwww t w w f www fff f", "[r] w t ss f"), // CBuffFlameStrikeAoe
    (codes::buff::id::FLAME_STRIKE, "wwww wwwwwww t w w f www fff f w w w w f f f", "[r] w t ss f t ss f t"), // CBuffFlameStrike
    (codes::buff::id::THUNDER_CLAP, "wwww wwwwwww t w w f www fff f f", "[r] w t ss f t"), // CBuffThunderClap
    (codes::buff::id::WATER_ELEMENTAL, "wwww wwwwwww t w w f www fff f", "[r] w t ss f"), // CBuffWaterElemental
    (codes::buff::id::BINV, "wwww wwwwwww t w w f www fff", "[r] w t ss f"), // CBaseBuffInvulnerable
    (codes::buff::id::CORRUPTION, "wwww wwwwwww t w w f www fff f", "[r] w t ss f"), // CBuffCorruption
    (codes::buff::id::CLOAK_OF_FLAMES, "wwww wwwwwww t w w f www fff w f f f w w w w w w w f", "[r] w t ss f t r r"), // CBuffItemCloakOfFlames
    (codes::buff::id::ILLUSION, "wwww wwwwwww t w w f www fff f f f", "[r] w t ss f"), // CBuffItemIllusion
    (codes::buff::id::MONSTER_LURE, "wwww wwwwwww t w w f www fff f f f f", "[r] w t ss f t t"), // CBuffItemMonsterLure
    (codes::buff::id::VENOM_BIPB, "wwww wwwwwww t w w f www fff f w w w f f f", "[r] w t ss f t ss f"), // CBuffPoisonDamage
    (codes::buff::id::VENOM_BIPD, "wwww wwwwwww t w w f www fff f w w w f f f w w w", "[r] w t ss f t ss f"), // CBuffPoisonDamageStackDoT
    (codes::buff::id::POISON_DAMAGE_STACK_INFO, "wwww wwwwwww t w w f www fff w w w f f f w w", "[r] w t ss f"), // CBuffPoisonDamageStackInfo
    (codes::buff::id::VAMPIRIC_POTION, "wwww wwwwwww t w w f www fff f f", "[r] w t ss f t"), // CBuffItemVampirePotion
    (codes::buff::id::REBORN, "wwww wwwwwww t w w f www fff f", "[r] w t ss f"), // CBuffRebirth
    (codes::buff::id::REJUVENATION_BIRG, "wwww wwwwwww t w w f www fff f f", "[r] w t ss f t"), // CBuffRegeneration
    (codes::buff::id::REGENERATION, "wwww wwwwwww t w w f www fff f f", "[r] w t ss f t"), // CBuffRegenLife
    (codes::buff::id::CLARITY_POTION, "wwww wwwwwww t w w f www fff f f", "[r] w t ss f t"), // CBuffRegenMana
    (codes::buff::id::HEADHUNTER_SPIRIT, "wwww wwwwwww t w w f www fff f w w w", "[r] w t ss f"), // CBuffSpiritTroll
    (codes::buff::id::SOUL_THEFT, "wwww wwwwwww t w w f www fff", "[r] w t ss f"), // CBuffSoulTrapVision
    (codes::buff::id::ITEM_WEB, "wwww wwwwwww t w w f www fff", "[r] w t ss f"), // CBuffItemWeb
    (codes::buff::id::MANA_DRAIN, "wwww wwwwwww t w w f www fff f", "[r] w t ss f r r"), // CBuffManaDrain
    (codes::buff::id::ACID_BOMB, "wwww wwwwwww t w w f www fff f w w w f f f f f", "[r] w t ss f t ss f"), // CBuffAcidBomb
    (codes::buff::id::BLACK_ARROW, "wwww wwwwwww t w w f www fff w w w w w w f w", "[r] w t ss f"), // CBuffBlackArrow
    (codes::buff::id::BREATH_OF_FIRE, "wwww wwwwwww t w w f www fff f w w w", "[r] w t ss f t ss f"), // CBuffBreathOfFire
    (codes::buff::id::BATTLE_ROAR, "wwww wwwwwww t w w f www fff", "[r] w t ss f f f [w] f t r f r f"), // CBuffBattleRoar
    (codes::buff::id::CHEMICAL_RAGE, "wwww wwwwwww t w w f www fff f w w w", "[r] w t ss f"), // CBuffChemicalRage
    (codes::buff::id::CLUSTER_ROCKETS, "wwww wwwwwww t w w f www fff f w", "[r] w t ss f"), // CBuffClusterRockets
    (codes::buff::id::DARK_CONVERSION, "wwww wwwwwww t w w f www fff w w w w w w w", "[r] w t ss f r w t w"), // CBuffDarkConversion
    (codes::buff::id::DRUNKEN_HAZE, "wwww wwwwwww t w w f www fff f f w f", "[r] w t ss f t t"), // CBuffDrunkenHaze
    (codes::buff::id::DOOM_BNDI, "wwww wwwwwww t w w f www fff f w w w f", "[r] w t ss f"), // CBuffDoomMinion
    (codes::buff::id::DARK_MINION, "wwww wwwwwww t w w f www fff f", "[r] w t ss f"), // CBuffDarkMinion
    (codes::buff::id::DOOM_BNDO, "wwww wwwwwww t w w f www fff f w w w w w w w w w f w", "[r] w t ss f t ss f"), // CBuffDoom
    (codes::buff::id::PANDAREN_ELEMENTAL, "wwww wwwwwww t w w f www fff f w w w", "[r] w t ss f"), // CBuffElementalFury
    (codes::buff::id::BUFF_ENGINEERING_UPGRADE, "wwww wwwwwww t w w", "[r] w"), // CBuffEngineeringUpgrade
    (codes::buff::id::POCKET_FACTORY, "wwww wwwwwww t w w f www fff f w f w f f", "[r] w t ss f t"), // CBuffFactory
    (codes::buff::id::HEALING_SPRAY, "wwww wwwwwww t w w f www fff f w", "[r] w t ss f"), // CBuffHealingSpray
    (codes::buff::id::HOWL_OF_TERROR, "wwww wwwwwww t w w f www fff", "[r] w t ss f f f [w] f t r f r f"), // CBuffHowlOfTerror
    (codes::buff::id::INCINERATE, "wwww wwwwwww t w w f www fff w w w w w f f f f f f", "[r] w t ss f"), // CBuffIncinerate
    (codes::buff::id::LAVA_SPAWN, "wwww wwwwwww t w w f www fff f w w f w w w f f f w w w w f w", "[r] w t ss f t"), // CBuffLavaMonster
    (codes::buff::id::MIND_ROT, "wwww wwwwwww t w w f www fff f", "[r] w t ss f r r"), // CBuffMindRot
    (codes::buff::id::MANA_SHIELD, "wwww wwwwwww t w w f www fff f f", "[r] w t ss f"), // CBuffManaShield
    (codes::buff::id::PARASITE_BNPA, "wwww wwwwwww t w w f www fff f w w w f f f w w f w w", "[r] w t ss f t ss f"), // CBuffParasite
    (codes::buff::id::PERM_IMMOLATION, "wwww wwwwwww t w w f www fff w f f f w w w w w w w f", "[r] w t ss f t r r"), // CBuffPermImmolation
    (codes::buff::id::PARASITE_BNPM, "wwww wwwwwww t w w f www fff f", "[r] w t ss f"), // CBuffParasiteMinion
    (codes::buff::id::RAIN_OF_FIRE, "wwww wwwwwww t w w f www fff f w w w", "[r] w t ss f t ss f"), // CBuffRainOfFire
    (codes::buff::id::RAIN_OF_FIRE_AOE, "wwww wwwwwww t w w f www fff f", "[r] w t ss f"), // CBuffRainOfFireAoe
    (codes::buff::id::SANCTUARY, "wwww wwwwwww t w w f www fff f f", "[r] w t ss f r r t"), // CBuffSanctuary
    (codes::buff::id::BEAR, "wwww wwwwwww t w w f www fff f", "[r] w t ss f"), // CBuffSummonGrizzly
    (codes::buff::id::SILENCE, "wwww wwwwwww t w w f www fff f f w f", "[r] w t ss f t t"), // CBuffSilence
    (codes::buff::id::SOUL_PRESERVATION, "wwww wwwwwww t w w f www fff w w w", "[r] w t ss f"), // CBuffSoulPreservation
    (codes::buff::id::SOUL_BURN, "wwww wwwwwww t w w f www fff f f f f f w w w", "[r] w t ss f t f [w] t"), // CBuffSoulBurn
    (codes::buff::id::QUILBEAST, "wwww wwwwwww t w w f www fff f", "[r] w t ss f"), // CBuffSummonQuillbeast
    (codes::buff::id::SPELL_SHIELD, "wwww wwwwwww t w w f www fff", "[r] w t ss f"), // CBuffSpellShield
    (codes::buff::id::STAMPEDE, "wwww wwwwwww t w w f www fff", "[r] w t ss f"), // CBuffStampede
    (codes::buff::id::HAWK, "wwww wwwwwww t w w f www fff f", "[r] w t ss f"), // CBuffSummonWarEagle
    (codes::buff::id::TRANSMUTE, "wwww wwwwwww t w w f www fff w w w w f f", "[r] w t ss f"), // CBuffTransmute
    (codes::buff::id::TORNADO_BNTO, "wwww wwwwwww t w w f www fff f", "[r] w t ss f"), // CBuffTornado
    (codes::buff::id::VOLCANO_AOE, "wwww wwwwwww t w w f www fff f", "[r] w t ss f"), // CBuffVolcanoAoe
    (codes::buff::id::WATERY_MINION, "wwww wwwwwww t w w f www fff f", "[r] w t ss f"), // CBuffWateryMinion
    (codes::buff::id::EARTHQUAKE_AOE, "wwww wwwwwww t w w f www fff f", "[r] w t ss f"), // CBuffEarthquakeAoe
    (codes::buff::id::EARTHQUAKE, "wwww wwwwwww t w w f www fff f f", "[r] w t ss f t"), // CBuffEarthquake
    (codes::buff::id::HEX, "wwww wwwwwww t w w f www fff f w w w w w w f f f f f w", "[r] w t ss f t"), // CBuffHex
    (codes::buff::id::MIRROR_IMAGE, "wwww wwwwwww t w w f www fff f f f", "[r] w t ss f"), // CBuffMirrorImage
    (codes::buff::id::FERAL_SPIRIT, "wwww wwwwwww t w w f www fff f w w w", "[r] w t ss f"), // CBuffSpiritWolf
    (codes::buff::id::SHOCKWAVE, "wwww wwwwwww t w w f www fff w w w", "[r] w t ss f"), // CBuffShockwave
    (codes::buff::id::VOODOO_CASTER, "wwww wwwwwww t w w f www fff", "[r] w t ss f"), // CBuffVoodooCaster
    (codes::buff::id::BIG_BAD_VOODOO, "wwww wwwwwww t w w f www fff", "[r] w t ss f"), // CBuffVoodoo
    (codes::buff::id::WARD, "wwww wwwwwww t w w f www fff f", "[r] w t ss f"), // CBuffWard
    (codes::buff::id::WIND_WALK, "wwww wwwwwww t w w f www fff f f f f w w w w", "[r] w t ss f"), // CBuffWindWalk
    (codes::buff::id::WHIRLWIND_AOE, "wwww wwwwwww t w w f www fff f", "[r] w t ss f"), // CBuffWhirlwindAoe
    (codes::buff::id::SPEED, "wwww wwwwwww t w w f www fff f f", "[r] w t ss f t"), // CBuffSpeed
    (codes::buff::id::STACK_INFO, "wwww wwwwwww t w w f www fff", "[r] w t ss f"), // CBuffStackInfo
    (codes::buff::id::STUNNED_BSTN, "wwww wwwwwww t w w f www fff w w w w w", "[r] w t ss f"), // CBuffStun
    (codes::buff::id::TARGET, "wwww wwwwwww t w w f www fff w w w", "[r] w t ss f"), // CBuffTarget
    (codes::buff::id::TIMED_LIFE, "wwww wwwwwww t w w f www fff f", "[r] w t ss f"), // CBuffTimedLife
    (codes::buff::id::ANIMATE_DEAD, "wwww wwwwwww t w w f www fff f", "[r] w t ss f"), // CBuffAnimateDead
    (codes::buff::id::CARRION_BEETLES, "wwww wwwwwww t w w f www fff f w w w", "[r] w t ss f"), // CBuffCarrionScarab
    (codes::buff::id::CARRION_SWARM, "wwww wwwwwww t w w f www fff w w w", "[r] w t ss f"), // CBuffCarrionSwarm
    (codes::buff::id::DEATH_AND_DECAY, "wwww wwwwwww t w w f www fff f w w w w f", "[r] w t ss f t"), // CBuffDeathAndDecayAoe
    (codes::buff::id::FROST_ARMOR, "wwww wwwwwww t w w f www fff f f w", "[r] w t ss f t"), // CBuffFrostArmor
    (codes::buff::id::IMPALE, "wwww wwwwwww t w w f www fff w w w f f f f f", "[r] w t ss f r r t t"), // CBuffImpale
    (codes::buff::id::SLEEP, "wwww wwwwwww t w w f www fff w w w", "[r] w t ss f"), // CBuffSleep
    (codes::buff::id::SLEEP_PAUSE, "wwww wwwwwww t w w f www fff w w w", "[r] w t ss f"), // CBuffSleepPause
    (codes::buff::id::SLEEP_STUN, "wwww wwwwwww t w w f www fff w w w w w", "[r] w t ss f"), // CBuffSleepStun
    (codes::buff::id::AURA_OF_BLIGHT, "wwww wwwwwww t w w f www fff f www", "[r] w t ss f r t"), // CBuffAuraBlightRegen
    (codes::buff::id::ANTI_MAGIC_SHELL_BAM2, "wwww wwwwwww t w w f www fff f", "[r] w t ss f"), // CBuffAntiMagicShellTwo
    (codes::buff::id::ANTI_MAGIC_SHELL_BAMS, "wwww wwwwwww t w w f www fff f", "[r] w t ss f"), // CBuffAntiMagicShell
    (codes::buff::id::DISEASE, "wwww wwwwwww t w w f www fff f w w w", "[r] w t ss f t ss f"), // CBuffAuraPlague
    (codes::buff::id::MANA_REGENERATION_AURA, "wwww wwwwwww t w w f www fff f www", "[r] w t ss f r t"), // CBuffAuraRegenMana
    (codes::buff::id::TORNADO_BASL, "wwww wwwwwww t w w f www fff f w w w f f", "[r] w t ss f t"), // CBuffAuraSlow
    (codes::buff::id::BARKSKIN, "wwww wwwwwww t w w f www fff f", "[r] w t ss f t"), // CBuffBarkskin
    (codes::buff::id::BLOODLUST, "wwww wwwwwww t w w f www fff f f w w w f f", "[r] w t ss f t t"), // CBuffBloodlust
    (codes::buff::id::BURNING_OIL, "wwww wwwwwww t w w f www fff f w w w w f f f", "[r] w t ss f t ss f t"), // CBuffBallsOfFire
    (codes::buff::id::BERSERK, "wwww wwwwwww t w w f www fff f f f", "[r] w t ss f t"), // CBuffBerserkerRage
    (codes::buff::id::DIZZINESS, "wwww wwwwwww t w w f www fff w w w f f f f f f", "[r] w t ss f t t t"), // CBuffCargoHoldDeath
    (codes::buff::id::CLOUD, "wwww wwwwwww t w w f www fff f f w f w", "[r] w t ss f t t"), // CBuffCloudOfFog
    (codes::buff::id::CONTROL_MAGIC, "wwww wwwwwww t w w f www fff f", "[r] w t ss f"), // CBuffControlMagic
    (codes::buff::id::CORROSIVE_BREATH, "wwww wwwwwww t w w f www fff f w w w", "[r] w t ss f t ss f"), // CBuffCorrosiveBreath
    (codes::buff::id::CRIPPLE, "wwww wwwwwww t w w f www fff f f", "[r] w t ss f t f [w]"), // CBuffCripple
    (codes::buff::id::COLD_ARROWS_BCSD, "wwww wwwwwww t w w f www fff f w w w f f f w w w", "[r] w t ss f t ss f"), // CBuffColdArrowStackDoT
    (codes::buff::id::COLD_ARROW_STACK_INFO, "wwww wwwwwww t w w f www fff w w w f f f w w", "[r] w t ss f"), // CBuffColdArrowStackInfo
    (codes::buff::id::CYCLONE_BCY2, "wwww wwwwwww t w w f www fff w w w f f f", "[r] w t ss f r r t"), // CBuffCycloneTwo
    (codes::buff::id::CYCLONE_BCYC, "wwww wwwwwww t w w f www fff w w w f f f", "[r] w t ss f r r t"), // CBuffCyclone
    (codes::buff::id::DRAIN_CASTER, "wwww wwwwwww t w w f www fff", "[r] w t ss f"), // CBuffDrainCaster
    (codes::buff::id::DRAIN_CASTER_LIFE, "wwww wwwwwww t w w f www fff", "[r] w t ss f"), // CBuffDrainCasterLife
    (codes::buff::id::DRAIN_CASTER_MANA, "wwww wwwwwww t w w f www fff", "[r] w t ss f"), // CBuffDrainCasterMana
    (codes::buff::id::SCROLL_OF_PROTECTION, "wwww wwwwwww t w w f www fff f", "[r] w t ss f t"), // CBuffDefense
    (codes::buff::id::DUST_OF_APPEARANCE, "wwww wwwwwww t w w f www fff w w", "[r] w t ss f"), // CBuffDetected
    (codes::buff::id::DIGESTING, "wwww wwwwwww t w w f www fff f w w w w w w", "[r] w t ss f t ss f"), // CBuffDigesting
    (codes::buff::id::DRAIN_TARGET, "wwww wwwwwww t w w f www fff", "[r] w t ss f"), // CBuffDrainTarget
    (codes::buff::id::DRAIN_TARGET_LIFE, "wwww wwwwwww t w w f www fff", "[r] w t ss f"), // CBuffDrainTargetLife
    (codes::buff::id::DRAIN_TARGET_MANA, "wwww wwwwwww t w w f www fff", "[r] w t ss f"), // CBuffDrainTargetMana
    (codes::buff::id::DEVOUR, "wwww wwwwwww t w w f www fff w w", "[r] w t ss f"), // CBuffDevourVision
    (codes::buff::id::EAT_TREE, "wwww wwwwwww t w w f www fff f f", "[r] w t ss f t"), // CBuffEatTree
    (codes::buff::id::ENSNARE_BENA, "wwww wwwwwww t w w f www fff w w f f", "[r] w t ss f t"), // CBuffEnsnareAir
    (codes::buff::id::ENSNARE_BENG, "wwww wwwwwww t w w f www fff w w f f", "[r] w t ss f t"), // CBuffEnsnareGround
    (codes::buff::id::ENSNARE_BENS, "wwww wwwwwww t w w f www fff w w f f", "[r] w t ss f t"), // CBuffEnsnare
    (codes::buff::id::ETHEREAL, "wwww wwwwwww t w w f www fff f f w w w w", "[r] w t ss f t"), // CBuffEthereal
    (codes::buff::id::SENTRY_WARD, "wwww wwwwwww t w w f www fff f", "[r] w t ss f"), // CBuffEvilEye
    (codes::buff::id::FAERIE_FIRE, "wwww wwwwwww t w w f www fff w w f", "[r] w t ss f t"), // CBuffFaerieFire
    (codes::buff::id::FREEZE, "wwww wwwwwww t w w f www fff w w w w w", "[r] w t ss f"), // CBuffFreeze
    (codes::buff::id::FREEZING_BREATH, "wwww wwwwwww t w w f www fff w w w w w", "[r] w t ss f"), // CBuffFreezingBreath
    (codes::buff::id::FRENZY, "wwww wwwwwww t w w f www fff f f w w w f f", "[r] w t ss f t t"), // CBuffFrenzy
    (codes::buff::id::GRAB_TREE, "wwww wwwwwww t w w f www fff w w w w", "[r] w t ss f w"), // CBuffGrabTree
    (codes::buff::id::HEAL, "wwww wwwwwww t w w f www fff w w w", "[r] w t ss f"), // CBuffHeal
    (codes::buff::id::HEALING_WARD, "wwww wwwwwww t w w f www fff f", "[r] w t ss f"), // CBuffHealingWard
    (codes::buff::id::INNER_FIRE, "wwww wwwwwww t w w f www fff", "[r] w t ss f f f [w] f t r f r f"), // CBuffInnerFire
    (codes::buff::id::INVISIBILITY, "wwww wwwwwww t w w f www fff", "[r] w t ss f"), // CBuffInvisibility
    (codes::buff::id::LIFE_DOT, "wwww wwwwwww t w w f www fff f w w w", "[r] w t ss f t ss f"), // CBuffLifeDoT
    (codes::buff::id::LIQUID_FIRE, "wwww wwwwwww t w w f www fff f w w w f f f", "[r] w t ss f t ss f"), // CBuffLiquidFire
    (codes::buff::id::LIGHTNING_SHIELD_AOE, "wwww wwwwwww t w w f www fff f", "[r] w t ss f"), // CBuffLightningShieldAoe
    (codes::buff::id::LIGHTNING_SHIELD, "wwww wwwwwww t w w f www fff w f f f w w w w", "[r] w t ss f t"), // CBuffLightningShield
    (codes::buff::id::MECHANICAL_CRITTER, "wwww wwwwwww t w w f www fff f", "[r] w t ss f"), // CBuffMechanicalCritter
    (codes::buff::id::MANA_FLARE_BMFA, "wwww wwwwwww t w w f www fff f w w w w w f f", "[r] w t ss f"), // CBuffManaFlareAoe
    (codes::buff::id::MANA_FLARE_BMFL, "wwww wwwwwww t w w f www fff w w w f f f f f f w w", "[r] w t ss f r t t"), // CBuffManaFlare
    (codes::buff::id::MAGIC_LEASH_CASTER, "wwww wwwwwww t w w f www fff", "[r] w t ss f"), // CBuffMagicLeashCaster
    (codes::buff::id::AERIAL_SHACKLES, "wwww wwwwwww t w w f www fff f w w w", "[r] w t ss f t ss f"), // CBuffMagicLeashTarget
    (codes::buff::id::PLAGUE_WARD, "wwww wwwwwww t w w f www fff f", "[r] w t ss f"), // CBuffPlagueWard
    (codes::buff::id::POLYMORPH, "wwww wwwwwww t w w f www fff f w w w w w w f f f f f w", "[r] w t ss f t"), // CBuffPolymorph
    (codes::buff::id::POSSESSION_BPOC, "wwww wwwwwww t w w f www fff", "[r] w t ss f"), // CBuffPossessionCaster
    (codes::buff::id::POSSESSION_BPOS, "wwww wwwwwww t w w f www fff w w w", "[r] w t ss f"), // CBuffPossession
    (codes::buff::id::PURGE, "wwww wwwwwww t w w f www fff w w w f f f f f f", "[r] w t ss f t t t"), // CBuffPurge
    (codes::buff::id::PULSE_REVEAL, "wwww wwwwwww t w w f www fff", "[r] w t ss f t"), // CBuffPulseReveal
    (codes::buff::id::POISON_BPSD, "wwww wwwwwww t w w f www fff f w w w f f f w w w", "[r] w t ss f t ss f"), // CBuffPoisonAttackStackDoT
    (codes::buff::id::PHASE_SHIFT, "wwww wwwwwww t w w f www fff w w w w w w", "[r] w t ss f"), // CBuffPhaseShift
    (codes::buff::id::POISON_ATTACK_STACK_INFO, "wwww wwwwwww t w w f www fff w w w f f f w w", "[r] w t ss f"), // CBuffPoisonAttackStackInfo
    (codes::buff::id::PHOENIX_FIRE, "wwww wwwwwww t w w f www fff f w w w", "[r] w t ss f t ss f"), // CBuffPhoenixFire
    (codes::buff::id::SKELETAL_MINION, "wwww wwwwwww t w w f www fff f", "[r] w t ss f"), // CBuffRaiseDead
    (codes::buff::id::REJUVENATION_BREJ, "wwww wwwwwww t w w f www fff f f", "[r] w t ss f t"), // CBuffRejuvination
    (codes::buff::id::RANGE_LISTENER, "wwww wwwwwww t w w f www fff", "[r] w t ss f r t"), // CBuffRangeListener
    (codes::buff::id::ROAR, "wwww wwwwwww t w w f www fff", "[r] w t ss f f f [w] f t r f r f"), // CBuffRoar
    (codes::buff::id::REPLENISH, "wwww wwwwwww t w w f www fff w w w", "[r] w t ss f"), // CBuffReplenish
    (codes::buff::id::ESSENCE_OF_BLIGHT, "wwww wwwwwww t w w f www fff w w w", "[r] w t ss f"), // CBuffReplenishLife
    (codes::buff::id::SPIRIT_TOUCH, "wwww wwwwwww t w w f www fff w w w", "[r] w t ss f"), // CBuffReplenishMana
    (codes::buff::id::SHARED_VISION, "wwww wwwwwww t w w f www fff w w", "[r] w t ss f"), // CBuffSharedVision
    (codes::buff::id::WAND_OF_SHADOWSIGHT, "wwww wwwwwww t w w f www fff w w", "[r] w t ss f"), // CBuffShadowSight
    (codes::buff::id::SLOW, "wwww wwwwwww t w w f www fff f f", "[r] w t ss f t"), // CBuffSlow
    (codes::buff::id::SPIDERLING, "wwww wwwwwww t w w f www fff w w w", "[r] w t ss f"), // CBuffSpiderAttack
    (codes::buff::id::SPEED_BONUS, "wwww wwwwwww t w w f www fff f f", "[r] w t ss f t"), // CBuffSpeedBonus
    (codes::buff::id::SPIRIT_LINK, "wwww wwwwwww t w w f www fff f", "[r] w t ss f"), // CBuffSpiritLink
    (codes::buff::id::SLOW_POISON_BSPO, "wwww wwwwwww t w w f www fff f w w w f f f", "[r] w t ss f t ss f"), // CBuffSlowPoison
    (codes::buff::id::SLOW_POISON_BSSD, "wwww wwwwwww t w w f www fff f w w w f f f w w w", "[r] w t ss f t ss f"), // CBuffSlowPoisonStackDoT
    (codes::buff::id::SLOW_POISON_STACK_INFO, "wwww wwwwwww t w w f www fff w w w f f f w w", "[r] w t ss f"), // CBuffSlowPoisonStackInfo
    (codes::buff::id::STASIS_TRAP, "wwww wwwwwww t w w f www fff f f f f f f", "[r] w t ss f r t"), // CBuffStasisTrapTrigger
    (codes::buff::id::TAUNT, "wwww wwwwwww t w w f www fff", "[r] w t ss f"), // CBuffTaunt
    (codes::buff::id::TORNADO_DAMAGE_AOE, "wwww wwwwwww t w w f www fff f", "[r] w t ss f"), // CBuffTornadoDamageAoe
    (codes::buff::id::BUFF_TELEPORT_REVEAL, "wwww wwwwwww t w", ""), // CBuffTeleportReveal
    (codes::buff::id::TORNADO_SPIN_AOE, "wwww wwwwwww t w w f www fff f", "[r] w t ss f"), // CBuffTornadoSpinAoe
    (codes::buff::id::TORNADO_SPIN, "wwww wwwwwww t w w f www fff w w w f f f", "[r] w t ss f r r t"), // CBuffTornadoSpin
    (codes::buff::id::UNHOLY_FRENZY, "wwww wwwwwww t w w f www fff f w w w f w", "[r] w t ss f t ss f t"), // CBuffUnholyFrenzy
    (codes::buff::id::UNSUMMON, "wwww wwwwwww t w w f www fff f w w w f f f", "[r] w t ss f t ss f"), // CBuffUnsummon
    (codes::buff::id::SPIRIT_OF_VENGEANCE, "wwww wwwwwww t w w f www fff f w w w", "[r] w t ss f"), // CBuffVengeance
    (codes::buff::id::INVULNERABLE, "wwww wwwwwww t w w f www fff", "[r] w t ss f"), // CBuffInvulnerable
    (codes::buff::id::WEB_BWEA, "wwww wwwwwww t w w f www fff w w f f", "[r] w t ss f t"), // CBuffWebAir
    (codes::buff::id::WEB_BWEB, "wwww wwwwwww t w w f www fff w w f f", "[r] w t ss f t"), // CBuffWeb
    // END genlayouts.py
    // Abilities whose classes' own fields and saves are those already read (their
    // classes by their vf7's code, their own vf18 and save vf14 from the vtables):
    // each family's own and tail.
    // (spell2: own `FUN_6f42f3e0`, save `FUN_6f42e2b0`.)
    (codes::ability::id::BREATH_OF_FROST_CREEP, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"),
    (codes::ability::id::KEEPER_ENTANGLING_ROOTS, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"),
    (codes::ability::id::WARDEN_FAN_OF_KNIVES, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"),
    (codes::ability::id::KEEPER_FORCE_OF_NATURE, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"),
    (codes::ability::id::WARDEN_SHADOW_STRIKE, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"),
    (codes::ability::id::BLOOD_MAGE_BANISH, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"),
    (codes::ability::id::MOUNTAIN_KING_THUNDER_BOLT, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"),
    (codes::ability::id::MOUNTAIN_KING_THUNDER_CLAP, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"),
    (codes::ability::id::ITEM_DISPEL_CHAIN, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"),
    (codes::ability::id::ITEM_DETECT_AOE, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"),
    (codes::ability::id::ALCHEMIST_ACID_BOMB, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"),
    (codes::ability::id::BREWMASTER_BREATH_OF_FIRE, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"),
    (codes::ability::id::DARK_RANGER_CHARM, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"),
    (codes::ability::id::PIT_LORD_DOOM, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"),
    (codes::ability::id::FIRE_BOLT, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"),
    (codes::ability::id::SEA_WITCH_FORKED_LIGHTNING, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"),
    (codes::ability::id::FIRELORD_SOUL_BURN, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"),
    (codes::ability::id::TINKERER_SUMMON_FACTORY_LEVEL_0, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"),
    (codes::ability::id::ALCHEMIST_TRANSMUTE, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"),
    (codes::ability::id::FARSEER_FAR_SIGHT, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"),
    (codes::ability::id::SHADOW_HUNTER_SERPENT_WARD_AOWD, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"),
    (codes::ability::id::CHIEFTAIN_WAR_STOMP, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"),
    (codes::ability::id::DREADLORD_CARRION_SWARM, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"),
    (codes::ability::id::LICH_FROST_NOVA, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"),
    (codes::ability::id::ABSORB_MANA, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"),
    (codes::ability::id::ANTI_MAGIC_SHIELD, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"),
    (codes::ability::id::CARGO_HOLD_DEATH, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"),
    (codes::ability::id::CONTROL_MAGIC, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"),
    (codes::ability::id::COUPLE_INSTANT_ARCHER_ACOI, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"),
    (codes::ability::id::DECOUPLE, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"),
    (codes::ability::id::DEVOUR_MAGIC, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"),
    (codes::ability::id::EAT_TREE, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"),
    (codes::ability::id::SENTRY_WARD, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"),
    (codes::ability::id::GRAB_TREE, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"),
    (codes::ability::id::INVISIBILITY, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"),
    (codes::ability::id::LIGHTNING_SHIELD, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"),
    (codes::ability::id::POSSESSION, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"),
    (codes::ability::id::PURGE, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"),
    (codes::ability::id::STASIS_TRAP, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"),
    (codes::ability::id::TAUNT, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"),
    // (half2: own `FUN_6f42f4b0`, save `FUN_6f42e2b0`.)
    (codes::ability::id::REINCARNATION_CREEP, "wwww wwwwwww t w ffff wwww f", "wwwww r t t"),
    (codes::ability::id::WARDEN_SPIRIT_OF_VENGEANCE, "wwww wwwwwww t w ffff wwww f", "wwwww r t t"),
    (codes::ability::id::BLOOD_MAGE_PHOENIX, "wwww wwwwwww t w ffff wwww f", "wwwww r t t"),
    (codes::ability::id::ARCH_MAGE_WATER_ELEMENTAL, "wwww wwwwwww t w ffff wwww f", "wwwww r t t"),
    (codes::ability::id::PIT_LORD_HOWL_OF_TERROR, "wwww wwwwwww t w ffff wwww f", "wwwww r t t"),
    (codes::ability::id::FIRELORD_SUMMON_LAVA_SPAWN, "wwww wwwwwww t w ffff wwww f", "wwwww r t t"),
    (codes::ability::id::BEAST_MASTER_SUMMON_BEAR, "wwww wwwwwww t w ffff wwww f", "wwwww r t t"),
    (codes::ability::id::BEAST_MASTER_SUMMON_QUILBEAST, "wwww wwwwwww t w ffff wwww f", "wwwww r t t"),
    (codes::ability::id::BEAST_MASTER_SUMMON_HAWK, "wwww wwwwwww t w ffff wwww f", "wwwww r t t"),
    (codes::ability::id::WATERY_MINION, "wwww wwwwwww t w ffff wwww f", "wwwww r t t"),
    (codes::ability::id::CHIEFTAIN_REINCARNATION, "wwww wwwwwww t w ffff wwww f", "wwwww r t t"),
    (codes::ability::id::FARSEER_SPIRIT_WOLF, "wwww wwwwwww t w ffff wwww f", "wwwww r t t"),
    (codes::ability::id::BLADE_MASTER_WIND_WALK, "wwww wwwwwww t w ffff wwww f", "wwwww r t t"),
    (codes::ability::id::DROP_INSTANT, "wwww wwwwwww t w ffff wwww f", "wwwww r t t"),
    (codes::ability::id::REVENGE, "wwww wwwwwww t w ffff wwww f", "wwwww r t t"),
    (codes::ability::id::ROAR, "wwww wwwwwww t w ffff wwww f", "wwwww r t t"),
    (codes::ability::id::STAND_DOWN, "wwww wwwwwww t w ffff wwww f", "wwwww r t t"),
    // (base5: own `FUN_6f42e830`, save `FUN_6f42d5f0`.)
    (codes::ability::id::DEMON_HUNTER_EVASION, "wwww wwwwwww t w", "wwwww"),
    (codes::ability::id::PIT_LORD_CLEAVING_ATTACK, "wwww wwwwwww t w", "wwwww"),
    (codes::ability::id::BALLS_OF_FIRE, "wwww wwwwwww t w", "wwwww"),
    (codes::ability::id::CORROSIVE_BREATH, "wwww wwwwwww t w", "wwwww"),
    (codes::ability::id::FEEDBACK, "wwww wwwwwww t w", "wwwww"),
    (codes::ability::id::FLAK_CANNON, "wwww wwwwwww t w", "wwwww"),
    (codes::ability::id::FROST_ATTACK, "wwww wwwwwww t w", "wwwww"),
    (codes::ability::id::FREEZING_BREATH, "wwww wwwwwww t w", "wwwww"),
    (codes::ability::id::FRAG_SHARDS, "wwww wwwwwww t w", "wwwww"),
    (codes::ability::id::GYROCOPTER_BOMBS, "wwww wwwwwww t w", "wwwww"),
    (codes::ability::id::SHADOW_MELD_AKAMA, "wwww wwwwwww t w", "wwwww"),
    (codes::ability::id::IMPALING_BOLT, "wwww wwwwwww t w", "wwwww"),
    (codes::ability::id::LIQUID_FIRE, "wwww wwwwwww t w", "wwwww"),
    (codes::ability::id::MOON_GLAIVE, "wwww wwwwwww t w", "wwwww"),
    (codes::ability::id::ROCKET_ATTACK, "wwww wwwwwww t w", "wwwww"),
    (codes::ability::id::RESISTANT_SKIN, "wwww wwwwwww t w", "wwwww"),
    (codes::ability::id::STORM_HAMMERS, "wwww wwwwwww t w", "wwwww"),
    // (spell3: own `FUN_6f42f3e0`, save `FUN_6f42d2a0`.)
    (codes::ability::id::COLD_ARROWS, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t t"),
    (codes::ability::id::PRIESTESS_SEARING_ARROWS, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t t"),
    (codes::ability::id::DARK_RANGER_BLACK_ARROW, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t t"),
    (codes::ability::id::FIRELORD_INCINERATE_ANIA, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t t"),
    (codes::ability::id::PARASITE, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t t"),
    (codes::ability::id::LICH_FROST_ARMOR_AUTOCAST, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t t"),
    (codes::ability::id::ORB_OF_ANNIHILATION, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t t"),
    (codes::ability::id::FRENZY, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t t"),
    (codes::ability::id::HEAL, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t t"),
    (codes::ability::id::SLOW, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t t"),
    (codes::ability::id::SPELL_STEAL, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t t"),
    // (base0: own `FUN_6f42e830`, save `FUN_6f03eda0`.)
    (codes::ability::id::AWAKEN, "wwww wwwwwww t w", ""),
    (codes::ability::id::DEATH_DAMAGE_SAPPER, "wwww wwwwwww t w", ""),
    (codes::ability::id::GHOST_VISIBLE, "wwww wwwwwww t w", ""),
    (codes::ability::id::FROST_BREATH, "wwww wwwwwww t w", ""),
    (codes::ability::id::SPAWN_SPIDERLING, "wwww wwwwwww t w", ""),
    // Death Coil, Cripple, Unholy Frenzy (the spell's fields, their save `FUN_6f42e2b0`).
    (codes::ability::id::DEATH_KNIGHT_DEATH_COIL, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"),
    (codes::ability::id::CRIPPLE, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"),
    (codes::ability::id::UNHOLY_FRENZY, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"),
    // CAbilityAllied (`FUN_6f42dd20`): a counted list of references (`FUN_6f3e2ce0`), a
    // reference, a counted list of two words and a word, a reference, and a timer.
    (codes::ability::id::ALLIED_BUILDING, "wwww wwwwwww t w", "[r] r [www] r t"),
    // CAbilityMakeItem (own `vf18_6f42eec0`: two reals and a word; its save
    // `FUN_6f42d3c0`, as Sell Items').
    (codes::ability::id::MAKE_ITEMS, "wwww wwwwwww t w ffw", "r r 12(w fff t ss f t ss f w) wwwww"),
    // CAbilitySpiderAttack (`FUN_6f42e830`, its save `FUN_6f03eda0`): the ability's
    // own fields and lists alone.
    (codes::ability::id::SPIDER_ATTACK, "wwww wwwwwww t w", ""),
    // CAbilityBurrow, a morph (`FUN_6f4eb500`: the spell's first fields, two reals,
    // the two unit types, a real and two words by build; its save `FUN_6f4e9d90`, the
    // morph's `FUN_6f4ea410`: the buff base's tail, a reference, five timers, and
    // `FUN_6f224e20`'s structure; then its own timer, `+0x1a4`: read, a Crypt Fiend's
    // every second, event 0xd01bf).
    (codes::ability::id::BURROW, "wwww wwwwwww t w ffff wwww f ff ww f w w", "wwwww r t t t t t wwwww f w fff w f [ww] f f f [w] f s t"),
    (codes::ability::id::DREADLORD_SLEEP, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"),
    (codes::ability::id::REJUVINATION, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"),
    // CAbilityAnimateDead (`FUN_6f4e9990`): the buff base's tail, a reference, three
    // timers; CAbilityDevour (`FUN_6f5aadb0`, own `FUN_6f5aba20`: a word more) two and
    // from build 6029 a third; CAbilityManaBurn (`FUN_6f648560`, own `FUN_6f649130`: two
    // words and a real more) three; CAbilityRaiseDead (`FUN_6f5221e0`, own
    // `FUN_6f522d50`: from build 6011 two words more) `FUN_6f42d2a0`'s and a timer.
    (codes::ability::id::ANIMATE_DEAD_CREEP, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t t"),
    (codes::ability::id::DEVOUR_DRAGON_CREEP, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff w", "wwwww r t t t"),
    (codes::ability::id::DEMON_HUNTER_MANA_BURN, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff ww f", "wwwww r t t t"),
    (codes::ability::id::RAISE_DEAD, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff ww", "wwwww r t t t t"),
    // CAbilityShadowMeld (`FUN_6f522420`, own `FUN_6f523050`: three reals more): the
    // buff base's tail, a timer, and from build 6018 a second.
    (codes::ability::id::SHADOW_MELD, "wwww wwwwwww t w fff", "wwwww t t"),
    // CAbilityBurrowDetector (`FUN_6f42d860`, own `FUN_6f42ebd0`: a real and a word
    // more): the buff base's tail and two references.
    (codes::ability::id::BURROW_DETECTION_FLYERS, "wwww wwwwwww t w f w", "wwwww r r"),
    // CAbilityCriticalStrike (the buff base's tail; own `FUN_6f4eb470`: a word more).
    (codes::ability::id::BLADE_MASTER_CRITICAL_STRIKE, "wwww wwwwwww t w w", "wwwww"),
    // CAbilityPurchaseItem (`FUN_6f42df00`): a reference.
    (codes::ability::id::PURCHASE_ITEM, "wwww wwwwwww t w", "r"),
    // An item's Town Portal (read: the melee start's scroll on Forestwalk's computer's
    // Dreadlord): 5.0, three words, its item, three words, a reference; ten words,
    // 0xd0142, four words, its listener on its holder's mana, two timers, and three
    // references.  Provisional: the fields' meanings.
    (codes::ability::id::ITEM_TOWN_PORTAL, "wwww wwwwwww t w f www r www r", "wwwwwwwwww w wwww r t t r r r"),
    // CAbilityDevourCargo (`FUN_6f5aabb0`): a real, three words, two counted lists of
    // two words and a real, two of words, two timers, and two counted lists of two reals.
    (codes::ability::id::CARGO_HOLD_DEVOUR, "wwww wwwwwww t w", "f www [wwf] [wwf] [w] [w] t t [ff] [ff]"),
    // CAbilityGraveyard (`CAbilityGraveyard::vf14`, 0x6f5f5550; own CAbility's): its
    // ten corpse places (a reference and a word each, from the last: `FUN_6f4a7060`
    // makes ten), two FloatMinis' values, a word, its ten units (a counted list, the
    // constructor's ten), and a timer.
    (codes::ability::id::GRAVEYARD, "wwww wwwwwww t w", "10(r w) f f w [r] t"),
    // CAbilityHero (`CAbilityHero::vf14`, 0x6f52a090; own CAbility's): three words,
    // six FloatMinis' values, a word, three values more, three words, three counted
    // lists of words, two references, and a word.
    (codes::ability::id::HERO, "wwww wwwwwww t w", "www ffffff w fff www [w] [w] [w] r r w"),
    // CAbilityDeathSpawnSpider (CAgent's `FUN_6f03eda0` alone).
    (codes::ability::id::SPAWN_SPIDER, "wwww wwwwwww t w", ""),
    // CAbilityNeutralInteract (as CAbilityNeutralDetectAoe's).
    (codes::ability::id::SELECT_USER, "wwww wwwwwww t w w", "wwwww"),
    (codes::ability::id::DEATH_KNIGHT_UNHOLY_AURA, "wwww wwwwwww t w w ff", "[r] w r t w ww ww"),
    (codes::ability::id::SELL_ITEMS, "wwww wwwwwww t w ff", "r r 12(w fff t ss f t ss f w) wwwww"),
    // The dynamic shops' (CAbilitySellItemDynamic and CAbilitySellUnitDynamic vf14): the
    // shop's, then a word and twelve more (+0x47c and on; read, Echo Isles' at 60
    // seconds).
    (codes::ability::id::SELL_ITEM, "wwww wwwwwww t w ff", "r r 12(w fff t ss f t ss f w) wwwww w 12(w)"),
    (codes::ability::id::SELL_UNIT, "wwww wwwwwww t w ff", "r r 12(w fff t ss f t ss f w) wwwww w 12(w)"),
    // `FUN_6f42e0e0` on that: four timers and a reference (own `FUN_6f42f1b0`: three
    // words, three reals, three words, a real, three words).
    (codes::ability::id::REPAIR_ORC, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff www fff www f www", "wwwww r t t t t t t t r"),
    (codes::ability::id::RESTORATION, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff www fff www f www", "wwwww r t t t t t t t r"),
    // The aura base (`FUN_6f532730`: a counted list of references and a word):
    // CAbilityVulnerable's own a word (`FUN_6f42f5a0`); auras (`FUN_6f42d0c0`, own
    // `FUN_6f42e920`: a word and two reals) a reference, a timer, and from build 6047 a
    // word and two pairs.
    (codes::ability::id::INVULNERABLE, "wwww wwwwwww t w w", "[r] w"),
    (codes::ability::id::PRIESTESS_TRUESHOT_AURA, "wwww wwwwwww t w w ff", "[r] w r t w ww ww"),
    (codes::ability::id::PALADIN_DEVOTION_AURA, "wwww wwwwwww t w w ff", "[r] w r t w ww ww"),
    (codes::ability::id::MANA_REGENERATION_AURA, "wwww wwwwwww t w w ff", "[r] w r t w ww ww"),
    // The other auras whose classes keep the base's save (no vf 0x38 nor 0x48 of
    // their own): Brilliance, Command, Endurance, War Drums (CAbilityAuraKotoBeast),
    // Thorns, Vampiric, a healing ward's (read: Hills of Glory's Command Aura's 4276
    // bytes, as an Unholy Aura's).
    (codes::ability::id::ARCH_MAGE_BRILLIANCE_AURA, "wwww wwwwwww t w w ff", "[r] w r t w ww ww"),
    (codes::ability::id::COMMAND_AURA, "wwww wwwwwww t w w ff", "[r] w r t w ww ww"),
    (codes::ability::id::CHIEFTAIN_ENDURANCE_AURA, "wwww wwwwwww t w w ff", "[r] w r t w ww ww"),
    (codes::ability::id::AURA_WAR_DRUMS, "wwww wwwwwww t w w ff", "[r] w r t w ww ww"),
    (codes::ability::id::KEEPER_THORNS_AURA, "wwww wwwwwww t w w ff", "[r] w r t w ww ww"),
    (codes::ability::id::DREADLORD_VAMPIRIC_AURA, "wwww wwwwwww t w w ff", "[r] w r t w ww ww"),
    (codes::ability::id::AURA_REGENERATION_WARD, "wwww wwwwwww t w w ff", "[r] w r t w ww ww"),
    // Effects on the widget base (`FUN_6f42e650`, own `FUN_6f42f720`: four words): the
    // sprite and a word, a timer, a word, two counted lists of words; `FUN_6f42e6d0`
    // three reals more.
    (codes::class::EFFECT_IMAGE, "wwww", "p w t w [w] [w]"),
    (codes::class::SPECIAL_EFFECT, "wwww", "p w t w [w] [w] fff"),
    // CAbilityUpgrade-like (`FUN_6f42d320`, own `FUN_6f42e970`: two reals more): two
    // references.
    (codes::ability::id::UPGRADE_STRUCTURE, "wwww wwwwwww t w ff", "r r"),
    // Records of the agent base alone (vf 0x38 `FUN_6f0562b0`, no lists of events
    // 0xd0000 to 0xd03ff): an acolyte's ring slot (own `FUN_6f4eaef0`: three reals and
    // six words); a rally point (`FUN_6f42e810`, own `FUN_6f42f7f0`) a reference.
    (codes::class::MINE_SLOT, "!fff wwwwww", ""),
    // CSplashData (own `FUN_6f49c330`: a word and five reals).
    (codes::class::SPLASH, "!w fffff", ""),
    (codes::class::RALLY_POINT, "!w ff wwww", "ww"),
    // CEffectBlight (`FUN_6f490810`, own `FUN_6f490850`): a timer.
    (codes::class::BLIGHT_GROWTH, "ww fff ww", "t"),
    // Buffs (own on `FUN_6f532960`: the ability's, a word, a real, three words, three
    // reals; then a real and three words, and for a devotion aura from build 6047 a
    // real): the aura base's, an extended timer (two shorts and a real more), for
    // trueshot a counted list of words, and a timer.
    (codes::buff::id::TRUESHOT_AURA, "wwww wwwwwww t w w f www fff f www", "[r] w t ss f [w] t"),
    // Command's and War Drums' (CBuffAuraCommand, CBuffAuraKotoBeast) save as
    // Trueshot's (`FUN_6f4ea860`); Thorns' and Vampiric's as the aura base's
    // (`FUN_6f42e4c0`: its extended timer alone; read, Forestwalk's Dreadlord's at 900
    // seconds); Endurance's (CBuffAuraEndurance, own `FUN_6f4eba40`: two reals more,
    // save `FUN_6f4ea910`: a timer after its extended one).
    (codes::buff::id::AURA_COMMAND, "wwww wwwwwww t w w f www fff f www", "[r] w t ss f [w] t"),
    (codes::buff::id::ENDURANCE_AURA, "wwww wwwwwww t w w f www fff f www ff", "[r] w t ss f t"),
    (codes::buff::id::WAR_DRUMS, "wwww wwwwwww t w w f www fff f www", "[r] w t ss f [w] t"),
    (codes::buff::id::THORNS_AURA, "wwww wwwwwww t w w f www fff f www", "[r] w t ss f"),
    (codes::buff::id::VAMPIRIC_AURA, "wwww wwwwwww t w w f www fff f www", "[r] w t ss f"),
    (codes::buff::id::DEVOTION_AURA, "wwww wwwwwww t w w f www fff f www f", "[r] w t ss f t"),
    // CBuffAuraUnholy (`FUN_6f4ea950`, own `FUN_6f4ebac0`: a real, two words, three
    // reals more): the aura base's, an extended timer, a reference, and a timer.
    (codes::buff::id::UNHOLY_AURA, "wwww wwwwwww t w w f www fff f ww f ff", "[r] w t ss f r t"),
    // CAbilityBlightRegen (`FUN_6f42d4b0`): a timer; CAbilityAcolyteHarvest
    // (`FUN_6f42cf90`, own `FUN_6f42e8d0`: a real and three words): from build 6000 a
    // timer.
    (codes::ability::id::REGENERATE_ON_BLIGHT, "wwww wwwwwww t w", "t"),
    (codes::ability::id::ACOLYTE_HARVEST, "wwww wwwwwww t w f www", "t"),
    // On CAbilityUpgrade-like's (`FUN_6f42d320`: two references): CAbilityQueue
    // (`FUN_6f42df70`, own `FUN_6f42f030`: two reals, a word, counted lists) two
    // references more; CAbilitySellUnit (`FUN_6f42d3c0`, own `FUN_6f42e970`) twelve
    // slots (a word, three reals, two extended timers, a word) and five words.
    (codes::ability::id::QUEUE, "wwww wwwwwww t w ff w [w] [www] [w]", "r r r r"),
    (codes::ability::id::SELL_UNITS, "wwww wwwwwww t w ff", "r r 12(w fff t ss f t ss f w) wwwww"),
    // CAbilityRally (`FUN_6f42dff0`, own `FUN_6f42f120`): a counted list of references.
    (codes::ability::id::RALLY, "wwww wwwwwww t w www ff wwww", "[r]"),
    // CAbilityInventory (`FUN_6f5675a0`): counted lists, a word, a reference, and its
    // slots' timers.
    (codes::ability::id::INVENTORY, "wwww wwwwwww t w", "[www] w r [w] [t]"),
    // CAbilityMilitiaConvert (`FUN_6f648660`): a timer.
    (codes::ability::id::MILITIA_CONVERSION, "wwww wwwwwww t w", "t"),
    // CAbilityNeutralDetectAoe (the buff base's tail; own `FUN_6f42ef50`: from build
    // 6037 a word).
    (codes::ability::id::NEUTRAL_DETECTION_REVEAL_ABILITY, "wwww wwwwwww t w w", "wwwww"),
    // CAbilityNeutral (`FUN_6f42dd20`): counted references, a reference, a counted
    // list of three words, a reference, and a timer.
    (codes::ability::id::NEUTRAL_BUILDING, "wwww wwwwwww t w", "[r] r [www] r t"),
    // COrderPoint (`FUN_6f6961c0`, own `FUN_6f696bd0`): the order's three words and a
    // reference.
    (b"ord.", "wwwwww ff", "www r"),
    // CGhostImage (`FUN_6f6db1a0`, the agent base alone; own `FUN_6f6db2b0`: three
    // words): its sprite, two shorts, five words, three words, nine words, a short.
    (codes::class::FOG_GHOST, "!www", "p ss wwwww www 9w s"),
    // CAbilityAttack (`FUN_6f49bd60`): its fields
    // in order, with counted lists of words, of reals, and of three reals
    // (`FUN_6f491650`), timers, references (`FUN_6f4915d0`), and words.
    (codes::ability::id::ATTACK, "wwww wwwwwww t w", "wwwwww [w][w][w][w] [fff] [w][w][w][w] [fff][fff] [f] [fff][fff] fff tttt [w] w [r] ff [f][f] f ttt ff wwww f [www] [t] www fff wwww f"),
    // Creep camps (the agent base alone, no own fields): an indicator
    // (`FUN_6f64f590`) a reference, a short, three reals, two words; the camps' UI
    // (`FUN_6f64f5f0`) a counted list of references.
    (codes::class::CURSOR_PART, "!", "r s fff ww"),
    (codes::class::CURSOR, "!", "[r]"),
    // `FUN_6f5f5270` on the buff base: a reference, two timers, and from build 6026 a
    // reference (own as `Alam`'s).
    (codes::ability::id::ENSNARE, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t r"),
    // CAbilityGoldMine (`FUN_6f5f54f0` on `FUN_6f4ea3a0`: a reference; own
    // `FUN_6f5f62c0`): counted references (`FUN_6f5b0980`) and a timer.
    (codes::ability::id::GOLD_MINE, "wwww wwwwwww t w w f wwwww wwww f wwwwww", "r [r] t"),
    // MineShaft (`FUN_6f5f5f20`, the agent base alone; own `FUN_6f5f7ab0`): a timer.
    (codes::class::MINE_SHAFT, "!wwww f", "t"),
    // CDetectedData (`FUN_6f696040`, own `FUN_6f03edf0`: a word): a word and a counted
    // list of words (a player's each: read, 16), twice.
    (codes::class::DETECTION, "w", "w [w] w [w]"),
    // A player's upgrade (CPlayerUpgrade vf14): five words, then a counted list of
    // three each.
    (codes::class::PLAYER_UPGRADE, "w", "wwwww [www]"),
    // The upgrades' effects (CUpgradeArmor and the rest, own `FUN_6f6b5140`): their own
    // fields alone.
    (codes::upgrade::id::ARMOR, "wffw", ""), // CUpgradeArmor
    (codes::upgrade::id::ARMOR_TYPE, "wffw", ""), // CUpgradeArmorType
    (codes::upgrade::id::SPILL_DISTANCE, "wffw", ""), // CUpgradeAttackSpillDistance
    (codes::upgrade::id::ATTACK_TARGETS, "wffw", ""), // CUpgradeAttackTargetCount
    (codes::upgrade::id::ATTACK_DICE, "wffw", ""), // CUpgradeAttackDice
    (codes::upgrade::id::ATTACK_RANGE, "wffw", ""), // CUpgradeAttackRange
    (codes::upgrade::id::ATTACK_SPEED, "wffw", ""), // CUpgradeAttackSpeed
    (codes::upgrade::id::ATTACK_DAMAGE, "wffw", ""), // CUpgradeAttackEx
    (codes::upgrade::id::AURA_DATA_AN_ABILITYS_DATAA, "wffw", ""), // CUpgradeAuraData
    (codes::upgrade::id::ULTRAVISION, "wffw", ""), // CUpgradeAddUltraVision
    (codes::upgrade::id::WEAPONS_ENABLED, "wffw", ""), // CUpgradeEnableWeapon
    (codes::upgrade::id::HIT_POINTS_PERCENT, "wffw", ""), // CUpgradeHitPoints
    (codes::upgrade::id::HIT_POINT_REGENERATION, "wffw", ""), // CUpgradeHitPointsRegen
    (codes::upgrade::id::HIT_POINTS, "wffw", ""), // CUpgradeHitPointsEx
    (codes::upgrade::id::ABILITY_LEVEL, "wffw", ""), // CUpgradeSpell
    (codes::upgrade::id::LUMBER_HARVESTED, "wffw", ""), // CUpgradeLumberJack
    (codes::upgrade::id::MANA_PERCENT, "wffw", ""), // CUpgradeMana
    (codes::upgrade::id::MANA_REGENERATION, "wffw", ""), // CUpgradeManaRegen
    (codes::upgrade::id::MANA, "wffw", ""), // CUpgradeManaEx
    (codes::upgrade::id::MOVE_SPEED_PERCENT, "wffw", ""), // CUpgradeMove
    (codes::upgrade::id::MOVE_SPEED, "wffw", ""), // CUpgradeMoveEx
    (codes::upgrade::id::RAISE_DEAD_DURATION, "wffw", ""), // CUpgradeRaiseDead
    (codes::upgrade::id::SIGHT, "wffw", ""), // CUpgradeSight
    (codes::upgrade::id::SPIKES, "wffw", ""), // CUpgradeSpikes
    (codes::upgrade::id::MAXIMUM_TRAINED, "wffw", ""), // CUpgradeAddTechMaxAllowed
    // (Its mask of the players shown it, and each one's count.)
    (codes::class::SHARED_VISION, "w", "w [w]"),
    // Harvests (`FUN_6f42da60` on `FUN_6f42d950`: a counted list of three words, of
    // timers, six words, a real; then two timers and from build 6028 a third; own on
    // `FUN_6f42ee40`: the ability's and eight words, then two reals and a word):
    // CAbilityHarvestLumber; CAbilityHarvest (`FUN_6f42d8c0`, own `FUN_6f42ec10`) from
    // build 6037 a timer more and three reals more.
    (codes::ability::id::HARVEST_LUMBER, "wwww wwwwwww t w wwww wwww ff w", "[www] [t] wwwwww f t t t"),
    (codes::ability::id::HARVEST, "wwww wwwwwww t w wwww wwww ff w fff", "[www] [t] wwwwww f t t t t"),
    // CThreadAI (`FUN_6f9bef30`, own `FUN_6f9bf070`: six words and a real): an extended
    // timer and a word.
    (codes::class::AI_THREAD, "wwwwww f", "t ss f w"),
    // CSoundWar3 (`FUN_6f2469a0`): its extended timer, then a flag and a sound's
    // (`FUN_6f359e90`: its file, three reals, six words), or else another flag and a
    // sound's (`FUN_6f359fa0`: its file, a reference, words and reals, a second string,
    // with a flag sixteen words more, and words by the bits of a mask).
    (codes::class::SOUND, "w", "t ss f ?(z fff www w)(?(z r w fff w z ?(f ww fff wwwwww f www) m &1(ww) &2(www) &4(www) &8(www) &10(www) &20(w) &40(w) &80(w) &100(w)))"),
    // Build abilities (`FUN_6f567130` on CAbilityUpgrade-like's two references): a count
    // and a short, then each building its two words, three references, four words, and
    // with a flag four words and two reals (own `FUN_6f567ac0`: two reals and three
    // words; `FUN_6f567ec0` three words more).
    (codes::ability::id::BUILD_HUMAN, "wwww wwwwwww t w ff www", "r r w s #(ww r r r www ?(wwww ff))"),
    (codes::ability::id::BUILD_UNDEAD, "wwww wwwwwww t w ff www www", "r r w s #(ww r r r www ?(wwww ff))"),
    // The orcs', night elves', and neutral's (own `FUN_6f567c60`: two reals, six words).
    (codes::ability::id::BUILD_ORC, "wwww wwwwwww t w ff www www", "r r w s #(ww r r r www ?(wwww ff))"),
    (codes::ability::id::BUILD_NIGHT_ELF, "wwww wwwwwww t w ff www www", "r r w s #(ww r r r www ?(wwww ff))"),
    (codes::ability::id::BUILD_NEUTRAL, "wwww wwwwwww t w ff www www", "r r w s #(ww r r r www ?(wwww ff))"),
    // CAbilityBuildInProgress (`FUN_6f42d4f0`, own `FUN_6f42ea00`: two reals, three
    // words, seven reals, a real from build 6047): a word, a reference, two timers.
    (codes::ability::id::BUILD_IN_PROGRESS, "wwww wwwwwww t w ff www fffffff f", "r r w r t t"),
    // CAbilityCannibalize (`FUN_6f4e9db0` on the buff base: a reference, two timers, two
    // references, a timer; own `FUN_6f4eb120`: `Alam`'s, three words, two from 6011).
    (codes::ability::id::CANNIBALIZE, "wwww wwwwwww t w ffff wwww f wwww ff wwww ff www ww", "wwwww r t t r r t"),
    // CQuestManager (`FUN_6f246820`, own `FUN_6f03edf0`: a word): its quests and its
    // defeat conditions, each a reference.
    (codes::class::TRIGGER_QUEUE, "w", "[r] [r]"),
    // CAbilityBlightedGoldMine (`FUN_6f4e9bb0` on `FUN_6f4ea4b0` and `FUN_6f4ea3a0`: a
    // reference, two more, a timer from build 6037, and a counted list of references;
    // own `FUN_6f4eb0f0` on `FUN_6f4eb6e0`: three words, a real, a word, two reals, two
    // words).
    (codes::ability::id::BLIGHTED_GOLD_MINE, "wwww wwwwwww t w www f w ff ww", "r r r t [r]"),
    // CTimerDialogWar3 (`FUN_6f246ad0`, own `FUN_6f2472c0`: three words and a real): a
    // word, two words (`FUN_6f3b6e90`, `FUN_6f3b6fc0`), a string, and a real.
    (codes::class::TIMER_DIALOG, "www f", "w ww z f"),
    // CLeaderboardWar3 (`FUN_6f246530`, own `FUN_6f03edf0`: a word): its frame's
    // (`FUN_6f3cb7a0`: its size, a short count of items, each `FUN_6f3a6b00`'s label
    // and seven words, and whether shown), a word (+0x40, the frame's +0x194), a word
    // (+0x28), and its title.
    (codes::class::LEADERBOARD, "w", "w {z wwwwwww} w w w z"),
    // CCameraSetup (`FUN_6f246030`, own `FUN_6f03edf0`): three reals (+0xa4 to +0xac).
    (codes::class::CAMERA_SETUP, "w", "fff"),
    // CTriggerRegion (`FUN_6f05fea0`, the agent base alone, no own fields): the lists
    // of events 0x401900c8 and 0x401900c9 (its enter and leave registrations), and its
    // pathing region (+0x20).
    (codes::class::REGION, "!", "e e r"),
    // CRegionEventReg (`FUN_6f27eb00`, own `FUN_6f27f140`: five words): its region
    // (+0x34), its filter (+0x38), a timer (+0x3c, CAgentTimer), a counted list of a
    // reference and a word (+0x50, `FUN_6f271ae0`), and events 0x80205's and 0x80206's
    // lists.
    (codes::class::REGION_EVENT, "wwwww", "r r t [rw] e e"),
    // CTriggerCondition (`FUN_6f299690`, own `FUN_6f03edf0`): its filter (+0x24).
    (codes::class::TRIGGER_CONDITION, "w", "r"),
    // CPlayerChatEventReg (`FUN_6f27e820` on the player event's, own `FUN_6f27f190`:
    // the player event's and a word): its text (+0x50), and events 0x80210's and
    // 0x80260's lists.
    (codes::class::PLAYER_CHAT_EVENT, "wwwwwwwwww w", "r [rw] z e e"),
    // CGuardPost (`FUN_6f03eda0`, own `FUN_6f9befe0`): a word, two reals (+0x28, +0x30),
    // and eight words.
    (codes::ability::id::GUARD_POST, "w ff wwwwwwww", ""),
    // CCompAI (`FUN_6f9ca100`, own `FUN_6f9ca230`: words, reals, and counted lists):
    // two words, a counted list of a reference, a word, a reference, a word, and a
    // reference, a counted list of references, and eight timers.
    (codes::class::PLAYER_AI, "wwww f ww f ww f ww f w [wwf] [f] w ff wwf wwf wwf [w] wwfw w ww [w] [w] [wwf] w wwf wwf w wwwf wwf ww", "ww [rwrwr] [r] 8t"),
    // CGameCacheManager (`FUN_6f246380`, own `FUN_6f03edf0`: a word): a string and a
    // word, then its caches (`FUN_6f21acd0`: a name, a word, and its missions,
    // `FUN_6f21b1d0`: a name, five words, and five lists of named values: words,
    // reals, words, units (`FUN_6f21b0b0`: a word, a list of three words, and
    // `FUN_6f224e20`'s fields), and strings).
    (codes::class::GAME_CACHE_MANAGER, "w", "z w [z w [z wwwww [zw] [zf] [zw] [zw [www] wwwww f w fff w f [ww] fff [w] f s] [zz]]]"),
    // CCaptainAI (`FUN_6f9d5cb0` on `FUN_6f6533a0` and `FUN_6f6be8b0`: a sprite and a
    // word; a word, two shorts, a byte and with it a short-counted list of references
    // and two reals, two bytes, a real; four references, two timers, a counted list of
    // references, four words, then its fields through CSave, `FUN_6f9d41a0`: words by
    // vf 8, reals by vf 4, and two words and a real by vf 0; own `FUN_6f03edf0`: a
    // word).
    (codes::class::AI_CAPTAIN, "w", "p w w ss ?b({r} ff) b b f r r r r t t [r] wwww wwww (wwf) wwwwww fffffff ww ff 19(wwf) 12w"),
    // CCameraWar3 (`FUN_6f362530`, no event lists; own `FUN_6f03ede0`, nothing): a word,
    // a real, a word, a reference, three reals, nine reals (`FUN_6f06b880`), a timer,
    // two words, a counted list of three reals (`FUN_6f35d3f0`), twenty references, a
    // byte, five references, sixteen reals, and four sets of nine reals.
    (codes::class::CAMERA, "!", "w f w r fff 9f t ww [fff] 20r b 5r 16f 4(9f)"),
    // CGameState (`FUN_6f28aad0`, own `FUN_6f28b190`: three words and a real): ten more
    // event lists; a counted list of counted lists of words; a counted list of words; a
    // size, and its counted tables (a size, a count, with any a name, and that many
    // references and words); four references; a size and a counted list of a word, a
    // reference, and a word (`FUN_6f282260`); a size and a counted list of a reference
    // and two words; a counted list of a word and its counted shorts; three words and a
    // real; two timers; and `FUN_6f9a1230`'s: a word, a counted list of words and a word
    // (`FUN_6f9a0080`), two words, and a counted list of a word, ten reals, a word, a
    // byte, a word, and a string.
    (codes::class::GAME_STATE, "www f", "10e [[w]] [w] w [w w ~(z) #(rw)] r r r r w [w r w] w [r w w] [w [s]] w w f w t t w [w] w w w [w ffff ffffff w b w z]"),
    // CItem (`FUN_6f65ab10`, own `FUN_6f03edf0`: a word): the widget's state (as
    // `Widget`), three reals, four words, a real, a word, a counted list of two words
    // and a real, a timer, two words, and a timer.
    (codes::class::ITEM, "w", "p w w ss ?b({r} ff) bb f b fff wwww f w [wwf] t ww t"),
    (b"tsk.", "wwwww fff", "r"),
    (codes::class::TASK_T, "wwwwwwww f", "r"),
    // CPlayerWar3 (`FUN_6f269d70`): the lists of events 0x8020b to 0x80233, 0x80305 to
    // 0x80318, 0x80260 and 0x80261; its name and a byte; its selection
    // (`FUN_6f292c10`: a list, ten control groups' two lists each, references, lists of
    // lists, words); a reference; two counted lists of words; twelve words; seven
    // references; two reals; counted references and words; a word; `FUN_6f246a30`
    // (short-counted lists of eight words and of two, four words); words, a counted list
    // of three words, words, its timer, words, a string, and six words.
    (codes::class::PLAYER, "w", "63e zb [r] w 20[r] r r [r] [[r]] w [r] r r [[r]] w w w r w r [w] [w] 12w 7r ff [r] [w] w {wfffffff} {ww} wwww wwww [www] www t www z wwwwww"),
];

/// Agents whose class writes its whole second record itself, without the agent's
/// handlers, by kind.
const WHOLE: &[(&[u8; 4], &str)] = &[
    // CGroup (`FUN_6f2464b0`): its units (`FUN_6f28d2b0`), two references, four words.
    (codes::class::GROUP, "[r] r r wwww"),
];

/// CUnit's fields after its event lists, in the order `FUN_6f696490` writes them.
const UNIT_FIELDS: &str = "t wwwwwww r s www t wwwwww www wwwwwwwwwwww www www rf rf rff wwwwwwwwwwwwwwwwwwwww fff wwwww rrr ffff bb w www r b ww wwwwww www wwwww w";

/// A widget's sprite as saved (`FUN_6f1a4050`): its model's path, a real, a short, a
/// byte, a word, two reals, two words, a real, a byte, three bytes, and a list of
/// entries (`FUN_6f19de40`: a real, a word, and a list of words each).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Sprite {
    pub model: String,
    pub real: f32,
    pub short: u16,
    pub byte: u8,
    pub word: u32,
    pub reals: [f32; 2],
    pub words: [u32; 2],
    pub last_real: f32,
    pub last_byte: u8,
    pub flags: [u8; 3],
    pub entries: Vec<(f32, u32, Vec<u32>)>,
}

impl Sprite {
    fn read(r: &mut Reader) -> Result<Option<Sprite>, Error> {
        if r.u8()? == 0 {
            return Ok(None);
        }
        let mut s = Sprite { model: r.cstr()?, real: r.f32()?, short: r.u16()?, byte: r.u8()?, word: r.u32()?, ..Default::default() };
        s.reals = reals(r)?;
        s.words = words(r)?;
        s.last_real = r.f32()?;
        s.last_byte = r.u8()?;
        s.flags = [r.u8()?, r.u8()?, r.u8()?];
        let n = r.u32()?;
        for _ in 0..n {
            let (real, word) = (r.f32()?, r.u32()?);
            let list = (0..r.u32()?).map(|_| r.u32()).collect::<Result<_, _>>()?;
            s.entries.push((real, word, list));
        }
        Ok(Some(s))
    }
}

/// A widget's state after its handlers (`FUN_6f6be8b0`'s tail, `FUN_6f6533a0`,
/// `FUN_6f654c20`): its sprite, its owner, a word (a unit's type), two shorts, an
/// optional list of references with two reals (`FUN_6f064240`), two bytes, a real, and
/// a byte.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Widget {
    pub sprite: Option<Sprite>,
    /// Its owner's player number (+0x24: read, the units' 0, 1, 12, and 15 by their
    /// owners).
    pub owner: u32,
    pub word: u32,
    pub shorts: [u16; 2],
    pub refs: Option<(Vec<Ref>, [f32; 2])>,
    pub bytes: [u8; 2],
    pub last_real: f32,
    pub byte: u8,
}

impl Widget {
    fn read(r: &mut Reader) -> Result<Widget, Error> {
        let mut w = Widget { sprite: Sprite::read(r)?, owner: r.u32()?, word: r.u32()?, shorts: [r.u16()?, r.u16()?], ..Default::default() };
        if r.u8()? != 0 {
            let refs = (0..r.u16()?).map(|_| Ref::read(r)).collect::<Result<_, _>>()?;
            w.refs = Some((refs, reals(r)?));
        }
        w.bytes = [r.u8()?, r.u8()?];
        w.last_real = r.f32()?;
        w.byte = r.u8()?;
        Ok(w)
    }
}

impl SavedObject {
    /// Its second record read by its kind.
    pub fn second(&self) -> Result<Second, Error> {
        if self.second.is_empty() {
            return Ok(Second::Empty);
        }
        let mut r = Reader::new(&self.second);
        let out = match &self.entry.kind.to_be_bytes() {
            codes::class::UNIT_AI_REF => Second::UnitRefs { words: words(&mut r)?, handlers: Handlers::read(&mut r)? },
            codes::class::UNIT => {
                let own = r.u32()?;
                let (handlers, widget) = (Handlers::read(&mut r)?, Widget::read(&mut r)?);
                let list = |r: &mut Reader| -> Result<Vec<(Ref, u32)>, Error> { (0..r.u32()?).map(|_| Ok((Ref::read(r)?, r.u32()?))).collect() };
                let lists = (0..37 + 17).map(|_| list(&mut r)).collect::<Result<_, _>>()?;
                Second::Unit { own, handlers, widget, lists, fields: fields(&mut r, UNIT_FIELDS)? }
            }
            codes::class::DESTRUCTABLE => {
                let own = (r.u32()?, reals(&mut r)?);
                let (handlers, widget) = (Handlers::read(&mut r)?, Widget::read(&mut r)?);
                Second::Destructable { own, handlers, widget, bytes: [r.u8()?, r.u8()?], words: words(&mut r)? }
            }
            kind => match (AGENTS.iter().find(|(k, _, _)| *k == kind), WHOLE.iter().find(|(k, _)| *k == kind)) {
                // An own layout starting "!": no lists for events 0xd0000 to 0xd03ff.
                (Some((_, own, tail)), _) => match own.strip_prefix('!') {
                    Some(own) => Second::Agent { own: fields(&mut r, own)?, handlers: Handlers::read_base(&mut r)?, tail: fields(&mut r, tail)? },
                    None => Second::Agent { own: fields(&mut r, own)?, handlers: Handlers::read(&mut r)?, tail: fields(&mut r, tail)? },
                },
                (None, Some((_, layout))) => Second::Whole(fields(&mut r, layout)?),
                _ => return Ok(Second::Raw(self.second.clone())),
            },
        };
        if r.remaining() != 0 {
            return Err(Error::Format("an object's second record longer than its class's fields"));
        }
        Ok(out)
    }
}

/// The script data as saved (`FUN_6f90d350`): the JASS virtual machines' code, their
/// states, and their threads, each section its size first.
#[derive(Clone, Debug, Default)]
pub struct ScriptData {
    /// The number of code blocks' places (+0x44, the first unused) and threads' (+0x8c).
    pub code_places: u32,
    pub thread_places: u32,
    /// The code blocks by place (from 1; `None` for an empty one).
    pub code: Vec<Option<Code>>,
    /// The machines (`FUN_6f9074d0`), each with its place.
    pub machines: Vec<(u32, Machine)>,
    /// Each machine's threads (`FUN_6f91a920`), in the machines' order.
    pub threads: Vec<Vec<Thread>>,
    /// The machines' places by thread (`FUN_6f90d350`, each +0x2850), from the second.
    pub owners: Vec<u32>,
}

/// A code block (`FUN_6f910730`): its words (two per instruction) and its strings
/// (from the second).
#[derive(Clone, Debug, Default)]
pub struct Code {
    pub words: Vec<u32>,
    pub strings: Vec<String>,
}

/// A JASS machine's state (`FUN_6f91a0d0`).
#[derive(Clone, Debug, Default)]
pub struct Machine {
    /// Its code block's place (`FUN_6f9108a0`).
    pub code: u32,
    /// A word (+0x34), a real, and a word (+0x44).
    pub flag: u32,
    pub real: f32,
    pub word: u32,
    /// Its instruction (in words from its code's start) and its return instructions.
    pub at: u32,
    pub returns: Vec<u32>,
    /// Its functions (from the second): name and a word (+0xc).
    pub functions: Vec<(String, u32)>,
    /// Its globals by name.
    pub globals: Vec<(String, Value)>,
    /// Its frames (`FUN_6f91a540`).
    pub frames: Vec<Frame>,
    /// Its stack's values.
    pub stack: Vec<Value>,
    /// Its strings (from the second, `FUN_6f916930`).
    pub strings: Vec<String>,
}

/// A JASS value (`FUN_6f91a6e0`): its type (+0x18; above 8 an array), a word (+0x1c),
/// an array's element count (`FUN_6f916a50`), and its word (a code value an
/// instruction) or an array's words.
#[derive(Clone, Debug, Default)]
pub struct Value {
    pub kind: u32,
    pub word: u32,
    pub length: u32,
    pub value: Contents,
}

/// A JASS value's word, or an array's words.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Contents {
    One(u32),
    Array(Vec<u32>),
}

impl Default for Contents {
    fn default() -> Self {
        Contents::One(0)
    }
}

/// A frame (`FUN_6f91a540`): its arguments, a word (+0xb8), and its locals by name.
#[derive(Clone, Debug, Default)]
pub struct Frame {
    pub arguments: Vec<Value>,
    pub word: u32,
    pub locals: Vec<(String, Value)>,
}

/// A thread (`FUN_6f91a790`): its machine's place, its instruction, its frames, a word,
/// a real, and a word, and its stack.
#[derive(Clone, Debug, Default)]
pub struct Thread {
    pub machine: u32,
    pub at: u32,
    pub frames: Vec<Frame>,
    pub flag: u32,
    pub real: f32,
    pub word: u32,
    pub stack: Vec<Value>,
}

/// Reads a section, its size first, by `f`, which must read it all.
fn sized<T>(r: &mut Reader, f: impl FnOnce(&mut Reader) -> Result<T, Error>) -> Result<T, Error> {
    let n = r.u32()? as usize;
    let mut s = Reader::new(r.bytes(n)?);
    let t = f(&mut s)?;
    if s.remaining() != 0 {
        return Err(Error::Format("a script data section not read to its end"));
    }
    Ok(t)
}

/// A word count, then that many by `f`.
fn counted<T>(r: &mut Reader, mut f: impl FnMut(&mut Reader) -> Result<T, Error>) -> Result<Vec<T>, Error> {
    let n = r.u32()?;
    (0..n).map(|_| f(r)).collect()
}

/// A word count, then one fewer by `f` (the tables whose first entry is not saved).
fn from_second<T>(r: &mut Reader, mut f: impl FnMut(&mut Reader) -> Result<T, Error>) -> Result<Vec<T>, Error> {
    let n = r.u32()?;
    (1..n.max(1)).map(|_| f(r)).collect()
}

impl Value {
    fn read(r: &mut Reader) -> Result<Value, Error> {
        sized(r, |r| {
            let (kind, word, array, length) = (r.u32()?, r.u32()?, r.u32()?, r.u32()?);
            let value = if array != 0 { Contents::Array(counted(r, |r| r.u32())?) } else { Contents::One(r.u32()?) };
            Ok(Value { kind, word, length, value })
        })
    }
}

impl Frame {
    fn read(r: &mut Reader) -> Result<Frame, Error> {
        sized(r, |r| {
            let arguments = counted(r, Value::read)?;
            let word = r.u32()?;
            let locals = sized(r, |r| counted(r, |r| Ok((r.cstr()?, Value::read(r)?))))?;
            Ok(Frame { arguments, word, locals })
        })
    }
}

impl ScriptData {
    pub fn parse(b: &[u8]) -> Result<ScriptData, Error> {
        let mut r = Reader::new(b);
        let mut d = ScriptData { code_places: r.u32()?, thread_places: r.u32()?, ..Default::default() };
        let places = d.code_places;
        d.code = sized(&mut r, |r| {
            (1..places.max(1))
                .map(|_| {
                    if r.u32()? == 0 {
                        return Ok(None);
                    }
                    sized(r, |r| {
                        let words = counted(r, |r| r.u32())?;
                        let strings = from_second(r, |r| r.cstr())?;
                        Ok(Some(Code { words, strings }))
                    })
                })
                .collect()
        })?;
        d.machines = sized(&mut r, |r| {
            counted(r, |r| {
                let place = r.u32()?;
                let code = r.u32()?;
                let m = sized(r, |r| {
                    let mut m = Machine { code, flag: r.u32()?, real: r.f32()?, word: r.u32()?, at: r.u32()?, ..Default::default() };
                    m.returns = counted(r, |r| r.u32())?;
                    m.functions = sized(r, |r| from_second(r, |r| Ok((r.cstr()?, r.u32()?))))?;
                    m.globals = sized(r, |r| counted(r, |r| Ok((r.cstr()?, Value::read(r)?))))?;
                    m.frames = sized(r, |r| counted(r, Frame::read))?;
                    m.stack = sized(r, |r| counted(r, Value::read))?;
                    m.strings = sized(r, |r| from_second(r, |r| r.cstr()))?;
                    Ok(m)
                })?;
                Ok((place, m))
            })
        })?;
        let n = d.machines.len();
        d.threads = sized(&mut r, |r| {
            (0..n)
                .map(|_| {
                    sized(r, |r| {
                        counted(r, |r| {
                            sized(r, |r| {
                                let (machine, at) = (r.u32()?, r.u32()?);
                                let frames = sized(r, |r| counted(r, Frame::read))?;
                                let (flag, real, word) = (r.u32()?, r.f32()?, r.u32()?);
                                let stack = sized(r, |r| counted(r, Value::read))?;
                                Ok(Thread { machine, at, frames, flag, real, word, stack })
                            })
                        })
                    })
                })
                .collect()
        })?;
        d.owners = sized(&mut r, |r| from_second(r, |r| r.u32()))?;
        if r.remaining() != 0 {
            return Err(Error::Format("script data not read to its end"));
        }
        Ok(d)
    }
}

/// A record being written, as the original's data store writes (little-endian).
#[derive(Default)]
struct Out(Vec<u8>);

impl Out {
    fn u8(&mut self, v: u8) {
        self.0.push(v);
    }
    fn u16(&mut self, v: u16) {
        self.0.extend(v.to_le_bytes());
    }
    fn u32(&mut self, v: u32) {
        self.0.extend(v.to_le_bytes());
    }
    fn f32(&mut self, v: f32) {
        self.0.extend(v.to_le_bytes());
    }
    fn cstr(&mut self, s: &str) {
        self.0.extend(s.as_bytes());
        self.0.push(0);
    }
    fn words(&mut self, w: &[u32]) {
        for &v in w {
            self.u32(v);
        }
    }
    fn reals(&mut self, w: &[f32]) {
        for &v in w {
            self.f32(v);
        }
    }
    /// A section, its size first, as `f` writes it.
    fn sized(&mut self, f: impl FnOnce(&mut Out) -> Result<(), Error>) -> Result<(), Error> {
        let at = self.0.len();
        self.u32(0);
        f(self)?;
        let n = (self.0.len() - at - 4) as u32;
        self.0[at..at + 4].copy_from_slice(&n.to_le_bytes());
        Ok(())
    }
}

impl Ref {
    fn write(&self, o: &mut Out) {
        o.words(&[self.0, self.1]);
    }
}

impl Link {
    fn write(&self, o: &mut Out) {
        self.0.write(o);
        o.u32(self.1);
    }
}

impl Timing {
    fn write(&self, o: &mut Out) {
        match self.0 {
            None => o.u32(0),
            Some((a, b, c, d, e)) => {
                o.u32(1);
                o.reals(&[a, b]);
                o.words(&[c, d, e]);
            }
        }
    }
}

impl ObjectBase {
    fn write(&self, o: &mut Out) {
        o.f32(self.real);
        o.words(&self.words);
        for (a, b, c, d) in &self.links {
            a.write(o);
            b.write(o);
            c.write(o);
            d.write(o);
        }
        for t in &self.timings {
            t.write(o);
        }
    }
}

impl PropertyBase {
    fn write(&self, o: &mut Out) {
        self.object.write(o);
        self.links[0].write(o);
        self.links[1].write(o);
        o.f32(self.real);
        o.u32(self.word);
    }
}

impl<T> Array<T> {
    fn write(&self, o: &mut Out, entry: impl Fn(&mut Out, &T)) {
        o.words(&self.words);
        o.u32(self.entries.len() as u32);
        for e in &self.entries {
            entry(o, e);
        }
    }
}

impl First {
    /// The record as its class writes it.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut o = Out::default();
        let refs = |o: &mut Out, r: &[Ref]| r.iter().for_each(|r| r.write(o));
        match self {
            First::Region { refs: r, word, bounds, words } => {
                refs(&mut o, r);
                o.u32(*word);
                o.words(bounds);
                o.words(words);
            }
            First::Flag { base, value } => {
                base.write(&mut o);
                o.u32(*value);
            }
            First::Real { base, values } => {
                base.write(&mut o);
                o.reals(values);
            }
            First::Pos { base, values, refs: r } => {
                base.write(&mut o);
                o.reals(values);
                refs(&mut o, r);
            }
            First::Agent { object, agent, refs: r, links } => {
                object.write(&mut o);
                agent.write(&mut o);
                o.u32(r.len() as u32);
                refs(&mut o, r);
                links.iter().for_each(|l| l.write(&mut o));
            }
            First::Threshold { object, real, word } => {
                object.write(&mut o);
                o.f32(*real);
                o.u32(*word);
            }
            First::FlagListener { object, word } => {
                object.write(&mut o);
                o.u32(*word);
            }
            First::Interference { object, real, word, array } => {
                object.write(&mut o);
                o.f32(*real);
                o.u32(*word);
                array.write(&mut o, |o, e| o.words(e));
            }
            First::Continuous { object, values } => {
                object.write(&mut o);
                o.reals(values);
            }
            First::Cluster { base, values, refs: r, words, reals, more, flags, last, separate } => {
                base.write(&mut o);
                o.reals(values);
                refs(&mut o, r);
                o.words(words);
                o.reals(reals);
                o.words(more);
                o.words(flags);
                last.write(&mut o);
                if let Some((a, b, w)) = separate {
                    o.f32(*a);
                    o.f32(*b);
                    o.u32(*w);
                }
            }
            First::Path { reals, words, real, word, ways, refs: r } => {
                o.reals(reals);
                o.words(words);
                o.f32(*real);
                o.u32(*word);
                for (a, b, c, d) in ways {
                    a.write(&mut o, |o, e| o.reals(e));
                    o.words(&[*b, *c]);
                    o.u16(*d);
                }
                refs(&mut o, r);
            }
            First::Accelerator { refs: r, array, word } => {
                refs(&mut o, r);
                array.write(&mut o, |o, e| o.words(e));
                o.u32(*word);
            }
            First::System { owner, short } => {
                owner.write(&mut o);
                o.u16(*short);
            }
            First::AgentDef { object, array } => {
                object.write(&mut o);
                array.write(&mut o, |o, e| o.words(e));
            }
            First::Cluster2 { array, words, reals, more, last_reals, word, refs: r } => {
                array.write(&mut o, |o, (a, b, c)| {
                    o.words(a);
                    o.reals(b);
                    o.u32(*c);
                });
                o.words(words);
                o.reals(reals);
                o.words(more);
                o.reals(last_reals);
                o.u32(*word);
                refs(&mut o, r);
            }
            First::ClusterGroup { word, reals } => {
                o.u32(*word);
                o.reals(reals);
            }
            First::ProjectilePos { base, values, refs: r, reals, list, timing } => {
                base.write(&mut o);
                o.reals(values);
                refs(&mut o, r);
                o.reals(reals);
                o.u32(list.len() as u32);
                refs(&mut o, list);
                timing.write(&mut o);
            }
            First::Projectile { words, timings, reals, more, last } => {
                o.words(words);
                timings.iter().for_each(|t| t.write(&mut o));
                o.reals(reals);
                o.words(more);
                o.reals(last);
            }
            First::Empty => {}
            First::Raw(b) => o.0.extend(b),
        }
        o.0
    }
}

impl Handlers {
    fn write(&self, o: &mut Out) {
        let list = |o: &mut Out, l: &[(Ref, u32)]| {
            o.u32(l.len() as u32);
            for (r, w) in l {
                r.write(o);
                o.u32(*w);
            }
        };
        o.u32(self.keyed.len() as u32);
        for (k, l) in &self.keyed {
            o.u32(*k);
            list(o, l);
        }
        for l in self.fixed.iter().chain(&self.events) {
            list(o, l);
        }
    }
}

impl Sprite {
    fn write(s: &Option<Sprite>, o: &mut Out) {
        let Some(s) = s else {
            o.u8(0);
            return;
        };
        o.u8(1);
        o.cstr(&s.model);
        o.f32(s.real);
        o.u16(s.short);
        o.u8(s.byte);
        o.u32(s.word);
        o.reals(&s.reals);
        o.words(&s.words);
        o.f32(s.last_real);
        o.u8(s.last_byte);
        s.flags.iter().for_each(|&b| o.u8(b));
        o.u32(s.entries.len() as u32);
        for (real, word, list) in &s.entries {
            o.f32(*real);
            o.u32(*word);
            o.u32(list.len() as u32);
            o.words(list);
        }
    }
}

impl Widget {
    fn write(&self, o: &mut Out) {
        Sprite::write(&self.sprite, o);
        o.u32(self.owner);
        o.u32(self.word);
        o.u16(self.shorts[0]);
        o.u16(self.shorts[1]);
        match &self.refs {
            None => o.u8(0),
            Some((refs, reals)) => {
                o.u8(1);
                o.u16(refs.len() as u16);
                refs.iter().for_each(|r| r.write(o));
                o.reals(reals);
            }
        }
        o.u8(self.bytes[0]);
        o.u8(self.bytes[1]);
        o.f32(self.last_real);
        o.u8(self.byte);
    }
}

impl Second {
    /// The record as the kind's class writes it.
    pub fn to_bytes(&self, kind: u32) -> Result<Vec<u8>, Error> {
        let mut o = Out::default();
        let kind = &kind.to_be_bytes();
        match self {
            Second::UnitRefs { words, handlers } => {
                o.words(words);
                handlers.write(&mut o);
            }
            Second::Unit { own, handlers, widget, lists, fields } => {
                o.u32(*own);
                handlers.write(&mut o);
                widget.write(&mut o);
                for l in lists {
                    o.u32(l.len() as u32);
                    for (r, w) in l {
                        r.write(&mut o);
                        o.u32(*w);
                    }
                }
                put_fields(&mut o, UNIT_FIELDS, fields)?;
            }
            Second::Destructable { own, handlers, widget, bytes, words } => {
                o.u32(own.0);
                o.reals(&own.1);
                handlers.write(&mut o);
                widget.write(&mut o);
                o.u8(bytes[0]);
                o.u8(bytes[1]);
                o.words(words);
            }
            Second::Agent { own, handlers, tail } => {
                let (_, own_layout, tail_layout) = AGENTS.iter().find(|(k, _, _)| *k == kind).ok_or(Error::Format("no layout for the agent's kind"))?;
                put_fields(&mut o, own_layout.trim_start_matches('!'), own)?;
                handlers.write(&mut o);
                put_fields(&mut o, tail_layout, tail)?;
            }
            Second::Whole(f) => {
                let (_, layout) = WHOLE.iter().find(|(k, _)| *k == kind).ok_or(Error::Format("no layout for the kind"))?;
                put_fields(&mut o, layout, f)?;
            }
            Second::Empty => {}
            Second::Raw(b) => o.0.extend(b),
        }
        Ok(o.0)
    }
}

/// Writes fields by the layout `fields` read them with.
fn put_fields(o: &mut Out, layout: &str, f: &[Field]) -> Result<(), Error> {
    let bad = || Error::Format("fields not as their layout has them");
    let chars: Vec<char> = layout.chars().filter(|c| !c.is_whitespace()).collect();
    let close = |k: usize| {
        let (open, close) = match chars[k] {
            '[' => ('[', ']'),
            '{' => ('{', '}'),
            _ => ('(', ')'),
        };
        let (mut depth, mut j) = (0, k);
        loop {
            if chars[j] == open {
                depth += 1;
            } else if chars[j] == close {
                depth -= 1;
                if depth == 0 {
                    return j;
                }
            }
            j += 1;
        }
    };
    let inner = |a: usize, b: usize| -> String { chars[a + 1..b].iter().collect() };
    // The single group a conditional or plain group holds.
    fn group(f: Option<&Field>) -> Result<&[Field], Error> {
        match f {
            Some(Field::List(g)) if g.len() == 1 => Ok(&g[0]),
            _ => Err(Error::Format("fields not as their layout has them")),
        }
    }
    let mut it = f.iter();
    let (mut mask, mut last) = (0u32, 0u32);
    let mut k = 0;
    while k < chars.len() {
        let mut times = 0usize;
        while chars[k].is_ascii_digit() {
            times = times * 10 + chars[k].to_digit(10).unwrap() as usize;
            k += 1;
        }
        match chars[k] {
            '?' => {
                let byte = chars[k + 1] == 'b';
                let open = if byte { k + 2 } else { k + 1 };
                let x = close(open);
                let y = (chars.get(x + 1) == Some(&'(')).then(|| close(x + 1));
                let flag = match (byte, it.next()) {
                    (true, Some(Field::Byte(b))) => {
                        o.u8(*b);
                        *b as u32
                    }
                    (false, Some(Field::Word(w))) => {
                        o.u32(*w);
                        *w
                    }
                    _ => return Err(bad()),
                };
                if flag != 0 {
                    put_fields(o, &inner(open, x), group(it.next())?)?;
                } else if let Some(y) = y {
                    put_fields(o, &inner(x + 1, y), group(it.next())?)?;
                }
                k = y.unwrap_or(x) + 1;
                continue;
            }
            '~' => {
                let end = close(k + 1);
                if last != 0 {
                    put_fields(o, &inner(k + 1, end), group(it.next())?)?;
                }
                k = end + 1;
                continue;
            }
            '#' => {
                let end = close(k + 1);
                let Some(Field::List(groups)) = it.next() else { return Err(bad()) };
                if groups.len() != last as usize {
                    return Err(bad());
                }
                for g in groups {
                    put_fields(o, &inner(k + 1, end), g)?;
                }
                k = end + 1;
                continue;
            }
            '&' => {
                let open = k + 1 + chars[k + 1..].iter().position(|&c| c == '(').expect("layout bits");
                let bits = u32::from_str_radix(&chars[k + 1..open].iter().collect::<String>(), 16).expect("layout bits");
                let end = close(open);
                if mask & bits != 0 {
                    put_fields(o, &inner(open, end), group(it.next())?)?;
                }
                k = end + 1;
                continue;
            }
            _ => {}
        }
        let start = k;
        if matches!(chars[k], '[' | '{' | '(') {
            k = close(k);
        }
        let token: String = chars[start..=k].iter().collect();
        let body = || token[1..token.len() - 1].to_string();
        for _ in 0..times.max(1) {
            match (token.chars().next().unwrap(), it.next()) {
                ('w', Some(Field::Word(v))) => {
                    last = *v;
                    o.u32(*v);
                }
                ('m', Some(Field::Word(v))) => {
                    mask = *v;
                    o.u32(*v);
                }
                ('f', Some(Field::Real(v))) => o.f32(*v),
                ('b', Some(Field::Byte(v))) => o.u8(*v),
                ('s', Some(Field::Short(v))) => o.u16(*v),
                ('r', Some(Field::Ref(v))) => v.write(o),
                ('t', Some(Field::Timer(r, w, t))) => {
                    r.write(o);
                    o.u32(*w);
                    t.write(o);
                }
                ('z', Some(Field::Text(s))) => o.cstr(s),
                ('n', Some(Field::Bytes(b))) => {
                    o.u32(b.len() as u32);
                    o.0.extend(b);
                }
                ('p', Some(Field::Sprite(s))) => Sprite::write(s, o),
                ('e', Some(Field::List(groups))) => {
                    o.u32(groups.len() as u32);
                    for g in groups {
                        put_fields(o, "rw", g)?;
                    }
                }
                ('[', Some(Field::List(groups))) => {
                    o.u32(groups.len() as u32);
                    for g in groups {
                        put_fields(o, &body(), g)?;
                    }
                }
                ('{', Some(Field::List(groups))) => {
                    o.u16(groups.len() as u16);
                    for g in groups {
                        put_fields(o, &body(), g)?;
                    }
                }
                ('(', Some(Field::List(groups))) if groups.len() == 1 => put_fields(o, &body(), &groups[0])?,
                _ => return Err(bad()),
            }
        }
        k += 1;
    }
    if it.next().is_some() {
        return Err(bad());
    }
    Ok(())
}

impl MapFrame {
    fn write(&self, o: &mut Out) {
        o.words(&[self.width, self.height]);
        o.reals(&self.bounds);
        o.words(&self.cells);
        o.reals(&self.scale);
    }
}

impl PathingMaps {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut o = Out::default();
        for x in &self.refs {
            x.write(&mut o);
        }
        for p in &self.proximity {
            p.frame.write(&mut o);
            o.words(&p.words);
            o.u32(p.cells.len() as u32);
            for (x, y, flags, refs) in &p.cells {
                o.words(&[*x, *y, *flags, refs.len() as u32]);
                for r in refs {
                    r.write(&mut o);
                }
            }
        }
        for g in &self.grids {
            g.frame.write(&mut o);
            for &c in &g.cells {
                o.u16(c);
            }
        }
        for x in &self.tail {
            x.write(&mut o);
        }
        o.0
    }
}

impl ObjectManager {
    fn write(&self, o: &mut Out) {
        let section = |o: &mut Out, b: &[u8]| {
            o.u32(b.len() as u32);
            o.0.extend(b);
        };
        section(o, &self.header);
        for t in &self.tables {
            o.u32((4 + 16 * t.len()) as u32);
            o.u32(t.len() as u32);
            for e in t {
                o.words(&[e.family, e.kind, e.tag, e.birth]);
            }
        }
        section(o, &self.free[0]);
        section(o, &self.free[1]);
        o.u16(self.classes.len() as u16);
        for c in &self.classes {
            o.words(c);
        }
        for &(a, b, c, d, e) in &self.clocks {
            o.f32(a);
            o.u32(b);
            o.f32(c);
            o.words(&[d, e]);
        }
        section(o, &self.block);
        o.u32(self.word);
        for d in &self.data {
            section(o, d);
        }
        o.words(&self.words);
        o.reals(&self.bounds);
    }

    /// Sets each table's objects: their entries, and their records' data sections.
    pub fn set_objects(&mut self, objects: &[Vec<SavedObject>; 2]) {
        for (k, list) in objects.iter().enumerate() {
            self.tables[k] = list.iter().map(|o| o.entry).collect();
            let records = |b: fn(&SavedObject) -> &Vec<u8>| {
                let mut out = Vec::new();
                for o in list {
                    out.extend((b(o).len() as u32).to_le_bytes());
                    out.extend(b(o));
                    out.extend(b"espi");
                }
                out
            };
            self.data[2 + k] = records(|o| &o.first);
            self.data[4 + k] = records(|o| &o.second);
        }
    }
}

impl SaveGame {
    /// The record as the original writes it.
    pub fn to_record(&self) -> Vec<u8> {
        let mut o = Out::default();
        o.cstr(&self.map);
        o.cstr(&self.game_name);
        o.cstr(&self.info_name);
        o.cstr(&self.info_password);
        o.0.extend(&self.info_stat);
        o.u8(0);
        o.words(&self.info_words);
        o.u16(self.slots.len() as u16);
        o.0.extend(&self.slots);
        o.words(&[self.objects_word, self.map_xoro]);
        o.0.extend(self.map_sha1);
        o.u32(self.local_slot as u32);
        o.u32(self.local_player);
        o.cstr(&self.player_name);
        o.words(&self.preferences);
        if let Some(m) = &self.objects {
            o.words(&[self.game_ms, self.game_word]);
            m.write(&mut o);
        }
        o.0.extend(&self.rest);
        o.0
    }

    /// The file: the record in the recorded-game container, in blocks of 0xffff bytes
    /// as the original's saves are, with the header the original's (`h`).
    pub fn to_file(&self, h: &crate::container::Header) -> Vec<u8> {
        crate::container::pack(h, &self.to_record(), 0xffff)
    }
}

impl GameState {
    /// The state's bytes as the original writes them.
    pub fn to_bytes(&self) -> Result<Vec<u8>, Error> {
        let mut o = Out::default();
        for (name, f) in &self.parts {
            if *name == "fog" {
                put_fields(&mut o, "bb wwww", &f[..6])?;
                for m in &f[6..10] {
                    let Field::List(m) = m else { return Err(Error::Format("a fog map not a list")) };
                    for v in m {
                        put_fields(&mut o, "s", v)?;
                    }
                }
                put_fields(&mut o, "ffffff sss w", &f[10..])?;
            } else {
                let (_, layout) = STATE.iter().find(|(n, _)| n == name).ok_or(Error::Format("no such part"))?;
                put_fields(&mut o, layout, f)?;
            }
        }
        o.0.extend(&self.rest);
        Ok(o.0)
    }
}

impl Value {
    fn write(&self, o: &mut Out) -> Result<(), Error> {
        o.sized(|o| {
            o.words(&[self.kind, self.word, matches!(self.value, Contents::Array(_)) as u32, self.length]);
            match &self.value {
                Contents::One(v) => o.u32(*v),
                Contents::Array(a) => {
                    o.u32(a.len() as u32);
                    o.words(a);
                }
            }
            Ok(())
        })
    }
}

/// Named values: a count, then each name and value.
fn put_named(o: &mut Out, v: &[(String, Value)]) -> Result<(), Error> {
    o.u32(v.len() as u32);
    for (n, v) in v {
        o.cstr(n);
        v.write(o)?;
    }
    Ok(())
}

/// Values: a count, then each.
fn put_values(o: &mut Out, v: &[Value]) -> Result<(), Error> {
    o.u32(v.len() as u32);
    v.iter().try_for_each(|v| v.write(o))
}

impl Frame {
    fn write(&self, o: &mut Out) -> Result<(), Error> {
        o.sized(|o| {
            put_values(o, &self.arguments)?;
            o.u32(self.word);
            o.sized(|o| put_named(o, &self.locals))
        })
    }
}

/// Frames: a count, then each.
fn put_frames(o: &mut Out, f: &[Frame]) -> Result<(), Error> {
    o.u32(f.len() as u32);
    f.iter().try_for_each(|f| f.write(o))
}

impl ScriptData {
    /// The script data as `FUN_6f90d350` writes it (without the size before it).
    pub fn to_bytes(&self) -> Result<Vec<u8>, Error> {
        let mut o = Out::default();
        o.words(&[self.code_places, self.thread_places]);
        o.sized(|o| {
            for (k, c) in self.code.iter().enumerate() {
                match c {
                    None => o.u32(0),
                    Some(c) => {
                        o.u32(k as u32 + 1);
                        o.sized(|o| {
                            o.u32(c.words.len() as u32);
                            o.words(&c.words);
                            o.u32(c.strings.len() as u32 + 1);
                            c.strings.iter().for_each(|s| o.cstr(s));
                            Ok(())
                        })?;
                    }
                }
            }
            Ok(())
        })?;
        o.sized(|o| {
            o.u32(self.machines.len() as u32);
            for (place, m) in &self.machines {
                o.words(&[*place, m.code]);
                o.sized(|o| {
                    o.u32(m.flag);
                    o.f32(m.real);
                    o.words(&[m.word, m.at]);
                    o.u32(m.returns.len() as u32);
                    o.words(&m.returns);
                    o.sized(|o| {
                        o.u32(m.functions.len() as u32 + 1);
                        for (n, w) in &m.functions {
                            o.cstr(n);
                            o.u32(*w);
                        }
                        Ok(())
                    })?;
                    o.sized(|o| put_named(o, &m.globals))?;
                    o.sized(|o| put_frames(o, &m.frames))?;
                    o.sized(|o| put_values(o, &m.stack))?;
                    o.sized(|o| {
                        o.u32(m.strings.len() as u32 + 1);
                        m.strings.iter().for_each(|s| o.cstr(s));
                        Ok(())
                    })
                })?;
            }
            Ok(())
        })?;
        o.sized(|o| {
            for threads in &self.threads {
                o.sized(|o| {
                    o.u32(threads.len() as u32);
                    for t in threads {
                        o.sized(|o| {
                            o.words(&[t.machine, t.at]);
                            o.sized(|o| put_frames(o, &t.frames))?;
                            o.u32(t.flag);
                            o.f32(t.real);
                            o.u32(t.word);
                            o.sized(|o| put_values(o, &t.stack))
                        })?;
                    }
                    Ok(())
                })?;
            }
            Ok(())
        })?;
        o.sized(|o| {
            o.u32(self.owners.len() as u32 + 1);
            o.words(&self.owners);
            Ok(())
        })?;
        Ok(o.0)
    }
}

/// A saved unit's state as far as named (read against the unit tables and the screen):
/// the CUnit's record and the properties its agent wrapper lists.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SavedUnit {
    /// Its object's tag and birth (as references to it name it).
    pub tag: u32,
    pub birth: u32,
    /// Its type (the widget's word) and owner.
    pub type_id: u32,
    pub owner: u32,
    /// Its position cluster's (`` `pcu ``): its position in pathing cells from the map's
    /// origin, its facing (radians), its move speed (cells a second), and its collision
    /// radius (cells).
    pub cell: [f32; 2],
    pub facing: f32,
    pub speed: f32,
    pub collision: f32,
    /// Its life and mana (`` `rel `` 1 and 2): the value, its regeneration a second, a
    /// real, and the maximum; the value as of the property's real, an agent clock time
    /// (read: a peasant's life set to 123.5 at one second is saved as 123.5 at
    /// 0.99999905, and the others' full life at 0.0), regeneration counting on from it
    /// (`SavedUnit::now`).
    pub life: [f32; 4],
    pub mana: [f32; 4],
    pub life_at: f32,
    pub mana_at: f32,
    /// Its sight by day (`` `rel `` 5's first).
    pub sight: f32,
}

impl SavedUnit {
    /// A value (life or mana) at an agent clock time: as saved, plus its regeneration
    /// since, within its maximum (a stamp before the clock's last wrap at 300 seconds
    /// counting from there: read, the hero map's Dreadlord at 300 seconds, its clock at
    /// 0.45).
    pub fn now(values: [f32; 4], at: f32, clock: f32) -> f32 {
        let mut since = clock - at;
        while since < 0.0 {
            since += 300.0;
        }
        (values[0] + values[1] * since).min(values[3])
    }
}

impl SaveGame {
    /// The units, in the object table's order.
    pub fn units(&self) -> Result<Vec<SavedUnit>, Error> {
        let objects = self.objects.as_ref().ok_or(Error::Format("no object manager"))?.objects()?;
        let all: std::collections::HashMap<(u32, u32), &SavedObject> = objects.iter().flatten().map(|o| ((o.entry.tag, o.entry.birth), o)).collect();
        let mut out = Vec::new();
        for o in objects.iter().flatten().filter(|o| &o.entry.kind.to_be_bytes() == codes::class::UNIT) {
            let Second::Unit { widget, .. } = o.second()? else { continue };
            let mut u = SavedUnit { tag: o.entry.tag, birth: o.entry.birth, type_id: widget.word, owner: widget.owner, ..Default::default() };
            if let First::Agent { refs, .. } = o.first()? {
                for r in refs {
                    match all.get(&(r.0, r.1)).map(|p| p.first()).transpose()? {
                        Some(First::Cluster { values, .. }) => {
                            u.cell = [values[0], values[1]];
                            (u.speed, u.facing, u.collision) = (values[4], values[5], values[6]);
                        }
                        Some(First::Real { base, values }) => match base.object.words[0] {
                            1 => (u.life, u.life_at) = (values, base.real),
                            2 => (u.mana, u.mana_at) = (values, base.real),
                            5 => u.sight = values[0],
                            _ => {}
                        },
                        _ => {}
                    }
                }
            }
            out.push(u);
        }
        Ok(out)
    }

    /// Each player's state (`` `flg `` properties by the JASS player state's number, gold
    /// and lumber, and the gathered, ten times the value), the players in their agents'
    /// order (player 0 first).
    pub fn player_states(&self) -> Result<Vec<Vec<(u32, i32)>>, Error> {
        let objects = self.objects.as_ref().ok_or(Error::Format("no object manager"))?.objects()?;
        let all: std::collections::HashMap<(u32, u32), &SavedObject> = objects.iter().flatten().map(|o| ((o.entry.tag, o.entry.birth), o)).collect();
        let mut out = Vec::new();
        for o in objects.iter().flatten().filter(|o| &o.entry.kind.to_be_bytes() == codes::class::PLAYER) {
            let mut states = Vec::new();
            if let First::Agent { refs, .. } = o.first()? {
                for r in refs {
                    if let Some(First::Flag { base, value }) = all.get(&(r.0, r.1)).map(|p| p.first()).transpose()? {
                        states.push((base.object.words[0], value as i32));
                    }
                }
            }
            out.push(states);
        }
        Ok(out)
    }

    /// The players' tech registrations, by player (each `+ply`'s counted list of an id
    /// and seven words kept as reals: its limit, made, in production, in production the
    /// other way, present, killed, and flags), in the order of the players' records.
    pub fn player_techs(&self) -> Result<Vec<Vec<[u32; 8]>>, Error> {
        let objects = self.objects.as_ref().ok_or(Error::Format("no object manager"))?.objects()?;
        let mut out = Vec::new();
        for o in objects.iter().flatten().filter(|o| &o.entry.kind.to_be_bytes() == codes::class::PLAYER) {
            let Second::Agent { tail, .. } = o.second()? else { return Err(Error::Format("a player's record not read")) };
            let regs = tail.iter().find_map(|f| match f {
                Field::List(entries) if !entries.is_empty() && entries.iter().all(|e| e.len() == 8 && matches!(e[0], Field::Word(_)) && e[1..].iter().all(|v| matches!(v, Field::Real(_)))) => Some(entries),
                _ => None,
            });
            out.push(regs.map_or_else(Vec::new, |entries| {
                entries
                    .iter()
                    .map(|e| {
                        let mut r = [0u32; 8];
                        for (k, v) in e.iter().enumerate() {
                            r[k] = match v {
                                Field::Word(w) => *w,
                                Field::Real(x) => x.to_bits(),
                                _ => 0,
                            };
                        }
                        r
                    })
                    .collect()
            }));
        }
        Ok(out)
    }

    /// The JASS handles (CGameState's table, `FUN_6f28aad0`'s counted list of a
    /// reference, a word, and a word): each handle's agent and its handle value.
    pub fn handles(&self) -> Result<Vec<(Ref, u32)>, Error> {
        let objects = self.objects.as_ref().ok_or(Error::Format("no object manager"))?.objects()?;
        let o = objects.iter().flatten().find(|o| &o.entry.kind.to_be_bytes() == codes::class::GAME_STATE).ok_or(Error::Format("no game state"))?;
        let Second::Agent { tail, .. } = o.second()? else { return Err(Error::Format("the game state not read")) };
        // The table follows the four references and the size before it.
        let Some(Field::List(entries)) = tail.get(21) else { return Err(Error::Format("no handle table")) };
        Ok(entries
            .iter()
            .filter_map(|e| match e[..] {
                [Field::Ref(r), Field::Word(_), Field::Word(h)] => Some((r, h)),
                _ => None,
            })
            .collect())
    }
}

impl SaveGame {
    /// The script data (`ScriptData`), from the game's state.
    pub fn script_data(&self) -> Result<ScriptData, Error> {
        let g = self.state()?;
        match g.parts.iter().find(|(n, _)| *n == "script data").map(|(_, f)| &f[..]) {
            Some([Field::Bytes(b)]) => ScriptData::parse(b),
            _ => Err(Error::Format("no script data")),
        }
    }

    /// CGameState's indexed reals (`` `rel `` properties of its agent wrapper; 2 the time
    /// of day, read: 15.5 set by SetTimeOfDay), each its index and four values.
    pub fn game_reals(&self) -> Result<Vec<(u32, [f32; 4])>, Error> {
        let objects = self.objects.as_ref().ok_or(Error::Format("no object manager"))?.objects()?;
        let all: std::collections::HashMap<(u32, u32), &SavedObject> = objects.iter().flatten().map(|o| ((o.entry.tag, o.entry.birth), o)).collect();
        let o = objects.iter().flatten().find(|o| &o.entry.kind.to_be_bytes() == codes::class::GAME_STATE).ok_or(Error::Format("no game state"))?;
        let mut out = Vec::new();
        if let First::Agent { refs, .. } = o.first()? {
            for r in refs {
                if let Some(First::Real { base, values }) = all.get(&(r.0, r.1)).map(|p| p.first()).transpose()? {
                    out.push((base.object.words[0], values));
                }
            }
        }
        Ok(out)
    }
}

/// A saved JASS timer (CTimerWar3, `+tmr`) as far as named.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SavedTimer {
    /// Its object's tag and birth.
    pub tag: u32,
    pub birth: u32,
    /// Its timeout (its own fields' first real).
    pub timeout: f32,
    /// While running, its node's block (CAgentTimer's): when it is due on its clock
    /// (the main clock's current wrap), the node's period, and whether the node repeats
    /// (read: a test map's 10-second timer started at its start due at 10.0, the
    /// restock timer 120.0 and repeating).
    pub run: Option<(f32, f32, bool)>,
    /// Its agent timer's flags (2 repeating, 0x10 while in its chunks), its run's
    /// 120-second chunks and those left, and the run beyond them (CAgentTimerExtended's
    /// +0x14, +0x16, and +0x18).
    pub flags: u32,
    pub chunks: (u16, u16),
    pub rest: f32,
}

impl SaveGame {
    /// The players' names, by player in the object table's order (each player's record,
    /// `+ply`, the first text of its tail, after its event lists: read, the original's
    /// save of a LAN game on Booty Bay, "Host" and "Jack"; a computer's "Computer
    /// (Normal)").
    pub fn player_names(&self) -> Result<Vec<String>, Error> {
        Ok(self.player_records()?.into_iter().map(|(name, _)| name).collect())
    }

    /// The players' races as the game drew them, by player (each player's record's
    /// tail word 110, common.j's numbering: 1 human, 2 orc, 3 undead, 4 night elf; read,
    /// Booty Bay's LAN game's human and orc, the hero map's and Echo Isles' human and
    /// undead computer, where the slot table says random).
    pub fn player_races(&self) -> Result<Vec<u32>, Error> {
        Ok(self.player_records()?.into_iter().map(|(_, race)| race).collect())
    }

    fn player_records(&self) -> Result<Vec<(String, u32)>, Error> {
        let objects = self.objects.as_ref().ok_or(Error::Format("no object manager"))?.objects()?;
        let mut out = Vec::new();
        for o in objects.iter().flatten().filter(|o| &o.entry.kind.to_be_bytes() == codes::class::PLAYER) {
            let (name, race) = match o.second()? {
                Second::Agent { tail, .. } => (
                    tail.iter().find_map(|f| match f {
                        Field::Text(t) => Some(t.clone()),
                        _ => None,
                    }),
                    match tail.get(110) {
                        Some(Field::Word(r)) => *r,
                        _ => 0,
                    },
                ),
                _ => (None, 0),
            };
            out.push((name.unwrap_or_default(), race));
        }
        Ok(out)
    }

    /// The slot table's slots (W3GS's nine bytes each after their count: id, download,
    /// status, computer, team, color, race flags, computer level, handicap).
    pub fn slot_list(&self) -> Vec<[u8; 9]> {
        let n = self.slots.first().copied().unwrap_or(0) as usize;
        self.slots.get(1..1 + 9 * n).map_or(Vec::new(), |b| b.chunks_exact(9).map(|c| c.try_into().unwrap()).collect())
    }

    /// The JASS timers, in the object table's order.
    pub fn timers(&self) -> Result<Vec<SavedTimer>, Error> {
        let objects = self.objects.as_ref().ok_or(Error::Format("no object manager"))?.objects()?;
        let mut out = Vec::new();
        for o in objects.iter().flatten().filter(|o| &o.entry.kind.to_be_bytes() == codes::class::TIMER) {
            let Second::Agent { own, tail, .. } = o.second()? else { continue };
            let timeout = match own.get(1) {
                Some(Field::Real(v)) => *v,
                _ => 0.0,
            };
            let (run, flags) = match tail.first() {
                Some(Field::Timer(_, flags, Timing(Some((due, length, _, _, periodic))))) => (Some((*due, *length, *periodic != 0)), *flags),
                _ => (None, 0),
            };
            let short = |k: usize| match tail.get(k) {
                Some(Field::Short(v)) => *v,
                _ => 0,
            };
            let rest = match tail.get(3) {
                Some(Field::Real(v)) => *v,
                _ => timeout,
            };
            out.push(SavedTimer { tag: o.entry.tag, birth: o.entry.birth, timeout, run, flags, chunks: (short(1), short(2)), rest });
        }
        Ok(out)
    }
}

/// A saved destructable as far as named.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SavedDestructable {
    pub tag: u32,
    pub birth: u32,
    /// Its type (the widget's word).
    pub type_id: u32,
    /// Its life and maximum (its own fields' first and third reals; read: a tree the
    /// test map kills goes from 50 of 50 to 0).
    pub life: f32,
    pub max_life: f32,
    /// Its variation, z, and scale (read against Booty Bay's war3map.doo: its last
    /// bytes' first, its last words' first as a real, and the sprite's first real).
    pub variation: u8,
    pub z: f32,
    pub scale: f32,
    /// Where it stands (the widget's position).
    pub x: f32,
    pub y: f32,
}

impl SaveGame {
    /// The destructables, in the object table's order.
    pub fn destructables(&self) -> Result<Vec<SavedDestructable>, Error> {
        let objects = self.objects.as_ref().ok_or(Error::Format("no object manager"))?.objects()?;
        let mut out = Vec::new();
        for o in objects.iter().flatten().filter(|o| &o.entry.kind.to_be_bytes() == codes::class::DESTRUCTABLE) {
            let Second::Destructable { own, widget, bytes, words, .. } = o.second()? else { continue };
            let pos = widget.refs.as_ref().map_or([0.0; 2], |r| r.1);
            let scale = widget.sprite.as_ref().map_or(1.0, |s| s.reals[0]);
            out.push(SavedDestructable {
                tag: o.entry.tag,
                birth: o.entry.birth,
                type_id: widget.word,
                life: own.1[0],
                max_life: own.1[2],
                variation: bytes[0],
                z: f32::from_bits(words[0]),
                scale,
                x: pos[0],
                y: pos[1],
            });
        }
        Ok(out)
    }
}

/// A saved item as far as named.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SavedItem {
    pub tag: u32,
    pub birth: u32,
    /// Its type and owner (the widget's word and +0x24).
    pub type_id: u32,
    pub owner: u32,
    /// Its position property's (`` `pos ``) reals, the first two its place in pathing
    /// cells from the map's origin.
    pub pos: [f32; 7],
    /// Its life and maximum (its own reals' first and third).
    pub life: f32,
    pub max_life: f32,
}

impl SaveGame {
    /// The items, in the object table's order.
    pub fn items(&self) -> Result<Vec<SavedItem>, Error> {
        let objects = self.objects.as_ref().ok_or(Error::Format("no object manager"))?.objects()?;
        let all: std::collections::HashMap<(u32, u32), &SavedObject> = objects.iter().flatten().map(|o| ((o.entry.tag, o.entry.birth), o)).collect();
        let mut out = Vec::new();
        for o in objects.iter().flatten().filter(|o| &o.entry.kind.to_be_bytes() == codes::class::ITEM) {
            let Second::Agent { tail, .. } = o.second()? else { continue };
            let word = |k: usize| match tail.get(k) {
                Some(Field::Word(v)) => *v,
                _ => 0,
            };
            let real = |k: usize| match tail.get(k) {
                Some(Field::Real(v)) => *v,
                _ => 0.0,
            };
            // The widget's sprite, owner, and type first; the item's reals after the
            // widget's fields (`Widget`, eight fields without its optional list).
            let mut i = SavedItem { tag: o.entry.tag, birth: o.entry.birth, owner: word(1), type_id: word(2), ..Default::default() };
            let at = tail.iter().position(|f| matches!(f, Field::Real(_))).map_or(0, |k| k + 2);
            (i.life, i.max_life) = (real(at), real(at + 2));
            if let First::Agent { refs, .. } = o.first()? {
                for r in refs {
                    if let Some(First::Pos { values, .. }) = all.get(&(r.0, r.1)).map(|p| p.first()).transpose()? {
                        i.pos = values;
                    }
                }
            }
            out.push(i);
        }
        Ok(out)
    }
}
