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
    /// Roads, plus ramps and rubble (service walkers, cart pushers): the original's
    /// road-only routing over its citizen route grid.
    Roads,
    /// Anywhere people can walk (immigrants, soldiers, peasants, animals): see
    /// `citizen_ground`.
    Land,
    /// By road if a road route exists, else over land (the homeless, caravans,
    /// gatherers): the original tries its road search before its land search.
    PreferRoads,
    /// Open river water (boats); see `water::navigable`.
    Water,
    /// Invaders: as on land, but a gatehouse bars the way like a wall.
    Hostile,
    /// Straight across anything (the original's cross-country moves): laborers going
    /// to the tile of a monument site they work, craftsmen coming down off one.
    Any,
    /// Frogs: over land as people walk it, and through water.
    Amphibious,
    /// Locusts, and a frog's last hop into a house: anywhere on the map.
    Air,
    /// Crocodiles and hippos: as invaders on land, and through any water.
    Wading,
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
    /// A bazaar buyer's haul: each resource on its list and what it has picked up.
    #[serde(default)]
    pub carried: Vec<(u16, i32)>,
    /// Roamers: still on the first leg out to a road eight tiles from home, and how
    /// many more junctions may fail their random pick before they turn back toward
    /// that spot (-1 = not yet counting).
    #[serde(default)]
    pub roam_out: bool,
    #[serde(default)]
    pub roam_wait: i8,
    /// Wild animals: the frame of the animation they are showing, which also times
    /// some of what they do.
    #[serde(default)]
    pub frame: u8,
    /// The action a figure drawn into a fight goes back to when it is over.
    #[serde(default)]
    pub resume: u16,
    /// Predators: looks round since the last time one saw anything.
    #[serde(default)]
    pub look: u8,
    /// Tile centres reached this tick, for the traffic tally.
    #[serde(skip)]
    pub centres: u8,
    /// Up on a pyramid or mastaba: the block he stands on or is heading for.
    #[serde(default)]
    pub perch: Option<crate::pyramids::Perch>,
    /// The figure this one works with: a sled's laborer and the mason he drags it to
    /// (each names the other), or the one a sled puller or sled follows.
    #[serde(default)]
    pub link: FigureId,
    /// A sled laborer's storage yard.
    #[serde(default)]
    pub yard: u32,
    /// Invaders: which of their army's formations they march in (numbered as the
    /// army raised them).
    #[serde(default)]
    pub band: u8,
    /// The city's warships and transports: their orders and state (see `navy.rs`).
    #[serde(default)]
    pub ship: Option<Box<crate::navy::Ship>>,
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
            carried: Vec::new(),
            roam_out: false,
            roam_wait: -1,
            frame: 0,
            resume: 0,
            look: 0,
            centres: 0,
            perch: None,
            link: 0,
            yard: 0,
            band: 0,
            ship: None,
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

/// What a tile is to people on foot, as the original's citizen route grid rates it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ground {
    /// Roads (and the gatehouses and roadblocks on them).
    Road,
    /// Access ramps and rubble: walkers bound for a building may use them like road.
    Rough,
    /// Open land: clear ground, trees, scrub, marsh, dunes, dry floodplain, canals.
    Open,
}

/// How the citizen route grid rates terrain `t`, or `None` where people can't walk: a
/// dike or flooded floodplain blocks; then a road is road, a ramp rough; buildings
/// block; a canal, dry floodplain, trees, scrub, marsh and dunes are open; rubble is
/// rough; rock, water, walls, ore, gardens and raised ground block; the rest is open.
pub fn citizen_ground(t: u32) -> Option<Ground> {
    use terrain::*;
    if t & DIKE != 0 || t & (WATER | FLOODPLAIN) == WATER | FLOODPLAIN {
        return None;
    }
    if t & ROAD != 0 {
        // A road across water is only walkable as a bridge or ferry crossing.
        return (t & WATER == 0).then_some(Ground::Road);
    }
    if t & ACCESS_RAMP != 0 {
        return Some(Ground::Rough);
    }
    // A fort's parade ground and the festival square's paving are buildings the
    // original's route grids let people cross.
    if t & WALKABLE_BUILDING != 0 {
        return Some(Ground::Open);
    }
    if t & (BUILDING | GATEHOUSE) != 0 {
        return None;
    }
    if t & (CANAL | FLOODPLAIN | TREE | SHRUB | MARSHLAND | DUNE) != 0 {
        return Some(Ground::Open);
    }
    if t & RUBBLE != 0 {
        return Some(Ground::Rough);
    }
    const BLOCKING: u32 = !(GROUNDWATER | MEADOW | FOUNTAIN_RANGE | IRRIGATION_RANGE | BRIDGE);
    (t & BLOCKING == 0).then_some(Ground::Open)
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
        Travel::Roads => ferry || citizen_ground(t).is_some_and(|g| g != Ground::Open),
        Travel::Land | Travel::PreferRoads => ferry || citizen_ground(t).is_some(),
        Travel::Water => crate::water::navigable(map, x, y),
        Travel::Any => true,
        // Invaders cross trees, scrub, marsh, dunes, rubble, gardens, canals, ramps and
        // dry floodplain (the original's route grid for non-citizens); buildings, walls
        // and gatehouses they must batter down.
        Travel::Wading => t & terrain::WATER != 0 || passable(map, Travel::Hostile, x, y),
        Travel::Hostile => {
            const OPEN: u32 = terrain::TREE
                | terrain::SHRUB
                | terrain::MARSHLAND
                | terrain::DUNE
                | terrain::RUBBLE
                | terrain::GARDEN
                | terrain::CANAL
                | terrain::ACCESS_RAMP
                | terrain::FLOODPLAIN;
            t & terrain::GATEHOUSE == 0 && (t & terrain::ROAD != 0 && t & terrain::WATER == 0 || t & (mask::IMPASSABLE | terrain::BUILDING) & !OPEN == 0 || t & terrain::WALKABLE_BUILDING != 0 || ferry)
        }
        Travel::Amphibious => citizen_ground(t).is_some() || t & terrain::WATER != 0 && t & terrain::BUILDING == 0,
        Travel::Air => true,
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
    // Fliers go straight: diagonally while both axes differ, then along the other.
    if travel == Travel::Air {
        let mut route = VecDeque::new();
        let (mut x, mut y) = from;
        while let Some(d) = direction_to((x, y), to) {
            let (dx, dy) = NEIGHBOURS[d as usize];
            x += dx;
            y += dy;
            route.push_back(d);
        }
        return Some(route);
    }
    if travel == Travel::PreferRoads {
        return find_route(map, Travel::Roads, from, to).or_else(|| find_route(map, Travel::Land, from, to));
    }
    let w = map.width;
    let idx = |x: i32, y: i32| (y * w + x) as usize;
    let dirs: &[u8] = match travel {
        Travel::Roads => &[0, 2, 4, 6],
        Travel::Land | Travel::PreferRoads | Travel::Water | Travel::Hostile | Travel::Any | Travel::Amphibious | Travel::Air | Travel::Wading => &[0, 2, 4, 6, 1, 3, 5, 7],
    };
    if known_unreachable(map, travel, from, to) {
        return None;
    }
    with_scratch(map, |s| {
        let to_i = idx(to.0, to.1);
        s.came[idx(from.0, from.1)] = 8;
        s.seen[idx(from.0, from.1)] = s.stamp;
        s.queue.push(from);
        let mut head = 0;
        while let Some(&(x, y)) = s.queue.get(head) {
            head += 1;
            if (x, y) == to {
                break;
            }
            for &d in dirs {
                let (dx, dy) = NEIGHBOURS[d as usize];
                let (nx, ny) = (x + dx, y + dy);
                if !map.contains(nx, ny) {
                    continue;
                }
                let n = idx(nx, ny);
                // The destination may be a building entrance off the road network;
                // allow it.
                if s.seen[n] == s.stamp || !(n == to_i || s.passable(map, travel, nx, ny)) {
                    continue;
                }
                // Both tiles a diagonal step passes between are inside the map, as
                // the step's ends are.
                if d % 2 == 1 {
                    let (a, b) = (idx(nx, y), idx(x, ny));
                    if !(a == to_i || s.passable(map, travel, nx, y)) || !(b == to_i || s.passable(map, travel, x, ny)) {
                        continue;
                    }
                }
                s.seen[n] = s.stamp;
                s.came[n] = d;
                s.queue.push((nx, ny));
            }
        }
        if s.seen[to_i] != s.stamp {
            // Everything reached is passable but `from`: whole regions nothing else
            // reaches out of, remembered so the next search from inside them for a
            // tile they don't touch fails at once.
            let start = if passable(map, travel, from.0, from.1) { 0 } else { 1 };
            remember_unreachable(map, travel, &s.queue[start..]);
            return None;
        }
        let mut route = VecDeque::new();
        let (mut x, mut y) = to;
        while (x, y) != from {
            let d = s.came[idx(x, y)];
            route.push_front(d);
            let (dx, dy) = NEIGHBOURS[d as usize];
            x -= dx;
            y -= dy;
        }
        Some(route)
    })
}

/// Reusable buffers for searches over the map, so a search neither allocates nor
/// clears a map-sized array: a tile's entries count only when stamped with the
/// current search's number.
pub(crate) struct Scratch {
    pub stamp: u32,
    /// The search that last reached each tile.
    pub seen: Vec<u32>,
    /// The step that reached each tile (valid where `seen` is current).
    pub came: Vec<u8>,
    /// Whether each tile is passable: `stamp * 2 + 1` yes, `stamp * 2` no, anything
    /// else not yet asked in this search.
    pass: Vec<u32>,
    pub queue: Vec<(i32, i32)>,
}

impl Scratch {
    /// `passable`, asked once per tile per search. Only for tiles inside the map, and
    /// only while the map doesn't change during the search.
    #[inline]
    pub fn passable(&mut self, map: &Map, travel: Travel, x: i32, y: i32) -> bool {
        let i = (y * map.width + x) as usize;
        let v = self.pass[i];
        if v >> 1 == self.stamp {
            return v & 1 != 0;
        }
        let ok = passable(map, travel, x, y);
        self.pass[i] = self.stamp << 1 | ok as u32;
        ok
    }
}

thread_local! {
    static SCRATCH: std::cell::RefCell<Scratch> = const { std::cell::RefCell::new(Scratch { stamp: 0, seen: Vec::new(), came: Vec::new(), pass: Vec::new(), queue: Vec::new() }) };
}

/// Runs a search with the scratch buffers sized for `map` and a fresh stamp. Searches
/// must not nest.
pub(crate) fn with_scratch<R>(map: &Map, f: impl FnOnce(&mut Scratch) -> R) -> R {
    SCRATCH.with_borrow_mut(|s| {
        let n = (map.width * map.height).max(0) as usize;
        // Stamps fit in 31 bits (the passable cache keeps one bit beside them).
        if s.seen.len() != n || s.stamp >= u32::MAX >> 2 {
            s.seen = vec![0; n];
            s.came = vec![0; n];
            s.pass = vec![0; n];
            s.stamp = 0;
        }
        s.stamp += 1;
        s.queue.clear();
        f(s)
    })
}

/// Regions of the map that failed route searches filled without finding their way
/// out, for one kind of travel and one state of the terrain. Each is whole: every
/// passable tile next to one of its tiles is in it, so a search starting in one can
/// only reach its tiles and the tiles beside them.
struct DeadEnds {
    travel: Travel,
    version: u64,
    width: i32,
    /// The region each tile is in; marks below `first` are from older terrain.
    marks: Vec<u32>,
    first: u32,
    next: u32,
}

thread_local! {
    static DEAD_ENDS: std::cell::RefCell<Vec<DeadEnds>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// The tiles a route search from `from` starts into: `from` itself if passable, and
/// its passable neighbours north, east, south and west. Everything it reaches is in
/// their regions.
fn start_tiles(map: &Map, travel: Travel, from: (i32, i32)) -> impl Iterator<Item = (i32, i32)> + '_ {
    std::iter::once((0, 0)).chain([0, 2, 4, 6].map(|d| NEIGHBOURS[d])).map(move |(dx, dy)| (from.0 + dx, from.1 + dy)).filter(move |&(x, y)| passable(map, travel, x, y))
}

/// Whether an earlier failed search proves there is no route from `from` to `to`: the
/// search from `from` would stay inside one remembered region, and `to` is not beside
/// any tile of it, nor next to `from`.
fn known_unreachable(map: &Map, travel: Travel, from: (i32, i32), to: (i32, i32)) -> bool {
    DEAD_ENDS.with_borrow(|all| {
        let Some(d) = all.iter().find(|d| d.travel == travel && d.version == map.terrain.version() && d.width == map.width) else { return false };
        let mark = |(x, y): (i32, i32)| if map.contains(x, y) { d.marks[(y * map.width + x) as usize] } else { 0 };
        let mut starts = start_tiles(map, travel, from);
        let Some(first) = starts.next() else { return false };
        let region = mark(first);
        if region < d.first || !starts.all(|t| mark(t) == region) {
            return false;
        }
        if (to.0 - from.0).abs() + (to.1 - from.1).abs() <= 1 || mark(to) == region {
            return false;
        }
        [0, 2, 4, 6].iter().all(|&i| mark((to.0 + NEIGHBOURS[i].0, to.1 + NEIGHBOURS[i].1)) != region)
    })
}

/// Records `tiles`, every passable tile a failed search reached, as a region.
fn remember_unreachable(map: &Map, travel: Travel, tiles: &[(i32, i32)]) {
    if tiles.is_empty() {
        return;
    }
    DEAD_ENDS.with_borrow_mut(|all| {
        let n = (map.width * map.height).max(0) as usize;
        let i = match all.iter().position(|d| d.travel == travel) {
            Some(i) => i,
            None => {
                all.push(DeadEnds { travel, version: 0, width: 0, marks: Vec::new(), first: 1, next: 1 });
                all.len() - 1
            }
        };
        let d = &mut all[i];
        if d.width != map.width || d.marks.len() != n || d.next >= u32::MAX - 1 {
            *d = DeadEnds { travel, version: map.terrain.version(), width: map.width, marks: vec![0; n], first: 1, next: 1 };
        } else if d.version != map.terrain.version() {
            d.version = map.terrain.version();
            d.first = d.next;
        }
        let at = |(x, y): (i32, i32)| (y * map.width + x) as usize;
        // Regions that meet share their tiles; keep only the first, so that a tile's
        // mark always names a whole region.
        if tiles.iter().any(|&t| d.marks[at(t)] >= d.first) {
            return;
        }
        let region = d.next;
        d.next += 1;
        for &t in tiles {
            d.marks[at(t)] = region;
        }
    })
}

/// Walking distances from `from` over tiles a figure travelling by `travel` may stand
/// on, stepping north, east, south and west, indexed `y * width + x`: 1 at `from`, 0
/// where it can't get to (the original's distance flood, `FUN_00519a30`).
pub fn route_distances(map: &Map, travel: Travel, from: (i32, i32)) -> Vec<i32> {
    let (w, h) = (map.width, map.height);
    let mut dist = vec![0i32; (w * h).max(0) as usize];
    if !map.contains(from.0, from.1) {
        return dist;
    }
    dist[(from.1 * w + from.0) as usize] = 1;
    let mut queue = VecDeque::from([from]);
    while let Some((x, y)) = queue.pop_front() {
        let d = dist[(y * w + x) as usize];
        for i in (0..8).step_by(2) {
            let (nx, ny) = (x + NEIGHBOURS[i].0, y + NEIGHBOURS[i].1);
            if !passable(map, travel, nx, ny) {
                continue;
            }
            let n = (ny * w + nx) as usize;
            if dist[n] == 0 {
                dist[n] = d + 1;
                queue.push_back((nx, ny));
            }
        }
    }
    dist
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
                self.centres = self.centres.saturating_add(1);
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
    fn invaders_cross_rubble_and_trees_but_not_buildings() {
        let mut map = open_map(5, 1);
        map.terrain.set(1, 0, terrain::RUBBLE);
        map.terrain.set(2, 0, terrain::TREE | terrain::SHRUB);
        map.terrain.set(3, 0, terrain::FLOODPLAIN);
        assert!(find_route(&map, Travel::Hostile, (0, 0), (4, 0)).is_some());
        for (bits, open) in [(terrain::BUILDING, false), (terrain::WALL, false), (terrain::ROCK, false), (terrain::WATER | terrain::FLOODPLAIN, false), (terrain::DUNE, true)] {
            map.terrain.set(3, 0, bits);
            assert_eq!(passable(&map, Travel::Hostile, 3, 0), open, "{bits:#x}");
        }
    }

    #[test]
    fn a_parade_ground_is_a_building_everyone_on_foot_may_cross() {
        let mut map = open_map(3, 1);
        map.terrain.set(1, 0, terrain::BUILDING | terrain::WALKABLE_BUILDING);
        for travel in [Travel::Land, Travel::PreferRoads, Travel::Hostile, Travel::Wading] {
            assert!(passable(&map, travel, 1, 0), "{travel:?}");
        }
        assert!(!passable(&map, Travel::Roads, 1, 0));
        map.terrain.set(1, 0, terrain::BUILDING);
        assert!(!passable(&map, Travel::Land, 1, 0));
    }

    #[test]
    fn people_cross_rough_ground_but_road_walkers_keep_to_roads() {
        use terrain::*;
        let mut map = open_map(3, 1);
        for (bits, land, roads) in [
            (FLOODPLAIN, true, false),
            (TREE | SHRUB, true, false),
            (MARSHLAND, true, false),
            (DUNE, true, false),
            (CANAL, true, false),
            (MEADOW | GROUNDWATER, true, false),
            (RUBBLE, true, true),
            (ACCESS_RAMP, true, true),
            (ROAD | BUILDING, true, true),
            (ROAD | DIKE, false, false),
            (WATER | FLOODPLAIN, false, false),
            (ROAD | WATER, false, false),
            (BUILDING, false, false),
            (ROCK, false, false),
            (GARDEN, false, false),
            (ELEVATION, false, false),
        ] {
            map.terrain.set(1, 0, bits);
            assert_eq!(passable(&map, Travel::Land, 1, 0), land, "land {bits:#x}");
            assert_eq!(passable(&map, Travel::Roads, 1, 0), roads, "roads {bits:#x}");
        }
    }

    #[test]
    fn preferring_roads_takes_the_road_when_there_is_one() {
        // Row 0 is a road looping round a clear row 1.
        let mut map = open_map(5, 3);
        for (x, y) in [(0, 1), (0, 0), (1, 0), (2, 0), (3, 0), (4, 0), (4, 1)] {
            map.terrain.set(x, y, terrain::ROAD);
        }
        let road = find_route(&map, Travel::PreferRoads, (0, 1), (4, 1)).unwrap();
        assert_eq!(road.len(), 6);
        map.terrain.set(2, 0, 0);
        let land = find_route(&map, Travel::PreferRoads, (0, 1), (4, 1)).unwrap();
        assert_eq!(land.len(), 4);
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
