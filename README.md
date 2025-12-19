# RAX

RAX is a minimal, modular KVM-based hypervisor for Linux hosts. This initial version focuses on a clean architecture, correctness of the boot path, and clear extension points for additional ISAs and devices.

## Features

- KVM-based virtualization (hardware-accelerated, accurate CPU execution)
- x86_64 Linux kernel boot via the 32-bit boot protocol
- Modular ISA abstraction (`src/arch`) for future architectures
- Simple PIO bus with a 16550 UART for serial console output
- Config file support (TOML) and CLI overrides

## Requirements

- Linux host with KVM enabled (`/dev/kvm`)
- An x86_64 Linux `bzImage` kernel
- Optional initrd

## Build

```
cargo build --release
```

## Run

```
RUST_LOG=info cargo run --release -- \
  --kernel /path/to/bzImage \
  --initrd /path/to/initrd \
  --cmdline "console=ttyS0 earlycon=uart,io,0x3f8 root=/dev/ram0"
```

You can also provide a config file:

```
cargo run --release -- --config examples/config.toml
```

## Notes and current limits

- Only vCPU 0 is started; additional vCPUs are created but not run yet.
- Devices are minimal (serial only). No storage, networking, or PCI yet.
- The architecture is structured to extend additional ISAs and device models.

## Tests

```
cargo test
```
