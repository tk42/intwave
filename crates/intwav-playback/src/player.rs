//! cpal-backed device player (behind the `device` feature).
//!
//! Opens the output device at the source's **native sample rate** when possible
//! (no resampling); otherwise falls back to a supported rate with a float
//! [`LinearResampler`] — preview-only, never on the save path. The feeder runs
//! inside the audio callback guarded by a mutex; a production build would move
//! it to a dedicated thread with a lock-free ring, but this is sufficient for a
//! preview player.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, SampleRate};

use crate::error::PlaybackError;
use crate::feeder::{Feeder, PreviewChain};
use crate::resample::LinearResampler;
use crate::source::FrameSource;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PlayerState {
    Stopped,
    Playing,
    Paused,
}

const STOPPED: u8 = 0;
const PLAYING: u8 = 1;
const PAUSED: u8 = 2;

struct Inner<S: FrameSource> {
    feeder: Feeder<S>,
    resampler: Option<LinearResampler>,
    src_channels: usize,
    out_queue: VecDeque<f32>,
}

impl<S: FrameSource> Inner<S> {
    fn render_into(&mut self, data: &mut [f32]) {
        if self.resampler.is_none() {
            // Native rate: fill directly (feeder zero-pads at end).
            let _ = self.feeder.fill(data);
            return;
        }
        let need = data.len();
        while self.out_queue.len() < need && !self.feeder.is_finished() {
            let want_in_frames = 4096usize;
            let mut inbuf = vec![0.0f32; want_in_frames * self.src_channels];
            let n = self.feeder.fill(&mut inbuf).unwrap_or(0);
            if let Some(rs) = self.resampler.as_mut() {
                let produced = rs.process(&inbuf[..n * self.src_channels]);
                self.out_queue.extend(produced);
            }
            if n < want_in_frames {
                break;
            }
        }
        for slot in data.iter_mut() {
            *slot = self.out_queue.pop_front().unwrap_or(0.0);
        }
    }
}

/// A device player owning the cpal stream. Not `Send` (cpal streams aren't on
/// all platforms); own it on the thread that created it.
pub struct Player<S: FrameSource> {
    _stream: cpal::Stream,
    inner: Arc<Mutex<Inner<S>>>,
    state: Arc<AtomicU8>,
}

impl<S: FrameSource + Send + 'static> Player<S> {
    /// Build a player for `source`, opening the device at native rate if
    /// supported, else a resampled fallback.
    pub fn new(source: S, chain: PreviewChain) -> Result<Self, PlaybackError> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| PlaybackError::Device("no default output device".into()))?;

        let src_rate = source.sample_rate();
        let src_ch = source.channels();

        let (config, out_rate) = choose_config(&device, src_ch, src_rate)?;
        let resampler = if out_rate == src_rate {
            None
        } else {
            Some(LinearResampler::new(src_ch as usize, src_rate, out_rate))
        };

        let inner = Arc::new(Mutex::new(Inner {
            feeder: Feeder::new(source, chain),
            resampler,
            src_channels: src_ch as usize,
            out_queue: VecDeque::new(),
        }));
        let state = Arc::new(AtomicU8::new(STOPPED));

        let cb_inner = Arc::clone(&inner);
        let cb_state = Arc::clone(&state);
        let stream = device
            .build_output_stream(
                &config,
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    if cb_state.load(Ordering::Relaxed) != PLAYING {
                        data.fill(0.0);
                        return;
                    }
                    if let Ok(mut inner) = cb_inner.lock() {
                        inner.render_into(data);
                    } else {
                        data.fill(0.0);
                    }
                },
                |err| eprintln!("playback stream error: {err}"),
                None,
            )
            .map_err(|e| PlaybackError::Device(e.to_string()))?;

        stream
            .play()
            .map_err(|e| PlaybackError::Device(e.to_string()))?;

        Ok(Self {
            _stream: stream,
            inner,
            state,
        })
    }

    pub fn play(&self) {
        self.state.store(PLAYING, Ordering::Relaxed);
    }
    pub fn pause(&self) {
        self.state.store(PAUSED, Ordering::Relaxed);
    }
    pub fn stop(&self) {
        self.state.store(STOPPED, Ordering::Relaxed);
        if let Ok(mut inner) = self.inner.lock() {
            inner.feeder.seek(0);
            inner.out_queue.clear();
        }
    }

    pub fn state(&self) -> PlayerState {
        match self.state.load(Ordering::Relaxed) {
            PLAYING => PlayerState::Playing,
            PAUSED => PlayerState::Paused,
            _ => PlayerState::Stopped,
        }
    }

    pub fn seek(&self, frame: u64) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.feeder.seek(frame);
            inner.out_queue.clear();
        }
    }

    pub fn set_region(&self, start: u64, end: u64) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.feeder.set_region(start, end);
            inner.out_queue.clear();
        }
    }

    pub fn set_looping(&self, looping: bool) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.feeder.set_looping(looping);
        }
    }

    /// Current playhead frame (source-frame coordinate).
    pub fn position(&self) -> u64 {
        self.inner.lock().map(|i| i.feeder.position()).unwrap_or(0)
    }
}

/// Pick an `f32` output config with the source channel count, preferring the
/// native sample rate. Returns the config and the chosen output rate.
fn choose_config(
    device: &cpal::Device,
    src_ch: u16,
    src_rate: u32,
) -> Result<(cpal::StreamConfig, u32), PlaybackError> {
    let configs = device
        .supported_output_configs()
        .map_err(|e| PlaybackError::Device(e.to_string()))?
        .filter(|c| c.channels() == src_ch && c.sample_format() == SampleFormat::F32)
        .collect::<Vec<_>>();

    if configs.is_empty() {
        return Err(PlaybackError::Device(format!(
            "no f32 output config with {src_ch} channels"
        )));
    }

    // Prefer a config whose supported range contains the native rate.
    for c in &configs {
        if c.min_sample_rate().0 <= src_rate && src_rate <= c.max_sample_rate().0 {
            let config = (*c).with_sample_rate(SampleRate(src_rate)).config();
            return Ok((config, src_rate));
        }
    }

    // Fallback: clamp the native rate into the first supported range.
    let c = configs[0];
    let out_rate = src_rate.clamp(c.min_sample_rate().0, c.max_sample_rate().0);
    let config = c.with_sample_rate(SampleRate(out_rate)).config();
    Ok((config, out_rate))
}
