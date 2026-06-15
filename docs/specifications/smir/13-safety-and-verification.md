# 13. Safety and Verification

## 1. Fail-closed policy

SMIR native execution must fail closed. A region is native-executable only if every relevant gate accepts it and lowering succeeds. Otherwise execution remains in or returns to the interpreter.

## 2. JIT-safe opcode whitelist

`OpKind::is_jit_safe` is a conservative whitelist for the x86 hot-block JIT. It allows validated register/immediate integer-core operations and excludes memory, stack/string/atomic, division gaps, FP/SIMD, system, flag-plumbing, and unvalidated operations unless a separate mode explicitly handles them.

## 3. Clobber safety

Identity-mapped native execution has no ordinary scratch GPRs because host registers carry guest values. The clobber gate rejects:

- virtual-temp writes;
- non-whitelisted operations;
- unsafe guest RSP/RBP reads/writes in x86 identity mode;
- unsupported memory forms unless helper mode permits simple load/store.

A trailing `TestCondition` feeding a `CondBranch` may be exempt because the lowerer can fold it into a direct host branch without materializing a temp.

## 4. Reserved-register safety

AArch64 identity mode reserves platform/state/link/stack-related registers. Regions touching reserved registers must be rejected or handled by a state-backed strategy.

## 5. Memory safety

Native memory lowering must preserve:

- page-fault precision;
- write detection for SMC/code pages;
- MMIO behavior;
- permission checks;
- segment bases;
- exclusive monitors;
- atomic order constraints.

If it cannot, it must use helpers or reject.

## 6. Host-state hygiene

Trampolines must preserve host ABI obligations and restore/sanitize host-visible flags/control registers. The AArch64 FP/SIMD trampoline restores host FPCR/FPSR and callee-saved vector state. The x86 trampoline sanitizes host EFLAGS bits after guest execution.

## 7. Differential verification

The project’s declared verification model compares architectural state against external oracles:

- x86-64 against KVM/real hardware;
- AArch64, Hexagon, and RISC-V against QEMU harnesses;
- APX encodings against LLVM assembler output and documented semantics where no silicon/QEMU oracle exists.

SMIR lifter tests also compare lifted/interpreted SMIR execution against architecture interpreters for RISC-V and Hexagon according to the root README.

## 8. Runtime verification

`RAX_JIT_VERIFY=1` is described as re-running native regions in the interpreter and diffing state. A backend adding new native coverage should provide a similar differential path or prove equivalence through tests.

## 9. Required tests for new native lowering

A new native lowering rule SHOULD include:

1. unit tests for byte/word emission if applicable;
2. execution tests via `ExecMem` or target trampoline;
3. interpreter/native differential tests;
4. fault-path tests for memory operations;
5. flag-differential tests;
6. boundary tests for widths, sign bits, zero, overflow, and carry/borrow;
7. reserved-register/clobber-gate tests.

## 10. Correctness hierarchy

When behavior differs:

1. external oracle or architecture manual semantics wins;
2. source interpreter semantics should match oracle;
3. SMIR lifter/interpreter must match source interpreter/oracle;
4. native lowering must match SMIR interpreter;
5. optimizer must preserve SMIR interpreter semantics.
