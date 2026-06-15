#!/usr/bin/env bash
# Profile-guided optimization (PGO) build for rax.
#
# Instruments the build, trains on representative interpreter workloads (the
# register- and memory-bound bench loops + the microkernel), merges the profile,
# and rebuilds with the profile applied. The giant opcode-dispatch matches are an
# ideal PGO target: ~+20% interpreter throughput over a plain release build.
#
# Output: target/release/rax (PGO-optimized, target-cpu=native by default).
# Override the ISA for a portable build:  PGO_TARGET_CPU=x86-64-v3 make pgo
set -euo pipefail
cd "$(dirname "$0")/.."

TARGET_CPU="${PGO_TARGET_CPU:-native}"
TMP_PARENT="${PGO_TMPDIR:-${TMPDIR:-/tmp}}"
TMP_PARENT="${TMP_PARENT%/}"
if [ -z "$TMP_PARENT" ]; then
  TMP_PARENT="/"
fi

check_private_dir() {
  local dir="$1"
  local mode

  if [ ! -d "$dir" ] || [ ! -O "$dir" ]; then
    echo "error: PGO work directory is not owned by the current user: $dir" >&2
    exit 1
  fi

  if mode="$(stat -f '%Lp' "$dir" 2>/dev/null)"; then
    :
  elif mode="$(stat -c '%a' "$dir" 2>/dev/null)"; then
    :
  else
    echo "error: unable to verify PGO work directory permissions: $dir" >&2
    exit 1
  fi

  if [ "$mode" != "700" ]; then
    echo "error: PGO work directory must be mode 700, got $mode: $dir" >&2
    exit 1
  fi
}

WORKDIR="$(mktemp -d "$TMP_PARENT/rax-pgo.XXXXXX")"
cleanup() {
  rm -rf -- "$WORKDIR"
}
trap cleanup EXIT

chmod 700 "$WORKDIR"
check_private_dir "$WORKDIR"

PROFILE_DIR="$WORKDIR/raw"
PROFILE_DATA="$WORKDIR/merged.profdata"
mkdir -m 700 "$PROFILE_DIR"
check_private_dir "$PROFILE_DIR"

# Locate llvm-profdata: PATH first, then the rustup llvm-tools component.
PROFDATA="$(command -v llvm-profdata || true)"
if [ -z "$PROFDATA" ]; then
  PROFDATA="$(find "$(rustc --print sysroot)" -name 'llvm-profdata*' 2>/dev/null | head -1)"
fi
if [ -z "$PROFDATA" ]; then
  echo "error: llvm-profdata not found. Install with:" >&2
  echo "       rustup component add llvm-tools-preview" >&2
  exit 1
fi

echo "[pgo] 1/4 instrumented build (target-cpu=$TARGET_CPU)"
RUSTFLAGS="-Cprofile-generate=$PROFILE_DIR -C target-cpu=$TARGET_CPU" \
  cargo build --release --examples

echo "[pgo] 2/4 training run (representative workloads)"
./target/release/examples/bench_loop 0x2000000 >/dev/null 2>&1 || true
./target/release/examples/bench_mem 0x1000000 >/dev/null 2>&1 || true
./target/release/examples/run_microkernel >/dev/null 2>&1 || true

echo "[pgo] 3/4 merging profile data"
"$PROFDATA" merge -o "$PROFILE_DATA" "$PROFILE_DIR"

echo "[pgo] 4/4 optimized rebuild"
RUSTFLAGS="-Cprofile-use=$PROFILE_DATA -Cllvm-args=-pgo-warn-missing-function=0 -C target-cpu=$TARGET_CPU" \
  cargo build --release

echo "[pgo] done -> target/release/rax  (PGO, target-cpu=$TARGET_CPU)"
