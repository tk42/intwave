# intwav

[English](README.md) | [日本語](README.ja.md) | [Français](README.fr.md) | [Español](README.es.md) | [Deutsch](README.de.md) | [简体中文](README.zh-CN.md) | [한국어](README.ko.md)

**Ganzzahl-PCM-Schutzwerkzeug für die Audioverarbeitung** — zur Archivierung analoger Übertragungen (Schallplatten, Tonbänder, Kassetten), die in 24-Bit-PCM digitalisiert wurden.

> 24-Bit-PCM genau so bewahren, wie es erfasst wurde. Keine Audioverbesserung — Audioerhaltung.

intwav prüft, schneidet und archiviert Ganzzahl-PCM verlustfrei **ohne** Fließkomma-Konvertierung, Requantisierung oder Resampling. Es ist keine DAW und "verbessert" das Audio nicht — es bewahrt das PCM exakt so, wie es aufgenommen wurde, und speichert es als verlustfreies FLAC mit einem nachvollziehbaren und protokollierten Verarbeitungspfad.

## Status: v0.4

Implementierte Befehle:

| Befehl | Zweck |
|---|---|
| `intwav info <in>`   | Format, Parameter, Dauer, Spitzenwert (Peak), Anzahl der Übersteuerungen (Clips) |
| `intwav check <in>`  | Vollständige Prüfung: Info + DC-Offset + Stille-Erkennung |
| `intwav peak <in>`   | Spitzenpegel pro Kanal (dBFS + Rohwert) |
| `intwav clips <in>`  | Anzahl der übersteuerten Samples |
| `intwav trim <in> [out] --from <ts> --to <ts>` | Bereich extrahieren, Sample-Werte bleiben unverändert |
| `intwav split <in> --out <dir> (--cue <f> \| --by silence\|ab)` | In Tracks aufteilen (CUE-Liste, Stille oder A/B-Seite) mit Metadaten |
| `intwav gain <in> <out> --db <n>` | Festkomma-Gain, ganzzahlige dB (-96..=24); positiver (`+`) Gain erfordert `--allow-clipping` |
| `intwav fade-in <in> <out> --duration <d>` | Lineares Festkomma-Fade-In |
| `intwav fade-out <in> <out> --duration <d>` | Lineares Festkomma-Fade-Out |
| `intwav dc-correct <in> <out>` | DC-Offset pro Kanal entfernen |
| `intwav export16 <in> <out> [--dither tpdf]` | 16-Bit-Derivatausgabe mit TPDF-Dither (kein Master) |
| `intwav verify <a> [b]` | PCM-Prüfsumme berechnen oder nachweisen, dass zwei Dateien identisches PCM enthalten |

Zeitstempel sind im Format `HH:MM:SS.mmm`, `MM:SS.mmm`, `SS.mmm` oder in einfachen Sekunden anzugeben; Dauern akzeptieren auch `5s` / `250ms`.
Alle Verarbeitungsbefehle akzeptieren `--output-format flac|wav` (Standard: wird aus der Ausgabedateiendung abgeleitet, sonst FLAC) und `--report <path>` für einen JSON-Verarbeitungsbericht (§13/§22), der PCM-SHA-256-Prüfsummen und einen Hash des Verarbeitungsprotokolls enthält.

Gain, Fades, DC-Korrektur und 16-Bit-Dithering sind ausnahmslos **Festkomma-Ganzzahloperationen**. Gain-Koeffizienten stammen aus einer vorberechneten Q31-Tabelle (ohne `pow`); TPDF-Dithering nutzt einen ganzzahligen PRNG (Pseudozufallszahlengenerator) mit einem reproduzierbaren `--seed`.

### Formate

* Eingang: WAV und FLAC, 16/24/32-Bit **Ganzzahl**-PCM (Integer), Mono oder Stereo.
* Ausgang: FLAC (Standard) oder WAV.
* Fließkomma-WAV, komprimiertes WAV, MP3/AAC/Opus, DSD und Mehrkanalton werden mit einer expliziten Fehlermeldung **abgelehnt** — es findet niemals eine stille Konvertierung statt.

## Die Garantie der Fließkommafreiheit (The float-free guarantee)

Die gesamte Sample-Mathematik befindet sich in `intwav-core`, das `no_std` + `alloc` ist, keine Abhängigkeiten aufweist und **keine Fließkommazahlen** verwendet — einschließlich dBFS, das mit einer Festkomma-Integer-Logarithmusnäherung berechnet wird (Genauigkeit < 0,004 dB). Die FLAC-Dekodierung nutzt die rein in Rust geschriebene Bibliothek `claxon`; die FLAC-Kodierung wird an die externe `flac`-Binärdatei delegiert, sodass die interne Fließkommaanalyse von libFLAC diesen Prozess niemals berührt.

`scripts/check-no-float.sh` setzt dies in der CI durch: Es durchsucht den Core-Quellcode nach Fließkommakonstrukten und disassembliert das kompilierte Core-Objekt, wobei der Build fehlschlägt, falls eine Fließkomma-Arithmetik-Anweisung (x86-64 SSE/x87 oder aarch64 FP) auftaucht.

## Architektur

```
crates/
  intwav-core     rein ganzzahlige DSP: Analyse, gefensterte Stille-Erkennung, dBFS, Slicing, Gain/Fade/DC, TPDF-Dither (ohne Fließkommazahlen geprüft)
  intwav-codec    Ganzzahl-E/S für WAV (hound) + FLAC (claxon-Dekodierung / flac-CLI-Kodierung), Metadaten, Header-Probe
  intwav-engine   gemeinsame CLI/GUI-Engine: Operationen, eingefrorener JSON-Bericht, codierte Fehler, verifizierte atomare Schreibvorgänge, einmalig decodierte Scratch-Datei + Wellenform-Pyramide, zerstörungsfreies Projekt (.iwproj) + Undo/Render (fließkommafreier Quellcode)
  intwav-playback Vorschau-Wiedergabe (cpal): Vorschau der Ganzzahl-Operationskette, Fließkomma nur an der Gerätegrenze — außerhalb des Speicherpfads, NICHT auf Fließkommazahlen geprüft
  intwav-cli      die `intwav`-Binärdatei: schlankes Front-End über der Engine
```

Das Crate `intwav-engine` bildet die Grundlage für die GUI (Tauri + React): Jede Operation verläuft synchron und aufrufergesteuert (Fortschritt + Abbrechen), jeder Schreibvorgang wird verifiziert (`pcm_verified`), und die CLI sowie die GUI teilen sich diese Engine unverändert. `open_source` decodiert eine große Quelle einmalig in eine spulbare Scratch-Datei (seekable) und erstellt in einem einzigen Durchlauf gleichzeitig die Wellenform und den PCM-Hash. `intwav-playback` gibt die Vorschau aus dieser Scratch-Datei wieder, wobei exakt dieselbe Ganzzahl-Operationskette wie beim Export ausgeführt wird und Fließkomma nur bei der finalen Konvertierung für das Audiogerät zum Einsatz kommt (native Abtastrate bevorzugt, Fließkomma-Resampling als Fallback).

## GUI (Tauri + React) — Vorschau

Eine Desktop-GUI befindet sich unter `app/`: ein Tauri-v2-Backend (`src-tauri/`, ein vom Core-Workspace **entkoppeltes** Crate, damit sein aufwändiger Build die CI nicht verlangsamt), das die Engine als Befehle bereitstellt, sowie ein React- + TypeScript-Frontend (standardmäßig Japanisch, zweisprachig). Es öffnet WAV/FLAC über `open_source` (einmalig decodierte Scratch-Datei + Wellenform), zeigt die Wellenform sowie ein **Integer-Safe**-Statuspanel an, das von den Fakten des eingefrorenen Berichts gesteuert wird, und führt trim/gain/export16/verify mit Live-Fortschritts- und Abbruchfunktion aus — alles über dieselbe Engine, die auch die CLI nutzt.

```bash
cd app
npm install
npm run tauri dev     # Entwicklung (erfordert eine Desktop-Sitzung)
npm run tauri build   # signierte App bündeln (macOS/Windows/Linux)
```

Das Frontend lässt sich mit `npm run build` im Headless-Modus bauen; das Backend wird mit `cargo check` innerhalb von `app/src-tauri` kompiliert und geprüft.

## Bauen und Testen

```bash
cargo build --release          # Binärdatei unter target/release/intwav
cargo test --workspace         # Unit- + End-to-End-Tests
bash scripts/check-no-float.sh # Garantie der Fließkommafreiheit überprüfen
```

Erfordert das Befehlszeilenwerkzeug `flac` für die FLAC-Ausgabe.

## Lizenz
Apache-2.0
