[← Documentation home](../../README.md)

# Generated suites

Large instruction sets cannot be maintained reliably as a hand-written list in the root README. `rax` therefore uses generated source, cases, and inventories. Generated material provides breadth only when its provenance and reachability are explicit.

## Roles of generated material

Generated artifacts serve several different purposes:

- **encoding cases:** exact bytes and operands accepted by an assembler/specification source;
- **semantic cases:** inputs and expected outputs derived from a reference model;
- **coverage inventories:** finite sets of mnemonics/forms expected to be implemented or explicitly unimplemented;
- **oracle corpora:** cases serialized for external execution and comparison;
- **source invariants:** assertions that implementation files and manifests agree;
- **test inclusion:** Rust modules included by a registered aggregate target.

These roles should not be conflated. An encoding corpus proves neither reachability nor semantics by itself.

## Directory contract

```text
tests/generated/
```

contains checked-in generated Rust or data. Those files are not Cargo integration-test roots. They execute only when an explicitly registered suite includes them.

A generated file is therefore described by two edges:

```text
source corpus/specification → generator → generated artifact
registered suite → include/module edge → generated artifact
```

Both must exist.

## Arm generation

The repository includes `tools/asl-parser/` and architecture references under `docs/architecture/arm/`. The generated Arm corpus covers selected AArch64, SVE/SVE2, NEON, and AArch32/Thumb spaces.

A defensible regeneration record includes:

- exact ASL/specification snapshot or corpus revision;
- parser/generator commit;
- command and feature flags;
- assembler version when LLVM supplies encodings;
- accepted/rejected case counts;
- reasons for filtered encodings;
- output path;
- aggregate test that includes the result;
- expected inventory delta.

Register-only generated sweeps must state that they do not cover memory, privilege, exception delivery, or machine integration unless the harness explicitly models those dimensions.

## x86 Intel corpus and generated inventories

The x86 generated system uses a checked-in instruction/intrinsics corpus and source inventory checks to track modern SIMD/EVEX/AVX-512 families and the set still marked unimplemented. The `x86_64-suite` Cargo feature enables the generated aggregate.

Relevant target families include:

- `x86_64_avx512_inventory`;
- `x86_64_unimplemented_manifests`;
- `x86_64_unimplemented_source_inventory`;
- `x86_64_evex_qemu_diff`;
- `x86_64_apx_map4_qemu_diff`;
- `x86_64_unimplemented_qemu_diff`.

A coverage inventory can say that every item in corpus `C` is classified. It cannot say that corpus `C` is the entire architecture unless the corpus source and filters establish that.

## APX generation

LLVM can provide authoritative assembled bytes for APX syntax it accepts. Store the exact bytes in the generated case and distinguish encoding validation from semantic validation.

Until an execution reference runs the cases:

- LLVM validates assembly/encoding provenance;
- direct expected-state tests validate documented semantics;
- a QEMU differential target may remain staged and self-skipping;
- public docs must not report the staged target as an executed oracle.

## Hexagon and RISC-V corpora

Hexagon and RISC-V differential tools generate or consume compact cases for QEMU user-mode reference programs. Their reproducibility record should include the cross-compiler/QEMU versions and serialization format. Packet, vector, and memory cases need separate generators or explicit combinations because their state spaces differ substantially from scalar register-only cases.

## Generated manifests

A manifest should classify each item into mutually intelligible states, for example:

```text
implemented and reachable
implemented but feature/profile gated
intentionally unsupported
not yet implemented
reference cannot execute
undefined/reserved
excluded by generator bug/workaround
```

Do not collapse every non-passing item into “unimplemented.” A reserved encoding, missing host feature, unsupported reference, and decoder gap require different action.

## Regeneration workflow

1. Start from a clean worktree.
2. Record tool versions.
3. Run the generator with the documented command.
4. Inspect counts and warnings.
5. Diff generated artifacts.
6. Explain every semantic inventory change.
7. Run the aggregate that includes the generated file.
8. Run external differential cases where prerequisites exist.
9. Confirm no generated file became unreachable.
10. Commit generator and output changes together unless the workflow explicitly separates them.

A generated diff that only reformats thousands of lines can hide a semantic change. Keep stable ordering and deterministic formatting.

## Review checklist

- Is the source corpus licensed and checked in or pinned?
- Is the generator deterministic?
- Are tool versions or formats pinned tightly enough?
- Are rejected/filtered cases counted?
- Can an empty corpus accidentally produce a green run?
- Does a registered target actually include the artifact?
- Are undefined fields masked before comparison?
- Are instruction bytes printed on failure?
- Are random seeds reproducible?
- Does the generated manifest agree with reachable source?
- Did public coverage language become stronger than the evidence?

## Documentation rule

The root README should summarize architecture families and link here. Exhaustive instruction lists belong in generated inventories, architecture pages, or machine-readable artifacts—not as manually maintained marketing copy.
