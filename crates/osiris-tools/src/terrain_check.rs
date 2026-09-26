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
pub struct KindStats {
    pub tiles: usize,
    pub exact: usize,
    pub same_set: usize,
    misses: BTreeMap<String, usize>,
}

/// The Cleopatra missions store their cliffs 152 ids above this Expansion pack's cliff
/// group: they were saved by a build with more images before it.
const STORED_CLIFFS: u32 = 23928;
const CLIFF_ID_SHIFT: u32 = 152;

/// What to print while checking.
#[derive(Default)]
pub struct Options {
    /// Where each kind of mismatch first occurs.
    pub verbose: bool,
    /// Every mismatched tile of one source (a map's file name or `mission1.pak #N`) and
    /// kind, with the grids the rules read.
    pub tiles: Option<(String, String)>,
}

/// Tile counts per group (`maps` or `missions`) and kind.
pub type Report = BTreeMap<(&'static str, &'static str), KindStats>;

/// `check-terrain-images <game dir> [-v] [--tiles <source> <kind>] [--baseline <file>]
/// [--write-baseline <file>]`: checks, then fails if any kind's exact matches fell below
/// the baseline's figure, or records the figures.
pub fn command(game: &Path, args: &[&str]) -> Result<()> {
    let mut options = Options::default();
    let (mut baseline, mut write) = (None, None);
    let mut i = 0;
    while i < args.len() {
        match (args[i], args.get(i + 1), args.get(i + 2)) {
            ("-v", _, _) => options.verbose = true,
            ("--tiles", Some(name), Some(kind)) => {
                options.tiles = Some((name.to_string(), kind.to_string()));
                i += 2;
            }
            ("--baseline", Some(file), _) => {
                baseline = Some(PathBuf::from(file));
                i += 1;
            }
            ("--write-baseline", Some(file), _) => {
                write = Some(PathBuf::from(file));
                i += 1;
            }
            (a, _, _) => anyhow::bail!("check-terrain-images: unexpected argument {a}"),
        }
        i += 1;
    }
    let report = check_terrain_images(game, &options)?;
    if let Some(file) = write {
        std::fs::write(&file, baseline_text(&report))?;
        println!("wrote {}", file.display());
    }
    if let Some(file) = baseline {
        let text = std::fs::read_to_string(&file)?;
        let drops = compare_with_baseline(&report, &text)?;
        if !drops.is_empty() {
            anyhow::bail!("terrain images fell below the baseline in {}:\n  {}", file.display(), drops.join("\n  "));
        }
        println!("every kind is at or above the baseline in {}", file.display());
    }
    Ok(())
}

/// The baseline file: one `group kind tiles exact same_set` line per kind.
pub fn baseline_text(report: &Report) -> String {
    let mut text = String::from("# check-terrain-images baseline: group, kind, tiles checked, exact matches, matches within the same set\n");
    for ((group, kind), st) in report {
        text += &format!("{group} {kind} {} {} {}\n", st.tiles, st.exact, st.same_set);
    }
    text
}

/// Each kind whose exact or same-set matches fell below its baseline figures (or whose
/// tile count changed, which means the sources or the kinds changed and the baseline
/// needs a look).
pub fn compare_with_baseline(report: &Report, baseline: &str) -> Result<Vec<String>> {
    let mut drops = Vec::new();
    for line in baseline.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')) {
        let f: Vec<&str> = line.split_whitespace().collect();
        let [group, kind, tiles, exact, same_set] = f[..] else { anyhow::bail!("bad baseline line: {line}") };
        let (tiles, exact, same_set): (usize, usize, usize) = (tiles.parse()?, exact.parse()?, same_set.parse()?);
        let Some(st) = report.iter().find(|((g, k), _)| *g == group && *k == kind).map(|(_, s)| s) else {
            drops.push(format!("{group} {kind}: no tiles (baseline {tiles})"));
            continue;
        };
        if st.tiles != tiles {
            drops.push(format!("{group} {kind}: {} tiles checked, baseline {tiles}", st.tiles));
        } else if st.exact < exact || st.same_set < same_set {
            drops.push(format!("{group} {kind}: {} exact and {} in the same set, baseline {exact} and {same_set}", st.exact, st.same_set));
        }
    }
    Ok(drops)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The game data: `$OSIRIS_TEST_DATA`, else `PharaohData` at the top of the
    /// workspace, where development keeps it.
    fn data_dir() -> Option<PathBuf> {
        let dir = std::env::var_os("OSIRIS_TEST_DATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../PharaohData"));
        dir.join("Maps").is_dir().then_some(dir)
    }

    /// Redraws every shipped map and campaign mission and fails if any kind of terrain
    /// matches fewer stored images than `data/terrain_baseline.txt` records. Needs the
    /// game data and takes minutes unoptimised, so run it in release after touching
    /// terrain images (4 s):
    ///
    /// ```sh
    /// cargo test --release -p osiris-tools -- --ignored
    /// ```
    ///
    /// After a rule gets better, record the new figures with `cargo run --release -p
    /// osiris-tools -- check-terrain-images PharaohData --write-baseline
    /// crates/osiris-tools/data/terrain_baseline.txt`.
    #[test]
    #[ignore = "needs the game data and a release build; see the doc comment"]
    fn terrain_images_hold_the_baseline() {
        let data = data_dir().expect("game data (set OSIRIS_TEST_DATA if it isn't in PharaohData)");
        let report = check_terrain_images(&data, &Options::default()).expect("check the terrain images");
        let drops = compare_with_baseline(&report, include_str!("../data/terrain_baseline.txt")).expect("read the baseline");
        assert!(drops.is_empty(), "terrain images fell below the baseline:\n  {}", drops.join("\n  "));
    }
}

/// Redraws the terrain of every map and campaign mission saved at version 147 to 160
/// (later versions come from other editors) and compares it with the stored images,
/// per kind of terrain: exact matches, and matches within the set of images a rule
/// picks from by state the file doesn't hold (earthquake cracks, floodplain crops).
/// Also counts the tiles inside the diamond that nothing draws and the tiles outside it
/// that something does, as stored and as a new city draws them.
pub fn check_terrain_images(game: &Path, options: &Options) -> Result<Report> {
    let lib = ImageLibrary::open(&game.join("Data"))?;
    let defs = osiris_sim::Defs::load(&lib).map_err(anyhow::Error::msg)?;
    let mut stats: Report = BTreeMap::new();
    let mut checked = 0;
    println!("{:32} {:>4} {:>7} {:>7} {:>6} {:>6} {:>7} {:>6}", "map or mission", "ver", "tiles", "exact", "holes", "after", "outside", "after");
    for (name, s) in sources(game)? {
        if !(147..=160).contains(&s.version) {
            continue;
        }
        let group = if name.ends_with(".map") { "maps" } else { "missions" };
        checked += 1;
        let stored = Map::from_scenario(&s);
        // Some files were saved by an earlier build that laid bare land blocks right up
        // to the diamond's edge: redraw those that way, and say so.
        let mut map = stored.clone();
        let mut choices = osiris_sim::terrain_images::rebuild(&mut map, &defs);
        let mut legacy = stored.clone();
        let legacy_choices = osiris_sim::terrain_images::rebuild_with_edge_margin(&mut legacy, &defs, 0);
        let land_exact = |m: &Map| {
            (0..m.height).flat_map(|y| (0..m.width).map(move |x| (x, y))).filter(|&(x, y)| inside_diamond(&s, x, y) && kind_of(stored.terrain.at_or(x, y, 0)) == "land" && m.images.at_or(x, y, 0) == stored.images.at_or(x, y, 0)).count()
        };
        let no_margin = land_exact(&legacy) > land_exact(&map);
        if no_margin {
            (map, choices) = (legacy, legacy_choices);
        }
        let mut on_load = stored.clone();
        osiris_sim::terrain_images::redraw_on_load(&mut on_load, &defs);
        let (covered_stored, covered_on_load) = (covered(&stored), covered(&on_load));
        let (mut tiles, mut exact, mut holes, mut holes_on_load) = (0, 0, 0, 0);
        let (mut outside, mut outside_on_load) = (0, 0);
        let dump_kind = options.tiles.as_ref().filter(|(n, _)| *n == name || n == "*").map(|(_, k)| k.as_str());
        for y in 0..map.height {
            for x in 0..map.width {
                if !inside_diamond(&s, x, y) {
                    outside += (stored.images.at_or(x, y, 0) != 0) as usize;
                    outside_on_load += (on_load.images.at_or(x, y, 0) != 0) as usize;
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
                } else if dump_kind == Some(kind) {
                    let around: Vec<String> = osiris_sim::map::NEIGHBOURS.iter().map(|&(dx, dy)| format!("{:x}", stored.terrain_around(x + dx, y + dy, 0))).collect();
                    println!(
                        "    {name} {x},{y}: stored {} got {}, terrain {:#x} random {} moisture {} fertility {} veg {} bits {:#x} around [{}]",
                        describe_id(&lib, want),
                        describe_id(&lib, got),
                        stored.terrain.at_or(x, y, 0),
                        stored.random.at_or(x, y, 0),
                        stored.moisture.at_or(x, y, 0),
                        stored.fertility.at_or(x, y, 0),
                        stored.vegetation.at_or(x, y, 0),
                        stored.bitfields.at_or(x, y, 0),
                        around.join(" ")
                    );
                }
                if want == got || choices.at_or(x, y, Default::default()).contains(want) {
                    st.same_set += 1;
                } else {
                    let key = format!("{} -> {}", describe_id(&lib, want), describe_id(&lib, got));
                    let n = st.misses.entry(key.clone()).or_default();
                    *n += 1;
                    if options.verbose && *n == 1 {
                        println!("    {name} {x},{y}: stored {key}, terrain {:#x}", stored.terrain.at_or(x, y, 0));
                    }
                }
            }
        }
        println!("{name:32} {:>4} {tiles:>7} {exact:>7} {holes:>6} {holes_on_load:>6} {outside:>7} {outside_on_load:>6}{}", s.version, if no_margin { "  (no edge margin)" } else { "" });
    }
    println!("\n{checked} maps and missions. Per kind: tiles, exact matches, same image set, and the commonest misses (stored -> redrawn)");
    for ((group, kind), st) in &stats {
        let pct = |n: usize| 100.0 * n as f64 / st.tiles.max(1) as f64;
        println!("{group:8} {kind:11} {:8} {:8} {:8.3}% {:8} {:8.3}%", st.tiles, st.exact, pct(st.exact), st.same_set, pct(st.same_set));
        let mut misses: Vec<_> = st.misses.iter().collect();
        misses.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
        for (k, n) in misses.iter().take(if options.verbose { 10 } else { 3 }) {
            println!("{:30}{n:7}  {k}", "");
        }
    }
    Ok(stats)
}
