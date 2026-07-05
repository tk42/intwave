//! Engine configuration injected by the host (CLI vs GUI).

use std::ffi::OsString;

/// Host-supplied settings. The key knob is the FLAC encoder location: the CLI
/// injects `"flac"` (a `PATH` lookup); the GUI injects the absolute path of its
/// bundled per-platform sidecar binary.
#[derive(Clone, Debug)]
pub struct EngineConfig {
    pub flac_exe: OsString,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            flac_exe: OsString::from("flac"),
        }
    }
}
