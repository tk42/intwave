//! `intwav-playback` — preview playback for intwav.
//!
//! This crate is deliberately **outside** the float-free save path. Playback is
//! the one place float is unavoidable (audio devices speak `f32`), so it lives
//! here and is **not** float-scanned. The guarantee it upholds: everything up to
//! the final `i32 -> f32` device conversion is the *same integer op-chain* the
//! engine would render, so what you hear equals what you'll export (Q11).
//!
//! The device-free logic — [`FrameSource`], [`Feeder`], [`LinearResampler`] — is
//! unit-tested without any audio hardware. The cpal-backed [`Player`] lives
//! behind the default `device` feature.

mod error;
mod feeder;
mod resample;
mod source;

pub use error::PlaybackError;
pub use feeder::{Feeder, PreviewChain};
pub use resample::LinearResampler;
pub use source::{BufferSource, FrameSource};

#[cfg(feature = "device")]
mod player;
#[cfg(feature = "device")]
pub use player::{Player, PlayerState};
