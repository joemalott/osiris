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

/// The Kingdom map's edits come back from a saved map: a city's kind, name, goods,
/// demand, route and cost, an object moved, one deleted and one added, a new sea
/// route, and prices.
#[test]
fn empire_edits_survive_a_save() {
    use osiris_formats::empire::{EmpireObject, EmpireRoute, city, object};
    let Some(data) = data_dir() else { return };
    let path = data.join("Maps").join("Default.map");
    let Ok(file) = ChunkFile::open(&path, Layout::Map) else { return };
    let mut s = Scenario::from_chunks(&file).unwrap();
    let e = &mut s.empire;
    let c = e.objects.iter().position(|o| o.in_use && o.kind == object::CITY).unwrap();
    {
        let o = &mut e.objects[c];
        o.city_type = city::FOREIGN_TRADING;
        o.city_name_id = 36;
        o.text_align = 3;
        o.sells = vec![20, 26, 29, 32];
        o.buys = vec![1, 13];
        o.demand = vec![0; 36];
        for (r, t) in [(20, 3), (26, 1), (29, 2), (32, 2), (1, 2), (13, 3)] {
            o.demand[r] = t;
        }
        o.trade_route_id = 1;
        o.trade_route_cost = 1500;
        o.x += 17;
        o.y -= 9;
    }
    // Delete the next object, closing the gap, and add a region at the end.
    e.objects.remove(c + 1);
    let last = e.objects.iter().position(|o| !o.in_use).unwrap();
    e.objects.insert(last, EmpireObject { kind: object::REGION, in_use: true, x: 300, y: 400, width: 90, height: 20, city_name_id: 4, ..Default::default() });
    e.objects.truncate(200);
    e.routes[1] = EmpireRoute { in_use: true, route_type: 2, step: 5, points: vec![(100, 100), (200, 180), (320, 190)], from_object: c as i16, to_object: -1, raw: Vec::new() };
    e.prices[20] = (999, 888);

    let out = std::env::temp_dir().join(format!("osiris-empire-roundtrip-{}.map", std::process::id()));
    s.save_map(&path, &out).unwrap();
    let back = Scenario::from_chunks(&ChunkFile::open(&out, Layout::Map).unwrap()).unwrap();
    std::fs::remove_file(&out).ok();
    let (a, b) = (&s.empire, &back.empire);
    let o = &b.objects[c];
    assert_eq!((o.city_type, o.city_name_id, o.text_align, o.trade_route_id, o.trade_route_cost), (city::FOREIGN_TRADING, 36, 3, 1, 1500));
    assert_eq!((o.x, o.y), (a.objects[c].x, a.objects[c].y));
    assert_eq!((&o.sells, &o.buys), (&vec![20, 26, 29, 32], &vec![1, 13]));
    for r in [20, 26, 29, 1, 13] {
        assert_eq!(o.demand[r], a.objects[c].demand[r], "demand for {r}");
    }
    // Resources past the file's masks come back at the middle tier, as the game reads them.
    assert_eq!(o.demand[32], 2);
    for i in 0..200 {
        assert_eq!((b.objects[i].kind, b.objects[i].in_use, b.objects[i].city_name_id, b.objects[i].x), (a.objects[i].kind, a.objects[i].in_use, a.objects[i].city_name_id, a.objects[i].x), "object {i}");
    }
    let r = &b.routes[1];
    assert_eq!((r.in_use, r.route_type, r.step, &r.points, r.from_object, r.to_object), (true, 2, 5, &a.routes[1].points, c as i16, -1));
    assert_eq!(b.prices[20], (999, 888));
    assert_eq!(b.prices[1], a.prices[1]);
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

/// Events edited, added and deleted come back from a saved map with every field the
/// Mission Editor sets; the events left alone keep their bytes.
#[test]
fn event_edits_survive_a_save() {
    use osiris_formats::{EventRecord, EventValue};
    let Some(data) = data_dir() else { return };
    let path = data.join("Maps").join("Warfare.map");
    let Ok(file) = ChunkFile::open(&path, Layout::Map) else { return };
    let mut s = Scenario::from_chunks(&file).unwrap();
    let before = s.events.clone();
    assert!(before.len() > 10);
    // Change the first event into a festival request from a city.
    {
        let e = &mut s.events[0];
        e.kind = 1;
        e.subtype = 3;
        e.sender = 0;
        e.god = 2;
        e.trigger = 0;
        e.month = 5;
        e.year_fixed = -1;
        e.time.min = 3;
        e.time.max = 6;
        e.item = EventValue { value: 15, fixed: 15, min: 5, max: -1 };
        e.amount = EventValue { value: 7, fixed: -1, min: 7, max: 12 };
        e.location = [2, -1, 2, 4];
        e.months = 18;
        e.on_completed = 3;
        e.on_refusal = -1;
        e.on_too_late = 9;
        e.on_defeat = -1;
        e.link_reasons = [0, 2, 5, 6];
    }
    // Delete the second, and add an invasion by sea at the end.
    s.events.remove(1);
    s.events.push(EventRecord {
        kind: 2,
        trigger: 0x10,
        item: EventValue { value: 1, fixed: 3, min: -1, max: -1 },
        amount: EventValue { value: 0, fixed: -1, min: 20, max: 40 },
        location: [0, -1, 9, 12],
        route: [0, 2, -1, -1],
        months: 6,
        god: 4,
        attack_target: 3,
        on_completed: -1,
        on_refusal: -1,
        on_too_late: -1,
        on_defeat: -1,
        year_fixed: 2,
        time: EventValue { value: 0, fixed: 0, min: -1, max: -1 },
        link_reasons: [6; 4],
        ..Default::default()
    });

    let out = std::env::temp_dir().join(format!("osiris-events-roundtrip-{}.map", std::process::id()));
    s.save_map(&path, &out).unwrap();
    let back = Scenario::from_chunks(&ChunkFile::open(&out, Layout::Map).unwrap()).unwrap();
    std::fs::remove_file(&out).ok();
    assert_eq!(back.events.len(), s.events.len());
    let key = |e: &EventRecord| {
        format!(
            "{} {} {} {} {} {:?} {:?} {:?} {:?} {:?} {} {} {} {} {} {} {} {} {} {:?}",
            e.kind, e.subtype, e.sender, e.god, e.trigger, (e.month, e.year_fixed, e.time.min, e.time.max), e.item, e.amount, e.location, e.route, e.months, e.attack_target, e.on_completed, e.on_refusal, e.on_too_late, e.on_defeat, e.city, e.defeat_link, e.year, e.link_reasons
        )
    };
    for (a, b) in s.events.iter().zip(&back.events) {
        assert_eq!(key(a), key(b));
    }
    // The events after the deleted one are the file's own, moved up a slot and
    // numbered by their new place.
    for (i, (a, b)) in before[2..].iter().zip(&back.events[1..]).enumerate() {
        assert_eq!(a.raw[6..], b.raw[6..]);
        assert_eq!(i16::from_le_bytes([b.raw[4], b.raw[5]]), i as i16 + 1);
    }
}
