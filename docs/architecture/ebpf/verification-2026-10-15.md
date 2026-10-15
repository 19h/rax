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

## Group 2: executable layout and stack gaps

Findings 6 and 7 are addressed by a checked version-aware mapping constructor,
while preserving the original constructor as a documented V3/V4 convenience.
The rodata address comes from the executable, rather than an invented fixed
offset. Legacy rodata may contain identical text bytes. Gap reads and legacy
rodata accesses failed under the original constructor behavior; regression
matrices cover all versions, gap enable/disable, frame/gap boundaries, writes,
SMIR reads/probes, lifting reads, permissions, overflow, and contained text.

The mapping requires a memory access to stay within a mapped region, including
gapped frames. The pinned Anza MemoryRegion::get implementation checks whether
the starting address lies in a gap, but can allow an unaligned access starting
in a frame to extend beyond that frame into contiguous host backing. RAX
intentionally rejects such accesses at the virtual gap boundary. This is a
stricter mapping contract and a recorded difference from that reference edge.
For F frames, construction uses O(F) regions; lookup is O(F), memory O(backing
bytes + F). Contiguous stacks use one region.

## Group 3: SMIR calls, guest boundaries, and ranges

Findings 2 and 9 are addressed. Call frames share direct-CPU policy, preserve
r6-r9/r10, and restore caller PCs. Legacy SBPF keys are explicitly registered
with the lifter. Direct call targets and registered indirect entries are lifted;
optimizer reachability retains these entries and syscall continuations. A function
range now uses consumed guest bytes, including wide instructions and instructions
that otherwise emit no operations. A4 is confirmed.

Additional corrections: add_function installs executable blocks; syscalls advance
to resumable continuations; guest instruction markers preserve precise PCs and
accounting at O0/O1/O2; execution resumes at a marker inside an optimized block.
SBPF CALLX truncates byte addresses to slots as the reference does. Markers are
side-effecting, have no register inputs/outputs, fail both JIT admission checks,
and are rejected by both host lowerers. Operation-effect and source-register
metadata were split into bounded modules while retaining exhaustive dispatch.

The original call, range, and syscall regressions failed before changes. The
independent evaluator now models relative calls, version-specific register calls,
frames, and returns from RFC 4.3.2 and Anza interpreter rules. It remains a
specification-derived evaluator, not execution of the upstream SBPF crate.

CFG discovery uses a visited set and an address worklist; reverse block-cache
lookups make worst-case time O(instructions + blocks^2). Marker resume scans the
cached block operations only when the requested PC is not already a block entry.
Native eBPF execution remains outside admission.

Group 3 gates: `ebpf_smir_lift` 18 passed, `ebpf_diff` 5 passed (LLVM MC
executed), `isa_oracle` 80 passed, portable library 7788 passed / 2 ignored.
The two ignored tests are pre-existing full-microkernel lowering/roundtrip probes.

## Group 4: atomic transactions and byte order

Findings 3 and 5 are dynamically confirmed and corrected. Baseline four-thread
increments yielded 6227 instead of 16384 (32-bit little-endian). The baseline
big-endian atomic matrix diverged in memory bytes. Byte-only custom memory
accepted interruptible atomics, and a failed direct CAS bypassed write permission.
All four regressions failed before implementation.

The direct memory trait now requires an indivisible transaction or rejects the
operation. Shared GuestMemoryMmap uses checked vm-memory atomic references and
sequentially consistent CAS. A global transaction lock prevents overlapping
mixed-width atomics between eBPF contexts; no guest callback runs under that lock.
The vm-memory () bitmap needs no explicit dirty notification. Private memory uses
exclusive mutable ownership. SMIR uses the marker's guest byte order, translates
CAS expected/replacement/previous values, and implements big-endian arithmetic
RMW by a backend CAS loop. With k failed CAS attempts, time is O(k+1), space O(1);
contention imposes no finite retry bound.

Tests cover all 10 atomic immediate forms, both byte orders, both widths, zero,
all-ones, asymmetric byte patterns, overflow, successful/failed comparisons,
fetch results, register aliases, concurrent direct increments, and concurrent
SMIR big-endian RMW. Additional segmented-memory generic atomic NEG/NAND/MIN/MAX
operations silently returned the unchanged value; an observed temporary reversal
failed NEG32 (3 instead of 4294967293), and the exhaustive implementation passes.
A3 is confirmed for the tested shared backends. Custom backends retain the
responsibility to implement indivisible underlying SMIR atomic primitives.

Group 4 gates: ebpf_smir_lift 25 passed, ebpf_diff 5 passed, isa_oracle 80 passed.

Bounded opportunity, medium impact, non-blocking: replace the shared-memory global
transaction lock with region/address synchronization if throughput measurements
justify it, while retaining mixed-width overlap exclusion. External host access
must obey the backend's atomic/memory contract.
