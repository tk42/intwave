import { useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";

import {
  analyzeAudio,
  cancelJob,
  openAudio,
  runExport16,
  runGain,
  runTrim,
  runVerify,
  waveform,
  type AnalyzeDto,
  type OpenDto,
  type OutFormat,
  type ProcessReport,
  type WaveDto,
} from "./api";
import { makeT, type Locale } from "./i18n";
import { StatusBadges } from "./components/StatusBadges";
import { Waveform } from "./components/Waveform";

const WAVE_COLUMNS = 1200;

export default function App() {
  const [locale, setLocale] = useState<Locale>("ja");
  const t = useMemo(() => makeT(locale), [locale]);

  const [session, setSession] = useState<OpenDto | null>(null);
  const [analysis, setAnalysis] = useState<AnalyzeDto | null>(null);
  const [wave, setWave] = useState<WaveDto | null>(null);
  const [report, setReport] = useState<ProcessReport | null>(null);
  const [busy, setBusy] = useState(false);
  const [progress, setProgress] = useState(0);
  const [error, setError] = useState<string | null>(null);

  const [fromSec, setFromSec] = useState(0);
  const [toSec, setToSec] = useState(0);
  const [db, setDb] = useState(-3);
  const [allowClipping, setAllowClipping] = useState(false);
  const [format, setFormat] = useState<OutFormat>("flac");
  const [overwrite, setOverwrite] = useState(false);

  const jobRef = useRef<string | null>(null);

  useEffect(() => {
    const un = listen<{ jobId: string; permille: number }>("progress", (e) => {
      if (e.payload.jobId === jobRef.current) setProgress(e.payload.permille);
    });
    return () => {
      un.then((f) => f());
    };
  }, []);

  const rate = session?.sampleRate ?? 1;
  const totalFrames = session?.frames ?? 0;
  const toFrame = (sec: number) => Math.max(0, Math.min(totalFrames, Math.round(sec * rate)));

  const selection = useMemo(() => {
    if (!session || totalFrames === 0) return null;
    return { from: toFrame(fromSec) / totalFrames, to: toFrame(toSec) / totalFrames };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [fromSec, toSec, session, totalFrames]);

  async function withBusy<R>(fn: () => Promise<R>): Promise<R | undefined> {
    setBusy(true);
    setError(null);
    setProgress(0);
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

  async function doOpen() {
    const path = await openDialog({
      multiple: false,
      filters: [{ name: "Audio", extensions: ["wav", "flac"] }],
    });
    if (typeof path !== "string") return;
    await withBusy(async () => {
      const s = await openAudio(path);
      setSession(s);
      setAnalysis(await analyzeAudio(path));
      setReport(null);
      setFromSec(0);
      setToSec(s.durationMs / 1000);
      setWave(await waveform(s.id, 0, s.frames, WAVE_COLUMNS));
    });
  }

  function newJob() {
    const id = `job-${Date.now()}`;
    jobRef.current = id;
    return id;
  }

  async function pickOutput(): Promise<string | null> {
    const ext = format;
    const out = await saveDialog({ filters: [{ name: ext.toUpperCase(), extensions: [ext] }] });
    return typeof out === "string" ? out : null;
  }

  async function doTrim() {
    if (!session) return;
    const out = await pickOutput();
    if (!out) return;
    await withBusy(async () => {
      const r = await runTrim(
        session.sourcePath,
        out,
        toFrame(fromSec),
        toFrame(toSec),
        format,
        overwrite,
        newJob(),
      );
      setReport(r);
    });
  }

  async function doExport16() {
    if (!session) return;
    const out = await pickOutput();
    if (!out) return;
    await withBusy(async () => {
      setReport(await runExport16(session.sourcePath, out, 1, format, overwrite, newJob()));
    });
  }

  async function doGain() {
    if (!session) return;
    const out = await pickOutput();
    if (!out) return;
    await withBusy(async () => {
      setReport(
        await runGain(session.sourcePath, out, db, allowClipping, format, overwrite, newJob()),
      );
    });
  }

  async function doVerify() {
    if (!session) return;
    const other = await openDialog({
      multiple: false,
      filters: [{ name: "Audio", extensions: ["wav", "flac"] }],
    });
    if (typeof other !== "string") return;
    await withBusy(async () => setReport(await runVerify(session.sourcePath, other)));
  }

  function doCancel() {
    if (jobRef.current) cancelJob(jobRef.current);
  }

  return (
    <div className="app">
      <header className="topbar">
        <div className="brand">{t("app.title")}</div>
        <div className="spacer" />
        <button onClick={doOpen} disabled={busy}>
          {t("action.open")}
        </button>
        <select value={locale} onChange={(e) => setLocale(e.target.value as Locale)}>
          <option value="ja">日本語</option>
          <option value="en">English</option>
        </select>
      </header>

      <main className="body">
        <section className="center">
          <Waveform wave={wave} selection={selection} />
          {!session && <div className="empty">{t("msg.dropHint")}</div>}

          <div className="controls">
            <label>
              {t("label.from")}
              <input
                type="number"
                step="0.001"
                value={fromSec}
                onChange={(e) => setFromSec(parseFloat(e.target.value) || 0)}
              />
            </label>
            <label>
              {t("label.to")}
              <input
                type="number"
                step="0.001"
                value={toSec}
                onChange={(e) => setToSec(parseFloat(e.target.value) || 0)}
              />
            </label>
            <label>
              {t("label.db")}
              <input
                type="number"
                step="1"
                value={db}
                onChange={(e) => setDb(parseInt(e.target.value) || 0)}
              />
            </label>
            <label className="check">
              <input
                type="checkbox"
                checked={allowClipping}
                onChange={(e) => setAllowClipping(e.target.checked)}
              />
              {t("label.allowClipping")}
            </label>
            <label>
              {t("label.outputFormat")}
              <select value={format} onChange={(e) => setFormat(e.target.value as OutFormat)}>
                <option value="flac">FLAC</option>
                <option value="wav">WAV</option>
              </select>
            </label>
            <label className="check">
              <input
                type="checkbox"
                checked={overwrite}
                onChange={(e) => setOverwrite(e.target.checked)}
              />
              {t("label.overwrite")}
            </label>
          </div>

          <div className="actions">
            <button onClick={doTrim} disabled={busy || !session}>
              {t("action.trim")}
            </button>
            <button onClick={doGain} disabled={busy || !session}>
              {t("action.gain")}
            </button>
            <button onClick={doExport16} disabled={busy || !session}>
              {t("action.export16")}
            </button>
            <button onClick={doVerify} disabled={busy || !session}>
              {t("action.verify")}
            </button>
            {busy && (
              <button className="cancel" onClick={doCancel}>
                {t("action.cancel")}
              </button>
            )}
          </div>

          {busy && (
            <div className="progress">
              <div className="bar" style={{ width: `${progress / 10}%` }} />
              <span>
                {t("msg.processing")} {Math.round(progress / 10)}%
              </span>
            </div>
          )}
          {error && <div className="error">error: {error}</div>}
        </section>

        <aside className="inspector">
          <StatusBadges report={report} t={t} />

          {analysis ? (
            <dl className="info">
              <dt>{t("label.format")}</dt>
              <dd>{analysis.format}</dd>
              <dt>{t("label.bitDepth")}</dt>
              <dd>{analysis.bitDepth}-bit</dd>
              <dt>{t("label.sampleRate")}</dt>
              <dd>{analysis.sampleRate} Hz</dd>
              <dt>{t("label.channels")}</dt>
              <dd>{analysis.channels}</dd>
              <dt>{t("label.duration")}</dt>
              <dd>{((session?.durationMs ?? 0) / 1000).toFixed(3)} s</dd>
              <dt>{t("label.peak")}</dt>
              <dd>{analysis.peakDbfs.join(" / ")} dBFS</dd>
              <dt>{t("label.clipped")}</dt>
              <dd>{analysis.totalClipped}</dd>
            </dl>
          ) : (
            <div className="empty small">{t("msg.noFile")}</div>
          )}

          {report && (
            <div className="report">
              <h3>{t("report.title")}</h3>
              <pre>{JSON.stringify(report, null, 2)}</pre>
            </div>
          )}
        </aside>
      </main>
    </div>
  );
}
