//! JSON processing reports (spec §13, strengthened for §20/§22).
//!
//! A single [`OpReport`] shape covers every operation. Boolean invariants
//! document that the tool used no float / dither / resample / requantize unless
//! explicitly stated, and the optional hash fields provide PCM checksums and a
//! processing-log hash for archival provenance.

use std::path::Path;

use anyhow::Context;
use serde::Serialize;

pub const TOOL_NAME: &str = "intwav";
pub const TOOL_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Peak dBFS per channel, as strings (matching the spec example).
#[derive(Debug, Clone, Serialize)]
pub struct PeakDbfs {
    pub left: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub right: Option<String>,
}

/// Unified processing report. Fields that do not apply to an operation are
/// omitted from the JSON.
#[derive(Debug, Default, Serialize)]
pub struct OpReport {
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
    pub sample_values_modified: bool,
    pub floating_point_used: bool,
    pub dither_used: bool,
    pub resampled: bool,
    pub requantized: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub peak_before_dbfs: Option<PeakDbfs>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub peak_after_dbfs: Option<PeakDbfs>,
    pub clipped_samples: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_pcm_sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_pcm_sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub processing_log_sha256: Option<String>,
}

impl OpReport {
    /// A report pre-filled with the tool identity and the float-free invariant.
    pub fn new(operation: &str) -> Self {
        Self {
            tool: TOOL_NAME,
            version: TOOL_VERSION,
            operation: operation.to_string(),
            floating_point_used: false,
            ..Default::default()
        }
    }

    /// Compute `processing_log_sha256` over the canonical report (with the hash
    /// field itself excluded), then store it.
    pub fn finalize_log_hash(&mut self) {
        self.processing_log_sha256 = None;
        let canonical = serde_json::to_string(self).unwrap_or_default();
        self.processing_log_sha256 = Some(crate::hash::text_sha256(&canonical));
    }

    /// Serialize to pretty JSON at `path`.
    pub fn write(&self, path: &Path) -> anyhow::Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json).with_context(|| format!("writing report {}", path.display()))?;
        Ok(())
    }
}
