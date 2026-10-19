# eBPF / SBPF conformance verification, 2026-10-16

Baseline: `51aa9b11fbe4181b63f19e704c343c531bfb1d16` (the 2026-10-15 semantic
verification record is [verification-2026-10-15.md](verification-2026-10-15.md)).
Host: aarch64-apple-darwin (Darwin 27.2.0), stable Rust 1.98.1 (LLVM 22.1.8),
LLVM tools 23.0.0git, QEMU 11.1.1, OrbStack Docker 29.4.0. Pre-existing
untracked content in the shared worktree is outside this task's ownership.

## Acceptance criteria

The earlier record left full opcode-by-opcode conformance unknown. This task
replaces that unknown with exhaustive, independently authored evidence:

1. Every RFC 9669 instruction form, in both byte orders, agrees with the
   Linux kernel (the reference eBPF implementation) on every rax plane: the
   direct `EbpfCpu` and SMIR at O0, O1, and O2.
2. rax passes the eBPF Foundation `bpf_conformance` suite on every plane.
3. RFC 9669 decode admission agrees with the Linux verifier's encoding
   checks, with every difference attributed to a documented cause.
4. Solana SBPF V0-V4 execution agrees with the pinned Anza interpreter on
   every plane, including traps, faulting slots, and compute units; SBPF
   admission equals the pinned `RequisiteVerifier`.
5. The evidence is durable: plain `cargo test` replays it without Docker,
   QEMU, C++, or the upstream Rust package, and CI runs those targets.
6. Every defect found has a regression test that fails on the baseline.
7. Where the toolchain can discharge it, conformance is proven for every
   operand value rather than sampled: RFC 9669 admission and direct
   execution of every opcode byte in both byte orders, and SBPF admission
   for every opcode byte and version (formal section). What is not proven
   is named, with the measured reason.

## Oracles

| Oracle | Authority | Independence |
|---|---|---|
| Linux 7.0.14 arm64 BPF JIT (`BPF_PROG_TEST_RUN`), OrbStack | Reference eBPF implementation, little-endian | Separate kernel code base; executes identical program bytes |
| Linux 6.12.107 s390x generic BPF interpreter (Debian 13) under QEMU TCG | Reference eBPF implementation, big-endian (no BPF JIT in that build) | A second kernel implementation (C interpreter) on a big-endian ISA |
| Linux verifiers of both kernels | Kernel admission and encoding checks | Load-only harness mode |
| eBPF Foundation `bpf_conformance` @ `698e3795` | Hand-written RFC test expectations, own runner and assembler | Official plugin protocol; rax is the plugin |
| Anza `solana-sbpf` 0.25.0 @ `3e1510b1` interpreter and `RequisiteVerifier` | Solana runtime semantics and admission | Upstream Rust code, interpreted with its tracer |
| LLVM MC 23 | BPF encodings | Existing `ebpf_diff` checks |

Provenance, checksums, and licenses are in
[PROVENANCE.md](../../specifications/ebpf/PROVENANCE.md). The drivers are
`tools/ebpf-conformance/run.py` and `tools/ebpf-reference/run.py`.

## Assumption register

| ID | Assumption | Basis | Dependent result | Stress test | Falsification probe | Status |
|---|---|---|---|---|---|---|
| B1 | The kernel's `BPF_PROG_TYPE_SYSCALL` context round trip transports the complete case state | `bpf_prog_test_run_syscall` copies the context back; smoke tests | Kernel campaigns | Every byte of every case output region compared; injected one-byte corruption | Corrupt `output.bin` and observe zero mismatches | confirmed: 4 of 4 planes report the injected byte |
| B2 | Verifier rejections caused by policy (immediate zero divisor, immediate shift range, unaligned stack, reads of `r1..r5` after calls) are not ISA rules | RFC 9669 4.1 and 3; kernel `check_alu_op`, `check_stack_*` | Kernel case exclusions; admission classification | Admission campaign over 1.78 M encodings | An RFC 9669 rule requiring such a rejection | confirmed |
| B3 | Linux extensions after RFC 9669 (`may_goto`, load-acquire/store-release, `gotox`, arena casts) are outside RFC 9669 conformance | RFC 9669 text; Linux `instruction-set.rst` history | Admission disagreements | Every accepted-by-kernel/rejected-by-rax encoding is enumerated | An RFC 9669 section defining them | confirmed |
| B4 | `verify_sbpf` equals `RequisiteVerifier`, so SBPF streams filtered by either verifier are identical | 26,067,640-program differential | Engine-side Anza corpus replay | Every opcode x field partition, programs of every length class | A streamed program that upstream rejects | confirmed: 0 upstream rejections in every stream |
| B5 | An embedder resolves unregistered syscall keys as the upstream loader does | The engine exits to the host for unregistered keys | SBPF `Unsupported` comparisons | V3/V4 unresolvable call kinds; random keys | A loader that resolves keys differently | retained (embedding contract) |
| B6 | Digests (two FNV-1a 64 lanes) detect accidental differences | Order-sensitive digest of complete outcome images | Durable corpora | Five engine mutations | A mutation that survives | confirmed: 5 of 5 killed |
| B7 | `tools/ebpf-formal/src/spec.rs` and the generated Table 18 registry render RFC 9669 sections 2-5 faithfully | Written from the RFC text; the registry is parsed from the checksummed RFC | Formal tier, RFC 9669 | The proofs found two spec errors: LDDW `next_imm` (section 3.1), and the MEMSX loads that section 5.2 defines but Table 18 omits (resolved for the normative section 5.2) | An RFC clause the spec contradicts, or a kernel or `bpf_conformance` case the proven engine fails | retained: the proven engine also agrees with both kernels and `bpf_conformance` on every corpus |
| B8 | Kani 0.68.0 / CBMC 6.11.0 are sound for these harnesses | Bit-precise model; unwinding assertions on; undefined-behavior checks on | Formal tier | Spec-error and harness-error counterexamples were reported precisely (LDDW, unwind bounds) | A known-false assertion reported as proved | retained |
| B9 | The division and remainder primitives (`rax::isa::ebpf::arith`, `spec::ops`) are exactly the Rust operators, and the ALU computes every quotient and remainder through them | One-line bodies; the eight ALU sites call them | Formal tier, DIV/MOD/SDIV/SMOD | The uninterpreted operator over-approximates the real one and asserts its domain | An ALU division not routed through `arith` | confirmed by inspection of `cpu.rs` |
| B10 | The engine reaches memory only through the `EbpfMemory`/`SmirMemory` methods the memory contract proves | `ProofMemory` decomposition; trap built from the operation, not the backend error | Formal tier, memory instructions | Contract proofs over every address, width, value, byte order | A trait method used by execution but not proven | confirmed |
| B11 | `code_addr(imm)` is the instruction `imm + 1` slots after the LDDW | Linux verifier probe: only `imm = 3` targeted the subprogram at slot 4 | D13, formal tier | Every subtype bound and unbound on every plane | A platform defining another base | confirmed (RFC 9669 leaves the base implicit) |
| B12 | `r10` is read-only (platform contract) | RFC 9669 section 6 puts verification out of scope; the Linux verifier rejects writes | Admission, formal tier | Kernel admission corpora | A conformant platform that writes `r10` | confirmed |
| B13 | Hash-map seeding does not affect results | Lookups do not depend on the seed | Formal tier stubs | Fixed seeds in every harness | A result that changes with the seed | confirmed by construction |

## Change-surface map

| Plane | Scope |
|---|---|
| Direct decode/execute | Affected: meter charges before fetch; SBPF call-entry order; V3/V4 unresolved relative calls; LDDW pseudo-loads and BTF helpers (D13); division and remainder through the one-line `arith` primitives, and the 32-bit `MUL` product in its unsigned form (both behavior-preserving, for the formal tier) |
| CPU state | Affected: traps leave registers and frames at the pre-instruction state |
| Memory/MMU | Affected: `FlatMemory` checked bounds (shared by every ISA) |
| SMIR lift | Affected: unliftable instructions become exact deferred traps |
| SMIR IR | Affected: `TrapKind::Ebpf(EbpfTrap)`; `DivU`/`DivS` are side-effecting |
| SMIR interpreter | Affected: fatal meter trap, precise memory-fault trap, fetch validation at guest markers |
| Optimizer | Affected: block merging never consumes the function entry; DCE keeps trapping divisions (shared by every ISA) |
| Native lowering / JIT | Unaffected: `TrapKind::Ebpf` is rejected by every lowerer's catch-all; eBPF native admission stays closed |
| Oracle/analysis | Unaffected interfaces; `isa_oracle` passes unchanged |
| C ABI | Unaffected; `rax-capi` rerun |
| Admission | Added: `rax::isa::ebpf::verify_sbpf` and `SbpfVerifierError` |
| Tests/docs/CI | New corpora, replays, regression tests; `ebpf_diff` and `ebpf_smir_lift` added to CI; Kani harnesses in `tools/ebpf-formal` (outside the workspace and CI) |

## Defects found and corrected

Each regression test failed on the baseline (checked by exporting HEAD with
`git archive`, adding only the tests) and passes after the change.

| # | Impact | Finding (oracle) | Correction | Regression test |
|---|---|---|---|---|
| D1 | High | SMIR reported a configured instruction-meter exhaustion as the resumable interpreter slice `InsnLimit` (Anza) | Fatal `EbpfTrap::ExceededMaxInstructions` at the guest marker; only the interpreter's own budget is `InsnLimit` | `instruction_meter_exhaustion_is_the_same_fatal_trap_on_every_plane` |
| D2 | Medium | SMIR memory faults were generic `MemoryFault` exits without access size; `FlatMemory` reported the end address (Anza) | `EbpfTrap::MemoryFault` with the effective start address, width, and direction, exactly as the direct CPU | `guest_memory_faults_report_the_direct_cpu_trap_on_every_plane` |
| D3 | High | `FlatMemory::probe` and its `MemoryReader::read` overflowed for guest addresses near 2^64: a checked-build panic, and in release a wrong `Ok` or a reversed slice panic; an in-bounds write ending at 2^64 overflowed the exclusive-monitor check (every ISA) | Checked u64 spans; u128 monitor overlap | `bounds_checks_at_the_top_of_the_address_space_never_overflow` |
| D4 | High | O2 `block_merging` merged a function's entry block into its single counted predecessor, leaving an empty `Unreachable` entry (every ISA; found via an SBPF registry entry inside another function) | The entry has an implicit external predecessor and is never a merge target | `block_merging_never_consumes_the_function_entry` |
| D5 | High | Dead-code elimination removed a division whose result was dead, deleting its divide-by-zero or overflow trap (every ISA) | `DivU`/`DivS` are side-effecting | `dead_code_elimination_keeps_divisions_that_may_trap`, `optimization_keeps_a_trapping_division_whose_result_is_dead` |
| D6 | High | The direct CPU did not charge an instruction whose fetch overran the text or failed to decode; Anza does, so consumed compute units differed by one on those failures | Charge before fetch; SMIR markers mirror the fetch validation | `text_overrun_and_unliftable_instructions_trap_only_when_reached` |
| D7 | High | SMIR could not lift about 7.5 % of upstream-verified random programs: falling off the text end, undecodable instructions (V3/V4 call kinds 2-10), and RFC pseudo-loads or BTF calls on untaken paths failed the whole lift | `TrapKind::Ebpf` deferred traps raise the direct CPU's exact trap only when reached | same test |
| D8 | Medium | SBPF register and registry calls validated the target before pushing the frame (Anza pushes first: `CallDepthExceeded` wins); V3/V4 relative calls outside the text raised `CallOutsideTextSegment` instead of an unresolved call | Shared `EbpfConfig::enter_call` in the reference order; precise state on trap | `sbpf_call_entry_follows_the_reference_order_and_stays_precise` |
| D9 | Medium | No SBPF admission: rax executes programs the Solana loader rejects | `verify_sbpf`, equal to `RequisiteVerifier` | `every_reference_rule_reports_its_error_and_slot` plus the exhaustive differential |
| D10 | Medium | No CI workflow ran `ebpf_diff` or `ebpf_smir_lift` | Added to `ci.yml` test-core and `full-suite.yml` unit | `ci_actions_pinned` |
| D11 | Low | Documentation misstated V0/V1 ALU32 sign extension and the V2 relocated memory opcodes | Corrected from the verified opcode tables | Decoder/verifier opcode-set equality test |
| D12 | Medium | The public `EbpfVcpu` `VCpu` adapter was never constructed by any test, backend, or caller | Direct tests of stepping, the 100,000-instruction batch boundary, `SystemCall` exit and host resume, `Shutdown`, trap-as-error with halt and wake, state round trip, and stores, atomics, loads, and a store fault through shared `GuestMemoryMmap` | `vcpu_adapter_steps_exits_resumes_traps_and_round_trips_state` |
| D13 | High | RFC 9669 places LDDW subtypes 0x1-0x6 in base64 and the helper call by BTF ID in base32 (Appendix A), groups rax claims, but every plane trapped on them (`UnsupportedPseudoLoad`, `UnsupportedBtfHelper`) | `EbpfPlatform` binds maps and platform variables in `EbpfConfig` and `EbpfLifter`; `code_addr` is computed (target `i + imm + 1`, established against the Linux verifier); BTF helpers get a handler registry and a resumable `EbpfExit::BtfHelper` / `EbpfHelperKey::Btf` exit in their own namespace; unbound objects still trap precisely | `every_lddw_subtype_loads_its_table_12_value_on_every_plane`, `btf_helper_calls_exit_and_resume_identically_on_every_plane`, `registered_btf_helpers_run_in_their_own_namespace`; the replaced tests asserted the old traps |
| D14 | Medium | The admission campaign varied register fields over {0, 1, 2, 9, 10, 11, 15} only, so LDDW subtypes 0x3-0x6 and CALL kinds above 2 never reached a kernel verifier | Every opcode is also crossed with all 16 x 16 register-field values (196,608 more encodings); the new class (`map_by_idx` without a map set) is documented | `decoder_admission_matches_the_{little,big}_endian_kernel_verifier_campaign` |
| D15 | Medium | No differential exercised Solana syscall compute-unit charging: the test syscall consumed nothing | Metered syscall `rax_charge` (consumes `r1 & 0xff` units through Anza's context; rax charges `insn_count` at the host exit, saturating at the budget like the reference) and a `metered-syscall` corpus stream at budgets around every call | `every_plane_reproduces_the_pinned_anza_corpus` (a non-saturating charge fails it) |

Harness defects found during the campaign (not engine defects) are recorded
for reproducibility: on-demand lifting must reuse one `LiftContext`, a lift
failure must not be counted as an upstream rejection, and the conformance
runner silently skips its raw negative tests (their expected error is empty),
so `run.py` and `ebpf_diff` enforce them separately.

## Results

| Campaign | Oracle | Scope | Result |
|---|---|---|---|
| `bpf_conformance` | eBPF Foundation suite, `--cpu_version v4`, default groups | 312 tests; 1 skip (`callx.data`, non-RFC uBPF group, rejected by the RFC dialect) | 312/312 on each of 4 planes |
| Reserved-field negatives | `bpf_conformance/negative/unused-*.data` | 45 programs | rejected on each of 4 planes |
| Kernel LE | Linux 7.0.14 arm64 JIT | 154 forms; 147,698 cases (scale 0) and 190,738 (scale 400) | 0 mismatches, 0 kernel rejections |
| Kernel BE | Linux 6.12.107 s390x interpreter | the same 154 forms; 147,698 and 190,738 cases | 0 mismatches, 0 kernel rejections |
| Admission LE | Linux 7.0.14 verifier | 1,980,208 encodings | 477 disagreements, all explained (below) |
| Admission BE | Linux 6.12.107 verifier | 1,980,208 encodings | 401 disagreements, all explained (below) |
| SBPF opcode sets | Anza `RequisiteVerifier` | 256 opcodes x V0-V4 | decoder sets equal (88, 88, 97, 110, 110) |
| SBPF single instruction | Anza interpreter | 110,517 programs, 663,102 executions | 0 mismatches |
| SBPF boundary operands | Anza interpreter | 11,424 programs, 603,648 executions | 0 mismatches |
| SBPF random programs | Anza interpreter | 30,000 programs, 120,000 executions | 0 mismatches |
| SBPF admission | Anza `RequisiteVerifier` | 26,067,640 programs; all 15 error variants | identical verdict, variant, and slot |
| Pinned Anza tests | Anza interpreter | 30 configurations from the earlier record | pass |

The kernel forms are every RFC 9669 instruction in the base32, base64,
atomic32, atomic64, divmul32, and divmul64 groups. ALU and jump forms in both
widths and source kinds run over the full 40 x 40 (register) or 40 x 32
(immediate) boundary cross product with distinct, aliased, `r0`, and `r9`
operands. The remaining forms are `MOVSX` 8/16/32, NEG, all nine byte-order
conversions, LDDW, aligned loads, sign-extending loads, and stores at every
window offset, all ten atomic operations in both widths, and local and nested
calls with stack-pointer arguments. A batch carries about 200 cases.

The candidates are every opcode crossed with register, offset, and immediate
partitions (with five LDDW second-slot variants), plus every opcode crossed
with all 16 x 16 register-field values at three immediates. The second set
reaches every LDDW subtype (0x0-0x6) and CALL kind; the first campaign used
only register values {0, 1, 2, 9, 10, 11, 15} and so never presented
subtypes 0x3-0x6 to the verifiers. The kernels reject subtype 0x3
(`var_addr`) and 0x4 (`code_addr`) only by program-safety checks (BTF lookup,
subprogram structure), never as encodings.

### Admission disagreements (Linux 7.0.14, little-endian)

| Count | rax | Kernel | Reason |
|---|---|---|---|
| 288 | decodes | `invalid shift` | RFC 9669 4.1 masks shift counts; the verifier rejects out-of-range immediates by policy |
| 72 | decodes | `div by zero` | RFC 9669 4.1 defines division by zero; the verifier rejects an immediate zero divisor by policy |
| 40 | decodes | `fd_idx without fd_array is invalid` | LDDW subtypes 0x5/0x6 (`map_by_idx`) name the program's map set, which the load did not bind; rax admits the encoding and traps with `UnsupportedPseudoLoad` if it executes unbound |
| 74 | rejects | accepts | Load-acquire / store-release atomics (Linux 6.15), all widths |
| 3 | rejects | accepts | `may_goto` (`BPF_JCOND`, Linux 6.9) |

### Admission disagreements (Linux 6.12.107, big-endian)

| Count | rax | Kernel | Reason |
|---|---|---|---|
| 288 | decodes | `invalid shift` | as little-endian |
| 72 | decodes | `div by zero` | as little-endian |
| 40 | decodes | `fd_idx without fd_array is invalid` | as little-endian |
| 1 | rejects | accepts | `may_goto` with offset 1; Linux 6.12 still rejects offset 0 |

Linux 6.12 predates load-acquire and store-release, so it rejects those
encodings as `BPF_ATOMIC uses invalid atomic opcode`, as rax does. In both
byte orders the kernel accepts 15,583 encodings that rax also decodes. No other
encoding is accepted by one side and syntactically rejected by the other.
Semantic rejections (for example `jump out of range`, pointer arithmetic, or
`invalid mem access 'scalar'`) concern program safety, not encoding, and were
inspected row by row in both corpora.

### Durable replays (`cargo test --test ebpf_diff`)

| Module | Corpus | Content |
|---|---|---|
| `conformance_corpus` | `bpf_conformance.txt` (57 KiB) | 312 runs, 1 skip, 45 rejects |
| `kernel_corpus` | `linux_kernel_le.txt`, `linux_kernel_be.txt` (84 KiB each) | 1,410 per-program digests each = 147,698 cases each |
| `admission_corpus` | `linux_kernel_admission_le.txt` (98 KiB), `linux_kernel_admission_be.txt` (87 KiB) | decoder verdict digest over 1,980,208 encodings per byte order and every classified disagreement |
| `sbpf_corpus` | `anza_sbpf.txt` | 20 digests (five versions x four streams, including `metered-syscall`) over 26,393 programs / 127,683 executions |

The whole target (12 tests) runs in about 40 s in a debug build, so the kernel and Anza
replays also execute with overflow checks enabled.

### Source coverage of the engine replays

Measured with `-C instrument-coverage` over `ebpf_diff`, `ebpf_smir_lift`,
`isa_oracle`, and the `isa::ebpf` library tests (the external tools excluded),
with LLVM 23 `llvm-profdata`/`llvm-cov`, after all changes:

| File | Lines | Before D12 and the opcode-set test |
|---|---|---|
| `isa/ebpf/decode.rs` | 95.6 % | 94.5 % |
| `isa/ebpf/validation.rs` | 98.5 % | 98.5 % |
| `isa/ebpf/verifier.rs` | 98.0 % | 98.0 % |
| `isa/ebpf/calls.rs` | 95.9 % | 95.9 % |
| `isa/ebpf/cpu.rs` | 96.2 % | 83.9 % |
| `isa/ebpf/mod.rs` | 99.6 % | |
| `isa/ebpf/memory.rs` | 80.7 % | 78.4 % |
| `isa/ebpf/atomics.rs` | 85.1 % | 85.1 % |
| `isa/ebpf/layout.rs` | 91.8 % | 91.8 % |
| `isa/ebpf/disasm.rs` | 24.1 % (only the oracle formats text) | 24.1 % |
| `smir/lift/ebpf/mod.rs` | 97.7 % | 97.7 % |
| `smir/lift/ebpf/control_flow.rs` | 84.0 % | 83.1 % |
| all eBPF modules | 91.2 % (3,663 of 4,018) | 88.8 % |

Every unexecuted line outside the disassembler was inspected:

- `Display`, `Debug`, `Default`, and accessor code.
- `unreachable!` invariant arms, and control-flow variants the eBPF lifter
  never produces: it emits conditional branches as `CondBranchReg` on a
  computed condition, never flag-based `CondBranch`, `IndirectBranch`,
  `IndirectBranchMem`, or `Trap` control flow.
- SMIR atomic, compare-and-swap, and exclusive-monitor adapters on
  `SbpfMemoryMapping`. SBPF V0-V4 have no atomic opcodes (the decoder's
  opcode sets equal the Anza verifier's), so no guest program reaches them.
- The `EbpfMemory` implementation for an owned `GuestMemoryMmap`. The vCPU
  adapter uses the `&GuestMemoryMmap` implementation, which D12 exercises.
- Fault arms for atomic widths or kinds that decoding has already rejected.

### Mutation sensitivity

Each mutation was applied to an isolated copy; the column lists the
`ebpf_diff` tests that failed.

| Mutation | Plane | Detected by |
|---|---|---|
| `ARSH32` made logical | direct | kernel LE and BE, Anza corpus, pre-existing seeded sweep |
| `MOVSX` from 16 bits decoded as 8 | decoder | kernel LE and BE only |
| V2 `SUB_IMM` operands not swapped | decoder | Anza corpus, pre-existing SBPF sweep |
| Meter charged after fetch | direct | Anza corpus only |
| SMIR `MOVSX` from 16 bits as 32 | SMIR lifter | kernel LE and BE only |

Three of the five mutations survive every pre-existing test.

## Formal verification (Kani)

The campaigns above are finite. The formal tier replaces sampling with
bounded model checking where the toolchain can discharge it: a harness
leaves every instruction field, every register, the meter, and the data
memory symbolic, so a proof covers every value. Kani 0.68.0 with CBMC
6.11.0 runs in a Linux arm64 container from a pinned image
(`tools/ebpf-formal`, [README](../../../tools/ebpf-formal/README.md);
`python3 tools/ebpf-formal/run.py`).

| Family | Harnesses | Statement | Result |
|---|---|---|---|
| RFC 9669 admission and direct execution | 512 (256 opcode bytes x 2 byte orders) | `decode_insn` accepts exactly the encodings Appendix A Table 18 admits under the platform contract (no packet group, read-only `r10`, unused fields zero per section 3.1); one `execute_decoded` step on an admitted instruction yields the outcome of the independent `spec.rs`: registers, PC, memory, fault (start address, width, direction), exit, helper exits by static and BTF ID, local call frame, pseudo-load trap | verified, 512 of 512 |
| Fetch and meter | 1 | the rest of `EbpfCpu::step` for every PC, text range, meter state, and byte order | verified |
| Memory contract | 3 | `FlatMemory` equals `ProofMemory` in result and effect for every method the engine calls, for every address, width, value, and byte order | verified, 3 of 3 |
| SBPF admission | 1,280 (256 opcode bytes x V0-V4) | `verify_sbpf` returns the `RequisiteVerifier` verdict, variant, and slot for every value of the instruction's other bits and of an LDDW second slot, mid-text and as the last slot | verified, 1,280 of 1,280 |

One `python3 tools/ebpf-formal/run.py --jobs 12` invocation on the final
tree discharged all four families on 2026-10-19: `Complete - 1796
successfully verified harnesses, 0 failures, 1796 total`. It took 7 minutes
to compile and 1 h 59 min to verify, using 48,828 CPU-seconds of CBMC; the
longest harness took 560 s. The last 34 harnesses ran on one of the twelve
jobs, which is how Kani partitioned them.

Division and remainder are proven through a functionally consistent
uninterpreted operator that replaces both the engine's `arith` primitives
and the specification's `ops` (B9). Bit-level SAT cannot equate two divider
circuits: the same 64-bit or 32-bit divider applied twice to the same
operands did not finish in 15 minutes with CaDiCaL or Kissat 4.0.1, nor in
400 s with Kissat's congruence closure and complete sweeping on the isolated
CNF.

Multiplication is proven bit-precisely, but only between multipliers of the
same structure: an identical 64-bit pair takes 30 s, while CBMC bit-blasts a
signed multiply through sign-magnitude negation, so a signed and an unsigned
multiply of the same width form a hard miter (8 bits in 2.3 s; 16 and 32
bits unfinished after 12 minutes). The engine computed the 32-bit `MUL` as
`i32::wrapping_mul`, and the four 32-bit `MUL` harnesses
(`direct_{le,be}_{24,2c}`) did not finish (the longest had run 45 minutes
when observed). The low 32 bits of a product do not depend on operand
signedness, so the engine now computes it as `u32::wrapping_mul`, as RFC
9669 defines `MUL`, and sign-extends afterwards where SBPF requires it (V0,
V1, V3, and V4; V2 has no 32-bit `MUL`). The four harnesses then verify in
97-122 s. The kernel corpora (`mul32_imm` and `mul32_reg` over the boundary
cross product), the six `bpf_conformance` `mul32` programs, and the Anza
streams of those versions cover the changed arm on every plane.

The proofs found two errors in the specification, none in the engine:

- The spec admitted a nonzero `next_imm` for LDDW subtypes 0x1 and 0x3-0x5,
  which Table 12 does not use and section 3.1 requires to be zero
  (`direct_le_18`, `direct_be_18`). The engine's rejection was correct.
- RFC 9669 is inconsistent with itself: section 5.2 defines the
  sign-extending loads `{MEMSX, <size>, LDX}` for B, H, and W (opcodes 0x91,
  0x89, 0x81), and section 2.4 places every defined instruction in base32,
  but Appendix A's Table 18 has no row for them, so a spec generated from
  Table 18 alone rejects them (`direct_le_91`). Section 5.2 governs:
  Appendix A states that the table's descriptions are informative and that
  the referenced section is authoritative. The engine, both Linux kernels,
  and `bpf_conformance` (`rfc9669_ldxsw` and its siblings) accept them. The
  spec adds the three rows (`spec::SECTION_5_2`, modeled on the
  unsigned-load rows) without editing the generated Table 18 registry.

Each corrected harness verifies against the engine unchanged.

Not discharged:

- SMIR planes (`smir_*`, `sbpf_smir_*`). The lifter and interpreter keep
  blocks, functions, and virtual-register values in `std` hash maps (every
  IR operation reads and writes `SmirContext`'s `HashMap<VirtualId, u64>`),
  and CBMC's symbolic execution does not resolve hashbrown's bucket
  iteration and probe loops even for concrete keys. With admission and the
  register fields made concrete (one IR shape), a symbolic-execution profile
  of a probe variant of `smir_o0_le_07` shows the path guard growing to
  77,326 terms inside the lifter's block-cache iteration; the harness did
  not finish in about one CPU-hour at 11 GiB. The fully symbolic harnesses
  did not finish in 2.5 CPU-hours at 9-23 GiB. A second probe (2026-10-20,
  not kept) replaced every map and set on the eBPF lift and interpret path
  with a verification-only linear map under `cfg(kani)`. Hashbrown left the
  profile and, with admission concrete, the guard stayed at most 18 terms,
  but symbolic execution then advanced at about four calls per second at
  7-18 GiB while constructing IR values (`OpKind` has 365 variants; the
  register and context types span every ISA). No probe finished lifting one
  instruction: the ALU, load, and branch probes at O0 ran 41 minutes, and
  the register-ALU and O2 probes were stopped after 10 minutes to bound
  memory. Heap storage as such is not the obstacle: an operation read back
  from a presized, grown, or cloned `Vec` still prunes its unreachable
  dispatch arms (6,375 steps against 12,451 with the arm reachable), while a
  `BTreeMap` holding two concrete entries did not finish symbolic execution
  in 30 minutes. Discharging these planes would need map models in the
  engine and a smaller IR representation, or a different prover.
- SBPF execution against the Anza interpreter (`sbpf_step_*`,
  `sbpf_divider_*`). With the text served from a static array both
  interpreters dispatch on a concrete opcode; four probes reached 8-10 GiB
  each and were stopped after about 74 CPU-minutes when the 64 GiB machine
  ran out of memory. The 52 divider harnesses would additionally equate the
  reference's and the engine's divider circuits.

These planes rest on the differential evidence: every corpus replays
identically on every plane (`ebpf_diff`), and every SBPF stream matches
Anza.

## Validation record

Run on the final tree on 2026-10-19, after the 32-bit `MUL` change. An
unrelated commit by another contributor (`8a749768e`, x86-64 decode and
Linux user mode) had landed under this uncommitted work. The broad
cross-ISA gate ran on 2026-10-16, before the behavior-preserving `arith`
and 32-bit `MUL` refactors and the V3/V4 call-kind coverage test, which
the eBPF targets, the live oracles, and the formal tier cover. Every
command used `cargo +stable` and `--locked`; no dependency or engine lock
state changed.

| Check | Command | Result |
|---|---|---|
| Whitespace | `git diff --check` | clean |
| Format | `cargo fmt --all --check`; `cargo fmt --check` in the three tool packages; `gen_proofs.py --check`, `gen_registry.py --check` | clean |
| Clippy | `clippy --all-targets --features x86_64-suite`; `tools/ebpf-formal` with `--all-targets` | exit 0, no warnings (the engine lint tables allow warnings) |
| Build | `build --all-targets --no-default-features --features x86_64-suite,smir-jit` | exit 0 |
| Doctests | `test --doc --no-default-features --features x86_64-suite,smir-jit` | 0 run, 5 ignored (pre-existing `ignore` examples) |
| eBPF replays | `test --no-default-features --features x86_64-suite,smir-jit --test ebpf_diff --test ebpf_smir_lift` | 12 and 46 passed |
| Static oracle | `--test isa_oracle` | 80 passed |
| eBPF library | `--lib isa::ebpf` | 6 passed |
| Broad cross-ISA gate (2026-10-16) | `--lib` plus `x86_64`, `arm`, `aarch64_smir_programs`, `smir_jit_x86_aarch64`, `smir_jit_aarch32_aarch64`, `smir_jit_thumb_aarch64`, `riscv_smir_aarch64_jit`, `riscv_smir_lift`, `hexagon_smir_lift`, `smir_avx10_roundtrip`, `isa_oracle`, `ebpf_diff`, `ebpf_smir_lift`, `realmode_boot`, `riscv_boot`; `--test-threads=1`, `--no-fail-fast` | 85,762 passed, 0 failed, 5 ignored (pre-existing): library 9,984 (2 ignored), `x86_64` 28,541 (3 ignored), `arm` 46,850, `aarch64_smir_programs` 75, `smir_jit_x86_aarch64` 31, `smir_jit_aarch32_aarch64` 26, `smir_jit_thumb_aarch64` 25, `riscv_smir_aarch64_jit` 26, `riscv_smir_lift` 13, `smir_avx10_roundtrip` 52, `isa_oracle` 80, `ebpf_diff` 12, `ebpf_smir_lift` 45, `riscv_boot` 2. `hexagon_smir_lift` (Linux only) and `realmode_boot` (x86-64 only) are empty on this host |
| C API | `test -p rax-capi` | 168 passed |
| CI pins | `--test ci_actions_pinned` | 11 passed |
| `bpf_conformance` | `python3 tools/ebpf-conformance/run.py conformance` | 312/312 on each of the 4 planes; 45 negatives rejected on each; corpus equal to the pinned suite |
| Kernel and admission | `python3 tools/ebpf-conformance/run.py all` (from clean), `admission-be --export` (2026-10-16 to 2026-10-17) | as in Results; the final tree replays both kernel corpora and both admission corpora unchanged (`ebpf_diff`) |
| Anza | `python3 tools/ebpf-reference/run.py` | 12 passed; 0 streamed programs rejected by the upstream verifier; every execution equal on 5 implementations |
| Formal tier | `python3 tools/ebpf-formal/run.py --jobs 12` | 1,796 of 1,796 harnesses verified, 0 failed (the four default families) |

### Continuous validation

[`ebpf-validation.yml`](../../../.github/workflows/ebpf-validation.yml)
reruns the live oracles and the formal tier on GitHub-hosted runners. Its
first run (run `39338966500`, commit `b79c32ca2`, 2026-10-19) passed all 19
jobs in 44 minutes:

| Job | Runner | Result |
|---|---|---|
| `bpf_conformance` | `ubuntu-24.04` | 312/312 on each of the 4 planes; 45 negatives rejected; corpus equal to the pinned suite |
| Anza | `ubuntu-24.04` | every stream of V0-V4 equal on 5 implementations; 0 programs rejected by the upstream verifier |
| Linux kernel, little-endian | `ubuntu-24.04-arm` | Linux `6.17.0-1022-azure` aarch64 (Docker 28.0.4), a third kernel next to the two above: 154 forms, 190,738 kernel-verified cases, 0 rejected, 0 mismatches |
| Linux kernel, big-endian | `ubuntu-24.04` | the pinned Debian s390x 6.12.107 under QEMU: 190,738 cases, 0 rejected, 0 mismatches |
| Admission, big-endian | `ubuntu-24.04` | 1,980,208 encodings, 401 disagreements, 0 unexplained (the recorded table) |
| Kani, 14 shards | `ubuntu-24.04-arm` | 1,796 of 1,796 harnesses verified, 0 failed (4 step and memory, 1,280 SBPF admission, 512 RFC 9669 direct) |

The JIT gate test `ebpf_deferred_trap_is_interpreter_only` runs in the
`--lib` set with `smir-jit`. Every native admission gate rejects a block that
ends in `TrapKind::Ebpf` unless that block is excluded as an interpreter
frontier, and the x86-64 and AArch64 lowerers both return `UnsupportedOp`.

### Quality gates

| Gate | Evidence |
|---|---|
| QG1 Assumptions | B1-B13 above. B5 is a retained embedding contract; B7 (spec fidelity) and B8 (prover soundness) are retained with the cross-checks stated; the rest are confirmed |
| QG2 Requirements | Criteria 1-6 map to the kernel, `bpf_conformance`, admission, Anza, replay/CI, and defect sections; criterion 7 maps to the formal section, which names the planes it does not discharge and the measured reason |
| QG3 Reproducibility | Generators, seeds (`seed_of` of the form name), kernel package checksums, archive checksums, and corpus SHA-256 values are in `tests/generated/manifest.toml` and PROVENANCE.md. The formal tier pins the base image digest (`tools/ebpf-formal/BASE_IMAGE`), Kani 0.68.0, and its own lockfile; `gen_registry.py --check` and `gen_proofs.py --check` regenerate the registry and harnesses |
| QG4 Edges | Every admission disagreement and every unexecuted line was classified; the boundaries table lists what is not claimed. The RFC 9669 inconsistency between section 5.2 and Table 18 (MEMSX loads) is resolved toward the normative section and recorded |
| QG5 Provenance | RFC 9669, the pinned `bpf_conformance` and Anza archives, and the Debian kernel are recorded with checksums. Each RFC section cited in the conformance statement was checked against the checksummed `rfc9669.txt` |
| QG6 Bounded expansion | Shared SMIR fixes (D3-D5) were required by the eBPF oracles and were checked on every ISA by the broad gate; other findings are recorded, not implemented |
| QG7 Worktree | Only the files listed in this change were edited; pre-existing untracked content is untouched |
| QG8 Behavior | Regression tests failed on the baseline export; mutation testing killed 5 of 5; skip counts are explicit (one non-RFC group); the formal families report their harness totals, and a failing or unwinding proof fails `run.py` |
| QG9 Consistency | `Cargo.toml` targets, CI workflows, the manifest, AGENTS.md, and both READMEs agree with the code |

## Conformance statement and boundaries

rax implements RFC 9669 conformance groups base32, base64, atomic32,
atomic64, divmul32, and divmul64 in both byte orders (section 2.4 requires
base32 and makes every other group optional), and Solana SBPF V0-V4
execution and admission as the pinned Anza `solana-sbpf` 0.25.0 defines
them. The evidence, strongest first:

1. Proven for every value (formal tier): RFC 9669 decode admission and
   direct-CPU execution of every opcode byte in both byte orders against an
   independent specification of sections 2-5 and Table 18 (with the section
   5.2 loads that Table 18 omits); the rest of the step (meter and fetch);
   the memory backend contract; and SBPF admission (`verify_sbpf` equals
   `RequisiteVerifier`) for every opcode byte and version.
2. Agreement with independent implementations on finite, partitioned
   campaigns, on every plane (direct, SMIR O0/O1/O2): both Linux kernels
   and their verifiers, `bpf_conformance`, and the Anza interpreter and
   verifier.
3. The SMIR planes equal the direct CPU on every replayed corpus, and SBPF
   execution equals the Anza interpreter on every campaign stream. Neither
   is proven for every value; the formal section gives the measured reason.

Explicit boundaries:

| Impact | Item | Status |
|---|---|---|
| Low | RFC 9669 `packet` group (`BPF_ABS`/`BPF_IND`) | Section 2.4 makes it optional; section 5.5 deprecates the instructions ("SHOULD no longer be used") and defines their encoding but no packet-access semantics; Table 13 lists the group as Historical. Rejected at decode; not claimed |
| Low | Linux extensions after RFC 9669 (`may_goto`, load-acquire/store-release, `gotox`, arena casts) | Not RFC 9669; rejected by the RFC dialect; not claimed |
| — | Helper semantics (static and BTF IDs), maps, platform variables | Section 4.3.1 leaves helper semantics to the platform and section 5.4 defines LDDW subtypes over platform objects. rax implements the instruction contract on every plane (resumable host exits in separate namespaces, `EbpfPlatform` bindings, precise traps when unbound); what a helper or map does belongs to the embedder |
| — | Program verification | Section 6 puts verification out of scope ("the details are out of scope of this document"; verifier expectations are future IETF work). rax enforces encoding admission (proven against Table 18), the read-only `r10` contract (B12), and precise runtime traps; SBPF admission equals `RequisiteVerifier` (proven) |
| — | Solana ELF loading and relocation | Not part of rax: the engine takes text and rodata bytes (`SbpfMemoryMapping::new_solana_with_config`) and has no ELF parser under `src/isa/ebpf`. Syscall compute-unit charging is the embedder's action at the host exit, checked against the reference by the `metered-syscall` stream (D15) |
| — | Unaligned accesses | RFC 9669 states no alignment rule (the word does not occur in it); `bpf_conformance` includes misaligned word and doubleword loads and stores, which pass on every plane; the formal tier covers every address; the SBPF campaigns cover unaligned accesses against Anza. The kernel campaign uses the stack, where the verifier requires alignment by policy |
| Medium | SMIR planes and SBPF execution are not formally proven | Tool-bounded (formal section); established by the differential replays |
| Low | O2 strength reduction rewrites `DivU` by a power-of-two immediate to `Shr` of `src1`. That would be wrong for x86 8-bit `DIV` (`AX` dividend, `AH` remainder, `#DE` on overflow), but x86 `DIV` has no immediate form and no pass substitutes an immediate into `DivU` (`constant_propagation` rewrites only moves, bitwise/add/sub, and shifts; copy propagation is register-to-register) | Latent hazard for a future pass; not reachable today; no eBPF impact |
| — | Native eBPF JIT | Admission remains closed; every native gate rejects a block ending in `TrapKind::Ebpf` (`ebpf_deferred_trap_is_interpreter_only`) |
