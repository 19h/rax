//! Opt-in production RISC-V SMIR native execution path.
//!
//! One decoded guest instruction is lifted, optimized, lowered, and cached by
//! `(PC, encoding, length, optimization level)`. Unsupported boundaries remain
//! interpreter-exact: they execute through `RiscVCpu::execute` using the already
//! fetched instruction. Native memory helpers record precise synchronous traps
//! in a stack-owned context, so faulting operations are not replayed and device
//! accesses are not duplicated.

use std::collections::HashMap;
use std::sync::Arc;

use super::*;
use crate::smir::RiscVLifter;
use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{BlockId, FunctionId, OpId, SourceArch};
use crate::smir::ir::{CallingConv, SmirBlock, SmirFunction, Terminator};
use crate::smir::lift::riscv::RiscVExtensions;
use crate::smir::lift::{ControlFlow, LiftContext, LiftResult, SmirLifter};
use crate::smir::lower::SmirLowerer;
#[cfg(target_arch = "aarch64")]
use crate::smir::lower::cross::riscv_guest_to_aarch64_host::RiscVAarch64Lowerer;
#[cfg(target_arch = "x86_64")]
use crate::smir::lower::cross::riscv_guest_to_x86_64_host::RiscVX86_64Lowerer;
use crate::smir::lower::runtime::{
    ExecMem, RISCV_FP_RESULT_INVALID, RiscVAtomicCasResult, RiscVAtomicCasStatus,
    RiscVAtomicResult, RiscVFpOpCode, RiscVFpResult, RiscVGuestRegs, RiscVLoadResult,
};
use crate::smir::optimize::{OptLevel, optimize_function};

const MAX_CACHE_ENTRIES: usize = 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct JitKey {
    pc: u64,
    raw: u32,
    len: u8,
    opt_level: u8,
}

impl JitKey {
    fn new(pc: u64, insn: &Insn, level: OptLevel) -> Self {
        Self {
            pc,
            raw: insn.raw,
            len: insn.len,
            opt_level: match level {
                OptLevel::O0 => 0,
                OptLevel::O1 => 1,
                OptLevel::O2 => 2,
            },
        }
    }
}

struct NativeBlock {
    executable: ExecMem,
    entry_offset: usize,
}

#[derive(Clone)]
enum CacheEntry {
    Native(Arc<NativeBlock>),
    InterpreterOnly,
}

/// Per-hart JIT cache and observability counters.
#[derive(Default)]
pub(super) struct RiscVJitCache {
    entries: HashMap<JitKey, CacheEntry>,
    cache_hits: u64,
    cache_misses: u64,
    native_executions: u64,
    interpreter_fallbacks: u64,
}

impl RiscVJitCache {
    pub(super) fn clear(&mut self) {
        *self = Self::default();
    }

    fn stats(&self) -> RiscVJitStats {
        RiscVJitStats {
            cache_entries: self.entries.len(),
            cache_hits: self.cache_hits,
            cache_misses: self.cache_misses,
            native_executions: self.native_executions,
            interpreter_fallbacks: self.interpreter_fallbacks,
        }
    }

    fn resolve(
        &mut self,
        key: JitKey,
        cfg: RiscVConfig,
        insn: &Insn,
        bytes: &[u8],
        level: OptLevel,
    ) -> CacheEntry {
        if let Some(entry) = self.entries.get(&key) {
            self.cache_hits = self.cache_hits.wrapping_add(1);
            return entry.clone();
        }

        self.cache_misses = self.cache_misses.wrapping_add(1);
        let entry = compile_native_block(cfg, insn, key.pc, bytes, level)
            .map_or(CacheEntry::InterpreterOnly, CacheEntry::Native);
        if self.entries.len() >= MAX_CACHE_ENTRIES {
            self.entries.clear();
        }
        self.entries.insert(key, entry.clone());
        entry
    }
}

impl RiscVCpu {
    /// Fetch and execute one instruction through the host-native RISC-V SMIR JIT
    /// when the instruction lies inside the admitted native boundary.
    ///
    /// Unsupported instructions, vector operations, environmental operations,
    /// and lowering failures execute through the interpreter using the same
    /// already-decoded instruction. The ordinary [`Self::step`] path never
    /// enters native code.
    pub fn step_jit(&mut self, level: OptLevel) -> RiscVExit {
        if let Some(trap) = self.pending_machine_interrupt() {
            self.deliver_trap(trap, self.pc);
            return RiscVExit::Continue;
        }

        let pc = self.pc;
        let insn = match decode_at(self.mem.as_ref(), pc, self.cfg.xlen, &self.cfg.isa) {
            Ok(insn) => insn,
            Err(DecodeError::Fetch(_)) => {
                let trap = Trap {
                    cause: cause::INSTR_ACCESS_FAULT,
                    tval: pc,
                };
                self.deliver_trap(trap, pc);
                return RiscVExit::Trap(trap);
            }
        };
        let raw = insn.raw.to_le_bytes();
        let bytes = &raw[..usize::from(insn.len)];
        let key = JitKey::new(pc, &insn, level);
        let entry = self.jit.resolve(key, self.cfg, &insn, bytes, level);
        let CacheEntry::Native(block) = entry else {
            return self.execute_jit_fallback(&insn, pc);
        };

        let reservation_before = self.reservation;
        let memory = self.mem.as_mut() as *mut dyn Memory;
        let reservation = &mut self.reservation as *mut Option<u64>;
        let mut context = JitContext {
            memory,
            reservation,
            address_mask: self.xmask(),
            fault: None,
        };
        let mut state = self.export_jit_state();
        state.ctx = (&mut context as *mut JitContext) as usize as u64;
        state.load_fn = jit_load as *const () as usize as u64;
        state.store_fn = jit_store as *const () as usize as u64;
        state.atomic_rmw_fn = jit_atomic_rmw as *const () as usize as u64;
        state.cas_fn = jit_compare_and_swap as *const () as usize as u64;
        state.cas_pair_fn = jit_compare_and_swap_pair as *const () as usize as u64;
        state.load_exclusive_fn = jit_load_exclusive as *const () as usize as u64;
        state.store_exclusive_fn = jit_store_exclusive as *const () as usize as u64;
        state.clear_exclusive_fn = jit_clear_exclusive as *const () as usize as u64;
        state.int_crypto_fn = jit_int_crypto as *const () as usize as u64;
        state.fp_fn = jit_scalar_fp as *const () as usize as u64;
        state.vector_fn = jit_vector_unsupported as *const () as usize as u64;

        block.executable.run_riscv(block.entry_offset, &mut state);
        self.jit.native_executions = self.jit.native_executions.wrapping_add(1);

        match (state.exit_reason, context.fault) {
            (0, None) => {
                self.import_jit_state(&state);
                self.cycle = self.cycle.wrapping_add(1);
                self.instret = self.instret.wrapping_add(1);
                RiscVExit::Continue
            }
            (1, Some(trap)) => {
                self.cycle = self.cycle.wrapping_add(1);
                self.deliver_trap(trap, pc);
                RiscVExit::Trap(trap)
            }
            _ => {
                // Helper-free native failures (for example an illegal dynamic
                // FP rounding mode) are replay-safe. Restore non-state ABI data
                // before executing the decoded interpreter path.
                self.reservation = reservation_before;
                self.execute_jit_fallback(&insn, pc)
            }
        }
    }

    /// Run through [`Self::step_jit`] until a non-`Continue` exit or the
    /// instruction budget is exhausted.
    pub fn run_jit(&mut self, max_insns: u64, level: OptLevel) -> RiscVExit {
        for _ in 0..max_insns {
            match self.step_jit(level) {
                RiscVExit::Continue => {}
                exit => return exit,
            }
        }
        RiscVExit::Continue
    }

    /// Current JIT cache and execution counters.
    pub fn jit_stats(&self) -> RiscVJitStats {
        self.jit.stats()
    }

    /// Drop all cached native/interpreter decisions and reset JIT counters.
    pub fn clear_jit_cache(&mut self) {
        self.jit.clear();
    }

    fn execute_jit_fallback(&mut self, insn: &Insn, pc: u64) -> RiscVExit {
        self.jit.interpreter_fallbacks = self.jit.interpreter_fallbacks.wrapping_add(1);
        self.cycle = self.cycle.wrapping_add(1);
        match self.execute(insn, pc) {
            Ok(exit) => {
                self.instret = self.instret.wrapping_add(1);
                exit
            }
            Err(trap) => {
                self.deliver_trap(trap, pc);
                RiscVExit::Trap(trap)
            }
        }
    }

    fn export_jit_state(&self) -> RiscVGuestRegs {
        let mut state = RiscVGuestRegs {
            x: self.x,
            f: self.f,
            pc: self.pc,
            fcsr: u64::from(self.fcsr),
            vl: self.vl,
            vtype: self.vtype,
            vstart: self.vstart,
            vcsr: self.vcsr(),
            ..Default::default()
        };
        for register in 0..32 {
            let start = register * VLENB as usize;
            state.v[register].copy_from_slice(&self.v[start..start + VLENB as usize]);
        }
        state
    }

    fn import_jit_state(&mut self, state: &RiscVGuestRegs) {
        self.x = state.x.map(|value| value & self.xmask());
        self.x[0] = 0;
        self.f = state.f;
        self.pc = state.pc & self.xmask();
        self.fcsr = state.fcsr as u32 & 0xff;
        self.vl = state.vl;
        self.vtype = state.vtype;
        self.vstart = state.vstart;
        self.set_vcsr(state.vcsr);
        for register in 0..32 {
            let start = register * VLENB as usize;
            self.v[start..start + VLENB as usize].copy_from_slice(&state.v[register]);
        }
    }
}

fn compile_native_block(
    cfg: RiscVConfig,
    insn: &Insn,
    pc: u64,
    bytes: &[u8],
    level: OptLevel,
) -> Option<Arc<NativeBlock>> {
    // These decoded operations do not yet have an exact dedicated lift. Pair
    // loads/stores and Zc* macro instructions overlap encodings otherwise
    // consumed by the base scalar/compressed lift paths.
    if matches!(
        insn.op,
        Op::LdPair
            | Op::SdPair
            | Op::CmPush
            | Op::CmPop
            | Op::CmPopRetz
            | Op::CmPopRet
            | Op::CmMvsa01
            | Op::CmMva01s
            | Op::CmJt
            | Op::CmJalt
    ) {
        return None;
    }
    // Control-flow instruction-alignment traps without C are currently an
    // interpreter-only boundary: the scalar lifter represents only the target.
    if !cfg.isa.c
        && matches!(
            insn.op,
            Op::Jal | Op::Jalr | Op::Beq | Op::Bne | Op::Blt | Op::Bge | Op::Bltu | Op::Bgeu
        )
    {
        return None;
    }
    let extensions = extensions_for_isa(&cfg.isa);
    let mut lifter = match cfg.xlen {
        Xlen::Rv32 => RiscVLifter::new_rv32(extensions),
        Xlen::Rv64 => RiscVLifter::new_rv64(extensions),
    };
    let source_arch = match cfg.xlen {
        Xlen::Rv32 => SourceArch::RiscV32,
        Xlen::Rv64 => SourceArch::RiscV64,
    };
    let mut context = LiftContext::new(source_arch);
    let lifted = lifter.lift_insn(pc, bytes, &mut context).ok()?;
    if !admit_lifted_instruction(&lifted) {
        return None;
    }
    let (mut function, return_pcs) = function_for_lift(pc, lifted)?;
    optimize_function(&mut function, level);
    #[cfg(target_arch = "x86_64")]
    let mut lowerer = RiscVX86_64Lowerer::new();
    #[cfg(target_arch = "aarch64")]
    let mut lowerer = RiscVAarch64Lowerer::new();
    lowerer.set_return_pcs(return_pcs);
    let lowered = lowerer.lower_function(&function).ok()?;
    let code = lowerer.finalize().ok()?;
    let executable = ExecMem::new(&code).ok()?;
    Some(Arc::new(NativeBlock {
        executable,
        entry_offset: lowered.entry_offset,
    }))
}

fn admit_lifted_instruction(lifted: &LiftResult) -> bool {
    let mut memory_accesses = 0usize;
    for op in &lifted.ops {
        match op.kind {
            OpKind::RvVector { .. } | OpKind::Syscall { .. } | OpKind::Breakpoint => return false,
            OpKind::Load { .. }
            | OpKind::Store { .. }
            | OpKind::AtomicRmw { .. }
            | OpKind::Cas { .. }
            | OpKind::CasPair { .. }
            | OpKind::LoadExclusive { .. }
            | OpKind::StoreExclusive { .. } => memory_accesses += 1,
            _ => {}
        }
    }
    // Multiple helper calls can expose partial device reads/writes or an
    // instruction-specific partial-fault policy that the generic Memory trait
    // cannot roll back. Single-access instructions retain precise restart.
    memory_accesses <= 1
}

fn function_for_lift(pc: u64, lifted: LiftResult) -> Option<(SmirFunction, HashMap<BlockId, u64>)> {
    let entry = BlockId(0);
    let mut return_pcs = HashMap::new();
    let mut blocks = Vec::new();
    let terminator = match lifted.control_flow {
        ControlFlow::Fallthrough | ControlFlow::NextInsn => {
            return_pcs.insert(entry, pc.wrapping_add(lifted.bytes_consumed as u64));
            Terminator::Return { values: vec![] }
        }
        ControlFlow::Branch { target } | ControlFlow::DirectBranch(target) => {
            return_pcs.insert(entry, target);
            Terminator::Return { values: vec![] }
        }
        ControlFlow::CondBranchReg {
            cond,
            taken,
            not_taken,
        } => {
            let taken_id = BlockId(1);
            let not_taken_id = BlockId(2);
            return_pcs.insert(taken_id, taken);
            return_pcs.insert(not_taken_id, not_taken);
            blocks.push(SmirBlock {
                id: taken_id,
                guest_pc: taken,
                phis: vec![],
                ops: vec![],
                terminator: Terminator::Return { values: vec![] },
                exec_count: 0,
            });
            blocks.push(SmirBlock {
                id: not_taken_id,
                guest_pc: not_taken,
                phis: vec![],
                ops: vec![],
                terminator: Terminator::Return { values: vec![] },
                exec_count: 0,
            });
            Terminator::CondBranch {
                cond,
                true_target: taken_id,
                false_target: not_taken_id,
            }
        }
        ControlFlow::IndirectBranch { target } => Terminator::IndirectBranch {
            target,
            possible_targets: vec![],
        },
        _ => return None,
    };

    let mut ops = lifted.ops;
    for (index, op) in ops.iter_mut().enumerate() {
        op.id = OpId(index as u16);
    }
    blocks.insert(
        0,
        SmirBlock {
            id: entry,
            guest_pc: pc,
            phis: vec![],
            ops,
            terminator,
            exec_count: 0,
        },
    );
    let mut function = SmirFunction::new(FunctionId(0), entry, pc);
    function.blocks = blocks;
    function.guest_range = (pc, pc.wrapping_add(lifted.bytes_consumed as u64));
    function.calling_convention = CallingConv::RiscVStd;
    Some((function, return_pcs))
}

fn extensions_for_isa(isa: &Isa) -> RiscVExtensions {
    RiscVExtensions {
        m: isa.m,
        a: isa.a,
        f: isa.f,
        d: isa.d,
        q: isa.q,
        c: isa.c,
        zicsr: isa.zicsr,
        zifencei: isa.zifencei,
        zihintpause: isa.zihintpause,
        zihintntl: isa.zihintntl,
        zacas: isa.zacas,
        zawrs: isa.zawrs,
        zicbom: isa.zicbom,
        zicboz: isa.zicboz,
        zicbop: isa.zicbop,
        zba: isa.zba,
        zbb: isa.zbb,
        zbc: isa.zbc,
        zbs: isa.zbs,
        zicond: isa.zicond,
        zfa: isa.zfa,
        zbkb: isa.zbkb,
        zfh: isa.zfh,
        zbkx: isa.zbkx,
        zknh: isa.zknh,
        zksh: isa.zksh,
        zksed: isa.zksed,
        zkne: isa.zkne,
        zknd: isa.zknd,
        zcb: isa.zcb,
        zcmp: isa.zcmp,
        zcmt: isa.zcmt,
        zclsd: isa.zclsd,
        zilsd: isa.zilsd,
        h: isa.h,
        svinval: isa.svinval,
        v: isa.v,
        xandes: isa.xandes,
        xthead: isa.xthead,
        xhazard3: isa.xhazard3,
        xida_sltw: isa.xida_sltw,
    }
}

struct JitContext {
    memory: *mut dyn Memory,
    reservation: *mut Option<u64>,
    address_mask: u64,
    fault: Option<Trap>,
}

impl JitContext {
    unsafe fn from_abi<'a>(ctx: u64) -> Option<&'a mut Self> {
        if ctx == 0 {
            None
        } else {
            Some(unsafe { &mut *(ctx as usize as *mut Self) })
        }
    }

    fn record_fault(&mut self, cause: u64, tval: u64) {
        if self.fault.is_none() {
            self.fault = Some(Trap { cause, tval });
        }
    }

    fn normalize_addr(&self, addr: u64) -> u64 {
        addr & self.address_mask
    }

    unsafe fn memory(&mut self) -> &mut dyn Memory {
        unsafe { &mut *self.memory }
    }

    unsafe fn reservation(&mut self) -> &mut Option<u64> {
        unsafe { &mut *self.reservation }
    }
}

fn valid_size(size: u64) -> Option<usize> {
    match size {
        1 | 2 | 4 | 8 => Some(size as usize),
        _ => None,
    }
}

unsafe fn jit_load_impl(ctx: u64, addr: u64, size: u64, signed: u64) -> RiscVLoadResult {
    let Some(context) = (unsafe { JitContext::from_abi(ctx) }) else {
        return RiscVLoadResult::default();
    };
    let Some(size) = valid_size(size) else {
        context.record_fault(cause::LOAD_ACCESS_FAULT, addr);
        return RiscVLoadResult::default();
    };
    let addr = context.normalize_addr(addr);
    let mut bytes = [0u8; 8];
    if unsafe { context.memory() }
        .read(addr, &mut bytes[..size])
        .is_err()
    {
        context.record_fault(cause::LOAD_ACCESS_FAULT, addr);
        return RiscVLoadResult::default();
    }
    let raw = u64::from_le_bytes(bytes);
    RiscVLoadResult {
        value: if signed != 0 {
            sign_extend(raw, size)
        } else {
            raw & mask_bytes(size)
        },
        success: 1,
    }
}

unsafe fn jit_store_impl(ctx: u64, addr: u64, value: u64, size: u64) -> u64 {
    let Some(context) = (unsafe { JitContext::from_abi(ctx) }) else {
        return 0;
    };
    let Some(size) = valid_size(size) else {
        context.record_fault(cause::STORE_ACCESS_FAULT, addr);
        return 0;
    };
    let addr = context.normalize_addr(addr);
    if unsafe { context.memory() }
        .write(addr, &value.to_le_bytes()[..size])
        .is_err()
    {
        context.record_fault(cause::STORE_ACCESS_FAULT, addr);
        return 0;
    }
    1
}

fn decode_order(order: u64) -> Option<u64> {
    (order <= 4).then_some(order)
}

fn fence_before(order: u64) {
    use std::sync::atomic::{Ordering, fence};
    match order {
        2 | 3 => fence(Ordering::Release),
        4 => fence(Ordering::SeqCst),
        _ => {}
    }
}

fn fence_after(order: u64) {
    use std::sync::atomic::{Ordering, fence};
    match order {
        1 | 3 => fence(Ordering::Acquire),
        4 => fence(Ordering::SeqCst),
        _ => {}
    }
}

unsafe fn jit_atomic_rmw_impl(
    ctx: u64,
    addr: u64,
    operand: u64,
    size: u64,
    op: u64,
    order: u64,
) -> RiscVAtomicResult {
    let Some(context) = (unsafe { JitContext::from_abi(ctx) }) else {
        return RiscVAtomicResult::default();
    };
    let addr = context.normalize_addr(addr);
    if !matches!(size, 4 | 8) || addr % size != 0 || decode_order(order).is_none() || op > 11 {
        context.record_fault(cause::STORE_MISALIGNED, addr);
        return RiscVAtomicResult::default();
    }
    fence_before(order);
    let mut bytes = [0u8; 8];
    if unsafe { context.memory() }
        .read(addr, &mut bytes[..size as usize])
        .is_err()
    {
        context.record_fault(cause::LOAD_ACCESS_FAULT, addr);
        return RiscVAtomicResult::default();
    }
    let bits = size * 8;
    let mask = if size == 8 {
        u64::MAX
    } else {
        (1u64 << bits) - 1
    };
    let old = u64::from_le_bytes(bytes) & mask;
    let operand = operand & mask;
    let signed = |value: u64| -> i64 {
        if size == 8 {
            value as i64
        } else {
            value as u32 as i32 as i64
        }
    };
    let new = match op {
        0 => old.wrapping_add(operand),
        1 => old.wrapping_sub(operand),
        2 => 0u64.wrapping_sub(old),
        3 => old & operand,
        4 => old | operand,
        5 => old ^ operand,
        6 => !(old & operand),
        7 => signed(old).max(signed(operand)) as u64,
        8 => signed(old).min(signed(operand)) as u64,
        9 => old.max(operand),
        10 => old.min(operand),
        11 => operand,
        _ => {
            context.record_fault(cause::STORE_ACCESS_FAULT, addr);
            return RiscVAtomicResult::default();
        }
    } & mask;
    if unsafe { context.memory() }
        .write(addr, &new.to_le_bytes()[..size as usize])
        .is_err()
    {
        context.record_fault(cause::STORE_ACCESS_FAULT, addr);
        return RiscVAtomicResult::default();
    }
    fence_after(order);
    RiscVAtomicResult {
        value: old,
        access_success: 1,
    }
}

unsafe fn jit_compare_and_swap_impl(
    ctx: u64,
    addr: u64,
    expected: u64,
    new_value: u64,
    size: u64,
    order: u64,
) -> RiscVAtomicCasResult {
    let Some(context) = (unsafe { JitContext::from_abi(ctx) }) else {
        return RiscVAtomicCasResult::default();
    };
    let addr = context.normalize_addr(addr);
    if !matches!(size, 4 | 8) || addr % size != 0 || decode_order(order).is_none() {
        context.record_fault(cause::STORE_MISALIGNED, addr);
        return RiscVAtomicCasResult::default();
    }
    fence_before(order);
    let mut bytes = [0u8; 8];
    if unsafe { context.memory() }
        .read(addr, &mut bytes[..size as usize])
        .is_err()
    {
        context.record_fault(cause::LOAD_ACCESS_FAULT, addr);
        return RiscVAtomicCasResult::default();
    }
    let mask = if size == 8 {
        u64::MAX
    } else {
        u64::from(u32::MAX)
    };
    let old = u64::from_le_bytes(bytes) & mask;
    if old != expected & mask {
        fence_after(order);
        return RiscVAtomicCasResult {
            old,
            status: RiscVAtomicCasStatus::CompareFailed as u64,
        };
    }
    fence_before(order);
    if unsafe { context.memory() }
        .write(addr, &(new_value & mask).to_le_bytes()[..size as usize])
        .is_err()
    {
        context.record_fault(cause::STORE_ACCESS_FAULT, addr);
        return RiscVAtomicCasResult::default();
    }
    fence_after(order);
    RiscVAtomicCasResult {
        old,
        status: RiscVAtomicCasStatus::Swapped as u64,
    }
}

#[allow(clippy::too_many_arguments)]
unsafe fn jit_compare_and_swap_pair_impl(
    ctx: u64,
    addr: u64,
    expected_lo: u64,
    expected_hi: u64,
    new_lo: u64,
    new_hi: u64,
    order: u64,
    failure_order: u64,
    out_old_hi: *mut u64,
) -> RiscVAtomicCasResult {
    let Some(context) = (unsafe { JitContext::from_abi(ctx) }) else {
        return RiscVAtomicCasResult::default();
    };
    let addr = context.normalize_addr(addr);
    let required_failure_order = if matches!(order, 1 | 3 | 4) { 1 } else { 0 };
    if addr % 16 != 0 {
        context.record_fault(cause::STORE_MISALIGNED, addr);
        return RiscVAtomicCasResult::default();
    }
    if decode_order(order).is_none() || failure_order != required_failure_order {
        context.record_fault(cause::STORE_ACCESS_FAULT, addr);
        return RiscVAtomicCasResult::default();
    }
    if out_old_hi.is_null() {
        context.record_fault(cause::STORE_ACCESS_FAULT, addr);
        return RiscVAtomicCasResult::default();
    }
    let mut bytes = [0u8; 16];
    if unsafe { context.memory() }.read(addr, &mut bytes).is_err() {
        context.record_fault(cause::LOAD_ACCESS_FAULT, addr);
        return RiscVAtomicCasResult::default();
    }
    let old = [
        u64::from_le_bytes(bytes[..8].try_into().unwrap()),
        u64::from_le_bytes(bytes[8..].try_into().unwrap()),
    ];
    let status = if old == [expected_lo, expected_hi] {
        fence_before(order);
        bytes[..8].copy_from_slice(&new_lo.to_le_bytes());
        bytes[8..].copy_from_slice(&new_hi.to_le_bytes());
        if unsafe { context.memory() }.write(addr, &bytes).is_err() {
            context.record_fault(cause::STORE_ACCESS_FAULT, addr);
            return RiscVAtomicCasResult::default();
        }
        RiscVAtomicCasStatus::Swapped
    } else {
        fence_after(failure_order);
        RiscVAtomicCasStatus::CompareFailed
    };
    if status == RiscVAtomicCasStatus::Swapped {
        fence_after(order);
    }
    unsafe { out_old_hi.write(old[1]) };
    RiscVAtomicCasResult {
        old: old[0],
        status: status as u64,
    }
}

unsafe fn jit_load_exclusive_impl(ctx: u64, addr: u64, size: u64) -> RiscVAtomicResult {
    let Some(context) = (unsafe { JitContext::from_abi(ctx) }) else {
        return RiscVAtomicResult::default();
    };
    let addr = context.normalize_addr(addr);
    if !matches!(size, 4 | 8) || addr % size != 0 {
        context.record_fault(cause::LOAD_MISALIGNED, addr);
        return RiscVAtomicResult::default();
    }
    let mut bytes = [0u8; 8];
    if unsafe { context.memory() }
        .read(addr, &mut bytes[..size as usize])
        .is_err()
    {
        context.record_fault(cause::LOAD_ACCESS_FAULT, addr);
        return RiscVAtomicResult::default();
    }
    unsafe { *context.reservation() = Some(addr) };
    RiscVAtomicResult {
        value: u64::from_le_bytes(bytes),
        access_success: 1,
    }
}

unsafe fn jit_store_exclusive_impl(
    ctx: u64,
    addr: u64,
    value: u64,
    size: u64,
) -> RiscVAtomicResult {
    let Some(context) = (unsafe { JitContext::from_abi(ctx) }) else {
        return RiscVAtomicResult::default();
    };
    let addr = context.normalize_addr(addr);
    if !matches!(size, 4 | 8) || addr % size != 0 {
        context.record_fault(cause::STORE_MISALIGNED, addr);
        return RiscVAtomicResult::default();
    }
    let succeeds = unsafe { context.reservation().take() == Some(addr) };
    if succeeds
        && unsafe { context.memory() }
            .write(addr, &value.to_le_bytes()[..size as usize])
            .is_err()
    {
        context.record_fault(cause::STORE_ACCESS_FAULT, addr);
        return RiscVAtomicResult::default();
    }
    RiscVAtomicResult {
        value: u64::from(succeeds),
        access_success: 1,
    }
}

unsafe fn jit_clear_exclusive_impl(ctx: u64) {
    if let Some(context) = unsafe { JitContext::from_abi(ctx) } {
        unsafe { *context.reservation() = None };
    }
}

fn int_crypto_op(code: u64) -> Option<Op> {
    Some(match code {
        0 => Op::Clmul,
        1 => Op::Clmulh,
        2 => Op::Clmulr,
        3 => Op::Xperm4,
        4 => Op::Xperm8,
        5 => Op::Sha512Sig0l,
        6 => Op::Sha512Sig0h,
        7 => Op::Sha512Sig1l,
        8 => Op::Sha512Sig1h,
        9 => Op::Sha512Sum0r,
        10 => Op::Sha512Sum1r,
        11 => Op::Sm4ed,
        12 => Op::Sm4ks,
        13 => Op::Aes32esi,
        14 => Op::Aes32esmi,
        15 => Op::Aes32dsi,
        16 => Op::Aes32dsmi,
        17 => Op::Aes64es,
        18 => Op::Aes64esm,
        19 => Op::Aes64ds,
        20 => Op::Aes64dsm,
        21 => Op::Aes64im,
        22 => Op::Aes64ks1i,
        23 => Op::Aes64ks2,
        _ => return None,
    })
}

unsafe fn jit_int_crypto_impl(op_code: u64, src1: u64, src2: u64, imm: u64, xlen: u64) -> u64 {
    let Some(op) = int_crypto_op(op_code) else {
        return 0;
    };
    let Ok(xlen) = u32::try_from(xlen) else {
        return 0;
    };
    super::super::crypto::eval_int_crypto(op, src1, src2, imm as u8, xlen).unwrap_or(0)
}

unsafe fn jit_scalar_fp_impl(
    op_code: u64,
    rm_field: u64,
    fcsr: u64,
    a: u64,
    b: u64,
    c: u64,
) -> RiscVFpResult {
    let Some(op_code) = RiscVFpOpCode::from_code(op_code) else {
        return RiscVFpResult {
            value: 0,
            fcsr_status: RISCV_FP_RESULT_INVALID,
        };
    };
    let Some((value, new_fcsr)) = super::super::float::eval_scalar_fp(
        op_code.into_op(),
        rm_field as u8,
        fcsr as u32,
        a,
        b,
        c,
    ) else {
        return RiscVFpResult {
            value: 0,
            fcsr_status: RISCV_FP_RESULT_INVALID,
        };
    };
    RiscVFpResult {
        value,
        fcsr_status: u64::from(new_fcsr),
    }
}

unsafe fn jit_vector_unsupported_impl(_state: *mut RiscVGuestRegs, _insn: u64, _xlen: u64) -> u64 {
    0
}

// The x86-64 code generator intentionally uses the SysV ABI even on targets
// whose platform C ABI differs. AArch64 uses AAPCS64, which is its C ABI.
// Keep one semantic implementation per helper and expose architecture-correct
// entry wrappers to generated code.
macro_rules! define_jit_abi {
    ($name:ident, $implementation:ident, ($($arg:ident: $ty:ty),* $(,)?) -> $ret:ty) => {
        #[cfg(target_arch = "x86_64")]
        unsafe extern "sysv64" fn $name($($arg: $ty),*) -> $ret {
            unsafe { $implementation($($arg),*) }
        }

        #[cfg(target_arch = "aarch64")]
        unsafe extern "C" fn $name($($arg: $ty),*) -> $ret {
            unsafe { $implementation($($arg),*) }
        }
    };
}

define_jit_abi!(jit_load, jit_load_impl, (ctx: u64, addr: u64, size: u64, signed: u64) -> RiscVLoadResult);
define_jit_abi!(jit_store, jit_store_impl, (ctx: u64, addr: u64, value: u64, size: u64) -> u64);
define_jit_abi!(jit_atomic_rmw, jit_atomic_rmw_impl, (ctx: u64, addr: u64, operand: u64, size: u64, op: u64, order: u64) -> RiscVAtomicResult);
define_jit_abi!(jit_compare_and_swap, jit_compare_and_swap_impl, (ctx: u64, addr: u64, expected: u64, new_value: u64, size: u64, order: u64) -> RiscVAtomicCasResult);
define_jit_abi!(jit_compare_and_swap_pair, jit_compare_and_swap_pair_impl, (ctx: u64, addr: u64, expected_lo: u64, expected_hi: u64, new_lo: u64, new_hi: u64, order: u64, failure_order: u64, out_old_hi: *mut u64) -> RiscVAtomicCasResult);
define_jit_abi!(jit_load_exclusive, jit_load_exclusive_impl, (ctx: u64, addr: u64, size: u64) -> RiscVAtomicResult);
define_jit_abi!(jit_store_exclusive, jit_store_exclusive_impl, (ctx: u64, addr: u64, value: u64, size: u64) -> RiscVAtomicResult);
define_jit_abi!(jit_clear_exclusive, jit_clear_exclusive_impl, (ctx: u64) -> ());
define_jit_abi!(jit_int_crypto, jit_int_crypto_impl, (op_code: u64, src1: u64, src2: u64, imm: u64, xlen: u64) -> u64);
define_jit_abi!(jit_scalar_fp, jit_scalar_fp_impl, (op_code: u64, rm_field: u64, fcsr: u64, a: u64, b: u64, c: u64) -> RiscVFpResult);
define_jit_abi!(jit_vector_unsupported, jit_vector_unsupported_impl, (state: *mut RiscVGuestRegs, insn: u64, xlen: u64) -> u64);
