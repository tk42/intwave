# intwav

[English](README.md) | [日本語](README.ja.md) | [Français](README.fr.md) | [Español](README.es.md) | [Deutsch](README.de.md) | [简体中文](README.zh-CN.md) | [한국어](README.ko.md)

**整数PCM保護型オーディオ処理ツール** — 24bit PCMにデジタル化されたアナログ音源（レコード、オープンリール、カセットテープ）をアーカイブするためのツールです。

> 24bit PCMを、そのまま守る。音質改善ではなく、音質保全。

intwav は、浮動小数点変換、再量子化、リサンプリングを**一切行わず**、整数PCMの検査、トリミング、可逆圧縮アーカイブを実行します。DAWではないため、音質の「改善」は行いません。取り込んだPCMをそっくりそのまま保持し、説明可能でログ記録された処理パスを通じて、可逆圧縮のFLACとして保存します。

## ステータス: v0.4

実装済みのコマンド:

| コマンド | 目的 |
|---|---|
| `intwav info <in>`   | フォーマット、パラメータ、再生時間、ピーク、クリップ数の表示 |
| `intwav check <in>`  | 完全検査: info + DCオフセット + 無音検出 |
| `intwav peak <in>`   | チャンネルごとのピークレベル (dBFS + 生の値) |
| `intwav clips <in>`  | クリップ（音割れ）サンプルのカウント |
| `intwav trim <in> [out] --from <ts> --to <ts>` | サンプル値を一切変更せずに範囲を抽出 |
| `intwav split <in> --out <dir> (--cue <f> \| --by silence\|ab)` | メタデータを保持してトラック分割（CUEリスト、無音検出、またはA/B面） |
| `intwav gain <in> <out> --db <n>` | 固定小数点ゲイン調整、整数dB (-96..=24)。正のゲイン (`+`) には `--allow-clipping` が必要 |
| `intwav fade-in <in> <out> --duration <d>` | 線形固定小数点フェードイン |
| `intwav fade-out <in> <out> --duration <d>` | 線形固定小数点フェードアウト |
| `intwav dc-correct <in> <out>` | チャンネルごとのDCオフセットを除去 |
| `intwav export16 <in> <out> [--dither tpdf]` | TPDFディザリングを用いた16-bit派生出力（マスター用ではありません） |
| `intwav verify <a> [b]` | PCMのチェックサム計算、または2つのファイルが同一のPCMを保持しているかの検証 |

タイムスタンプは `HH:MM:SS.mmm`、`MM:SS.mmm`、`SS.mmm`、または秒数そのものを指定できます。また、期間（duration）には `5s` / `250ms` の形式も使用可能です。
すべての処理コマンドは `--output-format flac|wav`（デフォルト: 出力ファイルの拡張子から推論、指定がなければFLAC）および、PCMのSHA-256チェックサムと処理ログのハッシュ値を含むJSON処理レポート (§13/§22) を出力する `--report <path>` を受け付けます。

ゲイン、フェード、DC補正、および16-bitディザリングはすべて**固定小数点整数**演算です。ゲイン係数は事前計算されたQ31テーブルから取得され（`pow` 関数非使用）、TPDFディザリングは再現可能な `--seed` を指定できる整数PRNG（疑似乱数生成器）を使用します。

### 対応フォーマット

* 入力: WAV および FLAC、16/24/32-bit **整数** PCM、モノラルまたはステレオ。
* 出力: FLAC (デフォルト) または WAV。
* 浮動小数点WAV、圧縮WAV、MP3/AAC/Opus、DSD、マルチチャンネル音源は明示的なエラーとともに**拒否**されます（暗黙的に変換されることはありません）。

## 浮動小数点非使用の保証 (The float-free guarantee)

すべてのサンプル演算は `intwav-core` 内で行われます。このクレートは `no_std` + `alloc` で依存関係がなく、**浮動小数点を一切使用しません**。dBFS の計算すら固定小数点整数の対数近似を用いて行われています（誤差 < 0.004 dB）。FLAC のデコードには純Rust製の `claxon` を使用し、FLAC のエンコードは外部の `flac` バイナリに委譲しているため、libFLAC 内部の浮動小数点解析が当プロセスに入ることはありません。

`scripts/check-no-float.sh` は CI でこれを強制します。コアソースの浮動小数点構文をスキャンし、コンパイルされたコアオブジェクトを逆アセンブルして、浮動小数点演算命令 (x86-64 SSE/x87 または aarch64 FP) が現れた場合にビルドを失敗させます。

## アーキテクチャ

```
crates/
  intwav-core   整数のみのDSP: 解析、dBFS、スライシング、ゲイン/フェード/DC、TPDFディザ (浮動小数点非使用スキャン済み)
  intwav-codec  WAV (hound) + FLAC (claxon デコード / flac-CLI エンコード) の整数入出力 + メタデータ
  intwav-cli    `intwav` バイナリ: コマンド解析、ファイルI/O、JSONレポート、チェックサム
```

## ビルドとテスト

```bash
cargo build --release          # バイナリ生成先: target/release/intwav
cargo test --workspace         # ユニットテスト + E2Eテスト
bash scripts/check-no-float.sh # 浮動小数点非使用の保証を検証
```

FLAC 出力にはコマンドラインツール `flac` が必要です。

## ライセンス
Apache-2.0
