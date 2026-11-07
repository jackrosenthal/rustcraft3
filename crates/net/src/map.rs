//! The map checksums a game is announced and checked with: the map file's size and
//! CRC-32, and two checksums over the game's scripts and the map's game files, which
//! tell whether two computers would play the same game.

/// The map's files the game checksums cover, in order; the script may also be under
/// `scripts\`.
pub const GAME_FILES: [&str; 9] = ["war3map.j", "war3map.w3e", "war3map.wpm", "war3map.doo", "war3map.w3u", "war3map.w3b", "war3map.w3d", "war3map.w3a", "war3map.w3q"];

const MAGIC: u32 = 0x03F1379E;

/// What MAPCHECK and the stat string carry about a map.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Checksums {
    pub size: u32,
    /// CRC-32 of the map file.
    pub crc32: u32,
    /// Each 32-bit word xored in and rotated left 3, over common.j, blizzard.j, a
    /// constant, and the map's game files.
    pub xoro: u32,
    pub sha1: [u8; 20],
}

fn xoro(data: &[u8]) -> u32 {
    let mut v = 0u32;
    let (words, rest) = data.as_chunks::<4>();
    for w in words {
        v = (v ^ u32::from_le_bytes(*w)).rotate_left(3);
    }
    for &b in rest {
        v = (v ^ b as u32).rotate_left(3);
    }
    v
}

impl Checksums {
    /// The checksums of a map file, given the game's `common.j` and `blizzard.j` and a way
    /// to read the map's own files by name.
    pub fn compute(map_file: &[u8], common_j: &[u8], blizzard_j: &[u8], mut read: impl FnMut(&str) -> Option<Vec<u8>>) -> Checksums {
        let mut sha = sha1_smol::Sha1::new();
        sha.update(common_j);
        sha.update(blizzard_j);
        sha.update(&MAGIC.to_le_bytes());
        let mut v = (xoro(common_j) ^ xoro(blizzard_j)).rotate_left(3);
        v = (v ^ MAGIC).rotate_left(3);
        for (k, name) in GAME_FILES.iter().enumerate() {
            let data = if k == 0 { read(name).or_else(|| read("scripts\\war3map.j")) } else { read(name) };
            if let Some(d) = data {
                sha.update(&d);
                v = (v ^ xoro(&d)).rotate_left(3);
            }
        }
        Checksums { size: map_file.len() as u32, crc32: crc32fast::hash(map_file), xoro: v, sha1: sha.digest().bytes() }
    }
}

/// The public key game.dll checks maps' strong signatures with (`FUN_6f78b640`: its
/// modulus at 0x6fb879e8, little-endian, and its exponent 0x10001 at 0x6fb87ae8).
const SIGNING_KEY: [u8; 256] = [
    0xe9, 0xf8, 0xfe, 0x07, 0xe8, 0xc5, 0xa6, 0xc2, 0x7a, 0x17, 0x5d, 0xa7, 0x41, 0x64, 0x8e, 0x56, 0x0b, 0x04, 0x4a, 0x11, 0xa7, 0xda, 0x9f, 0x02, 0x40, 0x8b, 0x06, 0xfc, 0x23, 0x9e, 0x73, 0x53,
    0x90, 0x4f, 0x83, 0x0d, 0xbb, 0x86, 0x8e, 0x01, 0x4f, 0xff, 0x65, 0x08, 0x24, 0x89, 0xe8, 0xe1, 0x85, 0x9b, 0xae, 0x47, 0xc4, 0x97, 0xa1, 0xbc, 0x99, 0x47, 0x44, 0xac, 0x99, 0x4b, 0x57, 0xed,
    0xa2, 0x70, 0x64, 0xd8, 0x69, 0xe9, 0x06, 0xf7, 0x06, 0x8b, 0xbf, 0x91, 0x88, 0x12, 0xc8, 0x3b, 0x29, 0x6b, 0xa6, 0x2b, 0x51, 0x55, 0x2d, 0xd2, 0xce, 0x0a, 0xd6, 0x25, 0xcd, 0x49, 0x84, 0x14,
    0x80, 0xd2, 0xa0, 0x3d, 0x89, 0x0a, 0xe1, 0x95, 0x55, 0xed, 0x8d, 0x89, 0xee, 0x7e, 0x7d, 0x4e, 0x16, 0x30, 0xd7, 0x11, 0x9b, 0xe3, 0x29, 0xe1, 0x70, 0x39, 0xeb, 0xde, 0xc5, 0xcc, 0x69, 0x40,
    0x1b, 0xfb, 0xa5, 0x86, 0xca, 0x9b, 0xba, 0x6a, 0xf5, 0xa8, 0x0a, 0xa3, 0xdb, 0x56, 0x3a, 0xfa, 0xd6, 0x34, 0xac, 0x1f, 0x44, 0x7c, 0x39, 0xc9, 0x5b, 0x25, 0xee, 0xc0, 0x25, 0xe6, 0xee, 0x6f,
    0x52, 0x71, 0x82, 0x07, 0xf7, 0xe9, 0xfb, 0x1e, 0x8a, 0x09, 0x59, 0x68, 0x5b, 0x40, 0xa7, 0x93, 0x72, 0xa0, 0xd8, 0xc8, 0x4e, 0x86, 0x83, 0x60, 0x63, 0x4c, 0x6e, 0x5f, 0xdd, 0xc7, 0x03, 0xdf,
    0x88, 0x77, 0xb7, 0xd9, 0xf1, 0xfa, 0x08, 0x57, 0x9b, 0xd4, 0x1e, 0x1b, 0x85, 0x5a, 0x5b, 0xc1, 0xb2, 0xf8, 0x9a, 0xa1, 0xfa, 0xad, 0x0d, 0xe7, 0x36, 0x24, 0xbd, 0x0b, 0x82, 0x5b, 0xd2, 0xec,
    0x27, 0xd7, 0x24, 0xe5, 0xe9, 0x62, 0x7e, 0x40, 0x8e, 0x81, 0xfa, 0x83, 0xb2, 0x73, 0x95, 0x23, 0xcb, 0x79, 0x81, 0x46, 0x81, 0x33, 0x8b, 0xe3, 0x4b, 0xdd, 0x10, 0x45, 0x95, 0x24, 0x1c, 0xd4,
];

/// Whether a map is Blizzard's: it ends in a strong signature ('NGIS' and 256 bytes) of
/// the file before it and its name in capitals (`name`, the file's alone), as game.dll
/// checks one (`FUN_6f8e9d30`): the signature, a little-endian number, to the key's
/// exponent is the SHA-1 then bytes 0xbb, the last 0x0b.  A Blizzard map renamed is not
/// one (read: the original's game played alone of Booty Bay and Glacial Thaw are of type
/// 9, of a signed copy of Booty Bay renamed bbw4.w3x 1).
pub fn blizzard_signed(file: &[u8], name: &str) -> bool {
    let Some(at) = file.len().checked_sub(260) else { return false };
    if &file[at..at + 4] != b"NGIS" {
        return false;
    }
    let mut sha = sha1_smol::Sha1::new();
    sha.update(&file[..at]);
    sha.update(name.to_ascii_uppercase().as_bytes());
    let mut want = [0xbbu8; 256];
    want[..20].copy_from_slice(&sha.digest().bytes());
    want[255] = 0x0b;
    let limbs = |b: &[u8]| b.chunks(4).map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect::<Vec<u32>>();
    let (n, sig) = (limbs(&SIGNING_KEY), limbs(&file[at + 4..]));
    // The signature to the 65537th power, by squaring sixteen times and one product.
    let mut m = sig.clone();
    for _ in 0..16 {
        m = mul_mod(&m, &m, &n);
    }
    m = mul_mod(&m, &sig, &n);
    m.iter().flat_map(|w| w.to_le_bytes()).eq(want)
}

/// `a` times `b` modulo `n`, numbers as little-endian 32-bit words, `n`'s count of them.
fn mul_mod(a: &[u32], b: &[u32], n: &[u32]) -> Vec<u32> {
    let mut p = vec![0u32; a.len() + b.len()];
    for (i, &x) in a.iter().enumerate() {
        let mut carry = 0u64;
        for (j, &y) in b.iter().enumerate() {
            let t = p[i + j] as u64 + x as u64 * y as u64 + carry;
            p[i + j] = t as u32;
            carry = t >> 32;
        }
        p[i + b.len()] = carry as u32;
    }
    // The remainder a bit at a time, from the top.
    let mut r = vec![0u32; n.len() + 1];
    for bit in (0..p.len() * 32).rev() {
        let mut carry = (p[bit / 32] >> (bit % 32)) & 1;
        for w in r.iter_mut() {
            let next = *w >> 31;
            *w = *w << 1 | carry;
            carry = next;
        }
        let at_least = (0..r.len()).rev().map(|k| (r[k], n.get(k).copied().unwrap_or(0))).find(|(x, y)| x != y).is_none_or(|(x, y)| x > y);
        if at_least {
            let mut borrow = 0i64;
            for (k, w) in r.iter_mut().enumerate() {
                let t = *w as i64 - n.get(k).copied().unwrap_or(0) as i64 - borrow;
                *w = t as u32;
                borrow = (t < 0) as i64;
            }
        }
    }
    r.truncate(n.len());
    r
}
