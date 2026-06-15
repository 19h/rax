# 12. Cross-Target Lowering

## 1. Definition

Cross-target lowering means the source guest ISA and emitted host ISA may differ. SMIR enables this by preserving guest semantics in architecture-tagged IR and delaying host mapping to the lowerer.

## 2. Mechanism

```text
source instruction
    → architecture-specific lifter
    → architecture-neutral / architecture-tagged SMIR
    → optimizer preserving region frontiers
    → host-specific lowerer
    → runtime ABI for guest state
```

The key is that `ArchReg::Arm(X0)` does not become a host x86 register until an x86 lowerer decides how to represent it.

## 3. Identity mapping

Identity mapping is used when guest and host have compatible register files and the backend chooses speed over generality. Example: x86 guest on x86 host, AArch64 guest on AArch64 host.

Pros:

- low per-op marshalling cost;
- native instructions can operate directly on guest values;
- hot loops can be extremely fast.

Cons:

- no free scratch registers unless reserved;
- guest stack pointer may conflict with host stack;
- virtual temporaries can clobber guest registers;
- lowerer must reject many otherwise-valid SMIR functions.

## 4. State-backed mapping

State-backed lowering stores guest architecture state in a memory struct. Example: `Aarch64X86_64Lowerer` with `Aarch64GuestRegs` and `RDI` state pointer.

Pros:

- guest/host ISA mismatch is straightforward;
- virtual temporaries can live in host stack slots;
- reserved-host-register hazards are explicit;
- easier to support guest state not present in host register file.

Cons:

- more memory traffic;
- more ABI code;
- every architecture register access must be loaded/stored or cached safely.

## 5. AArch64 guest to x86-64 host

The implemented state-backed lowerer:

1. scans the function for virtual registers;
2. allocates stack slots for virtuals;
3. emits an x86-64 SysV prologue;
4. uses `RDI` as `*mut Aarch64GuestRegs`;
5. loads/stores X/SP/PC/NZCV/FP state through fixed offsets;
6. emits x86 arithmetic, logic, branch, load/store, exclusive, and atomic sequences for supported scalar operations;
7. updates `pc` at exits;
8. rejects unsupported operations.

## 6. AArch64 host identity lowering

The AArch64 host path uses AArch64 native instruction emission and a trampoline that maps guest X registers and NZCV into host registers. The FP/SIMD trampoline additionally marshals V registers and FPCR/FPSR.

## 7. x86 guest on AArch64 host

The root README claims groundwork for x86-on-ARM through the AArch64 host backend. The source baseline includes AArch64 runtime support. Exact integrated coverage for x86 guest semantics on AArch64 is source-defined by `lower/aarch64.rs` and its tests.

## 8. Memory cross-targeting

Memory is the most important cross-target boundary. A lowerer must not assume host virtual address equals guest virtual address unless that is proven by the machine backend. Helper calls are the general solution for MMU, MMIO, permissions, page faults, and self-modifying code.

## 9. Flags cross-targeting

Flags may be:

- held in native host flags;
- recomputed into guest state;
- materialized from lazy descriptors;
- stored in architecture-specific fields (`rflags`, `nzcv`);
- elided when dead.

Cross-target lowerers must handle guest flag conventions even when the host has different flags.

## 10. Failure model

Cross-target lowering is partial by design. Failure to lower a region is not an emulator failure; it is a request to run the interpreter. This is a correctness-preserving boundary.
