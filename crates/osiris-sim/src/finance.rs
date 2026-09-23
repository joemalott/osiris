//! Money: monthly taxes, wages and debt interest, in the original's order.

use crate::world::World;

/// The wage the Kingdom pays per ten workers, which cities are measured against.
pub const KINGDOM_WAGES: i32 = 30;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Finance {
    /// Percent, 0..=25.
    pub tax_rate: i32,
    /// Deben per worker per year (x10), as the original stores it.
    pub wages: i32,
    /// Difficulty's money percentage applied to house tax multipliers.
    pub tax_multiplier_pct: i32,
    pub this_year: YearTotals,
    pub last_year: YearTotals,
    /// The treasury at the end of last year.
    #[serde(default)]
    pub last_year_balance: i32,
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
        }
    }
}

impl World {
    /// Taxes the city could collect this month from covered and uncovered houses.
    pub fn monthly_tax_estimate(&self) -> (i32, i32) {
        let (mut covered, mut uncovered) = (0, 0);
        for b in self.buildings.iter() {
            let Some(h) = &b.house else { continue };
            if h.population <= 0 {
                continue;
            }
            let mult = self.balance.house(h.level).tax_multiplier * self.finance.tax_multiplier_pct / 100;
            let tax = h.population * mult;
            if h.coverage.tax > 0 {
                covered += tax;
            } else {
                uncovered += tax;
            }
        }
        let rate = self.finance.tax_rate;
        (covered / 2 * rate / 100, uncovered / 2 * rate / 100)
    }

    pub(crate) fn advance_month_finance(&mut self) {
        let (taxes, _) = self.monthly_tax_estimate();
        self.treasury += taxes;
        self.finance.this_year.taxes += taxes;
        let wages = self.finance.wages * self.labor.employed / 10 / 12;
        self.treasury -= wages;
        self.finance.this_year.wages += wages;
        if self.treasury < 0 {
            let interest = (-self.treasury) * 10 / 100 / 12;
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
