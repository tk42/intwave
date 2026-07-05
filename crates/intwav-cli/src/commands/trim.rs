//! `trim` — extract a time range (delegates to the engine).

use std::path::Path;

use anyhow::{bail, Context, Result};
use intwav_engine::{probe, trim, CancelToken, NoProgress, OutputFormat, TrimParams};

use super::{engine_config, maybe_write_report, resolve_output_format};
use crate::timecode::{ns_to_frame, parse_timestamp_ns};

#[allow(clippy::too_many_arguments)]
pub fn cmd_trim(
    input: &Path,
    output: &Path,
    from: &str,
    to: &str,
    output_format: Option<OutputFormat>,
    report: Option<&Path>,
    overwrite: bool,
) -> Result<()> {
    let (spec, _fmt) = probe(input)
        .map_err(anyhow::Error::new)
        .with_context(|| format!("reading {}", input.display()))?;

    let from_ns = parse_timestamp_ns(from).map_err(|e| anyhow::anyhow!(e))?;
    let to_ns = parse_timestamp_ns(to).map_err(|e| anyhow::anyhow!(e))?;
    if to_ns < from_ns {
        bail!("--from ({from}) is after --to ({to})");
    }
    let from_frame = ns_to_frame(from_ns, spec.sample_rate);
    let to_frame = ns_to_frame(to_ns, spec.sample_rate);

    let p = TrimParams {
        from_frame,
        to_frame,
        format: resolve_output_format(output_format, output),
        overwrite,
    };
    let report_data = trim(
        input,
        output,
        &p,
        &engine_config(),
        &NoProgress,
        &CancelToken::new(),
    )
    .map_err(anyhow::Error::new)?;
    maybe_write_report(&report_data, report)?;

    println!(
        "Trimmed frames [{from_frame}, {to_frame}) -> {} ({} frames)",
        output.display(),
        to_frame - from_frame
    );
    Ok(())
}
