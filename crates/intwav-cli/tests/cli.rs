//! End-to-end tests driving the built `intwav` binary.

use std::path::Path;
use std::process::Command;

use intwav_codec::{read, write_wav, PcmBuffer};

const BIN: &str = env!("CARGO_BIN_EXE_intwav");

/// A deterministic 24-bit stereo ramp (no float needed to build it).
fn ramp_pcm(frames: usize, sample_rate: u32) -> PcmBuffer {
    let mut samples = Vec::with_capacity(frames * 2);
    for i in 0..frames {
        // Distinct, in-range values per channel.
        let l = (i as i32 * 7) % ((1 << 23) - 1);
        let r = -((i as i32 * 13) % (1 << 23));
        samples.push(l);
        samples.push(r);
    }
    PcmBuffer {
        bit_depth: 24,
        sample_rate,
        channels: 2,
        samples,
    }
}

fn run(args: &[&str]) -> std::process::Output {
    Command::new(BIN)
        .args(args)
        .output()
        .expect("failed to spawn intwav")
}

fn flac_available() -> bool {
    Command::new("flac")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[test]
fn info_runs_on_wav() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("in.wav");
    write_wav(&ramp_pcm(1000, 48_000), &input).unwrap();

    let out = run(&["info", input.to_str().unwrap()]);
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Decoded PCM: 24-bit integer"));
    assert!(stdout.contains("Floating point used: no"));
}

#[test]
fn trim_wav_preserves_exact_samples_and_report() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("in.wav");
    let output = dir.path().join("out.wav");
    let report = dir.path().join("r.json");

    let rate = 48_000;
    let pcm = ramp_pcm(2000, rate);
    write_wav(&pcm, &input).unwrap();

    // 0.010s..0.020s -> frames [480, 960).
    let out = run(&[
        "trim",
        input.to_str().unwrap(),
        output.to_str().unwrap(),
        "--from",
        "0.010",
        "--to",
        "0.020",
        "--output-format",
        "wav",
        "--report",
        report.to_str().unwrap(),
    ]);
    assert!(out.status.success(), "trim failed: {:?}", out);

    // Output samples must equal the input's frame range, unchanged.
    let (back, _fmt) = read(&output).unwrap();
    let expected = &pcm.samples[480 * 2..960 * 2];
    assert_eq!(back.samples, expected, "trimmed samples must be identical");
    assert_eq!(back.frames(), 480);

    // Report must assert the preservation invariants.
    let json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&report).unwrap()).unwrap();
    assert_eq!(json["from_sample"], 480);
    assert_eq!(json["to_sample"], 960);
    assert_eq!(json["sample_values_modified"], false);
    assert_eq!(json["floating_point_used"], false);
    assert_eq!(json["resampled"], false);
    assert_eq!(json["requantized"], false);
    assert_eq!(json["operation"], "trim");
}

#[test]
fn trim_to_flac_is_bit_exact() {
    if !flac_available() {
        eprintln!("skipping: `flac` not on PATH");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("in.wav");
    let output = dir.path().join("out.flac");

    let pcm = ramp_pcm(3000, 96_000);
    write_wav(&pcm, &input).unwrap();

    // 0.005s..0.015s at 96k -> frames [480, 1440).
    let out = run(&[
        "trim",
        input.to_str().unwrap(),
        output.to_str().unwrap(),
        "--from",
        "0.005",
        "--to",
        "0.015",
    ]);
    assert!(out.status.success(), "trim->flac failed: {:?}", out);

    let (back, fmt) = read(&output).unwrap();
    assert_eq!(fmt, intwav_codec::SourceFormat::Flac);
    assert_eq!(back.samples, &pcm.samples[480 * 2..1440 * 2]);
}

#[test]
fn unknown_extension_fails_cleanly() {
    let out = run(&["info", "nope.mp3"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("error:"));
}

#[test]
fn missing_input_fails_cleanly() {
    let out = run(&["info", "does-not-exist.wav"]);
    assert!(!out.status.success());
    // No panic; a plain error line.
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.starts_with("error:"));
}

#[test]
fn from_after_to_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("in.wav");
    let output = dir.path().join("out.wav");
    write_wav(&ramp_pcm(1000, 48_000), &input).unwrap();
    let out = run(&[
        "trim",
        input.to_str().unwrap(),
        output.to_str().unwrap(),
        "--from",
        "0.020",
        "--to",
        "0.010",
        "--output-format",
        "wav",
    ]);
    assert!(!out.status.success());
    // The range is rejected before any output is written.
    assert!(!Path::new(&output).exists());
}
