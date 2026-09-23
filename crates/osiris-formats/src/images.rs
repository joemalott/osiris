//! The global image id space used by maps, saves and game code.
//!
//! Each sprite pack occupies a fixed id range starting at a known base; an id resolves
//! to `(pack, id - base)`. Several packs begin with 201 unused `SYSTEM.BMP` slots, and
//! those ranges overlap the tail of the previous pack; ids falling on such a slot
//! resolve to the earlier pack.
//!
//! The core packs are always loaded. Every other pack in the data directory (the
//! monuments, temple complexes, enemy armies and so on) is opened the first time it
//! is used, and gets its own range above `EXTRA_BASE`.

use crate::sg3::Sg3;
use crate::{Error, Result};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Sprite packs of the base game and their base ids, in resolution order.
pub const CORE_PACKS: &[(&str, u32)] = &[
    ("Pharaoh_Unloaded", 0),
    ("SprMain", 700),
    ("Pharaoh_General", 11706),
    ("Pharaoh_Terrain", 14252),
    ("SprAmbient", 15831),
    ("Pharaoh_Fonts", 18765),
    ("Empire", 20305),
    ("SprMain2", 20683),
    ("Expansion", 23035),
];

pub const SYSTEM_SLOTS: usize = 201;
/// Ids of the extra packs start here, each pack in a range of `EXTRA_STRIDE` ids.
pub const EXTRA_BASE: u32 = 100_000;
pub const EXTRA_STRIDE: u32 = 16_384;

pub struct Pack {
    pub base: u32,
    pub name: String,
    dir: PathBuf,
    sg3: OnceLock<Option<Sg3>>,
    has_system_slots: bool,
}

impl Pack {
    /// The pack's images, opened on first use. A pack that fails to open is empty.
    pub fn sg3(&self) -> Option<&Sg3> {
        self.sg3.get_or_init(|| Sg3::open(&self.dir, &self.name).ok()).as_ref()
    }

    fn len(&self) -> usize {
        self.sg3().map_or(0, Sg3::len)
    }
}

pub struct ImageLibrary {
    packs: Vec<Pack>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PackImage {
    pub pack: u16,
    pub index: u16,
}

impl ImageLibrary {
    pub fn open(data_dir: &Path) -> Result<Self> {
        let mut packs = Vec::new();
        for &(name, base) in CORE_PACKS {
            let sg3 = Sg3::open(data_dir, name)?;
            let has_system_slots = sg3.group_starts.first() == Some(&0);
            packs.push(Pack { base, name: name.to_owned(), dir: data_dir.to_owned(), sg3: OnceLock::from(Some(sg3)), has_system_slots });
        }
        let mut extras: Vec<String> = std::fs::read_dir(data_dir)
            .map(|rd| {
                rd.filter_map(|e| e.ok())
                    .filter_map(|e| {
                        let p = e.path();
                        p.extension()?.eq_ignore_ascii_case("sg3").then(|| p.file_stem()?.to_str().map(str::to_owned))?
                    })
                    .filter(|n| !CORE_PACKS.iter().any(|(c, _)| c.eq_ignore_ascii_case(n)))
                    .collect()
            })
            .unwrap_or_default();
        extras.sort_by_key(|n| n.to_lowercase());
        for (i, name) in extras.into_iter().enumerate() {
            let base = EXTRA_BASE + i as u32 * EXTRA_STRIDE;
            packs.push(Pack { base, name, dir: data_dir.to_owned(), sg3: OnceLock::new(), has_system_slots: false });
        }
        Ok(Self { packs })
    }

    pub fn packs(&self) -> &[Pack] {
        &self.packs
    }

    pub fn pack(&self, index: u16) -> &Pack {
        &self.packs[index as usize]
    }

    /// The images of pack `index`.
    pub fn sg3(&self, index: u16) -> Option<&Sg3> {
        self.packs.get(index as usize)?.sg3()
    }

    pub fn pack_by_name(&self, name: &str) -> Option<(u16, &Pack)> {
        self.packs.iter().enumerate().find(|(_, p)| p.name.eq_ignore_ascii_case(name)).map(|(i, p)| (i as u16, p))
    }

    pub fn resolve(&self, id: u32) -> Option<PackImage> {
        if id >= EXTRA_BASE {
            let core = CORE_PACKS.len() as u32;
            let pack = core + (id - EXTRA_BASE) / EXTRA_STRIDE;
            let index = ((id - EXTRA_BASE) % EXTRA_STRIDE) as usize;
            let p = self.packs.get(pack as usize)?;
            return (index < p.len()).then_some(PackImage { pack: pack as u16, index: index as u16 });
        }
        for (i, p) in self.packs.iter().take(CORE_PACKS.len()).enumerate() {
            let Some(index) = id.checked_sub(p.base).map(|v| v as usize) else {
                continue;
            };
            if index >= p.len() || (p.has_system_slots && index < SYSTEM_SLOTS) {
                continue;
            }
            return Some(PackImage { pack: i as u16, index: index as u16 });
        }
        // A system slot no other pack claims (the first pack's, whose ids start at 0)
        // is the pack's own image: the UI's arrows and other system sprites.
        self.packs.iter().take(CORE_PACKS.len()).enumerate().find_map(|(i, p)| {
            let index = id.checked_sub(p.base)? as usize;
            (index < SYSTEM_SLOTS.min(p.len())).then_some(PackImage { pack: i as u16, index: index as u16 })
        })
    }

    /// Global id of image `offset` in group `group` of pack `pack_name`.
    pub fn group_id(&self, pack_name: &str, group: usize, offset: usize) -> Result<u32> {
        let (_, p) = self.pack_by_name(pack_name).ok_or_else(|| Error::Invalid(format!("no pack {pack_name}")))?;
        let start = p
            .sg3()
            .and_then(|s| s.group_start(group))
            .ok_or_else(|| Error::Invalid(format!("{pack_name}: no group {group}")))?;
        Ok(p.base + (start + offset) as u32)
    }

    pub fn record(&self, img: PackImage) -> &crate::ImageRecord {
        &self.packs[img.pack as usize].sg3().expect("resolved packs are open").records[img.index as usize]
    }
}
