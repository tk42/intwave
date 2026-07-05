//! CLI command layer — thin wrappers over `intwav-engine`. The engine does all
//! decoding, processing, verification, and report building; this layer parses
//! arguments, formats human output, and writes the report if requested.

mod edit;
mod export;
mod inspect;
mod split;
mod trim;
mod verify;

pub use edit::{cmd_dc_correct, cmd_fade_in, cmd_fade_out, cmd_gain};
pub use export::cmd_export16;
pub use inspect::{cmd_check, cmd_clips, cmd_info, cmd_peak};
pub use split::{cmd_split, SplitMode};
pub use trim::cmd_trim;
pub use verify::cmd_verify;

use std::path::Path;

use intwav_engine::{detect_format, EngineConfig, OutputFormat, ProcessReport, SourceFormat};

/// The CLI runs FLAC through the `flac` binary on `PATH`.
pub(crate) fn engine_config() -> EngineConfig {
    EngineConfig::default()
}

/// Decide the output container: explicit flag wins, else infer from the output
/// extension, else default to FLAC (spec §11.2).
pub(crate) fn resolve_output_format(flag: Option<OutputFormat>, output: &Path) -> OutputFormat {
    if let Some(f) = flag {
        return f;
    }
    match detect_format(output) {
        Ok(SourceFormat::Wav) => OutputFormat::Wav,
        _ => OutputFormat::Flac,
    }
}

/// Channel labels for display: L/R for stereo, single unlabeled for mono.
pub(crate) fn channel_label(channels: u16, ch: usize) -> String {
    match (channels, ch) {
        (2, 0) => "L".to_string(),
        (2, 1) => "R".to_string(),
        _ => String::new(),
    }
}

pub(crate) fn label_and_space(channels: u16, ch: usize) -> (String, &'static str) {
    let label = channel_label(channels, ch);
    let space = if label.is_empty() { "" } else { " " };
    (label, space)
}

/// Write the report to `path` if one was requested.
pub(crate) fn maybe_write_report(
    report: &ProcessReport,
    path: Option<&Path>,
) -> anyhow::Result<()> {
    if let Some(path) = path {
        report
            .write(path)
            .map_err(|e| anyhow::anyhow!("writing report: {e}"))?;
    }
    Ok(())
}
