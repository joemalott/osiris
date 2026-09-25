//! Entertainment venues that sit on the road network. A booth, bandstand or pavilion
//! is a plaza laid over a crossing of roads; the tiles between the roads hold its
//! stalls, stages and gardens, and walkers keep using the roads through it.
//!
//! The festival square is placed the same way (`FUN_0043f410`): its 5x5 footprint
//! must be clear ground or road, and its middle tile a crossing whose four arms run
//! as road out to the square's edges, i.e. its middle row and middle column are road.
//! Unlike the smaller venues, the other 16 tiles may be road or not. Its paving
//! covers all 25 tiles, and a city has only one.

use crate::buildings::{BuildingId, kind};
use crate::map::{mask, terrain};
use crate::religion::FESTIVAL_SQUARE;
use crate::world::World;

/// The original's warning 19:75, for a venue placed off a crossing of roads.
pub const OVER_AN_INTERSECTION: &str = "You must place this entertainment venue over an intersection.";
/// The original's warning 19:213, for a festival square off a crossing of roads.
pub const FESTIVAL_OVER_A_CROSSING: &str = "Build the Festival Square over a road intersection";
/// The original's warning 19:2, for a second festival square.
pub const ONLY_ONE: &str = "You can only have one building of this type";

/// Which tiles of a booth's footprint must be road (`[orientation][y][x]`). The one
/// non-road tile holds the stall.
const BOOTH_ROADS: [[[u8; 2]; 2]; 4] = [[[0, 1], [1, 1]], [[1, 0], [1, 1]], [[1, 1], [1, 0]], [[1, 1], [0, 1]]];

/// For each booth orientation, two tiles (relative to the footprint) of which at least
/// one must be road: the road carrying on out of the plaza's corner.
const BOOTH_EXITS: [[(i32, i32); 2]; 4] = [[(1, 2), (2, 1)], [(-1, 1), (0, 2)], [(0, -1), (-1, 0)], [(1, -1), (2, 0)]];

const BANDSTAND_ROADS: [[[u8; 3]; 3]; 4] = [
    [[0, 1, 0], [0, 1, 0], [1, 1, 1]],
    [[1, 0, 0], [1, 1, 1], [1, 0, 0]],
    [[1, 1, 1], [0, 1, 0], [0, 1, 0]],
    [[0, 0, 1], [1, 1, 1], [0, 0, 1]],
];

/// Pavilion road layouts; orientations 0..8 use these four, some mirrored.
const PAVILION_ROADS: [[[u8; 4]; 4]; 4] = [
    [[0, 0, 1, 0], [0, 0, 1, 0], [0, 0, 1, 0], [1, 1, 1, 1]],
    [[1, 0, 0, 0], [1, 0, 0, 0], [1, 1, 1, 1], [1, 0, 0, 0]],
    [[1, 1, 1, 1], [0, 1, 0, 0], [0, 1, 0, 0], [0, 1, 0, 0]],
    [[0, 0, 0, 1], [1, 1, 1, 1], [0, 0, 0, 1], [0, 0, 0, 1]],
];

#[derive(Clone, Copy)]
enum Piece {
    Stall,
    Garden,
    /// The bandstand's stage: its main half and the other half.
    Stage(bool),
    /// The pavilion's 2x2 dance floor.
    Pavilion,
}

/// What stands on the non-road tiles of a bandstand, per orientation.
const BANDSTAND_PIECES: [[(Piece, i32, i32); 4]; 4] = [
    [(Piece::Garden, 2, 1), (Piece::Stall, 2, 0), (Piece::Stage(true), 0, 0), (Piece::Stage(false), 0, 1)],
    [(Piece::Garden, 1, 2), (Piece::Stall, 2, 2), (Piece::Stage(true), 1, 0), (Piece::Stage(false), 2, 0)],
    [(Piece::Garden, 2, 1), (Piece::Stall, 2, 2), (Piece::Stage(true), 0, 1), (Piece::Stage(false), 0, 2)],
    [(Piece::Garden, 1, 2), (Piece::Stall, 0, 2), (Piece::Stage(true), 1, 0), (Piece::Stage(false), 0, 0)],
];

const PAVILION_PIECES: [[(Piece, i32, i32); 6]; 8] = [
    [(Piece::Garden, 1, 2), (Piece::Garden, 3, 2), (Piece::Pavilion, 0, 0), (Piece::Stage(true), 3, 0), (Piece::Stage(false), 3, 1), (Piece::Stall, 0, 2)],
    [(Piece::Garden, 2, 2), (Piece::Garden, 0, 2), (Piece::Pavilion, 2, 0), (Piece::Stage(true), 0, 0), (Piece::Stage(false), 0, 1), (Piece::Stall, 3, 2)],
    [(Piece::Garden, 3, 0), (Piece::Garden, 3, 3), (Piece::Pavilion, 1, 0), (Piece::Stage(true), 1, 3), (Piece::Stage(false), 2, 3), (Piece::Stall, 3, 1)],
    [(Piece::Garden, 3, 3), (Piece::Garden, 1, 0), (Piece::Pavilion, 1, 2), (Piece::Stage(true), 2, 0), (Piece::Stage(false), 3, 0), (Piece::Stall, 3, 2)],
    [(Piece::Garden, 3, 3), (Piece::Garden, 0, 3), (Piece::Pavilion, 2, 1), (Piece::Stage(true), 0, 1), (Piece::Stage(false), 0, 2), (Piece::Stall, 2, 3)],
    [(Piece::Garden, 1, 3), (Piece::Garden, 3, 3), (Piece::Pavilion, 0, 1), (Piece::Stage(true), 3, 1), (Piece::Stage(false), 3, 2), (Piece::Stall, 0, 3)],
    [(Piece::Garden, 2, 0), (Piece::Garden, 2, 2), (Piece::Pavilion, 0, 2), (Piece::Stage(false), 0, 0), (Piece::Stage(true), 1, 0), (Piece::Stall, 2, 3)],
    [(Piece::Garden, 0, 3), (Piece::Garden, 2, 1), (Piece::Pavilion, 0, 0), (Piece::Stage(false), 1, 3), (Piece::Stage(true), 2, 3), (Piece::Stall, 2, 0)],
];

/// Stage image offsets (from `stand_sn_s`) for the main and other half, by the
/// stage's orientation, as seen with north up.
const STAGE_MAIN: [u32; 4] = [1, 2, 1, 3];
const STAGE_OTHER: [u32; 4] = [0, 3, 0, 2];

impl World {
    /// Whether `k` is a venue that must be built over roads.
    pub fn is_road_venue(&self, k: u16) -> bool {
        matches!(k, kind::BOOTH | kind::BANDSTAND | kind::PAVILION)
    }

    /// Whether `k` is placed over a crossing of roads: the venues and the festival
    /// square.
    pub fn is_built_over_roads(&self, k: u16) -> bool {
        self.is_road_venue(k) || k == FESTIVAL_SQUARE
    }

    /// Whether tile `(dx, dy)` of venue `k` in orientation `o` must be road.
    fn venue_road(k: u16, o: usize, dx: i32, dy: i32) -> bool {
        let (x, y) = (dx as usize, dy as usize);
        let v = match k {
            FESTIVAL_SQUARE => u8::from(dx == 2 || dy == 2),
            kind::BOOTH => BOOTH_ROADS[o][y][x],
            kind::BANDSTAND => BANDSTAND_ROADS[o][y][x],
            _ => {
                let t = &PAVILION_ROADS[o / 2];
                match o {
                    1 | 5 => t[y][3 - x],
                    3 | 7 => t[3 - y][x],
                    _ => t[y][x],
                }
            }
        };
        v == 1
    }

    fn venue_orientations(k: u16) -> usize {
        match k {
            kind::PAVILION => 8,
            FESTIVAL_SQUARE => 1,
            _ => 4,
        }
    }

    /// The orientation venue `k` (or the festival square) would take at `(x, y)`, if
    /// the roads there allow one.
    pub fn venue_orientation(&self, k: u16, x: i32, y: i32) -> Option<usize> {
        let size = self.size_of(k);
        for dy in 0..size {
            for dx in 0..size {
                let (tx, ty) = (x + dx, y + dy);
                if !self.map.contains(tx, ty) {
                    return None;
                }
                let t = self.map.terrain.at_or(tx, ty, 0);
                let road = t & terrain::ROAD != 0;
                if t & terrain::BUILDING != 0 || (t & mask::NOT_CLEAR != 0 && !road) {
                    return None;
                }
            }
        }
        (0..Self::venue_orientations(k)).find(|&o| {
            self.venue_fits(k, o, x, y)
                && (k != kind::BOOTH || BOOTH_EXITS[o].iter().any(|&(ex, ey)| self.map.terrain_is(x + ex, y + ey, terrain::ROAD)))
        })
    }

    /// Whether the roads at `(x, y)` match venue `k` in orientation `o`: a venue's
    /// road tiles exactly, the festival square's cross at least.
    fn venue_fits(&self, k: u16, o: usize, x: i32, y: i32) -> bool {
        let size = self.size_of(k);
        let fits = |dx: i32, dy: i32| {
            let road = self.map.terrain_is(x + dx, y + dy, terrain::ROAD);
            let wanted = Self::venue_road(k, o, dx, dy);
            if k == FESTIVAL_SQUARE { road || !wanted } else { road == wanted }
        };
        (0..size).all(|dy| (0..size).all(|dx| fits(dx, dy)))
    }

    /// Lays out a new venue's tiles: plaza over the roads, stalls and stages between.
    pub(crate) fn place_venue(&mut self, id: BuildingId) {
        let Some(b) = self.buildings.get(id) else { return };
        let (k, x, y, size) = (b.kind, b.x, b.y, b.size);
        // The festival square paves its whole footprint, road or not, with one image
        // per tile; the roads stay under it for walkers.
        if k == FESTIVAL_SQUARE {
            let plaza = self.defs.building(k).map_or(0, |d| d.anims.get("square").map_or(d.image, |a| a.image));
            for dy in 0..size {
                for dx in 0..size {
                    self.map.set_single_image(x + dx, y + dy, plaza + (dx + dy * size) as u32);
                }
            }
            return;
        }
        // The roads are still there under the new building's tiles.
        let Some(o) = (0..Self::venue_orientations(k)).find(|&o| self.venue_fits(k, o, x, y)) else { return };
        let image = |key: u16, anim: &str| {
            self.defs.building(key).map_or(0, |d| d.anims.get(anim).map_or(d.image, |a| a.image))
        };
        let plaza = image(k, "square");
        let stall = image(kind::BOOTH, "");
        let garden = image(kind::GARDENS, "");
        let stage = image(kind::BANDSTAND, "stand_sn_s");
        let floor = image(kind::PAVILION, "");
        for dy in 0..size {
            for dx in 0..size {
                if Self::venue_road(k, o, dx, dy) {
                    self.map.set_single_image(x + dx, y + dy, plaza + (dx + dy * size) as u32);
                }
            }
        }
        let pieces: &[(Piece, i32, i32)] = match k {
            kind::BOOTH => {
                let (dx, dy) = [(0, 0), (1, 0), (1, 1), (0, 1)][o];
                &[(Piece::Stall, dx, dy)]
            }
            kind::BANDSTAND => &BANDSTAND_PIECES[o],
            _ => &PAVILION_PIECES[o],
        };
        let stage_o = if k == kind::PAVILION { o / 2 } else { o };
        for &(piece, dx, dy) in pieces {
            let (px, py) = (x + dx, y + dy);
            match piece {
                Piece::Stall => self.map.set_single_image(px, py, stall),
                Piece::Garden => self.map.set_single_image(px, py, garden + (self.map.random.at_or(px, py, 0) & 3) as u32),
                Piece::Stage(main) => {
                    let off = if main { STAGE_MAIN[stage_o] } else { STAGE_OTHER[stage_o] };
                    self.map.set_single_image(px, py, stage + off);
                }
                Piece::Pavilion => self.map.set_footprint(px, py, 2, floor),
            }
        }
        if let Some(b) = self.buildings.get_mut(id) {
            b.orientation = o as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every piece of a venue sits on a tile that is not road.
    #[test]
    fn pieces_avoid_roads() {
        for (o, pieces) in BANDSTAND_PIECES.iter().enumerate() {
            for &(_, dx, dy) in pieces {
                assert!(!World::venue_road(kind::BANDSTAND, o, dx, dy), "bandstand {o} ({dx},{dy})");
            }
        }
        for (o, pieces) in PAVILION_PIECES.iter().enumerate() {
            for &(p, dx, dy) in pieces {
                let n = if matches!(p, Piece::Pavilion) { 2 } else { 1 };
                for yy in dy..dy + n {
                    for xx in dx..dx + n {
                        assert!(!World::venue_road(kind::PAVILION, o, xx, yy), "pavilion {o} ({xx},{yy})");
                    }
                }
            }
        }
    }

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
        world.treasury = 100_000;
        world.figures = Default::default();
        let (w, h) = (world.map.width, world.map.height);
        let (x, y) = (0..h - 20).flat_map(|y| (0..w - 20).map(move |x| (x, y))).find(|&(x, y)| world.map.area_clear_of(x, y, 20, mask::NOT_CLEAR))?;
        Some((world, x, y))
    }

    /// The festival square goes over a crossing whose roads run through its middle
    /// row and column; the other tiles may be road or not; it paves all 25 tiles and
    /// keeps the roads walkable; a city gets one.
    #[test]
    fn festival_square_straddles_a_crossing() {
        let Some((mut world, x, y)) = sandbox() else { return };
        let road = |w: &mut World, tx: i32, ty: i32| w.map.terrain.update(tx, ty, |t| t | terrain::ROAD);
        // Bare ground: no crossing.
        assert_eq!(world.can_place(FESTIVAL_SQUARE, x + 1, y + 1), Err(FESTIVAL_OVER_A_CROSSING));
        // A crossing at (x + 3, y + 3), its roads running on past the square.
        for i in 0..8 {
            road(&mut world, x + i, y + 3);
            road(&mut world, x + 3, y + i);
        }
        assert_eq!(world.can_place(FESTIVAL_SQUARE, x + 1, y + 1), Ok(()));
        // Off centre, the cross doesn't line up.
        assert!(world.can_place(FESTIVAL_SQUARE, x + 2, y + 1).is_err());
        assert!(world.can_place(FESTIVAL_SQUARE, x, y + 1).is_err());
        // Another road on a corner of the square doesn't matter.
        road(&mut world, x + 1, y + 1);
        assert_eq!(world.can_place(FESTIVAL_SQUARE, x + 1, y + 1), Ok(()));
        // A T junction won't do: one arm must reach every edge.
        world.map.terrain.update(x + 3, y + 5, |t| t & !terrain::ROAD);
        assert_eq!(world.can_place(FESTIVAL_SQUARE, x + 1, y + 1), Err(FESTIVAL_OVER_A_CROSSING));
        road(&mut world, x + 3, y + 5);
        // The ghost is the square's own paving, one image a tile.
        let plaza = world.defs.building(FESTIVAL_SQUARE).unwrap().anims["square"].image;
        let preview = world.placement_preview(FESTIVAL_SQUARE, x + 1, y + 1);
        assert!(preview.result.is_ok() && preview.tiles.len() == 25 && preview.tiles.iter().all(|t| !t.red));
        let mut ghost: Vec<u32> = world.placement_ghost(FESTIVAL_SQUARE, x + 1, y + 1).iter().map(|g| g.image).collect();
        ghost.sort();
        assert_eq!(ghost, (0..25).map(|i| plaza + i).collect::<Vec<_>>());
        // Built: every tile paved, the roads still there and walkable.
        assert!(matches!(world.build(FESTIVAL_SQUARE, x + 1, y + 1, x + 1, y + 1, false), crate::world::Outcome::Done { items: 1, .. }));
        for dy in 0..5 {
            for dx in 0..5 {
                assert_eq!(world.map.images.at_or(x + 1 + dx, y + 1 + dy, 0), plaza + (dx + dy * 5) as u32);
            }
        }
        for i in 0..8 {
            assert!(crate::figures::passable(&world.map, crate::figures::Travel::Roads, x + i, y + 3));
            assert!(crate::figures::passable(&world.map, crate::figures::Travel::Roads, x + 3, y + i));
        }
        assert!(world.buildings.iter().any(|b| b.kind == FESTIVAL_SQUARE && b.road.is_some()));
        // A second crossing elsewhere: still only one square.
        for i in 0..8 {
            road(&mut world, x + 10 + i, y + 13);
            road(&mut world, x + 13, y + 10 + i);
        }
        assert_eq!(world.can_place(FESTIVAL_SQUARE, x + 11, y + 11), Err(ONLY_ONE));
    }
}

