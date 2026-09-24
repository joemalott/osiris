//! Labor: the workforce is 60% of the working-age citizens, less the nobles' share,
//! handed out to buildings by labor category. In the original a building can only
//! hire once its labor seeker has found houses nearby, and a category's workers are
//! shared among its buildings by how many houses each has found; with
//! `Rules::global_labor_pool` that step is skipped and all share alike.

use crate::buildings::BuildingId;
use crate::world::World;

/// The original's nine categories, in its order.
pub const CATEGORIES: [&str; 9] = [
    "food_production",
    "industry_commerce",
    "entertainment",
    "religion",
    "education",
    "water_health",
    "infrastructure",
    "government",
    "military",
];

/// When workers are short, categories without a priority get this many workers each
/// round, in this order, until none are left.
const ROUNDS: [(usize, i32); 9] = [(0, 4), (1, 3), (6, 2), (3, 3), (5, 1), (8, 2), (7, 1), (2, 1), (4, 1)];
/// Levels from here up are nobles, who don't work.
const NOBLE_LEVEL: u8 = 14;

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Labor {
    pub available: i32,
    pub employed: i32,
    pub needed: i32,
    pub unemployed: i32,
    /// Workers the categories wanted but didn't get.
    #[serde(default)]
    pub shortage: i32,
    /// The player's priority per category (index into `CATEGORIES`): 0 none, 1 first.
    #[serde(default)]
    pub priorities: Vec<u8>,
    /// Workers needed and given, per category.
    #[serde(default)]
    pub by_category: Vec<(i32, i32)>,
}

/// Priority ranks the player can give.
pub const MAX_PRIORITY: u8 = 9;

/// Shares out `available` workers among categories needing `needed`: first those the
/// player ranked, in rank order, each in full; then the rest a few at a time.
fn allot(needed: &[i32], priorities: &[u8], available: i32) -> Vec<i32> {
    let total: i32 = needed.iter().sum();
    if total <= available {
        return needed.to_vec();
    }
    let priority = |ci: usize| priorities.get(ci).copied().unwrap_or(0);
    let mut given = vec![0; needed.len()];
    let mut left = available;
    for rank in 1..=MAX_PRIORITY {
        if left <= 0 {
            break;
        }
        if let Some(ci) = (0..needed.len()).find(|&ci| priority(ci) == rank) {
            given[ci] = needed[ci].min(left);
            left -= given[ci];
        }
    }
    let mut round = 1;
    while left > 0 && round < available {
        for &(ci, n) in &ROUNDS {
            let room = needed.get(ci).copied().unwrap_or(0) - given[ci];
            if priority(ci) != 0 || room <= 0 {
                continue;
            }
            let n = n.min(left).min(room);
            given[ci] += n;
            left -= n;
            if left <= 0 {
                break;
            }
        }
        round += 1;
    }
    given
}

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
        let Some(slot) = p.get_mut(ci) else { return };
        *slot = rank.min(MAX_PRIORITY);
    }

    pub fn workers_needed(&self, kind: u16) -> i32 {
        self.balance.stats(kind).employees
    }

    /// The workforce: 60% of those of working age, times the commoners' share of the
    /// people.
    fn workforce(&self) -> i32 {
        let (mut commoners, mut all) = (0, 0);
        for h in self.buildings.iter().filter_map(|b| b.house.as_ref()).filter(|h| h.population > 0) {
            all += h.population;
            if h.level < NOBLE_LEVEL {
                commoners += h.population;
            }
        }
        let share = if all > 0 { commoners * 100 / all } else { 0 };
        self.census.working_age() * 60 / 100 * share / 100
    }

    /// Tick 25: recompute the workforce and staff buildings.
    pub(crate) fn update_labor(&mut self) {
        // Test runs can staff everything, to try out one system in isolation.
        let available = if self.test_full_staff { i32::MAX / 2 } else { self.workforce() };
        // Buildings that may hire: (id, category, workers wanted, houses found).
        let mut hiring: Vec<(BuildingId, usize, i32, i32)> = Vec::new();
        let mut needed = vec![0; CATEGORIES.len()];
        let mut found = vec![0; CATEGORIES.len()];
        let mut by_category = vec![(0, 0); CATEGORIES.len()];
        let mut staffed: Vec<BuildingId> = Vec::new();
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
            staffed.push(b.id);
            if !self.has_labor_access(b.id) {
                continue;
            }
            by_category[ci].0 += want;
            let houses = if self.rules.global_labor_pool { 1 } else { b.houses_covered.max(1) };
            needed[ci] += want;
            found[ci] += houses;
            hiring.push((b.id, ci, want, houses));
        }
        let given = allot(&needed, &self.labor.priorities, available);
        // Each building gets its share of its category's workers by the houses it has
        // found; what rounding leaves goes to the first that still want more.
        let mut workers: Vec<i32> = vec![0; hiring.len()];
        let mut spare = given.clone();
        for (i, &(_, ci, want, houses)) in hiring.iter().enumerate() {
            let n = if given[ci] >= needed[ci] {
                want
            } else {
                let share = houses * 10000 / found[ci].max(1);
                (given[ci] * share / 100 / 100).min(want)
            };
            workers[i] = n;
            spare[ci] -= n;
        }
        for (i, &(_, ci, want, _)) in hiring.iter().enumerate() {
            let n = (want - workers[i]).min(spare[ci]).max(0);
            workers[i] += n;
            spare[ci] -= n;
        }
        for &id in &staffed {
            if let Some(b) = self.buildings.get_mut(id) {
                b.workers = 0;
            }
        }
        for (i, &(id, ci, _, _)) in hiring.iter().enumerate() {
            by_category[ci].1 += workers[i];
            if let Some(b) = self.buildings.get_mut(id) {
                b.workers = workers[i];
            }
        }
        let total: i32 = needed.iter().sum();
        let employed = total.min(available);
        let shortage = if total > available { (0..CATEGORIES.len()).map(|ci| needed[ci] - given[ci]).sum() } else { 0 };
        let priorities = std::mem::take(&mut self.labor.priorities);
        self.labor = Labor {
            available,
            employed,
            needed: total,
            unemployed: (available - employed).max(0),
            shortage,
            priorities,
            by_category,
        };
        self.unemployment = if available > 0 { self.labor.unemployed * 100 / available } else { 0 };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranked_categories_come_first_then_rounds() {
        let needed = [10, 10, 0, 0, 0, 0, 10, 0, 0];
        // Enough for everyone.
        assert_eq!(allot(&needed, &[], 40), needed.to_vec());
        // Short: food 4, industry 3, infrastructure 2 a round.
        assert_eq!(allot(&needed, &[], 9), vec![4, 3, 0, 0, 0, 0, 2, 0, 0]);
        assert_eq!(allot(&needed, &[], 12), vec![7, 3, 0, 0, 0, 0, 2, 0, 0]);
        // Infrastructure ranked first is filled before the rounds.
        let mut p = vec![0; 9];
        p[6] = 1;
        assert_eq!(allot(&needed, &p, 12), vec![2, 0, 0, 0, 0, 0, 10, 0, 0]);
    }
}
