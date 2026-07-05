//! `split` — divide a transfer into tracks by CUE list, silence, or A/B side.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use intwav_codec::{read, Metadata, OutputFormat, PcmBuffer};
use intwav_core::frame_slice;
use serde_json::json;

use super::{analyze_pcm, output_format_str, write_output};
use crate::hash::pcm_sha256;
use crate::params::parse_cue;
use crate::report::OpReport;
use crate::timecode::ns_to_frame;

/// How to determine track boundaries.
pub enum SplitMode {
    /// CUE-style text file of `timestamp title` lines.
    Cue(PathBuf),
    /// Split at the midpoint of each detected silent region.
    Silence,
    /// Two tracks (A/B side) split at the longest silence, else the midpoint.
    Ab,
}

struct Segment {
    from_frame: u64,
    to_frame: u64,
    title: String,
}

pub fn cmd_split(
    input: &Path,
    out_dir: &Path,
    mode: SplitMode,
    output_format: Option<OutputFormat>,
    album: Option<&str>,
    artist: Option<&str>,
    report_path: Option<&Path>,
) -> Result<()> {
    let (pcm, source) = read(input).with_context(|| format!("reading {}", input.display()))?;
    let frames = pcm.frames();

    let segments = match mode {
        SplitMode::Cue(ref path) => segments_from_cue(path, &pcm)?,
        SplitMode::Silence => segments_from_silence(&pcm)?,
        SplitMode::Ab => segments_ab(&pcm)?,
    };

    std::fs::create_dir_all(out_dir)
        .with_context(|| format!("creating output directory {}", out_dir.display()))?;

    // FLAC unless the caller overrides; extension follows the format.
    let fmt = output_format.unwrap_or(OutputFormat::Flac);
    let ext = match fmt {
        OutputFormat::Flac => "flac",
        OutputFormat::Wav => "wav",
    };

    let mut output_files = Vec::new();
    let mut seg_reports = Vec::new();

    for (i, seg) in segments.iter().enumerate() {
        let track_no = i + 1;
        let slice = frame_slice(
            &pcm.samples,
            pcm.channels as usize,
            seg.from_frame,
            seg.to_frame,
        )
        .context("selecting track range")?;
        let track_pcm = PcmBuffer {
            bit_depth: pcm.bit_depth,
            sample_rate: pcm.sample_rate,
            channels: pcm.channels,
            samples: slice.to_vec(),
        };

        let filename = track_filename(track_no, &seg.title, ext);
        let out_path = out_dir.join(&filename);

        let tags = build_tags(track_no, &seg.title, album, artist);
        write_output(&track_pcm, &out_path, fmt, &tags)?;

        println!(
            "Track {track_no:02} [{}, {}) -> {} ({} frames)",
            seg.from_frame,
            seg.to_frame,
            out_path.display(),
            seg.to_frame - seg.from_frame
        );

        if report_path.is_some() {
            seg_reports.push(json!({
                "track": track_no,
                "title": seg.title,
                "from_sample": seg.from_frame,
                "to_sample": seg.to_frame,
                "file": out_path.display().to_string(),
                "pcm_sha256": pcm_sha256(&track_pcm),
            }));
        }
        output_files.push(out_path.display().to_string());
    }

    if let Some(report_path) = report_path {
        let mut report = OpReport::new("split");
        report.input_file = Some(input.display().to_string());
        report.output_files = output_files;
        report.input_format = Some(source.as_str().to_string());
        report.output_format = Some(output_format_str(fmt).to_string());
        report.decoded_pcm_bit_depth = pcm.bit_depth;
        report.sample_rate = pcm.sample_rate;
        report.channels = pcm.channels;
        report.parameters = Some(json!({ "tracks": seg_reports, "total_frames": frames }));
        report.input_pcm_sha256 = Some(pcm_sha256(&pcm));
        report.finalize_log_hash();
        report.write(report_path)?;
    }
    Ok(())
}

fn segments_from_cue(path: &Path, pcm: &PcmBuffer) -> Result<Vec<Segment>> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("reading cue file {}", path.display()))?;
    let points = parse_cue(&text).map_err(|e| anyhow::anyhow!(e))?;
    let frames = pcm.frames();
    let mut segs = Vec::new();
    for (i, p) in points.iter().enumerate() {
        let from = ns_to_frame(p.start_ns, pcm.sample_rate).min(frames);
        let to = points
            .get(i + 1)
            .map(|n| ns_to_frame(n.start_ns, pcm.sample_rate).min(frames))
            .unwrap_or(frames);
        if to <= from {
            continue;
        }
        let title = if p.title.is_empty() {
            format!("Track {:02}", i + 1)
        } else {
            p.title.clone()
        };
        segs.push(Segment {
            from_frame: from,
            to_frame: to,
            title,
        });
    }
    Ok(segs)
}

fn segments_from_silence(pcm: &PcmBuffer) -> Result<Vec<Segment>> {
    let analysis = analyze_pcm(pcm)?;
    let frames = pcm.frames();
    // Split points at the midpoint of each silent region.
    let mut cuts: Vec<u64> = analysis
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

fn segments_ab(pcm: &PcmBuffer) -> Result<Vec<Segment>> {
    let analysis = analyze_pcm(pcm)?;
    let frames = pcm.frames();
    // Split at the longest silent region's midpoint, else the file midpoint.
    let cut = analysis
        .silent_regions
        .iter()
        .max_by_key(|r| r.len_frames())
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

fn build_tags(track_no: usize, title: &str, album: Option<&str>, artist: Option<&str>) -> Metadata {
    let mut tags = Metadata::new();
    if !title.is_empty() {
        tags.push(("TITLE".to_string(), title.to_string()));
    }
    tags.push(("TRACKNUMBER".to_string(), track_no.to_string()));
    if let Some(album) = album {
        tags.push(("ALBUM".to_string(), album.to_string()));
    }
    if let Some(artist) = artist {
        tags.push(("ARTIST".to_string(), artist.to_string()));
    }
    tags
}

/// Build a filesystem-safe track filename.
fn track_filename(track_no: usize, title: &str, ext: &str) -> String {
    let sanitized: String = title
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, ' ' | '-' | '_' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let sanitized = sanitized.trim();
    if sanitized.is_empty() {
        format!("track{track_no:02}.{ext}")
    } else {
        format!("{track_no:02} {sanitized}.{ext}")
    }
}
