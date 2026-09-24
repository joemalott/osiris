//! The difficulty setting. Most of what it changes lives in the game's model files,
//! one per difficulty: `Pharaoh_Model_<D>.txt`, `Tax_Sentiment_Model_<D>.txt` and
//! `Figure_model_<d>.txt`. The rest is the exe's own tables below, each indexed
//! Very Easy to Impossible.

use std::sync::Arc;

use crate::balance::Balance;
use crate::world::World;

pub const VERY_EASY: u8 = 0;
pub const EASY: u8 = 1;
pub const NORMAL: u8 = 2;
pub const HARD: u8 = 3;
pub const IMPOSSIBLE: u8 = 4;

/// How each difficulty is spelled in the model file names.
pub const FILE_NAMES: [&str; 5] = ["VeryEasy", "Easy", "Normal", "Hard", "Impossible"];

/// Percent of the scenario's initial funds and rescue loan the city gets (exe 0x5d1960).
pub const FUNDS_PCT: [i32; 5] = [200, 100, 100, 100, 75];
/// The kingdom rating a mission starts at (exe 0x5d1988).
pub const STARTING_KINGDOM: [i32; 5] = [70, 60, 50, 50, 40];
/// Sentiment in a new or small town (exe 0x5d199c).
pub const BASELINE_SENTIMENT: [i32; 5] = [80, 70, 60, 55, 50];
/// Percent of an invasion's size that comes, for land and Bedouin attacks (exe 0x5d19b0).
pub const INVASION_PCT: [i32; 5] = [40, 60, 80, 100, 120];
/// Percent of a target's money a thief takes.
pub const THEFT_PCT: [i32; 5] = [3, 6, 9, 12, 15];
/// Kingdom rating lost for each year in debt, from the first to the tenth and on
/// (exe 0x5da330).
pub const DEBT_YEAR_PENALTY: [[i32; 10]; 5] = [
    [-2, -5, -7, -10, -15, -20, -27, -35, -47, -50],
    [-3, -7, -12, -20, -30, -35, -40, -45, -50, -50],
    [-5, -10, -20, -35, -50, -50, -50, -50, -50, -50],
    [-10, -25, -40, -50, -50, -50, -50, -50, -50, -50],
    [-15, -35, -50, -50, -50, -50, -50, -50, -50, -50],
];
/// Favour a god picked for the day gathers at mood 90+, 80-89 and 70-79 (exe 0x56f720).
pub const FAVOUR: [[i32; 5]; 3] = [[6, 4, 2, 1, 1], [3, 2, 1, 1, 0], [2, 1, 0, 0, 0]];
/// Wrath it gathers at mood 21-30, 11-20 and 10 or less (exe 0x56f798); none at 31-49.
pub const WRATH: [[i32; 5]; 3] = [[0, 0, 0, 1, 2], [0, 1, 1, 2, 3], [1, 1, 2, 4, 5]];

pub(crate) fn normal() -> u8 {
    NORMAL
}

impl World {
    /// Gives the world every difficulty's balance tables and takes up the ones of
    /// its own difficulty.
    pub fn attach_balances(&mut self, balances: Arc<[Arc<Balance>; 5]>) {
        self.balances = Some(balances);
        self.set_difficulty(self.difficulty);
    }

    /// Plays on at difficulty `d` from now on. The original lets it change mid-game
    /// and reads it live; going lower also lowers the lowest difficulty played, which
    /// the time limit's grace years go by.
    pub fn set_difficulty(&mut self, d: u8) {
        self.difficulty = d.min(IMPOSSIBLE);
        self.lowest_difficulty = self.lowest_difficulty.min(self.difficulty);
        if let Some(b) = &self.balances {
            self.balance = b[self.difficulty as usize].clone();
        }
    }

    /// A new game at difficulty `d`: the treasury holds the scenario's funds scaled
    /// by the difficulty, and the kingdom rating starts at the difficulty's.
    pub fn begin_at(&mut self, d: u8) {
        self.set_difficulty(d);
        self.lowest_difficulty = self.difficulty;
        let d = self.difficulty as usize;
        self.treasury = self.treasury * FUNDS_PCT[d] / 100;
        self.finance.last_year_balance = self.treasury;
        self.ratings.kingdom = STARTING_KINGDOM[d];
        self.ratings.last_kingdom = STARTING_KINGDOM[d];
    }

    /// "Lower Difficulty" on the Out of Time screen: play on at Easy (Very Easy if
    /// already on Easy), with that difficulty's extra years on the time limit.
    pub fn lower_difficulty_for_time(&mut self) {
        let d = (self.lowest_difficulty.max(1) - 1).min(EASY);
        self.set_difficulty(d);
        self.lowest_difficulty = d;
        self.lost = false;
        self.defeat_shown = false;
    }

    /// The difficulty's entry in a table of five.
    pub(crate) fn by_difficulty<T: Copy>(&self, table: [T; 5]) -> T {
        table[self.difficulty.min(IMPOSSIBLE) as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world() -> Option<World> {
        let data = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../PharaohData");
        if !data.is_dir() {
            return None;
        }
        let library = osiris_formats::ImageLibrary::open(&data.join("Data")).expect("open image library");
        let scenario = osiris_formats::Scenario::load_map(&data.join("Maps/Sandbox.map")).expect("load map");
        let defs = Arc::new(crate::defs::Defs::load(&library).expect("load defs"));
        let model = osiris_formats::Model::parse(&String::from_utf8_lossy(&std::fs::read(data.join("Pharaoh_Model_Normal.txt")).expect("read model"))).expect("parse model");
        Some(World::new(&scenario, defs, Arc::new(Balance::from_model(&model))))
    }

    #[test]
    fn a_new_game_takes_the_difficultys_funds_and_rating() {
        let Some(mut w) = world() else { return };
        let funds = w.treasury;
        w.begin_at(VERY_EASY);
        assert_eq!((w.treasury, w.ratings.kingdom, w.lowest_difficulty), (funds * 2, 70, VERY_EASY));
        let Some(mut w) = world() else { return };
        w.begin_at(IMPOSSIBLE);
        assert_eq!((w.treasury, w.ratings.kingdom), (funds * 3 / 4, 40));
        // Going up mid-game leaves the lowest played where it was.
        w.set_difficulty(EASY);
        w.set_difficulty(HARD);
        assert_eq!((w.difficulty, w.lowest_difficulty), (HARD, EASY));
    }

    #[test]
    fn lowering_the_difficulty_out_of_time_plays_on_at_easy() {
        let Some(mut w) = world() else { return };
        w.begin_at(HARD);
        w.lost = true;
        w.lower_difficulty_for_time();
        assert_eq!((w.difficulty, w.lowest_difficulty, w.lost), (EASY, EASY, false));
        w.lower_difficulty_for_time();
        assert_eq!(w.difficulty, VERY_EASY);
    }
}
