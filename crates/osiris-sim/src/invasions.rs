//! Invasions. The scenario plans them as events: who attacks (a foreign nation,
//! an Egyptian army, Pharaoh's army or Bedouin raiders), how many, where they land
//! (land invasion points 1-8, sea points 9-16), when they arrive, how much warning the
//! city gets, and what they go for. The warnings come as the army draws near (when
//! first sighted, then at two years, one year, six months and one month), and then it
//! is upon the city.
//!
//! Invaders fight any soldier or constable who comes near and otherwise march on
//! their target, wrecking each building they reach. A city with few people left and
//! more invaders than soldiers is lost.

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
const TRIGGER_BY_FAVOUR: u8 = 16;
/// Action of a routed invader.
const FLEEING: u16 = 148;

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
            .filter(|e| e.kind == crate::scenario_events::event::INVASION)
            .map(|e| Invasion {
                invader: e.item.value.clamp(1, 4) as u8,
                amount: e.amount.into(),
                point: osiris_formats::EventValue { value: e.location[0], fixed: e.location[1], min: e.location[2], max: e.location[3] }.into(),
                year: e.year as i32,
                month: e.month as i32,
                warning: e.months as i32,
                target: e.attack_target,
                interval: (e.time.min, e.time.max),
                recurring: e.trigger == crate::scenario_events::trigger::RECURRING,
                by_favour: e.trigger == TRIGGER_BY_FAVOUR,
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
                }
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
            } else if p.by_favour {
                p.armed = false;
            } else {
                p.done = true;
            }
        }
        let by_sea = (9..=16).contains(&point) && !self.invasions.sea_points.is_empty();
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
        self.invasions.armies.push(Army { figures: Vec::new(), invader: inv.invader, nation, target: 0, priority: inv.target, morale: 100, fleeing: false, entry: spot });
        if by_sea {
            self.launch_sea_invasion(army, nation, amount, spot);
        } else {
            let spot = self.nearest_land(spot).unwrap_or(spot);
            self.invasions.armies[army].entry = spot;
            self.put_ashore(army, nation, amount, spot);
        }
        let group = phrases(inv.invader);
        self.post_invasion_text(inv.invader, &format!("{group}_city_attacked_alert"), 0);
        if let Some(n) = self.notices.log.last_mut() {
            n.tile = Some(spot);
        }
    }

    /// Puts `men` of army `army` on the field at `spot`: its arms in proportion to their
    /// frequency in the figure model, among the arms its nation has art for.
    pub(crate) fn put_ashore(&mut self, army: usize, nation: u16, men: i32, spot: (i32, i32)) {
        let invader = self.invasions.armies.get(army).map_or(invader::ENEMY, |a| a.invader);
        let arms: Vec<(u16, i32)> = if invader == invader::BEDOUIN {
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
        let total: i32 = arms.iter().map(|a| a.1).sum::<i32>().max(1);
        let first = self.invasions.armies.get(army).map_or(0, |a| a.figures.len() as i32);
        for n in first..first + men {
            let mut pick = (n * 37 + self.rng.below(total)) % total;
            let k = arms.iter().find(|a| {
                pick -= a.1;
                pick < 0
            });
            let k = k.map_or(ENEMY_INFANTRY, |a| a.0);
            let fid = self.figures.spawn(k, spot.0, spot.1, Travel::Hostile);
            if let Some(f) = self.figures.get_mut(fid) {
                f.cargo = nation;
                f.formation = 1000 + army as u16;
                f.slot = (n % 16) as u8;
                f.action = 1;
            }
            if let Some(a) = self.invasions.armies.get_mut(army) {
                a.figures.push(fid);
            }
        }
    }

    /// Sends an army at once (for tests and the scripted harness).
    pub fn invade_now(&mut self, invader: u8, amount: i32, point: i32) {
        self.invasions.planned.push(Invasion {
            invader,
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

    /// What an army goes for: the buildings its orders name, nearest first.
    fn choose_target(&self, army: usize, from: (i32, i32)) -> Option<BuildingId> {
        let priority = self.invasions.armies.get(army)?.priority;
        let weight = |k: u16, level: u8| -> i32 {
            match priority {
                0 => matches!(k, kind::GRANARY | kind::STORAGE_YARD | kind::BAZAAR) as i32 * 10 + self.is_farm(k) as i32 * 8,
                1 => matches!(k, kind::VILLAGE_PALACE | kind::TAX_COLLECTOR) as i32 * 10,
                2 => level as i32,
                3 => matches!(k, crate::military::FORT_ARCHERS | crate::military::FORT_INFANTRY | crate::military::FORT_CHARIOTEERS | 55 | 94) as i32 * 10,
                _ => 1,
            }
        };
        self.buildings
            .iter()
            .filter(|b| !matches!(b.kind, crate::military::FORT_GROUND | kind::ROAD | kind::BURNING_RUIN) && !crate::defenses::is_defense(b.kind))
            .map(|b| {
                let level = b.house.as_ref().map_or(0, |h| h.level + 1);
                let d = (b.x - from.0).abs() + (b.y - from.1).abs();
                (weight(b.kind, level).max(1) * 100 - d, b.id)
            })
            .max_by_key(|&(score, id)| (score, std::cmp::Reverse(id)))
            .map(|(_, id)| id)
    }

    /// An army loses a man: its morale drops, and when it breaks the army flees.
    pub(crate) fn army_loses(&mut self, army: usize) {
        let Some(a) = self.invasions.armies.get_mut(army) else { return };
        let size = a.figures.len().max(1) as i32;
        a.morale = (a.morale - crate::military::morale_loss(100 / size)).max(0);
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
        self.engage_as_invader(fid);
        if self.figures.get(fid).is_some_and(|f| f.action == action::ATTACK) {
            return;
        }
        let army = self.figures.get(fid).map_or(0, |f| f.formation.saturating_sub(1000)) as usize;
        // Soldiers close by draw them off.
        let near = self
            .figures
            .iter()
            .filter(|o| !o.dead && o.action != action::CORPSE && (crate::military::is_soldier(o.kind) || o.kind == crate::crime::CONSTABLE))
            .filter(|o| (o.x - x).abs() <= CHASE_RANGE && (o.y - y).abs() <= CHASE_RANGE)
            .min_by_key(|o| (o.x - x).abs() + (o.y - y).abs())
            .map(|o| (o.x, o.y));
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
            if f.attack_tick < 24 {
                return;
            }
            f.attack_tick = 0;
            let attack = self.fighter_stats(fid).attack.max(1);
            let Some(b) = self.buildings.get_mut(target) else { return };
            b.enemy_damage += attack;
            if b.enemy_damage > BUILDING_HP {
                // Walls crumble; everything else is put to the torch.
                if crate::defenses::is_defense(b.kind) {
                    self.wreck(target, false);
                } else {
                    self.destroy(target, true);
                }
                // Its id may go to the ruin: look for a new target.
                if let Some(a) = self.invasions.armies.get_mut(army) {
                    a.target = 0;
                }
            }
            return;
        }
        // Attackers spread around the building rather than crowd one tile.
        let slot = self.figures.get(fid).map_or(0, |f| f.slot) as usize;
        let mut ring: Vec<(i32, i32)> = (by - 1..=by + h)
            .flat_map(|yy| (bx - 1..=bx + w).map(move |xx| (xx, yy)))
            .filter(|&(xx, yy)| (xx == bx - 1 || yy == by - 1 || xx == bx + w || yy == by + h) && crate::figures::passable(&self.map, Travel::Land, xx, yy))
            .collect();
        ring.sort_by_key(|&(xx, yy)| (xx - x).abs() + (yy - y).abs());
        // A wall is attacked from the near side.
        let pick = if battering { 0 } else { slot % ring.len().max(1) };
        let spot = ring.get(pick).copied().unwrap_or((bx, by));
        // After a failed search for a way, wait a while before searching again.
        if let Some(f) = self.figures.get_mut(fid)
            && f.counter > 0
        {
            f.counter -= 1;
            return;
        }
        let map = &self.map;
        let f = self.figures.get_mut(fid).expect("present");
        if (f.destination != Some(spot) || (!f.moving && f.route.is_empty())) && !f.go_to(map, spot) {
            // Walled off: batter the nearest part of the wall instead.
            f.destination = Some(spot);
            f.counter = 50;
            let wall = self.nearest_defense((x, y), i32::MAX);
            if let (Some(w), Some(a)) = (wall, self.invasions.armies.get_mut(army))
                && a.target != w
            {
                a.target = w;
            }
            return;
        }
        if f.walk(map) == Step::Lost {
            f.route.clear();
        }
    }

    fn engage_as_invader(&mut self, fid: FigureId) {
        let Some(f) = self.figures.get(fid) else { return };
        let (x, y) = (f.x, f.y);
        let foe = self
            .figures
            .iter()
            .filter(|o| !o.dead && o.action != action::CORPSE && (crate::military::is_soldier(o.kind) || o.kind == crate::crime::CONSTABLE))
            .find(|o| (o.x - x).abs() <= 1 && (o.y - y).abs() <= 1)
            .map(|o| o.id);
        if let Some(foe) = foe
            && let Some(f) = self.figures.get_mut(fid)
        {
            f.foe = foe;
            f.action = action::ATTACK;
            f.attack_tick = 0;
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

    /// Daily: a city with invaders in it is lost when its people have dwindled to a
    /// quarter of their peak and the invaders outnumber its soldiers, or are gone.
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
        if dwindled || self.population <= 0 {
            self.invasions.lost = true;
            self.messages.push_back("message_mission_defeat".to_owned());
        }
    }
}
