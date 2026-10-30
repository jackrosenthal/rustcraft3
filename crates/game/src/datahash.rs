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

/// The data tables' modules from their files as the game's archives give them (the
/// map's own over the data set's; `read` by path): 4 the unit tables (`FUN_6f688f70`),
/// 5 AbilityData (`FUN_6f6ac4f0`), 7 ItemData (`FUN_6f658a20`: hashed in a table of its
/// own first, `rol` once more), 9 UpgradeData (`FUN_6f6ae8d0`).  Read: Booty Bay's,
/// each as the original's.
pub fn modules(read: impl Fn(&str) -> Vec<u8>) -> [(u8, u32); 4] {
    let table = |n: &str| Slk::parse(&read(n)).hash();
    let units = unit_tables(&read("Units\\UnitData.slk"), &read("Units\\UnitAbilities.slk"), &read("Units\\UnitWeapons.slk"), &read("Units\\UnitBalance.slk"));
    [(4, slot(units)), (5, slot(table("Units\\AbilityData.slk"))), (7, slot(slot(table("Units\\ItemData.slk")))), (9, slot(table("Units\\UpgradeData.slk")))]
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
