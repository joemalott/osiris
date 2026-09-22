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
}

impl World {
    /// Whether building `id` currently has access to labor.
    pub fn has_labor_access(&self, id: BuildingId) -> bool {
        let Some(b) = self.buildings.get(id) else { return false };
        b.road.is_some() && (self.rules.global_labor_pool || b.houses_covered > 0)
    }

    pub fn workers_needed(&self, kind: u16) -> i32 {
        self.balance.stats(kind).employees
    }

    /// Tick 25: recompute the workforce and staff buildings.
    pub(crate) fn update_labor(&mut self) {
        let available = self.census.working_age() * 60 / 100;
        // Buildings that want workers, grouped by category, in building order.
        let mut groups: Vec<(usize, Vec<(BuildingId, i32)>)> = Vec::new();
        let mut needed = 0;
        for b in self.buildings.iter() {
            if b.is_house() {
                continue;
            }
            let want = self.workers_needed(b.kind);
            if want <= 0 {
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
        if wanted <= available {
            for (i, (_, v)) in groups.iter().enumerate() {
                allot[i] = v.iter().map(|&(_, w)| w).sum();
            }
        } else {
            let caps: Vec<i32> = groups.iter().map(|(_, v)| v.iter().map(|&(_, w)| w).sum()).collect();
            let mut left = available;
            while left > 0 {
                let mut gave = false;
                for (i, (ci, _)) in groups.iter().enumerate() {
                    let room = caps[i] - allot[i];
                    if room <= 0 || left <= 0 {
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
        self.labor = Labor {
            available,
            employed,
            needed,
            unemployed: (available - employed).max(0),
        };
        self.unemployment = if available > 0 { self.labor.unemployed * 100 / available } else { 0 };
    }
}
