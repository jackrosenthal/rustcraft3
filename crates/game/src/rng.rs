//! The game's random number generator (game.dll 1.27b FUN_6f1b7130, seeded by
//! FUN_6f1cc4e0), used by GetRandomInt, GetRandomReal, and the simulation.

/// 256-byte table at 0x6fa92f10, read as little-endian words at byte offsets.
const TABLE: [u32; 64] = [
    0x9927148E, 0x08C7AAFD, 0x1F3EE6D5, 0xDA55BBF6, 0x6A4AA075, 0xFF97BDE8, 0x9FBC9BDE, 0x46A18A81,
    0x63E30B6E, 0x5D6C7A76, 0xCA69D388, 0x25B947C3, 0x3FA2AB83, 0xBA7C41A6, 0x0195ACE5, 0xC109CF7E,
    0x717062D9, 0x0205DB8D, 0x54EF8724, 0x3037D4C6, 0x7BCB1BD0, 0xECD8E4B8, 0xDCADCE49, 0xC494A913,
    0x0DAE398F, 0x0EDD5218, 0x85F5FA78, 0x6DAFD258, 0x3B53B2A4, 0xBE50A551, 0x11F42DFC, 0xF1169848,
    0x663DDF86, 0x2F2E445E, 0x176B0736, 0xB64C298B, 0xE75F89E2, 0xE121A7CD, 0xED65C94D, 0x239CEEFE,
    0x04B77D33, 0x402A9A9E, 0xF35B10B3, 0x921C7782, 0x571E4E20, 0x8C067222, 0xFB732C67, 0xBF0AC259,
    0x0CF95C79, 0x68121A28, 0x42193474, 0xF884C0B1, 0x9D15F038, 0x6F3AF260, 0x91EB90B4, 0x61357F1D,
    0x5603325A, 0x932BC5A3, 0x434B0F80, 0x3CE0A8F7, 0x2664D196, 0x4FCC45D7, 0xB5E9B0C8, 0xEA31D600,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Rng {
    value: u32,
    /// Four byte offsets into the table.
    taps: u32,
}

fn word(offset: u32) -> u32 {
    TABLE[(offset / 4) as usize & 63]
}

impl Rng {
    /// The generator's state, for checksums.
    pub fn state(&self) -> u64 {
        (self.value as u64) << 32 | self.taps as u64
    }

    pub fn new(seed: u32) -> Rng {
        let taps = (seed % 0x3b).wrapping_mul(0x400)
            | (seed % 0x3d).wrapping_mul(4)
            | (seed % 0x35).wrapping_mul(0x40000)
            | (seed / 0x2f).wrapping_mul(0x11).wrapping_add(seed).wrapping_mul(0x400_0000);
        Rng { value: seed, taps }
    }

    /// The seed a game starts from (FUN_6f29e300): the setup's, or 'war3' when the map
    /// locks its random seed (MAP_LOCK_RANDOM_SEED).
    pub fn game_seed(seed: u32, map_flags: i32) -> u32 {
        if map_flags & 0x8000 != 0 { 0x7761_7233 } else { seed }
    }

    pub fn next_u32(&mut self) -> u32 {
        let t = self.taps;
        let b2 = (t >> 16) & 0xff;
        let b1 = (t >> 8) & 0xff;
        let b0 = t & 0xff;
        let b3 = t >> 24;
        let i3 = if b3 < 4 { b3 + 0xb8 } else { b3 - 4 };
        let i2 = if b2 < 0xc { b2 + 200 } else { b2 - 0xc };
        let i1 = if b1 < 0x18 { b1 + 0xd4 } else { b1 - 0x18 };
        let i0 = if b0 < 0x1c { b0 + 0xd8 } else { b0 - 0x1c };
        self.value = self.value.wrapping_add(
            word(i1).rotate_left(3) ^ word(i2).rotate_left(2) ^ word(i0) ^ word(i3).rotate_left(1),
        );
        self.taps = (i3 << 24) | (i2 << 16) | (i1 << 8) | i0;
        self.value
    }

    /// GetRandomInt (game.dll 0x6f201e30), including its handling of a maximum below the
    /// minimum.
    /// A uniform real in [lo, hi), built from the generator's mantissa bits as
    /// GetRandomReal does.
    pub fn real(&mut self, lo: f32, hi: f32) -> f32 {
        if (hi - lo).abs() < f32::EPSILON {
            return lo;
        }
        let (lo, hi) = if hi < lo { (hi, lo) } else { (lo, hi) };
        let r = f32::from_bits((self.next_u32() & 0x7f_ffff) | 0x3f80_0000) - 1.0;
        lo + r * (hi - lo)
    }

    pub fn int(&mut self, lo: i32, hi: i32) -> i32 {
        if lo == hi {
            return lo;
        }
        let span = if hi < lo { lo.wrapping_sub(hi) } else { hi.wrapping_sub(lo) };
        let r = self.next_u32() as u64;
        lo.wrapping_add(((r * (span as u32 as u64).wrapping_add(1)) >> 32) as i32)
    }
}

/// The original's CRandData (game.dll FUN_6f65c7c0): 45 generators beside the main one,
/// each drawn on by one of the game's purposes through FUN_6f693660 (an integer below a
/// bound), FUN_6f6936a0 (a real), or FUN_6f695c70 (dice), by its index.  They are seeded
/// together from one value (FUN_6f693710): a generator seeded with it gives each its
/// seed in turn.  Measured under Wine: with the game's seed 0x35000000, 42 of the 45
/// were as so seeded at the first turn of Booty Bay, and the other three as many draws
/// on.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct RandData {
    pub streams: Vec<Rng>,
}

impl Default for RandData {
    /// As constructed, before the game seeds them: each seeded with 0.
    fn default() -> RandData {
        RandData { streams: vec![Rng::new(0); RandData::COUNT] }
    }
}

impl RandData {
    pub const COUNT: usize = 45;

    pub fn new(seed: u32) -> RandData {
        let mut master = Rng::new(seed);
        RandData { streams: (0..RandData::COUNT).map(|_| Rng::new(master.next_u32())).collect() }
    }

    /// The generators' part of the game's checksum (the rand module's, FUN_6f672050):
    /// `b * 0x7a2d ^ b` summed over every byte of every generator's value and taps.
    pub fn checksum(&self) -> u32 {
        let mut sum = 0u32;
        for r in &self.streams {
            for b in r.value.to_le_bytes().into_iter().chain(r.taps.to_le_bytes()) {
                let b = b as u32;
                sum = sum.wrapping_add(b.wrapping_mul(0x7a2d) ^ b);
            }
        }
        sum
    }
}

/// StringHash: Storm's SStrHash (ordinal 590), Bob Jenkins' lookup2 over the string
/// upper-cased with `/` mapped to `\`.
pub fn string_hash(s: &str) -> i32 {
    let k: Vec<u8> = s
        .bytes()
        .take(0x3ff)
        .map(|c| match c {
            b'a'..=b'z' => c - 0x20,
            b'/' => b'\\',
            c => c,
        })
        .collect();
    lookup2(&k, 0) as i32
}

fn mix(a: &mut u32, b: &mut u32, c: &mut u32) {
    *a = a.wrapping_sub(*b).wrapping_sub(*c) ^ (*c >> 13);
    *b = b.wrapping_sub(*c).wrapping_sub(*a) ^ (*a << 8);
    *c = c.wrapping_sub(*a).wrapping_sub(*b) ^ (*b >> 13);
    *a = a.wrapping_sub(*b).wrapping_sub(*c) ^ (*c >> 12);
    *b = b.wrapping_sub(*c).wrapping_sub(*a) ^ (*a << 16);
    *c = c.wrapping_sub(*a).wrapping_sub(*b) ^ (*b >> 5);
    *a = a.wrapping_sub(*b).wrapping_sub(*c) ^ (*c >> 3);
    *b = b.wrapping_sub(*c).wrapping_sub(*a) ^ (*a << 10);
    *c = c.wrapping_sub(*a).wrapping_sub(*b) ^ (*b >> 15);
}

fn lookup2(k: &[u8], init: u32) -> u32 {
    let (mut a, mut b, mut c) = (0x9e37_79b9u32, 0x9e37_79b9u32, init);
    let mut p = k;
    while p.len() >= 12 {
        let w = |i: usize| u32::from_le_bytes([p[i], p[i + 1], p[i + 2], p[i + 3]]);
        a = a.wrapping_add(w(0));
        b = b.wrapping_add(w(4));
        c = c.wrapping_add(w(8));
        mix(&mut a, &mut b, &mut c);
        p = &p[12..];
    }
    c = c.wrapping_add(k.len() as u32);
    let n = p.len();
    let byte = |i: usize| p[i] as u32;
    if n > 10 { c = c.wrapping_add(byte(10) << 24); }
    if n > 9 { c = c.wrapping_add(byte(9) << 16); }
    if n > 8 { c = c.wrapping_add(byte(8) << 8); }
    if n > 7 { b = b.wrapping_add(byte(7) << 24); }
    if n > 6 { b = b.wrapping_add(byte(6) << 16); }
    if n > 5 { b = b.wrapping_add(byte(5) << 8); }
    if n > 4 { b = b.wrapping_add(byte(4)); }
    if n > 3 { a = a.wrapping_add(byte(3) << 24); }
    if n > 2 { a = a.wrapping_add(byte(2) << 16); }
    if n > 1 { a = a.wrapping_add(byte(1) << 8); }
    if n > 0 { a = a.wrapping_add(byte(0)); }
    mix(&mut a, &mut b, &mut c);
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The original's generators at the first turn of Booty Bay with the game's seed
    /// 0x35000000 (read under Wine): the main one 27 draws on, and the others as seeded
    /// (all but three, which had been drawn on).
    #[test]
    fn the_generators_seed_as_the_originals() {
        let mut main = Rng::new(0x3500_0000);
        for _ in 0..27 {
            main.next_u32();
        }
        assert_eq!((main.value, main.taps), (0x0d5a_5d0a, 0x8064_a444));
        let r = RandData::new(0x3500_0000);
        assert_eq!((r.streams[0].value, r.streams[0].taps), (0x213e_e20d, 0x4c98_b0d0));
        assert_eq!((r.streams[1].value, r.streams[1].taps), (0x5a78_9536, 0x6028_58f0));
        assert_eq!((r.streams[44].value, r.streams[44].taps), (0xecd1_333d, 0x20a8_40c0));
    }
}
