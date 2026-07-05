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
    assert!(stdout.contains("Floating point used in save path: no"));
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
    assert_eq!(json["floating_point_used_in_save_path"], false);
    assert_eq!(json["resampled"], false);
    assert_eq!(json["requantized"], false);
    assert_eq!(json["pcm_verified"], true);
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

/// Build a mono WAV of a constant sample value.
fn const_pcm(value: i32, frames: usize, bit_depth: u16, rate: u32) -> PcmBuffer {
    PcmBuffer {
        bit_depth,
        sample_rate: rate,
        channels: 1,
        samples: vec![value; frames],
    }
}

#[test]
fn gain_minus_six_db_halves_samples() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("in.wav");
    let output = dir.path().join("out.wav");
    write_wav(&const_pcm(1000, 100, 24, 48_000), &input).unwrap();

    let out = run(&[
        "gain",
        input.to_str().unwrap(),
        output.to_str().unwrap(),
        "--db",
        "-6",
        "--output-format",
        "wav",
    ]);
    assert!(out.status.success(), "{:?}", out);
    let (back, _) = read(&output).unwrap();
    // -6 dB coefficient is 0.50118 -> round(1000 * that) = 501.
    assert!(back.samples.iter().all(|&s| s == 501));
}

#[test]
fn positive_gain_refuses_clipping_without_flag() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("in.wav");
    let output = dir.path().join("out.wav");
    // Near full-scale so +12 dB will clip.
    write_wav(&const_pcm((1 << 23) - 1, 50, 24, 48_000), &input).unwrap();

    let refused = run(&[
        "gain",
        input.to_str().unwrap(),
        output.to_str().unwrap(),
        "--db",
        "12",
        "--output-format",
        "wav",
    ]);
    assert!(!refused.status.success());
    assert!(!Path::new(&output).exists());

    let allowed = run(&[
        "gain",
        input.to_str().unwrap(),
        output.to_str().unwrap(),
        "--db",
        "12",
        "--allow-clipping",
        "--output-format",
        "wav",
    ]);
    assert!(allowed.status.success());
    let (back, _) = read(&output).unwrap();
    assert!(back.samples.iter().all(|&s| s == (1 << 23) - 1)); // saturated
}

#[test]
fn fade_in_starts_silent() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("in.wav");
    let output = dir.path().join("out.wav");
    write_wav(&const_pcm(1000, 48_000, 24, 48_000), &input).unwrap();

    let out = run(&[
        "fade-in",
        input.to_str().unwrap(),
        output.to_str().unwrap(),
        "--duration",
        "0.5s",
        "--output-format",
        "wav",
    ]);
    assert!(out.status.success(), "{:?}", out);
    let (back, _) = read(&output).unwrap();
    assert_eq!(back.samples[0], 0); // silent at the very start
    assert_eq!(*back.samples.last().unwrap(), 1000); // unchanged past the fade
}

#[test]
fn export16_is_deterministic_and_16bit() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("in.wav");
    let out1 = dir.path().join("a.wav");
    let out2 = dir.path().join("b.wav");
    // 24-bit ramp.
    let pcm = PcmBuffer {
        bit_depth: 24,
        sample_rate: 48_000,
        channels: 2,
        samples: (0..2000i32).map(|i| (i * 517) % (1 << 23)).collect(),
    };
    write_wav(&pcm, &input).unwrap();

    for out in [&out1, &out2] {
        let r = run(&[
            "export16",
            input.to_str().unwrap(),
            out.to_str().unwrap(),
            "--seed",
            "99",
            "--output-format",
            "wav",
        ]);
        assert!(r.status.success(), "{:?}", r);
    }
    let (a, _) = read(&out1).unwrap();
    let (b, _) = read(&out2).unwrap();
    assert_eq!(a.bit_depth, 16);
    assert_eq!(a.samples, b.samples, "same seed -> identical dither");
}

#[test]
fn split_by_cue_tracks_concatenate_to_input() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("in.wav");
    let out_dir = dir.path().join("tracks");
    let cue = dir.path().join("tracks.cue");

    let rate = 48_000;
    // 3 seconds of ramp.
    let pcm = PcmBuffer {
        bit_depth: 24,
        sample_rate: rate,
        channels: 2,
        samples: (0..3 * rate as i32 * 2)
            .map(|i| (i * 3) % (1 << 23))
            .collect(),
    };
    write_wav(&pcm, &input).unwrap();
    std::fs::write(
        &cue,
        "00:00:00.000 One\n00:00:01.000 Two\n00:00:02.000 Three\n",
    )
    .unwrap();

    let out = run(&[
        "split",
        input.to_str().unwrap(),
        "--out",
        out_dir.to_str().unwrap(),
        "--cue",
        cue.to_str().unwrap(),
        "--output-format",
        "wav",
    ]);
    assert!(out.status.success(), "{:?}", out);

    // Concatenate the three tracks in order; must reproduce the input samples.
    let mut joined = Vec::new();
    for name in ["01 One.wav", "02 Two.wav", "03 Three.wav"] {
        let (t, _) = read(&out_dir.join(name)).unwrap();
        joined.extend_from_slice(&t.samples);
    }
    assert_eq!(joined, pcm.samples, "tracks must rejoin bit-exactly");
}

#[test]
fn verify_detects_identical_and_different() {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.wav");
    let b = dir.path().join("b.wav");
    let c = dir.path().join("c.wav");
    write_wav(&const_pcm(123, 100, 24, 48_000), &a).unwrap();
    write_wav(&const_pcm(123, 100, 24, 48_000), &b).unwrap();
    write_wav(&const_pcm(124, 100, 24, 48_000), &c).unwrap();

    let same = run(&["verify", a.to_str().unwrap(), b.to_str().unwrap()]);
    assert!(same.status.success());
    let diff = run(&["verify", a.to_str().unwrap(), c.to_str().unwrap()]);
    assert!(!diff.status.success());
}

#[test]
fn cli_render_matches_engine_render() {
    // The central Q21 guard: the CLI and a direct engine call must produce
    // byte-identical PCM. Both go through intwav-engine, so this catches any
    // divergence in how the CLI maps arguments to engine parameters.
    use intwav_engine::{trim as engine_trim, CancelToken, EngineConfig, NoProgress, TrimParams};

    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("in.wav");
    let cli_out = dir.path().join("cli.wav");
    let eng_out = dir.path().join("eng.wav");
    write_wav(&ramp_pcm(2000, 48_000), &input).unwrap();

    // Engine render directly: 0.010s..0.020s at 48k -> frames [480, 960).
    let p = TrimParams {
        from_frame: 480,
        to_frame: 960,
        format: intwav_codec::OutputFormat::Wav,
        overwrite: false,
    };
    engine_trim(
        &input,
        &eng_out,
        &p,
        &EngineConfig::default(),
        &NoProgress,
        &CancelToken::new(),
    )
    .unwrap();

    // CLI render with the equivalent timestamps.
    let out = run(&[
        "trim",
        input.to_str().unwrap(),
        cli_out.to_str().unwrap(),
        "--from",
        "0.010",
        "--to",
        "0.020",
        "--output-format",
        "wav",
    ]);
    assert!(out.status.success(), "{:?}", out);

    let (cli_pcm, _) = read(&cli_out).unwrap();
    let (eng_pcm, _) = read(&eng_out).unwrap();
    assert_eq!(
        intwav_engine::pcm_sha256(&cli_pcm),
        intwav_engine::pcm_sha256(&eng_pcm),
        "CLI and engine renders must be byte-identical"
    );
}

#[test]
fn dc_correct_removes_offset() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("in.wav");
    let output = dir.path().join("out.wav");
    write_wav(&const_pcm(500, 1000, 24, 48_000), &input).unwrap();

    let out = run(&[
        "dc-correct",
        input.to_str().unwrap(),
        output.to_str().unwrap(),
        "--output-format",
        "wav",
    ]);
    assert!(out.status.success(), "{:?}", out);
    let (back, _) = read(&output).unwrap();
    // A constant 500 has mean 500; correction zeroes it.
    assert!(back.samples.iter().all(|&s| s == 0));
}
