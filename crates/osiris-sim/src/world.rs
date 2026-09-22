//! The whole simulation state and the commands that change it.

use crate::defs::Defs;
use crate::map::{Map, NEIGHBOURS, mask, terrain};
use crate::rng::Rng;
use crate::rules::Rules;
use crate::tiles::{ContextCounters, TileRules};
use crate::time::GameTime;
use osiris_formats::Scenario;
use std::collections::VecDeque;
use std::sync::Arc;

/// Per-building-type numbers from the difficulty's model file.
#[derive(Debug, Clone, Copy, Default, serde::Serialize, serde::Deserialize)]
pub struct BuildingStats {
    pub cost: i32,
    pub desirability: i32,
    pub des_step: i32,
    pub des_step_size: i32,
    pub des_range: i32,
    pub employees: i32,
    pub fire_risk: i32,
    pub damage_risk: i32,
}

pub const TYPE_ROAD: usize = 5;
pub const TYPE_CLEAR_LAND: usize = 9;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Clears trees, shrubs, rubble, roads and buildings in the rectangle.
    Clear { x0: i32, y0: i32, x1: i32, y1: i32 },
    /// Lays road along the routed path from `start` to `end`.
    Road { start: (i32, i32), end: (i32, i32) },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Done { items: i32, cost: i32 },
    NotEnoughMoney,
    Blocked,
}

pub struct World {
    pub map: Map,
    pub time: GameTime,
    pub rng: Rng,
    pub rules: Rules,
    pub treasury: i32,
    pub defs: Arc<Defs>,
    pub stats: Arc<Vec<BuildingStats>>,
    pub scenario_name: String,
    /// Where immigrants arrive and emigrants leave, in map coordinates.
    pub entry_point: (i32, i32),
    pub exit_point: (i32, i32),
    counters: ContextCounters,
}

impl World {
    pub fn new(scenario: &Scenario, defs: Arc<Defs>, stats: Arc<Vec<BuildingStats>>) -> Self {
        let info = &scenario.info;
        Self {
            map: Map::from_scenario(scenario),
            time: GameTime::new(info.start_year as i32),
            rng: Rng::from_seed(scenario.random_iv[0], scenario.random_iv[1]),
            rules: Rules::default(),
            treasury: info.initial_funds,
            defs,
            stats,
            scenario_name: info.subtitle.clone(),
            entry_point: (info.entry_point.x, info.entry_point.y),
            exit_point: (info.exit_point.x, info.exit_point.y),
            counters: ContextCounters::default(),
        }
    }

    pub fn cost_of(&self, building_type: usize) -> i32 {
        self.stats.get(building_type).map_or(0, |s| s.cost)
    }

    fn tile_rules(&mut self) -> (TileRules<'_>, &mut Map) {
        let desirability = &|_x: i32, _y: i32| 0;
        (
            TileRules {
                defs: &self.defs,
                counters: &mut self.counters,
                desirability,
            },
            &mut self.map,
        )
    }

    /// Runs one simulation tick.
    pub fn tick(&mut self) {
        self.rng.next();
        let _roll = self.time.advance();
    }

    pub fn apply(&mut self, cmd: &Command) -> Outcome {
        match *cmd {
            Command::Clear { x0, y0, x1, y1 } => self.clear(x0, y0, x1, y1, false),
            Command::Road { start, end } => self.road(start, end, false),
        }
    }

    /// What `cmd` would cost, without changing anything.
    pub fn estimate(&mut self, cmd: &Command) -> Outcome {
        match *cmd {
            Command::Clear { x0, y0, x1, y1 } => self.clear(x0, y0, x1, y1, true),
            Command::Road { start, end } => self.road(start, end, true),
        }
    }

    fn clear(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, measure: bool) -> Outcome {
        let (x0, x1) = (x0.min(x1).max(0), x0.max(x1).min(self.map.width - 1));
        let (y0, y1) = (y0.min(y1).max(0), y0.max(y1).min(self.map.height - 1));
        let mut items = 0;
        for y in y0..=y1 {
            for x in x0..=x1 {
                let t = self.map.terrain.at_or(x, y, 0);
                if t & (terrain::ROCK | terrain::ELEVATION | terrain::DUNE) != 0
                    || t & terrain::BUILDING != 0
                    || t & terrain::WATER != 0
                {
                    continue;
                }
                if t & terrain::CANAL != 0 {
                    items += 1;
                    if !measure {
                        self.map.terrain.set(x, y, t & !mask::CLEARABLE);
                    }
                } else if t & mask::NOT_CLEAR != 0 && t & mask::CLEARABLE != 0 {
                    items += 1;
                    if !measure {
                        if t & terrain::ROAD != 0 {
                            self.map.bitfields.update(x, y, |b| b & !0x80);
                        }
                        self.map.terrain.set(x, y, t & !mask::CLEARABLE);
                    }
                }
            }
        }
        let cost = items * self.cost_of(TYPE_CLEAR_LAND);
        if measure || items == 0 {
            return Outcome::Done { items, cost };
        }
        if cost > self.treasury {
            return Outcome::NotEnoughMoney;
        }
        self.treasury -= cost;
        let radius = (x1 - x0).max(y1 - y0) + 3;
        let (mut rules, map) = self.tile_rules();
        rules.empty_land_in(map, x0 - 2, y0 - 2, x1 + 2, y1 + 2, true);
        for y in y0..=y1 {
            for x in x0..=x1 {
                rules.rubble_image(map, x, y);
            }
        }
        rules.roads_in(map, x0 - 1, y0 - 1, x0 + radius - 2, y0 + radius - 2);
        Outcome::Done { items, cost }
    }

    fn road_passable(&self, x: i32, y: i32) -> bool {
        self.map.contains(x, y) && !self.map.terrain_is(x, y, mask::ROAD_BLOCKED)
    }

    /// Breadth-first distances from `start` over tiles a road may cross; 0 = unreachable.
    fn road_distances(&self, start: (i32, i32)) -> Vec<i32> {
        let (w, h) = (self.map.width, self.map.height);
        let mut dist = vec![0i32; (w * h) as usize];
        if !self.road_passable(start.0, start.1) {
            return dist;
        }
        let mut queue = VecDeque::from([start]);
        dist[(start.1 * w + start.0) as usize] = 1;
        while let Some((x, y)) = queue.pop_front() {
            let d = dist[(y * w + x) as usize];
            for i in (0..8).step_by(2) {
                let (nx, ny) = (x + NEIGHBOURS[i].0, y + NEIGHBOURS[i].1);
                if nx < 0 || ny < 0 || nx >= w || ny >= h || !self.road_passable(nx, ny) {
                    continue;
                }
                let n = (ny * w + nx) as usize;
                if dist[n] == 0 {
                    dist[n] = d + 1;
                    queue.push_back((nx, ny));
                }
            }
        }
        dist
    }

    /// The tiles of the routed path from `start` to `end`, walking back from the end
    /// toward the start the way the original does.
    pub fn road_path(&self, start: (i32, i32), end: (i32, i32)) -> Option<Vec<(i32, i32)>> {
        const PREFERENCE: [[usize; 4]; 8] = [
            [0, 2, 6, 4],
            [0, 2, 6, 4],
            [2, 4, 0, 6],
            [2, 4, 0, 6],
            [4, 6, 2, 0],
            [4, 6, 2, 0],
            [6, 0, 4, 2],
            [6, 0, 4, 2],
        ];
        let w = self.map.width;
        let dist = self.road_distances(start);
        let at = |x: i32, y: i32| {
            if self.map.contains(x, y) { dist[(y * w + x) as usize] } else { 0 }
        };
        let mut cur = end;
        let mut path = Vec::new();
        for _ in 0..400 {
            let d = at(cur.0, cur.1);
            if d <= 0 {
                return None;
            }
            path.push(cur);
            let Some(dir) = general_direction(cur, start) else {
                return Some(path);
            };
            let next = PREFERENCE[dir].iter().map(|&i| (cur.0 + NEIGHBOURS[i].0, cur.1 + NEIGHBOURS[i].1)).find(|&(x, y)| {
                let nd = at(x, y);
                nd > 0 && nd < d
            })?;
            cur = next;
        }
        None
    }

    fn road(&mut self, start: (i32, i32), end: (i32, i32), measure: bool) -> Outcome {
        let Some(path) = self.road_path(start, end) else {
            return Outcome::Blocked;
        };
        let new: Vec<_> = path
            .iter()
            .copied()
            .filter(|&(x, y)| !self.map.terrain_is(x, y, terrain::ROAD))
            .collect();
        let items = new.len() as i32;
        let cost = items * self.cost_of(TYPE_ROAD);
        if measure {
            return Outcome::Done { items, cost };
        }
        if cost > self.treasury {
            return Outcome::NotEnoughMoney;
        }
        self.treasury -= cost;
        for &(x, y) in &new {
            self.map.terrain.update(x, y, |t| t | terrain::ROAD);
            self.map.bitfields.update(x, y, |b| b & !0x10);
        }
        let (mut rules, map) = self.tile_rules();
        for &(x, y) in &path {
            rules.roads_in(map, x - 1, y - 1, x + 1, y + 1);
        }
        for &(x, y) in &path {
            rules.empty_land_in(map, x - 4, y - 4, x + 4, y + 4, false);
        }
        Outcome::Done { items, cost }
    }
}

/// The original's coarse 8-way direction from `from` toward `to`, by sign only.
pub fn general_direction(from: (i32, i32), to: (i32, i32)) -> Option<usize> {
    use std::cmp::Ordering::*;
    match (from.0.cmp(&to.0), from.1.cmp(&to.1)) {
        (Less, Greater) => Some(1),
        (Less, Equal) => Some(2),
        (Less, Less) => Some(3),
        (Equal, Greater) => Some(0),
        (Equal, Less) => Some(4),
        (Greater, Greater) => Some(7),
        (Greater, Equal) => Some(6),
        (Greater, Less) => Some(5),
        (Equal, Equal) => None,
    }
}
