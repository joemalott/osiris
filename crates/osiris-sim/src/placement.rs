//! The placement preview drawn while the player holds a building tool: every tile the
//! building would take, each allowed (green) or not (red), and the tiles that are at
//! fault marked with why. It is built from the same per-tile checks
//! [`World::can_place`] makes, and its verdict is `can_place`'s own, so a preview with
//! no red tile always builds.
//!
//! A pyramid or mastaba is drawn as the original draws it (`FUN_004edbb0`): each tile
//! of its footprint green or red by its own ground, the tiles of the row past its
//! south edge only where they block, and a complex's mortuary temple, causeway and
//! valley temple, as far as the causeway could be laid out, all in the colour of the
//! whole placement. Osiris also marks the block where the causeway failed.
//!
//! Where a building may go, the original shows its ghost instead of the tiles: the
//! art it would put on the map, tinted green (`FUN_0043b630`). Houses, tombs and the
//! other monuments but the sphinx keep their tiles.

use crate::map::edge;
use crate::world::{Outcome, World};

/// One tile of a placement preview.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PreviewTile {
    pub x: i32,
    pub y: i32,
    /// The rule this very tile breaks, if it breaks one.
    pub blocked: Option<&'static str>,
    /// Drawn red: it breaks a rule, or it is shown in the colour of a placement that
    /// fails (a complex's parts, or everything when no one tile is at fault).
    pub red: bool,
}

/// What placing a building would take and whether it may be placed.
#[derive(Debug, Clone)]
pub struct Preview {
    pub tiles: Vec<PreviewTile>,
    /// [`World::can_place`]'s verdict.
    pub result: Result<(), &'static str>,
}

/// One image of a placement ghost, drawn from draw tile `(x, y)` as the map draws
/// its images.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GhostImage {
    pub x: i32,
    pub y: i32,
    pub image: u32,
}

impl World {
    /// Whether building type `k` shows its ghost where it may be placed. The
    /// original draws only the tiles of houses, roads and walls, bridges, tombs and
    /// every monument but the sphinx.
    pub fn has_ghost(&self, k: u16) -> bool {
        use crate::buildings::kind;
        let drawn = |k: u16| self.defs.building(k).is_some();
        let monument = crate::monuments::monument_def(k).is_some_and(|d| d.style != crate::monuments::Style::Sphinx);
        drawn(k)
            && !kind::is_house(k)
            && !monument
            && !crate::royal_tombs::is_royal_tomb(k)
            && !crate::defenses::is_wall(k)
            && ![kind::ROAD, crate::irrigation::DITCH, crate::bridges::LOW_BRIDGE].contains(&k)
    }

    /// The art that building `k` placed with its top-left tile at `(x, y)` (an
    /// altar or oracle: clicked on tile `(x, y)`) would put on the map, found by
    /// building it on a copy of the city: the images of every draw tile it lays or
    /// changes. Empty where it can't be built; money is not asked about.
    pub fn placement_ghost(&self, k: u16, x: i32, y: i32) -> Vec<GhostImage> {
        let mut trial = self.clone();
        trial.treasury = i32::MAX / 4;
        if !matches!(trial.build(k, x, y, x, y, false), Outcome::Done { .. }) {
            return Vec::new();
        }
        let (old, new) = (&self.map, &trial.map);
        let mut out = Vec::new();
        for ty in 0..new.height {
            for tx in 0..new.width {
                let image = new.images.at_or(tx, ty, 0);
                let draw = new.edges.at_or(tx, ty, 0) & edge::DRAW_TILE != 0;
                let changed = image != old.images.at_or(tx, ty, 0) || new.edges.at_or(tx, ty, 0) != old.edges.at_or(tx, ty, 0);
                if draw && changed && image != 0 && new.building.at_or(tx, ty, 0) != 0 {
                    out.push(GhostImage { x: tx, y: ty, image });
                }
            }
        }
        out
    }

    /// The tile of building type `k`'s footprint that sits under the cursor: a tomb's
    /// anchor block, as in the original, otherwise the middle tile.
    pub fn cursor_tile(&self, k: u16) -> (i32, i32) {
        if let Some(def) = crate::monuments::monument_def(k).filter(|d| crate::pyramids::blockwise(d.style)) {
            return crate::pyramids::anchor(def.style, crate::pyramids::variant(def.cols, def.style));
        }
        let (w, h) = self.footprint_of(k);
        ((w - 1) / 2, (h - 1) / 2)
    }

    /// The preview of a building of type `k` placed with its top-left tile at `(x, y)`.
    pub fn placement_preview(&self, k: u16, x: i32, y: i32) -> Preview {
        let result = self.can_place(k, x, y);
        let failed = result.is_err();
        let own = |x: i32, y: i32, blocked: Option<&'static str>| PreviewTile { x, y, blocked, red: blocked.is_some() };
        let whole = |x: i32, y: i32| PreviewTile { x, y, blocked: None, red: failed };
        let mut tiles = Vec::new();
        if crate::temple_complex::is_upgrade(k) {
            tiles.push(whole(x, y));
        } else if self.uses_common_rules(k) {
            let (w, h) = self.footprint_of(k);
            for yy in y..y + h {
                for xx in x..x + w {
                    tiles.push(own(xx, yy, self.footprint_tile_problem(k, xx, yy)));
                }
            }
            if crate::military::fort_soldier(k).is_some() {
                let why = "No room for the parade ground";
                tiles.extend(crate::military::fort_ground(x, y).map(|(xx, yy)| own(xx, yy, (!self.fort_ground_tile_clear(xx, yy)).then_some(why))));
            }
            // The row past a tomb's south edge shows only where it blocks.
            tiles.extend(self.tomb_row(k, (x, y)).into_iter().filter_map(|(xx, yy)| self.tomb_row_problem(xx, yy).map(|why| own(xx, yy, Some(why)))));
            if let Some(def) = crate::monuments::monument_def(k) {
                let walk = self.complex_walk(def.style, crate::pyramids::variant(def.cols, def.style), (x, y));
                tiles.extend(Self::part_tiles(&walk.parts).into_iter().map(|(px, py)| whole(x + px, y + py)));
                if let Some(((bx, by), why)) = walk.fail {
                    tiles.extend([(0, 0), (1, 0), (0, 1), (1, 1)].map(|(dx, dy)| own(bx + dx, by + dy, Some(why))));
                }
            }
        } else if k == crate::defenses::GATEHOUSE {
            tiles.extend(self.gatehouse_tiles(x, y).into_iter().map(|((xx, yy), why)| own(xx, yy, why)));
        } else if crate::defenses::is_tower(k) {
            for yy in y..y + 2 {
                for xx in x..x + 2 {
                    tiles.push(own(xx, yy, self.tower_tile_problem(xx, yy)));
                }
            }
        } else if let Some(cut) = self.royal_tomb_tiles(k, x, y) {
            // A royal tomb's bulk in the cliffs, and its entrance just outside.
            tiles.extend(cut.into_iter().map(|((xx, yy), why)| own(xx, yy, why)));
        } else {
            let (w, h) = self.footprint_of(k);
            for yy in y..y + h {
                for xx in x..x + w {
                    tiles.push(whole(xx, yy));
                }
            }
        }
        // A rule that is no one tile's fault (granite in storage, one at a time...)
        // turns the whole preview red.
        if failed && tiles.iter().all(|t| !t.red) {
            for t in &mut tiles {
                t.red = true;
            }
        }
        debug_assert!(failed || tiles.iter().all(|t| !t.red), "a preview that builds shows no red");
        Preview { tiles, result }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mission_one() -> Option<World> {
        let data = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../PharaohData");
        if !data.join("mission1.pak").is_file() {
            return None;
        }
        let library = osiris_formats::ImageLibrary::open(&data.join("Data")).expect("open image library");
        let scenario = osiris_formats::MissionPak::open(&data.join("mission1.pak")).expect("pak").scenario(1).expect("scenario");
        let defs = std::sync::Arc::new(crate::defs::Defs::load(&library).expect("load defs"));
        let model_text = std::fs::read(data.join("Pharaoh_Model_Normal.txt")).expect("read model");
        let model = osiris_formats::Model::parse(&String::from_utf8_lossy(&model_text)).expect("parse model");
        let balance = std::sync::Arc::new(crate::balance::Balance::from_model(&model));
        let mut world = World::new(&scenario, defs, balance);
        world.start(&scenario);
        world.scenario_allowed = None;
        world.treasury = 100_000;
        Some(world)
    }

    #[test]
    fn statue_ghost_is_its_chosen_look() {
        let Some(mut world) = mission_one() else { return };
        const LARGE_STATUE: u16 = 43;
        world.statue_variant = 2;
        world.statue_facing = 3;
        let (x, y) = (0..world.map.height)
            .flat_map(|y| (0..world.map.width).map(move |x| (x, y)))
            .find(|&(x, y)| x > 20 && y > 20 && world.can_place(LARGE_STATUE, x, y).is_ok())
            .expect("a site");
        let images = world.map.images.clone();
        let started = std::time::Instant::now();
        let ghost = world.placement_ghost(LARGE_STATUE, x, y);
        eprintln!("ghost worked out in {:?}", started.elapsed());
        // Drawn from the footprint's left corner, as built.
        assert_eq!(ghost, vec![GhostImage { x, y: y + 2, image: world.statue_image(LARGE_STATUE).unwrap() }]);
        assert!(world.map.images == images && world.map.building.at_or(x, y, 0) == 0, "the city itself is untouched");
        // Nothing where it can't go.
        world.create_building(LARGE_STATUE, x, y);
        assert!(world.placement_ghost(LARGE_STATUE, x, y).is_empty());
    }
}
