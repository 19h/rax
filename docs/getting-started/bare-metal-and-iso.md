[← Documentation home](../../README.md)

# Bare-metal programs and bootable ISO images

`rax` has runnable paths that do not use the direct Linux loaders. RISC-V and Hexagon currently run bare-metal programs; the microkernel supplies multi-architecture integration workloads; and the x86 PC machine contains a legacy real-mode/El Torito route for bootable ISO media. These paths have different loaders, machine state, stop conditions, and validation expectations.

## General bare-metal workflow

A bare-metal program must provide enough information for the selected loader to place code, choose an entry point, and construct initial architectural state. Prefer a self-describing ELF where the machine supports one.

A useful workflow is:

1. Build a tiny program that writes a fixed marker to the machine’s UART or semihosting-like exit path.
2. Confirm the marker under the software backend without the JIT.
3. Confirm the expected halt or exit reason.
4. Add exception, memory, vector, or device behavior incrementally.
5. Compare final state or output with an external reference where a differential harness exists.

Record the ELF machine type, entry point, loadable segments, endianness, and linker script. A command reaching a clean process exit is not enough if the test did not observe the expected marker or state.

## RISC-V RV64 bare-metal

Launch an RV64 ELF through the software machine:

```sh
cargo build --release --no-default-features

./target/release/rax \
    --arch riscv64 \
    --backend emulator \
    --kernel program.elf
```

The current RISC-V machine is a small bare-metal platform with guest RAM, UART-visible output, and a halt/termination path. It is intended for programs and test payloads rather than a privileged Linux kernel.

Current boundary:

- scalar RV64, floating-point, compressed, atomic, bit-manipulation, crypto, and vector semantics have implementation and test surfaces;
- the runnable machine does not provide a complete privileged architecture, supervisor-mode platform, or Sv39 MMU suitable for Linux;
- QEMU differential tests compare selected scalar and vector state for cases that execute;
- SMIR lift and host-native tests cover selected RISC-V regions, not every machine interaction.

Use the registered targets rather than guessing test names:

```sh
cargo test --release --test riscv_boot
cargo test --release --test riscv_diff
cargo test --release --test riscv_vector
cargo test --release --test riscv_smir_lift
```

Host-specific JIT targets are:

```sh
cargo test --release --test riscv_smir_x86_jit
cargo test --release --test riscv_smir_aarch64_jit
```

A target can compile and self-gate without executing the desired host path. Read test counts and skip output.

## Qualcomm Hexagon bare-metal

Launch a Hexagon ELF:

```sh
cargo build --release --no-default-features

./target/release/rax \
    --arch hexagon \
    --backend emulator \
    --kernel program.elf \
    --hexagon-isa v68
```

The current public ISA selector accepts:

```text
v4, v5, v55, v60, v62, v65, v66, v67, v68, v69
```

and defaults to `v68`. Claims about later revisions in comments, historical reports, or old prose do not extend this public selector. Resolve that discrepancy in code and tests before advertising a later profile.

Additional controls:

```sh
--hexagon-endian little
--hexagon-entry 0x10000
--hexagon-load-addr 0x10000
```

Prefer the ELF’s own entry and program headers. Use explicit entry/load overrides for flat or intentionally relocated inputs, and document why the override is required. Endianness is part of the execution contract; do not assume that changing a flag makes an arbitrary image valid in the other byte order.

Hexagon execution is packet-aware. A valid test must consider:

- packet boundaries and commit behavior;
- `.new` forwarding;
- predicates and control transfer;
- loop state;
- scalar and vector register state;
- memory effects and exceptions;
- HVX width and lane interpretation where applicable.

Registered differential and integration targets include:

```sh
cargo test --release --test hexagon_bare_metal
cargo test --release --test hexagon_diff
cargo test --release --test hexagon_cf_diff
cargo test --release --test hexagon_float_diff
cargo test --release --test hexagon_mem_diff
cargo test --release --test hexagon_hvx_diff
cargo test --release --test hexagon_hvx_mem_diff
cargo test --release --test hexagon_smir_lift
```

Those names describe separate evidence domains. A scalar QEMU comparison does not establish HVX memory correctness, and an HVX arithmetic comparison does not establish packet-level control flow.

## Multi-architecture microkernel

`microkernel/` is an independent freestanding package used to exercise more than one instruction in isolation. It builds bare-metal workloads for x86-64, AArch64, and ARMv6 and a hosted x86-64 variant for reference work.

```sh
cd microkernel
make
make run
```

The runner expects each guest to emit a pass marker and compares the N-body checksum across the supported builds. This provides integration evidence across:

- startup and linker state;
- stack and memory behavior;
- allocator use;
- control flow and arithmetic;
- UART/console output;
- a nontrivial deterministic workload.

It does not prove that the three architecture implementations are equivalent in general. The same checksum can coexist with untested exception, vector, memory-ordering, or device bugs.

The root repository also registers:

```sh
cargo test --release --test microkernel_multiarch
```

See [Microkernel harness](../development/microkernel.md) for toolchain requirements, environment variables, result markers, and Intel SDE cross-checking.

## x86 bootable ISO path

The x86 PC machine includes a legacy path that begins in real mode, uses a small BIOS implementation, discovers boot media through El Torito, and accesses CD content through the emulated IDE/ATAPI route. A typical launch is:

```sh
cargo build --release --no-default-features

./target/release/rax \
    --arch x86-64 \
    --backend emulator \
    --kernel /path/to/bootable.iso \
    --memory 512M
```

Historical project demonstrations include TempleOS V5.03 progressing from real mode into its 64-bit environment and accessing its CD. Treat that as a named image/configuration result, not a general guarantee that every BIOS-bootable ISO or PC operating system will boot.

The ISO route exercises a different integration surface than direct Linux loading:

- reset-vector and real-mode execution;
- BIOS interrupt behavior;
- mode transitions;
- El Torito catalog handling;
- IDE/ATAPI commands;
- platform interrupts and timers;
- guest expectations about display and input hardware.

The current platform is serial-oriented and does not provide a generally usable VGA console. An ISO that requires graphical output, a specific keyboard controller behavior, or unimplemented BIOS services may appear to hang despite continued execution.

The registered machine-level test for the real-mode path is:

```sh
cargo test --release --test realmode_boot
```

That test’s observed milestone defines its scope. It should not be summarized as universal BIOS compatibility.

## 32-bit Arm and machine-specific images

The repository contains AArch32, Thumb, Cortex-M, Cortex-R, S3C64xx/S3C6410, and S5L8900 implementation work. These are not one generic `--arch arm` machine.

Public architecture families include:

```text
armv7a
armv8a32
cortex-m
cortex-r
```

The public CLI currently exposes only the architecture family and `--dtb`; finer Arm ISA selectors and load controls are available through TOML configuration. Some machine routes are selected by implementation-specific policy or environment, including the documented `RAX_MACHINE=s5l8900` path.

Before publishing a launch command for one of these machines, state:

- the exact `arch` and ISA profile;
- the machine model;
- image format and load address;
- entry state (ARM or Thumb, privilege level, vector base);
- external or generated DTB;
- expected UART or boot milestone;
- whether the result is source-presence, unit-test, machine-test, or manual boot evidence.

The broad AArch32 semantic implementation must not be converted into a claim that a 32-bit Linux guest reaches a shell. That milestone is not currently established by the root project documentation.

## Choosing a stop condition

Bare-metal programs often terminate differently from hosted processes. Depending on the machine, completion may be represented by:

- a guest `HLT`, `WFI`, or architecture-specific halt;
- a write to a platform exit register;
- a UART pass/fail marker followed by a spin loop;
- a bounded instruction count in a test harness;
- an emulation exception deliberately interpreted by the harness.

Define the success condition before running the program. “The emulator stopped” is ambiguous without the final exit reason and expected output.

## Debugging a bare-metal image

Start without the JIT:

```sh
cargo build --release --no-default-features --features trace,debug
```

Then use one of:

```sh
./target/release/rax ... --trace baremetal.trace
./target/release/rax ... --gdb 1234 --wait-gdb
```

For an unexplained first-instruction failure, inspect:

```sh
readelf -h program.elf
readelf -l program.elf
objdump -d program.elf | head -100
```

Verify:

- ELF class and machine;
- byte order;
- entry point;
- loadable segment addresses and permissions;
- alignment;
- instruction-set mode at entry;
- stack and reset-state assumptions;
- whether the program expects devices absent from the selected machine.

## Evidence checklist

For a bare-metal or ISO result, record:

```text
rax commit:
host and build features:
selected architecture/backend/machine:
image path and SHA-256:
ELF/ISO metadata:
ISA revision and endianness:
entry and load address overrides:
memory:
expected output/exit:
actual output/exit:
test target or command:
external oracle and version, if used:
number of cases actually executed:
```

## Related pages

- [Linux guests](linux-guests.md)
- [Machines and boot](../architecture/machines.md)
- [Hexagon architecture](../architecture/hexagon/README.md)
- [RISC-V architecture](../architecture/riscv/README.md)
- [Arm architecture](../architecture/arm/README.md)
- [Verification model](../development/verification.md)
