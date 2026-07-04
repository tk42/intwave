# intwav

[English](README.md) | [日本語](README.ja.md) | [Français](README.fr.md) | [Español](README.es.md) | [Deutsch](README.de.md) | [简体中文](README.zh-CN.md) | [한국어](README.ko.md)

**Outil de protection audio PCM entier** — pour l'archivage de transferts analogiques (disques, bandes, cassettes) numérisés en PCM 24 bits.

> Préserver le PCM 24 bits tel quel. Pas d'amélioration audio — de la préservation audio.

intwav inspecte, découpe et archive sans perte le PCM entier **sans** conversion en virgule flottante, requantification ou rééchantillonnage. Ce n'est pas une station de travail audio numérique (DAW) et il n'« améliore » pas l'audio — il préserve le PCM exactement tel qu'il a été capturé et le stocke sous forme de FLAC sans perte, avec un chemin de traitement explicable et journalisé.

## Statut : v0.1

Commandes implémentées :

| Commande | Objectif |
|---|---|
| `intwav info <in>`   | Format, paramètres, durée, crête (peak), nombre d'écrêtages (clips) |
| `intwav check <in>`  | Inspection complète : info + décalage DC + détection de silence |
| `intwav peak <in>`   | Niveau de crête par canal (dBFS + brut) |
| `intwav clips <in>`  | Comptage des échantillons écrêtés |
| `intwav trim <in> [out] --from <ts> --to <ts>` | Extraire une plage, valeurs d'échantillon inchangées |

Les horodatages sont au format `HH:MM:SS.mmm`, `MM:SS.mmm`, `SS.mmm` ou simplement en secondes.
`trim` accepte `--output-format flac|wav` (par défaut : déduit à partir de l'extension de sortie, sinon FLAC) et `--report <path>` pour un rapport de traitement JSON (§13).

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
  intwav-core   traitement uniquement entier : analyse, dBFS, découpage de trames (scanné sans float)
  intwav-codec  E/S entières WAV (hound) + FLAC (décodage claxon / encodage flac-CLI)
  intwav-cli    le binaire `intwav` : analyse des commandes, E/S de fichiers, rapports JSON
```

## Compilation et tests

```bash
cargo build --release          # binaire dans target/release/intwav
cargo test --workspace         # tests unitaires et de bout en bout
bash scripts/check-no-float.sh # vérifier la garantie sans virgule flottante
```

Nécessite l'outil en ligne de commande `flac` pour la sortie FLAC.

## Licence
Apache-2.0
