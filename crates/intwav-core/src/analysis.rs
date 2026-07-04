//! Single-pass integer analysis of interleaved PCM: peak, clipping, DC offset
//! and a simple silence scan. No floating point.

use alloc::vec::Vec;

use crate::{full_scale_magnitude, positive_rail, CoreError};

/// Per-channel accumulated statistics.
#[derive(Debug, Clone)]
pub struct ChannelStats {
    /// Largest absolute sample value seen on this channel.
    pub peak_magnitude: i64,
    /// Number of samples that reached either clipping rail.
    pub clipped: u64,
    /// Sum of all samples on this channel (for DC-offset estimation).
    pub sum: i64,
}

impl ChannelStats {
    fn new() -> Self {
        Self {
            peak_magnitude: 0,
            clipped: 0,
            sum: 0,
        }
    }

    /// Integer DC offset estimate (mean sample value) over `frames` frames.
    pub fn dc_offset(&self, frames: u64) -> i64 {
        if frames == 0 {
            0
        } else {
            self.sum / frames as i64
        }
    }
}

/// A run of consecutive frames whose every-channel magnitude stayed at or below
/// the silence threshold. Half-open range `[start_frame, end_frame)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SilentRegion {
    pub start_frame: u64,
    pub end_frame: u64,
}

impl SilentRegion {
    pub fn len_frames(&self) -> u64 {
        self.end_frame - self.start_frame
    }
}

/// Result of [`analyze`].
#[derive(Debug, Clone)]
pub struct Analysis {
    pub channels: usize,
    pub bit_depth: u16,
    /// Number of frames (samples per channel).
    pub frames: u64,
    pub per_channel: Vec<ChannelStats>,
    pub silent_regions: Vec<SilentRegion>,
}

impl Analysis {
    /// Total clipped samples across all channels.
    pub fn total_clipped(&self) -> u64 {
        self.per_channel.iter().map(|c| c.clipped).sum()
    }

    /// The 0 dBFS reference magnitude for this bit depth.
    pub fn reference_magnitude(&self) -> i64 {
        full_scale_magnitude(self.bit_depth)
    }
}

/// Analyze interleaved integer PCM in a single pass.
///
/// * `samples` — interleaved samples, length must be a multiple of `channels`.
/// * `channels` — 1 or 2 for the supported formats (must be > 0).
/// * `bit_depth` — 16, 24, or 32; determines the clipping rails.
/// * `silence_threshold` — a frame counts as silent when every channel's
///   magnitude is `<= silence_threshold`.
/// * `min_silence_frames` — silent runs shorter than this are not reported.
pub fn analyze(
    samples: &[i32],
    channels: usize,
    bit_depth: u16,
    silence_threshold: i64,
    min_silence_frames: u64,
) -> Result<Analysis, CoreError> {
    if channels == 0 {
        return Err(CoreError::ZeroChannels);
    }
    if !samples.len().is_multiple_of(channels) {
        return Err(CoreError::RaggedInterleave {
            len: samples.len(),
            channels,
        });
    }

    let pos_rail = positive_rail(bit_depth); // e.g. 2^23 - 1
    let neg_rail = -(1i64 << (bit_depth - 1)); // e.g. -2^23
    let frames = (samples.len() / channels) as u64;

    let mut per_channel: Vec<ChannelStats> = (0..channels).map(|_| ChannelStats::new()).collect();
    let mut silent_regions: Vec<SilentRegion> = Vec::new();
    let mut run_start: Option<u64> = None;

    for (frame_idx, frame) in samples.chunks_exact(channels).enumerate() {
        let mut frame_silent = true;
        for (ch, &s) in frame.iter().enumerate() {
            let s = s as i64;
            let mag = s.unsigned_abs() as i64;
            let stats = &mut per_channel[ch];
            if mag > stats.peak_magnitude {
                stats.peak_magnitude = mag;
            }
            stats.sum += s;
            if s >= pos_rail || s <= neg_rail {
                stats.clipped += 1;
            }
            if mag > silence_threshold {
                frame_silent = false;
            }
        }

        match (frame_silent, run_start) {
            (true, None) => run_start = Some(frame_idx as u64),
            (false, Some(start)) => {
                push_region(
                    &mut silent_regions,
                    start,
                    frame_idx as u64,
                    min_silence_frames,
                );
                run_start = None;
            }
            _ => {}
        }
    }
    if let Some(start) = run_start {
        push_region(&mut silent_regions, start, frames, min_silence_frames);
    }

    Ok(Analysis {
        channels,
        bit_depth,
        frames,
        per_channel,
        silent_regions,
    })
}

fn push_region(regions: &mut Vec<SilentRegion>, start: u64, end: u64, min_len: u64) {
    if end - start >= min_len {
        regions.push(SilentRegion {
            start_frame: start,
            end_frame: end,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn peak_and_dc_stereo() {
        // L: 100, -300, 200 ; R: -50, 400, 0
        let samples = vec![100, -50, -300, 400, 200, 0];
        let a = analyze(&samples, 2, 24, 0, 1).unwrap();
        assert_eq!(a.frames, 3);
        assert_eq!(a.per_channel[0].peak_magnitude, 300);
        assert_eq!(a.per_channel[1].peak_magnitude, 400);
        assert_eq!(a.per_channel[0].sum, 0);
        assert_eq!(a.per_channel[0].dc_offset(3), 0);
    }

    #[test]
    fn clip_detection_24bit_rails() {
        let pos = (1i32 << 23) - 1; // +full scale
        let neg = -(1i32 << 23); // -full scale
        let samples = vec![pos, neg, 0, 5];
        let a = analyze(&samples, 2, 24, 0, 1).unwrap();
        // ch0: pos(clip), 0 -> 1 clip ; ch1: neg(clip), 5 -> 1 clip
        assert_eq!(a.per_channel[0].clipped, 1);
        assert_eq!(a.per_channel[1].clipped, 1);
        assert_eq!(a.total_clipped(), 2);
    }

    #[test]
    fn silence_regions() {
        // mono: loud, silent x3, loud, silent x1(end)
        let samples = vec![1000, 0, 0, 0, 1000, 0];
        let a = analyze(&samples, 1, 24, 0, 2).unwrap();
        // Only the 3-frame run qualifies (min_silence_frames = 2). Trailing
        // single-frame run is dropped.
        assert_eq!(a.silent_regions.len(), 1);
        assert_eq!(a.silent_regions[0].start_frame, 1);
        assert_eq!(a.silent_regions[0].end_frame, 4);
    }

    #[test]
    fn ragged_interleave_errors() {
        let samples = vec![1, 2, 3];
        assert!(analyze(&samples, 2, 24, 0, 1).is_err());
    }
}
