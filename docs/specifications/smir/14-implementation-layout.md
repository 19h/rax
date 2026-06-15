# 14. Implementation Layout

## 1. Core module

`src/smir/mod.rs` exposes the SMIR public surface and re-exports the commonly used types.

## 2. Core semantic files

| File | Responsibility |
|---|---|
| `types.rs` | IDs, source architecture, `VReg`, `ArchReg`, registers, widths, addressing, operands, memory orders, FP/vector/condition types. |
| `ir.rs` | Modules, functions, blocks, phis, terminators, call targets, runtime functions, traps, builder. |
| `ops.rs` | `SmirOp`, `OpKind`, x86 hints, opcode metadata, JIT whitelist, source/destination analysis helpers. |
| `flags.rs` | Lazy flags, materialized flags, RFLAGS/NZCV conversion, condition materialization. |
| `context.rs` | `SmirContext`, architecture register states, virtual register file, exit/debug state. |
| `memory.rs` | `SmirMemory`, `MemoryReader`, flat memory, atomics, exclusive monitor, helper functions. |

## 3. Lifters

| File | Responsibility |
|---|---|
| `lift/mod.rs` | Common trait, lift result, control-flow type, lift context. |
| `lift/x86_64.rs` | x86-64/APX/VEX/EVEX-aware lifting. |
| `lift/aarch64.rs` | AArch64 decoder-backed lifting. |
| `lift/hexagon.rs` | Hexagon packet/HVX/scalar lifting. |
| `lift/riscv.rs` | RISC-V RV32/RV64 extension-gated lifting. |
| `lift/avx10.rs` | AVX10-visible operation lifting support. |

## 4. Execution and optimization

| File | Responsibility |
|---|---|
| `interp.rs` | Direct interpreter over cached SMIR blocks. |
| `opt.rs` | Optimization levels and passes. |

## 5. Lowering

| File | Responsibility |
|---|---|
| `lower/mod.rs` | Lowerer trait, result, relocations, runtime-helper enum, code buffer, lower errors. |
| `lower/regalloc.rs` | x86-oriented physical register model and allocator. |
| `lower/x86_64.rs` | x86-64 emitter and lowerer. |
| `lower/aarch64.rs` | native AArch64 lowerer. |
| `lower/aarch64_x86.rs` | state-backed AArch64 guest to x86-64 host lowerer. |
| `lower/avx10.rs` | EVEX/AVX10 lowering component. |
| `lower/runtime.rs` | native execution runtime, trampolines, executable memory, safety gates. |

## 6. Integration outside `src/smir`

The x86-64 VM run loop, MMU, SMC dirty-page journal, and hot-region cache live outside `src/smir`. They are integration-layer responsibilities that call into SMIR lifters, optimizer, lowerers, and runtime.

## 7. Ownership boundaries

- Lifters own source-ISA decode-to-SMIR translation.
- `ops.rs` owns opcode metadata and semantic names.
- Interpreter owns canonical execution when no native proof exists.
- Optimizer owns semantics-preserving transforms.
- Lowerers own host-specific instruction selection and ABI.
- Runtime owns executable memory and entry/exit marshalling.
- Machine backends own hotness, cache invalidation, MMU helper implementations, and fallback dispatch.
