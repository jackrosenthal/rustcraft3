//! MPQ hashing and encryption primitives.

use std::sync::OnceLock;

pub const HASH_TABLE_OFFSET: u32 = 0;
pub const HASH_NAME_A: u32 = 1;
pub const HASH_NAME_B: u32 = 2;
pub const HASH_FILE_KEY: u32 = 3;

fn table() -> &'static [u32; 0x500] {
    static TABLE: OnceLock<[u32; 0x500]> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut t = [0u32; 0x500];
        let mut seed: u32 = 0x0010_0001;
        for i in 0..0x100 {
            let mut idx = i;
            for _ in 0..5 {
                seed = (seed * 125 + 3) % 0x2A_AAAB;
                let hi = (seed & 0xFFFF) << 16;
                seed = (seed * 125 + 3) % 0x2A_AAAB;
                let lo = seed & 0xFFFF;
                t[idx] = hi | lo;
                idx += 0x100;
            }
        }
        t
    })
}

/// Hashes a file name the way Storm does: case-insensitive, with `/` treated as `\`.
pub fn hash_string(s: &str, kind: u32) -> u32 {
    let t = table();
    let mut seed1: u32 = 0x7FED_7FED;
    let mut seed2: u32 = 0xEEEE_EEEE;
    for &b in s.as_bytes() {
        let ch = match b.to_ascii_uppercase() {
            b'/' => b'\\',
            c => c,
        } as u32;
        seed1 = t[(kind * 0x100 + ch) as usize] ^ seed1.wrapping_add(seed2);
        seed2 = ch
            .wrapping_add(seed1)
            .wrapping_add(seed2)
            .wrapping_add(seed2 << 5)
            .wrapping_add(3);
    }
    seed1
}

pub fn decrypt(data: &mut [u32], mut key: u32) {
    let t = table();
    let mut seed: u32 = 0xEEEE_EEEE;
    for v in data.iter_mut() {
        seed = seed.wrapping_add(t[0x400 + (key & 0xFF) as usize]);
        let ch = *v ^ key.wrapping_add(seed);
        key = ((!key << 0x15).wrapping_add(0x1111_1111)) | (key >> 0x0B);
        seed = ch
            .wrapping_add(seed)
            .wrapping_add(seed << 5)
            .wrapping_add(3);
        *v = ch;
    }
}

/// Decrypts a byte buffer in place; a trailing partial dword is left untouched, as Storm does.
pub fn decrypt_bytes(data: &mut [u8], key: u32) {
    let n = data.len() / 4;
    let mut words: Vec<u32> = data[..n * 4]
        .chunks_exact(4)
        .map(|c| u32::from_le_bytes(c.try_into().unwrap()))
        .collect();
    decrypt(&mut words, key);
    for (c, w) in data[..n * 4].chunks_exact_mut(4).zip(words) {
        c.copy_from_slice(&w.to_le_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_table_keys() {
        assert_eq!(hash_string("(hash table)", HASH_FILE_KEY), 0xC3AF_3770);
        assert_eq!(hash_string("(block table)", HASH_FILE_KEY), 0xEC83_B3A3);
    }
}

/// Encrypts in place (the inverse of [`decrypt`]).
pub fn encrypt(data: &mut [u32], mut key: u32) {
    let t = table();
    let mut seed: u32 = 0xEEEE_EEEE;
    for v in data.iter_mut() {
        seed = seed.wrapping_add(t[0x400 + (key & 0xFF) as usize]);
        let ch = *v;
        *v = ch ^ key.wrapping_add(seed);
        key = ((!key << 0x15).wrapping_add(0x1111_1111)) | (key >> 0x0B);
        seed = ch
            .wrapping_add(seed)
            .wrapping_add(seed << 5)
            .wrapping_add(3);
    }
}
