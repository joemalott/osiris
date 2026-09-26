//! Least-significant-bit-first bit reader, as Bink video and audio packets are written
//! (FFmpeg's `BITSTREAM_READER_LE`).
//!
//! Ported from FFmpeg's libavcodec/get_bits.h, licensed LGPL-2.1-or-later and used
//! here under GPL-3.0-or-later, as the LGPL allows.

/// Bytes of zeros a packet buffer needs past its end so reads never go out of bounds.
pub const PADDING: usize = 8;

pub struct Bits<'a> {
    data: &'a [u8],
    pos: usize,
    len: usize,
}

impl<'a> Bits<'a> {
    /// Reads `len` bytes of `data`, which must have [`PADDING`] more bytes after them.
    pub fn new(data: &'a [u8], len: usize) -> Bits<'a> {
        debug_assert!(data.len() >= len + PADDING);
        Bits { data, pos: 0, len: len * 8 }
    }

    #[inline]
    fn peek64(&self) -> u64 {
        let i = self.pos >> 3;
        // Past the end (a corrupt packet overreading) reads zeros.
        let word = match self.data.get(i..i + 8) {
            Some(b) => u64::from_le_bytes(b.try_into().unwrap()),
            None => 0,
        };
        word >> (self.pos & 7)
    }

    /// The next `n` bits (at most 32) without consuming them.
    #[inline]
    pub fn peek(&self, n: u32) -> u32 {
        (self.peek64() & ((1u64 << n) - 1)) as u32
    }

    /// Reads `n` bits (0 to 32).
    #[inline]
    pub fn get(&mut self, n: u32) -> u32 {
        let v = self.peek(n);
        self.pos += n as usize;
        v
    }

    #[inline]
    pub fn bit(&mut self) -> bool {
        self.get(1) != 0
    }

    #[inline]
    pub fn skip(&mut self, n: usize) {
        self.pos += n;
    }

    pub fn count(&self) -> usize {
        self.pos
    }

    pub fn left(&self) -> isize {
        self.len as isize - self.pos as isize
    }

    /// Skips to the next multiple of 32 bits.
    pub fn align32(&mut self) {
        self.pos = (self.pos + 31) & !31;
    }
}
