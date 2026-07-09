//! The non-destructive project document (spec §11 / Q8-B, Q9, Q15).
//!
//! The engine owns this — it is the single source of truth for an editing
//! session, shared verbatim by the CLI and GUI. Editing never touches audio;
//! it manipulates this document, and rendering (see `render.rs`) reads the
//! immutable sources through the canonical integer op-chain. All fields are
//! integer/string (no float) so the machine layer stays culture- and
//! platform-neutral and the engine stays float-free.

use serde::{Deserialize, Serialize};

pub const PROJECT_VERSION: &str = "1.0";

fn default_version() -> String {
    PROJECT_VERSION.to_string()
}

/// An immutable source file referenced by the project. Relative-path-first so a
/// `project + sources/` folder can move as a unit (Q15).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SourceRef {
    pub id: String,
    pub relative_path: String,
    pub last_known_absolute_path: String,
    pub pcm_sha256: String,
    pub sample_rate: u32,
    pub bit_depth: u16,
    pub channels: u16,
    pub frames: u64,
}

/// The per-region processing chain, in **fixed canonical order** (Q12): the
/// struct shape *is* the order — there is no way to reorder stages. Requantization
/// (`export16_seed`) is always terminal.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct OpChain {
    #[serde(default)]
    pub dc_correct: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gain_db: Option<i32>,
    #[serde(default)]
    pub fade_in_frames: u64,
    #[serde(default)]
    pub fade_out_frames: u64,
    /// Terminal 16-bit requantization with TPDF dither (derivative only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub export16_seed: Option<u32>,
}

impl OpChain {
    /// True when the chain leaves sample values unchanged (trim/split only).
    pub fn is_lossless(&self) -> bool {
        !self.dc_correct
            && self.gain_db.is_none()
            && self.fade_in_frames == 0
            && self.fade_out_frames == 0
            && self.export16_seed.is_none()
    }

    pub fn modifies_samples(&self) -> bool {
        !self.is_lossless()
    }

    /// True when the chain reduces bit depth (bars it from a Master export).
    pub fn requantizes(&self) -> bool {
        self.export16_seed.is_some()
    }
}

/// A region: a slice of a source plus a processing chain and track metadata.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Region {
    pub id: String,
    pub source_id: String,
    pub start_frame: u64,
    pub end_frame: u64,
    #[serde(default)]
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub track_number: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artist: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub album: Option<String>,
    #[serde(default)]
    pub ops: OpChain,
}

/// A named position marker (frame index).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Marker {
    pub id: String,
    pub frame: u64,
    #[serde(default)]
    pub label: String,
}

/// One append-only provenance record (Q16): what was done, for archival audit.
/// Distinct from the volatile undo stack.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProvenanceEntry {
    pub action: String,
    #[serde(default)]
    pub detail: String,
}

/// The whole project. Serialized to `.iwproj` as versioned JSON.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Document {
    #[serde(default = "default_version")]
    pub project_version: String,
    #[serde(default)]
    pub sources: Vec<SourceRef>,
    #[serde(default)]
    pub markers: Vec<Marker>,
    #[serde(default)]
    pub regions: Vec<Region>,
    #[serde(default)]
    pub history: Vec<ProvenanceEntry>,
}

impl Default for Document {
    fn default() -> Self {
        Self {
            project_version: default_version(),
            sources: Vec::new(),
            markers: Vec::new(),
            regions: Vec::new(),
            history: Vec::new(),
        }
    }
}

impl Document {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn source(&self, id: &str) -> Option<&SourceRef> {
        self.sources.iter().find(|s| s.id == id)
    }

    pub fn region(&self, id: &str) -> Option<&Region> {
        self.regions.iter().find(|r| r.id == id)
    }

    pub(crate) fn region_mut(&mut self, id: &str) -> Option<&mut Region> {
        self.regions.iter_mut().find(|r| r.id == id)
    }

    pub(crate) fn marker_mut(&mut self, id: &str) -> Option<&mut Marker> {
        self.markers.iter_mut().find(|m| m.id == id)
    }

    /// Regions ordered by track number then start frame (a convenience view;
    /// the stored order is authoritative for export ordering).
    pub fn regions_in_order(&self) -> Vec<&Region> {
        let mut v: Vec<&Region> = self.regions.iter().collect();
        v.sort_by_key(|r| (r.track_number.unwrap_or(u32::MAX), r.start_frame));
        v
    }
}
