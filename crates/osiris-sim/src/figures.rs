//! Walkers: people, animals and carts that move over the map.
//!
//! Movement follows the original: a figure advances 15 steps per tile, switches its
//! logical tile at step 8, and chooses its next direction when it reaches a tile
//! centre. Directions are indexes into `map::NEIGHBOURS` (0 = north, clockwise).

use crate::map::{Map, NEIGHBOURS, mask, terrain};
use std::collections::VecDeque;

pub type FigureId = u32;

/// How a figure may travel, which decides the tiles its routes can use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Travel {
    /// Roads only (service walkers, cart pushers).
    Roads,
    /// Any passable land, preferring nothing (immigrants, animals, hunters).
    Land,
    /// Open river water (boats); see `water::navigable`.
    Water,
    /// Invaders: as on land, but a gatehouse bars the way like a wall.
    Hostile,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Step {
    /// Still walking.
    Moving,
    /// Reached the end of its route.
    Arrived,
    /// The next tile is blocked; the route must be recomputed.
    Blocked,
    /// No route exists.
    Lost,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Figure {
    pub id: FigureId,
    /// Figure type id (original numbering, see `figures.toml`).
    pub kind: u16,
    pub x: i32,
    pub y: i32,
    /// Movement progress within the current step, 0..15.
    pub progress: u8,
    /// Current heading, 0..8.
    pub direction: u8,
    pub travel: Travel,
    /// Remaining route as directions, first step first.
    pub route: VecDeque<u8>,
    pub destination: Option<(i32, i32)>,
    /// Game ticks of movement per simulation tick.
    pub speed: u8,
    /// Animation frame counter (in ticks).
    pub anim_tick: u32,
    pub moving: bool,
    /// Building this figure belongs to (0 = none).
    pub home: u32,
    /// Building it is heading for (0 = none).
    pub target: u32,
    /// Type-specific state machine position.
    pub action: u16,
    /// Generic counter used by actions (waiting time, roam length, ...).
    pub counter: i32,
    /// Remaining roam steps for roaming walkers.
    pub roam_left: i32,
    pub roam_turn: i8,
    pub roam_recent: VecDeque<(i32, i32)>,
    /// Payload: resource id and amount, or people for migrants.
    pub cargo: u16,
    pub amount: i32,
    pub dead: bool,
    /// Fighters: damage taken, the formation they belong to and their place in it,
    /// the figure they are fighting, and how far their blow has come.
    #[serde(default)]
    pub damage: i32,
    #[serde(default)]
    pub formation: u16,
    #[serde(default)]
    pub slot: u8,
    #[serde(default)]
    pub foe: u32,
    #[serde(default)]
    pub attack_tick: u16,
    /// Consecutive failed pathfinds to the current kind of target (invaders picking an
    /// attack spot): backs off the retry interval so a permanently unreachable target
    /// (across water, with no wall to batter) doesn't force a full-map search anew
    /// every few dozen ticks forever.
    #[serde(default)]
    pub stuck: u8,
}

impl Figure {
    pub fn new(id: FigureId, kind: u16, x: i32, y: i32, travel: Travel) -> Self {
        Self {
            id,
            kind,
            x,
            y,
            progress: 0,
            direction: 0,
            travel,
            route: VecDeque::new(),
            destination: None,
            speed: 1,
            anim_tick: 0,
            moving: false,
            home: 0,
            target: 0,
            action: 0,
            counter: 0,
            roam_left: 0,
            roam_turn: 2,
            roam_recent: VecDeque::new(),
            cargo: 0,
            amount: 0,
            dead: false,
            damage: 0,
            formation: 0,
            slot: 0,
            foe: 0,
            attack_tick: 0,
            stuck: 0,
        }
    }

    /// Pixel offset from the centre of its tile for drawing, in the original's
    /// orientation-0 projection.
    pub fn pixel_offset(&self) -> (i32, i32) {
        if !self.moving {
            return (0, 0);
        }
        let p = if self.progress >= 8 { self.progress as i32 - 15 } else { self.progress as i32 };
        match self.direction {
            0 => (2 * p, -p),
            1 => (4 * p, 0),
            2 => (2 * p, p),
            3 => (0, 2 * p),
            4 => (-2 * p, p),
            5 => (-4 * p, 0),
            6 => (-2 * p, -p),
            _ => (0, -2 * p),
        }
    }
}

/// Whether a figure travelling by `travel` may stand on `(x, y)`.
pub fn passable(map: &Map, travel: Travel, x: i32, y: i32) -> bool {
    if !map.contains(x, y) {
        return false;
    }
    let t = map.terrain.at_or(x, y, 0);
    // A working ferry's crossing, or a bridge, counts as road for people on foot.
    let ferry = t & (terrain::FERRY_ROUTE | terrain::BRIDGE) != 0;
    match travel {
        Travel::Roads => t & (terrain::ROAD | terrain::ACCESS_RAMP) != 0 && t & terrain::WATER == 0 || ferry,
        Travel::Land => {
            t & terrain::ROAD != 0 && t & terrain::WATER == 0
                || t & (mask::IMPASSABLE | terrain::BUILDING) == 0
                || ferry
        }
        Travel::Water => crate::water::navigable(map, x, y),
        Travel::Hostile => t & terrain::GATEHOUSE == 0 && passable(map, Travel::Land, x, y),
    }
}

/// Shortest route from `from` to `to` as directions. Land figures and boats move in 8
/// directions but never cut a corner between two blocked tiles; road figures move
/// orthogonally.
pub fn find_route(map: &Map, travel: Travel, from: (i32, i32), to: (i32, i32)) -> Option<VecDeque<u8>> {
    if from == to {
        return Some(VecDeque::new());
    }
    // `from`/`to` can come straight from a click or script command past the map edge
    // (e.g. `move_company`, which doesn't clamp its target tile): without this, the
    // flat-index lookups below panic instead of just failing to find a route, matching
    // the guard `water::water_path` and `World::road_path` already use.
    if !map.contains(from.0, from.1) || !map.contains(to.0, to.1) {
        return None;
    }
    let (w, h) = (map.width, map.height);
    let idx = |x: i32, y: i32| (y * w + x) as usize;
    let mut came = vec![u8::MAX; (w * h) as usize];
    let mut queue = VecDeque::from([from]);
    came[idx(from.0, from.1)] = 8;
    let dirs: &[u8] = match travel {
        Travel::Roads => &[0, 2, 4, 6],
        Travel::Land | Travel::Water | Travel::Hostile => &[0, 2, 4, 6, 1, 3, 5, 7],
    };
    // The destination may be a building entrance off the road network; allow it.
    let ok = |x: i32, y: i32| (x, y) == to || passable(map, travel, x, y);
    while let Some((x, y)) = queue.pop_front() {
        if (x, y) == to {
            break;
        }
        for &d in dirs {
            let (dx, dy) = NEIGHBOURS[d as usize];
            let (nx, ny) = (x + dx, y + dy);
            if !map.contains(nx, ny) || came[idx(nx, ny)] != u8::MAX || !ok(nx, ny) {
                continue;
            }
            if d % 2 == 1 && !(ok(x + dx, y) && ok(x, y + dy)) {
                continue;
            }
            came[idx(nx, ny)] = d;
            queue.push_back((nx, ny));
        }
    }
    if came[idx(to.0, to.1)] == u8::MAX {
        return None;
    }
    let mut route = VecDeque::new();
    let (mut x, mut y) = to;
    while (x, y) != from {
        let d = came[idx(x, y)];
        route.push_front(d);
        let (dx, dy) = NEIGHBOURS[d as usize];
        x -= dx;
        y -= dy;
    }
    Some(route)
}

impl Figure {
    /// Sets a destination and computes the route to it.
    pub fn go_to(&mut self, map: &Map, to: (i32, i32)) -> bool {
        self.destination = Some(to);
        match find_route(map, self.travel, (self.x, self.y), to) {
            Some(r) => {
                self.route = r;
                true
            }
            None => {
                self.route.clear();
                false
            }
        }
    }

    /// Advances along the route by `speed` movement ticks.
    pub fn walk(&mut self, map: &Map) -> Step {
        self.anim_tick += 1;
        for _ in 0..self.speed {
            if !self.moving {
                // At a tile centre: take the next step, if any.
                let Some(&d) = self.route.front() else {
                    return if self.destination.is_some_and(|dst| dst != (self.x, self.y)) {
                        Step::Lost
                    } else {
                        Step::Arrived
                    };
                };
                let (dx, dy) = NEIGHBOURS[d as usize];
                let (nx, ny) = (self.x + dx, self.y + dy);
                if Some((nx, ny)) != self.destination && !passable(map, self.travel, nx, ny) {
                    return Step::Blocked;
                }
                self.route.pop_front();
                self.direction = d;
                self.moving = true;
                self.progress = 0;
            }
            self.progress += 1;
            if self.progress == 8 {
                let (dx, dy) = NEIGHBOURS[self.direction as usize];
                self.x += dx;
                self.y += dy;
            }
            if self.progress >= 15 {
                self.progress = 0;
                self.moving = false;
            }
        }
        Step::Moving
    }

    /// Current walk-cycle frame for a `frames`-long animation. The original advances
    /// one frame per tick.
    pub fn frame(&self, frames: u32) -> u32 {
        if frames == 0 { 0 } else { self.anim_tick % frames }
    }
}

/// Storage for all figures, with stable ids.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Figures {
    slots: Vec<Option<Figure>>,
    free: Vec<FigureId>,
}

impl Figures {
    pub fn spawn(&mut self, kind: u16, x: i32, y: i32, travel: Travel) -> FigureId {
        let id = self.free.pop().unwrap_or_else(|| {
            self.slots.push(None);
            self.slots.len() as FigureId
        });
        self.slots[id as usize - 1] = Some(Figure::new(id, kind, x, y, travel));
        id
    }

    pub fn get(&self, id: FigureId) -> Option<&Figure> {
        self.slots.get((id as usize).wrapping_sub(1))?.as_ref()
    }

    pub fn get_mut(&mut self, id: FigureId) -> Option<&mut Figure> {
        self.slots.get_mut((id as usize).wrapping_sub(1))?.as_mut()
    }

    pub fn remove(&mut self, id: FigureId) {
        if let Some(slot) = self.slots.get_mut((id as usize).wrapping_sub(1))
            && slot.take().is_some()
        {
            self.free.push(id);
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &Figure> {
        self.slots.iter().flatten()
    }

    pub fn ids(&self) -> Vec<FigureId> {
        self.iter().map(|f| f.id).collect()
    }

    pub fn len(&self) -> usize {
        self.iter().count()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// The direction (index into `NEIGHBOURS`) from one tile toward another, if they differ.
pub fn direction_to(from: (i32, i32), to: (i32, i32)) -> Option<u8> {
    let d = ((to.0 - from.0).signum(), (to.1 - from.1).signum());
    NEIGHBOURS.iter().position(|&n| n == d).map(|i| i as u8)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::Grid;

    fn open_map(w: i32, h: i32) -> Map {
        Map {
            width: w,
            height: h,
            terrain: Grid::new(w, h),
            images: Grid::new(w, h),
            edges: Grid::new(w, h),
            bitfields: Grid::new(w, h),
            elevation: Grid::new(w, h),
            random: Grid::new(w, h),
            fertility: Grid::new(w, h),
            moisture: Grid::new(w, h),
            vegetation: Grid::new(w, h),
            building: Grid::new(w, h),
            border: Vec::new(),
        }
    }

    #[test]
    fn walks_fifteen_steps_per_tile() {
        let map = open_map(10, 10);
        let mut f = Figure::new(1, 0, 0, 0, Travel::Land);
        assert!(f.go_to(&map, (3, 0)));
        let mut ticks = 0;
        while f.walk(&map) == Step::Moving {
            ticks += 1;
        }
        assert_eq!((f.x, f.y, ticks), (3, 0, 45));
    }

    #[test]
    fn roads_only_route_follows_road() {
        let mut map = open_map(5, 5);
        for x in 0..5 {
            map.terrain.set(x, 2, terrain::ROAD);
        }
        let r = find_route(&map, Travel::Roads, (0, 2), (4, 2)).unwrap();
        assert_eq!(r.len(), 4);
        assert!(find_route(&map, Travel::Roads, (0, 2), (0, 0)).is_none());
    }
}
