//! The `megacity` script step: a big city laid out from the map with the game's own
//! build commands (and the Clear Land tool), meant to feed, house and employ itself so
//! it grows for years.
//!
//! It scores squares of the map a road can reach from the entry by the farmable
//! floodplain, the groundwater away from it and the open land in them, tries the best
//! three both ways round on copies of the world, and builds the plan that gets the
//! most farms, water supplies and house lots. One road grid covers the square: roads
//! every 7 tiles across the floodplain's strip and every 5 along it, so a block is 4
//! tiles by 6. On the floodplain only the cross roads are laid, and 3x3 farms fill the
//! 6-tile bands between them, each touching a road; roads cut off from the entry are
//! taken up again. The whole blocks nearest the floodplain take granaries, work camps,
//! storage yards and workshops, broken ones by it meadow farms (watered by a water lift
//! and a ditch where the river allows). The rest follow a pattern of 4 by 3 blocks,
//! seven of them services (temples, shrines, bazaars, water, health, schools, the law,
//! fire and architects, the venues' schools) and five housing, each housing block with
//! a statue in it. Venues and the festival square go over road crossings. Clay pits,
//! reed gatherers, a shipwright and fishing wharves go where the map allows, and the
//! labor priorities are set as a player would: food, industry, health, infrastructure.

use osiris_sim::buildings::kind;
use osiris_sim::map::{mask, terrain};
use osiris_sim::{Command, Outcome, World};
use std::collections::{BTreeMap, VecDeque};

const BARLEY: u16 = 100;
const FLAX: u16 = 101;
/// Food farms, most wanted first: grain, figs, chickpeas, lettuce, pomegranates.
const FOOD_FARMS: [u16; 5] = [102, 196, 105, 103, 104];
const MEDIUM_STATUE: u16 = 42;
const SMALL_STATUE: u16 = 41;
const POLICE: u16 = 55;
const FIREHOUSE: u16 = 167;
const COURTHOUSE: u16 = 184;
const WATER_SUPPLY: u16 = 180;
const PHYSICIAN: u16 = 206;
const DENTIST: u16 = 49;
const WORK_CAMP: u16 = 199;
const POTTERY: u16 = 114;
const BREWERY: u16 = 110;
const WEAVER: u16 = 111;
const PAPYRUS: u16 = 203;
const CLAY_PIT: u16 = 109;
const REED_GATHERER: u16 = 195;
const SHIPWRIGHT: u16 = 74;
const FISHING_WHARF: u16 = 76;
const WATER_LIFT: u16 = osiris_sim::irrigation::WATER_LIFT;
const DITCH: u16 = osiris_sim::irrigation::DITCH;
const FESTIVAL_SQUARE: u16 = osiris_sim::religion::FESTIVAL_SQUARE;
/// Stand-ins in the cards for the module's two temples and its food farm.
const TEMPLE_A: u16 = 1000;
const TEMPLE_B: u16 = 1001;
const MEADOW_FARM: u16 = 1002;
/// A shrine, to each known god in turn.
const SHRINE: u16 = 1003;
const SHRINE_OSIRIS: u16 = 140;
/// Meadow poorer than this isn't farmed: irrigation adds only 40.
const MEADOW_FERTILITY: i32 = 25;

/// What a block holds: buildings at (dx, dy) in a block 4 tiles across and 6 down,
/// every one touching the block's edge (so a road).
type Card = &'static [(u16, i32, i32)];

const MARKET: Card = &[
    (TEMPLE_A, 0, 0),
    (FIREHOUSE, 3, 0),
    (POLICE, 3, 1),
    (kind::ARCHITECT_POST, 3, 2),
    (kind::BAZAAR, 0, 3),
    (PHYSICIAN, 2, 3),
    (kind::APOTHECARY, 0, 5),
    (DENTIST, 1, 5),
    (kind::WELL, 2, 5),
    (SHRINE, 3, 5),
];
const WATER: Card = &[
    (TEMPLE_B, 0, 0),
    (FIREHOUSE, 3, 0),
    (SHRINE, 3, 1),
    (SMALL_STATUE, 3, 2),
    (WATER_SUPPLY, 0, 3),
    (kind::SCRIBAL_SCHOOL, 2, 3),
    (kind::WELL, 0, 5),
    (SHRINE, 1, 5),
    (SHRINE, 2, 5),
    (SMALL_STATUE, 3, 5),
];
const LAW: Card = &[
    (COURTHOUSE, 0, 0),
    (FIREHOUSE, 3, 0),
    (POLICE, 3, 1),
    (SHRINE, 3, 2),
    (kind::TAX_COLLECTOR, 0, 3),
    (kind::MORTUARY, 2, 3),
    (SHRINE, 0, 5),
    (SMALL_STATUE, 1, 5),
    (kind::WELL, 2, 5),
    (kind::ARCHITECT_POST, 3, 5),
];
const CULTURE: Card = &[
    (kind::CONSERVATORY, 0, 0),
    (FIREHOUSE, 3, 0),
    (SHRINE, 3, 1),
    (SHRINE, 3, 2),
    (kind::JUGGLER_SCHOOL, 0, 3),
    (kind::BAZAAR, 2, 3),
    (SHRINE, 0, 5),
    (kind::WELL, 1, 5),
    (SMALL_STATUE, 2, 5),
    (SHRINE, 3, 5),
];
const DANCE: Card = &[(kind::DANCE_SCHOOL, 0, 0), (WATER_SUPPLY, 0, 4), (MEDIUM_STATUE, 2, 4)];
const CARE: Card = &[
    (kind::BAZAAR, 0, 0),
    (WATER_SUPPLY, 2, 0),
    (PHYSICIAN, 0, 2),
    (kind::SCRIBAL_SCHOOL, 2, 2),
    (kind::TAX_COLLECTOR, 0, 4),
    (kind::MORTUARY, 2, 4),
];
const GRANARY: Card = &[(kind::GRANARY, 0, 0), (WORK_CAMP, 0, 4), (FIREHOUSE, 2, 4), (kind::ARCHITECT_POST, 3, 4), (SHRINE, 2, 5), (SHRINE, 3, 5)];
const CAMPS: Card = &[(WORK_CAMP, 0, 0), (WORK_CAMP, 2, 0), (WORK_CAMP, 0, 2), (WORK_CAMP, 2, 2), (kind::BAZAAR, 0, 4), (SHRINE, 2, 4), (SHRINE, 3, 5)];
const STORAGE: Card = &[
    (kind::STORAGE_YARD, 0, 0),
    (FIREHOUSE, 3, 0),
    (kind::ARCHITECT_POST, 3, 1),
    (SHRINE, 3, 2),
    (kind::STORAGE_YARD, 0, 3),
    (SHRINE, 3, 3),
    (SHRINE, 3, 4),
    (SHRINE, 3, 5),
];
const WORKSHOPS: Card = &[(POTTERY, 0, 0), (BREWERY, 2, 0), (WEAVER, 0, 2), (PAPYRUS, 2, 2), (BREWERY, 0, 4), (FIREHOUSE, 2, 4), (SHRINE, 3, 4), (SHRINE, 2, 5), (SHRINE, 3, 5)];
const MEADOW: Card = &[(MEADOW_FARM, 0, 0), (MEADOW_FARM, 0, 3)];

/// The city's module of 4 blocks across (along the floodplain) by 3 down (away from
/// it); `None` is housing.
const PATTERN: [[Option<Card>; 4]; 3] = [[None, Some(MARKET), None, Some(CULTURE)], [Some(CARE), None, Some(WATER), None], [Some(LAW), None, Some(CARE), Some(DANCE)]];

/// What the step built, for its report.
#[derive(Default)]
struct Tally {
    built: BTreeMap<u16, u32>,
    failed: BTreeMap<u16, u32>,
    roads: i32,
}

struct Planner<'a> {
    world: &'a mut World,
    /// Road lines: x = ox + k * sx, y = oy + k * sy; the region's bounds.
    ox: i32,
    oy: i32,
    sx: i32,
    sy: i32,
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    /// Roads of constant y cross the floodplain (else roads of constant x do).
    rows_cross: bool,
    /// Road tiles joined to the entry.
    joined: Vec<bool>,
    tally: Tally,
    /// What the plan did, for the report.
    notes: Vec<String>,
    /// Temples and shrines built so far, to deal the gods out in turn.
    turns: (usize, usize),
}

/// A number from the seed, the way the other harness steps draw theirs.
fn mix(seed: u64, n: u64) -> u64 {
    let mut s = (seed ^ n.wrapping_mul(0x9e37_79b9_7f4a_7c15)) | 1;
    s ^= s << 13;
    s ^= s >> 7;
    s ^= s << 17;
    s
}

/// Builds the megacity: region half-size `r` tiles (0 picks 60). The three best
/// sites are each tried both ways round on a copy of the world, and the plan that
/// gets the most farms, houses and water supplies is built. Returns the city's centre.
pub fn megacity(world: &mut World, seed: u64, r: i32) -> Option<(i32, i32)> {
    let r = if r <= 0 { 60 } else { r.clamp(15, 100) };
    if world.treasury < 200_000 {
        eprintln!("megacity: treasury raised from {} to 200000", world.treasury);
        world.treasury = 200_000;
    }
    let sites = pick_sites(world, r, 3);
    let mut best: Option<((i32, i32), bool, i32)> = None;
    for &c in &sites {
        for rows_cross in [true, false] {
            let mut trial = world.clone();
            let p = plan(&mut trial, seed, r, c, rows_cross);
            let score = p.score();
            eprintln!("megacity: trying {},{} with {}: {} farms, {} house lots, {} water supplies (score {score})", c.0, c.1, if rows_cross { "rows" } else { "columns" }, p.count(|w, k| w.is_farm(k)), p.count(|_, k| k == kind::VACANT_LOT), p.count(|_, k| k == WATER_SUPPLY));
            if best.is_none_or(|b| score > b.2) {
                best = Some((c, rows_cross, score));
            }
        }
    }
    let Some((c, rows_cross, _)) = best else {
        eprintln!("megacity: no land reachable from the entry");
        return None;
    };
    let start_money = world.treasury;
    let p = plan(world, seed, r, c, rows_cross);
    for n in &p.notes {
        eprintln!("megacity: {n}");
    }
    let t = &p.tally;
    let name = |k: &u16| p.world.defs.building(*k).map_or_else(|| k.to_string(), |d| d.key.clone());
    let built: Vec<String> = t.built.iter().map(|(k, n)| format!("{} {n}", name(k))).collect();
    let failed: Vec<String> = t.failed.iter().map(|(k, n)| format!("{} {n}", name(k))).collect();
    eprintln!("megacity: {} road tiles, spent {}; built {}", t.roads, start_money - p.world.treasury, built.join(", "));
    eprintln!("megacity: couldn't place {}", failed.join(", "));
    Some(c)
}

/// Lays the whole city round centre `c`, with the roads of constant y crossing the
/// floodplain when `rows_cross`.
fn plan(world: &mut World, seed: u64, r: i32, (cx, cy): (i32, i32), rows_cross: bool) -> Planner<'_> {
    let (sx, sy) = if rows_cross { (5, 7) } else { (7, 5) };
    let (nx, ny) = (r / sx, r / sy);
    let (ox, oy) = (cx + (mix(seed, 1) % sx as u64) as i32, cy + (mix(seed, 2) % sy as u64) as i32);
    let mut p = Planner { ox, oy, sx, sy, x0: ox - nx * sx, y0: oy - ny * sy, x1: ox + nx * sx, y1: oy + ny * sy, rows_cross, joined: Vec::new(), tally: Tally::default(), notes: Vec::new(), turns: ((mix(seed, 3) % 5) as usize, (mix(seed, 4) % 5) as usize), world };
    p.notes.push(format!("seed {seed}: centre {cx},{cy}, region {},{} to {},{}, roads every {sx} across and {sy} down, floodplain crossed by {}", p.x0, p.y0, p.x1, p.y1, if rows_cross { "rows" } else { "columns" }));
    // Trees and shrubs come off the city's ground first (the Clear Land tool), so the
    // grid runs unbroken; rock, marsh and water stay.
    if let Outcome::Done { items, cost } = p.world.apply(&Command::Clear { x0: p.x0, y0: p.y0, x1: p.x1, y1: p.y1 }) {
        p.notes.push(format!("cleared {items} tiles for {cost}"));
    }
    p.lay_roads();
    p.join_entry();
    p.clear_stray_roads();
    let (farms, food) = p.floodplain_farms();
    p.raw_materials();
    p.blocks(farms, food);
    // As a player would at the Overseer of Labor: food first (farms and lifts), then
    // industry (the work camps that tend the floodplain, storage and workshops), then
    // health and water (against malaria and disease), then infrastructure (granaries,
    // firemen and architects).
    for (rank, category) in [(1, "food_production"), (2, "industry_commerce"), (3, "water_health"), (4, "infrastructure")] {
        if let Some(ci) = osiris_sim::labor::CATEGORIES.iter().position(|&c| c == category) {
            p.world.set_labor_priority(ci, rank);
        }
    }
    p
}

/// Whether tile (x, y) is open land: no terrain in the way, no building.
fn open(w: &World, x: i32, y: i32) -> bool {
    w.map.contains(x, y) && !w.map.terrain_is(x, y, mask::NOT_CLEAR) && w.map.building.at_or(x, y, 0) == 0
}

/// Dry floodplain with nothing on it.
fn floodplain(w: &World, x: i32, y: i32) -> bool {
    w.map.contains(x, y) && w.map.terrain_is(x, y, terrain::FLOODPLAIN) && !w.map.terrain_is(x, y, mask::NOT_CLEAR & !terrain::FLOODPLAIN) && w.map.building.at_or(x, y, 0) == 0
}

/// Land the Clear Land tool can make open: nothing but trees, shrubs and the like.
fn clearable(w: &World, x: i32, y: i32) -> bool {
    w.map.contains(x, y) && !w.map.terrain_is(x, y, mask::NOT_CLEAR & !mask::CLEARABLE)
}

/// Tiles a road could reach from the entry once trees and shrubs are cleared.
fn reachable(w: &World) -> Vec<bool> {
    let (mw, mh) = (w.map.width, w.map.height);
    let mut seen = vec![false; (mw * mh) as usize];
    let passable = |x: i32, y: i32| w.map.contains(x, y) && !w.map.terrain_is(x, y, mask::NOT_CLEAR & !mask::CLEARABLE & !terrain::FLOODPLAIN);
    let (ex, ey) = w.entry_point;
    if !passable(ex, ey) {
        return seen;
    }
    seen[(ey * mw + ex) as usize] = true;
    let mut queue = VecDeque::from([(ex, ey)]);
    while let Some((x, y)) = queue.pop_front() {
        for (dx, dy) in [(0, -1), (1, 0), (0, 1), (-1, 0)] {
            let (nx, ny) = (x + dx, y + dy);
            if passable(nx, ny) && !seen[(ny * mw + nx) as usize] {
                seen[(ny * mw + nx) as usize] = true;
                queue.push_back((nx, ny));
            }
        }
    }
    seen
}

/// The centres of the `n` best squares of half-size `r`, far enough apart: those a road can reach from the entry that
/// holds the most farmable floodplain (up to forty farms' worth), then groundwater
/// away from the floodplain (for the houses' water), then open land, a little nearer
/// the entry on a tie.
fn pick_sites(w: &World, r: i32, n: usize) -> Vec<(i32, i32)> {
    let (mw, mh) = (w.map.width, w.map.height);
    let reach = reachable(w);
    // Floodplain a farm fits on: tiles of some 3x3 square all of floodplain.
    let mut farmable = vec![false; (mw * mh) as usize];
    for y in 0..mh {
        for x in 0..mw {
            if (0..3).all(|d| (0..3).all(|e| floodplain(w, x + d, y + e))) {
                for (d, e) in (0..3).flat_map(|d| (0..3).map(move |e| (d, e))) {
                    farmable[((y + e) * mw + x + d) as usize] = true;
                }
            }
        }
    }
    let far = distance_to_floodplain(w);
    // Summed-area tables of land, groundwater land away from the farms (where the
    // houses will be) and floodplain.
    let mut sums = [vec![0i32; ((mw + 1) * (mh + 1)) as usize], vec![0i32; ((mw + 1) * (mh + 1)) as usize], vec![0i32; ((mw + 1) * (mh + 1)) as usize]];
    for y in 0..mh {
        for x in 0..mw {
            let ok = reach[(y * mw + x) as usize];
            let land = ok && clearable(w, x, y);
            let values = [land as i32, (land && far[(y * mw + x) as usize] > 6 && w.map.terrain_is(x, y, terrain::GROUNDWATER)) as i32, (ok && farmable[(y * mw + x) as usize]) as i32];
            for (s, v) in sums.iter_mut().zip(values) {
                let i = ((y + 1) * (mw + 1) + x + 1) as usize;
                s[i] = v + s[i - 1] + s[i - (mw + 1) as usize] - s[i - (mw + 2) as usize];
            }
        }
    }
    let area = |s: &Vec<i32>, x0: i32, y0: i32, x1: i32, y1: i32| {
        let (x0, y0, x1, y1) = (x0.clamp(0, mw), y0.clamp(0, mh), (x1 + 1).clamp(0, mw), (y1 + 1).clamp(0, mh));
        let at = |x: i32, y: i32| s[(y * (mw + 1) + x) as usize];
        at(x1, y1) - at(x0, y1) - at(x1, y0) + at(x0, y0)
    };
    let (ex, ey) = w.entry_point;
    let mut scored: Vec<(i32, (i32, i32))> = Vec::new();
    for cy in (0..mh).step_by(3) {
        for cx in (0..mw).step_by(3) {
            let (land, water, flood) = (area(&sums[0], cx - r, cy - r, cx + r, cy + r), area(&sums[1], cx - r, cy - r, cx + r, cy + r), area(&sums[2], cx - r, cy - r, cx + r, cy + r));
            if land < r * r {
                continue;
            }
            // The land wants about twice the floodplain (a third of the square farmed),
            // and groundwater under it for the houses' water.
            let score = (flood / 9).min(40) * 60 + water.min(1500) + land / 4 - ((cx - ex).abs() + (cy - ey).abs());
            scored.push((-score, (cx, cy)));
        }
    }
    scored.sort();
    // The best few, each at least `r` from the others.
    let mut picked: Vec<(i32, i32)> = Vec::new();
    for (_, c) in scored {
        if picked.len() >= n {
            break;
        }
        if picked.iter().all(|p| (p.0 - c.0).abs().max((p.1 - c.1).abs()) >= r) {
            picked.push(c);
        }
    }
    picked
}

/// Every tile's distance in steps to the nearest floodplain.
fn distance_to_floodplain(w: &World) -> Vec<i32> {
    let (mw, mh) = (w.map.width, w.map.height);
    let mut dist = vec![i32::MAX; (mw * mh) as usize];
    let mut queue = VecDeque::new();
    for y in 0..mh {
        for x in 0..mw {
            if w.map.terrain_is(x, y, terrain::FLOODPLAIN) {
                dist[(y * mw + x) as usize] = 0;
                queue.push_back((x, y));
            }
        }
    }
    while let Some((x, y)) = queue.pop_front() {
        let d = dist[(y * mw + x) as usize];
        for (dx, dy) in [(0, -1), (1, 0), (0, 1), (-1, 0)] {
            let (nx, ny) = (x + dx, y + dy);
            if nx >= 0 && ny >= 0 && nx < mw && ny < mh && dist[(ny * mw + nx) as usize] > d + 1 {
                dist[(ny * mw + nx) as usize] = d + 1;
                queue.push_back((nx, ny));
            }
        }
    }
    dist
}

impl Planner<'_> {
    /// Buildings built of the kinds `f` picks.
    fn count(&self, f: impl Fn(&World, u16) -> bool) -> u32 {
        self.tally.built.iter().filter(|(k, _)| f(self.world, **k)).map(|(_, n)| n).sum()
    }

    /// How good the plan is: farms (up to fifty) count most, then water supplies,
    /// then house lots.
    fn score(&self) -> i32 {
        let farms = self.count(|w, k| w.is_farm(k)).min(50) as i32;
        farms * 40 + self.count(|_, k| k == WATER_SUPPLY) as i32 * 40 + self.count(|_, k| k == kind::VACANT_LOT) as i32
    }

    fn build(&mut self, k: u16, x: i32, y: i32) -> bool {
        let ok = self.world.is_allowed(k) && self.world.can_place(k, x, y).is_ok() && matches!(self.world.apply(&Command::Build { kind: k, x, y, x1: x, y1: y }), Outcome::Done { .. });
        *if ok { self.tally.built.entry(k) } else { self.tally.failed.entry(k) }.or_default() += 1;
        ok
    }

    fn road(&mut self, a: (i32, i32), b: (i32, i32)) -> bool {
        match self.world.apply(&Command::Road { start: a, end: b }) {
            Outcome::Done { items, .. } => {
                self.tally.roads += items;
                true
            }
            _ => false,
        }
    }

    /// Lays the grid: each line's runs of open land (and, for the lines crossing the
    /// floodplain, floodplain) as straight roads.
    fn lay_roads(&mut self) {
        let reach = reachable(self.world);
        let mw = self.world.map.width;
        let mut lines: Vec<(bool, i32)> = Vec::new();
        lines.extend((self.y0..=self.y1).step_by(self.sy as usize).map(|y| (true, y)));
        lines.extend((self.x0..=self.x1).step_by(self.sx as usize).map(|x| (false, x)));
        for (row, at) in lines {
            let crossing = row == self.rows_cross;
            let (a, b) = if row { (self.x0, self.x1) } else { (self.y0, self.y1) };
            let tile = |i: i32| if row { (i, at) } else { (at, i) };
            let mut run: Option<i32> = None;
            for i in a..=b + 1 {
                let (x, y) = tile(i);
                let ok = i <= b && self.world.map.contains(x, y) && reach[(y * mw + x) as usize] && (open(self.world, x, y) || self.world.map.terrain_is(x, y, terrain::ROAD) || (crossing && floodplain(self.world, x, y)));
                match (ok, run) {
                    (true, None) => run = Some(i),
                    (false, Some(s)) => {
                        if i - 1 > s {
                            self.road(tile(s), tile(i - 1));
                        }
                        run = None;
                    }
                    _ => {}
                }
            }
        }
    }

    /// Joins the entry to the nearest grid road and marks the roads joined to it.
    fn join_entry(&mut self) {
        let (ex, ey) = self.world.entry_point;
        let mut nearest: Option<((i32, i32), i32)> = None;
        for y in self.y0..=self.y1 {
            for x in self.x0..=self.x1 {
                if self.world.map.terrain_is(x, y, terrain::ROAD) && !self.world.map.terrain_is(x, y, terrain::FLOODPLAIN) {
                    let d = (x - ex).abs() + (y - ey).abs();
                    if nearest.is_none_or(|n| d < n.1) {
                        nearest = Some(((x, y), d));
                    }
                }
            }
        }
        if let Some((p, _)) = nearest
            && !self.road((ex, ey), p)
        {
            // Trees in the way: clear a way through them, then lay the road.
            let cleared = self.clear_way((ex, ey), p);
            if !self.road((ex, ey), p) {
                self.notes.push(format!("couldn't join the entry {ex},{ey} to the city at {p:?}"));
            } else {
                self.notes.push(format!("cleared {cleared} tiles of trees for the road from the entry"));
            }
        }
        self.mark_joined();
    }

    /// Clears the grid's roads that don't join the entry (cut off by water or rock):
    /// a house beside one would send its newcomers there, and they'd never arrive.
    fn clear_stray_roads(&mut self) {
        let mut cleared = 0;
        for y in self.y0..=self.y1 {
            for x in self.x0..=self.x1 {
                if self.world.map.terrain_is(x, y, terrain::ROAD) && !self.is_joined((x, y)) && matches!(self.world.apply(&Command::Clear { x0: x, y0: y, x1: x, y1: y }), Outcome::Done { .. }) {
                    cleared += 1;
                }
            }
        }
        if cleared > 0 {
            self.notes.push(format!("took up {cleared} road tiles cut off from the entry"));
        }
    }

    /// Clears the trees and shrubs on the shortest way from `a` to `b` over land the
    /// Clear Land tool can open, returning how many tiles it cleared.
    fn clear_way(&mut self, a: (i32, i32), b: (i32, i32)) -> i32 {
        let w = &*self.world;
        let mw = w.map.width;
        let passable = |x: i32, y: i32| clearable(w, x, y) || w.map.terrain_is(x, y, terrain::ROAD);
        let mut from = vec![-1i32; (mw * w.map.height) as usize];
        from[(a.1 * mw + a.0) as usize] = a.1 * mw + a.0;
        let mut queue = VecDeque::from([a]);
        while let Some((x, y)) = queue.pop_front() {
            if (x, y) == b {
                break;
            }
            for (dx, dy) in [(0, -1), (1, 0), (0, 1), (-1, 0)] {
                let (nx, ny) = (x + dx, y + dy);
                if passable(nx, ny) && from[(ny * mw + nx) as usize] < 0 {
                    from[(ny * mw + nx) as usize] = y * mw + x;
                    queue.push_back((nx, ny));
                }
            }
        }
        let mut path = Vec::new();
        let (mut at, start) = (b.1 * mw + b.0, a.1 * mw + a.0);
        while at >= 0 && at != start {
            path.push((at % mw, at / mw));
            at = from[at as usize];
        }
        if at < 0 {
            return 0;
        }
        let mut cleared = 0;
        for (x, y) in path {
            if !open(self.world, x, y)
                && !self.world.map.terrain_is(x, y, terrain::ROAD)
                && let Outcome::Done { items, .. } = self.world.apply(&Command::Clear { x0: x, y0: y, x1: x, y1: y })
            {
                cleared += items;
            }
        }
        cleared
    }

    fn mark_joined(&mut self) {
        let w = &*self.world;
        let (mw, mh) = (w.map.width, w.map.height);
        let mut joined = vec![false; (mw * mh) as usize];
        let (ex, ey) = w.entry_point;
        let road = |x: i32, y: i32| w.map.contains(x, y) && w.map.terrain_is(x, y, terrain::ROAD);
        if road(ex, ey) {
            joined[(ey * mw + ex) as usize] = true;
            let mut queue = VecDeque::from([(ex, ey)]);
            while let Some((x, y)) = queue.pop_front() {
                for (dx, dy) in [(0, -1), (1, 0), (0, 1), (-1, 0)] {
                    let (nx, ny) = (x + dx, y + dy);
                    if road(nx, ny) && !joined[(ny * mw + nx) as usize] {
                        joined[(ny * mw + nx) as usize] = true;
                        queue.push_back((nx, ny));
                    }
                }
            }
        }
        self.joined = joined;
    }

    fn is_joined(&self, (x, y): (i32, i32)) -> bool {
        self.world.map.contains(x, y) && self.joined[(y * self.world.map.width + x) as usize]
    }

    /// A joined road beside the footprint `w` x `h` at (x, y).
    fn has_road(&self, x: i32, y: i32, w: i32, h: i32) -> bool {
        let top = (0..w).map(|i| (x + i, y - 1));
        let bottom = (0..w).map(|i| (x + i, y + h));
        let left = (0..h).map(|i| (x - 1, y + i));
        let right = (0..h).map(|i| (x + w, y + i));
        top.chain(bottom).chain(left).chain(right).any(|p| self.is_joined(p))
    }

    /// The crop for the next farm: mostly food, alternating kinds, with a barley and a
    /// flax farm in every ten when the mission grows them.
    fn crop(&self, n: u32) -> u16 {
        let food: Vec<u16> = FOOD_FARMS.iter().copied().filter(|&k| self.world.is_allowed(k)).collect();
        let pick = |slot: u32| food.get(slot as usize % food.len().max(1)).copied().unwrap_or(FOOD_FARMS[0]);
        match n % 10 {
            3 if self.world.is_allowed(BARLEY) => BARLEY,
            7 if self.world.is_allowed(FLAX) => FLAX,
            i => pick(i / 2 + (i % 2)),
        }
    }

    /// Fills the floodplain in the region with 3x3 farms beside joined roads. Returns
    /// the farms and the food farms built.
    fn floodplain_farms(&mut self) -> (u32, u32) {
        let (mut farms, mut food) = (0, 0);
        for y in self.y0..=self.y1 {
            for x in self.x0..=self.x1 {
                if !floodplain(self.world, x, y) || !self.has_road(x, y, 3, 3) {
                    continue;
                }
                let k = self.crop(farms);
                if self.world.can_place(k, x, y).is_ok() && (0..3).all(|d| (0..3).all(|e| floodplain(self.world, x + d, y + e))) && self.build(k, x, y) {
                    farms += 1;
                    food += FOOD_FARMS.contains(&k) as u32;
                }
            }
        }
        self.notes.push(format!("{farms} floodplain farms, {food} growing food"));
        (farms, food)
    }

    /// Clay pits, reed gatherers, a shipwright and fishing wharves where the map
    /// allows, near the city, each given a road to the grid.
    fn raw_materials(&mut self) {
        // Fish don't depend on the flood: a shipwright builds the wharves' boats.
        for (k, want) in [(CLAY_PIT, 4), (REED_GATHERER, 2), (SHIPWRIGHT, 2), (FISHING_WHARF, 10)] {
            if !self.world.is_allowed(k) || (k == FISHING_WHARF && self.world.buildings.count_of(SHIPWRIGHT) == 0) {
                continue;
            }
            let size = self.world.size_of(k);
            let (cx, cy) = ((self.x0 + self.x1) / 2, (self.y0 + self.y1) / 2);
            let margin = 12;
            let mut spots: Vec<(i32, (i32, i32))> = Vec::new();
            for y in self.y0 - margin..=self.y1 + margin {
                for x in self.x0 - margin..=self.x1 + margin {
                    // Off the grid's own lines, so no road is taken.
                    let on_line = |v: i32, o: i32, s: i32| (v - o).rem_euclid(s) == 0;
                    if (0..size).any(|d| on_line(x + d, self.ox, self.sx) || on_line(y + d, self.oy, self.sy)) {
                        continue;
                    }
                    if self.world.can_place(k, x, y).is_ok() {
                        spots.push(((x - cx).abs() + (y - cy).abs(), (x, y)));
                    }
                }
            }
            spots.sort();
            let mut placed = 0;
            for (_, (x, y)) in spots {
                if placed >= want {
                    break;
                }
                if self.world.can_place(k, x, y).is_err() {
                    continue;
                }
                if !self.has_road(x, y, size, size) && !self.road_to_grid(x, y, size) {
                    continue;
                }
                if self.build(k, x, y) {
                    placed += 1;
                }
            }
        }
    }

    /// Lays a road from beside the footprint at (x, y) to the nearest joined road
    /// within 16 tiles.
    fn road_to_grid(&mut self, x: i32, y: i32, size: i32) -> bool {
        let mut best: Option<((i32, i32), i32)> = None;
        for yy in y - 16..=y + 16 {
            for xx in x - 16..=x + 16 {
                if self.is_joined((xx, yy)) {
                    let d = (xx - x).abs() + (yy - y).abs();
                    if best.is_none_or(|b| d < b.1) {
                        best = Some(((xx, yy), d));
                    }
                }
            }
        }
        let Some((target, _)) = best else { return false };
        let sides = [(x - 1, y), (x + size, y), (x, y - 1), (x, y + size), (x - 1, y + size - 1), (x + size, y + size - 1)];
        for s in sides {
            if open(self.world, s.0, s.1) && self.road(s, target) {
                self.mark_joined();
                if self.has_road(x, y, size, size) {
                    return true;
                }
            }
        }
        false
    }

    /// Lays a road from any open or dry floodplain tile beside the water lift at (x, y),
    /// other than those behind it (`behind`, where its ditch starts), to the nearest
    /// joined road within 16 tiles.
    fn road_to_lift(&mut self, x: i32, y: i32, behind: &[(i32, i32)]) -> bool {
        let mut best: Option<((i32, i32), i32)> = None;
        for yy in y - 16..=y + 16 {
            for xx in x - 16..=x + 16 {
                let d = (xx - x).abs() + (yy - y).abs();
                if self.is_joined((xx, yy)) && best.is_none_or(|b| d < b.1) {
                    best = Some(((xx, yy), d));
                }
            }
        }
        let Some((target, _)) = best else { return false };
        let sides = [(x, y - 1), (x + 1, y - 1), (x + 2, y), (x + 2, y + 1), (x + 1, y + 2), (x, y + 2), (x - 1, y + 1), (x - 1, y)];
        for s in sides {
            if behind.contains(&s) || !(open(self.world, s.0, s.1) || floodplain(self.world, s.0, s.1)) {
                continue;
            }
            if self.road(s, target) {
                self.mark_joined();
                if self.has_road(x, y, 2, 2) {
                    return true;
                }
            }
        }
        false
    }

    /// The blocks between the roads: those mostly of fertile meadow take meadow farms;
    /// the whole blocks nearest the floodplain take the farms' granaries, work camps,
    /// storage and workshops; the rest follow the city's pattern; then the venues go
    /// over the crossings and houses fill every open block.
    fn blocks(&mut self, farms: u32, food: u32) {
        let (bw, bh) = (self.sx - 1, self.sy - 1);
        let mw = self.world.map.width;
        let dist = distance_to_floodplain(self.world);
        // (distance to the floodplain, position along it, x, y, pattern slot, module)
        let mut whole: Vec<(i32, i32, i32, i32, Option<Card>, i32)> = Vec::new();
        let mut broken: Vec<(i32, i32, i32)> = Vec::new();
        for (bj, by) in (self.y0 + 1..self.y1).step_by(self.sy as usize).enumerate() {
            for (bi, bx) in (self.x0 + 1..self.x1).step_by(self.sx as usize).enumerate() {
                let tiles = || (by..by + bh).flat_map(move |y| (bx..bx + bw).map(move |x| (x, y)));
                let land = tiles().filter(|&(x, y)| open(self.world, x, y)).count() as i32;
                if land < 6 || !self.has_road(bx, by, bw, bh) {
                    continue;
                }
                let d = tiles().map(|(x, y)| if self.world.map.contains(x, y) { dist[(y * mw + x) as usize] } else { i32::MAX }).min().unwrap_or(i32::MAX);
                if land < bw * bh {
                    broken.push((d, bx, by));
                    continue;
                }
                let (u, v) = if self.rows_cross { (bi as i32, bj as i32) } else { (bj as i32, bi as i32) };
                let module = u.div_euclid(4) * 7 + v.div_euclid(3) * 3;
                whole.push((d, if self.rows_cross { by } else { bx }, bx, by, PATTERN[v.rem_euclid(3) as usize][u.rem_euclid(4) as usize], module));
            }
        }
        // The farms' blocks: granaries for the food harvest (a granary holds four
        // farms' crops, and has a work camp by it), a work camp for every five farms,
        // two storage and two workshop blocks, taken by the whole blocks nearest
        // the floodplain and dealt out along it.
        let granaries = (food as i32 + 3) / 4;
        let camp_blocks = (((farms as i32 + 4) / 5 - granaries).max(0) + 3) / 4;
        let mut wanted: Vec<Card> = Vec::new();
        let (mut g, mut c, mut st, mut wk) = (granaries, camp_blocks, 2, 2);
        while g + c + st + wk > 0 {
            for (left, card) in [(&mut g, GRANARY), (&mut c, CAMPS), (&mut st, STORAGE), (&mut wk, WORKSHOPS)] {
                if *left > 0 {
                    *left -= 1;
                    wanted.push(card);
                }
            }
        }
        // Blocks mostly of fertile meadow are farmed first (and watered below).
        let meadow_ok = FOOD_FARMS.iter().any(|&k| self.world.is_allowed(k));
        let meadow = |w: &World, bx: i32, by: i32| {
            let tiles: Vec<(i32, i32)> = (by..by + bh).flat_map(|y| (bx..bx + bw).map(move |x| (x, y))).filter(|&(x, y)| w.map.terrain_is(x, y, terrain::MEADOW)).collect();
            let fertility = tiles.iter().map(|&(x, y)| w.map.fertility.at_or(x, y, 0) as i32).sum::<i32>() / tiles.len().max(1) as i32;
            meadow_ok && tiles.len() as i32 * 2 >= bw * bh && fertility >= MEADOW_FERTILITY
        };
        let meadows: Vec<(i32, i32)> = whole.iter().map(|b| (b.2, b.3)).chain(broken.iter().map(|b| (b.1, b.2))).filter(|&(bx, by)| meadow(self.world, bx, by)).collect();
        for &(bx, by) in &meadows {
            self.lay_card(bx, by, MEADOW, 0);
        }
        whole.retain(|b| !meadows.contains(&(b.2, b.3)));
        broken.retain(|b| !meadows.contains(&(b.1, b.2)));
        whole.sort_by_key(|b| (b.0, b.1));
        let mut farm_side: Vec<(i32, i32, i32)> = whole.iter().take(wanted.len()).map(|b| (b.1, b.2, b.3)).collect();
        farm_side.sort();
        for (&(_, bx, by), &card) in farm_side.iter().zip(&wanted) {
            self.lay_card(bx, by, card, 0);
        }
        let mut city: Vec<(i32, i32, Card, i32)> = Vec::new();
        let mut housing: Vec<(i32, i32)> = Vec::new();
        let rest = whole.iter().skip(wanted.len()).map(|&(_, _, bx, by, slot, module)| (bx, by, slot, module));
        for (bx, by, slot, module) in rest.chain(broken.iter().map(|&(_, bx, by)| (bx, by, None, 0))) {
            match slot {
                Some(card) => city.push((bx, by, card, module)),
                None => housing.push((bx, by)),
            }
        }
        // The ditches go in before the city's buildings can stand in their way.
        self.irrigate();
        for &(bx, by, card, module) in &city {
            self.lay_card(bx, by, card, module);
        }
        self.venues(&city);
        // Every third housing block with groundwater along its edge gets a water
        // supply, so the houses round it can grow past huts and shanties.
        for (i, &(bx, by)) in housing.iter().enumerate() {
            if i % 3 == 0 {
                let spots = [(0, 0), (2, 0), (0, 4), (2, 4), (0, 2), (2, 2)];
                for (dx, dy) in spots {
                    let (dx, dy) = if self.rows_cross { (dx, dy) } else { (dy, dx) };
                    let (x, y) = (bx + dx, by + dy);
                    if self.world.map.terrain_is(x, y, terrain::GROUNDWATER) && self.world.can_place(WATER_SUPPLY, x, y).is_ok() && self.build(WATER_SUPPLY, x, y) {
                        break;
                    }
                }
            }
        }
        // A statue in the middle of each housing block lifts the desirability the huts
        // drag down, so they can grow.
        for (i, &(bx, by)) in housing.iter().enumerate() {
            let (dx, dy) = if i % 2 == 0 { (0, 2) } else { (2, 2) };
            let (dx, dy) = if self.rows_cross { (dx, dy) } else { (dy, dx) };
            if self.world.can_place(MEDIUM_STATUE, bx + dx, by + dy).is_ok() {
                self.build(MEDIUM_STATUE, bx + dx, by + dy);
            }
        }
        for (bx, by) in housing {
            let (x1, y1) = (bx + bw - 1, by + bh - 1);
            if let Outcome::Done { items, .. } = self.world.apply(&Command::Build { kind: kind::VACANT_LOT, x: bx, y: by, x1, y1 }) {
                *self.tally.built.entry(kind::VACANT_LOT).or_default() += items as u32;
            }
        }
    }

    /// Lays a card's buildings in the block at (bx, by), turned when the blocks lie
    /// across; `module` picks the temples' gods.
    fn lay_card(&mut self, bx: i32, by: i32, card: Card, module: i32) {
        let gods = self.gods();
        let mut meadow = 0;
        for &(k, dx, dy) in card {
            let (dx, dy) = if self.rows_cross { (dx, dy) } else { (dy, dx) };
            let k = match k {
                TEMPLE_A | TEMPLE_B | SHRINE if gods.is_empty() => kind::GARDENS,
                // The two temples of a module are to different gods, the gods taken in
                // turn over the city so each has as many.
                TEMPLE_A | TEMPLE_B => {
                    self.turns.0 += 1;
                    kind::TEMPLE_OSIRIS + gods[(self.turns.0 + module as usize) % gods.len()] as u16
                }
                SHRINE => {
                    self.turns.1 += 1;
                    SHRINE_OSIRIS + gods[self.turns.1 % gods.len()] as u16
                }
                MEADOW_FARM => {
                    // A food crop, where the meadow is fertile enough to be worth it.
                    let fertility: i32 = (0..3).flat_map(|d| (0..3).map(move |e| (d, e))).map(|(d, e)| self.world.map.fertility.at_or(bx + dx + d, by + dy + e, 0) as i32).sum::<i32>() / 9;
                    let floodplain = (0..3).any(|d| (0..3).any(|e| self.world.map.terrain_is(bx + dx + d, by + dy + e, terrain::FLOODPLAIN)));
                    if fertility < MEADOW_FERTILITY || floodplain {
                        continue;
                    }
                    meadow += 1;
                    let food: Vec<u16> = FOOD_FARMS.iter().copied().filter(|&k| self.world.is_allowed(k)).collect();
                    let Some(&k) = food.get(meadow as usize % food.len().max(1)) else { continue };
                    k
                }
                k => k,
            };
            self.build(k, bx + dx, by + dy);
        }
    }

    /// The gods the city knows (their temples allowed), by index: temples and shrines
    /// to them keep them content, so they don't curse the city.
    fn gods(&self) -> Vec<usize> {
        let allowed = |g: usize| self.world.is_allowed(kind::TEMPLE_OSIRIS + g as u16);
        let known: Vec<usize> = (0..5).filter(|&g| allowed(g) && self.world.religion.gods.get(g).is_some_and(|x| x.status != 0)).collect();
        if known.is_empty() { (0..5).filter(|&g| allowed(g)).collect() } else { known }
    }

    /// The festival square over the crossing nearest the middle that takes it, and by
    /// each culture block a booth and a bandstand, by each dance block a pavilion,
    /// over a crossing at one of its corners.
    fn venues(&mut self, city: &[(i32, i32, Card, i32)]) {
        let (bw, bh) = (self.sx - 1, self.sy - 1);
        let mut crossings: Vec<(i32, (i32, i32))> = Vec::new();
        let (cx, cy) = ((self.x0 + self.x1) / 2, (self.y0 + self.y1) / 2);
        for y in (self.y0..=self.y1).step_by(self.sy as usize) {
            for x in (self.x0..=self.x1).step_by(self.sx as usize) {
                crossings.push(((x - cx).abs() + (y - cy).abs(), (x, y)));
            }
        }
        crossings.sort();
        if self.world.is_allowed(FESTIVAL_SQUARE) && !crossings.iter().any(|&(_, (x, y))| self.venue_at(FESTIVAL_SQUARE, x, y)) {
            *self.tally.failed.entry(FESTIVAL_SQUARE).or_default() += 1;
        }
        for &(bx, by, card, _) in city {
            let corners = [(bx - 1, by - 1), (bx + bw, by - 1), (bx - 1, by + bh), (bx + bw, by + bh)];
            let wants: &[u16] = if card.as_ptr() == CULTURE.as_ptr() {
                &[kind::BOOTH, kind::BANDSTAND]
            } else if card.as_ptr() == DANCE.as_ptr() {
                &[kind::PAVILION]
            } else {
                &[]
            };
            for &k in wants {
                if self.world.is_allowed(k) && !corners.iter().any(|&(x, y)| self.venue_at(k, x, y)) {
                    *self.tally.failed.entry(k).or_default() += 1;
                }
            }
        }
    }

    /// Builds venue `k` over the crossing at (x, y), at the first spot around it that fits.
    fn venue_at(&mut self, k: u16, x: i32, y: i32) -> bool {
        let s = self.world.size_of(k);
        for vy in y - s + 1..=y {
            for vx in x - s + 1..=x {
                if self.world.can_place(k, vx, vy).is_ok() {
                    return self.build(k, vx, vy);
                }
            }
        }
        false
    }

    /// Waters the meadow farms: each gets a ditch, from a ditch already dug nearby if
    /// there is one, else from a new water lift, one standing in the river if any is
    /// near enough, else one on the floodplain's bank (which fills when the flood is
    /// out).
    fn irrigate(&mut self) {
        let farms: Vec<(i32, i32)> = self.world.buildings.iter().filter(|b| self.world.is_farm(b.kind) && !self.world.map.terrain_is(b.x, b.y, terrain::FLOODPLAIN)).map(|b| (b.x, b.y)).collect();
        let (mut lifts, mut watered) = (0, 0);
        for (fx, fy) in farms {
            // Ends for the ditch: open tiles beside the farm.
            let ends: Vec<(i32, i32)> = (-1..=3).flat_map(|d| [(fx + d, fy - 1), (fx + d, fy + 3), (fx - 1, fy + d), (fx + 3, fy + d)]).filter(|&(x, y)| open(self.world, x, y)).collect();
            let near = |r: i32| (fy - r..=fy + r).flat_map(move |y| (fx - r..=fx + r).map(move |x| (x, y)));
            let dug: Vec<(i32, i32)> = near(12).filter(|&(x, y)| self.world.map.terrain_is(x, y, terrain::CANAL)).collect();
            if dug.iter().any(|&(x, y)| (x - fx - 1).abs() <= 3 && (y - fy - 1).abs() <= 3) {
                watered += 1;
                continue;
            }
            let from_ditch = dug.iter().flat_map(|&d| ends.iter().map(move |&e| (d, e))).find(|&(d, e)| self.world.ditch_path(d, e).is_some_and(|p| p.len() < 20));
            if let Some((d, e)) = from_ditch
                && matches!(self.world.apply(&Command::Build { kind: DITCH, x: d.0, y: d.1, x1: e.0, y1: e.1 }), Outcome::Done { .. })
            {
                *self.tally.built.entry(DITCH).or_default() += 1;
                watered += 1;
                continue;
            }
            // Lift sites nearby, those with the river at their intake first.
            let mut sites: Vec<(bool, i32, (i32, i32))> = Vec::new();
            for (x, y) in near(30) {
                if self.world.can_place(WATER_LIFT, x, y).is_ok()
                    && let Some((facing, _)) = osiris_sim::irrigation::lift_site(&self.world.map, x, y)
                {
                    let dry = !osiris_sim::irrigation::INTAKE[facing as usize].iter().any(|&(dx, dy)| self.world.map.terrain_is(x + dx, y + dy, terrain::WATER));
                    sites.push((dry, (x - fx).abs() + (y - fy).abs(), (x, y)));
                }
            }
            sites.sort();
            let mut done = false;
            for &(_, _, (lx, ly)) in sites.iter().take(16) {
                let Some((facing, _)) = osiris_sim::irrigation::lift_site(&self.world.map, lx, ly) else { continue };
                let outlet = osiris_sim::irrigation::INTAKE[(facing as usize + 2) % 4][0];
                let start = (lx + outlet.0, ly + outlet.1);
                let Some(end) = ends.iter().copied().find(|&e| self.world.ditch_path(start, e).is_some_and(|p| p.len() < 60)) else { continue };
                if self.build(WATER_LIFT, lx, ly) {
                    // A lift hires through a road beside it like any building; one
                    // that can't be given one would stand idle.
                    let behind = osiris_sim::irrigation::INTAKE[(facing as usize + 2) % 4].map(|(dx, dy)| (lx + dx, ly + dy));
                    if !self.has_road(lx, ly, 2, 2) && !self.road_to_lift(lx, ly, &behind) {
                        self.world.apply(&Command::Clear { x0: lx, y0: ly, x1: lx + 1, y1: ly + 1 });
                        *self.tally.built.entry(WATER_LIFT).or_default() -= 1;
                        continue;
                    }
                    lifts += 1;
                    if matches!(self.world.apply(&Command::Build { kind: DITCH, x: start.0, y: start.1, x1: end.0, y1: end.1 }), Outcome::Done { .. }) {
                        *self.tally.built.entry(DITCH).or_default() += 1;
                        watered += 1;
                        done = true;
                        break;
                    }
                }
            }
            if !done {
                *self.tally.failed.entry(DITCH).or_default() += 1;
            }
        }
        self.notes.push(format!("{lifts} water lifts, {watered} meadow farms given a ditch"));
    }
}
