//! Labor: the workforce is 60% of working-age citizens, handed out to buildings by
//! labor category. In the original a building can only hire once its labor seeker
//! has found houses nearby; with `Rules::global_labor_pool` that step is skipped.

use crate::buildings::BuildingId;
use crate::world::World;

pub const CATEGORIES: [&str; 11] = [
    "food_production",
    "industry_commerce",
    "entertainment",
    "religion",
    "education",
    "water_health",
    "infrastructure",
    "government",
    "military",
    "culture",
    "storage",
];

/// Workers handed to each category per allocation round when workers are short.
fn default_priority(category: &str) -> i32 {
    match category {
        "food_production" => 4,
        "infrastructure" | "government" | "storage" => 3,
        "military" | "industry_commerce" => 2,
        _ => 1,
    }
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Labor {
    pub available: i32,
    pub employed: i32,
    pub needed: i32,
    pub unemployed: i32,
    /// The player's priority per category (index into `CATEGORIES`): 0 none, 1 first.
    #[serde(default)]
    pub priorities: Vec<u8>,
    /// Workers needed and given, per category.
    #[serde(default)]
    pub by_category: Vec<(i32, i32)>,
}

/// Priority ranks the player can give.
pub const MAX_PRIORITY: u8 = 9;

impl World {
    /// Whether building `id` currently has access to labor.
    pub fn has_labor_access(&self, id: BuildingId) -> bool {
        let Some(b) = self.buildings.get(id) else { return false };
        b.road.is_some() && (self.rules.global_labor_pool || b.houses_covered > 0)
    }

    /// Gives category `ci` priority `rank` (0 removes it). A rank already held by
    /// another category moves to this one.
    pub fn set_labor_priority(&mut self, ci: usize, rank: u8) {
        let p = &mut self.labor.priorities;
        if p.len() < CATEGORIES.len() {
            p.resize(CATEGORIES.len(), 0);
        }
        if rank > 0 {
            p.iter_mut().filter(|r| **r == rank).for_each(|r| *r = 0);
        }
        p[ci] = rank.min(MAX_PRIORITY);
    }

    pub fn workers_needed(&self, kind: u16) -> i32 {
        self.balance.stats(kind).employees
    }

    /// Tick 25: recompute the workforce and staff buildings.
    pub(crate) fn update_labor(&mut self) {
        // Test runs can staff everything, to try out one system in isolation.
        let available = if self.test_full_staff { i32::MAX / 2 } else { self.census.working_age() * 60 / 100 };
        // Buildings that want workers, grouped by category, in building order.
        let mut groups: Vec<(usize, Vec<(BuildingId, i32)>)> = Vec::new();
        let mut needed = 0;
        for b in self.buildings.iter() {
            if b.is_house() {
                continue;
            }
            let want = self.workers_needed(b.kind);
            // Floodplain farms are worked by peasants from work camps instead.
            if want <= 0 || self.is_floodplain_farm(b.id) {
                continue;
            }
            let Some(cat) = self.defs.building(b.kind).and_then(|d| d.labor.as_deref()) else { continue };
            let Some(ci) = CATEGORIES.iter().position(|c| *c == cat) else { continue };
            needed += want;
            let access = self.has_labor_access(b.id);
            let entry = (b.id, if access { want } else { 0 });
            match groups.iter_mut().find(|(c, _)| *c == ci) {
                Some((_, v)) => v.push(entry),
                None => groups.push((ci, vec![entry])),
            }
        }
        let wanted: i32 = groups.iter().flat_map(|(_, v)| v.iter().map(|&(_, w)| w)).sum();
        // Per-category allotment.
        let mut allot: Vec<i32> = vec![0; groups.len()];
        let caps: Vec<i32> = groups.iter().map(|(_, v)| v.iter().map(|&(_, w)| w).sum()).collect();
        if wanted <= available {
            allot.clone_from(&caps);
        } else {
            let mut left = available;
            // Categories the player ranked are staffed in full first, in rank order.
            let priority = |ci: usize| self.labor.priorities.get(ci).copied().unwrap_or(0);
            for rank in 1..=MAX_PRIORITY {
                for (i, (ci, _)) in groups.iter().enumerate() {
                    if priority(*ci) == rank {
                        let n = caps[i].min(left);
                        allot[i] = n;
                        left -= n;
                    }
                }
            }
            while left > 0 {
                let mut gave = false;
                for (i, (ci, _)) in groups.iter().enumerate() {
                    let room = caps[i] - allot[i];
                    if room <= 0 || left <= 0 || priority(*ci) > 0 {
                        continue;
                    }
                    let n = default_priority(CATEGORIES[*ci]).min(room).min(left);
                    allot[i] += n;
                    left -= n;
                    gave = true;
                }
                if !gave {
                    break;
                }
            }
        }
        let mut employed = 0;
        for (i, (_, v)) in groups.iter().enumerate() {
            let mut left = allot[i];
            for &(id, want) in v {
                let n = want.min(left);
                left -= n;
                employed += n;
                if let Some(b) = self.buildings.get_mut(id) {
                    b.workers = n;
                }
            }
        }
        let mut by_category = vec![(0, 0); CATEGORIES.len()];
        for b in self.buildings.iter().filter(|b| !b.is_house()) {
            let want = self.workers_needed(b.kind);
            let cat = self.defs.building(b.kind).and_then(|d| d.labor.as_deref());
            if want <= 0 || self.is_floodplain_farm(b.id) {
                continue;
            }
            if let Some(ci) = cat.and_then(|c| CATEGORIES.iter().position(|x| *x == c)) {
                by_category[ci].0 += want;
                by_category[ci].1 += b.workers;
            }
        }
        let priorities = std::mem::take(&mut self.labor.priorities);
        self.labor = Labor {
            available,
            employed,
            needed,
            unemployed: (available - employed).max(0),
            priorities,
            by_category,
        };
        self.unemployment = if available > 0 { self.labor.unemployed * 100 / available } else { 0 };
    }
}
