//! The Nile's yearly flood.
//!
//! Every year a season/duration/quality triple (set by the scenario, then re-rolled each
//! year) picks a window of "cycles" (25 ticks each, 392 to a pseudo-year) during which the
//! floodplain along the river goes underwater a row at a time, sits fully inundated, then
//! dries back out the same way. `Floods::rows` (built once at scenario start by a BFS out
//! from the river, matching the original's shore-distance rings) says which row each
//! floodplain tile belongs to; `flood_progress` (30 = dry, 0 = fully flooded) says how far
//! the water has advanced, and only the row it just crossed needs its terrain and image
//! touched each time it moves.
//!
//! Faithful to the original's `city_floods.cpp` / `grid/floodplain.cpp` state machine and
//! cycle math. Simplified: no per-tile randomized sub-row flooding timing (rows go
//! under/dry all at once), no floodplain grass growth/aging animation, no dike-breach or
//! sealed-basin fertility bonus features (those are optional gameplay-enhancement flags in
//! the reimplementation, not baseline behaviour).

use crate::grid::Grid;
use crate::map::{Map, terrain};
use crate::world::World;
use std::collections::VecDeque;

/// Ticks in one of the original's "cycles".
const CYCLE_TICKS: i32 = 25;
/// Cycles in a pseudo-year, per the original (`392 * 25 = 9800`, slightly longer than the
/// 9792-tick calendar year; `current_cycle` wraps on this, not on the calendar, just as the
/// original's does).
const CYCLES_IN_YEAR: i32 = 392;
/// Distinct floodplain shore-distance rows the flood can reach.
const MAX_ROWS: i32 = 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum FloodState {
    Resting,
    #[default]
    Farmable,
    Imminent,
    Flooding,
    Inundated,
    Contracting,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Floods {
    pub state: FloodState,
    season_initial: i32,
    duration_initial: i32,
    season: i32,
    duration: i32,
    quality_initial: i32,
    quality_current: i32,
    quality_next: i32,
    quality_last: i32,
    /// 0..=30; 30 is fully dry, 0 is fully inundated.
    flood_progress: i32,
    flood_progress_target: i32,
    /// Counts up to 20 ticks between each one-step move of `flood_progress`.
    flood_progress_tick: i32,
    /// Rows of floodplain found on this map at `init_floods`; 0 means the map has no
    /// floodplain terrain at all, and the flood never runs.
    floodplain_width: i32,
    /// Row index of each tile (`-1` = not floodplain), by shore distance from the river.
    rows: Grid<i8>,
    /// Tiles bucketed by row, for fast per-row updates as the flood advances or recedes.
    row_tiles: Vec<Vec<(i32, i32)>>,
}

impl Default for Floods {
    fn default() -> Self {
        Self {
            state: FloodState::default(),
            season_initial: 0,
            duration_initial: 0,
            season: 0,
            duration: 0,
            quality_initial: 100,
            quality_current: 100,
            quality_next: 100,
            quality_last: 100,
            flood_progress: 30,
            flood_progress_target: 30,
            flood_progress_tick: 0,
            floodplain_width: 0,
            rows: Grid::new(0, 0),
            row_tiles: Vec::new(),
        }
    }
}

impl Floods {
    /// The cycle the flood starts rising, from the season byte, as the original computes it.
    fn start_cycle(&self) -> i32 {
        (self.season as f64 * 1.05 + 14.5) as i32
    }

    /// The cycle the flood finishes contracting.
    fn end_cycle(&self) -> i32 {
        self.start_cycle() + self.duration + self.floodplain_width * 2
    }

    /// Cycles the rising/contracting edge takes to cross the whole floodplain width, at
    /// last year's quality (the quality that's actually flooding right now).
    fn period_length(&self) -> i32 {
        (self.quality_last as f64 * self.floodplain_width as f64 * 0.01) as i32
    }
}

/// The settings chunk's fields, decoded from bytes without interpreting them yet.
#[derive(Debug, Clone, Copy, Default)]
struct RawSettings {
    season_initial: i32,
    duration_initial: i32,
    quality_initial: i32,
    season: i32,
    duration: i32,
    quality_current: i32,
    quality_next: i32,
    quality_last: i32,
}

/// Decodes the scenario's `floodplain_settings` chunk (32 bytes pre-147, 36 bytes from 147
/// on; layout matches the original's `iob_floodplain_settings` binding). Missing or short
/// input decodes to all-zero fields rather than panicking.
fn parse_settings(bytes: &[u8]) -> RawSettings {
    let u8_at = |o: usize| bytes.get(o).copied().unwrap_or(0) as i32;
    let i32_at = |o: usize| {
        bytes
            .get(o..o + 4)
            .and_then(|s| <[u8; 4]>::try_from(s).ok())
            .map(i32::from_le_bytes)
            .unwrap_or(0)
    };
    RawSettings {
        season_initial: u8_at(0),
        duration_initial: i32_at(4),
        quality_initial: u8_at(8),
        season: i32_at(12),
        duration: i32_at(16),
        quality_current: u8_at(20),
        quality_next: u8_at(28),
        quality_last: if bytes.len() >= 33 { u8_at(32) } else { u8_at(20) },
    }
}

/// Map-coordinate tiles, bucketed by floodplain row.
type RowTiles = Vec<Vec<(i32, i32)>>;

/// BFS shore-distance rows out from the river, restricted to floodplain terrain, matching
/// the original's row 0 = floodplain tiles touching virgin water, row N = floodplain tiles
/// touching a row-(N-1) tile. Returns the per-tile row grid, tiles bucketed by row, and the
/// floodplain width (rows found, 0 if the map has no floodplain at all).
fn build_floodplain_rows(map: &Map) -> (Grid<i8>, RowTiles, i32) {
    let (w, h) = (map.width, map.height);
    let mut rows = Grid::filled(w, h, -1i8);
    let mut row_tiles: Vec<Vec<(i32, i32)>> = vec![Vec::new(); MAX_ROWS as usize];
    let mut queue = VecDeque::new();

    for y in 0..h {
        for x in 0..w {
            if !map.terrain_is(x, y, terrain::WATER) || map.terrain_is(x, y, terrain::FLOODPLAIN) {
                continue;
            }
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let (nx, ny) = (x + dx, y + dy);
                    if map.terrain_is(nx, ny, terrain::FLOODPLAIN) && rows.at_or(nx, ny, -1) == -1 {
                        rows.set(nx, ny, 0);
                        row_tiles[0].push((nx, ny));
                        queue.push_back((nx, ny));
                    }
                }
            }
        }
    }

    let mut max_row = if row_tiles[0].is_empty() { -1 } else { 0 };
    while let Some((x, y)) = queue.pop_front() {
        let r = rows.at_or(x, y, -1) as i32;
        if r >= MAX_ROWS - 1 {
            continue;
        }
        for dy in -1..=1 {
            for dx in -1..=1 {
                if dx == 0 && dy == 0 {
                    continue;
                }
                let (nx, ny) = (x + dx, y + dy);
                if map.terrain_is(nx, ny, terrain::FLOODPLAIN) && rows.at_or(nx, ny, -1) == -1 {
                    let nr = r + 1;
                    rows.set(nx, ny, nr as i8);
                    row_tiles[nr as usize].push((nx, ny));
                    max_row = max_row.max(nr);
                    queue.push_back((nx, ny));
                }
            }
        }
    }

    let width = (max_row + 1).clamp(0, MAX_ROWS);
    (rows, row_tiles, width)
}

/// The state and (when the state pins it) flood-progress target for a given cycle, as a
/// pure function of a `Floods`' season/duration/quality/width, so the state machine's
/// timing can be tested without a `World`. Mirrors `floods_t::cycle_states_recalc`.
fn cycle_state(floods: &Floods, cycle: i32) -> (FloodState, Option<i32>) {
    if floods.floodplain_width <= 0 {
        return (FloodState::Farmable, Some(0));
    }
    let cycle_start = floods.start_cycle();
    let cycle_end = floods.end_cycle();
    let cycle_end_last_year = cycle_end - 380;
    let flooding_period = floods.period_length();

    if cycle < cycle_end_last_year + 28 {
        (FloodState::Resting, Some(30))
    } else if cycle < cycle_start - 28 {
        (FloodState::Farmable, None)
    } else if cycle < cycle_start {
        (FloodState::Imminent, None)
    } else if cycle <= cycle_start + flooding_period {
        (FloodState::Flooding, Some(0))
    } else if cycle <= cycle_end - flooding_period {
        (FloodState::Inundated, Some(0))
    } else if cycle <= cycle_end {
        (FloodState::Contracting, Some(30))
    } else if cycle <= cycle_end + 28 {
        (FloodState::Resting, Some(30))
    } else {
        (FloodState::Farmable, None)
    }
}

impl World {
    /// Called once when the scenario starts, with the scenario's `floodplain_settings` chunk.
    pub(crate) fn init_floods(&mut self, settings: &[u8]) {
        let raw = parse_settings(settings);
        let (rows, row_tiles, width) = build_floodplain_rows(&self.map);
        self.floods = Floods {
            state: FloodState::Farmable,
            season_initial: raw.season_initial,
            duration_initial: raw.duration_initial,
            season: if raw.season != 0 { raw.season } else { raw.season_initial },
            duration: if raw.duration != 0 { raw.duration } else { raw.duration_initial },
            quality_initial: raw.quality_initial,
            quality_current: if raw.quality_current != 0 { raw.quality_current } else { raw.quality_initial },
            quality_next: if raw.quality_next != 0 { raw.quality_next } else { 100 },
            quality_last: if raw.quality_last != 0 { raw.quality_last } else { 100 },
            flood_progress: 30,
            flood_progress_target: 30,
            flood_progress_tick: 0,
            floodplain_width: width,
            rows,
            row_tiles,
        };
        // Establish the right state/target for wherever in the year the scenario starts,
        // without stepping flood_progress or firing transition hooks off a fake "Farmable"
        // starting point.
        self.floods.state = self.current_cycle_state().0;
    }

    fn current_cycle_state(&self) -> (FloodState, Option<i32>) {
        cycle_state(&self.floods, self.current_flood_cycle().0)
    }

    /// `(cycle, subcycle)` for the game clock's current position in the pseudo-year.
    fn current_flood_cycle(&self) -> (i32, i32) {
        let ticks_since_year_start =
            (self.time.month * crate::time::DAYS_PER_MONTH + self.time.day) * crate::time::TICKS_PER_DAY + self.time.tick;
        let cycle = (ticks_since_year_start / CYCLE_TICKS as u32) as i32 % CYCLES_IN_YEAR;
        let subcycle = (ticks_since_year_start % CYCLE_TICKS as u32) as i32;
        (cycle, subcycle)
    }

    /// Called every tick.
    pub(crate) fn update_floods(&mut self) {
        if self.floods.floodplain_width <= 0 {
            self.floods.state = FloodState::Farmable;
            return;
        }

        let (cycle, subcycle) = self.current_flood_cycle();
        let old_state = self.floods.state;
        let old_progress = self.floods.flood_progress;

        let (new_state, target) = cycle_state(&self.floods, cycle);
        self.floods.state = new_state;
        if let Some(t) = target {
            self.floods.flood_progress_target = t.clamp(0, 30);
        }
        // With floods turned off the seasons still turn, so floodplain farms are still
        // harvested and replanted each year, but the water stays in the river.
        if !self.rules.floods {
            self.floods.flood_progress_target = 30;
        }

        // Once a year, at the cycle right before the flood starts rising: roll next year's
        // quality forward and tell the player what to expect.
        if subcycle == 0 && cycle == self.floods.start_cycle() - 1 {
            self.roll_next_flood_quality();
            if self.rules.floods {
                self.queue_flood_prediction_message();
            }
        }

        if new_state == FloodState::Imminent && old_state != FloodState::Imminent {
            self.harvest_floodplain_farms();
        }
        if new_state == FloodState::Farmable && old_state != FloodState::Farmable {
            self.reset_floodplain_farms();
        }
        // Once the flood is in, it has done Osiris's work.
        if new_state == FloodState::Inundated && self.religion.osiris_flood_destroys == 2 {
            self.religion.osiris_flood_destroys = 0;
        }

        if self.floods.flood_progress != self.floods.flood_progress_target {
            self.floods.flood_progress_tick += 1;
            if self.floods.flood_progress_tick > 20 {
                self.floods.flood_progress_tick = 0;
                if self.floods.flood_progress > self.floods.flood_progress_target {
                    self.floods.flood_progress -= 1;
                } else {
                    self.floods.flood_progress += 1;
                }
            }
        }
        self.floods.flood_progress = self.floods.flood_progress.clamp(0, 30);

        let new_progress = self.floods.flood_progress;
        if new_progress != old_progress {
            self.apply_flood_progress(old_progress, new_progress);
        }
    }

    pub fn flood_state(&self) -> FloodState {
        self.floods.state
    }

    /// Whether this map has a floodplain for the Nile to flood.
    /// Changes the quality of the next flood (Osiris's blessings and curses).
    pub fn adjust_next_flood_quality(&mut self, delta: i32) {
        self.floods.quality_next = (self.floods.quality_next + delta).clamp(0, 100);
    }

    /// The month of the flood's season (season / 30).
    pub(crate) fn flood_month(&self) -> i32 {
        self.floods.season / 30 % 12
    }

    /// The month (0 = the first) the next flood starts rising.
    pub fn flood_start_month(&self) -> u32 {
        let ticks = self.floods.start_cycle().max(0) as u32 * CYCLE_TICKS as u32;
        ticks / crate::time::TICKS_PER_DAY / crate::time::DAYS_PER_MONTH % crate::time::MONTHS_PER_YEAR
    }

    pub fn has_floodplain(&self) -> bool {
        self.floods.floodplain_width > 0
    }

    /// Whether `(x, y)` is currently underwater from the flood (or, for a floodplain tile
    /// with a building on it, would be if the building weren't hiding the water).
    pub fn is_flooded(&self, x: i32, y: i32) -> bool {
        match self.floods.rows.get(x, y) {
            Some(r) if r >= 0 => r as i32 <= MAX_ROWS - 1 - self.floods.flood_progress,
            _ => false,
        }
    }

    /// Tiles whose ditches the Nile fills each day, row by row: with the flood out, the
    /// row at the flood's edge and the two behind it toward the river; otherwise the row
    /// at the river's edge, with the dry bank beside the floodplain where it meets the
    /// river.
    pub(crate) fn river_ditch_sources(&self) -> Vec<(i32, i32)> {
        let level = self.floods.flood_progress;
        let mut tiles = Vec::new();
        for (i, list) in [level.min(MAX_ROWS - 1), level + 1, level + 2].into_iter().enumerate() {
            if i > 0 && list >= MAX_ROWS {
                continue;
            }
            let row = (MAX_ROWS - 1 - list) as usize;
            tiles.extend(self.floods.row_tiles.get(row).into_iter().flatten().copied());
            if row == 0 && self.has_floodplain() {
                let map = &self.map;
                let open_water = |x: i32, y: i32| crate::map::NEIGHBOURS.iter().any(|&(dx, dy)| map.terrain_is(x + dx, y + dy, terrain::WATER) && !map.terrain_is(x + dx, y + dy, terrain::FLOODPLAIN | terrain::DIKE));
                for y in 0..map.height {
                    for x in 0..map.width {
                        if map.terrain_is(x, y, terrain::CANAL) && !map.terrain_is(x, y, terrain::WATER) && crate::irrigation::floodplain_bank(map, x, y) && open_water(x, y) {
                            tiles.push((x, y));
                        }
                    }
                }
            }
        }
        tiles
    }

    fn roll_next_flood_quality(&mut self) {
        self.floods.season = self.floods.season_initial;
        self.floods.duration = self.floods.duration_initial;
        self.floods.quality_last = self.floods.quality_current;
        self.floods.quality_current = self.floods.quality_next;
        // The original re-rolls this with libc `rand()` (non-deterministic); we use the
        // world's own deterministic RNG instead so replays and saves reproduce it.
        let roll = self.rng.below(100) + 20;
        self.floods.quality_next = (self.floods.quality_next + roll) % 100;
        // A temple complex to Osiris makes good floods likelier.
        if self.complex_blessing(crate::temple_complex::OSIRIS, 0) {
            self.floods.quality_next = (self.floods.quality_next + 10).min(100);
        }
    }

    fn queue_flood_prediction_message(&mut self) {
        let key = match self.floods.quality_next {
            100 => "message_perfect_inundation",
            75..=99 => "message_excellent_inundation",
            50..=74 => "message_good_inundation",
            25..=49 => "message_mediocre_inundation",
            1..=24 => "message_poor_inundation",
            _ => "message_no_inundation",
        };
        self.post(key, None, true);
    }

    /// Floods or dries the rows crossed by `flood_progress` moving from `old` to `new`.
    fn apply_flood_progress(&mut self, old: i32, new: i32) {
        if new < old {
            for row in (MAX_ROWS - old)..(MAX_ROWS - new) {
                self.flood_row(row, true);
            }
        } else {
            for row in (MAX_ROWS - new)..(MAX_ROWS - old) {
                self.flood_row(row, false);
            }
        }
    }

    fn flood_row(&mut self, row: i32, flooding: bool) {
        if !(0..MAX_ROWS).contains(&row) {
            return;
        }
        let tiles = self.floods.row_tiles[row as usize].clone();
        for (x, y) in tiles {
            self.flood_tile(x, y, flooding, row);
        }
    }

    fn flood_tile(&mut self, x: i32, y: i32, flooding: bool, row: i32) {
        // Buildings (floodplain farms) are never turned into water tiles; `is_flooded`
        // reports they're underwater regardless, and farms.rs handles their own state via
        // the Imminent/Farmable hooks.
        let mut is_building = self.map.terrain_is(x, y, terrain::BUILDING);
        // Osiris's anger: the flood destroys the farms it reaches.
        let farm = self.map.building.at_or(x, y, 0);
        if flooding && is_building && self.religion.osiris_flood_destroys != 0 && self.is_floodplain_farm(farm) {
            self.religion.osiris_flood_destroys = 2;
            let tiles: Vec<(i32, i32)> = self.buildings.get(farm).map(|b| b.tiles().collect()).unwrap_or_default();
            self.demolish(farm);
            is_building = self.map.terrain_is(x, y, terrain::BUILDING);
            for (tx, ty) in tiles {
                if (tx, ty) != (x, y) && self.is_flooded(tx, ty) {
                    let r = self.floods.rows.get(tx, ty).unwrap_or(0) as i32;
                    self.flood_tile(tx, ty, true, r);
                }
            }
        }
        if flooding {
            if !is_building {
                self.map.terrain.update(x, y, |t| {
                    let t = t | terrain::WATER;
                    if t & terrain::ROAD != 0 { (t & !terrain::ROAD) | terrain::SUBMERGED_ROAD } else { t }
                });
                self.redraw_after_flood(x, y, true);
            }
            // The flood restores fertility as it reaches each row; drier rows (further
            // from the river) top out lower, and a poor flood restores less of that.
            let max_fertile = (99 - (99 * row) / MAX_ROWS).clamp(0, 99);
            let restored = (max_fertile * self.floods.quality_current.clamp(0, 100) / 100).clamp(0, 99) as u8;
            if self.map.fertility.at_or(x, y, 0) < restored {
                self.map.fertility.set(x, y, restored);
            }
        } else if !is_building {
            self.map.terrain.update(x, y, |t| {
                let t = t & !(terrain::WATER | terrain::DEEPWATER);
                if t & terrain::SUBMERGED_ROAD != 0 { (t & !terrain::SUBMERGED_ROAD) | terrain::ROAD } else { t }
            });
            self.redraw_after_flood(x, y, false);
        } else {
            // Ditches come back out of the water, and those beside open into it or not.
            self.ditch_images_in(x - 1, y - 1, x + 1, y + 1);
        }
    }

    /// Redraws the land, water, roads and ditches around a tile the flood just rose
    /// onto or left, as the original does (FUN_004bd950): the floodplain and water (see
    /// `terrain_images::refresh_flood`), then the roads and ditches within 2 tiles, or 3
    /// as the water leaves.
    fn redraw_after_flood(&mut self, x: i32, y: i32, flooding: bool) {
        crate::terrain_images::refresh_flood(&mut self.map, &self.defs, x, y, flooding);
        let r = if flooding { 2 } else { 3 };
        let (mut rules, map) = self.tile_rules();
        rules.roads_in(map, x - r, y - r, x + r, y + r);
        self.ditch_images_in(x - r, y - r, x + r, y + r);
    }

    // `harvest_floodplain_farms` and `reset_floodplain_farms`, called above on the
    // Farmable->Imminent and ->Farmable transitions, are already implemented over in
    // farms.rs (concurrently with this module); they're not declared here too, since two
    // `impl World` methods with the same name would be a duplicate definition.
}

#[cfg(test)]
mod tests {
    use super::*;

    fn synthetic(season: i32, duration: i32, quality_last: i32, width: i32) -> Floods {
        Floods {
            season,
            duration,
            quality_last,
            floodplain_width: width,
            ..Floods::default()
        }
    }

    /// Across one pseudo-year the state machine should visit every state in order and end
    /// back where it started.
    #[test]
    fn state_machine_cycles_through_year_in_order() {
        let floods = synthetic(100, 80, 60, 20);
        // Same numbers computed by hand from `cycle_state`'s formulas, to sanity check the
        // test itself: start_cycle = (100*1.05+14.5) as i32 = 119, end_cycle = 119+80+40 = 239,
        // flooding_period = (60*20*0.01) as i32 = 12.
        assert_eq!(floods.start_cycle(), 119);
        assert_eq!(floods.end_cycle(), 239);
        assert_eq!(floods.period_length(), 12);

        let mut order = Vec::new();
        let mut last = None;
        for cycle in 0..CYCLES_IN_YEAR {
            let (state, _) = cycle_state(&floods, cycle);
            if last != Some(state) {
                order.push(state);
                last = Some(state);
            }
        }
        assert_eq!(
            order,
            vec![
                FloodState::Farmable,
                FloodState::Imminent,
                FloodState::Flooding,
                FloodState::Inundated,
                FloodState::Contracting,
                FloodState::Resting,
                FloodState::Farmable,
            ]
        );
    }

    /// A map with no floodplain terrain never floods.
    #[test]
    fn no_floodplain_means_no_flood() {
        let floods = synthetic(100, 80, 60, 0);
        for cycle in [0, 100, 200, 300, 391] {
            assert_eq!(cycle_state(&floods, cycle).0, FloodState::Farmable);
        }
    }

    #[test]
    fn settings_chunk_parses_known_layout() {
        let mut bytes = vec![0u8; 36];
        bytes[0] = 210; // season_initial
        bytes[4..8].copy_from_slice(&40i32.to_le_bytes()); // duration_initial
        bytes[8] = 100; // quality_initial
        bytes[12..16].copy_from_slice(&210i32.to_le_bytes()); // season
        bytes[16..20].copy_from_slice(&40i32.to_le_bytes()); // duration
        bytes[20] = 80; // quality_current
        bytes[28] = 55; // quality_next
        bytes[29] = 12; // flood_progress (unused by us)
        bytes[32] = 90; // quality_last

        let raw = parse_settings(&bytes);
        assert_eq!(raw.season_initial, 210);
        assert_eq!(raw.duration_initial, 40);
        assert_eq!(raw.quality_initial, 100);
        assert_eq!(raw.season, 210);
        assert_eq!(raw.duration, 40);
        assert_eq!(raw.quality_current, 80);
        assert_eq!(raw.quality_next, 55);
        assert_eq!(raw.quality_last, 90);

        // The pre-147 32-byte layout has no quality_last; we fall back to quality_current.
        let short = &bytes[..32];
        let raw_short = parse_settings(short);
        assert_eq!(raw_short.quality_last, 80);
    }

    #[test]
    fn empty_settings_dont_panic() {
        let raw = parse_settings(&[]);
        assert_eq!(raw.season_initial, 0);
        assert_eq!(raw.duration_initial, 0);
    }

    // --- Tests below need the original game's data files (PharaohData/), which aren't part
    // of this repository; they skip themselves when that directory isn't present. ---

    fn pharaoh_data_dir() -> Option<std::path::PathBuf> {
        let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../PharaohData");
        dir.is_dir().then_some(dir)
    }

    #[test]
    fn settings_chunk_from_real_scenario_is_sane() {
        let Some(data) = pharaoh_data_dir() else { return };
        let pak = osiris_formats::MissionPak::open(&data.join("mission1.pak")).expect("open mission1.pak");
        // Mission index 2, "The Precarious Nile", has a working flood.
        let scenario = pak.scenario(2).expect("scenario 2");
        let bytes = &scenario.floodplain_settings;
        assert!(bytes.len() == 32 || bytes.len() == 36, "unexpected chunk length {}", bytes.len());
        let raw = parse_settings(bytes);
        assert!((0..=255).contains(&raw.season_initial));
        assert!(raw.duration_initial >= 0);
        assert!((0..=100).contains(&raw.quality_initial));
    }

    fn load_test_world(data: &std::path::Path, mission: usize) -> World {
        let library = osiris_formats::ImageLibrary::open(&data.join("Data")).expect("open image library");
        let pak = osiris_formats::MissionPak::open(&data.join("mission1.pak")).expect("open mission1.pak");
        let scenario = pak.scenario(mission).expect("scenario");
        let defs = std::sync::Arc::new(crate::defs::Defs::load(&library).expect("load defs"));
        let model_text = std::fs::read(data.join("Pharaoh_Model_Normal.txt")).expect("read model");
        let model = osiris_formats::Model::parse(&String::from_utf8_lossy(&model_text)).expect("parse model");
        let balance = std::sync::Arc::new(crate::balance::Balance::from_model(&model));
        let mut world = World::new(&scenario, defs, balance);
        world.start(&scenario);
        world
    }

    /// Runs mission 2 ("The Precarious Nile") for two game years and checks the flood
    /// actually happens: it reaches Inundated, water tiles appear, and then disappear again.
    #[test]
    fn flood_rises_and_recedes_over_two_years() {
        let Some(data) = pharaoh_data_dir() else { return };
        let mut world = load_test_world(&data, 2);
        assert!(world.floods.floodplain_width > 0, "mission 2 should have floodplain terrain");

        let mut saw_inundated = false;
        let mut saw_water = false;
        let mut water_receded = false;
        let mut ever_had_water = false;

        for _ in 0..(2 * 9600) {
            world.tick();
            if world.flood_state() == FloodState::Inundated {
                saw_inundated = true;
            }
            let has_water = world
                .floods
                .row_tiles
                .iter()
                .flatten()
                .any(|&(x, y)| world.map.terrain_is(x, y, terrain::WATER));
            if has_water {
                saw_water = true;
                ever_had_water = true;
            } else if ever_had_water {
                water_receded = true;
            }
        }

        assert!(saw_inundated, "flood never reached Inundated in two years");
        assert!(saw_water, "flood never put any floodplain tile underwater");
        assert!(water_receded, "flood water never receded after rising");
    }
}
