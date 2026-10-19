# Pinned Anza SBPF execution oracle

Run from the repository root:

```sh
python3 tools/ebpf-reference/run.py
RAX_SBPF_CORPUS_OUT=1 python3 tools/ebpf-reference/run.py   # rewrite the corpus
```

The runner verifies and extracts the upstream source archive under `docs/`,
then runs this independent Cargo package with its committed lockfile. It uses
stable Rust, an isolated target directory, the engine's existing dependency
patches, and Anza SBPF commit `3e1510b1eac344af15c99dcdb4c3b3e6dc2b3d7b`
(version 0.25.0). Cargo may retrieve lockfile-pinned build dependencies on first
use. Modified or unexpected files in the extracted upstream cache are rejected.

This package is outside the engine workspace and must be invoked separately.
It does not change the engine's feature defaults or dependency lockfile.
Upstream execution is interpreted, with its tracer enabled; native SBPF JIT
execution is not exercised.

The case streams, rax execution planes, outcomes, and digests live in
`tests/support/ebpf_sbpf_oracle.rs`, shared with the engine's `ebpf_diff`
replay of `tests/generated/ebpf/anza_sbpf.txt`. Every comparison covers the
result (`r0` and all eleven GPRs, or the trap class and faulting slot), the
executed instruction count (compute units), and the final stack, heap, and
input bytes, on the upstream interpreter, the direct CPU, and SMIR O0/O1/O2.

| Test | Coverage (V0-V4) |
|---|---|
| `verified_opcode_sets_match_the_rax_decoder` | Opcodes admitted upstream equal the rax decoder's (88/88/97/110/110) |
| `every_verified_opcode_matches_anza_execution` | 110,517 programs, 663,102 executions: every admitted opcode with random fields, operands, and meter budgets |
| `boundary_operands_match_anza_for_every_verified_opcode` | 11,424 programs, 603,648 executions: the 32x32 boundary-operand cross product, aliased registers, every boundary immediate |
| `random_programs_match_anza_execution` | 30,000 programs, 120,000 executions: calls of every kind, CALLX, syscalls, LDDW, backward branches, meter exhaustion, faults |
| `sbpf_admission_matches_the_reference_verifier_exhaustively` | 26,067,640 programs: `verify_sbpf` equals `RequisiteVerifier` (verdict, variant, slot) |
| `corpus_streams_match_anza_and_the_checked_in_digests` | The replayed corpus: 26,888 programs, 122,883 executions |
| Original four tests | 30 configurations: nested frames, versioned rodata, gaps, data aliases |

The VM's register buffer does not contain final interpreted GPRs. The adapter
uses the trace before successful root EXIT, checks the returned r0 and zero call
depth, and compares those eleven GPRs. On a trap it compares the upstream error
class and `registers[11]` slot. The embedding resolves an unregistered syscall
key as the upstream loader does: an unsupported instruction at the call.
`RAX_SBPF_REPLAY=<file>` (with `RAX_SBPF_DUMP=1` for SMIR) re-runs one printed
mismatch. The source archive, checksum, retrieval date, and license provenance
are recorded in [PROVENANCE.md](../../docs/specifications/ebpf/PROVENANCE.md).
