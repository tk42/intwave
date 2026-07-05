//! `export16` — 24/32-bit to 16-bit derivative output with TPDF dither.
//!
//! This is a derivative/distribution output, NOT a preservation master (spec
//! §11.6): it deliberately changes sample values (requantization + dither).

use std::path::Path;

use anyhow::{bail, Context, Result};
use intwav_codec::{read, OutputFormat, PcmBuffer};
use intwav_core::{requantize_to_16, Rng};
use serde_json::json;

use super::{analyze_pcm, build_peak_dbfs, output_format_str, resolve_output_format, write_output};
use crate::hash::pcm_sha256;
use crate::report::OpReport;

pub fn cmd_export16(
    input: &Path,
    output: &Path,
    dither: &str,
    seed: u32,
    output_format: Option<OutputFormat>,
    report_path: Option<&Path>,
) -> Result<()> {
    if !dither.eq_ignore_ascii_case("tpdf") {
        bail!("unsupported dither {dither:?} (only 'tpdf' is supported)");
    }
    let (pcm, source) = read(input).with_context(|| format!("reading {}", input.display()))?;

    let before = report_path
        .is_some()
        .then(|| Ok::<_, anyhow::Error>((analyze_pcm(&pcm)?, pcm_sha256(&pcm))))
        .transpose()?;

    let mut rng = Rng::new(seed);
    let (samples16, clipped) = requantize_to_16(&pcm.samples, pcm.bit_depth, &mut rng)?;
    let out_pcm = PcmBuffer {
        bit_depth: 16,
        sample_rate: pcm.sample_rate,
        channels: pcm.channels,
        samples: samples16,
    };

    let fmt = resolve_output_format(output_format, output);
    write_output(&out_pcm, output, fmt, &Vec::new())?;

    println!(
        "Exported 16-bit (TPDF dither, seed {seed}) -> {} ({clipped} clipped) \
         [derivative output, not a preservation master]",
        output.display()
    );

    if let (Some(report_path), Some((before_analysis, before_hash))) = (report_path, before) {
        let after_analysis = analyze_pcm(&out_pcm)?;
        let mut report = OpReport::new("export16");
        report.input_file = Some(input.display().to_string());
        report.output_file = Some(output.display().to_string());
        report.input_format = Some(source.as_str().to_string());
        report.output_format = Some(output_format_str(fmt).to_string());
        report.decoded_pcm_bit_depth = pcm.bit_depth;
        report.sample_rate = pcm.sample_rate;
        report.channels = pcm.channels;
        report.parameters = Some(json!({
            "dither": "tpdf",
            "seed": seed,
            "source_bit_depth": pcm.bit_depth,
            "target_bit_depth": 16,
            "derivative_output": true,
        }));
        report.sample_values_modified = true;
        report.dither_used = true;
        report.requantized = true;
        report.peak_before_dbfs = Some(build_peak_dbfs(&before_analysis, pcm.channels));
        report.peak_after_dbfs = Some(build_peak_dbfs(&after_analysis, out_pcm.channels));
        report.clipped_samples = clipped;
        report.input_pcm_sha256 = Some(before_hash);
        report.output_pcm_sha256 = Some(pcm_sha256(&out_pcm));
        report.finalize_log_hash();
        report.write(report_path)?;
    }
    Ok(())
}
