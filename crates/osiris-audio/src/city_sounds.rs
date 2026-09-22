//! The building-type -> looping ambience file table (`data/city_sounds.toml`).
//!
//! See that file's header comment for the sourcing of each entry (the Akhenaten
//! reimplementation's `src/scripts/city/sounds.js` channel table and
//! `src/sound/sound_city.cpp` building-to-channel grouping, read for facts only).

use std::collections::HashMap;

use serde::Deserialize;

/// One row of `city_sounds.toml`.
#[derive(Debug, Clone, Deserialize)]
pub struct ChannelSound {
    /// Building type id, matching `crates/osiris-sim/data/buildings.toml`'s `id`.
    pub id: u32,
    /// The building's snake_case key in `buildings.toml`, kept for readability/debugging.
    #[allow(dead_code)]
    pub key: String,
    /// Path relative to `PharaohData/AUDIO/`, e.g. `"Ambient/HOUSING1.MP3"`.
    pub file: String,
}

#[derive(Debug, Deserialize)]
struct CitySoundsFile {
    channel: Vec<ChannelSound>,
}

/// The table embedded in the crate at `data/city_sounds.toml`.
pub const EMBEDDED_TOML: &str = include_str!("../data/city_sounds.toml");

/// Parses a `city_sounds.toml` document into a lookup by building id.
pub fn parse(text: &str) -> Result<HashMap<u32, ChannelSound>, toml::de::Error> {
    let file: CitySoundsFile = toml::from_str(text)?;
    Ok(file.channel.into_iter().map(|c| (c.id, c)).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn embedded_table_parses() {
        let table = parse(EMBEDDED_TOML).expect("embedded city_sounds.toml must parse");
        assert!(!table.is_empty());
        // id must match the key used to index the map.
        for (id, entry) in &table {
            assert_eq!(*id, entry.id);
        }
    }

    /// Every file referenced by city_sounds.toml must exist under PharaohData/AUDIO.
    /// Skipped if PharaohData isn't present (e.g. in an environment without the original
    /// game data checked out).
    #[test]
    fn referenced_files_exist_in_pharaoh_data() {
        let audio_dir = crate::test_support::pharaoh_audio_dir();
        let Some(audio_dir) = audio_dir else {
            eprintln!("skipping: PharaohData/AUDIO not found");
            return;
        };

        let table = parse(EMBEDDED_TOML).expect("embedded city_sounds.toml must parse");
        for entry in table.values() {
            let path = audio_dir.join(&entry.file);
            assert!(
                path_exists_case_insensitive(&path),
                "city_sounds.toml: building {} ({}) references missing file {:?}",
                entry.id,
                entry.key,
                path
            );
        }
    }

    /// Case-insensitive existence check: the original data ships with inconsistent casing
    /// (`shr_osiris.wav` vs `SHR_RA.WAV`) and this crate should not depend on the host
    /// filesystem being case-insensitive to find them.
    fn path_exists_case_insensitive(path: &Path) -> bool {
        if path.exists() {
            return true;
        }
        let (Some(dir), Some(name)) = (path.parent(), path.file_name()) else {
            return false;
        };
        let Ok(entries) = std::fs::read_dir(dir) else {
            return false;
        };
        entries
            .filter_map(|e| e.ok())
            .any(|e| e.file_name().eq_ignore_ascii_case(name))
    }
}
