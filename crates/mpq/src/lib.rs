//! Reader for Warcraft III MPQ archives (format version 0, as used by the 1.27b data files
//! and by maps, which may prefix the archive with a 512-byte map header).

mod adpcm;
mod compression;
pub mod crypt;
mod huffman;

use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crypt::*;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
    #[error("not an MPQ archive")]
    NotMpq,
    #[error("corrupt archive: {0}")]
    Corrupt(&'static str),
    #[error("unsupported: {0}")]
    Unsupported(String),
    #[error("file not found: {0}")]
    NotFound(String),
}

const FLAG_IMPLODE: u32 = 0x0000_0100;
const FLAG_COMPRESS: u32 = 0x0000_0200;
const FLAG_ENCRYPTED: u32 = 0x0001_0000;
const FLAG_FIX_KEY: u32 = 0x0002_0000;
const FLAG_SINGLE_UNIT: u32 = 0x0100_0000;
const FLAG_SECTOR_CRC: u32 = 0x0400_0000;
const FLAG_EXISTS: u32 = 0x8000_0000;

const HASH_EMPTY: u32 = 0xFFFF_FFFF;
const HASH_DELETED: u32 = 0xFFFF_FFFE;

#[derive(Clone, Copy, Debug)]
struct HashEntry {
    name_a: u32,
    name_b: u32,
    locale: u16,
    block: u32,
}

#[derive(Clone, Copy, Debug)]
struct BlockEntry {
    offset: u32,
    csize: u32,
    fsize: u32,
    flags: u32,
}

pub struct Archive {
    path: PathBuf,
    file: Mutex<File>,
    base: u64,
    archive_size: u64,
    sector_size: usize,
    hashes: Vec<HashEntry>,
    blocks: Vec<BlockEntry>,
}

fn read_u32s(b: &[u8]) -> Vec<u32> {
    b.chunks_exact(4)
        .map(|c| u32::from_le_bytes(c.try_into().unwrap()))
        .collect()
}

impl Archive {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, Error> {
        let path = path.as_ref().to_path_buf();
        let mut file = File::open(&path)?;
        let len = file.metadata()?.len();
        // The header may start at any 512-byte boundary (maps carry an HM3W header first).
        let mut base = 0u64;
        let mut hdr = [0u8; 32];
        loop {
            if base + 32 > len {
                return Err(Error::NotMpq);
            }
            file.seek(SeekFrom::Start(base))?;
            file.read_exact(&mut hdr)?;
            if &hdr[0..4] == b"MPQ\x1A" {
                break;
            }
            if &hdr[0..4] == b"MPQ\x1B" {
                base += u32::from_le_bytes(hdr[8..12].try_into().unwrap()) as u64;
                continue;
            }
            base += 512;
        }
        let w = read_u32s(&hdr);
        let archive_size = w[2] as u64;
        let sector_shift = u16::from_le_bytes([hdr[14], hdr[15]]);
        let hash_pos = w[4] as u64;
        let block_pos = w[5] as u64;
        // Protected maps put garbage in the high bits of the entry counts; clamp to the file.
        let hash_count = (w[6] as u64).min((len.saturating_sub(base + hash_pos)) / 16) as usize;
        let block_count = (w[7] as u64).min((len.saturating_sub(base + block_pos)) / 16) as usize;

        let mut table = |pos: u64, count: usize, key: &str| -> Result<Vec<u32>, Error> {
            let mut raw = vec![0u8; count * 16];
            file.seek(SeekFrom::Start(base + pos))?;
            file.read_exact(&mut raw)?;
            let mut v = read_u32s(&raw);
            decrypt(&mut v, hash_string(key, HASH_FILE_KEY));
            Ok(v)
        };
        let hashes = table(hash_pos, hash_count, "(hash table)")?
            .chunks_exact(4)
            .map(|c| HashEntry {
                name_a: c[0],
                name_b: c[1],
                locale: (c[2] & 0xFFFF) as u16,
                block: c[3],
            })
            .collect();
        let blocks = table(block_pos, block_count, "(block table)")?
            .chunks_exact(4)
            .map(|c| BlockEntry {
                offset: c[0],
                csize: c[1],
                fsize: c[2],
                flags: c[3],
            })
            .collect();
        Ok(Archive {
            path,
            file: Mutex::new(file),
            base,
            archive_size: if archive_size == 0 { len - base } else { archive_size },
            sector_size: 512usize << sector_shift.min(20),
            hashes,
            blocks,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn find(&self, name: &str) -> Option<BlockEntry> {
        if self.hashes.is_empty() {
            return None;
        }
        let n = self.hashes.len();
        let start = hash_string(name, HASH_TABLE_OFFSET) as usize % n;
        let a = hash_string(name, HASH_NAME_A);
        let b = hash_string(name, HASH_NAME_B);
        let mut best: Option<(u16, BlockEntry)> = None;
        for i in 0..n {
            let h = self.hashes[(start + i) % n];
            if h.block == HASH_EMPTY {
                break;
            }
            if h.block == HASH_DELETED || h.name_a != a || h.name_b != b {
                continue;
            }
            let Some(&blk) = self.blocks.get(h.block as usize) else { continue };
            if blk.flags & FLAG_EXISTS == 0 {
                continue;
            }
            // Prefer the neutral locale; fall back to whatever locale is present.
            if h.locale == 0 {
                return Some(blk);
            }
            if best.is_none() {
                best = Some((h.locale, blk));
            }
        }
        best.map(|(_, b)| b)
    }

    pub fn contains(&self, name: &str) -> bool {
        self.find(name).is_some()
    }

    pub fn read(&self, name: &str) -> Result<Vec<u8>, Error> {
        let blk = self.find(name).ok_or_else(|| Error::NotFound(name.to_string()))?;
        self.read_block(name, blk)
    }

    fn read_block(&self, name: &str, blk: BlockEntry) -> Result<Vec<u8>, Error> {
        let fsize = blk.fsize as usize;
        let start = self.base + blk.offset as u64;
        let avail = (self.base + self.archive_size).saturating_sub(start) as usize;
        let mut raw = vec![0u8; (blk.csize as usize).min(avail)];
        {
            let mut f = self.file.lock().unwrap();
            f.seek(SeekFrom::Start(start))?;
            f.read_exact(&mut raw)?;
        }
        let mut key = 0;
        if blk.flags & FLAG_ENCRYPTED != 0 {
            let base_name = name.rsplit(['\\', '/']).next().unwrap_or(name);
            key = hash_string(base_name, HASH_FILE_KEY);
            if blk.flags & FLAG_FIX_KEY != 0 {
                key = key.wrapping_add(blk.offset) ^ blk.fsize;
            }
        }
        let compressed = blk.flags & (FLAG_COMPRESS | FLAG_IMPLODE) != 0;
        let unpack = |sector: &[u8], out_len: usize| -> Result<Vec<u8>, Error> {
            if !compressed || sector.len() >= out_len {
                return Ok(sector[..out_len.min(sector.len())].to_vec());
            }
            if blk.flags & FLAG_IMPLODE != 0 {
                compression::explode(sector, out_len)
            } else {
                compression::decompress(sector, out_len)
            }
        };

        if blk.flags & FLAG_SINGLE_UNIT != 0 {
            if key != 0 {
                decrypt_bytes(&mut raw, key);
            }
            return unpack(&raw, fsize);
        }
        if !compressed {
            if key != 0 {
                for (i, s) in raw.chunks_mut(self.sector_size).enumerate() {
                    decrypt_bytes(s, key.wrapping_add(i as u32));
                }
            }
            raw.truncate(fsize);
            return Ok(raw);
        }

        let nsectors = fsize.div_ceil(self.sector_size);
        let ntable = nsectors + 1 + usize::from(blk.flags & FLAG_SECTOR_CRC != 0);
        if raw.len() < ntable * 4 {
            return Err(Error::Corrupt("sector table"));
        }
        let mut offsets = read_u32s(&raw[..ntable * 4]);
        if key != 0 {
            decrypt(&mut offsets, key.wrapping_sub(1));
        }
        let mut out = Vec::with_capacity(fsize);
        for i in 0..nsectors {
            let (s, e) = (offsets[i] as usize, offsets[i + 1] as usize);
            if s > e || e > raw.len() {
                return Err(Error::Corrupt("sector offsets"));
            }
            let mut sector = raw[s..e].to_vec();
            if key != 0 {
                decrypt_bytes(&mut sector, key.wrapping_add(i as u32));
            }
            let want = (fsize - out.len()).min(self.sector_size);
            let mut data = unpack(&sector, want)?;
            data.truncate(want);
            out.extend_from_slice(&data);
        }
        Ok(out)
    }

    /// Names from the archive's `(listfile)`, if it has one.
    pub fn listfile(&self) -> Vec<String> {
        self.read("(listfile)")
            .map(|b| {
                String::from_utf8_lossy(&b)
                    .split(['\r', '\n', ';'])
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default()
    }
}

/// An ordered set of archives searched from highest to lowest priority, the way the game
/// layers patch, expansion, and base data (and a map on top of all of them).
#[derive(Default)]
pub struct Chain {
    archives: Vec<Archive>,
    cache: Mutex<HashMap<String, Option<usize>>>,
}

impl Chain {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds an archive with lower priority than every archive already present.
    pub fn push_back(&mut self, a: Archive) {
        self.archives.push(a);
        self.cache.lock().unwrap().clear();
    }

    /// Adds an archive with higher priority than every archive already present.
    pub fn push_front(&mut self, a: Archive) {
        self.archives.insert(0, a);
        self.cache.lock().unwrap().clear();
    }

    pub fn pop_front(&mut self) -> Option<Archive> {
        self.cache.lock().unwrap().clear();
        (!self.archives.is_empty()).then(|| self.archives.remove(0))
    }

    pub fn archives(&self) -> &[Archive] {
        &self.archives
    }

    fn locate(&self, name: &str) -> Option<usize> {
        let key = name.to_ascii_lowercase().replace('/', "\\");
        if let Some(&r) = self.cache.lock().unwrap().get(&key) {
            return r;
        }
        let r = self.archives.iter().position(|a| a.contains(&key));
        self.cache.lock().unwrap().insert(key, r);
        r
    }

    pub fn contains(&self, name: &str) -> bool {
        self.locate(name).is_some()
    }

    pub fn read(&self, name: &str) -> Result<Vec<u8>, Error> {
        let i = self.locate(name).ok_or_else(|| Error::NotFound(name.to_string()))?;
        self.archives[i].read(&name.replace('/', "\\"))
    }

    pub fn read_string(&self, name: &str) -> Result<String, Error> {
        let b = self.read(name)?;
        let b = b.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&b);
        Ok(String::from_utf8_lossy(b).into_owned())
    }
}
