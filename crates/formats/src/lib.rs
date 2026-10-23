//! Parsers for the file formats shipped in the Warcraft III data archives.

pub mod blp;
pub mod campaign;
pub mod map;
pub mod mdx;
pub mod profile;
pub mod slk;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("format error: {0}")]
    Format(&'static str),
    #[error("unexpected end of data")]
    Eof,
}

/// Little-endian cursor over a byte slice.
#[derive(Clone)]
pub struct Reader<'a> {
    pub data: &'a [u8],
    pub pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Reader { data, pos: 0 }
    }
    pub fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }
    pub fn bytes(&mut self, n: usize) -> Result<&'a [u8], Error> {
        let s = self.data.get(self.pos..self.pos + n).ok_or(Error::Eof)?;
        self.pos += n;
        Ok(s)
    }
    pub fn tag(&mut self) -> Result<[u8; 4], Error> {
        Ok(self.bytes(4)?.try_into().unwrap())
    }
    pub fn u8(&mut self) -> Result<u8, Error> {
        Ok(self.bytes(1)?[0])
    }
    pub fn u16(&mut self) -> Result<u16, Error> {
        Ok(u16::from_le_bytes(self.bytes(2)?.try_into().unwrap()))
    }
    pub fn u32(&mut self) -> Result<u32, Error> {
        Ok(u32::from_le_bytes(self.bytes(4)?.try_into().unwrap()))
    }
    pub fn i32(&mut self) -> Result<i32, Error> {
        Ok(self.u32()? as i32)
    }
    pub fn f32(&mut self) -> Result<f32, Error> {
        Ok(f32::from_bits(self.u32()?))
    }
    pub fn vec3(&mut self) -> Result<[f32; 3], Error> {
        Ok([self.f32()?, self.f32()?, self.f32()?])
    }
    pub fn vec4(&mut self) -> Result<[f32; 4], Error> {
        Ok([self.f32()?, self.f32()?, self.f32()?, self.f32()?])
    }
    /// A NUL-terminated string.
    pub fn cstr(&mut self) -> Result<String, Error> {
        let rest = &self.data[self.pos.min(self.data.len())..];
        let n = rest.iter().position(|&b| b == 0).ok_or(Error::Eof)?;
        self.pos += n + 1;
        Ok(String::from_utf8_lossy(&rest[..n]).into_owned())
    }
    /// A fixed-size, NUL-padded string field.
    pub fn fixed_str(&mut self, n: usize) -> Result<String, Error> {
        let b = self.bytes(n)?;
        let end = b.iter().position(|&c| c == 0).unwrap_or(n);
        Ok(String::from_utf8_lossy(&b[..end]).into_owned())
    }
    pub fn skip(&mut self, n: usize) -> Result<(), Error> {
        self.bytes(n).map(|_| ())
    }
    pub fn sub(&mut self, n: usize) -> Result<Reader<'a>, Error> {
        Ok(Reader::new(self.bytes(n)?))
    }
}
