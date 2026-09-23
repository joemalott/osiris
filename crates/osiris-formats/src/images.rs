//! The global image id space used by maps, saves and game code.
//!
//! Each sprite pack occupies a fixed id range starting at a known base; an id resolves
//! to `(pack, id - base)`. Several packs begin with 201 unused `SYSTEM.BMP` slots, and
//! those ranges overlap the tail of the previous pack; ids falling on such a slot
//! resolve to the earlier pack.

use crate::sg3::Sg3;
use crate::{Error, Result};
use std::path::Path;

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

pub struct Pack {
    pub base: u32,
    pub sg3: Sg3,
    has_system_slots: bool,
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
            packs.push(Pack {
                base,
                sg3,
                has_system_slots,
            });
        }
        Ok(Self { packs })
    }

    pub fn packs(&self) -> &[Pack] {
        &self.packs
    }

    pub fn pack(&self, index: u16) -> &Pack {
        &self.packs[index as usize]
    }

    pub fn pack_by_name(&self, name: &str) -> Option<(u16, &Pack)> {
        self.packs
            .iter()
            .enumerate()
            .find(|(_, p)| p.sg3.name.eq_ignore_ascii_case(name))
            .map(|(i, p)| (i as u16, p))
    }

    pub fn resolve(&self, id: u32) -> Option<PackImage> {
        for (i, p) in self.packs.iter().enumerate() {
            let Some(index) = id.checked_sub(p.base).map(|v| v as usize) else {
                continue;
            };
            if index >= p.sg3.len() || (p.has_system_slots && index < SYSTEM_SLOTS) {
                continue;
            }
            return Some(PackImage {
                pack: i as u16,
                index: index as u16,
            });
        }
        // A system slot no other pack claims (the first pack's, whose ids start at 0)
        // is the pack's own image: the UI's arrows and other system sprites.
        self.packs.iter().enumerate().find_map(|(i, p)| {
            let index = id.checked_sub(p.base)? as usize;
            (index < SYSTEM_SLOTS.min(p.sg3.len())).then_some(PackImage { pack: i as u16, index: index as u16 })
        })
    }

    /// Global id of image `offset` in group `group` of pack `pack_name`.
    pub fn group_id(&self, pack_name: &str, group: usize, offset: usize) -> Result<u32> {
        let (_, p) = self
            .pack_by_name(pack_name)
            .ok_or_else(|| Error::Invalid(format!("no pack {pack_name}")))?;
        let start = p
            .sg3
            .group_start(group)
            .ok_or_else(|| Error::Invalid(format!("{pack_name}: no group {group}")))?;
        Ok(p.base + (start + offset) as u32)
    }

    pub fn record(&self, img: PackImage) -> &crate::ImageRecord {
        &self.packs[img.pack as usize].sg3.records[img.index as usize]
    }
}
