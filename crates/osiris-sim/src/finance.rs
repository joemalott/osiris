//! Money: monthly taxes, wages and debt interest, in the original's order.

use crate::world::World;

/// The wage the Kingdom pays per ten workers at the start, which cities are measured
/// against; scenario events raise and lower it.
pub const KINGDOM_WAGES: i32 = 30;

/// Percent of the assessed tax the city collects (the original's table has 50 for
/// every difficulty).
const TAX_COLLECTED_PCT: i32 = 50;

/// The first noble house level (common manor), taxed apart from commoners.
const NOBLE_LEVEL: u8 = 14;

fn kingdom_wages() -> i32 {
    KINGDOM_WAGES
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Finance {
    /// Percent, 0..=25.
    pub tax_rate: i32,
    /// Deben per worker per year (x10), as the original stores it.
    pub wages: i32,
    /// A per-difficulty tax percentage from Akhenaten's mission data; not applied, as
    /// the original collects `TAX_COLLECTED_PCT` at every difficulty.
    pub tax_multiplier_pct: i32,
    pub this_year: YearTotals,
    pub last_year: YearTotals,
    /// The treasury at the end of last year.
    #[serde(default)]
    pub last_year_balance: i32,
    #[serde(default = "kingdom_wages")]
    pub kingdom_wages: i32,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct YearTotals {
    pub taxes: i32,
    pub wages: i32,
    pub interest: i32,
    pub construction: i32,
    #[serde(default)]
    pub imports: i32,
    #[serde(default)]
    pub exports: i32,
    /// Gold mined and delivered to the palace.
    #[serde(default)]
    pub gold: i32,
    #[serde(default)]
    pub tribute: i32,
    /// The governor's salary.
    #[serde(default)]
    pub salary: i32,
}

impl Default for Finance {
    fn default() -> Self {
        Self {
            tax_rate: 7,
            wages: 30,
            tax_multiplier_pct: 150,
            this_year: YearTotals::default(),
            last_year: YearTotals::default(),
            last_year_balance: 0,
            kingdom_wages: KINGDOM_WAGES,
        }
    }
}

impl World {
    /// The month's tax on `population` people living at house `level`.
    pub fn house_tax(&self, level: u8, population: i32) -> i32 {
        self.collect_tax(population * self.balance.house(level).tax_multiplier)
    }

    /// The rate's share of a sum of people times their level's tax multiplier, of
    /// which the city collects half at every difficulty.
    fn collect_tax(&self, assessed: i32) -> i32 {
        assessed * self.finance.tax_rate / 100 * TAX_COLLECTED_PCT / 100
    }

    /// Taxes the city could collect this month from covered and uncovered houses.
    /// Commoners (below the common manor) and nobles are assessed separately.
    pub fn monthly_tax_estimate(&self) -> (i32, i32) {
        // [covered, uncovered] x [commoners, nobles]
        let mut assessed = [[0; 2]; 2];
        for b in self.buildings.iter() {
            let Some(h) = &b.house else { continue };
            if h.population <= 0 {
                continue;
            }
            let tax = h.population * self.balance.house(h.level).tax_multiplier;
            assessed[(h.coverage.tax <= 0) as usize][(h.level >= NOBLE_LEVEL) as usize] += tax;
        }
        let [covered, uncovered] = assessed.map(|[commoners, nobles]| self.collect_tax(commoners) + self.collect_tax(nobles));
        (covered, uncovered)
    }

    pub(crate) fn advance_month_finance(&mut self) {
        let (taxes, _) = self.monthly_tax_estimate();
        self.treasury += taxes;
        self.finance.this_year.taxes += taxes;
        let wages = self.finance.wages * self.labor.employed / 10 / 12;
        self.treasury -= wages;
        self.finance.this_year.wages += wages;
        if self.treasury < 0 {
            // The scenario's rate; a temple complex to Ra lowers it by five.
            let ra = if self.complex_blessing(crate::temple_complex::RA, 0) { 5 } else { 0 };
            let rate = (self.debt_rate - ra).max(0);
            let interest = (-self.treasury) * rate / 100 / 12;
            self.treasury -= interest;
            self.finance.this_year.interest += interest;
        }
    }

    pub(crate) fn advance_year_finance(&mut self) {
        self.finance.last_year = std::mem::take(&mut self.finance.this_year);
        self.finance.last_year_balance = self.treasury;
    }

    /// Percentage of the population living in houses a tax collector has visited.
    pub fn percentage_taxed(&self) -> i32 {
        let (mut all, mut taxed) = (0, 0);
        for h in self.buildings.iter().filter_map(|b| b.house.as_ref()) {
            all += h.population;
            if h.coverage.tax > 0 {
                taxed += h.population;
            }
        }
        if all > 0 { taxed * 100 / all } else { 0 }
    }

    /// Tick 48: tax coverage fades a little each day.
    pub(crate) fn decay_tax_coverage(&mut self) {
        for b in self.buildings.iter_mut() {
            if let Some(h) = b.house.as_mut() {
                h.coverage.tax = (h.coverage.tax - 1).max(0);
            }
        }
    }
}
