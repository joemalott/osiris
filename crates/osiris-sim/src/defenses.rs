//! The city's defences. Walls are laid like roads, a tile at a time; each tile's
//! image depends on which neighbours are walls too. A gatehouse stands on a road
//! through a wall: the city's people pass, invaders must break it down. Invaders who
//! cannot reach their target batter the wall in their way until it falls, so a
//! double wall holds them twice as long. A tower is built into a wall two tiles thick;
//! the recruiter sends it a sentry, who throws javelins at invaders in range.

use crate::buildings::BuildingId;
use crate::figures::{FigureId, Step, Travel};
use crate::map::{NEIGHBOURS, terrain};
use crate::world::World;

pub const MUD_WALL: u16 = 6;
pub const BRICK_WALL: u16 = 169;
pub const MUD_GATEHOUSE: u16 = 58;
pub const CLAY_GATEHOUSE: u16 = 170;
pub const BRICK_GATEHOUSE: u16 = 171;
pub const MUD_TOWER: u16 = 59;
pub const BRICK_TOWER: u16 = 172;
pub const CLAY_TOWER: u16 = 173;
pub const TOWER_SENTRY: u16 = 42;

pub fn is_wall(k: u16) -> bool {
    matches!(k, MUD_WALL | BRICK_WALL)
}

pub fn is_gatehouse(k: u16) -> bool {
    matches!(k, MUD_GATEHOUSE | CLAY_GATEHOUSE | BRICK_GATEHOUSE)
}

pub fn is_tower(k: u16) -> bool {
    matches!(k, MUD_TOWER | BRICK_TOWER | CLAY_TOWER)
}

/// Any part of the defences invaders must break through.
pub fn is_defense(k: u16) -> bool {
    is_wall(k) || is_gatehouse(k) || is_tower(k)
}

impl World {
    /// The tiles a dragged wall covers: along the drag's row, then down its column.
    pub(crate) fn wall_sites(&self, x: i32, y: i32, x1: i32, y1: i32) -> Vec<(i32, i32)> {
        let mut v: Vec<(i32, i32)> = (x.min(x1)..=x.max(x1)).map(|xx| (xx, y)).collect();
        v.extend((y.min(y1)..=y.max(y1)).filter(|&yy| yy != y).map(|yy| (x1, yy)));
        v
    }

    /// Placement rules of the defences: gatehouses go on a road, towers into a wall
    /// two tiles thick.
    pub(crate) fn can_place_defense(&self, k: u16, x: i32, y: i32) -> Option<Result<(), &'static str>> {
        if is_gatehouse(k) {
            let t = self.map.terrain.at_or(x, y, 0);
            return Some(if t & terrain::ROAD == 0 || self.map.building.at_or(x, y, 0) != 0 { Err("Gatehouses go on a road") } else { Ok(()) });
        }
        if is_tower(k) {
            let all_wall = (y..y + 2).all(|yy| (x..x + 2).all(|xx| self.map.building.get(xx, yy).and_then(|id| self.buildings.get(id)).is_some_and(|b| is_wall(b.kind))));
            return Some(if all_wall { Ok(()) } else { Err("Towers must be built into a wall two tiles thick") });
        }
        None
    }

    /// Sets up a new piece of the defences: walls and gatehouses mark their terrain
    /// and redraw with their neighbours; a tower replaces the wall it stands in.
    pub(crate) fn place_defense(&mut self, id: BuildingId) {
        let Some(b) = self.buildings.get(id) else { return };
        let (k, x, y) = (b.kind, b.x, b.y);
        if is_wall(k) {
            self.map.terrain.update(x, y, |t| t | terrain::WALL);
        } else if is_gatehouse(k) {
            // People walk the road through the gate.
            self.map.terrain.update(x, y, |t| (t & !terrain::BUILDING) | terrain::GATEHOUSE | terrain::ROAD);
            let ns = self.map.terrain_is(x, y - 1, terrain::ROAD) || self.map.terrain_is(x, y + 1, terrain::ROAD);
            if let Some(a) = self.defs.building(k).and_then(|d| d.anims.get(if ns { "base_n" } else { "base_w" })) {
                let image = a.image;
                self.map.set_single_image(x, y, image);
            }
        }
        self.redraw_walls_around(x, y, 3);
    }

    /// A tower going up takes the place of the walls under it.
    pub(crate) fn clear_walls_for_tower(&mut self, x: i32, y: i32) {
        for yy in y..y + 2 {
            for xx in x..x + 2 {
                let id = self.map.building.at_or(xx, yy, 0);
                if self.buildings.get(id).is_some_and(|b| is_wall(b.kind)) {
                    self.demolish(id);
                    self.map.terrain.update(xx, yy, |t| t & !terrain::WALL);
                }
            }
        }
    }

    /// Chooses each wall tile's image from its neighbours in the square around
    /// `(x, y)`.
    pub(crate) fn redraw_walls_around(&mut self, x: i32, y: i32, r: i32) {
        let Some(base) = self.defs.building(MUD_WALL).map(|d| d.image) else { return };
        for yy in y - r..=y + r {
            for xx in x - r..=x + r {
                let id = self.map.building.at_or(xx, yy, 0);
                if !self.buildings.get(id).is_some_and(|b| is_wall(b.kind)) {
                    continue;
                }
                let tiles = NEIGHBOURS.map(|(dx, dy)| if self.map.terrain_is(xx + dx, yy + dy, terrain::WALL | terrain::GATEHOUSE) { 1 } else { 0 });
                let mut counters = Vec::new();
                if let Some(c) = crate::tiles::match_context(&self.defs.contexts.wall, &mut counters, tiles) {
                    self.map.set_single_image(xx, yy, base + c.group_offset + c.item_offset);
                }
            }
        }
    }

    /// When a wall or gatehouse goes, its terrain goes with it.
    pub(crate) fn remove_defense(&mut self, k: u16, x: i32, y: i32) {
        if is_wall(k) {
            self.map.terrain.update(x, y, |t| t & !terrain::WALL);
        } else if is_gatehouse(k) {
            self.map.terrain.update(x, y, |t| t & !terrain::GATEHOUSE);
            let (mut rules, map) = self.tile_rules();
            rules.roads_in(map, x - 1, y - 1, x + 1, y + 1);
        }
        self.redraw_walls_around(x, y, 2);
    }

    /// The nearest piece of the defences to `from` within `range` tiles.
    pub(crate) fn nearest_defense(&self, from: (i32, i32), range: i32) -> Option<BuildingId> {
        self.buildings
            .iter()
            .filter(|b| is_defense(b.kind))
            .map(|b| ((b.x - from.0).abs().max((b.y - from.1).abs()), b.id))
            .filter(|&(d, _)| d <= range)
            .min()
            .map(|(_, id)| id)
    }

    /// Daily: towers without a sentry get one from a staffed recruiter.
    pub(crate) fn man_towers(&mut self) {
        let recruiter = self.buildings.iter().find(|b| b.kind == crate::military::RECRUITER && b.workers > 0 && b.road.is_some()).and_then(|b| b.road);
        let Some(from) = recruiter else { return };
        let towers: Vec<(BuildingId, (i32, i32))> = self
            .buildings
            .iter()
            .filter(|b| is_tower(b.kind) && b.workers > 0 && b.walkers[0] == 0)
            .map(|b| (b.id, (b.x, b.y)))
            .collect();
        for (tower, (tx, ty)) in towers {
            let fid = self.figures.spawn(TOWER_SENTRY, from.0, from.1, Travel::Land);
            let map = &self.map;
            let Some(f) = self.figures.get_mut(fid) else { continue };
            f.home = tower;
            f.action = 1;
            // Up onto the tower: the sentry walks to its foot, then stands on it.
            let foot = [(tx - 1, ty), (tx, ty - 1), (tx + 2, ty), (tx, ty + 2), (tx - 1, ty + 1), (tx + 1, ty - 1)]
                .into_iter()
                .find(|&(x, y)| crate::figures::passable(map, Travel::Land, x, y));
            if !foot.is_some_and(|p| f.go_to(map, p)) {
                f.dead = true;
                continue;
            }
            self.buildings.get_mut(tower).expect("present").walkers[0] = fid;
        }
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
