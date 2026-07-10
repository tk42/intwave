# Releasing the intwav desktop app

The GUI is a Tauri v2 app. `npm run tauri dev` is **development only**. Producing
a distributable is `npm run tauri build`, which emits, per host OS:

- **macOS**: `intwav.app` and `intwav_<ver>_<arch>.dmg`
- **Windows**: `.msi` (WiX) and/or `.exe` (NSIS)
- **Linux**: `.deb` and `.AppImage`

under `src-tauri/target/release/bundle/` (e.g.
`bundle/dmg/intwav_<ver>_aarch64.dmg`, `bundle/macos/intwav.app`).

Note: `npm run build` (frontend only → `dist/`) and `cargo build` (Rust only) do
**not** produce an installer — only `npm run tauri build` does.

**Headless / CI:** the macOS `.dmg` step (`create-dmg`) styles the disk-image
window via Finder/AppleScript, which needs a desktop session. On a headless box
(SSH, CI) set `CI=true` to skip the styling and emit a plain `.dmg`:

```bash
CI=true npm run tauri build
```

On a normal macOS desktop, `npm run tauri build` produces the styled `.dmg`
directly. (GitHub Actions sets `CI=true` automatically.)

## 1. The `flac` sidecar (required, or FLAC output breaks on users' machines)

FLAC encoding shells out to the `flac` binary (this is deliberate — it keeps
libFLAC's floating-point analysis out of intwav's own process). The GUI bundles
`flac` as a **Tauri sidecar** (`bundle.externalBin` in `tauri.conf.json`), and at
startup `resolve_flac()` uses the copy next to the executable, falling back to
`flac` on `PATH` in dev.

**The bundled binary must be self-contained.** A Homebrew `flac` is dynamically
linked to `libFLAC.dylib`/`libogg.dylib` and will **not** run on a machine
without Homebrew:

```
$ otool -L flac
    /opt/homebrew/.../libFLAC.14.dylib   ← not present on users' Macs
    /opt/homebrew/.../libogg.0.dylib
```

Before building a release, place a **self-contained** binary at
`src-tauri/binaries/flac-<target-triple>` (e.g. `flac-aarch64-apple-darwin`,
`flac-x86_64-pc-windows-msvc.exe`). Options:

- Build `flac` from source statically (`./configure --disable-shared --enable-static`).
- Or bundle its dylibs into the app and rewrite load paths (`bundle.macOS.frameworks`
  + `install_name_tool`).

`scripts/prepare-flac.sh` vendors the host's `flac` for a **local** build (fine
on your dev machine); it warns if the binary is not self-contained.

## 2. Signing & notarization (required for distribution outside your machine)

`npm run tauri build` with no signing config produces an **ad-hoc-signed** app.
Gatekeeper/SmartScreen will block it on other machines. For real distribution:

- **macOS**: an Apple **Developer ID Application** certificate + **notarization**.
  Tauri reads these from env vars:
  `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`,
  `APPLE_ID`, `APPLE_PASSWORD` (app-specific), `APPLE_TEAM_ID`.
- **Windows**: a code-signing certificate (`tauri.conf.json > bundle.windows.certificateThumbprint`
  or a signing command).
- **License note**: the `flac` CLI frontend is GPLv2. We invoke it as a **separate
  process** (no linking), so it does not impose GPL on intwav — but ship its
  license text in the About box / bundle.

## 3. CI: one tag → all platforms

`.github/workflows/release.yml` uses `tauri-apps/tauri-action` to build macOS
(arm64 + x64), Windows, and Linux on a tag push and attach the bundles to a
GitHub Release. Provide the signing secrets and a per-platform self-contained
`flac` (fetch/build step) in that workflow.

## 4. Auto-update (optional, Q21)

Add `@tauri-apps/plugin-updater` + the Rust `tauri-plugin-updater`, host a signed
`latest.json`, and the app can self-update. Archival users value stability, but a
signed update channel is how you ship correctness/security fixes.
