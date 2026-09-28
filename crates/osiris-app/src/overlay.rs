//! City overlays: views of one service or risk across the city. As in the original,
//! the buildings that provide the service stay standing, every other building is
//! flattened to its footprint, houses get a column whose height shows how well served
//! they are, and only the matching walkers are shown.
//!
//! "Hide cliffs" (the original's overlay 0x33, text 14:51, last in its Overlays
//! menu) is a view rather than a service: every building stands and every walker
//! shows, the cliffs lie flat as footprints (FUN_00432620), and sealed royal tombs
//! are drawn open (see `osiris_sim::royal_tombs`).

use osiris_formats::ImageLibrary;
use osiris_sim::World;
use osiris_sim::buildings::Building;
use osiris_sim::map::terrain;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Overlay {
    Water,
    Fire,
    Damage,
    Entertainment,
    Religion,
    Education,
    Health,
    Bazaar,
    Tax,
    Desirability,
    HideCliffs,
}

/// The overlay menu: each overlay and its name in text group 14.
pub const MENU: [(Overlay, usize); 11] = [
    (Overlay::Water, 2),
    (Overlay::Fire, 8),
    (Overlay::Damage, 9),
    (Overlay::Entertainment, 3),
    (Overlay::Religion, 4),
    (Overlay::Education, 5),
    (Overlay::Health, 6),
    (Overlay::Bazaar, 26),
    (Overlay::Tax, 25),
    (Overlay::Desirability, 27),
    (Overlay::HideCliffs, 51),
];

/// Column colours: offsets into the column group, each colour being capital, shaft
/// and base.
const PLAIN: u32 = 0;
const YELLOW: u32 = 3;
const ORANGE: u32 = 6;
const RED: u32 = 9;
const BLUE: u32 = 12;

#[derive(Clone, Copy)]
pub struct OverlayImages {
    /// Pharaoh_General group 103: overlay columns.
    pub column: u32,
    /// Pharaoh_Terrain group 20: flattened building footprints.
    pub flat: u32,
    /// Pharaoh_Terrain group 45: desirability tiles, worst to best.
    pub desirability: u32,
    /// Pharaoh_Terrain group 59: dry, groundwater, and well-watered ground.
    pub water: u32,
}

impl OverlayImages {
    pub fn load(lib: &ImageLibrary) -> osiris_formats::Result<Self> {
        Ok(Self {
            column: lib.group_id("Pharaoh_General", 103, 0)?,
            flat: lib.group_id("Pharaoh_Terrain", 20, 0)?,
            desirability: lib.group_id("Pharaoh_Terrain", 45, 0)?,
            water: lib.group_id("Pharaoh_Terrain", 59, 0)?,
        })
    }
}

/// How one tile is drawn under an overlay.
pub enum TileLook {
    Normal,
    /// A building flattened to its footprint.
    Flat,
    /// Bare ground replaced by an overlay tile.
    Ground(u32),
    /// A cliff laid flat: the plain footprint tile on each tile it covers.
    FlatCliff,
}

/// A column over a house: its colour offset and height (0..=10).
pub struct Column {
    pub color: u32,
    pub height: i32,
}

/// Whether tile `(x, y)` is cliff that "Hide cliffs" (or H) lays flat: cliff with
/// no building on it (FUN_00432620).
pub fn flat_cliff(world: &World, x: i32, y: i32) -> bool {
    let t = world.map.terrain.at_or(x, y, 0);
    let cliff = terrain::CLIFF | terrain::ROCK;
    t & cliff == cliff && t & terrain::BUILDING == 0
}

fn key_starts(key: &str, prefixes: &[&str]) -> bool {
    prefixes.iter().any(|p| key.starts_with(p))
}

impl Overlay {
    /// Building keys (prefixes) left standing in this overlay.
    fn buildings(self) -> &'static [&'static str] {
        match self {
            Overlay::Water => &["well", "water_supply", "water_lift"],
            Overlay::Fire => &["firehouse", "burning_ruin"],
            Overlay::Damage => &["architect_post"],
            Overlay::Entertainment => &["juggler_school", "booth", "bandstand", "pavillion", "conservatory", "dance_school", "senet_house", "zoo"],
            Overlay::Religion => &["temple", "shrine", "festival_square"],
            Overlay::Education => &["scribal_school", "library", "academy"],
            Overlay::Health => &["apothecary", "physician", "dentist", "mortuary"],
            Overlay::Bazaar => &["bazaar", "granary"],
            Overlay::Tax => &["tax_collector", "village_palace", "town_palace", "city_palace"],
            Overlay::Desirability | Overlay::HideCliffs => &[],
        }
    }

    /// Walker keys shown in this overlay.
    fn walkers(self) -> &'static [&'static str] {
        match self {
            Overlay::Water => &["water_carrier"],
            Overlay::Fire => &["fireman"],
            Overlay::Damage => &["architect"],
            Overlay::Entertainment => &["juggler", "musician", "dancer", "senet_player"],
            Overlay::Religion => &["priest"],
            Overlay::Education => &["teacher", "scriber", "librarian"],
            Overlay::Health => &["dentist", "physician", "herbalist", "embalmer"],
            Overlay::Bazaar => &["market_trader", "market_buyer"],
            Overlay::Tax => &["tax_collector"],
            Overlay::Desirability | Overlay::HideCliffs => &[],
        }
    }

    /// Whether this overlay shows the city's own buildings, walkers and animations,
    /// as "Hide cliffs" does (FUN_00431690 and FUN_004387d0 pass all for 0x33).
    pub fn shows_everything(self) -> bool {
        self == Overlay::HideCliffs
    }

    pub fn shows_figure(self, world: &World, kind: u16) -> bool {
        self.shows_everything() || world.defs.figure(kind).is_some_and(|d| self.walkers().contains(&d.key.as_str()))
    }

    fn shows_building(self, world: &World, b: &Building) -> bool {
        self.shows_everything() || world.defs.building(b.kind).is_some_and(|d| key_starts(&d.key, self.buildings()))
    }

    /// How tile `(x, y)` should be drawn.
    pub fn look(self, world: &World, img: &OverlayImages, x: i32, y: i32) -> TileLook {
        let map = &world.map;
        let t = map.terrain.at_or(x, y, 0);
        if t & terrain::BUILDING != 0 {
            let id = map.building.at_or(x, y, 0);
            return match world.buildings.get(id) {
                Some(b) if self.shows_building(world, b) => TileLook::Normal,
                Some(b) if b.is_house() && self == Overlay::Desirability => TileLook::Ground(desirability_tile(img, world, x, y)),
                _ => TileLook::Flat,
            };
        }
        match self {
            Overlay::Desirability => {
                let d = world.desirability.at_or(x, y, 0) as i32;
                let open = t & (terrain::ROAD | terrain::WATER | terrain::TREE | terrain::ROCK | terrain::DUNE) == 0;
                if d != 0 && open { TileLook::Ground(desirability_tile(img, world, x, y)) } else { TileLook::Normal }
            }
            Overlay::Water => {
                let open = t & (terrain::ROAD | terrain::WATER | terrain::TREE | terrain::ROCK | terrain::DUNE | terrain::SHRUB) == 0;
                if !open {
                    return TileLook::Normal;
                }
                let offset = if near_well(world, x, y) {
                    2
                } else if t & terrain::GROUNDWATER != 0 {
                    1
                } else {
                    0
                };
                TileLook::Ground(img.water + offset)
            }
            Overlay::HideCliffs if flat_cliff(world, x, y) => TileLook::FlatCliff,
            _ => TileLook::Normal,
        }
    }

    /// The column to draw over building `b`, if any.
    pub fn column(self, b: &Building) -> Option<Column> {
        let risk = |h: i32| {
            let color = if h <= 5 {
                PLAIN
            } else if h < 7 {
                YELLOW
            } else if h < 9 {
                ORANGE
            } else {
                RED
            };
            Column { color, height: h.clamp(0, 10) }
        };
        match self {
            Overlay::Fire => return (!b.is_house() || b.house.as_ref().is_some_and(|h| h.population > 0)).then(|| risk(b.fire_risk / 100)),
            Overlay::Damage => return Some(risk(b.damage_risk / 100)),
            _ => {}
        }
        let h = b.house.as_ref().filter(|h| h.population > 0)?;
        let c = &h.coverage;
        let (value, color) = match self {
            Overlay::Water => (if c.water_supply > 0 { 100 } else if h.well_access { 50 } else { 0 }, BLUE),
            Overlay::Entertainment => (h.entertainment, BLUE),
            Overlay::Religion => (h.gods, BLUE),
            Overlay::Education => (h.education, BLUE),
            Overlay::Health => (h.health, BLUE),
            Overlay::Bazaar => (c.bazaar, PLAIN),
            Overlay::Tax => (c.tax, BLUE),
            _ => return None,
        };
        (value > 0).then(|| Column { color, height: (value / 10).clamp(1, 10) })
    }
}

fn desirability_tile(img: &OverlayImages, world: &World, x: i32, y: i32) -> u32 {
    let d = world.desirability.at_or(x, y, 0) as i32;
    let offset = match d {
        i32::MIN..=-11 => 0,
        -10..=-6 => 1,
        -5..=-1 => 2,
        0..=1 => 3,
        2..=4 => 4,
        5..=9 => 5,
        10..=14 => 6,
        15..=19 => 7,
        20..=24 => 8,
        _ => 9,
    };
    img.desirability + offset
}

/// Whether a working well reaches tile `(x, y)` (the same radius houses use).
fn near_well(world: &World, x: i32, y: i32) -> bool {
    world
        .buildings
        .iter()
        .filter(|b| b.kind == osiris_sim::buildings::kind::WELL)
        .any(|b| (x - b.x).abs() <= 2 && (y - b.y).abs() <= 2)
}
