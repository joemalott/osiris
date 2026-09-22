//! Entertainment. Performer schools train entertainers who walk to a venue that needs
//! one; the venue then puts on shows for a while and the performer roams the streets
//! from it, entertaining nearby houses.

use crate::buildings::{BuildingId, kind};
use crate::figures::{Step, Travel};
use crate::world::World;

pub const JUGGLER: u16 = 15;
/// Days of shows a performer's arrival buys a venue.
const SHOW_DAYS: i32 = 32;
const GOING_TO_VENUE: u16 = 10;

impl World {
    /// Tick 31: schools send performers to venues whose shows are running out.
    pub(crate) fn school_walkers(&mut self) {
        let schools: Vec<BuildingId> =
            self.buildings.iter().filter(|b| b.kind == kind::JUGGLER_SCHOOL).map(|b| b.id).collect();
        for school in schools {
            let Some(s) = self.buildings.get(school) else { continue };
            let Some(road) = s.road else { continue };
            if s.workers <= 0 || s.walkers[0] != 0 {
                continue;
            }
            if s.spawn_delay > 0 {
                self.buildings.get_mut(school).expect("present").spawn_delay -= 1;
                continue;
            }
            let (sx, sy) = (s.x, s.y);
            let en_route: Vec<u32> = self
                .figures
                .iter()
                .filter(|f| f.kind == JUGGLER && f.action == GOING_TO_VENUE)
                .map(|f| f.target)
                .collect();
            let venue = self
                .buildings
                .iter()
                .filter(|b| b.kind == kind::BOOTH && b.workers > 0 && b.road.is_some())
                .filter(|b| b.progress <= 0 && !en_route.contains(&b.id))
                .min_by_key(|b| (b.x - sx).abs() + (b.y - sy).abs())
                .map(|b| (b.id, b.road.unwrap()));
            let Some((venue, vroad)) = venue else { continue };
            let fid = self.figures.spawn(JUGGLER, road.0, road.1, Travel::Roads);
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
            let needed = self.balance.stats(kind::JUGGLER_SCHOOL).employees.max(1);
            s.spawn_delay = match s.workers * 100 / needed {
                p if p >= 100 => 3,
                p if p >= 75 => 7,
                p if p >= 50 => 15,
                p if p >= 25 => 29,
                _ => 44,
            };
        }
    }

    /// Daily: venues count down their remaining shows.
    pub(crate) fn update_venues(&mut self) {
        for b in self.buildings.iter_mut() {
            if b.kind == kind::BOOTH && b.progress > 0 {
                b.progress -= 1;
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
                let (venue, school) = (f.target, f.home);
                // The performer now belongs to the venue and roams from it.
                if let Some(s) = self.buildings.get_mut(school) {
                    s.walkers[0] = 0;
                }
                if let Some(v) = self.buildings.get_mut(venue) {
                    v.progress = SHOW_DAYS;
                    v.walkers[0] = fid;
                }
                let roam = self.defs.figure(JUGGLER).and_then(|d| d.int("max_roam_length")).unwrap_or(640) as i32;
                let f = self.figures.get_mut(fid).expect("present");
                f.home = venue;
                f.action = 1;
                f.roam_left = roam;
                f.route.clear();
                f.destination = None;
            }
            _ => f.dead = true,
        }
    }
}
