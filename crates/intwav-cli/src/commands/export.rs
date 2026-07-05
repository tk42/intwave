//! `export16` — 16-bit derivative output with TPDF dither (delegates to engine).

use std::path::Path;

use anyhow::{bail, Result};
use intwav_engine::{export16, CancelToken, Export16Params, NoProgress, OutputFormat};

use super::{engine_config, maybe_write_report, resolve_output_format};

#[allow(clippy::too_many_arguments)]
pub fn cmd_export16(
    input: &Path,
    output: &Path,
    dither: &str,
    seed: u32,
    output_format: Option<OutputFormat>,
    report: Option<&Path>,
    overwrite: bool,
) -> Result<()> {
    if !dither.eq_ignore_ascii_case("tpdf") {
        bail!("unsupported dither {dither:?} (only 'tpdf' is supported)");
    }
    let p = Export16Params {
        seed,
        format: resolve_output_format(output_format, output),
        overwrite,
    };
    let r = export16(
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
        "Exported 16-bit (TPDF dither, seed {seed}) -> {} ({} clipped) \
         [derivative output, not a preservation master]",
        output.display(),
        r.clipped_samples
    );
    Ok(())
}
