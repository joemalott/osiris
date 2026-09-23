//! Long-run robustness sweep over every campaign mission: not part of the normal
//! suite (needs a real copy of the game data, and takes a while), so it's `#[ignore]`.
//! Run it explicitly after a change that touches simulation code:
//!
//! ```sh
//! cargo test --release -p osiris-sim --test mission_sweep -- --ignored --nocapture
//! ```
//!
//! It looks for the game data at `$OSIRIS_TEST_DATA`, or else the path this project
//! keeps it at during development.

use osiris_formats::{ImageLibrary, MissionPak, Model};
use osiris_sim::{Balance, Defs, World};
use std::path::PathBuf;
use std::sync::Arc;

fn data_dir() -> PathBuf {
    std::env::var_os("OSIRIS_TEST_DATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/Users/jmalott/Desktop/Projects/Osiris/PharaohData"))
}

/// Every mission slot in `mission1.pak`, ticked a long while and saved/reloaded: it
/// must not panic, and the reload must reproduce the save exactly (byte for byte),
/// which is what the game relies on to save/load a game in progress.
#[test]
#[ignore = "needs the real Pharaoh game data; see module docs"]
fn all_missions_run_long_without_panicking() {
    let data = data_dir();
    let library = ImageLibrary::open(&data.join("Data")).expect("game data (set OSIRIS_TEST_DATA if it isn't at the default path)");
    let defs = Arc::new(Defs::load(&library).unwrap());
    let model = Model::parse(&String::from_utf8_lossy(&std::fs::read(data.join("Pharaoh_Model_Normal.txt")).unwrap())).unwrap();
    let balance = Arc::new(Balance::from_model(&model));
    let pak = MissionPak::open(&data.join("mission1.pak")).unwrap();

    let mut failures = Vec::new();
    for i in 0..pak.slots() {
        let Ok(scenario) = pak.scenario(i) else { continue };
        let mut world = World::new(&scenario, defs.clone(), balance.clone());
        world.start(&scenario);
        world.load_mission(i as i32);
        for _ in 0..60_000 {
            world.tick();
        }
        let bytes = world.save().unwrap();
        let reloaded = World::load(&bytes, defs.clone(), balance.clone()).unwrap();
        if reloaded.save().unwrap() != bytes {
            failures.push(i);
        }
    }
    assert!(failures.is_empty(), "missions not identical after reload: {failures:?}");
}
