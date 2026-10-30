//! Saved games (.w3z), as the original writes them (`FUN_6f1e8630`): the recorded-game
//! container (`container`), its record a data store of little-endian bytes, words,
//! dwords, and NUL-terminated strings (`FUN_6f06b170`, `FUN_6f06b220`, `FUN_6f06b3a0`,
//! `FUN_6f06bc20`).  Read so far: the header and the object manager's layout (its
//! tables, classes, clocks, and data sections, and each object's records, `first` and
//! `second`), and the game's own state after it (`SaveGame::state`), all but its
//! script data, kept whole.

use crate::{Error, Reader};

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
    /// The engine's object manager (`FUN_6f04dd30`).
    pub objects: ObjectManager,
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
            (s.game_ms, s.game_word, s.objects) = (ms, word, objects);
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
    /// record, three words, eight reals, three words, two flags, a reference (and, with
    /// the second flag, a nested object's record, not yet read).
    Cluster { base: PropertyBase, values: [f32; 7], refs: [Ref; 2], words: [u32; 3], reals: [f32; 8], more: [u32; 3], flags: [u32; 2], last: Ref },
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
            (b"^pmr", _) => {
                let refs = [Ref::read(&mut r)?, Ref::read(&mut r)?];
                let word = r.u32()?;
                let bounds = [r.u32()?, r.u32()?, r.u32()?, r.u32()?];
                First::Region { refs, word, bounds, words: [r.u32()?, r.u32()?, r.u32()?] }
            }
            (b"`flg", _) => First::Flag { base: PropertyBase::read(&mut r)?, value: r.u32()? },
            (b"`rel", _) => First::Real { base: PropertyBase::read(&mut r)?, values: [r.f32()?, r.f32()?, r.f32()?, r.f32()?] },
            (b"`pos", _) => {
                let base = PropertyBase::read(&mut r)?;
                let mut values = [0f32; 7];
                for v in &mut values {
                    *v = r.f32()?;
                }
                First::Pos { base, values, refs: [Ref::read(&mut r)?, Ref::read(&mut r)?] }
            }
            (_, b"+agl") => {
                let object = ObjectBase::read(&mut r)?;
                let agent = Ref::read(&mut r)?;
                let refs = (0..r.u32()?).map(|_| Ref::read(&mut r)).collect::<Result<_, _>>()?;
                First::Agent { object, agent, refs, links: [Link::read(&mut r)?, Link::read(&mut r)?, Link::read(&mut r)?, Link::read(&mut r)?] }
            }
            (b"`rtl", _) => First::Threshold { object: ObjectBase::read(&mut r)?, real: r.f32()?, word: r.u32()? },
            (b"`fcl", _) => First::FlagListener { object: ObjectBase::read(&mut r)?, word: r.u32()? },
            (b"`pil", _) => First::Interference { object: ObjectBase::read(&mut r)?, real: r.f32()?, word: r.u32()?, array: Array::read(&mut r, |r| words::<3>(r))? },
            (b"`rcm", _) => First::Continuous { object: ObjectBase::read(&mut r)?, values: reals(&mut r)? },
            (b"`pcu", _) => {
                let base = PropertyBase::read(&mut r)?;
                let (values, refs) = (reals(&mut r)?, [Ref::read(&mut r)?, Ref::read(&mut r)?]);
                let (words3, reals8, more) = (words(&mut r)?, reals(&mut r)?, words(&mut r)?);
                let flags = words(&mut r)?;
                if flags[1] != 0 {
                    return Ok(First::Raw(self.first.clone()));
                }
                First::Cluster { base, values, refs, words: words3, reals: reals8, more, flags, last: Ref::read(&mut r)? }
            }
            (b"l^pa", _) => {
                let (reals6, words5, real, word) = (reals(&mut r)?, words(&mut r)?, r.f32()?, r.u32()?);
                let way = |r: &mut Reader| -> Result<_, Error> { Ok((Array::read(r, |r| reals::<2>(r))?, r.u32()?, r.u32()?, r.u16()?)) };
                let ways = [way(&mut r)?, way(&mut r)?];
                First::Path { reals: reals6, words: words5, real, word, ways, refs: [Ref::read(&mut r)?, Ref::read(&mut r)?, Ref::read(&mut r)?, Ref::read(&mut r)?] }
            }
            (b"l^ac", _) => First::Accelerator { refs: [Ref::read(&mut r)?, Ref::read(&mut r)?, Ref::read(&mut r)?, Ref::read(&mut r)?], array: Array::read(&mut r, |r| words::<3>(r))?, word: r.u32()? },
            (b"l^sy", _) => First::System { owner: Ref::read(&mut r)?, short: r.u16()? },
            (b"^adf", _) => First::AgentDef { object: ObjectBase::read(&mut r)?, array: Array::read(&mut r, |r| words::<11>(r))? },
            (b"^clu", _) => {
                let array = Array::read(&mut r, |r| Ok((words::<3>(r)?, reals::<6>(r)?, r.u32()?)))?;
                let (words3, reals4, more, last_reals, word) = (words(&mut r)?, reals(&mut r)?, words(&mut r)?, reals(&mut r)?, r.u32()?);
                First::Cluster2 { array, words: words3, reals: reals4, more, last_reals, word, refs: [Ref::read(&mut r)?, Ref::read(&mut r)?, Ref::read(&mut r)?] }
            }
            (b"^ama" | b"^pma", _) => First::Empty,
            _ => return Ok(First::Raw(self.first.clone())),
        };
        if r.remaining() != 0 {
            return Err(Error::Format("an object's first record longer than its class's fields"));
        }
        Ok(out)
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
    (b"Aalr", ABILITY_OWN, "t r"),
    // CAbilityWander (`FUN_6f5ab430`): a timer.
    (b"Awan", ABILITY_OWN, "t"),
    // `FUN_6f42dea0`: three words and a timer.
    (b"Afih", ABILITY_OWN, "www t"),
    (b"Afiu", ABILITY_OWN, "www t"),
    // Those whose class's fields are all their own (CAgent's vf 0x38, `FUN_6f03eda0`).
    // CAlliances: a word (`FUN_6f03edf0`).
    (b"ally", "w", ""),
    (b"Artn", ABILITY_OWN, ""),
    // CUnitAI (`FUN_6f9ce510`): fourteen words.
    (b"AI-U", "wwwwwwwwwwwwww", ""),
    // CTask (`FUN_6f696dd0`): five words; `tskA` (`FUN_6f696ec0`): six.
    (b"task", "wwwww", ""),
    (b"tskA", "wwwwww", ""),
    // CCreepSpot (`FUN_6f9bef80`): two words and a counted list of three words each.
    (b"AIcs", "ww [www]", ""),
    // `FUN_6f9ca630`: two words, four reals, nine words.
    (b"AIts", "ww ffff wwwwwwwww", ""),
    // `FUN_6f649220`: the ability's and two words; `FUN_6f42e9b0`: a real and three words.
    (b"Amil", "wwww wwwwwww t w ww", ""),
    (b"Abli", "wwww wwwwwww t w f www", ""),
    // CAbilityMove (`FUN_6f5fec20`, own `FUN_6f5fec80`: the ability's, then words and
    // reals, three words from build 6000, a word from 6029): two timers.
    (b"Amov", "wwww wwwwwww t w www ff wwwww ff www w", "t t"),
    // CAbilityCreepSleep (`FUN_6f5224c0`, own `FUN_6f5230a0`: the ability's and a real):
    // two timers and a counted list (`FUN_6f3e2ce0` each, a reference).
    (b"ACsp", "wwww wwwwwww t w f", "t t [r]"),
    // CJassFunc (`FUN_6f211180`): a word.
    (b"hdlr", "w", "w"),
    // CMapLocation (`FUN_6f27e7d0`): two reals; CRect (`FUN_6f246870`): four.
    (b"+loc", "w", "ff"),
    (b"+rct", "w", "ffff"),
    // CTriggerAction (`FUN_6f299650`) and a filter (`FUN_6f211140`): a reference.
    (b"+tac", "w", "r"),
    (b"+flt", "w", "r"),
    // CForce (`FUN_6f2462d0`): three references.
    (b"+frc", "w", "rrr"),
    // CTimerWar3 (`FUN_6f246c10`, its own `FUN_6f247300`: a word and two reals): its
    // timer (CAgentTimerExtended, `FUN_6f060be0`: a timer, two shorts, a real), a
    // reference, and the list of event 0x80204.
    (b"+tmr", "w ff", "t ss f r [rw]"),
    // CTriggerWar3 (`FUN_6f299840`, its own `FUN_6f2998a0`: six words): three counted
    // lists (`FUN_6f294ba0`, `FUN_6f294c80`, `FUN_6f294c10`).
    (b"+trg", "wwwwww", "[r] [r] [r]"),
    // Event registrations, each ending with its event's handler list: a unit's
    // (`FUN_6f27ede0`, own `FUN_6f27f140`: five words) four words and a reference, a
    // player's (`FUN_6f27e900`, own `FUN_6f27f1b0`: ten words) a reference, a game's
    // (`FUN_6f27e6a0`) only its list.
    (b"uevt", "wwwww", "wwww r [rw]"),
    (b"pevt", "wwwwwwwwww", "r [rw]"),
    (b"gevt", "wwwww", "[rw]"),
    // Registrations on the event-registration base (`FUN_6f27ef60`: event 0x80202's
    // list): a game state's (`FUN_6f27e630`, own `FUN_6f27f260`) two references and two
    // lists; an alliance's (`FUN_6f27e330`, own `FUN_6f27eff0`) a reference and two
    // lists; a player state's (`FUN_6f27e9a0`, own `FUN_6f27f230`) the player
    // registration's, a reference, and event 0x8020b's list; a timer's (`FUN_6f27ec50`)
    // a reference and event 0x80204's list.
    (b"gfvt", "wwwwww f w", "e r r e e"),
    (b"alvt", "wwwwww f w www", "e r e e"),
    (b"psvt", "wwwwwwwwww w f", "r e r e"),
    (b"tmet", "wwwww", "r e"),
    // Orders (`FUN_6f6960f0`, own `FUN_6f696b40`: six words): three words from build
    // 6000; a targeted one (`FUN_6f696220`, own `FUN_6f696cf0`) two references more.
    (b"+ord", "wwwwww", "www"),
    (b"ordt", "wwwwww ff www wwww ff", "www r r"),
    // Tasks: `FUN_6f696330` (own `FUN_6f696f50`) and `FUN_6f696410` (own
    // `FUN_6f6970b0`), a reference each.
    // The buff base's tail (`FUN_6f42d5f0`: from build 6001 a word and two pairs of
    // words): CAbilityAvenger's own a word more from 6003 (`FUN_6f567cf0`).
    (b"Aven", "wwww wwwwwww t w w", "wwwww"),
    (b"Amim", "wwww wwwwwww t w", "wwwww"),
    // On it (`FUN_6f42e2b0`): a reference and two timers; (`FUN_6f42d2a0`) three; with
    // own fields `FUN_6f42f3e0` (on `FUN_6f42f4b0`: the ability's, four reals, four words,
    // a real; then four words, two reals, four words, two reals).
    (b"Alam", "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"),
    (b"Auns", "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"),
    (b"ACtb", "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"),
    (b"ACtc", "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"),
    (b"AOsh", "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t"),
    (b"Aadm", "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t t"),
    (b"Anhe", "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t t"),
    (b"Ainf", "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t t"),
    // `FUN_6f42e0e0` on that: four timers and a reference (own `FUN_6f42f1b0`: three
    // words, three reals, three words, a real, three words).
    (b"Arep", "wwww wwwwwww t w ffff wwww f wwww ff wwww ff www fff www f www", "wwwww r t t t t t t t r"),
    (b"Arst", "wwww wwwwwww t w ffff wwww f wwww ff wwww ff www fff www f www", "wwwww r t t t t t t t r"),
    // The aura base (`FUN_6f532730`: a counted list of references and a word):
    // CAbilityVulnerable's own a word (`FUN_6f42f5a0`); auras (`FUN_6f42d0c0`, own
    // `FUN_6f42e920`: a word and two reals) a reference, a timer, and from build 6047 a
    // word and two pairs.
    (b"Avul", "wwww wwwwwww t w w", "[r] w"),
    (b"AEar", "wwww wwwwwww t w w ff", "[r] w r t w ww ww"),
    (b"AHad", "wwww wwwwwww t w w ff", "[r] w r t w ww ww"),
    (b"Aarm", "wwww wwwwwww t w w ff", "[r] w r t w ww ww"),
    // Effects on the widget base (`FUN_6f42e650`, own `FUN_6f42f720`: four words): the
    // sprite and a word, a timer, a word, two counted lists of words; `FUN_6f42e6d0`
    // three reals more.
    (b"+EIm", "wwww", "p w t w [w] [w]"),
    (b"+EIP", "wwww", "p w t w [w] [w] fff"),
    // CAbilityUpgrade-like (`FUN_6f42d320`, own `FUN_6f42e970`: two reals more): two
    // references.
    (b"Aupg", "wwww wwwwwww t w ff", "r r"),
    // Records of the agent base alone (vf 0x38 `FUN_6f0562b0`, no lists of events
    // 0xd0000 to 0xd03ff): an acolyte's ring slot (own `FUN_6f4eaef0`: three reals and
    // six words); a rally point (`FUN_6f42e810`, own `FUN_6f42f7f0`) a reference.
    (b"rslt", "!fff wwwwww", ""),
    (b"rlyp", "!w ff wwww", "ww"),
    // CEffectBlight (`FUN_6f490810`, own `FUN_6f490850`): a timer.
    (b"Xbli", "ww fff ww", "t"),
    // Buffs (own on `FUN_6f532960`: the ability's, a word, a real, three words, three
    // reals; then a real and three words, and for a devotion aura from build 6047 a
    // real): the aura base's, an extended timer (two shorts and a real more), for
    // trueshot a counted list of words, and a timer.
    (b"BEar", "wwww wwwwwww t w w f www fff f www", "[r] w t ss f [w] t"),
    (b"BHad", "wwww wwwwwww t w w f www fff f www f", "[r] w t ss f t"),
    // CAbilityBlightRegen (`FUN_6f42d4b0`): a timer; CAbilityAcolyteHarvest
    // (`FUN_6f42cf90`, own `FUN_6f42e8d0`: a real and three words): from build 6000 a
    // timer.
    (b"Ablr", "wwww wwwwwww t w", "t"),
    (b"Aaha", "wwww wwwwwww t w f www", "t"),
    // On CAbilityUpgrade-like's (`FUN_6f42d320`: two references): CAbilityQueue
    // (`FUN_6f42df70`, own `FUN_6f42f030`: two reals, a word, counted lists) two
    // references more; CAbilitySellUnit (`FUN_6f42d3c0`, own `FUN_6f42e970`) twelve
    // slots (a word, three reals, two extended timers, a word) and five words.
    (b"Aque", "wwww wwwwwww t w ff w [w] [www] [w]", "r r r r"),
    (b"Asel", "wwww wwwwwww t w ff", "r r 12(w fff t ss f t ss f w) wwwww"),
    // CAbilityRally (`FUN_6f42dff0`, own `FUN_6f42f120`): a counted list of references.
    (b"ARal", "wwww wwwwwww t w www ff wwww", "[r]"),
    // CAbilityInventory (`FUN_6f5675a0`): counted lists, a word, a reference, and its
    // slots' timers.
    (b"AInv", "wwww wwwwwww t w", "[www] w r [w] [t]"),
    // CAbilityMilitiaConvert (`FUN_6f648660`): a timer.
    (b"Amic", "wwww wwwwwww t w", "t"),
    // CAbilityNeutralDetectAoe (the buff base's tail; own `FUN_6f42ef50`: from build
    // 6037 a word).
    (b"Andt", "wwww wwwwwww t w w", "wwwww"),
    // CAbilityNeutral (`FUN_6f42dd20`): counted references, a reference, a counted
    // list of three words, a reference, and a timer.
    (b"Aneu", "wwww wwwwwww t w", "[r] r [www] r t"),
    // COrderPoint (`FUN_6f6961c0`, own `FUN_6f696bd0`): the order's three words and a
    // reference.
    (b"ord.", "wwwwww ff", "www r"),
    // CGhostImage (`FUN_6f6db1a0`, the agent base alone; own `FUN_6f6db2b0`: three
    // words): its sprite, two shorts, five words, three words, nine words, a short.
    (b"+wgh", "!www", "p ss wwwww www 9w s"),
    // CAbilityAttack (`FUN_6f49bd60`): its fields
    // in order, with counted lists of words, of reals, and of three reals
    // (`FUN_6f491650`), timers, references (`FUN_6f4915d0`), and words.
    (b"Aatk", "wwww wwwwwww t w", "wwwwww [w][w][w][w] [fff] [w][w][w][w] [fff][fff] [f] [fff][fff] fff tttt [w] w [r] ff [f][f] f ttt ff wwww f [www] [t] www fff wwww f"),
    // Creep camps (the agent base alone, no own fields): an indicator
    // (`FUN_6f64f590`) a reference, a short, three reals, two words; the camps' UI
    // (`FUN_6f64f5f0`) a counted list of references.
    (b"+ccp", "!", "r s fff ww"),
    (b"+ccu", "!", "[r]"),
    // `FUN_6f5f5270` on the buff base: a reference, two timers, and from build 6026 a
    // reference (own as `Alam`'s).
    (b"Aens", "wwww wwwwwww t w ffff wwww f wwww ff wwww ff", "wwwww r t t r"),
    // CAbilityGoldMine (`FUN_6f5f54f0` on `FUN_6f4ea3a0`: a reference; own
    // `FUN_6f5f62c0`): counted references (`FUN_6f5b0980`) and a timer.
    (b"Agld", "wwww wwwwwww t w w f wwwww wwww f wwwwww", "r [r] t"),
    // MineShaft (`FUN_6f5f5f20`, the agent base alone; own `FUN_6f5f7ab0`): a timer.
    (b"shft", "!wwww f", "t"),
    // Harvests (`FUN_6f42da60` on `FUN_6f42d950`: a counted list of three words, of
    // timers, six words, a real; then two timers and from build 6028 a third; own on
    // `FUN_6f42ee40`: the ability's and eight words, then two reals and a word):
    // CAbilityHarvestLumber; CAbilityHarvest (`FUN_6f42d8c0`, own `FUN_6f42ec10`) from
    // build 6037 a timer more and three reals more.
    (b"Ahrl", "wwww wwwwwww t w wwww wwww ff w", "[www] [t] wwwwww f t t t"),
    (b"Ahar", "wwww wwwwwww t w wwww wwww ff w fff", "[www] [t] wwwwww f t t t t"),
    // CThreadAI (`FUN_6f9bef30`, own `FUN_6f9bf070`: six words and a real): an extended
    // timer and a word.
    (b"AItr", "wwwwww f", "t ss f w"),
    // CSoundWar3 (`FUN_6f2469a0`): its extended timer, then a flag and a sound's
    // (`FUN_6f359e90`: its file, three reals, six words), or else another flag and a
    // sound's (`FUN_6f359fa0`: its file, a reference, words and reals, a second string,
    // with a flag sixteen words more, and words by the bits of a mask).
    (b"+snd", "w", "t ss f ?(z fff www w)(?(z r w fff w z ?(f ww fff wwwwww f www) m &1(ww) &2(www) &4(www) &8(www) &10(www) &20(w) &40(w) &80(w) &100(w)))"),
    // Build abilities (`FUN_6f567130` on CAbilityUpgrade-like's two references): a count
    // and a short, then each building its two words, three references, four words, and
    // with a flag four words and two reals (own `FUN_6f567ac0`: two reals and three
    // words; `FUN_6f567ec0` three words more).
    (b"AHbu", "wwww wwwwwww t w ff www", "r r w s #(ww r r r www ?(wwww ff))"),
    (b"AUbu", "wwww wwwwwww t w ff www www", "r r w s #(ww r r r www ?(wwww ff))"),
    // CAbilityBuildInProgress (`FUN_6f42d4f0`, own `FUN_6f42ea00`: two reals, three
    // words, seven reals, a real from build 6047): a word, a reference, two timers.
    (b"ABnP", "wwww wwwwwww t w ff www fffffff f", "r r w r t t"),
    // CAbilityCannibalize (`FUN_6f4e9db0` on the buff base: a reference, two timers, two
    // references, a timer; own `FUN_6f4eb120`: `Alam`'s, three words, two from 6011).
    (b"Acan", "wwww wwwwwww t w ffff wwww f wwww ff wwww ff www ww", "wwwww r t t r r t"),
    // CQuestManager (`FUN_6f246820`, own `FUN_6f03edf0`: a word): its quests and its
    // defeat conditions, each a reference.
    (b"+qum", "w", "[r] [r]"),
    // CAbilityBlightedGoldMine (`FUN_6f4e9bb0` on `FUN_6f4ea4b0` and `FUN_6f4ea3a0`: a
    // reference, two more, a timer from build 6037, and a counted list of references;
    // own `FUN_6f4eb0f0` on `FUN_6f4eb6e0`: three words, a real, a word, two reals, two
    // words).
    (b"Abgm", "wwww wwwwwww t w www f w ff ww", "r r r t [r]"),
    // CTimerDialogWar3 (`FUN_6f246ad0`, own `FUN_6f2472c0`: three words and a real): a
    // word, two words (`FUN_6f3b6e90`, `FUN_6f3b6fc0`), a string, and a real.
    (b"+tid", "www f", "w ww z f"),
    // CCompAI (`FUN_6f9ca100`, own `FUN_6f9ca230`: words, reals, and counted lists):
    // two words, a counted list of a reference, a word, a reference, a word, and a
    // reference, a counted list of references, and eight timers.
    (b"AI-1", "wwww f ww f ww f ww f w [wwf] [f] w ff wwf wwf wwf [w] wwfw w ww [w] [w] [wwf] w wwf wwf w wwwf wwf ww", "ww [rwrwr] [r] 8t"),
    // CGameCacheManager (`FUN_6f246380`, own `FUN_6f03edf0`: a word): a string and a
    // word, then its caches (`FUN_6f21acd0`: a name, a word, and its missions,
    // `FUN_6f21b1d0`: a name, five words, and five lists of named values: words,
    // reals, words, units (`FUN_6f21b0b0`: a word, a list of three words, and
    // `FUN_6f224e20`'s fields), and strings).
    (b"gccm", "w", "z w [z w [z wwwww [zw] [zf] [zw] [zw [www] wwwww f w fff w f [ww] fff [w] f s] [zz]]]"),
    // CCaptainAI (`FUN_6f9d5cb0` on `FUN_6f6533a0` and `FUN_6f6be8b0`: a sprite and a
    // word; a word, two shorts, a byte and with it a short-counted list of references
    // and two reals, two bytes, a real; four references, two timers, a counted list of
    // references, four words, then its fields through CSave, `FUN_6f9d41a0`: words by
    // vf 8, reals by vf 4, and two words and a real by vf 0; own `FUN_6f03edf0`: a
    // word).
    (b"AI-C", "w", "p w w ss ?b({r} ff) b b f r r r r t t [r] wwww wwww (wwf) wwwwww fffffff ww ff 19(wwf) 12w"),
    // CCameraWar3 (`FUN_6f362530`, no event lists; own `FUN_6f03ede0`, nothing): a word,
    // a real, a word, a reference, three reals, nine reals (`FUN_6f06b880`), a timer,
    // two words, a counted list of three reals (`FUN_6f35d3f0`), twenty references, a
    // byte, five references, sixteen reals, and four sets of nine reals.
    (b"+cam", "!", "w f w r fff 9f t ww [fff] 20r b 5r 16f 4(9f)"),
    // CGameState (`FUN_6f28aad0`, own `FUN_6f28b190`: three words and a real): ten more
    // event lists; a counted list of counted lists of words; a counted list of words; a
    // size, and its counted tables (a size, a count, with any a name, and that many
    // references and words); four references; a size and a counted list of a word, a
    // reference, and a word (`FUN_6f282260`); a size and a counted list of a reference
    // and two words; a counted list of a word and its counted shorts; three words and a
    // real; two timers; and `FUN_6f9a1230`'s: a word, a counted list of words and a word
    // (`FUN_6f9a0080`), two words, and a counted list of a word, ten reals, a word, a
    // byte, a word, and a string.
    (b"+gam", "www f", "10e [[w]] [w] w [w w ~(z) #(rw)] r r r r w [w r w] w [r w w] [w [s]] w w f w t t w [w] w w w [w ffff ffffff w b w z]"),
    (b"tsk.", "wwwww fff", "r"),
    (b"tskt", "wwwwwwww f", "r"),
    // CPlayerWar3 (`FUN_6f269d70`): the lists of events 0x8020b to 0x80233, 0x80305 to
    // 0x80318, 0x80260 and 0x80261; its name and a byte; its selection
    // (`FUN_6f292c10`: a list, ten control groups' two lists each, references, lists of
    // lists, words); a reference; two counted lists of words; twelve words; seven
    // references; two reals; counted references and words; a word; `FUN_6f246a30`
    // (short-counted lists of eight words and of two, four words); words, a counted list
    // of three words, words, its timer, words, a string, and six words.
    (b"+ply", "w", "63e zb [r] w 20[r] r r [r] [[r]] w [r] r r [[r]] w w w r w r [w] [w] 12w 7r ff [r] [w] w {wfffffff} {ww} wwww wwww [www] www t www z wwwwww"),
];

/// Agents whose class writes its whole second record itself, without the agent's
/// handlers, by kind.
const WHOLE: &[(&[u8; 4], &str)] = &[
    // CGroup (`FUN_6f2464b0`): its units (`FUN_6f28d2b0`), two references, four words.
    (b"+grp", "[r] r r wwww"),
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
/// `FUN_6f654c20`): its sprite, a real, a word, two shorts, an optional list of
/// references with two reals (`FUN_6f064240`), two bytes, a real, and a byte.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Widget {
    pub sprite: Option<Sprite>,
    pub real: f32,
    pub word: u32,
    pub shorts: [u16; 2],
    pub refs: Option<(Vec<Ref>, [f32; 2])>,
    pub bytes: [u8; 2],
    pub last_real: f32,
    pub byte: u8,
}

impl Widget {
    fn read(r: &mut Reader) -> Result<Widget, Error> {
        let mut w = Widget { sprite: Sprite::read(r)?, real: r.f32()?, word: r.u32()?, shorts: [r.u16()?, r.u16()?], ..Default::default() };
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
            b"uref" => Second::UnitRefs { words: words(&mut r)?, handlers: Handlers::read(&mut r)? },
            b"+w3u" => {
                let own = r.u32()?;
                let (handlers, widget) = (Handlers::read(&mut r)?, Widget::read(&mut r)?);
                let list = |r: &mut Reader| -> Result<Vec<(Ref, u32)>, Error> { (0..r.u32()?).map(|_| Ok((Ref::read(r)?, r.u32()?))).collect() };
                let lists = (0..37 + 17).map(|_| list(&mut r)).collect::<Result<_, _>>()?;
                Second::Unit { own, handlers, widget, lists, fields: fields(&mut r, UNIT_FIELDS)? }
            }
            b"+w3d" => {
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
