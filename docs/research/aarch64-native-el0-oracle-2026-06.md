# AArch64 Native EL0 Oracle

Repository paths are normalized to the current layout; the historical commits
retain their original paths in Git history.

Status: implemented and green on `m9g.large` / Graviton5-class hardware. The
strict architectural comparison includes FPCR/FPSR globally, so floating-point
control and status state is now part of the normal native oracle gate rather
than a side probe.

`tests/suites/differential/arm/aarch64.rs` now prefers a native EL0 oracle on
aarch64 hosts. The test
builds `tools/arm-diff/oracle.c` with the host C compiler as
`tools/arm-diff/oracle-native`, executes test instructions directly in user
mode, and captures architectural state through signal frames. Non-aarch64 hosts
keep the existing `qemu-aarch64` fallback. Set `RAX_ARM_DIFF_FORCE_QEMU=1` to
force the fallback path.

The native oracle covers the hardware-exposed A64 EL0 ISA surface on this host:
GPRs, SP, PC, NZCV, FPCR/FPSR, V0-V31, SVE P0-P15 at VL=128, FFR, and the shared
scratch memory window. Unsupported CPU extensions are treated as out of scope for
this specific silicon only when the hardware traps and `/proc/cpuinfo` does not
advertise the required feature. Base-ISA illegal encodings still fail if rax
executes them.

Strict command:

```bash
RUSTFLAGS=-Awarnings cargo test --quiet --test arm_diff -- --nocapture
```

Current result on this host:

```text
running 679 tests
test result: ok. 679 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

The comprehensive SVE2 and NEON/VFP/FP16 generated sweeps also pass under that
strict comparison. Feature-gated encodings that this CPU does not expose, such
as FEAT_SVE_B16B16 `BFADD/BFSUB/BFMUL`, remain future coverage: native hardware
traps them here, so the harness skips only those cases after checking the host
feature flags. Nondeterministic RNDR/RNDRRS reads are covered as EL0
trap-vs-execute legality tests rather than value comparisons.

Native AArch32/Thumb EL0 execution is not available on this instance: `lscpu`
reports only 64-bit CPU op modes. `tests/suites/differential/arm/aarch32.rs`
therefore still needs
the `qemu-arm` and ARM32 cross-toolchain path, or a different machine that
exposes AArch32 EL0.

The native EL0 oracle is not a replacement for KVM or bare metal when the target
behavior requires EL1+ control: page table ownership, privileged system
registers, PAC key programming, exception-vector delivery, and hypervisor state
remain outside this EC2 guest's direct hardware oracle surface.
