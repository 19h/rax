# Rax Emulator Makefile
#
# Targets:
#   build          - Build rax library and binaries (release)
#   build-debug    - Build rax in debug mode
#   test           - Run all tests including x86_64-suite
#   test-quick     - Run tests without x86_64-suite
#   microkernel    - Build the bare-metal microkernel
#   run-microkernel- Build and run the microkernel in the emulator
#   run-linux      - Run a Linux kernel in the emulator (configure linux/config.toml)
#   clean          - Clean all build artifacts
#   help           - Show this help message

.PHONY: all build build-debug test test-quick microkernel run-microkernel run-linux clean help

# Default target
all: build

# Build rax in release mode
build:
	cargo build --release

# Build rax in debug mode
build-debug:
	cargo build

# Run all tests including x86_64-suite (comprehensive)
test:
	cargo test --release --features x86_64-suite

# Run tests without the full x86_64 instruction suite (faster)
test-quick:
	cargo test --release

# Build the bare-metal microkernel
microkernel: microkernel/microkernel.bin

microkernel/microkernel.bin: microkernel/src/main.rs microkernel/Cargo.toml microkernel/linker.ld
	cd microkernel && cargo +nightly build --release
	llvm-objcopy -O binary microkernel/target/x86_64-unknown-none/release/microkernel microkernel/microkernel.bin
	@echo "Built microkernel/microkernel.bin ($$(stat -c%s microkernel/microkernel.bin 2>/dev/null || stat -f%z microkernel/microkernel.bin) bytes)"

# Build and run the microkernel in the emulator
run-microkernel: microkernel
	cargo run --release --no-default-features --example run_microkernel

# Run a Linux kernel (requires linux/config.toml to be configured)
run-linux: build
	@if grep -q '/path/to/bzImage' linux/config.toml 2>/dev/null; then \
		echo "Error: linux/config.toml still contains placeholder paths."; \
		echo ""; \
		echo "To run a Linux kernel, edit linux/config.toml and set:"; \
		echo "  kernel = \"/path/to/your/bzImage\""; \
		echo "  initrd = \"/path/to/your/initrd\"  (optional)"; \
		echo ""; \
		echo "You can build a minimal kernel with:"; \
		echo "  git clone --depth 1 https://github.com/torvalds/linux"; \
		echo "  cd linux && make tinyconfig && make -j\$$(nproc)"; \
		exit 1; \
	fi
	cargo run --release -- --config linux/config.toml

# Clean all build artifacts
clean:
	cargo clean
	cd microkernel && cargo clean
	rm -f microkernel/microkernel.bin

# Show help
help:
	@echo "Rax Emulator - Available targets:"
	@echo ""
	@echo "  make build           - Build rax library (release mode)"
	@echo "  make build-debug     - Build rax library (debug mode)"
	@echo "  make test            - Run all tests including x86_64-suite"
	@echo "  make test-quick      - Run tests without x86_64-suite (faster)"
	@echo "  make microkernel     - Build the bare-metal microkernel binary"
	@echo "  make run-microkernel - Build and run microkernel in emulator"
	@echo "  make run-linux       - Run a Linux kernel (edit linux/config.toml first)"
	@echo "  make clean           - Clean all build artifacts"
	@echo "  make help            - Show this help message"
	@echo ""
	@echo "Examples:"
	@echo "  make build test      - Build and run all tests"
	@echo "  make run-microkernel - Quick demo of the emulator"
	@echo "  make run-linux       - Run Linux (after configuring linux/config.toml)"
