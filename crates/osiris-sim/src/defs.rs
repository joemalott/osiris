//! Static game definitions resolved against the loaded sprite packs.

use osiris_formats::ImageLibrary;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct ContextRow {
    pub tiles: [u8; 8],
    pub offsets: [u32; 4],
    pub canal: u32,
    pub variants: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ContextTables {
    pub water: Vec<ContextRow>,
    pub dirt_road: Vec<ContextRow>,
    pub paved_road: Vec<ContextRow>,
    pub wall: Vec<ContextRow>,
    pub wall_gatehouse: Vec<ContextRow>,
    pub elevation: Vec<ContextRow>,
    pub earthquake: Vec<ContextRow>,
    pub canal: Vec<ContextRow>,
    pub deepwater: Vec<ContextRow>,
    pub floodsystem: Vec<ContextRow>,
    pub grass_corners: Vec<ContextRow>,
}

/// First image id of each terrain image group used by the tile rules.
#[derive(Debug, Clone, Default)]
pub struct TerrainImages {
    pub shrub: u32,
    pub tree: u32,
    pub water: u32,
    pub empty_land_alt: u32,
    pub rock: u32,
    pub empty_land: u32,
    pub reeds: u32,
    pub floodplain: u32,
    pub reeds_grown: u32,
    pub road: u32,
    pub rubble: u32,
    pub meadow_with_grass: u32,
    pub ore_rock: u32,
    pub grass: u32,
    pub meadow_tallgrass: u32,
    pub meadow_inner: u32,
    pub deepwater: u32,
    pub grass_edges: u32,
    pub meadow_outer: u32,
}

pub struct Defs {
    pub contexts: ContextTables,
    pub terrain: TerrainImages,
}

impl Defs {
    pub fn load(lib: &ImageLibrary) -> Result<Self, String> {
        let contexts: ContextTables = toml::from_str(include_str!("../data/image_context.toml"))
            .map_err(|e| format!("image_context.toml: {e}"))?;
        let t = |group: usize| {
            lib.group_id("Pharaoh_Terrain", group, 0)
                .map_err(|e| e.to_string())
        };
        let terrain = TerrainImages {
            shrub: t(2)?,
            tree: t(4)?,
            water: t(5)?,
            empty_land_alt: t(7)?,
            rock: t(8)?,
            empty_land: t(10)?,
            reeds: t(11)?,
            floodplain: t(31)?,
            reeds_grown: t(32)?,
            road: t(33)?,
            rubble: t(34)?,
            meadow_with_grass: t(37)?,
            ore_rock: t(42)?,
            grass: t(46)?,
            meadow_tallgrass: t(54)?,
            meadow_inner: t(55)?,
            deepwater: t(61)?,
            grass_edges: t(64)?,
            meadow_outer: t(66)?,
        };
        Ok(Self { contexts, terrain })
    }
}
