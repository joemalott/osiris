//! The Mission Editor ("Kingdom builder"): the original's editor for scenario maps.
//! It opens a `.map`, or makes a new one from the map open (Default.map to begin
//! with), paints its terrain, sets the points where people, rivers, invaders and
//! animals come in, edits the scenario's options, and writes the result back as an
//! original-format `.map`. The facts it follows are in notes/editor.md.
//!
//! The editor keeps the file it opened as its template: saving writes the grids, the
//! scenario's info and the Kingdom map (kingdom.rs) into a copy of it, so the chunks
//! it doesn't edit yet (the events) are kept as they were.

pub mod kingdom;
pub mod options;
pub mod script;
pub mod terrain;
pub mod view;

use anyhow::{Context, Result};
use osiris_formats::chunks::{ChunkFile, GRID_SIZE, GRID_TILES, Layout};
use osiris_formats::scenario::TilePoint;
use osiris_formats::{Scenario, TextTable};
use osiris_sim::Defs;
use osiris_sim::map::{Map, terrain as bits};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use terrain::{Diamond, Paint};

/// The map the editor starts from, as the Mission Editor Guide says to: a large
/// blank play area.
pub const DEFAULT_MAP: &str = "Default.map";

/// The sizes New Map offers (text group 33: Tiny, Small, Medium, Large, Huge,
/// Enormous): width and height in tiles (the table at 0x5e2a64). Each is centred in
/// the 228x228 grid.
pub const MAP_SIZES: [i32; 6] = [56, 84, 112, 140, 170, 226];

/// Where the scenario's info keeps the monuments' era.
const ERA_BYTE: usize = 996;

/// Osiris's own terrain bits, which no map of the original holds.
const OSIRIS_BITS: u32 = bits::BRIDGE | bits::WALKABLE_BUILDING;

/// How many strokes Ctrl+Z (Cmd+Z on macOS) can step back through.
const UNDO_LIMIT: usize = 20;

/// One undo step: the map and the point lists as they stood before a stroke, a
/// road, a point placed, or an explicit Refresh Map (not before an Options change,
/// which undo leaves alone).
struct Snapshot {
    map: Map,
    entry: TilePoint,
    exit: TilePoint,
    river_in: TilePoint,
    river_out: TilePoint,
    invasion_land: Vec<TilePoint>,
    invasion_sea: Vec<TilePoint>,
    fishing: Vec<TilePoint>,
    predator: Vec<TilePoint>,
    prey: Vec<TilePoint>,
    disembark: Vec<TilePoint>,
}

impl Snapshot {
    fn capture(e: &Editor) -> Self {
        let i = &e.scenario.info;
        Self {
            map: e.map.clone(),
            entry: i.entry_point,
            exit: i.exit_point,
            river_in: i.river_entry_point,
            river_out: i.river_exit_point,
            invasion_land: i.invasion_points_land.clone(),
            invasion_sea: i.invasion_points_sea.clone(),
            fishing: i.fishing_points.clone(),
            predator: i.predator_herd_points.clone(),
            prey: i.prey_herd_points.clone(),
            disembark: i.disembark_points.clone(),
        }
    }

    /// Puts the map and points back as they were; leaves everything else (the
    /// scenario's options) as it stands now.
    fn restore(self, e: &mut Editor) {
        e.map = self.map;
        let i = &mut e.scenario.info;
        i.entry_point = self.entry;
        i.exit_point = self.exit;
        i.river_entry_point = self.river_in;
        i.river_exit_point = self.river_out;
        i.invasion_points_land = self.invasion_land;
        i.invasion_points_sea = self.invasion_sea;
        i.fishing_points = self.fishing;
        i.predator_herd_points = self.predator;
        i.prey_herd_points = self.prey;
        i.disembark_points = self.disembark;
        e.redraw();
        e.dirty = true;
    }
}

/// A point the editor places, and which slot of its list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Point {
    Entry,
    Exit,
    /// Invasion points 0-7 by land, 8-15 by sea.
    Invasion(u8),
    RiverIn,
    RiverOut,
    Fishing(u8),
    /// Where the climate's beasts ("killers") start.
    Predator(u8),
    /// Where hunted game starts.
    Prey(u8),
    /// Where enemy transports land their soldiers.
    Disembark(u8),
}

/// What a click on the map does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    None,
    Paint(Paint),
    /// Roads are laid like the city's, dragged from one tile to another.
    Road,
    Point(Point),
}

impl Tool {
    /// The tool's name in the black box under the map (text group 49).
    pub fn name_id(self) -> Option<usize> {
        Some(match self {
            Tool::None => return None,
            Tool::Paint(Paint::Grass) => 0,
            Tool::Paint(Paint::Trees) => 1,
            Tool::Paint(Paint::Water) => 2,
            Tool::Paint(Paint::Rock(k)) if k & bits::CLIFF != 0 => 33,
            Tool::Paint(Paint::Rock(_)) => 5,
            Tool::Paint(Paint::Meadow) => 6,
            Tool::Paint(Paint::Road) | Tool::Road => 10,
            Tool::Paint(Paint::Floodplain) => 26,
            Tool::Paint(Paint::Marshland) => 29,
            Tool::Paint(Paint::Dunes) => 30,
            Tool::Point(Point::Entry) => 15,
            Tool::Point(Point::Exit) => 16,
            Tool::Point(Point::Invasion(_)) => 13,
            Tool::Point(Point::RiverIn) => 18,
            Tool::Point(Point::RiverOut) => 19,
            Tool::Point(Point::Fishing(_)) => 24,
            Tool::Point(Point::Predator(_)) => 25,
            Tool::Point(Point::Prey(_)) => 28,
            Tool::Point(Point::Disembark(_)) => 31,
        })
    }
}

/// What the editor asks the app to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    /// Back to the main menu.
    Exit,
    /// The list of maps, to open another.
    Open,
    /// Play the map as it stands (saved at this path first).
    Play(PathBuf),
}

pub struct Editor {
    pub defs: Arc<Defs>,
    pub text: Arc<TextTable>,
    /// The file the scenario came from, whose chunks saving keeps.
    template: ChunkFile,
    /// The scenario as it will be saved: its info, and its 228x228 grids for the
    /// tiles outside the map.
    pub scenario: Scenario,
    /// The map being edited.
    pub map: Map,
    /// Where Save writes; `None` until the map has a name of the player's.
    pub path: Option<PathBuf>,
    /// The map's name, shown at the top of the screen.
    pub name: String,
    /// Where the player's own maps go (the user folder's `maps`).
    pub maps_dir: PathBuf,
    /// The game's folder (for Pharaoh2.emp, which the Kingdom map's Reset reads).
    pub data: PathBuf,
    pub tool: Tool,
    /// The brush, 0 (one tile) to 4.
    pub brush: u8,
    /// Changes not yet saved.
    pub dirty: bool,
    /// The era whose monuments the scenario offers (1 Pyramids, 2 Valley of the
    /// Kings, 3 Alexandria, 4 Abu Simbel): byte 996 of the scenario's info, which
    /// `ScenarioInfo` doesn't keep.
    pub era: u8,
    /// Ctrl+Z history: the state before each of the last `UNDO_LIMIT` strokes.
    history: Vec<Snapshot>,
    pub view: view::View,
    pub request: Option<Request>,
}

/// Grid offset of map tile `(x, y)` in a `width x height` map centred in the grid.
fn start_offset(width: i32, height: i32) -> i32 {
    (GRID_SIZE as i32 - height) / 2 * GRID_SIZE as i32 + (GRID_SIZE as i32 - width) / 2
}

impl Editor {
    /// Opens the map at `path` for editing.
    pub fn open(path: &Path, defs: Arc<Defs>, text: Arc<TextTable>, maps_dir: PathBuf) -> Result<Self> {
        let bytes = std::fs::read(path).with_context(|| path.display().to_string())?;
        let template = ChunkFile::parse(&bytes, Layout::Map)?;
        let mut scenario = Scenario::from_chunks(&template)?;
        scenario.fix_image_ids(0);
        let name = path.file_stem().map_or_else(String::new, |s| s.to_string_lossy().into_owned());
        // The shipped maps are kept as they are: saving one asks for a name of its own.
        let own = path.parent().is_some_and(|p| p == maps_dir);
        let map = Map::from_scenario(&scenario);
        let byte = |at: usize| template.get("scenario_info").and_then(|d| d.get(at)).copied().unwrap_or(0);
        let era = byte(ERA_BYTE);
        let mut e = Self {
            defs,
            text,
            template,
            scenario,
            map,
            path: own.then(|| path.to_owned()),
            name,
            maps_dir,
            // A shipped map's folder is the game's Maps; the app sets it for others.
            data: path.parent().and_then(Path::parent).map(Path::to_path_buf).unwrap_or_default(),
            tool: Tool::None,
            brush: 2,
            dirty: false,
            era,
            history: Vec::new(),
            view: view::View::default(),
            request: None,
        };
        e.redraw();
        Ok(e)
    }

    /// New Map (FUN_004de430): the open scenario keeps its settings, Kingdom and
    /// events, but its map becomes `MAP_SIZES[size]` tiles of plain land, with fresh
    /// random ground, no points set, and outside the map's diamond the terrain the
    /// grid around a map holds.
    pub fn new_map(&mut self, size: usize) {
        let n = MAP_SIZES[size.min(MAP_SIZES.len() - 1)];
        let s = &mut self.scenario;
        let info = &mut s.info;
        info.width = n;
        info.height = n;
        info.start_offset = start_offset(n, n);
        info.border_size = GRID_SIZE as i32 - n;
        let none = TilePoint { x: -1, y: -1 };
        for p in [&mut info.earthquake_point, &mut info.entry_point, &mut info.exit_point, &mut info.river_entry_point, &mut info.river_exit_point] {
            *p = none;
        }
        for list in [&mut info.invasion_points_land, &mut info.invasion_points_sea, &mut info.fishing_points, &mut info.predator_herd_points, &mut info.prey_herd_points, &mut info.disembark_points] {
            list.iter_mut().for_each(|p| *p = none);
        }
        let diamond = Diamond::new(n, n);
        let x0 = (GRID_SIZE as i32 - n) / 2;
        let y0 = (GRID_SIZE as i32 - n) / 2;
        let shown = |i: usize| {
            let (gx, gy) = ((i % GRID_SIZE) as i32, (i / GRID_SIZE) as i32);
            diamond.inside(gx - x0, gy - y0) && (x0..x0 + n).contains(&gx) && (y0..y0 + n).contains(&gy)
        };
        s.images = vec![0; GRID_TILES];
        s.edges = vec![0; GRID_TILES];
        s.terrain = (0..GRID_TILES).map(|i| if shown(i) { 0 } else { terrain::OUTSIDE }).collect();
        s.bitfields = vec![0; GRID_TILES];
        s.elevation = vec![0; GRID_TILES];
        s.soil_fertility = vec![0; GRID_TILES];
        // Grown: every shipped map holds 255 on every tile (trees and reeds fully
        // grown).
        s.vegetation_growth = vec![255; GRID_TILES];
        s.moisture = vec![0; GRID_TILES];
        // Fresh random ground from the game's generator (FUN_0046fc80).
        let mut rng = osiris_sim::rng::Rng::from_seed(s.random_iv[0], s.random_iv[1]);
        s.random = (0..GRID_TILES)
            .map(|_| {
                rng.next();
                rng.short() as u8
            })
            .collect();
        s.camera = [0, 0];
        self.map = Map::from_scenario(&self.scenario);
        self.view = view::View::default();
        self.dirty = true;
        self.history.clear();
        self.refresh_map();
    }

    /// Everything the terrain decides worked out again, and the terrain images
    /// redrawn: Refresh Map (Alt+Z, FUN_00486e70) and what follows every stroke.
    pub fn refresh_map(&mut self) {
        terrain::settle_all(&mut self.map);
        terrain::refresh_grass(&mut self.map, self.scenario.info.climate);
        terrain::refresh_deep_water(&mut self.map);
        self.redraw();
    }

    /// Remembers the map and its points as they are now, so Ctrl+Z can put them
    /// back: called before a stroke begins (mouse down), a road is dragged, or an
    /// explicit Refresh Map, never for an Options change. Oldest steps past
    /// `UNDO_LIMIT` are dropped.
    fn push_undo(&mut self) {
        self.history.push(Snapshot::capture(self));
        if self.history.len() > UNDO_LIMIT {
            self.history.remove(0);
        }
    }

    /// Ctrl+Z (Cmd+Z on macOS): undoes the last stroke, road, point or Refresh Map.
    pub fn undo(&mut self) {
        if let Some(s) = self.history.pop() {
            s.restore(self);
        }
    }

    /// Redraws every terrain image as the game will when the map starts (the
    /// original's pass run on a copy, so the terrain itself is saved as painted: the
    /// game marks the floodplain's banks as grassland for itself).
    pub fn redraw(&mut self) {
        let mut shown = self.map.clone();
        osiris_sim::terrain_images::rebuild(&mut shown, &self.defs);
        self.map.images = shown.images;
        self.map.edges = shown.edges;
        self.map.bitfields = shown.bitfields;
        self.view.map_changed();
    }

    /// The tool at tile `(x, y)`, as a click or a drag over it: a brush paints its
    /// tiles (and keeps the floodplain whole), a point tool sets its point where the
    /// original allows it. Returns whether anything changed.
    pub fn apply(&mut self, x: i32, y: i32) -> bool {
        match self.tool {
            Tool::None | Tool::Road => false,
            Tool::Paint(paint) => self.paint(paint, x, y),
            Tool::Point(p) => self.place(p, x, y),
        }
    }

    fn paint(&mut self, paint: Paint, x: i32, y: i32) -> bool {
        let mut changed = false;
        let mut wet = false;
        for (dx, dy) in terrain::brush(self.brush) {
            let (tx, ty) = (x + dx, y + dy);
            if !self.map.contains(tx, ty) || !self.shown(tx, ty) {
                continue;
            }
            let before = self.map.terrain.at_or(tx, ty, 0);
            let painted = terrain::paint_tile(&mut self.map, paint, tx, ty);
            let settled = terrain::settle(&mut self.map, tx, ty);
            let after = self.map.terrain.at_or(tx, ty, 0);
            wet |= settled || (before ^ after) & (bits::MARSHLAND | bits::FLOODPLAIN | bits::WATER) != 0;
            changed |= painted || settled;
        }
        if paint == Paint::Meadow {
            let r = self.brush as i32 + 3;
            terrain::refresh_meadow_soil(&mut self.map, x - r, y - r, x + r, y + r);
        }
        if changed {
            self.dirty = true;
            if wet {
                terrain::refresh_grass(&mut self.map, self.scenario.info.climate);
                terrain::refresh_deep_water(&mut self.map);
            }
            self.redraw();
        }
        changed
    }

    /// A road from `a` to `b`, along x first and then y, on land a road may take.
    pub fn road(&mut self, a: (i32, i32), b: (i32, i32)) -> bool {
        let mut tiles = Vec::new();
        let step = |from: i32, to: i32| if to >= from { 1 } else { -1 };
        let mut x = a.0;
        while x != b.0 {
            tiles.push((x, a.1));
            x += step(a.0, b.0);
        }
        let mut y = a.1;
        while y != b.1 {
            tiles.push((b.0, y));
            y += step(a.1, b.1);
        }
        tiles.push(b);
        let blocked = bits::TREE | bits::ROCK | bits::WATER | bits::DUNE | bits::MARSHLAND | bits::ELEVATION | bits::ACCESS_RAMP;
        let mut changed = false;
        for (x, y) in tiles {
            if self.map.contains(x, y) && self.shown(x, y) && self.map.terrain.at_or(x, y, 0) & blocked == 0 {
                changed |= terrain::paint_tile(&mut self.map, Paint::Road, x, y);
                terrain::settle(&mut self.map, x, y);
            }
        }
        if changed {
            self.dirty = true;
            self.redraw();
        }
        changed
    }

    fn shown(&self, x: i32, y: i32) -> bool {
        Diamond::new(self.map.width, self.map.height).inside(x, y)
    }

    /// Whether point `p` may go on tile `(x, y)`, by the original's rules
    /// (FUN_00478440): people and land invaders come in on open land near the map's
    /// edge, ships on water there; a river's ends lie on its water at the edge;
    /// fishing grounds on water; each climate's animals on the ground they live on;
    /// enemy transports land at the edge of deep water.
    pub fn allowed(&self, p: Point, x: i32, y: i32) -> bool {
        if !self.map.contains(x, y) {
            return false;
        }
        let t = self.map.terrain.at_or(x, y, 0);
        let open = t & 0xeefe_d73f == 0;
        let water = t & bits::WATER != 0;
        let edge = Diamond::new(self.map.width, self.map.height).near_edge(x, y, 4);
        let around = |dx: i32, dy: i32| self.map.terrain_around(x + dx, y + dy, terrain::OUTSIDE);
        match p {
            Point::Entry | Point::Exit => open && edge,
            Point::Invasion(i) if i < 8 => open && edge,
            Point::Invasion(_) => water && edge,
            Point::RiverIn | Point::RiverOut => edge && [(0, -1), (1, 0), (0, 1), (-1, 0)].iter().all(|&(dx, dy)| around(dx, dy) & bits::WATER != 0),
            Point::Fishing(_) => water,
            Point::Predator(_) => herd_ground(t, predator_ground(self.scenario.info.climate, self.scenario.info.alt_predator_type != 0)),
            Point::Prey(_) => herd_ground(t, prey_ground(self.scenario.info.climate)),
            Point::Disembark(_) => water && t & bits::DEEPWATER != 0 && osiris_sim::map::NEIGHBOURS.iter().any(|&(dx, dy)| around(dx, dy) & bits::DEEPWATER == 0),
        }
    }

    fn place(&mut self, p: Point, x: i32, y: i32) -> bool {
        if !self.allowed(p, x, y) {
            return false;
        }
        let at = TilePoint { x, y };
        let slot = self.point_mut(p);
        if *slot == at {
            return false;
        }
        *slot = at;
        self.dirty = true;
        true
    }

    /// The scenario's field for point `p`.
    pub fn point(&self, p: Point) -> TilePoint {
        let i = &self.scenario.info;
        let none = TilePoint { x: -1, y: -1 };
        let get = |v: &Vec<TilePoint>, n: u8| v.get(n as usize).copied().unwrap_or(none);
        match p {
            Point::Entry => i.entry_point,
            Point::Exit => i.exit_point,
            Point::Invasion(n) if n < 8 => get(&i.invasion_points_land, n),
            Point::Invasion(n) => get(&i.invasion_points_sea, n - 8),
            Point::RiverIn => i.river_entry_point,
            Point::RiverOut => i.river_exit_point,
            Point::Fishing(n) => get(&i.fishing_points, n),
            Point::Predator(n) => get(&i.predator_herd_points, n),
            Point::Prey(n) => get(&i.prey_herd_points, n),
            Point::Disembark(n) => get(&i.disembark_points, n),
        }
    }

    fn point_mut(&mut self, p: Point) -> &mut TilePoint {
        let i = &mut self.scenario.info;
        fn slot(v: &mut Vec<TilePoint>, n: u8, len: usize) -> &mut TilePoint {
            if v.len() < len {
                v.resize(len, TilePoint { x: -1, y: -1 });
            }
            &mut v[n as usize]
        }
        match p {
            Point::Entry => &mut i.entry_point,
            Point::Exit => &mut i.exit_point,
            Point::Invasion(n) if n < 8 => slot(&mut i.invasion_points_land, n, 8),
            Point::Invasion(n) => slot(&mut i.invasion_points_sea, n - 8, 8),
            Point::RiverIn => &mut i.river_entry_point,
            Point::RiverOut => &mut i.river_exit_point,
            Point::Fishing(n) => slot(&mut i.fishing_points, n, 8),
            Point::Predator(n) => slot(&mut i.predator_herd_points, n, 4),
            Point::Prey(n) => slot(&mut i.prey_herd_points, n, 4),
            Point::Disembark(n) => slot(&mut i.disembark_points, n, 3),
        }
    }

    /// Every point the map has, for drawing their flags.
    pub fn points(&self) -> Vec<(Point, TilePoint)> {
        let mut all = vec![Point::Entry, Point::Exit, Point::RiverIn, Point::RiverOut];
        all.extend((0..16).map(Point::Invasion));
        all.extend((0..8).map(Point::Fishing));
        all.extend((0..4).map(Point::Predator));
        all.extend((0..4).map(Point::Prey));
        all.extend((0..3).map(Point::Disembark));
        all.into_iter().map(|p| (p, self.point(p))).filter(|(_, t)| t.is_valid()).collect()
    }

    /// The Resets menu (text group 10): clears the killer, fishing, invasion,
    /// disembark or prey points.
    pub fn clear_points(&mut self, which: usize) {
        let none = TilePoint { x: -1, y: -1 };
        let i = &mut self.scenario.info;
        let list = match which {
            1 => &mut i.predator_herd_points,
            2 => &mut i.fishing_points,
            3 => {
                i.invasion_points_land.iter_mut().for_each(|p| *p = none);
                &mut i.invasion_points_sea
            }
            4 => &mut i.disembark_points,
            _ => &mut i.prey_herd_points,
        };
        list.iter_mut().for_each(|p| *p = none);
        self.dirty = true;
    }

    /// The scenario with the map written into its grids.
    pub fn scenario_to_save(&self) -> Scenario {
        let mut s = self.scenario.clone();
        let m = &self.map;
        for y in 0..m.height {
            for x in 0..m.width {
                let Some(o) = s.offset(x, y) else { continue };
                s.images[o] = m.images.at_or(x, y, 0);
                s.edges[o] = m.edges.at_or(x, y, 0);
                s.terrain[o] = m.terrain.at_or(x, y, 0) & !OSIRIS_BITS;
                s.bitfields[o] = m.bitfields.at_or(x, y, 0);
                s.random[o] = m.random.at_or(x, y, 0);
                s.elevation[o] = m.elevation.at_or(x, y, 0);
                s.soil_fertility[o] = m.fertility.at_or(x, y, 0);
                s.vegetation_growth[o] = m.vegetation.at_or(x, y, 0);
                s.moisture[o] = m.moisture.at_or(x, y, 0);
            }
        }
        s.camera = self.view.camera_to_save(m, &s);
        s
    }

    /// The map as an original-format `.map` file.
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        let mut file = self.scenario_to_save().to_chunk_file(self.template.clone())?;
        if let Some(info) = file.get_mut("scenario_info") {
            info[ERA_BYTE] = self.era;
        }
        Ok(file.to_bytes())
    }

    /// Saves the map to `path`, which becomes where Save writes.
    pub fn save_as(&mut self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, self.to_bytes()?).with_context(|| path.display().to_string())?;
        self.path = Some(path.to_owned());
        self.name = path.file_stem().map_or_else(String::new, |s| s.to_string_lossy().into_owned());
        self.dirty = false;
        Ok(())
    }

    /// Where Save As puts a map named `name`: the player's maps folder.
    pub fn path_for(&self, name: &str) -> PathBuf {
        self.maps_dir.join(format!("{}.map", crate::sanitize(name)))
    }

    /// The map written where the test game reads it, which the app then plays.
    pub fn play(&mut self) -> Result<()> {
        let path = self.maps_dir.join("_playtest.map");
        std::fs::create_dir_all(&self.maps_dir)?;
        std::fs::write(&path, self.to_bytes()?)?;
        self.request = Some(Request::Play(path));
        Ok(())
    }

    /// What the black info box already lists in red: no entry point, no exit
    /// point, or the river's ends not both set. Play and Save warn before going
    /// ahead without them.
    pub fn missing_points(&self) -> Vec<&'static str> {
        let i = &self.scenario.info;
        let mut v = Vec::new();
        if !i.entry_point.is_valid() {
            v.push("entry point");
        }
        if !i.exit_point.is_valid() {
            v.push("exit point");
        }
        if !(i.river_entry_point.is_valid() && i.river_exit_point.is_valid()) {
            v.push("river points");
        }
        v
    }
}

/// The ground each climate's beast lives on (table 0x5e4cb4 by climate and the
/// scenario's choice): hippos and crocodiles in marsh or water, hyenas on dunes, the
/// others anywhere open.
fn predator_ground(climate: u8, alt: bool) -> u32 {
    match (climate, alt) {
        (0 | 1, false) => bits::MARSHLAND | bits::WATER,
        (2, false) => bits::DUNE,
        _ => 0,
    }
}

/// The ground each climate's game lives on (table 0x5e4c24): the humid north's birds
/// in marsh, antelope and ostriches anywhere open.
fn prey_ground(climate: u8) -> u32 {
    if climate == 0 { bits::MARSHLAND } else { 0 }
}

/// Whether a herd needing `ground` may start on terrain `t` (FUN_004ec410, as the
/// editor asks it): never on buildings, roads, rock and the like; on the ground it
/// needs if it needs any, else on land with none of the kinds it could need.
fn herd_ground(t: u32, ground: u32) -> bool {
    if t & 0x3038_d022 != 0 {
        return false;
    }
    let kind = t & 0x2635_1277;
    if ground == 0 && kind == 0 {
        return true;
    }
    kind & ground != 0
}

/// Every `.map` the editor offers: the game's own, then the player's.
pub fn list_maps(data: &Path, maps_dir: &Path) -> Vec<PathBuf> {
    let mut v = crate::list_files(&data.join("Maps"), "map");
    v.extend(crate::list_files(maps_dir, "map"));
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data() -> Option<PathBuf> {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../PharaohData");
        dir.is_dir().then_some(dir)
    }

    fn editor(data: &Path, map: &str) -> Editor {
        let lib = osiris_formats::ImageLibrary::open(&data.join("Data")).unwrap();
        let defs = Arc::new(Defs::load(&lib).unwrap());
        let text = Arc::new(TextTable::parse(&std::fs::read(data.join("Pharaoh_Text.eng")).unwrap()).unwrap());
        Editor::open(&data.join("Maps").join(map), defs, text, std::env::temp_dir().join("osiris-editor-test")).unwrap()
    }

    /// A map saved untouched reads back with the same terrain, info and chunks.
    #[test]
    fn untouched_map_saves_as_it_was() {
        let Some(data) = data() else { return };
        let e = editor(&data, "Warfare.map");
        let bytes = e.to_bytes().unwrap();
        let again = Scenario::from_chunks(&ChunkFile::parse(&bytes, Layout::Map).unwrap()).unwrap();
        let before = Scenario::load_map(&data.join("Maps/Warfare.map")).unwrap();
        assert_eq!(format!("{:?}", again.info), format!("{:?}", before.info));
        assert_eq!(again.terrain, before.terrain.iter().map(|t| t & !OSIRIS_BITS).collect::<Vec<_>>());
        assert_eq!(again.moisture, before.moisture);
        assert_eq!(format!("{:?}", again.empire), format!("{:?}", before.empire));
    }

    /// A new map of each size is its size, centred in the grid, and what is painted
    /// on it and the points set on it come back from the saved file.
    #[test]
    fn new_maps_save_and_load() {
        let Some(data) = data() else { return };
        let mut e = editor(&data, DEFAULT_MAP);
        for (size, &n) in MAP_SIZES.iter().enumerate() {
            e.new_map(size);
            assert_eq!((e.map.width, e.map.height), (n, n));
            assert_eq!(e.scenario.info.start_offset, start_offset(n, n));
        }
        e.new_map(1);
        e.tool = Tool::Paint(Paint::Water);
        e.brush = 4;
        for x in 10..70 {
            e.apply(x, 42);
        }
        assert!(e.map.terrain.at_or(40, 42, 0) & bits::WATER != 0);
        assert!(e.map.terrain.at_or(40, 50, 0) & bits::GROUNDWATER != 0, "grass beside the new water");
        let edge = (0..e.map.width).find(|&x| e.allowed(Point::Entry, x, 42)).expect("an edge tile");
        e.tool = Tool::Point(Point::Entry);
        assert!(e.apply(edge, 42));
        let bytes = e.to_bytes().unwrap();
        let s = Scenario::from_chunks(&ChunkFile::parse(&bytes, Layout::Map).unwrap()).unwrap();
        assert_eq!((s.info.width, s.info.entry_point.x, s.info.entry_point.y), (84, edge, 42));
        let m = Map::from_scenario(&s);
        assert!(m.terrain.at_or(40, 42, 0) & bits::WATER != 0);
        assert_eq!(m.images.at_or(40, 42, 0), e.map.images.at_or(40, 42, 0));
    }

    /// Every field the Options screens show, set away from its default, comes back
    /// unchanged from a save and reopen: description, starting conditions, climate,
    /// enemy and its toggle, gods and temple complexes, buildings allowed, every win
    /// criterion, era, monuments, burial provisions, flood plain settings and the
    /// picture.
    #[test]
    fn options_round_trip() {
        use osiris_formats::scenario::Goal;

        let Some(data) = data() else { return };
        let mut e = editor(&data, DEFAULT_MAP);
        e.era = 1; // Pyramids: monuments 1-3 below (kind 1) fit any era, but 1 in particular.
        let i = &mut e.scenario.info;
        i.subtitle = "Round-trip test map".to_owned();
        i.player_rank = 5;
        i.start_year = -1234;
        i.initial_funds = 54321;
        i.rescue_loan = 999;
        i.win.milestone_years = [11, 22, 33];
        i.debt_interest_rate = 12;
        i.current_pharaoh = 7;
        i.player_incarnation = 3;
        i.climate = 2;
        i.enemy_id = 5;
        i.player_faction = 1;
        i.gods = [1, 2, 0, 2, 1];
        i.gods_known = [true, true, false, true, true];
        if i.reserved.len() < 114 {
            i.reserved.resize(114, 0);
        }
        for (g, &known) in i.gods_known.iter().enumerate() {
            i.reserved[104 + g] = known as i16;
        }
        for id in [2, 10, 20, 30, 40] {
            i.reserved[id] = 1;
        }
        i.is_open_play = false;
        i.win.culture = Goal { enabled: true, value: 111 };
        i.win.prosperity = Goal { enabled: true, value: 222 };
        i.win.kingdom = Goal { enabled: true, value: 333 };
        i.win.housing_count = Goal { enabled: true, value: 5 };
        i.win.housing_level = Goal { enabled: true, value: 9 };
        i.win.time_limit = Goal { enabled: true, value: 44 };
        i.win.survival_time = Goal { enabled: true, value: 55 };
        i.win.population = Goal { enabled: true, value: 6000 };
        i.monuments = [1, 2, 3];
        if i.burial_provisions_required.len() < 36 {
            i.burial_provisions_required.resize(36, 0);
        }
        for (n, &r) in [1, 8, 10, 13, 15, 17, 18, 19, 20, 23, 24, 25, 26, 28, 30].iter().enumerate() {
            i.burial_provisions_required[r] = 100 + n as u32;
        }
        i.image_id = 5;
        let f = &mut e.scenario.floodplain_settings;
        if f.len() < 12 {
            f.resize(12, 0);
        }
        f[0..4].copy_from_slice(&210i32.to_le_bytes());
        f[4..8].copy_from_slice(&90i32.to_le_bytes());
        f[8..12].copy_from_slice(&60i32.to_le_bytes());

        let bytes = e.to_bytes().unwrap();
        let path = std::env::temp_dir().join(format!("osiris-options-roundtrip-{}.map", std::process::id()));
        std::fs::write(&path, &bytes).unwrap();
        let reopened = Editor::open(&path, e.defs.clone(), e.text.clone(), e.maps_dir.clone()).unwrap();
        std::fs::remove_file(&path).ok();

        assert_eq!(reopened.era, e.era);
        assert_eq!(format!("{:?}", reopened.scenario.info), format!("{:?}", e.scenario.info));
        assert_eq!(reopened.scenario.floodplain_settings, e.scenario.floodplain_settings);
    }
}
