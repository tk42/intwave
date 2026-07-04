# intwav

[English](README.md) | [日本語](README.ja.md) | [Français](README.fr.md) | [Español](README.es.md) | [Deutsch](README.de.md) | [简体中文](README.zh-CN.md) | [한국어](README.ko.md)

**Herramienta de protección de audio PCM entero** — para el archivo de transferencias analógicas (discos, cintas, casetes) digitalizadas en PCM de 24 bits.

> Preservando el PCM de 24 bits tal como fue capturado. No es mejora de audio — es preservación de audio.

intwav inspecciona, recorta y archiva sin pérdida PCM entero **sin** conversión a punto flotante, recuantificación ni remuestreo. No es una estación de trabajo de audio digital (DAW) ni "mejora" el audio; preserva el PCM exactamente como se capturó y lo almacena como FLAC sin pérdida, con una ruta de procesamiento explicable y registrada en un registro.

## Estado: v0.1

Comandos implementados:

| Comando | Propósito |
|---|---|
| `intwav info <in>`   | Formato, parámetros, duración, pico, recuento de saturaciones (clips) |
| `intwav check <in>`  | Inspección completa: info + desplazamiento DC + detección de silencio |
| `intwav peak <in>`   | Nivel de pico por canal (dBFS + valor en bruto) |
| `intwav clips <in>`  | Recuento de muestras saturadas |
| `intwav trim <in> [out] --from <ts> --to <ts>` | Extraer un rango, manteniendo los valores de muestra intactos |

Las marcas de tiempo están en formato `HH:MM:SS.mmm`, `MM:SS.mmm`, `SS.mmm` o en segundos simples.
`trim` acepta `--output-format flac|wav` (por defecto: se deduce por la extensión de salida; si no, FLAC) y `--report <path>` para generar un informe de procesamiento JSON (§13).

### Formatos

* Entrada: WAV y FLAC, PCM **entero** de 16/24/32 bits, mono o estéreo.
* Salida: FLAC (por defecto) o WAV.
* WAV de punto flotante, WAV comprimido, MP3/AAC/Opus, DSD y multicanal son **rechazados** con un error explícito — nunca se convierten de forma silenciosa.

## La garantía libre de punto flotante

Todas las matemáticas de muestras residen en `intwav-core`, que es `no_std` + `alloc`, no tiene dependencias y **no utiliza punto flotante** — incluido dBFS, que se calcula con una aproximación logarítmica de enteros de punto fijo (precisión < 0.004 dB). La decodificación FLAC utiliza `claxon` puro en Rust; la codificación FLAC se delega al binario externo `flac`, por lo que el análisis interno de punto flotante de libFLAC nunca entra en este proceso.

`scripts/check-no-float.sh` impone esto en CI: escanea el código fuente en busca de construcciones de punto flotante y desensambla el objeto compilado, haciendo fallar la compilación si aparece cualquier instrucción aritmética de punto flotante (x86-64 SSE/x87 o aarch64 FP).

## Arquitectura

```
crates/
  intwav-core   procesamiento puramente entero: análisis, dBFS, segmentación de tramas (escaneado sin flotantes)
  intwav-codec  E/S entera de WAV (hound) + FLAC (decodificación claxon / codificación flac-CLI)
  intwav-cli    el binario `intwav`: análisis de comandos, E/S de archivos, informes JSON
```

## Compilación y pruebas

```bash
cargo build --release          # binario en target/release/intwav
cargo test --workspace         # pruebas unitarias y de extremo a extremo
bash scripts/check-no-float.sh # verificar la garantía libre de punto flotante
```

Requiere la herramienta de línea de comandos `flac` para la salida en formato FLAC.

## Licencia
Apache-2.0
