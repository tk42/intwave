//! Fixed-point sample processing: gain, linear fades, and DC-offset removal.
//! All arithmetic is integer; gain coefficients come from a precomputed Q31
//! table (no float, no `pow`). Every operation saturates to the bit-depth rails
//! and reports how many samples clipped.

use crate::{full_scale_magnitude, positive_rail, CoreError};

// Q31 gain coefficients for integer dB values (coeff = round(10^(dB/20) * 2^31)).
// Precomputed offline and embedded as integer constants so this file contains
// no floating point. 0 dB == 2^31 (unity).
const GAIN_DB_MIN: i32 = -96;
const GAIN_DB_MAX: i32 = 24;
#[rustfmt::skip]
const GAIN_Q31: [i64; 121] = [
    34035, 38188, 42848, 48076, 53942, 60524,
    67909, 76196, 85493, 95925, 107629, 120762,
    135497, 152030, 170581, 191395, 214748, 240952,
    270352, 303340, 340353, 381883, 428479, 480762,
    539424, 605243, 679094, 761956, 854929, 959246,
    1076291, 1207619, 1354971, 1520302, 1705807, 1913947,
    2147484, 2409516, 2703522, 3033401, 3403532, 3818826,
    4284793, 4807617, 5394235, 6052431, 6790940, 7619560,
    8549286, 9592457, 10762914, 12076188, 13549706, 15203020,
    17058069, 19139468, 21474836, 24095163, 27035217, 30334013,
    34035322, 38188260, 42847932, 48076170, 53942350, 60524313,
    67909396, 76195595, 85492864, 95924571, 107629139, 120761880,
    135497058, 152030200, 170580690, 191394682, 214748365, 240951628,
    270352174, 303340128, 340353221, 381882595, 428479319, 480761704,
    539423504, 605243126, 679093957, 761955951, 854928639, 959245710,
    1076291389, 1207618800, 1354970580, 1520301996, 1705806895, 1913946816,
    2147483648, 2409516283, 2703521736, 3033401279, 3403532215, 3818825955,
    4284793195, 4807617038, 5394235037, 6052431259, 6790939566, 7619559515,
    8549286389, 9592457100, 10762913888, 12076188004, 13549705799, 15203019956,
    17058068952, 19139468159, 21474836480, 24095162834, 27035217359, 30334012793,
    34035322146,
];

/// The Q31 unity coefficient (0 dB).
pub const GAIN_UNITY_Q31: i64 = 1 << 31;

/// Q31 gain coefficient for an integer dB value, or `None` if outside the
/// supported range (`-96..=24` dB).
pub fn gain_q31_for_db(db: i32) -> Option<i64> {
    if !(GAIN_DB_MIN..=GAIN_DB_MAX).contains(&db) {
        return None;
    }
    Some(GAIN_Q31[(db - GAIN_DB_MIN) as usize])
}

/// Absolute sample magnitude corresponding to a dBFS level (`neg_db <= 0`) at a
/// given bit depth, via the Q31 gain table. Float-free (no `pow`). Returns
/// `None` if `neg_db` is positive or below the table's -96 dB floor. Useful for
/// turning a user-facing dBFS silence threshold into an integer magnitude.
pub fn magnitude_for_dbfs(bit_depth: u16, neg_db: i32) -> Option<i64> {
    if neg_db > 0 {
        return None;
    }
    let coeff = gain_q31_for_db(neg_db)?;
    let full = full_scale_magnitude(bit_depth) as i128;
    Some(((full * coeff as i128) >> 31) as i64)
}

/// Round-half-away-from-zero arithmetic right shift of a 128-bit value.
fn round_shift(v: i128, shift: u32) -> i128 {
    let half = 1i128 << (shift - 1);
    if v >= 0 {
        (v + half) >> shift
    } else {
        -(((-v) + half) >> shift)
    }
}

/// Saturate a wide value to the bit-depth rails; returns `(sample, clipped)`.
fn saturate(v: i128, bit_depth: u16) -> (i32, bool) {
    let max = positive_rail(bit_depth) as i128;
    let min = -(full_scale_magnitude(bit_depth) as i128);
    if v > max {
        (max as i32, true)
    } else if v < min {
        (min as i32, true)
    } else {
        (v as i32, false)
    }
}

/// Apply a Q31 gain coefficient to every sample in place. Returns the number of
/// samples that saturated (clipped). Coefficients above unity can clip; the
/// caller decides whether that is allowed.
pub fn apply_gain_q31(samples: &mut [i32], coeff_q31: i64, bit_depth: u16) -> u64 {
    let mut clipped = 0u64;
    for s in samples.iter_mut() {
        let prod = (*s as i128) * (coeff_q31 as i128);
        let (v, clip) = saturate(round_shift(prod, 31), bit_depth);
        if clip {
            clipped += 1;
        }
        *s = v;
    }
    clipped
}

/// Predict whether applying `coeff_q31` would clip, without mutating. Used to
/// warn before a positive-gain operation.
pub fn gain_would_clip(samples: &[i32], coeff_q31: i64, bit_depth: u16) -> u64 {
    let max = positive_rail(bit_depth) as i128;
    let min = -(full_scale_magnitude(bit_depth) as i128);
    let mut clipped = 0u64;
    for &s in samples {
        let v = round_shift((s as i128) * (coeff_q31 as i128), 31);
        if v > max || v < min {
            clipped += 1;
        }
    }
    clipped
}

fn frames_of(samples: &[i32], channels: usize) -> Result<u64, CoreError> {
    if channels == 0 {
        return Err(CoreError::ZeroChannels);
    }
    if !samples.len().is_multiple_of(channels) {
        return Err(CoreError::RaggedInterleave {
            len: samples.len(),
            channels,
        });
    }
    Ok((samples.len() / channels) as u64)
}

/// Linear fade-in over the first `fade_frames` frames (in place). Gain ramps
/// from 0 at frame 0 to unity at frame `fade_frames`; frames beyond are left
/// unchanged. If `fade_frames` exceeds the clip length it is clamped.
pub fn apply_fade_in(
    samples: &mut [i32],
    channels: usize,
    fade_frames: u64,
    bit_depth: u16,
) -> Result<(), CoreError> {
    let frames = frames_of(samples, channels)?;
    let n = fade_frames.min(frames);
    if n == 0 {
        return Ok(());
    }
    for f in 0..n {
        let coeff = ((f as i128) << 31) / n as i128; // 0 .. <unity
        apply_frame_gain(samples, channels, f as usize, coeff, bit_depth);
    }
    Ok(())
}

/// Linear fade-out over the last `fade_frames` frames (in place). Gain ramps
/// from unity down toward 0 across the tail.
pub fn apply_fade_out(
    samples: &mut [i32],
    channels: usize,
    fade_frames: u64,
    bit_depth: u16,
) -> Result<(), CoreError> {
    let frames = frames_of(samples, channels)?;
    let n = fade_frames.min(frames);
    if n == 0 {
        return Ok(());
    }
    let start = frames - n;
    for p in 0..n {
        let coeff = (((n - p) as i128) << 31) / n as i128; // unity .. ~0
        apply_frame_gain(samples, channels, (start + p) as usize, coeff, bit_depth);
    }
    Ok(())
}

fn apply_frame_gain(
    samples: &mut [i32],
    channels: usize,
    frame: usize,
    coeff_q31: i128,
    bit_depth: u16,
) {
    let base = frame * channels;
    for ch in 0..channels {
        let idx = base + ch;
        let prod = (samples[idx] as i128) * coeff_q31;
        samples[idx] = saturate(round_shift(prod, 31), bit_depth).0;
    }
}

/// Subtract a per-channel DC offset from every sample in place (saturating).
/// `offsets[ch]` is the integer mean to remove (see `ChannelStats::dc_offset`).
/// Returns the number of samples that saturated.
pub fn apply_dc_correction(
    samples: &mut [i32],
    channels: usize,
    offsets: &[i64],
    bit_depth: u16,
) -> Result<u64, CoreError> {
    frames_of(samples, channels)?;
    if offsets.len() != channels {
        return Err(CoreError::ChannelMismatch {
            expected: channels,
            got: offsets.len(),
        });
    }
    let mut clipped = 0u64;
    for (i, s) in samples.iter_mut().enumerate() {
        let ch = i % channels;
        let (v, clip) = saturate(*s as i128 - offsets[ch] as i128, bit_depth);
        if clip {
            clipped += 1;
        }
        *s = v;
    }
    Ok(clipped)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn unity_gain_is_identity() {
        let mut s = vec![100, -200, 300, (1 << 23) - 1, -(1 << 23)];
        let clipped = apply_gain_q31(&mut s, GAIN_UNITY_Q31, 24);
        assert_eq!(s, vec![100, -200, 300, (1 << 23) - 1, -(1 << 23)]);
        assert_eq!(clipped, 0);
    }

    #[test]
    fn minus_six_db_halves() {
        let c = gain_q31_for_db(-6).unwrap();
        let mut s = vec![1000, -1000, 4_000_000];
        apply_gain_q31(&mut s, c, 24);
        // ~0.5012 scaling.
        assert_eq!(s[0], 501);
        assert_eq!(s[1], -501);
        assert_eq!(s[2], 2_004_749);
    }

    #[test]
    fn positive_gain_clips_and_counts() {
        let c = gain_q31_for_db(6).unwrap();
        let near_full = (1 << 23) - 1;
        let mut s = vec![near_full, -near_full, 10];
        let clipped = apply_gain_q31(&mut s, c, 24);
        assert_eq!(clipped, 2);
        assert_eq!(s[0], (1 << 23) - 1); // saturated to positive rail
        assert_eq!(s[2], 20); // ~2x, no clip
    }

    #[test]
    fn gain_range_bounds() {
        assert!(gain_q31_for_db(-97).is_none());
        assert!(gain_q31_for_db(25).is_none());
        assert_eq!(gain_q31_for_db(0), Some(GAIN_UNITY_Q31));
    }

    #[test]
    fn fade_in_starts_silent_reaches_unity() {
        let mut s = vec![1000i32; 10]; // mono, 10 frames
        apply_fade_in(&mut s, 1, 10, 24).unwrap();
        assert_eq!(s[0], 0); // coeff 0 at frame 0
        assert!(s[1] < s[5] && s[5] < s[9]); // monotonic ramp up
        assert!(s[9] <= 1000);
    }

    #[test]
    fn fade_out_ends_quiet() {
        let mut s = vec![1000i32; 10];
        apply_fade_out(&mut s, 1, 10, 24).unwrap();
        assert_eq!(s[0], 1000); // unity at start of fade
        assert!(s[9] < s[5] && s[5] < s[0]); // monotonic ramp down
    }

    #[test]
    fn dc_correction_subtracts_mean() {
        // mono, mean +5
        let mut s = vec![5, 5, 5, 5];
        let clipped = apply_dc_correction(&mut s, 1, &[5], 24).unwrap();
        assert_eq!(s, vec![0, 0, 0, 0]);
        assert_eq!(clipped, 0);
    }
}
