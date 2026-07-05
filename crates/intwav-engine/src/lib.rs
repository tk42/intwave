//! `intwav-engine` — the shared CLI/GUI engine.
//!
//! Everything on intwav's **save path** lives here or below: the operations
//! (analyze/verify/trim/split/gain/fade/dc-correct/export16), the frozen §13
//! [`ProcessReport`], the coded [`EngineError`] taxonomy, verified atomic writes
//! (`pcm_verified`), and the waveform pyramid. Operations are **synchronous and
//! caller-driven** — they take a [`ProgressSink`] and a [`CancelToken`] so the
//! CLI drives them inline and the GUI drives them from a background task.
//!
//! This crate is **float-free in source** (enforced by `scripts/check-no-float.sh`):
//! progress is integer permille, ratios are reported as raw byte/sample counts,
//! and all sample math is delegated to `intwav-core`. Only presentation layers
//! (CLI text, GUI drawing, playback) may use float — none of them are here.

mod audio;
mod config;
mod error;
mod hash;
mod ops;
mod progress;
mod report;
mod waveform;
mod write;

pub use audio::{analyze_file, default_silence, AudioReport, SilentRegion};
pub use config::EngineConfig;
pub use error::{EngineError, EngineResult, ErrorCode};
pub use hash::{pcm_sha256, pcm_slice_sha256, text_sha256};
pub use ops::{
    dc_correct, export16, fade, gain, split, trim, verify, DcParams, Export16Params, FadeKind,
    FadeParams, GainParams, Segment, SplitParams, TrimParams,
};
pub use progress::{CancelToken, FnProgress, NoProgress, ProgressSink};
pub use report::{format_dbfs, peak_dbfs, PeakDbfs, ProcessReport, TOOL_NAME, TOOL_VERSION};
pub use waveform::{build_pyramid, WaveformLevel, WaveformPyramid};

// Re-export the codec types the host needs to call ops.
pub use intwav_codec::{detect_format, probe, AudioSpec, OutputFormat, PcmBuffer, SourceFormat};
pub use intwav_core::SilenceConfig;
