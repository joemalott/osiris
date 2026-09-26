//! Optional rule changes, saved with each game. The defaults are the original rules,
//! except where a rule fixes a plain bug in the original (`ptah_speeds_guilds`).

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
    /// New storage yards and granaries start refusing every good, rather than
    /// accepting what they normally take.
    pub storage_accepts_none: bool,
    /// Fixes the original's lost bonus: a Ptah temple complex with its altar speeds
    /// the carpenters' guild by half again, as its list of Ptah's work says it should.
    pub ptah_speeds_guilds: bool,
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
            storage_accepts_none: false,
            ptah_speeds_guilds: true,
        }
    }
}
