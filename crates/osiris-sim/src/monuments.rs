//! Monuments. A monument is laid out on a grid of 2x2 blocks and built in phases.
//! The first two phases level the site: laborers from work camps each work a block
//! until it is done. Every later phase adds a course of material: storage yards drag
//! it over on sleds (at most 400 units a sled, never more than the course still
//! needs), and a guild's mason, waiting on site, lays it block by block, as far as the
//! delivered material allows. Courses that use timber for ramps also need a carpenter
//! on site. When every block of a phase is done, the next phase begins; after the
//! last, the monument is complete.
//!
//! Mastabas are brick: a bricklayer lays six courses of bricks (with clay from the
//! second course on). Stepped pyramids are stone: after five foundation courses the
//! pyramid rises in rings, each ring a block in from the last, six courses to a ring.

use crate::buildings::{BuildingId, kind};
use crate::figures::{FigureId, Step, Travel};
use crate::world::World;

/// Progress a block needs in each phase: one worker adds one point a tick.
pub const BLOCK_WORK: u16 = 200;
/// Most a sled carries.
pub const SLED_LOAD: i32 = 400;
/// Men pulling each sled.
const SLED_PULLERS: usize = 6;
/// Height one ring of a stepped pyramid adds, in pixels (six courses of 15).
pub const RING_LIFT: i32 = 90;
const COURSES_PER_RING: i32 = 6;

pub const CARPENTER: u16 = 79;
pub const BRICKLAYER: u16 = 80;
pub const STONEMASON: u16 = 81;
pub const SLED: u16 = 86;
pub const SLED_PULLER: u16 = 96;

const CLAY: u16 = 11;
const BRICKS: u16 = 12;
const TIMBER: u16 = 20;
const STONE: u16 = 24;
const LIMESTONE: u16 = 25;

pub const SMALL_BENT_PYRAMID: u16 = 241;
pub const MEDIUM_BENT_PYRAMID: u16 = 242;
pub const SMALL_MUDBRICK_PYRAMID: u16 = 243;
pub const MEDIUM_MUDBRICK_PYRAMID: u16 = 244;
pub const LARGE_MUDBRICK_PYRAMID: u16 = 245;
pub const LARGE_STEPPED_PYRAMID: u16 = 250;
pub const SMALL_PYRAMID: u16 = 253;
pub const MEDIUM_PYRAMID: u16 = 254;
pub const LARGE_PYRAMID: u16 = 255;
pub const SMALL_STEPPED_PYRAMID: u16 = 319;
pub const MEDIUM_STEPPED_PYRAMID: u16 = 324;

/// What a 2x2 block of a mastaba is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Block {
    Main,
    Side,
    Wall,
    Entrance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    /// Brick courses; the entrance in the last column of `entrance_row`, `side_rows`
    /// rows of side blocks at the end, and the first row's last block a side block
    /// when `first_row_side`.
    Mastaba { entrance_row: i32, side_rows: i32, first_row_side: bool },
    Pyramid(Family),
}

/// Pyramids: stepped pyramids are plain stone; bent and true pyramids are stone and
/// mudbrick pyramids brick, all three cased in limestone and polished at the end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    Stepped,
    Bent,
    True,
    Mudbrick,
}

/// One monument type: its block grid when facing north, and its phases.
pub struct MonumentDef {
    pub kind: u16,
    /// Blocks across and down.
    pub cols: i32,
    pub rows: i32,
    pub style: Style,
    /// Phases, the last being completion.
    pub phase_count: u8,
    /// Final phases that only polish the casing.
    pub polish: u8,
    /// Mastaba material per phase: (resource, units).
    mastaba_phases: &'static [&'static [(u16, i32)]],
    /// The monument's worth toward the monument rating.
    pub weight: i32,
    /// Its name (text group 198).
    pub title: usize,
}

const SMALL_MASTABA_PHASES: [&[(u16, i32)]; 9] = [
    &[],
    &[],
    &[(BRICKS, 4800)],
    &[(CLAY, 2000), (BRICKS, 4000)],
    &[(CLAY, 1600), (BRICKS, 3200)],
    &[(CLAY, 1200), (BRICKS, 2400)],
    &[(CLAY, 800), (BRICKS, 1600)],
    &[(CLAY, 400), (BRICKS, 800)],
    &[],
];
const MEDIUM_MASTABA_PHASES: [&[(u16, i32)]; 9] = [
    &[],
    &[],
    &[(BRICKS, 8000)],
    &[(CLAY, 4000), (BRICKS, 8000)],
    &[(CLAY, 3200), (BRICKS, 6400)],
    &[(CLAY, 2400), (BRICKS, 4800)],
    &[(CLAY, 1600), (BRICKS, 3200)],
    &[(CLAY, 800), (BRICKS, 1600)],
    &[],
];
const LARGE_MASTABA_PHASES: [&[(u16, i32)]; 9] = [
    &[],
    &[],
    &[(BRICKS, 13600)],
    &[(CLAY, 6800), (BRICKS, 13600)],
    &[(CLAY, 5600), (BRICKS, 10800)],
    &[(CLAY, 4000), (BRICKS, 8400)],
    &[(CLAY, 2800), (BRICKS, 5600)],
    &[(CLAY, 1400), (BRICKS, 2800)],
    &[],
];

/// A pyramid's foundation courses (phases 2-6), in its core material; timber builds
/// the ramps from the second.
const PYRAMID_FOUNDATION: [(i32, i32); 5] = [(0, 4800), (2000, 4000), (1600, 3200), (1200, 2400), (800, 1600)];
/// Every later course: timber and core material.
const PYRAMID_COURSE: (i32, i32) = (400, 800);
/// Limestone casing on a rising course, from the first course to the last.
const CASING: (i32, i32) = (1200, 200);
/// Phases before a pyramid's courses start rising.
const PYRAMID_BASE_PHASES: u8 = 7;

const fn mastaba(kind: u16, (cols, rows): (i32, i32), style: Style, phases: &'static [&'static [(u16, i32)]], weight: i32, title: usize) -> MonumentDef {
    MonumentDef { kind, cols, rows, style, phase_count: phases.len() as u8, polish: 0, mastaba_phases: phases, weight, title }
}

/// A pyramid `blocks` across whose last phase is `last`, the final `polish` of them
/// polishing. (Phase counts from Akhenaten's reconstruction; the rating weights of
/// pyramids other than stepped ones are estimates.)
const fn pyramid(kind: u16, family: Family, blocks: i32, last: u8, polish: u8, weight: i32, title: usize) -> MonumentDef {
    MonumentDef { kind, cols: blocks, rows: blocks, style: Style::Pyramid(family), phase_count: last + 1, polish, mastaba_phases: &[], weight, title }
}

pub const MONUMENTS: [MonumentDef; 14] = [
    mastaba(kind::SMALL_MASTABA, (2, 5), Style::Mastaba { entrance_row: 2, side_rows: 1, first_row_side: false }, &SMALL_MASTABA_PHASES, 2, 18),
    mastaba(kind::MEDIUM_MASTABA, (3, 7), Style::Mastaba { entrance_row: 3, side_rows: 2, first_row_side: true }, &MEDIUM_MASTABA_PHASES, 2, 19),
    mastaba(kind::LARGE_MASTABA, (4, 9), Style::Mastaba { entrance_row: 4, side_rows: 3, first_row_side: true }, &LARGE_MASTABA_PHASES, 3, 20),
    pyramid(SMALL_STEPPED_PYRAMID, Family::Stepped, 4, 24, 0, 8, 8),
    pyramid(MEDIUM_STEPPED_PYRAMID, Family::Stepped, 6, 32, 0, 16, 9),
    pyramid(LARGE_STEPPED_PYRAMID, Family::Stepped, 10, 36, 0, 24, 10),
    pyramid(SMALL_BENT_PYRAMID, Family::Bent, 4, 24, 2, 12, 1),
    pyramid(MEDIUM_BENT_PYRAMID, Family::Bent, 6, 32, 3, 20, 2),
    pyramid(SMALL_MUDBRICK_PYRAMID, Family::Mudbrick, 4, 26, 2, 12, 3),
    pyramid(MEDIUM_MUDBRICK_PYRAMID, Family::Mudbrick, 6, 35, 3, 20, 4),
    pyramid(LARGE_MUDBRICK_PYRAMID, Family::Mudbrick, 8, 41, 4, 28, 5),
    pyramid(SMALL_PYRAMID, Family::True, 4, 27, 2, 16, 13),
    pyramid(MEDIUM_PYRAMID, Family::True, 6, 36, 3, 28, 14),
    pyramid(LARGE_PYRAMID, Family::True, 10, 42, 5, 40, 15),
];

pub fn monument_def(k: u16) -> Option<&'static MonumentDef> {
    MONUMENTS.iter().find(|m| m.kind == k)
}

/// Phase, finished, and (resource, delivered, needed) for the phase's materials.
pub type MonumentStatus = (u8, bool, Vec<(u16, i32, i32)>);

/// Phases that level the site.
const LEVELING_PHASES: u8 = 2;
/// Laborers one monument takes at a time.
const MAX_LABORERS: usize = 5;

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Monument {
    pub phase: u8,
    /// Material delivered for the current phase, and on its way.
    pub delivered: Vec<(u16, i32)>,
    pub in_flight: Vec<(u16, i32)>,
    /// Work done on each block this phase.
    pub progress: Vec<u16>,
    /// Guild craftsmen on site: (figure type, figure).
    #[serde(default)]
    pub craftsmen: Vec<(u16, FigureId)>,
    pub finished: bool,
}

impl Monument {
    fn amount(list: &[(u16, i32)], r: u16) -> i32 {
        list.iter().filter(|e| e.0 == r).map(|e| e.1).sum()
    }

    fn add(list: &mut Vec<(u16, i32)>, r: u16, n: i32) {
        match list.iter_mut().find(|e| e.0 == r) {
            Some(e) => e.1 += n,
            None => list.push((r, n)),
        }
        list.retain(|e| e.1 != 0);
    }

    pub fn has_craftsman(&self, figure: u16) -> bool {
        self.craftsmen.iter().any(|c| c.0 == figure)
    }
}

impl MonumentDef {
    /// The phase after the last rising course: polishing, or completion.
    fn courses_end(&self) -> u8 {
        self.phase_count - 1 - self.polish
    }

    /// The material phase `p` needs.
    pub fn phase(&self, p: u8) -> Vec<(u16, i32)> {
        let family = match self.style {
            Style::Mastaba { .. } => return self.mastaba_phases.get(p as usize).map_or_else(Vec::new, |m| m.to_vec()),
            Style::Pyramid(f) => f,
        };
        if p < LEVELING_PHASES || p >= self.courses_end() {
            return Vec::new();
        }
        let core = if family == Family::Mudbrick { BRICKS } else { STONE };
        let (timber, amount) = if p < PYRAMID_BASE_PHASES { PYRAMID_FOUNDATION[(p - LEVELING_PHASES) as usize] } else { PYRAMID_COURSE };
        let mut out = Vec::new();
        if timber > 0 {
            out.push((TIMBER, timber));
        }
        out.push((core, amount));
        if p >= PYRAMID_BASE_PHASES && family != Family::Stepped {
            // The casing thins as the pyramid narrows.
            let courses = (self.courses_end() - PYRAMID_BASE_PHASES).max(1) as i32;
            let k = (p - PYRAMID_BASE_PHASES) as i32;
            let casing = CASING.0 - (CASING.0 - CASING.1) * k / (courses - 1).max(1);
            out.push((LIMESTONE, casing / 100 * 100));
        }
        out
    }

    /// The craftsmen phase `p` needs on site, the one who lays the blocks first:
    /// bricklayers for bricks (and the clay that binds them), stonemasons for stone
    /// and limestone and for polishing, carpenters for the timber ramps.
    pub fn crew(&self, p: u8) -> Vec<u16> {
        let phase = self.phase(p);
        let has = |r: u16| phase.iter().any(|e| e.0 == r);
        let mut crew = Vec::new();
        if has(BRICKS) || has(CLAY) {
            crew.push(BRICKLAYER);
        }
        let polishing = matches!(self.style, Style::Pyramid(_)) && p >= self.courses_end() && p + 1 < self.phase_count;
        if has(STONE) || has(LIMESTONE) || polishing {
            crew.push(STONEMASON);
        }
        if has(TIMBER) {
            crew.push(CARPENTER);
        }
        crew
    }

    fn block(&self, c: i32, r: i32) -> Block {
        let Style::Mastaba { entrance_row, side_rows, first_row_side } = self.style else { return Block::Wall };
        if r == 0 {
            return if first_row_side && c == self.cols - 1 { Block::Side } else { Block::Main };
        }
        if r >= self.rows - side_rows {
            return Block::Side;
        }
        if r == entrance_row && c == self.cols - 1 {
            return Block::Entrance;
        }
        Block::Wall
    }

    /// What the current phase still needs of `r`, counting what is on its way.
    fn needs(&self, m: &Monument, r: u16) -> i32 {
        let want = Monument::amount(&self.phase(m.phase), r);
        want - Monument::amount(&m.delivered, r) - Monument::amount(&m.in_flight, r)
    }

    /// How many of the phase's blocks the delivered material pays for.
    fn blocks_paid(&self, m: &Monument) -> usize {
        let blocks = (self.cols * self.rows) as usize;
        self.phase(m.phase)
            .iter()
            .map(|&(r, want)| (Monument::amount(&m.delivered, r) as i64 * blocks as i64 / want.max(1) as i64) as usize)
            .min()
            .unwrap_or(blocks)
            .min(blocks)
    }

    /// Rings of a stepped pyramid.
    fn rings(&self) -> i32 {
        (self.cols.min(self.rows) + 1) / 2
    }
}

impl World {
    /// The footprint of monument type `k` placed facing north.
    pub fn monument_footprint(&self, k: u16) -> Option<(i32, i32)> {
        monument_def(k).map(|d| (d.cols * 2, d.rows * 2))
    }

    /// Lays out a new monument: its footprint and the staked-out site.
    pub(crate) fn place_monument(&mut self, id: BuildingId) {
        let Some(b) = self.buildings.get_mut(id) else { return };
        let Some(def) = monument_def(b.kind) else { return };
        b.monument = Some(Monument { progress: vec![0; (def.cols * def.rows) as usize], ..Default::default() });
        self.refresh_monument_images(id);
    }

    fn levelled_ground(site: u32, x: i32, y: i32) -> u32 {
        site + 41 + ((x * 3 + y) % 9) as u32
    }

    /// Redraws a monument's ground-level tiles for its phase and the blocks done so far.
    pub fn refresh_monument_images(&mut self, id: BuildingId) {
        let Some(b) = self.buildings.get(id) else { return };
        let Some(def) = monument_def(b.kind) else { return };
        let Some(m) = b.monument.as_ref() else { return };
        let (phase, finished) = (m.phase, m.finished);
        let (x0, y0) = (b.x, b.y);
        let (w, h) = b.footprint();
        let Some(bdef) = self.defs.building(b.kind) else { return };
        let site = bdef.image;
        if phase < LEVELING_PHASES && !finished {
            // Staked-out ground, with the corners and edges marked.
            let (x1, y1) = (x0 + w - 1, y0 + h - 1);
            for y in y0..=y1 {
                for x in x0..=x1 {
                    let inside = x > x0 && x < x1 || y > y0 && y < y1;
                    let image = if phase == 1 {
                        Self::levelled_ground(site, x, y)
                    } else if (x, y) == (x0, y0) {
                        site
                    } else if (x, y) == (x0, y1) {
                        site - 2
                    } else if (x, y) == (x1, y1) {
                        site - 4
                    } else if (x, y) == (x1, y0) {
                        site - 6
                    } else if x == x0 {
                        site - 1
                    } else if y == y1 {
                        site - 3
                    } else if x == x1 {
                        site - 5
                    } else if y == y0 && inside {
                        site - 7
                    } else {
                        site + 5 + ((x + y) % 7) as u32
                    };
                    self.map.set_single_image(x, y, image);
                }
            }
            return;
        }
        // Each block shows the course it has reached; the rest is levelled ground.
        let stacks = self.monument_stacks(id);
        for r in 0..def.rows {
            for c in 0..def.cols {
                let (bx, by) = (x0 + c * 2, y0 + r * 2);
                match stacks.iter().find(|s| (s.0, s.1, s.3) == (bx, by, 0)) {
                    Some(&(_, _, image, _)) => self.map.set_footprint(bx, by, 2, image),
                    None => {
                        for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                            self.map.set_single_image(bx + dx, by + dy, Self::levelled_ground(site, bx + dx, by + dy));
                        }
                    }
                }
            }
        }
    }

    /// The images a monument's blocks show above the levelled site: (block x, block y,
    /// image, lift in pixels). Lift 0 is the block's ground image; a pyramid's blocks
    /// stack the finished rings under the ring being laid.
    pub fn monument_stacks(&self, id: BuildingId) -> Vec<(i32, i32, u32, i32)> {
        let mut out = Vec::new();
        let Some(b) = self.buildings.get(id) else { return out };
        let Some(def) = monument_def(b.kind) else { return out };
        let Some(m) = &b.monument else { return out };
        if m.phase < LEVELING_PHASES && !m.finished {
            return out;
        }
        let Some(bdef) = self.defs.building(b.kind) else { return out };
        let img = |key: &str| bdef.anims.get(key).map_or(0, |a| a.image);
        let done = |i: usize| m.finished || m.progress.get(i).copied().unwrap_or(0) >= BLOCK_WORK;
        match def.style {
            Style::Mastaba { .. } => {
                let bricks = img("base_bricks");
                let course = |phase: u8| phase as i32 - LEVELING_PHASES as i32 + 1;
                for r in 0..def.rows {
                    for c in 0..def.cols {
                        let i = (r * def.cols + c) as usize;
                        let layer = if m.finished {
                            course(def.phase_count - 2)
                        } else if done(i) {
                            course(m.phase)
                        } else {
                            course(m.phase) - 1
                        };
                        if layer <= 0 {
                            continue;
                        }
                        let l = (layer - 1) as u32;
                        let image = match def.block(c, r) {
                            Block::Entrance => bricks + 110,
                            _ if r == 0 => bricks + l * 8 + 7,
                            _ if r == def.rows - 1 => bricks + l * 8 + 5,
                            _ => bricks + 96 + l,
                        };
                        out.push((b.x + c * 2, b.y + r * 2, image, 0));
                    }
                }
            }
            Style::Pyramid(family) => {
                let (corner, wall, cube) = (img("corner_bricks"), img("wall_bricks"), img("base_bricks"));
                let courses = def.rings() * COURSES_PER_RING;
                let rising = (def.courses_end() - PYRAMID_BASE_PHASES) as i32;
                let topped = m.finished || m.phase >= def.courses_end();
                // Casing goes on in the polishing phases: six stages of white limestone.
                let cased = family != Family::Stepped;
                let casing = |i: usize| -> u32 {
                    if !cased || !topped {
                        return 0;
                    }
                    if m.finished {
                        return COURSES_PER_RING as u32;
                    }
                    let k = (m.phase - def.courses_end()) as i32 + done(i) as i32;
                    (k * COURSES_PER_RING / def.polish.max(1) as i32) as u32
                };
                // The last course laid once phase `p` is done, spreading the rising
                // phases evenly over the courses.
                let course_after = |p: u8| (p as i32 - PYRAMID_BASE_PHASES as i32 + 1) * courses / rising.max(1) - 1;
                for r in 0..def.rows {
                    for c in 0..def.cols {
                        let i = (r * def.cols + c) as usize;
                        let (bx, by) = (b.x + c * 2, b.y + r * 2);
                        let reached = if topped {
                            courses - 1
                        } else if m.phase < PYRAMID_BASE_PHASES {
                            // The foundation: a floor of stone once the block's first course is laid.
                            if m.phase > LEVELING_PHASES || done(i) {
                                out.push((bx, by, cube, 0));
                            }
                            continue;
                        } else if done(i) {
                            course_after(m.phase)
                        } else {
                            course_after(m.phase - 1)
                        };
                        if reached < 0 {
                            out.push((bx, by, cube, 0));
                            continue;
                        }
                        // Blocks further in rise with the higher rings.
                        let d = c.min(r).min(def.cols - 1 - c).min(def.rows - 1 - r);
                        let ring = (reached / COURSES_PER_RING).min(d);
                        for below in 0..ring {
                            out.push((bx, by, cube + (COURSES_PER_RING - 1) as u32, below * RING_LIFT));
                        }
                        let course = if ring < reached / COURSES_PER_RING { COURSES_PER_RING - 1 } else { reached % COURSES_PER_RING } as u32;
                        // A cased edge shows its casing stage instead (courses 6-11).
                        let edge = match casing(i) {
                            0 => course,
                            stage => COURSES_PER_RING as u32 - 1 + stage,
                        };
                        let (lo, hi_c, hi_r) = (ring, def.cols - 1 - ring, def.rows - 1 - ring);
                        let (west, east, north, south) = (c == lo, c == hi_c, r == lo, r == hi_r);
                        let image = match (west || east, north || south) {
                            (true, true) => {
                                let v = match (west, north) {
                                    (true, true) => 0,
                                    (true, false) => 1,
                                    (false, false) => 2,
                                    (false, true) => 3,
                                };
                                corner + edge * 8 + v
                            }
                            (true, false) => wall + edge * 8 + if west { 0 } else { 2 },
                            (false, true) => wall + edge * 8 + if north { 3 } else { 1 },
                            (false, false) => cube + course,
                        };
                        out.push((bx, by, image, ring * RING_LIFT));
                    }
                }
            }
        }
        out
    }

    /// The monument's blocks that still want laborers, craftsmen or material.
    fn active_monuments(&self) -> Vec<BuildingId> {
        self.buildings.iter().filter(|b| b.monument.as_ref().is_some_and(|m| !m.finished)).map(|b| b.id).collect()
    }

    /// A tile beside monument `id` for walkers to stand on, nearest to `from`.
    pub fn monument_access(&self, id: BuildingId, from: (i32, i32)) -> Option<(i32, i32)> {
        let b = self.buildings.get(id)?;
        let (w, h) = b.footprint();
        let mut best: Option<((i32, i32), i32)> = None;
        for y in b.y - 1..=b.y + h {
            for x in b.x - 1..=b.x + w {
                let on_ring = x == b.x - 1 || y == b.y - 1 || x == b.x + w || y == b.y + h;
                if !on_ring || !crate::figures::passable(&self.map, Travel::Land, x, y) {
                    continue;
                }
                let d = (x - from.0).abs() + (y - from.1).abs();
                if best.is_none_or(|(_, bd)| d < bd) {
                    best = Some(((x, y), d));
                }
            }
        }
        best.map(|(p, _)| p)
    }

    /// A monument block a work-camp laborer could level: the first block without a
    /// laborer on it, of the nearest monument still being levelled.
    pub(crate) fn leveling_job(&self, from: (i32, i32)) -> Option<(BuildingId, usize)> {
        let busy: Vec<(u32, i32)> = self.figures.iter().filter(|f| f.kind == crate::farms::PEASANT && f.action >= 3).map(|f| (f.target, f.amount)).collect();
        self.active_monuments()
            .into_iter()
            .filter_map(|id| {
                let b = self.buildings.get(id)?;
                let m = b.monument.as_ref()?;
                if m.phase >= LEVELING_PHASES || busy.iter().filter(|b| b.0 == id).count() >= MAX_LABORERS {
                    return None;
                }
                let block = m.progress.iter().enumerate().position(|(i, &p)| p < BLOCK_WORK && !busy.contains(&(id, i as i32)))?;
                Some(((b.x - from.0).abs() + (b.y - from.1).abs(), id, block))
            })
            .min()
            .map(|(_, id, block)| (id, block))
    }

    /// The next block of monument `id` that no other laborer is levelling.
    pub(crate) fn next_leveling_block(&self, id: BuildingId, me: FigureId) -> Option<usize> {
        let m = self.buildings.get(id)?.monument.as_ref()?;
        if m.phase >= LEVELING_PHASES {
            return None;
        }
        let busy: Vec<i32> = self.figures.iter().filter(|f| f.kind == crate::farms::PEASANT && f.action == 4 && f.target == id && f.id != me).map(|f| f.amount).collect();
        m.progress.iter().enumerate().position(|(i, &p)| p < BLOCK_WORK && !busy.contains(&(i as i32)))
    }

    /// A laborer at a monument works its block; true when the block is done.
    pub(crate) fn level_block(&mut self, id: BuildingId, block: usize) -> bool {
        let Some(m) = self.buildings.get_mut(id).and_then(|b| b.monument.as_mut()) else { return true };
        if m.phase >= LEVELING_PHASES {
            return true;
        }
        let Some(p) = m.progress.get_mut(block) else { return true };
        *p = (*p + 1).min(BLOCK_WORK);
        *p >= BLOCK_WORK
    }

    /// Whether monument `id`'s current phase wants craftsman `figure` on site.
    fn wants_craftsman(&self, id: BuildingId, figure: u16) -> bool {
        let Some(b) = self.buildings.get(id) else { return false };
        let Some(def) = monument_def(b.kind) else { return false };
        let Some(m) = &b.monument else { return false };
        if m.finished || m.phase < LEVELING_PHASES {
            return false;
        }
        def.crew(m.phase).contains(&figure)
    }

    /// Tick 31: guilds send their craftsman to a monument that needs one.
    pub(crate) fn guild_walkers(&mut self) {
        let guilds: Vec<(BuildingId, u16)> = self
            .buildings
            .iter()
            .filter_map(|b| match b.kind {
                kind::BRICKLAYERS_GUILD => Some((b.id, BRICKLAYER)),
                kind::STONEMASONS_GUILD => Some((b.id, STONEMASON)),
                kind::CARPENTERS_GUILD => Some((b.id, CARPENTER)),
                _ => None,
            })
            .collect();
        for (g, figure) in guilds {
            let Some(gb) = self.buildings.get(g) else { continue };
            let Some(road) = gb.road else { continue };
            if gb.workers <= 0 || gb.walkers[0] != 0 {
                continue;
            }
            // The bricklayers keep a load of bricks to work with.
            if figure == BRICKLAYER && gb.stock.get(BRICKS as usize).copied().unwrap_or(0) < crate::economy::LOAD {
                continue;
            }
            let from = (gb.x, gb.y);
            let target = self.active_monuments().into_iter().find(|&id| {
                self.wants_craftsman(id, figure) && !self.buildings.get(id).and_then(|b| b.monument.as_ref()).is_some_and(|m| m.has_craftsman(figure))
            });
            let Some(target) = target else { continue };
            let Some(spot) = self.monument_access(target, from) else { continue };
            let fid = self.figures.spawn(figure, road.0, road.1, Travel::Land);
            let map = &self.map;
            if let Some(f) = self.figures.get_mut(fid) {
                f.home = g;
                f.target = target;
                f.action = 1;
                if !f.go_to(map, spot) {
                    f.dead = true;
                    continue;
                }
            }
            self.buildings.get_mut(g).expect("present").walkers[0] = fid;
            if let Some(m) = self.buildings.get_mut(target).and_then(|b| b.monument.as_mut()) {
                m.craftsmen.push((figure, fid));
            }
        }
    }

    /// A craftsman walks to the monument and stays while its course needs him. The
    /// first of the phase's crew lays each block as far as the delivered material
    /// allows, and only while the rest of the crew is there too.
    pub(crate) fn update_craftsman(&mut self, fid: FigureId) {
        let Some(f) = self.figures.get(fid) else { return };
        let (act, target, figure) = (f.action, f.target, f.kind);
        let listed = self.buildings.get(target).and_then(|b| b.monument.as_ref()).is_some_and(|m| m.craftsmen.contains(&(figure, fid)));
        if act != 3 && !(listed && self.wants_craftsman(target, figure)) {
            if let Some(m) = self.buildings.get_mut(target).and_then(|b| b.monument.as_mut()) {
                m.craftsmen.retain(|c| c.1 != fid);
            }
            self.send_home(fid);
            return;
        }
        match act {
            1 => {
                let map = &self.map;
                let f = self.figures.get_mut(fid).expect("present");
                match f.walk(map) {
                    Step::Moving => {}
                    Step::Arrived => f.action = 2,
                    _ => f.dead = true,
                }
            }
            2 => {
                let Some(b) = self.buildings.get(target) else { return };
                let Some(def) = monument_def(b.kind) else { return };
                let m = b.monument.as_ref().expect("working");
                let crew = def.crew(m.phase);
                let here = |k: u16| m.craftsmen.iter().any(|&(kk, c)| kk == k && self.figures.get(c).is_some_and(|f| f.action == 2));
                let lead = crew.first().copied().unwrap_or(0);
                let next = if figure != lead || !crew.iter().all(|&k| here(k)) {
                    None
                } else {
                    let paid = def.blocks_paid(m);
                    m.progress.iter().enumerate().position(|(i, &p)| p < BLOCK_WORK && i < paid)
                };
                // The rest of the crew works while the one laying blocks does.
                let moving = next.is_some() || figure != lead && m.craftsmen.iter().any(|&(k, c)| k == lead && self.figures.get(c).is_some_and(|f| f.action == 2 && f.moving));
                if let Some(f) = self.figures.get_mut(fid) {
                    f.moving = moving;
                }
                if let Some(i) = next {
                    let m = self.buildings.get_mut(target).and_then(|b| b.monument.as_mut()).expect("working");
                    m.progress[i] += 1;
                    if m.progress[i] >= BLOCK_WORK {
                        self.refresh_monument_images(target);
                    }
                }
            }
            _ => {
                let map = &self.map;
                let f = self.figures.get_mut(fid).expect("present");
                if f.walk(map) != Step::Moving {
                    f.dead = true;
                }
            }
        }
    }

    fn send_home(&mut self, fid: FigureId) {
        let Some(home) = self.figures.get(fid).map(|f| f.home) else { return };
        let road = self.buildings.get(home).and_then(|b| b.road);
        let map = &self.map;
        let f = self.figures.get_mut(fid).expect("present");
        f.action = 3;
        match road {
            Some(r) if f.go_to(map, r) => {}
            _ => f.dead = true,
        }
    }

    /// A storage yard's monument errand: a sled of whatever a monument's course still
    /// needs. Returns whether one was sent.
    pub(crate) fn yard_monument_errand(&mut self, yard: BuildingId) -> bool {
        let Some(y) = self.buildings.get(yard) else { return false };
        let Some(road) = y.road else { return false };
        let from = (y.x, y.y);
        for id in self.active_monuments() {
            let Some(b) = self.buildings.get(id) else { continue };
            let Some(def) = monument_def(b.kind) else { continue };
            let m = b.monument.as_ref().expect("active");
            for (r, _) in def.phase(m.phase) {
                let need = def.needs(m, r);
                let have = self.stored(yard, r);
                let amount = need.min(have).min(SLED_LOAD);
                if amount < crate::economy::LOAD.min(need) || amount <= 0 {
                    continue;
                }
                let Some(spot) = self.monument_access(id, from) else { continue };
                if crate::figures::find_route(&self.map, Travel::Land, road, spot).is_none() {
                    continue;
                }
                self.take_stored(yard, r, amount);
                if let Some(m) = self.buildings.get_mut(id).and_then(|b| b.monument.as_mut()) {
                    Monument::add(&mut m.in_flight, r, amount);
                }
                self.spawn_sled(yard, id, r, amount, road, spot);
                return true;
            }
        }
        false
    }

    fn spawn_sled(&mut self, yard: BuildingId, target: BuildingId, r: u16, amount: i32, road: (i32, i32), spot: (i32, i32)) {
        let sled = self.figures.spawn(SLED, road.0, road.1, Travel::Land);
        let map = &self.map;
        if let Some(f) = self.figures.get_mut(sled) {
            f.home = yard;
            f.target = target;
            f.cargo = r;
            f.amount = amount;
            f.action = 1;
            f.go_to(map, spot);
        }
        let mut lead = sled;
        for i in 0..SLED_PULLERS {
            let p = self.figures.spawn(SLED_PULLER, road.0, road.1, Travel::Land);
            if let Some(f) = self.figures.get_mut(p) {
                f.target = lead;
                f.counter = i as i32 * 4;
            }
            lead = p;
        }
        self.buildings.get_mut(yard).expect("present").walkers[2] = sled;
    }

    /// A sled travels to its monument and hands over its load.
    pub(crate) fn update_sled(&mut self, fid: FigureId) {
        let map = &self.map;
        let Some(f) = self.figures.get_mut(fid) else { return };
        match f.walk(map) {
            Step::Moving => {}
            step => {
                let (target, r, amount) = (f.target, f.cargo, f.amount);
                f.dead = true;
                let Some(m) = self.buildings.get_mut(target).and_then(|b| b.monument.as_mut()) else { return };
                Monument::add(&mut m.in_flight, r, -amount);
                if step == Step::Arrived {
                    Monument::add(&mut m.delivered, r, amount);
                }
            }
        }
    }

    /// Pullers walk behind the sled (or the puller ahead) and go when it goes.
    pub(crate) fn update_sled_puller(&mut self, fid: FigureId) {
        let Some(f) = self.figures.get(fid) else { return };
        if f.counter > 0 {
            self.figures.get_mut(fid).expect("present").counter -= 1;
            return;
        }
        let lead = f.target;
        let Some(l) = self.figures.get(lead).filter(|l| !l.dead) else {
            self.figures.get_mut(fid).expect("present").dead = true;
            return;
        };
        let to = (l.x, l.y);
        let map = &self.map;
        let f = self.figures.get_mut(fid).expect("present");
        if !f.moving && (f.x, f.y) != to && f.destination != Some(to) {
            f.go_to(map, to);
        }
        f.walk(map);
    }

    /// Daily: monuments whose phase is complete move on to the next.
    pub(crate) fn update_monuments(&mut self) {
        for id in self.active_monuments() {
            // Craftsmen who never arrived are forgotten.
            let alive: Vec<(u16, FigureId)> = self.buildings.get(id).and_then(|b| b.monument.as_ref()).map_or_else(Vec::new, |m| m.craftsmen.clone());
            let alive: Vec<(u16, FigureId)> = alive.into_iter().filter(|&(_, c)| self.figures.get(c).is_some_and(|f| !f.dead && f.target == id)).collect();
            if let Some(m) = self.buildings.get_mut(id).and_then(|b| b.monument.as_mut()) {
                m.craftsmen = alive;
            }
            let Some(b) = self.buildings.get(id) else { continue };
            let Some(def) = monument_def(b.kind) else { continue };
            let m = b.monument.as_ref().expect("active");
            let all_done = m.progress.iter().all(|&p| p >= BLOCK_WORK);
            let paid = def.phase(m.phase).iter().all(|&(r, want)| Monument::amount(&m.delivered, r) >= want);
            if !all_done || !paid {
                continue;
            }
            let (x, y) = (b.x, b.y);
            let last = def.phase_count - 1;
            let m = self.buildings.get_mut(id).and_then(|b| b.monument.as_mut()).expect("active");
            m.phase += 1;
            m.delivered.clear();
            m.progress.iter_mut().for_each(|p| *p = 0);
            if m.phase >= last {
                m.finished = true;
                let name = match def.style {
                    Style::Mastaba { .. } => "mastaba",
                    Style::Pyramid(Family::Stepped) => "stepped_pyramid",
                    Style::Pyramid(Family::Bent) => "bent_pyramid",
                    Style::Pyramid(Family::True) => "pyramid",
                    Style::Pyramid(Family::Mudbrick) => "mudbrick_pyramid",
                };
                self.post_event_text(crate::scenario_events::EventText {
                    title: format!("{name}_congratulations_title"),
                    body: format!("{name}_congratulations"),
                    template: 131,
                    ..Default::default()
                });
                if let Some(n) = self.notices.log.last_mut() {
                    n.tile = Some((x, y));
                }
            }
            self.refresh_monument_images(id);
        }
    }

    /// The monument's progress for the info window: phase, whether it is finished, and
    /// for each material of the phase what has been delivered and what is needed.
    pub fn monument_status(&self, id: BuildingId) -> Option<MonumentStatus> {
        let b = self.buildings.get(id)?;
        let def = monument_def(b.kind)?;
        let m = b.monument.as_ref()?;
        let needs = def.phase(m.phase).iter().map(|&(r, want)| (r, Monument::amount(&m.delivered, r), want)).collect();
        Some((m.phase, m.finished, needs))
    }
}
