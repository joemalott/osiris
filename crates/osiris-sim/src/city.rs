//! The per-tick schedule. Like the original, city-wide systems each run on a fixed
//! tick of the 51-tick day, and walkers move every tick.

use crate::buildings::kind;
use crate::desirability::{self, Influence};
use crate::houses;
use crate::world::World;

/// Radius around a well within which houses count as having water.
const WELL_RADIUS: i32 = 2;

impl World {
    pub(crate) fn run_tick(&mut self) {
        self.rng.next();
        let roll = self.time.advance();
        self.update_floods();
        self.update_earthquakes();
        match self.time.tick {
            1 => {
                self.check_unlocks();
                self.check_milestones();
                self.update_trade_problems();
                self.update_crime();
                self.update_gods_day();
                self.update_recruiters();
                self.check_siege();
            }
            4 => self.check_bankruptcy(),
            7 => {
                for id in self.buildings.ids() {
                    self.refresh_road_access(id);
                }
            }
            10 => self.update_desirability(),
            18 => self.grow_vegetation(),
            12 => self.decay_houses_covered(),
            20 => self.update_production(),
            22 => self.update_room(),
            23 => self.update_migration(),
            25 => self.update_labor(),
            27 => self.update_wells(),
            33 => {
                self.update_farms();
                self.update_venues();
            }
            34 => {
                self.update_navy_yards();
                self.update_shipwrights();
                self.update_ferries();
            }
            31 => {
                self.generate_walkers();
                self.send_carts();
                self.bazaar_walkers();
                self.lodge_walkers();
                self.work_camp_walkers();
                self.school_walkers();
                self.venue_walkers();
                self.guild_walkers();
                self.artisan_walkers();
            }
            35 => self.decay_house_services(),
            32 => self.update_trade(),
            36 => self.update_culture(),
            38 => self.update_building_desirability(),
            39 => self.evolve_houses(),
            43 => self.update_burning_ruins(),
            45 => {
                self.update_monuments();
                self.release_criminals();
            }
            44 => self.check_fire_and_collapse(),
            48 => self.decay_tax_coverage(),
            _ => {}
        }
        self.update_figures();
        if roll.week {
            self.consume_food();
            self.consume_goods();
            self.update_sentiment();
            self.crime_half_month();
        }
        if roll.month {
            self.migration.newcomers_this_month = 0;
            self.advance_month_finance();
            self.pay_salary();
            self.count_debt_months();
            self.regrow_herds();
            self.update_gods_month();
            self.update_ratings_month();
            self.update_health_month();
            self.update_funerals();
            self.update_invasions();
            self.update_morale_month();
            self.update_distant_battle();
            self.process_scenario_events();
            self.update_sieges();
            self.update_all_roads();
            if roll.year {
                self.advance_year_finance();
                self.reset_trade_year();
                self.pay_tribute();
            }
            self.update_ratings(roll.year);
            // A temple complex to Ra raises the Kingdom's regard each year.
            if roll.year && self.complex_blessing(crate::temple_complex::RA, 0) {
                self.ratings.change_kingdom(1);
            }
            let years = self.time.year - self.scenario_events.start_year;
            let survived = self.survival.is_some_and(|n| years >= n);
            if !self.won && !self.lost && (self.goals_met() || survived) {
                self.won = true;
                self.messages.push_back("victory".to_owned());
            }
            if !self.won && !self.lost && self.time_limit.is_some_and(|n| years >= n) {
                self.lost = true;
                self.messages.push_back("out_of_time".to_owned());
            }
        }
    }

    fn update_figures(&mut self) {
        self.gather_combatants();
        for fid in self.figures.ids() {
            let kind = self.figures.get(fid).map_or(0, |f| f.kind);
            match kind {
                crate::people::figure_kind::IMMIGRANT
                | crate::people::figure_kind::EMIGRANT
                | crate::people::figure_kind::HOMELESS => self.update_migrant(fid),
                k if crate::entertainment::performer_slot(k).is_some() => self.update_entertainer(fid),
                k if crate::services::is_roamer(k) => self.update_roamer(fid),
                crate::economy::CART_PUSHER | crate::economy::STORAGEYARD_CART => self.update_cart(fid),
                crate::economy::LUMBERJACK | crate::economy::REED_GATHERER => self.update_gatherer(fid),
                crate::trade::TRADE_CARAVAN => self.update_caravan(fid),
                crate::monuments::BRICKLAYER | crate::monuments::STONEMASON | crate::monuments::CARPENTER => self.update_craftsman(fid),
                crate::royal_tombs::TOMB_ARTISAN => self.update_tomb_worker(fid),
                crate::monuments::SLED => self.update_sled(fid),
                crate::monuments::SLED_PULLER => self.update_sled_puller(fid),
                crate::monuments::FUNERAL_WALKER => self.update_funeral_walker(fid),
                crate::trade::CARAVAN_DONKEY => self.update_donkey(fid),
                crate::food::MARKET_BUYER => self.update_buyer(fid),
                k if crate::animals::is_animal(k) => self.update_animal(fid),
                k if crate::animals::is_hunter(k) => self.update_hunter(fid),
                crate::farms::PEASANT => self.update_peasant(fid),
                crate::health::PLAGUED_CITIZEN | crate::crime::PROTESTER | crate::crime::ROBBER => self.update_wanderer(fid),
                k if crate::military::is_soldier(k) => self.update_soldier(fid),
                crate::military::STANDARD_BEARER => self.update_standard_bearer(fid),
                crate::military::ARROW | crate::military::JAVELIN => self.update_missile(fid),
                crate::navy::ENEMY_TRANSPORT => self.update_enemy_transport(fid),
                crate::navy::WARSHIP => self.update_warship(fid),
                crate::navy::TRANSPORT => self.update_warship(fid),
                k if crate::invasions::is_invader_kind(k) => self.update_invader(fid),
                crate::defenses::TOWER_SENTRY => self.update_sentry(fid),
                crate::fishing::FISHING_BOAT => self.update_fishing_boat(fid),
                crate::docks::TRADE_SHIP => self.update_trade_ship(fid),
                crate::docks::DOCKER => self.update_docker(fid),
                crate::water::FERRY_BOAT => self.update_ferry_boat(fid),
                _ => {}
            }
            if self.figures.get(fid).is_some_and(|f| f.dead) {
                let f = self.figures.get(fid).expect("checked").clone();
                if f.kind == crate::people::figure_kind::IMMIGRANT && f.amount > 0 {
                    // Release the room this immigrant had reserved.
                    if let Some(h) = self.buildings.get_mut(f.target).and_then(|b| b.house.as_mut()) {
                        h.incoming = (h.incoming - f.amount).max(0);
                    }
                }
                self.figures.remove(fid);
                for b in self.buildings.iter_mut() {
                    for w in b.walkers.iter_mut().chain(&mut b.performers) {
                        if *w == fid {
                            *w = 0;
                        }
                    }
                }
            }
        }
    }

    fn influence_of(&self, k: u16) -> Influence {
        let s = self.balance.stats(k);
        Influence {
            value: s.desirability,
            step: s.des_step,
            step_size: s.des_step_size,
            range: s.des_range,
        }
    }

    fn update_desirability(&mut self) {
        let mut grid = std::mem::replace(&mut self.desirability, crate::grid::Grid::new(0, 0));
        let influences: Vec<Influence> = (0..=u16::MAX as usize)
            .take(self.balance.stats.len())
            .map(|k| self.influence_of(k as u16))
            .collect();
        // Every building spreads its type's influence, vacant lots included.
        desirability::recompute(&mut grid, &self.map, &self.buildings, |b| influences.get(b.kind as usize).copied().unwrap_or_default());
        self.desirability = grid;
    }

    fn update_building_desirability(&mut self) {
        for id in self.buildings.ids() {
            let Some(b) = self.buildings.get(id) else { continue };
            let d = desirability::at_building(&self.desirability, &self.map, b.x, b.y, b.size);
            if let Some(b) = self.buildings.get_mut(id) {
                b.desirability = d;
            }
        }
    }

    fn decay_houses_covered(&mut self) {
        for b in self.buildings.iter_mut() {
            b.houses_covered = (b.houses_covered - 1).max(0);
        }
    }

    fn update_wells(&mut self) {
        let wells: Vec<(i32, i32)> = self
            .buildings
            .iter()
            .filter(|b| b.kind == kind::WELL)
            .map(|b| (b.x, b.y))
            .collect();
        for b in self.buildings.iter_mut() {
            let Some(h) = b.house.as_mut() else { continue };
            let (x0, y0) = (b.x - WELL_RADIUS, b.y - WELL_RADIUS);
            let (x1, y1) = (b.x + b.size - 1 + WELL_RADIUS, b.y + b.size - 1 + WELL_RADIUS);
            h.well_access = wells.iter().any(|&(wx, wy)| wx >= x0 && wx <= x1 && wy >= y0 && wy <= y1);
        }
    }

    fn decay_house_services(&mut self) {
        // Under a complex to Bast, what services leave behind lasts twice as long.
        if self.complex_blessing(crate::temple_complex::BAST, 0) && self.time.day % 2 == 1 {
            return;
        }
        for b in self.buildings.iter_mut() {
            if let Some(h) = b.house.as_mut() {
                houses::decay(&mut h.coverage);
            }
        }
    }

    /// Daily: each house's entertainment, education, health and religion. The city's
    /// share of entertainment is the average coverage of booths, bandstands, pavilions,
    /// senet houses and zoos (400, 700, 1200, 5000 and 7500 people each), over 5. A
    /// staffed bandstand also counts as a booth, and a pavilion as both.
    fn update_culture(&mut self) {
        let base = self.entertainment_base();
        for b in self.buildings.iter_mut() {
            if let Some(h) = b.house.as_mut() {
                h.derive_culture(base);
            }
        }
    }
}
