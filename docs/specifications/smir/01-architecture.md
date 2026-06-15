# 01. Architecture

## 1. Layering

SMIR separates source decode, semantic representation, optimization, host lowering, and execution runtime.

```text
Source ISA bytes or already-decoded instructions
  ├── x86-64: variable-length decoder and lifter are interleaved
  ├── AArch64: existing ARM decoder feeds the lifter
  ├── Hexagon: packet-aware decoded form plus direct opcode fallback
  ├── RISC-V: 16/32-bit decode, extension-gated lifting
  └── AVX10: EVEX-visible vector semantics
        │
        ▼
LiftResult { ops, bytes_consumed, control_flow, branch_targets }
        │
        ▼
SmirBlock { phis, ops, terminator }
        │
        ▼
SmirFunction / SmirModule
        │
        ├── interpreter
        ├── optimizer
        └── lowerer → CodeBuffer → ExecMem/runtime
```

## 2. Required invariants

1. A lifter MUST encode guest semantics, not host instructions.
2. A `SmirOp` MUST be sequentially visible within its block.
3. A `Terminator` MUST be the only control-flow exit from a block.
4. Architectural register identity MUST be preserved in `ArchReg`; host mapping is the lowerer's responsibility.
5. Memory effects MUST go through `SmirMemory`, direct lowering only when the lowerer/runtime proves equivalent, or helper calls.
6. Region exits MUST preserve architectural state needed by the interpreter.
7. Unknown or unproven native lowering MUST fail closed and return to interpretation.

## 3. Execution modes

| Mode | Input | State | Output |
|---|---|---|---|
| Direct interpreter | Cached/lifted `SmirBlock` | `SmirContext` + `SmirMemory` | `BlockResult` / `ExitReason` |
| x86-64 identity native | x86-family SMIR region | `GuestRegs`, host GPR identity map | mutated `GuestRegs`, optional `exit_pc` |
| AArch64 identity native | AArch64 SMIR region on AArch64 host | `Aarch64GuestRegs`, host X/V identity map | mutated `Aarch64GuestRegs` |
| AArch64 state-backed x86 | AArch64 SMIR region on x86-64 host | `Aarch64GuestRegs` pointed to by `RDI` | mutated `Aarch64GuestRegs` |
| Specialized target lowering | subset ops such as AVX10 | backend-selected state/register policy | target-specific bytes |

## 4. Design axes

SMIR achieves retargetability by keeping these axes independent:

- **Source ISA**: the architecture whose instruction is lifted.
- **IR semantics**: the operation vocabulary and typed widths.
- **Guest state layout**: `ArchRegState`, `GuestRegs`, or `Aarch64GuestRegs`.
- **Host ISA**: the emitted machine code target.
- **Runtime ABI**: identity register map, state pointer, helper-call protocol, or interpreter fallback.

## 5. Identity versus state-backed execution

Identity-mapped execution loads guest architectural registers into same-numbered host registers. It is fast but restrictive because all host registers may be live guest state. State-backed execution keeps guest registers in a memory struct and emits loads/stores around operations. It is slower but naturally supports cross-ISA lowering.

## 6. Native exits

A native region can terminate at an internal exit stub. The stub records the next guest PC into the runtime state (`exit_pc` for x86 `GuestRegs`, `pc` for AArch64 state) and returns to the trampoline. The interpreter resumes at that PC.

## 7. Helper calls

Lowered code MAY call runtime helpers for:

- memory load/store with fault reporting;
- vector load/store transfer through AArch64 state slots;
- guest call-out / lift-through-calls;
- exceptions and privileged functions where direct lowering is not safe.

Helper calls are part of the lowering ABI. They must preserve or explicitly marshal all guest state that can be observed after the helper returns.
