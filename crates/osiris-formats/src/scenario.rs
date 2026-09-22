//! Scenario data common to maps, saves and campaign entries: the tile grids and the
//! 1592-byte `scenario_info` block.
//!
//! Grids are 228x228 and indexed by grid offset. The playable map is the
//! `width x height` rectangle starting at `start_offset`; map tile `(x, y)` lives at
//! `start_offset + y * 228 + x`.

use crate::bytes::Reader;
use crate::chunks::{ChunkFile, GRID_SIZE, GRID_TILES};
use crate::Result;

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
    pub monuments: [u16; 3],
    pub burial_provisions_required: Vec<u32>,
    pub current_pharaoh: u32,
    pub player_incarnation: u32,
}

fn points_u16(r: &mut Reader, n: usize) -> Result<Vec<TilePoint>> {
    let xs: Vec<i32> = (0..n).map(|_| r.u16().map(|v| v as i16 as i32)).collect::<Result<_>>()?;
    let ys: Vec<i32> = (0..n).map(|_| r.u16().map(|v| v as i16 as i32)).collect::<Result<_>>()?;
    Ok(xs.into_iter().zip(ys).map(|(x, y)| TilePoint { x, y }).collect())
}

fn points_i32(r: &mut Reader, n: usize) -> Result<Vec<TilePoint>> {
    let xs: Vec<i32> = (0..n).map(|_| r.i32()).collect::<Result<_>>()?;
    let ys: Vec<i32> = (0..n).map(|_| r.i32()).collect::<Result<_>>()?;
    Ok(xs.into_iter().zip(ys).map(|(x, y)| TilePoint { x, y }).collect())
}

fn point(r: &mut Reader) -> Result<TilePoint> {
    Ok(TilePoint {
        x: r.i16()? as i32,
        y: r.i16()? as i32,
    })
}

impl ScenarioInfo {
    pub fn parse(data: &[u8]) -> Result<Self> {
        let mut r = Reader::new(data, "scenario_info");
        let mut s = Self {
            start_year: r.i16()?,
            ..Default::default()
        };
        r.skip(2)?;
        s.empire_id = r.i16()?;
        r.skip(4)?;
        for g in &mut s.gods_known {
            *g = r.u8()? != 0;
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
        let _alt_predator_type = r.u16()?;
        s.predator_herd_types = (0..4).map(|_| r.u16()).collect::<Result<_>>()?;
        r.skip(34)?;
        s.invasion_points_land = points_u16(&mut r, 8)?;
        s.invasion_points_sea = points_u16(&mut r, 8)?;
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
    pub const SHORE: u32 = 0x8000_0000;
}

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
}

fn u32_grid(bytes: &[u8]) -> Vec<u32> {
    bytes
        .chunks_exact(4)
        .map(|c| u32::from_le_bytes(c.try_into().unwrap()))
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
        };
        debug_assert_eq!(s.images.len(), GRID_TILES);
        Ok(s)
    }

    pub fn load_map(path: &std::path::Path) -> Result<Self> {
        let data = crate::read_file(path)?;
        Self::from_chunks(&ChunkFile::parse(&data, crate::chunks::Layout::Map)?)
    }

    /// Grid offset of map tile `(x, y)`, or `None` outside the playable rectangle.
    pub fn offset(&self, x: i32, y: i32) -> Option<usize> {
        let i = &self.info;
        (x >= 0 && y >= 0 && x < i.width && y < i.height)
            .then(|| (i.start_offset + y * GRID_SIZE as i32 + x) as usize)
    }
}
