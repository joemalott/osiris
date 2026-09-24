//! City sentiment: how the people feel about their governor. Every eight days each
//! occupied house's happiness moves with the tax rate (by how many people the tax
//! collectors reach), wages against the Kingdom's, unemployment, food, and how many
//! live in huts; the city's sentiment is the average. Towns under 300 people have no
//! complaints.

use crate::world::World;


/// Why sentiment is low, for the overseers.
pub mod cause {
    pub const NONE: u8 = 0;
    pub const NO_FOOD: u8 = 1;
    pub const NO_JOBS: u8 = 2;
    pub const HIGH_TAXES: u8 = 3;
    pub const LOW_WAGES: u8 = 4;
    pub const MANY_HUTS: u8 = 5;
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Sentiment {
    /// The hut penalty applies every other update.
    pub include_huts: bool,
    /// Unused since sentiment reports became city warnings; kept for old saves.
    pub message_delay: i32,
    pub low_mood_cause: u8,
}

/// City warnings (text group 19) about sentiment: 103 loathed (0), 104-113 one per
/// ten points ("very angry" .. "love you"), 114 idolized (100); 115-119 add the
/// reason (food, jobs, taxes, wages, slums).
const LOATHED: u16 = 103;
const IDOLIZED: u16 = 114;
const REASONS: u16 = 114;

impl World {
    /// The tax part of sentiment, from the tax sentiment model: the tax rate's row, at
    /// the column for the city's tax coverage.
    fn tax_sentiment(&self) -> i32 {
        let rate = self.finance.tax_rate.clamp(0, 25) as usize;
        let pct = self.percentage_taxed();
        let col = if pct <= 0 { 0 } else { (pct / 10 + 1).min(10) as usize };
        self.balance.tax_sentiment.get(rate).and_then(|r| r.get(col)).copied().unwrap_or(0)
    }

    fn wage_sentiment(&self) -> i32 {
        // Horus's oracle (Ra) has people content with two less than they'd want.
        let horus = if self.complex_blessing(crate::temple_complex::RA, crate::temple_complex::ORACLE) { 2 } else { 0 };
        let diff = self.finance.wages - self.finance.kingdom_wages + horus;
        match diff {
            d if d < 0 => (d / 2).min(-1),
            d if d > 7 => 4,
            d if d > 4 => 3,
            d if d > 1 => 2,
            d if d > 0 => 1,
            _ => 0,
        }
    }

    fn employment_sentiment(&self) -> i32 {
        match self.unemployment {
            u if u > 25 => -3,
            u if u > 17 => -2,
            u if u > 10 => -1,
            u if u > 4 => 0,
            _ => 1,
        }
    }

    /// The penalty for many living in huts, sharper when others live in residences or
    /// manors.
    fn hut_penalty(&mut self) -> i32 {
        if !self.sentiment_state.include_huts {
            self.sentiment_state.include_huts = true;
            return 0;
        }
        self.sentiment_state.include_huts = false;
        let (mut huts, mut residences, mut manors) = (0, 0, 0);
        for h in self.buildings.iter().filter_map(|b| b.house.as_ref()) {
            if h.level <= 1 {
                huts += h.population;
            }
            if h.level >= 10 {
                residences += h.population;
            }
            if h.level >= 14 {
                manors += h.population;
            }
        }
        let pct = if self.population > 0 { huts * 100 / self.population } else { 0 };
        let ladder = |steps: [i32; 5]| match pct {
            p if p >= 57 => steps[0],
            p if p >= 40 => steps[1],
            p if p >= 26 => steps[2],
            p if p >= 10 => steps[3],
            _ => steps[4],
        };
        if manors > 0 {
            ladder([0, -3, -4, -5, -6])
        } else if residences > 0 {
            ladder([0, -2, -3, -4, -5])
        } else {
            match pct {
                p if p >= 40 => 0,
                p if p >= 26 => -1,
                p if p >= 10 => -2,
                _ => -3,
            }
        }
    }

    /// When sentiment crosses into another ten in a city of more than 300, a warning
    /// says how the people feel, and below 50 (or at 0) why. The first two campaign
    /// missions stay quiet.
    fn report_sentiment(&mut self, previous: i32) {
        let value = self.sentiment;
        let tutorial = self.mission.as_ref().is_some_and(|m| m.id < 2);
        if tutorial || previous / 10 == value / 10 || self.population <= 300 {
            return;
        }
        let why = self.sentiment_state.low_mood_cause as u16;
        let (line, reason) = match value {
            v if v < 1 => (LOATHED, why != 0),
            v if v > 99 => (IDOLIZED, false),
            v => (104 + (v / 10) as u16, v < 50 && why != 0),
        };
        self.warnings.push_back(line);
        if reason {
            self.warnings.push_back(REASONS + why);
        }
    }

    /// Every eight days: houses grow happier or unhappier, and the city's sentiment is
    /// their average.
    pub(crate) fn update_sentiment(&mut self) {
        let small = self.population < 300;
        let taxes = self.tax_sentiment();
        let wages = self.wage_sentiment();
        let jobs = self.employment_sentiment();
        let huts = self.hut_penalty();
        let pop = self.population;
        let baseline = self.by_difficulty(crate::difficulty::BASELINE_SENTIMENT);
        let houses = self.balance.houses.clone();
        let (mut food_total, mut needing_food, mut huts_total, mut counted) = (0, 0, 0, 0);
        for b in self.buildings.iter_mut() {
            let Some(h) = b.house.as_mut() else { continue };
            if h.population <= 0 {
                h.happiness = 10 + baseline;
                continue;
            }
            if small {
                h.happiness = baseline + if pop < 200 { 10 } else { 0 };
                continue;
            }
            counted += 1;
            let needs_food = houses.get(h.level as usize).is_some_and(|m| m.food_types > 0);
            let (food, hut) = if !needs_food {
                h.days_without_food = 0;
                huts_total += huts;
                (0, huts)
            } else {
                needing_food += 1;
                let kinds = h.foods.iter().filter(|&&f| f > 0).count();
                let f = if kinds >= 2 {
                    h.days_without_food = 0;
                    2
                } else if kinds == 1 {
                    h.days_without_food = 0;
                    1
                } else {
                    h.days_without_food = (h.days_without_food + 1).min(3);
                    -h.days_without_food
                };
                food_total += f;
                (f, 0)
            };
            h.happiness = (h.happiness + taxes + wages + jobs + food + hut).clamp(0, 100);
        }
        let occupied: Vec<i32> = self.buildings.iter().filter_map(|b| b.house.as_ref()).filter(|h| h.population > 0).map(|h| h.happiness).collect();
        let previous = self.sentiment;
        self.sentiment = if occupied.is_empty() { self.by_difficulty(crate::difficulty::BASELINE_SENTIMENT) } else { occupied.iter().sum::<i32>() / occupied.len() as i32 };
        // Hathor's oracle (Bast) lifts the whole city's mood.
        if self.complex_blessing(crate::temple_complex::BAST, crate::temple_complex::ORACLE) {
            self.sentiment = (self.sentiment + 10).min(100);
        }
        // The worst of the reasons, for the overseers; it stands until another
        // reason turns negative (small towns leave it alone).
        if !small {
            let food_avg = if needing_food > 0 { food_total / needing_food } else { 0 };
            let huts_avg = if counted > 0 { huts_total / counted } else { 0 };
            let mut worst = 0;
            for (v, c) in [(food_avg, cause::NO_FOOD), (jobs, cause::NO_JOBS), (taxes, cause::HIGH_TAXES), (wages, cause::LOW_WAGES), (huts_avg, cause::MANY_HUTS)] {
                if v < worst {
                    worst = v;
                    self.sentiment_state.low_mood_cause = c;
                }
            }
        }
        self.report_sentiment(previous);
    }
}

#[cfg(test)]
mod tests {
    use crate::world::World;

    fn sandbox() -> Option<World> {
        let data = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../PharaohData");
        if !data.is_dir() {
            return None;
        }
        let library = osiris_formats::ImageLibrary::open(&data.join("Data")).expect("open image library");
        let scenario = osiris_formats::Scenario::load_map(&data.join("Maps/Sandbox.map")).expect("load map");
        let defs = std::sync::Arc::new(crate::defs::Defs::load(&library).expect("load defs"));
        let model_text = std::fs::read(data.join("Pharaoh_Model_Normal.txt")).expect("read model");
        let model = osiris_formats::Model::parse(&String::from_utf8_lossy(&model_text)).expect("parse model");
        let balance = std::sync::Arc::new(crate::balance::Balance::from_model(&model));
        Some(World::new(&scenario, defs, balance))
    }

    #[test]
    fn a_new_ten_of_sentiment_gets_a_warning() {
        let Some(mut world) = sandbox() else { return };
        world.population = 400;
        world.sentiment_state.low_mood_cause = super::cause::HIGH_TAXES;
        world.sentiment = 42;
        world.report_sentiment(55);
        assert_eq!(world.warnings.drain(..).collect::<Vec<_>>(), vec![108, 117]);
        world.sentiment = 47;
        world.report_sentiment(42);
        assert!(world.warnings.is_empty(), "same ten");
        world.sentiment = 63;
        world.report_sentiment(47);
        assert_eq!(world.warnings.drain(..).collect::<Vec<_>>(), vec![110], "no reason above 50");
        world.population = 300;
        world.sentiment = 75;
        world.report_sentiment(63);
        assert!(world.warnings.is_empty(), "towns of 300 or less");
    }
}
