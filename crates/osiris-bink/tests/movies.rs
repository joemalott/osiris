//! Decodes every movie Pharaoh ships in full and checks the frame and sample counts and
//! the checksums of three frames. The checksums are of FFmpeg's decoding, which this
//! crate matched exactly on every frame (`examples/compare.rs` redoes that comparison).
//! Needs the game data, so it's `#[ignore]`:
//!
//! ```sh
//! cargo test --release -p osiris-bink --test movies -- --ignored
//! ```
//!
//! It looks for the game data at `$OSIRIS_TEST_DATA`, or else `PharaohData` at the top
//! of the checkout.

use std::path::PathBuf;

fn data_dir() -> PathBuf {
    std::env::var_os("OSIRIS_TEST_DATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../PharaohData")))
}

/// FNV-1a over the visible pixels of a frame's three planes.
fn frame_hash(movie: &osiris_bink::Movie<impl std::io::Read + std::io::Seek>) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for plane in &movie.planes()[..3] {
        for row in plane.rows() {
            for &b in row {
                h = (h ^ b as u64).wrapping_mul(0x100_0000_01b3);
            }
        }
    }
    h
}

/// (file, frames, audio samples, hashes of the frames a quarter and half way through and the last)
const MOVIES: [(&str, u32, usize, [u64; 3]); 7] = [
    ("Intro_big.bik", 3282, 6030720, [0x4fd9fb452f5ad643, 0xcc2f2e49e8b56c4d, 0x22ebea5d4e874452]),
    ("pre_dynastic_big.bik", 709, 1303680, [0xfda31c1535db2256, 0x5ae0e65f62dc5ae4, 0xc735e367ba818c38]),
    ("Archaic_big.bik", 1114, 4097280, [0xbf83cfa0d22779b1, 0x902bc8771672f84a, 0xd6e7ec702dc3e93b]),
    ("old_kingdom_big.bik", 855, 1572480, [0xa71e33bcf8f823ce, 0x5e3ed992c017ea6f, 0x8fd16b9842861312]),
    ("middle_kingdom_big.bik", 1735, 3189120, [0xdbe8c196abcb90b8, 0x68d1f0d846124ca0, 0x89ff6e32678ca63c]),
    ("new_king_big.bik", 798, 1466880, [0xd1d5659fafc01d4b, 0x9bfdcbc386446c79, 0x1a7d16ca7456fe84]),
    ("win_grand_big.bik", 1721, 3164160, [0x9d81d690ec3bad4d, 0xed55acb94a310ccc, 0x1139bbc16fa88f2e]),
];

#[test]
#[ignore = "needs the real Pharaoh game data; see module docs"]
fn every_movie_decodes() {
    let dir = data_dir().join("BINKS/High");
    let mut failures = Vec::new();
    for (name, frames, sample_count, hashes) in MOVIES {
        let mut movie = osiris_bink::Movie::open(dir.join(name)).expect("movie (set OSIRIS_TEST_DATA if the game data isn't at the default path)");
        assert_eq!((movie.width(), movie.height(), movie.fps()), (560, 333, 24.0), "{name}");
        assert_eq!(movie.frames(), frames, "{name}");
        let checked = [frames as usize / 4, frames as usize / 2, frames as usize - 1];
        let mut got = [0u64; 3];
        let mut samples = Vec::new();
        let mut n = 0;
        while movie.next_frame(&mut samples).unwrap_or_else(|e| panic!("{name} frame {n}: {e}")) {
            if let Some(i) = checked.iter().position(|&c| c == n) {
                got[i] = frame_hash(&movie);
            }
            n += 1;
        }
        if n != frames as usize || samples.len() != sample_count || got != hashes {
            failures.push(format!("{name}: {n} frames, {} samples, hashes {got:#x?}", samples.len()));
        }
        assert!(samples.iter().all(|s| s.is_finite() && s.abs() <= 2.0), "{name}: bad samples");
    }
    assert!(failures.is_empty(), "{failures:#?}");
}
