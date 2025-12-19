# RAX

RAX is a minimal x86_64 hypervisor/emulator written in Rust that supports both KVM hardware acceleration and software emulation.

## Features

- **Dual Backend Architecture**: KVM hardware acceleration (Linux) or pure software emulation (cross-platform)
- x86_64 Linux kernel boot via the 64-bit boot protocol
- Modular ISA abstraction (`src/arch`) for future architectures
- Simple PIO bus with a 16550 UART for serial console output
- Config file support (TOML) and CLI overrides

## Requirements

### KVM Backend (Linux)
- Linux host with KVM enabled (`/dev/kvm`)
- An x86_64 Linux `bzImage` kernel
- Optional initrd

### Emulator Backend (Cross-platform)
- No special requirements - runs on any platform with Rust support

## Build

```bash
# Build with KVM support (default, Linux only)
cargo build --release

# Build without KVM support (cross-platform)
cargo build --release --no-default-features
```

## Run

```bash
# Run with KVM (default on Linux)
RUST_LOG=info cargo run --release -- \
  --kernel /path/to/bzImage \
  --initrd /path/to/initrd \
  --cmdline "console=ttyS0 earlycon=uart,io,0x3f8"

# Run with software emulator
RUST_LOG=info cargo run --release -- \
  --backend emulator \
  --kernel /path/to/bzImage \
  --initrd /path/to/initrd

# Use a config file
cargo run --release -- --config examples/config.toml
```

## Backend Comparison

| Feature | KVM | Emulator |
|---------|-----|----------|
| Platform | Linux only | Cross-platform |
| Performance | Native speed | Interpreted |
| Boot Status | Full Linux boot | Early kernel init |
| Use Case | Production VMs | Development/debugging |

## Current Status

- **KVM backend**: Fully functional, boots Linux kernels
- **Emulator backend**: Executes early kernel boot code; page table improvements needed for full boot

## Notes and Limits

- Only vCPU 0 is started; additional vCPUs are created but not run yet
- Devices are minimal (serial only). No storage, networking, or PCI yet
- The architecture is structured to extend additional ISAs and device models

## Tests

```bash
cargo test
```
