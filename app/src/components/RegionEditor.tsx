import { useState } from "react";
import type { OpChain, Region, SourceRef } from "../api";

type T = (k: string) => string;

interface Props {
  region: Region;
  source: SourceRef | undefined;
  busy: boolean;
  onRename: (title: string) => void;
  onSetRange: (startFrame: number, endFrame: number) => void;
  onSetOps: (ops: OpChain) => void;
  onRemove: () => void;
  t: T;
}

// Editor for the one selected region. Numeric fields buffer locally and commit
// on blur so typing doesn't hit the engine per keystroke. Mount with a `key` of
// the region id so switching regions resets the buffer.
export function RegionEditor({
  region: r,
  source,
  busy,
  onRename,
  onSetRange,
  onSetOps,
  onRemove,
  t,
}: Props) {
  const rate = source?.sample_rate ?? 1;
  const [d, setD] = useState<Record<string, string>>({});
  const val = (k: string, fallback: string) => d[k] ?? fallback;
  const set = (k: string, v: string) => setD((p) => ({ ...p, [k]: v }));
  const clear = (k: string) => setD((p) => { const n = { ...p }; delete n[k]; return n; });

  function commitRange() {
    const from = Math.round((parseFloat(val("from", String(r.start_frame / rate))) || 0) * rate);
    const to = Math.round((parseFloat(val("to", String(r.end_frame / rate))) || 0) * rate);
    clear("from");
    clear("to");
    onSetRange(Math.max(0, from), Math.max(from + 1, to));
  }

  return (
    <div className="region-editor">
      <div className="re-head">
        <span className="re-label">{t("re.selected")}</span>
        <input
          className="re-title"
          key={`title-${r.title}`}
          defaultValue={r.title}
          disabled={busy}
          onBlur={(e) => onRename(e.target.value)}
        />
        <span className="spacer" />
        <button className="mini danger" onClick={onRemove} disabled={busy}>
          {t("re.remove")}
        </button>
      </div>

      <div className="re-fields">
        <label className="mini">
          {t("label.from")}
          <input
            type="number"
            step="0.001"
            value={val("from", (r.start_frame / rate).toFixed(3))}
            onChange={(e) => set("from", e.target.value)}
            onBlur={commitRange}
            disabled={busy}
          />
        </label>
        <label className="mini">
          {t("label.to")}
          <input
            type="number"
            step="0.001"
            value={val("to", (r.end_frame / rate).toFixed(3))}
            onChange={(e) => set("to", e.target.value)}
            onBlur={commitRange}
            disabled={busy}
          />
        </label>
        <label className="mini">
          {t("label.gainDb")}
          <input
            type="number"
            step="1"
            value={val("gain", r.ops.gain_db?.toString() ?? "")}
            onChange={(e) => set("gain", e.target.value)}
            onBlur={() => {
              const raw = val("gain", "");
              clear("gain");
              onSetOps({ ...r.ops, gain_db: raw === "" ? undefined : parseInt(raw) || 0 });
            }}
            disabled={busy}
          />
        </label>
        <label className="mini">
          {t("label.fadeInS")}
          <input
            type="number"
            step="0.1"
            value={val("fin", (r.ops.fade_in_frames / rate).toFixed(2))}
            onChange={(e) => set("fin", e.target.value)}
            onBlur={() => {
              const s = parseFloat(val("fin", "0")) || 0;
              clear("fin");
              onSetOps({ ...r.ops, fade_in_frames: Math.round(s * rate) });
            }}
            disabled={busy}
          />
        </label>
        <label className="mini">
          {t("label.fadeOutS")}
          <input
            type="number"
            step="0.1"
            value={val("fout", (r.ops.fade_out_frames / rate).toFixed(2))}
            onChange={(e) => set("fout", e.target.value)}
            onBlur={() => {
              const s = parseFloat(val("fout", "0")) || 0;
              clear("fout");
              onSetOps({ ...r.ops, fade_out_frames: Math.round(s * rate) });
            }}
            disabled={busy}
          />
        </label>
        <label className="mini check">
          <input
            type="checkbox"
            checked={r.ops.dc_correct}
            onChange={(e) => onSetOps({ ...r.ops, dc_correct: e.target.checked })}
            disabled={busy}
          />
          {t("label.dcCorrectFlag")}
        </label>
        <label className="mini check">
          <input
            type="checkbox"
            checked={r.ops.export16_seed !== undefined}
            onChange={(e) =>
              onSetOps({ ...r.ops, export16_seed: e.target.checked ? 1 : undefined })
            }
            disabled={busy}
          />
          {t("label.export16flag")}
        </label>
      </div>
    </div>
  );
}
