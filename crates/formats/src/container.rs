//! The original's recorded-game container, which its replays (.w3g) and saved game
//! caches (.w3v) share (NetGameStore.cpp, `FUN_6f3103b0`): a 0x44-byte header (the
//! magic, its own size, the file's size, 1, the record's size, the blocks, the
//! product, version, build, flags, a length in milliseconds, and its CRC-32 over itself
//! with that field zeroed), then the record in blocks of a fixed size, the last padded
//! with zeros, each compressed alone and checked by the CRC-32s of its header and its
//! data, each folded to 16 bits (verified against a replay and a cache the original
//! wrote).

use crate::{Error, Reader};

pub const MAGIC: &[u8; 28] = b"Warcraft III recorded game\x1a\0";

/// The header's fields past its sizes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Header {
    pub product: [u8; 4],
    pub version: u32,
    pub build: u16,
    pub flags: u16,
    pub length_ms: u32,
}

/// The header and the record, uncompressed.
pub fn unpack(file: &[u8]) -> Result<(Header, Vec<u8>), Error> {
    let mut r = Reader::new(file);
    if r.bytes(28)? != MAGIC {
        return Err(Error::Format("not a recorded game"));
    }
    let header_size = r.u32()? as usize;
    let _compressed = r.u32()?;
    let _header_version = r.u32()?;
    let size = r.u32()? as usize;
    let blocks = r.u32()?;
    let h = Header { product: r.tag()?, version: r.u32()?, build: r.u16()?, flags: r.u16()?, length_ms: r.u32()? };
    r.pos = header_size;
    // Each block is its own zlib stream, flushed but not finished.
    let mut data = Vec::with_capacity(size);
    for _ in 0..blocks {
        let packed = r.u16()? as usize;
        let unpacked = r.u16()? as usize;
        r.u32()?;
        let mut out = Vec::with_capacity(unpacked);
        let mut z = flate2::Decompress::new(true);
        let _ = z.decompress_vec(r.bytes(packed)?, &mut out, flate2::FlushDecompress::Sync);
        if out.len() < unpacked {
            return Err(Error::Format("short block"));
        }
        data.extend_from_slice(&out[..unpacked]);
    }
    data.truncate(size);
    Ok((h, data))
}

/// The file holding a record, in blocks of `block` bytes (8192 for replays, 2048 for
/// caches: `FUN_6f30f0f0`'s argument).
pub fn pack(h: &Header, data: &[u8], block: usize) -> Vec<u8> {
    let mut blocks = Vec::new();
    for chunk in data.chunks(block) {
        let mut plain = chunk.to_vec();
        plain.resize(block, 0);
        let mut z = flate2::Compress::new(flate2::Compression::default(), true);
        let mut packed = Vec::with_capacity(block + 64);
        let _ = z.compress_vec(&plain, &mut packed, flate2::FlushCompress::Sync);
        let fold = |c: u32| (c ^ (c >> 16)) & 0xffff;
        let mut head = Vec::new();
        head.extend((packed.len() as u16).to_le_bytes());
        head.extend((block as u16).to_le_bytes());
        let c1 = crc(&[&head[..], &[0u8; 4][..]].concat());
        let c2 = crc(&packed);
        head.extend((fold(c1) | fold(c2) << 16).to_le_bytes());
        blocks.push((head, packed));
    }
    let body_len: usize = blocks.iter().map(|(h, p)| h.len() + p.len()).sum();
    let mut out = MAGIC.to_vec();
    for v in [0x44u32, (0x44 + body_len) as u32, 1, data.len() as u32, blocks.len() as u32] {
        out.extend(v.to_le_bytes());
    }
    out.extend(h.product);
    out.extend(h.version.to_le_bytes());
    out.extend(h.build.to_le_bytes());
    out.extend(h.flags.to_le_bytes());
    out.extend(h.length_ms.to_le_bytes());
    let c = crc(&[&out[..], &[0u8; 4][..]].concat());
    out.extend(c.to_le_bytes());
    for (h, p) in blocks {
        out.extend(h);
        out.extend(p);
    }
    out
}

fn crc(b: &[u8]) -> u32 {
    let mut h = crc32fast::Hasher::new();
    h.update(b);
    h.finalize()
}
