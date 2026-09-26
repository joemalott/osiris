//! The campaign missions that start with a city: the buildings their saved-game
//! building tables hold become Osiris buildings of the same kind on the same tiles.
//! Thinis (mission 23, Civil War) is the one the briefing speaks of: its temple
//! complex to Osiris and its mansion survived the conquest.
//!
//! They need the game data (`$OSIRIS_TEST_DATA`, else `PharaohData` at the top of the
//! workspace), so they are ignored by default:
//!
//! ```sh
//! cargo test --release -p osiris-sim --test saved_buildings -- --ignored
//! ```

use osiris_sim::map::terrain;
use osiris_sim::{Balance, Defs, World};
use std::path::PathBuf;
use std::sync::Arc;

fn data_dir() -> PathBuf {
    std::env::var_os("OSIRIS_TEST_DATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../PharaohData"))
}

struct Game {
    defs: Arc<Defs>,
    balance: Arc<Balance>,
    pak: osiris_formats::MissionPak,
}

fn game() -> Game {
    let data = data_dir();
    let library = osiris_formats::ImageLibrary::open(&data.join("Data")).expect("game data (set OSIRIS_TEST_DATA if it isn't at the default path)");
    Game {
        defs: Arc::new(Defs::load(&library).expect("load defs")),
        balance: Arc::new(Balance::load(&data, "Normal").expect("model")),
        pak: osiris_formats::MissionPak::open(&data.join("mission1.pak")).expect("open mission1.pak"),
    }
}

fn start(g: &Game, n: usize) -> (osiris_formats::Scenario, World) {
    let scenario = g.pak.scenario(n).expect("scenario");
    let mut world = World::new(&scenario, g.defs.clone(), g.balance.clone());
    world.start(&scenario);
    world.load_mission(n as i32);
    (scenario, world)
}

#[test]
#[ignore = "needs the real Pharaoh game data; see module docs"]
fn thinis_starts_with_its_temple_complex_and_mansion() {
    let g = game();
    let (_, world) = start(&g, 23);
    let complex = world.buildings.iter().find(|b| b.kind == 65).expect("the temple complex to Osiris");
    // Its three parts run down from (135, 138), turned along y (the original's
    // orientation 6), so its court's corner is two tiles west of the first.
    assert_eq!((complex.x, complex.y, complex.footprint(), complex.orientation), (133, 138, (7, 13), 1));
    assert_eq!(complex.upgrades, 3, "the altar and the oracle stand");
    assert_eq!(world.map.building.at_or(135, 144, 0), complex.id);
    let mansion = world.buildings.iter().find(|b| b.kind == 78).expect("the family mansion");
    assert_eq!((mansion.x, mansion.y, mansion.size), (149, 159, 4));
    // The rest of the city: 21 houses holding 402 people, and 36 pieces of wall.
    assert_eq!(world.buildings.iter().filter(|b| b.is_house()).count(), 21);
    assert_eq!(world.population, 402);
    assert_eq!(world.buildings.count_of(osiris_sim::defenses::WALL), 36);
    let granary = world.buildings.iter().find(|b| b.kind == 71).expect("the granary");
    assert_eq!(granary.stock[8], 500, "game meat");
}

/// Every standing record of every mission becomes a building where the record says,
/// and no tile is left marked as built on without one.
#[test]
#[ignore = "needs the real Pharaoh game data; see module docs"]
fn every_mission_sets_up_the_buildings_it_holds() {
    let g = game();
    let mut with_buildings = Vec::new();
    for n in 0..g.pak.slots() {
        if g.pak.entry(n).is_none() {
            continue;
        }
        let (scenario, world) = start(&g, n);
        if scenario.buildings.is_empty() {
            continue;
        }
        with_buildings.push(n);
        for r in &scenario.buildings {
            // Storage rooms, parade grounds, a complex's altar and oracle, and the
            // parts after a building's first are set up with the first.
            let part = [73, 54, 211, 212].contains(&r.kind()) || r.prev_part() != 0;
            let Some(p) = world.placement(r, &scenario.buildings) else {
                assert!(part, "mission {n}: {r:?} not set up");
                continue;
            };
            let id = world.map.building.at_or(p.x, p.y, 0);
            let b = world.buildings.get(id).unwrap_or_else(|| panic!("mission {n}: no building at {},{} for {r:?}", p.x, p.y));
            assert_eq!((b.kind, b.x, b.y), (p.kind, p.x, p.y), "mission {n}: {r:?}");
            // Buildings of several records span more than their first's size (a
            // storage yard's office is one tile of its nine).
            let parts = osiris_sim::monuments::monument_def(b.kind).is_some() || osiris_sim::temple_complex::is_complex(b.kind);
            if !parts && b.kind != osiris_sim::defenses::GATEHOUSE && b.kind != osiris_sim::buildings::kind::STORAGE_YARD {
                assert_eq!(b.size, r.size(), "mission {n}: {r:?}");
            }
        }
        for y in 0..world.map.height {
            for x in 0..world.map.width {
                let t = world.map.terrain.at_or(x, y, 0);
                if t & (terrain::BUILDING | terrain::WALL | terrain::GATEHOUSE) != 0 {
                    assert_ne!(world.map.building.at_or(x, y, 0), 0, "mission {n}: {x},{y} is built on by nothing");
                }
            }
        }
    }
    assert_eq!(with_buildings, [11, 23, 25, 26, 32, 34, 40, 42, 44, 46, 49, 50, 51]);
}
