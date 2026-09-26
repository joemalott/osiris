//! Replay regression tests: each recorded game in `tests/replays/` is played back
//! from its starting save and must come out the same, month by month, as when it
//! was recorded. Not part of the normal suite (needs a real copy of the game data),
//! so it's `#[ignore]`. Run it after a change that touches simulation code:
//!
//! ```sh
//! cargo test --release -p osiris-sim --test replays -- --ignored --nocapture
//! ```
//!
//! It looks for the game data at `$OSIRIS_TEST_DATA`, or else the path this project
//! keeps it at during development. A change meant to alter the simulation breaks
//! these; record them again with `tests/replays/record.sh`. The same check from the
//! command line: `osiris-tools replay <game dir> crates/osiris-sim/tests/replays/*.osiris-replay`.

use osiris_formats::ImageLibrary;
use osiris_sim::replay::{Player, Replay};
use osiris_sim::{Balance, Defs};
use std::path::PathBuf;
use std::sync::Arc;

fn data_dir() -> PathBuf {
    std::env::var_os("OSIRIS_TEST_DATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/Users/jmalott/Desktop/Projects/Osiris/PharaohData"))
}

#[test]
#[ignore = "needs the real Pharaoh game data; see module docs"]
fn recorded_games_replay_identically() {
    let data = data_dir();
    let library = ImageLibrary::open(&data.join("Data")).expect("game data (set OSIRIS_TEST_DATA if it isn't at the default path)");
    let defs = Arc::new(Defs::load(&library).unwrap());
    let balances = Balance::load_all(&data).unwrap();
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/replays");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().path()).filter(|p| p.extension().is_some_and(|e| e == "osiris-replay")).collect();
    files.sort();
    assert!(!files.is_empty(), "no replays in {}", dir.display());
    let mut failures = Vec::new();
    for path in &files {
        let replay = Replay::read(path).unwrap();
        let (mut world, mut player) = Player::start(replay, defs.clone(), balances.clone()).unwrap();
        let name = path.file_name().unwrap().to_string_lossy();
        match player.run(&mut world) {
            Ok((commands, months)) => println!("{name}: identical ({commands} commands, {months} months)"),
            Err(d) => failures.push(format!("{name}: {d}")),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
