//! The Nile's yearly flood (stub; being implemented).

use crate::world::World;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum FloodState {
    Resting,
    #[default]
    Farmable,
    Imminent,
    Flooding,
    Inundated,
    Contracting,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Floods {
    pub state: FloodState,
}

impl World {
    /// Called once when the scenario starts, with the scenario's `floodplain_settings` chunk.
    pub(crate) fn init_floods(&mut self, _settings: &[u8]) {}

    /// Called every tick.
    pub(crate) fn update_floods(&mut self) {}

    pub fn flood_state(&self) -> FloodState {
        self.floods.state
    }
}
