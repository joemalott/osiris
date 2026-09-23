//! Monuments. Each is built in phases, each phase a number of pieces of work:
//! laborers from work camps level and lay out the site, and guild craftsmen waiting
//! on site do the rest, as far as the material storage yards drag over on sleds (at
//! most 400 units a sled, never more than the phase still needs) allows. When every
//! piece of a phase is done, the next begins; after the last, the monument is
//! complete. Pyramids and mastabas are built block by block instead: see
//! [`crate::pyramids`].

use crate::buildings::{BuildingId, kind};
use crate::figures::{FigureId, Step, Travel};
use crate::world::World;

/// Progress a block needs in each phase: one worker adds one point a tick.
pub const BLOCK_WORK: u16 = 200;
/// Most a sled carries.
pub const SLED_LOAD: i32 = 400;
/// Men pulling each sled.
const SLED_PULLERS: usize = 6;

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
pub const MUDBRICK_PYRAMID_COMPLEX: u16 = 246;
pub const GRAND_MUDBRICK_PYRAMID_COMPLEX: u16 = 247;
pub const STEPPED_PYRAMID_COMPLEX: u16 = 251;
pub const GRAND_STEPPED_PYRAMID_COMPLEX: u16 = 252;
pub const PYRAMID_COMPLEX: u16 = 256;
pub const GRAND_PYRAMID_COMPLEX: u16 = 257;
pub const SPHINX: u16 = 210;
pub const SMALL_OBELISK: u16 = 262;
pub const LARGE_OBELISK: u16 = 263;
pub const MAUSOLEUM: u16 = 222;
pub const SUN_TEMPLE: u16 = 264;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    /// A brick tomb of one or two courses, with a chapel on its east side.
    Mastaba,
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
    /// A 5x5 obelisk in an 11x10 walled court, a gate, and an avenue to a 3x3 fore
    /// temple. Laborers level the site; carpenters and stonemasons take turns
    /// shaping the obelisk; masons build the gate and the walls a sled of sandstone at
    /// a time; laborers lay the court's floor; masons build the fore temple.
    SunTemple,
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

/// The sun temple: 11x21 tiles; its obelisk's 5x5 corner; sandstone taken when it is
/// placed; and the obelisk's thirteen steps, each carpenters' (true) or masons'
/// work, with its ticks.
const SUN_TEMPLE_SIZE: (i32, i32) = (11, 21);
const SUN_OBELISK: (i32, i32) = (3, 1);
const SUN_TEMPLE_PLACEMENT: i32 = 22000;
const SUN_OBELISK_STEPS: [(bool, u16); 13] = [
    (true, 260),
    (true, 260),
    (true, 450),
    (true, 450),
    (false, 360),
    (true, 520),
    (false, 360),
    (true, 520),
    (false, 300),
    (true, 290),
    (false, 270),
    (true, 310),
    (false, 380),
];
/// The sun temple's phases after the levelling: the obelisk's steps, then the gate,
/// walls, floor and fore temple.
const SUN_GATE: u8 = 14;
const SUN_WALLS: u8 = 15;
const SUN_FLOOR: u8 = 16;
const SUN_FORE: u8 = 17;
/// Sleds of sandstone the fore temple takes.
const SUN_FORE_SLEDS: usize = 3;

/// What a part of the sun temple is. Each has its image offset in the extras pack.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SunPart {
    /// The 2x2 gate.
    Gate,
    /// A corner pier (NW, NE, SE, SW).
    Corner(u8),
    SideWall,
    EndWall,
    Floor,
    Path,
    Planter,
    Palm,
    /// The 3x3 fore temple.
    Fore,
}

impl SunPart {
    fn is_wall(self) -> bool {
        matches!(self, SunPart::Corner(_) | SunPart::SideWall | SunPart::EndWall)
    }

    fn is_floor(self) -> bool {
        matches!(self, SunPart::Floor | SunPart::Path | SunPart::Planter | SunPart::Palm)
    }

    fn size(self) -> i32 {
        match self {
            SunPart::Gate => 2,
            SunPart::Fore => 3,
            _ => 1,
        }
    }

    /// Offset of the finished image in the extras pack, facing the default view.
    fn image(self) -> u32 {
        match self {
            SunPart::Gate => 0,
            SunPart::Corner(c) => 2 + [0, 1, 2, 3][c as usize],
            SunPart::SideWall => 6,
            SunPart::EndWall => 7,
            SunPart::Floor => 8,
            SunPart::Path => 9,
            SunPart::Planter => 10,
            SunPart::Palm => 11,
            SunPart::Fore => 12,
        }
    }
}

/// The sun temple's parts other than the obelisk, at their tiles, in the order the
/// laborers level them.
fn sun_temple_parts() -> Vec<((i32, i32), SunPart)> {
    let mut v = vec![((6, 9), SunPart::Gate), ((0, 0), SunPart::Corner(0)), ((10, 0), SunPart::Corner(1)), ((10, 9), SunPart::Corner(2)), ((0, 9), SunPart::Corner(3))];
    for y in 1..=8 {
        v.push(((0, y), SunPart::SideWall));
        v.push(((10, y), SunPart::SideWall));
    }
    for x in 1..=9 {
        v.push(((x, 0), SunPart::EndWall));
        if !(6..=7).contains(&x) {
            v.push(((x, 9), SunPart::EndWall));
        }
    }
    let planters = [(2, 2), (2, 4), (8, 2), (8, 4)];
    let palms = [(2, 1), (2, 3), (2, 5), (8, 1), (8, 3), (8, 5)];
    let (ox, oy) = SUN_OBELISK;
    for y in 1..=8 {
        for x in 1..=9 {
            let obelisk = (ox..ox + 5).contains(&x) && (oy..oy + 5).contains(&y);
            let part = if planters.contains(&(x, y)) {
                SunPart::Planter
            } else if palms.contains(&(x, y)) {
                SunPart::Palm
            } else if x == 7 && y >= 6 {
                SunPart::Path
            } else if obelisk {
                continue;
            } else {
                SunPart::Floor
            };
            v.push(((x, y), part));
        }
    }
    for y in 11..=17 {
        v.push(((6, y), SunPart::Path));
        v.push(((7, y), SunPart::Path));
    }
    v.push(((5, 18), SunPart::Fore));
    v
}

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
    /// Phases, the last being completion (pyramids and mastabas: their stages).
    pub phase_count: u8,
    /// The monument's worth toward the monument rating.
    pub weight: i32,
    /// Its name (text group 198).
    pub title: usize,
}

/// A mastaba (`cols` x `rows` blocks) or a pyramid (`blocks` square): built block by
/// block, in three stages (the site, the courses, the polishing).
const fn tomb(kind: u16, (cols, rows): (i32, i32), style: Style, weight: i32, title: usize) -> MonumentDef {
    MonumentDef { kind, cols, rows, style, phase_count: 4, weight, title }
}

const fn pyramid(kind: u16, family: Family, blocks: i32, weight: i32, title: usize) -> MonumentDef {
    tomb(kind, (blocks, blocks), Style::Pyramid(family), weight, title)
}

/// An obelisk: no leveling (it starts at the first building phase), timber for the
/// scaffolding in its first phases, stonemasons carving from its third until the
/// last art stage. Its work is counted per tile.
const fn obelisk(kind: u16, size: i32, stages: u8, granite: i32, timber: &'static [i32], weight: i32, title: usize) -> MonumentDef {
    MonumentDef { kind, cols: size, rows: size, style: Style::Obelisk { size, stages, granite, timber }, phase_count: LEVELING_PHASES + stages + 1, weight, title }
}

pub const MONUMENTS: [MonumentDef; 25] = [
    MonumentDef { kind: SPHINX, cols: 3, rows: 6, style: Style::Sphinx, phase_count: LEVELING_PHASES + SPHINX_PHASES.len() as u8 + 1, weight: 1, title: 21 },
    // The rating weight is a placeholder.
    MonumentDef { kind: MAUSOLEUM, cols: 11, rows: 4, style: Style::Mausoleum, phase_count: 6, weight: 4, title: 25 },
    // The rating weight is a placeholder.
    MonumentDef { kind: SUN_TEMPLE, cols: 1, rows: 1, style: Style::SunTemple, phase_count: SUN_FORE + 2, weight: 4, title: 24 },
    obelisk(SMALL_OBELISK, 3, 4, 100, &[200, 200, 200], 2, 22),
    obelisk(LARGE_OBELISK, 5, 6, 200, &[400, 400, 400, 200], 4, 23),
    // Sizes in blocks, as in the original: large is 8 across in every family.
    tomb(kind::SMALL_MASTABA, (2, 5), Style::Mastaba, 2, 18),
    tomb(kind::MEDIUM_MASTABA, (3, 7), Style::Mastaba, 2, 19),
    tomb(kind::LARGE_MASTABA, (4, 9), Style::Mastaba, 3, 20),
    pyramid(SMALL_STEPPED_PYRAMID, Family::Stepped, 4, 8, 8),
    pyramid(MEDIUM_STEPPED_PYRAMID, Family::Stepped, 6, 16, 9),
    pyramid(LARGE_STEPPED_PYRAMID, Family::Stepped, 8, 24, 10),
    pyramid(SMALL_BENT_PYRAMID, Family::Bent, 4, 12, 1),
    pyramid(MEDIUM_BENT_PYRAMID, Family::Bent, 6, 20, 2),
    pyramid(SMALL_MUDBRICK_PYRAMID, Family::Mudbrick, 4, 12, 3),
    pyramid(MEDIUM_MUDBRICK_PYRAMID, Family::Mudbrick, 6, 20, 4),
    pyramid(LARGE_MUDBRICK_PYRAMID, Family::Mudbrick, 8, 28, 5),
    pyramid(SMALL_PYRAMID, Family::True, 4, 16, 13),
    pyramid(MEDIUM_PYRAMID, Family::True, 6, 28, 14),
    pyramid(LARGE_PYRAMID, Family::True, 8, 40, 15),
    // Complexes: a larger pyramid with a mortuary temple, causeway and valley temple.
    // (Their rating weights are placeholders.)
    pyramid(STEPPED_PYRAMID_COMPLEX, Family::Stepped, 10, 32, 11),
    pyramid(GRAND_STEPPED_PYRAMID_COMPLEX, Family::Stepped, 12, 40, 12),
    pyramid(MUDBRICK_PYRAMID_COMPLEX, Family::Mudbrick, 10, 36, 6),
    pyramid(GRAND_MUDBRICK_PYRAMID_COMPLEX, Family::Mudbrick, 12, 44, 7),
    pyramid(PYRAMID_COMPLEX, Family::True, 10, 52, 16),
    pyramid(GRAND_PYRAMID_COMPLEX, Family::True, 12, 64, 17),
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
        Style::SunTemple => Some((SANDSTONE, SUN_TEMPLE_PLACEMENT)),
        _ => None,
    }
}

/// Phase, finished, and (resource, delivered, needed) for the phase's materials.
pub type MonumentStatus = (u8, bool, Vec<(u16, i32, i32)>);

/// Scaffolding pieces (image, pixel offset) and the tile they are placed from.
pub type Scaffold = (Vec<(u32, (i32, i32))>, (i32, i32));

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
    /// A pyramid's or mastaba's blocks.
    #[serde(default)]
    pub blocks: Vec<crate::pyramids::Block>,
    /// A pyramid complex's temples and causeway.
    #[serde(default)]
    pub parts: Vec<crate::pyramids::Part>,
}

impl Monument {
    pub(crate) fn amount(list: &[(u16, i32)], r: u16) -> i32 {
        list.iter().filter(|e| e.0 == r).map(|e| e.1).sum()
    }

    pub(crate) fn add(list: &mut Vec<(u16, i32)>, r: u16, n: i32) {
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
    /// The material phase `p` needs.
    pub fn phase(&self, p: u8) -> Vec<(u16, i32)> {
        match self.style {
            Style::Obelisk { timber, .. } => {
                let own = p.saturating_sub(LEVELING_PHASES) as usize;
                timber.get(own).map_or_else(Vec::new, |&t| vec![(TIMBER, t)])
            }
            Style::Sphinx => {
                let own = p.saturating_sub(LEVELING_PHASES) as usize;
                let Some(&(timber, paint, clay)) = SPHINX_PHASES.get(own) else { return Vec::new() };
                [(TIMBER, timber), (PAINT, paint), (CLAY, clay)].into_iter().filter(|m| m.1 > 0).collect()
            }
            Style::Mausoleum => {
                let blocks = (MAUSOLEUM_BLOCKS * MAUSOLEUM_PARTS.len()) as i32 * SLED_LOAD;
                match p {
                    1 | 3 => vec![(SANDSTONE, blocks)],
                    2 => vec![(TIMBER, MAUSOLEUM_TIMBER)],
                    _ => Vec::new(),
                }
            }
            Style::SunTemple => match p {
                SUN_GATE => vec![(SANDSTONE, SLED_LOAD)],
                SUN_WALLS => vec![(SANDSTONE, SLED_LOAD * sun_temple_parts().iter().filter(|p| p.1.is_wall()).count() as i32)],
                SUN_FORE => vec![(SANDSTONE, SLED_LOAD * SUN_FORE_SLEDS as i32)],
                _ => Vec::new(),
            },
            // Pyramids and mastabas take their material a unit at a time.
            Style::Pyramid(_) | Style::Mastaba => Vec::new(),
        }
    }

    /// The craftsmen phase `p` needs on site, the one who does the work first:
    /// bricklayers for bricks, stonemasons for stone, sandstone and carving,
    /// carpenters for timber. (Pyramids and mastabas choose theirs a unit at a time.)
    pub fn crew(&self, p: u8) -> Vec<u16> {
        match self.style {
            Style::SunTemple => {
                return match p {
                    1..=13 => vec![if SUN_OBELISK_STEPS[(p - 1) as usize].0 { CARPENTER } else { STONEMASON }],
                    SUN_GATE | SUN_WALLS | SUN_FORE => vec![STONEMASON],
                    _ => Vec::new(),
                };
            }
            Style::Pyramid(_) | Style::Mastaba => return Vec::new(),
            _ => {}
        }
        let phase = self.phase(p);
        let has = |r: u16| phase.iter().any(|e| e.0 == r);
        let mut crew = Vec::new();
        if has(BRICKS) {
            crew.push(BRICKLAYER);
        }
        let carving = matches!(self.style, Style::Obelisk { .. } | Style::Sphinx) && p >= LEVELING_PHASES + 2 && p + 1 < self.phase_count;
        if has(STONE) || has(LIMESTONE) || has(SANDSTONE) || carving {
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
            Style::SunTemple => match p {
                0 => sun_temple_parts().len(),
                SUN_WALLS => sun_temple_parts().iter().filter(|p| p.1.is_wall()).count(),
                SUN_FLOOR => sun_temple_parts().iter().filter(|p| p.1.is_floor()).count(),
                SUN_FORE => SUN_FORE_SLEDS,
                p if p <= SUN_GATE => 1,
                _ => 0,
            },
            _ => (self.cols * self.rows) as usize,
        }
    }

    /// Ticks of work each piece of phase `p` takes.
    pub fn unit_work(&self, p: u8) -> u16 {
        match self.style {
            Style::Mausoleum => MAUSOLEUM_WORK,
            Style::SunTemple => match p {
                1..=13 => SUN_OBELISK_STEPS[(p - 1) as usize].1,
                _ => MAUSOLEUM_WORK,
            },
            _ => BLOCK_WORK,
        }
    }

    /// Whether phase `p` is laborers' work: the levelling, and a mausoleum's
    /// courtyard.
    pub fn laborers(&self, p: u8) -> bool {
        match self.style {
            Style::Mausoleum => p == 0 || p == 4,
            Style::SunTemple => p == 0 || p == SUN_FLOOR,
            Style::Pyramid(_) | Style::Mastaba => p == crate::pyramids::PREP,
            _ => p < LEVELING_PHASES,
        }
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
        matches!(self.style, Style::Mastaba | Style::Pyramid(_) | Style::Mausoleum)
    }

}

impl World {
    /// The footprint of monument type `k` placed facing north.
    pub fn monument_footprint(&self, k: u16) -> Option<(i32, i32)> {
        monument_def(k).map(|d| match d.style {
            Style::Obelisk { size, .. } => (size, size),
            Style::Sphinx => (6, 18),
            Style::Mausoleum => MAUSOLEUM_SIZE,
            Style::SunTemple => SUN_TEMPLE_SIZE,
            _ => (d.cols * 2, d.rows * 2),
        })
    }

    /// Monument-specific placement rules: an obelisk's granite must be in storage,
    /// and only one obelisk may be under construction at a time.
    pub(crate) fn can_place_monument(&self, k: u16, (x, y): (i32, i32)) -> Result<(), &'static str> {
        let Some(def) = monument_def(k) else { return Ok(()) };
        if let Style::Pyramid(_) = def.style {
            // The row past the pyramid's south edge must be free (roads may cross it).
            let (w, h) = self.monument_footprint(k).unwrap_or((0, 0));
            let blocked = crate::map::mask::NOT_CLEAR & !(crate::map::terrain::ROAD | crate::map::terrain::TREE | crate::map::terrain::SHRUB);
            if (x..x + w).any(|xx| self.map.terrain_is(xx, y + h, blocked)) {
                return Err("Must be built on land free of obstructions");
            }
            self.complex_parts(def.style, crate::pyramids::variant(def.cols, def.style), (x, y))?;
        }
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
        if def.style == Style::SunTemple {
            if self.buildings.iter().any(|b| b.kind == SUN_TEMPLE && b.monument.as_ref().is_some_and(|m| !m.finished)) {
                return Err("You can only have one sun temple under construction at a time");
            }
            if self.yards_stored(SANDSTONE) < SUN_TEMPLE_PLACEMENT {
                return Err("You need 220 blocks of sandstone to build a sun temple");
            }
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
        if crate::pyramids::blockwise(def.style) {
            let variant = crate::pyramids::variant(def.cols, def.style);
            m.blocks = crate::pyramids::layout(def.style, variant);
            // A tick count for each tile's site work, and one for the centre's foundation.
            m.progress = vec![0; m.blocks.len() * 4 + 1];
            // A complex's temples and causeway take their ground now.
            m.parts = self.complex_parts(def.style, variant, (x0, y0)).unwrap_or_default();
            for (px, py) in Self::part_tiles(&m.parts) {
                self.map.terrain.update(x0 + px, y0 + py, |t| t | crate::map::terrain::BUILDING);
                self.map.building.set(x0 + px, y0 + py, id);
            }
        }
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
            Style::SunTemple => Some((SANDSTONE, SUN_TEMPLE_PLACEMENT)),
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

    /// The staked-out foundation: its corners and edges marked, stony ground within.
    pub(crate) fn foundation(site: u32, (x, y): (i32, i32), (x0, y0): (i32, i32), (x1, y1): (i32, i32)) -> u32 {
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
    pub(crate) fn trench_piece(n: bool, e: bool, s: bool, w: bool) -> u32 {
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
        if def.style == Style::SunTemple {
            // The gate, the corner piers and the fore temple's corners are staked out
            // until levelled.
            let Some(stake) = self.defs.building(b.kind).and_then(|d| d.anims.get("stake")).map(|a| a.image) else { return Vec::new() };
            let mut out = Vec::new();
            for (i, ((x, y), part)) in sun_temple_parts().into_iter().enumerate() {
                if m.progress.get(i).is_some_and(|&p| p >= MAUSOLEUM_WORK) {
                    continue;
                }
                match part {
                    SunPart::Gate | SunPart::Corner(_) => out.push((b.x + x, b.y + y, stake)),
                    SunPart::Fore => out.extend([(0, 0), (2, 0), (0, 2), (2, 2)].map(|(dx, dy)| (b.x + x + dx, b.y + y + dy, stake))),
                    _ => {}
                }
            }
            return out;
        }
        if crate::pyramids::blockwise(def.style) {
            return self.tomb_stakes(id);
        }
        Vec::new()
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

    /// The sun temple's tiles: each part bare, levelled, then built; the obelisk in its
    /// three looks as the carpenters and masons work it.
    fn refresh_sun_temple(&mut self, id: BuildingId) {
        let Some(b) = self.buildings.get(id) else { return };
        let (Some(bdef), Some(m)) = (self.defs.building(b.kind), b.monument.as_ref()) else { return };
        let (x0, y0) = (b.x, b.y);
        let img = |key: &str| bdef.anims.get(key).map(|a| a.image);
        let (Some(extras), Some(ground)) = (img("extras"), img("ground")) else { return };
        let (phase, finished) = (m.phase, m.finished);
        let done = |i: usize| m.progress.get(i).is_some_and(|&p| p >= MAUSOLEUM_WORK);
        let (w, h) = SUN_TEMPLE_SIZE;
        let bare = |x: i32, y: i32| m.ground.get((y * w + x) as usize).copied().filter(|&g| g != 0);
        let levelled = |x: i32, y: i32| ground + (((x * 7 + y * 3) & 7) as u32);
        // Tiles outside the parts keep their ground; the parts' tiles are set below.
        let mut singles: Vec<(i32, i32, u32)> = (0..h).flat_map(|y| (0..w).map(move |x| (x, y))).filter_map(|(x, y)| bare(x, y).map(|g| (x, y, g))).collect();
        let mut blocks: Vec<(i32, i32, i32, u32)> = Vec::new();
        let (mut walls, mut floors) = (0, 0);
        for (i, ((x, y), part)) in sun_temple_parts().into_iter().enumerate() {
            // Which piece of its phase's work this part is.
            let built = finished
                || match part {
                    SunPart::Gate => phase > SUN_GATE,
                    p if p.is_wall() => {
                        walls += 1;
                        phase > SUN_WALLS || phase == SUN_WALLS && done(walls - 1)
                    }
                    p if p.is_floor() => {
                        floors += 1;
                        phase > SUN_FLOOR || phase == SUN_FLOOR && done(floors - 1)
                    }
                    _ => phase > SUN_FORE,
                };
            let levelled_now = phase > 0 || done(i);
            let n = part.size();
            if built {
                blocks.push((x, y, n, extras + part.image()));
                continue;
            }
            for dy in 0..n {
                for dx in 0..n {
                    let (tx, ty) = (x + dx, y + dy);
                    let image = if levelled_now { levelled(tx, ty) } else { bare(tx, ty).unwrap_or_else(|| levelled(tx, ty)) };
                    singles.push((tx, ty, image));
                }
            }
        }
        let step = if finished { 13 } else { phase.saturating_sub(1).min(13) };
        let obelisk = img(match step {
            0..=8 => "obelisk1",
            9..=12 => "obelisk2",
            _ => "obelisk3",
        });
        for (x, y, image) in singles {
            self.map.set_single_image(x0 + x, y0 + y, image);
        }
        for (x, y, n, image) in blocks {
            self.map.set_footprint(x0 + x, y0 + y, n, image);
        }
        if let Some(image) = obelisk {
            self.map.set_footprint(x0 + SUN_OBELISK.0, y0 + SUN_OBELISK.1, 5, image);
        }
    }

    /// The scaffolding standing about a sun temple's obelisk as it is worked:
    /// (image, pixel offset from the obelisk's left edge and the top of its footprint).
    pub fn sun_temple_scaffold(&self, id: BuildingId) -> Scaffold {
        let none = (Vec::new(), (0, 0));
        let Some(b) = self.buildings.get(id).filter(|b| b.kind == SUN_TEMPLE) else { return none };
        let (Some(bdef), Some(m)) = (self.defs.building(b.kind), b.monument.as_ref()) else { return none };
        let Some(extras) = bdef.anims.get("extras").map(|a| a.image) else { return none };
        let step = if m.finished { 13 } else { m.phase.saturating_sub(1).min(13) };
        let pieces = match step {
            1 | 10 | 11 => 1,
            2 | 8 | 9 => 2,
            3 | 6 | 7 => 3,
            4 | 5 => 4,
            _ => 0,
        };
        let at = [(60, -29), (187, -29), (82, -96), (170, -100)];
        let list = at.iter().take(pieces).enumerate().map(|(i, &o)| (extras + 13 + (i % 2) as u32, o)).collect();
        (list, (b.x + SUN_OBELISK.0, b.y + SUN_OBELISK.1))
    }

    /// Redraws a monument's ground-level tiles for its phase and the blocks done so far.
    pub fn refresh_monument_images(&mut self, id: BuildingId) {
        let Some(b) = self.buildings.get(id) else { return };
        let Some(def) = monument_def(b.kind) else { return };
        let Some(m) = b.monument.as_ref() else { return };
        let (phase, finished) = (m.phase, m.finished);
        let (x0, y0) = (b.x, b.y);
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
        if def.style == Style::SunTemple {
            self.refresh_sun_temple(id);
            return;
        }
        if let Style::Obelisk { size, stages, .. } = def.style {
            let stage = if finished { stages } else { phase.saturating_sub(LEVELING_PHASES).clamp(1, stages) };
            let key = ["sa", "sb", "sc", "sd", "se", "sf"][(stage - 1) as usize];
            let image = bdef.anims.get(key).map_or(site, |a| a.image);
            self.map.set_footprint(x0, y0, size, image);
            return;
        }
        if crate::pyramids::blockwise(def.style) {
            // The site's ground, then each block begun: its course on the map, or
            // (higher courses) drawn raised over it.
            for ((x, y), image) in self.tomb_site(id) {
                self.map.set_single_image(x0 + x, y0 + y, image);
            }
            for (bx, by, image, lift) in self.tomb_blocks(id) {
                if lift == 0 {
                    self.map.set_footprint(bx, by, 2, image);
                }
            }
        }
    }

    /// The images a monument shows above its site: (block x, block y, image, lift in
    /// pixels). Lift 0 is drawn on the map; the rest are raised over it.
    pub fn monument_stacks(&self, id: BuildingId) -> Vec<(i32, i32, u32, i32)> {
        let Some(def) = self.buildings.get(id).and_then(|b| monument_def(b.kind)) else { return Vec::new() };
        if crate::pyramids::blockwise(def.style) { self.tomb_blocks(id) } else { Vec::new() }
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
                let block = if crate::pyramids::blockwise(def.style) {
                    let taken: Vec<i32> = busy.iter().filter(|b| b.0 == id).map(|b| b.1).collect();
                    self.tomb_site_job(id, &taken)?
                } else {
                    let work = def.unit_work(m.phase);
                    m.progress.iter().enumerate().position(|(i, &p)| p < work && !busy.contains(&(id, i as i32)))?
                };
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
        let busy: Vec<i32> = self.figures.iter().filter(|f| f.kind == crate::farms::PEASANT && f.action >= 3 && f.target == id && f.id != me).map(|f| f.amount).collect();
        if crate::pyramids::blockwise(def.style) {
            return self.tomb_site_job(id, &busy);
        }
        let work = def.unit_work(m.phase);
        m.progress.iter().enumerate().position(|(i, &p)| p < work && !busy.contains(&(i as i32)))
    }

    /// A laborer at a monument works its block; true when the block is done.
    pub(crate) fn level_block(&mut self, id: BuildingId, block: usize) -> bool {
        let Some(def) = self.buildings.get(id).and_then(|b| monument_def(b.kind)) else { return true };
        let Some(m) = self.buildings.get_mut(id).and_then(|b| b.monument.as_mut()) else { return true };
        if !def.laborers(m.phase) {
            return true;
        }
        if crate::pyramids::blockwise(def.style) {
            // A touch of one tile of the site; the tile then shows the step.
            let Some(p) = m.progress.get_mut(block) else { return true };
            *p += 1;
            if *p < crate::pyramids::TILE_WORK {
                return false;
            }
            *p = 0;
            self.finish_site_touch(id, block);
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
        if crate::pyramids::blockwise(def.style) {
            return self.tomb_wants(id, figure);
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
            // A pyramid or mastaba takes a craftsman from every guild; other monuments
            // one of each.
            let target = self.active_monuments().into_iter().find(|&id| {
                let tomb = self.buildings.get(id).and_then(|b| monument_def(b.kind)).is_some_and(|d| crate::pyramids::blockwise(d.style));
                self.wants_craftsman(id, figure) && (tomb || !self.buildings.get(id).and_then(|b| b.monument.as_ref()).is_some_and(|m| m.has_craftsman(figure)))
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
        let busy = f.amount > 0;
        if act != 3 && !(listed && (busy || self.wants_craftsman(target, figure))) {
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
            2 if self.buildings.get(target).and_then(|b| monument_def(b.kind)).is_some_and(|d| crate::pyramids::blockwise(d.style)) => self.work_on_tomb(fid),
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

    /// A craftsman at a pyramid or mastaba: he takes on a block's next unit (and the
    /// material for it), a step of polishing, or a ramp (the carpenters' guild giving
    /// up timber for it), and works it through.
    fn work_on_tomb(&mut self, fid: FigureId) {
        use crate::pyramids::Job;
        let encode = |j: Job| match j {
            Job::Unit(i) => 1 + i as i32,
            Job::Polish(i) => 100_000 + i as i32,
            Job::Ramp(i) => 200_000 + i as i32,
            Job::Part(i) => 300_000 + i as i32,
        };
        let decode = |a: i32| match a {
            a if a >= 300_000 => Some(Job::Part((a - 300_000) as usize)),
            a if a >= 200_000 => Some(Job::Ramp((a - 200_000) as usize)),
            a if a >= 100_000 => Some(Job::Polish((a - 100_000) as usize)),
            a if a > 0 => Some(Job::Unit((a - 1) as usize)),
            _ => None,
        };
        let Some(f) = self.figures.get(fid) else { return };
        let (target, figure, home, held, ticks) = (f.target, f.kind, f.home, f.amount, f.counter);
        match decode(held) {
            Some(job) => {
                let done = ticks + 1 >= Self::tomb_job_work(job) as i32;
                if let Some(f) = self.figures.get_mut(fid) {
                    f.counter = if done { 0 } else { ticks + 1 };
                    f.amount = if done { 0 } else { held };
                    f.moving = !done;
                }
                if done {
                    self.finish_tomb_job(target, job);
                }
            }
            None => {
                let crew: Vec<FigureId> = self.buildings.get(target).and_then(|b| b.monument.as_ref()).map_or_else(Vec::new, |m| m.craftsmen.iter().map(|c| c.1).collect());
                let taken: Vec<Job> = crew.iter().filter(|&&c| c != fid).filter_map(|&c| self.figures.get(c).and_then(|o| decode(o.amount))).collect();
                let job = self.tomb_job(target, figure, &taken);
                match job {
                    Some(Job::Unit(i)) => {
                        let r = self.tomb_unit_material(target, i);
                        if let (Some(r), Some(m)) = (r, self.buildings.get_mut(target).and_then(|b| b.monument.as_mut())) {
                            Monument::add(&mut m.delivered, r, -crate::pyramids::UNIT_MATERIAL);
                        }
                    }
                    Some(Job::Ramp(_)) => {
                        if let Some(g) = self.buildings.get_mut(home)
                            && let Some(t) = g.stock.get_mut(TIMBER as usize)
                        {
                            *t = (*t - crate::pyramids::RAMP_TIMBER).max(0);
                        }
                    }
                    _ => {}
                }
                if let Some(f) = self.figures.get_mut(fid) {
                    f.amount = job.map_or(0, encode);
                    f.counter = 0;
                    f.moving = job.is_some();
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
            let wants: Vec<(u16, i32)> = if crate::pyramids::blockwise(def.style) {
                self.tomb_material_wants(id)
            } else {
                def.phase(m.phase).into_iter().map(|(r, _)| (r, def.needs(m, r))).collect()
            };
            for (r, need) in wants {
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
            let (x, y) = (b.x, b.y);
            let finished = if crate::pyramids::blockwise(def.style) {
                self.advance_tomb(id)
            } else {
                let m = b.monument.as_ref().expect("active");
                let all_done = m.progress.iter().all(|&p| p >= def.unit_work(m.phase));
                let paid = def.phase(m.phase).iter().all(|&(r, want)| Monument::amount(&m.delivered, r) >= want);
                if !all_done || !paid {
                    continue;
                }
                let last = def.phase_count - 1;
                let m = self.buildings.get_mut(id).and_then(|b| b.monument.as_mut()).expect("active");
                m.phase += 1;
                m.delivered.clear();
                m.progress = vec![0; def.units(m.phase)];
                m.phase >= last
            };
            if finished {
                let m = self.buildings.get_mut(id).and_then(|b| b.monument.as_mut()).expect("active");
                m.finished = true;
                let name = match def.style {
                    Style::Mastaba => "mastaba",
                    Style::Pyramid(Family::Stepped) => "stepped_pyramid",
                    Style::Pyramid(Family::Bent) => "bent_pyramid",
                    Style::Pyramid(Family::True) => "pyramid",
                    Style::Pyramid(Family::Mudbrick) => "mudbrick_pyramid",
                    Style::Obelisk { .. } => "obelisk",
                    Style::Sphinx => "sphinx",
                    Style::Mausoleum => "mausoleum",
                    Style::SunTemple => "sun_temple",
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
    /// for each material of the phase what has been delivered and what is needed (for
    /// a pyramid or mastaba, what is on site and what the rest of it needs).
    pub fn monument_status(&self, id: BuildingId) -> Option<MonumentStatus> {
        let b = self.buildings.get(id)?;
        let def = monument_def(b.kind)?;
        let m = b.monument.as_ref()?;
        let needs = if crate::pyramids::blockwise(def.style) {
            self.tomb_remaining(id).into_iter().map(|(r, rest)| (r, Monument::amount(&m.delivered, r), rest)).collect()
        } else {
            def.phase(m.phase).iter().map(|&(r, want)| (r, Monument::amount(&m.delivered, r), want)).collect()
        };
        Some((m.phase, m.finished, needs))
    }

    /// A game saved before pyramids and mastabas were built block by block: those
    /// still under way start over on their site, and finished ones keep their look.
    pub(crate) fn upgrade_monuments(&mut self) {
        let old: Vec<(BuildingId, bool)> = self
            .buildings
            .iter()
            .filter(|b| monument_def(b.kind).is_some_and(|d| crate::pyramids::blockwise(d.style)))
            .filter_map(|b| b.monument.as_ref().filter(|m| m.blocks.is_empty()).map(|m| (b.id, m.finished)))
            .collect();
        for (id, finished) in old {
            let Some(def) = self.buildings.get(id).and_then(|b| monument_def(b.kind)) else { continue };
            let mut blocks = crate::pyramids::layout(def.style, crate::pyramids::variant(def.cols, def.style));
            if finished {
                crate::pyramids::finish_blocks(def.style, &mut blocks);
            }
            if let Some(m) = self.buildings.get_mut(id).and_then(|b| b.monument.as_mut()) {
                m.progress = vec![0; blocks.len() * 4 + 1];
                m.blocks = blocks;
                m.phase = if finished { crate::pyramids::POLISH } else { crate::pyramids::PREP };
                m.delivered.clear();
            }
            self.refresh_monument_images(id);
        }
    }

    /// The craftsmen a monument wants now.
    pub fn monument_crew(&self, id: BuildingId) -> Vec<u16> {
        let Some(b) = self.buildings.get(id) else { return Vec::new() };
        let (Some(def), Some(m)) = (monument_def(b.kind), b.monument.as_ref()) else { return Vec::new() };
        if crate::pyramids::blockwise(def.style) {
            return [STONEMASON, BRICKLAYER, CARPENTER].into_iter().filter(|&k| self.tomb_wants(id, k)).collect();
        }
        def.crew(m.phase)
    }

    /// How far along a monument is, 0-100.
    pub fn monument_percent(&self, id: BuildingId) -> i32 {
        let Some(b) = self.buildings.get(id) else { return 0 };
        let (Some(def), Some(m)) = (monument_def(b.kind), b.monument.as_ref()) else { return 0 };
        if m.finished {
            return 100;
        }
        if crate::pyramids::blockwise(def.style) {
            return self.tomb_percent(id).unwrap_or(0);
        }
        let units = m.progress.len().max(1) as i32;
        let within = m.progress.iter().map(|&p| p as i32).sum::<i32>() * 100 / (units * def.unit_work(m.phase).max(1) as i32);
        ((m.phase as i32 * 100 + within) / (def.phase_count as i32 - 1).max(1)).min(99)
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
