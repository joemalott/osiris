//! Per-tile map state. Coordinates are map-relative: `(0, 0)` is the first playable tile.

use crate::grid::Grid;
use osiris_formats::Scenario;
use osiris_formats::chunks::GRID_SIZE;
pub use osiris_formats::scenario::terrain;

/// Terrain combinations used by placement and tile-image rules.
pub mod mask {
    use super::terrain::*;
    pub const NOT_CLEAR: u32 = TREE
        | ROCK
        | WATER
        | BUILDING
        | SHRUB
        | GARDEN
        | ROAD
        | CANAL
        | ELEVATION
        | ACCESS_RAMP
        | RUBBLE
        | WALL
        | GATEHOUSE
        | FLOODPLAIN
        | MARSHLAND
        | DIKE
        | ORE
        | DUNE
        | DEEPWATER
        | SUBMERGED_ROAD;
    pub const CLEARABLE: u32 = NOT_CLEAR
        & !(ROCK | WATER | ELEVATION | FLOODPLAIN | MARSHLAND | ORE | DUNE | DEEPWATER | SUBMERGED_ROAD);
    pub const ROAD_BLOCKED: u32 = NOT_CLEAR & !(FLOODPLAIN | ROAD);
    pub const IMPASSABLE: u32 = NOT_CLEAR & !(ROAD | GATEHOUSE | DEEPWATER | SUBMERGED_ROAD);
}

/// Neighbour offsets in the order the tile-context tables use: N, NE, E, SE, S, SW, W, NW
/// (in map coordinates, "north" is `y - 1`).
pub const NEIGHBOURS: [(i32, i32); 8] =
    [(0, -1), (1, -1), (1, 0), (1, 1), (0, 1), (-1, 1), (-1, 0), (-1, -1)];

/// Multi-tile bits in `edges`.
pub mod edge {
    pub const X: u8 = 0x07;
    pub const Y: u8 = 0x38;
    pub const DRAW_TILE: u8 = 0x40;
    pub const NATIVE_LAND: u8 = 0x80;
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Map {
    pub width: i32,
    pub height: i32,
    pub terrain: Grid<u32>,
    pub images: Grid<u32>,
    pub edges: Grid<u8>,
    pub bitfields: Grid<u8>,
    pub elevation: Grid<u8>,
    pub random: Grid<u8>,
    pub fertility: Grid<u8>,
    pub moisture: Grid<u8>,
    pub vegetation: Grid<u8>,
    /// Building occupying each tile; 0 = none.
    pub building: Grid<u32>,
    /// Terrain of the ring of tiles just outside the map, as the map file stores it
    /// (see `terrain_around`). Only redrawing the terrain on load reads it.
    #[serde(skip)]
    pub border: Vec<u32>,
}

impl Map {
    pub fn from_scenario(s: &Scenario) -> Self {
        let (w, h) = (s.info.width, s.info.height);
        let off = |x, y| s.offset(x, y).expect("inside map");
        Self {
            width: w,
            height: h,
            terrain: Grid::from_fn(w, h, |x, y| s.terrain[off(x, y)] & !terrain::PARADE_GROUND),
            images: Grid::from_fn(w, h, |x, y| s.images[off(x, y)]),
            edges: Grid::from_fn(w, h, |x, y| s.edges[off(x, y)]),
            bitfields: Grid::from_fn(w, h, |x, y| s.bitfields[off(x, y)]),
            elevation: Grid::from_fn(w, h, |x, y| s.elevation[off(x, y)]),
            random: Grid::from_fn(w, h, |x, y| s.random[off(x, y)]),
            fertility: Grid::from_fn(w, h, |x, y| s.soil_fertility[off(x, y)]),
            moisture: Grid::from_fn(w, h, |x, y| s.moisture[off(x, y)]),
            vegetation: Grid::from_fn(w, h, |x, y| s.vegetation_growth[off(x, y)]),
            building: Grid::new(w, h),
            border: Self::border_ring(w, h)
                .map(|(x, y)| usize::try_from(s.info.start_offset + y * GRID_SIZE as i32 + x).ok().and_then(|i| s.terrain.get(i).copied()).unwrap_or(0))
                .collect(),
        }
    }

    /// The tiles just outside a `w x h` map: the rows above and below (corners
    /// included), then the columns to the left and right.
    fn border_ring(w: i32, h: i32) -> impl Iterator<Item = (i32, i32)> {
        let rows = [-1, h].into_iter().flat_map(move |y| (-1..=w).map(move |x| (x, y)));
        let columns = [-1, w].into_iter().flat_map(move |x| (0..h).map(move |y| (x, y)));
        rows.chain(columns)
    }

    /// Terrain at `(x, y)`, reading tiles just outside the map from `border` and
    /// `outside` beyond them.
    pub fn terrain_around(&self, x: i32, y: i32, outside: u32) -> u32 {
        if self.contains(x, y) {
            return self.terrain.at_or(x, y, outside);
        }
        let (w, h) = (self.width, self.height);
        let index = if (y == -1 || y == h) && (-1..=w).contains(&x) {
            (if y == -1 { 0 } else { w + 2 }) + x + 1
        } else if (x == -1 || x == w) && (0..h).contains(&y) {
            2 * (w + 2) + if x == -1 { 0 } else { h } + y
        } else {
            return outside;
        };
        self.border.get(index as usize).copied().unwrap_or(outside)
    }

    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= 0 && y >= 0 && x < self.width && y < self.height
    }

    pub fn terrain_is(&self, x: i32, y: i32, mask: u32) -> bool {
        self.terrain.at_or(x, y, 0) & mask != 0
    }

    /// Any tile within `radius` of the `size x size` area at `(x, y)` has terrain `mask`.
    pub fn terrain_in_radius(&self, x: i32, y: i32, size: i32, radius: i32, mask: u32) -> bool {
        for yy in (y - radius)..(y + size + radius) {
            for xx in (x - radius)..(x + size + radius) {
                if self.terrain_is(xx, yy, mask) {
                    return true;
                }
            }
        }
        false
    }

    /// Every tile of the `size x size` area at `(x, y)` lies inside the map and has
    /// none of `mask`.
    pub fn area_clear_of(&self, x: i32, y: i32, size: i32, mask: u32) -> bool {
        (y..y + size).all(|yy| (x..x + size).all(|xx| self.contains(xx, yy) && !self.terrain_is(xx, yy, mask)))
    }

    /// Marks `(x, y)` as a single-tile image drawn from its own tile.
    pub fn set_single_image(&mut self, x: i32, y: i32, image: u32) {
        self.images.set(x, y, image);
        self.edges.update(x, y, |e| (e & edge::NATIVE_LAND) | edge::DRAW_TILE);
        self.bitfields.update(x, y, |b| b & 0xf0);
    }

    /// Places a `size x size` footprint drawn from its leftmost tile `(x, y + size - 1)`.
    pub fn set_footprint(&mut self, x: i32, y: i32, size: i32, image: u32) {
        for dy in 0..size {
            for dx in 0..size {
                let draw = dx == 0 && dy == size - 1;
                self.images.set(x + dx, y + dy, image);
                self.edges.update(x + dx, y + dy, |e| {
                    (e & edge::NATIVE_LAND)
                        | (dx as u8 & edge::X)
                        | ((dy as u8) << 3 & edge::Y)
                        | if draw { edge::DRAW_TILE } else { 0 }
                });
                self.bitfields
                    .update(x + dx, y + dy, |b| (b & 0xf0) | (size - 1) as u8 & 0x0f);
            }
        }
    }
}
