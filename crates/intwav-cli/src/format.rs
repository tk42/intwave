//! Presentation helpers. dBFS values arrive from the core as integer
//! centibels; formatting to a one-decimal string stays integer-only.

use intwav_core::NEG_INF_CB;

/// Format a centibel (1/100 dB) value as a one-decimal dB string, e.g.
/// `-480` -> `"-4.8"`. The silence sentinel renders as `"-inf"`.
pub fn format_dbfs(centibels: i32) -> String {
    if centibels == NEG_INF_CB {
        return "-inf".to_string();
    }
    // Round centibels to tenths of a dB (nearest, symmetric about zero).
    let tenths = div_round(centibels as i64, 10);
    let neg = tenths < 0;
    let mag = tenths.unsigned_abs();
    format!("{}{}.{}", if neg { "-" } else { "" }, mag / 10, mag % 10)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_centibels() {
        assert_eq!(format_dbfs(-480), "-4.8");
        assert_eq!(format_dbfs(-481), "-4.8");
        assert_eq!(format_dbfs(-485), "-4.9"); // rounds away from zero
        assert_eq!(format_dbfs(0), "0.0");
        assert_eq!(format_dbfs(-50), "-0.5");
        assert_eq!(format_dbfs(NEG_INF_CB), "-inf");
    }
}
