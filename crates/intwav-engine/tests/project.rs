//! v2 project layer: `.iwproj` round-trip, source verification, and rendering
//! through the canonical op-chain with the Master/Derivative gate.

use std::path::Path;

use intwav_codec::{read, write_wav, OutputFormat, PcmBuffer};
use intwav_engine::{
    open_project, render_document, render_region, save_project, source_ref_from_file,
    validate_export, verify_source, CancelToken, Document, EngineConfig, ExportKind, NoProgress,
    OpChain, Region,
};

fn ramp(frames: usize, ch: u16) -> PcmBuffer {
    let mut samples = Vec::new();
    for i in 0..frames {
        for c in 0..ch {
            samples.push(((i as i32) * 13 + c as i32 * 3) % ((1 << 23) - 1));
        }
    }
    PcmBuffer {
        bit_depth: 24,
        sample_rate: 48_000,
        channels: ch,
        samples,
    }
}

fn region(id: &str, source_id: &str, start: u64, end: u64, ops: OpChain) -> Region {
    Region {
        id: id.into(),
        source_id: source_id.into(),
        start_frame: start,
        end_frame: end,
        title: format!("Track {id}"),
        track_number: Some(1),
        artist: None,
        album: None,
        ops,
    }
}

#[test]
fn iwproj_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let wav = dir.path().join("src.wav");
    write_wav(&ramp(1000, 2), &wav).unwrap();

    let mut doc = Document::new();
    doc.sources
        .push(source_ref_from_file("src", &wav, dir.path()).unwrap());
    doc.regions
        .push(region("r1", "src", 0, 500, OpChain::default()));

    let proj = dir.path().join("p.iwproj");
    save_project(&doc, &proj).unwrap();
    let reopened = open_project(&proj).unwrap();
    assert_eq!(reopened, doc);
    // Source stored relative to the project dir.
    assert_eq!(reopened.sources[0].relative_path, "src.wav");
}

#[test]
fn source_hash_verify_detects_tampering() {
    let dir = tempfile::tempdir().unwrap();
    let wav = dir.path().join("src.wav");
    write_wav(&ramp(500, 1), &wav).unwrap();
    let src = source_ref_from_file("src", &wav, dir.path()).unwrap();

    // Matching file verifies.
    verify_source(&src, &wav).unwrap();

    // Overwrite with different PCM -> mismatch.
    write_wav(&ramp(500, 1).clone_with_offset(1), &wav).unwrap();
    assert!(verify_source(&src, &wav).is_err());
}

// Small helper to perturb PCM for the tamper test.
trait Perturb {
    fn clone_with_offset(&self, delta: i32) -> PcmBuffer;
}
impl Perturb for PcmBuffer {
    fn clone_with_offset(&self, delta: i32) -> PcmBuffer {
        let mut p = self.clone();
        for s in &mut p.samples {
            *s += delta;
        }
        p
    }
}

#[test]
fn render_region_lossless_preserves_samples() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("out.wav");
    let src = ramp(1000, 2);
    let r = region("r", "src", 200, 700, OpChain::default());

    let report = render_region(
        &src,
        &r,
        ExportKind::Master,
        &out,
        OutputFormat::Wav,
        &EngineConfig::default(),
        false,
    )
    .unwrap();
    assert!(!report.sample_values_modified);
    assert!(report.pcm_verified);
    assert!(!report.requantized);

    let (back, _) = read(&out).unwrap();
    assert_eq!(back.samples, &src.samples[200 * 2..700 * 2]);
}

#[test]
fn render_region_gain_modifies_samples() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("out.wav");
    let src = PcmBuffer {
        bit_depth: 24,
        sample_rate: 48_000,
        channels: 1,
        samples: vec![1000; 200],
    };
    let ops = OpChain {
        gain_db: Some(-6),
        ..Default::default()
    };
    let r = region("r", "src", 0, 200, ops);

    let report = render_region(
        &src,
        &r,
        ExportKind::Master,
        &out,
        OutputFormat::Wav,
        &EngineConfig::default(),
        false,
    )
    .unwrap();
    assert!(report.sample_values_modified);

    let (back, _) = read(&out).unwrap();
    assert!(back.samples.iter().all(|&s| s == 501)); // -6 dB of 1000
}

#[test]
fn master_export_refuses_requantization() {
    let ops = OpChain {
        export16_seed: Some(1),
        ..Default::default()
    };
    assert!(validate_export(&ops, ExportKind::Master).is_err());
    assert!(validate_export(&ops, ExportKind::Derivative).is_ok());
}

#[test]
fn render_document_tracks_rejoin_to_source() {
    let dir = tempfile::tempdir().unwrap();
    let wav = dir.path().join("src.wav");
    let src = ramp(3000, 2);
    write_wav(&src, &wav).unwrap();

    let mut doc = Document::new();
    doc.sources
        .push(source_ref_from_file("src", &wav, dir.path()).unwrap());
    // Two contiguous lossless regions covering [0, 3000).
    doc.regions
        .push(region("a", "src", 0, 1000, OpChain::default()));
    doc.regions
        .push(region("b", "src", 1000, 3000, OpChain::default()));

    let out_dir = dir.path().join("out");
    let reports = render_document(
        &doc,
        dir.path(),
        ExportKind::Master,
        &out_dir,
        OutputFormat::Wav,
        &EngineConfig::default(),
        false,
        &NoProgress,
        &CancelToken::new(),
    )
    .unwrap();
    assert_eq!(reports.len(), 2);
    assert!(reports.iter().all(|r| r.pcm_verified));

    // Concatenate the two rendered tracks -> the original source PCM.
    let mut joined = Vec::new();
    for name in ["01 Track a.wav", "02 Track b.wav"] {
        let (t, _) = read(&out_dir.join(name)).unwrap();
        joined.extend_from_slice(&t.samples);
    }
    assert_eq!(joined, src.samples);
}

#[test]
fn render_document_missing_source_errors() {
    // A region referencing a source with no resolvable path fails cleanly.
    let dir = tempfile::tempdir().unwrap();
    let mut doc = Document::new();
    doc.sources.push(intwav_engine::SourceRef {
        id: "gone".into(),
        relative_path: "missing.wav".into(),
        last_known_absolute_path: "/nowhere/missing.wav".into(),
        pcm_sha256: "x".into(),
        sample_rate: 48_000,
        bit_depth: 24,
        channels: 2,
        frames: 10,
    });
    doc.regions
        .push(region("r", "gone", 0, 10, OpChain::default()));
    let err = render_document(
        &doc,
        dir.path(),
        ExportKind::Master,
        &dir.path().join("out"),
        OutputFormat::Wav,
        &EngineConfig::default(),
        false,
        &NoProgress,
        &CancelToken::new(),
    )
    .unwrap_err();
    assert_eq!(err.code, intwav_engine::ErrorCode::SourceMissing);
    let _ = Path::new("");
}
