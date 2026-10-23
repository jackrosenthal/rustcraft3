//! Sector decompression.  A compressed sector starts with a mask byte naming the
//! codecs that were applied; they are undone in the fixed order Storm uses.

use crate::Error;
use std::io::Read;

pub const HUFFMAN: u8 = 0x01;
pub const ZLIB: u8 = 0x02;
pub const PKWARE: u8 = 0x08;
pub const BZIP2: u8 = 0x10;
pub const ADPCM_MONO: u8 = 0x40;
pub const ADPCM_STEREO: u8 = 0x80;

/// Decompresses one sector whose first byte is the compression mask.
pub fn decompress(data: &[u8], out_len: usize) -> Result<Vec<u8>, Error> {
    let (&mask, rest) = data.split_first().ok_or(Error::Corrupt("empty sector"))?;
    let known = HUFFMAN | ZLIB | PKWARE | BZIP2 | ADPCM_MONO | ADPCM_STEREO;
    if mask & !known != 0 {
        return Err(Error::Unsupported(format!("compression mask {mask:#04x}")));
    }
    if mask & BZIP2 != 0 {
        let mut out = Vec::with_capacity(out_len);
        bzip2_rs::DecoderReader::new(rest)
            .read_to_end(&mut out)
            .map_err(|_| Error::Corrupt("bzip2"))?;
        return chain(mask & !BZIP2, out, out_len);
    }
    if mask & PKWARE != 0 {
        return chain(mask & !PKWARE, explode(rest, out_len)?, out_len);
    }
    chain(mask, rest.to_vec(), out_len)
}

fn chain(mask: u8, mut buf: Vec<u8>, out_len: usize) -> Result<Vec<u8>, Error> {
    if mask & ZLIB != 0 {
        let mut out = Vec::with_capacity(out_len);
        flate2::read::ZlibDecoder::new(&buf[..])
            .read_to_end(&mut out)
            .map_err(|_| Error::Corrupt("zlib"))?;
        buf = out;
    }
    if mask & HUFFMAN != 0 {
        buf = crate::huffman::decompress(&buf, out_len)?;
    }
    if mask & ADPCM_STEREO != 0 {
        buf = crate::adpcm::decompress(&buf, 2);
    }
    if mask & ADPCM_MONO != 0 {
        buf = crate::adpcm::decompress(&buf, 1);
    }
    Ok(buf)
}

/// PKWARE DCL "implode" decompression, used for sectors flagged with the implode block flag
/// and for the 0x08 compression mask.
pub fn explode(data: &[u8], out_len: usize) -> Result<Vec<u8>, Error> {
    use implode::exploder::Exploder;
    use implode::symbol::DEFAULT_CODE_TABLE;
    let mut ex = Exploder::new(&DEFAULT_CODE_TABLE);
    let mut out = Vec::with_capacity(out_len);
    let mut pos = 0;
    while !ex.ended && pos < data.len() && out.len() < out_len {
        let (used, block) = ex
            .explode_block(&data[pos..])
            .map_err(|_| Error::Corrupt("pkware implode"))?;
        if used == 0 && block.is_empty() {
            return Err(Error::Corrupt("pkware implode stalled"));
        }
        pos += used;
        out.extend_from_slice(block);
    }
    out.truncate(out_len);
    Ok(out)
}
