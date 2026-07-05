//! Read-only inspection commands: info, check, peak, clips.

use std::path::Path;

use anyhow::{Context, Result};
use intwav_codec::read;

use super::{analyze_pcm, channel_label, label_and_space, peak_dbfs_cb, print_info_block};
use crate::format::format_dbfs;
use crate::timecode::format_duration;

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
        let (label, space) = label_and_space(pcm.channels, ch);
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
        let (label, space) = label_and_space(pcm.channels, ch);
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
