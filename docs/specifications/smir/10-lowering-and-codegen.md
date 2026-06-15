# 10. Lowering and Code Generation

## 1. Lowerer trait

```rust
trait SmirLowerer: Send {
    fn target_arch(&self) -> &'static str;
    fn lower_function(&mut self, func: &SmirFunction) -> Result<LowerResult, LowerError>;
    fn code_buffer(&self) -> &CodeBuffer;
    fn finalize(&mut self) -> Result<Vec<u8>, LowerError>;
}
```

`target_arch` describes emitted host machine code, not guest ISA.

## 2. LowerResult

```rust
struct LowerResult {
    code_size: usize,
    entry_offset: usize,
    block_offsets: HashMap<BlockId, usize>,
    relocations: Vec<Relocation>,
    stack_size: usize,
}
```

## 3. CodeBuffer

`CodeBuffer` stores raw emitted bytes or words, current position, labels, fixups, and patch helpers. It can emit little-endian integers, raw bytes, align, clear, define labels, and patch PC-relative/absolute slots.

## 4. Relocations

Relocations support PC-relative 8-bit and 32-bit branches, absolute 32/64-bit slots, block targets, guest-address targets, external symbols, and runtime helpers.

## 5. Lowering errors

A lowerer MUST return an error rather than approximate semantics when it cannot lower an operation. Important errors include unsupported operation, register-allocation failure, undefined label, relocation range failure, invalid operand/register, stack overflow, and internal error.

## 6. x86-64 backend

The x86-64 backend has:

- `X86Emitter`: raw x86 instruction encoding, REX, ModR/M, SIB, SSE, VEX, EVEX, jumps, ALU, memory, and bit operations.
- `X86_64Lowerer`: block/function lowering, register allocation, x86 hints, native exits, helper modes, and jump fixups.
- `X86Cond`: mapping from SMIR `Condition` to x86 condition-code encodings.

The backend uses a mixture of direct instruction selection, peephole fusions, x86 encoding hints, and safety-gated helper calls.

## 7. x86 register allocation

`RegAlloc` is x86-oriented. It maps virtuals and some architecture registers to `PhysReg`, spills to stack when needed, tracks caller/callee-saved use, and computes 16-byte-aligned frame size. It excludes RSP/RBP from general allocation.

The identity-mapped JIT path is more restrictive than the general allocator: because guest GPRs occupy same-named host GPRs, virtual temporary writes can be unsafe and are gate-rejected unless specially folded.

## 8. AArch64 native backend

The AArch64 backend emits native AArch64 instruction words. Runtime support exists for identity-mapped AArch64 execution on AArch64 hosts, including scalar and FP/SIMD trampolines. Coverage is source-defined by `lower/aarch64.rs` and tests.

## 9. AArch64-to-x86-64 state-backed backend

`Aarch64X86_64Lowerer` lowers AArch64-lifted scalar SMIR to x86-64 SysV leaf functions. The host function receives `RDI = *mut Aarch64GuestRegs`. Guest architectural state is loaded from and stored to that struct. Virtual registers are stack slots. This is the implemented direct cross-target lowerer.

## 10. AVX10 lowering component

`Avx10Lowerer` emits EVEX-encoded x86 instructions for AVX10 operation variants. It is a specialized component rather than an entire function ABI. It is selected by host backends that can legally emit AVX10 code.

## 11. Memory helper mode

A lowerer may operate in memory-helper mode. In that mode, `Load` and `Store` become calls through runtime helper function pointers rather than direct host memory operations. Helper failure must bail to a precise native exit.

## 12. Call helper mode

A lowerer may lower guest calls as runtime call-outs. The helper receives current guest state, target PC, and return PC, runs the callee in the interpreter, then either resumes native execution at the continuation or bails to the interpreter.

## 13. Prologue/epilogue constraints

Native prologues and epilogues must preserve guest-observable flags when flags are live. The x86 lowerer uses flag-preserving stack adjustment patterns in paths where guest flags must survive.
