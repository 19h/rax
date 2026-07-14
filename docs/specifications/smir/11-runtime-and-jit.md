# 11. Runtime and JIT

## 1. Cargo feature

`smir-jit` is enabled by default in `Cargo.toml`. Runtime dispatch may still disable promotion by environment or safety gate.

## 2. Runtime module

`lower/runtime.rs` provides the executable-code bridge. It is gated by `smir-jit` and includes host-specific support for x86-64 and AArch64.

## 3. x86 GuestRegs

`GuestRegs` layout:

```text
gpr[32]
rflags
exit_pc
ctx
load_fn
store_fn
fs_base
gs_base
call_fn
```

GPR indices are x86 register encodings. APX R16-R31 are present in the state even when the host has no corresponding physical GPR path.

## 4. AArch64GuestRegs

`Aarch64GuestRegs` layout:

```text
x[31]
sp
pc
nzcv
fpcr
fpsr
v[64]
ctx
load_fn
store_fn
exclusive_addr
exclusive_size
exclusive_valid
vec_load_fn
vec_store_fn
```

The vector array stores V0-V31 as two `u64` words each.

### 4.1. AArch32GuestRegs

`Aarch32GuestRegs` is the AArch32 scalar identity-bridge state:

```text
r[16]
cpsr
```

On an AArch64 host, the bridge zero-extends AArch32 R0-R15 into W0-W15 of a temporary `Aarch64GuestRegs` state, imports CPSR.NZCV into PSTATE.NZCV, executes the lowered block through the existing AArch64 identity trampoline, and narrows W0-W15 back to 32 bits. Only CPSR bits N, Z, C, and V are replaced on return; every other CPSR field is preserved from the entry snapshot.

Native admission is fail-closed. The current AArch32-to-AArch64 scalar gate accepts terminal, register-only W32 regions over R0-R14 from either A32 or unpredicated T16/T32 with exact integer operations covered by the lowerer: moves, add/subtract with carry or borrow, integer comparisons and flag-setting arithmetic, logical operations without flag updates, immediate shifts and rotates, multiply and multiply-add/subtract, signed/unsigned division, count-leading-zero, bit reverse, byte reverse, signed/unsigned byte and halfword extension, and bitfield operations. R15 is excluded because AArch32 reads of the program counter have pipeline and alignment semantics rather than ordinary W15 identity semantics.

The Thumb lifter retains the decoded 2-byte or 4-byte width, normalizes architectural R13 to the identity-mapped X13/W13 register rather than AArch64 SP, and uses the Thumb `PC + 4` branch base. Direct BL writes `(PC + 4) | 1` to R14. Its initial native subset admits T16 arithmetic forms whose architectural contract updates all NZCV bits, unflagged high-register moves/adds and REV, plus unflagged T32 arithmetic/logical/shift, multiply/divide, bit-manipulation, MOVW/MOVT, extend, and bitfield forms. T16 MOVS/logical/shift forms are excluded because they preserve a subset of C/V that the current generic flag IR does not express. Explicit conditions, nonzero IT state, register-controlled shifts, rotated extend operands, RRX, encoded LSR/ASR by 32, memory, VFP/NEON, system state, non-terminal native control flow, and materialized virtual-register results retain interpreter fallback until their complete architectural contracts are represented and lowered. The bridge preserves CPSR.T and both split IT fields because it merges only NZCV on return.

## 5. x86-64 trampoline

`rax_smir_enter_native`:

1. preserves host callee-saved registers;
2. stores entry/state pointers on the host stack;
3. loads guest RFLAGS and GPRs into host registers;
4. calls the lowered block;
5. stores host registers and flags back to `GuestRegs`;
6. sanitizes host EFLAGS bits before returning to Rust;
7. restores host state.

Guest RSP is not loaded into host RSP. The block runs on the host stack.

## 6. AArch64 trampolines

`rax_a64_enter_native` marshals GPRs and NZCV for identity-mapped AArch64 native blocks. `rax_a64_enter_native_fp` additionally marshals V0-V31 plus FPCR/FPSR for scalar FP/SIMD regions and restores host FPCR/FPSR afterward.

The x86 VCPU also uses the scalar AArch64 trampoline for eligible x86-lifted SMIR. Legacy x86 GPR encodings RAX-R15 map to X0-X15. The bridge maps x86 CF/ZF/SF/OF to PSTATE.C/Z/N/V and merges only those four flags back into RFLAGS; PF, AF, control flags, and reserved bits remain from the pre-region snapshot. Eligibility rejects live PF/AF definitions or consumers, unsupported registers/virtual temporaries, memory, and flag contracts whose AArch64 carry convention is not normalized. The x86-register SBB lowering normalizes canonical x86 borrow-CF to AArch64 no-borrow-C around SBC; live CF outputs from SUB/CMP/NEG and generic CF-based unsigned conditions remain excluded. Flag-setting SBB remains eligible only when its unavailable PF/AF definitions are dead. The strict x86 lifter continues to reject the architecturally invalid APX NF ADC/SBB encodings; no-flag SBB coverage exercises the SMIR lowering contract directly.

The architecture-specific scalar whitelist admits exact register-only BMI1 BLS, ADX, BT/BTS/BTR/BTC, CLC/STC/CMC, MOV, NOT, low-byte SETcc, 16-bit register CMOVcc, 16-bit XCHG, 16-bit MOVSX/MOVZX and CBW, and APX NDD SHLD/SHRD lowering in addition to the shared scalar set. ADD, SUB, ADC, SBB, NEG, INC, DEC, AND, OR, XOR, SHL, SHR, SAR, ROL, ROR, RCL, and RCR admit legacy x86 GPR destinations at 8/16/32/64 bits; BT, its update forms, destructive SHLD/SHRD, APX NDD SHLD/SHRD, single-result signed multiply, BSF/BSR, primitive CLZ/CTZ/POPCNT, and architectural x86 POPCNT/TZCNT/LZCNT operations admit 16/32/64 bits. Exact no-flag implicit W16 MUL/IMUL pairs are also admitted, and W32 MULX/full-product forms use widening multiply rather than an unavailable W32 high-half instruction. Every admitted 8/16-bit destination path explicitly merges its result into the original x86 register after computing the narrow result and flags. W16 extension results are formed in a saved scratch register before the low word is merged, preserving the original source in destructive CBW and source/destination aliases; optimizer-folded immediate sources use the same merge path. BSF/BSR merge only their `Specific(ZF)` output into NZCV, preserving live CF/SF/OF across both full-width and W16 paths. `X86Count` preserves all NZCV bits under APX NF, implements POPCNT's represented all-zero/ZF contract, and merges only requested CF/ZF for TZCNT/LZCNT; live POPCNT PF/AF outputs still force interpreter fallback because NZCV cannot represent them. W16 multiply computes the complete product before merging either architectural half, preserving both destination upper parts and all source aliases; single-result signed multiply likewise computes into a saved scratch destination, including destructive and APX NDD source aliases. MULX with identical output registers retains the architecturally final high half at both W32 and W64. The strict APX lifter consumes opcode-69's width-dependent imm16 payload without absorbing the following instruction. Destructive W16 double shifts snapshot the original destination before computing and merging the low word. APX NDD double shifts compute through a saved scratch destination so independent-destination aliases with the base, fill, or CL count retain the original inputs; flag-setting W16 register-count forms and immediate effective counts above 16 remain excluded. Other destination-producing operations remain limited to 32/64-bit forms unless their lowerer has a separately validated x86 partial-register merge: AArch64 W-register writes zero-extend, while x86 8/16-bit writes preserve the destination's upper bits. Unsupported subregister shapes fail before native execution.

Register-only x86 CRC32C is admitted for byte, halfword, word, and doubleword data when the AArch64 host exposes FEAT_CRC32. Native CRC32C{B,H,W,X} writes Wd, exactly zero-extending the architectural x86 result while preserving NZCV; unsupported hosts retain interpreter fallback.

Guest SP/X18/X28/X30 restrictions are part of the identity-map contract and must be enforced by clobber gates or lowerer rejection.

## 7. Executable memory

`ExecMem` creates a page-aligned executable mapping. Host-specific behavior:

- non-AArch64 Unix: RW `mmap`, copy, `mprotect` RX;
- Linux/AArch64: RW `mmap`, copy, `mprotect` RX, `__clear_cache`;
- macOS/AArch64: `MAP_JIT`, `pthread_jit_write_protect_np`, copy, write-protect, `sys_icache_invalidate`.

This is the W^X / MAP_JIT boundary.

## 8. Run methods

`ExecMem` supports:

```text
run                  x86 identity mapped block through GuestRegs
run_aarch64          state-backed AArch64-on-x86 ABI fn(*mut Aarch64GuestRegs)
run_aarch64_identity AArch64 identity mapped block through AArch64 trampoline
run_aarch64_identity_fp AArch64 identity mapped FP/SIMD block through FP trampoline
run_aarch32_identity AArch32 scalar state through the AArch64 identity trampoline
```

## 9. Hot-block policy

The x86 VCPU hot-block JIT is on by default on supported x86-64 and AArch64 hosts: hot loops are promoted, lifted to SMIR, optimized at O2, lowered through the selected host backend, cached, and run through W^X executable memory. The x86-64 backend's verification mode re-runs native regions in the interpreter and diffs state. AArch64 production regressions directly compare the bridged native result with the interpreter.

## 10. Native exits

A native exit records a resume PC in runtime state and returns to the host dispatcher. Exits can replace a complete frontier block or a specific `(source block, target block)` edge. Edge exits let auto-promoted regions yield on backward edges without globally replacing the target block. Native exits are also used for helper faults, call-helper bails, and unsupported frontiers.

## 11. Cache invalidation

Self-modifying code must invalidate decode/lift/JIT caches. Store helpers must detect writes to code pages and bail if needed.
