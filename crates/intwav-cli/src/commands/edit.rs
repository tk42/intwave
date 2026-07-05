//! Sample-modifying edits: gain, fades, DC correction (delegate to the engine).

use std::path::Path;

use anyhow::{Context, Result};
use intwav_engine::{
    dc_correct, fade, gain, probe, CancelToken, DcParams, FadeKind, FadeParams, GainParams,
    NoProgress, OutputFormat,
};

use super::{engine_config, maybe_write_report, resolve_output_format};
use crate::params::parse_duration_frames;

pub fn cmd_gain(
    input: &Path,
    output: &Path,
    db: i32,
    allow_clipping: bool,
    output_format: Option<OutputFormat>,
    report: Option<&Path>,
    overwrite: bool,
) -> Result<()> {
    let p = GainParams {
        db,
        allow_clipping,
        format: resolve_output_format(output_format, output),
        overwrite,
    };
    let r = gain(
        input,
        output,
        &p,
        &engine_config(),
        &NoProgress,
        &CancelToken::new(),
    )
    .map_err(anyhow::Error::new)?;
    maybe_write_report(&r, report)?;
    println!(
        "Applied {db} dB gain -> {} ({} clipped)",
        output.display(),
        r.clipped_samples
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn run_fade(
    kind: FadeKind,
    label: &str,
    input: &Path,
    output: &Path,
    duration: &str,
    output_format: Option<OutputFormat>,
    report: Option<&Path>,
    overwrite: bool,
) -> Result<()> {
    let (spec, _) = probe(input)
        .map_err(anyhow::Error::new)
        .with_context(|| format!("reading {}", input.display()))?;
    let frames =
        parse_duration_frames(duration, spec.sample_rate).map_err(|e| anyhow::anyhow!(e))?;
    let p = FadeParams {
        kind,
        frames,
        format: resolve_output_format(output_format, output),
        overwrite,
    };
    let r = fade(
        input,
        output,
        &p,
        &engine_config(),
        &NoProgress,
        &CancelToken::new(),
    )
    .map_err(anyhow::Error::new)?;
    maybe_write_report(&r, report)?;
    println!("Applied {frames}-frame {label} -> {}", output.display());
    Ok(())
}

pub fn cmd_fade_in(
    input: &Path,
    output: &Path,
    duration: &str,
    output_format: Option<OutputFormat>,
    report: Option<&Path>,
    overwrite: bool,
) -> Result<()> {
    run_fade(
        FadeKind::In,
        "fade-in",
        input,
        output,
        duration,
        output_format,
        report,
        overwrite,
    )
}

pub fn cmd_fade_out(
    input: &Path,
    output: &Path,
    duration: &str,
    output_format: Option<OutputFormat>,
    report: Option<&Path>,
    overwrite: bool,
) -> Result<()> {
    run_fade(
        FadeKind::Out,
        "fade-out",
        input,
        output,
        duration,
        output_format,
        report,
        overwrite,
    )
}

pub fn cmd_dc_correct(
    input: &Path,
    output: &Path,
    output_format: Option<OutputFormat>,
    report: Option<&Path>,
    overwrite: bool,
) -> Result<()> {
    let p = DcParams {
        format: resolve_output_format(output_format, output),
        overwrite,
    };
    let r = dc_correct(
        input,
        output,
        &p,
        &engine_config(),
        &NoProgress,
        &CancelToken::new(),
    )
    .map_err(anyhow::Error::new)?;
    maybe_write_report(&r, report)?;
    println!("Removed DC offset -> {}", output.display());
    Ok(())
}
