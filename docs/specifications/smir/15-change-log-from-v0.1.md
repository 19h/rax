# 15. Change Log from SMIR v0.1

## 1. JIT status changed

Old spec: JIT Tier 1 and Tier 2 marked future.  
Current source: `smir-jit` is a default feature; lowering and runtime infrastructure are implemented.

## 2. Runtime expanded

Old spec: no concrete native runtime ABI.  
Current source: `GuestRegs`, `Aarch64GuestRegs`, x86-64 and AArch64 trampolines, W^X / MAP_JIT executable memory, helper pointers, and native exit protocols exist.

## 3. Opcode set expanded

Old spec: approximate operation catalog.  
Current source: large `OpKind` catalog with APX, AVX10, HVX, RISC-V FP/crypto/vector exact ops, system ops, flag ops, and memory/atomic/exclusive variants.

## 4. Cross-target lowering exists

Old spec: “JIT-ready” design.  
Current source: `Aarch64X86_64Lowerer` lowers AArch64 SMIR to x86-64 SysV functions with a state pointer; native AArch64 lowerer/runtime support also exists.

## 5. Safety gates formalized

Old spec: not detailed.  
Current source: `OpKind::is_jit_safe`, clobber safety gates, native-exit exclusions, RSP/RBP restrictions, memory-helper allowances, and fail-closed behavior are explicit.

## 6. Optimizer became emulator-region-aware

Old spec: general dead flags and constants.  
Current source: optimizer has frontier-aware liveness so architectural state is live at interpreter re-entry frontiers.

## 7. x86 lifting changed

Current source accounts for REX2/APX, APX-EVEX, ADX carry variants, segment overrides, strict mode, and lift-through-calls.

## 8. AArch64 lifting changed

Current source accounts for system registers, MTE tag-clear masks, SP/XZR distinctions, conditions, shifted/extended operands, and memory pre-ops.

## 9. Hexagon lifting changed

Current source accounts for packet-local `.new` producer tracking, histogram deferral, HVX V/Q registers, control-register transfer modeling, GP-relative and special addressing, and direct opcode fallback for formerly unknown decoded words.

## 10. RISC-V lifting changed

Current source includes extension configuration and exact helper forms for FP, crypto/bitmanip, RVV, and CSR/vector state behavior.

## 11. Memory changed

Current source includes richer memory errors, atomics, exclusive monitor behavior, helper-backed native memory, vector memory helpers, and SMC/JIT invalidation obligations.

## 12. Documentation status changed

This folder supersedes the original v0.1 docs. The original docs are retained only as historical context unless replaced in the repository.
