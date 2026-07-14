# 09. Optimization

## 1. Levels

```text
O0: no optimization / debugging
O1: fast basic optimizations
O2: full region optimization used by hot-block JIT paths
```

## 2. Statistics

`OptStats` tracks dead flag elimination, constant propagation, expression folding, dead-op elimination, strength reductions, block merges, redundant loads, vector-alignment inference, copy propagation, and branch folding.

## 3. Frontier-aware liveness

SMIR optimization is region-aware, not whole-program compiler optimization. A region may return to the interpreter at any frontier. At such an exit, every touched architectural register and relevant flag must be treated as live-out.

This prevents transforms such as dead-code elimination from deleting the final write to a guest register just because no later block in the same region reads it.

## 4. Register liveness

The liveness transfer function walks block operations backward:

1. remove fully-defined destinations from the live set;
2. add source registers;
3. for partial x86 writes, keep the destination live because the write reads the previous upper bits;
4. merge terminator uses;
5. propagate successor live-ins for internal edges;
6. seed all touched architecture state at region exits.

## 5. Flag liveness

Flag liveness mirrors register liveness. `flags_must_write` and `flags_read` metadata must be correct for every opcode. A transform may set `FlagUpdate::None` only when none of the written flags are live.

## 6. Constant propagation and folding

Constant propagation is block-local and conservative. It tracks full-width definitions. x86 8/16-bit partial writes invalidate constants because upper bits are preserved and may be unknown. W32 and W64 definitions can be tracked as full architectural writes.

Constant folding that changes flag behavior is legal only when flags are dead or the replacement is flag-equivalent.

## 7. Dead-code elimination

DCE must preserve:

- memory effects;
- flag effects when live;
- architectural register writes live at exits;
- traps/exceptions;
- system effects;
- helper calls;
- exact semantic escape operations.

## 8. Branch folding

Branch folding may remove unreachable blocks only when terminator condition semantics are known and all removed blocks are not externally reachable region exits.

## 9. Redundant-load elimination

Ordinary guest-memory reads are observable: they can fault, access MMIO, or return a changing device value even when their effective address is unchanged. O2 therefore preserves repeated loads by default. Load forwarding is enabled only when `FunctionAttrs::allow_redundant_load_elimination` explicitly establishes that all ordinary loads in the function are non-faulting, non-volatile, and stable until an intervening SMIR memory write. Address, width, and signed/zero-extension mode are all part of the forwarding key.

## 10. Complexity

Let `B` be blocks, `E` edges, `N` operations, and `R` live registers/flags. The implemented frontier liveness fixpoint is bounded by approximately `B + 2` iterations and costs `O((B + 2)·(N + E + R))` time with `O(B·R)` live-set space.
