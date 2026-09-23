//! The four ratings the Pharaoh judges a city by, and the mission goals set on them.
//!
//! Culture (monthly) adds points for how much of the city booths, temples, schools,
//! libraries and academies reach. Prosperity moves once a year, up for a profit, work
//! for everyone, fair wages, fine houses and luxury exports, down for the opposite,
//! and never above what the city's houses allow. The monument rating (monthly) counts
//! each monument's worth by how far it has been built. The kingdom rating falls with
//! debt and neglect and rises with success.

use crate::buildings::kind;
use crate::world::World;

/// Kingdom rating a new city starts with (Normal difficulty).
const STARTING_KINGDOM: i32 = 50;

/// Coverage percentage steps and the culture points they give, per source.
const STEPS: [i32; 5] = [100, 85, 70, 50, 30];
const ENTERTAINMENT_POINTS: [i32; 5] = [25, 18, 12, 8, 3];
const RELIGION_POINTS: [i32; 5] = [30, 22, 14, 9, 3];
const SCHOOL_POINTS: [i32; 5] = [15, 10, 6, 4, 1];
const LIBRARY_POINTS: [i32; 5] = [20, 14, 8, 4, 2];
const ACADEMY_POINTS: [i32; 5] = [10, 7, 4, 2, 1];

/// People each building serves, for coverage.
const BOOTH_SERVES: i32 = 400;
const BANDSTAND_SERVES: i32 = 700;
const PAVILION_SERVES: i32 = 1200;
const SCHOOL_SERVES: i32 = 75;
const LIBRARY_SERVES: i32 = 800;
const ACADEMY_SERVES: i32 = 100;
pub const SCHOOL: u16 = 51;
pub const LIBRARY: u16 = 53;
pub const ACADEMY: u16 = 135;
pub const MORTUARY: u16 = 47;
pub const DENTIST: u16 = 49;
pub const PHYSICIAN: u16 = 206;

/// Monument worth by type, and the rating's scale and offset.
const MONUMENT_WEIGHTS: [(u16, i32); 3] = [(kind::SMALL_MASTABA, 2), (kind::MEDIUM_MASTABA, 2), (kind::LARGE_MASTABA, 3)];
const MONUMENT_MULT: f32 = 2.25;
const MONUMENT_OFFSET: f32 = 4.5;

/// Kingdom penalties for years of unpaid tribute (1, 2, 3, 4, 5+).
const TRIBUTE_PENALTY: [i32; 5] = [-3, -3, -5, -8, -8];

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
    /// Treasury (plus construction) at the end of last year, for prosperity.
    pub last_year_worth: i32,
    pub tribute_unpaid_years: i32,
    pub tribute_paid_last_year: bool,
    /// Debt: 0 none yet, 1 bailed out once, 2 and 3 later penalties, 4 capped.
    pub debt_state: u8,
    pub months_in_debt: i32,
    pub kingdom_cap: i32,
    /// Kingdom rating last year, to tell the trend.
    pub last_kingdom: i32,
    pub luxury_exported: i32,
    /// City health, 0-100, and what it is moving toward.
    #[serde(default)]
    pub health: i32,
    #[serde(default)]
    pub health_target: i32,
    /// Population at the end of each month, oldest first (the last 400).
    #[serde(default)]
    pub population_history: Vec<i32>,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Coverage {
    pub booth: i32,
    pub bandstand: i32,
    pub pavilion: i32,
    pub school: i32,
    pub library: i32,
    pub academy: i32,
    #[serde(default)]
    pub senet: i32,
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
            last_year_worth: 0,
            tribute_unpaid_years: 0,
            tribute_paid_last_year: true,
            debt_state: 0,
            months_in_debt: 0,
            kingdom_cap: 100,
            last_kingdom: STARTING_KINGDOM,
            luxury_exported: 0,
            health: 50,
            health_target: 50,
            population_history: Vec::new(),
        }
    }
}

impl Ratings {
    pub fn change_kingdom(&mut self, amount: i32) {
        self.kingdom = (self.kingdom + amount).clamp(0, self.kingdom_cap);
    }
}

fn points(coverage: i32, table: [i32; 5]) -> i32 {
    STEPS.iter().zip(table).find(|(s, _)| coverage >= **s).map_or(0, |(_, p)| p)
}

fn percent(served: i32, of: i32) -> i32 {
    if of > 0 { (served * 100 / of).min(100) } else { 0 }
}

impl World {
    fn staffed(&self, k: u16) -> i32 {
        self.buildings.iter().filter(|b| b.kind == k && b.workers > 0).count() as i32
    }

    /// Monthly: coverage, culture, the prosperity cap, the monument rating, and the
    /// kingdom's month.
    pub(crate) fn update_ratings_month(&mut self) {
        let pop = self.population;
        let school_age: i32 = self.census.at_age[0..14].iter().sum();
        let academy_age: i32 = self.census.at_age[14..21].iter().sum();
        let c = Coverage {
            booth: percent(BOOTH_SERVES * self.staffed(kind::BOOTH), pop),
            bandstand: percent(BANDSTAND_SERVES * self.staffed(kind::BANDSTAND), pop),
            pavilion: percent(PAVILION_SERVES * self.staffed(kind::PAVILION), pop),
            school: percent(SCHOOL_SERVES * self.staffed(SCHOOL), school_age),
            library: percent(LIBRARY_SERVES * self.staffed(LIBRARY), pop),
            academy: percent(ACADEMY_SERVES * self.staffed(ACADEMY), academy_age),
            senet: if self.staffed(32) > 0 { 100 } else { 0 },
            apothecary: percent(100 * self.staffed(kind::APOTHECARY), pop),
            physician: percent(1000 * self.staffed(PHYSICIAN), pop),
            dentist: percent(1000 * self.staffed(DENTIST), pop),
            mortuary: percent(1000 * self.staffed(MORTUARY), pop),
        };
        let religion = if self.rules.gods_enabled { self.religion.coverage_common } else { 100 };
        self.ratings.culture = if pop <= 0 {
            0
        } else {
            (points(c.booth, ENTERTAINMENT_POINTS)
                + points(religion, RELIGION_POINTS)
                + points(c.school, SCHOOL_POINTS)
                + points(c.library, LIBRARY_POINTS)
                + points(c.academy, ACADEMY_POINTS))
            .clamp(0, 100)
        };
        self.ratings.coverage = c;
        self.update_prosperity_max();
        self.update_monument_rating();
        self.update_health();
        let history = &mut self.ratings.population_history;
        history.push(self.population);
        if history.len() > 400 {
            history.remove(0);
        }
        self.update_debt();
        let r = &mut self.ratings;
        r.kingdom = r.kingdom.clamp(0, r.kingdom_cap);
    }

    /// City health: the share of people who have a doctor (an apothecary for huts, a
    /// physician above) and food; health moves toward it two points a month.
    fn update_health(&mut self) {
        let pop = self.population;
        if pop < 200 {
            self.ratings.health = 50;
            self.ratings.health_target = 50;
            return;
        }
        let mut healthy = 0;
        for h in self.buildings.iter().filter_map(|b| b.house.as_ref()).filter(|h| h.population > 0) {
            let cared = if h.level <= 1 { h.coverage.apothecary > 0 } else { h.coverage.physician > 0 && h.days_without_food == 0 };
            healthy += if cared { h.population } else { h.population / 4 };
        }
        let target = healthy * 100 / pop;
        let r = &mut self.ratings;
        r.health_target = target;
        r.health = if r.health < target { (r.health + 2).min(target) } else { (r.health - 2).max(target) }.clamp(0, 100);
    }

    /// The prosperity cap: the average of what each occupied house's level allows.
    fn update_prosperity_max(&mut self) {
        let (mut sum, mut n) = (0, 0);
        for h in self.buildings.iter().filter_map(|b| b.house.as_ref()).filter(|h| h.population > 0) {
            sum += self.balance.house(h.level).prosperity;
            n += 1;
        }
        self.ratings.prosperity_max = if n > 0 { sum / n } else { 0 };
    }

    /// How far a monument has been built, 0-100.
    fn monument_progress(&self, id: u32) -> i32 {
        let Some(b) = self.buildings.get(id) else { return 0 };
        let Some(m) = &b.monument else { return 0 };
        if m.finished {
            return 100;
        }
        let Some(def) = crate::monuments::monument_def(b.kind) else { return 0 };
        let phases = def.phases.len() as i32 - 1;
        let done = m.progress.iter().map(|&p| p as i32).sum::<i32>();
        let per_phase = (m.progress.len() as i32 * crate::monuments::BLOCK_WORK as i32).max(1);
        (m.phase as i32 * 100 + done * 100 / per_phase) / phases.max(1)
    }

    fn update_monument_rating(&mut self) {
        let sum: f32 = self
            .buildings
            .iter()
            .filter_map(|b| MONUMENT_WEIGHTS.iter().find(|w| w.0 == b.kind).map(|w| w.1 as f32 * self.monument_progress(b.id) as f32 / 100.0))
            .sum();
        self.ratings.monument = if sum <= 0.0 { 0 } else { (MONUMENT_MULT * sum + MONUMENT_OFFSET).clamp(0.0, 100.0) as i32 };
    }

    /// Debt: the first time the treasury runs dry the Kingdom bails the city out (at a
    /// cost to prosperity); staying in debt a year, then another, costs the kingdom
    /// rating, and a third caps it.
    fn update_debt(&mut self) {
        let r = &mut self.ratings;
        if self.treasury >= 0 {
            r.months_in_debt = 0;
            return;
        }
        if r.debt_state == 0 {
            r.debt_state = 1;
            r.prosperity = (r.prosperity - 3).max(0);
            r.months_in_debt = 0;
            return;
        }
        r.months_in_debt += 1;
        if r.months_in_debt < 12 {
            return;
        }
        r.months_in_debt = 0;
        match r.debt_state {
            1 => r.change_kingdom(-5),
            2 => r.change_kingdom(-10),
            _ => r.kingdom_cap = 10,
        }
        r.debt_state = (r.debt_state + 1).min(4);
    }

    /// Yearly: tribute, prosperity and the kingdom's year.
    pub(crate) fn update_ratings_year(&mut self) {
        self.pay_tribute();
        // Prosperity.
        let mut change = 0;
        change += match self.unemployment {
            u if u < 5 => 1,
            u if u >= 15 => -1,
            _ => 0,
        };
        let worth = self.treasury + self.finance.last_year.construction;
        change += if worth > self.ratings.last_year_worth { 5 } else { -1 };
        self.ratings.last_year_worth = self.treasury;
        let food_kinds = self.buildings.iter().filter_map(|b| b.house.as_ref()).map(|h| h.foods.iter().filter(|&&f| f > 0).count()).max().unwrap_or(0);
        if food_kinds >= 2 {
            change += 1;
        }
        let kingdom_wage = crate::finance::KINGDOM_WAGES;
        if self.finance.wages >= kingdom_wage + 2 {
            change += 1;
        } else if self.finance.wages < kingdom_wage {
            change -= 1;
        }
        let pop = self.population.max(1);
        let (mut huts, mut manors) = (0, 0);
        for h in self.buildings.iter().filter_map(|b| b.house.as_ref()) {
            if h.level <= 3 {
                huts += h.population;
            }
            if h.level >= 14 {
                manors += h.population;
            }
        }
        if huts * 100 / pop > 30 {
            change -= 1;
        }
        if manors * 100 / pop > 10 {
            change += 1;
        }
        if !self.ratings.tribute_paid_last_year {
            change -= 1;
        }
        if self.staffed(32) > 0 {
            change += 1;
        }
        change += match self.ratings.luxury_exported {
            n if n > 500 => 2,
            n if n > 100 => 1,
            _ => 0,
        };
        self.ratings.luxury_exported = 0;
        let r = &mut self.ratings;
        r.prosperity = (r.prosperity + change).clamp(0, r.prosperity_max).clamp(0, 100);
        // The kingdom's year: tribute owed, and early missions' slow decline.
        if !r.tribute_paid_last_year {
            let i = (r.tribute_unpaid_years.max(1) - 1).min(4) as usize;
            r.change_kingdom(TRIBUTE_PENALTY[i]);
        }
        if self.mission.as_ref().is_some_and(|m| m.id < 3) {
            self.ratings.change_kingdom(-2);
        }
        self.ratings.last_kingdom = self.ratings.kingdom;
    }

    /// Tribute to the Kingdom from last year's accounts.
    fn pay_tribute(&mut self) {
        let last = &self.finance.last_year;
        let income = last.taxes + last.exports + last.gold;
        let expenses = last.imports + last.wages + last.construction + last.interest;
        let pop = self.population;
        let tribute = if self.treasury <= 0 {
            None
        } else if income <= expenses {
            Some(match pop {
                p if p <= 1000 => 0,
                p if p <= 2000 => 100,
                _ => 200,
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
