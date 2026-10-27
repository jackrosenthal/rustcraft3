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
