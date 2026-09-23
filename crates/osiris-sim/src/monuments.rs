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
pub const FUNERAL_WALKER: u16 = 94;

const CLAY: u16 = 11;
const BRICKS: u16 = 12;
const TIMBER: u16 = 20;
const STONE: u16 = 24;
const LIMESTONE: u16 = 25;
const GRANITE: u16 = 26;
const PAINT: u16 = 33;
const SANDSTONE: u16 = 30;

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
pub const SPHINX: u16 = 210;
pub const SMALL_OBELISK: u16 = 262;
pub const LARGE_OBELISK: u16 = 263;
pub const MAUSOLEUM: u16 = 222;

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
    /// Granite paid for when placed, then scaffolding and carving; `size` tiles
    /// square, drawn in `stages` images.
    Obelisk { size: i32, stages: u8, granite: i32, timber: &'static [i32] },
    /// Three 6x6 parts in a line (head, body, tail), carved from a buried outcrop in
    /// six stages: carpenters' scaffolding, then stonemasons carving, then painting.
    Sphinx,
    /// Four 4x4 buildings in a row (tower, colonnade, hall, pylon) in a 22x8
    /// courtyard of paving, statues, planters and sphinxes. Laborers level the
    /// courtyard; stonemasons raise the first storey a block (one sled of sandstone) at
    /// a time; carpenters build each part a wooden ramp; the masons raise the second
    /// storey; and laborers lay out the courtyard.
    Mausoleum,
}

/// A mausoleum: 22x8 tiles, the parts at these columns of rows 2-5, the phases'
/// work (laborers' tiles, first-storey blocks, ramps, second-storey blocks, the
/// courtyard) and the ticks each takes.
const MAUSOLEUM_SIZE: (i32, i32) = (22, 8);
const MAUSOLEUM_PARTS: [i32; 4] = [2, 6, 10, 14];
const MAUSOLEUM_BLOCKS: usize = 8;
const MAUSOLEUM_WORK: u16 = 50;
/// Sandstone taken from storage when a mausoleum is placed, and on each sled.
const MAUSOLEUM_PLACEMENT: i32 = 24000;
/// Timber for the ramps. The manual says a mausoleum needs wood; how much is not
/// known, so this is a placeholder.
const MAUSOLEUM_TIMBER: i32 = 400;

/// What a courtyard tile of a mausoleum becomes, at column `x` and row `y`: an image
/// offset in the skin's extras pack (paving, patterned paving, flowers, palm, the
/// statues and the sphinx halves), or `None` for the parts' own tiles.
fn mausoleum_decor(x: i32, y: i32) -> Option<u32> {
    let edge = y == 0 || y == 7;
    Some(match (x, y) {
        (0..=1, _) => 0,
        (2..=17, 2..=5) => return None,
        (2..=17, _) => {
            let first = (x - 2) % 4 < 2;
            match (first, y) {
                (true, 0) => 4,
                (true, 7) => 6,
                (true, _) => 0,
                (false, _) => 1,
            }
        }
        (18, _) if edge => 2,
        (19..=21, _) if edge => if x % 2 == 0 { 2 } else { 3 },
        (18, 1 | 2 | 5 | 6) => 0,
        (_, 3 | 4) => 1,
        (_, 1) => 13,
        (_, 2) => 12,
        (_, 5) => 8,
        (_, _) => 9,
    })
}

/// The courtyard tiles of a mausoleum, in the order its laborers work them.
fn mausoleum_tiles() -> Vec<(i32, i32)> {
    let (w, h) = MAUSOLEUM_SIZE;
    (0..h).flat_map(|y| (0..w).map(move |x| (x, y))).filter(|&(x, y)| mausoleum_decor(x, y).is_some()).collect()
}

/// The sphinx's phases after placing it: (timber, paint, clay). These are the
/// placeholder amounts of the reconstruction the facts come from; the original's
/// are not known.
const SPHINX_PHASES: [(i32, i32, i32); 7] = [(400, 0, 0), (400, 0, 0), (800, 0, 0), (600, 0, 0), (400, 0, 0), (200, 0, 0), (0, 400, 400)];

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

/// An obelisk: no leveling (it starts at the first building phase), timber for the
/// scaffolding in its first phases, stonemasons carving from its third until the
/// last art stage. Its work is counted per tile.
const fn obelisk(kind: u16, size: i32, stages: u8, granite: i32, timber: &'static [i32], weight: i32, title: usize) -> MonumentDef {
    MonumentDef { kind, cols: size, rows: size, style: Style::Obelisk { size, stages, granite, timber }, phase_count: LEVELING_PHASES + stages + 1, polish: 0, mastaba_phases: &[], weight, title }
}

pub const MONUMENTS: [MonumentDef; 18] = [
    MonumentDef { kind: SPHINX, cols: 3, rows: 6, style: Style::Sphinx, phase_count: LEVELING_PHASES + SPHINX_PHASES.len() as u8 + 1, polish: 0, mastaba_phases: &[], weight: 1, title: 21 },
    // The rating weight is a placeholder.
    MonumentDef { kind: MAUSOLEUM, cols: 11, rows: 4, style: Style::Mausoleum, phase_count: 6, polish: 0, mastaba_phases: &[], weight: 4, title: 25 },
    obelisk(SMALL_OBELISK, 3, 4, 100, &[200, 200, 200], 2, 22),
    obelisk(LARGE_OBELISK, 5, 6, 200, &[400, 400, 400, 200], 4, 23),
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

/// The monument a scenario names by its title (text group 198); the three
/// mausoleum titles are one building in different looks.
pub fn monument_for_title(t: usize) -> Option<&'static MonumentDef> {
    let t = if (26..=27).contains(&t) { 25 } else { t };
    MONUMENTS.iter().find(|m| m.title == t)
}

/// What a monument takes from storage when it is placed: (resource, units).
pub fn placement_cost(k: u16) -> Option<(u16, i32)> {
    match monument_def(k)?.style {
        Style::Obelisk { granite, .. } => Some((GRANITE, granite)),
        Style::Mausoleum => Some((SANDSTONE, MAUSOLEUM_PLACEMENT)),
        _ => None,
    }
}

/// Phase, finished, and (resource, delivered, needed) for the phase's materials.
pub type MonumentStatus = (u8, bool, Vec<(u16, i32, i32)>);

/// A site tile: where, its image, and the next stage's image fading in over it with
/// how far in.
type SiteTile = ((i32, i32), u32, Option<(u32, f32)>);

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
    /// A tomb whose funeral procession has come: it houses the deceased.
    #[serde(default)]
    pub funeral_done: bool,
    /// The ground the site was staked out on, a row at a time, shown until the
    /// laborers have levelled it.
    #[serde(default)]
    pub ground: Vec<u32>,
    /// Which of a mausoleum's three looks it has.
    #[serde(default)]
    pub skin: u8,
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
            Style::Obelisk { timber, .. } => {
                let own = p.saturating_sub(LEVELING_PHASES) as usize;
                return timber.get(own).map_or_else(Vec::new, |&t| vec![(TIMBER, t)]);
            }
            Style::Sphinx => {
                let own = p.saturating_sub(LEVELING_PHASES) as usize;
                let Some(&(timber, paint, clay)) = SPHINX_PHASES.get(own) else { return Vec::new() };
                return [(TIMBER, timber), (PAINT, paint), (CLAY, clay)].into_iter().filter(|m| m.1 > 0).collect();
            }
            Style::Mausoleum => {
                let blocks = (MAUSOLEUM_BLOCKS * MAUSOLEUM_PARTS.len()) as i32 * SLED_LOAD;
                return match p {
                    1 | 3 => vec![(SANDSTONE, blocks)],
                    2 => vec![(TIMBER, MAUSOLEUM_TIMBER)],
                    _ => Vec::new(),
                };
            }
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
        let brickwork = matches!(self.style, Style::Mastaba { .. } | Style::Pyramid(_));
        if has(BRICKS) || has(CLAY) && brickwork {
            crew.push(BRICKLAYER);
        }
        let polishing = matches!(self.style, Style::Pyramid(_)) && p >= self.courses_end() && p + 1 < self.phase_count;
        let carving = matches!(self.style, Style::Obelisk { .. } | Style::Sphinx) && p >= LEVELING_PHASES + 2 && p + 1 < self.phase_count;
        if has(STONE) || has(LIMESTONE) || has(SANDSTONE) || polishing || carving {
            crew.push(STONEMASON);
        }
        if has(TIMBER) {
            crew.push(CARPENTER);
        }
        crew
    }

    /// Pieces of work in phase `p`: a block of the site for most monuments; for a
    /// mausoleum its courtyard tiles, storey blocks or ramps.
    pub fn units(&self, p: u8) -> usize {
        match self.style {
            Style::Mausoleum => match p {
                0 | 4 => mausoleum_tiles().len(),
                1 | 3 => MAUSOLEUM_BLOCKS * MAUSOLEUM_PARTS.len(),
                2 => MAUSOLEUM_PARTS.len(),
                _ => 0,
            },
            _ => (self.cols * self.rows) as usize,
        }
    }

    /// Ticks of work each piece of phase `p` takes.
    pub fn unit_work(&self, _p: u8) -> u16 {
        match self.style {
            Style::Mausoleum => MAUSOLEUM_WORK,
            _ => BLOCK_WORK,
        }
    }

    /// Whether phase `p` is laborers' work: the levelling, and a mausoleum's
    /// courtyard.
    pub fn laborers(&self, p: u8) -> bool {
        match self.style {
            Style::Mausoleum => p == 0 || p == 4,
            _ => p < LEVELING_PHASES,
        }
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
        let blocks = self.units(m.phase);
        self.phase(m.phase)
            .iter()
            .map(|&(r, want)| (Monument::amount(&m.delivered, r) as i64 * blocks as i64 / want.max(1) as i64) as usize)
            .min()
            .unwrap_or(blocks)
            .min(blocks)
    }

    /// Whether the monument is a tomb, which takes burial provisions.
    pub fn is_tomb(&self) -> bool {
        matches!(self.style, Style::Mastaba { .. } | Style::Pyramid(_) | Style::Mausoleum)
    }

    /// Rings of a stepped pyramid.
    fn rings(&self) -> i32 {
        (self.cols.min(self.rows) + 1) / 2
    }
}

impl World {
    /// The footprint of monument type `k` placed facing north.
    pub fn monument_footprint(&self, k: u16) -> Option<(i32, i32)> {
        monument_def(k).map(|d| match d.style {
            Style::Obelisk { size, .. } => (size, size),
            Style::Sphinx => (6, 18),
            Style::Mausoleum => MAUSOLEUM_SIZE,
            _ => (d.cols * 2, d.rows * 2),
        })
    }

    /// Monument-specific placement rules: an obelisk's granite must be in storage,
    /// and only one obelisk may be under construction at a time.
    pub(crate) fn can_place_monument(&self, k: u16) -> Result<(), &'static str> {
        let Some(def) = monument_def(k) else { return Ok(()) };
        if let Style::Obelisk { granite, .. } = def.style {
            let building = self.buildings.iter().any(|b| matches!(b.kind, SMALL_OBELISK | LARGE_OBELISK) && b.monument.as_ref().is_some_and(|m| !m.finished));
            if building {
                return Err("Only one obelisk at a time");
            }
            if self.yards_stored(GRANITE) < granite {
                return Err("Not enough granite in storage");
            }
        }
        if def.style == Style::Mausoleum && self.yards_stored(SANDSTONE) < MAUSOLEUM_PLACEMENT {
            return Err("You need 240 blocks of sandstone to build a mausoleum");
        }
        Ok(())
    }

    /// Lays out a new monument: its footprint and the staked-out site.
    pub(crate) fn place_monument(&mut self, id: BuildingId) {
        let Some(b) = self.buildings.get_mut(id) else { return };
        let Some(def) = monument_def(b.kind) else { return };
        let (w, h) = b.footprint();
        let (x0, y0) = (b.x, b.y);
        let ground = (y0..y0 + h).flat_map(|y| (x0..x0 + w).map(move |x| (x, y))).map(|(x, y)| self.map.images.at_or(x, y, 0)).collect();
        let mut m = Monument { progress: vec![0; def.units(0)], ground, ..Default::default() };
        if def.style == Style::Sphinx {
            // Carved from the rock where it stands: no leveling.
            m.phase = LEVELING_PHASES;
        }
        if def.style == Style::Mausoleum {
            // Its look is the mausoleum the scenario names (text 198: 25, 26 or 27).
            m.skin = self.scenario_monuments.iter().find_map(|&t| (25..=27).contains(&t).then(|| (t - 25) as u8)).unwrap_or(0);
        }
        let paid = match def.style {
            // The granite goes to the site at once, and there is no leveling.
            Style::Obelisk { granite, .. } => {
                m.phase = LEVELING_PHASES;
                Some((GRANITE, granite))
            }
            Style::Mausoleum => Some((SANDSTONE, MAUSOLEUM_PLACEMENT)),
            _ => None,
        };
        if let Some((r, amount)) = paid {
            let mut left = amount;
            let yards: Vec<BuildingId> = self.buildings.iter().filter(|y| y.kind == kind::STORAGE_YARD).map(|y| y.id).collect();
            for y in yards {
                if left > 0 {
                    left -= self.take_stored(y, r, left);
                }
            }
        }
        if let Some(b) = self.buildings.get_mut(id) {
            b.monument = Some(m);
        }
        self.refresh_monument_images(id);
    }

    /// The site art every tomb's pack shares, by its group 2 start (`site` is group 2
    /// offset 7): levelled ground (offset 12, eight looks), the surveyor's stake
    /// (group 8), the four stages of the levelling trenches (groups 3-6, nine pieces
    /// each) and a pyramid's basement (group 7, four 2x2 pieces).
    fn grounded(site: u32, dx: i32, dy: i32) -> u32 {
        site + 5 + ((dy * 4 + dx) & 7) as u32
    }

    pub(crate) fn stake_image(site: u32) -> u32 {
        site + 13
    }

    fn trench(site: u32, stage: u8) -> u32 {
        site + 14 + 9 * (stage as u32 - 1)
    }

    fn basement(site: u32) -> u32 {
        site + 50
    }

    /// The staked-out foundation: its corners and edges marked, stony ground within.
    fn foundation(site: u32, (x, y): (i32, i32), (x0, y0): (i32, i32), (x1, y1): (i32, i32)) -> u32 {
        let inside = x > x0 && x < x1 || y > y0 && y < y1;
        if (x, y) == (x0, y0) {
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
        }
    }

    /// The piece of a trench grid for a tile, given which of its neighbours (north,
    /// east, south, west: y-1, x+1, y+1, x-1) are trenches too. The nine pieces are
    /// the corners NE, ES, SW, WN, the T-joins open to the west, north, east and
    /// south, and the crossing.
    fn trench_piece(n: bool, e: bool, s: bool, w: bool) -> u32 {
        match (n, e, s, w) {
            (true, true, true, true) => 8,
            (false, true, true, true) => 5,
            (true, false, true, true) => 6,
            (true, true, false, true) => 7,
            (true, true, true, false) => 4,
            (true, true, false, false) => 0,
            (false, true, true, false) => 1,
            (false, false, true, true) => 2,
            (true, false, false, true) => 3,
            // Straight runs and ends, from the T-joins that carry them.
            (true, false, _, false) | (_, false, true, false) => 4,
            (false, true, false, _) | (false, _, false, true) => 5,
            _ => 8,
        }
    }

    /// Each tile's site image and, over it, the next stage fading in with the work
    /// done on its block: (tile, image, fading image and how far in).
    fn site_tiles(&self, id: BuildingId) -> Vec<SiteTile> {
        let mut out = Vec::new();
        let Some(b) = self.buildings.get(id) else { return out };
        let (Some(def), Some(m)) = (monument_def(b.kind), b.monument.as_ref()) else { return out };
        if m.finished || !matches!(def.style, Style::Mastaba { .. } | Style::Pyramid(_)) {
            return out;
        }
        let Some(site) = self.defs.building(b.kind).map(|d| d.image) else { return out };
        let (x0, y0) = (b.x, b.y);
        let (w, h) = b.footprint();
        let (x1, y1) = (x0 + w - 1, y0 + h - 1);
        let work = |x: i32, y: i32| -> u16 {
            let i = ((y - y0) / 2 * def.cols + (x - x0) / 2) as usize;
            m.progress.get(i).copied().unwrap_or(0).min(BLOCK_WORK)
        };
        let inside = |x: i32, y: i32| x >= x0 && x <= x1 && y >= y0 && y <= y1;
        let pyramid = matches!(def.style, Style::Pyramid(_));
        let phase = m.phase;
        for y in y0..=y1 {
            for x in x0..=x1 {
                let p = work(x, y);
                let fade = (p > 0 && p < BLOCK_WORK).then_some(p as f32 / BLOCK_WORK as f32);
                let (dx, dy) = (x - x0, y - y0);
                let grounded = Self::grounded(site, dx, dy);
                let foundation = Self::foundation(site, (x, y), (x0, y0), (x1, y1));
                // In the flooding phases every tile of the site is trench; while
                // digging, only the blocks begun.
                let dug = |xx: i32, yy: i32| inside(xx, yy) && (phase != 2 || work(xx, yy) > 0);
                let piece = || Self::trench_piece(dug(x, y - 1), dug(x + 1, y), dug(x, y + 1), dug(x - 1, y));
                let (under, over) = match phase {
                    // Bare ground with the corners staked; levelled ground spreads.
                    0 => {
                        let bare = m.ground.get((dy * w + dx) as usize).copied().filter(|&g| g != 0).unwrap_or(grounded);
                        if p >= BLOCK_WORK { (grounded, None) } else { (bare, Some(grounded)) }
                    }
                    // The foundation marked out on the levelled ground.
                    1 => if p >= BLOCK_WORK { (foundation, None) } else { (grounded, Some(foundation)) },
                    // A pyramid's site is trenched, flooded to find the level,
                    // filled with rubble and smoothed.
                    2..=5 if pyramid => {
                        let stage = phase - 1;
                        let next = Self::trench(site, stage) + piece();
                        let before = if phase == 2 { foundation } else { Self::trench(site, stage - 1) + piece() };
                        if p >= BLOCK_WORK { (next, None) } else { (before, Some(next)) }
                    }
                    _ if pyramid => (Self::trench(site, 4) + Self::trench_piece(y > y0, x < x1, y < y1, x > x0), None),
                    _ => (foundation, None),
                };
                out.push(((x, y), under, over.zip(fade)));
            }
        }
        out
    }

    /// The images fading in over a monument's tiles as its site is worked:
    /// (x, y, image, opacity).
    pub fn monument_fades(&self, id: BuildingId) -> Vec<(i32, i32, u32, f32)> {
        self.site_tiles(id).into_iter().filter_map(|((x, y), _, over)| over.map(|(image, a)| (x, y, image, a))).collect()
    }

    /// The surveyor's stakes at a site's corners, standing until their block is
    /// worked: (x, y, image).
    pub fn monument_stakes(&self, id: BuildingId) -> Vec<(i32, i32, u32)> {
        let Some(b) = self.buildings.get(id) else { return Vec::new() };
        let (Some(def), Some(m)) = (monument_def(b.kind), b.monument.as_ref()) else { return Vec::new() };
        if m.phase != 0 || m.finished {
            return Vec::new();
        }
        if def.style == Style::Mausoleum {
            // The courtyard's four corners, until their tiles are levelled.
            let Some(stake) = self.defs.building(b.kind).and_then(|d| d.anims.get("stake")).map(|a| a.image) else { return Vec::new() };
            let tiles = mausoleum_tiles();
            let (w, h) = MAUSOLEUM_SIZE;
            return [(0, 0), (w - 1, 0), (0, h - 1), (w - 1, h - 1)]
                .into_iter()
                .filter(|c| tiles.iter().position(|t| t == c).and_then(|i| m.progress.get(i)).is_some_and(|&p| p < MAUSOLEUM_WORK))
                .map(|(x, y)| (b.x + x, b.y + y, stake))
                .collect();
        }
        if !matches!(def.style, Style::Mastaba { .. } | Style::Pyramid(_)) {
            return Vec::new();
        }
        let Some(site) = self.defs.building(b.kind).map(|d| d.image) else { return Vec::new() };
        let (w, h) = b.footprint();
        let (x1, y1) = (b.x + w - 1, b.y + h - 1);
        [(b.x, b.y), (x1, b.y), (b.x, y1), (x1, y1)]
            .into_iter()
            .filter(|&(x, y)| m.progress.get(((y - b.y) / 2 * def.cols + (x - b.x) / 2) as usize).is_some_and(|&p| p == 0))
            .map(|(x, y)| (x, y, Self::stake_image(site)))
            .collect()
    }

    /// A mausoleum's tiles: the courtyard bare, then levelled, then laid out; each part
    /// its foundation blocks, then its columns in scaffolding once its ramp is built,
    /// then finished. (The part images face the default view.)
    fn refresh_mausoleum(&mut self, id: BuildingId) {
        let Some(b) = self.buildings.get(id) else { return };
        let (Some(bdef), Some(m)) = (self.defs.building(b.kind), b.monument.as_ref()) else { return };
        let (x0, y0) = (b.x, b.y);
        let img = |key: &str| bdef.anims.get(key).map(|a| a.image);
        let skin = m.skin.min(2);
        let (Some(extras), Some(ground)) = (img(&format!("extra_v{skin}")), img("ground")) else { return };
        let (phase, finished) = (m.phase, m.finished);
        let done = |i: usize| m.progress.get(i).is_some_and(|&p| p >= MAUSOLEUM_WORK);
        let mut tiles = Vec::new();
        for (i, (x, y)) in mausoleum_tiles().into_iter().enumerate() {
            let levelled = ground + (((x * 7 + y * 3) & 7) as u32);
            let image = match phase {
                _ if finished => extras + mausoleum_decor(x, y).unwrap_or(0),
                0 if !done(i) => m.ground.get((y * MAUSOLEUM_SIZE.0 + x) as usize).copied().filter(|&g| g != 0).unwrap_or(levelled),
                4 if done(i) => extras + mausoleum_decor(x, y).unwrap_or(0),
                _ => levelled,
            };
            tiles.push((x0 + x, y0 + y, image));
        }
        let parts: Vec<(i32, u32)> = MAUSOLEUM_PARTS
            .iter()
            .enumerate()
            .filter_map(|(k, &px)| {
                let own = |i: usize| (k * MAUSOLEUM_BLOCKS..(k + 1) * MAUSOLEUM_BLOCKS).contains(&i);
                let stage = match phase {
                    _ if finished => 3,
                    0 | 1 => 1,
                    2 => if done(k) { 2 } else { 1 },
                    3 => if (0..m.progress.len()).filter(|&i| own(i)).all(done) { 3 } else { 2 },
                    _ => 3,
                };
                let part = ["a", "b", "c", "d"][k];
                img(&format!("{stage}{part}_v{skin}")).map(|image| (px, image))
            })
            .collect();
        for (x, y, image) in tiles {
            self.map.set_single_image(x, y, image);
        }
        for (px, image) in parts {
            self.map.set_footprint(x0 + px, y0 + 2, 4, image);
        }
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
        if def.style == Style::Sphinx {
            // Each part shows its stage of carving: the rough outcrop first, the
            // finished, painted figure at the end.
            let own = phase.saturating_sub(LEVELING_PHASES);
            let stage = if finished { 6 } else { own.clamp(1, 6) };
            for (part, letter) in ["a", "b", "c"].iter().enumerate() {
                let image = bdef.anims.get(&format!("s{stage}{letter}1")).map_or(site, |a| a.image);
                self.map.set_footprint(x0, y0 + 6 * part as i32, 6, image);
            }
            return;
        }
        if def.style == Style::Mausoleum {
            self.refresh_mausoleum(id);
            return;
        }
        if let Style::Obelisk { size, stages, .. } = def.style {
            let stage = if finished { stages } else { phase.saturating_sub(LEVELING_PHASES).clamp(1, stages) };
            let key = ["sa", "sb", "sc", "sd", "se", "sf"][(stage - 1) as usize];
            let image = bdef.anims.get(key).map_or(site, |a| a.image);
            self.map.set_footprint(x0, y0, size, image);
            return;
        }
        // The site's ground, then each block's course where one has been laid.
        for ((x, y), image, _) in self.site_tiles(id) {
            self.map.set_single_image(x, y, image);
        }
        if phase < LEVELING_PHASES && !finished {
            return;
        }
        let pyramid = matches!(def.style, Style::Pyramid(_));
        if pyramid && !finished && phase == PYRAMID_BASE_PHASES - 1 {
            // The basement, dug in the four blocks about the centre.
            let (cx, cy) = (x0 + w / 2, y0 + h / 2);
            let base = Self::basement(site);
            for (bx, by, piece) in [(cx, cy - 2, 0), (cx - 2, cy, 1), (cx - 2, cy - 2, 2), (cx, cy, 3)] {
                self.map.set_footprint(bx, by, 2, base + piece);
            }
        }
        let stacks = self.monument_stacks(id);
        for &(bx, by, image, lift) in &stacks {
            if lift == 0 {
                self.map.set_footprint(bx, by, 2, image);
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
            Style::Obelisk { .. } | Style::Sphinx | Style::Mausoleum => {}
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
                            // Still levelling: the site shows its trenches and basement.
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
                let def = monument_def(b.kind)?;
                if !def.laborers(m.phase) || busy.iter().filter(|b| b.0 == id).count() >= MAX_LABORERS {
                    return None;
                }
                let work = def.unit_work(m.phase);
                let block = m.progress.iter().enumerate().position(|(i, &p)| p < work && !busy.contains(&(id, i as i32)))?;
                Some(((b.x - from.0).abs() + (b.y - from.1).abs(), id, block))
            })
            .min()
            .map(|(_, id, block)| (id, block))
    }

    /// The next block of monument `id` that no other laborer is levelling.
    pub(crate) fn next_leveling_block(&self, id: BuildingId, me: FigureId) -> Option<usize> {
        let b = self.buildings.get(id)?;
        let (def, m) = (monument_def(b.kind)?, b.monument.as_ref()?);
        if !def.laborers(m.phase) {
            return None;
        }
        let work = def.unit_work(m.phase);
        let busy: Vec<i32> = self.figures.iter().filter(|f| f.kind == crate::farms::PEASANT && f.action == 4 && f.target == id && f.id != me).map(|f| f.amount).collect();
        m.progress.iter().enumerate().position(|(i, &p)| p < work && !busy.contains(&(i as i32)))
    }

    /// A laborer at a monument works its block; true when the block is done.
    pub(crate) fn level_block(&mut self, id: BuildingId, block: usize) -> bool {
        let Some(def) = self.buildings.get(id).and_then(|b| monument_def(b.kind)) else { return true };
        let Some(m) = self.buildings.get_mut(id).and_then(|b| b.monument.as_mut()) else { return true };
        if !def.laborers(m.phase) {
            return true;
        }
        let work = def.unit_work(m.phase);
        let Some(p) = m.progress.get_mut(block) else { return true };
        let was = *p;
        *p = (*p + 1).min(work);
        let done = *p >= work;
        if done && was < work {
            // The block's tiles now show the finished stage.
            self.refresh_monument_images(id);
        }
        done
    }

    /// Whether monument `id`'s current phase wants craftsman `figure` on site.
    fn wants_craftsman(&self, id: BuildingId, figure: u16) -> bool {
        let Some(b) = self.buildings.get(id) else { return false };
        let Some(def) = monument_def(b.kind) else { return false };
        let Some(m) = &b.monument else { return false };
        if m.finished || def.laborers(m.phase) {
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
                    let work = def.unit_work(m.phase);
                    m.progress.iter().enumerate().position(|(i, &p)| p < work && i < paid)
                };
                // The rest of the crew works while the one laying blocks does.
                let moving = next.is_some() || figure != lead && m.craftsmen.iter().any(|&(k, c)| k == lead && self.figures.get(c).is_some_and(|f| f.action == 2 && f.moving));
                if let Some(f) = self.figures.get_mut(fid) {
                    f.moving = moving;
                }
                if let Some(i) = next {
                    let work = def.unit_work(m.phase);
                    let m = self.buildings.get_mut(target).and_then(|b| b.monument.as_mut()).expect("working");
                    m.progress[i] += 1;
                    if m.progress[i] >= work {
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
            let all_done = m.progress.iter().all(|&p| p >= def.unit_work(m.phase));
            let paid = def.phase(m.phase).iter().all(|&(r, want)| Monument::amount(&m.delivered, r) >= want);
            if !all_done || !paid {
                continue;
            }
            let (x, y) = (b.x, b.y);
            let last = def.phase_count - 1;
            let m = self.buildings.get_mut(id).and_then(|b| b.monument.as_mut()).expect("active");
            m.phase += 1;
            m.delivered.clear();
            m.progress = vec![0; def.units(m.phase)];
            if m.phase >= last {
                m.finished = true;
                let name = match def.style {
                    Style::Mastaba { .. } => "mastaba",
                    Style::Pyramid(Family::Stepped) => "stepped_pyramid",
                    Style::Pyramid(Family::Bent) => "bent_pyramid",
                    Style::Pyramid(Family::True) => "pyramid",
                    Style::Pyramid(Family::Mudbrick) => "mudbrick_pyramid",
                    Style::Obelisk { .. } => "obelisk",
                    Style::Sphinx => "sphinx",
                    Style::Mausoleum => "mausoleum",
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

    /// Burial provisions the scenario asks for: (resource, units needed, units sent).
    pub fn burial_needs(&self) -> Vec<(u16, i32, i32)> {
        self.burial.iter().enumerate().filter(|(_, p)| p.0 > 0).map(|(r, &(need, sent))| (r as u16, need, sent)).collect()
    }

    pub fn burial_complete(&self) -> bool {
        self.burial.iter().all(|&(need, sent)| sent >= need)
    }

    /// Sends up to `units` of burial provision `r` from storage (granaries too, for
    /// food); returns what was sent.
    pub fn dispatch_burial(&mut self, r: u16, units: i32) -> i32 {
        let Some(&(need, sent)) = self.burial.get(r as usize) else { return 0 };
        let mut left = units.min(need - sent).min(self.city_stored(r)).max(0);
        let stores: Vec<BuildingId> = self
            .buildings
            .iter()
            .filter(|b| b.kind == kind::STORAGE_YARD || crate::economy::resource::is_food(r) && b.kind == kind::GRANARY)
            .map(|b| b.id)
            .collect();
        let mut moved = 0;
        for id in stores {
            if left <= 0 {
                break;
            }
            let taken = self.take_stored(id, r, left);
            left -= taken;
            moved += taken;
        }
        self.burial[r as usize].1 += moved;
        moved
    }

    /// Monthly: once every provision has been sent, a funeral procession walks from
    /// the edge of the map to each finished tomb that has had none.
    pub(crate) fn update_funerals(&mut self) {
        if self.burial.iter().all(|p| p.0 == 0) || !self.burial_complete() {
            return;
        }
        let walking: Vec<u32> = self.figures.iter().filter(|f| f.kind == FUNERAL_WALKER).map(|f| f.target).collect();
        let tombs: Vec<BuildingId> = self
            .buildings
            .iter()
            .filter(|b| monument_def(b.kind).is_some_and(|d| d.is_tomb()) && b.monument.as_ref().is_some_and(|m| m.finished && !m.funeral_done))
            .map(|b| b.id)
            .filter(|id| !walking.contains(id))
            .collect();
        let (ex, ey) = self.entry_point;
        for id in tombs {
            let Some(spot) = self.monument_access(id, (ex, ey)) else { continue };
            let fid = self.figures.spawn(FUNERAL_WALKER, ex, ey, Travel::Land);
            let map = &self.map;
            if let Some(f) = self.figures.get_mut(fid) {
                f.target = id;
                if !f.go_to(map, spot) {
                    f.dead = true;
                }
            }
        }
    }

    /// The procession walks to its tomb, and the deceased is laid to rest.
    pub(crate) fn update_funeral_walker(&mut self, fid: FigureId) {
        let map = &self.map;
        let Some(f) = self.figures.get_mut(fid) else { return };
        match f.walk(map) {
            Step::Moving => {}
            step => {
                f.dead = true;
                let target = f.target;
                if step == Step::Arrived
                    && let Some(m) = self.buildings.get_mut(target).and_then(|b| b.monument.as_mut())
                {
                    m.funeral_done = true;
                }
            }
        }
    }
}
