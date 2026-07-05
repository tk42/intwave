//! `open_source` — decode a source **once** into a scratch file while building
//! the waveform pyramid and the PCM hash in the same streaming pass (spec §10.4
//! / Q3 memory spine). Afterwards the scratch supports O(1) random access via
//! [`ScratchReader`](crate::ScratchReader), so nothing decodes the source again.

use std::path::{Path, PathBuf};

use intwav_codec::{probe, stream_decode, AudioSpec, CodecError, SourceFormat};

use crate::error::EngineResult;
use crate::hash::PcmHasher;
use crate::progress::{CancelToken, ProgressSink};
use crate::scratch::ScratchWriter;
use crate::waveform::{WaveformBuilder, WaveformPyramid};

/// Waveform + streaming configuration for [`open_source`].
#[derive(Debug, Clone, Copy)]
pub struct OpenParams {
    pub base_bucket_frames: u64,
    pub factor: u32,
    pub max_levels: usize,
    pub block_frames: usize,
}

impl Default for OpenParams {
    fn default() -> Self {
        Self {
            base_bucket_frames: 256,
            factor: 8,
            max_levels: 8,
            block_frames: 1 << 16,
        }
    }
}

/// The result of opening a source: the scratch path plus everything derived in
/// the single decode pass.
#[derive(Debug, Clone)]
pub struct OpenSource {
    pub scratch_path: PathBuf,
    pub spec: AudioSpec,
    pub format: SourceFormat,
    pub pcm_sha256: String,
    pub waveform: WaveformPyramid,
}

/// Decode `input` into `scratch_path`, building the waveform and PCM hash in one
/// pass. The caller owns the scratch file's lifetime (delete it when the source
/// is closed).
pub fn open_source(
    input: &Path,
    scratch_path: &Path,
    params: &OpenParams,
    progress: &dyn ProgressSink,
    cancel: &CancelToken,
) -> EngineResult<OpenSource> {
    progress.set_permille(0);
    // Probe the header for stream parameters up front (cheap, no sample decode).
    let (spec, format) = probe(input)?;
    cancel.check()?;

    let mut writer = ScratchWriter::create(
        scratch_path,
        spec.bit_depth,
        spec.sample_rate,
        spec.channels,
    )?;
    let mut hasher = PcmHasher::new(spec.bit_depth, spec.channels, spec.sample_rate);
    let mut waveform = WaveformBuilder::new(
        spec.channels as usize,
        spec.bit_depth,
        params.base_bucket_frames,
        params.factor,
        params.max_levels,
    );

    // Estimate total frames for progress (0 if unknown).
    let total_frames = spec.frames.unwrap_or(0);
    let mut done_frames: u64 = 0;
    let ch = spec.channels.max(1) as u64;
    let mut cancel_err: Option<crate::error::EngineError> = None;

    let (_decoded, _fmt) = stream_decode(input, params.block_frames, |block| {
        if cancel.is_cancelled() {
            cancel_err = Some(cancel.check().unwrap_err());
            // Surface as an I/O-shaped error to stop the decode iterator.
            return Err(CodecError::FlacEncode("cancelled".to_string()));
        }
        writer.write_block(block).map_err(CodecError::Io)?;
        hasher.update(block);
        waveform.push_block(block);
        if total_frames > 0 {
            done_frames += (block.len() as u64) / ch;
            progress.set_permille(((done_frames.min(total_frames)) * 1000 / total_frames) as u32);
        }
        Ok(())
    })
    .map_err(|e| cancel_err.clone().unwrap_or_else(|| e.into()))?;

    let frames = writer.finish()?;
    progress.set_permille(1000);

    Ok(OpenSource {
        scratch_path: scratch_path.to_path_buf(),
        spec: AudioSpec {
            bit_depth: spec.bit_depth,
            sample_rate: spec.sample_rate,
            channels: spec.channels,
            frames: Some(frames),
        },
        format,
        pcm_sha256: hasher.finish(),
        waveform: waveform.finish(),
    })
}
