import { useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";

import {
  addRegion,
  addSource,
  analyzeAudio,
  cancelJob,
  openAudio,
  playbackLoad,
  playbackPause,
  playbackPlay,
  playbackSeek,
  playbackStatus,
  playbackStop,
  projectNew,
  projectOpen,
  projectSave,
  redo,
  removeRegion,
  renameRegion,
  renderProject,
  runVerify,
  setRegionOps,
  setRegionRange,
  undo,
  waveform,
  type AnalyzeDto,
  type DocSnapshot,
  type ExportKind,
  type OpChain,
  type OpenDto,
  type OutFormat,
  type ProcessReport,
  type Region,
  type WaveDto,
} from "./api";
import { makeT, type Locale } from "./i18n";
import { Inspector } from "./components/Inspector";
import { RegionEditor } from "./components/RegionEditor";
import { TrackList } from "./components/TrackList";
import { Waveform, type WaveRegion } from "./components/Waveform";

const WAVE_COLUMNS = 1200;
const baseName = (p: string) => p.split(/[\\/]/).pop() ?? p;
const fmtClock = (s: number) => {
  const m = Math.floor(s / 60);
  const r = Math.floor(s % 60);
  return `${m}:${r.toString().padStart(2, "0")}`;
};

export default function App() {
  const [locale, setLocale] = useState<Locale>("ja");
  const t = useMemo(() => makeT(locale), [locale]);

  const [session, setSession] = useState<OpenDto | null>(null);
  const [analysis, setAnalysis] = useState<AnalyzeDto | null>(null);
  const [wave, setWave] = useState<WaveDto | null>(null);
  const [snap, setSnap] = useState<DocSnapshot | null>(null);
  const [currentSourceId, setCurrentSourceId] = useState<string | null>(null);
  const [activeId, setActiveId] = useState<string | null>(null);

  const [report, setReport] = useState<ProcessReport | null>(null);
  const [busy, setBusy] = useState(false);
  const [progress, setProgress] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const [playing, setPlaying] = useState(false);
  const [playhead, setPlayhead] = useState(0);

  const [splitMode, setSplitMode] = useState<"silence" | "ab">("silence");
  const [format, setFormat] = useState<OutFormat>("flac");
  const [overwrite, setOverwrite] = useState(false);

  const jobRef = useRef<string | null>(null);

  const doc = snap?.document ?? null;
  const regions: Region[] = useMemo(
    () => (doc?.regions ?? []).filter((r) => r.source_id === currentSourceId),
    [doc, currentSourceId],
  );
  const activeRegion = regions.find((r) => r.id === activeId) ?? null;
  const source = doc?.sources.find((s) => s.id === currentSourceId);

  const rate = session?.sampleRate ?? 1;
  const totalFrames = session?.frames ?? 0;
  const durSec = totalFrames / rate;

  useEffect(() => {
    const un = listen<{ jobId: string; permille: number }>("progress", (e) => {
      if (e.payload.jobId === jobRef.current) setProgress(e.payload.permille);
    });
    return () => {
      un.then((f) => f());
    };
  }, []);

  // Poll playback state while a file is loaded.
  useEffect(() => {
    if (!session) return;
    const timer = setInterval(async () => {
      try {
        const s = await playbackStatus();
        setPlaying(s.playing);
        setPlayhead(s.position);
        if (s.error) setError(s.error);
      } catch {
        /* ignore */
      }
    }, 100);
    return () => clearInterval(timer);
  }, [session]);

  // Keep the active selection valid after undo/redo/split/remove.
  useEffect(() => {
    if (regions.length === 0) {
      if (activeId !== null) setActiveId(null);
    } else if (!regions.some((r) => r.id === activeId)) {
      setActiveId(regions[0].id);
    }
  }, [regions, activeId]);

  const waveRegions: WaveRegion[] = useMemo(
    () =>
      totalFrames === 0
        ? []
        : regions.map((r) => ({
            id: r.id,
            from: r.start_frame / totalFrames,
            to: r.end_frame / totalFrames,
            title: r.title,
          })),
    [regions, totalFrames],
  );
  const playheadFraction = totalFrames > 0 ? playhead / totalFrames : null;

  async function withBusy<R>(fn: () => Promise<R>): Promise<R | undefined> {
    setBusy(true);
    setError(null);
    try {
      return await fn();
    } catch (e) {
      setError(String(e));
      return undefined;
    } finally {
      setBusy(false);
      jobRef.current = null;
    }
  }

  function newJob() {
    const id = `job-${Date.now()}`;
    jobRef.current = id;
    return id;
  }

  // Load the audio subsystem (session + analysis + waveform + playback) for a path.
  async function loadAudio(path: string): Promise<OpenDto> {
    const s = await openAudio(path);
    setSession(s);
    setAnalysis(await analyzeAudio(path));
    setWave(await waveform(s.id, 0, s.frames, WAVE_COLUMNS));
    setPlayhead(0);
    await playbackLoad(s.id, null, 0, 0);
    return s;
  }

  // "Open" accepts audio (→ fresh 1-track project) or a saved .iwproj.
  async function doOpen() {
    const path = await openDialog({
      multiple: false,
      filters: [
        { name: "Audio / Project", extensions: ["wav", "flac", "iwproj"] },
        { name: "Audio", extensions: ["wav", "flac"] },
        { name: "intwav project", extensions: ["iwproj"] },
      ],
    });
    if (typeof path !== "string") return;
    setReport(null);
    setNotice(null);
    if (path.toLowerCase().endsWith(".iwproj")) {
      await openProjectFile(path);
    } else {
      await openAudioAsProject(path);
    }
  }

  async function openAudioAsProject(path: string) {
    await withBusy(async () => {
      const s = await loadAudio(path);
      await projectNew();
      const srcId = `src-${Date.now()}`;
      await addSource(srcId, path);
      setCurrentSourceId(srcId);
      const regId = `reg-${Date.now()}`;
      const snapshot = await addRegion({
        id: regId,
        source_id: srcId,
        start_frame: 0,
        end_frame: s.frames,
        title: t("track.default"),
        ops: { dc_correct: false, fade_in_frames: 0, fade_out_frames: 0 },
      });
      setSnap(snapshot);
      setActiveId(regId);
    });
  }

  async function openProjectFile(path: string) {
    await withBusy(async () => {
      const snapshot = await projectOpen(path);
      const first = snapshot.document.sources[0];
      if (!first) {
        setSnap(snapshot);
        setError(t("err.noSourceInProject"));
        return;
      }
      await loadAudio(first.last_known_absolute_path);
      setCurrentSourceId(first.id);
      setSnap(snapshot);
      setActiveId(snapshot.document.regions.find((r) => r.source_id === first.id)?.id ?? null);
    });
  }

  // ---- region edits ----

  const selectRegion = (id: string) => {
    setActiveId(id);
    const r = regions.find((x) => x.id === id);
    if (r) {
      playbackSeek(r.start_frame);
      setPlayhead(r.start_frame);
    }
  };

  async function moveBoundary(id: string, edge: "start" | "end", fraction: number) {
    const r = regions.find((x) => x.id === id);
    if (!r) return;
    const frame = Math.round(fraction * totalFrames);
    const start = edge === "start" ? Math.min(frame, r.end_frame - 1) : r.start_frame;
    const end = edge === "end" ? Math.max(frame, r.start_frame + 1) : r.end_frame;
    await withBusy(async () => setSnap(await setRegionRange(id, Math.max(0, start), end)));
  }

  async function addWholeRegion() {
    if (!currentSourceId) return;
    const regId = `reg-${Date.now()}`;
    await withBusy(async () =>
      setSnap(
        await addRegion({
          id: regId,
          source_id: currentSourceId,
          start_frame: 0,
          end_frame: totalFrames,
          title: `${t("track.default")} ${regions.length + 1}`,
          ops: { dc_correct: false, fade_in_frames: 0, fade_out_frames: 0 },
        }),
      ),
    );
    setActiveId(regId);
  }

  // Cut points (frames) strictly inside the source, from the analysis.
  function computeCuts(): number[] {
    if (!analysis || totalFrames === 0) return [];
    if (splitMode === "ab") {
      let best: [number, number] | null = null;
      for (const r of analysis.silentRegions) {
        if (!best || r[1] - r[0] > best[1] - best[0]) best = r;
      }
      const mid = best ? Math.round((best[0] + best[1]) / 2) : Math.round(totalFrames / 2);
      return [mid].filter((f) => f > 0 && f < totalFrames);
    }
    return analysis.silentRegions
      .map(([s, e]) => Math.round((s + e) / 2))
      .filter((f) => f > 0 && f < totalFrames)
      .sort((a, b) => a - b);
  }

  async function doSplit() {
    if (!currentSourceId) return;
    const bounds = [0, ...computeCuts(), totalFrames];
    if (bounds.length <= 2) {
      setNotice(t("notice.noCuts"));
      return;
    }
    await withBusy(async () => {
      let s: DocSnapshot | null = snap;
      for (const r of regions) s = await removeRegion(r.id);
      let firstId: string | null = null;
      for (let i = 0; i + 1 < bounds.length; i++) {
        const id = `reg-${Date.now()}-${i}`;
        if (!firstId) firstId = id;
        s = await addRegion({
          id,
          source_id: currentSourceId,
          start_frame: bounds[i],
          end_frame: bounds[i + 1],
          title: `Track ${String(i + 1).padStart(2, "0")}`,
          track_number: i + 1,
          ops: { dc_correct: false, fade_in_frames: 0, fade_out_frames: 0 },
        });
      }
      setSnap(s);
      setActiveId(firstId);
    });
  }

  // ---- export / project ----

  async function doExport(kind: ExportKind) {
    if (regions.length === 0) return;
    const dir = await openDialog({ directory: true });
    if (typeof dir !== "string") return;
    setReport(null);
    setNotice(null);
    await withBusy(async () => {
      const reports = await renderProject(kind, dir, format, overwrite, newJob());
      setReport(reports[0] ?? null);
      setNotice(t("notice.exported").replace("{n}", String(reports.length)));
    });
  }

  async function doSaveProject() {
    const path = await saveDialog({
      filters: [{ name: "intwav project", extensions: ["iwproj"] }],
    });
    if (typeof path !== "string") return;
    await withBusy(async () => {
      await projectSave(path);
      setNotice(t("notice.saved"));
    });
  }

  async function doVerify() {
    if (!session) return;
    const other = await openDialog({
      multiple: false,
      filters: [{ name: "Audio", extensions: ["wav", "flac"] }],
    });
    if (typeof other !== "string") return;
    setNotice(null);
    await withBusy(async () => setReport(await runVerify(session.sourcePath, other)));
  }

  // ---- transport / undo ----

  function togglePlay() {
    if (playing) playbackPause();
    else playbackPlay();
  }
  function seekFraction(f: number) {
    const frame = Math.round(f * totalFrames);
    setPlayhead(frame);
    playbackSeek(frame);
  }
  const doUndo = () => withBusy(async () => setSnap(await undo()));
  const doRedo = () => withBusy(async () => setSnap(await redo()));

  function doCancel() {
    if (jobRef.current) cancelJob(jobRef.current);
  }

  const hasSession = !!session;
  const disabled = busy || !hasSession;

  return (
    <div className="app">
      <header className="topbar">
        <span className="brand">
          <span className="brand-mark">◈</span> intwav
        </span>
        {session ? (
          <>
            <span className="filename">{baseName(session.sourcePath)}</span>
            <span className="pill safe">🛡 Integer-Safe</span>
          </>
        ) : (
          <span className="filename muted">{t("app.subtitle")}</span>
        )}
        <span className="spacer" />
        <button className="ghost" onClick={doUndo} disabled={busy || !snap?.canUndo}>
          ↶
        </button>
        <button className="ghost" onClick={doRedo} disabled={busy || !snap?.canRedo}>
          ↷
        </button>
        <button className="ghost" onClick={doVerify} disabled={disabled} title={t("action.verify.tip")}>
          {t("action.verify")}
        </button>
        <select value={locale} onChange={(e) => setLocale(e.target.value as Locale)}>
          <option value="ja">日本語</option>
          <option value="en">English</option>
        </select>
      </header>

      <main className="workspace">
        <TrackList
          regions={regions}
          activeId={activeId}
          hasSource={hasSession}
          busy={busy}
          splitMode={splitMode}
          onSelect={selectRegion}
          onSetSplitMode={setSplitMode}
          onAddRegion={addWholeRegion}
          onSplit={doSplit}
          t={t}
        />

        <section className="stage">
          <Waveform
            wave={wave}
            regions={waveRegions}
            activeId={activeId}
            playhead={playheadFraction}
            onSeek={hasSession ? seekFraction : undefined}
            onSelectRegion={hasSession ? selectRegion : undefined}
            onMoveBoundary={hasSession ? moveBoundary : undefined}
          />

          {!session ? (
            <div className="empty">
              {t("msg.dropHint")}
              <div className="empty-cta">
                <button onClick={doOpen} disabled={busy}>
                  {t("action.open")}
                </button>
              </div>
            </div>
          ) : (
            <>
              <div className="transport">
                <button onClick={togglePlay} disabled={disabled}>
                  {playing ? `⏸` : `▶`}
                </button>
                <button onClick={() => playbackStop()} disabled={disabled}>
                  ⏹
                </button>
                <span className="playpos">
                  {fmtClock(playhead / rate)} / {fmtClock(durSec)}
                </span>
                <span className="spacer" />
                <span className="hint">{t("msg.waveHint")}</span>
              </div>

              {activeRegion ? (
                <RegionEditor
                  key={activeRegion.id}
                  region={activeRegion}
                  source={source}
                  busy={busy}
                  onRename={(title) =>
                    withBusy(async () => setSnap(await renameRegion(activeRegion.id, title)))
                  }
                  onSetRange={(s, e) =>
                    withBusy(async () => setSnap(await setRegionRange(activeRegion.id, s, e)))
                  }
                  onSetOps={(ops: OpChain) =>
                    withBusy(async () => setSnap(await setRegionOps(activeRegion.id, ops)))
                  }
                  onRemove={() =>
                    withBusy(async () => setSnap(await removeRegion(activeRegion.id)))
                  }
                  t={t}
                />
              ) : (
                <div className="empty small">{t("rail.noTracks")}</div>
              )}

              {busy && (
                <div className="progress">
                  <div className="bar" style={{ width: `${progress / 10}%` }} />
                  <span>
                    {t("msg.processing")} {Math.round(progress / 10)}%
                  </span>
                  <button className="cancel-inline" onClick={doCancel}>
                    {t("action.cancel")}
                  </button>
                </div>
              )}
              {notice && <div className="notice">{notice}</div>}
              {error && <div className="error">error: {error}</div>}
            </>
          )}
        </section>

        <aside className="inspector">
          <Inspector session={session} analysis={analysis} report={report} t={t} />
        </aside>
      </main>

      <footer className="dock">
        <button onClick={doOpen} disabled={busy} title={t("action.open.tip")}>
          {t("action.open")}
        </button>
        <button onClick={doSaveProject} disabled={disabled || regions.length === 0}>
          {t("action.saveProject")}
        </button>
        <span className="dock-sep" />
        <label className="dock-opt">
          {t("label.outputFormat")}
          <select value={format} onChange={(e) => setFormat(e.target.value as OutFormat)}>
            <option value="flac">FLAC</option>
            <option value="wav">WAV</option>
          </select>
        </label>
        <label className="dock-opt check">
          <input type="checkbox" checked={overwrite} onChange={(e) => setOverwrite(e.target.checked)} />
          {t("label.overwrite")}
        </label>
        <span className="spacer" />
        <button
          className="primary"
          onClick={() => doExport("master")}
          disabled={disabled || regions.length === 0}
          title={t("proj.master.tip")}
        >
          {t("proj.master")}
        </button>
        <button
          onClick={() => doExport("derivative")}
          disabled={disabled || regions.length === 0}
          title={t("proj.derivative.tip")}
        >
          {t("proj.derivative")}
        </button>
      </footer>
    </div>
  );
}
