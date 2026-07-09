//! Rendering the non-destructive document to files (Q9, Q12, Q14).
//!
//! A region renders by reading its source samples and running the **fixed
//! canonical integer op-chain** (`dc → gain → fade-in → fade-out → export16`),
//! then a verified atomic write. The Master/Derivative distinction is an
//! **engine-enforced invariant**: a Master export refuses any requantization.

use std::collections::HashMap;
use std::path::Path;

use intwav_codec::{read, Metadata, OutputFormat, PcmBuffer};
use intwav_core::{
    analyze, apply_dc_correction, apply_fade_in, apply_fade_out, apply_gain_q31, frame_slice,
    gain_q31_for_db, requantize_to_16, Rng,
};
use serde_json::json;

use crate::audio::default_silence;
use crate::config::EngineConfig;
use crate::document::{Document, OpChain, Region};
use crate::error::{EngineError, EngineResult, ErrorCode};
use crate::hash::pcm_sha256;
use crate::progress::{CancelToken, ProgressSink};
use crate::project::{resolve_source, verify_source};
use crate::report::ProcessReport;
use crate::write::write_verified;

/// Preservation master vs distribution derivative (Q14).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportKind {
    Master,
    Derivative,
}

impl ExportKind {
    fn as_str(&self) -> &'static str {
        match self {
            ExportKind::Master => "master",
            ExportKind::Derivative => "derivative",
        }
    }
}

/// Enforce the Master invariant: no requantization / bit-depth reduction.
pub fn validate_export(ops: &OpChain, kind: ExportKind) -> EngineResult<()> {
    if kind == ExportKind::Master && ops.requantizes() {
        return Err(EngineError::new(
            ErrorCode::MasterExportRequantizeRefused,
            "a master export cannot requantize; remove export16 or use a derivative export",
        ));
    }
    Ok(())
}

/// Apply the canonical op-chain to a buffer in place. Returns `(clipped,
/// requantized)`.
fn apply_chain(buf: &mut PcmBuffer, ops: &OpChain) -> EngineResult<(u64, bool)> {
    let ch = buf.channels as usize;
    let mut clipped = 0u64;

    if ops.dc_correct {
        let sil = default_silence(buf.sample_rate, buf.bit_depth);
        let a = analyze(&buf.samples, ch, buf.bit_depth, sil)?;
        let offsets: Vec<i64> = (0..ch)
            .map(|c| a.per_channel[c].dc_offset(a.frames))
            .collect();
        clipped += apply_dc_correction(&mut buf.samples, ch, &offsets, buf.bit_depth)?;
    }
    if let Some(db) = ops.gain_db {
        let coeff = gain_q31_for_db(db).ok_or_else(|| {
            EngineError::new(
                ErrorCode::InvalidParameter,
                format!("unsupported gain {db} dB"),
            )
        })?;
        clipped += apply_gain_q31(&mut buf.samples, coeff, buf.bit_depth);
    }
    if ops.fade_in_frames > 0 {
        apply_fade_in(&mut buf.samples, ch, ops.fade_in_frames, buf.bit_depth)?;
    }
    if ops.fade_out_frames > 0 {
        apply_fade_out(&mut buf.samples, ch, ops.fade_out_frames, buf.bit_depth)?;
    }
    let mut requantized = false;
    if let Some(seed) = ops.export16_seed {
        let mut rng = Rng::new(seed);
        let (s16, cl) = requantize_to_16(&buf.samples, buf.bit_depth, &mut rng)?;
        buf.samples = s16;
        buf.bit_depth = 16;
        clipped += cl;
        requantized = true;
    }
    Ok((clipped, requantized))
}

/// Render one region from an already-decoded source buffer.
pub fn render_region(
    source_pcm: &PcmBuffer,
    region: &Region,
    kind: ExportKind,
    output: &Path,
    format: OutputFormat,
    cfg: &EngineConfig,
    overwrite: bool,
) -> EngineResult<ProcessReport> {
    validate_export(&region.ops, kind)?;

    let ch = source_pcm.channels as usize;
    let slice = frame_slice(
        &source_pcm.samples,
        ch,
        region.start_frame,
        region.end_frame,
    )?;
    let mut buf = PcmBuffer {
        bit_depth: source_pcm.bit_depth,
        sample_rate: source_pcm.sample_rate,
        channels: source_pcm.channels,
        samples: slice.to_vec(),
    };
    let source_range_hash = pcm_sha256(&buf);
    let (clipped, requantized) = apply_chain(&mut buf, &region.ops)?;

    let write = write_verified(&buf, output, format, &region_tags(region), cfg, overwrite)?;

    let mut r = ProcessReport::new("render");
    r.output_file = Some(output.display().to_string());
    r.output_format = Some(
        match format {
            OutputFormat::Wav => "WAV",
            OutputFormat::Flac => "FLAC",
        }
        .to_string(),
    );
    r.decoded_pcm_bit_depth = source_pcm.bit_depth;
    r.sample_rate = source_pcm.sample_rate;
    r.channels = source_pcm.channels;
    r.from_sample = Some(region.start_frame);
    r.to_sample = Some(region.end_frame);
    r.parameters = Some(json!({
        "export_kind": kind.as_str(),
        "region_id": region.id,
        "title": region.title,
        "source_range_hash": source_range_hash,
        "ops": serde_json::to_value(&region.ops).unwrap_or(serde_json::Value::Null),
    }));
    r.sample_values_modified = region.ops.modifies_samples();
    r.requantized = requantized;
    r.dither_used = requantized;
    r.clipped_samples = clipped;
    r.pcm_verified = write.pcm_verified;
    r.output_pcm_sha256 = Some(write.output_hash);
    r.finalize_log_hash();
    Ok(r)
}

/// Render every region of a document to `out_dir`, decoding each source once,
/// verifying source hashes first (Q15). Sequential — the caller (GUI) can batch
/// in parallel across regions if desired (Q19).
#[allow(clippy::too_many_arguments)]
pub fn render_document(
    doc: &Document,
    project_dir: &Path,
    kind: ExportKind,
    out_dir: &Path,
    format: OutputFormat,
    cfg: &EngineConfig,
    overwrite: bool,
    progress: &dyn ProgressSink,
    cancel: &CancelToken,
) -> EngineResult<Vec<ProcessReport>> {
    std::fs::create_dir_all(out_dir)?;
    let ext = match format {
        OutputFormat::Flac => "flac",
        OutputFormat::Wav => "wav",
    };
    let regions = doc.regions_in_order();
    let total = regions.len().max(1);

    let mut cache: HashMap<String, PcmBuffer> = HashMap::new();
    let mut reports = Vec::new();
    for (i, region) in regions.iter().enumerate() {
        cancel.check()?;
        if !cache.contains_key(&region.source_id) {
            let src = doc.source(&region.source_id).ok_or_else(|| {
                EngineError::new(
                    ErrorCode::InvalidParameter,
                    format!("region references unknown source {:?}", region.source_id),
                )
            })?;
            let path = resolve_source(src, project_dir).ok_or_else(|| {
                EngineError::new(
                    ErrorCode::SourceMissing,
                    format!("source {:?} could not be found", src.id),
                )
            })?;
            verify_source(src, &path)?;
            let (pcm, _fmt) = read(&path)?;
            cache.insert(region.source_id.clone(), pcm);
        }
        let pcm = &cache[&region.source_id];
        let out = out_dir.join(track_filename(i + 1, &region.title, ext));
        reports.push(render_region(
            pcm, region, kind, &out, format, cfg, overwrite,
        )?);
        progress.set_permille((((i + 1) * 1000) / total) as u32);
    }
    Ok(reports)
}

fn region_tags(region: &Region) -> Metadata {
    let mut tags = Metadata::new();
    if !region.title.is_empty() {
        tags.push(("TITLE".to_string(), region.title.clone()));
    }
    if let Some(n) = region.track_number {
        tags.push(("TRACKNUMBER".to_string(), n.to_string()));
    }
    if let Some(a) = &region.album {
        tags.push(("ALBUM".to_string(), a.clone()));
    }
    if let Some(a) = &region.artist {
        tags.push(("ARTIST".to_string(), a.clone()));
    }
    tags
}

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
