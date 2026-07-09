// Minimal bilingual i18n, Japanese-default (Q17). The machine layer (reports,
// error codes) never localizes; only these UI strings do.

export type Locale = "ja" | "en";

type Dict = Record<string, string>;

const ja: Dict = {
  "app.title": "intwav — 整数PCM保護アーカイブエディタ",
  "action.open": "ファイルを開く",
  "action.trim": "トリム",
  "action.export16": "16bit派生を書き出す",
  "action.gain": "ゲイン",
  "action.verify": "PCM検証",
  "action.cancel": "キャンセル",
  "label.format": "形式",
  "label.bitDepth": "ビット深度",
  "label.sampleRate": "サンプルレート",
  "label.channels": "チャンネル数",
  "label.duration": "再生時間",
  "label.peak": "ピーク",
  "label.clipped": "クリップ数",
  "label.from": "開始 (秒)",
  "label.to": "終了 (秒)",
  "label.db": "ゲイン (dB)",
  "label.allowClipping": "クリップを許可",
  "label.outputFormat": "出力形式",
  "label.overwrite": "上書きする",
  "status.integerSafe": "Integer Safe",
  "status.fpSavePath": "保存経路で浮動小数点",
  "status.sampleModified": "サンプル値変更",
  "status.requantized": "再量子化",
  "status.dither": "ディザ",
  "status.resampled": "リサンプリング",
  "status.pcmVerified": "PCM検証",
  "status.on": "ON",
  "status.yes": "YES",
  "status.no": "NO",
  "msg.noFile": "ファイルが読み込まれていません。",
  "msg.dropHint": "WAV / FLAC を開いてください",
  "msg.processing": "処理中…",
  "msg.done": "完了",
  "report.title": "処理レポート",
};

const en: Dict = {
  "app.title": "intwav — Integer-PCM Archive Editor",
  "action.open": "Open file",
  "action.trim": "Trim",
  "action.export16": "Export 16-bit derivative",
  "action.gain": "Gain",
  "action.verify": "Verify PCM",
  "action.cancel": "Cancel",
  "label.format": "Format",
  "label.bitDepth": "Bit depth",
  "label.sampleRate": "Sample rate",
  "label.channels": "Channels",
  "label.duration": "Duration",
  "label.peak": "Peak",
  "label.clipped": "Clipped",
  "label.from": "From (s)",
  "label.to": "To (s)",
  "label.db": "Gain (dB)",
  "label.allowClipping": "Allow clipping",
  "label.outputFormat": "Output format",
  "label.overwrite": "Overwrite",
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
  "msg.noFile": "No file loaded.",
  "msg.dropHint": "Open a WAV / FLAC file",
  "msg.processing": "Processing…",
  "msg.done": "Done",
  "report.title": "Processing report",
};

const dicts: Record<Locale, Dict> = { ja, en };

export function makeT(locale: Locale) {
  const d = dicts[locale];
  return (key: string): string => d[key] ?? key;
}
