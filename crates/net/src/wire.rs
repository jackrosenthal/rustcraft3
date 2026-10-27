//! Little-endian fields, as every W3GS packet lays them out.

use crate::Error;

/// Reads a packet body's fields in order.
pub struct Reader<'a> {
    b: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    pub fn new(b: &'a [u8]) -> Reader<'a> {
        Reader { b, at: 0 }
    }

    pub fn bytes(&mut self, n: usize) -> Result<&'a [u8], Error> {
        let s = self.b.get(self.at..self.at + n).ok_or(Error::Short)?;
        self.at += n;
        Ok(s)
    }

    pub fn array<const N: usize>(&mut self) -> Result<[u8; N], Error> {
        Ok(self.bytes(N)?.try_into().unwrap())
    }

    pub fn u8(&mut self) -> Result<u8, Error> {
        Ok(self.bytes(1)?[0])
    }

    pub fn u16(&mut self) -> Result<u16, Error> {
        Ok(u16::from_le_bytes(self.array()?))
    }

    pub fn u32(&mut self) -> Result<u32, Error> {
        Ok(u32::from_le_bytes(self.array()?))
    }

    /// A NUL-terminated string (its bytes taken as UTF-8, lossily).
    pub fn cstr(&mut self) -> Result<String, Error> {
        let rest = &self.b[self.at.min(self.b.len())..];
        let n = rest.iter().position(|&c| c == 0).ok_or(Error::Short)?;
        self.at += n + 1;
        Ok(String::from_utf8_lossy(&rest[..n]).into_owned())
    }

    pub fn rest(&mut self) -> &'a [u8] {
        let s = &self.b[self.at.min(self.b.len())..];
        self.at = self.b.len();
        s
    }

    /// The bytes not yet read.
    pub fn rest_len(&self) -> usize {
        self.b.len().saturating_sub(self.at)
    }

    pub fn is_empty(&self) -> bool {
        self.at >= self.b.len()
    }
}

/// Writes a packet body's fields in order.
#[derive(Default)]
pub struct Writer(pub Vec<u8>);

impl Writer {
    pub fn u8(&mut self, v: u8) -> &mut Self {
        self.0.push(v);
        self
    }

    pub fn u16(&mut self, v: u16) -> &mut Self {
        self.0.extend(v.to_le_bytes());
        self
    }

    pub fn u32(&mut self, v: u32) -> &mut Self {
        self.0.extend(v.to_le_bytes());
        self
    }

    pub fn bytes(&mut self, v: &[u8]) -> &mut Self {
        self.0.extend_from_slice(v);
        self
    }

    pub fn cstr(&mut self, v: &str) -> &mut Self {
        self.0.extend(v.bytes().filter(|&c| c != 0));
        self.0.push(0);
        self
    }
}
