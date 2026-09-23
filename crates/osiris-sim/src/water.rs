//! The river: buildings on its shore, where boats may sail, and ferries.
//!
//! Wharves, docks, the shipwright and ferry landings are built half on the water. Their
//! footprint's front row stands in the river, and the row of tiles in front of that must
//! be water too; the rest of the footprint is dry land. The first side that fits, tried
//! north, east, south, west, is the way the building faces, and its image is the base
//! image plus that side (each has one image per facing). Boats moor at two tiles just off
//! the front: one tile out for fishing wharves and ferries, two for docks and the
//! shipwright.
//!
//! Boats sail only on river water that has water all round it, corners included, so they
//! keep a tile off the banks; flooded floodplain never counts. They start and leave at
//! the scenario's river entry and exit points. A building has open water when a tile
//! beside it can be reached by boat from the river entry.
//!
//! Two staffed ferry landings within four tiles of each other along either axis are
//! linked by a boat path between their mooring tiles. The path's water carries people
//! on foot as if it were road, and each landing keeps a ferry boat plying the crossing.

use crate::buildings::{Building, BuildingId};
use crate::figures::{FigureId, Step, Travel};
use crate::map::{Map, NEIGHBOURS, terrain};
use crate::world::World;
use std::collections::VecDeque;

pub const SHIPWRIGHT: u16 = crate::buildings::kind::SHIPWRIGHT;
pub const DOCK: u16 = 75;
pub const FISHING_WHARF: u16 = 76;
pub const FERRY: u16 = 136;
pub const TRANSPORT_WHARF: u16 = 181;
pub const WARSHIP_WHARF: u16 = 182;

pub const FERRY_BOAT: u16 = 76;

/// Tiles either axis may differ by for two ferry landings to be linked.
const FERRY_REACH: i32 = 4;
/// Ticks a ferry boat waits at a landing.
const FERRY_WAIT: i32 = 50;
const FERRY_START_WAIT: i32 = 20;

mod ferry_action {
    pub const WAITING: u16 = 0;
    pub const CROSSING: u16 = 1;
    pub const LANDED: u16 = 2;
    pub const RETURNING: u16 = 3;
}

/// River state that isn't in the terrain: the entry and exit points boats use, the
/// fishing grounds, and the water images covered by buildings.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Water {
    pub river_entry: Option<(i32, i32)>,
    pub river_exit: Option<(i32, i32)>,
    /// Where fish can be caught.
    pub fishing_points: Vec<(i32, i32)>,
    /// The water tiles under each building on the river.
    pub covered: Vec<Covered>,
}

/// Water tiles a building stands on, with the images they had, to restore when it goes.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Covered {
    pub building: BuildingId,
    pub tiles: Vec<(i32, i32, u32)>,
}

/// A test on a tile of the placement window: (row, column, window size).
type WindowTest = fn(i32, i32, i32) -> bool;

impl Water {
    /// The scenario's river points and fishing grounds. A scenario without fishing
    /// points gets one per 500 tiles of deep river water (at most eight), spread over
    /// that water.
    pub fn from_scenario(s: &osiris_formats::Scenario, map: &Map) -> Self {
        let i = &s.info;
        let valid = |p: &osiris_formats::scenario::TilePoint| (p.x > 0 && map.contains(p.x, p.y)).then_some((p.x, p.y));
        let mut fishing_points: Vec<(i32, i32)> =
            i.fishing_points.iter().filter_map(valid).filter(|&(x, y)| map.terrain_is(x, y, terrain::WATER)).collect();
        if fishing_points.is_empty() {
            let deep: Vec<(i32, i32)> = (0..map.height)
                .flat_map(|y| (0..map.width).map(move |x| (x, y)))
                .filter(|&(x, y)| map.terrain_is(x, y, terrain::DEEPWATER) && !map.terrain_is(x, y, terrain::FLOODPLAIN))
                .collect();
            let n = (deep.len() / 500).clamp(1, 8);
            if !deep.is_empty() {
                for k in 0..n {
                    let (x, y) = deep[(k * deep.len() / n + deep.len() / (2 * n)).min(deep.len() - 1)];
                    fishing_points.push((x, y));
                }
            }
        }
        Self {
            river_entry: valid(&i.river_entry_point),
            river_exit: valid(&i.river_exit_point).or(valid(&i.river_entry_point)),
            fishing_points,
            covered: Vec::new(),
        }
    }
}

/// Buildings that stand on the shore.
pub fn is_shore_building(k: u16) -> bool {
    matches!(k, SHIPWRIGHT | DOCK | FISHING_WHARF | FERRY | TRANSPORT_WHARF | WARSHIP_WHARF)
}

/// How far off the front boats moor.
fn mooring_offset(k: u16) -> i32 {
    if matches!(k, FISHING_WHARF | FERRY) { 1 } else { 2 }
}

/// River water for shore placement: flooded floodplain doesn't count.
fn is_river(map: &Map, x: i32, y: i32) -> bool {
    let t = map.terrain.at_or(x, y, 0);
    t & terrain::WATER != 0 && t & terrain::FLOODPLAIN == 0
}

/// The side a `size` footprint at `(x, y)` would face the river from (0 north, 1 east,
/// 2 south, 3 west), if any. Looking at the footprint with a one-tile border, the facing
/// side's border and the footprint's first line along it must be water right across,
/// and the rest of the footprint must be dry.
pub fn shore_orientation(map: &Map, x: i32, y: i32, size: i32) -> Option<u8> {
    let n = size + 2;
    let wet = |r: i32, c: i32| is_river(map, x - 1 + c, y - 1 + r);
    let inner = |i: i32| i > 0 && i < n - 1;
    // For each side: is (row, column) in the wet band, and is it footprint beyond it?
    let sides: [(WindowTest, WindowTest); 4] = [
        (|r, _, _| r <= 1, |r, _, n| r > 1 && r < n - 1),
        (|_, c, n| c >= n - 2, |_, c, n| c > 0 && c < n - 2),
        (|r, _, n| r >= n - 2, |r, _, n| r > 0 && r < n - 2),
        (|_, c, _| c <= 1, |_, c, n| c > 1 && c < n - 1),
    ];
    for (o, (band, dry)) in sides.iter().enumerate() {
        let fits = (0..n).all(|r| {
            (0..n).all(|c| {
                let w = wet(r, c);
                let along = if o % 2 == 0 { inner(c) } else { inner(r) };
                !(band(r, c, n) && !w) && !(dry(r, c, n) && along && w)
            })
        });
        if fits {
            return Some(o as u8);
        }
    }
    None
}

/// The two tiles boats moor at off a shore building's front.
pub fn mooring_tiles(b: &Building) -> [(i32, i32); 2] {
    let (x, y, s, off) = (b.x, b.y, b.size, mooring_offset(b.kind));
    match b.orientation {
        0 => [(x + 1, y - off), (x, y - off)],
        1 => [(x + s + off - 1, y), (x + s + off - 1, y + 1)],
        2 => [(x, y + s + off - 1), (x + 1, y + s + off - 1)],
        _ => [(x - off, y), (x - off, y + 1)],
    }
}

fn water_at(map: &Map, x: i32, y: i32) -> bool {
    !map.contains(x, y) || map.terrain_is(x, y, terrain::WATER)
}

/// Whether a boat may sail on `(x, y)`: river water, not flooded floodplain nor under a
/// building, with water on all eight sides (the map's edge counts as water).
pub fn navigable(map: &Map, x: i32, y: i32) -> bool {
    let t = map.terrain.at_or(x, y, 0);
    map.contains(x, y)
        && t & terrain::WATER != 0
        && t & (terrain::FLOODPLAIN | terrain::BUILDING) == 0
        && NEIGHBOURS.iter().all(|&(dx, dy)| water_at(map, x + dx, y + dy))
}

/// Tiles a boat can reach from `from`, by breadth-first search over navigable water.
pub fn reachable(map: &Map, from: (i32, i32)) -> Vec<bool> {
    let w = map.width;
    let mut seen = vec![false; (map.width * map.height).max(0) as usize];
    if !map.contains(from.0, from.1) {
        return seen;
    }
    seen[(from.1 * w + from.0) as usize] = true;
    let mut queue = VecDeque::from([from]);
    while let Some((x, y)) = queue.pop_front() {
        for d in (0..8).step_by(2) {
            let (nx, ny) = (x + NEIGHBOURS[d].0, y + NEIGHBOURS[d].1);
            if map.contains(nx, ny) && !seen[(ny * w + nx) as usize] && navigable(map, nx, ny) {
                seen[(ny * w + nx) as usize] = true;
                queue.push_back((nx, ny));
            }
        }
    }
    seen
}

/// An orthogonal boat path from `from` to `to` (both ends may be off open water), as
/// the tiles crossed, ends included.
fn water_path(map: &Map, from: (i32, i32), to: (i32, i32)) -> Option<Vec<(i32, i32)>> {
    let w = map.width;
    if !map.contains(from.0, from.1) || !map.contains(to.0, to.1) {
        return None;
    }
    let mut came = vec![u8::MAX; (map.width * map.height) as usize];
    came[(from.1 * w + from.0) as usize] = 8;
    let mut queue = VecDeque::from([from]);
    while let Some((x, y)) = queue.pop_front() {
        if (x, y) == to {
            break;
        }
        for d in (0..8).step_by(2) {
            let (nx, ny) = (x + NEIGHBOURS[d].0, y + NEIGHBOURS[d].1);
            if !map.contains(nx, ny) || came[(ny * w + nx) as usize] != u8::MAX {
                continue;
            }
            if (nx, ny) == to || navigable(map, nx, ny) {
                came[(ny * w + nx) as usize] = d as u8;
                queue.push_back((nx, ny));
            }
        }
    }
    if came[(to.1 * w + to.0) as usize] == u8::MAX {
        return None;
    }
    let mut path = vec![to];
    let (mut x, mut y) = to;
    while (x, y) != from {
        let (dx, dy) = NEIGHBOURS[came[(y * w + x) as usize] as usize];
        x -= dx;
        y -= dy;
        path.push((x, y));
    }
    path.reverse();
    Some(path)
}

impl World {
    /// Checks the shore rule for building `k` at `(x, y)`: it must face the river, its
    /// wet tiles free and its dry tiles clear.
    pub(crate) fn can_place_on_shore(&self, k: u16, x: i32, y: i32) -> Result<(), &'static str> {
        let size = self.size_of(k);
        if !self.map.contains(x, y) || !self.map.contains(x + size - 1, y + size - 1) {
            return Err("Outside the map");
        }
        if shore_orientation(&self.map, x, y, size).is_none() {
            return Err("Must be built on the shore, facing the water");
        }
        for yy in y..y + size {
            for xx in x..x + size {
                let blocked = if is_river(&self.map, xx, yy) {
                    self.map.terrain_is(xx, yy, terrain::BUILDING)
                } else {
                    self.map.terrain_is(xx, yy, crate::map::mask::NOT_CLEAR)
                };
                if blocked {
                    return Err("Can't build there");
                }
                if self.figures.iter().any(|f| (f.x, f.y) == (xx, yy)) {
                    return Err("People are in the way");
                }
            }
        }
        Ok(())
    }

    /// Finishes placing a shore building: its facing, the image for it, and the water
    /// images it covers (kept to restore when it goes). A ferry landing is road.
    pub(crate) fn place_on_shore(&mut self, id: BuildingId) {
        let Some(b) = self.buildings.get(id) else { return };
        let (k, x, y, size) = (b.kind, b.x, b.y, b.size);
        let orientation = shore_orientation(&self.map, x, y, size).unwrap_or(0);
        let tiles = b.tiles().filter(|&(xx, yy)| is_river(&self.map, xx, yy)).map(|(xx, yy)| (xx, yy, self.map.images.at_or(xx, yy, 0))).collect();
        self.water.covered.push(Covered { building: id, tiles });
        let image = b.image + orientation as u32;
        if let Some(b) = self.buildings.get_mut(id) {
            b.orientation = orientation;
            b.image = image;
        }
        self.map.set_footprint(x, y, size, image);
        if k == FERRY {
            for yy in y..y + size {
                for xx in x..x + size {
                    self.map.terrain.update(xx, yy, |t| t | terrain::ROAD | terrain::FERRY_ROUTE);
                }
            }
            self.update_ferry_routes();
        }
    }

    /// A shore building has gone: its water tiles get their images back, and a ferry
    /// landing stops being road.
    pub(crate) fn remove_from_shore(&mut self, b: &Building) {
        if let Some(i) = self.water.covered.iter().position(|c| c.building == b.id) {
            for (x, y, image) in self.water.covered.swap_remove(i).tiles {
                self.map.set_single_image(x, y, image);
            }
        }
        if b.kind == FERRY {
            for (x, y) in b.tiles() {
                self.map.terrain.update(x, y, |t| t & !(terrain::ROAD | terrain::FERRY_ROUTE));
            }
            self.update_ferry_routes();
        }
    }

    /// The river entry snapped to the nearest open water, where boats appear.
    pub fn river_entry(&self) -> Option<(i32, i32)> {
        self.water.river_entry.and_then(|p| self.nearest_navigable(p))
    }

    pub fn river_exit(&self) -> Option<(i32, i32)> {
        self.water.river_exit.and_then(|p| self.nearest_navigable(p))
    }

    fn nearest_navigable(&self, (x, y): (i32, i32)) -> Option<(i32, i32)> {
        (0..=3).find_map(|r| {
            (y - r..=y + r).find_map(|yy| (x - r..=x + r).find(|&xx| navigable(&self.map, xx, yy)).map(|xx| (xx, yy)))
        })
    }

    /// Whether building `id` has a tile beside it that boats can reach from the river
    /// entry.
    pub fn has_open_water(&self, id: BuildingId) -> bool {
        let Some(b) = self.buildings.get(id) else { return false };
        let Some(entry) = self.river_entry() else { return false };
        let reach = reachable(&self.map, entry);
        let w = self.map.width;
        crate::buildings::ring(b.x, b.y, b.size).any(|(x, y)| self.map.contains(x, y) && reach[(y * w + x) as usize])
    }

    /// Where a boat of building `id` moors: the first mooring tile a boat at `from` can
    /// sail to.
    pub fn mooring_for(&self, id: BuildingId, from: (i32, i32)) -> Option<(i32, i32)> {
        let b = self.buildings.get(id)?;
        mooring_tiles(b).into_iter().find(|&t| t == from || crate::figures::find_route(&self.map, Travel::Water, from, t).is_some())
    }

    /// Relinks ferry landings: clears the old crossings and marks a boat path between
    /// the mooring tiles of every pair of staffed landings close enough together.
    pub(crate) fn update_ferry_routes(&mut self) {
        for y in 0..self.map.height {
            for x in 0..self.map.width {
                let t = self.map.terrain.at_or(x, y, 0);
                if t & terrain::FERRY_ROUTE != 0 && t & terrain::BUILDING == 0 {
                    self.map.terrain.set(x, y, t & !terrain::FERRY_ROUTE);
                }
            }
        }
        let ferries: Vec<Building> = self.buildings.iter().filter(|b| b.kind == FERRY && b.workers > 0).cloned().collect();
        let mut marked = Vec::new();
        for (i, a) in ferries.iter().enumerate() {
            for b in &ferries[i + 1..] {
                if (a.x - b.x).abs() > FERRY_REACH && (a.y - b.y).abs() > FERRY_REACH {
                    continue;
                }
                for from in mooring_tiles(a) {
                    for to in mooring_tiles(b) {
                        if let Some(path) = water_path(&self.map, from, to) {
                            marked.extend(path);
                        }
                    }
                }
            }
        }
        for (x, y) in marked {
            self.map.terrain.update(x, y, |t| t | terrain::FERRY_ROUTE);
        }
    }

    /// The landing linked to ferry `id` nearest to it, if any.
    fn ferry_partner(&self, id: BuildingId) -> Option<BuildingId> {
        let a = self.buildings.get(id)?;
        let from = mooring_tiles(a)[0];
        self.buildings
            .iter()
            .filter(|b| b.kind == FERRY && b.id != id && b.workers > 0)
            .filter(|b| (a.x - b.x).abs() <= FERRY_REACH || (a.y - b.y).abs() <= FERRY_REACH)
            .filter(|b| self.map.terrain_is(mooring_tiles(b)[0].0, mooring_tiles(b)[0].1, terrain::FERRY_ROUTE))
            .min_by_key(|b| ((b.x - a.x).pow(2) + (b.y - a.y).pow(2), b.id))
            .filter(|_| self.map.terrain_is(from.0, from.1, terrain::FERRY_ROUTE))
            .map(|b| b.id)
    }

    /// Daily: ferry crossings follow staffing, and each linked landing keeps a ferry
    /// boat.
    pub(crate) fn update_ferries(&mut self) {
        if self.buildings.iter().all(|b| b.kind != FERRY) {
            return;
        }
        self.update_ferry_routes();
        for id in self.buildings.ids() {
            let Some(b) = self.buildings.get(id) else { continue };
            if b.kind != FERRY || b.walkers[0] != 0 || b.workers <= 0 || self.ferry_partner(id).is_none() {
                continue;
            }
            let (x, y) = mooring_tiles(b)[0];
            let fid = self.figures.spawn(FERRY_BOAT, x, y, Travel::Water);
            if let Some(f) = self.figures.get_mut(fid) {
                f.home = id;
                f.action = ferry_action::WAITING;
                f.counter = FERRY_START_WAIT;
                f.direction = (b.orientation * 2 + 4) % 8;
            }
            self.buildings.get_mut(id).expect("present").walkers[0] = fid;
        }
    }

    /// A ferry boat waits at its landing, crosses to the linked one, rests there and
    /// comes back.
    pub(crate) fn update_ferry_boat(&mut self, fid: FigureId) {
        let Some(f) = self.figures.get(fid) else { return };
        let (act, home) = (f.action, f.home);
        if self.buildings.get(home).is_none_or(|b| b.kind != FERRY) {
            self.figures.get_mut(fid).expect("present").dead = true;
            return;
        }
        match act {
            ferry_action::WAITING | ferry_action::LANDED => {
                let f = self.figures.get_mut(fid).expect("present");
                f.counter -= 1;
                if f.counter > 0 {
                    return;
                }
                let to = if act == ferry_action::WAITING {
                    self.ferry_partner(home).and_then(|p| self.buildings.get(p)).map(|b| mooring_tiles(b)[0])
                } else {
                    self.buildings.get(home).map(|b| mooring_tiles(b)[0])
                };
                let map = &self.map;
                let f = self.figures.get_mut(fid).expect("present");
                match to {
                    Some(to) if f.go_to(map, to) => {
                        f.action = if act == ferry_action::WAITING { ferry_action::CROSSING } else { ferry_action::RETURNING };
                    }
                    _ => f.counter = FERRY_WAIT,
                }
            }
            _ => {
                let wait = FERRY_WAIT + self.rng.byte() % FERRY_WAIT;
                let map = &self.map;
                let f = self.figures.get_mut(fid).expect("present");
                match f.walk(map) {
                    Step::Moving => {}
                    Step::Arrived => {
                        f.action = if act == ferry_action::CROSSING { ferry_action::LANDED } else { ferry_action::WAITING };
                        f.counter = wait;
                    }
                    Step::Blocked | Step::Lost => {
                        f.route.clear();
                        f.action = ferry_action::LANDED;
                        f.counter = FERRY_WAIT;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::Grid;

    /// A map with water on rows `y0..y1`.
    fn river_map(w: i32, h: i32, y0: i32, y1: i32) -> Map {
        let mut m = Map {
            width: w,
            height: h,
            terrain: Grid::new(w, h),
            images: Grid::new(w, h),
            edges: Grid::new(w, h),
            bitfields: Grid::new(w, h),
            elevation: Grid::new(w, h),
            random: Grid::new(w, h),
            fertility: Grid::new(w, h),
            moisture: Grid::new(w, h),
            vegetation: Grid::new(w, h),
            building: Grid::new(w, h),
        };
        for y in y0..y1 {
            for x in 0..w {
                m.terrain.set(x, y, terrain::WATER);
            }
        }
        m
    }

    #[test]
    fn faces_the_river_on_the_side_it_touches() {
        // River on rows 0..5; land from row 5 down.
        let m = river_map(12, 12, 0, 5);
        // Front row (y = 4) in the water, the row above water too: faces north.
        assert_eq!(shore_orientation(&m, 3, 4, 2), Some(0));
        assert_eq!(shore_orientation(&m, 3, 4, 3), Some(0));
        // Entirely on land, or entirely in the water: no.
        assert_eq!(shore_orientation(&m, 3, 6, 2), None);
        assert_eq!(shore_orientation(&m, 3, 1, 2), None);
        // River below: faces south.
        let m = river_map(12, 12, 6, 12);
        assert_eq!(shore_orientation(&m, 3, 5, 2), Some(2));
    }

    #[test]
    fn boats_keep_off_the_bank() {
        let m = river_map(12, 12, 0, 5);
        assert!(navigable(&m, 5, 2));
        assert!(navigable(&m, 5, 3));
        assert!(!navigable(&m, 5, 4));
        assert!(!navigable(&m, 5, 6));
        let r = crate::figures::find_route(&m, Travel::Water, (0, 1), (11, 3)).unwrap();
        assert!(!r.is_empty());
    }

    /// The Sandbox map with a small town on the south bank of its river: houses, a
    /// fishing wharf, a dock, a granary and a storage yard, and a shipwright and a ferry
    /// landing on the north bank opposite another.
    fn sandbox_town() -> Option<World> {
        let data = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../PharaohData");
        if !data.is_dir() {
            return None;
        }
        let library = osiris_formats::ImageLibrary::open(&data.join("Data")).expect("open image library");
        let scenario = osiris_formats::Scenario::load_map(&data.join("Maps/Sandbox.map")).expect("load map");
        let defs = std::sync::Arc::new(crate::defs::Defs::load(&library).expect("load defs"));
        let model_text = std::fs::read(data.join("Pharaoh_Model_Normal.txt")).expect("read model");
        let model = osiris_formats::Model::parse(&String::from_utf8_lossy(&model_text)).expect("parse model");
        let balance = std::sync::Arc::new(crate::balance::Balance::from_model(&model));
        let mut world = World::new(&scenario, defs, balance);
        world.start(&scenario);
        world.rules.fire = false;
        world.rules.collapse = false;
        world.rules.global_labor_pool = true;
        use crate::world::{Command, Outcome};
        let steps = [
            Command::Road { start: (150, 126), end: (176, 126) },
            Command::Road { start: (176, 126), end: (176, 128) },
            Command::Road { start: (176, 128), end: (190, 128) },
            Command::Road { start: (170, 126), end: (170, 131) },
            Command::Build { kind: crate::buildings::kind::VACANT_LOT, x: 160, y: 127, x1: 168, y1: 130 },
            Command::Build { kind: FISHING_WHARF, x: 165, y: 124, x1: 165, y1: 124 },
            Command::Build { kind: DOCK, x: 179, y: 125, x1: 179, y1: 125 },
            Command::Build { kind: crate::buildings::kind::GRANARY, x: 152, y: 127, x1: 152, y1: 127 },
            Command::Build { kind: crate::buildings::kind::STORAGE_YARD, x: 182, y: 132, x1: 182, y1: 132 },
            Command::Build { kind: FERRY, x: 170, y: 124, x1: 170, y1: 124 },
            Command::Road { start: (168, 110), end: (176, 110) },
            Command::Road { start: (176, 110), end: (176, 109) },
            Command::Road { start: (176, 109), end: (190, 109) },
            Command::Build { kind: SHIPWRIGHT, x: 180, y: 110, x1: 180, y1: 110 },
            Command::Build { kind: FERRY, x: 170, y: 111, x1: 170, y1: 111 },
        ];
        for cmd in &steps {
            assert!(matches!(world.apply(cmd), Outcome::Done { .. }), "{cmd:?}");
        }
        Some(world)
    }

    #[test]
    fn shore_buildings_face_the_river() {
        let Some(world) = sandbox_town() else { return };
        let facing = |x: i32, y: i32| world.buildings.get(world.map.building.at_or(x, y, 0)).map(|b| b.orientation);
        // South bank faces north, north bank south.
        assert_eq!(facing(165, 124), Some(0));
        assert_eq!(facing(179, 125), Some(0));
        assert_eq!(facing(180, 110), Some(2));
        assert_eq!(world.can_place_on_shore(FISHING_WHARF, 165, 128), Err("Must be built on the shore, facing the water"));
    }

    #[test]
    fn fish_reach_the_granary_and_ships_trade_at_the_dock() {
        let Some(mut world) = sandbox_town() else { return };
        let yard = world.map.building.at_or(182, 132, 0);
        world.add_stored(yard, 14, 400);
        let city = world.trade.cities.iter().position(|c| c.sea && c.sells[13]).expect("a sea city selling pottery");
        world.trade.cities[city].open = true;
        world.set_trade(13, crate::trade::status::IMPORT, 1000);
        for _ in 0..7500 {
            world.tick();
        }
        let granary = world.map.building.at_or(152, 127, 0);
        assert!(world.stored(granary, crate::economy::resource::FISH) > 0, "fish delivered");
        assert!(world.stored(yard, 13) > 0, "pottery unloaded from a ship");
        assert!(world.finance.this_year.imports > 0, "imports {:?}", world.finance.this_year);
        // The shipwright built the wharf's boat.
        let wharf = world.map.building.at_or(165, 124, 0);
        assert!(world.wharf_boat(wharf).is_some());
        // The staffed ferries link the banks for people on foot.
        let route = crate::figures::find_route(&world.map, Travel::Roads, (171, 126), (170, 110));
        assert!(route.is_some_and(|r| r.len() < 25), "ferry crossing");
    }
}
