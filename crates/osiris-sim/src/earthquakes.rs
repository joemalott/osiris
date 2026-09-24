//! Earthquakes. The scenario plans them as events of type 3: a month, a number of
//! years after the start, and a severity. The map editor sets one epicentre for all
//! of them. When a quake comes the epicentre cracks open, then every tick one of four
//! crack fronts may move a tile north, east, south or west, splitting the ground and
//! burning whatever stands there. Fronts stop at rock, water and cliffs. After 25
//! steps per point of severity the ground is still again.
//!
//! Cracks are rock with the earthquake mark (`0x80` in the bitfields): nobody can walk
//! or build on them, and they lower the desirability around them.

use crate::map::terrain;
use crate::scenario_events::{EventText, Pick, event};
use crate::world::World;

/// Quake states.
pub mod state {
    pub const WAITING: u8 = 0;
    pub const QUAKING: u8 = 1;
    pub const DONE: u8 = 2;
}

/// Steps a quake lasts per point of severity (and at least).
const STEPS_PER_SEVERITY: i32 = 25;

/// Terrain a crack front cannot move into.
const STOPS_CRACKS: u32 = terrain::ROCK | terrain::WATER | terrain::ELEVATION;

/// Bitfield mark of a crack (shared with plazas, which are road).
pub const CRACK_MARK: u8 = 0x80;

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Quake {
    /// Comes in this month (0 = January) of this many years after the start.
    pub month: i32,
    pub year: i32,
    severity_pick: Pick,
    /// The severity picked when the quake began.
    pub severity: i32,
    pub state: u8,
    pub steps: i32,
    /// The four crack fronts, all starting at the epicentre.
    pub fronts: [(i32, i32); 4],
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Earthquakes {
    pub list: Vec<Quake>,
    /// Where every quake starts; `None` when the map has no epicentre.
    pub epicentre: Option<(i32, i32)>,
}

impl Earthquakes {
    pub fn from_scenario(s: &osiris_formats::Scenario) -> Self {
        let list = s
            .events
            .iter()
            .filter(|e| e.kind == event::EARTHQUAKE)
            .map(|e| Quake { month: e.month as i32, year: e.year as i32, severity_pick: e.amount.into(), ..Default::default() })
            .collect();
        let p = s.info.earthquake_point;
        Self { list, epicentre: p.is_valid().then_some((p.x, p.y)) }
    }
}

/// Which front moves, and where, for the low four bits of the random byte. Front 0
/// runs mostly north, 1 east, 2 south, 3 west.
fn direction(r: i32) -> (usize, i32, i32) {
    match r & 0xf {
        0 | 4 => (0, 0, -1),
        5 => (0, -1, 0),
        6 => (0, 1, 0),
        1 | 7 => (1, 1, 0),
        8 => (1, 0, -1),
        9 => (1, 0, 1),
        2 | 10 => (2, 0, 1),
        11 => (2, -1, 0),
        12 => (2, 1, 0),
        3 | 13 => (3, -1, 0),
        14 => (3, 0, -1),
        _ => (3, 0, 1),
    }
}

impl World {
    /// Every tick: a planned quake whose month has come begins, and a quake under way
    /// takes one step. Only one quake runs at a time.
    pub(crate) fn update_earthquakes(&mut self) {
        let now = (self.time.year - self.scenario_events.start_year, self.time.month as i32);
        for i in 0..self.earthquakes.list.len() {
            let q = &self.earthquakes.list[i];
            match q.state {
                state::WAITING if self.rules.disasters && (q.year, q.month) == now => {
                    let Some(epicentre) = self.earthquakes.epicentre else { return };
                    self.start_quake(i, epicentre, None);
                    return;
                }
                state::QUAKING => {
                    self.quake_step(i);
                    return;
                }
                _ => {}
            }
        }
    }

    /// Starts quake `i` at `at`: the ground there cracks and the player is told. A
    /// `severity` overrides the scenario's pick.
    fn start_quake(&mut self, i: usize, at: (i32, i32), severity: Option<i32>) {
        let pick = self.earthquakes.list[i].severity_pick;
        let severity = match severity {
            Some(s) => s,
            None => self.roll(pick),
        };
        let q = &mut self.earthquakes.list[i];
        q.severity = severity;
        q.state = state::QUAKING;
        q.steps = 0;
        q.fronts = [at; 4];
        self.crack(at.0, at.1);
        self.post_event_text(EventText {
            title: "earthquake_title".into(),
            body: "earthquake_initial_announcement".into(),
            reason: "earthquake_no_reason_A".into(),
            template: 261,
            ..Default::default()
        });
        if let Some(n) = self.notices.log.last_mut() {
            n.tile = Some(at);
        }
    }

    /// One step of quake `i`: a front may move a tile and crack it.
    fn quake_step(&mut self, i: usize) {
        let r = self.rng.byte();
        let (w, h) = (self.map.width, self.map.height);
        let q = &mut self.earthquakes.list[i];
        q.steps += 1;
        if q.steps >= (STEPS_PER_SEVERITY * q.severity).max(STEPS_PER_SEVERITY) {
            q.state = state::DONE;
        }
        let (front, dx, dy) = direction(r);
        let (x, y) = q.fronts[front];
        let next = ((x + dx).clamp(0, w - 1), (y + dy).clamp(0, h - 1));
        if self.map.terrain_is(next.0, next.1, STOPS_CRACKS) {
            return;
        }
        self.earthquakes.list[i].fronts[front] = next;
        self.crack(next.0, next.1);
    }

    /// The ground splits at `(x, y)`: a building there burns (the ruin on the crack
    /// itself goes), the tile becomes cracked rock, and the cracks, roads and road
    /// access around it are redrawn.
    fn crack(&mut self, x: i32, y: i32) {
        if !self.map.contains(x, y) {
            return;
        }
        let id = self.map.building.at_or(x, y, 0);
        if id != 0 {
            self.wreck(id, true);
            let ruin = self.map.building.at_or(x, y, 0);
            if ruin != 0 {
                self.demolish(ruin);
            }
        }
        self.map.terrain.set(x, y, terrain::ROCK);
        self.map.bitfields.update(x, y, |b| (b | CRACK_MARK) & 0xf0);
        let (x0, y0) = ((x - 1).max(0), (y - 1).max(0));
        let (x1, y1) = ((x + 1).min(self.map.width - 1), (y + 1).min(self.map.height - 1));
        let (mut rules, map) = self.tile_rules();
        for yy in y0..=y1 {
            for xx in x0..=x1 {
                rules.crack_image(map, xx, yy);
            }
        }
        rules.roads_in(map, x0, y0, x1, y1);
        self.dust(x, y, 1);
        let near: Vec<_> = self.buildings.iter().filter(|b| b.x <= x + 2 && b.y <= y + 2 && b.x + b.size + 2 > x && b.y + b.size + 2 > y).map(|b| b.id).collect();
        for id in near {
            self.refresh_road_access(id);
        }
    }

    /// For scripts: starts the scenario's next planned quake now (or a severity 5 one
    /// if none is planned), at the epicentre or `at`, with its own severity or
    /// `severity`.
    pub fn quake_now(&mut self, at: Option<(i32, i32)>, severity: Option<i32>) -> bool {
        let Some(at) = at.or(self.earthquakes.epicentre) else { return false };
        let i = match self.earthquakes.list.iter().position(|q| q.state == state::WAITING) {
            Some(i) => i,
            None => {
                self.earthquakes.list.push(Quake { severity_pick: Pick { value: 5, fixed: 5, min: -1, max: -1 }, ..Default::default() });
                self.earthquakes.list.len() - 1
            }
        };
        self.start_quake(i, at, severity);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_front_has_three_directions() {
        let mut seen = [0; 4];
        for r in 0..16 {
            let (front, dx, dy) = direction(r);
            assert_eq!(dx.abs() + dy.abs(), 1);
            seen[front] += 1;
        }
        assert_eq!(seen, [4; 4]);
    }
}
