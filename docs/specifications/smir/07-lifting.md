# 07. Lifting

## 1. Trait

```rust
trait SmirLifter: Send {
    fn source_arch(&self) -> SourceArch;
    fn lift_insn(addr, bytes, ctx) -> Result<LiftResult, LiftError>;
    fn lift_block(addr, mem, ctx) -> Result<SmirBlock, LiftError>;
    fn lift_function(entry, mem, ctx) -> Result<SmirFunction, LiftError>;
}
```

## 2. LiftResult

```rust
struct LiftResult {
    ops: Vec<SmirOp>,
    bytes_consumed: usize,
    control_flow: ControlFlow,
    branch_targets: Vec<GuestAddr>,
}
```

## 3. ControlFlow

`ControlFlow` models fallthrough, direct branch, conditional branch, register/memory indirect branch, call, return, trap, and syscall. It is an instruction-level result used by block/function lifting to create terminators and discover successor blocks.

## 4. LiftContext

`LiftContext` carries:

```text
arch
vreg_alloc
block_alloc
guest_pc
endian
known_functions
symbols
block_cache
extended_imm
```

Hexagon uses `extended_imm` for constant extenders. Other architectures use it as inert state.

## 5. x86-64 lifting

The x86 lifter interleaves decoding and lifting due to variable-length instruction encoding. It handles legacy prefixes, REX, REX2/APX, VEX/EVEX/APX-EVEX forms, segment overrides, operand/address-size overrides, LOCK/REP prefixes, APX no-flags behavior, and x86-specific flag update masks.

The lifter can run in strict mode and supports lift-through-calls with a block cap. Lift-through-calls is used by JIT call-helper mode: the call is represented in SMIR, while the continuation is also lifted.

## 6. AArch64 lifting

The AArch64 lifter consumes the existing ARM decoder. It maps `SP` versus `XZR/WZR`, AArch64 conditions to SMIR `Condition`, shifts to `ShiftOp`, extends to `ExtendOp`, and supported system registers such as NZCV/FPCR/FPSR to `ArmReg` storage.

MTE tag-clearing add/sub forms are represented by arithmetic plus masking. Memory operands are translated to `Address` plus pre-ops when an index calculation must be materialized.

## 7. Hexagon lifting

The Hexagon lifter is packet-aware. It tracks packet-local producers for `.new` values, packet start PC for packet-relative branches, constant extenders, pending histogram ops awaiting same-packet `.tmp` vector loads, HVX vector and Q registers, control-register value transfers, GP-relative addressing, and post-increment/circular/bit-reversed addressing forms.

Unknown decoded scalar words may be re-decoded at opcode level and lifted when a regular scalar operation can be represented.

## 8. RISC-V lifting

The RISC-V lifter supports RV32/RV64 selection and extension configuration. It handles RV64I, M, A, F, D, C, and selected bitmanip/crypto/vector/CSR behavior. Some FP, crypto, and RVV instructions lift to architecture-exact SMIR operations rather than generic arithmetic.

## 9. Error handling

A lifter may fail with invalid encoding, unsupported instruction, memory error, incomplete instruction, or internal error. Strict mode must fail on unsupported instructions instead of silently emitting approximate semantics.

## 10. Lifter obligations

A lifter MUST:

1. set `guest_pc` accurately on emitted ops;
2. report `bytes_consumed` accurately;
3. emit correct `ControlFlow`;
4. include direct branch targets when known;
5. preserve architectural side effects, including flags, predicates, post-increment, system registers, and implicit operands;
6. reject instead of approximating when semantics cannot be represented.
