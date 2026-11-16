//! Older maps brought up to the version the original's editor saves: 160 (0xa0,
//! which FUN_004d5520 sets before writing a map). The map loader (FUN_004d4ae0)
//! upgrades a file older than that as it reads it, step by step; a map saved again
//! is written in the new version, so the steps aren't run twice. Of those steps,
//! these touch what a map file holds:
//!
//! - before 151: the climate's other beast (info bytes 700-701) is cleared;
//! - before 152: an event naming a resource past 30 moves it up by five, past the
//!   five goods Cleopatra added at 31, so deben and troops become 36 and 37, and its
//!   three choices are packed (FUN_004d40c0);
//! - before 153: the empire's trade amounts, three bit masks, become a tier byte per
//!   resource (FUN_004d19e0; Osiris's reader goes by the masks below 160);
//! - before 155: info bytes 1122-1231 are cleared: buildings-allowed words 45-99,
//!   the zoo first among them;
//! - before 157: the monuments' era (byte 996) becomes 1, the Pyramids;
//! - before 158: the "player fights Egypt" toggle (byte 997) is cleared.
//!
//! The Mission Editor saves into the result, so what it writes is read the same way
//! by the game (Osiris's included, which upgrades items and allowed buildings by the
//! file's version) as by the editor that made it.

use crate::chunks::{ChunkFile, Layout};
use crate::events::{events_from_chunks, write_events};
use crate::{Empire, Result};

/// The version the original's editor writes maps in.
pub const EDITOR_VERSION: i32 = 160;

/// Event kinds whose item is a resource (FUN_004493c0, the table at 0x5d9c78):
/// requests, demand and price changes, gifts.
pub fn item_is_resource(kind: u8) -> bool {
    matches!(kind, 1 | 13 | 14 | 15 | 16 | 23)
}

/// An event's item as FUN_004d40c0 upgrades it from a file before version 152:
/// every resource past 30 moves up by five, and the three choices are packed so the
/// ones in use come first.
pub fn upgrade_item(v: crate::EventValue) -> crate::EventValue {
    let up = |r: i16| if r > 30 { r + 5 } else { r };
    let mut choices = [up(v.fixed), up(v.min), up(v.max)].into_iter().filter(|&r| r >= 0);
    let mut next = || choices.next().unwrap_or(-1);
    crate::EventValue { value: up(v.value), fixed: next(), min: next(), max: next() }
}

/// `file`, a map, as the original's editor would save it once loaded: in version
/// 160's layout, with the loader's upgrades applied. A map already of version 160
/// (or later) comes back as it is.
pub fn map_as_saved(file: &ChunkFile) -> Result<ChunkFile> {
    let old = file.version;
    if file.layout != Layout::Map || old >= EDITOR_VERSION {
        return Ok(file.clone());
    }
    let empire = Empire::from_chunks(file)?;
    let mut events = events_from_chunks(file)?;
    let mut out = file.relaid(EDITOR_VERSION);
    // Every object record is written whole in the new layout, from the bytes both
    // layouts share and the tiers read from the masks.
    empire.write_chunks(&mut out)?;
    if old < 152 {
        for e in events.iter_mut().filter(|e| item_is_resource(e.kind)) {
            e.item = upgrade_item(e.item);
        }
        write_events(&mut out, &events)?;
    }
    if let Some(info) = out.get_mut("scenario_info") {
        if old < 151 {
            info[700..702].fill(0);
        }
        if old < 155 {
            info[1122..1232].fill(0);
        }
        if old < 157 {
            info[996] = 1;
        }
        if old < 158 {
            info[997] = 0;
        }
    }
    Ok(out)
}
