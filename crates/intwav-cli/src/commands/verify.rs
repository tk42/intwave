//! `verify` — checksum a file, or prove two files carry identical PCM.

use std::path::Path;

use anyhow::{bail, Result};

use super::maybe_write_report;

pub fn cmd_verify(a: &Path, b: Option<&Path>, report: Option<&Path>) -> Result<()> {
    let (r, mismatch) = intwav_engine::verify(a, b).map_err(anyhow::Error::new)?;

    if let Some(h) = &r.input_pcm_sha256 {
        println!(
            "{}  {} PCM sha256 = {h}",
            a.display(),
            r.input_format.as_deref().unwrap_or("")
        );
    }
    if let (Some(bp), Some(h)) = (b, &r.output_pcm_sha256) {
        println!(
            "{}  {} PCM sha256 = {h}",
            bp.display(),
            r.output_format.as_deref().unwrap_or("")
        );
    }
    if b.is_some() {
        println!(
            "PCM identical: {}",
            if r.pcm_verified { "yes" } else { "no" }
        );
    }

    maybe_write_report(&r, report)?;

    if let Some(m) = mismatch {
        bail!("PCM mismatch: {m}");
    }
    Ok(())
}
