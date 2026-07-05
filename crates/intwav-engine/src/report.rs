//! Frozen v1.0 processing report (spec §13). This is a public contract shared
//! by the CLI and GUI: the common §13.2 block is typed top-level fields;
//! per-operation §13.3 specifics live under `parameters`. Field names and the
//! machine layer never localize.

use std::path::Path;

use intwav_core::NEG_INF_CB;
use serde::Serialize;

use crate::error::EngineResult;

pub const TOOL_NAME: &str = "intwav";
pub const TOOL_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Peak dBFS per channel, as strings.
#[derive(Debug, Clone, Serialize)]
pub struct PeakDbfs {
    pub left: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub right: Option<String>,
}

/// Format a centibel (1/100 dB) value as a one-decimal dB string (integer-only,
/// so the engine stays float-free). The silence sentinel renders as `"-inf"`.
pub fn format_dbfs(centibels: i32) -> String {
    if centibels == NEG_INF_CB {
        return "-inf".to_string();
    }
    let den = 10i64;
    let half = den / 2;
    let num = centibels as i64;
    let tenths = if num >= 0 {
        (num + half) / den
    } else {
        -((-num + half) / den)
    };
    let neg = tenths < 0;
    let mag = tenths.unsigned_abs();
    format!("{}{}.{}", if neg { "-" } else { "" }, mag / 10, mag % 10)
}

/// Build a per-channel [`PeakDbfs`] from centibel values.
pub fn peak_dbfs(centibels: &[i32]) -> PeakDbfs {
    let left = format_dbfs(centibels.first().copied().unwrap_or(NEG_INF_CB));
    let right = centibels.get(1).map(|&c| format_dbfs(c));
    PeakDbfs { left, right }
}

/// The unified processing report. Fields not relevant to an operation are
/// omitted from the JSON.
#[derive(Debug, Default, Serialize)]
pub struct ProcessReport {
    pub tool: &'static str,
    pub version: &'static str,
    pub operation: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_file: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub output_files: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_format: Option<String>,
    pub decoded_pcm_bit_depth: u16,
    pub sample_rate: u32,
    pub channels: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_sample: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to_sample: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<serde_json::Value>,
    // ---- §13.2 common invariant block ----
    pub sample_values_modified: bool,
    pub floating_point_used_in_save_path: bool,
    pub requantized: bool,
    pub dither_used: bool,
    pub resampled: bool,
    pub clipped_samples: u64,
    pub pcm_verified: bool,
    // ---- optional levels / checksums ----
    #[serde(skip_serializing_if = "Option::is_none")]
    pub peak_before_dbfs: Option<PeakDbfs>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub peak_after_dbfs: Option<PeakDbfs>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_pcm_sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_pcm_sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub processing_log_sha256: Option<String>,
}

impl ProcessReport {
    /// A report pre-filled with tool identity and the float-free invariant.
    pub fn new(operation: &str) -> Self {
        Self {
            tool: TOOL_NAME,
            version: TOOL_VERSION,
            operation: operation.to_string(),
            floating_point_used_in_save_path: false,
            ..Default::default()
        }
    }

    /// Compute `processing_log_sha256` over the canonical report (with the hash
    /// field excluded), then store it.
    pub fn finalize_log_hash(&mut self) {
        self.processing_log_sha256 = None;
        let canonical = serde_json::to_string(self).unwrap_or_default();
        self.processing_log_sha256 = Some(crate::hash::text_sha256(&canonical));
    }

    /// Serialize to pretty JSON at `path`.
    pub fn write(&self, path: &Path) -> EngineResult<()> {
        let json = serde_json::to_string_pretty(self).map_err(|e| {
            crate::error::EngineError::new(crate::error::ErrorCode::IoError, e.to_string())
        })?;
        std::fs::write(path, json)?;
        Ok(())
    }
}
