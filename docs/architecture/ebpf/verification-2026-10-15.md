# eBPF / SBPF semantic verification, 2026-10-15

Baseline: `7951b3b29d1f011999b32de797e7d12dee8501b8` (implementation introduced
at `b6e098307b48d1ef5fb9e1e8f3838a5190a51776`). No tracked baseline changes;
pre-existing untracked content is outside this task's ownership.

## Acceptance criteria and source authority

Verify all nine external-review findings, correct reproducible defects in direct
and SMIR execution, audit adjoining encoding/control-flow/memory contracts, and
commit isolated semantic groups with exact owned paths. This is bounded semantic
verification, not a proof of full ISA conformance. Native eBPF JIT admission is
outside the supported implementation; it must remain closed.

Primary authorities: RFC 9669 sections 3-5 in
`docs/specifications/ebpf/rfc9669.txt`; Anza SBPF reference `program.rs`,
`interpreter.rs`, `verifier.rs`, `vm.rs`, `memory_region.rs`, and `elf.rs` in
`docs/specifications/ebpf/sbpf/`. Provenance is recorded beside those sources.

## Assumption register

| ID | Assumption | Basis | Dependent result | Stress test | Falsification probe | Status |
|---|---|---|---|---|---|---|
| A1 | RFC 9669 and the bundled Anza version matrix define the requested targets | Review and repository guide | All semantic changes | All five SBPF versions, both RFC byte orders | Compare version predicates with pinned primary source | confirmed |
| A2 | Unsupported platform pseudo-loads may fail explicitly | Review's permitted correction; platform resolver absent | Pseudo-load rejection in execution/lift | Every subtype 1-6; preserve destination and PC | Execute each subtype without a resolver and observe any constant load | retained |
| A3 | Shared memory requires indivisible atomics | RFC 9669 section 5.3; GuestMemoryMmap has shared access | Direct atomic interface and implementations | Concurrent increments, CAS failure, both widths/byte orders | Lost updates or ordinary load/store used in an atomic transaction | retained |
| A4 | A lifted function range describes guest bytes, not IR size | SmirFunction guest_range contract | Range tracking | Wide instructions and expanding division | Compare decoded byte extents with lifted range | confirmed |

## Change-surface map

| Plane | Scope |
|---|---|
| Direct decode/execute, CPU state | Affected: reserved encodings, r10, pseudo-load rejection, frame/call policy |
| Memory/MMU | Affected: atomic transactions, versioned regions, stack gaps |
| SMIR lift/interpreter | Affected: calls/returns, atomics, guest boundaries, fault PC |
| SMIR IR/optimizer | Audit operation metadata and preserved effects; use existing operations where exact |
| Native lowering/JIT | No new admission; eBPF native execution remains unsupported |
| Backend | Uses direct CPU and shared GuestMemoryMmap; atomic backing is affected |
| Machine/device | Unaffected: no eBPF machine/device wiring change |
| Oracle/analysis | Affected decoder/lift output; audit call classification and register effects |
| C ABI | No ABI layout or identifier changes; oracle behavior follows engine |
| Tests/docs | New regression matrices in existing registered targets; behavioral documentation |

## Baseline evidence

Host: aarch64-apple-darwin; stable Rust 1.98.1 (48a229cea), LLVM 22.1.8.
`cargo +stable test --locked --no-default-features --test ebpf_smir_lift
--test ebpf_diff -- --nocapture`: 4 + 4 passed before fixes. LLVM MC was present.
These original suites omit calls and exclude big-endian atomics; their success
does not falsify the review.

## Bounded findings

| Impact | Finding | Evidence / disposition |
|---|---|---|
| High | r10 writes accepted beyond manual SBPF ADD64_IMM | Decoder lacks destination validation; Anza verifier check_registers permits manual adjustment only in V1/V2. In scope. |
| High | Dynamic-frame initial r10 uses one frame instead of total stack | EbpfCpu::new vs reference EbpfVm::new. In scope. |
| Medium | RFC decoder accepts SBPF-only CALLX opcode | RFC section 4.3 defines K CALL only. In scope. |
| Medium | Reference source provenance used moving branch names | Pin and hash added reference files; retain original imported files unchanged. |

Results, regression counts, residual limitations, and final quality-gate audit
are recorded as each semantic group is verified.

## Group 1: decoding, pseudo-loads, and frame policy

Four new regression tests failed on the baseline: reserved EXIT fields, writable
r10, pseudo-loads interpreted as constants, and manual-frame initial r10. After
the changes: `ebpf_smir_lift` 8 passed, `ebpf_diff` 4 passed (LLVM MC present),
`isa_oracle` 80 passed. No ignored tests in these targets. The existing differential
sweep emitted nonzero unused source/immediate fields; its encoding generator now
clears them per RFC section 3.1. The independent evaluator's writable-r10 version
matrix was also stale and is corrected directly from Anza verifier rules.

Findings 1, 4, and 8 are addressed. Pseudo-loads retain decode/analysis support
but deliberately return UnsupportedPseudoLoad / LiftError::Unsupported rather
than invent platform addresses. A2 is confirmed. Manual-frame default initial
r10 assumes the configuration's full `stack_frame_size * max_call_depth` stack;
embedders supplying a different backing size must set r10 explicitly.
