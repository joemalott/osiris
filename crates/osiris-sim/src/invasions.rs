//! Invasions. The scenario plans them as events: who attacks (a foreign nation,
//! an Egyptian army, Pharaoh's army or Bedouin raiders), how many, where they land
//! (land invasion points 1-8, sea points 9-16), when they arrive, how much warning the
//! city gets, and what they go for. The warnings come as the army draws near (when
//! first sighted, then at two years, one year, six months and one month), and then it
//! is upon the city.
//!
//! Invaders fight any soldier or constable who comes near and otherwise march on
//! their target, wrecking each building they reach. An army that can find nothing
//! to attack for about 25 days, not even by battering through walls, goes home a
//! formation at a time. A city with few people left and more invaders than soldiers
//! is lost.

use crate::balance::UnitStats;
use crate::buildings::{BuildingId, kind};
use crate::figures::{Figure, FigureId, Step, Travel};
use crate::military::action;
use crate::scenario_events::{EventText, Pick};
use crate::world::World;

pub const ENEMY_ARCHER: u16 = 43;
pub const ENEMY_INFANTRY: u16 = 44;
pub const ENEMY_CHARIOT: u16 = 45;
pub const BEDOUIN: u16 = 99;


/// Who invades.
pub mod invader {
    pub const ENEMY: u8 = 1;
    pub const EGYPT: u8 = 2;
    pub const PHARAOH: u8 = 3;
    pub const BEDOUIN: u8 = 4;
}

/// Damage a building takes before invaders bring it down.
const BUILDING_HP: i32 = 1000;
/// Tiles within which invaders turn on soldiers.
const CHASE_RANGE: i32 = 5;
/// Months before arrival when reminders come.
const REMINDERS: [i32; 4] = [24, 12, 6, 1];
/// Pharaoh calls off an army sent for lost favour if the kingdom rating is back above
/// this before it arrives.
const CALL_OFF_KINGDOM: i32 = 14;
const TRIGGER_BY_FAVOUR: u8 = 16;

/// How long an invader waits before retrying a failed pathfind, given how many times
/// in a row it has already failed: backs off so a permanently unreachable target (an
/// island across water, say, with no wall in reach to batter instead) isn't searched
/// for again (a full-map scan) every 50 ticks for the rest of the game.
fn stuck_backoff(consecutive_failures: u8) -> i32 {
    50i32 << consecutive_failures.min(5)
}

/// Action of a routed invader.
const FLEEING: u16 = action::FLEEING;

/// The formation slots the original keeps for invaders and herds (10 to 49 of its
/// 50; the first nine are the city's companies). An army that finds none free when
/// it arrives doesn't bring the formations that don't fit (FUN_004b6ff0 returns 0,
/// and FUN_00446a30 spawns no one for them).
pub(crate) const ENEMY_FORMATION_SLOTS: usize = 40;
/// Consecutive failed target searches (two a day) after which a formation gives up
/// and goes home: the 49th (FUN_004b8cf0, its counter at formation +0x68).
const GIVE_UP_AFTER: u8 = 48;
/// Tiles within which soldiers of the city hold an army's attention, so it doesn't
/// look for a building to attack (FUN_004ba290 with 16).
const SOLDIERS_NEAR: i32 = 16;

/// How the original splits an arm of an invading army into formations
/// (FUN_00446a30): one up to 16 men, two up to 32, else three, the first taking
/// what doesn't divide evenly.
fn formation_sizes(men: i32) -> Vec<i32> {
    match men {
        ..=0 => Vec::new(),
        1..=16 => vec![men],
        17..=32 => vec![men - men / 2, men / 2],
        _ => vec![men - 2 * (men / 3), men / 3, men / 3],
    }
}

/// Whether an invader looking for a building to attack counts `(x, y)` as ground he
/// can cross on the way, walls and buildings included (he batters through them): the
/// original's "through everything" search over its non-citizen route grid
/// (FUN_0051b040, which FUN_004ba3b0 runs before choosing), where only water, dikes
/// and ground nothing crosses (rock, cliffs, ore, raised ground) stop him.
fn crossable(map: &crate::map::Map, x: i32, y: i32) -> bool {
    use crate::map::terrain;
    if crate::figures::passable(map, Travel::Hostile, x, y) {
        return true;
    }
    let t = map.terrain.at_or(x, y, 0);
    t & (terrain::BUILDING | terrain::WALL | terrain::GATEHOUSE) != 0 && t & (terrain::WATER | terrain::DIKE) == 0
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Invasion {
    pub invader: u8,
    amount: Pick,
    point: Pick,
    /// Arrival: years after the start, and month.
    pub year: i32,
    pub month: i32,
    /// Months of warning the city gets.
    pub warning: i32,
    pub target: i8,
    /// Years between recurring invasions (0: once).
    interval: (i16, i16),
    recurring: bool,
    /// Comes when the kingdom rating falls to nothing, not on a date.
    pub by_favour: bool,
    /// Comes only when another event leads to it, not on a date.
    #[serde(default)]
    pub via_event: bool,
    /// The scenario event it was planned from.
    #[serde(default)]
    pub event: Option<usize>,
    /// Warships coming by sea with it, when not the event's own number (scripts).
    #[serde(default)]
    pub warships: Option<i32>,
    pub armed: bool,
    pub announced: bool,
    last_warning: i32,
    pub done: bool,
}

/// An army on the field.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Army {
    pub figures: Vec<FigureId>,
    pub invader: u8,
    /// Which army (index into the army definitions); invaders keep it in `cargo`.
    pub nation: u16,
    pub target: BuildingId,
    pub priority: i8,
    #[serde(default = "full_morale")]
    pub morale: i32,
    /// Broken and running for the edge of the map.
    #[serde(default)]
    pub fleeing: bool,
    /// Where it came in, and leaves.
    #[serde(default)]
    pub entry: (i32, i32),
    /// Formations raised so far (the next one's number).
    #[serde(default)]
    pub bands: u8,
    /// Searches in a row that found no building the army can get at.
    #[serde(default)]
    pub failed_searches: u8,
    /// Formations that have given up and are marching home.
    #[serde(default)]
    pub withdrawn: Vec<u8>,
}

fn full_morale() -> i32 {
    100
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Invasions {
    pub planned: Vec<Invasion>,
    pub armies: Vec<Army>,
    pub land_points: Vec<(i32, i32)>,
    pub sea_points: Vec<(i32, i32)>,
    /// Where invaders from the sea come ashore.
    #[serde(default)]
    pub landings: Vec<(i32, i32)>,
    /// The scenario's foreign enemy (nation index).
    pub nation: u16,
    pub peak_population: i32,
    pub lost: bool,
}

impl Invasions {
    pub fn from_scenario(s: &osiris_formats::Scenario, defs: &crate::defs::Defs) -> Self {
        let info = &s.info;
        let planned = s
            .events
            .iter()
            .enumerate()
            .filter(|(_, e)| e.kind == crate::scenario_events::event::INVASION)
            .map(|(i, e)| Invasion {
                invader: e.item.value.clamp(1, 4) as u8,
                amount: e.amount.into(),
                point: osiris_formats::EventValue { value: e.location[0], fixed: e.location[1], min: e.location[2], max: e.location[3] }.into(),
                year: e.year as i32,
                month: e.month as i32,
                warning: e.months as i32,
                target: e.attack_target,
                interval: crate::scenario_events::repeat_interval(e),
                recurring: e.trigger == crate::scenario_events::trigger::RECURRING,
                by_favour: e.trigger == TRIGGER_BY_FAVOUR,
                via_event: e.trigger == crate::scenario_events::trigger::ONLY_VIA_EVENT,
                event: Some(i),
                ..Default::default()
            })
            .collect();
        let points = |p: &[osiris_formats::scenario::TilePoint]| p.iter().filter(|p| p.is_valid()).map(|p| (p.x, p.y)).collect();
        Self {
            planned,
            land_points: points(&info.invasion_points_land),
            sea_points: points(&info.invasion_points_sea),
            landings: points(&info.disembark_points),
            nation: defs.armies.iter().position(|a| a.enemy_ids.contains(&(info.enemy_id as i64))).unwrap_or(0) as u16,
            ..Default::default()
        }
    }
}

/// The phrase group of an invader's messages.
fn phrases(invader: u8) -> &'static str {
    match invader {
        invader::EGYPT => "eg_city_attacks_you",
        invader::PHARAOH => "pharaoh_attacks_you",
        invader::BEDOUIN => "bedouin_attacks_you",
        _ => "foreign_army_attacks_you",
    }
}

pub fn is_invader_kind(k: u16) -> bool {
    matches!(k, ENEMY_ARCHER | ENEMY_INFANTRY | ENEMY_CHARIOT | BEDOUIN)
}

impl World {
    pub fn is_invader(&self, f: &Figure) -> bool {
        is_invader_kind(f.kind)
    }

    /// An invader's stats: his nation's row for his arm (infantry, archers, chariots),
    /// the Egyptian rows for Egyptian and Pharaoh's armies, the Bedouin row for raiders.
    pub fn invader_stats(&self, f: &Figure) -> Option<UnitStats> {
        if !is_invader_kind(f.kind) {
            return None;
        }
        let arm = match f.kind {
            ENEMY_ARCHER => 1,
            ENEMY_CHARIOT => 2,
            _ => 0,
        };
        let army = self.defs.armies.get(f.cargo as usize)?;
        let row = |r: usize| self.balance.enemy_units.get(r).copied().unwrap_or_default();
        Some(match (army.key.as_str(), army.stats_row) {
            // Egyptian armies fight with the Egyptian rows of the figure list.
            ("egyptian", _) => self.balance.unit(54 + arm as u16),
            // Bedouin raiders are Bedouin infantry with Libyan archers.
            ("bedouin", Some(r)) => if arm == 0 { self.balance.unit(BEDOUIN) } else { row(r + arm) },
            (_, Some(r)) => row(r + arm),
            // Barbarians have no nation rows: the figure list's barbarians.
            (_, None) => self.balance.unit(if arm == 1 { 51 } else { 49 }),
        })
    }

    fn months_now(&self) -> i32 {
        (self.time.year - self.scenario_events.start_year) * 12 + self.time.month as i32
    }

    /// Monthly: invasions draw near, with their warnings, and arrive.
    pub(crate) fn update_invasions(&mut self) {
        let now = self.months_now();
        let favour_lost = self.ratings.kingdom <= 0;
        for i in 0..self.invasions.planned.len() {
            let inv = &mut self.invasions.planned[i];
            if inv.done {
                continue;
            }
            if inv.by_favour {
                // Pharaoh's patience runs out when the kingdom rating is gone.
                if !inv.armed {
                    if !favour_lost {
                        continue;
                    }
                    inv.armed = true;
                    let arrive = now + inv.warning.max(1);
                    inv.year = arrive / 12;
                    inv.month = arrive % 12;
                } else if self.ratings.kingdom > CALL_OFF_KINGDOM {
                    // Won back Pharaoh's favour before his army comes: it stays home.
                    inv.done = true;
                    self.post("message_attack_called_off", None, true);
                    continue;
                }
            } else if inv.via_event && !inv.armed {
                continue;
            }
            let left = inv.year * 12 + inv.month - now;
            let group = phrases(inv.invader);
            let mut post = None;
            if left > 0 && left <= inv.warning.max(1) && !inv.announced {
                inv.announced = true;
                inv.last_warning = left;
                post = Some(format!("{group}_initial_announcement"));
            } else if inv.announced && left > 0 && REMINDERS.contains(&left) && inv.last_warning != left {
                inv.last_warning = left;
                post = Some(match left {
                    24 => format!("{group}_2year_reminder"),
                    12 => format!("{group}_1year_reminder"),
                    6 => format!("{group}_6month_warning"),
                    _ if inv.invader == invader::ENEMY => format!("{group}_1month_Warning"),
                    _ => format!("{group}_1month_warning"),
                });
            }
            let arrive = left <= 0;
            let invader = inv.invader;
            if let Some(body) = post {
                self.post_invasion_text(invader, &body, left);
            }
            if arrive {
                self.launch_invasion(i);
            }
        }
    }

    fn post_invasion_text(&mut self, invader: u8, body: &str, months: i32) {
        let group = phrases(invader);
        let reason = if invader == invader::PHARAOH && self.ratings.kingdom <= 0 { format!("{group}_because_of_low_favor") } else { format!("{group}_no_reason_A") };
        // "A Hyksos army" and the like, for foreign invaders.
        let army = self.defs.armies.get(self.invasions.nation as usize).and_then(|a| a.name).map(|n| n + 28);
        self.post_event_text(EventText { title: format!("{group}_title"), body: body.to_owned(), reason, months, template: 131, army, ..Default::default() });
    }

    /// The army arrives at its invasion point.
    fn launch_invasion(&mut self, i: usize) {
        let inv = self.invasions.planned[i].clone();
        let amount = self.roll_pick(inv.amount).max(1);
        let point = self.roll_pick(inv.point);
        {
            let p = &mut self.invasions.planned[i];
            p.announced = false;
            if p.recurring {
                let (min, max) = (p.interval.0 as i32, p.interval.1 as i32);
                let years = if min >= 0 && max > min { min + self.rng.below(max - min + 1) } else { min.max(max) };
                p.year += years.max(1);
            } else if p.by_favour || p.via_event {
                p.armed = false;
            } else {
                p.done = true;
            }
        }
        let by_sea = (9..=16).contains(&point) && !self.invasions.sea_points.is_empty();
        // Land and Bedouin attacks come in the difficulty's share of their size; no
        // army is ever more than 150.
        let scaled = if by_sea && inv.invader != invader::BEDOUIN { amount } else { amount * self.by_difficulty(crate::difficulty::INVASION_PCT) / 100 };
        let amount = scaled.clamp(1, 150);
        let spot = if by_sea {
            self.invasions.sea_points.get(point as usize - 9).copied()
        } else {
            self.invasions.land_points.get((point.max(1) - 1) as usize).copied()
        }
        .or_else(|| self.invasions.land_points.first().copied())
        .unwrap_or(self.entry_point);
        let nation = match inv.invader {
            invader::BEDOUIN => self.defs.army("bedouin"),
            invader::EGYPT | invader::PHARAOH => self.defs.army("egyptian"),
            _ => None,
        }
        .map_or(self.invasions.nation, |n| n as u16);
        let army = self.invasions.armies.len();
        self.invasions.armies.push(Army { invader: inv.invader, nation, priority: inv.target, morale: 100, entry: spot, ..Default::default() });
        if by_sea {
            // The scenario's warships come too (byte 58 of the event), Egyptian ones
            // for Pharaoh's or an Egyptian city's army.
            let warships = inv.warships.or_else(|| inv.event.and_then(|e| self.scenario_events.list.get(e)).map(|e| e.god as u8 as i32)).unwrap_or(0);
            self.launch_sea_invasion(army, nation, amount, spot, warships, matches!(inv.invader, invader::EGYPT | invader::PHARAOH));
        } else {
            let spot = self.nearest_land(spot).unwrap_or(spot);
            self.invasions.armies[army].entry = spot;
            self.put_ashore(army, nation, amount, spot);
        }
        let group = phrases(inv.invader);
        self.post_invasion_text(inv.invader, &format!("{group}_city_attacked_alert"), 0);
        // The army's arrival leads on to whatever event follows it (the next wave).
        if let Some(on) = inv.event.and_then(|e| self.scenario_events.list.get(e)).map(|e| e.on_completed) {
            self.follow(on, inv.event.unwrap_or(0), crate::scenario_events::Outcome::Completed);
        }
        if let Some(n) = self.notices.log.last_mut() {
            n.tile = Some(spot);
        }
    }

    /// Puts `men` of army `army` on the field at `spot`: its arms in proportion to their
    /// frequency in the figure model, among the arms its nation has art for, each arm
    /// in formations as the original raises them, as many as there are free slots for.
    pub(crate) fn put_ashore(&mut self, army: usize, nation: u16, men: i32, spot: (i32, i32)) {
        let invader = self.invasions.armies.get(army).map_or(invader::ENEMY, |a| a.invader);
        let mut arms: Vec<(u16, i32)> = if invader == invader::BEDOUIN {
            vec![(BEDOUIN, 1)]
        } else {
            [ENEMY_INFANTRY, ENEMY_ARCHER, ENEMY_CHARIOT]
                .into_iter()
                .enumerate()
                .filter(|&(i, _)| self.defs.armies.get(nation as usize).is_some_and(|a| a.arms[i].is_some()))
                .map(|(_, k)| {
                    let probe = Figure { kind: k, cargo: nation, ..Figure::new(0, k, 0, 0, Travel::Land) };
                    (k, self.invader_stats(&probe).map_or(0, |s| if s.hp > 0 { s.frequency.max(1) } else { 0 }))
                })
                .filter(|a| a.1 > 0)
                .collect()
        };
        if arms.is_empty() {
            arms.push((ENEMY_INFANTRY, 1));
        }
        let total: i32 = arms.iter().map(|a| a.1).sum::<i32>().max(1);
        let first = self.invasions.armies.get(army).map_or(0, |a| a.figures.len() as i32);
        // How many men of each arm.
        let mut counts = vec![0; arms.len()];
        for n in first..first + men {
            let mut pick = (n * 37 + self.rng.below(total)) % total;
            let i = arms.iter().position(|a| {
                pick -= a.1;
                pick < 0
            });
            counts[i.unwrap_or(0)] += 1;
        }
        let mut n = first;
        for (i, &(k, _)) in arms.iter().enumerate() {
            for size in formation_sizes(counts[i]) {
                if self.enemy_formations() >= ENEMY_FORMATION_SLOTS {
                    return;
                }
                let Some(band) = self.invasions.armies.get_mut(army).map(|a| {
                    a.bands = a.bands.saturating_add(1);
                    a.bands - 1
                }) else {
                    return;
                };
                for _ in 0..size {
                    let fid = self.figures.spawn(k, spot.0, spot.1, Travel::Hostile);
                    if let Some(f) = self.figures.get_mut(fid) {
                        f.cargo = nation;
                        f.formation = 1000 + army as u16;
                        f.slot = (n % 16) as u8;
                        f.band = band;
                        f.action = 1;
                    }
                    if let Some(a) = self.invasions.armies.get_mut(army) {
                        a.figures.push(fid);
                    }
                    n += 1;
                }
            }
        }
    }

    /// The formation slots in use by invaders and herds: each herd, each loaded
    /// transport (its men are a formation afloat), and each formation of an army with
    /// anyone left standing in it.
    pub(crate) fn enemy_formations(&self) -> usize {
        let mut bands: Vec<(usize, u8)> = Vec::new();
        for (i, a) in self.invasions.armies.iter().enumerate() {
            for &fid in &a.figures {
                if let Some(f) = self.figures.get(fid)
                    && !f.dead
                    && f.action != action::CORPSE
                    && !bands.contains(&(i, f.band))
                {
                    bands.push((i, f.band));
                }
            }
        }
        let afloat = self.figures.iter().filter(|f| crate::navy::is_enemy_transport(f.kind) && !f.dead && f.amount > 0).count();
        bands.len() + afloat + self.herds.len()
    }

    /// Another event has led to the invasion planned from scenario event `event`: it
    /// sets out now and arrives when its months of warning have passed.
    pub(crate) fn arm_invasion(&mut self, event: usize) {
        let now = self.months_now();
        if let Some(inv) = self.invasions.planned.iter_mut().find(|p| p.via_event && p.event == Some(event) && !p.armed) {
            inv.armed = true;
            inv.announced = false;
            let arrive = now + inv.warning.max(1);
            inv.year = arrive / 12;
            inv.month = arrive % 12;
        }
    }

    /// Sends an army at once (for tests and the scripted harness).
    pub fn invade_now(&mut self, invader: u8, amount: i32, point: i32) {
        self.invade_by_sea_now(invader, amount, point, 0);
    }

    /// Sends an army at once, with `warships` if it comes by sea.
    pub fn invade_by_sea_now(&mut self, invader: u8, amount: i32, point: i32, warships: i32) {
        self.invasions.planned.push(Invasion {
            invader,
            warships: Some(warships),
            amount: Pick { value: amount as i16, fixed: amount as i16, min: -1, max: -1 },
            point: Pick { value: point as i16, fixed: point as i16, min: -1, max: -1 },
            ..Default::default()
        });
        let i = self.invasions.planned.len() - 1;
        self.launch_invasion(i);
    }

    fn roll_pick(&mut self, p: Pick) -> i32 {
        let (fixed, min, max) = (p.fixed as i32, p.min as i32, p.max as i32);
        if max == -1 || (fixed == -1 && min == max) {
            return if fixed >= 0 { fixed } else { p.value as i32 };
        }
        if fixed < 0 {
            return if max > min { min + self.rng.below(max - min) } else { p.value as i32 };
        }
        let choices = if min < 0 { 1 } else if max > -1 { 3 } else { 2 };
        match self.rng.below(choices) {
            0 => fixed,
            1 => min,
            _ => max,
        }
    }

    /// The nearest walkable land tile to `p`.
    fn nearest_land(&self, p: (i32, i32)) -> Option<(i32, i32)> {
        for r in 0..20 {
            for dy in -r..=r {
                for dx in -r..=r {
                    let (x, y) = (p.0 + dx, p.1 + dy);
                    if (dx.abs() == r || dy.abs() == r) && crate::figures::passable(&self.map, Travel::Land, x, y) {
                        return Some((x, y));
                    }
                }
            }
        }
        None
    }

    /// The nearest building an invader at `from` can walk up to, and the tile beside it
    /// he would stand on: what he batters when his way to the target is shut.
    fn nearest_reachable_building(&self, from: (i32, i32)) -> Option<(BuildingId, (i32, i32))> {
        let map = &self.map;
        let w = map.width;
        if !map.contains(from.0, from.1) {
            return None;
        }
        // Invaders hemmed in together ask the same question, often in the same tick:
        // the answer holds while the terrain and buildings stay as they are.
        type Key = (u64, u64, u64, (i32, i32));
        type Found = Option<(BuildingId, (i32, i32))>;
        thread_local! {
            static RECENT: std::cell::RefCell<std::collections::VecDeque<(Key, Found)>> = const { std::cell::RefCell::new(std::collections::VecDeque::new()) };
        }
        let key = (map.terrain.version(), map.building.version(), self.buildings.generation(), from);
        if let Some(found) = RECENT.with_borrow(|r| r.iter().find(|e| e.0 == key).map(|e| e.1)) {
            return found;
        }
        let found = crate::figures::with_scratch(map, |s| {
            s.seen[(from.1 * w + from.0) as usize] = s.stamp;
            s.queue.push(from);
            let mut head = 0;
            while let Some(&(x, y)) = s.queue.get(head) {
                head += 1;
                for (dx, dy) in [(0, -1), (1, 0), (0, 1), (-1, 0), (1, -1), (1, 1), (-1, 1), (-1, -1)] {
                    let id = map.building.at_or(x + dx, y + dy, 0);
                    if id != 0 && self.buildings.get(id).is_some_and(|b| !matches!(b.kind, crate::military::FORT_GROUND | kind::ROAD | kind::BURNING_RUIN)) {
                        return Some((id, (x, y)));
                    }
                }
                for (dx, dy) in [(0, -1), (1, 0), (0, 1), (-1, 0)] {
                    let (nx, ny) = (x + dx, y + dy);
                    if !map.contains(nx, ny) {
                        continue;
                    }
                    let n = (ny * w + nx) as usize;
                    if s.seen[n] != s.stamp && s.passable(map, Travel::Hostile, nx, ny) {
                        s.seen[n] = s.stamp;
                        s.queue.push((nx, ny));
                    }
                }
            }
            None
        });
        RECENT.with_borrow_mut(|r| {
            if r.len() >= 16 {
                r.pop_front();
            }
            r.push_back((key, found));
        });
        found
    }

    /// The tiles an invader at `from` could get to were he to batter through every
    /// wall and building in his way (see `crossable`), as a map-sized mask. Invaders
    /// in one stretch of land share the answer while the map stays as it is.
    fn land_reach(&self, from: (i32, i32)) -> std::rc::Rc<Vec<bool>> {
        let map = &self.map;
        let w = map.width;
        type Key = (u64, u64, u64);
        thread_local! {
            static RECENT: std::cell::RefCell<std::collections::VecDeque<(Key, std::rc::Rc<Vec<bool>>)>> = const { std::cell::RefCell::new(std::collections::VecDeque::new()) };
        }
        let key = (map.terrain.version(), map.building.version(), self.buildings.generation());
        let at = (from.1 * w + from.0) as usize;
        let size = (w * map.height).max(0) as usize;
        if let Some(r) = RECENT.with_borrow(|r| r.iter().find(|l| l.0 == key && l.1.len() == size && l.1.get(at).copied().unwrap_or(false)).map(|l| l.1.clone())) {
            return r;
        }
        let mut seen = vec![false; size];
        if map.contains(from.0, from.1) {
            seen[at] = true;
            let mut queue = vec![from];
            let mut head = 0;
            while let Some(&(x, y)) = queue.get(head) {
                head += 1;
                for (dx, dy) in [(0, -1), (1, 0), (0, 1), (-1, 0)] {
                    let (nx, ny) = (x + dx, y + dy);
                    if !map.contains(nx, ny) {
                        continue;
                    }
                    let n = (ny * w + nx) as usize;
                    if !seen[n] && crossable(map, nx, ny) {
                        seen[n] = true;
                        queue.push((nx, ny));
                    }
                }
            }
        }
        let r = std::rc::Rc::new(seen);
        RECENT.with_borrow_mut(|l| {
            l.retain(|e| e.0 == key);
            if l.len() >= 4 {
                l.pop_front();
            }
            l.push_back((key, r.clone()));
        });
        r
    }

    /// Whether any tile of building `id` is in `reach`.
    fn within_reach(&self, id: BuildingId, reach: &[bool]) -> bool {
        let Some(b) = self.buildings.get(id) else { return false };
        let (w, h) = b.footprint();
        let mw = self.map.width;
        (b.y..b.y + h).any(|y| (b.x..b.x + w).any(|x| self.map.contains(x, y) && reach[(y * mw + x) as usize]))
    }

    /// What an army goes for: the buildings its orders name, nearest first, among
    /// those it can get at over land from `from` (FUN_004ba3b0 only looks at
    /// buildings its "through everything" search from the army reached).
    fn choose_target(&self, army: usize, from: (i32, i32)) -> Option<BuildingId> {
        let priority = self.invasions.armies.get(army)?.priority;
        let weight = |k: u16, level: u8| -> i32 {
            match priority {
                0 => matches!(k, kind::GRANARY | kind::STORAGE_YARD | kind::BAZAAR) as i32 * 10 + self.is_farm(k) as i32 * 8,
                1 => (kind::PALACES.contains(&k) || k == kind::TAX_COLLECTOR) as i32 * 10,
                2 => level as i32,
                3 => matches!(k, crate::military::FORT_ARCHERS | crate::military::FORT_INFANTRY | crate::military::FORT_CHARIOTEERS | 55 | 94) as i32 * 10,
                _ => 1,
            }
        };
        let reach = self.land_reach(from);
        self.buildings
            .iter()
            .filter(|b| !matches!(b.kind, crate::military::FORT_GROUND | kind::ROAD | kind::BURNING_RUIN) && !crate::defenses::is_defense(b.kind))
            .filter(|b| self.within_reach(b.id, &reach))
            .map(|b| {
                let level = b.house.as_ref().map_or(0, |h| h.level + 1);
                let d = (b.x - from.0).abs() + (b.y - from.1).abs();
                (weight(b.kind, level).max(1) * 100 - d, b.id)
            })
            .max_by_key(|&(score, id)| (score, std::cmp::Reverse(id)))
            .map(|(_, id)| id)
    }

    /// Twice a day, as the original updates its formations (FUN_004b8cf0, at ticks 5
    /// and 29): each army looks again for something to attack from where its leading
    /// formation stands, unless the city's soldiers are near. When a search finds no
    /// building it can get at, even by battering through walls (a city across water,
    /// or none left), the leading formation counts it, and at the 49th search in a
    /// row it gives up: its men march back to where they came in and leave the map
    /// (figure action 3 in FUN_004991a0, which removes them when they arrive or find
    /// no way there). The next formation then takes the lead and counts afresh.
    pub(crate) fn update_armies(&mut self) {
        for a in 0..self.invasions.armies.len() {
            if self.invasions.armies[a].fleeing {
                continue;
            }
            let members: Vec<(FigureId, u8, u16, i32, i32)> = self.invasions.armies[a]
                .figures
                .iter()
                .filter_map(|&fid| self.figures.get(fid))
                .filter(|f| !f.dead && f.action != action::CORPSE)
                .map(|f| (f.id, f.band, f.action, f.x, f.y))
                .collect();
            // Formations that gave up keep leaving, once their fights are over.
            let entry = self.invasions.armies[a].entry;
            let withdrawn = self.invasions.armies[a].withdrawn.clone();
            for &(fid, band, act, _, _) in &members {
                if withdrawn.contains(&band) && !matches!(act, action::ATTACK | FLEEING) {
                    self.withdraw(fid, entry);
                }
            }
            let Some(&(_, band, _, x, y)) = members.iter().filter(|m| !withdrawn.contains(&m.1) && m.2 != FLEEING).min_by_key(|m| m.1) else { continue };
            let soldiers_near = self.figures.iter().any(|f| crate::military::is_soldier(f.kind) && !f.dead && f.action != action::CORPSE && (f.x - x).abs() <= SOLDIERS_NEAR && (f.y - y).abs() <= SOLDIERS_NEAR);
            if soldiers_near {
                continue;
            }
            let target = self.invasions.armies[a].target;
            let found = if self.buildings.get(target).is_some() && self.within_reach(target, &self.land_reach((x, y))) {
                true
            } else {
                let t = self.choose_target(a, (x, y));
                self.invasions.armies[a].target = t.unwrap_or(0);
                t.is_some()
            };
            let army = &mut self.invasions.armies[a];
            if found {
                army.failed_searches = 0;
                continue;
            }
            army.failed_searches = army.failed_searches.saturating_add(1);
            if army.failed_searches > GIVE_UP_AFTER {
                army.failed_searches = 0;
                army.withdrawn.push(band);
                for &(fid, b, act, _, _) in &members {
                    if b == band && !matches!(act, action::ATTACK | FLEEING) {
                        self.withdraw(fid, entry);
                    }
                }
            }
        }
    }

    /// An invader whose formation has given up heads for `entry`, and leaves.
    fn withdraw(&mut self, fid: FigureId, entry: (i32, i32)) {
        let map = &self.map;
        if let Some(f) = self.figures.get_mut(fid) {
            f.action = FLEEING;
            f.foe = 0;
            f.go_to(map, entry);
        }
    }

    /// An army loses a man: its morale drops, and when it breaks the army flees.
    pub(crate) fn army_loses(&mut self, army: usize) {
        // The share of the army still standing that he was.
        let standing = self.invasions.armies.get(army).map_or(0, |a| a.figures.iter().filter(|&&g| self.figures.get(g).is_some_and(|f| !f.dead && f.action != action::CORPSE)).count() as i32);
        let Some(a) = self.invasions.armies.get_mut(army) else { return };
        a.morale = (a.morale - crate::military::morale_loss(if standing > 0 { 100 / standing } else { 0 })).max(0);
        if a.morale <= crate::military::BROKEN_MORALE && !a.fleeing {
            a.fleeing = true;
            let (entry, figures) = (a.entry, a.figures.clone());
            let map = &self.map;
            for fid in figures {
                if let Some(f) = self.figures.get_mut(fid)
                    && f.action != action::CORPSE
                {
                    f.action = FLEEING;
                    f.foe = 0;
                    f.go_to(map, entry);
                }
            }
        }
    }

    /// An invader's turn: fight what is at hand, chase soldiers near by, else march on
    /// the army's target and wreck it.
    pub(crate) fn update_invader(&mut self, fid: FigureId) {
        let Some(f) = self.figures.get(fid) else { return };
        let (act, kind, x, y) = (f.action, f.kind, f.x, f.y);
        match act {
            action::CORPSE => return self.update_invader_corpse(fid),
            action::ATTACK => return self.fight(fid),
            FLEEING => {
                // Running for the edge of the map, and gone once there.
                let map = &self.map;
                let f = self.figures.get_mut(fid).expect("present");
                if f.walk(map) != Step::Moving {
                    f.dead = true;
                    let army = f.formation.saturating_sub(1000) as usize;
                    if let Some(a) = self.invasions.armies.get_mut(army) {
                        a.figures.retain(|&g| g != fid);
                    }
                }
                return;
            }
            _ => {}
        }
        if kind == ENEMY_ARCHER {
            self.shoot_at_foes(fid);
        }
        self.engage(fid, 1);
        if self.figures.get(fid).is_some_and(|f| f.action == action::ATTACK) {
            return;
        }
        let army = self.figures.get(fid).map_or(0, |f| f.formation.saturating_sub(1000)) as usize;
        // Soldiers close by draw them off, those not already beset by two first.
        let near = self.nearest_open_foe(true, (x, y), CHASE_RANGE).or_else(|| self.nearest_foe(true, (x, y), CHASE_RANGE)).map(|o| (o.1, o.2));
        let map = &self.map;
        if let Some(to) = near {
            let f = self.figures.get_mut(fid).expect("present");
            if !f.moving && f.destination != Some(to) {
                f.go_to(map, to);
            }
            f.walk(map);
            return;
        }
        // March on the target.
        let target = self.invasions.armies.get(army).map_or(0, |a| a.target);
        let target = if self.buildings.get(target).is_some() {
            target
        } else {
            let t = self.choose_target(army, (x, y)).unwrap_or(0);
            if let Some(a) = self.invasions.armies.get_mut(army) {
                a.target = t;
            }
            t
        };
        let Some(b) = self.buildings.get(target) else { return };
        let (bx, by) = (b.x, b.y);
        let (w, h) = b.footprint();
        let battering = crate::defenses::is_defense(b.kind);
        let adjacent = x >= bx - 1 && x <= bx + w && y >= by - 1 && y <= by + h;
        if adjacent {
            let f = self.figures.get_mut(fid).expect("present");
            f.moving = false;
            f.route.clear();
            f.attack_tick += 1;
            // The defences take a blow every 4 ticks and fall once past their count.
            let hp = crate::defenses::hit_points(b.kind);
            if f.attack_tick < if hp.is_some() { 4 } else { 24 } {
                return;
            }
            f.attack_tick = 0;
            let attack = if hp.is_some() { 1 } else { self.fighter_stats(fid).attack.max(1) };
            let Some(b) = self.buildings.get_mut(target) else { return };
            b.enemy_damage += attack;
            if b.enemy_damage > hp.unwrap_or(BUILDING_HP) {
                // Walls crumble; everything else is put to the torch.
                if crate::defenses::is_defense(b.kind) {
                    self.wreck(target, false);
                } else {
                    self.plunder(target);
                    self.destroy(target, true);
                }
                // Its id may go to the ruin: look for a new target.
                if let Some(a) = self.invasions.armies.get_mut(army) {
                    a.target = 0;
                }
            }
            return;
        }
        // After a failed search for a way, wait a while before searching again.
        if let Some(f) = self.figures.get_mut(fid)
            && f.counter > 0
        {
            f.counter -= 1;
            return;
        }
        // A figure already headed for a spot keeps going rather than repicking one
        // every tick: the ring below is sorted by distance from the figure's current
        // position, so as it walks the "nearest" slot keeps changing, which would
        // otherwise force an expensive pathfind almost every tick instead of only
        // when the figure actually needs a new route.
        let f = self.figures.get(fid).expect("present");
        let need_route = f.destination.is_none() || (!f.moving && f.route.is_empty());
        if need_route {
            // Attackers spread around the building rather than crowd one tile.
            let slot = f.slot as usize;
            let mut ring: Vec<(i32, i32)> = (by - 1..=by + h)
                .flat_map(|yy| (bx - 1..=bx + w).map(move |xx| (xx, yy)))
                .filter(|&(xx, yy)| (xx == bx - 1 || yy == by - 1 || xx == bx + w || yy == by + h) && crate::figures::passable(&self.map, Travel::Hostile, xx, yy))
                .collect();
            ring.sort_by_key(|&(xx, yy)| (xx - x).abs() + (yy - y).abs());
            // A wall is attacked from the near side.
            let pick = if battering { 0 } else { slot % ring.len().max(1) };
            let spot = ring.get(pick).copied().unwrap_or((bx, by));
            let map = &self.map;
            let f = self.figures.get_mut(fid).expect("present");
            if !f.go_to(map, spot) {
                // Hemmed in, by walls or by the city itself: as in the original, they
                // batter whatever stands in their way, the nearest building they can
                // get at. If there is none (an island across water, say), back off
                // further each consecutive failure so a permanently unreachable target
                // isn't searched for (a full-map scan) again every 50 ticks forever.
                f.destination = Some(spot);
                f.counter = stuck_backoff(f.stuck);
                f.stuck = f.stuck.saturating_add(1);
                if let Some((w, tile)) = self.nearest_reachable_building((x, y)) {
                    if let Some(a) = self.invasions.armies.get_mut(army) {
                        a.target = w;
                    }
                    let map = &self.map;
                    if let Some(f) = self.figures.get_mut(fid)
                        && f.go_to(map, tile)
                    {
                        f.counter = 0;
                    }
                }
                return;
            }
            if let Some(f) = self.figures.get_mut(fid) {
                f.stuck = 0;
            }
        }
        let map = &self.map;
        let f = self.figures.get_mut(fid).expect("present");
        if f.walk(map) == Step::Lost {
            f.route.clear();
        }
    }

    /// Fallen invaders lie a while; an army with none left standing is beaten.
    fn update_invader_corpse(&mut self, fid: FigureId) {
        let Some(f) = self.figures.get_mut(fid) else { return };
        f.counter += 1;
        if f.counter < 200 {
            return;
        }
        f.dead = true;
        let army = f.formation.saturating_sub(1000) as usize;
        if let Some(a) = self.invasions.armies.get_mut(army) {
            a.figures.retain(|&g| g != fid);
            if a.figures.is_empty() && a.invader == invader::PHARAOH && self.ratings.kingdom < 35 {
                // Beating back Pharaoh's army earns grudging respect.
                self.ratings.change_kingdom(10);
            }
        }
    }

    /// Daily: a city with invaders in it falls when its people have dwindled to a
    /// quarter of their peak and the invaders outnumber its soldiers, or are gone
    /// ("Defeat!" first, then the game ends).
    pub(crate) fn check_siege(&mut self) {
        self.invasions.peak_population = self.invasions.peak_population.max(self.population);
        if self.invasions.lost {
            return;
        }
        let invaders = self.figures.iter().filter(|f| is_invader_kind(f.kind) && f.action != action::CORPSE).count() as i32;
        if invaders == 0 {
            return;
        }
        let soldiers = self.figures.iter().filter(|f| crate::military::is_soldier(f.kind) && f.action != action::CORPSE).count() as i32;
        let dwindled = self.population < self.invasions.peak_population / 4 && invaders > 2 + soldiers;
        // (A city not yet settled cannot fall.)
        if self.invasions.peak_population > 0 && (dwindled || self.population <= 0) {
            self.invasions.lost = true;
            self.lose(true);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stuck_backoff_grows_then_caps() {
        // A single failure keeps the original 50-tick retry (temporary blockages,
        // like a wall going up, should still be noticed quickly).
        assert_eq!(stuck_backoff(0), 50);
        // Repeated failures back off exponentially...
        assert_eq!((stuck_backoff(1), stuck_backoff(2), stuck_backoff(3), stuck_backoff(4)), (100, 200, 400, 800));
        // ...capped so it never stalls forever.
        assert_eq!((stuck_backoff(5), stuck_backoff(6), stuck_backoff(255)), (1600, 1600, 1600));
    }

    /// Campaign mission 20 (with the real game data, when present).
    fn mission(n: usize) -> Option<World> {
        let data = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../PharaohData");
        if !data.join("mission1.pak").is_file() {
            return None;
        }
        let library = osiris_formats::ImageLibrary::open(&data.join("Data")).ok()?;
        let defs = std::sync::Arc::new(crate::defs::Defs::load(&library).ok()?);
        let model = osiris_formats::Model::parse(&String::from_utf8_lossy(&std::fs::read(data.join("Pharaoh_Model_Normal.txt")).ok()?)).ok()?;
        let balance = std::sync::Arc::new(crate::balance::Balance::from_model(&model));
        let scenario = osiris_formats::MissionPak::open(&data.join("mission1.pak")).ok()?.scenario(n).ok()?;
        let mut world = World::new(&scenario, defs, balance);
        world.start(&scenario);
        world.load_mission(n as i32);
        Some(world)
    }

    #[test]
    fn invasions_only_via_event_wait_for_their_event() {
        let Some(mut world) = mission(20) else { return };
        // Mission 20's later waves of Pharaoh's army follow the one his lost favour
        // sends; none comes while the kingdom rating holds.
        assert!(world.invasions.planned.iter().filter(|p| p.via_event).count() >= 3);
        for _ in 0..3000 {
            world.tick();
        }
        assert!(world.invasions.armies.is_empty(), "no army without a cause");
        // Favour gone: the first army comes, and the next wave follows it.
        for _ in 0..40_000 {
            world.ratings.kingdom = 0;
            world.tick();
            if world.invasions.armies.len() >= 2 {
                break;
            }
        }
        assert!(world.invasions.armies.len() >= 2, "armies {}", world.invasions.armies.len());
    }

    #[test]
    fn hemmed_in_invaders_find_a_building_to_batter() {
        use crate::map::terrain;
        let Some(mut world) = mission(1) else { return };
        let (w, h) = (world.map.width, world.map.height);
        // A well with open ground for five tiles west of it.
        let clear = |world: &World, x: i32, y: i32| (x - 6..=x).all(|xx| world.can_place(kind::WELL, xx, y).is_ok());
        let (x, y) = (6..h - 1).flat_map(|y| (6..w - 1).map(move |x| (x, y))).find(|&(x, y)| clear(&world, x, y)).expect("open ground");
        assert!(matches!(world.apply(&crate::world::Command::Build { kind: kind::WELL, x, y, x1: x, y1: y }), crate::world::Outcome::Done { .. }));
        let well = world.map.building.at_or(x, y, 0);
        // Rubble between the invader and the well is no bar.
        for xx in x - 4..x - 1 {
            world.map.terrain.set(xx, y, terrain::RUBBLE);
        }
        let found = world.nearest_reachable_building((x - 5, y));
        assert_eq!(found, Some((well, (x - 1, y))));
    }

    #[test]
    fn arms_split_into_formations_as_the_original_does() {
        assert_eq!(formation_sizes(0), Vec::<i32>::new());
        assert_eq!(formation_sizes(16), vec![16]);
        assert_eq!(formation_sizes(17), vec![9, 8]);
        assert_eq!(formation_sizes(32), vec![16, 16]);
        assert_eq!(formation_sizes(50), vec![18, 16, 16]);
    }

    /// A well, and open ground far from it to strand an army on.
    fn well_and_open_ground(world: &mut World) -> (i32, i32) {
        let (w, h) = (world.map.width, world.map.height);
        let (x, y) = (6..h - 1).flat_map(|y| (6..w - 1).map(move |x| (x, y))).find(|&(x, y)| world.can_place(kind::WELL, x, y).is_ok()).expect("open ground");
        assert!(matches!(world.apply(&crate::world::Command::Build { kind: kind::WELL, x, y, x1: x, y1: y }), crate::world::Outcome::Done { .. }));
        let open = |world: &World, px: i32, py: i32| (px - 3..=px + 3).all(|xx| (py - 3..=py + 3).all(|yy| crate::figures::passable(&world.map, Travel::Hostile, xx, yy)));
        (6..h - 6).flat_map(|y| (6..w - 6).map(move |x| (x, y))).filter(|&(px, py)| (px - x).abs() + (py - y).abs() > 20).find(|&(px, py)| open(world, px, py)).expect("room for an island")
    }

    #[test]
    fn stranded_invaders_give_up_and_go_home() {
        use crate::map::terrain;
        let Some(mut world) = mission(1) else { return };
        let p = well_and_open_ground(&mut world);
        // An island of one tile's radius: nothing the army could batter its way to.
        for dy in -3..=3i32 {
            for dx in -3..=3i32 {
                if dx.abs().max(dy.abs()) >= 2 {
                    world.map.terrain.set(p.0 + dx, p.1 + dy, terrain::WATER);
                }
            }
        }
        world.invasions.land_points = vec![p];
        world.invasions.sea_points.clear();
        world.invade_now(invader::ENEMY, 40, 1);
        let men = world.invasions.armies[0].figures.len();
        let bands = world.invasions.armies[0].bands;
        assert!(men > 16 && bands >= 2, "men {men}, formations {bands}");
        let standing = |world: &World| world.figures.iter().filter(|f| is_invader_kind(f.kind) && !f.dead).count();
        // Before 49 searches (twice a day) the army still waits.
        for _ in 0..20 * 51 {
            world.tick();
        }
        assert_eq!(standing(&world), men, "gave up too soon");
        // Then the formations give up one after another and go.
        for _ in 0..(bands as usize * 26 + 5) * 51 {
            world.tick();
        }
        assert_eq!(standing(&world), 0, "invaders still stranded");
        assert!(world.invasions.armies[0].figures.is_empty());
    }

    #[test]
    fn walled_off_invaders_keep_attacking() {
        use crate::map::terrain;
        let Some(mut world) = mission(1) else { return };
        let p = well_and_open_ground(&mut world);
        // Walled in rather than cut off by water: walls can be battered down.
        for dy in -3..=3i32 {
            for dx in -3..=3i32 {
                if dx.abs().max(dy.abs()) == 2 {
                    world.map.terrain.set(p.0 + dx, p.1 + dy, terrain::WALL);
                }
            }
        }
        world.invasions.land_points = vec![p];
        world.invasions.sea_points.clear();
        world.invade_now(invader::ENEMY, 10, 1);
        for _ in 0..60 * 51 {
            world.tick();
        }
        let a = &world.invasions.armies[0];
        assert!(a.withdrawn.is_empty() && a.failed_searches == 0, "withdrawn {:?}, failed {}", a.withdrawn, a.failed_searches);
    }

    #[test]
    fn armies_raise_no_more_formations_than_there_are_slots() {
        let Some(mut world) = mission(1) else { return };
        let p = well_and_open_ground(&mut world);
        world.invasions.land_points = vec![p];
        world.invasions.sea_points.clear();
        // At most nine formations each (three arms of three), so twenty armies can't
        // all fit.
        for _ in 0..20 {
            world.invade_now(invader::ENEMY, 150, 1);
        }
        assert!(world.enemy_formations() <= ENEMY_FORMATION_SLOTS, "{}", world.enemy_formations());
        // The later armies find no room.
        assert!(world.invasions.armies.last().is_some_and(|a| a.figures.is_empty()));
        let men = world.figures.iter().filter(|f| is_invader_kind(f.kind)).count();
        assert!(!world.invasions.armies[0].figures.is_empty() && men < 20 * 150, "{men}");
    }
}
