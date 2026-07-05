//! The shared operations, each returning a verified [`ProcessReport`]. These are
//! synchronous and caller-driven (progress + cancel); the CLI and GUI both call
//! them. Numeric parameters are already resolved to sample frames / integer dB
//! by the caller — no string parsing happens here.

use std::path::Path;

use intwav_codec::{read, Metadata, OutputFormat, PcmBuffer};
use intwav_core::{
    analyze, apply_dc_correction, apply_fade_in, apply_fade_out, apply_gain_q31, dbfs_centibels,
    frame_slice, gain_q31_for_db, gain_would_clip, requantize_to_16, Rng, GAIN_UNITY_Q31,
};
use serde_json::json;

use crate::audio::default_silence;
use crate::config::EngineConfig;
use crate::error::{EngineError, EngineResult, ErrorCode};
use crate::hash::pcm_sha256;
use crate::progress::{CancelToken, ProgressSink};
use crate::report::{format_dbfs, peak_dbfs, PeakDbfs, ProcessReport};
use crate::write::write_verified;

fn output_format_str(f: OutputFormat) -> &'static str {
    match f {
        OutputFormat::Wav => "WAV",
        OutputFormat::Flac => "FLAC",
    }
}

/// Per-channel peak dBFS for a buffer.
fn peak_report(pcm: &PcmBuffer) -> EngineResult<PeakDbfs> {
    let sil = default_silence(pcm.sample_rate, pcm.bit_depth);
    let a = analyze(&pcm.samples, pcm.channels as usize, pcm.bit_depth, sil)?;
    let reference = a.reference_magnitude();
    let cbs: Vec<i32> = a
        .per_channel
        .iter()
        .map(|c| dbfs_centibels(c.peak_magnitude, reference))
        .collect();
    Ok(peak_dbfs(&cbs))
}

fn total_clipped(pcm: &PcmBuffer) -> EngineResult<u64> {
    let sil = default_silence(pcm.sample_rate, pcm.bit_depth);
    let a = analyze(&pcm.samples, pcm.channels as usize, pcm.bit_depth, sil)?;
    Ok(a.total_clipped())
}

// ---------------------------------------------------------------- trim

pub struct TrimParams {
    pub from_frame: u64,
    pub to_frame: u64,
    pub format: OutputFormat,
    pub overwrite: bool,
}

pub fn trim(
    input: &Path,
    output: &Path,
    p: &TrimParams,
    cfg: &EngineConfig,
    progress: &dyn ProgressSink,
    cancel: &CancelToken,
) -> EngineResult<ProcessReport> {
    progress.set_permille(0);
    let (pcm, source) = read(input)?;
    cancel.check()?;

    let slice = frame_slice(
        &pcm.samples,
        pcm.channels as usize,
        p.from_frame,
        p.to_frame,
    )?;
    let out_pcm = PcmBuffer {
        bit_depth: pcm.bit_depth,
        sample_rate: pcm.sample_rate,
        channels: pcm.channels,
        samples: slice.to_vec(),
    };
    progress.set_permille(400);
    cancel.check()?;

    let write = write_verified(&out_pcm, output, p.format, &Vec::new(), cfg, p.overwrite)?;
    progress.set_permille(1000);

    let mut r = ProcessReport::new("trim");
    r.input_file = Some(input.display().to_string());
    r.output_file = Some(output.display().to_string());
    r.input_format = Some(source.as_str().to_string());
    r.output_format = Some(output_format_str(p.format).to_string());
    r.decoded_pcm_bit_depth = pcm.bit_depth;
    r.sample_rate = pcm.sample_rate;
    r.channels = pcm.channels;
    r.from_sample = Some(p.from_frame);
    r.to_sample = Some(p.to_frame);
    r.parameters = Some(json!({ "output_sample_count": out_pcm.frames() }));
    r.peak_before_dbfs = Some(peak_report(&pcm)?);
    r.clipped_samples = total_clipped(&pcm)?;
    r.pcm_verified = write.pcm_verified;
    r.input_pcm_sha256 = Some(pcm_sha256(&pcm));
    r.output_pcm_sha256 = Some(write.output_hash);
    r.finalize_log_hash();
    Ok(r)
}

// ---------------------------------------------------------------- gain

pub struct GainParams {
    pub db: i32,
    pub allow_clipping: bool,
    pub format: OutputFormat,
    pub overwrite: bool,
}

pub fn gain(
    input: &Path,
    output: &Path,
    p: &GainParams,
    cfg: &EngineConfig,
    progress: &dyn ProgressSink,
    cancel: &CancelToken,
) -> EngineResult<ProcessReport> {
    let coeff = gain_q31_for_db(p.db).ok_or_else(|| {
        EngineError::new(
            ErrorCode::InvalidParameter,
            format!("unsupported gain {} dB (supported range is -96..=24)", p.db),
        )
    })?;
    progress.set_permille(0);
    let (mut pcm, source) = read(input)?;
    let before_peak = peak_report(&pcm)?;
    let before_hash = pcm_sha256(&pcm);
    cancel.check()?;

    let clipping_risk = coeff > GAIN_UNITY_Q31;
    if clipping_risk {
        let would = gain_would_clip(&pcm.samples, coeff, pcm.bit_depth);
        if would > 0 && !p.allow_clipping {
            return Err(EngineError::new(
                ErrorCode::ClipRefused,
                format!(
                    "gain of {} dB would clip {would} sample(s); pass allow_clipping to proceed",
                    p.db
                ),
            ));
        }
    }
    let clipped = apply_gain_q31(&mut pcm.samples, coeff, pcm.bit_depth);
    progress.set_permille(500);
    cancel.check()?;

    let after_peak = peak_report(&pcm)?;
    let write = write_verified(&pcm, output, p.format, &Vec::new(), cfg, p.overwrite)?;
    progress.set_permille(1000);

    let mut r = ProcessReport::new("gain");
    fill_edit_common(&mut r, input, output, &source_str(source), p.format, &pcm);
    r.parameters = Some(json!({
        "gain_spec": format!("{} dB", p.db),
        "gain_coefficient_fixed_point": coeff,
        "clipping_risk": clipping_risk,
        "allow_clipping": p.allow_clipping,
    }));
    r.sample_values_modified = true;
    r.clipped_samples = clipped;
    r.peak_before_dbfs = Some(before_peak);
    r.peak_after_dbfs = Some(after_peak);
    r.pcm_verified = write.pcm_verified;
    r.input_pcm_sha256 = Some(before_hash);
    r.output_pcm_sha256 = Some(write.output_hash);
    r.finalize_log_hash();
    Ok(r)
}

// ---------------------------------------------------------------- fade

#[derive(Clone, Copy)]
pub enum FadeKind {
    In,
    Out,
}

pub struct FadeParams {
    pub kind: FadeKind,
    pub frames: u64,
    pub format: OutputFormat,
    pub overwrite: bool,
}

pub fn fade(
    input: &Path,
    output: &Path,
    p: &FadeParams,
    cfg: &EngineConfig,
    progress: &dyn ProgressSink,
    cancel: &CancelToken,
) -> EngineResult<ProcessReport> {
    progress.set_permille(0);
    let (mut pcm, source) = read(input)?;
    let before_peak = peak_report(&pcm)?;
    let before_hash = pcm_sha256(&pcm);
    cancel.check()?;

    let (op_name, fade_type) = match p.kind {
        FadeKind::In => ("fade-in", "in"),
        FadeKind::Out => ("fade-out", "out"),
    };
    match p.kind {
        FadeKind::In => apply_fade_in(
            &mut pcm.samples,
            pcm.channels as usize,
            p.frames,
            pcm.bit_depth,
        )?,
        FadeKind::Out => apply_fade_out(
            &mut pcm.samples,
            pcm.channels as usize,
            p.frames,
            pcm.bit_depth,
        )?,
    }
    progress.set_permille(500);
    cancel.check()?;

    let after_peak = peak_report(&pcm)?;
    let write = write_verified(&pcm, output, p.format, &Vec::new(), cfg, p.overwrite)?;
    progress.set_permille(1000);

    let mut r = ProcessReport::new(op_name);
    fill_edit_common(&mut r, input, output, &source_str(source), p.format, &pcm);
    r.parameters = Some(json!({
        "fade_type": fade_type,
        "duration_samples": p.frames,
        "curve": "linear",
        "coefficient_type": "fixed-point-q31",
    }));
    r.sample_values_modified = true;
    r.peak_before_dbfs = Some(before_peak);
    r.peak_after_dbfs = Some(after_peak);
    r.pcm_verified = write.pcm_verified;
    r.input_pcm_sha256 = Some(before_hash);
    r.output_pcm_sha256 = Some(write.output_hash);
    r.finalize_log_hash();
    Ok(r)
}

// ------------------------------------------------------------ dc-correct

pub struct DcParams {
    pub format: OutputFormat,
    pub overwrite: bool,
}

pub fn dc_correct(
    input: &Path,
    output: &Path,
    p: &DcParams,
    cfg: &EngineConfig,
    progress: &dyn ProgressSink,
    cancel: &CancelToken,
) -> EngineResult<ProcessReport> {
    progress.set_permille(0);
    let (mut pcm, source) = read(input)?;
    let before_peak = peak_report(&pcm)?;
    let before_hash = pcm_sha256(&pcm);
    cancel.check()?;

    let sil = default_silence(pcm.sample_rate, pcm.bit_depth);
    let a = analyze(&pcm.samples, pcm.channels as usize, pcm.bit_depth, sil)?;
    let offsets: Vec<i64> = (0..pcm.channels as usize)
        .map(|ch| a.per_channel[ch].dc_offset(a.frames))
        .collect();
    let clipped = apply_dc_correction(
        &mut pcm.samples,
        pcm.channels as usize,
        &offsets,
        pcm.bit_depth,
    )?;
    progress.set_permille(500);
    cancel.check()?;

    let after_peak = peak_report(&pcm)?;
    let write = write_verified(&pcm, output, p.format, &Vec::new(), cfg, p.overwrite)?;
    progress.set_permille(1000);

    let mut r = ProcessReport::new("dc-correct");
    fill_edit_common(&mut r, input, output, &source_str(source), p.format, &pcm);
    r.parameters = Some(json!({ "removed_offset": offsets }));
    r.sample_values_modified = true;
    r.clipped_samples = clipped;
    r.peak_before_dbfs = Some(before_peak);
    r.peak_after_dbfs = Some(after_peak);
    r.pcm_verified = write.pcm_verified;
    r.input_pcm_sha256 = Some(before_hash);
    r.output_pcm_sha256 = Some(write.output_hash);
    r.finalize_log_hash();
    Ok(r)
}

// ------------------------------------------------------------- export16

pub struct Export16Params {
    pub seed: u32,
    pub format: OutputFormat,
    pub overwrite: bool,
}

pub fn export16(
    input: &Path,
    output: &Path,
    p: &Export16Params,
    cfg: &EngineConfig,
    progress: &dyn ProgressSink,
    cancel: &CancelToken,
) -> EngineResult<ProcessReport> {
    progress.set_permille(0);
    let (pcm, source) = read(input)?;
    let before_peak = peak_report(&pcm)?;
    let before_hash = pcm_sha256(&pcm);
    cancel.check()?;

    let mut rng = Rng::new(p.seed);
    let (samples16, clipped) = requantize_to_16(&pcm.samples, pcm.bit_depth, &mut rng)?;
    let out_pcm = PcmBuffer {
        bit_depth: 16,
        sample_rate: pcm.sample_rate,
        channels: pcm.channels,
        samples: samples16,
    };
    progress.set_permille(500);
    cancel.check()?;

    let after_peak = peak_report(&out_pcm)?;
    let write = write_verified(&out_pcm, output, p.format, &Vec::new(), cfg, p.overwrite)?;
    progress.set_permille(1000);

    let mut r = ProcessReport::new("export16");
    r.input_file = Some(input.display().to_string());
    r.output_file = Some(output.display().to_string());
    r.input_format = Some(source.as_str().to_string());
    r.output_format = Some(output_format_str(p.format).to_string());
    r.decoded_pcm_bit_depth = pcm.bit_depth;
    r.sample_rate = pcm.sample_rate;
    r.channels = pcm.channels;
    r.parameters = Some(json!({
        "dither_type": "tpdf",
        "seed": p.seed,
        "source_bit_depth": pcm.bit_depth,
        "output_bit_depth": 16,
        "derivative_copy": true,
    }));
    r.sample_values_modified = true;
    r.requantized = true;
    r.dither_used = true;
    r.clipped_samples = clipped;
    r.peak_before_dbfs = Some(before_peak);
    r.peak_after_dbfs = Some(after_peak);
    r.pcm_verified = write.pcm_verified;
    r.input_pcm_sha256 = Some(before_hash);
    r.output_pcm_sha256 = Some(write.output_hash);
    r.finalize_log_hash();
    Ok(r)
}

// --------------------------------------------------------------- split

pub struct Segment {
    pub from_frame: u64,
    pub to_frame: u64,
    pub title: String,
}

pub struct SplitParams {
    pub segments: Vec<Segment>,
    pub album: Option<String>,
    pub artist: Option<String>,
    pub format: OutputFormat,
    pub overwrite: bool,
}

pub fn split(
    input: &Path,
    out_dir: &Path,
    p: &SplitParams,
    cfg: &EngineConfig,
    progress: &dyn ProgressSink,
    cancel: &CancelToken,
) -> EngineResult<ProcessReport> {
    progress.set_permille(0);
    let (pcm, source) = read(input)?;
    std::fs::create_dir_all(out_dir)?;
    let ext = match p.format {
        OutputFormat::Flac => "flac",
        OutputFormat::Wav => "wav",
    };

    let total = p.segments.len().max(1);
    let mut output_files = Vec::new();
    let mut track_reports = Vec::new();
    let mut all_verified = true;

    for (i, seg) in p.segments.iter().enumerate() {
        cancel.check()?;
        let track_no = i + 1;
        let slice = frame_slice(
            &pcm.samples,
            pcm.channels as usize,
            seg.from_frame,
            seg.to_frame,
        )?;
        let track_pcm = PcmBuffer {
            bit_depth: pcm.bit_depth,
            sample_rate: pcm.sample_rate,
            channels: pcm.channels,
            samples: slice.to_vec(),
        };
        let filename = track_filename(track_no, &seg.title, ext);
        let out_path = out_dir.join(&filename);
        let tags = build_tags(
            track_no,
            &seg.title,
            p.album.as_deref(),
            p.artist.as_deref(),
        );
        let write = write_verified(&track_pcm, &out_path, p.format, &tags, cfg, p.overwrite)?;
        all_verified &= write.pcm_verified;

        track_reports.push(json!({
            "track": track_no,
            "title": seg.title,
            "from_sample": seg.from_frame,
            "to_sample": seg.to_frame,
            "file": out_path.display().to_string(),
            "pcm_sha256": write.output_hash,
        }));
        output_files.push(out_path.display().to_string());
        progress.set_permille((track_no * 1000 / total) as u32);
    }

    let concatenation_verified = is_contiguous_partition(&p.segments);

    let mut r = ProcessReport::new("split");
    r.input_file = Some(input.display().to_string());
    r.output_files = output_files;
    r.input_format = Some(source.as_str().to_string());
    r.output_format = Some(output_format_str(p.format).to_string());
    r.decoded_pcm_bit_depth = pcm.bit_depth;
    r.sample_rate = pcm.sample_rate;
    r.channels = pcm.channels;
    r.parameters = Some(json!({
        "track_count": p.segments.len(),
        "tracks": track_reports,
        "concatenation_verified": concatenation_verified,
    }));
    r.pcm_verified = all_verified;
    r.input_pcm_sha256 = Some(pcm_sha256(&pcm));
    r.finalize_log_hash();
    Ok(r)
}

// --------------------------------------------------------------- verify

/// Checksum one file, or compare two. Returns the report plus a mismatch
/// description when two differing files are compared.
pub fn verify(a: &Path, b: Option<&Path>) -> EngineResult<(ProcessReport, Option<String>)> {
    let (pcm_a, fmt_a) = read(a)?;
    let hash_a = pcm_sha256(&pcm_a);

    let mut r = ProcessReport::new("verify");
    r.input_file = Some(a.display().to_string());
    r.input_format = Some(fmt_a.as_str().to_string());
    r.decoded_pcm_bit_depth = pcm_a.bit_depth;
    r.sample_rate = pcm_a.sample_rate;
    r.channels = pcm_a.channels;
    r.input_pcm_sha256 = Some(hash_a.clone());

    let mut mismatch = None;
    if let Some(b) = b {
        let (pcm_b, fmt_b) = read(b)?;
        let hash_b = pcm_sha256(&pcm_b);
        r.output_file = Some(b.display().to_string());
        r.output_format = Some(fmt_b.as_str().to_string());
        r.output_pcm_sha256 = Some(hash_b.clone());
        let identical = hash_a == hash_b;
        r.pcm_verified = identical;
        r.parameters = Some(json!({ "pcm_identical": identical }));
        if !identical {
            mismatch = Some(describe_mismatch(&pcm_a, &pcm_b));
        }
    } else {
        r.pcm_verified = true;
    }
    r.finalize_log_hash();
    Ok((r, mismatch))
}

// ------------------------------------------------------------- helpers

fn source_str(source: intwav_codec::SourceFormat) -> String {
    source.as_str().to_string()
}

fn fill_edit_common(
    r: &mut ProcessReport,
    input: &Path,
    output: &Path,
    source: &str,
    format: OutputFormat,
    pcm: &PcmBuffer,
) {
    r.input_file = Some(input.display().to_string());
    r.output_file = Some(output.display().to_string());
    r.input_format = Some(source.to_string());
    r.output_format = Some(output_format_str(format).to_string());
    r.decoded_pcm_bit_depth = pcm.bit_depth;
    r.sample_rate = pcm.sample_rate;
    r.channels = pcm.channels;
}

fn is_contiguous_partition(segments: &[Segment]) -> bool {
    if segments.is_empty() {
        return false;
    }
    for w in segments.windows(2) {
        if w[0].to_frame != w[1].from_frame {
            return false;
        }
    }
    true
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

fn describe_mismatch(a: &PcmBuffer, b: &PcmBuffer) -> String {
    let _ = format_dbfs; // (kept available for future detailed reports)
    if a.bit_depth != b.bit_depth || a.sample_rate != b.sample_rate || a.channels != b.channels {
        return format!(
            "stream shape differs ({}-bit/{}Hz/{}ch vs {}-bit/{}Hz/{}ch)",
            a.bit_depth, a.sample_rate, a.channels, b.bit_depth, b.sample_rate, b.channels
        );
    }
    if a.samples.len() != b.samples.len() {
        return format!(
            "length differs ({} vs {} samples)",
            a.samples.len(),
            b.samples.len()
        );
    }
    match a.samples.iter().zip(&b.samples).position(|(x, y)| x != y) {
        Some(i) => {
            let ch = a.channels.max(1) as usize;
            format!(
                "first difference at frame {} channel {} ({} vs {})",
                i / ch,
                i % ch,
                a.samples[i],
                b.samples[i]
            )
        }
        None => "unknown".to_string(),
    }
}
