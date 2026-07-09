# CLAUDE.md

Guidance for working in this repository.

## What this is

`intwav` — an integer-PCM archival CLI. Core principle: **preserve** 24-bit
integer PCM without floating-point conversion, requantization, or resampling.
See `README.md` for the product framing and the spec for full requirements.

## The non-negotiable invariant

`intwav-core` must stay **float-free**. This is the product's entire reason to
exist. Concretely, in `crates/intwav-core/`:

- No `f32`/`f64`, no `as f32`/`as f64`, no decimal literals, no `libm`.
- No external dependencies; keep it `no_std` + `alloc`.
- Anything logarithmic/decibel-related uses the fixed-point routines in
  `dbfs.rs` (integer log2 table + scaling), not float math.

`scripts/check-no-float.sh` enforces this by scanning source **and** the
disassembled release object. Run it after any change to core:

```bash
bash scripts/check-no-float.sh
```

The check covers **two crates**: a source-token ban on `intwav-core` **and**
`intwav-engine` (the save-path orchestrator has no legitimate float need —
progress is integer permille, ratios are raw byte/sample counts), plus a
disassembly scan of the `intwav-core` object only (the engine links the codec's
float FLAC/WAV libs, so its object can't be cleanly disassembled). The codec,
CLI, and future GUI/playback crates may use float and are not scanned. FLAC
encoding is deliberately delegated to the external `flac` binary (see
`intwav-codec/src/flac.rs`) to keep libFLAC's float analysis out of process; the
engine takes a configurable `flac` path so the GUI can inject a bundled sidecar.

## Layout

- `crates/intwav-core` — all sample math: `analyze`, `dbfs_centibels`,
  `frame_slice`, gain (`apply_gain_q31`, `gain_q31_for_db`), fades
  (`apply_fade_in/out`), `apply_dc_correction`, and `requantize_to_16` + `Rng`
  (`dither.rs`). Gain uses a precomputed Q31 table; no `pow`/float anywhere.
- `crates/intwav-codec` — `PcmBuffer`, `Metadata`, WAV/FLAC read (`read`), header
  `probe`, memory-bounded `stream_decode` (block callback), WAV write, FLAC
  encode (Vorbis tags, configurable `flac` path). Decode never routes samples
  through float; unsupported/float input is an explicit error, never a silent
  conversion.
- `crates/intwav-engine` — the shared CLI/GUI engine (float-free in source). The
  operations (`trim`/`split`/`gain`/`fade`/`dc_correct`/`export16`/`verify`/
  `analyze_file`) are synchronous and caller-driven (`ProgressSink` +
  `CancelToken`); the frozen §13 `ProcessReport`, coded `EngineError`, verified
  atomic writes (`write_verified` → `pcm_verified`), and SHA-256 helpers live
  here. `open_source` decodes a source **once** into a seekable scratch file
  (`ScratchReader`/`ScratchWriter`) while building the waveform pyramid
  (`WaveformBuilder`) and PCM hash in the same pass — the GUI's memory spine.
  Ops take typed params (frames/dB), never strings. The **non-destructive
  project model** also lives here: `Document` (sources/regions/markers/`OpChain`/
  provenance), the command-pattern `Editor` (volatile undo/redo + append-only
  provenance), `.iwproj` save/open with relative-path source refs + hash verify,
  and `render_region`/`render_document` (fixed canonical op-chain, engine-enforced
  Master vs Derivative `ExportKind` invariant).
- `crates/intwav-playback` — preview playback (**off** the save path, NOT
  float-scanned — this is the one place float is legitimate). `Feeder` turns a
  `FrameSource` (scratch or in-memory) into `f32` blocks, applying the same
  integer gain/fade op-chain as export and converting to `f32` only at the end;
  `LinearResampler` is the device-rate fallback. The cpal `Player` is behind the
  default `device` feature; the feeder/resampler are unit-tested without hardware.
- `crates/intwav-cli` — the `intwav` binary; a thin front-end over the engine.
  One submodule per command group under `commands/`; argument/timecode/CUE
  parsing in `params.rs`/`timecode.rs`. Output-producing commands take
  `--overwrite`/`-f` (the engine refuses `OUTPUT_EXISTS` otherwise).
- `app/` — the Tauri v2 + React/TypeScript desktop GUI. `src-tauri/` is a
  Cargo crate **detached** from the core workspace (its own `[workspace]`) so
  the heavy Tauri build never slows the core workspace or CI; it path-depends on
  the engine and exposes it as Tauri commands (open/analyze/waveform/trim/gain/
  export16/verify + progress events + cancel). Build it separately (`npm run
  tauri dev` from `app`), not via the root `cargo` commands.

## Conventions

- No panics on bad input — return errors (`CoreError` / `CodecError` / `anyhow`).
- Sample values are frames of interleaved `i32`; 24-bit samples are sign-extended
  into `i32`. "sample" in reports/timestamps means frame index.
- Trimming must never alter sample values (verified by tests that binary-compare
  decoded PCM).

## Checks before committing

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
bash scripts/check-no-float.sh
```
