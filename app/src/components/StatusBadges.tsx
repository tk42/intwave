import type { ProcessReport } from "../api";

type T = (k: string) => string;

// Renders the Integer-Safe status (spec §8.2) from a frozen ProcessReport. The
// badges are a localized *view* of machine facts; the report fields never change.
export function StatusBadges({ report, t }: { report: ProcessReport | null; t: T }) {
  const yes = t("status.yes");
  const no = t("status.no");

  // Good = green; attention (sample-modifying / requantized) = amber.
  const rows: { label: string; value: string; tone: "good" | "warn" | "bad" }[] = [
    { label: t("status.integerSafe"), value: t("status.on"), tone: "good" },
    {
      label: t("status.fpSavePath"),
      value: report?.floating_point_used_in_save_path ? yes : no,
      tone: report?.floating_point_used_in_save_path ? "bad" : "good",
    },
    {
      label: t("status.sampleModified"),
      value: report?.sample_values_modified ? yes : no,
      tone: report?.sample_values_modified ? "warn" : "good",
    },
    {
      label: t("status.requantized"),
      value: report?.requantized ? yes : no,
      tone: report?.requantized ? "warn" : "good",
    },
    {
      label: t("status.dither"),
      value: report?.dither_used ? yes : no,
      tone: report?.dither_used ? "warn" : "good",
    },
    {
      label: t("status.resampled"),
      value: report?.resampled ? yes : no,
      tone: report?.resampled ? "bad" : "good",
    },
    {
      label: t("status.pcmVerified"),
      value: report ? (report.pcm_verified ? yes : no) : "—",
      tone: report ? (report.pcm_verified ? "good" : "bad") : "good",
    },
  ];

  return (
    <div className="badges">
      {rows.map((r) => (
        <div key={r.label} className={`badge ${r.tone}`}>
          <span className="badge-label">{r.label}</span>
          <span className="badge-value">{r.value}</span>
        </div>
      ))}
    </div>
  );
}
