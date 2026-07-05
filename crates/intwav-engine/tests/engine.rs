//! Engine-level tests for the frozen contracts: verified atomic write,
//! OUTPUT_EXISTS, clip refusal, and report invariants.

use std::path::Path;

use intwav_codec::{read, write_wav, OutputFormat, PcmBuffer};
use intwav_engine::{
    export16, gain, trim, CancelToken, EngineConfig, ErrorCode, Export16Params, GainParams,
    NoProgress, TrimParams,
};

fn ramp(frames: usize, channels: u16, bit_depth: u16, rate: u32) -> PcmBuffer {
    let mut samples = Vec::new();
    for i in 0..frames {
        for ch in 0..channels {
            samples.push(((i as i32) * 7 + ch as i32) % ((1 << (bit_depth - 1)) - 1));
        }
    }
    PcmBuffer {
        bit_depth,
        sample_rate: rate,
        channels,
        samples,
    }
}

fn cfg() -> EngineConfig {
    EngineConfig::default()
}

#[test]
fn trim_writes_verified_and_preserves_samples() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("in.wav");
    let output = dir.path().join("out.wav");
    let pcm = ramp(2000, 2, 24, 48_000);
    write_wav(&pcm, &input).unwrap();

    let p = TrimParams {
        from_frame: 480,
        to_frame: 960,
        format: OutputFormat::Wav,
        overwrite: false,
    };
    let report = trim(
        &input,
        &output,
        &p,
        &cfg(),
        &NoProgress,
        &CancelToken::new(),
    )
    .unwrap();

    assert!(report.pcm_verified, "write must self-verify");
    assert!(!report.sample_values_modified);
    assert!(!report.floating_point_used_in_save_path);
    assert_eq!(report.from_sample, Some(480));

    let (back, _) = read(&output).unwrap();
    assert_eq!(back.samples, &pcm.samples[480 * 2..960 * 2]);
}

#[test]
fn output_exists_is_refused_then_allowed_with_overwrite() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("in.wav");
    let output = dir.path().join("out.wav");
    write_wav(&ramp(500, 2, 24, 48_000), &input).unwrap();
    // Pre-create the destination.
    std::fs::write(&output, b"existing").unwrap();

    let mut p = TrimParams {
        from_frame: 0,
        to_frame: 100,
        format: OutputFormat::Wav,
        overwrite: false,
    };
    let err = trim(
        &input,
        &output,
        &p,
        &cfg(),
        &NoProgress,
        &CancelToken::new(),
    )
    .unwrap_err();
    assert_eq!(err.code, ErrorCode::OutputExists);
    // The existing file is untouched.
    assert_eq!(std::fs::read(&output).unwrap(), b"existing");

    p.overwrite = true;
    let report = trim(
        &input,
        &output,
        &p,
        &cfg(),
        &NoProgress,
        &CancelToken::new(),
    )
    .unwrap();
    assert!(report.pcm_verified);
    assert!(read(&output).is_ok()); // now a real WAV
}

#[test]
fn positive_gain_clip_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("in.wav");
    let output = dir.path().join("out.wav");
    // Near full scale so +12 dB clips.
    let pcm = PcmBuffer {
        bit_depth: 24,
        sample_rate: 48_000,
        channels: 1,
        samples: vec![(1 << 23) - 1; 100],
    };
    write_wav(&pcm, &input).unwrap();

    let p = GainParams {
        db: 12,
        allow_clipping: false,
        format: OutputFormat::Wav,
        overwrite: false,
    };
    let err = gain(
        &input,
        &output,
        &p,
        &cfg(),
        &NoProgress,
        &CancelToken::new(),
    )
    .unwrap_err();
    assert_eq!(err.code, ErrorCode::ClipRefused);
    assert!(!Path::new(&output).exists());
}

#[test]
fn export16_marks_requantized_and_dithered() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("in.wav");
    let output = dir.path().join("out.wav");
    write_wav(&ramp(1000, 2, 24, 48_000), &input).unwrap();

    let p = Export16Params {
        seed: 7,
        format: OutputFormat::Wav,
        overwrite: false,
    };
    let report = export16(
        &input,
        &output,
        &p,
        &cfg(),
        &NoProgress,
        &CancelToken::new(),
    )
    .unwrap();
    assert!(report.requantized);
    assert!(report.dither_used);
    assert!(report.sample_values_modified);
    assert!(report.pcm_verified);
    assert!(!report.floating_point_used_in_save_path);
    let (back, _) = read(&output).unwrap();
    assert_eq!(back.bit_depth, 16);
}

#[test]
fn cancelled_token_aborts() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("in.wav");
    let output = dir.path().join("out.wav");
    write_wav(&ramp(1000, 2, 24, 48_000), &input).unwrap();

    let cancel = CancelToken::new();
    cancel.cancel();
    let p = TrimParams {
        from_frame: 0,
        to_frame: 500,
        format: OutputFormat::Wav,
        overwrite: false,
    };
    let err = trim(&input, &output, &p, &cfg(), &NoProgress, &cancel).unwrap_err();
    assert_eq!(err.code, ErrorCode::Cancelled);
    assert!(!Path::new(&output).exists());
}
