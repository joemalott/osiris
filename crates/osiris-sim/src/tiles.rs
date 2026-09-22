//! Rules that choose terrain images from a tile's terrain and its neighbours.
//!
//! Editor maps store correct images, so these only run for tiles whose terrain
//! changes during play (roads, cleared land, rubble, ...), matching the original's
//! per-region refresh functions.

use crate::defs::{ContextRow, Defs};
use crate::map::{Map, NEIGHBOURS, mask, terrain};

/// Rotating variant counters, one per context-table row. The original keeps these as
/// global state, so repeated edge tiles cycle through their variants.
#[derive(Debug, Clone, Default)]
pub struct ContextCounters {
    dirt_road: Vec<u32>,
    paved_road: Vec<u32>,
}

#[derive(Debug, Clone, Copy)]
struct ContextImage {
    group_offset: u32,
    item_offset: u32,
}

fn match_context(rows: &[ContextRow], counters: &mut Vec<u32>, tiles: [u8; 8]) -> Option<ContextImage> {
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

fn fill_matches(map: &Map, x: i32, y: i32, mask: u32, hit: u8, miss: u8) -> [u8; 8] {
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

/// Grass level derived from the moisture byte: 0 none, 1..=12 growing to full,
/// 13 other, 16+ transition edges.
pub fn grass_level(map: &Map, x: i32, y: i32) -> u32 {
    let m = map.moisture.at_or(x, y, 0) as u32;
    if m & 0x80 != 0 {
        m - 0x80 + 16
    } else if m & 0x7 != 0 {
        (m - 0x7) / 8 + 1
    } else if m == 0 {
        0
    } else {
        13
    }
}

pub struct TileRules<'a> {
    pub defs: &'a Defs,
    pub counters: &'a mut ContextCounters,
    /// Desirability at a tile; roads pave themselves in desirable areas.
    pub desirability: &'a dyn Fn(i32, i32) -> i32,
}

impl TileRules<'_> {
    pub fn road_image(&mut self, map: &mut Map, x: i32, y: i32) {
        let t = map.terrain.at_or(x, y, 0);
        if t & terrain::ROAD == 0 || t & (terrain::WATER | terrain::BUILDING | terrain::CANAL) != 0 {
            return;
        }
        if map.bitfields.at_or(x, y, 0) & 0x80 != 0 {
            return; // plaza
        }
        let base = self.defs.terrain.road;
        let tiles = road_tiles(map, x, y);
        let d = (self.desirability)(x, y);
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

    fn clear_empty_land(&mut self, map: &mut Map, x: i32, y: i32) {
        if !map.terrain_is(x, y, mask::NOT_CLEAR) {
            map.set_single_image(x, y, 0);
        }
    }

    /// `size x size` tiles at `(x, y)` are inside the map, clear, and not yet imaged.
    fn is_clear_unimaged(map: &Map, x: i32, y: i32, size: i32) -> bool {
        (y..y + size).all(|yy| {
            (x..x + size).all(|xx| {
                map.contains(xx, yy) && !map.terrain_is(xx, yy, mask::NOT_CLEAR) && map.images.at_or(xx, yy, 1) == 0
            })
        })
    }

    fn set_empty_land(map: &mut Map, x: i32, y: i32, size: i32, image: u32) {
        let mut index = 0;
        for dy in 0..size {
            for dx in 0..size {
                let (xx, yy) = (x + dx, y + dy);
                map.terrain.update(xx, yy, |t| t & !mask::CLEARABLE);
                map.building.set(xx, yy, 0);
                map.bitfields.update(xx, yy, |b| b & !0x10);
                map.set_single_image(xx, yy, image + index);
                index += 1;
            }
        }
    }

    fn empty_land_pass1(&mut self, map: &mut Map, x: i32, y: i32) {
        if map.terrain_is(x, y, mask::NOT_CLEAR) || map.images.at_or(x, y, 1) != 0 {
            return;
        }
        let base = if map.bitfields.at_or(x, y, 0) & 0x20 != 0 {
            self.defs.terrain.empty_land_alt
        } else {
            self.defs.terrain.empty_land
        };
        let r = random(map, x, y);
        if Self::is_clear_unimaged(map, x, y, 4) {
            Self::set_empty_land(map, x, y, 4, base + 42);
        } else if Self::is_clear_unimaged(map, x, y, 3) {
            Self::set_empty_land(map, x, y, 3, base + 24 + 9 * (r & 1));
        } else if Self::is_clear_unimaged(map, x, y, 2) {
            Self::set_empty_land(map, x, y, 2, base + 8 + 4 * (r & 3));
        } else {
            Self::set_empty_land(map, x, y, 1, base + (r & 7));
        }
    }

    fn empty_land_pass2(&mut self, map: &mut Map, x: i32, y: i32) {
        let grass = grass_level(map, x, y);
        if map.terrain_is(x, y, mask::NOT_CLEAR | terrain::MEADOW) {
            return;
        }
        let base = self.defs.terrain.grass;
        let r = random(map, x, y);
        if (1..=11).contains(&grass) {
            Self::set_empty_land(map, x, y, 1, base + grass - 1 + 12 * (r % 3));
        } else if grass == 12 {
            let near = mask::NOT_CLEAR | terrain::MEADOW;
            let radius = if map.terrain_in_radius(x, y, 1, 1, near) {
                1
            } else if map.terrain_in_radius(x, y, 1, 2, near) || self.nonfull_grass_near(map, x, y) {
                2
            } else {
                3
            };
            let offset = match radius {
                1 => 36,
                2 => 60,
                _ => 48,
            };
            Self::set_empty_land(map, x, y, 1, base + offset + r % 12);
        } else if grass >= 16 {
            // Orientation 0: transition edges map straight onto the edge group.
            Self::set_empty_land(map, x, y, 1, self.defs.terrain.grass_edges + grass - 16);
        }
    }

    fn nonfull_grass_near(&self, map: &Map, x: i32, y: i32) -> bool {
        for yy in y - 1..=y + 1 {
            for xx in x - 1..=x + 1 {
                if map.contains(xx, yy) && grass_level(map, xx, yy) < 12 {
                    return true;
                }
            }
        }
        false
    }

    /// Recomputes cleared-land images in a region, as the original does after clearing.
    pub fn empty_land_in(&mut self, map: &mut Map, x0: i32, y0: i32, x1: i32, y1: i32, clear_first: bool) {
        let (x0, y0) = (x0.max(0), y0.max(0));
        let (x1, y1) = (x1.min(map.width - 1), y1.min(map.height - 1));
        if clear_first {
            for y in y0..=y1 {
                for x in x0..=x1 {
                    self.clear_empty_land(map, x, y);
                }
            }
        }
        for y in y0..=y1 {
            for x in x0..=x1 {
                self.empty_land_pass1(map, x, y);
            }
        }
        for y in y0..=y1 {
            for x in x0..=x1 {
                self.empty_land_pass2(map, x, y);
            }
        }
    }
}
