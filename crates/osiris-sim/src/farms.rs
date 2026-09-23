//! Farms. Floodplain farms have no staff of their own: work camps send peasants who
//! tend a farm for 96 days, during which its crop grows by fertility x 0.16 a day. The
//! crop is harvested when the flood approaches. Meadow farms hire like other buildings
//! and grow in proportion to their workers, harvesting when the crop is ripe.

use crate::buildings::{BuildingId, kind};
use crate::economy::LOAD;
use crate::figures::{Step, Travel};
use crate::floods::FloodState;
use crate::map::terrain;
use crate::world::World;

pub const PROGRESS_MAX: i32 = 2000;
const GROWTH_PER_FERTILITY: f32 = 0.16;
const LABOR_DAYS: i32 = 96;
/// Farms ask for peasants again once this many labor days remain.
const RELABOR_BELOW: i32 = 47;
pub const PEASANT: u16 = 35;
/// Peasants a work camp has out at once.
const MAX_PEASANTS_OUT: usize = 4;

impl World {
    pub fn is_farm(&self, k: u16) -> bool {
        self.defs.building(k).is_some_and(|d| d.has_flag("is_farm"))
    }

    pub fn is_floodplain_farm(&self, id: BuildingId) -> bool {
        let Some(b) = self.buildings.get(id) else { return false };
        self.is_farm(b.kind) && self.map.terrain_is(b.x, b.y, terrain::FLOODPLAIN)
    }

    /// Average soil fertility under a building, 0..=100.
    pub fn fertility(&self, id: BuildingId) -> i32 {
        let Some(b) = self.buildings.get(id) else { return 0 };
        let tiles: Vec<i32> = b.tiles().map(|(x, y)| self.map.fertility.at_or(x, y, 0) as i32).collect();
        (tiles.iter().sum::<i32>() / tiles.len().max(1) as i32).min(100)
    }

    /// Tick 33: crops grow.
    pub(crate) fn update_farms(&mut self) {
        for id in self.buildings.ids() {
            let Some(b) = self.buildings.get(id) else { continue };
            if !self.is_farm(b.kind) {
                continue;
            }
            let step = (self.fertility(id) as f32 * GROWTH_PER_FERTILITY) as i32;
            let floodplain = self.is_floodplain_farm(id);
            let Some(b) = self.buildings.get_mut(id) else { continue };
            if floodplain {
                if b.labor_days > 0 {
                    b.progress += step;
                    b.labor_days -= 1;
                }
                if b.labor_days <= 0 {
                    b.workers = 0;
                }
            } else if b.workers > 0 {
                b.progress += (step as f32 * b.workers as f32 / 10.0) as i32;
            }
            b.progress = b.progress.min(PROGRESS_MAX);
        }
        // Ripe meadow crops are harvested straight away.
        for id in self.buildings.ids() {
            let ripe = self
                .buildings
                .get(id)
                .is_some_and(|b| self.is_farm(b.kind) && b.progress >= PROGRESS_MAX && b.walkers[2] == 0);
            if ripe && !self.is_floodplain_farm(id) {
                self.harvest(id, false);
            }
        }
    }

    /// Moves a farm's crop into its store as produce, ready to be carted away.
    fn harvest(&mut self, id: BuildingId, floodplain: bool) {
        let fertility = self.fertility(id);
        let Some(b) = self.buildings.get(id) else { return };
        let Some(def) = self.defs.building(b.kind) else { return };
        let outputs: Vec<u16> = def.outputs.iter().filter_map(|k| self.resource_id(k)).collect();
        let Some(&main) = outputs.first() else { return };
        let mut progress = b.progress;
        if floodplain {
            progress = progress * fertility / 100;
        }
        // The original counts progress in steps of 20.
        let mut produce = ((progress / 20 * 20) as f32 / 2.5) as i32;
        // Osiris's blessing doubles the harvest.
        if self.religion.osiris_double_harvest_days > 0 {
            produce *= 2;
        }
        let Some(b) = self.buildings.get_mut(id) else { return };
        b.progress = 0;
        if produce <= 0 {
            return;
        }
        b.stock[main as usize] += produce;
        if let Some(&second) = outputs.get(1) {
            b.stock[second as usize] += produce / 10;
        }
        if floodplain {
            b.labor_days = 0;
            b.workers = 0;
        }
    }

    /// Flood hook: harvest every floodplain farm before the water arrives.
    pub(crate) fn harvest_floodplain_farms(&mut self) {
        for id in self.buildings.ids() {
            if self.is_floodplain_farm(id) {
                self.harvest(id, true);
            }
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
                if let Some((monument, block)) = self.leveling_job((cx, cy))
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

    pub(crate) fn update_peasant(&mut self, fid: u32) {
        let map = &self.map;
        let Some(f) = self.figures.get_mut(fid) else { return };
        let (act, home, farm) = (f.action, f.home, f.target);
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
                self.figures.get_mut(fid).expect("present").action = 4;
            }
            (1, Step::Arrived) => {
                let camp = self.buildings.get(home).map(|b| (b.x, b.y));
                let Some((cx, cy)) = camp else {
                    self.figures.get_mut(fid).expect("present").dead = true;
                    return;
                };
                if let Some(b) = self.buildings.get_mut(farm) {
                    let dist = (((b.x - cx).pow(2) + (b.y - cy).pow(2)) as f32).sqrt();
                    b.workers = (((1.0 - dist / 20.0) * 12.0) as i32).clamp(2, 10);
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
