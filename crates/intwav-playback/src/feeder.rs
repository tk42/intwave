//! The feeder turns a [`FrameSource`] into interleaved `f32` blocks for the audio
//! device, applying the region's **integer** preview op-chain (gain + linear
//! fades, matching the export math) and converting to `f32` only at the very end.
//!
//! This is the "preview == export" guarantee (Q11): everything up to the final
//! `i32 -> f32` device conversion is the same integer arithmetic the engine
//! would use to render. The `f32` conversion is the only float, and it lives
//! here in the playback layer (off the save path, not float-scanned).

use intwav_core::{full_scale_magnitude, positive_rail, GAIN_UNITY_Q31};

use crate::error::PlaybackError;
use crate::source::FrameSource;

/// Optional preview processing applied while playing (mirrors the export ops).
#[derive(Clone, Copy, Default)]
pub struct PreviewChain {
    /// Q31 gain coefficient; `None` = unity.
    pub gain_q31: Option<i64>,
    /// Linear fade-in over the first N frames of the region.
    pub fade_in_frames: u64,
    /// Linear fade-out over the last N frames of the region.
    pub fade_out_frames: u64,
}

/// Streams `f32` frames from a source with playhead, region, looping, and the
/// preview op-chain.
pub struct Feeder<S: FrameSource> {
    source: S,
    start: u64,
    end: u64,
    pos: u64,
    chain: PreviewChain,
    looping: bool,
    channels: usize,
    bit_depth: u16,
    full_scale: f32,
}

impl<S: FrameSource> Feeder<S> {
    pub fn new(source: S, chain: PreviewChain) -> Self {
        let frames = source.frames();
        let channels = source.channels() as usize;
        let bit_depth = source.bit_depth();
        let full_scale = full_scale_magnitude(bit_depth) as f32;
        Self {
            start: 0,
            end: frames,
            pos: 0,
            chain,
            looping: false,
            channels,
            bit_depth,
            full_scale,
            source,
        }
    }

    pub fn channels(&self) -> usize {
        self.channels
    }
    pub fn sample_rate(&self) -> u32 {
        self.source.sample_rate()
    }
    pub fn position(&self) -> u64 {
        self.pos
    }
    pub fn is_finished(&self) -> bool {
        !self.looping && self.pos >= self.end
    }
    pub fn set_looping(&mut self, looping: bool) {
        self.looping = looping;
    }

    /// Restrict playback to `[start, end)` (clamped to the source), and seek to
    /// `start`.
    pub fn set_region(&mut self, start: u64, end: u64) {
        let total = self.source.frames();
        self.start = start.min(total);
        self.end = end.min(total).max(self.start);
        self.pos = self.start;
    }

    /// Seek to a frame, clamped to the region.
    pub fn seek(&mut self, frame: u64) {
        self.pos = frame.clamp(self.start, self.end);
    }

    /// Fill `out` (interleaved `f32`, length a multiple of channels) with the
    /// next frames. Returns the number of frames written; the remainder (at
    /// end-of-region without looping) is zeroed.
    pub fn fill(&mut self, out: &mut [f32]) -> Result<usize, PlaybackError> {
        let ch = self.channels;
        if ch == 0 {
            return Ok(0);
        }
        let n_frames = out.len() / ch;
        let mut written = 0usize;

        while written < n_frames {
            if self.pos >= self.end {
                if self.looping && self.end > self.start {
                    self.pos = self.start;
                } else {
                    break;
                }
            }
            let take = ((n_frames - written) as u64).min(self.end - self.pos) as usize;
            let block = self.source.read_range(self.pos, self.pos + take as u64)?;
            let len = self.end - self.start;
            for f in 0..take {
                let coeff = self.frame_coeff(self.pos - self.start, len);
                for c in 0..ch {
                    let s = block[f * ch + c] as i128;
                    let v = if coeff == GAIN_UNITY_Q31 as i128 {
                        s
                    } else {
                        saturate(round_shift(s * coeff, 31), self.bit_depth)
                    };
                    out[(written + f) * ch + c] = v as f32 / self.full_scale;
                }
                self.pos += 1;
            }
            written += take;
        }
        for x in out[written * ch..].iter_mut() {
            *x = 0.0;
        }
        Ok(written)
    }

    /// Combined Q31 coefficient for a region-relative frame position, mirroring
    /// the core gain + linear fade math.
    fn frame_coeff(&self, p: u64, len: u64) -> i128 {
        let mut c: i128 = self.chain.gain_q31.unwrap_or(GAIN_UNITY_Q31) as i128;
        let fi = self.chain.fade_in_frames.min(len);
        if fi > 0 && p < fi {
            let fin = ((p as i128) << 31) / fi as i128; // core fade-in: f/n
            c = (c * fin) >> 31;
        }
        let fo = self.chain.fade_out_frames.min(len);
        if fo > 0 && p >= len - fo {
            let rel = len - p; // frames remaining incl. current
            let fout = ((rel as i128) << 31) / fo as i128; // core fade-out: (n-p)/n
            c = (c * fout) >> 31;
        }
        c
    }

    /// Test/offline helper: render the whole region to a single `f32` buffer.
    pub fn render_region(&mut self) -> Result<Vec<f32>, PlaybackError> {
        self.seek(self.start);
        let mut out = Vec::new();
        let mut buf = vec![0.0f32; self.channels * 1024];
        loop {
            let n = self.fill(&mut buf)?;
            out.extend_from_slice(&buf[..n * self.channels]);
            if n < 1024 {
                break;
            }
        }
        Ok(out)
    }
}

fn round_shift(v: i128, shift: u32) -> i128 {
    let half = 1i128 << (shift - 1);
    if v >= 0 {
        (v + half) >> shift
    } else {
        -(((-v) + half) >> shift)
    }
}

fn saturate(v: i128, bit_depth: u16) -> i128 {
    let max = positive_rail(bit_depth) as i128;
    let min = -(full_scale_magnitude(bit_depth) as i128);
    v.clamp(min, max)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::BufferSource;
    use intwav_codec::PcmBuffer;
    use intwav_core::gain_q31_for_db;

    fn buf(samples: Vec<i32>, ch: u16) -> BufferSource {
        BufferSource::new(PcmBuffer {
            bit_depth: 24,
            sample_rate: 48_000,
            channels: ch,
            samples,
        })
    }

    #[test]
    fn unity_conversion_maps_full_scale() {
        // Mono: +full-scale-ish and -full-scale.
        let fs = 1 << 23;
        let src = buf(vec![fs - 1, -fs, 0], 1);
        let mut feeder = Feeder::new(src, PreviewChain::default());
        let out = feeder.render_region().unwrap();
        assert!((out[0] - ((fs - 1) as f32 / fs as f32)).abs() < 1e-6);
        assert!((out[1] - (-1.0)).abs() < 1e-6);
        assert_eq!(out[2], 0.0);
    }

    #[test]
    fn gain_preview_matches_integer_math() {
        // -6 dB on 1000 -> 501 (same as the engine), then /full_scale.
        let src = buf(vec![1000; 4], 1);
        let chain = PreviewChain {
            gain_q31: gain_q31_for_db(-6),
            ..Default::default()
        };
        let mut feeder = Feeder::new(src, chain);
        let out = feeder.render_region().unwrap();
        let expected = 501.0f32 / (1 << 23) as f32;
        for &v in &out {
            assert!((v - expected).abs() < 1e-6);
        }
    }

    #[test]
    fn fade_in_starts_silent() {
        let src = buf(vec![1000; 100], 1);
        let chain = PreviewChain {
            fade_in_frames: 100,
            ..Default::default()
        };
        let mut feeder = Feeder::new(src, chain);
        let out = feeder.render_region().unwrap();
        assert_eq!(out[0], 0.0); // fade-in coeff 0 at frame 0
        assert!(out[50] > 0.0 && out[50] < out[99]); // ramps up
    }

    #[test]
    fn seek_and_partial_fill_zero_pads() {
        let src = buf((0..10).collect(), 1);
        let mut feeder = Feeder::new(src, PreviewChain::default());
        feeder.seek(8);
        let mut out = vec![9.9f32; 5]; // request 5 frames, only 2 remain
        let n = feeder.fill(&mut out).unwrap();
        assert_eq!(n, 2);
        assert_eq!(out[2], 0.0); // zero-padded
        assert_eq!(out[3], 0.0);
    }
}
