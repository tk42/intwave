# intwav

[English](README.md) | [日本語](README.ja.md) | [Français](README.fr.md) | [Español](README.es.md) | [Deutsch](README.de.md) | [简体中文](README.zh-CN.md) | [한국어](README.ko.md)

**Herramienta de protección de audio PCM entero** — para el archivo de transferencias analógicas (discos, cintas, casetes) digitalizadas en PCM de 24 bits.

> Preservando el PCM de 24 bits tal como fue capturado. No es mejora de audio — es preservación de audio.

intwav inspecciona, recorta y archiva sin pérdida PCM entero **sin** conversión a punto flotante, recuantificación ni remuestreo. No es una estación de trabajo de audio digital (DAW) ni "mejora" el audio; preserva el PCM exactamente como se capturó y lo almacena como FLAC sin pérdida, con una ruta de procesamiento explicable y registrada en un registro.

## Estado: v0.4

Comandos implementados:

| Comando | Propósito |
|---|---|
| `intwav info <in>`   | Formato, parámetros, duración, pico, recuento de saturaciones (clips) |
| `intwav check <in>`  | Inspección completa: info + desplazamiento DC + detección de silencio |
| `intwav peak <in>`   | Nivel de pico por canal (dBFS + valor en bruto) |
| `intwav clips <in>`  | Recuento de muestras saturadas |
| `intwav trim <in> [out] --from <ts> --to <ts>` | Extraer un rango, manteniendo los valores de muestra intactos |
| `intwav split <in> --out <dir> (--cue <f> \| --by silence\|ab)` | Dividir en pistas (lista CUE, silencio o cara A/B) con metadatos |
| `intwav gain <in> <out> --db <n>` | Ganancia de punto fijo, dB entero (-96..=24); ganancia positiva (`+`) requiere `--allow-clipping` |
| `intwav fade-in <in> <out> --duration <d>` | Fade-in lineal de punto fijo |
| `intwav fade-out <in> <out> --duration <d>` | Fade-out lineal de punto fijo |
| `intwav dc-correct <in> <out>` | Eliminar el desplazamiento DC por canal |
| `intwav export16 <in> <out> [--dither tpdf]` | Salida derivada de 16 bits con dithering TPDF (no es un máster) |
| `intwav verify <a> [b]` | Calcular suma de comprobación PCM, o demostrar que dos archivos contienen idéntico PCM |

Las marcas de tiempo están en formato `HH:MM:SS.mmm`, `MM:SS.mmm`, `SS.mmm` o en segundos simples; las duraciones también aceptan `5s` / `250ms`.
Todos los comandos de procesamiento aceptan `--output-format flac|wav` (por defecto: se deduce por la extensión de salida; si no, FLAC) y `--report <path>` para generar un informe de procesamiento JSON (§13/§22) que incluye sumas de comprobación SHA-256 del PCM y un hash del registro de procesamiento.

La ganancia, los fades, la corrección DC y el dithering de 16 bits son operaciones totalmente en **enteros de punto fijo**. Los coeficientes de ganancia provienen de una tabla Q31 precalculada (sin `pow`); el dithering TPDF utiliza un generador de números pseudoaleatorios (PRNG) entero con un `--seed` reproducible.

### Formats

* Entrada: WAV y FLAC, PCM **entero** de 16/24/32 bits, mono o estéreo.
* Salida: FLAC (por defecto) o WAV.
* WAV de punto flotante, WAV comprimido, MP3/AAC/Opus, DSD y multicanal son **rechazados** con un error explícito — nunca se convierten de forma silenciosa.

## La garantía libre de punto flotante

Todas las matemáticas de muestras residen en `intwav-core`, que es `no_std` + `alloc`, no tiene dependencias y **no utiliza punto flotante** — incluido dBFS, que se calcula con una aproximación logarítmica de enteros de punto fijo (precisión < 0.004 dB). La decodificación FLAC utiliza `claxon` puro en Rust; la codificación FLAC se delega al binario externo `flac`, por lo que el análisis interno de punto flotante de libFLAC nunca entra en este proceso.

`scripts/check-no-float.sh` impone esto en CI: escanea el código fuente en busca de construcciones de punto flotante y desensambla el objeto compilado, haciendo fallar la compilación si aparece cualquier instrucción aritmética de punto flotante (x86-64 SSE/x87 o aarch64 FP).

## Arquitectura

```
crates/
  intwav-core     DSP puramente entero: análisis, detección de silencio con ventanas, dBFS, segmentación, ganancia/fade/DC, dither TPDF (escaneado sin flotantes)
  intwav-codec    E/S entera de WAV (hound) + FLAC (decodificación claxon / codificación flac-CLI), metadatos, sonda de encabezado
  intwav-engine   motor compartido para CLI/GUI: operaciones, informe JSON congelado, errores codificados, escrituras atómicas verificadas, archivo temporal (scratch) decodificado una sola vez + pirámide de forma de onda (fuente libre de flotantes)
  intwav-playback reproducción de vista previa (cpal): vista previa de cadena de operaciones entera, punto flotante únicamente en el límite del dispositivo — fuera de la ruta de guardado, NO escaneado de flotantes
  intwav-cli      el binario `intwav`: interfaz ligera sobre el motor
```

La crate `intwav-engine` es la base de una futura interfaz gráfica (GUI con Tauri + React): cada operación es síncrona y controlada por el llamante (progreso + cancelación), cada escritura es verificada (`pcm_verified`), y la CLI junto con la GUI lo comparten literalmente. `open_source` decodifica una sola vez una fuente de gran tamaño en un archivo temporal (scratch) en el que se puede buscar (seekable), al tiempo que construye la forma de onda y el hash PCM en una sola pasada. `intwav-playback` realiza la vista previa desde ese archivo temporal, ejecutando la misma cadena de operaciones entera que se usaría al exportar, utilizando punto flotante únicamente en la conversión final del dispositivo audio (priorizando la frecuencia nativa, con remuestreo flotante como alternativa de respaldo). La propia GUI (Tauri + React) constituye la fase restante.

## Compilación y pruebas

```bash
cargo build --release          # binario en target/release/intwav
cargo test --workspace         # pruebas unitarias y de extremo a extremo
bash scripts/check-no-float.sh # verificar la garantía libre de punto flotante
```

Requiere la herramienta de línea de comandos `flac` para la salida en formato FLAC.

## Licencia
Apache-2.0
