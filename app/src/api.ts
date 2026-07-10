// Typed wrappers over the Tauri command surface (see src-tauri/src/lib.rs).
// Tauri v2 converts camelCase JS argument keys to snake_case Rust params.

import { invoke } from "@tauri-apps/api/core";

export interface OpenDto {
  id: string;
  sourcePath: string;
  format: string;
  bitDepth: number;
  sampleRate: number;
  channels: number;
  frames: number;
  durationMs: number;
  pcmSha256: string;
}

export interface AnalyzeDto {
  format: string;
  bitDepth: number;
  sampleRate: number;
  channels: number;
  frames: number;
  peakDbfs: string[];
  peakMagnitude: number[];
  clipped: number[];
  totalClipped: number;
  dcOffset: number[];
  silentRegions: [number, number][];
}

export interface WaveDto {
  channels: number;
  columns: number;
  min: number[];
  max: number[];
}

// Mirrors the frozen §13 ProcessReport (snake_case machine layer).
export interface ProcessReport {
  operation: string;
  sample_values_modified: boolean;
  floating_point_used_in_save_path: boolean;
  requantized: boolean;
  dither_used: boolean;
  resampled: boolean;
  clipped_samples: number;
  pcm_verified: boolean;
  output_pcm_sha256?: string;
  processing_log_sha256?: string;
  output_format?: string;
  [k: string]: unknown;
}

export type OutFormat = "flac" | "wav";

export const openAudio = (path: string) => invoke<OpenDto>("open_audio", { path });
export const closeAudio = (id: string) => invoke<void>("close_audio", { id });
export const analyzeAudio = (path: string) => invoke<AnalyzeDto>("analyze_audio", { path });

export const waveform = (id: string, from: number, to: number, columns: number) =>
  invoke<WaveDto>("waveform", { id, from, to, columns });

export const runTrim = (
  input: string,
  output: string,
  from: number,
  to: number,
  format: OutFormat,
  overwrite: boolean,
  jobId: string,
) => invoke<ProcessReport>("run_trim", { input, output, from, to, format, overwrite, jobId });

export const runGain = (
  input: string,
  output: string,
  db: number,
  allowClipping: boolean,
  format: OutFormat,
  overwrite: boolean,
  jobId: string,
) =>
  invoke<ProcessReport>("run_gain", {
    input,
    output,
    db,
    allowClipping,
    format,
    overwrite,
    jobId,
  });

export const runExport16 = (
  input: string,
  output: string,
  seed: number,
  format: OutFormat,
  overwrite: boolean,
  jobId: string,
) =>
  invoke<ProcessReport>("run_export16", { input, output, seed, format, overwrite, jobId });

export const runVerify = (a: string, b?: string) =>
  invoke<ProcessReport>("run_verify", { a, b: b ?? null });

export const runDcCorrect = (
  input: string,
  output: string,
  format: OutFormat,
  overwrite: boolean,
  jobId: string,
) => invoke<ProcessReport>("run_dc_correct", { input, output, format, overwrite, jobId });

export const runFade = (
  input: string,
  output: string,
  kind: "in" | "out",
  durationFrames: number,
  format: OutFormat,
  overwrite: boolean,
  jobId: string,
) =>
  invoke<ProcessReport>("run_fade", {
    input,
    output,
    kind,
    durationFrames,
    format,
    overwrite,
    jobId,
  });

export const runSplit = (
  input: string,
  outDir: string,
  mode: "silence" | "ab",
  album: string | null,
  artist: string | null,
  format: OutFormat,
  overwrite: boolean,
  jobId: string,
) =>
  invoke<ProcessReport>("run_split", {
    input,
    outDir,
    mode,
    album,
    artist,
    format,
    overwrite,
    jobId,
  });

export const cancelJob = (jobId: string) => invoke<void>("cancel_job", { jobId });

// ---- playback ----

export interface PlaybackStatus {
  playing: boolean;
  position: number;
  error?: string;
}

export const playbackLoad = (
  id: string,
  gainDb: number | null,
  fadeInFrames: number,
  fadeOutFrames: number,
) => invoke<void>("playback_load", { id, gainDb, fadeInFrames, fadeOutFrames });
export const playbackPlay = () => invoke<void>("playback_play");
export const playbackPause = () => invoke<void>("playback_pause");
export const playbackStop = () => invoke<void>("playback_stop");
export const playbackSeek = (frame: number) => invoke<void>("playback_seek", { frame });
export const playbackStatus = () => invoke<PlaybackStatus>("playback_status");

// ---- v2 non-destructive project (engine-owned document; snake_case = serde) ----

export interface OpChain {
  dc_correct: boolean;
  gain_db?: number;
  fade_in_frames: number;
  fade_out_frames: number;
  export16_seed?: number;
}

export interface Region {
  id: string;
  source_id: string;
  start_frame: number;
  end_frame: number;
  title: string;
  track_number?: number;
  artist?: string;
  album?: string;
  ops: OpChain;
}

export interface Marker {
  id: string;
  frame: number;
  label: string;
}

export interface SourceRef {
  id: string;
  relative_path: string;
  last_known_absolute_path: string;
  pcm_sha256: string;
  sample_rate: number;
  bit_depth: number;
  channels: number;
  frames: number;
}

export interface ProvenanceEntry {
  action: string;
  detail: string;
}

export interface ProjectDocument {
  project_version: string;
  sources: SourceRef[];
  markers: Marker[];
  regions: Region[];
  history: ProvenanceEntry[];
}

export interface DocSnapshot {
  document: ProjectDocument;
  canUndo: boolean;
  canRedo: boolean;
}

export type ExportKind = "master" | "derivative";

export const projectNew = () => invoke<DocSnapshot>("project_new");
export const projectOpen = (path: string) => invoke<DocSnapshot>("project_open", { path });
export const projectSave = (path: string) => invoke<void>("project_save", { path });
export const docSnapshot = () => invoke<DocSnapshot>("doc_snapshot");

export const addSource = (id: string, path: string) =>
  invoke<DocSnapshot>("add_source", { id, path });

export const addRegion = (region: Region) => invoke<DocSnapshot>("add_region", { region });
export const removeRegion = (id: string) => invoke<DocSnapshot>("remove_region", { id });
export const setRegionRange = (id: string, start: number, end: number) =>
  invoke<DocSnapshot>("set_region_range", { id, start, end });
export const renameRegion = (id: string, title: string) =>
  invoke<DocSnapshot>("rename_region", { id, title });
export const setRegionOps = (id: string, ops: OpChain) =>
  invoke<DocSnapshot>("set_region_ops", { id, ops });
export const reorderRegions = (order: string[]) =>
  invoke<DocSnapshot>("reorder_regions", { order });

export const addMarker = (marker: Marker) => invoke<DocSnapshot>("add_marker", { marker });
export const removeMarker = (id: string) => invoke<DocSnapshot>("remove_marker", { id });
export const moveMarker = (id: string, frame: number) =>
  invoke<DocSnapshot>("move_marker", { id, frame });

export const undo = () => invoke<DocSnapshot>("undo");
export const redo = () => invoke<DocSnapshot>("redo");

export const renderProject = (
  kind: ExportKind,
  outDir: string,
  format: OutFormat,
  overwrite: boolean,
  jobId: string,
) => invoke<ProcessReport[]>("render_project", { kind, outDir, format, overwrite, jobId });
