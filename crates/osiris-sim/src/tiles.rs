//! Rules that choose terrain images from a tile's terrain and its neighbours.
//!
//! Editor maps store correct images, so these only run for tiles whose terrain
//! changes during play (roads, cleared land, rubble, ...), matching the original's
//! per-region refresh functions.

use crate::defs::{ContextRow, Defs};
use crate::map::{Map, NEIGHBOURS, terrain};

/// Rotating variant counters, one per context-table row. The original keeps these as
/// global state, so repeated edge tiles cycle through their variants.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ContextCounters {
    dirt_road: Vec<u32>,
    paved_road: Vec<u32>,
    earthquake: Vec<u32>,
}

/// A resolved image offset from a context-table match. `pub(crate)` so other tile-image
/// producers (e.g. `floods.rs`, matching the water table against flooded floodplain tiles)
/// can reuse the same neighbour-context matching as roads and rubble.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ContextImage {
    pub(crate) group_offset: u32,
    pub(crate) item_offset: u32,
}

pub(crate) fn match_context(rows: &[ContextRow], counters: &mut Vec<u32>, tiles: [u8; 8]) -> Option<ContextImage> {
    if counters.len() != rows.len() {
        counters.resize(rows.len(), 0);
    }
    for (i, row) in rows.iter().enumerate() {
        if row.tiles.iter().zip(tiles).all(|(&want, got)| want == 2 || want == got) {
            counters[i] += 1;
            if counters[i] >= row.variants {
                counters[i] = 0;
            }
            return Some(ContextImage {
                group_offset: row.offsets[0],
                item_offset: counters[i],
            });
        }
    }
    None
}

pub(crate) fn fill_matches(map: &Map, x: i32, y: i32, mask: u32, hit: u8, miss: u8) -> [u8; 8] {
    NEIGHBOURS.map(|(dx, dy)| if map.terrain_is(x + dx, y + dy, mask) { hit } else { miss })
}

fn road_tiles(map: &Map, x: i32, y: i32) -> [u8; 8] {
    let mut tiles = fill_matches(map, x, y, terrain::ROAD, 1, 0);
    for i in (0..8).step_by(2) {
        let (dx, dy) = NEIGHBOURS[i];
        if map.terrain_is(x + dx, y + dy, terrain::ACCESS_RAMP) {
            tiles[i] = 1;
        }
    }
    tiles
}

fn random(map: &Map, x: i32, y: i32) -> u32 {
    map.random.at_or(x, y, 0) as u32
}

pub struct TileRules<'a> {
    pub defs: &'a Defs,
    pub counters: &'a mut ContextCounters,
    /// The city's desirability grid; roads pave themselves in desirable areas.
    pub desirability: &'a crate::grid::Grid<i8>,
}

impl TileRules<'_> {
    pub fn road_image(&mut self, map: &mut Map, x: i32, y: i32) {
        let t = map.terrain.at_or(x, y, 0);
        if t & terrain::ROAD == 0 || t & (terrain::WATER | terrain::BUILDING | terrain::CANAL | terrain::GATEHOUSE) != 0 {
            return;
        }
        // A roadblock (or anything else standing on the road) keeps its own image.
        if map.building.at_or(x, y, 0) != 0 {
            return;
        }
        if map.bitfields.at_or(x, y, 0) & 0x80 != 0 {
            return; // plaza
        }
        let base = self.defs.terrain.road;
        let tiles = road_tiles(map, x, y);
        let d = self.desirability.at_or(x, y, 0) as i32;
        let paved = d > 4 || (d > 0 && t & terrain::FOUNTAIN_RANGE != 0);
        let image = if paved {
            let c = match_context(&self.defs.contexts.paved_road, &mut self.counters.paved_road, tiles);
            c.map(|c| base + c.group_offset + c.item_offset)
        } else if t & terrain::FLOODPLAIN == 0 {
            let fp = self.defs.terrain.floodplain;
            if map.terrain_is(x, y - 1, terrain::FLOODPLAIN) {
                Some(fp + 84)
            } else if map.terrain_is(x + 1, y, terrain::FLOODPLAIN) {
                Some(fp + 85)
            } else if map.terrain_is(x, y + 1, terrain::FLOODPLAIN) {
                Some(fp + 86)
            } else if map.terrain_is(x - 1, y, terrain::FLOODPLAIN) {
                Some(fp + 87)
            } else {
                match_context(&self.defs.contexts.dirt_road, &mut self.counters.dirt_road, tiles)
                    .map(|c| base + c.group_offset + c.item_offset + 49)
            }
        } else {
            match_context(&self.defs.contexts.dirt_road, &mut self.counters.dirt_road, tiles)
                .map(|c| base + c.group_offset + c.item_offset + 49 + 344)
        };
        if let Some(image) = image {
            map.set_single_image(x, y, image);
        }
    }

    pub fn roads_in(&mut self, map: &mut Map, x0: i32, y0: i32, x1: i32, y1: i32) {
        for y in y0..=y1 {
            for x in x0..=x1 {
                self.road_image(map, x, y);
            }
        }
    }

    /// Earthquake cracks: rock tiles with the earthquake mark (`0x80` in the bitfields)
    /// join up with the marked rock around them. Tiles no row matches get the first
    /// crack image.
    pub fn crack_image(&mut self, map: &mut Map, x: i32, y: i32) {
        let marked = |x: i32, y: i32| map.terrain_is(x, y, terrain::ROCK) && map.bitfields.at_or(x, y, 0) & 0x80 != 0;
        if !marked(x, y) {
            return;
        }
        let tiles = NEIGHBOURS.map(|(dx, dy)| marked(x + dx, y + dy) as u8);
        let base = self.defs.terrain.earthquake;
        let image = match_context(&self.defs.contexts.earthquake, &mut self.counters.earthquake, tiles).map_or(base, |c| base + c.group_offset + c.item_offset);
        map.set_single_image(x, y, image);
    }

    pub fn rubble_image(&mut self, map: &mut Map, x: i32, y: i32) {
        let forbidden = terrain::CANAL
            | terrain::ELEVATION
            | terrain::ACCESS_RAMP
            | terrain::ROAD
            | terrain::BUILDING
            | terrain::GARDEN;
        if map.terrain_is(x, y, terrain::RUBBLE) && !map.terrain_is(x, y, forbidden) {
            let image = self.defs.terrain.rubble + (random(map, x, y) & 7);
            map.set_single_image(x, y, image);
        }
    }
}
