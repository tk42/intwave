import { useEffect, useRef } from "react";
import type { WaveDto } from "../api";

interface Props {
  wave: WaveDto | null;
  // Selection as a [0,1] fraction of the visible range, or null.
  selection: { from: number; to: number } | null;
}

// Canvas waveform: one lane per channel, each column a min..max vertical bar.
// The pixel mapping here is float — allowed, this is display only.
export function Waveform({ wave, selection }: Props) {
  const ref = useRef<HTMLCanvasElement | null>(null);

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

    // Background.
    ctx.fillStyle = "#12151b";
    ctx.fillRect(0, 0, cssW, cssH);

    if (!wave || wave.columns === 0) return;
    const ch = Math.max(1, wave.channels);
    const laneH = cssH / ch;
    const colW = cssW / wave.columns;
    const scale = 32768;

    for (let c = 0; c < ch; c++) {
      const mid = laneH * c + laneH / 2;
      // Center line.
      ctx.strokeStyle = "#232a34";
      ctx.beginPath();
      ctx.moveTo(0, mid);
      ctx.lineTo(cssW, mid);
      ctx.stroke();

      ctx.strokeStyle = "#4ea1ff";
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

    // Selection overlay.
    if (selection) {
      const x0 = selection.from * cssW;
      const x1 = selection.to * cssW;
      ctx.fillStyle = "rgba(78,161,255,0.18)";
      ctx.fillRect(Math.min(x0, x1), 0, Math.abs(x1 - x0), cssH);
      ctx.strokeStyle = "#4ea1ff";
      ctx.strokeRect(Math.min(x0, x1), 0, Math.abs(x1 - x0), cssH);
    }
  }, [wave, selection]);

  return <canvas ref={ref} className="waveform" />;
}
