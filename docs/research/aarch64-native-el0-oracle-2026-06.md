# AArch64 Native EL0 Oracle

Status: implemented on `m9g.large` / Graviton5-class hardware. The strict
architectural comparison now includes FPCR/FPSR globally; that stronger gate is
still exposing remaining SIMD/SVE floating-point status-flag gaps in rax.

`tests/arm_diff.rs` now prefers a native EL0 oracle on aarch64 hosts. The test
builds `tools/arm-diff/oracle.c` with the host C compiler as
`tools/arm-diff/oracle-native`, executes test instructions directly in user
mode, and captures architectural state through signal frames. Non-aarch64 hosts
keep the existing `qemu-aarch64` fallback. Set `RAX_ARM_DIFF_FORCE_QEMU=1` to
force the fallback path.

The native oracle covers the hardware-exposed A64 EL0 ISA surface on this host:
GPRs, SP, PC, NZCV, FPCR/FPSR, V0-V31, SVE P0-P15 at VL=128, and the shared
scratch memory window. Unsupported CPU extensions are treated as out of scope
for this specific silicon only when the hardware traps and `/proc/cpuinfo`
does not advertise the required feature. Base-ISA illegal encodings still fail
if rax executes them.

Strict command:

```bash
RUSTFLAGS=-Awarnings cargo test --quiet --test arm_diff -- --nocapture
```

Current result on this host after enabling global FPCR/FPSR comparison:

```text
running 220 tests
SVE2 SWEEP PROBE: 879 mnemonics, 5274 cases, 0 gaps, 0 value-mismatch, 0 fault-disagree
NEON SWEEP PROBE: 3761 mnemonics, 63024 cases, 0 gaps, 0 value-mismatch, 0 fault-disagree
test result: FAILED. 168 passed; 52 failed
```

The failures are currently FPSR/QC side-effect mismatches in FP-heavy
SIMD/BF16/FP16/SVE families. Scalar FP finite arithmetic, scalar FP exception
flags, scalar FSQRT inexact/invalid, and scalar FCCMP/FCMPE invalid-operand
status have focused native-oracle coverage and pass under the stricter
comparison.

Native AArch32/Thumb EL0 execution is not available on this instance: `lscpu`
reports only 64-bit CPU op modes. `tests/arm_diff32.rs` therefore still needs
the `qemu-arm` and ARM32 cross-toolchain path, or a different machine that
exposes AArch32 EL0.
