//! Command implementations. Each decodes input via `intwav-codec`, runs
//! integer analysis/processing via `intwav-core`, and prints or writes results.
//!
//! Shared helpers live here; one submodule per command group.

mod edit;
mod export;
mod inspect;
mod split;
mod trim;
mod verify;

pub use edit::{cmd_dc_correct, cmd_fade_in, cmd_fade_out, cmd_gain};
pub use export::cmd_export16;
pub use inspect::{cmd_check, cmd_clips, cmd_info, cmd_peak};
pub use split::{cmd_split, SplitMode};
pub use trim::cmd_trim;
pub use verify::cmd_verify;

use std::path::Path;

use anyhow::{Context, Result};
use intwav_codec::{detect_format, encode_flac, write_wav, Metadata, OutputFormat, PcmBuffer};
use intwav_core::{analyze, dbfs_centibels, Analysis};

use crate::format::format_dbfs;
use crate::report::PeakDbfs;
use crate::timecode::format_duration;

/// Silence threshold and minimum run length for detection. Heuristics for the
/// "simple silence detection" of §11.1: magnitude below ~-72 dBFS for at least
/// half a second.
pub(crate) fn silence_params(pcm: &PcmBuffer) -> (i64, u64) {
    let threshold = intwav_core::full_scale_magnitude(pcm.bit_depth) >> 12;
    let min_frames = (pcm.sample_rate as u64 / 2).max(1);
    (threshold, min_frames)
}

pub(crate) fn analyze_pcm(pcm: &PcmBuffer) -> Result<Analysis> {
    let (threshold, min_frames) = silence_params(pcm);
    analyze(
        &pcm.samples,
        pcm.channels as usize,
        pcm.bit_depth,
        threshold,
        min_frames,
    )
    .map_err(Into::into)
}

/// Channel labels for display: L/R for stereo, single unlabeled for mono.
pub(crate) fn channel_label(channels: u16, ch: usize) -> String {
    match (channels, ch) {
        (2, 0) => "L".to_string(),
        (2, 1) => "R".to_string(),
        _ => String::new(),
    }
}

pub(crate) fn peak_dbfs_cb(analysis: &Analysis, ch: usize) -> i32 {
    let reference = analysis.reference_magnitude();
    dbfs_centibels(analysis.per_channel[ch].peak_magnitude, reference)
}

pub(crate) fn build_peak_dbfs(analysis: &Analysis, channels: u16) -> PeakDbfs {
    let left = format_dbfs(peak_dbfs_cb(analysis, 0));
    let right = if channels >= 2 {
        Some(format_dbfs(peak_dbfs_cb(analysis, 1)))
    } else {
        None
    };
    PeakDbfs { left, right }
}

/// Print the format/parameter/peak block that matches the spec §11.1 example.
pub(crate) fn print_info_block(pcm: &PcmBuffer, source: &str, analysis: &Analysis) {
    println!("Format: {source}");
    println!("Decoded PCM: {}-bit integer", pcm.bit_depth);
    println!("Sample rate: {} Hz", pcm.sample_rate);
    println!("Channels: {}", pcm.channels);
    println!("Total frames: {}", pcm.frames());
    println!(
        "Duration: {}",
        format_duration(pcm.frames(), pcm.sample_rate)
    );
    for ch in 0..pcm.channels as usize {
        let (label, space) = label_and_space(pcm.channels, ch);
        println!(
            "Peak{space}{label}: {} dBFS",
            format_dbfs(peak_dbfs_cb(analysis, ch))
        );
    }
    println!("Clipped samples: {}", analysis.total_clipped());
    println!("Processing mode: integer-only");
    println!("Floating point used: no");
}

pub(crate) fn label_and_space(channels: u16, ch: usize) -> (String, &'static str) {
    let label = channel_label(channels, ch);
    let space = if label.is_empty() { "" } else { " " };
    (label, space)
}

/// Decide the output container: explicit flag wins, else infer from the output
/// path's extension, else default to FLAC (spec §11.2).
pub(crate) fn resolve_output_format(flag: Option<OutputFormat>, output: &Path) -> OutputFormat {
    if let Some(fmt) = flag {
        return fmt;
    }
    match detect_format(output) {
        Ok(intwav_codec::SourceFormat::Wav) => OutputFormat::Wav,
        _ => OutputFormat::Flac,
    }
}

/// Write a PCM buffer to `output` in the chosen container. `tags` are applied to
/// FLAC output (ignored for WAV, which carries no Vorbis comments).
pub(crate) fn write_output(
    pcm: &PcmBuffer,
    output: &Path,
    format: OutputFormat,
    tags: &Metadata,
) -> Result<()> {
    match format {
        OutputFormat::Wav => {
            write_wav(pcm, output).with_context(|| format!("writing WAV {}", output.display()))
        }
        OutputFormat::Flac => encode_flac(pcm, output, tags)
            .with_context(|| format!("encoding FLAC {}", output.display())),
    }
}

pub(crate) fn output_format_str(format: OutputFormat) -> &'static str {
    match format {
        OutputFormat::Wav => "WAV",
        OutputFormat::Flac => "FLAC",
    }
}
