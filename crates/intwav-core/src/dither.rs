//! Integer TPDF dither and requantization to 16 bits.
//!
//! Going from a higher bit depth to 16 bits by truncation adds quantization
//! distortion; adding triangular-PDF dither before rounding decorrelates the
//! error (spec §9.6). The randomness comes from an integer xorshift PRNG, so no
//! floating point is involved. A fixed seed makes the output reproducible.

use alloc::vec::Vec;

use crate::{positive_rail, CoreError};

/// Deterministic integer PRNG (xorshift32). Not cryptographic — only used to
/// generate dither noise.
#[derive(Debug, Clone)]
pub struct Rng {
    state: u32,
}

impl Rng {
    /// Seed the generator. A zero seed is remapped to a fixed non-zero constant
    /// (xorshift cannot leave the zero state).
    pub fn new(seed: u32) -> Self {
        Self {
            state: if seed == 0 { 0x9E37_79B9 } else { seed },
        }
    }

    fn next_u32(&mut self) -> u32 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.state = x;
        x
    }

    /// Uniform integer in `[0, n)`. `n` must be > 0.
    fn below(&mut self, n: u32) -> u32 {
        self.next_u32() % n
    }
}

/// Round-half-away-from-zero integer division.
fn div_round(num: i64, den: i64) -> i64 {
    let half = den / 2;
    if num >= 0 {
        (num + half) / den
    } else {
        -((-num + half) / den)
    }
}

/// Requantize integer PCM to 16-bit with TPDF dither.
///
/// * `src_bit_depth` must be 16, 24, or 32. A depth of 16 is a straight copy
///   (nothing to dither).
/// * Returns the 16-bit samples (as `i32` in `[-32768, 32767]`) and the number
///   of samples that saturated.
pub fn requantize_to_16(
    samples: &[i32],
    src_bit_depth: u16,
    rng: &mut Rng,
) -> Result<(Vec<i32>, u64), CoreError> {
    if !matches!(src_bit_depth, 16 | 24 | 32) {
        return Err(CoreError::UnsupportedBitDepth(src_bit_depth));
    }
    if src_bit_depth == 16 {
        return Ok((samples.to_vec(), 0));
    }

    let shift = src_bit_depth - 16;
    let step = 1i64 << shift; // number of source codes per 16-bit code
    let max = positive_rail(16); // 32767
    let min = -(1i64 << 15); // -32768

    let mut out = Vec::with_capacity(samples.len());
    let mut clipped = 0u64;
    for &s in samples {
        // TPDF dither: difference of two independent uniforms in [0, step),
        // giving a triangular distribution over (-step, step).
        let n1 = rng.below(step as u32) as i64;
        let n2 = rng.below(step as u32) as i64;
        let dithered = s as i64 + (n1 - n2);
        let q = div_round(dithered, step);
        let v = if q > max {
            clipped += 1;
            max as i32
        } else if q < min {
            clipped += 1;
            min as i32
        } else {
            q as i32
        };
        out.push(v);
    }
    Ok((out, clipped))
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn passthrough_for_16bit() {
        let mut rng = Rng::new(1);
        let s = vec![1, -2, 3, 32767, -32768];
        let (out, clipped) = requantize_to_16(&s, 16, &mut rng).unwrap();
        assert_eq!(out, s);
        assert_eq!(clipped, 0);
    }

    #[test]
    fn requantize_24_to_16_is_near_shift() {
        // A 24-bit value of 256*k dithers to ~k in 16-bit.
        let mut rng = Rng::new(12345);
        let s = vec![256 * 100, 256 * -100, 256 * 1000];
        let (out, _clipped) = requantize_to_16(&s, 24, &mut rng).unwrap();
        // Dither is < 1 LSB, so each result is within 1 of the ideal shift.
        assert!((out[0] - 100).abs() <= 1);
        assert!((out[1] + 100).abs() <= 1);
        assert!((out[2] - 1000).abs() <= 1);
    }

    #[test]
    fn deterministic_with_seed() {
        let s = vec![12345, -6789, 100000, -100000];
        let (a, _) = requantize_to_16(&s, 24, &mut Rng::new(42)).unwrap();
        let (b, _) = requantize_to_16(&s, 24, &mut Rng::new(42)).unwrap();
        assert_eq!(a, b); // same seed -> same dither
    }

    #[test]
    fn saturates_at_16bit_rails() {
        let mut rng = Rng::new(7);
        // 24-bit full scale -> would exceed 16-bit positive rail.
        let s = vec![(1 << 23) - 1];
        let (out, clipped) = requantize_to_16(&s, 24, &mut rng).unwrap();
        assert_eq!(out[0], 32767);
        assert_eq!(clipped, 1);
    }
}
