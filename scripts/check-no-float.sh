#!/usr/bin/env bash
#
# check-no-float.sh — enforce the float-free guarantee of the save path (spec §14).
#
# Checks:
#   1. Source scan of intwav-core AND intwav-engine: no float types, casts, math,
#      or decimal literals. The engine has no legitimate float need (progress is
#      integer permille, ratios are raw byte/sample counts computed GUI-side).
#   2. Disassembly scan of the compiled intwav-core object: no floating-point
#      arithmetic instructions (x86-64 SSE/x87 or aarch64 FP).
#
# The disassembly scan is core-only: the engine links the codec (which links the
# float FLAC/WAV libs), so its object cannot be cleanly disassembled — the
# source-token ban is the enforceable guarantee there. The codec, CLI, GUI, and
# playback crates are presentation/host layers and are not scanned.
#
# Exit non-zero on any violation so CI fails the build.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CORE_SRC="$REPO_ROOT/crates/intwav-core/src"
ENGINE_SRC="$REPO_ROOT/crates/intwav-engine/src"
DENYLIST="$REPO_ROOT/scripts/fp-mnemonics.txt"

fail() {
	echo "FAIL: $*" >&2
	exit 1
}

# Scan one crate's src/ for float constructs, ignoring test modules and comments.
scan_source() {
	local src_dir="$1"
	local label="$2"
	local violations=0
	local file stripped
	while IFS= read -r -d '' file; do
		# Drop `#[cfg(test)] mod tests { ... }` (to EOF) and strip line comments
		# so prose in doc comments (numbers like 6.02) is ignored.
		stripped="$(awk '
			/#\[cfg\(test\)\]/ { intest=1 }
			intest==1 { next }
			{ sub(/\/\/.*/, ""); print }
		' "$file")"

		if echo "$stripped" | grep -nE '\b(f32|f64)\b|\bas +f(32|64)\b|std::f(32|64)|\blibm\b' >/dev/null; then
			echo "  float type/cast/lib in ${file#"$REPO_ROOT"/}:" >&2
			echo "$stripped" | grep -nE '\b(f32|f64)\b|\bas +f(32|64)\b|std::f(32|64)|\blibm\b' >&2 || true
			violations=1
		fi
		if echo "$stripped" | grep -nE '[^.0-9][0-9]+\.[0-9]+' >/dev/null; then
			echo "  decimal literal in ${file#"$REPO_ROOT"/}:" >&2
			echo "$stripped" | grep -nE '[^.0-9][0-9]+\.[0-9]+' >&2 || true
			violations=1
		fi
	done < <(find "$src_dir" -name '*.rs' -print0)

	if [ "$violations" -ne 0 ]; then
		fail "$label source contains floating-point constructs"
	fi
	echo "    ok: no float types, casts, or decimal literals in $label source"
}

echo "==> [1/2] Source scan of intwav-core and intwav-engine"
scan_source "$CORE_SRC" "intwav-core"
scan_source "$ENGINE_SRC" "intwav-engine"

echo "==> [2/2] Disassembly scan of intwav-core"
echo "    building release object..."
cargo build --release -p intwav-core >/dev/null

# Newest matching rlib (contains the compiled core object).
RLIB="$(ls -t "$REPO_ROOT"/target/release/deps/libintwav_core-*.rlib 2>/dev/null | head -1 || true)"
[ -n "$RLIB" ] || fail "could not locate compiled intwav-core rlib"

OBJDUMP="${OBJDUMP:-objdump}"
command -v "$OBJDUMP" >/dev/null || fail "$OBJDUMP not found (set OBJDUMP=llvm-objdump)"

# Unique mnemonics from instruction lines (those beginning with 'address:').
# --no-show-raw-insn drops the raw byte column so the mnemonic is the first
# token after the address on both LLVM objdump (macOS) and GNU objdump (Linux).
mnemonics="$(
	"$OBJDUMP" -d --no-show-raw-insn "$RLIB" 2>/dev/null |
		grep -E '^[[:space:]]*[0-9a-fA-F]+:' |
		sed -E 's/^[[:space:]]*[0-9a-fA-F]+:[[:space:]]*//' |
		awk '{print tolower($1)}' |
		sort -u
)"

# Load denylist (strip comments/blank lines) and intersect.
deny="$(grep -vE '^[[:space:]]*(#|$)' "$DENYLIST" | tr '[:upper:]' '[:lower:]' | sort -u)"
hits="$(comm -12 <(printf '%s\n' "$mnemonics") <(printf '%s\n' "$deny") || true)"

if [ -n "$hits" ]; then
	echo "  forbidden floating-point instructions found in $(basename "$RLIB"):" >&2
	echo "$hits" | sed 's/^/    /' >&2
	fail "intwav-core object contains floating-point arithmetic instructions"
fi
echo "    ok: no floating-point instructions in core object ($(printf '%s\n' "$mnemonics" | wc -l | tr -d ' ') distinct mnemonics scanned)"

echo "PASS: intwav-core is float-free (source + disassembly)"
