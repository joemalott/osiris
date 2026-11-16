//! What the Mission Editor saves: every field it sets lands at the byte the original
//! keeps it at (the offsets in notes/editor.md, read from FUN_00478440, FUN_00532920,
//! FUN_00533470, FUN_00448a90 and the editor's other windows), and reads back as it
//! was written. Needs the original game's data (`PharaohData/`, or `OSIRIS_TEST_DATA`);
//! without it the tests pass without checking anything.

use osiris_formats::events::{RECORD, events_from_chunks, write_events};
use osiris_formats::scenario::{Goal, TilePoint};
use osiris_formats::{ChunkFile, EventRecord, EventValue, Layout, Scenario, ScenarioInfo};
use std::path::PathBuf;

fn data_dir() -> Option<PathBuf> {
    let dir = std::env::var_os("OSIRIS_TEST_DATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../PharaohData"));
    dir.is_dir().then_some(dir)
}

fn map(data: &std::path::Path, name: &str) -> ChunkFile {
    ChunkFile::parse(&std::fs::read(data.join("Maps").join(name)).unwrap(), Layout::Map).unwrap()
}

fn i16_at(b: &[u8], at: usize) -> i16 {
    i16::from_le_bytes([b[at], b[at + 1]])
}

fn i32_at(b: &[u8], at: usize) -> i32 {
    i32::from_le_bytes(b[at..at + 4].try_into().unwrap())
}

fn tp(x: i32, y: i32) -> TilePoint {
    TilePoint { x, y }
}

/// Every field of the scenario's info the editor sets, each away from what
/// Default.map holds, is written at its byte and parses back the same; the bytes
/// the editor doesn't model (the era at 996, the open-play number at 998) are left
/// as they were.
#[test]
fn info_fields_land_where_the_original_keeps_them() {
    let Some(data) = data_dir() else { return };
    let file = map(&data, "Default.map");
    let original = file.get("scenario_info").unwrap().to_vec();
    let mut i = ScenarioInfo::parse(&original).unwrap();
    i.start_year = -2345;
    i.gods = [2, 1, 0, 1, 1];
    i.gods_known = [true, true, false, true, true];
    i.initial_funds = 76543;
    i.enemy_id = 9;
    i.subtitle = "Editor field test".into();
    i.image_id = 21;
    i.is_open_play = true;
    i.player_rank = 7;
    i.predator_herd_points = (0..4).map(|n| tp(10 + n, 20 + n)).collect();
    i.fishing_points = (0..8).map(|n| tp(30 + n, 40 + n)).collect();
    i.alt_predator_type = 1;
    i.invasion_points_land = (0..8).map(|n| tp(50 + n, 60 + n)).collect();
    i.invasion_points_sea = (0..8).map(|n| tp(70 + n, 80 + n)).collect();
    let w = &mut i.win;
    w.culture = Goal { enabled: true, value: 41 };
    w.prosperity = Goal { enabled: true, value: 42 };
    w.monuments = Goal { enabled: true, value: 43 };
    w.kingdom = Goal { enabled: true, value: 44 };
    w.housing_count = Goal { enabled: true, value: 45 };
    w.housing_level = Goal { enabled: true, value: 12 };
    w.time_limit = Goal { enabled: true, value: 46 };
    w.survival_time = Goal { enabled: true, value: 47 };
    w.population = Goal { enabled: true, value: 4800 };
    w.milestone_years = [5, 10, 15];
    i.earthquake_point = tp(90, 91);
    i.entry_point = tp(92, 93);
    i.exit_point = tp(94, 95);
    i.river_entry_point = tp(96, 97);
    i.river_exit_point = tp(98, 99);
    i.rescue_loan = 3333;
    i.climate = 2;
    i.player_faction = 1;
    i.prey_herd_points = (0..4).map(|n| tp(100 + n, 110 + n)).collect();
    i.reserved.resize(114, 0);
    i.reserved[2] = 1;
    i.reserved[45] = 1;
    i.reserved[104 + 3] = 1;
    i.disembark_points = (0..3).map(|n| tp(120 + n, 130 + n)).collect();
    i.debt_interest_rate = 17;
    i.monuments = [3, 21, 25];
    i.burial_provisions_required.resize(36, 0);
    i.burial_provisions_required[13] = 1600;
    i.burial_provisions_required[30] = 320;
    i.current_pharaoh = 88;
    i.player_incarnation = 12;

    let mut b = original.clone();
    i.write_into(&mut b);
    assert_eq!(format!("{:?}", ScenarioInfo::parse(&b).unwrap()), format!("{i:?}"));

    // Starting conditions (FUN_00532920).
    assert_eq!(i16_at(&b, 0), -2345, "start year");
    assert_eq!(i32_at(&b, 32), 76543, "funds");
    assert_eq!(i16_at(&b, 650), 7, "rank");
    assert_eq!(i32_at(&b, 956), 3333, "Pharaoh's gift");
    assert_eq!([i32_at(&b, 960), i32_at(&b, 964), i32_at(&b, 968)], [5, 10, 15], "milestones");
    assert_eq!(i32_at(&b, 1284), 17, "debt interest");
    assert_eq!(i32_at(&b, 1584), 88, "current Pharaoh");
    assert_eq!(i32_at(&b, 1588), 12, "incarnation");
    // The Options screen (FUN_004138d0).
    assert_eq!(&b[60..78], b"Editor field test\0", "brief description");
    assert_eq!(i16_at(&b, 36), 9, "enemy");
    assert_eq!(b[984], 2, "terrain set");
    assert_eq!(b[997], 1, "player fights Egypt");
    assert_eq!(i16_at(&b, 646), 21, "picture");
    assert_eq!([b[10], b[12], b[14], b[16], b[18]], [2, 1, 0, 1, 1], "gods");
    assert_eq!(i16_at(&b, 1240 + 2 * 3), 1, "Seth's temple complex");
    assert_eq!((i16_at(&b, 1032 + 2 * 2), i16_at(&b, 1032 + 2 * 45)), (1, 1), "buildings allowed");
    // Win criteria (FUN_00533470).
    assert_eq!(i16_at(&b, 648), 1, "open play");
    assert_eq!([i32_at(&b, 844), i32_at(&b, 848), i32_at(&b, 852), i32_at(&b, 856), i32_at(&b, 860), i32_at(&b, 864)], [41, 42, 43, 44, 45, 12]);
    assert_eq!(&b[868..874], &[1, 1, 1, 1, 1, 1]);
    assert_eq!([i32_at(&b, 880), i32_at(&b, 884), i32_at(&b, 888), i32_at(&b, 892), i32_at(&b, 896), i32_at(&b, 900)], [1, 46, 1, 47, 1, 4800]);
    assert_eq!([i16_at(&b, 1288), i16_at(&b, 1290), i16_at(&b, 1292)], [3, 21, 25], "monuments");
    assert_eq!((i32_at(&b, 1296 + 4 * 13), i32_at(&b, 1296 + 4 * 30)), (1600, 320), "burial provisions");
    // The points (FUN_00478440's first branch).
    assert_eq!((i16_at(&b, 904), i16_at(&b, 906)), (90, 91), "earthquake");
    assert_eq!((i16_at(&b, 908), i16_at(&b, 910)), (92, 93), "entry");
    assert_eq!((i16_at(&b, 912), i16_at(&b, 914)), (94, 95), "exit");
    assert_eq!((i16_at(&b, 948), i16_at(&b, 950)), (96, 97), "river in");
    assert_eq!((i16_at(&b, 952), i16_at(&b, 954)), (98, 99), "river out");
    for n in 0..16 {
        let (x, y) = if n < 8 { (50 + n, 60 + n) } else { (70 + n - 8, 80 + n - 8) };
        assert_eq!((i16_at(&b, 744 + 2 * n), i16_at(&b, 776 + 2 * n)), (x as i16, y as i16), "invasion point {n}");
    }
    for n in 0..8 {
        assert_eq!((i16_at(&b, 668 + 2 * n), i16_at(&b, 684 + 2 * n)), (30 + n as i16, 40 + n as i16), "fishing point {n}");
    }
    for n in 0..4 {
        assert_eq!((i16_at(&b, 652 + 2 * n), i16_at(&b, 660 + 2 * n)), (10 + n as i16, 20 + n as i16), "predator point {n}");
        assert_eq!((i32_at(&b, 1000 + 4 * n), i32_at(&b, 1016 + 4 * n)), (100 + n as i32, 110 + n as i32), "prey point {n}");
    }
    for n in 0..3 {
        assert_eq!((i32_at(&b, 1260 + 4 * n), i32_at(&b, 1272 + 4 * n)), (120 + n as i32, 130 + n as i32), "disembark point {n}");
    }
    assert_eq!(i16_at(&b, 700), 1, "the climate's other beast");
    // Left alone: the era (written by the editor itself) and the open-play number.
    assert_eq!((b[996], b[998], b[999]), (original[996], original[998], original[999]));
}

/// An event with every field the planning window sets lands at the record's bytes
/// the original reads (file offset = memory offset + 4 from 0x785d70), and reads
/// back the same.
#[test]
fn event_fields_land_where_the_original_keeps_them() {
    let Some(data) = data_dir() else { return };
    let mut file = map(&data, "Default.map");
    let base = |e: &EventRecord| EventRecord { raw: Vec::new(), time: EventValue { value: 0, fixed: 0, ..e.time }, ..e.clone() };
    let a = EventRecord {
        kind: 2,
        month: 7,
        item: EventValue { value: 3, fixed: 3, min: -1, max: -1 },
        amount: EventValue { value: 12, fixed: -1, min: 10, max: 20 },
        year: 4,
        year_fixed: -1,
        time: EventValue { value: 0, fixed: 0, min: 3, max: 6 },
        location: [10, -1, 9, 12],
        on_completed: 1,
        on_refusal: -1,
        on_too_late: -1,
        on_defeat: -1,
        trigger: 0x10,
        months: 18,
        god: 4,
        attack_target: 3,
        sender: 0,
        route: [5, 5, -1, -1],
        subtype: 0,
        link_reasons: [1, 2, 3, 4],
        ..Default::default()
    };
    let b = EventRecord {
        kind: 1,
        month: 11,
        item: EventValue { value: 15, fixed: 15, min: 17, max: 19 },
        amount: EventValue { value: 6, fixed: 6, min: -1, max: -1 },
        year: 2,
        year_fixed: 2,
        time: EventValue { value: 0, fixed: 0, min: -1, max: -1 },
        location: [3, 3, -1, -1],
        on_completed: -1,
        on_refusal: 0,
        on_too_late: 0,
        on_defeat: 0,
        trigger: 1,
        months: 24,
        god: 2,
        sender: 1,
        route: [0, -1, 0, 49],
        subtype: 3,
        link_reasons: [6, 5, 0, 6],
        ..Default::default()
    };
    write_events(&mut file, &[a.clone(), b.clone()]).unwrap();
    let chunk = file.get("scenario_events").unwrap();
    assert_eq!(i32_at(chunk, 0), 2, "the count, the list's own i32 before the records");
    let r = &chunk[RECORD..2 * RECORD];
    assert_eq!(i16_at(r, 4), 1, "the event's number");
    assert_eq!((r[6], r[7]), (1, 11), "type and month");
    assert_eq!([i16_at(r, 8), i16_at(r, 10), i16_at(r, 12), i16_at(r, 14)], [15, 15, 17, 19], "items");
    assert_eq!([i16_at(r, 16), i16_at(r, 18), i16_at(r, 20), i16_at(r, 22)], [6, 6, -1, -1], "amount");
    assert_eq!([i16_at(r, 24), i16_at(r, 26), i16_at(r, 28), i16_at(r, 30)], [2, 2, -1, -1], "year");
    assert_eq!([i16_at(r, 32), i16_at(r, 34), i16_at(r, 36), i16_at(r, 38)], [3, 3, -1, -1], "cities");
    assert_eq!((i16_at(r, 40), i16_at(r, 42), i16_at(r, 86), i16_at(r, 88)), (-1, 0, 0, 0), "follow-ups");
    assert_eq!(i32_at(r, 44), 1, "the trigger, a dword");
    assert_eq!(i16_at(r, 48), 24, "months to comply");
    assert_eq!((r[58], r[90], r[100]), (2, 1, 3), "god, sender, reason");
    assert_eq!([i16_at(r, 92), i16_at(r, 94), i16_at(r, 96), i16_at(r, 98)], [0, -1, 0, 49], "route");
    assert_eq!(&r[110..114], &[6, 5, 0, 6], "follow-up reasons");
    let r = &chunk[..RECORD];
    assert_eq!((i32_at(r, 44), r[58], i16_at(r, 60)), (0x10, 4, 3), "favour trigger, warships, target");

    let back = events_from_chunks(&file).unwrap();
    assert_eq!(back.len(), 2);
    assert_eq!(base(&back[0]), base(&a));
    assert_eq!(base(&back[1]), base(&b));
}

/// A count of 0 is no events at all (FUN_0044ba90 runs `0 < count`), whatever the
/// records behind it hold: deleting every event keeps the stale bytes but reads
/// back as none.
#[test]
fn no_events_is_a_count_of_zero() {
    let Some(data) = data_dir() else { return };
    let mut file = map(&data, "Warfare.map");
    let before = events_from_chunks(&file).unwrap();
    assert!(!before.is_empty());
    let stale = file.get("scenario_events").unwrap()[4..RECORD].to_vec();
    write_events(&mut file, &[]).unwrap();
    let chunk = file.get("scenario_events").unwrap();
    assert_eq!(i32_at(chunk, 0), 0);
    assert_eq!(&chunk[4..RECORD], &stale[..]);
    assert!(events_from_chunks(&file).unwrap().is_empty());
    let again = ChunkFile::parse(&file.to_bytes(), Layout::Map).unwrap();
    assert!(Scenario::from_chunks(&again).unwrap().events.is_empty());
}

/// The Kingdom map's edits come back from the file: an object's kind, place, name,
/// goods, demand, route and cost, a route's points and kind, and the prices.
#[test]
fn empire_edits_round_trip() {
    let Some(data) = data_dir() else { return };
    let file = map(&data, "Warfare.map");
    let mut s = Scenario::from_chunks(&file).unwrap();
    let free = s.empire.objects.iter().position(|o| !o.in_use).unwrap();
    {
        let o = &mut s.empire.objects[free];
        o.in_use = true;
        o.kind = osiris_formats::empire::object::CITY;
        o.city_type = osiris_formats::empire::city::FOREIGN_TRADING;
        o.x = 321;
        o.y = 654;
        o.width = 40;
        o.height = 30;
        o.city_name_id = 17;
        o.text_align = 2;
        o.trade_route_id = 7;
        o.trade_route_cost = 2100;
        o.sells = vec![5, 9, 20];
        o.buys = vec![1, 13];
        o.demand.resize(36, 0);
        o.demand[5] = 3;
        o.demand[9] = 1;
        o.demand[20] = 2;
        o.demand[1] = 3;
        o.demand[13] = 1;
    }
    let route = s.empire.routes.iter().position(|r| !r.in_use).unwrap();
    {
        let r = &mut s.empire.routes[route];
        r.in_use = true;
        r.route_type = 2;
        r.step = 5;
        r.points = vec![(321, 654), (400, 700), (500, 720)];
        r.from_object = free as i16;
        r.to_object = -1;
    }
    s.empire.prices[12] = (155, 125);
    s.empire.prices[30] = (41, 33);
    let back = Scenario::from_chunks(&ChunkFile::parse(&s.to_chunk_file(file).unwrap().to_bytes(), Layout::Map).unwrap()).unwrap();
    let (o, b) = (&s.empire.objects[free], &back.empire.objects[free]);
    assert_eq!((b.in_use, b.kind, b.city_type, b.x, b.y, b.width, b.height), (true, o.kind, o.city_type, 321, 654, 40, 30));
    assert_eq!((b.city_name_id, b.text_align, b.trade_route_id, b.trade_route_cost), (17, 2, 7, 2100));
    assert_eq!((b.sells.clone(), b.buys.clone()), (vec![5, 9, 20], vec![1, 13]));
    for r in [5, 9, 20, 1, 13] {
        assert_eq!(b.demand[r], o.demand[r], "demand for {r}");
    }
    let r = &back.empire.routes[route];
    assert_eq!((r.in_use, r.route_type, r.step, r.points.clone(), r.from_object, r.to_object), (true, 2, 5, vec![(321, 654), (400, 700), (500, 720)], free as i16, -1));
    assert_eq!((back.empire.prices[12], back.empire.prices[30]), ((155, 125), (41, 33)));
    // Everything else as it was.
    for (n, (x, y)) in s.empire.objects.iter().zip(&back.empire.objects).enumerate() {
        if n != free {
            assert_eq!((x.in_use, x.kind, x.city_type, x.sells.clone(), x.buys.clone()), (y.in_use, y.kind, y.city_type, y.sells.clone(), y.buys.clone()), "object {n}");
        }
    }
}

/// Every shipped map brought up to the editor's version 160 (as the original's
/// loader upgrades it, FUN_004d4ae0) writes out and reads back in that version's
/// layout with the same terrain, Kingdom and events but for the upgrades: deben and
/// troops renumbered past Cleopatra's goods, the zoo cleared until set, which a
/// version this new then keeps.
#[test]
fn old_maps_upgrade_to_the_editors_version() {
    use osiris_formats::upgrade::{EDITOR_VERSION, item_is_resource, map_as_saved, upgrade_item};
    let Some(data) = data_dir() else { return };
    let mut names: Vec<_> = std::fs::read_dir(data.join("Maps")).unwrap().filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().into_owned()).filter(|n| n.ends_with(".map") && !n.starts_with('_')).collect();
    names.sort();
    assert!(names.len() > 30);
    for name in names {
        let file = map(&data, &name);
        let up = map_as_saved(&file).unwrap();
        let back = ChunkFile::parse(&up.to_bytes(), Layout::Map).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(back.version, EDITOR_VERSION, "{name}");
        let (a, mut b) = (Scenario::from_chunks(&file).unwrap(), Scenario::from_chunks(&back).unwrap());
        assert_eq!(a.terrain, b.terrain, "{name}");
        assert_eq!(a.empire.prices, b.empire.prices, "{name}");
        for (n, (x, y)) in a.empire.objects.iter().zip(&b.empire.objects).enumerate() {
            assert_eq!((x.in_use, x.kind, x.x, x.y, x.city_type, &x.sells, &x.buys, &x.demand), (y.in_use, y.kind, y.x, y.y, y.city_type, &y.sells, &y.buys, &y.demand), "{name}: object {n}");
            assert_eq!(&x.raw[..62], &y.raw[..62], "{name}: object {n}'s bytes");
        }
        assert_eq!(a.events.len(), b.events.len(), "{name}");
        for (x, y) in a.events.iter().zip(&b.events) {
            let item = if file.version < 152 && item_is_resource(x.kind) { upgrade_item(x.item) } else { x.item };
            assert_eq!((x.kind, item, x.amount, x.year), (y.kind, y.item, y.amount, y.year), "{name}");
        }
        if file.version < 155 {
            assert!(!b.allowed_structures()[45], "{name}: no zoo until set");
            b.info.reserved[45] = 1;
            let mut again = back.clone();
            b.write_chunks(&mut again).unwrap();
            assert!(Scenario::from_chunks(&ChunkFile::parse(&again.to_bytes(), Layout::Map).unwrap()).unwrap().allowed_structures()[45], "{name}: the zoo, once allowed, stays");
        }
        // Upgrading again is nothing.
        assert!(map_as_saved(&back).unwrap().to_bytes() == back.to_bytes(), "{name}");
    }
}
