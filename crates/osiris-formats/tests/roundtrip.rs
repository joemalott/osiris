//! Writing maps back: every shipped map and campaign entry must survive
//! parse -> to_bytes -> parse unchanged. Needs the original game's data (`PharaohData/`,
//! or `OSIRIS_TEST_DATA`); without it the tests pass without checking anything.

use osiris_formats::{ChunkFile, Layout, MissionPak, Scenario, ScenarioInfo};
use std::path::PathBuf;

fn data_dir() -> Option<PathBuf> {
    let dir = std::env::var_os("OSIRIS_TEST_DATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../PharaohData"));
    dir.is_dir().then_some(dir)
}

fn maps(data: &std::path::Path) -> Vec<PathBuf> {
    let mut maps: Vec<PathBuf> = std::fs::read_dir(data.join("Maps"))
        .expect("read Maps")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("map")))
        .collect();
    maps.sort();
    maps
}

/// Serializes `file`, reads the result back and checks both hold the same chunks.
fn check_round_trip(what: &str, file: &ChunkFile) {
    let bytes = file.to_bytes();
    let again = ChunkFile::parse(&bytes, file.layout).unwrap_or_else(|e| panic!("{what}: {e}"));
    assert_eq!(again.version, file.version, "{what}");
    assert_eq!(again.trailing, file.trailing, "{what}");
    assert!(file.names().eq(again.names()), "{what}");
    for name in file.names() {
        assert!(file.get(name) == again.get(name), "{what}: chunk {name} differs");
        assert_eq!(file.storage(name), again.storage(name), "{what}: chunk {name}");
    }
    let a = Scenario::from_chunks(file).unwrap();
    let b = Scenario::from_chunks(&again).unwrap();
    assert_eq!(format!("{a:?}"), format!("{b:?}"), "{what}: scenario differs");

    // Writing the unchanged scenario back changes nothing either.
    let rewritten = a.to_chunk_file(again).unwrap();
    for name in file.names() {
        assert!(file.get(name) == rewritten.get(name), "{what}: rewritten chunk {name} differs");
    }
}

fn check_info(what: &str, file: &ChunkFile) {
    let original = file.get("scenario_info").unwrap();
    let info = ScenarioInfo::parse(original).unwrap();
    let mut written = original.to_vec();
    info.write_into(&mut written);
    assert!(written == original, "{what}: scenario_info not reproduced");
}

#[test]
fn maps_round_trip() {
    let Some(data) = data_dir() else { return };
    let maps = maps(&data);
    assert!(!maps.is_empty());
    for path in maps {
        let what = path.display().to_string();
        let original = std::fs::read(&path).unwrap();
        let file = ChunkFile::parse(&original, Layout::Map).unwrap();
        check_round_trip(&what, &file);
        check_info(&what, &file);
    }
}

#[test]
fn campaign_entries_round_trip() {
    let Some(data) = data_dir() else { return };
    let Ok(pak) = MissionPak::open(&data.join("mission1.pak")) else { return };
    let mut checked = 0;
    for i in 0..pak.slots() {
        if pak.entry(i).is_none() {
            continue;
        }
        let file = pak.chunks(i).unwrap();
        let what = format!("mission1.pak entry {i}");
        check_round_trip(&what, &file);
        check_info(&what, &file);
        checked += 1;
    }
    assert!(checked > 0);
}

#[test]
fn edits_survive_a_save() {
    let Some(data) = data_dir() else { return };
    let path = data.join("Maps").join("Default.map");
    let Ok(file) = ChunkFile::open(&path, Layout::Map) else { return };
    let mut s = Scenario::from_chunks(&file).unwrap();
    let off = s.offset(10, 12).unwrap();
    s.elevation[off] = 3;
    s.terrain[off] ^= osiris_formats::scenario::terrain::ROCK;
    s.info.initial_funds = 12345;
    s.info.subtitle = "Written by Osiris".into();
    s.info.gods_known = [true, false, true, false, true];
    s.info.win.population.enabled = true;
    s.info.win.population.value = 4321;
    let out = std::env::temp_dir().join(format!("osiris-roundtrip-{}.map", std::process::id()));
    s.save_map(&path, &out).unwrap();
    let back = Scenario::from_chunks(&ChunkFile::open(&out, Layout::Map).unwrap()).unwrap();
    std::fs::remove_file(&out).ok();
    assert_eq!(back.elevation[off], 3);
    assert_eq!(back.terrain[off], s.terrain[off]);
    assert_eq!(back.info.initial_funds, 12345);
    assert_eq!(back.info.subtitle, "Written by Osiris");
    assert_eq!(back.info.gods_known, [true, false, true, false, true]);
    assert!(back.info.win.population.enabled);
    assert_eq!(back.info.win.population.value, 4321);
    assert_eq!(format!("{:?}", back.empire), format!("{:?}", s.empire));
}

/// The invasion points are one list of sixteen, the x's before the y's: land points
/// first, then sea points, each on the map's edge.
#[test]
fn invasion_points_are_sixteen_xs_then_ys() {
    let Some(data) = data_dir() else { return };
    let s = Scenario::load_map(&data.join("Maps/Hostile Nations.map")).unwrap();
    let p = |v: &[osiris_formats::scenario::TilePoint], i: usize| (v[i].x, v[i].y);
    assert_eq!(p(&s.info.invasion_points_land, 0), (46, 25));
    assert_eq!(p(&s.info.invasion_points_land, 1), (7, 65));
    assert_eq!(p(&s.info.invasion_points_sea, 0), (70, 1));
    assert_eq!(p(&s.info.invasion_points_sea, 3), (138, 69));
}
