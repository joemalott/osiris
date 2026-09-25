//! Tomb robbers. A house whose crime risk boils over sends out a tomb robber instead
//! of a thief once the city has burial goods worth taking: any burial provisions
//! sent, or, in the Valley of the Kings and at Alexandria, a finished royal tomb or
//! mausoleum. He loiters like a thief, then runs (at twice a walker's pace, by road
//! where he can) for the nearest tomb he can reach. At a finished royal tomb or
//! mausoleum he plunders the burial and the kingdom rating falls by 2 to 10,
//! by difficulty; at any other tomb he makes off with a load of the most valuable
//! provision sent, which must be sent again, and the rating falls by half as much.
//! Either way he is gone. A constable who catches him strikes him down; when his
//! body is gone the kingdom rating rises by 1.

use crate::buildings::BuildingId;
use crate::figures::{FigureId, Step, Travel};
use crate::monuments::{MAUSOLEUM, Style};
use crate::world::World;

pub const TOMB_ROBBER: u16 = 24;

/// Missions whose robbers go for finished tombs even with no provisions sent: the
/// third field of text group 307 is `V` (Valley of the Kings) or `A` (Alexandria).
const TOMB_CITIES: [i32; 5] = [39, 40, 44, 49, 51];
/// The resources that count as burial provisions (`FUN_004ea930`).
const PROVISIONS: [u16; 15] = [1, 8, 10, 13, 15, 17, 18, 19, 20, 23, 24, 25, 26, 28, 30];
/// What each resource is worth to a robber, who takes the dearest (the first
/// column of the exe's price table at 0x5d2c48).
const WORTH: [i32; 36] = [0, 28, 44, 38, 38, 38, 38, 44, 44, 44, 250, 40, 200, 180, 44, 215, 42, 180, 60, 200, 50, 60, 44, 215, 200, 200, 200, 200, 250, 60, 200, 110, 40, 155, 250, 72];
/// Kingdom rating lost to a plundered tomb, Very Easy to Impossible; stolen
/// provisions cost half as much.
const PLUNDER_KINGDOM: [i32; 5] = [-2, -4, -6, -8, -10];
/// A loiterer's counter passes this before he sets off.
const LOITER: i32 = 40;
/// Ticks a fallen robber lies before he is gone.
const FALL_TICKS: i32 = 128;
/// One load of provisions, in units.
const LOAD: i32 = 100;

/// City warnings (text group 19).
mod warning {
    pub const TOMB_PLUNDERED: u16 = 249;
    pub const MAUSOLEUM_PLUNDERED: u16 = 250;
    pub const PROVISIONS_STOLEN: u16 = 251;
    pub const APPREHENDED: u16 = 252;
}

mod robber_action {
    pub const LOITERING: u16 = 1;
    pub const TO_TOMB: u16 = 2;
}

impl World {
    /// Whether any burial provision has been sent.
    fn provisions_sent(&self) -> bool {
        PROVISIONS.iter().any(|&r| self.burial.get(r as usize).is_some_and(|p| p.1 > 0))
    }

    fn tomb_city(&self) -> bool {
        self.mission.as_ref().is_some_and(|m| TOMB_CITIES.contains(&m.id))
    }

    /// Whether building `id` is a tomb and finished: a mausoleum or royal tomb whose
    /// burial can be plundered.
    fn finished_tomb(&self, id: BuildingId) -> bool {
        self.buildings.get(id).is_some_and(|b| (b.kind == MAUSOLEUM || crate::royal_tombs::is_royal_tomb(b.kind)) && b.monument.as_ref().is_some_and(|m| m.finished))
    }

    /// Whether a house at the end of its tether sends out a tomb robber rather than
    /// a thief (`FUN_00469d50`).
    pub(crate) fn tomb_robbers_come(&self) -> bool {
        self.tomb_city() && self.buildings.iter().any(|b| self.finished_tomb(b.id)) || self.provisions_sent()
    }

    /// Tombs worth a robber's trip (`FUN_00492d20`): any tomb once provisions have
    /// been sent, finished royal tombs and mausoleums always.
    fn robber_targets(&self) -> Vec<BuildingId> {
        let sent = self.provisions_sent();
        self.buildings
            .iter()
            .filter(|b| is_tomb(b.kind) && (sent || self.finished_tomb(b.id)))
            .map(|b| b.id)
            .collect()
    }

    pub(crate) fn spawn_tomb_robber(&mut self, house: BuildingId) {
        let Some(b) = self.buildings.get(house) else { return };
        let Some((x, y)) = crate::buildings::road_within(&self.map, b.x, b.y, b.size, 2) else { return };
        let wait = 10 + (self.map.random.at_or(b.x, b.y, 0) as i32 & 15);
        let fid = self.figures.spawn(TOMB_ROBBER, x, y, Travel::PreferRoads);
        if let Some(f) = self.figures.get_mut(fid) {
            f.home = house;
            f.action = robber_action::LOITERING;
            f.counter = wait;
            f.speed = 2;
        }
    }

    /// A constable sets on figure `fid`: if it is a tomb robber, he stands and
    /// fights (to no effect: he has no attack).
    pub(crate) fn corner_tomb_robber(&mut self, fid: FigureId, constable: FigureId) {
        use crate::military::action::{ATTACK, CORPSE};
        let at = self.figures.get(constable).map(|c| (c.x, c.y));
        let Some(f) = self.figures.get_mut(fid).filter(|f| f.kind == TOMB_ROBBER && !matches!(f.action, ATTACK | CORPSE)) else { return };
        f.resume = f.action;
        f.foe = constable;
        f.action = ATTACK;
        f.attack_tick = 0;
        f.route.clear();
        f.moving = false;
        if let Some(at) = at {
            f.direction = crate::figures::direction_to((f.x, f.y), at).unwrap_or(f.direction);
        }
    }

    /// A tomb robber loiters, then runs for the nearest tomb he can reach and robs
    /// it.
    pub(crate) fn update_tomb_robber(&mut self, fid: FigureId) {
        use crate::military::action::{ATTACK, CORPSE};
        let Some(f) = self.figures.get(fid) else { return };
        let (act, pos) = (f.action, (f.x, f.y));
        match act {
            CORPSE => {
                let f = self.figures.get_mut(fid).expect("present");
                f.counter += 1;
                if f.counter >= FALL_TICKS {
                    f.dead = true;
                    self.ratings.change_kingdom(1);
                    self.warnings.push_back(warning::APPREHENDED);
                }
            }
            ATTACK => {
                self.fight(fid);
                // Free again: off to a tomb, the nearest from here.
                let f = self.figures.get_mut(fid).expect("present");
                if f.action == 0 {
                    f.action = robber_action::LOITERING;
                    f.counter = LOITER;
                }
            }
            robber_action::LOITERING => {
                let f = self.figures.get_mut(fid).expect("present");
                f.counter += 1;
                if f.counter <= LOITER {
                    return;
                }
                let mut tombs: Vec<(BuildingId, (i32, i32))> = self
                    .robber_targets()
                    .into_iter()
                    .filter_map(|id| self.monument_access(id, pos).map(|spot| (id, spot)))
                    .collect();
                tombs.sort_by_key(|&(id, (x, y))| ((x - pos.0).abs().max((y - pos.1).abs()), id));
                let map = &self.map;
                let f = self.figures.get_mut(fid).expect("present");
                f.counter = 0;
                match tombs.into_iter().find(|&(_, spot)| f.go_to(map, spot)) {
                    Some((id, _)) => {
                        f.target = id;
                        f.action = robber_action::TO_TOMB;
                    }
                    None => f.dead = true,
                }
            }
            _ => {
                let map = &self.map;
                let f = self.figures.get_mut(fid).expect("present");
                match f.walk(map) {
                    Step::Moving => {}
                    Step::Arrived => {
                        let target = f.target;
                        f.dead = true;
                        self.rob_tomb(target);
                    }
                    _ => f.dead = true,
                }
            }
        }
    }

    /// A robber reaches tomb `id`: a finished royal tomb or mausoleum is plundered,
    /// any other gives up a load of provisions.
    fn rob_tomb(&mut self, id: BuildingId) {
        let Some(k) = self.buildings.get(id).map(|b| b.kind) else { return };
        let full = self.by_difficulty(PLUNDER_KINGDOM);
        let loss = if self.finished_tomb(id) {
            self.warnings.push_back(if k == MAUSOLEUM { warning::MAUSOLEUM_PLUNDERED } else { warning::TOMB_PLUNDERED });
            full
        } else if is_tomb(k) {
            self.warnings.push_back(warning::PROVISIONS_STOLEN);
            self.steal_provisions();
            full / 2
        } else {
            0
        };
        self.ratings.change_kingdom(loss);
    }

    /// Takes a load of the most valuable provision sent (`FUN_00492fc0`); the royal
    /// tombs lose their furnishings until it is made good.
    fn steal_provisions(&mut self) {
        let pick = PROVISIONS
            .iter()
            .filter(|&&r| self.burial.get(r as usize).is_some_and(|p| p.1 > 0))
            .fold(None::<u16>, |best, &r| if best.is_none_or(|b| WORTH[r as usize] > WORTH[b as usize]) { Some(r) } else { best });
        let Some(r) = pick else { return };
        let sent = &mut self.burial[r as usize].1;
        *sent = (*sent - LOAD).max(0);
        if !self.burial_complete() {
            let tombs: Vec<BuildingId> = self.buildings.iter().filter(|b| crate::royal_tombs::is_royal_tomb(b.kind)).map(|b| b.id).collect();
            for t in tombs {
                self.refresh_royal_tomb(t);
            }
        }
    }

    /// Sends a tomb robber out from the tile `at` at once (for testing).
    pub fn tomb_robber_now(&mut self, at: (i32, i32)) -> FigureId {
        let fid = self.figures.spawn(TOMB_ROBBER, at.0, at.1, Travel::PreferRoads);
        if let Some(f) = self.figures.get_mut(fid) {
            f.action = robber_action::LOITERING;
            f.counter = LOITER;
            f.speed = 2;
        }
        fid
    }
}

/// Whether a building of kind `k` is a tomb robbers go for: a pyramid, mastaba,
/// mausoleum or royal tomb.
fn is_tomb(k: u16) -> bool {
    crate::monuments::monument_def(k).is_some_and(|d| matches!(d.style, Style::Mastaba | Style::Pyramid(_) | Style::Mausoleum | Style::RoyalTomb))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::world::{Command, Outcome};

    /// Mission 12's land with a small pyramid staked out at 58,40 and a road along
    /// row 52; no fires, collapses or invasions.
    pub(crate) fn town() -> Option<World> {
        let data = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../PharaohData");
        if !data.join("mission1.pak").is_file() {
            return None;
        }
        let library = osiris_formats::ImageLibrary::open(&data.join("Data")).expect("open image library");
        let scenario = osiris_formats::MissionPak::open(&data.join("mission1.pak")).expect("pak").scenario(12).expect("mission 12");
        let defs = std::sync::Arc::new(crate::defs::Defs::load(&library).expect("load defs"));
        let model = osiris_formats::Model::parse(&String::from_utf8_lossy(&std::fs::read(data.join("Pharaoh_Model_Normal.txt")).expect("read model"))).expect("parse model");
        let mut balance = crate::balance::Balance::from_model(&model);
        balance.set_units(&osiris_formats::model::parse_figures(&String::from_utf8_lossy(&std::fs::read(data.join("Figure_model_normal.txt")).expect("read figures"))).expect("parse figures"));
        let mut world = World::new(&scenario, defs, std::sync::Arc::new(balance));
        world.start(&scenario);
        world.load_mission(12);
        world.invasions.planned.clear();
        world.rules.fire = false;
        world.rules.collapse = false;
        world.treasury = 100_000;
        world.scenario_monuments = [13, 0, 0];
        world.scenario_allowed = None;
        if let Some(m) = world.mission.as_mut() {
            m.allowed.extend([crate::monuments::SMALL_PYRAMID, 55]);
        }
        for cmd in [Command::Build { kind: crate::monuments::SMALL_PYRAMID, x: 58, y: 40, x1: 58, y1: 40 }, Command::Road { start: (36, 52), end: (74, 52) }] {
            assert!(matches!(world.apply(&cmd), Outcome::Done { .. }), "{cmd:?}");
        }
        Some(world)
    }

    fn run_until_gone(w: &mut World, fid: FigureId) {
        for _ in 0..2000 {
            if w.figures.get(fid).is_none_or(|f| f.dead || f.kind != TOMB_ROBBER) {
                return;
            }
            w.tick();
        }
        panic!("the robber is still about");
    }

    #[test]
    fn no_provisions_no_robbers_outside_the_tomb_cities() {
        let Some(mut w) = town() else { return };
        assert!(!w.tomb_robbers_come());
        w.burial[1] = (500, 100);
        assert!(w.tomb_robbers_come());
    }

    #[test]
    fn a_robber_takes_a_load_of_the_dearest_provision() {
        let Some(mut w) = town() else { return };
        w.burial[1] = (500, 300);
        w.burial[13] = (500, 200);
        let kingdom = w.ratings.kingdom;
        let fid = w.tomb_robber_now((40, 52));
        run_until_gone(&mut w, fid);
        // Pottery is worth more than grain; on Normal the rating falls by half of 6.
        assert_eq!((w.burial[1].1, w.burial[13].1), (300, 100));
        assert_eq!(w.ratings.kingdom, kingdom - 3);
        assert!(w.warnings.contains(&warning::PROVISIONS_STOLEN));
    }

    #[test]
    fn a_constable_strikes_a_robber_down() {
        let Some(mut w) = town() else { return };
        w.burial[1] = (500, 300);
        assert!(matches!(w.apply(&Command::Build { kind: 55, x: 48, y: 53, x1: 48, y1: 53 }), Outcome::Done { .. }));
        let station = w.buildings.iter().find(|b| b.kind == 55).map(|b| b.id).expect("station");
        let kingdom = w.ratings.kingdom;
        let fid = w.tomb_robber_now((44, 52));
        let c = w.figures.spawn(crate::crime::CONSTABLE, 45, 52, Travel::Roads);
        if let Some(f) = w.figures.get_mut(c) {
            f.home = station;
            f.action = crate::services::action::ROAMING;
            f.roam_left = 1000;
        }
        run_until_gone(&mut w, fid);
        assert_eq!(w.burial[1].1, 300);
        assert_eq!(w.ratings.kingdom, kingdom + 1);
        assert!(w.warnings.contains(&warning::APPREHENDED));
    }
}
