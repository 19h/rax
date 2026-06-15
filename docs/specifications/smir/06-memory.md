# 06. Memory

## 1. Traits

`SmirMemory` defines execution memory:

```rust
read(addr, buf)
write(addr, data)
atomic_load(addr, size, order)
atomic_store(addr, value, size, order)
compare_and_swap(addr, expected, new, size, success_order, failure_order)
atomic_rmw(addr, op, operand, size, order)
load_exclusive(addr, size)
store_exclusive(addr, value, size)
clear_exclusive()
fence(kind)
prefetch(addr, write)
probe(addr, size, write)
```

`MemoryReader` is the read-only interface used by lifters.

## 2. Errors

Memory operations may produce:

```text
PageFault { addr, write, user }
AccessViolation { addr, write }
Alignment { addr, required }
Mmio { addr, size }
ExclusiveFailed
OutOfBounds { addr }
```

The interpreter maps memory errors to `ExitReason::MemoryFault`. Native helper paths must preserve precise guest PC for restart.

## 3. Addressing

All memory ops use the `Address` enum. Lowerers must compute the same effective address as the source architecture.

- x86 FS/GS use `SegmentRel` and runtime FS/GS base fields.
- AArch64 base+offset and register-indexed forms can expand through temporary `Add` operations.
- Hexagon GP-relative and circular/bit-reversed addressing are represented using `GpRel`, modifier registers, or architecture-specific op forms.
- PC-relative addressing may require relocation-like handling by a lowerer.

## 4. Widths and sign extension

`MemWidth` defines access size. Scalar loads carry `SignExtend::Zero` or `SignExtend::Sign`. Vector loads/stores carry `VecWidth` or architecture-specific sizes.

## 5. Atomics

Atomic operations carry `MemoryOrder` and `AtomicOp`. Interpreters may implement sequential behavior in a single-threaded model; lowerers must respect architecture-required ordering if they emit native atomics.

## 6. Exclusive monitor

AArch64 and RISC-style load-linked/store-conditional semantics are modeled through `ExclusiveMonitor` or runtime-state fields. A store to an overlapping monitored region must clear or fail the monitor according to the source architecture.

## 7. Memory helper lowering

Native JIT memory operations may lower to helper functions rather than direct host memory accesses. The helper must report success/fault separately from the loaded value. x86 helper ABI uses value and ok in machine return registers; AArch64 helper ABI uses AAPCS64-compatible two-eightbyte returns for scalar loads and state-mediated helpers for vector load/store.

## 8. Self-modifying code

Writes to executable/code pages must invalidate corresponding decoded/lifted/native caches. The root README describes dirty-page journal eviction for compiled blocks. A native store helper must be able to bail when a store writes a code page so the interpreter can revalidate decode/lift state.

## 9. Direct-host-pointer lowering

Direct host-memory lowering is valid only when:

1. the guest memory mapping is flat or otherwise proven equivalent;
2. alignment/fault/MMIO behavior is preserved or impossible;
3. self-modifying-code invalidation is not bypassed;
4. the region safety gate permits it.

Otherwise, memory must use helpers or the interpreter.
