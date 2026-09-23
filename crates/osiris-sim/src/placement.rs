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

use crate::world::World;

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

impl World {
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
