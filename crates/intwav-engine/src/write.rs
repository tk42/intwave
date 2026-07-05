//! Verified atomic output writes (spec §17.2 + §13 `pcm_verified`).
//!
//! Every write goes: create a temp file **in the destination directory** (so
//! `rename` is atomic on the same filesystem) → write → **re-decode and compare
//! the hash to the intended samples** → atomic `rename` into place. On any
//! failure the temp is removed and the destination is left byte-untouched.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use intwav_codec::{
    encode_flac, read_flac, read_wav, write_wav, Metadata, OutputFormat, PcmBuffer,
};

use crate::config::EngineConfig;
use crate::error::{EngineError, EngineResult, ErrorCode};
use crate::hash::pcm_sha256;

static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Result of a verified write.
pub struct WriteOutcome {
    pub output_hash: String,
    pub pcm_verified: bool,
}

/// Write `pcm` to `output` atomically, verifying the written file re-decodes to
/// exactly `pcm`'s samples. Refuses to clobber an existing file unless
/// `overwrite` is set (still atomic when it does).
pub fn write_verified(
    pcm: &PcmBuffer,
    output: &Path,
    format: OutputFormat,
    tags: &Metadata,
    cfg: &EngineConfig,
    overwrite: bool,
) -> EngineResult<WriteOutcome> {
    if output.exists() && !overwrite {
        return Err(EngineError::new(
            ErrorCode::OutputExists,
            format!("output already exists: {}", output.display()),
        ));
    }

    let intended = pcm_sha256(pcm);
    let dir = match output.parent() {
        Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
        _ => PathBuf::from("."),
    };
    let fname = output
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "out".to_string());
    let n = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let tmp = dir.join(format!(".{fname}.iwtmp.{}.{n}", std::process::id()));

    let outcome = write_and_verify(pcm, &tmp, format, tags, cfg, &intended);
    match outcome {
        Ok(outcome) => match std::fs::rename(&tmp, output) {
            Ok(()) => Ok(outcome),
            Err(e) => {
                let _ = std::fs::remove_file(&tmp);
                Err(EngineError::from(e))
            }
        },
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            Err(e)
        }
    }
}

fn write_and_verify(
    pcm: &PcmBuffer,
    tmp: &Path,
    format: OutputFormat,
    tags: &Metadata,
    cfg: &EngineConfig,
    intended: &str,
) -> EngineResult<WriteOutcome> {
    match format {
        OutputFormat::Wav => write_wav(pcm, tmp)?,
        OutputFormat::Flac => encode_flac(pcm, tmp, tags, cfg.flac_exe.as_os_str())?,
    };
    // Re-decode by explicit format (the temp file has no meaningful extension).
    let decoded = match format {
        OutputFormat::Wav => read_wav(tmp)?,
        OutputFormat::Flac => read_flac(tmp)?,
    };
    let output_hash = pcm_sha256(&decoded);
    if output_hash != *intended {
        return Err(EngineError::new(
            ErrorCode::PcmVerifyFailed,
            "written output did not re-decode to the intended samples",
        ));
    }
    Ok(WriteOutcome {
        output_hash,
        pcm_verified: true,
    })
}
