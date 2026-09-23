//! Entertainment. Performer schools train entertainers who walk to a venue that needs
//! one; the venue then puts on shows for a while and sends performers roaming the
//! streets from it, entertaining nearby houses.
//!
//! Booths take jugglers; bandstands take jugglers and musicians; pavilions take
//! jugglers, musicians and dancers.

use crate::buildings::{BuildingId, kind};
use crate::figures::{Step, Travel};
use crate::world::World;

pub const JUGGLER: u16 = 15;
pub const MUSICIAN: u16 = 16;
pub const DANCER: u16 = 17;
/// Days of shows a performer's arrival buys a venue.
const SHOW_DAYS: i32 = 32;
const GOING_TO_VENUE: u16 = 10;

/// Each performer: its school, and the venues it plays at.
const PERFORMERS: [(u16, u16, &[u16]); 3] = [
    (JUGGLER, kind::JUGGLER_SCHOOL, &[kind::BOOTH, kind::BANDSTAND, kind::PAVILION]),
    (MUSICIAN, kind::CONSERVATORY, &[kind::BANDSTAND, kind::PAVILION]),
    (DANCER, kind::DANCE_SCHOOL, &[kind::PAVILION]),
];

/// The venue slot (index into `shows` and `performers`) of a performer figure.
pub fn performer_slot(figure: u16) -> Option<usize> {
    PERFORMERS.iter().position(|p| p.0 == figure)
}

impl World {
    /// Tick 31: schools send performers to the venue that most needs one.
    pub(crate) fn school_walkers(&mut self) {
        for (slot, &(performer, school_kind, venues)) in PERFORMERS.iter().enumerate() {
            let schools: Vec<BuildingId> = self.buildings.iter().filter(|b| b.kind == school_kind).map(|b| b.id).collect();
            for school in schools {
                self.send_performer(school, slot, performer, venues);
            }
        }
    }

    fn send_performer(&mut self, school: BuildingId, slot: usize, performer: u16, venues: &[u16]) {
        let Some(s) = self.buildings.get(school) else { return };
        let Some(road) = s.road else { return };
        if s.workers <= 0 || s.walkers[0] != 0 {
            return;
        }
        if s.spawn_delay > 0 {
            self.buildings.get_mut(school).expect("present").spawn_delay -= 1;
            return;
        }
        let (sx, sy) = (s.x, s.y);
        let en_route: Vec<u32> =
            self.figures.iter().filter(|f| f.kind == performer && f.action == GOING_TO_VENUE).map(|f| f.target).collect();
        // As in the original, the nearest venue counting its remaining shows as distance.
        let venue = self
            .buildings
            .iter()
            .filter(|b| venues.contains(&b.kind) && b.workers > 0 && b.road.is_some() && !en_route.contains(&b.id))
            .filter(|b| b.shows[slot] < SHOW_DAYS / 2)
            .min_by_key(|b| b.shows[slot] + (b.x - sx).abs() + (b.y - sy).abs())
            .map(|b| (b.id, b.road.expect("filtered")));
        let Some((venue, vroad)) = venue else { return };
        let fid = self.figures.spawn(performer, road.0, road.1, Travel::Roads);
        let map = &self.map;
        if let Some(f) = self.figures.get_mut(fid) {
            f.home = school;
            f.target = venue;
            f.action = GOING_TO_VENUE;
            if !f.go_to(map, vroad) {
                f.dead = true;
            }
        }
        let s = self.buildings.get_mut(school).expect("present");
        s.walkers[0] = fid;
        let needed = self.balance.stats(s.kind).employees.max(1);
        s.spawn_delay = match s.workers * 100 / needed {
            p if p >= 100 => 3,
            p if p >= 75 => 7,
            p if p >= 50 => 15,
            p if p >= 25 => 29,
            _ => 44,
        };
    }

    /// Tick 31: venues with shows running send their performers roaming.
    pub(crate) fn venue_walkers(&mut self) {
        for id in self.buildings.ids() {
            let Some(b) = self.buildings.get(id) else { continue };
            if !self.is_road_venue(b.kind) || b.road.is_none() || b.workers <= 0 {
                continue;
            }
            if b.spawn_delay > 0 {
                self.buildings.get_mut(id).expect("present").spawn_delay -= 1;
                continue;
            }
            let Some(slot) = (0..3).find(|&s| b.shows[s] > 0 && b.performers[s] == 0) else { continue };
            let needed = self.workers_needed(b.kind).max(1);
            let delay = match b.workers * 100 / needed {
                p if p >= 100 => 0,
                p if p >= 75 => 1,
                p if p >= 50 => 3,
                p if p >= 25 => 7,
                _ => 15,
            };
            if let Some(fid) = self.spawn_roaming_figure(id, PERFORMERS[slot].0) {
                let b = self.buildings.get_mut(id).expect("present");
                b.performers[slot] = fid;
                b.spawn_delay = delay;
            }
        }
    }

    /// Daily: venues count down their remaining shows.
    pub(crate) fn update_venues(&mut self) {
        for b in self.buildings.iter_mut() {
            if matches!(b.kind, kind::BOOTH | kind::BANDSTAND | kind::PAVILION) {
                for days in &mut b.shows {
                    *days = (*days - 1).max(0);
                }
            }
        }
    }

    pub(crate) fn update_entertainer(&mut self, fid: u32) {
        let Some(f) = self.figures.get(fid) else { return };
        if f.action != GOING_TO_VENUE {
            self.update_roamer(fid);
            return;
        }
        let map = &self.map;
        let f = self.figures.get_mut(fid).expect("present");
        match f.walk(map) {
            Step::Moving => {}
            Step::Arrived => {
                let (venue, school, performer) = (f.target, f.home, f.kind);
                f.dead = true;
                if let Some(s) = self.buildings.get_mut(school) {
                    s.walkers[0] = 0;
                }
                // Shows start only if the venue has at least half its staff.
                let needed = self.buildings.get(venue).map_or(0, |v| self.workers_needed(v.kind));
                let slot = performer_slot(performer).expect("performer");
                if let Some(v) = self.buildings.get_mut(venue)
                    && v.workers * 2 > needed
                {
                    v.shows[slot] = SHOW_DAYS;
                }
            }
            _ => f.dead = true,
        }
    }
}
