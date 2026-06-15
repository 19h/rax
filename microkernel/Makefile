# Multi-architecture bare-metal microkernel.
#
#   make            build all three bare-metal kernels (x86_64, aarch64, armv6)
#   make x86_64     build just the x86_64 kernel  -> microkernel-x86_64.bin
#   make aarch64    build just the aarch64 kernel -> microkernel-aarch64.bin
#   make armv6      build just the armv6 kernel   -> microkernel-armv6.bin
#   make run        build + boot all three under rax and assert RESULT PASS
#   make usermode   build the x86_64 usermode binary (for Intel SDE)
#   make test-sde   run the usermode binary under Intel SDE
#   make clean      remove build artifacts
#
# Bare-metal builds need a nightly toolchain with the rust-src component (used by
# build-std) and llvm-objcopy on PATH.

.PHONY: all x86_64 aarch64 armv6 run usermode test-sde test-native clean

all:
	./build.sh all

x86_64:
	./build.sh x86_64
aarch64:
	./build.sh aarch64
armv6:
	./build.sh armv6

# Boot all three under rax and check the result sentinels + cross-arch checksum.
run:
	./run-all.sh

# ---------------------------------------------------------------------------
# Usermode (Intel SDE) build: the same suite as a hosted x86_64 Linux binary.
# ---------------------------------------------------------------------------
usermode: target/x86_64-unknown-linux-gnu/release/microkernel

target/x86_64-unknown-linux-gnu/release/microkernel: src/*.rs Cargo.toml
	cargo +nightly build --release --features usermode --target x86_64-unknown-linux-gnu

test-native: usermode
	./target/x86_64-unknown-linux-gnu/release/microkernel

test-sde: usermode
	@if command -v sde64 >/dev/null 2>&1; then \
		sde64 -- ./target/x86_64-unknown-linux-gnu/release/microkernel; \
	elif [ -n "$$SDE_PATH" ]; then \
		$$SDE_PATH/sde64 -- ./target/x86_64-unknown-linux-gnu/release/microkernel; \
	else \
		echo "Error: Intel SDE not found. Set SDE_PATH or add sde64 to PATH"; \
		exit 1; \
	fi

clean:
	cargo clean
	rm -f microkernel-x86_64.bin microkernel-aarch64.bin microkernel-armv6.bin
