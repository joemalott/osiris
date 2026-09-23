//! `check-terrain-images`: redraws the terrain of every map and campaign mission and
//! compares it with the images the files store.

use anyhow::Result;
use osiris_formats::{ImageLibrary, MissionPak, Scenario};
use osiris_sim::grid::Grid;
use osiris_sim::map::{Map, terrain as t};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Every map in `Maps` and every campaign mission, by name.
pub fn sources(game: &Path) -> Result<Vec<(String, Scenario)>> {
    let mut sources: Vec<(String, Scenario)> = Vec::new();
    let mut maps: Vec<PathBuf> = std::fs::read_dir(game.join("Maps"))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("map")))
        .collect();
    maps.sort();
    for p in maps {
        sources.push((p.file_name().unwrap().to_string_lossy().into_owned(), Scenario::load_map(&p)?));
    }
    let pak = MissionPak::open(&game.join("mission1.pak"))?;
    for i in 0..pak.slots() {
        if pak.entry(i).is_some() {
            sources.push((format!("mission1.pak #{i}"), pak.scenario(i)?));
        }
    }
    Ok(sources)
}

/// The kind of terrain a tile is counted under, by its most telling terrain bit.
fn kind_of(terrain: u32) -> &'static str {
    const KINDS: &[(u32, &str)] = &[
        (t::BUILDING, "building"),
        (t::WALL | t::GATEHOUSE, "wall"),
        (t::CLIFF, "cliff"),
        (t::ACCESS_RAMP, "ramp"),
        (t::ELEVATION, "elevation"),
        (t::DEEPWATER, "deepwater"),
        (t::WATER, "water"),
        (t::CANAL, "canal"),
        (t::GARDEN, "garden"),
        (t::ROAD, "road"),
        (t::MARSHLAND, "marsh"),
        (t::FLOODPLAIN, "floodplain"),
        (t::ORE, "ore"),
        (t::ROCK, "rock"),
        (t::TREE, "tree"),
        (t::SHRUB, "shrub"),
        (t::DUNE, "dune"),
        (t::RUBBLE, "rubble"),
        (t::MEADOW, "meadow"),
    ];
    KINDS.iter().find(|(m, _)| terrain & m != 0).map_or("land", |(_, n)| n)
}

/// An image id as pack, group and offset: `T8+3` is `Pharaoh_Terrain` group 8, image 3.
fn describe_id(lib: &ImageLibrary, id: u32) -> String {
    let Some(img) = lib.resolve(id) else { return format!("?{id}") };
    let p = lib.pack(img.pack);
    let Some(sg) = p.sg3() else { return format!("{}:{}", p.name, img.index) };
    let i = img.index as usize;
    let group = (1..sg.group_starts.len()).filter_map(|g| Some((g, sg.group_start(g)?))).filter(|&(_, s)| s <= i).max_by_key(|&(_, s)| s);
    let short = match p.name.as_str() {
        "Pharaoh_Terrain" => "T",
        "Pharaoh_General" => "G",
        "Expansion" => "X",
        n => n,
    };
    match group {
        Some((g, s)) => format!("{short}{g}+{}", i - s),
        None => format!("{short}:{i}"),
    }
}

/// Map tile `(x, y)` lies inside the diamond the city view shows (tiles in the corners
/// of the map's rectangle are never drawn and hold no image).
fn inside_diamond(s: &Scenario, x: i32, y: i32) -> bool {
    const GRID: i32 = 228;
    let (gx, gy) = (s.info.start_offset % GRID + x, s.info.start_offset / GRID + y);
    (gx - gy).abs() < s.info.width / 2 + 1 && (gy - (GRID - gx) + 1).abs() < s.info.height / 2 + 1
}

/// Which tiles some image covers: every tile marked to draw paints its footprint (its
/// size from the bitfields, drawn from its leftmost tile as in orientation 0).
fn covered(map: &Map) -> Grid<bool> {
    let mut covered = Grid::new(map.width, map.height);
    for y in 0..map.height {
        for x in 0..map.width {
            if map.edges.at_or(x, y, 0) & 0x40 == 0 || map.images.at_or(x, y, 0) == 0 {
                continue;
            }
            let size = (map.bitfields.at_or(x, y, 0) & 0x0f) as i32 + 1;
            for yy in y + 1 - size..=y {
                for xx in x..x + size {
                    covered.set(xx, yy, true);
                }
            }
        }
    }
    covered
}

#[derive(Default)]
struct KindStats {
    tiles: usize,
    exact: usize,
    same_set: usize,
    misses: BTreeMap<String, usize>,
}

/// The Cleopatra missions store their cliffs 152 ids above this Expansion pack's cliff
/// group: they were saved by a build with more images before it.
const STORED_CLIFFS: u32 = 23928;
const CLIFF_ID_SHIFT: u32 = 152;

/// Redraws the terrain of every map and campaign mission saved at version 147 to 160
/// (later versions come from other editors) and compares it with the stored images,
/// per kind of terrain: exact matches, and matches within the set of images a rule
/// picks from by state the file doesn't hold (earthquake cracks, floodplain crops).
/// Also counts the tiles inside the diamond that nothing draws, as stored and as a new
/// city draws them. With `verbose`, prints where each kind of mismatch first occurs.
pub fn check_terrain_images(game: &Path, verbose: bool) -> Result<()> {
    let lib = ImageLibrary::open(&game.join("Data"))?;
    let defs = osiris_sim::Defs::load(&lib).map_err(anyhow::Error::msg)?;
    let mut stats: BTreeMap<(&str, &str), KindStats> = BTreeMap::new();
    let mut checked = 0;
    println!("{:32} {:>4} {:>7} {:>7} {:>6} {:>6}", "map or mission", "ver", "tiles", "exact", "holes", "after");
    for (name, s) in sources(game)? {
        if !(147..=160).contains(&s.version) {
            continue;
        }
        let group = if name.ends_with(".map") { "maps" } else { "missions" };
        checked += 1;
        let stored = Map::from_scenario(&s);
        let mut map = stored.clone();
        let choices = osiris_sim::terrain_images::rebuild(&mut map, &defs);
        let mut on_load = stored.clone();
        osiris_sim::terrain_images::redraw_on_load(&mut on_load, &defs, s.version);
        let (covered_stored, covered_on_load) = (covered(&stored), covered(&on_load));
        let (mut tiles, mut exact, mut holes, mut holes_on_load) = (0, 0, 0, 0);
        for y in 0..map.height {
            for x in 0..map.width {
                if !inside_diamond(&s, x, y) {
                    continue;
                }
                let kind = kind_of(stored.terrain.at_or(x, y, 0));
                if matches!(kind, "building" | "wall" | "canal") {
                    continue;
                }
                holes += !covered_stored.at_or(x, y, false) as usize;
                holes_on_load += !covered_on_load.at_or(x, y, false) as usize;
                let mut want = stored.images.at_or(x, y, 0);
                if kind == "cliff" && want >= STORED_CLIFFS {
                    want -= CLIFF_ID_SHIFT;
                }
                let got = map.images.at_or(x, y, 0);
                let st = stats.entry((group, kind)).or_default();
                st.tiles += 1;
                tiles += 1;
                if want == got {
                    st.exact += 1;
                    exact += 1;
                }
                if want == got || choices.at_or(x, y, Default::default()).contains(want) {
                    st.same_set += 1;
                } else {
                    let key = format!("{} -> {}", describe_id(&lib, want), describe_id(&lib, got));
                    let n = st.misses.entry(key.clone()).or_default();
                    *n += 1;
                    if verbose && *n == 1 {
                        println!("    {name} {x},{y}: stored {key}, terrain {:#x}", stored.terrain.at_or(x, y, 0));
                    }
                }
            }
        }
        println!("{name:32} {:>4} {tiles:>7} {exact:>7} {holes:>6} {holes_on_load:>6}", s.version);
    }
    println!("\n{checked} maps and missions. Per kind: tiles, exact matches, same image set, and the commonest misses (stored -> redrawn)");
    for ((group, kind), st) in &stats {
        let pct = |n: usize| 100.0 * n as f64 / st.tiles.max(1) as f64;
        println!("{group:8} {kind:11} {:8} {:8} {:8.3}% {:8} {:8.3}%", st.tiles, st.exact, pct(st.exact), st.same_set, pct(st.same_set));
        let mut misses: Vec<_> = st.misses.iter().collect();
        misses.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
        for (k, n) in misses.iter().take(if verbose { 10 } else { 3 }) {
            println!("{:30}{n:7}  {k}", "");
        }
    }
    Ok(())
}
