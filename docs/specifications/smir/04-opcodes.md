# 04. Opcode Catalog

## 1. Scope

`OpKind` is the semantic opcode catalog. It is broader than the original v0.1 spec. The current source includes generic scalar operations, memory operations, atomics, scalar FP, generic vector forms, AVX10 forms, Hexagon/HVX forms, RISC-V exact helper forms, flag operations, system operations, and debug/meta operations.

The exact Rust field list for each variant is source-defined. This document specifies the semantic groups and contracts that lifters, interpreters, optimizers, and lowerers must preserve.

## 2. Implemented operation index

Integer arithmetic: Add, Sub, Adc, Sbb, Neg, Inc, Dec, Cmp, MulU, MulS, MulAdd, MulSub, DivU, DivS.
Bitwise logical: And, Or, Xor, Not, Test, AndNot.
Shifts/rotates: Shl, Shr, Sar, Shld, Shrd, Rol, Ror, Rcl, Rcr, BidirShift, SatOrigShl.
Hexagon scalar/DSP arithmetic: SatN, ClMul, CmpyW128Sat, HexCabacDecBin, HexTlbMatch.
Bit manipulation: Bt, Bts, Btr, Btc, Bsf, Bsr, Bextr, Bzhi, Pdep, Pext, Clz, Ctz, Popcnt, Bswap, Rbit, Bfx, Bfi.
Movement/conversion: Mov, CMove, Select, ZeroExtend, SignExtend, Cwd, Truncate, Lea, Xchg, Leave.
Memory: Load, Store, PredLoad, PredStore, RepStos, RepMovs, LoadPair, StorePair, VLoad, VStore.
Atomics/exclusive: AtomicLoad, AtomicStore, AtomicRmw, AtomicCmpXadd, Cas, LoadExclusive, StoreExclusive, ClearExclusive.
I/O/system/misc memory: Prefetch, Fence, IoIn, IoOut.
Scalar FP: FAdd, FSub, FMul, FDiv, FFma, FAbs, FNeg, FSqrt, FMin, FMax, FCmp, FConvert, IntToFp, FpToInt, FRound.
Generic vector: VAdd, VSub, VMax, VMul, VAnd, VOr, VXor, VShift, VCmp, VMov, VInsertLane, VExtractLane, VShuffle, VBroadcast, VMin, VFma.
Hexagon/HVX vector: VLane, VWidenMul, VWidenExt, VWidenAddSub, VLaneUnary, VNavg, VShiftAcc, VPack, VPackSat, VLut16, VLut, VDelta, VShuffVdd, VDealB4W, VAlign, VShuffle2, VShuffleEO, VCmpToQ, VQFromVAndR, VMaskZero, VBlend, VLaneCond, VCarry, VSwap, VShuffleEOPair, VShuffleDeal, VDealVdd, VHist, VCondMove, VPrefixSumQ, VShiftV, VMulShiftSat, VNarrowShiftSat, VSatDW, VNarrowShiftV, VMulSubLane, VMulSubLaneFrac, VMulSubLaneSh, VMulWord64Pair, VSlideReduceMul, VRotReduceMulPair, VReduceMul.
AVX10: VDotProduct, VMultiplyAdd52, VPopcnt, VPermute, VShuffleBitQM, VDotProductBF16, VCvtFP32ToBF16, VCvtBF16ToFP32, VFP16Arith, VCvtFpToIntSat, VMinMax, VMpsadbw, VDotProductExt.
Flag operations: ReadFlags, WriteFlags, SetCF, SetDF, CmcCF, MaterializeFlags, TestCondition, SetCC.
Privileged/system: Syscall, Swi, ReadSysReg, WriteSysReg.
Architecture-exact semantic escapes: HexFp, HexFp3, HexFpRecip, HexFpDf, HexFpScFma, RvFp, RvIntCrypto, RvVector.
Meta/debug: Nop, Undefined, Breakpoint.

## 3. Integer arithmetic

Integer arithmetic uses explicit `OpWidth`. Results are masked to the operation width unless the variant explicitly writes high/low components.

- `Add/Sub/Adc/Sbb/Neg/Inc/Dec` MAY update flags according to `FlagUpdate`.
- `Cmp` computes flags without storing a result.
- `MulU/MulS` can write low and optional high results.
- `DivU/DivS` represent single-width division in generic SMIR. x86 double-width `RDX:RAX` division is not safely modeled by the current generic variant and is excluded from the native JIT whitelist.
- `MulAdd/MulSub` represent fused integer multiply-accumulate/subtract patterns for RISC/DSP lifting.

## 4. Bitwise and bit manipulation

Logical operations are width-masked. `Test` is an AND-for-flags operation. Bit-manipulation variants model x86 BMI, ARM bitfield, RISC-V bitmanip, and cross-architecture equivalents. Flag side effects remain variant-specific.

## 5. Shifts and rotates

Shift counts use source-architecture semantics. Lowerers must preserve masking and undefined/unchanged flag behavior where specified by the source architecture. Hexagon-specific bidirectional and saturating shifts encode signed variable-count behavior that cannot be reduced to a single conventional left/right shift.

## 6. Data movement

`Mov`, extension, truncation, `Lea`, and exchange variants are semantic data movement. x86 partial-register writes are handled in interpreter/lowerer semantics: 8/16-bit GPR writes merge, 32-bit writes zero-extend, and 64-bit writes replace.

## 7. Memory

Memory operations carry `Address`, `MemWidth`, and sign-extension information. Predicated Hexagon load/store variants MUST suppress the memory access entirely when the predicate is false, including suppressing faults.

REP/string, pair load/store, vector load/store, and helper-backed memory all remain memory effects and must participate in alias/fault/SMC handling.

## 8. Atomics and exclusive monitor

Atomic operations carry `MemoryOrder`. `LoadExclusive`/`StoreExclusive` use the `ExclusiveMonitor` state or runtime-state equivalent. `Cas` returns both old value and success. `AtomicCmpXadd` models x86 CMPccXADD-style compare/update/flag behavior.

## 9. Floating point

Generic scalar FP variants exist for common arithmetic and conversions. For source ISAs whose behavior depends on exact flags, dynamic rounding, NaN-boxing, or non-native rounding, lifters may emit architecture-exact variants instead of generic `FAdd`/`FMul` forms.

## 10. Vector and SIMD

Generic vector forms cover common packed arithmetic/logical/compare/move/shuffle semantics. Architecture-specific vector forms are used for HVX and AVX10 where exact semantics need additional structure.

## 11. Architecture-exact semantic escapes

The following variants are deliberately not decomposed into primitive SMIR in the current implementation:

- `HexFp`, `HexFp3`, `HexFpRecip`, `HexFpDf`, `HexFpScFma` for exact Hexagon floating-point and fixup behavior.
- `HexCabacDecBin` and `HexTlbMatch` for exact scalar Hexagon helpers.
- `RvFp` for RISC-V scalar FP/FMA with fflags, NaN canonicalization/boxing, and dynamic rounding.
- `RvIntCrypto` for RISC-V crypto and bit-manip helpers that would otherwise require table-heavy expansions.
- `RvVector` for RVV 1.0, because element width/count are dynamic state (`vtype`, `vl`, `vstart`, `vcsr`).

These variants are self-contained interpreter operations and are not native-JIT-whitelisted unless a backend explicitly implements them.

## 12. Flag operations

Flag operations explicitly read, write, set, complement, or materialize flags. `TestCondition` and `SetCC` materialize boolean condition results from the current flag state.

## 13. System operations

System operations represent syscall/software interrupt and system-register access. Native lowerers must either implement their runtime ABI precisely or reject them.

## 14. JIT whitelist

`OpKind::is_jit_safe` is not a generic lowerability predicate. It is a fail-safe native hot-block JIT whitelist for the x86 identity-mapped path. Operations outside the whitelist may still be interpretable and may still be lowerable by other backends.

## 15. Adding an opcode

A new opcode MUST define:

1. field-level semantics and widths;
2. source lifter ownership;
3. interpreter behavior;
4. flag reads/writes;
5. memory/atomic/fault behavior;
6. optimizer source/destination/register/flag use metadata;
7. native lowerer behavior or explicit rejection;
8. differential tests where an oracle exists.
