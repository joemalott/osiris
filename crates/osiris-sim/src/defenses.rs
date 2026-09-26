//! The city's defences, as the original offers them: one wall, one tower and one
//! gatehouse (types 169, 173 and 202).
//!
//! Walls are dragged out in a straight line one or two tiles thick; each tile's image
//! depends on which neighbours are walls, and on towers and gatehouses beside it. A
//! tower is a 2x2 block built on clear land or over walls (the original warns when it
//! touches no wall); the recruiter sends it a sentry, who throws javelins at invaders
//! in range. A gatehouse is two 2x2 towers astride a two-tile stretch of road, five
//! tiles long, turned with R: the city's people pass along the road, invaders must
//! break it down. Invaders who cannot reach their target batter the defences in their
//! way: a wall takes 200 blows, a tower or gatehouse 400.
//!
//! A roadblock also stands on a road: walkers bound somewhere pass it, but roaming
//! walkers turn back.

use crate::buildings::BuildingId;
use crate::figures::{FigureId, Step, Travel};
use crate::map::{NEIGHBOURS, terrain};
use crate::world::World;

pub const WALL: u16 = 169;
pub const TOWER: u16 = 173;
pub const GATEHOUSE: u16 = 202;
/// A mud wall and tower that are not the original's, which earlier Osiris builds
/// offered in their place; a saved game's are turned into the original's on load.
pub const OLD_WALL: u16 = 6;
pub const OLD_TOWER: u16 = 59;
/// A one-tile gatehouse that is not the original's. A saved game's are kept as they
/// stand (they cannot become the five-tile gatehouse), but no longer built.
pub const OLD_GATEHOUSE: u16 = 58;
pub const TOWER_SENTRY: u16 = 42;
pub const ROADBLOCK: u16 = 138;

/// The patron god's gatehouse towers, by god (Osiris, Ra, Ptah, Seth, Bast).
const GATEHOUSE_TOWERS: [&str; 5] = ["tower_osiris", "tower_ra", "tower_ptah", "tower_seth", "tower_bast"];

pub fn is_wall(k: u16) -> bool {
    k == WALL
}

pub fn is_gatehouse(k: u16) -> bool {
    matches!(k, GATEHOUSE | OLD_GATEHOUSE)
}

pub fn is_tower(k: u16) -> bool {
    k == TOWER
}

/// Any part of the defences invaders must break through.
pub fn is_defense(k: u16) -> bool {
    is_wall(k) || is_gatehouse(k) || is_tower(k)
}

/// Blows an invader must land to bring a piece of the defences down: 200 for a wall,
/// 400 for a tower or gatehouse (the original's per-tile damage limits).
pub fn hit_points(k: u16) -> Option<i32> {
    if is_wall(k) {
        Some(200)
    } else if is_tower(k) || is_gatehouse(k) {
        Some(400)
    } else {
        None
    }
}

/// A gatehouse's footprint for a facing: 0 runs north-south with the road crossing
/// it east-west, 1 runs east-west with the road north-south.
pub fn gatehouse_footprint(facing: u8) -> (i32, i32) {
    if facing == 0 { (2, 5) } else { (5, 2) }
}

/// The gatehouse's parts relative to its corner, in the original's order: first
/// tower, the road's two tiles, second tower. `(dx, dy, size, road)`.
pub fn gatehouse_parts(facing: u8) -> [(i32, i32, i32, bool); 4] {
    let p = [(0, 0, 2, false), (2, 0, 1, true), (2, 1, 1, true), (3, 0, 2, false)];
    p.map(|(along, across, size, road)| if facing == 0 { (across, along, size, road) } else { (along, across, size, road) })
}

impl World {
    /// The tiles a dragged wall covers: the rectangle from `(x, y)` to `(x1, y1)`,
    /// no more than two tiles across the drag's longer direction.
    pub fn wall_sites(&self, x: i32, y: i32, x1: i32, y1: i32) -> Vec<(i32, i32)> {
        let (mut x1, mut y1) = (x1, y1);
        if (y1 - y).abs() < (x1 - x).abs() {
            y1 = y1.clamp(y - 1, y + 1);
        } else {
            x1 = x1.clamp(x - 1, x + 1);
        }
        let mut v = Vec::new();
        for yy in y.min(y1)..=y.max(y1) {
            for xx in x.min(x1)..=x.max(x1) {
                v.push((xx, yy));
            }
        }
        v
    }

    /// What is wrong with tile `(xx, yy)` of a gatehouse: its towers need clear land,
    /// its road tiles road or clear land.
    pub(crate) fn gatehouse_tile_problem(&self, xx: i32, yy: i32, road: bool) -> Option<&'static str> {
        if road && self.map.contains(xx, yy) && self.map.terrain_is(xx, yy, terrain::ROAD) {
            let t = self.map.terrain.at_or(xx, yy, 0);
            let blocked = t & (terrain::WATER | terrain::GATEHOUSE | terrain::BUILDING) != 0 || self.map.building.at_or(xx, yy, 0) != 0;
            return blocked.then_some("Gatehouses go across a road");
        }
        self.footprint_tile_problem(GATEHOUSE, xx, yy)
    }

    /// Every tile of a gatehouse at `(x, y)` with what is wrong with it, if anything.
    pub(crate) fn gatehouse_tiles(&self, x: i32, y: i32) -> Vec<((i32, i32), Option<&'static str>)> {
        let mut v = Vec::new();
        for (dx, dy, size, road) in gatehouse_parts(self.gatehouse_facing) {
            for yy in y + dy..y + dy + size {
                for xx in x + dx..x + dx + size {
                    v.push(((xx, yy), self.gatehouse_tile_problem(xx, yy, road)));
                }
            }
        }
        v
    }

    /// What is wrong with tile `(xx, yy)` of a tower: it goes on clear land or walls.
    pub(crate) fn tower_tile_problem(&self, xx: i32, yy: i32) -> Option<&'static str> {
        let on_wall = self.buildings.get(self.map.building.at_or(xx, yy, 0)).is_some_and(|b| is_wall(b.kind));
        if on_wall && !self.figures.iter().any(|f| (f.x, f.y) == (xx, yy)) {
            return None;
        }
        self.footprint_tile_problem(TOWER, xx, yy)
    }

    /// A tower going up takes the place of the walls under it.
    pub(crate) fn clear_walls_for_tower(&mut self, x: i32, y: i32) {
        for yy in y..y + 2 {
            for xx in x..x + 2 {
                let id = self.map.building.at_or(xx, yy, 0);
                if self.buildings.get(id).is_some_and(|b| is_wall(b.kind)) {
                    self.demolish(id);
                }
            }
        }
    }

    /// Placement rules of the defences: gatehouses and roadblocks go on roads, towers
    /// on clear land or walls.
    pub(crate) fn can_place_defense(&self, k: u16, x: i32, y: i32) -> Option<Result<(), &'static str>> {
        if k == GATEHOUSE {
            return Some(self.gatehouse_tiles(x, y).into_iter().find_map(|(_, why)| why).map_or(Ok(()), Err));
        }
        if is_gatehouse(k) || k == ROADBLOCK {
            let t = self.map.terrain.at_or(x, y, 0);
            let why = if k == ROADBLOCK { "Roadblocks go on a road" } else { "Gatehouses go on a road" };
            return Some(if t & terrain::ROAD == 0 || self.map.building.at_or(x, y, 0) != 0 { Err(why) } else { Ok(()) });
        }
        if is_tower(k) {
            let why = (y..y + 2).flat_map(|yy| (x..x + 2).map(move |xx| (xx, yy))).find_map(|(xx, yy)| self.tower_tile_problem(xx, yy));
            return Some(why.map_or(Ok(()), Err));
        }
        None
    }

    /// Sets up a new piece of the defences: walls, towers and gatehouses mark their
    /// terrain and redraw the walls around them.
    pub(crate) fn place_defense(&mut self, id: BuildingId) {
        let Some(b) = self.buildings.get(id) else { return };
        let (k, x, y) = (b.kind, b.x, b.y);
        if is_wall(k) {
            self.map.terrain.update(x, y, |t| t | terrain::WALL);
        } else if k == ROADBLOCK {
            // The road stays a road for walkers going somewhere.
            self.map.terrain.update(x, y, |t| (t & !terrain::BUILDING) | terrain::ROAD);
            return;
        } else if is_tower(k) {
            for yy in y..y + 2 {
                for xx in x..x + 2 {
                    self.map.terrain.update(xx, yy, |t| t | terrain::GATEHOUSE);
                }
            }
            // The original warns of a tower that touches no wall (19:39).
            let ring = (y - 1..=y + 2).flat_map(|yy| (x - 1..=x + 2).map(move |xx| (xx, yy)));
            if !ring.into_iter().any(|(xx, yy)| self.map.terrain_is(xx, yy, terrain::WALL)) {
                self.warnings.push_back(39);
            }
        } else if k == GATEHOUSE {
            self.place_gatehouse(id);
        } else if is_gatehouse(k) {
            // The one-tile gatehouse that is not the original's, kept for old saves.
            self.map.terrain.update(x, y, |t| (t & !terrain::BUILDING) | terrain::GATEHOUSE | terrain::ROAD);
            let ns = self.map.terrain_is(x, y - 1, terrain::ROAD) || self.map.terrain_is(x, y + 1, terrain::ROAD);
            if let Some(a) = self.defs.building(k).and_then(|d| d.anims.get(if ns { "base_n" } else { "base_w" })) {
                let image = a.image;
                self.map.set_single_image(x, y, image);
            }
        }
        let (w, h) = self.buildings.get(id).map_or((1, 1), |b| b.footprint());
        self.redraw_walls_around(x, y, w.max(h) + 2);
    }

    /// Lays out a gatehouse: its towers in the look of the city's patron god (else the
    /// first god it knows, else Osiris's), its road tiles open to walkers.
    fn place_gatehouse(&mut self, id: BuildingId) {
        let Some(b) = self.buildings.get(id) else { return };
        let (x, y) = (b.x, b.y);
        let facing = b.orientation;
        let gods = &self.religion.gods;
        let god = gods.iter().position(|g| g.status == 2).or_else(|| gods.iter().position(|g| g.status == 1)).unwrap_or(0);
        let Some(def) = self.defs.building(GATEHOUSE) else { return };
        let look = |key: &str| def.anims.get(key).map_or(def.image, |a| a.image);
        // The images face the way the road runs.
        let turn = u32::from(facing == 0);
        let images = [look(GATEHOUSE_TOWERS[god]), look("road_a"), look("road_b"), look(GATEHOUSE_TOWERS[god])];
        for ((dx, dy, size, road), image) in gatehouse_parts(facing).into_iter().zip(images) {
            let (px, py) = (x + dx, y + dy);
            for yy in py..py + size {
                for xx in px..px + size {
                    self.map.terrain.update(xx, yy, |t| {
                        let t = t | terrain::GATEHOUSE;
                        if road { (t & !terrain::BUILDING) | terrain::ROAD } else { t }
                    });
                }
            }
            self.map.set_footprint(px, py, size, image + turn);
        }
    }

    /// Chooses each wall tile's image from its neighbours in the square around
    /// `(x, y)`: first by the walls around it; then, beside a tower or gatehouse, by
    /// the table for walls that meet one, where it has a row.
    pub(crate) fn redraw_walls_around(&mut self, x: i32, y: i32, r: i32) {
        let Some(base) = self.defs.building(WALL).map(|d| d.image) else { return };
        for yy in y - r..=y + r {
            for xx in x - r..=x + r {
                if !self.map.terrain_is(xx, yy, terrain::WALL) {
                    continue;
                }
                // The wall table marks the neighbours that are not wall: 1 is open, 0 wall.
                let tiles = NEIGHBOURS.map(|(dx, dy)| u8::from(!self.map.terrain_is(xx + dx, yy + dy, terrain::WALL)));
                let mut counters = Vec::new();
                let mut image = crate::tiles::match_context(&self.defs.contexts.wall, &mut counters, tiles);
                let beside = (0..8).step_by(2).any(|i| {
                    let (dx, dy) = NEIGHBOURS[i];
                    self.map.terrain_is(xx + dx, yy + dy, terrain::GATEHOUSE)
                });
                if beside {
                    let tiles = std::array::from_fn(|i| {
                        let (dx, dy) = NEIGHBOURS[i];
                        let mask = if i % 2 == 0 { terrain::WALL | terrain::GATEHOUSE } else { terrain::WALL };
                        u8::from(self.map.terrain_is(xx + dx, yy + dy, mask))
                    });
                    let mut counters = Vec::new();
                    if let Some(c) = crate::tiles::match_context(&self.defs.contexts.wall_gatehouse, &mut counters, tiles) {
                        image = Some(c);
                    }
                }
                if let Some(c) = image {
                    self.map.set_single_image(xx, yy, base + c.group_offset + c.item_offset);
                }
            }
        }
    }

    /// Whether a roaming walker may step onto `(x, y)`: not past a roadblock.
    pub fn roamer_may_enter(&self, x: i32, y: i32) -> bool {
        let id = self.map.building.at_or(x, y, 0);
        id == 0 || !self.buildings.get(id).is_some_and(|b| b.kind == ROADBLOCK)
    }

    /// When a piece of the defences goes, its terrain goes with it; a gatehouse's road
    /// stays a road.
    pub(crate) fn remove_defense(&mut self, k: u16, x: i32, y: i32, (w, h): (i32, i32)) {
        if is_wall(k) {
            self.map.terrain.update(x, y, |t| t & !terrain::WALL);
        } else if is_gatehouse(k) || is_tower(k) || k == ROADBLOCK {
            for yy in y..y + h {
                for xx in x..x + w {
                    self.map.terrain.update(xx, yy, |t| t & !terrain::GATEHOUSE);
                }
            }
            let (mut rules, map) = self.tile_rules();
            rules.roads_in(map, x - 1, y - 1, x + w, y + h);
        }
        self.redraw_walls_around(x, y, w.max(h) + 1);
    }

    /// Turns an old save's non-original walls and towers into the original's, and its
    /// lists of what may be built with them.
    pub(crate) fn upgrade_defenses(&mut self) {
        for b in self.buildings.iter_mut() {
            b.kind = match b.kind {
                OLD_WALL => WALL,
                OLD_TOWER => TOWER,
                k => k,
            };
        }
        let swap = |set: &mut std::collections::BTreeSet<u16>| {
            for (old, new) in [(OLD_WALL, WALL), (OLD_TOWER, TOWER), (OLD_GATEHOUSE, GATEHOUSE)] {
                if set.remove(&old) {
                    set.insert(new);
                }
            }
        };
        if let Some(m) = &mut self.mission {
            swap(&mut m.allowed);
            for u in &mut m.unlocks {
                for k in &mut u.enable {
                    *k = match *k {
                        OLD_WALL => WALL,
                        OLD_TOWER => TOWER,
                        OLD_GATEHOUSE => GATEHOUSE,
                        k => k,
                    };
                }
            }
        }
        if let Some(set) = &mut self.scenario_allowed {
            swap(set);
        }
    }

    /// The first staffed tower without a sentry, if any.
    pub(crate) fn tower_wanting_sentry(&self) -> Option<BuildingId> {
        self.buildings.iter().find(|b| is_tower(b.kind) && b.workers > 0 && b.walkers[0] == 0).map(|b| b.id)
    }

    /// When its turn to enlist comes, a recruiter sends a sentry to a staffed tower
    /// that lacks one before it raises a soldier (as the original does). True if it
    /// sent one.
    pub(crate) fn man_a_tower(&mut self, recruiter: BuildingId) -> bool {
        let Some(from) = self.buildings.get(recruiter).and_then(|b| b.road) else { return false };
        let Some(tower) = self.tower_wanting_sentry() else { return false };
        let Some((tx, ty)) = self.buildings.get(tower).map(|b| (b.x, b.y)) else { return false };
        let fid = self.figures.spawn(TOWER_SENTRY, from.0, from.1, Travel::Land);
        let map = &self.map;
        let Some(f) = self.figures.get_mut(fid) else { return true };
        f.home = tower;
        f.action = 1;
        // Up onto the tower: the sentry walks to its foot, then stands on it.
        let foot = [(tx - 1, ty), (tx, ty - 1), (tx + 2, ty), (tx, ty + 2), (tx - 1, ty + 1), (tx + 1, ty - 1)]
            .into_iter()
            .find(|&(x, y)| crate::figures::passable(map, Travel::Land, x, y));
        if !foot.is_some_and(|p| f.go_to(map, p)) {
            f.dead = true;
            return true;
        }
        self.buildings.get_mut(tower).expect("present").walkers[0] = fid;
        true
    }

    /// A sentry climbs his tower and throws javelins at invaders in range.
    pub(crate) fn update_sentry(&mut self, fid: FigureId) {
        let Some(f) = self.figures.get(fid) else { return };
        if f.action == crate::military::action::CORPSE {
            let f = self.figures.get_mut(fid).expect("present");
            f.counter += 1;
            if f.counter > 200 {
                f.dead = true;
            }
            return;
        }
        let tower = f.home;
        let Some(b) = self.buildings.get(tower).filter(|b| is_tower(b.kind)) else {
            self.figures.get_mut(fid).expect("present").dead = true;
            return;
        };
        let (tx, ty) = (b.x, b.y);
        if f.action == 1 {
            let map = &self.map;
            let f = self.figures.get_mut(fid).expect("present");
            if f.walk(map) != Step::Moving {
                f.x = tx;
                f.y = ty;
                f.action = crate::military::action::AT_STANDARD;
                f.moving = false;
            }
            return;
        }
        self.shoot_at_foes(fid);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::{Command, Outcome};

    fn sandbox() -> Option<World> {
        let data = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../PharaohData");
        if !data.is_dir() {
            return None;
        }
        let library = osiris_formats::ImageLibrary::open(&data.join("Data")).expect("open image library");
        let scenario = osiris_formats::Scenario::load_map(&data.join("Maps/Sandbox.map")).expect("load map");
        let defs = std::sync::Arc::new(crate::defs::Defs::load(&library).expect("load defs"));
        let model_text = std::fs::read(data.join("Pharaoh_Model_Normal.txt")).expect("read model");
        let model = osiris_formats::Model::parse(&String::from_utf8_lossy(&model_text)).expect("parse model");
        let balance = std::sync::Arc::new(crate::balance::Balance::from_model(&model));
        let mut world = World::new(&scenario, defs, balance);
        world.start(&scenario);
        world.treasury = 100_000;
        if let Some(a) = &mut world.scenario_allowed {
            a.extend([WALL, TOWER, GATEHOUSE]);
        }
        Some(world)
    }

    fn build(world: &mut World, k: u16, (x, y): (i32, i32), (x1, y1): (i32, i32)) -> Outcome {
        world.apply(&Command::Build { kind: k, x, y, x1, y1 })
    }

    #[test]
    fn a_wall_is_dragged_straight_and_at_most_two_thick() {
        let Some(world) = sandbox() else { return };
        let sites = world.wall_sites(10, 10, 20, 14);
        assert_eq!(sites.len(), 22, "11 long, two thick");
        assert!(sites.iter().all(|&(x, y)| (10..=20).contains(&x) && (10..=11).contains(&y)));
        assert_eq!(world.wall_sites(10, 10, 9, 16).len(), 14, "down the column, two thick");
        assert_eq!(world.wall_sites(10, 10, 10, 10), vec![(10, 10)]);
    }

    #[test]
    fn a_wall_faces_run_along_it() {
        let Some(mut world) = sandbox() else { return };
        let base = world.defs.building(WALL).map(|d| d.image).expect("wall image");
        // Along x: open north and south, walls east and west, the image whose face
        // runs along the x axis; along y the other one; alone, the post.
        assert!(matches!(build(&mut world, WALL, (160, 134), (164, 134)), Outcome::Done { items: 5, .. }));
        assert!(matches!(build(&mut world, WALL, (168, 134), (168, 138)), Outcome::Done { items: 5, .. }));
        assert!(matches!(build(&mut world, WALL, (172, 136), (172, 136)), Outcome::Done { items: 1, .. }));
        assert_eq!(world.map.images.at_or(162, 134, 0), base + 1);
        assert_eq!(world.map.images.at_or(168, 136, 0), base + 4);
        assert_eq!(world.map.images.at_or(172, 136, 0), base + 26);
    }

    #[test]
    fn a_gatehouse_stands_across_a_road() {
        let Some(mut world) = sandbox() else { return };
        // The sandbox has a road along row 131.
        let (x, y) = (158, 129);
        world.gatehouse_facing = 0;
        assert_eq!(world.footprint_of(GATEHOUSE), (2, 5));
        assert!(matches!(build(&mut world, GATEHOUSE, (x, y), (x, y)), Outcome::Done { .. }), "{:?}", world.can_place(GATEHOUSE, x, y));
        let id = world.map.building.at_or(x, y, 0);
        assert_eq!(world.buildings.get(id).map(|b| (b.kind, b.footprint())), Some((GATEHOUSE, (2, 5))));
        for yy in y..y + 5 {
            for xx in x..x + 2 {
                assert_eq!(world.map.building.at_or(xx, yy, 0), id);
                assert!(world.map.terrain_is(xx, yy, terrain::GATEHOUSE));
                let road = yy == y + 2;
                assert_eq!(world.map.terrain_is(xx, yy, terrain::ROAD), road, "({xx},{yy})");
                // People walk the road through it; invaders must break it down.
                assert_eq!(crate::figures::passable(&world.map, Travel::Roads, xx, yy), road);
                assert!(!crate::figures::passable(&world.map, Travel::Hostile, xx, yy));
            }
        }
        // Its towers need clear land: not over the road.
        world.gatehouse_facing = 1;
        assert!(world.can_place(GATEHOUSE, 150, y + 1).is_err());
        world.demolish(id);
        for xx in x..x + 2 {
            assert!(world.map.terrain_is(xx, y + 2, terrain::ROAD) && !world.map.terrain_is(xx, y + 2, terrain::GATEHOUSE));
        }
        assert!(!world.map.terrain_is(x, y, terrain::GATEHOUSE | terrain::BUILDING));
    }

    #[test]
    fn a_tower_goes_on_walls_or_clear_land() {
        let Some(mut world) = sandbox() else { return };
        assert!(matches!(build(&mut world, WALL, (160, 134), (170, 135)), Outcome::Done { items: 22, .. }));
        assert!(matches!(build(&mut world, TOWER, (163, 134), (163, 134)), Outcome::Done { .. }));
        let id = world.map.building.at_or(163, 134, 0);
        assert_eq!(world.buildings.get(id).map(|b| b.kind), Some(TOWER));
        assert!(!world.map.terrain_is(164, 135, terrain::WALL));
        assert!(world.map.terrain_is(164, 135, terrain::GATEHOUSE));
        assert!(!world.warnings.contains(&39));
        // Away from any wall it may still be built, with the original's warning.
        assert!(matches!(build(&mut world, TOWER, (163, 140), (163, 140)), Outcome::Done { .. }));
        assert!(world.warnings.contains(&39));
        assert_eq!((hit_points(WALL), hit_points(TOWER), hit_points(GATEHOUSE), hit_points(70)), (Some(200), Some(400), Some(400), None));
    }

    #[test]
    fn old_walls_and_towers_become_the_originals() {
        let Some(mut world) = sandbox() else { return };
        let wall = world.create_building(OLD_WALL, 160, 120);
        let tower = world.create_building(OLD_TOWER, 162, 120);
        if let Some(a) = &mut world.scenario_allowed {
            a.retain(|&k| !is_defense(k));
            a.extend([OLD_WALL, OLD_TOWER, OLD_GATEHOUSE]);
        }
        world.upgrade_defenses();
        assert_eq!(world.buildings.get(wall).map(|b| b.kind), Some(WALL));
        assert_eq!(world.buildings.get(tower).map(|b| b.kind), Some(TOWER));
        assert!(world.is_allowed(WALL) && world.is_allowed(TOWER) && world.is_allowed(GATEHOUSE));
    }
}
