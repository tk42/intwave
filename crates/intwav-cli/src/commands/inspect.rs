//! Read-only inspection: info, check, peak, clips.

use std::path::Path;

use anyhow::{Context, Result};
use intwav_engine::{analyze_file, format_dbfs, AudioReport};

use super::{channel_label, label_and_space};
use crate::timecode::format_duration;

fn analyze(input: &Path) -> Result<AudioReport> {
    analyze_file(input, None)
        .map_err(|e| anyhow::anyhow!(e))
        .with_context(|| format!("reading {}", input.display()))
}

fn print_info_block(a: &AudioReport) {
    println!("Format: {}", a.format);
    println!("Decoded PCM: {}-bit integer", a.bit_depth);
    println!("Sample rate: {} Hz", a.sample_rate);
    println!("Channels: {}", a.channels);
    println!("Total frames: {}", a.frames);
    println!("Duration: {}", format_duration(a.frames, a.sample_rate));
    for ch in 0..a.channels as usize {
        let (label, space) = label_and_space(a.channels, ch);
        println!(
            "Peak{space}{label}: {} dBFS",
            format_dbfs(a.peak_centibels[ch])
        );
    }
    println!("Clipped samples: {}", a.total_clipped);
    println!("Processing mode: integer-only");
    println!("Floating point used in save path: no");
}

pub fn cmd_info(input: &Path) -> Result<()> {
    print_info_block(&analyze(input)?);
    Ok(())
}

pub fn cmd_check(input: &Path) -> Result<()> {
    let a = analyze(input)?;
    print_info_block(&a);
    for ch in 0..a.channels as usize {
        let (label, space) = label_and_space(a.channels, ch);
        println!("DC offset{space}{label}: {}", a.dc_offset[ch]);
    }
    if a.silent_regions.is_empty() {
        println!("Silent regions: none");
    } else {
        println!("Silent regions: {}", a.silent_regions.len());
        for r in &a.silent_regions {
            println!(
                "  {} - {}",
                format_duration(r.start_frame, a.sample_rate),
                format_duration(r.end_frame, a.sample_rate)
            );
        }
    }
    Ok(())
}

pub fn cmd_peak(input: &Path) -> Result<()> {
    let a = analyze(input)?;
    for ch in 0..a.channels as usize {
        let (label, space) = label_and_space(a.channels, ch);
        println!(
            "Peak{space}{label}: {} dBFS (raw {})",
            format_dbfs(a.peak_centibels[ch]),
            a.peak_magnitude[ch]
        );
    }
    Ok(())
}

pub fn cmd_clips(input: &Path) -> Result<()> {
    let a = analyze(input)?;
    println!("Clipped samples: {}", a.total_clipped);
    if a.channels > 1 {
        for ch in 0..a.channels as usize {
            println!("  {}: {}", channel_label(a.channels, ch), a.clipped[ch]);
        }
    }
    Ok(())
}
