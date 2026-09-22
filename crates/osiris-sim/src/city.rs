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
        match self.time.tick {
            7 => {
                for id in self.buildings.ids() {
                    self.refresh_road_access(id);
                }
            }
            10 => self.update_desirability(),
            12 => self.decay_houses_covered(),
            22 => self.update_room(),
            23 => self.update_migration(),
            25 => self.update_labor(),
            27 => self.update_wells(),
            31 => self.generate_walkers(),
            9 => self.decay_house_services(),
            36 => self.update_culture(),
            38 => self.update_building_desirability(),
            39 => self.evolve_houses(),
            43 => self.update_burning_ruins(),
            44 => self.check_fire_and_collapse(),
            48 => self.decay_tax_coverage(),
            _ => {}
        }
        self.update_figures();
        if roll.month {
            self.migration.newcomers_this_month = 0;
            self.advance_month_finance();
        }
        if roll.year {
            self.advance_year_finance();
        }
    }

    fn update_figures(&mut self) {
        for fid in self.figures.ids() {
            let kind = self.figures.get(fid).map_or(0, |f| f.kind);
            match kind {
                crate::people::figure_kind::IMMIGRANT
                | crate::people::figure_kind::EMIGRANT
                | crate::people::figure_kind::HOMELESS => self.update_migrant(fid),
                k if crate::services::is_roamer(k) => self.update_roamer(fid),
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
                    for w in &mut b.walkers {
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
