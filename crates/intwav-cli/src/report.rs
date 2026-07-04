//! JSON report structures (spec §13). Emitted for operations that produce
//! output (e.g. `trim`) when `--report` is given.

use serde::Serialize;

/// Peak dBFS per channel, as strings (matching the spec example).
#[derive(Debug, Serialize)]
pub struct PeakDbfs {
    pub left: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub right: Option<String>,
}

/// Report for a `trim` operation. The boolean fields document, per §13, that
/// the operation preserved sample values and used no float / dither / resample
/// / requantize.
#[derive(Debug, Serialize)]
pub struct TrimReport {
    pub tool: &'static str,
    pub version: &'static str,
    pub input_file: String,
    pub output_file: String,
    pub input_format: String,
    pub decoded_pcm_bit_depth: u16,
    pub sample_rate: u32,
    pub channels: u16,
    pub operation: &'static str,
    pub from_sample: u64,
    pub to_sample: u64,
    pub sample_values_modified: bool,
    pub floating_point_used: bool,
    pub dither_used: bool,
    pub resampled: bool,
    pub requantized: bool,
    pub peak_before_dbfs: PeakDbfs,
    pub clipped_samples: u64,
}

pub const TOOL_NAME: &str = "intwav";
pub const TOOL_VERSION: &str = env!("CARGO_PKG_VERSION");
