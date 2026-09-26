//! Bink audio decoding, the RDFT variant (the one Pharaoh's movies use; the DCT
//! variant is not supported).
//!
//! Ported from FFmpeg's libavcodec/binkaudio.c (Copyright (c) 2007-2011 Peter Ross,
//! Copyright (c) 2009 Daniel Verkamp) and the band edges of libavcodec/wma_freqs.c,
//! licensed LGPL-2.1-or-later and used here under GPL-3.0-or-later, as the LGPL allows.
//! The inverse real FFT is our own.
//!
//! Each block codes a spectrum of `frame_len` values, quantised per critical band, whose
//! inverse real FFT gives `frame_len` samples; a block overlaps the next by a sixteenth
//! and the overlap is cross-faded. Stereo is coded as one channel at twice the sample
//! rate, so the output is already interleaved.

use crate::bits::Bits;
use crate::{Error, Result};

const FLAG_STEREO: u16 = 0x2000;
const FLAG_DCT: u16 = 0x1000;

const CRITICAL_FREQS: [u32; 25] = [
    100, 200, 300, 400, 510, 630, 770, 920, 1080, 1270, 1480, 1720, 2000, 2320, 2700, 3150, 3700,
    4400, 5300, 6400, 7700, 9500, 12000, 15500, 24500,
];

const RLE_LENGTHS: [usize; 16] = [2, 3, 4, 5, 6, 8, 9, 10, 11, 12, 13, 14, 15, 16, 32, 64];

pub struct AudioDecoder {
    pub sample_rate: u32,
    pub channels: u16,
    frame_len: usize,
    overlap_len: usize,
    block_size: usize,
    num_bands: usize,
    bands: [usize; 26],
    root: f32,
    quant_table: [f32; 96],
    previous: Vec<f32>,
    first: bool,
    fft: Fft,
    coeffs: Vec<f32>,
    out: Vec<f32>,
}

impl AudioDecoder {
    pub fn new(sample_rate: u32, flags: u16) -> Result<AudioDecoder> {
        if flags & FLAG_DCT != 0 {
            return Err(Error::Unsupported("Bink DCT audio"));
        }
        let channels: u16 = if flags & FLAG_STEREO != 0 { 2 } else { 1 };
        let mut frame_len_bits = if sample_rate < 22050 {
            9
        } else if sample_rate < 44100 {
            10
        } else {
            11
        };
        // Interleaved channels are coded as one at a multiple of the rate.
        let rate = sample_rate * channels as u32;
        frame_len_bits += (channels as u32).ilog2();
        let frame_len = 1usize << frame_len_bits;
        let overlap_len = frame_len / 16;
        let rate_half = (rate as usize).div_ceil(2);
        let root = (2.0 / ((frame_len as f64).sqrt() * 32768.0)) as f32;
        let quant_table = std::array::from_fn(|i| (i as f32 * 0.152_891_65_f32).exp() * root);
        let mut num_bands = 1;
        while num_bands < 25 {
            if rate_half <= CRITICAL_FREQS[num_bands - 1] as usize {
                break;
            }
            num_bands += 1;
        }
        let mut bands = [0usize; 26];
        bands[0] = 2;
        for i in 1..num_bands {
            bands[i] = (CRITICAL_FREQS[i - 1] as usize * frame_len / rate_half) & !1;
        }
        bands[num_bands] = frame_len;
        Ok(AudioDecoder {
            sample_rate,
            channels,
            frame_len,
            overlap_len,
            block_size: frame_len - overlap_len,
            num_bands,
            bands,
            root,
            quant_table,
            previous: vec![0.0; overlap_len],
            first: true,
            fft: Fft::new(frame_len),
            coeffs: vec![0.0; frame_len + 2],
            out: vec![0.0; frame_len],
        })
    }

    /// Decodes one audio packet (`len` bytes, followed by padding) and appends its
    /// interleaved samples, full scale at 1.0, to `out`.
    pub fn decode(&mut self, packet: &[u8], len: usize, out: &mut Vec<f32>) -> Result<()> {
        if len < 4 {
            return Err(Error::Invalid("audio packet too small"));
        }
        let mut gb = Bits::new(packet, len);
        // The decoded size in bytes.
        gb.skip(32);
        while gb.left() > 0 {
            self.decode_block(&mut gb)?;
            out.extend_from_slice(&self.out[..self.block_size]);
            gb.align32();
        }
        Ok(())
    }

    fn decode_block(&mut self, gb: &mut Bits) -> Result<()> {
        let n = self.frame_len;
        let coeffs = &mut self.coeffs;
        if gb.left() < 58 {
            return Err(Error::Invalid("audio block overread"));
        }
        coeffs[0] = get_float(gb) * self.root;
        coeffs[1] = get_float(gb) * self.root;
        if gb.left() < self.num_bands as isize * 8 {
            return Err(Error::Invalid("audio block overread"));
        }
        let mut quant = [0f32; 25];
        for q in quant.iter_mut().take(self.num_bands) {
            *q = self.quant_table[(gb.get(8) as usize).min(95)];
        }
        let mut k = 0;
        let mut q = quant[0];
        let mut i = 2;
        while i < n {
            let mut j = if gb.bit() { i + RLE_LENGTHS[gb.get(4) as usize] * 8 } else { i + 8 };
            j = j.min(n);
            let width = gb.get(4);
            if width == 0 {
                coeffs[i..j].fill(0.0);
                i = j;
                while self.bands[k] < i {
                    q = quant[k];
                    k += 1;
                }
            } else {
                while i < j {
                    if self.bands[k] == i {
                        q = quant[k];
                        k += 1;
                    }
                    let coeff = gb.get(width);
                    coeffs[i] = if coeff != 0 {
                        if gb.bit() { -q * coeff as f32 } else { q * coeff as f32 }
                    } else {
                        0.0
                    };
                    i += 1;
                }
            }
        }
        // coeffs holds the spectrum as (re, im) pairs, but the Nyquist value's real part
        // where the DC's imaginary part would be.
        coeffs[n] = coeffs[1];
        coeffs[n + 1] = 0.0;
        coeffs[1] = 0.0;
        self.fft.inverse_real(coeffs, &mut self.out);

        let count = self.overlap_len as f32;
        if !self.first {
            for i in 0..self.overlap_len {
                self.out[i] = (self.previous[i] * (count - i as f32) + self.out[i] * i as f32) / count;
            }
        }
        self.previous.copy_from_slice(&self.out[n - self.overlap_len..]);
        self.first = false;
        Ok(())
    }
}

fn get_float(gb: &mut Bits) -> f32 {
    let power = gb.get(5) as i32;
    let f = gb.get(23) as f32 * 2f32.powi(power - 23);
    if gb.bit() { -f } else { f }
}

/// A radix-2 complex FFT of half the transform size, used for the inverse real FFT.
struct Fft {
    n: usize,
    /// e^(2 pi i k / (n/2)) for k < n/4, the complex FFT's twiddles.
    twiddles: Vec<(f32, f32)>,
    /// e^(2 pi i k / n) for k < n/2, for splitting the real transform.
    split: Vec<(f32, f32)>,
    bitrev: Vec<u32>,
    buf: Vec<(f32, f32)>,
}

impl Fft {
    fn new(n: usize) -> Fft {
        let m = n / 2;
        let tau = std::f64::consts::TAU;
        let twiddles = (0..m / 2)
            .map(|k| {
                let a = tau * k as f64 / m as f64;
                (a.cos() as f32, a.sin() as f32)
            })
            .collect();
        let split = (0..m)
            .map(|k| {
                let a = tau * k as f64 / n as f64;
                (a.cos() as f32, a.sin() as f32)
            })
            .collect();
        let bits = m.trailing_zeros();
        let bitrev = (0..m as u32).map(|i| if bits == 0 { 0 } else { i.reverse_bits() >> (32 - bits) }).collect();
        Fft { n, twiddles, split, bitrev, buf: vec![(0.0, 0.0); m] }
    }

    /// x[t] = 1/2 the sum over the Hermitian spectrum of X[k] e^(-2 pi i k t / n), for the
    /// n/2 + 1 values X in `spec` as (re, im) pairs.
    fn inverse_real(&mut self, spec: &[f32], out: &mut [f32]) {
        let n = self.n;
        let m = n / 2;
        // Fold the spectrum into the half-size complex sequence z whose transform
        // interleaves the even and odd output samples: Z[k] = E[k] + i O[k], with
        // E[k] = X[k] + conj(X[m-k]) and O[k] = (X[k] - conj(X[m-k])) w^k.
        for k in 0..m {
            let (xr, xi) = (spec[2 * k], spec[2 * k + 1]);
            let (yr, yi) = (spec[2 * (m - k)], -spec[2 * (m - k) + 1]);
            let (er, ei) = (xr + yr, xi + yi);
            let (dr, di) = (xr - yr, xi - yi);
            // w = e^(-2 pi i k / n)
            let (c, s) = self.split[k];
            let (or, oi) = (dr * c + di * s, di * c - dr * s);
            // z = E + i O
            self.buf[self.bitrev[k] as usize] = (er - oi, ei + or);
        }
        // Forward transform (e^-i) of size m, in place on the bit-reversed buffer.
        let mut len = 2;
        while len <= m {
            let half = len / 2;
            let step = m / len;
            for start in (0..m).step_by(len) {
                for j in 0..half {
                    let (c, s) = self.twiddles[j * step];
                    let (ar, ai) = self.buf[start + j];
                    let (br, bi) = self.buf[start + j + half];
                    // b * e^(-i a)
                    let (tr, ti) = (br * c + bi * s, bi * c - br * s);
                    self.buf[start + j] = (ar + tr, ai + ti);
                    self.buf[start + j + half] = (ar - tr, ai - ti);
                }
            }
            len *= 2;
        }
        // Halved, as the reference scales its transform.
        for t in 0..m {
            out[2 * t] = self.buf[t].0 * 0.5;
            out[2 * t + 1] = self.buf[t].1 * 0.5;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The fast inverse real FFT against the plain sum it stands for.
    #[test]
    fn inverse_real_matches_the_sum() {
        for n in [16usize, 2048] {
            let spec: Vec<f32> = (0..n + 2).map(|i| if i == 1 || i == n + 1 { 0.0 } else { ((i * 7919) % 97) as f32 / 97.0 - 0.5 }).collect();
            let mut out = vec![0.0; n];
            Fft::new(n).inverse_real(&spec, &mut out);
            for t in [0, 1, n / 3, n - 1] {
                let mut sum = 0f64;
                for k in 0..n {
                    // The Hermitian extension of the n/2 + 1 values given.
                    let (re, im) = if k <= n / 2 { (spec[2 * k], spec[2 * k + 1]) } else { (spec[2 * (n - k)], -spec[2 * (n - k) + 1]) };
                    let a = -std::f64::consts::TAU * (k * t) as f64 / n as f64;
                    sum += re as f64 * a.cos() - im as f64 * a.sin();
                }
                assert!((out[t] as f64 - sum / 2.0).abs() < 1e-3, "n {n} t {t}: {} vs {}", out[t], sum / 2.0);
            }
        }
    }
}
