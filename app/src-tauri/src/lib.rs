//! intwav GUI backend: Tauri commands over `intwav-engine`.
//!
//! Every command delegates to the same engine the CLI uses (CLI↔GUI parity),
//! so the GUI never re-implements save-path logic. Long operations run with a
//! Tauri-event `ProgressSink` and a cancellable `CancelToken` registered by job
//! id. Numbers cross the boundary as sample frames / integer dB — the frontend
//! resolves timecodes to frames.

use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use intwav_engine::{
    analyze_file, dc_correct, export16 as engine_export16, gain as engine_gain, open_project,
    open_source, render_document, save_project, source_ref_from_file, trim as engine_trim,
    verify as engine_verify, CancelToken, Command, DcParams, Document, Editor, EngineConfig,
    Export16Params, ExportKind, GainParams, Marker, OpChain, OpenParams, OutputFormat,
    ProcessReport, ProgressSink, Region, ScratchReader, TrimParams, WaveformPyramid,
};

/// A decoded, scratch-backed source held open in the GUI.
struct Session {
    source_path: PathBuf,
    scratch_path: PathBuf,
    #[allow(dead_code)]
    reader: Mutex<ScratchReader>,
    waveform: WaveformPyramid,
    frames: u64,
}

pub struct AppState {
    sessions: Mutex<HashMap<String, Session>>,
    cancels: Mutex<HashMap<String, CancelToken>>,
    config: EngineConfig,
    counter: AtomicU64,
    /// The engine-owned project document + undo/redo (Q8-B).
    editor: Mutex<Editor>,
    /// Directory of the current `.iwproj`, for relative source resolution.
    project_dir: Mutex<Option<PathBuf>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
            cancels: Mutex::new(HashMap::new()),
            config: EngineConfig::default(),
            counter: AtomicU64::new(0),
            editor: Mutex::new(Editor::new(Document::new())),
            project_dir: Mutex::new(None),
        }
    }
}

impl AppState {
    /// Build state with an explicit FLAC encoder path (the bundled sidecar).
    fn with_flac(flac_exe: OsString) -> Self {
        let mut s = Self::default();
        s.config.flac_exe = flac_exe;
        s
    }
}

/// Locate the FLAC encoder. In a bundled app the sidecar sits next to the
/// executable (Tauri strips the target-triple suffix); in dev we fall back to
/// `flac` on `PATH`.
fn resolve_flac() -> OsString {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let name = if cfg!(windows) { "flac.exe" } else { "flac" };
            let candidate = dir.join(name);
            if candidate.exists() {
                return candidate.into_os_string();
            }
        }
    }
    OsString::from("flac")
}

/// A snapshot of the document plus undo/redo availability. The frontend is a
/// thin view that re-renders from this after every mutation (Q8-B).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DocSnapshot {
    document: Document,
    can_undo: bool,
    can_redo: bool,
}

fn snapshot(ed: &Editor) -> DocSnapshot {
    DocSnapshot {
        document: ed.document().clone(),
        can_undo: ed.can_undo(),
        can_redo: ed.can_redo(),
    }
}

fn apply_cmd(state: &State<'_, AppState>, cmd: Command) -> Result<DocSnapshot, String> {
    let mut ed = state.editor.lock().unwrap();
    ed.apply(cmd).map_err(|e| e.to_string())?;
    Ok(snapshot(&ed))
}

fn parent_dir(path: &str) -> PathBuf {
    PathBuf::from(path)
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

// ------------------------------------------------------------ progress

#[derive(Clone, Serialize)]
struct ProgressEvent {
    job_id: String,
    permille: u32,
}

struct EventProgress {
    app: AppHandle,
    job_id: String,
}

impl ProgressSink for EventProgress {
    fn set_permille(&self, permille: u32) {
        let _ = self.app.emit(
            "progress",
            ProgressEvent {
                job_id: self.job_id.clone(),
                permille,
            },
        );
    }
}

// ---------------------------------------------------------------- DTOs

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OpenDto {
    id: String,
    source_path: String,
    format: String,
    bit_depth: u16,
    sample_rate: u32,
    channels: u16,
    frames: u64,
    duration_ms: u64,
    pcm_sha256: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AnalyzeDto {
    format: String,
    bit_depth: u16,
    sample_rate: u32,
    channels: u16,
    frames: u64,
    peak_dbfs: Vec<String>,
    peak_magnitude: Vec<i64>,
    clipped: Vec<u64>,
    total_clipped: u64,
    dc_offset: Vec<i64>,
    silent_regions: Vec<[u64; 2]>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WaveDto {
    channels: usize,
    columns: usize,
    /// Interleaved per column per channel: index = col * channels + ch.
    min: Vec<i16>,
    max: Vec<i16>,
}

fn duration_ms(frames: u64, rate: u32) -> u64 {
    if rate == 0 {
        0
    } else {
        (frames as u128 * 1000 / rate as u128) as u64
    }
}

fn parse_format(s: &str) -> OutputFormat {
    if s.eq_ignore_ascii_case("wav") {
        OutputFormat::Wav
    } else {
        OutputFormat::Flac
    }
}

// ------------------------------------------------------------ commands

#[tauri::command]
fn open_audio(state: State<'_, AppState>, path: String) -> Result<OpenDto, String> {
    let input = PathBuf::from(&path);
    let id = format!("s{}", state.counter.fetch_add(1, Ordering::Relaxed));
    let scratch_path = std::env::temp_dir().join(format!("intwav-{id}.iwscr"));

    let open = open_source(
        &input,
        &scratch_path,
        &OpenParams::default(),
        &intwav_engine::NoProgress,
        &CancelToken::new(),
    )
    .map_err(|e| e.to_string())?;

    let reader = ScratchReader::open(&scratch_path).map_err(|e| e.to_string())?;
    let frames = open.spec.frames.unwrap_or(0);
    let dto = OpenDto {
        id: id.clone(),
        source_path: path.clone(),
        format: open.format.as_str().to_string(),
        bit_depth: open.spec.bit_depth,
        sample_rate: open.spec.sample_rate,
        channels: open.spec.channels,
        frames,
        duration_ms: duration_ms(frames, open.spec.sample_rate),
        pcm_sha256: open.pcm_sha256.clone(),
    };

    let session = Session {
        source_path: input,
        scratch_path,
        reader: Mutex::new(reader),
        waveform: open.waveform,
        frames,
    };
    // Replace any previous session under this id, removing its scratch file.
    if let Some(prev) = state.sessions.lock().unwrap().insert(id, session) {
        let _ = std::fs::remove_file(prev.scratch_path);
    }
    Ok(dto)
}

#[tauri::command]
fn close_audio(state: State<'_, AppState>, id: String) {
    if let Some(sess) = state.sessions.lock().unwrap().remove(&id) {
        let _ = std::fs::remove_file(sess.scratch_path);
    }
}

#[tauri::command]
fn analyze_audio(path: String) -> Result<AnalyzeDto, String> {
    let a = analyze_file(&PathBuf::from(path), None).map_err(|e| e.to_string())?;
    let peak_dbfs = a
        .peak_centibels
        .iter()
        .map(|&cb| intwav_engine::format_dbfs(cb))
        .collect();
    Ok(AnalyzeDto {
        format: a.format,
        bit_depth: a.bit_depth,
        sample_rate: a.sample_rate,
        channels: a.channels,
        frames: a.frames,
        peak_dbfs,
        peak_magnitude: a.peak_magnitude,
        clipped: a.clipped,
        total_clipped: a.total_clipped,
        dc_offset: a.dc_offset,
        silent_regions: a
            .silent_regions
            .iter()
            .map(|r| [r.start_frame, r.end_frame])
            .collect(),
    })
}

#[tauri::command]
fn waveform(
    state: State<'_, AppState>,
    id: String,
    from: u64,
    to: u64,
    columns: usize,
) -> Result<WaveDto, String> {
    let sessions = state.sessions.lock().unwrap();
    let sess = sessions.get(&id).ok_or("unknown session")?;
    Ok(viewport_waveform(
        &sess.waveform,
        from.min(sess.frames),
        to.min(sess.frames),
        columns.max(1),
    ))
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
fn run_trim(
    app: AppHandle,
    state: State<'_, AppState>,
    input: String,
    output: String,
    from: u64,
    to: u64,
    format: String,
    overwrite: bool,
    job_id: String,
) -> Result<ProcessReport, String> {
    let cancel = register_job(&state, &job_id);
    let progress = EventProgress {
        app,
        job_id: job_id.clone(),
    };
    let params = TrimParams {
        from_frame: from,
        to_frame: to,
        format: parse_format(&format),
        overwrite,
    };
    let r = engine_trim(
        &PathBuf::from(input),
        &PathBuf::from(output),
        &params,
        &state.config,
        &progress,
        &cancel,
    )
    .map_err(|e| e.to_string());
    unregister_job(&state, &job_id);
    r
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
fn run_gain(
    app: AppHandle,
    state: State<'_, AppState>,
    input: String,
    output: String,
    db: i32,
    allow_clipping: bool,
    format: String,
    overwrite: bool,
    job_id: String,
) -> Result<ProcessReport, String> {
    let cancel = register_job(&state, &job_id);
    let progress = EventProgress {
        app,
        job_id: job_id.clone(),
    };
    let params = GainParams {
        db,
        allow_clipping,
        format: parse_format(&format),
        overwrite,
    };
    let r = engine_gain(
        &PathBuf::from(input),
        &PathBuf::from(output),
        &params,
        &state.config,
        &progress,
        &cancel,
    )
    .map_err(|e| e.to_string());
    unregister_job(&state, &job_id);
    r
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
fn run_export16(
    app: AppHandle,
    state: State<'_, AppState>,
    input: String,
    output: String,
    seed: u32,
    format: String,
    overwrite: bool,
    job_id: String,
) -> Result<ProcessReport, String> {
    let cancel = register_job(&state, &job_id);
    let progress = EventProgress {
        app,
        job_id: job_id.clone(),
    };
    let params = Export16Params {
        seed,
        format: parse_format(&format),
        overwrite,
    };
    let r = engine_export16(
        &PathBuf::from(input),
        &PathBuf::from(output),
        &params,
        &state.config,
        &progress,
        &cancel,
    )
    .map_err(|e| e.to_string());
    unregister_job(&state, &job_id);
    r
}

#[tauri::command]
fn run_dc_correct(
    app: AppHandle,
    state: State<'_, AppState>,
    input: String,
    output: String,
    format: String,
    overwrite: bool,
    job_id: String,
) -> Result<ProcessReport, String> {
    let cancel = register_job(&state, &job_id);
    let progress = EventProgress {
        app,
        job_id: job_id.clone(),
    };
    let params = DcParams {
        format: parse_format(&format),
        overwrite,
    };
    let r = dc_correct(
        &PathBuf::from(input),
        &PathBuf::from(output),
        &params,
        &state.config,
        &progress,
        &cancel,
    )
    .map_err(|e| e.to_string());
    unregister_job(&state, &job_id);
    r
}

#[tauri::command]
fn run_verify(a: String, b: Option<String>) -> Result<ProcessReport, String> {
    let b_path = b.map(PathBuf::from);
    let (report, _mismatch) =
        engine_verify(&PathBuf::from(a), b_path.as_deref()).map_err(|e| e.to_string())?;
    Ok(report)
}

#[tauri::command]
fn cancel_job(state: State<'_, AppState>, job_id: String) {
    if let Some(tok) = state.cancels.lock().unwrap().get(&job_id) {
        tok.cancel();
    }
}

// ------------------------------------------------------------- helpers

fn register_job(state: &State<'_, AppState>, job_id: &str) -> CancelToken {
    let tok = CancelToken::new();
    state
        .cancels
        .lock()
        .unwrap()
        .insert(job_id.to_string(), tok.clone());
    tok
}

fn unregister_job(state: &State<'_, AppState>, job_id: &str) {
    state.cancels.lock().unwrap().remove(job_id);
}

/// Downsample the pyramid to exactly `columns` display columns over `[from, to)`.
fn viewport_waveform(pyr: &WaveformPyramid, from: u64, to: u64, columns: usize) -> WaveDto {
    let ch = pyr.channels.max(1);
    let range = to.saturating_sub(from).max(1);
    let target = (range / columns as u64).max(1);

    // Coarsest level whose bucket_frames <= target (levels are ascending).
    let mut level = &pyr.levels[0];
    for lvl in &pyr.levels {
        if lvl.bucket_frames <= target {
            level = lvl;
        } else {
            break;
        }
    }
    let bf = level.bucket_frames.max(1);
    let total_buckets = level.buckets();
    let first_b = (from / bf) as usize;
    let last_b = (to.div_ceil(bf) as usize).min(total_buckets);

    let mut min = vec![0i16; columns * ch];
    let mut max = vec![0i16; columns * ch];
    let span = last_b.saturating_sub(first_b).max(1);
    for col in 0..columns {
        let b0 = first_b + col * span / columns;
        let b1 = (first_b + (col + 1) * span / columns)
            .max(b0 + 1)
            .min(total_buckets);
        for c in 0..ch {
            let mut mn = i16::MAX;
            let mut mx = i16::MIN;
            for b in b0..b1 {
                mn = mn.min(level.min[b * ch + c]);
                mx = mx.max(level.max[b * ch + c]);
            }
            if b0 >= b1 {
                mn = 0;
                mx = 0;
            }
            min[col * ch + c] = mn;
            max[col * ch + c] = mx;
        }
    }
    WaveDto {
        channels: ch,
        columns,
        min,
        max,
    }
}

/// Access a session's source path (unused for now; kept for future ops that
/// read the scratch rather than re-decoding).
#[allow(dead_code)]
fn source_path(state: &State<'_, AppState>, id: &str) -> Option<PathBuf> {
    state
        .sessions
        .lock()
        .unwrap()
        .get(id)
        .map(|s| s.source_path.clone())
}

// ------------------------------------------------- v2 project commands

#[tauri::command]
fn project_new(state: State<'_, AppState>) -> DocSnapshot {
    let mut ed = state.editor.lock().unwrap();
    *ed = Editor::new(Document::new());
    *state.project_dir.lock().unwrap() = None;
    snapshot(&ed)
}

#[tauri::command]
fn project_open(state: State<'_, AppState>, path: String) -> Result<DocSnapshot, String> {
    let doc = open_project(&PathBuf::from(&path)).map_err(|e| e.to_string())?;
    *state.project_dir.lock().unwrap() = Some(parent_dir(&path));
    let mut ed = state.editor.lock().unwrap();
    *ed = Editor::new(doc);
    Ok(snapshot(&ed))
}

#[tauri::command]
fn project_save(state: State<'_, AppState>, path: String) -> Result<(), String> {
    let ed = state.editor.lock().unwrap();
    save_project(ed.document(), &PathBuf::from(&path)).map_err(|e| e.to_string())?;
    *state.project_dir.lock().unwrap() = Some(parent_dir(&path));
    Ok(())
}

#[tauri::command]
fn doc_snapshot(state: State<'_, AppState>) -> DocSnapshot {
    snapshot(&state.editor.lock().unwrap())
}

#[tauri::command]
fn add_source(state: State<'_, AppState>, id: String, path: String) -> Result<DocSnapshot, String> {
    let dir = state
        .project_dir
        .lock()
        .unwrap()
        .clone()
        .unwrap_or_else(|| parent_dir(&path));
    let src = source_ref_from_file(&id, &PathBuf::from(&path), &dir).map_err(|e| e.to_string())?;
    let mut ed = state.editor.lock().unwrap();
    ed.add_source(src);
    Ok(snapshot(&ed))
}

#[tauri::command]
fn add_region(state: State<'_, AppState>, region: Region) -> Result<DocSnapshot, String> {
    apply_cmd(&state, Command::AddRegion(region))
}

#[tauri::command]
fn remove_region(state: State<'_, AppState>, id: String) -> Result<DocSnapshot, String> {
    apply_cmd(&state, Command::RemoveRegion(id))
}

#[tauri::command]
fn set_region_range(
    state: State<'_, AppState>,
    id: String,
    start: u64,
    end: u64,
) -> Result<DocSnapshot, String> {
    apply_cmd(&state, Command::SetRegionRange { id, start, end })
}

#[tauri::command]
fn rename_region(
    state: State<'_, AppState>,
    id: String,
    title: String,
) -> Result<DocSnapshot, String> {
    apply_cmd(&state, Command::RenameRegion { id, title })
}

#[tauri::command]
fn set_region_ops(
    state: State<'_, AppState>,
    id: String,
    ops: OpChain,
) -> Result<DocSnapshot, String> {
    apply_cmd(&state, Command::SetRegionOps { id, ops })
}

#[tauri::command]
fn reorder_regions(state: State<'_, AppState>, order: Vec<String>) -> Result<DocSnapshot, String> {
    apply_cmd(&state, Command::ReorderRegions(order))
}

#[tauri::command]
fn add_marker(state: State<'_, AppState>, marker: Marker) -> Result<DocSnapshot, String> {
    apply_cmd(&state, Command::AddMarker(marker))
}

#[tauri::command]
fn remove_marker(state: State<'_, AppState>, id: String) -> Result<DocSnapshot, String> {
    apply_cmd(&state, Command::RemoveMarker(id))
}

#[tauri::command]
fn move_marker(state: State<'_, AppState>, id: String, frame: u64) -> Result<DocSnapshot, String> {
    apply_cmd(&state, Command::MoveMarker { id, frame })
}

#[tauri::command]
fn undo(state: State<'_, AppState>) -> Result<DocSnapshot, String> {
    let mut ed = state.editor.lock().unwrap();
    ed.undo().map_err(|e| e.to_string())?;
    Ok(snapshot(&ed))
}

#[tauri::command]
fn redo(state: State<'_, AppState>) -> Result<DocSnapshot, String> {
    let mut ed = state.editor.lock().unwrap();
    ed.redo().map_err(|e| e.to_string())?;
    Ok(snapshot(&ed))
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
fn render_project(
    app: AppHandle,
    state: State<'_, AppState>,
    kind: String,
    out_dir: String,
    format: String,
    overwrite: bool,
    job_id: String,
) -> Result<Vec<ProcessReport>, String> {
    let export_kind = if kind.eq_ignore_ascii_case("master") {
        ExportKind::Master
    } else {
        ExportKind::Derivative
    };
    let out = PathBuf::from(&out_dir);
    // Sources resolve relative to the project dir (absolute fallback covers the
    // unsaved case, since source refs also carry the absolute path).
    let project_dir = state
        .project_dir
        .lock()
        .unwrap()
        .clone()
        .unwrap_or_else(|| out.clone());
    let doc = state.editor.lock().unwrap().document().clone();

    let cancel = register_job(&state, &job_id);
    let progress = EventProgress {
        app,
        job_id: job_id.clone(),
    };
    let r = render_document(
        &doc,
        &project_dir,
        export_kind,
        &out,
        parse_format(&format),
        &state.config,
        overwrite,
        &progress,
        &cancel,
    )
    .map_err(|e| e.to_string());
    unregister_job(&state, &job_id);
    r
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::with_flac(resolve_flac()))
        .invoke_handler(tauri::generate_handler![
            open_audio,
            close_audio,
            analyze_audio,
            waveform,
            run_trim,
            run_gain,
            run_export16,
            run_dc_correct,
            run_verify,
            cancel_job,
            project_new,
            project_open,
            project_save,
            doc_snapshot,
            add_source,
            add_region,
            remove_region,
            set_region_range,
            rename_region,
            set_region_ops,
            reorder_regions,
            add_marker,
            remove_marker,
            move_marker,
            undo,
            redo,
            render_project,
        ])
        .run(tauri::generate_context!())
        .expect("error while running intwav GUI");
}
