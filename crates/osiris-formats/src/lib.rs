//! Readers for the data files shipped with Pharaoh (1999) and Cleopatra.
//!
//! Everything here is pure: parsers take byte slices (or paths, for convenience) and
//! return plain Rust data. Nothing depends on a renderer or the simulation.

pub mod bytes;
pub mod campaign;
pub mod chunks;
pub mod empire;
pub mod events;
pub mod images;
pub mod messages;
pub mod model;
pub mod pkware;
pub mod scenario;
pub mod sg3;
pub mod text;

pub use campaign::Campaign;
pub use chunks::{ChunkFile, Layout, MissionPak};
pub use empire::Empire;
pub use events::{EventRecord, EventValue, Phrases};
pub use images::{ImageLibrary, PackImage};
pub use messages::{Message, MessageTable};
pub use model::{BuildingModel, FigureModel, HouseModel, Model};
pub use scenario::{Scenario, ScenarioInfo};
pub use sg3::{ImageKind, ImageRecord, Rgba, Sg3, Sprite};
pub use text::TextTable;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("io error on {path}: {source}")]
    Io {
        path: std::path::PathBuf,
        source: std::io::Error,
    },
    #[error("{0}: file truncated")]
    Truncated(&'static str),
    #[error("{0}")]
    Invalid(String),
}

pub type Result<T> = std::result::Result<T, Error>;

pub(crate) fn read_file(path: &std::path::Path) -> Result<Vec<u8>> {
    std::fs::read(path).map_err(|source| Error::Io {
        path: path.to_owned(),
        source,
    })
}
