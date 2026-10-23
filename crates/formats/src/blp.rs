//! BLP1 textures: either JPEG-compressed (four-component BGRA JPEG with a shared header)
//! or paletted with a separate 0/1/4/8-bit alpha plane.

use crate::{Error, Reader};

#[derive(Clone, Debug)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    /// RGBA8, row-major.
    pub rgba: Vec<u8>,
}

pub struct Blp<'a> {
    data: &'a [u8],
    jpeg: bool,
    alpha_bits: u32,
    pub width: u32,
    pub height: u32,
    has_mips: bool,
    offsets: [u32; 16],
    sizes: [u32; 16],
}

impl<'a> Blp<'a> {
    pub fn parse(data: &'a [u8]) -> Result<Self, Error> {
        let mut r = Reader::new(data);
        if r.bytes(4)? != b"BLP1" {
            return Err(Error::Format("not a BLP1 texture"));
        }
        let content = r.u32()?;
        let alpha_bits = r.u32()?;
        let width = r.u32()?;
        let height = r.u32()?;
        let _extra = r.u32()?;
        let has_mips = r.u32()? != 0;
        let mut offsets = [0; 16];
        let mut sizes = [0; 16];
        for o in &mut offsets {
            *o = r.u32()?;
        }
        for s in &mut sizes {
            *s = r.u32()?;
        }
        Ok(Blp {
            data,
            jpeg: content == 0,
            alpha_bits,
            width,
            height,
            has_mips,
            offsets,
            sizes,
        })
    }

    pub fn mip_count(&self) -> usize {
        if !self.has_mips {
            return 1;
        }
        let max = (32 - self.width.max(self.height).leading_zeros()) as usize;
        (0..16.min(max))
            .take_while(|&i| self.sizes[i] != 0)
            .count()
            .max(1)
    }

    pub fn mip(&self, level: usize) -> Result<Image, Error> {
        let w = (self.width >> level).max(1);
        let h = (self.height >> level).max(1);
        let off = self.offsets[level] as usize;
        let len = self.sizes[level] as usize;
        let body = self
            .data
            .get(off..off + len)
            .ok_or(Error::Format("BLP mip out of range"))?;
        let mut rgba = vec![0u8; (w * h * 4) as usize];
        if self.jpeg {
            let hdr_len = u32::from_le_bytes(self.data[156..160].try_into().unwrap()) as usize;
            let mut jpg = Vec::with_capacity(hdr_len + len);
            jpg.extend_from_slice(&self.data[160..160 + hdr_len]);
            jpg.extend_from_slice(body);
            let mut dec = jpeg_decoder::Decoder::new(&jpg[..]);
            let px = dec.decode().map_err(|_| Error::Format("BLP JPEG"))?;
            let info = dec.info().ok_or(Error::Format("BLP JPEG info"))?;
            let (jw, jh) = (info.width as u32, info.height as u32);
            let comps = px.len() / (jw * jh) as usize;
            for y in 0..h.min(jh) {
                for x in 0..w.min(jw) {
                    let s = ((y * jw + x) as usize) * comps;
                    let d = ((y * w + x) * 4) as usize;
                    // Components are stored B, G, R, A.  The decoder treats four-component
                    // data as Adobe CMYK and inverts it, so invert back.
                    let c = |i: usize| if comps == 4 { 255 - px[s + i] } else { px[s + i.min(comps - 1)] };
                    rgba[d] = c(2);
                    rgba[d + 1] = c(1);
                    rgba[d + 2] = c(0);
                    // The fourth component is alpha even when the header's alpha bits are 0
                    // (the campaign loading screens' emblems show through it).
                    rgba[d + 3] = if comps == 4 { c(3) } else { 255 };
                }
            }
        } else {
            let pal = self
                .data
                .get(156..156 + 1024)
                .ok_or(Error::Format("BLP palette"))?;
            let n = (w * h) as usize;
            let idx = body.get(..n).ok_or(Error::Format("BLP indices"))?;
            let alpha = &body[n..];
            for i in 0..n {
                let p = idx[i] as usize * 4;
                rgba[i * 4] = pal[p + 2];
                rgba[i * 4 + 1] = pal[p + 1];
                rgba[i * 4 + 2] = pal[p];
                rgba[i * 4 + 3] = match self.alpha_bits {
                    1 => alpha.get(i / 8).map_or(255, |b| if b >> (i % 8) & 1 != 0 { 255 } else { 0 }),
                    4 => alpha.get(i / 2).map_or(255, |b| ((b >> ((i % 2) * 4)) & 0xF) * 17),
                    8 => alpha.get(i).copied().unwrap_or(255),
                    _ => 255,
                };
            }
        }
        Ok(Image { width: w, height: h, rgba })
    }
}

/// Decodes the top-level image of any texture format the game loads: BLP, TGA, or JPEG.
pub fn decode_texture(path: &str, data: &[u8]) -> Result<Vec<Image>, Error> {
    if data.starts_with(b"BLP1") {
        let b = Blp::parse(data)?;
        return (0..b.mip_count()).map(|i| b.mip(i)).collect();
    }
    let fmt = if path.to_ascii_lowercase().ends_with(".tga") {
        image::ImageFormat::Tga
    } else {
        image::guess_format(data).map_err(|_| Error::Format("unknown image format"))?
    };
    let img = image::load_from_memory_with_format(data, fmt)
        .map_err(|_| Error::Format("image decode"))?
        .to_rgba8();
    Ok(vec![Image {
        width: img.width(),
        height: img.height(),
        rgba: img.into_raw(),
    }])
}
