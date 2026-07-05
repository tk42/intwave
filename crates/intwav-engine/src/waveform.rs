//! Integer min/max mipmap pyramid for waveform display.
//!
//! Built incrementally via [`WaveformBuilder`] so the pyramid can form in the
//! single streaming decode pass (the same pass that writes the scratch file).
//! All bucket math is integer — the waveform data itself is float-free; only the
//! eventual pixel mapping (in the frontend) uses float. Buckets are stored as
//! display-scaled `i16` (a display artifact that never feeds the save path).

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

/// Incrementally accumulates the finest waveform level frame-by-frame, then
/// derives the coarser levels on [`finish`](WaveformBuilder::finish).
pub struct WaveformBuilder {
    channels: usize,
    bit_depth: u16,
    base_bucket_frames: u64,
    factor: u32,
    max_levels: usize,
    min0: Vec<i16>,
    max0: Vec<i16>,
    cur_min: Vec<i32>,
    cur_max: Vec<i32>,
    cur_frames: u64,
}

impl WaveformBuilder {
    pub fn new(
        channels: usize,
        bit_depth: u16,
        base_bucket_frames: u64,
        factor: u32,
        max_levels: usize,
    ) -> Self {
        Self {
            channels,
            bit_depth,
            base_bucket_frames: base_bucket_frames.max(1),
            factor: factor.max(2),
            max_levels: max_levels.max(1),
            min0: Vec::new(),
            max0: Vec::new(),
            cur_min: vec![i32::MAX; channels],
            cur_max: vec![i32::MIN; channels],
            cur_frames: 0,
        }
    }

    /// Feed a block of interleaved samples (a whole number of frames).
    pub fn push_block(&mut self, samples: &[i32]) {
        if self.channels == 0 {
            return;
        }
        for frame in samples.chunks_exact(self.channels) {
            for (ch, &v) in frame.iter().enumerate() {
                if v < self.cur_min[ch] {
                    self.cur_min[ch] = v;
                }
                if v > self.cur_max[ch] {
                    self.cur_max[ch] = v;
                }
            }
            self.cur_frames += 1;
            if self.cur_frames >= self.base_bucket_frames {
                self.flush_bucket();
            }
        }
    }

    fn flush_bucket(&mut self) {
        if self.cur_frames == 0 {
            return;
        }
        for ch in 0..self.channels {
            self.min0.push(scale(self.cur_min[ch], self.bit_depth));
            self.max0.push(scale(self.cur_max[ch], self.bit_depth));
            self.cur_min[ch] = i32::MAX;
            self.cur_max[ch] = i32::MIN;
        }
        self.cur_frames = 0;
    }

    /// Flush the trailing partial bucket and build the coarser levels.
    pub fn finish(mut self) -> WaveformPyramid {
        self.flush_bucket();

        let mut levels = vec![WaveformLevel {
            bucket_frames: self.base_bucket_frames,
            channels: self.channels,
            min: self.min0,
            max: self.max0,
        }];

        while levels.len() < self.max_levels {
            let prev = levels.last().unwrap();
            let pb = prev.buckets();
            if pb <= 1 {
                break;
            }
            let mut mn: Vec<i16> = Vec::new();
            let mut mx: Vec<i16> = Vec::new();
            let mut b = 0usize;
            while b < pb {
                let end = (b + self.factor as usize).min(pb);
                for ch in 0..self.channels {
                    let mut cmn = i16::MAX;
                    let mut cmx = i16::MIN;
                    for bb in b..end {
                        let vmn = prev.min[bb * self.channels + ch];
                        let vmx = prev.max[bb * self.channels + ch];
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
                bucket_frames: prev.bucket_frames * self.factor as u64,
                channels: self.channels,
                min: mn,
                max: mx,
            });
        }

        WaveformPyramid {
            channels: self.channels,
            bit_depth: self.bit_depth,
            factor: self.factor,
            levels,
        }
    }
}

/// Build a waveform pyramid from a whole interleaved buffer (wraps
/// [`WaveformBuilder`]).
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
    let mut builder =
        WaveformBuilder::new(channels, bit_depth, base_bucket_frames, factor, max_levels);
    builder.push_block(samples);
    builder.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pyramid_min_max_of_ramp() {
        let samples: Vec<i32> = (0..1000).collect();
        let p = build_pyramid(&samples, 1, 24, 100, 8, 4);
        assert_eq!(p.levels[0].buckets(), 10);
        assert_eq!(p.levels[0].min[0], 0);
        assert_eq!(p.levels[0].max[9], (999i32 >> 8) as i16);
        assert!(p.levels.len() >= 2);
        assert!(p.levels[1].buckets() < p.levels[0].buckets());
    }

    #[test]
    fn stereo_channels_independent() {
        let mut samples = Vec::new();
        for i in 0..400 {
            samples.push(i * 100); // L
            samples.push(-50); // R
        }
        let p = build_pyramid(&samples, 2, 24, 100, 8, 3);
        let r_scaled = (-50i32 >> 8) as i16;
        for b in 0..p.levels[0].buckets() {
            assert_eq!(p.levels[0].min[b * 2 + 1], r_scaled);
            assert_eq!(p.levels[0].max[b * 2 + 1], r_scaled);
        }
    }

    #[test]
    fn incremental_matches_whole_buffer() {
        let samples: Vec<i32> = (0..3333).map(|i| (i * 37) % 5000 - 2500).collect();
        let whole = build_pyramid(&samples, 1, 24, 64, 8, 6);

        // Feed the same data in odd-sized blocks (frame-aligned for mono).
        let mut b = WaveformBuilder::new(1, 24, 64, 8, 6);
        for chunk in samples.chunks(97) {
            b.push_block(chunk);
        }
        let streamed = b.finish();

        assert_eq!(streamed.levels.len(), whole.levels.len());
        for (a, c) in streamed.levels.iter().zip(&whole.levels) {
            assert_eq!(a.min, c.min);
            assert_eq!(a.max, c.max);
        }
    }
}
