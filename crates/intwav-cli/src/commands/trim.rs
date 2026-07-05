//! `trim` — extract a time range without altering sample values.

use std::path::Path;

use anyhow::{bail, Context, Result};
use intwav_codec::{read, OutputFormat, PcmBuffer};
use intwav_core::frame_slice;

use super::{analyze_pcm, build_peak_dbfs, output_format_str, resolve_output_format, write_output};
use crate::hash::pcm_sha256;
use crate::report::OpReport;
use crate::timecode::{ns_to_frame, parse_timestamp_ns};

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

    let fmt = resolve_output_format(output_format, output);
    write_output(&out_pcm, output, fmt, &Vec::new())?;

    if let Some(report_path) = report_path {
        // "before" statistics describe the full input prior to trimming.
        let analysis = analyze_pcm(&pcm)?;
        let mut report = OpReport::new("trim");
        report.input_file = Some(input.display().to_string());
        report.output_file = Some(output.display().to_string());
        report.input_format = Some(source.as_str().to_string());
        report.output_format = Some(output_format_str(fmt).to_string());
        report.decoded_pcm_bit_depth = pcm.bit_depth;
        report.sample_rate = pcm.sample_rate;
        report.channels = pcm.channels;
        report.from_sample = Some(from_frame);
        report.to_sample = Some(to_frame);
        report.peak_before_dbfs = Some(build_peak_dbfs(&analysis, pcm.channels));
        report.clipped_samples = analysis.total_clipped();
        report.input_pcm_sha256 = Some(pcm_sha256(&pcm));
        report.output_pcm_sha256 = Some(pcm_sha256(&out_pcm));
        report.finalize_log_hash();
        report.write(report_path)?;
    }

    println!(
        "Trimmed frames [{from_frame}, {to_frame}) -> {} ({} frames)",
        output.display(),
        to_frame - from_frame
    );
    Ok(())
}
