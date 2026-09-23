//! Fire and collapse. Once a day every building's collapse risk grows by its model
//! value, and on roughly one day in eight its fire risk grows too; past 1000 it
//! collapses into rubble or burns. Firemen and architects reset these risks (see
//! `services`). Burning ruins smoulder for a while, may spread fire, then turn to rubble.

use crate::buildings::{Building, BuildingId, kind};
use crate::figures::Travel;
use crate::map::{NEIGHBOURS, terrain};
use crate::people::figure_kind::HOMELESS;
use crate::missions::Condition;
use crate::world::World;

const THRESHOLD: i32 = 1000;

impl World {
    fn fire_proof(&self, b: &Building) -> bool {
        let def = self.defs.building(b.kind);
        def.is_some_and(|d| d.int("fire_proof").unwrap_or(0) != 0) || b.kind == kind::BURNING_RUIN
    }

    /// Tick 44.
    pub(crate) fn check_fire_and_collapse(&mut self) {
        self.rng.next();
        let global = self.rng.byte() & 7;
        let mut collapse = Vec::new();
        let mut burn = Vec::new();
        for id in self.buildings.ids() {
            let Some(b) = self.buildings.get(id) else { continue };
            let stats = self.balance.stats(b.kind);
            let fire_proof = self.fire_proof(b);
            let empty_hut = b.house.as_ref().is_some_and(|h| h.population <= 0 && h.level == 0);
            let hashed = (b.id as i32 + self.map.random.at_or(b.x, b.y, 0) as i32) & 7;
            let (rules_fire, rules_collapse) = (self.rules.fire, self.rules.collapse);
            let Some(b) = self.buildings.get_mut(id) else { continue };
            if rules_collapse && b.kind != kind::BURNING_RUIN {
                b.damage_risk += stats.damage_risk;
                if b.damage_risk > THRESHOLD {
                    collapse.push(id);
                    continue;
                }
            }
            if rules_fire && !fire_proof && !empty_hut && hashed == global {
                b.fire_risk += stats.fire_risk;
            }
            if b.fire_risk > THRESHOLD {
                burn.push(id);
            }
        }
        for id in collapse {
            self.destroy(id, false);
        }
        for id in burn {
            self.destroy(id, true);
        }
    }

    /// Replaces a building with rubble (collapse) or burning ruins (fire).
    pub fn destroy(&mut self, id: BuildingId, by_fire: bool) {
        let Some(b) = self.buildings.get(id).cloned() else { return };
        if by_fire {
            self.post_trouble("message_fire_in_the_city", (b.x, b.y), Condition::Fire);
            self.events.fire = true;
        } else {
            self.post_trouble("message_collapsed_building", (b.x, b.y), Condition::Collapse);
            self.events.collapse = true;
        }
        self.wreck(id, by_fire);
    }

    /// Replaces a building with rubble or burning ruins, without a message.
    pub(crate) fn wreck(&mut self, id: BuildingId, by_fire: bool) {
        let Some(b) = self.buildings.get(id).cloned() else { return };
        let tiles: Vec<(i32, i32)> = b.tiles().collect();
        if let Some(h) = &b.house
            && h.population > 0
        {
            let (x, y) = b.road.unwrap_or((b.x, b.y));
            let fid = self.figures.spawn(HOMELESS, x, y, Travel::Land);
            if let Some(f) = self.figures.get_mut(fid) {
                f.amount = h.population;
            }
        }
        self.demolish(id);
        for &(x, y) in &tiles {
            // A venue's plaza is road underneath; the road survives.
            if self.map.terrain_is(x, y, terrain::ROAD) {
                let (mut rules, map) = self.tile_rules();
                rules.roads_in(map, x - 1, y - 1, x + 1, y + 1);
                continue;
            }
            if by_fire {
                let ruin = self.create_building(kind::BURNING_RUIN, x, y);
                self.rng.next();
                let duration = (self.rng.byte() & 127) + 120;
                let variant = (self.map.random.at_or(x, y, 0) & 3) as u32;
                let base = self.defs.building(kind::BURNING_RUIN).map_or(0, |d| d.image);
                if let Some(r) = self.buildings.get_mut(ruin) {
                    r.progress = duration;
                    r.anim = variant;
                }
                self.set_building_image(ruin, base + 9 * variant);
            } else {
                self.make_rubble(x, y);
            }
        }
    }

    fn make_rubble(&mut self, x: i32, y: i32) {
        self.map.terrain.update(x, y, |t| t | terrain::RUBBLE);
        let image = self.defs.terrain.rubble + (self.map.random.at_or(x, y, 0) as u32 & 7);
        self.map.set_single_image(x, y, image);
    }

    /// Tick 43: ruins burn down and may spread.
    pub(crate) fn update_burning_ruins(&mut self) {
        let desert = self.climate == 2;
        let ruins: Vec<BuildingId> = self.buildings.iter().filter(|b| b.kind == kind::BURNING_RUIN).map(|b| b.id).collect();
        for id in ruins {
            let Some(r) = self.buildings.get(id) else { continue };
            let (x, y, dur) = (r.x, r.y, r.progress);
            if dur <= 0 {
                self.demolish(id);
                self.make_rubble(x, y);
                continue;
            }
            self.rng.next();
            let burn = self.rng.byte() & 15;
            if let Some(r) = self.buildings.get_mut(id) {
                r.progress -= burn;
            }
            let interval = if desert { 3 } else { 7 };
            if !self.rules.fire || dur & interval != 0 {
                continue;
            }
            self.rng.next();
            if self.rng.byte() & 3 != 0 {
                continue;
            }
            let dir = (self.rng.byte_alt() & 7) as usize;
            for d in [dir, (dir + 1) % 8, (dir + 7) % 8] {
                let (dx, dy) = NEIGHBOURS[d];
                let target = self.map.building.at_or(x + dx, y + dy, 0);
                if target == 0 || target == id {
                    continue;
                }
                let Some(t) = self.buildings.get(target) else { continue };
                if self.fire_proof(t) {
                    continue;
                }
                self.destroy(target, true);
                break;
            }
        }
    }
}
