# 08. Interpretation

## 1. Interpreter state

`SmirInterpreter` owns a block cache, a function cache, a per-run instruction limit, and a block-ID to guest-address map.

## 2. Run loop

The run loop:

1. checks instruction limit;
2. consumes pending `ctx.exit_reason`;
3. checks breakpoints;
4. fetches the block at `ctx.pc` from cache;
5. executes the block;
6. updates `ctx.pc` on continuation;
7. returns on exit;
8. returns `SingleStep` when debug single-step is enabled.

If a block is not present, the interpreter returns `ExitReason::BlockNotFound` so the caller can lift or dispatch another path.

## 3. Block execution

A block executes its operations sequentially. Memory errors are converted to `ExitReason::MemoryFault` with a best-effort fault address and write flag. After all ops execute, the interpreter executes the terminator.

## 4. Register access

`read_vreg` dispatches:

```text
Virtual(id) → VRegFile
Imm(value)  → value
Arch(reg)   → read_arch_reg(reg)
```

`write_vreg` ignores writes to `Imm`, writes virtuals to `VRegFile`, and writes architectural registers through `write_arch_reg`.

## 5. Architecture state

The interpreter must preserve architecture-specific register semantics:

- x86 GPR indices include APX EGPRs; RFLAGS and FS/GS bases are explicit registers.
- x86 8/16-bit writes merge, 32-bit writes zero-extend, 64-bit writes replace.
- AArch64 X31-as-zero and SP must remain distinct at lift time.
- Hexagon scalar, predicate, vector, Q, modifier, circular, loop, and USR registers are separate state.
- RISC-V x0 reads zero and ignores writes; vector CSRs and FCSR aliases are modeled explicitly.

## 6. Vectors

`VRegFile::VecValue` is large enough to hold current x86 ZMM and Hexagon HVX values in the implementation. Architecture-specific vector register banks are accessed through architecture state helpers or exact op handlers.

## 7. Exact helper ops

The interpreter is the normative execution path for exact operations such as `RvVector`, `RvFp`, `RvIntCrypto`, and Hexagon FP/HVX helper variants unless a backend provides a tested native implementation.

## 8. Termination

Terminators produce either a next PC (`Continue`) or an `ExitReason`. Native lowering must match this behavior by branching internally, recording a native exit PC, or returning to the interpreter.
