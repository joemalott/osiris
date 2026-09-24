//! The plagues of frogs, locusts and hail, which Ptah, Osiris and Seth send in their
//! wrath and some scenarios send as events, and the wrecks Seth makes of the city's
//! boats.
//!
//! Frogs come out of the water or marsh nearest each house they are sent to, wait up
//! to three days, hop (at half a walker's pace) to a road beside it and jump in: its
//! people leave and nobody moves in for the months the plague has left. Locusts rise
//! from a point on the map's edge, nine to a floodplain farm, settle on its tiles and
//! eat the crop down to nothing over 32 days, then fly off. Hail darkens the city for
//! eight days and strikes down people in the streets. While a plague lasts its
//! ambient track takes the place of the music.

use crate::buildings::BuildingId;
use crate::figures::{FigureId, Step, Travel};
use crate::map::terrain;
use crate::world::World;

pub const SHIPWRECK: u16 = 67;
pub const FROG: u16 = 106;
pub const LOCUST: u16 = 107;

/// A month of ticks: a frog plague lasts as many as the event or curse says.
const MONTH_TICKS: i32 = 816;
/// Ticks the locusts stay once they have come, and what is left of them when they
/// have eaten everything and leave.
const LOCUST_TICKS: i32 = 2632;
const LOCUSTS_LEAVE: i32 = 1000;
/// Ticks a hailstorm lasts.
pub const HAIL_TICKS: i32 = 408;
/// Months of frogs Ptah sends.
pub const PTAH_FROG_MONTHS: i32 = 6;
/// Sentiment each house loses to locusts, frogs and hail.
const PLAGUE_SENTIMENT: i32 = -15;
/// Ticks a wreck's flotsam floats (one frame each).
pub const WRECK_TICKS: i32 = 36;
/// Ticks a figure struck down lies dying before it is gone, and the frame of its
/// death animation for each two of them.
pub const FALL_TICKS: i32 = 128;
pub const FALL_FRAMES: [u8; 64] = {
    let mut t = [6u8; 64];
    let mut i = 0;
    while i < 36 {
        t[i] = match i {
            0..=3 => i as u8,
            4..=15 => 4,
            _ => 5,
        };
        i += 1;
    }
    t
};

/// What frogs and locusts are doing.
pub mod action {
    /// Waiting where they appeared.
    pub const WAITING: u16 = 8;
    /// On their way: a frog to the road by its house, a locust to its farm tile.
    pub const GOING: u16 = 9;
    /// A frog jumping into its house; a locust eating.
    pub const ARRIVING: u16 = 10;
    /// A locust flying off.
    pub const LEAVING: u16 = 11;
}

/// The ambient track a plague plays in place of the music.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Track {
    Frogs,
    Locusts,
    Hailstorm,
}

impl Track {
    /// Under AUDIO/.
    pub fn file(self) -> &'static str {
        match self {
            Track::Frogs => "Ambient/Frogs.mp3",
            Track::Locusts => "Ambient/Locusts.mp3",
            Track::Hailstorm => "Ambient/Hailstorm.mp3",
        }
    }
}

/// Ticks left of each plague. Any of them running out ends the track, whichever
/// started it.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Plagues {
    pub frogs: i32,
    pub locusts: i32,
    pub hail: i32,
    pub track: Option<Track>,
}

impl World {
    /// Every tick: the plagues' time runs down.
    pub(crate) fn update_plagues(&mut self) {
        let p = &mut self.plagues;
        for t in [&mut p.frogs, &mut p.locusts, &mut p.hail] {
            if *t > 0 {
                *t -= 1;
                if *t == 0 {
                    p.track = None;
                }
            }
        }
    }

    /// Osiris's wrath, or a scenario's plague of locusts: they will come before the
    /// next flood (see `locusts_descend`).
    pub(crate) fn arm_locusts(&mut self) {
        self.religion.osiris_locusts = true;
        self.change_house_sentiment(PLAGUE_SENTIMENT);
    }

    /// The locusts come: nine to every floodplain farm, from a point near the map's
    /// edge (a random step back along its width from seven eighths across, three
    /// eighths down; both measured on the width, as the original does). Each farm's
    /// workers are gone.
    pub(crate) fn send_locusts(&mut self) {
        self.plagues.locusts = LOCUST_TICKS;
        self.plagues.track = Some(Track::Locusts);
        let farms: Vec<BuildingId> = self.buildings.ids().into_iter().filter(|&id| self.is_floodplain_farm(id)).collect();
        for id in farms {
            let Some(b) = self.buildings.get(id) else { continue };
            let (fx, fy) = (b.x, b.y);
            for i in 0..9 {
                let r = self.rng.below((self.map.width / 4).max(1));
                let from = self.edge_point(self.map.width * 7 / 8 - r, self.map.width * 3 / 8 - r);
                let fid = self.figures.spawn(LOCUST, from.0, from.1, Travel::Air);
                let wait = self.rng.below(300);
                if let Some(f) = self.figures.get_mut(fid) {
                    f.target = id;
                    f.action = action::WAITING;
                    f.counter = wait;
                    f.destination = Some((fx + i / 3, fy + i % 3));
                }
            }
            if let Some(b) = self.buildings.get_mut(id) {
                b.workers = 0;
            }
        }
    }

    fn edge_point(&self, x: i32, y: i32) -> (i32, i32) {
        (x.clamp(0, self.map.width - 1), y.clamp(0, self.map.height - 1))
    }

    pub(crate) fn update_locust(&mut self, fid: FigureId) {
        let Some(f) = self.figures.get(fid) else { return };
        let farm = f.target;
        if self.buildings.get(farm).is_none() {
            self.figures.get_mut(fid).expect("present").dead = true;
            return;
        }
        let left = self.plagues.locusts;
        let (w, act) = (self.map.width, f.action);
        match act {
            action::WAITING => {
                let map = &self.map;
                let f = self.figures.get_mut(fid).expect("present");
                f.anim_tick = 0;
                f.counter -= 1;
                if f.counter > 0 {
                    return;
                }
                let to = f.destination.unwrap_or((f.x, f.y));
                f.go_to(map, to);
                if f.route.is_empty() {
                    f.dead = true;
                } else {
                    f.action = action::GOING;
                }
            }
            action::GOING => {
                let map = &self.map;
                let f = self.figures.get_mut(fid).expect("present");
                if f.walk(map) != Step::Moving {
                    f.action = action::ARRIVING;
                }
            }
            action::ARRIVING => {
                self.figures.get_mut(fid).expect("present").anim_tick += 1;
                // The crop can grow no higher than the locusts' time left above the
                // point they leave at.
                if let Some(b) = self.buildings.get_mut(farm) {
                    b.progress = b.progress.min(left - LOCUSTS_LEAVE).max(0);
                }
                if left > LOCUSTS_LEAVE {
                    return;
                }
                let r = self.rng.below((w / 4).max(1));
                let to = self.edge_point(w * 3 / 8 - r + 2, w * 7 / 8 - r - 2);
                let map = &self.map;
                let f = self.figures.get_mut(fid).expect("present");
                f.action = action::LEAVING;
                f.go_to(map, to);
            }
            _ => {
                let map = &self.map;
                let f = self.figures.get_mut(fid).expect("present");
                if f.walk(map) != Step::Moving {
                    f.dead = true;
                }
            }
        }
    }

    /// Ptah's wrath, or a scenario's plague of frogs lasting `months`: from the
    /// finest houses down, houses holding up to 65% of the people are sought out, and
    /// a frog sets out for each with an even chance.
    pub(crate) fn plague_of_frogs(&mut self, months: i32) {
        self.plagues.frogs = months * MONTH_TICKS;
        let total: i32 = self.buildings.iter().filter_map(|b| b.house.as_ref()).map(|h| h.population).sum();
        let limit = total * 65 / 100;
        let mut houses: Vec<(u8, BuildingId, i32)> = self.buildings.iter().filter_map(|b| b.house.as_ref().map(|h| (h.level, b.id, h.population))).collect();
        houses.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        let mut sought = 0;
        for (_, id, pop) in houses {
            if sought >= limit {
                break;
            }
            sought += pop;
            if sought <= limit {
                self.send_frog(id);
            }
        }
        if self.plagues.frogs > 0 {
            self.plagues.track = Some(Track::Frogs);
        }
        self.change_house_sentiment(PLAGUE_SENTIMENT);
    }

    /// Even odds that a frog comes for house `id`, out of the nearer of the closest
    /// marsh and the closest water (neither, when they are as near).
    fn send_frog(&mut self, id: BuildingId) {
        if self.rng.below(100) >= 50 {
            return;
        }
        let Some(b) = self.buildings.get(id) else { return };
        let (x, y) = (b.x, b.y);
        let reach = self.map.width.max(self.map.height);
        let marsh = self.nearest_terrain(x, y, terrain::MARSHLAND, reach);
        let water = self.nearest_terrain(x, y, terrain::WATER, reach);
        let dist = |p: Option<(i32, i32)>| p.map_or(reach, |(px, py)| (((px - x) * (px - x) + (py - y) * (py - y)) as f64).sqrt() as i32);
        let (dm, dw) = (dist(marsh), dist(water));
        let spot = if dw < dm && dw < reach {
            water
        } else if dm < dw && dm < reach {
            marsh
        } else {
            None
        };
        let Some((sx, sy)) = spot else { return };
        let fid = self.figures.spawn(FROG, sx, sy, Travel::Amphibious);
        let wait = self.rng.below(150);
        if let Some(f) = self.figures.get_mut(fid) {
            f.target = id;
            f.action = action::WAITING;
            f.counter = wait;
        }
    }

    /// The first tile with any of `bits` (and no dike) in growing rings round
    /// `(x, y)`, each ring walked along its top, right, bottom and left sides.
    fn nearest_terrain(&self, x: i32, y: i32, bits: u32, reach: i32) -> Option<(i32, i32)> {
        let hit = |xx: i32, yy: i32| self.map.contains(xx, yy) && self.map.terrain_is(xx, yy, bits) && !self.map.terrain_is(xx, yy, terrain::DIKE);
        if hit(x, y) {
            return Some((x, y));
        }
        for r in 1..=reach {
            let top = (x - r..=x + r).map(|xx| (xx, y - r));
            let right = (y - r + 1..=y + r).map(|yy| (x + r, yy));
            let bottom = (x - r..x + r).rev().map(|xx| (xx, y + r));
            let left = (y - r + 1..y + r).rev().map(|yy| (x - r, yy));
            if let Some(p) = top.chain(right).chain(bottom).chain(left).find(|&(xx, yy)| hit(xx, yy)) {
                return Some(p);
            }
        }
        None
    }

    pub(crate) fn update_frog(&mut self, fid: FigureId) {
        let Some(f) = self.figures.get(fid) else { return };
        let house = f.target;
        let Some((hx, hy, size)) = self.buildings.get(house).filter(|b| b.house.is_some()).map(|b| (b.x, b.y, b.size)) else {
            self.figures.get_mut(fid).expect("present").dead = true;
            return;
        };
        match f.action {
            action::WAITING => {
                let road = crate::buildings::road_within(&self.map, hx, hy, size, 2);
                let map = &self.map;
                let f = self.figures.get_mut(fid).expect("present");
                f.anim_tick = 0;
                f.counter -= 1;
                if f.counter > 0 {
                    return;
                }
                match road {
                    Some(t) if f.go_to(map, t) => f.action = action::GOING,
                    _ => f.dead = true,
                }
            }
            action::GOING => {
                // Frogs hop on half the ticks.
                if self.rng.below(100) >= 50 {
                    self.figures.get_mut(fid).expect("present").anim_tick += 1;
                    return;
                }
                let map = &self.map;
                let f = self.figures.get_mut(fid).expect("present");
                match f.walk(map) {
                    Step::Moving => {}
                    Step::Arrived => {
                        f.action = action::ARRIVING;
                        f.travel = Travel::Air;
                        f.go_to(map, (hx, hy));
                    }
                    Step::Blocked => {
                        f.action = action::WAITING;
                        f.counter = 1;
                    }
                    Step::Lost => f.dead = true,
                }
            }
            _ => {
                let map = &self.map;
                let f = self.figures.get_mut(fid).expect("present");
                if f.walk(map) == Step::Moving {
                    return;
                }
                f.dead = true;
                let months = (self.plagues.frogs / 51 / 16).max(1);
                self.drive_out(house, months);
            }
        }
    }

    /// Hail from Seth or a scenario: darkness over the city for eight days, and in the
    /// streets half the soldiers, invaders, boats and dangerous beasts struck down and
    /// three in four of everyone else.
    pub(crate) fn hailstorm(&mut self) {
        self.hail_strikes();
        self.plagues.hail = HAIL_TICKS;
        self.plagues.track = Some(Track::Hailstorm);
        self.change_house_sentiment(PLAGUE_SENTIMENT);
    }

    /// Seth's great wrath: every fishing boat, warship and transport sinks, leaving
    /// flotsam a moment; the wharves will want new ones.
    pub(crate) fn sink_boats(&mut self) {
        for fid in self.figures.ids() {
            let Some(f) = self.figures.get_mut(fid) else { continue };
            if !matches!(f.kind, crate::fishing::FISHING_BOAT | crate::navy::WARSHIP | crate::navy::TRANSPORT) || f.dead || f.action == crate::military::action::CORPSE {
                continue;
            }
            f.kind = SHIPWRECK;
            f.home = 0;
            f.target = 0;
            f.action = 0;
            f.counter = 0;
            f.route.clear();
            f.moving = false;
            f.progress = 0;
        }
    }

    /// For scripts: frogs for `n` months, locusts at once, hail or sinking boats now.
    pub fn plague_now(&mut self, which: &str, n: i32) -> bool {
        match which {
            "frogs" => self.plague_of_frogs(n.max(1)),
            "locusts" => self.send_locusts(),
            "hail" => self.hailstorm(),
            "sink" => self.sink_boats(),
            _ => return false,
        }
        true
    }

    pub(crate) fn update_shipwreck(&mut self, fid: FigureId) {
        let Some(f) = self.figures.get_mut(fid) else { return };
        f.counter += 1;
        if f.counter >= WRECK_TICKS {
            f.dead = true;
        }
    }

    /// A figure struck down by hail lies dying, then is gone.
    pub(crate) fn update_fallen(&mut self, fid: FigureId) {
        let Some(f) = self.figures.get_mut(fid) else { return };
        f.counter += 1;
        if f.counter >= FALL_TICKS {
            f.dead = true;
        }
    }
}

/// Whether a figure of kind `k` lies dead by its own rules (fighters and warships)
/// rather than as one struck down by hail.
pub fn keeps_own_corpse(k: u16) -> bool {
    crate::military::is_soldier(k)
        || crate::invasions::is_invader_kind(k)
        || matches!(k, crate::navy::WARSHIP | crate::navy::TRANSPORT | crate::navy::ENEMY_TRANSPORT | crate::defenses::TOWER_SENTRY | crate::military::STANDARD_BEARER)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fall_frames_follow_the_originals_table() {
        assert_eq!(&FALL_FRAMES[..6], &[0, 1, 2, 3, 4, 4]);
        assert_eq!(FALL_FRAMES.iter().filter(|&&f| f == 4).count(), 12);
        assert_eq!(FALL_FRAMES.iter().filter(|&&f| f == 5).count(), 20);
        assert_eq!(FALL_FRAMES.iter().filter(|&&f| f == 6).count(), 28);
    }

    fn sandbox() -> Option<World> {
        let data = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../PharaohData");
        if !data.join("Maps/Sandbox.map").is_file() {
            return None;
        }
        let library = osiris_formats::ImageLibrary::open(&data.join("Data")).ok()?;
        let scenario = osiris_formats::Scenario::load_map(&data.join("Maps/Sandbox.map")).ok()?;
        let defs = std::sync::Arc::new(crate::defs::Defs::load(&library).ok()?);
        let model = osiris_formats::Model::parse(&String::from_utf8_lossy(&std::fs::read(data.join("Pharaoh_Model_Normal.txt")).ok()?)).ok()?;
        let balance = std::sync::Arc::new(crate::balance::Balance::from_model(&model));
        let mut world = World::new(&scenario, defs, balance);
        world.start(&scenario);
        Some(world)
    }

    /// Seth's boats become flotsam that floats 36 ticks, and the wharf loses its boat.
    #[test]
    fn sunk_boats_leave_flotsam_a_moment() {
        let Some(mut world) = sandbox() else { return };
        let fid = world.figures.spawn(crate::fishing::FISHING_BOAT, 3, 3, Travel::Water);
        world.figures.get_mut(fid).expect("spawned").home = 7;
        world.sink_boats();
        let f = world.figures.get(fid).expect("still there");
        assert_eq!((f.kind, f.home), (SHIPWRECK, 0));
        for _ in 0..WRECK_TICKS - 1 {
            world.update_shipwreck(fid);
        }
        assert!(!world.figures.get(fid).expect("floating").dead);
        world.update_shipwreck(fid);
        assert!(world.figures.get(fid).expect("gone next").dead);
    }

    /// A hailstorm's track plays until its eight days are out.
    #[test]
    fn a_plague_track_ends_with_its_time() {
        let Some(mut world) = sandbox() else { return };
        world.hailstorm();
        assert_eq!(world.plagues.track, Some(Track::Hailstorm));
        for _ in 0..HAIL_TICKS - 1 {
            world.update_plagues();
        }
        assert_eq!(world.plagues.track, Some(Track::Hailstorm));
        world.update_plagues();
        assert_eq!(world.plagues.track, None);
    }
}
