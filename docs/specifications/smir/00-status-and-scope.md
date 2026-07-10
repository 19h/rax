# 00. Status and Scope

## 1. Baseline

This specification is synchronized to repository `HexRaysSA/rax`, default branch `master`, commit `7ff6953e9919916632e4c321a64a14fb1fac1f73`. The source files used are enumerated in `PROVENANCE.md`.

Paths below use the current repository layout; the baseline commit stores some
of the same files under their pre-reorganization names.

## 2. Status summary

| Area | Current status |
|---|---|
| Core IR | Implemented in `src/smir/ir/mod.rs`, `ir/types.rs`, and `ir/ops.rs`. |
| Source lifters | Implemented for x86-64, AArch64, Hexagon, RISC-V, and AVX10-visible operation families. |
| Interpreter | Implemented in `interpret.rs`; executes blocks from cache over `SmirContext` and `SmirMemory`. |
| Optimizer | Implemented in `optimize.rs`; includes O0/O1/O2, frontier-aware liveness, dead-flag elimination, constant/copy/branch transforms. |
| Lowering framework | Implemented in `lower/mod.rs`; lowerers emit machine-code bytes/words and return `LowerResult`. |
| x86-64 native backend | Implemented in `lower/x86_64/mod.rs`; used by the hot-block JIT path. |
| AArch64 native backend | Implemented in `lower/aarch64/mod.rs` and runtime trampoline support exists in `lower/runtime.rs`. Exact coverage is source-defined. |
| AArch64 guest to x86-64 host | Implemented as a state-backed scalar lowerer in `lower/cross/aarch64_guest_to_x86_64_host.rs`. |
| AVX10 lowering | Implemented as a specialized EVEX/AVX10 lowering component in `lower/x86_64/avx10.rs`. |
| Native runtime | Implemented in `lower/runtime.rs`; x86-64 and AArch64 host trampolines plus W^X / MAP_JIT executable memory handling. |
| JIT feature | `smir-jit` is a default Cargo feature. |
| Fallback | Unsupported or unproven regions must fall back to the interpreter. |

## 3. Compatibility with v0.1

The v0.1 spec is no longer sufficient because it treats JIT tiers as future and materially under-specifies:

- the large current opcode catalog;
- x86 APX/REX2 and AVX10-visible semantics;
- Hexagon packet, `.new`, HVX, and exact semantic operations;
- RISC-V FP, crypto, RVV, and CSR escape operations;
- AArch64 MTE tag-clear forms, system register access, and state-backed lowering;
- native runtime ABIs, W^X executable memory, and host I-cache maintenance;
- fail-safe JIT gating, memory helper lowering, call helpers, and native exits;
- frontier-aware optimizer liveness.

## 4. Scope

This folder specifies the architecture and source-level contract of SMIR. It does not restate every line of Rust source. Exact bit-level behavior for a complex operation remains source-defined when the Rust source intentionally ports QEMU/KVM-verified helper logic, for example RISC-V vector instructions and Hexagon FP fixups.

## 5. Non-goals

- Defining a public stable binary format for SMIR modules.
- Defining a source-compatible API independent of the Rust code.
- Replacing architecture reference manuals.
- Claiming every `OpKind` is natively lowerable on every target.
- Claiming that the integrated production hot-block dispatcher is enabled on every host that has a lowerer.

## 6. Assumptions and falsification probes

| ID | Assumption | Stress test / falsification probe |
|---|---|---|
| A1 | Source at `7ff6953e9919916632e4c321a64a14fb1fac1f73` is the intended baseline. | Re-run `compare_commits master..master` and compare SHAs in `PROVENANCE.md`. |
| A2 | Source files are newer and more precise than old markdown. | Diff `docs/specifications/smir/*.md` against `src/smir/*` and tests. |
| A3 | “Cross-target lowering” means guest ISA and host ISA may differ. | Instantiate `Aarch64X86_64Lowerer` and run through `ExecMem::run_aarch64`. |
| A4 | Unsupported native lowering is correctness-preserving by fallback. | Force unsupported ops through native path in a test and confirm rejection before execution. |
| A5 | The AArch64 runtime trampolines are implemented, but production integration coverage is source-defined. | Search callers of `run_aarch64_identity` and `run_aarch64_identity_fp` at the baseline commit. |

## 7. Quality gates for this rewrite

- Every old major topic is retained or replaced by a current source-matched topic.
- The JIT is no longer described as future.
- Cross-target lowering is described using implemented state-backed and identity-mapped strategies.
- Source/documentation drift is explicitly called out.
- Safety gates are part of the specification, not an implementation detail.
