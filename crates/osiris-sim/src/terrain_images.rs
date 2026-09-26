//! Rebuilds every terrain image from the terrain grid.
//!
//! The original redraws the terrain of a map or campaign mission whenever it starts one:
//! it turns shrubs into trees, then clears and redraws earthquake cracks, rocks (ore and
//! Cleopatra's cliffs too), trees, gardens, grass and empty land, the floodplain,
//! meadow, marshland, dunes, water, rubble, roads and plazas. It then marks the
//! floodplain's rows and banks and does it all again, since the banks change how the
//! land and water beside the floodplain are drawn. Maps saved at version 147 to 160
//! store the result; older ones store ids from an earlier sprite layout, and later
//! versions come from other editors (Akhenaten writes 189) whose images need not follow
//! the terrain, so Osiris runs this pass on both.
//!
//! Almost every image follows from the terrain, the random grid, moisture and soil
//! fertility. Two things don't: earthquake cracks cycle through their variants, and a
//! dry floodplain tile shows how far its crops have grown, which a new map starts at 0.
//! Canals, walls, buildings and the entry and exit flags are not drawn here; roads are
//! never paved (a new map has no desirability).

use crate::defs::{ContextRow, Defs};
use crate::grid::Grid;
use crate::map::{Map, NEIGHBOURS, terrain};

/// The set a rebuilt tile's image was picked from: its first image, how many images it
/// holds and how far apart they lie (0 or 1: side by side). The pass picks one of them
/// from state a map doesn't store (a rotating counter, the floodplain's growth, how often
/// the water has been drawn), so a stored map can hold another image of the set.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Choice {
    pub first: u32,
    pub count: u32,
    pub stride: u32,
}

impl Choice {
    pub fn contains(&self, image: u32) -> bool {
        let step = self.stride.max(1);
        image >= self.first && (image - self.first) % step == 0 && (image - self.first) / step < self.count
    }
}

/// Water the city view animates, as the original's (FUN_00430ef0) does whenever it
/// draws a tile more than 60 ms after the last step: open water steps through its six
/// images one at a time, deep water through its six 15-image loops. The step is written
/// back into the image grid, so a saved game holds whichever frame each tile was on.
/// Returns the set `image` belongs to, if it animates.
pub fn animated_water(defs: &Defs, image: u32) -> Option<Choice> {
    let t = &defs.terrain;
    if (t.flood_water..t.flood_water + 6).contains(&image) {
        Some(Choice { first: t.flood_water, count: 6, stride: 1 })
    } else if (t.deepwater..t.deepwater + 90).contains(&image) {
        Some(Choice { first: t.deepwater + (image - t.deepwater) % 15, count: 6, stride: 15 })
    } else {
        None
    }
}

/// The image an animated water tile shows `step` frames after `image` (see
/// `animated_water`); other images are returned as they are.
pub fn water_frame(defs: &Defs, image: u32, step: u32) -> u32 {
    match animated_water(defs, image) {
        Some(c) => {
            let frame = (image - c.first) / c.stride.max(1);
            c.first + (frame + step) % c.count * c.stride.max(1)
        }
        None => image,
    }
}

/// Terrain the rebuild never redraws.
const KEEP: u32 = terrain::BUILDING | terrain::WALL | terrain::GATEHOUSE | terrain::CANAL | terrain::ELEVATION | terrain::ACCESS_RAMP;

/// Terrain of the grid around a map: trees and water with the outside-the-map bit, which
/// `terrain` calls `DIKE`.
const OUTSIDE: u32 = terrain::DIKE | terrain::WATER | terrain::TREE;

/// Terrain bits the pass ignores when it asks whether land is clear.
const CLEAR_IGNORED: u32 = terrain::GROUNDWATER | terrain::MEADOW | terrain::FOUNTAIN_RANGE | terrain::IRRIGATION_RANGE | terrain::BRIDGE;
/// Terrain that keeps grass off land that is otherwise clear (with `0x20_0000`, a bit no
/// map sets).
const NO_GRASS: u32 = terrain::TREE | terrain::ROCK | terrain::MEADOW | terrain::FLOODPLAIN | terrain::ORE | 0x20_0000 | terrain::DUNE | terrain::CLIFF;

/// Rebuilds every terrain image of `map` and returns, for each tile, the set its image
/// was picked from (`count` 0 for tiles the pass leaves alone). Like the original, it
/// also marks the floodplain's banks as grassland (groundwater) and clears meadow,
/// marshland and dunes from them.
pub fn rebuild(map: &mut Map, defs: &Defs) -> Grid<Choice> {
    rebuild_with_edge_margin(map, defs, EDGE_MARGIN)
}

/// How many tiles from the diamond's edge bare land keeps to single tiles: its blocks
/// may not reach within 3 tiles of the edge (FUN_0047c880 asks FUN_0046f9a0).
pub const EDGE_MARGIN: i32 = 3;

/// `rebuild`, with bare land blocks kept `edge_margin` tiles from the diamond's edge.
/// Some campaign missions and Gateway to Atlantis were saved by an earlier build that
/// laid blocks right up to the edge (margin 0); the checker redraws them that way.
pub fn rebuild_with_edge_margin(map: &mut Map, defs: &Defs, edge_margin: i32) -> Grid<Choice> {
    let mut pass = Pass::new(map, defs);
    pass.edge_margin = edge_margin;
    pass.shrubs_to_trees(map);
    pass.clear(map);
    pass.run(map);
    pass.mark_banks(map);
    pass.run(map);
    pass.clear_outside(map);
    pass.choices
}

/// What a new city does to the map's stored images. Maps the original didn't save
/// (older than version 147, with ids from an earlier sprite layout, or newer than 160,
/// from another editor) have all their terrain drawn again. The rest have their rocks,
/// ore, cliffs and dunes redrawn (the Cleopatra missions store their cliffs with ids
/// from another layout) and lose any image outside the diamond, as the full pass
/// leaves them: the Sandbox maps keep thousands of stale tiles there.
pub fn redraw_on_load(map: &mut Map, defs: &Defs, version: i32) {
    if !(147..=160).contains(&version) {
        rebuild(map, defs);
    } else {
        redraw_outcrops(map, defs);
    }
}

/// Redraws only rocks, ore, cliffs and dunes, as the original does on every start, and
/// clears the tiles outside the diamond.
pub fn redraw_outcrops(map: &mut Map, defs: &Defs) {
    let mut pass = Pass::new(map, defs);
    pass.rocks(map);
    pass.dunes(map);
    pass.clear_outside(map);
}

/// Redraws land freed in play (cleared land, a removed road or building), as the
/// original does after clearing: the rectangle's empty land in full, the grass in a
/// ring 5 tiles wider (grass borders depend on what stands beside them), then the
/// meadow in the rectangle, which the empty-land pass leaves alone.
/// Dry floodplain the change left without an image gets its soil back last: the
/// empty-land pass fills it with bare land blocks like any open ground.
pub fn refresh_land(map: &mut Map, defs: &Defs, x0: i32, y0: i32, x1: i32, y1: i32) {
    let covered = terrain::WATER | terrain::BUILDING | terrain::WALKABLE_BUILDING | terrain::ROAD | terrain::CANAL | terrain::RUBBLE;
    let mut floodplain = Vec::new();
    for y in y0.max(0)..=y1.min(map.height - 1) {
        for x in x0.max(0)..=x1.min(map.width - 1) {
            let t = map.terrain.at_or(x, y, 0);
            if t & terrain::FLOODPLAIN != 0 && t & covered == 0 && map.images.at_or(x, y, 1) == 0 {
                floodplain.push((x, y));
            }
        }
    }
    let mut pass = Pass::new(map, defs);
    pass.empty_land_in(map, (x0, y0, x1, y1), false);
    pass.empty_land_in(map, (x0 - 5, y0 - 5, x1 + 5, y1 + 5), true);
    pass.meadow_in(map, (x0, y0, x1, y1));
    for &(x, y) in &floodplain {
        map.set_single_image(x, y, 0);
    }
    for &(x, y) in &floodplain {
        let image = pass.floodplain_soil(map, x, y) + growth_around(map, defs, x, y);
        map.set_single_image(x, y, image);
    }
}

/// How far the crops on the dry floodplain around `(x, y)` have grown (0-5), read from
/// their images: Osiris doesn't track the floodplain's growth, and a cleared tile should
/// look like the field it sits in. The most common stage wins; 0 with none around.
fn growth_around(map: &Map, defs: &Defs, x: i32, y: i32) -> u32 {
    let base = defs.terrain.floodplain;
    let mut counts = [0u32; 6];
    for (dx, dy) in NEIGHBOURS {
        let (nx, ny) = (x + dx, y + dy);
        let image = map.images.at_or(nx, ny, 0);
        let dry = map.terrain_is(nx, ny, terrain::FLOODPLAIN) && !map.terrain_is(nx, ny, terrain::WATER | terrain::BUILDING | terrain::WALKABLE_BUILDING | terrain::ROAD | terrain::CANAL);
        if dry && (base..base + 48).contains(&image) {
            counts[((image - base) % 6) as usize] += 1;
        }
    }
    (0..6).rev().max_by_key(|&g| counts[g as usize]).filter(|&g| counts[g as usize] > 0).unwrap_or(0)
}

/// Redraws only the grass in a rectangle, as the original does around a new road or
/// ditch: grass beside it takes its bordered look.
pub fn refresh_grass(map: &mut Map, defs: &Defs, x0: i32, y0: i32, x1: i32, y1: i32) {
    let mut pass = Pass::new(map, defs);
    pass.empty_land_in(map, (x0, y0, x1, y1), true);
}

#[derive(Default)]
struct Counters {
    earthquake: Vec<u32>,
    dirt_road: Vec<u32>,
    shore: Vec<u32>,
}

/// What the floodplain's row marking left on a tile.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum Bank {
    #[default]
    None,
    /// A floodplain tile.
    Floodplain,
    /// Land or river beside the floodplain.
    Shore,
}

/// The diamond of the map the city view shows, as the original works it out from the
/// map's size (it centres the map in its 228x228 grid).
struct Diamond {
    x0: i32,
    y0: i32,
    x_last: i32,
    y_last: i32,
    cx: i32,
    cy: i32,
}

impl Diamond {
    const GRID: i32 = 228;

    fn new(width: i32, height: i32) -> Self {
        let x0 = (Self::GRID - width) / 2;
        let y0 = (Self::GRID - height) / 2;
        let x_last = x0 + width - 1;
        let y_last = y0 + height - 1;
        Self { x0, y0, x_last, y_last, cx: (x_last + x0) / 2, cy: (y_last + y0) / 2 }
    }

    fn inside(&self, x: i32, y: i32) -> bool {
        let (gx, gy) = (x + self.x0, y + self.y0);
        if gx > self.cx {
            if gy <= self.cy { gx - gy <= self.x_last - self.cy } else { gx + gy <= self.x_last + 1 + self.cy }
        } else if gy <= self.cy {
            self.x0 + self.cy <= gx + gy
        } else {
            self.cx - self.y_last <= gx - gy
        }
    }

    /// Inside, but within `margin` tiles of the diamond's edge.
    fn near_edge(&self, x: i32, y: i32, margin: i32) -> bool {
        if !self.inside(x, y) {
            return false;
        }
        let (gx, gy) = (x + self.x0, y + self.y0);
        if gx <= self.cx {
            if gy <= self.cy { gx - margin + gy < self.x0 + self.cy } else { gx - gy - margin < self.cx - self.y_last }
        } else if gy <= self.cy {
            self.x_last - self.cy < gx - gy + margin
        } else {
            self.x_last + 1 + self.cy < gx + gy + margin
        }
    }

    /// The tiles of map row `y` inside the diamond.
    fn row(&self, y: i32) -> std::ops::Range<i32> {
        let gy = y + self.y0;
        let start = if gy <= self.cy { self.cy + self.x0 - gy } else { self.x0 - self.cy - 1 + gy };
        (start - self.x0)..(2 * self.cx - start + 2 - self.x0)
    }
}

/// A context-table match: the row's image offset, the picked variant and how many
/// variants the row has.
#[derive(Debug, Clone, Copy)]
struct Context {
    offset: u32,
    item: u32,
    variants: u32,
}

fn row_matches(row: &ContextRow, tiles: [u8; 8]) -> bool {
    row.tiles.iter().zip(tiles).all(|(&want, got)| want == 2 || want == got)
}

/// The first row matching `tiles`; its variant is `pick % variants`, or with no `pick`
/// the row's rotating counter.
fn context(rows: &[ContextRow], counters: &mut Vec<u32>, tiles: [u8; 8], pick: Option<u32>) -> Option<Context> {
    if counters.len() != rows.len() {
        counters.resize(rows.len(), 0);
    }
    let i = rows.iter().position(|r| row_matches(r, tiles))?;
    let row = &rows[i];
    let item = match pick {
        Some(p) => p % row.variants.max(1),
        None => {
            counters[i] += 1;
            if counters[i] >= row.variants {
                counters[i] = 0;
            }
            counters[i]
        }
    };
    Some(Context { offset: row.offsets[0], item, variants: row.variants })
}

struct Pass<'a> {
    defs: &'a Defs,
    diamond: Diamond,
    banks: Grid<Bank>,
    counters: Counters,
    choices: Grid<Choice>,
    edge_margin: i32,
}

impl<'a> Pass<'a> {
    fn new(map: &Map, defs: &'a Defs) -> Self {
        Self {
            defs,
            diamond: Diamond::new(map.width, map.height),
            banks: Grid::new(map.width, map.height),
            counters: Counters::default(),
            choices: Grid::new(map.width, map.height),
            edge_margin: EDGE_MARGIN,
        }
    }

    /// Terrain at `(x, y)`; tiles around the map have the terrain the file gives them.
    fn t(map: &Map, x: i32, y: i32) -> u32 {
        map.terrain_around(x, y, OUTSIDE)
    }

    fn random(map: &Map, x: i32, y: i32) -> u32 {
        map.random.at_or(x, y, 0) as u32
    }

    fn put(&mut self, map: &mut Map, x: i32, y: i32, image: u32) {
        self.put_choice(map, x, y, image, image, 1);
    }

    fn put_choice(&mut self, map: &mut Map, x: i32, y: i32, image: u32, first: u32, count: u32) {
        map.set_single_image(x, y, image);
        self.choices.set(x, y, Choice { first, count, stride: 1 });
    }

    fn put_footprint(&mut self, map: &mut Map, x: i32, y: i32, size: i32, image: u32) {
        map.set_footprint(x, y, size, image);
        for yy in y..y + size {
            for xx in x..x + size {
                self.choices.set(xx, yy, Choice { first: image, count: 1, stride: 1 });
            }
        }
    }

    /// 1 for each neighbour with any of `mask`.
    fn neighbours(map: &Map, x: i32, y: i32, mask: u32) -> [u8; 8] {
        NEIGHBOURS.map(|(dx, dy)| (Self::t(map, x + dx, y + dy) & mask != 0) as u8)
    }

    /// 1 for each neighbour without any of `mask`.
    fn neighbours_without(map: &Map, x: i32, y: i32, mask: u32) -> [u8; 8] {
        Self::neighbours(map, x, y, mask).map(|m| 1 - m)
    }

    fn count(tiles: [u8; 8]) -> usize {
        tiles.iter().filter(|&&m| m != 0).count()
    }

    /// Calls `f` on every tile of the map.
    fn each_tile(&mut self, map: &mut Map, mut f: impl FnMut(&mut Self, &mut Map, i32, i32)) {
        for y in 0..map.height {
            for x in 0..map.width {
                f(self, map, x, y);
            }
        }
    }

    /// Calls `f` on every tile inside the diamond.
    fn each_shown_tile(&mut self, map: &mut Map, mut f: impl FnMut(&mut Self, &mut Map, i32, i32)) {
        for y in 0..map.height {
            for x in self.diamond.row(y) {
                f(self, map, x, y);
            }
        }
    }

    /// For each tile inside the diamond that `want`s it, calls `f` on the 3x3 tiles around.
    fn around_each(&mut self, map: &mut Map, want: impl Fn(&Map, i32, i32) -> bool, f: fn(&mut Self, &mut Map, i32, i32)) {
        for y in 0..map.height {
            for x in 0..map.width {
                if !want(map, x, y) || !self.diamond.inside(x, y) {
                    continue;
                }
                for yy in (y - 1).max(0)..=(y + 1).min(map.height - 1) {
                    for xx in (x - 1).max(0)..=(x + 1).min(map.width - 1) {
                        f(self, map, xx, yy);
                    }
                }
            }
        }
    }

    /// The original turns every shrub inside the diamond into a tree when a map starts.
    fn shrubs_to_trees(&mut self, map: &mut Map) {
        self.each_shown_tile(map, |_, map, x, y| {
            map.terrain.update(x, y, |t| if t & terrain::SHRUB != 0 { t & !terrain::SHRUB | terrain::TREE } else { t });
        });
    }

    /// Tiles outside the diamond are never shown and hold no image, except where a
    /// rock or dune drawn inside reaches over the edge.
    fn clear_outside(&mut self, map: &mut Map) {
        for y in 0..map.height {
            for x in 0..map.width {
                if !self.diamond.inside(x, y) && Self::t(map, x, y) & KEEP == 0 && map.bitfields.at_or(x, y, 0) & 0x0f == 0 {
                    map.images.set(x, y, 0);
                    self.choices.set(x, y, Choice::default());
                }
            }
        }
    }

    /// Forgets every image the pass draws.
    fn clear(&mut self, map: &mut Map) {
        for y in 0..map.height {
            for x in 0..map.width {
                if Self::t(map, x, y) & KEEP == 0 {
                    map.set_single_image(x, y, 0);
                }
            }
        }
    }

    fn run(&mut self, map: &mut Map) {
        let t = Self::t;
        self.around_each(map, |m, x, y| t(m, x, y) & terrain::ROCK != 0 && m.bitfields.at_or(x, y, 0) & 0x80 != 0, Self::earthquake);
        self.rocks(map);
        self.around_each(map, |m, x, y| is_tree(t(m, x, y)), Self::tree_at);
        self.gardens(map);
        self.empty_land(map);
        self.around_each(map, |m, x, y| t(m, x, y) & terrain::FLOODPLAIN != 0 && t(m, x, y) & terrain::BUILDING == 0, Self::floodplain);
        self.around_each(map, |m, x, y| meadow_ok(t(m, x, y)) && t(m, x, y) & terrain::RUBBLE == 0, Self::meadow);
        self.around_each(map, |m, x, y| t(m, x, y) & terrain::MARSHLAND != 0 && t(m, x, y) & terrain::BUILDING == 0, Self::marshland);
        self.dunes(map);
        self.around_each(map, |m, x, y| t(m, x, y) & terrain::FLOODPLAIN != 0 && t(m, x, y) & terrain::BUILDING == 0, Self::floodplain);
        self.around_each(map, |m, x, y| t(m, x, y) & terrain::WATER != 0 && t(m, x, y) & terrain::BUILDING == 0, Self::water);
        self.each_tile(map, Self::rubble);
        self.each_tile(map, Self::road);
        self.plazas(map);
    }

    /// Marks the floodplain's rows out from the river, the way the original does before
    /// its second pass: every floodplain tile and every tile beside one that the rows
    /// reach becomes grassland and loses meadow, marshland and dunes.
    fn mark_banks(&mut self, map: &mut Map) {
        let (w, h) = (map.width, map.height);
        let mut rows: Grid<u8> = Grid::new(w, h);
        let mut round = 30u8;
        loop {
            let mut changed = false;
            for y in 0..h {
                for x in 0..w {
                    if rows.at_or(x, y, 0) != 0 {
                        continue;
                    }
                    let t = Self::t(map, x, y);
                    let beside = Self::count(Self::neighbours(map, x, y, terrain::FLOODPLAIN));
                    if t & terrain::FLOODPLAIN == 0 && beside == 0 {
                        continue;
                    }
                    let open_water = NEIGHBOURS
                        .iter()
                        .filter(|&&(dx, dy)| {
                            let n = Self::t(map, x + dx, y + dy);
                            n & terrain::WATER != 0 && n & (terrain::FLOODPLAIN | terrain::DIKE) == 0
                        })
                        .count();
                    if open_water + beside == 8 && t & (terrain::FLOODPLAIN | terrain::WATER) == terrain::WATER {
                        continue;
                    }
                    let mut row = 30u8;
                    if open_water == 0 {
                        row = 0;
                        for (dx, dy) in NEIGHBOURS {
                            let n = rows.at_or(x + dx, y + dy, 0);
                            if Self::t(map, x + dx, y + dy) & terrain::FLOODPLAIN != 0 && row < n && (n == 1 || n != round) {
                                row = (n - 1).max(1);
                            }
                        }
                        if row == 0 {
                            continue;
                        }
                    }
                    map.terrain.set(x, y, t & !(terrain::DUNE | terrain::MARSHLAND | terrain::MEADOW) | terrain::GROUNDWATER);
                    rows.set(x, y, row);
                    self.banks.set(x, y, if t & terrain::FLOODPLAIN != 0 { Bank::Floodplain } else { Bank::Shore });
                    changed = true;
                }
            }
            if !changed {
                break;
            }
            round = round.saturating_sub(1);
        }
    }

    fn earthquake(&mut self, map: &mut Map, x: i32, y: i32) {
        let marked = |map: &Map, x: i32, y: i32| Self::t(map, x, y) & terrain::ROCK != 0 && map.bitfields.at_or(x, y, 0) & 0x80 != 0;
        if !marked(map, x, y) {
            return;
        }
        let tiles = NEIGHBOURS.map(|(dx, dy)| marked(map, x + dx, y + dy) as u8);
        let base = self.defs.terrain.earthquake;
        match context(&self.defs.contexts.earthquake, &mut self.counters.earthquake, tiles, None) {
            Some(c) => self.put_choice(map, x, y, base + c.offset + c.item, base + c.offset, c.variants),
            None => self.put(map, x, y, base),
        }
    }

    /// `size x size` tiles at `(x, y)` fit the map, have no image yet and hold exactly
    /// `kind` (grassland, meadow and the like aside).
    fn block_of(map: &Map, x: i32, y: i32, size: i32, kind: u32) -> bool {
        if x < 0 || y < 0 || x + size > map.width || y + size > map.height {
            return false;
        }
        (y..y + size).all(|yy| (x..x + size).all(|xx| Self::t(map, xx, yy) & !CLEAR_IGNORED & !terrain::FLOODPLAIN == kind && map.images.at_or(xx, yy, 1) == 0))
    }

    /// Picks the biggest outcrop (3x3, 2x2 or a single tile) of `kind` at `(x, y)`.
    fn outcrop(&mut self, map: &mut Map, x: i32, y: i32, kind: u32, base: u32) {
        let r = Self::random(map, x, y);
        if Self::block_of(map, x, y, 3, kind) {
            self.put_footprint(map, x, y, 3, base + 12 + (r & 1));
        } else if Self::block_of(map, x, y, 2, kind) {
            self.put_footprint(map, x, y, 2, base + 8 + (r & 3));
        } else {
            self.put(map, x, y, base + (r & 7));
        }
    }

    fn redrawn_outcrop(map: &Map, x: i32, y: i32, kind: u32) -> bool {
        Self::t(map, x, y) & kind != 0 && map.bitfields.at_or(x, y, 0) & 0x80 == 0 && Self::t(map, x, y) & (terrain::ELEVATION | terrain::ACCESS_RAMP) == 0
    }

    fn rocks(&mut self, map: &mut Map) {
        self.each_shown_tile(map, |_, map, x, y| {
            if Self::redrawn_outcrop(map, x, y, terrain::ROCK) {
                map.set_single_image(x, y, 0);
            }
        });
        self.each_shown_tile(map, |p, map, x, y| {
            if !Self::redrawn_outcrop(map, x, y, terrain::ROCK) || map.images.at_or(x, y, 1) != 0 {
                return;
            }
            let t = Self::t(map, x, y);
            let ore = terrain::ORE | terrain::ROCK;
            let cliff = terrain::CLIFF | terrain::ROCK;
            if t & ore == ore {
                p.outcrop(map, x, y, ore, p.defs.terrain.ore_rock);
            } else if t & cliff == cliff {
                let r = Self::random(map, x, y);
                let image = match cliff_offset(map, x, y, r) {
                    Some(offset) => p.defs.terrain.cliff + offset,
                    None => p.defs.terrain.rock + (r & 7),
                };
                p.put(map, x, y, image);
            } else {
                p.outcrop(map, x, y, terrain::ROCK, p.defs.terrain.rock);
            }
        });
    }

    fn dunes(&mut self, map: &mut Map) {
        self.each_shown_tile(map, |_, map, x, y| {
            if Self::redrawn_outcrop(map, x, y, terrain::DUNE) {
                map.set_single_image(x, y, 0);
            }
        });
        self.each_shown_tile(map, |p, map, x, y| {
            if Self::redrawn_outcrop(map, x, y, terrain::DUNE) && map.images.at_or(x, y, 1) == 0 {
                p.outcrop(map, x, y, terrain::DUNE, p.defs.terrain.dune);
            }
        });
    }

    fn gardens(&mut self, map: &mut Map) {
        let is_garden = |map: &Map, x, y| Self::t(map, x, y) & terrain::GARDEN != 0 && Self::t(map, x, y) & (terrain::ELEVATION | terrain::ACCESS_RAMP) == 0;
        self.each_shown_tile(map, |_, map, x, y| {
            if is_garden(map, x, y) {
                map.set_single_image(x, y, 0);
            }
        });
        self.each_shown_tile(map, |p, map, x, y| {
            if !is_garden(map, x, y) || map.images.at_or(x, y, 1) != 0 {
                return;
            }
            let base = p.defs.terrain.garden;
            let r = Self::random(map, x, y);
            if Self::block_of(map, x, y, 3, terrain::GARDEN) {
                p.put_footprint(map, x, y, 3, base + 7);
            } else if Self::block_of(map, x, y, 2, terrain::GARDEN) {
                p.put_footprint(map, x, y, 2, base + [6, 6, 5, 4][(r & 3) as usize]);
            } else {
                let pattern = if y & 1 == 0 { [0, 1, 0, 1] } else { [2, 3, 2, 3] };
                p.put(map, x, y, base + pattern[(x & 3) as usize]);
            }
        });
    }

    /// Grass level from the moisture byte (0-12), or 0x80 plus the edge image of land
    /// where grass meets bare ground.
    fn grass(map: &Map, x: i32, y: i32) -> u32 {
        let v = map.moisture.at_or(x, y, 0) as u32;
        if v <= 100 {
            v / 8
        } else if v & 8 == 0 {
            v & 7 | 0x80
        } else {
            ((v & 3) + 8) | 0x80
        }
    }

    /// Any neighbour holds something other than clear land.
    fn beside_anything(map: &Map, x: i32, y: i32) -> bool {
        NEIGHBOURS.iter().any(|&(dx, dy)| Self::t(map, x + dx, y + dy) & !CLEAR_IGNORED != 0)
    }

    /// Calls `f` on every tile of the rectangle, clipped to the map.
    fn each_in(&mut self, map: &mut Map, (x0, y0, x1, y1): (i32, i32, i32, i32), mut f: impl FnMut(&mut Self, &mut Map, i32, i32)) {
        for y in y0.max(0)..=y1.min(map.height - 1) {
            for x in x0.max(0)..=x1.min(map.width - 1) {
                f(self, map, x, y);
            }
        }
    }

    /// Cleared land, grass, and the bare land blocks that fill the rest.
    fn empty_land(&mut self, map: &mut Map) {
        self.empty_land_in(map, (0, 0, map.width - 1, map.height - 1), false);
    }

    /// Cleared land in a rectangle: grassland (groundwater) takes its grass image, other
    /// clear land loses its image and gets bare land, singles first where the random grid
    /// says so, then the biggest blocks that fit. With `grass_only` only the grass is
    /// redrawn, as the original does in a wider ring around a change. Meadow keeps its
    /// image here; `meadow_in` redraws it.
    fn empty_land_in(&mut self, map: &mut Map, rect: (i32, i32, i32, i32), grass_only: bool) {
        self.each_in(map, rect, |p, map, x, y| {
            let t = Self::t(map, x, y);
            let clear = if p.banks.at_or(x, y, Bank::None) == Bank::Shore { t & terrain::BUILDING == 0 } else { t & !CLEAR_IGNORED == 0 };
            if !clear {
                return;
            }
            if t & terrain::GROUNDWATER == 0 {
                if !grass_only {
                    map.set_single_image(x, y, 0);
                }
            } else if t & NO_GRASS == 0 {
                let image = p.grass_image(map, x, y);
                p.put(map, x, y, image);
            }
        });
        if grass_only {
            return;
        }
        let bare = |map: &Map, x, y| Self::t(map, x, y) & !CLEAR_IGNORED & !terrain::FLOODPLAIN == 0 && map.images.at_or(x, y, 1) == 0;
        let base = |p: &Self, map: &Map, x, y| if map.bitfields.at_or(x, y, 0) & 0x20 != 0 { p.defs.terrain.empty_land_alt } else { p.defs.terrain.empty_land };
        self.each_in(map, rect, |p, map, x, y| {
            if bare(map, x, y) && Self::random(map, x, y) & 0xf0 == 0 {
                let image = base(p, map, x, y) + (Self::random(map, x, y) & 7);
                p.put(map, x, y, image);
            }
        });
        self.each_in(map, rect, |p, map, x, y| {
            if !bare(map, x, y) {
                return;
            }
            let base = base(p, map, x, y);
            let r = Self::random(map, x, y);
            let (size, image) = if p.land_block(map, x, y, 4) {
                (4, base + 42)
            } else if p.land_block(map, x, y, 3) {
                (3, base + 24 + 9 * (r & 1))
            } else if p.land_block(map, x, y, 2) {
                (2, base + 8 + 4 * (r & 3))
            } else {
                (1, base + (r & 7))
            };
            let mut index = 0;
            for yy in y..y + size {
                for xx in x..x + size {
                    p.put(map, xx, yy, image + index);
                    index += 1;
                }
            }
        });
    }

    /// Grassland: the bank of the floodplain beside it, else grass by its moisture, and
    /// full grass by how much grass and open land lies around it.
    fn grass_image(&mut self, map: &Map, x: i32, y: i32) -> u32 {
        let t = &self.defs.terrain;
        let beside_floodplain = Self::neighbours(map, x, y, terrain::FLOODPLAIN);
        if Self::count(beside_floodplain) > 0 {
            let offset = context(&self.defs.contexts.shore, &mut self.counters.shore, beside_floodplain, None).map_or(0, |c| c.offset) + 48;
            let offset = if Self::t(map, x, y) & terrain::ROAD != 0 { road_beside_floodplain(offset) } else { offset };
            return t.floodplain + offset;
        }
        let g = Self::grass(map, x, y);
        let r = Self::random(map, x, y);
        if g < 11 {
            t.grass + g + 12 * (r % 3)
        } else if g >= 0x80 {
            t.grass_edges + (g & 0x7f)
        } else if Self::beside_anything(map, x, y) {
            t.grass + 36 + r % 12
        } else {
            let all_full = NEIGHBOURS.iter().all(|&(dx, dy)| {
                let (nx, ny) = (x + dx, y + dy);
                let v = map.moisture.at_or(nx, ny, 0).min(100) as u32;
                v / 8 == 11 && Self::t(map, nx, ny) & (terrain::FLOODPLAIN | terrain::MEADOW | terrain::TREE) == 0 && !Self::beside_anything(map, nx, ny)
            });
            t.grass + if all_full { 48 } else { 60 } + r % 12
        }
    }

    /// `size x size` bare tiles at `(x, y)`, at least 3 tiles from the map's edges and
    /// the diamond's, all on the floodplain or all off it, with no image yet.
    fn land_block(&self, map: &Map, x: i32, y: i32, size: i32) -> bool {
        if x < 3 || y < 3 || x + size > map.width - 3 || y + size > map.height - 3 {
            return false;
        }
        let mut on_floodplain = None;
        for yy in y..y + size {
            for xx in x..x + size {
                let t = Self::t(map, xx, yy);
                if self.diamond.near_edge(xx, yy, self.edge_margin) || self.banks.at_or(xx, yy, Bank::None) == Bank::Shore {
                    return false;
                }
                let fp = t & terrain::FLOODPLAIN != 0;
                if *on_floodplain.get_or_insert(fp) != fp {
                    return false;
                }
                let allowed = terrain::GROUNDWATER | terrain::MEADOW | terrain::FOUNTAIN_RANGE | terrain::FLOODPLAIN | terrain::IRRIGATION_RANGE;
                if t & 0x1001_ffff & !allowed != 0 || map.images.at_or(xx, yy, 1) != 0 {
                    return false;
                }
            }
        }
        true
    }

    /// Dry floodplain: its soil's fertility, and one of two looks per soil.
    fn floodplain(&mut self, map: &mut Map, x: i32, y: i32) {
        let t = Self::t(map, x, y);
        if t & terrain::FLOODPLAIN == 0 || t & (terrain::BUILDING | terrain::ROAD | terrain::CANAL) != 0 {
            return;
        }
        let base = self.defs.terrain.floodplain;
        if self.banks.at_or(x, y, Bank::None) == Bank::None {
            self.put(map, x, y, base);
            return;
        }
        let first = self.floodplain_soil(map, x, y);
        // Crops growing on the floodplain (0-5) are not in the map; a new city starts at 0.
        self.put_choice(map, x, y, first, first, 6);
    }

    /// The first image of a dry floodplain tile's soil: by its fertility, and one of two
    /// looks per soil.
    fn floodplain_soil(&self, map: &Map, x: i32, y: i32) -> u32 {
        let fertility = map.fertility.at_or(x, y, 0) as f64;
        let soil = ((fertility * 0.01 * 8.0) as u32 & !1) | Self::random(map, x, y) & 1;
        self.defs.terrain.floodplain + soil * 6
    }

    /// Every meadow tile in the rectangle redraws itself and the meadow around it.
    fn meadow_in(&mut self, map: &mut Map, rect: (i32, i32, i32, i32)) {
        self.each_in(map, rect, |p, map, x, y| {
            let t = Self::t(map, x, y);
            if !meadow_ok(t) || t & terrain::RUBBLE != 0 || !p.diamond.inside(x, y) {
                return;
            }
            for yy in (y - 1).max(0)..=(y + 1).min(map.height - 1) {
                for xx in (x - 1).max(0)..=(x + 1).min(map.width - 1) {
                    p.meadow(map, xx, yy);
                }
            }
        });
    }

    /// Meadow by its soil's fertility and the grass on it.
    fn meadow(&mut self, map: &mut Map, x: i32, y: i32) {
        let t = Self::t(map, x, y);
        if !meadow_ok(t) || t & terrain::RUBBLE != 0 {
            return;
        }
        let terrain_images = &self.defs.terrain;
        let density = ((map.fertility.at_or(x, y, 0) as f64 - 1.0) * (1.0 / 99.0) * 3.0) as i32;
        let grass = ((map.moisture.at_or(x, y, 0).min(100) as f64 - 1.0) / 96.0 * 12.0) as i32;
        let r = Self::random(map, x, y) & 7;
        let (base, density, offset) = match (density, grass) {
            (0, 0) => (terrain_images.meadow_outer, 0, r),
            (2, 0) => (terrain_images.meadow_inner, 0, r),
            (2, 11) => (terrain_images.meadow_tallgrass, 0, r),
            (d, g) => (terrain_images.meadow_with_grass, d, g as u32),
        };
        let image = (base as i32 + density * 12) as u32 + offset;
        self.put(map, x, y, image);
    }

    /// Marsh edges follow the water table; the middle of a marsh is full reeds.
    fn marshland(&mut self, map: &mut Map, x: i32, y: i32) {
        let t = Self::t(map, x, y);
        if t & terrain::MARSHLAND == 0 || t & terrain::BUILDING != 0 {
            return;
        }
        let tiles = Self::neighbours_without(map, x, y, terrain::MARSHLAND);
        let r = Self::random(map, x, y);
        let c = context(&self.defs.contexts.water, &mut Vec::new(), tiles, Some(r));
        let image = self.defs.terrain.reeds + c.map_or(0, |c| c.offset + c.item);
        self.put(map, x, y, image);
        self.tree(map, x, y);
    }

    fn tree_at(&mut self, map: &mut Map, x: i32, y: i32) {
        if is_tree(Self::t(map, x, y)) {
            self.tree(map, x, y);
        }
    }

    /// Trees by how dense the wood around them is, and reeds in the middle of a marsh.
    fn tree(&mut self, map: &mut Map, x: i32, y: i32) {
        let t = Self::t(map, x, y);
        let r = Self::random(map, x, y) & 7;
        let terrain_images = &self.defs.terrain;
        if t & terrain::TREE == 0 || t & terrain::DIKE != 0 {
            if t & terrain::MARSHLAND != 0 && t & terrain::DIKE == 0 && Self::count(Self::neighbours(map, x, y, terrain::MARSHLAND)) == 8 {
                let base = if map.vegetation.at_or(x, y, 0) == 255 { terrain_images.reeds_grown } else { terrain_images.reeds };
                self.put(map, x, y, base + r);
            }
            return;
        }
        let moisture = map.moisture.at_or(x, y, 0).min(100) as u32;
        let image = if map.vegetation.at_or(x, y, 0) != 255 {
            terrain_images.young_tree + r + moisture / 34 * 8
        } else {
            let wood = (1..=3u32).rev().find(|&ring| only_rocks_trees_in_ring(map, x, y, ring as i32)).unwrap_or(0);
            if wood < 2 {
                terrain_images.tree + r + wood * 8 + moisture / 34 * 16
            } else {
                terrain_images.tree + r + 48 + (wood - 2) * 8
            }
        };
        self.put(map, x, y, image);
    }

    fn water(&mut self, map: &mut Map, x: i32, y: i32) {
        let t = Self::t(map, x, y);
        if t & terrain::WATER == 0 || t & terrain::BUILDING != 0 {
            return;
        }
        let terrain_images = &self.defs.terrain;
        let r = Self::random(map, x, y);
        let tiles = Self::neighbours_without(map, x, y, terrain::WATER);
        let (offset, item) = context(&self.defs.contexts.water, &mut Vec::new(), tiles, Some(r)).map_or((0, 0), |c| (c.offset, c.item));
        // Flooded floodplain takes the flood water set, except in open water.
        let image = if t & terrain::FLOODPLAIN != 0 {
            (offset >= 8).then_some(terrain_images.flood_water + item + offset)
        } else {
            let floodplain = Self::neighbours(map, x, y, terrain::FLOODPLAIN);
            (Self::count(floodplain) > 0).then(|| self.water_beside_floodplain(x, y, offset, item, floodplain))
        };
        // Open water, shores, and deep water: its variant picks one of six 15-frame loops.
        let image = match image {
            Some(image) => image,
            None if t & terrain::DEEPWATER == 0 => {
                if offset == 0 { terrain_images.flood_water + item } else { terrain_images.water + item + offset }
            }
            None => self.deep_water(map, x, y, r),
        };
        match animated_water(self.defs, image) {
            Some(c) => {
                map.set_single_image(x, y, image);
                self.choices.set(x, y, c);
            }
            None => self.put(map, x, y, image),
        }
    }

    /// Deep water by the deep water around it. The original (FUN_0047a220) searches the
    /// shore table from its last row and picks the row's variant by the random grid,
    /// or where that is 0 by the row's rotating counter, and first resets every counter
    /// of the table (FUN_00484ee0): grass beside the floodplain picks from the same
    /// table and counters, so each deep water tile restarts their rotation.
    fn deep_water(&mut self, map: &Map, x: i32, y: i32, r: u32) -> u32 {
        let base = self.defs.terrain.deepwater;
        let rows = &self.defs.contexts.shore;
        self.counters.shore.clear();
        self.counters.shore.resize(rows.len(), 0);
        let tiles = Self::neighbours_without(map, x, y, terrain::DEEPWATER);
        let Some(i) = rows.iter().rposition(|row| row_matches(row, tiles)) else { return base };
        let v = rows[i].variants.max(1);
        let item = if r != 0 {
            r % v
        } else {
            self.counters.shore[i] = if 1 >= v { 0 } else { 1 };
            self.counters.shore[i]
        };
        base + item * 15 + rows[i].offsets[0]
    }

    /// River water beside the floodplain (`floodplain` marks those neighbours): where a
    /// marked bank lies on the side the water table picked (`offset`), the water meets
    /// the floodplain's edge; elsewhere it takes the flood water set.
    fn water_beside_floodplain(&self, x: i32, y: i32, offset: u32, item: u32, floodplain: [u8; 8]) -> u32 {
        let shore: [bool; 8] = std::array::from_fn(|i| {
            let (dx, dy) = NEIGHBOURS[i];
            self.banks.at_or(x + dx, y + dy, Bank::None) == Bank::Shore
        });
        let mut size = Self::count(floodplain).min(2) as u32 * 2;
        let mut widen = false;
        let side = match offset {
            8 => shore[4].then_some(4),
            12 => shore[6].then_some(6),
            16 => shore[0].then_some(0),
            20 => shore[2].then_some(2),
            24 | 36 => {
                widen = true;
                if offset == 24 && shore[6] {
                    Some(6)
                } else if offset == 36 && shore[2] {
                    Some(2)
                } else {
                    shore[4].then_some(4)
                }
            }
            28 | 32 => {
                widen = true;
                if offset == 28 && shore[6] {
                    Some(6)
                } else if offset == 32 && shore[2] {
                    Some(2)
                } else {
                    shore[0].then_some(0)
                }
            }
            _ => None,
        };
        let Some(side) = side else {
            return self.defs.terrain.flood_water + item + offset;
        };
        if !widen {
            size = 0;
        } else if size == 2 && [0, 2, 4, 6].iter().any(|&i| floodplain[i] != 0) {
            size = 4;
        }
        let mut image = self.defs.terrain.floodplain + ((side as u32 + 4) % 8 + 20) * 3 + size;
        if floodplain[(side + 1) % 8] != 0 || floodplain[(side + 2) % 8] != 0 {
            image += 1;
        }
        image
    }

    fn rubble(&mut self, map: &mut Map, x: i32, y: i32) {
        let t = Self::t(map, x, y);
        let forbidden = terrain::CANAL | terrain::ELEVATION | terrain::ACCESS_RAMP | terrain::BUILDING | terrain::ROAD | terrain::GARDEN;
        if t & terrain::RUBBLE != 0 && t & forbidden == 0 && self.diamond.inside(x, y) {
            let image = self.defs.terrain.rubble + (Self::random(map, x, y) & 7);
            self.put(map, x, y, image);
        }
    }

    /// Dirt roads (a new map has no desirability, so none are paved), and roads on the
    /// floodplain's banks.
    fn road(&mut self, map: &mut Map, x: i32, y: i32) {
        let t = Self::t(map, x, y);
        if t & terrain::ROAD == 0 || t & (terrain::WATER | terrain::BUILDING | terrain::CANAL) != 0 || map.bitfields.at_or(x, y, 0) & 0x80 != 0 {
            return;
        }
        let tiles = Self::neighbours(map, x, y, terrain::ROAD | terrain::SUBMERGED_ROAD);
        let c = context(&self.defs.contexts.dirt_road, &mut self.counters.dirt_road, tiles, None);
        let offset = c.map_or(0, |c| c.offset + c.item);
        let terrain_images = &self.defs.terrain;
        let image = if t & terrain::FLOODPLAIN == 0 && self.banks.at_or(x, y, Bank::None) != Bank::Shore {
            terrain_images.dirt_road + offset
        } else {
            let current = map.images.at_or(x, y, 0) as i64 - terrain_images.floodplain as i64;
            let edge = road_beside_floodplain_i(current);
            if edge == current && !(84..=87).contains(&current) {
                terrain_images.floodplain_road + offset
            } else {
                (terrain_images.floodplain as i64 + edge) as u32
            }
        };
        self.put(map, x, y, image);
    }

    fn plazas(&mut self, map: &mut Map) {
        let plaza = |map: &Map, x, y| Self::t(map, x, y) & terrain::ROAD != 0 && map.bitfields.at_or(x, y, 0) & 0x80 != 0;
        self.each_tile(map, |_, map, x, y| {
            if plaza(map, x, y) && map.building.at_or(x, y, 0) == 0 {
                map.set_single_image(x, y, 0);
            }
        });
        let base = self.defs.terrain.plaza;
        self.each_tile(map, |p, map, x, y| {
            if !plaza(map, x, y) || map.images.at_or(x, y, 1) != 0 {
                return;
            }
            let score: i32 = [(1, 0), (0, 1), (1, 1)]
                .iter()
                .map(|&(dx, dy)| {
                    let (nx, ny) = (x + dx, y + dy);
                    let n = Self::t(map, nx, ny);
                    (n & terrain::ROAD != 0) as i32
                        - (map.bitfields.at_or(nx, ny, 0) & 0x80 == 0) as i32
                        - (n & terrain::WATER != 0) as i32
                        - (n & terrain::BUILDING != 0) as i32
                        - (n & terrain::CANAL != 0) as i32
                        - (map.images.at_or(nx, ny, 0) != 0) as i32
                })
                .sum();
            if score == 3 {
                let image = base + 6 + (Self::random(map, x, y) & 1);
                p.put_footprint(map, x, y, 2, image);
            } else {
                let image = base + match (x & 1, y & 1) {
                    (0, 0) => 0,
                    (1, 1) => 1,
                    _ => 2,
                };
                p.put(map, x, y, image);
            }
        });
    }

}

fn is_tree(t: u32) -> bool {
    t & terrain::TREE != 0 && t & (terrain::ELEVATION | terrain::ACCESS_RAMP) == 0
}

fn meadow_ok(t: u32) -> bool {
    t & terrain::MEADOW != 0
        && t & (terrain::CANAL | terrain::ELEVATION | terrain::ACCESS_RAMP) == 0
        && t & terrain::ROAD == 0
        && (t & terrain::BUILDING == 0 || t & terrain::BRIDGE != 0)
        && t & terrain::WALKABLE_BUILDING == 0
        && t & terrain::GARDEN == 0
}

/// A road on the floodplain's straight banks takes the bank's road piece.
fn road_beside_floodplain(offset: u32) -> u32 {
    road_beside_floodplain_i(offset as i64) as u32
}

fn road_beside_floodplain_i(offset: i64) -> i64 {
    match offset {
        0x30 => 0x54,
        0x32 => 0x55,
        0x34 => 0x56,
        0x36 => 0x57,
        o => o,
    }
}

/// Every tile of the ring `distance` tiles out from `(x, y)` that lies within a tile of
/// the map is rock or tree.
fn only_rocks_trees_in_ring(map: &Map, x: i32, y: i32, distance: i32) -> bool {
    for yy in y - distance..=y + distance {
        for xx in x - distance..=x + distance {
            if (xx - x).abs().max((yy - y).abs()) != distance || xx < -1 || yy < -1 || xx > map.width || yy > map.height {
                continue;
            }
            if map.terrain_around(xx, yy, OUTSIDE) & (terrain::ROCK | terrain::TREE) == 0 {
                return false;
            }
        }
    }
    true
}

/// The image of cliff tile `(x, y)`, as the original draws it on every start (a royal
/// tomb, sealed or removed, goes back to it).
pub(crate) fn cliff_image(map: &Map, defs: &Defs, x: i32, y: i32) -> u32 {
    let r = map.random.at_or(x, y, 0) as u32;
    cliff_offset(map, x, y, r).map_or(defs.terrain.rock + (r & 7), |o| defs.terrain.cliff + o)
}

/// Cliff images at these offsets from the cliff group are the flat tops of a plateau:
/// the city view draws them with the tall images, 150 pixels up, level with the tops
/// of the cliff columns around them (FUN_00439f10), not with the ground.
pub const CLIFF_TOPS: std::ops::Range<u32> = 48..72;

/// How far above its tile the city view draws a plateau top (`CLIFF_TOPS`).
pub const CLIFF_TOP_RISE: i32 = 150;

/// Which cliff image a cliff tile takes from the cliffs around it (orientation 0), or
/// `None` for a lone outcrop, drawn as plain rock (FUN_00475fd0). The columns at
/// offsets 9..12, 15..18 and 36..48 turn their face away from the camera: only their
/// top, their foot and a 1 px outline are drawn, and the raised plateau tops in front
/// of them hide the outline.
fn cliff_offset(map: &Map, x: i32, y: i32, r: u32) -> Option<u32> {
    let cliff = terrain::CLIFF | terrain::ROCK;
    let m: [bool; 8] = std::array::from_fn(|i| {
        let (dx, dy) = NEIGHBOURS[i];
        let t = map.terrain_around(x + dx, y + dy, OUTSIDE);
        t & cliff == cliff && t & terrain::DIKE == 0
    });
    let n = m.iter().filter(|&&c| c).count();
    let (r3, r6) = (r % 3, r % 6);
    match n {
        8 => return Some(48 + r % 24),
        7 => {
            for (i, base) in [(1, 12), (7, 15), (5, 18), (3, 21)] {
                if !m[i] {
                    return Some(base + r3);
                }
            }
        }
        6 => {
            if !m[7] && !m[3] {
                return Some(72 + r3);
            }
            if !m[1] && !m[5] {
                return Some(75 + r3);
            }
        }
        _ => {}
    }
    let all = |idx: &[usize]| idx.iter().all(|&i| m[i]);
    if all(&[6, 7, 0, 1, 2]) {
        return Some(24 + r6);
    }
    if all(&[4, 5, 6, 7, 0]) {
        return Some(30 + r6);
    }
    if all(&[2, 3, 4, 5, 6]) {
        return Some(36 + r6);
    }
    if all(&[0, 1]) {
        if all(&[2, 3, 4]) {
            return Some(42 + r6);
        }
        if m[2] {
            return Some(r3);
        }
    }
    if all(&[6, 7, 0]) {
        return Some(3 + r3);
    }
    if all(&[4, 5, 6]) {
        return Some(6 + r3);
    }
    if all(&[2, 3, 4]) {
        return Some(9 + r3);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use osiris_formats::{ImageLibrary, Scenario};
    use std::path::PathBuf;

    fn pharaoh_data_dir() -> Option<PathBuf> {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../PharaohData");
        dir.is_dir().then_some(dir)
    }

    fn load(data: &std::path::Path, map: &str) -> (Scenario, Map, Defs, ImageLibrary) {
        let library = ImageLibrary::open(&data.join("Data")).expect("open image library");
        let defs = Defs::load(&library).expect("load defs");
        let scenario = Scenario::load_map(&data.join("Maps").join(map)).expect("load map");
        let map = Map::from_scenario(&scenario);
        (scenario, map, defs, library)
    }

    #[test]
    fn diamond_rows_hold_the_shown_tiles() {
        let d = Diamond::new(140, 140);
        assert_eq!(d.row(0), 69..71);
        for y in 0..140 {
            for x in 0..140 {
                assert_eq!(d.inside(x, y), d.row(y).contains(&x), "{x},{y}");
            }
        }
    }

    #[test]
    fn redraws_a_map_as_the_editor_saved_it() {
        let Some(data) = pharaoh_data_dir() else { return };
        let (_, stored, defs, _) = load(&data, "Toshka.map");
        let mut map = stored.clone();
        rebuild(&mut map, &defs);
        let diamond = Diamond::new(map.width, map.height);
        for y in 0..map.height {
            for x in 0..map.width {
                if diamond.inside(x, y) && stored.terrain.at_or(x, y, 0) & KEEP == 0 {
                    assert_eq!(map.images.at_or(x, y, 0), stored.images.at_or(x, y, 0), "{x},{y}");
                }
            }
        }
    }

    #[test]
    fn stale_tiles_outside_the_diamond_are_cleared() {
        let Some(data) = pharaoh_data_dir() else { return };
        let (scenario, mut map, defs, _) = load(&data, "Sandbox.map");
        let diamond = Diamond::new(map.width, map.height);
        let outside = |map: &Map| (0..map.height).flat_map(|y| (0..map.width).map(move |x| (x, y))).filter(|&(x, y)| !diamond.inside(x, y) && map.images.at_or(x, y, 0) != 0).count();
        assert!(outside(&map) > 4000);
        redraw_on_load(&mut map, &defs, scenario.version);
        assert_eq!(outside(&map), 0);
    }

    #[test]
    fn old_maps_get_terrain_images() {
        let Some(data) = pharaoh_data_dir() else { return };
        let (scenario, mut map, defs, library) = load(&data, "Bridges.map");
        assert!(scenario.version < 147);
        redraw_on_load(&mut map, &defs, scenario.version);
        let (terrain_pack, _) = library.pack_by_name("Pharaoh_Terrain").expect("terrain pack");
        let mut drawn = 0;
        for y in 0..map.height {
            for x in 0..map.width {
                let id = map.images.at_or(x, y, 0);
                if map.edges.at_or(x, y, 0) & 0x40 != 0 && id != 0 {
                    drawn += 1;
                    assert_eq!(library.resolve(id).map(|i| i.pack), Some(terrain_pack), "{x},{y} id {id}");
                }
            }
        }
        assert!(drawn > 9000);
    }
}
