//! Timestamp <-> sample-frame conversion and duration formatting, all in
//! integer arithmetic (no float).

/// Parse `HH:MM:SS.fff`, `MM:SS.fff`, or `SS.fff` into whole nanoseconds.
///
/// The fractional part may have up to 9 digits (nanosecond resolution);
/// extra digits are an error rather than silently truncated. Every component
/// after the first may be present; a bare `SS.fff` is accepted.
pub fn parse_timestamp_ns(s: &str) -> Result<u128, String> {
    let (time_part, frac_part) = match s.split_once('.') {
        Some((t, f)) => (t, Some(f)),
        None => (s, None),
    };

    // Split colon-separated fields; allow 1..=3 (S, M:S, H:M:S).
    let fields: Vec<&str> = time_part.split(':').collect();
    if fields.is_empty() || fields.len() > 3 {
        return Err(format!("invalid timestamp {s:?}"));
    }
    let mut secs: u128 = 0;
    for (i, field) in fields.iter().enumerate() {
        if field.is_empty() {
            return Err(format!("invalid timestamp {s:?}: empty field"));
        }
        let v: u128 = field
            .parse()
            .map_err(|_| format!("invalid timestamp {s:?}: {field:?} is not a number"))?;
        // Minutes/seconds must be < 60 except the most-significant field.
        if i > 0 && v >= 60 {
            return Err(format!("invalid timestamp {s:?}: {field:?} must be < 60"));
        }
        secs = secs * 60 + v;
    }

    let mut ns = secs * 1_000_000_000;
    if let Some(frac) = frac_part {
        if frac.is_empty() || frac.len() > 9 || !frac.bytes().all(|b| b.is_ascii_digit()) {
            return Err(format!(
                "invalid timestamp {s:?}: fractional part must be 1..=9 digits"
            ));
        }
        let mut frac_ns: u128 = frac
            .parse()
            .map_err(|_| format!("invalid timestamp {s:?}"))?;
        // Scale to nanoseconds by the number of missing digits.
        for _ in 0..(9 - frac.len()) {
            frac_ns *= 10;
        }
        ns += frac_ns;
    }
    Ok(ns)
}

/// Convert a time in nanoseconds to the nearest sample frame at `sample_rate`.
/// Round-half-up: `frame = round(ns * rate / 1e9)`.
pub fn ns_to_frame(ns: u128, sample_rate: u32) -> u64 {
    let rate = sample_rate as u128;
    let frame = (ns * rate + 500_000_000) / 1_000_000_000;
    frame as u64
}

/// Format a frame count at `sample_rate` as a human duration.
/// `MM:SS.mmm`, or `H:MM:SS.mmm` when the duration reaches an hour.
pub fn format_duration(frames: u64, sample_rate: u32) -> String {
    if sample_rate == 0 {
        return "0:00.000".to_string();
    }
    // Total milliseconds, rounded to nearest.
    let total_ms = (frames as u128 * 1000 + sample_rate as u128 / 2) / sample_rate as u128;
    let ms = (total_ms % 1000) as u64;
    let total_secs = (total_ms / 1000) as u64;
    let s = total_secs % 60;
    let total_mins = total_secs / 60;
    let m = total_mins % 60;
    let h = total_mins / 60;
    if h > 0 {
        format!("{h}:{m:02}:{s:02}.{ms:03}")
    } else {
        format!("{m:02}:{s:02}.{ms:03}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_full_timestamp() {
        assert_eq!(parse_timestamp_ns("00:01:23.000").unwrap(), 83_000_000_000);
        assert_eq!(parse_timestamp_ns("00:05:41.500").unwrap(), 341_500_000_000);
        assert_eq!(parse_timestamp_ns("10").unwrap(), 10_000_000_000);
        assert_eq!(parse_timestamp_ns("1:30").unwrap(), 90_000_000_000);
    }

    #[test]
    fn frac_scaling() {
        assert_eq!(parse_timestamp_ns("0.5").unwrap(), 500_000_000);
        assert_eq!(parse_timestamp_ns("0.123456789").unwrap(), 123_456_789);
    }

    #[test]
    fn rejects_bad_timestamps() {
        assert!(parse_timestamp_ns("00:99:00").is_err()); // minutes >= 60
        assert!(parse_timestamp_ns("1.2222222222").is_err()); // >9 frac digits
        assert!(parse_timestamp_ns("a:b").is_err());
        assert!(parse_timestamp_ns("::").is_err());
    }

    #[test]
    fn ns_to_frame_rounding() {
        // 10 s at 96 kHz = 960000 frames exactly.
        assert_eq!(ns_to_frame(10_000_000_000, 96_000), 960_000);
        // Half a sample rounds up.
        let half_sample_ns = 1_000_000_000u128 / 96_000 / 2 + 1;
        assert_eq!(ns_to_frame(half_sample_ns, 96_000), 1);
    }

    #[test]
    fn duration_formatting() {
        // 42:15.238 style from the spec example.
        let rate = 96_000;
        let frames = (42 * 60 + 15) * rate as u64 + rate as u64 * 238 / 1000;
        assert_eq!(format_duration(frames, rate), "42:15.238");
        // Over an hour.
        let frames_h = 3661 * rate as u64;
        assert_eq!(format_duration(frames_h, rate), "1:01:01.000");
    }
}
