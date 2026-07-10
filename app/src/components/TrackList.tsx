import type { Region } from "../api";

type T = (k: string) => string;

interface Props {
  regions: Region[];
  activeId: string | null;
  hasSource: boolean;
  busy: boolean;
  splitMode: "silence" | "ab";
  onSelect: (id: string) => void;
  onSetSplitMode: (m: "silence" | "ab") => void;
  onAddRegion: () => void;
  onSplit: () => void;
  t: T;
}

// Left rail: the tracks (regions). Splitting a long transfer just creates more
// regions here — export then writes one file per track.
export function TrackList({
  regions,
  activeId,
  hasSource,
  busy,
  splitMode,
  onSelect,
  onSetSplitMode,
  onAddRegion,
  onSplit,
  t,
}: Props) {
  return (
    <div className="rail">
      <div className="rail-title">
        {t("rail.tracks")} <span className="count">{regions.length}</span>
      </div>

      {regions.length === 0 ? (
        <div className="empty small">{hasSource ? t("rail.noTracks") : t("msg.noFile")}</div>
      ) : (
        <ul className="tracklist">
          {regions.map((r, i) => (
            <li
              key={r.id}
              className={r.id === activeId ? "track active" : "track"}
              onClick={() => onSelect(r.id)}
            >
              <span className="track-no">{String(i + 1).padStart(2, "0")}</span>
              <span className="track-title">{r.title}</span>
            </li>
          ))}
        </ul>
      )}

      <div className="rail-actions">
        <button className="mini" onClick={onAddRegion} disabled={busy || !hasSource}>
          + {t("rail.add")}
        </button>
        <div className="split-row">
          <select
            value={splitMode}
            onChange={(e) => onSetSplitMode(e.target.value as "silence" | "ab")}
            disabled={busy || !hasSource}
          >
            <option value="silence">{t("opt.silence")}</option>
            <option value="ab">{t("opt.ab")}</option>
          </select>
          <button className="mini" onClick={onSplit} disabled={busy || !hasSource}>
            {t("rail.split")}
          </button>
        </div>
      </div>
    </div>
  );
}
