//! Structured analysis result for the inspection commands. The engine returns
//! data; the CLI/GUI format it for display.

use std::path::Path;

use intwav_codec::read;
use intwav_core::{analyze, dbfs_centibels, magnitude_for_dbfs, SilenceConfig};

use crate::error::EngineResult;

/// A detected silent region, in frames `[start, end)`.
#[derive(Debug, Clone, Copy)]
pub struct SilentRegion {
    pub start_frame: u64,
    pub end_frame: u64,
}

/// Result of analyzing a file: format/parameters plus per-channel measurements.
#[derive(Debug, Clone)]
pub struct AudioReport {
    pub format: String,
    pub bit_depth: u16,
    pub sample_rate: u32,
    pub channels: u16,
    pub frames: u64,
    /// Peak magnitude per channel (raw integer sample value).
    pub peak_magnitude: Vec<i64>,
    /// Peak per channel in centibels (1/100 dB); `i32::MIN` means -inf.
    pub peak_centibels: Vec<i32>,
    pub clipped: Vec<u64>,
    pub total_clipped: u64,
    pub dc_offset: Vec<i64>,
    pub silent_regions: Vec<SilentRegion>,
}

/// A sensible default silence configuration for interactive inspection:
/// threshold ~-60 dBFS, ~20 ms window, ~0.5 s minimum gap. GUI overrides these.
pub fn default_silence(sample_rate: u32, bit_depth: u16) -> SilenceConfig {
    let threshold = magnitude_for_dbfs(bit_depth, -60).unwrap_or(0);
    let window = ((sample_rate as u64) / 50).max(1); // 20 ms
    let min_gap = ((sample_rate as u64) / 2).max(1); // 0.5 s
    SilenceConfig::new(threshold, window, min_gap)
}

/// Decode and analyze a file. Pass `Some(config)` for explicit silence
/// parameters, or `None` to use [`default_silence`] derived from the decoded
/// stream.
pub fn analyze_file(path: &Path, silence: Option<SilenceConfig>) -> EngineResult<AudioReport> {
    let (pcm, format) = read(path)?;
    let silence = silence.unwrap_or_else(|| default_silence(pcm.sample_rate, pcm.bit_depth));
    let a = analyze(&pcm.samples, pcm.channels as usize, pcm.bit_depth, silence)?;
    let reference = a.reference_magnitude();

    let peak_magnitude: Vec<i64> = a.per_channel.iter().map(|c| c.peak_magnitude).collect();
    let peak_centibels: Vec<i32> = peak_magnitude
        .iter()
        .map(|&p| dbfs_centibels(p, reference))
        .collect();
    let clipped: Vec<u64> = a.per_channel.iter().map(|c| c.clipped).collect();
    let dc_offset: Vec<i64> = a
        .per_channel
        .iter()
        .map(|c| c.dc_offset(a.frames))
        .collect();
    let silent_regions = a
        .silent_regions
        .iter()
        .map(|r| SilentRegion {
            start_frame: r.start_frame,
            end_frame: r.end_frame,
        })
        .collect();

    Ok(AudioReport {
        format: format.as_str().to_string(),
        bit_depth: pcm.bit_depth,
        sample_rate: pcm.sample_rate,
        channels: pcm.channels,
        frames: pcm.frames(),
        peak_magnitude,
        peak_centibels,
        clipped,
        total_clipped: a.total_clipped(),
        dc_offset,
        silent_regions,
    })
}
