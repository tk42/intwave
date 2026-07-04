//! `intwav-core` — the integer-only audio-processing core.
//!
//! This crate holds every operation that touches sample values: analysis
//! (peak/clip/DC/silence), the dBFS conversion, and frame-accurate slicing for
//! trimming. It is deliberately `no_std` + `alloc`, has **no dependencies**, and
//! uses **no floating point**. That guarantee is enforced in CI by
//! `scripts/check-no-float.sh`, which disassembles this crate's compiled object
//! and fails the build if any floating-point arithmetic instruction appears.
//!
//! Callers (the codec/CLI crates) decode PCM to interleaved `i32` samples and
//! hand slices here; sample values are never converted to float on the way in
//! or out.

#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;

mod analysis;
mod dbfs;

pub use analysis::{analyze, Analysis, ChannelStats, SilentRegion};
pub use dbfs::{dbfs_centibels, NEG_INF_CB};

/// Errors from core processing. No panics on malformed input — every fallible
/// entry point returns one of these.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreError {
    /// `channels` was 0.
    ZeroChannels,
    /// `samples.len()` was not a whole number of frames.
    RaggedInterleave { len: usize, channels: usize },
    /// A trim range fell outside `[0, frames]` or had `from > to`.
    RangeOutOfBounds {
        from_frame: u64,
        to_frame: u64,
        frames: u64,
    },
}

impl core::error::Error for CoreError {}

impl core::fmt::Display for CoreError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            CoreError::ZeroChannels => write!(f, "channel count must be greater than zero"),
            CoreError::RaggedInterleave { len, channels } => write!(
                f,
                "sample count {len} is not a multiple of channel count {channels}"
            ),
            CoreError::RangeOutOfBounds {
                from_frame,
                to_frame,
                frames,
            } => write!(
                f,
                "trim range [{from_frame}, {to_frame}) is invalid for {frames} frames"
            ),
        }
    }
}

/// The 0 dBFS reference magnitude for a given bit depth: `2^(bit_depth-1)`.
/// A sample reaching `±2^(bit_depth-1)` corresponds to 0 dBFS.
pub const fn full_scale_magnitude(bit_depth: u16) -> i64 {
    1i64 << (bit_depth - 1)
}

/// The positive clipping rail: `2^(bit_depth-1) - 1`.
pub const fn positive_rail(bit_depth: u16) -> i64 {
    (1i64 << (bit_depth - 1)) - 1
}

/// Frame-accurate slice of interleaved PCM for trimming.
///
/// Returns the samples for frames `[from_frame, to_frame)` without copying or
/// altering any value — trimming never changes sample data (spec §9.3). Honors
/// channel boundaries: the returned slice always starts and ends on a frame.
pub fn frame_slice(
    samples: &[i32],
    channels: usize,
    from_frame: u64,
    to_frame: u64,
) -> Result<&[i32], CoreError> {
    if channels == 0 {
        return Err(CoreError::ZeroChannels);
    }
    if !samples.len().is_multiple_of(channels) {
        return Err(CoreError::RaggedInterleave {
            len: samples.len(),
            channels,
        });
    }
    let frames = (samples.len() / channels) as u64;
    if from_frame > to_frame || to_frame > frames {
        return Err(CoreError::RangeOutOfBounds {
            from_frame,
            to_frame,
            frames,
        });
    }
    let start = from_frame as usize * channels;
    let end = to_frame as usize * channels;
    Ok(&samples[start..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_slice_respects_channel_boundaries() {
        let samples = [10, 11, 20, 21, 30, 31, 40, 41]; // 4 stereo frames
        let s = frame_slice(&samples, 2, 1, 3).unwrap();
        assert_eq!(s, &[20, 21, 30, 31]);
    }

    #[test]
    fn frame_slice_full_range() {
        let samples = [1, 2, 3, 4];
        assert_eq!(frame_slice(&samples, 2, 0, 2).unwrap(), &samples[..]);
    }

    #[test]
    fn frame_slice_rejects_out_of_bounds() {
        let samples = [1, 2, 3, 4];
        assert!(frame_slice(&samples, 2, 0, 3).is_err());
        assert!(frame_slice(&samples, 2, 2, 1).is_err());
    }

    #[test]
    fn scale_helpers() {
        assert_eq!(full_scale_magnitude(24), 1 << 23);
        assert_eq!(positive_rail(24), (1 << 23) - 1);
        assert_eq!(full_scale_magnitude(16), 1 << 15);
    }
}
