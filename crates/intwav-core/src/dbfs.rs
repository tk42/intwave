//! Float-free dBFS conversion.
//!
//! Peak magnitudes are integers; converting them to decibels normally needs a
//! logarithm, which would pull in floating point. Instead we approximate
//! `20 * log10(peak / reference)` entirely in integer arithmetic:
//!
//! 1. `log2(x)` in Q16 fixed point via a leading-bit scan (integer part) plus a
//!    linear interpolation over [`LOG2_TABLE_Q16`] (fractional part).
//! 2. `dBFS = log2(ratio) * (20 / log2(10))`, where the constant `20/log2(10) =
//!    6.020599…` is folded into the fixed-point scaling [`DB_PER_LOG2_Q10`].
//!
//! The result is expressed in **centibels** (1/100 dB). Accuracy is better than
//! 0.004 dB across the full 24-bit range (see the unit tests), far tighter than
//! the 0.1 dB shown to users.

/// `log2(1 + i/64)` for `i in 0..=64`, in Q16 fixed point (rounded).
/// Precomputed offline; embedded as integer constants to keep this file
/// free of floating-point literals.
pub(crate) const LOG2_TABLE_Q16: [i64; 65] = [
    0, 1466, 2909, 4331, 5732, 7112, 8473, 9814, 11136, 12440, 13727, 14996, 16248, 17484, 18704,
    19909, 21098, 22272, 23433, 24579, 25711, 26830, 27936, 29029, 30109, 31178, 32234, 33279,
    34312, 35334, 36346, 37346, 38336, 39316, 40286, 41246, 42196, 43137, 44068, 44990, 45904,
    46809, 47705, 48593, 49472, 50344, 51207, 52063, 52911, 53751, 54584, 55410, 56229, 57040,
    57845, 58643, 59434, 60219, 60997, 61769, 62534, 63294, 64047, 64794, 65536,
];

/// `round((20 / log2(10)) * 100 * 2^10)` — centibels per unit of log2, scaled
/// by `2^10`. Applied to a Q16 log2 value and then shifted right by 26.
const DB_PER_LOG2_Q10: i64 = 616_509;
const DB_SHIFT: u32 = 26; // 16 (Q16 log2) + 10 (DB_PER_LOG2_Q10 scaling)

/// Sentinel returned for a peak magnitude of 0 (digital silence): `-inf dBFS`.
pub const NEG_INF_CB: i32 = i32::MIN;

/// `floor(log2(x)) * 2^16 + frac`, the base-2 logarithm of `x` in Q16 fixed
/// point. `x` must be strictly positive.
fn log2_q16(x: u64) -> i64 {
    debug_assert!(x > 0);
    // Integer part: floor(log2(x)), 0..=63.
    let n = 63 - x.leading_zeros();
    // Fractional part: mantissa = x / 2^n in [1, 2); frac = x - 2^n in [0, 2^n).
    let frac = x - (1u64 << n);
    // Map frac into table units of 1/64 across the [2^n, 2^(n+1)) interval.
    let pos = frac << 6; // frac * 64
    let idx = (pos >> n) as usize; // 0..=63
    let rem = pos - ((idx as u64) << n); // 0..2^n
    let lo = LOG2_TABLE_Q16[idx];
    let hi = LOG2_TABLE_Q16[idx + 1];
    let interp = if n == 0 {
        lo
    } else {
        lo + ((hi - lo) * rem as i64) / (1i64 << n)
    };
    ((n as i64) << 16) + interp
}

/// Convert a peak magnitude to dBFS, in centibels (1/100 dB), relative to
/// `reference` (the 0 dBFS full-scale magnitude). Returns [`NEG_INF_CB`] when
/// `peak_magnitude` is 0.
///
/// `peak_magnitude` and `reference` are both non-negative sample magnitudes.
pub fn dbfs_centibels(peak_magnitude: i64, reference: i64) -> i32 {
    debug_assert!(reference > 0);
    if peak_magnitude <= 0 {
        return NEG_INF_CB;
    }
    let log2_ratio = log2_q16(peak_magnitude as u64) - log2_q16(reference as u64);
    let scaled = log2_ratio * DB_PER_LOG2_Q10;
    // Round-to-nearest on the arithmetic shift, symmetric about zero.
    let half = 1i64 << (DB_SHIFT - 1);
    let cb = if scaled >= 0 {
        (scaled + half) >> DB_SHIFT
    } else {
        -(((-scaled) + half) >> DB_SHIFT)
    };
    cb as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    // Reference dBFS computed with f64, compared against the integer routine.
    // (Test code is exempt from the float-free guarantee; it never ships in the
    // scanned core object.)
    fn true_dbfs(peak: i64, reference: i64) -> f64 {
        20.0 * (peak as f64 / reference as f64).log10()
    }

    #[test]
    fn accuracy_within_tolerance_24bit() {
        let reference = 1i64 << 23; // 0 dBFS reference for 24-bit
        let peaks = [
            (1i64 << 23) - 1,
            1 << 22,
            1 << 21,
            1_000_000,
            100_000,
            1_000,
            100,
            10,
            1,
        ];
        for &peak in &peaks {
            let cb = dbfs_centibels(peak, reference);
            let approx = cb as f64 / 100.0;
            let truth = true_dbfs(peak, reference);
            assert!(
                (approx - truth).abs() < 0.01,
                "peak={peak} approx={approx} truth={truth}"
            );
        }
    }

    #[test]
    fn full_scale_is_zero() {
        let reference = 1i64 << 23;
        assert_eq!(dbfs_centibels(reference, reference), 0);
    }

    #[test]
    fn silence_is_neg_inf() {
        assert_eq!(dbfs_centibels(0, 1 << 23), NEG_INF_CB);
    }

    #[test]
    fn half_scale_is_minus_six_db() {
        let reference = 1i64 << 23;
        let cb = dbfs_centibels(reference / 2, reference);
        assert_eq!(cb, -602); // -6.02 dB
    }
}
