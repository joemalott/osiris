//! How pyramids and mastabas are built, as in the original. A tomb is a grid of 2x2
//! blocks, each with its own state. Laborers first prepare the whole site together,
//! a step at a time for every block: clear the ground, lay rough sand, dig trenches,
//! flood them to find the level, drain them, fill them and smooth the ground, and
//! (for a pyramid) lay the foundation in the four centre blocks. A mastaba's site is
//! only cleared and sanded.
//!
//! Then the courses rise. The blocks are built in lockstep: no block takes another
//! unit of work until every block still building has caught up. A unit is a sixth
//! of a course on one block, a stonemason's (or, for bricks, a bricklayer's) 110
//! ticks, and a sled of 400 units of stone, bricks or limestone covers two of
//! them. The inner blocks rise higher: a pyramid's centre has as many courses as
//! the pyramid is large (small 2 .. grand 6), each ring outward one fewer. Every
//! block's top course is its outer face: limestone casing on bent, true and
//! mudbrick pyramids. Carpenters build a ramp on set blocks at each step of the
//! lower courses, and a block waits for its ramp. Last, stonemasons polish the
//! casing, top down.

use crate::buildings::BuildingId;
use crate::map::{mask, terrain};
use crate::monuments::{Family, Monument, Style, monument_def};
use crate::world::World;

/// Work ticks: a laborer's touch of one tile, a mason's unit of a course, a
/// polisher's unit, a carpenter's ramp.
pub const TILE_WORK: u16 = 50;
pub const UNIT_WORK: u16 = 110;
pub const POLISH_WORK: u16 = 40;
pub const RAMP_WORK: u16 = 30;
/// Material a unit takes: half a sled.
pub const UNIT_MATERIAL: i32 = 200;
/// Timber the carpenters' guild gives up for each ramp.
pub const RAMP_TIMBER: i32 = 100;

const STONE: u16 = 24;
const LIMESTONE: u16 = 25;
const BRICKS: u16 = 12;

/// The prepared site's steps (a block's state), and the two building states.
const CLEARED: u8 = 4;
const SANDED: u8 = 5;
const DUG: u8 = 6;
const FLOODED: u8 = 7;
const DRAINED: u8 = 8;
const FILLED: u8 = 9;
const SMOOTH: u8 = 10;
const FOUNDATION: u8 = 11;
const BUILDING: u8 = 0;
const BUILT: u8 = 1;
const ALL_TILES: u8 = 0xf;

/// A tomb's stages: preparing the site, raising the courses, polishing.
pub const PREP: u8 = 0;
pub const RAISE: u8 = 1;
pub const POLISH: u8 = 2;

/// One 2x2 block of a pyramid or mastaba.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Block {
    /// Its top-left tile within the footprint.
    pub x: i32,
    pub y: i32,
    /// Its place in its ring: corners 0 (-x-y), 3 (+x-y), 1 (-x+y), 2 (+x+y); edges
    /// 7 (-y), 4 (-x), 6 (+x), 5 (+y); a mastaba's inside 8 and its chapel 9.
    pub kind: u8,
    /// Its last course.
    pub top: u8,
    /// A site step (4-11), then 0 while building and 1 once built.
    pub state: u8,
    /// Which of its four tiles have had the current site step.
    pub mask: u8,
    /// The course it is on, and the units of it done.
    pub level: u8,
    pub counter: u8,
    /// Its ramp, if it has one: the progress it waits for it at, and whether built.
    pub ramp_at: u8,
    pub ramp: bool,
    /// Stepped pyramids' ramps span two blocks: this one shows its partner's.
    #[serde(default)]
    pub ramp_shown: bool,
}

/// A part of a pyramid complex: the mortuary temple against the pyramid's east face,
/// the causeway blocks, and the valley temple on the shore. `x`, `y` are its top-left
/// tile from the pyramid's, and may lie beyond its footprint.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Part {
    pub x: i32,
    pub y: i32,
    /// 0 mortuary temple, 1 causeway, 2 valley temple.
    pub kind: u8,
    pub built: bool,
}

/// Ticks a stonemason takes over a part of a complex.
pub const PART_WORK: u16 = 200;
/// Most causeway blocks before the shore.
const MAX_CAUSEWAY: usize = 20;

/// The original's placement messages for tombs (text group 19, 211 and 212).
pub const FREE_OF_OBSTRUCTIONS: &str = "Must be built on land free of obstructions";
pub const CAUSEWAY_TO_WATER: &str = "Monument's causeway must lead to water";

/// What a tomb's own tiles may not hold (the original's 0xfeffd76e): trees, shrubs and
/// meadow are fine, the laborers clear them.
pub(crate) const TOMB_BLOCKED: u32 = mask::NOT_CLEAR & !(terrain::TREE | terrain::SHRUB) | terrain::FERRY_ROUTE;
/// The row past its south edge may also carry a road (0xfeffd72e).
const ROW_BLOCKED: u32 = TOMB_BLOCKED & !terrain::ROAD;
/// A complex's part tiles (0xeeffd76e), the same as the tomb's.
const PART_BLOCKED: u32 = TOMB_BLOCKED;
/// A part's unchecked top-left tile: only nothing built there.
const PART_CORNER_BLOCKED: u32 = terrain::BUILDING | terrain::ROAD | terrain::CANAL | terrain::WALL | terrain::GATEHOUSE | terrain::DIKE;
/// Open water the valley temple looks onto (the original's 0xfbffff7b clear bits).
const OPEN_WATER: u32 = terrain::WATER | terrain::GROUNDWATER | terrain::DEEPWATER;

/// A pyramid complex's parts as far as they could be laid out, and where and why
/// they could go no further.
#[derive(Debug, Clone, Default)]
pub struct ComplexWalk {
    pub parts: Vec<Part>,
    /// The top-left tile of the block that is neither clear nor the shore (or is one
    /// block too many), and the message.
    pub fail: Option<((i32, i32), &'static str)>,
}

/// The footprint tile under the cursor while placing a tomb: the original's anchor,
/// the first block of its layout table.
pub fn anchor(style: Style, variant: usize) -> (i32, i32) {
    match style {
        Style::Mastaba => {
            let (x0, y0) = MASTABAS[variant.min(2)].iter().fold((0, 0), |(mx, my), b| (mx.min(b.0 as i32), my.min(b.1 as i32)));
            (-x0, -y0)
        }
        _ => {
            let shift = 2 * (variant as i32 + 1);
            (shift, shift)
        }
    }
}

/// Where a complex's mortuary temple stands, from the pyramid's anchor: the block
/// just past the middle of its east face (the original's table at 0x5f5d08). Only
/// the complex and grand pyramids have one.
fn complex_start(style: Style, variant: usize) -> Option<(i32, i32)> {
    match (style, variant) {
        (Style::Pyramid(_), 3) => Some((12, 1)),
        (Style::Pyramid(_), 4) => Some((14, 1)),
        _ => None,
    }
}

/// Ramps of each size of pyramid: (block, progress it is needed at, partner block).
const RAMPS: [&[(u8, u8, u8)]; 5] = [
    &[(13, 1, 0), (14, 2, 15), (11, 3, 0), (9, 4, 7), (6, 5, 0), (5, 6, 0)],
    &[(31, 1, 0), (32, 2, 0), (33, 3, 0), (34, 4, 35), (29, 5, 0), (27, 6, 0), (9, 7, 7), (6, 8, 0), (5, 9, 0), (8, 10, 0), (10, 11, 12), (13, 12, 0)],
    &[
        (57, 1, 0), (58, 2, 0), (59, 3, 0), (60, 4, 0), (61, 5, 0), (62, 6, 0), (29, 7, 0), (27, 8, 0), (25, 9, 0),
        (23, 10, 21), (20, 11, 0), (19, 12, 0), (5, 13, 4), (8, 14, 0), (10, 15, 0), (13, 16, 0), (14, 17, 15), (11, 18, 0),
    ],
    &[
        (91, 1, 0), (92, 2, 0), (93, 3, 0), (94, 4, 0), (95, 5, 0), (96, 6, 0), (62, 7, 63), (55, 8, 0), (53, 9, 0), (51, 10, 0),
        (49, 11, 0), (47, 12, 0), (20, 13, 0), (19, 14, 0), (18, 15, 0), (17, 16, 16), (22, 17, 0), (24, 18, 0), (10, 19, 12),
        (13, 20, 0), (14, 21, 0), (11, 22, 0), (9, 23, 7), (6, 24, 0),
    ],
    &[
        (133, 1, 0), (134, 2, 0), (135, 3, 0), (136, 4, 0), (137, 5, 0), (138, 6, 0), (96, 7, 0), (97, 8, 0), (98, 9, 0),
        (89, 10, 0), (87, 11, 0), (85, 12, 0), (51, 13, 0), (49, 14, 0), (47, 15, 0), (45, 16, 43), (42, 17, 0), (41, 18, 0),
        (19, 19, 0), (18, 20, 0), (17, 21, 0), (22, 22, 0), (24, 23, 0), (26, 24, 0), (13, 25, 0), (14, 26, 15), (11, 27, 0),
        (9, 28, 7), (6, 29, 0), (5, 30, 0),
    ],
];

/// Mastaba blocks of each size: offset from the chapel block, kind, last course.
const MASTABAS: [&[(i8, i8, u8, u8)]; 3] = [
    &[(0, 0, 9, 0), (-2, -4, 7, 0), (0, -4, 7, 0), (-2, -2, 8, 0), (0, -2, 8, 0), (-2, 0, 8, 0), (-2, 2, 8, 0), (0, 2, 8, 0), (-2, 4, 5, 0), (0, 4, 5, 0)],
    &[
        (0, 0, 9, 0), (-2, -4, 8, 0), (0, -4, 8, 0), (-2, -2, 8, 0), (0, -2, 8, 0), (-2, 0, 8, 0), (-2, 2, 8, 0), (0, 2, 8, 0),
        (-2, 4, 8, 0), (0, 4, 8, 0), (-4, -6, 7, 0), (-2, -6, 7, 0), (0, -6, 7, 0), (-4, -4, 8, 0), (-4, -2, 8, 0), (-4, 0, 8, 0),
        (-4, 2, 8, 0), (-4, 4, 8, 0), (-4, 6, 5, 0), (-2, 6, 5, 0), (0, 6, 5, 0),
    ],
    &[
        (0, 0, 9, 0), (-2, -4, 8, 1), (0, -4, 8, 1), (-2, -2, 8, 1), (0, -2, 8, 1), (-2, 0, 8, 1), (-2, 2, 8, 1), (0, 2, 8, 1),
        (-2, 4, 8, 1), (0, 4, 8, 1), (-4, -6, 7, 1), (-2, -6, 7, 1), (0, -6, 7, 1), (-4, -4, 8, 1), (-4, -2, 8, 1), (-4, 0, 8, 1),
        (-4, 2, 8, 1), (-4, 4, 8, 1), (-4, 6, 5, 1), (-2, 6, 5, 1), (0, 6, 5, 1), (-6, -8, 7, 0), (-4, -8, 7, 0), (-2, -8, 7, 0),
        (0, -8, 7, 0), (-6, -6, 7, 1), (-6, -4, 8, 1), (-6, -2, 8, 1), (-6, 0, 8, 1), (-6, 2, 8, 1), (-6, 4, 8, 1), (-6, 6, 5, 1),
        (-6, 8, 5, 0), (-4, 8, 5, 0), (-2, 8, 5, 0), (0, 8, 5, 0),
    ],
];

/// Large mastaba blocks that show a full lower course beneath their second.
const MASTABA_FILLER: [usize; 16] = [2, 4, 5, 7, 9, 12, 20, 25, 26, 27, 28, 29, 30, 31, 32, 33];

/// Bent pyramids' half courses (three units, 45 pixels) and each course's height.
const BENT_HALF: [&[bool]; 2] = [&[false, true], &[false, true, true]];
const BENT_RAISE: [&[i32]; 2] = [&[0, 90], &[0, 90, 135]];
const COURSE_RAISE: i32 = 90;

/// The size of a tomb: 0 small .. 4 grand (mastabas 0-2).
pub fn variant(cols: i32, style: Style) -> usize {
    match style {
        Style::Mastaba => (cols as usize).saturating_sub(2).min(2),
        _ => ((cols as usize) / 2).saturating_sub(2).min(4),
    }
}

/// The blocks of a new tomb, in the original's order (the ramps refer to them).
pub fn layout(style: Style, variant: usize) -> Vec<Block> {
    let block = |x: i32, y: i32, kind: u8, top: u8| Block { x, y, kind, top, state: 2, mask: ALL_TILES, ..Default::default() };
    let mut v = Vec::new();
    match style {
        Style::Mastaba => {
            let table = MASTABAS[variant.min(2)];
            let (x0, y0) = table.iter().fold((0, 0), |(mx, my), b| (mx.min(b.0 as i32), my.min(b.1 as i32)));
            for &(dx, dy, kind, top) in table {
                v.push(block(dx as i32 - x0, dy as i32 - y0, kind, top));
            }
        }
        _ => {
            let courses = variant as i32 + 2;
            let shift = 2 * (courses - 1);
            for r in 0..courses {
                let (lo, hi) = (-2 * r, 2 + 2 * r);
                for y in (lo..=hi).step_by(2) {
                    let xs: Vec<i32> = if y == lo || y == hi { (lo..=hi).step_by(2).collect() } else { vec![lo, hi] };
                    for x in xs {
                        let (left, right, top, bottom) = (x == lo, x == hi, y == lo, y == hi);
                        let kind = match (left, right, top, bottom) {
                            (true, _, true, _) => 0,
                            (_, true, true, _) => 3,
                            (true, _, _, true) => 1,
                            (_, true, _, true) => 2,
                            (_, _, true, _) => 7,
                            (true, _, _, _) => 4,
                            (_, true, _, _) => 6,
                            _ => 5,
                        };
                        v.push(block(x + shift, y + shift, kind, (courses - 1 - r) as u8));
                    }
                }
            }
            for &(b, at, _) in RAMPS[variant.min(4)] {
                if let Some(bl) = v.get_mut(b as usize) {
                    bl.ramp_at = at;
                }
            }
        }
    }
    v
}

impl Block {
    fn progress(&self) -> u32 {
        self.level as u32 * 6 + self.counter as u32
    }

    /// The site step this block does next, if it still has one.
    fn next_step(&self, mastaba: bool) -> Option<u8> {
        let last = if mastaba { SANDED } else { SMOOTH };
        if self.state == BUILDING || self.state == BUILT {
            return None;
        }
        if self.mask != ALL_TILES {
            return Some(self.state);
        }
        match self.state {
            s if s >= last => None,
            2 | 3 => Some(CLEARED),
            s => Some(s + 1),
        }
    }

    /// Whether a site step is done for the whole block at one touch.
    fn whole(step: u8) -> bool {
        matches!(step, SANDED | FOUNDATION)
    }

    /// Whether its ramp holds it up.
    fn waiting_for_ramp(&self) -> bool {
        self.ramp_at > 0 && !self.ramp && self.progress() >= self.ramp_at as u32
    }
}

/// What a tomb is made of: (the course material below each block's top, the top).
fn materials(style: Style) -> (u16, u16) {
    match style {
        Style::Pyramid(Family::Bent | Family::True) => (STONE, LIMESTONE),
        Style::Pyramid(Family::Mudbrick) => (BRICKS, LIMESTONE),
        Style::Mastaba => (BRICKS, BRICKS),
        _ => (STONE, STONE),
    }
}

/// Units in the course a block is on.
fn course_limit(style: Style, variant: usize, b: &Block, index: usize) -> u8 {
    match style {
        Style::Mastaba if index == 0 => 1,
        Style::Pyramid(Family::Bent) if BENT_HALF[variant.min(1)].get(b.level as usize).copied().unwrap_or(false) => 3,
        _ => 6,
    }
}

/// Makes a tomb's blocks look finished (for a game saved before blocks were kept).
pub fn finish_blocks(style: Style, blocks: &mut [Block]) {
    let var = 0;
    for (i, b) in blocks.iter_mut().enumerate() {
        b.state = BUILT;
        b.mask = ALL_TILES;
        b.level = b.top;
        b.counter = if polished(style) { 0 } else { course_limit(style, var, b, i) };
    }
}

/// The course a block's next unit is on: the next course once its current one is
/// full.
fn next_level(style: Style, variant: usize, b: &Block, index: usize) -> u8 {
    if b.counter > 0 && b.counter >= course_limit(style, variant, b, index) && b.level < b.top { b.level + 1 } else { b.level }
}

/// Units in each of a block's courses, and those done.
fn units(style: Style, variant: usize, b: &Block, index: usize) -> (u32, u32) {
    let mut total = 0;
    let mut done = 0;
    for level in 0..=b.top {
        let probe = Block { level, ..b.clone() };
        let limit = course_limit(style, variant, &probe, index) as u32;
        total += limit;
        done += match b.state {
            BUILT => limit,
            BUILDING if level < b.level => limit,
            BUILDING if level == b.level => (b.counter as u32).min(limit),
            _ => 0,
        };
    }
    (total, done)
}

/// Whether a style is built block by block.
pub fn blockwise(style: Style) -> bool {
    matches!(style, Style::Pyramid(_) | Style::Mastaba)
}

/// Whether a tomb's casing is polished.
fn polished(style: Style) -> bool {
    matches!(style, Style::Pyramid(Family::Bent | Family::True | Family::Mudbrick))
}

/// Work a craftsman could do on a tomb: which block, and whether it is polishing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Job {
    Unit(usize),
    Polish(usize),
    Ramp(usize),
    /// A part of a pyramid complex.
    Part(usize),
}

impl World {
    /// Walks a pyramid complex's parts for a pyramid placed at `(x0, y0)`, as the
    /// original does (its `FUN_004ed360` mode 0, which also draws the placement
    /// preview): the mortuary temple is the 2x2 block just past the middle of the
    /// pyramid's east face, and the causeway runs east from it, a block (two tiles) at
    /// a time, whatever the view, until a block is no longer clear. That block must be
    /// on the shore, a straight north-south shoreline: its west column land, its east
    /// column open water. At least one causeway block must lead there, and at most
    /// [`MAX_CAUSEWAY`]. Smaller pyramids and mastabas have no complex.
    ///
    /// The original never looks at a block's top-left tile; Osiris only asks that no
    /// building or road stands there, so a part can't be built over one.
    pub(crate) fn complex_walk(&self, style: Style, variant: usize, (x0, y0): (i32, i32)) -> ComplexWalk {
        let mut walk = ComplexWalk::default();
        let Some((dx, dy)) = complex_start(style, variant) else { return walk };
        let (ax, ay) = anchor(style, variant);
        let (sx, sy) = (x0 + ax + dx, y0 + ay + dy);
        let map = &self.map;
        let tile = |x: i32, y: i32| map.terrain.at_or(x, y, 0);
        let clear = |x: i32, y: i32| map.contains(x, y) && tile(x, y) & PART_BLOCKED == 0 && map.building.at_or(x, y, 0) == 0;
        let corner = |x: i32, y: i32| map.contains(x, y) && tile(x, y) & PART_CORNER_BLOCKED == 0 && map.building.at_or(x, y, 0) == 0;
        let block_clear = |x: i32, y: i32| corner(x, y) && clear(x + 1, y) && clear(x, y + 1) && clear(x + 1, y + 1);
        let water = |x: i32, y: i32| map.contains(x, y) && tile(x, y) & terrain::WATER != 0 && tile(x, y) & !OPEN_WATER == 0;
        let mut bx = sx;
        while block_clear(bx, sy) {
            if walk.parts.len() > MAX_CAUSEWAY {
                walk.fail = Some(((bx, sy), CAUSEWAY_TO_WATER));
                return walk;
            }
            walk.parts.push(Part { x: bx - x0, y: sy - y0, kind: if walk.parts.is_empty() { 0 } else { 1 }, built: false });
            bx += 2;
        }
        let shore = corner(bx, sy) && clear(bx, sy + 1) && water(bx + 1, sy) && water(bx + 1, sy + 1);
        if walk.parts.len() < 2 || !shore {
            walk.fail = Some(((bx, sy), CAUSEWAY_TO_WATER));
            return walk;
        }
        walk.parts.push(Part { x: bx - x0, y: sy - y0, kind: 2, built: false });
        walk
    }

    /// A pyramid complex's parts for a pyramid placed at `(x0, y0)`, or why there can
    /// be none: see [`World::complex_walk`].
    pub(crate) fn complex_parts(&self, style: Style, variant: usize, at: (i32, i32)) -> Result<Vec<Part>, &'static str> {
        let walk = self.complex_walk(style, variant, at);
        match walk.fail {
            Some((_, why)) => Err(why),
            None => Ok(walk.parts),
        }
    }

    /// Why tile `(x, y)` of the row just past a tomb's south edge keeps it from being
    /// placed, if it does: the original wants that row free (roads may cross it).
    pub(crate) fn tomb_row_problem(&self, x: i32, y: i32) -> Option<&'static str> {
        self.map.terrain_is(x, y, ROW_BLOCKED).then_some(FREE_OF_OBSTRUCTIONS)
    }

    /// The tiles a complex's parts stand on, from the pyramid's top-left.
    pub(crate) fn part_tiles(parts: &[Part]) -> Vec<(i32, i32)> {
        parts.iter().flat_map(|p| [(p.x, p.y), (p.x + 1, p.y), (p.x, p.y + 1), (p.x + 1, p.y + 1)]).collect()
    }

    fn tomb(&self, id: BuildingId) -> Option<(Style, usize, &Monument)> {
        let b = self.buildings.get(id)?;
        let def = monument_def(b.kind)?;
        let m = b.monument.as_ref()?;
        blockwise(def.style).then(|| (def.style, variant(def.cols, def.style), m))
    }

    /// The blocks still building whose next unit comes first: those furthest behind.
    fn frontier(&self, id: BuildingId) -> Vec<usize> {
        let Some((_, _, m)) = self.tomb(id) else { return Vec::new() };
        let low = m.blocks.iter().filter(|b| b.state == BUILDING).map(Block::progress).min();
        let Some(low) = low else { return Vec::new() };
        m.blocks.iter().enumerate().filter(|(_, b)| b.state == BUILDING && b.progress() == low).map(|(i, _)| i).collect()
    }

    /// The blocks to polish next: the highest casing not yet polished.
    fn polish_frontier(&self, id: BuildingId) -> Vec<usize> {
        let Some((_, _, m)) = self.tomb(id) else { return Vec::new() };
        let high = m.blocks.iter().filter(|b| b.state == BUILT && b.counter > 0).map(Block::progress).max();
        let Some(high) = high else { return Vec::new() };
        m.blocks.iter().enumerate().filter(|(_, b)| b.state == BUILT && b.counter > 0 && b.progress() == high).map(|(i, _)| i).collect()
    }

    /// The material block `i`'s next unit takes: its core below its top course,
    /// the casing on it.
    pub(crate) fn tomb_unit_material(&self, id: BuildingId, i: usize) -> Option<u16> {
        let (style, var, m) = self.tomb(id)?;
        let b = m.blocks.get(i)?;
        let (core, top) = materials(style);
        Some(if next_level(style, var, b, i) < b.top { core } else { top })
    }

    /// The craftsman a unit on block `i` wants: bricklayers lay bricks, stonemasons
    /// stone and limestone and do the polishing.
    fn unit_craftsman(&self, id: BuildingId, i: usize) -> u16 {
        match self.tomb_unit_material(id, i) {
            Some(BRICKS) => crate::monuments::BRICKLAYER,
            _ => crate::monuments::STONEMASON,
        }
    }

    /// Tomb work for a craftsman of type `figure`, other than the jobs in `taken`: a
    /// block's next unit, a ramp, polishing, and (for stonemasons with nothing else to
    /// do once the site is ready) the next part of a complex.
    pub(crate) fn tomb_job(&self, id: BuildingId, figure: u16, taken: &[Job]) -> Option<Job> {
        let (style, _, m) = self.tomb(id)?;
        let job = match m.phase {
            RAISE if figure == crate::monuments::CARPENTER => {
                // A ramp a block is waiting for.
                return m.blocks.iter().enumerate().find(|(i, b)| b.waiting_for_ramp() && !taken.contains(&Job::Ramp(*i))).map(|(i, _)| Job::Ramp(i));
            }
            RAISE => self
                .frontier(id)
                .into_iter()
                .find(|&i| {
                    let b = &m.blocks[i];
                    !taken.contains(&Job::Unit(i)) && !b.waiting_for_ramp() && self.unit_craftsman(id, i) == figure && {
                        let r = self.tomb_unit_material(id, i).unwrap_or(STONE);
                        Monument::amount(&m.delivered, r) >= UNIT_MATERIAL
                    }
                })
                .map(Job::Unit),
            POLISH if figure == crate::monuments::STONEMASON && polished(style) => self.polish_frontier(id).into_iter().find(|&i| !taken.contains(&Job::Polish(i))).map(Job::Polish),
            _ => None,
        };
        job.or_else(|| {
            // The complex's parts go up one at a time, outward from the pyramid.
            if figure != crate::monuments::STONEMASON || m.phase == PREP {
                return None;
            }
            let next = m.parts.iter().position(|p| !p.built)?;
            (!taken.contains(&Job::Part(next))).then_some(Job::Part(next))
        })
    }

    /// Whether a tomb wants a craftsman of type `figure` at all.
    pub(crate) fn tomb_wants(&self, id: BuildingId, figure: u16) -> bool {
        let Some((style, _, m)) = self.tomb(id) else { return false };
        match m.phase {
            RAISE if figure == crate::monuments::CARPENTER => m.blocks.iter().any(Block::waiting_for_ramp),
            PREP => false,
            _ if figure == crate::monuments::STONEMASON && m.parts.iter().any(|p| !p.built) => true,
            RAISE => self.frontier(id).into_iter().any(|i| self.unit_craftsman(id, i) == figure),
            POLISH => figure == crate::monuments::STONEMASON && polished(style),
            _ => false,
        }
    }

    /// A craftsman finishes his work: the block's unit is laid, its casing polished a
    /// step, or its ramp built.
    pub(crate) fn finish_tomb_job(&mut self, id: BuildingId, job: Job) {
        let Some((style, var, _)) = self.tomb(id) else { return };
        let Some(m) = self.buildings.get_mut(id).and_then(|b| b.monument.as_mut()) else { return };
        match job {
            // (Its material was taken when the craftsman took on the unit.)
            Job::Unit(i) => {
                let Some(b) = m.blocks.get(i) else { return };
                let limit = course_limit(style, var, b, i);
                let b = &mut m.blocks[i];
                if b.counter >= limit && b.level < b.top {
                    b.level += 1;
                    b.counter = 1;
                } else {
                    b.counter += 1;
                }
                let limit = course_limit(style, var, b, i);
                if b.level == b.top && b.counter >= limit {
                    b.state = BUILT;
                }
            }
            Job::Polish(i) => {
                if let Some(b) = m.blocks.get_mut(i) {
                    b.counter = b.counter.saturating_sub(1);
                }
            }
            Job::Part(i) => {
                if let Some(p) = m.parts.get_mut(i) {
                    p.built = true;
                }
            }
            Job::Ramp(i) => {
                if let Some(b) = m.blocks.get_mut(i) {
                    b.ramp = true;
                    b.ramp_shown = true;
                }
                // A stepped pyramid's ramp spans the partner block too.
                if let Style::Pyramid(Family::Stepped) = style
                    && let Some(&(_, _, partner)) = RAMPS[var.min(4)].iter().find(|r| r.0 as usize == i)
                    && partner > 0
                    && let Some(p) = m.blocks.get_mut(partner as usize)
                {
                    p.ramp_shown = true;
                }
            }
        }
        self.refresh_monument_images(id);
    }

    /// Ticks a job takes.
    pub fn tomb_job_work(job: Job) -> u16 {
        match job {
            Job::Unit(_) => UNIT_WORK,
            Job::Polish(_) => POLISH_WORK,
            Job::Ramp(_) => RAMP_WORK,
            Job::Part(_) => PART_WORK,
        }
    }

    /// What a tomb wants dragged over: for each material the frontier's next units
    /// need, a sled while fewer units are paid for than blocks wait (never more
    /// than the rest of the tomb needs).
    pub(crate) fn tomb_material_wants(&self, id: BuildingId) -> Vec<(u16, i32)> {
        let Some((_, _, m)) = self.tomb(id) else { return Vec::new() };
        if m.phase != RAISE {
            return Vec::new();
        }
        let mut wants: Vec<(u16, i32)> = Vec::new();
        for i in self.frontier(id) {
            if m.blocks[i].waiting_for_ramp() {
                continue;
            }
            let Some(r) = self.tomb_unit_material(id, i) else { continue };
            match wants.iter_mut().find(|w| w.0 == r) {
                Some(w) => w.1 += UNIT_MATERIAL,
                None => wants.push((r, UNIT_MATERIAL)),
            }
        }
        let remaining = self.tomb_remaining(id);
        // Units being laid have had their material already.
        let mut in_hand: Vec<(u16, i32)> = Vec::new();
        for &(_, c) in &m.craftsmen {
            if let Some(Job::Unit(i)) = self.figures.get(c).and_then(|f| crate::monuments::decode_tomb_job(f.amount))
                && let Some(r) = self.tomb_unit_material(id, i)
            {
                Monument::add(&mut in_hand, r, UNIT_MATERIAL);
            }
        }
        wants
            .into_iter()
            .filter_map(|(r, want)| {
                let have = Monument::amount(&m.delivered, r) + Monument::amount(&m.in_flight, r);
                let rest = remaining.iter().find(|x| x.0 == r).map_or(0, |x| x.1) - Monument::amount(&in_hand, r) - have;
                let short = (want - have).min(rest);
                // A full sled where the rest of the tomb needs that much.
                (short > 0).then_some((r, crate::monuments::SLED_LOAD.min(rest)))
            })
            .collect()
    }

    /// Material the tomb's remaining units need, by resource.
    pub fn tomb_remaining(&self, id: BuildingId) -> Vec<(u16, i32)> {
        let Some((style, var, m)) = self.tomb(id) else { return Vec::new() };
        let (core, top) = materials(style);
        let mut out = vec![(core, 0)];
        if top != core {
            out.push((top, 0));
        }
        for (i, b) in m.blocks.iter().enumerate() {
            for level in 0..=b.top {
                let probe = Block { level, ..b.clone() };
                let limit = course_limit(style, var, &probe, i) as i32;
                let done = match b.state {
                    BUILT => limit,
                    BUILDING if level < b.level => limit,
                    BUILDING if level == b.level => (b.counter as i32).min(limit),
                    _ => 0,
                };
                let r = if level < b.top { core } else { top };
                if let Some(e) = out.iter_mut().find(|e| e.0 == r) {
                    e.1 += (limit - done) * UNIT_MATERIAL;
                }
            }
        }
        out
    }

    /// Site work for a laborer: the first tile (block * 4 + tile) of the step the
    /// whole site does next that nobody holds, nearest the centre (for the steps
    /// the original works outward) or the east edge (the digging and filling), or
    /// farthest from it (the draining). `N * 4` is the centre's foundation.
    pub(crate) fn tomb_site_job(&self, id: BuildingId, busy: &[i32]) -> Option<usize> {
        let (style, var, m) = self.tomb(id)?;
        if m.phase != PREP {
            return None;
        }
        let mastaba = matches!(style, Style::Mastaba);
        let step = m.blocks.iter().filter_map(|b| b.next_step(mastaba)).min();
        let Some(step) = step else {
            // Every block smooth: the foundation of the centre, once.
            let centre = m.blocks.len() * 4;
            return (!mastaba && m.blocks.iter().take(4).all(|b| b.state != FOUNDATION) && !busy.contains(&(centre as i32))).then_some(centre);
        };
        let east = [11usize, 27, 51, 83, 123][var.min(4)];
        let key = |i: usize| -> i64 {
            let (b, o) = (&m.blocks[i], &m.blocks[if mastaba { 0 } else { east.min(m.blocks.len() - 1) }]);
            let d = ((b.x - o.x).abs() + (b.y - o.y).abs()) as i64;
            match step {
                DUG | FILLED if !mastaba => d,
                DRAINED => -d,
                _ => {
                    let c = &m.blocks[0];
                    ((b.x - c.x).abs() + (b.y - c.y).abs()) as i64
                }
            }
        };
        let mut order: Vec<usize> = (0..m.blocks.len()).filter(|&i| m.blocks[i].next_step(mastaba) == Some(step)).collect();
        order.sort_by_key(|&i| key(i));
        for i in order {
            let b = &m.blocks[i];
            let fresh = b.mask == ALL_TILES;
            let tiles: &[usize] = if Block::whole(step) { &[0] } else { &[0, 1, 2, 3] };
            for &t in tiles {
                let unit = (i * 4 + t) as i32;
                let open = fresh || b.mask & (1 << t) == 0;
                if open && !busy.contains(&unit) {
                    return Some(unit as usize);
                }
            }
        }
        None
    }

    /// A laborer finishes a touch of the site.
    pub(crate) fn finish_site_touch(&mut self, id: BuildingId, unit: usize) {
        let Some((style, _, _)) = self.tomb(id) else { return };
        let mastaba = matches!(style, Style::Mastaba);
        let Some(m) = self.buildings.get_mut(id).and_then(|b| b.monument.as_mut()) else { return };
        let n = m.blocks.len();
        if unit == n * 4 {
            for b in m.blocks.iter_mut().take(4) {
                b.state = FOUNDATION;
                b.mask = ALL_TILES;
            }
        } else if let Some(b) = m.blocks.get_mut(unit / 4) {
            let Some(step) = b.next_step(mastaba) else { return };
            if b.mask == ALL_TILES {
                b.state = step;
                b.mask = 0;
            }
            b.mask |= if Block::whole(step) { ALL_TILES } else { 1 << (unit % 4) };
        }
        self.refresh_monument_images(id);
    }

    /// Daily: a prepared site starts building; a tomb whose last unit is laid (and,
    /// with a casing, polished) is finished. Returns whether it just finished.
    pub(crate) fn advance_tomb(&mut self, id: BuildingId) -> bool {
        let Some((style, _, m)) = self.tomb(id) else { return false };
        let mastaba = matches!(style, Style::Mastaba);
        match m.phase {
            PREP => {
                let smooth = m.blocks.iter().all(|b| b.next_step(mastaba).is_none());
                let founded = mastaba || m.blocks.iter().take(4).all(|b| b.state == FOUNDATION);
                if smooth && founded {
                    let m = self.buildings.get_mut(id).and_then(|b| b.monument.as_mut()).expect("tomb");
                    m.phase = RAISE;
                    for b in &mut m.blocks {
                        b.state = BUILDING;
                        b.mask = ALL_TILES;
                    }
                    self.refresh_monument_images(id);
                }
                false
            }
            RAISE if m.blocks.iter().all(|b| b.state == BUILT) && (polished(style) || m.parts.iter().all(|p| p.built)) => {
                let m = self.buildings.get_mut(id).and_then(|b| b.monument.as_mut()).expect("tomb");
                if polished(style) {
                    m.phase = POLISH;
                    // The ramps come down.
                    for b in &mut m.blocks {
                        b.ramp_shown = false;
                    }
                    self.refresh_monument_images(id);
                    false
                } else {
                    for b in &mut m.blocks {
                        b.ramp_shown = false;
                    }
                    true
                }
            }
            POLISH => m.blocks.iter().all(|b| b.counter == 0) && m.parts.iter().all(|p| p.built),
            _ => false,
        }
    }

    /// Test runs: puts a tomb at the start of stage `stage` (PREP, RAISE or POLISH;
    /// anything later finishes it), its blocks as the original leaves them there.
    pub fn set_tomb_stage(&mut self, id: BuildingId, stage: u8) {
        let Some((style, var, _)) = self.tomb(id) else { return };
        let Some(m) = self.buildings.get_mut(id).and_then(|b| b.monument.as_mut()) else { return };
        let fresh = layout(style, var);
        m.blocks = fresh;
        m.progress = vec![0; m.blocks.len() * 4 + 1];
        m.delivered.clear();
        m.phase = stage.min(POLISH);
        m.finished = stage > POLISH;
        for (i, b) in m.blocks.iter_mut().enumerate() {
            match stage {
                PREP => {}
                RAISE => b.state = BUILDING,
                _ => {
                    b.state = BUILT;
                    b.level = b.top;
                    b.counter = if stage > POLISH && polished(style) { 0 } else { course_limit(style, var, b, i) };
                    b.ramp = b.ramp_at > 0;
                }
            }
        }
        if stage > POLISH {
            m.parts.iter_mut().for_each(|p| p.built = true);
        }
        self.refresh_monument_images(id);
    }

    /// Test runs: a tomb's state in a line: stage, percent, blocks by state, the
    /// lowest and highest progress, ramps, parts, and material on site.
    pub fn tomb_summary(&self, id: BuildingId) -> Option<String> {
        let (_, _, m) = self.tomb(id)?;
        let count = |s: u8| m.blocks.iter().filter(|b| b.state == s).count();
        let prog: Vec<u32> = m.blocks.iter().filter(|b| b.state == BUILDING).map(Block::progress).collect();
        let ramps = (m.blocks.iter().filter(|b| b.ramp).count(), m.blocks.iter().filter(|b| b.ramp_at > 0).count());
        let parts = (m.parts.iter().filter(|p| p.built).count(), m.parts.len());
        Some(format!(
            "stage {} {}% finished {} building {} built {} prep {} progress {:?}..{:?} ramps {ramps:?} parts {parts:?} delivered {:?} in flight {:?} remaining {:?} polish left {:?}",
            m.phase,
            self.tomb_percent(id).unwrap_or(0),
            m.finished,
            count(BUILDING),
            count(BUILT),
            m.blocks.len() - count(BUILDING) - count(BUILT),
            prog.iter().min(),
            prog.iter().max(),
            m.delivered,
            m.in_flight,
            self.tomb_remaining(id),
            m.blocks.iter().enumerate().filter(|(_, b)| m.phase == POLISH && b.counter > 0).map(|(i, b)| (i, b.level, b.counter)).collect::<Vec<_>>(),
        ))
    }

    /// How far along a tomb is, 0-100: the site is the first 5%, then the units of
    /// building and polishing.
    pub fn tomb_percent(&self, id: BuildingId) -> Option<i32> {
        let (style, var, m) = self.tomb(id)?;
        if m.finished {
            return Some(100);
        }
        let n = m.blocks.len().max(1) as i32;
        if m.phase == PREP {
            let mastaba = matches!(style, Style::Mastaba);
            let steps = if mastaba { 2 } else { 7 };
            let done: i32 = m.blocks.iter().map(|b| if b.state >= CLEARED { ((b.state - CLEARED) as i32 + (b.mask == ALL_TILES) as i32).min(steps) } else { 0 }).sum();
            return Some(done * 5 / (n * steps));
        }
        let (mut total, mut done) = (0, 0);
        for (i, b) in m.blocks.iter().enumerate() {
            let (t, d) = units(style, var, b, i);
            total += t;
            done += d;
            if polished(style) {
                // The top course is polished unit by unit.
                let probe = Block { level: b.top, ..b.clone() };
                let limit = course_limit(style, var, &probe, i) as u32;
                total += limit;
                if m.phase == POLISH {
                    done += limit - (b.counter as u32).min(limit);
                }
            }
        }
        Some(5 + (95 * done / total.max(1)) as i32)
    }

    /// Each tile of a tomb's site and the image it shows, from its block's state.
    pub(crate) fn tomb_site(&self, id: BuildingId) -> Vec<((i32, i32), u32)> {
        let mut out = Vec::new();
        let Some(b) = self.buildings.get(id) else { return out };
        let Some((style, _, m)) = self.tomb(id) else { return out };
        let Some(site) = self.defs.building(b.kind).map(|d| d.image) else { return out };
        let mastaba = matches!(style, Style::Mastaba);
        let (w, h) = b.footprint();
        let (x0, y0, x1, y1) = (0, 0, w - 1, h - 1);
        // Each tile's site step: its block's, if the tile has had it, else the one before.
        let step_at = |x: i32, y: i32| -> Option<u8> {
            let bl = m.blocks.iter().find(|bl| (bl.x..bl.x + 2).contains(&x) && (bl.y..bl.y + 2).contains(&y))?;
            let t = ((y - bl.y) * 2 + (x - bl.x)) as u8;
            Some(match bl.state {
                BUILDING | BUILT => if mastaba { SANDED } else { SMOOTH },
                s if bl.mask & (1 << t) != 0 => s,
                CLEARED => 2,
                s => s - 1,
            })
        };
        let dug = |x: i32, y: i32| step_at(x, y).is_some_and(|s| s >= DUG);
        for y in y0..=y1 {
            for x in x0..=x1 {
                let Some(step) = step_at(x, y) else { continue };
                let piece = Self::trench_piece(dug(x, y - 1), dug(x + 1, y), dug(x, y + 1), dug(x - 1, y));
                let bare = m.ground.get((y * w + x) as usize).copied().filter(|&g| g != 0);
                let image = match step {
                    s if s < SANDED => bare.unwrap_or(site + 5),
                    SANDED => Self::foundation(site, (x, y), (x0, y0), (x1, y1)),
                    DUG | DRAINED => site + 14 + piece,
                    FLOODED => site + 23 + piece,
                    FILLED => site + 32 + piece,
                    _ => site + 41 + piece,
                };
                out.push(((x, y), image));
            }
        }
        // A complex's parts are staked out until built.
        for p in m.parts.iter().filter(|p| !p.built) {
            for (x, y) in [(p.x, p.y), (p.x + 1, p.y), (p.x, p.y + 1), (p.x + 1, p.y + 1)] {
                out.push(((x, y), site + 13));
            }
        }
        out
    }

    /// The stakes at a tomb site's four outer corners, until they are cleared.
    pub(crate) fn tomb_stakes(&self, id: BuildingId) -> Vec<(i32, i32, u32)> {
        let Some(b) = self.buildings.get(id) else { return Vec::new() };
        let Some((_, _, m)) = self.tomb(id) else { return Vec::new() };
        if m.phase != PREP {
            return Vec::new();
        }
        let Some(site) = self.defs.building(b.kind).map(|d| d.image) else { return Vec::new() };
        let (w, h) = b.footprint();
        [(0, 0), (w - 1, 0), (0, h - 1), (w - 1, h - 1)]
            .into_iter()
            .filter(|&(x, y)| {
                m.blocks
                    .iter()
                    .find(|bl| (bl.x..bl.x + 2).contains(&x) && (bl.y..bl.y + 2).contains(&y))
                    .is_some_and(|bl| bl.state < CLEARED || bl.state == CLEARED && bl.mask & (1 << ((y - bl.y) * 2 + (x - bl.x))) == 0)
            })
            .map(|(x, y)| (b.x + x, b.y + y, site + 13))
            .collect()
    }

    /// Each block's sprite: (block x, block y, image, lift in pixels). Blocks not
    /// yet begun show the site (the centre its foundation); lift 0 goes on the
    /// map, higher courses are drawn raised.
    pub(crate) fn tomb_blocks(&self, id: BuildingId) -> Vec<(i32, i32, u32, i32)> {
        let mut out = Vec::new();
        let Some(bld) = self.buildings.get(id) else { return out };
        let Some((style, var, m)) = self.tomb(id) else { return out };
        let Some(bdef) = self.defs.building(bld.kind) else { return out };
        let site = bdef.image;
        let Some(base) = bdef.anims.get("blocks").map(|a| a.image) else { return out };
        let half = bdef.anims.get("half").map(|a| a.image);
        let mastaba = matches!(style, Style::Mastaba);
        let bent = matches!(style, Style::Pyramid(Family::Bent));
        let raise = |level: u8| -> i32 {
            if bent {
                BENT_RAISE[var.min(1)].get(level as usize).copied().unwrap_or(level as i32 * COURSE_RAISE)
            } else {
                level as i32 * COURSE_RAISE
            }
        };
        let half_course = |level: u8| bent && BENT_HALF[var.min(1)].get(level as usize).copied().unwrap_or(false);
        let (x0, y0) = (bld.x, bld.y);
        for (i, b) in m.blocks.iter().enumerate() {
            let (bx, by) = (x0 + b.x, y0 + b.y);
            let rand = (self.map.random.at_or(bx, by, 0) & 1) as u32;
            if m.phase == PREP {
                if !mastaba && i < 4 && b.state == FOUNDATION {
                    out.push((bx, by, site + 50 + [2, 0, 1, 3][i], 0));
                }
                continue;
            }
            if b.state == BUILDING && b.counter == 0 {
                if !mastaba && i < 4 {
                    out.push((bx, by, site + 50 + [2, 0, 1, 3][i], 0));
                }
                continue;
            }
            let kind = b.kind as u32;
            let top = b.level == b.top;
            let image = if m.phase == POLISH || (m.finished && polished(style)) {
                let c = if m.finished { 0 } else { b.counter as u32 };
                match half {
                    Some(h) if half_course(b.level) => h + (5 - c.min(5)) * 8 + kind,
                    _ => base + (11 - c.min(6)) * 8 + kind,
                }
            } else if mastaba && kind == 8 || !top {
                base + 95 + b.counter.clamp(1, 6) as u32 + 6 * rand
            } else if mastaba && kind >= 9 {
                base + 109 + (kind & 1)
            } else if half_course(b.level) {
                half.map_or(base, |h| h + (b.counter.clamp(1, 3) as u32 - 1) * 8 + kind)
            } else {
                base + (b.counter.clamp(1, 6) as u32 - 1) * 8 + kind
            };
            // A large mastaba's second course stands on a full first.
            if mastaba && var == 2 && b.level >= 1 && MASTABA_FILLER.contains(&i) {
                out.push((bx, by, base + 101 + 6 * rand, 0));
            }
            out.push((bx, by, image, raise(b.level)));
        }
        // A complex's temples and causeway once built (each 2x2).
        for p in m.parts.iter().filter(|p| p.built) {
            let key = ["mortuary", "causeway", "valley"][p.kind.min(2) as usize];
            if let Some(a) = bdef.anims.get(key) {
                out.push((x0 + p.x, y0 + p.y, a.image, 0));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placement_tables_match_the_original() {
        let pyramid = Style::Pyramid(Family::True);
        // The anchor block sits so that the footprint spans -a .. size-a (0x56fe30).
        assert_eq!((0..5).map(|v| anchor(pyramid, v)).collect::<Vec<_>>(), [(2, 2), (4, 4), (6, 6), (8, 8), (10, 10)]);
        assert_eq!((0..3).map(|v| anchor(Style::Mastaba, v)).collect::<Vec<_>>(), [(2, 4), (4, 6), (6, 8)]);
        // The row past the south edge starts at (-2,6) .. (-10,14) for pyramids and
        // (-2,6) .. (-6,10) for mastabas (0x5700b4, 0x5700c8): one past the footprint.
        for (v, size) in [8, 12, 16, 20, 24].into_iter().enumerate() {
            let (ax, ay) = anchor(pyramid, v);
            assert_eq!((-ax, size - ay), (-2 - 2 * v as i32, 6 + 2 * v as i32));
        }
        for (v, (w, h)) in [(4, 10), (6, 14), (8, 18)].into_iter().enumerate() {
            let (ax, ay) = anchor(Style::Mastaba, v);
            assert_eq!((-ax, h - ay, w), (-2 - 2 * v as i32, 6 + 2 * v as i32, 4 + 2 * v as i32));
        }
        // The mortuary temple is just past the east face, on its middle rows.
        assert_eq!((complex_start(pyramid, 2), complex_start(pyramid, 3), complex_start(pyramid, 4)), (None, Some((12, 1)), Some((14, 1))));
        assert_eq!(complex_start(Style::Mastaba, 2), None);
    }
}
