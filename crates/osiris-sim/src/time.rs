//! Game clock: 51 ticks make a day (the original's city update runs ticks 0 to 50),
//! 16 days a month, 12 months a year.

pub const TICKS_PER_DAY: u32 = 51;
pub const DAYS_PER_MONTH: u32 = 16;
pub const MONTHS_PER_YEAR: u32 = 12;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GameTime {
    pub tick: u32,
    pub day: u32,
    pub month: u32,
    /// Negative years are BC.
    pub year: i32,
    /// Ticks since the scenario started.
    pub total_ticks: u64,
}

/// What rolled over when the clock advanced one tick.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Rollover {
    pub day: bool,
    /// Half-month boundary (days 0 and 8), where several monthly systems run.
    pub week: bool,
    pub month: bool,
    pub year: bool,
}

impl GameTime {
    pub fn new(year: i32) -> Self {
        Self {
            tick: 0,
            day: 0,
            month: 0,
            year,
            total_ticks: 0,
        }
    }

    pub fn advance(&mut self) -> Rollover {
        let mut r = Rollover::default();
        self.total_ticks += 1;
        self.tick += 1;
        if self.tick < TICKS_PER_DAY {
            return r;
        }
        self.tick = 0;
        self.day += 1;
        r.day = true;
        if self.day.is_multiple_of(DAYS_PER_MONTH / 2) {
            r.week = true;
        }
        if self.day < DAYS_PER_MONTH {
            return r;
        }
        self.day = 0;
        self.month += 1;
        r.month = true;
        if self.month < MONTHS_PER_YEAR {
            return r;
        }
        self.month = 0;
        self.year += 1;
        r.year = true;
        r
    }

    /// Days elapsed since the scenario began.
    pub fn total_days(&self) -> u64 {
        self.total_ticks / TICKS_PER_DAY as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn year_has_9792_ticks() {
        let mut t = GameTime::new(-3500);
        let mut years = 0;
        for _ in 0..9792 {
            years += t.advance().year as u32;
        }
        assert_eq!((years, t.year, t.month, t.day, t.tick), (1, -3499, 0, 0, 0));
    }
}
