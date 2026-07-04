//! Command implementations. Each decodes input via `intwav-codec`, runs
//! integer analysis/slicing via `intwav-core`, and prints or writes results.

use std::path::Path;

use anyhow::{bail, Context, Result};
use intwav_codec::{detect_format, encode_flac, read, write_wav, OutputFormat, PcmBuffer};
use intwav_core::{analyze, dbfs_centibels, frame_slice, Analysis};

use crate::format::format_dbfs;
use crate::report::{PeakDbfs, TrimReport, TOOL_NAME, TOOL_VERSION};
use crate::timecode::{format_duration, ns_to_frame, parse_timestamp_ns};

/// Silence threshold and minimum run length used by `check`. Heuristics for the
/// "simple silence detection" of §11.1: magnitude below ~-72 dBFS for at least
/// half a second.
fn silence_params(pcm: &PcmBuffer) -> (i64, u64) {
    let threshold = intwav_core::full_scale_magnitude(pcm.bit_depth) >> 12;
    let min_frames = (pcm.sample_rate as u64 / 2).max(1);
    (threshold, min_frames)
}

fn analyze_pcm(pcm: &PcmBuffer) -> Result<Analysis> {
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
fn channel_label(channels: u16, ch: usize) -> String {
    match (channels, ch) {
        (2, 0) => "L".to_string(),
        (2, 1) => "R".to_string(),
        _ => String::new(),
    }
}

fn peak_dbfs_cb(analysis: &Analysis, ch: usize) -> i32 {
    let reference = analysis.reference_magnitude();
    dbfs_centibels(analysis.per_channel[ch].peak_magnitude, reference)
}

/// Print the format/parameter/peak block that matches the spec §11.1 example.
fn print_info_block(pcm: &PcmBuffer, source: &str, analysis: &Analysis) {
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
        let label = channel_label(pcm.channels, ch);
        let space = if label.is_empty() { "" } else { " " };
        println!(
            "Peak{space}{label}: {} dBFS",
            format_dbfs(peak_dbfs_cb(analysis, ch))
        );
    }
    println!("Clipped samples: {}", analysis.total_clipped());
    println!("Processing mode: integer-only");
    println!("Floating point used: no");
}

pub fn cmd_info(input: &Path) -> Result<()> {
    let (pcm, source) = read(input).with_context(|| format!("reading {}", input.display()))?;
    let analysis = analyze_pcm(&pcm)?;
    print_info_block(&pcm, source.as_str(), &analysis);
    Ok(())
}

pub fn cmd_check(input: &Path) -> Result<()> {
    let (pcm, source) = read(input).with_context(|| format!("reading {}", input.display()))?;
    let analysis = analyze_pcm(&pcm)?;
    print_info_block(&pcm, source.as_str(), &analysis);

    // Extra inspection beyond info: DC offset and silence.
    for ch in 0..pcm.channels as usize {
        let label = channel_label(pcm.channels, ch);
        let space = if label.is_empty() { "" } else { " " };
        println!(
            "DC offset{space}{label}: {}",
            analysis.per_channel[ch].dc_offset(analysis.frames)
        );
    }
    if analysis.silent_regions.is_empty() {
        println!("Silent regions: none");
    } else {
        println!("Silent regions: {}", analysis.silent_regions.len());
        for region in &analysis.silent_regions {
            println!(
                "  {} - {}",
                format_duration(region.start_frame, pcm.sample_rate),
                format_duration(region.end_frame, pcm.sample_rate)
            );
        }
    }
    Ok(())
}

pub fn cmd_peak(input: &Path) -> Result<()> {
    let (pcm, _source) = read(input).with_context(|| format!("reading {}", input.display()))?;
    let analysis = analyze_pcm(&pcm)?;
    for ch in 0..pcm.channels as usize {
        let label = channel_label(pcm.channels, ch);
        let space = if label.is_empty() { "" } else { " " };
        println!(
            "Peak{space}{label}: {} dBFS (raw {})",
            format_dbfs(peak_dbfs_cb(&analysis, ch)),
            analysis.per_channel[ch].peak_magnitude
        );
    }
    Ok(())
}

pub fn cmd_clips(input: &Path) -> Result<()> {
    let (pcm, _source) = read(input).with_context(|| format!("reading {}", input.display()))?;
    let analysis = analyze_pcm(&pcm)?;
    println!("Clipped samples: {}", analysis.total_clipped());
    if pcm.channels > 1 {
        for ch in 0..pcm.channels as usize {
            println!(
                "  {}: {}",
                channel_label(pcm.channels, ch),
                analysis.per_channel[ch].clipped
            );
        }
    }
    Ok(())
}

/// Decide the output container: explicit flag wins, else infer from the output
/// path's extension, else default to FLAC (spec §11.2).
fn resolve_output_format(flag: Option<OutputFormat>, output: &Path) -> Result<OutputFormat> {
    if let Some(fmt) = flag {
        return Ok(fmt);
    }
    match detect_format(output) {
        Ok(intwav_codec::SourceFormat::Wav) => Ok(OutputFormat::Wav),
        Ok(intwav_codec::SourceFormat::Flac) => Ok(OutputFormat::Flac),
        Err(_) => Ok(OutputFormat::Flac),
    }
}

fn write_output(pcm: &PcmBuffer, output: &Path, format: OutputFormat) -> Result<()> {
    match format {
        OutputFormat::Wav => {
            write_wav(pcm, output).with_context(|| format!("writing WAV {}", output.display()))
        }
        OutputFormat::Flac => {
            encode_flac(pcm, output).with_context(|| format!("encoding FLAC {}", output.display()))
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn cmd_trim(
    input: &Path,
    output: &Path,
    from: &str,
    to: &str,
    output_format: Option<OutputFormat>,
    report_path: Option<&Path>,
) -> Result<()> {
    let (pcm, source) = read(input).with_context(|| format!("reading {}", input.display()))?;

    let from_ns = parse_timestamp_ns(from).map_err(|e| anyhow::anyhow!(e))?;
    let to_ns = parse_timestamp_ns(to).map_err(|e| anyhow::anyhow!(e))?;
    if to_ns < from_ns {
        bail!("--from ({from}) is after --to ({to})");
    }
    let from_frame = ns_to_frame(from_ns, pcm.sample_rate);
    let to_frame = ns_to_frame(to_ns, pcm.sample_rate);

    // Frame-accurate slice; sample values are copied unchanged.
    let slice = frame_slice(&pcm.samples, pcm.channels as usize, from_frame, to_frame)
        .context("selecting trim range")?;
    let out_pcm = PcmBuffer {
        bit_depth: pcm.bit_depth,
        sample_rate: pcm.sample_rate,
        channels: pcm.channels,
        samples: slice.to_vec(),
    };

    let fmt = resolve_output_format(output_format, output)?;
    write_output(&out_pcm, output, fmt)?;

    if let Some(report_path) = report_path {
        // "before" statistics describe the full input prior to trimming.
        let analysis = analyze_pcm(&pcm)?;
        let peak_before = build_peak_dbfs(&analysis, pcm.channels);
        let report = TrimReport {
            tool: TOOL_NAME,
            version: TOOL_VERSION,
            input_file: input.display().to_string(),
            output_file: output.display().to_string(),
            input_format: source.as_str().to_string(),
            decoded_pcm_bit_depth: pcm.bit_depth,
            sample_rate: pcm.sample_rate,
            channels: pcm.channels,
            operation: "trim",
            from_sample: from_frame,
            to_sample: to_frame,
            sample_values_modified: false,
            floating_point_used: false,
            dither_used: false,
            resampled: false,
            requantized: false,
            peak_before_dbfs: peak_before,
            clipped_samples: analysis.total_clipped(),
        };
        let json = serde_json::to_string_pretty(&report)?;
        std::fs::write(report_path, json)
            .with_context(|| format!("writing report {}", report_path.display()))?;
    }

    println!(
        "Trimmed frames [{from_frame}, {to_frame}) -> {} ({} frames)",
        output.display(),
        to_frame - from_frame
    );
    Ok(())
}

fn build_peak_dbfs(analysis: &Analysis, channels: u16) -> PeakDbfs {
    let left = format_dbfs(peak_dbfs_cb(analysis, 0));
    let right = if channels >= 2 {
        Some(format_dbfs(peak_dbfs_cb(analysis, 1)))
    } else {
        None
    };
    PeakDbfs { left, right }
}
