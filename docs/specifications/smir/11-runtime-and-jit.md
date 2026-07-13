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

The architecture-specific scalar whitelist admits exact register-only BMI1 BLS, ADX, BT/BTS/BTR/BTC, CLC/STC/CMC, MOV, NOT, low-byte SETcc, 16-bit register CMOVcc, and 16-bit XCHG lowering in addition to the shared scalar set. ADD, SUB, ADC, SBB, NEG, INC, DEC, AND, OR, XOR, SHL, SHR, SAR, ROL, and ROR admit legacy x86 GPR destinations at 8/16/32/64 bits; BT and its update forms admit 16/32/64 bits. Every admitted 8/16-bit destination path explicitly merges its result into the original x86 register after computing the narrow result and flags. Other destination-producing operations remain limited to 32/64-bit forms unless their lowerer has a separately validated x86 partial-register merge: AArch64 W-register writes zero-extend, while x86 8/16-bit writes preserve the destination's upper bits. Unsupported subregister shapes fail before native execution.

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
```

## 9. Hot-block policy

The x86 VCPU hot-block JIT is on by default on supported x86-64 and AArch64 hosts: hot loops are promoted, lifted to SMIR, optimized at O2, lowered through the selected host backend, cached, and run through W^X executable memory. The x86-64 backend's verification mode re-runs native regions in the interpreter and diffs state. AArch64 production regressions directly compare the bridged native result with the interpreter.

## 10. Native exits

A native exit records a resume PC in runtime state and returns to the host dispatcher. Exits can replace a complete frontier block or a specific `(source block, target block)` edge. Edge exits let auto-promoted regions yield on backward edges without globally replacing the target block. Native exits are also used for helper faults, call-helper bails, and unsupported frontiers.

## 11. Cache invalidation

Self-modifying code must invalidate decode/lift/JIT caches. Store helpers must detect writes to code pages and bail if needed.
