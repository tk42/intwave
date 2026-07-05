//! SHA-256 helpers for report checksums and the processing-log hash (§22).

use intwav_codec::PcmBuffer;
use sha2::{Digest, Sha256};

/// Incremental hasher producing the same digest as [`pcm_sha256`], for the
/// streaming decode pass (hash while writing scratch, no second read).
pub struct PcmHasher(Sha256);

impl PcmHasher {
    pub fn new(bit_depth: u16, channels: u16, sample_rate: u32) -> Self {
        let mut h = Sha256::new();
        h.update(b"intwav-pcm-v1");
        h.update([bit_depth as u8]);
        h.update([channels as u8]);
        h.update(sample_rate.to_le_bytes());
        Self(h)
    }

    pub fn update(&mut self, samples: &[i32]) {
        for &s in samples {
            self.0.update(s.to_le_bytes());
        }
    }

    pub fn finish(self) -> String {
        hex(&self.0.finalize())
    }
}

/// SHA-256 of interleaved PCM, hashing each sample as little-endian `i32`.
/// A container-independent fingerprint of the sample stream: decode two files
/// and compare hashes to prove identical PCM.
pub fn pcm_sha256(pcm: &PcmBuffer) -> String {
    let mut hasher = PcmHasher::new(pcm.bit_depth, pcm.channels, pcm.sample_rate);
    hasher.update(&pcm.samples);
    hasher.finish()
}

/// SHA-256 of a subrange of interleaved PCM (for `source_range_hash`).
pub fn pcm_slice_sha256(
    bit_depth: u16,
    sample_rate: u32,
    channels: u16,
    samples: &[i32],
) -> String {
    let mut hasher = PcmHasher::new(bit_depth, channels, sample_rate);
    hasher.update(samples);
    hasher.finish()
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
