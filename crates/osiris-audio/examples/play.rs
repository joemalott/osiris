//! Manual test: plays a music file for 3 seconds, then exits.
//!
//! Usage:
//!   cargo run -p osiris-audio --example play -- <path-to-PharaohData> [track-name]
//!
//! `track-name` is a bare filename under `AUDIO/Music/` (extension optional, defaults to
//! `.mp3`), e.g. `Ra` or `Khu.mp3`. Defaults to `Ra` if not given.

use std::env;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

use osiris_audio::{Audio, Track};

fn main() {
    let mut args = env::args().skip(1);
    let game_dir = args.next().map(PathBuf::from).unwrap_or_else(|| {
        eprintln!("usage: play <path-to-PharaohData> [track-name]");
        std::process::exit(1);
    });
    let track_name = args.next().unwrap_or_else(|| "Ra".to_string());

    let audio = Audio::new(&game_dir).expect("failed to initialize osiris-audio");

    println!("Playing {track_name} for 3 seconds...");
    audio.play_music(&Track::new(track_name));
    thread::sleep(Duration::from_secs(3));
    audio.stop_music();
}
