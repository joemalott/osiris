//! Bink 1 video decoding (revisions 'c' to 'k'; the old 'b' revision is not supported).
//!
//! Ported from FFmpeg's libavcodec/bink.c (Copyright (c) 2009 Konstantin Shishkov,
//! Copyright (C) 2011 Peter Ross) and libavcodec/binkdsp.c (Copyright (c) 2009 Konstantin
//! Shishkov), licensed LGPL-2.1-or-later and used here under
//! GPL-3.0-or-later, as the LGPL allows.
//!
//! A frame is three (or, with alpha, four) planes of 8x8 blocks, chroma at half size.
//! Each plane is coded a block row at a time: first nine "bundles" of values (block
//! types, colours, patterns, motion vectors, DC values, run lengths), each read with its
//! own 16-symbol Huffman tree, then the blocks themselves, which take values from the
//! bundles and read the rest (DCT coefficients, residues) straight from the bitstream.
//! Pixel arithmetic wraps as the reference's 8-bit stores do.

use std::sync::OnceLock;

use crate::bits::Bits;
use crate::tables::{INTER_QUANT, INTRA_QUANT, PATTERNS, SCAN, TREE_BITS, TREE_LENS};
use crate::{Error, Result};

const FLAG_ALPHA: u32 = 0x0010_0000;

const SRC_BLOCK_TYPES: usize = 0;
const SRC_SUB_BLOCK_TYPES: usize = 1;
const SRC_COLORS: usize = 2;
const SRC_PATTERN: usize = 3;
const SRC_X_OFF: usize = 4;
const SRC_Y_OFF: usize = 5;
const SRC_INTRA_DC: usize = 6;
const SRC_INTER_DC: usize = 7;
const SRC_RUN: usize = 8;
const NB_SRC: usize = 9;

const SKIP_BLOCK: i32 = 0;
const SCALED_BLOCK: i32 = 1;
const MOTION_BLOCK: i32 = 2;
const RUN_BLOCK: i32 = 3;
const RESIDUE_BLOCK: i32 = 4;
const INTRA_BLOCK: i32 = 5;
const FILL_BLOCK: i32 = 6;
const INTER_BLOCK: i32 = 7;
const PATTERN_BLOCK: i32 = 8;
const RAW_BLOCK: i32 = 9;

const RLE_LENS: [usize; 4] = [4, 8, 12, 32];

/// Bits in the first DC value of a bundle.
const DC_START_BITS: u32 = 11;

/// A lookup table for one of the sixteen fixed Huffman trees: indexed by the next
/// `bits` bits of the stream, it gives the leaf and its code length.
struct Vlc {
    bits: u32,
    table: Vec<(u8, u8)>,
}

fn vlcs() -> &'static [Vlc; 16] {
    static VLCS: OnceLock<[Vlc; 16]> = OnceLock::new();
    VLCS.get_or_init(|| {
        std::array::from_fn(|t| {
            let bits = TREE_LENS[t][15] as u32;
            let mut table = vec![(0u8, 0u8); 1 << bits];
            for leaf in 0..16 {
                let (code, len) = (TREE_BITS[t][leaf] as usize, TREE_LENS[t][leaf] as u32);
                for high in 0..1usize << (bits - len) {
                    table[code | high << len] = (leaf as u8, len as u8);
                }
            }
            Vlc { bits, table }
        })
    })
}

/// A Huffman tree as a frame uses it: which fixed tree, and which symbol each leaf is.
#[derive(Clone, Copy, Default)]
struct Tree {
    vlc: usize,
    syms: [u8; 16],
}

impl Tree {
    #[inline]
    fn get(&self, gb: &mut Bits) -> i32 {
        let vlc = &vlcs()[self.vlc];
        let (leaf, len) = vlc.table[gb.peek(vlc.bits) as usize];
        gb.skip(len as usize);
        self.syms[leaf as usize] as i32
    }
}

/// Decoded values of one data type, filled a block row at a time and read back by the
/// blocks. The reference keeps bytes (or 16-bit DC values); values are stored here
/// already narrowed the way it narrows them.
#[derive(Default)]
struct Bundle {
    /// Bits in the count of values the next read adds.
    len: u32,
    tree: Tree,
    data: Vec<i16>,
    /// Where decoding continues, or `None` once the plane's values are all read.
    cur_dec: Option<usize>,
    /// The next value a block takes.
    cur_ptr: usize,
}

impl Bundle {
    #[inline]
    fn next(&mut self) -> i32 {
        let v = self.data.get(self.cur_ptr).copied().unwrap_or(0);
        self.cur_ptr += 1;
        v as i32
    }

    /// The count of values to read now, or `None` when this row needs none: the values
    /// already decoded aren't used up yet, or the plane's values have all been read.
    fn check_read(&mut self, gb: &mut Bits) -> Option<usize> {
        let dec = self.cur_dec?;
        if dec > self.cur_ptr {
            return None;
        }
        let t = gb.get(self.len) as usize;
        if t == 0 {
            self.cur_dec = None;
            return None;
        }
        Some(t)
    }
}

/// One plane of a decoded frame, with room around it for the blocks that overhang.
#[derive(Clone)]
pub struct Plane {
    pub data: Vec<u8>,
    pub stride: usize,
    pub width: usize,
    pub height: usize,
}

impl Plane {
    fn new(width: usize, height: usize, blocks_w: usize, blocks_h: usize) -> Plane {
        // As wide as FFmpeg's frame pool makes it, so that a motion vector reaching past
        // the right edge wraps to the same pixels; 16x16 blocks may overhang by 8.
        let stride = (blocks_w * 8 + 8).next_multiple_of(32);
        Plane { data: vec![0; stride * (blocks_h * 8 + 16)], stride, width, height }
    }

    /// The visible rows, `width` bytes each.
    pub fn rows(&self) -> impl Iterator<Item = &[u8]> {
        self.data.chunks(self.stride).take(self.height).map(|r| &r[..self.width])
    }
}

pub struct VideoDecoder {
    width: usize,
    height: usize,
    version: u8,
    has_alpha: bool,
    swap_planes: bool,
    /// The frame being decoded and the one before it (planes Y, U, V and alpha).
    pub(crate) cur: Vec<Plane>,
    last: Vec<Plane>,
    bundles: [Bundle; NB_SRC],
    col_high: [Tree; 16],
    col_lastval: i32,
}

impl VideoDecoder {
    pub fn new(width: u32, height: u32, revision: u8, flags: u32) -> Result<VideoDecoder> {
        if revision <= b'b' {
            return Err(Error::Unsupported("Bink revision 'b' video"));
        }
        let (width, height) = (width as usize, height as usize);
        let has_alpha = flags & FLAG_ALPHA != 0;
        let (bw, bh) = (width.div_ceil(8), height.div_ceil(8));
        let (cbw, cbh) = (width.div_ceil(16), height.div_ceil(16));
        let mut planes = vec![
            Plane::new(width, height, bw, bh),
            Plane::new(width.div_ceil(2), height.div_ceil(2), cbw, cbh),
            Plane::new(width.div_ceil(2), height.div_ceil(2), cbw, cbh),
        ];
        if has_alpha {
            planes.push(Plane::new(width, height, bw, bh));
        }
        let mut bundles: [Bundle; NB_SRC] = Default::default();
        for b in &mut bundles {
            b.data = vec![0; bw * bh * 64];
        }
        Ok(VideoDecoder {
            width,
            height,
            version: revision,
            has_alpha,
            swap_planes: revision >= b'h',
            last: planes.clone(),
            cur: planes,
            bundles,
            col_high: [Tree::default(); 16],
            col_lastval: 0,
        })
    }

    /// Decodes a frame's video packet (`len` bytes, followed by padding).
    pub fn decode(&mut self, packet: &[u8], len: usize) -> Result<()> {
        std::mem::swap(&mut self.cur, &mut self.last);
        let mut gb = Bits::new(packet, len);
        let bits_count = len * 8;
        if self.has_alpha {
            if self.version >= b'i' {
                gb.skip(32);
            }
            self.decode_plane(&mut gb, 3, false)?;
        }
        if self.version >= b'i' {
            gb.skip(32);
        }
        for plane in 0..3 {
            let idx = if plane == 0 || !self.swap_planes { plane } else { plane ^ 3 };
            self.decode_plane(&mut gb, idx, plane != 0)?;
            if gb.count() >= bits_count {
                break;
            }
        }
        Ok(())
    }

    fn init_lengths(&mut self, width: usize, bw: usize) {
        let width = width.next_multiple_of(8);
        let log2 = |v: usize| usize::BITS - 1 - v.leading_zeros();
        let b = &mut self.bundles;
        b[SRC_BLOCK_TYPES].len = log2((width >> 3) + 511) + 1;
        b[SRC_SUB_BLOCK_TYPES].len = log2((width >> 4) + 511) + 1;
        b[SRC_COLORS].len = log2(bw * 64 + 511) + 1;
        for i in [SRC_INTRA_DC, SRC_INTER_DC, SRC_X_OFF, SRC_Y_OFF] {
            b[i].len = log2((width >> 3) + 511) + 1;
        }
        b[SRC_PATTERN].len = log2((bw << 3) + 511) + 1;
        b[SRC_RUN].len = log2(bw * 48 + 511) + 1;
    }

    fn read_bundle(&mut self, gb: &mut Bits, n: usize) -> Result<()> {
        if n == SRC_COLORS {
            for i in 0..16 {
                self.col_high[i] = read_tree(gb)?;
            }
            self.col_lastval = 0;
        }
        if n != SRC_INTRA_DC && n != SRC_INTER_DC {
            self.bundles[n].tree = read_tree(gb)?;
        }
        self.bundles[n].cur_dec = Some(0);
        self.bundles[n].cur_ptr = 0;
        Ok(())
    }

    fn read_block_types(&mut self, gb: &mut Bits, n: usize) -> Result<()> {
        let version = self.version;
        let b = &mut self.bundles[n];
        let Some(mut t) = b.check_read(gb) else { return Ok(()) };
        if version == b'k' {
            t ^= 0xBB;
            if t == 0 {
                b.cur_dec = None;
                return Ok(());
            }
        }
        let mut dec = b.cur_dec.unwrap();
        let end = dec + t;
        if end > b.data.len() {
            return Err(Error::Invalid("too many block type values"));
        }
        if gb.left() < 1 {
            return Err(Error::Invalid("block types overread"));
        }
        if gb.bit() {
            let v = gb.get(4) as i16;
            b.data[dec..end].fill(v);
            dec = end;
        } else {
            let mut last = 0;
            while dec < end {
                let v = b.tree.get(gb);
                if v < 12 {
                    last = v as i16;
                    b.data[dec] = last;
                    dec += 1;
                } else {
                    let run = RLE_LENS[(v - 12) as usize];
                    if end - dec < run {
                        return Err(Error::Invalid("block type run too long"));
                    }
                    b.data[dec..dec + run].fill(last);
                    dec += run;
                }
            }
        }
        b.cur_dec = Some(dec);
        Ok(())
    }

    fn read_colors(&mut self, gb: &mut Bits) -> Result<()> {
        let old = self.version < b'i';
        let b = &mut self.bundles[SRC_COLORS];
        let Some(t) = b.check_read(gb) else { return Ok(()) };
        let mut dec = b.cur_dec.unwrap();
        let end = dec + t;
        if end > b.data.len() {
            return Err(Error::Invalid("too many colour values"));
        }
        if gb.left() < 1 {
            return Err(Error::Invalid("colours overread"));
        }
        let col_high = &self.col_high;
        let lastval = &mut self.col_lastval;
        let mut one = |gb: &mut Bits| {
            *lastval = col_high[*lastval as usize].get(gb);
            let mut v = (*lastval << 4) | b.tree.get(gb);
            if old {
                let sign = ((v as u8 as i8) >> 7) as i32;
                v = ((v & 0x7F) ^ sign) - sign;
                v += 0x80;
            }
            v as u8 as i16
        };
        if gb.bit() {
            let v = one(gb);
            b.data[dec..end].fill(v);
            dec = end;
        } else {
            while dec < end {
                if gb.left() < 2 {
                    return Err(Error::Invalid("colours overread"));
                }
                b.data[dec] = one(gb);
                dec += 1;
            }
        }
        b.cur_dec = Some(dec);
        Ok(())
    }

    fn read_patterns(&mut self, gb: &mut Bits) -> Result<()> {
        let b = &mut self.bundles[SRC_PATTERN];
        let Some(t) = b.check_read(gb) else { return Ok(()) };
        let mut dec = b.cur_dec.unwrap();
        let end = dec + t;
        if end > b.data.len() {
            return Err(Error::Invalid("too many pattern values"));
        }
        while dec < end {
            if gb.left() < 2 {
                return Err(Error::Invalid("patterns overread"));
            }
            let lo = b.tree.get(gb);
            let hi = b.tree.get(gb);
            b.data[dec] = (lo | hi << 4) as u8 as i16;
            dec += 1;
        }
        b.cur_dec = Some(dec);
        Ok(())
    }

    fn read_motion_values(&mut self, gb: &mut Bits, n: usize) -> Result<()> {
        let b = &mut self.bundles[n];
        let Some(t) = b.check_read(gb) else { return Ok(()) };
        let mut dec = b.cur_dec.unwrap();
        let end = dec + t;
        if end > b.data.len() {
            return Err(Error::Invalid("too many motion values"));
        }
        if gb.left() < 1 {
            return Err(Error::Invalid("motion values overread"));
        }
        if gb.bit() {
            let mut v = gb.get(4) as i32;
            if v != 0 && gb.bit() {
                v = -v;
            }
            b.data[dec..end].fill(v as i8 as i16);
            dec = end;
        } else {
            while dec < end {
                let mut v = b.tree.get(gb);
                if v != 0 && gb.bit() {
                    v = -v;
                }
                b.data[dec] = v as i8 as i16;
                dec += 1;
            }
        }
        b.cur_dec = Some(dec);
        Ok(())
    }

    fn read_dcs(&mut self, gb: &mut Bits, n: usize, has_sign: bool) -> Result<()> {
        let b = &mut self.bundles[n];
        let Some(mut len) = b.check_read(gb) else { return Ok(()) };
        let mut dec = b.cur_dec.unwrap();
        let start_bits = DC_START_BITS - has_sign as u32;
        if gb.left() < start_bits as isize {
            return Err(Error::Invalid("DC values overread"));
        }
        let mut v = gb.get(start_bits) as i32;
        if v != 0 && has_sign && gb.bit() {
            v = -v;
        }
        if dec >= b.data.len() {
            return Err(Error::Invalid("too many DC values"));
        }
        b.data[dec] = v as i16;
        dec += 1;
        len -= 1;
        let mut i = 0;
        while i < len {
            let len2 = (len - i).min(8);
            if b.data.len() - dec < len2 {
                return Err(Error::Invalid("too many DC values"));
            }
            let bsize = gb.get(4);
            if bsize != 0 {
                for _ in 0..len2 {
                    let mut v2 = gb.get(bsize) as i32;
                    if v2 != 0 && gb.bit() {
                        v2 = -v2;
                    }
                    v += v2;
                    b.data[dec] = v as i16;
                    dec += 1;
                    if !(-32768..=32767).contains(&v) {
                        return Err(Error::Invalid("DC value out of bounds"));
                    }
                }
            } else {
                b.data[dec..dec + len2].fill(v as i16);
                dec += len2;
            }
            i += 8;
        }
        b.cur_dec = Some(dec);
        Ok(())
    }

    fn read_runs(&mut self, gb: &mut Bits) -> Result<()> {
        let b = &mut self.bundles[SRC_RUN];
        let Some(t) = b.check_read(gb) else { return Ok(()) };
        let mut dec = b.cur_dec.unwrap();
        let end = dec + t;
        if end > b.data.len() {
            return Err(Error::Invalid("run value out of bounds"));
        }
        if gb.left() < 1 {
            return Err(Error::Invalid("runs overread"));
        }
        if gb.bit() {
            let v = gb.get(4) as i16;
            b.data[dec..end].fill(v);
            dec = end;
        } else {
            while dec < end {
                b.data[dec] = b.tree.get(gb) as i16;
                dec += 1;
            }
        }
        b.cur_dec = Some(dec);
        Ok(())
    }

    #[inline]
    fn value(&mut self, n: usize) -> i32 {
        self.bundles[n].next()
    }

    fn decode_plane(&mut self, gb: &mut Bits, plane_idx: usize, is_chroma: bool) -> Result<()> {
        let (bw, bh) = if is_chroma {
            (self.width.div_ceil(16), self.height.div_ceil(16))
        } else {
            (self.width.div_ceil(8), self.height.div_ceil(8))
        };
        let width = self.width >> is_chroma as u32;
        let height = self.height >> is_chroma as u32;

        if self.version == b'k' && gb.bit() {
            let fill = gb.get(8) as u8;
            let p = &mut self.cur[plane_idx];
            for row in p.data.chunks_mut(p.stride).take(height) {
                row[..width].fill(fill);
            }
            gb.align32();
            return Ok(());
        }

        self.init_lengths(width.max(8), bw);
        for i in 0..NB_SRC {
            self.read_bundle(gb, i)?;
        }

        let mut dst_plane = std::mem::take(&mut self.cur[plane_idx].data);
        let result = self.decode_blocks(gb, plane_idx, &mut dst_plane, bw, bh);
        self.cur[plane_idx].data = dst_plane;
        result?;
        gb.align32();
        Ok(())
    }

    fn decode_blocks(&mut self, gb: &mut Bits, plane_idx: usize, dst: &mut [u8], bw: usize, bh: usize) -> Result<()> {
        let stride = self.cur[plane_idx].stride;
        let prev = std::mem::take(&mut self.last[plane_idx].data);
        let result = self.decode_blocks_from(gb, dst, &prev, stride, bw, bh);
        self.last[plane_idx].data = prev;
        result
    }

    fn decode_blocks_from(&mut self, gb: &mut Bits, dst: &mut [u8], prev: &[u8], stride: usize, bw: usize, bh: usize) -> Result<()> {
        let ref_end = (bw - 1 + stride * (bh - 1)) * 8;
        let mut coordmap = [0usize; 64];
        for (i, c) in coordmap.iter_mut().enumerate() {
            *c = (i & 7) + (i >> 3) * stride;
        }
        let mut ublock = [0u8; 64];
        let mut block;
        let mut dctblock;
        let mut coef_idx = [0usize; 64];

        for by in 0..bh {
            self.read_block_types(gb, SRC_BLOCK_TYPES)?;
            self.read_block_types(gb, SRC_SUB_BLOCK_TYPES)?;
            self.read_colors(gb)?;
            self.read_patterns(gb)?;
            self.read_motion_values(gb, SRC_X_OFF)?;
            self.read_motion_values(gb, SRC_Y_OFF)?;
            self.read_dcs(gb, SRC_INTRA_DC, false)?;
            self.read_dcs(gb, SRC_INTER_DC, true)?;
            self.read_runs(gb)?;

            let mut bx = 0;
            while bx < bw {
                let off = 8 * by * stride + bx * 8;
                let blk = self.value(SRC_BLOCK_TYPES);
                // A 16x16 block's other three quarters are already drawn.
                if ((by & 1) != 0 || (bx & 1) != 0) && blk == SCALED_BLOCK {
                    bx += 2;
                    continue;
                }
                match blk {
                    SKIP_BLOCK => copy8(dst, off, prev, off, stride),
                    SCALED_BLOCK => {
                        let sub = self.value(SRC_SUB_BLOCK_TYPES);
                        match sub {
                            RUN_BLOCK => {
                                if gb.left() < 4 {
                                    return Err(Error::Invalid("run block overread"));
                                }
                                let scan = &PATTERNS[gb.get(4) as usize];
                                self.read_run_block(gb, scan, |pos, v| ublock[pos] = v)?;
                            }
                            INTRA_BLOCK => {
                                dctblock = [0; 64];
                                dctblock[0] = self.value(SRC_INTRA_DC);
                                let (q, n) = read_dct_coeffs(gb, &mut dctblock, &mut coef_idx)?;
                                unquantize(&mut dctblock, &INTRA_QUANT[q], &coef_idx[..n]);
                                idct_put(&mut ublock, 0, 8, &dctblock);
                            }
                            FILL_BLOCK => {
                                let v = self.value(SRC_COLORS) as u8;
                                for y in 0..16 {
                                    dst[off + y * stride..off + y * stride + 16].fill(v);
                                }
                            }
                            PATTERN_BLOCK => {
                                let col = [self.value(SRC_COLORS) as u8, self.value(SRC_COLORS) as u8];
                                for j in 0..8 {
                                    let mut v = self.value(SRC_PATTERN);
                                    for i in 0..8 {
                                        ublock[i + j * 8] = col[(v & 1) as usize];
                                        v >>= 1;
                                    }
                                }
                            }
                            RAW_BLOCK => {
                                for p in ublock.iter_mut() {
                                    *p = self.value(SRC_COLORS) as u8;
                                }
                            }
                            _ => return Err(Error::Invalid("bad 16x16 block type")),
                        }
                        if sub != FILL_BLOCK {
                            for j in 0..8 {
                                for i in 0..8 {
                                    let v = ublock[j * 8 + i];
                                    let o = off + 2 * j * stride + 2 * i;
                                    dst[o] = v;
                                    dst[o + 1] = v;
                                    dst[o + stride] = v;
                                    dst[o + stride + 1] = v;
                                }
                            }
                        }
                        bx += 1;
                    }
                    MOTION_BLOCK => self.motion(dst, prev, off, stride, ref_end)?,
                    RUN_BLOCK => {
                        let scan = &PATTERNS[gb.get(4) as usize];
                        self.read_run_block(gb, scan, |pos, v| dst[off + coordmap[pos]] = v)?;
                    }
                    RESIDUE_BLOCK => {
                        self.motion(dst, prev, off, stride, ref_end)?;
                        block = [0; 64];
                        let masks = gb.get(7) as i32;
                        read_residue(gb, &mut block, masks);
                        for y in 0..8 {
                            for x in 0..8 {
                                let p = &mut dst[off + y * stride + x];
                                *p = p.wrapping_add(block[y * 8 + x] as u8);
                            }
                        }
                    }
                    INTRA_BLOCK => {
                        dctblock = [0; 64];
                        dctblock[0] = self.value(SRC_INTRA_DC);
                        let (q, n) = read_dct_coeffs(gb, &mut dctblock, &mut coef_idx)?;
                        unquantize(&mut dctblock, &INTRA_QUANT[q], &coef_idx[..n]);
                        idct_put(dst, off, stride, &dctblock);
                    }
                    FILL_BLOCK => {
                        let v = self.value(SRC_COLORS) as u8;
                        for y in 0..8 {
                            dst[off + y * stride..off + y * stride + 8].fill(v);
                        }
                    }
                    INTER_BLOCK => {
                        self.motion(dst, prev, off, stride, ref_end)?;
                        dctblock = [0; 64];
                        dctblock[0] = self.value(SRC_INTER_DC);
                        let (q, n) = read_dct_coeffs(gb, &mut dctblock, &mut coef_idx)?;
                        unquantize(&mut dctblock, &INTER_QUANT[q], &coef_idx[..n]);
                        idct_add(dst, off, stride, &mut dctblock);
                    }
                    PATTERN_BLOCK => {
                        let col = [self.value(SRC_COLORS) as u8, self.value(SRC_COLORS) as u8];
                        for i in 0..8 {
                            let mut v = self.value(SRC_PATTERN);
                            for j in 0..8 {
                                dst[off + i * stride + j] = col[(v & 1) as usize];
                                v >>= 1;
                            }
                        }
                    }
                    RAW_BLOCK => {
                        for y in 0..8 {
                            for x in 0..8 {
                                dst[off + y * stride + x] = self.value(SRC_COLORS) as u8;
                            }
                        }
                    }
                    _ => return Err(Error::Invalid("bad block type")),
                }
                bx += 1;
            }
        }
        Ok(())
    }

    /// Copies the 8x8 block a motion vector points at in the previous frame.
    fn motion(&mut self, dst: &mut [u8], prev: &[u8], off: usize, stride: usize, ref_end: usize) -> Result<()> {
        let xoff = self.value(SRC_X_OFF) as isize;
        let yoff = self.value(SRC_Y_OFF) as isize;
        let r = off as isize + xoff + yoff * stride as isize;
        if r < 0 || r as usize > ref_end {
            return Err(Error::Invalid("motion vector out of bounds"));
        }
        copy8(dst, off, prev, r as usize, stride);
        Ok(())
    }

    /// Fills a block from runs of colours along one of the sixteen scan patterns.
    fn read_run_block(&mut self, gb: &mut Bits, scan: &[u8; 64], mut put: impl FnMut(usize, u8)) -> Result<()> {
        let mut i = 0;
        let mut s = 0;
        loop {
            let run = self.value(SRC_RUN) as usize + 1;
            i += run;
            if i > 64 {
                return Err(Error::Invalid("run out of bounds"));
            }
            if gb.bit() {
                let v = self.value(SRC_COLORS) as u8;
                for _ in 0..run {
                    put(scan[s] as usize, v);
                    s += 1;
                }
            } else {
                for _ in 0..run {
                    let v = self.value(SRC_COLORS) as u8;
                    put(scan[s] as usize, v);
                    s += 1;
                }
            }
            if i >= 63 {
                break;
            }
        }
        if i == 63 {
            let v = self.value(SRC_COLORS) as u8;
            put(scan[s] as usize, v);
        }
        Ok(())
    }
}

fn copy8(dst: &mut [u8], doff: usize, src: &[u8], soff: usize, stride: usize) {
    for y in 0..8 {
        let (d, s) = (doff + y * stride, soff + y * stride);
        dst[d..d + 8].copy_from_slice(&src[s..s + 8]);
    }
}

/// Reads which of the sixteen fixed trees a bundle uses and how its leaves map to
/// symbols: either a few symbols listed first and the rest in order, or a shuffle made
/// by merging ever larger runs.
fn read_tree(gb: &mut Bits) -> Result<Tree> {
    if gb.left() < 4 {
        return Err(Error::Invalid("tree overread"));
    }
    let mut tree = Tree { vlc: gb.get(4) as usize, syms: [0; 16] };
    if tree.vlc == 0 {
        for (i, s) in tree.syms.iter_mut().enumerate() {
            *s = i as u8;
        }
        return Ok(tree);
    }
    if gb.bit() {
        let mut len = gb.get(3) as usize;
        let mut seen = [false; 16];
        for i in 0..=len {
            tree.syms[i] = gb.get(4) as u8;
            seen[tree.syms[i] as usize] = true;
        }
        for (i, &seen) in seen.iter().enumerate() {
            if len >= 15 {
                break;
            }
            if !seen {
                len += 1;
                tree.syms[len] = i as u8;
            }
        }
    } else {
        let len = gb.get(2) as usize;
        let mut a: [u8; 16] = std::array::from_fn(|i| i as u8);
        let mut b = [0u8; 16];
        for i in 0..=len {
            let size = 1 << i;
            let mut t = 0;
            while t < 16 {
                merge(gb, &mut b[t..t + 2 * size], &a[t..t + 2 * size], size);
                t += size << 1;
            }
            std::mem::swap(&mut a, &mut b);
        }
        tree.syms = a;
    }
    Ok(tree)
}

/// Merges two lists of `size` (the halves of `src`) into `dst`, a bit choosing each step.
fn merge(gb: &mut Bits, dst: &mut [u8], src: &[u8], size: usize) {
    let (mut i1, mut i2) = (0, size);
    let (mut n1, mut n2) = (size, size);
    let mut d = 0;
    loop {
        if !gb.bit() {
            dst[d] = src[i1];
            i1 += 1;
            n1 -= 1;
        } else {
            dst[d] = src[i2];
            i2 += 1;
            n2 -= 1;
        }
        d += 1;
        if n1 == 0 || n2 == 0 {
            break;
        }
    }
    while n1 > 0 {
        dst[d] = src[i1];
        d += 1;
        i1 += 1;
        n1 -= 1;
    }
    while n2 > 0 {
        dst[d] = src[i2];
        d += 1;
        i2 += 1;
        n2 -= 1;
    }
}

#[inline]
fn coef_value(gb: &mut Bits, bits: i32) -> i32 {
    if bits == 0 {
        1 - ((gb.get(1) as i32) << 1)
    } else {
        let t = (gb.get(bits as u32) | 1 << bits) as i32;
        if gb.bit() { -t } else { t }
    }
}

/// Reads an 8x8 block's DCT coefficients (besides the DC), coded from the highest bit
/// plane down with the coefficients split into ever smaller groups. Returns the
/// quantiser index and how many coefficients were set (their scan positions go to
/// `coef_idx`).
fn read_dct_coeffs(gb: &mut Bits, block: &mut [i32; 64], coef_idx: &mut [usize; 64]) -> Result<(usize, usize)> {
    let mut coef_list = [0i32; 128];
    let mut mode_list = [0u8; 128];
    let (mut list_start, mut list_end) = (64usize, 64usize);
    let mut coef_count = 0;
    if gb.left() < 4 {
        return Err(Error::Invalid("DCT overread"));
    }
    for (c, m) in [(4, 0), (24, 0), (44, 0), (1, 3), (2, 3), (3, 3)] {
        coef_list[list_end] = c;
        mode_list[list_end] = m;
        list_end += 1;
    }
    let mut bits = gb.get(4) as i32 - 1;
    while bits >= 0 {
        let mut list_pos = list_start;
        while list_pos < list_end {
            if (mode_list[list_pos] as i32 | coef_list[list_pos]) == 0 || !gb.bit() {
                list_pos += 1;
                continue;
            }
            let mut ccoef = coef_list[list_pos];
            let mode = mode_list[list_pos];
            match mode {
                0 | 2 => {
                    if mode == 0 {
                        coef_list[list_pos] = ccoef + 4;
                        mode_list[list_pos] = 1;
                    } else {
                        coef_list[list_pos] = 0;
                        mode_list[list_pos] = 0;
                        list_pos += 1;
                    }
                    for _ in 0..4 {
                        if gb.bit() {
                            list_start -= 1;
                            coef_list[list_start] = ccoef;
                            mode_list[list_start] = 3;
                        } else {
                            let t = coef_value(gb, bits);
                            block[SCAN[ccoef as usize] as usize] = t;
                            coef_idx[coef_count] = ccoef as usize;
                            coef_count += 1;
                        }
                        ccoef += 1;
                    }
                }
                1 => {
                    mode_list[list_pos] = 2;
                    for _ in 0..3 {
                        ccoef += 4;
                        coef_list[list_end] = ccoef;
                        mode_list[list_end] = 2;
                        list_end += 1;
                    }
                }
                _ => {
                    let t = coef_value(gb, bits);
                    block[SCAN[ccoef as usize] as usize] = t;
                    coef_idx[coef_count] = ccoef as usize;
                    coef_count += 1;
                    coef_list[list_pos] = 0;
                    mode_list[list_pos] = 0;
                    list_pos += 1;
                }
            }
        }
        bits -= 1;
    }
    Ok((gb.get(4) as usize, coef_count))
}

fn unquantize(block: &mut [i32; 64], quant: &[u32; 64], coef_idx: &[usize]) {
    block[0] = ((block[0] as u32).wrapping_mul(quant[0]) as i32) >> 11;
    for &idx in coef_idx {
        let p = SCAN[idx] as usize;
        block[p] = ((block[p] as u32).wrapping_mul(quant[idx]) as i32) >> 11;
    }
}

/// Reads the difference a residue block adds to its motion-copied pixels, from the
/// highest bit plane down (`masks_count` limits how many bits are set).
fn read_residue(gb: &mut Bits, block: &mut [i16; 64], mut masks_count: i32) {
    let mut coef_list = [0i32; 128];
    let mut mode_list = [0u8; 128];
    let (mut list_start, mut list_end) = (64usize, 64usize);
    let mut nz_coeff = [0usize; 64];
    let mut nz_count = 0;
    for (c, m) in [(4, 0), (24, 0), (44, 0), (0, 2)] {
        coef_list[list_end] = c;
        mode_list[list_end] = m;
        list_end += 1;
    }
    let mut mask: i32 = 1 << gb.get(3);
    while mask != 0 {
        for &nz in &nz_coeff[..nz_count] {
            if !gb.bit() {
                continue;
            }
            if block[nz] < 0 {
                block[nz] = block[nz].wrapping_sub(mask as i16);
            } else {
                block[nz] = block[nz].wrapping_add(mask as i16);
            }
            masks_count -= 1;
            if masks_count < 0 {
                return;
            }
        }
        let mut list_pos = list_start;
        while list_pos < list_end {
            if (coef_list[list_pos] | mode_list[list_pos] as i32) == 0 || !gb.bit() {
                list_pos += 1;
                continue;
            }
            let mut ccoef = coef_list[list_pos];
            let mode = mode_list[list_pos];
            match mode {
                0 | 2 => {
                    if mode == 0 {
                        coef_list[list_pos] = ccoef + 4;
                        mode_list[list_pos] = 1;
                    } else {
                        coef_list[list_pos] = 0;
                        mode_list[list_pos] = 0;
                        list_pos += 1;
                    }
                    for _ in 0..4 {
                        if gb.bit() {
                            list_start -= 1;
                            coef_list[list_start] = ccoef;
                            mode_list[list_start] = 3;
                        } else {
                            let p = SCAN[ccoef as usize] as usize;
                            nz_coeff[nz_count] = p;
                            nz_count += 1;
                            block[p] = if gb.bit() { -mask } else { mask } as i16;
                            masks_count -= 1;
                            if masks_count < 0 {
                                return;
                            }
                        }
                        ccoef += 1;
                    }
                }
                1 => {
                    mode_list[list_pos] = 2;
                    for _ in 0..3 {
                        ccoef += 4;
                        coef_list[list_end] = ccoef;
                        mode_list[list_end] = 2;
                        list_end += 1;
                    }
                }
                _ => {
                    let p = SCAN[ccoef as usize] as usize;
                    nz_coeff[nz_count] = p;
                    nz_count += 1;
                    block[p] = if gb.bit() { -mask } else { mask } as i16;
                    coef_list[list_pos] = 0;
                    mode_list[list_pos] = 0;
                    list_pos += 1;
                    masks_count -= 1;
                    if masks_count < 0 {
                        return;
                    }
                }
            }
        }
        mask >>= 1;
    }
}

const A1: i32 = 2896;
const A2: i32 = 2217;
const A3: i32 = 3784;
const A4: i32 = -5352;

#[inline]
fn mul(x: i32, y: i32) -> i32 {
    ((x as u32).wrapping_mul(y as u32) as i32) >> 11
}

/// The Bink IDCT butterfly over eight values `s[0..8]` (taken `step` apart).
#[inline]
fn idct8(s: [i32; 8]) -> [i32; 8] {
    let a0 = s[0].wrapping_add(s[4]);
    let a1 = s[0].wrapping_sub(s[4]);
    let a2 = s[2].wrapping_add(s[6]);
    let a3 = mul(A1, s[2].wrapping_sub(s[6]));
    let a4 = s[5].wrapping_add(s[3]);
    let a5 = s[5].wrapping_sub(s[3]);
    let a6 = s[1].wrapping_add(s[7]);
    let a7 = s[1].wrapping_sub(s[7]);
    let b0 = a4.wrapping_add(a6);
    let b1 = mul(A3, a5.wrapping_add(a7));
    let b2 = mul(A4, a5).wrapping_sub(b0).wrapping_add(b1);
    let b3 = mul(A1, a6.wrapping_sub(a4)).wrapping_sub(b2);
    let b4 = mul(A2, a7).wrapping_add(b3).wrapping_sub(b1);
    [
        a0.wrapping_add(a2).wrapping_add(b0),
        a1.wrapping_add(a3).wrapping_sub(a2).wrapping_add(b2),
        a1.wrapping_sub(a3).wrapping_add(a2).wrapping_add(b3),
        a0.wrapping_sub(a2).wrapping_sub(b4),
        a0.wrapping_sub(a2).wrapping_add(b4),
        a1.wrapping_sub(a3).wrapping_add(a2).wrapping_sub(b3),
        a1.wrapping_add(a3).wrapping_sub(a2).wrapping_sub(b2),
        a0.wrapping_add(a2).wrapping_sub(b0),
    ]
}

/// The column pass, into `temp` (a column that is all zero below the DC is copied).
fn idct_cols(block: &[i32; 64]) -> [i32; 64] {
    let mut temp = [0i32; 64];
    for i in 0..8 {
        let col: [i32; 8] = std::array::from_fn(|k| block[i + 8 * k]);
        let out = if col[1..].iter().all(|&v| v == 0) { [col[0]; 8] } else { idct8(col) };
        for k in 0..8 {
            temp[i + 8 * k] = out[k];
        }
    }
    temp
}

#[inline]
fn munge_row(x: i32) -> i32 {
    x.wrapping_add(0x7F) >> 8
}

fn idct_rows(temp: &[i32; 64], mut put: impl FnMut(usize, usize, i32)) {
    for i in 0..8 {
        let row: [i32; 8] = std::array::from_fn(|k| temp[8 * i + k]);
        let out = idct8(row);
        for (k, &v) in out.iter().enumerate() {
            put(i, k, munge_row(v));
        }
    }
}

fn idct_put(dst: &mut [u8], off: usize, stride: usize, block: &[i32; 64]) {
    let temp = idct_cols(block);
    idct_rows(&temp, |y, x, v| dst[off + y * stride + x] = v as u8);
}

fn idct_add(dst: &mut [u8], off: usize, stride: usize, block: &mut [i32; 64]) {
    let temp = idct_cols(block);
    idct_rows(&temp, |y, x, v| block[y * 8 + x] = v);
    for y in 0..8 {
        for x in 0..8 {
            let p = &mut dst[off + y * stride + x];
            *p = p.wrapping_add(block[y * 8 + x] as u8);
        }
    }
}
