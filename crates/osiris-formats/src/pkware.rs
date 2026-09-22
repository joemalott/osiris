//! PKWare Data Compression Library "explode", used for compressed chunks in maps and
//! saves. Follows the algorithm as documented in Mark Adler's `blast.c`.
//!
//! The stream starts with two bytes: literal mode (0 = raw bytes; 1 = Huffman-coded,
//! which the game never writes and we reject) and dictionary size bits (4, 5 or 6).
//! Bits are read least-significant first; Huffman codes are stored inverted.

use crate::{Error, Result};

const MAX_BITS: usize = 13;

struct Huffman {
    count: [u16; MAX_BITS + 1],
    symbol: Vec<u16>,
}

impl Huffman {
    /// Builds a canonical code from run-length compressed code lengths: each byte is
    /// `(repeat - 1) << 4 | length`.
    fn new(rep: &[u8]) -> Self {
        let mut lengths = Vec::new();
        for &b in rep {
            let len = b & 15;
            for _ in 0..=(b >> 4) {
                lengths.push(len as usize);
            }
        }
        let mut count = [0u16; MAX_BITS + 1];
        for &l in &lengths {
            count[l] += 1;
        }
        let mut offs = [0u16; MAX_BITS + 1];
        for len in 1..MAX_BITS {
            offs[len + 1] = offs[len] + count[len];
        }
        let mut symbol = vec![0u16; lengths.len()];
        for (sym, &l) in lengths.iter().enumerate() {
            symbol[offs[l] as usize] = sym as u16;
            offs[l] += 1;
        }
        Self { count, symbol }
    }

    /// True if the code lengths describe a complete prefix code.
    #[cfg(test)]
    fn is_complete(&self) -> bool {
        let mut left: i32 = 1;
        for len in 1..=MAX_BITS {
            left = (left << 1) - self.count[len] as i32;
            if left < 0 {
                return false;
            }
        }
        left == 0
    }
}

const LENGTH_CODE: [u8; 6] = [2, 35, 36, 53, 38, 23];
const DIST_CODE: [u8; 7] = [2, 20, 53, 230, 247, 151, 248];
const LENGTH_BASE: [u16; 16] = [3, 2, 4, 5, 6, 7, 8, 9, 10, 12, 16, 24, 40, 72, 136, 264];
const LENGTH_EXTRA: [u8; 16] = [0, 0, 0, 0, 0, 0, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8];

struct Bits<'a> {
    data: &'a [u8],
    pos: usize,
    buf: u32,
    cnt: u32,
}

impl Bits<'_> {
    fn need(&mut self, n: u32) -> Result<u32> {
        while self.cnt < n {
            let b = *self
                .data
                .get(self.pos)
                .ok_or(Error::Truncated("pkware stream"))?;
            self.pos += 1;
            self.buf |= (b as u32) << self.cnt;
            self.cnt += 8;
        }
        let v = self.buf & ((1u32 << n) - 1);
        self.buf >>= n;
        self.cnt -= n;
        Ok(v)
    }

    fn decode(&mut self, h: &Huffman) -> Result<u16> {
        let (mut code, mut first, mut index) = (0i32, 0i32, 0i32);
        for len in 1..=MAX_BITS {
            code |= (self.need(1)? ^ 1) as i32;
            let count = h.count[len] as i32;
            if code - first < count {
                return Ok(h.symbol[(index + code - first) as usize]);
            }
            index += count;
            first = (first + count) << 1;
            code <<= 1;
        }
        Err(Error::Invalid("pkware: bad huffman code".into()))
    }
}

/// Decompresses `data`, which must expand to exactly `expected_len` bytes.
pub fn explode(data: &[u8], expected_len: usize) -> Result<Vec<u8>> {
    if data.len() < 2 {
        return Err(Error::Truncated("pkware header"));
    }
    if data[0] != 0 {
        return Err(Error::Invalid("pkware: coded literals are not supported".into()));
    }
    let dict_bits = data[1] as u32;
    if !(4..=6).contains(&dict_bits) {
        return Err(Error::Invalid(format!("pkware: dictionary bits {dict_bits}")));
    }
    let length_code = Huffman::new(&LENGTH_CODE);
    let dist_code = Huffman::new(&DIST_CODE);
    let mut bits = Bits {
        data: &data[2..],
        pos: 0,
        buf: 0,
        cnt: 0,
    };
    let mut out = Vec::with_capacity(expected_len);
    loop {
        if bits.need(1)? == 1 {
            let sym = bits.decode(&length_code)? as usize;
            let len = LENGTH_BASE[sym] as usize + bits.need(LENGTH_EXTRA[sym] as u32)? as usize;
            if len == 519 {
                break;
            }
            let shift = if len == 2 { 2 } else { dict_bits };
            let dist = ((bits.decode(&dist_code)? as usize) << shift)
                + bits.need(shift)? as usize
                + 1;
            if dist > out.len() {
                return Err(Error::Invalid("pkware: distance before start".into()));
            }
            let start = out.len() - dist;
            for i in 0..len {
                out.push(out[start + i]);
            }
        } else {
            out.push(bits.need(8)? as u8);
        }
        if out.len() > expected_len {
            return Err(Error::Invalid("pkware: output overrun".into()));
        }
    }
    if out.len() != expected_len {
        return Err(Error::Invalid(format!(
            "pkware: expanded to {} bytes, expected {expected_len}",
            out.len()
        )));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_tables_are_complete() {
        assert!(Huffman::new(&LENGTH_CODE).is_complete());
        assert!(Huffman::new(&DIST_CODE).is_complete());
        assert_eq!(Huffman::new(&LENGTH_CODE).symbol.len(), 16);
        assert_eq!(Huffman::new(&DIST_CODE).symbol.len(), 64);
    }

    #[test]
    fn blast_reference_vector() {
        // The test vector from blast.c: decompresses to "AIAIAIAIAIAIA".
        let data = [0x00, 0x04, 0x82, 0x24, 0x25, 0x8f, 0x80, 0x7f];
        assert_eq!(explode(&data, 13).unwrap(), b"AIAIAIAIAIAIA");
    }
}
