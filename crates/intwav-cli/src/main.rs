//! `intwav` — integer PCM protection tool (CLI).
//!
//! v0.1 commands: info, check, peak, clips, trim. File I/O and report emission
//! live here; all sample math is delegated to `intwav-core` (float-free) and
//! decoding/encoding to `intwav-codec`.

mod commands;
mod format;
mod report;
mod timecode;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use intwav_codec::OutputFormat;

#[derive(Parser)]
#[command(
    name = "intwav",
    version,
    about = "Integer PCM protection tool — inspect, trim, and losslessly archive 24-bit PCM",
    long_about = "intwav inspects and trims integer PCM (WAV/FLAC) without floating-point \
                  conversion, requantization, or resampling, and stores results as lossless FLAC."
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
        /// Output container (default: infer from output extension, else flac).
        #[arg(long = "output-format", value_enum)]
        output_format: Option<CliOutputFormat>,
        /// Write a JSON processing report to this path.
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
