//! Numbers from the difficulty's model file, converted for the simulation.

use crate::houses::HouseModel;
use crate::world::BuildingStats;
use osiris_formats::Model;

/// The gameplay constants in `data/balance.toml`.
pub fn data() -> toml::Table {
    toml::from_str(include_str!("../data/balance.toml")).expect("balance.toml is valid")
}

#[derive(Debug, Clone, Default)]
pub struct Balance {
    /// Indexed by building type id.
    pub stats: Vec<BuildingStats>,
    /// The 20 house levels, crude hut first.
    pub houses: Vec<HouseModel>,
    /// Sentiment from the tax rate (row, 0-25%) at the city's tax coverage (11 columns).
    pub tax_sentiment: Vec<Vec<i32>>,
}

impl Balance {
    pub fn from_model(model: &Model) -> Self {
        let mut stats = Vec::new();
        for b in &model.buildings {
            let id = b.id as usize;
            if stats.len() <= id {
                stats.resize(id + 1, BuildingStats::default());
            }
            stats[id] = BuildingStats {
                cost: b.cost as i32,
                desirability: b.desirability_value as i32,
                des_step: b.desirability_step as i32,
                des_step_size: b.desirability_step_size as i32,
                des_range: b.desirability_range as i32,
                employees: b.employees as i32,
                fire_risk: b.fire_risk as i32,
                damage_risk: b.damage_risk as i32,
            };
        }
        let houses = model
            .houses
            .iter()
            .map(|h| HouseModel {
                devolve_desirability: h.devolve_desirability as i32,
                evolve_desirability: h.evolve_desirability as i32,
                entertainment: h.entertainment as i32,
                water: h.water as i32,
                religion: h.religion as i32,
                education: h.education as i32,
                bazaar: h.market as i32,
                dentist: h.dentist as i32,
                physician: h.physician as i32,
                health: h.health as i32,
                food_types: h.food as i32,
                pottery: h.pottery as i32,
                linen: h.linen as i32,
                jewelry: h.jewelry as i32,
                beer: h.beer as i32,
                crime_increment: h.crime_increment as i32,
                crime_base: h.crime_base as i32,
                prosperity: h.prosperity as i32,
                max_people: h.capacity as i32,
                tax_multiplier: h.tax_multiplier as i32,
                malaria_increment: h.malaria_increment as i32,
                disease_increment: h.disease_increment as i32,
            })
            .collect();
        Self { stats, houses, tax_sentiment: Vec::new() }
    }

    pub fn stats(&self, kind: u16) -> BuildingStats {
        self.stats.get(kind as usize).copied().unwrap_or_default()
    }

    pub fn house(&self, level: u8) -> &HouseModel {
        &self.houses[(level as usize).min(self.houses.len() - 1)]
    }
}
