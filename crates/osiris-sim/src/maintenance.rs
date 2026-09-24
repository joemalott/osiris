//! Fire and collapse. Once a day every building's collapse risk grows by its model
//! value, and on roughly one day in eight its fire risk grows by one to five times its
//! model value; at 1000 it collapses into rubble or burns. Passing firemen and
//! architects wear these risks down (see `services`). Burning ruins smoulder for 25 to
//! 32 days, may spread fire with the wind, then turn to rubble.

use crate::buildings::{Building, BuildingId, kind};
use crate::figures::Travel;
use crate::map::{NEIGHBOURS, terrain};
use crate::people::figure_kind::HOMELESS;
use crate::missions::Condition;
use crate::world::World;

const THRESHOLD: i32 = 1000;
/// A burning ruin starts 1-8 days old and is rubble at this age.
const RUIN_AGE: i32 = 33;
/// Monuments and tombs fire never spreads to.
const FIRE_SPREAD_EXEMPT: [u16; 17] = [183, 207, 210, 213, 215, 218, 219, 220, 222, 225, 227, 228, 229, 230, 234, 235, 236];

impl World {
    fn fire_proof(&self, b: &Building) -> bool {
        let def = self.defs.building(b.kind);
        def.is_some_and(|d| d.int("fire_proof").unwrap_or(0) != 0) || b.kind == kind::BURNING_RUIN
    }

    /// Tick 44. Every building's collapse risk grows by its model value (huts and
    /// anything on the floodplain stay at 0) and it collapses at 1000. Otherwise a
    /// fresh draw gives a multiplier of 1-5, and on the one day in eight its hash
    /// matches, fire risk grows by the model value times it (plus 3 in the desert);
    /// an empty house or a building with no fire risk goes back to 0. It burns at 1000.
    /// Only a building's main part is checked (a fort's parade ground is not).
    pub(crate) fn check_fire_and_collapse(&mut self) {
        self.rng.next();
        let global = self.rng.byte() & 7;
        let desert = self.climate == 2;
        let mut collapse = Vec::new();
        let mut burn = Vec::new();
        for id in self.buildings.ids() {
            let Some(b) = self.buildings.get(id) else { continue };
            if matches!(b.kind, kind::BURNING_RUIN | crate::military::FORT_GROUND) {
                continue;
            }
            let stats = self.balance.stats(b.kind);
            let on_floodplain = self.map.terrain_is(b.x, b.y, terrain::FLOODPLAIN);
            let hashed = (b.id as i32 + self.map.random.at_or(b.x, b.y, 0) as i32) & 7;
            let (rules_fire, rules_collapse) = (self.rules.fire, self.rules.collapse);
            let Some(b) = self.buildings.get_mut(id) else { continue };
            if rules_collapse {
                b.damage_risk += stats.damage_risk;
                if b.house.as_ref().is_some_and(|h| h.level < 2) || on_floodplain {
                    b.damage_risk = 0;
                }
                if b.damage_risk >= THRESHOLD {
                    collapse.push(id);
                    continue;
                }
            }
            self.rng.next();
            let times = self.rng.short() % 5 + 1;
            if !rules_fire || hashed != global {
                continue;
            }
            match &b.house {
                Some(h) if h.population < 1 => b.fire_risk = 0,
                Some(_) => b.fire_risk += stats.fire_risk * times,
                None if stats.fire_risk == 0 => b.fire_risk = 0,
                None => b.fire_risk += stats.fire_risk * times + if desert { 3 } else { 0 },
            }
            if b.fire_risk >= THRESHOLD {
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
            // Nothing is left standing in the river.
            if self.map.terrain_is(x, y, terrain::WATER) {
                continue;
            }
            // A venue's plaza is road underneath; the road survives.
            if self.map.terrain_is(x, y, terrain::ROAD) {
                let (mut rules, map) = self.tile_rules();
                rules.roads_in(map, x - 1, y - 1, x + 1, y + 1);
                continue;
            }
            if by_fire {
                let ruin = self.create_building(kind::BURNING_RUIN, x, y);
                self.rng.next();
                let age = (self.rng.byte() & 7) + 1;
                let variant = (self.map.random.at_or(x, y, 0) & 3) as u32;
                let base = self.defs.building(kind::BURNING_RUIN).map_or(0, |d| d.image);
                if let Some(r) = self.buildings.get_mut(ruin) {
                    r.progress = age;
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

    /// Tick 43: ruins age a day and are rubble at 33 days. Every eighth day (fourth in
    /// the desert) one in four spreads fire downwind: to the building the year's wind
    /// points at, or failing that one either side of it. Fire-proof buildings,
    /// monuments and tombs don't catch.
    pub(crate) fn update_burning_ruins(&mut self) {
        let interval = if self.climate == 2 { 3 } else { 7 };
        let ruins: Vec<BuildingId> = self.buildings.iter().filter(|b| b.kind == kind::BURNING_RUIN).map(|b| b.id).collect();
        for id in ruins {
            let Some(r) = self.buildings.get_mut(id) else { continue };
            r.progress += 1;
            let (x, y, age) = (r.x, r.y, r.progress);
            if age >= RUIN_AGE {
                self.demolish(id);
                self.make_rubble(x, y);
                continue;
            }
            if !self.rules.fire || age & interval != 0 {
                continue;
            }
            self.rng.next();
            if (self.map.random.at_or(x, y, 0) as i32 & 3) != (self.rng.short() & 3) {
                continue;
            }
            let wind = self.wind as usize;
            for d in [wind, (wind + 7) % 8, (wind + 1) % 8] {
                let (dx, dy) = NEIGHBOURS[d];
                let target = self.map.building.at_or(x + dx, y + dy, 0);
                if target == 0 || target == id {
                    continue;
                }
                let Some(t) = self.buildings.get(target) else { continue };
                if self.fire_proof(t) || FIRE_SPREAD_EXEMPT.contains(&t.kind) {
                    continue;
                }
                self.wreck(target, true);
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::world::{Command, Outcome, World};

    fn sandbox() -> Option<World> {
        let data = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../PharaohData");
        if !data.is_dir() {
            return None;
        }
        let library = osiris_formats::ImageLibrary::open(&data.join("Data")).expect("open image library");
        let scenario = osiris_formats::Scenario::load_map(&data.join("Maps/Sandbox.map")).expect("load map");
        let defs = std::sync::Arc::new(crate::defs::Defs::load(&library).expect("load defs"));
        let model_text = std::fs::read(data.join("Pharaoh_Model_Normal.txt")).expect("read model");
        let model = osiris_formats::Model::parse(&String::from_utf8_lossy(&model_text)).expect("parse model");
        let balance = std::sync::Arc::new(crate::balance::Balance::from_model(&model));
        let mut world = World::new(&scenario, defs, balance);
        world.start(&scenario);
        Some(world)
    }

    /// On the days its hash comes up, a building's fire risk grows by one to five times
    /// its model value.
    #[test]
    fn fire_risk_grows_by_one_to_five_times_the_model_value() {
        let Some(mut world) = sandbox() else { return };
        world.rules.fire = true;
        world.rules.collapse = true;
        let k = crate::buildings::kind::BAZAAR;
        assert!(matches!(world.apply(&Command::Build { kind: k, x: 160, y: 127, x1: 160, y1: 127 }), Outcome::Done { .. }));
        let id = world.map.building.at_or(160, 127, 0);
        let inc = world.balance.stats(k).fire_risk;
        assert!(inc > 0);
        let mut steps = Vec::new();
        for _ in 0..400 {
            let before = world.buildings.get(id).expect("standing").fire_risk;
            world.check_fire_and_collapse();
            let Some(b) = world.buildings.get_mut(id) else { break };
            if b.fire_risk != before {
                steps.push(b.fire_risk - before);
            }
            b.fire_risk = 0;
        }
        assert!(steps.len() > 20, "{steps:?}");
        assert!(steps.iter().all(|&s| s % inc == 0 && (inc..=5 * inc).contains(&s)), "{steps:?}");
        assert!(steps.iter().any(|&s| s > inc), "{steps:?}");
    }

    /// Burning ruins turn to rubble after 25 to 32 days.
    #[test]
    fn ruins_burn_for_25_to_32_days() {
        let Some(mut world) = sandbox() else { return };
        world.rules.fire = false;
        let k = crate::buildings::kind::BAZAAR;
        assert!(matches!(world.apply(&Command::Build { kind: k, x: 160, y: 127, x1: 160, y1: 127 }), Outcome::Done { .. }));
        let id = world.map.building.at_or(160, 127, 0);
        world.wreck(id, true);
        let ruin = world.map.building.at_or(160, 127, 0);
        assert_eq!(world.buildings.get(ruin).map(|b| b.kind), Some(crate::buildings::kind::BURNING_RUIN));
        let mut days = 0;
        while world.buildings.get(ruin).is_some_and(|b| b.kind == crate::buildings::kind::BURNING_RUIN) {
            world.update_burning_ruins();
            days += 1;
        }
        assert!((25..=32).contains(&days), "{days}");
    }
}
