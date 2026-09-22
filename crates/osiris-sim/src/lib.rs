//! The Osiris simulation: a deterministic library with no global state. A `World` is
//! built from a scenario and advanced one tick at a time; the player changes it only
//! through `Command`s.

pub mod buildings;
pub mod defs;
pub mod desirability;
pub mod figures;
pub mod houses;
pub mod grid;
pub mod map;
pub mod rng;
pub mod rules;
pub mod tiles;
pub mod time;
pub mod world;

pub use defs::Defs;
pub use rules::Rules;
pub use world::{BuildingStats, Command, Outcome, World};
