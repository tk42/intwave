//! WAV read/write over `hound`, with strict integer-PCM validation.

use std::path::Path;

use hound::{SampleFormat, WavReader, WavSpec, WavWriter};

use crate::{validate_shape, CodecError, PcmBuffer};

/// Read a WAV file into integer PCM, rejecting anything that would require a
/// float or lossy interpretation.
pub fn read_wav(path: &Path) -> Result<PcmBuffer, CodecError> {
    let reader = WavReader::open(path)?;
    let spec = reader.spec();

    // Reject float WAV outright — this tool never converts float to integer.
    if spec.sample_format == SampleFormat::Float {
        return Err(CodecError::FloatWav);
    }
    validate_shape(spec.bits_per_sample, spec.channels)?;

    // hound yields each stored integer sample sign-extended into i32, for
    // 16/24/32-bit input alike — exactly our internal representation.
    let samples = reader
        .into_samples::<i32>()
        .collect::<Result<Vec<i32>, _>>()?;

    Ok(PcmBuffer {
        bit_depth: spec.bits_per_sample,
        sample_rate: spec.sample_rate,
        channels: spec.channels,
        samples,
    })
}

/// Write integer PCM to a WAV file. Sample values are written verbatim.
pub fn write_wav(pcm: &PcmBuffer, path: &Path) -> Result<(), CodecError> {
    validate_shape(pcm.bit_depth, pcm.channels)?;
    let spec = WavSpec {
        channels: pcm.channels,
        sample_rate: pcm.sample_rate,
        bits_per_sample: pcm.bit_depth,
        sample_format: SampleFormat::Int,
    };
    let mut writer = WavWriter::create(path, spec)?;
    for &s in &pcm.samples {
        writer.write_sample(s)?;
    }
    writer.finalize()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wav_roundtrip_24bit_stereo() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("rt.wav");
        let pcm = PcmBuffer {
            bit_depth: 24,
            sample_rate: 96_000,
            channels: 2,
            samples: vec![0, 1, -1, (1 << 23) - 1, -(1 << 23), 12345],
        };
        write_wav(&pcm, &path).unwrap();
        let back = read_wav(&path).unwrap();
        assert_eq!(back, pcm);
    }
}
