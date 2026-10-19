# Generated test material

This directory contains checked-in generator output and include-only Rust data.
No file here is an independent Cargo integration-test target.

The ARM ASL corpus under `arm/{a32,a64,t16,t32,sysreg}` retains its original
module names because generated source refers to those names. Only `arm/a64`
is reachable from the current `arm` Cargo target; moving the corpus does not
activate the dormant AArch32, Thumb, or system-register suites.

`arm/oracle_cases/` contains tables included by differential runners:

- `aarch32_sweep.rs`: A32, T16, and T32 instruction encodings.
- `neon_sweep.rs`: Advanced SIMD, VFP, and FP16 encodings.
- `sve2_sweep.rs`: SVE2 and SVE2.1 encodings.

`cortex_m/thumb_oracle.json` holds the Cortex-M4 differential corpus that
`tools/cortex-m-diff/qemu_oracle.py` generates from QEMU; the library unit
tests of `src/backend/emulator/cortex_m_oracle_tests.rs` include it.

`x86_64/inventories/` contains include-only instruction inventories used by
coverage and differential runners. It includes the AVX-512 case table, the
extension-specific unimplemented-mnemonic sets, and the source-diagnostic
inventory. These files are data inputs, not test targets.

`ebpf/` contains external-oracle corpora replayed by the `ebpf_diff` target on
every eBPF execution plane: the eBPF Foundation `bpf_conformance` vectors
(`bpf_conformance.txt`), per-program digests of Linux kernel
`BPF_PROG_TEST_RUN` results for the RFC 9669 campaign in both byte orders
(`linux_kernel_le.txt`, `linux_kernel_be.txt`), both kernels' verifier
verdicts on every RFC 9669 encoding partition with each disagreement and its
reason (`linux_kernel_admission_le.txt`, `linux_kernel_admission_be.txt`), and
digests of the pinned Anza interpreter's SBPF outcomes (`anza_sbpf.txt`). Their case streams are generated
deterministically by `tests/support/ebpf_rfc_oracle.rs` and
`tests/support/ebpf_sbpf_oracle.rs`; `tools/ebpf-conformance/run.py` and
`tools/ebpf-reference/run.py` regenerate them from the live oracles.

`manifest.toml` records the provenance that can be established from tracked
files. A field set to `"unknown"` means the repository does not currently
contain enough information to reproduce it exactly.

The historical structure generator is retained as
`tools/testgen/arm_structure.py`. It is not
a complete regeneration entry point because its required `structure.json`
input is absent.
