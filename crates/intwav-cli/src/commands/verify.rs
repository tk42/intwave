//! `verify` — checksum a file, or prove two files carry identical PCM.

use std::path::Path;

use anyhow::{bail, Context, Result};
use intwav_codec::read;

use crate::hash::pcm_sha256;
use crate::report::OpReport;

pub fn cmd_verify(a: &Path, b: Option<&Path>, report_path: Option<&Path>) -> Result<()> {
    let (pcm_a, fmt_a) = read(a).with_context(|| format!("reading {}", a.display()))?;
    let hash_a = pcm_sha256(&pcm_a);
    println!("{}  {} PCM sha256 = {hash_a}", a.display(), fmt_a.as_str());

    let mut report = OpReport::new("verify");
    report.input_file = Some(a.display().to_string());
    report.input_format = Some(fmt_a.as_str().to_string());
    report.decoded_pcm_bit_depth = pcm_a.bit_depth;
    report.sample_rate = pcm_a.sample_rate;
    report.channels = pcm_a.channels;
    report.input_pcm_sha256 = Some(hash_a.clone());

    let mut mismatch: Option<String> = None;

    if let Some(b) = b {
        let (pcm_b, fmt_b) = read(b).with_context(|| format!("reading {}", b.display()))?;
        let hash_b = pcm_sha256(&pcm_b);
        println!("{}  {} PCM sha256 = {hash_b}", b.display(), fmt_b.as_str());
        report.output_file = Some(b.display().to_string());
        report.output_format = Some(fmt_b.as_str().to_string());
        report.output_pcm_sha256 = Some(hash_b.clone());

        let identical = hash_a == hash_b;
        report.parameters = Some(serde_json::json!({ "pcm_identical": identical }));
        if identical {
            println!("PCM identical: yes");
        } else {
            // Locate the first differing frame for a helpful message.
            mismatch = Some(describe_mismatch(&pcm_a, &pcm_b));
            println!("PCM identical: no");
        }
    }

    if let Some(report_path) = report_path {
        report.finalize_log_hash();
        report.write(report_path)?;
    }

    if let Some(msg) = mismatch {
        bail!("PCM mismatch: {msg}");
    }
    Ok(())
}

fn describe_mismatch(a: &intwav_codec::PcmBuffer, b: &intwav_codec::PcmBuffer) -> String {
    if a.bit_depth != b.bit_depth || a.sample_rate != b.sample_rate || a.channels != b.channels {
        return format!(
            "stream shape differs ({}-bit/{}Hz/{}ch vs {}-bit/{}Hz/{}ch)",
            a.bit_depth, a.sample_rate, a.channels, b.bit_depth, b.sample_rate, b.channels
        );
    }
    if a.samples.len() != b.samples.len() {
        return format!(
            "length differs ({} vs {} samples)",
            a.samples.len(),
            b.samples.len()
        );
    }
    match a.samples.iter().zip(&b.samples).position(|(x, y)| x != y) {
        Some(i) => {
            let ch = a.channels.max(1) as usize;
            format!(
                "first difference at frame {} channel {} ({} vs {})",
                i / ch,
                i % ch,
                a.samples[i],
                b.samples[i]
            )
        }
        None => "unknown".to_string(),
    }
}
