//! Royal burial tombs (Cleopatra's Valley of the Kings): small, medium, large and
//! grand, after the tombs of Thutmose, Tutankhamun, Seti and Ramses. A tomb is cut
//! into a cliff: its bulk must be all cliff and its entrance, just outside it, clear
//! land with clear land beside it. There is no player rotation; each layout is fixed.
//!
//! Inside are chambers (corridors, halls and burial rooms), each worked in three
//! stages by one man at a time: a stonemason digs it out, then an artisan from the
//! Artisans' Guild plasters it (bringing 100 clay) and one paints it (bringing 100
//! paint). The first chamber is open from the start; each opens the next ones when
//! it is plastered, so several are worked at once. No one enters without lamps:
//! every man sent in takes 100 of the tomb's lamps, and laborers from the work
//! camps fetch 400 at a time from a storage yard when the stock is under 400.
//!
//! While being cut the tomb is drawn open, its chambers in their stage (rubble,
//! plastered, painted, and furnished once the burial provisions are in), dark when
//! it has no lamps and no one inside. When every chamber is painted and the burial
//! provisions are delivered, the tomb is sealed: it hides, drawn as plain cliff.

use crate::buildings::{BuildingId, kind};
use crate::figures::{FigureId, Step, Travel};
use crate::map::terrain;
use crate::monuments::STONEMASON;
use crate::world::World;

pub const SMALL_ROYAL_TOMB: u16 = 229;
pub const MEDIUM_ROYAL_TOMB: u16 = 234;
pub const LARGE_ROYAL_TOMB: u16 = 235;
pub const GRAND_ROYAL_TOMB: u16 = 236;
pub const ARTISANS_GUILD: u16 = 231;
pub const TOMB_ARTISAN: u16 = 108;

const CLAY: u16 = 11;
const PAINT: u16 = 33;
const LAMPS: u16 = 34;
/// Lamps a laborer brings, and the stock under which the tomb sends for more.
const LAMP_RUN: i32 = 400;
/// Lamps each man sent into the tomb takes.
const LAMPS_PER_MAN: i32 = 100;
/// Clay or paint an artisan brings for one chamber.
const ARTISAN_LOAD: i32 = 100;
/// Terrain the tests ignore: shrub, groundwater, meadow, fountain and irrigation range.
const IGNORED: u32 = terrain::SHRUB | terrain::GROUNDWATER | terrain::MEADOW | terrain::FOUNTAIN_RANGE | terrain::IRRIGATION_RANGE | terrain::BRIDGE;

/// A chamber: its tiles (from the bulk's corner), which way its image faces, its lit
/// image group (the unlit one follows), the work each stage takes, the chambers it
/// opens, and whether it is furnished once the burial provisions are in.
pub struct Chamber {
    pub x: i32,
    pub y: i32,
    pub size: i32,
    pub dir: u32,
    pub lit: u16,
    pub work: u16,
    pub opens: &'static [usize],
    pub furnished: bool,
}

/// Image groups of the entrance and of the cliff edges drawn round the bulk.
pub struct Groups {
    pub entrance: u16,
    pub edge: u16,
    pub corner: u16,
}

/// A tomb's layout: its bulk (tiles across and down); its entrance tile (from the
/// bulk's corner) and facing; and its bulk's tiles by row: `#` a chamber's, `.` bare
/// ground, `a`-`d` a cliff edge and `A`-`D` a cliff corner facing 0-3.
pub struct Layout {
    pub size: (i32, i32),
    pub entrance: (i32, i32, u32),
    pub groups: Groups,
    pub rows: &'static [&'static str],
    pub chambers: &'static [Chamber],
}

pub fn layout(k: u16) -> Option<&'static Layout> {
    Some(match k {
        SMALL_ROYAL_TOMB => &SMALL,
        MEDIUM_ROYAL_TOMB => &MEDIUM,
        LARGE_ROYAL_TOMB => &LARGE,
        GRAND_ROYAL_TOMB => &GRAND,
        _ => return None,
    })
}

pub fn is_royal_tomb(k: u16) -> bool {
    layout(k).is_some()
}

/// A chamber's state: its stage (0 shut, 1 digging, 2 plastering, 3 painting, 4
/// done), the work its stage still needs, and the man on it.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ChamberState {
    pub progress: u8,
    pub left: u16,
    pub worker: FigureId,
}

/// The phrase announcing a tier's completion.
pub fn phrase(k: u16) -> &'static str {
    match k {
        SMALL_ROYAL_TOMB => "smalltomb",
        MEDIUM_ROYAL_TOMB => "medtomb",
        LARGE_ROYAL_TOMB => "largetomb",
        _ => "grandtomb",
    }
}

/// A tile a tomb would take, and the placement rule it breaks, if any.
pub(crate) type TileRule = ((i32, i32), Option<&'static str>);

/// Placement messages, worst first.
const OUTSIDE: &str = "Outside the map";
const INTO_CLIFFS: &str = "A royal tomb must be cut into the cliffs";
const ENTRANCE: &str = "The tomb's entrance must open onto clear land";
const IN_THE_WAY: &str = "People are in the way";

impl World {
    /// A tomb's entrance tile.
    pub fn royal_tomb_entrance(&self, id: BuildingId) -> Option<(i32, i32)> {
        let b = self.buildings.get(id)?;
        let l = layout(b.kind)?;
        Some((b.x + l.entrance.0, b.y + l.entrance.1))
    }

    /// Placement: every bulk tile must be cliff and nothing else, the entrance tile
    /// clear land with clear land on at least one side, and no other tomb of the same
    /// size may be under way.
    pub(crate) fn can_place_royal_tomb(&self, k: u16, x: i32, y: i32) -> Option<Result<(), &'static str>> {
        let tiles = self.royal_tomb_tiles(k, x, y)?;
        // The worst problem first, whichever tile has it.
        for why in [OUTSIDE, INTO_CLIFFS, ENTRANCE, IN_THE_WAY] {
            if tiles.iter().any(|&(_, p)| p == Some(why)) {
                return Some(Err(why));
            }
        }
        if self.buildings.iter().any(|b| b.kind == k && b.monument.is_some() && self.royal_tomb_percent(b.id) < 100) {
            return Some(Err("Only one tomb of this size may be cut at a time"));
        }
        Some(Ok(()))
    }

    /// Each tile a royal tomb of type `k` at `(x, y)` would take, its bulk and then its
    /// entrance, with the placement rule it breaks, if any.
    pub(crate) fn royal_tomb_tiles(&self, k: u16, x: i32, y: i32) -> Option<Vec<TileRule>> {
        let l = layout(k)?;
        let t = |xx: i32, yy: i32| self.map.terrain.at_or(xx, yy, 0) & !IGNORED;
        let (w, h) = l.size;
        let inside = |xx: i32, yy: i32| xx >= 1 && yy >= 1 && xx < self.map.width - 1 && yy < self.map.height - 1;
        let cliff = terrain::CLIFF | terrain::ROCK;
        let mut tiles: Vec<TileRule> = (y..y + h)
            .flat_map(|yy| (x..x + w).map(move |xx| (xx, yy)))
            .map(|(xx, yy)| {
                let why = if !inside(xx, yy) {
                    Some(OUTSIDE)
                } else if t(xx, yy) != cliff {
                    Some(INTO_CLIFFS)
                } else {
                    None
                };
                ((xx, yy), why)
            })
            .collect();
        let (ex, ey) = (x + l.entrance.0, y + l.entrance.1);
        let clear = |xx: i32, yy: i32| self.map.contains(xx, yy) && t(xx, yy) == 0;
        let entrance = if !inside(ex, ey) {
            Some(OUTSIDE)
        } else if !clear(ex, ey) || ![(0, -1), (1, 0), (0, 1), (-1, 0)].iter().any(|&(dx, dy)| clear(ex + dx, ey + dy)) {
            Some(ENTRANCE)
        } else if self.figures.iter().any(|f| (f.x, f.y) == (ex, ey)) {
            Some(IN_THE_WAY)
        } else {
            None
        };
        tiles.push(((ex, ey), entrance));
        Some(tiles)
    }

    /// A new tomb: its entrance tile taken, the first chamber open for digging.
    pub(crate) fn place_royal_tomb(&mut self, id: BuildingId) {
        let Some(l) = self.buildings.get(id).and_then(|b| layout(b.kind)) else { return };
        let Some((ex, ey)) = self.royal_tomb_entrance(id) else { return };
        self.map.terrain.update(ex, ey, |t| (t & !(terrain::MEADOW | terrain::SHRUB)) | terrain::BUILDING);
        self.map.building.set(ex, ey, id);
        if let Some(m) = self.buildings.get_mut(id).and_then(|b| b.monument.as_mut()) {
            m.chambers = l.chambers.iter().map(|_| ChamberState::default()).collect();
            m.chambers[0] = ChamberState { progress: 1, left: l.chambers[0].work, worker: 0 };
        }
    }

    /// How far a tomb is cut, 0-100: its chambers' stages over four each.
    pub fn royal_tomb_percent(&self, id: BuildingId) -> i32 {
        let Some(m) = self.buildings.get(id).and_then(|b| b.monument.as_ref()) else { return 0 };
        if m.chambers.is_empty() {
            return if m.finished { 100 } else { 0 };
        }
        let done: i32 = m.chambers.iter().map(|c| c.progress as i32).sum();
        let whole = m.chambers.len() as i32 * 4;
        if done >= whole { 100 } else { (done * 100 / whole).min(99) }
    }

    /// Lamps in a tomb's stock.
    pub fn royal_tomb_lamps(&self, id: BuildingId) -> i32 {
        self.buildings.get(id).and_then(|b| b.monument.as_ref()).map_or(0, |m| m.lamps)
    }

    /// Whether a tomb is dark: no lamps, and no one inside.
    fn royal_tomb_dark(&self, id: BuildingId) -> bool {
        let Some(m) = self.buildings.get(id).and_then(|b| b.monument.as_ref()) else { return false };
        m.lamps <= 0 && m.chambers.iter().all(|c| c.worker == 0)
    }

    /// Draws a tomb: bare ground and cliff edges over its bulk, each open chamber in
    /// its stage, and its entrance; once sealed, plain cliff and a rock at the door.
    pub(crate) fn refresh_royal_tomb(&mut self, id: BuildingId) {
        let Some(b) = self.buildings.get(id) else { return };
        let (Some(l), Some(bdef), Some(m)) = (layout(b.kind), self.defs.building(b.kind), b.monument.as_ref()) else { return };
        let (x0, y0) = (b.x, b.y);
        let img = |key: &str| bdef.anims.get(key).map(|a| a.image);
        let group = |g: u16| img(&format!("g{g}"));
        let (Some(ground), Some(rock)) = (img("ground"), img("rock")) else { return };
        let (ex, ey) = (x0 + l.entrance.0, y0 + l.entrance.1);
        let random = |x: i32, y: i32| self.map.random.at_or(x, y, 0) as u32;
        let mut singles: Vec<(i32, i32, u32)> = Vec::new();
        let mut blocks: Vec<(i32, i32, i32, u32)> = Vec::new();
        if m.finished {
            for y in y0..y0 + l.size.1 {
                for x in x0..x0 + l.size.0 {
                    singles.push((x, y, crate::terrain_images::cliff_image(&self.map, &self.defs, x, y)));
                }
            }
            singles.push((ex, ey, rock + (random(ex, ey) & 7)));
        } else {
            let dark = self.royal_tomb_dark(id);
            let furnished = self.burial_complete() && self.royal_tomb_percent(id) == 100;
            let bare = |x: i32, y: i32| ground + random(x, y) % 5;
            for (dy, row) in l.rows.iter().enumerate() {
                for (dx, c) in row.bytes().enumerate() {
                    let (x, y) = (x0 + dx as i32, y0 + dy as i32);
                    let image = match c {
                        b'.' => Some(bare(x, y)),
                        b'a'..=b'd' => group(l.groups.edge).map(|g| g + (c - b'a') as u32),
                        b'A'..=b'D' => group(l.groups.corner).map(|g| g + (c - b'A') as u32),
                        _ => None,
                    };
                    if let Some(image) = image {
                        singles.push((x, y, image));
                    }
                }
            }
            for (c, s) in l.chambers.iter().zip(&m.chambers) {
                let (x, y) = (x0 + c.x, y0 + c.y);
                if s.progress == 0 {
                    for yy in y..y + c.size {
                        for xx in x..x + c.size {
                            singles.push((xx, yy, bare(xx, yy)));
                        }
                    }
                    continue;
                }
                let image = if dark {
                    group(c.lit + 1).map(|g| g + c.dir)
                } else {
                    let stage = match s.progress {
                        3 => 4,
                        4 => 8,
                        _ => 0,
                    } + if furnished && c.furnished { 4 } else { 0 };
                    group(c.lit).map(|g| g + c.dir + stage)
                };
                if let Some(image) = image {
                    blocks.push((x, y, c.size, image));
                }
            }
            if let Some(g) = group(l.groups.entrance) {
                singles.push((ex, ey, g + l.entrance.2));
            }
        }
        for (x, y, image) in singles {
            self.map.set_single_image(x, y, image);
        }
        for (x, y, n, image) in blocks {
            self.map.set_footprint(x, y, n, image);
        }
    }

    /// The first chamber of tomb `id` at `stage` that a man could be sent into now:
    /// no one on it, and lamps enough for him.
    fn royal_tomb_chamber_at(&self, id: BuildingId, stage: u8) -> Option<usize> {
        let m = self.buildings.get(id).and_then(|b| b.monument.as_ref()).filter(|m| !m.finished)?;
        if m.lamps < LAMPS_PER_MAN {
            return None;
        }
        m.chambers.iter().position(|c| c.worker == 0 && c.left > 0 && c.progress == stage)
    }

    /// A chamber of tomb `id` that man `figure` could take on now: digging for a
    /// stonemason, plastering or painting for an artisan.
    pub(crate) fn royal_tomb_job(&self, id: BuildingId, figure: u16) -> Option<usize> {
        match figure {
            STONEMASON => self.royal_tomb_chamber_at(id, 1),
            TOMB_ARTISAN => self.royal_tomb_chamber_at(id, 2).or_else(|| self.royal_tomb_chamber_at(id, 3)),
            _ => None,
        }
    }

    /// Whether some open chamber of tomb `id` waits for a man of trade `figure`, with
    /// no one on it (lamps or not).
    pub fn royal_tomb_waiting(&self, id: BuildingId, figure: u16) -> bool {
        let Some(m) = self.buildings.get(id).and_then(|b| b.monument.as_ref()).filter(|m| !m.finished) else { return false };
        let stages: &[u8] = if figure == STONEMASON { &[1] } else { &[2, 3] };
        m.chambers.iter().any(|c| c.worker == 0 && c.left > 0 && stages.contains(&c.progress))
    }

    /// Sends man `fid` into chamber `c`: he takes the tomb's lamps for it.
    pub(crate) fn assign_tomb_job(&mut self, id: BuildingId, c: usize, fid: FigureId) {
        if let Some(m) = self.buildings.get_mut(id).and_then(|b| b.monument.as_mut()) {
            m.lamps -= LAMPS_PER_MAN;
            m.chambers[c].worker = fid;
        }
        if let Some(f) = self.figures.get_mut(fid) {
            f.amount = c as i32 + 1;
        }
    }

    /// A chamber's stage is done: it moves on (opening the chambers after it when it
    /// is plastered), and its man is free.
    fn finish_stage(&mut self, id: BuildingId, c: usize) {
        let Some(l) = self.buildings.get(id).and_then(|b| layout(b.kind)) else { return };
        let Some(m) = self.buildings.get_mut(id).and_then(|b| b.monument.as_mut()) else { return };
        let s = &mut m.chambers[c];
        s.worker = 0;
        if s.progress >= 4 {
            return;
        }
        s.progress += 1;
        s.left = if s.progress < 4 { l.chambers[c].work } else { 0 };
        if s.progress == 3 {
            for &o in l.chambers[c].opens {
                if let Some(n) = m.chambers.get_mut(o).filter(|n| n.progress == 0) {
                    n.progress = 1;
                    n.left = l.chambers[o].work;
                }
            }
        }
        self.refresh_royal_tomb(id);
    }

    /// A clear tile beside a tomb's entrance for its men to stand on, nearest `from`.
    pub fn royal_tomb_access(&self, id: BuildingId, from: (i32, i32)) -> Option<(i32, i32)> {
        let (ex, ey) = self.royal_tomb_entrance(id)?;
        [(0, -1), (1, 0), (0, 1), (-1, 0)]
            .iter()
            .map(|&(dx, dy)| (ex + dx, ey + dy))
            .filter(|&(x, y)| crate::figures::passable(&self.map, Travel::Land, x, y))
            .min_by_key(|&(x, y)| (x - from.0).abs() + (y - from.1).abs())
    }

    /// Puts man `fid` at his chamber `c` of tomb `id` (inside it, on the open-cut
    /// floor), or back out beside the entrance.
    fn move_tomb_worker(&mut self, fid: FigureId, id: BuildingId, c: Option<usize>) {
        let Some(b) = self.buildings.get(id) else { return };
        let Some(l) = layout(b.kind) else { return };
        let Some(f) = self.figures.get(fid) else { return };
        let at = match c.and_then(|c| l.chambers.get(c)) {
            Some(ch) => Some((b.x + ch.x + (ch.size - 1) / 2, b.y + ch.y + (ch.size - 1) / 2)),
            None => self.royal_tomb_access(id, (f.x, f.y)),
        };
        let (Some((x, y)), Some(f)) = (at, self.figures.get_mut(fid)) else { return };
        f.x = x;
        f.y = y;
        f.progress = 0;
        f.route.clear();
        f.destination = None;
    }

    /// A stonemason or artisan at a tomb: he walks to its entrance and in to his
    /// chamber, works its stage a point a tick, and goes home when it is done (a mason
    /// first takes on another chamber waiting to be dug, if there are lamps for him).
    pub(crate) fn update_tomb_worker(&mut self, fid: FigureId) {
        let Some(f) = self.figures.get(fid) else { return };
        let (act, target, figure, job) = (f.action, f.target, f.kind, f.amount);
        let chamber = (job > 0).then(|| (job - 1) as usize);
        let mine = chamber.is_some_and(|c| self.buildings.get(target).and_then(|b| b.monument.as_ref()).is_some_and(|m| m.chambers.get(c).is_some_and(|s| s.worker == fid)));
        if act != 3 && !mine {
            self.royal_tomb_worker_home(fid);
            return;
        }
        match act {
            1 => {
                let map = &self.map;
                let f = self.figures.get_mut(fid).expect("present");
                match f.walk(map) {
                    Step::Moving => {}
                    Step::Arrived => {
                        f.action = 2;
                        self.move_tomb_worker(fid, target, chamber);
                    }
                    _ => self.royal_tomb_worker_home(fid),
                }
            }
            2 => {
                let c = chamber.expect("working");
                let Some(m) = self.buildings.get_mut(target).and_then(|b| b.monument.as_mut()) else { return };
                let s = &mut m.chambers[c];
                s.left = s.left.saturating_sub(1);
                let done = s.left == 0;
                if let Some(f) = self.figures.get_mut(fid) {
                    f.moving = !done;
                }
                if !done {
                    return;
                }
                self.finish_stage(target, c);
                let next = if figure == STONEMASON { self.royal_tomb_job(target, figure) } else { None };
                match next {
                    Some(n) => {
                        self.assign_tomb_job(target, n, fid);
                        self.move_tomb_worker(fid, target, Some(n));
                        self.refresh_royal_tomb(target);
                    }
                    None => self.royal_tomb_worker_home(fid),
                }
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

    /// A tomb worker leaves his chamber and walks home.
    fn royal_tomb_worker_home(&mut self, fid: FigureId) {
        let Some(f) = self.figures.get(fid) else { return };
        let (target, home, act) = (f.target, f.home, f.action);
        if act == 2 {
            self.move_tomb_worker(fid, target, None);
        }
        if let Some(m) = self.buildings.get_mut(target).and_then(|b| b.monument.as_mut()) {
            for s in m.chambers.iter_mut().filter(|s| s.worker == fid) {
                s.worker = 0;
            }
            m.craftsmen.retain(|c| c.1 != fid);
        }
        let road = self.buildings.get(home).and_then(|b| b.road);
        let map = &self.map;
        let f = self.figures.get_mut(fid).expect("present");
        f.action = 3;
        f.amount = 0;
        f.moving = false;
        match road {
            Some(r) if f.go_to(map, r) => {}
            _ => f.dead = true,
        }
    }

    /// Sends man `fid` (just spawned at `road`) to chamber `c` of tomb `id`.
    fn send_to_tomb(&mut self, fid: FigureId, home: BuildingId, id: BuildingId, c: usize, spot: (i32, i32)) {
        self.assign_tomb_job(id, c, fid);
        let map = &self.map;
        if let Some(f) = self.figures.get_mut(fid) {
            f.home = home;
            f.target = id;
            f.action = 1;
            if !f.go_to(map, spot) {
                f.dead = true;
            }
        }
        self.refresh_royal_tomb(id);
    }

    /// A stonemasons' guild sends its mason into a tomb chamber waiting to be dug.
    /// Returns whether it did.
    pub(crate) fn send_tomb_mason(&mut self, guild: BuildingId, road: (i32, i32), tomb: BuildingId) -> bool {
        let Some(c) = self.royal_tomb_job(tomb, STONEMASON) else { return false };
        let Some(spot) = self.royal_tomb_access(tomb, road) else { return false };
        let fid = self.figures.spawn(STONEMASON, road.0, road.1, Travel::Land);
        self.send_to_tomb(fid, guild, tomb, c, spot);
        if let Some(m) = self.buildings.get_mut(tomb).and_then(|b| b.monument.as_mut()) {
            m.craftsmen.push((STONEMASON, fid));
        }
        if let Some(g) = self.buildings.get_mut(guild) {
            g.walkers[0] = fid;
        }
        true
    }

    /// Tick 31: each Artisans' Guild with fewer artisans out than its staff allows
    /// (one per quarter staffed) sends one to a chamber waiting for plaster, taking
    /// 100 clay, or failing that to one waiting for paint, taking 100 paint.
    pub(crate) fn artisan_walkers(&mut self) {
        let needed = self.workers_needed(ARTISANS_GUILD).max(1);
        let guilds: Vec<BuildingId> = self.buildings.iter().filter(|b| b.kind == ARTISANS_GUILD).map(|b| b.id).collect();
        for g in guilds {
            let Some(gb) = self.buildings.get(g) else { continue };
            let Some(road) = gb.road else { continue };
            let cap = match gb.workers * 100 / needed {
                p if p >= 100 => 4,
                p if p >= 75 => 3,
                p if p >= 50 => 2,
                p if p >= 25 => 1,
                _ => 0,
            };
            let out = self.figures.iter().filter(|f| f.kind == TOMB_ARTISAN && f.home == g && !f.dead).count();
            if out >= cap {
                continue;
            }
            let stock = |r: u16| gb.stock.get(r as usize).copied().unwrap_or(0);
            let (clay, paint) = (stock(CLAY), stock(PAINT));
            let tombs: Vec<BuildingId> = self.buildings.iter().filter(|b| is_royal_tomb(b.kind) && b.monument.as_ref().is_some_and(|m| !m.finished)).map(|b| b.id).collect();
            let wants = |stage: u8| tombs.iter().find_map(|&t| self.royal_tomb_chamber_at(t, stage).map(|c| (t, c)));
            let job = if clay >= ARTISAN_LOAD && let Some(j) = wants(2) {
                Some((j, CLAY))
            } else if paint >= ARTISAN_LOAD && let Some(j) = wants(3) {
                Some((j, PAINT))
            } else {
                None
            };
            let Some(((tomb, c), r)) = job else { continue };
            let Some(spot) = self.royal_tomb_access(tomb, road) else { continue };
            if let Some(s) = self.buildings.get_mut(g).and_then(|b| b.stock.get_mut(r as usize)) {
                *s -= ARTISAN_LOAD;
            }
            let fid = self.figures.spawn(TOMB_ARTISAN, road.0, road.1, Travel::Land);
            if let Some(f) = self.figures.get_mut(fid) {
                f.cargo = r;
            }
            self.send_to_tomb(fid, g, tomb, c, spot);
        }
    }

    /// A tomb a work-camp laborer should fetch lamps for, and the storage yard to
    /// fetch them from: a tomb under 400 lamps with no one fetching them and no one in
    /// its first chamber, and the yard with lamps nearest to it.
    pub(crate) fn lamp_job(&self, from: (i32, i32)) -> Option<(BuildingId, BuildingId)> {
        if self.is_stockpiled(LAMPS) {
            return None;
        }
        let tomb = self
            .buildings
            .iter()
            .filter(|b| is_royal_tomb(b.kind))
            .filter(|b| b.monument.as_ref().is_some_and(|m| !m.finished && !m.lamp_run && m.lamps < LAMP_RUN && m.chambers.first().is_some_and(|c| c.worker == 0)))
            .min_by_key(|b| (b.x - from.0).abs() + (b.y - from.1).abs())?;
        let yard = self
            .buildings
            .iter()
            .filter(|y| y.kind == kind::STORAGE_YARD && y.road.is_some() && self.stored(y.id, LAMPS) > 0)
            .min_by_key(|y| (y.x - tomb.x).abs() + (y.y - tomb.y).abs())?;
        Some((tomb.id, yard.id))
    }

    /// Sends peasant `fid` (just spawned) for lamps: to the yard, then to the tomb.
    pub(crate) fn send_for_lamps(&mut self, fid: FigureId, tomb: BuildingId, yard: BuildingId) -> bool {
        let Some(road) = self.buildings.get(yard).and_then(|y| y.road) else { return false };
        let map = &self.map;
        let Some(f) = self.figures.get_mut(fid) else { return false };
        f.target = tomb;
        f.amount = yard as i32;
        f.action = 5;
        if !f.go_to(map, road) {
            return false;
        }
        if let Some(m) = self.buildings.get_mut(tomb).and_then(|b| b.monument.as_mut()) {
            m.lamp_run = true;
        }
        true
    }

    /// A laborer fetching lamps: at the yard he loads up to 400 and carries them to
    /// the tomb, whose stock they join; then he goes back to his camp.
    pub(crate) fn update_lamp_carrier(&mut self, fid: FigureId) {
        let map = &self.map;
        let Some(f) = self.figures.get_mut(fid) else { return };
        let (act, tomb, amount, home) = (f.action, f.target, f.amount, f.home);
        match f.walk(map) {
            Step::Moving => {}
            Step::Arrived if act == 5 => {
                let taken = self.take_stored(amount as BuildingId, LAMPS, LAMP_RUN);
                let from = self.figures.get(fid).map_or((0, 0), |f| (f.x, f.y));
                let spot = self.royal_tomb_access(tomb, from);
                let map = &self.map;
                let f = self.figures.get_mut(fid).expect("present");
                f.cargo = LAMPS;
                f.amount = taken;
                f.action = 6;
                if taken <= 0 || !spot.is_some_and(|s| f.go_to(map, s)) {
                    f.dead = true;
                    self.end_lamp_run(tomb);
                }
            }
            Step::Arrived => {
                if let Some(m) = self.buildings.get_mut(tomb).and_then(|b| b.monument.as_mut()) {
                    m.lamps += amount;
                    m.lamp_run = false;
                }
                self.refresh_royal_tomb(tomb);
                let back = self.buildings.get(home).and_then(|b| b.road);
                let map = &self.map;
                let f = self.figures.get_mut(fid).expect("present");
                f.cargo = 0;
                f.amount = 0;
                f.action = 2;
                match back {
                    Some(r) if f.go_to(map, r) => {}
                    _ => f.dead = true,
                }
            }
            _ => {
                f.dead = true;
                self.end_lamp_run(tomb);
            }
        }
    }

    fn end_lamp_run(&mut self, tomb: BuildingId) {
        if let Some(m) = self.buildings.get_mut(tomb).and_then(|b| b.monument.as_mut()) {
            m.lamp_run = false;
        }
    }

    /// Daily, for a tomb under way: men who never came are forgotten, as is a lamp run
    /// no one is on. When every chamber is painted the completion is announced; when
    /// the burial provisions are in too, the tomb is sealed. Returns whether it was.
    pub(crate) fn update_royal_tomb(&mut self, id: BuildingId) -> bool {
        let Some(m) = self.buildings.get(id).and_then(|b| b.monument.as_ref()) else { return false };
        let workers: Vec<FigureId> = m.chambers.iter().map(|c| c.worker).collect();
        let alive: Vec<bool> = workers.iter().map(|&w| w != 0 && self.figures.get(w).is_some_and(|f| !f.dead && f.target == id && f.action != 3)).collect();
        let running = self.figures.iter().any(|f| f.kind == crate::farms::PEASANT && f.target == id && matches!(f.action, 5 | 6) && !f.dead);
        let k = self.buildings.get(id).map_or(0, |b| b.kind);
        if let Some(m) = self.buildings.get_mut(id).and_then(|b| b.monument.as_mut()) {
            for (c, ok) in m.chambers.iter_mut().zip(alive) {
                if !ok {
                    c.worker = 0;
                }
            }
            m.lamp_run &= running;
        }
        let done = self.royal_tomb_percent(id) == 100;
        let announce = done && self.buildings.get(id).and_then(|b| b.monument.as_ref()).is_some_and(|m| !m.announced);
        if announce {
            if let Some(m) = self.buildings.get_mut(id).and_then(|b| b.monument.as_mut()) {
                m.announced = true;
            }
            let (x, y) = self.buildings.get(id).map_or((0, 0), |b| (b.x, b.y));
            let name = phrase(k);
            self.post_event_text(crate::scenario_events::EventText {
                title: format!("{name}_congratulations_title"),
                body: format!("{name}_congratulations"),
                template: 131,
                ..Default::default()
            });
            if let Some(n) = self.notices.log.last_mut() {
                n.tile = Some((x, y));
            }
        }
        let idle = self.buildings.get(id).and_then(|b| b.monument.as_ref()).is_some_and(|m| m.chambers.iter().all(|c| c.worker == 0));
        let sealed = done && idle && self.burial_complete();
        if sealed && let Some(m) = self.buildings.get_mut(id).and_then(|b| b.monument.as_mut()) {
            m.finished = true;
        }
        self.refresh_royal_tomb(id);
        sealed
    }

    /// A tomb being removed: its bulk goes back to cliff and its entrance to clear
    /// land, both redrawn.
    pub(crate) fn remove_royal_tomb(&mut self, k: u16, x: i32, y: i32) {
        let Some(l) = layout(k) else { return };
        let (ex, ey) = (x + l.entrance.0, y + l.entrance.1);
        self.map.terrain.update(ex, ey, |t| t & !terrain::BUILDING);
        self.map.building.set(ex, ey, 0);
        self.map.set_single_image(ex, ey, 0);
        for yy in y..y + l.size.1 {
            for xx in x..x + l.size.0 {
                let image = crate::terrain_images::cliff_image(&self.map, &self.defs, xx, yy);
                self.map.set_single_image(xx, yy, image);
            }
        }
    }
}

const SMALL: Layout = Layout {
    size: (11, 20),
    entrance: (11, 4, 3),
    groups: Groups { entrance: 17, edge: 18, corner: 19 },
    rows: &[
        "CcccccccccD",
        "b.........d",
        "b.....###.d",
        "b..########",
        "b..########",
        "b..###....d",
        "b.###.....d",
        "b.###.....d",
        "b.###.....d",
        "b.###.....d",
        "b.###.....d",
        "b.###.....d",
        "b..###....d",
        "b..###....d",
        "b..###....d",
        "b..###....d",
        "b..######.d",
        "b..###.##.d",
        "b.........d",
        "BaaaaaaaaaA",
    ],
    chambers: &[
        Chamber { x: 9, y: 3, size: 2, dir: 3, lit: 20, work: 400, opens: &[1], furnished: false },
        Chamber { x: 6, y: 2, size: 3, dir: 0, lit: 5, work: 600, opens: &[2], furnished: false },
        Chamber { x: 3, y: 3, size: 3, dir: 3, lit: 7, work: 700, opens: &[3], furnished: false },
        Chamber { x: 2, y: 6, size: 3, dir: 3, lit: 5, work: 600, opens: &[4], furnished: false },
        Chamber { x: 2, y: 9, size: 3, dir: 3, lit: 5, work: 600, opens: &[5, 6], furnished: false },
        Chamber { x: 3, y: 12, size: 3, dir: 2, lit: 11, work: 600, opens: &[], furnished: true },
        Chamber { x: 3, y: 15, size: 3, dir: 2, lit: 13, work: 600, opens: &[7], furnished: true },
        Chamber { x: 6, y: 16, size: 1, dir: 1, lit: 1, work: 200, opens: &[8], furnished: false },
        Chamber { x: 7, y: 16, size: 2, dir: 1, lit: 9, work: 400, opens: &[], furnished: true },
    ],
};

const MEDIUM: Layout = Layout {
    size: (14, 16),
    entrance: (14, 11, 3),
    groups: Groups { entrance: 19, edge: 20, corner: 21 },
    rows: &[
        "CccccccccccccD",
        "..............",
        "....#####.....",
        "....#####.###.",
        "....#########.",
        "....#####.###.",
        "....#####.....",
        ".......#......",
        "......###.....",
        "......######..",
        "......########",
        "..###.########",
        "..###.###.....",
        "..#######.....",
        "..............",
        "BaaaaaaaaaaaaA",
    ],
    chambers: &[
        Chamber { x: 12, y: 10, size: 2, dir: 3, lit: 22, work: 400, opens: &[1], furnished: false },
        Chamber { x: 9, y: 9, size: 3, dir: 0, lit: 5, work: 600, opens: &[2, 5], furnished: false },
        Chamber { x: 6, y: 11, size: 3, dir: 3, lit: 13, work: 600, opens: &[3], furnished: true },
        Chamber { x: 5, y: 13, size: 1, dir: 1, lit: 1, work: 200, opens: &[4], furnished: false },
        Chamber { x: 2, y: 11, size: 3, dir: 3, lit: 7, work: 500, opens: &[], furnished: true },
        Chamber { x: 6, y: 8, size: 3, dir: 3, lit: 15, work: 600, opens: &[6], furnished: true },
        Chamber { x: 7, y: 7, size: 1, dir: 0, lit: 1, work: 200, opens: &[7], furnished: false },
        Chamber { x: 4, y: 2, size: 5, dir: 3, lit: 11, work: 800, opens: &[8], furnished: true },
        Chamber { x: 9, y: 4, size: 1, dir: 1, lit: 1, work: 200, opens: &[9], furnished: false },
        Chamber { x: 10, y: 3, size: 3, dir: 1, lit: 9, work: 500, opens: &[], furnished: true },
    ],
};

const LARGE: Layout = Layout {
    size: (17, 33),
    entrance: (7, -1, 2),
    groups: Groups { entrance: 25, edge: 26, corner: 27 },
    rows: &[
        "Cccccc##ccccccccD",
        "......##.........",
        ".....###.........",
        ".....###.........",
        ".....###.........",
        "......##.........",
        "......##.........",
        ".......#.........",
        "..###.###........",
        "..#######........",
        "..###.###........",
        "......###........",
        "......###........",
        "......###........",
        ".......##........",
        ".......##........",
        "......###........",
        "......###........",
        "......###........",
        "..############...",
        "..##.######.##...",
        ".....######......",
        ".....######.###..",
        "..##.##########..",
        "..#########.###..",
        ".........#.......",
        ".......#####.....",
        ".......#####.....",
        ".......#####.....",
        ".......#####.....",
        ".......#####.....",
        ".................",
        "BaaaaaaaaaaaaaaaA",
    ],
    chambers: &[
        Chamber { x: 6, y: 0, size: 2, dir: 2, lit: 28, work: 400, opens: &[1], furnished: false },
        Chamber { x: 5, y: 2, size: 3, dir: 3, lit: 5, work: 600, opens: &[2], furnished: false },
        Chamber { x: 6, y: 5, size: 2, dir: 2, lit: 15, work: 400, opens: &[3], furnished: false },
        Chamber { x: 7, y: 7, size: 1, dir: 0, lit: 1, work: 200, opens: &[4], furnished: false },
        Chamber { x: 6, y: 8, size: 3, dir: 3, lit: 7, work: 700, opens: &[5, 7], furnished: false },
        Chamber { x: 5, y: 9, size: 1, dir: 1, lit: 1, work: 200, opens: &[6], furnished: false },
        Chamber { x: 2, y: 8, size: 3, dir: 3, lit: 9, work: 700, opens: &[], furnished: false },
        Chamber { x: 6, y: 11, size: 3, dir: 3, lit: 5, work: 600, opens: &[8], furnished: false },
        Chamber { x: 7, y: 14, size: 2, dir: 2, lit: 3, work: 400, opens: &[9], furnished: false },
        Chamber { x: 6, y: 16, size: 3, dir: 3, lit: 5, work: 600, opens: &[10], furnished: false },
        Chamber { x: 5, y: 19, size: 6, dir: 2, lit: 21, work: 1000, opens: &[11], furnished: true },
        Chamber { x: 4, y: 19, size: 1, dir: 1, lit: 1, work: 200, opens: &[12, 15], furnished: false },
        Chamber { x: 2, y: 19, size: 2, dir: 3, lit: 13, work: 400, opens: &[], furnished: false },
        Chamber { x: 11, y: 19, size: 1, dir: 1, lit: 1, work: 200, opens: &[14, 17], furnished: false },
        Chamber { x: 12, y: 19, size: 2, dir: 1, lit: 11, work: 400, opens: &[], furnished: true },
        Chamber { x: 4, y: 24, size: 1, dir: 1, lit: 1, work: 200, opens: &[16, 13], furnished: false },
        Chamber { x: 2, y: 23, size: 2, dir: 3, lit: 11, work: 400, opens: &[], furnished: true },
        Chamber { x: 11, y: 23, size: 1, dir: 1, lit: 1, work: 200, opens: &[18, 19], furnished: false },
        Chamber { x: 12, y: 22, size: 3, dir: 1, lit: 17, work: 500, opens: &[], furnished: false },
        Chamber { x: 9, y: 25, size: 1, dir: 0, lit: 1, work: 200, opens: &[20], furnished: false },
        Chamber { x: 7, y: 26, size: 5, dir: 2, lit: 19, work: 500, opens: &[], furnished: true },
    ],
};

const GRAND: Layout = Layout {
    size: (29, 23),
    entrance: (29, 19, 3),
    groups: Groups { entrance: 31, edge: 32, corner: 33 },
    rows: &[
        "CcccccccccccccccccccccccccccD",
        ".............................",
        ".............###.............",
        ".............###.............",
        ".............###.............",
        ".....###.###..#..............",
        ".....###.###.###.............",
        ".....###.#######.............",
        "......#...#..###.............",
        "..#########..................",
        "..##.######.##...............",
        ".....#########...............",
        ".....######.....##...........",
        "..##.#########..##...........",
        "..#########.##.###...........",
        "........#......###...........",
        "....#####......###...........",
        "....#####.......#.......###..",
        "....##############.##.#######",
        "....#########################",
        "....##############..........d",
        "............................d",
        "BaaaaaaaaaaaaaaaaaaaaaaaaaaaA",
    ],
    chambers: &[
        Chamber { x: 27, y: 18, size: 2, dir: 3, lit: 34, work: 400, opens: &[1], furnished: false },
        Chamber { x: 24, y: 17, size: 3, dir: 0, lit: 5, work: 600, opens: &[2], furnished: false },
        Chamber { x: 22, y: 18, size: 2, dir: 3, lit: 13, work: 400, opens: &[3], furnished: false },
        Chamber { x: 21, y: 19, size: 1, dir: 1, lit: 1, work: 200, opens: &[4], furnished: false },
        Chamber { x: 19, y: 18, size: 2, dir: 3, lit: 13, work: 400, opens: &[5], furnished: false },
        Chamber { x: 18, y: 19, size: 1, dir: 1, lit: 1, work: 200, opens: &[6], furnished: false },
        Chamber { x: 15, y: 18, size: 3, dir: 0, lit: 7, work: 700, opens: &[7, 10], furnished: false },
        Chamber { x: 16, y: 17, size: 1, dir: 0, lit: 1, work: 200, opens: &[8], furnished: false },
        Chamber { x: 15, y: 14, size: 3, dir: 0, lit: 23, work: 700, opens: &[9], furnished: false },
        Chamber { x: 16, y: 12, size: 2, dir: 0, lit: 9, work: 400, opens: &[], furnished: false },
        Chamber { x: 12, y: 18, size: 3, dir: 0, lit: 5, work: 600, opens: &[11], furnished: false },
        Chamber { x: 9, y: 18, size: 3, dir: 0, lit: 5, work: 600, opens: &[12], furnished: false },
        Chamber { x: 4, y: 16, size: 5, dir: 3, lit: 27, work: 800, opens: &[13], furnished: true },
        Chamber { x: 8, y: 15, size: 1, dir: 0, lit: 1, work: 200, opens: &[14], furnished: false },
        Chamber { x: 5, y: 9, size: 6, dir: 0, lit: 25, work: 1000, opens: &[15], furnished: true },
        Chamber { x: 4, y: 14, size: 1, dir: 1, lit: 1, work: 200, opens: &[16, 17], furnished: false },
        Chamber { x: 2, y: 13, size: 2, dir: 3, lit: 9, work: 400, opens: &[], furnished: true },
        Chamber { x: 11, y: 13, size: 1, dir: 1, lit: 1, work: 200, opens: &[18, 19], furnished: false },
        Chamber { x: 12, y: 13, size: 2, dir: 1, lit: 9, work: 400, opens: &[], furnished: true },
        Chamber { x: 11, y: 11, size: 1, dir: 1, lit: 1, work: 200, opens: &[20, 21], furnished: false },
        Chamber { x: 12, y: 10, size: 2, dir: 1, lit: 11, work: 400, opens: &[], furnished: false },
        Chamber { x: 4, y: 9, size: 1, dir: 1, lit: 1, work: 200, opens: &[22, 23], furnished: false },
        Chamber { x: 2, y: 9, size: 2, dir: 3, lit: 11, work: 400, opens: &[], furnished: false },
        Chamber { x: 6, y: 8, size: 1, dir: 0, lit: 1, work: 200, opens: &[24, 25], furnished: false },
        Chamber { x: 5, y: 5, size: 3, dir: 0, lit: 19, work: 500, opens: &[], furnished: false },
        Chamber { x: 10, y: 8, size: 1, dir: 0, lit: 1, work: 200, opens: &[26], furnished: false },
        Chamber { x: 9, y: 5, size: 3, dir: 0, lit: 21, work: 500, opens: &[27], furnished: false },
        Chamber { x: 12, y: 7, size: 1, dir: 1, lit: 1, work: 200, opens: &[28], furnished: false },
        Chamber { x: 13, y: 6, size: 3, dir: 1, lit: 15, work: 500, opens: &[29], furnished: false },
        Chamber { x: 14, y: 5, size: 1, dir: 0, lit: 1, work: 200, opens: &[30], furnished: false },
        Chamber { x: 13, y: 2, size: 3, dir: 0, lit: 17, work: 500, opens: &[], furnished: false },
    ],
};

#[cfg(test)]
mod tests {
    use super::*;

    /// Each layout's chambers cover exactly its `#` tiles, without overlapping, and
    /// its entrance lies just outside the bulk, beside the first chamber.
    #[test]
    fn layouts_are_consistent() {
        for k in [SMALL_ROYAL_TOMB, MEDIUM_ROYAL_TOMB, LARGE_ROYAL_TOMB, GRAND_ROYAL_TOMB] {
            let l = layout(k).expect("tomb");
            let (w, h) = l.size;
            assert_eq!(l.rows.len() as i32, h, "{k}");
            assert!(l.rows.iter().all(|r| r.len() as i32 == w), "{k}");
            let mut covered = vec![0; (w * h) as usize];
            for c in l.chambers {
                for y in c.y..c.y + c.size {
                    for x in c.x..c.x + c.size {
                        covered[(y * w + x) as usize] += 1;
                    }
                }
                assert!(c.opens.iter().all(|&o| o < l.chambers.len()), "{k}");
            }
            for (y, row) in l.rows.iter().enumerate() {
                for (x, ch) in row.bytes().enumerate() {
                    assert_eq!(covered[y * w as usize + x], (ch == b'#') as i32, "{k} at {x},{y}");
                }
            }
            let (ex, ey, _) = l.entrance;
            assert!(!(0..w).contains(&ex) || !(0..h).contains(&ey), "{k}");
            let c0 = &l.chambers[0];
            let near = |v: i32, lo: i32, n: i32| (lo - 1..=lo + n).contains(&v);
            assert!(near(ex, c0.x, c0.size) && near(ey, c0.y, c0.size), "{k}");
        }
    }
}
