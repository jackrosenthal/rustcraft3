//! Saved games (.w3z), as the original writes them (`FUN_6f1e8630`): the recorded-game
//! container (`container`), its record a data store of little-endian bytes, words,
//! dwords, and NUL-terminated strings (`FUN_6f06b170`, `FUN_6f06b220`, `FUN_6f06b3a0`,
//! `FUN_6f06bc20`).  Read so far: the header and the object manager's layout (its
//! tables, classes, clocks, and data sections, each kept whole); the game's own state
//! after it is kept as it is.

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
    /// The rest of the record (the game's own state), not yet read.
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
