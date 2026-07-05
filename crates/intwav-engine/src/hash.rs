//! SHA-256 helpers for report checksums and the processing-log hash (§22).

use intwav_codec::PcmBuffer;
use sha2::{Digest, Sha256};

/// SHA-256 of interleaved PCM, hashing each sample as little-endian `i32`.
/// A container-independent fingerprint of the sample stream: decode two files
/// and compare hashes to prove identical PCM.
pub fn pcm_sha256(pcm: &PcmBuffer) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"intwav-pcm-v1");
    hasher.update([pcm.bit_depth as u8]);
    hasher.update([pcm.channels as u8]);
    hasher.update(pcm.sample_rate.to_le_bytes());
    for &s in &pcm.samples {
        hasher.update(s.to_le_bytes());
    }
    hex(&hasher.finalize())
}

/// SHA-256 of a subrange of interleaved PCM (for `source_range_hash`).
pub fn pcm_slice_sha256(
    bit_depth: u16,
    sample_rate: u32,
    channels: u16,
    samples: &[i32],
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"intwav-pcm-v1");
    hasher.update([bit_depth as u8]);
    hasher.update([channels as u8]);
    hasher.update(sample_rate.to_le_bytes());
    for &s in samples {
        hasher.update(s.to_le_bytes());
    }
    hex(&hasher.finalize())
}

/// SHA-256 of an arbitrary UTF-8 string (the processing-log hash).
pub fn text_sha256(s: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(s.as_bytes());
    hex(&hasher.finalize())
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push_str(&format!("{b:02x}"));
    }
    out
}
