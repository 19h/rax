# eBPF formal verification (Kani)

Bounded model checking of the rax eBPF implementation with
[Kani](https://github.com/model-checking/kani) 0.68.0 (CBMC 6.11.0). Each
proof leaves every instruction field, every register, and the data memory
symbolic, so a successful proof covers every value, not a sample.

```sh
python3 tools/ebpf-formal/run.py                  # the discharged families
python3 tools/ebpf-formal/run.py --harness direct_le_3f
python3 tools/ebpf-formal/run.py --all            # also the intractable ones
```

CI ([`ebpf-validation.yml`](../../.github/workflows/ebpf-validation.yml))
runs the default families on `ubuntu-24.04-arm` in 14 shards of `--harness`
filters (RFC 9669 by byte order and opcode nibble, the step and memory
proofs, SBPF admission by version). The `ci_actions_pinned` contract checks
that the shards select each default-family harness exactly once.

Kani's C library model does not link on macOS, so `run.py` runs the proofs
in a Linux arm64 container built from the pinned `rust` image in
`BASE_IMAGE` and Kani 0.68.0. The package is outside the engine workspace
and has its own lockfile. The SBPF proofs compile the pinned Anza
`solana-sbpf` source, which `run.py` verifies and extracts with
`tools/ebpf-reference/run.py`'s `extract`.

| File | Role |
|---|---|
| `gen_registry.py` -> `src/registry.rs` | RFC 9669 Appendix A (Table 18), parsed from `docs/specifications/ebpf/rfc9669.txt` (checksum verified); the admission specification |
| `src/spec.rs` | Independent one-instruction semantics written from RFC 9669 sections 2-5, and the section 5.2 sign-extending loads that Table 18 omits (`SECTION_5_2`) |
| `gen_proofs.py` -> `src/proofs/opcodes.rs`, `src/sbpf/opcodes.rs` | One harness per opcode byte, byte order, and plane (RFC 9669); per SBPF version, opcode byte, and plane |
| `src/proofs/mod.rs` | The symbolic RFC 9669 machine and the direct-CPU check |
| `src/proofs/fetch.rs` | The rest of `EbpfCpu::step`: meter, fetch, and the bytes handed to the decoder |
| `src/proofs/memory.rs` | `ProofMemory`, and the proof that `FlatMemory` meets it |
| `src/proofs/operators.rs` | The functionally consistent division and remainder operator |
| `src/proofs/smir.rs` | Lift, optimize at O0/O1/O2, and interpret one guest instruction |
| `src/sbpf/mod.rs` | SBPF V0-V4 against the Anza `RequisiteVerifier` and `Interpreter::step` |
| `src/sbpf/smir.rs` | SBPF SMIR at O0/O1/O2 against the direct CPU on the same domain |

## Discharged families (the default)

**RFC 9669 direct plane** (`direct_{le,be}_XX`, 512 harnesses). For each
opcode byte and byte order:

1. `decode_insn` accepts an encoding exactly when the registry admits it under
   the platform contract in `src/spec.rs`: no packet group, `r10` is a
   read-only frame pointer, and unused fields are zero (section 3.1),
   including an LDDW `next_imm` that Table 12 does not use. RFC 9669 is
   inconsistent here: section 5.2 defines the sign-extending loads 0x81,
   0x89, and 0x91, but Table 18 has no rows for them, so `spec.rs` adds
   them from the normative section 5.2.
2. One `EbpfCpu::execute_decoded` step on an admitted instruction yields the
   specified registers, program counter, memory, fault (start address,
   width, direction), exit, helper exit, program-local call frame, or
   pseudo-load trap.

**Fetch and meter** (`proofs::fetch`). The rest of `EbpfCpu::step` for every
PC, text range, meter state, and byte order: the meter traps exactly when
exhausted and otherwise charges before the fetch, and the decoder receives
exactly the 8 (or 16) bytes at the PC, or the step traps precisely.

**Memory contract** (`proofs::memory`). The engine reaches memory only
through the `EbpfMemory` and `SmirMemory` methods named there; `FlatMemory`
returns the same result and leaves the same bytes as `ProofMemory` for every
address, width, value, and byte order.

**SBPF admission** (`sbpf_admission_vN_XX`, 1,280 harnesses). For each
version and opcode byte, `verify_sbpf` returns the `RequisiteVerifier`
verdict (variant and slot) for every value of the other instruction bits and
of an LDDW second slot, with the instruction in the middle of a four-slot
text and as the last slot.

## Model assumptions

- Hash-map seeding is stubbed: `RandomState::new` returns a fixed key, and the
  reference function registry's `thread_rng` returns a zeroed generator.
  Neither seed changes a lookup result.
- Division and remainder: the engine's `rax::isa::ebpf::arith` primitives and
  the specification's `spec::ops` are each exactly the Rust operator, and
  the direct-plane harnesses replace both with the functionally consistent
  operator of `src/proofs/operators.rs`. It returns equal results for equal
  operands and independent results otherwise, so it over-approximates the
  real operator: the proofs show both sides apply the same operation to the
  same operands under the same zero-divisor and signed-overflow guards. It
  asserts the Rust operator's domain, so an unguarded call still fails.
- Multiplication is not abstracted. SAT equates two multipliers only when
  they have the same structure, and CBMC bit-blasts a signed multiply
  through sign-magnitude negation, so the engine's 32-bit `MUL` computes the
  product as `u32::wrapping_mul` (equal low 32 bits) like `spec.rs`. With
  `i32::wrapping_mul` the four 32-bit `MUL` harnesses do not finish.
- The reference `MemoryMapping` is built with its default (no-op)
  access-violation handler passed as a closure; Kani 0.68 cannot box a
  function item.
- Unwinding assertions are enabled: a loop bound that is too small fails the
  proof instead of truncating it.

## Not discharged (`--all`)

The SMIR harnesses (`smir_*`, `sbpf_smir_*`) and the SBPF execution
harnesses (`sbpf_step_vN_XX`, `sbpf_divider_vN_XX`) are defined but were not
discharged on 16 cores and 64 GiB.

The SMIR lifter and interpreter keep blocks and functions in `std` hash
maps, and CBMC's symbolic execution does not resolve hashbrown's bucket
iteration and probe loops even for concrete keys: each loop unrolls to the
bound under a growing path guard. The lifter finds a block's successors by
iterating its block cache (`LiftContext::block_cache.iter().find(..)`). A
symbolic-execution profile (`--show-goto-symex-steps`) was taken of a probe
variant of `smir_o0_le_07` (not kept) with admission and the register
fields made concrete, so the IR has one shape. Its guard grew to 77,326 terms inside that iteration before
the lifter returned, and the run did not finish in about one CPU-hour at
11 GiB. The fully symbolic harness did not finish in 2.5 CPU-hours. Heap
storage as such is not the obstacle: a dispatch on an operation read back
from a `Vec` (presized, grown, or cloned) prunes its unreachable arms (6,375
steps, against 12,451 with the arm reachable). A `BTreeMap` holding two
concrete entries did not finish symbolic execution in 30 minutes. A second
probe replaced every map and set on the eBPF lift and interpret path with a
verification-only linear map under `cfg(kani)` (not kept): hashbrown left
the profile and the guard stayed at most 18 terms, but symbolic execution
then ran at about four calls per second at 7-18 GiB constructing IR values
(`OpKind` has 365 variants; register and context types span every ISA), and
no probe finished lifting one instruction (the longest ran 41 minutes).

The SBPF execution harnesses fetch from a static text (`TEXT`, so both
interpreters dispatch on a concrete opcode, with
`sbpf::fetch_matches_the_mapping` relating the engine's fetch to its
mapping). Four probes grew to 8-10 GiB each plus swap and were stopped
after about 74 CPU-minutes when the machine ran out of memory. The divider
family would also equate the reference's and the engine's divider
circuits, which bit-level SAT cannot do: the same 64-bit divider applied
twice to the same operands times out after 15 minutes with CaDiCaL and
with Kissat 4.0.1, and after 400 s with Kissat's congruence closure and
complete sweeping on the isolated CNF. CBMC's SMT backends fail on these
models: Bitwuzla reports `ERROR` for the division checks, the SMT2 path
with Z3 aborts, and the incremental SMT2 backend cannot encode `bswap`,
`overflow_result`, or `r_ok`. The equivalence evidence for these planes is
the differential replay of every corpus on every plane (`ebpf_diff`).

SMIR harnesses reduce the interpreter model without changing eBPF semantics
(`src/proofs/smir.rs`): the dispatch chain's pure forward from
`execute_op_x86_stack_flags` through `execute_op_binary` is replaced by its
target `execute_op_flags`, because none of those categories handles an
`OpKind` the eBPF lifter emits; the categories after `system` and the RVV
delegation are replaced by panics, which would fail the proof if reached.
