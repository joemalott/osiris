//! The editor's terrain brushes and what the original works out again after a stroke:
//! the floodplain's shape, grassland (moisture), deep water and the meadow's soil.
//! See notes/editor.md for the functions of the original each part follows.
//!
//! Tiles outside the map read as the 228x228 grid around it holds them: river and
//! trees with the outside bit (`DIKE`), `0x80005`.

use osiris_sim::map::{Map, NEIGHBOURS, terrain};

/// Terrain of the grid around a map.
pub const OUTSIDE: u32 = terrain::DIKE | terrain::WATER | terrain::TREE;

/// Terrain that may not lie beside the floodplain: trees, rock, scrub, gardens,
/// raised land, meadow, rubble, marshland, ore, dunes and cliffs (`0x22341a33`).
const BLOCKS_FLOODPLAIN: u32 = 0x2234_1a33;

/// The brushes (text 48/0-4): every tile within `size` steps, `|dx| + |dy| <= size`,
/// 1, 5, 13, 25 or 41 tiles, in the order of the original's table (0x5cc778).
pub fn brush(size: u8) -> Vec<(i32, i32)> {
    let mut v = vec![(0, 0)];
    for r in 1..=size as i32 {
        // Clockwise from north, as the table lists each ring.
        for i in 0..4 * r {
            let (q, k) = (i / r, i % r);
            v.push(match q {
                0 => (k, -r + k),
                1 => (r - k, k),
                2 => (-k, r - k),
                _ => (-r + k, -k),
            });
        }
    }
    v
}

/// What a terrain brush lays down.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Paint {
    /// Plain land: takes away everything else.
    Grass,
    Trees,
    Water,
    Meadow,
    /// A road tile, over whatever lies there.
    Road,
    /// Rock of a kind: `ROCK`, `ROCK | ORE` (ore-bearing) or `ROCK | CLIFF`.
    Rock(u32),
    Dunes,
    Marshland,
    Floodplain,
}

fn t(map: &Map, x: i32, y: i32) -> u32 {
    map.terrain_around(x, y, OUTSIDE)
}

fn set(map: &mut Map, x: i32, y: i32, v: u32) {
    if map.contains(x, y) {
        map.terrain.set(x, y, v);
    }
}

/// Lays `paint` on tile `(x, y)` as the original's brushes do (FUN_00477960 and the
/// ones after it): each clears the terrain its mask leaves out and sets its own bit,
/// and does nothing where its terrain already lies. Water and floodplain only go on
/// land that isn't raised. Returns whether the tile changed.
pub fn paint_tile(map: &mut Map, paint: Paint, x: i32, y: i32) -> bool {
    if !map.contains(x, y) {
        return false;
    }
    let old = map.terrain.at_or(x, y, 0);
    let flat = map.elevation.at_or(x, y, 0) == 0;
    let new = match paint {
        Paint::Grass => {
            if old == 0 {
                return false;
            }
            map.bitfields.update(x, y, |b| b & 0xe0);
            map.edges.update(x, y, |e| (e & 0xc0) | 0x40);
            map.building.set(x, y, 0);
            old & 0x99ca_f680
        }
        Paint::Trees if old & terrain::TREE != 0 => return false,
        Paint::Trees => old & 0x99ca_f681 | terrain::TREE,
        Paint::Rock(kind) if old & 0x2030_0002 == kind => return false,
        Paint::Rock(kind) => ((old & 0xdfcf_fffd) | kind) & 0xb9fa_f682,
        Paint::Dunes if old & terrain::DUNE != 0 => return false,
        Paint::Dunes => old & 0x9bcb_f680 | terrain::DUNE,
        Paint::Meadow if old & terrain::MEADOW != 0 => return false,
        Paint::Meadow => old & 0x99ca_fe80 | terrain::MEADOW,
        Paint::Marshland if old & terrain::MARSHLAND != 0 => return false,
        Paint::Marshland => old & 0x99ce_f680 | terrain::MARSHLAND,
        Paint::Floodplain if !flat => return false,
        Paint::Floodplain => old & 0x99cb_f680 | terrain::FLOODPLAIN,
        Paint::Water if !flat || old & terrain::WATER != 0 => return false,
        Paint::Water => old & 0x9dca_f784 | terrain::WATER,
        Paint::Road if old & terrain::ROAD != 0 => return false,
        Paint::Road => old & 0xd9cb_f7c0 | terrain::ROAD,
    };
    map.terrain.set(x, y, new);
    new != old
}

/// Whether tile `(x, y)` is hemmed in by `mask` (FUN_00477eb0 finding it closed):
/// the same bit on two opposite corners, or `mask` on every side's corner group.
fn enclosed(map: &Map, x: i32, y: i32, mask: u32) -> bool {
    let n = |dx: i32, dy: i32| t(map, x + dx, y + dy);
    let (nw, ne, sw, se) = (n(-1, -1), n(1, -1), n(-1, 1), n(1, 1));
    let (north, south, west, east) = (n(0, -1), n(0, 1), n(-1, 0), n(1, 0));
    if mask & nw & se != 0 || mask & ne & sw != 0 {
        return true;
    }
    mask & (nw | west | north) != 0 && mask & (ne | east | north) != 0 && mask & (sw | south | west) != 0 && mask & (se | south | east) != 0
}

/// The 3x3 block around `(x, y)` inside the map.
fn around(map: &Map, x: i32, y: i32) -> impl Iterator<Item = (i32, i32)> {
    let (w, h) = (map.width, map.height);
    ((y - 1).max(0)..=(y + 1).min(h - 1)).flat_map(move |yy| ((x - 1).max(0)..=(x + 1).min(w - 1)).map(move |xx| (xx, yy)))
}

/// Floodplain fills the land it hems in (FUN_00477f70).
fn grow_floodplain(map: &mut Map, x: i32, y: i32) -> bool {
    let mut changed = false;
    let mut stack = vec![(x, y)];
    while let Some((x, y)) = stack.pop() {
        for (xx, yy) in around(map, x, y).collect::<Vec<_>>() {
            let v = t(map, xx, yy);
            if v & (terrain::FLOODPLAIN | terrain::WATER) == 0 && enclosed(map, xx, yy, terrain::FLOODPLAIN | terrain::WATER) {
                set(map, xx, yy, v & 0xd9cb_f680 | terrain::FLOODPLAIN);
                changed = true;
                stack.push((xx, yy));
            }
        }
    }
    changed
}

/// Trees, rock, meadow and the like push the floodplain back from around them
/// (FUN_00478090).
fn strip_floodplain(map: &mut Map, x: i32, y: i32) -> bool {
    let mut changed = false;
    let mut stack = vec![(x, y)];
    while let Some((x, y)) = stack.pop() {
        for (xx, yy) in around(map, x, y).collect::<Vec<_>>() {
            let v = t(map, xx, yy);
            if v & terrain::FLOODPLAIN != 0 {
                set(map, xx, yy, v & !terrain::FLOODPLAIN);
                changed = true;
                if v & BLOCKS_FLOODPLAIN != 0 {
                    stack.push((xx, yy));
                }
            }
        }
    }
    changed
}

/// Neighbour steps in the order FUN_004781a0 tries them: N, E, S, W, NE, SE, SW, NW.
const ERODE_ORDER: [(i32, i32); 8] = [(0, -1), (1, 0), (0, 1), (-1, 0), (1, -1), (1, 1), (-1, 1), (-1, -1)];

/// Land hemmed in by floodplain wears it away: the floodplain neighbour with the
/// fewest floodplain tiles of its own turns to grassland, until the land is open
/// (FUN_00478270, FUN_004781a0).
fn erode_floodplain(map: &mut Map, x: i32, y: i32) -> bool {
    if t(map, x, y) & terrain::FLOODPLAIN != 0 {
        return false;
    }
    let mut changed = false;
    loop {
        if !enclosed(map, x, y, terrain::FLOODPLAIN) {
            return changed;
        }
        let mut best = 0;
        let mut pick = None;
        for (dx, dy) in ERODE_ORDER {
            let (nx, ny) = (x + dx, y + dy);
            let own = NEIGHBOURS.iter().filter(|&&(ex, ey)| t(map, nx + ex, ny + ey) & terrain::FLOODPLAIN != 0).count() as i32;
            if t(map, nx, ny) & terrain::FLOODPLAIN != 0 && best < 8 - own {
                best = 8 - own;
                pick = Some((nx, ny));
            }
        }
        let Some((nx, ny)) = pick.filter(|&(nx, ny)| map.contains(nx, ny)) else { return changed };
        let v = t(map, nx, ny);
        set(map, nx, ny, v & !terrain::FLOODPLAIN | terrain::GROUNDWATER);
        changed = true;
        erode_floodplain(map, nx, ny);
    }
}

/// Keeps the floodplain whole around a tile just painted (the checks in
/// FUN_00478440 after each brush tile): floodplain spreads into the land it
/// encloses, blocking terrain strips it from around itself, and land it encloses
/// wears it away. Returns whether any tile changed.
pub fn settle(map: &mut Map, x: i32, y: i32) -> bool {
    let v = t(map, x, y);
    if v & terrain::FLOODPLAIN != 0 {
        grow_floodplain(map, x, y)
    } else if v & BLOCKS_FLOODPLAIN != 0 {
        strip_floodplain(map, x, y)
    } else if v & terrain::WATER != 0 {
        false
    } else {
        erode_floodplain(map, x, y)
    }
}

/// The same checks over the whole map, as Refresh Map runs them (FUN_00478330).
pub fn settle_all(map: &mut Map) {
    for y in 0..map.height {
        for x in 0..map.width {
            settle(map, x, y);
        }
    }
}

/// How far grassland reaches from water for each climate (humid, normal, arid): the
/// search radius of FUN_00487450.
pub fn grass_reach(climate: u8) -> i32 {
    match climate {
        0 => 46,
        2 => 8,
        _ => 16,
    }
}

/// Distance in rings (the larger of the x and y steps) from each tile to the nearest
/// tile holding `mask` that isn't outside the map, or `i32::MAX` for none.
fn ring_distance(map: &Map, mask: u32) -> Vec<i32> {
    let (w, h) = (map.width, map.height);
    let mut d = vec![i32::MAX; (w * h) as usize];
    let mut queue = std::collections::VecDeque::new();
    for y in 0..h {
        for x in 0..w {
            let v = map.terrain.at_or(x, y, 0);
            if v & mask != 0 && v & terrain::DIKE == 0 {
                d[(y * w + x) as usize] = 0;
                queue.push_back((x, y));
            }
        }
    }
    while let Some((x, y)) = queue.pop_front() {
        let next = d[(y * w + x) as usize] + 1;
        for (dx, dy) in NEIGHBOURS {
            let (nx, ny) = (x + dx, y + dy);
            if nx >= 0 && ny >= 0 && nx < w && ny < h && d[(ny * w + nx) as usize] > next {
                d[(ny * w + nx) as usize] = next;
                queue.push_back((nx, ny));
            }
        }
    }
    d
}

/// The original's grass scale (FUN_00486ec0): `span` rings shared out over 96
/// levels, less one.
fn grass_level(a: i32, span: i32) -> i32 {
    let v = (span - a).max(0);
    (v as f64 / span.max(12) as f64 * 96.0) as i32 - 1
}

/// The grass-border table (26 rows at 0x5cbfb8): which of the eight neighbours
/// (N, NE, E, SE, S, SW, W, NW; 2 = either) are full grass, and the border image it
/// picks (14 = full grass). Only the first of the four view orientations is used.
const GRASS_EDGES: [([u8; 8], u8); 26] = [
    ([1, 2, 1, 2, 1, 2, 1, 2], 14),
    ([1, 2, 1, 2, 1, 2, 0, 2], 14),
    ([0, 2, 1, 2, 1, 2, 1, 2], 14),
    ([1, 2, 0, 2, 1, 2, 1, 2], 14),
    ([1, 2, 1, 2, 0, 2, 1, 2], 14),
    ([1, 2, 0, 2, 1, 2, 0, 2], 14),
    ([0, 2, 1, 2, 0, 2, 1, 2], 14),
    ([1, 2, 1, 2, 1, 2, 0, 2], 14),
    ([0, 2, 1, 2, 1, 2, 1, 2], 14),
    ([1, 2, 0, 2, 1, 2, 1, 2], 14),
    ([1, 2, 1, 2, 0, 2, 1, 2], 14),
    ([1, 2, 1, 2, 0, 0, 0, 2], 1),
    ([0, 2, 1, 2, 1, 2, 0, 0], 3),
    ([0, 0, 0, 2, 1, 2, 1, 2], 5),
    ([1, 2, 0, 0, 0, 2, 1, 2], 7),
    ([1, 2, 0, 0, 0, 0, 0, 2], 0),
    ([0, 2, 1, 2, 0, 0, 0, 0], 2),
    ([0, 0, 0, 2, 1, 2, 0, 0], 4),
    ([0, 0, 0, 0, 0, 2, 1, 2], 6),
    ([0, 1, 0, 0, 0, 1, 0, 0], 12),
    ([0, 0, 0, 1, 0, 0, 0, 1], 13),
    ([0, 1, 0, 0, 0, 0, 0, 0], 8),
    ([0, 0, 0, 1, 0, 0, 0, 0], 9),
    ([0, 0, 0, 0, 0, 1, 0, 0], 10),
    ([0, 0, 0, 0, 0, 0, 0, 1], 11),
    ([0, 0, 0, 0, 0, 0, 0, 0], 14),
];

fn moisture(map: &Map, x: i32, y: i32) -> u8 {
    map.moisture.at_or(x, y, 0)
}

fn water(map: &Map, x: i32, y: i32) -> bool {
    t(map, x, y) & terrain::WATER != 0
}

/// Water on both sides of the tile, north and south or west and east.
fn between_water(map: &Map, x: i32, y: i32) -> bool {
    (water(map, x, y - 1) && water(map, x, y + 1)) || (water(map, x - 1, y) && water(map, x + 1, y))
}

/// Whether neighbour `(x, y)` counts as full grass for the border beside it: grass
/// that is full (100) or lies between water, or marsh and water themselves.
fn full_grass(map: &Map, x: i32, y: i32) -> bool {
    if t(map, x, y) & (terrain::MARSHLAND | terrain::WATER) != 0 || between_water(map, x, y) {
        return true;
    }
    moisture(map, x, y) == 100
}

/// Works out the grassland of the whole map, as FUN_00487450 does over its
/// rectangle: land within reach of water (by climate) or of floodplain and marsh
/// (8 rings) is grassland (the `GROUNDWATER` bit) with a moisture level that falls
/// off with distance; water, floodplain and marsh hold none. Then, row by row,
/// grassland beside full grass takes one of the border images, recorded in the
/// moisture byte as `0x80 | image` (or 100 for full grass), and loses any meadow.
pub fn refresh_grass(map: &mut Map, climate: u8) {
    let reach = grass_reach(climate);
    let to_water = ring_distance(map, terrain::WATER);
    let to_marsh = ring_distance(map, terrain::FLOODPLAIN | terrain::MARSHLAND);
    let (w, h) = (map.width, map.height);
    for y in 0..h {
        for x in 0..w {
            let v = map.terrain.at_or(x, y, 0);
            if v & (terrain::FLOODPLAIN | terrain::MARSHLAND | terrain::WATER) != 0 {
                map.moisture.set(x, y, 0);
                map.terrain.set(x, y, v & !terrain::GROUNDWATER);
                continue;
            }
            let i = (y * w + x) as usize;
            let dw = Some(to_water[i]).filter(|&d| d < reach);
            let dm = Some(to_marsh[i]).filter(|&d| d < 8);
            // Steps short of one count as none (the original's `v - 1U` mask).
            let steps = |v: i32| if v < 1 { 0 } else { v };
            let from_water = match dw {
                None => 0,
                Some(d) if d < reach - 12 => grass_level(0, reach),
                Some(d) => grass_level(steps(d - (reach - 12)), 12),
            };
            let level = match (dw, dm) {
                (None, None) => {
                    map.moisture.set(x, y, 0);
                    map.terrain.set(x, y, v & !terrain::GROUNDWATER);
                    continue;
                }
                (_, None) => from_water,
                (_, Some(d)) => from_water.max(grass_level(steps(d - 1), 8)),
            };
            map.terrain.set(x, y, v | terrain::GROUNDWATER);
            map.moisture.set(x, y, level.max(1) as u8);
        }
    }
    // The borders: once the stale image of a missing match carries over, as the
    // original's global does.
    let mut image = 14u8;
    for y in 0..h {
        for x in 0..w {
            if map.terrain.at_or(x, y, 0) & terrain::GROUNDWATER == 0 {
                continue;
            }
            if between_water(map, x, y) {
                map.moisture.set(x, y, 100);
                continue;
            }
            let m = moisture(map, x, y);
            if !(41..=90).contains(&m) {
                if m == 100 {
                    map.moisture.set(x, y, 100);
                }
                continue;
            }
            let flags = NEIGHBOURS.map(|(dx, dy)| full_grass(map, x + dx, y + dy) as u8);
            if flags.iter().all(|&f| f == 0) {
                continue;
            }
            map.terrain.update(x, y, |v| v & !terrain::MEADOW);
            if let Some(&(_, found)) = GRASS_EDGES.iter().find(|(want, _)| want.iter().zip(flags).all(|(&w, f)| w == 2 || w == f)) {
                image = found;
            }
            map.moisture.set(x, y, if image == 14 { 100 } else { image | 0x80 });
        }
    }
}

/// Deep water (FUN_004877f0): water with water all around it and no floodplain
/// beside it, then thinned where it only reaches out in narrow strips.
pub fn refresh_deep_water(map: &mut Map) {
    let (w, h) = (map.width, map.height);
    let inside = |x: i32, y: i32| x >= 0 && y >= 0 && x < w && y < h;
    for y in 0..h {
        for x in 0..w {
            let v = map.terrain.at_or(x, y, 0);
            if v & (terrain::WATER | terrain::DIKE) != terrain::WATER {
                continue;
            }
            let mut deep = true;
            for yy in y - 1..=y + 1 {
                for xx in x - 1..=x + 1 {
                    if inside(xx, yy) {
                        let n = map.terrain.at_or(xx, yy, 0);
                        if n & terrain::WATER == 0 || n & terrain::FLOODPLAIN != 0 {
                            deep = false;
                        }
                    }
                }
            }
            map.terrain.set(x, y, if deep { v | terrain::DEEPWATER } else { v & !terrain::DEEPWATER });
        }
    }
    let deep = |map: &Map, x: i32, y: i32| inside(x, y) && map.terrain.at_or(x, y, 0) & terrain::DEEPWATER != 0;
    // A deep tile stays deep with two deep tiles in its row and in its column, and
    // no run of exactly two deep neighbours between shallow ones around it.
    let keeps = |map: &Map, x: i32, y: i32| {
        let row = (x - 1..=x + 1).filter(|&xx| deep(map, xx, y)).count() > 1;
        let column = (y - 1..=y + 1).filter(|&yy| deep(map, x, yy)).count() > 1;
        if !(row && column) {
            return false;
        }
        let flags = NEIGHBOURS.map(|(dx, dy)| deep(map, x + dx, y + dy));
        let (mut i, mut end, mut run) = (0usize, 16usize, -1i32);
        while i < end {
            if run > 7 {
                break;
            }
            if !flags[i & 7] {
                if run < 0 {
                    end = i + 10;
                } else if run == 2 {
                    return false;
                }
                run = 0;
            } else if run >= 0 {
                run += 1;
            }
            i += 1;
        }
        true
    };
    for y in 0..h {
        for x in 0..w {
            if !deep(map, x, y) {
                continue;
            }
            let mut stack = vec![(x, y)];
            while let Some((cx, cy)) = stack.pop() {
                for (nx, ny) in around(map, cx, cy).collect::<Vec<_>>() {
                    if deep(map, nx, ny) && !keeps(map, nx, ny) {
                        map.terrain.update(nx, ny, |v| v & !terrain::DEEPWATER);
                        stack.push((nx, ny));
                    }
                }
            }
        }
    }
}

/// The meadow's soil in a rectangle (FUN_00488e90), after the meadow brush: each
/// meadow tile's fertility follows how much of the land within four rings is meadow,
/// nearer rings counting more (FUN_00481540); every other tile's is 0.
pub fn refresh_meadow_soil(map: &mut Map, x0: i32, y0: i32, x1: i32, y1: i32) {
    let (w, h) = (map.width, map.height);
    for y in y0.max(0)..=y1.min(h - 1) {
        for x in x0.max(0)..=x1.min(w - 1) {
            if map.terrain.at_or(x, y, 0) & terrain::MEADOW == 0 {
                map.fertility.set(x, y, 0);
                continue;
            }
            let (mut score, mut total) = (0.0f64, 0.0f64);
            for ring in (1..=4i32).rev() {
                let weight = 1.0 / ring as f64 * 100.0;
                for dy in -ring..=ring {
                    for dx in -ring..=ring {
                        if dx.abs() != ring && dy.abs() != ring {
                            continue;
                        }
                        let (nx, ny) = (x + dx, y + dy);
                        if nx < 0 || ny < 0 || nx >= w || ny >= h {
                            continue;
                        }
                        total += weight;
                        score += if map.terrain.at_or(nx, ny, 0) & terrain::MEADOW != 0 { weight } else { -weight };
                    }
                }
            }
            let share = if total > 0.0 { (score.max(0.0) / total * 100.0) as i32 } else { 0 };
            let step = (share as f64 * 0.11) as i32;
            map.fertility.set(x, y, ((step + 1) as f64 / 12.0 * 99.0) as u8);
        }
    }
}

/// The map's diamond, as the original works it out from the map's size (FUN_0046f7b0):
/// the tiles shown, and those near its edge where the points that bring people and
/// ships in must lie.
pub struct Diamond {
    x0: i32,
    y0: i32,
    x_last: i32,
    y_last: i32,
    cx: i32,
    cy: i32,
}

impl Diamond {
    pub fn new(width: i32, height: i32) -> Self {
        let x0 = (228 - width) / 2;
        let y0 = (228 - height) / 2;
        let x_last = x0 + width - 1;
        let y_last = y0 + height - 1;
        Self { x0, y0, x_last, y_last, cx: (x_last + x0) / 2, cy: (y_last + y0) / 2 }
    }

    /// Whether map tile `(x, y)` is shown (FUN_0046f930).
    pub fn inside(&self, x: i32, y: i32) -> bool {
        let (gx, gy) = (x + self.x0, y + self.y0);
        if gx > self.cx {
            if gy <= self.cy { gx - gy <= self.x_last - self.cy } else { gx + gy <= self.x_last + 1 + self.cy }
        } else if gy <= self.cy {
            self.x0 + self.cy <= gx + gy
        } else {
            self.cx - self.y_last <= gx - gy
        }
    }

    /// Shown, and within `margin` tiles of the diamond's edge (FUN_0046f9a0).
    pub fn near_edge(&self, x: i32, y: i32, margin: i32) -> bool {
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use osiris_formats::Scenario;

    #[test]
    fn brushes_are_diamonds() {
        for (size, n) in [(0u8, 1usize), (1, 5), (2, 13), (3, 25), (4, 41)] {
            let b = brush(size);
            assert_eq!(b.len(), n);
            assert!(b.iter().all(|&(x, y)| x.abs() + y.abs() <= size as i32));
            let mut sorted = b.clone();
            sorted.sort();
            sorted.dedup();
            assert_eq!(sorted.len(), n, "no tile twice");
        }
        // The original's table starts each ring due north and goes clockwise.
        assert_eq!(&brush(1)[1..], &[(0, -1), (1, 0), (0, 1), (-1, 0)]);
    }

    fn data() -> Option<std::path::PathBuf> {
        let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../PharaohData");
        dir.is_dir().then_some(dir)
    }

    /// The grassland worked out again over a whole shipped map matches what the
    /// original's editor saved in it, tile for tile, for most of the land.
    #[test]
    fn grassland_matches_shipped_maps() {
        let Some(data) = data() else { return };
        for name in ["Small Oasis.map", "Two Cities.map", "Kyrene.map", "Shaat.map", "Alexandria.map", "Warfare.map", "Enkomi.map", "Holy Grail.map", "Toshka.map", "Unreliant.map"] {
            let s = Scenario::load_map(&data.join("Maps").join(name)).unwrap();
            let map = Map::from_scenario(&s);
            let mut again = map.clone();
            refresh_grass(&mut again, s.info.climate);
            let (mut same, mut all) = (0, 0);
            for y in 0..map.height {
                for x in 0..map.width {
                    let v = map.terrain.at_or(x, y, 0);
                    if v & (terrain::DIKE | terrain::BUILDING | terrain::ROAD) != 0 {
                        continue;
                    }
                    all += 1;
                    let a = (v & terrain::GROUNDWATER != 0, map.moisture.at_or(x, y, 0));
                    let b = (again.terrain.at_or(x, y, 0) & terrain::GROUNDWATER != 0, again.moisture.at_or(x, y, 0));
                    same += (a == b) as i32;
                }
            }
            let share = same as f64 / all as f64;
            eprintln!("{name} (climate {}): {same}/{all} = {share:.3}", s.info.climate);
            assert!(share > 0.96, "{name}: only {share:.3} of the grassland matches");
        }
    }

    /// Deep water worked out again, and the floodplain's shape checked again, agree
    /// with the shipped maps.
    #[test]
    fn deep_water_and_floodplain_match_shipped_maps() {
        let Some(data) = data() else { return };
        for name in ["Two Cities.map", "Kyrene.map", "Alexandria.map", "Warfare.map", "Holy Grail.map", "Pharaohs Memphis.map"] {
            let s = Scenario::load_map(&data.join("Maps").join(name)).unwrap();
            let map = Map::from_scenario(&s);
            let mut again = map.clone();
            refresh_deep_water(&mut again);
            settle_all(&mut again);
            let (mut deep, mut plain, mut all) = (0, 0, 0);
            for y in 0..map.height {
                for x in 0..map.width {
                    let (a, b) = (map.terrain.at_or(x, y, 0), again.terrain.at_or(x, y, 0));
                    if a & terrain::DIKE != 0 {
                        continue;
                    }
                    all += 1;
                    deep += (a & terrain::DEEPWATER == b & terrain::DEEPWATER) as i32;
                    plain += (a & terrain::FLOODPLAIN == b & terrain::FLOODPLAIN) as i32;
                }
            }
            eprintln!("{name}: deep water {deep}/{all}, floodplain {plain}/{all}");
            assert!(deep as f64 / all as f64 > 0.97, "{name}: deep water");
            assert!(plain as f64 / all as f64 > 0.99, "{name}: floodplain");
        }
    }
}
