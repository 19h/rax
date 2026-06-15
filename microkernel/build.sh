#!/usr/bin/env bash
# Build the bare-metal microkernel for one (or all) architectures and emit a
# flat binary `microkernel-<arch>.bin` next to this script.
#
#   ./build.sh x86_64 | aarch64 | armv6 | all
#
# Requires a nightly toolchain with the rust-src component (for build-std) and
# llvm-objcopy on PATH. The aarch64 target is installed on demand.
set -euo pipefail

cd "$(dirname "$0")"

OBJCOPY="${OBJCOPY:-$(command -v llvm-objcopy || command -v objcopy)}"
if [ -z "${OBJCOPY:-}" ]; then
    echo "error: need llvm-objcopy or objcopy on PATH" >&2
    exit 1
fi

# Build core (and compiler-builtins with its memory intrinsics) from source for
# every bare-metal target: the prebuilt cores for *-none targets omit the
# memcpy/memset/bcmp symbols the test suite emits.
BUILDSTD="-Z build-std=core,compiler_builtins -Z build-std-features=compiler-builtins-mem"

build_one() {
    local arch="$1" target out
    case "$arch" in
        x86_64)
            target="x86_64-unknown-none"
            out="target/$target/release/microkernel"
            cargo +nightly build --release --target "$target" $BUILDSTD
            ;;
        aarch64)
            target="aarch64-unknown-none-softfloat"
            out="target/$target/release/microkernel"
            rustup target add "$target" >/dev/null 2>&1 || true
            cargo +nightly build --release --target "$target" $BUILDSTD
            ;;
        armv6)
            target="./armv6-rax-none-eabi.json"
            out="target/armv6-rax-none-eabi/release/microkernel"
            cargo +nightly build --release --target "$target" \
                $BUILDSTD -Z unstable-options -Z json-target-spec
            ;;
        *)
            echo "unknown arch: $arch (want x86_64|aarch64|armv6|all)" >&2
            exit 1
            ;;
    esac
    "$OBJCOPY" -O binary "$out" "microkernel-$arch.bin"
    local size
    size=$(stat -c%s "microkernel-$arch.bin" 2>/dev/null || stat -f%z "microkernel-$arch.bin")
    echo "built microkernel-$arch.bin ($size bytes)"
}

if [ "${1:-all}" = "all" ]; then
    for a in x86_64 aarch64 armv6; do build_one "$a"; done
else
    build_one "$1"
fi
