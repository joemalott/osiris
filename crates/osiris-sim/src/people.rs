//! Migration: immigrants walking in to houses with room, emigrants leaving.

use crate::buildings::{BuildingId, kind};
use crate::figures::{Step, Travel};
use crate::world::World;

pub mod figure_kind {
    pub const IMMIGRANT: u16 = 1;
    pub const EMIGRANT: u16 = 2;
    pub const HOMELESS: u16 = 3;
}

/// Tunables for migration, from `balance.toml`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MigrationParams {
    /// People arriving per migration update at 100% attraction.
    pub max_newcomers: i32,
    /// Smallest group sent at once; smaller batches wait in a queue.
    pub min_batch: i32,
    /// People per immigrant walker.
    pub per_walker: i32,
    /// Houses with more room than this are served first.
    pub first_pass_room: i32,
    /// (above, percent): sentiment above `above` gives `percent`; first match wins.
    pub sentiment_table: Vec<(i32, i32)>,
    pub unemployment_table: Vec<(i32, i32)>,
}

impl Default for MigrationParams {
    fn default() -> Self {
        Self::from_balance(&crate::balance::data())
    }
}

impl MigrationParams {
    pub fn from_balance(t: &toml::Table) -> Self {
        let m = &t["migration"];
        let int = |v: &toml::Value, k: &str| v.get(k).and_then(|x| x.as_integer()).unwrap_or(0) as i32;
        let table = |k: &str| -> Vec<(i32, i32)> {
            m.get(k)
                .and_then(|x| x.as_array())
                .map(|a| a.iter().map(|e| (int(e, "above"), int(e, "percent"))).collect())
                .unwrap_or_default()
        };
        Self {
            max_newcomers: int(m, "max_newcomers_per_update"),
            min_batch: int(m, "max_immigration_amount_per_batch"),
            per_walker: m["house_assignment"]
                .get("max_people_per_house_per_batch")
                .and_then(|x| x.as_integer())
                .unwrap_or(4) as i32,
            first_pass_room: int(&m["house_assignment"], "pass_1_min_room"),
            sentiment_table: table("sentiment_table"),
            unemployment_table: table("unemployment_table"),
        }
    }

    pub fn percentage(&self, sentiment: i32, unemployment: i32) -> i32 {
        let pick = |t: &[(i32, i32)], v: i32| t.iter().find(|&&(above, _)| v > above).map_or(0, |&(_, p)| p);
        pick(&self.sentiment_table, sentiment) + pick(&self.unemployment_table, unemployment)
    }
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Migration {
    pub queue: i32,
    /// -100..=100: how attractive the city is right now.
    pub percentage: i32,
    pub room_in_houses: i32,
    pub newcomers_this_month: i32,
}

impl World {
    fn house_room(&self, id: BuildingId) -> i32 {
        let Some(b) = self.buildings.get(id) else { return 0 };
        let Some(h) = &b.house else { return 0 };
        let cap = self.balance.house(h.level).max_people * b.size * b.size;
        cap - h.population - h.incoming
    }

    /// Tick 22: room left in all houses.
    pub(crate) fn update_room(&mut self) {
        let ids: Vec<BuildingId> = self.buildings.iter().filter(|b| b.is_house()).map(|b| b.id).collect();
        self.migration.room_in_houses = ids.iter().map(|&id| self.house_room(id).max(0)).sum();
    }

    /// Tick 23: decide how many people arrive and send them.
    pub(crate) fn update_migration(&mut self) {
        let p = self.migration_params.clone();
        let pct = p.percentage(self.sentiment, self.unemployment);
        self.migration.percentage = pct;
        if pct <= 0 || self.migration.room_in_houses <= 0 {
            return;
        }
        let batch = p.max_newcomers * pct / 100;
        let total = batch + self.migration.queue;
        if total < p.min_batch {
            self.migration.queue = total;
            return;
        }
        self.migration.queue = 0;
        let sent = self.create_immigrants(total);
        self.migration.queue += total - sent;
    }

    /// Sends immigrant walkers toward houses with room; returns how many people left.
    fn create_immigrants(&mut self, mut people: i32) -> i32 {
        let entry = self.entry_point;
        if !self.map.contains(entry.0, entry.1) {
            return 0;
        }
        let per = self.migration_params.per_walker;
        let houses: Vec<BuildingId> = self.buildings.iter().filter(|b| b.is_house()).map(|b| b.id).collect();
        let mut sent = 0;
        // Houses with room for a full walker first, then any room.
        for pass in 0..2 {
            for &id in &houses {
                if people <= 0 {
                    return sent;
                }
                let room = self.house_room(id);
                if room <= 0 || (pass == 0 && room <= self.migration_params.first_pass_room) {
                    continue;
                }
                let n = people.min(per).min(room);
                let Some(b) = self.buildings.get(id) else { continue };
                // Immigrants walk to the house's road access and step in from there.
                let Some(target) = b.road else { continue };
                let fid = self.figures.spawn(figure_kind::IMMIGRANT, entry.0, entry.1, Travel::Land);
                let f = self.figures.get_mut(fid).expect("just spawned");
                f.target = id;
                f.amount = n;
                f.counter = 10 + (id as i32 & 0x7f) % 20;
                f.destination = Some(target);
                if let Some(h) = self.buildings.get_mut(id).and_then(|b| b.house.as_mut()) {
                    h.incoming += n;
                }
                people -= n;
                sent += n;
            }
        }
        sent
    }

    /// Adds people to a house (as many as fit), turning a vacant lot into a hut.
    /// Returns how many moved in.
    pub fn add_people(&mut self, id: BuildingId, n: i32) -> i32 {
        let room = self.house_room(id).max(0);
        let n = n.min(room);
        let Some(b) = self.buildings.get_mut(id) else { return 0 };
        let Some(h) = b.house.as_mut() else { return 0 };
        if n <= 0 {
            return 0;
        }
        let was_empty = h.population <= 0;
        h.population += n;
        self.population += n;
        self.migration.newcomers_this_month += n;
        if was_empty {
            self.set_house_level(id, 0);
        }
        n
    }

    /// Picks the image for a house at `level` and applies it.
    pub fn set_house_level(&mut self, id: BuildingId, level: u8) {
        let Some(b) = self.buildings.get_mut(id) else { return };
        let Some(h) = b.house.as_mut() else { return };
        h.level = level;
        b.kind = kind::HOUSE_FIRST + level as u16;
        let def = self.defs.building(b.kind);
        let image = def
            .and_then(|d| {
                let v = &d.variants;
                (!v.is_empty()).then(|| v[(self.map.random.at_or(b.x, b.y, 0) as usize) % v.len()])
            })
            .or_else(|| def.map(|d| d.image))
            .unwrap_or(0);
        self.set_building_image(id, image);
    }

    /// Per-tick behaviour of migrants.
    pub(crate) fn update_migrant(&mut self, fid: u32) {
        let map = &self.map;
        let Some(f) = self.figures.get_mut(fid) else { return };
        match f.kind {
            figure_kind::IMMIGRANT => {
                if f.counter > 0 {
                    f.counter -= 1;
                    return;
                }
                if !f.moving && f.route.is_empty() && f.destination.is_some_and(|d| d != (f.x, f.y)) {
                    let d = f.destination.unwrap();
                    if !f.go_to(map, d) {
                        f.dead = true;
                        return;
                    }
                }
                match f.walk(map) {
                    Step::Moving => {}
                    Step::Arrived => {
                        let (house, n) = (f.target, f.amount);
                        f.dead = true;
                        f.amount = 0;
                        if let Some(h) = self.buildings.get_mut(house).and_then(|b| b.house.as_mut()) {
                            h.incoming = (h.incoming - n).max(0);
                        }
                        let (x, y) = (f.x, f.y);
                        let settled = if self.buildings.get(house).is_some() { self.add_people(house, n) } else { 0 };
                        if settled < n {
                            // No room after all: they wander off to find another home.
                            let fid = self.figures.spawn(figure_kind::HOMELESS, x, y, Travel::Land);
                            if let Some(h) = self.figures.get_mut(fid) {
                                h.amount = n - settled;
                            }
                        }
                    }
                    Step::Blocked | Step::Lost => {
                        let d = f.destination.unwrap_or((f.x, f.y));
                        if !f.go_to(map, d) {
                            f.dead = true;
                        }
                    }
                }
            }
            figure_kind::EMIGRANT | figure_kind::HOMELESS => {
                let exit = self.exit_point;
                if !f.moving && f.route.is_empty() && (f.x, f.y) != exit && !f.go_to(map, exit) {
                    f.dead = true;
                    return;
                }
                if matches!(f.walk(map), Step::Arrived | Step::Lost) {
                    f.dead = true;
                }
            }
            _ => {}
        }
    }
}
