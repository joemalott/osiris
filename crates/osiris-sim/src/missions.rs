//! Campaign mission rules: which buildings may be built, tutorial unlock steps, win
//! conditions and the messages shown along the way (data in `missions.toml`).

use crate::buildings::kind;
use crate::world::World;
use serde::Deserialize;
use std::collections::BTreeSet;

#[derive(Debug, Clone, Deserialize)]
struct RawMission {
    id: i32,
    #[serde(default)]
    name: String,
    #[serde(default)]
    buildings: Vec<String>,
    #[serde(default)]
    funds: toml::Table,
    #[serde(default)]
    win: toml::Table,
    #[serde(default)]
    vars: toml::Table,
    #[serde(default)]
    unlocks: Vec<RawUnlock>,
}

#[derive(Debug, Clone, Deserialize)]
struct RawUnlock {
    when: String,
    #[serde(default)]
    enable: Vec<String>,
    #[serde(default)]
    message: Option<String>,
}

#[derive(Deserialize)]
struct MissionsFile {
    mission: Vec<RawMission>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Condition {
    Start,
    Fire,
    Collapse,
    Disease,
    /// Crime breaks out; no mission waits for it.
    Crime,
    Population(i32),
    Stored(u16, i32),
    Built(u16),
    GoldDelivered(i32),
    /// A condition this engine doesn't model yet; never fires.
    Unknown(String),
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Unlock {
    pub when: Condition,
    pub enable: Vec<u16>,
    pub message: Option<String>,
    pub done: bool,
}

#[derive(Debug, Clone, Copy, Default, serde::Serialize, serde::Deserialize)]
pub struct Goal {
    pub enabled: bool,
    pub value: i32,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Goals {
    pub population: Goal,
    pub housing_count: Goal,
    /// The house level (0 = crude hut) that many houses must reach.
    pub housing_level: Goal,
    pub culture: Goal,
    pub prosperity: Goal,
    pub monuments: Goal,
    pub kingdom: Goal,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Mission {
    pub id: i32,
    pub name: String,
    pub allowed: BTreeSet<u16>,
    pub unlocks: Vec<Unlock>,
    pub goals: Goals,
    pub population_cap: Option<i32>,
    pub start_message: Option<String>,
    pub religion_enabled: bool,
    /// The rank Pharaoh has given the governor, which sets the salary he may draw.
    #[serde(default)]
    pub player_rank: u8,
}

/// Always available, whatever the mission says.
const ALWAYS: [u16; 3] = [kind::ROAD, kind::VACANT_LOT, kind::WELL];
/// Buildings the original never offers in any menu (Akhenaten additions): the bull
/// trainer, the tower gatehouse, the dike, the food mill and the industry office.
const NEVER_BUILT: [u16; 5] = [37, 302, 337, 360, 361];
/// The palaces and the mansions, for ranks 0-5, 6-7 and 8-10.
const RANKED: [[u16; 3]; 2] = [kind::PALACES, [77, 78, 79]];

fn goal(t: &toml::Table, key: &str) -> Goal {
    let Some(g) = t.get(key).and_then(|v| v.as_table()) else { return Goal::default() };
    Goal {
        enabled: g.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false),
        value: g.get("goal").or_else(|| g.get("years")).and_then(|v| v.as_integer()).unwrap_or(0) as i32,
    }
}

/// Earlier Osiris builds' village and town palaces (types 84 and 85, unused entries in
/// the original), and the original's types they are now.
const OLD_PALACES: [(u16, u16); 2] = [(84, kind::VILLAGE_PALACE), (85, kind::TOWN_PALACE)];

/// Building types of earlier Osiris builds that loading a game turns into others
/// (palaces, walls, towers, gatehouses): never to be offered, or a city allowed
/// them wouldn't come back from a save the same.
pub fn is_obsolete_kind(k: u16) -> bool {
    use crate::defenses::{OLD_GATEHOUSE, OLD_TOWER, OLD_WALL};
    OLD_PALACES.iter().any(|p| p.0 == k) || [OLD_WALL, OLD_TOWER, OLD_GATEHOUSE].contains(&k)
}

impl World {
    /// A saved game's palaces of the old types become the original's, in the city and
    /// in the lists of what may be built.
    pub(crate) fn upgrade_palaces(&mut self) {
        let new = |k: u16| OLD_PALACES.iter().find(|p| p.0 == k).map_or(k, |p| p.1);
        for b in self.buildings.iter_mut() {
            b.kind = new(b.kind);
        }
        let swap = |set: &mut BTreeSet<u16>| *set = set.iter().map(|&k| new(k)).collect();
        if let Some(m) = &mut self.mission {
            swap(&mut m.allowed);
            for k in m.unlocks.iter_mut().flat_map(|u| u.enable.iter_mut()) {
                *k = new(*k);
            }
        }
        if let Some(set) = &mut self.scenario_allowed {
            swap(set);
        }
    }

    fn building_kind(&self, key: &str) -> Option<u16> {
        self.defs.building_by_key(key).map(|b| b.id)
    }

    fn parse_condition(&self, s: &str) -> Condition {
        let s = s.trim();
        let num = |t: &str| t.trim().parse::<i32>().ok();
        match s {
            "start" => return Condition::Start,
            "fire" => return Condition::Fire,
            "collapse" => return Condition::Collapse,
            "disease" => return Condition::Disease,
            _ => {}
        }
        if let Some(n) = s.strip_prefix("population >=").and_then(num) {
            return Condition::Population(n);
        }
        if let Some(n) = s.strip_prefix("gold_delivered >=").and_then(num) {
            return Condition::GoldDelivered(n);
        }
        if let Some(rest) = s.strip_prefix("resource_stored:")
            && let Some((res, n)) = rest.split_once(">=")
            && let (Some(r), Some(n)) = (self.resource_id(res.trim()), num(n))
        {
            return Condition::Stored(r, n);
        }
        if let Some(k) = s.strip_prefix("building_built:").and_then(|k| self.building_kind(k.trim())) {
            return Condition::Built(k);
        }
        Condition::Unknown(s.to_owned())
    }

    /// Loads mission `id`'s rules; missions without data allow everything.
    pub fn load_mission(&mut self, id: i32) {
        let file: MissionsFile =
            toml::from_str(include_str!("../data/missions.toml")).expect("missions.toml is valid");
        let Some(raw) = file.mission.into_iter().find(|m| m.id == id) else {
            self.mission = None;
            return;
        };
        let mut allowed: BTreeSet<u16> = raw.buildings.iter().filter_map(|k| self.building_kind(k)).collect();
        allowed.extend(ALWAYS);
        let unlocks: Vec<Unlock> = raw
            .unlocks
            .iter()
            .map(|u| Unlock {
                when: self.parse_condition(&u.when),
                enable: u.enable.iter().filter_map(|k| self.building_kind(k)).collect(),
                message: u.message.clone(),
                done: false,
            })
            .collect();
        let mission = Mission {
            id,
            name: raw.name,
            allowed,
            unlocks,
            goals: Goals {
                population: goal(&raw.win, "population"),
                housing_count: goal(&raw.win, "housing_count"),
                housing_level: goal(&raw.win, "housing_level"),
                culture: goal(&raw.win, "culture"),
                prosperity: goal(&raw.win, "prosperity"),
                monuments: goal(&raw.win, "monuments"),
                kingdom: goal(&raw.win, "kingdom"),
            },
            population_cap: raw.vars.get("population_cap").and_then(|v| v.as_integer()).map(|v| v as i32),
            start_message: raw.funds.get("start_message").and_then(|v| v.as_str()).map(str::to_owned),
            religion_enabled: raw.funds.get("religion_enabled").and_then(|v| v.as_bool()).unwrap_or(true),
            player_rank: raw.funds.get("player_rank").and_then(|v| v.as_integer()).unwrap_or(0) as u8,
        };
        if let Some(m) = &mission.start_message {
            self.messages.push_back(m.clone());
        }
        self.mission = Some(mission);
        self.check_unlocks();
    }

    pub fn is_allowed(&self, k: u16) -> bool {
        if NEVER_BUILT.contains(&k) {
            return false;
        }
        // Temple complexes follow the scenario's gods, and their upgrades the complex.
        if let Some(ok) = self.complex_allowed(k) {
            return ok;
        }
        let monument = self.defs.building(k).is_some_and(|d| d.has_flag("is_monument"));
        let lists = |k: u16| match (&self.mission, &self.scenario_allowed) {
            (Some(m), _) => m.allowed.contains(&k),
            // The editor's structure flags don't cover monuments: a custom scenario
            // offers the ones it names.
            (None, Some(_)) if monument => true,
            (None, Some(allowed)) => allowed.contains(&k),
            (None, None) => true,
        };
        // The palace and the mansion come in three sizes: the governor's rank picks
        // the one offered.
        if let Some((family, tier)) = RANKED.iter().find_map(|f| f.iter().position(|&x| x == k).map(|t| (f, t))) {
            let rank = self.assigned_rank();
            let fits = match tier {
                0 => rank <= 5,
                1 => (6..=7).contains(&rank),
                _ => (8..=10).contains(&rank),
            };
            return fits && family.iter().any(|&x| lists(x));
        }
        lists(k) && self.monument_allowed(k)
    }

    /// Only the monuments the scenario names (by their text-198 title) may be built,
    /// each as many times as it is named: a player builds at most the scenario's three
    /// (Mission Editor Guide, "Number of Monuments").
    fn monument_allowed(&self, k: u16) -> bool {
        let Some(def) = self.defs.building(k).filter(|d| d.has_flag("is_monument")) else { return true };
        let Some(title) = def.extra.get("info_title_id").and_then(|v| v.as_array()).and_then(|a| a.get(1)).and_then(|v| v.as_integer()) else { return true };
        // The three mausoleum entries are one building.
        let named = |m: u16| m as i64 == title || (title == 25 && matches!(m, 26 | 27));
        let listed = self.scenario_monuments.iter().filter(|&&m| m > 0 && named(m)).count();
        let built = self.buildings.iter().filter(|b| b.kind == k && b.monument.is_some()).count();
        built < listed
    }

    fn condition_met(&self, c: &Condition) -> bool {
        match c {
            Condition::Start => true,
            Condition::Fire => self.events.fire,
            Condition::Collapse => self.events.collapse,
            Condition::Disease => self.events.disease,
            Condition::Population(n) => self.population >= *n,
            Condition::Stored(r, n) => {
                let total: i32 = self.buildings.iter().map(|b| b.stock.get(*r as usize).copied().unwrap_or(0)).sum();
                total >= *n
            }
            Condition::Built(k) => self.buildings.iter().any(|b| b.kind == *k && b.workers > 0),
            Condition::GoldDelivered(n) => self.gold_delivered >= *n,
            Condition::Crime | Condition::Unknown(_) => false,
        }
    }

    /// Daily: fire any tutorial steps whose condition is now met.
    pub(crate) fn check_unlocks(&mut self) {
        let Some(mut m) = self.mission.take() else { return };
        for u in m.unlocks.iter_mut().filter(|u| !u.done) {
            if self.condition_met(&u.when) {
                u.done = true;
                m.allowed.extend(u.enable.iter().copied());
                if let Some(msg) = &u.message {
                    self.messages.push_back(msg.clone());
                }
            }
        }
        self.mission = Some(m);
    }

    /// The goals to meet: the campaign mission's, else the scenario's own.
    pub fn goals(&self) -> Goals {
        self.mission.as_ref().map_or_else(|| self.scenario_goals.clone(), |m| m.goals.clone())
    }

    /// Whether every goal is met (false when there are none). The housing goal counts
    /// whenever it asks for any houses, whatever its flags say: that many occupied
    /// houses at the level or better.
    pub fn goals_met(&self) -> bool {
        let g = self.goals();
        let housing = g.housing_count.value != 0;
        let any = [g.population, g.culture, g.prosperity, g.monuments, g.kingdom].iter().any(|g| g.enabled) || housing;
        if !any {
            return false;
        }
        let r = &self.ratings;
        for (goal, value) in [(g.culture, r.culture), (g.prosperity, r.prosperity), (g.monuments, r.monument), (g.kingdom, r.kingdom)] {
            if goal.enabled && value < goal.value {
                return false;
            }
        }
        if g.population.enabled && self.population < g.population.value {
            return false;
        }
        // The monument goal also wants every burial provision delivered.
        if g.monuments.enabled && !self.burial_complete() {
            return false;
        }
        if housing {
            let min_level = g.housing_level.value.max(0) as u8;
            let count = self
                .buildings
                .iter()
                .filter(|b| b.house.as_ref().is_some_and(|h| h.population > 0 && h.level >= min_level))
                .count() as i32;
            if count < g.housing_count.value {
                return false;
            }
        }
        true
    }

    /// The year the time limit, or else the survival time, runs out (years after
    /// the start). A time limit gets 7 more years on Very Easy and 2 on Easy (the
    /// lowest difficulty played).
    pub fn deadline(&self) -> Option<i32> {
        match (self.time_limit, self.survival) {
            (Some(n), _) => Some(n + time_limit_grace(self.lowest_difficulty as usize)),
            (None, s) => s,
        }
    }

    /// "Pharaohs Tomb": wins the mission on the spot, the same way meeting every goal
    /// does (see [`Self::check_victory`]).
    pub fn win_now(&mut self) {
        if self.won || self.lost {
            return;
        }
        self.won = true;
        self.messages.push_back("victory".to_owned());
    }

    /// Monthly: the original's verdict on the mission. Meeting the goals wins, except
    /// that a survival scenario is only judged when its time is up: won if the goals
    /// are met then, lost if not (or if it also has a time limit). A time limit
    /// running out loses.
    pub(crate) fn check_victory(&mut self) {
        if self.won || self.lost {
            return;
        }
        let years = self.time.year - self.scenario_events.start_year;
        let up = self.deadline().is_some_and(|d| years >= d);
        let met = self.goals_met();
        let verdict = if self.survival.is_some() {
            up.then_some(met && self.time_limit.is_none())
        } else if met {
            Some(true)
        } else {
            up.then_some(false)
        };
        match verdict {
            Some(true) => {
                self.won = true;
                self.messages.push_back("victory".to_owned());
            }
            Some(false) => self.lose(false),
            None => {}
        }
    }

    /// The mission is lost, as the original takes it: the first time it only shows
    /// "Defeat!" (message 311) and the game ends when next judged, except that a
    /// time limit running out on Easy or harder ends it at once. `fell`: the city fell
    /// to invaders rather than running out of time.
    pub(crate) fn lose(&mut self, fell: bool) {
        if self.won || self.lost {
            return;
        }
        if self.defeat_shown || self.out_of_time_at_once(fell) {
            self.lost = true;
        } else {
            self.defeat_shown = true;
            self.messages.push_back("message_mission_defeat".to_owned());
        }
    }

    fn out_of_time_at_once(&self, fell: bool) -> bool {
        !fell && self.time_limit.is_some() && self.lowest_difficulty >= crate::difficulty::EASY
    }

    /// Whether the game ended with time run out (the original's "Out of Time!" screen,
    /// which offers a lower difficulty) rather than in plain defeat.
    pub fn lost_to_time(&self) -> bool {
        self.lost && !self.invasions.lost && self.out_of_time_at_once(false)
    }

    /// Each tick: once "Defeat!" has been read the game ends (the original judges the
    /// mission every tick, and the city stands still while the message is open).
    pub(crate) fn settle_defeat(&mut self) {
        if self.defeat_shown && !self.lost && !self.won {
            self.lost = true;
        }
    }
}

/// Extra years a time limit allows by difficulty (0 Very Easy .. 4 Impossible).
pub fn time_limit_grace(difficulty: usize) -> i32 {
    match difficulty {
        0 => 7,
        1 => 2,
        _ => 0,
    }
}

impl Goals {
    /// The goals a scenario file sets.
    pub fn from_scenario(w: &osiris_formats::scenario::WinCriteria) -> Self {
        let g = |g: &osiris_formats::scenario::Goal| Goal { enabled: g.enabled, value: g.value };
        Goals {
            population: g(&w.population),
            housing_count: g(&w.housing_count),
            housing_level: g(&w.housing_level),
            culture: g(&w.culture),
            prosperity: g(&w.prosperity),
            monuments: g(&w.monuments),
            kingdom: g(&w.kingdom),
        }
    }
}

/// One-off city events that tutorial steps react to.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct CityEvents {
    pub fire: bool,
    pub collapse: bool,
    pub disease: bool,
}

/// The `Pharaoh_MM.eng` entry for a message key used by mission rules.
pub fn message_id(key: &str) -> Option<u32> {
    use std::sync::OnceLock;
    static KEYS: OnceLock<toml::Table> = OnceLock::new();
    let keys = KEYS.get_or_init(|| toml::from_str(include_str!("../data/message_keys.toml")).expect("valid"));
    keys.get(key).and_then(|v| v.as_integer()).map(|v| v as u32)
}

/// The materials each monument (by its text-198 title) is built from, which decide
/// the guilds a scenario offers.
const MONUMENT_MATERIALS: [[u16; 3]; 38] = [
    [0, 0, 0],
    [24, 25, 20], [24, 25, 20],
    [12, 25, 20], [12, 25, 20], [12, 25, 20], [12, 25, 20], [12, 25, 20],
    [24, 0, 20], [24, 0, 20], [24, 0, 20], [24, 0, 20], [24, 0, 20],
    [24, 25, 20], [24, 25, 20], [24, 25, 20], [24, 25, 20], [24, 25, 20],
    [12, 0, 0], [12, 0, 0], [12, 0, 0],
    [24, 20, 0], [26, 20, 0], [26, 20, 0], [30, 20, 0], [30, 20, 0], [30, 20, 0], [30, 20, 0],
    [35, 20, 0], [35, 29, 20], [35, 26, 20], [30, 25, 20], [30, 25, 20],
    [33, 34, 11], [33, 34, 11], [33, 34, 11], [33, 34, 11],
    [20, 30, 0],
];

/// What a custom scenario lets the player build: the editor's allowed-structures
/// list, the temples of the scenario's gods, farms and raw-material works for what the home city can
/// produce, workshops for what it can produce or import, and the guilds its
/// monuments need.
pub(crate) fn scenario_allowed(scenario: &osiris_formats::Scenario, trade: &crate::trade::Trade) -> BTreeSet<u16> {
    use osiris_formats::empire::{city, object};
    let flag = scenario.allowed_structures();
    let f = |i: usize| flag[i];
    let info = &scenario.info;
    let god = |g: usize| info.gods[g] != 0;
    let complex_god = |g: usize| info.temple_complex_gods()[g];
    let home = scenario.empire.objects.iter().filter(|o| o.in_use && o.kind == object::CITY && o.city_type == city::OURS);
    let home_sells: Vec<u8> = home.flat_map(|o| o.sells.iter().copied()).collect();
    // The home city lists the raw goods the land yields.
    let produce = |r: u16| home_sells.contains(&(r as u8));
    // Some trading city sells it, and its route allows some.
    let import = |r: u16| {
        trade.cities.iter().any(|c| {
            c.trades() && c.sells.get(r as usize).copied().unwrap_or(false) && trade.routes.get(c.route as usize).is_some_and(|rt| rt.limit.get(r as usize).is_some_and(|&l| l > 0))
        })
    };
    // Workshops name their product; its raw good stands in for it.
    let raw = |r: u16| match r {
        15 => 14,
        17 => 16,
        33 => 32,
        19 => 18,
        13 => 11,
        10 => 29,
        28 => 20,
        23 => 22,
        r => r,
    };
    let obtain = |r: u16| import(raw(r)) || produce(raw(r));
    let clay = produce(11) || obtain(11);
    let brick_works = (clay && produce(1)) || (clay && import(9)) || (produce(1) && import(11)) || (import(9) && import(11));
    let needs = |m: u16| {
        info.monuments.iter().filter(|&&t| t > 0).any(|&t| MONUMENT_MATERIALS.get(t as usize).is_some_and(|mats| mats.contains(&m)))
    };
    let mut allowed = BTreeSet::new();
    let mut allow = |ks: &[u16], ok: bool| {
        if ok {
            allowed.extend(ks.iter().copied());
        }
    };
    allow(&(kind::HOUSE_FIRST..=kind::HOUSE_LAST).collect::<Vec<_>>(), true);
    allow(&[kind::ROAD, kind::WELL, kind::ARCHITECT_POST, kind::FIREHOUSE, 55], true);
    allow(&[kind::GOLD_MINE], f(2));
    allow(&[7], f(3));
    allow(&[kind::IRRIGATION_DITCH], f(4));
    allow(&[76], f(5) && produce(7));
    allow(&[kind::SHIPWRIGHT], f(5) || f(43) || f(44));
    allow(&[kind::WORK_CAMP], f(6));
    allow(&[kind::GRANARY], f(7));
    allow(&[kind::BAZAAR], f(8));
    allow(&[kind::STORAGE_YARD], f(9));
    allow(&[75], f(10));
    allow(&[kind::BOOTH, kind::JUGGLER_SCHOOL], f(11));
    allow(&[kind::BANDSTAND, kind::CONSERVATORY], f(12));
    allow(&[kind::PAVILION, kind::DANCE_SCHOOL], f(13));
    allow(&[32], f(14));
    allow(&[209], f(15));
    allow(&[kind::SCRIBAL_SCHOOL], f(16));
    allow(&[kind::LIBRARY], f(17));
    allow(&[kind::WATER_SUPPLY], f(18));
    allow(&[49], f(19));
    allow(&[kind::APOTHECARY], f(20));
    allow(&[206], f(21));
    allow(&[kind::MORTUARY], f(22));
    allow(&[kind::TAX_COLLECTOR], f(23));
    allow(&[184], f(24));
    allow(&kind::PALACES, f(25));
    allow(&[77, 78, 79], f(26));
    allow(&[crate::defenses::ROADBLOCK], f(27));
    allow(&[82], f(28));
    allow(&[136], f(29));
    allow(&[kind::GARDENS], f(30));
    allow(&[38], f(31));
    allow(&[41, 42, 43], f(32));
    allow(&[crate::defenses::WALL], f(33));
    allow(&[crate::defenses::TOWER], f(34));
    allow(&[crate::defenses::GATEHOUSE], f(35));
    allow(&[95], f(36));
    allow(&[45], f(37));
    allow(&[44], f(38));
    allow(&[40], f(39));
    allow(&[94], f(40));
    allow(&[112], f(41));
    allow(&[205], f(42));
    allow(&[182], f(43));
    allow(&[181], f(44));
    allow(&[226], f(45));
    for (g, (temple, shrine)) in [(60, 140), (61, 141), (62, 142), (63, 143), (64, 144)].into_iter().enumerate() {
        allow(&[temple, shrine], god(g));
        allow(&[65 + g as u16], complex_god(g));
    }
    // Farms and raw materials: what the land yields.
    for (k, r) in [(102, 1), (194, 2), (103, 3), (105, 4), (104, 5), (196, 6), (kind::HUNTING_LODGE, 8), (100, 14), (101, 16), (224, 32)] {
        allow(&[k], produce(r));
    }
    for (k, r) in [(kind::CLAY_PIT, 11), (162, 18), (108, 20), (195, 22), (106, 24), (107, 25), (216, 26), (217, 29), (221, 30)] {
        allow(&[k], produce(r));
    }
    // Workshops: what can be had of their raw good.
    for (k, r) in [(114, 11), (110, 15), (111, 16), (kind::JEWELER, 18), (203, 22), (233, 32)] {
        allow(&[k], obtain(r));
    }
    allow(&[232], obtain(31) && obtain(13));
    allow(&[204], brick_works);
    // Guilds: the materials of the scenario's monuments.
    allow(&[kind::CARPENTERS_GUILD], needs(20));
    allow(&[kind::BRICKLAYERS_GUILD], needs(12));
    allow(&[kind::STONEMASONS_GUILD], [24, 25, 26, 30, 34, 35].iter().any(|&m| needs(m)));
    allow(&[231], needs(33) || needs(11));
    allowed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load(edit: impl FnOnce(&mut osiris_formats::Scenario)) -> Option<World> {
        let data = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../PharaohData");
        if !data.is_dir() {
            return None;
        }
        let library = osiris_formats::ImageLibrary::open(&data.join("Data")).expect("open image library");
        let mut scenario = osiris_formats::Scenario::load_map(&data.join("Maps/Sandbox.map")).expect("load map");
        edit(&mut scenario);
        let defs = std::sync::Arc::new(crate::defs::Defs::load(&library).expect("load defs"));
        let model_text = std::fs::read(data.join("Pharaoh_Model_Normal.txt")).expect("read model");
        let model = osiris_formats::Model::parse(&String::from_utf8_lossy(&model_text)).expect("parse model");
        let balance = std::sync::Arc::new(crate::balance::Balance::from_model(&model));
        Some(World::new(&scenario, defs, balance))
    }

    #[test]
    fn custom_maps_follow_the_allowed_structures_list() {
        let Some(world) = load(|s| {
            s.version = 160;
            s.info.reserved[8] = 0;
            s.info.reserved[9] = 1;
            s.info.reserved[25] = 1;
            s.info.player_rank = 7;
        }) else {
            return;
        };
        assert!(world.is_allowed(kind::ROAD));
        assert!(!world.is_allowed(kind::BAZAAR), "bazaar struck off the list");
        assert!(world.is_allowed(kind::STORAGE_YARD));
        // Rank 7 gets the town palace only.
        assert!(!world.is_allowed(kind::VILLAGE_PALACE));
        assert!(world.is_allowed(kind::TOWN_PALACE));
        assert!(!world.is_allowed(kind::CITY_PALACE));
        assert!(!world.is_allowed(84) && !world.is_allowed(85));
        // Never in the original's menus.
        for k in NEVER_BUILT {
            assert!(!world.is_allowed(k));
        }
    }

    #[test]
    fn custom_maps_offer_the_monuments_they_name() {
        // A small mastaba (title 18) and a small stepped pyramid (title 8).
        let Some(world) = load(|s| s.info.monuments = [18, 8, 0]) else { return };
        assert!(world.is_allowed(258));
        assert!(world.is_allowed(319));
        assert!(!world.is_allowed(259), "a medium mastaba isn't named");
    }

    #[test]
    fn palaces_and_mansions_are_the_originals() {
        let Some(mut world) = load(|_| {}) else { return };
        // The original's table: palaces 187-189 of 4, 5 and 6 tiles, mansions of 3, 4
        // and 5; the model prices the palaces at 900, 1000 and 1200.
        let size = |k: u16| world.defs.building(k).map(|d| d.size);
        assert_eq!(kind::PALACES.map(size), [Some(4), Some(5), Some(6)]);
        assert_eq!([77, 78, 79].map(size), [Some(3), Some(4), Some(5)]);
        assert_eq!(kind::PALACES.map(|k| world.balance.stats(k).cost), [900, 1000, 1200]);
        // A game saved with the old palace types gets the original's.
        let old = world.create_building(84, 60, 60);
        world.scenario_allowed = Some([84, 85].into_iter().collect());
        world.upgrade_palaces();
        assert_eq!(world.buildings.get(old).map(|b| b.kind), Some(kind::VILLAGE_PALACE));
        assert_eq!(world.scenario_allowed, Some([kind::VILLAGE_PALACE, kind::TOWN_PALACE].into_iter().collect()));
    }

    #[test]
    fn survival_is_judged_when_the_time_is_up() {
        let Some(mut world) = load(|_| {}) else { return };
        world.mission = None;
        world.scenario_goals = Goals { population: Goal { enabled: true, value: 1 }, ..Default::default() };
        world.survival = Some(3);
        world.population = 10;
        world.check_victory();
        assert!(!world.won && !world.lost, "goals met early do not end a survival scenario");
        world.time.year = world.scenario_events.start_year + 3;
        world.population = 0;
        world.check_victory();
        // Defeat is shown first; the game ends when next judged.
        assert!(!world.lost && world.defeat_shown);
        assert_eq!(world.messages.back().map(String::as_str), Some("message_mission_defeat"));
        world.settle_defeat();
        assert!(world.lost && !world.lost_to_time(), "time up without the goals loses");
    }

    #[test]
    fn a_time_limit_runs_out() {
        let Some(mut world) = load(|_| {}) else { return };
        world.mission = None;
        world.scenario_goals = Goals { population: Goal { enabled: true, value: 5000 }, ..Default::default() };
        world.time_limit = Some(2);
        world.time.year = world.scenario_events.start_year + 1;
        world.check_victory();
        assert!(!world.lost);
        world.time.year += 1;
        world.check_victory();
        // On Normal the game ends at once, with no "Defeat!" first.
        assert!(world.lost && world.lost_to_time() && !world.defeat_shown);
        assert_eq!((time_limit_grace(0), time_limit_grace(1), time_limit_grace(2)), (7, 2, 0));
    }

    #[test]
    fn a_housing_goal_of_no_houses_asks_nothing() {
        let Some(mut world) = load(|_| {}) else { return };
        world.mission = None;
        world.scenario_goals = Goals {
            housing_count: Goal { enabled: true, value: 0 },
            housing_level: Goal { enabled: true, value: 14 },
            population: Goal { enabled: true, value: 0 },
            ..Default::default()
        };
        assert!(world.goals_met());
    }
}
