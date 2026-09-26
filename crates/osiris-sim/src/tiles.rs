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
    /// Redraws road tile `(x, y)` (see `terrain_images::road_image`).
    pub fn road_image(&mut self, map: &mut Map, x: i32, y: i32) {
        let shore = crate::terrain_images::floodplain_shore(map, x, y);
        let counters = crate::terrain_images::RoadCounters { dirt: &mut self.counters.dirt_road, paved: &mut self.counters.paved_road };
        if let Some(image) = crate::terrain_images::road_image(map, self.defs, counters, Some(self.desirability), shore, x, y) {
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
