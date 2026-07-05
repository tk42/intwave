//! Sample-modifying edits: gain, fades, DC-offset correction. Each applies a
//! fixed-point (integer) transform from `intwav-core` and can emit a report.

use std::path::Path;

use anyhow::{bail, Context, Result};
use intwav_codec::{read, OutputFormat, PcmBuffer};
use intwav_core::{
    apply_dc_correction, apply_fade_in, apply_fade_out, apply_gain_q31, gain_q31_for_db,
    gain_would_clip, GAIN_UNITY_Q31,
};
use serde_json::json;

use super::{analyze_pcm, build_peak_dbfs, output_format_str, resolve_output_format, write_output};
use crate::hash::pcm_sha256;
use crate::params::parse_duration_frames;
use crate::report::OpReport;

/// What an edit produced, for the report.
struct EditMeta {
    clipped: u64,
    params: serde_json::Value,
    dither_used: bool,
}

/// Shared edit pipeline: decode, capture "before" stats (only if a report is
/// requested), apply the in-place transform, write output, emit the report.
fn run_edit<F>(
    input: &Path,
    output: &Path,
    output_format: Option<OutputFormat>,
    report_path: Option<&Path>,
    operation: &str,
    apply: F,
) -> Result<()>
where
    F: FnOnce(&mut PcmBuffer) -> Result<EditMeta>,
{
    let (mut pcm, source) = read(input).with_context(|| format!("reading {}", input.display()))?;

    let want_report = report_path.is_some();
    let before = if want_report {
        Some((analyze_pcm(&pcm)?, pcm_sha256(&pcm)))
    } else {
        None
    };

    let meta = apply(&mut pcm)?;

    let fmt = resolve_output_format(output_format, output);
    write_output(&pcm, output, fmt, &Vec::new())?;

    if let (Some(report_path), Some((before_analysis, before_hash))) = (report_path, before) {
        let after_analysis = analyze_pcm(&pcm)?;
        let mut report = OpReport::new(operation);
        report.input_file = Some(input.display().to_string());
        report.output_file = Some(output.display().to_string());
        report.input_format = Some(source.as_str().to_string());
        report.output_format = Some(output_format_str(fmt).to_string());
        report.decoded_pcm_bit_depth = pcm.bit_depth;
        report.sample_rate = pcm.sample_rate;
        report.channels = pcm.channels;
        report.parameters = Some(meta.params);
        report.sample_values_modified = true;
        report.dither_used = meta.dither_used;
        report.peak_before_dbfs = Some(build_peak_dbfs(&before_analysis, pcm.channels));
        report.peak_after_dbfs = Some(build_peak_dbfs(&after_analysis, pcm.channels));
        report.clipped_samples = meta.clipped;
        report.input_pcm_sha256 = Some(before_hash);
        report.output_pcm_sha256 = Some(pcm_sha256(&pcm));
        report.finalize_log_hash();
        report.write(report_path)?;
    }
    Ok(())
}

pub fn cmd_gain(
    input: &Path,
    output: &Path,
    db: i32,
    allow_clipping: bool,
    output_format: Option<OutputFormat>,
    report_path: Option<&Path>,
) -> Result<()> {
    let coeff = gain_q31_for_db(db)
        .with_context(|| format!("unsupported gain {db} dB (supported range is -96..=24)"))?;
    run_edit(input, output, output_format, report_path, "gain", |pcm| {
        // Positive gain can clip: require explicit consent (spec §11.4).
        if coeff > GAIN_UNITY_Q31 {
            let would = gain_would_clip(&pcm.samples, coeff, pcm.bit_depth);
            if would > 0 && !allow_clipping {
                bail!(
                    "gain of {db} dB would clip {would} sample(s); \
                     re-run with --allow-clipping to proceed"
                );
            }
        }
        let clipped = apply_gain_q31(&mut pcm.samples, coeff, pcm.bit_depth);
        println!(
            "Applied {db} dB gain -> {} ({clipped} clipped)",
            output.display()
        );
        Ok(EditMeta {
            clipped,
            params: json!({ "db": db, "allow_clipping": allow_clipping }),
            dither_used: false,
        })
    })
}

pub fn cmd_fade_in(
    input: &Path,
    output: &Path,
    duration: &str,
    output_format: Option<OutputFormat>,
    report_path: Option<&Path>,
) -> Result<()> {
    let duration = duration.to_string();
    run_edit(
        input,
        output,
        output_format,
        report_path,
        "fade-in",
        |pcm| {
            let frames = parse_duration_frames(&duration, pcm.sample_rate)
                .map_err(|e| anyhow::anyhow!(e))?;
            apply_fade_in(
                &mut pcm.samples,
                pcm.channels as usize,
                frames,
                pcm.bit_depth,
            )?;
            println!("Applied {frames}-frame fade-in -> {}", output.display());
            Ok(EditMeta {
                clipped: 0,
                params: json!({ "duration": duration, "fade_frames": frames, "curve": "linear" }),
                dither_used: false,
            })
        },
    )
}

pub fn cmd_fade_out(
    input: &Path,
    output: &Path,
    duration: &str,
    output_format: Option<OutputFormat>,
    report_path: Option<&Path>,
) -> Result<()> {
    let duration = duration.to_string();
    run_edit(
        input,
        output,
        output_format,
        report_path,
        "fade-out",
        |pcm| {
            let frames = parse_duration_frames(&duration, pcm.sample_rate)
                .map_err(|e| anyhow::anyhow!(e))?;
            apply_fade_out(
                &mut pcm.samples,
                pcm.channels as usize,
                frames,
                pcm.bit_depth,
            )?;
            println!("Applied {frames}-frame fade-out -> {}", output.display());
            Ok(EditMeta {
                clipped: 0,
                params: json!({ "duration": duration, "fade_frames": frames, "curve": "linear" }),
                dither_used: false,
            })
        },
    )
}

pub fn cmd_dc_correct(
    input: &Path,
    output: &Path,
    output_format: Option<OutputFormat>,
    report_path: Option<&Path>,
) -> Result<()> {
    run_edit(
        input,
        output,
        output_format,
        report_path,
        "dc-correct",
        |pcm| {
            let analysis = analyze_pcm(pcm)?;
            let offsets: Vec<i64> = (0..pcm.channels as usize)
                .map(|ch| analysis.per_channel[ch].dc_offset(analysis.frames))
                .collect();
            let clipped = apply_dc_correction(
                &mut pcm.samples,
                pcm.channels as usize,
                &offsets,
                pcm.bit_depth,
            )?;
            println!(
                "Removed DC offset {:?} -> {} ({clipped} clipped)",
                offsets,
                output.display()
            );
            Ok(EditMeta {
                clipped,
                params: json!({ "removed_offset": offsets }),
                dither_used: false,
            })
        },
    )
}
