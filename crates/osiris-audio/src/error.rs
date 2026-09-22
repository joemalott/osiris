//! The crate's single error type.

/// Everything that can go wrong in `osiris-audio`.
///
/// Note that a missing output device is *not* an error: [`crate::Audio::new`] degrades to a
/// silent, no-op player in that case instead of returning `Err`. This type only covers
/// failures that mean a specific sound genuinely could not be prepared or decoded.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("io error on {path}: {source}")]
    Io {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to decode {path}: {source}")]
    Decode {
        path: std::path::PathBuf,
        #[source]
        source: rodio::decoder::DecoderError,
    },
    #[error("failed to parse {what}: {source}")]
    Config {
        what: &'static str,
        #[source]
        source: toml::de::Error,
    },
}

pub type Result<T> = std::result::Result<T, Error>;
