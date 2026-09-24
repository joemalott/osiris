//! The governor's own affairs: the salary he draws from the treasury into his
//! personal savings each month (only with a mansion to live in, and not when the city
//! is deep in debt), gifts from those savings to the Kingdom, and donations back to the
//! city. Drawing more than his rank allows costs kingdom rating each year; drawing
//! less earns a little.

use crate::world::World;

/// Monthly salary of each rank, village elder to Pharaoh.
pub const SALARIES: [i32; 11] = [0, 2, 5, 8, 12, 20, 30, 40, 60, 80, 100];
/// The last rank, Pharaoh himself: he owes no tribute.
pub const PHARAOH_RANK: u8 = 10;
/// The treasury below which no salary is paid.
const SALARY_DEBT_LIMIT: i32 = -5000;
/// Gift sizes (modest, generous, lavish): cost is savings / rate + minimum.
const GIFT_RATE: [i32; 3] = [8, 4, 2];
const GIFT_MIN: [i32; 3] = [20, 50, 100];
/// Kingdom rating a gift earns, by how many gifts were sent in the last year.
const GIFT_GAIN: [[i32; 3]; 4] = [[3, 5, 10], [1, 3, 5], [0, 1, 3], [0, 0, 1]];
/// Months without a gift after which gifts count as fresh again.
const GIFT_MEMORY_MONTHS: i32 = 12;
const PERSONAL_MANSION: u16 = 77;
const DYNASTY_MANSION: u16 = 79;

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Governor {
    /// The rank the scenario gives the governor.
    #[serde(default)]
    pub assigned_rank: u8,
    /// The salary rank the governor pays himself; it starts at his rank.
    pub salary_rank: u8,
    pub savings: i32,
    /// Gifts sent recently, and months since the last.
    pub gifts: i32,
    pub months_since_gift: i32,
}

impl Governor {
    /// A new governor of the given rank, drawing that rank's salary.
    pub fn with_rank(rank: u8) -> Self {
        Self { assigned_rank: rank, salary_rank: rank, ..Default::default() }
    }
}

impl World {
    /// The rank Pharaoh has given the governor.
    pub fn assigned_rank(&self) -> u8 {
        self.governor.assigned_rank
    }

    pub fn has_mansion(&self) -> bool {
        self.buildings.iter().any(|b| (PERSONAL_MANSION..=DYNASTY_MANSION).contains(&b.kind))
    }

    /// Monthly: the governor draws his salary, and old gifts are forgotten.
    pub(crate) fn pay_salary(&mut self) {
        let g = &mut self.governor;
        g.months_since_gift += 1;
        if g.months_since_gift >= GIFT_MEMORY_MONTHS {
            g.gifts = 0;
        }
        if self.treasury <= SALARY_DEBT_LIMIT || !self.has_mansion() {
            return;
        }
        let pay = SALARIES[self.governor.salary_rank.min(10) as usize];
        self.treasury -= pay;
        self.finance.this_year.salary += pay;
        self.governor.savings += pay;
    }

    /// Yearly: Pharaoh judges the salary the governor takes: a point lost per rank
    /// drawn above his own, one gained for drawing below it.
    pub(crate) fn salary_year(&mut self) {
        let delta = self.governor.salary_rank as i32 - self.assigned_rank() as i32;
        if delta > 0 {
            self.ratings.change_kingdom(-delta);
        } else if delta < 0 {
            self.ratings.change_kingdom(1);
        }
    }

    pub fn set_salary_rank(&mut self, rank: u8) {
        self.governor.salary_rank = rank.min(10);
    }

    /// What a gift of size 0-2 costs from the governor's savings.
    pub fn gift_cost(&self, size: usize) -> i32 {
        let size = size.min(GIFT_RATE.len() - 1);
        self.governor.savings / GIFT_RATE[size] + GIFT_MIN[size]
    }

    /// Sends a gift to the Kingdom from the governor's savings; false if he cannot
    /// afford it.
    pub fn send_gift(&mut self, size: usize) -> bool {
        let cost = self.gift_cost(size);
        if size > 2 || cost > self.governor.savings {
            return false;
        }
        let tier = (self.governor.gifts as usize).min(3);
        self.governor.savings -= cost;
        self.governor.gifts += 1;
        self.governor.months_since_gift = 0;
        self.ratings.change_kingdom(GIFT_GAIN[tier][size]);
        true
    }

    /// The governor gives part of his savings to the city.
    pub fn donate(&mut self, amount: i32) {
        let amount = amount.clamp(0, self.governor.savings);
        self.governor.savings -= amount;
        self.treasury += amount;
    }
}
