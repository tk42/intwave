# intwav

[English](README.md) | [日本語](README.ja.md) | [Français](README.fr.md) | [Español](README.es.md) | [Deutsch](README.de.md) | [简体中文](README.zh-CN.md) | [한국어](README.ko.md)

**Outil de protection audio PCM entier** — pour l'archivage de transferts analogiques (disques, bandes, cassettes) numérisés en PCM 24 bits.

> Préserver le PCM 24 bits tel quel. Pas d'amélioration audio — de la préservation audio.

intwav inspecte, découpe et archive sans perte le PCM entier **sans** conversion en virgule flottante, requantification ou rééchantillonnage. Ce n'est pas une station de travail audio numérique (DAW) et il n'« améliore » pas l'audio — il préserve le PCM exactement tel qu'il a été capturé et le stocke sous forme de FLAC sans perte, avec un chemin de traitement explicable et journalisé.

## Statut : v0.4

Commandes implémentées :

| Commande | Objectif |
|---|---|
| `intwav info <in>`   | Format, paramètres, durée, crête (peak), nombre d'écrêtages (clips) |
| `intwav check <in>`  | Inspection complète : info + décalage DC + détection de silence |
| `intwav peak <in>`   | Niveau de crête par canal (dBFS + brut) |
| `intwav clips <in>`  | Comptage des échantillons écrêtés |
| `intwav trim <in> [out] --from <ts> --to <ts>` | Extraire une plage, valeurs d'échantillon inchangées |
| `intwav split <in> --out <dir> (--cue <f> \| --by silence\|ab)` | Découper en pistes (liste CUE, silence, ou face A/B) avec métadonnées |
| `intwav gain <in> <out> --db <n>` | Gain en virgule fixe, dB entier (-96..=24) ; un gain `+` nécessite `--allow-clipping` |
| `intwav fade-in <in> <out> --duration <d>` | Fondu entrant (fade-in) linéaire en virgule fixe |
| `intwav fade-out <in> <out> --duration <d>` | Fondu sortant (fade-out) linéaire en virgule fixe |
| `intwav dc-correct <in> <out>` | Supprimer le décalage DC par canal |
| `intwav export16 <in> <out> [--dither tpdf]` | Sortie dérivée 16 bits avec dithering TPDF (pas un master) |
| `intwav verify <a> [b]` | Calculer la somme de contrôle PCM, ou prouver que deux fichiers contiennent un PCM identique |

Les horodatages sont au format `HH:MM:SS.mmm`, `MM:SS.mmm`, `SS.mmm` ou simplement en secondes ; les durées acceptent également `5s` / `250ms`.
Toutes les commandes de traitement acceptent `--output-format flac|wav` (par défaut : déduit à partir de l'extension de sortie, sinon FLAC) et `--report <path>` pour un rapport de traitement JSON (§13/§22) contenant les sommes de contrôle SHA-256 du PCM et un hachage du journal de traitement.

Le gain, les fondus, la correction DC et le dithering 16 bits sont tous des opérations en **entier à virgule fixe**. Les coefficients de gain proviennent d'une table Q31 précalculée (sans `pow`) ; le dithering TPDF utilise un générateur de nombres pseudo-aléatoires (PRNG) entier avec un `--seed` reproductible.

### Formats

* Entrée : WAV et FLAC, PCM **entier** 16/24/32 bits, mono ou stéréo.
* Sortie : FLAC (par défaut) ou WAV.
* Les fichiers WAV en virgule flottante, WAV compressés, MP3/AAC/Opus, DSD et multicanaux sont **rejetés** avec une erreur explicite — et ne sont jamais convertis silencieusement.

## La garantie sans virgule flottante

Tous les calculs sur les échantillons se trouvent dans `intwav-core`, qui est `no_std` + `alloc`, ne possède aucune dépendance et n'utilise **aucune virgule flottante** — y compris le dBFS, qui est calculé avec une approximation logarithmique entière à virgule fixe (précision < 0,004 dB). Le décodage FLAC utilise la bibliothèque Rust pure `claxon` ; l'encodage FLAC est délégué au binaire externe `flac` de sorte que l'analyse en virgule flottante interne de libFLAC n'entre jamais dans ce processus.

`scripts/check-no-float.sh` applique cette règle dans l'intégration continue (CI) : il scanne le code source principal à la recherche de constructions en virgule flottante et désassemble l'objet compilé, faisant échouer le build si une instruction arithmétique en virgule flottante (x86-64 SSE/x87 ou aarch64 FP) apparaît.

## Architecture

```
crates/
  intwav-core     DSP uniquement entier : analyse, détection de silence fenestrée, dBFS, découpage, gain/fondu/DC, dither TPDF (scanné sans float)
  intwav-codec    E/S entières WAV (hound) + FLAC (décodage claxon / encodage flac-CLI), métadonnées, sonde d'en-tête
  intwav-engine   moteur partagé CLI/GUI : opérations, rapport JSON figé, erreurs codées, écritures atomiques vérifiées, fichier temporaire (scratch) décodé une seule fois + pyramide de forme d'onde (source sans virgule flottante)
  intwav-playback lecture d'aperçu (cpal) : aperçu de la chaîne d'opérations entière, virgule flottante uniquement à la frontière du périphérique — hors du chemin d'enregistrement, NON scanné pour les floats
  intwav-cli      le binaire `intwav` : interface légère au-dessus du moteur
```

La crate `intwav-engine` constitue la base d'une future interface graphique (GUI avec Tauri + React) : chaque opération est synchrone et pilotée par l'appelant (progression + annulation), chaque écriture est vérifiée (`pcm_verified`), et la CLI ainsi que l'interface graphique le partagent à l'identique. `open_source` décode une seule fois une source volumineuse dans un fichier temporaire (scratch) avec recherche (seekable) tout en construisant la forme d'onde et le hachage PCM en une seule passe. `intwav-playback` lit l'aperçu à partir de ce fichier temporaire en exécutant la même chaîne d'opérations entière que lors de l'exportation, en utilisant les virgules flottantes uniquement lors de la conversion finale pour le périphérique audio (priorité à la fréquence native, rééchantillonnage flottant en secours). L'interface graphique elle-même (Tauri + React) constitue la phase restante.

## Compilation et tests

```bash
cargo build --release          # binaire dans target/release/intwav
cargo test --workspace         # tests unitaires et de bout en bout
bash scripts/check-no-float.sh # vérifier la garantie sans virgule flottante
```

Nécessite l'outil en ligne de commande `flac` pour la sortie FLAC.

## Licence
Apache-2.0
