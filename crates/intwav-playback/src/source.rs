//! A random-access integer PCM source for playback. Implemented for the engine's
//! seekable scratch file and for an in-memory buffer (used by tests).

use intwav_codec::PcmBuffer;
use intwav_engine::ScratchReader;

use crate::error::PlaybackError;

/// Random-access source of interleaved `i32` frames.
pub trait FrameSource {
    fn frames(&self) -> u64;
    fn channels(&self) -> u16;
    fn bit_depth(&self) -> u16;
    fn sample_rate(&self) -> u32;
    /// Read frames `[from, to)` as interleaved `i32`.
    fn read_range(&mut self, from: u64, to: u64) -> Result<Vec<i32>, PlaybackError>;
}

impl FrameSource for ScratchReader {
    fn frames(&self) -> u64 {
        self.frames
    }
    fn channels(&self) -> u16 {
        self.channels
    }
    fn bit_depth(&self) -> u16 {
        self.bit_depth
    }
    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }
    fn read_range(&mut self, from: u64, to: u64) -> Result<Vec<i32>, PlaybackError> {
        // Fully-qualified to call the inherent method, not this trait method.
        ScratchReader::read_range(self, from, to).map_err(PlaybackError::from)
    }
}

/// In-memory PCM source (small clips, tests).
pub struct BufferSource {
    pcm: PcmBuffer,
}

impl BufferSource {
    pub fn new(pcm: PcmBuffer) -> Self {
        Self { pcm }
    }
}

impl FrameSource for BufferSource {
    fn frames(&self) -> u64 {
        self.pcm.frames()
    }
    fn channels(&self) -> u16 {
        self.pcm.channels
    }
    fn bit_depth(&self) -> u16 {
        self.pcm.bit_depth
    }
    fn sample_rate(&self) -> u32 {
        self.pcm.sample_rate
    }
    fn read_range(&mut self, from: u64, to: u64) -> Result<Vec<i32>, PlaybackError> {
        let ch = self.pcm.channels as u64;
        let frames = self.pcm.frames();
        if from > to || to > frames {
            return Err(PlaybackError::Range(format!(
                "range [{from}, {to}) invalid for {frames} frames"
            )));
        }
        let start = (from * ch) as usize;
        let end = (to * ch) as usize;
        Ok(self.pcm.samples[start..end].to_vec())
    }
}
