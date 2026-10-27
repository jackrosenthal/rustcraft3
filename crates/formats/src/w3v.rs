//! The original's saved game caches (.w3v; SaveGameCache writes every saved cache into
//! a profile's one `Campaigns.w3v`) in the recorded-game container (`crate::container`,
//! blocks of 2048 bytes, product W3XP or WAR3, version 27, build 6059; `FUN_6f246fe0`).
//! The record (`FUN_6f21acd0`, `FUN_6f21b1d0`, `FUN_6f224ca0`): a word, then the caches
//! (a count, each its name, a word, and its missions: a count, each its name, five
//! words, then its integers, reals, booleans, units, and strings, each list a count and
//! its entries by name).  The words are counters of the tables' (each table's +0x28: 1
//! for one cache, 3 for its three missions, 0 for each mission's five tables in the
//! sample, though they held entries; a sync that adds a key raises it, 0x70's removal
//! lowers it).

use crate::{Error, Reader};

/// The container's fields for a cache file: the product W3XP, or WAR3 for Reign of
/// Chaos (`FUN_6f32ba20`), version 27, build 6059 (the length field is never set on
/// this path: the sample holds what was left in the object).
pub fn header(expansion: bool) -> crate::container::Header {
    crate::container::Header { product: if expansion { *b"PX3W" } else { *b"3RAW" }, version: 27, build: 6059, flags: 0, length_ms: 0 }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Caches {
    pub counter: u32,
    pub caches: Vec<Cache>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Cache {
    pub name: String,
    pub counter: u32,
    pub missions: Vec<Mission>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Mission {
    pub name: String,
    pub counters: [u32; 5],
    pub ints: Vec<(String, i32)>,
    pub reals: Vec<(String, f32)>,
    pub bools: Vec<(String, u32)>,
    /// Each unit's record as the original writes it (`unit_record_len`).
    pub units: Vec<(String, Vec<u8>)>,
    pub strings: Vec<(String, String)>,
}

/// The length of a stored unit's record (`FUN_6f21b560`, `FUN_6f224e20`, which
/// SyncStoredUnit's action shares): its type; a count of item slots and each slot's type,
/// charges, and a third word; then its hero data: five words, a real, a word, three
/// reals, a word, a real, a count of abilities and each one's two words, three reals,
/// a count of words and the words, a real, and a 16-bit mask.
pub fn unit_record_len(b: &[u8]) -> Option<usize> {
    let mut r = Reader::new(b);
    let words = |r: &mut Reader, n: usize| r.skip(n * 4).ok();
    let count = |r: &mut Reader| r.u32().ok().map(|n| n as usize);
    words(&mut r, 1)?;
    let items = count(&mut r)?;
    words(&mut r, items * 3)?;
    words(&mut r, 12)?;
    let abilities = count(&mut r)?;
    words(&mut r, abilities * 2 + 3)?;
    let n = count(&mut r)?;
    words(&mut r, n + 1)?;
    r.skip(2).ok()?;
    Some(r.pos)
}

impl Caches {
    pub fn parse(file: &[u8]) -> Result<Caches, Error> {
        let (_, data) = crate::container::unpack(file)?;
        let mut r = Reader::new(&data);
        let counter = r.u32()?;
        let mut caches = Vec::new();
        for _ in 0..r.u32()? {
            let (name, counter) = (r.cstr()?, r.u32()?);
            let mut missions = Vec::new();
            for _ in 0..r.u32()? {
                let mut m = Mission { name: r.cstr()?, ..Default::default() };
                for c in &mut m.counters {
                    *c = r.u32()?;
                }
                for _ in 0..r.u32()? {
                    m.ints.push((r.cstr()?, r.i32()?));
                }
                for _ in 0..r.u32()? {
                    m.reals.push((r.cstr()?, r.f32()?));
                }
                for _ in 0..r.u32()? {
                    m.bools.push((r.cstr()?, r.u32()?));
                }
                for _ in 0..r.u32()? {
                    let key = r.cstr()?;
                    let n = unit_record_len(&r.data[r.pos..]).ok_or(Error::Eof)?;
                    m.units.push((key, r.bytes(n)?.to_vec()));
                }
                for _ in 0..r.u32()? {
                    m.strings.push((r.cstr()?, r.cstr()?));
                }
                missions.push(m);
            }
            caches.push(Cache { name, counter, missions });
        }
        Ok(Caches { counter, caches })
    }

    /// The record, uncompressed.
    pub fn data(&self) -> Vec<u8> {
        let mut d = Vec::new();
        let u32 = |d: &mut Vec<u8>, v: u32| d.extend(v.to_le_bytes());
        let cstr = |d: &mut Vec<u8>, s: &str| {
            d.extend(s.as_bytes());
            d.push(0);
        };
        u32(&mut d, self.counter);
        u32(&mut d, self.caches.len() as u32);
        for c in &self.caches {
            cstr(&mut d, &c.name);
            u32(&mut d, c.counter);
            u32(&mut d, c.missions.len() as u32);
            for m in &c.missions {
                cstr(&mut d, &m.name);
                for &v in &m.counters {
                    u32(&mut d, v);
                }
                u32(&mut d, m.ints.len() as u32);
                for (k, v) in &m.ints {
                    cstr(&mut d, k);
                    u32(&mut d, *v as u32);
                }
                u32(&mut d, m.reals.len() as u32);
                for (k, v) in &m.reals {
                    cstr(&mut d, k);
                    u32(&mut d, v.to_bits());
                }
                u32(&mut d, m.bools.len() as u32);
                for (k, v) in &m.bools {
                    cstr(&mut d, k);
                    u32(&mut d, *v);
                }
                u32(&mut d, m.units.len() as u32);
                for (k, v) in &m.units {
                    cstr(&mut d, k);
                    d.extend(v);
                }
                u32(&mut d, m.strings.len() as u32);
                for (k, v) in &m.strings {
                    cstr(&mut d, k);
                    cstr(&mut d, v);
                }
            }
        }
        d
    }

    pub fn to_bytes(&self, expansion: bool) -> Vec<u8> {
        crate::container::pack(&header(expansion), &self.data(), 2048)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A cache the original saved (`docs/reference/cache_sample.w3v`, a test map storing
    /// each kind): read, and its record written back the same.
    #[test]
    fn the_originals_cache_reads_and_writes_back() {
        let file = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/reference/cache_sample.w3v")).unwrap();
        let c = Caches::parse(&file).unwrap();
        assert_eq!(c.counter, 1);
        let t = &c.caches[0];
        assert_eq!((t.name.as_str(), t.counter, t.missions.len()), ("TestCache.w3v", 3, 3));
        assert_eq!(t.missions[0].ints, vec![("Count".into(), 42), ("Neg".into(), -7)]);
        assert_eq!(t.missions[0].reals, vec![("Ratio".into(), 1.5)]);
        assert_eq!(t.missions[1].bools, vec![("Done".into(), 1)]);
        assert_eq!(t.missions[1].strings, vec![("Name".into(), "hello".into())]);
        assert_eq!(&t.missions[2].units[0].1[..4], b"lapH");
        let (_, data) = crate::container::unpack(&file).unwrap();
        assert_eq!(c.data(), data);
        let again = Caches::parse(&c.to_bytes(true)).unwrap();
        assert_eq!(again, c);
    }
}
