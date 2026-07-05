//! `split` — compute track boundaries (CUE/silence/AB) then delegate the actual
//! rendering to the engine.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use intwav_engine::{
    analyze_file, probe, split, CancelToken, NoProgress, OutputFormat, Segment, SplitParams,
};

use super::{engine_config, maybe_write_report};
use crate::params::parse_cue;
use crate::timecode::ns_to_frame;

/// How to determine track boundaries.
pub enum SplitMode {
    Cue(PathBuf),
    Silence,
    Ab,
}

#[allow(clippy::too_many_arguments)]
pub fn cmd_split(
    input: &Path,
    out_dir: &Path,
    mode: SplitMode,
    output_format: Option<OutputFormat>,
    album: Option<&str>,
    artist: Option<&str>,
    report: Option<&Path>,
    overwrite: bool,
) -> Result<()> {
    let segments = match mode {
        SplitMode::Cue(ref path) => segments_from_cue(input, path)?,
        SplitMode::Silence => segments_from_silence(input)?,
        SplitMode::Ab => segments_ab(input)?,
    };

    // FLAC unless the caller overrides.
    let format = output_format.unwrap_or(OutputFormat::Flac);
    let p = SplitParams {
        segments,
        album: album.map(|s| s.to_string()),
        artist: artist.map(|s| s.to_string()),
        format,
        overwrite,
    };

    let r = split(
        input,
        out_dir,
        &p,
        &engine_config(),
        &NoProgress,
        &CancelToken::new(),
    )
    .map_err(anyhow::Error::new)?;

    for (i, file) in r.output_files.iter().enumerate() {
        println!("Track {:02} -> {file}", i + 1);
    }
    maybe_write_report(&r, report)?;
    Ok(())
}

fn frame_count(input: &Path) -> Result<(u32, u64)> {
    let (spec, _) = probe(input)
        .map_err(anyhow::Error::new)
        .with_context(|| format!("reading {}", input.display()))?;
    let frames = match spec.frames {
        Some(f) => f,
        None => {
            analyze_file(input, None)
                .map_err(anyhow::Error::new)?
                .frames
        }
    };
    Ok((spec.sample_rate, frames))
}

fn segments_from_cue(input: &Path, cue: &Path) -> Result<Vec<Segment>> {
    let (rate, frames) = frame_count(input)?;
    let text = std::fs::read_to_string(cue)
        .with_context(|| format!("reading cue file {}", cue.display()))?;
    let points = parse_cue(&text).map_err(|e| anyhow::anyhow!(e))?;

    let mut segs = Vec::new();
    for (i, pt) in points.iter().enumerate() {
        let from = ns_to_frame(pt.start_ns, rate).min(frames);
        let to = points
            .get(i + 1)
            .map(|n| ns_to_frame(n.start_ns, rate).min(frames))
            .unwrap_or(frames);
        if to <= from {
            continue;
        }
        let title = if pt.title.is_empty() {
            format!("Track {:02}", i + 1)
        } else {
            pt.title.clone()
        };
        segs.push(Segment {
            from_frame: from,
            to_frame: to,
            title,
        });
    }
    Ok(segs)
}

fn segments_from_silence(input: &Path) -> Result<Vec<Segment>> {
    let a = analyze_file(input, None).map_err(anyhow::Error::new)?;
    let frames = a.frames;
    let mut cuts: Vec<u64> = a
        .silent_regions
        .iter()
        .map(|r| (r.start_frame + r.end_frame) / 2)
        .filter(|&c| c > 0 && c < frames)
        .collect();
    cuts.sort_unstable();
    cuts.dedup();

    let mut segs = Vec::new();
    let mut start = 0u64;
    for (i, &cut) in cuts.iter().enumerate() {
        segs.push(Segment {
            from_frame: start,
            to_frame: cut,
            title: format!("Track {:02}", i + 1),
        });
        start = cut;
    }
    segs.push(Segment {
        from_frame: start,
        to_frame: frames,
        title: format!("Track {:02}", cuts.len() + 1),
    });
    Ok(segs)
}

fn segments_ab(input: &Path) -> Result<Vec<Segment>> {
    let a = analyze_file(input, None).map_err(anyhow::Error::new)?;
    let frames = a.frames;
    let cut = a
        .silent_regions
        .iter()
        .max_by_key(|r| r.end_frame - r.start_frame)
        .map(|r| (r.start_frame + r.end_frame) / 2)
        .filter(|&c| c > 0 && c < frames)
        .unwrap_or(frames / 2);
    Ok(vec![
        Segment {
            from_frame: 0,
            to_frame: cut,
            title: "A".to_string(),
        },
        Segment {
            from_frame: cut,
            to_frame: frames,
            title: "B".to_string(),
        },
    ])
}
