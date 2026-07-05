//! Closed error taxonomy shared verbatim by the CLI and the (future) GUI.
//!
//! Each variant has a **stable string code** that is part of the frozen public
//! contract: adding a code later is fine, renaming one is breaking. The GUI
//! switches on `code` to pick a localized dialog; `message` is an English
//! fallback and is never parsed for meaning.

use std::fmt;

use intwav_codec::CodecError;
use intwav_core::CoreError;

/// Stable, machine-readable error categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    UnsupportedFormat,
    FloatWavRejected,
    UnsupportedBitDepth,
    UnknownExtension,
    RangeOutOfBounds,
    InvalidParameter,
    ClipRefused,
    FlacEncoderMissing,
    PcmVerifyFailed,
    SourceHashMismatch,
    SourceMissing,
    OutputExists,
    MasterExportRequantizeRefused,
    Cancelled,
    ProjectParseError,
    IoError,
}

impl ErrorCode {
    /// The frozen wire string for this code.
    pub fn as_str(&self) -> &'static str {
        match self {
            ErrorCode::UnsupportedFormat => "UNSUPPORTED_FORMAT",
            ErrorCode::FloatWavRejected => "FLOAT_WAV_REJECTED",
            ErrorCode::UnsupportedBitDepth => "UNSUPPORTED_BIT_DEPTH",
            ErrorCode::UnknownExtension => "UNKNOWN_EXTENSION",
            ErrorCode::RangeOutOfBounds => "RANGE_OUT_OF_BOUNDS",
            ErrorCode::InvalidParameter => "INVALID_PARAMETER",
            ErrorCode::ClipRefused => "CLIP_REFUSED",
            ErrorCode::FlacEncoderMissing => "FLAC_ENCODER_MISSING",
            ErrorCode::PcmVerifyFailed => "PCM_VERIFY_FAILED",
            ErrorCode::SourceHashMismatch => "SOURCE_HASH_MISMATCH",
            ErrorCode::SourceMissing => "SOURCE_MISSING",
            ErrorCode::OutputExists => "OUTPUT_EXISTS",
            ErrorCode::MasterExportRequantizeRefused => "MASTER_EXPORT_REQUANTIZE_REFUSED",
            ErrorCode::Cancelled => "CANCELLED",
            ErrorCode::ProjectParseError => "PROJECT_PARSE_ERROR",
            ErrorCode::IoError => "IO_ERROR",
        }
    }
}

/// A coded engine error: a stable `code`, a human `message`, and optional
/// `detail` for diagnostics.
#[derive(Debug, Clone)]
pub struct EngineError {
    pub code: ErrorCode,
    pub message: String,
    pub detail: Option<String>,
}

impl EngineError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            detail: None,
        }
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }
}

impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}", self.code.as_str(), self.message)?;
        if let Some(detail) = &self.detail {
            write!(f, " ({detail})")?;
        }
        Ok(())
    }
}

impl std::error::Error for EngineError {}

pub type EngineResult<T> = Result<T, EngineError>;

impl From<CoreError> for EngineError {
    fn from(e: CoreError) -> Self {
        let code = match e {
            CoreError::RangeOutOfBounds { .. } => ErrorCode::RangeOutOfBounds,
            CoreError::UnsupportedBitDepth(_) => ErrorCode::UnsupportedBitDepth,
            CoreError::ZeroChannels
            | CoreError::RaggedInterleave { .. }
            | CoreError::ChannelMismatch { .. } => ErrorCode::InvalidParameter,
        };
        EngineError::new(code, e.to_string())
    }
}

impl From<CodecError> for EngineError {
    fn from(e: CodecError) -> Self {
        let code = match e {
            CodecError::FloatWav => ErrorCode::FloatWavRejected,
            CodecError::Unsupported(_) => ErrorCode::UnsupportedFormat,
            CodecError::UnknownExtension(_) => ErrorCode::UnknownExtension,
            CodecError::FlacEncoderMissing => ErrorCode::FlacEncoderMissing,
            CodecError::Core(ref c) => return EngineError::from(c.clone()),
            _ => ErrorCode::IoError,
        };
        EngineError::new(code, e.to_string())
    }
}

impl From<std::io::Error> for EngineError {
    fn from(e: std::io::Error) -> Self {
        EngineError::new(ErrorCode::IoError, e.to_string())
    }
}
