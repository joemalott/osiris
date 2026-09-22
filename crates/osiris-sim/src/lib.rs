//! The Osiris simulation: a deterministic library with no global state. A `World` is
//! built from a scenario and advanced one tick at a time; the player changes it only
//! through `Command`s.

pub mod animals;
pub mod balance;
pub mod build;
pub mod buildings;
pub mod census;
pub mod city;
pub mod defs;
pub mod desirability;
pub mod economy;
pub mod figures;
pub mod finance;
pub mod food;
pub mod houses;
pub mod labor;
pub mod grid;
pub mod maintenance;
pub mod map;
pub mod missions;
pub mod people;
pub mod rng;
pub mod rules;
pub mod services;
pub mod tiles;
pub mod time;
pub mod world;

pub use balance::Balance;
pub use defs::Defs;
pub use rules::Rules;
pub use world::{BuildingStats, Command, Outcome, World};
