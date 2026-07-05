//! Integer min/max mipmap pyramid for waveform display.
//!
//! Built in one pass over decoded PCM (in the GUI, the same pass that writes the
//! scratch file). All bucket math is integer — the waveform data itself is
//! float-free; only the eventual pixel mapping (in the frontend) uses float.
//! Buckets are stored as display-scaled `i16` (the waveform is a display artifact
//! and never feeds the save path).

/// One level of the pyramid: `(min, max)` per bucket per channel, interleaved
/// as `index = bucket * channels + ch`.
#[derive(Debug, Clone)]
pub struct WaveformLevel {
    pub bucket_frames: u64,
    pub channels: usize,
    pub min: Vec<i16>,
    pub max: Vec<i16>,
}

impl WaveformLevel {
    pub fn buckets(&self) -> usize {
        if self.channels == 0 {
            0
        } else {
            self.min.len() / self.channels
        }
    }
}

/// A pyramid of increasingly coarse waveform levels.
#[derive(Debug, Clone)]
pub struct WaveformPyramid {
    pub channels: usize,
    pub bit_depth: u16,
    pub factor: u32,
    pub levels: Vec<WaveformLevel>,
}

/// Scale a full-precision sample down to the `i16` display range.
fn scale(v: i32, bit_depth: u16) -> i16 {
    let shift = bit_depth as i32 - 16;
    let s = if shift > 0 { v >> shift } else { v };
    s.clamp(i16::MIN as i32, i16::MAX as i32) as i16
}

/// Build a waveform pyramid from interleaved integer PCM.
///
/// * `base_bucket_frames` — frames per bucket at the finest level (e.g. 256).
/// * `factor` — aggregation ratio between levels (clamped to ≥ 2, e.g. 8).
/// * `max_levels` — cap on the number of levels.
pub fn build_pyramid(
    samples: &[i32],
    channels: usize,
    bit_depth: u16,
    base_bucket_frames: u64,
    factor: u32,
    max_levels: usize,
) -> WaveformPyramid {
    let factor = factor.max(2);
    let base = base_bucket_frames.max(1);
    let frames = if channels == 0 {
        0
    } else {
        samples.len() / channels
    };

    // Finest level directly from samples.
    let mut min0: Vec<i16> = Vec::new();
    let mut max0: Vec<i16> = Vec::new();
    let mut f = 0usize;
    while f < frames {
        let end = (f + base as usize).min(frames);
        for ch in 0..channels {
            let mut mn = i32::MAX;
            let mut mx = i32::MIN;
            for fr in f..end {
                let v = samples[fr * channels + ch];
                if v < mn {
                    mn = v;
                }
                if v > mx {
                    mx = v;
                }
            }
            min0.push(scale(mn, bit_depth));
            max0.push(scale(mx, bit_depth));
        }
        f = end;
    }

    let mut levels = vec![WaveformLevel {
        bucket_frames: base,
        channels,
        min: min0,
        max: max0,
    }];

    // Coarser levels by aggregating `factor` finer buckets (min-of-mins,
    // max-of-maxs) — pure integer downsampling.
    while levels.len() < max_levels.max(1) {
        let prev = levels.last().unwrap();
        let pb = prev.buckets();
        if pb <= 1 {
            break;
        }
        let mut mn: Vec<i16> = Vec::new();
        let mut mx: Vec<i16> = Vec::new();
        let mut b = 0usize;
        while b < pb {
            let end = (b + factor as usize).min(pb);
            for ch in 0..channels {
                let mut cmn = i16::MAX;
                let mut cmx = i16::MIN;
                for bb in b..end {
                    let vmn = prev.min[bb * channels + ch];
                    let vmx = prev.max[bb * channels + ch];
                    if vmn < cmn {
                        cmn = vmn;
                    }
                    if vmx > cmx {
                        cmx = vmx;
                    }
                }
                mn.push(cmn);
                mx.push(cmx);
            }
            b = end;
        }
        levels.push(WaveformLevel {
            bucket_frames: prev.bucket_frames * factor as u64,
            channels,
            min: mn,
            max: mx,
        });
    }

    WaveformPyramid {
        channels,
        bit_depth,
        factor,
        levels,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pyramid_min_max_of_ramp() {
        // Mono ramp 0..1000 at 24-bit, base bucket 100 frames.
        let samples: Vec<i32> = (0..1000).collect();
        let p = build_pyramid(&samples, 1, 24, 100, 8, 4);
        assert_eq!(p.levels[0].buckets(), 10);
        // Bucket 0 covers frames 0..100 -> min 0, max 99 (scaled by >>8 = 0).
        assert_eq!(p.levels[0].min[0], 0);
        // Last bucket covers 900..1000 -> max 999 >> 8 = 3.
        assert_eq!(p.levels[0].max[9], (999i32 >> 8) as i16);
        // Coarser level exists and is smaller.
        assert!(p.levels.len() >= 2);
        assert!(p.levels[1].buckets() < p.levels[0].buckets());
    }

    #[test]
    fn stereo_channels_independent() {
        // L ramps up, R is constant -50.
        let mut samples = Vec::new();
        for i in 0..400 {
            samples.push(i * 100); // L
            samples.push(-50); // R
        }
        let p = build_pyramid(&samples, 2, 24, 100, 8, 3);
        // R min/max always -50 scaled.
        let r_scaled = (-50i32 >> 8) as i16;
        for b in 0..p.levels[0].buckets() {
            assert_eq!(p.levels[0].min[b * 2 + 1], r_scaled);
            assert_eq!(p.levels[0].max[b * 2 + 1], r_scaled);
        }
    }
}
