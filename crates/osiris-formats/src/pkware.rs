//! PKWare Data Compression Library "explode" and "implode", used for compressed chunks
//! in maps and saves. Explode follows the algorithm as documented in Mark Adler's
//! `blast.c`; implode writes the same format back with a greedy LZ77 match search.
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
        return Err(Error::Invalid(
            "pkware: coded literals are not supported".into(),
        ));
    }
    let dict_bits = data[1] as u32;
    if !(4..=6).contains(&dict_bits) {
        return Err(Error::Invalid(format!(
            "pkware: dictionary bits {dict_bits}"
        )));
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
            let dist =
                ((bits.decode(&dist_code)? as usize) << shift) + bits.need(shift)? as usize + 1;
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

/// Dictionary size bits the game's files use (a 4096-byte window).
const IMPLODE_DICT_BITS: u32 = 6;
const MAX_DIST: usize = 1 << (IMPLODE_DICT_BITS + 6);
/// Length-2 matches carry only 2 low distance bits, so they reach back 256 bytes.
const MAX_DIST_LEN2: usize = 256;
const MAX_LEN: usize = 518;
/// The length that marks the end of the stream.
const END_LEN: usize = 519;
const HASH_BITS: u32 = 15;
/// Positions tried per match search; bounds the time spent on long runs.
const MAX_CHAIN: usize = 128;

/// Per-symbol `(code, length)` of a canonical code, as `Bits::decode` reads it.
fn encode_table(rep: &[u8]) -> Vec<(u16, u8)> {
    let h = Huffman::new(rep);
    let mut table = vec![(0u16, 0u8); h.symbol.len()];
    let (mut code, mut index) = (0u16, 0usize);
    for len in 1..=MAX_BITS {
        for _ in 0..h.count[len] {
            table[h.symbol[index] as usize] = (code, len as u8);
            code += 1;
            index += 1;
        }
        code <<= 1;
    }
    table
}

struct BitWriter {
    out: Vec<u8>,
    buf: u32,
    cnt: u32,
}

impl BitWriter {
    /// Writes the low `n` bits of `v`, least significant first.
    fn put(&mut self, n: u32, v: u32) {
        self.buf |= (v & ((1u32 << n) - 1)) << self.cnt;
        self.cnt += n;
        while self.cnt >= 8 {
            self.out.push(self.buf as u8);
            self.buf >>= 8;
            self.cnt -= 8;
        }
    }

    /// Writes a Huffman code most significant bit first, each bit inverted.
    fn code(&mut self, (code, len): (u16, u8)) {
        for i in (0..len as u32).rev() {
            self.put(1, ((code as u32 >> i) & 1) ^ 1);
        }
    }

    fn finish(mut self) -> Vec<u8> {
        if self.cnt > 0 {
            self.out.push(self.buf as u8);
        }
        self.out
    }
}

fn length_symbol(len: usize) -> usize {
    (0..LENGTH_BASE.len())
        .find(|&s| {
            let base = LENGTH_BASE[s] as usize;
            len >= base && len < base + (1 << LENGTH_EXTRA[s])
        })
        .expect("match length in range")
}

fn hash(data: &[u8], i: usize) -> usize {
    let v = (data[i] as u32) | (data[i + 1] as u32) << 8 | (data[i + 2] as u32) << 16;
    (v.wrapping_mul(0x9E37_79B1) >> (32 - HASH_BITS)) as usize
}

/// Compresses `data` as a PKWare DCL stream in binary mode with a 4096-byte dictionary,
/// the settings of every compressed chunk in the game's own files.
pub fn implode(data: &[u8]) -> Vec<u8> {
    let length_code = encode_table(&LENGTH_CODE);
    let dist_code = encode_table(&DIST_CODE);
    let mut w = BitWriter {
        out: vec![0, IMPLODE_DICT_BITS as u8],
        buf: 0,
        cnt: 0,
    };
    let emit_match = |w: &mut BitWriter, len: usize, dist: usize| {
        let sym = length_symbol(len);
        w.put(1, 1);
        w.code(length_code[sym]);
        w.put(LENGTH_EXTRA[sym] as u32, (len - LENGTH_BASE[sym] as usize) as u32);
        if len == END_LEN {
            return;
        }
        let shift = if len == 2 { 2 } else { IMPLODE_DICT_BITS };
        let d = dist - 1;
        w.code(dist_code[d >> shift]);
        w.put(shift, d as u32);
    };

    // Hash chains over 3-byte prefixes; `prev` is indexed by position within the window.
    let mut head = vec![usize::MAX; 1 << HASH_BITS];
    let mut prev = vec![usize::MAX; MAX_DIST];
    let insert = |head: &mut [usize], prev: &mut [usize], i: usize| {
        if i + 3 <= data.len() {
            let h = hash(data, i);
            prev[i % MAX_DIST] = head[h];
            head[h] = i;
        }
    };
    let mut i = 0;
    while i < data.len() {
        let max_len = MAX_LEN.min(data.len() - i);
        let (mut best_len, mut best_dist) = (0, 0);
        if max_len >= 3 {
            let mut cand = head[hash(data, i)];
            let mut tries = 0;
            while cand != usize::MAX && i - cand <= MAX_DIST && tries < MAX_CHAIN {
                let len = data[cand..]
                    .iter()
                    .zip(&data[i..i + max_len])
                    .take_while(|(a, b)| a == b)
                    .count();
                if len > best_len {
                    (best_len, best_dist) = (len, i - cand);
                    if len == max_len {
                        break;
                    }
                }
                let next = prev[cand % MAX_DIST];
                if next == usize::MAX || next >= cand {
                    break;
                }
                cand = next;
                tries += 1;
            }
        }
        if best_len < 3 {
            // A hash collision's short match may lie beyond a pair's reach.
            best_len = 0;
        }
        if best_len == 0 && max_len >= 2 {
            // A near pair still beats two literals.
            let lo = i.saturating_sub(MAX_DIST_LEN2);
            if let Some(p) = (lo..i).rev().find(|&p| data[p] == data[i] && data[p + 1] == data[i + 1]) {
                (best_len, best_dist) = (2, i - p);
            }
        }
        if best_len >= 2 {
            emit_match(&mut w, best_len, best_dist);
            for p in i..i + best_len {
                insert(&mut head, &mut prev, p);
            }
            i += best_len;
        } else {
            w.put(1, 0);
            w.put(8, data[i] as u32);
            insert(&mut head, &mut prev, i);
            i += 1;
        }
    }
    emit_match(&mut w, END_LEN, 0);
    w.finish()
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

    #[test]
    fn implode_round_trips() {
        // A small xorshift generator: no dependency, reproducible.
        let mut state = 0x2545_F491_4F6C_DD1Du64;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        let mut cases: Vec<Vec<u8>> = vec![
            Vec::new(),
            vec![7],
            vec![0; 1],
            vec![0; 2],
            vec![0; 519],
            vec![0; 100_000],
            b"AIAIAIAIAIAIA".to_vec(),
            (0..=255u8).cycle().take(20_000).collect(),
        ];
        for n in [3usize, 17, 300, 4097, 70_000] {
            // Pure noise, then noise over a small alphabet (lots of short matches), then
            // runs and repeats at every distance up to past the window.
            cases.push((0..n).map(|_| next() as u8).collect());
            cases.push((0..n).map(|_| (next() % 4) as u8).collect());
            let mut v = Vec::with_capacity(n);
            while v.len() < n {
                let r = next();
                match r % 3 {
                    0 => v.extend(std::iter::repeat_n(r as u8 >> 3, (r >> 8) as usize % 600)),
                    1 if !v.is_empty() => {
                        let dist = 1 + (r >> 8) as usize % v.len().min(5000);
                        let len = (r >> 24) as usize % 700;
                        for _ in 0..len {
                            v.push(v[v.len() - dist]);
                        }
                    }
                    _ => v.extend((0..(r >> 8) as usize % 40).map(|_| next() as u8)),
                }
            }
            v.truncate(n);
            cases.push(v);
        }
        for data in cases {
            let packed = implode(&data);
            assert_eq!(&packed[..2], &[0, 6]);
            assert_eq!(explode(&packed, data.len()).unwrap(), data, "{} bytes", data.len());
        }
    }

    #[test]
    fn implode_compresses_repeats() {
        let packed = implode(&vec![0u8; 51984]);
        assert!(packed.len() < 1000, "{}", packed.len());
    }
}
