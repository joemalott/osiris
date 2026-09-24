//! Farms. Floodplain farms have no staff of their own: work camps send peasants who
//! tend a farm for 96 days, during which it counts as fully staffed. A farm's crop
//! grows each day at tick 20 by fertility x 0.16 / 99 x workers x 10 (at least 1), toward 2000.
//! Floodplain crops are harvested when the flood approaches, meadow crops on the first
//! day of their harvest months; a harvest yields 8 per percent of the crop grown.

use crate::buildings::{BuildingId, kind};
use crate::economy::LOAD;
use crate::figures::{Step, Travel};
use crate::floods::FloodState;
use crate::map::terrain;
use crate::world::World;

pub const PROGRESS_MAX: i32 = 2000;
const GROWTH_PER_FERTILITY: f64 = 0.16;
/// Meadow harvest months (0 = the first month) by farm type.
const HARVEST_MONTHS: [(u16, &[u32]); 8] = [(100, &[1, 7]), (101, &[11]), (102, &[4, 0]), (103, &[3]), (104, &[5, 10]), (105, &[3]), (196, &[8]), (224, &[11])];
/// Straw a grain farm sends out with each harvest.
const STRAW_PER_HARVEST: i32 = 100;
const LABOR_DAYS: i32 = 96;
/// Farms ask for peasants again once this many labor days remain.
const RELABOR_BELOW: i32 = 47;
pub const PEASANT: u16 = 35;
/// Peasants a work camp has out at once.
const MAX_PEASANTS_OUT: usize = 4;
/// Tiles of a tomb's site a laborer works before he goes home (the original's count
/// at 0x4ab4bf).
const SITE_TOUCHES: i32 = 4;
/// A laborer standing before he heads home.
pub const PAUSE: u16 = 9;
const TILE_WORK: u16 = crate::pyramids::TILE_WORK;

impl World {
    pub fn is_farm(&self, k: u16) -> bool {
        self.defs.building(k).is_some_and(|d| d.has_flag("is_farm"))
    }

    pub fn is_floodplain_farm(&self, id: BuildingId) -> bool {
        let Some(b) = self.buildings.get(id) else { return false };
        self.is_farm(b.kind) && self.map.terrain_is(b.x, b.y, terrain::FLOODPLAIN)
    }

    /// A farm's fertility: its tiles' average plus 2, and irrigation's bonus, at most 99.
    pub fn fertility(&self, id: BuildingId) -> i32 {
        let Some(b) = self.buildings.get(id) else { return 0 };
        let tiles: Vec<i32> = b.tiles().map(|(x, y)| self.map.fertility.at_or(x, y, 0) as i32 + 1).collect();
        (tiles.iter().sum::<i32>() / tiles.len().max(1) as i32 + 1 + self.irrigation_bonus(id)).min(99)
    }

    /// A day's crop growth at `fertility` with `workers` tending it.
    fn crop_growth(fertility: i32, workers: i32) -> i32 {
        ((fertility as f64 * GROWTH_PER_FERTILITY / 99.0 * workers as f64 * 10.0) as i32).max(1)
    }

    /// Tick 20, with the industries: crops grow. A floodplain farm counts as fully
    /// staffed while it has labor days left.
    pub(crate) fn grow_crops(&mut self) {
        for id in self.buildings.ids() {
            let Some(b) = self.buildings.get(id) else { continue };
            if !self.is_farm(b.kind) {
                continue;
            }
            let floodplain = self.is_floodplain_farm(id);
            let full = self.workers_needed(b.kind);
            let mothballed = self.defs.building(b.kind).and_then(|d| d.outputs.first()).and_then(|o| self.resource_id(o)).is_some_and(|r| self.is_mothballed(r));
            let fertility = self.fertility(id);
            let Some(b) = self.buildings.get_mut(id) else { continue };
            if floodplain {
                b.workers = if b.labor_days > 0 && !mothballed { full } else { 0 };
            }
            if b.workers > 0 {
                b.progress = (b.progress + Self::crop_growth(fertility, b.workers)).min(PROGRESS_MAX);
            }
        }
    }

    /// Tick 33: floodplain farms use up a day of labor, and meadow crops are brought
    /// in on the first day of their harvest months.
    pub(crate) fn update_farms(&mut self) {
        for id in self.buildings.ids() {
            if self.is_floodplain_farm(id)
                && let Some(b) = self.buildings.get_mut(id)
            {
                b.labor_days = (b.labor_days - 1).max(0);
            }
        }
        // Meadow crops are brought in on the first day of their harvest months.
        if self.time.day == 0 {
            let month = self.time.month;
            for id in self.buildings.ids() {
                let due = self.buildings.get(id).is_some_and(|b| HARVEST_MONTHS.iter().any(|&(k, m)| k == b.kind && m.contains(&month)));
                if due && !self.is_floodplain_farm(id) {
                    self.harvest(id, false);
                }
            }
        }
    }

    /// Brings in a farm's crop as produce, ready to be carted away: 8 per percent of the
    /// crop grown, doubled on the floodplain by Osiris's blessing, and a load of straw
    /// from a grain farm. The floodplain's soil is worn down by the harvest, to a fifth,
    /// or to half when irrigated.
    fn harvest(&mut self, id: BuildingId, floodplain: bool) {
        let Some(b) = self.buildings.get(id) else { return };
        let Some(def) = self.defs.building(b.kind) else { return };
        let outputs: Vec<u16> = def.outputs.iter().filter_map(|k| self.resource_id(k)).collect();
        let Some(&main) = outputs.first() else { return };
        let mut produce = b.progress * 100 / PROGRESS_MAX * 8;
        if floodplain && self.religion.osiris_double_harvest {
            produce *= 2;
        }
        let tiles: Vec<(i32, i32)> = b.tiles().collect();
        let straw = outputs.get(1).copied().filter(|&r| !self.is_mothballed(r));
        let kept = if self.is_irrigated(id) { crate::irrigation::HARVEST_KEPT_IRRIGATED } else { crate::irrigation::HARVEST_KEPT };
        let Some(b) = self.buildings.get_mut(id) else { return };
        b.progress = 0;
        if produce <= 0 {
            return;
        }
        b.stock[main as usize] += produce;
        if let Some(second) = straw {
            b.stock[second as usize] += STRAW_PER_HARVEST;
        }
        if floodplain {
            b.labor_days = 0;
            b.workers = 0;
            for (x, y) in tiles {
                let f = self.map.fertility.at_or(x, y, 0) as i32;
                self.map.fertility.set(x, y, (f * kept / 100).max(1) as u8);
            }
        }
    }

    /// Flood hook: harvest every floodplain farm before the water arrives.
    /// Osiris's doubled harvest is spent on the first floodplain harvest.
    pub(crate) fn harvest_floodplain_farms(&mut self) {
        let mut any = false;
        for id in self.buildings.ids() {
            if self.is_floodplain_farm(id) {
                self.harvest(id, true);
                any = true;
            }
        }
        if any {
            self.religion.osiris_double_harvest = false;
        }
    }

    /// Flood hook: after the flood, floodplain farms start from bare soil.
    pub(crate) fn reset_floodplain_farms(&mut self) {
        for id in self.buildings.ids() {
            if self.is_floodplain_farm(id)
                && let Some(b) = self.buildings.get_mut(id)
            {
                b.progress = 0;
                b.labor_days = 0;
                b.workers = 0;
            }
        }
    }

    /// Tick 31: work camps send peasants to floodplain farms that need tending, and
    /// spare ones to level monument sites. A camp has up to four out at once and sends
    /// the next after a wait that grows as its staff shrinks.
    /// Whether a floodplain farm waits for a peasant, which calls laborers home from a
    /// tomb's site (the original's check at 0x4bf9d0).
    fn farm_needs_peasant(&self) -> bool {
        if self.flood_state() != FloodState::Farmable {
            return false;
        }
        let busy: Vec<u32> = self.figures.iter().filter(|f| f.kind == PEASANT).map(|f| f.target).collect();
        self.buildings
            .iter()
            .any(|b| self.is_farm(b.kind) && b.labor_days <= RELABOR_BELOW && !busy.contains(&b.id) && self.map.terrain_is(b.x, b.y, terrain::FLOODPLAIN))
    }

    pub(crate) fn work_camp_walkers(&mut self) {
        let farmable = self.flood_state() == FloodState::Farmable;
        let camps: Vec<BuildingId> = self.buildings.iter().filter(|b| b.kind == kind::WORK_CAMP).map(|b| b.id).collect();
        let needed = self.workers_needed(kind::WORK_CAMP).max(1);
        for camp in camps {
            let Some(c) = self.buildings.get(camp) else { continue };
            let Some(road) = c.road else { continue };
            let (cx, cy, workers, delay) = (c.x, c.y, c.workers, c.spawn_delay);
            let out = self.figures.iter().filter(|f| f.kind == PEASANT && f.home == camp && !f.dead).count();
            let staffed = workers * 100 / needed;
            let wait = match staffed {
                s if s >= 100 => 3,
                s if s >= 75 => 7,
                s if s >= 50 => 15,
                s if s >= 25 => 29,
                s if s > 0 => 47,
                _ => continue,
            };
            if out >= MAX_PEASANTS_OUT {
                continue;
            }
            // The wait runs until a peasant goes out.
            let waited = (delay + 1).min(wait + 1);
            if let Some(c) = self.buildings.get_mut(camp) {
                c.spawn_delay = waited;
            }
            if waited <= wait {
                continue;
            }
            let busy: Vec<u32> = self.figures.iter().filter(|f| f.kind == PEASANT).map(|f| f.target).collect();
            // Floodplain farms come first; spare laborers level monument sites.
            let farm = self
                .buildings
                .iter()
                .filter(|b| farmable && self.is_farm(b.kind) && b.labor_days <= RELABOR_BELOW && !busy.contains(&b.id))
                .filter(|b| self.map.terrain_is(b.x, b.y, terrain::FLOODPLAIN))
                .min_by_key(|b| (b.x - cx).pow(2) + (b.y - cy).pow(2))
                .map(|b| b.id);
            let Some(farm) = farm else {
                // A royal tomb short of lamps sends a laborer to fetch them.
                if let Some((tomb, yard)) = self.lamp_job((cx, cy)) {
                    let fid = self.figures.spawn(PEASANT, road.0, road.1, Travel::Land);
                    if let Some(c) = self.buildings.get_mut(camp) {
                        c.spawn_delay = 0;
                    }
                    if let Some(f) = self.figures.get_mut(fid) {
                        f.home = camp;
                    }
                    if !self.send_for_lamps(fid, tomb, yard)
                        && let Some(f) = self.figures.get_mut(fid)
                    {
                        f.dead = true;
                    }
                    continue;
                }
                // Levelling a site or dragging a sled of material, whichever monument
                // is nearer.
                let level = self.leveling_job((cx, cy));
                let haul = self.haul_job((cx, cy));
                let dist = |id: BuildingId| self.buildings.get(id).map_or(i32::MAX, |b| (b.x - cx).abs() + (b.y - cy).abs());
                if let Some(job) = haul
                    && level.is_none_or(|(m, _)| dist(job.0) < dist(m))
                {
                    let fid = self.figures.spawn(PEASANT, road.0, road.1, Travel::Land);
                    if let Some(c) = self.buildings.get_mut(camp) {
                        c.spawn_delay = 0;
                    }
                    if let Some(f) = self.figures.get_mut(fid) {
                        f.home = camp;
                    }
                    if !self.send_hauler(fid, job)
                        && let Some(f) = self.figures.get_mut(fid)
                    {
                        f.dead = true;
                    }
                    continue;
                }
                if let Some((monument, block)) = level
                    && let Some(spot) = self.monument_access(monument, (cx, cy))
                {
                    let fid = self.figures.spawn(PEASANT, road.0, road.1, Travel::Land);
                    if let Some(c) = self.buildings.get_mut(camp) {
                        c.spawn_delay = 0;
                    }
                    let map = &self.map;
                    if let Some(f) = self.figures.get_mut(fid) {
                        f.home = camp;
                        f.target = monument;
                        f.amount = block as i32;
                        f.action = 3;
                        if !f.go_to(map, spot) {
                            f.dead = true;
                        }
                    }
                }
                continue;
            };
            let Some(dest) = self.buildings.get(farm).map(|b| b.road.unwrap_or((b.x, b.y))) else { continue };
            let fid = self.figures.spawn(PEASANT, road.0, road.1, Travel::Land);
            if let Some(c) = self.buildings.get_mut(camp) {
                c.spawn_delay = 0;
            }
            let map = &self.map;
            if let Some(f) = self.figures.get_mut(fid) {
                f.home = camp;
                f.target = farm;
                f.action = 1;
                if !f.go_to(map, dest) {
                    f.dead = true;
                }
            }
        }
    }

    /// A laborer goes home straight across country (the original's state 9).
    pub(crate) fn send_laborer_home(&mut self, fid: u32) {
        let Some(home) = self.figures.get(fid).map(|f| f.home) else { return };
        let back = self.buildings.get(home).and_then(|b| b.road);
        let map = &self.map;
        let f = self.figures.get_mut(fid).expect("present");
        f.action = 2;
        f.travel = Travel::Any;
        f.perch = None;
        f.link = 0;
        match back {
            Some(r) if f.go_to(map, r) => {}
            _ => f.dead = true,
        }
    }

    pub(crate) fn update_peasant(&mut self, fid: u32) {
        if self.figures.get(fid).is_some_and(|f| matches!(f.action, 5 | 6)) {
            self.update_lamp_carrier(fid);
            return;
        }
        if self.figures.get(fid).is_some_and(|f| crate::monuments::is_hauling(f.action)) {
            self.update_hauler(fid);
            return;
        }
        let Some(target) = self.figures.get(fid).map(|f| f.target) else { return };
        let tomb = !self.tomb_route(target).is_empty();
        let map = &self.map;
        let Some(f) = self.figures.get_mut(fid) else { return };
        let (act, home, farm, unit) = (f.action, f.home, f.target, f.amount as usize);
        if act == 4 && tomb {
            // Working a tile of a pyramid's or mastaba's site (the original's laborer
            // at 0x4ab47b): after a touch he goes on to the next tile, four touches in
            // all, or home at once if a floodplain farm wants him.
            if !self.level_block(farm, unit) {
                return;
            }
            let f = self.figures.get_mut(fid).expect("present");
            f.counter += 1;
            let next = if f.counter >= SITE_TOUCHES || self.farm_needs_peasant() { None } else { self.next_leveling_block(farm, fid) };
            let tile = next.and_then(|n| self.tomb_unit_tile(farm, n));
            let map = &self.map;
            let f = self.figures.get_mut(fid).expect("present");
            if let (Some(n), Some(tile)) = (next, tile) {
                f.amount = n as i32;
                f.action = 8;
                if !f.go_to(map, tile) {
                    f.dead = true;
                }
                return;
            }
            // He stands a while where he worked, the ticks of his last touch run
            // down again (the original's state 8), then goes home.
            let f = self.figures.get_mut(fid).expect("present");
            f.action = PAUSE;
            f.counter = TILE_WORK as i32;
            f.moving = false;
            return;
        }
        if act == PAUSE {
            f.counter -= 1;
            if f.counter > 0 {
                return;
            }
            self.send_laborer_home(fid);
            return;
        }
        if act == 8 {
            // Crossing a tomb's site to his tile.
            match f.walk(map) {
                Step::Moving => {}
                Step::Arrived => f.action = 4,
                _ => f.dead = true,
            }
            return;
        }
        if act == 4 {
            // Levelling a monument block.
            let block = f.amount as usize;
            if self.level_block(farm, block) {
                // On to the next block nobody is working, while the site needs levelling.
                if let Some(next) = self.next_leveling_block(farm, fid) {
                    self.figures.get_mut(fid).expect("present").amount = next as i32;
                    return;
                }
                let back = self.buildings.get(home).and_then(|b| b.road);
                let map = &self.map;
                let f = self.figures.get_mut(fid).expect("present");
                f.action = 2;
                match back {
                    Some(r) if f.go_to(map, r) => {}
                    _ => f.dead = true,
                }
            }
            return;
        }
        match (act, f.walk(map)) {
            (_, Step::Moving) => {}
            (3, Step::Arrived) => {
                // At a tomb he crosses its site to his tile.
                let tile = if tomb { self.tomb_unit_tile(farm, unit) } else { None };
                let map = &self.map;
                let f = self.figures.get_mut(fid).expect("present");
                match tile {
                    Some(t) if t != (f.x, f.y) => {
                        f.travel = Travel::Any;
                        f.action = 8;
                        if !f.go_to(map, t) {
                            f.dead = true;
                        }
                    }
                    _ => f.action = 4,
                }
            }
            (1, Step::Arrived) => {
                if self.buildings.get(home).is_none() {
                    self.figures.get_mut(fid).expect("present").dead = true;
                    return;
                }
                let full = self.buildings.get(farm).map_or(0, |b| self.workers_needed(b.kind));
                if let Some(b) = self.buildings.get_mut(farm) {
                    b.workers = full;
                    b.labor_days = LABOR_DAYS;
                }
                let back = self.buildings.get(home).and_then(|b| b.road);
                let map = &self.map;
                let f = self.figures.get_mut(fid).expect("present");
                f.action = 2;
                match back {
                    Some(r) if f.go_to(map, r) => {}
                    _ => f.dead = true,
                }
            }
            _ => f.dead = true,
        }
    }

    /// The crop images to draw over a farm: (tile index, image id).
    pub fn crop_overlays(&self, id: BuildingId) -> Vec<(usize, u32)> {
        let Some(b) = self.buildings.get(id) else { return vec![] };
        let Some(crops) = self.defs.building(b.kind).and_then(|d| d.anims.get("crops")).map(|a| a.image) else {
            return vec![];
        };
        let n = if self.map.terrain_is(b.x, b.y, terrain::FLOODPLAIN) { 9 } else { 5 };
        let step = if n >= 9 { 200 } else { 400 };
        (0..n)
            .map(|i| (i, crops + ((b.progress - i as i32 * step) / 100).clamp(0, 5) as u32))
            .collect()
    }

    /// Farm field image for the footprint: fertile-looking farmland on the floodplain.
    pub fn farm_image(&self, id: BuildingId) -> Option<u32> {
        let b = self.buildings.get(id)?;
        let def = self.defs.building(b.kind)?;
        if self.map.terrain_is(b.x, b.y, terrain::FLOODPLAIN) {
            let base = def.anims.get("farmland")?.image;
            Some(base + (self.fertility(id) / 12).clamp(0, 7) as u32)
        } else {
            def.anims.get("farm_house").map(|a| a.image)
        }
    }

    /// Whole loads a farm has ready.
    pub fn farm_loads(&self, id: BuildingId, r: u16) -> i32 {
        self.stored(id, r) / LOAD
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crops_grow_by_fertility_and_workers() {
        // Rich soil, full staff: 16 a day, a full crop in about four months.
        assert_eq!(World::crop_growth(99, 10), 16);
        assert_eq!(World::crop_growth(50, 10), 8);
        // Half staff grows half as fast; poor soil still grows a little.
        assert_eq!(World::crop_growth(99, 5), 8);
        assert_eq!(World::crop_growth(2, 1), 1);
    }

    #[test]
    fn every_meadow_farm_has_harvest_months() {
        for k in [100, 101, 102, 103, 104, 105, 196, 224] {
            assert!(HARVEST_MONTHS.iter().any(|&(f, m)| f == k && !m.is_empty() && m.iter().all(|&m| m < 12)));
        }
    }
}
