//! Monuments. A monument is laid out on a grid of 2x2 blocks and built in phases.
//! The first two phases level the site: laborers from work camps each work a block
//! until it is done. Every later phase adds a course of material: storage yards drag
//! it over on sleds (at most 400 units a sled, never more than the course still
//! needs), and a guild's craftsman, waiting on site, lays it block by block, as far
//! as the delivered material allows. When every block of a phase is done, the next
//! phase begins; after the last, the monument is complete.
//!
//! Mastabas are brick: a bricklayer lays six courses of bricks (with clay from the
//! second course on).

use crate::buildings::{BuildingId, kind};
use crate::figures::{FigureId, Step, Travel};
use crate::world::World;

/// Progress a block needs in each phase: one worker adds one point a tick.
pub const BLOCK_WORK: u16 = 200;
/// Most a sled carries.
pub const SLED_LOAD: i32 = 400;
/// Men pulling each sled.
const SLED_PULLERS: usize = 6;

pub const BRICKLAYER: u16 = 80;
pub const SLED: u16 = 86;
pub const SLED_PULLER: u16 = 96;

const BRICKS: u16 = 12;
const CLAY: u16 = 11;

/// What a 2x2 block of a mastaba is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Block {
    Main,
    Side,
    Wall,
    Entrance,
}

/// One monument type: its block grid when facing north, and its phases.
pub struct MonumentDef {
    pub kind: u16,
    /// Blocks across and down.
    pub cols: i32,
    pub rows: i32,
    /// Row holding the entrance (in the last column).
    entrance_row: i32,
    /// Rows of side blocks at the end.
    side_rows: i32,
    /// Whether the first row's last block is a side block.
    first_row_side: bool,
    /// Material per phase: (resource, units).
    pub phases: &'static [&'static [(u16, i32)]],
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

pub const MONUMENTS: [MonumentDef; 3] = [
    MonumentDef { kind: kind::SMALL_MASTABA, cols: 2, rows: 5, entrance_row: 2, side_rows: 1, first_row_side: false, phases: &SMALL_MASTABA_PHASES },
    MonumentDef { kind: kind::MEDIUM_MASTABA, cols: 3, rows: 7, entrance_row: 3, side_rows: 2, first_row_side: true, phases: &MEDIUM_MASTABA_PHASES },
    MonumentDef { kind: kind::LARGE_MASTABA, cols: 4, rows: 9, entrance_row: 4, side_rows: 3, first_row_side: true, phases: &LARGE_MASTABA_PHASES },
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
    /// The craftsman on site.
    pub craftsman: FigureId,
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
}

impl MonumentDef {
    fn block(&self, c: i32, r: i32) -> Block {
        if r == 0 {
            return if self.first_row_side && c == self.cols - 1 { Block::Side } else { Block::Main };
        }
        if r >= self.rows - self.side_rows {
            return Block::Side;
        }
        if r == self.entrance_row && c == self.cols - 1 {
            return Block::Entrance;
        }
        Block::Wall
    }

    /// What the current phase still needs of `r`, counting what is on its way.
    fn needs(&self, m: &Monument, r: u16) -> i32 {
        let want = self.phases.get(m.phase as usize).map_or(0, |p| Monument::amount(p, r));
        want - Monument::amount(&m.delivered, r) - Monument::amount(&m.in_flight, r)
    }

    /// How many of the phase's blocks the delivered material pays for.
    fn blocks_paid(&self, m: &Monument) -> usize {
        let blocks = (self.cols * self.rows) as usize;
        let Some(phase) = self.phases.get(m.phase as usize) else { return blocks };
        phase
            .iter()
            .map(|&(r, want)| (Monument::amount(&m.delivered, r) as i64 * blocks as i64 / want.max(1) as i64) as usize)
            .min()
            .unwrap_or(blocks)
            .min(blocks)
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

    /// Redraws a monument's tiles for its phase and the blocks done so far.
    pub fn refresh_monument_images(&mut self, id: BuildingId) {
        let Some(b) = self.buildings.get(id) else { return };
        let Some(def) = monument_def(b.kind) else { return };
        let Some(m) = b.monument.clone() else { return };
        let (x0, y0) = (b.x, b.y);
        let (w, h) = b.footprint();
        let Some(bdef) = self.defs.building(b.kind) else { return };
        let site = bdef.image;
        let bricks = bdef.anims.get("base_bricks").map_or(0, |a| a.image);
        if m.phase < LEVELING_PHASES && !m.finished {
            // Staked-out ground, with the corners and edges marked.
            let (x1, y1) = (x0 + w - 1, y0 + h - 1);
            for y in y0..=y1 {
                for x in x0..=x1 {
                    let inside = x > x0 && x < x1 || y > y0 && y < y1;
                    let interior = site + 5 + ((x + y) % 7) as u32;
                    let image = if m.phase == 1 {
                        // Levelled ground.
                        site + 41 + ((x * 3 + y) % 9) as u32
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
                        interior
                    };
                    self.map.set_single_image(x, y, image);
                }
            }
            return;
        }
        // Brick courses: each block shows the course it has reached.
        let course = |phase: u8| phase as i32 - LEVELING_PHASES as i32 + 1;
        for r in 0..def.rows {
            for c in 0..def.cols {
                let i = (r * def.cols + c) as usize;
                let done_now = m.finished || m.progress.get(i).copied().unwrap_or(0) >= BLOCK_WORK;
                let layer = if m.finished {
                    course(def.phases.len() as u8 - 2)
                } else if done_now {
                    course(m.phase)
                } else {
                    course(m.phase) - 1
                };
                let (bx, by) = (x0 + c * 2, y0 + r * 2);
                if layer <= 0 {
                    for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                        self.map.set_single_image(bx + dx, by + dy, site + 41 + (((bx + dx) * 3 + by + dy) % 9) as u32);
                    }
                    continue;
                }
                let l = (layer - 1) as u32;
                let image = match def.block(c, r) {
                    Block::Entrance => bricks + 110,
                    _ if r == 0 => bricks + l * 8 + 7,
                    _ if r == def.rows - 1 => bricks + l * 8 + 5,
                    _ => bricks + 96 + l,
                };
                self.map.set_footprint(bx, by, 2, image);
            }
        }
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

    /// Tick 31: guilds send their craftsman to a monument that needs one.
    pub(crate) fn guild_walkers(&mut self) {
        let guilds: Vec<BuildingId> = self.buildings.iter().filter(|b| b.kind == kind::BRICKLAYERS_GUILD).map(|b| b.id).collect();
        for g in guilds {
            let Some(gb) = self.buildings.get(g) else { continue };
            let Some(road) = gb.road else { continue };
            // The guild keeps a load of bricks to work with.
            if gb.workers <= 0 || gb.walkers[0] != 0 || gb.stock.get(BRICKS as usize).copied().unwrap_or(0) < crate::economy::LOAD {
                continue;
            }
            let from = (gb.x, gb.y);
            let target = self.active_monuments().into_iter().find(|&id| {
                let Some(b) = self.buildings.get(id) else { return false };
                let Some(m) = b.monument.as_ref() else { return false };
                m.craftsman == 0 && m.phase >= LEVELING_PHASES && monument_def(b.kind).is_some_and(|d| d.phases.get(m.phase as usize).is_some_and(|p| !p.is_empty()))
            });
            let Some(target) = target else { continue };
            let Some(spot) = self.monument_access(target, from) else { continue };
            let fid = self.figures.spawn(BRICKLAYER, road.0, road.1, Travel::Land);
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
                m.craftsman = fid;
            }
        }
    }

    /// A craftsman walks to the monument, then lays each block of the course as far
    /// as the delivered material allows.
    pub(crate) fn update_craftsman(&mut self, fid: FigureId) {
        let Some(f) = self.figures.get(fid) else { return };
        let (act, target) = (f.action, f.target);
        let working = self.buildings.get(target).and_then(|b| b.monument.as_ref()).is_some_and(|m| !m.finished && m.craftsman == fid);
        if !working && act != 3 {
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
                let paid = def.blocks_paid(m);
                let next = m.progress.iter().enumerate().position(|(i, &p)| p < BLOCK_WORK && i < paid);
                let moving = next.is_some();
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
            let Some(phase) = def.phases.get(m.phase as usize) else { continue };
            for &(r, _) in phase.iter() {
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
            let Some(b) = self.buildings.get(id) else { continue };
            let Some(def) = monument_def(b.kind) else { continue };
            let m = b.monument.as_ref().expect("active");
            let all_done = m.progress.iter().all(|&p| p >= BLOCK_WORK);
            let paid = def.phases.get(m.phase as usize).is_none_or(|p| p.iter().all(|&(r, want)| Monument::amount(&m.delivered, r) >= want));
            if !all_done || !paid {
                continue;
            }
            let (x, y) = (b.x, b.y);
            let last = def.phases.len() as u8 - 1;
            let m = self.buildings.get_mut(id).and_then(|b| b.monument.as_mut()).expect("active");
            m.phase += 1;
            m.delivered.clear();
            m.progress.iter_mut().for_each(|p| *p = 0);
            let empty_phase = def.phases.get(m.phase as usize).is_some_and(|p| p.is_empty());
            if m.phase >= last || (m.phase >= LEVELING_PHASES && empty_phase) {
                m.finished = true;
                m.craftsman = 0;
                self.post("message_history_mastaba", Some((x, y)), true);
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
        let needs = def.phases.get(m.phase as usize).map_or_else(Vec::new, |p| p.iter().map(|&(r, want)| (r, Monument::amount(&m.delivered, r), want)).collect());
        Some((m.phase, m.finished, needs))
    }
}
