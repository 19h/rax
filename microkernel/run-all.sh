#!/usr/bin/env bash
# Build (if needed) and run the microkernel on all three architectures under the
# rax emulator, asserting each prints the RESULT PASS sentinel and that the
# n-body checksum is identical across architectures (cross-arch determinism).
#
# Env:
#   RAX_BIN       path to a prebuilt rax binary (default: build target/debug/rax)
#   FORCE_BUILD=1 rebuild the kernel binaries even if present
#   MEM           guest memory size (default 128M)
set -euo pipefail

cd "$(dirname "$0")/.."
ROOT="$PWD"
MK="$ROOT/microkernel"
DTB="$MK/dtb/s3c6410-smdk6410.dtb"
MEM="${MEM:-128M}"

TIMEOUT="$(command -v timeout || command -v gtimeout || true)"
run_to() { if [ -n "$TIMEOUT" ]; then "$TIMEOUT" 120 "$@"; else "$@"; fi; }

RAX_BIN="${RAX_BIN:-}"
if [ -z "$RAX_BIN" ]; then
    echo "[run-all] building rax (emulator backend)…" >&2
    cargo build --no-default-features --bin rax >&2
    RAX_BIN="$ROOT/target/debug/rax"
fi

for a in x86_64 aarch64 armv6; do
    if [ ! -f "$MK/microkernel-$a.bin" ] || [ "${FORCE_BUILD:-0}" = 1 ]; then
        "$MK/build.sh" "$a" >&2
    fi
done

# run <label> <rax-arch> [extra args...] -> prints NBODY_CKSUM on stdout
run() {
    local label="$1" rarch="$2"
    shift 2
    local bin="$MK/microkernel-$label.bin" out rc=0
    echo "================ $label ($rarch) ================" >&2
    out="$(run_to "$RAX_BIN" --backend emulator --arch "$rarch" --memory "$MEM" \
        --kernel "$bin" "$@" 2>&1)" || rc=$?
    echo "$out" | grep -E 'RAX-MK |NBODY_CKSUM|\[FAIL\]' >&2 || true
    if [ "$rc" -ne 0 ]; then
        echo "[run-all] $label: emulator exited $rc" >&2
        return 1
    fi
    if ! echo "$out" | grep -q 'RAX-MK: RESULT PASS'; then
        echo "[run-all] $label: missing RESULT PASS sentinel" >&2
        return 1
    fi
    if echo "$out" | grep -q 'RAX-MK: RESULT FAIL'; then
        echo "[run-all] $label: RESULT FAIL" >&2
        return 1
    fi
    echo "$out" | grep -oE 'NBODY_CKSUM=0x[0-9a-f]+' | head -1
}

c_x86="$(run x86_64 x86-64)"
c_a64="$(run aarch64 aarch64)"
c_a32="$(run armv6 armv7a --dtb "$DTB")"

echo "[run-all] checksums: x86_64=$c_x86 aarch64=$c_a64 armv6=$c_a32" >&2
if [ "$c_x86" != "$c_a64" ] || [ "$c_a64" != "$c_a32" ]; then
    echo "[run-all] CROSS-ARCH CHECKSUM MISMATCH" >&2
    exit 1
fi
echo "[run-all] PASS: all three architectures green and deterministic" >&2
