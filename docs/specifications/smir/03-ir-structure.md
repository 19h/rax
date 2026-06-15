# 03. IR Structure

## 1. Module

`SmirModule` is the top-level IR container.

```rust
struct SmirModule {
    id: ModuleId,
    source_arch: SourceArch,
    functions: Vec<SmirFunction>,
    symbols: HashMap<String, GuestAddr>,
    externals: Vec<ExternalRef>,
    metadata: ModuleMetadata,
}
```

A module is not required for every JIT region. Hot-block compilation can operate directly on a `SmirFunction`, but module semantics define the multi-function container.

## 2. Function

`SmirFunction` represents a lifted region of guest code.

```rust
struct SmirFunction {
    id: FunctionId,
    entry: BlockId,
    blocks: Vec<SmirBlock>,
    locals: Vec<LocalSlot>,
    guest_range: (GuestAddr, GuestAddr),
    calling_convention: CallingConv,
    attrs: FunctionAttrs,
}
```

For JIT regions, a “function” can be a hot loop or control-flow region, not necessarily an ABI-level guest function.

## 3. Block

`SmirBlock` is the basic scheduling unit.

```rust
struct SmirBlock {
    id: BlockId,
    guest_pc: GuestAddr,
    phis: Vec<PhiNode>,
    ops: Vec<SmirOp>,
    terminator: Terminator,
    exec_count: u64,
}
```

The `guest_pc` is the block entry PC. Individual operations retain their own `guest_pc` for precise fault/restart, tracing, and differential checks.

## 4. Operation

```rust
struct SmirOp {
    id: OpId,
    guest_pc: GuestAddr,
    kind: OpKind,
    x86_hint: Option<X86OpHint>,
}
```

`x86_hint` is non-semantic metadata used by x86 lowering to preserve or choose encodings. Optimizers MUST NOT rely on hints for semantic correctness.

## 5. Phi nodes

`PhiNode` exists for SSA-style entry merging:

```rust
struct PhiNode {
    dst: VReg,
    sources: Vec<(BlockId, VReg)>,
}
```

The current implementation is not a strict full-SSA IR. Phis may be empty in typical lifted blocks. Backend support for phis is source-defined.

## 6. Terminators

```rust
enum Terminator {
    Branch { target },
    CondBranch { cond, true_target, false_target },
    Switch { index, targets, default },
    IndirectBranch { target, possible_targets },
    IndirectBranchMem { addr, possible_targets },
    Return { values },
    Call { target, args, continuation },
    TailCall { target, args },
    Trap { kind },
    Unreachable,
}
```

Terminators MUST be interpreted after all ops in the block. Lowerers MAY fold a terminal condition into host control flow, but must preserve the observable terminator semantics.

## 7. Calls

`CallTarget` supports:

```text
Direct(FunctionId)
GuestAddr(GuestAddr)
Indirect(VReg)
IndirectMem(Address)
Runtime(RuntimeFunc)
```

A JIT region may either end at a call, lower the call to a runtime call-out, or lift through calls when the lifter/lowerer/runtime path is configured for it.

## 8. Traps

`TrapKind` includes breakpoint, undefined, divide-by-zero, overflow, bounds, invalid opcode, system call, and halt. A trap terminator MUST produce the corresponding `ExitReason` or runtime helper path.

## 9. Well-formedness

A well-formed function MUST satisfy:

1. `entry` refers to a block present in `blocks`.
2. Every direct successor in a terminator either refers to a block present in the function or is intentionally treated as a region exit by the optimizer/lowerer.
3. Every op `id` is unique within a block.
4. Every operation source is either an immediate, a virtual defined earlier in a valid dataflow path, or an architecture register.
5. Every architecture register variant must match the function/module `source_arch` unless the op is an explicit cross-architecture helper or test harness construction.
6. A lowerer must reject functions that violate its ABI assumptions.
