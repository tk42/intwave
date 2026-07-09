#!/usr/bin/env bash
#
# prepare-flac.sh — vendor a `flac` sidecar for `tauri build` into
# src-tauri/binaries/flac-<target-triple>.
#
# Usage:
#   scripts/prepare-flac.sh                 # copy the host's `flac` (dev/local)
#   FLAC_BIN=/path/to/static/flac scripts/prepare-flac.sh   # use a specific binary
#
# For a real release the vendored binary MUST be self-contained (statically
# linked, or with its dylibs bundled). A Homebrew `flac` is dynamically linked
# and will not run on machines without those libraries — this script warns.

set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
dest_dir="$here/../src-tauri/binaries"
mkdir -p "$dest_dir"

triple="$(rustc -vV | sed -n 's/host: //p')"
src="${FLAC_BIN:-$(command -v flac || true)}"
[ -n "$src" ] || { echo "error: no flac binary found (set FLAC_BIN)"; exit 1; }

ext=""
case "$triple" in *windows*) ext=".exe" ;; esac
dest="$dest_dir/flac-$triple$ext"
cp "$src" "$dest"
chmod +x "$dest"
echo "vendored $src -> $dest"

# Self-containedness check (best effort, macOS/Linux).
if command -v otool >/dev/null 2>&1; then
	if otool -L "$dest" | grep -qE '/opt/homebrew|/usr/local/opt|Cellar'; then
		echo "WARNING: $dest links Homebrew/local dylibs — NOT self-contained." >&2
		echo "         It runs on this machine only. Provide a static flac for release." >&2
	fi
elif command -v ldd >/dev/null 2>&1; then
	if ldd "$dest" 2>/dev/null | grep -qE 'libFLAC|libogg'; then
		echo "WARNING: $dest dynamically links libFLAC/libogg — provide a static build for release." >&2
	fi
fi
