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
| A2 | Unsupported platform pseudo-loads may fail explicitly | Review's permitted correction; platform resolver absent | Pseudo-load rejection in execution/lift | Every subtype 1-6; preserve destination and PC | Execute each subtype without a resolver and observe any constant load | confirmed |
| A3 | Shared memory requires indivisible atomics | RFC 9669 section 5.3; GuestMemoryMmap has shared access | Direct atomic interface and implementations | Concurrent increments, CAS failure, both widths/byte orders | Lost updates or ordinary load/store used in an atomic transaction | confirmed |
| A4 | A lifted function range describes guest bytes, not IR size | SmirFunction guest_range contract | Range tracking | Wide instructions and expanding division | Compare decoded byte extents with lifted range | confirmed |
| A5 | Manual-frame default stack backing is stack_frame_size times max_call_depth | CPU construction has no memory-map parameter; matches reference default allocation | Initial r10 in V1/V2 | An embedding supplies a differently sized stack | Compare explicit stack backing length to this product; set r10 to that backing's top when different | retained |

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

The initial region-per-frame representation was later replaced after executable
upstream verification of cross-frame accesses. Group 7 records the final
compressed translation and its exact start-address/backing-bounds contract.

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
frames, and returns from RFC 4.3.2 and Anza interpreter rules. That evaluator remains specification-derived. The separate pinned upstream
execution oracle added in Group 7 supplies independent runtime evidence.

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

## Group 5: exact runtime frontiers and legacy call resolution

Four further regressions failed before changes: single stepping executed a whole
block; SBPF division failures were generic Undefined exits; static SBPF CALL src
kinds beyond 0/1 were accepted; and legacy keys in both registries skipped the
external phase. The latter follows the two independent resolution branches in
Anza interpreter CALL_IMM. Direct execution now resolves external then internal;
the lifter accepts both registries, exits for a host syscall, and resumes a
deferred internal call exactly once. O0/O1/O2 tests check the return value,
callee-preserved register, PC, and instruction count across that host boundary.
Stepping/breakpoint probes cover stopping and resuming inside an optimized block;
division probes check cause, PC, instruction count, and unchanged destination.

A proposed repeated-load optimization defect was not reproduced: a changing-memory
backend observed both loads at O0/O1/O2. This hypothesis did not justify a code
change. The regression remains as an observable-memory probe.

Group 5 focused gates: ebpf_smir_lift 30 passed, ebpf_diff 5 passed, isa_oracle
80 passed, with no ignored cases in these targets.

## Group 6: program discovery and distinct function extents

Self-red-team exposed a second address-range requirement: a caller's range must
exclude the callee body. The intermediate closure representation reported (0,40)
for a 24-byte caller, which the new regression rejected. EbpfLifter::lift_program
now discovers a program as separate functions, while lift_function traverses only
intraprocedural successors. The example yields (0,24) and (24,40). Call execution
registers each function independently and optimizer reachability only needs the
caller's syscall continuations. Caller/callee ranges, nested calls, indirect
entries, recursion, and host syscall resumption pass in the updated harnesses.
Focused gates: ebpf_smir_lift 31 passed, ebpf_diff 5 passed, isa_oracle 80 passed.

## Group 7: independent upstream execution and exact address translation

The optional `tools/ebpf-reference` package compiles Anza solana-sbpf 0.25.0 from
commit 3e1510b1eac344af15c99dcdb4c3b3e6dc2b3d7b, extracted from the checksummed
source archive in docs. All four original bundled SBPF files match this pinned
commit byte-for-byte. The source archive retains upstream licenses. The tool has
a separate workspace and lockfile; the engine lockfile and dependency pins are
unchanged. `python3 tools/ebpf-reference/run.py` verifies cached source content
and executes the upstream interpreter with tracing enabled.

The upstream VM register buffer remains stale after interpreted exit. The oracle
therefore compares the returned r0 with the last trace before root EXIT, verifies
call_depth=0 and EXIT encoding, and takes the eleven final GPRs from that trace.
It does not claim that the VM register buffer supplies final registers.

Executable probes confirmed two layout details requiring further corrections:

- V3/V4 ordinary reads at the bytecode address were incorrectly allowed by a
  duplicate data alias. Upstream returns AccessViolation(Load, 0x100000000, 1,
  unallocated). EbpfMemory::fetch_bytes and the code MemoryReader now use separate
  executable text. Guest data reads/probes use only the versioned data regions.
- V0 translates the starting address and checks contiguous host backing. A
  four-byte access at stack+4094 succeeds; gap-start stack+4096 fails; an access
  extending past the final backing bytes fails. A frame-per-region model was too
  strict. The final map uses one stack backing and compressed virtual translation.

For frame size F=4096 bytes, relative virtual offset d and width n bytes:
`f = floor(d/F)`. Odd f faults. Otherwise `p = floor(f/2)*F + d mod F`, and
`p+n <= backing_length` is required. These are exact unsigned integer operations;
checked address/length arithmetic rejects wrap. For d=4094, n=4, p+n=4098 bytes;
for d=28670, n=4 with 16384 backing bytes, p+n=16386 bytes and the access faults.
Translation is O(1), region lookup O(region_count), storage O(backing bytes).

Final focused gates at this group: ebpf_smir_lift 33 passed, ebpf_diff 5 passed,
isa_oracle 80 passed. The upstream package has 4 tests covering 30 executed
configurations: 10 nested call/frame cases across V0-V4 and both call forms,
15 rodata/gap/permission cases, 2 forbidden V3/V4 data aliases, and 3 V0 cross-frame
accesses. Successful cases compare all eleven GPRs; fault probes compare exit
classification with the upstream interpreter. Direct and SMIR O0/O1/O2 GPR
comparisons cover the nested-call matrix. No upstream test self-skips.

## Group 8: representable instruction extents

The final boundary audit found two further configured-text defects. An aligned
call target at 0xfffffffffffffff8 passed a saturating eight-byte extent check,
changed the stack frame, and then faulted at the callee fetch. A wide instruction
at 0xfffffffffffffff0 was accepted even with exclusive text end u64::MAX, leaving
only 15 text bytes. Both regressions failed before correction.

Instruction extents now use checked addition. For the call, (2^64 - 8) + 8 =
2^64 bytes, outside a representable u64 exclusive end. For LDDW, (2^64 - 16)
to (2^64 - 1) supplies 15 bytes rather than the required 16 bytes. The direct CPU
rejects this wide instruction after only its first eight-byte fetch, reporting
the actual available length. The shared call validator rejects the call at the
caller PC before register/frame changes; direct and SMIR O0/O1/O2 agree.
These constant-time, constant-space checks close a medium-impact boundary gap.
Complete focused gates after correction: ebpf_smir_lift 35 passed, ebpf_diff
5 passed, isa_oracle 80 passed, with no ignored tests in these targets.

## Final finding audit

| External finding | Resolution | Direct evidence |
|---|---|---|
| 1: V3/V4 frame policy | Manual frames only in V1/V2; V0 gaps; V3/V4 fixed contiguous frames | Nested direct/SMIR matrix and 10 upstream call configurations |
| 2: SMIR calls/returns | Shadow frames, precise continuation, r6-r9/r10 preservation; legacy registry and dual resolution | r0=42 regression, nested/recursive/CALLX cases, host resumption, upstream GPRs |
| 3: big-endian atomics | Convert within indivisible CAS/RMW transactions and return old values in guest order | Both widths, all 10 immediate forms, aliases, O0/O1/O2, concurrent SMIR |
| 4: pseudo-load execution | Explicit unsupported execution/lift for subtypes 1-6 without a resolver | Both byte orders; destination and PC preserved on rejection |
| 5: shared atomicity | Transaction trait, synchronized shared host atomics, no byte-only fallback | Observed lost-update baseline; direct four-thread and shared-SMIR counters |
| 6: V0 gaps | Compressed reference translation, gap-start checks, contiguous backing bounds | Upstream gap-start and three cross-frame cases; direct/SMIR probes |
| 7: rodata layout | Explicit versioned rodata address; separate code fetch; no V3/V4 data alias | 15 upstream data/permission configurations and 2 alias cases |
| 8: reserved fields | Strict RFC unused fields; r10 write rules; invalid static SBPF call kinds | LE/BE malformed-field matrix; shared decoder/lifter/oracle paths |
| 9: function ranges | Consumed guest bytes and distinct function discovery | Wide/division/empty-IR extents and separate (0,24)/(24,40) function ranges |

## Verification scope and remaining boundaries

The finite matrices establish the behavior described above; full opcode-by-opcode
conformance remains unknown. This work does not supply a Linux BPF verifier, a
complete Solana ELF/relocation/syscall loader, or platform map/BTF pseudo-load
resolvers. The checked constructor is for raw text at MM_BYTECODE_START with
explicit executable rodata addresses; custom loaders can supply their own memory
interfaces. Helper callback keys are an embedding-defined u32 registry; separate
platform helper/BTF key spaces are not independently modeled. These are medium
capability boundaries, outside the corrected instruction and raw-text execution
contracts, and do not invalidate the measured results.

Native eBPF JIT admission remains disabled. Linux/Windows native execution and
full cross-platform runtime conformance were not established by this macOS
AArch64 run. The upstream tool deliberately runs interpretation rather than the
Anza native JIT. Full VM snapshot/migration of callback registries and shadow call
frames is outside this register/execution verification.

Assumptions A1-A4 are confirmed for the tested contracts. A5 remains the explicit
default stack-allocation assumption; embeddings with different backing lengths
must initialize r10 accordingly. No memory file or unrelated user content was
modified. All staging uses exact task-owned paths.
