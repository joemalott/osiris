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
    /// People leaving per migration update at -100% attraction.
    #[serde(default = "twelve")]
    pub max_leavers: i32,
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

fn twelve() -> i32 {
    12
}

/// A group of `n` movers, or of `n` plus the `queue` once that makes `min`; fewer
/// wait in the queue.
fn batch(queue: &mut i32, n: i32, min: i32) -> Option<i32> {
    if n <= 0 {
        return None;
    }
    if n >= min {
        return Some(n);
    }
    let total = n + *queue;
    if total < min {
        *queue = total;
        return None;
    }
    *queue = 0;
    Some(total)
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
            max_leavers: int(m, "max_leftovers_per_update"),
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

    /// Migration pressure from sentiment. Unemployment works through sentiment, as in
    /// Caesar III; the direct unemployment table is kept but not applied, because it
    /// would stop all immigration in a city that has no jobs yet (Pharaoh's tutorial).
    pub fn percentage(&self, sentiment: i32, _unemployment: i32) -> i32 {
        let pick = |t: &[(i32, i32)], v: i32| t.iter().find(|&&(above, _)| v > above).map_or(0, |&(_, p)| p);
        pick(&self.sentiment_table, sentiment)
    }
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Migration {
    pub queue: i32,
    /// Leavers waiting to make up a group.
    #[serde(default)]
    pub emigration_queue: i32,
    /// Updates after people arrive during which no one leaves, and the reverse.
    #[serde(default)]
    pub immigration_cooldown: i32,
    #[serde(default)]
    pub emigration_cooldown: i32,
    /// -100..=100: how attractive the city is right now.
    pub percentage: i32,
    pub room_in_houses: i32,
    pub newcomers_this_month: i32,
}

impl World {
    fn house_room(&self, id: BuildingId) -> i32 {
        let Some(b) = self.buildings.get(id) else { return 0 };
        let Some(h) = &b.house else { return 0 };
        // A plagued house takes in no one.
        if h.plague_days > 0 {
            return 0;
        }
        self.house_capacity(id) - h.population - h.incoming
    }

    /// Tick 22: room left in all houses.
    /// Room for more people in the city's houses.
    pub fn housing_room(&self) -> i32 {
        self.migration.room_in_houses
    }

    pub(crate) fn update_room(&mut self) {
        let ids: Vec<BuildingId> = self.buildings.iter().filter(|b| b.is_house()).map(|b| b.id).collect();
        self.migration.room_in_houses = ids.iter().map(|&id| self.house_room(id).max(0)).sum();
    }

    /// Tick 23: people arrive or leave by the city's sentiment, twelve at most per day,
    /// in groups of at least four (smaller numbers wait for more). Leaving stops
    /// arrivals for the next two updates and the reverse; no one leaves a town of 100
    /// or fewer, and no one comes while more than three invaders are in the city.
    pub(crate) fn update_migration(&mut self) {
        let p = self.migration_params.clone();
        let mut pct = p.percentage(self.sentiment, self.unemployment);
        let invaders = self.figures.iter().filter(|f| crate::invasions::is_invader_kind(f.kind)).count();
        if self.population >= 200_000 || (pct > 0 && invaders > 3) {
            pct = 0;
        }
        if pct > 0
            && let Some(cap) = self.population_cap()
            && self.population >= cap
        {
            pct = 0;
        }
        self.migration.percentage = pct;
        let m = &mut self.migration;
        if pct > 0 {
            if m.emigration_cooldown > 0 {
                m.emigration_cooldown -= 1;
                return;
            }
            m.immigration_cooldown = 2;
            if let Some(n) = batch(&mut m.queue, p.max_newcomers * pct / 100, p.min_batch) {
                self.create_immigrants(n);
            }
        } else if pct < 0 {
            if m.immigration_cooldown > 0 {
                m.immigration_cooldown -= 1;
                return;
            }
            if self.population <= 100 {
                return;
            }
            m.emigration_cooldown = 2;
            if let Some(n) = batch(&mut m.emigration_queue, p.max_leavers * -pct / 100, p.min_batch) {
                self.create_emigrants(n);
            }
        }
    }

    /// Sends immigrant walkers to houses reachable by road that have no one on the way
    /// yet: vacant lots first, up to four each, then houses with room for more than
    /// seven, four each, then any room, filling it. Whoever finds no room stays away.
    fn create_immigrants(&mut self, mut people: i32) {
        let entry = self.entry_point;
        if !self.map.contains(entry.0, entry.1) {
            return;
        }
        let per = self.migration_params.per_walker;
        let houses: Vec<BuildingId> = self.buildings.iter().filter(|b| b.is_house()).map(|b| b.id).collect();
        for pass in 0..3 {
            for &id in &houses {
                if people <= 0 {
                    return;
                }
                let room = self.house_room(id);
                let Some(b) = self.buildings.get(id) else { continue };
                let Some(h) = &b.house else { continue };
                let fits = match pass {
                    0 => h.population <= 0,
                    1 => room > self.migration_params.first_pass_room,
                    _ => true,
                };
                if room <= 0 || h.incoming > 0 || !fits {
                    continue;
                }
                let n = if pass == 2 { people.min(room) } else { people.min(per).min(room) };
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
            }
        }
    }

    /// Leavers come from the humblest houses first, up to four from each, and only from
    /// huts up to apartments; a house they empty becomes a vacant lot.
    pub(crate) fn create_emigrants(&mut self, mut people: i32) {
        let houses: Vec<BuildingId> = self.buildings.iter().filter(|b| b.is_house()).map(|b| b.id).collect();
        for level in 0..=9u8 {
            for &id in &houses {
                if people <= 0 {
                    return;
                }
                let Some(b) = self.buildings.get_mut(id) else { continue };
                let (x, y) = b.road.unwrap_or((b.x, b.y));
                let Some(h) = b.house.as_mut().filter(|h| h.population > 0 && h.level == level) else { continue };
                let n = h.population.min(4).min(people);
                h.population -= n;
                let emptied = h.population <= 0;
                self.population -= n;
                self.census.remove(&self.rng, n);
                if emptied {
                    self.make_vacant_lot(id);
                }
                let fid = self.figures.spawn(figure_kind::EMIGRANT, x, y, Travel::Land);
                if let Some(f) = self.figures.get_mut(fid) {
                    f.amount = n;
                }
                people -= n;
            }
        }
    }

    /// The mission's population cap, while it applies. Some tutorials lift it once the
    /// player reaches a step (a granary in mission 1, pottery in mission 2).
    pub fn population_cap(&self) -> Option<i32> {
        let m = self.mission.as_ref()?;
        let cap = m.population_cap?;
        let lifted = match m.id {
            1 => self.buildings.count_of(kind::GRANARY) > 0,
            2 => self.buildings.iter().map(|b| b.stock.get(13).copied().unwrap_or(0)).sum::<i32>() >= 100,
            _ => false,
        };
        (!lifted).then_some(cap)
    }

    /// Adds people to a house (as many as fit), turning a vacant lot into a hut.
    /// Returns how many moved in.
    pub fn add_people(&mut self, id: BuildingId, n: i32) -> i32 {
        let Some(h) = self.buildings.get(id).and_then(|b| b.house.as_ref()) else { return 0 };
        if h.population <= 0 {
            self.make_vacant_lot(id);
        }
        let room = self.house_room(id).max(0);
        let n = n.min(room);
        let Some(h) = self.buildings.get_mut(id).and_then(|b| b.house.as_mut()) else { return 0 };
        if n <= 0 {
            return 0;
        }
        h.population += n;
        self.population += n;
        self.census.add(&self.rng, n);
        self.migration.newcomers_this_month += n;
        n
    }

    /// Picks the image for a house at `level` and applies it.
    pub fn set_house_level(&mut self, id: BuildingId, level: u8) {
        let Some(b) = self.buildings.get_mut(id) else { return };
        let Some(h) = b.house.as_mut() else { return };
        h.level = level;
        b.kind = kind::HOUSE_FIRST + level as u16;
        let def = self.defs.building(b.kind);
        let merged = h.merged;
        let image = def
            .and_then(|d| {
                let v = if merged { &d.variants_merged } else { &d.variants };
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_groups_wait_for_four() {
        let mut queue = 0;
        assert_eq!(batch(&mut queue, 1, 4), None);
        assert_eq!(batch(&mut queue, 2, 4), None);
        assert_eq!(queue, 3);
        assert_eq!(batch(&mut queue, 6, 4), Some(6));
        assert_eq!(queue, 3);
        assert_eq!(batch(&mut queue, 1, 4), Some(4));
        assert_eq!(queue, 0);
        assert_eq!(batch(&mut queue, 0, 4), None);
    }

    #[test]
    fn sentiment_sets_the_migration_percentage() {
        let p = MigrationParams::default();
        let pct: Vec<i32> = [71, 70, 61, 60, 50, 49, 41, 40, 31, 30, 21, 20, 0].iter().map(|&s| p.percentage(s, 0)).collect();
        assert_eq!(pct, vec![100, 75, 75, 50, 50, 0, 0, -10, -10, -25, -25, -50, -50]);
    }
}
