//! Shared helpers for tests only.

use std::path::PathBuf;

/// Locates `PharaohData/AUDIO` next to the workspace, if the original game data has been
/// checked out. Returns `None` (rather than panicking) so tests that need it can skip
/// gracefully in environments without the data.
pub fn pharaoh_audio_dir() -> Option<PathBuf> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../PharaohData/AUDIO");
    dir.is_dir().then_some(dir)
}
