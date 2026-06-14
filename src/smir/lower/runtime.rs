//! Native execution runtime for SMIR-lowered blocks (the JIT back end's
//! "executor"). This is the bridge that takes the x86-64 machine code produced
//! by [`crate::smir::lower::x86_64::X86_64Lowerer`] and actually runs it on the
//! host CPU, marshalling guest register state in and out.
//!
//! Gated behind the `smir-jit` feature. Two host backends are provided:
//!  * x86-64: entry trampoline `rax_smir_enter_native` (hand-written x86-64
//!    assembly) marshalling the x86 [`GuestRegs`] file in/out.
//!  * aarch64: entry trampoline `rax_a64_enter_native` (AArch64 assembly)
//!    marshalling the [`Aarch64GuestRegs`] file in/out.
//! Both rely on the lowerer's 1:1 identity register map (guest GPR `N` ⇒ the
//! same-named host GPR), so a lowered block reads and writes guest state
//! directly; the only marshalling is once on entry and once on exit.
//!
//! The x86-64 path is validated bit-exact against KVM by the differential
//! harness in `tests/diff_fuzz.rs` (`smir_native_*` tests). The AArch64 path is
//! validated against the AArch64 interpreter.

#![cfg(feature = "smir-jit")]

use super::{
    X86_GUEST_CALL_FN_OFFSET, X86_GUEST_CTX_OFFSET, X86_GUEST_EXIT_PC_OFFSET,
    X86_GUEST_FS_BASE_OFFSET, X86_GUEST_GPR_COUNT, X86_GUEST_GS_BASE_OFFSET,
    X86_GUEST_LOAD_FN_OFFSET, X86_GUEST_RFLAGS_OFFSET, X86_GUEST_STORE_FN_OFFSET,
    X86_STATE_PTR_AT_RBP,
};

/// Apple I-cache invalidation (libSystem). Required after writing a `MAP_JIT`
/// region and before executing it: on AArch64 the instruction cache is not
/// coherent with the data cache, so freshly written code may otherwise execute
/// stale bytes (intermittently — only on region reuse / SMC — which x86-hosted
/// test harnesses can never catch).
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
unsafe extern "C" {
    fn sys_icache_invalidate(start: *mut core::ffi::c_void, len: usize);
}

/// compiler-rt instruction-cache flush (Linux/aarch64), same purpose as the
/// Apple `sys_icache_invalidate` above.
#[cfg(all(target_arch = "aarch64", target_os = "linux"))]
unsafe extern "C" {
    fn __clear_cache(start: *mut core::ffi::c_char, end: *mut core::ffi::c_char);
}

/// Guest register file marshalled in/out of a lowered native block.
///
/// `gpr[i]` is indexed by x86 register *encoding*
/// (0=RAX, 1=RCX, 2=RDX, 3=RBX, 4=RSP, 5=RBP, 6=RSI, 7=RDI, 8..=15=R8..=R15,
/// 16..=31=R16..=R31). `rflags` holds the materialized flags. `repr(C)` with a
/// fixed layout — the trampoline reads/writes by byte offset (`gpr[i]` at
/// `i*8`, `rflags` at [`X86_GUEST_RFLAGS_OFFSET`]).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GuestRegs {
    /// General-purpose registers, indexed by x86 encoding.
    pub gpr: [u64; X86_GUEST_GPR_COUNT],
    /// Materialized RFLAGS.
    pub rflags: u64,
    /// Resume guest PC, written by an exit stub when a block lowered with the
    /// general-exit ABI hands control back to the interpreter. Only meaningful
    /// for blocks run via [`ExecMem::run_with_exit`]. See
    /// [`enter_native`] (the R15-reserved trampoline) and the lowerer's
    /// `native_exit` mode.
    pub exit_pc: u64,
    /// Opaque context pointer passed as arg0 to the memory helpers (the
    /// `*mut X86_64Vcpu`). Set by the JIT before each run.
    pub ctx: u64,
    /// Address of the load helper `fn(ctx, addr, size, signed) -> (value, ok)`
    /// (SysV: value in RAX, ok in RDX).
    pub load_fn: u64,
    /// Address of the store helper `fn(ctx, addr, value, size) -> ok`.
    pub store_fn: u64,
    /// IA32_FS_BASE. The lowered code adds
    /// this to the effective address of an `fs:`-overridden memory operand
    /// ([`Address::SegmentRel`]). Set from `sregs.fs.base` before each run.
    pub fs_base: u64,
    /// IA32_GS_BASE. As `fs_base` but for
    /// `gs:`-overridden operands (per-CPU data in the Linux kernel).
    pub gs_base: u64,
    /// Address of the call helper `fn(gr, target_pc, return_pc) -> ok`. Used by
    /// the lift-through-calls path (RAX_JIT_CALL): a guest CALL in a JIT region
    /// lowers to a call-out into this helper, which runs the interpreter for the
    /// callee until it returns to `return_pc`, then resumes native execution.
    /// `ok == 0` means the callee bailed to the interpreter (an exit/exception)
    /// and the region must return; the helper has set `exit_pc`. NOTE: arg0 is
    /// the `*mut GuestRegs` itself (not `ctx`), because the helper needs the
    /// full marshalled guest state, and `gr.ctx` carries the vcpu pointer.
    pub call_fn: u64,
}

/// AArch64 guest register file for state-backed x86-64 lowering.
///
/// Unlike [`GuestRegs`], this ABI does not rely on identity-mapping guest
/// registers into host registers. Lowered code is entered as a normal SysV
/// function with `RDI = *mut Aarch64GuestRegs` and reads/writes architectural
/// state through this struct. NZCV is stored in architectural PSTATE position
/// (bits 31:28); the remaining bits are preserved as zero for now.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Aarch64GuestRegs {
    /// X0-X30.
    pub x: [u64; 31],
    /// Stack pointer.
    pub sp: u64,
    /// Program counter.
    pub pc: u64,
    /// PSTATE.NZCV in bits 31:28.
    pub nzcv: u64,
    /// Floating-point control register.
    pub fpcr: u64,
    /// Floating-point status register.
    pub fpsr: u64,
    /// V0-V31 as low/high u64 pairs.
    pub v: [u64; 64],
    /// Opaque context pointer passed as arg0 to AArch64 memory helpers.
    pub ctx: u64,
    /// Address of the load helper
    /// `extern "C" fn(ctx, addr, size, signed) -> (value, ok)`. The 16-byte
    /// return is an AAPCS64 two-eightbyte value: `value` in x0, `ok` (non-zero
    /// on success) in x1. The identity-map AArch64 lowerer's `Load` call-out
    /// fault-bails (records the faulting PC and exits to the interpreter) when
    /// `ok == 0` — so a `#[repr(C)] { value: u64, ok: u64 }` return is required
    /// for precise fault restart (analogous to the x86 helper's RAX:RDX).
    pub load_fn: u64,
    /// Address of `extern "C" fn(ctx, addr, value, size) -> ok` (non-zero on
    /// success; `ok == 0` fault-bails like the load helper).
    pub store_fn: u64,
    /// Address armed by the last load-exclusive.
    pub exclusive_addr: u64,
    /// Byte size armed by the last load-exclusive.
    pub exclusive_size: u64,
    /// Non-zero when the exclusive monitor is armed.
    pub exclusive_valid: u64,
}

impl Default for Aarch64GuestRegs {
    fn default() -> Self {
        Self {
            x: [0; 31],
            sp: 0,
            pc: 0,
            nzcv: 0,
            fpcr: 0,
            fpsr: 0,
            v: [0; 64],
            ctx: 0,
            load_fn: 0,
            store_fn: 0,
            exclusive_addr: 0,
            exclusive_size: 0,
            exclusive_valid: 0,
        }
    }
}

impl Aarch64GuestRegs {
    pub const X0_OFFSET: i32 = 0;
    pub const SP_OFFSET: i32 = 31 * 8;
    pub const PC_OFFSET: i32 = 32 * 8;
    pub const NZCV_OFFSET: i32 = 33 * 8;
    pub const FPCR_OFFSET: i32 = 34 * 8;
    pub const FPSR_OFFSET: i32 = 35 * 8;
    pub const V_OFFSET: i32 = 36 * 8;
    pub const CTX_OFFSET: i32 = Self::V_OFFSET + 64 * 8;
    pub const LOAD_FN_OFFSET: i32 = Self::CTX_OFFSET + 8;
    pub const STORE_FN_OFFSET: i32 = Self::LOAD_FN_OFFSET + 8;
    pub const EXCLUSIVE_ADDR_OFFSET: i32 = Self::STORE_FN_OFFSET + 8;
    pub const EXCLUSIVE_SIZE_OFFSET: i32 = Self::EXCLUSIVE_ADDR_OFFSET + 8;
    pub const EXCLUSIVE_VALID_OFFSET: i32 = Self::EXCLUSIVE_SIZE_OFFSET + 8;
}

// enter_native(rdi = entry ptr, rsi = *mut GuestRegs):
//   preserve host callee-saved -> load guest GPRs+RFLAGS into the identical host
//   regs -> `call` the block -> store the host regs back into GuestRegs.
// RSP (gpr[4]) is NOT loaded — the block runs on the host stack (it owns no
// guest stack). Alignment: 6 callee pushes (48) + `sub rsp,24` (72 total) leaves
// rsp 16-aligned at the `call`.
#[cfg(target_arch = "x86_64")]
core::arch::global_asm!(
    ".text",
    ".p2align 4",
    ".globl rax_smir_enter_native",
    ".type rax_smir_enter_native,@function",
    "rax_smir_enter_native:",
    "push rbp",
    "push rbx",
    "push r12",
    "push r13",
    "push r14",
    "push r15",
    "sub rsp, 24", // [rsp]=entry [rsp+8]=state [rsp+16]=pad ; rsp 16-aligned
    "mov [rsp], rdi",
    "mov [rsp+8], rsi",
    "mov rax, [rsi+256]", // RFLAGS
    "push rax",
    "popfq",
    "mov rax, [rsi+0]",
    "mov rcx, [rsi+8]",
    "mov rdx, [rsi+16]",
    "mov rbx, [rsi+24]",
    "mov rbp, [rsi+40]",
    "mov rdi, [rsi+56]",
    "mov r8,  [rsi+64]",
    "mov r9,  [rsi+72]",
    "mov r10, [rsi+80]",
    "mov r11, [rsi+88]",
    "mov r12, [rsi+96]",
    "mov r13, [rsi+104]",
    "mov r14, [rsi+112]",
    "mov r15, [rsi+120]",
    "mov rsi, [rsi+48]", // rsi last (was the base pointer)
    "call [rsp]",
    "push rax",          // save guest RAX ; state now at [rsp+16]
    "mov rax, [rsp+16]", // rax = *mut GuestRegs
    "mov [rax+8],   rcx",
    "mov [rax+16],  rdx",
    "mov [rax+24],  rbx",
    "mov [rax+40],  rbp",
    "mov [rax+48],  rsi",
    "mov [rax+56],  rdi",
    "mov [rax+64],  r8",
    "mov [rax+72],  r9",
    "mov [rax+80],  r10",
    "mov [rax+88],  r11",
    "mov [rax+96],  r12",
    "mov [rax+104], r13",
    "mov [rax+112], r14",
    "mov [rax+120], r15",
    "pushfq",
    "pop rcx",
    "mov [rax+256], rcx",
    // Sanitize the HOST EFLAGS before returning to Rust. The `popfq` above loaded
    // the GUEST RFLAGS into the host, and the region runs with them — but the
    // sticky control flags then LEAK into the host: AC (alignment check, set by
    // the kernel's SMAP `stac` for user copies) faults the next unaligned host
    // access with #AC/SIGBUS; DF (direction) reverses host `rep` string ops
    // (memcpy/memset) → corruption; TF would single-step → SIGTRAP; NT corrupts
    // a host `iret`. Clear bits 8(TF)/10(DF)/14(NT)/18(AC); the arithmetic flags
    // are caller-saved scratch the host re-derives, so they need no restore.
    "pushfq",
    "and qword ptr [rsp], -0x44501", // ~0x44500: clear TF(0x100)+DF(0x400)+NT(0x4000)+AC(0x40000)
    "popfq",
    "mov rcx, [rsp]", // saved guest RAX
    "mov [rax+0], rcx",
    "add rsp, 8",  // pop saved RAX
    "add rsp, 24", // pop locals
    "pop r15",
    "pop r14",
    "pop r13",
    "pop r12",
    "pop rbx",
    "pop rbp",
    "ret",
);

#[cfg(target_arch = "x86_64")]
unsafe extern "C" {
    fn rax_smir_enter_native(entry: *const u8, state: *mut GuestRegs);
}

// rax_a64_enter_native(x0 = entry ptr, x1 = *mut Aarch64GuestRegs):
//   Identity-mapped AArch64-on-AArch64 entry trampoline. Saves the host
//   callee-saved GPRs (x19-x30), loads guest X0-X17/X19-X27/X29 + NZCV from the
//   struct into the identical host registers, runs the block on the HOST stack,
//   then stores the live host registers back into the struct.
//
//   Reserved host registers (NOT mapped to guest, must be left untouched by the
//   block — the clobber gate enforces this):
//     x28 = persistent *mut Aarch64GuestRegs (so exit stubs can `str <pc>,[x28,#PC]`)
//     x30 = link register / return into this trampoline (block ends with `ret`)
//     x18 = platform register (reserved by the macOS ABI; never clobber)
//     sp  = host stack (guest SP is not loaded; SP-relative guest code deopts)
//   Guest x18/x28/x30 round-trip untouched (their struct slots are preserved).
//
//   Frame (112 bytes, 16-aligned): [sp+0..88] host x19..x30, [sp+96] entry.
//   Both `_rax_a64_enter_native` (Mach-O) and `rax_a64_enter_native` (ELF) are
//   defined so the C symbol resolves on macOS and Linux alike.
#[cfg(target_arch = "aarch64")]
core::arch::global_asm!(
    ".text",
    ".p2align 2",
    ".globl _rax_a64_enter_native",
    ".globl rax_a64_enter_native",
    "_rax_a64_enter_native:",
    "rax_a64_enter_native:",
    "sub sp, sp, #112",
    "stp x19, x20, [sp, #0]",
    "stp x21, x22, [sp, #16]",
    "stp x23, x24, [sp, #32]",
    "stp x25, x26, [sp, #48]",
    "stp x27, x28, [sp, #64]",
    "stp x29, x30, [sp, #80]",
    "str x0, [sp, #96]",   // stash entry (guest x0 is about to overwrite host x0)
    "mov x28, x1",         // x28 = regs ptr, reserved for the duration of the block
    "ldr w9, [x28, #264]", // NZCV (offset 33*8); load before guest x9 below
    "msr nzcv, x9",
    "ldp x0, x1, [x28, #0]",
    "ldp x2, x3, [x28, #16]",
    "ldp x4, x5, [x28, #32]",
    "ldp x6, x7, [x28, #48]",
    "ldp x8, x9, [x28, #64]",
    "ldp x10, x11, [x28, #80]",
    "ldp x12, x13, [x28, #96]",
    "ldp x14, x15, [x28, #112]",
    "ldp x16, x17, [x28, #128]",
    // skip x18 (offset 144) — reserved platform register
    "ldr x19, [x28, #152]",
    "ldr x20, [x28, #160]",
    "ldr x21, [x28, #168]",
    "ldr x22, [x28, #176]",
    "ldr x23, [x28, #184]",
    "ldr x24, [x28, #192]",
    "ldr x25, [x28, #200]",
    "ldr x26, [x28, #208]",
    "ldr x27, [x28, #216]",
    // skip x28 (offset 224) — holds the regs ptr
    "ldr x29, [x28, #232]",
    // skip x30 (offset 240) — reserved link register
    "ldr x30, [sp, #96]", // x30 = entry; blr sets x30 = return addr below
    "blr x30",
    "stp x0, x1, [x28, #0]",
    "stp x2, x3, [x28, #16]",
    "stp x4, x5, [x28, #32]",
    "stp x6, x7, [x28, #48]",
    "stp x8, x9, [x28, #64]",
    "stp x10, x11, [x28, #80]",
    "stp x12, x13, [x28, #96]",
    "stp x14, x15, [x28, #112]",
    "stp x16, x17, [x28, #128]",
    "str x19, [x28, #152]",
    "str x20, [x28, #160]",
    "str x21, [x28, #168]",
    "str x22, [x28, #176]",
    "str x23, [x28, #184]",
    "str x24, [x28, #192]",
    "str x25, [x28, #200]",
    "str x26, [x28, #208]",
    "str x27, [x28, #216]",
    "str x29, [x28, #232]",
    "mrs x9, nzcv", // x9 already stored above; reuse as scratch
    "str x9, [x28, #264]",
    "ldp x19, x20, [sp, #0]",
    "ldp x21, x22, [sp, #16]",
    "ldp x23, x24, [sp, #32]",
    "ldp x25, x26, [sp, #48]",
    "ldp x27, x28, [sp, #64]",
    "ldp x29, x30, [sp, #80]",
    "add sp, sp, #112",
    "ret",
);

#[cfg(target_arch = "aarch64")]
unsafe extern "C" {
    fn rax_a64_enter_native(entry: *const u8, regs: *mut Aarch64GuestRegs);
}

// rax_a64_enter_native_fp(x0 = entry, x1 = *mut Aarch64GuestRegs):
//   Like rax_a64_enter_native but ALSO marshals V0-V31 + FPCR/FPSR, for regions
//   that use scalar FP / SIMD. Saves the host AAPCS64 callee-saved low-64 of
//   V8-V15 and the host FPCR/FPSR (restored on exit so guest rounding never
//   leaks into host float code). Same GPR/NZCV/reserved-register contract as the
//   scalar trampoline. Frame: 192 B (x19-x30 @0..88, entry @96, host_fpcr @104,
//   host_fpsr @112, d8-d15 @120..184). Aarch64GuestRegs.v is [u64;64] @288, so
//   Vn occupies bytes 288 + n*16 (q-register pairs, imm7 = byteoffset/16).
#[cfg(target_arch = "aarch64")]
core::arch::global_asm!(
    ".text",
    ".p2align 2",
    ".globl _rax_a64_enter_native_fp",
    ".globl rax_a64_enter_native_fp",
    "_rax_a64_enter_native_fp:",
    "rax_a64_enter_native_fp:",
    "sub sp, sp, #192",
    "stp x19, x20, [sp, #0]",
    "stp x21, x22, [sp, #16]",
    "stp x23, x24, [sp, #32]",
    "stp x25, x26, [sp, #48]",
    "stp x27, x28, [sp, #64]",
    "stp x29, x30, [sp, #80]",
    "str x0, [sp, #96]",      // stash entry
    "stp d8, d9, [sp, #120]", // save host callee-saved V8-V15 (low 64)
    "stp d10, d11, [sp, #136]",
    "stp d12, d13, [sp, #152]",
    "stp d14, d15, [sp, #168]",
    "mrs x9, fpcr", // save host FPCR/FPSR
    "str x9, [sp, #104]",
    "mrs x9, fpsr",
    "str x9, [sp, #112]",
    "mov x28, x1",         // x28 = regs ptr
    "ldr w9, [x28, #272]", // guest FPCR -> host (honor guest rounding)
    "msr fpcr, x9",
    "ldr w9, [x28, #280]", // guest FPSR
    "msr fpsr, x9",
    "ldr w9, [x28, #264]", // NZCV
    "msr nzcv, x9",
    "ldp x0, x1, [x28, #0]",
    "ldp x2, x3, [x28, #16]",
    "ldp x4, x5, [x28, #32]",
    "ldp x6, x7, [x28, #48]",
    "ldp x8, x9, [x28, #64]",
    "ldp x10, x11, [x28, #80]",
    "ldp x12, x13, [x28, #96]",
    "ldp x14, x15, [x28, #112]",
    "ldp x16, x17, [x28, #128]",
    "ldr x19, [x28, #152]",
    "ldr x20, [x28, #160]",
    "ldr x21, [x28, #168]",
    "ldr x22, [x28, #176]",
    "ldr x23, [x28, #184]",
    "ldr x24, [x28, #192]",
    "ldr x25, [x28, #200]",
    "ldr x26, [x28, #208]",
    "ldr x27, [x28, #216]",
    "ldr x29, [x28, #232]",
    "ldp q0, q1, [x28, #288]", // load guest V0-V31
    "ldp q2, q3, [x28, #320]",
    "ldp q4, q5, [x28, #352]",
    "ldp q6, q7, [x28, #384]",
    "ldp q8, q9, [x28, #416]",
    "ldp q10, q11, [x28, #448]",
    "ldp q12, q13, [x28, #480]",
    "ldp q14, q15, [x28, #512]",
    "ldp q16, q17, [x28, #544]",
    "ldp q18, q19, [x28, #576]",
    "ldp q20, q21, [x28, #608]",
    "ldp q22, q23, [x28, #640]",
    "ldp q24, q25, [x28, #672]",
    "ldp q26, q27, [x28, #704]",
    "ldp q28, q29, [x28, #736]",
    "ldp q30, q31, [x28, #768]",
    "ldr x30, [sp, #96]", // entry
    "blr x30",
    "stp q0, q1, [x28, #288]", // store guest V0-V31
    "stp q2, q3, [x28, #320]",
    "stp q4, q5, [x28, #352]",
    "stp q6, q7, [x28, #384]",
    "stp q8, q9, [x28, #416]",
    "stp q10, q11, [x28, #448]",
    "stp q12, q13, [x28, #480]",
    "stp q14, q15, [x28, #512]",
    "stp q16, q17, [x28, #544]",
    "stp q18, q19, [x28, #576]",
    "stp q20, q21, [x28, #608]",
    "stp q22, q23, [x28, #640]",
    "stp q24, q25, [x28, #672]",
    "stp q26, q27, [x28, #704]",
    "stp q28, q29, [x28, #736]",
    "stp q30, q31, [x28, #768]",
    "stp x0, x1, [x28, #0]",
    "stp x2, x3, [x28, #16]",
    "stp x4, x5, [x28, #32]",
    "stp x6, x7, [x28, #48]",
    "stp x8, x9, [x28, #64]",
    "stp x10, x11, [x28, #80]",
    "stp x12, x13, [x28, #96]",
    "stp x14, x15, [x28, #112]",
    "stp x16, x17, [x28, #128]",
    "str x19, [x28, #152]",
    "str x20, [x28, #160]",
    "str x21, [x28, #168]",
    "str x22, [x28, #176]",
    "str x23, [x28, #184]",
    "str x24, [x28, #192]",
    "str x25, [x28, #200]",
    "str x26, [x28, #208]",
    "str x27, [x28, #216]",
    "str x29, [x28, #232]",
    "mrs x9, nzcv",
    "str x9, [x28, #264]",
    "mrs x9, fpsr", // guest FPSR (accumulated exception flags) out
    "str x9, [x28, #280]",
    "ldr x9, [sp, #104]", // restore host FPCR/FPSR
    "msr fpcr, x9",
    "ldr x9, [sp, #112]",
    "msr fpsr, x9",
    "ldp d8, d9, [sp, #120]", // restore host V8-V15
    "ldp d10, d11, [sp, #136]",
    "ldp d12, d13, [sp, #152]",
    "ldp d14, d15, [sp, #168]",
    "ldp x19, x20, [sp, #0]",
    "ldp x21, x22, [sp, #16]",
    "ldp x23, x24, [sp, #32]",
    "ldp x25, x26, [sp, #48]",
    "ldp x27, x28, [sp, #64]",
    "ldp x29, x30, [sp, #80]",
    "add sp, sp, #192",
    "ret",
);

#[cfg(target_arch = "aarch64")]
unsafe extern "C" {
    fn rax_a64_enter_native_fp(entry: *const u8, regs: *mut Aarch64GuestRegs);
}

/// Byte offset of `GuestRegs.exit_pc` (after `gpr[32]` + `rflags`). An exit stub
/// writes the resume guest PC here via the state pointer.
pub const EXIT_PC_OFFSET: i32 = X86_GUEST_EXIT_PC_OFFSET;

/// Offset of the `*mut GuestRegs` state pointer relative to a lowered block's
/// frame pointer (RBP), under the `rax_smir_enter_native` trampoline's stack
/// layout: the trampoline does `sub rsp,24; [rsp+8]=state` before `call`, and
/// the block's prologue `push rbp; mov rbp,rsp` lands RBP 24 bytes below that
/// slot — so `[rbp+24]` holds the state pointer throughout the block. An exit
/// stub loads it from here to record `exit_pc` (no reserved guest register).
pub const STATE_PTR_AT_RBP: i32 = X86_STATE_PTR_AT_RBP;

/// Byte offset of `GuestRegs.ctx` (the memory-helper context pointer).
pub const CTX_OFFSET: i32 = X86_GUEST_CTX_OFFSET;
/// Byte offset of `GuestRegs.load_fn` (the memory-load helper address).
pub const LOAD_FN_OFFSET: i32 = X86_GUEST_LOAD_FN_OFFSET;
/// Byte offset of `GuestRegs.store_fn` (the memory-store helper address).
pub const STORE_FN_OFFSET: i32 = X86_GUEST_STORE_FN_OFFSET;
/// Byte offset of `GuestRegs.fs_base` (the FS segment base for `fs:` operands).
pub const FS_BASE_OFFSET: i32 = X86_GUEST_FS_BASE_OFFSET;
/// Byte offset of `GuestRegs.gs_base` (the GS segment base for `gs:` operands).
pub const GS_BASE_OFFSET: i32 = X86_GUEST_GS_BASE_OFFSET;
/// Byte offset of `GuestRegs.call_fn` (the lift-through-calls helper address).
pub const CALL_FN_OFFSET: i32 = X86_GUEST_CALL_FN_OFFSET;

/// W^X executable memory holding a finalized lowered block. Maps RW, copies the
/// code in, then flips to RX; unmaps on drop.
pub struct ExecMem {
    ptr: *mut u8,
    len: usize,
}

impl ExecMem {
    /// Map `code` into a fresh W^X region and make it executable.
    ///
    /// The mechanism is host-specific: x86-64 (and any non-aarch64 unix) map RW,
    /// copy, then `mprotect` to RX. Apple-Silicon macOS uses a `MAP_JIT` region
    /// with `pthread_jit_write_protect_np` toggling plus an explicit I-cache
    /// invalidate. Linux/aarch64 maps RW→RX and flushes via `__clear_cache`.
    pub fn new(code: &[u8]) -> Result<Self, ExecMemError> {
        if code.is_empty() {
            return Err(ExecMemError::Empty);
        }
        let len = (code.len() + 0xFFF) & !0xFFF;
        let ptr = Self::map_code(code, len)?;
        Ok(ExecMem { ptr, len })
    }

    /// RW map → copy → `mprotect` RX. Used on x86-64 and any non-aarch64 unix.
    #[cfg(not(target_arch = "aarch64"))]
    fn map_code(code: &[u8], len: usize) -> Result<*mut u8, ExecMemError> {
        let ptr = unsafe {
            libc::mmap(
                core::ptr::null_mut(),
                len,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
                -1,
                0,
            )
        };
        if ptr == libc::MAP_FAILED {
            return Err(ExecMemError::Mmap);
        }
        let ptr = ptr as *mut u8;
        unsafe { core::ptr::copy_nonoverlapping(code.as_ptr(), ptr, code.len()) };
        if unsafe {
            libc::mprotect(
                ptr as *mut libc::c_void,
                len,
                libc::PROT_READ | libc::PROT_EXEC,
            )
        } != 0
        {
            unsafe { libc::munmap(ptr as *mut libc::c_void, len) };
            return Err(ExecMemError::Mprotect);
        }
        Ok(ptr)
    }

    /// Apple-Silicon macOS: a `MAP_JIT` RWX region. Writing it requires the
    /// calling thread to be in *write* mode (`pthread_jit_write_protect_np(0)`);
    /// after the copy we flip back to *execute* mode and invalidate the I-cache
    /// for the written range. The thread is left in execute mode, so even if a
    /// different thread later runs the block (`ExecMem` is `Send`/`Sync`) it sees
    /// executable pages. The toggle is thread-local; a thread that never wrote
    /// JIT memory is already in execute mode by default.
    #[cfg(all(target_arch = "aarch64", target_os = "macos"))]
    fn map_code(code: &[u8], len: usize) -> Result<*mut u8, ExecMemError> {
        let ptr = unsafe {
            libc::mmap(
                core::ptr::null_mut(),
                len,
                libc::PROT_READ | libc::PROT_WRITE | libc::PROT_EXEC,
                libc::MAP_PRIVATE | libc::MAP_ANON | libc::MAP_JIT,
                -1,
                0,
            )
        };
        if ptr == libc::MAP_FAILED {
            return Err(ExecMemError::Mmap);
        }
        let ptr = ptr as *mut u8;
        unsafe {
            libc::pthread_jit_write_protect_np(0);
            core::ptr::copy_nonoverlapping(code.as_ptr(), ptr, code.len());
            libc::pthread_jit_write_protect_np(1);
            sys_icache_invalidate(ptr as *mut core::ffi::c_void, len);
        }
        Ok(ptr)
    }

    /// Linux/aarch64: RW map → copy → `mprotect` RX → `__clear_cache`.
    #[cfg(all(target_arch = "aarch64", target_os = "linux"))]
    fn map_code(code: &[u8], len: usize) -> Result<*mut u8, ExecMemError> {
        let ptr = unsafe {
            libc::mmap(
                core::ptr::null_mut(),
                len,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
                -1,
                0,
            )
        };
        if ptr == libc::MAP_FAILED {
            return Err(ExecMemError::Mmap);
        }
        let ptr = ptr as *mut u8;
        unsafe { core::ptr::copy_nonoverlapping(code.as_ptr(), ptr, code.len()) };
        if unsafe {
            libc::mprotect(
                ptr as *mut libc::c_void,
                len,
                libc::PROT_READ | libc::PROT_EXEC,
            )
        } != 0
        {
            unsafe { libc::munmap(ptr as *mut libc::c_void, len) };
            return Err(ExecMemError::Mprotect);
        }
        unsafe {
            __clear_cache(
                ptr as *mut core::ffi::c_char,
                ptr.add(len) as *mut core::ffi::c_char,
            )
        };
        Ok(ptr)
    }

    /// Execute the block at `entry_offset` (the lowerer's `LowerResult.entry_offset`),
    /// marshalling `regs` in and reading the result back out.
    ///
    /// # Safety
    /// The caller must guarantee that the code was produced by a trusted lowerer
    /// for an identity-register-mapped block that does not require a guest stack
    /// (RSP is not loaded — the block runs on the host stack).
    #[cfg(target_arch = "x86_64")]
    pub fn run(&self, entry_offset: usize, regs: &mut GuestRegs) {
        let entry = unsafe { self.ptr.add(entry_offset) } as *const u8;
        unsafe { rax_smir_enter_native(entry, regs as *mut GuestRegs) };
    }

    /// Execute a state-backed AArch64-on-x86 lowered block.
    ///
    /// # Safety
    /// The mapped code must use the `extern "C" fn(*mut Aarch64GuestRegs)` ABI
    /// and preserve the host ABI. The AArch64 state-backed lowerer emits leaf
    /// functions using only caller-saved registers plus an RBP frame.
    pub fn run_aarch64(&self, entry_offset: usize, regs: &mut Aarch64GuestRegs) {
        type Entry = unsafe extern "C" fn(*mut Aarch64GuestRegs);
        let entry = unsafe { self.ptr.add(entry_offset) } as *const u8;
        let entry: Entry = unsafe { core::mem::transmute(entry) };
        unsafe { entry(regs as *mut Aarch64GuestRegs) };
    }

    /// Execute an identity-register-mapped AArch64 block on an AArch64 host.
    ///
    /// The block was lowered by `Aarch64Lowerer` under the 1:1 identity map
    /// (guest `Xn` ⇒ host `Xn`). [`rax_a64_enter_native`] marshals guest GPRs +
    /// NZCV from `regs` into the identical host registers, runs the block on the
    /// host stack, then writes the results back. Guest X18/X28/X30/SP are not
    /// mapped (reserved: platform / state-pointer / link / host-stack), so the
    /// block must not use them — enforced by the clobber gate.
    ///
    /// # Safety
    /// `entry_offset` must point at a block produced by a trusted AArch64
    /// identity lowerer that obeys the reserved-register contract above.
    #[cfg(target_arch = "aarch64")]
    pub fn run_aarch64_identity(&self, entry_offset: usize, regs: &mut Aarch64GuestRegs) {
        let entry = unsafe { self.ptr.add(entry_offset) } as *const u8;
        unsafe { rax_a64_enter_native(entry, regs as *mut Aarch64GuestRegs) };
    }

    /// As [`Self::run_aarch64_identity`] but for a region that uses scalar FP /
    /// SIMD: additionally marshals V0-V31 + FPCR/FPSR through the FP trampoline.
    /// Used only for regions whose ops touch V registers (the integer path keeps
    /// the cheaper GPR-only trampoline).
    ///
    /// # Safety
    /// As [`Self::run_aarch64_identity`].
    #[cfg(target_arch = "aarch64")]
    pub fn run_aarch64_identity_fp(&self, entry_offset: usize, regs: &mut Aarch64GuestRegs) {
        let entry = unsafe { self.ptr.add(entry_offset) } as *const u8;
        unsafe { rax_a64_enter_native_fp(entry, regs as *mut Aarch64GuestRegs) };
    }
}

impl Drop for ExecMem {
    fn drop(&mut self) {
        unsafe { libc::munmap(self.ptr as *mut libc::c_void, self.len) };
    }
}

// SAFETY: an ExecMem owns a private W^X mapping of immutable native code. After
// construction the bytes never change, and execution only reads them; the
// owning vcpu is the sole accessor. Sending the mapping to another thread (when
// a vcpu migrates) or sharing &ExecMem for read-only execution is therefore
// sound. The raw pointer alone makes ExecMem !Send/!Sync by default.
unsafe impl Send for ExecMem {}
unsafe impl Sync for ExecMem {}

/// Errors mapping/executing a lowered block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExecMemError {
    /// Empty code buffer.
    Empty,
    /// `mmap` failed.
    Mmap,
    /// `mprotect` to RX failed.
    Mprotect,
}

impl core::fmt::Display for ExecMemError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ExecMemError::Empty => write!(f, "empty code buffer"),
            ExecMemError::Mmap => write!(f, "mmap failed"),
            ExecMemError::Mprotect => write!(f, "mprotect to RX failed"),
        }
    }
}

impl std::error::Error for ExecMemError {}

/// Decide whether a lifted function is safe to execute through the native tier
/// under the 1:1 identity register map.
///
/// The identity map (guest GPR `N` ⇒ host GPR `N`) is what makes native
/// execution marshal-free, but it leaves *every* host GPR holding live guest
/// state — there is no free scratch register. So any value the block writes to a
/// `VReg::Virtual` (a non-architectural temporary the lifter introduced) would
/// be allocated onto a guest-occupied host register and silently corrupt guest
/// state on write-back. Such a block must NOT be promoted; the interpreter runs
/// it instead.
///
/// Exempt: a trailing `TestCondition` whose `dst` feeds the block's
/// `CondBranch` — the lowerer folds it into a direct `Jcc` off the live flags
/// and never materializes the temporary (see `X86_64Lowerer::lower_block`).
///
/// Pure architectural-register blocks (counter/pointer loops, ALU chains,
/// guest-conditional branches) pass — which is the bulk of hot code.
pub fn is_native_clobber_safe(func: &crate::smir::ir::SmirFunction) -> bool {
    func.blocks.iter().all(|b| block_is_clobber_safe(b, false))
}

/// Like [`is_native_clobber_safe`] but skips blocks in `excluded` (block-id ⇒
/// resume PC, i.e. the native-exit stubs). Those blocks are lowered to exit
/// stubs and never execute natively, so their ops can't clobber guest state —
/// excluding them lets the JIT accept regions whose loop is clobber-safe even
/// when an exit/continuation block uses a virtual temporary.
pub fn is_native_clobber_safe_excluding(
    func: &crate::smir::ir::SmirFunction,
    excluded: &std::collections::HashMap<crate::smir::types::BlockId, u64>,
    allow_mem: bool,
) -> bool {
    func.blocks
        .iter()
        .filter(|b| !excluded.contains_key(&b.id))
        .all(|b| block_is_clobber_safe(b, allow_mem))
}

/// True if every op in `block` is safe to execute natively under the JIT:
///   (1) it is on the fail-safe register-only whitelist ([`OpKind::is_jit_safe`])
///       — so it touches no memory and is validated bit-exact vs KVM; and
///   (2) it writes only architectural registers (no virtual temp, which would
///       alias a guest GPR under the identity register map).
/// A trailing `TestCondition` feeding the block's `CondBranch` is exempt (the
/// lowerer folds it into a direct `Jcc` and never materializes its dst).
fn block_is_clobber_safe(block: &crate::smir::ir::SmirBlock, allow_mem: bool) -> bool {
    use crate::smir::ir::Terminator;
    use crate::smir::ops::OpKind;
    use crate::smir::types::{ArchReg, VReg, X86Reg};

    // The native trampoline runs the region on the HOST stack: guest RSP is
    // never loaded into the host RSP, and the lowerer's prologue repurposes RBP
    // as the frame pointer (clobbering the guest RBP loaded in). So any op that
    // reads OR writes guest RSP/RBP would compute against the host RSP/RBP
    // instead of the guest value — silently wrong, and a write to RSP corrupts
    // the host stack. Such stack-frame code must stay in the interpreter. (The
    // differential fuzzers never generate RSP/RBP operands, so this gap was
    // invisible until real kernel code — which uses them constantly — was JIT'd.)
    let touches_sp_bp = |v: &VReg| {
        matches!(
            v,
            VReg::Arch(ArchReg::X86(X86Reg::Rsp)) | VReg::Arch(ArchReg::X86(X86Reg::Rbp))
        )
    };

    let n = block.ops.len();
    for (i, op) in block.ops.iter().enumerate() {
        if i + 1 == n {
            if let (Terminator::CondBranch { cond, .. }, OpKind::TestCondition { dst, .. }) =
                (&block.terminator, &op.kind)
            {
                if dst == cond {
                    continue;
                }
            }
        }
        // (1) fail-safe whitelist: any non-whitelisted op (div, FP/SIMD,
        // syscall, unvalidated) makes the whole region ineligible. When memory
        // JIT is enabled, register-destination Load/Store are additionally
        // allowed (they lower to MMU helper calls with fault-bail); RMW forms
        // still bail via the virtual-temp check below, and RSP/RBP-based
        // addresses via check (3).
        let mem_ok = allow_mem && matches!(op.kind, OpKind::Load { .. } | OpKind::Store { .. });
        if !op.kind.is_jit_safe() && !mem_ok {
            return false;
        }
        // (2) no virtual-temp writes (would clobber a guest GPR).
        if op
            .kind
            .dests()
            .iter()
            .any(|d| matches!(d, VReg::Virtual(_)))
        {
            return false;
        }
        // (3) guest RSP/RBP. A WRITE is never modeled (the trampoline freezes
        // both — see note above) → bail. A READ is fine ONLY as an operand of a
        // mem-JIT Load/Store (an address base/index, or a stored value): the MMU
        // helper reads the value from the GuestRegs struct — the correct frozen
        // guest RSP/RBP — not the host RSP/RBP. Any OTHER op reading RSP/RBP
        // would use the host frame pointer / host stack (wrong) → bail. (When
        // `allow_mem` is off, `mem_ok` is always false, so this is identical to
        // the prior "no RSP/RBP reads or writes" rule — the validated default.)
        if op.kind.dests().iter().any(touches_sp_bp) {
            return false;
        }
        if !mem_ok && op.kind.source_vregs().iter().any(touches_sp_bp) {
            return false;
        }
    }
    true
}

/// AArch64 analogue of [`is_native_clobber_safe_excluding`]: decide whether the
/// EXECUTED (non-exit) blocks of `func` are safe to run through the identity-map
/// AArch64 entry trampoline (`rax_a64_enter_native`). `excluded` holds the
/// native-exit (frontier) blocks, whose bodies never execute natively.
///
/// The identity map (guest `Xn` ⇒ host `Xn`) leaves every host GPR holding live
/// guest state, and the trampoline reserves host X18 (platform), X28 (state
/// pointer), X30 (link), and SP (host stack). So a block is unsafe if it:
///   1. uses a non-JIT-safe op (touches memory / has side effects / is
///      unvalidated) — except register-destination `Load`/`Store` when
///      `allow_mem` (they lower to MMU helper call-outs), and except `DivU`/
///      `DivS` which are clean on AArch64 (the shared [`OpKind::is_jit_safe`]
///      excludes them only to model x86's `#DE`);
///   2. writes a `VReg::Virtual` temporary (would alias a guest GPR); or
///   3. reads or writes guest X18/X28/X30/SP — a read is tolerated only as a
///      memory operand under `allow_mem` (the helper reads the frozen value
///      from the state struct, not the live host register).
/// A trailing `TestCondition` feeding the block's `CondBranch` is exempt (the
/// lowerer folds it into a `B.cond` and never materializes its dst).
pub fn is_aarch64_native_clobber_safe_excluding(
    func: &crate::smir::ir::SmirFunction,
    excluded: &std::collections::HashMap<crate::smir::types::BlockId, u64>,
    allow_mem: bool,
) -> bool {
    func.blocks
        .iter()
        .filter(|b| !excluded.contains_key(&b.id))
        .all(|b| aarch64_block_is_clobber_safe(b, allow_mem))
}

fn aarch64_block_is_clobber_safe(block: &crate::smir::ir::SmirBlock, allow_mem: bool) -> bool {
    use crate::smir::ir::Terminator;
    use crate::smir::ops::OpKind;
    use crate::smir::types::{ArchReg, ArmReg, VReg};

    // Reserved host registers under the identity-map trampoline. A guest write to
    // any of these clobbers host platform/state/link/stack; a guest read returns
    // the host (not guest) value. X28 holds the live state pointer; X18 is the
    // macOS platform register; X30 is the trampoline link; SP is the host stack
    // (guest SP is never loaded).
    let touches_reserved = |v: &VReg| {
        matches!(
            v,
            VReg::Arch(ArchReg::Arm(ArmReg::X(18)))
                | VReg::Arch(ArchReg::Arm(ArmReg::X(28)))
                | VReg::Arch(ArchReg::Arm(ArmReg::X(30)))
                | VReg::Arch(ArchReg::Arm(ArmReg::Sp))
        )
    };

    let n = block.ops.len();
    for (i, op) in block.ops.iter().enumerate() {
        if i + 1 == n {
            if let (Terminator::CondBranch { cond, .. }, OpKind::TestCondition { dst, .. }) =
                (&block.terminator, &op.kind)
            {
                if dst == cond {
                    continue;
                }
            }
        }
        let mem_ok = allow_mem && matches!(op.kind, OpKind::Load { .. } | OpKind::Store { .. });
        // AArch64-clean register-only ops that the x86-tuned `is_jit_safe`
        // whitelist omits: UDIV/SDIV never trap on AArch64 (no x86 `#DE`), and
        // CLZ/RBIT/REV(Bswap)/bitfield insert+extract are pure ALU ops the
        // native lowerer emits correctly (validated by the differential harness
        // in tests/aarch64_smir_native.rs). Admitting them lets the emulator JIT
        // real scalar loops that use them instead of deopting.
        let a64_ok = matches!(
            op.kind,
            OpKind::DivU { .. }
                | OpKind::DivS { .. }
                | OpKind::Clz { .. }
                | OpKind::Rbit { .. }
                | OpKind::Bswap { .. }
                | OpKind::Bfx { .. }
                | OpKind::Bfi { .. }
                // IEEE-exact / correctly-rounded scalar FP: lower to the native
                // f-ops and match the interpreter under default rounding (run via
                // the FP trampoline which marshals V0-V31 + FPCR/FPSR). The
                // directed-rounding/convert/min-max/fmov forms are deliberately
                // excluded (the lowerer has documented rounding/fusion deviations).
                | OpKind::FAdd { .. }
                | OpKind::FSub { .. }
                | OpKind::FMul { .. }
                | OpKind::FDiv { .. }
                | OpKind::FSqrt { .. }
                | OpKind::FAbs { .. }
                | OpKind::FNeg { .. }
        );
        if !op.kind.is_jit_safe() && !a64_ok && !mem_ok {
            return false;
        }
        if op
            .kind
            .dests()
            .iter()
            .any(|d| matches!(d, VReg::Virtual(_)))
        {
            return false;
        }
        if op.kind.dests().iter().any(touches_reserved) {
            return false;
        }
        if !mem_ok && op.kind.source_vregs().iter().any(touches_reserved) {
            return false;
        }
    }
    true
}

#[cfg(all(test, target_arch = "x86_64"))]
mod tests {
    use super::*;

    // Hand-assembled `mov eax, 0x2a ; ret` — proves ExecMem (W^X map) and the
    // enter_native trampoline marshal a result back out, independent of the
    // lowerer. The lowerer-driven end-to-end paths live in tests/diff_fuzz.rs.
    #[test]
    fn exec_mem_runs_raw_block() {
        let code = [0xB8, 0x2A, 0x00, 0x00, 0x00, 0xC3];
        let mem = ExecMem::new(&code).expect("ExecMem map");
        let mut regs = GuestRegs::default();
        regs.rflags = 0x2;
        mem.run(0, &mut regs);
        assert_eq!(regs.gpr[0], 0x2a, "RAX should be 0x2a");
    }

    // RAX = RBX + RCX, exercising guest-GPR marshal IN as well as OUT.
    //   lea eax,[rbx+rcx] won't preserve 64-bit; use: mov rax,rbx; add rax,rcx; ret
    #[test]
    fn exec_mem_marshals_inputs() {
        // 48 89 D8        mov rax, rbx
        // 48 01 C8        add rax, rcx
        // C3              ret
        let code = [0x48, 0x89, 0xD8, 0x48, 0x01, 0xC8, 0xC3];
        let mem = ExecMem::new(&code).expect("ExecMem map");
        let mut regs = GuestRegs::default();
        regs.gpr[3] = 40; // RBX
        regs.gpr[1] = 2; // RCX
        regs.rflags = 0x2;
        mem.run(0, &mut regs);
        assert_eq!(regs.gpr[0], 42, "RAX should be RBX+RCX");
    }

    // General-exit stub: a block (with the lowerer's `push rbp; mov rbp,rsp`
    // prologue) records its resume PC into exit_pc by loading the state pointer
    // from the trampoline frame into a push/pop-saved
    // scratch — no reserved guest register, runs under the existing trampoline.
    #[test]
    fn exec_mem_exit_pc_via_stub() {
        let mut code = vec![
            0x55, // push rbp
            0x48,
            0x89,
            0xE5, // mov rbp, rsp
            0x50, // push rax (scratch)
            0x48,
            0x8B,
            0x45,
            X86_STATE_PTR_AT_RBP as u8, // mov rax, [rbp+state_ptr]
            0xC7,
            0x80,
        ];
        code.extend_from_slice(&(X86_GUEST_EXIT_PC_OFFSET as u32).to_le_bytes());
        code.extend_from_slice(&0x1234_abcdu32.to_le_bytes());
        code.extend_from_slice(&[0xC7, 0x80]);
        code.extend_from_slice(&((X86_GUEST_EXIT_PC_OFFSET + 4) as u32).to_le_bytes());
        code.extend_from_slice(&0u32.to_le_bytes());
        code.extend_from_slice(&[
            0x58, // pop rax
            0x48, 0x89, 0xEC, // mov rsp, rbp
            0x5D, // pop rbp
            0xC3, // ret
        ]);
        let mem = ExecMem::new(&code).expect("ExecMem map");
        let mut regs = GuestRegs::default();
        regs.gpr[0] = 0xCAFE; // guest RAX must pass through (scratch restored)
        regs.rflags = 0x2;
        mem.run(0, &mut regs);
        assert_eq!(
            regs.exit_pc, 0x1234_abcd,
            "exit_pc recorded via frame state ptr"
        );
        assert_eq!(regs.gpr[0], 0xCAFE, "guest RAX restored after scratch use");
    }

    use crate::smir::flags::FlagUpdate;
    use crate::smir::ir::{FunctionBuilder, Terminator, TrapKind};
    use crate::smir::ops::OpKind;
    use crate::smir::types::{ArchReg, Condition, FunctionId, OpWidth, SrcOperand, VReg, X86Reg};

    fn rax() -> VReg {
        VReg::Arch(ArchReg::X86(X86Reg::Rax))
    }
    fn rcx() -> VReg {
        VReg::Arch(ArchReg::X86(X86Reg::Rcx))
    }

    #[test]
    fn clobber_gate_passes_pure_arch_block() {
        let mut b = FunctionBuilder::new(FunctionId(0), 0x1000);
        b.push_op(
            0x1000,
            OpKind::Add {
                dst: rax(),
                src1: rax(),
                src2: SrcOperand::Reg(rcx()),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        );
        b.set_terminator(Terminator::Return { values: vec![] });
        assert!(is_native_clobber_safe(&b.finish()));
    }

    #[test]
    fn clobber_gate_rejects_virtual_temp() {
        let mut b = FunctionBuilder::new(FunctionId(0), 0x1000);
        let tmp = b.alloc_vreg(); // VReg::Virtual
        b.push_op(
            0x1000,
            OpKind::Add {
                dst: tmp, // writes a virtual temporary -> would clobber a guest GPR
                src1: rax(),
                src2: SrcOperand::Reg(rcx()),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        );
        b.set_terminator(Terminator::Return { values: vec![] });
        assert!(!is_native_clobber_safe(&b.finish()));
    }

    #[test]
    fn clobber_gate_excludes_exit_blocks() {
        // entry: add rax,rcx (arch) → Branch to exit_blk
        // exit_blk: writes a VIRTUAL temp, then Trap (a frontier the JIT skips).
        let mut b = FunctionBuilder::new(FunctionId(0), 0x1000);
        let exit_blk = b.create_block(0x2000);
        b.push_op(
            0x1000,
            OpKind::Add {
                dst: rax(),
                src1: rax(),
                src2: SrcOperand::Reg(rcx()),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        );
        b.set_terminator(Terminator::Branch { target: exit_blk });
        b.switch_to_block(exit_blk);
        let tmp = b.alloc_vreg();
        b.push_op(
            0x2000,
            OpKind::Add {
                dst: tmp, // virtual temp — only safe because this block is skipped
                src1: rax(),
                src2: SrcOperand::Reg(rcx()),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        );
        b.set_terminator(Terminator::Trap {
            kind: TrapKind::Halt,
        });
        let func = b.finish();

        assert!(
            !is_native_clobber_safe(&func),
            "exit block's virtual write trips the strict gate"
        );
        let mut exits = std::collections::HashMap::new();
        exits.insert(exit_blk, 0x2000u64);
        assert!(
            is_native_clobber_safe_excluding(&func, &exits, false),
            "excluding the (skipped) exit block, the executed region is safe"
        );
    }

    #[test]
    fn clobber_gate_exempts_folded_testcondition() {
        let mut b = FunctionBuilder::new(FunctionId(0), 0x1000);
        let t_blk = b.create_block(0x2000);
        let f_blk = b.create_block(0x3000);
        let cond = b.alloc_vreg();
        b.push_op(
            0x1000,
            OpKind::Sub {
                dst: rcx(),
                src1: rcx(),
                src2: SrcOperand::imm(1),
                width: OpWidth::W64,
                flags: FlagUpdate::All,
            },
        );
        // Trailing TestCondition feeding the CondBranch: lowerer folds it, never
        // materializing `cond`, so the gate must treat the block as safe.
        b.push_op(
            0x1003,
            OpKind::TestCondition {
                dst: cond,
                cond: Condition::Ne,
            },
        );
        b.set_terminator(Terminator::CondBranch {
            cond,
            true_target: t_blk,
            false_target: f_blk,
        });
        b.switch_to_block(t_blk);
        b.set_terminator(Terminator::Return { values: vec![] });
        b.switch_to_block(f_blk);
        b.set_terminator(Terminator::Return { values: vec![] });
        assert!(is_native_clobber_safe(&b.finish()));
    }
}

#[cfg(all(test, target_arch = "aarch64"))]
mod tests_aarch64 {
    use super::*;

    // movz x0, #42 ; ret  → proves the MAP_JIT W^X mapping executes and the
    // identity trampoline marshals a result register back out, independent of
    // the lowerer. This is the AArch64 analogue of `exec_mem_runs_raw_block`.
    #[test]
    fn exec_mem_runs_raw_block_aarch64() {
        // d2800540 movz x0, #42 ; d65f03c0 ret
        let code: [u8; 8] = [0x40, 0x05, 0x80, 0xd2, 0xc0, 0x03, 0x5f, 0xd6];
        let mem = ExecMem::new(&code).expect("ExecMem map");
        let mut regs = Aarch64GuestRegs::default();
        mem.run_aarch64_identity(0, &mut regs);
        assert_eq!(regs.x[0], 42, "X0 should be 42");
    }

    // add x0, x1, x2 ; ret  → guest GPR marshal IN as well as OUT.
    #[test]
    fn exec_mem_marshals_inputs_aarch64() {
        // 8b020020 add x0, x1, x2 ; d65f03c0 ret
        let code: [u8; 8] = [0x20, 0x00, 0x02, 0x8b, 0xc0, 0x03, 0x5f, 0xd6];
        let mem = ExecMem::new(&code).expect("ExecMem map");
        let mut regs = Aarch64GuestRegs::default();
        regs.x[1] = 40;
        regs.x[2] = 2;
        mem.run_aarch64_identity(0, &mut regs);
        assert_eq!(regs.x[0], 42, "X0 should be X1+X2");
    }

    // Exercise the high callee-saved guest registers (x19..x29) round-trip,
    // since the trampoline loads/stores those via single ldr/str (not the ldp
    // pairs used for x0..x17).
    #[test]
    fn exec_mem_high_regs_roundtrip_aarch64() {
        // 8b150293 add x19, x20, x21 ; aa1303e0 mov x0, x19 ; d65f03c0 ret
        let code: [u8; 12] = [
            0x93, 0x02, 0x15, 0x8b, 0xe0, 0x03, 0x13, 0xaa, 0xc0, 0x03, 0x5f, 0xd6,
        ];
        let mem = ExecMem::new(&code).expect("ExecMem map");
        let mut regs = Aarch64GuestRegs::default();
        regs.x[20] = 100;
        regs.x[21] = 23;
        mem.run_aarch64_identity(0, &mut regs);
        assert_eq!(regs.x[19], 123, "X19 = X20 + X21");
        assert_eq!(regs.x[0], 123, "X0 = X19");
    }

    // subs x0, x1, x1 ; ret  → NZCV marshals out (5-5=0 sets Z and C).
    #[test]
    fn exec_mem_nzcv_roundtrip_aarch64() {
        // eb010020 subs x0, x1, x1 ; d65f03c0 ret
        let code: [u8; 8] = [0x20, 0x00, 0x01, 0xeb, 0xc0, 0x03, 0x5f, 0xd6];
        let mem = ExecMem::new(&code).expect("ExecMem map");
        let mut regs = Aarch64GuestRegs::default();
        regs.x[1] = 5;
        mem.run_aarch64_identity(0, &mut regs);
        assert_eq!(regs.x[0], 0, "5 - 5 = 0");
        assert_ne!(regs.nzcv & (1 << 30), 0, "Z (bit 30) set on zero result");
        assert_ne!(regs.nzcv & (1 << 29), 0, "C (bit 29) set: no borrow");
        assert_eq!(regs.nzcv & (1 << 31), 0, "N (bit 31) clear");
    }

    // FP trampoline: V-register + FPCR/FPSR marshaling. fadd d0,d1,d2 reads the
    // low 64 bits (f64) of V1,V2 and writes V0; this proves run_aarch64_identity_fp
    // marshals the SIMD/FP register file in and out.
    #[test]
    fn exec_mem_fp_marshals_v_regs_aarch64() {
        // 1e622820 fadd d0, d1, d2 ; d65f03c0 ret
        let code: [u8; 8] = [0x20, 0x28, 0x62, 0x1e, 0xc0, 0x03, 0x5f, 0xd6];
        let mem = ExecMem::new(&code).expect("ExecMem map");
        let mut regs = Aarch64GuestRegs::default();
        regs.v[2] = (2.0_f64).to_bits(); // V1 low (d1)
        regs.v[4] = (3.0_f64).to_bits(); // V2 low (d2)
        mem.run_aarch64_identity_fp(0, &mut regs);
        assert_eq!(f64::from_bits(regs.v[0]), 5.0, "V0 = V1 + V2 (f64)");
    }

    // The FP trampoline must still marshal GPRs/NZCV exactly like the scalar one.
    #[test]
    fn exec_mem_fp_trampoline_preserves_gprs_aarch64() {
        // 8b020020 add x0, x1, x2 ; d65f03c0 ret
        let code: [u8; 8] = [0x20, 0x00, 0x02, 0x8b, 0xc0, 0x03, 0x5f, 0xd6];
        let mem = ExecMem::new(&code).expect("ExecMem map");
        let mut regs = Aarch64GuestRegs::default();
        regs.x[1] = 40;
        regs.x[2] = 2;
        regs.x[25] = 0x1234_5678; // callee-saved guest reg must round-trip
        mem.run_aarch64_identity_fp(0, &mut regs);
        assert_eq!(regs.x[0], 42);
        assert_eq!(regs.x[25], 0x1234_5678);
    }

    // NZCV marshals IN: cset x0 reads the Z flag we seed in the struct.
    // cseteq x0  ==  csinc x0, xzr, xzr, ne  →  x0 = (Z==1) ? 1 : 0.
    #[test]
    fn exec_mem_nzcv_marshals_in_aarch64() {
        // 9a9f17e0 cset x0, eq ; d65f03c0 ret
        let code: [u8; 8] = [0xe0, 0x17, 0x9f, 0x9a, 0xc0, 0x03, 0x5f, 0xd6];
        let mem = ExecMem::new(&code).expect("ExecMem map");
        let mut regs = Aarch64GuestRegs::default();
        regs.nzcv = 1 << 30; // Z set
        mem.run_aarch64_identity(0, &mut regs);
        assert_eq!(regs.x[0], 1, "cset eq reads the seeded Z flag");
    }
}
