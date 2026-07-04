#!/usr/bin/env bash
#
# check-no-float.sh — enforce the float-free guarantee of intwav-core (spec §14).
#
# Two independent checks:
#   1. Source scan: intwav-core must contain no float types, casts, or math.
#   2. Disassembly scan: the compiled intwav-core object must contain no
#      floating-point arithmetic instructions (x86-64 SSE/x87 or aarch64 FP).
#
# Scope is intentionally intwav-core ONLY. The codec (WAV/FLAC) and CLI crates
# may legitimately touch float via dependencies; FLAC encoding is delegated to
# an out-of-process `flac` binary precisely so the core stays clean.
#
# Exit non-zero on any violation so CI fails the build.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CORE_SRC="$REPO_ROOT/crates/intwav-core/src"
DENYLIST="$REPO_ROOT/scripts/fp-mnemonics.txt"

fail() {
	echo "FAIL: $*" >&2
	exit 1
}

echo "==> [1/2] Source scan of intwav-core"
# Exclude test modules: test code is allowed to reference f64 for reference
# calculations and never ships in the scanned object. We scan whole files but
# tolerate matches inside `#[cfg(test)]` by scanning only non-test lines is hard
# in pure grep; instead we forbid float constructs everywhere EXCEPT lines that
# are clearly test-only. Simplest robust rule: forbid in all .rs files but allow
# the dedicated test helper by scanning files and skipping `mod tests` blocks.
#
# Pragmatic approach: grep for float tokens; if any hit is outside a test
# module, fail. We implement this by stripping test modules first.
violations=0
while IFS= read -r -d '' file; do
	# Remove `#[cfg(test)] mod tests { ... }` blocks (to end of file) before scan.
	# Drop `#[cfg(test)] mod tests { ... }` (to EOF) and strip line comments so
	# prose in doc comments (which may mention numbers like 6.02) is ignored.
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
	# Decimal literals (e.g. 0.5, 1.0) — a float would need one.
	if echo "$stripped" | grep -nE '[^.0-9][0-9]+\.[0-9]+' >/dev/null; then
		echo "  decimal literal in ${file#"$REPO_ROOT"/}:" >&2
		echo "$stripped" | grep -nE '[^.0-9][0-9]+\.[0-9]+' >&2 || true
		violations=1
	fi
done < <(find "$CORE_SRC" -name '*.rs' -print0)

if [ "$violations" -ne 0 ]; then
	fail "intwav-core source contains floating-point constructs"
fi
echo "    ok: no float types, casts, or decimal literals in core source"

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
