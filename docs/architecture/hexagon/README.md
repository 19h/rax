[← Documentation home](../../../README.md)

# Qualcomm Hexagon architecture

The Hexagon path is a software CPU and bare-metal machine target. It models scalar instructions, packet execution, control flow, memory, floating point, and HVX vector state, with several differential suites using `qemu-hexagon` as the external reference.

## Public ISA selector

The current CLI/TOML selector accepts:

```text
v4, v5, v55, v60, v62, v65, v66, v67, v68, v69
```

Default: `v68`.

This is an important interface boundary. Broader prose in the old root README described “V73,” but the public selector currently stops at `v69`, and the checked-in architecture references prominently include V68/V69 material. Until the public enum, source reachability, generated manifest, and tests agree on a later revision, documentation must not present `--hexagon-isa v73` as valid.

## Launching a bare-metal ELF

```sh
cargo build --release --no-default-features

./target/release/rax \
    --arch hexagon \
    --backend emulator \
    --kernel program.elf \
    --hexagon-isa v68 \
    --hexagon-endian little
```

The ELF entry point and segment addresses should normally control loading. The optional overrides are:

```text
--hexagon-entry <ADDRESS>
--hexagon-load-addr <ADDRESS>
```

Use them for a raw or intentionally relocated workload, not as a default workaround for an incorrect ELF.

## Architectural state

The Hexagon state represented by the implementation and tests includes:

- scalar general registers;
- predicate registers;
- user/status state;
- program-counter and control-flow state;
- loop registers and loop execution state;
- vector registers V0–V31;
- vector predicates Q0–Q3;
- packet staging and commit state;
- memory and selected architectural helper state.

A differential harness can compare only what its serialization format exports. “Zero divergence” means no difference in that projection for the executed corpus, not proof over hidden state or every input.

## Packet semantics

Hexagon is packetized. Instructions grouped into a packet conceptually observe the packet’s old architectural state and commit their effects at packet completion. Correct implementation therefore requires more than executing encodings one after another.

The packet model includes work for:

- parallel reads and packet-end commit;
- scalar `.new` forwarding;
- HVX/vector `.new` forwarding;
- duplex encodings;
- packet predicates;
- multiple stores and their ordering/commit rules;
- hardware loops using loop start/count state;
- circular and bit-reversed addressing;
- control-flow effects at packet boundaries;
- exception/fault behavior without leaking partial architectural commits.

Tests must cover packet combinations, not only isolated instructions, because forwarding and commit behavior are relational properties.

## Scalar execution

The scalar implementation spans ordinary integer arithmetic, logic, compare/predicate operations, shifts, bit manipulation, multiply/accumulate, control flow, loads/stores, addressing modes, loop control, and floating-point/helper families represented by the source and generated corpus.

Specialized operations documented by the former README include CABAC bin decode and reciprocal/square-root seed behavior. Such operations require table and corner-case validation; a mnemonic being decoded is not enough.

## HVX

The HVX surface uses 1024-bit vector registers and vector predicates. The current high-level inventory includes:

- vector ALU and logic;
- compares and min/max;
- multiply and accumulate families;
- widening/narrowing and saturating operations;
- shifts, rounds, and pack/unpack behavior;
- permutes and table operations;
- vector predicate operations;
- histogram/LUT-like instructions;
- vector memory loads and stores;
- `.cur`/`.tmp` and predicate-controlled memory forms represented in the implementation;
- scatter/gather work associated with later publicly selected profiles.

Memory and packet interactions are tested separately from pure register operations because faults, alignment, address generation, masking, and partial effects are different obligations.

## Differential suites

| Cargo target | Scope |
|---|---|
| `hexagon_diff` | scalar instruction comparison |
| `hexagon_cf_diff` | control-flow and packet-flow comparison |
| `hexagon_float_diff` | floating-point and related helper behavior |
| `hexagon_mem_diff` | scalar memory behavior |
| `hexagon_hvx_diff` | HVX register operation behavior |
| `hexagon_hvx_mem_diff` | HVX memory, scatter/gather, and memory-side behavior |
| `hexagon_bare_metal` | end-to-end bare-metal machine boot/execution |
| `hexagon_smir_lift` | Hexagon instruction lift to SMIR compared with the Hexagon interpreter |

The differential tools live under the repository’s Hexagon reference-tooling path and require the relevant compiler/QEMU support. A green command may mean the target skipped because the toolchain was unavailable; inspect the test count and skip output.

## SMIR

The project describes the Hexagon lifter as covering scalar and HVX operations. The lift test interprets the resulting SMIR and compares it with the Hexagon interpreter over the states represented by the suite.

That validates a chain of transformations only over the cases that ran:

```text
Hexagon bytes → Hexagon decode/semantics
             ↘ Hexagon lift → SMIR interpretation
```

It does not automatically establish a live native-JIT machine path for arbitrary Hexagon programs. Lowering support, helper calls, memory boundaries, packet atomicity, and vector width all constrain native execution independently.

## Known limitations and non-claims

- Hexagon is a bare-metal target; no general-purpose OS machine is documented.
- The public ISA selector stops at `v69`; later-revision prose must be reconciled before it becomes a public claim.
- QEMU is an implementation reference, not a formal specification.
- The harness compares an explicit projection of state, not every microarchitectural or unexported detail.
- Packet and memory behavior require combination tests; isolated opcode reachability is insufficient.
- A complete generated opcode inventory, a successful differential corpus, and a supported public ISA revision are three distinct claims.
