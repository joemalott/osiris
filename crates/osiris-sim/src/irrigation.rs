//! Irrigation: water lifts raise the Nile into irrigation ditches, and farms near
//! watered ditches grow on richer soil.
//!
//! Once a day (tick 27) the original rebuilds the water. Every ditch dries. A water
//! lift has water when either tile in front of it is water. Ditches on the floodplain
//! row at the river's edge (and, while the flood is out, the rows at its edge) fill
//! straight from the Nile. Then each staffed lift with water pours it into the ditch
//! behind it, and this repeats until nothing changes, because a ditch touching a dry
//! lift gives that lift water too. A ditch network fills whole: each ditch tile it
//! reaches from another marks the land two tiles round it as irrigated, and a staffed
//! lift with water marks the land two tiles round itself. A fill that runs into a
//! flooded ditch stops there.
//!
//! A farm is irrigated when any marked tile lies in the 5x5 square centred on its
//! top-left tile. Irrigation adds 40 to a meadow farm's fertility and 20 to a
//! floodplain farm's, and an irrigated floodplain harvest keeps half the soil's
//! fertility instead of a fifth.
//!
//! The same daily pass redraws wells and water supplies in their finer look where
//! desirability is 30 or more, and marks the land two tiles round each well as in a
//! fountain's range. A water supply works only with groundwater in the 3x3 square
//! centred on its top-left tile, and below desirability 30 it sends its water
//! carriers as if half as well staffed.

use crate::buildings::{Building, BuildingId, kind};
use crate::map::{Map, NEIGHBOURS, mask, terrain};
use crate::world::{Outcome, World};
use std::collections::VecDeque;

pub const WATER_LIFT: u16 = 7;
pub const DITCH: u16 = kind::IRRIGATION_DITCH;

/// From a watered ditch image to the same ditch dry.
const DRY: u32 = 48;
/// A ditch under a dirt road, and under a paved one.
const UNDER_ROAD: u32 = 15;
const UNDER_PAVED_ROAD: u32 = 42;
/// From a ditch image to the same ditch on the floodplain.
const ON_FLOODPLAIN: u32 = 21;
/// A water lift's image on the floodplain's bank: dry, and with water.
const BANK_DRY: u32 = 4;
const BANK_WET: u32 = 8;
/// How far round a watered ditch or a pumping lift the land is irrigated.
const IRRIGATION_RANGE: i32 = 2;
/// How far round a well the land is in a fountain's range.
const WELL_RANGE: i32 = 2;
/// Wells and water supplies take their finer look from this desirability, and a water
/// supply below it works as if half as well staffed.
const FINE_DESIRABILITY: i32 = 30;
const IRRIGATED_MEADOW: i32 = 40;
const IRRIGATED_FLOODPLAIN: i32 = 20;
/// Percent of a floodplain farm's fertility its harvest leaves, irrigated or not.
pub const HARVEST_KEPT_IRRIGATED: i32 = 50;
pub const HARVEST_KEPT: i32 = 20;
/// Most tiles a ditch fill works through.
const FILL_LIMIT: usize = 0xcb10;
/// Longest ditch walk back from the end of a drag.
const DITCH_STEPS: usize = 400;

/// A 2x2 lift's tiles in the order the original tests them.
const LIFT_TILES: [(i32, i32); 4] = [(0, 0), (1, 0), (0, 1), (1, 1)];
/// For each facing (0 north, 1 east, 2 south, 3 west), whether each of `LIFT_TILES`
/// is in the front row, the one standing in the water.
const FRONT: [[bool; 4]; 4] = [[true, true, false, false], [false, true, false, true], [false, false, true, true], [true, false, true, false]];
/// For each facing, the tiles round the front row that must be water or floodplain:
/// beside it, off its corners, and in front of it.
const FLANKS: [[(i32, i32); 6]; 4] = [
    [(-1, 0), (2, 0), (-1, -1), (2, -1), (0, -1), (1, -1)],
    [(1, -1), (1, 2), (2, -1), (2, 2), (2, 0), (2, 1)],
    [(2, 1), (-1, 1), (-1, 2), (2, 2), (0, 2), (1, 2)],
    [(0, -1), (0, 2), (-1, -1), (-1, 2), (-1, 0), (-1, 1)],
];
/// For each facing, the two tiles in front of a lift: its intake. The opposite
/// facing's are its outlet, where it pours into a ditch.
pub const INTAKE: [[(i32, i32); 2]; 4] = [[(0, -1), (1, -1)], [(2, 0), (2, 1)], [(0, 2), (1, 2)], [(-1, 0), (-1, 1)]];
/// What a lift's back row may not stand on, besides water and floodplain.
const LIFT_BLOCKED: u32 = mask::NOT_CLEAR & !(terrain::WATER | terrain::FLOODPLAIN);

/// A road, or a road under the flood.
fn roadish(t: u32) -> bool {
    t & (terrain::ROAD | terrain::SUBMERGED_ROAD) != 0
}

/// A tile the original's flood system tracks without it being floodplain: land or
/// water touching the floodplain, bar water with nothing but open water and floodplain
/// round it.
pub fn floodplain_bank(map: &Map, x: i32, y: i32) -> bool {
    let t = map.terrain.at_or(x, y, 0);
    if !map.contains(x, y) || t & terrain::FLOODPLAIN != 0 {
        return false;
    }
    let at = |i: usize| map.terrain.at_or(x + NEIGHBOURS[i].0, y + NEIGHBOURS[i].1, 0);
    let floodplain = (0..8).filter(|&i| at(i) & terrain::FLOODPLAIN != 0).count();
    if floodplain == 0 {
        return false;
    }
    let open = (0..8).filter(|&i| at(i) & terrain::WATER != 0 && at(i) & (terrain::FLOODPLAIN | terrain::DIKE) == 0).count();
    !(t & terrain::WATER != 0 && floodplain + open == 8)
}

/// The facing a water lift at `(x, y)` would take, and how many of its tiles are
/// blocked, if any facing fits. Its front row must stand in the river, or right
/// across on the floodplain's bank; its back row on dry land off the floodplain; and
/// the tiles round the front row must be water, floodplain or bank. Facings are tried
/// north, east, south, west. Once a front tile has been taken as river (or as bank),
/// later facings must take theirs the same way, as in the original.
pub fn lift_site(map: &Map, x: i32, y: i32) -> Option<(u8, i32)> {
    if !map.contains(x, y) || !map.contains(x + 1, y + 1) {
        return None;
    }
    // 1: the front is on the bank; 2: in the river.
    let mut front_kind = 0u8;
    for (side, front) in FRONT.iter().enumerate() {
        let mut problems = 0;
        let mut fits = true;
        for (i, &(dx, dy)) in LIFT_TILES.iter().enumerate() {
            let (tx, ty) = (x + dx, y + dy);
            let t = map.terrain.at_or(tx, ty, 0);
            let bank = floodplain_bank(map, tx, ty);
            if front[i] {
                let as_bank = bank && front_kind != 2 && {
                    front_kind = 1;
                    true
                };
                if t & terrain::WATER == 0 || front_kind == 1 {
                    if !as_bank {
                        fits = false;
                        break;
                    }
                } else {
                    front_kind = 2;
                }
                problems += (t & terrain::ROAD != 0) as i32 + (t & (terrain::TREE | terrain::BUILDING) != 0) as i32;
            } else {
                if t & (terrain::WATER | terrain::FLOODPLAIN) != 0 && !bank {
                    fits = false;
                    break;
                }
                problems += (t & (LIFT_BLOCKED | terrain::WATER) != 0) as i32;
            }
        }
        let wet = |&(dx, dy): &(i32, i32)| map.terrain_is(x + dx, y + dy, terrain::WATER | terrain::FLOODPLAIN) || floodplain_bank(map, x + dx, y + dy);
        if fits && FLANKS[side].iter().all(wet) {
            return Some((side as u8, problems));
        }
    }
    None
}

/// Whether a road tile can carry a ditch across it: the road runs straight along one
/// axis, not a plaza, and no ditch already runs along that axis beside it.
fn road_takes_ditch(map: &Map, x: i32, y: i32, ditch: &impl Fn(i32, i32) -> bool) -> bool {
    if map.bitfields.at_or(x, y, 0) & 0x80 != 0 {
        return false;
    }
    let road = |x: i32, y: i32| map.terrain_is(x, y, terrain::ROAD);
    let north_south = road(x, y - 1) || road(x, y + 1);
    let east_west = road(x - 1, y) || road(x + 1, y);
    match (north_south, east_west) {
        (true, false) => !ditch(x, y - 1) && !ditch(x, y + 1),
        (false, true) => !ditch(x - 1, y) && !ditch(x + 1, y),
        _ => false,
    }
}

/// Ditch images a road may be laid across, wet or dry: for each image the tiles along
/// the ditch's line, where no road may already run (east and west, or north and
/// south). Other ditch images (corners, junctions) take no road.
fn ditch_line(offset: u32) -> Option<[(i32, i32); 2]> {
    const EAST_WEST: [u32; 15] = [0, 7, 9, 15, 17, 19, 21, 28, 30, 36, 38, 40, 42, 44, 46];
    const NORTH_SOUTH: [u32; 15] = [1, 6, 8, 16, 18, 20, 22, 27, 29, 37, 39, 41, 43, 45, 47];
    let offset = offset % DRY;
    if EAST_WEST.contains(&offset) {
        Some([(-1, 0), (1, 0)])
    } else if NORTH_SOUTH.contains(&offset) {
        Some([(0, -1), (0, 1)])
    } else {
        None
    }
}

/// Whether a ditch may be dug through `(x, y)`: clear land, meadow, dry floodplain, an
/// existing ditch, or a road it can cross.
fn ditch_passable(map: &Map, x: i32, y: i32) -> bool {
    let t = map.terrain.at_or(x, y, 0);
    let blocked = mask::NOT_CLEAR & !(terrain::ROAD | terrain::CANAL | terrain::FLOODPLAIN);
    map.contains(x, y)
        && t & (blocked | terrain::WATER) == 0
        && (t & terrain::ROAD == 0 || road_takes_ditch(map, x, y, &|x, y| map.terrain_is(x, y, terrain::CANAL)))
}

impl World {
    /// Placement rule for a water lift at `(x, y)`.
    pub(crate) fn can_place_lift(&self, x: i32, y: i32) -> Result<(), &'static str> {
        let Some((_, problems)) = lift_site(&self.map, x, y) else {
            return Err("Must be built at the water's edge, or on the floodplain's bank");
        };
        if problems > 0 || LIFT_TILES.iter().any(|&(dx, dy)| self.map.building.at_or(x + dx, y + dy, 0) != 0) {
            return Err("Can't build there");
        }
        if LIFT_TILES.iter().any(|&(dx, dy)| self.figures.iter().any(|f| (f.x, f.y) == (x + dx, y + dy) && !self.map.terrain_is(f.x, f.y, terrain::WATER))) {
            return Err("People are in the way");
        }
        Ok(())
    }

    /// Finishes placing a water lift: its facing, its image, and the river images it
    /// covers (kept to restore when it goes).
    pub(crate) fn place_lift(&mut self, id: BuildingId) {
        let Some(b) = self.buildings.get(id) else { return };
        let (x, y) = (b.x, b.y);
        let facing = lift_site(&self.map, x, y).map_or(0, |(s, _)| s);
        let tiles = b.tiles().filter(|&(xx, yy)| self.map.terrain_is(xx, yy, terrain::WATER)).map(|(xx, yy)| (xx, yy, self.map.images.at_or(xx, yy, 0))).collect();
        self.water.covered.push(crate::water::Covered { building: id, tiles });
        if let Some(b) = self.buildings.get_mut(id) {
            b.orientation = facing;
        }
        self.refresh_lift_image(id);
        self.ditch_images_in(x - 1, y - 1, x + 2, y + 2);
    }

    /// A lift's image: one per facing, with a bank lift drawn dry or with water.
    pub fn lift_image(&self, b: &Building) -> u32 {
        let bank = b.tiles().any(|(x, y)| floodplain_bank(&self.map, x, y));
        let look = match (bank, b.water != 0) {
            (false, _) => 0,
            (true, false) => BANK_DRY,
            (true, true) => BANK_WET,
        };
        self.defs.terrain.water_lift + b.orientation as u32 % 4 + look
    }

    fn refresh_lift_image(&mut self, id: BuildingId) {
        let Some(b) = self.buildings.get(id) else { return };
        let image = self.lift_image(b);
        if b.image != image || self.map.images.at_or(b.x, b.y, 0) != image {
            self.set_building_image(id, image);
        }
    }

    /// Whether a lift is pumping: staffed, with water.
    pub fn lift_pumping(&self, b: &Building) -> bool {
        b.kind == WATER_LIFT && b.workers > 0 && b.water != 0
    }

    /// Whether a ditch may be dug through `(x, y)`, entering it north-south
    /// (`Some(true)`), east-west (`Some(false)`) or as an end of the drag (`None`). Over
    /// open ground as [`ditch_passable`] says; of buildings, through a roadblock, or
    /// straight through a water lift along the way it faces.
    fn ditch_through(&self, x: i32, y: i32, vertical: Option<bool>) -> bool {
        if !self.map.terrain_is(x, y, terrain::BUILDING) {
            return ditch_passable(&self.map, x, y);
        }
        match self.buildings.get(self.map.building.at_or(x, y, 0)) {
            Some(b) if b.kind == crate::defenses::ROADBLOCK => true,
            Some(b) if b.kind == WATER_LIFT => vertical == Some(b.orientation % 2 == 0),
            _ => false,
        }
    }

    /// Whether the tile at `(x, y)` is a roadblock.
    fn roadblock_at(&self, x: i32, y: i32) -> bool {
        self.buildings.get(self.map.building.at_or(x, y, 0)).is_some_and(|b| b.kind == crate::defenses::ROADBLOCK)
    }

    /// The tiles of a ditch dragged from `start` to `end`, walking back from the end
    /// toward the start as the original does, or `None` if no ditch can be dug there.
    pub fn ditch_path(&self, start: (i32, i32), end: (i32, i32)) -> Option<Vec<(i32, i32)>> {
        let map = &self.map;
        let (w, h) = (map.width, map.height);
        if !self.ditch_through(start.0, start.1, None) || !self.ditch_through(end.0, end.1, None) {
            return None;
        }
        let mut dist = vec![0i32; (w * h) as usize];
        dist[(start.1 * w + start.0) as usize] = 1;
        let mut queue = VecDeque::from([start]);
        while let Some((x, y)) = queue.pop_front() {
            let d = dist[(y * w + x) as usize];
            for i in (0..8).step_by(2) {
                let (nx, ny) = (x + NEIGHBOURS[i].0, y + NEIGHBOURS[i].1);
                if !map.contains(nx, ny) || dist[(ny * w + nx) as usize] != 0 || !self.ditch_through(nx, ny, Some(i % 4 == 0)) {
                    continue;
                }
                dist[(ny * w + nx) as usize] = d + 1;
                queue.push_back((nx, ny));
            }
        }
        let at = |(x, y): (i32, i32)| if map.contains(x, y) { dist[(y * w + x) as usize] } else { 0 };
        let mut cur = end;
        let mut path: Vec<(i32, i32)> = Vec::new();
        for _ in 0..DITCH_STEPS {
            let d = at(cur);
            if d <= 0 {
                return None;
            }
            // A road is crossed only straight over, with the ditch dug so far.
            let ditch = |x: i32, y: i32| map.terrain_is(x, y, terrain::CANAL) || path.contains(&(x, y));
            if map.terrain_is(cur.0, cur.1, terrain::ROAD) && !self.roadblock_at(cur.0, cur.1) && !road_takes_ditch(map, cur.0, cur.1, &ditch) {
                return None;
            }
            path.push(cur);
            let Some(dir) = crate::world::general_direction(cur, start) else {
                return Some(path);
            };
            cur = crate::world::ROUTE_PREFERENCE[dir].iter().map(|&i| (cur.0 + NEIGHBOURS[i].0, cur.1 + NEIGHBOURS[i].1)).find(|&n| {
                let nd = at(n);
                nd > 0 && nd < d
            })?;
        }
        None
    }

    /// Digs a ditch from `start` to `end`, paying for each new tile.
    pub(crate) fn build_ditch(&mut self, start: (i32, i32), end: (i32, i32), measure: bool) -> Outcome {
        if !self.is_allowed(DITCH) {
            return Outcome::Invalid("Not available yet");
        }
        let Some(path) = self.ditch_path(start, end) else {
            return Outcome::Blocked;
        };
        let new: Vec<(i32, i32)> = path.iter().copied().filter(|&(x, y)| !self.map.terrain_is(x, y, terrain::CANAL)).collect();
        let items = new.len() as i32;
        let cost = items * self.cost_of(DITCH as usize);
        if measure {
            return Outcome::Done { items, cost };
        }
        if self.out_of_money() {
            return Outcome::NotEnoughMoney;
        }
        self.treasury -= cost;
        self.finance.this_year.construction += cost;
        for &(x, y) in &new {
            self.map.terrain.update(x, y, |t| t | terrain::CANAL);
            self.map.bitfields.update(x, y, |b| b & !0x10);
        }
        let (x0, x1) = (start.0.min(end.0), start.0.max(end.0));
        let (y0, y1) = (start.1.min(end.1), start.1.max(end.1));
        let (x0, y0, x1, y1) = path.iter().fold((x0, y0, x1, y1), |(a, b, c, d), &(x, y)| (a.min(x), b.min(y), c.max(x), d.max(y)));
        self.ditch_images_in(x0 - 1, y0 - 1, x1 + 1, y1 + 1);
        crate::terrain_images::refresh_grass(&mut self.map, &self.defs, x0 - 5, y0 - 5, x1 + 5, y1 + 5);
        crate::terrain_images::refresh_water(&mut self.map, &self.defs, x0 - 1, y0 - 1, x1 + 1, y1 + 1);
        Outcome::Done { items, cost }
    }

    /// Whether a road may be laid over the ditch at `(x, y)`: the ditch runs straight
    /// (by its image) and no road (`road` says where) lies on its line beside it.
    pub(crate) fn road_crosses_ditch(&self, x: i32, y: i32, road: &impl Fn(i32, i32) -> bool) -> bool {
        let base = self.defs.terrain.canal;
        let image = self.map.images.at_or(x, y, 0);
        if image < base || image >= base + 2 * DRY {
            return false;
        }
        ditch_line(image - base).is_some_and(|line| line.iter().all(|&(dx, dy)| !road(x + dx, y + dy)))
    }

    /// Redraws the ditches in a rectangle, keeping each one wet or dry.
    pub(crate) fn ditch_images_in(&mut self, x0: i32, y0: i32, x1: i32, y1: i32) {
        for y in y0.max(0)..=y1.min(self.map.height - 1) {
            for x in x0.max(0)..=x1.min(self.map.width - 1) {
                if let Some(image) = self.ditch_image(x, y) {
                    if self.map.terrain_is(x, y, terrain::ROAD) {
                        self.map.bitfields.update(x, y, |b| b & !0x80);
                    }
                    self.map.set_single_image(x, y, image);
                }
            }
        }
    }

    /// A ditch tile's image, from which ways it runs (to ditches, to water on the
    /// floodplain, and to a lift facing along it), whether it crosses a road, and
    /// whether it holds water.
    pub fn ditch_image(&self, x: i32, y: i32) -> Option<u32> {
        let map = &self.map;
        let t = |x: i32, y: i32| map.terrain.at_or(x, y, 0);
        let here = t(x, y);
        if here & terrain::CANAL == 0 || here & terrain::WATER != 0 || map.building.at_or(x, y, 0) != 0 {
            return None;
        }
        let base = self.defs.terrain.canal;
        let current = map.images.at_or(x, y, 0);
        let wet = current >= base && current < base + DRY;
        let here_road = here & terrain::ROAD != 0;
        let mut ways = [0u8; 8];
        for d in (0..8).step_by(2) {
            let (dx, dy) = NEIGHBOURS[d];
            let (nx, ny) = (x + dx, y + dy);
            let n = t(nx, ny);
            // On the floodplain a ditch opens into the water beside it.
            if here & terrain::FLOODPLAIN != 0 && n & terrain::WATER != 0 {
                ways[d] = 1;
            }
            if n & terrain::CANAL != 0 {
                // Across the ditch's line: beside the neighbour, and beside this tile.
                let (px, py) = (dy.abs(), dx.abs());
                let joins = if here_road {
                    !roadish(n) && (roadish(t(x - px, y - py)) || roadish(t(x + px, y + py)))
                } else {
                    !(roadish(n) && !roadish(t(nx - px, ny - py)) && !roadish(t(nx + px, ny + py)))
                };
                if joins {
                    ways[d] = 1;
                }
            }
            if n & terrain::BUILDING != 0
                && let Some(lift) = self.buildings.get(map.building.at_or(nx, ny, 0)).filter(|b| b.kind == WATER_LIFT)
            {
                let along = (lift.orientation % 2 == 0) == (dx == 0);
                let both_roads = t(x - dx, y - dy) & here & terrain::ROAD != 0;
                if along && !both_roads {
                    ways[d] = 1;
                }
            }
        }
        let row = self.defs.contexts.canal.iter().find(|r| r.tiles.iter().zip(ways).all(|(&want, got)| want == 2 || want == got))?;
        let mut offset = row.offsets[0];
        if here_road {
            if row.canal == 0 {
                offset = if roadish(t(x, y - 1)) { 0 } else { 1 };
            }
            let d = self.desirability.at_or(x, y, 0) as i32;
            let paved = here & terrain::FLOODPLAIN == 0 && (d > 4 || (d > 0 && here & terrain::FOUNTAIN_RANGE != 0));
            offset += if paved { UNDER_PAVED_ROAD } else { UNDER_ROAD };
        }
        if here & terrain::FLOODPLAIN != 0 {
            offset += ON_FLOODPLAIN;
        }
        Some(base + offset + if wet { 0 } else { DRY })
    }

    /// Whether `(x, y)` is a ditch whose image the water may change.
    fn open_ditch(&self, x: i32, y: i32) -> bool {
        self.map.terrain_is(x, y, terrain::CANAL) && !self.map.terrain_is(x, y, terrain::WATER) && self.map.building.at_or(x, y, 0) == 0
    }

    /// Whether a ditch tile holds water.
    pub fn ditch_wet(&self, x: i32, y: i32) -> bool {
        let base = self.defs.terrain.canal;
        let image = self.map.images.at_or(x, y, 0);
        self.map.terrain_is(x, y, terrain::CANAL) && image >= base && image < base + DRY
    }

    /// Marks the land within `range` of the `size` square at `(x, y)` with `flag`.
    fn mark_range(&mut self, x: i32, y: i32, size: i32, range: i32, flag: u32) {
        for yy in (y - range).max(0)..=(y + size - 1 + range).min(self.map.height - 1) {
            for xx in (x - range).max(0)..=(x + size - 1 + range).min(self.map.width - 1) {
                self.map.terrain.update(xx, yy, |t| t | flag);
            }
        }
    }

    /// Fills the ditch network from `start`: each tile it reaches holds water, each
    /// ditch tile reached from another irrigates the land round it, and a dry lift it
    /// touches gets water. The next tile worked on is the first new ditch found beside
    /// the last (north, east, south, west), the rest wait their turn; a flooded ditch
    /// ends the fill.
    fn fill_ditches(&mut self, start: (i32, i32), visited: &mut [bool]) {
        if !self.map.terrain_is(start.0, start.1, terrain::CANAL) {
            return;
        }
        let w = self.map.width;
        let base = self.defs.terrain.canal;
        let mut waiting = VecDeque::new();
        let mut cur = start;
        for _ in 0..FILL_LIMIT {
            let (x, y) = cur;
            if self.map.terrain_is(x, y, terrain::WATER) {
                return;
            }
            let image = self.map.images.at_or(x, y, 0);
            if self.map.building.at_or(x, y, 0) == 0 && image >= base + DRY && image < base + 2 * DRY {
                self.map.images.set(x, y, image - DRY);
            }
            visited[(y * w + x) as usize] = true;
            let mut next = None;
            for d in (0..8).step_by(2) {
                let (nx, ny) = (x + NEIGHBOURS[d].0, y + NEIGHBOURS[d].1);
                let lift = self.map.building.at_or(nx, ny, 0);
                if self.buildings.get(lift).is_some_and(|b| b.kind == WATER_LIFT) {
                    if self.buildings.get(lift).is_some_and(|b| b.water == 0) {
                        if let Some(b) = self.buildings.get_mut(lift) {
                            b.water = 2;
                        }
                        self.refresh_lift_image(lift);
                    }
                } else if self.map.terrain_is(nx, ny, terrain::CANAL) && !visited[(ny * w + nx) as usize] {
                    self.mark_range(nx, ny, 1, IRRIGATION_RANGE, terrain::IRRIGATION_RANGE);
                    if next.is_none() {
                        next = Some((nx, ny));
                    } else {
                        waiting.push_back((nx, ny));
                    }
                }
            }
            match next.or_else(|| waiting.pop_front()) {
                Some(n) => cur = n,
                None => return,
            }
        }
    }

    /// Tick 27: the day's water, as described at the top of this module.
    pub(crate) fn update_irrigation(&mut self) {
        let (w, h) = (self.map.width, self.map.height);
        let base = self.defs.terrain.canal;
        for y in 0..h {
            for x in 0..w {
                self.map.terrain.update(x, y, |t| t & !(terrain::IRRIGATION_RANGE | terrain::FOUNTAIN_RANGE));
                if self.open_ditch(x, y) {
                    let image = self.map.images.at_or(x, y, 0);
                    if image >= base && image < base + DRY {
                        self.map.images.set(x, y, image + DRY);
                    }
                }
            }
        }
        let mut visited = vec![false; (w * h).max(0) as usize];
        let lifts: Vec<BuildingId> = self.buildings.iter().filter(|b| b.kind == WATER_LIFT).map(|b| b.id).collect();
        for &id in &lifts {
            let Some(b) = self.buildings.get(id) else { continue };
            let intake = INTAKE[b.orientation as usize % 4].iter().any(|&(dx, dy)| self.map.terrain_is(b.x + dx, b.y + dy, terrain::WATER));
            let water = if intake { 2 } else { 0 };
            if b.water != water {
                if let Some(b) = self.buildings.get_mut(id) {
                    b.water = water;
                }
                self.refresh_lift_image(id);
            }
        }
        for (x, y) in self.river_ditch_sources() {
            if self.map.terrain_is(x, y, terrain::CANAL) && !visited[(y * w + x) as usize] {
                self.fill_ditches((x, y), &mut visited);
            }
        }
        loop {
            let mut pumped = false;
            for &id in &lifts {
                let Some(b) = self.buildings.get(id).filter(|b| b.water == 2 && b.workers > 0) else { continue };
                let (x, y, back) = (b.x, b.y, (b.orientation as usize + 2) % 4);
                if let Some(b) = self.buildings.get_mut(id) {
                    b.water = 1;
                }
                pumped = true;
                for (dx, dy) in INTAKE[back] {
                    self.fill_ditches((x + dx, y + dy), &mut visited);
                }
            }
            if !pumped {
                break;
            }
        }
        for &id in &lifts {
            if let Some(b) = self.buildings.get(id).filter(|b| self.lift_pumping(b)) {
                let (x, y, s) = (b.x, b.y, b.size);
                self.mark_range(x, y, s, IRRIGATION_RANGE, terrain::IRRIGATION_RANGE);
            }
        }
        let water: Vec<(BuildingId, u16, i32, i32)> = self.buildings.iter().filter(|b| matches!(b.kind, kind::WELL | kind::WATER_SUPPLY)).map(|b| (b.id, b.kind, b.x, b.y)).collect();
        for (id, k, x, y) in water {
            let fine = self.desirability.at_or(x, y, 0) as i32 >= FINE_DESIRABILITY;
            if let Some(image) = self.defs.building(k).map(|d| d.image + if fine { 2 } else { 0 })
                && self.buildings.get(id).is_some_and(|b| b.image != image)
            {
                self.set_building_image(id, image);
            }
            if k == kind::WELL {
                self.mark_range(x, y, 1, WELL_RANGE, terrain::FOUNTAIN_RANGE);
            }
        }
    }

    /// Whether farm `id` is irrigated: land marked by a ditch or lift lies in the
    /// square the original checks, from `size - 1` tiles before its top-left tile to
    /// `size - 1` after.
    pub fn is_irrigated(&self, id: BuildingId) -> bool {
        let Some(b) = self.buildings.get(id).filter(|b| self.is_farm(b.kind)) else { return false };
        let r = b.size - 1;
        (b.y - r..=b.y + r).any(|y| (b.x - r..=b.x + r).any(|x| self.map.terrain_is(x, y, terrain::IRRIGATION_RANGE)))
    }

    /// Whether a farm counts as on the floodplain for irrigation: any of its tiles is
    /// floodplain or the floodplain's bank.
    pub(crate) fn farm_on_floodplain(&self, id: BuildingId) -> bool {
        self.buildings.get(id).is_some_and(|b| b.tiles().any(|(x, y)| self.map.terrain_is(x, y, terrain::FLOODPLAIN) || floodplain_bank(&self.map, x, y)))
    }

    /// What irrigation adds to farm `id`'s fertility.
    pub(crate) fn irrigation_bonus(&self, id: BuildingId) -> i32 {
        match (self.is_irrigated(id), self.farm_on_floodplain(id)) {
            (false, _) => 0,
            (true, false) => IRRIGATED_MEADOW,
            (true, true) => IRRIGATED_FLOODPLAIN,
        }
    }

    /// Whether a water supply has groundwater to draw on: in the 3x3 square centred on
    /// its top-left tile.
    pub fn water_supply_has_water(&self, b: &Building) -> bool {
        (b.y - 1..=b.y + 1).any(|y| (b.x - 1..=b.x + 1).any(|x| self.map.terrain_is(x, y, terrain::GROUNDWATER)))
    }

    /// Days until a water supply's next water carrier: none without groundwater or
    /// staff; below desirability 30 its staffing counts half.
    pub(crate) fn water_supply_delay(&self, b: &Building) -> Option<i32> {
        if !self.water_supply_has_water(b) {
            return None;
        }
        let needed = self.workers_needed(b.kind).max(1);
        let mut pct = b.workers * 100 / needed;
        if (self.desirability.at_or(b.x, b.y, 0) as i32) < FINE_DESIRABILITY {
            pct /= 2;
        }
        let d = crate::services::walker_delays(b.kind);
        Some(match pct {
            p if p >= 100 => d[0],
            p if p >= 75 => d[1],
            p if p >= 50 => d[2],
            p if p >= 25 => d[3],
            p if p >= 1 => d[4],
            _ => return None,
        })
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A 12x12 map: river water in columns 0-2, floodplain in 3-5, dry land beyond.
    fn river_map() -> Map {
        use crate::grid::Grid;
        let mut map = Map { width: 12, height: 12, terrain: Grid::new(12, 12), images: Grid::new(12, 12), edges: Grid::new(12, 12), bitfields: Grid::new(12, 12), elevation: Grid::new(12, 12), random: Grid::new(12, 12), fertility: Grid::new(12, 12), moisture: Grid::new(12, 12), vegetation: Grid::new(12, 12), building: Grid::new(12, 12), border: Vec::new() };
        for y in 0..12 {
            for x in 0..12 {
                let t = match x {
                    0..=2 => terrain::WATER,
                    3..=5 => terrain::FLOODPLAIN,
                    _ => 0,
                };
                map.terrain.set(x, y, t);
            }
        }
        map
    }

    #[test]
    fn lifts_face_the_water_and_stand_on_its_bank() {
        let mut map = river_map();
        // Beside the floodplain the front row stands on its bank, facing west.
        assert_eq!(lift_site(&map, 6, 4), Some((3, 0)));
        // One tile further inland there is no bank to stand on.
        assert_eq!(lift_site(&map, 7, 4), None);
        // Out on the floodplain the back row would be on floodplain.
        assert_eq!(lift_site(&map, 4, 4), None);
        // Straight on a river with no floodplain: front row in the water, facing west.
        for y in 0..12 {
            for x in 3..6 {
                map.terrain.set(x, y, 0);
            }
        }
        assert_eq!(lift_site(&map, 2, 4), Some((3, 0)));
        assert_eq!(lift_site(&map, 3, 4), None);
        map.terrain.set(3, 5, terrain::TREE);
        assert_eq!(lift_site(&map, 2, 4), Some((3, 1)));
    }

    /// Mission 3's map, everything allowed and staffed.
    pub(crate) fn mission_world(n: usize) -> Option<World> {
        let data = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../PharaohData");
        if !data.is_dir() {
            return None;
        }
        let library = osiris_formats::ImageLibrary::open(&data.join("Data")).expect("open image library");
        let pak = osiris_formats::MissionPak::open(&data.join("mission1.pak")).expect("open mission1.pak");
        let scenario = pak.scenario(n).expect("scenario");
        let defs = std::sync::Arc::new(crate::defs::Defs::load(&library).expect("load defs"));
        let model_text = std::fs::read(data.join("Pharaoh_Model_Normal.txt")).expect("read model");
        let model = osiris_formats::Model::parse(&String::from_utf8_lossy(&model_text)).expect("parse model");
        let balance = std::sync::Arc::new(crate::balance::Balance::from_model(&model));
        let mut world = World::new(&scenario, defs, balance);
        world.start(&scenario);
        world.rules.global_labor_pool = true;
        world.rules.fire = false;
        world.rules.collapse = false;
        world.test_full_staff = true;
        world.treasury = 100_000;
        world.scenario_allowed = None;
        Some(world)
    }

    fn apply(w: &mut World, cmd: crate::world::Command) -> BuildingId {
        let out = w.apply(&cmd);
        assert!(matches!(out, Outcome::Done { items: 1.., .. }), "{cmd:?}: {out:?}");
        match cmd {
            crate::world::Command::Build { x, y, x1, y1, .. } => w.map.building.at_or(x1.max(x), y1.max(y), 0),
            _ => 0,
        }
    }

    #[test]
    fn a_lift_waters_ditches_and_the_farms_beside_them() {
        use crate::world::Command;
        let Some(mut w) = mission_world(3) else { return };
        // A lift on the floodplain's bank, fed by a ditch from the river along the
        // floodplain, pouring into a ditch north across the meadow.
        let lift = apply(&mut w, Command::Build { kind: WATER_LIFT, x: 59, y: 17, x1: 59, y1: 17 });
        apply(&mut w, Command::Road { start: (61, 17), end: (64, 17) });
        apply(&mut w, Command::Build { kind: DITCH, x: 41, y: 19, x1: 59, y1: 19 });
        apply(&mut w, Command::Build { kind: DITCH, x: 60, y: 16, x1: 60, y1: 10 });
        for y in 11..=13 {
            for x in 61..=63 {
                w.map.terrain.update(x, y, |t| t | terrain::MEADOW);
                w.map.fertility.set(x, y, 20);
            }
        }
        for y in 20..=22 {
            for x in 50..=52 {
                w.map.fertility.set(x, y, 30);
            }
        }
        let meadow = apply(&mut w, Command::Build { kind: kind::FARM_FIRST, x: 61, y: 11, x1: 61, y1: 11 });
        let floodplain = apply(&mut w, Command::Build { kind: kind::FARM_FIRST, x: 50, y: 20, x1: 50, y1: 20 });
        assert_eq!(w.buildings.get(lift).map(|b| b.orientation), Some(2));
        for _ in 0..2 * crate::time::TICKS_PER_DAY {
            w.tick();
        }
        assert_eq!(w.buildings.get(lift).map(|b| (b.water, b.workers > 0)), Some((1, true)));
        assert!(w.ditch_wet(50, 19) && w.ditch_wet(60, 12));
        assert!(w.is_irrigated(meadow) && w.is_irrigated(floodplain));
        // Soil 20 and 30: their averages plus 2, then 40 on the meadow and 20 on the floodplain.
        assert_eq!(w.fertility(meadow), 22 + 40);
        assert_eq!(w.fertility(floodplain), 32 + 20);
        // An irrigated floodplain harvest leaves half the soil.
        w.buildings.get_mut(floodplain).expect("farm").progress = 1000;
        w.harvest_floodplain_farms();
        assert_eq!(w.map.fertility.at_or(51, 21, 0), 15);
        // Without the lift the meadow ditch runs dry; the river still feeds the floodplain's.
        w.demolish(lift);
        for _ in 0..crate::time::TICKS_PER_DAY {
            w.tick();
        }
        assert!(!w.ditch_wet(60, 12) && w.ditch_wet(50, 19));
        assert!(!w.is_irrigated(meadow));
        assert_eq!(w.fertility(meadow), 22);
    }

    #[test]
    fn water_supplies_need_groundwater_and_work_at_half_strength_in_poor_areas() {
        use crate::world::Command;
        let Some(mut w) = mission_world(3) else { return };
        for y in 0..w.map.height {
            for x in 0..w.map.width {
                w.map.terrain.update(x, y, |t| t | terrain::GROUNDWATER);
            }
        }
        let (x, y) = (0..w.map.height).flat_map(|y| (0..w.map.width).map(move |x| (x, y))).find(|&(x, y)| w.can_place(kind::WATER_SUPPLY, x, y).is_ok()).expect("a site");
        let id = apply(&mut w, Command::Build { kind: kind::WATER_SUPPLY, x, y, x1: x, y1: y });
        let needed = w.workers_needed(kind::WATER_SUPPLY);
        w.buildings.get_mut(id).expect("supply").workers = needed;
        w.desirability.set(x, y, 40);
        let b = w.buildings.get(id).expect("supply").clone();
        assert_eq!(w.water_supply_delay(&b), Some(1));
        w.desirability.set(x, y, 29);
        assert_eq!(w.water_supply_delay(&b), Some(7));
        for y in y - 1..=y + 1 {
            for x in x - 1..=x + 1 {
                w.map.terrain.update(x, y, |t| t & !terrain::GROUNDWATER);
            }
        }
        assert_eq!(w.water_supply_delay(&b), None);
    }

    #[test]
    fn roads_cross_straight_ditches_and_ditches_pass_lifts_and_roadblocks() {
        use crate::world::Command;
        let Some(mut w) = mission_world(3) else { return };
        let lift = apply(&mut w, Command::Build { kind: WATER_LIFT, x: 59, y: 17, x1: 59, y1: 17 });
        assert_eq!(w.buildings.get(lift).map(|b| b.orientation), Some(2));
        apply(&mut w, Command::Build { kind: DITCH, x: 60, y: 16, x1: 60, y1: 10 });
        // A road straight over the ditch, but not along it.
        let path = w.road_path((57, 13), (63, 13)).expect("a road across the ditch");
        assert!(path.contains(&(60, 13)));
        assert!(w.road_path((60, 11), (60, 15)).is_none());
        apply(&mut w, Command::Road { start: (57, 13), end: (63, 13) });
        assert!(w.map.terrain_is(60, 13, terrain::ROAD | terrain::CANAL) && w.map.terrain_is(60, 13, terrain::CANAL));
        // A second road beside the first can't run the ditch's line onto it.
        assert!(w.road_path((60, 14), (60, 14)).is_none());
        // A ditch runs straight through the lift the way it faces, never across it.
        let through = w.ditch_path((60, 15), (60, 21)).expect("a ditch through the lift");
        assert!(through.contains(&(60, 17)) && through.contains(&(60, 18)));
        assert!(w.ditch_through(59, 18, Some(true)) && !w.ditch_through(59, 18, Some(false)));
        assert!(w.ditch_path((60, 17), (60, 15)).is_none());
        // And through a roadblock.
        apply(&mut w, Command::Road { start: (66, 10), end: (66, 16) });
        apply(&mut w, Command::Build { kind: crate::defenses::ROADBLOCK, x: 66, y: 13, x1: 66, y1: 13 });
        let across = w.ditch_path((64, 13), (68, 13)).expect("a ditch through the roadblock");
        assert!(across.contains(&(66, 13)));
    }

    #[test]
    fn groundwater_under_any_tile_will_do() {
        let Some(mut w) = mission_world(3) else { return };
        for y in 0..w.map.height {
            for x in 0..w.map.width {
                w.map.terrain.update(x, y, |t| t & !terrain::GROUNDWATER);
            }
        }
        // Mansions need it too.
        let (x, y) = (0..w.map.height).flat_map(|y| (0..w.map.width).map(move |x| (x, y))).find(|&(x, y)| w.can_place(77, x, y) == Err(crate::build::NEEDS_GROUNDWATER)).expect("a site");
        assert_eq!(w.can_place(kind::WELL, x, y), Err(crate::build::NEEDS_GROUNDWATER));
        assert_eq!(w.can_place(kind::WATER_SUPPLY, x, y), Err(crate::build::NEEDS_GROUNDWATER));
        w.map.terrain.update(x + 1, y + 1, |t| t | terrain::GROUNDWATER);
        assert_eq!(w.can_place(kind::WATER_SUPPLY, x, y), Ok(()));
        assert_eq!(w.can_place(kind::WELL, x + 1, y + 1), Ok(()));
    }

    #[test]
    fn ditches_cross_only_straight_roads() {
        let mut map = river_map();
        for x in 6..12 {
            map.terrain.set(x, 5, terrain::ROAD);
        }
        let none = |_: i32, _: i32| false;
        assert!(road_takes_ditch(&map, 8, 5, &none));
        // A ditch already running along the road's line blocks it.
        assert!(!road_takes_ditch(&map, 8, 5, &|x, y| (x, y) == (9, 5)));
        // A junction can't take one.
        map.terrain.set(8, 6, terrain::ROAD);
        assert!(!road_takes_ditch(&map, 8, 5, &none));
        assert!(!ditch_passable(&map, 1, 1));
        assert!(ditch_passable(&map, 4, 1));
    }
}
