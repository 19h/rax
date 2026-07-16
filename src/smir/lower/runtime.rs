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
//!  * RISC-V on x86-64/AArch64: a state-backed native entry point that reads
//!    and writes [`RiscVGuestRegs`] directly.
//! The first two paths rely on the lowerer's 1:1 identity register map (guest
//! GPR `N` ⇒ the same-named host GPR), so their only marshalling is once on
//! entry and once on exit.
//!
//! The x86-64 path is validated bit-exact against KVM by the differential
//! harness in `tests/suites/differential/x86_64/fuzz.rs` (`smir_native_*` tests). The AArch64 path is
//! validated against the AArch64 interpreter.

#![cfg(feature = "smir-jit")]

use super::{
    X86_GUEST_APX_ENABLED_OFFSET, X86_GUEST_CALL_FN_OFFSET, X86_GUEST_CPL_OFFSET,
    X86_GUEST_CR0_OFFSET, X86_GUEST_CR4_OFFSET, X86_GUEST_CTX_OFFSET, X86_GUEST_EXIT_PC_OFFSET,
    X86_GUEST_FS_BASE_OFFSET, X86_GUEST_GPR_COUNT, X86_GUEST_GS_BASE_OFFSET, X86_GUEST_K_OFFSET,
    X86_GUEST_LOAD_FN_OFFSET, X86_GUEST_MXCSR_OFFSET, X86_GUEST_PAIR_LOAD_FN_OFFSET,
    X86_GUEST_PAIR_STORE_FN_OFFSET, X86_GUEST_RFLAGS_OFFSET, X86_GUEST_STORE_FN_OFFSET,
    X86_GUEST_TSC_AUX_OFFSET, X86_GUEST_VEC_LOAD_FN_OFFSET, X86_GUEST_VEC_STORE_FN_OFFSET,
    X86_GUEST_VECTOR_ACTIVE_OFFSET, X86_GUEST_XCR0_OFFSET, X86_GUEST_XGETBV1_OFFSET,
    X86_GUEST_ZMM_OFFSET, X86_HOST_MXCSR_OFFSET, X86_STATE_PTR_AT_RBP,
};

/// Apple I-cache invalidation (libSystem). Required after writing a `MAP_JIT`
/// region and before executing it: on AArch64 the instruction cache is not
/// coherent with the data cache, so freshly written code may otherwise execute
/// stale bytes.
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
unsafe extern "C" {
    fn sys_icache_invalidate(start: *mut core::ffi::c_void, len: usize);
}

#[cfg(all(target_arch = "x86_64", target_os = "macos"))]
fn running_under_rosetta() -> bool {
    static TRANSLATED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *TRANSLATED.get_or_init(|| {
        let mut translated = 0i32;
        let mut size = core::mem::size_of_val(&translated);
        let result = unsafe {
            libc::sysctlbyname(
                c"sysctl.proc_translated".as_ptr(),
                (&mut translated as *mut i32).cast(),
                &mut size,
                core::ptr::null_mut(),
                0,
            )
        };
        result == 0 && translated == 1
    })
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
#[repr(C, align(64))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
    /// Complete architectural ZMM0-ZMM31 state. XMM and YMM values occupy the
    /// corresponding low 128/256 bits. Kept in one canonical representation so
    /// the native trampoline can import/export the entire overlapping register
    /// file with one 64-byte transfer per physical register.
    pub zmm: [[u64; 8]; 32],
    /// AVX-512 architectural opmask registers K0-K7.
    pub k: [u64; 8],
    /// Non-zero only for a region containing an admitted native vector op. The
    /// trampoline branches around every AVX-512 instruction when this is zero,
    /// preserving GPR-only JIT execution on hosts without AVX-512 support.
    pub vector_active: u64,
    /// Guest architectural MXCSR control/status. Loaded before native vector
    /// execution and captured afterward.
    pub mxcsr: u32,
    /// Host-thread MXCSR saved by the trampoline. Helper call boundaries switch
    /// to this value so Rust code never executes under guest FP control state.
    pub host_mxcsr: u32,
    /// Guest IA32_TSC_AUX MSR. RDPID reads this state-backed value rather than
    /// exposing the host thread's processor identifier.
    pub tsc_aux: u32,
    /// Guest XCR0 extended-state enable bitmap.
    pub xcr0: u64,
    /// Guest XGETBV(ECX=1) XINUSE bitmap. The lowered instruction masks it by
    /// XCR0, matching the architectural definition of enabled in-use state.
    pub xgetbv1: u64,
    /// Guest CR4. XGETBV deoptimizes unless OSXSAVE (bit 18) is set, allowing
    /// the interpreter to deliver the architectural #UD precisely.
    pub cr4: u64,
    /// Guest CR0. XSETBV checks PE before enforcing CPL=0.
    pub cr0: u64,
    /// Current privilege level derived from CS.RPL.
    pub cpl: u64,
    /// Non-zero when the emulator exposes APX and permits XCR0.APX_F.
    pub apx_enabled: u64,
    /// Address of `extern "C" fn(state, addr, dst_idx, size, zero_upper) -> ok`.
    /// The helper writes a complete post-load ZMM slot in `state.zmm[dst_idx]`.
    pub vec_load_fn: u64,
    /// Address of `extern "C" fn(state, addr, src_idx, size) -> ok`.
    /// The helper reads the source bytes from `state.zmm[src_idx]`.
    pub vec_store_fn: u64,
    /// Address of `extern "C" fn(state, dst_low, dst_high) -> ok`.
    /// The helper performs one complete APX POP2 stack transfer and commits the
    /// two destinations plus RSP only after the complete 16-byte read succeeds.
    pub pair_load_fn: u64,
    /// Address of `extern "C" fn(state, src_low, src_high) -> ok`.
    /// The helper performs one complete APX PUSH2 stack transfer and commits
    /// RSP only after the complete 16-byte write succeeds.
    pub pair_store_fn: u64,
}

impl Default for GuestRegs {
    fn default() -> Self {
        Self {
            gpr: [0; X86_GUEST_GPR_COUNT],
            rflags: 0,
            exit_pc: 0,
            ctx: 0,
            load_fn: 0,
            store_fn: 0,
            fs_base: 0,
            gs_base: 0,
            call_fn: 0,
            zmm: [[0; 8]; 32],
            k: [0; 8],
            vector_active: 0,
            mxcsr: 0x1F80,
            host_mxcsr: 0,
            tsc_aux: 0,
            xcr0: 1,
            xgetbv1: 0,
            cr4: 0,
            cr0: 0,
            cpl: 0,
            apx_enabled: 0,
            vec_load_fn: 0,
            vec_store_fn: 0,
            pair_load_fn: 0,
            pair_store_fn: 0,
        }
    }
}

impl GuestRegs {
    /// Install one complete architectural vector register.
    pub fn set_zmm(&mut self, index: usize, value: [u64; 8]) {
        self.zmm[index] = value;
    }

    /// Read one complete architectural vector register.
    pub fn get_zmm(&self, index: usize) -> [u64; 8] {
        self.zmm[index]
    }
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
    /// Address of the vector-load helper
    /// `extern "C" fn(state: *mut Aarch64GuestRegs, addr, dst_idx, size) -> ok`.
    /// It reads `size` bytes from guest memory (via `state.ctx`), zero-pads to 16
    /// bytes, and writes them into `state.v[2*dst_idx..]`; the lowered code then
    /// reloads the destination V register from that slot. Reusing `v[]` as the
    /// transfer scratch avoids a separate buffer.
    pub vec_load_fn: u64,
    /// Address of the vector-store helper
    /// `extern "C" fn(state, addr, src_idx, size) -> ok`: reads `state.v[2*src_idx..]`
    /// (which the lowered code has just written from the source V register) and
    /// stores `size` bytes to guest memory.
    pub vec_store_fn: u64,
    /// Native-region exit metadata. Bit 0 records that an exit occurred; the
    /// remaining bits carry architecture-specific state changes that cannot be
    /// represented by `pc` alone (currently AArch32 CPSR.T interworking).
    pub exit_flags: u64,
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
            vec_load_fn: 0,
            vec_store_fn: 0,
            exit_flags: 0,
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
    pub const VEC_LOAD_FN_OFFSET: i32 = Self::EXCLUSIVE_VALID_OFFSET + 8;
    pub const VEC_STORE_FN_OFFSET: i32 = Self::VEC_LOAD_FN_OFFSET + 8;
    pub const EXIT_FLAGS_OFFSET: i32 = Self::VEC_STORE_FN_OFFSET + 8;

    /// A native exit, rather than an ordinary `Return`, recorded `pc`.
    pub const EXIT_VALID: u64 = 1 << 0;
    /// The AArch32 exit selected Thumb state.
    pub const EXIT_AARCH32_T: u64 = 1 << 1;
    /// [`Self::EXIT_AARCH32_T`] contains a CPSR.T update.
    pub const EXIT_AARCH32_T_VALID: u64 = 1 << 2;
}

/// AArch32 scalar register file used by the A32-on-AArch64 identity JIT.
///
/// A32 r0-r15 map to host W0-W15 for admitted register-only regions.  CPSR is
/// retained in its complete architectural representation; only NZCV (bits
/// 31:28) is imported into host PSTATE and merged back after execution.  The
/// remaining CPSR fields (Q, GE, endianness, interrupt masks, T, and mode) are
/// stable across a scalar native region except that a validated interworking
/// exit can explicitly replace T from the branch target's low bit.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Aarch32GuestRegs {
    /// A32 r0-r15, with r15 holding the dispatcher PC snapshot.
    pub r: [u32; 16],
    /// Complete AArch32 CPSR.
    pub cpsr: u32,
}

/// Control-flow result from an A32-on-AArch64 native region.
///
/// `exited` distinguishes a valid exit to guest address zero from an ordinary
/// SMIR `Return`; `pc` is the zero-extended 32-bit dispatcher target when an
/// exit occurred.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Aarch32NativeExit {
    pub exited: bool,
    pub pc: u64,
}

/// Runtime-helper configuration for an AArch32 region lowered through the
/// AArch64 identity trampoline.
///
/// The callbacks use the same AAPCS64 contract as [`Aarch64GuestRegs`]. The
/// load helper returns a `#[repr(C)] { value: u64, ok: u64 }`; the store helper
/// returns non-zero on success. A zero `ok` value exits the region at the
/// faulting SMIR operation without committing its destination or writeback.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Aarch32MemHelpers {
    /// Opaque helper context pointer.
    pub ctx: u64,
    /// Address of `extern "C" fn(ctx, addr, size, signed) -> (value, ok)`.
    pub load_fn: u64,
    /// Address of `extern "C" fn(ctx, addr, value, size) -> ok`.
    pub store_fn: u64,
}

pub use super::cross::riscv_x86_64_abi::{
    RISCV_FP_RESULT_INVALID, RiscVAtomicCasStatus, RiscVAtomicOpCode, RiscVFpOpCode,
    RiscVIntCryptoOpCode, RiscVMemoryOrderCode,
};

/// Two-register SysV result of [`RiscVGuestRegs::cas_fn`].
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RiscVAtomicCasResult {
    pub old: u64,
    /// [`RiscVAtomicCasStatus`] encoded as a `u64`.
    pub status: u64,
}

/// Two-register SysV result for atomic RMW and exclusive-access helpers.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RiscVAtomicResult {
    pub value: u64,
    /// One for a completed access and zero for a guest-memory fault.
    pub access_success: u64,
}

/// Two-register SysV result of [`RiscVGuestRegs::load_fn`].
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RiscVLoadResult {
    pub value: u64,
    /// One for a completed access and zero for a guest-memory fault.
    pub success: u64,
}

/// Two-register native-ABI result of [`RiscVGuestRegs::fp_fn`].
///
/// A valid operation returns the raw destination in `value` and the updated
/// FCSR in `fcsr_status`. An illegal operation or rounding mode returns
/// [`RISCV_FP_RESULT_INVALID`] in `fcsr_status`; generated code then traps
/// without committing either destination.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RiscVFpResult {
    pub value: u64,
    pub fcsr_status: u64,
}

/// State ABI for RISC-V SMIR lowered to an x86-64 or AArch64 host.
///
/// The state-backed cross-lowerer accesses every field through the first native
/// ABI argument (RDI under x86-64 SysV; X0 under AAPCS64). All scalar fields use
/// eight-byte slots, including `fcsr`, so the layout is identical for RV32 and
/// RV64. Helper signatures follow the host ABI: x86-64 SysV or AAPCS64.
/// Atomic helpers share `ctx` and implement one indivisible transaction per
/// call. Integer-crypto and scalar-FP helpers are pure. `vector_fn(state, insn,
/// xlen)` returns exact success as one; every other status must leave both this
/// state and guest memory unchanged.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RiscVGuestRegs {
    /// Integer registers x0-x31.  The lowerer hard-wires reads of x0 to zero and
    /// discards writes, independently of this backing slot.
    pub x: [u64; 32],
    /// Floating-point registers f0-f31 as raw IEEE-754/NaN-boxed bits.
    pub f: [u64; 32],
    /// Program counter at dispatcher re-entry.
    pub pc: u64,
    /// FCSR in bits 7:0; upper bits are reserved and kept zero by the guest ISA.
    pub fcsr: u64,
    /// Native exit classification: 0=return/branch, 1=trap, 2=syscall,
    /// 3=breakpoint.
    pub exit_reason: u64,
    /// Opaque memory-helper context.
    pub ctx: u64,
    /// Address of `extern "sysv64" fn(ctx, addr, size, signed) -> {value, success}`.
    pub load_fn: u64,
    /// Address of `extern "sysv64" fn(ctx, addr, value, size) -> success`.
    /// Returning zero must leave guest memory unchanged.
    pub store_fn: u64,
    /// `extern "sysv64" fn(ctx, addr, operand, size, op_code, order_code) ->
    /// {old, access_success}`.
    pub atomic_rmw_fn: u64,
    /// `extern "sysv64" fn(ctx, addr, expected, new, size, order_code) ->
    /// {old, status}`, where status is [`RiscVAtomicCasStatus`].
    pub cas_fn: u64,
    /// `extern "sysv64" fn(ctx, addr, expected_lo, expected_hi, new_lo,
    /// new_hi, success_order_code, failure_order_code, out_old_hi) ->
    /// {old_lo, status}`. `out_old_hi` receives the high word only for a
    /// completed access.
    pub cas_pair_fn: u64,
    /// `extern "sysv64" fn(ctx, addr, size) -> {value, access_success}`;
    /// establishes a reservation on success.
    pub load_exclusive_fn: u64,
    /// `extern "sysv64" fn(ctx, addr, value, size) ->
    /// {reservation_success, access_success}`.
    pub store_exclusive_fn: u64,
    /// `extern "sysv64" fn(ctx)`; clears the reservation without storing.
    pub clear_exclusive_fn: u64,
    /// `extern "sysv64" fn(op_code, src1, src2, imm, xlen) -> value`.
    pub int_crypto_fn: u64,
    /// `extern "sysv64" fn(op_code, rm, fcsr, a, b, c) -> {value, fcsr_status}`.
    pub fp_fn: u64,
    /// Raw 128-bit RVV register file, v0-v31.
    pub v: [[u8; 16]; 32],
    /// RVV vector length CSR (`vl`).
    pub vl: u64,
    /// RVV vector type CSR (`vtype`).
    pub vtype: u64,
    /// RVV vector restart CSR (`vstart`).
    pub vstart: u64,
    /// RVV fixed-point control/status CSR (`vcsr`).
    pub vcsr: u64,
    /// `extern "sysv64" fn(state, insn, xlen) -> success`.
    /// Non-success must be transactional with respect to state and memory.
    pub vector_fn: u64,
    /// Zcmt jump-vector-table CSR. Bits 5:0 are the WARL mode field and are
    /// zero for the only currently defined mode.
    pub jvt: u64,
}

impl Default for RiscVGuestRegs {
    fn default() -> Self {
        Self {
            x: [0; 32],
            f: [0; 32],
            pc: 0,
            fcsr: 0,
            exit_reason: 0,
            ctx: 0,
            load_fn: 0,
            store_fn: 0,
            atomic_rmw_fn: 0,
            cas_fn: 0,
            cas_pair_fn: 0,
            load_exclusive_fn: 0,
            store_exclusive_fn: 0,
            clear_exclusive_fn: 0,
            int_crypto_fn: 0,
            fp_fn: 0,
            v: [[0; 16]; 32],
            vl: 0,
            vtype: 0,
            vstart: 0,
            vcsr: 0,
            vector_fn: 0,
            jvt: 0,
        }
    }
}

impl RiscVGuestRegs {
    pub const X_OFFSET: i32 = 0;
    pub const F_OFFSET: i32 = 32 * 8;
    pub const PC_OFFSET: i32 = Self::F_OFFSET + 32 * 8;
    pub const FCSR_OFFSET: i32 = Self::PC_OFFSET + 8;
    pub const EXIT_REASON_OFFSET: i32 = Self::FCSR_OFFSET + 8;
    pub const CTX_OFFSET: i32 = Self::EXIT_REASON_OFFSET + 8;
    pub const LOAD_FN_OFFSET: i32 = Self::CTX_OFFSET + 8;
    pub const STORE_FN_OFFSET: i32 = Self::LOAD_FN_OFFSET + 8;
    pub const ATOMIC_RMW_FN_OFFSET: i32 = Self::STORE_FN_OFFSET + 8;
    pub const CAS_FN_OFFSET: i32 = Self::ATOMIC_RMW_FN_OFFSET + 8;
    pub const CAS_PAIR_FN_OFFSET: i32 = Self::CAS_FN_OFFSET + 8;
    pub const LOAD_EXCLUSIVE_FN_OFFSET: i32 = Self::CAS_PAIR_FN_OFFSET + 8;
    pub const STORE_EXCLUSIVE_FN_OFFSET: i32 = Self::LOAD_EXCLUSIVE_FN_OFFSET + 8;
    pub const CLEAR_EXCLUSIVE_FN_OFFSET: i32 = Self::STORE_EXCLUSIVE_FN_OFFSET + 8;
    pub const INT_CRYPTO_FN_OFFSET: i32 = Self::CLEAR_EXCLUSIVE_FN_OFFSET + 8;
    pub const FP_FN_OFFSET: i32 = Self::INT_CRYPTO_FN_OFFSET + 8;
    pub const V_OFFSET: i32 = Self::FP_FN_OFFSET + 8;
    pub const VL_OFFSET: i32 = Self::V_OFFSET + 32 * 16;
    pub const VTYPE_OFFSET: i32 = Self::VL_OFFSET + 8;
    pub const VSTART_OFFSET: i32 = Self::VTYPE_OFFSET + 8;
    pub const VCSR_OFFSET: i32 = Self::VSTART_OFFSET + 8;
    pub const VECTOR_FN_OFFSET: i32 = Self::VCSR_OFFSET + 8;
    pub const JVT_OFFSET: i32 = Self::VECTOR_FN_OFFSET + 8;
}

// enter_native(rdi = entry ptr, rsi = *mut GuestRegs):
//   preserve host callee-saved -> load guest GPRs+RFLAGS and, for an admitted
//   vector region, ZMM0-ZMM31+K0-K7 into the identical host regs -> `call` the
//   block -> store the live architectural state back into GuestRegs.
// RSP (gpr[4]) is NOT loaded — the block runs on the host stack (it owns no
// guest stack). Alignment: 6 callee pushes (48) + `sub rsp,24` (72 total) leaves
// rsp 16-aligned at the `call`.
#[cfg(target_arch = "x86_64")]
macro_rules! x86_enter_native_trampoline {
    ($global:literal, $type_directive:literal, $label:literal) => {
        core::arch::global_asm!(
            ".text",
            ".p2align 4",
            $global,
            $type_directive,
            $label,
            "push rbp",
            "push rbx",
            "push r12",
            "push r13",
            "push r14",
            "push r15",
            "sub rsp, 24", // [rsp]=entry [rsp+8]=state [rsp+16]=pad ; rsp 16-aligned
            "mov [rsp], rdi",
            "mov [rsp+8], rsi",
            // Preserve host FP control/status before any guest state is loaded.
            // Helper call boundaries use the saved copy while executing Rust.
            "stmxcsr [rsi+2444]",
            // Vector state is optional. The branch executes before guest RFLAGS
            // are installed, so its CMP cannot perturb architectural flags.
            "cmp qword ptr [rsi+2432], 0",
            "je 2f",
            "vmovdqu64 zmm0,  [rsi+320]",
            "vmovdqu64 zmm1,  [rsi+384]",
            "vmovdqu64 zmm2,  [rsi+448]",
            "vmovdqu64 zmm3,  [rsi+512]",
            "vmovdqu64 zmm4,  [rsi+576]",
            "vmovdqu64 zmm5,  [rsi+640]",
            "vmovdqu64 zmm6,  [rsi+704]",
            "vmovdqu64 zmm7,  [rsi+768]",
            "vmovdqu64 zmm8,  [rsi+832]",
            "vmovdqu64 zmm9,  [rsi+896]",
            "vmovdqu64 zmm10, [rsi+960]",
            "vmovdqu64 zmm11, [rsi+1024]",
            "vmovdqu64 zmm12, [rsi+1088]",
            "vmovdqu64 zmm13, [rsi+1152]",
            "vmovdqu64 zmm14, [rsi+1216]",
            "vmovdqu64 zmm15, [rsi+1280]",
            "vmovdqu64 zmm16, [rsi+1344]",
            "vmovdqu64 zmm17, [rsi+1408]",
            "vmovdqu64 zmm18, [rsi+1472]",
            "vmovdqu64 zmm19, [rsi+1536]",
            "vmovdqu64 zmm20, [rsi+1600]",
            "vmovdqu64 zmm21, [rsi+1664]",
            "vmovdqu64 zmm22, [rsi+1728]",
            "vmovdqu64 zmm23, [rsi+1792]",
            "vmovdqu64 zmm24, [rsi+1856]",
            "vmovdqu64 zmm25, [rsi+1920]",
            "vmovdqu64 zmm26, [rsi+1984]",
            "vmovdqu64 zmm27, [rsi+2048]",
            "vmovdqu64 zmm28, [rsi+2112]",
            "vmovdqu64 zmm29, [rsi+2176]",
            "vmovdqu64 zmm30, [rsi+2240]",
            "vmovdqu64 zmm31, [rsi+2304]",
            "kmovq k0, [rsi+2368]",
            "kmovq k1, [rsi+2376]",
            "kmovq k2, [rsi+2384]",
            "kmovq k3, [rsi+2392]",
            "kmovq k4, [rsi+2400]",
            "kmovq k5, [rsi+2408]",
            "kmovq k6, [rsi+2416]",
            "kmovq k7, [rsi+2424]",
            "ldmxcsr [rsi+2440]",
            "2:",
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
            // Guest flags are captured above, so the vector-active test can no
            // longer alter the state returned to the emulator.
            "cmp qword ptr [rax+2432], 0",
            "je 3f",
            "vmovdqu64 [rax+320],  zmm0",
            "vmovdqu64 [rax+384],  zmm1",
            "vmovdqu64 [rax+448],  zmm2",
            "vmovdqu64 [rax+512],  zmm3",
            "vmovdqu64 [rax+576],  zmm4",
            "vmovdqu64 [rax+640],  zmm5",
            "vmovdqu64 [rax+704],  zmm6",
            "vmovdqu64 [rax+768],  zmm7",
            "vmovdqu64 [rax+832],  zmm8",
            "vmovdqu64 [rax+896],  zmm9",
            "vmovdqu64 [rax+960],  zmm10",
            "vmovdqu64 [rax+1024], zmm11",
            "vmovdqu64 [rax+1088], zmm12",
            "vmovdqu64 [rax+1152], zmm13",
            "vmovdqu64 [rax+1216], zmm14",
            "vmovdqu64 [rax+1280], zmm15",
            "vmovdqu64 [rax+1344], zmm16",
            "vmovdqu64 [rax+1408], zmm17",
            "vmovdqu64 [rax+1472], zmm18",
            "vmovdqu64 [rax+1536], zmm19",
            "vmovdqu64 [rax+1600], zmm20",
            "vmovdqu64 [rax+1664], zmm21",
            "vmovdqu64 [rax+1728], zmm22",
            "vmovdqu64 [rax+1792], zmm23",
            "vmovdqu64 [rax+1856], zmm24",
            "vmovdqu64 [rax+1920], zmm25",
            "vmovdqu64 [rax+1984], zmm26",
            "vmovdqu64 [rax+2048], zmm27",
            "vmovdqu64 [rax+2112], zmm28",
            "vmovdqu64 [rax+2176], zmm29",
            "vmovdqu64 [rax+2240], zmm30",
            "vmovdqu64 [rax+2304], zmm31",
            "kmovq [rax+2368], k0",
            "kmovq [rax+2376], k1",
            "kmovq [rax+2384], k2",
            "kmovq [rax+2392], k3",
            "kmovq [rax+2400], k4",
            "kmovq [rax+2408], k5",
            "kmovq [rax+2416], k6",
            "kmovq [rax+2424], k7",
            "stmxcsr [rax+2440]",
            "3:",
            "ldmxcsr [rax+2444]",
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
    };
}

#[cfg(all(target_arch = "x86_64", target_vendor = "apple"))]
x86_enter_native_trampoline!(
    ".globl _rax_smir_enter_native",
    "",
    "_rax_smir_enter_native:"
);

#[cfg(all(target_arch = "x86_64", not(target_vendor = "apple")))]
x86_enter_native_trampoline!(
    ".globl rax_smir_enter_native",
    ".type rax_smir_enter_native,@function",
    "rax_smir_enter_native:"
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
    "mrs x9, fpcr", // guest FPCR out (MSR FPCR inside region may update it)
    "str x9, [x28, #272]",
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
/// code in, then flips to RX. Drop normally unmaps it; x86-64 code running under
/// Rosetta retains an inaccessible virtual-address reservation to prevent stale
/// translated-code aliasing.
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

    /// Address range of the executable mapping, for async-signal-safe JIT
    /// crash diagnostics. The returned span includes page-alignment padding.
    #[cfg(target_arch = "x86_64")]
    pub(crate) fn mapping_bounds(&self) -> (*const u8, usize) {
        (self.ptr.cast_const(), self.len)
    }

    /// RW map → copy → `mprotect` RX. Used on x86-64 and any non-aarch64 Unix.
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

    /// Execute a state-backed RISC-V-on-x86-64 lowered block.
    ///
    /// # Safety
    /// The mapped code must have been emitted for the
    /// `extern "sysv64" fn(*mut RiscVGuestRegs)` ABI and must preserve it.
    #[cfg(target_arch = "x86_64")]
    pub fn run_riscv(&self, entry_offset: usize, regs: &mut RiscVGuestRegs) {
        type Entry = unsafe extern "sysv64" fn(*mut RiscVGuestRegs);
        let entry = unsafe { self.ptr.add(entry_offset) } as *const u8;
        let entry: Entry = unsafe { core::mem::transmute(entry) };
        unsafe { entry(regs as *mut RiscVGuestRegs) };
    }

    /// Execute a state-backed RISC-V-on-AArch64 lowered block.
    ///
    /// # Safety
    /// The mapped code must have been emitted for the
    /// `extern "C" fn(*mut RiscVGuestRegs)` AAPCS64 ABI and must preserve it.
    #[cfg(target_arch = "aarch64")]
    pub fn run_riscv(&self, entry_offset: usize, regs: &mut RiscVGuestRegs) {
        type Entry = unsafe extern "C" fn(*mut RiscVGuestRegs);
        let entry = unsafe { self.ptr.add(entry_offset) } as *const u8;
        let entry: Entry = unsafe { core::mem::transmute(entry) };
        unsafe { entry(regs as *mut RiscVGuestRegs) };
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

    /// Execute an admitted register-only AArch32 region on an AArch64 host.
    ///
    /// The native block uses the same identity mapping as the AArch64 scalar
    /// trampoline, but every architectural value is narrowed to 32 bits at the
    /// ABI boundary.  [`is_aarch32_aarch64_native_clobber_safe_excluding`]
    /// guarantees that the block cannot observe AArch64-only registers, host
    /// SP, or the guest PC pipeline value.
    #[cfg(target_arch = "aarch64")]
    pub fn run_aarch32_identity(&self, entry_offset: usize, regs: &mut Aarch32GuestRegs) {
        let _ = self.run_aarch32_identity_until_exit(entry_offset, regs);
    }

    /// Execute an admitted register-only AArch32 control-flow region.
    ///
    /// This is the control-flow-aware form of [`Self::run_aarch32_identity`].
    /// It returns the guest PC recorded by a native exit. This compatibility
    /// form cannot distinguish an exit to guest address zero from an ordinary
    /// empty `Return`; use [`Self::run_aarch32_identity_exit`] when that
    /// distinction matters. The function must be lowered with matching exit
    /// metadata and modes.
    #[cfg(target_arch = "aarch64")]
    pub fn run_aarch32_identity_until_exit(
        &self,
        entry_offset: usize,
        regs: &mut Aarch32GuestRegs,
    ) -> u64 {
        self.run_aarch32_identity_exit(entry_offset, regs).pc
    }

    /// Execute an admitted AArch32 control-flow region and return an
    /// unambiguous exit record. Unlike [`Self::run_aarch32_identity_until_exit`],
    /// this distinguishes an exit to address zero from an ordinary return.
    #[cfg(target_arch = "aarch64")]
    pub fn run_aarch32_identity_exit(
        &self,
        entry_offset: usize,
        regs: &mut Aarch32GuestRegs,
    ) -> Aarch32NativeExit {
        self.run_aarch32_identity_configured(entry_offset, regs, None)
    }

    /// Execute an admitted AArch32 region with MMU memory-helper call-outs.
    ///
    /// Returns the guest PC recorded by a helper fault or native frontier exit;
    /// zero means that a self-contained region returned without recording an
    /// exit. The caller must lower the region with memory helpers enabled and
    /// 32-bit helper-address arithmetic.
    #[cfg(target_arch = "aarch64")]
    pub fn run_aarch32_identity_with_mem(
        &self,
        entry_offset: usize,
        regs: &mut Aarch32GuestRegs,
        helpers: Aarch32MemHelpers,
    ) -> u64 {
        self.run_aarch32_identity_configured(entry_offset, regs, Some(helpers))
            .pc
    }

    #[cfg(target_arch = "aarch64")]
    fn run_aarch32_identity_configured(
        &self,
        entry_offset: usize,
        regs: &mut Aarch32GuestRegs,
        helpers: Option<Aarch32MemHelpers>,
    ) -> Aarch32NativeExit {
        const NZCV: u32 = 0xf000_0000;
        const CPSR_T: u32 = 1 << 5;

        let mut state = Aarch64GuestRegs::default();
        for (dst, src) in state.x[..16].iter_mut().zip(regs.r) {
            *dst = u64::from(src);
        }
        state.nzcv = u64::from(regs.cpsr & NZCV);
        if let Some(helpers) = helpers {
            state.ctx = helpers.ctx;
            state.load_fn = helpers.load_fn;
            state.store_fn = helpers.store_fn;
        }
        self.run_aarch64_identity(entry_offset, &mut state);
        for (dst, src) in regs.r.iter_mut().zip(state.x[..16].iter()) {
            *dst = *src as u32;
        }
        regs.cpsr = (regs.cpsr & !NZCV) | (state.nzcv as u32 & NZCV);
        if state.exit_flags & Aarch64GuestRegs::EXIT_AARCH32_T_VALID != 0 {
            regs.cpsr &= !CPSR_T;
            if state.exit_flags & Aarch64GuestRegs::EXIT_AARCH32_T != 0 {
                regs.cpsr |= CPSR_T;
            }
        }
        Aarch32NativeExit {
            exited: state.exit_flags & Aarch64GuestRegs::EXIT_VALID != 0,
            pc: state.pc,
        }
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
        #[cfg(all(target_arch = "x86_64", target_os = "macos"))]
        if running_under_rosetta() {
            // Rosetta can retain a translated block after munmap and reuse it
            // if another thread immediately receives the same executable
            // virtual address. Keep the address reserved for this process while
            // releasing its physical pages, which prevents stale-code aliasing.
            unsafe {
                libc::mprotect(self.ptr as *mut libc::c_void, self.len, libc::PROT_NONE);
                libc::madvise(self.ptr as *mut libc::c_void, self.len, libc::MADV_DONTNEED);
            }
            return;
        }
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
/// Exemptions are virtual values that the lowerer proves it never
/// materializes: a trailing `TestCondition` whose `dst` feeds the block's
/// `CondBranch`, and (when MMU helpers are enabled) single-use load temporaries
/// in exact x86 memory-source ALU/count/CRC32 pairs or the pre-decrement RSP
/// snapshot in PUSH RSP. The former folds to a direct `Jcc`; the memory forms
/// fold to exact helper-backed operations.
///
/// Pure architectural-register blocks (counter/pointer loops, ALU chains,
/// guest-conditional branches) pass — which is the bulk of hot code. Validated
/// Validated MOV/ADD/SUB forms involving guest RSP/RBP use their state-backed
/// slots rather than the corresponding host stack/frame registers.
pub fn is_native_clobber_safe(func: &crate::smir::ir::SmirFunction) -> bool {
    is_native_clobber_safe_excluding(func, &std::collections::HashMap::new(), false)
}

/// Like [`is_native_clobber_safe`] but skips blocks in `excluded` (block-id ⇒
/// resume PC, i.e. the native-exit stubs). Those blocks are lowered to exit
/// stubs and never execute natively, so their ops can't clobber guest state —
/// excluding them lets the JIT accept regions whose loop is clobber-safe even
/// when an exit/continuation block uses a virtual temporary.
pub fn is_native_clobber_safe_excluding(
    func: &crate::smir::ir::SmirFunction,
    excluded: &std::collections::HashMap<crate::smir::ir::types::BlockId, u64>,
    allow_mem: bool,
) -> bool {
    let flag_live_in = x86_flag_live_in(func, excluded);
    func.blocks
        .iter()
        .filter(|b| !excluded.contains_key(&b.id))
        .all(|b| {
            let flags_live_out = x86_block_flag_live_out(b, excluded, &flag_live_in);
            block_is_clobber_safe(b, allow_mem, flags_live_out)
        })
}

/// Convert the four x86 status flags representable by AArch64 PSTATE into
/// architectural NZCV bit positions. PF/AF and every control flag remain in the
/// x86 state object and are deliberately not encoded here.
pub fn x86_rflags_to_aarch64_nzcv(rflags: u64) -> u64 {
    const X86_CF: u64 = 1 << 0;
    const X86_ZF: u64 = 1 << 6;
    const X86_SF: u64 = 1 << 7;
    const X86_OF: u64 = 1 << 11;
    const A64_N: u64 = 1 << 31;
    const A64_Z: u64 = 1 << 30;
    const A64_C: u64 = 1 << 29;
    const A64_V: u64 = 1 << 28;

    (u64::from(rflags & X86_SF != 0) * A64_N)
        | (u64::from(rflags & X86_ZF != 0) * A64_Z)
        | (u64::from(rflags & X86_CF != 0) * A64_C)
        | (u64::from(rflags & X86_OF != 0) * A64_V)
}

/// Merge architectural NZCV back into an x86 RFLAGS snapshot. Exactly
/// CF/ZF/SF/OF are replaced; PF/AF, control flags, reserved bits, and all other
/// state are preserved from `prior_rflags`.
pub fn merge_aarch64_nzcv_into_x86_rflags(prior_rflags: u64, nzcv: u64) -> u64 {
    const X86_CF: u64 = 1 << 0;
    const X86_ZF: u64 = 1 << 6;
    const X86_SF: u64 = 1 << 7;
    const X86_OF: u64 = 1 << 11;
    const X86_NZCV: u64 = X86_CF | X86_ZF | X86_SF | X86_OF;
    const A64_N: u64 = 1 << 31;
    const A64_Z: u64 = 1 << 30;
    const A64_C: u64 = 1 << 29;
    const A64_V: u64 = 1 << 28;

    (prior_rflags & !X86_NZCV)
        | (u64::from(nzcv & A64_C != 0) * X86_CF)
        | (u64::from(nzcv & A64_Z != 0) * X86_ZF)
        | (u64::from(nzcv & A64_N != 0) * X86_SF)
        | (u64::from(nzcv & A64_V != 0) * X86_OF)
}

/// Decide whether x86-lifted SMIR can execute through the AArch64 identity-map
/// trampoline without changing architectural meaning. This is intentionally a
/// separate gate from [`is_aarch64_native_clobber_safe_excluding`], which models
/// an AArch64 guest and therefore has different register and flag semantics.
///
/// The initial production bridge is register-only and maps legacy x86 GPRs
/// RAX..R15 to X0..X15. It admits only operations already in the scalar JIT
/// whitelist (plus validated BMI/ADX scalar forms), rejects virtual writes and
/// non-legacy register operands, and applies an x86-specific flag-liveness pass:
///
/// - PF/AF have no NZCV representation, so a definition is allowed only when it
///   is dead before any use or native exit; parity consumers always bail.
/// - NZV and carry-producing operations use the canonical CF→C mapping.
/// - AArch64 subtraction exposes no-borrow in C, the inverse of x86 CF. A live
///   CF definition by SUB/CMP/NEG therefore bails. SBB is admitted because its
///   x86-register lowering explicitly inverts C before and after SBC. Generic
///   CF-based unsigned conditions still bail pending equivalent normalization.
pub fn is_x86_aarch64_native_clobber_safe_excluding(
    func: &crate::smir::ir::SmirFunction,
    excluded: &std::collections::HashMap<crate::smir::ir::types::BlockId, u64>,
) -> bool {
    let flag_live_in = x86_flag_live_in(func, excluded);
    func.blocks
        .iter()
        .filter(|block| !excluded.contains_key(&block.id))
        .all(|block| {
            let flags_live_out = x86_block_flag_live_out(block, excluded, &flag_live_in);
            x86_aarch64_block_is_clobber_safe(block, flags_live_out)
        })
}

/// Verify host support for scalar x86 extensions emitted directly by the
/// identity-register native JIT. Generic scalar lowerings use baseline x86-64;
/// Encoding-hinted MULX, scalar BMI/ADX operations, and native count operations
/// require additional CPUID features; CRC32C requires SSE4.2, while RDRAND and
/// RDSEED retain their independent architectural feature gates. Excluded exit
/// blocks do not execute natively and therefore do not contribute feature
/// requirements.
pub fn x86_native_scalar_features_supported_excluding(
    func: &crate::smir::ir::SmirFunction,
    excluded: &std::collections::HashMap<crate::smir::ir::types::BlockId, u64>,
) -> bool {
    let (needs_bmi2, needs_bmi1, needs_lzcnt, needs_popcnt, needs_adx) =
        x86_native_scalar_feature_requirements_excluding(func, excluded);
    let needs_sse42 = func
        .blocks
        .iter()
        .filter(|block| !excluded.contains_key(&block.id))
        .flat_map(|block| &block.ops)
        .any(|op| matches!(op.kind, crate::smir::ir::ops::OpKind::Crc32C { .. }));
    let (needs_rdrand, needs_rdseed) = func
        .blocks
        .iter()
        .filter(|block| !excluded.contains_key(&block.id))
        .flat_map(|block| &block.ops)
        .fold((false, false), |(rdrand, rdseed), op| match op.kind {
            crate::smir::ir::ops::OpKind::X86Random { seed, .. } => {
                (rdrand || !seed, rdseed || seed)
            }
            _ => (rdrand, rdseed),
        });

    #[cfg(target_arch = "x86_64")]
    {
        (!needs_bmi2 || std::is_x86_feature_detected!("bmi2"))
            && (!needs_bmi1 || std::is_x86_feature_detected!("bmi1"))
            && (!needs_lzcnt || std::is_x86_feature_detected!("lzcnt"))
            && (!needs_popcnt || std::is_x86_feature_detected!("popcnt"))
            && (!needs_adx || std::is_x86_feature_detected!("adx"))
            && (!needs_sse42 || std::is_x86_feature_detected!("sse4.2"))
            && (!needs_rdrand || std::is_x86_feature_detected!("rdrand"))
            && (!needs_rdseed || std::is_x86_feature_detected!("rdseed"))
    }

    #[cfg(not(target_arch = "x86_64"))]
    {
        !(needs_bmi2
            || needs_bmi1
            || needs_lzcnt
            || needs_popcnt
            || needs_adx
            || needs_sse42
            || needs_rdrand
            || needs_rdseed)
    }
}

fn x86_native_scalar_feature_requirements_excluding(
    func: &crate::smir::ir::SmirFunction,
    excluded: &std::collections::HashMap<crate::smir::ir::types::BlockId, u64>,
) -> (bool, bool, bool, bool, bool) {
    use crate::smir::ir::ops::X86OpHint;

    use crate::smir::ir::ops::{OpKind, X86CountKind};

    let mut needs_bmi2 = false;
    let mut needs_bmi1 = false;
    let mut needs_lzcnt = false;
    let mut needs_popcnt = false;
    let mut needs_adx = false;
    for op in func
        .blocks
        .iter()
        .filter(|block| !excluded.contains_key(&block.id))
        .flat_map(|block| &block.ops)
    {
        needs_bmi2 |= matches!(op.x86_hint, Some(X86OpHint::Mulx))
            || matches!(
                op.kind,
                OpKind::Bzhi { .. } | OpKind::Pdep { .. } | OpKind::Pext { .. }
            );
        needs_bmi1 |= matches!(
            op.kind,
            OpKind::Bextr { .. }
                | OpKind::X86Bls { .. }
                | OpKind::Ctz { .. }
                | OpKind::X86Count {
                    kind: X86CountKind::Tzcnt,
                    ..
                }
        );
        needs_lzcnt |= matches!(
            op.kind,
            OpKind::Clz { .. }
                | OpKind::X86Count {
                    kind: X86CountKind::Lzcnt,
                    ..
                }
        );
        needs_popcnt |= matches!(
            op.kind,
            OpKind::Popcnt { .. }
                | OpKind::X86Count {
                    kind: X86CountKind::Popcnt,
                    ..
                }
        );
        needs_adx |= matches!(op.kind, OpKind::X86Adx { .. });
    }
    (needs_bmi2, needs_bmi1, needs_lzcnt, needs_popcnt, needs_adx)
}

/// Return whether `op` is one of the register-only x86 vector operations whose
/// interpreter semantics and native EVEX encoding are both regression-covered.
/// Every operand must be architectural: virtual vector values still require a
/// separate vector allocator/spill discipline and therefore remain ineligible.
fn x86_packed_shift_imm_shape_valid(op: &crate::smir::ir::ops::OpKind) -> bool {
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::{ArchReg, ShiftOp, VReg, VecElementType, VecWidth, X86Reg};
    let OpKind::X86PackedShiftImm {
        dst,
        src,
        width,
        elem,
        shift,
        byte_lane,
        ..
    } = op
    else {
        return false;
    };
    let valid_vector = |reg: &VReg| {
        matches!(
            (reg, width),
            (
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(0..=31))),
                VecWidth::V128
            ) | (
                VReg::Arch(ArchReg::X86(X86Reg::Ymm(0..=31))),
                VecWidth::V256
            ) | (
                VReg::Arch(ArchReg::X86(X86Reg::Zmm(0..=31))),
                VecWidth::V512
            )
        )
    };
    let valid_operation = if *byte_lane {
        *elem == VecElementType::I8 && matches!(shift, ShiftOp::Lsl | ShiftOp::Lsr)
    } else {
        matches!(
            elem,
            VecElementType::I16 | VecElementType::I32 | VecElementType::I64
        ) && matches!(shift, ShiftOp::Lsl | ShiftOp::Lsr | ShiftOp::Asr)
    };
    valid_vector(dst) && valid_vector(src) && valid_operation
}

fn x86_packed_shift_imm_feature_requirements(
    op: &crate::smir::ir::ops::OpKind,
) -> (bool, bool, bool) {
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::{ArchReg, ShiftOp, VReg, VecElementType, VecWidth, X86Reg};
    let OpKind::X86PackedShiftImm {
        dst,
        src,
        width,
        elem,
        shift,
        ..
    } = op
    else {
        return (false, false, false);
    };
    let high = |reg: &VReg| {
        matches!(
            reg,
            VReg::Arch(ArchReg::X86(
                X86Reg::Xmm(16..=31) | X86Reg::Ymm(16..=31) | X86Reg::Zmm(16..=31)
            ))
        )
    };
    let evex = *width == VecWidth::V512
        || high(dst)
        || high(src)
        || (*elem == VecElementType::I64 && *shift == ShiftOp::Asr);
    if evex {
        (false, false, *width != VecWidth::V512)
    } else {
        (*width == VecWidth::V128, *width == VecWidth::V256, false)
    }
}

fn x86_packed_shift_shape_valid(op: &crate::smir::ir::ops::OpKind) -> bool {
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::{ArchReg, ShiftOp, VReg, VecElementType, VecWidth, X86Reg};
    let OpKind::X86PackedShift {
        dst,
        src,
        count,
        width,
        elem,
        shift,
    } = op
    else {
        return false;
    };
    let vector = |reg: &VReg| {
        matches!(
            (reg, width),
            (
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(0..=31))),
                VecWidth::V128
            ) | (
                VReg::Arch(ArchReg::X86(X86Reg::Ymm(0..=31))),
                VecWidth::V256
            ) | (
                VReg::Arch(ArchReg::X86(X86Reg::Zmm(0..=31))),
                VecWidth::V512
            )
        )
    };
    vector(dst)
        && vector(src)
        && matches!(count, VReg::Arch(ArchReg::X86(X86Reg::Xmm(0..=31))))
        && matches!(
            elem,
            VecElementType::I16 | VecElementType::I32 | VecElementType::I64
        )
        && matches!(shift, ShiftOp::Lsl | ShiftOp::Lsr | ShiftOp::Asr)
}

fn x86_packed_shift_feature_requirements(op: &crate::smir::ir::ops::OpKind) -> (bool, bool, bool) {
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::{ArchReg, ShiftOp, VReg, VecElementType, VecWidth, X86Reg};
    let OpKind::X86PackedShift {
        dst,
        src,
        count,
        width,
        elem,
        shift,
    } = op
    else {
        return (false, false, false);
    };
    let high = |reg: &VReg| {
        matches!(
            reg,
            VReg::Arch(ArchReg::X86(
                X86Reg::Xmm(16..=31) | X86Reg::Ymm(16..=31) | X86Reg::Zmm(16..=31)
            ))
        )
    };
    let evex = *width == VecWidth::V512
        || high(dst)
        || high(src)
        || high(count)
        || (*elem == VecElementType::I64 && *shift == ShiftOp::Asr);
    if evex {
        (false, false, *width != VecWidth::V512)
    } else {
        (*width == VecWidth::V128, *width == VecWidth::V256, false)
    }
}

fn x86_vector_width_from_lanes(
    elem: crate::smir::ir::types::VecElementType,
    lanes: u8,
) -> Option<crate::smir::ir::types::VecWidth> {
    use crate::smir::ir::types::VecWidth;

    [VecWidth::V128, VecWidth::V256, VecWidth::V512]
        .into_iter()
        .find(|width| width.lanes(elem) as u8 == lanes)
}

/// Exact PAVGB/PAVGW and VPAVGB/VPAVGW semantic shape. The instructions use
/// unsigned lanes and round upward: `(a + b + 1) >> 1`. Restricting native
/// admission to this complete shape prevents the width-general Hexagon VLane
/// family from inheriting an unrelated x86 encoding.
fn x86_vector_integer_average_shape_valid(op: &crate::smir::ir::ops::OpKind) -> bool {
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::{ArchReg, VLaneOp, VReg, VecElementType, VecWidth, X86Reg};

    let OpKind::VLane {
        dst,
        src1,
        src2,
        elem,
        lanes,
        op: VLaneOp::AvgRnd,
        signed: false,
        set_ovf: false,
    } = op
    else {
        return false;
    };
    if !matches!(elem, VecElementType::I8 | VecElementType::I16) {
        return false;
    }
    let Some(width) = x86_vector_width_from_lanes(*elem, *lanes) else {
        return false;
    };
    let vector_matches_width = |reg: &VReg| {
        matches!(
            (reg, width),
            (
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(0..=31))),
                VecWidth::V128
            ) | (
                VReg::Arch(ArchReg::X86(X86Reg::Ymm(0..=31))),
                VecWidth::V256
            ) | (
                VReg::Arch(ArchReg::X86(X86Reg::Zmm(0..=31))),
                VecWidth::V512
            )
        )
    };
    [dst, src1, src2].into_iter().all(vector_matches_width)
}

/// Exact PSIGNB/PSIGNW/PSIGND and VPSIGNB/VPSIGNW/VPSIGND semantic shape.
/// This operation uses signed control lanes but wrapping negation of the data
/// lane, and the ISA exposes only legacy 128-bit and VEX 128/256-bit forms.
fn x86_vector_integer_sign_shape_valid(op: &crate::smir::ir::ops::OpKind) -> bool {
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::{ArchReg, VLaneOp, VReg, VecElementType, VecWidth, X86Reg};

    let OpKind::VLane {
        dst,
        src1,
        src2,
        elem,
        lanes,
        op: VLaneOp::Sign,
        signed: true,
        set_ovf: false,
    } = op
    else {
        return false;
    };
    if !matches!(
        elem,
        VecElementType::I8 | VecElementType::I16 | VecElementType::I32
    ) {
        return false;
    }
    let Some(width @ (VecWidth::V128 | VecWidth::V256)) =
        x86_vector_width_from_lanes(*elem, *lanes)
    else {
        return false;
    };
    let vector_matches_width = |reg: &VReg| {
        matches!(
            (reg, width),
            (
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(0..=31))),
                VecWidth::V128
            ) | (
                VReg::Arch(ArchReg::X86(X86Reg::Ymm(0..=31))),
                VecWidth::V256
            )
        )
    };
    [dst, src1, src2].into_iter().all(vector_matches_width)
}

/// Exact packed-integer PMIN*/PMAX* and VPMIN*/VPMAX* semantic shape. The
/// x86 encodings cover signed and unsigned byte, word, and dword lanes; qword
/// lanes exist only in EVEX. Architectural register-width matching prevents
/// unrelated Hexagon VLane operations from entering the x86 native path.
fn x86_vector_integer_minmax_shape_valid(op: &crate::smir::ir::ops::OpKind) -> bool {
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::{ArchReg, VLaneOp, VReg, VecElementType, VecWidth, X86Reg};

    let OpKind::VLane {
        dst,
        src1,
        src2,
        elem,
        lanes,
        op: VLaneOp::Min | VLaneOp::Max,
        set_ovf: false,
        ..
    } = op
    else {
        return false;
    };
    if !matches!(
        elem,
        VecElementType::I8 | VecElementType::I16 | VecElementType::I32 | VecElementType::I64
    ) {
        return false;
    }
    let Some(width) = x86_vector_width_from_lanes(*elem, *lanes) else {
        return false;
    };
    let vector_matches_width = |reg: &VReg| {
        matches!(
            (reg, width),
            (
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(0..=31))),
                VecWidth::V128
            ) | (
                VReg::Arch(ArchReg::X86(X86Reg::Ymm(0..=31))),
                VecWidth::V256
            ) | (
                VReg::Arch(ArchReg::X86(X86Reg::Zmm(0..=31))),
                VecWidth::V512
            )
        )
    };
    [dst, src1, src2].into_iter().all(vector_matches_width)
}

fn x86_vector_integer_minmax_encoding(
    op: &crate::smir::ir::ops::OpKind,
) -> Option<(crate::smir::ir::ops::X86VecMap, u8)> {
    use crate::smir::ir::ops::{OpKind, X86VecMap};
    use crate::smir::ir::types::{VLaneOp, VecElementType};

    let OpKind::VLane {
        elem, op, signed, ..
    } = op
    else {
        return None;
    };
    match (*elem, *op, *signed) {
        (VecElementType::I8, VLaneOp::Min, false) => Some((X86VecMap::Map0F, 0xDA)),
        (VecElementType::I8, VLaneOp::Max, false) => Some((X86VecMap::Map0F, 0xDE)),
        (VecElementType::I16, VLaneOp::Min, true) => Some((X86VecMap::Map0F, 0xEA)),
        (VecElementType::I16, VLaneOp::Max, true) => Some((X86VecMap::Map0F, 0xEE)),
        (VecElementType::I8, VLaneOp::Min, true) => Some((X86VecMap::Map0F38, 0x38)),
        (VecElementType::I32 | VecElementType::I64, VLaneOp::Min, true) => {
            Some((X86VecMap::Map0F38, 0x39))
        }
        (VecElementType::I16, VLaneOp::Min, false) => Some((X86VecMap::Map0F38, 0x3A)),
        (VecElementType::I32 | VecElementType::I64, VLaneOp::Min, false) => {
            Some((X86VecMap::Map0F38, 0x3B))
        }
        (VecElementType::I8, VLaneOp::Max, true) => Some((X86VecMap::Map0F38, 0x3C)),
        (VecElementType::I32 | VecElementType::I64, VLaneOp::Max, true) => {
            Some((X86VecMap::Map0F38, 0x3D))
        }
        (VecElementType::I16, VLaneOp::Max, false) => Some((X86VecMap::Map0F38, 0x3E)),
        (VecElementType::I32 | VecElementType::I64, VLaneOp::Max, false) => {
            Some((X86VecMap::Map0F38, 0x3F))
        }
        _ => None,
    }
}

/// Exact PSADBW/VPSADBW semantic shape. Every consecutive group of eight
/// unsigned-byte absolute differences produces one zero-extended qword result.
/// Restricting admission to architectural registers of the declared width
/// excludes the width-general IR operation used by non-x86 source ISAs.
fn x86_vector_sad_bytes_shape_valid(op: &crate::smir::ir::ops::OpKind) -> bool {
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::{ArchReg, VReg, VecWidth, X86Reg};

    let OpKind::VSadBytes {
        dst,
        src1,
        src2,
        width,
    } = op
    else {
        return false;
    };
    let vector_matches_width = |reg: &VReg| {
        matches!(
            (reg, width),
            (
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(0..=31))),
                VecWidth::V128
            ) | (
                VReg::Arch(ArchReg::X86(X86Reg::Ymm(0..=31))),
                VecWidth::V256
            ) | (
                VReg::Arch(ArchReg::X86(X86Reg::Zmm(0..=31))),
                VecWidth::V512
            )
        )
    };
    [dst, src1, src2].into_iter().all(vector_matches_width)
}

/// Exact register-only PHMINPOSUW/VPHMINPOSUW semantic shape. Both encodings
/// address only the architectural low 16 XMM registers; memory sources remain
/// expanded so alignment and fault ordering stay explicit in SMIR.
fn x86_phminposuw_shape_valid(op: &crate::smir::ir::ops::OpKind) -> bool {
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::{ArchReg, VReg, X86Reg};

    let OpKind::X86Phminposuw { dst, src } = op else {
        return false;
    };
    let low_xmm = |reg: &VReg| matches!(reg, VReg::Arch(ArchReg::X86(X86Reg::Xmm(0..=15))));
    low_xmm(dst) && low_xmm(src)
}

/// Exact register-only shapes shared by MOVMSKPS, MOVMSKPD, and PMOVMSKB.
/// RSP/RBP destinations are excluded because the x86 native trampoline owns
/// those host registers. High vector registers require EVEX, which this
/// instruction family does not define.
fn x86_mov_mask_shape_valid(op: &crate::smir::ir::ops::OpKind) -> bool {
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::{ArchReg, OpWidth, VReg, VecElementType, X86Reg};

    let OpKind::X86MovMask {
        dst,
        src,
        elem,
        lanes,
        dst_width,
    } = op
    else {
        return false;
    };
    let gpr = matches!(
        dst,
        VReg::Arch(ArchReg::X86(
            X86Reg::Rax
                | X86Reg::Rcx
                | X86Reg::Rdx
                | X86Reg::Rbx
                | X86Reg::Rsi
                | X86Reg::Rdi
                | X86Reg::R8
                | X86Reg::R9
                | X86Reg::R10
                | X86Reg::R11
                | X86Reg::R12
                | X86Reg::R13
                | X86Reg::R14
                | X86Reg::R15
        ))
    );
    let source = match (elem, lanes, src) {
        (VecElementType::I8, 16, VReg::Arch(ArchReg::X86(X86Reg::Xmm(0..=15))))
        | (VecElementType::F32, 4, VReg::Arch(ArchReg::X86(X86Reg::Xmm(0..=15))))
        | (VecElementType::F64, 2, VReg::Arch(ArchReg::X86(X86Reg::Xmm(0..=15))))
        | (VecElementType::I8, 32, VReg::Arch(ArchReg::X86(X86Reg::Ymm(0..=15))))
        | (VecElementType::F32, 8, VReg::Arch(ArchReg::X86(X86Reg::Ymm(0..=15))))
        | (VecElementType::F64, 4, VReg::Arch(ArchReg::X86(X86Reg::Ymm(0..=15)))) => true,
        _ => false,
    };
    gpr && source && matches!(dst_width, OpWidth::W32 | OpWidth::W64)
}

/// Exact register-only MOVD/MOVQ shape. The native identity bridge can expose
/// architectural GPRs other than RSP/RBP and all 32 XMM registers; encoding
/// metadata later restricts legacy/VEX forms to XMM0..15. Memory operands stay
/// expanded into explicit scalar loads/stores so their fault effects remain
/// visible to the JIT memory gate.
fn x86_movd_q_shape_valid(op: &crate::smir::ir::ops::OpKind) -> bool {
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::{ArchReg, OpWidth, VReg, X86Reg};

    let OpKind::X86MovdQ {
        dst,
        src,
        width,
        zero_upper,
    } = op
    else {
        return false;
    };
    let gpr = |reg: &VReg| matches!(reg, VReg::Arch(ArchReg::X86(x86)) if x86.gpr_index().is_some_and(|index| index <= 15 && !matches!(index, 4 | 5)));
    let xmm = |reg: &VReg| matches!(reg, VReg::Arch(ArchReg::X86(X86Reg::Xmm(0..=31))));
    let vector_dst = xmm(dst) && gpr(src);
    let gpr_dst = gpr(dst) && xmm(src);
    matches!(width, OpWidth::W32 | OpWidth::W64)
        && (vector_dst || gpr_dst)
        && (!gpr_dst || !*zero_upper)
}

/// Exact MPSADBW/VMPSADBW semantic shape for the legacy and VEX encodings.
/// Each 128-bit lane produces eight unsigned-word sums from a four-byte
/// stationary block and eight sliding four-byte windows. AVX10.2 masking and
/// 512-bit replication are deliberately outside this classic shape.
fn x86_vector_mpsadbw_shape_valid(op: &crate::smir::ir::ops::OpKind) -> bool {
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::{ArchReg, VReg, VecWidth, X86Reg};

    let OpKind::VMpsadbw {
        dst,
        src1,
        src2,
        mask,
        width,
        zeroing,
        ..
    } = op
    else {
        return false;
    };
    let vector_matches_width = |reg: &VReg| {
        matches!(
            (reg, width),
            (
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(0..=31))),
                VecWidth::V128
            ) | (
                VReg::Arch(ArchReg::X86(X86Reg::Ymm(0..=31))),
                VecWidth::V256
            )
        )
    };
    mask.is_none() && !zeroing && [dst, src1, src2].into_iter().all(vector_matches_width)
}

/// Exact non-accumulating PMADDUBSW/VPMADDUBSW semantic shape. `VReg::Imm(0)`
/// denotes the instruction's implicit all-zero word accumulator; unlike VNNI,
/// the architectural destination is not an input except in the legacy
/// destructive encoding where the lifter also names it as `src1`.
fn x86_vector_integer_maddubs_shape_valid(op: &crate::smir::ir::ops::OpKind) -> bool {
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::{ArchReg, VReg, VecElementType, VecWidth, X86Reg};

    let OpKind::VDotProduct {
        dst,
        acc,
        src1,
        src2,
        mask,
        src_elem,
        acc_elem,
        width,
        src1_unsigned,
        saturate,
        zeroing,
    } = op
    else {
        return false;
    };
    let vector_matches_width = |reg: &VReg| {
        matches!(
            (reg, width),
            (
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(0..=31))),
                VecWidth::V128
            ) | (
                VReg::Arch(ArchReg::X86(X86Reg::Ymm(0..=31))),
                VecWidth::V256
            ) | (
                VReg::Arch(ArchReg::X86(X86Reg::Zmm(0..=31))),
                VecWidth::V512
            )
        )
    };

    *acc == VReg::Imm(0)
        && mask.is_none()
        && *src_elem == VecElementType::I8
        && *acc_elem == VecElementType::I16
        && *src1_unsigned
        && *saturate
        && !*zeroing
        && [dst, src1, src2].into_iter().all(vector_matches_width)
}

/// Exact non-accumulating PMADDWD/VPMADDWD semantic shape. The signed word
/// products are paired into wrapping signed dword sums; `VReg::Imm(0)` is the
/// canonical all-zero accumulator used to distinguish this from VNNI.
fn x86_vector_integer_maddwd_shape_valid(op: &crate::smir::ir::ops::OpKind) -> bool {
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::{ArchReg, VReg, VecElementType, VecWidth, X86Reg};

    let OpKind::VDotProduct {
        dst,
        acc,
        src1,
        src2,
        mask,
        src_elem,
        acc_elem,
        width,
        src1_unsigned,
        saturate,
        zeroing,
    } = op
    else {
        return false;
    };
    let vector_matches_width = |reg: &VReg| {
        matches!(
            (reg, width),
            (
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(0..=31))),
                VecWidth::V128
            ) | (
                VReg::Arch(ArchReg::X86(X86Reg::Ymm(0..=31))),
                VecWidth::V256
            ) | (
                VReg::Arch(ArchReg::X86(X86Reg::Zmm(0..=31))),
                VecWidth::V512
            )
        )
    };

    *acc == VReg::Imm(0)
        && mask.is_none()
        && *src_elem == VecElementType::I16
        && *acc_elem == VecElementType::I32
        && !*src1_unsigned
        && !*saturate
        && !*zeroing
        && [dst, src1, src2].into_iter().all(vector_matches_width)
}

/// Exact PMULHW/PMULHUW/PMULHRSW semantic shapes. The first two retain bits
/// 31:16 of each signed or unsigned word product; PMULHRSW adds the
/// architectural 0x4000 rounding bias before an arithmetic right shift by 15.
fn x86_vector_integer_mul_shift_shape_valid(op: &crate::smir::ir::ops::OpKind) -> bool {
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::{ArchReg, VReg, VecElementType, VecWidth, X86Reg};

    let OpKind::VMulShiftSat {
        dst,
        src1,
        src2,
        src_elem,
        lanes,
        signed1,
        signed2,
        shift_left,
        round,
        sat_bits,
        out_shift,
    } = op
    else {
        return false;
    };
    let width = match lanes {
        8 => VecWidth::V128,
        16 => VecWidth::V256,
        32 => VecWidth::V512,
        _ => return false,
    };
    let vector_matches_width = |reg: &VReg| {
        matches!(
            (reg, width),
            (
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(0..=31))),
                VecWidth::V128
            ) | (
                VReg::Arch(ArchReg::X86(X86Reg::Ymm(0..=31))),
                VecWidth::V256
            ) | (
                VReg::Arch(ArchReg::X86(X86Reg::Zmm(0..=31))),
                VecWidth::V512
            )
        )
    };

    *src_elem == VecElementType::I16
        && *shift_left == 0
        && *sat_bits == 0
        && ((*signed1 && *signed2 && *round && *out_shift == 15)
            || (*signed1 == *signed2 && !*round && *out_shift == 16))
        && [dst, src1, src2].into_iter().all(vector_matches_width)
}

pub fn is_x86_native_vector_op(op: &crate::smir::ir::ops::OpKind) -> bool {
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::{ArchReg, VReg, VecWidth, X86Reg};

    if !matches!(
        op,
        OpKind::VMov { .. }
            | OpKind::VAdd { .. }
            | OpKind::VSub { .. }
            | OpKind::VAddSubSat { .. }
            | OpKind::VMul { .. }
            | OpKind::VUnary { .. }
            | OpKind::VCmp { .. }
            | OpKind::VInterleave { .. }
            | OpKind::VPackSat { .. }
            | OpKind::VByteShuffle { .. }
            | OpKind::VHorizontalBin { .. }
            | OpKind::VMulShiftSat { .. }
            | OpKind::VLane { .. }
            | OpKind::VSadBytes { .. }
            | OpKind::X86Phminposuw { .. }
            | OpKind::X86MovMask { .. }
            | OpKind::X86MovdQ { .. }
            | OpKind::VMpsadbw { .. }
            | OpKind::VAnd { .. }
            | OpKind::VAndNot { .. }
            | OpKind::VOr { .. }
            | OpKind::VXor { .. }
            | OpKind::VPopcnt { .. }
            | OpKind::VShuffleBitQM { .. }
            | OpKind::VConflict { .. }
            | OpKind::VLeadingZeros { .. }
            | OpKind::X86PermuteBytesWords { .. }
            | OpKind::VCompress { .. }
            | OpKind::VExpand { .. }
            | OpKind::X86NarrowInt { .. }
            | OpKind::X86Aes { .. }
            | OpKind::X86Sha512Msg1 { .. }
            | OpKind::X86Sha512Msg2 { .. }
            | OpKind::X86Sha512Rounds2 { .. }
            | OpKind::X86Sm3Msg1 { .. }
            | OpKind::X86Sm3Msg2 { .. }
            | OpKind::X86Sm3Rounds2 { .. }
            | OpKind::X86Sm4 { .. }
            | OpKind::X86PackedShiftImm { .. }
            | OpKind::X86PackedShift { .. }
            | OpKind::VDotProduct { .. }
            | OpKind::VDotProductBF16 { .. }
            | OpKind::VCvtFP32ToBF16 { .. }
            | OpKind::VFP16Arith { .. }
            | OpKind::VMultiplyAdd52 { .. }
            | OpKind::X86PackedShiftVariable { .. }
            | OpKind::X86PackedRotate { .. }
            | OpKind::X86TernaryLogic { .. }
            | OpKind::X86PackedFunnelShift { .. }
            | OpKind::X86MultiShiftQB { .. }
    ) {
        return false;
    }

    if matches!(op, OpKind::VLane { .. })
        && !x86_vector_integer_average_shape_valid(op)
        && !x86_vector_integer_sign_shape_valid(op)
        && !x86_vector_integer_minmax_shape_valid(op)
    {
        return false;
    }

    if matches!(op, OpKind::VSadBytes { .. }) && !x86_vector_sad_bytes_shape_valid(op) {
        return false;
    }

    if matches!(op, OpKind::X86Phminposuw { .. }) && !x86_phminposuw_shape_valid(op) {
        return false;
    }

    if matches!(op, OpKind::X86MovMask { .. }) && !x86_mov_mask_shape_valid(op) {
        return false;
    }

    if matches!(op, OpKind::X86MovdQ { .. }) && !x86_movd_q_shape_valid(op) {
        return false;
    }

    if matches!(op, OpKind::VMpsadbw { .. }) && !x86_vector_mpsadbw_shape_valid(op) {
        return false;
    }

    if matches!(op, OpKind::VMulShiftSat { .. }) && !x86_vector_integer_mul_shift_shape_valid(op) {
        return false;
    }

    if let OpKind::VMov { dst, src, width } = op {
        let vector_matches_width = |reg: &VReg| {
            matches!(
                (reg, width),
                (
                    VReg::Arch(ArchReg::X86(X86Reg::Xmm(0..=31))),
                    VecWidth::V128
                ) | (
                    VReg::Arch(ArchReg::X86(X86Reg::Ymm(0..=31))),
                    VecWidth::V256
                ) | (
                    VReg::Arch(ArchReg::X86(X86Reg::Zmm(0..=31))),
                    VecWidth::V512
                )
            )
        };
        if !vector_matches_width(dst) || !vector_matches_width(src) {
            return false;
        }
    }

    if let OpKind::VAnd {
        dst,
        src1,
        src2,
        width,
    }
    | OpKind::VAndNot {
        dst,
        src1,
        src2,
        width,
    }
    | OpKind::VOr {
        dst,
        src1,
        src2,
        width,
    }
    | OpKind::VXor {
        dst,
        src1,
        src2,
        width,
    } = op
    {
        let vector_matches_width = |reg: &VReg| {
            matches!(
                (reg, width),
                (
                    VReg::Arch(ArchReg::X86(X86Reg::Xmm(0..=31))),
                    VecWidth::V128
                ) | (
                    VReg::Arch(ArchReg::X86(X86Reg::Ymm(0..=31))),
                    VecWidth::V256
                ) | (
                    VReg::Arch(ArchReg::X86(X86Reg::Zmm(0..=31))),
                    VecWidth::V512
                )
            )
        };
        if ![dst, src1, src2].into_iter().all(vector_matches_width) {
            return false;
        }
    }

    if let OpKind::VAdd {
        dst,
        src1,
        src2,
        elem,
        lanes,
    }
    | OpKind::VSub {
        dst,
        src1,
        src2,
        elem,
        lanes,
    }
    | OpKind::VAddSubSat {
        dst,
        src1,
        src2,
        elem,
        lanes,
        ..
    }
    | OpKind::VMul {
        dst,
        src1,
        src2,
        elem,
        lanes,
    } = op
    {
        let elem_valid = if matches!(op, OpKind::VAddSubSat { .. }) {
            matches!(
                elem,
                crate::smir::ir::types::VecElementType::I8
                    | crate::smir::ir::types::VecElementType::I16
            )
        } else if matches!(op, OpKind::VMul { .. }) {
            matches!(
                elem,
                crate::smir::ir::types::VecElementType::I16
                    | crate::smir::ir::types::VecElementType::I32
                    | crate::smir::ir::types::VecElementType::I64
            )
        } else {
            matches!(
                elem,
                crate::smir::ir::types::VecElementType::I8
                    | crate::smir::ir::types::VecElementType::I16
                    | crate::smir::ir::types::VecElementType::I32
                    | crate::smir::ir::types::VecElementType::I64
            )
        };
        if !elem_valid {
            return false;
        }
        let Some(width) = x86_vector_width_from_lanes(*elem, *lanes) else {
            return false;
        };
        let vector_matches_width = |reg: &VReg| {
            matches!(
                (reg, width),
                (
                    VReg::Arch(ArchReg::X86(X86Reg::Xmm(0..=31))),
                    VecWidth::V128
                ) | (
                    VReg::Arch(ArchReg::X86(X86Reg::Ymm(0..=31))),
                    VecWidth::V256
                ) | (
                    VReg::Arch(ArchReg::X86(X86Reg::Zmm(0..=31))),
                    VecWidth::V512
                )
            )
        };
        if ![dst, src1, src2].into_iter().all(vector_matches_width) {
            return false;
        }
    }

    if let OpKind::VCmp {
        dst,
        src1,
        src2,
        cond,
        elem,
        lanes,
    } = op
    {
        if !matches!(
            (elem, cond),
            (
                crate::smir::ir::types::VecElementType::I8
                    | crate::smir::ir::types::VecElementType::I16
                    | crate::smir::ir::types::VecElementType::I32
                    | crate::smir::ir::types::VecElementType::I64,
                crate::smir::ir::types::VecCmpCond::Eq | crate::smir::ir::types::VecCmpCond::Gt
            )
        ) {
            return false;
        }
        let Some(width @ (VecWidth::V128 | VecWidth::V256)) =
            x86_vector_width_from_lanes(*elem, *lanes)
        else {
            return false;
        };
        let vector_matches_width = |reg: &VReg| {
            matches!(
                (reg, width),
                (
                    VReg::Arch(ArchReg::X86(X86Reg::Xmm(0..=31))),
                    VecWidth::V128
                ) | (
                    VReg::Arch(ArchReg::X86(X86Reg::Ymm(0..=31))),
                    VecWidth::V256
                )
            )
        };
        if ![dst, src1, src2].into_iter().all(vector_matches_width) {
            return false;
        }
    }

    if let OpKind::VInterleave {
        dst,
        src1,
        src2,
        elem,
        lanes,
        block_lanes,
        ..
    } = op
    {
        if !matches!(
            elem,
            crate::smir::ir::types::VecElementType::I8
                | crate::smir::ir::types::VecElementType::I16
                | crate::smir::ir::types::VecElementType::I32
                | crate::smir::ir::types::VecElementType::I64
        ) || *block_lanes != (16 / elem.bytes()) as u8
        {
            return false;
        }
        let Some(width) = x86_vector_width_from_lanes(*elem, *lanes) else {
            return false;
        };
        let vector_matches_width = |reg: &VReg| {
            matches!(
                (reg, width),
                (
                    VReg::Arch(ArchReg::X86(X86Reg::Xmm(0..=31))),
                    VecWidth::V128
                ) | (
                    VReg::Arch(ArchReg::X86(X86Reg::Ymm(0..=31))),
                    VecWidth::V256
                ) | (
                    VReg::Arch(ArchReg::X86(X86Reg::Zmm(0..=31))),
                    VecWidth::V512
                )
            )
        };
        if ![dst, src1, src2].into_iter().all(vector_matches_width) {
            return false;
        }
    }

    if let OpKind::VPackSat {
        dst,
        src1,
        src2,
        src_elem,
        src_lanes,
        block_lanes,
        ..
    } = op
    {
        if !matches!(
            src_elem,
            crate::smir::ir::types::VecElementType::I16
                | crate::smir::ir::types::VecElementType::I32
        ) || *block_lanes != (16 / src_elem.bytes()) as u8
        {
            return false;
        }
        let Some(width) = x86_vector_width_from_lanes(*src_elem, *src_lanes) else {
            return false;
        };
        let vector_matches_width = |reg: &VReg| {
            matches!(
                (reg, width),
                (
                    VReg::Arch(ArchReg::X86(X86Reg::Xmm(0..=31))),
                    VecWidth::V128
                ) | (
                    VReg::Arch(ArchReg::X86(X86Reg::Ymm(0..=31))),
                    VecWidth::V256
                ) | (
                    VReg::Arch(ArchReg::X86(X86Reg::Zmm(0..=31))),
                    VecWidth::V512
                )
            )
        };
        if ![dst, src1, src2].into_iter().all(vector_matches_width) {
            return false;
        }
    }

    if let OpKind::VByteShuffle {
        dst,
        src,
        control,
        lanes,
        block_lanes,
    } = op
    {
        if *block_lanes != 16 {
            return false;
        }
        let Some(width) =
            x86_vector_width_from_lanes(crate::smir::ir::types::VecElementType::I8, *lanes)
        else {
            return false;
        };
        let vector_matches_width = |reg: &VReg| {
            matches!(
                (reg, width),
                (
                    VReg::Arch(ArchReg::X86(X86Reg::Xmm(0..=31))),
                    VecWidth::V128
                ) | (
                    VReg::Arch(ArchReg::X86(X86Reg::Ymm(0..=31))),
                    VecWidth::V256
                ) | (
                    VReg::Arch(ArchReg::X86(X86Reg::Zmm(0..=31))),
                    VecWidth::V512
                )
            )
        };
        if ![dst, src, control].into_iter().all(vector_matches_width) {
            return false;
        }
    }

    if let OpKind::VHorizontalBin {
        dst,
        src1,
        src2,
        elem,
        lanes,
        block_lanes,
        saturating,
        ..
    } = op
    {
        if !matches!(
            elem,
            crate::smir::ir::types::VecElementType::I16
                | crate::smir::ir::types::VecElementType::I32
        ) || *block_lanes != (16 / elem.bytes()) as u8
            || (*saturating && *elem != crate::smir::ir::types::VecElementType::I16)
        {
            return false;
        }
        let Some(width) = x86_vector_width_from_lanes(*elem, *lanes) else {
            return false;
        };
        if width == VecWidth::V512 {
            return false;
        }
        let vector_matches_width = |reg: &VReg| {
            matches!(
                (reg, width),
                (
                    VReg::Arch(ArchReg::X86(X86Reg::Xmm(0..=31))),
                    VecWidth::V128
                ) | (
                    VReg::Arch(ArchReg::X86(X86Reg::Ymm(0..=31))),
                    VecWidth::V256
                )
            )
        };
        if ![dst, src1, src2].into_iter().all(vector_matches_width) {
            return false;
        }
    }

    if let OpKind::VUnary {
        dst,
        src,
        elem,
        lanes,
        op: crate::smir::ir::types::VecUnaryOp::Abs,
    } = op
    {
        if !matches!(
            elem,
            crate::smir::ir::types::VecElementType::I8
                | crate::smir::ir::types::VecElementType::I16
                | crate::smir::ir::types::VecElementType::I32
                | crate::smir::ir::types::VecElementType::I64
        ) {
            return false;
        }
        let Some(width) = x86_vector_width_from_lanes(*elem, *lanes) else {
            return false;
        };
        let vector_matches_width = |reg: &VReg| {
            matches!(
                (reg, width),
                (
                    VReg::Arch(ArchReg::X86(X86Reg::Xmm(0..=31))),
                    VecWidth::V128
                ) | (
                    VReg::Arch(ArchReg::X86(X86Reg::Ymm(0..=31))),
                    VecWidth::V256
                ) | (
                    VReg::Arch(ArchReg::X86(X86Reg::Zmm(0..=31))),
                    VecWidth::V512
                )
            )
        };
        if !vector_matches_width(dst) || !vector_matches_width(src) {
            return false;
        }
    } else if matches!(op, OpKind::VUnary { .. }) {
        return false;
    }

    if matches!(op, OpKind::X86PackedShiftImm { .. }) && !x86_packed_shift_imm_shape_valid(op) {
        return false;
    }
    if matches!(op, OpKind::X86PackedShift { .. }) && !x86_packed_shift_shape_valid(op) {
        return false;
    }

    if let OpKind::VDotProduct {
        dst,
        acc,
        src1,
        src2,
        src_elem,
        acc_elem,
        src1_unsigned,
        mask,
        width,
        zeroing,
        saturate,
    } = op
    {
        let valid_vector = |reg: &VReg| {
            matches!(
                reg,
                VReg::Arch(ArchReg::X86(
                    X86Reg::Xmm(index) | X86Reg::Ymm(index) | X86Reg::Zmm(index)
                )) if *index <= 31
            )
        };
        let vnni = dst == acc
            && [dst, acc, src1, src2].into_iter().all(valid_vector)
            && *acc_elem == crate::smir::ir::types::VecElementType::I32
            && *width != crate::smir::ir::types::VecWidth::V64
            && !(*zeroing && mask.is_none())
            && !matches!(
                mask,
                Some(VReg::Arch(ArchReg::X86(X86Reg::K(0 | 8..=u8::MAX))))
            )
            && matches!(
                (src_elem, src1_unsigned),
                (crate::smir::ir::types::VecElementType::I8, true)
                    | (crate::smir::ir::types::VecElementType::I16, false)
            );
        let _ = saturate; // Both VNNI saturation modes are structurally valid.
        if !vnni
            && !x86_vector_integer_maddubs_shape_valid(op)
            && !x86_vector_integer_maddwd_shape_valid(op)
        {
            return false;
        }
    }

    if let OpKind::VConflict {
        dst,
        src,
        mask,
        elem,
        width,
        zeroing,
    } = op
    {
        let valid_vector = |reg: &VReg| {
            matches!(
                reg,
                VReg::Arch(ArchReg::X86(
                    X86Reg::Xmm(index) | X86Reg::Ymm(index) | X86Reg::Zmm(index)
                )) if *index <= 31
            )
        };
        if !valid_vector(dst)
            || !valid_vector(src)
            || !matches!(
                elem,
                crate::smir::ir::types::VecElementType::I32
                    | crate::smir::ir::types::VecElementType::I64
            )
            || *width == crate::smir::ir::types::VecWidth::V64
            || (*zeroing && mask.is_none())
            || matches!(
                mask,
                Some(VReg::Arch(ArchReg::X86(X86Reg::K(0 | 8..=u8::MAX))))
            )
        {
            return false;
        }
    }

    if let OpKind::VLeadingZeros {
        dst,
        src,
        mask,
        elem,
        width,
        zeroing,
    } = op
    {
        let valid_vector = |reg: &VReg| {
            matches!(
                (reg, width),
                (
                    VReg::Arch(ArchReg::X86(X86Reg::Xmm(0..=31))),
                    crate::smir::ir::types::VecWidth::V128
                ) | (
                    VReg::Arch(ArchReg::X86(X86Reg::Ymm(0..=31))),
                    crate::smir::ir::types::VecWidth::V256
                ) | (
                    VReg::Arch(ArchReg::X86(X86Reg::Zmm(0..=31))),
                    crate::smir::ir::types::VecWidth::V512
                )
            )
        };
        if !valid_vector(dst)
            || !valid_vector(src)
            || !matches!(
                elem,
                crate::smir::ir::types::VecElementType::I32
                    | crate::smir::ir::types::VecElementType::I64
            )
            || *width == crate::smir::ir::types::VecWidth::V64
            || (*zeroing && mask.is_none())
            || mask.is_some_and(|mask| !matches!(mask, VReg::Arch(ArchReg::X86(X86Reg::K(1..=7)))))
        {
            return false;
        }
    }

    if let OpKind::X86PermuteBytesWords {
        dst,
        table1,
        table2,
        indices,
        mask,
        elem,
        width,
        overwrite_table,
        zeroing,
    } = op
    {
        let valid_vector = |reg: &VReg| {
            matches!(
                (reg, width),
                (
                    VReg::Arch(ArchReg::X86(X86Reg::Xmm(0..=31))),
                    crate::smir::ir::types::VecWidth::V128
                ) | (
                    VReg::Arch(ArchReg::X86(X86Reg::Ymm(0..=31))),
                    crate::smir::ir::types::VecWidth::V256
                ) | (
                    VReg::Arch(ArchReg::X86(X86Reg::Zmm(0..=31))),
                    crate::smir::ir::types::VecWidth::V512
                )
            )
        };
        let valid_alias = match table2 {
            None => !overwrite_table,
            Some(_) if *overwrite_table => dst == table1,
            Some(_) => dst == indices,
        };
        if ![dst, table1, indices].into_iter().all(valid_vector)
            || table2.is_some_and(|reg| !valid_vector(&reg))
            || !matches!(
                elem,
                crate::smir::ir::types::VecElementType::I8
                    | crate::smir::ir::types::VecElementType::I16
            )
            || !valid_alias
            || (*zeroing && mask.is_none())
            || mask.is_some_and(|mask| !matches!(mask, VReg::Arch(ArchReg::X86(X86Reg::K(1..=7)))))
        {
            return false;
        }
    }

    if let OpKind::VCompress {
        dst,
        src,
        mask,
        elem,
        width,
        zeroing,
    }
    | OpKind::VExpand {
        dst,
        src,
        mask,
        elem,
        width,
        zeroing,
    } = op
    {
        let valid_vector = |reg: &VReg| {
            matches!(
                (reg, width),
                (
                    VReg::Arch(ArchReg::X86(X86Reg::Xmm(0..=31))),
                    crate::smir::ir::types::VecWidth::V128
                ) | (
                    VReg::Arch(ArchReg::X86(X86Reg::Ymm(0..=31))),
                    crate::smir::ir::types::VecWidth::V256
                ) | (
                    VReg::Arch(ArchReg::X86(X86Reg::Zmm(0..=31))),
                    crate::smir::ir::types::VecWidth::V512
                )
            )
        };
        if !valid_vector(dst)
            || !valid_vector(src)
            || !matches!(
                elem,
                crate::smir::ir::types::VecElementType::I8
                    | crate::smir::ir::types::VecElementType::I16
                    | crate::smir::ir::types::VecElementType::I32
                    | crate::smir::ir::types::VecElementType::I64
                    | crate::smir::ir::types::VecElementType::F32
                    | crate::smir::ir::types::VecElementType::F64
            )
            || (*zeroing && mask.is_none())
            || mask.is_some_and(|mask| !matches!(mask, VReg::Arch(ArchReg::X86(X86Reg::K(1..=7)))))
        {
            return false;
        }
    }

    if let OpKind::X86NarrowInt {
        dst,
        src,
        mask,
        src_elem,
        dst_elem,
        width,
        zeroing,
        ..
    } = op
    {
        let valid_source = matches!(
            (src, width),
            (
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(0..=31))),
                crate::smir::ir::types::VecWidth::V128
            ) | (
                VReg::Arch(ArchReg::X86(X86Reg::Ymm(0..=31))),
                crate::smir::ir::types::VecWidth::V256
            ) | (
                VReg::Arch(ArchReg::X86(X86Reg::Zmm(0..=31))),
                crate::smir::ir::types::VecWidth::V512
            )
        );
        let valid_pair = matches!(
            (src_elem, dst_elem),
            (
                crate::smir::ir::types::VecElementType::I16,
                crate::smir::ir::types::VecElementType::I8
            ) | (
                crate::smir::ir::types::VecElementType::I32,
                crate::smir::ir::types::VecElementType::I8
            ) | (
                crate::smir::ir::types::VecElementType::I64,
                crate::smir::ir::types::VecElementType::I8
            ) | (
                crate::smir::ir::types::VecElementType::I32,
                crate::smir::ir::types::VecElementType::I16
            ) | (
                crate::smir::ir::types::VecElementType::I64,
                crate::smir::ir::types::VecElementType::I16
            ) | (
                crate::smir::ir::types::VecElementType::I64,
                crate::smir::ir::types::VecElementType::I32
            )
        );
        let output_bytes = width.lanes(*src_elem) * dst_elem.bytes();
        let valid_destination = if output_bytes <= 16 {
            matches!(dst, VReg::Arch(ArchReg::X86(X86Reg::Xmm(0..=31))))
        } else {
            matches!(dst, VReg::Arch(ArchReg::X86(X86Reg::Ymm(0..=31))))
        };
        if !valid_source
            || !valid_pair
            || !valid_destination
            || (*zeroing && mask.is_none())
            || mask.is_some_and(|mask| !matches!(mask, VReg::Arch(ArchReg::X86(X86Reg::K(1..=7)))))
        {
            return false;
        }
    }

    if let OpKind::X86Aes {
        dst,
        src1,
        src2,
        width,
        op,
        imm,
    } = op
    {
        let valid_vector = |reg: &VReg| {
            matches!(
                (reg, width),
                (
                    VReg::Arch(ArchReg::X86(X86Reg::Xmm(0..=31))),
                    crate::smir::ir::types::VecWidth::V128
                ) | (
                    VReg::Arch(ArchReg::X86(X86Reg::Ymm(0..=31))),
                    crate::smir::ir::types::VecWidth::V256
                ) | (
                    VReg::Arch(ArchReg::X86(X86Reg::Zmm(0..=31))),
                    crate::smir::ir::types::VecWidth::V512
                )
            )
        };
        let valid = match op {
            crate::smir::ir::types::X86AesOp::Enc
            | crate::smir::ir::types::X86AesOp::EncLast
            | crate::smir::ir::types::X86AesOp::Dec
            | crate::smir::ir::types::X86AesOp::DecLast => {
                *imm == 0
                    && valid_vector(dst)
                    && valid_vector(src1)
                    && src2.is_some_and(|reg| valid_vector(&reg))
            }
            crate::smir::ir::types::X86AesOp::InvMixColumns
            | crate::smir::ir::types::X86AesOp::KeygenAssist => {
                *width == crate::smir::ir::types::VecWidth::V128
                    && matches!(dst, VReg::Arch(ArchReg::X86(X86Reg::Xmm(0..=15))))
                    && matches!(src1, VReg::Arch(ArchReg::X86(X86Reg::Xmm(0..=15))))
                    && src2.is_none()
                    && (*op == crate::smir::ir::types::X86AesOp::KeygenAssist || *imm == 0)
            }
        };
        if !valid {
            return false;
        }
    }

    let valid_sha512 = match op {
        OpKind::X86Sha512Msg1 { dst, src } => {
            matches!(dst, VReg::Arch(ArchReg::X86(X86Reg::Ymm(0..=15))))
                && matches!(src, VReg::Arch(ArchReg::X86(X86Reg::Xmm(0..=15))))
        }
        OpKind::X86Sha512Msg2 { dst, src } => {
            matches!(dst, VReg::Arch(ArchReg::X86(X86Reg::Ymm(0..=15))))
                && matches!(src, VReg::Arch(ArchReg::X86(X86Reg::Ymm(0..=15))))
        }
        OpKind::X86Sha512Rounds2 { dst, state, wk } => {
            matches!(dst, VReg::Arch(ArchReg::X86(X86Reg::Ymm(0..=15))))
                && matches!(state, VReg::Arch(ArchReg::X86(X86Reg::Ymm(0..=15))))
                && matches!(wk, VReg::Arch(ArchReg::X86(X86Reg::Xmm(0..=15))))
        }
        _ => true,
    };
    if !valid_sha512 {
        return false;
    }

    let valid_sm3 = match op {
        OpKind::X86Sm3Msg1 { dst, src1, src2 } | OpKind::X86Sm3Msg2 { dst, src1, src2 } => {
            [dst, src1, src2]
                .into_iter()
                .all(|reg| matches!(reg, VReg::Arch(ArchReg::X86(X86Reg::Xmm(0..=15)))))
        }
        OpKind::X86Sm3Rounds2 {
            dst, state, words, ..
        } => [dst, state, words]
            .into_iter()
            .all(|reg| matches!(reg, VReg::Arch(ArchReg::X86(X86Reg::Xmm(0..=15))))),
        _ => true,
    };
    if !valid_sm3 {
        return false;
    }

    if let OpKind::X86Sm4 {
        dst,
        src1,
        src2,
        width,
        ..
    } = op
    {
        let valid = |reg: &VReg| {
            matches!(
                (reg, width),
                (
                    VReg::Arch(ArchReg::X86(X86Reg::Xmm(0..=15))),
                    crate::smir::ir::types::VecWidth::V128
                ) | (
                    VReg::Arch(ArchReg::X86(X86Reg::Ymm(0..=15))),
                    crate::smir::ir::types::VecWidth::V256
                )
            )
        };
        if ![dst, src1, src2].into_iter().all(valid) {
            return false;
        }
    }

    if let OpKind::VMultiplyAdd52 {
        dst,
        acc,
        src1,
        src2,
        mask,
        width,
        zeroing,
        ..
    } = op
    {
        let valid_vector = |reg: &VReg| {
            matches!(
                reg,
                VReg::Arch(ArchReg::X86(
                    X86Reg::Xmm(index) | X86Reg::Ymm(index) | X86Reg::Zmm(index)
                )) if *index <= 31
            )
        };
        if dst != acc
            || ![dst, acc, src1, src2].into_iter().all(valid_vector)
            || *width == crate::smir::ir::types::VecWidth::V64
            || (*zeroing && mask.is_none())
            || matches!(
                mask,
                Some(VReg::Arch(ArchReg::X86(X86Reg::K(0 | 8..=u8::MAX))))
            )
        {
            return false;
        }
    }

    if let OpKind::VDotProductBF16 {
        dst,
        acc,
        src1,
        src2,
        mask,
        width,
        zeroing,
    } = op
    {
        let valid_vector = |reg: &VReg| {
            matches!(
                reg,
                VReg::Arch(ArchReg::X86(
                    X86Reg::Xmm(index) | X86Reg::Ymm(index) | X86Reg::Zmm(index)
                )) if *index <= 31
            )
        };
        if dst != acc
            || ![dst, acc, src1, src2].into_iter().all(valid_vector)
            || *width == crate::smir::ir::types::VecWidth::V64
            || (*zeroing && mask.is_none())
            || matches!(
                mask,
                Some(VReg::Arch(ArchReg::X86(X86Reg::K(0 | 8..=u8::MAX))))
            )
        {
            return false;
        }
    }

    if let OpKind::VCvtFP32ToBF16 {
        dst,
        src1,
        src2,
        mask,
        width,
        zeroing,
    } = op
    {
        let valid_vector = |reg: &VReg, expected: crate::smir::ir::types::VecWidth| {
            matches!(
                (reg, expected),
                (
                    VReg::Arch(ArchReg::X86(X86Reg::Xmm(0..=31))),
                    crate::smir::ir::types::VecWidth::V128
                ) | (
                    VReg::Arch(ArchReg::X86(X86Reg::Ymm(0..=31))),
                    crate::smir::ir::types::VecWidth::V256
                ) | (
                    VReg::Arch(ArchReg::X86(X86Reg::Zmm(0..=31))),
                    crate::smir::ir::types::VecWidth::V512
                )
            )
        };
        let output_width = match (width, src2.is_some()) {
            (crate::smir::ir::types::VecWidth::V128, _) => crate::smir::ir::types::VecWidth::V128,
            (crate::smir::ir::types::VecWidth::V256, false) => {
                crate::smir::ir::types::VecWidth::V128
            }
            (crate::smir::ir::types::VecWidth::V256, true) => {
                crate::smir::ir::types::VecWidth::V256
            }
            (crate::smir::ir::types::VecWidth::V512, false) => {
                crate::smir::ir::types::VecWidth::V256
            }
            (crate::smir::ir::types::VecWidth::V512, true) => {
                crate::smir::ir::types::VecWidth::V512
            }
            (crate::smir::ir::types::VecWidth::V64, _) => return false,
        };
        if !valid_vector(dst, output_width)
            || !valid_vector(src1, *width)
            || src2.is_some_and(|src2| !valid_vector(&src2, *width))
            || (*zeroing && mask.is_none())
            || mask.is_some_and(|mask| !matches!(mask, VReg::Arch(ArchReg::X86(X86Reg::K(1..=7)))))
        {
            return false;
        }
    }

    if let OpKind::VFP16Arith {
        dst,
        src1,
        src2,
        mask,
        op,
        width,
        zeroing,
    } = op
    {
        let valid_vector = |reg: &VReg| {
            matches!(
                (reg, width),
                (
                    VReg::Arch(ArchReg::X86(X86Reg::Xmm(0..=31))),
                    crate::smir::ir::types::VecWidth::V128
                ) | (
                    VReg::Arch(ArchReg::X86(X86Reg::Ymm(0..=31))),
                    crate::smir::ir::types::VecWidth::V256
                ) | (
                    VReg::Arch(ArchReg::X86(X86Reg::Zmm(0..=31))),
                    crate::smir::ir::types::VecWidth::V512
                )
            )
        };
        if ![dst, src1, src2].into_iter().all(valid_vector)
            || !matches!(
                op,
                crate::smir::ir::types::Avx10FP16Op::Add
                    | crate::smir::ir::types::Avx10FP16Op::Sub
                    | crate::smir::ir::types::Avx10FP16Op::Mul
                    | crate::smir::ir::types::Avx10FP16Op::Div
            )
            || *width == crate::smir::ir::types::VecWidth::V64
            || (*zeroing && mask.is_none())
            || mask.is_some_and(|mask| !matches!(mask, VReg::Arch(ArchReg::X86(X86Reg::K(1..=7)))))
        {
            return false;
        }
    }

    let non_accumulating_madd =
        x86_vector_integer_maddubs_shape_valid(op) || x86_vector_integer_maddwd_shape_valid(op);
    if matches!(op, OpKind::X86MovMask { .. } | OpKind::X86MovdQ { .. }) {
        // Shape validation above already checked the scalar GPR destination
        // or source and the vector counterpart; the generic vector-only
        // register filter below intentionally cannot describe mixed families.
        return true;
    }
    op.dests().into_iter().chain(op.source_vregs()).all(|reg| {
        matches!(
            reg,
            VReg::Arch(ArchReg::X86(
                X86Reg::Xmm(_) | X86Reg::Ymm(_) | X86Reg::Zmm(_) | X86Reg::K(_)
            ))
        ) || (non_accumulating_madd && reg == VReg::Imm(0))
    })
}

fn x86_vector_move_encoding_valid(
    prefix: crate::smir::ir::ops::X86SsePrefix,
    opcode: u8,
    evex: bool,
) -> bool {
    use crate::smir::ir::ops::X86SsePrefix;

    match opcode {
        0x10 | 0x11 | 0x28 | 0x29 => {
            matches!(prefix, X86SsePrefix::None | X86SsePrefix::OpSize)
        }
        0x6F | 0x7F => {
            matches!(prefix, X86SsePrefix::OpSize | X86SsePrefix::Rep)
                || evex && prefix == X86SsePrefix::Repne
        }
        _ => false,
    }
}

fn x86_vector_logic_encoding_valid(
    kind: &crate::smir::ir::ops::OpKind,
    prefix: crate::smir::ir::ops::X86SsePrefix,
    opcode: u8,
    evex: bool,
    w: bool,
) -> bool {
    use crate::smir::ir::ops::{OpKind, X86SsePrefix};

    let opcode_matches_kind = matches!(
        (kind, opcode),
        (OpKind::VAnd { .. }, 0x54 | 0xDB)
            | (OpKind::VAndNot { .. }, 0x55 | 0xDF)
            | (OpKind::VOr { .. }, 0x56 | 0xEB)
            | (OpKind::VXor { .. }, 0x57 | 0xEF)
    );
    if !opcode_matches_kind {
        return false;
    }

    match opcode {
        0x54..=0x57 => {
            matches!(prefix, X86SsePrefix::None | X86SsePrefix::OpSize)
                && (!evex
                    || matches!(
                        (prefix, w),
                        (X86SsePrefix::None, false) | (X86SsePrefix::OpSize, true)
                    ))
        }
        0xDB | 0xDF | 0xEB | 0xEF => prefix == X86SsePrefix::OpSize,
        _ => false,
    }
}

fn x86_vector_integer_arithmetic_encoding_valid(
    kind: &crate::smir::ir::ops::OpKind,
    prefix: crate::smir::ir::ops::X86SsePrefix,
    opcode: u8,
    evex: bool,
    w: bool,
) -> bool {
    use crate::smir::ir::ops::{OpKind, X86SsePrefix};
    use crate::smir::ir::types::VecElementType;

    let (elem, expected_opcode) = match kind {
        OpKind::VAdd { elem, .. } => (
            *elem,
            match elem {
                VecElementType::I8 => 0xFC,
                VecElementType::I16 => 0xFD,
                VecElementType::I32 => 0xFE,
                VecElementType::I64 => 0xD4,
                _ => return false,
            },
        ),
        OpKind::VSub { elem, .. } => (
            *elem,
            match elem {
                VecElementType::I8 => 0xF8,
                VecElementType::I16 => 0xF9,
                VecElementType::I32 => 0xFA,
                VecElementType::I64 => 0xFB,
                _ => return false,
            },
        ),
        OpKind::VAddSubSat {
            elem,
            subtract,
            signed,
            ..
        } => (
            *elem,
            match (*elem, *subtract, *signed) {
                (VecElementType::I8, false, true) => 0xEC,
                (VecElementType::I16, false, true) => 0xED,
                (VecElementType::I8, false, false) => 0xDC,
                (VecElementType::I16, false, false) => 0xDD,
                (VecElementType::I8, true, true) => 0xE8,
                (VecElementType::I16, true, true) => 0xE9,
                (VecElementType::I8, true, false) => 0xD8,
                (VecElementType::I16, true, false) => 0xD9,
                _ => return false,
            },
        ),
        OpKind::VMul { elem, .. } => (
            *elem,
            match elem {
                VecElementType::I16 => 0xD5,
                VecElementType::I32 | VecElementType::I64 => 0x40,
                _ => return false,
            },
        ),
        _ => return false,
    };
    prefix == X86SsePrefix::OpSize
        && opcode == expected_opcode
        && (!evex
            || match elem {
                VecElementType::I32 => !w,
                VecElementType::I64 => w,
                VecElementType::I8 | VecElementType::I16 => true,
                _ => false,
            })
}

fn x86_vector_integer_arithmetic_map_valid(
    kind: &crate::smir::ir::ops::OpKind,
    map: crate::smir::ir::ops::X86VecMap,
) -> bool {
    use crate::smir::ir::ops::{OpKind, X86VecMap};
    use crate::smir::ir::types::VecElementType;

    match kind {
        OpKind::VMul {
            elem: VecElementType::I32 | VecElementType::I64,
            ..
        } => map == X86VecMap::Map0F38,
        _ => map == X86VecMap::Map0F,
    }
}

fn x86_vector_integer_abs_encoding_valid(
    elem: crate::smir::ir::types::VecElementType,
    prefix: crate::smir::ir::ops::X86SsePrefix,
    opcode: u8,
    evex: bool,
    w: bool,
) -> bool {
    use crate::smir::ir::ops::X86SsePrefix;
    use crate::smir::ir::types::VecElementType;

    prefix == X86SsePrefix::OpSize
        && opcode
            == match elem {
                VecElementType::I8 => 0x1C,
                VecElementType::I16 => 0x1D,
                VecElementType::I32 => 0x1E,
                VecElementType::I64 => 0x1F,
                _ => return false,
            }
        && (!evex
            || match elem {
                VecElementType::I32 => !w,
                VecElementType::I64 => w,
                VecElementType::I8 | VecElementType::I16 => true,
                _ => false,
            })
}

fn x86_vector_integer_compare_encoding_valid(
    elem: crate::smir::ir::types::VecElementType,
    cond: crate::smir::ir::types::VecCmpCond,
    prefix: crate::smir::ir::ops::X86SsePrefix,
    opcode: u8,
) -> bool {
    use crate::smir::ir::ops::X86SsePrefix;
    use crate::smir::ir::types::{VecCmpCond, VecElementType};

    prefix == X86SsePrefix::OpSize
        && opcode
            == match (elem, cond) {
                (VecElementType::I8, VecCmpCond::Gt) => 0x64,
                (VecElementType::I16, VecCmpCond::Gt) => 0x65,
                (VecElementType::I32, VecCmpCond::Gt) => 0x66,
                (VecElementType::I8, VecCmpCond::Eq) => 0x74,
                (VecElementType::I16, VecCmpCond::Eq) => 0x75,
                (VecElementType::I32, VecCmpCond::Eq) => 0x76,
                (VecElementType::I64, VecCmpCond::Eq) => 0x29,
                (VecElementType::I64, VecCmpCond::Gt) => 0x37,
                _ => return false,
            }
}

fn x86_vector_integer_interleave_encoding_valid(
    elem: crate::smir::ir::types::VecElementType,
    high: bool,
    prefix: crate::smir::ir::ops::X86SsePrefix,
    opcode: u8,
) -> bool {
    use crate::smir::ir::ops::X86SsePrefix;
    use crate::smir::ir::types::VecElementType;

    prefix == X86SsePrefix::OpSize
        && opcode
            == match (elem, high) {
                (VecElementType::I8, false) => 0x60,
                (VecElementType::I16, false) => 0x61,
                (VecElementType::I32, false) => 0x62,
                (VecElementType::I64, false) => 0x6C,
                (VecElementType::I8, true) => 0x68,
                (VecElementType::I16, true) => 0x69,
                (VecElementType::I32, true) => 0x6A,
                (VecElementType::I64, true) => 0x6D,
                _ => return false,
            }
}

fn x86_vector_integer_pack_encoding_valid(
    src_elem: crate::smir::ir::types::VecElementType,
    to_unsigned: bool,
    prefix: crate::smir::ir::ops::X86SsePrefix,
    opcode: u8,
) -> bool {
    use crate::smir::ir::ops::X86SsePrefix;
    use crate::smir::ir::types::VecElementType;

    prefix == X86SsePrefix::OpSize
        && opcode
            == match (src_elem, to_unsigned) {
                (VecElementType::I16, false) => 0x63,
                (VecElementType::I16, true) => 0x67,
                (VecElementType::I32, false) => 0x6B,
                (VecElementType::I32, true) => 0x2B,
                _ => return false,
            }
}

fn x86_vector_integer_horizontal_encoding_valid(
    elem: crate::smir::ir::types::VecElementType,
    subtract: bool,
    saturating: bool,
    prefix: crate::smir::ir::ops::X86SsePrefix,
    opcode: u8,
) -> bool {
    use crate::smir::ir::ops::X86SsePrefix;
    use crate::smir::ir::types::VecElementType;

    prefix == X86SsePrefix::OpSize
        && opcode
            == match (elem, subtract, saturating) {
                (VecElementType::I16, false, false) => 0x01,
                (VecElementType::I32, false, false) => 0x02,
                (VecElementType::I16, false, true) => 0x03,
                (VecElementType::I16, true, false) => 0x05,
                (VecElementType::I32, true, false) => 0x06,
                (VecElementType::I16, true, true) => 0x07,
                _ => return false,
            }
}

/// Validate encoding metadata that can change the native opcode selected for
/// an otherwise well-formed architectural vector operation. This keeps
/// malformed SMIR from using native-vector admission to execute an arbitrary
/// hinted opcode.
fn x86_native_vector_smir_op(op: &crate::smir::ir::ops::SmirOp) -> bool {
    use crate::smir::ir::ops::{OpKind, X86OpHint, X86VecMap};
    use crate::smir::ir::types::{ArchReg, VReg, VecWidth, X86Reg};

    if !is_x86_native_vector_op(&op.kind) {
        return false;
    }
    let low_vector = |reg: &VReg| {
        matches!(
            reg,
            VReg::Arch(ArchReg::X86(
                X86Reg::Xmm(0..=15) | X86Reg::Ymm(0..=15) | X86Reg::Zmm(0..=15)
            ))
        )
    };

    if let OpKind::VMov { dst, src, width } = &op.kind {
        return match op.x86_hint {
            Some(X86OpHint::SseMov { prefix, opcode }) => {
                *width == VecWidth::V128
                    && low_vector(dst)
                    && low_vector(src)
                    && x86_vector_move_encoding_valid(prefix, opcode, false)
            }
            Some(X86OpHint::VexOp {
                map,
                pp,
                opcode,
                width: encoded_width,
                ..
            }) => {
                map == X86VecMap::Map0F
                    && encoded_width == *width
                    && *width != VecWidth::V512
                    && low_vector(dst)
                    && low_vector(src)
                    && x86_vector_move_encoding_valid(pp, opcode, false)
            }
            Some(X86OpHint::EvexOp {
                map,
                pp,
                opcode,
                width: encoded_width,
                ..
            }) => {
                map == X86VecMap::Map0F
                    && encoded_width == *width
                    && x86_vector_move_encoding_valid(pp, opcode, true)
            }
            _ => false,
        };
    }

    let logic_operands = match &op.kind {
        OpKind::VAnd {
            dst,
            src1,
            src2,
            width,
        }
        | OpKind::VAndNot {
            dst,
            src1,
            src2,
            width,
        }
        | OpKind::VOr {
            dst,
            src1,
            src2,
            width,
        }
        | OpKind::VXor {
            dst,
            src1,
            src2,
            width,
        } => Some((dst, src1, src2, width)),
        _ => None,
    };
    if let Some((dst, src1, src2, width)) = logic_operands {
        return match op.x86_hint {
            Some(X86OpHint::SseOp { prefix, opcode }) => {
                *width == VecWidth::V128
                    && dst == src1
                    && [dst, src1, src2].into_iter().all(low_vector)
                    && x86_vector_logic_encoding_valid(&op.kind, prefix, opcode, false, false)
            }
            Some(X86OpHint::VexOp {
                map,
                pp,
                opcode,
                width: encoded_width,
                w,
            }) => {
                map == X86VecMap::Map0F
                    && encoded_width == *width
                    && *width != VecWidth::V512
                    && [dst, src1, src2].into_iter().all(low_vector)
                    && x86_vector_logic_encoding_valid(&op.kind, pp, opcode, false, w)
            }
            Some(X86OpHint::EvexOp {
                map,
                pp,
                opcode,
                width: encoded_width,
                w,
            }) => {
                map == X86VecMap::Map0F
                    && encoded_width == *width
                    && x86_vector_logic_encoding_valid(&op.kind, pp, opcode, true, w)
            }
            _ => false,
        };
    }

    if let OpKind::VCmp {
        dst,
        src1,
        src2,
        cond,
        elem,
        lanes,
    } = &op.kind
    {
        let Some(width) = x86_vector_width_from_lanes(*elem, *lanes) else {
            return false;
        };
        let expected_map = if *elem == crate::smir::ir::types::VecElementType::I64 {
            X86VecMap::Map0F38
        } else {
            X86VecMap::Map0F
        };
        return match op.x86_hint {
            Some(X86OpHint::SseOp { prefix, opcode }) => {
                width == VecWidth::V128
                    && dst == src1
                    && [dst, src1, src2].into_iter().all(low_vector)
                    && x86_vector_integer_compare_encoding_valid(*elem, *cond, prefix, opcode)
            }
            Some(X86OpHint::VexOp {
                map,
                pp,
                opcode,
                width: encoded_width,
                ..
            }) => {
                map == expected_map
                    && encoded_width == width
                    && width != VecWidth::V512
                    && [dst, src1, src2].into_iter().all(low_vector)
                    && x86_vector_integer_compare_encoding_valid(*elem, *cond, pp, opcode)
            }
            _ => false,
        };
    }

    if let OpKind::VInterleave {
        dst,
        src1,
        src2,
        elem,
        lanes,
        high,
        ..
    } = &op.kind
    {
        let Some(width) = x86_vector_width_from_lanes(*elem, *lanes) else {
            return false;
        };
        return match op.x86_hint {
            Some(X86OpHint::SseOp { prefix, opcode }) => {
                width == VecWidth::V128
                    && dst == src1
                    && [dst, src1, src2].into_iter().all(low_vector)
                    && x86_vector_integer_interleave_encoding_valid(*elem, *high, prefix, opcode)
            }
            Some(X86OpHint::VexOp {
                map,
                pp,
                opcode,
                width: encoded_width,
                ..
            }) => {
                map == X86VecMap::Map0F
                    && encoded_width == width
                    && width != VecWidth::V512
                    && [dst, src1, src2].into_iter().all(low_vector)
                    && x86_vector_integer_interleave_encoding_valid(*elem, *high, pp, opcode)
            }
            Some(X86OpHint::EvexOp {
                map,
                pp,
                opcode,
                width: encoded_width,
                w,
            }) => {
                map == X86VecMap::Map0F
                    && encoded_width == width
                    && x86_vector_integer_interleave_encoding_valid(*elem, *high, pp, opcode)
                    && match elem {
                        crate::smir::ir::types::VecElementType::I8
                        | crate::smir::ir::types::VecElementType::I16 => true,
                        crate::smir::ir::types::VecElementType::I32 => !w,
                        crate::smir::ir::types::VecElementType::I64 => w,
                        _ => false,
                    }
            }
            _ => false,
        };
    }

    if let OpKind::VPackSat {
        dst,
        src1,
        src2,
        src_elem,
        to_unsigned,
        src_lanes,
        ..
    } = &op.kind
    {
        let Some(width) = x86_vector_width_from_lanes(*src_elem, *src_lanes) else {
            return false;
        };
        let expected_map =
            if *src_elem == crate::smir::ir::types::VecElementType::I32 && *to_unsigned {
                X86VecMap::Map0F38
            } else {
                X86VecMap::Map0F
            };
        return match op.x86_hint {
            Some(X86OpHint::SseOp { prefix, opcode }) => {
                width == VecWidth::V128
                    && dst == src2
                    && [dst, src1, src2].into_iter().all(low_vector)
                    && x86_vector_integer_pack_encoding_valid(
                        *src_elem,
                        *to_unsigned,
                        prefix,
                        opcode,
                    )
            }
            Some(X86OpHint::VexOp {
                map,
                pp,
                opcode,
                width: encoded_width,
                ..
            }) => {
                map == expected_map
                    && encoded_width == width
                    && width != VecWidth::V512
                    && [dst, src1, src2].into_iter().all(low_vector)
                    && x86_vector_integer_pack_encoding_valid(*src_elem, *to_unsigned, pp, opcode)
            }
            Some(X86OpHint::EvexOp {
                map,
                pp,
                opcode,
                width: encoded_width,
                w,
            }) => {
                map == expected_map
                    && encoded_width == width
                    && (*src_elem == crate::smir::ir::types::VecElementType::I16 || !w)
                    && x86_vector_integer_pack_encoding_valid(*src_elem, *to_unsigned, pp, opcode)
            }
            _ => false,
        };
    }

    if let OpKind::VByteShuffle {
        dst,
        src,
        control,
        lanes,
        ..
    } = &op.kind
    {
        let Some(width) =
            x86_vector_width_from_lanes(crate::smir::ir::types::VecElementType::I8, *lanes)
        else {
            return false;
        };
        return match op.x86_hint {
            Some(X86OpHint::SseOp { prefix, opcode }) => {
                width == VecWidth::V128
                    && dst == src
                    && [dst, src, control].into_iter().all(low_vector)
                    && prefix == crate::smir::ir::ops::X86SsePrefix::OpSize
                    && opcode == 0x00
            }
            Some(X86OpHint::VexOp {
                map,
                pp,
                opcode,
                width: encoded_width,
                ..
            }) => {
                map == X86VecMap::Map0F38
                    && pp == crate::smir::ir::ops::X86SsePrefix::OpSize
                    && opcode == 0x00
                    && encoded_width == width
                    && width != VecWidth::V512
                    && [dst, src, control].into_iter().all(low_vector)
            }
            Some(X86OpHint::EvexOp {
                map,
                pp,
                opcode,
                width: encoded_width,
                ..
            }) => {
                map == X86VecMap::Map0F38
                    && pp == crate::smir::ir::ops::X86SsePrefix::OpSize
                    && opcode == 0x00
                    && encoded_width == width
            }
            _ => false,
        };
    }

    if let OpKind::VHorizontalBin {
        dst,
        src1,
        src2,
        elem,
        lanes,
        subtract,
        saturating,
        ..
    } = &op.kind
    {
        let Some(width) = x86_vector_width_from_lanes(*elem, *lanes) else {
            return false;
        };
        return match op.x86_hint {
            Some(X86OpHint::SseOp { prefix, opcode }) => {
                width == VecWidth::V128
                    && dst == src1
                    && [dst, src1, src2].into_iter().all(low_vector)
                    && x86_vector_integer_horizontal_encoding_valid(
                        *elem,
                        *subtract,
                        *saturating,
                        prefix,
                        opcode,
                    )
            }
            Some(X86OpHint::VexOp {
                map,
                pp,
                opcode,
                width: encoded_width,
                ..
            }) => {
                map == X86VecMap::Map0F38
                    && encoded_width == width
                    && width != VecWidth::V512
                    && [dst, src1, src2].into_iter().all(low_vector)
                    && x86_vector_integer_horizontal_encoding_valid(
                        *elem,
                        *subtract,
                        *saturating,
                        pp,
                        opcode,
                    )
            }
            _ => false,
        };
    }

    if x86_vector_integer_mul_shift_shape_valid(&op.kind) {
        let OpKind::VMulShiftSat {
            dst,
            src1,
            src2,
            lanes,
            signed1,
            round,
            ..
        } = &op.kind
        else {
            unreachable!("validated PMULH[RU]SW shape is VMulShiftSat");
        };
        let width = match lanes {
            8 => VecWidth::V128,
            16 => VecWidth::V256,
            32 => VecWidth::V512,
            _ => unreachable!("validated PMULH[RU]SW lane count"),
        };
        let (expected_map, expected_opcode) = if *round {
            (X86VecMap::Map0F38, 0x0B)
        } else if *signed1 {
            (X86VecMap::Map0F, 0xE5)
        } else {
            (X86VecMap::Map0F, 0xE4)
        };
        return match op.x86_hint {
            Some(X86OpHint::SseOp { prefix, opcode }) => {
                width == VecWidth::V128
                    && dst == src1
                    && [dst, src1, src2].into_iter().all(low_vector)
                    && prefix == crate::smir::ir::ops::X86SsePrefix::OpSize
                    && opcode == expected_opcode
            }
            Some(X86OpHint::VexOp {
                map,
                pp,
                opcode,
                width: encoded_width,
                ..
            }) => {
                map == expected_map
                    && pp == crate::smir::ir::ops::X86SsePrefix::OpSize
                    && opcode == expected_opcode
                    && encoded_width == width
                    && width != VecWidth::V512
                    && [dst, src1, src2].into_iter().all(low_vector)
            }
            Some(X86OpHint::EvexOp {
                map,
                pp,
                opcode,
                width: encoded_width,
                ..
            }) => {
                map == expected_map
                    && pp == crate::smir::ir::ops::X86SsePrefix::OpSize
                    && opcode == expected_opcode
                    && encoded_width == width
            }
            _ => false,
        };
    }

    if x86_vector_integer_average_shape_valid(&op.kind) {
        let OpKind::VLane {
            dst,
            src1,
            src2,
            elem,
            lanes,
            ..
        } = &op.kind
        else {
            unreachable!("validated PAVG shape is a VLane");
        };
        let width = x86_vector_width_from_lanes(*elem, *lanes)
            .expect("validated PAVG shape has an x86 vector width");
        let expected_opcode = match elem {
            crate::smir::ir::types::VecElementType::I8 => 0xE0,
            crate::smir::ir::types::VecElementType::I16 => 0xE3,
            _ => unreachable!("validated PAVG element width"),
        };
        return match op.x86_hint {
            Some(X86OpHint::SseOp { prefix, opcode }) => {
                width == VecWidth::V128
                    && dst == src1
                    && [dst, src1, src2].into_iter().all(low_vector)
                    && prefix == crate::smir::ir::ops::X86SsePrefix::OpSize
                    && opcode == expected_opcode
            }
            Some(X86OpHint::VexOp {
                map,
                pp,
                opcode,
                width: encoded_width,
                ..
            }) => {
                map == X86VecMap::Map0F
                    && pp == crate::smir::ir::ops::X86SsePrefix::OpSize
                    && opcode == expected_opcode
                    && encoded_width == width
                    && width != VecWidth::V512
                    && [dst, src1, src2].into_iter().all(low_vector)
            }
            Some(X86OpHint::EvexOp {
                map,
                pp,
                opcode,
                width: encoded_width,
                ..
            }) => {
                map == X86VecMap::Map0F
                    && pp == crate::smir::ir::ops::X86SsePrefix::OpSize
                    && opcode == expected_opcode
                    && encoded_width == width
            }
            _ => false,
        };
    }

    if x86_vector_integer_sign_shape_valid(&op.kind) {
        let OpKind::VLane {
            dst,
            src1,
            src2,
            elem,
            lanes,
            ..
        } = &op.kind
        else {
            unreachable!("validated PSIGN shape is a VLane");
        };
        let width = x86_vector_width_from_lanes(*elem, *lanes)
            .expect("validated PSIGN shape has an x86 vector width");
        let expected_opcode = match elem {
            crate::smir::ir::types::VecElementType::I8 => 0x08,
            crate::smir::ir::types::VecElementType::I16 => 0x09,
            crate::smir::ir::types::VecElementType::I32 => 0x0A,
            _ => unreachable!("validated PSIGN element width"),
        };
        return match op.x86_hint {
            Some(X86OpHint::SseOp { prefix, opcode }) => {
                width == VecWidth::V128
                    && dst == src1
                    && [dst, src1, src2].into_iter().all(low_vector)
                    && prefix == crate::smir::ir::ops::X86SsePrefix::OpSize
                    && opcode == expected_opcode
            }
            Some(X86OpHint::VexOp {
                map,
                pp,
                opcode,
                width: encoded_width,
                ..
            }) => {
                map == X86VecMap::Map0F38
                    && pp == crate::smir::ir::ops::X86SsePrefix::OpSize
                    && opcode == expected_opcode
                    && encoded_width == width
                    && [dst, src1, src2].into_iter().all(low_vector)
            }
            _ => false,
        };
    }

    if x86_vector_integer_minmax_shape_valid(&op.kind) {
        let OpKind::VLane {
            dst,
            src1,
            src2,
            elem,
            lanes,
            ..
        } = &op.kind
        else {
            unreachable!("validated packed min/max shape is a VLane");
        };
        let width = x86_vector_width_from_lanes(*elem, *lanes)
            .expect("validated packed min/max shape has an x86 vector width");
        let (expected_map, expected_opcode) = x86_vector_integer_minmax_encoding(&op.kind)
            .expect("validated packed min/max shape has an x86 encoding");
        return match op.x86_hint {
            Some(X86OpHint::SseOp { prefix, opcode }) => {
                *elem != crate::smir::ir::types::VecElementType::I64
                    && width == VecWidth::V128
                    && dst == src1
                    && [dst, src1, src2].into_iter().all(low_vector)
                    && prefix == crate::smir::ir::ops::X86SsePrefix::OpSize
                    && opcode == expected_opcode
            }
            Some(X86OpHint::VexOp {
                map,
                pp,
                opcode,
                width: encoded_width,
                ..
            }) => {
                *elem != crate::smir::ir::types::VecElementType::I64
                    && map == expected_map
                    && pp == crate::smir::ir::ops::X86SsePrefix::OpSize
                    && opcode == expected_opcode
                    && encoded_width == width
                    && width != VecWidth::V512
                    && [dst, src1, src2].into_iter().all(low_vector)
            }
            Some(X86OpHint::EvexOp {
                map,
                pp,
                opcode,
                width: encoded_width,
                w,
            }) => {
                map == expected_map
                    && pp == crate::smir::ir::ops::X86SsePrefix::OpSize
                    && opcode == expected_opcode
                    && encoded_width == width
                    && match elem {
                        crate::smir::ir::types::VecElementType::I8
                        | crate::smir::ir::types::VecElementType::I16 => true,
                        crate::smir::ir::types::VecElementType::I32 => !w,
                        crate::smir::ir::types::VecElementType::I64 => w,
                        _ => false,
                    }
            }
            _ => false,
        };
    }

    if x86_vector_sad_bytes_shape_valid(&op.kind) {
        let OpKind::VSadBytes {
            dst,
            src1,
            src2,
            width,
        } = &op.kind
        else {
            unreachable!("validated PSADBW shape is VSadBytes");
        };
        return match op.x86_hint {
            Some(X86OpHint::SseOp { prefix, opcode }) => {
                *width == VecWidth::V128
                    && dst == src1
                    && [dst, src1, src2].into_iter().all(low_vector)
                    && prefix == crate::smir::ir::ops::X86SsePrefix::OpSize
                    && opcode == 0xF6
            }
            Some(X86OpHint::VexOp {
                map,
                pp,
                opcode,
                width: encoded_width,
                ..
            }) => {
                map == X86VecMap::Map0F
                    && pp == crate::smir::ir::ops::X86SsePrefix::OpSize
                    && opcode == 0xF6
                    && encoded_width == *width
                    && *width != VecWidth::V512
                    && [dst, src1, src2].into_iter().all(low_vector)
            }
            Some(X86OpHint::EvexOp {
                map,
                pp,
                opcode,
                width: encoded_width,
                ..
            }) => {
                map == X86VecMap::Map0F
                    && pp == crate::smir::ir::ops::X86SsePrefix::OpSize
                    && opcode == 0xF6
                    && encoded_width == *width
            }
            _ => false,
        };
    }

    if x86_phminposuw_shape_valid(&op.kind) {
        let OpKind::X86Phminposuw { dst, src } = &op.kind else {
            unreachable!("validated PHMINPOSUW shape is X86Phminposuw");
        };
        return match op.x86_hint {
            Some(X86OpHint::SseOp { prefix, opcode }) => {
                prefix == crate::smir::ir::ops::X86SsePrefix::OpSize
                    && opcode == 0x41
                    && low_vector(dst)
                    && low_vector(src)
            }
            Some(X86OpHint::VexOp {
                map,
                pp,
                opcode,
                width,
                ..
            }) => {
                map == X86VecMap::Map0F38
                    && pp == crate::smir::ir::ops::X86SsePrefix::OpSize
                    && opcode == 0x41
                    && width == VecWidth::V128
                    && low_vector(dst)
                    && low_vector(src)
            }
            _ => false,
        };
    }

    if x86_movd_q_shape_valid(&op.kind) {
        let OpKind::X86MovdQ {
            dst,
            src,
            width,
            zero_upper,
        } = &op.kind
        else {
            unreachable!("validated MOVD/MOVQ shape is X86MovdQ");
        };
        let vector_dst = matches!(dst, VReg::Arch(ArchReg::X86(X86Reg::Xmm(_))));
        let xmm = if vector_dst { dst } else { src };
        let expected_opcode = if vector_dst { 0x6E } else { 0x7E };
        return match op.x86_hint {
            Some(X86OpHint::SseOp { prefix, opcode }) => {
                prefix == crate::smir::ir::ops::X86SsePrefix::OpSize
                    && opcode == expected_opcode
                    && !*zero_upper
                    && low_vector(xmm)
            }
            Some(X86OpHint::VexOp {
                map,
                pp,
                opcode,
                width: encoded_width,
                w,
            }) => {
                map == X86VecMap::Map0F
                    && pp == crate::smir::ir::ops::X86SsePrefix::OpSize
                    && opcode == expected_opcode
                    && encoded_width == VecWidth::V128
                    && w == (*width == crate::smir::ir::types::OpWidth::W64)
                    && *zero_upper == vector_dst
                    && low_vector(xmm)
            }
            Some(X86OpHint::EvexOp {
                map,
                pp,
                opcode,
                width: encoded_width,
                w,
            }) => {
                map == X86VecMap::Map0F
                    && pp == crate::smir::ir::ops::X86SsePrefix::OpSize
                    && opcode == expected_opcode
                    && encoded_width == VecWidth::V128
                    && w == (*width == crate::smir::ir::types::OpWidth::W64)
                    && *zero_upper == vector_dst
            }
            _ => false,
        };
    }

    if x86_mov_mask_shape_valid(&op.kind) {
        let OpKind::X86MovMask {
            elem,
            lanes,
            dst_width,
            ..
        } = &op.kind
        else {
            unreachable!("validated MOVMSK shape is X86MovMask");
        };
        let expected_width = x86_vector_width_from_lanes(*elem, *lanes)
            .expect("validated MOVMSK shape has an x86 vector width");
        return match op.x86_hint {
            Some(X86OpHint::SseOp { prefix, opcode }) => {
                expected_width == VecWidth::V128
                    && matches!(
                        dst_width,
                        crate::smir::ir::types::OpWidth::W32 | crate::smir::ir::types::OpWidth::W64
                    )
                    && match (opcode, prefix, elem) {
                        (
                            0x50,
                            crate::smir::ir::ops::X86SsePrefix::None,
                            crate::smir::ir::types::VecElementType::F32,
                        )
                        | (
                            0x50,
                            crate::smir::ir::ops::X86SsePrefix::OpSize,
                            crate::smir::ir::types::VecElementType::F64,
                        )
                        | (
                            0xD7,
                            crate::smir::ir::ops::X86SsePrefix::OpSize,
                            crate::smir::ir::types::VecElementType::I8,
                        ) => true,
                        _ => false,
                    }
            }
            Some(X86OpHint::VexOp {
                map,
                pp,
                opcode,
                width,
                ..
            }) => {
                map == X86VecMap::Map0F
                    && width == expected_width
                    && width != VecWidth::V512
                    && *dst_width == crate::smir::ir::types::OpWidth::W32
                    && match (opcode, pp, elem) {
                        (
                            0x50,
                            crate::smir::ir::ops::X86SsePrefix::None,
                            crate::smir::ir::types::VecElementType::F32,
                        )
                        | (
                            0x50,
                            crate::smir::ir::ops::X86SsePrefix::OpSize,
                            crate::smir::ir::types::VecElementType::F64,
                        )
                        | (
                            0xD7,
                            crate::smir::ir::ops::X86SsePrefix::OpSize,
                            crate::smir::ir::types::VecElementType::I8,
                        ) => true,
                        _ => false,
                    }
            }
            _ => false,
        };
    }

    if x86_vector_mpsadbw_shape_valid(&op.kind) {
        let OpKind::VMpsadbw {
            dst,
            src1,
            src2,
            width,
            ..
        } = &op.kind
        else {
            unreachable!("validated MPSADBW shape is VMpsadbw");
        };
        return match op.x86_hint {
            Some(X86OpHint::SseOp { prefix, opcode }) => {
                *width == VecWidth::V128
                    && dst == src1
                    && [dst, src1, src2].into_iter().all(low_vector)
                    && prefix == crate::smir::ir::ops::X86SsePrefix::OpSize
                    && opcode == 0x42
            }
            Some(X86OpHint::VexOp {
                map,
                pp,
                opcode,
                width: encoded_width,
                ..
            }) => {
                map == X86VecMap::Map0F3A
                    && pp == crate::smir::ir::ops::X86SsePrefix::OpSize
                    && opcode == 0x42
                    && encoded_width == *width
                    && [dst, src1, src2].into_iter().all(low_vector)
            }
            _ => false,
        };
    }

    if x86_vector_integer_maddubs_shape_valid(&op.kind) {
        let OpKind::VDotProduct {
            dst,
            src1,
            src2,
            width,
            ..
        } = &op.kind
        else {
            unreachable!("validated PMADDUBSW shape is a VDotProduct");
        };
        return match op.x86_hint {
            Some(X86OpHint::SseOp { prefix, opcode }) => {
                *width == VecWidth::V128
                    && dst == src1
                    && [dst, src1, src2].into_iter().all(low_vector)
                    && prefix == crate::smir::ir::ops::X86SsePrefix::OpSize
                    && opcode == 0x04
            }
            Some(X86OpHint::VexOp {
                map,
                pp,
                opcode,
                width: encoded_width,
                ..
            }) => {
                map == X86VecMap::Map0F38
                    && pp == crate::smir::ir::ops::X86SsePrefix::OpSize
                    && opcode == 0x04
                    && encoded_width == *width
                    && *width != VecWidth::V512
                    && [dst, src1, src2].into_iter().all(low_vector)
            }
            Some(X86OpHint::EvexOp {
                map,
                pp,
                opcode,
                width: encoded_width,
                ..
            }) => {
                map == X86VecMap::Map0F38
                    && pp == crate::smir::ir::ops::X86SsePrefix::OpSize
                    && opcode == 0x04
                    && encoded_width == *width
            }
            _ => false,
        };
    }

    if x86_vector_integer_maddwd_shape_valid(&op.kind) {
        let OpKind::VDotProduct {
            dst,
            src1,
            src2,
            width,
            ..
        } = &op.kind
        else {
            unreachable!("validated PMADDWD shape is a VDotProduct");
        };
        return match op.x86_hint {
            Some(X86OpHint::SseOp { prefix, opcode }) => {
                *width == VecWidth::V128
                    && dst == src1
                    && [dst, src1, src2].into_iter().all(low_vector)
                    && prefix == crate::smir::ir::ops::X86SsePrefix::OpSize
                    && opcode == 0xF5
            }
            Some(X86OpHint::VexOp {
                map,
                pp,
                opcode,
                width: encoded_width,
                ..
            }) => {
                map == X86VecMap::Map0F
                    && pp == crate::smir::ir::ops::X86SsePrefix::OpSize
                    && opcode == 0xF5
                    && encoded_width == *width
                    && *width != VecWidth::V512
                    && [dst, src1, src2].into_iter().all(low_vector)
            }
            Some(X86OpHint::EvexOp {
                map,
                pp,
                opcode,
                width: encoded_width,
                ..
            }) => {
                map == X86VecMap::Map0F
                    && pp == crate::smir::ir::ops::X86SsePrefix::OpSize
                    && opcode == 0xF5
                    && encoded_width == *width
            }
            _ => false,
        };
    }

    if let OpKind::VUnary {
        dst,
        src,
        elem,
        lanes,
        op: crate::smir::ir::types::VecUnaryOp::Abs,
    } = &op.kind
    {
        let Some(width) = x86_vector_width_from_lanes(*elem, *lanes) else {
            return false;
        };
        return match op.x86_hint {
            Some(X86OpHint::SseOp { prefix, opcode }) => {
                width == VecWidth::V128
                    && *elem != crate::smir::ir::types::VecElementType::I64
                    && low_vector(dst)
                    && low_vector(src)
                    && x86_vector_integer_abs_encoding_valid(*elem, prefix, opcode, false, false)
            }
            Some(X86OpHint::VexOp {
                map,
                pp,
                opcode,
                width: encoded_width,
                w,
            }) => {
                map == X86VecMap::Map0F38
                    && encoded_width == width
                    && width != VecWidth::V512
                    && *elem != crate::smir::ir::types::VecElementType::I64
                    && low_vector(dst)
                    && low_vector(src)
                    && x86_vector_integer_abs_encoding_valid(*elem, pp, opcode, false, w)
            }
            Some(X86OpHint::EvexOp {
                map,
                pp,
                opcode,
                width: encoded_width,
                w,
            }) => {
                map == X86VecMap::Map0F38
                    && encoded_width == width
                    && x86_vector_integer_abs_encoding_valid(*elem, pp, opcode, true, w)
            }
            _ => false,
        };
    }

    let (dst, src1, src2, elem, lanes) = match &op.kind {
        OpKind::VAdd {
            dst,
            src1,
            src2,
            elem,
            lanes,
        }
        | OpKind::VSub {
            dst,
            src1,
            src2,
            elem,
            lanes,
        }
        | OpKind::VAddSubSat {
            dst,
            src1,
            src2,
            elem,
            lanes,
            ..
        }
        | OpKind::VMul {
            dst,
            src1,
            src2,
            elem,
            lanes,
        } => (dst, src1, src2, elem, lanes),
        _ => return true,
    };
    let Some(width) = x86_vector_width_from_lanes(*elem, *lanes) else {
        return false;
    };

    match op.x86_hint {
        Some(X86OpHint::SseOp { prefix, opcode }) => {
            width == VecWidth::V128
                && dst == src1
                && [dst, src1, src2].into_iter().all(low_vector)
                && !matches!(
                    op.kind,
                    OpKind::VMul {
                        elem: crate::smir::ir::types::VecElementType::I64,
                        ..
                    }
                )
                && x86_vector_integer_arithmetic_encoding_valid(
                    &op.kind, prefix, opcode, false, false,
                )
        }
        Some(X86OpHint::VexOp {
            map,
            pp,
            opcode,
            width: encoded_width,
            w,
        }) => {
            x86_vector_integer_arithmetic_map_valid(&op.kind, map)
                && encoded_width == width
                && width != VecWidth::V512
                && [dst, src1, src2].into_iter().all(low_vector)
                && !matches!(
                    op.kind,
                    OpKind::VMul {
                        elem: crate::smir::ir::types::VecElementType::I64,
                        ..
                    }
                )
                && x86_vector_integer_arithmetic_encoding_valid(&op.kind, pp, opcode, false, w)
        }
        Some(X86OpHint::EvexOp {
            map,
            pp,
            opcode,
            width: encoded_width,
            w,
        }) => {
            x86_vector_integer_arithmetic_map_valid(&op.kind, map)
                && encoded_width == width
                && x86_vector_integer_arithmetic_encoding_valid(&op.kind, pp, opcode, true, w)
        }
        _ => false,
    }
}

fn x86_vector_move_needs_vl(op: &crate::smir::ir::ops::SmirOp) -> bool {
    use crate::smir::ir::ops::{OpKind, X86OpHint};
    use crate::smir::ir::types::{ArchReg, VReg, VecWidth, X86Reg};

    let OpKind::VMov { dst, src, width } = &op.kind else {
        return false;
    };
    if *width == VecWidth::V512 {
        return false;
    }
    let high_vector = |reg: &VReg| {
        matches!(
            reg,
            VReg::Arch(ArchReg::X86(
                X86Reg::Xmm(16..=31) | X86Reg::Ymm(16..=31) | X86Reg::Zmm(16..=31)
            ))
        )
    };
    high_vector(dst) || high_vector(src) || matches!(op.x86_hint, Some(X86OpHint::EvexOp { .. }))
}

/// Return `(AVX, AVX2, AVX-512DQ, AVX-512VL)` requirements for an admitted
/// architectural vector-logic operation. VEX integer logic needs AVX2 only at
/// 256 bits; EVEX floating logical encodings are in AVX-512DQ, while EVEX
/// integer D/Q encodings are in AVX-512F.
fn x86_vector_logic_feature_requirements(
    op: &crate::smir::ir::ops::SmirOp,
) -> (bool, bool, bool, bool) {
    use crate::smir::ir::ops::{OpKind, X86OpHint};
    use crate::smir::ir::types::VecWidth;

    if !matches!(
        op.kind,
        OpKind::VAnd { .. } | OpKind::VAndNot { .. } | OpKind::VOr { .. } | OpKind::VXor { .. }
    ) {
        return (false, false, false, false);
    }

    match op.x86_hint {
        Some(X86OpHint::VexOp { opcode, width, .. }) => {
            let integer_256 =
                width == VecWidth::V256 && matches!(opcode, 0xDB | 0xDF | 0xEB | 0xEF);
            (true, integer_256, false, false)
        }
        Some(X86OpHint::EvexOp { opcode, width, .. }) => (
            false,
            false,
            matches!(opcode, 0x54..=0x57),
            width != VecWidth::V512,
        ),
        _ => (false, false, false, false),
    }
}

/// Return `(AVX, AVX2, AVX-512VL)` requirements for an admitted wrapping or
/// saturating packed-integer add/subtract operation.
fn x86_vector_integer_arithmetic_feature_requirements(
    op: &crate::smir::ir::ops::SmirOp,
) -> (bool, bool, bool) {
    use crate::smir::ir::ops::{OpKind, X86OpHint};
    use crate::smir::ir::types::VecWidth;

    if !matches!(
        op.kind,
        OpKind::VAdd { .. } | OpKind::VSub { .. } | OpKind::VAddSubSat { .. }
    ) {
        return (false, false, false);
    }
    match op.x86_hint {
        Some(X86OpHint::VexOp { width, .. }) => (true, width == VecWidth::V256, false),
        Some(X86OpHint::EvexOp { width, .. }) => (false, false, width != VecWidth::V512),
        _ => (false, false, false),
    }
}

/// Return `(SSE4.1, AVX, AVX2, AVX-512DQ, AVX-512VL)` requirements for an
/// admitted low-product packed-integer multiply operation. AVX-512F/BW are
/// already unconditional trampoline requirements.
fn x86_vector_integer_multiply_feature_requirements(
    op: &crate::smir::ir::ops::SmirOp,
) -> (bool, bool, bool, bool, bool) {
    use crate::smir::ir::ops::{OpKind, X86OpHint};
    use crate::smir::ir::types::{VecElementType, VecWidth};

    let OpKind::VMul { elem, .. } = op.kind else {
        return (false, false, false, false, false);
    };
    match op.x86_hint {
        Some(X86OpHint::SseOp { .. }) => (elem == VecElementType::I32, false, false, false, false),
        Some(X86OpHint::VexOp { width, .. }) => {
            (false, true, width == VecWidth::V256, false, false)
        }
        Some(X86OpHint::EvexOp { width, .. }) => (
            false,
            false,
            false,
            elem == VecElementType::I64,
            width != VecWidth::V512,
        ),
        _ => (false, false, false, false, false),
    }
}

/// Return `(SSSE3, AVX, AVX2, AVX-512VL)` requirements for an admitted packed
/// integer absolute-value operation. AVX-512F/BW are unconditional trampoline
/// requirements and cover EVEX dword/qword and byte/word forms respectively.
fn x86_vector_integer_abs_feature_requirements(
    op: &crate::smir::ir::ops::SmirOp,
) -> (bool, bool, bool, bool) {
    use crate::smir::ir::ops::{OpKind, X86OpHint};
    use crate::smir::ir::types::VecWidth;

    if !matches!(
        op.kind,
        OpKind::VUnary {
            op: crate::smir::ir::types::VecUnaryOp::Abs,
            ..
        }
    ) {
        return (false, false, false, false);
    }
    match op.x86_hint {
        Some(X86OpHint::SseOp { .. }) => (true, false, false, false),
        Some(X86OpHint::VexOp { width, .. }) => (false, true, width == VecWidth::V256, false),
        Some(X86OpHint::EvexOp { width, .. }) => (false, false, false, width != VecWidth::V512),
        _ => (false, false, false, false),
    }
}

/// Return `(SSE4.1, SSE4.2, AVX, AVX2)` requirements for an admitted fixed-
/// predicate packed-integer comparison. Byte/word/dword legacy forms use the
/// x86-64 baseline SSE2 feature.
fn x86_vector_integer_compare_feature_requirements(
    op: &crate::smir::ir::ops::SmirOp,
) -> (bool, bool, bool, bool) {
    use crate::smir::ir::ops::{OpKind, X86OpHint};
    use crate::smir::ir::types::{VecCmpCond, VecElementType, VecWidth};

    let OpKind::VCmp { elem, cond, .. } = op.kind else {
        return (false, false, false, false);
    };
    match op.x86_hint {
        Some(X86OpHint::SseOp { .. }) => (
            elem == VecElementType::I64 && cond == VecCmpCond::Eq,
            elem == VecElementType::I64 && cond == VecCmpCond::Gt,
            false,
            false,
        ),
        Some(X86OpHint::VexOp { width, .. }) => (false, false, true, width == VecWidth::V256),
        _ => (false, false, false, false),
    }
}

/// Return `(AVX, AVX2, AVX-512VL)` requirements for an admitted packed-integer
/// interleave. Legacy forms use baseline SSE2; EVEX.512 forms do not need VL.
fn x86_vector_integer_interleave_feature_requirements(
    op: &crate::smir::ir::ops::SmirOp,
) -> (bool, bool, bool) {
    use crate::smir::ir::ops::{OpKind, X86OpHint};
    use crate::smir::ir::types::VecWidth;

    if !matches!(op.kind, OpKind::VInterleave { .. }) {
        return (false, false, false);
    }
    match op.x86_hint {
        Some(X86OpHint::SseOp { .. }) => (false, false, false),
        Some(X86OpHint::VexOp { width, .. }) => (true, width == VecWidth::V256, false),
        Some(X86OpHint::EvexOp { width, .. }) => (false, false, width != VecWidth::V512),
        _ => (false, false, false),
    }
}

/// Return `(SSE4.1, AVX, AVX2, AVX-512VL)` requirements for an admitted packed
/// saturating narrow. Legacy PACKUSDW is the only non-baseline SSE2 form.
fn x86_vector_integer_pack_feature_requirements(
    op: &crate::smir::ir::ops::SmirOp,
) -> (bool, bool, bool, bool) {
    use crate::smir::ir::ops::{OpKind, X86OpHint};
    use crate::smir::ir::types::{VecElementType, VecWidth};

    let OpKind::VPackSat {
        src_elem,
        to_unsigned,
        ..
    } = op.kind
    else {
        return (false, false, false, false);
    };
    match op.x86_hint {
        Some(X86OpHint::SseOp { .. }) => (
            src_elem == VecElementType::I32 && to_unsigned,
            false,
            false,
            false,
        ),
        Some(X86OpHint::VexOp { width, .. }) => (false, true, width == VecWidth::V256, false),
        Some(X86OpHint::EvexOp { width, .. }) => (false, false, false, width != VecWidth::V512),
        _ => (false, false, false, false),
    }
}

/// Return `(SSSE3, AVX, AVX2, AVX-512VL)` requirements for an admitted packed
/// byte shuffle. EVEX.512 uses AVX-512BW without the VL extension.
fn x86_vector_byte_shuffle_feature_requirements(
    op: &crate::smir::ir::ops::SmirOp,
) -> (bool, bool, bool, bool) {
    use crate::smir::ir::ops::{OpKind, X86OpHint};
    use crate::smir::ir::types::VecWidth;

    if !matches!(op.kind, OpKind::VByteShuffle { .. }) {
        return (false, false, false, false);
    }
    match op.x86_hint {
        Some(X86OpHint::SseOp { .. }) => (true, false, false, false),
        Some(X86OpHint::VexOp { width, .. }) => (false, true, width == VecWidth::V256, false),
        Some(X86OpHint::EvexOp { width, .. }) => (false, false, false, width != VecWidth::V512),
        _ => (false, false, false, false),
    }
}

/// Return `(SSSE3, AVX, AVX2)` requirements for an admitted packed-integer
/// horizontal operation. VEX.128 uses AVX; VEX.256 uses AVX2.
fn x86_vector_integer_horizontal_feature_requirements(
    op: &crate::smir::ir::ops::SmirOp,
) -> (bool, bool, bool) {
    use crate::smir::ir::ops::{OpKind, X86OpHint};
    use crate::smir::ir::types::VecWidth;

    if !matches!(op.kind, OpKind::VHorizontalBin { .. }) {
        return (false, false, false);
    }
    match op.x86_hint {
        Some(X86OpHint::SseOp { .. }) => (true, false, false),
        Some(X86OpHint::VexOp { width, .. }) => (false, true, width == VecWidth::V256),
        _ => (false, false, false),
    }
}

/// Return `(SSSE3, AVX, AVX2, AVX-512VL)` requirements for an admitted
/// PMULHW/PMULHUW/PMULHRSW operation. Legacy PMULHW/PMULHUW are baseline SSE2;
/// legacy PMULHRSW requires SSSE3. EVEX.512 uses the trampoline's AVX-512BW;
/// EVEX.128/256 additionally require AVX-512VL.
fn x86_vector_integer_mul_shift_feature_requirements(
    op: &crate::smir::ir::ops::SmirOp,
) -> (bool, bool, bool, bool) {
    use crate::smir::ir::ops::{OpKind, X86OpHint};
    use crate::smir::ir::types::VecWidth;

    if !x86_vector_integer_mul_shift_shape_valid(&op.kind) {
        return (false, false, false, false);
    }
    let legacy_needs_ssse3 = matches!(op.kind, OpKind::VMulShiftSat { round: true, .. });
    match op.x86_hint {
        Some(X86OpHint::SseOp { .. }) => (legacy_needs_ssse3, false, false, false),
        Some(X86OpHint::VexOp { width, .. }) => (false, true, width == VecWidth::V256, false),
        Some(X86OpHint::EvexOp { width, .. }) => (false, false, false, width != VecWidth::V512),
        _ => (false, false, false, false),
    }
}

/// Return `(AVX, AVX2, AVX-512VL)` requirements for an admitted
/// PAVGB/PAVGW/VPAVGB/VPAVGW. Legacy forms are baseline SSE2 on x86-64;
/// EVEX.512 uses the trampoline's unconditional AVX-512BW requirement.
fn x86_vector_integer_average_feature_requirements(
    op: &crate::smir::ir::ops::SmirOp,
) -> (bool, bool, bool) {
    use crate::smir::ir::ops::X86OpHint;
    use crate::smir::ir::types::VecWidth;

    if !x86_vector_integer_average_shape_valid(&op.kind) {
        return (false, false, false);
    }
    match op.x86_hint {
        Some(X86OpHint::SseOp { .. }) => (false, false, false),
        Some(X86OpHint::VexOp { width, .. }) => (true, width == VecWidth::V256, false),
        Some(X86OpHint::EvexOp { width, .. }) => (false, false, width != VecWidth::V512),
        _ => (false, false, false),
    }
}

/// Return `(SSSE3, AVX, AVX2)` requirements for an admitted
/// PSIGNB/PSIGNW/PSIGND or VPSIGNB/VPSIGNW/VPSIGND operation.
fn x86_vector_integer_sign_feature_requirements(
    op: &crate::smir::ir::ops::SmirOp,
) -> (bool, bool, bool) {
    use crate::smir::ir::ops::X86OpHint;
    use crate::smir::ir::types::VecWidth;

    if !x86_vector_integer_sign_shape_valid(&op.kind) {
        return (false, false, false);
    }
    match op.x86_hint {
        Some(X86OpHint::SseOp { .. }) => (true, false, false),
        Some(X86OpHint::VexOp { width, .. }) => (false, true, width == VecWidth::V256),
        _ => (false, false, false),
    }
}

/// Return `(SSE4.1, AVX, AVX2, AVX-512VL)` requirements for an admitted
/// packed-integer minimum/maximum. The original unsigned-byte/signed-word
/// legacy encodings are baseline SSE2; other legacy encodings require SSE4.1.
fn x86_vector_integer_minmax_feature_requirements(
    op: &crate::smir::ir::ops::SmirOp,
) -> (bool, bool, bool, bool) {
    use crate::smir::ir::ops::{X86OpHint, X86VecMap};
    use crate::smir::ir::types::VecWidth;

    if !x86_vector_integer_minmax_shape_valid(&op.kind) {
        return (false, false, false, false);
    }
    let map = x86_vector_integer_minmax_encoding(&op.kind)
        .map(|(map, _)| map)
        .expect("validated packed min/max shape has an x86 encoding");
    match op.x86_hint {
        Some(X86OpHint::SseOp { .. }) => (map == X86VecMap::Map0F38, false, false, false),
        Some(X86OpHint::VexOp { width, .. }) => (false, true, width == VecWidth::V256, false),
        Some(X86OpHint::EvexOp { width, .. }) => (false, false, false, width != VecWidth::V512),
        _ => (false, false, false, false),
    }
}

/// Return `(AVX, AVX2, AVX-512VL)` requirements for an admitted
/// PSADBW/VPSADBW. Legacy PSADBW is baseline SSE2 on x86-64; EVEX.512 uses
/// the trampoline's unconditional AVX-512BW requirement.
fn x86_vector_sad_bytes_feature_requirements(
    op: &crate::smir::ir::ops::SmirOp,
) -> (bool, bool, bool) {
    use crate::smir::ir::ops::X86OpHint;
    use crate::smir::ir::types::VecWidth;

    if !x86_vector_sad_bytes_shape_valid(&op.kind) {
        return (false, false, false);
    }
    match op.x86_hint {
        Some(X86OpHint::SseOp { .. }) => (false, false, false),
        Some(X86OpHint::VexOp { width, .. }) => (true, width == VecWidth::V256, false),
        Some(X86OpHint::EvexOp { width, .. }) => (false, false, width != VecWidth::V512),
        _ => (false, false, false),
    }
}

/// Return `(SSE4.1, AVX)` requirements for an admitted register-only
/// PHMINPOSUW/VPHMINPOSUW operation.
fn x86_phminposuw_feature_requirements(op: &crate::smir::ir::ops::SmirOp) -> (bool, bool) {
    use crate::smir::ir::ops::X86OpHint;

    if !x86_phminposuw_shape_valid(&op.kind) {
        return (false, false);
    }
    match op.x86_hint {
        Some(X86OpHint::SseOp { .. }) => (true, false),
        Some(X86OpHint::VexOp { .. }) => (false, true),
        _ => (false, false),
    }
}

/// Return `(AVX, AVX2)` requirements for an admitted MOVMSK operation.
/// Every VEX form needs AVX; only VPMOVMSKB with a 256-bit source additionally
/// needs AVX2. Legacy SSE/SSE2 forms are baseline on x86-64 hosts.
fn x86_mov_mask_feature_requirements(op: &crate::smir::ir::ops::SmirOp) -> (bool, bool) {
    use crate::smir::ir::ops::X86OpHint;
    use crate::smir::ir::types::VecWidth;

    if !x86_mov_mask_shape_valid(&op.kind) {
        return (false, false);
    }
    match op.x86_hint {
        Some(X86OpHint::VexOp { opcode, width, .. }) => {
            (true, opcode == 0xD7 && width == VecWidth::V256)
        }
        _ => (false, false),
    }
}

/// Return `(SSE4.1, AVX, AVX2)` requirements for an admitted
/// MPSADBW/VMPSADBW. VEX.128 requires AVX; VEX.256 additionally requires
/// AVX2. AVX10.2 EVEX forms are not admitted by this classic path.
fn x86_vector_mpsadbw_feature_requirements(
    op: &crate::smir::ir::ops::SmirOp,
) -> (bool, bool, bool) {
    use crate::smir::ir::ops::X86OpHint;
    use crate::smir::ir::types::VecWidth;

    if !x86_vector_mpsadbw_shape_valid(&op.kind) {
        return (false, false, false);
    }
    match op.x86_hint {
        Some(X86OpHint::SseOp { .. }) => (true, false, false),
        Some(X86OpHint::VexOp { width, .. }) => (false, true, width == VecWidth::V256),
        _ => (false, false, false),
    }
}

/// Return `(SSSE3, AVX, AVX2, AVX-512VL)` requirements for an admitted
/// PMADDUBSW/VPMADDUBSW. EVEX.512 requires AVX-512BW, which the vector-state
/// trampoline already requires unconditionally; EVEX.128/256 additionally
/// require AVX-512VL.
fn x86_vector_integer_maddubs_feature_requirements(
    op: &crate::smir::ir::ops::SmirOp,
) -> (bool, bool, bool, bool) {
    use crate::smir::ir::ops::X86OpHint;
    use crate::smir::ir::types::VecWidth;

    if !x86_vector_integer_maddubs_shape_valid(&op.kind) {
        return (false, false, false, false);
    }
    match op.x86_hint {
        Some(X86OpHint::SseOp { .. }) => (true, false, false, false),
        Some(X86OpHint::VexOp { width, .. }) => (false, true, width == VecWidth::V256, false),
        Some(X86OpHint::EvexOp { width, .. }) => (false, false, false, width != VecWidth::V512),
        _ => (false, false, false, false),
    }
}

/// Return `(AVX, AVX2, AVX-512VL)` requirements for an admitted
/// PMADDWD/VPMADDWD. Legacy PMADDWD is baseline SSE2 on x86-64. EVEX.512 uses
/// AVX-512BW, already required by the vector-state trampoline; EVEX.128/256
/// additionally require AVX-512VL.
fn x86_vector_integer_maddwd_feature_requirements(
    op: &crate::smir::ir::ops::SmirOp,
) -> (bool, bool, bool) {
    use crate::smir::ir::ops::X86OpHint;
    use crate::smir::ir::types::VecWidth;

    if !x86_vector_integer_maddwd_shape_valid(&op.kind) {
        return (false, false, false);
    }
    match op.x86_hint {
        Some(X86OpHint::SseOp { .. }) => (false, false, false),
        Some(X86OpHint::VexOp { width, .. }) => (true, width == VecWidth::V256, false),
        Some(X86OpHint::EvexOp { width, .. }) => (false, false, width != VecWidth::V512),
        _ => (false, false, false),
    }
}

/// Whether any executable (non-exit) block contains an admitted native vector
/// operation. This controls vector-state marshalling in the entry trampoline.
pub fn uses_x86_native_vectors_excluding(
    func: &crate::smir::ir::SmirFunction,
    excluded: &std::collections::HashMap<crate::smir::ir::types::BlockId, u64>,
) -> bool {
    func.blocks
        .iter()
        .filter(|block| !excluded.contains_key(&block.id))
        .flat_map(|block| &block.ops)
        .any(|op| x86_native_vector_smir_op(op) || x86_jit_vector_mem_shape_valid(&op.kind))
}

/// Return `(AES-NI, VAES, AVX-512VL)` requirements contributed by an admitted
/// `X86Aes` operation. Low-register 128/256-bit rounds are re-encoded with VEX;
/// high registers require EVEX.VL, while 512-bit rounds use EVEX without VL.
fn x86_aes_feature_requirements(op: &crate::smir::ir::ops::OpKind) -> (bool, bool, bool) {
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::{ArchReg, VReg, VecWidth, X86AesOp, X86Reg};

    let OpKind::X86Aes {
        dst,
        src1,
        src2,
        width,
        op,
        ..
    } = op
    else {
        return (false, false, false);
    };
    match op {
        X86AesOp::InvMixColumns | X86AesOp::KeygenAssist => (true, false, false),
        X86AesOp::Enc | X86AesOp::EncLast | X86AesOp::Dec | X86AesOp::DecLast => {
            let high_vector = |reg: &VReg| {
                matches!(
                    reg,
                    VReg::Arch(ArchReg::X86(
                        X86Reg::Xmm(16..=31) | X86Reg::Ymm(16..=31) | X86Reg::Zmm(16..=31)
                    ))
                )
            };
            let needs_vl = *width != VecWidth::V512
                && (high_vector(dst)
                    || high_vector(src1)
                    || src2.is_some_and(|reg| high_vector(&reg)));
            (false, true, needs_vl)
        }
    }
}

fn x86_sha512_feature_required(op: &crate::smir::ir::ops::OpKind) -> bool {
    use crate::smir::ir::ops::OpKind;

    matches!(
        op,
        OpKind::X86Sha512Msg1 { .. }
            | OpKind::X86Sha512Msg2 { .. }
            | OpKind::X86Sha512Rounds2 { .. }
    )
}

fn x86_sm3_feature_required(op: &crate::smir::ir::ops::OpKind) -> bool {
    use crate::smir::ir::ops::OpKind;

    matches!(
        op,
        OpKind::X86Sm3Msg1 { .. } | OpKind::X86Sm3Msg2 { .. } | OpKind::X86Sm3Rounds2 { .. }
    )
}

fn x86_sm4_feature_required(op: &crate::smir::ir::ops::OpKind) -> bool {
    matches!(op, crate::smir::ir::ops::OpKind::X86Sm4 { .. })
}

/// Verify that this host can execute every admitted vector opcode in `func`.
/// The trampoline itself uses 512-bit VMOVDQU64 and 64-bit KMOVQ, so AVX-512F
/// and AVX-512BW are unconditional requirements for every vector region.
pub fn x86_native_vector_features_supported_excluding(
    func: &crate::smir::ir::SmirFunction,
    excluded: &std::collections::HashMap<crate::smir::ir::types::BlockId, u64>,
) -> bool {
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::VecWidth;

    let mut any = false;
    let mut needs_vl = false;
    let mut needs_vbmi = false;
    let mut needs_vbmi2 = false;
    let mut needs_bitalg = false;
    let mut needs_vpopcntdq = false;
    let mut needs_vnni = false;
    let mut needs_ifma = false;
    let mut needs_bf16 = false;
    let mut needs_cd = false;
    let mut needs_fp16 = false;
    let mut needs_aes = false;
    let mut needs_vaes = false;
    let mut needs_sha512 = false;
    let mut needs_sm3 = false;
    let mut needs_sm4 = false;
    let mut needs_shift_avx = false;
    let mut needs_shift_avx2 = false;
    let mut needs_logic_avx = false;
    let mut needs_logic_avx2 = false;
    let mut needs_dq = false;
    let mut needs_int_arith_avx = false;
    let mut needs_int_arith_avx2 = false;
    let mut needs_mul_sse41 = false;
    let mut needs_mul_avx = false;
    let mut needs_mul_avx2 = false;
    let mut needs_mul_dq = false;
    let mut needs_abs_ssse3 = false;
    let mut needs_abs_avx = false;
    let mut needs_abs_avx2 = false;
    let mut needs_cmp_sse41 = false;
    let mut needs_cmp_sse42 = false;
    let mut needs_cmp_avx = false;
    let mut needs_cmp_avx2 = false;
    let mut needs_interleave_avx = false;
    let mut needs_interleave_avx2 = false;
    let mut needs_pack_sse41 = false;
    let mut needs_pack_avx = false;
    let mut needs_pack_avx2 = false;
    let mut needs_byte_shuffle_ssse3 = false;
    let mut needs_byte_shuffle_avx = false;
    let mut needs_byte_shuffle_avx2 = false;
    let mut needs_horizontal_ssse3 = false;
    let mut needs_horizontal_avx = false;
    let mut needs_horizontal_avx2 = false;
    let mut needs_mul_shift_ssse3 = false;
    let mut needs_mul_shift_avx = false;
    let mut needs_mul_shift_avx2 = false;
    let mut needs_average_avx = false;
    let mut needs_average_avx2 = false;
    let mut needs_sign_ssse3 = false;
    let mut needs_sign_avx = false;
    let mut needs_sign_avx2 = false;
    let mut needs_minmax_sse41 = false;
    let mut needs_minmax_avx = false;
    let mut needs_minmax_avx2 = false;
    let mut needs_sad_bytes_avx = false;
    let mut needs_sad_bytes_avx2 = false;
    let mut needs_phminposuw_sse41 = false;
    let mut needs_phminposuw_avx = false;
    let mut needs_mov_mask_avx = false;
    let mut needs_mov_mask_avx2 = false;
    let mut needs_mpsadbw_sse41 = false;
    let mut needs_mpsadbw_avx = false;
    let mut needs_mpsadbw_avx2 = false;
    let mut needs_maddubs_ssse3 = false;
    let mut needs_maddubs_avx = false;
    let mut needs_maddubs_avx2 = false;
    let mut needs_maddwd_avx = false;
    let mut needs_maddwd_avx2 = false;

    for op in func
        .blocks
        .iter()
        .filter(|block| !excluded.contains_key(&block.id))
        .flat_map(|block| &block.ops)
        .filter(|op| x86_native_vector_smir_op(op) || x86_jit_vector_mem_shape_valid(&op.kind))
    {
        any = true;
        let kind = &op.kind;
        let width = match kind {
            OpKind::VMov { width, .. }
            | OpKind::VAnd { width, .. }
            | OpKind::VAndNot { width, .. }
            | OpKind::VOr { width, .. }
            | OpKind::VXor { width, .. }
            | OpKind::VPopcnt { width, .. }
            | OpKind::VShuffleBitQM { width, .. }
            | OpKind::VConflict { width, .. }
            | OpKind::VLeadingZeros { width, .. }
            | OpKind::X86PermuteBytesWords { width, .. }
            | OpKind::VCompress { width, .. }
            | OpKind::VExpand { width, .. }
            | OpKind::X86NarrowInt { width, .. }
            | OpKind::X86Aes { width, .. }
            | OpKind::X86PackedShiftImm { width, .. }
            | OpKind::X86PackedShift { width, .. }
            | OpKind::VSadBytes { width, .. }
            | OpKind::VMpsadbw { width, .. }
            | OpKind::VDotProduct { width, .. }
            | OpKind::VDotProductBF16 { width, .. }
            | OpKind::VCvtFP32ToBF16 { width, .. }
            | OpKind::VFP16Arith { width, .. }
            | OpKind::VMultiplyAdd52 { width, .. }
            | OpKind::X86PackedShiftVariable { width, .. }
            | OpKind::X86PackedRotate { width, .. }
            | OpKind::X86TernaryLogic { width, .. }
            | OpKind::X86PackedFunnelShift { width, .. }
            | OpKind::X86MultiShiftQB { width, .. }
            | OpKind::VLoad { width, .. }
            | OpKind::VStore { width, .. } => *width,
            OpKind::X86Phminposuw { .. } => VecWidth::V128,
            OpKind::X86MovdQ { .. } => VecWidth::V128,
            OpKind::X86MovMask { elem, lanes, .. } => x86_vector_width_from_lanes(*elem, *lanes)
                .expect("admitted MOVMSK operation has exact lanes"),
            OpKind::VAdd { elem, lanes, .. }
            | OpKind::VSub { elem, lanes, .. }
            | OpKind::VAddSubSat { elem, lanes, .. }
            | OpKind::VMul { elem, lanes, .. }
            | OpKind::VUnary { elem, lanes, .. }
            | OpKind::VCmp { elem, lanes, .. }
            | OpKind::VInterleave { elem, lanes, .. }
            | OpKind::VHorizontalBin { elem, lanes, .. }
            | OpKind::VMulShiftSat {
                src_elem: elem,
                lanes,
                ..
            }
            | OpKind::VLane { elem, lanes, .. } => x86_vector_width_from_lanes(*elem, *lanes)
                .expect("admitted integer vector operation has exact lanes"),
            OpKind::VPackSat {
                src_elem,
                src_lanes,
                ..
            } => x86_vector_width_from_lanes(*src_elem, *src_lanes)
                .expect("admitted integer pack has exact lanes"),
            OpKind::VByteShuffle { lanes, .. } => {
                x86_vector_width_from_lanes(crate::smir::ir::types::VecElementType::I8, *lanes)
                    .expect("admitted byte shuffle has exact lanes")
            }
            OpKind::X86Sha512Msg1 { .. }
            | OpKind::X86Sha512Msg2 { .. }
            | OpKind::X86Sha512Rounds2 { .. } => VecWidth::V256,
            OpKind::X86Sm3Msg1 { .. }
            | OpKind::X86Sm3Msg2 { .. }
            | OpKind::X86Sm3Rounds2 { .. } => VecWidth::V128,
            OpKind::X86Sm4 { width, .. } => *width,
            _ => unreachable!("filtered to native vector operations"),
        };
        let (aes, vaes, aes_vl) = x86_aes_feature_requirements(kind);
        let (shift_avx, shift_avx2, shift_vl) = x86_packed_shift_imm_feature_requirements(kind);
        let (count_avx, count_avx2, count_vl) = x86_packed_shift_feature_requirements(kind);
        let (logic_avx, logic_avx2, logic_dq, logic_vl) = x86_vector_logic_feature_requirements(op);
        let (int_arith_avx, int_arith_avx2, int_arith_vl) =
            x86_vector_integer_arithmetic_feature_requirements(op);
        let (mul_sse41, mul_avx, mul_avx2, mul_dq, mul_vl) =
            x86_vector_integer_multiply_feature_requirements(op);
        let (abs_ssse3, abs_avx, abs_avx2, abs_vl) =
            x86_vector_integer_abs_feature_requirements(op);
        let (cmp_sse41, cmp_sse42, cmp_avx, cmp_avx2) =
            x86_vector_integer_compare_feature_requirements(op);
        let (interleave_avx, interleave_avx2, interleave_vl) =
            x86_vector_integer_interleave_feature_requirements(op);
        let (pack_sse41, pack_avx, pack_avx2, pack_vl) =
            x86_vector_integer_pack_feature_requirements(op);
        let (byte_shuffle_ssse3, byte_shuffle_avx, byte_shuffle_avx2, byte_shuffle_vl) =
            x86_vector_byte_shuffle_feature_requirements(op);
        let (horizontal_ssse3, horizontal_avx, horizontal_avx2) =
            x86_vector_integer_horizontal_feature_requirements(op);
        let (mul_shift_ssse3, mul_shift_avx, mul_shift_avx2, mul_shift_vl) =
            x86_vector_integer_mul_shift_feature_requirements(op);
        let (average_avx, average_avx2, average_vl) =
            x86_vector_integer_average_feature_requirements(op);
        let (sign_ssse3, sign_avx, sign_avx2) = x86_vector_integer_sign_feature_requirements(op);
        let (minmax_sse41, minmax_avx, minmax_avx2, minmax_vl) =
            x86_vector_integer_minmax_feature_requirements(op);
        let (sad_bytes_avx, sad_bytes_avx2, sad_bytes_vl) =
            x86_vector_sad_bytes_feature_requirements(op);
        let (phminposuw_sse41, phminposuw_avx) = x86_phminposuw_feature_requirements(op);
        let (mov_mask_avx, mov_mask_avx2) = x86_mov_mask_feature_requirements(op);
        let (mpsadbw_sse41, mpsadbw_avx, mpsadbw_avx2) =
            x86_vector_mpsadbw_feature_requirements(op);
        let (maddubs_ssse3, maddubs_avx, maddubs_avx2, maddubs_vl) =
            x86_vector_integer_maddubs_feature_requirements(op);
        let (maddwd_avx, maddwd_avx2, maddwd_vl) =
            x86_vector_integer_maddwd_feature_requirements(op);
        needs_vl |= match kind {
            OpKind::VMov { .. } => x86_vector_move_needs_vl(op),
            OpKind::VAnd { .. }
            | OpKind::VAndNot { .. }
            | OpKind::VOr { .. }
            | OpKind::VXor { .. } => logic_vl,
            OpKind::VAdd { .. } | OpKind::VSub { .. } | OpKind::VAddSubSat { .. } => int_arith_vl,
            OpKind::VMul { .. } => mul_vl,
            OpKind::VUnary { .. } => abs_vl,
            OpKind::VCmp { .. } => false,
            OpKind::VInterleave { .. } => interleave_vl,
            OpKind::VPackSat { .. } => pack_vl,
            OpKind::VByteShuffle { .. } => byte_shuffle_vl,
            OpKind::VHorizontalBin { .. } => false,
            OpKind::VMulShiftSat { .. } => mul_shift_vl,
            OpKind::VLane {
                op: crate::smir::ir::types::VLaneOp::AvgRnd,
                ..
            } => average_vl,
            OpKind::VLane {
                op: crate::smir::ir::types::VLaneOp::Sign,
                ..
            } => false,
            OpKind::VLane {
                op: crate::smir::ir::types::VLaneOp::Min | crate::smir::ir::types::VLaneOp::Max,
                ..
            } => minmax_vl,
            OpKind::VSadBytes { .. } => sad_bytes_vl,
            OpKind::X86Phminposuw { .. } => false,
            OpKind::X86MovdQ { .. } => false,
            OpKind::X86MovMask { .. } => false,
            OpKind::VMpsadbw { .. } => false,
            OpKind::VDotProduct { .. } if x86_vector_integer_maddubs_shape_valid(kind) => {
                maddubs_vl
            }
            OpKind::VDotProduct { .. } if x86_vector_integer_maddwd_shape_valid(kind) => maddwd_vl,
            OpKind::X86Aes { .. } => aes_vl,
            OpKind::X86PackedShiftImm { .. } => shift_vl,
            OpKind::X86PackedShift { .. } => count_vl,
            OpKind::X86Sha512Msg1 { .. }
            | OpKind::X86Sha512Msg2 { .. }
            | OpKind::X86Sha512Rounds2 { .. }
            | OpKind::X86Sm3Msg1 { .. }
            | OpKind::X86Sm3Msg2 { .. }
            | OpKind::X86Sm3Rounds2 { .. }
            | OpKind::X86Sm4 { .. }
            | OpKind::VLoad { .. }
            | OpKind::VStore { .. } => false,
            _ => width != VecWidth::V512,
        };
        needs_vbmi |= matches!(
            kind,
            OpKind::X86MultiShiftQB { .. } | OpKind::X86PermuteBytesWords { .. }
        );
        needs_vbmi2 |= matches!(kind, OpKind::X86PackedFunnelShift { .. })
            || matches!(
                kind,
                OpKind::VCompress {
                    elem: crate::smir::ir::types::VecElementType::I8
                        | crate::smir::ir::types::VecElementType::I16,
                    ..
                } | OpKind::VExpand {
                    elem: crate::smir::ir::types::VecElementType::I8
                        | crate::smir::ir::types::VecElementType::I16,
                    ..
                }
            );
        if let OpKind::VPopcnt { elem, .. } = kind {
            needs_bitalg |= matches!(
                elem,
                crate::smir::ir::types::VecElementType::I8
                    | crate::smir::ir::types::VecElementType::I16
            );
            needs_vpopcntdq |= matches!(
                elem,
                crate::smir::ir::types::VecElementType::I32
                    | crate::smir::ir::types::VecElementType::I64
            );
        }
        needs_bitalg |= matches!(kind, OpKind::VShuffleBitQM { .. });
        needs_vnni |= matches!(kind, OpKind::VDotProduct { .. })
            && !x86_vector_integer_maddubs_shape_valid(kind)
            && !x86_vector_integer_maddwd_shape_valid(kind);
        needs_ifma |= matches!(kind, OpKind::VMultiplyAdd52 { .. });
        needs_bf16 |= matches!(
            kind,
            OpKind::VDotProductBF16 { .. } | OpKind::VCvtFP32ToBF16 { .. }
        );
        needs_cd |= matches!(
            kind,
            OpKind::VConflict { .. } | OpKind::VLeadingZeros { .. }
        );
        needs_fp16 |= matches!(kind, OpKind::VFP16Arith { .. });
        needs_aes |= aes;
        needs_vaes |= vaes;
        needs_sha512 |= x86_sha512_feature_required(kind);
        needs_sm3 |= x86_sm3_feature_required(kind);
        needs_sm4 |= x86_sm4_feature_required(kind);
        needs_shift_avx |= shift_avx || count_avx;
        needs_shift_avx2 |= shift_avx2 || count_avx2;
        needs_logic_avx |= logic_avx;
        needs_logic_avx2 |= logic_avx2;
        needs_dq |= logic_dq;
        needs_int_arith_avx |= int_arith_avx;
        needs_int_arith_avx2 |= int_arith_avx2;
        needs_mul_sse41 |= mul_sse41;
        needs_mul_avx |= mul_avx;
        needs_mul_avx2 |= mul_avx2;
        needs_mul_dq |= mul_dq;
        needs_abs_ssse3 |= abs_ssse3;
        needs_abs_avx |= abs_avx;
        needs_abs_avx2 |= abs_avx2;
        needs_cmp_sse41 |= cmp_sse41;
        needs_cmp_sse42 |= cmp_sse42;
        needs_cmp_avx |= cmp_avx;
        needs_cmp_avx2 |= cmp_avx2;
        needs_interleave_avx |= interleave_avx;
        needs_interleave_avx2 |= interleave_avx2;
        needs_pack_sse41 |= pack_sse41;
        needs_pack_avx |= pack_avx;
        needs_pack_avx2 |= pack_avx2;
        needs_byte_shuffle_ssse3 |= byte_shuffle_ssse3;
        needs_byte_shuffle_avx |= byte_shuffle_avx;
        needs_byte_shuffle_avx2 |= byte_shuffle_avx2;
        needs_horizontal_ssse3 |= horizontal_ssse3;
        needs_horizontal_avx |= horizontal_avx;
        needs_horizontal_avx2 |= horizontal_avx2;
        needs_mul_shift_ssse3 |= mul_shift_ssse3;
        needs_mul_shift_avx |= mul_shift_avx;
        needs_mul_shift_avx2 |= mul_shift_avx2;
        needs_average_avx |= average_avx;
        needs_average_avx2 |= average_avx2;
        needs_sign_ssse3 |= sign_ssse3;
        needs_sign_avx |= sign_avx;
        needs_sign_avx2 |= sign_avx2;
        needs_minmax_sse41 |= minmax_sse41;
        needs_minmax_avx |= minmax_avx;
        needs_minmax_avx2 |= minmax_avx2;
        needs_sad_bytes_avx |= sad_bytes_avx;
        needs_sad_bytes_avx2 |= sad_bytes_avx2;
        needs_phminposuw_sse41 |= phminposuw_sse41;
        needs_phminposuw_avx |= phminposuw_avx;
        needs_mov_mask_avx |= mov_mask_avx;
        needs_mov_mask_avx2 |= mov_mask_avx2;
        needs_mpsadbw_sse41 |= mpsadbw_sse41;
        needs_mpsadbw_avx |= mpsadbw_avx;
        needs_mpsadbw_avx2 |= mpsadbw_avx2;
        needs_maddubs_ssse3 |= maddubs_ssse3;
        needs_maddubs_avx |= maddubs_avx;
        needs_maddubs_avx2 |= maddubs_avx2;
        needs_maddwd_avx |= maddwd_avx;
        needs_maddwd_avx2 |= maddwd_avx2;
    }

    if !any {
        return true;
    }

    #[cfg(target_arch = "x86_64")]
    {
        std::is_x86_feature_detected!("avx512f")
            && std::is_x86_feature_detected!("avx512bw")
            && (!needs_vl || std::is_x86_feature_detected!("avx512vl"))
            && (!needs_vbmi || std::is_x86_feature_detected!("avx512vbmi"))
            && (!needs_vbmi2 || std::is_x86_feature_detected!("avx512vbmi2"))
            && (!needs_bitalg || std::is_x86_feature_detected!("avx512bitalg"))
            && (!needs_vpopcntdq || std::is_x86_feature_detected!("avx512vpopcntdq"))
            && (!needs_vnni || std::is_x86_feature_detected!("avx512vnni"))
            && (!needs_ifma || std::is_x86_feature_detected!("avx512ifma"))
            && (!needs_bf16 || std::is_x86_feature_detected!("avx512bf16"))
            && (!needs_cd || std::is_x86_feature_detected!("avx512cd"))
            && (!needs_fp16 || std::is_x86_feature_detected!("avx512fp16"))
            && (!(needs_dq || needs_mul_dq) || std::is_x86_feature_detected!("avx512dq"))
            && (!needs_aes || std::is_x86_feature_detected!("aes"))
            && (!needs_vaes || std::is_x86_feature_detected!("vaes"))
            && (!needs_sha512
                || (std::is_x86_feature_detected!("avx2")
                    && std::is_x86_feature_detected!("sha512")))
            && (!needs_sm3
                || (std::is_x86_feature_detected!("avx") && std::is_x86_feature_detected!("sm3")))
            && (!needs_sm4
                || (std::is_x86_feature_detected!("avx2") && std::is_x86_feature_detected!("sm4")))
            && (!needs_shift_avx || std::is_x86_feature_detected!("avx"))
            && (!needs_shift_avx2 || std::is_x86_feature_detected!("avx2"))
            && (!needs_logic_avx || std::is_x86_feature_detected!("avx"))
            && (!needs_logic_avx2 || std::is_x86_feature_detected!("avx2"))
            && (!needs_int_arith_avx || std::is_x86_feature_detected!("avx"))
            && (!needs_int_arith_avx2 || std::is_x86_feature_detected!("avx2"))
            && (!needs_mul_sse41 || std::is_x86_feature_detected!("sse4.1"))
            && (!needs_mul_avx || std::is_x86_feature_detected!("avx"))
            && (!needs_mul_avx2 || std::is_x86_feature_detected!("avx2"))
            && (!needs_abs_ssse3 || std::is_x86_feature_detected!("ssse3"))
            && (!needs_abs_avx || std::is_x86_feature_detected!("avx"))
            && (!needs_abs_avx2 || std::is_x86_feature_detected!("avx2"))
            && (!needs_cmp_sse41 || std::is_x86_feature_detected!("sse4.1"))
            && (!needs_cmp_sse42 || std::is_x86_feature_detected!("sse4.2"))
            && (!needs_cmp_avx || std::is_x86_feature_detected!("avx"))
            && (!needs_cmp_avx2 || std::is_x86_feature_detected!("avx2"))
            && (!needs_interleave_avx || std::is_x86_feature_detected!("avx"))
            && (!needs_interleave_avx2 || std::is_x86_feature_detected!("avx2"))
            && (!needs_pack_sse41 || std::is_x86_feature_detected!("sse4.1"))
            && (!needs_pack_avx || std::is_x86_feature_detected!("avx"))
            && (!needs_pack_avx2 || std::is_x86_feature_detected!("avx2"))
            && (!needs_byte_shuffle_ssse3 || std::is_x86_feature_detected!("ssse3"))
            && (!needs_byte_shuffle_avx || std::is_x86_feature_detected!("avx"))
            && (!needs_byte_shuffle_avx2 || std::is_x86_feature_detected!("avx2"))
            && (!needs_horizontal_ssse3 || std::is_x86_feature_detected!("ssse3"))
            && (!needs_horizontal_avx || std::is_x86_feature_detected!("avx"))
            && (!needs_horizontal_avx2 || std::is_x86_feature_detected!("avx2"))
            && (!needs_mul_shift_ssse3 || std::is_x86_feature_detected!("ssse3"))
            && (!needs_mul_shift_avx || std::is_x86_feature_detected!("avx"))
            && (!needs_mul_shift_avx2 || std::is_x86_feature_detected!("avx2"))
            && (!needs_average_avx || std::is_x86_feature_detected!("avx"))
            && (!needs_average_avx2 || std::is_x86_feature_detected!("avx2"))
            && (!needs_sign_ssse3 || std::is_x86_feature_detected!("ssse3"))
            && (!needs_sign_avx || std::is_x86_feature_detected!("avx"))
            && (!needs_sign_avx2 || std::is_x86_feature_detected!("avx2"))
            && (!needs_minmax_sse41 || std::is_x86_feature_detected!("sse4.1"))
            && (!needs_minmax_avx || std::is_x86_feature_detected!("avx"))
            && (!needs_minmax_avx2 || std::is_x86_feature_detected!("avx2"))
            && (!needs_sad_bytes_avx || std::is_x86_feature_detected!("avx"))
            && (!needs_sad_bytes_avx2 || std::is_x86_feature_detected!("avx2"))
            && (!needs_phminposuw_sse41 || std::is_x86_feature_detected!("sse4.1"))
            && (!needs_phminposuw_avx || std::is_x86_feature_detected!("avx"))
            && (!needs_mov_mask_avx || std::is_x86_feature_detected!("avx"))
            && (!needs_mov_mask_avx2 || std::is_x86_feature_detected!("avx2"))
            && (!needs_mpsadbw_sse41 || std::is_x86_feature_detected!("sse4.1"))
            && (!needs_mpsadbw_avx || std::is_x86_feature_detected!("avx"))
            && (!needs_mpsadbw_avx2 || std::is_x86_feature_detected!("avx2"))
            && (!needs_maddubs_ssse3 || std::is_x86_feature_detected!("ssse3"))
            && (!needs_maddubs_avx || std::is_x86_feature_detected!("avx"))
            && (!needs_maddubs_avx2 || std::is_x86_feature_detected!("avx2"))
            && (!needs_maddwd_avx || std::is_x86_feature_detected!("avx"))
            && (!needs_maddwd_avx2 || std::is_x86_feature_detected!("avx2"))
    }

    #[cfg(not(target_arch = "x86_64"))]
    {
        let _ = (
            needs_vl,
            needs_vbmi,
            needs_vbmi2,
            needs_bitalg,
            needs_vpopcntdq,
            needs_vnni,
            needs_ifma,
            needs_bf16,
            needs_cd,
            needs_fp16,
            needs_aes,
            needs_vaes,
            needs_sha512,
            needs_sm3,
            needs_sm4,
            needs_shift_avx,
            needs_shift_avx2,
            needs_logic_avx,
            needs_logic_avx2,
            needs_dq,
            needs_int_arith_avx,
            needs_int_arith_avx2,
            needs_mul_sse41,
            needs_mul_avx,
            needs_mul_avx2,
            needs_mul_dq,
            needs_abs_ssse3,
            needs_abs_avx,
            needs_abs_avx2,
            needs_cmp_sse41,
            needs_cmp_sse42,
            needs_cmp_avx,
            needs_cmp_avx2,
            needs_interleave_avx,
            needs_interleave_avx2,
            needs_pack_sse41,
            needs_pack_avx,
            needs_pack_avx2,
            needs_byte_shuffle_ssse3,
            needs_byte_shuffle_avx,
            needs_byte_shuffle_avx2,
            needs_horizontal_ssse3,
            needs_horizontal_avx,
            needs_horizontal_avx2,
            needs_average_avx,
            needs_average_avx2,
            needs_sign_ssse3,
            needs_sign_avx,
            needs_sign_avx2,
            needs_minmax_sse41,
            needs_minmax_avx,
            needs_minmax_avx2,
            needs_sad_bytes_avx,
            needs_sad_bytes_avx2,
            needs_phminposuw_sse41,
            needs_phminposuw_avx,
            needs_mov_mask_avx,
            needs_mov_mask_avx2,
            needs_mpsadbw_sse41,
            needs_mpsadbw_avx,
            needs_mpsadbw_avx2,
            needs_maddubs_ssse3,
            needs_maddubs_avx,
            needs_maddubs_avx2,
            needs_maddwd_avx,
            needs_maddwd_avx2,
        );
        false
    }
}

fn x86_flag_live_in(
    func: &crate::smir::ir::SmirFunction,
    excluded: &std::collections::HashMap<crate::smir::ir::types::BlockId, u64>,
) -> std::collections::HashMap<crate::smir::ir::types::BlockId, crate::smir::ir::flags::FlagSet> {
    use crate::smir::ir::flags::FlagSet;

    let mut live_in: std::collections::HashMap<_, _> = func
        .blocks
        .iter()
        .filter(|b| !excluded.contains_key(&b.id))
        .map(|b| (b.id, FlagSet::EMPTY))
        .collect();

    let mut changed = true;
    while changed {
        changed = false;
        for block in func
            .blocks
            .iter()
            .rev()
            .filter(|b| !excluded.contains_key(&b.id))
        {
            let mut live = x86_block_flag_live_out(block, excluded, &live_in);
            for op in block.ops.iter().rev() {
                live = x86_flags_before_op(&op.kind, live);
            }
            if live_in.get(&block.id).copied() != Some(live) {
                live_in.insert(block.id, live);
                changed = true;
            }
        }
    }

    live_in
}

fn x86_block_flag_live_out(
    block: &crate::smir::ir::SmirBlock,
    excluded: &std::collections::HashMap<crate::smir::ir::types::BlockId, u64>,
    live_in: &std::collections::HashMap<
        crate::smir::ir::types::BlockId,
        crate::smir::ir::flags::FlagSet,
    >,
) -> crate::smir::ir::flags::FlagSet {
    use crate::smir::ir::flags::FlagSet;

    let successors = block.terminator.successors();
    if successors.is_empty() {
        return FlagSet::ALL_X86;
    }

    let mut live = FlagSet::EMPTY;
    for succ in successors {
        live = live.union(if excluded.contains_key(&succ) {
            FlagSet::ALL_X86
        } else {
            live_in.get(&succ).copied().unwrap_or(FlagSet::ALL_X86)
        });
    }
    live
}

fn x86_flags_before_op(
    op: &crate::smir::ir::ops::OpKind,
    live_after: crate::smir::ir::flags::FlagSet,
) -> crate::smir::ir::flags::FlagSet {
    live_after
        .difference(x86_flag_defs(op))
        .union(x86_flag_uses(op))
}

fn x86_flag_uses(op: &crate::smir::ir::ops::OpKind) -> crate::smir::ir::flags::FlagSet {
    use crate::smir::ir::flags::{FlagSet, FlagState};
    use crate::smir::ir::ops::{OpKind, X86AdxKind};

    match op {
        OpKind::TestCondition { cond, .. }
        | OpKind::SetCC { cond, .. }
        | OpKind::CMove { cond, .. } => FlagState::required_flags(*cond),
        OpKind::Adc { .. } | OpKind::Sbb { .. } | OpKind::Rcl { .. } | OpKind::Rcr { .. } => {
            FlagSet::CF
        }
        OpKind::X86Adx { kind, .. } => match kind {
            X86AdxKind::Adcx => FlagSet::CF,
            X86AdxKind::Adox => FlagSet::OF,
        },
        OpKind::CmcCF => FlagSet::CF,
        _ => FlagSet::EMPTY,
    }
}

fn x86_flag_defs(op: &crate::smir::ir::ops::OpKind) -> crate::smir::ir::flags::FlagSet {
    use crate::smir::ir::flags::FlagSet;
    use crate::smir::ir::ops::OpKind;

    match op {
        OpKind::Add { flags, .. }
        | OpKind::Sub { flags, .. }
        | OpKind::Adc { flags, .. }
        | OpKind::Sbb { flags, .. }
        | OpKind::Neg { flags, .. }
        | OpKind::Inc { flags, .. }
        | OpKind::Dec { flags, .. }
        | OpKind::MulU { flags, .. }
        | OpKind::MulS { flags, .. }
        | OpKind::And { flags, .. }
        | OpKind::Or { flags, .. }
        | OpKind::Xor { flags, .. }
        | OpKind::AndNot { flags, .. }
        | OpKind::Shl { flags, .. }
        | OpKind::Shr { flags, .. }
        | OpKind::Sar { flags, .. }
        | OpKind::Shld { flags, .. }
        | OpKind::Shrd { flags, .. }
        | OpKind::X86NddDoubleShift { flags, .. }
        | OpKind::Rol { flags, .. }
        | OpKind::Ror { flags, .. }
        | OpKind::Rcl { flags, .. }
        | OpKind::Rcr { flags, .. }
        | OpKind::Bsf { flags, .. }
        | OpKind::Bsr { flags, .. }
        | OpKind::Bextr { flags, .. }
        | OpKind::Bzhi { flags, .. }
        | OpKind::X86Bls { flags, .. }
        | OpKind::X86Adx { flags, .. }
        | OpKind::X86Count { flags, .. } => flags.as_set(),
        OpKind::Cmp { .. } | OpKind::Test { .. } => FlagSet::ALL_X86,
        OpKind::X86Random { .. } => FlagSet::ALL_X86,
        OpKind::Bt { .. }
        | OpKind::Bts { .. }
        | OpKind::Btr { .. }
        | OpKind::Btc { .. }
        | OpKind::SetCF { .. }
        | OpKind::CmcCF => FlagSet::CF,
        _ => FlagSet::EMPTY,
    }
}

fn x86_block_preserves_live_flags(
    block: &crate::smir::ir::SmirBlock,
    mut live: crate::smir::ir::flags::FlagSet,
    preserved_clobber_exceptions: &std::collections::HashSet<usize>,
) -> bool {
    use crate::smir::ir::flags::{FlagSet, FlagUpdate};
    use crate::smir::ir::ops::{OpKind, X86AdxKind};

    for (index, op) in block.ops.iter().enumerate().rev() {
        if let OpKind::X86Adx {
            kind,
            flags: FlagUpdate::None,
            ..
        } = &op.kind
        {
            let native_output = match kind {
                X86AdxKind::Adcx => FlagSet::CF,
                X86AdxKind::Adox => FlagSet::OF,
            };
            if !live.intersection(native_output).is_empty() {
                return false;
            }
        }
        if !preserved_clobber_exceptions.contains(&index)
            && x86_native_op_would_clobber_preserved_flags(&op.kind)
            && !live.is_empty()
        {
            return false;
        }
        live = x86_flags_before_op(&op.kind, live);
    }
    true
}

fn x86_native_identity_gpr(reg: &crate::smir::ir::types::VReg) -> bool {
    use crate::smir::ir::types::{ArchReg, VReg};

    matches!(reg, VReg::Arch(ArchReg::X86(x86)) if x86.gpr_index().is_some_and(|index| index <= 15 && !matches!(index, 4 | 5)))
}

pub(crate) fn x86_state_backed_stack_mov_valid(op: &crate::smir::ir::ops::OpKind) -> bool {
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::{ArchReg, OpWidth, SrcOperand, VReg};

    let gpr_index = |reg: &VReg| match reg {
        VReg::Arch(ArchReg::X86(x86)) => x86.gpr_index(),
        _ => None,
    };
    let is_stack = |reg: &VReg| gpr_index(reg).is_some_and(|index| matches!(index, 4 | 5));

    matches!(
        op,
        OpKind::Mov {
            dst,
            src: SrcOperand::Reg(src),
            width: OpWidth::W8 | OpWidth::W16 | OpWidth::W32 | OpWidth::W64,
        } if gpr_index(dst).is_some()
            && gpr_index(src).is_some()
            && (is_stack(dst) || is_stack(src))
    ) || matches!(
        op,
        OpKind::Mov {
            dst,
            src: SrcOperand::Imm(_) | SrcOperand::Imm64(_),
            width: OpWidth::W8 | OpWidth::W16 | OpWidth::W32 | OpWidth::W64,
        } if is_stack(dst)
    )
}

pub(crate) fn x86_state_backed_stack_alu_valid(op: &crate::smir::ir::ops::OpKind) -> bool {
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::{ArchReg, OpWidth, SrcOperand, VReg};

    let gpr_index = |reg: &VReg| match reg {
        VReg::Arch(ArchReg::X86(x86)) => x86.gpr_index(),
        _ => None,
    };
    let is_stack = |reg: &VReg| gpr_index(reg).is_some_and(|index| matches!(index, 4 | 5));
    let valid = |dst: &VReg, src1: &VReg, src2: &SrcOperand, width: &OpWidth| {
        matches!(
            width,
            OpWidth::W8 | OpWidth::W16 | OpWidth::W32 | OpWidth::W64
        ) && gpr_index(dst).is_some()
            && gpr_index(src1).is_some()
            && match src2 {
                SrcOperand::Reg(src2) => gpr_index(src2).is_some(),
                SrcOperand::Imm(value) => *width != OpWidth::W64 || i32::try_from(*value).is_ok(),
                _ => false,
            }
            && (is_stack(dst)
                || is_stack(src1)
                || matches!(src2, SrcOperand::Reg(src2) if is_stack(src2)))
    };

    match op {
        OpKind::Add {
            dst,
            src1,
            src2,
            width,
            flags,
        }
        | OpKind::Sub {
            dst,
            src1,
            src2,
            width,
            flags,
        } => {
            valid(dst, src1, src2, width)
                && matches!(
                    flags,
                    crate::smir::ir::flags::FlagUpdate::None
                        | crate::smir::ir::flags::FlagUpdate::All
                )
        }
        _ => false,
    }
}

fn x86_native_rdpid_gpr(reg: &crate::smir::ir::types::VReg) -> bool {
    use crate::smir::ir::types::{ArchReg, VReg};

    matches!(reg, VReg::Arch(ArchReg::X86(x86)) if x86.gpr_index().is_some_and(|index| index <= 31 && !matches!(index, 4 | 5)))
}

fn x86_random_shape_valid(op: &crate::smir::ir::ops::OpKind) -> bool {
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::OpWidth;

    matches!(
        op,
        OpKind::X86Random {
            dst,
            width: OpWidth::W16 | OpWidth::W32 | OpWidth::W64,
            ..
        } if x86_native_identity_gpr(dst)
    )
}

fn x86_xgetbv_shape_valid(op: &crate::smir::ir::ops::OpKind) -> bool {
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::{ArchReg, VReg, X86Reg};

    matches!(
        op,
        OpKind::X86XGetBv {
            dst_low: VReg::Arch(ArchReg::X86(X86Reg::Rax)),
            dst_high: VReg::Arch(ArchReg::X86(X86Reg::Rdx)),
            selector: VReg::Arch(ArchReg::X86(X86Reg::Rcx)),
        }
    )
}

fn x86_xsetbv_shape_valid(op: &crate::smir::ir::ops::OpKind) -> bool {
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::{ArchReg, VReg, X86Reg};

    matches!(
        op,
        OpKind::X86XSetBv {
            selector: VReg::Arch(ArchReg::X86(X86Reg::Rcx)),
            src_low: VReg::Arch(ArchReg::X86(X86Reg::Rax)),
            src_high: VReg::Arch(ArchReg::X86(X86Reg::Rdx)),
        }
    )
}

fn x86_crc32_shape_valid(op: &crate::smir::ir::ops::OpKind) -> bool {
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::OpWidth;

    matches!(
        op,
        OpKind::Crc32C {
            dst,
            crc,
            data,
            data_width: OpWidth::W8 | OpWidth::W16 | OpWidth::W32 | OpWidth::W64,
        } if dst == crc && x86_native_identity_gpr(dst) && x86_native_identity_gpr(data)
    )
}

fn x86_jit_mem_address_shape_valid(addr: &crate::smir::ir::types::Address) -> bool {
    use crate::smir::ir::types::{Address, ArchReg, VReg, X86Reg};

    let state_gpr =
        |reg: &VReg| matches!(reg, VReg::Arch(ArchReg::X86(x86)) if x86.gpr_index().is_some());
    let scale = |value: &u8| matches!(value, 1 | 2 | 4 | 8);
    match addr {
        Address::Direct(base) | Address::BaseOffset { base, .. } => state_gpr(base),
        Address::BaseIndexScale {
            base,
            index,
            scale: amount,
            ..
        } => base.as_ref().is_none_or(state_gpr) && state_gpr(index) && scale(amount),
        Address::PcRel { base, .. } => base.is_some(),
        Address::Absolute(_) => true,
        Address::SegmentRel {
            segment,
            base,
            index,
            scale: amount,
            ..
        } => {
            matches!(
                segment,
                VReg::Arch(ArchReg::X86(X86Reg::FsBase | X86Reg::GsBase))
            ) && base.as_ref().is_none_or(state_gpr)
                && index.as_ref().is_none_or(state_gpr)
                && scale(amount)
        }
        Address::GpRel { .. } => false,
    }
}

/// Admit only architectural vector moves whose register class exactly matches
/// the transfer width. Virtual vector temporaries remain ineligible because the
/// identity bridge has no stable state slot for them across an MMU helper call.
pub(crate) fn x86_jit_vector_mem_shape_valid(op: &crate::smir::ir::ops::OpKind) -> bool {
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::{ArchReg, VReg, VecWidth, X86Reg};

    let vector_matches_width = |reg: &VReg, width: VecWidth| {
        matches!(
            (reg, width),
            (
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(0..=31))),
                VecWidth::V128
            ) | (
                VReg::Arch(ArchReg::X86(X86Reg::Ymm(0..=31))),
                VecWidth::V256
            ) | (
                VReg::Arch(ArchReg::X86(X86Reg::Zmm(0..=31))),
                VecWidth::V512
            )
        )
    };

    match op {
        OpKind::VLoad { dst, addr, width } => {
            vector_matches_width(dst, *width) && x86_jit_mem_address_shape_valid(addr)
        }
        OpKind::VStore { src, addr, width } => {
            vector_matches_width(src, *width) && x86_jit_mem_address_shape_valid(addr)
        }
        _ => false,
    }
}

fn x86_binary_alu_shape(
    kind: &crate::smir::ir::ops::OpKind,
) -> Option<(
    u8,
    crate::smir::ir::types::VReg,
    crate::smir::ir::types::VReg,
    crate::smir::ir::types::SrcOperand,
    crate::smir::ir::types::OpWidth,
    crate::smir::ir::flags::FlagUpdate,
)> {
    use crate::smir::ir::ops::OpKind;

    match kind {
        OpKind::Add {
            dst,
            src1,
            src2,
            width,
            flags,
        } => Some((0, *dst, *src1, src2.clone(), *width, *flags)),
        OpKind::Or {
            dst,
            src1,
            src2,
            width,
            flags,
        } => Some((1, *dst, *src1, src2.clone(), *width, *flags)),
        OpKind::Adc {
            dst,
            src1,
            src2,
            width,
            flags,
        } => Some((2, *dst, *src1, src2.clone(), *width, *flags)),
        OpKind::Sbb {
            dst,
            src1,
            src2,
            width,
            flags,
        } => Some((3, *dst, *src1, src2.clone(), *width, *flags)),
        OpKind::And {
            dst,
            src1,
            src2,
            width,
            flags,
        } => Some((4, *dst, *src1, src2.clone(), *width, *flags)),
        OpKind::Sub {
            dst,
            src1,
            src2,
            width,
            flags,
        } => Some((5, *dst, *src1, src2.clone(), *width, *flags)),
        OpKind::Xor {
            dst,
            src1,
            src2,
            width,
            flags,
        } => Some((6, *dst, *src1, src2.clone(), *width, *flags)),
        _ => None,
    }
}

/// Validate the exact fault-precise scalar memory-destination sequence emitted
/// by the x86 lifter: `Load old; ALU result,old,source` without flags; `Store
/// result`; then the same ALU into a dead virtual with full flag updates. The
/// post-store replay is architecturally significant: a failing store must leave
/// the incoming flags unchanged.
pub(crate) fn x86_jit_mem_alu_rmw_sequence_len(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    virtual_definitions: &std::collections::HashMap<crate::smir::ir::types::VReg, usize>,
    virtual_uses: &std::collections::HashMap<crate::smir::ir::types::VReg, usize>,
) -> Option<usize> {
    use crate::smir::ir::flags::FlagUpdate;
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::{ArchReg, OpWidth, SignExtend, SrcOperand, VReg};

    if !allow_mem {
        return None;
    }
    let load = block.ops.get(index)?;
    let (old, addr, mem_width) = match &load.kind {
        OpKind::Load {
            dst: old @ VReg::Virtual(_),
            addr,
            width,
            sign: SignExtend::Zero,
        } => (*old, addr, *width),
        _ => return None,
    };
    let width = mem_width.to_op_width()?;
    if !matches!(
        width,
        OpWidth::W8 | OpWidth::W16 | OpWidth::W32 | OpWidth::W64
    ) || !x86_jit_mem_address_shape_valid(addr)
    {
        return None;
    }

    let compute = block.ops.get(index + 1)?;
    let store = block.ops.get(index + 2)?;
    let replay = block.ops.get(index + 3)?;
    if [compute, store, replay]
        .into_iter()
        .any(|op| op.guest_pc != load.guest_pc)
    {
        return None;
    }
    let (compute_tag, result, compute_old, source, compute_width, compute_flags) =
        x86_binary_alu_shape(&compute.kind)?;
    let (replay_tag, flags_result, replay_old, replay_source, replay_width, replay_flags) =
        x86_binary_alu_shape(&replay.kind)?;
    let VReg::Virtual(_) = result else {
        return None;
    };
    let VReg::Virtual(_) = flags_result else {
        return None;
    };
    let source_valid = match &source {
        SrcOperand::Reg(VReg::Arch(ArchReg::X86(reg))) => reg.gpr_index().is_some(),
        SrcOperand::Imm(value) => width != OpWidth::W64 || i32::try_from(*value).is_ok(),
        _ => false,
    };
    if compute_tag != replay_tag
        || compute_old != old
        || replay_old != old
        || source != replay_source
        || compute_width != width
        || replay_width != width
        || compute_flags != FlagUpdate::None
        || replay_flags != FlagUpdate::All
        || !source_valid
        || !matches!(
            &store.kind,
            OpKind::Store {
                src,
                addr: store_addr,
                width: store_width,
            } if *src == result && *store_addr == *addr && *store_width == mem_width
        )
        || virtual_definitions.get(&old) != Some(&1)
        || virtual_uses.get(&old) != Some(&2)
        || virtual_definitions.get(&result) != Some(&1)
        || virtual_uses.get(&result) != Some(&1)
        || virtual_definitions.get(&flags_result) != Some(&1)
        || virtual_uses.contains_key(&flags_result)
    {
        return None;
    }

    Some(4)
}

fn x86_flagged_unary_shape(
    kind: &crate::smir::ir::ops::OpKind,
) -> Option<(
    u8,
    crate::smir::ir::types::VReg,
    crate::smir::ir::types::VReg,
    crate::smir::ir::types::OpWidth,
    crate::smir::ir::flags::FlagUpdate,
)> {
    use crate::smir::ir::ops::OpKind;

    match kind {
        OpKind::Neg {
            dst,
            src,
            width,
            flags,
        } => Some((0, *dst, *src, *width, *flags)),
        OpKind::Inc {
            dst,
            src,
            width,
            flags,
        } => Some((1, *dst, *src, *width, *flags)),
        OpKind::Dec {
            dst,
            src,
            width,
            flags,
        } => Some((2, *dst, *src, *width, *flags)),
        _ => None,
    }
}

/// Validate the exact fault-precise scalar memory-destination unary sequence
/// emitted by the x86 lifter. `NEG`/`INC`/`DEC` use `Load old; unary result`
/// without flags; `Store result`; then a full-flag replay into a dead virtual.
/// Flag-neutral `NOT` reuses the load virtual in place and needs no replay.
pub(crate) fn x86_jit_mem_unary_rmw_sequence_len(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    virtual_definitions: &std::collections::HashMap<crate::smir::ir::types::VReg, usize>,
    virtual_uses: &std::collections::HashMap<crate::smir::ir::types::VReg, usize>,
) -> Option<usize> {
    use crate::smir::ir::flags::FlagUpdate;
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::{OpWidth, SignExtend, VReg};

    if !allow_mem {
        return None;
    }
    let load = block.ops.get(index)?;
    let (old, addr, mem_width) = match &load.kind {
        OpKind::Load {
            dst: old @ VReg::Virtual(_),
            addr,
            width,
            sign: SignExtend::Zero,
        } => (*old, addr, *width),
        _ => return None,
    };
    let width = mem_width.to_op_width()?;
    if !matches!(
        width,
        OpWidth::W8 | OpWidth::W16 | OpWidth::W32 | OpWidth::W64
    ) || !x86_jit_mem_address_shape_valid(addr)
    {
        return None;
    }

    let compute = block.ops.get(index + 1)?;
    let store = block.ops.get(index + 2)?;
    if compute.guest_pc != load.guest_pc || store.guest_pc != load.guest_pc {
        return None;
    }

    if let Some((compute_tag, result, compute_old, compute_width, compute_flags)) =
        x86_flagged_unary_shape(&compute.kind)
    {
        let replay = block.ops.get(index + 3)?;
        let (replay_tag, flags_result, replay_old, replay_width, replay_flags) =
            x86_flagged_unary_shape(&replay.kind)?;
        if replay.guest_pc != load.guest_pc
            || !matches!(result, VReg::Virtual(_))
            || !matches!(flags_result, VReg::Virtual(_))
            || compute_tag != replay_tag
            || compute_old != old
            || replay_old != old
            || compute_width != width
            || replay_width != width
            || compute_flags != FlagUpdate::None
            || replay_flags != FlagUpdate::All
            || !matches!(
                &store.kind,
                OpKind::Store {
                    src,
                    addr: store_addr,
                    width: store_width,
                } if *src == result && *store_addr == *addr && *store_width == mem_width
            )
            || virtual_definitions.get(&old) != Some(&1)
            || virtual_uses.get(&old) != Some(&2)
            || virtual_definitions.get(&result) != Some(&1)
            || virtual_uses.get(&result) != Some(&1)
            || virtual_definitions.get(&flags_result) != Some(&1)
            || virtual_uses.contains_key(&flags_result)
        {
            return None;
        }
        return Some(4);
    }

    if !matches!(
        &compute.kind,
        OpKind::Not {
            dst,
            src,
            width: not_width,
        } if *dst == old && *src == old && *not_width == width
    ) || !matches!(
        &store.kind,
        OpKind::Store {
            src,
            addr: store_addr,
            width: store_width,
        } if *src == old && *store_addr == *addr && *store_width == mem_width
    ) || virtual_definitions.get(&old) != Some(&2)
        || virtual_uses.get(&old) != Some(&2)
    {
        return None;
    }

    Some(3)
}

fn x86_shift_rmw_shape(
    kind: &crate::smir::ir::ops::OpKind,
) -> Option<(
    u8,
    crate::smir::ir::types::VReg,
    crate::smir::ir::types::VReg,
    crate::smir::ir::types::SrcOperand,
    crate::smir::ir::types::OpWidth,
    crate::smir::ir::flags::FlagUpdate,
)> {
    use crate::smir::ir::ops::OpKind;

    match kind {
        OpKind::Rol {
            dst,
            src,
            amount,
            width,
            flags,
        } => Some((0, *dst, *src, amount.clone(), *width, *flags)),
        OpKind::Ror {
            dst,
            src,
            amount,
            width,
            flags,
        } => Some((1, *dst, *src, amount.clone(), *width, *flags)),
        OpKind::Rcl {
            dst,
            src,
            amount,
            width,
            flags,
        } => Some((2, *dst, *src, amount.clone(), *width, *flags)),
        OpKind::Rcr {
            dst,
            src,
            amount,
            width,
            flags,
        } => Some((3, *dst, *src, amount.clone(), *width, *flags)),
        OpKind::Shl {
            dst,
            src,
            amount,
            width,
            flags,
        } => Some((4, *dst, *src, amount.clone(), *width, *flags)),
        OpKind::Shr {
            dst,
            src,
            amount,
            width,
            flags,
        } => Some((5, *dst, *src, amount.clone(), *width, *flags)),
        OpKind::Sar {
            dst,
            src,
            amount,
            width,
            flags,
        } => Some((7, *dst, *src, amount.clone(), *width, *flags)),
        _ => None,
    }
}

/// Validate the exact four-op fault-precise memory-destination shift/rotate
/// sequence emitted by the x86 lifter. Immediate counts are normalized exactly
/// as x86 does. ROL/ROR/RCL/RCR accept every immediate/CL count because native
/// value/CF behavior follows the operand or through-carry period while the saved
/// RFLAGS merge classifies zero/one/multi using the raw masked count. SAR also
/// accepts every count because repeated sign fill keeps its result and CF
/// representable. Subword SHL/SHR counts are also representable: equality
/// derives CF from the staged original operand, while oversized counts clear
/// Rax's deterministic CF/OF outputs after replay.
pub(crate) fn x86_jit_mem_shift_rmw_sequence_len(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    virtual_definitions: &std::collections::HashMap<crate::smir::ir::types::VReg, usize>,
    virtual_uses: &std::collections::HashMap<crate::smir::ir::types::VReg, usize>,
) -> Option<usize> {
    use crate::smir::ir::flags::{FlagSet, FlagUpdate};
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::{OpWidth, SignExtend, SrcOperand, VReg};

    if !allow_mem {
        return None;
    }
    let load = block.ops.get(index)?;
    let (old, addr, mem_width) = match &load.kind {
        OpKind::Load {
            dst: old @ VReg::Virtual(_),
            addr,
            width,
            sign: SignExtend::Zero,
        } => (*old, addr, *width),
        _ => return None,
    };
    let width = mem_width.to_op_width()?;
    if !matches!(
        width,
        OpWidth::W8 | OpWidth::W16 | OpWidth::W32 | OpWidth::W64
    ) || !x86_jit_mem_address_shape_valid(addr)
    {
        return None;
    }

    let compute = block.ops.get(index + 1)?;
    let store = block.ops.get(index + 2)?;
    let replay = block.ops.get(index + 3)?;
    if [compute, store, replay]
        .into_iter()
        .any(|op| op.guest_pc != load.guest_pc)
    {
        return None;
    }
    if matches!(
        compute.x86_hint,
        Some(crate::smir::ir::ops::X86OpHint::ShiftGroup6)
    ) || matches!(
        replay.x86_hint,
        Some(crate::smir::ir::ops::X86OpHint::ShiftGroup6)
    ) {
        return None;
    }
    let (compute_tag, result, compute_old, amount, compute_width, compute_flags) =
        x86_shift_rmw_shape(&compute.kind)?;
    let (replay_tag, flags_result, replay_old, replay_amount, replay_width, replay_flags) =
        x86_shift_rmw_shape(&replay.kind)?;
    let count_valid = match &amount {
        SrcOperand::Imm(value) if (0..=i64::from(u8::MAX)).contains(value) => true,
        SrcOperand::Reg(VReg::Arch(crate::smir::ir::types::ArchReg::X86(
            crate::smir::ir::types::X86Reg::Rcx,
        ))) => true,
        _ => false,
    };
    let rotate_flags = FlagSet::CF.union(FlagSet::OF);
    let replay_flags_valid = if compute_tag <= 3 {
        matches!(replay_flags, FlagUpdate::Specific(set) if set == rotate_flags)
    } else {
        replay_flags == FlagUpdate::All
    };
    if !matches!(result, VReg::Virtual(_))
        || !matches!(flags_result, VReg::Virtual(_))
        || compute_tag != replay_tag
        || compute_old != old
        || replay_old != old
        || amount != replay_amount
        || compute_width != width
        || replay_width != width
        || compute_flags != FlagUpdate::None
        || !replay_flags_valid
        || !count_valid
        || !matches!(
            &store.kind,
            OpKind::Store {
                src,
                addr: store_addr,
                width: store_width,
            } if *src == result && *store_addr == *addr && *store_width == mem_width
        )
        || virtual_definitions.get(&old) != Some(&1)
        || virtual_uses.get(&old) != Some(&2)
        || virtual_definitions.get(&result) != Some(&1)
        || virtual_uses.get(&result) != Some(&1)
        || virtual_definitions.get(&flags_result) != Some(&1)
        || virtual_uses.contains_key(&flags_result)
    {
        return None;
    }

    Some(4)
}

/// Validate an exact scalar memory-source pair emitted by the x86 lifter:
/// `Load virtual; ALU/CMP/TEST ... virtual`. The load result must be an SSA
/// single-definition/single-use value, and every architectural operand must be
/// representable by the native identity bridge. Native lowering replaces the
/// pair with one fault-precise MMU helper load and a stack-backed scalar source,
/// so the virtual never aliases a live guest GPR. This also admits the exact
/// destructive two-operand `IMUL dst,virtual` and hinted
/// `IMUL dst,virtual,immediate` shapes.
pub(crate) fn x86_jit_mem_alu_source_sequence_len(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    virtual_definitions: &std::collections::HashMap<crate::smir::ir::types::VReg, usize>,
    virtual_uses: &std::collections::HashMap<crate::smir::ir::types::VReg, usize>,
) -> Option<usize> {
    use crate::smir::ir::flags::FlagUpdate;
    use crate::smir::ir::ops::{OpKind, X86OpHint};
    use crate::smir::ir::types::{OpWidth, SignExtend, SrcOperand, VReg};

    if !allow_mem {
        return None;
    }
    let load = block.ops.get(index)?;
    let (temporary, addr, mem_width) = match &load.kind {
        OpKind::Load {
            dst: temporary @ VReg::Virtual(_),
            addr,
            width,
            sign: SignExtend::Zero,
        } => (*temporary, addr, *width),
        _ => return None,
    };
    let width = mem_width.to_op_width()?;
    if !matches!(
        width,
        OpWidth::W8 | OpWidth::W16 | OpWidth::W32 | OpWidth::W64
    ) || !x86_jit_mem_address_shape_valid(addr)
        || virtual_definitions.get(&temporary) != Some(&1)
        || virtual_uses.get(&temporary) != Some(&1)
    {
        return None;
    }

    let consumer = block.ops.get(index + 1)?;
    if consumer.guest_pc != load.guest_pc {
        return None;
    }
    let identity = |reg: &VReg| x86_native_identity_gpr(reg);
    let imm_valid = |value: i64| width != OpWidth::W64 || i32::try_from(value).is_ok();
    let imul_imm_valid = |value: i64, hint: Option<X86OpHint>| match hint {
        Some(X86OpHint::ImulImm8) => i8::try_from(value).is_ok(),
        Some(X86OpHint::ImulImm32) => match width {
            OpWidth::W16 => i16::try_from(value).is_ok(),
            OpWidth::W32 | OpWidth::W64 => i32::try_from(value).is_ok(),
            _ => false,
        },
        _ => false,
    };
    let binary_shape =
        |dst: &VReg, src1: &VReg, src2: &SrcOperand, op_width: OpWidth, flags: FlagUpdate| {
            op_width == width
                && identity(dst)
                && matches!(flags, FlagUpdate::None | FlagUpdate::All)
                && match (src1, src2) {
                    (lhs, SrcOperand::Reg(rhs)) if *rhs == temporary => identity(lhs),
                    (lhs, SrcOperand::Reg(rhs)) if *lhs == temporary => identity(rhs),
                    (lhs, SrcOperand::Imm(value)) if *lhs == temporary => imm_valid(*value),
                    _ => false,
                }
        };

    let valid = match &consumer.kind {
        OpKind::Add {
            dst,
            src1,
            src2,
            width,
            flags,
        }
        | OpKind::Sub {
            dst,
            src1,
            src2,
            width,
            flags,
        }
        | OpKind::Adc {
            dst,
            src1,
            src2,
            width,
            flags,
        }
        | OpKind::Sbb {
            dst,
            src1,
            src2,
            width,
            flags,
        }
        | OpKind::And {
            dst,
            src1,
            src2,
            width,
            flags,
        }
        | OpKind::Or {
            dst,
            src1,
            src2,
            width,
            flags,
        }
        | OpKind::Xor {
            dst,
            src1,
            src2,
            width,
            flags,
        } => binary_shape(dst, src1, src2, *width, *flags),
        OpKind::Cmp {
            src1,
            src2,
            width: op_width,
        }
        | OpKind::Test {
            src1,
            src2,
            width: op_width,
        } if *op_width == width => match (src1, src2) {
            (lhs, SrcOperand::Reg(rhs)) if *lhs == temporary => identity(rhs),
            (lhs, SrcOperand::Reg(rhs)) if *rhs == temporary => identity(lhs),
            (lhs, SrcOperand::Imm(value)) if *lhs == temporary => imm_valid(*value),
            _ => false,
        },
        OpKind::MulS {
            dst_lo,
            dst_hi: None,
            src1,
            src2: SrcOperand::Reg(source),
            width: op_width,
            flags,
        } => {
            *op_width == width
                && matches!(width, OpWidth::W16 | OpWidth::W32 | OpWidth::W64)
                && dst_lo == src1
                && *source == temporary
                && identity(dst_lo)
                && matches!(flags, FlagUpdate::None | FlagUpdate::All)
                && consumer.x86_hint.is_none()
        }
        OpKind::MulS {
            dst_lo,
            dst_hi: None,
            src1,
            src2: SrcOperand::Imm(value),
            width: op_width,
            flags,
        } => {
            *op_width == width
                && matches!(width, OpWidth::W16 | OpWidth::W32 | OpWidth::W64)
                && *src1 == temporary
                && identity(dst_lo)
                && matches!(flags, FlagUpdate::None | FlagUpdate::All)
                && imul_imm_valid(*value, consumer.x86_hint)
        }
        _ => false,
    };

    valid.then_some(2)
}

/// Validate the exact two-op scalar count memory-source shape emitted by the
/// x86 lifter: `Load virtual; X86Count architectural_dst,virtual`. The helper-
/// backed lowerer stages the loaded value on its own stack, so the SSA virtual
/// must have exactly one definition and one use and never enters the identity
/// register map.
pub(crate) fn x86_jit_mem_count_source_sequence_len(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    virtual_definitions: &std::collections::HashMap<crate::smir::ir::types::VReg, usize>,
    virtual_uses: &std::collections::HashMap<crate::smir::ir::types::VReg, usize>,
) -> Option<usize> {
    use crate::smir::ir::flags::FlagSet;
    use crate::smir::ir::ops::{OpKind, X86CountKind};
    use crate::smir::ir::types::{OpWidth, SignExtend, VReg};

    if !allow_mem {
        return None;
    }
    let load = block.ops.get(index)?;
    let (temporary, addr, mem_width) = match &load.kind {
        OpKind::Load {
            dst: temporary @ VReg::Virtual(_),
            addr,
            width,
            sign: SignExtend::Zero,
        } => (*temporary, addr, *width),
        _ => return None,
    };
    let width = mem_width.to_op_width()?;
    if !matches!(width, OpWidth::W16 | OpWidth::W32 | OpWidth::W64)
        || !x86_jit_mem_address_shape_valid(addr)
        || virtual_definitions.get(&temporary) != Some(&1)
        || virtual_uses.get(&temporary) != Some(&1)
    {
        return None;
    }

    let consumer = block.ops.get(index + 1)?;
    let OpKind::X86Count {
        dst,
        src,
        width: count_width,
        kind,
        flags,
    } = &consumer.kind
    else {
        return None;
    };
    let defined = match kind {
        X86CountKind::Popcnt => FlagSet::ALL_X86,
        X86CountKind::Tzcnt | X86CountKind::Lzcnt => FlagSet::CF.union(FlagSet::ZF),
    };
    (consumer.guest_pc == load.guest_pc
        && *src == temporary
        && *count_width == width
        && x86_native_identity_gpr(dst)
        && flags.as_set().difference(defined).is_empty())
    .then_some(2)
}

/// Validate the exact two-op bit-scan memory-source shape emitted by the x86
/// lifter: `Load virtual; Bsf/Bsr architectural_dst,virtual`. The helper-backed
/// lowerer consumes the load from caller-owned stack storage, so the virtual
/// must remain a single-definition/single-use value and the scan must request
/// only its architecturally defined ZF update.
pub(crate) fn x86_jit_mem_bit_scan_source_sequence_len(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    virtual_definitions: &std::collections::HashMap<crate::smir::ir::types::VReg, usize>,
    virtual_uses: &std::collections::HashMap<crate::smir::ir::types::VReg, usize>,
) -> Option<usize> {
    use crate::smir::ir::flags::{FlagSet, FlagUpdate};
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::{OpWidth, SignExtend, VReg};

    if !allow_mem {
        return None;
    }
    let load = block.ops.get(index)?;
    let (temporary, addr, mem_width) = match &load.kind {
        OpKind::Load {
            dst: temporary @ VReg::Virtual(_),
            addr,
            width,
            sign: SignExtend::Zero,
        } => (*temporary, addr, *width),
        _ => return None,
    };
    let width = mem_width.to_op_width()?;
    if !matches!(width, OpWidth::W16 | OpWidth::W32 | OpWidth::W64)
        || !x86_jit_mem_address_shape_valid(addr)
        || virtual_definitions.get(&temporary) != Some(&1)
        || virtual_uses.get(&temporary) != Some(&1)
    {
        return None;
    }

    let consumer = block.ops.get(index + 1)?;
    let (dst, src, scan_width, flags) = match &consumer.kind {
        OpKind::Bsf {
            dst,
            src,
            width,
            flags,
        }
        | OpKind::Bsr {
            dst,
            src,
            width,
            flags,
        } => (dst, src, width, flags),
        _ => return None,
    };
    (consumer.guest_pc == load.guest_pc
        && *src == temporary
        && *scan_width == width
        && x86_native_identity_gpr(dst)
        && *flags == FlagUpdate::Specific(FlagSet::ZF))
    .then_some(2)
}

/// Validate the exact non-modifying immediate memory bit-test shape emitted by
/// the x86 lifter: `Load virtual; Bt virtual,imm`. Register-index memory forms
/// first perform signed bit-string address adjustment and therefore remain a
/// distinct lowering problem. The loaded virtual must have one definition and
/// one use and the already-normalized immediate must select a bit in the
/// loaded operand.
pub(crate) fn x86_jit_mem_bit_test_source_sequence_len(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    virtual_definitions: &std::collections::HashMap<crate::smir::ir::types::VReg, usize>,
    virtual_uses: &std::collections::HashMap<crate::smir::ir::types::VReg, usize>,
) -> Option<usize> {
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::{OpWidth, SignExtend, SrcOperand, VReg};

    if !allow_mem {
        return None;
    }
    let load = block.ops.get(index)?;
    let (temporary, addr, mem_width) = match &load.kind {
        OpKind::Load {
            dst: temporary @ VReg::Virtual(_),
            addr,
            width,
            sign: SignExtend::Zero,
        } => (*temporary, addr, *width),
        _ => return None,
    };
    let width = mem_width.to_op_width()?;
    if !matches!(width, OpWidth::W16 | OpWidth::W32 | OpWidth::W64)
        || !x86_jit_mem_address_shape_valid(addr)
        || virtual_definitions.get(&temporary) != Some(&1)
        || virtual_uses.get(&temporary) != Some(&1)
    {
        return None;
    }

    let consumer = block.ops.get(index + 1)?;
    let OpKind::Bt {
        src,
        index: SrcOperand::Imm(bit),
        width: bit_width,
    } = &consumer.kind
    else {
        return None;
    };
    (consumer.guest_pc == load.guest_pc
        && *src == temporary
        && *bit_width == width
        && (0..i64::from(width.bits())).contains(bit))
    .then_some(2)
}

/// Validate the exact fault-precise immediate memory bit-update sequence
/// emitted by the x86 lifter:
///
/// `Load old; Mov mask,1; Shl mask,imm; [Not mask]; Or/And/Xor new,old,mask;
/// Store new; Bt old,imm`. O2 may fold the W64 mask construction into an
/// immediate `Or`/`And`/`Xor`, which is accepted only when its mask exactly
/// corresponds to the final `Bt` index.
///
/// The optional `Not` and following `And` identify BTR; `Or` identifies BTS
/// and `Xor` identifies BTC. Register-index bit-string forms perform signed
/// address adjustment before this sequence and are intentionally excluded.
/// Every temporary must have the exact SSA definition/use counts implied by
/// the lifter so the native lowerer can eliminate the complete sequence.
pub(crate) fn x86_jit_mem_bit_update_rmw_sequence_len(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    virtual_definitions: &std::collections::HashMap<crate::smir::ir::types::VReg, usize>,
    virtual_uses: &std::collections::HashMap<crate::smir::ir::types::VReg, usize>,
) -> Option<usize> {
    use crate::smir::ir::flags::FlagUpdate;
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::{OpWidth, SignExtend, SrcOperand, VReg};

    if !allow_mem {
        return None;
    }
    let load = block.ops.get(index)?;
    let (old, addr, mem_width) = match &load.kind {
        OpKind::Load {
            dst: old @ VReg::Virtual(_),
            addr,
            width,
            sign: SignExtend::Zero,
        } => (*old, addr, *width),
        _ => return None,
    };
    let width = mem_width.to_op_width()?;
    if !matches!(width, OpWidth::W16 | OpWidth::W32 | OpWidth::W64)
        || !x86_jit_mem_address_shape_valid(addr)
    {
        return None;
    }

    // O2 constant-folds the W64 mask producer into the update. Match that
    // exact four-op form before checking the unfused lifter sequence below.
    if width == OpWidth::W64 {
        let compute = block.ops.get(index + 1)?;
        let folded = match &compute.kind {
            OpKind::Or {
                dst,
                src1,
                src2: SrcOperand::Imm(mask),
                width,
                flags,
            } => Some((0u8, *dst, *src1, *mask, *width, *flags)),
            OpKind::And {
                dst,
                src1,
                src2: SrcOperand::Imm(mask),
                width,
                flags,
            } => Some((1u8, *dst, *src1, *mask, *width, *flags)),
            OpKind::Xor {
                dst,
                src1,
                src2: SrcOperand::Imm(mask),
                width,
                flags,
            } => Some((2u8, *dst, *src1, *mask, *width, *flags)),
            _ => None,
        };
        if let Some((action, result, compute_old, mask, compute_width, compute_flags)) = folded {
            let VReg::Virtual(_) = result else {
                return None;
            };
            let store = block.ops.get(index + 2)?;
            let replay = block.ops.get(index + 3)?;
            let bit = match &replay.kind {
                OpKind::Bt {
                    src,
                    index: SrcOperand::Imm(bit),
                    width: replay_width,
                } if *src == old
                    && *replay_width == width
                    && (0..i64::from(width.bits())).contains(bit) =>
                {
                    *bit
                }
                _ => return None,
            };
            let bit_mask = 1u64 << (bit as u32);
            let expected_mask = match action {
                0 | 2 => bit_mask as i64,
                1 => (!bit_mask) as i64,
                _ => unreachable!(),
            };
            return (compute_old == old
                && mask == expected_mask
                && compute_width == width
                && compute_flags == FlagUpdate::None
                && block.ops[index..index + 4]
                    .iter()
                    .all(|op| op.guest_pc == load.guest_pc)
                && matches!(
                    &store.kind,
                    OpKind::Store {
                        src,
                        addr: store_addr,
                        width: store_width,
                    } if *src == result && *store_addr == *addr && *store_width == mem_width
                )
                && virtual_definitions.get(&old) == Some(&1)
                && virtual_uses.get(&old) == Some(&2)
                && virtual_definitions.get(&result) == Some(&1)
                && virtual_uses.get(&result) == Some(&1))
            .then_some(4);
        }
    }

    let mask = match &block.ops.get(index + 1)?.kind {
        OpKind::Mov {
            dst: mask @ VReg::Virtual(_),
            src: SrcOperand::Imm(1),
            width: mov_width,
        } if *mov_width == width => *mask,
        _ => return None,
    };
    let bit = match &block.ops.get(index + 2)?.kind {
        OpKind::Shl {
            dst,
            src,
            amount: SrcOperand::Imm(bit),
            width: shift_width,
            flags: FlagUpdate::None,
        } if *dst == mask
            && *src == mask
            && *shift_width == width
            && (0..i64::from(width.bits())).contains(bit) =>
        {
            *bit
        }
        _ => return None,
    };

    // (action tag, compute index, sequence length, exact mask def/use count)
    let (action, compute_index, consumed, mask_count) = match &block.ops.get(index + 3)?.kind {
        OpKind::Or { .. } => (0u8, index + 3, 6usize, 2usize),
        OpKind::Xor { .. } => (2u8, index + 3, 6usize, 2usize),
        OpKind::Not {
            dst,
            src,
            width: not_width,
        } if *dst == mask && *src == mask && *not_width == width => {
            (1u8, index + 4, 7usize, 3usize)
        }
        _ => return None,
    };

    let compute = block.ops.get(compute_index)?;
    let (result, compute_old, compute_mask, compute_width, compute_flags) = match &compute.kind {
        OpKind::Or {
            dst,
            src1,
            src2,
            width,
            flags,
        } if action == 0 => (*dst, *src1, src2, *width, *flags),
        OpKind::And {
            dst,
            src1,
            src2,
            width,
            flags,
        } if action == 1 => (*dst, *src1, src2, *width, *flags),
        OpKind::Xor {
            dst,
            src1,
            src2,
            width,
            flags,
        } if action == 2 => (*dst, *src1, src2, *width, *flags),
        _ => return None,
    };
    let VReg::Virtual(_) = result else {
        return None;
    };
    if compute_old != old
        || !matches!(compute_mask, SrcOperand::Reg(reg) if *reg == mask)
        || compute_width != width
        || compute_flags != FlagUpdate::None
    {
        return None;
    }

    let store = block.ops.get(compute_index + 1)?;
    let replay = block.ops.get(compute_index + 2)?;
    if block.ops[index..index + consumed]
        .iter()
        .any(|op| op.guest_pc != load.guest_pc)
        || !matches!(
            &store.kind,
            OpKind::Store {
                src,
                addr: store_addr,
                width: store_width,
            } if *src == result && *store_addr == *addr && *store_width == mem_width
        )
        || !matches!(
            &replay.kind,
            OpKind::Bt {
                src,
                index: SrcOperand::Imm(replay_bit),
                width: replay_width,
            } if *src == old && *replay_bit == bit && *replay_width == width
        )
        || virtual_definitions.get(&old) != Some(&1)
        || virtual_uses.get(&old) != Some(&2)
        || virtual_definitions.get(&mask) != Some(&mask_count)
        || virtual_uses.get(&mask) != Some(&mask_count)
        || virtual_definitions.get(&result) != Some(&1)
        || virtual_uses.get(&result) != Some(&1)
    {
        return None;
    }

    Some(consumed)
}

/// Validate the exact two-op shape emitted for a memory-source x86 CRC32.
/// The virtual load result must be single-definition/single-use so native
/// lowering can eliminate it without creating an identity-map GPR alias.
fn x86_mem_crc32_pair_valid(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    virtual_definitions: &std::collections::HashMap<crate::smir::ir::types::VReg, usize>,
    virtual_uses: &std::collections::HashMap<crate::smir::ir::types::VReg, usize>,
) -> bool {
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::{MemWidth, OpWidth, SignExtend, VReg};

    if !allow_mem {
        return false;
    }
    let (load_pc, temporary, addr, width) = match block.ops.get(index) {
        Some(op) => match &op.kind {
            OpKind::Load {
                dst: VReg::Virtual(temporary),
                addr,
                width,
                sign: SignExtend::Zero,
            } => (op.guest_pc, VReg::Virtual(*temporary), addr, *width),
            _ => return false,
        },
        None => return false,
    };
    let data_width = match width {
        MemWidth::B1 => OpWidth::W8,
        MemWidth::B2 => OpWidth::W16,
        MemWidth::B4 => OpWidth::W32,
        MemWidth::B8 => OpWidth::W64,
        _ => return false,
    };
    let crc = match block.ops.get(index + 1) {
        Some(op) if op.guest_pc == load_pc => op,
        _ => return false,
    };
    let accumulator_valid = matches!(
        &crc.kind,
        OpKind::Crc32C {
            dst,
            crc,
            data,
            data_width: crc_width,
        } if dst == crc
            && *data == temporary
            && *crc_width == data_width
            && x86_native_identity_gpr(dst)
    );
    if !accumulator_valid || !x86_jit_mem_address_shape_valid(addr) {
        return false;
    }

    virtual_definitions.get(&temporary) == Some(&1) && virtual_uses.get(&temporary) == Some(&1)
}

/// Validate the exact five-op APX PUSH2 shape emitted by the x86 lifter. Both
/// source snapshots are single-definition/single-use virtuals; native lowering
/// replaces the complete sequence with one paired helper call.
pub(crate) fn x86_jit_push2_sequence_len(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    virtual_definitions: &std::collections::HashMap<crate::smir::ir::types::VReg, usize>,
    virtual_uses: &std::collections::HashMap<crate::smir::ir::types::VReg, usize>,
) -> Option<usize> {
    use crate::smir::ir::flags::FlagUpdate;
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::{Address, ArchReg, MemWidth, OpWidth, SrcOperand, VReg, X86Reg};

    if !allow_mem {
        return None;
    }
    let rsp = VReg::Arch(ArchReg::X86(X86Reg::Rsp));
    let first = block.ops.get(index)?;
    let (tmp_low, src_low) = match first.kind {
        OpKind::Mov {
            dst: temporary @ VReg::Virtual(_),
            src: SrcOperand::Reg(source @ VReg::Arch(ArchReg::X86(reg))),
            width: OpWidth::W64,
        } if reg.gpr_index().is_some() && source != rsp => (temporary, source),
        _ => return None,
    };
    let second = block.ops.get(index + 1)?;
    let (tmp_high, src_high) = match second.kind {
        OpKind::Mov {
            dst: temporary @ VReg::Virtual(_),
            src: SrcOperand::Reg(source @ VReg::Arch(ArchReg::X86(reg))),
            width: OpWidth::W64,
        } if reg.gpr_index().is_some() && source != rsp => (temporary, source),
        _ => return None,
    };
    let sub = block.ops.get(index + 2)?;
    let store_low = block.ops.get(index + 3)?;
    let store_high = block.ops.get(index + 4)?;
    if [second, sub, store_low, store_high]
        .iter()
        .any(|op| op.guest_pc != first.guest_pc)
        || !matches!(
            sub.kind,
            OpKind::Sub {
                dst,
                src1,
                src2: SrcOperand::Imm(16),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            } if dst == rsp && src1 == rsp
        )
        || !matches!(
            store_low.kind,
            OpKind::Store {
                src,
                addr: Address::Direct(base),
                width: MemWidth::B8,
            } if src == tmp_low && base == rsp
        )
        || !matches!(
            &store_high.kind,
            OpKind::Store {
                src,
                addr,
                width: MemWidth::B8,
            } if *src == tmp_high && *addr == Address::base_off(rsp, 8)
        )
        || virtual_definitions.get(&tmp_low) != Some(&1)
        || virtual_uses.get(&tmp_low) != Some(&1)
        || virtual_definitions.get(&tmp_high) != Some(&1)
        || virtual_uses.get(&tmp_high) != Some(&1)
    {
        return None;
    }

    let _ = (src_low, src_high);
    Some(5)
}

/// Identify a PUSH2-like same-instruction sequence that failed exact
/// validation. This prevents its virtual snapshots and two stores from being
/// admitted independently.
pub(crate) fn x86_jit_push2_candidate(block: &crate::smir::ir::SmirBlock, index: usize) -> bool {
    use crate::smir::ir::flags::FlagUpdate;
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::{ArchReg, OpWidth, SrcOperand, VReg, X86Reg};

    let rsp = VReg::Arch(ArchReg::X86(X86Reg::Rsp));
    let Some(first) = block.ops.get(index) else {
        return false;
    };
    matches!(
        first.kind,
        OpKind::Mov {
            dst: VReg::Virtual(_),
            width: OpWidth::W64,
            ..
        }
    ) && matches!(
        block.ops.get(index + 1),
        Some(second) if second.guest_pc == first.guest_pc
            && matches!(second.kind, OpKind::Mov { dst: VReg::Virtual(_), width: OpWidth::W64, .. })
    ) && matches!(
        block.ops.get(index + 2),
        Some(sub) if sub.guest_pc == first.guest_pc
            && matches!(
                sub.kind,
                OpKind::Sub {
                    dst,
                    src1,
                    src2: SrcOperand::Imm(16),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                } if dst == rsp && src1 == rsp
            )
    )
}

/// Validate the exact five-op APX POP2 shape emitted by the x86 lifter. The
/// V-register destination consumes `[RSP]`; the distinct ModRM B-register
/// destination consumes `[RSP+8]`.
pub(crate) fn x86_jit_pop2_sequence_len(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    virtual_definitions: &std::collections::HashMap<crate::smir::ir::types::VReg, usize>,
    virtual_uses: &std::collections::HashMap<crate::smir::ir::types::VReg, usize>,
) -> Option<usize> {
    use crate::smir::ir::flags::FlagUpdate;
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::{
        Address, ArchReg, MemWidth, OpWidth, SignExtend, SrcOperand, VReg, X86Reg,
    };

    if !allow_mem {
        return None;
    }
    let rsp = VReg::Arch(ArchReg::X86(X86Reg::Rsp));
    let first = block.ops.get(index)?;
    let tmp_low = match first.kind {
        OpKind::Load {
            dst: temporary @ VReg::Virtual(_),
            addr: Address::Direct(base),
            width: MemWidth::B8,
            sign: SignExtend::Zero,
        } if base == rsp => temporary,
        _ => return None,
    };
    let second = block.ops.get(index + 1)?;
    let tmp_high = match &second.kind {
        OpKind::Load {
            dst: temporary @ VReg::Virtual(_),
            addr,
            width: MemWidth::B8,
            sign: SignExtend::Zero,
        } if *addr == Address::base_off(rsp, 8) => *temporary,
        _ => return None,
    };
    let add = block.ops.get(index + 2)?;
    let low_commit = block.ops.get(index + 3)?;
    let high_commit = block.ops.get(index + 4)?;
    let dst_low = match low_commit.kind {
        OpKind::Mov {
            dst: destination @ VReg::Arch(ArchReg::X86(reg)),
            src: SrcOperand::Reg(source),
            width: OpWidth::W64,
        } if reg.gpr_index().is_some() && destination != rsp && source == tmp_low => destination,
        _ => return None,
    };
    let dst_high = match high_commit.kind {
        OpKind::Mov {
            dst: destination @ VReg::Arch(ArchReg::X86(reg)),
            src: SrcOperand::Reg(source),
            width: OpWidth::W64,
        } if reg.gpr_index().is_some() && destination != rsp && source == tmp_high => destination,
        _ => return None,
    };
    if dst_low == dst_high
        || [second, add, low_commit, high_commit]
            .iter()
            .any(|op| op.guest_pc != first.guest_pc)
        || !matches!(
            add.kind,
            OpKind::Add {
                dst,
                src1,
                src2: SrcOperand::Imm(16),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            } if dst == rsp && src1 == rsp
        )
        || virtual_definitions.get(&tmp_low) != Some(&1)
        || virtual_uses.get(&tmp_low) != Some(&1)
        || virtual_definitions.get(&tmp_high) != Some(&1)
        || virtual_uses.get(&tmp_high) != Some(&1)
    {
        return None;
    }
    Some(5)
}

/// Identify a POP2-like paired-load prefix that failed exact validation.
pub(crate) fn x86_jit_pop2_candidate(block: &crate::smir::ir::SmirBlock, index: usize) -> bool {
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::{Address, ArchReg, MemWidth, SignExtend, VReg, X86Reg};

    let rsp = VReg::Arch(ArchReg::X86(X86Reg::Rsp));
    let Some(first) = block.ops.get(index) else {
        return false;
    };
    matches!(
        first.kind,
        OpKind::Load {
            dst: VReg::Virtual(_),
            addr: Address::Direct(base),
            width: MemWidth::B8,
            sign: SignExtend::Zero,
        } if base == rsp
    ) && matches!(
        block.ops.get(index + 1),
        Some(second) if second.guest_pc == first.guest_pc
            && matches!(
                &second.kind,
                OpKind::Load {
                    dst: VReg::Virtual(_),
                    addr,
                    width: MemWidth::B8,
                    sign: SignExtend::Zero,
                } if *addr == Address::base_off(rsp, 8)
            )
    )
}

/// Validate the exact POP shapes emitted by the x86 lifter. Ordinary POP uses
/// a helper load followed by a state-backed RSP increment. POP RSP commits the
/// loaded value without exposing the increment, while POP SP first computes
/// the full-width increment and then replaces only its low 16 bits.
pub(crate) fn x86_jit_pop_sequence_len(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    virtual_definitions: &std::collections::HashMap<crate::smir::ir::types::VReg, usize>,
    virtual_uses: &std::collections::HashMap<crate::smir::ir::types::VReg, usize>,
) -> Option<usize> {
    use crate::smir::ir::flags::FlagUpdate;
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::{
        Address, ArchReg, MemWidth, OpWidth, SignExtend, SrcOperand, VReg, X86Reg,
    };

    if !allow_mem {
        return None;
    }
    let rsp = VReg::Arch(ArchReg::X86(X86Reg::Rsp));
    let load = block.ops.get(index)?;
    let (popped, mem_width, delta) = match load.kind {
        OpKind::Load {
            dst,
            addr: Address::Direct(base),
            width: mem_width @ (MemWidth::B2 | MemWidth::B8),
            sign: SignExtend::Zero,
        } if base == rsp => (
            dst,
            mem_width,
            if mem_width == MemWidth::B2 { 2 } else { 8 },
        ),
        _ => return None,
    };
    let same_pc = |offset: usize| {
        block
            .ops
            .get(index + offset)
            .is_some_and(|op| op.guest_pc == load.guest_pc)
    };

    if matches!(popped, VReg::Arch(ArchReg::X86(reg)) if reg.gpr_index().is_some()) && popped != rsp
    {
        let increment = block.ops.get(index + 1)?;
        if increment.guest_pc == load.guest_pc
            && matches!(
                increment.kind,
                OpKind::Add {
                    dst,
                    src1,
                    src2: SrcOperand::Imm(amount),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                } if dst == rsp && src1 == rsp && amount == delta
            )
        {
            return Some(2);
        }
        return None;
    }

    let VReg::Virtual(_) = popped else {
        return None;
    };
    if virtual_definitions.get(&popped) != Some(&1) || virtual_uses.get(&popped) != Some(&1) {
        return None;
    }

    if mem_width == MemWidth::B8 {
        let commit = block.ops.get(index + 1)?;
        return (same_pc(1)
            && matches!(
                commit.kind,
                OpKind::Mov {
                    dst,
                    src: SrcOperand::Reg(src),
                    width: OpWidth::W64,
                } if dst == rsp && src == popped
            ))
        .then_some(2);
    }

    let increment = block.ops.get(index + 1)?;
    let incremented = match increment.kind {
        OpKind::Add {
            dst: temporary @ VReg::Virtual(_),
            src1,
            src2: SrcOperand::Imm(2),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        } if src1 == rsp && increment.guest_pc == load.guest_pc => temporary,
        _ => return None,
    };
    if virtual_definitions.get(&incremented) != Some(&1)
        || virtual_uses.get(&incremented) != Some(&1)
        || !same_pc(2)
        || !same_pc(3)
    {
        return None;
    }
    let increment_commit = &block.ops[index + 2];
    let low_commit = &block.ops[index + 3];
    (matches!(
        increment_commit.kind,
        OpKind::Mov {
            dst,
            src: SrcOperand::Reg(src),
            width: OpWidth::W64,
        } if dst == rsp && src == incremented
    ) && matches!(
        low_commit.kind,
        OpKind::Mov {
            dst,
            src: SrcOperand::Reg(src),
            width: OpWidth::W16,
        } if dst == rsp && src == popped
    ))
    .then_some(4)
}

/// Identify a POP-like same-instruction sequence that failed exact validation.
/// Without this fail-closed check, its individual helper load and stack-state
/// operations could be admitted independently with different alias ordering.
pub(crate) fn x86_jit_pop_candidate(block: &crate::smir::ir::SmirBlock, index: usize) -> bool {
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::{Address, ArchReg, MemWidth, SignExtend, VReg, X86Reg};

    let rsp = VReg::Arch(ArchReg::X86(X86Reg::Rsp));
    let Some(load) = block.ops.get(index) else {
        return false;
    };
    if !matches!(
        load.kind,
        OpKind::Load {
            addr: Address::Direct(base),
            width: MemWidth::B2 | MemWidth::B8,
            sign: SignExtend::Zero,
            ..
        } if base == rsp
    ) {
        return false;
    }
    matches!(
        block.ops.get(index + 1),
        Some(next) if next.guest_pc == load.guest_pc
            && match &next.kind {
                OpKind::Add { src1, .. } => *src1 == rsp,
                OpKind::Mov { dst, .. } => *dst == rsp,
                _ => false,
            }
    )
}

/// Validate the exact PUSH shapes emitted by the x86 lifter. Lowering performs
/// the helper-backed store against `old_rsp - width` before committing RSP, so
/// a fault restarts with the architectural stack pointer unchanged. PUSH RSP
/// has an additional single-use virtual snapshot of the pre-decrement source.
pub(crate) fn x86_jit_push_sequence_len(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    virtual_definitions: &std::collections::HashMap<crate::smir::ir::types::VReg, usize>,
    virtual_uses: &std::collections::HashMap<crate::smir::ir::types::VReg, usize>,
) -> Option<usize> {
    use crate::smir::ir::flags::FlagUpdate;
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::{Address, ArchReg, MemWidth, OpWidth, SrcOperand, VReg, X86Reg};

    if !allow_mem {
        return None;
    }
    let rsp = VReg::Arch(ArchReg::X86(X86Reg::Rsp));
    let (sub_index, store_index, snapshot) = match block.ops.get(index).map(|op| &op.kind) {
        Some(OpKind::Mov {
            dst: temporary @ VReg::Virtual(_),
            src: SrcOperand::Reg(source),
            width: OpWidth::W16 | OpWidth::W64,
        }) if *source == rsp => (index + 1, index + 2, Some(*temporary)),
        _ => (index, index + 1, None),
    };
    let sub = block.ops.get(sub_index)?;
    let delta = match sub.kind {
        OpKind::Sub {
            dst,
            src1,
            src2: SrcOperand::Imm(delta @ (2 | 8)),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        } if dst == rsp && src1 == rsp => delta,
        _ => return None,
    };
    let store = block.ops.get(store_index)?;
    if sub.guest_pc != store.guest_pc
        || snapshot.is_some_and(|_| block.ops[index].guest_pc != sub.guest_pc)
    {
        return None;
    }
    let source_valid = |source: VReg| {
        matches!(source, VReg::Imm(_))
            || matches!(source, VReg::Arch(ArchReg::X86(reg)) if reg.gpr_index().is_some())
    };
    let expected_width = if delta == 2 {
        MemWidth::B2
    } else {
        MemWidth::B8
    };
    let store_source = match &store.kind {
        OpKind::Store {
            src,
            addr: Address::Direct(base),
            width,
        } if *base == rsp && *width == expected_width => *src,
        _ => return None,
    };

    match snapshot {
        Some(temporary) => {
            let expected_snapshot_width = if delta == 2 {
                OpWidth::W16
            } else {
                OpWidth::W64
            };
            if store_source != temporary
                || !matches!(block.ops[index].kind, OpKind::Mov { width, .. } if width == expected_snapshot_width)
                || virtual_definitions.get(&temporary) != Some(&1)
                || virtual_uses.get(&temporary) != Some(&1)
            {
                return None;
            }
            Some(3)
        }
        None => {
            if store_source == rsp || !source_valid(store_source) {
                return None;
            }
            Some(2)
        }
    }
}

pub(crate) fn x86_jit_push_candidate(block: &crate::smir::ir::SmirBlock, index: usize) -> bool {
    use crate::smir::ir::flags::FlagUpdate;
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::{Address, ArchReg, MemWidth, OpWidth, SrcOperand, VReg, X86Reg};

    let rsp = VReg::Arch(ArchReg::X86(X86Reg::Rsp));
    let Some(sub) = block.ops.get(index) else {
        return false;
    };
    let delta = match sub.kind {
        OpKind::Sub {
            dst,
            src1,
            src2: SrcOperand::Imm(delta @ (2 | 8)),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        } if dst == rsp && src1 == rsp => delta,
        _ => return false,
    };
    matches!(
        block.ops.get(index + 1),
        Some(crate::smir::ir::ops::SmirOp {
            guest_pc,
            kind: OpKind::Store {
                addr: Address::Direct(base),
                width,
                ..
            },
            ..
        }) if *guest_pc == sub.guest_pc
            && *base == rsp
            && *width == if delta == 2 { MemWidth::B2 } else { MemWidth::B8 }
    )
}

/// True if every op in `block` is safe to execute natively under the JIT:
///   (1) it is on the fail-safe register-only whitelist (`SmirOp::is_jit_safe`)
///       — so it touches no memory and is validated bit-exact vs KVM; and
///   (2) it writes only architectural registers (no virtual temp, which would
///       alias a guest GPR under the identity register map).
/// A trailing `TestCondition` feeding the block's `CondBranch`, exact
/// helper-backed scalar/CRC memory sequences, and exact POP/PUSH stack
/// temporaries are exempt because the lowerer never materializes their virtual
/// destinations.
fn block_is_clobber_safe(
    block: &crate::smir::ir::SmirBlock,
    allow_mem: bool,
    flags_live_out: crate::smir::ir::flags::FlagSet,
) -> bool {
    use crate::smir::ir::Terminator;
    use crate::smir::ir::ops::{OpKind, X86OpHint};
    use crate::smir::ir::types::{ArchReg, VReg, X86Reg};

    // The native trampoline runs the region on the HOST stack: guest RSP is
    // never loaded into the host RSP, and the lowerer's prologue repurposes RBP
    // as the frame pointer. Validated MOV forms use GuestRegs slots and keep the
    // prologue's saved guest RBP coherent; other RSP/RBP operations would still
    // compute against host stack state and therefore remain ineligible.
    let touches_sp_bp = |v: &VReg| {
        matches!(
            v,
            VReg::Arch(ArchReg::X86(X86Reg::Rsp)) | VReg::Arch(ArchReg::X86(X86Reg::Rbp))
        )
    };

    let n = block.ops.len();
    // Count virtual definitions and uses once. Exact helper-sequence validation
    // then remains O(1) per candidate and the complete gate remains O(N).
    let mut virtual_definitions = std::collections::HashMap::new();
    let mut virtual_uses = std::collections::HashMap::new();
    for op in &block.ops {
        for reg in op.kind.dests() {
            if matches!(reg, VReg::Virtual(_)) {
                *virtual_definitions.entry(reg).or_insert(0usize) += 1;
            }
        }
        for reg in op.kind.source_vregs() {
            if matches!(reg, VReg::Virtual(_)) {
                *virtual_uses.entry(reg).or_insert(0usize) += 1;
            }
        }
    }

    // Generic flag-suppressed ADC/SBB lowering cannot retain a live carry and
    // is rejected by `x86_block_preserves_live_flags`. An exact helper-backed
    // memory RMW sequence is different: its compute is wrapped by PUSHFQ/POPFQ
    // and its post-store replay consumes the same incoming carry. Exempt only
    // that validated compute index from the generic clobber check.
    let mut preserved_clobber_exceptions = std::collections::HashSet::new();
    let mut scan = 0;
    while scan < n {
        if let Some(consumed) = x86_jit_mem_alu_rmw_sequence_len(
            block,
            scan,
            allow_mem,
            &virtual_definitions,
            &virtual_uses,
        ) {
            preserved_clobber_exceptions.insert(scan + 1);
            scan += consumed;
        } else {
            scan += 1;
        }
    }
    if !x86_block_preserves_live_flags(block, flags_live_out, &preserved_clobber_exceptions) {
        return false;
    }

    let mut i = 0;
    while i < n {
        if let Some(consumed) = x86_jit_mem_shift_rmw_sequence_len(
            block,
            i,
            allow_mem,
            &virtual_definitions,
            &virtual_uses,
        ) {
            i += consumed;
            continue;
        }
        if let Some(consumed) = x86_jit_mem_unary_rmw_sequence_len(
            block,
            i,
            allow_mem,
            &virtual_definitions,
            &virtual_uses,
        ) {
            i += consumed;
            continue;
        }
        if let Some(consumed) = x86_jit_mem_alu_rmw_sequence_len(
            block,
            i,
            allow_mem,
            &virtual_definitions,
            &virtual_uses,
        ) {
            i += consumed;
            continue;
        }
        if let Some(consumed) = x86_jit_mem_bit_update_rmw_sequence_len(
            block,
            i,
            allow_mem,
            &virtual_definitions,
            &virtual_uses,
        ) {
            i += consumed;
            continue;
        }
        if let Some(consumed) = x86_jit_mem_alu_source_sequence_len(
            block,
            i,
            allow_mem,
            &virtual_definitions,
            &virtual_uses,
        ) {
            i += consumed;
            continue;
        }
        if let Some(consumed) = x86_jit_mem_bit_test_source_sequence_len(
            block,
            i,
            allow_mem,
            &virtual_definitions,
            &virtual_uses,
        ) {
            i += consumed;
            continue;
        }
        if let Some(consumed) = x86_jit_mem_bit_scan_source_sequence_len(
            block,
            i,
            allow_mem,
            &virtual_definitions,
            &virtual_uses,
        ) {
            i += consumed;
            continue;
        }
        if let Some(consumed) = x86_jit_mem_count_source_sequence_len(
            block,
            i,
            allow_mem,
            &virtual_definitions,
            &virtual_uses,
        ) {
            i += consumed;
            continue;
        }
        if let Some(consumed) =
            x86_jit_pop2_sequence_len(block, i, allow_mem, &virtual_definitions, &virtual_uses)
        {
            i += consumed;
            continue;
        }
        if x86_jit_pop2_candidate(block, i) {
            return false;
        }
        if let Some(consumed) =
            x86_jit_push2_sequence_len(block, i, allow_mem, &virtual_definitions, &virtual_uses)
        {
            i += consumed;
            continue;
        }
        if x86_jit_push2_candidate(block, i) {
            return false;
        }
        if let Some(consumed) =
            x86_jit_pop_sequence_len(block, i, allow_mem, &virtual_definitions, &virtual_uses)
        {
            i += consumed;
            continue;
        }
        if x86_jit_pop_candidate(block, i) {
            return false;
        }
        if let Some(consumed) =
            x86_jit_push_sequence_len(block, i, allow_mem, &virtual_definitions, &virtual_uses)
        {
            i += consumed;
            continue;
        }
        if x86_jit_push_candidate(block, i) {
            return false;
        }
        if x86_mem_crc32_pair_valid(block, i, allow_mem, &virtual_definitions, &virtual_uses) {
            i += 2;
            continue;
        }
        let op = &block.ops[i];
        if i + 1 == n {
            if let (Terminator::CondBranch { cond, .. }, OpKind::TestCondition { dst, .. }) =
                (&block.terminator, &op.kind)
            {
                if dst == cond {
                    i += 1;
                    continue;
                }
            }
        }
        // (1) fail-safe whitelist: any non-whitelisted op (div, general FP/SIMD,
        // syscall, unvalidated) makes the whole region ineligible. When memory
        // JIT is enabled, register-destination Load/Store are additionally
        // allowed (they lower to MMU helper calls with fault-bail); RMW forms
        // still bail via the virtual-temp check below, and RSP/RBP-based
        // addresses via check (3). Explicitly admitted x86 scalar/vector
        // families receive exact shape checks below and, where needed, separate
        // host-feature gates before execution.
        let cldemote_ok = matches!(
            &op.kind,
            OpKind::X86CacheControl {
                addr,
                kind: crate::smir::ir::ops::X86CacheControlKind::Cldemote,
            } if x86_jit_mem_address_shape_valid(addr)
        );
        let alignment_ok = matches!(
            &op.kind,
            OpKind::X86CheckAlignment { addr, alignment }
                if matches!(alignment, 16 | 32 | 64) && x86_jit_mem_address_shape_valid(addr)
        );
        let vector_mem_ok = allow_mem && x86_jit_vector_mem_shape_valid(&op.kind);
        let stack_mov_ok = x86_state_backed_stack_mov_valid(&op.kind);
        let stack_alu_ok = x86_state_backed_stack_alu_valid(&op.kind);
        let stack_state_ok = stack_mov_ok || stack_alu_ok;
        let mem_ok = (allow_mem && matches!(op.kind, OpKind::Load { .. } | OpKind::Store { .. }))
            || cldemote_ok
            || alignment_ok
            || vector_mem_ok;
        let scalar_ok = matches!(
            op.kind,
            OpKind::AndNot { .. } | OpKind::X86Bls { .. } | OpKind::X86Adx { .. }
        );
        let vector_ok = x86_native_vector_smir_op(op);
        if !op.is_jit_safe() && !mem_ok && !scalar_ok && !vector_ok {
            return false;
        }
        if x86_movx_uses_ambiguous_high_byte_source(op) {
            return false;
        }
        if matches!(op.x86_hint, Some(X86OpHint::LegacyHighByteReg))
            && !x86_legacy_high_byte_movx_shape_valid(op)
        {
            return false;
        }
        if matches!(op.x86_hint, Some(X86OpHint::Mulx)) && !x86_mulx_shape_valid(op) {
            return false;
        }
        if matches!(
            op.kind,
            OpKind::MulU {
                dst_hi: Some(_),
                width: crate::smir::ir::types::OpWidth::W16,
                ..
            } | OpKind::MulS {
                dst_hi: Some(_),
                width: crate::smir::ir::types::OpWidth::W16,
                ..
            }
        ) && !x86_word_full_mul_shape_valid(&op.kind)
        {
            return false;
        }
        if matches!(op.kind, OpKind::Bsf { .. } | OpKind::Bsr { .. })
            && !x86_bit_scan_shape_valid(&op.kind)
        {
            return false;
        }
        if matches!(op.kind, OpKind::Crc32C { .. }) && !x86_crc32_shape_valid(&op.kind) {
            return false;
        }
        if let OpKind::X86ReadPid { dst } = &op.kind {
            if !x86_native_rdpid_gpr(dst) {
                return false;
            }
        }
        if matches!(op.kind, OpKind::X86Random { .. }) && !x86_random_shape_valid(&op.kind) {
            return false;
        }
        if matches!(op.kind, OpKind::X86CacheControl { .. }) && !cldemote_ok {
            return false;
        }
        if matches!(op.kind, OpKind::X86CheckAlignment { .. }) && !alignment_ok {
            return false;
        }
        if matches!(op.kind, OpKind::VLoad { .. } | OpKind::VStore { .. }) && !vector_mem_ok {
            return false;
        }
        if matches!(op.kind, OpKind::X86XGetBv { .. }) && !x86_xgetbv_shape_valid(&op.kind) {
            return false;
        }
        if matches!(op.kind, OpKind::X86XSetBv { .. }) {
            if !x86_xsetbv_shape_valid(&op.kind)
                || !block.ops[i + 1..]
                    .iter()
                    .any(|next| next.guest_pc != op.guest_pc)
            {
                return false;
            }
        }
        if let OpKind::Fence { kind } = &op.kind {
            if !matches!(
                *kind,
                crate::smir::ir::types::FenceKind::LoadLoad
                    | crate::smir::ir::types::FenceKind::Full
                    | crate::smir::ir::types::FenceKind::StoreStore
            ) {
                return false;
            }
        }
        if matches!(
            op.kind,
            OpKind::Bt { .. } | OpKind::Bts { .. } | OpKind::Btr { .. } | OpKind::Btc { .. }
        ) && !x86_bit_test_shape_valid(&op.kind)
        {
            return false;
        }
        if matches!(op.kind, OpKind::Cwd { .. }) && !x86_cwd_shape_valid(&op.kind) {
            return false;
        }
        if matches!(op.kind, OpKind::Rcl { .. } | OpKind::Rcr { .. })
            && !x86_carry_rotate_shape_valid(&op.kind)
        {
            return false;
        }
        if matches!(
            op.kind,
            OpKind::AndNot { .. }
                | OpKind::Bextr { .. }
                | OpKind::Bzhi { .. }
                | OpKind::X86Bls { .. }
                | OpKind::Pdep { .. }
                | OpKind::Pext { .. }
        ) && !x86_bmi_shape_valid(&op.kind)
        {
            return false;
        }
        if matches!(op.kind, OpKind::X86Adx { .. }) && !x86_adx_shape_valid(&op.kind) {
            return false;
        }
        if matches!(
            op.kind,
            OpKind::Clz { .. }
                | OpKind::Ctz { .. }
                | OpKind::Popcnt { .. }
                | OpKind::X86Count { .. }
        ) && !x86_count_shape_valid(&op.kind)
        {
            return false;
        }
        if matches!(op.kind, OpKind::Bswap { .. }) && !x86_bswap_shape_valid(&op.kind) {
            return false;
        }
        if matches!(op.kind, OpKind::Xchg { .. }) && !x86_xchg_shape_valid(&op.kind) {
            return false;
        }
        if matches!(op.kind, OpKind::X86NddDoubleShift { .. })
            && !x86_ndd_double_shift_shape_valid(&op.kind)
        {
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
        // (3) guest RSP/RBP. Validated MOV/ADD/SUB reads/writes are state-backed.
        // Other writes are not modeled and bail. A read is additionally valid
        // as an operand of a mem-JIT Load/Store (an address base/index, or a stored value): the MMU
        // helper reads the value from the GuestRegs struct — the current guest
        // RSP/RBP — not the host RSP/RBP. CLDEMOTE is also safe because
        // its architecturally ignorable hint never materializes the address;
        // X86CheckAlignment snapshots live GPRs and computes from GuestRegs.
        // Any OTHER op reading RSP/RBP would use the host frame pointer / host
        // stack (wrong) → bail.
        if !stack_state_ok && op.kind.dests().iter().any(touches_sp_bp) {
            return false;
        }
        if !mem_ok && !stack_state_ok && op.kind.source_vregs().iter().any(touches_sp_bp) {
            return false;
        }
        i += 1;
    }
    true
}

fn x86_aarch64_block_flags_are_representable(
    block: &crate::smir::ir::SmirBlock,
    mut live: crate::smir::ir::flags::FlagSet,
) -> bool {
    use crate::smir::ir::flags::FlagSet;
    use crate::smir::ir::ops::{OpKind, X86AdxKind};

    let unavailable = FlagSet::PF.union(FlagSet::AF);
    for op in block.ops.iter().rev() {
        let uses = x86_flag_uses(&op.kind);
        if !uses.intersection(unavailable).is_empty() {
            return false;
        }

        // Canonical bridge state stores x86 CF directly in NZCV.C. ADC and the
        // rotate/ADX carry chains consume that representation directly. The
        // x86-register SBB lowering normalizes CF around SBC. Unsigned condition
        // evaluation still expects AArch64's no-borrow convention and therefore
        // cannot consume canonical x86 CF without an equivalent normalization.
        if !uses.intersection(FlagSet::CF).is_empty()
            && !matches!(
                op.kind,
                OpKind::Adc { .. }
                    | OpKind::Sbb { .. }
                    | OpKind::Rcl { .. }
                    | OpKind::Rcr { .. }
                    | OpKind::X86Adx {
                        kind: X86AdxKind::Adcx,
                        ..
                    }
                    | OpKind::CmcCF
            )
        {
            return false;
        }

        let defs = x86_flag_defs(&op.kind);
        if !defs.intersection(unavailable).intersection(live).is_empty() {
            return false;
        }
        if !defs.intersection(FlagSet::CF).intersection(live).is_empty()
            && matches!(
                op.kind,
                OpKind::Sub { .. } | OpKind::Neg { .. } | OpKind::Cmp { .. }
            )
        {
            return false;
        }

        live = live.difference(defs).union(uses);
    }
    true
}

fn x86_aarch64_block_is_clobber_safe(
    block: &crate::smir::ir::SmirBlock,
    flags_live_out: crate::smir::ir::flags::FlagSet,
) -> bool {
    use crate::smir::ir::Terminator;
    use crate::smir::ir::ops::{OpKind, X86OpHint};
    use crate::smir::ir::types::VReg;

    if !x86_aarch64_block_flags_are_representable(block, flags_live_out) {
        return false;
    }

    let folded_branch_cond = matches!(
        (&block.terminator, block.ops.last().map(|op| &op.kind)),
        (
            Terminator::CondBranch { cond, .. },
            Some(OpKind::TestCondition { dst, .. })
        ) if cond == dst
    );

    let n = block.ops.len();
    for (index, op) in block.ops.iter().enumerate() {
        if index + 1 == n {
            if let (Terminator::CondBranch { cond, .. }, OpKind::TestCondition { dst, .. }) =
                (&block.terminator, &op.kind)
            {
                if dst == cond {
                    // The lowerer folds this virtual condition result directly
                    // into B.cond, so it does not consume a mapped host GPR.
                    continue;
                }
            }
        }

        if !x86_aarch64_scalar_shape_valid(&op.kind) {
            return false;
        }
        // AH/CH/DH/BH require x86 byte-lane extraction. The generic AArch64
        // register map sees only the parent GPR and cannot infer that lane from
        // the encoding hint, so retain interpreter fallback for these forms.
        if matches!(op.x86_hint, Some(X86OpHint::LegacyHighByteReg)) {
            return false;
        }
        if matches!(op.x86_hint, Some(X86OpHint::Mulx)) && !x86_mulx_shape_valid(op) {
            return false;
        }
        if matches!(
            op.kind,
            OpKind::MulU {
                dst_hi: Some(_),
                width: crate::smir::ir::types::OpWidth::W16,
                ..
            } | OpKind::MulS {
                dst_hi: Some(_),
                width: crate::smir::ir::types::OpWidth::W16,
                ..
            }
        ) && !x86_word_full_mul_shape_valid(&op.kind)
        {
            return false;
        }
        if matches!(op.kind, OpKind::Bsf { .. } | OpKind::Bsr { .. })
            && !x86_bit_scan_shape_valid(&op.kind)
        {
            return false;
        }
        if matches!(op.kind, OpKind::Cwd { .. }) && !x86_cwd_shape_valid(&op.kind) {
            return false;
        }
        if matches!(op.kind, OpKind::Rcl { .. } | OpKind::Rcr { .. })
            && !x86_carry_rotate_shape_valid(&op.kind)
        {
            return false;
        }
        if matches!(
            op.kind,
            OpKind::AndNot { .. }
                | OpKind::Bextr { .. }
                | OpKind::Bzhi { .. }
                | OpKind::X86Bls { .. }
                | OpKind::Pdep { .. }
                | OpKind::Pext { .. }
        ) && !x86_bmi_shape_valid(&op.kind)
        {
            return false;
        }
        if matches!(op.kind, OpKind::X86Adx { .. }) && !x86_adx_shape_valid(&op.kind) {
            return false;
        }
        if matches!(
            op.kind,
            OpKind::Bt { .. } | OpKind::Bts { .. } | OpKind::Btr { .. } | OpKind::Btc { .. }
        ) && !x86_aarch64_bit_test_shape_valid(&op.kind)
        {
            return false;
        }
        if matches!(
            op.kind,
            OpKind::Clz { .. }
                | OpKind::Ctz { .. }
                | OpKind::Popcnt { .. }
                | OpKind::X86Count { .. }
        ) && !x86_count_shape_valid(&op.kind)
        {
            return false;
        }
        if matches!(op.kind, OpKind::Bswap { .. }) && !x86_bswap_shape_valid(&op.kind) {
            return false;
        }
        if matches!(op.kind, OpKind::Xchg { .. }) && !x86_xchg_shape_valid(&op.kind) {
            return false;
        }
        if matches!(op.kind, OpKind::X86NddDoubleShift { .. })
            && !x86_ndd_double_shift_shape_valid(&op.kind)
        {
            return false;
        }

        if op
            .kind
            .dests()
            .iter()
            .any(|dst| !x86_aarch64_legacy_gpr(dst))
            || op
                .kind
                .source_vregs()
                .iter()
                .any(|source| !x86_aarch64_legacy_gpr(source))
        {
            return false;
        }
    }

    // Terminator operands bypass `OpKind::{dests,source_vregs}`. Validate them
    // explicitly so an APX/virtual condition or switch index cannot read an
    // un-marshalled host X16+ register. The trailing TestCondition exception is
    // safe because the lowerer folds it directly into B.cond and never reads
    // its virtual destination.
    match &block.terminator {
        Terminator::Branch { .. } => true,
        Terminator::CondBranch { cond, .. } => {
            folded_branch_cond || matches!(cond, VReg::Imm(_)) || x86_aarch64_legacy_gpr(cond)
        }
        Terminator::Switch { index, .. } => {
            matches!(index, VReg::Imm(_)) || x86_aarch64_legacy_gpr(index)
        }
        Terminator::Return { values } => values.is_empty(),
        _ => false,
    }
}

fn x86_aarch64_legacy_gpr(vreg: &crate::smir::ir::types::VReg) -> bool {
    use crate::smir::ir::types::{ArchReg, VReg, X86Reg};

    matches!(
        vreg,
        VReg::Arch(ArchReg::X86(
            X86Reg::Rax
                | X86Reg::Rcx
                | X86Reg::Rdx
                | X86Reg::Rbx
                | X86Reg::Rsp
                | X86Reg::Rbp
                | X86Reg::Rsi
                | X86Reg::Rdi
                | X86Reg::R8
                | X86Reg::R9
                | X86Reg::R10
                | X86Reg::R11
                | X86Reg::R12
                | X86Reg::R13
                | X86Reg::R14
                | X86Reg::R15
        ))
    )
}

/// Architecture-specific scalar whitelist for the x86 VCPU identity bridge.
///
/// AArch64 W-register writes zero-extend. That is exact for x86 32-bit GPR
/// destinations, but not for 8/16-bit destinations, which preserve the upper
/// bits. Keep every destination-producing operation at W32/W64 unless its
/// lowering has a separately validated x86 partial-write implementation. This
/// explicit match also makes future additions to the shared x86-host whitelist
/// fail closed until their AArch64-host shape is reviewed.
fn x86_aarch64_scalar_shape_valid(op: &crate::smir::ir::ops::OpKind) -> bool {
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::{OpWidth, SrcOperand};

    let full_gpr_write = |width: &OpWidth| matches!(width, OpWidth::W32 | OpWidth::W64);
    let scalar_read_width = |width: &OpWidth| {
        matches!(
            width,
            OpWidth::W8 | OpWidth::W16 | OpWidth::W32 | OpWidth::W64
        )
    };

    match op {
        OpKind::Add { dst, width, .. }
        | OpKind::Sub { dst, width, .. }
        | OpKind::Adc { dst, width, .. }
        | OpKind::Sbb { dst, width, .. }
        | OpKind::Neg { dst, width, .. }
        | OpKind::Inc { dst, width, .. }
        | OpKind::Dec { dst, width, .. }
        | OpKind::And { dst, width, .. }
        | OpKind::Or { dst, width, .. }
        | OpKind::Xor { dst, width, .. }
        | OpKind::Shl { dst, width, .. }
        | OpKind::Shr { dst, width, .. }
        | OpKind::Sar { dst, width, .. }
        | OpKind::Rol { dst, width, .. }
        | OpKind::Ror { dst, width, .. }
        | OpKind::Rcl { dst, width, .. }
        | OpKind::Rcr { dst, width, .. } => {
            full_gpr_write(width)
                || (x86_aarch64_legacy_gpr(dst) && matches!(width, OpWidth::W8 | OpWidth::W16))
        }
        OpKind::X86NddDoubleShift {
            dst,
            amount,
            width,
            flags,
            ..
        } => {
            full_gpr_write(width)
                || (matches!(width, OpWidth::W16)
                    && x86_aarch64_legacy_gpr(dst)
                    && (!flags.updates_any()
                        || matches!(amount, SrcOperand::Imm(value) if (*value as u64 & 0x1f) <= 16)))
        }
        OpKind::Shld {
            dst,
            amount,
            width,
            flags,
            ..
        }
        | OpKind::Shrd {
            dst,
            amount,
            width,
            flags,
            ..
        } => {
            full_gpr_write(width)
                || (matches!(width, OpWidth::W16)
                    && x86_aarch64_legacy_gpr(dst)
                    && (!flags.updates_any()
                        || matches!(amount, SrcOperand::Imm(value) | SrcOperand::Imm64(value) if (*value as u64 & 0x1f) <= 16)))
        }
        OpKind::MulS {
            dst_lo,
            dst_hi,
            width,
            ..
        } => {
            full_gpr_write(width)
                || (matches!(width, OpWidth::W16)
                    && x86_aarch64_legacy_gpr(dst_lo)
                    && dst_hi.as_ref().is_none_or(x86_aarch64_legacy_gpr))
        }
        OpKind::MulU {
            dst_lo,
            dst_hi: Some(dst_hi),
            width: OpWidth::W16,
            ..
        } => x86_aarch64_legacy_gpr(dst_lo) && x86_aarch64_legacy_gpr(dst_hi),
        OpKind::Bsf {
            dst, src, width, ..
        }
        | OpKind::Bsr {
            dst, src, width, ..
        }
        | OpKind::Clz { dst, src, width }
        | OpKind::Ctz { dst, src, width }
        | OpKind::Popcnt { dst, src, width } => {
            full_gpr_write(width)
                || (matches!(width, OpWidth::W16)
                    && x86_aarch64_legacy_gpr(dst)
                    && x86_aarch64_legacy_gpr(src))
        }
        OpKind::X86Count {
            dst, src, width, ..
        } => {
            full_gpr_write(width)
                || (matches!(width, OpWidth::W16)
                    && x86_aarch64_legacy_gpr(dst)
                    && x86_aarch64_legacy_gpr(src))
        }
        OpKind::Crc32C {
            dst,
            crc,
            data,
            data_width,
        } => {
            dst == crc
                && x86_aarch64_legacy_gpr(dst)
                && x86_aarch64_legacy_gpr(data)
                && matches!(
                    data_width,
                    OpWidth::W8 | OpWidth::W16 | OpWidth::W32 | OpWidth::W64
                )
        }
        OpKind::AndNot { width, .. }
        | OpKind::MulU { width, .. }
        | OpKind::Bextr { width, .. }
        | OpKind::Bzhi { width, .. }
        | OpKind::X86Bls { width, .. }
        | OpKind::X86Adx { width, .. }
        | OpKind::Pdep { width, .. }
        | OpKind::Pext { width, .. }
        | OpKind::Bswap { width, .. } => full_gpr_write(width),
        OpKind::Mov { dst, width, .. } => {
            full_gpr_write(width)
                || (x86_aarch64_legacy_gpr(dst) && matches!(width, OpWidth::W8 | OpWidth::W16))
        }
        // Register SETcc is architecturally byte-sized. Legacy high-byte
        // destinations lift through virtual merge temporaries and are rejected
        // by the register/hint checks below; this arm admits low-byte forms.
        OpKind::SetCC { dst, width, .. } => {
            x86_aarch64_legacy_gpr(dst) && matches!(width, OpWidth::W8)
        }
        OpKind::CMove {
            dst, src, width, ..
        } => {
            full_gpr_write(width)
                || (matches!(width, OpWidth::W16)
                    && x86_aarch64_legacy_gpr(dst)
                    && x86_aarch64_legacy_gpr(src))
        }
        OpKind::Not {
            dst, src, width, ..
        } => {
            full_gpr_write(width)
                || (matches!(width, OpWidth::W8 | OpWidth::W16)
                    && dst == src
                    && x86_aarch64_legacy_gpr(dst))
        }
        OpKind::Xchg { reg1, reg2, width } => {
            full_gpr_write(width)
                || (matches!(width, OpWidth::W16)
                    && x86_aarch64_legacy_gpr(reg1)
                    && x86_aarch64_legacy_gpr(reg2))
        }
        OpKind::ZeroExtend {
            dst,
            from_width,
            to_width,
            ..
        }
        | OpKind::SignExtend {
            dst,
            from_width,
            to_width,
            ..
        } => {
            full_gpr_write(to_width)
                || (matches!((from_width, to_width), (OpWidth::W8, OpWidth::W16))
                    && x86_aarch64_legacy_gpr(dst))
        }
        // CWD/CDQ/CQO has dedicated x86 partial-write lowering and native
        // machine regressions for its W8/W16 merge behavior.
        OpKind::Cwd { width, .. } => scalar_read_width(width),
        OpKind::Cmp { width, .. } | OpKind::Test { width, .. } => scalar_read_width(width),
        OpKind::Bt { .. } | OpKind::Bts { .. } | OpKind::Btr { .. } | OpKind::Btc { .. } => {
            x86_aarch64_bit_test_shape_valid(op)
        }
        OpKind::TestCondition { .. }
        | OpKind::Lea { .. }
        | OpKind::SetCF { .. }
        | OpKind::CmcCF
        | OpKind::Nop => true,
        _ => false,
    }
}

fn x86_aarch64_bit_test_shape_valid(op: &crate::smir::ir::ops::OpKind) -> bool {
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::{OpWidth, SrcOperand};

    let index_valid = |index: &SrcOperand| {
        matches!(index, SrcOperand::Imm(_) | SrcOperand::Imm64(_))
            || matches!(index, SrcOperand::Reg(reg) if x86_aarch64_legacy_gpr(reg))
    };
    match op {
        OpKind::Bt { src, index, width } => {
            x86_aarch64_legacy_gpr(src)
                && index_valid(index)
                && matches!(width, OpWidth::W16 | OpWidth::W32 | OpWidth::W64)
        }
        OpKind::Bts {
            dst,
            src,
            index,
            width,
        }
        | OpKind::Btr {
            dst,
            src,
            index,
            width,
        }
        | OpKind::Btc {
            dst,
            src,
            index,
            width,
        } => {
            dst == src
                && x86_aarch64_legacy_gpr(dst)
                && index_valid(index)
                && matches!(width, OpWidth::W16 | OpWidth::W32 | OpWidth::W64)
        }
        _ => false,
    }
}

fn x86_bit_scan_shape_valid(op: &crate::smir::ir::ops::OpKind) -> bool {
    use crate::smir::ir::flags::{FlagSet, FlagUpdate};
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::{ArchReg, OpWidth, VReg, X86Reg};

    let native_gpr = |reg: &VReg| {
        matches!(
            reg,
            VReg::Arch(ArchReg::X86(
                X86Reg::Rax
                    | X86Reg::Rcx
                    | X86Reg::Rdx
                    | X86Reg::Rbx
                    | X86Reg::Rsi
                    | X86Reg::Rdi
                    | X86Reg::R8
                    | X86Reg::R9
                    | X86Reg::R10
                    | X86Reg::R11
                    | X86Reg::R12
                    | X86Reg::R13
                    | X86Reg::R14
                    | X86Reg::R15
            ))
        )
    };

    matches!(
        op,
        OpKind::Bsf {
            dst,
            src,
            width: OpWidth::W16 | OpWidth::W32 | OpWidth::W64,
            flags: FlagUpdate::Specific(FlagSet::ZF),
        } | OpKind::Bsr {
            dst,
            src,
            width: OpWidth::W16 | OpWidth::W32 | OpWidth::W64,
            flags: FlagUpdate::Specific(FlagSet::ZF),
        } if native_gpr(dst) && native_gpr(src)
    )
}

fn x86_bit_test_shape_valid(op: &crate::smir::ir::ops::OpKind) -> bool {
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::{ArchReg, OpWidth, SrcOperand, VReg, X86Reg};

    let native_gpr = |reg: &VReg| {
        matches!(
            reg,
            VReg::Arch(ArchReg::X86(
                X86Reg::Rax
                    | X86Reg::Rcx
                    | X86Reg::Rdx
                    | X86Reg::Rbx
                    | X86Reg::Rsi
                    | X86Reg::Rdi
                    | X86Reg::R8
                    | X86Reg::R9
                    | X86Reg::R10
                    | X86Reg::R11
                    | X86Reg::R12
                    | X86Reg::R13
                    | X86Reg::R14
                    | X86Reg::R15
            ))
        )
    };
    let index_valid = |index: &SrcOperand| {
        matches!(index, SrcOperand::Imm(_) | SrcOperand::Imm64(_))
            || matches!(index, SrcOperand::Reg(reg) if native_gpr(reg))
    };
    let width_valid = |width: &OpWidth| matches!(width, OpWidth::W16 | OpWidth::W32 | OpWidth::W64);

    match op {
        OpKind::Bt { src, index, width } => {
            native_gpr(src) && index_valid(index) && width_valid(width)
        }
        OpKind::Bts {
            dst,
            src,
            index,
            width,
        }
        | OpKind::Btr {
            dst,
            src,
            index,
            width,
        }
        | OpKind::Btc {
            dst,
            src,
            index,
            width,
        } => dst == src && native_gpr(dst) && index_valid(index) && width_valid(width),
        _ => false,
    }
}

fn x86_cwd_shape_valid(op: &crate::smir::ir::ops::OpKind) -> bool {
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::{ArchReg, OpWidth, VReg, X86Reg};

    matches!(
        op,
        OpKind::Cwd {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Rdx)),
            src: VReg::Arch(ArchReg::X86(X86Reg::Rax)),
            width: OpWidth::W16 | OpWidth::W32 | OpWidth::W64,
        }
    )
}

fn x86_carry_rotate_shape_valid(op: &crate::smir::ir::ops::OpKind) -> bool {
    use crate::smir::ir::flags::{FlagSet, FlagUpdate};
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::{ArchReg, OpWidth, SrcOperand, VReg, X86Reg};

    let native_gpr = |reg: &VReg| {
        matches!(
            reg,
            VReg::Arch(ArchReg::X86(
                X86Reg::Rax
                    | X86Reg::Rcx
                    | X86Reg::Rdx
                    | X86Reg::Rbx
                    | X86Reg::Rsi
                    | X86Reg::Rdi
                    | X86Reg::R8
                    | X86Reg::R9
                    | X86Reg::R10
                    | X86Reg::R11
                    | X86Reg::R12
                    | X86Reg::R13
                    | X86Reg::R14
                    | X86Reg::R15
            ))
        )
    };
    let defined_flags = FlagSet::CF.union(FlagSet::OF);

    matches!(
        op,
        OpKind::Rcl {
            dst,
            src,
            amount: SrcOperand::Imm(1),
            width: OpWidth::W8 | OpWidth::W16 | OpWidth::W32 | OpWidth::W64,
            flags: FlagUpdate::Specific(set),
        }
        | OpKind::Rcr {
            dst,
            src,
            amount: SrcOperand::Imm(1),
            width: OpWidth::W8 | OpWidth::W16 | OpWidth::W32 | OpWidth::W64,
            flags: FlagUpdate::Specific(set),
        } if native_gpr(dst) && native_gpr(src) && *set == defined_flags
    )
}

fn x86_bmi_shape_valid(op: &crate::smir::ir::ops::OpKind) -> bool {
    use crate::smir::ir::flags::{FlagSet, FlagUpdate};
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::{ArchReg, OpWidth, SrcOperand, VReg, X86Reg};

    let native_gpr = |reg: &VReg| {
        matches!(
            reg,
            VReg::Arch(ArchReg::X86(
                X86Reg::Rax
                    | X86Reg::Rcx
                    | X86Reg::Rdx
                    | X86Reg::Rbx
                    | X86Reg::Rsi
                    | X86Reg::Rdi
                    | X86Reg::R8
                    | X86Reg::R9
                    | X86Reg::R10
                    | X86Reg::R11
                    | X86Reg::R12
                    | X86Reg::R13
                    | X86Reg::R14
                    | X86Reg::R15
            ))
        )
    };
    let andn_flags = FlagSet::CF
        .union(FlagSet::ZF)
        .union(FlagSet::SF)
        .union(FlagSet::OF);
    let bextr_flags = FlagSet::CF.union(FlagSet::ZF).union(FlagSet::OF);
    let bzhi_flags = FlagSet::CF
        .union(FlagSet::ZF)
        .union(FlagSet::SF)
        .union(FlagSet::OF);

    match op {
        OpKind::AndNot {
            dst,
            src1,
            src2: SrcOperand::Reg(src2),
            width: OpWidth::W32 | OpWidth::W64,
            flags,
        } => {
            native_gpr(dst)
                && native_gpr(src1)
                && native_gpr(src2)
                && (*flags == FlagUpdate::None || *flags == FlagUpdate::Specific(andn_flags))
        }
        OpKind::Bextr {
            dst,
            src,
            control,
            width: OpWidth::W32 | OpWidth::W64,
            flags,
        } => {
            native_gpr(dst)
                && native_gpr(src)
                && native_gpr(control)
                && (*flags == FlagUpdate::None || *flags == FlagUpdate::Specific(bextr_flags))
        }
        OpKind::Bzhi {
            dst,
            src,
            index,
            width: OpWidth::W32 | OpWidth::W64,
            flags,
        } => {
            native_gpr(dst)
                && native_gpr(src)
                && native_gpr(index)
                && (*flags == FlagUpdate::None || *flags == FlagUpdate::Specific(bzhi_flags))
        }
        OpKind::X86Bls {
            dst,
            src,
            width: OpWidth::W32 | OpWidth::W64,
            flags,
            ..
        } => {
            native_gpr(dst)
                && native_gpr(src)
                && (*flags == FlagUpdate::None || *flags == FlagUpdate::Specific(andn_flags))
        }
        OpKind::Pdep {
            dst,
            src,
            mask,
            width: OpWidth::W32 | OpWidth::W64,
        }
        | OpKind::Pext {
            dst,
            src,
            mask,
            width: OpWidth::W32 | OpWidth::W64,
        } => native_gpr(dst) && native_gpr(src) && native_gpr(mask),
        _ => false,
    }
}

fn x86_adx_shape_valid(op: &crate::smir::ir::ops::OpKind) -> bool {
    use crate::smir::ir::flags::{FlagSet, FlagUpdate};
    use crate::smir::ir::ops::{OpKind, X86AdxKind};
    use crate::smir::ir::types::{ArchReg, OpWidth, VReg, X86Reg};

    let native_gpr = |reg: &VReg| {
        matches!(
            reg,
            VReg::Arch(ArchReg::X86(
                X86Reg::Rax
                    | X86Reg::Rcx
                    | X86Reg::Rdx
                    | X86Reg::Rbx
                    | X86Reg::Rsi
                    | X86Reg::Rdi
                    | X86Reg::R8
                    | X86Reg::R9
                    | X86Reg::R10
                    | X86Reg::R11
                    | X86Reg::R12
                    | X86Reg::R13
                    | X86Reg::R14
                    | X86Reg::R15
            ))
        )
    };

    let OpKind::X86Adx {
        dst,
        src1,
        src2,
        width: OpWidth::W32 | OpWidth::W64,
        kind,
        flags,
    } = op
    else {
        return false;
    };
    let output = match kind {
        X86AdxKind::Adcx => FlagSet::CF,
        X86AdxKind::Adox => FlagSet::OF,
    };

    native_gpr(dst)
        && native_gpr(src1)
        && native_gpr(src2)
        && (*flags == FlagUpdate::None || *flags == FlagUpdate::Specific(output))
}

fn x86_count_shape_valid(op: &crate::smir::ir::ops::OpKind) -> bool {
    use crate::smir::ir::flags::FlagSet;
    use crate::smir::ir::ops::{OpKind, X86CountKind};
    use crate::smir::ir::types::{ArchReg, OpWidth, VReg, X86Reg};

    let native_gpr = |reg: &VReg| {
        matches!(
            reg,
            VReg::Arch(ArchReg::X86(
                X86Reg::Rax
                    | X86Reg::Rcx
                    | X86Reg::Rdx
                    | X86Reg::Rbx
                    | X86Reg::Rsi
                    | X86Reg::Rdi
                    | X86Reg::R8
                    | X86Reg::R9
                    | X86Reg::R10
                    | X86Reg::R11
                    | X86Reg::R12
                    | X86Reg::R13
                    | X86Reg::R14
                    | X86Reg::R15
            ))
        )
    };

    let (dst, src, width, flags_valid) = match op {
        OpKind::Clz { dst, src, width }
        | OpKind::Ctz { dst, src, width }
        | OpKind::Popcnt { dst, src, width } => (dst, src, width, true),
        OpKind::X86Count {
            dst,
            src,
            width,
            kind,
            flags,
        } => {
            let architecturally_defined = match kind {
                X86CountKind::Popcnt => FlagSet::ALL_X86,
                X86CountKind::Tzcnt | X86CountKind::Lzcnt => FlagSet::CF.union(FlagSet::ZF),
            };
            (
                dst,
                src,
                width,
                flags
                    .as_set()
                    .difference(architecturally_defined)
                    .is_empty(),
            )
        }
        _ => return false,
    };

    matches!(width, OpWidth::W16 | OpWidth::W32 | OpWidth::W64)
        && native_gpr(dst)
        && native_gpr(src)
        && flags_valid
}

fn x86_bswap_shape_valid(op: &crate::smir::ir::ops::OpKind) -> bool {
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::{ArchReg, OpWidth, VReg, X86Reg};

    let native_gpr = |reg: &VReg| {
        matches!(
            reg,
            VReg::Arch(ArchReg::X86(
                X86Reg::Rax
                    | X86Reg::Rcx
                    | X86Reg::Rdx
                    | X86Reg::Rbx
                    | X86Reg::Rsi
                    | X86Reg::Rdi
                    | X86Reg::R8
                    | X86Reg::R9
                    | X86Reg::R10
                    | X86Reg::R11
                    | X86Reg::R12
                    | X86Reg::R13
                    | X86Reg::R14
                    | X86Reg::R15
            ))
        )
    };

    matches!(
        op,
        OpKind::Bswap {
            dst,
            src,
            width: OpWidth::W16 | OpWidth::W32 | OpWidth::W64,
        } if native_gpr(dst) && native_gpr(src)
    )
}

fn x86_xchg_shape_valid(op: &crate::smir::ir::ops::OpKind) -> bool {
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::{ArchReg, OpWidth, VReg, X86Reg};

    let native_gpr = |reg: &VReg| {
        matches!(
            reg,
            VReg::Arch(ArchReg::X86(
                X86Reg::Rax
                    | X86Reg::Rcx
                    | X86Reg::Rdx
                    | X86Reg::Rbx
                    | X86Reg::Rsi
                    | X86Reg::Rdi
                    | X86Reg::R8
                    | X86Reg::R9
                    | X86Reg::R10
                    | X86Reg::R11
                    | X86Reg::R12
                    | X86Reg::R13
                    | X86Reg::R14
                    | X86Reg::R15
            ))
        )
    };

    matches!(
        op,
        OpKind::Xchg {
            reg1,
            reg2,
            width: OpWidth::W16 | OpWidth::W32 | OpWidth::W64,
        } if native_gpr(reg1) && native_gpr(reg2)
    )
}

fn x86_mulx_shape_valid(op: &crate::smir::ir::ops::SmirOp) -> bool {
    use crate::smir::ir::flags::{FlagSet, FlagUpdate};
    use crate::smir::ir::ops::{OpKind, X86OpHint};
    use crate::smir::ir::types::{ArchReg, OpWidth, SrcOperand, VReg, X86Reg};

    let native_gpr = |reg: &VReg| {
        matches!(
            reg,
            VReg::Arch(ArchReg::X86(
                X86Reg::Rax
                    | X86Reg::Rcx
                    | X86Reg::Rdx
                    | X86Reg::Rbx
                    | X86Reg::Rsi
                    | X86Reg::Rdi
                    | X86Reg::R8
                    | X86Reg::R9
                    | X86Reg::R10
                    | X86Reg::R11
                    | X86Reg::R12
                    | X86Reg::R13
                    | X86Reg::R14
                    | X86Reg::R15
            ))
        )
    };

    matches!(op.x86_hint, Some(X86OpHint::Mulx))
        && matches!(
            &op.kind,
            OpKind::MulU {
                dst_lo,
                dst_hi: Some(dst_hi),
                src1: VReg::Arch(ArchReg::X86(X86Reg::Rdx)),
                src2: SrcOperand::Reg(src2),
                width: OpWidth::W32 | OpWidth::W64,
                flags: FlagUpdate::None,
            } if native_gpr(dst_lo) && native_gpr(dst_hi) && native_gpr(src2)
        )
}

fn x86_word_full_mul_shape_valid(op: &crate::smir::ir::ops::OpKind) -> bool {
    use crate::smir::ir::flags::FlagUpdate;
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::{ArchReg, OpWidth, SrcOperand, VReg, X86Reg};

    matches!(
        op,
        OpKind::MulU {
            dst_lo: VReg::Arch(ArchReg::X86(X86Reg::Rax)),
            dst_hi: Some(VReg::Arch(ArchReg::X86(X86Reg::Rdx))),
            src1: VReg::Arch(ArchReg::X86(X86Reg::Rax)),
            src2: SrcOperand::Reg(src2),
            width: OpWidth::W16,
            flags: FlagUpdate::None,
        }
            | OpKind::MulS {
                dst_lo: VReg::Arch(ArchReg::X86(X86Reg::Rax)),
                dst_hi: Some(VReg::Arch(ArchReg::X86(X86Reg::Rdx))),
                src1: VReg::Arch(ArchReg::X86(X86Reg::Rax)),
                src2: SrcOperand::Reg(src2),
                width: OpWidth::W16,
                flags: FlagUpdate::None,
            } if x86_aarch64_legacy_gpr(src2)
    )
}

fn x86_movx_uses_ambiguous_high_byte_source(op: &crate::smir::ir::ops::SmirOp) -> bool {
    use crate::smir::ir::ops::{OpKind, X86OpHint};
    use crate::smir::ir::types::{ArchReg, OpWidth, VReg, X86Reg};

    if matches!(
        op.x86_hint,
        Some(X86OpHint::RexByteReg | X86OpHint::LegacyHighByteReg)
    ) {
        return false;
    }

    matches!(
        &op.kind,
        OpKind::ZeroExtend {
            src: VReg::Arch(ArchReg::X86(X86Reg::Rsi | X86Reg::Rdi)),
            from_width: OpWidth::W8,
            ..
        } | OpKind::SignExtend {
            src: VReg::Arch(ArchReg::X86(X86Reg::Rsi | X86Reg::Rdi)),
            from_width: OpWidth::W8,
            ..
        }
    )
}

fn x86_legacy_high_byte_movx_shape_valid(op: &crate::smir::ir::ops::SmirOp) -> bool {
    use crate::smir::ir::ops::{OpKind, X86OpHint};
    use crate::smir::ir::types::{ArchReg, OpWidth, VReg, X86Reg};

    let parent = |reg: &VReg| {
        matches!(
            reg,
            VReg::Arch(ArchReg::X86(
                X86Reg::Rax | X86Reg::Rcx | X86Reg::Rdx | X86Reg::Rbx
            ))
        )
    };
    let destination = |reg: &VReg| {
        matches!(
            reg,
            VReg::Arch(ArchReg::X86(
                X86Reg::Rax | X86Reg::Rcx | X86Reg::Rdx | X86Reg::Rbx | X86Reg::Rsi | X86Reg::Rdi
            ))
        )
    };

    matches!(op.x86_hint, Some(X86OpHint::LegacyHighByteReg))
        && matches!(
            &op.kind,
            OpKind::ZeroExtend {
                dst,
                src,
                from_width: OpWidth::W8,
                to_width: OpWidth::W16 | OpWidth::W32,
            } | OpKind::SignExtend {
                dst,
                src,
                from_width: OpWidth::W8,
                to_width: OpWidth::W16 | OpWidth::W32,
            } if parent(src) && destination(dst)
        )
}

fn x86_ndd_double_shift_shape_valid(op: &crate::smir::ir::ops::OpKind) -> bool {
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::{ArchReg, OpWidth, SrcOperand, VReg, X86Reg};
    let OpKind::X86NddDoubleShift {
        dst,
        base,
        fill,
        amount,
        width,
        ..
    } = op
    else {
        return false;
    };
    let native_gpr = |reg: &VReg| {
        matches!(
            reg,
            VReg::Arch(ArchReg::X86(
                X86Reg::Rax
                    | X86Reg::Rcx
                    | X86Reg::Rdx
                    | X86Reg::Rbx
                    | X86Reg::Rsi
                    | X86Reg::Rdi
                    | X86Reg::R8
                    | X86Reg::R9
                    | X86Reg::R10
                    | X86Reg::R11
                    | X86Reg::R12
                    | X86Reg::R13
                    | X86Reg::R14
                    | X86Reg::R15
            ))
        )
    };
    native_gpr(dst)
        && native_gpr(base)
        && native_gpr(fill)
        && matches!(width, OpWidth::W16 | OpWidth::W32 | OpWidth::W64)
        && matches!(
            amount,
            SrcOperand::Imm(_) | SrcOperand::Reg(VReg::Arch(ArchReg::X86(X86Reg::Rcx)))
        )
}

fn x86_native_op_would_clobber_preserved_flags(op: &crate::smir::ir::ops::OpKind) -> bool {
    use crate::smir::ir::flags::FlagUpdate;
    use crate::smir::ir::ops::OpKind;

    matches!(
        op,
        OpKind::Adc {
            flags: FlagUpdate::None,
            ..
        } | OpKind::Sbb {
            flags: FlagUpdate::None,
            ..
        } | OpKind::Shld {
            flags: FlagUpdate::None,
            ..
        } | OpKind::Shrd {
            flags: FlagUpdate::None,
            ..
        }
    )
}

/// Decide whether AArch32-lifted scalar SMIR can execute through the AArch64
/// identity trampoline without exposing host-only state.
///
/// The default contract is deliberately register-only and AArch32-state
/// specific (A32 or T16/T32 without hidden instruction predication):
/// r0-r14 map to W0-W14, r15 is rejected because architectural PC reads are
/// pipeline-relative and writes are control flow, and every data result is
/// W32.  Flag-discarding comparison temporaries are accepted because the
/// lowerer maps them to WZR; all materialized virtual registers are rejected.
/// Direct internal branches are admitted. Conditional branches accept either
/// an AArch32 r0-r14 zero test (Thumb CBZ/CBNZ) or a final `TestCondition` whose
/// virtual destination is consumed only by the terminator; the AArch64 lowerer
/// respectively emits `CBZ`/`CBNZ` or folds the pair into `B.cond`. A direct
/// guest call is admitted only when its final operation writes the exact A32
/// or Thumb link value to r14; callers must pair this gate with
/// `Aarch64Lowerer::set_guest_call_exits(true)` so the call becomes a native
/// frontier exit. Direct and register BLX calls additionally carry an explicit
/// interworking target; callers must enable
/// `Aarch64Lowerer::set_guest_interworking_call_exits(true)`. BLX LR has an
/// exact W32 virtual snapshot before the r14 link write so the old target is
/// consumed in architectural order. A register-indirect terminator is admitted only for an
/// AArch32 r0-r14 target with no speculative target list; callers must pair it
/// with `Aarch64Lowerer::set_guest_indirect_exits(true)`, which records an
/// interworking dispatcher exit and exports target bit 0 as CPSR.T. CFG targets
/// must exist, phi nodes and locals are rejected, and frontier blocks named in
/// `excluded` must still be present for native-exit lowering. Predicated data
/// instructions, Thumb IT state, SIMD/VFP, and other CPSR fields remain
/// interpreter-only. Use
/// [`is_aarch32_aarch64_native_clobber_safe_excluding_with_mem`] to admit the
/// validated scalar memory-helper shapes.
pub fn is_aarch32_aarch64_native_clobber_safe_excluding(
    func: &crate::smir::ir::SmirFunction,
    excluded: &std::collections::HashMap<crate::smir::ir::types::BlockId, u64>,
) -> bool {
    is_aarch32_aarch64_native_clobber_safe_excluding_with_mem(func, excluded, false)
}

/// Memory-aware form of
/// [`is_aarch32_aarch64_native_clobber_safe_excluding`].
///
/// When `allow_mem` is true, scalar B1/B2/B4 loads/stores and B4 load/store
/// pairs are admitted only when every address component and value register is
/// AArch32 r0-r14. Scalar loads additionally admit a frozen absolute address in
/// the 32-bit guest domain for validated A32/T16/T32 literal forms; absolute
/// stores and pairs remain rejected. Pair destinations must be distinct.
/// Callers must pair this gate with `Aarch64Lowerer::set_mem_helpers(true)` and
/// `Aarch64Lowerer::set_mem_helper_addr_width(OpWidth::W32)`.
pub fn is_aarch32_aarch64_native_clobber_safe_excluding_with_mem(
    func: &crate::smir::ir::SmirFunction,
    excluded: &std::collections::HashMap<crate::smir::ir::types::BlockId, u64>,
    allow_mem: bool,
) -> bool {
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::{ArchReg, ArmReg, Condition, OpWidth, SrcOperand, VReg};
    use crate::smir::ir::{CallTarget, Terminator};

    if !func.locals.is_empty()
        || func.get_block(func.entry).is_none()
        || excluded.keys().any(|id| func.get_block(*id).is_none())
    {
        return false;
    }

    let mut block_ids = std::collections::HashSet::with_capacity(func.blocks.len());
    if func.blocks.iter().any(|block| !block_ids.insert(block.id)) {
        return false;
    }

    let target_exists = |target| func.get_block(target).is_some();
    let gpr = |reg: &VReg| matches!(reg, VReg::Arch(ArchReg::Arm(ArmReg::X(index))) if *index < 15);
    func.blocks
        .iter()
        .filter(|block| !excluded.contains_key(&block.id))
        .all(|block| {
            if !block.phis.is_empty() {
                return false;
            }

            let ordinary_ops_valid = |ops: &[crate::smir::ir::ops::SmirOp]| {
                ops.iter()
                    .all(|op| aarch32_aarch64_native_op_shape_valid(&op.kind, allow_mem))
            };

            match &block.terminator {
                Terminator::Return { values } => {
                    values.is_empty() && ordinary_ops_valid(&block.ops)
                }
                Terminator::Branch { target } => {
                    target_exists(*target) && ordinary_ops_valid(&block.ops)
                }
                Terminator::CondBranch {
                    cond,
                    true_target,
                    false_target,
                } => {
                    if gpr(cond) {
                        return target_exists(*true_target)
                            && target_exists(*false_target)
                            && ordinary_ops_valid(&block.ops);
                    }
                    let Some((test, prefix)) = block.ops.split_last() else {
                        return false;
                    };
                    let OpKind::TestCondition {
                        dst,
                        cond: condition,
                    } = &test.kind
                    else {
                        return false;
                    };
                    matches!(cond, VReg::Virtual(_))
                        && dst == cond
                        && !matches!(condition, Condition::Parity | Condition::NoParity)
                        && target_exists(*true_target)
                        && target_exists(*false_target)
                        && ordinary_ops_valid(prefix)
                }
                Terminator::Call {
                    target: CallTarget::GuestAddr(target),
                    args,
                    continuation,
                } => {
                    let Some(continuation_pc) = func
                        .get_block(*continuation)
                        .map(|continuation| continuation.guest_pc)
                    else {
                        return false;
                    };
                    let Some((link, prefix)) = block.ops.split_last() else {
                        return false;
                    };
                    let OpKind::Mov {
                        dst,
                        src: SrcOperand::Imm(link_pc),
                        width: OpWidth::W32,
                    } = &link.kind
                    else {
                        return false;
                    };
                    let arm_link = continuation_pc;
                    let thumb_link = continuation_pc | 1;
                    args.is_empty()
                        && *target <= u64::from(u32::MAX)
                        && *target & 1 == 0
                        && continuation_pc <= u64::from(u32::MAX)
                        && continuation_pc & 1 == 0
                        && *dst == VReg::Arch(ArchReg::Arm(ArmReg::X(14)))
                        && (*link_pc == arm_link as i64 || *link_pc == thumb_link as i64)
                        && ordinary_ops_valid(prefix)
                }
                Terminator::Call {
                    target: CallTarget::GuestAddrInterworking { addr, thumb },
                    args,
                    continuation,
                } => {
                    let Some(continuation_pc) = func
                        .get_block(*continuation)
                        .map(|continuation| continuation.guest_pc)
                    else {
                        return false;
                    };
                    let Some((link, prefix)) = block.ops.split_last() else {
                        return false;
                    };
                    let OpKind::Mov {
                        dst,
                        src: SrcOperand::Imm(link_pc),
                        width: OpWidth::W32,
                    } = &link.kind
                    else {
                        return false;
                    };
                    let expected_link = continuation_pc | u64::from(!*thumb);
                    args.is_empty()
                        && *addr <= u64::from(u32::MAX)
                        && if *thumb {
                            *addr & 1 == 0
                        } else {
                            *addr & 3 == 0
                        }
                        && continuation_pc <= u64::from(u32::MAX)
                        && continuation_pc & 1 == 0
                        && *dst == VReg::Arch(ArchReg::Arm(ArmReg::X(14)))
                        && *link_pc == expected_link as i64
                        && ordinary_ops_valid(prefix)
                }
                Terminator::Call {
                    target: CallTarget::IndirectInterworking(target),
                    args,
                    continuation,
                } => {
                    let Some(continuation_pc) = func
                        .get_block(*continuation)
                        .map(|continuation| continuation.guest_pc)
                    else {
                        return false;
                    };
                    if !args.is_empty()
                        || continuation_pc > u64::from(u32::MAX)
                        || continuation_pc & 1 != 0
                    {
                        return false;
                    }
                    let link_valid = |link: &crate::smir::ir::ops::SmirOp| {
                        matches!(
                            &link.kind,
                            OpKind::Mov {
                                dst: VReg::Arch(ArchReg::Arm(ArmReg::X(14))),
                                src: SrcOperand::Imm(link_pc),
                                width: OpWidth::W32,
                            } if *link_pc == continuation_pc as i64
                                || *link_pc == (continuation_pc | 1) as i64
                        )
                    };
                    match target {
                        VReg::Arch(ArchReg::Arm(ArmReg::X(index))) if *index < 14 => {
                            let Some((link, prefix)) = block.ops.split_last() else {
                                return false;
                            };
                            link_valid(link) && ordinary_ops_valid(prefix)
                        }
                        VReg::Virtual(snapshot) => {
                            let [prefix @ .., snapshot_op, link] = block.ops.as_slice() else {
                                return false;
                            };
                            matches!(
                                &snapshot_op.kind,
                                OpKind::Mov {
                                    dst: VReg::Virtual(id),
                                    src: SrcOperand::Reg(VReg::Arch(ArchReg::Arm(ArmReg::X(14)))),
                                    width: OpWidth::W32,
                                } if id == snapshot
                            ) && link_valid(link)
                                && ordinary_ops_valid(prefix)
                        }
                        _ => false,
                    }
                }
                Terminator::IndirectBranch {
                    target,
                    possible_targets,
                } => possible_targets.is_empty() && gpr(target) && ordinary_ops_valid(&block.ops),
                Terminator::Switch { .. }
                | Terminator::IndirectBranchMem { .. }
                | Terminator::Call { .. }
                | Terminator::TailCall { .. }
                | Terminator::Trap { .. }
                | Terminator::Unreachable => false,
            }
        })
}

fn aarch32_aarch64_native_op_shape_valid(
    op: &crate::smir::ir::ops::OpKind,
    allow_mem: bool,
) -> bool {
    use crate::smir::ir::flags::{FlagSet, FlagUpdate};
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::{
        Address, ArchReg, ArmReg, MemWidth, OpWidth, ShiftOp, SignExtend, SrcOperand, VReg,
    };

    let gpr = |reg: &VReg| matches!(reg, VReg::Arch(ArchReg::Arm(ArmReg::X(index))) if *index < 15);
    let source = |src: &SrcOperand| match src {
        SrcOperand::Imm(_) | SrcOperand::Imm64(_) => true,
        SrcOperand::Reg(reg) => gpr(reg),
        SrcOperand::Shifted { reg, shift, amount } => {
            gpr(reg)
                && *amount < 32
                && !matches!(shift, ShiftOp::Rrx)
                && !(*amount == 0 && matches!(shift, ShiftOp::Lsr | ShiftOp::Asr))
        }
        SrcOperand::Extended { .. } => false,
    };
    let arithmetic_dst = |dst: &VReg, flags: &FlagUpdate| {
        gpr(dst) || (matches!(dst, VReg::Virtual(_)) && flags.updates_any())
    };
    let partial_nz = FlagUpdate::Specific(FlagSet::SF.union(FlagSet::ZF));
    let partial_nzc = FlagUpdate::Specific(FlagSet::SF.union(FlagSet::ZF).union(FlagSet::CF));
    let nzcv = FlagUpdate::Specific(FlagSet::NZCV);
    let register_address = |addr: &Address| match addr {
        Address::Direct(base) | Address::BaseOffset { base, .. } => gpr(base),
        Address::BaseIndexScale {
            base: Some(base),
            index,
            scale: 1 | 2 | 4 | 8,
            ..
        } => gpr(base) && gpr(index),
        _ => false,
    };
    let load_address = |addr: &Address| {
        register_address(addr)
            || matches!(addr, Address::Absolute(address) if *address <= u64::from(u32::MAX))
    };

    match op {
        OpKind::Nop => true,
        OpKind::Mov {
            dst,
            src,
            width: OpWidth::W32,
        } => gpr(dst) && source(src),
        OpKind::Add {
            dst,
            src1,
            src2,
            width: OpWidth::W32,
            flags,
        }
        | OpKind::Sub {
            dst,
            src1,
            src2,
            width: OpWidth::W32,
            flags,
        } => arithmetic_dst(dst, flags) && gpr(src1) && source(src2),
        OpKind::Adc {
            dst,
            src1,
            src2,
            width: OpWidth::W32,
            flags,
        }
        | OpKind::Sbb {
            dst,
            src1,
            src2,
            width: OpWidth::W32,
            flags,
        } => {
            arithmetic_dst(dst, flags)
                && gpr(src1)
                && match src2 {
                    SrcOperand::Imm(_) | SrcOperand::Imm64(_) => true,
                    SrcOperand::Reg(reg) => gpr(reg),
                    SrcOperand::Shifted { .. } | SrcOperand::Extended { .. } => false,
                }
        }
        OpKind::And {
            dst,
            src1,
            src2,
            width: OpWidth::W32,
            flags,
        }
        | OpKind::Or {
            dst,
            src1,
            src2,
            width: OpWidth::W32,
            flags,
        }
        | OpKind::Xor {
            dst,
            src1,
            src2,
            width: OpWidth::W32,
            flags,
        }
        | OpKind::AndNot {
            dst,
            src1,
            src2,
            width: OpWidth::W32,
            flags,
        } => {
            (*flags == FlagUpdate::None || *flags == partial_nz)
                && (gpr(dst) || (*flags == partial_nz && matches!(dst, VReg::Virtual(_))))
                && (gpr(src1)
                    || (*flags == partial_nz
                        && matches!(op, OpKind::AndNot { .. })
                        && matches!(src1, VReg::Imm(-1))
                        && matches!(src2, SrcOperand::Reg(_))))
                && source(src2)
        }
        OpKind::Not {
            dst,
            src,
            width: OpWidth::W32,
        }
        | OpKind::Clz {
            dst,
            src,
            width: OpWidth::W32,
        }
        | OpKind::Rbit {
            dst,
            src,
            width: OpWidth::W32,
        }
        | OpKind::Bswap {
            dst,
            src,
            width: OpWidth::W32,
        } => gpr(dst) && gpr(src),
        OpKind::ArmRegShift {
            dst,
            src,
            amount,
            shift,
            width: OpWidth::W32,
            flags,
        } => {
            gpr(dst)
                && gpr(src)
                && matches!(
                    shift,
                    ShiftOp::Lsl | ShiftOp::Lsr | ShiftOp::Asr | ShiftOp::Ror
                )
                && (*flags == FlagUpdate::None || *flags == partial_nzc)
                && match amount {
                    SrcOperand::Imm(_) | SrcOperand::Imm64(_) => true,
                    SrcOperand::Reg(reg) => gpr(reg),
                    SrcOperand::Shifted { .. } | SrcOperand::Extended { .. } => false,
                }
        }
        OpKind::ArmDpRegShift {
            kind,
            dst,
            rn,
            rm,
            rs,
            shift,
            flags,
        } => {
            (dst.is_some() == kind.writes_result())
                && dst.as_ref().is_none_or(gpr)
                && (rn.is_some() == kind.uses_rn())
                && rn.as_ref().is_none_or(gpr)
                && gpr(rm)
                && gpr(rs)
                && matches!(
                    shift,
                    ShiftOp::Lsl | ShiftOp::Lsr | ShiftOp::Asr | ShiftOp::Ror
                )
                && (*flags == FlagUpdate::None
                    || (kind.is_logical() && *flags == partial_nzc)
                    || (!kind.is_logical() && *flags == nzcv))
        }
        OpKind::Neg {
            dst,
            src,
            width: OpWidth::W32,
            flags,
        } => arithmetic_dst(dst, flags) && gpr(src),
        OpKind::SignExtend {
            dst,
            src,
            from_width: OpWidth::W8 | OpWidth::W16,
            to_width: OpWidth::W32,
        }
        | OpKind::ZeroExtend {
            dst,
            src,
            from_width: OpWidth::W8 | OpWidth::W16,
            to_width: OpWidth::W32,
        } => gpr(dst) && gpr(src),
        OpKind::Shl {
            dst,
            src,
            amount: SrcOperand::Imm(amount),
            width: OpWidth::W32,
            flags,
        }
        | OpKind::Shr {
            dst,
            src,
            amount: SrcOperand::Imm(amount),
            width: OpWidth::W32,
            flags,
        }
        | OpKind::Sar {
            dst,
            src,
            amount: SrcOperand::Imm(amount),
            width: OpWidth::W32,
            flags,
        }
        | OpKind::Ror {
            dst,
            src,
            amount: SrcOperand::Imm(amount),
            width: OpWidth::W32,
            flags,
        } => {
            gpr(dst)
                && gpr(src)
                && ((*flags == FlagUpdate::None && (1..32).contains(amount))
                    || (*flags == partial_nzc
                        && !matches!(op, OpKind::Ror { .. })
                        && (1..=32).contains(amount)))
        }
        OpKind::MulU {
            dst_lo,
            dst_hi,
            src1,
            src2,
            width: OpWidth::W32,
            flags,
        }
        | OpKind::MulS {
            dst_lo,
            dst_hi,
            src1,
            src2,
            width: OpWidth::W32,
            flags,
        } => {
            ((*flags == FlagUpdate::None && dst_hi.as_ref().is_none_or(gpr))
                || (*flags == partial_nz && dst_hi.is_none()))
                && gpr(dst_lo)
                && gpr(src1)
                && source(src2)
                && (*flags == partial_nz || dst_hi.as_ref() != Some(dst_lo))
        }
        OpKind::MulAdd {
            dst,
            acc,
            src1,
            src2,
            width: OpWidth::W32,
        }
        | OpKind::MulSub {
            dst,
            acc,
            src1,
            src2,
            width: OpWidth::W32,
        } => gpr(dst) && gpr(acc) && gpr(src1) && gpr(src2),
        OpKind::DivU {
            quot,
            rem: None,
            src1,
            src2,
            width: OpWidth::W32,
            flags: FlagUpdate::None,
        }
        | OpKind::DivS {
            quot,
            rem: None,
            src1,
            src2,
            width: OpWidth::W32,
            flags: FlagUpdate::None,
        } => gpr(quot) && gpr(src1) && source(src2),
        OpKind::Bfx {
            dst,
            src,
            lsb,
            width_bits,
            op_width: OpWidth::W32,
            ..
        } => {
            gpr(dst)
                && gpr(src)
                && *width_bits != 0
                && u16::from(*lsb) + u16::from(*width_bits) <= 32
        }
        OpKind::Bfi {
            dst,
            dst_in,
            src,
            lsb,
            width_bits,
            op_width: OpWidth::W32,
        } => {
            gpr(dst)
                && gpr(dst_in)
                && gpr(src)
                && *width_bits != 0
                && u16::from(*lsb) + u16::from(*width_bits) <= 32
        }
        OpKind::Load {
            dst,
            addr,
            width,
            sign,
        } => {
            allow_mem
                && gpr(dst)
                && load_address(addr)
                && matches!(
                    (width, sign),
                    (
                        MemWidth::B1 | MemWidth::B2,
                        SignExtend::Zero | SignExtend::Sign
                    ) | (MemWidth::B4, SignExtend::Zero)
                )
        }
        OpKind::Store {
            src,
            addr,
            width: MemWidth::B1 | MemWidth::B2 | MemWidth::B4,
        } => allow_mem && gpr(src) && register_address(addr),
        OpKind::LoadPair {
            dst1,
            dst2,
            addr,
            width: MemWidth::B4,
        } => allow_mem && dst1 != dst2 && gpr(dst1) && gpr(dst2) && register_address(addr),
        OpKind::StorePair {
            src1,
            src2,
            addr,
            width: MemWidth::B4,
        } => allow_mem && gpr(src1) && gpr(src2) && register_address(addr),
        _ => false,
    }
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
    excluded: &std::collections::HashMap<crate::smir::ir::types::BlockId, u64>,
    allow_mem: bool,
) -> bool {
    let blocks = func.blocks.iter().filter(|b| !excluded.contains_key(&b.id));
    let mut uses_fp_trampoline = false;
    let mut uses_mem_helper = false;
    for block in blocks {
        if !aarch64_block_is_clobber_safe(block, allow_mem) {
            return false;
        }
        for op in &block.ops {
            uses_fp_trampoline |= aarch64_op_needs_fp_trampoline(&op.kind);
            uses_mem_helper |= allow_mem && aarch64_mem_helper_op(&op.kind);
        }
    }
    // The FP trampoline keeps guest V0-V31/FPCR/FPSR live in host SIMD/FP
    // state for the whole region, while extern memory helpers may clobber the
    // AAPCS64 caller-saved subset. Keep those paths separate.
    !(uses_fp_trampoline && uses_mem_helper)
}

fn aarch64_mem_helper_op(op: &crate::smir::ir::ops::OpKind) -> bool {
    use crate::smir::ir::ops::OpKind;

    matches!(
        op,
        OpKind::Load { .. } | OpKind::Store { .. } | OpKind::VLoad { .. } | OpKind::VStore { .. }
    )
}

fn aarch64_fp_trampoline_vreg(vreg: &crate::smir::ir::types::VReg) -> bool {
    use crate::smir::ir::types::{ArchReg, ArmReg, VReg};

    matches!(
        vreg,
        VReg::Arch(ArchReg::Arm(ArmReg::V(_) | ArmReg::Fpcr | ArmReg::Fpsr))
    )
}

fn aarch64_fp_sysreg(reg: u32) -> bool {
    const SYSREG_FPCR: u32 = (3 << 14) | (3 << 11) | (4 << 7) | (4 << 3);
    const SYSREG_FPSR: u32 = SYSREG_FPCR | 1;

    matches!(reg, SYSREG_FPCR | SYSREG_FPSR)
}

fn aarch64_op_needs_fp_trampoline(op: &crate::smir::ir::ops::OpKind) -> bool {
    use crate::smir::ir::ops::OpKind;

    let touches_raw_fp_sysreg = match op {
        OpKind::ReadSysReg { reg, .. } | OpKind::WriteSysReg { reg, .. } => aarch64_fp_sysreg(*reg),
        _ => false,
    };

    touches_raw_fp_sysreg
        || op.dests().iter().any(aarch64_fp_trampoline_vreg)
        || op.source_vregs().iter().any(aarch64_fp_trampoline_vreg)
}

fn aarch64_block_is_clobber_safe(block: &crate::smir::ir::SmirBlock, allow_mem: bool) -> bool {
    use crate::smir::ir::Terminator;
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::{ArchReg, ArmReg, VReg};

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
        let mem_ok = allow_mem && aarch64_mem_helper_op(&op.kind);
        // AArch64-clean register-only ops that the x86-tuned `is_jit_safe`
        // whitelist omits: UDIV/SDIV never trap on AArch64 (no x86 `#DE`), and
        // CLZ/RBIT/REV(Bswap)/bitfield insert+extract are pure ALU ops the
        // native lowerer emits correctly (validated by the differential harness
        // in tests/suites/smir/lower/aarch64_native.rs). Admitting them lets the emulator JIT
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
                // NEON three-same vector arithmetic/logic the lowerer emits
                // natively (run via the V-register FP trampoline). Element-type/
                // arrangement forms the lowerer can't handle bail at lower time.
                | OpKind::VAdd { .. }
                | OpKind::VSub { .. }
                | OpKind::VMul { .. }
                | OpKind::VDiv { .. }
                | OpKind::VUnary { .. }
                | OpKind::VReduce { .. }
                | OpKind::VFMinMaxNm { .. }
                | OpKind::VPermute2 { .. }
                | OpKind::VTableLookup { .. }
                | OpKind::VMax { .. }
                | OpKind::VMin { .. }
                | OpKind::VAnd { .. }
                | OpKind::VOr { .. }
                | OpKind::VXor { .. }
                | OpKind::VFma { .. }
        );
        if !op.is_jit_safe() && !a64_ok && !mem_ok {
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

#[cfg(test)]
mod jit_gate_tests {
    use super::*;

    use crate::smir::ir::flags::{FlagSet, FlagUpdate};
    use crate::smir::ir::ops::{
        ArmDpRegShiftKind, OpKind, X86AdxKind, X86BlsKind, X86CacheControlKind, X86CountKind,
        X86OpHint, X86SsePrefix, X86VecMap,
    };
    use crate::smir::ir::types::{
        Address, ArchReg, ArmReg, BlockId, Condition, DispSize, FenceKind, FpPrecision, FunctionId,
        LocalId, MemWidth, OpWidth, ShiftOp, SignExtend, SrcOperand, VLaneOp, VReg, VecCmpCond,
        VecElementType, VecUnaryOp, VecWidth, VirtualId, X86AesOp, X86Reg,
    };
    use crate::smir::ir::{CallTarget, FunctionBuilder, LocalSlot, PhiNode, Terminator};

    fn x86(reg: X86Reg) -> VReg {
        VReg::Arch(ArchReg::X86(reg))
    }

    fn arm_x(n: u8) -> VReg {
        VReg::Arch(ArchReg::Arm(ArmReg::X(n)))
    }

    fn arm_v(n: u8) -> VReg {
        VReg::Arch(ArchReg::Arm(ArmReg::V(n)))
    }

    fn x86_gate(op: OpKind) -> bool {
        let mut b = FunctionBuilder::new(FunctionId(0), 0x1000);
        b.push_op(0x1000, op);
        b.set_terminator(Terminator::Return { values: vec![] });
        is_native_clobber_safe(&b.finish())
    }

    fn aarch64_gate(ops: Vec<OpKind>, allow_mem: bool) -> bool {
        let mut b = FunctionBuilder::new(FunctionId(0), 0x1000);
        for (i, op) in ops.into_iter().enumerate() {
            b.push_op(0x1000 + i as u64 * 4, op);
        }
        b.set_terminator(Terminator::Return { values: vec![] });
        is_aarch64_native_clobber_safe_excluding(
            &b.finish(),
            &std::collections::HashMap::new(),
            allow_mem,
        )
    }

    fn aarch32_gate_with_mem(ops: Vec<OpKind>, allow_mem: bool) -> bool {
        let mut b = FunctionBuilder::new(FunctionId(0), 0x1000);
        for (i, op) in ops.into_iter().enumerate() {
            b.push_op(0x1000 + i as u64 * 4, op);
        }
        b.set_terminator(Terminator::Return { values: vec![] });
        is_aarch32_aarch64_native_clobber_safe_excluding_with_mem(
            &b.finish(),
            &std::collections::HashMap::new(),
            allow_mem,
        )
    }

    fn aarch32_gate(ops: Vec<OpKind>) -> bool {
        aarch32_gate_with_mem(ops, false)
    }

    fn aarch32_cond_cfg(
        test_dst: VReg,
        branch_cond: VReg,
        condition: Condition,
        op_after_test: Option<OpKind>,
    ) -> crate::smir::ir::SmirFunction {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        let true_target = builder.create_block(0x2000);
        let false_target = builder.create_block(0x1004);
        builder.push_op(
            0x1000,
            OpKind::TestCondition {
                dst: test_dst,
                cond: condition,
            },
        );
        if let Some(op) = op_after_test {
            builder.push_op(0x1000, op);
        }
        builder.set_terminator(Terminator::CondBranch {
            cond: branch_cond,
            true_target,
            false_target,
        });
        builder.switch_to_block(true_target);
        builder.set_terminator(Terminator::Return { values: Vec::new() });
        builder.switch_to_block(false_target);
        builder.set_terminator(Terminator::Return { values: Vec::new() });
        builder.finish()
    }

    fn aarch32_call_cfg(
        target: CallTarget,
        link_dst: VReg,
        link_pc: i64,
        link_width: OpWidth,
        args: Vec<VReg>,
        continuation_pc: u64,
    ) -> crate::smir::ir::SmirFunction {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        let continuation = builder.create_block(continuation_pc);
        builder.push_op(
            0x1000,
            OpKind::Mov {
                dst: link_dst,
                src: SrcOperand::Imm(link_pc),
                width: link_width,
            },
        );
        builder.set_terminator(Terminator::Call {
            target,
            args,
            continuation,
        });
        builder.switch_to_block(continuation);
        builder.set_terminator(Terminator::Return { values: Vec::new() });
        builder.finish()
    }

    fn aarch32_indirect_cfg(
        target: VReg,
        possible_targets: Vec<BlockId>,
    ) -> crate::smir::ir::SmirFunction {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.set_terminator(Terminator::IndirectBranch {
            target,
            possible_targets,
        });
        builder.finish()
    }

    fn aarch32_blx_lr_cfg(
        snapshot_dst: VReg,
        snapshot_src: VReg,
        call_target: VReg,
        link_pc: i64,
        args: Vec<VReg>,
    ) -> crate::smir::ir::SmirFunction {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        let continuation = builder.create_block(0x1004);
        builder.push_op(
            0x1000,
            OpKind::Mov {
                dst: snapshot_dst,
                src: SrcOperand::Reg(snapshot_src),
                width: OpWidth::W32,
            },
        );
        builder.push_op(
            0x1000,
            OpKind::Mov {
                dst: arm_x(14),
                src: SrcOperand::Imm(link_pc),
                width: OpWidth::W32,
            },
        );
        builder.set_terminator(Terminator::Call {
            target: CallTarget::IndirectInterworking(call_target),
            args,
            continuation,
        });
        builder.switch_to_block(continuation);
        builder.set_terminator(Terminator::Return { values: Vec::new() });
        builder.finish()
    }

    #[test]
    fn aarch64_guest_state_layout_matches_native_exit_offsets() {
        assert_eq!(
            std::mem::offset_of!(Aarch64GuestRegs, pc),
            Aarch64GuestRegs::PC_OFFSET as usize
        );
        assert_eq!(
            std::mem::offset_of!(Aarch64GuestRegs, vec_store_fn),
            Aarch64GuestRegs::VEC_STORE_FN_OFFSET as usize
        );
        assert_eq!(
            std::mem::offset_of!(Aarch64GuestRegs, exit_flags),
            Aarch64GuestRegs::EXIT_FLAGS_OFFSET as usize
        );
        assert_eq!(Aarch64GuestRegs::EXIT_FLAGS_OFFSET, 864);
        assert_eq!(std::mem::size_of::<Aarch64GuestRegs>(), 872);
        assert_eq!(Aarch64GuestRegs::EXIT_VALID, 1);
        assert_eq!(Aarch64GuestRegs::EXIT_AARCH32_T, 2);
        assert_eq!(Aarch64GuestRegs::EXIT_AARCH32_T_VALID, 4);
    }

    #[test]
    fn aarch32_aarch64_gate_accepts_closed_direct_cfg_and_exact_folded_condition() {
        let mut branch = FunctionBuilder::new(FunctionId(0), 0x1000);
        let exit = branch.create_block(0x2000);
        branch.set_terminator(Terminator::Branch { target: exit });
        branch.switch_to_block(exit);
        branch.set_terminator(Terminator::Return { values: Vec::new() });
        assert!(is_aarch32_aarch64_native_clobber_safe_excluding(
            &branch.finish(),
            &std::collections::HashMap::new(),
        ));

        let cond = VReg::Virtual(VirtualId(7));
        let function = aarch32_cond_cfg(cond, cond, Condition::Ne, None);
        assert!(is_aarch32_aarch64_native_clobber_safe_excluding(
            &function,
            &std::collections::HashMap::new(),
        ));

        let excluded = std::collections::HashMap::from([
            (function.blocks[1].id, function.blocks[1].guest_pc),
            (function.blocks[2].id, function.blocks[2].guest_pc),
        ]);
        assert!(is_aarch32_aarch64_native_clobber_safe_excluding(
            &function, &excluded,
        ));

        let mut zero_test = FunctionBuilder::new(FunctionId(0), 0x1000);
        let nonzero = zero_test.create_block(0x1002);
        let zero = zero_test.create_block(0x1006);
        zero_test.set_terminator(Terminator::CondBranch {
            cond: arm_x(7),
            true_target: nonzero,
            false_target: zero,
        });
        zero_test.switch_to_block(nonzero);
        zero_test.set_terminator(Terminator::Return { values: Vec::new() });
        zero_test.switch_to_block(zero);
        zero_test.set_terminator(Terminator::Return { values: Vec::new() });
        assert!(is_aarch32_aarch64_native_clobber_safe_excluding(
            &zero_test.finish(),
            &std::collections::HashMap::new(),
        ));

        for link_pc in [0x1004, 0x1005] {
            let call = aarch32_call_cfg(
                CallTarget::GuestAddr(0x2000),
                arm_x(14),
                link_pc,
                OpWidth::W32,
                Vec::new(),
                0x1004,
            );
            assert!(is_aarch32_aarch64_native_clobber_safe_excluding(
                &call,
                &std::collections::HashMap::new(),
            ));
        }

        for (target, link_pc) in [
            (
                CallTarget::GuestAddrInterworking {
                    addr: 0x2002,
                    thumb: true,
                },
                0x1004,
            ),
            (
                CallTarget::GuestAddrInterworking {
                    addr: 0x2000,
                    thumb: false,
                },
                0x1005,
            ),
            (CallTarget::IndirectInterworking(arm_x(0)), 0x1004),
            (CallTarget::IndirectInterworking(arm_x(13)), 0x1005),
        ] {
            assert!(is_aarch32_aarch64_native_clobber_safe_excluding(
                &aarch32_call_cfg(target, arm_x(14), link_pc, OpWidth::W32, Vec::new(), 0x1004,),
                &std::collections::HashMap::new(),
            ));
        }
        let snapshot = VReg::Virtual(VirtualId(11));
        assert!(is_aarch32_aarch64_native_clobber_safe_excluding(
            &aarch32_blx_lr_cfg(snapshot, arm_x(14), snapshot, 0x1004, Vec::new()),
            &std::collections::HashMap::new(),
        ));

        for target in [arm_x(0), arm_x(7), arm_x(14)] {
            let indirect = aarch32_indirect_cfg(target, Vec::new());
            assert!(is_aarch32_aarch64_native_clobber_safe_excluding(
                &indirect,
                &std::collections::HashMap::new(),
            ));
        }
    }

    #[test]
    fn aarch32_aarch64_gate_rejects_malformed_or_stateful_cfg_shapes() {
        let cond = VReg::Virtual(VirtualId(7));
        let other = VReg::Virtual(VirtualId(8));
        for function in [
            aarch32_cond_cfg(other, cond, Condition::Eq, None),
            aarch32_cond_cfg(cond, arm_x(0), Condition::Eq, None),
            aarch32_cond_cfg(cond, cond, Condition::Parity, None),
            aarch32_cond_cfg(cond, cond, Condition::NoParity, None),
            aarch32_cond_cfg(cond, cond, Condition::Eq, Some(OpKind::Nop)),
        ] {
            assert!(!is_aarch32_aarch64_native_clobber_safe_excluding(
                &function,
                &std::collections::HashMap::new(),
            ));
        }

        let malformed_calls = [
            aarch32_call_cfg(
                CallTarget::GuestAddr(0x2000),
                arm_x(14),
                0x1006,
                OpWidth::W32,
                Vec::new(),
                0x1004,
            ),
            aarch32_call_cfg(
                CallTarget::GuestAddr(0x2000),
                arm_x(13),
                0x1004,
                OpWidth::W32,
                Vec::new(),
                0x1004,
            ),
            aarch32_call_cfg(
                CallTarget::GuestAddr(0x2000),
                arm_x(14),
                0x1004,
                OpWidth::W64,
                Vec::new(),
                0x1004,
            ),
            aarch32_call_cfg(
                CallTarget::GuestAddr(0x2000),
                arm_x(14),
                0x1004,
                OpWidth::W32,
                vec![arm_x(0)],
                0x1004,
            ),
            aarch32_call_cfg(
                CallTarget::GuestAddr(0x2001),
                arm_x(14),
                0x1004,
                OpWidth::W32,
                Vec::new(),
                0x1004,
            ),
            aarch32_call_cfg(
                CallTarget::GuestAddr(u64::from(u32::MAX) + 1),
                arm_x(14),
                0x1004,
                OpWidth::W32,
                Vec::new(),
                0x1004,
            ),
            aarch32_call_cfg(
                CallTarget::Direct(FunctionId(9)),
                arm_x(14),
                0x1004,
                OpWidth::W32,
                Vec::new(),
                0x1004,
            ),
        ];
        for call in malformed_calls {
            assert!(!is_aarch32_aarch64_native_clobber_safe_excluding(
                &call,
                &std::collections::HashMap::new(),
            ));
        }

        let malformed_interworking_calls = [
            aarch32_call_cfg(
                CallTarget::GuestAddrInterworking {
                    addr: 0x2001,
                    thumb: true,
                },
                arm_x(14),
                0x1004,
                OpWidth::W32,
                Vec::new(),
                0x1004,
            ),
            aarch32_call_cfg(
                CallTarget::GuestAddrInterworking {
                    addr: 0x2002,
                    thumb: false,
                },
                arm_x(14),
                0x1005,
                OpWidth::W32,
                Vec::new(),
                0x1004,
            ),
            aarch32_call_cfg(
                CallTarget::GuestAddrInterworking {
                    addr: 0x2000,
                    thumb: true,
                },
                arm_x(14),
                0x1005,
                OpWidth::W32,
                Vec::new(),
                0x1004,
            ),
            aarch32_call_cfg(
                CallTarget::IndirectInterworking(arm_x(14)),
                arm_x(14),
                0x1004,
                OpWidth::W32,
                Vec::new(),
                0x1004,
            ),
            aarch32_call_cfg(
                CallTarget::IndirectInterworking(arm_x(15)),
                arm_x(14),
                0x1004,
                OpWidth::W32,
                Vec::new(),
                0x1004,
            ),
        ];
        for call in malformed_interworking_calls {
            assert!(!is_aarch32_aarch64_native_clobber_safe_excluding(
                &call,
                &std::collections::HashMap::new(),
            ));
        }

        let snapshot = VReg::Virtual(VirtualId(11));
        for call in [
            aarch32_blx_lr_cfg(snapshot, arm_x(13), snapshot, 0x1004, Vec::new()),
            aarch32_blx_lr_cfg(
                snapshot,
                arm_x(14),
                VReg::Virtual(VirtualId(12)),
                0x1004,
                Vec::new(),
            ),
            aarch32_blx_lr_cfg(snapshot, arm_x(14), snapshot, 0x1006, Vec::new()),
            aarch32_blx_lr_cfg(snapshot, arm_x(14), snapshot, 0x1004, vec![arm_x(0)]),
        ] {
            assert!(!is_aarch32_aarch64_native_clobber_safe_excluding(
                &call,
                &std::collections::HashMap::new(),
            ));
        }

        for indirect in [
            aarch32_indirect_cfg(arm_x(15), Vec::new()),
            aarch32_indirect_cfg(VReg::Virtual(VirtualId(9)), Vec::new()),
            aarch32_indirect_cfg(arm_x(0), vec![BlockId(1)]),
        ] {
            assert!(!is_aarch32_aarch64_native_clobber_safe_excluding(
                &indirect,
                &std::collections::HashMap::new(),
            ));
        }

        let mut missing_test = FunctionBuilder::new(FunctionId(0), 0x1000);
        let true_target = missing_test.create_block(0x2000);
        let false_target = missing_test.create_block(0x1004);
        missing_test.set_terminator(Terminator::CondBranch {
            cond,
            true_target,
            false_target,
        });
        missing_test.switch_to_block(true_target);
        missing_test.set_terminator(Terminator::Return { values: Vec::new() });
        missing_test.switch_to_block(false_target);
        missing_test.set_terminator(Terminator::Return { values: Vec::new() });
        assert!(!is_aarch32_aarch64_native_clobber_safe_excluding(
            &missing_test.finish(),
            &std::collections::HashMap::new(),
        ));

        let mut missing_target = FunctionBuilder::new(FunctionId(0), 0x1000);
        missing_target.set_terminator(Terminator::Branch {
            target: BlockId(u32::MAX),
        });
        assert!(!is_aarch32_aarch64_native_clobber_safe_excluding(
            &missing_target.finish(),
            &std::collections::HashMap::new(),
        ));

        let mut nonempty_return = FunctionBuilder::new(FunctionId(0), 0x1000);
        nonempty_return.set_terminator(Terminator::Return {
            values: vec![arm_x(0)],
        });
        assert!(!is_aarch32_aarch64_native_clobber_safe_excluding(
            &nonempty_return.finish(),
            &std::collections::HashMap::new(),
        ));

        let mut structural = aarch32_cond_cfg(cond, cond, Condition::Eq, None);
        let predecessor = structural.blocks[1].id;
        structural.blocks[0].phis.push(PhiNode {
            dst: cond,
            sources: vec![(predecessor, arm_x(0))],
        });
        assert!(!is_aarch32_aarch64_native_clobber_safe_excluding(
            &structural,
            &std::collections::HashMap::new(),
        ));
        structural.blocks[0].phis.clear();
        structural.locals.push(LocalSlot {
            id: LocalId(0),
            size: 4,
            align: 4,
        });
        assert!(!is_aarch32_aarch64_native_clobber_safe_excluding(
            &structural,
            &std::collections::HashMap::new(),
        ));
        structural.locals.clear();
        structural.blocks.push(structural.blocks[0].clone());
        assert!(!is_aarch32_aarch64_native_clobber_safe_excluding(
            &structural,
            &std::collections::HashMap::new(),
        ));

        let function = aarch32_cond_cfg(cond, cond, Condition::Eq, None);
        let nonexistent_exit = std::collections::HashMap::from([(BlockId(u32::MAX), 0x3000)]);
        assert!(!is_aarch32_aarch64_native_clobber_safe_excluding(
            &function,
            &nonexistent_exit,
        ));
        let mut missing_entry = function;
        missing_entry.entry = BlockId(u32::MAX);
        assert!(!is_aarch32_aarch64_native_clobber_safe_excluding(
            &missing_entry,
            &std::collections::HashMap::new(),
        ));
    }

    #[test]
    fn aarch32_aarch64_gate_accepts_scalar_w32_matrix_and_rejects_hidden_state() {
        assert!(aarch32_gate(vec![
            OpKind::Mov {
                dst: arm_x(0),
                src: SrcOperand::Imm(0x1234),
                width: OpWidth::W32,
            },
            OpKind::Add {
                dst: arm_x(1),
                src1: arm_x(2),
                src2: SrcOperand::Shifted {
                    reg: arm_x(3),
                    shift: ShiftOp::Lsl,
                    amount: 7,
                },
                width: OpWidth::W32,
                flags: FlagUpdate::All,
            },
            OpKind::Sub {
                dst: VReg::Virtual(VirtualId(0)),
                src1: arm_x(4),
                src2: SrcOperand::Reg(arm_x(5)),
                width: OpWidth::W32,
                flags: FlagUpdate::All,
            },
            OpKind::MulAdd {
                dst: arm_x(6),
                acc: arm_x(7),
                src1: arm_x(8),
                src2: arm_x(9),
                width: OpWidth::W32,
            },
            OpKind::Clz {
                dst: arm_x(10),
                src: arm_x(11),
                width: OpWidth::W32,
            },
            OpKind::Bswap {
                dst: arm_x(12),
                src: arm_x(14),
                width: OpWidth::W32,
            },
            OpKind::Neg {
                dst: arm_x(4),
                src: arm_x(5),
                width: OpWidth::W32,
                flags: FlagUpdate::All,
            },
            OpKind::SignExtend {
                dst: arm_x(0),
                src: arm_x(1),
                from_width: OpWidth::W8,
                to_width: OpWidth::W32,
            },
            OpKind::ZeroExtend {
                dst: arm_x(2),
                src: arm_x(3),
                from_width: OpWidth::W16,
                to_width: OpWidth::W32,
            },
        ]));

        for rejected in [
            OpKind::Mov {
                dst: arm_x(15),
                src: SrcOperand::Reg(arm_x(0)),
                width: OpWidth::W32,
            },
            OpKind::Mov {
                dst: arm_x(0),
                src: SrcOperand::Reg(arm_x(15)),
                width: OpWidth::W32,
            },
            OpKind::Mov {
                dst: arm_x(0),
                src: SrcOperand::Reg(arm_x(1)),
                width: OpWidth::W64,
            },
            OpKind::Mov {
                dst: VReg::Virtual(VirtualId(0)),
                src: SrcOperand::Reg(arm_x(1)),
                width: OpWidth::W32,
            },
            OpKind::And {
                dst: arm_x(0),
                src1: arm_x(1),
                src2: SrcOperand::Reg(arm_x(2)),
                width: OpWidth::W32,
                flags: FlagUpdate::All,
            },
            OpKind::Mov {
                dst: arm_x(0),
                src: SrcOperand::Shifted {
                    reg: arm_x(1),
                    shift: ShiftOp::Rrx,
                    amount: 0,
                },
                width: OpWidth::W32,
            },
            OpKind::SignExtend {
                dst: arm_x(0),
                src: arm_x(1),
                from_width: OpWidth::W32,
                to_width: OpWidth::W64,
            },
            OpKind::Adc {
                dst: arm_x(0),
                src1: arm_x(1),
                src2: SrcOperand::Shifted {
                    reg: arm_x(2),
                    shift: ShiftOp::Lsl,
                    amount: 0,
                },
                width: OpWidth::W32,
                flags: FlagUpdate::None,
            },
        ] {
            assert!(!aarch32_gate(vec![rejected.clone()]), "{rejected:?}");
        }
    }

    #[test]
    fn aarch32_aarch64_gate_admits_selective_nzcv_and_independent_register_shifts() {
        let nz = FlagUpdate::Specific(FlagSet::SF.union(FlagSet::ZF));
        let nzc = FlagUpdate::Specific(FlagSet::SF.union(FlagSet::ZF).union(FlagSet::CF));
        let mut accepted = Vec::new();
        for kind in 0..4 {
            accepted.push(match kind {
                0 => OpKind::And {
                    dst: arm_x(0),
                    src1: arm_x(0),
                    src2: SrcOperand::Imm(-1),
                    width: OpWidth::W32,
                    flags: nz,
                },
                1 => OpKind::Or {
                    dst: arm_x(1),
                    src1: arm_x(1),
                    src2: SrcOperand::Reg(arm_x(2)),
                    width: OpWidth::W32,
                    flags: nz,
                },
                2 => OpKind::Xor {
                    dst: VReg::Virtual(VirtualId(9)),
                    src1: arm_x(3),
                    src2: SrcOperand::Reg(arm_x(4)),
                    width: OpWidth::W32,
                    flags: nz,
                },
                _ => OpKind::AndNot {
                    dst: arm_x(5),
                    src1: VReg::Imm(-1),
                    src2: SrcOperand::Reg(arm_x(6)),
                    width: OpWidth::W32,
                    flags: nz,
                },
            });
        }
        accepted.extend([
            OpKind::MulU {
                dst_lo: arm_x(7),
                dst_hi: None,
                src1: arm_x(7),
                src2: SrcOperand::Reg(arm_x(0)),
                width: OpWidth::W32,
                flags: nz,
            },
            OpKind::Shl {
                dst: arm_x(8),
                src: arm_x(9),
                amount: SrcOperand::Imm(1),
                width: OpWidth::W32,
                flags: nzc,
            },
            OpKind::Shr {
                dst: arm_x(10),
                src: arm_x(11),
                amount: SrcOperand::Imm(32),
                width: OpWidth::W32,
                flags: nzc,
            },
            OpKind::Sar {
                dst: arm_x(12),
                src: arm_x(13),
                amount: SrcOperand::Imm(32),
                width: OpWidth::W32,
                flags: nzc,
            },
        ]);
        for shift in [ShiftOp::Lsl, ShiftOp::Lsr, ShiftOp::Asr, ShiftOp::Ror] {
            accepted.push(OpKind::ArmRegShift {
                dst: arm_x(0),
                src: arm_x(0),
                amount: if shift == ShiftOp::Ror {
                    SrcOperand::Imm(0x120)
                } else {
                    SrcOperand::Reg(arm_x(1))
                },
                shift,
                width: OpWidth::W32,
                flags: nzc,
            });
        }
        accepted.push(OpKind::ArmRegShift {
            dst: arm_x(2),
            src: arm_x(4),
            amount: SrcOperand::Reg(arm_x(3)),
            shift: ShiftOp::Lsl,
            width: OpWidth::W32,
            flags: FlagUpdate::None,
        });
        assert!(aarch32_gate(accepted));

        let bad_nz = FlagUpdate::Specific(FlagSet::ZF);
        for rejected in [
            OpKind::And {
                dst: arm_x(0),
                src1: VReg::Imm(-1),
                src2: SrcOperand::Reg(arm_x(2)),
                width: OpWidth::W32,
                flags: nz,
            },
            OpKind::And {
                dst: arm_x(0),
                src1: arm_x(1),
                src2: SrcOperand::Reg(arm_x(2)),
                width: OpWidth::W32,
                flags: bad_nz,
            },
            OpKind::MulU {
                dst_lo: arm_x(0),
                dst_hi: Some(arm_x(1)),
                src1: arm_x(2),
                src2: SrcOperand::Reg(arm_x(3)),
                width: OpWidth::W32,
                flags: nz,
            },
            OpKind::Shl {
                dst: arm_x(0),
                src: arm_x(1),
                amount: SrcOperand::Imm(0),
                width: OpWidth::W32,
                flags: nzc,
            },
            OpKind::Shr {
                dst: arm_x(0),
                src: arm_x(1),
                amount: SrcOperand::Imm(33),
                width: OpWidth::W32,
                flags: nzc,
            },
            OpKind::Ror {
                dst: arm_x(0),
                src: arm_x(1),
                amount: SrcOperand::Imm(1),
                width: OpWidth::W32,
                flags: nzc,
            },
            OpKind::ArmRegShift {
                dst: arm_x(0),
                src: arm_x(0),
                amount: SrcOperand::Reg(arm_x(2)),
                shift: ShiftOp::Rrx,
                width: OpWidth::W32,
                flags: nzc,
            },
            OpKind::ArmRegShift {
                dst: arm_x(0),
                src: arm_x(15),
                amount: SrcOperand::Reg(arm_x(2)),
                shift: ShiftOp::Lsl,
                width: OpWidth::W32,
                flags: nzc,
            },
            OpKind::ArmRegShift {
                dst: arm_x(15),
                src: arm_x(0),
                amount: SrcOperand::Reg(arm_x(2)),
                shift: ShiftOp::Lsl,
                width: OpWidth::W32,
                flags: nzc,
            },
            OpKind::ArmRegShift {
                dst: arm_x(0),
                src: arm_x(0),
                amount: SrcOperand::Shifted {
                    reg: arm_x(2),
                    shift: ShiftOp::Lsl,
                    amount: 1,
                },
                shift: ShiftOp::Lsr,
                width: OpWidth::W32,
                flags: nzc,
            },
            OpKind::ArmRegShift {
                dst: arm_x(0),
                src: arm_x(0),
                amount: SrcOperand::Reg(arm_x(15)),
                shift: ShiftOp::Asr,
                width: OpWidth::W32,
                flags: nzc,
            },
            OpKind::ArmRegShift {
                dst: arm_x(0),
                src: arm_x(0),
                amount: SrcOperand::Imm(1),
                shift: ShiftOp::Ror,
                width: OpWidth::W64,
                flags: nzc,
            },
            OpKind::ArmRegShift {
                dst: arm_x(0),
                src: arm_x(0),
                amount: SrcOperand::Imm(1),
                shift: ShiftOp::Ror,
                width: OpWidth::W32,
                flags: bad_nz,
            },
        ] {
            assert!(!aarch32_gate(vec![rejected.clone()]), "{rejected:?}");
        }
    }

    #[test]
    fn aarch32_aarch64_gate_exactly_validates_data_processing_register_shifts() {
        let nzc = FlagSet::SF.union(FlagSet::ZF).union(FlagSet::CF);
        let mut accepted = Vec::new();
        for opcode in 0_u8..16 {
            let kind = ArmDpRegShiftKind::from_opcode(opcode).unwrap();
            for shift in [ShiftOp::Lsl, ShiftOp::Lsr, ShiftOp::Asr, ShiftOp::Ror] {
                for flags in [
                    FlagUpdate::None,
                    FlagUpdate::Specific(if kind.is_logical() {
                        nzc
                    } else {
                        FlagSet::NZCV
                    }),
                ] {
                    accepted.push(OpKind::ArmDpRegShift {
                        kind,
                        dst: kind.writes_result().then(|| arm_x(14)),
                        rn: kind.uses_rn().then(|| arm_x(13)),
                        rm: arm_x(12),
                        rs: arm_x(11),
                        shift,
                        flags,
                    });
                }
            }
        }
        assert!(aarch32_gate(accepted));

        let valid_add = || OpKind::ArmDpRegShift {
            kind: ArmDpRegShiftKind::Add,
            dst: Some(arm_x(0)),
            rn: Some(arm_x(1)),
            rm: arm_x(2),
            rs: arm_x(3),
            shift: ShiftOp::Lsl,
            flags: FlagUpdate::Specific(FlagSet::NZCV),
        };
        let mut rejected = Vec::new();
        for mutate in 0..10 {
            let mut op = valid_add();
            let OpKind::ArmDpRegShift {
                dst,
                rn,
                rm,
                rs,
                shift,
                flags,
                ..
            } = &mut op
            else {
                unreachable!()
            };
            match mutate {
                0 => *dst = None,
                1 => *dst = Some(arm_x(15)),
                2 => *rn = None,
                3 => *rn = Some(arm_x(15)),
                4 => *rm = arm_x(15),
                5 => *rs = arm_x(15),
                6 => *shift = ShiftOp::Rrx,
                7 => *flags = FlagUpdate::Specific(nzc),
                8 => *flags = FlagUpdate::All,
                9 => *dst = Some(VReg::virt(0)),
                _ => unreachable!(),
            }
            rejected.push(op);
        }
        rejected.extend([
            OpKind::ArmDpRegShift {
                kind: ArmDpRegShiftKind::Tst,
                dst: Some(arm_x(15)),
                rn: Some(arm_x(1)),
                rm: arm_x(2),
                rs: arm_x(3),
                shift: ShiftOp::Lsr,
                flags: FlagUpdate::Specific(nzc),
            },
            OpKind::ArmDpRegShift {
                kind: ArmDpRegShiftKind::Mov,
                dst: Some(arm_x(0)),
                rn: Some(arm_x(15)),
                rm: arm_x(2),
                rs: arm_x(3),
                shift: ShiftOp::Ror,
                flags: FlagUpdate::Specific(nzc),
            },
            OpKind::ArmDpRegShift {
                kind: ArmDpRegShiftKind::And,
                dst: Some(arm_x(0)),
                rn: Some(arm_x(1)),
                rm: arm_x(2),
                rs: arm_x(3),
                shift: ShiftOp::Asr,
                flags: FlagUpdate::Specific(FlagSet::NZCV),
            },
        ]);
        for op in rejected {
            assert!(!aarch32_gate(vec![op.clone()]), "{op:?}");
        }
    }

    #[test]
    fn aarch32_aarch64_gate_admits_only_bounded_scalar_memory_shapes() {
        let valid = vec![
            OpKind::Load {
                dst: arm_x(12),
                addr: Address::Absolute(0xffff_fffc),
                width: MemWidth::B2,
                sign: SignExtend::Sign,
            },
            OpKind::Load {
                dst: arm_x(0),
                addr: Address::BaseOffset {
                    base: arm_x(13),
                    offset: -4,
                    disp_size: DispSize::Auto,
                },
                width: MemWidth::B4,
                sign: SignExtend::Zero,
            },
            OpKind::Load {
                dst: arm_x(1),
                addr: Address::BaseIndexScale {
                    base: Some(arm_x(2)),
                    index: arm_x(3),
                    scale: 4,
                    disp: 0,
                    disp_size: DispSize::Auto,
                },
                width: MemWidth::B1,
                sign: SignExtend::Sign,
            },
            OpKind::Store {
                src: arm_x(14),
                addr: Address::Direct(arm_x(4)),
                width: MemWidth::B2,
            },
            OpKind::LoadPair {
                dst1: arm_x(5),
                dst2: arm_x(6),
                addr: Address::BaseOffset {
                    base: arm_x(7),
                    offset: -8,
                    disp_size: DispSize::Auto,
                },
                width: MemWidth::B4,
            },
            OpKind::StorePair {
                src1: arm_x(8),
                src2: arm_x(9),
                addr: Address::Direct(arm_x(10)),
                width: MemWidth::B4,
            },
        ];
        assert!(!aarch32_gate_with_mem(valid.clone(), false));
        assert!(aarch32_gate_with_mem(valid, true));

        for invalid in [
            OpKind::Load {
                dst: arm_x(15),
                addr: Address::Direct(arm_x(1)),
                width: MemWidth::B4,
                sign: SignExtend::Zero,
            },
            OpKind::Load {
                dst: arm_x(0),
                addr: Address::Direct(arm_x(15)),
                width: MemWidth::B1,
                sign: SignExtend::Zero,
            },
            OpKind::Load {
                dst: arm_x(0),
                addr: Address::Direct(arm_x(1)),
                width: MemWidth::B4,
                sign: SignExtend::Sign,
            },
            OpKind::Load {
                dst: arm_x(0),
                addr: Address::Absolute(u64::from(u32::MAX) + 1),
                width: MemWidth::B4,
                sign: SignExtend::Zero,
            },
            OpKind::Store {
                src: arm_x(0),
                addr: Address::Absolute(0x1000),
                width: MemWidth::B4,
            },
            OpKind::Store {
                src: arm_x(0),
                addr: Address::Direct(arm_x(1)),
                width: MemWidth::B8,
            },
            OpKind::LoadPair {
                dst1: arm_x(0),
                dst2: arm_x(0),
                addr: Address::Direct(arm_x(1)),
                width: MemWidth::B4,
            },
            OpKind::LoadPair {
                dst1: arm_x(0),
                dst2: arm_x(15),
                addr: Address::Direct(arm_x(1)),
                width: MemWidth::B4,
            },
            OpKind::StorePair {
                src1: arm_x(0),
                src2: arm_x(1),
                addr: Address::Direct(arm_x(2)),
                width: MemWidth::B8,
            },
        ] {
            assert!(
                !aarch32_gate_with_mem(vec![invalid.clone()], true),
                "{invalid:?}"
            );
        }
    }

    fn x86_aarch64_gate(ops: Vec<OpKind>) -> bool {
        let mut b = FunctionBuilder::new(FunctionId(0), 0x1000);
        for (i, op) in ops.into_iter().enumerate() {
            b.push_op(0x1000 + i as u64, op);
        }
        b.set_terminator(Terminator::Return { values: vec![] });
        is_x86_aarch64_native_clobber_safe_excluding(&b.finish(), &std::collections::HashMap::new())
    }

    #[test]
    fn x86_aarch64_nzcv_bridge_is_exhaustive_and_preserves_unrepresented_rflags() {
        const CF: u64 = 1 << 0;
        const PF: u64 = 1 << 2;
        const AF: u64 = 1 << 4;
        const ZF: u64 = 1 << 6;
        const SF: u64 = 1 << 7;
        const IF: u64 = 1 << 9;
        const OF: u64 = 1 << 11;
        const STATUS4: u64 = CF | ZF | SF | OF;
        let preserved = PF | AF | IF | (1 << 1) | (1 << 21);

        for bits in 0_u64..16 {
            let rflags = preserved
                | ((bits & 0b0001 != 0) as u64 * CF)
                | ((bits & 0b0010 != 0) as u64 * ZF)
                | ((bits & 0b0100 != 0) as u64 * SF)
                | ((bits & 0b1000 != 0) as u64 * OF);
            let nzcv = x86_rflags_to_aarch64_nzcv(rflags);
            assert_eq!(
                (nzcv >> 28) & 0xf,
                (bits & 0b0100) << 1
                    | (bits & 0b0010) << 1
                    | (bits & 0b0001) << 1
                    | (bits & 0b1000) >> 3
            );

            let prior = preserved | STATUS4;
            let merged =
                merge_aarch64_nzcv_into_x86_rflags(prior, nzcv | u64::MAX.wrapping_shl(32));
            assert_eq!(
                merged & STATUS4,
                rflags & STATUS4,
                "status pattern {bits:#06b}"
            );
            assert_eq!(
                merged & !STATUS4,
                prior & !STATUS4,
                "preserved pattern {bits:#06b}"
            );
        }
    }

    #[test]
    fn x86_aarch64_gate_accepts_representable_bls_adx_bit_tests_and_nf_alu() {
        let bls_flags = FlagSet::CF
            .union(FlagSet::ZF)
            .union(FlagSet::SF)
            .union(FlagSet::OF);
        assert!(x86_aarch64_gate(vec![
            OpKind::X86Bls {
                dst: x86(X86Reg::Rax),
                src: x86(X86Reg::Rcx),
                width: OpWidth::W64,
                kind: X86BlsKind::Blsi,
                flags: FlagUpdate::Specific(bls_flags),
            },
            OpKind::X86Adx {
                dst: x86(X86Reg::Rdx),
                src1: x86(X86Reg::Rdx),
                src2: x86(X86Reg::Rbx),
                width: OpWidth::W64,
                kind: X86AdxKind::Adcx,
                flags: FlagUpdate::Specific(FlagSet::CF),
            },
            OpKind::Add {
                dst: x86(X86Reg::Rsp),
                src1: x86(X86Reg::Rsp),
                src2: SrcOperand::Imm(16),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
            OpKind::Add {
                dst: x86(X86Reg::Rax),
                src1: x86(X86Reg::Rax),
                src2: SrcOperand::Reg(x86(X86Reg::Rcx)),
                width: OpWidth::W16,
                flags: FlagUpdate::None,
            },
            OpKind::Sub {
                dst: x86(X86Reg::Rdx),
                src1: x86(X86Reg::Rdx),
                src2: SrcOperand::Reg(x86(X86Reg::Rbx)),
                width: OpWidth::W8,
                flags: FlagUpdate::None,
            },
            OpKind::Adc {
                dst: x86(X86Reg::R8),
                src1: x86(X86Reg::R8),
                src2: SrcOperand::Reg(x86(X86Reg::R9)),
                width: OpWidth::W16,
                flags: FlagUpdate::None,
            },
            OpKind::Sbb {
                dst: x86(X86Reg::R9),
                src1: x86(X86Reg::R9),
                src2: SrcOperand::Reg(x86(X86Reg::R10)),
                width: OpWidth::W8,
                flags: FlagUpdate::None,
            },
            OpKind::Neg {
                dst: x86(X86Reg::R10),
                src: x86(X86Reg::R10),
                width: OpWidth::W8,
                flags: FlagUpdate::None,
            },
            OpKind::Inc {
                dst: x86(X86Reg::R11),
                src: x86(X86Reg::R11),
                width: OpWidth::W16,
                flags: FlagUpdate::None,
            },
            OpKind::Dec {
                dst: x86(X86Reg::R12),
                src: x86(X86Reg::R12),
                width: OpWidth::W8,
                flags: FlagUpdate::None,
            },
            OpKind::And {
                dst: x86(X86Reg::R13),
                src1: x86(X86Reg::R13),
                src2: SrcOperand::Reg(x86(X86Reg::R14)),
                width: OpWidth::W16,
                flags: FlagUpdate::None,
            },
            OpKind::Or {
                dst: x86(X86Reg::R14),
                src1: x86(X86Reg::R14),
                src2: SrcOperand::Reg(x86(X86Reg::R15)),
                width: OpWidth::W8,
                flags: FlagUpdate::None,
            },
            OpKind::Xor {
                dst: x86(X86Reg::R15),
                src1: x86(X86Reg::R15),
                src2: SrcOperand::Reg(x86(X86Reg::Rax)),
                width: OpWidth::W16,
                flags: FlagUpdate::None,
            },
            OpKind::Mov {
                dst: x86(X86Reg::Rax),
                src: SrcOperand::Reg(x86(X86Reg::Rcx)),
                width: OpWidth::W16,
            },
            OpKind::SetCC {
                dst: x86(X86Reg::Rdx),
                cond: crate::smir::ir::types::Condition::Eq,
                width: OpWidth::W8,
            },
            OpKind::CMove {
                dst: x86(X86Reg::Rsi),
                src: x86(X86Reg::Rdi),
                cond: crate::smir::ir::types::Condition::Eq,
                width: OpWidth::W16,
            },
            OpKind::Not {
                dst: x86(X86Reg::Rbx),
                src: x86(X86Reg::Rbx),
                width: OpWidth::W8,
            },
            OpKind::Xchg {
                reg1: x86(X86Reg::Rsi),
                reg2: x86(X86Reg::Rdi),
                width: OpWidth::W16,
            },
            OpKind::Bt {
                src: x86(X86Reg::R8),
                index: SrcOperand::Reg(x86(X86Reg::R9)),
                width: OpWidth::W16,
            },
            OpKind::Bts {
                dst: x86(X86Reg::R10),
                src: x86(X86Reg::R10),
                index: SrcOperand::Imm(15),
                width: OpWidth::W16,
            },
            OpKind::Btc {
                dst: x86(X86Reg::R11),
                src: x86(X86Reg::R11),
                index: SrcOperand::Imm64(63),
                width: OpWidth::W64,
            },
            OpKind::SetCF { value: true },
            OpKind::CmcCF,
        ]));
    }

    #[test]
    fn x86_aarch64_gate_accepts_no_flag_sbb_complete_width_matrix() {
        for width in [OpWidth::W8, OpWidth::W16, OpWidth::W32, OpWidth::W64] {
            for src2 in [SrcOperand::Reg(x86(X86Reg::Rcx)), SrcOperand::Imm64(-1)] {
                assert!(
                    x86_aarch64_gate(vec![OpKind::Sbb {
                        dst: x86(X86Reg::Rax),
                        src1: x86(X86Reg::Rax),
                        src2,
                        width,
                        flags: FlagUpdate::None,
                    }]),
                    "no-flag SBB {width:?} must be eligible"
                );
            }
        }
    }

    #[test]
    fn x86_aarch64_gate_accepts_subword_shift_rotate_matrix() {
        for width in [OpWidth::W8, OpWidth::W16] {
            for amount in [SrcOperand::Imm(3), SrcOperand::Reg(x86(X86Reg::Rcx))] {
                for op in [
                    OpKind::Shl {
                        dst: x86(X86Reg::Rax),
                        src: x86(X86Reg::Rax),
                        amount: amount.clone(),
                        width,
                        flags: FlagUpdate::None,
                    },
                    OpKind::Shr {
                        dst: x86(X86Reg::Rax),
                        src: x86(X86Reg::Rax),
                        amount: amount.clone(),
                        width,
                        flags: FlagUpdate::None,
                    },
                    OpKind::Sar {
                        dst: x86(X86Reg::Rax),
                        src: x86(X86Reg::Rax),
                        amount: amount.clone(),
                        width,
                        flags: FlagUpdate::None,
                    },
                    OpKind::Rol {
                        dst: x86(X86Reg::Rax),
                        src: x86(X86Reg::Rax),
                        amount: amount.clone(),
                        width,
                        flags: FlagUpdate::None,
                    },
                    OpKind::Ror {
                        dst: x86(X86Reg::Rax),
                        src: x86(X86Reg::Rax),
                        amount: amount.clone(),
                        width,
                        flags: FlagUpdate::None,
                    },
                ] {
                    assert!(
                        x86_aarch64_gate(vec![op]),
                        "subword shift/rotate {width:?} amount {amount:?} must be eligible"
                    );
                }
            }
        }

        let rotate_flags = FlagUpdate::Specific(FlagSet::CF.union(FlagSet::OF));
        for width in [OpWidth::W8, OpWidth::W16] {
            for op in [
                OpKind::Rol {
                    dst: x86(X86Reg::Rax),
                    src: x86(X86Reg::Rax),
                    amount: SrcOperand::Imm(1),
                    width,
                    flags: rotate_flags,
                },
                OpKind::Ror {
                    dst: x86(X86Reg::Rax),
                    src: x86(X86Reg::Rax),
                    amount: SrcOperand::Reg(x86(X86Reg::Rcx)),
                    width,
                    flags: rotate_flags,
                },
            ] {
                assert!(
                    x86_aarch64_gate(vec![op]),
                    "flag-setting subword rotate {width:?} must be eligible"
                );
            }
        }
    }

    #[test]
    fn x86_aarch64_gate_accepts_subword_carry_rotate_partial_writes() {
        let flags = FlagUpdate::Specific(FlagSet::CF.union(FlagSet::OF));
        for width in [OpWidth::W8, OpWidth::W16] {
            for right in [false, true] {
                let op = if right {
                    OpKind::Rcr {
                        dst: x86(X86Reg::Rax),
                        src: x86(X86Reg::Rax),
                        amount: SrcOperand::Imm(1),
                        width,
                        flags,
                    }
                } else {
                    OpKind::Rcl {
                        dst: x86(X86Reg::Rax),
                        src: x86(X86Reg::Rax),
                        amount: SrcOperand::Imm(1),
                        width,
                        flags,
                    }
                };
                assert!(
                    x86_aarch64_gate(vec![op]),
                    "{} {width:?} must be eligible",
                    if right { "RCR" } else { "RCL" }
                );
            }
        }
    }

    #[test]
    fn x86_aarch64_gate_accepts_apx_ndd_double_shift_width_direction_and_count_matrix() {
        for width in [OpWidth::W16, OpWidth::W32, OpWidth::W64] {
            for left in [false, true] {
                for amount in [SrcOperand::Imm(4), SrcOperand::Reg(x86(X86Reg::Rcx))] {
                    assert!(
                        x86_aarch64_gate(vec![OpKind::X86NddDoubleShift {
                            dst: x86(X86Reg::Rbx),
                            base: x86(X86Reg::Rax),
                            fill: x86(X86Reg::Rbx),
                            amount: amount.clone(),
                            width,
                            left,
                            flags: FlagUpdate::None,
                        }]),
                        "APX NF NDD double shift {width:?} left={left} amount={amount:?}"
                    );
                }
            }
        }

        assert!(!x86_aarch64_scalar_shape_valid(
            &OpKind::X86NddDoubleShift {
                dst: x86(X86Reg::Rbx),
                base: x86(X86Reg::Rax),
                fill: x86(X86Reg::Rdx),
                amount: SrcOperand::Reg(x86(X86Reg::Rcx)),
                width: OpWidth::W16,
                left: true,
                flags: FlagUpdate::All,
            }
        ));
        for (amount, expected) in [(16, true), (17, false), (31, false), (32, true)] {
            assert_eq!(
                x86_aarch64_scalar_shape_valid(&OpKind::X86NddDoubleShift {
                    dst: x86(X86Reg::Rbx),
                    base: x86(X86Reg::Rax),
                    fill: x86(X86Reg::Rdx),
                    amount: SrcOperand::Imm(amount),
                    width: OpWidth::W16,
                    left: false,
                    flags: FlagUpdate::All,
                }),
                expected,
                "W16 flag-setting APX NDD immediate count {amount}"
            );
        }
    }

    #[test]
    fn x86_aarch64_gate_accepts_w16_destructive_double_shift_partial_writes() {
        for left in [false, true] {
            for amount in [SrcOperand::Imm(4), SrcOperand::Reg(x86(X86Reg::Rcx))] {
                let op = if left {
                    OpKind::Shld {
                        dst: x86(X86Reg::Rax),
                        src: x86(X86Reg::Rbx),
                        amount,
                        width: OpWidth::W16,
                        flags: FlagUpdate::None,
                    }
                } else {
                    OpKind::Shrd {
                        dst: x86(X86Reg::Rax),
                        src: x86(X86Reg::Rbx),
                        amount,
                        width: OpWidth::W16,
                        flags: FlagUpdate::None,
                    }
                };
                assert!(
                    x86_aarch64_gate(vec![op]),
                    "APX NF destructive W16 double shift left={left}"
                );
            }
        }

        for (amount, expected) in [(16, true), (17, false), (31, false), (32, true)] {
            assert_eq!(
                x86_aarch64_scalar_shape_valid(&OpKind::Shld {
                    dst: x86(X86Reg::Rax),
                    src: x86(X86Reg::Rbx),
                    amount: SrcOperand::Imm(amount),
                    width: OpWidth::W16,
                    flags: FlagUpdate::All,
                }),
                expected,
                "W16 flag-setting SHLD immediate count {amount}"
            );
        }
        assert!(!x86_aarch64_scalar_shape_valid(&OpKind::Shrd {
            dst: x86(X86Reg::Rax),
            src: x86(X86Reg::Rbx),
            amount: SrcOperand::Reg(x86(X86Reg::Rcx)),
            width: OpWidth::W16,
            flags: FlagUpdate::All,
        }));
    }

    #[test]
    fn x86_aarch64_gate_accepts_w16_scan_and_unary_count_partial_writes() {
        let rax = x86(X86Reg::Rax);
        let rbx = x86(X86Reg::Rbx);
        let zf_only = FlagUpdate::Specific(FlagSet::ZF);
        for op in [
            OpKind::Bsf {
                dst: rax,
                src: rbx,
                width: OpWidth::W16,
                flags: zf_only,
            },
            OpKind::Bsr {
                dst: rax,
                src: rax,
                width: OpWidth::W16,
                flags: zf_only,
            },
            OpKind::Clz {
                dst: rax,
                src: rbx,
                width: OpWidth::W16,
            },
            OpKind::Ctz {
                dst: rax,
                src: rax,
                width: OpWidth::W16,
            },
            OpKind::Popcnt {
                dst: rax,
                src: rbx,
                width: OpWidth::W16,
            },
        ] {
            assert!(x86_aarch64_gate(vec![op.clone()]), "supported {op:?}");
        }

        for op in [
            OpKind::Bsf {
                dst: rax,
                src: rbx,
                width: OpWidth::W16,
                flags: FlagUpdate::All,
            },
            OpKind::Bsr {
                dst: rax,
                src: rbx,
                width: OpWidth::W8,
                flags: zf_only,
            },
            OpKind::Popcnt {
                dst: rax,
                src: rbx,
                width: OpWidth::W8,
            },
        ] {
            assert!(!x86_aarch64_gate(vec![op.clone()]), "unsupported {op:?}");
        }
    }

    #[test]
    fn x86_aarch64_gate_accepts_crc32c_widths_and_rejects_malformed_shapes() {
        let rax = x86(X86Reg::Rax);
        let rbx = x86(X86Reg::Rbx);
        for width in [OpWidth::W8, OpWidth::W16, OpWidth::W32, OpWidth::W64] {
            assert!(x86_aarch64_gate(vec![OpKind::Crc32C {
                dst: rax,
                crc: rax,
                data: rbx,
                data_width: width,
            }]));
            assert!(x86_aarch64_gate(vec![OpKind::Crc32C {
                dst: rax,
                crc: rax,
                data: rax,
                data_width: width,
            }]));
        }

        for op in [
            OpKind::Crc32C {
                dst: rax,
                crc: rbx,
                data: rbx,
                data_width: OpWidth::W32,
            },
            OpKind::Crc32C {
                dst: rax,
                crc: rax,
                data: VReg::virt(0),
                data_width: OpWidth::W8,
            },
            OpKind::Crc32C {
                dst: rax,
                crc: rax,
                data: rbx,
                data_width: OpWidth::W128,
            },
        ] {
            assert!(!x86_aarch64_gate(vec![op.clone()]), "malformed {op:?}");
        }
    }

    #[test]
    fn x86_aarch64_gate_accepts_x86_count_full_and_w16_contracts() {
        let rax = x86(X86Reg::Rax);
        let rbx = x86(X86Reg::Rbx);
        for kind in [
            X86CountKind::Popcnt,
            X86CountKind::Tzcnt,
            X86CountKind::Lzcnt,
        ] {
            for width in [OpWidth::W16, OpWidth::W32, OpWidth::W64] {
                assert!(
                    x86_aarch64_gate(vec![OpKind::X86Count {
                        dst: rax,
                        src: rbx,
                        width,
                        kind,
                        flags: FlagUpdate::None,
                    }]),
                    "APX NF {kind:?} {width:?}"
                );
            }
        }

        let count_flags = FlagUpdate::Specific(FlagSet::CF.union(FlagSet::ZF));
        for kind in [X86CountKind::Tzcnt, X86CountKind::Lzcnt] {
            assert!(x86_aarch64_gate(vec![OpKind::X86Count {
                dst: rax,
                src: rbx,
                width: OpWidth::W16,
                kind,
                flags: count_flags,
            }]));
        }

        let popcnt_all = OpKind::X86Count {
            dst: rax,
            src: rbx,
            width: OpWidth::W16,
            kind: X86CountKind::Popcnt,
            flags: FlagUpdate::All,
        };
        assert!(x86_aarch64_scalar_shape_valid(&popcnt_all));
        assert!(
            !x86_aarch64_gate(vec![popcnt_all]),
            "terminal POPCNT has live PF/AF outputs unavailable in NZCV"
        );

        for op in [
            OpKind::X86Count {
                dst: rax,
                src: rbx,
                width: OpWidth::W8,
                kind: X86CountKind::Popcnt,
                flags: FlagUpdate::None,
            },
            OpKind::X86Count {
                dst: rax,
                src: rbx,
                width: OpWidth::W64,
                kind: X86CountKind::Tzcnt,
                flags: FlagUpdate::All,
            },
        ] {
            assert!(!x86_aarch64_gate(vec![op.clone()]), "unsupported {op:?}");
        }
    }

    #[test]
    fn x86_aarch64_gate_accepts_only_architectural_w16_extend_partial_writes() {
        let rax = x86(X86Reg::Rax);
        let rbx = x86(X86Reg::Rbx);
        for op in [
            OpKind::ZeroExtend {
                dst: rax,
                src: rbx,
                from_width: OpWidth::W8,
                to_width: OpWidth::W16,
            },
            OpKind::SignExtend {
                dst: rax,
                src: rax,
                from_width: OpWidth::W8,
                to_width: OpWidth::W16,
            },
        ] {
            assert!(
                x86_aarch64_gate(vec![op.clone()]),
                "architectural W16 extension must JIT: {op:?}"
            );
        }

        for op in [
            OpKind::ZeroExtend {
                dst: rax,
                src: rbx,
                from_width: OpWidth::W16,
                to_width: OpWidth::W16,
            },
            OpKind::SignExtend {
                dst: rax,
                src: rbx,
                from_width: OpWidth::W8,
                to_width: OpWidth::W8,
            },
            OpKind::ZeroExtend {
                dst: x86(X86Reg::R16),
                src: rbx,
                from_width: OpWidth::W8,
                to_width: OpWidth::W16,
            },
            OpKind::SignExtend {
                dst: rax,
                src: VReg::Virtual(VirtualId(9)),
                from_width: OpWidth::W8,
                to_width: OpWidth::W16,
            },
        ] {
            assert!(
                !x86_aarch64_gate(vec![op.clone()]),
                "non-architectural W16 extension must deopt: {op:?}"
            );
        }
    }

    #[test]
    fn x86_aarch64_gate_rejects_unrepresentable_flags_registers_and_shapes() {
        let full_flag_add = OpKind::Add {
            dst: x86(X86Reg::Rax),
            src1: x86(X86Reg::Rax),
            src2: SrcOperand::Imm(1),
            width: OpWidth::W64,
            flags: FlagUpdate::All,
        };
        assert!(!x86_aarch64_gate(vec![full_flag_add]));

        // Flag-setting SBB defines PF/AF, which cannot cross the NZCV bridge.
        assert!(!x86_aarch64_gate(vec![OpKind::Sbb {
            dst: x86(X86Reg::Rax),
            src1: x86(X86Reg::Rax),
            src2: SrcOperand::Reg(x86(X86Reg::Rcx)),
            width: OpWidth::W64,
            flags: FlagUpdate::All,
        }]));

        assert!(!x86_aarch64_gate(vec![OpKind::SetCC {
            dst: x86(X86Reg::Rax),
            cond: crate::smir::ir::types::Condition::Parity,
            width: OpWidth::W8,
        }]));
        assert!(!x86_aarch64_gate(vec![OpKind::SetCC {
            dst: x86(X86Reg::Rax),
            cond: crate::smir::ir::types::Condition::Ult,
            width: OpWidth::W8,
        }]));

        assert!(!x86_aarch64_gate(vec![OpKind::Mov {
            dst: x86(X86Reg::R18),
            src: SrcOperand::Reg(x86(X86Reg::Rax)),
            width: OpWidth::W64,
        }]));
        assert!(!x86_aarch64_gate(vec![OpKind::Mov {
            dst: VReg::virt(0),
            src: SrcOperand::Reg(x86(X86Reg::Rax)),
            width: OpWidth::W64,
        }]));

        // Other unmerged subword destination families remain fail-closed.
        assert!(!x86_aarch64_gate(vec![OpKind::AndNot {
            dst: x86(X86Reg::Rax),
            src1: x86(X86Reg::Rax),
            src2: SrcOperand::Reg(x86(X86Reg::Rcx)),
            width: OpWidth::W16,
            flags: FlagUpdate::None,
        }]));
        assert!(!x86_aarch64_gate(vec![OpKind::SetCC {
            dst: x86(X86Reg::Rax),
            cond: crate::smir::ir::types::Condition::Eq,
            width: OpWidth::W16,
        }]));
        assert!(!x86_aarch64_gate(vec![OpKind::Bts {
            dst: x86(X86Reg::Rax),
            src: x86(X86Reg::Rax),
            index: SrcOperand::Imm(7),
            width: OpWidth::W8,
        }]));
        assert!(!x86_aarch64_gate(vec![OpKind::Btr {
            dst: x86(X86Reg::Rax),
            src: x86(X86Reg::Rcx),
            index: SrcOperand::Imm(0),
            width: OpWidth::W64,
        }]));
        assert!(!x86_aarch64_gate(vec![OpKind::Bt {
            src: x86(X86Reg::Rax),
            index: SrcOperand::Reg(VReg::virt(2)),
            width: OpWidth::W64,
        }]));
    }

    #[test]
    fn x86_aarch64_gate_validates_terminator_register_operands() {
        let mut cond = FunctionBuilder::new(FunctionId(0), 0x1000);
        let cond_true = cond.create_block(0x1010);
        let cond_false = cond.create_block(0x1020);
        cond.set_terminator(Terminator::CondBranch {
            cond: x86(X86Reg::R18),
            true_target: cond_true,
            false_target: cond_false,
        });
        cond.switch_to_block(cond_true);
        cond.set_terminator(Terminator::Return { values: vec![] });
        cond.switch_to_block(cond_false);
        cond.set_terminator(Terminator::Return { values: vec![] });
        assert!(!is_x86_aarch64_native_clobber_safe_excluding(
            &cond.finish(),
            &std::collections::HashMap::new()
        ));

        let mut switch = FunctionBuilder::new(FunctionId(1), 0x2000);
        let case = switch.create_block(0x2010);
        let default = switch.create_block(0x2020);
        switch.set_terminator(Terminator::Switch {
            index: VReg::virt(7),
            targets: vec![case],
            default,
        });
        switch.switch_to_block(case);
        switch.set_terminator(Terminator::Return { values: vec![] });
        switch.switch_to_block(default);
        switch.set_terminator(Terminator::Return { values: vec![] });
        assert!(!is_x86_aarch64_native_clobber_safe_excluding(
            &switch.finish(),
            &std::collections::HashMap::new()
        ));

        let mut legacy = FunctionBuilder::new(FunctionId(2), 0x3000);
        let legacy_true = legacy.create_block(0x3010);
        let legacy_false = legacy.create_block(0x3020);
        legacy.set_terminator(Terminator::CondBranch {
            cond: x86(X86Reg::Rcx),
            true_target: legacy_true,
            false_target: legacy_false,
        });
        legacy.switch_to_block(legacy_true);
        legacy.set_terminator(Terminator::Return { values: vec![] });
        legacy.switch_to_block(legacy_false);
        legacy.set_terminator(Terminator::Return { values: vec![] });
        assert!(is_x86_aarch64_native_clobber_safe_excluding(
            &legacy.finish(),
            &std::collections::HashMap::new()
        ));
    }

    #[test]
    fn x86_aes_feature_requirements_distinguish_vex_evex_vl_and_aes_ni() {
        let aes = |dst, src1, src2, width, op| OpKind::X86Aes {
            dst,
            src1,
            src2,
            width,
            op,
            imm: 0,
        };
        assert_eq!(
            x86_aes_feature_requirements(&aes(
                x86(X86Reg::Xmm(1)),
                x86(X86Reg::Xmm(2)),
                Some(x86(X86Reg::Xmm(3))),
                VecWidth::V128,
                X86AesOp::Enc,
            )),
            (false, true, false)
        );
        assert_eq!(
            x86_aes_feature_requirements(&aes(
                x86(X86Reg::Xmm(16)),
                x86(X86Reg::Xmm(17)),
                Some(x86(X86Reg::Xmm(18))),
                VecWidth::V128,
                X86AesOp::EncLast,
            )),
            (false, true, true)
        );
        assert_eq!(
            x86_aes_feature_requirements(&aes(
                x86(X86Reg::Zmm(16)),
                x86(X86Reg::Zmm(17)),
                Some(x86(X86Reg::Zmm(18))),
                VecWidth::V512,
                X86AesOp::Dec,
            )),
            (false, true, false)
        );
        assert_eq!(
            x86_aes_feature_requirements(&aes(
                x86(X86Reg::Xmm(9)),
                x86(X86Reg::Xmm(8)),
                None,
                VecWidth::V128,
                X86AesOp::InvMixColumns,
            )),
            (true, false, false)
        );
        assert_eq!(
            x86_aes_feature_requirements(&OpKind::Nop),
            (false, false, false)
        );
    }

    #[test]
    fn x86_sha512_feature_requirement_is_exact_to_the_three_native_ops() {
        assert!(x86_sha512_feature_required(&OpKind::X86Sha512Msg1 {
            dst: x86(X86Reg::Ymm(1)),
            src: x86(X86Reg::Xmm(2)),
        }));
        assert!(x86_sha512_feature_required(&OpKind::X86Sha512Msg2 {
            dst: x86(X86Reg::Ymm(1)),
            src: x86(X86Reg::Ymm(2)),
        }));
        assert!(x86_sha512_feature_required(&OpKind::X86Sha512Rounds2 {
            dst: x86(X86Reg::Ymm(1)),
            state: x86(X86Reg::Ymm(2)),
            wk: x86(X86Reg::Xmm(3)),
        }));
        assert!(!x86_sha512_feature_required(&OpKind::Nop));
    }

    #[test]
    fn x86_sm3_feature_requirement_is_exact_to_the_three_native_ops() {
        assert!(x86_sm3_feature_required(&OpKind::X86Sm3Msg1 {
            dst: x86(X86Reg::Xmm(1)),
            src1: x86(X86Reg::Xmm(2)),
            src2: x86(X86Reg::Xmm(3)),
        }));
        assert!(x86_sm3_feature_required(&OpKind::X86Sm3Msg2 {
            dst: x86(X86Reg::Xmm(1)),
            src1: x86(X86Reg::Xmm(2)),
            src2: x86(X86Reg::Xmm(3)),
        }));
        assert!(x86_sm3_feature_required(&OpKind::X86Sm3Rounds2 {
            dst: x86(X86Reg::Xmm(1)),
            state: x86(X86Reg::Xmm(2)),
            words: x86(X86Reg::Xmm(3)),
            imm: 0x3E,
        }));
        assert!(!x86_sm3_feature_required(&OpKind::Nop));
    }

    #[test]
    fn x86_sm4_feature_requirement_is_exact_to_the_native_op() {
        assert!(x86_sm4_feature_required(&OpKind::X86Sm4 {
            dst: x86(X86Reg::Ymm(1)),
            src1: x86(X86Reg::Ymm(2)),
            src2: x86(X86Reg::Ymm(3)),
            width: VecWidth::V256,
            key_schedule: false,
        }));
        assert!(!x86_sm4_feature_required(&OpKind::Nop));
    }

    #[test]
    fn x86_packed_shift_imm_requirements_select_vex_or_evex_exactly() {
        let op = |dst, src, width, elem, shift| OpKind::X86PackedShiftImm {
            dst,
            src,
            width,
            elem,
            shift,
            amount: 3,
            byte_lane: false,
        };
        assert_eq!(
            x86_packed_shift_imm_feature_requirements(&op(
                x86(X86Reg::Xmm(1)),
                x86(X86Reg::Xmm(2)),
                VecWidth::V128,
                VecElementType::I32,
                ShiftOp::Lsr
            )),
            (true, false, false)
        );
        assert_eq!(
            x86_packed_shift_imm_feature_requirements(&op(
                x86(X86Reg::Ymm(1)),
                x86(X86Reg::Ymm(2)),
                VecWidth::V256,
                VecElementType::I32,
                ShiftOp::Lsl
            )),
            (false, true, false)
        );
        assert_eq!(
            x86_packed_shift_imm_feature_requirements(&op(
                x86(X86Reg::Xmm(16)),
                x86(X86Reg::Xmm(17)),
                VecWidth::V128,
                VecElementType::I32,
                ShiftOp::Asr
            )),
            (false, false, true)
        );
        assert_eq!(
            x86_packed_shift_imm_feature_requirements(&op(
                x86(X86Reg::Zmm(1)),
                x86(X86Reg::Zmm(2)),
                VecWidth::V512,
                VecElementType::I64,
                ShiftOp::Asr
            )),
            (false, false, false)
        );
    }

    #[test]
    fn x86_packed_shared_count_requirements_select_vex_or_evex_exactly() {
        let op = |dst, src, count, width, elem, shift| OpKind::X86PackedShift {
            dst,
            src,
            count,
            width,
            elem,
            shift,
        };
        assert_eq!(
            x86_packed_shift_feature_requirements(&op(
                x86(X86Reg::Xmm(1)),
                x86(X86Reg::Xmm(2)),
                x86(X86Reg::Xmm(3)),
                VecWidth::V128,
                VecElementType::I32,
                ShiftOp::Lsr
            )),
            (true, false, false)
        );
        assert_eq!(
            x86_packed_shift_feature_requirements(&op(
                x86(X86Reg::Ymm(1)),
                x86(X86Reg::Ymm(2)),
                x86(X86Reg::Xmm(3)),
                VecWidth::V256,
                VecElementType::I16,
                ShiftOp::Lsl
            )),
            (false, true, false)
        );
        assert_eq!(
            x86_packed_shift_feature_requirements(&op(
                x86(X86Reg::Xmm(1)),
                x86(X86Reg::Xmm(2)),
                x86(X86Reg::Xmm(18)),
                VecWidth::V128,
                VecElementType::I32,
                ShiftOp::Asr
            )),
            (false, false, true)
        );
        assert_eq!(
            x86_packed_shift_feature_requirements(&op(
                x86(X86Reg::Xmm(1)),
                x86(X86Reg::Xmm(2)),
                x86(X86Reg::Xmm(3)),
                VecWidth::V128,
                VecElementType::I64,
                ShiftOp::Asr
            )),
            (false, false, true)
        );
        assert_eq!(
            x86_packed_shift_feature_requirements(&op(
                x86(X86Reg::Zmm(17)),
                x86(X86Reg::Zmm(18)),
                x86(X86Reg::Xmm(19)),
                VecWidth::V512,
                VecElementType::I64,
                ShiftOp::Lsl
            )),
            (false, false, false)
        );
    }

    #[test]
    fn x86_vector_guest_state_layout_matches_trampoline_offsets() {
        assert_eq!(
            std::mem::offset_of!(GuestRegs, zmm),
            X86_GUEST_ZMM_OFFSET as usize
        );
        assert_eq!(
            std::mem::offset_of!(GuestRegs, k),
            X86_GUEST_K_OFFSET as usize
        );
        assert_eq!(
            std::mem::offset_of!(GuestRegs, vector_active),
            X86_GUEST_VECTOR_ACTIVE_OFFSET as usize
        );
        assert_eq!(
            std::mem::offset_of!(GuestRegs, mxcsr),
            X86_GUEST_MXCSR_OFFSET as usize
        );
        assert_eq!(
            std::mem::offset_of!(GuestRegs, host_mxcsr),
            X86_HOST_MXCSR_OFFSET as usize
        );
        assert_eq!(
            std::mem::offset_of!(GuestRegs, tsc_aux),
            X86_GUEST_TSC_AUX_OFFSET as usize
        );
        assert_eq!(
            std::mem::offset_of!(GuestRegs, xcr0),
            X86_GUEST_XCR0_OFFSET as usize
        );
        assert_eq!(
            std::mem::offset_of!(GuestRegs, xgetbv1),
            X86_GUEST_XGETBV1_OFFSET as usize
        );
        assert_eq!(
            std::mem::offset_of!(GuestRegs, cr4),
            X86_GUEST_CR4_OFFSET as usize
        );
        assert_eq!(
            std::mem::offset_of!(GuestRegs, cr0),
            X86_GUEST_CR0_OFFSET as usize
        );
        assert_eq!(
            std::mem::offset_of!(GuestRegs, cpl),
            X86_GUEST_CPL_OFFSET as usize
        );
        assert_eq!(
            std::mem::offset_of!(GuestRegs, apx_enabled),
            X86_GUEST_APX_ENABLED_OFFSET as usize
        );
        assert_eq!(
            std::mem::offset_of!(GuestRegs, vec_load_fn),
            X86_GUEST_VEC_LOAD_FN_OFFSET as usize
        );
        assert_eq!(
            std::mem::offset_of!(GuestRegs, vec_store_fn),
            X86_GUEST_VEC_STORE_FN_OFFSET as usize
        );
        assert_eq!(
            std::mem::offset_of!(GuestRegs, pair_load_fn),
            X86_GUEST_PAIR_LOAD_FN_OFFSET as usize
        );
        assert_eq!(
            std::mem::offset_of!(GuestRegs, pair_store_fn),
            X86_GUEST_PAIR_STORE_FN_OFFSET as usize
        );
        assert_eq!(std::mem::align_of::<GuestRegs>(), 64);

        let mut regs = GuestRegs::default();
        let low = [0x0101_0101_0101_0101; 8];
        let high = [0x3131_3131_3131_3131; 8];
        regs.set_zmm(0, low);
        regs.set_zmm(31, high);
        assert_eq!(regs.get_zmm(0), low);
        assert_eq!(regs.get_zmm(31), high);
        assert_eq!(regs.mxcsr, 0x1F80);
    }

    #[test]
    fn clobber_gate_admits_only_architectural_native_vector_operands() {
        let zmm1 = x86(X86Reg::Zmm(1));
        let zmm2 = x86(X86Reg::Zmm(2));
        let zmm3 = x86(X86Reg::Zmm(3));
        let ymm1 = x86(X86Reg::Ymm(1));
        let ymm2 = x86(X86Reg::Ymm(2));
        let ymm3 = x86(X86Reg::Ymm(3));
        let xmm1 = x86(X86Reg::Xmm(1));
        let xmm2 = x86(X86Reg::Xmm(2));
        let xmm3 = x86(X86Reg::Xmm(3));
        let k4 = x86(X86Reg::K(4));
        let k5 = x86(X86Reg::K(5));
        let native_ops = [
            OpKind::VPopcnt {
                dst: zmm1,
                src: zmm2,
                mask: Some(k4),
                elem: VecElementType::I32,
                width: VecWidth::V512,
                zeroing: true,
            },
            OpKind::VShuffleBitQM {
                dst: k5,
                src: zmm3,
                indices: zmm2,
                mask: Some(k4),
                width: VecWidth::V512,
            },
            OpKind::VConflict {
                dst: zmm1,
                src: zmm2,
                mask: Some(k4),
                elem: VecElementType::I32,
                width: VecWidth::V512,
                zeroing: true,
            },
            OpKind::VLeadingZeros {
                dst: zmm1,
                src: zmm2,
                mask: Some(k4),
                elem: VecElementType::I32,
                width: VecWidth::V512,
                zeroing: true,
            },
            OpKind::X86PermuteBytesWords {
                dst: zmm1,
                table1: zmm2,
                table2: None,
                indices: zmm3,
                mask: Some(k4),
                elem: VecElementType::I8,
                width: VecWidth::V512,
                overwrite_table: false,
                zeroing: true,
            },
            OpKind::VCompress {
                dst: zmm1,
                src: zmm2,
                mask: Some(k4),
                elem: VecElementType::I32,
                width: VecWidth::V512,
                zeroing: true,
            },
            OpKind::VExpand {
                dst: zmm1,
                src: zmm2,
                mask: Some(k4),
                elem: VecElementType::F64,
                width: VecWidth::V512,
                zeroing: false,
            },
            OpKind::X86NarrowInt {
                dst: ymm1,
                src: zmm2,
                mask: Some(k4),
                src_elem: VecElementType::I16,
                dst_elem: VecElementType::I8,
                width: VecWidth::V512,
                mode: crate::smir::ir::types::X86NarrowMode::Truncate,
                zeroing: true,
            },
            OpKind::X86Aes {
                dst: zmm1,
                src1: zmm2,
                src2: Some(zmm3),
                width: VecWidth::V512,
                op: X86AesOp::Enc,
                imm: 0,
            },
            OpKind::X86Aes {
                dst: xmm1,
                src1: xmm2,
                src2: None,
                width: VecWidth::V128,
                op: X86AesOp::KeygenAssist,
                imm: 0x5A,
            },
            OpKind::X86Sha512Msg1 {
                dst: ymm1,
                src: xmm2,
            },
            OpKind::X86Sha512Msg2 {
                dst: ymm1,
                src: ymm2,
            },
            OpKind::X86Sha512Rounds2 {
                dst: ymm1,
                state: ymm2,
                wk: xmm3,
            },
            OpKind::X86Sm3Msg1 {
                dst: xmm1,
                src1: xmm2,
                src2: xmm3,
            },
            OpKind::X86Sm3Msg2 {
                dst: xmm1,
                src1: xmm2,
                src2: xmm3,
            },
            OpKind::X86Sm3Rounds2 {
                dst: xmm1,
                state: xmm2,
                words: xmm3,
                imm: 0x3E,
            },
            OpKind::X86Sm4 {
                dst: ymm1,
                src1: ymm2,
                src2: ymm3,
                width: VecWidth::V256,
                key_schedule: false,
            },
            OpKind::X86PackedShiftImm {
                dst: zmm1,
                src: zmm2,
                width: VecWidth::V512,
                elem: VecElementType::I64,
                shift: ShiftOp::Asr,
                amount: 9,
                byte_lane: false,
            },
            OpKind::X86PackedShift {
                dst: zmm1,
                src: zmm2,
                count: xmm3,
                width: VecWidth::V512,
                elem: VecElementType::I64,
                shift: ShiftOp::Lsl,
            },
            OpKind::VCompress {
                dst: zmm1,
                src: zmm2,
                mask: Some(k4),
                elem: VecElementType::I8,
                width: VecWidth::V512,
                zeroing: true,
            },
            OpKind::VExpand {
                dst: zmm1,
                src: zmm2,
                mask: Some(k4),
                elem: VecElementType::I16,
                width: VecWidth::V512,
                zeroing: false,
            },
            OpKind::VDotProduct {
                dst: zmm1,
                acc: zmm1,
                src1: zmm2,
                src2: zmm3,
                mask: Some(k4),
                src_elem: VecElementType::I8,
                acc_elem: VecElementType::I32,
                width: VecWidth::V512,
                src1_unsigned: true,
                saturate: false,
                zeroing: true,
            },
            OpKind::VMultiplyAdd52 {
                dst: zmm1,
                acc: zmm1,
                src1: zmm2,
                src2: zmm3,
                mask: Some(k4),
                width: VecWidth::V512,
                high: false,
                zeroing: true,
            },
            OpKind::VDotProductBF16 {
                dst: zmm1,
                acc: zmm1,
                src1: zmm2,
                src2: zmm3,
                mask: Some(k4),
                width: VecWidth::V512,
                zeroing: true,
            },
            OpKind::VCvtFP32ToBF16 {
                dst: ymm1,
                src1: zmm2,
                src2: None,
                mask: Some(k4),
                width: VecWidth::V512,
                zeroing: true,
            },
            OpKind::VFP16Arith {
                dst: zmm1,
                src1: zmm2,
                src2: zmm3,
                mask: Some(k4),
                op: crate::smir::ir::types::Avx10FP16Op::Add,
                width: VecWidth::V512,
                zeroing: true,
            },
            OpKind::X86PackedShiftVariable {
                dst: zmm1,
                src: zmm2,
                count: zmm3,
                mask: Some(k4),
                width: VecWidth::V512,
                elem: VecElementType::I32,
                shift: ShiftOp::Lsl,
                zeroing: true,
            },
            OpKind::X86PackedRotate {
                dst: zmm1,
                src: zmm2,
                count: None,
                mask: Some(k4),
                amount: 7,
                width: VecWidth::V512,
                elem: VecElementType::I32,
                left: true,
                zeroing: true,
            },
            OpKind::X86TernaryLogic {
                dst: zmm1,
                src1: zmm1,
                src2: zmm2,
                src3: zmm3,
                mask: Some(k4),
                imm: 0x96,
                width: VecWidth::V512,
                elem: VecElementType::I32,
                zeroing: true,
            },
            OpKind::X86PackedFunnelShift {
                dst: zmm1,
                src: zmm1,
                fill: zmm2,
                count: Some(zmm3),
                mask: Some(k4),
                amount: 0,
                width: VecWidth::V512,
                elem: VecElementType::I32,
                left: true,
                zeroing: true,
            },
            OpKind::X86MultiShiftQB {
                dst: zmm1,
                control: zmm2,
                source: zmm3,
                mask: Some(k4),
                width: VecWidth::V512,
                zeroing: true,
            },
        ];
        for native in &native_ops {
            assert!(is_x86_native_vector_op(native), "{native:?}");
            assert!(x86_gate(native.clone()), "{native:?}");
        }

        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(0x1000, native_ops[0].clone());
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();
        assert!(uses_x86_native_vectors_excluding(
            &func,
            &std::collections::HashMap::new()
        ));

        for (move_kind, hint) in [
            (
                OpKind::VMov {
                    dst: xmm1,
                    src: xmm2,
                    width: VecWidth::V128,
                },
                X86OpHint::SseMov {
                    prefix: X86SsePrefix::None,
                    opcode: 0x28,
                },
            ),
            (
                OpKind::VMov {
                    dst: ymm1,
                    src: ymm2,
                    width: VecWidth::V256,
                },
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::None,
                    opcode: 0x28,
                    width: VecWidth::V256,
                    w: false,
                },
            ),
            (
                OpKind::VMov {
                    dst: zmm1,
                    src: zmm2,
                    width: VecWidth::V512,
                },
                X86OpHint::EvexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::None,
                    opcode: 0x28,
                    width: VecWidth::V512,
                    w: false,
                },
            ),
        ] {
            assert!(is_x86_native_vector_op(&move_kind));
            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            builder.push_op(0x1000, move_kind);
            builder.set_terminator(Terminator::Return { values: vec![] });
            let mut function = builder.finish();
            function.blocks[0].ops[0].x86_hint = Some(hint);
            assert!(x86_native_vector_smir_op(&function.blocks[0].ops[0]));
            assert!(is_native_clobber_safe(&function));
        }

        let unhinted_move = OpKind::VMov {
            dst: xmm1,
            src: xmm2,
            width: VecWidth::V128,
        };
        assert!(is_x86_native_vector_op(&unhinted_move));
        assert!(!x86_gate(unhinted_move));

        let virtual_source = OpKind::X86PackedShiftVariable {
            dst: zmm1,
            src: VReg::Virtual(VirtualId(7)),
            count: zmm2,
            mask: None,
            width: VecWidth::V512,
            elem: VecElementType::I32,
            shift: ShiftOp::Lsl,
            zeroing: false,
        };
        assert!(!is_x86_native_vector_op(&virtual_source));
        assert!(!x86_gate(virtual_source));

        for malformed_move in [
            OpKind::VMov {
                dst: xmm1,
                src: ymm2,
                width: VecWidth::V128,
            },
            OpKind::VMov {
                dst: ymm1,
                src: ymm2,
                width: VecWidth::V128,
            },
            OpKind::VMov {
                dst: zmm1,
                src: VReg::Virtual(VirtualId(9)),
                width: VecWidth::V512,
            },
            OpKind::VMov {
                dst: x86(X86Reg::Xmm(32)),
                src: xmm2,
                width: VecWidth::V128,
            },
        ] {
            assert!(!is_x86_native_vector_op(&malformed_move));
            assert!(!x86_gate(malformed_move));
        }

        let move_kind = OpKind::VMov {
            dst: xmm1,
            src: xmm2,
            width: VecWidth::V128,
        };
        let mut malformed_hint = FunctionBuilder::new(FunctionId(0), 0x1000);
        malformed_hint.push_op(0x1000, move_kind);
        malformed_hint.set_terminator(Terminator::Return { values: vec![] });
        let mut malformed_hint = malformed_hint.finish();
        malformed_hint.blocks[0].ops[0].x86_hint = Some(X86OpHint::VexOp {
            map: X86VecMap::Map0F38,
            pp: X86SsePrefix::OpSize,
            opcode: 0x40,
            width: VecWidth::V128,
            w: false,
        });
        assert!(!x86_native_vector_smir_op(&malformed_hint.blocks[0].ops[0]));
        assert!(!is_native_clobber_safe(&malformed_hint));

        let low_move = crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            OpKind::VMov {
                dst: xmm1,
                src: xmm2,
                width: VecWidth::V128,
            },
            X86OpHint::SseMov {
                prefix: X86SsePrefix::None,
                opcode: 0x28,
            },
        );
        assert!(!x86_vector_move_needs_vl(&low_move));
        let evex_move = crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            low_move.kind.clone(),
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::None,
                opcode: 0x28,
                width: VecWidth::V128,
                w: false,
            },
        );
        assert!(x86_native_vector_smir_op(&evex_move));
        assert!(x86_vector_move_needs_vl(&evex_move));
        let high_move = crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            OpKind::VMov {
                dst: x86(X86Reg::Ymm(16)),
                src: x86(X86Reg::Ymm(31)),
                width: VecWidth::V256,
            },
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::None,
                opcode: 0x28,
                width: VecWidth::V256,
                w: false,
            },
        );
        assert!(x86_native_vector_smir_op(&high_move));
        assert!(x86_vector_move_needs_vl(&high_move));

        for (logic_kind, hint, feature_requirements) in [
            (
                OpKind::VAnd {
                    dst: xmm1,
                    src1: xmm1,
                    src2: xmm2,
                    width: VecWidth::V128,
                },
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::None,
                    opcode: 0x54,
                },
                (false, false, false, false),
            ),
            (
                OpKind::VAndNot {
                    dst: xmm1,
                    src1: xmm2,
                    src2: xmm3,
                    width: VecWidth::V128,
                },
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::None,
                    opcode: 0x55,
                    width: VecWidth::V128,
                    w: false,
                },
                (true, false, false, false),
            ),
            (
                OpKind::VOr {
                    dst: ymm1,
                    src1: ymm2,
                    src2: ymm3,
                    width: VecWidth::V256,
                },
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0xEB,
                    width: VecWidth::V256,
                    w: false,
                },
                (true, true, false, false),
            ),
            (
                OpKind::VXor {
                    dst: zmm1,
                    src1: zmm2,
                    src2: zmm3,
                    width: VecWidth::V512,
                },
                X86OpHint::EvexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x57,
                    width: VecWidth::V512,
                    w: true,
                },
                (false, false, true, false),
            ),
            (
                OpKind::VAnd {
                    dst: x86(X86Reg::Ymm(16)),
                    src1: x86(X86Reg::Ymm(17)),
                    src2: x86(X86Reg::Ymm(18)),
                    width: VecWidth::V256,
                },
                X86OpHint::EvexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0xDB,
                    width: VecWidth::V256,
                    w: false,
                },
                (false, false, false, true),
            ),
        ] {
            let smir_op = crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                logic_kind.clone(),
                hint,
            );
            assert!(is_x86_native_vector_op(&logic_kind), "{logic_kind:?}");
            assert!(x86_native_vector_smir_op(&smir_op), "{smir_op:?}");
            assert_eq!(
                x86_vector_logic_feature_requirements(&smir_op),
                feature_requirements,
                "{smir_op:?}"
            );

            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            builder.push_op(0x1000, logic_kind);
            builder.set_terminator(Terminator::Return { values: vec![] });
            let mut function = builder.finish();
            function.blocks[0].ops[0].x86_hint = Some(hint);
            assert!(is_native_clobber_safe(&function), "{smir_op:?}");
        }

        let unhinted_logic = crate::smir::ir::ops::SmirOp::new(
            crate::smir::ir::types::OpId(0),
            0x1000,
            OpKind::VXor {
                dst: xmm1,
                src1: xmm1,
                src2: xmm2,
                width: VecWidth::V128,
            },
        );
        assert!(is_x86_native_vector_op(&unhinted_logic.kind));
        assert!(!x86_native_vector_smir_op(&unhinted_logic));

        for malformed_logic_kind in [
            OpKind::VOr {
                dst: xmm1,
                src1: xmm2,
                src2: VReg::Virtual(VirtualId(10)),
                width: VecWidth::V128,
            },
            OpKind::VXor {
                dst: xmm1,
                src1: xmm2,
                src2: ymm3,
                width: VecWidth::V128,
            },
            OpKind::VAndNot {
                dst: x86(X86Reg::Xmm(32)),
                src1: xmm2,
                src2: xmm3,
                width: VecWidth::V128,
            },
        ] {
            assert!(!is_x86_native_vector_op(&malformed_logic_kind));
        }

        for malformed_logic in [
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                OpKind::VAnd {
                    dst: xmm1,
                    src1: xmm2,
                    src2: xmm3,
                    width: VecWidth::V128,
                },
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::None,
                    opcode: 0x54,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                OpKind::VAnd {
                    dst: xmm1,
                    src1: xmm2,
                    src2: xmm3,
                    width: VecWidth::V128,
                },
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::None,
                    opcode: 0x57,
                    width: VecWidth::V128,
                    w: false,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                OpKind::VXor {
                    dst: zmm1,
                    src1: zmm2,
                    src2: zmm3,
                    width: VecWidth::V512,
                },
                X86OpHint::EvexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x57,
                    width: VecWidth::V512,
                    w: false,
                },
            ),
        ] {
            assert!(!x86_native_vector_smir_op(&malformed_logic));
        }

        for (arithmetic_kind, hint, feature_requirements) in [
            (
                OpKind::VAdd {
                    dst: xmm1,
                    src1: xmm1,
                    src2: xmm2,
                    elem: VecElementType::I8,
                    lanes: 16,
                },
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode: 0xFC,
                },
                (false, false, false),
            ),
            (
                OpKind::VAdd {
                    dst: xmm1,
                    src1: xmm2,
                    src2: xmm3,
                    elem: VecElementType::I32,
                    lanes: 4,
                },
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0xFE,
                    width: VecWidth::V128,
                    w: false,
                },
                (true, false, false),
            ),
            (
                OpKind::VSub {
                    dst: ymm1,
                    src1: ymm2,
                    src2: ymm3,
                    elem: VecElementType::I16,
                    lanes: 16,
                },
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0xF9,
                    width: VecWidth::V256,
                    w: false,
                },
                (true, true, false),
            ),
            (
                OpKind::VAdd {
                    dst: zmm1,
                    src1: zmm2,
                    src2: zmm3,
                    elem: VecElementType::I64,
                    lanes: 8,
                },
                X86OpHint::EvexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0xD4,
                    width: VecWidth::V512,
                    w: true,
                },
                (false, false, false),
            ),
            (
                OpKind::VSub {
                    dst: x86(X86Reg::Ymm(16)),
                    src1: x86(X86Reg::Ymm(17)),
                    src2: x86(X86Reg::Ymm(18)),
                    elem: VecElementType::I32,
                    lanes: 8,
                },
                X86OpHint::EvexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0xFA,
                    width: VecWidth::V256,
                    w: false,
                },
                (false, false, true),
            ),
            (
                OpKind::VAddSubSat {
                    dst: xmm1,
                    src1: xmm1,
                    src2: xmm2,
                    elem: VecElementType::I8,
                    lanes: 16,
                    subtract: false,
                    signed: true,
                },
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode: 0xEC,
                },
                (false, false, false),
            ),
            (
                OpKind::VAddSubSat {
                    dst: ymm1,
                    src1: ymm2,
                    src2: ymm3,
                    elem: VecElementType::I16,
                    lanes: 16,
                    subtract: true,
                    signed: false,
                },
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0xD9,
                    width: VecWidth::V256,
                    w: true,
                },
                (true, true, false),
            ),
            (
                OpKind::VAddSubSat {
                    dst: zmm1,
                    src1: zmm2,
                    src2: zmm3,
                    elem: VecElementType::I8,
                    lanes: 64,
                    subtract: false,
                    signed: false,
                },
                X86OpHint::EvexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0xDC,
                    width: VecWidth::V512,
                    w: true,
                },
                (false, false, false),
            ),
            (
                OpKind::VAddSubSat {
                    dst: x86(X86Reg::Ymm(16)),
                    src1: x86(X86Reg::Ymm(17)),
                    src2: x86(X86Reg::Ymm(18)),
                    elem: VecElementType::I16,
                    lanes: 16,
                    subtract: true,
                    signed: true,
                },
                X86OpHint::EvexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0xE9,
                    width: VecWidth::V256,
                    w: true,
                },
                (false, false, true),
            ),
        ] {
            let smir_op = crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                arithmetic_kind.clone(),
                hint,
            );
            assert!(
                is_x86_native_vector_op(&arithmetic_kind),
                "{arithmetic_kind:?}"
            );
            assert!(x86_native_vector_smir_op(&smir_op), "{smir_op:?}");
            assert_eq!(
                x86_vector_integer_arithmetic_feature_requirements(&smir_op),
                feature_requirements,
                "{smir_op:?}"
            );

            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            builder.push_op(0x1000, arithmetic_kind);
            builder.set_terminator(Terminator::Return { values: vec![] });
            let mut function = builder.finish();
            function.blocks[0].ops[0].x86_hint = Some(hint);
            assert!(is_native_clobber_safe(&function), "{smir_op:?}");
        }

        let unhinted_arithmetic = crate::smir::ir::ops::SmirOp::new(
            crate::smir::ir::types::OpId(0),
            0x1000,
            OpKind::VSub {
                dst: xmm1,
                src1: xmm1,
                src2: xmm2,
                elem: VecElementType::I64,
                lanes: 2,
            },
        );
        assert!(is_x86_native_vector_op(&unhinted_arithmetic.kind));
        assert!(!x86_native_vector_smir_op(&unhinted_arithmetic));

        let unhinted_saturating = crate::smir::ir::ops::SmirOp::new(
            crate::smir::ir::types::OpId(0),
            0x1000,
            OpKind::VAddSubSat {
                dst: xmm1,
                src1: xmm1,
                src2: xmm2,
                elem: VecElementType::I8,
                lanes: 16,
                subtract: false,
                signed: true,
            },
        );
        assert!(is_x86_native_vector_op(&unhinted_saturating.kind));
        assert!(!x86_native_vector_smir_op(&unhinted_saturating));

        for malformed_arithmetic_kind in [
            OpKind::VAdd {
                dst: xmm1,
                src1: xmm1,
                src2: xmm2,
                elem: VecElementType::F32,
                lanes: 4,
            },
            OpKind::VSub {
                dst: xmm1,
                src1: xmm1,
                src2: xmm2,
                elem: VecElementType::I16,
                lanes: 7,
            },
            OpKind::VAdd {
                dst: xmm1,
                src1: xmm2,
                src2: ymm3,
                elem: VecElementType::I32,
                lanes: 4,
            },
            OpKind::VAddSubSat {
                dst: xmm1,
                src1: xmm1,
                src2: xmm2,
                elem: VecElementType::I32,
                lanes: 4,
                subtract: false,
                signed: true,
            },
            OpKind::VAddSubSat {
                dst: ymm1,
                src1: ymm2,
                src2: ymm3,
                elem: VecElementType::I8,
                lanes: 31,
                subtract: true,
                signed: false,
            },
        ] {
            assert!(!is_x86_native_vector_op(&malformed_arithmetic_kind));
        }

        for malformed_arithmetic in [
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                OpKind::VAdd {
                    dst: xmm1,
                    src1: xmm2,
                    src2: xmm3,
                    elem: VecElementType::I8,
                    lanes: 16,
                },
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode: 0xFC,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                OpKind::VSub {
                    dst: ymm1,
                    src1: ymm2,
                    src2: ymm3,
                    elem: VecElementType::I16,
                    lanes: 16,
                },
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0xF8,
                    width: VecWidth::V256,
                    w: false,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                OpKind::VAdd {
                    dst: zmm1,
                    src1: zmm2,
                    src2: zmm3,
                    elem: VecElementType::I64,
                    lanes: 8,
                },
                X86OpHint::EvexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0xD4,
                    width: VecWidth::V512,
                    w: false,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                OpKind::VAddSubSat {
                    dst: xmm1,
                    src1: xmm1,
                    src2: xmm2,
                    elem: VecElementType::I8,
                    lanes: 16,
                    subtract: false,
                    signed: false,
                },
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode: 0xEC,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                OpKind::VAddSubSat {
                    dst: zmm1,
                    src1: zmm2,
                    src2: zmm3,
                    elem: VecElementType::I16,
                    lanes: 32,
                    subtract: true,
                    signed: true,
                },
                X86OpHint::EvexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::Rep,
                    opcode: 0xE9,
                    width: VecWidth::V512,
                    w: false,
                },
            ),
        ] {
            assert!(!x86_native_vector_smir_op(&malformed_arithmetic));
        }

        for (multiply_kind, hint, feature_requirements) in [
            (
                OpKind::VMul {
                    dst: xmm1,
                    src1: xmm1,
                    src2: xmm2,
                    elem: VecElementType::I16,
                    lanes: 8,
                },
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode: 0xD5,
                },
                (false, false, false, false, false),
            ),
            (
                OpKind::VMul {
                    dst: xmm1,
                    src1: xmm1,
                    src2: xmm2,
                    elem: VecElementType::I32,
                    lanes: 4,
                },
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode: 0x40,
                },
                (true, false, false, false, false),
            ),
            (
                OpKind::VMul {
                    dst: xmm1,
                    src1: xmm2,
                    src2: xmm3,
                    elem: VecElementType::I16,
                    lanes: 8,
                },
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0xD5,
                    width: VecWidth::V128,
                    w: true,
                },
                (false, true, false, false, false),
            ),
            (
                OpKind::VMul {
                    dst: ymm1,
                    src1: ymm2,
                    src2: ymm3,
                    elem: VecElementType::I32,
                    lanes: 8,
                },
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x40,
                    width: VecWidth::V256,
                    w: true,
                },
                (false, true, true, false, false),
            ),
            (
                OpKind::VMul {
                    dst: zmm1,
                    src1: zmm2,
                    src2: zmm3,
                    elem: VecElementType::I32,
                    lanes: 16,
                },
                X86OpHint::EvexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x40,
                    width: VecWidth::V512,
                    w: false,
                },
                (false, false, false, false, false),
            ),
            (
                OpKind::VMul {
                    dst: zmm1,
                    src1: zmm2,
                    src2: zmm3,
                    elem: VecElementType::I64,
                    lanes: 8,
                },
                X86OpHint::EvexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x40,
                    width: VecWidth::V512,
                    w: true,
                },
                (false, false, false, true, false),
            ),
            (
                OpKind::VMul {
                    dst: x86(X86Reg::Ymm(16)),
                    src1: x86(X86Reg::Ymm(17)),
                    src2: x86(X86Reg::Ymm(18)),
                    elem: VecElementType::I64,
                    lanes: 4,
                },
                X86OpHint::EvexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x40,
                    width: VecWidth::V256,
                    w: true,
                },
                (false, false, false, true, true),
            ),
        ] {
            let smir_op = crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                multiply_kind.clone(),
                hint,
            );
            assert!(is_x86_native_vector_op(&multiply_kind), "{multiply_kind:?}");
            assert!(x86_native_vector_smir_op(&smir_op), "{smir_op:?}");
            assert_eq!(
                x86_vector_integer_multiply_feature_requirements(&smir_op),
                feature_requirements,
                "{smir_op:?}"
            );

            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            builder.push_op(0x1000, multiply_kind);
            builder.set_terminator(Terminator::Return { values: vec![] });
            let mut function = builder.finish();
            function.blocks[0].ops[0].x86_hint = Some(hint);
            assert!(is_native_clobber_safe(&function), "{smir_op:?}");
        }

        let unhinted_multiply = crate::smir::ir::ops::SmirOp::new(
            crate::smir::ir::types::OpId(0),
            0x1000,
            OpKind::VMul {
                dst: xmm1,
                src1: xmm1,
                src2: xmm2,
                elem: VecElementType::I16,
                lanes: 8,
            },
        );
        assert!(is_x86_native_vector_op(&unhinted_multiply.kind));
        assert!(!x86_native_vector_smir_op(&unhinted_multiply));

        for malformed_multiply_kind in [
            OpKind::VMul {
                dst: xmm1,
                src1: xmm1,
                src2: xmm2,
                elem: VecElementType::I8,
                lanes: 16,
            },
            OpKind::VMul {
                dst: xmm1,
                src1: xmm1,
                src2: xmm2,
                elem: VecElementType::F32,
                lanes: 4,
            },
            OpKind::VMul {
                dst: ymm1,
                src1: ymm2,
                src2: ymm3,
                elem: VecElementType::I32,
                lanes: 7,
            },
            OpKind::VMul {
                dst: xmm1,
                src1: xmm2,
                src2: ymm3,
                elem: VecElementType::I32,
                lanes: 4,
            },
            OpKind::VMul {
                dst: xmm1,
                src1: xmm1,
                src2: VReg::Virtual(VirtualId(11)),
                elem: VecElementType::I16,
                lanes: 8,
            },
        ] {
            assert!(!is_x86_native_vector_op(&malformed_multiply_kind));
        }

        for malformed_multiply in [
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                OpKind::VMul {
                    dst: xmm1,
                    src1: xmm2,
                    src2: xmm3,
                    elem: VecElementType::I16,
                    lanes: 8,
                },
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode: 0xD5,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                OpKind::VMul {
                    dst: xmm1,
                    src1: xmm2,
                    src2: xmm3,
                    elem: VecElementType::I16,
                    lanes: 8,
                },
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x40,
                    width: VecWidth::V128,
                    w: false,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                OpKind::VMul {
                    dst: ymm1,
                    src1: ymm2,
                    src2: ymm3,
                    elem: VecElementType::I32,
                    lanes: 8,
                },
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x40,
                    width: VecWidth::V256,
                    w: false,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                OpKind::VMul {
                    dst: ymm1,
                    src1: ymm2,
                    src2: ymm3,
                    elem: VecElementType::I64,
                    lanes: 4,
                },
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x40,
                    width: VecWidth::V256,
                    w: true,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                OpKind::VMul {
                    dst: zmm1,
                    src1: zmm2,
                    src2: zmm3,
                    elem: VecElementType::I32,
                    lanes: 16,
                },
                X86OpHint::EvexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x40,
                    width: VecWidth::V512,
                    w: true,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                OpKind::VMul {
                    dst: zmm1,
                    src1: zmm2,
                    src2: zmm3,
                    elem: VecElementType::I64,
                    lanes: 8,
                },
                X86OpHint::EvexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x40,
                    width: VecWidth::V512,
                    w: false,
                },
            ),
        ] {
            assert!(!x86_native_vector_smir_op(&malformed_multiply));
        }

        for (abs_kind, hint, feature_requirements) in [
            (
                OpKind::VUnary {
                    dst: xmm1,
                    src: xmm2,
                    elem: VecElementType::I8,
                    lanes: 16,
                    op: VecUnaryOp::Abs,
                },
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode: 0x1C,
                },
                (true, false, false, false),
            ),
            (
                OpKind::VUnary {
                    dst: ymm1,
                    src: ymm2,
                    elem: VecElementType::I16,
                    lanes: 16,
                    op: VecUnaryOp::Abs,
                },
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x1D,
                    width: VecWidth::V256,
                    w: true,
                },
                (false, true, true, false),
            ),
            (
                OpKind::VUnary {
                    dst: x86(X86Reg::Zmm(16)),
                    src: x86(X86Reg::Zmm(17)),
                    elem: VecElementType::I32,
                    lanes: 16,
                    op: VecUnaryOp::Abs,
                },
                X86OpHint::EvexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x1E,
                    width: VecWidth::V512,
                    w: false,
                },
                (false, false, false, false),
            ),
            (
                OpKind::VUnary {
                    dst: x86(X86Reg::Ymm(16)),
                    src: x86(X86Reg::Ymm(17)),
                    elem: VecElementType::I64,
                    lanes: 4,
                    op: VecUnaryOp::Abs,
                },
                X86OpHint::EvexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x1F,
                    width: VecWidth::V256,
                    w: true,
                },
                (false, false, false, true),
            ),
        ] {
            let smir_op = crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                abs_kind.clone(),
                hint,
            );
            assert!(is_x86_native_vector_op(&abs_kind), "{abs_kind:?}");
            assert!(x86_native_vector_smir_op(&smir_op), "{smir_op:?}");
            assert_eq!(
                x86_vector_integer_abs_feature_requirements(&smir_op),
                feature_requirements,
                "{smir_op:?}"
            );

            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            builder.push_op(0x1000, abs_kind);
            builder.set_terminator(Terminator::Return { values: vec![] });
            let mut function = builder.finish();
            function.blocks[0].ops[0].x86_hint = Some(hint);
            assert!(is_native_clobber_safe(&function), "{smir_op:?}");
        }

        let unhinted_abs = crate::smir::ir::ops::SmirOp::new(
            crate::smir::ir::types::OpId(0),
            0x1000,
            OpKind::VUnary {
                dst: xmm1,
                src: xmm2,
                elem: VecElementType::I32,
                lanes: 4,
                op: VecUnaryOp::Abs,
            },
        );
        assert!(is_x86_native_vector_op(&unhinted_abs.kind));
        assert!(!x86_native_vector_smir_op(&unhinted_abs));

        for malformed_abs_kind in [
            OpKind::VUnary {
                dst: xmm1,
                src: xmm2,
                elem: VecElementType::I32,
                lanes: 4,
                op: VecUnaryOp::Neg,
            },
            OpKind::VUnary {
                dst: xmm1,
                src: xmm2,
                elem: VecElementType::F32,
                lanes: 4,
                op: VecUnaryOp::Abs,
            },
            OpKind::VUnary {
                dst: ymm1,
                src: xmm2,
                elem: VecElementType::I16,
                lanes: 16,
                op: VecUnaryOp::Abs,
            },
            OpKind::VUnary {
                dst: xmm1,
                src: VReg::Virtual(VirtualId(12)),
                elem: VecElementType::I8,
                lanes: 16,
                op: VecUnaryOp::Abs,
            },
        ] {
            assert!(!is_x86_native_vector_op(&malformed_abs_kind));
        }

        for malformed_abs in [
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                OpKind::VUnary {
                    dst: xmm1,
                    src: xmm2,
                    elem: VecElementType::I64,
                    lanes: 2,
                    op: VecUnaryOp::Abs,
                },
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode: 0x1F,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                OpKind::VUnary {
                    dst: ymm1,
                    src: ymm2,
                    elem: VecElementType::I16,
                    lanes: 16,
                    op: VecUnaryOp::Abs,
                },
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x1D,
                    width: VecWidth::V256,
                    w: false,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                OpKind::VUnary {
                    dst: zmm1,
                    src: zmm2,
                    elem: VecElementType::I32,
                    lanes: 16,
                    op: VecUnaryOp::Abs,
                },
                X86OpHint::EvexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x1E,
                    width: VecWidth::V512,
                    w: true,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                OpKind::VUnary {
                    dst: zmm1,
                    src: zmm2,
                    elem: VecElementType::I64,
                    lanes: 8,
                    op: VecUnaryOp::Abs,
                },
                X86OpHint::EvexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x1F,
                    width: VecWidth::V512,
                    w: false,
                },
            ),
        ] {
            assert!(!x86_native_vector_smir_op(&malformed_abs));
        }

        for invalid_vplzcnt in [
            OpKind::VLeadingZeros {
                dst: zmm1,
                src: VReg::Virtual(VirtualId(8)),
                mask: None,
                elem: VecElementType::I32,
                width: VecWidth::V512,
                zeroing: false,
            },
            OpKind::VLeadingZeros {
                dst: zmm1,
                src: zmm2,
                mask: None,
                elem: VecElementType::I16,
                width: VecWidth::V512,
                zeroing: false,
            },
            OpKind::VLeadingZeros {
                dst: zmm1,
                src: zmm2,
                mask: Some(x86(X86Reg::K(0))),
                elem: VecElementType::I32,
                width: VecWidth::V512,
                zeroing: false,
            },
            OpKind::VLeadingZeros {
                dst: zmm1,
                src: zmm2,
                mask: None,
                elem: VecElementType::I32,
                width: VecWidth::V512,
                zeroing: true,
            },
            OpKind::VLeadingZeros {
                dst: zmm1,
                src: zmm2,
                mask: None,
                elem: VecElementType::I32,
                width: VecWidth::V256,
                zeroing: false,
            },
        ] {
            assert!(!is_x86_native_vector_op(&invalid_vplzcnt));
            assert!(!x86_gate(invalid_vplzcnt));
        }

        for invalid_permute in [
            OpKind::X86PermuteBytesWords {
                dst: zmm1,
                table1: VReg::Virtual(VirtualId(9)),
                table2: None,
                indices: zmm3,
                mask: None,
                elem: VecElementType::I8,
                width: VecWidth::V512,
                overwrite_table: false,
                zeroing: false,
            },
            OpKind::X86PermuteBytesWords {
                dst: zmm1,
                table1: zmm2,
                table2: None,
                indices: zmm3,
                mask: None,
                elem: VecElementType::I32,
                width: VecWidth::V512,
                overwrite_table: false,
                zeroing: false,
            },
            OpKind::X86PermuteBytesWords {
                dst: zmm1,
                table1: zmm2,
                table2: None,
                indices: zmm3,
                mask: Some(x86(X86Reg::K(0))),
                elem: VecElementType::I8,
                width: VecWidth::V512,
                overwrite_table: false,
                zeroing: false,
            },
            OpKind::X86PermuteBytesWords {
                dst: zmm1,
                table1: zmm2,
                table2: None,
                indices: zmm3,
                mask: None,
                elem: VecElementType::I8,
                width: VecWidth::V512,
                overwrite_table: false,
                zeroing: true,
            },
            OpKind::X86PermuteBytesWords {
                dst: zmm1,
                table1: zmm2,
                table2: None,
                indices: zmm3,
                mask: None,
                elem: VecElementType::I8,
                width: VecWidth::V256,
                overwrite_table: false,
                zeroing: false,
            },
            OpKind::X86PermuteBytesWords {
                dst: zmm1,
                table1: zmm2,
                table2: None,
                indices: zmm3,
                mask: None,
                elem: VecElementType::I8,
                width: VecWidth::V512,
                overwrite_table: true,
                zeroing: false,
            },
            OpKind::X86PermuteBytesWords {
                dst: zmm1,
                table1: zmm2,
                table2: Some(zmm3),
                indices: zmm2,
                mask: None,
                elem: VecElementType::I8,
                width: VecWidth::V512,
                overwrite_table: false,
                zeroing: false,
            },
        ] {
            assert!(!is_x86_native_vector_op(&invalid_permute));
            assert!(!x86_gate(invalid_permute));
        }

        for invalid_narrow in [
            OpKind::X86NarrowInt {
                dst: ymm1,
                src: VReg::Virtual(VirtualId(10)),
                mask: None,
                src_elem: VecElementType::I16,
                dst_elem: VecElementType::I8,
                width: VecWidth::V512,
                mode: crate::smir::ir::types::X86NarrowMode::Truncate,
                zeroing: false,
            },
            OpKind::X86NarrowInt {
                dst: zmm1,
                src: zmm2,
                mask: None,
                src_elem: VecElementType::I16,
                dst_elem: VecElementType::I8,
                width: VecWidth::V512,
                mode: crate::smir::ir::types::X86NarrowMode::Truncate,
                zeroing: false,
            },
            OpKind::X86NarrowInt {
                dst: ymm1,
                src: zmm2,
                mask: Some(x86(X86Reg::K(0))),
                src_elem: VecElementType::I16,
                dst_elem: VecElementType::I8,
                width: VecWidth::V512,
                mode: crate::smir::ir::types::X86NarrowMode::Truncate,
                zeroing: false,
            },
            OpKind::X86NarrowInt {
                dst: ymm1,
                src: zmm2,
                mask: None,
                src_elem: VecElementType::I16,
                dst_elem: VecElementType::I8,
                width: VecWidth::V512,
                mode: crate::smir::ir::types::X86NarrowMode::Truncate,
                zeroing: true,
            },
        ] {
            assert!(!is_x86_native_vector_op(&invalid_narrow));
            assert!(!x86_gate(invalid_narrow));
        }

        for invalid_aes in [
            OpKind::X86Aes {
                dst: zmm1,
                src1: zmm2,
                src2: None,
                width: VecWidth::V512,
                op: X86AesOp::Enc,
                imm: 0,
            },
            OpKind::X86Aes {
                dst: zmm1,
                src1: zmm2,
                src2: Some(zmm3),
                width: VecWidth::V512,
                op: X86AesOp::DecLast,
                imm: 1,
            },
            OpKind::X86Aes {
                dst: xmm1,
                src1: xmm2,
                src2: Some(xmm1),
                width: VecWidth::V128,
                op: X86AesOp::KeygenAssist,
                imm: 0,
            },
            OpKind::X86Aes {
                dst: x86(X86Reg::Xmm(16)),
                src1: xmm2,
                src2: None,
                width: VecWidth::V128,
                op: X86AesOp::InvMixColumns,
                imm: 0,
            },
            OpKind::X86Aes {
                dst: xmm1,
                src1: xmm2,
                src2: None,
                width: VecWidth::V128,
                op: X86AesOp::InvMixColumns,
                imm: 1,
            },
            OpKind::X86Aes {
                dst: xmm1,
                src1: xmm2,
                src2: Some(xmm1),
                width: VecWidth::V256,
                op: X86AesOp::EncLast,
                imm: 0,
            },
        ] {
            assert!(!is_x86_native_vector_op(&invalid_aes));
            assert!(!x86_gate(invalid_aes));
        }

        for invalid_sha512 in [
            OpKind::X86Sha512Msg1 {
                dst: ymm1,
                src: VReg::Virtual(VirtualId(11)),
            },
            OpKind::X86Sha512Msg1 {
                dst: xmm1,
                src: xmm2,
            },
            OpKind::X86Sha512Msg2 {
                dst: ymm1,
                src: xmm2,
            },
            OpKind::X86Sha512Rounds2 {
                dst: ymm1,
                state: xmm2,
                wk: xmm3,
            },
            OpKind::X86Sha512Rounds2 {
                dst: ymm1,
                state: ymm2,
                wk: ymm3,
            },
            OpKind::X86Sha512Msg2 {
                dst: x86(X86Reg::Ymm(16)),
                src: ymm2,
            },
        ] {
            assert!(!is_x86_native_vector_op(&invalid_sha512));
            assert!(!x86_gate(invalid_sha512));
        }

        for invalid_sm3 in [
            OpKind::X86Sm3Msg1 {
                dst: VReg::Virtual(VirtualId(12)),
                src1: xmm2,
                src2: xmm3,
            },
            OpKind::X86Sm3Msg2 {
                dst: xmm1,
                src1: ymm2,
                src2: xmm3,
            },
            OpKind::X86Sm3Rounds2 {
                dst: xmm1,
                state: xmm2,
                words: x86(X86Reg::Xmm(16)),
                imm: 0xFF,
            },
        ] {
            assert!(!is_x86_native_vector_op(&invalid_sm3));
            assert!(!x86_gate(invalid_sm3));
        }

        for invalid_sm4 in [
            OpKind::X86Sm4 {
                dst: xmm1,
                src1: ymm2,
                src2: xmm3,
                width: VecWidth::V128,
                key_schedule: false,
            },
            OpKind::X86Sm4 {
                dst: ymm1,
                src1: ymm2,
                src2: ymm3,
                width: VecWidth::V512,
                key_schedule: true,
            },
            OpKind::X86Sm4 {
                dst: x86(X86Reg::Xmm(16)),
                src1: xmm2,
                src2: xmm3,
                width: VecWidth::V128,
                key_schedule: true,
            },
        ] {
            assert!(!is_x86_native_vector_op(&invalid_sm4));
            assert!(!x86_gate(invalid_sm4));
        }

        for invalid_shift_imm in [
            OpKind::X86PackedShiftImm {
                dst: xmm1,
                src: xmm2,
                width: VecWidth::V64,
                elem: VecElementType::I16,
                shift: ShiftOp::Lsr,
                amount: 1,
                byte_lane: false,
            },
            OpKind::X86PackedShiftImm {
                dst: xmm1,
                src: xmm2,
                width: VecWidth::V128,
                elem: VecElementType::F32,
                shift: ShiftOp::Lsl,
                amount: 1,
                byte_lane: false,
            },
            OpKind::X86PackedShiftImm {
                dst: xmm1,
                src: xmm2,
                width: VecWidth::V128,
                elem: VecElementType::I16,
                shift: ShiftOp::Asr,
                amount: 1,
                byte_lane: true,
            },
            OpKind::X86PackedShiftImm {
                dst: xmm1,
                src: VReg::Virtual(VirtualId(13)),
                width: VecWidth::V128,
                elem: VecElementType::I32,
                shift: ShiftOp::Lsr,
                amount: 1,
                byte_lane: false,
            },
        ] {
            assert!(!is_x86_native_vector_op(&invalid_shift_imm));
            assert!(!x86_gate(invalid_shift_imm));
        }

        for invalid_shared_count_shift in [
            OpKind::X86PackedShift {
                dst: xmm1,
                src: xmm2,
                count: xmm3,
                width: VecWidth::V64,
                elem: VecElementType::I16,
                shift: ShiftOp::Lsr,
            },
            OpKind::X86PackedShift {
                dst: ymm1,
                src: xmm2,
                count: xmm3,
                width: VecWidth::V128,
                elem: VecElementType::I32,
                shift: ShiftOp::Lsl,
            },
            OpKind::X86PackedShift {
                dst: xmm1,
                src: xmm2,
                count: ymm3,
                width: VecWidth::V128,
                elem: VecElementType::I32,
                shift: ShiftOp::Asr,
            },
            OpKind::X86PackedShift {
                dst: xmm1,
                src: xmm2,
                count: VReg::Virtual(VirtualId(14)),
                width: VecWidth::V128,
                elem: VecElementType::I64,
                shift: ShiftOp::Lsr,
            },
            OpKind::X86PackedShift {
                dst: xmm1,
                src: xmm2,
                count: xmm3,
                width: VecWidth::V128,
                elem: VecElementType::F32,
                shift: ShiftOp::Lsl,
            },
        ] {
            assert!(!is_x86_native_vector_op(&invalid_shared_count_shift));
            assert!(!x86_gate(invalid_shared_count_shift));
        }

        let invalid_bf16_output_width = OpKind::VCvtFP32ToBF16 {
            dst: zmm1,
            src1: zmm2,
            src2: None,
            mask: Some(k4),
            width: VecWidth::V512,
            zeroing: true,
        };
        assert!(!is_x86_native_vector_op(&invalid_bf16_output_width));
        assert!(!x86_gate(invalid_bf16_output_width));

        let invalid_bf16_mask_class = OpKind::VCvtFP32ToBF16 {
            dst: ymm1,
            src1: zmm2,
            src2: None,
            mask: Some(zmm3),
            width: VecWidth::V512,
            zeroing: false,
        };
        assert!(!is_x86_native_vector_op(&invalid_bf16_mask_class));
        assert!(!x86_gate(invalid_bf16_mask_class));

        let invalid_alias = OpKind::VDotProduct {
            dst: zmm1,
            acc: zmm2,
            src1: zmm2,
            src2: zmm3,
            mask: Some(k4),
            src_elem: VecElementType::I8,
            acc_elem: VecElementType::I32,
            width: VecWidth::V512,
            src1_unsigned: true,
            saturate: false,
            zeroing: true,
        };
        assert!(!is_x86_native_vector_op(&invalid_alias));
        assert!(!x86_gate(invalid_alias));

        let invalid_signedness = OpKind::VDotProduct {
            dst: zmm1,
            acc: zmm1,
            src1: zmm2,
            src2: zmm3,
            mask: None,
            src_elem: VecElementType::I16,
            acc_elem: VecElementType::I32,
            width: VecWidth::V512,
            src1_unsigned: true,
            saturate: false,
            zeroing: false,
        };
        assert!(!is_x86_native_vector_op(&invalid_signedness));
        assert!(!x86_gate(invalid_signedness));

        let invalid_zeroing = OpKind::VDotProduct {
            dst: zmm1,
            acc: zmm1,
            src1: zmm2,
            src2: zmm3,
            mask: None,
            src_elem: VecElementType::I8,
            acc_elem: VecElementType::I32,
            width: VecWidth::V512,
            src1_unsigned: true,
            saturate: false,
            zeroing: true,
        };
        assert!(!is_x86_native_vector_op(&invalid_zeroing));
        assert!(!x86_gate(invalid_zeroing));

        let invalid_ifma_alias = OpKind::VMultiplyAdd52 {
            dst: zmm1,
            acc: zmm2,
            src1: zmm2,
            src2: zmm3,
            mask: Some(k4),
            width: VecWidth::V512,
            high: false,
            zeroing: true,
        };
        assert!(!is_x86_native_vector_op(&invalid_ifma_alias));
        assert!(!x86_gate(invalid_ifma_alias));
    }

    #[test]
    fn x86_fixed_integer_compare_gate_validates_exact_shapes_encodings_and_features() {
        let xmm1 = x86(X86Reg::Xmm(1));
        let xmm2 = x86(X86Reg::Xmm(2));
        let xmm3 = x86(X86Reg::Xmm(3));
        let ymm1 = x86(X86Reg::Ymm(1));
        let ymm2 = x86(X86Reg::Ymm(2));
        let ymm3 = x86(X86Reg::Ymm(3));

        for (kind, hint, requirements) in [
            (
                OpKind::VCmp {
                    dst: xmm1,
                    src1: xmm1,
                    src2: xmm2,
                    cond: VecCmpCond::Eq,
                    elem: VecElementType::I8,
                    lanes: 16,
                },
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode: 0x74,
                },
                (false, false, false, false),
            ),
            (
                OpKind::VCmp {
                    dst: xmm1,
                    src1: xmm1,
                    src2: xmm2,
                    cond: VecCmpCond::Eq,
                    elem: VecElementType::I64,
                    lanes: 2,
                },
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode: 0x29,
                },
                (true, false, false, false),
            ),
            (
                OpKind::VCmp {
                    dst: xmm1,
                    src1: xmm1,
                    src2: xmm2,
                    cond: VecCmpCond::Gt,
                    elem: VecElementType::I64,
                    lanes: 2,
                },
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode: 0x37,
                },
                (false, true, false, false),
            ),
            (
                OpKind::VCmp {
                    dst: xmm1,
                    src1: xmm2,
                    src2: xmm3,
                    cond: VecCmpCond::Gt,
                    elem: VecElementType::I32,
                    lanes: 4,
                },
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x66,
                    width: VecWidth::V128,
                    w: true,
                },
                (false, false, true, false),
            ),
            (
                OpKind::VCmp {
                    dst: ymm1,
                    src1: ymm2,
                    src2: ymm3,
                    cond: VecCmpCond::Eq,
                    elem: VecElementType::I16,
                    lanes: 16,
                },
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x75,
                    width: VecWidth::V256,
                    w: false,
                },
                (false, false, true, true),
            ),
        ] {
            let smir_op = crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                kind.clone(),
                hint,
            );
            assert!(is_x86_native_vector_op(&kind), "{kind:?}");
            assert!(x86_native_vector_smir_op(&smir_op), "{smir_op:?}");
            assert_eq!(
                x86_vector_integer_compare_feature_requirements(&smir_op),
                requirements,
                "{smir_op:?}"
            );

            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            builder.push_op(0x1000, kind);
            builder.set_terminator(Terminator::Return { values: vec![] });
            let mut function = builder.finish();
            function.blocks[0].ops[0].x86_hint = Some(hint);
            assert!(is_native_clobber_safe(&function), "{smir_op:?}");
        }

        let unhinted = crate::smir::ir::ops::SmirOp::new(
            crate::smir::ir::types::OpId(0),
            0x1000,
            OpKind::VCmp {
                dst: xmm1,
                src1: xmm1,
                src2: xmm2,
                cond: VecCmpCond::Eq,
                elem: VecElementType::I32,
                lanes: 4,
            },
        );
        assert!(is_x86_native_vector_op(&unhinted.kind));
        assert!(!x86_native_vector_smir_op(&unhinted));

        for malformed_kind in [
            OpKind::VCmp {
                dst: xmm1,
                src1: xmm1,
                src2: xmm2,
                cond: VecCmpCond::Ne,
                elem: VecElementType::I32,
                lanes: 4,
            },
            OpKind::VCmp {
                dst: xmm1,
                src1: xmm1,
                src2: xmm2,
                cond: VecCmpCond::Eq,
                elem: VecElementType::F32,
                lanes: 4,
            },
            OpKind::VCmp {
                dst: x86(X86Reg::Zmm(1)),
                src1: x86(X86Reg::Zmm(2)),
                src2: x86(X86Reg::Zmm(3)),
                cond: VecCmpCond::Gt,
                elem: VecElementType::I64,
                lanes: 8,
            },
            OpKind::VCmp {
                dst: xmm1,
                src1: VReg::Virtual(VirtualId(30)),
                src2: xmm2,
                cond: VecCmpCond::Eq,
                elem: VecElementType::I8,
                lanes: 16,
            },
        ] {
            assert!(!is_x86_native_vector_op(&malformed_kind));
        }

        for malformed in [
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                OpKind::VCmp {
                    dst: xmm1,
                    src1: xmm2,
                    src2: xmm3,
                    cond: VecCmpCond::Eq,
                    elem: VecElementType::I8,
                    lanes: 16,
                },
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode: 0x74,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                OpKind::VCmp {
                    dst: xmm1,
                    src1: xmm1,
                    src2: xmm2,
                    cond: VecCmpCond::Eq,
                    elem: VecElementType::I16,
                    lanes: 8,
                },
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode: 0x74,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                OpKind::VCmp {
                    dst: ymm1,
                    src1: ymm2,
                    src2: ymm3,
                    cond: VecCmpCond::Eq,
                    elem: VecElementType::I64,
                    lanes: 4,
                },
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x29,
                    width: VecWidth::V256,
                    w: false,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                OpKind::VCmp {
                    dst: x86(X86Reg::Xmm(16)),
                    src1: x86(X86Reg::Xmm(17)),
                    src2: x86(X86Reg::Xmm(18)),
                    cond: VecCmpCond::Gt,
                    elem: VecElementType::I32,
                    lanes: 4,
                },
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x66,
                    width: VecWidth::V128,
                    w: false,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                OpKind::VCmp {
                    dst: ymm1,
                    src1: ymm2,
                    src2: ymm3,
                    cond: VecCmpCond::Eq,
                    elem: VecElementType::I16,
                    lanes: 16,
                },
                X86OpHint::EvexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x75,
                    width: VecWidth::V256,
                    w: false,
                },
            ),
        ] {
            assert!(!x86_native_vector_smir_op(&malformed));
        }
    }

    #[test]
    fn x86_integer_interleave_gate_validates_blocks_encodings_registers_and_features() {
        let xmm1 = x86(X86Reg::Xmm(1));
        let xmm2 = x86(X86Reg::Xmm(2));
        let xmm3 = x86(X86Reg::Xmm(3));
        let ymm1 = x86(X86Reg::Ymm(1));
        let ymm2 = x86(X86Reg::Ymm(2));
        let ymm3 = x86(X86Reg::Ymm(3));
        let zmm16 = x86(X86Reg::Zmm(16));
        let zmm17 = x86(X86Reg::Zmm(17));
        let zmm18 = x86(X86Reg::Zmm(18));

        for (kind, hint, requirements) in [
            (
                OpKind::VInterleave {
                    dst: xmm1,
                    src1: xmm1,
                    src2: xmm2,
                    elem: VecElementType::I8,
                    lanes: 16,
                    block_lanes: 16,
                    high: false,
                },
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode: 0x60,
                },
                (false, false, false),
            ),
            (
                OpKind::VInterleave {
                    dst: xmm1,
                    src1: xmm2,
                    src2: xmm3,
                    elem: VecElementType::I64,
                    lanes: 2,
                    block_lanes: 2,
                    high: true,
                },
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x6D,
                    width: VecWidth::V128,
                    w: true,
                },
                (true, false, false),
            ),
            (
                OpKind::VInterleave {
                    dst: ymm1,
                    src1: ymm2,
                    src2: ymm3,
                    elem: VecElementType::I16,
                    lanes: 16,
                    block_lanes: 8,
                    high: true,
                },
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x69,
                    width: VecWidth::V256,
                    w: false,
                },
                (true, true, false),
            ),
            (
                OpKind::VInterleave {
                    dst: zmm16,
                    src1: zmm17,
                    src2: zmm18,
                    elem: VecElementType::I32,
                    lanes: 16,
                    block_lanes: 4,
                    high: false,
                },
                X86OpHint::EvexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x62,
                    width: VecWidth::V512,
                    w: false,
                },
                (false, false, false),
            ),
            (
                OpKind::VInterleave {
                    dst: x86(X86Reg::Xmm(16)),
                    src1: x86(X86Reg::Xmm(17)),
                    src2: x86(X86Reg::Xmm(18)),
                    elem: VecElementType::I8,
                    lanes: 16,
                    block_lanes: 16,
                    high: false,
                },
                X86OpHint::EvexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x60,
                    width: VecWidth::V128,
                    w: true,
                },
                (false, false, true),
            ),
        ] {
            let smir_op = crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                kind.clone(),
                hint,
            );
            assert!(is_x86_native_vector_op(&kind), "{kind:?}");
            assert!(x86_native_vector_smir_op(&smir_op), "{smir_op:?}");
            assert_eq!(
                x86_vector_integer_interleave_feature_requirements(&smir_op),
                requirements,
                "{smir_op:?}"
            );

            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            builder.push_op(0x1000, kind);
            builder.set_terminator(Terminator::Return { values: vec![] });
            let mut function = builder.finish();
            function.blocks[0].ops[0].x86_hint = Some(hint);
            assert!(is_native_clobber_safe(&function), "{smir_op:?}");
        }

        for malformed_kind in [
            OpKind::VInterleave {
                dst: xmm1,
                src1: xmm1,
                src2: xmm2,
                elem: VecElementType::I8,
                lanes: 16,
                block_lanes: 8,
                high: false,
            },
            OpKind::VInterleave {
                dst: xmm1,
                src1: xmm1,
                src2: xmm2,
                elem: VecElementType::F32,
                lanes: 4,
                block_lanes: 4,
                high: false,
            },
            OpKind::VInterleave {
                dst: xmm1,
                src1: VReg::Virtual(VirtualId(40)),
                src2: xmm2,
                elem: VecElementType::I32,
                lanes: 4,
                block_lanes: 4,
                high: false,
            },
            OpKind::VInterleave {
                dst: xmm1,
                src1: xmm2,
                src2: ymm3,
                elem: VecElementType::I32,
                lanes: 4,
                block_lanes: 4,
                high: false,
            },
        ] {
            assert!(!is_x86_native_vector_op(&malformed_kind));
        }

        let unhinted = crate::smir::ir::ops::SmirOp::new(
            crate::smir::ir::types::OpId(0),
            0x1000,
            OpKind::VInterleave {
                dst: xmm1,
                src1: xmm1,
                src2: xmm2,
                elem: VecElementType::I8,
                lanes: 16,
                block_lanes: 16,
                high: false,
            },
        );
        assert!(is_x86_native_vector_op(&unhinted.kind));
        assert!(!x86_native_vector_smir_op(&unhinted));

        for malformed in [
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                OpKind::VInterleave {
                    dst: xmm1,
                    src1: xmm2,
                    src2: xmm3,
                    elem: VecElementType::I8,
                    lanes: 16,
                    block_lanes: 16,
                    high: false,
                },
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode: 0x60,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                OpKind::VInterleave {
                    dst: ymm1,
                    src1: ymm2,
                    src2: ymm3,
                    elem: VecElementType::I16,
                    lanes: 16,
                    block_lanes: 8,
                    high: true,
                },
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x69,
                    width: VecWidth::V256,
                    w: false,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                OpKind::VInterleave {
                    dst: x86(X86Reg::Ymm(16)),
                    src1: x86(X86Reg::Ymm(17)),
                    src2: x86(X86Reg::Ymm(18)),
                    elem: VecElementType::I32,
                    lanes: 8,
                    block_lanes: 4,
                    high: false,
                },
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x62,
                    width: VecWidth::V256,
                    w: false,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                OpKind::VInterleave {
                    dst: zmm16,
                    src1: zmm17,
                    src2: zmm18,
                    elem: VecElementType::I32,
                    lanes: 16,
                    block_lanes: 4,
                    high: false,
                },
                X86OpHint::EvexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x62,
                    width: VecWidth::V512,
                    w: true,
                },
            ),
        ] {
            assert!(!x86_native_vector_smir_op(&malformed));
        }
    }

    #[test]
    fn x86_saturating_pack_gate_validates_shapes_encodings_aliases_and_features() {
        let xmm1 = x86(X86Reg::Xmm(1));
        let xmm2 = x86(X86Reg::Xmm(2));
        let xmm3 = x86(X86Reg::Xmm(3));
        let ymm1 = x86(X86Reg::Ymm(1));
        let ymm2 = x86(X86Reg::Ymm(2));
        let ymm3 = x86(X86Reg::Ymm(3));
        let pack =
            |dst, src1, src2, src_elem, to_unsigned, src_lanes, block_lanes| OpKind::VPackSat {
                dst,
                src1,
                src2,
                src_elem,
                to_unsigned,
                src_lanes,
                block_lanes,
            };

        for (kind, hint, requirements) in [
            (
                pack(xmm1, xmm2, xmm1, VecElementType::I16, false, 8, 8),
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode: 0x63,
                },
                (false, false, false, false),
            ),
            (
                pack(xmm1, xmm2, xmm1, VecElementType::I32, true, 4, 4),
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode: 0x2B,
                },
                (true, false, false, false),
            ),
            (
                pack(xmm1, xmm3, xmm2, VecElementType::I32, true, 4, 4),
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x2B,
                    width: VecWidth::V128,
                    w: true,
                },
                (false, true, false, false),
            ),
            (
                pack(ymm1, ymm3, ymm2, VecElementType::I32, false, 8, 4),
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x6B,
                    width: VecWidth::V256,
                    w: false,
                },
                (false, true, true, false),
            ),
            (
                pack(
                    x86(X86Reg::Zmm(16)),
                    x86(X86Reg::Zmm(18)),
                    x86(X86Reg::Zmm(17)),
                    VecElementType::I16,
                    true,
                    32,
                    8,
                ),
                X86OpHint::EvexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x67,
                    width: VecWidth::V512,
                    w: true,
                },
                (false, false, false, false),
            ),
            (
                pack(
                    x86(X86Reg::Xmm(16)),
                    x86(X86Reg::Xmm(18)),
                    x86(X86Reg::Xmm(17)),
                    VecElementType::I32,
                    true,
                    4,
                    4,
                ),
                X86OpHint::EvexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x2B,
                    width: VecWidth::V128,
                    w: false,
                },
                (false, false, false, true),
            ),
        ] {
            let smir_op = crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                kind.clone(),
                hint,
            );
            assert!(is_x86_native_vector_op(&kind), "{kind:?}");
            assert!(x86_native_vector_smir_op(&smir_op), "{smir_op:?}");
            assert_eq!(
                x86_vector_integer_pack_feature_requirements(&smir_op),
                requirements,
                "{smir_op:?}"
            );

            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            builder.push_op(0x1000, kind);
            builder.set_terminator(Terminator::Return { values: vec![] });
            let mut function = builder.finish();
            function.blocks[0].ops[0].x86_hint = Some(hint);
            assert!(is_native_clobber_safe(&function), "{smir_op:?}");
        }

        for malformed_kind in [
            pack(xmm1, xmm2, xmm1, VecElementType::I8, false, 16, 16),
            pack(xmm1, xmm2, xmm1, VecElementType::I16, false, 7, 8),
            pack(xmm1, xmm2, xmm1, VecElementType::I16, false, 8, 4),
            pack(
                xmm1,
                VReg::Virtual(VirtualId(50)),
                xmm1,
                VecElementType::I16,
                false,
                8,
                8,
            ),
            pack(xmm1, ymm2, xmm1, VecElementType::I32, false, 4, 4),
        ] {
            assert!(!is_x86_native_vector_op(&malformed_kind));
        }

        let unhinted = crate::smir::ir::ops::SmirOp::new(
            crate::smir::ir::types::OpId(0),
            0x1000,
            pack(xmm1, xmm2, xmm1, VecElementType::I16, false, 8, 8),
        );
        assert!(is_x86_native_vector_op(&unhinted.kind));
        assert!(!x86_native_vector_smir_op(&unhinted));

        for malformed in [
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                pack(xmm1, xmm2, xmm3, VecElementType::I16, false, 8, 8),
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode: 0x63,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                pack(xmm1, xmm2, xmm1, VecElementType::I16, false, 8, 8),
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode: 0x67,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                pack(ymm1, ymm3, ymm2, VecElementType::I32, true, 8, 4),
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x2B,
                    width: VecWidth::V256,
                    w: false,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                pack(
                    x86(X86Reg::Ymm(16)),
                    x86(X86Reg::Ymm(18)),
                    x86(X86Reg::Ymm(17)),
                    VecElementType::I16,
                    true,
                    16,
                    8,
                ),
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x67,
                    width: VecWidth::V256,
                    w: false,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                pack(
                    x86(X86Reg::Zmm(16)),
                    x86(X86Reg::Zmm(18)),
                    x86(X86Reg::Zmm(17)),
                    VecElementType::I32,
                    false,
                    16,
                    4,
                ),
                X86OpHint::EvexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x6B,
                    width: VecWidth::V512,
                    w: true,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                pack(
                    x86(X86Reg::Ymm(16)),
                    x86(X86Reg::Ymm(18)),
                    x86(X86Reg::Ymm(17)),
                    VecElementType::I16,
                    false,
                    16,
                    8,
                ),
                X86OpHint::EvexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x63,
                    width: VecWidth::V128,
                    w: false,
                },
            ),
        ] {
            assert!(!x86_native_vector_smir_op(&malformed), "{malformed:?}");
        }
    }

    #[test]
    fn x86_byte_shuffle_gate_validates_blocks_encodings_aliases_and_features() {
        let xmm1 = x86(X86Reg::Xmm(1));
        let xmm2 = x86(X86Reg::Xmm(2));
        let xmm3 = x86(X86Reg::Xmm(3));
        let ymm1 = x86(X86Reg::Ymm(1));
        let ymm2 = x86(X86Reg::Ymm(2));
        let ymm3 = x86(X86Reg::Ymm(3));
        let shuffle = |dst, src, control, lanes, block_lanes| OpKind::VByteShuffle {
            dst,
            src,
            control,
            lanes,
            block_lanes,
        };

        for (kind, hint, requirements) in [
            (
                shuffle(xmm1, xmm1, xmm2, 16, 16),
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode: 0x00,
                },
                (true, false, false, false),
            ),
            (
                shuffle(xmm1, xmm2, xmm3, 16, 16),
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x00,
                    width: VecWidth::V128,
                    w: true,
                },
                (false, true, false, false),
            ),
            (
                shuffle(ymm1, ymm2, ymm3, 32, 16),
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x00,
                    width: VecWidth::V256,
                    w: false,
                },
                (false, true, true, false),
            ),
            (
                shuffle(
                    x86(X86Reg::Zmm(16)),
                    x86(X86Reg::Zmm(17)),
                    x86(X86Reg::Zmm(18)),
                    64,
                    16,
                ),
                X86OpHint::EvexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x00,
                    width: VecWidth::V512,
                    w: true,
                },
                (false, false, false, false),
            ),
            (
                shuffle(
                    x86(X86Reg::Xmm(16)),
                    x86(X86Reg::Xmm(17)),
                    x86(X86Reg::Xmm(18)),
                    16,
                    16,
                ),
                X86OpHint::EvexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x00,
                    width: VecWidth::V128,
                    w: false,
                },
                (false, false, false, true),
            ),
        ] {
            let smir_op = crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                kind.clone(),
                hint,
            );
            assert!(is_x86_native_vector_op(&kind), "{kind:?}");
            assert!(x86_native_vector_smir_op(&smir_op), "{smir_op:?}");
            assert_eq!(
                x86_vector_byte_shuffle_feature_requirements(&smir_op),
                requirements,
                "{smir_op:?}"
            );

            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            builder.push_op(0x1000, kind);
            builder.set_terminator(Terminator::Return { values: vec![] });
            let mut function = builder.finish();
            function.blocks[0].ops[0].x86_hint = Some(hint);
            assert!(is_native_clobber_safe(&function), "{smir_op:?}");
        }

        for malformed_kind in [
            shuffle(xmm1, xmm1, xmm2, 16, 8),
            shuffle(xmm1, xmm1, xmm2, 15, 16),
            shuffle(xmm1, VReg::Virtual(VirtualId(60)), xmm2, 16, 16),
            shuffle(xmm1, xmm2, ymm3, 16, 16),
        ] {
            assert!(!is_x86_native_vector_op(&malformed_kind));
        }

        let unhinted = crate::smir::ir::ops::SmirOp::new(
            crate::smir::ir::types::OpId(0),
            0x1000,
            shuffle(xmm1, xmm1, xmm2, 16, 16),
        );
        assert!(is_x86_native_vector_op(&unhinted.kind));
        assert!(!x86_native_vector_smir_op(&unhinted));

        for malformed in [
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                shuffle(xmm1, xmm2, xmm3, 16, 16),
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode: 0x00,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                shuffle(xmm1, xmm1, xmm2, 16, 16),
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::None,
                    opcode: 0x00,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                shuffle(ymm1, ymm2, ymm3, 32, 16),
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x00,
                    width: VecWidth::V256,
                    w: false,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                shuffle(
                    x86(X86Reg::Ymm(16)),
                    x86(X86Reg::Ymm(17)),
                    x86(X86Reg::Ymm(18)),
                    32,
                    16,
                ),
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x00,
                    width: VecWidth::V256,
                    w: false,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                shuffle(
                    x86(X86Reg::Zmm(16)),
                    x86(X86Reg::Zmm(17)),
                    x86(X86Reg::Zmm(18)),
                    64,
                    16,
                ),
                X86OpHint::EvexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x01,
                    width: VecWidth::V512,
                    w: false,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                shuffle(
                    x86(X86Reg::Ymm(16)),
                    x86(X86Reg::Ymm(17)),
                    x86(X86Reg::Ymm(18)),
                    32,
                    16,
                ),
                X86OpHint::EvexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x00,
                    width: VecWidth::V128,
                    w: false,
                },
            ),
        ] {
            assert!(!x86_native_vector_smir_op(&malformed), "{malformed:?}");
        }
    }

    #[test]
    fn x86_horizontal_integer_gate_validates_modes_encodings_aliases_and_features() {
        let xmm1 = x86(X86Reg::Xmm(1));
        let xmm2 = x86(X86Reg::Xmm(2));
        let xmm3 = x86(X86Reg::Xmm(3));
        let ymm1 = x86(X86Reg::Ymm(1));
        let ymm2 = x86(X86Reg::Ymm(2));
        let ymm3 = x86(X86Reg::Ymm(3));
        let horizontal = |dst, src1, src2, elem, lanes, block_lanes, subtract, saturating| {
            OpKind::VHorizontalBin {
                dst,
                src1,
                src2,
                elem,
                lanes,
                block_lanes,
                subtract,
                saturating,
            }
        };

        for (opcode, elem, subtract, saturating) in [
            (0x01, VecElementType::I16, false, false),
            (0x02, VecElementType::I32, false, false),
            (0x03, VecElementType::I16, false, true),
            (0x05, VecElementType::I16, true, false),
            (0x06, VecElementType::I32, true, false),
            (0x07, VecElementType::I16, true, true),
        ] {
            let block_lanes = (16 / elem.bytes()) as u8;
            for (kind, hint, requirements) in [
                (
                    horizontal(
                        xmm1,
                        xmm1,
                        xmm2,
                        elem,
                        VecWidth::V128.lanes(elem) as u8,
                        block_lanes,
                        subtract,
                        saturating,
                    ),
                    X86OpHint::SseOp {
                        prefix: X86SsePrefix::OpSize,
                        opcode,
                    },
                    (true, false, false),
                ),
                (
                    horizontal(
                        ymm1,
                        ymm2,
                        ymm3,
                        elem,
                        VecWidth::V256.lanes(elem) as u8,
                        block_lanes,
                        subtract,
                        saturating,
                    ),
                    X86OpHint::VexOp {
                        map: X86VecMap::Map0F38,
                        pp: X86SsePrefix::OpSize,
                        opcode,
                        width: VecWidth::V256,
                        w: true,
                    },
                    (false, true, true),
                ),
            ] {
                let smir_op = crate::smir::ir::ops::SmirOp::with_hint(
                    crate::smir::ir::types::OpId(0),
                    0x1000,
                    kind.clone(),
                    hint,
                );
                assert!(is_x86_native_vector_op(&kind), "{kind:?}");
                assert!(x86_native_vector_smir_op(&smir_op), "{smir_op:?}");
                assert_eq!(
                    x86_vector_integer_horizontal_feature_requirements(&smir_op),
                    requirements,
                    "{smir_op:?}"
                );

                let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
                builder.push_op(0x1000, kind);
                builder.set_terminator(Terminator::Return { values: vec![] });
                let mut function = builder.finish();
                function.blocks[0].ops[0].x86_hint = Some(hint);
                assert!(is_native_clobber_safe(&function), "{smir_op:?}");
            }
        }

        let vex128 = crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            horizontal(xmm1, xmm2, xmm3, VecElementType::I16, 8, 8, false, false),
            X86OpHint::VexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::OpSize,
                opcode: 0x01,
                width: VecWidth::V128,
                w: true,
            },
        );
        assert!(x86_native_vector_smir_op(&vex128));
        assert_eq!(
            x86_vector_integer_horizontal_feature_requirements(&vex128),
            (false, true, false)
        );

        for malformed_kind in [
            horizontal(xmm1, xmm1, xmm2, VecElementType::I16, 8, 4, false, false),
            horizontal(xmm1, xmm1, xmm2, VecElementType::I32, 4, 4, false, true),
            horizontal(
                x86(X86Reg::Zmm(1)),
                x86(X86Reg::Zmm(2)),
                x86(X86Reg::Zmm(3)),
                VecElementType::I16,
                32,
                8,
                false,
                false,
            ),
            horizontal(
                xmm1,
                VReg::Virtual(VirtualId(61)),
                xmm2,
                VecElementType::I16,
                8,
                8,
                false,
                false,
            ),
            horizontal(xmm1, xmm2, ymm3, VecElementType::I16, 8, 8, false, false),
        ] {
            assert!(!is_x86_native_vector_op(&malformed_kind));
        }

        let valid = horizontal(xmm1, xmm1, xmm2, VecElementType::I16, 8, 8, false, false);
        let unhinted = crate::smir::ir::ops::SmirOp::new(
            crate::smir::ir::types::OpId(0),
            0x1000,
            valid.clone(),
        );
        assert!(is_x86_native_vector_op(&unhinted.kind));
        assert!(!x86_native_vector_smir_op(&unhinted));

        for malformed in [
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                horizontal(xmm1, xmm2, xmm3, VecElementType::I16, 8, 8, false, false),
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode: 0x01,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                valid.clone(),
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::None,
                    opcode: 0x01,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                valid,
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode: 0x02,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                horizontal(ymm1, ymm2, ymm3, VecElementType::I32, 8, 4, true, false),
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x06,
                    width: VecWidth::V256,
                    w: false,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                horizontal(
                    x86(X86Reg::Ymm(16)),
                    x86(X86Reg::Ymm(17)),
                    x86(X86Reg::Ymm(18)),
                    VecElementType::I16,
                    16,
                    8,
                    false,
                    true,
                ),
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x03,
                    width: VecWidth::V256,
                    w: false,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                horizontal(xmm1, xmm2, xmm3, VecElementType::I16, 8, 8, true, true),
                X86OpHint::EvexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x07,
                    width: VecWidth::V128,
                    w: false,
                },
            ),
        ] {
            assert!(!x86_native_vector_smir_op(&malformed), "{malformed:?}");
        }
    }

    #[test]
    fn x86_mulhrs_gate_validates_rounding_shape_encodings_aliases_and_features() {
        let xmm1 = x86(X86Reg::Xmm(1));
        let xmm2 = x86(X86Reg::Xmm(2));
        let xmm3 = x86(X86Reg::Xmm(3));
        let ymm1 = x86(X86Reg::Ymm(1));
        let ymm2 = x86(X86Reg::Ymm(2));
        let ymm3 = x86(X86Reg::Ymm(3));
        let mulhrs = |dst, src1, src2, lanes| OpKind::VMulShiftSat {
            dst,
            src1,
            src2,
            src_elem: VecElementType::I16,
            lanes,
            signed1: true,
            signed2: true,
            shift_left: 0,
            round: true,
            sat_bits: 0,
            out_shift: 15,
        };

        for (kind, hint, requirements) in [
            (
                mulhrs(xmm1, xmm1, xmm2, 8),
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode: 0x0B,
                },
                (true, false, false, false),
            ),
            (
                mulhrs(xmm1, xmm2, xmm3, 8),
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x0B,
                    width: VecWidth::V128,
                    w: true,
                },
                (false, true, false, false),
            ),
            (
                mulhrs(ymm1, ymm2, ymm3, 16),
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x0B,
                    width: VecWidth::V256,
                    w: false,
                },
                (false, true, true, false),
            ),
            (
                mulhrs(
                    x86(X86Reg::Xmm(16)),
                    x86(X86Reg::Xmm(17)),
                    x86(X86Reg::Xmm(18)),
                    8,
                ),
                X86OpHint::EvexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x0B,
                    width: VecWidth::V128,
                    w: true,
                },
                (false, false, false, true),
            ),
            (
                mulhrs(
                    x86(X86Reg::Zmm(16)),
                    x86(X86Reg::Zmm(17)),
                    x86(X86Reg::Zmm(18)),
                    32,
                ),
                X86OpHint::EvexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x0B,
                    width: VecWidth::V512,
                    w: false,
                },
                (false, false, false, false),
            ),
        ] {
            let smir_op = crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                kind.clone(),
                hint,
            );
            assert!(x86_vector_integer_mul_shift_shape_valid(&kind), "{kind:?}");
            assert!(is_x86_native_vector_op(&kind), "{kind:?}");
            assert!(x86_native_vector_smir_op(&smir_op), "{smir_op:?}");
            assert_eq!(
                x86_vector_integer_mul_shift_feature_requirements(&smir_op),
                requirements,
                "{smir_op:?}"
            );

            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            builder.push_op(0x1000, kind);
            builder.set_terminator(Terminator::Return { values: vec![] });
            let mut function = builder.finish();
            function.blocks[0].ops[0].x86_hint = Some(hint);
            assert!(is_native_clobber_safe(&function), "{smir_op:?}");
        }

        let valid = mulhrs(xmm1, xmm1, xmm2, 8);
        let unhinted = crate::smir::ir::ops::SmirOp::new(
            crate::smir::ir::types::OpId(0),
            0x1000,
            valid.clone(),
        );
        assert!(is_x86_native_vector_op(&unhinted.kind));
        assert!(!x86_native_vector_smir_op(&unhinted));

        let configured = |src_elem, signed1, signed2, shift_left, round, sat_bits, out_shift| {
            OpKind::VMulShiftSat {
                dst: xmm1,
                src1: xmm1,
                src2: xmm2,
                src_elem,
                lanes: 8,
                signed1,
                signed2,
                shift_left,
                round,
                sat_bits,
                out_shift,
            }
        };
        for malformed_kind in [
            configured(VecElementType::I32, true, true, 0, true, 0, 15),
            configured(VecElementType::I16, false, true, 0, true, 0, 15),
            configured(VecElementType::I16, true, false, 0, true, 0, 15),
            configured(VecElementType::I16, true, true, 1, true, 0, 15),
            configured(VecElementType::I16, true, true, 0, false, 0, 15),
            configured(VecElementType::I16, true, true, 0, true, 16, 15),
            configured(VecElementType::I16, true, true, 0, true, 0, 14),
            mulhrs(xmm1, xmm1, xmm2, 7),
            mulhrs(xmm1, xmm1, ymm2, 8),
            mulhrs(VReg::Virtual(VirtualId(61)), xmm1, xmm2, 8),
        ] {
            assert!(
                !is_x86_native_vector_op(&malformed_kind),
                "{malformed_kind:?}"
            );
        }

        for malformed in [
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                mulhrs(xmm1, xmm2, xmm3, 8),
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode: 0x0B,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                valid.clone(),
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::None,
                    opcode: 0x0B,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                valid,
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode: 0x0A,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                mulhrs(ymm1, ymm2, ymm3, 16),
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x0B,
                    width: VecWidth::V256,
                    w: false,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                mulhrs(ymm1, ymm2, ymm3, 16),
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x0B,
                    width: VecWidth::V128,
                    w: false,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                mulhrs(
                    x86(X86Reg::Ymm(16)),
                    x86(X86Reg::Ymm(17)),
                    x86(X86Reg::Ymm(18)),
                    16,
                ),
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x0B,
                    width: VecWidth::V256,
                    w: false,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                mulhrs(
                    x86(X86Reg::Zmm(16)),
                    x86(X86Reg::Zmm(17)),
                    x86(X86Reg::Zmm(18)),
                    32,
                ),
                X86OpHint::EvexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x0B,
                    width: VecWidth::V512,
                    w: false,
                },
            ),
        ] {
            assert!(!x86_native_vector_smir_op(&malformed), "{malformed:?}");
        }
    }

    #[test]
    fn x86_mulhw_mulhuw_gate_validates_signedness_encodings_aliases_and_features() {
        let xmm1 = x86(X86Reg::Xmm(1));
        let xmm2 = x86(X86Reg::Xmm(2));
        let xmm3 = x86(X86Reg::Xmm(3));
        let ymm1 = x86(X86Reg::Ymm(1));
        let ymm2 = x86(X86Reg::Ymm(2));
        let ymm3 = x86(X86Reg::Ymm(3));
        let mul_high = |dst, src1, src2, lanes, signed| OpKind::VMulShiftSat {
            dst,
            src1,
            src2,
            src_elem: VecElementType::I16,
            lanes,
            signed1: signed,
            signed2: signed,
            shift_left: 0,
            round: false,
            sat_bits: 0,
            out_shift: 16,
        };

        for (kind, hint, requirements) in [
            (
                mul_high(xmm1, xmm1, xmm2, 8, true),
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode: 0xE5,
                },
                (false, false, false, false),
            ),
            (
                mul_high(xmm1, xmm2, xmm3, 8, false),
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0xE4,
                    width: VecWidth::V128,
                    w: true,
                },
                (false, true, false, false),
            ),
            (
                mul_high(ymm1, ymm2, ymm3, 16, true),
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0xE5,
                    width: VecWidth::V256,
                    w: false,
                },
                (false, true, true, false),
            ),
            (
                mul_high(
                    x86(X86Reg::Xmm(16)),
                    x86(X86Reg::Xmm(17)),
                    x86(X86Reg::Xmm(18)),
                    8,
                    true,
                ),
                X86OpHint::EvexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0xE5,
                    width: VecWidth::V128,
                    w: true,
                },
                (false, false, false, true),
            ),
            (
                mul_high(
                    x86(X86Reg::Zmm(16)),
                    x86(X86Reg::Zmm(17)),
                    x86(X86Reg::Zmm(18)),
                    32,
                    false,
                ),
                X86OpHint::EvexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0xE4,
                    width: VecWidth::V512,
                    w: false,
                },
                (false, false, false, false),
            ),
        ] {
            let smir_op = crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                kind.clone(),
                hint,
            );
            assert!(x86_vector_integer_mul_shift_shape_valid(&kind), "{kind:?}");
            assert!(is_x86_native_vector_op(&kind), "{kind:?}");
            assert!(x86_native_vector_smir_op(&smir_op), "{smir_op:?}");
            assert_eq!(
                x86_vector_integer_mul_shift_feature_requirements(&smir_op),
                requirements,
                "{smir_op:?}"
            );

            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            builder.push_op(0x1000, kind);
            builder.set_terminator(Terminator::Return { values: vec![] });
            let mut function = builder.finish();
            function.blocks[0].ops[0].x86_hint = Some(hint);
            assert!(is_native_clobber_safe(&function), "{smir_op:?}");
        }

        let valid = mul_high(xmm1, xmm1, xmm2, 8, true);
        let unhinted = crate::smir::ir::ops::SmirOp::new(
            crate::smir::ir::types::OpId(0),
            0x1000,
            valid.clone(),
        );
        assert!(is_x86_native_vector_op(&unhinted.kind));
        assert!(!x86_native_vector_smir_op(&unhinted));

        for malformed in [
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                valid.clone(),
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode: 0xE4,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                mul_high(ymm1, ymm2, ymm3, 16, false),
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0xE4,
                    width: VecWidth::V256,
                    w: false,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                mul_high(
                    x86(X86Reg::Ymm(16)),
                    x86(X86Reg::Ymm(17)),
                    x86(X86Reg::Ymm(18)),
                    16,
                    true,
                ),
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0xE5,
                    width: VecWidth::V256,
                    w: false,
                },
            ),
        ] {
            assert!(!x86_native_vector_smir_op(&malformed), "{malformed:?}");
        }

        let mixed_signedness = OpKind::VMulShiftSat {
            dst: xmm1,
            src1: xmm1,
            src2: xmm2,
            src_elem: VecElementType::I16,
            lanes: 8,
            signed1: true,
            signed2: false,
            shift_left: 0,
            round: false,
            sat_bits: 0,
            out_shift: 16,
        };
        assert!(!is_x86_native_vector_op(&mixed_signedness));
    }

    #[test]
    fn x86_pavg_gate_validates_rounding_shape_encodings_aliases_and_features() {
        let xmm1 = x86(X86Reg::Xmm(1));
        let xmm2 = x86(X86Reg::Xmm(2));
        let xmm3 = x86(X86Reg::Xmm(3));
        let ymm1 = x86(X86Reg::Ymm(1));
        let ymm2 = x86(X86Reg::Ymm(2));
        let ymm3 = x86(X86Reg::Ymm(3));
        let average = |dst, src1, src2, elem, lanes| OpKind::VLane {
            dst,
            src1,
            src2,
            elem,
            lanes,
            op: VLaneOp::AvgRnd,
            signed: false,
            set_ovf: false,
        };

        for (kind, hint, requirements) in [
            (
                average(xmm1, xmm1, xmm2, VecElementType::I8, 16),
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode: 0xE0,
                },
                (false, false, false),
            ),
            (
                average(xmm1, xmm2, xmm3, VecElementType::I16, 8),
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0xE3,
                    width: VecWidth::V128,
                    w: true,
                },
                (true, false, false),
            ),
            (
                average(ymm1, ymm2, ymm3, VecElementType::I8, 32),
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0xE0,
                    width: VecWidth::V256,
                    w: false,
                },
                (true, true, false),
            ),
            (
                average(
                    x86(X86Reg::Xmm(16)),
                    x86(X86Reg::Xmm(17)),
                    x86(X86Reg::Xmm(18)),
                    VecElementType::I16,
                    8,
                ),
                X86OpHint::EvexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0xE3,
                    width: VecWidth::V128,
                    w: true,
                },
                (false, false, true),
            ),
            (
                average(
                    x86(X86Reg::Zmm(16)),
                    x86(X86Reg::Zmm(17)),
                    x86(X86Reg::Zmm(18)),
                    VecElementType::I8,
                    64,
                ),
                X86OpHint::EvexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0xE0,
                    width: VecWidth::V512,
                    w: false,
                },
                (false, false, false),
            ),
        ] {
            let smir_op = crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                kind.clone(),
                hint,
            );
            assert!(x86_vector_integer_average_shape_valid(&kind), "{kind:?}");
            assert!(is_x86_native_vector_op(&kind), "{kind:?}");
            assert!(x86_native_vector_smir_op(&smir_op), "{smir_op:?}");
            assert_eq!(
                x86_vector_integer_average_feature_requirements(&smir_op),
                requirements,
                "{smir_op:?}"
            );

            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            builder.push_op(0x1000, kind);
            builder.set_terminator(Terminator::Return { values: vec![] });
            let mut function = builder.finish();
            function.blocks[0].ops[0].x86_hint = Some(hint);
            assert!(is_native_clobber_safe(&function), "{smir_op:?}");
        }

        let valid = average(xmm1, xmm1, xmm2, VecElementType::I8, 16);
        let unhinted = crate::smir::ir::ops::SmirOp::new(
            crate::smir::ir::types::OpId(0),
            0x1000,
            valid.clone(),
        );
        assert!(is_x86_native_vector_op(&unhinted.kind));
        assert!(!x86_native_vector_smir_op(&unhinted));

        for malformed_kind in [
            OpKind::VLane {
                dst: xmm1,
                src1: xmm1,
                src2: xmm2,
                elem: VecElementType::I8,
                lanes: 16,
                op: VLaneOp::Avg,
                signed: false,
                set_ovf: false,
            },
            OpKind::VLane {
                dst: xmm1,
                src1: xmm1,
                src2: xmm2,
                elem: VecElementType::I8,
                lanes: 16,
                op: VLaneOp::AvgRnd,
                signed: true,
                set_ovf: false,
            },
            OpKind::VLane {
                dst: xmm1,
                src1: xmm1,
                src2: xmm2,
                elem: VecElementType::I8,
                lanes: 16,
                op: VLaneOp::AvgRnd,
                signed: false,
                set_ovf: true,
            },
            average(xmm1, xmm1, xmm2, VecElementType::I32, 4),
            average(xmm1, xmm1, xmm2, VecElementType::I8, 15),
            average(xmm1, xmm1, ymm3, VecElementType::I8, 16),
            average(
                xmm1,
                VReg::Virtual(VirtualId(61)),
                xmm2,
                VecElementType::I8,
                16,
            ),
        ] {
            assert!(
                !is_x86_native_vector_op(&malformed_kind),
                "{malformed_kind:?}"
            );
        }

        for malformed in [
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                average(xmm1, xmm2, xmm3, VecElementType::I8, 16),
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode: 0xE0,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                valid.clone(),
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::None,
                    opcode: 0xE0,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                valid,
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode: 0xE3,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                average(ymm1, ymm2, ymm3, VecElementType::I8, 32),
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0xE0,
                    width: VecWidth::V256,
                    w: false,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                average(
                    x86(X86Reg::Ymm(16)),
                    x86(X86Reg::Ymm(17)),
                    x86(X86Reg::Ymm(18)),
                    VecElementType::I16,
                    16,
                ),
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0xE3,
                    width: VecWidth::V256,
                    w: false,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                average(
                    x86(X86Reg::Zmm(16)),
                    x86(X86Reg::Zmm(17)),
                    x86(X86Reg::Zmm(18)),
                    VecElementType::I8,
                    64,
                ),
                X86OpHint::EvexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0xE3,
                    width: VecWidth::V512,
                    w: false,
                },
            ),
        ] {
            assert!(!x86_native_vector_smir_op(&malformed), "{malformed:?}");
        }
    }

    #[test]
    fn x86_psign_gate_validates_signed_control_shape_encodings_aliases_and_features() {
        let xmm1 = x86(X86Reg::Xmm(1));
        let xmm2 = x86(X86Reg::Xmm(2));
        let xmm3 = x86(X86Reg::Xmm(3));
        let ymm1 = x86(X86Reg::Ymm(1));
        let ymm2 = x86(X86Reg::Ymm(2));
        let ymm3 = x86(X86Reg::Ymm(3));
        let sign = |dst, src1, src2, elem, lanes| OpKind::VLane {
            dst,
            src1,
            src2,
            elem,
            lanes,
            op: VLaneOp::Sign,
            signed: true,
            set_ovf: false,
        };

        for (kind, hint, requirements) in [
            (
                sign(xmm1, xmm1, xmm2, VecElementType::I8, 16),
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode: 0x08,
                },
                (true, false, false),
            ),
            (
                sign(xmm1, xmm2, xmm1, VecElementType::I16, 8),
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x09,
                    width: VecWidth::V128,
                    w: true,
                },
                (false, true, false),
            ),
            (
                sign(ymm1, ymm1, ymm3, VecElementType::I32, 8),
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x0A,
                    width: VecWidth::V256,
                    w: false,
                },
                (false, true, true),
            ),
        ] {
            let smir_op = crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                kind.clone(),
                hint,
            );
            assert!(x86_vector_integer_sign_shape_valid(&kind), "{kind:?}");
            assert!(is_x86_native_vector_op(&kind), "{kind:?}");
            assert!(x86_native_vector_smir_op(&smir_op), "{smir_op:?}");
            assert_eq!(
                x86_vector_integer_sign_feature_requirements(&smir_op),
                requirements,
                "{smir_op:?}"
            );

            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            builder.push_op(0x1000, kind);
            builder.set_terminator(Terminator::Return { values: vec![] });
            let mut function = builder.finish();
            function.blocks[0].ops[0].x86_hint = Some(hint);
            assert!(is_native_clobber_safe(&function), "{smir_op:?}");
        }

        let valid = sign(xmm1, xmm1, xmm2, VecElementType::I8, 16);
        let unhinted = crate::smir::ir::ops::SmirOp::new(
            crate::smir::ir::types::OpId(0),
            0x1000,
            valid.clone(),
        );
        assert!(is_x86_native_vector_op(&unhinted.kind));
        assert!(!x86_native_vector_smir_op(&unhinted));

        for malformed_kind in [
            OpKind::VLane {
                dst: xmm1,
                src1: xmm1,
                src2: xmm2,
                elem: VecElementType::I8,
                lanes: 16,
                op: VLaneOp::Sign,
                signed: false,
                set_ovf: false,
            },
            OpKind::VLane {
                dst: xmm1,
                src1: xmm1,
                src2: xmm2,
                elem: VecElementType::I8,
                lanes: 16,
                op: VLaneOp::Sign,
                signed: true,
                set_ovf: true,
            },
            sign(xmm1, xmm1, xmm2, VecElementType::I64, 2),
            sign(xmm1, xmm1, xmm2, VecElementType::I8, 15),
            sign(xmm1, xmm1, ymm3, VecElementType::I8, 16),
            sign(
                xmm1,
                VReg::Virtual(VirtualId(64)),
                xmm2,
                VecElementType::I8,
                16,
            ),
            sign(
                x86(X86Reg::Zmm(1)),
                x86(X86Reg::Zmm(2)),
                x86(X86Reg::Zmm(3)),
                VecElementType::I32,
                16,
            ),
        ] {
            assert!(
                !is_x86_native_vector_op(&malformed_kind),
                "{malformed_kind:?}"
            );
        }

        for malformed in [
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                sign(xmm1, xmm2, xmm3, VecElementType::I8, 16),
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode: 0x08,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                valid.clone(),
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::None,
                    opcode: 0x08,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                valid,
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode: 0x09,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                sign(ymm1, ymm2, ymm3, VecElementType::I32, 8),
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x0A,
                    width: VecWidth::V256,
                    w: false,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                sign(ymm1, ymm2, ymm3, VecElementType::I32, 8),
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::None,
                    opcode: 0x0A,
                    width: VecWidth::V256,
                    w: false,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                sign(ymm1, ymm2, ymm3, VecElementType::I32, 8),
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x09,
                    width: VecWidth::V256,
                    w: false,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                sign(ymm1, ymm2, ymm3, VecElementType::I32, 8),
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x0A,
                    width: VecWidth::V128,
                    w: false,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                sign(
                    x86(X86Reg::Ymm(16)),
                    x86(X86Reg::Ymm(17)),
                    x86(X86Reg::Ymm(18)),
                    VecElementType::I16,
                    16,
                ),
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x09,
                    width: VecWidth::V256,
                    w: false,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                sign(xmm1, xmm2, xmm3, VecElementType::I16, 8),
                X86OpHint::EvexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x09,
                    width: VecWidth::V128,
                    w: false,
                },
            ),
        ] {
            assert!(!x86_native_vector_smir_op(&malformed), "{malformed:?}");
        }
    }

    #[test]
    fn x86_packed_integer_minmax_gate_validates_shapes_encodings_aliases_and_features() {
        let xmm1 = x86(X86Reg::Xmm(1));
        let xmm2 = x86(X86Reg::Xmm(2));
        let xmm3 = x86(X86Reg::Xmm(3));
        let ymm1 = x86(X86Reg::Ymm(1));
        let ymm2 = x86(X86Reg::Ymm(2));
        let ymm3 = x86(X86Reg::Ymm(3));
        let minmax = |dst, src1, src2, elem, lanes, op, signed| OpKind::VLane {
            dst,
            src1,
            src2,
            elem,
            lanes,
            op,
            signed,
            set_ovf: false,
        };

        for (kind, hint, requirements) in [
            (
                minmax(
                    xmm1,
                    xmm1,
                    xmm2,
                    VecElementType::I8,
                    16,
                    VLaneOp::Min,
                    false,
                ),
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode: 0xDA,
                },
                (false, false, false, false),
            ),
            (
                minmax(xmm1, xmm1, xmm2, VecElementType::I32, 4, VLaneOp::Max, true),
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode: 0x3D,
                },
                (true, false, false, false),
            ),
            (
                minmax(
                    xmm1,
                    xmm2,
                    xmm1,
                    VecElementType::I16,
                    8,
                    VLaneOp::Min,
                    false,
                ),
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x3A,
                    width: VecWidth::V128,
                    w: true,
                },
                (false, true, false, false),
            ),
            (
                minmax(
                    ymm1,
                    ymm1,
                    ymm3,
                    VecElementType::I16,
                    16,
                    VLaneOp::Max,
                    true,
                ),
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0xEE,
                    width: VecWidth::V256,
                    w: false,
                },
                (false, true, true, false),
            ),
            (
                minmax(
                    x86(X86Reg::Xmm(16)),
                    x86(X86Reg::Xmm(17)),
                    x86(X86Reg::Xmm(18)),
                    VecElementType::I32,
                    4,
                    VLaneOp::Min,
                    true,
                ),
                X86OpHint::EvexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x39,
                    width: VecWidth::V128,
                    w: false,
                },
                (false, false, false, true),
            ),
            (
                minmax(
                    x86(X86Reg::Zmm(16)),
                    x86(X86Reg::Zmm(17)),
                    x86(X86Reg::Zmm(18)),
                    VecElementType::I8,
                    64,
                    VLaneOp::Min,
                    true,
                ),
                X86OpHint::EvexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x38,
                    width: VecWidth::V512,
                    w: true,
                },
                (false, false, false, false),
            ),
            (
                minmax(
                    x86(X86Reg::Zmm(19)),
                    x86(X86Reg::Zmm(20)),
                    x86(X86Reg::Zmm(21)),
                    VecElementType::I64,
                    8,
                    VLaneOp::Max,
                    false,
                ),
                X86OpHint::EvexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x3F,
                    width: VecWidth::V512,
                    w: true,
                },
                (false, false, false, false),
            ),
        ] {
            let smir_op = crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                kind.clone(),
                hint,
            );
            assert!(x86_vector_integer_minmax_shape_valid(&kind), "{kind:?}");
            assert!(is_x86_native_vector_op(&kind), "{kind:?}");
            assert!(x86_native_vector_smir_op(&smir_op), "{smir_op:?}");
            assert_eq!(
                x86_vector_integer_minmax_feature_requirements(&smir_op),
                requirements,
                "{smir_op:?}"
            );

            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            builder.push_op(0x1000, kind);
            builder.set_terminator(Terminator::Return { values: vec![] });
            let mut function = builder.finish();
            function.blocks[0].ops[0].x86_hint = Some(hint);
            assert!(is_native_clobber_safe(&function), "{smir_op:?}");
        }

        let valid = minmax(
            xmm1,
            xmm1,
            xmm2,
            VecElementType::I8,
            16,
            VLaneOp::Min,
            false,
        );
        let unhinted = crate::smir::ir::ops::SmirOp::new(
            crate::smir::ir::types::OpId(0),
            0x1000,
            valid.clone(),
        );
        assert!(is_x86_native_vector_op(&unhinted.kind));
        assert!(!x86_native_vector_smir_op(&unhinted));

        for malformed_kind in [
            OpKind::VLane {
                dst: xmm1,
                src1: xmm1,
                src2: xmm2,
                elem: VecElementType::I8,
                lanes: 16,
                op: VLaneOp::Min,
                signed: false,
                set_ovf: true,
            },
            minmax(xmm1, xmm1, xmm2, VecElementType::F32, 4, VLaneOp::Min, true),
            minmax(
                xmm1,
                xmm1,
                xmm2,
                VecElementType::I8,
                15,
                VLaneOp::Min,
                false,
            ),
            minmax(
                xmm1,
                xmm1,
                ymm3,
                VecElementType::I8,
                16,
                VLaneOp::Min,
                false,
            ),
            minmax(
                VReg::Virtual(VirtualId(65)),
                xmm1,
                xmm2,
                VecElementType::I8,
                16,
                VLaneOp::Min,
                false,
            ),
        ] {
            assert!(
                !is_x86_native_vector_op(&malformed_kind),
                "{malformed_kind:?}"
            );
        }

        for malformed in [
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                minmax(
                    xmm1,
                    xmm2,
                    xmm3,
                    VecElementType::I8,
                    16,
                    VLaneOp::Min,
                    false,
                ),
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode: 0xDA,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                valid.clone(),
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::None,
                    opcode: 0xDA,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                valid,
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode: 0xDE,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                minmax(
                    ymm1,
                    ymm2,
                    ymm3,
                    VecElementType::I16,
                    16,
                    VLaneOp::Max,
                    true,
                ),
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0xEE,
                    width: VecWidth::V256,
                    w: false,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                minmax(
                    x86(X86Reg::Ymm(16)),
                    x86(X86Reg::Ymm(17)),
                    x86(X86Reg::Ymm(18)),
                    VecElementType::I16,
                    16,
                    VLaneOp::Max,
                    true,
                ),
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0xEE,
                    width: VecWidth::V256,
                    w: false,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                minmax(
                    xmm1,
                    xmm2,
                    xmm3,
                    VecElementType::I64,
                    2,
                    VLaneOp::Max,
                    false,
                ),
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x3F,
                    width: VecWidth::V128,
                    w: true,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                minmax(
                    x86(X86Reg::Xmm(16)),
                    x86(X86Reg::Xmm(17)),
                    x86(X86Reg::Xmm(18)),
                    VecElementType::I32,
                    4,
                    VLaneOp::Min,
                    true,
                ),
                X86OpHint::EvexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x39,
                    width: VecWidth::V128,
                    w: true,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                minmax(
                    x86(X86Reg::Zmm(16)),
                    x86(X86Reg::Zmm(17)),
                    x86(X86Reg::Zmm(18)),
                    VecElementType::I64,
                    8,
                    VLaneOp::Max,
                    false,
                ),
                X86OpHint::EvexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x3F,
                    width: VecWidth::V512,
                    w: false,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                minmax(
                    x86(X86Reg::Ymm(16)),
                    x86(X86Reg::Ymm(17)),
                    x86(X86Reg::Ymm(18)),
                    VecElementType::I8,
                    32,
                    VLaneOp::Min,
                    true,
                ),
                X86OpHint::EvexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x38,
                    width: VecWidth::V128,
                    w: false,
                },
            ),
        ] {
            assert!(!x86_native_vector_smir_op(&malformed), "{malformed:?}");
        }
    }

    #[test]
    fn x86_psadbw_gate_validates_widths_encodings_aliases_and_features() {
        let xmm1 = x86(X86Reg::Xmm(1));
        let xmm2 = x86(X86Reg::Xmm(2));
        let xmm3 = x86(X86Reg::Xmm(3));
        let ymm1 = x86(X86Reg::Ymm(1));
        let ymm2 = x86(X86Reg::Ymm(2));
        let ymm3 = x86(X86Reg::Ymm(3));
        let sad = |dst, src1, src2, width| OpKind::VSadBytes {
            dst,
            src1,
            src2,
            width,
        };

        for (kind, hint, requirements) in [
            (
                sad(xmm1, xmm1, xmm2, VecWidth::V128),
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode: 0xF6,
                },
                (false, false, false),
            ),
            (
                sad(xmm1, xmm2, xmm3, VecWidth::V128),
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0xF6,
                    width: VecWidth::V128,
                    w: true,
                },
                (true, false, false),
            ),
            (
                sad(ymm1, ymm2, ymm3, VecWidth::V256),
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0xF6,
                    width: VecWidth::V256,
                    w: false,
                },
                (true, true, false),
            ),
            (
                sad(
                    x86(X86Reg::Xmm(16)),
                    x86(X86Reg::Xmm(17)),
                    x86(X86Reg::Xmm(18)),
                    VecWidth::V128,
                ),
                X86OpHint::EvexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0xF6,
                    width: VecWidth::V128,
                    w: true,
                },
                (false, false, true),
            ),
            (
                sad(
                    x86(X86Reg::Zmm(16)),
                    x86(X86Reg::Zmm(17)),
                    x86(X86Reg::Zmm(18)),
                    VecWidth::V512,
                ),
                X86OpHint::EvexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0xF6,
                    width: VecWidth::V512,
                    w: false,
                },
                (false, false, false),
            ),
        ] {
            let smir_op = crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                kind.clone(),
                hint,
            );
            assert!(x86_vector_sad_bytes_shape_valid(&kind), "{kind:?}");
            assert!(is_x86_native_vector_op(&kind), "{kind:?}");
            assert!(x86_native_vector_smir_op(&smir_op), "{smir_op:?}");
            assert_eq!(
                x86_vector_sad_bytes_feature_requirements(&smir_op),
                requirements,
                "{smir_op:?}"
            );

            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            builder.push_op(0x1000, kind);
            builder.set_terminator(Terminator::Return { values: vec![] });
            let mut function = builder.finish();
            function.blocks[0].ops[0].x86_hint = Some(hint);
            assert!(is_native_clobber_safe(&function), "{smir_op:?}");
        }

        let valid = sad(xmm1, xmm1, xmm2, VecWidth::V128);
        let unhinted = crate::smir::ir::ops::SmirOp::new(
            crate::smir::ir::types::OpId(0),
            0x1000,
            valid.clone(),
        );
        assert!(is_x86_native_vector_op(&unhinted.kind));
        assert!(!x86_native_vector_smir_op(&unhinted));

        for malformed_kind in [
            sad(xmm1, xmm1, ymm2, VecWidth::V128),
            sad(ymm1, ymm2, ymm3, VecWidth::V128),
            sad(VReg::Virtual(VirtualId(61)), xmm1, xmm2, VecWidth::V128),
        ] {
            assert!(
                !is_x86_native_vector_op(&malformed_kind),
                "{malformed_kind:?}"
            );
        }

        for malformed in [
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                sad(xmm1, xmm2, xmm3, VecWidth::V128),
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode: 0xF6,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                valid.clone(),
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::None,
                    opcode: 0xF6,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                valid,
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode: 0xE0,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                sad(ymm1, ymm2, ymm3, VecWidth::V256),
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0xF6,
                    width: VecWidth::V256,
                    w: false,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                sad(
                    x86(X86Reg::Ymm(16)),
                    x86(X86Reg::Ymm(17)),
                    x86(X86Reg::Ymm(18)),
                    VecWidth::V256,
                ),
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0xF6,
                    width: VecWidth::V256,
                    w: false,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                sad(
                    x86(X86Reg::Zmm(16)),
                    x86(X86Reg::Zmm(17)),
                    x86(X86Reg::Zmm(18)),
                    VecWidth::V512,
                ),
                X86OpHint::EvexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0xF6,
                    width: VecWidth::V256,
                    w: false,
                },
            ),
        ] {
            assert!(!x86_native_vector_smir_op(&malformed), "{malformed:?}");
        }
    }

    #[test]
    fn x86_phminposuw_gate_validates_registers_encodings_wig_and_features() {
        let xmm1 = x86(X86Reg::Xmm(1));
        let xmm2 = x86(X86Reg::Xmm(2));
        let minpos = |dst, src| OpKind::X86Phminposuw { dst, src };

        for (kind, hint, requirements) in [
            (
                minpos(xmm1, xmm2),
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode: 0x41,
                },
                (true, false),
            ),
            (
                minpos(xmm1, xmm1),
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x41,
                    width: VecWidth::V128,
                    w: true,
                },
                (false, true),
            ),
        ] {
            let smir_op = crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                kind.clone(),
                hint,
            );
            assert!(x86_phminposuw_shape_valid(&kind), "{kind:?}");
            assert!(is_x86_native_vector_op(&kind), "{kind:?}");
            assert!(x86_native_vector_smir_op(&smir_op), "{smir_op:?}");
            assert_eq!(
                x86_phminposuw_feature_requirements(&smir_op),
                requirements,
                "{smir_op:?}"
            );

            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            builder.push_op(0x1000, kind);
            builder.set_terminator(Terminator::Return { values: vec![] });
            let mut function = builder.finish();
            function.blocks[0].ops[0].x86_hint = Some(hint);
            assert!(is_native_clobber_safe(&function), "{smir_op:?}");
        }

        let unhinted = crate::smir::ir::ops::SmirOp::new(
            crate::smir::ir::types::OpId(0),
            0x1000,
            minpos(xmm1, xmm2),
        );
        assert!(is_x86_native_vector_op(&unhinted.kind));
        assert!(!x86_native_vector_smir_op(&unhinted));

        for malformed_kind in [
            minpos(x86(X86Reg::Xmm(16)), xmm2),
            minpos(xmm1, x86(X86Reg::Ymm(2))),
            minpos(VReg::Virtual(VirtualId(62)), xmm2),
        ] {
            assert!(!x86_phminposuw_shape_valid(&malformed_kind));
            assert!(!is_x86_native_vector_op(&malformed_kind));
        }

        for (hint, label) in [
            (
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::None,
                    opcode: 0x41,
                },
                "legacy mandatory prefix",
            ),
            (
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F3A,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x41,
                    width: VecWidth::V128,
                    w: false,
                },
                "VEX map",
            ),
            (
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x40,
                    width: VecWidth::V128,
                    w: false,
                },
                "VEX opcode",
            ),
            (
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x41,
                    width: VecWidth::V256,
                    w: false,
                },
                "VEX.L",
            ),
            (
                X86OpHint::EvexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x41,
                    width: VecWidth::V128,
                    w: false,
                },
                "nonexistent EVEX form",
            ),
        ] {
            let malformed = crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                minpos(xmm1, xmm2),
                hint,
            );
            assert!(!x86_native_vector_smir_op(&malformed), "{label}");
        }
    }

    #[test]
    fn x86_movd_q_gate_validates_direction_width_upper_state_and_encoding() {
        let gpr = |reg| x86(reg);
        let xmm = |index| x86(X86Reg::Xmm(index));
        let movd_q = |dst, src, width, zero_upper| OpKind::X86MovdQ {
            dst,
            src,
            width,
            zero_upper,
        };

        for (kind, hint) in [
            (
                movd_q(xmm(1), gpr(X86Reg::Rax), OpWidth::W32, false),
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode: 0x6E,
                },
            ),
            (
                movd_q(gpr(X86Reg::R8), xmm(9), OpWidth::W64, false),
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode: 0x7E,
                },
            ),
            (
                movd_q(xmm(2), gpr(X86Reg::Rdx), OpWidth::W64, true),
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x6E,
                    width: VecWidth::V128,
                    w: true,
                },
            ),
            (
                movd_q(gpr(X86Reg::R9), xmm(10), OpWidth::W32, false),
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x7E,
                    width: VecWidth::V128,
                    w: false,
                },
            ),
            (
                movd_q(xmm(17), gpr(X86Reg::R10), OpWidth::W64, true),
                X86OpHint::EvexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x6E,
                    width: VecWidth::V128,
                    w: true,
                },
            ),
            (
                movd_q(gpr(X86Reg::R11), xmm(18), OpWidth::W32, false),
                X86OpHint::EvexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x7E,
                    width: VecWidth::V128,
                    w: false,
                },
            ),
        ] {
            let smir_op = crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                kind.clone(),
                hint,
            );
            assert!(x86_movd_q_shape_valid(&kind), "{kind:?}");
            assert!(is_x86_native_vector_op(&kind), "{kind:?}");
            assert!(x86_native_vector_smir_op(&smir_op), "{smir_op:?}");

            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            builder.push_op(0x1000, kind);
            builder.set_terminator(Terminator::Return { values: vec![] });
            let mut function = builder.finish();
            function.blocks[0].ops[0].x86_hint = Some(hint);
            assert!(is_native_clobber_safe(&function), "{smir_op:?}");
        }

        for malformed in [
            movd_q(xmm(1), gpr(X86Reg::Rsp), OpWidth::W32, false),
            movd_q(gpr(X86Reg::Rbp), xmm(1), OpWidth::W64, false),
            movd_q(VReg::Virtual(VirtualId(63)), xmm(1), OpWidth::W32, false),
            movd_q(xmm(1), x86(X86Reg::Ymm(2)), OpWidth::W32, true),
            movd_q(gpr(X86Reg::Rax), xmm(1), OpWidth::W32, true),
            movd_q(xmm(1), gpr(X86Reg::Rax), OpWidth::W16, false),
            movd_q(xmm(32), gpr(X86Reg::Rax), OpWidth::W32, true),
        ] {
            assert!(!x86_movd_q_shape_valid(&malformed), "{malformed:?}");
            assert!(!is_x86_native_vector_op(&malformed), "{malformed:?}");
        }

        let vex_vector = movd_q(xmm(1), gpr(X86Reg::Rax), OpWidth::W32, true);
        let unhinted = crate::smir::ir::ops::SmirOp::new(
            crate::smir::ir::types::OpId(0),
            0x1000,
            vex_vector.clone(),
        );
        assert!(!x86_native_vector_smir_op(&unhinted));

        for hint in [
            X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0x6E,
            },
            X86OpHint::VexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::OpSize,
                opcode: 0x6E,
                width: VecWidth::V128,
                w: false,
            },
            X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::None,
                opcode: 0x6E,
                width: VecWidth::V128,
                w: false,
            },
            X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0x7E,
                width: VecWidth::V128,
                w: false,
            },
            X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0x6E,
                width: VecWidth::V256,
                w: false,
            },
            X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0x6E,
                width: VecWidth::V128,
                w: true,
            },
        ] {
            let malformed = crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                vex_vector.clone(),
                hint,
            );
            assert!(!x86_native_vector_smir_op(&malformed), "{malformed:?}");
        }

        let high_vex = crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            movd_q(xmm(17), gpr(X86Reg::Rax), OpWidth::W32, true),
            X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0x6E,
                width: VecWidth::V128,
                w: false,
            },
        );
        assert!(!x86_native_vector_smir_op(&high_vex));
    }

    #[test]
    fn x86_mov_mask_gate_validates_shapes_encodings_wig_and_features() {
        let xmm = |index| x86(X86Reg::Xmm(index));
        let ymm = |index| x86(X86Reg::Ymm(index));
        let mov_mask = |dst, src, elem, lanes, dst_width| OpKind::X86MovMask {
            dst,
            src,
            elem,
            lanes,
            dst_width,
        };

        for (kind, hint, requirements) in [
            (
                mov_mask(
                    x86(X86Reg::Rax),
                    xmm(1),
                    VecElementType::F32,
                    4,
                    OpWidth::W32,
                ),
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::None,
                    opcode: 0x50,
                },
                (false, false),
            ),
            (
                mov_mask(
                    x86(X86Reg::R8),
                    xmm(9),
                    VecElementType::F64,
                    2,
                    OpWidth::W64,
                ),
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode: 0x50,
                },
                (false, false),
            ),
            (
                mov_mask(
                    x86(X86Reg::R9),
                    xmm(10),
                    VecElementType::I8,
                    16,
                    OpWidth::W32,
                ),
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode: 0xD7,
                },
                (false, false),
            ),
            (
                mov_mask(
                    x86(X86Reg::R8),
                    ymm(9),
                    VecElementType::F32,
                    8,
                    OpWidth::W32,
                ),
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::None,
                    opcode: 0x50,
                    width: VecWidth::V256,
                    w: true,
                },
                (true, false),
            ),
            (
                mov_mask(
                    x86(X86Reg::Rdx),
                    ymm(1),
                    VecElementType::F64,
                    4,
                    OpWidth::W32,
                ),
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x50,
                    width: VecWidth::V256,
                    w: false,
                },
                (true, false),
            ),
            (
                mov_mask(
                    x86(X86Reg::Rax),
                    xmm(1),
                    VecElementType::I8,
                    16,
                    OpWidth::W32,
                ),
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0xD7,
                    width: VecWidth::V128,
                    w: true,
                },
                (true, false),
            ),
            (
                mov_mask(
                    x86(X86Reg::R9),
                    ymm(10),
                    VecElementType::I8,
                    32,
                    OpWidth::W32,
                ),
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0xD7,
                    width: VecWidth::V256,
                    w: true,
                },
                (true, true),
            ),
        ] {
            let smir_op = crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                kind.clone(),
                hint,
            );
            assert!(x86_mov_mask_shape_valid(&kind), "{kind:?}");
            assert!(is_x86_native_vector_op(&kind), "{kind:?}");
            assert!(x86_native_vector_smir_op(&smir_op), "{smir_op:?}");
            assert_eq!(
                x86_mov_mask_feature_requirements(&smir_op),
                requirements,
                "{smir_op:?}"
            );

            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            builder.push_op(0x1000, kind);
            builder.set_terminator(Terminator::Return { values: vec![] });
            let mut function = builder.finish();
            function.blocks[0].ops[0].x86_hint = Some(hint);
            assert!(is_native_clobber_safe(&function), "{smir_op:?}");
        }

        for malformed in [
            mov_mask(
                x86(X86Reg::Rsp),
                xmm(1),
                VecElementType::F32,
                4,
                OpWidth::W32,
            ),
            mov_mask(
                x86(X86Reg::Rbp),
                xmm(1),
                VecElementType::F32,
                4,
                OpWidth::W32,
            ),
            mov_mask(
                VReg::Virtual(VirtualId(63)),
                xmm(1),
                VecElementType::F32,
                4,
                OpWidth::W32,
            ),
            mov_mask(
                x86(X86Reg::Rax),
                xmm(16),
                VecElementType::F32,
                4,
                OpWidth::W32,
            ),
            mov_mask(
                x86(X86Reg::Rax),
                ymm(1),
                VecElementType::F32,
                4,
                OpWidth::W32,
            ),
            mov_mask(
                x86(X86Reg::Rax),
                xmm(1),
                VecElementType::I16,
                8,
                OpWidth::W32,
            ),
            mov_mask(
                x86(X86Reg::Rax),
                xmm(1),
                VecElementType::I8,
                16,
                OpWidth::W16,
            ),
        ] {
            assert!(!x86_mov_mask_shape_valid(&malformed), "{malformed:?}");
            assert!(!is_x86_native_vector_op(&malformed), "{malformed:?}");
        }

        let base = mov_mask(
            x86(X86Reg::Rax),
            xmm(1),
            VecElementType::F32,
            4,
            OpWidth::W32,
        );
        let unhinted = crate::smir::ir::ops::SmirOp::new(
            crate::smir::ir::types::OpId(0),
            0x1000,
            base.clone(),
        );
        assert!(is_x86_native_vector_op(&base));
        assert!(!x86_native_vector_smir_op(&unhinted));

        let vex_wide_destination = crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            mov_mask(
                x86(X86Reg::Rax),
                xmm(1),
                VecElementType::F32,
                4,
                OpWidth::W64,
            ),
            X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::None,
                opcode: 0x50,
                width: VecWidth::V128,
                w: false,
            },
        );
        assert!(!x86_native_vector_smir_op(&vex_wide_destination));

        for hint in [
            X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0x50,
            },
            X86OpHint::SseOp {
                prefix: X86SsePrefix::None,
                opcode: 0xD7,
            },
            X86OpHint::VexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::None,
                opcode: 0x50,
                width: VecWidth::V128,
                w: false,
            },
            X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::None,
                opcode: 0x51,
                width: VecWidth::V128,
                w: false,
            },
            X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::None,
                opcode: 0x50,
                width: VecWidth::V256,
                w: false,
            },
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::None,
                opcode: 0x50,
                width: VecWidth::V128,
                w: false,
            },
        ] {
            let malformed = crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                base.clone(),
                hint,
            );
            assert!(!x86_native_vector_smir_op(&malformed), "{malformed:?}");
        }
    }

    #[test]
    fn x86_mpsadbw_gate_validates_widths_encodings_aliases_and_features() {
        let xmm1 = x86(X86Reg::Xmm(1));
        let xmm2 = x86(X86Reg::Xmm(2));
        let xmm3 = x86(X86Reg::Xmm(3));
        let ymm1 = x86(X86Reg::Ymm(1));
        let ymm2 = x86(X86Reg::Ymm(2));
        let ymm3 = x86(X86Reg::Ymm(3));
        let mpsad = |dst, src1, src2, width, imm| OpKind::VMpsadbw {
            dst,
            src1,
            src2,
            mask: None,
            width,
            imm,
            zeroing: false,
        };

        for (kind, hint, requirements) in [
            (
                mpsad(xmm1, xmm1, xmm2, VecWidth::V128, 0xE7),
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode: 0x42,
                },
                (true, false, false),
            ),
            (
                mpsad(xmm1, xmm2, xmm3, VecWidth::V128, 0xFF),
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F3A,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x42,
                    width: VecWidth::V128,
                    w: true,
                },
                (false, true, false),
            ),
            (
                mpsad(ymm1, ymm2, ymm3, VecWidth::V256, 0x3F),
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F3A,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x42,
                    width: VecWidth::V256,
                    w: false,
                },
                (false, true, true),
            ),
        ] {
            let smir_op = crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                kind.clone(),
                hint,
            );
            assert!(x86_vector_mpsadbw_shape_valid(&kind), "{kind:?}");
            assert!(is_x86_native_vector_op(&kind), "{kind:?}");
            assert!(x86_native_vector_smir_op(&smir_op), "{smir_op:?}");
            assert_eq!(
                x86_vector_mpsadbw_feature_requirements(&smir_op),
                requirements,
                "{smir_op:?}"
            );

            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            builder.push_op(0x1000, kind);
            builder.set_terminator(Terminator::Return { values: vec![] });
            let mut function = builder.finish();
            function.blocks[0].ops[0].x86_hint = Some(hint);
            assert!(is_native_clobber_safe(&function), "{smir_op:?}");
        }

        let valid = mpsad(xmm1, xmm1, xmm2, VecWidth::V128, 0xFF);
        let unhinted = crate::smir::ir::ops::SmirOp::new(
            crate::smir::ir::types::OpId(0),
            0x1000,
            valid.clone(),
        );
        assert!(is_x86_native_vector_op(&unhinted.kind));
        assert!(!x86_native_vector_smir_op(&unhinted));

        for malformed_kind in [
            mpsad(xmm1, xmm1, ymm2, VecWidth::V128, 0),
            mpsad(ymm1, ymm2, ymm3, VecWidth::V128, 0),
            mpsad(
                x86(X86Reg::Zmm(1)),
                x86(X86Reg::Zmm(2)),
                x86(X86Reg::Zmm(3)),
                VecWidth::V512,
                0,
            ),
            mpsad(VReg::Virtual(VirtualId(61)), xmm1, xmm2, VecWidth::V128, 0),
            OpKind::VMpsadbw {
                dst: xmm1,
                src1: xmm2,
                src2: xmm3,
                mask: Some(x86(X86Reg::K(3))),
                width: VecWidth::V128,
                imm: 0,
                zeroing: false,
            },
            OpKind::VMpsadbw {
                dst: xmm1,
                src1: xmm2,
                src2: xmm3,
                mask: None,
                width: VecWidth::V128,
                imm: 0,
                zeroing: true,
            },
        ] {
            assert!(
                !is_x86_native_vector_op(&malformed_kind),
                "{malformed_kind:?}"
            );
        }

        for malformed in [
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                mpsad(xmm1, xmm2, xmm3, VecWidth::V128, 0),
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode: 0x42,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                valid.clone(),
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::None,
                    opcode: 0x42,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                valid,
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode: 0xF6,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                mpsad(ymm1, ymm2, ymm3, VecWidth::V256, 0),
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x42,
                    width: VecWidth::V256,
                    w: false,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                mpsad(ymm1, ymm2, ymm3, VecWidth::V256, 0),
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F3A,
                    pp: X86SsePrefix::None,
                    opcode: 0x42,
                    width: VecWidth::V128,
                    w: false,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                mpsad(
                    x86(X86Reg::Ymm(16)),
                    x86(X86Reg::Ymm(17)),
                    x86(X86Reg::Ymm(18)),
                    VecWidth::V256,
                    0,
                ),
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F3A,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x42,
                    width: VecWidth::V256,
                    w: false,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                mpsad(xmm1, xmm2, xmm3, VecWidth::V128, 0),
                X86OpHint::EvexOp {
                    map: X86VecMap::Map0F3A,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x42,
                    width: VecWidth::V128,
                    w: false,
                },
            ),
        ] {
            assert!(!x86_native_vector_smir_op(&malformed), "{malformed:?}");
        }
    }

    #[test]
    fn x86_maddubs_gate_validates_zero_accumulator_encodings_aliases_and_features() {
        let xmm1 = x86(X86Reg::Xmm(1));
        let xmm2 = x86(X86Reg::Xmm(2));
        let xmm3 = x86(X86Reg::Xmm(3));
        let ymm1 = x86(X86Reg::Ymm(1));
        let ymm2 = x86(X86Reg::Ymm(2));
        let ymm3 = x86(X86Reg::Ymm(3));
        let maddubs = |dst, src1, src2, width| OpKind::VDotProduct {
            dst,
            acc: VReg::Imm(0),
            src1,
            src2,
            mask: None,
            src_elem: VecElementType::I8,
            acc_elem: VecElementType::I16,
            width,
            src1_unsigned: true,
            saturate: true,
            zeroing: false,
        };

        for (kind, hint, requirements) in [
            (
                maddubs(xmm1, xmm1, xmm2, VecWidth::V128),
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode: 0x04,
                },
                (true, false, false, false),
            ),
            (
                maddubs(xmm1, xmm2, xmm3, VecWidth::V128),
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x04,
                    width: VecWidth::V128,
                    w: true,
                },
                (false, true, false, false),
            ),
            (
                maddubs(ymm1, ymm2, ymm3, VecWidth::V256),
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x04,
                    width: VecWidth::V256,
                    w: false,
                },
                (false, true, true, false),
            ),
            (
                maddubs(
                    x86(X86Reg::Xmm(16)),
                    x86(X86Reg::Xmm(17)),
                    x86(X86Reg::Xmm(18)),
                    VecWidth::V128,
                ),
                X86OpHint::EvexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x04,
                    width: VecWidth::V128,
                    w: true,
                },
                (false, false, false, true),
            ),
            (
                maddubs(
                    x86(X86Reg::Zmm(16)),
                    x86(X86Reg::Zmm(17)),
                    x86(X86Reg::Zmm(18)),
                    VecWidth::V512,
                ),
                X86OpHint::EvexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x04,
                    width: VecWidth::V512,
                    w: false,
                },
                (false, false, false, false),
            ),
        ] {
            let smir_op = crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                kind.clone(),
                hint,
            );
            assert!(x86_vector_integer_maddubs_shape_valid(&kind), "{kind:?}");
            assert!(is_x86_native_vector_op(&kind), "{kind:?}");
            assert!(x86_native_vector_smir_op(&smir_op), "{smir_op:?}");
            assert_eq!(
                x86_vector_integer_maddubs_feature_requirements(&smir_op),
                requirements,
                "{smir_op:?}"
            );

            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            builder.push_op(0x1000, kind);
            builder.set_terminator(Terminator::Return { values: vec![] });
            let mut function = builder.finish();
            function.blocks[0].ops[0].x86_hint = Some(hint);
            assert!(is_native_clobber_safe(&function), "{smir_op:?}");
        }

        let valid = maddubs(xmm1, xmm1, xmm2, VecWidth::V128);
        let unhinted = crate::smir::ir::ops::SmirOp::new(
            crate::smir::ir::types::OpId(0),
            0x1000,
            valid.clone(),
        );
        assert!(is_x86_native_vector_op(&unhinted.kind));
        assert!(!x86_native_vector_smir_op(&unhinted));

        let altered =
            |acc, mask, src_elem, acc_elem, src1_unsigned, saturate, zeroing| OpKind::VDotProduct {
                dst: xmm1,
                acc,
                src1: xmm1,
                src2: xmm2,
                mask,
                src_elem,
                acc_elem,
                width: VecWidth::V128,
                src1_unsigned,
                saturate,
                zeroing,
            };
        for malformed_kind in [
            altered(
                VReg::Imm(1),
                None,
                VecElementType::I8,
                VecElementType::I16,
                true,
                true,
                false,
            ),
            altered(
                VReg::Imm(0),
                Some(x86(X86Reg::K(1))),
                VecElementType::I8,
                VecElementType::I16,
                true,
                true,
                false,
            ),
            altered(
                VReg::Imm(0),
                None,
                VecElementType::I16,
                VecElementType::I16,
                true,
                true,
                false,
            ),
            altered(
                VReg::Imm(0),
                None,
                VecElementType::I8,
                VecElementType::I32,
                true,
                true,
                false,
            ),
            altered(
                VReg::Imm(0),
                None,
                VecElementType::I8,
                VecElementType::I16,
                false,
                true,
                false,
            ),
            altered(
                VReg::Imm(0),
                None,
                VecElementType::I8,
                VecElementType::I16,
                true,
                false,
                false,
            ),
            altered(
                VReg::Imm(0),
                None,
                VecElementType::I8,
                VecElementType::I16,
                true,
                true,
                true,
            ),
            maddubs(xmm1, xmm2, ymm3, VecWidth::V128),
            maddubs(xmm1, VReg::Virtual(VirtualId(62)), xmm2, VecWidth::V128),
        ] {
            assert!(
                !is_x86_native_vector_op(&malformed_kind),
                "{malformed_kind:?}"
            );
        }

        for malformed in [
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                maddubs(xmm1, xmm2, xmm3, VecWidth::V128),
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode: 0x04,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                valid.clone(),
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::None,
                    opcode: 0x04,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                valid,
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode: 0x05,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                maddubs(ymm1, ymm2, ymm3, VecWidth::V256),
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x04,
                    width: VecWidth::V256,
                    w: false,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                maddubs(
                    x86(X86Reg::Ymm(16)),
                    x86(X86Reg::Ymm(17)),
                    x86(X86Reg::Ymm(18)),
                    VecWidth::V256,
                ),
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x04,
                    width: VecWidth::V256,
                    w: false,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                maddubs(
                    x86(X86Reg::Zmm(16)),
                    x86(X86Reg::Zmm(17)),
                    x86(X86Reg::Zmm(18)),
                    VecWidth::V512,
                ),
                X86OpHint::EvexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x05,
                    width: VecWidth::V512,
                    w: false,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                maddubs(
                    x86(X86Reg::Ymm(16)),
                    x86(X86Reg::Ymm(17)),
                    x86(X86Reg::Ymm(18)),
                    VecWidth::V256,
                ),
                X86OpHint::EvexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x04,
                    width: VecWidth::V128,
                    w: false,
                },
            ),
        ] {
            assert!(!x86_native_vector_smir_op(&malformed), "{malformed:?}");
        }
    }

    #[test]
    fn x86_maddwd_gate_validates_wrapping_shape_encodings_aliases_and_features() {
        let xmm1 = x86(X86Reg::Xmm(1));
        let xmm2 = x86(X86Reg::Xmm(2));
        let xmm3 = x86(X86Reg::Xmm(3));
        let ymm1 = x86(X86Reg::Ymm(1));
        let ymm2 = x86(X86Reg::Ymm(2));
        let ymm3 = x86(X86Reg::Ymm(3));
        let maddwd = |dst, src1, src2, width| OpKind::VDotProduct {
            dst,
            acc: VReg::Imm(0),
            src1,
            src2,
            mask: None,
            src_elem: VecElementType::I16,
            acc_elem: VecElementType::I32,
            width,
            src1_unsigned: false,
            saturate: false,
            zeroing: false,
        };

        for (kind, hint, requirements) in [
            (
                maddwd(xmm1, xmm1, xmm2, VecWidth::V128),
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode: 0xF5,
                },
                (false, false, false),
            ),
            (
                maddwd(xmm1, xmm2, xmm3, VecWidth::V128),
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0xF5,
                    width: VecWidth::V128,
                    w: true,
                },
                (true, false, false),
            ),
            (
                maddwd(ymm1, ymm2, ymm3, VecWidth::V256),
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0xF5,
                    width: VecWidth::V256,
                    w: false,
                },
                (true, true, false),
            ),
            (
                maddwd(
                    x86(X86Reg::Xmm(16)),
                    x86(X86Reg::Xmm(17)),
                    x86(X86Reg::Xmm(18)),
                    VecWidth::V128,
                ),
                X86OpHint::EvexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0xF5,
                    width: VecWidth::V128,
                    w: true,
                },
                (false, false, true),
            ),
            (
                maddwd(
                    x86(X86Reg::Zmm(16)),
                    x86(X86Reg::Zmm(17)),
                    x86(X86Reg::Zmm(18)),
                    VecWidth::V512,
                ),
                X86OpHint::EvexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0xF5,
                    width: VecWidth::V512,
                    w: false,
                },
                (false, false, false),
            ),
        ] {
            let smir_op = crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                kind.clone(),
                hint,
            );
            assert!(x86_vector_integer_maddwd_shape_valid(&kind), "{kind:?}");
            assert!(is_x86_native_vector_op(&kind), "{kind:?}");
            assert!(x86_native_vector_smir_op(&smir_op), "{smir_op:?}");
            assert_eq!(
                x86_vector_integer_maddwd_feature_requirements(&smir_op),
                requirements,
                "{smir_op:?}"
            );

            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            builder.push_op(0x1000, kind);
            builder.set_terminator(Terminator::Return { values: vec![] });
            let mut function = builder.finish();
            function.blocks[0].ops[0].x86_hint = Some(hint);
            assert!(is_native_clobber_safe(&function), "{smir_op:?}");
        }

        let valid = maddwd(xmm1, xmm1, xmm2, VecWidth::V128);
        let unhinted = crate::smir::ir::ops::SmirOp::new(
            crate::smir::ir::types::OpId(0),
            0x1000,
            valid.clone(),
        );
        assert!(is_x86_native_vector_op(&unhinted.kind));
        assert!(!x86_native_vector_smir_op(&unhinted));

        let altered = |acc, mask, src1_unsigned, saturate, zeroing| OpKind::VDotProduct {
            dst: xmm1,
            acc,
            src1: xmm1,
            src2: xmm2,
            mask,
            src_elem: VecElementType::I16,
            acc_elem: VecElementType::I32,
            width: VecWidth::V128,
            src1_unsigned,
            saturate,
            zeroing,
        };
        for malformed_kind in [
            altered(VReg::Imm(1), None, false, false, false),
            altered(VReg::Imm(0), Some(x86(X86Reg::K(1))), false, false, false),
            altered(VReg::Imm(0), None, true, false, false),
            altered(VReg::Imm(0), None, false, true, false),
            altered(VReg::Imm(0), None, false, false, true),
            maddwd(xmm1, xmm2, ymm3, VecWidth::V128),
            maddwd(xmm1, VReg::Virtual(VirtualId(63)), xmm2, VecWidth::V128),
        ] {
            assert!(
                !is_x86_native_vector_op(&malformed_kind),
                "{malformed_kind:?}"
            );
        }

        for malformed in [
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                maddwd(xmm1, xmm2, xmm3, VecWidth::V128),
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode: 0xF5,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                valid,
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::None,
                    opcode: 0xF5,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                maddwd(ymm1, ymm2, ymm3, VecWidth::V256),
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0xF5,
                    width: VecWidth::V256,
                    w: false,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                maddwd(
                    x86(X86Reg::Ymm(16)),
                    x86(X86Reg::Ymm(17)),
                    x86(X86Reg::Ymm(18)),
                    VecWidth::V256,
                ),
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0xF5,
                    width: VecWidth::V256,
                    w: false,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                maddwd(
                    x86(X86Reg::Zmm(16)),
                    x86(X86Reg::Zmm(17)),
                    x86(X86Reg::Zmm(18)),
                    VecWidth::V512,
                ),
                X86OpHint::EvexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0xF4,
                    width: VecWidth::V512,
                    w: false,
                },
            ),
        ] {
            assert!(!x86_native_vector_smir_op(&malformed), "{malformed:?}");
        }
    }

    #[cfg(target_arch = "x86_64")]
    #[test]
    fn x86_vector_trampoline_round_trips_all_zmm_and_opmask_registers() {
        use crate::smir::lower::SmirLowerer;
        use crate::smir::lower::x86_64::X86_64Lowerer;

        if !std::is_x86_feature_detected!("avx512f") || !std::is_x86_feature_detected!("avx512bw") {
            return;
        }

        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(0x1000, OpKind::Nop);
        builder.set_terminator(Terminator::Return { values: vec![] });
        let mut lowerer = X86_64Lowerer::new();
        let lowered = lowerer
            .lower_function(&builder.finish())
            .expect("lower vector-state no-op region");
        let code = lowerer
            .finalize()
            .expect("finalize vector-state no-op region");
        let exec = ExecMem::new(&code).expect("map vector-state no-op region");

        let mut regs = GuestRegs {
            vector_active: 1,
            ..GuestRegs::default()
        };
        for register in 0..32 {
            for lane in 0..8 {
                regs.zmm[register][lane] =
                    0x5a00_0000_0000_0000 | ((register as u64) << 16) | lane as u64;
            }
        }
        for register in 0..8 {
            regs.k[register] = 0xa500_0000_0000_0000 | register as u64;
        }
        let expected_zmm = regs.zmm;
        let expected_k = regs.k;

        exec.run(lowered.entry_offset, &mut regs);

        assert_eq!(regs.zmm, expected_zmm);
        assert_eq!(regs.k, expected_k);
    }

    #[cfg(target_arch = "x86_64")]
    #[test]
    fn x86_vector_trampoline_round_trips_guest_mxcsr_and_restores_host() {
        if !std::is_x86_feature_detected!("avx512f") || !std::is_x86_feature_detected!("avx512bw") {
            return;
        }

        fn read_mxcsr() -> u32 {
            let mut value = 0u32;
            unsafe {
                core::arch::asm!(
                    "stmxcsr [{ptr}]",
                    ptr = in(reg) &mut value,
                    options(nostack, preserves_flags)
                );
            }
            value
        }

        // stmxcsr [rdi]; ldmxcsr [rsi]; ret
        let exec =
            ExecMem::new(&[0x0F, 0xAE, 0x1F, 0x0F, 0xAE, 0x16, 0xC3]).expect("map raw MXCSR block");
        let host_before = read_mxcsr();
        let mut observed = 0u32;
        let replacement = 0x5F80u32;
        let mut regs = GuestRegs {
            vector_active: 1,
            mxcsr: 0x3F80,
            ..GuestRegs::default()
        };
        regs.gpr[7] = (&mut observed as *mut u32) as u64;
        regs.gpr[6] = (&replacement as *const u32) as u64;

        exec.run(0, &mut regs);

        assert_eq!(observed, 0x3F80, "block did not observe guest MXCSR");
        assert_eq!(
            regs.mxcsr, replacement,
            "guest MXCSR write was not captured"
        );
        assert_eq!(
            regs.host_mxcsr, host_before,
            "host MXCSR save slot mismatch"
        );
        assert_eq!(
            read_mxcsr(),
            host_before,
            "guest MXCSR leaked into host Rust"
        );
    }

    #[cfg(target_arch = "x86_64")]
    #[test]
    fn x86_vector_trampoline_executes_masked_rotate_and_round_trips_state() {
        use crate::smir::lower::SmirLowerer;
        use crate::smir::lower::x86_64::X86_64Lowerer;

        if !std::is_x86_feature_detected!("avx512f") || !std::is_x86_feature_detected!("avx512bw") {
            return;
        }

        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(
            0x1000,
            OpKind::X86PackedRotate {
                dst: x86(X86Reg::Zmm(17)),
                src: x86(X86Reg::Zmm(18)),
                count: None,
                mask: Some(x86(X86Reg::K(4))),
                amount: 7,
                width: VecWidth::V512,
                elem: VecElementType::I32,
                left: true,
                zeroing: true,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });

        let mut lowerer = X86_64Lowerer::new();
        let lowered = lowerer
            .lower_function(&builder.finish())
            .expect("lower masked VPROLD region");
        assert!(lowered.relocations.is_empty());
        let code = lowerer.finalize().expect("finalize masked VPROLD region");
        let exec = ExecMem::new(&code).expect("map masked VPROLD region");

        let source = [
            0x0123_4567_89ab_cdef,
            0x1111_2222_3333_4444,
            0x8000_0001_7fff_ffff,
            0xdead_beef_cafe_babe,
            0x0102_0304_0506_0708,
            0xf0e0_d0c0_b0a0_9080,
            0x1357_9bdf_2468_ace0,
            0xffff_ffff_0000_0001,
        ];
        let mask = 0x5555u64;
        let mut expected = [0u64; 8];
        for lane in 0..16 {
            let input = (source[lane / 2] >> ((lane % 2) * 32)) as u32;
            let output = if ((mask >> lane) & 1) != 0 {
                input.rotate_left(7)
            } else {
                0
            };
            expected[lane / 2] |= (output as u64) << ((lane % 2) * 32);
        }

        let mut regs = GuestRegs::default();
        regs.vector_active = 1;
        regs.set_zmm(17, [u64::MAX; 8]);
        regs.set_zmm(18, source);
        regs.k[4] = mask;
        exec.run(lowered.entry_offset, &mut regs);

        assert_eq!(regs.get_zmm(17), expected);
        assert_eq!(regs.get_zmm(18), source, "source ZMM must survive");
        assert_eq!(regs.k[4], mask, "source opmask must survive");
    }

    #[test]
    fn clobber_gate_accepts_valid_mulx_and_rejects_malformed_shapes() {
        let mut b = FunctionBuilder::new(FunctionId(0), 0x1000);
        b.push_op(
            0x1000,
            OpKind::MulU {
                dst_lo: x86(X86Reg::Rbx),
                dst_hi: Some(x86(X86Reg::Rcx)),
                src1: x86(X86Reg::Rdx),
                src2: SrcOperand::Reg(x86(X86Reg::Rax)),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        );
        b.set_terminator(Terminator::Return { values: vec![] });

        let mut func = b.finish();
        let op = &mut func.blocks[0].ops[0];
        assert!(op.kind.is_jit_safe(), "generic MulU stays whitelisted");
        op.x86_hint = Some(X86OpHint::Mulx);

        assert!(
            is_native_clobber_safe(&func),
            "well-formed MULX must enter its non-destructive BMI2 lowering"
        );

        let mut excluded = std::collections::HashMap::new();
        excluded.insert(func.entry, 0x1000);
        assert!(
            x86_native_scalar_features_supported_excluding(&func, &excluded),
            "an excluded MULX block has no host BMI2 requirement"
        );

        #[cfg(target_arch = "x86_64")]
        assert_eq!(
            x86_native_scalar_features_supported_excluding(
                &func,
                &std::collections::HashMap::new()
            ),
            std::is_x86_feature_detected!("bmi2")
        );
        #[cfg(not(target_arch = "x86_64"))]
        assert!(!x86_native_scalar_features_supported_excluding(
            &func,
            &std::collections::HashMap::new()
        ));

        for (name, mutate) in [
            ("missing high destination", 0u8),
            ("wrong implicit source", 1),
            ("immediate source", 2),
            ("unsupported width", 3),
            ("flag-writing form", 4),
        ] {
            let mut malformed = func.clone();
            let OpKind::MulU {
                dst_hi,
                src1,
                src2,
                width,
                flags,
                ..
            } = &mut malformed.blocks[0].ops[0].kind
            else {
                unreachable!()
            };
            match mutate {
                0 => *dst_hi = None,
                1 => *src1 = x86(X86Reg::Rax),
                2 => *src2 = SrcOperand::Imm(7),
                3 => *width = OpWidth::W16,
                4 => *flags = FlagUpdate::All,
                _ => unreachable!(),
            }
            assert!(!is_native_clobber_safe(&malformed), "{name}");
        }
    }

    #[test]
    fn scalar_count_gate_tracks_features_and_rejects_malformed_shapes() {
        let valid = [
            OpKind::Popcnt {
                dst: x86(X86Reg::R8),
                src: x86(X86Reg::Rax),
                width: OpWidth::W16,
            },
            OpKind::Ctz {
                dst: x86(X86Reg::R9),
                src: x86(X86Reg::Rbx),
                width: OpWidth::W32,
            },
            OpKind::Clz {
                dst: x86(X86Reg::R15),
                src: x86(X86Reg::R14),
                width: OpWidth::W64,
            },
        ];
        for (op, expected) in valid.iter().cloned().zip([
            (false, false, false, true, false),
            (false, true, false, false, false),
            (false, false, true, false, false),
        ]) {
            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            builder.push_op(0x1000, op);
            builder.set_terminator(Terminator::Return { values: vec![] });
            assert_eq!(
                x86_native_scalar_feature_requirements_excluding(
                    &builder.finish(),
                    &std::collections::HashMap::new()
                ),
                expected,
                "each count operation must request exactly its own host extension"
            );
        }
        for op in &valid {
            assert!(op.is_jit_safe(), "count op must be on the scalar whitelist");
            assert!(
                x86_gate(op.clone()),
                "well-formed count op must pass the clobber gate"
            );
        }

        let x86_valid = [
            (
                OpKind::X86Count {
                    dst: x86(X86Reg::R8),
                    src: x86(X86Reg::Rax),
                    width: OpWidth::W16,
                    kind: X86CountKind::Popcnt,
                    flags: FlagUpdate::All,
                },
                (false, false, false, true, false),
            ),
            (
                OpKind::X86Count {
                    dst: x86(X86Reg::R9),
                    src: x86(X86Reg::Rbx),
                    width: OpWidth::W32,
                    kind: X86CountKind::Tzcnt,
                    flags: FlagUpdate::Specific(FlagSet::CF.union(FlagSet::ZF)),
                },
                (false, true, false, false, false),
            ),
            (
                OpKind::X86Count {
                    dst: x86(X86Reg::R15),
                    src: x86(X86Reg::R14),
                    width: OpWidth::W64,
                    kind: X86CountKind::Lzcnt,
                    flags: FlagUpdate::None,
                },
                (false, false, true, false, false),
            ),
        ];
        for (op, expected) in &x86_valid {
            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            builder.push_op(0x1000, op.clone());
            builder.set_terminator(Terminator::Return { values: vec![] });
            assert_eq!(
                x86_native_scalar_feature_requirements_excluding(
                    &builder.finish(),
                    &std::collections::HashMap::new()
                ),
                *expected
            );
            assert!(op.is_jit_safe());
            assert!(x86_gate(op.clone()));
        }

        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        for (index, op) in valid.into_iter().enumerate() {
            builder.push_op(0x1000 + index as u64, op);
        }
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut excluded = std::collections::HashMap::new();
        excluded.insert(func.entry, 0x1000);
        assert!(
            x86_native_scalar_features_supported_excluding(&func, &excluded),
            "an excluded count block has no host feature requirement"
        );

        #[cfg(target_arch = "x86_64")]
        assert_eq!(
            x86_native_scalar_features_supported_excluding(
                &func,
                &std::collections::HashMap::new()
            ),
            std::is_x86_feature_detected!("popcnt")
                && std::is_x86_feature_detected!("bmi1")
                && std::is_x86_feature_detected!("lzcnt")
        );
        #[cfg(not(target_arch = "x86_64"))]
        assert!(!x86_native_scalar_features_supported_excluding(
            &func,
            &std::collections::HashMap::new()
        ));

        for (name, op) in [
            (
                "byte width",
                OpKind::Popcnt {
                    dst: x86(X86Reg::Rax),
                    src: x86(X86Reg::Rcx),
                    width: OpWidth::W8,
                },
            ),
            (
                "guest stack source",
                OpKind::Ctz {
                    dst: x86(X86Reg::Rax),
                    src: x86(X86Reg::Rsp),
                    width: OpWidth::W64,
                },
            ),
            (
                "guest frame destination",
                OpKind::Clz {
                    dst: x86(X86Reg::Rbp),
                    src: x86(X86Reg::Rax),
                    width: OpWidth::W64,
                },
            ),
            (
                "extended guest register",
                OpKind::Popcnt {
                    dst: x86(X86Reg::R16),
                    src: x86(X86Reg::Rax),
                    width: OpWidth::W32,
                },
            ),
            (
                "virtual source",
                OpKind::Ctz {
                    dst: x86(X86Reg::Rax),
                    src: VReg::Virtual(VirtualId(0)),
                    width: OpWidth::W64,
                },
            ),
            (
                "foreign architecture source",
                OpKind::Clz {
                    dst: x86(X86Reg::Rax),
                    src: arm_x(0),
                    width: OpWidth::W64,
                },
            ),
            (
                "TZCNT undefined flag request",
                OpKind::X86Count {
                    dst: x86(X86Reg::Rax),
                    src: x86(X86Reg::Rcx),
                    width: OpWidth::W64,
                    kind: X86CountKind::Tzcnt,
                    flags: FlagUpdate::All,
                },
            ),
            (
                "LZCNT overflow flag request",
                OpKind::X86Count {
                    dst: x86(X86Reg::Rax),
                    src: x86(X86Reg::Rcx),
                    width: OpWidth::W64,
                    kind: X86CountKind::Lzcnt,
                    flags: FlagUpdate::Specific(FlagSet::OF),
                },
            ),
        ] {
            assert!(!x86_gate(op), "malformed {name} count must deopt");
        }
    }

    #[test]
    fn andn_gate_accepts_only_register_bmi_and_apx_nf_shapes() {
        let defined = FlagSet::CF
            .union(FlagSet::ZF)
            .union(FlagSet::SF)
            .union(FlagSet::OF);
        for (name, op) in [
            (
                "VEX flagful",
                OpKind::AndNot {
                    dst: x86(X86Reg::R8),
                    src1: x86(X86Reg::Rax),
                    src2: SrcOperand::Reg(x86(X86Reg::Rbx)),
                    width: OpWidth::W32,
                    flags: FlagUpdate::Specific(defined),
                },
            ),
            (
                "APX NF aliased",
                OpKind::AndNot {
                    dst: x86(X86Reg::Rax),
                    src1: x86(X86Reg::Rax),
                    src2: SrcOperand::Reg(x86(X86Reg::Rbx)),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
            ),
        ] {
            assert!(
                !op.is_jit_safe(),
                "{name} must remain scoped to the x86 exact-shape gate"
            );
            assert!(x86_gate(op.clone()), "{name} must pass the exact gate");
            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            builder.push_op(0x1000, op);
            builder.set_terminator(Terminator::Return { values: vec![] });
            assert_eq!(
                x86_native_scalar_feature_requirements_excluding(
                    &builder.finish(),
                    &std::collections::HashMap::new()
                ),
                (false, false, false, false, false),
                "generic lowering must not require host BMI1"
            );
        }

        for (name, op) in [
            (
                "word width",
                OpKind::AndNot {
                    dst: x86(X86Reg::Rax),
                    src1: x86(X86Reg::Rcx),
                    src2: SrcOperand::Reg(x86(X86Reg::Rdx)),
                    width: OpWidth::W16,
                    flags: FlagUpdate::Specific(defined),
                },
            ),
            (
                "overbroad flags",
                OpKind::AndNot {
                    dst: x86(X86Reg::Rax),
                    src1: x86(X86Reg::Rcx),
                    src2: SrcOperand::Reg(x86(X86Reg::Rdx)),
                    width: OpWidth::W64,
                    flags: FlagUpdate::All,
                },
            ),
            (
                "immediate source",
                OpKind::AndNot {
                    dst: x86(X86Reg::Rax),
                    src1: x86(X86Reg::Rcx),
                    src2: SrcOperand::Imm(1),
                    width: OpWidth::W64,
                    flags: FlagUpdate::Specific(defined),
                },
            ),
            (
                "extended guest destination",
                OpKind::AndNot {
                    dst: x86(X86Reg::R16),
                    src1: x86(X86Reg::Rcx),
                    src2: SrcOperand::Reg(x86(X86Reg::Rdx)),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
            ),
            (
                "virtual source",
                OpKind::AndNot {
                    dst: x86(X86Reg::Rax),
                    src1: VReg::Virtual(VirtualId(0)),
                    src2: SrcOperand::Reg(x86(X86Reg::Rdx)),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
            ),
        ] {
            assert!(
                !op.is_jit_safe(),
                "{name} must remain outside the shared architecture whitelist"
            );
            assert!(!x86_gate(op), "malformed {name} must deopt");
        }
    }

    #[test]
    fn x86_bls_gate_is_architecture_scoped_and_requires_exact_bmi1_shapes() {
        let defined = FlagSet::CF
            .union(FlagSet::ZF)
            .union(FlagSet::SF)
            .union(FlagSet::OF);
        for op in [
            OpKind::X86Bls {
                dst: x86(X86Reg::R8),
                src: x86(X86Reg::R9),
                width: OpWidth::W32,
                kind: X86BlsKind::Blsr,
                flags: FlagUpdate::Specific(defined),
            },
            OpKind::X86Bls {
                dst: x86(X86Reg::R15),
                src: x86(X86Reg::R15),
                width: OpWidth::W64,
                kind: X86BlsKind::Blsi,
                flags: FlagUpdate::None,
            },
        ] {
            assert!(
                !op.is_jit_safe(),
                "x86 BLS must remain outside the shared architecture whitelist"
            );
            assert!(x86_gate(op.clone()), "valid x86 BLS shape must JIT");
            assert!(
                !aarch64_gate(vec![op.clone()], false),
                "x86 BLS must not enter the AArch64 native gate"
            );
            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            builder.push_op(0x1000, op);
            builder.set_terminator(Terminator::Return { values: vec![] });
            assert_eq!(
                x86_native_scalar_feature_requirements_excluding(
                    &builder.finish(),
                    &std::collections::HashMap::new()
                ),
                (false, true, false, false, false),
                "native BLS encoding requires host BMI1"
            );
        }

        for malformed in [
            OpKind::X86Bls {
                dst: x86(X86Reg::Rax),
                src: x86(X86Reg::Rbx),
                width: OpWidth::W16,
                kind: X86BlsKind::Blsmsk,
                flags: FlagUpdate::Specific(defined),
            },
            OpKind::X86Bls {
                dst: x86(X86Reg::Rsp),
                src: x86(X86Reg::Rbx),
                width: OpWidth::W64,
                kind: X86BlsKind::Blsr,
                flags: FlagUpdate::None,
            },
            OpKind::X86Bls {
                dst: x86(X86Reg::Rax),
                src: VReg::Virtual(VirtualId(0)),
                width: OpWidth::W64,
                kind: X86BlsKind::Blsi,
                flags: FlagUpdate::None,
            },
            OpKind::X86Bls {
                dst: x86(X86Reg::Rax),
                src: x86(X86Reg::Rbx),
                width: OpWidth::W64,
                kind: X86BlsKind::Blsr,
                flags: FlagUpdate::Specific(FlagSet::ZF),
            },
        ] {
            assert!(!x86_gate(malformed), "malformed BLS shape must deopt");
        }
    }

    #[test]
    fn x86_adx_gate_tracks_cpuid_shapes_architecture_and_suppressed_flag_liveness() {
        for (kind, output) in [
            (X86AdxKind::Adcx, FlagSet::CF),
            (X86AdxKind::Adox, FlagSet::OF),
        ] {
            let op = OpKind::X86Adx {
                dst: x86(X86Reg::R8),
                src1: x86(X86Reg::Rax),
                src2: x86(X86Reg::Rbx),
                width: OpWidth::W64,
                kind,
                flags: FlagUpdate::Specific(output),
            };
            assert!(!op.is_jit_safe(), "ADX remains scoped to the x86 gate");
            assert!(x86_gate(op.clone()), "valid ADX shape must JIT");
            assert!(!aarch64_gate(vec![op.clone()], false));

            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            builder.push_op(0x1000, op);
            builder.set_terminator(Terminator::Return { values: vec![] });
            let func = builder.finish();
            assert_eq!(
                x86_native_scalar_feature_requirements_excluding(
                    &func,
                    &std::collections::HashMap::new()
                ),
                (false, false, false, false, true)
            );
            #[cfg(target_arch = "x86_64")]
            assert_eq!(
                x86_native_scalar_features_supported_excluding(
                    &func,
                    &std::collections::HashMap::new()
                ),
                std::is_x86_feature_detected!("adx")
            );
        }

        let suppressed = OpKind::X86Adx {
            dst: x86(X86Reg::R8),
            src1: x86(X86Reg::Rax),
            src2: x86(X86Reg::Rbx),
            width: OpWidth::W32,
            kind: X86AdxKind::Adcx,
            flags: FlagUpdate::None,
        };
        assert!(
            !x86_gate(suppressed.clone()),
            "suppressed native CF output cannot escape a region"
        );
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(0x1000, suppressed);
        builder.push_op(
            0x1001,
            OpKind::Xor {
                dst: x86(X86Reg::Rcx),
                src1: x86(X86Reg::Rcx),
                src2: SrcOperand::Reg(x86(X86Reg::Rcx)),
                width: OpWidth::W64,
                flags: FlagUpdate::All,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        assert!(
            is_native_clobber_safe(&builder.finish()),
            "suppressed native CF output is safe when overwritten before observation"
        );

        for malformed in [
            OpKind::X86Adx {
                dst: x86(X86Reg::Rax),
                src1: x86(X86Reg::Rcx),
                src2: x86(X86Reg::Rdx),
                width: OpWidth::W16,
                kind: X86AdxKind::Adcx,
                flags: FlagUpdate::Specific(FlagSet::CF),
            },
            OpKind::X86Adx {
                dst: x86(X86Reg::Rsp),
                src1: x86(X86Reg::Rcx),
                src2: x86(X86Reg::Rdx),
                width: OpWidth::W64,
                kind: X86AdxKind::Adox,
                flags: FlagUpdate::Specific(FlagSet::OF),
            },
            OpKind::X86Adx {
                dst: x86(X86Reg::Rax),
                src1: VReg::Virtual(VirtualId(0)),
                src2: x86(X86Reg::Rdx),
                width: OpWidth::W64,
                kind: X86AdxKind::Adcx,
                flags: FlagUpdate::Specific(FlagSet::CF),
            },
            OpKind::X86Adx {
                dst: x86(X86Reg::Rax),
                src1: x86(X86Reg::Rcx),
                src2: x86(X86Reg::Rdx),
                width: OpWidth::W64,
                kind: X86AdxKind::Adox,
                flags: FlagUpdate::Specific(FlagSet::CF),
            },
        ] {
            assert!(!x86_gate(malformed), "malformed ADX shape must deopt");
        }
    }

    #[test]
    fn bmi_gate_and_feature_requirements_cover_exact_native_shapes() {
        let bextr_flags = FlagSet::CF.union(FlagSet::ZF).union(FlagSet::OF);
        let bzhi_flags = FlagSet::CF
            .union(FlagSet::ZF)
            .union(FlagSet::SF)
            .union(FlagSet::OF);
        let valid = [
            (
                OpKind::Bextr {
                    dst: x86(X86Reg::Rax),
                    src: x86(X86Reg::Rdx),
                    control: x86(X86Reg::Rcx),
                    width: OpWidth::W32,
                    flags: FlagUpdate::Specific(bextr_flags),
                },
                (false, true, false, false, false),
            ),
            (
                OpKind::Bextr {
                    dst: x86(X86Reg::R15),
                    src: x86(X86Reg::R15),
                    control: x86(X86Reg::R15),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
                (false, true, false, false, false),
            ),
            (
                OpKind::Bzhi {
                    dst: x86(X86Reg::R8),
                    src: x86(X86Reg::R9),
                    index: x86(X86Reg::R10),
                    width: OpWidth::W32,
                    flags: FlagUpdate::Specific(bzhi_flags),
                },
                (true, false, false, false, false),
            ),
            (
                OpKind::Bzhi {
                    dst: x86(X86Reg::R11),
                    src: x86(X86Reg::R11),
                    index: x86(X86Reg::R11),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
                (true, false, false, false, false),
            ),
            (
                OpKind::Pdep {
                    dst: x86(X86Reg::R12),
                    src: x86(X86Reg::R12),
                    mask: x86(X86Reg::R13),
                    width: OpWidth::W32,
                },
                (true, false, false, false, false),
            ),
            (
                OpKind::Pext {
                    dst: x86(X86Reg::R14),
                    src: x86(X86Reg::R15),
                    mask: x86(X86Reg::R14),
                    width: OpWidth::W64,
                },
                (true, false, false, false, false),
            ),
        ];

        for (op, expected_features) in &valid {
            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            builder.push_op(0x1000, op.clone());
            builder.set_terminator(Terminator::Return { values: vec![] });
            let func = builder.finish();
            assert!(op.is_jit_safe(), "{op:?} must be on the scalar whitelist");
            assert!(
                is_native_clobber_safe(&func),
                "{op:?} must pass the x86 gate"
            );
            assert_eq!(
                x86_native_scalar_feature_requirements_excluding(
                    &func,
                    &std::collections::HashMap::new()
                ),
                *expected_features,
                "{op:?} host feature requirement"
            );
        }
        assert_eq!(x86_flag_defs(&valid[0].0), bextr_flags);
        assert_eq!(x86_flag_defs(&valid[2].0), bzhi_flags);
        assert_eq!(x86_flag_defs(&valid[1].0), FlagSet::EMPTY);

        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(0x1000, valid[0].0.clone());
        builder.push_op(0x1001, valid[2].0.clone());
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();
        let mut excluded = std::collections::HashMap::new();
        excluded.insert(func.entry, 0x1000);
        assert!(x86_native_scalar_features_supported_excluding(
            &func, &excluded
        ));
        #[cfg(target_arch = "x86_64")]
        assert_eq!(
            x86_native_scalar_features_supported_excluding(
                &func,
                &std::collections::HashMap::new()
            ),
            std::is_x86_feature_detected!("bmi1") && std::is_x86_feature_detected!("bmi2")
        );
        #[cfg(not(target_arch = "x86_64"))]
        assert!(!x86_native_scalar_features_supported_excluding(
            &func,
            &std::collections::HashMap::new()
        ));

        for (name, op) in [
            (
                "BEXTR word width",
                OpKind::Bextr {
                    dst: x86(X86Reg::Rax),
                    src: x86(X86Reg::Rdx),
                    control: x86(X86Reg::Rcx),
                    width: OpWidth::W16,
                    flags: FlagUpdate::Specific(bextr_flags),
                },
            ),
            (
                "BEXTR undefined flag request",
                OpKind::Bextr {
                    dst: x86(X86Reg::Rax),
                    src: x86(X86Reg::Rdx),
                    control: x86(X86Reg::Rcx),
                    width: OpWidth::W64,
                    flags: FlagUpdate::All,
                },
            ),
            (
                "BZHI incomplete flag request",
                OpKind::Bzhi {
                    dst: x86(X86Reg::Rax),
                    src: x86(X86Reg::Rdx),
                    index: x86(X86Reg::Rcx),
                    width: OpWidth::W64,
                    flags: FlagUpdate::Specific(FlagSet::ZF),
                },
            ),
            (
                "PDEP guest stack mask",
                OpKind::Pdep {
                    dst: x86(X86Reg::Rax),
                    src: x86(X86Reg::Rdx),
                    mask: x86(X86Reg::Rsp),
                    width: OpWidth::W64,
                },
            ),
            (
                "PEXT guest frame destination",
                OpKind::Pext {
                    dst: x86(X86Reg::Rbp),
                    src: x86(X86Reg::Rdx),
                    mask: x86(X86Reg::Rcx),
                    width: OpWidth::W64,
                },
            ),
            (
                "PDEP extended guest register",
                OpKind::Pdep {
                    dst: x86(X86Reg::R16),
                    src: x86(X86Reg::Rdx),
                    mask: x86(X86Reg::Rcx),
                    width: OpWidth::W32,
                },
            ),
            (
                "PEXT virtual source",
                OpKind::Pext {
                    dst: x86(X86Reg::Rax),
                    src: VReg::Virtual(VirtualId(0)),
                    mask: x86(X86Reg::Rcx),
                    width: OpWidth::W64,
                },
            ),
            (
                "BZHI foreign index",
                OpKind::Bzhi {
                    dst: x86(X86Reg::Rax),
                    src: x86(X86Reg::Rdx),
                    index: arm_x(0),
                    width: OpWidth::W64,
                    flags: FlagUpdate::Specific(bzhi_flags),
                },
            ),
        ] {
            assert!(
                op.is_jit_safe(),
                "malformed shape remains class-whitelisted"
            );
            assert!(!x86_gate(op), "malformed {name} must deopt");
        }
    }

    #[test]
    fn cwd_gate_accepts_only_implicit_architectural_registers_and_widths() {
        for width in [OpWidth::W16, OpWidth::W32, OpWidth::W64] {
            let op = OpKind::Cwd {
                dst: x86(X86Reg::Rdx),
                src: x86(X86Reg::Rax),
                width,
            };
            assert!(
                op.is_jit_safe(),
                "{width:?} must be on the scalar whitelist"
            );
            assert!(x86_gate(op), "{width:?} must pass the exact-shape gate");
        }

        for (name, op) in [
            (
                "byte width",
                OpKind::Cwd {
                    dst: x86(X86Reg::Rdx),
                    src: x86(X86Reg::Rax),
                    width: OpWidth::W8,
                },
            ),
            (
                "wide width",
                OpKind::Cwd {
                    dst: x86(X86Reg::Rdx),
                    src: x86(X86Reg::Rax),
                    width: OpWidth::W128,
                },
            ),
            (
                "wrong source",
                OpKind::Cwd {
                    dst: x86(X86Reg::Rdx),
                    src: x86(X86Reg::Rcx),
                    width: OpWidth::W64,
                },
            ),
            (
                "wrong destination",
                OpKind::Cwd {
                    dst: x86(X86Reg::Rcx),
                    src: x86(X86Reg::Rax),
                    width: OpWidth::W64,
                },
            ),
            (
                "virtual source",
                OpKind::Cwd {
                    dst: x86(X86Reg::Rdx),
                    src: VReg::Virtual(VirtualId(0)),
                    width: OpWidth::W32,
                },
            ),
            (
                "foreign destination",
                OpKind::Cwd {
                    dst: arm_x(0),
                    src: x86(X86Reg::Rax),
                    width: OpWidth::W16,
                },
            ),
        ] {
            assert!(op.is_jit_safe(), "{name} remains class-whitelisted");
            assert!(!x86_gate(op), "malformed {name} must deopt");
        }
    }

    #[test]
    fn carry_rotate_gate_admits_only_defined_immediate_one_forms() {
        let flags = FlagUpdate::Specific(FlagSet::CF.union(FlagSet::OF));
        for (name, op) in [
            (
                "RCL byte",
                OpKind::Rcl {
                    dst: x86(X86Reg::Rax),
                    src: x86(X86Reg::Rax),
                    amount: SrcOperand::Imm(1),
                    width: OpWidth::W8,
                    flags,
                },
            ),
            (
                "RCR word",
                OpKind::Rcr {
                    dst: x86(X86Reg::Rcx),
                    src: x86(X86Reg::Rcx),
                    amount: SrcOperand::Imm(1),
                    width: OpWidth::W16,
                    flags,
                },
            ),
            (
                "RCL dword NDD",
                OpKind::Rcl {
                    dst: x86(X86Reg::R8),
                    src: x86(X86Reg::Rax),
                    amount: SrcOperand::Imm(1),
                    width: OpWidth::W32,
                    flags,
                },
            ),
            (
                "RCR qword NDD",
                OpKind::Rcr {
                    dst: x86(X86Reg::R15),
                    src: x86(X86Reg::R14),
                    amount: SrcOperand::Imm(1),
                    width: OpWidth::W64,
                    flags,
                },
            ),
        ] {
            assert!(op.is_jit_safe(), "{name} must be class-whitelisted");
            assert!(x86_gate(op), "{name} must enter native lowering");
        }

        for (name, op) in [
            (
                "multi-bit undefined OF",
                OpKind::Rcl {
                    dst: x86(X86Reg::Rax),
                    src: x86(X86Reg::Rax),
                    amount: SrcOperand::Imm(2),
                    width: OpWidth::W64,
                    flags,
                },
            ),
            (
                "variable count",
                OpKind::Rcr {
                    dst: x86(X86Reg::Rax),
                    src: x86(X86Reg::Rax),
                    amount: SrcOperand::Reg(x86(X86Reg::Rcx)),
                    width: OpWidth::W64,
                    flags,
                },
            ),
            (
                "suppressed flags",
                OpKind::Rcl {
                    dst: x86(X86Reg::Rax),
                    src: x86(X86Reg::Rax),
                    amount: SrcOperand::Imm(1),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
            ),
            (
                "overbroad flags",
                OpKind::Rcr {
                    dst: x86(X86Reg::Rax),
                    src: x86(X86Reg::Rax),
                    amount: SrcOperand::Imm(1),
                    width: OpWidth::W64,
                    flags: FlagUpdate::All,
                },
            ),
            (
                "wide operand",
                OpKind::Rcl {
                    dst: x86(X86Reg::Rax),
                    src: x86(X86Reg::Rax),
                    amount: SrcOperand::Imm(1),
                    width: OpWidth::W128,
                    flags,
                },
            ),
            (
                "extended guest register",
                OpKind::Rcr {
                    dst: x86(X86Reg::R16),
                    src: x86(X86Reg::Rax),
                    amount: SrcOperand::Imm(1),
                    width: OpWidth::W64,
                    flags,
                },
            ),
            (
                "virtual source",
                OpKind::Rcl {
                    dst: x86(X86Reg::Rax),
                    src: VReg::Virtual(VirtualId(0)),
                    amount: SrcOperand::Imm(1),
                    width: OpWidth::W32,
                    flags,
                },
            ),
            (
                "foreign destination",
                OpKind::Rcr {
                    dst: arm_x(0),
                    src: x86(X86Reg::Rax),
                    amount: SrcOperand::Imm(1),
                    width: OpWidth::W16,
                    flags,
                },
            ),
        ] {
            assert!(op.is_jit_safe(), "{name} remains class-whitelisted");
            assert!(!x86_gate(op), "malformed {name} must deopt");
        }
    }

    #[test]
    fn bswap_gate_accepts_native_gpr_widths_and_rejects_alias_hazards() {
        for op in [
            OpKind::Bswap {
                dst: x86(X86Reg::R8),
                src: x86(X86Reg::Rax),
                width: OpWidth::W16,
            },
            OpKind::Bswap {
                dst: x86(X86Reg::R9),
                src: x86(X86Reg::R9),
                width: OpWidth::W32,
            },
            OpKind::Bswap {
                dst: x86(X86Reg::R15),
                src: x86(X86Reg::R14),
                width: OpWidth::W64,
            },
        ] {
            assert!(op.is_jit_safe());
            assert!(x86_gate(op));
        }

        for (name, op) in [
            (
                "byte width",
                OpKind::Bswap {
                    dst: x86(X86Reg::Rax),
                    src: x86(X86Reg::Rcx),
                    width: OpWidth::W8,
                },
            ),
            (
                "guest stack source",
                OpKind::Bswap {
                    dst: x86(X86Reg::Rax),
                    src: x86(X86Reg::Rsp),
                    width: OpWidth::W64,
                },
            ),
            (
                "guest frame destination",
                OpKind::Bswap {
                    dst: x86(X86Reg::Rbp),
                    src: x86(X86Reg::Rax),
                    width: OpWidth::W64,
                },
            ),
            (
                "extended guest register",
                OpKind::Bswap {
                    dst: x86(X86Reg::R16),
                    src: x86(X86Reg::Rax),
                    width: OpWidth::W32,
                },
            ),
            (
                "virtual source",
                OpKind::Bswap {
                    dst: x86(X86Reg::Rax),
                    src: VReg::Virtual(VirtualId(0)),
                    width: OpWidth::W64,
                },
            ),
            (
                "foreign architecture source",
                OpKind::Bswap {
                    dst: x86(X86Reg::Rax),
                    src: arm_x(0),
                    width: OpWidth::W64,
                },
            ),
        ] {
            assert!(!x86_gate(op), "malformed {name} Bswap must deopt");
        }
    }

    #[test]
    fn xchg_gate_accepts_native_register_shapes_and_rejects_unsafe_ir() {
        for op in [
            OpKind::Xchg {
                reg1: x86(X86Reg::Rax),
                reg2: x86(X86Reg::R8),
                width: OpWidth::W16,
            },
            OpKind::Xchg {
                reg1: x86(X86Reg::R9),
                reg2: x86(X86Reg::R9),
                width: OpWidth::W32,
            },
            OpKind::Xchg {
                reg1: x86(X86Reg::R15),
                reg2: x86(X86Reg::R14),
                width: OpWidth::W64,
            },
        ] {
            assert!(op.is_jit_safe());
            assert!(x86_gate(op));
        }

        for (name, op) in [
            (
                "byte width",
                OpKind::Xchg {
                    reg1: x86(X86Reg::Rax),
                    reg2: x86(X86Reg::Rcx),
                    width: OpWidth::W8,
                },
            ),
            (
                "guest stack register",
                OpKind::Xchg {
                    reg1: x86(X86Reg::Rax),
                    reg2: x86(X86Reg::Rsp),
                    width: OpWidth::W64,
                },
            ),
            (
                "guest frame register",
                OpKind::Xchg {
                    reg1: x86(X86Reg::Rbp),
                    reg2: x86(X86Reg::Rax),
                    width: OpWidth::W64,
                },
            ),
            (
                "extended guest register",
                OpKind::Xchg {
                    reg1: x86(X86Reg::R16),
                    reg2: x86(X86Reg::Rax),
                    width: OpWidth::W32,
                },
            ),
            (
                "virtual register",
                OpKind::Xchg {
                    reg1: x86(X86Reg::Rax),
                    reg2: VReg::Virtual(VirtualId(0)),
                    width: OpWidth::W64,
                },
            ),
            (
                "foreign architecture register",
                OpKind::Xchg {
                    reg1: x86(X86Reg::Rax),
                    reg2: arm_x(0),
                    width: OpWidth::W64,
                },
            ),
        ] {
            assert!(!x86_gate(op), "malformed {name} Xchg must deopt");
        }
    }

    #[test]
    fn x86_bit_test_gate_accepts_exact_register_shapes_and_rejects_unsafe_ir() {
        for op in [
            OpKind::Bt {
                src: x86(X86Reg::R8),
                index: SrcOperand::Reg(x86(X86Reg::R9)),
                width: OpWidth::W16,
            },
            OpKind::Bts {
                dst: x86(X86Reg::R10),
                src: x86(X86Reg::R10),
                index: SrcOperand::Imm(31),
                width: OpWidth::W32,
            },
            OpKind::Btr {
                dst: x86(X86Reg::R14),
                src: x86(X86Reg::R14),
                index: SrcOperand::Imm64(63),
                width: OpWidth::W64,
            },
            OpKind::Btc {
                dst: x86(X86Reg::R15),
                src: x86(X86Reg::R15),
                index: SrcOperand::Reg(x86(X86Reg::Rax)),
                width: OpWidth::W64,
            },
        ] {
            assert!(op.is_jit_safe(), "register bit test must be whitelisted");
            assert!(x86_gate(op), "well-formed register bit test must JIT");
        }

        for (name, op) in [
            (
                "byte width",
                OpKind::Bt {
                    src: x86(X86Reg::Rax),
                    index: SrcOperand::Imm(0),
                    width: OpWidth::W8,
                },
            ),
            (
                "non-destructive update",
                OpKind::Bts {
                    dst: x86(X86Reg::Rax),
                    src: x86(X86Reg::Rcx),
                    index: SrcOperand::Imm(0),
                    width: OpWidth::W64,
                },
            ),
            (
                "guest stack operand",
                OpKind::Btr {
                    dst: x86(X86Reg::Rsp),
                    src: x86(X86Reg::Rsp),
                    index: SrcOperand::Imm(0),
                    width: OpWidth::W64,
                },
            ),
            (
                "virtual index",
                OpKind::Btc {
                    dst: x86(X86Reg::Rax),
                    src: x86(X86Reg::Rax),
                    index: SrcOperand::Reg(VReg::Virtual(VirtualId(0))),
                    width: OpWidth::W64,
                },
            ),
        ] {
            assert!(op.is_jit_safe(), "{name} remains class-whitelisted");
            assert!(!x86_gate(op), "malformed {name} bit test must deopt");
        }
    }

    #[test]
    fn x86_immediate_memory_bit_update_gate_accepts_exact_rmw_and_fails_closed() {
        let old = VReg::Virtual(VirtualId(20));
        let mask = VReg::Virtual(VirtualId(21));
        let result = VReg::Virtual(VirtualId(22));
        let address = || Address::BaseIndexScale {
            base: Some(x86(X86Reg::Rsp)),
            index: x86(X86Reg::R16),
            scale: 2,
            disp: -8,
            disp_size: DispSize::Disp8,
        };
        let build = |action: u8, mem_width: MemWidth, bit: i64| {
            let width = mem_width.to_op_width().unwrap();
            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            builder.push_op(
                0x1000,
                OpKind::Load {
                    dst: old,
                    addr: address(),
                    width: mem_width,
                    sign: SignExtend::Zero,
                },
            );
            builder.push_op(
                0x1000,
                OpKind::Mov {
                    dst: mask,
                    src: SrcOperand::Imm(1),
                    width,
                },
            );
            builder.push_op(
                0x1000,
                OpKind::Shl {
                    dst: mask,
                    src: mask,
                    amount: SrcOperand::Imm(bit),
                    width,
                    flags: FlagUpdate::None,
                },
            );
            if action == 1 {
                builder.push_op(
                    0x1000,
                    OpKind::Not {
                        dst: mask,
                        src: mask,
                        width,
                    },
                );
            }
            let compute = match action {
                0 => OpKind::Or {
                    dst: result,
                    src1: old,
                    src2: SrcOperand::Reg(mask),
                    width,
                    flags: FlagUpdate::None,
                },
                1 => OpKind::And {
                    dst: result,
                    src1: old,
                    src2: SrcOperand::Reg(mask),
                    width,
                    flags: FlagUpdate::None,
                },
                2 => OpKind::Xor {
                    dst: result,
                    src1: old,
                    src2: SrcOperand::Reg(mask),
                    width,
                    flags: FlagUpdate::None,
                },
                _ => unreachable!(),
            };
            builder.push_op(0x1000, compute);
            builder.push_op(
                0x1000,
                OpKind::Store {
                    src: result,
                    addr: address(),
                    width: mem_width,
                },
            );
            builder.push_op(
                0x1000,
                OpKind::Bt {
                    src: old,
                    index: SrcOperand::Imm(bit),
                    width,
                },
            );
            builder.set_terminator(Terminator::Return { values: vec![] });
            builder.finish()
        };
        let build_folded = |action: u8, bit: i64, mask: i64, flags: FlagUpdate| {
            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            builder.push_op(
                0x1000,
                OpKind::Load {
                    dst: old,
                    addr: address(),
                    width: MemWidth::B8,
                    sign: SignExtend::Zero,
                },
            );
            let compute = match action {
                0 => OpKind::Or {
                    dst: result,
                    src1: old,
                    src2: SrcOperand::Imm(mask),
                    width: OpWidth::W64,
                    flags,
                },
                1 => OpKind::And {
                    dst: result,
                    src1: old,
                    src2: SrcOperand::Imm(mask),
                    width: OpWidth::W64,
                    flags,
                },
                2 => OpKind::Xor {
                    dst: result,
                    src1: old,
                    src2: SrcOperand::Imm(mask),
                    width: OpWidth::W64,
                    flags,
                },
                _ => unreachable!(),
            };
            builder.push_op(0x1000, compute);
            builder.push_op(
                0x1000,
                OpKind::Store {
                    src: result,
                    addr: address(),
                    width: MemWidth::B8,
                },
            );
            builder.push_op(
                0x1000,
                OpKind::Bt {
                    src: old,
                    index: SrcOperand::Imm(bit),
                    width: OpWidth::W64,
                },
            );
            builder.set_terminator(Terminator::Return { values: vec![] });
            builder.finish()
        };

        for (action, mem_width, bit) in [
            (0, MemWidth::B2, 15),
            (1, MemWidth::B4, 31),
            (2, MemWidth::B8, 63),
        ] {
            let function = build(action, mem_width, bit);
            assert!(is_native_clobber_safe_excluding(
                &function,
                &std::collections::HashMap::new(),
                true,
            ));
            assert!(!is_native_clobber_safe_excluding(
                &function,
                &std::collections::HashMap::new(),
                false,
            ));
        }

        for function in [
            build_folded(0, 5, 1 << 5, FlagUpdate::None),
            build_folded(1, 5, !(1 << 5), FlagUpdate::None),
            build_folded(2, 63, i64::MIN, FlagUpdate::None),
        ] {
            assert!(is_native_clobber_safe_excluding(
                &function,
                &std::collections::HashMap::new(),
                true,
            ));
            assert!(!is_native_clobber_safe_excluding(
                &function,
                &std::collections::HashMap::new(),
                false,
            ));
        }

        let folded_wrong_mask = build_folded(2, 63, 1, FlagUpdate::None);
        let mut folded_wrong_replay = build_folded(0, 5, 1 << 5, FlagUpdate::None);
        let OpKind::Bt { index, .. } = &mut folded_wrong_replay.blocks[0].ops[3].kind else {
            unreachable!()
        };
        *index = SrcOperand::Imm(6);
        let folded_flagged = build_folded(1, 5, !(1 << 5), FlagUpdate::All);
        for function in [folded_wrong_mask, folded_wrong_replay, folded_flagged] {
            assert!(!is_native_clobber_safe_excluding(
                &function,
                &std::collections::HashMap::new(),
                true,
            ));
        }

        let mut signed = build(0, MemWidth::B8, 5);
        let OpKind::Load { sign, .. } = &mut signed.blocks[0].ops[0].kind else {
            unreachable!()
        };
        *sign = SignExtend::Sign;

        let mut register_index = build(0, MemWidth::B8, 5);
        let OpKind::Shl { amount, .. } = &mut register_index.blocks[0].ops[2].kind else {
            unreachable!()
        };
        *amount = SrcOperand::Reg(x86(X86Reg::Rcx));

        let mut wrong_store = build(0, MemWidth::B8, 5);
        let OpKind::Store { addr, .. } = &mut wrong_store.blocks[0].ops[4].kind else {
            unreachable!()
        };
        *addr = Address::Absolute(0x2000);

        let mut wrong_replay = build(2, MemWidth::B8, 5);
        let OpKind::Bt { index, .. } = &mut wrong_replay.blocks[0].ops[5].kind else {
            unreachable!()
        };
        *index = SrcOperand::Imm(6);

        let mut wrong_reset = build(1, MemWidth::B8, 5);
        let OpKind::Not { width, .. } = &mut wrong_reset.blocks[0].ops[3].kind else {
            unreachable!()
        };
        *width = OpWidth::W32;

        let mut flagged_compute = build(0, MemWidth::B8, 5);
        let OpKind::Or { flags, .. } = &mut flagged_compute.blocks[0].ops[3].kind else {
            unreachable!()
        };
        *flags = FlagUpdate::All;

        let mut wrong_pc = build(0, MemWidth::B8, 5);
        wrong_pc.blocks[0].ops[5].guest_pc = 0x1001;

        for function in [
            signed,
            register_index,
            wrong_store,
            wrong_replay,
            wrong_reset,
            flagged_compute,
            wrong_pc,
        ] {
            assert!(!is_native_clobber_safe_excluding(
                &function,
                &std::collections::HashMap::new(),
                true,
            ));
        }
    }

    #[test]
    fn x86_scalar_memory_destination_alu_gate_accepts_exact_replay_and_fails_closed() {
        let old = VReg::Virtual(VirtualId(30));
        let result = VReg::Virtual(VirtualId(31));
        let flags_result = VReg::Virtual(VirtualId(32));
        let address = || Address::BaseIndexScale {
            base: Some(x86(X86Reg::Rsp)),
            index: x86(X86Reg::R16),
            scale: 2,
            disp: -8,
            disp_size: DispSize::Disp8,
        };
        let alu = |tag: u8,
                   dst: VReg,
                   src1: VReg,
                   src2: SrcOperand,
                   width: OpWidth,
                   flags: FlagUpdate| {
            match tag {
                0 => OpKind::Add {
                    dst,
                    src1,
                    src2,
                    width,
                    flags,
                },
                1 => OpKind::Or {
                    dst,
                    src1,
                    src2,
                    width,
                    flags,
                },
                2 => OpKind::Adc {
                    dst,
                    src1,
                    src2,
                    width,
                    flags,
                },
                3 => OpKind::Sbb {
                    dst,
                    src1,
                    src2,
                    width,
                    flags,
                },
                4 => OpKind::And {
                    dst,
                    src1,
                    src2,
                    width,
                    flags,
                },
                5 => OpKind::Sub {
                    dst,
                    src1,
                    src2,
                    width,
                    flags,
                },
                6 => OpKind::Xor {
                    dst,
                    src1,
                    src2,
                    width,
                    flags,
                },
                _ => unreachable!(),
            }
        };
        let build = |tag: u8,
                     source: SrcOperand,
                     mem_width: MemWidth,
                     compute_flags: FlagUpdate,
                     replay_flags: FlagUpdate,
                     replay_tag: u8,
                     store_addr: Address,
                     replay_pc: u64,
                     extra_old_use: bool| {
            let width = mem_width.to_op_width().unwrap();
            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            builder.push_op(
                0x1000,
                OpKind::Load {
                    dst: old,
                    addr: address(),
                    width: mem_width,
                    sign: SignExtend::Zero,
                },
            );
            builder.push_op(
                0x1000,
                alu(tag, result, old, source.clone(), width, compute_flags),
            );
            builder.push_op(
                0x1000,
                OpKind::Store {
                    src: result,
                    addr: store_addr,
                    width: mem_width,
                },
            );
            builder.push_op(
                replay_pc,
                alu(replay_tag, flags_result, old, source, width, replay_flags),
            );
            if extra_old_use {
                builder.push_op(
                    0x1001,
                    OpKind::Mov {
                        dst: x86(X86Reg::R11),
                        src: SrcOperand::Reg(old),
                        width,
                    },
                );
            }
            builder.set_terminator(Terminator::Return { values: vec![] });
            builder.finish()
        };

        for (tag, source, mem_width) in [
            (0, SrcOperand::Reg(x86(X86Reg::Rax)), MemWidth::B1),
            (1, SrcOperand::Reg(x86(X86Reg::Rsp)), MemWidth::B2),
            (2, SrcOperand::Reg(x86(X86Reg::Rbp)), MemWidth::B4),
            (3, SrcOperand::Reg(x86(X86Reg::R16)), MemWidth::B8),
            (4, SrcOperand::Imm(0x7F), MemWidth::B1),
            (5, SrcOperand::Imm(-0x1234), MemWidth::B8),
            (6, SrcOperand::Reg(x86(X86Reg::R15)), MemWidth::B4),
        ] {
            let function = build(
                tag,
                source.clone(),
                mem_width,
                FlagUpdate::None,
                FlagUpdate::All,
                tag,
                address(),
                0x1000,
                false,
            );
            assert!(
                is_native_clobber_safe_excluding(
                    &function,
                    &std::collections::HashMap::new(),
                    true,
                ),
                "tag={tag} source={source:?} width={mem_width:?}: {function:?}"
            );
            assert!(!is_native_clobber_safe_excluding(
                &function,
                &std::collections::HashMap::new(),
                false,
            ));
        }

        let invalid = [
            build(
                0,
                SrcOperand::Reg(x86(X86Reg::Rax)),
                MemWidth::B8,
                FlagUpdate::All,
                FlagUpdate::All,
                0,
                address(),
                0x1000,
                false,
            ),
            build(
                0,
                SrcOperand::Reg(x86(X86Reg::Rax)),
                MemWidth::B8,
                FlagUpdate::None,
                FlagUpdate::None,
                0,
                address(),
                0x1000,
                false,
            ),
            build(
                0,
                SrcOperand::Reg(x86(X86Reg::Rax)),
                MemWidth::B8,
                FlagUpdate::None,
                FlagUpdate::All,
                1,
                address(),
                0x1000,
                false,
            ),
            build(
                0,
                SrcOperand::Reg(x86(X86Reg::Rax)),
                MemWidth::B8,
                FlagUpdate::None,
                FlagUpdate::All,
                0,
                Address::Absolute(0x2000),
                0x1000,
                false,
            ),
            build(
                0,
                SrcOperand::Reg(x86(X86Reg::Rax)),
                MemWidth::B8,
                FlagUpdate::None,
                FlagUpdate::All,
                0,
                address(),
                0x1001,
                false,
            ),
            build(
                0,
                SrcOperand::Reg(x86(X86Reg::Rax)),
                MemWidth::B8,
                FlagUpdate::None,
                FlagUpdate::All,
                0,
                address(),
                0x1000,
                true,
            ),
        ];
        for function in invalid {
            assert!(!is_native_clobber_safe_excluding(
                &function,
                &std::collections::HashMap::new(),
                true,
            ));
        }

        let mut signed = build(
            0,
            SrcOperand::Imm(1),
            MemWidth::B8,
            FlagUpdate::None,
            FlagUpdate::All,
            0,
            address(),
            0x1000,
            false,
        );
        let OpKind::Load { sign, .. } = &mut signed.blocks[0].ops[0].kind else {
            unreachable!()
        };
        *sign = SignExtend::Sign;
        assert!(!is_native_clobber_safe_excluding(
            &signed,
            &std::collections::HashMap::new(),
            true,
        ));
    }

    #[test]
    fn x86_scalar_memory_destination_unary_gate_accepts_exact_replay_and_fails_closed() {
        let old = VReg::Virtual(VirtualId(40));
        let result = VReg::Virtual(VirtualId(41));
        let flags_result = VReg::Virtual(VirtualId(42));
        let address = || Address::BaseIndexScale {
            base: Some(x86(X86Reg::Rbp)),
            index: x86(X86Reg::R17),
            scale: 4,
            disp: 16,
            disp_size: DispSize::Disp8,
        };
        let unary = |tag: u8, dst: VReg, src: VReg, width: OpWidth, flags: FlagUpdate| match tag {
            0 => OpKind::Neg {
                dst,
                src,
                width,
                flags,
            },
            1 => OpKind::Inc {
                dst,
                src,
                width,
                flags,
            },
            2 => OpKind::Dec {
                dst,
                src,
                width,
                flags,
            },
            _ => unreachable!(),
        };
        let build_flagged = |tag: u8,
                             replay_tag: u8,
                             mem_width: MemWidth,
                             compute_flags: FlagUpdate,
                             replay_flags: FlagUpdate,
                             store_addr: Address,
                             replay_pc: u64,
                             extra_old_use: bool| {
            let width = mem_width.to_op_width().unwrap();
            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            builder.push_op(
                0x1000,
                OpKind::Load {
                    dst: old,
                    addr: address(),
                    width: mem_width,
                    sign: SignExtend::Zero,
                },
            );
            builder.push_op(0x1000, unary(tag, result, old, width, compute_flags));
            builder.push_op(
                0x1000,
                OpKind::Store {
                    src: result,
                    addr: store_addr,
                    width: mem_width,
                },
            );
            builder.push_op(
                replay_pc,
                unary(replay_tag, flags_result, old, width, replay_flags),
            );
            if extra_old_use {
                builder.push_op(
                    0x1001,
                    OpKind::Mov {
                        dst: x86(X86Reg::R11),
                        src: SrcOperand::Reg(old),
                        width,
                    },
                );
            }
            builder.set_terminator(Terminator::Return { values: vec![] });
            builder.finish()
        };

        for (tag, mem_width) in [(0, MemWidth::B1), (1, MemWidth::B4), (2, MemWidth::B8)] {
            let function = build_flagged(
                tag,
                tag,
                mem_width,
                FlagUpdate::None,
                FlagUpdate::All,
                address(),
                0x1000,
                false,
            );
            assert!(is_native_clobber_safe_excluding(
                &function,
                &std::collections::HashMap::new(),
                true,
            ));
            assert!(!is_native_clobber_safe_excluding(
                &function,
                &std::collections::HashMap::new(),
                false,
            ));
        }

        let mut not_builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        not_builder.push_op(
            0x1000,
            OpKind::Load {
                dst: old,
                addr: address(),
                width: MemWidth::B2,
                sign: SignExtend::Zero,
            },
        );
        not_builder.push_op(
            0x1000,
            OpKind::Not {
                dst: old,
                src: old,
                width: OpWidth::W16,
            },
        );
        not_builder.push_op(
            0x1000,
            OpKind::Store {
                src: old,
                addr: address(),
                width: MemWidth::B2,
            },
        );
        not_builder.set_terminator(Terminator::Return { values: vec![] });
        let not_function = not_builder.finish();
        assert!(is_native_clobber_safe_excluding(
            &not_function,
            &std::collections::HashMap::new(),
            true,
        ));

        for function in [
            build_flagged(
                0,
                0,
                MemWidth::B8,
                FlagUpdate::All,
                FlagUpdate::All,
                address(),
                0x1000,
                false,
            ),
            build_flagged(
                1,
                1,
                MemWidth::B8,
                FlagUpdate::None,
                FlagUpdate::None,
                address(),
                0x1000,
                false,
            ),
            build_flagged(
                2,
                0,
                MemWidth::B8,
                FlagUpdate::None,
                FlagUpdate::All,
                address(),
                0x1000,
                false,
            ),
            build_flagged(
                0,
                0,
                MemWidth::B8,
                FlagUpdate::None,
                FlagUpdate::All,
                Address::Absolute(0x2000),
                0x1000,
                false,
            ),
            build_flagged(
                0,
                0,
                MemWidth::B8,
                FlagUpdate::None,
                FlagUpdate::All,
                address(),
                0x1001,
                false,
            ),
            build_flagged(
                0,
                0,
                MemWidth::B8,
                FlagUpdate::None,
                FlagUpdate::All,
                address(),
                0x1000,
                true,
            ),
        ] {
            assert!(!is_native_clobber_safe_excluding(
                &function,
                &std::collections::HashMap::new(),
                true,
            ));
        }

        let mut malformed_not = not_function;
        let OpKind::Store { addr, .. } = &mut malformed_not.blocks[0].ops[2].kind else {
            unreachable!()
        };
        *addr = Address::Absolute(0x2000);
        assert!(!is_native_clobber_safe_excluding(
            &malformed_not,
            &std::collections::HashMap::new(),
            true,
        ));

        let mut signed = build_flagged(
            0,
            0,
            MemWidth::B8,
            FlagUpdate::None,
            FlagUpdate::All,
            address(),
            0x1000,
            false,
        );
        let OpKind::Load { sign, .. } = &mut signed.blocks[0].ops[0].kind else {
            unreachable!()
        };
        *sign = SignExtend::Sign;
        assert!(!is_native_clobber_safe_excluding(
            &signed,
            &std::collections::HashMap::new(),
            true,
        ));
    }

    #[test]
    fn x86_scalar_memory_destination_shift_gate_accepts_exact_replay_and_fails_closed() {
        let old = VReg::Virtual(VirtualId(50));
        let result = VReg::Virtual(VirtualId(51));
        let flags_result = VReg::Virtual(VirtualId(52));
        let address = || Address::BaseIndexScale {
            base: Some(x86(X86Reg::Rbx)),
            index: x86(X86Reg::R18),
            scale: 8,
            disp: -32,
            disp_size: DispSize::Disp8,
        };
        let shift = |tag: u8,
                     dst: VReg,
                     src: VReg,
                     amount: SrcOperand,
                     width: OpWidth,
                     flags: FlagUpdate| match tag {
            0 => OpKind::Rol {
                dst,
                src,
                amount,
                width,
                flags,
            },
            1 => OpKind::Ror {
                dst,
                src,
                amount,
                width,
                flags,
            },
            2 => OpKind::Rcl {
                dst,
                src,
                amount,
                width,
                flags,
            },
            3 => OpKind::Rcr {
                dst,
                src,
                amount,
                width,
                flags,
            },
            4 => OpKind::Shl {
                dst,
                src,
                amount,
                width,
                flags,
            },
            5 => OpKind::Shr {
                dst,
                src,
                amount,
                width,
                flags,
            },
            7 => OpKind::Sar {
                dst,
                src,
                amount,
                width,
                flags,
            },
            _ => unreachable!(),
        };
        let rotate_flags = FlagUpdate::Specific(FlagSet::CF.union(FlagSet::OF));
        let build = |tag: u8,
                     replay_tag: u8,
                     amount: SrcOperand,
                     replay_amount: SrcOperand,
                     mem_width: MemWidth,
                     compute_flags: FlagUpdate,
                     replay_flags: FlagUpdate,
                     store_addr: Address,
                     replay_pc: u64,
                     extra_old_use: bool| {
            let width = mem_width.to_op_width().unwrap();
            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            builder.push_op(
                0x1000,
                OpKind::Load {
                    dst: old,
                    addr: address(),
                    width: mem_width,
                    sign: SignExtend::Zero,
                },
            );
            builder.push_op(
                0x1000,
                shift(tag, result, old, amount, width, compute_flags),
            );
            builder.push_op(
                0x1000,
                OpKind::Store {
                    src: result,
                    addr: store_addr,
                    width: mem_width,
                },
            );
            builder.push_op(
                replay_pc,
                shift(
                    replay_tag,
                    flags_result,
                    old,
                    replay_amount,
                    width,
                    replay_flags,
                ),
            );
            if extra_old_use {
                builder.push_op(
                    0x1001,
                    OpKind::Mov {
                        dst: x86(X86Reg::R11),
                        src: SrcOperand::Reg(old),
                        width,
                    },
                );
            }
            builder.set_terminator(Terminator::Return { values: vec![] });
            builder.finish()
        };

        for (tag, amount, mem_width) in [
            (0, SrcOperand::Imm(7), MemWidth::B1),
            (1, SrcOperand::Imm(7), MemWidth::B2),
            (0, SrcOperand::Imm(31), MemWidth::B1),
            (1, SrcOperand::Imm(9), MemWidth::B1),
            (2, SrcOperand::Imm(1), MemWidth::B4),
            (3, SrcOperand::Imm(1), MemWidth::B8),
            (2, SrcOperand::Imm(32), MemWidth::B4),
            (3, SrcOperand::Imm(64), MemWidth::B8),
            (2, SrcOperand::Imm(7), MemWidth::B4),
            (3, SrcOperand::Imm(7), MemWidth::B8),
            (4, SrcOperand::Imm(0), MemWidth::B1),
            (5, SrcOperand::Imm(1), MemWidth::B4),
            (7, SrcOperand::Imm(255), MemWidth::B8),
            (0, SrcOperand::Reg(x86(X86Reg::Rcx)), MemWidth::B1),
            (1, SrcOperand::Reg(x86(X86Reg::Rcx)), MemWidth::B1),
            (0, SrcOperand::Reg(x86(X86Reg::Rcx)), MemWidth::B4),
            (1, SrcOperand::Reg(x86(X86Reg::Rcx)), MemWidth::B8),
            (2, SrcOperand::Reg(x86(X86Reg::Rcx)), MemWidth::B4),
            (3, SrcOperand::Reg(x86(X86Reg::Rcx)), MemWidth::B8),
            (4, SrcOperand::Reg(x86(X86Reg::Rcx)), MemWidth::B4),
            (5, SrcOperand::Reg(x86(X86Reg::Rcx)), MemWidth::B8),
            (7, SrcOperand::Reg(x86(X86Reg::Rcx)), MemWidth::B4),
        ] {
            let replay_flags = if tag <= 3 {
                rotate_flags
            } else {
                FlagUpdate::All
            };
            let function = build(
                tag,
                tag,
                amount.clone(),
                amount,
                mem_width,
                FlagUpdate::None,
                replay_flags,
                address(),
                0x1000,
                false,
            );
            assert!(is_native_clobber_safe_excluding(
                &function,
                &std::collections::HashMap::new(),
                true,
            ));
            assert!(!is_native_clobber_safe_excluding(
                &function,
                &std::collections::HashMap::new(),
                false,
            ));
        }

        for tag in 0..=3 {
            for mem_width in [MemWidth::B1, MemWidth::B2] {
                for amount in [
                    SrcOperand::Imm(0),
                    SrcOperand::Imm(1),
                    SrcOperand::Imm(9),
                    SrcOperand::Imm(17),
                    SrcOperand::Imm(31),
                    SrcOperand::Reg(x86(X86Reg::Rcx)),
                ] {
                    let function = build(
                        tag,
                        tag,
                        amount.clone(),
                        amount,
                        mem_width,
                        FlagUpdate::None,
                        rotate_flags,
                        address(),
                        0x1000,
                        false,
                    );
                    assert!(
                        is_native_clobber_safe_excluding(
                            &function,
                            &std::collections::HashMap::new(),
                            true,
                        ),
                        "subword rotate tag {tag} with {mem_width:?} must JIT"
                    );
                }
            }
        }

        for mem_width in [MemWidth::B1, MemWidth::B2] {
            for amount in [
                SrcOperand::Imm(0),
                SrcOperand::Imm(1),
                SrcOperand::Imm(8),
                SrcOperand::Imm(16),
                SrcOperand::Imm(31),
                SrcOperand::Reg(x86(X86Reg::Rcx)),
            ] {
                let function = build(
                    7,
                    7,
                    amount.clone(),
                    amount,
                    mem_width,
                    FlagUpdate::None,
                    FlagUpdate::All,
                    address(),
                    0x1000,
                    false,
                );
                assert!(
                    is_native_clobber_safe_excluding(
                        &function,
                        &std::collections::HashMap::new(),
                        true,
                    ),
                    "subword SAR with {mem_width:?} must JIT"
                );
            }
        }

        for tag in [4, 5] {
            for (mem_width, boundary) in [(MemWidth::B1, 8), (MemWidth::B2, 16)] {
                for amount in [
                    SrcOperand::Imm(0),
                    SrcOperand::Imm(1),
                    SrcOperand::Imm(boundary - 1),
                    SrcOperand::Imm(boundary),
                    SrcOperand::Imm(boundary + 1),
                    SrcOperand::Imm(31),
                    SrcOperand::Reg(x86(X86Reg::Rcx)),
                ] {
                    let function = build(
                        tag,
                        tag,
                        amount.clone(),
                        amount,
                        mem_width,
                        FlagUpdate::None,
                        FlagUpdate::All,
                        address(),
                        0x1000,
                        false,
                    );
                    assert!(
                        is_native_clobber_safe_excluding(
                            &function,
                            &std::collections::HashMap::new(),
                            true,
                        ),
                        "subword shift tag {tag} with {mem_width:?} must JIT"
                    );
                }
            }
        }

        for function in [
            {
                let mut function = build(
                    4,
                    4,
                    SrcOperand::Imm(1),
                    SrcOperand::Imm(1),
                    MemWidth::B8,
                    FlagUpdate::None,
                    FlagUpdate::All,
                    address(),
                    0x1000,
                    false,
                );
                function.blocks[0].ops[1].x86_hint = Some(X86OpHint::ShiftGroup6);
                function.blocks[0].ops[3].x86_hint = Some(X86OpHint::ShiftGroup6);
                function
            },
            build(
                0,
                0,
                SrcOperand::Imm(1),
                SrcOperand::Imm(1),
                MemWidth::B8,
                FlagUpdate::None,
                FlagUpdate::All,
                address(),
                0x1000,
                false,
            ),
            build(
                4,
                4,
                SrcOperand::Imm(1),
                SrcOperand::Imm(1),
                MemWidth::B8,
                FlagUpdate::All,
                FlagUpdate::All,
                address(),
                0x1000,
                false,
            ),
            build(
                5,
                7,
                SrcOperand::Imm(1),
                SrcOperand::Imm(1),
                MemWidth::B8,
                FlagUpdate::None,
                FlagUpdate::All,
                address(),
                0x1000,
                false,
            ),
            build(
                7,
                7,
                SrcOperand::Reg(x86(X86Reg::Rdx)),
                SrcOperand::Reg(x86(X86Reg::Rdx)),
                MemWidth::B8,
                FlagUpdate::None,
                FlagUpdate::All,
                address(),
                0x1000,
                false,
            ),
            build(
                4,
                4,
                SrcOperand::Imm(256),
                SrcOperand::Imm(256),
                MemWidth::B8,
                FlagUpdate::None,
                FlagUpdate::All,
                address(),
                0x1000,
                false,
            ),
            build(
                4,
                4,
                SrcOperand::Imm(1),
                SrcOperand::Imm(2),
                MemWidth::B8,
                FlagUpdate::None,
                FlagUpdate::All,
                address(),
                0x1000,
                false,
            ),
            build(
                4,
                4,
                SrcOperand::Imm(1),
                SrcOperand::Imm(1),
                MemWidth::B8,
                FlagUpdate::None,
                FlagUpdate::All,
                Address::Absolute(0x2000),
                0x1000,
                false,
            ),
            build(
                4,
                4,
                SrcOperand::Imm(1),
                SrcOperand::Imm(1),
                MemWidth::B8,
                FlagUpdate::None,
                FlagUpdate::All,
                address(),
                0x1001,
                false,
            ),
            build(
                4,
                4,
                SrcOperand::Imm(1),
                SrcOperand::Imm(1),
                MemWidth::B8,
                FlagUpdate::None,
                FlagUpdate::All,
                address(),
                0x1000,
                true,
            ),
        ] {
            assert!(!is_native_clobber_safe_excluding(
                &function,
                &std::collections::HashMap::new(),
                true,
            ));
        }
    }

    #[test]
    fn x86_scalar_memory_source_gate_accepts_exact_ssa_pairs_and_fails_closed() {
        let temporary = VReg::Virtual(VirtualId(17));
        let build = |consumer: OpKind,
                     mem_width: MemWidth,
                     sign: SignExtend,
                     consumer_pc: u64,
                     extra_use: bool,
                     addr: Address| {
            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            builder.push_op(
                0x1000,
                OpKind::Load {
                    dst: temporary,
                    addr,
                    width: mem_width,
                    sign,
                },
            );
            builder.push_op(consumer_pc, consumer);
            if extra_use {
                builder.push_op(
                    0x1001,
                    OpKind::Mov {
                        dst: x86(X86Reg::R11),
                        src: SrcOperand::Reg(temporary),
                        width: OpWidth::W64,
                    },
                );
            }
            builder.set_terminator(Terminator::Return { values: vec![] });
            builder.finish()
        };
        let address = || Address::BaseIndexScale {
            base: Some(x86(X86Reg::Rsp)),
            index: x86(X86Reg::R16),
            scale: 2,
            disp: -8,
            disp_size: DispSize::Disp8,
        };

        let valid = [
            (
                OpKind::Add {
                    dst: x86(X86Reg::R8),
                    src1: x86(X86Reg::R9),
                    src2: SrcOperand::Reg(temporary),
                    width: OpWidth::W8,
                    flags: FlagUpdate::All,
                },
                MemWidth::B1,
            ),
            (
                OpKind::Sub {
                    dst: x86(X86Reg::R10),
                    src1: temporary,
                    src2: SrcOperand::Reg(x86(X86Reg::R10)),
                    width: OpWidth::W16,
                    flags: FlagUpdate::None,
                },
                MemWidth::B2,
            ),
            (
                OpKind::Or {
                    dst: x86(X86Reg::Rax),
                    src1: temporary,
                    src2: SrcOperand::Imm(0x1234),
                    width: OpWidth::W32,
                    flags: FlagUpdate::All,
                },
                MemWidth::B4,
            ),
            (
                OpKind::Cmp {
                    src1: x86(X86Reg::Rbx),
                    src2: SrcOperand::Reg(temporary),
                    width: OpWidth::W64,
                },
                MemWidth::B8,
            ),
            (
                OpKind::Cmp {
                    src1: temporary,
                    src2: SrcOperand::Reg(x86(X86Reg::Rdi)),
                    width: OpWidth::W32,
                },
                MemWidth::B4,
            ),
            (
                OpKind::Cmp {
                    src1: temporary,
                    src2: SrcOperand::Imm(-1),
                    width: OpWidth::W64,
                },
                MemWidth::B8,
            ),
            (
                OpKind::Test {
                    src1: temporary,
                    src2: SrcOperand::Reg(x86(X86Reg::Rsi)),
                    width: OpWidth::W16,
                },
                MemWidth::B2,
            ),
            (
                OpKind::Test {
                    src1: temporary,
                    src2: SrcOperand::Imm(0x7F),
                    width: OpWidth::W8,
                },
                MemWidth::B1,
            ),
            (
                OpKind::MulS {
                    dst_lo: x86(X86Reg::Rax),
                    dst_hi: None,
                    src1: x86(X86Reg::Rax),
                    src2: SrcOperand::Reg(temporary),
                    width: OpWidth::W16,
                    flags: FlagUpdate::All,
                },
                MemWidth::B2,
            ),
            (
                OpKind::MulS {
                    dst_lo: x86(X86Reg::R8),
                    dst_hi: None,
                    src1: x86(X86Reg::R8),
                    src2: SrcOperand::Reg(temporary),
                    width: OpWidth::W32,
                    flags: FlagUpdate::All,
                },
                MemWidth::B4,
            ),
            (
                OpKind::MulS {
                    dst_lo: x86(X86Reg::R15),
                    dst_hi: None,
                    src1: x86(X86Reg::R15),
                    src2: SrcOperand::Reg(temporary),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
                MemWidth::B8,
            ),
            (
                OpKind::X86Count {
                    dst: x86(X86Reg::R8),
                    src: temporary,
                    width: OpWidth::W16,
                    kind: X86CountKind::Popcnt,
                    flags: FlagUpdate::All,
                },
                MemWidth::B2,
            ),
            (
                OpKind::X86Count {
                    dst: x86(X86Reg::R9),
                    src: temporary,
                    width: OpWidth::W32,
                    kind: X86CountKind::Tzcnt,
                    flags: FlagUpdate::Specific(FlagSet::CF.union(FlagSet::ZF)),
                },
                MemWidth::B4,
            ),
            (
                OpKind::X86Count {
                    dst: x86(X86Reg::R15),
                    src: temporary,
                    width: OpWidth::W64,
                    kind: X86CountKind::Lzcnt,
                    flags: FlagUpdate::None,
                },
                MemWidth::B8,
            ),
            (
                OpKind::Bsf {
                    dst: x86(X86Reg::R8),
                    src: temporary,
                    width: OpWidth::W16,
                    flags: FlagUpdate::Specific(FlagSet::ZF),
                },
                MemWidth::B2,
            ),
            (
                OpKind::Bsr {
                    dst: x86(X86Reg::R15),
                    src: temporary,
                    width: OpWidth::W64,
                    flags: FlagUpdate::Specific(FlagSet::ZF),
                },
                MemWidth::B8,
            ),
            (
                OpKind::Bt {
                    src: temporary,
                    index: SrcOperand::Imm(15),
                    width: OpWidth::W16,
                },
                MemWidth::B2,
            ),
        ];
        for (consumer, mem_width) in valid {
            let function = build(
                consumer,
                mem_width,
                SignExtend::Zero,
                0x1000,
                false,
                address(),
            );
            assert!(is_native_clobber_safe_excluding(
                &function,
                &std::collections::HashMap::new(),
                true,
            ));
            assert!(!is_native_clobber_safe_excluding(
                &function,
                &std::collections::HashMap::new(),
                false,
            ));
        }

        let binary = |dst, src1, flags| OpKind::Add {
            dst,
            src1,
            src2: SrcOperand::Reg(temporary),
            width: OpWidth::W64,
            flags,
        };
        let base = binary(x86(X86Reg::R8), x86(X86Reg::R9), FlagUpdate::All);
        let invalid = [
            build(
                base.clone(),
                MemWidth::B8,
                SignExtend::Sign,
                0x1000,
                false,
                address(),
            ),
            build(
                base.clone(),
                MemWidth::B8,
                SignExtend::Zero,
                0x1001,
                false,
                address(),
            ),
            build(
                base.clone(),
                MemWidth::B4,
                SignExtend::Zero,
                0x1000,
                false,
                address(),
            ),
            build(
                base.clone(),
                MemWidth::B8,
                SignExtend::Zero,
                0x1000,
                true,
                address(),
            ),
            build(
                binary(x86(X86Reg::R16), x86(X86Reg::R9), FlagUpdate::All),
                MemWidth::B8,
                SignExtend::Zero,
                0x1000,
                false,
                address(),
            ),
            build(
                binary(x86(X86Reg::R8), x86(X86Reg::Rsp), FlagUpdate::All),
                MemWidth::B8,
                SignExtend::Zero,
                0x1000,
                false,
                address(),
            ),
            build(
                binary(
                    x86(X86Reg::R8),
                    x86(X86Reg::R9),
                    FlagUpdate::Specific(FlagSet::ZF),
                ),
                MemWidth::B8,
                SignExtend::Zero,
                0x1000,
                false,
                address(),
            ),
            build(
                base,
                MemWidth::B8,
                SignExtend::Zero,
                0x1000,
                false,
                Address::Direct(VReg::Virtual(VirtualId(99))),
            ),
            build(
                OpKind::MulS {
                    dst_lo: x86(X86Reg::R8),
                    dst_hi: None,
                    src1: x86(X86Reg::R9),
                    src2: SrcOperand::Reg(temporary),
                    width: OpWidth::W64,
                    flags: FlagUpdate::All,
                },
                MemWidth::B8,
                SignExtend::Zero,
                0x1000,
                false,
                address(),
            ),
            build(
                OpKind::MulS {
                    dst_lo: x86(X86Reg::Rax),
                    dst_hi: Some(x86(X86Reg::Rdx)),
                    src1: x86(X86Reg::Rax),
                    src2: SrcOperand::Reg(temporary),
                    width: OpWidth::W64,
                    flags: FlagUpdate::All,
                },
                MemWidth::B8,
                SignExtend::Zero,
                0x1000,
                false,
                address(),
            ),
            build(
                OpKind::MulS {
                    dst_lo: x86(X86Reg::R8),
                    dst_hi: None,
                    src1: x86(X86Reg::R8),
                    src2: SrcOperand::Reg(temporary),
                    width: OpWidth::W8,
                    flags: FlagUpdate::All,
                },
                MemWidth::B1,
                SignExtend::Zero,
                0x1000,
                false,
                address(),
            ),
            build(
                OpKind::MulS {
                    dst_lo: x86(X86Reg::Rsp),
                    dst_hi: None,
                    src1: x86(X86Reg::Rsp),
                    src2: SrcOperand::Reg(temporary),
                    width: OpWidth::W64,
                    flags: FlagUpdate::All,
                },
                MemWidth::B8,
                SignExtend::Zero,
                0x1000,
                false,
                address(),
            ),
            build(
                OpKind::MulS {
                    dst_lo: x86(X86Reg::R8),
                    dst_hi: None,
                    src1: x86(X86Reg::R8),
                    src2: SrcOperand::Reg(temporary),
                    width: OpWidth::W64,
                    flags: FlagUpdate::Specific(FlagSet::CF.union(FlagSet::OF)),
                },
                MemWidth::B8,
                SignExtend::Zero,
                0x1000,
                false,
                address(),
            ),
            build(
                OpKind::X86Count {
                    dst: x86(X86Reg::R8),
                    src: temporary,
                    width: OpWidth::W64,
                    kind: X86CountKind::Tzcnt,
                    flags: FlagUpdate::All,
                },
                MemWidth::B8,
                SignExtend::Zero,
                0x1000,
                false,
                address(),
            ),
            build(
                OpKind::X86Count {
                    dst: x86(X86Reg::R16),
                    src: temporary,
                    width: OpWidth::W64,
                    kind: X86CountKind::Popcnt,
                    flags: FlagUpdate::All,
                },
                MemWidth::B8,
                SignExtend::Zero,
                0x1000,
                false,
                address(),
            ),
            build(
                OpKind::X86Count {
                    dst: x86(X86Reg::R8),
                    src: temporary,
                    width: OpWidth::W32,
                    kind: X86CountKind::Lzcnt,
                    flags: FlagUpdate::Specific(FlagSet::CF.union(FlagSet::ZF)),
                },
                MemWidth::B8,
                SignExtend::Zero,
                0x1000,
                false,
                address(),
            ),
            build(
                OpKind::Bsf {
                    dst: x86(X86Reg::R8),
                    src: temporary,
                    width: OpWidth::W64,
                    flags: FlagUpdate::All,
                },
                MemWidth::B8,
                SignExtend::Zero,
                0x1000,
                false,
                address(),
            ),
            build(
                OpKind::Bsr {
                    dst: x86(X86Reg::R16),
                    src: temporary,
                    width: OpWidth::W64,
                    flags: FlagUpdate::Specific(FlagSet::ZF),
                },
                MemWidth::B8,
                SignExtend::Zero,
                0x1000,
                false,
                address(),
            ),
            build(
                OpKind::Bsf {
                    dst: x86(X86Reg::R8),
                    src: temporary,
                    width: OpWidth::W32,
                    flags: FlagUpdate::Specific(FlagSet::ZF),
                },
                MemWidth::B8,
                SignExtend::Zero,
                0x1000,
                false,
                address(),
            ),
            build(
                OpKind::Bt {
                    src: temporary,
                    index: SrcOperand::Reg(x86(X86Reg::Rcx)),
                    width: OpWidth::W64,
                },
                MemWidth::B8,
                SignExtend::Zero,
                0x1000,
                false,
                address(),
            ),
            build(
                OpKind::Bt {
                    src: temporary,
                    index: SrcOperand::Imm(7),
                    width: OpWidth::W32,
                },
                MemWidth::B8,
                SignExtend::Zero,
                0x1000,
                false,
                address(),
            ),
            build(
                OpKind::Bt {
                    src: temporary,
                    index: SrcOperand::Imm(64),
                    width: OpWidth::W64,
                },
                MemWidth::B8,
                SignExtend::Zero,
                0x1000,
                false,
                address(),
            ),
        ];
        for (case, function) in invalid.into_iter().enumerate() {
            assert!(
                !is_native_clobber_safe_excluding(
                    &function,
                    &std::collections::HashMap::new(),
                    true,
                ),
                "invalid scalar memory-source case {case}: {function:?}"
            );
        }

        let mut hinted_imul = build(
            OpKind::MulS {
                dst_lo: x86(X86Reg::R8),
                dst_hi: None,
                src1: x86(X86Reg::R8),
                src2: SrcOperand::Reg(temporary),
                width: OpWidth::W64,
                flags: FlagUpdate::All,
            },
            MemWidth::B8,
            SignExtend::Zero,
            0x1000,
            false,
            address(),
        );
        hinted_imul.blocks[0].ops[1].x86_hint = Some(X86OpHint::ImulImm8);
        assert!(!is_native_clobber_safe_excluding(
            &hinted_imul,
            &std::collections::HashMap::new(),
            true,
        ));
    }

    #[test]
    fn x86_immediate_memory_imul_gate_enforces_width_hint_and_signed_range() {
        let temporary = VReg::Virtual(VirtualId(18));
        let build = |dst_lo: VReg,
                     dst_hi: Option<VReg>,
                     src1: VReg,
                     value: i64,
                     width: OpWidth,
                     mem_width: MemWidth,
                     flags: FlagUpdate,
                     hint: Option<X86OpHint>| {
            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            builder.push_op(
                0x1000,
                OpKind::Load {
                    dst: temporary,
                    addr: Address::Direct(x86(X86Reg::Rbx)),
                    width: mem_width,
                    sign: SignExtend::Zero,
                },
            );
            builder.push_op(
                0x1000,
                OpKind::MulS {
                    dst_lo,
                    dst_hi,
                    src1,
                    src2: SrcOperand::Imm(value),
                    width,
                    flags,
                },
            );
            builder.set_terminator(Terminator::Return { values: vec![] });
            let mut function = builder.finish();
            function.blocks[0].ops[1].x86_hint = hint;
            function
        };

        for (name, dst, value, width, mem_width, flags, hint) in [
            (
                "W16 imm8 lower bound",
                x86(X86Reg::Rax),
                i64::from(i8::MIN),
                OpWidth::W16,
                MemWidth::B2,
                FlagUpdate::All,
                X86OpHint::ImulImm8,
            ),
            (
                "W16 imm16",
                x86(X86Reg::R8),
                0x1234,
                OpWidth::W16,
                MemWidth::B2,
                FlagUpdate::All,
                X86OpHint::ImulImm32,
            ),
            (
                "W32 NF imm8 upper bound",
                x86(X86Reg::R9),
                i64::from(i8::MAX),
                OpWidth::W32,
                MemWidth::B4,
                FlagUpdate::None,
                X86OpHint::ImulImm8,
            ),
            (
                "W32 imm32 lower bound",
                x86(X86Reg::R10),
                i64::from(i32::MIN),
                OpWidth::W32,
                MemWidth::B4,
                FlagUpdate::All,
                X86OpHint::ImulImm32,
            ),
            (
                "W64 NF imm32 upper bound",
                x86(X86Reg::R15),
                i64::from(i32::MAX),
                OpWidth::W64,
                MemWidth::B8,
                FlagUpdate::None,
                X86OpHint::ImulImm32,
            ),
        ] {
            let function = build(
                dst,
                None,
                temporary,
                value,
                width,
                mem_width,
                flags,
                Some(hint),
            );
            assert!(
                is_native_clobber_safe_excluding(
                    &function,
                    &std::collections::HashMap::new(),
                    true,
                ),
                "{name}"
            );
            assert!(
                !is_native_clobber_safe_excluding(
                    &function,
                    &std::collections::HashMap::new(),
                    false,
                ),
                "{name} must still require memory helpers"
            );
        }

        for (name, dst_lo, dst_hi, src1, value, width, mem_width, flags, hint) in [
            (
                "W8 has no two/three-operand IMUL",
                x86(X86Reg::R8),
                None,
                temporary,
                1,
                OpWidth::W8,
                MemWidth::B1,
                FlagUpdate::All,
                Some(X86OpHint::ImulImm8),
            ),
            (
                "imm8 overflow",
                x86(X86Reg::R8),
                None,
                temporary,
                i64::from(i8::MAX) + 1,
                OpWidth::W16,
                MemWidth::B2,
                FlagUpdate::All,
                Some(X86OpHint::ImulImm8),
            ),
            (
                "imm16 overflow",
                x86(X86Reg::R8),
                None,
                temporary,
                i64::from(i16::MAX) + 1,
                OpWidth::W16,
                MemWidth::B2,
                FlagUpdate::All,
                Some(X86OpHint::ImulImm32),
            ),
            (
                "W32 imm32 overflow",
                x86(X86Reg::R8),
                None,
                temporary,
                i64::from(i32::MAX) + 1,
                OpWidth::W32,
                MemWidth::B4,
                FlagUpdate::All,
                Some(X86OpHint::ImulImm32),
            ),
            (
                "W64 imm32 underflow",
                x86(X86Reg::R8),
                None,
                temporary,
                i64::from(i32::MIN) - 1,
                OpWidth::W64,
                MemWidth::B8,
                FlagUpdate::All,
                Some(X86OpHint::ImulImm32),
            ),
            (
                "unhinted",
                x86(X86Reg::R8),
                None,
                temporary,
                7,
                OpWidth::W64,
                MemWidth::B8,
                FlagUpdate::All,
                None,
            ),
            (
                "wrong hint family",
                x86(X86Reg::R8),
                None,
                temporary,
                7,
                OpWidth::W64,
                MemWidth::B8,
                FlagUpdate::All,
                Some(X86OpHint::Mulx),
            ),
            (
                "stack destination",
                x86(X86Reg::Rsp),
                None,
                temporary,
                7,
                OpWidth::W64,
                MemWidth::B8,
                FlagUpdate::All,
                Some(X86OpHint::ImulImm8),
            ),
            (
                "extended destination",
                x86(X86Reg::R16),
                None,
                temporary,
                7,
                OpWidth::W64,
                MemWidth::B8,
                FlagUpdate::All,
                Some(X86OpHint::ImulImm8),
            ),
            (
                "widening destination",
                x86(X86Reg::Rax),
                Some(x86(X86Reg::Rdx)),
                temporary,
                7,
                OpWidth::W64,
                MemWidth::B8,
                FlagUpdate::All,
                Some(X86OpHint::ImulImm8),
            ),
            (
                "architectural first source",
                x86(X86Reg::R8),
                None,
                x86(X86Reg::Rax),
                7,
                OpWidth::W64,
                MemWidth::B8,
                FlagUpdate::All,
                Some(X86OpHint::ImulImm8),
            ),
            (
                "partial flags",
                x86(X86Reg::R8),
                None,
                temporary,
                7,
                OpWidth::W64,
                MemWidth::B8,
                FlagUpdate::Specific(FlagSet::CF.union(FlagSet::OF)),
                Some(X86OpHint::ImulImm8),
            ),
            (
                "memory width mismatch",
                x86(X86Reg::R8),
                None,
                temporary,
                7,
                OpWidth::W64,
                MemWidth::B4,
                FlagUpdate::All,
                Some(X86OpHint::ImulImm8),
            ),
        ] {
            let function = build(dst_lo, dst_hi, src1, value, width, mem_width, flags, hint);
            assert!(
                !is_native_clobber_safe_excluding(
                    &function,
                    &std::collections::HashMap::new(),
                    true,
                ),
                "{name}: {function:?}"
            );
        }
    }

    #[test]
    fn x86_crc32_gate_covers_register_and_single_use_memory_shapes() {
        for width in [OpWidth::W8, OpWidth::W16, OpWidth::W32, OpWidth::W64] {
            let op = OpKind::Crc32C {
                dst: x86(X86Reg::R8),
                crc: x86(X86Reg::R8),
                data: x86(X86Reg::R9),
                data_width: width,
            };
            assert!(op.is_jit_safe(), "CRC32 must be class-whitelisted");
            assert!(x86_gate(op), "{width:?} register CRC32 must JIT");
        }

        for (name, op) in [
            (
                "non-destructive destination",
                OpKind::Crc32C {
                    dst: x86(X86Reg::R8),
                    crc: x86(X86Reg::R9),
                    data: x86(X86Reg::R10),
                    data_width: OpWidth::W64,
                },
            ),
            (
                "stack destination",
                OpKind::Crc32C {
                    dst: x86(X86Reg::Rsp),
                    crc: x86(X86Reg::Rsp),
                    data: x86(X86Reg::R10),
                    data_width: OpWidth::W32,
                },
            ),
            (
                "virtual source",
                OpKind::Crc32C {
                    dst: x86(X86Reg::R8),
                    crc: x86(X86Reg::R8),
                    data: VReg::Virtual(VirtualId(1)),
                    data_width: OpWidth::W16,
                },
            ),
            (
                "invalid width",
                OpKind::Crc32C {
                    dst: x86(X86Reg::R8),
                    crc: x86(X86Reg::R8),
                    data: x86(X86Reg::R9),
                    data_width: OpWidth::W128,
                },
            ),
        ] {
            assert!(op.is_jit_safe(), "{name} remains class-whitelisted");
            assert!(!x86_gate(op), "malformed {name} CRC32 must deopt");
        }

        let memory_crc = |extra_use: bool, signed: SignExtend, crc_width: OpWidth| {
            let temporary = VReg::Virtual(VirtualId(7));
            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            builder.push_op(
                0x1000,
                OpKind::Load {
                    dst: temporary,
                    addr: Address::BaseIndexScale {
                        base: Some(x86(X86Reg::Rsp)),
                        index: x86(X86Reg::R16),
                        scale: 2,
                        disp: 8,
                        disp_size: DispSize::Disp8,
                    },
                    width: MemWidth::B4,
                    sign: signed,
                },
            );
            builder.push_op(
                0x1000,
                OpKind::Crc32C {
                    dst: x86(X86Reg::R10),
                    crc: x86(X86Reg::R10),
                    data: temporary,
                    data_width: crc_width,
                },
            );
            if extra_use {
                builder.push_op(
                    0x1001,
                    OpKind::Mov {
                        dst: x86(X86Reg::R11),
                        src: SrcOperand::Reg(temporary),
                        width: OpWidth::W64,
                    },
                );
            }
            builder.set_terminator(Terminator::Return { values: vec![] });
            builder.finish()
        };

        let valid = memory_crc(false, SignExtend::Zero, OpWidth::W32);
        assert!(is_native_clobber_safe_excluding(
            &valid,
            &std::collections::HashMap::new(),
            true
        ));
        assert!(
            !is_native_clobber_safe_excluding(&valid, &std::collections::HashMap::new(), false),
            "memory CRC32 requires MMU-helper mode"
        );
        for invalid in [
            memory_crc(true, SignExtend::Zero, OpWidth::W32),
            memory_crc(false, SignExtend::Sign, OpWidth::W32),
            memory_crc(false, SignExtend::Zero, OpWidth::W64),
        ] {
            assert!(!is_native_clobber_safe_excluding(
                &invalid,
                &std::collections::HashMap::new(),
                true
            ));
        }

        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(
            0x1000,
            OpKind::Crc32C {
                dst: x86(X86Reg::R8),
                crc: x86(X86Reg::R8),
                data: x86(X86Reg::R9),
                data_width: OpWidth::W64,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let function = builder.finish();
        #[cfg(target_arch = "x86_64")]
        assert_eq!(
            x86_native_scalar_features_supported_excluding(
                &function,
                &std::collections::HashMap::new()
            ),
            std::is_x86_feature_detected!("sse4.2")
        );
        #[cfg(not(target_arch = "x86_64"))]
        assert!(!x86_native_scalar_features_supported_excluding(
            &function,
            &std::collections::HashMap::new()
        ));
    }

    #[test]
    fn x86_random_gate_validates_width_destination_and_host_feature() {
        for seed in [false, true] {
            for width in [OpWidth::W16, OpWidth::W32, OpWidth::W64] {
                let op = OpKind::X86Random {
                    dst: x86(X86Reg::R9),
                    width,
                    seed,
                };
                assert!(op.is_jit_safe());
                assert!(x86_gate(op), "valid X86Random shape must enter the gate");
            }

            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            builder.push_op(
                0x1000,
                OpKind::X86Random {
                    dst: x86(X86Reg::R9),
                    width: OpWidth::W64,
                    seed,
                },
            );
            builder.set_terminator(Terminator::Return { values: vec![] });
            let function = builder.finish();
            #[cfg(target_arch = "x86_64")]
            assert_eq!(
                x86_native_scalar_features_supported_excluding(
                    &function,
                    &std::collections::HashMap::new()
                ),
                if seed {
                    std::is_x86_feature_detected!("rdseed")
                } else {
                    std::is_x86_feature_detected!("rdrand")
                }
            );
            #[cfg(not(target_arch = "x86_64"))]
            assert!(!x86_native_scalar_features_supported_excluding(
                &function,
                &std::collections::HashMap::new()
            ));
        }

        for (dst, width) in [
            (x86(X86Reg::Rsp), OpWidth::W64),
            (x86(X86Reg::Rbp), OpWidth::W32),
            (x86(X86Reg::R16), OpWidth::W16),
            (VReg::Virtual(VirtualId(9)), OpWidth::W64),
            (x86(X86Reg::R9), OpWidth::W8),
        ] {
            let malformed = OpKind::X86Random {
                dst,
                width,
                seed: false,
            };
            assert!(malformed.is_jit_safe());
            assert!(!x86_gate(malformed), "malformed X86Random must deopt");
        }
    }

    #[test]
    fn x86_cldemote_gate_admits_only_the_ignorable_cache_hint() {
        for addr in [
            Address::Direct(x86(X86Reg::Rbx)),
            Address::BaseOffset {
                base: x86(X86Reg::Rsp),
                offset: -64,
                disp_size: DispSize::Disp8,
            },
            Address::SegmentRel {
                segment: x86(X86Reg::FsBase),
                base: Some(x86(X86Reg::Rbp)),
                index: Some(x86(X86Reg::R9)),
                scale: 4,
                disp: 32,
            },
        ] {
            let op = OpKind::X86CacheControl {
                addr,
                kind: X86CacheControlKind::Cldemote,
            };
            assert!(
                !op.is_jit_safe(),
                "cache hints remain outside the ALU whitelist"
            );
            assert!(x86_gate(op), "CLDEMOTE must be admitted without memory JIT");
        }

        let malformed = OpKind::X86CacheControl {
            addr: Address::Direct(VReg::Virtual(VirtualId(12))),
            kind: X86CacheControlKind::Cldemote,
        };
        assert!(!x86_gate(malformed));

        for kind in [
            X86CacheControlKind::Clflush,
            X86CacheControlKind::Clflushopt,
            X86CacheControlKind::Clwb,
        ] {
            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            builder.push_op(
                0x1000,
                OpKind::X86CacheControl {
                    addr: Address::Direct(x86(X86Reg::Rbx)),
                    kind,
                },
            );
            builder.set_terminator(Terminator::Return { values: vec![] });
            let function = builder.finish();
            assert!(!is_native_clobber_safe_excluding(
                &function,
                &std::collections::HashMap::new(),
                true
            ));
        }
    }

    #[test]
    fn x86_alignment_gate_admits_exact_state_backed_shapes_without_memory_mode() {
        for (alignment, addr) in [
            (16, Address::Direct(x86(X86Reg::Rax))),
            (
                32,
                Address::BaseOffset {
                    base: x86(X86Reg::Rsp),
                    offset: i64::MIN,
                    disp_size: DispSize::Disp32,
                },
            ),
            (
                64,
                Address::BaseIndexScale {
                    base: Some(x86(X86Reg::Rbp)),
                    index: x86(X86Reg::R16),
                    scale: 8,
                    disp: -64,
                    disp_size: DispSize::Disp8,
                },
            ),
            (
                16,
                Address::PcRel {
                    offset: -32,
                    disp_size: DispSize::Disp32,
                    base: Some(0x1020),
                },
            ),
            (32, Address::Absolute(0x2000)),
            (
                64,
                Address::SegmentRel {
                    segment: x86(X86Reg::GsBase),
                    base: Some(x86(X86Reg::Rsp)),
                    index: Some(x86(X86Reg::R31)),
                    scale: 4,
                    disp: i64::MAX,
                },
            ),
        ] {
            let op = OpKind::X86CheckAlignment { addr, alignment };
            assert!(!op.is_jit_safe(), "alignment checks are control operations");
            assert!(
                x86_gate(op),
                "validated alignment check must not require MMU-helper mode"
            );
        }

        for malformed in [
            OpKind::X86CheckAlignment {
                addr: Address::Direct(x86(X86Reg::Rax)),
                alignment: 8,
            },
            OpKind::X86CheckAlignment {
                addr: Address::Direct(VReg::Virtual(VirtualId(4))),
                alignment: 16,
            },
            OpKind::X86CheckAlignment {
                addr: Address::BaseIndexScale {
                    base: None,
                    index: x86(X86Reg::Rax),
                    scale: 3,
                    disp: 0,
                    disp_size: DispSize::Auto,
                },
                alignment: 32,
            },
            OpKind::X86CheckAlignment {
                addr: Address::PcRel {
                    offset: 0,
                    disp_size: DispSize::Auto,
                    base: None,
                },
                alignment: 64,
            },
            OpKind::X86CheckAlignment {
                addr: Address::GpRel { offset: 0 },
                alignment: 16,
            },
        ] {
            assert!(!x86_gate(malformed), "malformed alignment check must deopt");
        }
    }

    #[test]
    fn x86_vector_memory_gate_requires_memory_mode_and_exact_architectural_shapes() {
        let gate = |op: OpKind, allow_mem: bool| {
            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            builder.push_op(0x1000, op);
            builder.set_terminator(Terminator::Return { values: vec![] });
            let function = builder.finish();
            is_native_clobber_safe_excluding(
                &function,
                &std::collections::HashMap::new(),
                allow_mem,
            )
        };

        let addresses = [
            Address::Direct(x86(X86Reg::Rsp)),
            Address::BaseIndexScale {
                base: Some(x86(X86Reg::Rbp)),
                index: x86(X86Reg::R16),
                scale: 8,
                disp: -64,
                disp_size: DispSize::Disp8,
            },
            Address::SegmentRel {
                segment: x86(X86Reg::FsBase),
                base: Some(x86(X86Reg::R31)),
                index: None,
                scale: 1,
                disp: i64::MIN,
            },
        ];
        let shapes = [
            (x86(X86Reg::Xmm(0)), VecWidth::V128),
            (x86(X86Reg::Ymm(16)), VecWidth::V256),
            (x86(X86Reg::Zmm(31)), VecWidth::V512),
        ];
        for (index, (vector, width)) in shapes.into_iter().enumerate() {
            let addr = addresses[index].clone();
            for op in [
                OpKind::VLoad {
                    dst: vector,
                    addr: addr.clone(),
                    width,
                },
                OpKind::VStore {
                    src: vector,
                    addr: addr.clone(),
                    width,
                },
            ] {
                assert!(x86_jit_vector_mem_shape_valid(&op));
                assert!(!gate(op.clone(), false));
                assert!(gate(op, true));
            }
        }

        for malformed in [
            OpKind::VLoad {
                dst: x86(X86Reg::Xmm(1)),
                addr: Address::Direct(x86(X86Reg::Rax)),
                width: VecWidth::V256,
            },
            OpKind::VStore {
                src: VReg::Virtual(VirtualId(4)),
                addr: Address::Direct(x86(X86Reg::Rax)),
                width: VecWidth::V128,
            },
            OpKind::VLoad {
                dst: x86(X86Reg::Zmm(32)),
                addr: Address::Direct(x86(X86Reg::Rax)),
                width: VecWidth::V512,
            },
            OpKind::VStore {
                src: x86(X86Reg::Ymm(2)),
                addr: Address::GpRel { offset: 0 },
                width: VecWidth::V256,
            },
        ] {
            assert!(!x86_jit_vector_mem_shape_valid(&malformed));
            assert!(!gate(malformed, true));
        }

        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(
            0x1000,
            OpKind::VLoad {
                dst: x86(X86Reg::Ymm(17)),
                addr: Address::Absolute(0x2000),
                width: VecWidth::V256,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        assert!(uses_x86_native_vectors_excluding(
            &builder.finish(),
            &std::collections::HashMap::new(),
        ));
    }

    #[test]
    fn x86_fence_gate_admits_exact_lfence_mfence_sfence_semantics() {
        for kind in [FenceKind::LoadLoad, FenceKind::Full, FenceKind::StoreStore] {
            let op = OpKind::Fence { kind };
            assert!(op.is_jit_safe(), "{kind:?} must be class-whitelisted");
            assert!(x86_gate(op), "{kind:?} must enter the native tier");
        }
        for kind in [
            FenceKind::LoadStore,
            FenceKind::StoreLoad,
            FenceKind::ISync,
            FenceKind::DSync,
        ] {
            let op = OpKind::Fence { kind };
            assert!(op.is_jit_safe(), "{kind:?} remains class-whitelisted");
            assert!(!x86_gate(op), "non-x86 fence {kind:?} must deopt");
        }
    }

    #[test]
    fn x86_stack_arithmetic_gate_admits_only_exact_state_backed_shapes() {
        for op in [
            OpKind::Add {
                dst: x86(X86Reg::Rsp),
                src1: x86(X86Reg::Rsp),
                src2: SrcOperand::Imm(8),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
            OpKind::Sub {
                dst: x86(X86Reg::R16),
                src1: x86(X86Reg::Rbp),
                src2: SrcOperand::Reg(x86(X86Reg::R31)),
                width: OpWidth::W8,
                flags: FlagUpdate::All,
            },
            OpKind::Add {
                dst: x86(X86Reg::Rax),
                src1: x86(X86Reg::Rcx),
                src2: SrcOperand::Reg(x86(X86Reg::Rsp)),
                width: OpWidth::W32,
                flags: FlagUpdate::All,
            },
        ] {
            assert!(x86_state_backed_stack_alu_valid(&op));
            assert!(x86_gate(op), "validated stack arithmetic must enter JIT");
        }

        for malformed in [
            OpKind::Add {
                dst: x86(X86Reg::Rsp),
                src1: VReg::Virtual(VirtualId(1)),
                src2: SrcOperand::Imm(8),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
            OpKind::Sub {
                dst: x86(X86Reg::Rsp),
                src1: x86(X86Reg::Rsp),
                src2: SrcOperand::Imm64(8),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
            OpKind::Add {
                dst: x86(X86Reg::Rsp),
                src1: x86(X86Reg::Rsp),
                src2: SrcOperand::Imm(i64::from(i32::MAX) + 1),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
            OpKind::Add {
                dst: x86(X86Reg::Rax),
                src1: x86(X86Reg::Rcx),
                src2: SrcOperand::Reg(x86(X86Reg::Rdx)),
                width: OpWidth::W64,
                flags: FlagUpdate::All,
            },
            OpKind::Sub {
                dst: x86(X86Reg::Rbp),
                src1: x86(X86Reg::Rbp),
                src2: SrcOperand::Imm(8),
                width: OpWidth::W128,
                flags: FlagUpdate::None,
            },
            OpKind::Add {
                dst: x86(X86Reg::Rsp),
                src1: x86(X86Reg::Rsp),
                src2: SrcOperand::Imm(8),
                width: OpWidth::W64,
                flags: FlagUpdate::Specific(FlagSet::ZF),
            },
        ] {
            assert!(!x86_state_backed_stack_alu_valid(&malformed));
        }
    }

    #[test]
    fn x86_push_gate_requires_fault_precise_helper_fusion_and_exact_rsp_snapshot() {
        let rsp = x86(X86Reg::Rsp);
        let mut ordinary = FunctionBuilder::new(FunctionId(0), 0x1000);
        ordinary.push_op(
            0x1000,
            OpKind::Sub {
                dst: rsp,
                src1: rsp,
                src2: SrcOperand::Imm(8),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        );
        ordinary.push_op(
            0x1000,
            OpKind::Store {
                src: x86(X86Reg::Rax),
                addr: Address::Direct(rsp),
                width: MemWidth::B8,
            },
        );
        ordinary.set_terminator(Terminator::Return { values: vec![] });
        let ordinary = ordinary.finish();
        assert!(!is_native_clobber_safe_excluding(
            &ordinary,
            &std::collections::HashMap::new(),
            false
        ));
        assert!(is_native_clobber_safe_excluding(
            &ordinary,
            &std::collections::HashMap::new(),
            true
        ));

        let temporary = VReg::Virtual(VirtualId(7));
        let mut push_rsp = FunctionBuilder::new(FunctionId(0), 0x2000);
        push_rsp.push_op(
            0x2000,
            OpKind::Mov {
                dst: temporary,
                src: SrcOperand::Reg(rsp),
                width: OpWidth::W64,
            },
        );
        push_rsp.push_op(
            0x2000,
            OpKind::Sub {
                dst: rsp,
                src1: rsp,
                src2: SrcOperand::Imm(8),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        );
        push_rsp.push_op(
            0x2000,
            OpKind::Store {
                src: temporary,
                addr: Address::Direct(rsp),
                width: MemWidth::B8,
            },
        );
        push_rsp.set_terminator(Terminator::Return { values: vec![] });
        assert!(is_native_clobber_safe_excluding(
            &push_rsp.finish(),
            &std::collections::HashMap::new(),
            true
        ));

        let mut malformed = FunctionBuilder::new(FunctionId(0), 0x3000);
        malformed.push_op(
            0x3000,
            OpKind::Sub {
                dst: rsp,
                src1: rsp,
                src2: SrcOperand::Imm(8),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        );
        malformed.push_op(
            0x3000,
            OpKind::Store {
                src: rsp,
                addr: Address::Direct(rsp),
                width: MemWidth::B8,
            },
        );
        malformed.set_terminator(Terminator::Return { values: vec![] });
        assert!(!is_native_clobber_safe_excluding(
            &malformed.finish(),
            &std::collections::HashMap::new(),
            true
        ));
    }

    #[test]
    fn x86_pop_gate_requires_exact_fault_precise_alias_shapes() {
        let rsp = x86(X86Reg::Rsp);
        let mut ordinary = FunctionBuilder::new(FunctionId(0), 0x1000);
        ordinary.push_op(
            0x1000,
            OpKind::Load {
                dst: x86(X86Reg::Rbp),
                addr: Address::Direct(rsp),
                width: MemWidth::B8,
                sign: SignExtend::Zero,
            },
        );
        ordinary.push_op(
            0x1000,
            OpKind::Add {
                dst: rsp,
                src1: rsp,
                src2: SrcOperand::Imm(8),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        );
        ordinary.set_terminator(Terminator::Return { values: vec![] });
        let ordinary = ordinary.finish();
        assert!(!is_native_clobber_safe_excluding(
            &ordinary,
            &std::collections::HashMap::new(),
            false
        ));
        assert!(is_native_clobber_safe_excluding(
            &ordinary,
            &std::collections::HashMap::new(),
            true
        ));

        let popped = VReg::Virtual(VirtualId(7));
        let mut pop_rsp = FunctionBuilder::new(FunctionId(1), 0x2000);
        pop_rsp.push_op(
            0x2000,
            OpKind::Load {
                dst: popped,
                addr: Address::Direct(rsp),
                width: MemWidth::B8,
                sign: SignExtend::Zero,
            },
        );
        pop_rsp.push_op(
            0x2000,
            OpKind::Mov {
                dst: rsp,
                src: SrcOperand::Reg(popped),
                width: OpWidth::W64,
            },
        );
        pop_rsp.set_terminator(Terminator::Return { values: vec![] });
        assert!(is_native_clobber_safe_excluding(
            &pop_rsp.finish(),
            &std::collections::HashMap::new(),
            true
        ));

        let popped = VReg::Virtual(VirtualId(8));
        let incremented = VReg::Virtual(VirtualId(9));
        let mut pop_sp = FunctionBuilder::new(FunctionId(2), 0x3000);
        pop_sp.push_op(
            0x3000,
            OpKind::Load {
                dst: popped,
                addr: Address::Direct(rsp),
                width: MemWidth::B2,
                sign: SignExtend::Zero,
            },
        );
        pop_sp.push_op(
            0x3000,
            OpKind::Add {
                dst: incremented,
                src1: rsp,
                src2: SrcOperand::Imm(2),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        );
        pop_sp.push_op(
            0x3000,
            OpKind::Mov {
                dst: rsp,
                src: SrcOperand::Reg(incremented),
                width: OpWidth::W64,
            },
        );
        pop_sp.push_op(
            0x3000,
            OpKind::Mov {
                dst: rsp,
                src: SrcOperand::Reg(popped),
                width: OpWidth::W16,
            },
        );
        pop_sp.set_terminator(Terminator::Return { values: vec![] });
        assert!(is_native_clobber_safe_excluding(
            &pop_sp.finish(),
            &std::collections::HashMap::new(),
            true
        ));

        // A same-instruction POP-like pair with the wrong delta must fail
        // closed instead of being admitted as an independent Load and ADD.
        let mut malformed = FunctionBuilder::new(FunctionId(3), 0x4000);
        malformed.push_op(
            0x4000,
            OpKind::Load {
                dst: x86(X86Reg::Rax),
                addr: Address::Direct(rsp),
                width: MemWidth::B8,
                sign: SignExtend::Zero,
            },
        );
        malformed.push_op(
            0x4000,
            OpKind::Add {
                dst: rsp,
                src1: rsp,
                src2: SrcOperand::Imm(2),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        );
        malformed.set_terminator(Terminator::Return { values: vec![] });
        assert!(!is_native_clobber_safe_excluding(
            &malformed.finish(),
            &std::collections::HashMap::new(),
            true
        ));

        // The same two SMIR operations at distinct guest PCs are independent
        // instructions, not a malformed fused POP, and remain individually safe.
        let mut independent = FunctionBuilder::new(FunctionId(4), 0x5000);
        independent.push_op(
            0x5000,
            OpKind::Load {
                dst: x86(X86Reg::Rax),
                addr: Address::Direct(rsp),
                width: MemWidth::B8,
                sign: SignExtend::Zero,
            },
        );
        independent.push_op(
            0x5001,
            OpKind::Add {
                dst: rsp,
                src1: rsp,
                src2: SrcOperand::Imm(2),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        );
        independent.set_terminator(Terminator::Return { values: vec![] });
        assert!(is_native_clobber_safe_excluding(
            &independent.finish(),
            &std::collections::HashMap::new(),
            true
        ));
    }

    #[test]
    fn x86_rdpid_gate_admits_physical_and_state_backed_destinations() {
        let valid = OpKind::X86ReadPid {
            dst: x86(X86Reg::R9),
        };
        assert!(valid.is_jit_safe(), "RDPID must be class-whitelisted");
        assert!(x86_gate(valid), "state-backed RDPID must enter native tier");

        for (name, dst) in [
            ("host stack", x86(X86Reg::Rsp)),
            ("frame pointer", x86(X86Reg::Rbp)),
            ("virtual", VReg::Virtual(VirtualId(1))),
        ] {
            let op = OpKind::X86ReadPid { dst };
            assert!(op.is_jit_safe(), "{name} remains class-whitelisted");
            assert!(!x86_gate(op), "malformed {name} RDPID must deopt");
        }
        assert!(
            x86_gate(OpKind::X86ReadPid {
                dst: x86(X86Reg::R16),
            }),
            "APX RDPID must update the state-backed EGPR slot"
        );
    }

    #[test]
    fn x86_xcr_gate_admits_only_fixed_implicit_register_shapes() {
        let valid = OpKind::X86XGetBv {
            dst_low: x86(X86Reg::Rax),
            dst_high: x86(X86Reg::Rdx),
            selector: x86(X86Reg::Rcx),
        };
        assert!(valid.is_jit_safe(), "XGETBV must be class-whitelisted");
        assert!(
            x86_gate(valid),
            "architectural XGETBV must enter native tier"
        );

        for malformed in [
            OpKind::X86XGetBv {
                dst_low: x86(X86Reg::R8),
                dst_high: x86(X86Reg::Rdx),
                selector: x86(X86Reg::Rcx),
            },
            OpKind::X86XGetBv {
                dst_low: x86(X86Reg::Rax),
                dst_high: x86(X86Reg::R9),
                selector: x86(X86Reg::Rcx),
            },
            OpKind::X86XGetBv {
                dst_low: x86(X86Reg::Rax),
                dst_high: x86(X86Reg::Rdx),
                selector: x86(X86Reg::R10),
            },
        ] {
            assert!(malformed.is_jit_safe());
            assert!(!x86_gate(malformed), "malformed XGETBV must deopt");
        }

        let xsetbv = OpKind::X86XSetBv {
            selector: x86(X86Reg::Rcx),
            src_low: x86(X86Reg::Rax),
            src_high: x86(X86Reg::Rdx),
        };
        assert!(xsetbv.is_jit_safe(), "XSETBV must be class-whitelisted");
        assert!(
            !x86_gate(xsetbv.clone()),
            "XSETBV without a known handoff PC must deopt"
        );
        let xset_gate = |op| {
            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            builder.push_op(0x1000, op);
            builder.push_op(0x1003, OpKind::Nop);
            builder.set_terminator(Terminator::Return { values: vec![] });
            is_native_clobber_safe(&builder.finish())
        };
        assert!(
            xset_gate(xsetbv),
            "architectural XSETBV with a handoff PC must enter native tier"
        );
        for malformed in [
            OpKind::X86XSetBv {
                selector: x86(X86Reg::R8),
                src_low: x86(X86Reg::Rax),
                src_high: x86(X86Reg::Rdx),
            },
            OpKind::X86XSetBv {
                selector: x86(X86Reg::Rcx),
                src_low: x86(X86Reg::R9),
                src_high: x86(X86Reg::Rdx),
            },
            OpKind::X86XSetBv {
                selector: x86(X86Reg::Rcx),
                src_low: x86(X86Reg::Rax),
                src_high: x86(X86Reg::R10),
            },
        ] {
            assert!(malformed.is_jit_safe());
            assert!(!xset_gate(malformed), "malformed XSETBV must deopt");
        }
    }

    #[test]
    fn clobber_gate_accepts_exact_bit_scan_shapes_and_rejects_malformed_ir() {
        let valid_flags = FlagUpdate::Specific(FlagSet::ZF);
        for op in [
            OpKind::Bsf {
                dst: x86(X86Reg::R8),
                src: x86(X86Reg::Rax),
                width: OpWidth::W16,
                flags: valid_flags,
            },
            OpKind::Bsr {
                dst: x86(X86Reg::R15),
                src: x86(X86Reg::R14),
                width: OpWidth::W64,
                flags: valid_flags,
            },
        ] {
            assert!(op.is_jit_safe(), "bit scan must be on the scalar whitelist");
            assert!(x86_gate(op), "well-formed bit scan must enter native JIT");
        }

        for (name, op) in [
            (
                "byte width",
                OpKind::Bsf {
                    dst: x86(X86Reg::Rax),
                    src: x86(X86Reg::Rcx),
                    width: OpWidth::W8,
                    flags: valid_flags,
                },
            ),
            (
                "wrong flag contract",
                OpKind::Bsr {
                    dst: x86(X86Reg::Rax),
                    src: x86(X86Reg::Rcx),
                    width: OpWidth::W64,
                    flags: FlagUpdate::All,
                },
            ),
            (
                "guest stack source",
                OpKind::Bsf {
                    dst: x86(X86Reg::Rax),
                    src: x86(X86Reg::Rsp),
                    width: OpWidth::W64,
                    flags: valid_flags,
                },
            ),
            (
                "extended guest register",
                OpKind::Bsr {
                    dst: x86(X86Reg::R16),
                    src: x86(X86Reg::Rax),
                    width: OpWidth::W32,
                    flags: valid_flags,
                },
            ),
        ] {
            assert!(!x86_gate(op), "malformed {name} bit scan must deopt");
        }
    }

    #[test]
    fn clobber_gate_rejects_flag_preserving_x86_native_flag_clobber_ops() {
        for (name, op) in [
            (
                "adc",
                OpKind::Adc {
                    dst: x86(X86Reg::Rax),
                    src1: x86(X86Reg::Rax),
                    src2: SrcOperand::Reg(x86(X86Reg::Rcx)),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
            ),
            (
                "sbb",
                OpKind::Sbb {
                    dst: x86(X86Reg::Rax),
                    src1: x86(X86Reg::Rax),
                    src2: SrcOperand::Reg(x86(X86Reg::Rcx)),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
            ),
            (
                "shld",
                OpKind::Shld {
                    dst: x86(X86Reg::Rax),
                    src: x86(X86Reg::Rdx),
                    amount: SrcOperand::Reg(x86(X86Reg::Rcx)),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
            ),
            (
                "shrd",
                OpKind::Shrd {
                    dst: x86(X86Reg::Rax),
                    src: x86(X86Reg::Rdx),
                    amount: SrcOperand::Reg(x86(X86Reg::Rcx)),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
            ),
        ] {
            assert!(op.is_jit_safe(), "{name} remains on the generic whitelist");
            assert!(
                !x86_gate(op),
                "{name} must preserve guest flags by deopting"
            );
        }
    }

    #[test]
    fn clobber_gate_admits_flag_preserving_shifts_and_direct_cl_aliases() {
        let rax = x86(X86Reg::Rax);
        let rcx = x86(X86Reg::Rcx);
        for (name, op) in [
            (
                "shl",
                OpKind::Shl {
                    dst: rax,
                    src: rax,
                    amount: SrcOperand::Imm(4),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
            ),
            (
                "shr",
                OpKind::Shr {
                    dst: rax,
                    src: rax,
                    amount: SrcOperand::Imm(4),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
            ),
            (
                "sar",
                OpKind::Sar {
                    dst: rax,
                    src: rax,
                    amount: SrcOperand::Imm(4),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
            ),
            (
                "rol",
                OpKind::Rol {
                    dst: rax,
                    src: rax,
                    amount: SrcOperand::Imm(4),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
            ),
            (
                "ror",
                OpKind::Ror {
                    dst: rax,
                    src: rax,
                    amount: SrcOperand::Imm(4),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
            ),
            (
                "CL-alias shl",
                OpKind::Shl {
                    dst: rcx,
                    src: rax,
                    amount: SrcOperand::Reg(rcx),
                    width: OpWidth::W64,
                    flags: FlagUpdate::All,
                },
            ),
            (
                "NF CL-alias shl",
                OpKind::Shl {
                    dst: rcx,
                    src: rax,
                    amount: SrcOperand::Reg(rcx),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
            ),
        ] {
            assert!(x86_gate(op), "{name} must remain native-eligible");
        }
    }

    #[test]
    fn clobber_gate_admits_flag_preserving_binary_alu_including_ndd_aliases() {
        let rax = x86(X86Reg::Rax);
        let r8 = x86(X86Reg::R8);
        for (name, op) in [
            (
                "add",
                OpKind::Add {
                    dst: r8,
                    src1: rax,
                    src2: SrcOperand::Reg(r8),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
            ),
            (
                "sub",
                OpKind::Sub {
                    dst: r8,
                    src1: rax,
                    src2: SrcOperand::Reg(r8),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
            ),
            (
                "and",
                OpKind::And {
                    dst: r8,
                    src1: rax,
                    src2: SrcOperand::Reg(r8),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
            ),
            (
                "or",
                OpKind::Or {
                    dst: r8,
                    src1: rax,
                    src2: SrcOperand::Reg(r8),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
            ),
            (
                "xor",
                OpKind::Xor {
                    dst: r8,
                    src1: rax,
                    src2: SrcOperand::Reg(r8),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
            ),
        ] {
            assert!(
                x86_gate(op),
                "NF APX NDD {name} must remain native-eligible"
            );
        }
    }

    #[test]
    fn clobber_gate_allows_dead_flag_preserving_x86_native_flag_clobber_ops() {
        let mut b = FunctionBuilder::new(FunctionId(0), 0x1000);
        b.push_op(
            0x1000,
            OpKind::Add {
                dst: x86(X86Reg::Rax),
                src1: x86(X86Reg::Rax),
                src2: SrcOperand::Imm(1),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        );
        b.push_op(
            0x1003,
            OpKind::Cmp {
                src1: x86(X86Reg::Rcx),
                src2: SrcOperand::Imm(0),
                width: OpWidth::W64,
            },
        );
        b.set_terminator(Terminator::Return { values: vec![] });

        assert!(
            is_native_clobber_safe(&b.finish()),
            "a later flag definition kills the exit live set, so the dead flag-preserving add can run natively"
        );
    }

    #[test]
    fn clobber_gate_allows_live_flags_across_natively_preserved_binary_alu() {
        let mut b = FunctionBuilder::new(FunctionId(0), 0x1000);
        b.push_op(
            0x1000,
            OpKind::Cmp {
                src1: x86(X86Reg::Rcx),
                src2: SrcOperand::Imm(0),
                width: OpWidth::W64,
            },
        );
        b.push_op(
            0x1003,
            OpKind::Add {
                dst: x86(X86Reg::Rax),
                src1: x86(X86Reg::Rax),
                src2: SrcOperand::Imm(1),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        );
        b.push_op(
            0x1006,
            OpKind::SetCC {
                dst: x86(X86Reg::Rbx),
                cond: crate::smir::ir::types::Condition::Eq,
                width: OpWidth::W64,
            },
        );
        b.set_terminator(Terminator::Return { values: vec![] });

        assert!(
            is_native_clobber_safe(&b.finish()),
            "the lowerer now preserves flags across ADD flags=None before setcc"
        );
    }

    #[test]
    fn clobber_gate_allows_flag_updating_x86_alu() {
        assert!(x86_gate(OpKind::Add {
            dst: x86(X86Reg::Rax),
            src1: x86(X86Reg::Rax),
            src2: SrcOperand::Reg(x86(X86Reg::Rcx)),
            width: OpWidth::W64,
            flags: FlagUpdate::All,
        }));
    }

    #[test]
    fn clobber_gate_admits_apx_ndd_binary_alu_aliasing_second_source() {
        let rax = x86(X86Reg::Rax);
        let r8 = x86(X86Reg::R8);
        for (name, op) in [
            (
                "add",
                OpKind::Add {
                    dst: r8,
                    src1: rax,
                    src2: SrcOperand::Reg(r8),
                    width: OpWidth::W64,
                    flags: FlagUpdate::All,
                },
            ),
            (
                "or",
                OpKind::Or {
                    dst: r8,
                    src1: rax,
                    src2: SrcOperand::Reg(r8),
                    width: OpWidth::W64,
                    flags: FlagUpdate::All,
                },
            ),
            (
                "and",
                OpKind::And {
                    dst: r8,
                    src1: rax,
                    src2: SrcOperand::Reg(r8),
                    width: OpWidth::W64,
                    flags: FlagUpdate::All,
                },
            ),
            (
                "sub",
                OpKind::Sub {
                    dst: r8,
                    src1: rax,
                    src2: SrcOperand::Reg(r8),
                    width: OpWidth::W64,
                    flags: FlagUpdate::All,
                },
            ),
            (
                "xor",
                OpKind::Xor {
                    dst: r8,
                    src1: rax,
                    src2: SrcOperand::Reg(r8),
                    width: OpWidth::W64,
                    flags: FlagUpdate::All,
                },
            ),
        ] {
            assert!(x86_gate(op), "alias-safe APX NDD {name} must JIT");
        }
    }

    #[test]
    fn clobber_gate_admits_apx_ndd_imul_aliasing_second_source_with_or_without_nf() {
        let rax = x86(X86Reg::Rax);
        let rbx = x86(X86Reg::Rbx);
        for flags in [FlagUpdate::All, FlagUpdate::None] {
            assert!(
                x86_gate(OpKind::MulS {
                    dst_lo: rbx,
                    dst_hi: None,
                    src1: rax,
                    src2: SrcOperand::Reg(rbx),
                    width: OpWidth::W64,
                    flags,
                }),
                "alias-safe APX NDD IMUL {flags:?} must JIT"
            );
        }
    }

    #[test]
    fn x86_aarch64_gate_accepts_sub64_multiply_contracts_and_partial_writes() {
        let rax = x86(X86Reg::Rax);
        let rbx = x86(X86Reg::Rbx);
        let rdx = x86(X86Reg::Rdx);
        for src2 in [SrcOperand::Reg(rbx), SrcOperand::Imm(0x1234)] {
            assert!(
                x86_aarch64_gate(vec![OpKind::MulS {
                    dst_lo: rbx,
                    dst_hi: None,
                    src1: rax,
                    src2,
                    width: OpWidth::W16,
                    flags: FlagUpdate::None,
                }]),
                "APX NF W16 single-result signed multiply"
            );
        }

        let flag_setting = OpKind::MulS {
            dst_lo: rbx,
            dst_hi: None,
            src1: rax,
            src2: SrcOperand::Imm(2),
            width: OpWidth::W16,
            flags: FlagUpdate::All,
        };
        assert!(x86_aarch64_scalar_shape_valid(&flag_setting));
        assert!(x86_aarch64_block_flags_are_representable(
            &{
                let mut builder = FunctionBuilder::new(FunctionId(7), 0x7000);
                builder.push_op(0x7000, flag_setting.clone());
                builder.set_terminator(Terminator::Return { values: vec![] });
                builder.finish().blocks.remove(0)
            },
            FlagSet::EMPTY,
        ));
        assert!(
            !x86_aarch64_gate(vec![flag_setting]),
            "terminal flag-setting IMUL defines unavailable live PF/AF"
        );

        for op in [
            OpKind::MulU {
                dst_lo: rax,
                dst_hi: Some(rdx),
                src1: rax,
                src2: SrcOperand::Reg(rbx),
                width: OpWidth::W16,
                flags: FlagUpdate::None,
            },
            OpKind::MulS {
                dst_lo: rax,
                dst_hi: Some(rdx),
                src1: rax,
                src2: SrcOperand::Reg(rdx),
                width: OpWidth::W16,
                flags: FlagUpdate::None,
            },
            OpKind::MulU {
                dst_lo: rbx,
                dst_hi: Some(rbx),
                src1: rdx,
                src2: SrcOperand::Reg(rax),
                width: OpWidth::W32,
                flags: FlagUpdate::None,
            },
            OpKind::MulU {
                dst_lo: rbx,
                dst_hi: Some(rbx),
                src1: rdx,
                src2: SrcOperand::Reg(rax),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        ] {
            assert!(x86_aarch64_gate(vec![op.clone()]), "supported {op:?}");
        }

        for op in [
            OpKind::MulS {
                dst_lo: rax,
                dst_hi: None,
                src1: rax,
                src2: SrcOperand::Reg(rbx),
                width: OpWidth::W8,
                flags: FlagUpdate::None,
            },
            OpKind::MulU {
                dst_lo: rax,
                dst_hi: None,
                src1: rax,
                src2: SrcOperand::Reg(rbx),
                width: OpWidth::W16,
                flags: FlagUpdate::None,
            },
        ] {
            assert!(!x86_aarch64_scalar_shape_valid(&op), "unsupported {op:?}");
        }

        for op in [
            OpKind::MulS {
                dst_lo: rbx,
                dst_hi: Some(rdx),
                src1: rax,
                src2: SrcOperand::Reg(rbx),
                width: OpWidth::W16,
                flags: FlagUpdate::None,
            },
            OpKind::MulU {
                dst_lo: rax,
                dst_hi: Some(rdx),
                src1: rax,
                src2: SrcOperand::Imm(3),
                width: OpWidth::W16,
                flags: FlagUpdate::None,
            },
            OpKind::MulU {
                dst_lo: rax,
                dst_hi: Some(rdx),
                src1: rax,
                src2: SrcOperand::Reg(rbx),
                width: OpWidth::W16,
                flags: FlagUpdate::All,
            },
        ] {
            assert!(
                x86_aarch64_scalar_shape_valid(&op),
                "lowerer-capable but non-architectural shape {op:?}"
            );
            assert!(!x86_aarch64_gate(vec![op.clone()]), "rejected {op:?}");
        }
    }

    #[test]
    fn clobber_gate_admits_only_exact_architectural_apx_ndd_double_shift_shapes() {
        let rax = x86(X86Reg::Rax);
        let rcx = x86(X86Reg::Rcx);
        let rbx = x86(X86Reg::Rbx);
        for op in [
            OpKind::X86NddDoubleShift {
                dst: rbx,
                base: rax,
                fill: rbx,
                amount: SrcOperand::Imm(4),
                width: OpWidth::W64,
                left: true,
                flags: FlagUpdate::All,
            },
            OpKind::X86NddDoubleShift {
                dst: rcx,
                base: rax,
                fill: rbx,
                amount: SrcOperand::Reg(rcx),
                width: OpWidth::W64,
                left: false,
                flags: FlagUpdate::None,
            },
        ] {
            assert!(x86_gate(op), "valid APX NDD double shift must JIT");
        }

        for op in [
            OpKind::X86NddDoubleShift {
                dst: rbx,
                base: VReg::Virtual(VirtualId(21)),
                fill: rbx,
                amount: SrcOperand::Imm(4),
                width: OpWidth::W64,
                left: true,
                flags: FlagUpdate::All,
            },
            OpKind::X86NddDoubleShift {
                dst: rbx,
                base: rax,
                fill: rbx,
                amount: SrcOperand::Reg(x86(X86Reg::Rdx)),
                width: OpWidth::W64,
                left: false,
                flags: FlagUpdate::All,
            },
            OpKind::X86NddDoubleShift {
                dst: rbx,
                base: rax,
                fill: rbx,
                amount: SrcOperand::Imm(4),
                width: OpWidth::W8,
                left: true,
                flags: FlagUpdate::All,
            },
            OpKind::X86NddDoubleShift {
                dst: x86(X86Reg::R16),
                base: rax,
                fill: rbx,
                amount: SrcOperand::Imm(4),
                width: OpWidth::W64,
                left: true,
                flags: FlagUpdate::All,
            },
        ] {
            assert!(!x86_gate(op), "malformed APX NDD double shift must deopt");
        }
    }

    #[test]
    fn clobber_gate_admits_explicit_legacy_high_byte_movx_shapes() {
        for src in [X86Reg::Rsi, X86Reg::Rdi] {
            assert!(
                !x86_gate(OpKind::ZeroExtend {
                    dst: x86(X86Reg::Rax),
                    src: x86(src),
                    from_width: OpWidth::W8,
                    to_width: OpWidth::W64,
                }),
                "W8 ZeroExtend from {src:?} can be legacy DH/BH and must deopt"
            );
            assert!(
                !x86_gate(OpKind::SignExtend {
                    dst: x86(X86Reg::Rax),
                    src: x86(src),
                    from_width: OpWidth::W8,
                    to_width: OpWidth::W64,
                }),
                "W8 SignExtend from {src:?} can be legacy DH/BH and must deopt"
            );
        }

        assert!(
            x86_gate(OpKind::ZeroExtend {
                dst: x86(X86Reg::Rax),
                src: x86(X86Reg::Rdx),
                from_width: OpWidth::W8,
                to_width: OpWidth::W64,
            }),
            "unambiguous DL byte source stays native-eligible"
        );
        assert!(
            x86_gate(OpKind::ZeroExtend {
                dst: x86(X86Reg::Rax),
                src: x86(X86Reg::Rsi),
                from_width: OpWidth::W16,
                to_width: OpWidth::W64,
            }),
            "word-sized RSI source is not a high-byte register ambiguity"
        );

        for op in [
            OpKind::ZeroExtend {
                dst: x86(X86Reg::Rax),
                src: x86(X86Reg::Rdi),
                from_width: OpWidth::W8,
                to_width: OpWidth::W64,
            },
            OpKind::SignExtend {
                dst: x86(X86Reg::Rax),
                src: x86(X86Reg::Rsi),
                from_width: OpWidth::W8,
                to_width: OpWidth::W64,
            },
        ] {
            let mut b = FunctionBuilder::new(FunctionId(0), 0x1000);
            b.push_op(0x1000, op);
            b.set_terminator(Terminator::Return { values: vec![] });
            let mut func = b.finish();
            func.blocks[0].ops[0].x86_hint = Some(X86OpHint::RexByteReg);
            assert!(
                is_native_clobber_safe(&func),
                "REX-prefixed byte-register MOVX cannot be AH/CH/DH/BH and may JIT"
            );
        }

        for (src, op) in [
            (
                X86Reg::Rax,
                OpKind::ZeroExtend {
                    dst: x86(X86Reg::Rax),
                    src: x86(X86Reg::Rax),
                    from_width: OpWidth::W8,
                    to_width: OpWidth::W32,
                },
            ),
            (
                X86Reg::Rcx,
                OpKind::ZeroExtend {
                    dst: x86(X86Reg::Rdx),
                    src: x86(X86Reg::Rcx),
                    from_width: OpWidth::W8,
                    to_width: OpWidth::W16,
                },
            ),
            (
                X86Reg::Rdx,
                OpKind::SignExtend {
                    dst: x86(X86Reg::Rsi),
                    src: x86(X86Reg::Rdx),
                    from_width: OpWidth::W8,
                    to_width: OpWidth::W32,
                },
            ),
            (
                X86Reg::Rbx,
                OpKind::SignExtend {
                    dst: x86(X86Reg::Rdi),
                    src: x86(X86Reg::Rbx),
                    from_width: OpWidth::W8,
                    to_width: OpWidth::W16,
                },
            ),
        ] {
            let mut b = FunctionBuilder::new(FunctionId(0), 0x1000);
            b.push_op(0x1000, op);
            b.set_terminator(Terminator::Return { values: vec![] });
            let mut func = b.finish();
            func.blocks[0].ops[0].x86_hint = Some(X86OpHint::LegacyHighByteReg);
            assert!(
                is_native_clobber_safe(&func),
                "explicit legacy high-byte parent {src:?} must JIT"
            );
        }

        for op in [
            OpKind::ZeroExtend {
                dst: x86(X86Reg::Rax),
                src: x86(X86Reg::Rsi),
                from_width: OpWidth::W8,
                to_width: OpWidth::W32,
            },
            OpKind::SignExtend {
                dst: x86(X86Reg::R8),
                src: x86(X86Reg::Rbx),
                from_width: OpWidth::W8,
                to_width: OpWidth::W32,
            },
            OpKind::ZeroExtend {
                dst: x86(X86Reg::Rax),
                src: x86(X86Reg::Rax),
                from_width: OpWidth::W16,
                to_width: OpWidth::W32,
            },
        ] {
            let mut b = FunctionBuilder::new(FunctionId(0), 0x1000);
            b.push_op(0x1000, op);
            b.set_terminator(Terminator::Return { values: vec![] });
            let mut func = b.finish();
            func.blocks[0].ops[0].x86_hint = Some(X86OpHint::LegacyHighByteReg);
            assert!(
                !is_native_clobber_safe(&func),
                "malformed legacy high-byte hint must deopt"
            );
        }
    }

    // Regression for issue #14: alias-safe ADC/SBB lowering removes the former
    // deliberate deopt for APX NDD operations whose destination is source 2.
    #[test]
    fn clobber_gate_admits_adc_sbb_dst_aliasing_src2() {
        fn gate(op: OpKind) -> bool {
            let mut b = FunctionBuilder::new(FunctionId(0), 0x1000);
            b.push_op(0x1000, op);
            b.set_terminator(Terminator::Return { values: vec![] });
            is_native_clobber_safe(&b.finish())
        }

        for op in [
            OpKind::Adc {
                dst: x86(X86Reg::R8),
                src1: x86(X86Reg::Rax),
                src2: SrcOperand::Reg(x86(X86Reg::R8)),
                width: OpWidth::W64,
                flags: FlagUpdate::All,
            },
            OpKind::Sbb {
                dst: x86(X86Reg::R8),
                src1: x86(X86Reg::Rax),
                src2: SrcOperand::Reg(x86(X86Reg::R8)),
                width: OpWidth::W64,
                flags: FlagUpdate::All,
            },
        ] {
            assert!(
                gate(op),
                "alias-safe ADC/SBB with dst==src2 must remain native-eligible"
            );
        }

        // A non-aliased ADC (dst != src2) stays native-eligible.
        assert!(
            gate(OpKind::Adc {
                dst: x86(X86Reg::R8),
                src1: x86(X86Reg::Rax),
                src2: SrcOperand::Reg(x86(X86Reg::Rcx)),
                width: OpWidth::W64,
                flags: FlagUpdate::All,
            }),
            "non-aliased ADC must stay native-eligible"
        );
    }

    #[test]
    fn aarch64_clobber_gate_rejects_fp_mixed_with_mem_helpers() {
        let fp_add = OpKind::FAdd {
            dst: arm_v(0),
            src1: arm_v(1),
            src2: arm_v(2),
            precision: FpPrecision::F64,
        };
        let load = OpKind::Load {
            dst: arm_x(0),
            addr: Address::Direct(arm_x(1)),
            width: MemWidth::B8,
            sign: SignExtend::Zero,
        };

        assert!(
            aarch64_gate(vec![fp_add.clone()], true),
            "pure FP blocks may use the FP trampoline"
        );
        assert!(
            aarch64_gate(vec![load.clone()], true),
            "integer memory-helper blocks stay eligible when memory JIT is enabled"
        );
        assert!(
            !aarch64_gate(vec![load.clone()], false),
            "memory ops still require the memory-helper gate"
        );
        assert!(
            !aarch64_gate(vec![fp_add, load], true),
            "helper-call regions must not run with live guest SIMD state"
        );
    }
}

#[cfg(all(test, target_arch = "x86_64"))]
mod tests {
    use super::*;

    // Hand-assembled `mov eax, 0x2a ; ret` — proves ExecMem (W^X map) and the
    // enter_native trampoline marshal a result back out, independent of the
    // lowerer. The lowerer-driven end-to-end paths live in
    // tests/suites/differential/x86_64/fuzz.rs.
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

    use crate::smir::ir::flags::FlagUpdate;
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::{
        ArchReg, Condition, FunctionId, OpWidth, SrcOperand, VReg, X86Reg,
    };
    use crate::smir::ir::{FunctionBuilder, Terminator, TrapKind};

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
                flags: FlagUpdate::All,
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
                flags: FlagUpdate::All,
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
                flags: FlagUpdate::All,
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
                flags: FlagUpdate::All,
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

    #[test]
    fn aarch32_exit_result_distinguishes_ordinary_return() {
        let code = 0xd65f_03c0u32.to_le_bytes(); // ret
        let mem = ExecMem::new(&code).expect("ExecMem map");
        let mut regs = Aarch32GuestRegs::default();
        let result = mem.run_aarch32_identity_exit(0, &mut regs);
        assert_eq!(
            result,
            Aarch32NativeExit {
                exited: false,
                pc: 0
            }
        );
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

    fn read_host_fpcr() -> u64 {
        let value: u64;
        unsafe {
            core::arch::asm!(
                "mrs {value}, fpcr",
                value = out(reg) value,
                options(nomem, nostack, preserves_flags)
            );
        }
        value
    }

    #[test]
    fn exec_mem_fp_trampoline_roundtrips_fpcr_aarch64() {
        // d51b4400 msr fpcr, x0 ; d65f03c0 ret
        let code: [u8; 8] = [0x00, 0x44, 0x1b, 0xd5, 0xc0, 0x03, 0x5f, 0xd6];
        let mem = ExecMem::new(&code).expect("ExecMem map");
        let mut regs = Aarch64GuestRegs::default();
        regs.x[0] = 0x00c0_0000;
        let host_fpcr = read_host_fpcr();

        mem.run_aarch64_identity_fp(0, &mut regs);

        assert_eq!(regs.fpcr & 0xffff_ffff, 0x00c0_0000);
        assert_eq!(read_host_fpcr(), host_fpcr, "host FPCR must be restored");
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
