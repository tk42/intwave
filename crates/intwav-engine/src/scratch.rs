//! Seekable scratch PCM: the decode-once-to-scratch working file (spec §10.4).
//!
//! Layout: a 22-byte header (`IWSCR1`, bit depth, channels, sample rate, frame
//! count) followed by raw interleaved little-endian `i32` samples. Random access
//! is O(1): `byte = HEADER_LEN + frame * channels * 4`. The source file is never
//! touched; all later reads (trim ranges, playback seek) come from here.

use std::fs::File;
use std::io::{BufWriter, Read, Seek, SeekFrom, Write};
use std::path::Path;

use crate::error::{EngineError, EngineResult, ErrorCode};

const MAGIC: &[u8; 6] = b"IWSCR1";
const HEADER_LEN: u64 = 6 + 2 + 2 + 4 + 8; // magic + bit_depth + channels + sample_rate + frames
const FRAMES_OFFSET: u64 = 6 + 2 + 2 + 4; // where the frame count is patched

/// Streams decoded PCM to a scratch file. The frame count is patched into the
/// header on [`finish`](ScratchWriter::finish).
pub struct ScratchWriter {
    file: BufWriter<File>,
    channels: u16,
    frames: u64,
}

impl ScratchWriter {
    pub fn create(
        path: &Path,
        bit_depth: u16,
        sample_rate: u32,
        channels: u16,
    ) -> std::io::Result<Self> {
        let mut w = BufWriter::new(File::create(path)?);
        w.write_all(MAGIC)?;
        w.write_all(&bit_depth.to_le_bytes())?;
        w.write_all(&channels.to_le_bytes())?;
        w.write_all(&sample_rate.to_le_bytes())?;
        w.write_all(&0u64.to_le_bytes())?; // frame count placeholder
        Ok(Self {
            file: w,
            channels,
            frames: 0,
        })
    }

    /// Append a block of interleaved samples (a whole number of frames).
    pub fn write_block(&mut self, samples: &[i32]) -> std::io::Result<()> {
        let mut bytes = Vec::with_capacity(samples.len() * 4);
        for &s in samples {
            bytes.extend_from_slice(&s.to_le_bytes());
        }
        self.file.write_all(&bytes)?;
        self.frames += (samples.len() / self.channels.max(1) as usize) as u64;
        Ok(())
    }

    /// Flush, patch the header frame count, and return the total frame count.
    pub fn finish(self) -> std::io::Result<u64> {
        let frames = self.frames;
        let mut file = self
            .file
            .into_inner()
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        file.seek(SeekFrom::Start(FRAMES_OFFSET))?;
        file.write_all(&frames.to_le_bytes())?;
        file.flush()?;
        Ok(frames)
    }
}

/// Random-access reader over a scratch file.
pub struct ScratchReader {
    file: File,
    pub bit_depth: u16,
    pub sample_rate: u32,
    pub channels: u16,
    pub frames: u64,
}

impl ScratchReader {
    pub fn open(path: &Path) -> EngineResult<Self> {
        let mut file = File::open(path)?;
        let mut magic = [0u8; 6];
        file.read_exact(&mut magic)?;
        if &magic != MAGIC {
            return Err(EngineError::new(
                ErrorCode::IoError,
                "not an intwav scratch file",
            ));
        }
        let mut u16buf = [0u8; 2];
        let mut u32buf = [0u8; 4];
        let mut u64buf = [0u8; 8];
        file.read_exact(&mut u16buf)?;
        let bit_depth = u16::from_le_bytes(u16buf);
        file.read_exact(&mut u16buf)?;
        let channels = u16::from_le_bytes(u16buf);
        file.read_exact(&mut u32buf)?;
        let sample_rate = u32::from_le_bytes(u32buf);
        file.read_exact(&mut u64buf)?;
        let frames = u64::from_le_bytes(u64buf);
        Ok(Self {
            file,
            bit_depth,
            sample_rate,
            channels,
            frames,
        })
    }

    /// Read frames `[from_frame, to_frame)` as interleaved `i32` — O(1) seek,
    /// no decode.
    pub fn read_range(&mut self, from_frame: u64, to_frame: u64) -> EngineResult<Vec<i32>> {
        if from_frame > to_frame || to_frame > self.frames {
            return Err(EngineError::new(
                ErrorCode::RangeOutOfBounds,
                format!(
                    "range [{from_frame}, {to_frame}) invalid for {} frames",
                    self.frames
                ),
            ));
        }
        let ch = self.channels as u64;
        let start_byte = HEADER_LEN + from_frame * ch * 4;
        let count = ((to_frame - from_frame) * ch) as usize;
        self.file.seek(SeekFrom::Start(start_byte))?;
        let mut bytes = vec![0u8; count * 4];
        self.file.read_exact(&mut bytes)?;
        let mut out = Vec::with_capacity(count);
        for c in bytes.chunks_exact(4) {
            out.push(i32::from_le_bytes([c[0], c[1], c[2], c[3]]));
        }
        Ok(out)
    }
}
