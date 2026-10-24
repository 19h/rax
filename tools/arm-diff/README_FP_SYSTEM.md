# A32/T32 floating-point system oracle

`fp_system_oracle.py` compares RAX with a separate bare-metal program running
under `qemu-system-arm -cpu max -machine virt -accel tcg,thread=single`.
LLVM assembles both instruction states. The guest loads every D register and
FPSCR, executes one instruction, and records every D register and FPSCR.

This complements `oracle-a32.c`, whose Linux user-mode signal-frame protocol
also covers general registers and scratch memory. The system oracle requires
LLVM (`llvm-mc` and `ld.lld` in one directory), `qemu-system-arm`, the stable
Rust toolchain and the repository's cached Cargo dependencies.

From the repository root:

```sh
python3 tools/arm-diff/fp_system_oracle.py \
    --llvm-bin /path/to/llvm/bin \
    --target-dir /path/to/native/cargo-target \
    --work-dir /path/to/empty-oracle-directory
```

An omitted work directory creates a retained temporary directory. Reusing a
directory requires its oracle ownership marker. `LLVM_BIN` or the directory
containing `llvm-mc` supplies the LLVM location when `--llvm-bin` is omitted.
The RAX library is built with stable Cargo, `--locked --offline` and
`--no-default-features --features x86_64-suite,smir-jit`. Cargo's reported
library artifact supplies the standalone comparison runner.

The version 1 corpus executes 94 instruction pairs, two instruction states,
32 operand samples and 64 FPSCR control profiles:

```text
94 * 2 * 32 * 64 = 385024 state comparisons
```

It includes scalar binary16/32/64 arithmetic and integral rounding, precision
conversions, scalar half/integer and directed vector half/integer conversion,
reciprocal estimates/refinement, and vector/indexed mixed-precision FMA.
The same operation receives identical operand samples in A32 and T32.

| FPSCR field | Bits | Values |
| --- | --- | --- |
| RM | 23:22 | Nearest-even, up, down, zero |
| FZ | 24 | 0, 1 |
| DN | 25 | 0, 1 |
| AHP | 26 | 0, 1 |
| FZ16 | 19 | 0, 1 |

The first 24 operand samples rotate explicit zero, signed zero, subnormal,
normal, infinity, quiet/signaling NaN and integer-boundary encodings across
all D registers. The last eight use deterministic seeded 64-bit register
values. Samples 28 through 31 additionally exercise initial cumulative
status values 0, IOC, IDC and all arithmetic flags. Floating exception enable
bits, VFP short-vector length and stride are clear.

The binary protocol is little-endian:

| Record | Byte offset | Field | Size in bytes |
| --- | ---: | --- | ---: |
| Input | 0 | State: 0=A32, 1=T32 | 4 |
| Input | 4 | Instruction word; T32 uses first halfword in bits 31:16 | 4 |
| Input | 8 | FPSCR | 4 |
| Input | 12 | D0 through D31 | 256 |
| Result | 0 | D0 through D31 | 256 |
| Result | 256 | FPSCR | 4 |

Each input is 268 bytes and each result is 260 bytes. The complete corpus has
103186432 input bytes and 100106240 result bytes. The guest uses one writable
RAM segment at `0x40010000` inside 128 MiB of emulated RAM. Each patched ARM or
Thumb slot is followed by an interworking return. The instruction and data
caches remain in their reset state.

A missing tool, assembly error, guest timeout, unsuccessful guest completion,
incomplete output, unexpected RAX exception or any register/status mismatch
causes a nonzero exit. `report.json` records every instruction's counters;
`manifest.json` records commands, tool versions, revision, timing, source fingerprints and artifact
hashes. Source and library-artifact changes during a run fail validation. The generated source, guest ELF, input/output records, compiler logs
and mismatch details remain in the work directory.

Coverage is bounded to the selected register-only instructions and disabled
floating exception enables. General registers, guest memory behavior,
exception delivery, unlisted instructions and other architecture profiles
remain separate validation. QEMU supplies an independent software oracle;
native A64 numerical tests and the pinned Arm instruction/shared operations
provide separate evidence.

Python 3.10 or newer is required.
