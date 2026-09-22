//! The whole simulation state and the commands that change it.

use crate::balance::Balance;
use crate::buildings::{BuildingId, Buildings};
use crate::defs::Defs;
use crate::figures::Figures;
use crate::grid::Grid;
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
    /// Places building type `kind` with its footprint's corner at `(x, y)`; houses
    /// fill the rectangle up to `(x1, y1)` with vacant lots.
    Build { kind: u16, x: i32, y: i32, x1: i32, y1: i32 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Done { items: i32, cost: i32 },
    NotEnoughMoney,
    Blocked,
    /// Placement rule failed, with a short reason for the player.
    Invalid(&'static str),
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct World {
    pub map: Map,
    pub time: GameTime,
    pub rng: Rng,
    pub rules: Rules,
    pub treasury: i32,
    /// Definitions and balance tables aren't saved; `load` reattaches them.
    #[serde(skip)]
    pub defs: Arc<Defs>,
    #[serde(skip)]
    pub balance: Arc<Balance>,
    pub buildings: Buildings,
    pub figures: Figures,
    pub desirability: Grid<i8>,
    pub population: i32,
    /// 0..=100; how content the citizens are.
    pub sentiment: i32,
    /// Percentage of workers without a job.
    pub unemployment: i32,
    pub migration: crate::people::Migration,
    pub census: crate::census::Census,
    pub labor: crate::labor::Labor,
    pub finance: crate::finance::Finance,
    /// Gold delivered to the palace since the scenario began.
    pub gold_delivered: i32,
    pub herds: Vec<crate::animals::Herd>,
    pub floods: crate::floods::Floods,
    pub mission: Option<crate::missions::Mission>,
    /// Message keys waiting to be shown to the player, oldest first.
    pub messages: VecDeque<String>,
    /// The message log shown in the messages window.
    #[serde(default)]
    pub notices: crate::notices::Notices,
    pub events: crate::missions::CityEvents,
    pub won: bool,
    pub migration_params: crate::people::MigrationParams,
    pub scenario_name: String,
    /// 0 central, 1 northern, 2 desert.
    pub climate: u8,
    /// Where immigrants arrive and emigrants leave, in map coordinates.
    pub entry_point: (i32, i32),
    pub exit_point: (i32, i32),
    #[serde(skip)]
    counters: ContextCounters,
}

/// Leading bytes of a saved game, followed by a format version.
const SAVE_MAGIC: &[u8; 8] = b"OSIRIS\0\0";
const SAVE_VERSION: u32 = 1;

impl World {
    /// Serialises the whole simulation.
    pub fn save(&self) -> Result<Vec<u8>, String> {
        let mut out = SAVE_MAGIC.to_vec();
        out.extend_from_slice(&SAVE_VERSION.to_le_bytes());
        let body = rmp_serde::to_vec_named(self).map_err(|e| e.to_string())?;
        out.extend_from_slice(&body);
        Ok(out)
    }

    /// Restores a saved game, reattaching the static definitions.
    pub fn load(data: &[u8], defs: Arc<Defs>, balance: Arc<Balance>) -> Result<Self, String> {
        if data.len() < 12 || &data[..8] != SAVE_MAGIC {
            return Err("not an Osiris saved game".into());
        }
        let version = u32::from_le_bytes(data[8..12].try_into().unwrap());
        if version != SAVE_VERSION {
            return Err(format!("saved game version {version} is not supported"));
        }
        let mut world: World = rmp_serde::from_slice(&data[12..]).map_err(|e| e.to_string())?;
        world.defs = defs;
        world.balance = balance;
        Ok(world)
    }
}

impl World {
    pub fn new(scenario: &Scenario, defs: Arc<Defs>, balance: Arc<Balance>) -> Self {
        let info = &scenario.info;
        let map = Map::from_scenario(scenario);
        let (w, h) = (map.width, map.height);
        Self {
            map,
            time: GameTime::new(info.start_year as i32),
            rng: Rng::from_seed(scenario.random_iv[0], scenario.random_iv[1]),
            rules: Rules::default(),
            treasury: info.initial_funds,
            defs,
            balance,
            buildings: Buildings::default(),
            figures: Figures::default(),
            desirability: Grid::new(w, h),
            population: 0,
            sentiment: 60,
            unemployment: 0,
            migration: Default::default(),
            census: Default::default(),
            labor: Default::default(),
            finance: Default::default(),
            gold_delivered: 0,
            herds: Vec::new(),
            floods: Default::default(),
            mission: None,
            messages: VecDeque::new(),
            notices: Default::default(),
            events: Default::default(),
            won: false,
            migration_params: Default::default(),
            scenario_name: info.subtitle.clone(),
            climate: info.climate,
            entry_point: (info.entry_point.x, info.entry_point.y),
            exit_point: (info.exit_point.x, info.exit_point.y),
            counters: ContextCounters::default(),
        }
    }

    /// Starts the scenario: places the herds from the map's prey points.
    pub fn start(&mut self, scenario: &Scenario) {
        let points: Vec<(i32, i32, i32)> = scenario
            .info
            .prey_herd_points
            .iter()
            .filter(|p| p.is_valid())
            .map(|p| (p.x, p.y, 0))
            .collect();
        self.create_herds(&points);
        self.init_floods(&scenario.floodplain_settings);
    }

    pub fn cost_of(&self, building_type: usize) -> i32 {
        self.balance.stats(building_type as u16).cost
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
        self.run_tick();
    }

    pub fn apply(&mut self, cmd: &Command) -> Outcome {
        match *cmd {
            Command::Clear { x0, y0, x1, y1 } => self.clear(x0, y0, x1, y1, false),
            Command::Road { start, end } => self.road(start, end, false),
            Command::Build { kind, x, y, x1, y1 } => self.build(kind, x, y, x1, y1, false),
        }
    }

    /// What `cmd` would cost, without changing anything.
    pub fn estimate(&mut self, cmd: &Command) -> Outcome {
        match *cmd {
            Command::Clear { x0, y0, x1, y1 } => self.clear(x0, y0, x1, y1, true),
            Command::Road { start, end } => self.road(start, end, true),
            Command::Build { kind, x, y, x1, y1 } => self.build(kind, x, y, x1, y1, true),
        }
    }

    pub(crate) fn clear(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, measure: bool) -> Outcome {
        let (x0, x1) = (x0.min(x1).max(0), x0.max(x1).min(self.map.width - 1));
        let (y0, y1) = (y0.min(y1).max(0), y0.max(y1).min(self.map.height - 1));
        let mut items = 0;
        let mut doomed: Vec<BuildingId> = Vec::new();
        for y in y0..=y1 {
            for x in x0..=x1 {
                let t = self.map.terrain.at_or(x, y, 0);
                if t & terrain::BUILDING != 0 {
                    let id = self.map.building.at_or(x, y, 0);
                    if id != 0 && !doomed.contains(&id) {
                        doomed.push(id);
                        items += 1;
                    }
                    continue;
                }
                if t & (terrain::ROCK | terrain::ELEVATION | terrain::DUNE) != 0 || t & terrain::WATER != 0 {
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
        let (mut bx0, mut by0, mut bx1, mut by1) = (x0, y0, x1, y1);
        for id in doomed {
            if let Some(b) = self.buildings.get(id) {
                bx0 = bx0.min(b.x);
                by0 = by0.min(b.y);
                bx1 = bx1.max(b.x + b.size - 1);
                by1 = by1.max(b.y + b.size - 1);
            }
            self.demolish(id);
        }
        let (x0, y0, x1, y1) = (bx0, by0, bx1, by1);
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
