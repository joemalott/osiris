//! The people of a festival. On the festival's day (FUN_004ea000) the original sends
//! people from the city's buildings to the festival square (FUN_00461950): scribes
//! from scribal schools and libraries, nobles from manors and estates, priests from
//! the temples, and jugglers and musicians from the performers' schools and the
//! venues. Each walks the roads to the middle of the square and then from one of its
//! tiles to another, ten stops in all, and is gone (figure type 91, action code at
//! 0x4aceb0, reached only through the figure action table at 0x5dc28c). They serve
//! no one on the way; the festival's lift to the city is the monthly sum in
//! `religion.rs`.

use crate::buildings::{BuildingId, kind, road_access};
use crate::figures::{Figure, FigureId, Step, Travel};
use crate::religion::FESTIVAL_SQUARE;
use crate::world::World;

/// The festival's people: the original's figure type 91.
pub const FESTIVAL_GUY: u16 = 91;

/// A festival walker's actions, numbered as the original's.
pub mod action {
    /// Just out of his building.
    pub const CREATED: u16 = 8;
    /// Choosing another tile of the square.
    pub const PICK: u16 = 9;
    pub const WALKING: u16 = 10;
    /// At a stop.
    pub const ARRIVED: u16 = 11;
}

/// Stops a festival walker makes on the square (the middle, then random tiles).
const STOPS: i32 = 10;

/// Each source's share of the festival's people, percent of ten times the festival's
/// size (0x5da544), and the building types of each (0x5da554). The pavilion is listed
/// twice, as in the original.
const SOURCES: [(i32, &[u16]); 4] = [
    (15, &[kind::SCRIBAL_SCHOOL, kind::LIBRARY]),
    (20, &[24, 25, 26, 27, 28, 29]),
    (30, &[60, 61, 62, 63, 64, 65, 66, 67, 68, 69]),
    (35, &[kind::BOOTH, kind::BANDSTAND, kind::PAVILION, SENET_HOUSE, kind::JUGGLER_SCHOOL, kind::CONSERVATORY, kind::PAVILION]),
];

const SENET_HOUSE: u16 = 32;

/// How many festival people `percent` (10 small, 20 lavish, 30 grand, 40 Bast's) asks
/// of each source, given how many of its buildings count: each source's share, less
/// what its buildings can't send, which passes to the next source (the last keeps
/// what it can't use).
fn quotas(percent: i32, available: [i32; 4]) -> [i32; 4] {
    let mut want = [0; 4];
    let mut send = [0; 4];
    for i in 0..4 {
        want[i] += SOURCES[i].0 * percent / 100;
        send[i] = available[i];
        if want[i] < send[i] {
            send[i] = want[i];
        } else if send[i] < want[i] && i != 3 {
            want[i + 1] = want[i] - send[i];
        }
    }
    send
}

/// A festival walker's looks: the figure type he is drawn and speaks as (priest 27,
/// juggler 15, musician 16, dancer 17, scribe 29, noble 40) and his walk in
/// `figures.toml`, by the building he came from (0x4ad0fb). A priest walks as his
/// temple's god's priests do; anyone whose building is gone, as a noble.
pub fn festival_look(world: &World, f: &Figure) -> (u16, &'static str) {
    let home = world.buildings.get(f.home).map_or(0, |b| b.kind);
    match home {
        kind::BANDSTAND | kind::CONSERVATORY => (16, "musician_walk"),
        kind::BOOTH | SENET_HOUSE | kind::JUGGLER_SCHOOL => (15, "juggler_walk"),
        kind::PAVILION | kind::DANCE_SCHOOL => (17, "dancer_walk"),
        kind::SCRIBAL_SCHOOL | kind::LIBRARY => (29, "scribe_walk"),
        60..=69 | 151 | 156 => match (home - 60) % 5 {
            0 => (27, "priest_osiris_walk"),
            1 => (27, "priest_ra_walk"),
            3 => (27, "priest_seth_walk"),
            4 => (27, "priest_bast_walk"),
            _ => (27, "priest_ptah_walk"),
        },
        _ => (40, "noble_walk"),
    }
}

impl World {
    /// The festival square's top-left tile, if the city has one.
    fn festival_square(&self) -> Option<(i32, i32)> {
        self.buildings.iter().find(|b| b.kind == FESTIVAL_SQUARE).map(|b| (b.x, b.y))
    }

    /// Whether building `b` is at work for a festival: a house with people in it, or
    /// anything else with workers.
    fn festival_ready(b: &crate::buildings::Building) -> bool {
        match &b.house {
            Some(h) => h.population > 0,
            None => b.workers > 0,
        }
    }

    /// The festival's people set out (FUN_00461950). `percent` is ten times the size.
    /// The buildings that count toward a source are those the census counts as
    /// working, which leaves out the booths, bandstands, pavilions and senet houses
    /// (it counts them elsewhere); but any working building of a source's types sends
    /// its people while the source's quota lasts, taken in building order.
    pub(crate) fn send_festival_people(&mut self, percent: i32) {
        let mut available = [0; 4];
        for (i, (_, kinds)) in SOURCES.iter().enumerate() {
            for &k in *kinds {
                if matches!(k, kind::BOOTH | kind::BANDSTAND | kind::PAVILION | SENET_HOUSE) {
                    continue;
                }
                available[i] += self.buildings.iter().filter(|b| b.kind == k && Self::festival_ready(b)).count() as i32;
            }
        }
        let mut left = quotas(percent, available);
        let mut senders: Vec<BuildingId> = Vec::new();
        for b in self.buildings.iter() {
            if !Self::festival_ready(b) {
                continue;
            }
            if left.iter().all(|&n| n == 0) {
                break;
            }
            for (i, (_, kinds)) in SOURCES.iter().enumerate() {
                if left[i] == 0 {
                    continue;
                }
                for &k in *kinds {
                    if b.kind == k {
                        left[i] -= 1;
                        senders.push(b.id);
                    }
                    if left[i] == 0 {
                        break;
                    }
                }
            }
        }
        for id in senders {
            self.send_festival_walker(id);
        }
    }

    /// One of the festival's people leaves building `id` by its road (FUN_00461b20),
    /// and becomes the building's walker for the while.
    fn send_festival_walker(&mut self, id: BuildingId) {
        let Some(b) = self.buildings.get(id) else { return };
        let road = if b.is_house() { road_access(&self.map, b.x, b.y, b.size) } else { b.road };
        let Some((x, y)) = road else { return };
        let fid = self.figures.spawn(FESTIVAL_GUY, x, y, Travel::Roads);
        if let Some(f) = self.figures.get_mut(fid) {
            f.home = id;
            f.action = action::CREATED;
        }
        if let Some(b) = self.buildings.get_mut(id) {
            b.walkers[0] = fid;
        }
    }

    /// A festival walker's day: by road to the middle of the square, then over the
    /// paving from tile to tile until his tenth stop, where he leaves the scene. He
    /// goes at once if the square is gone or he can't get there, and at his next stop
    /// if his way is blocked.
    pub(crate) fn update_festival_walker(&mut self, fid: FigureId) {
        let square = self.festival_square();
        let Some(f) = self.figures.get_mut(fid) else { return };
        let act = f.action;
        if act != action::WALKING {
            // His steps keep time even while he stands.
            f.anim_tick += 1;
        }
        let here = (f.x, f.y);
        match act {
            action::CREATED => {
                let map = &self.map;
                let f = self.figures.get_mut(fid).expect("present");
                let Some((sx, sy)) = square else {
                    f.dead = true;
                    return;
                };
                f.counter = STOPS;
                f.travel = Travel::Roads;
                f.go_to(map, (sx + 2, sy + 2));
                f.action = action::WALKING;
            }
            action::PICK => {
                let Some((sx, sy)) = square else {
                    self.figures.get_mut(fid).expect("present").dead = true;
                    return;
                };
                let spot = loop {
                    self.rng.next();
                    let dx = self.rng.short() * 5 >> 15;
                    self.rng.next();
                    let dy = self.rng.short() * 5 >> 15;
                    if (sx + dx, sy + dy) != here {
                        break (sx + dx, sy + dy);
                    }
                };
                let map = &self.map;
                let f = self.figures.get_mut(fid).expect("present");
                f.travel = Travel::Land;
                f.go_to(map, spot);
                f.action = action::WALKING;
            }
            action::WALKING => {
                let map = &self.map;
                let f = self.figures.get_mut(fid).expect("present");
                match f.walk(map) {
                    Step::Moving => {}
                    Step::Arrived => f.action = action::ARRIVED,
                    Step::Blocked => {
                        f.route.clear();
                        f.counter = 0;
                        f.action = action::ARRIVED;
                    }
                    Step::Lost => f.dead = true,
                }
            }
            action::ARRIVED => {
                let f = self.figures.get_mut(fid).expect("present");
                f.counter -= 1;
                if f.counter <= 0 {
                    f.dead = true;
                } else {
                    f.action = action::PICK;
                }
            }
            _ => self.figures.get_mut(fid).expect("present").action = action::PICK,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotas_pass_what_a_source_lacks_to_the_next() {
        // A small festival asks 1, 2, 3 and 3; a grand one 4, 6, 9 and 10.
        assert_eq!(quotas(10, [9, 9, 9, 9]), [1, 2, 3, 3]);
        assert_eq!(quotas(30, [9, 9, 9, 99]), [4, 6, 9, 10]);
        // No schools or libraries: their 3 go to the manors, the 6 of those the one
        // manor can't send to the temples, and the 3 the temples can't to the rest.
        assert_eq!(quotas(20, [0, 1, 9, 9]), [0, 1, 9, 9]);
        assert_eq!(quotas(20, [0, 1, 9, 99]), [0, 1, 9, 10]);
        // The performers' schools keep what they can't send.
        assert_eq!(quotas(40, [0, 0, 0, 2]), [0, 0, 0, 2]);
    }

    /// A clear stretch of the Sandbox map, as religion's tests use.
    fn sandbox() -> Option<(World, i32, i32)> {
        let data = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../PharaohData");
        if !data.join("Maps/Sandbox.map").is_file() {
            return None;
        }
        let library = osiris_formats::ImageLibrary::open(&data.join("Data")).ok()?;
        let scenario = osiris_formats::Scenario::load_map(&data.join("Maps/Sandbox.map")).ok()?;
        let defs = std::sync::Arc::new(crate::defs::Defs::load(&library).ok()?);
        let model = osiris_formats::Model::parse(&String::from_utf8_lossy(&std::fs::read(data.join("Pharaoh_Model_Normal.txt")).ok()?)).ok()?;
        let balance = std::sync::Arc::new(crate::balance::Balance::from_model(&model));
        let mut world = World::new(&scenario, defs, balance);
        world.start(&scenario);
        world.scenario_allowed = None;
        world.rules.fire = false;
        world.rules.collapse = false;
        let (w, h) = (world.map.width, world.map.height);
        let (x, y) = (0..h - 12).flat_map(|y| (0..w - 12).map(move |x| (x, y))).find(|&(x, y)| world.map.area_clear_of(x, y, 12, crate::map::mask::NOT_CLEAR))?;
        Some((world, x, y))
    }

    #[test]
    fn a_festivals_people_walk_the_square_ten_stops_and_go() {
        let Some((mut world, x, y)) = sandbox() else { return };
        use crate::world::Command;
        world.apply(&Command::Road { start: (x, y + 2), end: (x + 11, y + 2) });
        world.apply(&Command::Road { start: (x + 2, y), end: (x + 2, y + 11) });
        world.create_building(FESTIVAL_SQUARE, x, y);
        let temple = world.create_building(61, x + 6, y + 3);
        let school = world.create_building(kind::JUGGLER_SCHOOL, x + 3, y + 7);
        // A booth is sent from while the performers' share lasts, but isn't counted.
        let booth = world.create_building(kind::BOOTH, x + 3, y + 5);
        for id in [temple, school, booth] {
            world.refresh_road_access(id);
            world.buildings.get_mut(id).unwrap().workers = 5;
        }
        // A small festival asks 1, 2, 3 and 3; with no schools, libraries or manors the
        // temples may send 6 and the performers 3 + 5, but there is one temple and one
        // school to count.
        world.festival_now(crate::religion::RA, crate::religion::festival::SMALL);
        let walkers: Vec<(FigureId, u32)> = world.figures.iter().filter(|f| f.kind == FESTIVAL_GUY).map(|f| (f.id, f.home)).collect();
        assert_eq!(walkers.len(), 2);
        assert!(walkers.iter().any(|&(_, h)| h == temple));
        let looks: Vec<(u16, &str)> = walkers.iter().map(|&(id, _)| festival_look(&world, world.figures.get(id).unwrap())).collect();
        assert!(looks.contains(&(27, "priest_ra_walk")));
        assert_eq!(world.buildings.get(temple).unwrap().walkers[0], walkers.iter().find(|w| w.1 == temple).unwrap().0);
        let mut stops = 0;
        let mut on_square = true;
        for _ in 0..3000 {
            let before: Vec<u16> = walkers.iter().map(|&(id, _)| world.figures.get(id).map_or(0, |f| f.action)).collect();
            world.tick();
            for (i, &(id, _)) in walkers.iter().enumerate() {
                if let Some(f) = world.figures.get(id).filter(|f| f.kind == FESTIVAL_GUY)
                    && f.action == action::ARRIVED
                    && before[i] != action::ARRIVED
                {
                    stops += 1;
                    on_square &= (x..x + 5).contains(&f.x) && (y..y + 5).contains(&f.y);
                }
            }
            if walkers.iter().all(|&(id, _)| world.figures.get(id).is_none_or(|f| f.kind != FESTIVAL_GUY)) {
                break;
            }
        }
        assert_eq!(stops, 2 * STOPS);
        assert!(on_square);
        assert!(!world.figures.iter().any(|f| f.kind == FESTIVAL_GUY));
    }
}
