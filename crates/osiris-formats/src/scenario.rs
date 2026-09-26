//! Scenario data common to maps, saves and campaign entries: the tile grids and the
//! 1592-byte `scenario_info` block.
//!
//! Grids are 228x228 and indexed by grid offset. The playable map is the
//! `width x height` rectangle starting at `start_offset`; map tile `(x, y)` lives at
//! `start_offset + y * 228 + x`.

use crate::Result;
use crate::bytes::{Reader, Writer};
use crate::chunks::{ChunkFile, GRID_SIZE, GRID_TILES};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TilePoint {
    pub x: i32,
    pub y: i32,
}

impl TilePoint {
    pub fn is_valid(&self) -> bool {
        self.x >= 0 && self.y >= 0
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Goal {
    pub enabled: bool,
    pub value: i32,
}

#[derive(Debug, Clone, Default)]
pub struct WinCriteria {
    pub culture: Goal,
    pub prosperity: Goal,
    pub monuments: Goal,
    pub kingdom: Goal,
    pub housing_count: Goal,
    pub housing_level: Goal,
    pub time_limit: Goal,
    pub survival_time: Goal,
    pub population: Goal,
    pub milestone_years: [i32; 3],
}

#[derive(Debug, Clone, Default)]
pub struct ScenarioInfo {
    pub start_year: i16,
    pub empire_id: i16,
    pub gods_known: [bool; 5],
    /// Each god's status: 0 unknown, 1 known (local), 2 patron.
    pub gods: [u8; 5],
    pub initial_funds: i32,
    pub enemy_id: i16,
    pub width: i32,
    pub height: i32,
    pub start_offset: i32,
    pub border_size: i32,
    pub subtitle: String,
    pub brief_description: String,
    pub image_id: i16,
    pub is_open_play: bool,
    pub player_rank: i16,
    pub predator_herd_points: Vec<TilePoint>,
    pub predator_herd_types: Vec<u16>,
    /// The scenario's second choice of predator for its climate (Cleopatra's "killer
    /// type"): asps rather than hippos, lions rather than crocodiles, scorpions
    /// rather than hyenas.
    pub alt_predator_type: u16,
    pub fishing_points: Vec<TilePoint>,
    pub invasion_points_land: Vec<TilePoint>,
    pub invasion_points_sea: Vec<TilePoint>,
    pub win: WinCriteria,
    pub earthquake_point: TilePoint,
    pub entry_point: TilePoint,
    pub exit_point: TilePoint,
    pub river_entry_point: TilePoint,
    pub river_exit_point: TilePoint,
    pub rescue_loan: i32,
    pub has_animals: bool,
    pub flotsam_enabled: bool,
    pub climate: u8,
    pub player_faction: u8,
    pub prey_herd_points: Vec<TilePoint>,
    /// Editor-reserved words; the original stores per-building "allowed" flags here.
    pub reserved: Vec<i16>,
    pub disembark_points: Vec<TilePoint>,
    pub debt_interest_rate: u32,
    /// The Kingdom feeds the city grain (bytes 740-743): houses need no food supply.
    pub kingdom_supplies_grain: bool,
    pub monuments: [u16; 3],
    pub burial_provisions_required: Vec<u32>,
    pub current_pharaoh: u32,
    pub player_incarnation: u32,
}

fn points_u16(r: &mut Reader, n: usize) -> Result<Vec<TilePoint>> {
    let xs: Vec<i32> = (0..n)
        .map(|_| r.u16().map(|v| v as i16 as i32))
        .collect::<Result<_>>()?;
    let ys: Vec<i32> = (0..n)
        .map(|_| r.u16().map(|v| v as i16 as i32))
        .collect::<Result<_>>()?;
    Ok(xs
        .into_iter()
        .zip(ys)
        .map(|(x, y)| TilePoint { x, y })
        .collect())
}

fn points_i32(r: &mut Reader, n: usize) -> Result<Vec<TilePoint>> {
    let xs: Vec<i32> = (0..n).map(|_| r.i32()).collect::<Result<_>>()?;
    let ys: Vec<i32> = (0..n).map(|_| r.i32()).collect::<Result<_>>()?;
    Ok(xs
        .into_iter()
        .zip(ys)
        .map(|(x, y)| TilePoint { x, y })
        .collect())
}

fn point(r: &mut Reader) -> Result<TilePoint> {
    Ok(TilePoint {
        x: r.i16()? as i32,
        y: r.i16()? as i32,
    })
}

/// Writes up to `n` values with `put`, stepping over the stored ones `values` lacks.
fn put_n<T: Copy>(w: &mut Writer, n: usize, size: usize, values: &[T], put: impl Fn(&mut Writer, T)) {
    for i in 0..n {
        match values.get(i) {
            Some(&v) => put(w, v),
            None => w.skip(size),
        }
    }
}

fn put_points_u16(w: &mut Writer, n: usize, points: &[TilePoint]) {
    put_n(w, n, 2, points, |w, p| w.i16(p.x as i16));
    put_n(w, n, 2, points, |w, p| w.i16(p.y as i16));
}

fn put_points_i32(w: &mut Writer, n: usize, points: &[TilePoint]) {
    put_n(w, n, 4, points, |w, p| w.i32(p.x));
    put_n(w, n, 4, points, |w, p| w.i32(p.y));
}

fn put_point(w: &mut Writer, p: TilePoint) {
    w.i16(p.x as i16);
    w.i16(p.y as i16);
}

impl ScenarioInfo {
    /// The gods a temple complex may be built to (Osiris, Ra, Ptah, Seth, Bast):
    /// words at bytes 1240-1249 of the scenario's info, in the reserved block.
    pub fn temple_complex_gods(&self) -> [bool; 5] {
        std::array::from_fn(|g| self.reserved.get(104 + g).is_some_and(|&w| w != 0))
    }

    pub fn parse(data: &[u8]) -> Result<Self> {
        let mut r = Reader::new(data, "scenario_info");
        let mut s = Self {
            start_year: r.i16()?,
            ..Default::default()
        };
        r.skip(2)?;
        s.empire_id = r.i16()?;
        r.skip(4)?;
        for i in 0..5 {
            s.gods[i] = r.u8()?;
            s.gods_known[i] = s.gods[i] != 0;
            r.skip(1)?;
        }
        r.skip(12)?;
        s.initial_funds = r.i32()?;
        s.enemy_id = r.i16()?;
        r.skip(6)?;
        s.width = r.i32()?;
        s.height = r.i32()?;
        s.start_offset = r.i32()?;
        s.border_size = r.i32()?;
        s.subtitle = r.cstr(64)?;
        s.brief_description = r.cstr(522)?;
        s.image_id = r.i16()?;
        s.is_open_play = r.i16()? != 0;
        s.player_rank = r.i16()?;
        s.predator_herd_points = points_u16(&mut r, 4)?;
        s.fishing_points = points_u16(&mut r, 8)?;
        s.alt_predator_type = r.u16()?;
        s.predator_herd_types = (0..4).map(|_| r.u16()).collect::<Result<_>>()?;
        r.skip(30)?;
        s.kingdom_supplies_grain = r.i32()? != 0;
        // One list of sixteen points, all the x's then all the y's: the eight land
        // points, then the eight sea points (the editor stores point n at x 744 + 2n,
        // y 776 + 2n).
        let mut invasion = points_u16(&mut r, 16)?;
        s.invasion_points_sea = invasion.split_off(8);
        s.invasion_points_land = invasion;
        r.skip(36)?;
        let w = &mut s.win;
        for g in [
            &mut w.culture,
            &mut w.prosperity,
            &mut w.monuments,
            &mut w.kingdom,
            &mut w.housing_count,
            &mut w.housing_level,
        ] {
            g.value = r.i32()?;
        }
        for g in [
            &mut w.culture,
            &mut w.prosperity,
            &mut w.monuments,
            &mut w.kingdom,
            &mut w.housing_count,
            &mut w.housing_level,
        ] {
            g.enabled = r.u8()? != 0;
        }
        r.skip(6)?;
        for g in [&mut w.time_limit, &mut w.survival_time, &mut w.population] {
            g.enabled = r.i32()? != 0;
            g.value = r.i32()?;
        }
        s.earthquake_point = point(&mut r)?;
        s.entry_point = point(&mut r)?;
        s.exit_point = point(&mut r)?;
        r.skip(32)?;
        s.river_entry_point = point(&mut r)?;
        s.river_exit_point = point(&mut r)?;
        s.rescue_loan = r.i32()?;
        for y in &mut s.win.milestone_years {
            *y = r.i32()?;
        }
        r.skip(10)?;
        s.has_animals = r.u8()? != 0;
        s.flotsam_enabled = r.u8()? != 0;
        s.climate = r.u8()?;
        r.skip(11)?;
        let _unknown = r.u8()?;
        s.player_faction = r.u8()?;
        r.skip(2)?;
        s.prey_herd_points = points_i32(&mut r, 4)?;
        s.reserved = (0..114).map(|_| r.i16()).collect::<Result<_>>()?;
        s.disembark_points = points_i32(&mut r, 3)?;
        s.debt_interest_rate = r.u32()?;
        for m in &mut s.monuments {
            *m = r.u16()?;
        }
        r.skip(2)?;
        s.burial_provisions_required = (0..36).map(|_| r.u32()).collect::<Result<_>>()?;
        r.skip(36 * 4)?; // dispatched
        s.current_pharaoh = r.u32()?;
        s.player_incarnation = r.u32()?;
        debug_assert_eq!(r.remaining(), 0);
        Ok(s)
    }

    /// Writes every field `parse` reads back into the `scenario_info` chunk bytes,
    /// leaving the bytes it skips untouched. Flags keep their stored value while it
    /// still reads the same; lists shorter than their field leave the remaining stored
    /// entries as they are. A god counts as known when `gods_known` is set, whatever
    /// `gods` says (a known god with status 0 is written as 1).
    pub fn write_into(&self, data: &mut [u8]) {
        assert_eq!(data.len(), 1592, "scenario_info is 1592 bytes");
        let mut w = Writer::new(data);
        w.i16(self.start_year);
        w.skip(2);
        w.i16(self.empire_id);
        w.skip(4);
        for i in 0..5 {
            let status = match (self.gods_known[i], self.gods[i]) {
                (false, _) => 0,
                (true, 0) => 1,
                (true, g) => g,
            };
            w.u8(status);
            w.skip(1);
        }
        w.skip(12);
        w.i32(self.initial_funds);
        w.i16(self.enemy_id);
        w.skip(6);
        w.i32(self.width);
        w.i32(self.height);
        w.i32(self.start_offset);
        w.i32(self.border_size);
        w.cstr(64, &self.subtitle);
        w.cstr(522, &self.brief_description);
        w.i16(self.image_id);
        w.flag16(self.is_open_play);
        w.i16(self.player_rank);
        put_points_u16(&mut w, 4, &self.predator_herd_points);
        put_points_u16(&mut w, 8, &self.fishing_points);
        w.u16(self.alt_predator_type);
        put_n(&mut w, 4, 2, &self.predator_herd_types, |w, t| w.u16(t));
        w.skip(30);
        w.flag32(self.kingdom_supplies_grain);
        let invasion: Vec<TilePoint> = (0..16)
            .map(|i| {
                let list = if i < 8 { &self.invasion_points_land } else { &self.invasion_points_sea };
                list.get(i % 8).copied().unwrap_or(TilePoint { x: -1, y: -1 })
            })
            .collect();
        put_points_u16(&mut w, 16, &invasion);
        w.skip(36);
        let g = &self.win;
        let six = [
            g.culture,
            g.prosperity,
            g.monuments,
            g.kingdom,
            g.housing_count,
            g.housing_level,
        ];
        for goal in six {
            w.i32(goal.value);
        }
        for goal in six {
            w.flag8(goal.enabled);
        }
        w.skip(6);
        for goal in [g.time_limit, g.survival_time, g.population] {
            w.flag32(goal.enabled);
            w.i32(goal.value);
        }
        put_point(&mut w, self.earthquake_point);
        put_point(&mut w, self.entry_point);
        put_point(&mut w, self.exit_point);
        w.skip(32);
        put_point(&mut w, self.river_entry_point);
        put_point(&mut w, self.river_exit_point);
        w.i32(self.rescue_loan);
        for y in g.milestone_years {
            w.i32(y);
        }
        w.skip(10);
        w.flag8(self.has_animals);
        w.flag8(self.flotsam_enabled);
        w.u8(self.climate);
        w.skip(11);
        w.skip(1); // unknown
        w.u8(self.player_faction);
        w.skip(2);
        put_points_i32(&mut w, 4, &self.prey_herd_points);
        put_n(&mut w, 114, 2, &self.reserved, |w, v| w.i16(v));
        put_points_i32(&mut w, 3, &self.disembark_points);
        w.u32(self.debt_interest_rate);
        for m in self.monuments {
            w.u16(m);
        }
        w.skip(2);
        put_n(&mut w, 36, 4, &self.burial_provisions_required, |w, v| w.u32(v));
        w.skip(36 * 4); // dispatched
        w.u32(self.current_pharaoh);
        w.u32(self.player_incarnation);
        debug_assert_eq!(w.pos(), 1592);
    }
}

/// Terrain flags of the 32-bit terrain grid.
pub mod terrain {
    pub const TREE: u32 = 0x1;
    pub const ROCK: u32 = 0x2;
    pub const WATER: u32 = 0x4;
    pub const BUILDING: u32 = 0x8;
    pub const SHRUB: u32 = 0x10;
    pub const GARDEN: u32 = 0x20;
    pub const ROAD: u32 = 0x40;
    pub const GROUNDWATER: u32 = 0x80;
    pub const CANAL: u32 = 0x100;
    pub const ELEVATION: u32 = 0x200;
    pub const ACCESS_RAMP: u32 = 0x400;
    pub const MEADOW: u32 = 0x800;
    pub const RUBBLE: u32 = 0x1000;
    pub const FOUNTAIN_RANGE: u32 = 0x2000;
    pub const WALL: u32 = 0x4000;
    pub const GATEHOUSE: u32 = 0x8000;
    pub const FLOODPLAIN: u32 = 0x1_0000;
    pub const FERRY_ROUTE: u32 = 0x2_0000;
    pub const MARSHLAND: u32 = 0x4_0000;
    pub const DIKE: u32 = 0x8_0000;
    pub const ORE: u32 = 0x10_0000;
    pub const IRRIGATION_RANGE: u32 = 0x100_0000;
    pub const DUNE: u32 = 0x200_0000;
    pub const DEEPWATER: u32 = 0x400_0000;
    pub const SUBMERGED_ROAD: u32 = 0x800_0000;
    /// A bridge's deck over water (Osiris's own flag; not in the original grids).
    pub const BRIDGE: u32 = 0x1000_0000;
    /// Cleopatra's cliffs: always set together with `ROCK`.
    pub const CLIFF: u32 = 0x2000_0000;
    /// Building ground people may walk across, as the original's route grids let them
    /// (Osiris's own flag, set together with `BUILDING`): a fort's parade ground and
    /// the festival square's paving off its roads. Maps have the bit cleared on load.
    pub const WALKABLE_BUILDING: u32 = 0x4000_0000;
    pub const SHORE: u32 = 0x8000_0000;
}

/// Entries in the editor's allowed-structures list.
pub const ALLOWED_STRUCTURES: usize = 46;

/// Everything a scenario needs to set up a fresh city, from any of the three sources.
#[derive(Debug, Clone)]
pub struct Scenario {
    pub version: i32,
    pub info: ScenarioInfo,
    /// Global image id drawn at each tile (0 = nothing).
    pub images: Vec<u32>,
    /// Multi-tile bits: `0x07` x and `0x38` y within the footprint, `0x40` marks the tile
    /// the footprint's image is drawn from, `0x80` native land.
    pub edges: Vec<u8>,
    pub terrain: Vec<u32>,
    /// Low 4 bits: footprint size - 1. `0x10` constructing, `0x20` alternate terrain,
    /// `0x40` deleted, `0x80` plaza or earthquake.
    pub bitfields: Vec<u8>,
    pub random: Vec<u8>,
    pub elevation: Vec<u8>,
    pub soil_fertility: Vec<u8>,
    pub vegetation_growth: Vec<u8>,
    pub moisture: Vec<u8>,
    pub random_iv: [u32; 2],
    pub camera: [i32; 2],
    /// Raw `floodplain_settings` chunk (season, duration, quality, ...).
    pub floodplain_settings: Vec<u8>,
    pub empire: crate::Empire,
    pub events: Vec<crate::EventRecord>,
    /// The buildings standing in a saved game or campaign mission (none in a `.map`).
    pub buildings: Vec<crate::buildings::BuildingRecord>,
}

fn u32_grid(bytes: &[u8]) -> Vec<u32> {
    bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| u32::from_le_bytes(*c))
        .collect()
}

impl Scenario {
    pub fn from_chunks(file: &ChunkFile) -> Result<Self> {
        let iv = u32_grid(file.require("random_iv")?);
        let cam = u32_grid(file.require("city_view_camera")?);
        let s = Self {
            version: file.version,
            info: ScenarioInfo::parse(file.require("scenario_info")?)?,
            images: u32_grid(file.require("image_grid")?),
            edges: file.require("edge_grid")?.to_vec(),
            terrain: u32_grid(file.require("terrain_grid")?),
            bitfields: file.require("bitfields_grid")?.to_vec(),
            random: file.require("random_grid")?.to_vec(),
            elevation: file.require("elevation_grid")?.to_vec(),
            soil_fertility: file.require("soil_fertility_grid")?.to_vec(),
            vegetation_growth: file.require("vegetation_growth")?.to_vec(),
            moisture: file.require("moisture_grid")?.to_vec(),
            random_iv: [iv[0], iv[1]],
            camera: [cam[0] as i32, cam[1] as i32],
            floodplain_settings: file.get("floodplain_settings").map(<[u8]>::to_vec).unwrap_or_default(),
            empire: crate::Empire::from_chunks(file)?,
            events: crate::events::events_from_chunks(file)?,
            buildings: file.get("buildings").map(crate::buildings::records).unwrap_or_default(),
        };
        debug_assert_eq!(s.images.len(), GRID_TILES);
        Ok(s)
    }

    /// The editor's "Allowed structures" list: words 0-45 of the info's reserved block
    /// (bytes 1032-1123), in the order of text group 67 (2 gold mine, 3 water lift ...
    /// 45 zoo). Scenarios older than version 117 predate the list: its first 50 entries
    /// count as allowed. Those older than 155 predate the zoo, and have it cleared.
    pub fn allowed_structures(&self) -> [bool; ALLOWED_STRUCTURES] {
        std::array::from_fn(|i| match self.version {
            v if v < 117 => i < 45,
            v if v < 155 && i >= 45 => false,
            _ => self.info.reserved.get(i).is_some_and(|&w| w != 0),
        })
    }

    /// Campaign entries older than version 149 were written when the terrain pack started
    /// 539 ids later; `id_shift` moves them back. A few terrain ids that no longer exist
    /// (15600..=15616) map onto their replacements, whatever the source.
    pub fn fix_image_ids(&mut self, id_shift: u32) {
        for id in &mut self.images {
            if *id == 0 {
                continue;
            }
            let v = id.saturating_sub(id_shift);
            *id = match v {
                15600 | 15601 => 15063,
                15602..=15616 => 15063 + v - 15602,
                _ => v,
            };
        }
    }

    /// Writes the scenario's grids, info, random seed, camera and flood settings into
    /// `file`, the mirror of `from_chunks`, and the empire (its objects, routes and
    /// prices), and the events. Works on either layout;
    /// every chunk keeps its size, so a scenario whose `floodplain_settings` came from
    /// a file version with a different size is an error.
    pub fn write_chunks(&self, file: &mut ChunkFile) -> Result<()> {
        fn u32_bytes(grid: &[u32]) -> Vec<u8> {
            grid.iter().flat_map(|v| v.to_le_bytes()).collect()
        }
        file.set("image_grid", u32_bytes(&self.images))?;
        file.set("edge_grid", self.edges.clone())?;
        file.set("terrain_grid", u32_bytes(&self.terrain))?;
        file.set("bitfields_grid", self.bitfields.clone())?;
        file.set("random_grid", self.random.clone())?;
        file.set("elevation_grid", self.elevation.clone())?;
        file.set("soil_fertility_grid", self.soil_fertility.clone())?;
        file.set("vegetation_growth", self.vegetation_growth.clone())?;
        file.set("moisture_grid", self.moisture.clone())?;
        file.set("random_iv", u32_bytes(&self.random_iv))?;
        file.set("city_view_camera", u32_bytes(&self.camera.map(|v| v as u32)))?;
        if file.get("floodplain_settings").is_some() {
            file.set("floodplain_settings", self.floodplain_settings.clone())?;
        }
        let info = file
            .get_mut("scenario_info")
            .ok_or_else(|| crate::Error::Invalid("missing chunk scenario_info".into()))?;
        self.info.write_into(info);
        self.empire.write_chunks(file)?;
        crate::events::write_events(file, &self.events)
    }

    /// `template` with this scenario written into it: pass the file the scenario was
    /// loaded from (or any map, such as `Default.map`, for a new one) so that the
    /// chunks the scenario doesn't model are kept.
    pub fn to_chunk_file(&self, mut template: ChunkFile) -> Result<ChunkFile> {
        self.write_chunks(&mut template)?;
        Ok(template)
    }

    /// Saves the scenario as a `.map` at `path`, taking the chunks the scenario doesn't
    /// model from the map at `template` (usually the one it was loaded from; it may be
    /// the same path).
    pub fn save_map(&self, template: &std::path::Path, path: &std::path::Path) -> Result<()> {
        let file = ChunkFile::open(template, crate::chunks::Layout::Map)?;
        let bytes = self.to_chunk_file(file)?.to_bytes();
        std::fs::write(path, bytes).map_err(|source| crate::Error::Io {
            path: path.to_owned(),
            source,
        })
    }

    pub fn load_map(path: &std::path::Path) -> Result<Self> {
        let data = crate::read_file(path)?;
        let mut s = Self::from_chunks(&ChunkFile::parse(&data, crate::chunks::Layout::Map)?)?;
        s.fix_image_ids(0);
        Ok(s)
    }

    /// Grid offset of map tile `(x, y)`, or `None` outside the playable rectangle.
    /// `width`/`height`/`start_offset` come straight from the file, so the arithmetic
    /// is checked and the result re-validated against the grid's real size: a
    /// corrupt or hand-edited map/save could otherwise overflow (wrapping to a bogus
    /// offset in release builds) or land outside the 228x228 grid despite `x`/`y`
    /// themselves passing the width/height check.
    pub fn offset(&self, x: i32, y: i32) -> Option<usize> {
        let i = &self.info;
        if x < 0 || y < 0 || x >= i.width || y >= i.height {
            return None;
        }
        let row = y.checked_mul(GRID_SIZE as i32)?;
        let off = i.start_offset.checked_add(row)?.checked_add(x)?;
        usize::try_from(off).ok().filter(|&o| o < GRID_TILES)
    }
}
