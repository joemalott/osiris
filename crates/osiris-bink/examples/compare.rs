//! Compares this crate's decoding of Bink files with FFmpeg's (which must be on the
//! PATH, or named by $FFMPEG): every frame's Y, U and V planes, and the audio samples.
//!
//! cargo run --release -p osiris-bink --example compare -- PharaohData/BINKS/High/*.bik

use std::io::Read;
use std::process::{Command, Stdio};
use std::time::Instant;

fn main() {
    let ffmpeg = std::env::var("FFMPEG").unwrap_or_else(|_| "ffmpeg".into());
    let mut failed = false;
    for path in std::env::args().skip(1) {
        let mut movie = osiris_bink::Movie::open(&path).expect("open");
        let (w, h) = (movie.width() as usize, movie.height() as usize);
        let (cw, ch) = (w.div_ceil(2), h.div_ceil(2));
        let mut child = Command::new(&ffmpeg)
            .args(["-v", "error", "-i", &path, "-f", "rawvideo", "-pix_fmt", "yuv420p", "-"])
            .stdout(Stdio::piped())
            .spawn()
            .expect("ffmpeg");
        let mut ff = child.stdout.take().unwrap();
        let mut buf = vec![0u8; w * h + 2 * cw * ch];
        let mut samples = Vec::new();
        let mut frames = 0;
        let mut exact = 0;
        let mut max = [0u8; 3];
        let mut sum = [0u64; 3];
        let mut decode_time = 0.0;
        loop {
            let t = Instant::now();
            let more = movie.next_frame(&mut samples).expect("decode");
            decode_time += t.elapsed().as_secs_f64();
            if !more {
                break;
            }
            if ff.read_exact(&mut buf).is_err() {
                println!("{path}: ffmpeg gave fewer frames ({frames})");
                failed = true;
                break;
            }
            let planes = movie.planes();
            let mut same = true;
            let mut off = 0;
            for (p, (pw, ph)) in [(w, h), (cw, ch), (cw, ch)].into_iter().enumerate() {
                for y in 0..ph {
                    let ours = &planes[p].data[y * planes[p].stride..][..pw];
                    let theirs = &buf[off + y * pw..][..pw];
                    for (a, b) in ours.iter().zip(theirs) {
                        let d = a.abs_diff(*b);
                        max[p] = max[p].max(d);
                        sum[p] += d as u64;
                        same &= d == 0;
                    }
                }
                off += pw * ph;
            }
            exact += same as usize;
            frames += 1;
        }
        let _ = child.wait();
        let px = [w * h, cw * ch, cw * ch];
        let mean: Vec<String> = (0..3).map(|p| format!("{:.4}", sum[p] as f64 / (px[p] * frames.max(1)) as f64)).collect();
        println!(
            "{path}: {frames}/{} frames, {exact} identical; max diff Y {} U {} V {}; mean Y {} U {} V {}; decode {:.2} s ({:.0} fps)",
            movie.frames(),
            max[0],
            max[1],
            max[2],
            mean[0],
            mean[1],
            mean[2],
            decode_time,
            frames as f64 / decode_time
        );
        failed |= frames != movie.frames() as usize || max.iter().any(|&m| m > 0);

        let out = Command::new(&ffmpeg)
            .args(["-v", "error", "-i", &path, "-vn", "-f", "f32le", "-"])
            .output()
            .expect("ffmpeg audio");
        let theirs: Vec<f32> = out.stdout.chunks(4).map(|b| f32::from_le_bytes(b.try_into().unwrap())).collect();
        let n = theirs.len().min(samples.len());
        let (mut sig, mut noise, mut max_err) = (0f64, 0f64, 0f32);
        for (a, b) in samples[..n].iter().zip(&theirs[..n]) {
            sig += (*b as f64).powi(2);
            noise += ((a - b) as f64).powi(2);
            max_err = max_err.max((a - b).abs());
        }
        let snr = 10.0 * (sig / noise.max(1e-30)).log10();
        println!(
            "  audio: ours {} samples, ffmpeg {}; SNR {snr:.1} dB, max abs error {max_err:.2e}",
            samples.len(),
            theirs.len()
        );
        failed |= samples.len() != theirs.len() || snr < 80.0;
    }
    if failed {
        std::process::exit(1);
    }
}
