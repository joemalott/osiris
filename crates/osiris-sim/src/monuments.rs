//! Monuments. Each is built in phases, each phase a number of pieces of work:
//! laborers from work camps level and lay out the site, and guild craftsmen do the
//! rest; a mason who lays a block of stone waits for a laborer to drag its sled of
//! 400 units from a storage yard to him. When every piece of a phase is done, the
//! next begins; after the last, the monument is complete. Pyramids and mastabas are
//! built block by block instead: see [`crate::pyramids`].

use crate::buildings::{BuildingId, kind};
use crate::figures::{Figure, FigureId, Step, Travel};
use crate::world::World;

/// Progress a block needs in each phase: one worker adds one point a tick.
pub const BLOCK_WORK: u16 = 200;
/// Most a sled carries.
pub const SLED_LOAD: i32 = 400;
/// The progress a carpenters' guild builds up before each carpenter goes out, and
/// the timber he takes with him.
const CARPENTER_PROGRESS: i32 = 400;
const CARPENTER_TIMBER: i32 = 100;
/// Men pulling each sled besides the laborer at their head, and how many ticks each
/// puller and the sled keep behind the man ahead.
const SLED_PULLERS: usize = 5;
const PULLER_GAP: i32 = 7;
const SLED_GAP: i32 = 12;

pub const CARPENTER: u16 = 79;
pub const BRICKLAYER: u16 = 80;
pub const STONEMASON: u16 = 81;
pub const SLED: u16 = 86;
pub const SLED_PULLER: u16 = 96;
pub const FUNERAL_WALKER: u16 = 94;
/// A work-camp laborer's actions with a sled: going to the storage yard for it
/// (the original's state 15), loading it there (16), dragging it to the monument
/// (30 and 17 to a tomb, 23 and 26 to a sun temple or mausoleum), up a tomb's way up
/// (18), across to his mason (19 and 20), and standing there after (29).
pub const HAULING: u16 = 7;
pub const AT_YARD: u16 = 14;
pub const SLED_TO_TOMB: u16 = 10;
pub const SLED_CLIMB: u16 = 11;
pub const SLED_CROSS: u16 = 12;
pub const SLED_DONE: u16 = 13;

/// Whether a laborer's action is one of his sled's.
pub(crate) fn is_hauling(action: u16) -> bool {
    matches!(action, HAULING | AT_YARD | SLED_TO_TOMB | SLED_CLIMB | SLED_CROSS | SLED_DONE)
}

/// A stonemason laying a block of a mausoleum or sun temple: crossing the site to the
/// tile he lays it from (the original's mason state 9), and there, waiting for its
/// sled and then working it (10 and 11).
const SITE_CROSS: u16 = 6;
pub const AT_SPOT: u16 = 7;

const BRICKS: u16 = 12;
const TIMBER: u16 = 20;
const STONE: u16 = 24;
const LIMESTONE: u16 = 25;
const GRANITE: u16 = 26;
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
    /// Granite paid for when placed, then its `steps` of carpenters' scaffolding and
    /// masons' carving, one job each; `size` tiles square, drawn in `stages` images.
    Obelisk { size: i32, stages: u8, granite: i32, steps: &'static [Job] },
    /// Three 6x6 parts in a row along x (head, body, tail), carved from a buried
    /// outcrop in fifteen steps of carpenters' and stonemasons' jobs.
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
    /// A royal burial tomb, cut chamber by chamber into a cliff: see
    /// [`crate::royal_tombs`]. Its `cols` and `rows` are its bulk in tiles.
    RoyalTomb,
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

/// A craftsman's job on an obelisk, sphinx or sun temple obelisk: the craftsman
/// (carpenter or stonemason) and the ticks it takes him. A carpenter brings his
/// timber from his guild; no sleds are dragged.
pub type Job = (u16, u16);

/// The obelisks' steps, one job each (the original's tables at 0x586b08 and
/// 0x58fe48).
const SMALL_OBELISK_STEPS: [Job; 10] = [
    (CARPENTER, 320),
    (CARPENTER, 320),
    (CARPENTER, 600),
    (CARPENTER, 600),
    (STONEMASON, 580),
    (STONEMASON, 580),
    (STONEMASON, 520),
    (STONEMASON, 520),
    (STONEMASON, 340),
    (STONEMASON, 340),
];
const LARGE_OBELISK_STEPS: [Job; 18] = [
    (CARPENTER, 320),
    (CARPENTER, 320),
    (CARPENTER, 610),
    (CARPENTER, 610),
    (CARPENTER, 680),
    (CARPENTER, 680),
    (CARPENTER, 750),
    (CARPENTER, 750),
    (STONEMASON, 740),
    (STONEMASON, 740),
    (STONEMASON, 670),
    (STONEMASON, 670),
    (STONEMASON, 500),
    (STONEMASON, 500),
    (STONEMASON, 530),
    (STONEMASON, 530),
    (STONEMASON, 340),
    (STONEMASON, 340),
];

/// An obelisk's progress starts at 33 when placed and each finished step adds
/// 67/steps (rounded); it is done at 100. The step worked is the one that progress
/// falls in, so with 18 steps (progress 4 a step) the twelfth is never reached.
const OBELISK_START: i32 = 33;
const OBELISK_DONE: i32 = 100;

const fn obelisk_increment(steps: usize) -> i32 {
    (67 * 2 + steps as i32) / (2 * steps as i32)
}

/// The step (0-based) an obelisk of `steps` steps works after `k` finished ones, or
/// `None` once it is done.
const fn obelisk_step(steps: usize, k: usize) -> Option<usize> {
    let progress = OBELISK_START + obelisk_increment(steps) * k as i32;
    if progress >= OBELISK_DONE {
        return None;
    }
    let step = (progress - OBELISK_START) as usize * steps / 66 + 1;
    Some(if step > steps { steps - 1 } else { step - 1 })
}

/// How many steps an obelisk of `steps` steps works in all.
const fn obelisk_worked(steps: usize) -> u8 {
    let mut k = 0;
    while obelisk_step(steps, k).is_some() {
        k += 1;
    }
    k as u8
}

/// The obelisk's image (0-based of `stages`) after `k` finished steps: it only
/// begins to change past progress 66.
fn obelisk_image(steps: usize, stages: u8, k: usize) -> u8 {
    let progress = (OBELISK_START + obelisk_increment(steps) * k as i32).min(OBELISK_DONE);
    let scaled = ((progress - 66) * 3).max(0);
    (stages as i32 * scaled / 100).min(stages as i32 - 1) as u8
}

/// The sphinx's jobs: (step, craftsman, ticks). A step's jobs are worked side by
/// side, each by its own craftsman (the original's table at 0x5c2748).
const SPHINX_JOBS: [(u8, u16, u16); 22] = [
    (1, CARPENTER, 500),
    (2, CARPENTER, 700),
    (3, CARPENTER, 400),
    (4, STONEMASON, 600),
    (4, STONEMASON, 800),
    (4, STONEMASON, 400),
    (5, STONEMASON, 600),
    (5, STONEMASON, 800),
    (5, STONEMASON, 1200),
    (6, STONEMASON, 800),
    (7, STONEMASON, 1200),
    (7, STONEMASON, 650),
    (7, CARPENTER, 600),
    (8, STONEMASON, 650),
    (8, STONEMASON, 1200),
    (9, STONEMASON, 650),
    (10, STONEMASON, 650),
    (11, STONEMASON, 900),
    (12, STONEMASON, 500),
    (13, STONEMASON, 400),
    (14, CARPENTER, 540),
    (15, STONEMASON, 450),
];
const SPHINX_STEPS: u8 = 15;

/// The carving stage (1-6) each of the sphinx's three parts shows at a step (1-15;
/// 16 when finished): head first, then body, then tail, in turn.
fn sphinx_stage(step: u8, part: u8) -> u8 {
    (1 + (step + 1).saturating_sub(part) / 3).min(6)
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

/// An obelisk: no leveling (it starts at the first building phase), then a phase for
/// each step it works.
const fn obelisk(kind: u16, size: i32, stages: u8, granite: i32, steps: &'static [Job], weight: i32, title: usize) -> MonumentDef {
    MonumentDef { kind, cols: size, rows: size, style: Style::Obelisk { size, stages, granite, steps }, phase_count: LEVELING_PHASES + obelisk_worked(steps.len()) + 1, weight, title }
}

/// A royal tomb: its bulk in tiles, its one phase being the cutting.
const fn royal_tomb(kind: u16, (cols, rows): (i32, i32), weight: i32, title: usize) -> MonumentDef {
    MonumentDef { kind, cols, rows, style: Style::RoyalTomb, phase_count: 2, weight, title }
}

pub const MONUMENTS: [MonumentDef; 29] = [
    // The rating weights are placeholders.
    royal_tomb(crate::royal_tombs::SMALL_ROYAL_TOMB, (11, 20), 4, 33),
    royal_tomb(crate::royal_tombs::MEDIUM_ROYAL_TOMB, (14, 16), 8, 34),
    royal_tomb(crate::royal_tombs::LARGE_ROYAL_TOMB, (17, 33), 13, 35),
    royal_tomb(crate::royal_tombs::GRAND_ROYAL_TOMB, (29, 23), 18, 36),
    MonumentDef { kind: SPHINX, cols: 3, rows: 1, style: Style::Sphinx, phase_count: LEVELING_PHASES + SPHINX_STEPS + 1, weight: 1, title: 21 },
    // The rating weight is a placeholder.
    MonumentDef { kind: MAUSOLEUM, cols: 11, rows: 4, style: Style::Mausoleum, phase_count: 6, weight: 4, title: 25 },
    // The rating weight is a placeholder.
    MonumentDef { kind: SUN_TEMPLE, cols: 1, rows: 1, style: Style::SunTemple, phase_count: SUN_FORE + 2, weight: 4, title: 24 },
    // Granite taken at placement: 100 and 200 blocks (the original's placement check).
    obelisk(SMALL_OBELISK, 3, 4, 10_000, &SMALL_OBELISK_STEPS, 2, 22),
    obelisk(LARGE_OBELISK, 5, 6, 20_000, &LARGE_OBELISK_STEPS, 4, 23),
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

/// What a craftsman at a pyramid or mastaba is doing: walking about it, waiting (for
/// work, or for the sled with his unit's material), or working the ticks so far of
/// a job (`laying`: a unit of a course, whose first ticks look different).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TombPose {
    Walk,
    Idle,
    Work { ticks: i32, laying: bool },
}

/// Phase, finished, and (resource, delivered, needed) for the phase's materials.
pub type MonumentStatus = (u8, bool, Vec<(u16, i32, i32)>);

/// Scaffolding pieces (image, pixel offset) and the tile they are placed from.
pub type Scaffold = (Vec<(u32, (i32, i32))>, (i32, i32));

/// Phases that level the site.
const LEVELING_PHASES: u8 = 2;

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
    /// A royal tomb's chambers.
    #[serde(default)]
    pub chambers: Vec<crate::royal_tombs::ChamberState>,
    /// A royal tomb's lamps, and whether a laborer is fetching more.
    #[serde(default)]
    pub lamps: i32,
    #[serde(default)]
    pub lamp_run: bool,
    /// A royal tomb whose completion has been announced.
    #[serde(default)]
    pub announced: bool,
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
            // Carved from what was paid for at placing, or the rock; carpenters bring
            // their guild's timber.
            Style::Obelisk { .. } | Style::Sphinx => Vec::new(),
            Style::Mausoleum => {
                let blocks = (MAUSOLEUM_BLOCKS * MAUSOLEUM_PARTS.len()) as i32 * SLED_LOAD;
                match p {
                    1 | 3 => vec![(SANDSTONE, blocks)],
                    _ => Vec::new(),
                }
            }
            Style::SunTemple => match p {
                SUN_GATE => vec![(SANDSTONE, SLED_LOAD)],
                SUN_WALLS => vec![(SANDSTONE, SLED_LOAD * sun_temple_parts().iter().filter(|p| p.1.is_wall()).count() as i32)],
                SUN_FORE => vec![(SANDSTONE, SLED_LOAD * SUN_FORE_SLEDS as i32)],
                _ => Vec::new(),
            },
            // Pyramids and mastabas take their material a unit at a time; royal tombs'
            // artisans bring theirs.
            Style::Pyramid(_) | Style::Mastaba | Style::RoyalTomb => Vec::new(),
        }
    }

    /// The craftsmen phase `p` needs on site, the one who does the work first:
    /// bricklayers for bricks, stonemasons for stone, sandstone and carving,
    /// carpenters for timber. (Pyramids and mastabas choose theirs a unit at a time.)
    pub fn crew(&self, p: u8) -> Vec<u16> {
        let jobs = self.jobs(p);
        if !jobs.is_empty() {
            let mut crew: Vec<u16> = Vec::new();
            for (figure, _) in jobs {
                if !crew.contains(&figure) {
                    crew.push(figure);
                }
            }
            return crew;
        }
        match self.style {
            Style::SunTemple => {
                return match p {
                    SUN_GATE | SUN_WALLS | SUN_FORE => vec![STONEMASON],
                    _ => Vec::new(),
                };
            }
            // A mausoleum's ramps are carpenters' work.
            Style::Mausoleum if p == 2 => return vec![CARPENTER],
            Style::Pyramid(_) | Style::Mastaba | Style::RoyalTomb | Style::Obelisk { .. } | Style::Sphinx => return Vec::new(),
            _ => {}
        }
        let phase = self.phase(p);
        let has = |r: u16| phase.iter().any(|e| e.0 == r);
        let mut crew = Vec::new();
        if has(BRICKS) {
            crew.push(BRICKLAYER);
        }
        if has(STONE) || has(LIMESTONE) || has(SANDSTONE) {
            crew.push(STONEMASON);
        }
        if has(TIMBER) {
            crew.push(CARPENTER);
        }
        crew
    }

    /// The craftsmen's jobs of phase `p` of an obelisk, sphinx or sun temple
    /// obelisk, each worked by one craftsman who then goes home; none for other
    /// phases and monuments.
    pub fn jobs(&self, p: u8) -> Vec<Job> {
        match self.style {
            Style::Obelisk { steps, .. } => {
                let Some(k) = p.checked_sub(LEVELING_PHASES) else { return Vec::new() };
                obelisk_step(steps.len(), k as usize).map_or_else(Vec::new, |i| vec![steps[i]])
            }
            Style::Sphinx => {
                let step = p.saturating_sub(LEVELING_PHASES) + 1;
                SPHINX_JOBS.iter().filter(|j| p >= LEVELING_PHASES && j.0 == step).map(|j| (j.1, j.2)).collect()
            }
            Style::SunTemple => match p {
                1..=13 => {
                    let (carpenter, work) = SUN_OBELISK_STEPS[(p - 1) as usize];
                    vec![(if carpenter { CARPENTER } else { STONEMASON }, work)]
                }
                _ => Vec::new(),
            },
            _ => Vec::new(),
        }
    }

    /// Ticks of work piece `i` of phase `p` takes.
    pub fn work_of(&self, p: u8, i: usize) -> u16 {
        self.jobs(p).get(i).map_or_else(|| self.unit_work(p), |j| j.1)
    }

    /// Pieces of work in phase `p`: a block of the site for most monuments; for a
    /// mausoleum its courtyard tiles, storey blocks or ramps.
    pub fn units(&self, p: u8) -> usize {
        let jobs = self.jobs(p).len();
        if jobs > 0 {
            return jobs;
        }
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
            // A tomb's laborers only fetch lamps.
            Style::RoyalTomb => false,
            _ => p < LEVELING_PHASES,
        }
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

    /// Whether phase `p` is laid a block at a time by stonemasons, each block a sled
    /// of sandstone dragged to the mason who lays it: a mausoleum's two storeys, a sun
    /// temple's gate, walls and fore temple.
    pub fn sled_phase(&self, p: u8) -> bool {
        match self.style {
            Style::Mausoleum => p == 1 || p == 3,
            Style::SunTemple => matches!(p, SUN_GATE | SUN_WALLS | SUN_FORE),
            _ => false,
        }
    }

    /// The tile (from the monument's corner) the mason laying block `i` of sled phase
    /// `p` stands on: at a mausoleum one of the four beside its part, each laying two
    /// of a storey's eight blocks (the original's part table at 0x5e2ad8); at a sun
    /// temple the part itself.
    fn sled_block_spot(&self, p: u8, i: usize) -> Option<(i32, i32)> {
        match self.style {
            Style::Mausoleum => {
                let px = *MAUSOLEUM_PARTS.get(i / MAUSOLEUM_BLOCKS)?;
                Some([(px, 1), (px + 3, 1), (px, 6), (px + 3, 6)][i % MAUSOLEUM_BLOCKS / 2])
            }
            Style::SunTemple => {
                let parts = sun_temple_parts();
                let spot = match p {
                    SUN_GATE => parts.iter().find(|q| q.1 == SunPart::Gate),
                    SUN_WALLS => parts.iter().filter(|q| q.1.is_wall()).nth(i),
                    SUN_FORE => parts.iter().find(|q| q.1 == SunPart::Fore),
                    _ => None,
                };
                spot.map(|q| q.0)
            }
            _ => None,
        }
    }

    /// Whether the monument is a tomb, which takes burial provisions.
    pub fn is_tomb(&self) -> bool {
        matches!(self.style, Style::Mastaba | Style::Pyramid(_) | Style::Mausoleum | Style::RoyalTomb)
    }

}

/// A craftsman's tomb job, kept in his `amount`.
fn encode_tomb_job(j: crate::pyramids::Job) -> i32 {
    use crate::pyramids::Job;
    match j {
        Job::Unit(i) => 1 + i as i32,
        Job::Polish(i) => 100_000 + i as i32,
        Job::Ramp(i) => 200_000 + i as i32,
        Job::Part(i) => 300_000 + i as i32,
    }
}

pub(crate) fn decode_tomb_job(a: i32) -> Option<crate::pyramids::Job> {
    use crate::pyramids::Job;
    match a {
        a if a >= 300_000 => Some(Job::Part((a - 300_000) as usize)),
        a if a >= 200_000 => Some(Job::Ramp((a - 200_000) as usize)),
        a if a >= 100_000 => Some(Job::Polish((a - 100_000) as usize)),
        a if a > 0 => Some(Job::Unit((a - 1) as usize)),
        _ => None,
    }
}

impl World {
    /// The footprint of monument type `k` placed facing north.
    pub fn monument_footprint(&self, k: u16) -> Option<(i32, i32)> {
        monument_def(k).map(|d| match d.style {
            Style::Obelisk { size, .. } => (size, size),
            // Its three parts side by side along x in every view (the original's
            // placement at 0x470990 puts them at x, x+6 and x+12).
            Style::Sphinx => (18, 6),
            Style::Mausoleum => MAUSOLEUM_SIZE,
            Style::SunTemple => SUN_TEMPLE_SIZE,
            Style::RoyalTomb => (d.cols, d.rows),
            _ => (d.cols * 2, d.rows * 2),
        })
    }

    /// The row of tiles just past a pyramid's or mastaba's south edge, for one placed
    /// at `(x, y)` (the original's tables at 0x570094 and 0x5700a8); none for others.
    pub fn tomb_row(&self, k: u16, (x, y): (i32, i32)) -> Vec<(i32, i32)> {
        if !monument_def(k).is_some_and(|d| crate::pyramids::blockwise(d.style)) {
            return Vec::new();
        }
        let (w, h) = self.monument_footprint(k).unwrap_or((0, 0));
        (x..x + w).map(|xx| (xx, y + h)).collect()
    }

    /// Monument-specific placement rules: an obelisk's granite must be in storage,
    /// and only one obelisk may be under construction at a time.
    pub(crate) fn can_place_monument(&self, k: u16, (x, y): (i32, i32)) -> Result<(), &'static str> {
        let Some(def) = monument_def(k) else { return Ok(()) };
        if crate::pyramids::blockwise(def.style) {
            // The row past a pyramid's or mastaba's south edge must be free (roads may
            // cross it), and a complex's causeway must reach the water.
            if let Some(why) = self.tomb_row(k, (x, y)).into_iter().find_map(|(xx, yy)| self.tomb_row_problem(xx, yy)) {
                return Err(why);
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
                use crate::map::terrain::{BUILDING, SHRUB, TREE};
                self.map.terrain.update(x0 + px, y0 + py, |t| (t & !(TREE | SHRUB)) | BUILDING);
                self.map.building.set(x0 + px, y0 + py, id);
            }
        }
        if def.style == Style::Sphinx {
            // Carved from the rock where it stands: no leveling.
            m.phase = LEVELING_PHASES;
        }
        if def.style == Style::RoyalTomb {
            if let Some(b) = self.buildings.get_mut(id) {
                b.monument = Some(m);
            }
            self.place_royal_tomb(id);
            self.refresh_royal_tomb(id);
            return;
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
        if !crate::pyramids::blockwise(def.style) {
            m.progress = vec![0; def.units(m.phase)];
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
            let step = if finished { SPHINX_STEPS + 1 } else { phase.saturating_sub(LEVELING_PHASES) + 1 };
            for (part, letter) in ["a", "b", "c"].iter().enumerate() {
                let stage = sphinx_stage(step, part as u8);
                let image = bdef.anims.get(&format!("s{stage}{letter}1")).map_or(site, |a| a.image);
                self.map.set_footprint(x0 + 6 * part as i32, y0, 6, image);
            }
            return;
        }
        if def.style == Style::Mausoleum {
            self.refresh_mausoleum(id);
            return;
        }
        if def.style == Style::RoyalTomb {
            self.refresh_royal_tomb(id);
            return;
        }
        if def.style == Style::SunTemple {
            self.refresh_sun_temple(id);
            return;
        }
        if let Style::Obelisk { size, stages, steps, .. } = def.style {
            let done = if finished { steps.len() } else { phase.saturating_sub(LEVELING_PHASES) as usize };
            let key = ["sa", "sb", "sc", "sd", "se", "sf"][obelisk_image(steps.len(), stages, done) as usize];
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
        if crate::royal_tombs::is_royal_tomb(b.kind) {
            return self.royal_tomb_access(id, from);
        }
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

    /// Where a sled for monument `id` is dragged over the ground: the foot of a
    /// pyramid's or mastaba's way up (the laborer's destination from `FUN_004f3000`),
    /// else the edge of the site nearest the tile its mason `mason` lays his block
    /// from, whence it is dragged across the site to him.
    fn sled_spot(&self, id: BuildingId, mason: FigureId) -> Option<(i32, i32)> {
        if !self.tomb_route(id).is_empty() {
            return self.tomb_entry(id);
        }
        let at = self.mason_spot(mason)?;
        self.monument_access(id, at)
    }

    /// The map tile a mason lays block `i` of monument `id`'s phase from.
    fn site_spot(&self, id: BuildingId, i: usize) -> Option<(i32, i32)> {
        let b = self.buildings.get(id)?;
        let (def, m) = (monument_def(b.kind)?, b.monument.as_ref()?);
        def.sled_block_spot(m.phase, i).map(|(x, y)| (b.x + x, b.y + y))
    }

    /// The tile mason `mason` lays his block of a mausoleum or sun temple from.
    fn mason_spot(&self, mason: FigureId) -> Option<(i32, i32)> {
        let f = self.figures.get(mason)?;
        let i = usize::try_from(f.amount - 1).ok()?;
        self.site_spot(f.target, i)
    }

    /// The blocks of a mausoleum's or sun temple's sled phase a mason could be sent
    /// to lay (`FUN_004e05f0`, `FUN_005421d0`): not yet laid, no mason sent for it,
    /// and the tile it is laid from free of any other mason; each tile lays its
    /// blocks in turn.
    fn site_open_blocks(&self, id: BuildingId) -> Vec<usize> {
        let Some(b) = self.buildings.get(id) else { return Vec::new() };
        let (Some(def), Some(m)) = (monument_def(b.kind), b.monument.as_ref()) else { return Vec::new() };
        if !def.sled_phase(m.phase) {
            return Vec::new();
        }
        let work = def.unit_work(m.phase);
        let held: Vec<usize> = m
            .craftsmen
            .iter()
            .filter_map(|c| self.figures.get(c.1))
            .filter(|f| !f.dead && f.kind == STONEMASON && f.amount > 0)
            .map(|f| f.amount as usize - 1)
            .collect();
        let mut used: Vec<(i32, i32)> = held.iter().filter_map(|&i| def.sled_block_spot(m.phase, i)).collect();
        let mut out = Vec::new();
        for (i, &p) in m.progress.iter().enumerate() {
            if p >= work {
                continue;
            }
            let Some(spot) = def.sled_block_spot(m.phase, i) else { continue };
            if used.contains(&spot) {
                continue;
            }
            used.push(spot);
            out.push(i);
        }
        out
    }

    /// A mason at a mausoleum or sun temple waiting on his tile for his block's sled
    /// that no laborer is fetching yet.
    fn site_mason_waiting(&self, id: BuildingId) -> Option<FigureId> {
        let m = self.buildings.get(id)?.monument.as_ref()?;
        m.craftsmen.iter().map(|c| c.1).find(|&c| {
            self.figures
                .get(c)
                .is_some_and(|f| !f.dead && f.kind == STONEMASON && f.target == id && f.action == AT_SPOT && f.cargo == 0 && f.amount > 0 && self.sled_laborer(f).is_none())
        })
    }

    /// A monument block a work-camp laborer could level: the first block without a
    /// laborer on it, of the nearest monument still being levelled.
    pub(crate) fn leveling_job(&self, from: (i32, i32)) -> Option<(BuildingId, usize)> {
        let busy: Vec<(u32, i32)> = self.figures.iter().filter(|f| f.kind == crate::farms::PEASANT && matches!(f.action, 3 | 4 | 8)).map(|f| (f.target, f.amount)).collect();
        self.active_monuments()
            .into_iter()
            .filter_map(|id| {
                let b = self.buildings.get(id)?;
                let m = b.monument.as_ref()?;
                let def = monument_def(b.kind)?;
                // (As many laborers as there are tiles free: the original sets no limit.)
                if !def.laborers(m.phase) {
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
        let busy: Vec<i32> = self.figures.iter().filter(|f| f.kind == crate::farms::PEASANT && matches!(f.action, 3 | 4 | 8) && f.target == id && f.id != me).map(|f| f.amount).collect();
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
        if def.style == Style::RoyalTomb {
            return self.royal_tomb_job(id, figure).is_some();
        }
        def.crew(m.phase).contains(&figure)
    }

    /// Tick 31: guilds send a craftsman to a monument that needs one, up to four out
    /// at a time by their staffing (the original's guild spawners at 0x4612c0,
    /// 0x461490 and 0x4615d0).
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
            // As many craftsmen out as a quarter of its staff each (carpenters: the
            // first even with a handful), four when fully staffed.
            let pct = gb.workers * 100 / self.workers_needed(gb.kind).max(1);
            let cap = match pct {
                p if p >= 100 => 4,
                p if p >= 75 => 3,
                p if p >= 50 => 2,
                p if p >= 25 || figure == CARPENTER && p >= 1 => 1,
                _ => 0,
            };
            let out = self.figures.iter().filter(|f| f.kind == figure && f.home == g && !f.dead).count();
            if gb.workers <= 0 || out >= cap {
                continue;
            }
            // A carpenter goes out only once his guild has built up its progress (the
            // original's carpenters' guild at 0x4612c0).
            if figure == CARPENTER && gb.progress < CARPENTER_PROGRESS {
                continue;
            }
            let from = (gb.x, gb.y);
            // A pyramid or mastaba takes a craftsman from every guild, and a royal tomb
            // one for each chamber; an obelisk or sphinx one for each open job; other
            // monuments one of each.
            let target = self.active_monuments().into_iter().find(|&id| {
                let tomb = self.buildings.get(id).and_then(|b| monument_def(b.kind)).is_some_and(|d| crate::pyramids::blockwise(d.style) || d.style == Style::RoyalTomb);
                self.wants_craftsman(id, figure) && (tomb || self.open_jobs(id, figure) > 0)
            });
            let Some(target) = target else { continue };
            if self.buildings.get(target).is_some_and(|b| crate::royal_tombs::is_royal_tomb(b.kind)) {
                self.send_tomb_mason(g, road, target);
                continue;
            }
            // A pyramid's or mastaba's craftsmen are sent to a block of their own and
            // come to the foot of its way up; a mason with no block may be sent to the
            // site of a part of its complex.
            let tomb = !self.tomb_route(target).is_empty();
            let job = if tomb { self.tomb_free_job(target, figure) } else { None };
            // At a mausoleum or sun temple a mason is sent to lay a block, the one laid
            // from the tile nearest his guild, and comes to the edge of the site there.
            let block = self.site_open_blocks(target).into_iter().filter_map(|i| Some((self.site_spot(target, i)?, i))).min_by_key(|&((x, y), i)| ((x - from.0).abs() + (y - from.1).abs(), i));
            let block = block.filter(|_| figure == STONEMASON);
            let spot = match block {
                _ if tomb => self.tomb_entry(target),
                Some((at, _)) => self.monument_access(target, at),
                None => self.monument_access(target, from),
            };
            let Some(spot) = spot else { continue };
            let part = match job {
                Some(crate::pyramids::Job::Part(i)) => self.part_tile(target, i),
                _ => None,
            };
            let fid = self.figures.spawn(figure, road.0, road.1, Travel::Land);
            let map = &self.map;
            if let Some(f) = self.figures.get_mut(fid) {
                f.home = g;
                f.target = target;
                f.action = 1;
                f.amount = match block {
                    Some((_, i)) => i as i32 + 1,
                    None => job.map_or(0, encode_tomb_job),
                };
                if !part.is_some_and(|p| f.go_to(map, p)) && !f.go_to(map, spot) {
                    f.dead = true;
                    continue;
                }
            }
            let gb = self.buildings.get_mut(g).expect("present");
            gb.walkers[0] = fid;
            if figure == CARPENTER {
                // He takes a load of the guild's timber with him, and the guild starts
                // over.
                gb.progress = 0;
                if let Some(t) = gb.stock.get_mut(TIMBER as usize) {
                    *t = (*t - CARPENTER_TIMBER).max(0);
                }
            }
            if let Some(m) = self.buildings.get_mut(target).and_then(|b| b.monument.as_mut()) {
                m.craftsmen.push((figure, fid));
            }
        }
    }

    /// Tick 20: a carpenters' guild holding any timber builds up a point of progress
    /// for each worker, to at most 400 (the original's daily production at 0x455900).
    pub(crate) fn build_up_carpenters(&mut self) {
        for b in self.buildings.iter_mut().filter(|b| b.kind == kind::CARPENTERS_GUILD) {
            if b.workers > 0 && b.stock.get(TIMBER as usize).is_some_and(|&t| t > 0) {
                b.progress = (b.progress + b.workers).min(CARPENTER_PROGRESS);
            }
        }
    }

    /// How many more of craftsman `figure` monument `id` has room for: one for each
    /// of its jobs he could take that no one has come for, or for other monuments one
    /// if none of his kind is there.
    fn open_jobs(&self, id: BuildingId, figure: u16) -> usize {
        let Some(b) = self.buildings.get(id) else { return 0 };
        let (Some(def), Some(m)) = (monument_def(b.kind), b.monument.as_ref()) else { return 0 };
        if def.sled_phase(m.phase) {
            return if figure == STONEMASON { self.site_open_blocks(id).len() } else { 0 };
        }
        let jobs = def.jobs(m.phase);
        if jobs.is_empty() {
            return usize::from(!m.has_craftsman(figure));
        }
        let open = jobs.iter().enumerate().filter(|&(i, j)| j.0 == figure && m.progress.get(i).is_some_and(|&p| p < j.1)).count();
        open.saturating_sub(m.craftsmen.iter().filter(|c| c.0 == figure).count())
    }

    /// A craftsman walks to the monument and stays while its course needs him. The
    /// first of the phase's crew lays each block as far as the delivered material
    /// allows, and only while the rest of the crew is there too.
    pub(crate) fn update_craftsman(&mut self, fid: FigureId) {
        let Some(f) = self.figures.get(fid) else { return };
        if self.buildings.get(f.target).is_some_and(|b| crate::royal_tombs::is_royal_tomb(b.kind)) {
            self.update_tomb_worker(fid);
            return;
        }
        if !self.tomb_route(f.target).is_empty() {
            self.update_tomb_craftsman(fid);
            return;
        }
        let (act, target, figure) = (f.action, f.target, f.kind);
        let sleds = self.buildings.get(target).and_then(|b| Some(monument_def(b.kind)?.sled_phase(b.monument.as_ref()?.phase))).unwrap_or(false);
        if figure == STONEMASON && matches!(act, 1 | 2 | 5 | SITE_CROSS | AT_SPOT) && (sleds || matches!(act, 5 | SITE_CROSS | AT_SPOT)) {
            self.update_site_mason(fid);
            return;
        }
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
            2 if self.buildings.get(target).and_then(|b| Some(monument_def(b.kind)?.jobs(b.monument.as_ref()?.phase))).is_some_and(|j| !j.is_empty()) => self.work_job(fid),
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

    /// A stonemason laying a block of a mausoleum or sun temple (the original's mason
    /// states 9-12 there): he crosses the site from its edge to the tile he lays it
    /// from and waits there for a laborer to drag its sled to him; the sled starts
    /// him, and when the block is laid he goes home.
    fn update_site_mason(&mut self, fid: FigureId) {
        let Some(f) = self.figures.get(fid) else { return };
        let (act, target, held, cargo) = (f.action, f.target, f.amount, f.cargo);
        if act == 5 {
            // Off the site, then home.
            let map = &self.map;
            let f = self.figures.get_mut(fid).expect("present");
            if f.walk(map) != Step::Moving {
                f.travel = Travel::Land;
                self.send_home(fid);
            }
            return;
        }
        let listed = self.buildings.get(target).and_then(|b| b.monument.as_ref()).is_some_and(|m| m.craftsmen.contains(&(STONEMASON, fid)));
        let Some(b) = self.buildings.get(target) else { return self.leave_site(fid) };
        let (Some(def), Some(m)) = (monument_def(b.kind), b.monument.as_ref()) else { return self.leave_site(fid) };
        let block = usize::try_from(held - 1).ok().filter(|&i| listed && def.sled_phase(m.phase) && m.progress.get(i).is_some_and(|&p| p < def.unit_work(m.phase)));
        let (Some(i), Some(spot)) = (block, block.and_then(|i| self.site_spot(target, i))) else { return self.leave_site(fid) };
        let work = def.unit_work(m.phase);
        let map = &self.map;
        let f = self.figures.get_mut(fid).expect("present");
        match act {
            1 | SITE_CROSS => match f.walk(map) {
                Step::Moving => {}
                Step::Arrived if (f.x, f.y) == spot => {
                    f.action = AT_SPOT;
                    f.travel = Travel::Land;
                    f.moving = false;
                    f.counter = 0;
                }
                Step::Arrived if act == 1 => {
                    f.action = SITE_CROSS;
                    f.travel = Travel::Any;
                    if !f.go_to(map, spot) {
                        self.leave_site(fid);
                    }
                }
                _ => self.leave_site(fid),
            },
            2 => {
                // (Come to the site before its blocks were laid this way.)
                f.action = 1;
                f.travel = Travel::Any;
                if !f.go_to(map, spot) {
                    self.leave_site(fid);
                }
            }
            _ if cargo == 0 => {
                f.moving = false;
                f.counter += 1;
            }
            _ => {
                f.moving = true;
                let m = self.buildings.get_mut(target).and_then(|b| b.monument.as_mut()).expect("present");
                m.progress[i] += 1;
                if m.progress[i] >= work {
                    self.refresh_monument_images(target);
                    self.leave_site(fid);
                }
            }
        }
    }

    /// A mason at a mausoleum or sun temple is done there: he walks off the site and
    /// goes home.
    fn leave_site(&mut self, fid: FigureId) {
        let Some(f) = self.figures.get(fid) else { return };
        let (target, at) = (f.target, (f.x, f.y));
        if let Some(m) = self.buildings.get_mut(target).and_then(|b| b.monument.as_mut()) {
            m.craftsmen.retain(|c| c.1 != fid);
        }
        let on_site = !crate::figures::passable(&self.map, Travel::Land, at.0, at.1);
        let spot = if on_site { self.monument_access(target, at) } else { None };
        let map = &self.map;
        let f = self.figures.get_mut(fid).expect("present");
        f.amount = 0;
        f.cargo = 0;
        f.counter = 0;
        f.moving = false;
        f.link = 0;
        if let Some(spot) = spot {
            f.travel = Travel::Any;
            f.action = 5;
            if f.go_to(map, spot) {
                return;
            }
        }
        f.travel = Travel::Land;
        self.send_home(fid);
    }

    /// A craftsman at an obelisk, sphinx or sun temple obelisk takes one of the
    /// phase's jobs of his trade that no one else holds, works it through a tick at a
    /// time, and goes home (the original's tables at 0x586b08, 0x58fe48, 0x5c2748 and
    /// 0x596080: a job is held by its craftsman, and a carpenter's guild pays for each).
    fn work_job(&mut self, fid: FigureId) {
        let Some(f) = self.figures.get(fid) else { return };
        let (target, figure, held) = (f.target, f.kind, f.amount);
        let Some(b) = self.buildings.get(target) else { return };
        let (Some(def), Some(m)) = (monument_def(b.kind), b.monument.as_ref()) else { return };
        let jobs = def.jobs(m.phase);
        let open = |i: usize| jobs.get(i).is_some_and(|j| j.0 == figure && m.progress.get(i).is_some_and(|&p| p < j.1));
        let job = if held > 0 {
            Some(held as usize - 1).filter(|&i| open(i))
        } else {
            let taken: Vec<i32> = m.craftsmen.iter().filter(|c| c.1 != fid).filter_map(|c| self.figures.get(c.1)).map(|o| o.amount).collect();
            (0..jobs.len()).find(|&i| open(i) && !taken.contains(&(i as i32 + 1)))
        };
        let Some(i) = job else {
            self.leave_monument(fid);
            return;
        };
        let work = jobs[i].1;
        let m = self.buildings.get_mut(target).and_then(|b| b.monument.as_mut()).expect("working");
        m.progress[i] += 1;
        let done = m.progress[i] >= work;
        if let Some(f) = self.figures.get_mut(fid) {
            f.amount = i as i32 + 1;
            f.moving = true;
        }
        if done {
            self.refresh_monument_images(target);
            self.leave_monument(fid);
        }
    }

    /// Craftsman `fid` is done at his monument and goes home.
    fn leave_monument(&mut self, fid: FigureId) {
        let Some(target) = self.figures.get(fid).map(|f| f.target) else { return };
        if let Some(m) = self.buildings.get_mut(target).and_then(|b| b.monument.as_mut()) {
            m.craftsmen.retain(|c| c.1 != fid);
        }
        if let Some(f) = self.figures.get_mut(fid) {
            f.amount = 0;
            f.moving = false;
        }
        self.send_home(fid);
    }

    /// A pyramid's or mastaba's craftsman (the original's carpenter, bricklayer and
    /// stonemason AIs at 0x4a7603, 0x4a7ea4 and 0x4a87fc): he walks to the foot of the
    /// way up, climbs it to the height the building has reached, crosses to his block
    /// and works there, and goes home when his work at that height is done.
    fn update_tomb_craftsman(&mut self, fid: FigureId) {
        let Some(f) = self.figures.get(fid) else { return };
        let (act, target, figure, busy) = (f.action, f.target, f.kind, f.amount > 0);
        let listed = self.buildings.get(target).and_then(|b| b.monument.as_ref()).is_some_and(|m| m.craftsmen.contains(&(figure, fid)));
        match act {
            1 | 2 if !listed || !busy && !self.wants_craftsman(target, figure) => self.leave_tomb(fid),
            1 => {
                let map = &self.map;
                let f = self.figures.get_mut(fid).expect("present");
                match f.walk(map) {
                    Step::Moving => {}
                    Step::Arrived => {
                        f.action = 2;
                        f.moving = false;
                        f.travel = Travel::Land;
                    }
                    _ => f.dead = true,
                }
            }
            2 => self.work_on_tomb(fid),
            5 => {
                // Down off the tomb, then home.
                let map = &self.map;
                let f = self.figures.get_mut(fid).expect("present");
                if f.walk(map) != Step::Moving {
                    f.travel = Travel::Land;
                    self.send_home(fid);
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

    /// A craftsman at a pyramid or mastaba, with the work he was sent for (or, waiting
    /// at the foot, work he takes there): a block's next unit, a step of polishing, a
    /// ramp, or on the ground a part of a complex. Up on the tomb he climbs the way up
    /// (the original's `FUN_004f04d0`) until he is as high as his work, or for a ramp
    /// at its block, then crosses to his block (`FUN_004f0ef0`). There a mason waits
    /// for the unit's material and lays it, and a carpenter builds his ramp.
    fn work_on_tomb(&mut self, fid: FigureId) {
        use crate::pyramids::{Job, Perch};
        let Some(f) = self.figures.get(fid) else { return };
        let (target, figure, held, perch) = (f.target, f.kind, f.amount, f.perch);
        if let Some(mut p) = perch
            && !p.arrived()
        {
            p.step += 1;
            let f = self.figures.get_mut(fid).expect("present");
            f.perch = Some(p);
            f.moving = true;
            f.anim_tick += 1;
            return;
        }
        self.figures.get_mut(fid).expect("present").moving = false;
        let job = match decode_tomb_job(held) {
            Some(job) => job,
            None => {
                let crew: Vec<FigureId> = self.buildings.get(target).and_then(|b| b.monument.as_ref()).map_or_else(Vec::new, |m| m.craftsmen.iter().map(|c| c.1).collect());
                let taken: Vec<Job> = crew.iter().filter(|&&c| c != fid).filter_map(|&c| self.figures.get(c).and_then(|o| decode_tomb_job(o.amount))).collect();
                // Up on the tomb a craftsman always has work; at its foot he may wait
                // for some while the tomb wants his trade.
                let Some(job) = self.tomb_job(target, figure, &taken, perch.is_some()) else {
                    if perch.is_some() || !self.wants_craftsman(target, figure) {
                        self.leave_tomb(fid);
                    }
                    return;
                };
                let f = self.figures.get_mut(fid).expect("present");
                f.amount = encode_tomb_job(job);
                f.counter = 0;
                f.cargo = 0;
                job
            }
        };
        if let Job::Part(i) = job {
            // Worked on the ground at the part's site.
            let Some(spot) = self.part_tile(target, i) else {
                self.leave_tomb(fid);
                return;
            };
            let f = self.figures.get(fid).expect("present");
            if f.perch.is_some() {
                self.leave_tomb(fid);
                return;
            }
            if (f.x, f.y) != spot {
                let map = &self.map;
                let f = self.figures.get_mut(fid).expect("present");
                f.action = 1;
                if !f.go_to(map, spot) {
                    f.travel = Travel::Any;
                    f.go_to(map, spot);
                }
                return;
            }
            self.work_tomb_job(fid, job, 0);
            return;
        }
        let Some((block, height)) = self.job_spot(target, job) else {
            self.leave_tomb(fid);
            return;
        };
        let Some(p) = perch else {
            // At the foot: he steps up.
            let Some(&(first, _)) = self.tomb_route(target).first() else { return };
            self.figures.get_mut(fid).expect("present").perch = Some(Perch::foot(first as u16));
            return;
        };
        // A polisher walking round his ring looks again at each block he reaches.
        if let Job::Polish(_) = job
            && p.from.is_some()
            && p.route == 0
            && matches!(p.stage, crate::pyramids::OVER_FRONT | crate::pyramids::OVER_BACK)
            && p.block as usize != block
        {
            self.polish_round(fid, p.block as usize);
            return;
        }
        if p.at_foot() || p.block as usize != block || p.height != height {
            let route = self.tomb_route(target);
            let next = p.route as usize;
            let climbing = (p.at_foot() || p.route > 0)
                && next < route.len()
                && match job {
                    Job::Ramp(i) => route[next..].iter().any(|&(b, h)| b as usize == i && h == height),
                    _ => p.height < height,
                };
            if climbing {
                let (b, h) = route[next];
                self.perch_move(fid, b as usize, h, next as u8 + 1, true);
            } else {
                self.perch_move(fid, block, height, 0, false);
            }
            return;
        }
        // On his block. A mason waits there for a laborer to drag the unit's material
        // up to him, now and then turning where he stands (0x4a95b4, 0x4a868c).
        if let Job::Unit(_) = job
            && self.figures.get(fid).is_some_and(|f| f.cargo == 0)
        {
            let span = if figure == BRICKLAYER { 9 } else { 6 };
            let f = self.figures.get(fid).expect("present");
            let e = f.counter / 2 + fid as i32 * 13;
            if (e / span) & 3 == 2 && e & 7 == 4 {
                // (The original rolls C's rand() here, not the game's own random
                // numbers; a hash of the figure and the tick stands in for it.)
                let turn = (fid as u64).wrapping_mul(2_654_435_761).wrapping_add(self.time.total_ticks.wrapping_mul(40_503)) >> 16 & 3;
                let f = self.figures.get_mut(fid).expect("present");
                match turn {
                    0 => f.direction = (f.direction + 1) & 7,
                    1 => f.direction = (f.direction + 7) & 7,
                    _ => {}
                }
            }
            self.figures.get_mut(fid).expect("present").counter += 1;
            return;
        }
        self.work_tomb_job(fid, job, p.height);
    }

    /// A tick of a craftsman's work on job `job`, standing at height `height`; when it
    /// is done, what it made is added to the tomb and he moves on or goes home.
    fn work_tomb_job(&mut self, fid: FigureId, job: crate::pyramids::Job, height: u8) {
        use crate::pyramids::Job;
        let Some(f) = self.figures.get_mut(fid) else { return };
        f.counter += 1;
        if f.counter < Self::tomb_job_work(job) as i32 {
            return;
        }
        let (target, figure) = (f.target, f.kind);
        f.counter = 0;
        f.amount = 0;
        f.cargo = 0;
        self.finish_tomb_job(target, job);
        // A mason goes on to a free block where he stands, the height the building
        // is at (`FUN_004f0ef0`), or a polisher round his ring to the next block to
        // polish (`FUN_004f2230`); with none he goes home.
        let crew: Vec<FigureId> = self.buildings.get(target).and_then(|b| b.monument.as_ref()).map_or_else(Vec::new, |m| m.craftsmen.iter().map(|c| c.1).collect());
        let taken: Vec<Job> = crew.iter().filter(|&&c| c != fid).filter_map(|&c| self.figures.get(c).and_then(|o| decode_tomb_job(o.amount))).collect();
        let next = match job {
            Job::Unit(_) => self.next_unit(target, figure, &taken, height).map(Job::Unit),
            Job::Polish(i) => {
                self.figures.get_mut(fid).expect("present").amount = 0;
                return self.polish_round(fid, i);
            }
            _ => None,
        };
        match next {
            Some(next) => self.figures.get_mut(fid).expect("present").amount = encode_tomb_job(next),
            None => self.leave_tomb(fid),
        }
    }

    /// The block a mason who has laid a unit goes on to (`FUN_004f0ef0`): of the
    /// free blocks to be laid at his height with his material, the one farthest from
    /// the head of the way up (a block counting as far as the farther of it and its
    /// sled partner), preferring one whose partner's mason already waits for a sled,
    /// then one with a partner to be laid.
    fn next_unit(&self, id: BuildingId, figure: u16, taken: &[crate::pyramids::Job], height: u8) -> Option<usize> {
        use crate::pyramids::Job;
        let front = self.frontier(id);
        let route = self.tomb_route(id);
        let low = self.block_height(id, *front.first()?);
        let head = route.iter().find(|&&(_, h)| low <= h).or(route.last()).map_or(0, |r| r.0 as usize);
        let h = self.block_tile(id, head)?;
        let d2 = |i: usize| self.block_tile(id, i).map_or(0, |(x, y)| (x - h.0).pow(2) + (y - h.1).pow(2));
        let mut best: Option<(usize, i32, bool, bool)> = None;
        for &i in &front {
            if taken.contains(&Job::Unit(i)) || self.block_waits_for_ramp(id, i) || self.unit_craftsman(id, i) != figure || self.block_height(id, i) != height {
                continue;
            }
            let partner = self.sled_partner(id, i).map(|p| p.0).filter(|p| front.contains(p));
            let score = 2 * d2(i).max(partner.map_or(0, d2));
            let waits = partner.and_then(|p| self.block_mason(id, p)).is_some_and(|c| self.figures.get(c).is_some_and(|m| self.mason_waiting(m).is_some()));
            let better = match best {
                None => true,
                Some((_, s, w, p)) => score > s || score == s && (waits && !w || partner.is_some() && !p),
            };
            if better {
                best = Some((i, score, waits, partner.is_some()));
            }
        }
        best.map(|b| b.0)
    }

    /// A polisher goes round his ring (`FUN_004f2230`): he polishes the block he is on
    /// if it is next to be polished and nobody has it, else he looks on round the
    /// ring for one, takes it and steps one block on toward it; finding none all the
    /// way round, he goes home.
    fn polish_round(&mut self, fid: FigureId, at: usize) {
        use crate::pyramids::Job;
        let Some(f) = self.figures.get(fid) else { return };
        let (target, height) = (f.target, f.perch.map_or(0, |p| p.height));
        let crew: Vec<FigureId> = self.buildings.get(target).and_then(|b| b.monument.as_ref()).map_or_else(Vec::new, |m| m.craftsmen.iter().map(|c| c.1).collect());
        let taken: Vec<Job> = crew.iter().filter(|&&c| c != fid).filter_map(|&c| self.figures.get(c).and_then(|o| decode_tomb_job(o.amount))).collect();
        let front = self.polish_frontier(target);
        let free = |i: usize| front.contains(&i) && !taken.contains(&Job::Polish(i));
        let mut b = at;
        let found = loop {
            if free(b) {
                break Some(b);
            }
            b = self.ring_next(target, b);
            if b == at {
                break None;
            }
        };
        let Some(j) = found else {
            self.leave_tomb(fid);
            return;
        };
        self.figures.get_mut(fid).expect("present").amount = encode_tomb_job(Job::Polish(j));
        if j != at {
            let next = self.ring_next(target, at);
            let h = if next == j { self.block_height(target, j) } else { height };
            self.perch_move(fid, next, h, 0, true);
        }
    }

    /// Starts a figure up on a tomb toward block `block` at height `height`, fifteen
    /// ticks a tile (`FUN_004f02f0`); `route` is the entry of the way up it is, plus
    /// one, or 0. A move on the way up or round a ring (`on_way`) is over a front or
    /// back face by the block it goes to; any other crosses the top.
    fn perch_move(&mut self, fid: FigureId, block: usize, height: u8, route: u8, on_way: bool) {
        let Some(f) = self.figures.get(fid) else { return };
        let (Some(p), target) = (f.perch, f.target) else { return };
        let (Some(from), Some(to)) = (self.block_tile(target, p.block as usize), self.block_tile(target, block)) else { return };
        let (dx, dy) = (to.0 - from.0, to.1 - from.1);
        let tiles = ((dx * dx + dy * dy) as f64).sqrt() as u16;
        let stage = match on_way {
            false => crate::pyramids::CROSSING,
            true if self.block_faces_back(target, block) => crate::pyramids::OVER_BACK,
            true => crate::pyramids::OVER_FRONT,
        };
        let f = self.figures.get_mut(fid).expect("present");
        f.perch = Some(crate::pyramids::Perch {
            from: Some(p.block),
            from_height: p.height,
            block: block as u16,
            height,
            step: 0,
            steps: tiles * 15,
            route,
            stage,
        });
        if let Some(d) = crate::figures::direction_to(from, to) {
            f.direction = d;
        }
        f.x = to.0;
        f.y = to.1;
    }

    /// What craftsman `f` is doing at a pyramid or mastaba, for drawing him; `None` if
    /// he is not there (walking to or from it on the ground).
    pub fn tomb_pose(&self, f: &Figure) -> Option<TombPose> {
        use crate::pyramids::Job;
        if !matches!(f.kind, CARPENTER | BRICKLAYER | STONEMASON) || f.action != 2 || self.tomb_route(f.target).is_empty() {
            return None;
        }
        if f.moving {
            return Some(TombPose::Walk);
        }
        let Some(job) = decode_tomb_job(f.amount) else { return Some(TombPose::Idle) };
        let there = match job {
            Job::Part(i) => f.perch.is_none() && self.part_tile(f.target, i) == Some((f.x, f.y)),
            _ => f.perch.is_some_and(|p| p.arrived() && !p.at_foot() && self.job_spot(f.target, job).is_some_and(|(b, h)| p.block as usize == b && p.height == h)),
        };
        Some(match job {
            _ if !there => TombPose::Idle,
            Job::Unit(_) if f.cargo == 0 => TombPose::Idle,
            Job::Unit(_) => TombPose::Work { ticks: f.counter, laying: true },
            _ => TombPose::Work { ticks: f.counter, laying: false },
        })
    }

    /// A tomb craftsman is done there: he gives back a unit's material he has not
    /// laid, and goes home, first stepping down off the tomb if he is up on it.
    fn leave_tomb(&mut self, fid: FigureId) {
        let Some(f) = self.figures.get(fid) else { return };
        let (target, perch) = (f.target, f.perch);
        if let Some(m) = self.buildings.get_mut(target).and_then(|b| b.monument.as_mut()) {
            m.craftsmen.retain(|c| c.1 != fid);
        }
        let at = perch.and_then(|p| self.block_tile(target, p.block as usize));
        let spot = at.and_then(|t| self.monument_access(target, t));
        let map = &self.map;
        let f = self.figures.get_mut(fid).expect("present");
        f.amount = 0;
        f.cargo = 0;
        f.counter = 0;
        f.moving = false;
        f.perch = None;
        f.link = 0;
        if let (Some(at), Some(spot)) = (at, spot) {
            f.x = at.0;
            f.y = at.1;
            f.progress = 0;
            f.travel = Travel::Any;
            f.action = 5;
            if !f.go_to(map, spot) {
                f.dead = true;
            }
            return;
        }
        f.travel = Travel::Land;
        self.send_home(fid);
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

    /// A sled a work-camp laborer could fetch for a monument, nearest the camp at
    /// `from` first: (monument, storage yard, resource, amount, mason). As the
    /// original's laborer dispatcher (`FUN_004beeb0`) has it, a sled is fetched only
    /// for a mason waiting for one that no laborer is fetching for yet: on a pyramid
    /// or mastaba one waiting on his block (`FUN_004f29e0`), a sled of what the block's
    /// next unit is made of; at a mausoleum or sun temple one waiting on the tile he
    /// lays his block from (`FUN_004e05f0`, `FUN_005421d0`), a sled of sandstone.
    /// Obelisks and the sphinx get no sleds.
    pub(crate) fn haul_job(&self, from: (i32, i32)) -> Option<(BuildingId, BuildingId, u16, i32, FigureId)> {
        let mut monuments: Vec<BuildingId> = self.active_monuments();
        monuments.sort_by_key(|&id| self.buildings.get(id).map_or(i32::MAX, |b| (b.x - from.0).abs() + (b.y - from.1).abs()));
        for id in monuments {
            let Some(b) = self.buildings.get(id) else { continue };
            let Some(def) = monument_def(b.kind) else { continue };
            let m = b.monument.as_ref().expect("active");
            let (r, mason) = if crate::pyramids::blockwise(def.style) {
                let Some(mason) = self.sled_mason(id, false, 0) else { continue };
                let block = self.figures.get(mason).and_then(|f| self.mason_waiting(f));
                let Some(r) = block.and_then(|i| self.tomb_unit_material(id, i)) else { continue };
                (r, mason)
            } else if def.sled_phase(m.phase) {
                let Some(mason) = self.site_mason_waiting(id) else { continue };
                let Some(&(r, _)) = def.phase(m.phase).first() else { continue };
                (r, mason)
            } else {
                continue;
            };
            let at = (b.x, b.y);
            let yard = self
                .buildings
                .iter()
                .filter(|y| y.kind == kind::STORAGE_YARD && y.road.is_some() && self.stored(y.id, r) >= SLED_LOAD)
                .min_by_key(|y| ((y.x - at.0).abs() + (y.y - at.1).abs(), y.id));
            let Some(yard) = yard else { continue };
            let road = yard.road.expect("filtered");
            let Some(spot) = self.sled_spot(id, mason) else { continue };
            if crate::figures::find_route(&self.map, Travel::Land, road, spot).is_none() {
                continue;
            }
            return Some((id, yard.id, r, SLED_LOAD, mason));
        }
        None
    }

    /// Sends laborer `fid` (just spawned) to fetch a sled: to the yard first. The
    /// load counts as on its way from now, and the mason it is for knows him.
    pub(crate) fn send_hauler(&mut self, fid: FigureId, (monument, yard, r, amount, mason): (BuildingId, BuildingId, u16, i32, FigureId)) -> bool {
        let Some(road) = self.buildings.get(yard).and_then(|y| y.road) else { return false };
        let map = &self.map;
        let Some(f) = self.figures.get_mut(fid) else { return false };
        f.target = monument;
        f.yard = yard;
        f.counter = 0;
        f.cargo = r;
        f.amount = amount;
        f.action = HAULING;
        f.link = mason;
        if !f.go_to(map, road) {
            return false;
        }
        if let Some(m) = self.figures.get_mut(mason) {
            m.link = fid;
        }
        if let Some(m) = self.buildings.get_mut(monument).and_then(|b| b.monument.as_mut()) {
            Monument::add(&mut m.in_flight, r, amount);
        }
        true
    }

    /// The block a mason waits on for his sled (the original's stonemason in state 10,
    /// bricklayer in 11): up on it, at the height of its next unit, its material not
    /// yet come.
    pub(crate) fn mason_waiting(&self, f: &Figure) -> Option<usize> {
        if !matches!(f.kind, STONEMASON | BRICKLAYER) || f.dead || f.action != 2 || f.cargo != 0 {
            return None;
        }
        let Some(crate::pyramids::Job::Unit(i)) = decode_tomb_job(f.amount) else { return None };
        let p = f.perch?;
        (p.arrived() && !p.at_foot() && p.block as usize == i && p.height == self.block_height(f.target, i)).then_some(i)
    }

    /// The craftsman who has block `i` of tomb `id` as his unit.
    fn block_mason(&self, id: BuildingId, i: usize) -> Option<FigureId> {
        let m = self.buildings.get(id)?.monument.as_ref()?;
        m.craftsmen
            .iter()
            .map(|c| c.1)
            .find(|&c| self.figures.get(c).is_some_and(|f| !f.dead && decode_tomb_job(f.amount) == Some(crate::pyramids::Job::Unit(i))))
    }

    /// The laborer fetching mason `mason`'s sled, if they still name each other
    /// (`FUN_004f26a0`).
    fn sled_laborer(&self, mason: &Figure) -> Option<&Figure> {
        let l = self.figures.get(mason.link)?;
        (l.kind == crate::farms::PEASANT && !l.dead && is_hauling(l.action) && l.link == mason.id && l.target == mason.target).then_some(l)
    }

    /// A mason on tomb `id` a sled can go to (`FUN_004f29e0`): nearest the head of the
    /// way up, one waiting on his block whose sled partner, if it is also to be
    /// laid, waits too (the sled goes to the first of a pair), and whom no laborer is
    /// fetching for; with `steal`, also one whose laborer has not yet loaded his sled
    /// at the yard (who, finding his mason gone, is gone himself). `me` is the
    /// laborer asking.
    fn sled_mason(&self, id: BuildingId, steal: bool, me: FigureId) -> Option<FigureId> {
        let front = self.frontier_by_head(id);
        let waiting = |i: usize| self.block_mason(id, i).filter(|&c| self.figures.get(c).is_some_and(|f| self.mason_waiting(f) == Some(i)));
        for &i in &front {
            let partner = self.sled_partner(id, i).filter(|p| front.contains(&p.0));
            if partner.is_some_and(|p| p.1) {
                continue;
            }
            let Some(mason) = waiting(i) else { continue };
            if partner.is_some_and(|p| waiting(p.0).is_none()) {
                continue;
            }
            let laborer = self.figures.get(mason).and_then(|f| self.sled_laborer(f)).filter(|l| l.id != me);
            match laborer {
                None => return Some(mason),
                Some(l) if steal && matches!(l.action, HAULING | AT_YARD) => return Some(mason),
                _ => {}
            }
        }
        None
    }

    /// Laborer `fid` and mason `mason` take each other.
    fn link_sled(&mut self, fid: FigureId, mason: FigureId) {
        if let Some(m) = self.figures.get_mut(mason) {
            m.link = fid;
        }
        if let Some(f) = self.figures.get_mut(fid) {
            f.link = mason;
        }
    }

    /// Whether laborer `fid` and the mason his sled is for still name each other.
    fn sled_link_holds(&self, fid: FigureId) -> bool {
        let Some(f) = self.figures.get(fid) else { return false };
        self.figures.get(f.link).is_some_and(|m| m.link == fid && !m.dead && m.target == f.target)
    }

    /// Whether mason `mason` still has the block of `target` he waits on or lays: up
    /// on a pyramid or mastaba, a unit of a block (`FUN_004eea10`); at a mausoleum or
    /// sun temple, a block he goes to, waits at or lays (`FUN_004e0530`, `FUN_00542110`).
    fn sled_mason_busy(&self, target: BuildingId, mason: FigureId) -> bool {
        let Some(m) = self.figures.get(mason) else { return false };
        if m.dead || m.target != target || !matches!(m.kind, STONEMASON | BRICKLAYER) {
            return false;
        }
        if self.tomb_route(target).is_empty() {
            return m.amount > 0 && matches!(m.action, 1 | SITE_CROSS | AT_SPOT);
        }
        m.action == 2 && m.perch.is_some() && matches!(decode_tomb_job(m.amount), Some(crate::pyramids::Job::Unit(_)))
    }

    /// A sled laborer on the way from his yard, loaded, makes sure of his mason
    /// (`FUN_004a9730`): he keeps him while the mason still has his block, else takes
    /// another waiting for a sled. `Some(false)` while there is none but the tomb is
    /// still building, when he waits; `None` when he gives up, as he does at once if
    /// his mason is no longer up on the tomb.
    fn keep_sled_mason(&mut self, fid: FigureId) -> Option<bool> {
        let f = self.figures.get(fid)?;
        let (target, link) = (f.target, f.link);
        let up = self.figures.get(link).is_some_and(|m| !m.dead && m.target == target && m.perch.is_some());
        if !up {
            return None;
        }
        if self.sled_mason_busy(target, link) {
            return Some(true);
        }
        if let Some(mason) = self.sled_mason(target, true, fid) {
            self.link_sled(fid, mason);
            return Some(true);
        }
        self.tomb_building(target).then_some(false)
    }

    /// A sled laborer come to the foot of the way up (`FUN_004f2cf0`): if his mason no
    /// longer has his block he takes another waiting for a sled; then, if the mason
    /// of a block nearer the head of the way up waits for a sled of the same material
    /// that a laborer still on the ground (not yet at the yard, loading, or dragging
    /// his sled over) is fetching, the two laborers change masons, so the sleds that
    /// come first go to the blocks first in line.
    fn swap_sled_mason(&mut self, fid: FigureId) {
        let Some(f) = self.figures.get(fid) else { return };
        let (target, cargo) = (f.target, f.cargo);
        if !self.sled_mason_busy(target, f.link) {
            match self.sled_mason(target, true, fid) {
                Some(mason) => self.link_sled(fid, mason),
                None => return,
            }
        }
        let mine = self.figures.get(fid).map_or(0, |f| f.link);
        let Some(crate::pyramids::Job::Unit(block)) = self.figures.get(mine).and_then(|m| decode_tomb_job(m.amount)) else { return };
        let front = self.frontier_by_head(target);
        let Some(at) = front.iter().position(|&i| i == block) else { return };
        if at == 0 {
            return;
        }
        let blocks = self.buildings.get(target).and_then(|b| b.monument.as_ref()).map_or(0, |m| m.blocks.len());
        let mut best = at;
        let mut pick: Option<(FigureId, FigureId)> = None;
        for i in 0..blocks {
            if self.sled_partner(target, i).is_some_and(|p| p.1) {
                continue;
            }
            let Some(c) = self.block_mason(target, i) else { continue };
            let Some(cm) = self.figures.get(c) else { continue };
            if self.mason_waiting(cm) != Some(i) || cm.link == fid {
                continue;
            }
            let mut other = cm.link;
            if other != 0 {
                match self.figures.get(other) {
                    Some(l) if l.link == c => {
                        if !(l.kind == crate::farms::PEASANT && !l.dead && matches!(l.action, HAULING | AT_YARD | SLED_TO_TOMB)) {
                            continue;
                        }
                    }
                    _ => {
                        self.figures.get_mut(c).expect("present").link = 0;
                        other = 0;
                    }
                }
            }
            let k = front.iter().position(|&j| j == i).unwrap_or(999);
            if k < best && other != 0 && self.figures.get(other).is_some_and(|l| l.cargo == cargo) {
                best = k;
                pick = Some((c, other));
            }
        }
        if let Some((c, other)) = pick {
            self.link_sled(fid, c);
            self.link_sled(other, mine);
        }
    }

    /// A laborer whose mason has been taken by another laborer, or no longer wants the
    /// sled, before he has loaded it: the original removes him where he is.
    fn lose_hauler(&mut self, fid: FigureId) {
        let Some(f) = self.figures.get(fid) else { return };
        let (link, target, r, amount) = (f.link, f.target, f.cargo, f.amount);
        if let Some(m) = self.figures.get_mut(link).filter(|m| m.link == fid) {
            m.link = 0;
        }
        if amount > 0
            && let Some(m) = self.buildings.get_mut(target).and_then(|b| b.monument.as_mut())
        {
            Monument::add(&mut m.in_flight, r, -amount);
        }
        let f = self.figures.get_mut(fid).expect("present");
        f.amount = 0;
        f.dead = true;
    }

    /// A laborer with a sled (the original's laborer states 15-20, 23, 26, 29 and 30):
    /// he goes to the storage yard and loads it, and drags it to the monument with
    /// five pullers, the sled behind them. At a pyramid or mastaba he drags it to the
    /// foot of the way up, climbs it until he is as high as the building has got,
    /// crosses to his mason's block and gives him the load, which starts the mason and
    /// the mason on the partner block; at a mausoleum or sun temple he drags it across
    /// the site to the tile his mason lays from. He stands a while and goes home.
    pub(crate) fn update_hauler(&mut self, fid: FigureId) {
        let Some(f) = self.figures.get(fid) else { return };
        let (act, target) = (f.action, f.target);
        let tomb = !self.tomb_route(target).is_empty();
        match act {
            HAULING => self.haul_to_yard(fid),
            AT_YARD => self.load_sled(fid),
            SLED_TO_TOMB if tomb => self.drag_to_tomb(fid),
            SLED_TO_TOMB => self.drag_to_site(fid),
            SLED_CLIMB => self.sled_climb(fid),
            SLED_CROSS => {
                let f = self.figures.get_mut(fid).expect("present");
                f.anim_tick += 1;
                let Some(mut p) = f.perch else { return self.drop_sled(fid) };
                if !p.arrived() {
                    p.step += 1;
                    f.perch = Some(p);
                    f.moving = true;
                    return;
                }
                f.moving = false;
                self.deliver_sled(fid);
            }
            SLED_DONE => {
                // He stands 43 ticks, the train closing up behind him.
                let f = self.figures.get_mut(fid).expect("present");
                f.anim_tick += 1;
                f.counter += 1;
                if f.counter > 42 {
                    self.drop_sled(fid);
                }
            }
            _ => {}
        }
    }

    /// On his way to the storage yard (the original's state 15). Should his mason no
    /// longer name him he is gone.
    fn haul_to_yard(&mut self, fid: FigureId) {
        if !self.sled_link_holds(fid) {
            self.lose_hauler(fid);
            return;
        }
        let map = &self.map;
        let f = self.figures.get_mut(fid).expect("present");
        if f.yard == 0 {
            // (Saved before laborers kept their yard apart.)
            f.yard = f.counter.max(0) as u32;
            f.counter = 0;
        }
        match f.walk(map) {
            Step::Moving => {}
            Step::Arrived => {
                f.action = AT_YARD;
                f.counter = 0;
                f.moving = false;
            }
            _ => self.lose_hauler(fid),
        }
    }

    /// At the storage yard (the original's state 16): if his mason no longer names him
    /// or no longer has his block, he is gone. After four ticks he takes a full sled,
    /// if the yard still holds one, and sets off with it, five pullers and the sled
    /// falling in behind him. With nothing to load, or no way to the monument, he
    /// goes home.
    fn load_sled(&mut self, fid: FigureId) {
        let Some(f) = self.figures.get(fid) else { return };
        let (target, link, yard, r, amount) = (f.target, f.link, f.yard, f.cargo, f.amount);
        if !self.sled_link_holds(fid) || !self.sled_mason_busy(target, link) {
            self.lose_hauler(fid);
            return;
        }
        let f = self.figures.get_mut(fid).expect("present");
        f.counter += 1;
        f.moving = false;
        if f.counter <= 4 {
            return;
        }
        let live = self.buildings.get(target).and_then(|b| b.monument.as_ref()).is_some_and(|m| !m.finished);
        let spot = self.sled_spot(target, link);
        let full = amount > 0 && self.stored(yard, r) >= amount;
        let (Some(spot), true, true) = (spot, live, full) else {
            self.drop_sled(fid);
            return;
        };
        let map = &self.map;
        let f = self.figures.get_mut(fid).expect("present");
        let (x, y) = (f.x, f.y);
        if (x, y) != spot && !f.go_to(map, spot) {
            self.drop_sled(fid);
            return;
        }
        self.take_stored(yard, r, amount);
        let tomb = !self.tomb_route(target).is_empty();
        let f = self.figures.get_mut(fid).expect("present");
        f.action = SLED_TO_TOMB;
        // To a tomb he first makes sure of his mason (state 30); to a mausoleum or sun
        // temple he sets off at once.
        f.counter = if tomb { 0 } else { 1 };
        // Five pullers, each following the man ahead, and the sled last (0x4aa529).
        let mut lead = fid;
        for n in 1..=SLED_PULLERS + 1 {
            let kind = if n > SLED_PULLERS { SLED } else { SLED_PULLER };
            let p = self.figures.spawn(kind, x, y, Travel::Land);
            if let Some(f) = self.figures.get_mut(p) {
                f.link = lead;
                f.target = target;
                f.slot = n as u8;
                f.cargo = r;
                f.amount = amount;
            }
            lead = p;
        }
        if tomb && (x, y) == spot {
            self.arrive_at_foot(fid);
        }
    }

    /// Dragging a loaded sled to a tomb: waiting at the yard (state 30) until his
    /// mason, or another, waits for it and a sled for the same tomb less than seven
    /// tiles ahead has got clear (`FUN_004f2ba0`), then over the ground (state 17).
    fn drag_to_tomb(&mut self, fid: FigureId) {
        let Some(f) = self.figures.get(fid) else { return };
        let target = f.target;
        if f.counter == 0 {
            match self.keep_sled_mason(fid) {
                Some(true) => {}
                Some(false) => {
                    self.figures.get_mut(fid).expect("present").anim_tick += 1;
                    return;
                }
                None => {
                    self.drop_sled(fid);
                    return;
                }
            }
            let mine = self.figures.get(fid).map_or(0, |f| f.route.len() as i32);
            let crowded = self.figures.iter().any(|o| {
                o.id != fid && o.kind == crate::farms::PEASANT && o.action == SLED_TO_TOMB && o.counter == 1 && o.target == target && (o.route.len() as i32 - mine).abs() < 7
            });
            let f = self.figures.get_mut(fid).expect("present");
            f.anim_tick += 1;
            if crowded {
                return;
            }
            f.counter = 1;
        }
        let map = &self.map;
        let f = self.figures.get_mut(fid).expect("present");
        match f.walk(map) {
            Step::Moving => {}
            Step::Arrived => self.arrive_at_foot(fid),
            _ => self.drop_sled(fid),
        }
    }

    /// At the foot of the way up: he makes sure of his mason, perhaps changing with
    /// another laborer (`FUN_004f2cf0`), and steps up (`FUN_004f1cb0`).
    fn arrive_at_foot(&mut self, fid: FigureId) {
        self.swap_sled_mason(fid);
        let Some(target) = self.figures.get(fid).map(|f| f.target) else { return };
        let first = self.tomb_route(target).first().map_or(0, |r| r.0 as u16);
        let f = self.figures.get_mut(fid).expect("present");
        f.perch = Some(crate::pyramids::Perch::foot(first));
        f.action = SLED_CLIMB;
        f.moving = false;
    }

    /// A sled laborer on the way up (the original's state 18, `FUN_004f04d0`): should
    /// his mason no longer have his block he takes another waiting for a sled, or
    /// with none stands where he is while the tomb is still building. He takes the
    /// way up entry by entry, waiting where a block still wants its ramp, until he is
    /// as high as the building has got, then crosses to his mason's block.
    fn sled_climb(&mut self, fid: FigureId) {
        let Some(f) = self.figures.get(fid) else { return };
        let (target, link) = (f.target, f.link);
        if !self.sled_mason_busy(target, link) {
            match self.sled_mason(target, true, fid) {
                Some(mason) => self.link_sled(fid, mason),
                None if self.tomb_building(target) => {
                    self.figures.get_mut(fid).expect("present").moving = false;
                    return;
                }
                None => {
                    self.drop_sled(fid);
                    return;
                }
            }
        }
        let f = self.figures.get_mut(fid).expect("present");
        f.anim_tick += 1;
        let Some(mut p) = f.perch else { return self.drop_sled(fid) };
        if !p.arrived() {
            p.step += 1;
            f.perch = Some(p);
            f.moving = true;
            return;
        }
        f.moving = false;
        let Some(&low) = self.frontier(target).first() else {
            self.drop_sled(fid);
            return;
        };
        let head = self.block_height(target, low);
        let route = self.tomb_route(target);
        let next = if p.at_foot() { 0 } else { p.route as usize };
        if p.height < head && next < route.len() {
            let (b, h) = route[next];
            if !self.block_waits_for_ramp(target, b as usize) {
                self.perch_move(fid, b as usize, h, next as u8 + 1, true);
            }
            return;
        }
        // Over to his mason, where the mason stands.
        let link = self.figures.get(fid).map_or(0, |f| f.link);
        let Some(mp) = self.figures.get(link).filter(|m| m.target == target).and_then(|m| m.perch) else {
            self.drop_sled(fid);
            return;
        };
        self.perch_move(fid, mp.block as usize, mp.height, 0, false);
        self.figures.get_mut(fid).expect("present").action = SLED_CROSS;
    }

    /// The sled reaches the mason's block (the original's state 20): if he is still
    /// waiting for it he and the mason on the partner block start their units
    /// (`FUN_004f2960`), and the load is spent.
    fn deliver_sled(&mut self, fid: FigureId) {
        let Some(f) = self.figures.get(fid) else { return };
        let (target, link, r, amount) = (f.target, f.link, f.cargo, f.amount);
        let block = self.figures.get(link).and_then(|m| self.mason_waiting(m));
        if let Some(i) = block {
            let partner = self.sled_partner(target, i).map(|p| p.0).filter(|p| self.frontier(target).contains(p));
            let partner = partner.and_then(|p| self.block_mason(target, p)).filter(|&c| self.figures.get(c).is_some_and(|m| self.mason_waiting(m).is_some()));
            for c in std::iter::once(link).chain(partner) {
                if let Some(m) = self.figures.get_mut(c) {
                    m.cargo = 1;
                    m.counter = 0;
                }
            }
        }
        if let Some(m) = self.buildings.get_mut(target).and_then(|b| b.monument.as_mut()) {
            Monument::add(&mut m.in_flight, r, -amount);
        }
        let f = self.figures.get_mut(fid).expect("present");
        f.amount = 0;
        f.action = SLED_DONE;
        f.counter = 0;
    }

    /// Dragging a sled to a mausoleum or sun temple (the original's states 26 and 23):
    /// over the ground to the edge of the site, then straight across it to the tile
    /// his mason lays his block from.
    fn drag_to_site(&mut self, fid: FigureId) {
        let map = &self.map;
        let Some(f) = self.figures.get_mut(fid) else { return };
        let (stage, link) = (f.counter, f.link);
        match f.walk(map) {
            Step::Moving => {}
            Step::Arrived if stage == 1 => {
                let Some(spot) = self.mason_spot(link) else {
                    self.drop_sled(fid);
                    return;
                };
                let map = &self.map;
                let f = self.figures.get_mut(fid).expect("present");
                f.counter = 2;
                f.travel = Travel::Any;
                if (f.x, f.y) == spot {
                    self.deliver_to_site(fid);
                } else if !f.go_to(map, spot) {
                    self.drop_sled(fid);
                }
            }
            Step::Arrived => self.deliver_to_site(fid),
            _ => self.drop_sled(fid),
        }
    }

    /// The sled reaches the mason at a mausoleum or sun temple: if he waits for it the
    /// load starts him on his block (his state 10 to 11, 0x4ab537); the load is spent
    /// either way, and the laborer stands a while (state 29).
    fn deliver_to_site(&mut self, fid: FigureId) {
        let Some(f) = self.figures.get(fid) else { return };
        let (target, link, r, amount) = (f.target, f.link, f.cargo, f.amount);
        let waiting = self.figures.get(link).is_some_and(|m| m.kind == STONEMASON && !m.dead && m.target == target && m.action == AT_SPOT && m.cargo == 0);
        if waiting {
            let m = self.figures.get_mut(link).expect("present");
            m.cargo = 1;
            m.counter = 0;
        }
        if let Some(m) = self.buildings.get_mut(target).and_then(|b| b.monument.as_mut()) {
            Monument::add(&mut m.in_flight, r, -amount);
            if waiting {
                Monument::add(&mut m.delivered, r, amount);
            }
        }
        let f = self.figures.get_mut(fid).expect("present");
        f.amount = 0;
        f.action = SLED_DONE;
        f.counter = 0;
        f.moving = false;
    }

    /// A sled laborer is done or gives up: what he still drags is lost with the
    /// sled, his mason forgets him, and he goes home.
    fn drop_sled(&mut self, fid: FigureId) {
        let Some(f) = self.figures.get(fid) else { return };
        let (link, target, r, amount) = (f.link, f.target, f.cargo, f.amount);
        if let Some(m) = self.figures.get_mut(link).filter(|m| m.link == fid) {
            m.link = 0;
        }
        if amount > 0
            && let Some(m) = self.buildings.get_mut(target).and_then(|b| b.monument.as_mut())
        {
            Monument::add(&mut m.in_flight, r, -amount);
        }
        let f = self.figures.get_mut(fid).expect("present");
        f.amount = 0;
        f.cargo = 0;
        f.counter = 0;
        f.moving = false;
        self.send_laborer_home(fid);
    }

    /// A sled puller or the sled itself follows the man ahead of it the original's
    /// way (`FUN_004b23c0`, the puller at 0x4af2c0 and the sled at 0x4ab980), and goes
    /// when the laborer at the head of the train is done. It keeps `gap` ticks behind
    /// him on his very path: on the ground its step along a tile is his less the gap
    /// (of the fifteen a tile takes), it takes his tile when that step reaches the
    /// middle of the tile and his heading when it starts a tile; up on a tomb it
    /// makes the same moves, and when he steps up from the foot it stands while its
    /// step counts on up to nine and then joins him. The gap is seven ticks for a
    /// puller and twelve for the sled, and once the load is given over it closes up.
    pub(crate) fn update_sled_follower(&mut self, fid: FigureId) {
        let Some(f) = self.figures.get(fid) else { return };
        let (lead, slot, is_sled) = (f.link, f.slot as i32, f.kind == SLED);
        // The laborer at the head of the train.
        let mut head = lead;
        for _ in 0..=SLED_PULLERS {
            match self.figures.get(head).filter(|h| h.kind == SLED_PULLER || h.kind == SLED) {
                Some(h) => head = h.link,
                None => break,
            }
        }
        let alive = |id: FigureId| self.figures.get(id).is_some_and(|x| !x.dead);
        let head_on = self.figures.get(head).is_some_and(|h| !h.dead && h.kind == crate::farms::PEASANT && matches!(h.action, SLED_TO_TOMB | SLED_CLIMB | SLED_CROSS | SLED_DONE));
        if !head_on || !alive(lead) {
            self.figures.get_mut(fid).expect("present").dead = true;
            return;
        }
        let done = self.figures.get(head).filter(|h| h.action == SLED_DONE).map(|h| h.counter);
        let gap = match (is_sled, done) {
            (false, Some(w)) => (7 * slot - w).clamp(1, PULLER_GAP),
            (true, Some(w)) => (7 * slot - w + 5).clamp(1, SLED_GAP),
            (false, None) => PULLER_GAP,
            (true, None) => SLED_GAP,
        };
        let l = self.figures.get(lead).expect("alive").clone();
        let f = self.figures.get_mut(fid).expect("present");
        match (f.perch, l.perch) {
            (Some(mut p), Some(lp)) => {
                let gap = gap as u16;
                if p.block == lp.block {
                    if p.step + gap < lp.step {
                        p.step += 1;
                    }
                    f.moving = p.step < p.steps;
                    f.perch = Some(p);
                } else if lp.from != Some(p.block) || {
                    p.step += 1;
                    f.perch = Some(p);
                    p.step >= p.steps
                } {
                    f.perch = Some(crate::pyramids::Perch { step: lp.step.saturating_sub(gap), ..lp });
                    f.direction = l.direction;
                    f.moving = true;
                }
                f.anim_tick = l.anim_tick;
            }
            (None, Some(lp)) => {
                // The count starts from its step along the tile; it stands where it is
                // meanwhile.
                if f.counter < 0 {
                    f.counter = f.progress as i32;
                }
                f.counter += 1;
                if f.counter >= 9 {
                    f.counter = 0;
                    f.perch = Some(crate::pyramids::Perch { step: lp.step.saturating_sub(gap as u16), ..lp });
                    f.direction = l.direction;
                    f.x = l.x;
                    f.y = l.y;
                }
            }
            (_, None) => {
                f.perch = None;
                f.counter = -1;
                f.progress = (l.progress as i32 - gap).rem_euclid(15) as u8;
                if f.progress == 8 {
                    f.x = l.x;
                    f.y = l.y;
                }
                if f.progress == 0 {
                    f.direction = l.direction;
                }
                // Drawn part way along its step even while the train stands.
                f.moving = true;
                f.anim_tick = l.anim_tick;
            }
        }
    }

    /// Daily: monuments whose phase is complete move on to the next.
    pub(crate) fn update_monuments(&mut self) {
        for id in self.active_monuments() {
            // Craftsmen who never arrived are forgotten.
            let alive: Vec<(u16, FigureId)> = self.buildings.get(id).and_then(|b| b.monument.as_ref()).map_or_else(Vec::new, |m| m.craftsmen.clone());
            let alive: Vec<(u16, FigureId)> = alive.into_iter().filter(|&(_, c)| self.figures.get(c).is_some_and(|f| !f.dead && f.target == id)).collect();
            // What is on its way is what the live sleds and laborers bring.
            let mut coming: Vec<(u16, i32)> = Vec::new();
            for f in self.figures.iter().filter(|f| !f.dead && f.target == id && f.kind == crate::farms::PEASANT && is_hauling(f.action)) {
                Monument::add(&mut coming, f.cargo, f.amount);
            }
            if let Some(m) = self.buildings.get_mut(id).and_then(|b| b.monument.as_mut()) {
                m.craftsmen = alive;
                m.in_flight = coming;
            }
            let Some(b) = self.buildings.get(id) else { continue };
            let Some(def) = monument_def(b.kind) else { continue };
            let (x, y) = (b.x, b.y);
            if def.style == Style::RoyalTomb {
                // Announced and sealed on its own terms.
                self.update_royal_tomb(id);
                continue;
            }
            let finished = if crate::pyramids::blockwise(def.style) {
                self.advance_tomb(id)
            } else {
                // (A game saved when obelisks and the sphinx were built otherwise may
                // hold a different count of pieces for the phase.)
                let units = def.units(b.monument.as_ref().expect("active").phase);
                let m = self.buildings.get_mut(id).and_then(|b| b.monument.as_mut()).expect("active");
                if m.progress.len() != units {
                    m.progress.resize(units, 0);
                }
                let m = self.buildings.get(id).and_then(|b| b.monument.as_ref()).expect("active");
                let all_done = m.progress.iter().enumerate().all(|(i, &p)| p >= def.work_of(m.phase, i));
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
                    Style::RoyalTomb => crate::royal_tombs::phrase(def.kind),
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
        if def.style == Style::RoyalTomb {
            // Whoever the open chambers wait for.
            return [STONEMASON, crate::royal_tombs::TOMB_ARTISAN].into_iter().filter(|&k| self.royal_tomb_waiting(id, k)).collect();
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
        if def.style == Style::RoyalTomb {
            return self.royal_tomb_percent(id);
        }
        let needed: i32 = (0..m.progress.len()).map(|i| def.work_of(m.phase, i) as i32).sum();
        let within = m.progress.iter().map(|&p| p as i32).sum::<i32>() * 100 / needed.max(1);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::{Command, Outcome};

    /// Mission 12's land by the river with a small true pyramid staked out (its site
    /// already prepared), storage yards holding its stone and limestone, and two
    /// stonemasons' guilds and a carpenters' guild with timber for the ramps, all fully
    /// staffed; work camps only if `camps`.
    fn pyramid_town(camps: bool) -> Option<(World, BuildingId)> {
        let data = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../PharaohData");
        if !data.join("mission1.pak").is_file() {
            return None;
        }
        let library = osiris_formats::ImageLibrary::open(&data.join("Data")).expect("open image library");
        let scenario = osiris_formats::MissionPak::open(&data.join("mission1.pak")).expect("pak").scenario(12).expect("mission 12");
        let defs = std::sync::Arc::new(crate::defs::Defs::load(&library).expect("load defs"));
        let model_text = std::fs::read(data.join("Pharaoh_Model_Normal.txt")).expect("read model");
        let model = osiris_formats::Model::parse(&String::from_utf8_lossy(&model_text)).expect("parse model");
        let balance = std::sync::Arc::new(crate::balance::Balance::from_model(&model));
        let mut world = World::new(&scenario, defs, balance);
        world.start(&scenario);
        world.load_mission(12);
        world.invasions.planned.clear();
        world.rules.fire = false;
        world.rules.collapse = false;
        world.rules.global_labor_pool = true;
        world.test_full_staff = true;
        world.treasury = 100_000;
        world.scenario_monuments = [13, 0, 0];
        if let Some(m) = world.mission.as_mut() {
            m.allowed.insert(SMALL_PYRAMID);
        }
        let build = |kind: u16, x: i32, y: i32| Command::Build { kind, x, y, x1: x, y1: y };
        let mut steps = vec![
            build(SMALL_PYRAMID, 58, 40),
            Command::Road { start: (36, 52), end: (55, 52) },
            Command::Road { start: (55, 52), end: (74, 52) },
            Command::Road { start: (36, 56), end: (55, 56) },
            Command::Road { start: (36, 52), end: (36, 56) },
            build(kind::STONEMASONS_GUILD, 37, 57),
            build(kind::STONEMASONS_GUILD, 39, 57),
            build(kind::CARPENTERS_GUILD, 41, 57),
        ];
        steps.extend((0..8).map(|i| build(kind::STORAGE_YARD, 37 + 3 * i, 53)));
        if camps {
            steps.extend([build(kind::WORK_CAMP, 43, 57), build(kind::WORK_CAMP, 46, 57)]);
        }
        for cmd in &steps {
            assert!(matches!(world.apply(cmd), Outcome::Done { .. }), "{cmd:?}");
        }
        let yards: Vec<BuildingId> = world.buildings.iter().filter(|b| b.kind == kind::STORAGE_YARD).map(|b| b.id).collect();
        for (i, y) in yards.into_iter().enumerate() {
            world.add_stored(y, if i < 2 { STONE } else { LIMESTONE }, 3200);
        }
        // Timber for the six ramps.
        if let Some(g) = world.buildings.iter_mut().find(|b| b.kind == kind::CARPENTERS_GUILD) {
            g.stock[TIMBER as usize] = 600;
        }
        let id = world.buildings.iter().find(|b| b.kind == SMALL_PYRAMID).map(|b| b.id).expect("pyramid");
        world.set_tomb_stage(id, crate::pyramids::RAISE);
        Some((world, id))
    }

    #[test]
    fn laborers_drag_the_sleds_and_nothing_is_left_over() {
        let Some((mut world, id)) = pyramid_town(false) else { return };
        for _ in 0..3000 {
            world.tick();
        }
        // No work camp, no sleds: the masons wait.
        let m = world.buildings.get(id).and_then(|b| b.monument.as_ref()).expect("tomb");
        assert!(m.delivered.is_empty() && m.in_flight.is_empty());
        assert!(world.figures.iter().all(|f| f.kind != SLED));

        let Some((mut world, id)) = pyramid_town(true) else { return };
        let mut most_masons = 0;
        // The tiles each sled laborer has stood on since he loaded his sled.
        let mut trails: std::collections::HashMap<FigureId, std::collections::HashSet<(i32, i32)>> = std::collections::HashMap::new();
        let mut followed = 0;
        for _ in 0..60_000 {
            world.tick();
            let masons = world.figures.iter().filter(|f| f.kind == STONEMASON && !f.dead).count();
            most_masons = most_masons.max(masons);
            for f in world.figures.iter().filter(|f| f.kind == crate::farms::PEASANT && matches!(f.action, AT_YARD | SLED_TO_TOMB)) {
                trails.entry(f.id).or_default().insert((f.x, f.y));
            }
            // Pullers and sleds on the ground keep to their laborer's very path.
            for f in world.figures.iter().filter(|f| matches!(f.kind, SLED | SLED_PULLER) && !f.dead && f.perch.is_none()) {
                let mut head = f.link;
                while let Some(h) = world.figures.get(head).filter(|h| matches!(h.kind, SLED | SLED_PULLER)) {
                    head = h.link;
                }
                if world.figures.get(head).is_some_and(|h| h.action == SLED_TO_TOMB) {
                    assert!(trails.get(&head).is_some_and(|t| t.contains(&(f.x, f.y))), "{} off its laborer's path at {},{}", f.id, f.x, f.y);
                    followed += 1;
                }
            }
            if world.buildings.get(id).and_then(|b| b.monument.as_ref()).is_some_and(|m| m.finished) {
                break;
            }
        }
        assert!(followed > 0);
        let m = world.buildings.get(id).and_then(|b| b.monument.as_ref()).expect("tomb");
        assert!(m.finished);
        // Four masons from each fully staffed guild.
        assert_eq!(most_masons, 8);
        // Exactly the 4800 stone and 19200 limestone went into it.
        assert!(m.delivered.is_empty(), "left on site: {:?}", m.delivered);
        let left = |r: u16| world.buildings.iter().filter(|b| b.kind == kind::STORAGE_YARD).map(|b| world.stored(b.id, r)).sum::<i32>();
        assert_eq!((left(STONE), left(LIMESTONE)), (6400 - 4800, 19200 - 19200));
        // And the carpenters' guild gave 100 timber for each of the six ramps.
        assert!(world.buildings.iter().filter(|b| b.kind == kind::CARPENTERS_GUILD).all(|b| b.stock[TIMBER as usize] == 0));
    }

    #[test]
    fn obelisk_steps_follow_the_original_progress() {
        // Ten steps, all worked.
        let small: Vec<usize> = (0..).map_while(|k| obelisk_step(10, k)).collect();
        assert_eq!(small, (0..10).collect::<Vec<_>>());
        // Eighteen steps at 4 progress a step: the twelfth is skipped.
        let large: Vec<usize> = (0..).map_while(|k| obelisk_step(18, k)).collect();
        assert_eq!(large, (0..18).filter(|&i| i != 11).collect::<Vec<_>>());
        assert_eq!(monument_def(SMALL_OBELISK).map(|d| d.phase_count), Some(LEVELING_PHASES + 11));
        assert_eq!(monument_def(LARGE_OBELISK).map(|d| d.phase_count), Some(LEVELING_PHASES + 18));
        // The first four jobs are carpenters' and take no sleds.
        let def = monument_def(SMALL_OBELISK).expect("obelisk");
        for p in LEVELING_PHASES..def.phase_count - 1 {
            let carpenter = p < LEVELING_PHASES + 4;
            assert_eq!(def.crew(p), vec![if carpenter { CARPENTER } else { STONEMASON }]);
            assert!(def.phase(p).is_empty());
        }
        // The image only changes in the last part of the work.
        let images: Vec<u8> = (0..=10).map(|k| obelisk_image(10, 4, k)).collect();
        assert_eq!(images, [0, 0, 0, 0, 0, 0, 1, 1, 2, 3, 3]);
    }

    #[test]
    fn sphinx_parts_carve_in_turn() {
        let stages: Vec<[u8; 3]> = (1..=16).map(|s| [0, 1, 2].map(|p| sphinx_stage(s, p))).collect();
        assert_eq!(stages[0], [1, 1, 1]);
        assert_eq!(stages[1], [2, 1, 1]);
        assert_eq!(stages[3], [2, 2, 2]);
        assert_eq!(stages[13], [6, 5, 5]);
        assert_eq!(stages[15], [6, 6, 6]);
        let def = monument_def(SPHINX).expect("sphinx");
        // Step 4: three masons side by side; step 7: two masons and a carpenter.
        assert_eq!(def.jobs(LEVELING_PHASES + 3).len(), 3);
        assert_eq!(def.crew(LEVELING_PHASES + 6), vec![STONEMASON, CARPENTER]);
        assert!(def.jobs(def.phase_count - 1).is_empty());
    }

    #[test]
    fn sleds_start_the_masons_at_a_mausoleum_and_a_sun_temple() {
        let data = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../PharaohData");
        if !data.join("mission1.pak").is_file() {
            return;
        }
        for (kind, title, at, phase) in [(MAUSOLEUM, 25, (44, 42), 1), (SUN_TEMPLE, 24, (58, 30), SUN_WALLS)] {
            let library = osiris_formats::ImageLibrary::open(&data.join("Data")).expect("open image library");
            let scenario = osiris_formats::MissionPak::open(&data.join("mission1.pak")).expect("pak").scenario(12).expect("mission 12");
            let defs = std::sync::Arc::new(crate::defs::Defs::load(&library).expect("load defs"));
            let model_text = std::fs::read(data.join("Pharaoh_Model_Normal.txt")).expect("read model");
            let model = osiris_formats::Model::parse(&String::from_utf8_lossy(&model_text)).expect("parse model");
            let balance = std::sync::Arc::new(crate::balance::Balance::from_model(&model));
            let mut world = World::new(&scenario, defs, balance);
            world.start(&scenario);
            world.load_mission(12);
            world.invasions.planned.clear();
            world.rules.fire = false;
            world.rules.collapse = false;
            world.rules.global_labor_pool = true;
            world.test_full_staff = true;
            world.treasury = 100_000;
            world.scenario_monuments = [title, 0, 0];
            if let Some(m) = world.mission.as_mut() {
                m.allowed.insert(kind);
            }
            let build = |kind: u16, x: i32, y: i32| Command::Build { kind, x, y, x1: x, y1: y };
            let mut steps = vec![
                Command::Road { start: (36, 52), end: (55, 52) },
                Command::Road { start: (55, 52), end: (74, 52) },
                Command::Road { start: (36, 56), end: (55, 56) },
                Command::Road { start: (36, 52), end: (36, 56) },
                build(kind::STONEMASONS_GUILD, 37, 57),
                build(kind::STONEMASONS_GUILD, 39, 57),
                build(kind::WORK_CAMP, 43, 57),
                build(kind::WORK_CAMP, 46, 57),
            ];
            steps.extend((0..8).map(|i| build(kind::STORAGE_YARD, 37 + 3 * i, 53)));
            for cmd in &steps {
                assert!(matches!(world.apply(cmd), Outcome::Done { .. }), "{cmd:?}");
            }
            let yards: Vec<BuildingId> = world.buildings.iter().filter(|b| b.kind == kind::STORAGE_YARD).map(|b| b.id).collect();
            let fill = |world: &mut World| {
                for &y in &yards {
                    let have = world.stored(y, SANDSTONE);
                    world.add_stored(y, SANDSTONE, 3200 - have);
                }
            };
            fill(&mut world);
            assert!(matches!(world.apply(&build(kind, at.0, at.1)), Outcome::Done { .. }));
            fill(&mut world);
            let stock = |world: &World| yards.iter().map(|&y| world.stored(y, SANDSTONE)).sum::<i32>();
            let before = stock(&world);
            let id = world.buildings.iter().find(|b| b.kind == kind).map(|b| b.id).expect("monument");
            let def = monument_def(kind).expect("def");
            let units = def.units(phase);
            if let Some(m) = world.buildings.get_mut(id).and_then(|b| b.monument.as_mut()) {
                m.phase = phase;
                m.progress = vec![0; units];
            }
            let mut sleds = 0;
            for _ in 0..40_000 {
                world.tick();
                sleds = sleds.max(world.figures.iter().filter(|f| f.kind == SLED && !f.dead).count());
                // A mason works his block only once its sled has come.
                for f in world.figures.iter().filter(|f| f.kind == STONEMASON && f.action == AT_SPOT && f.moving) {
                    assert_eq!(f.cargo, 1);
                }
                if world.buildings.get(id).and_then(|b| b.monument.as_ref()).is_some_and(|m| m.phase != phase) {
                    break;
                }
            }
            let m = world.buildings.get(id).and_then(|b| b.monument.as_ref()).expect("monument");
            assert_ne!(m.phase, phase, "{kind}");
            assert!(sleds > 0);
            // A sled of sandstone for every block, no more.
            assert_eq!(before - stock(&world), units as i32 * SLED_LOAD, "{kind}");
        }
    }

    #[test]
    fn obelisk_is_built_by_guild_craftsmen_without_sleds() {
        let data = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../PharaohData");
        if !data.join("mission1.pak").is_file() {
            return;
        }
        let library = osiris_formats::ImageLibrary::open(&data.join("Data")).expect("open image library");
        let scenario = osiris_formats::MissionPak::open(&data.join("mission1.pak")).expect("pak").scenario(12).expect("mission 12");
        let defs = std::sync::Arc::new(crate::defs::Defs::load(&library).expect("load defs"));
        let model_text = std::fs::read(data.join("Pharaoh_Model_Normal.txt")).expect("read model");
        let model = osiris_formats::Model::parse(&String::from_utf8_lossy(&model_text)).expect("parse model");
        let balance = std::sync::Arc::new(crate::balance::Balance::from_model(&model));
        let mut world = World::new(&scenario, defs, balance);
        world.start(&scenario);
        world.load_mission(12);
        world.invasions.planned.clear();
        world.rules.fire = false;
        world.rules.collapse = false;
        world.rules.global_labor_pool = true;
        world.test_full_staff = true;
        world.treasury = 100_000;
        world.scenario_monuments = [22, 0, 0];
        if let Some(m) = world.mission.as_mut() {
            m.allowed.insert(SMALL_OBELISK);
        }
        let build = |kind: u16, x: i32, y: i32| Command::Build { kind, x, y, x1: x, y1: y };
        let mut steps = vec![
            Command::Road { start: (36, 52), end: (55, 52) },
            Command::Road { start: (55, 52), end: (74, 52) },
            Command::Road { start: (36, 56), end: (55, 56) },
            Command::Road { start: (36, 52), end: (36, 56) },
            build(kind::STONEMASONS_GUILD, 37, 57),
            build(kind::CARPENTERS_GUILD, 41, 57),
            build(kind::WORK_CAMP, 43, 57),
        ];
        steps.extend((0..4).map(|i| build(kind::STORAGE_YARD, 37 + 3 * i, 53)));
        for cmd in &steps {
            assert!(matches!(world.apply(cmd), Outcome::Done { .. }), "{cmd:?}");
        }
        let yards: Vec<BuildingId> = world.buildings.iter().filter(|b| b.kind == kind::STORAGE_YARD).map(|b| b.id).collect();
        for y in yards {
            world.add_stored(y, GRANITE, 3200);
        }
        let out = world.apply(&build(SMALL_OBELISK, 60, 48));
        assert!(matches!(out, Outcome::Done { .. }), "{out:?}");
        let id = world.buildings.iter().find(|b| b.kind == SMALL_OBELISK).map(|b| b.id).expect("obelisk");
        let guild = world.buildings.iter().find(|b| b.kind == kind::CARPENTERS_GUILD).map(|b| b.id).expect("guild");
        world.buildings.get_mut(guild).expect("guild").stock[TIMBER as usize] = 600;
        let mut ticks = 0;
        let mut most_carpenters = 0;
        while ticks < 60_000 && !world.buildings.get(id).and_then(|b| b.monument.as_ref()).is_some_and(|m| m.finished) {
            world.tick();
            ticks += 1;
            assert!(world.figures.iter().all(|f| f.kind != SLED), "no sleds for an obelisk");
            most_carpenters = most_carpenters.max(world.figures.iter().filter(|f| f.kind == CARPENTER && !f.dead).count());
        }
        assert!(world.buildings.get(id).and_then(|b| b.monument.as_ref()).is_some_and(|m| m.finished), "unfinished after {ticks} ticks");
        // One carpenter a job, each after the guild's 400 points of progress.
        assert_eq!(most_carpenters, 1);
        assert_eq!(world.buildings.get(guild).map(|g| g.stock[TIMBER as usize]), Some(200));
        // At least the jobs' own ticks, and the guild's four build-ups of 50 days.
        let work: i32 = SMALL_OBELISK_STEPS.iter().map(|j| j.1 as i32).sum();
        assert!(ticks > work + 4 * 50 * 51, "{ticks}");
    }
}
