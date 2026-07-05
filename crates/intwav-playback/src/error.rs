//! Playback errors. Distinct from `EngineError` because playback is off the
//! save path (a preview subsystem), but it maps engine errors through.

use std::fmt;

use intwav_engine::EngineError;

#[derive(Debug, Clone)]
pub enum PlaybackError {
    /// A frame range was out of bounds.
    Range(String),
    /// An I/O or scratch error while reading samples.
    Io(String),
    /// No audio device / device configuration failure.
    Device(String),
}

impl fmt::Display for PlaybackError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PlaybackError::Range(m) => write!(f, "range error: {m}"),
            PlaybackError::Io(m) => write!(f, "playback I/O error: {m}"),
            PlaybackError::Device(m) => write!(f, "audio device error: {m}"),
        }
    }
}

impl std::error::Error for PlaybackError {}

impl From<EngineError> for PlaybackError {
    fn from(e: EngineError) -> Self {
        use intwav_engine::ErrorCode;
        match e.code {
            ErrorCode::RangeOutOfBounds => PlaybackError::Range(e.message),
            _ => PlaybackError::Io(e.message),
        }
    }
}
