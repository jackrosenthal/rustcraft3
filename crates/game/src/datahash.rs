//! The keepalive's data modules: hashes of the game's data tables as the original loads
//! them.  The modules are the data store's slots (`FUN_6f06c180(0xd)` +0x10, the
//! keepalive folding each slot's `FUN_6f042da0`); a table's slot keeps
//! `rol(hash, 3)` of the loaded table's own hash (`FUN_6f042d80`), the unit tables'
//! (slot 4, `FUN_6f688f70`) that of four of its SLK files chained (`FUN_6f6a8110`).

use crate::efloat;

/// A cell's part of a table's hash (`FUN_6f1996c0`): a string's characters, an integer's
/// or a real's four bytes, each `b * 0x7a2d ^ b`; an empty cell nothing.  A cell is
/// typed as it is read (`FUN_6f19a070`): empty, else a string unless it starts with a
/// digit, '.', or '-', else a real (`efloat::parse`) if it holds '.', 'd', 'D', 'e', or
/// 'E', else an integer (`atoi`).
fn cell_hash(v: &[u8]) -> u32 {
    let byte = |b: u8| (b as u32).wrapping_mul(0x7a2d) ^ b as u32;
    let word = |w: u32| w.to_le_bytes().iter().fold(0u32, |s, &b| s.wrapping_add(byte(b)));
    match v.first() {
        None => 0,
        Some(&c) if !(c.is_ascii_digit() || c == b'.' || c == b'-') => v.iter().fold(0u32, |s, &b| s.wrapping_add(byte(b))),
        _ if v.iter().any(|c| b".dDeE".contains(c)) => word(efloat::parse(v)),
        _ => word(atoi(v) as u32),
    }
}

/// C's atoi: blanks, a sign, then digits.
fn atoi(v: &[u8]) -> i32 {
    let mut s = v.iter().copied().skip_while(|c| c.is_ascii_whitespace()).peekable();
    let neg = match s.peek() {
        Some(b'-') => {
            s.next();
            true
        }
        Some(b'+') => {
            s.next();
            false
        }
        _ => false,
    };
    let n = s.take_while(u8::is_ascii_digit).fold(0i32, |n, c| n.wrapping_mul(10).wrapping_add((c - b'0') as i32));
    if neg { n.wrapping_neg() } else { n }
}

/// A table read as the original reads an SLK file (CSLKDatabase, `FUN_6f198a20`): lines
/// to a CR or LF, the first "ID;PWXL;N;E" or nothing is read; then records up to 'E',
/// each its kind and ';' (else the rest is dropped): 'B' sizes the table (its Y rows,
/// its X columns), 'C' and 'F' move to their X and Y (1-based), and a 'C' record's K
/// stores its value there (`FUN_6f19a320`: after a '"' up to the next, else to the
/// line's end).  At most 96 columns.
pub struct Slk<'a> {
    pub rows: usize,
    pub cols: usize,
    cells: std::collections::HashMap<(usize, usize), &'a [u8]>,
}

impl<'a> Slk<'a> {
    pub fn parse(text: &'a [u8]) -> Slk<'a> {
        let mut t = Slk { rows: 0, cols: 0, cells: Default::default() };
        // `FUN_6f19a450`: a line ends at a CR or LF, and a CR then an LF after it are
        // passed over.
        let mut lines = std::iter::from_fn({
            let mut rest = text;
            move || {
                if rest.is_empty() {
                    return None;
                }
                let end = rest.iter().position(|&c| c == b'\r' || c == b'\n').unwrap_or(rest.len());
                let line = &rest[..end];
                let mut next = (end + 1).min(rest.len());
                if rest.get(next) == Some(&b'\r') {
                    next += 1;
                }
                if rest.get(next) == Some(&b'\n') {
                    next += 1;
                }
                rest = &rest[next..];
                Some(line)
            }
        });
        if lines.next() != Some(&b"ID;PWXL;N;E"[..]) {
            return t;
        }
        let (mut x, mut y) = (-1i64, -1i64);
        let num = |f: &[u8]| atoi(f) as i64 - 1;
        for line in lines {
            match line.first().copied().unwrap_or(0) {
                b'E' => break,
                b'N' => continue,
                _ if line.get(1) != Some(&b';') => break,
                kind @ (b'B' | b'C' | b'F') => {
                    let mut rest = &line[2..];
                    let (mut bx, mut by) = (None, None);
                    while let Some(&f) = rest.first() {
                        if kind == b'C' && f == b'K' {
                            let v = &rest[1..];
                            let v = v.strip_prefix(b"\"").unwrap_or(v);
                            let end = v.iter().position(|&c| c == b'"').unwrap_or(v.len());
                            if x >= 0 && y >= 0 && x < 96 {
                                t.cells.insert((y as usize, x as usize), &v[..end]);
                            }
                            // Past a closing quote the record's fields go on.
                            rest = v.get(end + 1..).unwrap_or(&[]);
                            rest = rest.strip_prefix(b";").unwrap_or(rest);
                            continue;
                        }
                        let end = rest.iter().position(|&c| c == b';').unwrap_or(rest.len());
                        match f {
                            b'X' if kind == b'B' => bx = Some(num(&rest[1..end])),
                            b'Y' if kind == b'B' => by = Some(num(&rest[1..end])),
                            b'X' => x = num(&rest[1..end]),
                            b'Y' => y = num(&rest[1..end]),
                            _ => {}
                        }
                        rest = rest.get(end + 1..).unwrap_or(&[]);
                    }
                    if kind == b'B' {
                        // `FUN_6f19b1d0`: its X and Y are kept as the current cell too.
                        if let Some(bx) = bx {
                            x = bx;
                            t.cols = t.cols.max((bx + 1).max(0) as usize).min(96);
                        }
                        if let Some(by) = by {
                            y = by;
                            t.rows = t.rows.max((by + 1).max(0) as usize);
                        }
                    }
                }
                _ => {}
            }
        }
        t
    }

    /// A cell's text, empty when none was stored.
    pub fn cell(&self, row: usize, col: usize) -> &'a [u8] {
        self.cells.get(&(row, col)).copied().unwrap_or(&[])
    }

    /// The table's hash (`FUN_6f1996c0`): every cell's within its size.
    pub fn hash(&self) -> u32 {
        self.cells.iter().filter(|((r, c), _)| *r < self.rows && *c < self.cols).fold(0u32, |s, (_, v)| s.wrapping_add(cell_hash(v)))
    }
}

/// The unit tables' hash (CUnitDatabase, `FUN_6f6a8110`): UnitData's, then
/// UnitAbilities', UnitWeapons', and UnitBalance's, each `h = rol(h, 3) ^ next`, the
/// whole rotated once more.  (unitUI is loaded with them but not hashed.)
pub fn unit_tables(data: &[u8], abilities: &[u8], weapons: &[u8], balance: &[u8]) -> u32 {
    [abilities, weapons, balance].iter().fold(Slk::parse(data).hash(), |h, t| h.rotate_left(3) ^ Slk::parse(t).hash()).rotate_left(3)
}

/// A table's slot's module value: `rol(0 ^ hash, 3)`.
pub fn slot(hash: u32) -> u32 {
    hash.rotate_left(3)
}

/// A meta table's fields whose strings are left out of the object data's hash: those
/// with a nonzero stringExt (CObjectMetaDB's column 12, `FUN_6f6e8540`), by id.
pub fn string_ext(meta: &Slk) -> std::collections::HashSet<[u8; 4]> {
    let Some(col) = (0..meta.cols).find(|&c| meta.cell(0, c) == b"stringExt") else { return Default::default() };
    (1..meta.rows).filter(|&r| atoi(meta.cell(r, col)) != 0).filter_map(|r| meta.cell(r, 0).try_into().ok()).collect()
}

/// A map's object modification file's hash (`FUN_6f6e50d0`): its objects, the original
/// ones modified then the new ones, each `h = rol(object ^ h, 3)`.  An object
/// (`FUN_6f6fb170`): `rol(rol(old, 3) ^ new, 3)`, then each modification `u = rol(u ^
/// m, 3)`.  A modification (`FUN_6f6e8790`): `rol(rol(field, 3) ^ type, 3)`, with its
/// level and data column in turn for the kinds that have them (`leveled`: abilities,
/// upgrades, doodads from version 2), then its value: an integer's or a real's bits
/// (types 0 and 1), an unreal's whole part (2, `efloat::to_int`), a string's hash (3,
/// `keepalive::payload_hash`, but for fields in `skip`).  Ids are the file's four
/// characters as a little-endian word.  None when the file does not read.  Read: a
/// custom map's units (83 objects, 2011 modifications) and abilities, each module as
/// the original's.
pub fn object_data(data: &[u8], leveled: Option<bool>, skip: &std::collections::HashSet<[u8; 4]>) -> Option<u32> {
    struct Rd<'a>(&'a [u8]);
    impl<'a> Rd<'a> {
        fn take(&mut self, n: usize) -> Option<&'a [u8]> {
            let (b, rest) = (self.0.get(..n)?, self.0.get(n..)?);
            self.0 = rest;
            Some(b)
        }
        fn word(&mut self) -> Option<u32> {
            self.take(4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        }
        fn cstr(&mut self) -> Option<&'a [u8]> {
            let end = self.0.iter().position(|&c| c == 0)?;
            let s = self.take(end)?;
            self.take(1)?;
            Some(s)
        }
    }
    let mut r = Rd(data);
    let version = r.word()?;
    let leveled = leveled.unwrap_or(version >= 2);
    let mut h = 0u32;
    for _list in 0..2 {
        let Some(n) = r.word() else { break };
        for _ in 0..n {
            let (old, new) = (r.word()?, r.word()?);
            let mut o = (old.rotate_left(3) ^ new).rotate_left(3);
            for _ in 0..r.word()? {
                let field: [u8; 4] = r.take(4)?.try_into().ok()?;
                let ty = r.word()?;
                let mut m = (u32::from_le_bytes(field).rotate_left(3) ^ ty).rotate_left(3);
                if leveled {
                    let (level, column) = (r.word()?, r.word()?);
                    m = ((m ^ level).rotate_left(3) ^ column).rotate_left(3);
                }
                m = match ty {
                    0 | 1 => (m ^ r.word()?).rotate_left(3),
                    2 => (m ^ efloat::to_int(r.word()?) as u32).rotate_left(3),
                    _ => {
                        let text = r.cstr()?;
                        if skip.contains(&field) { m } else { (m ^ crate::keepalive::payload_hash(text)).rotate_left(3) }
                    }
                };
                r.word()?;
                o = (o ^ m).rotate_left(3);
            }
            h = (o ^ h).rotate_left(3);
        }
    }
    Some(h)
}

/// The data modules from the tables as the game's archives give them (the map's own
/// over the data set's; `read` by path).  The data tables': 4 the unit tables
/// (`FUN_6f688f70`), 5 AbilityData (`FUN_6f6ac4f0`), 7 ItemData (`FUN_6f658a20`: hashed
/// in a table of its own first, `rol` once more), 9 UpgradeData (`FUN_6f6ae8d0`).  The
/// object data's, one per kind (CCustomData, `FUN_6f69fba0`; its vf0 `FUN_6f6a80e0`),
/// for a map without object data of its own each its kind's meta table's hash
/// (CObjectMetaDB, `FUN_6f6dd470`, hashed by `FUN_6f6e5150`): 12 units and 13 items
/// (both UnitMetaData), 14 destructables, 16 abilities, 17 buffs, 18 upgrades (15, the
/// doodads', is not made in a game; 19 is 0).  Read: Booty Bay's, each as the original's.
pub fn modules(read: impl Fn(&str) -> Vec<u8>) -> [(u8, u32); 11] {
    let table = |n: &str| Slk::parse(&read(n)).hash();
    let units = unit_tables(&read("Units\\UnitData.slk"), &read("Units\\UnitAbilities.slk"), &read("Units\\UnitWeapons.slk"), &read("Units\\UnitBalance.slk"));
    // A kind's module: its meta table's hash, with the map's object file folded in.
    let custom = |meta: &str, file: &str, leveled: Option<bool>| {
        let meta = read(meta);
        let meta = Slk::parse(&meta);
        let data = read(file);
        match (data.is_empty(), object_data(&data, leveled, &string_ext(&meta))) {
            (false, Some(o)) => (o ^ meta.hash()).rotate_left(3),
            _ => meta.hash(),
        }
    };
    [
        (4, slot(units)),
        (5, slot(table("Units\\AbilityData.slk"))),
        (7, slot(slot(table("Units\\ItemData.slk")))),
        (9, slot(table("Units\\UpgradeData.slk"))),
        (12, custom("Units\\UnitMetaData.slk", "war3map.w3u", Some(false))),
        (13, custom("Units\\UnitMetaData.slk", "war3map.w3t", Some(false))),
        (14, custom("Units\\DestructableMetaData.slk", "war3map.w3b", Some(false))),
        (16, custom("Units\\AbilityMetaData.slk", "war3map.w3a", Some(true))),
        (17, custom("Units\\AbilityBuffMetaData.slk", "war3map.w3h", Some(false))),
        (18, custom("Units\\UpgradeMetaData.slk", "war3map.w3q", Some(true))),
        (19, 0),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Cells typed and hashed as the original's: a string by its characters, an
    /// integer and a real by their bytes, an empty cell and a cell past the table's
    /// size not at all.
    #[test]
    fn cells_hash_as_the_originals() {
        let t = Slk::parse(b"ID;PWXL;N;E\r\nB;X2;Y2;D0\r\nC;X1;Y1;K\"ab\"\r\nC;X2;K12\r\nC;X1;Y2;K0.5\r\nC;X3;K\"past\"\r\nE\r\n");
        assert_eq!((t.rows, t.cols), (2, 2));
        let byte = |b: u32| b.wrapping_mul(0x7a2d) ^ b;
        let half = 0.5f32.to_bits().to_le_bytes();
        let want = byte(b'a' as u32) + byte(b'b' as u32) + byte(12) + half.iter().map(|&b| byte(b as u32)).sum::<u32>();
        assert_eq!(t.hash(), want);
        assert_eq!(Slk::parse(b"ID;P\r\nB;X1;Y1\r\nC;X1;Y1;K1\r\n").hash(), 0);
    }
}
