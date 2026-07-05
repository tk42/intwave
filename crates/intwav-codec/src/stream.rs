//! Memory-bounded streaming decode.
//!
//! Both backends decode lazily — `hound` and `claxon` buffer at most one frame
//! internally — so iterating their sample iterators in chunks never holds the
//! whole file in memory. This is the decode primitive the engine uses to build
//! a scratch file + waveform in a single pass over a large source (spec §10.4).

use std::path::Path;

use hound::{SampleFormat, WavReader};

use crate::{detect_format, validate_shape, AudioSpec, CodecError, SourceFormat};

/// Decode `path` block by block, invoking `on_block` with interleaved `i32`
/// samples. Each block is a whole number of frames (except possibly the last),
/// at most `block_frames` frames. Returns the stream spec, with `frames` set to
/// the exact decoded count.
pub fn stream_decode<F>(
    path: &Path,
    block_frames: usize,
    mut on_block: F,
) -> Result<(AudioSpec, SourceFormat), CodecError>
where
    F: FnMut(&[i32]) -> Result<(), CodecError>,
{
    let block_frames = block_frames.max(1);
    let format = detect_format(path)?;
    let spec = match format {
        SourceFormat::Wav => stream_wav(path, block_frames, &mut on_block)?,
        SourceFormat::Flac => stream_flac(path, block_frames, &mut on_block)?,
    };
    Ok((spec, format))
}

fn stream_wav<F>(
    path: &Path,
    block_frames: usize,
    on_block: &mut F,
) -> Result<AudioSpec, CodecError>
where
    F: FnMut(&[i32]) -> Result<(), CodecError>,
{
    let reader = WavReader::open(path)?;
    let s = reader.spec();
    if s.sample_format == SampleFormat::Float {
        return Err(CodecError::FloatWav);
    }
    validate_shape(s.bits_per_sample, s.channels)?;
    let ch = s.channels as usize;
    let capacity = block_frames * ch;

    let mut buf: Vec<i32> = Vec::with_capacity(capacity);
    let mut total_samples: u64 = 0;
    for sample in reader.into_samples::<i32>() {
        buf.push(sample?);
        total_samples += 1;
        if buf.len() >= capacity {
            on_block(&buf)?;
            buf.clear();
        }
    }
    if !buf.is_empty() {
        on_block(&buf)?;
    }
    Ok(AudioSpec {
        bit_depth: s.bits_per_sample,
        sample_rate: s.sample_rate,
        channels: s.channels,
        frames: Some(total_samples / ch as u64),
    })
}

fn stream_flac<F>(
    path: &Path,
    block_frames: usize,
    on_block: &mut F,
) -> Result<AudioSpec, CodecError>
where
    F: FnMut(&[i32]) -> Result<(), CodecError>,
{
    let mut reader = claxon::FlacReader::open(path)?;
    let info = reader.streaminfo();
    let bit_depth = info.bits_per_sample as u16;
    let channels = info.channels as u16;
    validate_shape(bit_depth, channels)?;
    let ch = channels as usize;
    let capacity = block_frames * ch;

    let mut buf: Vec<i32> = Vec::with_capacity(capacity);
    let mut total_samples: u64 = 0;
    for sample in reader.samples() {
        buf.push(sample?);
        total_samples += 1;
        if buf.len() >= capacity {
            on_block(&buf)?;
            buf.clear();
        }
    }
    if !buf.is_empty() {
        on_block(&buf)?;
    }
    Ok(AudioSpec {
        bit_depth,
        sample_rate: info.sample_rate,
        channels,
        frames: Some(total_samples / ch as u64),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{read, write_wav, PcmBuffer};

    #[test]
    fn streaming_reconstructs_whole_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("s.wav");
        let pcm = PcmBuffer {
            bit_depth: 24,
            sample_rate: 48_000,
            channels: 2,
            samples: (0..4000).map(|i| (i * 11) % (1 << 23)).collect(),
        };
        write_wav(&pcm, &path).unwrap();

        // Small block size to exercise chunk boundaries.
        let mut collected = Vec::new();
        let (spec, _) = stream_decode(&path, 100, |block| {
            collected.extend_from_slice(block);
            Ok(())
        })
        .unwrap();

        assert_eq!(spec.frames, Some(2000));
        let (whole, _) = read(&path).unwrap();
        assert_eq!(collected, whole.samples);
    }
}
