import { useEffect, useRef, useState, type MouseEvent } from "react";
import type { WaveDto } from "../api";

// A region drawn on the waveform, positioned in [0,1] fractions of the source.
export interface WaveRegion {
  id: string;
  from: number;
  to: number;
  title: string;
}

interface Props {
  wave: WaveDto | null;
  regions: WaveRegion[];
  activeId: string | null;
  // Playhead position as a [0,1] fraction, or null.
  playhead: number | null;
  // Click on empty space or a region body → seek to a [0,1] fraction.
  onSeek?: (fraction: number) => void;
  // Click on a region body → make it active.
  onSelectRegion?: (id: string) => void;
  // Drag a region edge → commit a new boundary ([0,1] fraction) on release.
  onMoveBoundary?: (id: string, edge: "start" | "end", fraction: number) => void;
}

const clamp01 = (v: number) => Math.max(0, Math.min(1, v));
// Edge grab distance, in px.
const EDGE_PX = 7;

// Region band colors (cycled by index); the active band is drawn brighter.
const BAND = ["#4ea1ff", "#3fb950", "#d9a441", "#c77dff", "#f0553f", "#38c6c6"];

type Drag =
  | { kind: "edge"; id: string; edge: "start" | "end"; frac: number }
  | { kind: "press"; frac: number };

// Canvas waveform: one lane per channel, each column a min..max vertical bar,
// with track regions overlaid as draggable bands. Pixel mapping is float —
// allowed, this is display only.
export function Waveform({
  wave,
  regions,
  activeId,
  playhead,
  onSeek,
  onSelectRegion,
  onMoveBoundary,
}: Props) {
  const ref = useRef<HTMLCanvasElement | null>(null);
  const [drag, setDrag] = useState<Drag | null>(null);
  const [hoverEdge, setHoverEdge] = useState(false);

  useEffect(() => {
    const canvas = ref.current;
    if (!canvas) return;
    const dpr = window.devicePixelRatio || 1;
    const cssW = canvas.clientWidth;
    const cssH = canvas.clientHeight;
    canvas.width = Math.max(1, Math.floor(cssW * dpr));
    canvas.height = Math.max(1, Math.floor(cssH * dpr));
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    ctx.scale(dpr, dpr);
    ctx.clearRect(0, 0, cssW, cssH);

    ctx.fillStyle = "#0f1218";
    ctx.fillRect(0, 0, cssW, cssH);

    if (!wave || wave.columns === 0) return;
    const ch = Math.max(1, wave.channels);
    const laneH = cssH / ch;
    const colW = cssW / wave.columns;
    const scale = 32768;

    // Region bands (behind the waveform lines).
    regions.forEach((r, i) => {
      const live =
        drag?.kind === "edge" && drag.id === r.id
          ? drag.edge === "start"
            ? { from: Math.min(drag.frac, r.to), to: r.to }
            : { from: r.from, to: Math.max(drag.frac, r.from) }
          : r;
      const x0 = clamp01(live.from) * cssW;
      const x1 = clamp01(live.to) * cssW;
      const color = BAND[i % BAND.length];
      const on = r.id === activeId;
      ctx.fillStyle = hexA(color, on ? 0.16 : 0.07);
      ctx.fillRect(x0, 0, x1 - x0, cssH);
      ctx.strokeStyle = hexA(color, on ? 0.9 : 0.4);
      ctx.lineWidth = on ? 2 : 1;
      ctx.strokeRect(x0 + 0.5, 0.5, x1 - x0 - 1, cssH - 1);
      ctx.lineWidth = 1;
      // Label.
      ctx.fillStyle = hexA(color, on ? 1 : 0.7);
      ctx.font = "11px ui-monospace, monospace";
      ctx.fillText(r.title, x0 + 6, 14, Math.max(10, x1 - x0 - 10));
    });

    // Waveform lanes.
    for (let c = 0; c < ch; c++) {
      const mid = laneH * c + laneH / 2;
      ctx.strokeStyle = "#232a34";
      ctx.beginPath();
      ctx.moveTo(0, mid);
      ctx.lineTo(cssW, mid);
      ctx.stroke();

      ctx.strokeStyle = "#7f93ac";
      ctx.beginPath();
      for (let col = 0; col < wave.columns; col++) {
        const mn = wave.min[col * ch + c] / scale;
        const mx = wave.max[col * ch + c] / scale;
        const x = col * colW + colW / 2;
        const yTop = mid - mx * (laneH / 2) * 0.95;
        const yBot = mid - mn * (laneH / 2) * 0.95;
        ctx.moveTo(x, yTop);
        ctx.lineTo(x, yBot);
      }
      ctx.stroke();
    }

    // Region edge handles (drawn last so they sit on top).
    regions.forEach((r, i) => {
      const color = BAND[i % BAND.length];
      const on = r.id === activeId;
      for (const [edge, f] of [
        ["start", r.from],
        ["end", r.to],
      ] as const) {
        const live =
          drag?.kind === "edge" && drag.id === r.id && drag.edge === edge ? drag.frac : f;
        const x = clamp01(live) * cssW;
        ctx.strokeStyle = hexA(color, on ? 1 : 0.55);
        ctx.lineWidth = 2;
        ctx.beginPath();
        ctx.moveTo(x, 0);
        ctx.lineTo(x, cssH);
        ctx.stroke();
        ctx.lineWidth = 1;
      }
    });

    // Playhead.
    if (playhead !== null) {
      const x = clamp01(playhead) * cssW;
      ctx.strokeStyle = "#e6edf3";
      ctx.lineWidth = 2;
      ctx.beginPath();
      ctx.moveTo(x, 0);
      ctx.lineTo(x, cssH);
      ctx.stroke();
      ctx.lineWidth = 1;
    }
  }, [wave, regions, activeId, playhead, drag, hoverEdge]);

  function fractionOf(e: MouseEvent<HTMLCanvasElement>): { f: number; w: number } {
    const r = e.currentTarget.getBoundingClientRect();
    return { f: clamp01((e.clientX - r.left) / r.width), w: r.width };
  }

  // Find a region edge within EDGE_PX of the pointer.
  function edgeAt(f: number, w: number): { id: string; edge: "start" | "end" } | null {
    const tol = EDGE_PX / w;
    for (const r of regions) {
      if (Math.abs(f - r.from) <= tol) return { id: r.id, edge: "start" };
      if (Math.abs(f - r.to) <= tol) return { id: r.id, edge: "end" };
    }
    return null;
  }

  function regionAt(f: number): string | null {
    for (const r of regions) if (f >= r.from && f <= r.to) return r.id;
    return null;
  }

  function onDown(e: MouseEvent<HTMLCanvasElement>) {
    const { f, w } = fractionOf(e);
    const hit = edgeAt(f, w);
    if (hit && onMoveBoundary) {
      setDrag({ kind: "edge", id: hit.id, edge: hit.edge, frac: f });
    } else {
      setDrag({ kind: "press", frac: f });
    }
  }
  function onMove(e: MouseEvent<HTMLCanvasElement>) {
    const { f, w } = fractionOf(e);
    if (drag?.kind === "edge") {
      setDrag({ ...drag, frac: f });
      return;
    }
    if (!drag) setHoverEdge(edgeAt(f, w) !== null);
  }
  function onUp(e: MouseEvent<HTMLCanvasElement>) {
    const { f } = fractionOf(e);
    if (drag?.kind === "edge") {
      onMoveBoundary?.(drag.id, drag.edge, f);
    } else if (drag?.kind === "press") {
      const rid = regionAt(f);
      if (rid) onSelectRegion?.(rid);
      onSeek?.(f);
    }
    setDrag(null);
  }

  return (
    <canvas
      ref={ref}
      className="waveform"
      style={{ cursor: drag?.kind === "edge" || hoverEdge ? "ew-resize" : "pointer" }}
      onMouseDown={onDown}
      onMouseMove={onMove}
      onMouseUp={onUp}
      onMouseLeave={() => {
        setDrag(null);
        setHoverEdge(false);
      }}
    />
  );
}

// #rrggbb + alpha → rgba() string.
function hexA(hex: string, a: number): string {
  const n = parseInt(hex.slice(1), 16);
  const r = (n >> 16) & 255;
  const g = (n >> 8) & 255;
  const b = n & 255;
  return `rgba(${r},${g},${b},${a})`;
}
