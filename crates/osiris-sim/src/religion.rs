//! The gods: Osiris, Ra, Ptah, Seth and Bast. A scenario makes each god unknown, known
//! (a local god) or the city's patron. Each known god's mood follows how much of the
//! city its shrines, temples and temple complexes reach (a patron needs twice as many),
//! lifted by recent festivals and held near indifference in small towns. A god far
//! from content gathers wrath and, once enough has gathered, curses the city; a god
//! well pleased gathers favour and blesses it. Festivals honour one god at a time.

use crate::buildings::{BuildingId, kind};
use crate::world::World;

pub const GODS: usize = 5;
pub const OSIRIS: usize = 0;
pub const RA: usize = 1;
pub const PTAH: usize = 2;
pub const SETH: usize = 3;
pub const BAST: usize = 4;
/// The five gods' names, by index, for messages outside the game's own text (the
/// cheat box's "god not worshipped" feedback; see `crate::cheats`).
pub const NAMES: [&str; GODS] = ["Osiris", "Ra", "Ptah", "Seth", "Bast"];

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
#[serde(default)]
pub struct Religion {
    pub gods: Vec<God>,
    /// Average coverage of the known gods.
    pub coverage_common: i32,
    pub festival: Option<PlannedFestival>,
    /// The two festival slots: months left of the year each festival ordered takes
    /// up (the first free slot is taken; with both taken a new festival waits out the
    /// first).
    pub festival_slots: (i32, i32),
    /// Ra's blessings and curses, in months left: exports sell for half again as much;
    /// trade allowances one step up, two steps down, one step down; no traders.
    pub ra_export_months: i32,
    pub ra_trade_up_months: i32,
    pub ra_trade_down2_months: i32,
    pub ra_trade_down_months: i32,
    pub ra_no_traders_months: i32,
    /// Osiris's blessing: the next floodplain harvest is doubled.
    pub osiris_double_harvest: bool,
    /// Osiris's wrath: locusts will eat the floodplain crops before the next flood.
    pub osiris_locusts: bool,
    /// Osiris's anger: the next flood destroys the farms it covers (1), and has
    /// begun to (2).
    pub osiris_flood_destroys: u8,
    /// Months before the gods' wrath is looked at again.
    pub wrath_message_delay: i32,
    /// Seth's minor blessing: the next troops sent to a distant battle win it
    /// without loss.
    pub seth_protects: bool,
    /// Seth's blessing: invaders he will strike down once they are in the city.
    pub seth_crush: i32,
}

impl Religion {
    pub fn new(statuses: [u8; 5]) -> Self {
        Self { gods: statuses.iter().map(|&s| God { status: s.min(2), ..Default::default() }).collect(), ..Default::default() }
    }

    pub fn known(&self) -> impl Iterator<Item = (usize, &God)> {
        self.gods.iter().enumerate().filter(|(_, g)| g.status != status::UNKNOWN)
    }

    /// Steps Ra moves the trade allowances: one up, two down, one down, added.
    pub fn ra_trade_steps(&self) -> i32 {
        (self.ra_trade_up_months > 0) as i32 - 2 * (self.ra_trade_down2_months > 0) as i32 - (self.ra_trade_down_months > 0) as i32
    }
}

/// Favour (mood 90+, 80-89) and wrath (21-30, 11-20, 10 or less) a god picked for
/// the day gathers (Normal difficulty).
const FAVOUR: [i32; 2] = [2, 1];
const WRATH: [i32; 3] = [1, 1, 2];
/// Shipwrights, weavers and jewellers, and the material Ptah stocks them with.
const PTAH_WORKSHOPS: [(u16, u16); 3] = [(74, 20), (111, 16), (113, 18)];
/// What a workshop Ptah stocks holds of its material after.
const PTAH_STOCK: i32 = 200;
/// The industries Ptah's wrath can raze: gold and gemstone mines, clay pits,
/// shipwrights, weavers and jewellers.
const PTAH_INDUSTRIES: [u16; 6] = [161, 162, 109, 74, 111, 113];
/// The goods Ptah's great blessing tops up in a storage yard: gems, clay, pottery,
/// flax, linen and jewellery.
const PTAH_GOODS: [u16; 6] = [18, 11, 13, 16, 17, 19];
/// Sentiment each house loses to locusts, frogs and hail.
const PLAGUE_SENTIMENT: i32 = -15;
/// Months a house the frogs reach stays empty.
const FROG_MONTHS: i32 = 5;
/// Bast's festival, which she throws herself.
const BAST_FESTIVAL: u8 = 4;

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
    /// gathers favour (2 at 90 or more, 1 at 80-89) or wrath (1 at 11-30, 2 at 10 or
    /// less), up to 50. A god above 50 loses its wrath, one below 50 its favour. On
    /// the first day of the month the god picked acts.
    pub(crate) fn update_gods_day(&mut self) {
        self.seth_strikes_invaders();
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
            match g.mood {
                m if m >= 90 => g.favour += FAVOUR[0],
                m if m >= 80 => g.favour += FAVOUR[1],
                m if m >= 31 => {}
                m if m >= 21 => g.wrath += WRATH[0],
                m if m >= 11 => g.wrath += WRATH[1],
                _ => g.wrath += WRATH[2],
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

    /// Monthly: coverage and moods, the gods' spells running out, and festivals.
    pub(crate) fn update_gods_month(&mut self) {
        self.locusts_descend();
        if self.religion.gods.is_empty() {
            return;
        }
        self.update_god_coverage();
        self.update_festival_month();
        let r = &mut self.religion;
        for m in [&mut r.ra_export_months, &mut r.ra_trade_up_months, &mut r.ra_trade_down2_months, &mut r.ra_trade_down_months, &mut r.ra_no_traders_months] {
            *m = (*m - 1).max(0);
        }
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
        // The original counts down twenty months after a god falls below 30, but
        // posts nothing.
        let least = self.religion.known().map(|(_, g)| g.mood).min().unwrap_or(50);
        if self.religion.wrath_message_delay > 0 {
            self.religion.wrath_message_delay -= 1;
        } else if least < 30 {
            self.religion.wrath_message_delay = 20;
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

    /// A fair coin for the gods' choices.
    fn coin(&mut self) -> bool {
        self.rng.below(2) == 1
    }

    /// Makes god `god` bless or curse the city now: for testing, and the original's
    /// per-god cheat codes (see `cheats.rs`), which just ask for one of these on the
    /// spot rather than waiting on the god's mood.
    pub fn god_acts_now(&mut self, god: usize, blessing: bool, major: bool) {
        if blessing {
            self.bless(god, major);
        } else {
            self.curse(god, major);
        }
    }

    /// "Fury of Seth": sends every warship, transport and fishing boat to the bottom.
    /// Unlike the per-god cheats above, the original doesn't gate this one on Seth
    /// being worshipped, so it calls the same effect `curse(SETH, true)` can pick
    /// directly rather than going through `god_acts_now`.
    pub fn fury_of_seth(&mut self) {
        self.seth_sinks_boats();
        self.post("message_wrath_of_seth", None, true);
    }

    fn bless(&mut self, god: usize, major: bool) {
        let key = match (god, major) {
            (OSIRIS, true) => {
                if self.coin() {
                    let q = (self.rng.below(3) * 5 + 10) * 2;
                    self.adjust_next_flood_quality(q);
                    "message_blessing_inundation_from_osiris"
                } else {
                    self.religion.osiris_double_harvest = true;
                    "message_blessing_from_osiris"
                }
            }
            (OSIRIS, false) => {
                let q = self.rng.below(4) * 5 + 5;
                self.adjust_next_flood_quality(q);
                "message_small_blessing_from_osiris"
            }
            (RA, true) => {
                if self.coin() {
                    self.ratings.change_kingdom(15);
                    "message_blessing_reputation_from_ra"
                } else {
                    self.religion.ra_export_months = 12;
                    "message_blessing_trade_from_ra"
                }
            }
            (RA, false) => {
                if self.coin() {
                    self.ratings.change_kingdom(5);
                    "message_minor_blessing_from_ra"
                } else {
                    self.religion.ra_trade_up_months = 12;
                    "message_minor_blessing_trading_from_ra"
                }
            }
            (PTAH, true) => {
                if self.ptah_fills_yard() {
                    "message_blessing_trade_from_ptah"
                } else {
                    "message_blessing_from_ptah"
                }
            }
            (PTAH, false) => {
                self.ptah_stocks_workshops();
                "message_minor_blessing_from_ptah"
            }
            (SETH, true) => {
                self.religion.seth_crush = 10;
                "message_blessing_trade_from_seth"
            }
            (SETH, false) => {
                self.religion.seth_protects = true;
                "message_minor_blessing_from_seth"
            }
            (_, true) => {
                self.bast_bounty();
                "message_blessing_from_bast"
            }
            _ => {
                self.bast_festival_now();
                return;
            }
        };
        self.post(key, None, true);
    }

    /// Bast's minor blessing: she throws a festival of her own, at once, taking the
    /// place of any festival being prepared. Also the original's "Meow" cheat.
    pub fn bast_festival_now(&mut self) {
        self.religion.festival = Some(PlannedFestival { god: OSIRIS, size: BAST_FESTIVAL, months_left: 1 });
        self.religion.festival_slots.0 = 1;
        self.post("message_small_blessing_from_bast", None, true);
        self.update_festival_month();
    }

    fn curse(&mut self, god: usize, major: bool) {
        let key = match (god, major) {
            (OSIRIS, true) => {
                if self.coin() {
                    self.religion.osiris_locusts = true;
                    self.change_house_sentiment(PLAGUE_SENTIMENT);
                    "message_wrath_of_osiris_2"
                } else {
                    let q = (-2 - self.rng.below(3)) * 10;
                    self.adjust_next_flood_quality(q);
                    "message_wrath_of_osiris"
                }
            }
            (OSIRIS, false) => {
                if self.coin() {
                    let q = (-1 - self.rng.below(4)) * 5;
                    self.adjust_next_flood_quality(q);
                    "message_wrath_of_osiris_4"
                } else {
                    self.religion.osiris_flood_destroys = 1;
                    "message_osiris_is_upset"
                }
            }
            (RA, true) => {
                if self.rng.below(3) == 0 {
                    self.religion.ra_trade_down2_months = 12;
                    "message_wrath_of_ra_2"
                } else if self.rng.below(3) != 1 {
                    self.religion.ra_no_traders_months = 12;
                    "message_wrath_of_ra_3"
                } else {
                    self.ratings.change_kingdom(-15);
                    "message_wrath_of_ra"
                }
            }
            (RA, false) => {
                if self.coin() {
                    self.ratings.change_kingdom(-5);
                    "message_ra_is_upset_2"
                } else {
                    self.religion.ra_trade_down_months = 12;
                    "message_ra_is_upset"
                }
            }
            (PTAH, true) => {
                if self.coin() {
                    self.frogs();
                    self.change_house_sentiment(PLAGUE_SENTIMENT);
                    "message_wrath_of_ptah_4"
                } else if self.ptah_razes_industry() {
                    "message_wrath_of_ptah_2"
                } else {
                    "message_wrath_of_ptah"
                }
            }
            (PTAH, false) => {
                // The fullest storage yard (the last of equals) burns with its goods.
                let yard = self.buildings.iter().filter(|b| b.kind == kind::STORAGE_YARD).max_by_key(|b| self.total_stored(b.id)).map(|b| b.id);
                match yard {
                    Some(id) => {
                        self.wreck(id, true);
                        "message_ptah_is_upset"
                    }
                    None => "message_wrath_of_ptah",
                }
            }
            (SETH, true) => {
                if self.coin() {
                    self.hailstorm();
                    self.change_house_sentiment(PLAGUE_SENTIMENT);
                    "message_hailstorm_wrath_of_seth"
                } else {
                    self.seth_sinks_boats();
                    "message_wrath_of_seth"
                }
            }
            (SETH, false) => {
                // Seth takes the most experienced company (the last of the equals) and
                // burns its fort.
                let best = self.military.companies.iter().filter(|c| c.fort != 0).fold(None, |best: Option<&crate::military::Company>, c| match best {
                    Some(b) if b.experience > c.experience => Some(b),
                    _ => Some(c),
                });
                match best.map(|c| c.fort) {
                    Some(fort) => {
                        self.wreck(fort, true);
                        "message_seth_is_upset"
                    }
                    None => "message_wrath_of_seth_noeffect",
                }
            }
            (_, true) => {
                // Fire takes the twenty finest houses.
                let mut houses: Vec<(u8, u32)> = self.buildings.iter().filter_map(|b| b.house.as_ref().map(|h| (h.level, b.id))).collect();
                houses.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
                for (_, id) in houses.into_iter().take(20) {
                    self.wreck(id, true);
                }
                "message_wrath_of_bast"
            }
            _ => {
                self.start_plague(true);
                "message_bast_is_upset"
            }
        };
        self.post(key, None, true);
    }

    /// Adds `delta` to the sentiment of every house, within 0-100.
    fn change_house_sentiment(&mut self, delta: i32) {
        for h in self.buildings.iter_mut().filter_map(|b| b.house.as_mut()) {
            h.happiness = (h.happiness + delta).clamp(0, 100);
        }
    }

    /// Ptah's great blessing: of the storage yards holding any of his goods, the one
    /// holding the least has the good it holds least of topped up to its limit.
    /// False if no yard holds any. Also the original's "Supreme Craftsman" cheat.
    pub fn ptah_fills_yard(&mut self) -> bool {
        let held = |w: &World, id: BuildingId| PTAH_GOODS.iter().map(|&r| w.stored(id, r)).sum::<i32>();
        let mut best: Option<(i32, BuildingId)> = None;
        for b in self.buildings.iter().filter(|b| b.kind == kind::STORAGE_YARD) {
            let n = held(self, b.id);
            if n > 0 && best.is_none_or(|(m, _)| n < m) {
                best = Some((n, b.id));
            }
        }
        let Some((_, yard)) = best else { return false };
        let mut least: Option<(i32, u16)> = None;
        for r in PTAH_GOODS {
            let n = self.stored(yard, r);
            if n > 0 && least.is_none_or(|(m, _)| n < m) {
                least = Some((n, r));
            }
        }
        if let Some((n, r)) = least {
            let limit = self.buildings.get(yard).map_or(0, |b| b.order_cap(r));
            self.add_stored(yard, r, (limit - n).max(0));
        }
        true
    }

    /// Ptah's minor blessing: every shipwright, weaver or jeweller (one kind, picked
    /// among those the city has) is stocked with 200 of its material. Also the
    /// original's "Noble Djed" cheat.
    pub fn ptah_stocks_workshops(&mut self) {
        let present: Vec<(u16, u16)> = PTAH_WORKSHOPS.into_iter().filter(|&(k, _)| self.buildings.iter().any(|b| b.kind == k)).collect();
        if present.is_empty() {
            return;
        }
        let (k, r) = present[self.rng.below(present.len() as i32) as usize];
        for b in self.buildings.iter_mut().filter(|b| b.kind == k) {
            if b.stock.len() <= r as usize {
                b.stock.resize(r as usize + 1, 0);
            }
            b.stock[r as usize] = b.stock[r as usize].max(PTAH_STOCK);
        }
    }

    /// Ptah's wrath: every building of one kind of industry, picked among those the
    /// city has, burns. False if it has none. Also the original's "Big Dave" cheat.
    pub fn ptah_razes_industry(&mut self) -> bool {
        let present: Vec<u16> = PTAH_INDUSTRIES.into_iter().filter(|&k| self.buildings.iter().any(|b| b.kind == k)).collect();
        if present.is_empty() {
            return false;
        }
        let k = present[self.rng.below(present.len() as i32) as usize];
        let ids: Vec<BuildingId> = self.buildings.iter().filter(|b| b.kind == k).map(|b| b.id).collect();
        for id in ids {
            self.wreck(id, true);
        }
        true
    }

    /// Ptah's frogs: from the finest houses down, houses holding up to 65% of the
    /// people are sought out, and each has an even chance that a frog reaches it and
    /// drives its people out; it stays empty while the plague lasts. Also Cleopatra's
    /// "Amphibious Assault" cheat (without the sentiment hit the natural curse adds).
    pub fn frogs(&mut self) {
        let total: i32 = self.buildings.iter().filter_map(|b| b.house.as_ref()).map(|h| h.population).sum();
        let limit = total * 65 / 100;
        let mut houses: Vec<(u8, BuildingId, i32)> = self.buildings.iter().filter_map(|b| b.house.as_ref().map(|h| (h.level, b.id, h.population))).collect();
        houses.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        let mut sought = 0;
        for (_, id, pop) in houses {
            if sought >= limit {
                break;
            }
            sought += pop;
            if sought > limit || self.rng.below(100) >= 50 {
                continue;
            }
            self.drive_out(id);
        }
    }

    /// A house's people leave it as homeless, and no one moves in for a while.
    fn drive_out(&mut self, id: BuildingId) {
        let Some(b) = self.buildings.get(id) else { return };
        let Some(h) = &b.house else { return };
        let (pop, (x, y)) = (h.population, b.road.unwrap_or((b.x, b.y)));
        if let Some(h) = self.buildings.get_mut(id).and_then(|b| b.house.as_mut()) {
            h.population = 0;
            h.quarantine = h.quarantine.max(FROG_MONTHS);
        }
        if pop <= 0 {
            return;
        }
        self.population -= pop;
        self.census.remove(&self.rng, pop);
        let fid = self.figures.spawn(crate::people::figure_kind::HOMELESS, x, y, crate::figures::Travel::Land);
        if let Some(f) = self.figures.get_mut(fid) {
            f.amount = pop;
        }
    }

    /// Seth's hail strikes down half of the soldiers, invaders, boats and dangerous
    /// beasts, and three in four of everyone else in the streets. (Figures the city keeps other
    /// accounts of, such as traders, monument crews and standard bearers, are
    /// spared.) Also Cleopatra's "Hail to the Chief" cheat (without the sentiment hit
    /// the natural curse adds).
    pub fn hailstorm(&mut self) {
        use crate::figures::Travel;
        for fid in self.figures.ids() {
            let Some(f) = self.figures.get(fid) else { continue };
            let k = f.kind;
            if f.dead || f.action == crate::military::action::CORPSE || !hail_strikes(k) {
                continue;
            }
            let fighter = crate::military::is_soldier(k) || crate::invasions::is_invader_kind(k) || matches!(k, crate::navy::WARSHIP | crate::navy::TRANSPORT | crate::navy::ENEMY_TRANSPORT);
            // Soldiers, invaders, boats and the dangerous beasts (crocodiles, hyenas,
            // hippos, asps, lions, scorpions) stand an even chance.
            let chance = if fighter || f.travel == Travel::Water || matches!(k, 82..=84 | 102..=104) { 50 } else { 75 };
            if self.rng.below(100) > chance {
                continue;
            }
            self.fall(fid, fighter);
        }
    }

    /// A figure is struck down: fighters and warships fall and lie a while, anyone
    /// else is gone.
    fn fall(&mut self, fid: crate::figures::FigureId, lies: bool) {
        let Some(f) = self.figures.get_mut(fid) else { return };
        if lies {
            f.action = crate::military::action::CORPSE;
            f.counter = 0;
            f.foe = 0;
            f.route.clear();
            f.moving = false;
        } else {
            f.dead = true;
        }
    }

    /// Seth's great wrath: every fishing boat, warship and transport is lost.
    fn seth_sinks_boats(&mut self) {
        for fid in self.figures.ids() {
            let Some(f) = self.figures.get(fid) else { continue };
            match f.kind {
                crate::fishing::FISHING_BOAT => self.fall(fid, false),
                crate::navy::WARSHIP | crate::navy::TRANSPORT if f.action != crate::military::action::CORPSE => self.fall(fid, true),
                _ => {}
            }
        }
    }

    /// Seth's great blessing: once invaders are in the city he strikes down up to
    /// ten of them and is done.
    fn seth_strikes_invaders(&mut self) {
        if self.religion.seth_crush <= 0 {
            return;
        }
        let invaders: Vec<(crate::figures::FigureId, (i32, i32))> = self
            .figures
            .iter()
            .filter(|f| crate::invasions::is_invader_kind(f.kind) && !f.dead && f.action != crate::military::action::CORPSE)
            .map(|f| (f.id, (f.x, f.y)))
            .collect();
        let Some(&(_, at)) = invaders.first() else { return };
        let n = std::mem::take(&mut self.religion.seth_crush) as usize;
        for &(fid, _) in invaders.iter().take(n) {
            self.fall(fid, true);
        }
        self.post("message_the_spirit_of_seth", Some(at), true);
    }

    /// Bast's great blessing: every house's food and goods are filled up (food to six
    /// meals a head, goods to twice its level's need), and every bazaar's stock is
    /// doubled or raised to 800 grain, 600 of other foods and 400 of goods. Only what
    /// is already there is filled. Also the original's "Cat Nip" cheat.
    pub fn bast_bounty(&mut self) {
        let houses = self.balance.houses.clone();
        for b in self.buildings.iter_mut() {
            let big = b.size > 1;
            if let Some(h) = b.house.as_mut() {
                let pop = h.population;
                for f in h.foods.iter_mut().filter(|f| **f != 0) {
                    *f = pop * 6;
                }
                if let Some(m) = houses.get(h.level as usize) {
                    for (slot, need) in [m.pottery, m.jewelry, m.linen, m.beer].into_iter().enumerate() {
                        if h.goods[slot] != 0 {
                            h.goods[slot] = need * if big { 8 } else { 2 };
                        }
                    }
                }
                continue;
            }
            if b.kind != kind::BAZAAR {
                continue;
            }
            for r in (1..=8).chain([13, 15, 17, 19]) {
                let Some(n) = b.stock.get_mut(r as usize).filter(|n| **n != 0) else { continue };
                let base = match r {
                    1 => 800,
                    2..=8 => 600,
                    _ => 400,
                };
                *n = (*n * 2).max(base).min(20000);
            }
        }
    }

    /// Monthly: when Osiris has sent locusts they come three months before the
    /// flood's season (by its month, season / 30) and eat every floodplain crop.
    fn locusts_descend(&mut self) {
        if !self.religion.osiris_locusts || !self.has_floodplain() || self.flood_month() != (self.time.month as i32 + 3) % 12 {
            return;
        }
        self.religion.osiris_locusts = false;
        for id in self.buildings.ids() {
            if self.is_floodplain_farm(id)
                && let Some(b) = self.buildings.get_mut(id)
            {
                b.progress = 0;
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
        (self.population / 500 + 1) * 100
    }

    /// Whether a festival can be ordered now: a festival square, nothing in
    /// preparation, and the city no more than 5000 in debt.
    pub fn can_hold_festival(&self) -> bool {
        self.buildings.iter().any(|b| b.kind == FESTIVAL_SQUARE) && self.religion.festival.is_none() && self.treasury > -5000
    }

    /// Orders a festival for `god`, paying for it now. It takes 2, 3 or 4 months by
    /// size, and takes a free festival slot for a year; with both slots taken it
    /// waits until the first frees. A grand festival needs its beer.
    pub fn plan_festival(&mut self, god: usize, size: u8) -> Result<(), &'static str> {
        if !self.can_hold_festival() {
            return Err("A festival can't be held now");
        }
        if self.religion.gods.get(god).is_none_or(|g| g.status == status::UNKNOWN) {
            return Err("The city does not know this god");
        }
        if size == festival::GRAND && self.yards_stored(15) < self.festival_beer() {
            return Err("Not enough beer");
        }
        let cost = self.festival_cost(size);
        let mut months = match size {
            festival::SMALL => 2,
            festival::LARGE => 3,
            _ => 4,
        };
        let slots = &mut self.religion.festival_slots;
        if slots.0 == 0 {
            slots.0 = 12;
        } else if slots.1 == 0 {
            slots.1 = 12;
        } else {
            months += slots.0;
        }
        if size == festival::GRAND {
            let mut left = self.festival_beer();
            for id in self.buildings.iter().filter(|b| b.kind == kind::STORAGE_YARD).map(|b| b.id).collect::<Vec<_>>() {
                left -= self.take_stored(id, 15, left);
            }
        }
        self.treasury -= cost;
        self.finance.this_year.construction += cost;
        self.religion.festival = Some(PlannedFestival { god, size, months_left: months });
        Ok(())
    }

    /// Monthly: the festival slots run down, and a festival whose day has come is
    /// held. With the first slot free it lifts every house's sentiment by 6, 10 or
    /// 15 and its god's mood by 1, 2 or 3; otherwise with the second free by 2, 3 or
    /// 5 and 1, 2 or 5; with both taken by nothing. Either way its god has had a
    /// festival.
    fn update_festival_month(&mut self) {
        let s = &mut self.religion.festival_slots;
        s.0 = (s.0 - 1).max(0);
        s.1 = (s.1 - 1).max(0);
        let Some(f) = self.religion.festival.as_mut() else { return };
        f.months_left -= 1;
        if f.months_left > 0 {
            return;
        }
        let f = self.religion.festival.take().expect("present");
        let (first, second) = self.religion.festival_slots;
        let (sentiment, mood) = match (first < 1, second < 1, f.size) {
            (true, _, festival::SMALL) => (6, 1),
            (true, _, festival::LARGE) => (10, 2),
            (true, _, _) => (15, 3),
            (false, true, festival::SMALL) => (2, 1),
            (false, true, festival::LARGE) => (3, 2),
            (false, true, festival::GRAND) => (5, 5),
            _ => (0, 0),
        };
        if sentiment != 0 {
            self.change_house_sentiment(sentiment);
        }
        if f.size == BAST_FESTIVAL {
            // Bast's own festival honours all the other gods.
            if first < 1 {
                for (i, g) in self.religion.gods.iter_mut().enumerate() {
                    if i != BAST {
                        g.mood += 3;
                    }
                }
            }
        } else if let Some(g) = self.religion.gods.get_mut(f.god) {
            g.mood += mood;
        }
        if let Some(g) = self.religion.gods.get_mut(f.god) {
            g.months_since_festival = 0;
        }
        let key = match f.size {
            festival::SMALL => "message_common_festival",
            festival::LARGE => "message_lavish_festival",
            festival::GRAND => "message_grand_festival",
            _ => return,
        };
        self.post(key, None, true);
    }

    /// The mood of the least happy known god, for the overseers.
    pub fn least_god_mood(&self) -> i32 {
        self.religion.known().map(|(_, g)| g.mood).min().unwrap_or(50)
    }
}

/// A trade allowance moved `steps` along 1500, 2500 and 4000 by Ra: one step up (an
/// allowance below 1500 comes up to it, one above 2500 stays), one down (1500 to
/// nothing), or two or more down (4000 to 1500, the others to nothing). Allowances
/// off the steps stay as they are when moved down.
pub fn ra_allowance(steps: i32, base: i32) -> i32 {
    match (steps, base) {
        (1, 2500) => 4000,
        (1, 1500) => 2500,
        (1, b) if b < 1500 => 1500,
        (-1, 4000) => 2500,
        (-1, 2500) => 1500,
        (-1, 1500) => 0,
        (s, 4000) if s <= -2 => 1500,
        (s, 1500 | 2500) if s <= -2 => 0,
        (_, b) => b,
    }
}

/// Whether Seth's hail can strike a figure of kind `k`: not the traders, monument
/// crews, standard bearers, missiles and ferries the city keeps other accounts of.
fn hail_strikes(k: u16) -> bool {
    use crate::{docks, military, monuments, royal_tombs, trade, water};
    !matches!(
        k,
        trade::TRADE_CARAVAN
            | trade::CARAVAN_DONKEY
            | docks::TRADE_SHIP
            | monuments::BRICKLAYER
            | monuments::STONEMASON
            | monuments::CARPENTER
            | monuments::SLED
            | monuments::SLED_PULLER
            | monuments::FUNERAL_WALKER
            | royal_tombs::TOMB_ARTISAN
            | military::STANDARD_BEARER
            | military::ARROW
            | military::JAVELIN
            | water::FERRY_BOAT
            | crate::defenses::TOWER_SENTRY
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::mask;

    /// The sandbox map with a clear 12x12 patch (with the real game data, when present).
    fn sandbox() -> Option<(World, i32, i32)> {
        let data = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../PharaohData");
        if !data.join("Maps/Sandbox.map").is_file() {
            return None;
        }
        let library = osiris_formats::ImageLibrary::open(&data.join("Data")).ok()?;
        let scenario = osiris_formats::Scenario::load_map(&data.join("Maps/Sandbox.map")).ok()?;
        let defs = std::sync::Arc::new(crate::defs::Defs::load(&library).ok()?);
        let model = osiris_formats::Model::parse(&String::from_utf8_lossy(&std::fs::read(data.join("Pharaoh_Model_Normal.txt")).ok()?)).ok()?;
        let balance = std::sync::Arc::new(crate::balance::Balance::from_model(&model));
        let mut world = World::new(&scenario, defs, balance);
        world.start(&scenario);
        let (w, h) = (world.map.width, world.map.height);
        let (x, y) = (0..h - 12).flat_map(|y| (0..w - 12).map(move |x| (x, y))).find(|&(x, y)| world.map.area_clear_of(x, y, 12, mask::NOT_CLEAR))?;
        world.messages.clear();
        Some((world, x, y))
    }

    fn house(world: &mut World, level: u8, x: i32, y: i32, happiness: i32) -> BuildingId {
        let id = world.create_building(kind::HOUSE_FIRST + level as u16, x, y);
        let h = world.buildings.get_mut(id).unwrap().house.as_mut().unwrap();
        h.population = 5;
        h.happiness = happiness;
        world.population += 5;
        id
    }

    #[test]
    fn ra_moves_allowances_along_the_steps() {
        assert_eq!([1500, 2500, 4000, 800, 3000].map(|b| ra_allowance(1, b)), [2500, 4000, 4000, 1500, 3000]);
        assert_eq!([1500, 2500, 4000, 800].map(|b| ra_allowance(-1, b)), [0, 1500, 2500, 800]);
        assert_eq!([1500, 2500, 4000, 800].map(|b| ra_allowance(-3, b)), [0, 0, 1500, 800]);
        assert_eq!(ra_allowance(0, 2500), 2500);
        let r = Religion { ra_trade_up_months: 3, ra_trade_down_months: 1, ra_trade_down2_months: 5, ..Default::default() };
        assert_eq!(r.ra_trade_steps(), -2);
    }

    #[test]
    fn ptahs_anger_burns_the_fullest_yard_without_a_fire_alarm() {
        let Some((mut world, x, y)) = sandbox() else { return };
        let a = world.create_building(kind::STORAGE_YARD, x, y);
        let b = world.create_building(kind::STORAGE_YARD, x + 4, y);
        world.buildings.get_mut(b).unwrap().spaces[0] = (13, 400);
        world.god_acts_now(PTAH, false, false);
        let yards: Vec<BuildingId> = world.buildings.iter().filter(|y| y.kind == kind::STORAGE_YARD).map(|y| y.id).collect();
        assert_eq!(yards, [a]);
        assert_eq!(world.messages.iter().collect::<Vec<_>>(), ["message_ptah_is_upset"]);
        assert!(world.buildings.iter().any(|r| r.kind == kind::BURNING_RUIN));
    }

    #[test]
    fn a_first_festival_lifts_the_second_slots_amounts_and_bast_throws_hers_at_once() {
        let Some((mut world, x, y)) = sandbox() else { return };
        world.religion = Religion::new([2, 1, 1, 1, 1]);
        world.create_building(FESTIVAL_SQUARE, x, y);
        let h = house(&mut world, 3, x + 6, y, 50);
        world.treasury = 1000;
        world.plan_festival(RA, festival::LARGE).unwrap();
        assert_eq!(world.religion.festival_slots, (12, 0));
        assert_eq!(world.religion.festival.as_ref().map(|f| f.months_left), Some(3));
        let mood = world.religion.gods[RA].mood;
        for _ in 0..3 {
            world.update_festival_month();
        }
        // The slot it took is still running, so it gives the second slot's lift.
        assert_eq!(world.buildings.get(h).unwrap().house.as_ref().unwrap().happiness, 53);
        assert_eq!(world.religion.gods[RA].mood, mood + 2);
        assert_eq!(world.messages.back().map(String::as_str), Some("message_lavish_festival"));
        // Bast's festival: the first slot is free for it, all but Bast are pleased,
        // and it counts as Osiris's festival.
        world.religion.gods[OSIRIS].months_since_festival = 9;
        world.god_acts_now(BAST, true, false);
        assert_eq!(world.buildings.get(h).unwrap().house.as_ref().unwrap().happiness, 68);
        assert_eq!(world.religion.gods[OSIRIS].months_since_festival, 0);
        assert_eq!(world.religion.gods[RA].mood, mood + 5);
        assert_eq!(world.religion.gods[BAST].mood, 50);
        assert!(world.religion.festival.is_none());
        assert_eq!(world.messages.back().map(String::as_str), Some("message_small_blessing_from_bast"));
    }

    #[test]
    fn seth_strikes_down_ten_invaders_once() {
        let Some((mut world, x, y)) = sandbox() else { return };
        world.god_acts_now(SETH, true, true);
        assert_eq!(world.messages.back().map(String::as_str), Some("message_blessing_trade_from_seth"));
        let ids: Vec<_> = (0..12).map(|i| world.figures.spawn(crate::invasions::ENEMY_INFANTRY, x + i % 6, y + i / 6, crate::figures::Travel::Hostile)).collect();
        world.seth_strikes_invaders();
        let fallen = ids.iter().filter(|&&f| world.figures.get(f).unwrap().action == crate::military::action::CORPSE).count();
        assert_eq!((fallen, world.religion.seth_crush), (10, 0));
        assert_eq!(world.messages.back().map(String::as_str), Some("message_the_spirit_of_seth"));
    }
}
