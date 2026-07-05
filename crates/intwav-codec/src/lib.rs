//! `intwav-codec` — integer PCM I/O for intwav.
//!
//! Decodes WAV (via `hound`) and FLAC (via the pure-Rust `claxon`) into
//! interleaved `i32` samples, and writes WAV directly or FLAC by shelling out
//! to the `flac` command-line encoder. No sample value is ever routed through
//! floating point:
//!
//! * Float WAV (`WAVE_FORMAT_IEEE_FLOAT`) and other unsupported inputs are
//!   rejected with an explicit error — never silently converted (spec §8.3,
//!   §19.2).
//! * FLAC decoding uses `claxon`, which yields integer samples.
//! * FLAC encoding delegates to the external `flac` binary, keeping libFLAC's
//!   internal floating-point analysis out of this process entirely.

mod flac;
mod wav;

use std::path::Path;

pub use flac::{encode_flac, read_flac, read_flac_tags};
pub use wav::{read_wav, write_wav};

/// Ordered Vorbis-comment metadata for FLAC output. Keys are conventionally
/// uppercase (`TITLE`, `ARTIST`, `ALBUM`, `TRACKNUMBER`, `DATE`, `GENRE`,
/// `COMMENT`, and archival tags like `SOURCE_MEDIA`). Duplicate keys are
/// permitted by Vorbis and preserved in order.
pub type Metadata = Vec<(String, String)>;

/// Interleaved integer PCM plus its stream parameters. This is the single
/// representation shared across the tool; `samples` are handed to `intwav-core`
/// as `&[i32]` slices without conversion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PcmBuffer {
    pub bit_depth: u16,
    pub sample_rate: u32,
    pub channels: u16,
    /// Interleaved samples, length is `frames * channels`.
    pub samples: Vec<i32>,
}

impl PcmBuffer {
    /// Number of frames (samples per channel).
    pub fn frames(&self) -> u64 {
        if self.channels == 0 {
            0
        } else {
            self.samples.len() as u64 / self.channels as u64
        }
    }

    /// Duration in whole nanoseconds (integer; no float).
    pub fn duration_ns(&self) -> u128 {
        if self.sample_rate == 0 {
            0
        } else {
            self.frames() as u128 * 1_000_000_000u128 / self.sample_rate as u128
        }
    }
}

/// Source container format detected for an input file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceFormat {
    Wav,
    Flac,
}

impl SourceFormat {
    pub fn as_str(&self) -> &'static str {
        match self {
            SourceFormat::Wav => "WAV",
            SourceFormat::Flac => "FLAC",
        }
    }
}

/// Output container the caller wants written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Wav,
    Flac,
}

/// Errors from decoding, validation, or encoding. Malformed or unsupported
/// input yields an explicit error rather than a panic (spec §19).
#[derive(Debug, thiserror::Error)]
pub enum CodecError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("WAV error: {0}")]
    Wav(#[from] hound::Error),
    #[error("FLAC decode error: {0}")]
    FlacDecode(#[from] claxon::Error),
    #[error(
        "floating-point WAV is not supported: this tool preserves integer PCM only \
         and will not convert (spec §8.3)"
    )]
    FloatWav,
    #[error("unsupported PCM: {0}")]
    Unsupported(String),
    #[error("could not determine format from extension of {0:?} (expected .wav or .flac)")]
    UnknownExtension(std::path::PathBuf),
    #[error(
        "the `flac` encoder is required for FLAC output but was not found on PATH \
         (install it, or use --output-format wav)"
    )]
    FlacEncoderMissing,
    #[error("`flac` encoder failed: {0}")]
    FlacEncode(String),
    #[error(transparent)]
    Core(#[from] intwav_core::CoreError),
}

/// Detect the source format from a path's extension (case-insensitive).
pub fn detect_format(path: &Path) -> Result<SourceFormat, CodecError> {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .as_deref()
    {
        Some("wav") => Ok(SourceFormat::Wav),
        Some("flac") => Ok(SourceFormat::Flac),
        _ => Err(CodecError::UnknownExtension(path.to_path_buf())),
    }
}

/// Stream parameters read from a file header, without decoding samples.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioSpec {
    pub bit_depth: u16,
    pub sample_rate: u32,
    pub channels: u16,
    /// Total frames, if the header records it (`None` for some FLAC streams).
    pub frames: Option<u64>,
}

/// Probe a file's stream parameters from its header (cheap — no sample decode).
/// Applies the same format/shape validation as [`read`].
pub fn probe(path: &Path) -> Result<(AudioSpec, SourceFormat), CodecError> {
    let format = detect_format(path)?;
    let spec = match format {
        SourceFormat::Wav => {
            let reader = hound::WavReader::open(path)?;
            let s = reader.spec();
            if s.sample_format == hound::SampleFormat::Float {
                return Err(CodecError::FloatWav);
            }
            validate_shape(s.bits_per_sample, s.channels)?;
            let frames = (reader.len() as u64) / (s.channels as u64).max(1);
            AudioSpec {
                bit_depth: s.bits_per_sample,
                sample_rate: s.sample_rate,
                channels: s.channels,
                frames: Some(frames),
            }
        }
        SourceFormat::Flac => {
            let reader = claxon::FlacReader::open(path)?;
            let info = reader.streaminfo();
            let bit_depth = info.bits_per_sample as u16;
            let channels = info.channels as u16;
            validate_shape(bit_depth, channels)?;
            AudioSpec {
                bit_depth,
                sample_rate: info.sample_rate,
                channels,
                frames: info.samples,
            }
        }
    };
    Ok((spec, format))
}

/// Decode any supported input into a [`PcmBuffer`], dispatching on extension.
pub fn read(path: &Path) -> Result<(PcmBuffer, SourceFormat), CodecError> {
    let format = detect_format(path)?;
    let pcm = match format {
        SourceFormat::Wav => read_wav(path)?,
        SourceFormat::Flac => read_flac(path)?,
    };
    Ok((pcm, format))
}

/// Validate that a bit depth / channel count is one this tool accepts.
pub(crate) fn validate_shape(bit_depth: u16, channels: u16) -> Result<(), CodecError> {
    if !matches!(bit_depth, 16 | 24 | 32) {
        return Err(CodecError::Unsupported(format!(
            "bit depth {bit_depth} (only 16, 24, and 32-bit integer PCM are supported)"
        )));
    }
    if !matches!(channels, 1 | 2) {
        return Err(CodecError::Unsupported(format!(
            "{channels} channels (only mono and stereo are supported)"
        )));
    }
    Ok(())
}
