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
    /// The row's two unnamed columns, i and j. Column i is what a mortuary, scribal
    /// school or library uses up per walker, and on the Ptah and Seth complexes the
    /// percentage of that the Oracle of Thoth or Altar of Anubis leaves; column j is
    /// the crime a priest of Ra or Seth takes away.
    #[serde(default)]
    pub i: i32,
    #[serde(default)]
    pub j: i32,
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

#[derive(Clone, serde::Serialize, serde::Deserialize)]
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
    /// The balance tables of every difficulty, when the game has them all, so a
    /// change of difficulty can swap `balance`.
    #[serde(skip)]
    pub balances: Option<Arc<[Arc<Balance>; 5]>>,
    /// 0 Very Easy, 1 Easy, 2 Normal, 3 Hard, 4 Impossible.
    #[serde(default = "crate::difficulty::normal")]
    pub difficulty: u8,
    /// The lowest difficulty played this mission.
    #[serde(default = "crate::difficulty::normal")]
    pub lowest_difficulty: u8,
    pub buildings: Buildings,
    pub figures: Figures,
    pub desirability: Grid<i8>,
    /// Recent service-walker traffic per tile, which steers roamers at junctions.
    #[serde(default)]
    pub traffic: crate::services::Traffic,
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
    /// City warnings (text group 19) waiting to flash on screen, oldest first.
    #[serde(default)]
    pub warnings: VecDeque<u16>,
    /// The message log shown in the messages window.
    #[serde(default)]
    pub notices: crate::notices::Notices,
    /// Regrowth of cut trees and reeds (255 = grown); absent until something is cut.
    #[serde(default)]
    pub vegetation: Option<crate::grid::Grid<u8>>,
    /// The empire's cities and trade routes.
    #[serde(default)]
    pub trade: crate::trade::Trade,
    #[serde(default)]
    pub sentiment_state: crate::sentiment::Sentiment,
    #[serde(default)]
    pub religion: crate::religion::Religion,
    #[serde(default)]
    pub ratings: crate::ratings::Ratings,
    /// The river's entry and exit, fishing grounds and water under buildings.
    #[serde(default)]
    pub water: crate::water::Water,
    pub events: crate::missions::CityEvents,
    /// The monuments the scenario lets the city build, by title (text group 198).
    #[serde(default)]
    pub scenario_monuments: [u16; 3],
    /// Burial provisions the tombs need: (units required, units sent) by resource.
    #[serde(default)]
    pub burial: Vec<(i32, i32)>,
    /// The city's forts and their companies.
    #[serde(default)]
    pub military: crate::military::Military,
    /// This tick's fighters on each side.
    #[serde(skip)]
    pub(crate) combatants: crate::military::Combatants,
    /// Invasions planned, and armies in the field.
    #[serde(default)]
    pub invasions: crate::invasions::Invasions,
    /// The governor's salary, savings and gifts to the Kingdom.
    #[serde(default)]
    pub governor: crate::kingdom::Governor,
    /// What the scenario has planned: requests, gifts, changes in the empire.
    #[serde(default)]
    pub scenario_events: crate::scenario_events::ScenarioEvents,
    /// Texts of the event messages in `messages`, in the same order.
    #[serde(default)]
    pub message_texts: VecDeque<crate::scenario_events::EventText>,
    /// The scenario's earthquakes and its epicentre.
    #[serde(default)]
    pub earthquakes: crate::earthquakes::Earthquakes,
    pub won: bool,
    /// The mission is lost (the city fell, or time ran out).
    #[serde(default)]
    pub lost: bool,
    /// The "Defeat!" message has been shown: the next loss ends the game.
    #[serde(default)]
    pub defeat_shown: bool,
    /// Years the scenario allows to meet its goals, and years to survive to win.
    #[serde(default)]
    pub time_limit: Option<i32>,
    #[serde(default)]
    pub survival: Option<i32>,
    pub migration_params: crate::people::MigrationParams,
    pub scenario_name: String,
    /// 0 humid, 1 normal, 2 arid.
    pub climate: u8,
    /// The scenario's other choice of beast for its climate (asps, lions or
    /// scorpions rather than hippos, crocodiles or hyenas).
    #[serde(default)]
    pub alt_predator: bool,
    /// Where immigrants arrive and emigrants leave, in map coordinates.
    pub entry_point: (i32, i32),
    pub exit_point: (i32, i32),
    /// Where the editor left the camera: the view's top-left corner in diagonal
    /// half-tile units, (x - y, x + y) of the map, or `None` when the map has none.
    #[serde(default)]
    pub start_corner: Option<(i32, i32)>,
    /// The gods a temple complex may be built to (Osiris, Ra, Ptah, Seth, Bast).
    #[serde(default)]
    pub complex_gods: [bool; 5],
    /// What the scenario lets the player build, used when no campaign mission
    /// rules apply (None: everything, as in saves from before it was read).
    #[serde(default)]
    pub scenario_allowed: Option<std::collections::BTreeSet<u16>>,
    /// The scenario file's own goals, used when no campaign mission rules apply.
    #[serde(default)]
    pub scenario_goals: crate::missions::Goals,
    /// Yearly interest on debt, in percent (the scenario's).
    #[serde(default = "default_debt_rate")]
    pub debt_rate: i32,
    /// The scenario has the Kingdom supply the city's grain.
    #[serde(default)]
    pub kingdom_grain: bool,
    /// For scripted tests only: every building gets all the workers it wants.
    #[serde(skip)]
    pub test_full_staff: bool,
    /// The build tool's statue choice: which of the statue's looks (each four
    /// facings), and which facing. Planner state, not part of the city.
    #[serde(skip)]
    pub statue_variant: u8,
    #[serde(skip, default = "default_statue_facing")]
    pub statue_facing: u8,
    /// The build tool's gatehouse facing (0 or 1, R turns it); the original starts
    /// each gatehouse at 1.
    #[serde(skip, default = "default_statue_facing")]
    pub gatehouse_facing: u8,
    /// The build tool's temple complex facing (0 along x, 1 along y; R turns it).
    #[serde(skip)]
    pub complex_facing: u8,
    /// Rotating variant counters of the road/earthquake context tables. Saved so a
    /// reloaded game re-images roads exactly as the running one would.
    #[serde(default)]
    counters: ContextCounters,
    /// The way fire spreads from burning ruins this year (0-7).
    #[serde(default)]
    pub wind: u8,
    /// Frogs, locusts and hail under way.
    #[serde(default)]
    pub plagues: crate::plagues::Plagues,
    /// Dust and sounds for the screen, with the tick each came on (see `effects`).
    #[serde(skip)]
    pub fx: Vec<(u64, crate::effects::Fx)>,
}

fn default_statue_facing() -> u8 {
    1
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
        world.upgrade_monuments();
        world.upgrade_companies();
        world.upgrade_fort_grounds();
        world.upgrade_statues();
        world.upgrade_traders();
        world.upgrade_defenses();
        world.upgrade_palaces();
        Ok(world)
    }
}

impl World {
    pub fn new(scenario: &Scenario, defs: Arc<Defs>, balance: Arc<Balance>) -> Self {
        let info = &scenario.info;
        let mut map = Map::from_scenario(scenario);
        crate::terrain_images::redraw_on_load(&mut map, &defs, scenario.version);
        let (w, h) = (map.width, map.height);
        let water = crate::water::Water::from_scenario(scenario, &map);
        let invasions = crate::invasions::Invasions::from_scenario(scenario, &defs);
        let trade = crate::trade::Trade::from_scenario(scenario);
        let scenario_allowed = Some(crate::missions::scenario_allowed(scenario, &trade));
        Self {
            map,
            time: GameTime::new(info.start_year as i32),
            rng: Rng::from_seed(scenario.random_iv[0], scenario.random_iv[1]),
            rules: Rules::default(),
            treasury: info.initial_funds,
            defs,
            balance,
            balances: None,
            difficulty: crate::difficulty::NORMAL,
            lowest_difficulty: crate::difficulty::NORMAL,
            buildings: Buildings::default(),
            figures: Figures::default(),
            desirability: Grid::new(w, h),
            traffic: Default::default(),
            population: 0,
            sentiment: 60,
            unemployment: 0,
            migration: Default::default(),
            census: Default::default(),
            labor: Default::default(),
            finance: crate::finance::Finance { last_year_balance: info.initial_funds, rescue_loan: info.rescue_loan, ..Default::default() },
            gold_delivered: 0,
            herds: Vec::new(),
            floods: Default::default(),
            mission: None,
            messages: VecDeque::new(),
            warnings: VecDeque::new(),
            notices: Default::default(),
            vegetation: None,
            trade,
            sentiment_state: Default::default(),
            religion: crate::religion::Religion::new(info.gods),
            ratings: crate::ratings::Ratings { milestones: info.win.milestone_years, ..Default::default() },
            water,
            events: Default::default(),
            governor: crate::kingdom::Governor::with_rank(info.player_rank.clamp(0, 10) as u8),
            military: Default::default(),
            combatants: Default::default(),
            invasions,
            scenario_monuments: info.monuments,
            burial: info.burial_provisions_required.iter().map(|&r| (r as i32 * 100, 0)).collect(),
            scenario_events: crate::scenario_events::ScenarioEvents::from_records(&scenario.events, info.start_year as i32),
            message_texts: VecDeque::new(),
            earthquakes: crate::earthquakes::Earthquakes::from_scenario(scenario),
            won: false,
            lost: false,
            defeat_shown: false,
            time_limit: info.win.time_limit.enabled.then_some(info.win.time_limit.value).filter(|&y| y > 0),
            survival: info.win.survival_time.enabled.then_some(info.win.survival_time.value).filter(|&y| y > 0),
            migration_params: Default::default(),
            scenario_name: info.subtitle.clone(),
            climate: info.climate,
            alt_predator: info.alt_predator_type != 0,
            entry_point: (info.entry_point.x, info.entry_point.y),
            start_corner: start_corner(scenario),
            complex_gods: info.temple_complex_gods(),
            scenario_allowed,
            scenario_goals: crate::missions::Goals::from_scenario(&info.win),
            debt_rate: info.debt_interest_rate as i32,
            kingdom_grain: info.kingdom_supplies_grain,
            test_full_staff: false,
            statue_variant: 0,
            statue_facing: 1,
            gatehouse_facing: 1,
            complex_facing: 0,
            exit_point: (info.exit_point.x, info.exit_point.y),
            counters: ContextCounters::default(),
            wind: 0,
            plagues: Default::default(),
            fx: Vec::new(),
        }
    }

    /// Starts the scenario: places the herds from the map's predator and prey points.
    pub fn start(&mut self, scenario: &Scenario) {
        let points = |list: &[osiris_formats::scenario::TilePoint]| list.iter().filter(|p| p.is_valid()).map(|p| (p.x, p.y)).collect::<Vec<_>>();
        self.create_herds(&points(&scenario.info.predator_herd_points), &points(&scenario.info.prey_herd_points));
        self.init_floods(&scenario.floodplain_settings);
    }

    pub fn cost_of(&self, building_type: usize) -> i32 {
        self.balance.stats(building_type as u16).cost
    }

    pub(crate) fn tile_rules(&mut self) -> (TileRules<'_>, &mut Map) {
        (
            TileRules {
                defs: &self.defs,
                counters: &mut self.counters,
                desirability: &self.desirability,
            },
            &mut self.map,
        )
    }

    /// Re-images every road tile, as the original does each month: roads in desirable
    /// areas turn paved (and back), and dirt roads pick up floodplain edges.
    pub(crate) fn update_all_roads(&mut self) {
        let (w, h) = (self.map.width, self.map.height);
        let (mut rules, map) = self.tile_rules();
        rules.roads_in(map, 0, 0, w - 1, h - 1);
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
        if self.out_of_money() {
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
                let (x, y, size) = (b.x, b.y, b.size);
                self.dust(x, y, size);
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
        self.ditch_images_in(x0 - 1, y0 - 1, x1 + 1, y1 + 1);
        Outcome::Done { items, cost }
    }

    /// A road's distance fill spreads over ditches too; the walk back takes only those
    /// it can cross.
    fn road_passable(&self, x: i32, y: i32) -> bool {
        self.map.contains(x, y) && !self.map.terrain_is(x, y, mask::ROAD_BLOCKED & !terrain::CANAL)
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
        const PREFERENCE: [[usize; 4]; 8] = ROUTE_PREFERENCE;
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
            // A ditch is crossed only straight over, with the road laid so far.
            let road = |x: i32, y: i32| self.map.terrain_is(x, y, terrain::ROAD) || path.contains(&(x, y));
            let crossable = |(x, y): (i32, i32)| !self.map.terrain_is(x, y, terrain::CANAL) || self.road_crosses_ditch(x, y, &road);
            if !crossable(cur) {
                return None;
            }
            path.push(cur);
            let Some(dir) = general_direction(cur, start) else {
                return Some(path);
            };
            let road = |x: i32, y: i32| self.map.terrain_is(x, y, terrain::ROAD) || path.contains(&(x, y));
            let next = PREFERENCE[dir].iter().map(|&i| (cur.0 + NEIGHBOURS[i].0, cur.1 + NEIGHBOURS[i].1)).find(|&(x, y)| {
                let nd = at(x, y);
                nd > 0 && nd < d && (!self.map.terrain_is(x, y, terrain::CANAL) || self.road_crosses_ditch(x, y, &road))
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
        if self.out_of_money() {
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
        for &(x, y) in path.iter().filter(|&&(x, y)| self.map.terrain_is(x, y, terrain::CANAL)).collect::<Vec<_>>() {
            self.ditch_images_in(x - 1, y - 1, x + 1, y + 1);
        }
        Outcome::Done { items, cost }
    }
}

/// The neighbours a routed road or ditch tries next when walking back from its end,
/// by the direction toward its start.
pub(crate) const ROUTE_PREFERENCE: [[usize; 4]; 8] = [
    [0, 2, 6, 4],
    [0, 2, 6, 4],
    [2, 4, 0, 6],
    [2, 4, 0, 6],
    [4, 6, 2, 0],
    [4, 6, 2, 0],
    [6, 0, 4, 2],
    [6, 0, 4, 2],
];

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

/// The map's stored camera, a screen tile of the original's 228-tile grid (a column
/// is two half tiles wide, a row half a tile high), turned into the map's own
/// diagonal coordinates. In the original, grid tile (x, y) sits in half-tile column
/// 230 + x - y and row 1 + x + y.
fn start_corner(s: &Scenario) -> Option<(i32, i32)> {
    let [cx, cy] = s.camera;
    if cx <= 0 && cy <= 0 {
        return None;
    }
    let g = osiris_formats::chunks::GRID_SIZE as i32;
    let (x0, y0) = (s.info.start_offset % g, s.info.start_offset / g);
    Some((2 * cx - (g + 2) - x0 + y0, cy - 1 - x0 - y0))
}

fn default_debt_rate() -> i32 {
    10
}
