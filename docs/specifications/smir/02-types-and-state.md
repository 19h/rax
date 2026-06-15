# 02. Types and State

## 1. Identifiers

SMIR uses compact IDs:

```rust
ModuleId(u64)
FunctionId(u32)
BlockId(u32)
OpId(u16)
VirtualId(u32)
LocalId(u16)
type GuestAddr = u64
```

`OpId` is block-local. `GuestAddr` is always an unsigned 64-bit guest virtual address even for 32-bit source architectures; 32-bit architectures define high-bit behavior at the lifter/interpreter layer.

## 2. Source architectures

Implemented `SourceArch` variants include:

```text
X86_64
Aarch64
Aarch32
Thumb
Hexagon
RiscV64
RiscV32
Mips64
Mips32
Sparc64
Sparc32
```

The architecture object provides default endianness, strict-alignment policy, and register width. x86-64 defaults to little-endian and non-strict alignment. The currently modeled RISC and DSP architectures default to little-endian except MIPS/SPARC variants, which default to big-endian in the source.

## 3. Values

```rust
enum VReg {
    Virtual(VirtualId),
    Arch(ArchReg),
    Imm(i64),
}
```

`Virtual` is an unbounded temporary namespace. `Arch` is guest architectural state. `Imm` is an embedded immediate and MUST NOT be written.

## 4. Architectural registers

```rust
enum ArchReg {
    X86(X86Reg),
    Arm(ArmReg),
    Hexagon(HexagonReg),
    RiscV(RiscVReg),
}
```

### x86 registers

The x86 register model includes GPRs `RAX..R15`, APX EGPRs `R16..R31`, `RIP`, `RFLAGS`, `FsBase`, `GsBase`, vector registers `Xmm/Ymm/Zmm(0..31)`, and opmask `K(0..7)`.

### AArch64 registers

The AArch64 register model includes `X(0..30)`, `Sp`, `Pc`, `Nzcv`, `V(0..31)`, `Fpcr`, `Fpsr`, and encoded system registers. `X31` is not a normal storage register: source/lifter code distinguishes `SP` from `XZR/WZR`; reads of zero register are lifted as `Imm(0)`.

### Hexagon registers

The Hexagon model includes `R(0..31)`, scalar predicates `P(0..3)`, PC/control/data registers (`Pc`, `Gp`, `Lr`, `Sp`, `Fp`, `Lc0/1`, `Sa0/1`, `Usr`), HVX vectors `V(0..31)`, vector predicates `Q(0..3)`, modifier registers `M(0..1)`, and circular-buffer start registers `Cs(0..1)`.

### RISC-V registers

The RISC-V model includes integer `X(0..31)`, floating `F(0..31)`, vector `V(0..31)`, `Pc`, and `Csr(u16)`. `x0` reads as zero and ignores writes.

## 5. Scalar widths

```text
OpWidth: W8, W16, W32, W64, W128
MemWidth: B1, B2, B4, B8, B16, B32, B64
```

Operations carry widths. Integer signedness is operation-specific, not a property of the value type. For example, `MulU` and `MulS` share widths but differ in signedness.

## 6. Addressing

```rust
enum Address {
    Direct(VReg),
    BaseOffset { base, offset, disp_size },
    BaseIndexScale { base, index, scale, disp, disp_size },
    PcRel { offset, disp_size, base },
    GpRel { offset },
    Absolute(u64),
    SegmentRel { segment, base, index, scale, disp },
}
```

`SegmentRel` is used for x86 FS/GS segment-base semantics. `PcRel` carries an optional base PC so lowerers can distinguish semantic PC-relative target addresses from layout-relative host displacements.

## 7. Source operands

```rust
enum SrcOperand {
    Reg(VReg),
    Imm(i64),
    Imm64(i64),
    Shifted { reg, shift, amount },
    Extended { reg, extend, shift },
}
```

`Shifted` and `Extended` encode ARM-style operand transforms. A lowerer may emit a folded host operand when legal or expand it into primitive operations.

## 8. Conditions

`Condition` abstracts x86, ARM, and generic branch/conditional semantics:

```text
Eq, Ne,
Ult, Ule, Ugt, Uge,
Slt, Sle, Sgt, Sge,
Negative, Positive,
Overflow, NoOverflow,
Parity, NoParity,
Always
```

A backend MUST map `Condition` to its native flags or to explicit comparisons. ARM carry and x86 borrow conventions differ; the mapping layer must account for this.

## 9. Execution context

`SmirContext` contains:

```text
source_arch
vregs
arch_regs
flags
pc
insn_count
cycle_count
exit_reason
debug
exclusive_monitor
```

`ArchRegState` contains architecture-specific register banks. Scalar and vector virtual values live in `VRegFile`; architectural values live in `ArchRegState` or in runtime structs during native execution.

## 10. Runtime structs

Native execution uses fixed `repr(C)` structs:

- `GuestRegs` for x86-family identity-mapped native execution: `gpr[32]`, `rflags`, `exit_pc`, context pointer, load/store/call helpers, and FS/GS bases.
- `Aarch64GuestRegs` for AArch64 native and state-backed execution: `x[31]`, `sp`, `pc`, `nzcv`, `fpcr`, `fpsr`, vector slots, context pointer, scalar/vector memory helpers, and exclusive-monitor state.
