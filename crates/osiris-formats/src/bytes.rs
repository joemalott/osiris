//! A little-endian cursor over a byte slice. Reads past the end return `Truncated`.

use crate::{Error, Result};

#[derive(Clone)]
pub struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
    what: &'static str,
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8], what: &'static str) -> Self {
        Self { data, pos: 0, what }
    }

    pub fn pos(&self) -> usize {
        self.pos
    }

    pub fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    pub fn seek(&mut self, pos: usize) -> Result<()> {
        if pos > self.data.len() {
            return Err(Error::Truncated(self.what));
        }
        self.pos = pos;
        Ok(())
    }

    pub fn skip(&mut self, n: usize) -> Result<()> {
        self.seek(self.pos + n)
    }

    pub fn bytes(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self.pos.checked_add(n).ok_or(Error::Truncated(self.what))?;
        let s = self
            .data
            .get(self.pos..end)
            .ok_or(Error::Truncated(self.what))?;
        self.pos = end;
        Ok(s)
    }

    pub fn array<const N: usize>(&mut self) -> Result<[u8; N]> {
        Ok(self.bytes(N)?.try_into().unwrap())
    }

    pub fn u8(&mut self) -> Result<u8> {
        Ok(self.array::<1>()?[0])
    }
    pub fn i8(&mut self) -> Result<i8> {
        Ok(self.u8()? as i8)
    }
    pub fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes(self.array()?))
    }
    pub fn i16(&mut self) -> Result<i16> {
        Ok(i16::from_le_bytes(self.array()?))
    }
    pub fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.array()?))
    }
    pub fn i32(&mut self) -> Result<i32> {
        Ok(i32::from_le_bytes(self.array()?))
    }

    /// Fixed-size, NUL-padded string field.
    pub fn cstr(&mut self, n: usize) -> Result<String> {
        let raw = self.bytes(n)?;
        let end = raw.iter().position(|&b| b == 0).unwrap_or(n);
        Ok(crate::text::decode_cp1252(&raw[..end]))
    }
}

/// A little-endian cursor that overwrites fields of a fixed-size byte slice in place,
/// the mirror of `Reader`. Writing past the end panics: callers own the layout.
pub struct Writer<'a> {
    data: &'a mut [u8],
    pos: usize,
}

impl<'a> Writer<'a> {
    pub fn new(data: &'a mut [u8]) -> Self {
        Self { data, pos: 0 }
    }

    pub fn pos(&self) -> usize {
        self.pos
    }

    /// Steps over `n` bytes, leaving them as they are.
    pub fn skip(&mut self, n: usize) {
        assert!(self.pos + n <= self.data.len(), "write past end");
        self.pos += n;
    }

    pub fn bytes(&mut self, b: &[u8]) {
        self.data[self.pos..self.pos + b.len()].copy_from_slice(b);
        self.pos += b.len();
    }

    pub fn u8(&mut self, v: u8) {
        self.bytes(&[v]);
    }
    pub fn u16(&mut self, v: u16) {
        self.bytes(&v.to_le_bytes());
    }
    pub fn i16(&mut self, v: i16) {
        self.bytes(&v.to_le_bytes());
    }
    pub fn u32(&mut self, v: u32) {
        self.bytes(&v.to_le_bytes());
    }
    pub fn i32(&mut self, v: i32) {
        self.bytes(&v.to_le_bytes());
    }

    /// A flag stored in `N` bytes. A stored value that already reads as `v` (any
    /// non-zero value is true) is kept as it is; otherwise it becomes 1 or 0.
    fn flag<const N: usize>(&mut self, v: bool) {
        let cur = &self.data[self.pos..self.pos + N];
        if cur.iter().any(|&b| b != 0) != v {
            let mut new = [0u8; N];
            new[0] = v as u8;
            self.bytes(&new);
        } else {
            self.pos += N;
        }
    }
    pub fn flag8(&mut self, v: bool) {
        self.flag::<1>(v);
    }
    pub fn flag16(&mut self, v: bool) {
        self.flag::<2>(v);
    }
    pub fn flag32(&mut self, v: bool) {
        self.flag::<4>(v);
    }

    /// Fixed-size, NUL-padded string field, the mirror of `Reader::cstr`: characters
    /// are stored as Windows-1252 bytes (`?` for anything beyond), then a NUL when
    /// there is room. Bytes after the NUL are left alone. A string longer than the
    /// field is cut to leave room for its NUL.
    pub fn cstr(&mut self, n: usize, s: &str) {
        let mut raw: Vec<u8> = s
            .chars()
            .map(crate::text::cp1252_byte)
            .collect();
        if raw.len() > n {
            raw.truncate(n - 1);
        }
        let start = self.pos;
        self.bytes(&raw);
        if raw.len() < n {
            self.u8(0);
        }
        self.pos = start + n;
    }
}
