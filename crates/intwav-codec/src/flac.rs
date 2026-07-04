//! FLAC decode via `claxon` (pure Rust, integer output) and encode via the
//! external `flac` command-line tool.
//!
//! Encoding is delegated to a separate process on purpose: libFLAC's encoder
//! uses floating point internally for LPC analysis, and keeping it out of the
//! intwav process is what lets `intwav-core` pass the disassembly float-scan.

use std::io::ErrorKind;
use std::path::Path;
use std::process::Command;

use crate::{validate_shape, write_wav, CodecError, PcmBuffer};

/// Decode a FLAC file into integer PCM.
pub fn read_flac(path: &Path) -> Result<PcmBuffer, CodecError> {
    let mut reader = claxon::FlacReader::open(path)?;
    let info = reader.streaminfo();
    let bit_depth = info.bits_per_sample as u16;
    let channels = info.channels as u16;
    validate_shape(bit_depth, channels)?;

    // claxon yields interleaved integer samples, sign-extended into i32.
    let samples = reader.samples().collect::<Result<Vec<i32>, _>>()?;

    Ok(PcmBuffer {
        bit_depth,
        sample_rate: info.sample_rate,
        channels,
        samples,
    })
}

/// Encode integer PCM to FLAC by writing a temporary WAV and invoking the
/// `flac` encoder. The output PCM is bit-exact with the input (FLAC is
/// lossless); only the container bytes differ (spec §9.3).
pub fn encode_flac(pcm: &PcmBuffer, out_path: &Path) -> Result<(), CodecError> {
    validate_shape(pcm.bit_depth, pcm.channels)?;

    // Write the PCM to a temp WAV that `flac` will consume.
    let tmp = tempfile::Builder::new()
        .prefix("intwav-")
        .suffix(".wav")
        .tempfile()?;
    write_wav(pcm, tmp.path())?;

    let status = Command::new("flac")
        .arg("--best")
        .arg("--silent")
        .arg("--force") // overwrite existing output
        .arg("-o")
        .arg(out_path)
        .arg(tmp.path())
        .status();

    match status {
        Ok(s) if s.success() => Ok(()),
        Ok(s) => Err(CodecError::FlacEncode(format!(
            "flac exited with {s} while encoding {}",
            out_path.display()
        ))),
        Err(e) if e.kind() == ErrorKind::NotFound => Err(CodecError::FlacEncoderMissing),
        Err(e) => Err(CodecError::Io(e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flac_available() -> bool {
        Command::new("flac")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    #[test]
    fn flac_roundtrip_is_bit_exact() {
        if !flac_available() {
            eprintln!("skipping: `flac` not on PATH");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let flac_path = dir.path().join("rt.flac");
        let pcm = PcmBuffer {
            bit_depth: 24,
            sample_rate: 96_000,
            channels: 2,
            // A handful of frames including both rails.
            samples: vec![0, 0, 1, -1, (1 << 23) - 1, -(1 << 23), 123456, -654321],
        };
        encode_flac(&pcm, &flac_path).unwrap();
        let back = read_flac(&flac_path).unwrap();
        assert_eq!(
            back.samples, pcm.samples,
            "PCM must survive FLAC round-trip"
        );
        assert_eq!(back.bit_depth, pcm.bit_depth);
        assert_eq!(back.sample_rate, pcm.sample_rate);
        assert_eq!(back.channels, pcm.channels);
    }
}
