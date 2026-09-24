//! The gods: Osiris, Ra, Ptah, Seth and Bast. A scenario makes each god unknown, known
//! (a local god) or the city's patron. Each known god's mood follows how much of the
//! city its shrines, temples and temple complexes reach (a patron needs twice as many),
//! lifted by recent festivals and held near indifference in small towns. A god far
//! from content gathers wrath and, once enough has gathered, curses the city; a god
//! well pleased gathers favour and blesses it. Festivals honour one god at a time.

use crate::buildings::kind;
use crate::world::World;

pub const GODS: usize = 5;
pub const OSIRIS: usize = 0;
pub const RA: usize = 1;
pub const PTAH: usize = 2;
pub const SETH: usize = 3;
pub const BAST: usize = 4;

pub mod status {
    pub const UNKNOWN: u8 = 0;
    pub const KNOWN: u8 = 1;
    pub const PATRON: u8 = 2;
}

const TEMPLE_FIRST: u16 = 60;
const COMPLEX_FIRST: u16 = 65;
const SHRINE_FIRST: u16 = 140;
pub const FESTIVAL_SQUARE: u16 = 209;
/// Wrath or favour a god can hold.
const MAX_COUNTER: i32 = 50;

/// Festival sizes.
pub mod festival {
    pub const SMALL: u8 = 1;
    pub const LARGE: u8 = 2;
    pub const GRAND: u8 = 3;
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct God {
    pub status: u8,
    pub mood: i32,
    pub target: i32,
    pub wrath: i32,
    pub favour: i32,
    pub months_since_festival: i32,
    /// Share of the city this god's buildings reach, percent.
    pub coverage: i32,
}

impl Default for God {
    fn default() -> Self {
        Self { status: status::UNKNOWN, mood: 50, target: 50, wrath: 0, favour: 0, months_since_festival: 0, coverage: 0 }
    }
}

/// A festival being prepared.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PlannedFestival {
    pub god: usize,
    pub size: u8,
    pub months_left: i32,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Religion {
    pub gods: Vec<God>,
    /// Average coverage of the known gods.
    pub coverage_common: i32,
    pub festival: Option<PlannedFestival>,
    /// Months since each of the last festivals, for the two-a-year limit.
    pub recent_festivals: Vec<i32>,
    /// The size of the last festival and how many times it has lifted sentiment.
    pub festival_mood: (i32, i32),
    /// Months of Ra's trade blessing (+) or curse (-), and no-trader months.
    pub ra_trade_months: i32,
    pub ra_trade_boost: i32,
    pub ra_no_traders_months: i32,
    /// Days of Osiris's doubled harvests.
    pub osiris_double_harvest_days: i32,
    pub wrath_message_delay: i32,
    /// Seth's minor blessing: the next troops sent to a distant battle win it
    /// without loss.
    #[serde(default)]
    pub seth_protects: bool,
}

impl Religion {
    pub fn new(statuses: [u8; 5]) -> Self {
        Self { gods: statuses.iter().map(|&s| God { status: s.min(2), ..Default::default() }).collect(), ..Default::default() }
    }

    pub fn known(&self) -> impl Iterator<Item = (usize, &God)> {
        self.gods.iter().enumerate().filter(|(_, g)| g.status != status::UNKNOWN)
    }
}

impl World {
    fn active(&self, k: u16) -> i32 {
        self.buildings.iter().filter(|b| b.kind == k && b.workers > 0).count() as i32
    }

    /// Shrines count only with road access.
    fn with_road(&self, k: u16) -> i32 {
        self.buildings.iter().filter(|b| b.kind == k && b.road.is_some()).count() as i32
    }

    /// Monthly: each known god's coverage, and the average.
    fn update_god_coverage(&mut self) {
        let pop = self.population;
        let mut total = 0;
        let mut known = 0;
        for g in 0..GODS {
            let st = self.religion.gods[g].status;
            if st == status::UNKNOWN {
                self.religion.gods[g].coverage = 0;
                continue;
            }
            let (shrine, temple) = if st == status::PATRON { (150, 375) } else { (300, 750) };
            let people = shrine * self.with_road(SHRINE_FIRST + g as u16)
                + temple * self.active(TEMPLE_FIRST + g as u16)
                + 8000 * self.active(COMPLEX_FIRST + g as u16);
            let cov = if pop > 0 { (people * 100 / pop).min(100) } else { 0 };
            self.religion.gods[g].coverage = cov;
            total += cov;
            known += 1;
        }
        self.religion.coverage_common = if known > 0 { total / known } else { 0 };
    }

    /// Daily: moods drift a point toward their targets, and one god picked at random
    /// gathers favour (2 at 90 or more, 1 at 80-89) or wrath (1 at 11-20, 2 at 10 or
    /// less), up to 50. A god above 50 loses its wrath, one below 50 its favour. On
    /// the first day of the month the god picked acts.
    pub(crate) fn update_gods_day(&mut self) {
        if !self.rules.gods_enabled || self.religion.gods.is_empty() {
            return;
        }
        for g in self.religion.gods.iter_mut().filter(|g| g.status != status::UNKNOWN) {
            if g.mood < g.target {
                g.mood += 1;
            } else if g.mood > g.target {
                g.mood -= 1;
            }
        }
        let pick = (self.rng.byte() as usize) % GODS;
        let g = &mut self.religion.gods[pick];
        if g.status != status::UNKNOWN {
            // Normal difficulty.
            match g.mood {
                m if m >= 90 => g.favour += 2,
                m if m >= 80 => g.favour += 1,
                m if m > 20 => {}
                m if m > 10 => g.wrath += 1,
                _ => g.wrath += 2,
            }
            if g.mood == 50 {
                g.wrath = 0;
            }
            g.wrath = g.wrath.min(MAX_COUNTER);
            g.favour = g.favour.min(MAX_COUNTER);
        }
        for g in &mut self.religion.gods {
            if g.mood < 50 {
                g.favour = 0;
            }
            if g.mood > 50 {
                g.wrath = 0;
            }
        }
        if self.time.day == 0 {
            self.gods_act(pick);
        }
    }

    /// Monthly: coverage and moods, then blessings and curses, and festivals.
    pub(crate) fn update_gods_month(&mut self) {
        if self.religion.gods.is_empty() {
            return;
        }
        self.update_god_coverage();
        self.update_festival_month();
        if !self.rules.gods_enabled {
            return;
        }
        let points = ((self.population - 350) / 50).clamp(0, 5);
        let (lo, hi) = (50 - 10 * points, 50 + 10 * points);
        for g in self.religion.gods.iter_mut().filter(|g| g.status != status::UNKNOWN) {
            g.months_since_festival += 1;
            let target = g.coverage + 12 - g.months_since_festival.min(40);
            g.target = target.clamp(0, 100).clamp(lo, hi);
        }
        // A warning when the gods are angry.
        self.religion.wrath_message_delay = (self.religion.wrath_message_delay - 1).max(0);
        let least = self.religion.known().map(|(_, g)| g.mood).min().unwrap_or(50);
        if least < 30 && self.religion.wrath_message_delay == 0 {
            self.religion.wrath_message_delay = 6;
            self.post("message_the_gods_are_wrathful", None, true);
        }
        let d = &mut self.religion.osiris_double_harvest_days;
        *d = (*d - 16).max(0);
        for m in [&mut self.religion.ra_trade_months, &mut self.religion.ra_no_traders_months] {
            *m = (*m - 1).max(0);
        }
        if self.religion.ra_trade_months == 0 {
            self.religion.ra_trade_boost = 0;
        }
    }

    /// The god picked that day acts: with all 50 favour and a festival within 15
    /// months it gives a major blessing, with 20 and one within 14 a minor one;
    /// otherwise, with all 50 wrath and no festival for more than 3 months, a major
    /// curse, with 20 a minor one. A blessing calms its mood by 30 or 12, a curse
    /// lifts it by as much.
    fn gods_act(&mut self, god: usize) {
        let Some(g) = self.religion.gods.get_mut(god).filter(|g| g.status != status::UNKNOWN) else { return };
        let recent = g.months_since_festival;
        if g.favour >= 20 && recent < 15 {
            let major = g.favour >= MAX_COUNTER;
            g.favour = 0;
            g.mood -= if major { 30 } else { 12 };
            self.bless(god, major);
        } else if recent > 3 && g.wrath > 19 {
            let major = g.wrath >= MAX_COUNTER;
            g.wrath = 0;
            g.mood += if major { 30 } else { 12 };
            self.curse(god, major);
        }
    }

    fn coin(&mut self) -> bool {
        self.rng.byte() & 1 == 0
    }

    /// Makes god `god` bless or curse the city now (for testing).
    pub fn god_acts_now(&mut self, god: usize, blessing: bool, major: bool) {
        if blessing {
            self.bless(god, major);
        } else {
            self.curse(god, major);
        }
    }

    fn bless(&mut self, god: usize, major: bool) {
        let key = match (god, major) {
            (OSIRIS, true) => {
                if self.coin() {
                    self.religion.osiris_double_harvest_days = 100 + self.rng.byte() % 50;
                    "message_blessing_from_osiris"
                } else {
                    let q = (self.rng.byte() % 3 * 5 + 10) * 2;
                    self.adjust_next_flood_quality(q);
                    "message_blessing_inundation_from_osiris"
                }
            }
            (OSIRIS, false) => {
                let q = self.rng.byte() % 4 * 5 + 5;
                self.adjust_next_flood_quality(q);
                "message_small_blessing_from_osiris"
            }
            (RA, true) => {
                if self.coin() {
                    self.religion.ra_trade_months = 12;
                    self.religion.ra_trade_boost = 2;
                    "message_blessing_trade_from_ra"
                } else {
                    self.ratings.change_kingdom(15);
                    "message_blessing_reputation_from_ra"
                }
            }
            (RA, false) => {
                if self.coin() {
                    self.religion.ra_trade_months = 12;
                    self.religion.ra_trade_boost = 1;
                    "message_minor_blessing_trading_from_ra"
                } else {
                    self.ratings.change_kingdom(5);
                    "message_minor_blessing_from_ra"
                }
            }
            (PTAH, true) => {
                self.ptah_restock();
                "message_blessing_from_ptah"
            }
            (PTAH, false) => {
                for b in self.buildings.iter_mut().filter(|b| matches!(b.kind, kind::SHIPWRIGHT | 111 | 113)) {
                    for r in [20u16, 16, 18] {
                        if b.stock.len() > r as usize {
                            b.stock[r as usize] = b.stock[r as usize].max(200);
                        }
                    }
                }
                "message_minor_blessing_from_ptah"
            }
            (SETH, true) => "message_the_spirit_of_seth",
            (SETH, false) => {
                self.religion.seth_protects = true;
                "message_minor_blessing_from_seth"
            }
            (_, true) => "message_blessing_from_bast",
            _ => {
                // A festival in the gods' honour.
                for g in [RA, PTAH, SETH] {
                    self.religion.gods[g].months_since_festival = 0;
                }
                "message_small_blessing_from_bast"
            }
        };
        self.post(key, None, true);
    }

    fn curse(&mut self, god: usize, major: bool) {
        let key = match (god, major) {
            (OSIRIS, true) => {
                let q = (-2 - self.rng.byte() % 3) * 10;
                self.adjust_next_flood_quality(q);
                "message_wrath_of_osiris"
            }
            (OSIRIS, false) => {
                let q = -(self.rng.byte() % 3 * 5 + 5);
                self.adjust_next_flood_quality(q);
                "message_wrath_of_osiris_2"
            }
            (RA, true) => match self.rng.byte() % 3 {
                0 => {
                    self.religion.ra_trade_months = 12;
                    self.religion.ra_trade_boost = -2;
                    "message_wrath_of_ra"
                }
                1 => {
                    self.ratings.change_kingdom(-15);
                    "message_wrath_of_ra_2"
                }
                _ => {
                    self.religion.ra_no_traders_months = 12;
                    "message_wrath_of_ra_3"
                }
            },
            (RA, false) => {
                if self.coin() {
                    self.religion.ra_trade_months = 12;
                    self.religion.ra_trade_boost = -1;
                } else {
                    self.ratings.change_kingdom(-5);
                }
                "message_wrath_of_ra_4"
            }
            (PTAH, true) => {
                let industries: Vec<u32> = self.buildings.iter().filter(|b| matches!(b.kind, 109 | 161 | 162 | 217 | kind::SHIPWRIGHT | 111 | 113)).map(|b| b.id).collect();
                if industries.is_empty() {
                    "message_wrath_of_ptah_4"
                } else {
                    let id = industries[self.rng.byte() as usize % industries.len()];
                    self.destroy(id, false);
                    "message_wrath_of_ptah"
                }
            }
            (PTAH, false) => {
                let yard = self.buildings.iter().filter(|b| b.kind == kind::STORAGE_YARD).max_by_key(|b| self.total_stored(b.id)).map(|b| b.id);
                match yard {
                    Some(id) => {
                        self.destroy(id, true);
                        "message_wrath_of_ptah_2"
                    }
                    None => "message_wrath_of_ptah_4",
                }
            }
            (SETH, false) => {
                // Seth takes the most experienced company (the last of the equals) and
                // razes its fort.
                let best = self.military.companies.iter().filter(|c| c.fort != 0).fold(None, |best: Option<&crate::military::Company>, c| match best {
                    Some(b) if b.experience > c.experience => Some(b),
                    _ => Some(c),
                });
                match best.map(|c| c.fort) {
                    Some(fort) => {
                        self.wreck(fort, false);
                        "message_seth_is_upset"
                    }
                    None => "message_wrath_of_seth_noeffect",
                }
            }
            (SETH, true) => "message_wrath_of_seth_noeffect",
            (_, true) => {
                // Fire takes the finest houses.
                let mut houses: Vec<(u8, u32)> = self.buildings.iter().filter_map(|b| b.house.as_ref().filter(|h| h.population > 0 && h.level >= 4).map(|h| (h.level, b.id))).collect();
                houses.sort_by(|a, b| b.cmp(a));
                if houses.is_empty() {
                    "message_bast_is_upset"
                } else {
                    for (_, id) in houses.into_iter().take(20) {
                        self.destroy(id, true);
                    }
                    "message_wrath_of_bast"
                }
            }
            _ => {
                self.start_plague(true);
                "message_bast_is_upset"
            }
        };
        self.post(key, None, true);
    }

    /// Ptah fills the emptiest storage yard with crafts and their materials.
    fn ptah_restock(&mut self) {
        let yard = self.buildings.iter().filter(|b| b.kind == kind::STORAGE_YARD && b.road.is_some()).min_by_key(|b| self.total_stored(b.id)).map(|b| b.id);
        let Some(yard) = yard else { return };
        for r in [18u16, 11, 13, 16, 17, 19] {
            let room = self.storage_room(yard, r).min(400);
            if room > 0 {
                self.add_stored(yard, r, room);
            }
        }
    }

    /// A festival's cost in deben for the city's size.
    pub fn festival_cost(&self, size: u8) -> i32 {
        let pop = self.population;
        match size {
            festival::SMALL => pop / 20 + 10,
            festival::LARGE => pop / 10 + 20,
            _ => pop / 5 + 40,
        }
    }

    /// Beer a grand festival pours.
    pub fn festival_beer(&self) -> i32 {
        self.population / 50 + 1
    }

    fn festival_months(&self, size: u8) -> i32 {
        let pop = self.population;
        match size {
            festival::SMALL => 2 + pop / 1000,
            festival::LARGE => 3 + pop / 1500,
            _ => 4 + pop / 2000,
        }
    }

    /// Whether a festival can be held now: a festival square, nothing in preparation,
    /// and fewer than two in the past year.
    pub fn can_hold_festival(&self) -> bool {
        self.buildings.iter().any(|b| b.kind == FESTIVAL_SQUARE)
            && self.religion.festival.is_none()
            && self.religion.recent_festivals.iter().filter(|&&m| m < 12).count() < 2
    }

    /// Orders a festival for `god`, paying for it now.
    pub fn plan_festival(&mut self, god: usize, size: u8) -> Result<(), &'static str> {
        if !self.can_hold_festival() {
            return Err("A festival can't be held now");
        }
        let cost = self.festival_cost(size);
        if self.treasury < cost {
            return Err("Not enough money");
        }
        let size = if size == festival::GRAND && self.yards_stored(15) < self.festival_beer() { festival::LARGE } else { size };
        if size == festival::GRAND {
            let mut left = self.festival_beer();
            for id in self.buildings.iter().filter(|b| b.kind == kind::STORAGE_YARD).map(|b| b.id).collect::<Vec<_>>() {
                left -= self.take_stored(id, 15, left);
            }
        }
        self.treasury -= cost;
        self.finance.this_year.construction += cost;
        let months = self.festival_months(size);
        self.religion.festival = Some(PlannedFestival { god, size, months_left: months });
        Ok(())
    }

    fn update_festival_month(&mut self) {
        for m in &mut self.religion.recent_festivals {
            *m += 1;
        }
        self.religion.recent_festivals.retain(|&m| m < 12);
        let Some(f) = self.religion.festival.as_mut() else { return };
        f.months_left -= 1;
        if f.months_left > 0 {
            return;
        }
        let f = self.religion.festival.take().expect("present");
        if let Some(g) = self.religion.gods.get_mut(f.god) {
            g.months_since_festival = 0;
        }
        self.religion.recent_festivals.push(0);
        let boost = festival_boost(f.size, 0);
        self.religion.festival_mood = (f.size as i32, 1);
        for h in self.buildings.iter_mut().filter_map(|b| b.house.as_mut()) {
            h.happiness = (h.happiness + boost).min(100);
        }
        let key = match f.size {
            festival::SMALL => "message_common_festival",
            festival::LARGE => "message_lavish_festival",
            _ => "message_grand_festival",
        };
        self.post(key, None, true);
    }

    /// What the last festival still adds to each sentiment update: a second lift after
    /// the first, then a little each time while it is less than a year old.
    pub(crate) fn festival_sentiment(&mut self) -> i32 {
        let (size, stage) = self.religion.festival_mood;
        if stage == 0 || self.religion.recent_festivals.is_empty() {
            self.religion.festival_mood.1 = 0;
            return 0;
        }
        self.religion.festival_mood.1 = stage + 1;
        festival_boost(size as u8, stage.min(2))
    }

    /// The mood of the least happy known god, for the overseers.
    pub fn least_god_mood(&self) -> i32 {
        self.religion.known().map(|(_, g)| g.mood).min().unwrap_or(50)
    }
}

/// Sentiment a festival of `size` adds: on the day, at the next update, and after.
fn festival_boost(size: u8, stage: i32) -> i32 {
    let row = match size {
        festival::SMALL => [7, 2, 1],
        festival::LARGE => [9, 3, 2],
        _ => [12, 5, 3],
    };
    row[stage.clamp(0, 2) as usize]
}
