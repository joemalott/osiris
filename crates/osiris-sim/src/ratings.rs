//! The four ratings the Pharaoh judges a city by, and the mission goals set on them.
//!
//! Culture (monthly) is the worst of eleven services' scores, each read off its
//! coverage. Prosperity moves every three months, by last year's accounts, work,
//! wages, food, trade, housing and tribute, and never above what the city's houses
//! allow. The monument rating (monthly) grows with the square root of the worth of the
//! scenario's monuments built so far and the burial provisions sent. The kingdom
//! rating moves once a year with tribute, profit, salary and progress toward the goals,
//! and with debt, gifts, requests and the gods whenever they happen.

use crate::buildings::kind;
use crate::world::World;

/// Kingdom rating a new city starts with at Normal; `World::begin_at` sets the
/// difficulty's.
const STARTING_KINGDOM: i32 = 50;

/// Culture: for each service, its coverage slot, the scores for coverage at or below
/// the first step, below each next step, and at or above the last, and the steps.
const CULTURE: [(usize, [i32; 7], [i32; 6]); 11] = [
    (JUGGLERS, [5, 15, 25, 40, 55, 75, 100], [0, 20, 40, 60, 80, 100]),
    (MUSICIANS, [10, 20, 30, 45, 60, 80, 100], [0, 20, 40, 60, 80, 100]),
    (DANCERS, [15, 25, 35, 50, 65, 85, 100], [0, 20, 40, 60, 80, 100]),
    (SENET, [50, 55, 65, 75, 85, 95, 100], [0, 20, 40, 60, 80, 100]),
    (ZOO, [70, 75, 80, 85, 90, 95, 100], [0, 20, 40, 60, 80, 100]),
    (RELIGION, [0, 10, 25, 40, 60, 85, 100], [0, 20, 40, 60, 80, 100]),
    (SCHOOLS, [40, 45, 50, 60, 75, 85, 100], [0, 20, 40, 60, 80, 100]),
    (LIBRARIES, [60, 65, 70, 75, 80, 90, 100], [0, 20, 40, 60, 80, 100]),
    (DENTISTS, [15, 20, 25, 35, 50, 65, 100], [0, 20, 40, 60, 80, 90]),
    (PHYSICIANS, [20, 25, 30, 40, 55, 70, 100], [0, 20, 40, 60, 80, 90]),
    (MORTUARIES, [35, 45, 55, 70, 85, 100, 100], [0, 20, 40, 60, 80, 100]),
];
const JUGGLERS: usize = 0;
const MUSICIANS: usize = 1;
const DANCERS: usize = 2;
const SENET: usize = 3;
const ZOO: usize = 4;
const SCHOOLS: usize = 5;
const LIBRARIES: usize = 6;
const RELIGION: usize = 7;
const DENTISTS: usize = 8;
const PHYSICIANS: usize = 10;
const MORTUARIES: usize = 11;
/// Steps for the culture bonus: five points for each service that scores more than
/// thirty below the worst when read against these.
const BONUS_STEPS: [i32; 6] = [1, 20, 40, 60, 80, 100];

/// People each staffed building serves, for coverage.
const BOOTH_SERVES: i32 = 400;
const BANDSTAND_SERVES: i32 = 700;
const PAVILION_SERVES: i32 = 1200;
const SENET_SERVES: i32 = 5000;
const ZOO_SERVES: i32 = 7500;
/// Children (up to thirteen) a school teaches.
pub const SCHOOL_SERVES: i32 = 300;
const LIBRARY_SERVES: i32 = 800;
const ACADEMY_SERVES: i32 = 100;
pub const SCHOOL: u16 = 51;
pub const LIBRARY: u16 = 53;
pub const ACADEMY: u16 = 135;
pub const MORTUARY: u16 = 47;
pub const DENTIST: u16 = 49;
pub const PHYSICIAN: u16 = 206;
const SENET_HOUSE: u16 = 32;
const ZOO_BUILDING: u16 = 226;

/// Worth of each monument toward the rating, by its title (text group 198): the
/// exe's monument table at 0x5d19c8, 16 bytes a title, the worth at +0xc. The royal
/// tombs (33-36) are worth 4, 12, 26 and 48.
const MONUMENT_WORTH: [i32; 38] = [
    0, 4, 10, 4, 12, 26, 48, 80, 3, 9, 20, 36, 60, 4, 12, 26, 48, 80, 2, 3, 11, 10, 1, 3, 4, 5, 5, 5, 8, 12, 10, 5, 5, 4, 12, 26,
    48, 48,
];
/// Burial provisions that count toward the monument rating, and the loads each point
/// takes.
const PROVISION_LOADS: [(u16, i32); 15] = [
    (1, 32),
    (8, 32),
    (10, 16),
    (13, 16),
    (15, 16),
    (17, 16),
    (18, 32),
    (19, 16),
    (20, 32),
    (23, 16),
    (24, 32),
    (25, 32),
    (26, 32),
    (28, 16),
    (30, 32),
];

/// House levels below this are huts and shanties; from `NOBLE_LEVEL` up, manors.
const SHANTY_LEVELS: u8 = 4;
const NOBLE_LEVEL: u8 = 14;
/// Missions from this one on lose a point of kingdom rating every year.
const FIRST_DECLINING_MISSION: i32 = 2;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Ratings {
    pub culture: i32,
    pub prosperity: i32,
    pub monument: i32,
    pub kingdom: i32,
    /// What the city's houses allow prosperity to reach.
    pub prosperity_max: i32,
    /// Coverage percentages, for culture and the overseers.
    pub coverage: Coverage,
    pub tribute_unpaid_years: i32,
    pub tribute_paid_last_year: bool,
    /// Kingdom rating last year, to tell the trend.
    pub last_kingdom: i32,
    /// Luxury goods exported (the original's prosperity doesn't use it).
    pub luxury_exported: i32,
    /// City health, 0-100, and what it is moving toward.
    #[serde(default)]
    pub health: i32,
    #[serde(default)]
    pub health_target: i32,
    /// Population at the end of each month, oldest first (the last 400).
    #[serde(default)]
    pub population_history: Vec<i32>,
    /// Years after the start when the Kingdom checks progress toward the goals
    /// (a quarter, half and three quarters of the way).
    #[serde(default)]
    pub milestones: [i32; 3],
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Coverage {
    /// Jugglers (booths, bandstands and pavilions).
    pub booth: i32,
    /// Musicians (bandstands and pavilions).
    pub bandstand: i32,
    /// Dancers (pavilions).
    pub pavilion: i32,
    pub school: i32,
    pub library: i32,
    pub academy: i32,
    #[serde(default)]
    pub senet: i32,
    #[serde(default)]
    pub zoo: i32,
    /// Share of the people living where each doctor has called.
    #[serde(default)]
    pub apothecary: i32,
    #[serde(default)]
    pub physician: i32,
    #[serde(default)]
    pub dentist: i32,
    #[serde(default)]
    pub mortuary: i32,
}

impl Default for Ratings {
    fn default() -> Self {
        Self {
            culture: 0,
            prosperity: 0,
            monument: 0,
            kingdom: STARTING_KINGDOM,
            prosperity_max: 0,
            coverage: Coverage::default(),
            tribute_unpaid_years: 0,
            tribute_paid_last_year: true,
            last_kingdom: STARTING_KINGDOM,
            luxury_exported: 0,
            health: 50,
            health_target: 50,
            population_history: Vec::new(),
            milestones: [0; 3],
        }
    }
}

impl Ratings {
    pub fn change_kingdom(&mut self, amount: i32) {
        self.kingdom = (self.kingdom + amount).clamp(0, 100);
    }
}

fn percent(served: i32, of: i32) -> i32 {
    if of > 0 { (served * 100 / of).min(100) } else { 0 }
}

/// A service's culture score for its coverage.
fn culture_score(cov: i32, values: &[i32; 7], steps: &[i32; 6]) -> i32 {
    if cov <= steps[0] {
        return values[0];
    }
    steps[1..].iter().position(|&s| cov < s).map_or(values[6], |i| values[i + 1])
}

/// The culture rating for coverages by slot (see `CULTURE`).
fn culture_rating(cov: &[i32; 12]) -> i32 {
    let worst = CULTURE.iter().map(|(slot, values, steps)| culture_score(cov[*slot], values, steps)).fold(100, i32::min);
    let bonus = CULTURE
        .iter()
        .filter(|(slot, values, _)| {
            let c = cov[*slot];
            let score = BONUS_STEPS.iter().position(|&s| c < s).map_or(values[6], |i| values[i]);
            score + 30 < worst
        })
        .count() as i32
        * 5;
    (worst + bonus).clamp(0, 100)
}

/// What the family history keeps of a won mission: the city as it stood, how long it
/// took, and the score.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MissionResult {
    pub culture: i32,
    pub prosperity: i32,
    pub kingdom: i32,
    pub population: i32,
    pub funds: i32,
    pub months: i32,
    pub score: i32,
    /// The lowest difficulty played at, which also scales the score.
    pub difficulty: u8,
}

/// The original's score: the ratings times the population, the worth of the
/// scenario's finished monuments squared over the months taken squared, and the funds
/// per month, scaled by the lowest difficulty played ((level + 1) / 3, so 1 at
/// Normal). The original then takes off a point for each cheat used; Osiris has none.
fn score(ratings: i32, population: i32, monuments: i32, funds: i32, months: i32, difficulty: i32) -> i32 {
    let m = months.max(1) as f64;
    let a = monuments as f64;
    let s = ratings as f64 * (population as f64 * 0.002) + a / (m * m * 0.00015625) * (a * 20.0) + funds as f64 / (m * (1.0 / 30.0));
    (s * ((difficulty as f64 + 1.0) * (1.0 / 3.0))) as i32
}

impl World {
    /// The mission's result as it stands. Months are counted from the start year's
    /// January, as the original does.
    pub fn mission_result(&self) -> MissionResult {
        let months = self.time.month as i32 + (self.scenario_events.start_year - self.time.year).abs() * 12;
        let worth = self.scenario_monument_buildings().into_iter().filter(|&(_, b)| self.monument_percent(b) >= 100).map(|(t, _)| MONUMENT_WORTH.get(t).copied().unwrap_or(0)).sum();
        let r = &self.ratings;
        MissionResult {
            culture: r.culture,
            prosperity: r.prosperity,
            kingdom: r.kingdom,
            population: self.population,
            funds: self.treasury,
            months,
            score: score(r.kingdom + r.prosperity + r.culture, self.population, worth, self.treasury, months, self.lowest_difficulty as i32),
            difficulty: self.lowest_difficulty,
        }
    }
}

/// The monument rating for the worth built (and provisions sent).
fn monument_rating(worth: i32, unfinished: bool) -> i32 {
    let r = (worth.max(0) as f64).sqrt() * 6.32 + 0.5 - if unfinished { 1.0 } else { 0.0 };
    (r.max(0.0) as i32).min(100)
}

impl World {
    fn staffed(&self, k: u16) -> i32 {
        self.buildings.iter().filter(|b| b.kind == k && b.workers > 0).count() as i32
    }

    /// Monthly: coverage, city health and the population history.
    pub(crate) fn update_ratings_month(&mut self) {
        let pop = self.population;
        let school_age: i32 = self.census.at_age[0..14].iter().sum();
        let academy_age: i32 = self.census.at_age[14..21].iter().sum();
        let (booths, bandstands, pavilions) = (self.staffed(kind::BOOTH), self.staffed(kind::BANDSTAND), self.staffed(kind::PAVILION));
        // Share of the people in houses each doctor has called on.
        let (mut apothecary, mut dentist, mut physician, mut mortuary) = (0, 0, 0, 0);
        for h in self.buildings.iter().filter_map(|b| b.house.as_ref()).filter(|h| h.population > 0) {
            let p = h.population;
            apothecary += if h.coverage.apothecary > 0 { p } else { 0 };
            dentist += if h.coverage.dentist > 0 { p } else { 0 };
            physician += if h.coverage.physician > 0 { p } else { 0 };
            mortuary += if h.coverage.mortuary > 0 { p } else { 0 };
        }
        let c = Coverage {
            booth: percent(BOOTH_SERVES * (booths + bandstands + pavilions), pop),
            bandstand: percent(BANDSTAND_SERVES * (bandstands + pavilions), pop),
            pavilion: percent(PAVILION_SERVES * pavilions, pop),
            school: percent(SCHOOL_SERVES * self.staffed(SCHOOL), school_age),
            library: percent(LIBRARY_SERVES * self.staffed(LIBRARY), pop),
            academy: percent(ACADEMY_SERVES * self.staffed(ACADEMY), academy_age),
            senet: percent(SENET_SERVES * self.staffed(SENET_HOUSE), pop),
            zoo: percent(ZOO_SERVES * self.staffed(ZOO_BUILDING), pop),
            apothecary: percent(apothecary, pop),
            physician: percent(physician, pop),
            dentist: percent(dentist, pop),
            mortuary: percent(mortuary, pop),
        };
        self.ratings.coverage = c;
        let history = &mut self.ratings.population_history;
        history.push(self.population);
        if history.len() > 400 {
            history.remove(0);
        }
    }

    /// Monthly, after the accounts (and at the year's end, after the year's): culture,
    /// the kingdom's month or year, the monument rating, the prosperity cap, and every
    /// third month prosperity.
    pub(crate) fn update_ratings(&mut self, year: bool) {
        self.update_culture_rating();
        self.update_kingdom(year);
        self.update_monument_rating();
        self.update_prosperity_max();
        if self.time.month.is_multiple_of(3) {
            self.update_prosperity();
        }
    }

    fn update_culture_rating(&mut self) {
        let c = &self.ratings.coverage;
        let religion = if self.rules.gods_enabled { self.religion.coverage_common } else { 100 };
        let mut cov = [0; 12];
        cov[JUGGLERS] = c.booth;
        cov[MUSICIANS] = c.bandstand;
        cov[DANCERS] = c.pavilion;
        cov[SENET] = c.senet;
        cov[ZOO] = c.zoo;
        cov[SCHOOLS] = c.school;
        cov[LIBRARIES] = c.library;
        cov[RELIGION] = religion;
        cov[DENTISTS] = c.dentist;
        cov[9] = c.apothecary;
        cov[PHYSICIANS] = c.physician;
        cov[MORTUARIES] = c.mortuary;
        self.ratings.culture = culture_rating(&cov);
    }

    /// The prosperity cap: what each occupied house's level allows, averaged over the
    /// people.
    fn update_prosperity_max(&mut self) {
        let (mut sum, mut people) = (0, 0);
        for h in self.buildings.iter().filter_map(|b| b.house.as_ref()).filter(|h| h.population > 0) {
            sum += self.balance.house(h.level).prosperity * h.population;
            people += h.population;
        }
        self.ratings.prosperity_max = if people > 0 { sum / people } else { 0 };
    }

    /// The scenario's monuments (up to three, by title) and the building standing for
    /// each: one of its type not taken by an earlier one. A royal tomb carried from an
    /// earlier Valley mission is none of them.
    fn scenario_monument_buildings(&self) -> Vec<(usize, crate::buildings::BuildingId)> {
        let mut out: Vec<(usize, crate::buildings::BuildingId)> = Vec::new();
        for &t in self.scenario_monuments.iter().filter(|&&t| t > 0) {
            let want = crate::monuments::monument_for_title(t as usize).map(|d| d.title);
            let b = self.buildings.iter().find(|b| {
                b.monument.as_ref().is_some_and(|m| !m.carried) && crate::monuments::monument_def(b.kind).map(|d| d.title) == want && out.iter().all(|&(_, id)| id != b.id)
            });
            if let Some(b) = b {
                out.push((t as usize, b.id));
            }
        }
        out
    }

    /// The monument rating (0x4f78a0): for each of the scenario's three monuments
    /// placed, its worth (by title, 0x5d19c8) times how far it is built, and a point
    /// off while any is unfinished; other monuments standing, such as tombs left by
    /// an earlier mission, count for nothing.
    fn update_monument_rating(&mut self) {
        let mut worth = 0;
        let mut unfinished = false;
        for (t, b) in self.scenario_monument_buildings() {
            let pct = self.monument_percent(b);
            unfinished |= pct < 100;
            worth += MONUMENT_WORTH.get(t).copied().unwrap_or(0) * pct / 100;
        }
        for &(r, loads) in &PROVISION_LOADS {
            if let Some(&(need, sent)) = self.burial.get(r as usize)
                && need > 0
            {
                worth += sent / 100 / loads;
            }
        }
        self.ratings.monument = monument_rating(worth, unfinished);
    }

    /// Every third month: prosperity moves by last year's accounts and trade, work,
    /// food, wages, housing, tribute, a senet house and a finished great monument.
    fn update_prosperity(&mut self) {
        let last = &self.finance.last_year;
        let (income, expenses) = (last.income(), last.expenses());
        let profit = income - expenses;
        let mut change = if profit > 0 {
            5
        } else if profit + last.construction / 2 > 0 {
            3
        } else if profit < 0 {
            -8
        } else {
            0
        };
        change += (last.exports - last.imports).signum();
        change += if self.labor.shortage > 0 {
            -1
        } else {
            match self.unemployment {
                u if u < 5 => 2,
                u if u < 10 => 0,
                u if u < 15 => -1,
                u if u < 20 => -2,
                _ => -3,
            }
        };
        let food_kinds = self.buildings.iter().filter_map(|b| b.house.as_ref()).map(|h| h.foods.iter().filter(|&&f| f > 0).count()).max().unwrap_or(0).max(1);
        if food_kinds < 2 {
            change -= 1;
        }
        // Wages against the Kingdom's; Ra's oracle makes the city's seem two better.
        let ra = if self.complex_blessing(crate::temple_complex::RA, crate::temple_complex::ORACLE) { 2 } else { 0 };
        change += match last.wage_months / 12 - self.finance.kingdom_wages + ra {
            d if d >= 6 => 2,
            d if d >= 1 => 1,
            0 => 0,
            d if d >= -5 => -1,
            d if d >= -10 => -2,
            _ => -3,
        };
        let (mut shanties, mut manors) = (0, 0);
        for h in self.buildings.iter().filter_map(|b| b.house.as_ref()).filter(|h| h.population > 0) {
            if h.level < SHANTY_LEVELS {
                shanties += h.population;
            }
            if h.level >= NOBLE_LEVEL {
                manors += h.population;
            }
        }
        if percent(shanties, self.population) > 95 {
            change -= 1;
        }
        if percent(manors, self.population) > 10 {
            change += 2;
        }
        if !self.ratings.tribute_paid_last_year {
            change -= 1;
        }
        if self.buildings.iter().any(|b| b.kind == SENET_HOUSE && b.workers > 0 && b.shows.iter().any(|&s| s > 0)) {
            change += 1;
        }
        let great = self.buildings.iter().any(|b| {
            b.monument.as_ref().is_some_and(|m| m.finished)
                && crate::monuments::monument_def(b.kind).is_some_and(|d| crate::pyramids::blockwise(d.style) || d.style == crate::monuments::Style::Sphinx)
        });
        if great {
            change += 1;
        }
        let r = &mut self.ratings;
        r.prosperity = (r.prosperity + change).min(r.prosperity_max).clamp(0, 100);
    }

    /// The kingdom's month, and at the year's end its judgement: a point lost every
    /// year after the first missions, tribute owed or a profit made, the governor's
    /// salary, and at the milestones whether the city is on course for its goals.
    fn update_kingdom(&mut self, year: bool) {
        if !year {
            return;
        }
        if self.mission.as_ref().is_none_or(|m| m.id >= FIRST_DECLINING_MISSION) {
            self.ratings.change_kingdom(-1);
        }
        if !self.ratings.tribute_paid_last_year {
            let penalty = match self.ratings.tribute_unpaid_years {
                ..=1 => -3,
                2 => -5,
                _ => -8,
            };
            self.ratings.change_kingdom(penalty);
        } else {
            let last = &self.finance.last_year;
            if last.expenses() + self.finance.this_year.tribute < last.income() {
                self.ratings.change_kingdom(1);
            }
        }
        self.salary_year();
        self.check_milestone();
        self.ratings.last_kingdom = self.ratings.kingdom;
    }

    /// At each milestone year the Kingdom checks that every goal is a quarter, half
    /// or three quarters met: five points if so, two lost if not.
    fn check_milestone(&mut self) {
        let years = self.time.year - self.scenario_events.start_year;
        let Some(i) = self.ratings.milestones.iter().position(|&y| y > 0 && y == years) else { return };
        let pct = [25, 50, 75][i];
        let Some(m) = &self.mission else { return };
        let g = &m.goals;
        let r = &self.ratings;
        let on_course = [(g.culture, r.culture), (g.prosperity, r.prosperity), (g.population, self.population), (g.monuments, r.monument), (g.kingdom, r.kingdom)]
            .iter()
            .all(|(goal, value)| !goal.enabled || *value >= goal.value * pct / 100);
        self.ratings.change_kingdom(if on_course { 5 } else { -2 });
    }

    /// Yearly, after the accounts: tribute to the Kingdom from last year's. A city
    /// with no money pays nothing and falls behind; Pharaoh pays none.
    pub(crate) fn pay_tribute(&mut self) {
        let last = &self.finance.last_year;
        let (income, expenses) = (last.income(), last.expenses());
        let pop = self.population;
        let tribute = if self.assigned_rank() == crate::kingdom::PHARAOH_RANK {
            Some(0)
        } else if self.treasury <= 0 {
            None
        } else if income <= expenses {
            Some(match pop {
                p if p > 2000 => 200,
                p if p > 1000 => 100,
                _ => 0,
            })
        } else {
            let min = match pop {
                p if p <= 500 => 50,
                p if p <= 1000 => 150,
                p if p <= 2000 => 225,
                p if p <= 3000 => 300,
                p if p <= 5000 => 400,
                _ => 500,
            };
            Some(min.max((income - expenses) / 4))
        };
        let r = &mut self.ratings;
        match tribute {
            Some(t) => {
                self.treasury -= t;
                self.finance.this_year.tribute += t;
                r.tribute_paid_last_year = true;
                r.tribute_unpaid_years = 0;
            }
            None => {
                r.tribute_paid_last_year = false;
                r.tribute_unpaid_years += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn culture_is_the_worst_service() {
        // Nothing at all: religion scores nothing.
        assert_eq!(culture_rating(&[0; 12]), 0);
        // Everything perfect.
        assert_eq!(culture_rating(&[100; 12]), 100);
        // All perfect but no zoo: the zoo's 70 is the worst.
        let mut cov = [100; 12];
        cov[ZOO] = 0;
        assert_eq!(culture_rating(&cov), 70);
        // Schools at 50%: 60.
        let mut cov = [100; 12];
        cov[SCHOOLS] = 50;
        assert_eq!(culture_rating(&cov), 60);
        // Dentists step up at 90% rather than 100%.
        assert_eq!(culture_score(95, &CULTURE[8].1, &CULTURE[8].2), 100);
        assert_eq!(culture_score(85, &CULTURE[8].1, &CULTURE[8].2), 65);
    }

    #[test]
    fn score_weighs_ratings_monuments_and_funds_by_time() {
        assert_eq!(score(150, 1000, 0, 1000, 12, 2), 2800);
        // A finished small mudbrick pyramid (worth 4) in three years.
        assert_eq!(score(0, 0, 4, 0, 36, 2), 1580);
        // A third, stored inexactly, leaves Impossible just under five thirds.
        assert_eq!(score(150, 1000, 0, 0, 12, 4), 499);
    }

    #[test]
    fn monument_rating_grows_with_the_root_of_worth() {
        assert_eq!(monument_rating(0, false), 0);
        assert_eq!(monument_rating(80, false), 57);
        assert_eq!(monument_rating(80, true), 56);
        assert_eq!(monument_rating(250, false), 100);
        assert_eq!(MONUMENT_WORTH[7], 80);
        assert_eq!(MONUMENT_WORTH[21], 10);
    }
}
