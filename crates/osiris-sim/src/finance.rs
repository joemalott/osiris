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
    /// What the Kingdom lends the city the first time it runs out of money.
    #[serde(default)]
    pub rescue_loan: i32,
    /// Whether that loan has been given.
    #[serde(default)]
    pub rescued: bool,
    /// Months the city has ended in debt in a row, and the years of it so far.
    #[serde(default)]
    pub months_in_debt: i32,
    #[serde(default)]
    pub debt_years: i32,
    /// The city has been told it is in debt again, since it was last out of it.
    #[serde(default)]
    pub debt_warned: bool,
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
    /// Money given to the city: the Kingdom's rescue loan and gifts of deben.
    #[serde(default)]
    pub donated: i32,
    /// The wage level summed over the months, for the year's average.
    #[serde(default)]
    pub wage_months: i32,
}

impl YearTotals {
    /// Income as the Kingdom counts it for tribute and prosperity.
    pub fn income(&self) -> i32 {
        self.taxes + self.exports + self.gold + self.donated
    }

    pub fn expenses(&self) -> i32 {
        self.imports + self.wages + self.construction + self.interest + self.salary
    }
}

impl Default for Finance {
    fn default() -> Self {
        Self {
            tax_rate: 7,
            wages: 30,
            tax_multiplier_pct: TAX_COLLECTED_PCT,
            this_year: YearTotals::default(),
            last_year: YearTotals::default(),
            last_year_balance: 0,
            kingdom_wages: KINGDOM_WAGES,
            rescue_loan: 0,
            rescued: false,
            months_in_debt: 0,
            debt_years: 0,
            debt_warned: false,
        }
    }
}

/// Kingdom rating lost for each further year in debt (Normal difficulty); the tenth
/// and later all cost the last.
const DEBT_YEAR_PENALTY: [i32; 10] = [-5, -10, -20, -35, -50, -50, -50, -50, -50, -50];

impl World {
    /// The original stops all building, whatever it costs, once the treasury is 5000
    /// or more in debt; until then anything may be built.
    pub fn out_of_money(&self) -> bool {
        self.treasury <= -5000
    }

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
        let wages = self.finance.wages * self.labor.employed / 120;
        self.treasury -= wages;
        self.finance.this_year.wages += wages;
        self.finance.this_year.wage_months += self.finance.wages;
        if self.treasury < 0 {
            // The scenario's rate; a temple complex to Ra lowers it by five.
            let ra = if self.complex_blessing(crate::temple_complex::RA, 0) { 5 } else { 0 };
            let rate = (self.debt_rate - ra).max(0);
            let interest = (-self.treasury) * rate / 100 / 12;
            self.treasury -= interest;
            self.finance.this_year.interest += interest;
        }
    }

    /// Monthly, after the salary: every twelfth month in a row that ends in debt costs
    /// kingdom rating, more each year.
    pub(crate) fn count_debt_months(&mut self) {
        let f = &mut self.finance;
        if self.treasury >= 0 {
            f.months_in_debt = 0;
            f.debt_years = 0;
            return;
        }
        f.months_in_debt += 1;
        if f.months_in_debt % 12 == 0 {
            f.debt_years = (f.debt_years + 1).min(DEBT_YEAR_PENALTY.len() as i32);
            let penalty = DEBT_YEAR_PENALTY[f.debt_years as usize - 1];
            self.ratings.change_kingdom(penalty);
            self.post("message_debt_anniversary", None, true);
        }
    }

    /// Daily: the first time the treasury goes below zero the Kingdom lends the
    /// scenario's rescue sum, and (unless the governor is Pharaoh) the city's
    /// prosperity drops by three. Later debts are only announced.
    pub(crate) fn check_bankruptcy(&mut self) {
        let f = &mut self.finance;
        if self.treasury >= 0 {
            f.months_in_debt = 0;
            f.debt_years = 0;
            f.debt_warned = false;
            return;
        }
        if f.rescued {
            if !f.debt_warned {
                f.debt_warned = true;
                self.post("message_debt_again", None, true);
            }
            return;
        }
        f.rescued = true;
        let loan = f.rescue_loan;
        f.this_year.donated += loan;
        self.treasury += loan;
        if loan <= 0 {
            return;
        }
        if self.assigned_rank() == crate::kingdom::PHARAOH_RANK {
            self.post("message_out_of_money_again", None, true);
        } else {
            self.ratings.prosperity = (self.ratings.prosperity - 3).max(0);
            self.post("message_out_of_money", None, true);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debt_penalties_grow_each_year() {
        assert_eq!(&DEBT_YEAR_PENALTY[..4], &[-5, -10, -20, -35]);
        let y = YearTotals { taxes: 100, exports: 50, gold: 10, donated: 5, imports: 20, wages: 30, construction: 40, interest: 1, salary: 2, ..Default::default() };
        assert_eq!((y.income(), y.expenses()), (165, 93));
    }
}
