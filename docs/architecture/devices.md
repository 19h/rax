[← Documentation home](../../README.md)

# Devices and platform wiring

Device documentation in `rax` must distinguish four levels:

1. **Model present:** source code implements registers or behavior.
2. **Bus reachable:** an I/O or MMIO bus can route accesses to it.
3. **Machine wired:** a selected machine instantiates it at a guest-visible address/IRQ.
4. **Guest exercised:** a named workload or test enumerates, binds, or performs I/O through it.

A source file alone establishes only the first level.

## x86 baseline platform

The maintained PC machine includes the conventional serial-oriented platform needed by the project’s direct Linux and legacy boot work. The high-level inventory includes:

- 16550-compatible serial controller;
- 8254 PIT;
- 8259 PIC;
- local APIC and I/O APIC;
- RTC/CMOS;
- 8237 DMA controller;
- i8042 keyboard controller;
- primary and secondary IDE paths;
- floppy controller work;
- system-control ports;
- QEMU-style `fw_cfg`;
- Bochs-style debug port;
- ATAPI CD support for the ISO path.

The serial console is interrupt-driven in the documented path so host keyboard input can reach the guest. The terminal front end uses a QEMU-style `Ctrl-A` multiplexer and restores host termios state on ordinary and guarded failure paths.

## PCI host bridge and optional devices

The source contains a PCI host bridge with configuration access, BAR assignment/routing, and an MMIO aperture. The aggregate `--pci-devices` option (or `pci_devices = true`) attaches the optional device set for the supported PC path.

| Class | Model | Current documented milestone |
|---|---|---|
| Network | Intel e1000 / 82540EM-oriented model | Linux enumeration and `eth0` bring-up; Microwire EEPROM interaction through EECD is represented |
| Storage | AHCI | controller enumeration/binding and SATA link-state behavior represented |
| Storage | NVMe | PCI endpoint/controller enumeration and command-path work represented |
| Storage | IDE | baseline IDE path, also used by legacy machine work |
| USB | UHCI | PCI endpoint/controller enumeration represented |
| Audio | AC97 | PCI endpoint enumeration represented |

“Enumerates” is not the same as sustained data-path validation. A documentation claim about networking, disk I/O, USB transfers, or audio playback should name the workload and result.

## Opt-in behavior

The optional PCI set is not attached by default. This preserves the baseline machine layout and avoids routing every memory access through a populated MMIO bridge when the device set is irrelevant.

A run using `--pci-devices` should record that flag in bug reports and benchmarks because it changes the machine’s address-space and polling behavior.

## Interrupts

The legacy and APIC interrupt models route timer, serial, and device events to the guest. Optional PCI interrupt delivery is currently described as limited/polled rather than a production-quality full interrupt architecture.

When debugging a device, separate:

- register model correctness;
- IRQ line assertion/deassertion;
- PIC/IOAPIC/LAPIC routing;
- guest mask/acknowledgment behavior;
- backend injection behavior;
- VM polling cadence.

A device can have correct registers and still fail because the interrupt path is wrong.

## VGA boundary

VGA is not wired into the maintained PC machine. The legacy `0xa0000` aperture conflicts with assumptions in the flat RAM model, and the project’s primary interaction surface is serial. Do not advertise graphical output or infer VGA support from any dormant/model code.

## AArch64 virtual platform devices

The generated-DTB AArch64 Linux machine exposes:

- PL011 UART;
- GICv3 distributor and redistributor;
- ICC system-register interface;
- Arm generic timer;
- PSCI;
- RAM description.

The DTB is part of the guest-visible device contract. Changes to addresses, interrupt specifiers, compatible strings, or CPU topology must be validated against both software and HVF machine paths.

## 32-bit Arm and SoC devices

Selected Arm machine work includes platform-specific interrupt, timer, GPIO, storage, DMA, display, touch, and serial models. The S5L8900 research path is the broadest named SoC example, with models for:

- dual PL192-style interrupt controllers;
- system controller/GPIO;
- timers;
- I²C and attached PMU/RTC/accelerometer-like devices;
- UART;
- AES engine;
- NAND plus ECC;
- PL080-style DMA and Apple data-mover work;
- SPI-attached panel/touch devices;
- LCD controller;
- USB OTG;
- NOR flash.

These devices belong to that machine’s research path. They are not implied by `--arch armv7a`, `cortex_m`, or `cortex_r` generally.

## RISC-V and Hexagon bare-metal devices

The bare-metal machines intentionally provide a small platform:

- RAM and ELF loading;
- UART output/input appropriate to the machine;
- halt/environment control.

They do not claim a full standard board, PCI subsystem, storage stack, or production interrupt environment.

## Device state and checkpoints

A whole-machine checkpoint must serialize every instantiated device’s mutable guest-visible state, including enough timing and queue state to resume consistently. Adding a device requires:

- stable serialization/version handling;
- restore-time reconstruction;
- address/IRQ reattachment;
- tests that mutate state before save and verify behavior after restore;
- failure behavior for incompatible snapshots.

A model that cannot be serialized either prevents complete checkpoints or must be explicitly excluded from the supported checkpoint configuration.

## Device validation ladder

Use the narrowest claim supported by the highest completed rung:

1. register unit tests;
2. bus/address-routing tests;
3. IRQ assertion and routing tests;
4. machine construction tests;
5. guest driver enumeration;
6. functional guest I/O;
7. save/restore during active use;
8. comparison with hardware/specification traces where practical.

The root README should normally mention only guest-visible milestones. Register inventories and implementation notes belong here or in device-specific development reports.
