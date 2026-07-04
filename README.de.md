# intwav

[English](README.md) | [日本語](README.ja.md) | [Français](README.fr.md) | [Español](README.es.md) | [Deutsch](README.de.md) | [简体中文](README.zh-CN.md) | [한국어](README.ko.md)

**Ganzzahl-PCM-Schutzwerkzeug für die Audioverarbeitung** — zur Archivierung analoger Übertragungen (Schallplatten, Tonbänder, Kassetten), die in 24-Bit-PCM digitalisiert wurden.

> 24-Bit-PCM genau so bewahren, wie es erfasst wurde. Keine Audioverbesserung — Audioerhaltung.

intwav prüft, schneidet und archiviert Ganzzahl-PCM verlustfrei **ohne** Fließkomma-Konvertierung, Requantisierung oder Resampling. Es ist keine DAW und "verbessert" das Audio nicht — es bewahrt das PCM exakt so, wie es aufgenommen wurde, und speichert es als verlustfreies FLAC mit einem nachvollziehbaren und protokollierten Verarbeitungspfad.

## Status: v0.1

Implementierte Befehle:

| Befehl | Zweck |
|---|---|
| `intwav info <in>`   | Format, Parameter, Dauer, Spitzenwert (Peak), Anzahl der Übersteuerungen (Clips) |
| `intwav check <in>`  | Vollständige Prüfung: Info + DC-Offset + Stille-Erkennung |
| `intwav peak <in>`   | Spitzenpegel pro Kanal (dBFS + Rohwert) |
| `intwav clips <in>`  | Anzahl der übersteuerten Samples |
| `intwav trim <in> [out] --from <ts> --to <ts>` | Bereich extrahieren, Sample-Werte bleiben unverändert |

Zeitstempel sind im Format `HH:MM:SS.mmm`, `MM:SS.mmm`, `SS.mmm` oder in einfachen Sekunden anzugeben.
`trim` akzeptiert `--output-format flac|wav` (Standard: wird aus der Ausgabedateiendung abgeleitet, sonst FLAC) und `--report <path>` für einen JSON-Verarbeitungsbericht (§13).

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
  intwav-core   rein ganzzahlige Verarbeitung: Analyse, dBFS, Frame-Slicing (ohne Fließkommazahlen geprüft)
  intwav-codec  Ganzzahl-E/S für WAV (hound) + FLAC (claxon-Dekodierung / flac-CLI-Kodierung)
  intwav-cli    die `intwav`-Binärdatei: Befehlsanalyse, Datei-E/S, JSON-Berichte
```

## Bauen und Testen

```bash
cargo build --release          # Binärdatei unter target/release/intwav
cargo test --workspace         # Unit- + End-to-End-Tests
bash scripts/check-no-float.sh # Garantie der Fließkommafreiheit überprüfen
```

Erfordert das Befehlszeilenwerkzeug `flac` für die FLAC-Ausgabe.

## Lizenz
Apache-2.0
