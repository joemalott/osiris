//! After the land changes in play, the terrain images redrawn around the change must
//! look like a whole-map redraw of the same terrain would: no hole, no stale edge, no
//! tile that doesn't join its neighbour. The original redraws the whole terrain only
//! when a map starts or a game loads and patches the rest locally; these tests hold
//! Osiris's local patches to its whole-map pass (`terrain_images::rebuild`), allowing
//! the few ways the original's patches may differ from it too (see `agrees`).
//!
//! They need the game data (`$OSIRIS_TEST_DATA`, else `PharaohData` at the top of the
//! workspace) and pass without it. An unoptimised build (plain `cargo test`) checks
//! less often and makes fewer edits; for the full run after touching terrain images:
//!
//! ```sh
//! cargo test --release -p osiris-sim --test terrain_refresh
//! ```

use osiris_sim::buildings::kind;
use osiris_sim::map::{Map, NEIGHBOURS, terrain};
use osiris_sim::terrain_images::{Choice, rebuild};
use osiris_sim::{Balance, Command, Defs, Outcome, World};
use std::path::PathBuf;
use std::sync::Arc;

/// Checks against a whole-map redraw cost a whole-map redraw each, which an unoptimised
/// build takes its time over: it looks this many times less often.
const SPARSE: usize = if cfg!(debug_assertions) { 8 } else { 1 };

fn data_dir() -> Option<PathBuf> {
    let dir = std::env::var_os("OSIRIS_TEST_DATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../PharaohData"));
    dir.join("mission1.pak").is_file().then_some(dir)
}

fn mission(data: &std::path::Path, n: usize) -> World {
    let library = osiris_formats::ImageLibrary::open(&data.join("Data")).expect("open image library");
    let pak = osiris_formats::MissionPak::open(&data.join("mission1.pak")).expect("open mission1.pak");
    let scenario = pak.scenario(n).expect("scenario");
    let defs = Arc::new(Defs::load(&library).expect("load defs"));
    let model = osiris_formats::Model::parse(&String::from_utf8_lossy(&std::fs::read(data.join("Pharaoh_Model_Normal.txt")).expect("model"))).expect("parse model");
    let mut world = World::new(&scenario, defs, Arc::new(Balance::from_model(&model)));
    world.start(&scenario);
    world.scenario_allowed = None;
    world.treasury = 1_000_000;
    world
}

/// A bare land image: any of the land blocks, as a patch lays the blocks that fit its
/// rectangle and the whole-map pass those that fit the map.
fn bare_land(defs: &Defs, image: u32) -> bool {
    let t = &defs.terrain;
    (t.empty_land..t.empty_land + 58).contains(&image) || (t.empty_land_alt..t.empty_land_alt + 58).contains(&image)
}

/// A tree image. A tree's look follows how much wood stands around it, but the
/// original never redraws the trees round land it clears (FUN_00474700), so a wood
/// thinned in play keeps its old looks.
fn tree(defs: &Defs, image: u32) -> bool {
    let t = &defs.terrain;
    (t.tree..t.tree + 64).contains(&image) || (t.young_tree..t.young_tree + 24).contains(&image)
}

/// A dry floodplain image. The whole-map pass marks the floodplain's rows afresh from
/// the river, and a stretch of floodplain cut off from it (by an earthquake's cracks,
/// say) gets no soil; in play the original keeps the rows it marked when the city
/// started (FUN_004be160), soil and all.
fn dry_floodplain(defs: &Defs, image: u32) -> bool {
    (defs.terrain.floodplain..defs.terrain.floodplain + 48).contains(&image)
}

/// Whether the image a tile shows after a patch agrees with the whole-map pass: the
/// same image, another of the set it was picked from (rotating counters, animated
/// water), or another look of the same land where the whole-map pass would pick one
/// by something play doesn't redraw (bare land blocks, trees, cut-off floodplain).
fn agrees(defs: &Defs, got: u32, want: u32, choice: Choice) -> bool {
    let both = |f: fn(&Defs, u32) -> bool| f(defs, got) && f(defs, want);
    got == want || choice.contains(got) || both(bare_land) || both(tree) || both(dry_floodplain)
}

/// A tile whose image disagrees with a whole-map redraw.
#[derive(Debug, Clone, Copy)]
struct Disagreement {
    x: i32,
    y: i32,
    shown: u32,
    redrawn: u32,
}

/// The tiles whose images disagree with a whole-map redraw. Buildings, walls and
/// ditches are left out: the pass doesn't draw them.
fn disagreements(world: &World) -> Vec<Disagreement> {
    let mut redrawn: Map = world.map.clone();
    let choices = rebuild(&mut redrawn, &world.defs);
    let skip = terrain::BUILDING | terrain::WALL | terrain::GATEHOUSE | terrain::CANAL | terrain::WALKABLE_BUILDING;
    let mut out = Vec::new();
    for y in 0..world.map.height {
        for x in 0..world.map.width {
            if world.map.terrain.at_or(x, y, 0) & skip != 0 || world.map.building.at_or(x, y, 0) != 0 {
                continue;
            }
            let (shown, want) = (world.map.images.at_or(x, y, 0), redrawn.images.at_or(x, y, 0));
            if !agrees(&world.defs, shown, want, choices.at_or(x, y, Choice::default())) {
                out.push(Disagreement { x, y, shown, redrawn: want });
            }
        }
    }
    out
}

/// A readable account of the first few disagreements: each image as an offset from
/// the terrain group it lies in, and the terrain of the tile and its neighbours.
fn describe(world: &World, d: &[Disagreement]) -> String {
    let t = &world.defs.terrain;
    let groups = [
        ("water", t.water),
        ("flood_water", t.flood_water),
        ("deepwater", t.deepwater),
        ("floodplain", t.floodplain),
        ("floodplain_road", t.floodplain_road),
        ("road", t.road),
        ("dirt_road", t.dirt_road),
        ("grass", t.grass),
        ("grass_edges", t.grass_edges),
        ("empty_land", t.empty_land),
        ("empty_land_alt", t.empty_land_alt),
        ("meadow_with_grass", t.meadow_with_grass),
        ("tree", t.tree),
        ("rock", t.rock),
        ("rubble", t.rubble),
        ("reeds", t.reeds),
        ("canal", t.canal),
    ];
    let name = |id: u32| groups.iter().filter(|(_, b)| *b <= id).max_by_key(|(_, b)| *b).map_or(format!("{id}"), |(n, b)| format!("{n}+{}", id - b));
    let mut s = format!("{} tiles disagree:", d.len());
    for d in &d[..d.len().min(8)] {
        let around: Vec<String> = NEIGHBOURS.iter().map(|&(dx, dy)| format!("{:x}", world.map.terrain.at_or(d.x + dx, d.y + dy, 0))).collect();
        s += &format!("\n  {},{}: shown {} redrawn {}, terrain {:#x}, around {}", d.x, d.y, name(d.shown), name(d.redrawn), world.map.terrain.at_or(d.x, d.y, 0), around.join(" "));
    }
    s
}

/// Rows of the floodplain seven apart, with the column where each first meets it from
/// the land side.
fn floodplain_edges(world: &World, rows: usize) -> Vec<(i32, i32)> {
    let m = &world.map;
    (0..m.height)
        .filter_map(|y| (1..m.width).find(|&x| m.terrain_is(x, y, terrain::FLOODPLAIN) && !m.terrain_is(x - 1, y, terrain::FLOODPLAIN | terrain::WATER)).map(|x| (x, y)))
        .step_by(7)
        .take(rows)
        .collect()
}

/// The Nile floods a campaign mission's floodplain and draws back, over roads and
/// ditches laid across its edge, and everything it crosses looks as a redraw of the
/// map would at every step.
#[test]
fn the_flood_redraws_what_it_crosses() {
    let Some(data) = data_dir() else { return };
    for n in [2, 3] {
        let mut world = mission(&data, n);
        assert!(world.has_floodplain(), "mission {n} has a floodplain");
        for (i, (x, y)) in floodplain_edges(&world, 6).into_iter().enumerate() {
            let cmd = if i % 2 == 0 { Command::Road { start: (x - 4, y), end: (x + 5, y) } } else { Command::Build { kind: kind::IRRIGATION_DITCH, x: x - 4, y, x1: x + 5, y1: y } };
            world.apply(&cmd);
        }
        let before = disagreements(&world);
        assert!(before.is_empty(), "mission {n} before the flood: {}", describe(&world, &before));
        let mut flooded = false;
        for step in 0..(9792 * 3 / 2) {
            world.tick();
            if step % (250 * SPARSE) == 0 {
                let flood = terrain::FLOODPLAIN | terrain::WATER;
                flooded |= (0..world.map.height).any(|y| (0..world.map.width).any(|x| world.map.terrain.at_or(x, y, 0) & flood == flood));
                let d = disagreements(&world);
                assert!(d.is_empty(), "mission {n} tick {step} ({:?}): {}", world.flood_state(), describe(&world, &d));
            }
        }
        assert!(flooded, "the flood never came to mission {n}");
    }
}

/// A small deterministic generator for the edits.
struct Lcg(u64);

impl Lcg {
    fn below(&mut self, n: i32) -> i32 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((self.0 >> 33) % n.max(1) as u64) as i32
    }
}

/// Random roads, ditches, buildings and clearing across campaign missions of every
/// kind of land (river and floodplain, marsh, meadow, trees, rock, dunes, cliffs): after
/// each edit the land around it agrees with a whole-map redraw.
#[test]
fn edits_in_play_redraw_the_land_like_a_new_map() {
    let Some(data) = data_dir() else { return };
    let edits: usize = std::env::var("OSIRIS_TERRAIN_EDITS").ok().and_then(|v| v.parse().ok()).unwrap_or(40 / SPARSE);
    let mut failures = Vec::new();
    for n in [2, 5, 12, 20, 38] {
        let mut world = mission(&data, n);
        let before = disagreements(&world);
        assert!(before.is_empty(), "mission {n} as it starts: {}", describe(&world, &before));
        let mut rng = Lcg(n as u64 * 7919 + 1);
        let (w, h) = (world.map.width, world.map.height);
        let mut done = 0;
        let mut tries = 0;
        while done < edits && tries < edits * 20 {
            tries += 1;
            let (x, y) = (w / 6 + rng.below(w * 2 / 3), h / 6 + rng.below(h * 2 / 3));
            let (x1, y1) = (x + rng.below(9) - 4, y + rng.below(9) - 4);
            let cmd = match rng.below(7) {
                0 | 1 => Command::Road { start: (x, y), end: (x1, y1) },
                2 => Command::Build { kind: kind::IRRIGATION_DITCH, x, y, x1, y1 },
                3 => Command::Build { kind: kind::WELL, x, y, x1: x, y1: y },
                4 => Command::Build { kind: kind::FARM_FIRST, x, y, x1: x, y1: y },
                5 => Command::Build { kind: kind::VACANT_LOT, x, y, x1: x + 1, y1: y + 1 },
                _ => Command::Clear { x0: x.min(x1), y0: y.min(y1), x1: x.max(x1), y1: y.max(y1) },
            };
            if !matches!(world.apply(&cmd), Outcome::Done { .. }) {
                continue;
            }
            done += 1;
            let d = disagreements(&world);
            if !d.is_empty() {
                failures.push(format!("mission {n} after {cmd:?}: {}", describe(&world, &d)));
                // Carry on from a clean map, so each failure is the edit's own.
                let defs = world.defs.clone();
                rebuild(&mut world.map, &defs);
            }
        }
        assert!(done > edits / 2, "mission {n}: only {done} of {tries} edits went through");
    }
    assert!(failures.is_empty(), "{} edits left the land unlike a redraw:\n{}", failures.len(), failures.join("\n"));
}

/// An earthquake cracks the land open across a city with roads and houses, and the
/// cracks, and the land, roads and water beside them, look as a redraw would.
#[test]
fn earthquake_cracks_join_the_land_around() {
    let Some(data) = data_dir() else { return };
    for n in [12, 20] {
        let mut world = mission(&data, n);
        // The epicentre on open land near the middle, as a scenario would put it.
        let (w, h) = (world.map.width, world.map.height);
        let open = |x: i32, y: i32| (-2..=2).all(|dy| (-2..=2).all(|dx| world.map.terrain.at_or(x + dx, y + dy, 1) & !(terrain::GROUNDWATER | terrain::MEADOW) == 0));
        let (cx, cy) = (0..w / 3).flat_map(|r| (-r..=r).flat_map(move |dy| (-r..=r).map(move |dx| (w / 2 + dx, h / 2 + dy)))).find(|&(x, y)| open(x, y)).expect("open land");
        for i in -3..=3 {
            world.apply(&Command::Road { start: (cx - 12, cy + i * 4), end: (cx + 12, cy + i * 4) });
            world.apply(&Command::Build { kind: kind::VACANT_LOT, x: cx - 10 + i * 3, y: cy + i * 4 + 1, x1: cx - 9 + i * 3, y1: cy + i * 4 + 2 });
        }
        assert!(world.quake_now(Some((cx, cy)), Some(100)), "mission {n}: the quake started");
        for step in 0..3000 {
            world.tick();
            if step % (100 * SPARSE) == 0 {
                let d = disagreements(&world);
                assert!(d.is_empty(), "mission {n} tick {step} of the quake: {}", describe(&world, &d));
            }
        }
        let cracked = (0..world.map.height).flat_map(|y| (0..world.map.width).map(move |x| (x, y))).filter(|&(x, y)| world.map.terrain_is(x, y, terrain::ROCK) && world.map.bitfields.at_or(x, y, 0) & 0x80 != 0).count();
        assert!(cracked > 10, "mission {n}: the quake cracked {cracked} tiles");
    }
}
