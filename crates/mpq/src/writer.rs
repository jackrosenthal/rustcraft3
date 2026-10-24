//! Writing a simple MPQ (format 1, files stored uncompressed and unencrypted), for
//! test maps.

use crate::crypt::{encrypt, hash_string, HASH_FILE_KEY, HASH_NAME_A, HASH_NAME_B, HASH_TABLE_OFFSET};

const FLAG_EXISTS: u32 = 0x8000_0000;
const FLAG_SINGLE_UNIT: u32 = 0x0100_0000;

/// The archive's bytes holding `files` (name, contents), with a (listfile).
pub fn write_archive(files: &[(String, Vec<u8>)]) -> Vec<u8> {
    let mut files: Vec<(String, Vec<u8>)> = files.iter().filter(|f| !f.0.eq_ignore_ascii_case("(listfile)") && !f.0.eq_ignore_ascii_case("(attributes)")).cloned().collect();
    let listfile = files.iter().map(|f| f.0.clone()).collect::<Vec<_>>().join("\r\n");
    files.push(("(listfile)".to_string(), listfile.into_bytes()));
    let hash_size = (files.len() * 2).next_power_of_two().max(16);
    let mut out = vec![0u8; 32];
    let mut blocks: Vec<[u32; 4]> = Vec::new();
    for (_, data) in &files {
        let offset = out.len() as u32;
        out.extend_from_slice(data);
        blocks.push([offset, data.len() as u32, data.len() as u32, FLAG_EXISTS | FLAG_SINGLE_UNIT]);
    }
    let mut hashes = vec![[0xFFFF_FFFFu32; 4]; hash_size];
    for (k, (name, _)) in files.iter().enumerate() {
        let mut i = hash_string(name, HASH_TABLE_OFFSET) as usize & (hash_size - 1);
        while hashes[i][3] != 0xFFFF_FFFF {
            i = (i + 1) & (hash_size - 1);
        }
        hashes[i] = [hash_string(name, HASH_NAME_A), hash_string(name, HASH_NAME_B), 0, k as u32];
    }
    let table = |rows: &[[u32; 4]], key: &str, out: &mut Vec<u8>| -> u32 {
        let offset = out.len() as u32;
        let mut words: Vec<u32> = rows.iter().flatten().copied().collect();
        encrypt(&mut words, hash_string(key, HASH_FILE_KEY));
        for w in words {
            out.extend_from_slice(&w.to_le_bytes());
        }
        offset
    };
    let hash_offset = table(&hashes, "(hash table)", &mut out);
    let block_offset = table(&blocks, "(block table)", &mut out);
    let size = out.len() as u32;
    // Magic, header size, archive size, format 0 and sector shift 3, the tables'
    // offsets, and their entry counts.
    let mut header = Vec::with_capacity(32);
    header.extend_from_slice(b"MPQ\x1a");
    header.extend_from_slice(&32u32.to_le_bytes());
    header.extend_from_slice(&size.to_le_bytes());
    header.extend_from_slice(&0u16.to_le_bytes());
    header.extend_from_slice(&3u16.to_le_bytes());
    header.extend_from_slice(&hash_offset.to_le_bytes());
    header.extend_from_slice(&block_offset.to_le_bytes());
    header.extend_from_slice(&(hash_size as u32).to_le_bytes());
    header.extend_from_slice(&(blocks.len() as u32).to_le_bytes());
    out[..32].copy_from_slice(&header);
    out
}
