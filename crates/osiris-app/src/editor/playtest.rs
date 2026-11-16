//! A map made from scratch in the editor and then played: New map, a river painted
//! across it with its banks and some land, the points set where the tools allow
//! them, events of several types planned through the planning window's own setters,
//! then "Play this mission" and a year of the city that starts from the file it
//! writes. The city must take in people, and each event must do what its type says
//! in the month it is planned for.

use super::events::{EvField, pick, set_number};
use super::{DEFAULT_MAP, Editor, Point, Request, Tool, test_data};
use crate::editor::terrain::Paint;
use osiris_formats::{ImageLibrary, Model, Scenario};
use osiris_sim::buildings::kind;
use osiris_sim::map::terrain as bits;
use osiris_sim::world::{Command, Outcome};
use osiris_sim::{Balance, Defs, World};
use std::path::Path;
use std::sync::Arc;

fn editor(data: &Path, maps: &Path) -> Editor {
    let lib = ImageLibrary::open(&data.join("Data")).unwrap();
    let defs = Arc::new(Defs::load(&lib).unwrap());
    let text = crate::lang::english_text(data);
    Editor::open(&data.join("Maps").join(DEFAULT_MAP), defs, text, maps.to_owned()).unwrap()
}

/// The first tile (scanning `tiles`) where point `p` may go.
fn first_allowed(e: &Editor, p: Point, tiles: impl IntoIterator<Item = (i32, i32)>) -> (i32, i32) {
    tiles.into_iter().find(|&(x, y)| e.allowed(p, x, y)).unwrap_or_else(|| panic!("nowhere for {p:?}"))
}

fn place(e: &mut Editor, p: Point, at: (i32, i32)) {
    e.tool = Tool::Point(p);
    assert!(e.apply(at.0, at.1), "{p:?} at {at:?}");
}

/// Ticks to the start of the next month (where the monthly passes run).
fn next_month(w: &mut World) {
    let m = w.time.month;
    while w.time.month == m {
        w.tick();
    }
}

/// Builds a map in the editor, plays it a year, and checks what the year brings.
#[test]
fn a_new_map_plays() {
    let Some(data) = test_data() else { return };
    let maps = std::env::temp_dir().join(format!("osiris-playtest-{}", std::process::id()));
    let mut e = editor(&data, &maps);
    e.new_map(3);
    assert_eq!((e.map.width, e.map.height), (140, 140));

    // A river from the top corner to the bottom one, floodplain down its west bank,
    // trees, rock and marsh east of it.
    e.tool = Tool::Paint(Paint::Water);
    e.brush = 3;
    for y in 0..140 {
        e.apply(70, y);
    }
    e.tool = Tool::Paint(Paint::Floodplain);
    e.brush = 1;
    for y in 20..120 {
        e.apply(65, y);
    }
    e.brush = 2;
    for (paint, x0) in [(Paint::Trees, 90), (Paint::Rock(bits::ROCK), 100), (Paint::Marshland, 80)] {
        e.tool = Tool::Paint(paint);
        for y in 40..50 {
            e.apply(x0, y);
        }
    }
    let t = |e: &Editor, x, y| e.map.terrain.at_or(x, y, 0);
    assert!(t(&e, 70, 70) & bits::WATER != 0 && t(&e, 70, 70) & bits::DEEPWATER != 0, "a deep river");
    assert!(t(&e, 65, 70) & bits::FLOODPLAIN != 0);
    assert!(t(&e, 90, 45) & bits::TREE != 0 && t(&e, 100, 45) & bits::ROCK != 0 && t(&e, 80, 45) & bits::MARSHLAND != 0);
    assert!(t(&e, 58, 70) & bits::GROUNDWATER != 0 && t(&e, 20, 70) & bits::GROUNDWATER == 0, "grassland near the river, none far from it");

    // The points: people in at the west corner and out at the east one, the river's
    // ends at the top and bottom, an invasion point, fishing and the herds.
    assert_eq!(e.missing_points().len(), 3);
    let entry = first_allowed(&e, Point::Entry, (0..10).map(|x| (x, 70)));
    let exit = first_allowed(&e, Point::Exit, (130..140).rev().map(|x| (x, 70)));
    let river_in = first_allowed(&e, Point::RiverIn, (0..10).map(|y| (70, y)));
    let river_out = first_allowed(&e, Point::RiverOut, (130..140).rev().map(|y| (70, y)));
    let invasion = first_allowed(&e, Point::Invasion(0), (0..30).map(|k| (35 - k, 35 - k)));
    for (p, at) in [(Point::Entry, entry), (Point::Exit, exit), (Point::RiverIn, river_in), (Point::RiverOut, river_out), (Point::Invasion(0), invasion), (Point::Fishing(0), (70, 60)), (Point::Predator(0), (80, 45)), (Point::Prey(0), (40, 90))] {
        place(&mut e, p, at);
    }
    assert!(e.missing_points().is_empty());
    assert!(!e.allowed(Point::Entry, 70, 70), "nobody walks in on the river");

    // The events, all in the first year: a request for grain in February, a rise in
    // the price of pottery in March, a gift of 500 deben in April, the Kingdom's
    // favour up 5 in May and, two months after it, down 3; wages up 2 in June; and an
    // army in September.
    let s = |e: &mut Editor| -> usize { e.add_event().unwrap() };
    let request = s(&mut e);
    set_number(&mut e.scenario, request, EvField::Year(true), 0);
    pick(&mut e.scenario, request, EvField::Month, 1);
    let mut plan = |e: &mut Editor, kind: usize, month: usize, item: Option<usize>, amount: i32| {
        let i = e.add_event().unwrap();
        pick(&mut e.scenario, i, EvField::Kind, kind);
        set_number(&mut e.scenario, i, EvField::Year(true), 0);
        pick(&mut e.scenario, i, EvField::Month, month);
        if let Some(r) = item {
            pick(&mut e.scenario, i, EvField::Item(0), r);
        }
        // Both ends typed: the high one first, so a value under the 8 a new event
        // starts with is fixed rather than the low end of a range up to 8.
        set_number(&mut e.scenario, i, EvField::Amount(true), amount);
        set_number(&mut e.scenario, i, EvField::Amount(false), amount);
        assert_eq!(super::events::ends(e.scenario.events[i].amount), (amount as i16, amount as i16));
        i
    };
    let _price = plan(&mut e, 15, 2, Some(13), 10);
    let _gift = plan(&mut e, 23, 3, Some(36), 500);
    let favour = plan(&mut e, 17, 4, None, 5);
    let after = plan(&mut e, 18, 0, None, 3);
    e.cycle_trigger(after);
    e.cycle_trigger(after);
    set_number(&mut e.scenario, after, EvField::Year(false), 2);
    set_number(&mut e.scenario, favour, EvField::Link(0), after as i32);
    let _wages = plan(&mut e, 8, 5, None, 2);
    let army = plan(&mut e, 2, 8, Some(1), 4);
    set_number(&mut e.scenario, army, EvField::Months, 2);
    assert!(super::events::valid_link(&e.scenario.events, favour, after as i16));

    // Play this mission: the map is written where the game reads it.
    e.play().unwrap();
    let Some(Request::Play(path)) = e.request.take() else { panic!("no play request") };
    let scenario = Scenario::load_map(&path).unwrap();
    std::fs::remove_dir_all(&maps).ok();
    assert_eq!(scenario.events.len(), 7);
    assert_eq!((scenario.info.entry_point.x, scenario.info.entry_point.y), entry);

    let model = Model::parse(&String::from_utf8_lossy(&std::fs::read(data.join("Pharaoh_Model_Normal.txt")).unwrap())).unwrap();
    let mut w = World::new(&scenario, e.defs.clone(), Arc::new(Balance::from_model(&model)));
    w.begin_at(osiris_sim::difficulty::NORMAL);
    w.start(&scenario);
    assert!(w.map.terrain.at_or(70, 70, 0) & bits::WATER != 0, "the river is in the city");
    assert_eq!(w.invasions.land_points, vec![invasion]);
    assert!(!w.herds.is_empty(), "the herds from the predator and prey points");

    // Houses on a road in from the entry.
    let (ex, ey) = entry;
    let end = (ex + 24, ey);
    assert!(matches!(w.apply(&Command::Road { start: entry, end }), Outcome::Done { .. }), "a road from the entry");
    for x in [ex + 4, ex + 12] {
        assert!(matches!(w.apply(&Command::Build { kind: kind::VACANT_LOT, x, y: ey + 1, x1: x + 6, y1: ey + 2 }), Outcome::Done { .. }), "houses at {x}");
    }

    let pottery = w.trade.prices[13];
    let mut seen = Vec::new();
    for month in 1..=12 {
        let kingdom = w.ratings.kingdom;
        let wage = w.finance.kingdom_wages;
        let donated = w.finance.this_year.donated;
        next_month(&mut w);
        assert_eq!(w.time.month as usize, month % 12);
        let ev = &w.scenario_events.list;
        match month {
            1 => {
                let r = &ev[request];
                assert!(r.active && r.resource == 1 && r.amount == 8 && !r.pharaoh, "the request opens in February: {r:?}");
                seen.push("request");
            }
            2 => {
                assert_eq!(w.trade.prices[13], (pottery.0 + 10, pottery.1 + 10), "pottery dearer in March");
                seen.push("price");
            }
            3 => {
                // A gift waits for the player's answer, as in the original.
                use osiris_sim::scenario_events::{event, gift_choice};
                let offered = w.scenario_events.list.iter().position(|e| e.kind == event::GIFT && e.active && e.choices & gift_choice::ACCEPT != 0);
                let i = offered.expect("the gift is offered in April");
                assert_eq!(w.finance.this_year.donated, donated, "nothing given before it's accepted");
                assert!(w.answer_gift(i, gift_choice::ACCEPT));
                assert_eq!(w.finance.this_year.donated - donated, 500, "the gift in April");
                seen.push("gift");
            }
            4 => {
                assert_eq!(w.ratings.kingdom - kingdom, 5, "favour up in May");
                seen.push("favour");
            }
            6 => {
                assert_eq!(w.ratings.kingdom - kingdom, -3, "and down two months later");
                seen.push("follow-up");
            }
            5 => {
                assert_eq!(w.finance.kingdom_wages - wage, 2, "wages up in June");
                assert_eq!(w.ratings.kingdom, kingdom, "the follow-up waits");
                seen.push("wages");
            }
            8 => {
                // Before the army: with no soldiers or walls it razes the town in a
                // couple of months (a building falls on the 11th blow, as in the exe).
                assert!(w.population > 0, "people moved in");
                assert!(!w.invasions.armies.is_empty() || w.figures.iter().any(|f| w.is_invader(f)), "the army arrives in September");
                seen.push("invasion");
            }
            _ => {}
        }
    }
    assert_eq!(seen.len(), 7, "{seen:?}");
}

/// One event of every type the planning window offers, one a month over two years,
/// each fires in the month and year it was given once the map is played.
#[test]
fn every_event_type_fires_on_its_date() {
    let Some(data) = test_data() else { return };
    let mut e = editor(&data, &std::env::temp_dir().join("osiris-playtest-types"));
    e.new_map(1);
    let entry = first_allowed(&e, Point::Entry, (0..10).map(|x| (x, 42)));
    place(&mut e, Point::Entry, entry);
    let invasion = first_allowed(&e, Point::Invasion(0), (0..84).flat_map(|y| (0..84).map(move |x| (x, y))));
    place(&mut e, Point::Invasion(0), invasion);
    assert!(e.scenario.events.is_empty(), "Default.map has none");
    e.add_event();
    let kinds = super::events::list(&e.scenario, 0, EvField::Kind).unwrap().1;
    e.delete_event(0);
    assert_eq!(kinds.len(), 26);
    let mut when = Vec::new();
    for (n, &k) in kinds.iter().enumerate() {
        let i = e.add_event().unwrap();
        // One a month from February (a January of the first year has no monthly
        // pass before it).
        let (year, month) = (((n + 1) / 12) as i32, (n + 1) % 12);
        pick(&mut e.scenario, i, EvField::Kind, k);
        set_number(&mut e.scenario, i, EvField::Year(true), year);
        set_number(&mut e.scenario, i, EvField::Year(false), year);
        pick(&mut e.scenario, i, EvField::Month, month);
        when.push(year * 12 + month as i32);
    }
    let bytes = e.to_bytes().unwrap();
    let scenario = Scenario::from_chunks(&osiris_formats::ChunkFile::parse(&bytes, osiris_formats::Layout::Map).unwrap()).unwrap();
    assert_eq!(scenario.events.iter().map(|e| e.kind as usize).collect::<Vec<_>>(), kinds);
    let model = Model::parse(&String::from_utf8_lossy(&std::fs::read(data.join("Pharaoh_Model_Normal.txt")).unwrap())).unwrap();
    let mut w = World::new(&scenario, e.defs.clone(), Arc::new(Balance::from_model(&model)));
    w.begin_at(osiris_sim::difficulty::NORMAL);
    w.start(&scenario);
    let mut fired = vec![None; kinds.len()];
    for _ in 0..28 {
        next_month(&mut w);
        let now = w.month_count();
        for (i, ev) in w.scenario_events.list.iter().enumerate() {
            if ev.trigger != osiris_sim::scenario_events::trigger::ONCE && fired[i].is_none() {
                fired[i] = Some(now);
            }
        }
    }
    for (i, &k) in kinds.iter().enumerate() {
        assert_eq!(fired[i], Some(when[i]), "event {i}, type {k}");
    }
}
