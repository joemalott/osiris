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
    /// Fighting stats of each figure type (`Figure_model.txt`, "ALL FIGURES").
    pub units: Vec<UnitStats>,
    /// Fighting stats of the foreign armies ("ALL ENEMIES"): five rows a nation.
    pub enemy_units: Vec<UnitStats>,
}

/// A fighter's stats from the figure model.
#[derive(Debug, Clone, Copy, Default)]
pub struct UnitStats {
    /// What sort of figure it is to the fighting rules (the model's first column):
    /// 1 a citizen, 2 the city's fighters, 3 an enemy, 4 a criminal, 5 a native.
    pub class: i32,
    pub hp: i32,
    pub attack: i32,
    pub armor: i32,
    pub missile_armor: i32,
    pub missile_attack: i32,
    pub missile_range: i32,
    /// Ticks between shots.
    pub missile_delay: i32,
    /// Index into the model's speed table (6 = normal walking pace).
    pub speed: i32,
    /// How often this unit appears in an army, as a weight.
    pub frequency: i32,
}

impl UnitStats {
    fn from_model(f: &osiris_formats::FigureModel) -> Self {
        Self {
            class: f.kind as i32,
            hp: f.hit_points as i32,
            attack: f.attack as i32,
            armor: f.armor as i32,
            missile_armor: f.armor_vs_missiles as i32,
            missile_attack: f.missile_attack as i32,
            missile_range: f.missile_range as i32,
            missile_delay: f.missile_rate_of_fire as i32,
            speed: f.speed as i32,
            frequency: f.frequency as i32,
        }
    }
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
                i: b.values.get(8).copied().unwrap_or(0.0) as i32,
                j: b.values.get(9).copied().unwrap_or(0.0) as i32,
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
                magistrate: h.physician as i32,
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
        Self { stats, houses, tax_sentiment: Vec::new(), units: Vec::new(), enemy_units: Vec::new() }
    }

    /// The model files of difficulty `d` (as spelled in their names) in the game
    /// folder `data`: buildings and houses, tax sentiment and fighting units.
    pub fn load(data: &std::path::Path, d: &str) -> Result<Self, String> {
        let model_path = data.join(format!("Pharaoh_Model_{d}.txt"));
        let model_text = std::fs::read(&model_path).map_err(|e| format!("{}: {e}", model_path.display()))?;
        let model = Model::parse(&String::from_utf8_lossy(&model_text)).map_err(|e| format!("{}: {e}", model_path.display()))?;
        let mut balance = Self::from_model(&model);
        if let Ok(t) = std::fs::read(data.join(format!("Tax_Sentiment_Model_{d}.txt"))) {
            balance.tax_sentiment = osiris_formats::model::parse_tax_sentiment(&String::from_utf8_lossy(&t));
        }
        let figures = data.join(format!("Figure_model_{}.txt", d.to_lowercase()));
        if let Ok(t) = std::fs::read(figures).or_else(|_| std::fs::read(data.join("Figure_model.txt"))) {
            balance.set_units(&osiris_formats::model::parse_figures(&String::from_utf8_lossy(&t)).map_err(|e| e.to_string())?);
        }
        Ok(balance)
    }

    /// The balance tables of all five difficulties, as the game plays them.
    pub fn load_all(data: &std::path::Path) -> Result<std::sync::Arc<[std::sync::Arc<Self>; 5]>, String> {
        let list = crate::difficulty::FILE_NAMES.iter().map(|d| Self::load(data, d).map(std::sync::Arc::new)).collect::<Result<Vec<_>, _>>()?;
        Ok(std::sync::Arc::new(list.try_into().unwrap_or_else(|_| unreachable!("five difficulties"))))
    }

    /// Takes the fighting stats from a parsed `Figure_model*.txt`.
    pub fn set_units(&mut self, rows: &[osiris_formats::FigureModel]) {
        for f in rows {
            let list = if f.category.is_some() { &mut self.enemy_units } else { &mut self.units };
            let i = f.id as usize;
            if list.len() <= i {
                list.resize(i + 1, UnitStats::default());
            }
            list[i] = UnitStats::from_model(f);
        }
    }

    pub fn unit(&self, kind: u16) -> UnitStats {
        self.units.get(kind as usize).copied().unwrap_or_default()
    }

    pub fn stats(&self, kind: u16) -> BuildingStats {
        self.stats.get(kind as usize).copied().unwrap_or_default()
    }

    pub fn house(&self, level: u8) -> &HouseModel {
        &self.houses[(level as usize).min(self.houses.len() - 1)]
    }
}
