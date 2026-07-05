//! Device-free integration test of the render pipeline: source -> feeder
//! (integer op-chain, f32 conversion) -> resampler. This is exactly what the
//! Player's audio callback runs, minus the cpal device.

use intwav_codec::PcmBuffer;
use intwav_core::gain_q31_for_db;
use intwav_playback::{BufferSource, Feeder, LinearResampler, PreviewChain};

fn stereo_ramp(frames: usize) -> PcmBuffer {
    let mut samples = Vec::new();
    for i in 0..frames {
        samples.push((i as i32 * 100) % (1 << 23)); // L
        samples.push(-((i as i32 * 50) % (1 << 23))); // R
    }
    PcmBuffer {
        bit_depth: 24,
        sample_rate: 48_000,
        channels: 2,
        samples,
    }
}

#[test]
fn feeder_output_is_bounded_and_gained() {
    let src = BufferSource::new(stereo_ramp(1000));
    let chain = PreviewChain {
        gain_q31: gain_q31_for_db(-6),
        ..Default::default()
    };
    let mut feeder = Feeder::new(src, chain);
    let out = feeder.render_region().unwrap();
    assert_eq!(out.len(), 1000 * 2);
    // f32 samples are within [-1, 1].
    assert!(out.iter().all(|&v| (-1.0..=1.0).contains(&v)));
}

#[test]
fn feeder_then_resampler_changes_rate() {
    // Source at 48k, device at 44.1k -> resampler downsamples.
    let src = BufferSource::new(stereo_ramp(4800)); // 0.1 s at 48k
    let mut feeder = Feeder::new(src, PreviewChain::default());
    let mut resampler = LinearResampler::new(2, 48_000, 44_100);

    let mut out = Vec::new();
    let mut buf = vec![0.0f32; 2 * 512];
    loop {
        let n = feeder.fill(&mut buf).unwrap();
        out.extend(resampler.process(&buf[..n * 2]));
        if n < 512 {
            break;
        }
    }
    // ~44100/48000 of the input frames; allow a little boundary slack.
    let out_frames = out.len() / 2;
    let expected = 4800 * 44_100 / 48_000;
    assert!(
        (out_frames as i64 - expected as i64).abs() < 50,
        "out_frames={out_frames} expected≈{expected}"
    );
}
