//! Parsing of user-facing parameters: durations and CUE-style track lists.
//! All conversions to sample counts are integer (via `timecode`).

use crate::timecode::{ns_to_frame, parse_timestamp_ns};

/// Parse a duration into a frame count at `sample_rate`.
///
/// Accepts `<n>ms`, `<n>s` (with optional fractional seconds, e.g. `5s`,
/// `0.25s`), a bare number of seconds, or a `HH:MM:SS.mmm` timestamp.
pub fn parse_duration_frames(s: &str, sample_rate: u32) -> Result<u64, String> {
    let s = s.trim();
    let ns = if let Some(ms) = s.strip_suffix("ms") {
        let ms: u128 = ms
            .trim()
            .parse()
            .map_err(|_| format!("invalid duration {s:?}"))?;
        ms * 1_000_000
    } else if let Some(secs) = s.strip_suffix('s') {
        parse_timestamp_ns(secs.trim())?
    } else {
        parse_timestamp_ns(s)?
    };
    Ok(ns_to_frame(ns, sample_rate))
}

/// A parsed CUE-style split point.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CuePoint {
    pub start_ns: u128,
    pub title: String,
}

/// Parse a CUE-style track list (spec §11.3):
///
/// ```text
/// 00:00:00.000 Track 01
/// 00:04:12.500 Track 02
/// ```
///
/// Blank lines and lines beginning with `#` are ignored. The timestamp is the
/// first whitespace-delimited token; the remainder of the line is the title.
pub fn parse_cue(text: &str) -> Result<Vec<CuePoint>, String> {
    let mut points = Vec::new();
    for (lineno, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (ts, title) = match line.split_once(char::is_whitespace) {
            Some((ts, rest)) => (ts, rest.trim().to_string()),
            None => (line, String::new()),
        };
        let start_ns = parse_timestamp_ns(ts).map_err(|e| format!("line {}: {e}", lineno + 1))?;
        points.push(CuePoint { start_ns, title });
    }
    if points.is_empty() {
        return Err("no track points found in cue file".to_string());
    }
    // Enforce ascending start times.
    for w in points.windows(2) {
        if w[1].start_ns <= w[0].start_ns {
            return Err("cue track points must be strictly increasing".to_string());
        }
    }
    Ok(points)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations() {
        assert_eq!(parse_duration_frames("5s", 48_000).unwrap(), 240_000);
        assert_eq!(parse_duration_frames("250ms", 48_000).unwrap(), 12_000);
        assert_eq!(parse_duration_frames("0.5s", 48_000).unwrap(), 24_000);
        assert_eq!(parse_duration_frames("1", 48_000).unwrap(), 48_000);
        assert_eq!(
            parse_duration_frames("00:00:02.000", 48_000).unwrap(),
            96_000
        );
    }

    #[test]
    fn cue_parsing() {
        let text =
            "# comment\n00:00:00.000 Track 01\n00:04:12.500 Track 02\n\n00:08:35.000 Track 03\n";
        let points = parse_cue(text).unwrap();
        assert_eq!(points.len(), 3);
        assert_eq!(points[0].title, "Track 01");
        assert_eq!(points[1].start_ns, 252_500_000_000);
    }

    #[test]
    fn cue_rejects_unordered() {
        let text = "00:00:10.000 A\n00:00:05.000 B\n";
        assert!(parse_cue(text).is_err());
    }
}
