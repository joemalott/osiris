//! Optional rule changes, saved with each game. The defaults are the original rules.

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Rules {
    /// Workers are drawn from the whole city rather than found by labor-seeker walkers,
    /// so a building only needs road access to be staffed.
    pub global_labor_pool: bool,
    /// When off, gods have no moods, blessings or curses, and houses never need
    /// religion to evolve.
    pub gods_enabled: bool,
    pub floods: bool,
    pub disasters: bool,
    pub fire: bool,
    pub disease: bool,
    pub collapse: bool,
}

impl Default for Rules {
    fn default() -> Self {
        Self {
            global_labor_pool: false,
            gods_enabled: true,
            floods: true,
            disasters: true,
            fire: true,
            disease: true,
            collapse: true,
        }
    }
}
