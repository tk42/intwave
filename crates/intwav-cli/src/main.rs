//! `intwav` — integer PCM protection tool (CLI).
//!
//! Commands: info, check, peak, clips, trim, split, gain, fade-in, fade-out,
//! dc-correct, export16, verify. File I/O and report emission live here; all
//! sample math is delegated to `intwav-core` (float-free) and decoding/encoding
//! to `intwav-codec`.

mod commands;
mod format;
mod hash;
mod params;
mod report;
mod timecode;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use intwav_codec::OutputFormat;

use commands::SplitMode;

#[derive(Parser)]
#[command(
    name = "intwav",
    version,
    about = "Integer PCM protection tool — inspect, trim, split, and losslessly archive integer PCM",
    long_about = "intwav inspects, trims, splits, and archives integer PCM (WAV/FLAC) without \
                  floating-point conversion, requantization, or resampling, storing results as \
                  lossless FLAC."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Clone, Copy, ValueEnum)]
enum CliOutputFormat {
    Flac,
    Wav,
}

impl From<CliOutputFormat> for OutputFormat {
    fn from(v: CliOutputFormat) -> Self {
        match v {
            CliOutputFormat::Flac => OutputFormat::Flac,
            CliOutputFormat::Wav => OutputFormat::Wav,
        }
    }
}

/// Split boundary selection.
#[derive(Clone, Copy, ValueEnum)]
enum CliSplitBy {
    Silence,
    Ab,
}

#[derive(Subcommand)]
enum Command {
    /// Show format, parameters, duration, peak, and clip count.
    Info { input: PathBuf },
    /// Full inspection: info plus DC offset and silence detection.
    Check { input: PathBuf },
    /// Report peak level per channel.
    Peak { input: PathBuf },
    /// Report clipped-sample counts.
    Clips { input: PathBuf },
    /// Extract a time range without altering sample values.
    Trim {
        input: PathBuf,
        output: PathBuf,
        /// Range start, e.g. 00:01:23.000
        #[arg(long)]
        from: String,
        /// Range end, e.g. 00:05:41.500
        #[arg(long)]
        to: String,
        #[arg(long = "output-format", value_enum)]
        output_format: Option<CliOutputFormat>,
        #[arg(long)]
        report: Option<PathBuf>,
    },
    /// Split into tracks by CUE list, silence, or A/B side.
    Split {
        input: PathBuf,
        /// Output directory for the track files.
        #[arg(long)]
        out: PathBuf,
        /// CUE-style track list (`timestamp title` per line).
        #[arg(long, conflicts_with = "by")]
        cue: Option<PathBuf>,
        /// Automatic split mode (when not using --cue).
        #[arg(long, value_enum, conflicts_with = "cue")]
        by: Option<CliSplitBy>,
        #[arg(long = "output-format", value_enum)]
        output_format: Option<CliOutputFormat>,
        /// ALBUM tag applied to every track.
        #[arg(long)]
        album: Option<String>,
        /// ARTIST tag applied to every track.
        #[arg(long)]
        artist: Option<String>,
        #[arg(long)]
        report: Option<PathBuf>,
    },
    /// Apply a fixed-point gain in integer dB (-96..=24).
    Gain {
        input: PathBuf,
        output: PathBuf,
        #[arg(long, allow_hyphen_values = true)]
        db: i32,
        /// Permit positive-gain clipping (otherwise it is refused).
        #[arg(long)]
        allow_clipping: bool,
        #[arg(long = "output-format", value_enum)]
        output_format: Option<CliOutputFormat>,
        #[arg(long)]
        report: Option<PathBuf>,
    },
    /// Linear fade-in over a duration (e.g. 5s, 250ms).
    FadeIn {
        input: PathBuf,
        output: PathBuf,
        #[arg(long)]
        duration: String,
        #[arg(long = "output-format", value_enum)]
        output_format: Option<CliOutputFormat>,
        #[arg(long)]
        report: Option<PathBuf>,
    },
    /// Linear fade-out over a duration (e.g. 5s, 250ms).
    FadeOut {
        input: PathBuf,
        output: PathBuf,
        #[arg(long)]
        duration: String,
        #[arg(long = "output-format", value_enum)]
        output_format: Option<CliOutputFormat>,
        #[arg(long)]
        report: Option<PathBuf>,
    },
    /// Remove per-channel DC offset.
    DcCorrect {
        input: PathBuf,
        output: PathBuf,
        #[arg(long = "output-format", value_enum)]
        output_format: Option<CliOutputFormat>,
        #[arg(long)]
        report: Option<PathBuf>,
    },
    /// Derivative 16-bit output with TPDF dither (not a preservation master).
    Export16 {
        input: PathBuf,
        output: PathBuf,
        /// Dither type (only `tpdf` is supported).
        #[arg(long, default_value = "tpdf")]
        dither: String,
        /// PRNG seed for reproducible dither.
        #[arg(long, default_value_t = 1)]
        seed: u32,
        #[arg(long = "output-format", value_enum)]
        output_format: Option<CliOutputFormat>,
        #[arg(long)]
        report: Option<PathBuf>,
    },
    /// Checksum a file's PCM, or verify two files carry identical PCM.
    Verify {
        input: PathBuf,
        /// Optional second file to compare against.
        other: Option<PathBuf>,
        #[arg(long)]
        report: Option<PathBuf>,
    },
}

fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Info { input } => commands::cmd_info(&input),
        Command::Check { input } => commands::cmd_check(&input),
        Command::Peak { input } => commands::cmd_peak(&input),
        Command::Clips { input } => commands::cmd_clips(&input),
        Command::Trim {
            input,
            output,
            from,
            to,
            output_format,
            report,
        } => commands::cmd_trim(
            &input,
            &output,
            &from,
            &to,
            output_format.map(Into::into),
            report.as_deref(),
        ),
        Command::Split {
            input,
            out,
            cue,
            by,
            output_format,
            album,
            artist,
            report,
        } => {
            let mode = match (cue, by) {
                (Some(path), _) => SplitMode::Cue(path),
                (None, Some(CliSplitBy::Silence)) => SplitMode::Silence,
                (None, Some(CliSplitBy::Ab)) => SplitMode::Ab,
                (None, None) => {
                    anyhow::bail!("specify --cue <file> or --by <silence|ab>")
                }
            };
            commands::cmd_split(
                &input,
                &out,
                mode,
                output_format.map(Into::into),
                album.as_deref(),
                artist.as_deref(),
                report.as_deref(),
            )
        }
        Command::Gain {
            input,
            output,
            db,
            allow_clipping,
            output_format,
            report,
        } => commands::cmd_gain(
            &input,
            &output,
            db,
            allow_clipping,
            output_format.map(Into::into),
            report.as_deref(),
        ),
        Command::FadeIn {
            input,
            output,
            duration,
            output_format,
            report,
        } => commands::cmd_fade_in(
            &input,
            &output,
            &duration,
            output_format.map(Into::into),
            report.as_deref(),
        ),
        Command::FadeOut {
            input,
            output,
            duration,
            output_format,
            report,
        } => commands::cmd_fade_out(
            &input,
            &output,
            &duration,
            output_format.map(Into::into),
            report.as_deref(),
        ),
        Command::DcCorrect {
            input,
            output,
            output_format,
            report,
        } => commands::cmd_dc_correct(
            &input,
            &output,
            output_format.map(Into::into),
            report.as_deref(),
        ),
        Command::Export16 {
            input,
            output,
            dither,
            seed,
            output_format,
            report,
        } => commands::cmd_export16(
            &input,
            &output,
            &dither,
            seed,
            output_format.map(Into::into),
            report.as_deref(),
        ),
        Command::Verify {
            input,
            other,
            report,
        } => commands::cmd_verify(&input, other.as_deref(), report.as_deref()),
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            // Explicit, non-panicking error reporting (spec §19).
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    }
}
