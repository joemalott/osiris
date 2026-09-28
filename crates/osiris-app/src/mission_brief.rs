//! What the Custom Missions and Explore History screens tell about the selected
//! scenario: its picture, climate, size, military activity, a rough difficulty and
//! the win conditions, reckoned from the scenario file as the original does.

use osiris_formats::events::EventRecord;
use osiris_formats::scenario::{ScenarioInfo, WinCriteria};
use osiris_formats::{Scenario, TextTable};

const REQUEST: u8 = 1;
const INVASION: u8 = 2;
const GIFT: u8 = 23;
/// Trigger bits: the event only happens when another leads to it; and a flag the
/// original treats as the start of chains that don't count as military activity.
const ONLY_VIA_EVENT: u8 = 0x1;
const CHAIN_ROOT: u8 = 0x10;

#[derive(Debug, Clone, Default)]
pub struct Brief {
    pub name: String,
    pub subtitle: String,
    pub start_year: i32,
    pub image: i16,
    pub climate: u8,
    pub width: i32,
    pub invasions: usize,
    pub challenge: i32,
    pub open_play: bool,
    pub win: WinCriteria,
    pub monuments: [u16; 3],
}

impl Brief {
    pub fn new(name: String, s: &Scenario) -> Self {
        let i = &s.info;
        let invasions = invasion_count(&s.events);
        Self {
            name,
            subtitle: i.subtitle.clone(),
            start_year: i.start_year as i32,
            image: i.image_id,
            climate: i.climate,
            width: i.width,
            invasions,
            challenge: challenge(i, &s.events, invasions),
            open_play: i.is_open_play,
            win: i.win.clone(),
            monuments: i.monuments,
        }
    }

    /// Text 44 line naming the map's size.
    pub fn size_text(&self) -> usize {
        match self.width {
            56 => 121,
            84 => 122,
            112 => 123,
            140 => 124,
            170 => 125,
            _ => 126,
        }
    }

    /// Text 44 line for the military activity.
    pub fn military_text(&self) -> usize {
        match self.invasions {
            0 => 112,
            1..=2 => 113,
            3..=4 => 114,
            5..=10 => 115,
            _ => 116,
        }
    }

    /// Text 32 line for how hard the mission is, "Trivial" to "Virtually impossible".
    pub fn challenge_text(&self) -> usize {
        let n = self.challenge;
        22 + match n {
            ..=100 => 0,
            ..=200 => 1,
            ..=400 => 2,
            ..=600 => 3,
            ..=900 => 4,
            ..=1300 => 5,
            ..=1700 => 6,
            ..=2100 => 7,
            ..=2500 => 8,
            _ => 9,
        }
    }
}

/// Marks event `i` and the only-via-event events it leads to: those a completion
/// starts, those a request or gift's refusal or lateness starts, and down a request's
/// defeat link. The original stops seven links deep.
fn mark(ev: &[EventRecord], seen: &mut [bool], i: usize, depth: u32) {
    if depth >= 7 {
        return;
    }
    let mut depth = depth + 1;
    let mut cur = i;
    loop {
        seen[cur] = true;
        let e = &ev[cur];
        let link = |t: i16| usize::try_from(t).ok().filter(|&t| t < ev.len() && t != cur && ev[t].trigger & ONLY_VIA_EVENT != 0);
        if let Some(t) = link(e.on_completed) {
            mark(ev, seen, t, depth);
        }
        if e.kind == REQUEST || e.kind == GIFT {
            for t in [e.on_refusal, e.on_too_late].into_iter().filter_map(link) {
                mark(ev, seen, t, depth);
            }
        }
        if e.kind != REQUEST || !(1..=2).contains(&e.defeat_link) {
            return;
        }
        let Some(t) = link(e.on_defeat) else { return };
        cur = t;
        depth += 1;
        if depth >= 8 {
            return;
        }
    }
}

/// What a briefing's Objectives panel lists. The original's briefing (FUN_0041a180)
/// reads the scenario's goals (0x784c24 to 0x784c5c): the population, the houses of a
/// level, then the culture, prosperity, monument and kingdom ratings, each on its own
/// label. It names no time limit or years to survive (only the city's corner counts
/// them down, FUN_0051f790); Osiris adds them after the goals, worded as Custom
/// Missions words them (text 44, lines 134 and 135).
#[derive(Debug, Clone, Default)]
pub struct Objectives {
    pub goals: osiris_sim::missions::Goals,
    pub time_limit: Option<i32>,
    pub survival: Option<i32>,
}

impl Objectives {
    pub fn from_win(w: &WinCriteria) -> Self {
        let years = |g: &osiris_formats::scenario::Goal| g.enabled.then_some(g.value).filter(|&y| y > 0);
        Self { goals: osiris_sim::missions::Goals::from_scenario(w), time_limit: years(&w.time_limit), survival: years(&w.survival_time) }
    }

    /// The panel's lines, in the original's order.
    pub fn lines(&self, text: &TextTable) -> Vec<String> {
        let t = |g: usize, i: usize| text.get(g, i).unwrap_or("").trim().to_string();
        let g = &self.goals;
        let mut lines = Vec::new();
        if g.population.enabled {
            lines.push(format!("{} {}", t(62, 11), g.population.value));
        }
        if g.housing_count.value != 0 {
            lines.push(format!("{} {}", g.housing_count.value, t(29, g.housing_level.value.max(0) as usize + 20)));
        }
        for (goal, id) in [(&g.culture, 12), (&g.prosperity, 13), (&g.monuments, 14), (&g.kingdom, 15)] {
            if goal.enabled {
                lines.push(format!("{} {}", t(62, id), goal.value));
            }
        }
        if let Some(y) = self.time_limit {
            lines.push(format!("{y} {}", t(44, 134)));
        }
        if let Some(y) = self.survival {
            lines.push(format!("{y} {}", t(44, 135)));
        }
        lines
    }
}

/// The first five missions teach the game, and their briefings name the tutorial's
/// goal (text 62, FUN_004e2530). Osiris doesn't keep the tutorial's steps as the
/// original does, so it is always the first step's goal, as before the mission.
pub fn tutorial_goal(text: &TextTable, mission: usize) -> Option<String> {
    const GOALS: [usize; 5] = [21, 24, 28, 33, 31];
    GOALS.get(mission).and_then(|&i| text.get(62, i)).map(|s| s.trim().to_owned())
}

/// Invasions that aren't part of a chain begun by a chain-root event.
fn invasion_count(ev: &[EventRecord]) -> usize {
    let mut seen = vec![false; ev.len()];
    for i in 0..ev.len() {
        if ev[i].trigger & CHAIN_ROOT != 0 {
            mark(ev, &mut seen, i, 1);
        }
    }
    ev.iter().zip(&seen).filter(|(e, s)| e.kind == INVASION && !**s).count()
}

/// The original's measure of a mission's difficulty: the raw goal values, with
/// monuments weighing ten times, plus steps for a large population goal, invasions,
/// requests (the first event is never counted) and gods to keep happy.
fn challenge(i: &ScenarioInfo, ev: &[EventRecord], invasions: usize) -> i32 {
    let w = &i.win;
    let mut n = w.kingdom.value + 2 * (w.prosperity.value + 10 * w.monuments.value + w.culture.value);
    if w.population.enabled && w.population.value > 500 {
        n += match w.population.value {
            ..=2000 => 20,
            ..=4000 => 50,
            ..=8000 => 80,
            _ => 100,
        };
    }
    n += match invasions {
        0 => 0,
        1..=2 => 40,
        3..=4 => 60,
        5..=10 => 80,
        _ => 100,
    };
    let requests = ev.iter().skip(1).filter(|e| e.kind == REQUEST).count();
    n += match requests {
        ..=5 => 20,
        6..=10 => 40,
        11..=20 => 60,
        21..=30 => 80,
        _ => 100,
    };
    let gods: i32 = i.gods.iter().map(|&g| if g <= 2 { g as i32 } else { 0 }).sum();
    n += match gods {
        0 => 0,
        1..=2 => 20,
        3..=5 => 50,
        6..=8 => 80,
        _ => 100,
    };
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(kind: u8, trigger: u8) -> EventRecord {
        EventRecord { kind, trigger, on_completed: -1, on_refusal: -1, on_too_late: -1, on_defeat: -1, ..Default::default() }
    }

    #[test]
    fn invasions_led_to_from_a_chain_root_do_not_count() {
        let mut ev = vec![event(REQUEST, CHAIN_ROOT), event(INVASION, ONLY_VIA_EVENT), event(INVASION, 0), event(INVASION, ONLY_VIA_EVENT)];
        ev[0].on_refusal = 1;
        assert_eq!(invasion_count(&ev), 2);
        // Only events that wait for another can be led to.
        ev[0].on_completed = 2;
        assert_eq!(invasion_count(&ev), 2);
        ev[0].on_completed = 3;
        assert_eq!(invasion_count(&ev), 1);
    }

    #[test]
    fn challenge_adds_goals_and_threats() {
        let mut info = ScenarioInfo::default();
        info.win.culture.value = 30;
        info.win.prosperity.value = 20;
        info.win.kingdom.value = 40;
        info.win.population = osiris_formats::scenario::Goal { enabled: true, value: 3000 };
        info.gods = [2, 1, 0, 0, 0];
        let ev = vec![event(REQUEST, 0), event(REQUEST, 0), event(INVASION, 0)];
        // 40 + 2*(20+30) = 140, +50 population, +40 one invasion, +20 requests, +50 gods.
        assert_eq!(challenge(&info, &ev, invasion_count(&ev)), 300);
    }
}
