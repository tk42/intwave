//! Streaming linear resampler for the device-rate fallback (Q11): used only when
//! the output device cannot open at the source's native sample rate. This is
//! preview-only and off the save path, so float is fine here — saved output is
//! never resampled (spec §12.6).

/// Linear-interpolation resampler that streams block by block, carrying one
/// frame of history across calls for seamless interpolation.
pub struct LinearResampler {
    channels: usize,
    step: f64,      // input frames advanced per output frame = in_rate / out_rate
    pos: f64,       // next output position, in input-frame coordinates of the current block
    last: Vec<f32>, // last input frame of the previous block (coordinate -1)
    primed: bool,
}

impl LinearResampler {
    pub fn new(channels: usize, in_rate: u32, out_rate: u32) -> Self {
        let out_rate = out_rate.max(1);
        Self {
            channels,
            step: in_rate as f64 / out_rate as f64,
            pos: 0.0,
            last: vec![0.0; channels],
            primed: false,
        }
    }

    pub fn is_identity(&self) -> bool {
        (self.step - 1.0).abs() < f64::EPSILON
    }

    /// Resample a block of interleaved input frames into interleaved output.
    pub fn process(&mut self, input: &[f32]) -> Vec<f32> {
        let ch = self.channels;
        if ch == 0 || input.is_empty() {
            return Vec::new();
        }
        let in_frames = input.len() / ch;
        let get = |i: i64, c: usize, last: &[f32]| -> f32 {
            if i < 0 {
                last[c]
            } else {
                input[i as usize * ch + c]
            }
        };

        let mut out = Vec::new();
        // Emit while both surrounding input frames are available in this block
        // (i0 in [-1, in_frames-2], i0+1 in [0, in_frames-1]).
        let limit = in_frames as f64 - 1.0;
        while self.pos < limit {
            let i0 = self.pos.floor() as i64;
            let frac = (self.pos - i0 as f64) as f32;
            for c in 0..ch {
                let a = get(i0, c, &self.last);
                let b = get(i0 + 1, c, &self.last);
                out.push(a + (b - a) * frac);
            }
            self.pos += self.step;
        }
        // Carry into the next block: shift coordinates back by in_frames.
        self.pos -= in_frames as f64;
        self.last.clear();
        self.last
            .extend_from_slice(&input[(in_frames - 1) * ch..in_frames * ch]);
        self.primed = true;
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_when_rates_equal() {
        let mut r = LinearResampler::new(1, 48_000, 48_000);
        assert!(r.is_identity());
        let input: Vec<f32> = (0..10).map(|i| i as f32).collect();
        let out = r.process(&input);
        // Identity has a one-frame lag but preserves values.
        for (k, &v) in out.iter().enumerate() {
            assert!((v - input[k]).abs() < 1e-5);
        }
    }

    #[test]
    fn upsample_2x_doubles_length() {
        // in 24k -> out 48k => step 0.5 => ~2 outputs per input.
        let mut r = LinearResampler::new(1, 24_000, 48_000);
        let input: Vec<f32> = (0..100).map(|i| i as f32).collect();
        let out = r.process(&input);
        // Roughly 2x, minus boundary frames.
        assert!(out.len() >= 190 && out.len() <= 200);
        // Monotonic ramp preserved.
        for w in out.windows(2) {
            assert!(w[1] >= w[0] - 1e-4);
        }
    }

    #[test]
    fn streaming_matches_across_blocks() {
        // Resampling in two halves equals resampling the whole (up to the last
        // partial frame), because the resampler carries history.
        let input: Vec<f32> = (0..200).map(|i| (i as f32 * 0.5).sin()).collect();
        let mut whole = LinearResampler::new(1, 30_000, 48_000);
        let a = whole.process(&input);

        let mut split = LinearResampler::new(1, 30_000, 48_000);
        let mut b = split.process(&input[..100]);
        b.extend(split.process(&input[100..]));

        // Compare the overlapping prefix.
        let n = a.len().min(b.len());
        for k in 0..n {
            assert!((a[k] - b[k]).abs() < 1e-4, "mismatch at {k}");
        }
    }
}
