//! `.iwproj` persistence and source resolution/verification (spec §11.4 / Q15).
//!
//! The project is versioned JSON referencing immutable sources by relative path
//! (with an absolute fallback) plus a recorded PCM hash. Opening re-derives
//! everything from the sources; a source that changed behind our back is caught
//! by the hash check.

use std::path::{Path, PathBuf};

use intwav_codec::read;

use crate::document::{Document, SourceRef};
use crate::error::{EngineError, EngineResult, ErrorCode};
use crate::hash::pcm_sha256;

/// Serialize a project to `.iwproj` (pretty JSON).
pub fn save_project(doc: &Document, path: &Path) -> EngineResult<()> {
    let json = serde_json::to_string_pretty(doc)
        .map_err(|e| EngineError::new(ErrorCode::IoError, e.to_string()))?;
    std::fs::write(path, json)?;
    Ok(())
}

/// Parse a `.iwproj` file.
pub fn open_project(path: &Path) -> EngineResult<Document> {
    let text = std::fs::read_to_string(path)?;
    serde_json::from_str(&text)
        .map_err(|e| EngineError::new(ErrorCode::ProjectParseError, e.to_string()))
}

/// Build a [`SourceRef`] by decoding a file (computes params + PCM hash).
/// `project_dir` is the directory the `.iwproj` lives in; a source under it is
/// stored as a relative path.
pub fn source_ref_from_file(id: &str, path: &Path, project_dir: &Path) -> EngineResult<SourceRef> {
    let (pcm, _fmt) = read(path)?;
    Ok(SourceRef {
        id: id.to_string(),
        relative_path: relative_path(path, project_dir),
        last_known_absolute_path: path.to_string_lossy().into_owned(),
        pcm_sha256: pcm_sha256(&pcm),
        sample_rate: pcm.sample_rate,
        bit_depth: pcm.bit_depth,
        channels: pcm.channels,
        frames: pcm.frames(),
    })
}

/// Resolve a source to an existing path — relative-to-project first, then the
/// last-known absolute path. `None` triggers a relink in the caller.
pub fn resolve_source(source: &SourceRef, project_dir: &Path) -> Option<PathBuf> {
    let rel = project_dir.join(&source.relative_path);
    if rel.exists() {
        return Some(rel);
    }
    let abs = PathBuf::from(&source.last_known_absolute_path);
    if abs.exists() {
        return Some(abs);
    }
    None
}

/// Confirm a resolved source still matches the project's recorded PCM hash.
pub fn verify_source(source: &SourceRef, resolved: &Path) -> EngineResult<()> {
    let (pcm, _fmt) = read(resolved)?;
    if pcm_sha256(&pcm) != source.pcm_sha256 {
        return Err(EngineError::new(
            ErrorCode::SourceHashMismatch,
            format!("source {:?} no longer matches the project", source.id),
        ));
    }
    Ok(())
}

fn relative_path(path: &Path, project_dir: &Path) -> String {
    match path.strip_prefix(project_dir) {
        Ok(stripped) => stripped.to_string_lossy().into_owned(),
        Err(_) => path.to_string_lossy().into_owned(),
    }
}
