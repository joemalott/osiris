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

#[derive(Debug, Clone, Default, Deserialize)]
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
    #[serde(default)]
    pub shore: Vec<ContextRow>,
}

/// First image id of each terrain image group used by the tile rules.
#[derive(Debug, Clone, Default)]
pub struct TerrainImages {
    pub shrub: u32,
    pub tree: u32,
    pub water: u32,
    /// Earthquake cracks (`Pharaoh_Terrain` group 6).
    pub earthquake: u32,
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
    /// Dunes (`Pharaoh_Terrain` group 13): 8 single tiles, 4 2x2 and 2 3x3, like rocks.
    pub dune: u32,
    /// Trees still growing (`Pharaoh_Terrain` group 12, running on through 14 and 25).
    pub young_tree: u32,
    /// Water beside the floodplain and open water (`Pharaoh_Terrain` group 19).
    pub flood_water: u32,
    /// Dirt roads (`Pharaoh_Terrain` group 43).
    pub dirt_road: u32,
    /// Dirt road on the floodplain (`Pharaoh_Terrain` group 51).
    pub floodplain_road: u32,
    /// Cleopatra's cliffs (`Expansion` group 33, CliffTiles.bmp).
    pub cliff: u32,
    /// Plaza tiles (`Pharaoh_General` group 168).
    pub plaza: u32,
    /// Garden tiles (`Pharaoh_General` group 59).
    pub garden: u32,
}

/// A sprite reference as written in the data files.
#[derive(Debug, Clone, Deserialize)]
pub struct ImageRef {
    pub pack: String,
    pub group: usize,
    #[serde(default)]
    pub offset: i32,
    #[serde(default)]
    pub frames: u32,
    #[serde(default)]
    pub x: i32,
    #[serde(default)]
    pub y: i32,
    #[serde(default)]
    pub duration: u32,
}

/// An image reference resolved to a global image id.
#[derive(Debug, Clone, Copy, Default)]
pub struct Anim {
    pub image: u32,
    pub frames: u32,
    pub x: i32,
    pub y: i32,
    pub duration: u32,
}

#[derive(Debug, Clone, Deserialize)]
struct RawBuilding {
    id: u16,
    key: String,
    #[serde(default = "one")]
    size: i32,
    #[serde(default)]
    labor: Option<String>,
    #[serde(default)]
    figure: Option<OneOrMany>,
    #[serde(default)]
    image: Option<ImageRef>,
    #[serde(default)]
    anims: std::collections::BTreeMap<String, ImageRef>,
    #[serde(default)]
    needs: Vec<String>,
    #[serde(default)]
    flags: Vec<String>,
    #[serde(default)]
    inputs: Vec<String>,
    #[serde(default)]
    outputs: Vec<String>,
    #[serde(default)]
    text_id: Option<i32>,
    #[serde(default)]
    variants: Option<toml::Value>,
    #[serde(default)]
    variants_merged: Option<toml::Value>,
    #[serde(flatten)]
    extra: toml::Table,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum OneOrMany {
    One(String),
    Many(Vec<String>),
}

impl OneOrMany {
    fn list(&self) -> Vec<String> {
        match self {
            Self::One(s) => vec![s.clone()],
            Self::Many(v) => v.clone(),
        }
    }
}

fn one() -> i32 {
    1
}

#[derive(Debug, Clone, Default)]
pub struct BuildingDef {
    pub id: u16,
    pub key: String,
    pub size: i32,
    pub labor: Option<String>,
    /// The walker it sends out (the first, when there are several).
    pub figure: Option<u16>,
    /// All walker types it can send out.
    pub figures: Vec<u16>,
    pub image: u32,
    pub anims: std::collections::BTreeMap<String, Anim>,
    /// House variants (level images), when present.
    pub variants: Vec<u32>,
    /// A small house level's images for four lots merged into one 2x2 house.
    pub variants_merged: Vec<u32>,
    pub needs: Vec<String>,
    pub flags: Vec<String>,
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
    pub text_id: Option<i32>,
    pub extra: toml::Table,
}

impl BuildingDef {
    pub fn has_flag(&self, f: &str) -> bool {
        self.flags.iter().any(|x| x == f)
    }
    pub fn needs(&self, n: &str) -> bool {
        self.needs.iter().any(|x| x == n)
    }
    pub fn int(&self, key: &str) -> Option<i64> {
        match self.extra.get(key)? {
            toml::Value::Integer(i) => Some(*i),
            toml::Value::Boolean(b) => Some(*b as i64),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct RawFigure {
    id: u16,
    key: String,
    #[serde(default)]
    anims: std::collections::BTreeMap<String, ImageRef>,
    #[serde(flatten)]
    extra: toml::Table,
}

#[derive(Debug, Clone, Default)]
pub struct FigureDef {
    pub id: u16,
    pub key: String,
    pub anims: std::collections::BTreeMap<String, Anim>,
    pub extra: toml::Table,
}

impl FigureDef {
    pub fn int(&self, key: &str) -> Option<i64> {
        match self.extra.get(key)? {
            toml::Value::Integer(i) => Some(*i),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Menu {
    pub key: String,
    #[serde(default)]
    pub items: Vec<String>,
}

#[derive(Deserialize)]
struct BuildingsFile {
    building: Vec<RawBuilding>,
}
#[derive(Deserialize)]
struct FiguresFile {
    figure: Vec<RawFigure>,
}
#[derive(Deserialize)]
struct MenusFile {
    menu: Vec<Menu>,
}

#[derive(Default)]
pub struct Defs {
    pub contexts: ContextTables,
    pub terrain: TerrainImages,
    /// Indexed by building type id.
    pub buildings: Vec<Option<BuildingDef>>,
    /// Indexed by figure type id.
    pub figures: Vec<Option<FigureDef>>,
    pub menus: Vec<Menu>,
    /// Resource keys indexed by resource id.
    pub resources: Vec<String>,
    /// Invading armies: the foreign nations, then the Bedouin and the Egyptians.
    pub armies: Vec<ArmyDef>,
}

/// One arm of an army (infantry, archers, chariots): marching, striking and falling.
#[derive(Debug, Clone, Copy, Default)]
pub struct ArmSprites {
    pub walk: Anim,
    pub attack: Anim,
    pub death: Anim,
}

/// An invading army's make-up and art, from `enemies.toml`.
#[derive(Debug, Clone, Default)]
pub struct ArmyDef {
    pub key: String,
    /// Its name in text group 37 (the army's title is 28 further on).
    pub name: Option<usize>,
    /// Scenario enemy ids that name it.
    pub enemy_ids: Vec<i64>,
    /// First of its five rows in the figure model's enemy list.
    pub stats_row: Option<usize>,
    /// Infantry, archers, chariots.
    pub arms: [Option<ArmSprites>; 3],
    /// Its transport ships: sailing, at rest, sinking.
    pub transport: Option<ArmSprites>,
}

fn load_armies(lib: &osiris_formats::ImageLibrary) -> Result<Vec<ArmyDef>, String> {
    let t: toml::Table = toml::from_str(include_str!("../data/enemies.toml")).map_err(|e| format!("enemies.toml: {e}"))?;
    let army = |v: &toml::Value| -> Option<ArmyDef> {
        let pack = v.get("pack")?.as_str()?;
        let anim = |arm: &toml::Value, key: &str| -> Option<Anim> {
            let a = arm.get(key)?;
            let group = a.get("group")?.as_integer()? as usize;
            let frames = a.get("frames")?.as_integer()? as u32;
            Some(Anim { image: lib.group_id(pack, group, 0).ok()?, frames, x: 0, y: 0, duration: 1 })
        };
        let arm = |key: &str| -> Option<ArmSprites> {
            let a = v.get(key)?;
            Some(ArmSprites { walk: anim(a, "walk")?, attack: anim(a, "attack")?, death: anim(a, "death")? })
        };
        let ship = |key: &str| -> Option<ArmSprites> {
            let a = v.get(key)?;
            Some(ArmSprites { walk: anim(a, "swim")?, attack: anim(a, "idle")?, death: anim(a, "death")? })
        };
        Some(ArmyDef {
            key: v.get("key")?.as_str()?.to_owned(),
            name: v.get("name_text").and_then(|n| n.as_array()).and_then(|a| a.get(1)).and_then(|i| i.as_integer()).map(|i| i as usize),
            enemy_ids: v.get("enemy_ids").and_then(|a| a.as_array()).map(|a| a.iter().filter_map(|i| i.as_integer()).collect()).unwrap_or_default(),
            stats_row: v.get("stats_row").and_then(|r| r.as_integer()).filter(|&r| r >= 0).map(|r| r as usize),
            arms: [arm("infantry"), arm("archer"), arm("chariot")],
            transport: ship("transport"),
        })
    };
    let mut out: Vec<ArmyDef> = t.get("nation").and_then(|n| n.as_array()).map(|a| a.iter().filter_map(army).collect()).unwrap_or_default();
    out.extend(["bedouin", "egyptian"].iter().filter_map(|k| t.get(*k).and_then(army)));
    Ok(out)
}

#[derive(Deserialize)]
struct EnumEntry {
    id: i32,
    key: String,
}

#[derive(Deserialize)]
struct EnumsFile {
    resource: Vec<EnumEntry>,
}

fn resolve(lib: &ImageLibrary, r: &ImageRef) -> Option<Anim> {
    let base = lib.group_id(&r.pack, r.group, 0).ok()?;
    let image = base.checked_add_signed(r.offset)?;
    Some(Anim {
        image,
        frames: r.frames,
        x: r.x,
        y: r.y,
        duration: r.duration.max(1),
    })
}

/// House variant tables look like `{ 1 = {pack,..}, 2 = {..} }` or a list.
fn resolve_variants(lib: &ImageLibrary, v: &toml::Value) -> Vec<u32> {
    let refs: Vec<toml::Value> = match v {
        toml::Value::Table(t) => {
            let mut items: Vec<_> = t.iter().collect();
            items.sort_by_key(|(k, _)| k.parse::<i32>().unwrap_or(0));
            items.into_iter().map(|(_, v)| v.clone()).collect()
        }
        toml::Value::Array(a) => a.clone(),
        _ => vec![],
    };
    refs.into_iter()
        .filter_map(|r| r.try_into::<ImageRef>().ok())
        .filter_map(|r| resolve(lib, &r).map(|a| a.image))
        .collect()
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
            earthquake: t(6)?,
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
            dune: t(13)?,
            young_tree: t(12)?,
            flood_water: t(19)?,
            dirt_road: t(43)?,
            floodplain_road: t(51)?,
            cliff: lib.group_id("Expansion", 33, 0).map_err(|e| e.to_string())?,
            plaza: lib.group_id("Pharaoh_General", 168, 0).map_err(|e| e.to_string())?,
            garden: lib.group_id("Pharaoh_General", 59, 0).map_err(|e| e.to_string())?,
        };
        let figures_raw: FiguresFile = toml::from_str(include_str!("../data/figures.toml"))
            .map_err(|e| format!("figures.toml: {e}"))?;
        let mut figures: Vec<Option<FigureDef>> = Vec::new();
        for f in figures_raw.figure {
            let id = f.id as usize;
            if figures.len() <= id {
                figures.resize(id + 1, None);
            }
            figures[id] = Some(FigureDef {
                id: f.id,
                anims: f.anims.iter().filter_map(|(k, r)| Some((k.clone(), resolve(lib, r)?))).collect(),
                key: f.key,
                extra: f.extra,
            });
        }
        let figure_id = |key: &str| {
            figures
                .iter()
                .flatten()
                .find(|f| f.key == key)
                .map(|f| f.id)
        };
        let buildings_raw: BuildingsFile = toml::from_str(include_str!("../data/buildings.toml"))
            .map_err(|e| format!("buildings.toml: {e}"))?;
        let mut buildings: Vec<Option<BuildingDef>> = Vec::new();
        for b in buildings_raw.building {
            let id = b.id as usize;
            if buildings.len() <= id {
                buildings.resize(id + 1, None);
            }
            buildings[id] = Some(BuildingDef {
                id: b.id,
                size: b.size,
                labor: b.labor,
                figures: b.figure.iter().flat_map(|f| f.list()).filter_map(|k| figure_id(&k)).collect(),
                figure: b.figure.as_ref().and_then(|f| figure_id(f.list().first()?)),
                image: b.image.as_ref().and_then(|r| resolve(lib, r)).map_or(0, |a| a.image),
                anims: b.anims.iter().filter_map(|(k, r)| Some((k.clone(), resolve(lib, r)?))).collect(),
                variants: b.variants.as_ref().map(|v| resolve_variants(lib, v)).unwrap_or_default(),
                variants_merged: b.variants_merged.as_ref().map(|v| resolve_variants(lib, v)).unwrap_or_default(),
                needs: b.needs,
                flags: b.flags,
                inputs: b.inputs,
                outputs: b.outputs,
                text_id: b.text_id,
                key: b.key,
                extra: b.extra,
            });
        }
        // Buildings whose config doesn't name a labor category use the original's table.
        let labor: toml::Table = toml::from_str(include_str!("../data/labor_categories.toml"))
            .map_err(|e| format!("labor_categories.toml: {e}"))?;
        for (id, cat) in &labor {
            let (Ok(id), Some(cat)) = (id.parse::<usize>(), cat.as_str()) else { continue };
            if let Some(Some(b)) = buildings.get_mut(id)
                && b.labor.is_none()
            {
                b.labor = Some(cat.to_owned());
            }
        }
        let menus: MenusFile = toml::from_str(include_str!("../data/menus.toml"))
            .map_err(|e| format!("menus.toml: {e}"))?;
        let enums: EnumsFile = toml::from_str(include_str!("../data/enums.toml"))
            .map_err(|e| format!("enums.toml: {e}"))?;
        let mut resources = Vec::new();
        for r in enums.resource {
            let i = r.id.max(0) as usize;
            if resources.len() <= i {
                resources.resize(i + 1, String::new());
            }
            resources[i] = r.key;
        }
        Ok(Self {
            contexts,
            terrain,
            buildings,
            figures,
            menus: menus.menu,
            resources,
            armies: load_armies(lib)?,
        })
    }

    pub fn building(&self, id: u16) -> Option<&BuildingDef> {
        self.buildings.get(id as usize)?.as_ref()
    }

    pub fn building_by_key(&self, key: &str) -> Option<&BuildingDef> {
        self.buildings.iter().flatten().find(|b| b.key == key)
    }

    pub fn figure(&self, id: u16) -> Option<&FigureDef> {
        self.figures.get(id as usize)?.as_ref()
    }

    pub fn figure_by_key(&self, key: &str) -> Option<&FigureDef> {
        self.figures.iter().flatten().find(|f| f.key == key)
    }

    pub fn army(&self, key: &str) -> Option<usize> {
        self.armies.iter().position(|a| a.key == key)
    }

    pub fn menu(&self, key: &str) -> Option<&Menu> {
        self.menus.iter().find(|m| m.key == key)
    }
}
