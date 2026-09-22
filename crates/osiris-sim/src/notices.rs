//! The city's message log. Everything worth telling the player about (fires,
//! collapses, population milestones, flood forecasts) is posted here with the date and,
//! where it has one, the tile it happened on. Urgent or first-time messages also go to
//! `World::messages`, the queue of pop-up dialogs.

use crate::missions::Condition;
use crate::world::World;

/// Messages kept in the log; older ones drop off.
const MAX_NOTICES: usize = 100;

/// Population milestones and their messages, in the order they are reached.
const MILESTONES: [(i32, &str); 10] = [
    (100, "message_population_milestone_100"),
    (500, "message_population_milestone_500"),
    (1000, "message_population_milestone_1000"),
    (2000, "message_population_milestone_2000"),
    (3000, "message_population_milestone_3000"),
    (5000, "message_population_milestone_5000"),
    (10000, "message_population_milestone_10000"),
    (15000, "message_population_milestone_15000"),
    (20000, "message_population_milestone_20000"),
    (25000, "message_population_milestone_25000"),
];

/// Messages about the same kind of trouble pop up at most once in this many months;
/// in between they only go to the log.
const POPUP_COOLDOWN_MONTHS: i32 = 2;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Notice {
    /// A message key from message_keys.toml.
    pub key: String,
    pub month: u32,
    pub year: i32,
    /// Where it happened, for "go to problem".
    pub tile: Option<(i32, i32)>,
    pub read: bool,
}

impl Notice {
    /// Months since the start of year 0, for comparing dates.
    fn stamp(&self) -> i32 {
        self.year * 12 + self.month as i32
    }
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Notices {
    pub log: Vec<Notice>,
    /// The highest population milestone announced so far.
    pub milestone: i32,
}

impl World {
    fn month_stamp(&self) -> i32 {
        self.time.year * 12 + self.time.month as i32
    }

    /// Adds a message to the log, and to the pop-up queue if `popup`.
    pub fn post(&mut self, key: &str, tile: Option<(i32, i32)>, popup: bool) {
        self.notices.log.push(Notice {
            key: key.to_owned(),
            month: self.time.month,
            year: self.time.year,
            tile,
            read: popup,
        });
        if self.notices.log.len() > MAX_NOTICES {
            self.notices.log.remove(0);
        }
        if popup {
            self.messages.push_back(key.to_owned());
        }
    }

    /// Posts a message about trouble at a tile. It pops up unless the same message
    /// popped up recently, or a tutorial step is about to explain it instead.
    pub(crate) fn post_trouble(&mut self, key: &str, tile: (i32, i32), tutorial: Condition) {
        let now = self.month_stamp();
        let recent = self
            .notices
            .log
            .iter()
            .any(|n| n.key == key && n.read && now - n.stamp() < POPUP_COOLDOWN_MONTHS);
        let explained = self
            .mission
            .as_ref()
            .is_some_and(|m| m.unlocks.iter().any(|u| !u.done && u.when == tutorial));
        self.post(key, Some(tile), !recent && !explained);
    }

    /// Daily: announce population milestones.
    pub(crate) fn check_milestones(&mut self) {
        let next = MILESTONES.iter().find(|&&(n, _)| n > self.notices.milestone && self.population >= n);
        if let Some(&(n, key)) = next {
            self.notices.milestone = n;
            self.post(key, None, true);
        }
    }

    /// Log entries with a tile, newest first.
    pub fn problems(&self) -> impl Iterator<Item = &Notice> {
        self.notices.log.iter().rev().filter(|n| n.tile.is_some())
    }

    pub fn unread_notices(&self) -> usize {
        self.notices.log.iter().filter(|n| !n.read).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn milestones_are_reached_in_order() {
        let (n, key) = MILESTONES[0];
        assert_eq!(n, 100);
        assert!(key.ends_with("_100"));
        assert!(MILESTONES.windows(2).all(|w| w[0].0 < w[1].0));
    }
}
