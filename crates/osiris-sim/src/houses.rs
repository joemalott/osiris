//! Houses: population, the services and goods they have access to, and evolution.
//!
//! Each of the 20 house levels has a row in the model file listing the desirability at
//! which it devolves or may evolve, and what the *next* level needs. A house evolves
//! when its desirability reaches the evolve threshold and it already satisfies the
//! next level's needs; it devolves when desirability drops to the devolve threshold or
//! it no longer satisfies its own level's needs.

use crate::rules::Rules;

/// One row of the model file's house table.
#[derive(Debug, Clone, Copy, Default, serde::Serialize, serde::Deserialize)]
pub struct HouseModel {
    pub devolve_desirability: i32,
    pub evolve_desirability: i32,
    pub entertainment: i32,
    pub water: i32,
    pub religion: i32,
    pub education: i32,
    pub bazaar: i32,
    pub dentist: i32,
    pub physician: i32,
    pub health: i32,
    pub food_types: i32,
    pub pottery: i32,
    pub linen: i32,
    pub jewelry: i32,
    pub beer: i32,
    pub crime_increment: i32,
    pub crime_base: i32,
    pub prosperity: i32,
    pub max_people: i32,
    pub tax_multiplier: i32,
    pub malaria_increment: i32,
    pub disease_increment: i32,
}

/// Service coverage counters: set to 100 (or similar) when a walker passes, decaying
/// daily. A non-zero value means "has access".
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Coverage {
    pub water_supply: i32,
    pub juggler: i32,
    pub musician: i32,
    pub dancer: i32,
    pub senet: i32,
    pub zoo: i32,
    pub school: i32,
    pub library: i32,
    pub academy: i32,
    pub apothecary: i32,
    pub dentist: i32,
    pub mortuary: i32,
    pub physician: i32,
    pub magistrate: i32,
    pub temples: [i32; 5],
    pub shrine: i32,
    pub bazaar: i32,
    pub tax: i32,
}

pub const FOOD_TYPES: usize = 4;

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct House {
    /// 0 = crude hut (or vacant lot when empty) .. 19 = palatial estate.
    pub level: u8,
    pub population: i32,
    /// People on their way here as immigrants.
    pub incoming: i32,
    pub coverage: Coverage,
    /// Tiles within reach of a well.
    pub well_access: bool,
    /// Derived each day from coverage.
    pub entertainment: i32,
    pub education: i32,
    pub health: i32,
    pub gods: i32,
    pub foods: [i32; FOOD_TYPES],
    /// Pottery, jewelry, linen, beer.
    pub goods: [i32; 4],
    pub devolve_delay: i32,
    /// Why the house can't evolve (or is decaying), for the info panel.
    pub blocked_by: Option<Need>,
    /// The house fails its own level's needs and will devolve soon.
    #[serde(default)]
    pub decaying: bool,
    /// How happy the household is with the governor, 0-100.
    #[serde(default)]
    pub happiness: i32,
    /// Sentiment updates in a row without food (up to 3).
    #[serde(default)]
    pub days_without_food: i32,
    /// How healthy the household is, 0-100; it drifts toward what its apothecary,
    /// physician and dentist give it.
    #[serde(default = "half")]
    pub common_health: i32,
    /// Days of plague left.
    #[serde(default)]
    pub plague_days: i32,
    /// Crime brewing in an unhappy household, 0-100.
    #[serde(default)]
    pub criminal_active: i32,
}

fn half() -> i32 {
    50
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Need {
    Desirability,
    Water,
    WaterSupply,
    Entertainment,
    Education,
    Religion,
    Dentist,
    Physician,
    Health,
    Food,
    Pottery,
    Linen,
    Jewelry,
    Beer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Progress {
    Evolve,
    Stay,
    Decay,
}

impl House {
    pub fn kind(&self) -> u16 {
        crate::buildings::kind::HOUSE_FIRST + self.level as u16
    }

    pub fn is_vacant(&self) -> bool {
        self.population <= 0
    }

    /// Recomputes entertainment/education/health/religion from coverage.
    pub fn derive_culture(&mut self, city_entertainment_base: i32) {
        let c = &self.coverage;
        // Each performer type contributes up to its share of 100.
        let mut ent = city_entertainment_base;
        ent += c.juggler.min(100) * 10 / 50;
        ent += c.musician.min(100) * 10 / 40;
        ent += c.dancer.min(100) * 10 / 40;
        ent += c.senet.min(100) * 10 / 25;
        ent += c.zoo.min(100) * 10 / 25;
        self.entertainment = ent.min(100);
        self.education = if c.school > 0 || c.library > 0 {
            if c.school > 0 && c.library > 0 {
                if c.academy > 0 { 3 } else { 2 }
            } else {
                1
            }
        } else {
            0
        };
        let temples = c.temples.iter().filter(|&&t| t > 0).count() as i32;
        self.gods = if temples == 0 && c.shrine > 0 { 1 } else { temples };
        self.health = (c.apothecary > 0) as i32 + (c.physician > 0) as i32;
    }

    /// Whether this house has what `model` asks for; the first unmet need otherwise.
    pub fn meets(&self, model: &HouseModel, rules: &Rules) -> Result<(), Need> {
        let has_water_supply = self.coverage.water_supply > 0;
        if !has_water_supply {
            if model.water >= 2 {
                return Err(Need::WaterSupply);
            }
            if model.water == 1 && !self.well_access {
                return Err(Need::Water);
            }
        }
        if self.entertainment < model.entertainment {
            return Err(Need::Entertainment);
        }
        if self.education < model.education {
            return Err(Need::Education);
        }
        if rules.gods_enabled && self.gods < model.religion {
            return Err(Need::Religion);
        }
        if (self.coverage.dentist <= 0) & (model.dentist > 0) {
            return Err(Need::Dentist);
        }
        if (self.coverage.physician <= 0) & (model.physician > 0) {
            return Err(Need::Physician);
        }
        if self.health < model.health {
            return Err(Need::Health);
        }
        let food_types = self.foods.iter().filter(|&&f| f > 0).count() as i32;
        if food_types < model.food_types {
            return Err(Need::Food);
        }
        if self.goods[0] < model.pottery {
            return Err(Need::Pottery);
        }
        if self.goods[2] < model.linen {
            return Err(Need::Linen);
        }
        if self.goods[1] < model.jewelry {
            return Err(Need::Jewelry);
        }
        if model.beer > 0 && self.goods[3] <= 0 {
            return Err(Need::Beer);
        }
        Ok(())
    }

    /// Decides whether the house evolves, stays or devolves.
    pub fn progress(&mut self, models: &[HouseModel], desirability: i32, rules: &Rules) -> Progress {
        let level = self.level as usize;
        let model = &models[level];
        let evolve_des = if level + 1 >= models.len() { 1000 } else { model.evolve_desirability };
        self.decaying = false;
        let mut status = if desirability <= model.devolve_desirability {
            self.blocked_by = Some(Need::Desirability);
            Progress::Decay
        } else if desirability >= evolve_des {
            Progress::Evolve
        } else {
            self.blocked_by = Some(Need::Desirability);
            Progress::Stay
        };
        if let Err(need) = self.meets(model, rules) {
            self.blocked_by = Some(need);
            status = Progress::Decay;
        }
        if status == Progress::Decay {
            self.decaying = true;
        } else if status == Progress::Evolve {
            match models.get(level + 1).map(|next| self.meets(next, rules)) {
                Some(Ok(())) => self.blocked_by = None,
                Some(Err(need)) => {
                    self.blocked_by = Some(need);
                    status = Progress::Stay;
                }
                None => status = Progress::Stay,
            }
        }
        status
    }
}

/// Daily decay of coverage counters.
pub fn decay(c: &mut Coverage) {
    let dec = |v: &mut i32| *v = (*v - 1).max(0);
    for v in [
        &mut c.water_supply,
        &mut c.juggler,
        &mut c.musician,
        &mut c.dancer,
        &mut c.senet,
        &mut c.zoo,
        &mut c.school,
        &mut c.library,
        &mut c.academy,
        &mut c.apothecary,
        &mut c.dentist,
        &mut c.mortuary,
        &mut c.physician,
        &mut c.magistrate,
        &mut c.shrine,
        &mut c.bazaar,
    ] {
        dec(v);
    }
    for t in &mut c.temples {
        dec(t);
    }
}
