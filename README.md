# intwav

[English](README.md) | [日本語](README.ja.md) | [Français](README.fr.md) | [Español](README.es.md) | [Deutsch](README.de.md) | [简体中文](README.zh-CN.md) | [한국어](README.ko.md)

**Integer-PCM protection tool for audio processing** — archiving analog transfers (records, reels, cassettes) digitized to 24-bit PCM.

> Preserving 24-bit PCM exactly as captured. Not audio enhancement — audio preservation.

intwav inspects, trims, and losslessly archives integer PCM **without** floating-point conversion, requantization, or resampling. It is not a DAW and does not "improve" audio — it preserves the PCM exactly as captured and stores it as lossless FLAC, with an explainable, logged processing path.

## Status: v0.1

Implemented commands:

| Command | Purpose |
|---|---|
| `intwav info <in>`   | Format, parameters, duration, peak, clip count |
| `intwav check <in>`  | Full inspection: info + DC offset + silence detection |
| `intwav peak <in>`   | Per-channel peak level (dBFS + raw) |
| `intwav clips <in>`  | Clipped-sample counts |
| `intwav trim <in> [out] --from <ts> --to <ts>` | Extract a range, sample values unchanged |

Timestamps are `HH:MM:SS.mmm`, `MM:SS.mmm`, `SS.mmm`, or plain seconds.
`trim` accepts `--output-format flac|wav` (default: infer from the output extension, else FLAC) and `--report <path>` for a JSON processing report (§13).

### Formats

* Input: WAV and FLAC, 16/24/32-bit **integer** PCM, mono or stereo.
* Output: FLAC (default) or WAV.
* Float WAV, compressed WAV, MP3/AAC/Opus, DSD, and multichannel are **rejected** with an explicit error — never silently converted.

## The float-free guarantee

All sample math lives in `intwav-core`, which is `no_std` + `alloc`, has no dependencies, and uses **no floating point** — including dBFS, which is computed with a fixed-point integer log approximation (accuracy < 0.004 dB). FLAC decoding uses the pure-Rust `claxon`; FLAC encoding is delegated to the external `flac` binary so libFLAC's internal float analysis never enters this process.

`scripts/check-no-float.sh` enforces this in CI: it scans the core source for float constructs and disassembles the compiled core object, failing the build if any floating-point arithmetic instruction (x86-64 SSE/x87 or aarch64 FP) appears.

## Architecture

```
crates/
  intwav-core   integer-only processing: analysis, dBFS, frame slicing (float-scanned)
  intwav-codec  WAV (hound) + FLAC (claxon decode / flac-CLI encode) integer I/O
  intwav-cli    the `intwav` binary: command parsing, file I/O, JSON reports
```

## Build & test

```bash
cargo build --release          # binary at target/release/intwav
cargo test --workspace         # unit + end-to-end tests
bash scripts/check-no-float.sh # verify the float-free guarantee
```

Requires the `flac` command-line tool for FLAC output.

## License
Apache-2.0
