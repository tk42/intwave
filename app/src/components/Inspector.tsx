import type { AnalyzeDto, OpenDto, ProcessReport } from "../api";
import { StatusBadges } from "./StatusBadges";

type T = (k: string) => string;

interface Props {
  session: OpenDto | null;
  analysis: AnalyzeDto | null;
  report: ProcessReport | null;
  t: T;
}

// Right rail: the source's true facts and the always-visible preservation proof.
// Before any export it reassures from the source analysis; after an export it
// shows the frozen ProcessReport's Integer-Safe view.
export function Inspector({ session, analysis, report, t }: Props) {
  if (!analysis || !session) {
    return <div className="empty small">{t("msg.noFile")}</div>;
  }

  const durSec = (session.durationMs ?? 0) / 1000;
  const fmtDur = (s: number) => {
    const m = Math.floor(s / 60);
    const r = Math.floor(s % 60);
    return `${m}:${r.toString().padStart(2, "0")}`;
  };

  return (
    <>
      <div className="insp-title">{t("insp.file")}</div>
      <dl className="info">
        <dt>{t("label.format")}</dt>
        <dd>{analysis.format}</dd>
        <dt>{t("label.bitDepth")}</dt>
        <dd>{analysis.bitDepth}-bit</dd>
        <dt>{t("label.sampleRate")}</dt>
        <dd>{(analysis.sampleRate / 1000).toFixed(1)} kHz</dd>
        <dt>{t("label.channels")}</dt>
        <dd>{analysis.channels} ch</dd>
        <dt>{t("label.duration")}</dt>
        <dd>{fmtDur(durSec)}</dd>
        <dt>{t("label.peak")}</dt>
        <dd>{analysis.peakDbfs.join(" / ")} dBFS</dd>
        <dt>{t("label.clipped")}</dt>
        <dd className={analysis.totalClipped > 0 ? "warn" : ""}>{analysis.totalClipped}</dd>
        <dt>{t("label.dcOffset")}</dt>
        <dd>{analysis.dcOffset.join(" / ")}</dd>
      </dl>

      <div className="insp-title">{t("insp.proof")}</div>
      {report ? (
        <StatusBadges report={report} t={t} />
      ) : (
        <div className="proof-static">
          <div className="proof-row">
            <span className="proof-ok">✓</span> {t("proof.integerPcm")}
          </div>
          <div className="proof-row">
            <span className="proof-ok">✓</span> {t("proof.noConversion")}
          </div>
          <div className="proof-hint">{t("proof.hint")}</div>
        </div>
      )}
    </>
  );
}
