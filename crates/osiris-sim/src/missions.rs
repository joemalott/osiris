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
    /// 1-based house level (1 = crude hut).
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
    pub initial_funds: Option<i32>,
    pub house_tax_pct: Option<i32>,
    pub religion_enabled: bool,
    /// The rank Pharaoh has given the governor, which sets the salary he may draw.
    #[serde(default)]
    pub player_rank: u8,
}

/// Always available, whatever the mission says.
const ALWAYS: [u16; 3] = [kind::ROAD, kind::VACANT_LOT, kind::WELL];
/// Difficulty column used from per-difficulty tables (Normal).
const DIFFICULTY: usize = 2;

fn goal(t: &toml::Table, key: &str) -> Goal {
    let Some(g) = t.get(key).and_then(|v| v.as_table()) else { return Goal::default() };
    Goal {
        enabled: g.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false),
        value: g.get("goal").or_else(|| g.get("years")).and_then(|v| v.as_integer()).unwrap_or(0) as i32,
    }
}

impl World {
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
        let arr = |t: &toml::Table, k: &str| {
            t.get(k).and_then(|v| v.as_array()).and_then(|a| a.get(DIFFICULTY)).and_then(|v| v.as_integer()).map(|v| v as i32)
        };
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
            initial_funds: arr(&raw.funds, "initial_funds"),
            house_tax_pct: arr(&raw.funds, "house_tax_multipliers"),
            religion_enabled: raw.funds.get("religion_enabled").and_then(|v| v.as_bool()).unwrap_or(true),
            player_rank: raw.funds.get("player_rank").and_then(|v| v.as_integer()).unwrap_or(0) as u8,
        };
        if let Some(f) = mission.initial_funds {
            self.treasury = f;
        }
        if let Some(t) = mission.house_tax_pct {
            self.finance.tax_multiplier_pct = t;
        }
        if let Some(m) = &mission.start_message {
            self.messages.push_back(m.clone());
        }
        self.mission = Some(mission);
        self.check_unlocks();
    }

    pub fn is_allowed(&self, k: u16) -> bool {
        // Temple complexes follow the scenario's gods, and their upgrades the complex.
        if let Some(ok) = self.complex_allowed(k) {
            return ok;
        }
        self.mission.as_ref().is_none_or(|m| m.allowed.contains(&k)) && self.monument_allowed(k)
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

    /// Whether every enabled goal is met.
    pub fn goals_met(&self) -> bool {
        let Some(m) = &self.mission else { return false };
        let g = &m.goals;
        let any = [g.population, g.housing_count, g.housing_level, g.culture, g.prosperity, g.monuments, g.kingdom].iter().any(|g| g.enabled);
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
        if g.housing_count.enabled || g.housing_level.enabled {
            let min_level = (g.housing_level.value.max(1) - 1) as u8;
            let count = self
                .buildings
                .iter()
                .filter(|b| b.house.as_ref().is_some_and(|h| h.population > 0 && h.level >= min_level))
                .count() as i32;
            if count < g.housing_count.value.max(1) {
                return false;
            }
        }
        true
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
