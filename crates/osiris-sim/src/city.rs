//! The per-tick schedule. Like the original, city-wide systems each run on a fixed
//! tick of the 50-tick day, and walkers move every tick.

use crate::buildings::{BuildingId, kind};
use crate::desirability::{self, Influence};
use crate::houses::{self, Progress};
use crate::world::World;

/// Radius around a well within which houses count as having water.
const WELL_RADIUS: i32 = 2;

impl World {
    pub(crate) fn run_tick(&mut self) {
        self.rng.next();
        let roll = self.time.advance();
        self.update_floods();
        match self.time.tick {
            1 => {
                self.finish_production();
                self.check_unlocks();
                self.check_milestones();
                self.update_trade_problems();
                self.update_house_health();
                self.update_crime();
                self.update_recruiters();
                self.man_towers();
                self.check_siege();
            }
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
            28 => self.update_shrines(),
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
            }
            9 => self.decay_house_services(),
            32 => self.update_trade(),
            36 => self.update_culture(),
            38 => self.update_building_desirability(),
            39 => self.evolve_houses(),
            43 => self.update_burning_ruins(),
            45 => self.update_monuments(),
            44 => self.check_fire_and_collapse(),
            48 => self.decay_tax_coverage(),
            _ => {}
        }
        self.update_gods_tick();
        self.update_figures();
        if roll.week {
            self.consume_food();
            self.consume_goods();
            self.update_sentiment();
        }
        if roll.month {
            self.migration.newcomers_this_month = 0;
            self.advance_month_finance();
            self.regrow_herds();
            self.update_gods_month();
            self.update_ratings_month();
            self.check_outbreak();
            self.pay_salary();
            self.update_funerals();
            self.update_invasions();
            self.update_morale_month();
            self.update_distant_battle();
            self.process_scenario_events();
            self.update_sieges();
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
        if roll.year {
            self.advance_year_finance();
            // A temple complex to Ra raises the Kingdom's regard each year.
            if self.complex_blessing(crate::temple_complex::RA, 0) {
                self.ratings.change_kingdom(1);
            }
            self.reset_trade_year();
            self.update_ratings_year();
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
        desirability::recompute(&mut grid, &self.map, &self.buildings, |b| {
            // Empty lots don't spread anything.
            if b.house.as_ref().is_some_and(|h| h.population <= 0) {
                return Influence::default();
            }
            influences.get(b.kind as usize).copied().unwrap_or_default()
        });
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

    /// Tick 28: houses within three tiles of a shrine have access to religion.
    fn update_shrines(&mut self) {
        let shrines: Vec<(i32, i32, i32)> = self
            .buildings
            .iter()
            .filter(|b| (kind::SHRINE_OSIRIS..=kind::SHRINE_BAST).contains(&b.kind))
            .map(|b| (b.x, b.y, b.size))
            .collect();
        for b in self.buildings.iter_mut() {
            let Some(h) = b.house.as_mut() else { continue };
            let near = shrines.iter().any(|&(sx, sy, ss)| {
                b.x + b.size > sx - 3 && b.x <= sx + ss - 1 + 3 && b.y + b.size > sy - 3 && b.y <= sy + ss - 1 + 3
            });
            h.coverage.shrine = if near { crate::services::VISIT } else { 0 };
        }
    }

    fn decay_house_services(&mut self) {
        for b in self.buildings.iter_mut() {
            if let Some(h) = b.house.as_mut() {
                houses::decay(&mut h.coverage);
            }
        }
    }

    fn update_culture(&mut self) {
        for b in self.buildings.iter_mut() {
            if let Some(h) = b.house.as_mut() {
                h.derive_culture(0);
            }
        }
    }

    fn evolve_houses(&mut self) {
        let ids: Vec<BuildingId> = self.buildings.iter().filter(|b| b.is_house()).map(|b| b.id).collect();
        for id in ids {
            let Some(b) = self.buildings.get_mut(id) else { continue };
            let des = b.desirability;
            let Some(h) = b.house.as_mut() else { continue };
            if h.population <= 0 {
                continue;
            }
            let status = h.progress(&self.balance.houses, des, &self.rules);
            let level = h.level;
            match status {
                Progress::Evolve if (level as usize) + 1 < self.balance.houses.len() => {
                    h.devolve_delay = 0;
                    self.set_house_level(id, level + 1);
                }
                Progress::Decay if level > 0 => {
                    // A few checks' grace before devolving, as in the original.
                    h.devolve_delay += 1;
                    if h.devolve_delay > 2 {
                        h.devolve_delay = 0;
                        self.set_house_level(id, level - 1);
                        self.evict_overflow(id);
                    }
                }
                _ => h.devolve_delay = 0,
            }
        }
    }

    /// After devolving, people above the new capacity leave as homeless.
    fn evict_overflow(&mut self, id: BuildingId) {
        let Some(b) = self.buildings.get(id) else { return };
        let Some(h) = &b.house else { return };
        let cap = self.balance.house(h.level).max_people * b.size * b.size;
        let extra = h.population - cap;
        if extra > 0 {
            let (x, y) = b.road.unwrap_or((b.x, b.y));
            if let Some(h) = self.buildings.get_mut(id).and_then(|b| b.house.as_mut()) {
                h.population = cap;
            }
            self.population -= extra;
            self.census.remove(&self.rng, extra);
            let fid = self.figures.spawn(crate::people::figure_kind::HOMELESS, x, y, crate::figures::Travel::Land);
            if let Some(f) = self.figures.get_mut(fid) {
                f.amount = extra;
            }
        }
    }
}
