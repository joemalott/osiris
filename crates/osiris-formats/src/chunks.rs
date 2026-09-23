//! The positional chunk layout shared by scenario maps (`.map`), saved games (`.sav`)
//! and the campaign entries in `mission1.pak`.
//!
//! A file is a fixed sequence of chunks whose sizes depend only on the file version.
//! Compressed chunks are prefixed by a `u32` holding the compressed length, or
//! `0x8000_0000` when the body is stored raw.

use crate::bytes::Reader;
use crate::{Error, Result, pkware};

pub const GRID_SIZE: usize = 228;
pub const GRID_TILES: usize = GRID_SIZE * GRID_SIZE;
const RAW_MARKER: u32 = 0x8000_0000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    /// Scenario files written by the editor.
    Map,
    /// Saved games, and the campaign scenarios in `mission1.pak`.
    Save,
}

struct Spec {
    name: &'static str,
    size: usize,
    compressed: bool,
}

const fn c(name: &'static str, size: usize) -> Spec {
    Spec {
        name,
        size,
        compressed: true,
    }
}
const fn u(name: &'static str, size: usize) -> Spec {
    Spec {
        name,
        size,
        compressed: false,
    }
}

fn schema(layout: Layout, v: i32) -> Vec<Spec> {
    let g1 = GRID_TILES;
    let g2 = GRID_TILES * 2;
    let g4 = GRID_TILES * 4;
    match layout {
        Layout::Map => vec![
            u("mission_index", 4),
            u("file_version", 4),
            u("chunks_schema", 6004),
            u("image_grid", g4),
            u("edge_grid", g1),
            u("terrain_grid", g4),
            u("bitfields_grid", g1),
            u("random_grid", g1),
            u("elevation_grid", g1),
            u("random_iv", 8),
            u("city_view_camera", 8),
            u("scenario_info", 1592),
            u("soil_fertility_grid", g1),
            u("scenario_events", 18600),
            u("scenario_events_extra", 28),
            c("junk11", 1280),
            c("empire_map_objects", if v < 160 { 15200 } else { 19600 }),
            c("empire_map_routes", 16200),
            u("vegetation_growth", g1),
            c("floodplain_settings", if v < 147 { 32 } else { 36 }),
            u("trade_prices", 288),
            c("moisture_grid", g1),
        ],
        Layout::Save => {
            let mut s = vec![
                u("mission_index", 4),
                u("file_version", 4),
                u("chunks_schema", 6004),
                c("image_grid", g4),
                c("edge_grid", g1),
                c("building_grid", g2),
                c("terrain_grid", g4),
                c("aqueduct_grid", g1),
                c("figure_grid", g2),
                c("bitfields_grid", g1),
                c("sprite_grid", g1),
                u("random_grid", g1),
                c("desirability_grid", g1),
                c("elevation_grid", g1),
                c("building_damage_grid", g2),
                c("aqueduct_backup_grid", g1),
                c("sprite_backup_grid", g1),
                c("figures", 776000),
                c("route_figures", 2000),
                c("route_paths", 500000),
                c("formations", 7200),
                u("formations_info", 12),
                c("city_data", 37808),
                u("city_data_extra", 72),
                c("buildings", 1056000),
                u("city_view_orientation", 4),
                u("game_time", 20),
                u("building_highest_id_ever", 8),
                u("random_iv", 8),
                u("city_view_camera", 8),
                u("city_graph_order", 8),
                u("empire_map_params", 12),
                c("empire_cities", if v > 177 { 8480 } else { 6466 }),
                u("building_count_industry", 288),
                u("trade_prices", 288),
                u("figure_names", 84),
                u("scenario_info", 1592),
                u("max_year", 4),
                c("messages", 48000),
                u("message_extra", 182),
                u("building_burning_list_info", 8),
                u("figure_sequence", 4),
                u("scenario_carry_settings", 12),
                c("invasion_warnings", 3232),
                u("scenario_is_custom", 4),
                u("city_sounds", 8960),
                u("building_highest_id", 4),
                u("empire_traders", 8804),
                c("building_list_burning", 1000),
                c("building_list_small", 1000),
                c("building_list_large", 8000),
                u("junk7a", 32),
                u("junk7b", 24),
                u("building_storages", 39200),
                c("trade_routes_limits", 2880),
                c("trade_routes_traded", 2880),
                u("routing_stats", 50),
                u("scenario_map_name", 65),
                u("bookmarks", 32),
                u("junk9a", 12),
                u("junk9b", 396),
                u("soil_fertility_grid", g1),
                u("scenario_events", 18600),
                u("scenario_events_extra", 28),
                u("junk10a", if v < 149 { 11000 } else { 11200 }),
                u("junk10b", 2200),
                u("junk10c", 16),
                u("junk10d", 8200),
                c("junk11", 1280),
                c("empire_map_objects", if v < 160 { 15200 } else { 19600 }),
                c("empire_map_routes", 16200),
                u("vegetation_growth", g1),
                u("junk14", 20),
                u("bizarre_ordered_fields_1", 528),
                c("floodplain_settings", if v < 147 { 32 } else { 36 }),
                c("grid03_32bit", g4),
                u("bizarre_ordered_fields_4", 312),
                u("junk16", 64),
                u("tutorial_flags", 41),
                c("grid04_8bit", g1),
                u("junk17", 1),
                c("moisture_grid", g1),
                u("bizarre_ordered_fields_2", 240),
                u("bizarre_ordered_fields_3", 432),
                u("junk18", 8),
            ];
            if v >= 160 {
                s.extend([
                    u("junk19", 20),
                    u("bizarre_ordered_fields_5", 648),
                    u("bizarre_ordered_fields_6", 648),
                    u("bizarre_ordered_fields_7", 360),
                    u("bizarre_ordered_fields_8", 1344),
                    u("bizarre_ordered_fields_9", 1776),
                ]);
            }
            s
        }
    }
}

/// All chunks of a file, decompressed.
#[derive(Debug)]
pub struct ChunkFile {
    pub layout: Layout,
    pub version: i32,
    chunks: Vec<(&'static str, Vec<u8>)>,
    /// Bytes left after the last chunk (campaign entries of version 160 have 16).
    pub trailing: usize,
}

impl ChunkFile {
    pub fn parse(data: &[u8], layout: Layout) -> Result<Self> {
        if data.len() < 8 {
            return Err(Error::Truncated("chunk file"));
        }
        let version = i32::from_le_bytes(data[4..8].try_into().unwrap());
        let mut r = Reader::new(data, "chunk file");
        let mut chunks = Vec::new();
        for spec in schema(layout, version) {
            let body = if spec.compressed {
                let prefix = r.u32()?;
                if prefix == RAW_MARKER {
                    r.bytes(spec.size)?.to_vec()
                } else {
                    let packed = r.bytes(prefix as usize)?;
                    pkware::explode(packed, spec.size)
                        .map_err(|e| Error::Invalid(format!("chunk {}: {e}", spec.name)))?
                }
            } else {
                r.bytes(spec.size)?.to_vec()
            };
            chunks.push((spec.name, body));
        }
        Ok(Self {
            layout,
            version,
            chunks,
            trailing: r.remaining(),
        })
    }

    pub fn get(&self, name: &str) -> Option<&[u8]> {
        self.chunks
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, d)| d.as_slice())
    }

    pub fn require(&self, name: &'static str) -> Result<&[u8]> {
        self.get(name)
            .ok_or_else(|| Error::Invalid(format!("missing chunk {name}")))
    }

    pub fn names(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.chunks.iter().map(|(n, _)| *n)
    }
}

/// `mission1.pak`: a table of 60 `u32` offsets, then campaign scenarios in save layout.
pub struct MissionPak {
    data: Vec<u8>,
    offsets: Vec<usize>,
}

impl MissionPak {
    pub fn open(path: &std::path::Path) -> Result<Self> {
        Self::parse(crate::read_file(path)?)
    }

    pub fn parse(data: Vec<u8>) -> Result<Self> {
        let mut r = Reader::new(&data, "mission pak");
        let mut offsets = Vec::new();
        for _ in 0..60 {
            offsets.push(r.u32()? as usize);
        }
        if offsets.iter().any(|&o| o > data.len()) {
            return Err(Error::Invalid("mission pak: offset past end".into()));
        }
        Ok(Self { data, offsets })
    }

    /// Number of slots in the table; empty slots hold no scenario.
    pub fn slots(&self) -> usize {
        self.offsets.len()
    }

    pub fn entry(&self, index: usize) -> Option<&[u8]> {
        let start = *self.offsets.get(index)?;
        if start == 0 {
            return None;
        }
        let end = self
            .offsets
            .iter()
            .copied()
            .filter(|&o| o > start)
            .min()
            .unwrap_or(self.data.len());
        Some(&self.data[start..end])
    }

    pub fn chunks(&self, index: usize) -> Result<ChunkFile> {
        let entry = self
            .entry(index)
            .ok_or_else(|| Error::Invalid(format!("mission pak: no entry {index}")))?;
        ChunkFile::parse(entry, Layout::Save)
    }

    /// Campaign scenario `index`, with image ids moved into the current id space.
    /// Entries older than version 149 use the older ids, unless the top byte of their
    /// first word is set to something other than 0xff: entries 6, 7, 17 and 28 were saved
    /// again by a later build and already hold current ids.
    pub fn scenario(&self, index: usize) -> Result<crate::Scenario> {
        let file = self.chunks(index)?;
        let mut s = crate::Scenario::from_chunks(&file)?;
        let resaved = self.entry(index).is_some_and(|e| e.get(3).is_some_and(|&b| b != 0xff));
        s.fix_image_ids(if file.version < 149 && !resaved { 539 } else { 0 });
        Ok(s)
    }
}
