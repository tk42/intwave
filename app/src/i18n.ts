// Minimal bilingual i18n, Japanese-default (Q17). The machine layer (reports,
// error codes) never localizes; only these UI strings do.

export type Locale = "ja" | "en";

type Dict = Record<string, string>;

const ja: Dict = {
  "app.subtitle": "整数PCM保護アーカイブエディタ",

  "action.open": "開く",
  "action.open.tip": "WAV / FLAC またはプロジェクト(.iwproj)を開きます（元ファイルは変更しません）",
  "action.saveProject": "プロジェクト保存",
  "action.verify": "PCM照合",
  "action.verify.tip": "別のファイルを選び、PCMが完全一致するか検証します",
  "action.cancel": "キャンセル",

  // Left rail (tracks)
  "rail.tracks": "トラック",
  "rail.noTracks": "トラックがありません。「+ 追加」または「分割」で作成します。",
  "rail.add": "追加",
  "rail.split": "分割",
  "opt.silence": "無音で分割",
  "opt.ab": "A/B面で2分割",
  "track.default": "トラック 1",

  // Region editor
  "re.selected": "選択中のリージョン",
  "re.remove": "削除",
  "label.from": "開始 (秒)",
  "label.to": "終了 (秒)",
  "label.gainDb": "ゲイン (dB)",
  "label.fadeInS": "フェードイン (秒)",
  "label.fadeOutS": "フェードアウト (秒)",
  "label.dcCorrectFlag": "DC補正",
  "label.export16flag": "16bit派生",

  // Inspector
  "insp.file": "ファイル情報",
  "insp.proof": "保全の証明",
  "proof.integerPcm": "整数PCMを保持",
  "proof.noConversion": "浮動小数点変換なし",
  "proof.hint": "書き出すと処理レポートがここに表示されます。",
  "label.format": "形式",
  "label.bitDepth": "ビット深度",
  "label.sampleRate": "サンプルレート",
  "label.channels": "チャンネル",
  "label.duration": "長さ",
  "label.peak": "ピーク",
  "label.clipped": "クリップ数",
  "label.dcOffset": "DCオフセット",

  // Export dock
  "label.outputFormat": "出力形式",
  "label.overwrite": "上書き",
  "proj.master": "マスター書き出し (24bit)",
  "proj.master.tip": "再量子化・ディザ・リサンプリングなし。16bit派生を含むリージョンがあると拒否されます",
  "proj.derivative": "配布用 (16bit可)",
  "proj.derivative.tip": "16bit派生（TPDFディザ・再量子化）を許可して書き出します",

  // Transport / status / messages
  "action.play": "再生",
  "action.pause": "一時停止",
  "action.stop": "停止",
  "msg.dropHint": "「開く」から WAV / FLAC または .iwproj を読み込んでください",
  "msg.waveHint": "波形をクリックで再生位置へ、リージョンの端をドラッグで範囲を調整",
  "msg.processing": "処理中…",
  "msg.noFile": "ファイルが読み込まれていません。",
  "msg.none": "なし",

  "notice.exported": "{n} 件を書き出しました。",
  "notice.saved": "プロジェクトを保存しました。",
  "notice.noCuts": "分割点が見つかりませんでした（無音区間が検出されていません）。",
  "err.noSourceInProject": "プロジェクトに音源が含まれていません。",

  // Preservation proof badges (from ProcessReport)
  "status.integerSafe": "Integer Safe",
  "status.fpSavePath": "保存経路で浮動小数点",
  "status.sampleModified": "サンプル値変更",
  "status.requantized": "再量子化",
  "status.dither": "ディザ",
  "status.resampled": "リサンプリング",
  "status.pcmVerified": "PCM一致",
  "status.on": "ON",
  "status.yes": "YES",
  "status.no": "NO",
  "report.title": "処理レポート",
};

const en: Dict = {
  "app.subtitle": "Integer-PCM archive editor",

  "action.open": "Open",
  "action.open.tip": "Open a WAV / FLAC or a project (.iwproj) — the source is never modified",
  "action.saveProject": "Save project",
  "action.verify": "Verify PCM",
  "action.verify.tip": "Pick another file and check the PCM is byte-identical",
  "action.cancel": "Cancel",

  "rail.tracks": "Tracks",
  "rail.noTracks": "No tracks yet. Use “+ Add” or “Split” to create some.",
  "rail.add": "Add",
  "rail.split": "Split",
  "opt.silence": "By silence",
  "opt.ab": "A/B side (2)",
  "track.default": "Track 1",

  "re.selected": "Selected region",
  "re.remove": "Remove",
  "label.from": "From (s)",
  "label.to": "To (s)",
  "label.gainDb": "Gain (dB)",
  "label.fadeInS": "Fade in (s)",
  "label.fadeOutS": "Fade out (s)",
  "label.dcCorrectFlag": "DC correct",
  "label.export16flag": "16-bit",

  "insp.file": "File",
  "insp.proof": "Preservation proof",
  "proof.integerPcm": "Integer PCM preserved",
  "proof.noConversion": "No floating-point conversion",
  "proof.hint": "The processing report appears here after export.",
  "label.format": "Format",
  "label.bitDepth": "Bit depth",
  "label.sampleRate": "Sample rate",
  "label.channels": "Channels",
  "label.duration": "Duration",
  "label.peak": "Peak",
  "label.clipped": "Clipped",
  "label.dcOffset": "DC offset",

  "label.outputFormat": "Output format",
  "label.overwrite": "Overwrite",
  "proj.master": "Export master (24-bit)",
  "proj.master.tip": "No requantize/dither/resample. Refused if any region is a 16-bit derivative.",
  "proj.derivative": "Derivative (16-bit ok)",
  "proj.derivative.tip": "Allows a 16-bit derivative (TPDF dither, requantized).",

  "action.play": "Play",
  "action.pause": "Pause",
  "action.stop": "Stop",
  "msg.dropHint": "Use “Open” to load a WAV / FLAC or .iwproj",
  "msg.waveHint": "Click the waveform to seek; drag a region edge to adjust its range",
  "msg.processing": "Processing…",
  "msg.noFile": "No file loaded.",
  "msg.none": "none",

  "notice.exported": "Exported {n} file(s).",
  "notice.saved": "Project saved.",
  "notice.noCuts": "No split points found (no silence detected).",
  "err.noSourceInProject": "The project contains no source.",

  "status.integerSafe": "Integer Safe",
  "status.fpSavePath": "Floating point in save path",
  "status.sampleModified": "Sample values modified",
  "status.requantized": "Requantized",
  "status.dither": "Dither",
  "status.resampled": "Resampled",
  "status.pcmVerified": "PCM verified",
  "status.on": "ON",
  "status.yes": "YES",
  "status.no": "NO",
  "report.title": "Processing report",
};

const dicts: Record<Locale, Dict> = { ja, en };

export function makeT(locale: Locale) {
  const d = dicts[locale];
  return (key: string): string => d[key] ?? key;
}
