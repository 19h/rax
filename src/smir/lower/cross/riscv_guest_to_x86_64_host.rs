//! State-backed RISC-V-to-x86-64 SMIR lowerer.
//!
//! Lowered code uses the `extern "sysv64" fn(*mut RiscVGuestRegs)` ABI. RDI
//! remains the persistent guest-state pointer, architectural registers are
//! loaded/stored through that state, and SSA temporaries occupy stack slots.
//! This is intentionally separate from [`crate::smir::lower::x86_64`], whose
//! execution ABI identity-maps an x86 guest register file onto the host GPRs.

use std::collections::{HashMap, HashSet};

use crate::isa::riscv::Op as RvOp;
use crate::smir::ir::flags::FlagUpdate;
use crate::smir::ir::ops::{OpKind, SmirOp};
use crate::smir::ir::types::{
    Address, ArchReg, AtomicOp, BlockId, Condition, MemWidth, MemoryOrder, OpWidth, RiscVReg,
    ShiftOp, SignExtend, SrcOperand, VReg, VirtualId,
};
use crate::smir::ir::{CallTarget, SmirBlock, SmirFunction, Terminator, TrapKind};
use crate::smir::lower::cross::riscv_x86_64_abi::{
    RISCV_FP_RESULT_INVALID, RiscVAtomicOpCode, RiscVFpOpCode, RiscVIntCryptoOpCode,
    RiscVMemoryOrderCode,
};
use crate::smir::lower::regalloc::PhysReg;
use crate::smir::lower::x86_64::{X86Cond, X86Emitter};
use crate::smir::lower::{CodeBuffer, LowerError, LowerResult, RelocKind, Relocation, SmirLowerer};

const STATE: PhysReg = PhysReg::Rdi;
const ACC: PhysReg = PhysReg::Rax;
const HI: PhysReg = PhysReg::Rdx;
const RHS: PhysReg = PhysReg::Rcx;
const TMP0: PhysReg = PhysReg::R8;
const TMP1: PhysReg = PhysReg::R9;
const TMP2: PhysReg = PhysReg::R10;
const TARGET: PhysReg = PhysReg::R11;
const ADDR: PhysReg = PhysReg::Rsi;

// Keep these in lockstep with lower::runtime::RiscVGuestRegs.  All fields are
// eight-byte quantities so the code generator can use one scalar addressing
// convention and the ABI is stable across RV32/RV64 guests.
const RV_X_OFFSET: i32 = 0;
const RV_F_OFFSET: i32 = 32 * 8;
const RV_PC_OFFSET: i32 = RV_F_OFFSET + 32 * 8;
const RV_FCSR_OFFSET: i32 = RV_PC_OFFSET + 8;
const RV_EXIT_REASON_OFFSET: i32 = RV_FCSR_OFFSET + 8;
const RV_CTX_OFFSET: i32 = RV_EXIT_REASON_OFFSET + 8;
const RV_LOAD_FN_OFFSET: i32 = RV_CTX_OFFSET + 8;
const RV_STORE_FN_OFFSET: i32 = RV_LOAD_FN_OFFSET + 8;
const RV_ATOMIC_RMW_FN_OFFSET: i32 = RV_STORE_FN_OFFSET + 8;
const RV_CAS_FN_OFFSET: i32 = RV_ATOMIC_RMW_FN_OFFSET + 8;
const RV_LOAD_EXCLUSIVE_FN_OFFSET: i32 = RV_CAS_FN_OFFSET + 8;
const RV_STORE_EXCLUSIVE_FN_OFFSET: i32 = RV_LOAD_EXCLUSIVE_FN_OFFSET + 8;
const RV_CLEAR_EXCLUSIVE_FN_OFFSET: i32 = RV_STORE_EXCLUSIVE_FN_OFFSET + 8;
const RV_INT_CRYPTO_FN_OFFSET: i32 = RV_CLEAR_EXCLUSIVE_FN_OFFSET + 8;
const RV_FP_FN_OFFSET: i32 = RV_INT_CRYPTO_FN_OFFSET + 8;
const RV_V_OFFSET: i32 = RV_FP_FN_OFFSET + 8;
const RV_VL_OFFSET: i32 = RV_V_OFFSET + 32 * 16;
const RV_VTYPE_OFFSET: i32 = RV_VL_OFFSET + 8;
const RV_VSTART_OFFSET: i32 = RV_VTYPE_OFFSET + 8;
const RV_VCSR_OFFSET: i32 = RV_VSTART_OFFSET + 8;
const RV_VECTOR_FN_OFFSET: i32 = RV_VCSR_OFFSET + 8;

const EXIT_RETURN: i64 = 0;
const EXIT_TRAP: i64 = 1;
const EXIT_SYSCALL: i64 = 2;
const EXIT_BREAKPOINT: i64 = 3;

#[derive(Clone, Copy)]
enum BinOp {
    Add,
    Sub,
    And,
    Or,
    Xor,
}

/// Lower scalar RISC-V SMIR to x86-64 using an explicit state pointer.
pub struct RiscVX86_64Lowerer {
    code: CodeBuffer,
    block_offsets: HashMap<BlockId, usize>,
    pending_jumps: Vec<(usize, BlockId, RelocKind)>,
    virtual_slots: HashMap<VirtualId, i32>,
    frame_size: usize,
    relocations: Vec<Relocation>,
    /// Optional exact PC for a block that returns to the dispatcher.  This is
    /// required for compressed-instruction blocks because operation metadata
    /// records the instruction start but not its byte length.
    return_pcs: HashMap<BlockId, u64>,
}

impl RiscVX86_64Lowerer {
    pub fn new() -> Self {
        Self {
            code: CodeBuffer::with_capacity(4096),
            block_offsets: HashMap::new(),
            pending_jumps: Vec::new(),
            virtual_slots: HashMap::new(),
            frame_size: 0,
            relocations: Vec::new(),
            return_pcs: HashMap::new(),
        }
    }

    /// Set exact dispatcher resume PCs for blocks whose terminator returns.
    pub fn set_return_pcs(&mut self, pcs: HashMap<BlockId, u64>) {
        self.return_pcs = pcs;
    }

    fn collect_virtuals(&mut self, func: &SmirFunction) {
        let mut ids = HashSet::new();
        for block in &func.blocks {
            for phi in &block.phis {
                if let VReg::Virtual(id) = phi.dst {
                    ids.insert(id);
                }
                for (_, src) in &phi.sources {
                    if let VReg::Virtual(id) = *src {
                        ids.insert(id);
                    }
                }
            }
            for op in &block.ops {
                for reg in op.kind.dests().into_iter().chain(op.kind.source_vregs()) {
                    if let VReg::Virtual(id) = reg {
                        ids.insert(id);
                    }
                }
            }
            match &block.terminator {
                Terminator::CondBranch { cond, .. }
                | Terminator::IndirectBranch { target: cond, .. } => {
                    if let VReg::Virtual(id) = *cond {
                        ids.insert(id);
                    }
                }
                Terminator::Call { target, .. } | Terminator::TailCall { target, .. } => {
                    if let CallTarget::Indirect(reg) = target {
                        if let VReg::Virtual(id) = *reg {
                            ids.insert(id);
                        }
                    }
                }
                Terminator::Return { values } => {
                    for value in values {
                        if let VReg::Virtual(id) = *value {
                            ids.insert(id);
                        }
                    }
                }
                _ => {}
            }
        }

        let mut ids: Vec<_> = ids.into_iter().collect();
        ids.sort_by_key(|id| id.0);
        self.virtual_slots.clear();
        for (index, id) in ids.into_iter().enumerate() {
            self.virtual_slots.insert(id, -8 * (index as i32 + 1));
        }
        self.frame_size = align16(self.virtual_slots.len() * 8);
    }

    fn virtual_slot(&self, id: VirtualId) -> Result<i32, LowerError> {
        self.virtual_slots
            .get(&id)
            .copied()
            .ok_or_else(|| LowerError::RegisterAllocationFailed {
                reason: format!("missing stack slot for virtual {id:?}"),
            })
    }

    fn emit_prologue(&mut self) {
        let mut e = X86Emitter::new(&mut self.code);
        e.emit_push(PhysReg::Rbp);
        e.emit_mov_rr(PhysReg::Rbp, PhysReg::Rsp, OpWidth::W64);
        if self.frame_size != 0 {
            e.emit_sub_ri(PhysReg::Rsp, self.frame_size as i64, OpWidth::W64);
        }
        // Canonicalize the externally visible backing slot as well as
        // hard-wiring generated x0 reads and discarding generated writes.
        e.emit_xor_rr(ACC, ACC, OpWidth::W32);
        e.emit_mov_mr(STATE, RV_X_OFFSET, ACC, OpWidth::W64);
    }

    fn emit_epilogue(&mut self) {
        let mut e = X86Emitter::new(&mut self.code);
        e.emit_mov_rr(PhysReg::Rsp, PhysReg::Rbp, OpWidth::W64);
        e.emit_pop(PhysReg::Rbp);
        e.emit_ret();
    }

    fn emit_mov_imm(&mut self, dst: PhysReg, value: i64, width: OpWidth) {
        let mut e = X86Emitter::new(&mut self.code);
        if width == OpWidth::W64 {
            e.emit_mov_ri_imm64(dst, value);
        } else {
            e.emit_mov_ri(dst, value, width);
        }
    }

    fn arch_offset(reg: RiscVReg) -> Result<Option<i32>, LowerError> {
        match reg {
            RiscVReg::X(0) => Ok(None),
            RiscVReg::X(n @ 1..=31) => Ok(Some(RV_X_OFFSET + i32::from(n) * 8)),
            RiscVReg::F(n @ 0..=31) => Ok(Some(RV_F_OFFSET + i32::from(n) * 8)),
            RiscVReg::Pc => Ok(Some(RV_PC_OFFSET)),
            RiscVReg::Csr(0x003) => Ok(Some(RV_FCSR_OFFSET)),
            RiscVReg::Csr(0xc20) => Ok(Some(RV_VL_OFFSET)),
            RiscVReg::Csr(0xc21) => Ok(Some(RV_VTYPE_OFFSET)),
            RiscVReg::Csr(0x008) => Ok(Some(RV_VSTART_OFFSET)),
            RiscVReg::Csr(0x00f) => Ok(Some(RV_VCSR_OFFSET)),
            other => Err(LowerError::InvalidRegister(format!(
                "unsupported state-backed RISC-V register {other:?}"
            ))),
        }
    }

    fn load_vreg_to(&mut self, reg: VReg, dst: PhysReg, width: OpWidth) -> Result<(), LowerError> {
        match reg {
            VReg::Imm(value) => self.emit_mov_imm(dst, value, width),
            VReg::Virtual(id) => {
                let offset = self.virtual_slot(id)?;
                let mut e = X86Emitter::new(&mut self.code);
                e.emit_mov_rm(dst, PhysReg::Rbp, offset, OpWidth::W64);
            }
            VReg::Arch(ArchReg::RiscV(rv)) => {
                if let Some(offset) = Self::arch_offset(rv)? {
                    let mut e = X86Emitter::new(&mut self.code);
                    e.emit_mov_rm(dst, STATE, offset, OpWidth::W64);
                } else {
                    let mut e = X86Emitter::new(&mut self.code);
                    e.emit_xor_rr(dst, dst, OpWidth::W32);
                }
            }
            VReg::Arch(other) => {
                return Err(LowerError::InvalidRegister(format!(
                    "non-RISC-V register in RISC-V lowerer: {other:?}"
                )));
            }
        }
        Ok(())
    }

    fn load_src_to(
        &mut self,
        src: &SrcOperand,
        dst: PhysReg,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        match src {
            SrcOperand::Reg(reg) => self.load_vreg_to(*reg, dst, width),
            SrcOperand::Imm(value) | SrcOperand::Imm64(value) => {
                self.emit_mov_imm(dst, *value, width);
                Ok(())
            }
            other => Err(LowerError::UnsupportedOp {
                op: format!("RISC-V scalar source {other:?}"),
            }),
        }
    }

    fn normalize_scalar(&mut self, reg: PhysReg, width: OpWidth) -> Result<(), LowerError> {
        let mut e = X86Emitter::new(&mut self.code);
        match width {
            OpWidth::W8 | OpWidth::W16 => e.emit_movzx(reg, reg, width, OpWidth::W64),
            OpWidth::W32 => e.emit_mov_rr(reg, reg, OpWidth::W32),
            OpWidth::W64 => {}
            OpWidth::W128 => {
                return Err(LowerError::UnsupportedOp {
                    op: "RISC-V scalar W128 result".into(),
                });
            }
        }
        Ok(())
    }

    fn store_reg_to(&mut self, dst: VReg, src: PhysReg, width: OpWidth) -> Result<(), LowerError> {
        self.normalize_scalar(src, width)?;
        match dst {
            VReg::Imm(_) => {}
            VReg::Virtual(id) => {
                let offset = self.virtual_slot(id)?;
                let mut e = X86Emitter::new(&mut self.code);
                e.emit_mov_mr(PhysReg::Rbp, offset, src, OpWidth::W64);
            }
            VReg::Arch(ArchReg::RiscV(rv)) => {
                if let Some(offset) = Self::arch_offset(rv)? {
                    let mut e = X86Emitter::new(&mut self.code);
                    e.emit_mov_mr(STATE, offset, src, OpWidth::W64);
                }
            }
            VReg::Arch(other) => {
                return Err(LowerError::InvalidRegister(format!(
                    "non-RISC-V destination in RISC-V lowerer: {other:?}"
                )));
            }
        }
        Ok(())
    }

    fn require_no_flags(flags: FlagUpdate, op: &'static str) -> Result<(), LowerError> {
        if flags == FlagUpdate::None {
            Ok(())
        } else {
            Err(LowerError::UnsupportedOp {
                op: format!("RISC-V {op} requests non-architectural flags {flags:?}"),
            })
        }
    }

    fn lower_binop(
        &mut self,
        dst: VReg,
        src1: VReg,
        src2: &SrcOperand,
        width: OpWidth,
        flags: FlagUpdate,
        op: BinOp,
    ) -> Result<(), LowerError> {
        Self::require_no_flags(flags, "integer binop")?;
        self.load_vreg_to(src1, ACC, width)?;
        self.load_src_to(src2, RHS, width)?;
        {
            let mut e = X86Emitter::new(&mut self.code);
            match op {
                BinOp::Add => e.emit_add_rr(ACC, RHS, width),
                BinOp::Sub => e.emit_sub_rr(ACC, RHS, width),
                BinOp::And => e.emit_and_rr(ACC, RHS, width),
                BinOp::Or => e.emit_or_rr(ACC, RHS, width),
                BinOp::Xor => e.emit_xor_rr(ACC, RHS, width),
            }
        }
        self.store_reg_to(dst, ACC, width)
    }

    fn emit_shift(
        &mut self,
        reg: PhysReg,
        amount: &SrcOperand,
        width: OpWidth,
        kind: ShiftOp,
        left_rotate: bool,
    ) -> Result<(), LowerError> {
        match amount {
            SrcOperand::Imm(value) | SrcOperand::Imm64(value) => {
                let amount = *value as u8;
                if amount == 0 {
                    return Ok(());
                }
                let mut e = X86Emitter::new(&mut self.code);
                match (kind, left_rotate) {
                    (ShiftOp::Lsl, false) => e.emit_shl_ri(reg, amount, width),
                    (ShiftOp::Lsr, false) => e.emit_shr_ri(reg, amount, width),
                    (ShiftOp::Asr, false) => e.emit_sar_ri(reg, amount, width),
                    (ShiftOp::Ror, false) => e.emit_ror_ri(reg, amount, width),
                    (ShiftOp::Ror, true) => e.emit_rol_ri(reg, amount, width),
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: format!("RISC-V shift {kind:?}"),
                        });
                    }
                }
            }
            SrcOperand::Reg(amount) => {
                self.load_vreg_to(*amount, RHS, width)?;
                let mut e = X86Emitter::new(&mut self.code);
                match (kind, left_rotate) {
                    (ShiftOp::Lsl, false) => e.emit_shl_cl(reg, width),
                    (ShiftOp::Lsr, false) => e.emit_shr_cl(reg, width),
                    (ShiftOp::Asr, false) => e.emit_sar_cl(reg, width),
                    (ShiftOp::Ror, false) => e.emit_ror_cl(reg, width),
                    (ShiftOp::Ror, true) => e.emit_rol_cl(reg, width),
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: format!("RISC-V shift {kind:?}"),
                        });
                    }
                }
            }
            other => {
                return Err(LowerError::UnsupportedOp {
                    op: format!("RISC-V shift amount {other:?}"),
                });
            }
        }
        Ok(())
    }

    fn lower_shift(
        &mut self,
        dst: VReg,
        src: VReg,
        amount: &SrcOperand,
        width: OpWidth,
        flags: FlagUpdate,
        kind: ShiftOp,
        left_rotate: bool,
    ) -> Result<(), LowerError> {
        Self::require_no_flags(flags, "shift")?;
        self.load_vreg_to(src, ACC, width)?;
        self.emit_shift(ACC, amount, width, kind, left_rotate)?;
        self.store_reg_to(dst, ACC, width)
    }

    fn lower_mul(
        &mut self,
        dst_lo: VReg,
        dst_hi: Option<VReg>,
        src1: VReg,
        src2: &SrcOperand,
        width: OpWidth,
        flags: FlagUpdate,
        signed: bool,
    ) -> Result<(), LowerError> {
        Self::require_no_flags(flags, if signed { "MulS" } else { "MulU" })?;
        if !matches!(width, OpWidth::W32 | OpWidth::W64) {
            return Err(LowerError::UnsupportedOp {
                op: format!("RISC-V multiply width {width:?}"),
            });
        }
        self.load_vreg_to(src1, ACC, width)?;
        self.load_src_to(src2, RHS, width)?;
        {
            let mut e = X86Emitter::new(&mut self.code);
            if signed {
                e.emit_imul(RHS, width);
            } else {
                e.emit_mul(RHS, width);
            }
        }
        self.store_reg_to(dst_lo, ACC, width)?;
        if let Some(dst_hi) = dst_hi {
            self.store_reg_to(dst_hi, HI, width)?;
        }
        Ok(())
    }

    fn emit_jcc_placeholder(&mut self, cond: X86Cond) -> usize {
        let start = self.code.position();
        let mut e = X86Emitter::new(&mut self.code);
        e.emit_jcc_rel32(cond, 0);
        start + 2
    }

    fn emit_jmp_placeholder(&mut self) -> usize {
        let start = self.code.position();
        let mut e = X86Emitter::new(&mut self.code);
        e.emit_jmp_rel32(0);
        start + 1
    }

    fn patch_rel32_to_current(&mut self, offset: usize) -> Result<(), LowerError> {
        let target = self.code.position();
        let rel = target as i64 - offset as i64 - 4;
        if !(i32::MIN as i64..=i32::MAX as i64).contains(&rel) {
            return Err(LowerError::RelocationOutOfRange { offset, target });
        }
        self.code.patch_i32(offset, rel as i32);
        Ok(())
    }

    fn lower_div(
        &mut self,
        quot: VReg,
        rem: Option<VReg>,
        src1: VReg,
        src2: &SrcOperand,
        width: OpWidth,
        flags: FlagUpdate,
        signed: bool,
    ) -> Result<(), LowerError> {
        Self::require_no_flags(flags, if signed { "DivS" } else { "DivU" })?;
        if !matches!(width, OpWidth::W32 | OpWidth::W64) {
            return Err(LowerError::UnsupportedOp {
                op: format!("RISC-V divide width {width:?}"),
            });
        }

        // SMIR division is made total here.  The RISC-V lifter already guards
        // zero and MIN/-1 with Select, but keeping the lowerer total prevents a
        // malformed/manual SMIR block from raising host #DE.
        self.load_src_to(src2, RHS, width)?;
        {
            let mut e = X86Emitter::new(&mut self.code);
            e.emit_test_rr(RHS, RHS, width);
        }
        let nonzero = self.emit_jcc_placeholder(X86Cond::Ne);
        self.emit_mov_imm(ACC, 0, width);
        self.emit_mov_imm(HI, 0, width);
        let done = self.emit_jmp_placeholder();

        self.patch_rel32_to_current(nonzero)?;
        self.load_vreg_to(src1, ACC, width)?;
        if signed {
            let min = if width == OpWidth::W32 {
                i64::from(i32::MIN)
            } else {
                i64::MIN
            };
            self.emit_mov_imm(TMP0, min, width);
            {
                let mut e = X86Emitter::new(&mut self.code);
                e.emit_cmp_rr(ACC, TMP0, width);
            }
            let normal = self.emit_jcc_placeholder(X86Cond::Ne);
            {
                let mut e = X86Emitter::new(&mut self.code);
                e.emit_cmp_ri(RHS, -1, width);
            }
            let normal_from_rhs = self.emit_jcc_placeholder(X86Cond::Ne);
            self.emit_mov_imm(HI, 0, width);
            let overflow_done = self.emit_jmp_placeholder();
            self.patch_rel32_to_current(normal)?;
            self.patch_rel32_to_current(normal_from_rhs)?;
            {
                let mut e = X86Emitter::new(&mut self.code);
                if width == OpWidth::W32 {
                    e.emit_cdq();
                } else {
                    e.emit_cqo();
                }
                e.emit_idiv(RHS, width);
            }
            self.patch_rel32_to_current(overflow_done)?;
        } else {
            let mut e = X86Emitter::new(&mut self.code);
            e.emit_zero_rdx();
            e.emit_div(RHS, width);
        }
        self.patch_rel32_to_current(done)?;
        self.store_reg_to(quot, ACC, width)?;
        if let Some(rem) = rem {
            self.store_reg_to(rem, HI, width)?;
        }
        Ok(())
    }

    fn lower_clz(&mut self, dst: VReg, src: VReg, width: OpWidth) -> Result<(), LowerError> {
        if !matches!(width, OpWidth::W32 | OpWidth::W64) {
            return Err(LowerError::UnsupportedOp {
                op: format!("RISC-V CLZ width {width:?}"),
            });
        }
        self.load_vreg_to(src, ACC, width)?;
        {
            let mut e = X86Emitter::new(&mut self.code);
            e.emit_test_rr(ACC, ACC, width);
        }
        let nonzero = self.emit_jcc_placeholder(X86Cond::Ne);
        self.emit_mov_imm(ACC, i64::from(width.bits()), width);
        let done = self.emit_jmp_placeholder();
        self.patch_rel32_to_current(nonzero)?;
        {
            let mut e = X86Emitter::new(&mut self.code);
            e.emit_bsr(ACC, ACC, width);
        }
        self.emit_mov_imm(TMP0, i64::from(width.bits() - 1), width);
        {
            let mut e = X86Emitter::new(&mut self.code);
            e.emit_sub_rr(TMP0, ACC, width);
            e.emit_mov_rr(ACC, TMP0, width);
        }
        self.patch_rel32_to_current(done)?;
        self.store_reg_to(dst, ACC, width)
    }

    fn lower_ctz(&mut self, dst: VReg, src: VReg, width: OpWidth) -> Result<(), LowerError> {
        if !matches!(width, OpWidth::W32 | OpWidth::W64) {
            return Err(LowerError::UnsupportedOp {
                op: format!("RISC-V CTZ width {width:?}"),
            });
        }
        self.load_vreg_to(src, ACC, width)?;
        {
            let mut e = X86Emitter::new(&mut self.code);
            e.emit_test_rr(ACC, ACC, width);
        }
        let nonzero = self.emit_jcc_placeholder(X86Cond::Ne);
        self.emit_mov_imm(ACC, i64::from(width.bits()), width);
        let done = self.emit_jmp_placeholder();
        self.patch_rel32_to_current(nonzero)?;
        {
            let mut e = X86Emitter::new(&mut self.code);
            e.emit_bsf(ACC, ACC, width);
        }
        self.patch_rel32_to_current(done)?;
        self.store_reg_to(dst, ACC, width)
    }

    fn lower_popcnt(&mut self, dst: VReg, src: VReg, width: OpWidth) -> Result<(), LowerError> {
        if !matches!(width, OpWidth::W32 | OpWidth::W64) {
            return Err(LowerError::UnsupportedOp {
                op: format!("RISC-V POPCNT width {width:?}"),
            });
        }

        // SWAR population count keeps the baseline x86-64 contract and avoids
        // executing POPCNT on hosts that do not advertise the extension.
        self.load_vreg_to(src, ACC, width)?;
        {
            let mut e = X86Emitter::new(&mut self.code);

            // x -= (x >> 1) & 0x5555...
            e.emit_mov_rr(TMP0, ACC, width);
            e.emit_shr_ri(TMP0, 1, width);
        }
        self.emit_mov_imm(TMP1, 0x5555_5555_5555_5555, width);
        {
            let mut e = X86Emitter::new(&mut self.code);
            e.emit_and_rr(TMP0, TMP1, width);
            e.emit_sub_rr(ACC, TMP0, width);

            // x = (x & 0x3333...) + ((x >> 2) & 0x3333...)
            e.emit_mov_rr(TMP0, ACC, width);
            e.emit_shr_ri(TMP0, 2, width);
        }
        self.emit_mov_imm(TMP1, 0x3333_3333_3333_3333, width);
        {
            let mut e = X86Emitter::new(&mut self.code);
            e.emit_and_rr(ACC, TMP1, width);
            e.emit_and_rr(TMP0, TMP1, width);
            e.emit_add_rr(ACC, TMP0, width);

            // x = (x + (x >> 4)) & 0x0f0f...
            e.emit_mov_rr(TMP0, ACC, width);
            e.emit_shr_ri(TMP0, 4, width);
            e.emit_add_rr(ACC, TMP0, width);
        }
        self.emit_mov_imm(TMP1, 0x0f0f_0f0f_0f0f_0f0f, width);
        {
            let mut e = X86Emitter::new(&mut self.code);
            e.emit_and_rr(ACC, TMP1, width);

            for shift in [8, 16] {
                e.emit_mov_rr(TMP0, ACC, width);
                e.emit_shr_ri(TMP0, shift, width);
                e.emit_add_rr(ACC, TMP0, width);
            }
            if width == OpWidth::W64 {
                e.emit_mov_rr(TMP0, ACC, width);
                e.emit_shr_ri(TMP0, 32, width);
                e.emit_add_rr(ACC, TMP0, width);
            }
            e.emit_and_ri(ACC, if width == OpWidth::W32 { 0x3f } else { 0x7f }, width);
        }
        self.store_reg_to(dst, ACC, width)
    }

    fn lower_rbit(&mut self, dst: VReg, src: VReg, width: OpWidth) -> Result<(), LowerError> {
        if !matches!(width, OpWidth::W32 | OpWidth::W64) {
            return Err(LowerError::UnsupportedOp {
                op: format!("RISC-V RBIT width {width:?}"),
            });
        }
        self.load_vreg_to(src, ACC, width)?;
        let masks: &[(u8, u64)] = if width == OpWidth::W32 {
            &[(1, 0x5555_5555), (2, 0x3333_3333), (4, 0x0f0f_0f0f)]
        } else {
            &[
                (1, 0x5555_5555_5555_5555),
                (2, 0x3333_3333_3333_3333),
                (4, 0x0f0f_0f0f_0f0f_0f0f),
            ]
        };
        for &(shift, mask) in masks {
            {
                let mut e = X86Emitter::new(&mut self.code);
                e.emit_mov_rr(TMP0, ACC, width);
                e.emit_shr_ri(TMP0, shift, width);
            }
            self.emit_mov_imm(TMP1, mask as i64, width);
            {
                let mut e = X86Emitter::new(&mut self.code);
                e.emit_and_rr(TMP0, TMP1, width);
                e.emit_and_rr(ACC, TMP1, width);
                e.emit_shl_ri(ACC, shift, width);
                e.emit_or_rr(ACC, TMP0, width);
            }
        }
        {
            let mut e = X86Emitter::new(&mut self.code);
            e.emit_bswap(ACC, width);
        }
        self.store_reg_to(dst, ACC, width)
    }

    fn scalar_mem_width(width: MemWidth) -> Result<(OpWidth, i64), LowerError> {
        match width {
            MemWidth::B1 => Ok((OpWidth::W8, 1)),
            MemWidth::B2 => Ok((OpWidth::W16, 2)),
            MemWidth::B4 => Ok((OpWidth::W32, 4)),
            MemWidth::B8 => Ok((OpWidth::W64, 8)),
            _ => Err(LowerError::UnsupportedOp {
                op: format!("RISC-V scalar memory width {width:?}"),
            }),
        }
    }

    fn add_i64_to_reg(&mut self, reg: PhysReg, value: i64) {
        if value == 0 {
            return;
        }
        if i32::try_from(value).is_ok() {
            let mut e = X86Emitter::new(&mut self.code);
            e.emit_add_ri(reg, value, OpWidth::W64);
        } else {
            self.emit_mov_imm(TMP2, value, OpWidth::W64);
            let mut e = X86Emitter::new(&mut self.code);
            e.emit_add_rr(reg, TMP2, OpWidth::W64);
        }
    }

    fn load_addr_to(&mut self, addr: &Address, dst: PhysReg) -> Result<(), LowerError> {
        match addr {
            Address::Direct(base) => self.load_vreg_to(*base, dst, OpWidth::W64)?,
            Address::BaseOffset { base, offset, .. } => {
                self.load_vreg_to(*base, dst, OpWidth::W64)?;
                self.add_i64_to_reg(dst, *offset);
            }
            Address::BaseIndexScale {
                base,
                index,
                scale,
                disp,
                ..
            } => {
                if let Some(base) = base {
                    self.load_vreg_to(*base, dst, OpWidth::W64)?;
                } else {
                    let mut e = X86Emitter::new(&mut self.code);
                    e.emit_xor_rr(dst, dst, OpWidth::W32);
                }
                self.load_vreg_to(*index, TMP2, OpWidth::W64)?;
                match scale {
                    1 => {}
                    2 | 4 | 8 => {
                        let mut e = X86Emitter::new(&mut self.code);
                        e.emit_shl_ri(TMP2, scale.trailing_zeros() as u8, OpWidth::W64);
                    }
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: format!("RISC-V memory scale {scale}"),
                        });
                    }
                }
                {
                    let mut e = X86Emitter::new(&mut self.code);
                    e.emit_add_rr(dst, TMP2, OpWidth::W64);
                }
                self.add_i64_to_reg(dst, i64::from(*disp));
            }
            Address::Absolute(value) => self.emit_mov_imm(dst, *value as i64, OpWidth::W64),
            Address::PcRel { offset, base, .. } => {
                let value = base.unwrap_or(0).wrapping_add(*offset as u64);
                self.emit_mov_imm(dst, value as i64, OpWidth::W64);
            }
            other => {
                return Err(LowerError::UnsupportedOp {
                    op: format!("RISC-V memory address {other:?}"),
                });
            }
        }
        Ok(())
    }

    fn emit_mem_helper_call(&mut self, target: PhysReg) {
        let mut e = X86Emitter::new(&mut self.code);
        // After the frame prologue RSP is 16-byte aligned.  Preserve the
        // caller-saved state pointer and add one padding word before CALL.
        e.emit_push(STATE);
        e.emit_sub_ri(PhysReg::Rsp, 8, OpWidth::W64);
        e.emit_mov_rm(STATE, STATE, RV_CTX_OFFSET, OpWidth::W64);
        e.emit_call_reg(target);
        e.emit_add_ri(PhysReg::Rsp, 8, OpWidth::W64);
        e.emit_pop(STATE);
    }

    fn emit_trap_unless_one(&mut self, status: PhysReg, guest_pc: u64) -> Result<(), LowerError> {
        {
            let mut e = X86Emitter::new(&mut self.code);
            e.emit_cmp_ri(status, 1, OpWidth::W64);
        }
        let success = self.emit_jcc_placeholder(X86Cond::E);
        self.emit_arch_exit(guest_pc, EXIT_TRAP);
        self.patch_rel32_to_current(success)
    }

    fn lower_load(
        &mut self,
        dst: VReg,
        addr: &Address,
        width: MemWidth,
        sign: SignExtend,
        guest_pc: u64,
    ) -> Result<(), LowerError> {
        let (_, size) = Self::scalar_mem_width(width)?;
        self.load_addr_to(addr, ADDR)?;
        {
            let mut e = X86Emitter::new(&mut self.code);
            e.emit_mov_rm(TARGET, STATE, RV_LOAD_FN_OFFSET, OpWidth::W64);
            e.emit_mov_ri(HI, size, OpWidth::W64);
            e.emit_mov_ri(
                RHS,
                i64::from(matches!(sign, SignExtend::Sign)),
                OpWidth::W64,
            );
        }
        self.emit_mem_helper_call(TARGET);
        // The two-u64 SysV result is RAX=value, RDX=success. A fault exits
        // before the load destination is committed.
        self.emit_trap_unless_one(HI, guest_pc)?;
        self.store_reg_to(dst, ACC, OpWidth::W64)
    }

    fn lower_store(
        &mut self,
        src: VReg,
        addr: &Address,
        width: MemWidth,
        guest_pc: u64,
    ) -> Result<(), LowerError> {
        let (op_width, size) = Self::scalar_mem_width(width)?;
        self.load_addr_to(addr, ADDR)?;
        self.load_vreg_to(src, HI, op_width)?;
        {
            let mut e = X86Emitter::new(&mut self.code);
            e.emit_mov_rm(TARGET, STATE, RV_STORE_FN_OFFSET, OpWidth::W64);
            e.emit_mov_ri(RHS, size, OpWidth::W64);
        }
        self.emit_mem_helper_call(TARGET);
        self.emit_trap_unless_one(ACC, guest_pc)
    }

    fn atomic_op_code(op: AtomicOp) -> i64 {
        match op {
            AtomicOp::Add => RiscVAtomicOpCode::Add as i64,
            AtomicOp::Sub => RiscVAtomicOpCode::Sub as i64,
            AtomicOp::Neg => RiscVAtomicOpCode::Neg as i64,
            AtomicOp::And => RiscVAtomicOpCode::And as i64,
            AtomicOp::Or => RiscVAtomicOpCode::Or as i64,
            AtomicOp::Xor => RiscVAtomicOpCode::Xor as i64,
            AtomicOp::Nand => RiscVAtomicOpCode::Nand as i64,
            AtomicOp::Max => RiscVAtomicOpCode::Max as i64,
            AtomicOp::Min => RiscVAtomicOpCode::Min as i64,
            AtomicOp::Umax => RiscVAtomicOpCode::Umax as i64,
            AtomicOp::Umin => RiscVAtomicOpCode::Umin as i64,
            AtomicOp::Swap => RiscVAtomicOpCode::Swap as i64,
        }
    }

    fn memory_order_code(order: MemoryOrder) -> i64 {
        match order {
            MemoryOrder::Relaxed => RiscVMemoryOrderCode::Relaxed as i64,
            MemoryOrder::Acquire => RiscVMemoryOrderCode::Acquire as i64,
            MemoryOrder::Release => RiscVMemoryOrderCode::Release as i64,
            MemoryOrder::AcqRel => RiscVMemoryOrderCode::AcqRel as i64,
            MemoryOrder::SeqCst => RiscVMemoryOrderCode::SeqCst as i64,
        }
    }

    fn int_crypto_op_code(op: RvOp) -> Result<i64, LowerError> {
        let code = match op {
            RvOp::Clmul => RiscVIntCryptoOpCode::Clmul,
            RvOp::Clmulh => RiscVIntCryptoOpCode::Clmulh,
            RvOp::Clmulr => RiscVIntCryptoOpCode::Clmulr,
            RvOp::Xperm4 => RiscVIntCryptoOpCode::Xperm4,
            RvOp::Xperm8 => RiscVIntCryptoOpCode::Xperm8,
            RvOp::Sha512Sig0l => RiscVIntCryptoOpCode::Sha512Sig0l,
            RvOp::Sha512Sig0h => RiscVIntCryptoOpCode::Sha512Sig0h,
            RvOp::Sha512Sig1l => RiscVIntCryptoOpCode::Sha512Sig1l,
            RvOp::Sha512Sig1h => RiscVIntCryptoOpCode::Sha512Sig1h,
            RvOp::Sha512Sum0r => RiscVIntCryptoOpCode::Sha512Sum0r,
            RvOp::Sha512Sum1r => RiscVIntCryptoOpCode::Sha512Sum1r,
            RvOp::Sm4ed => RiscVIntCryptoOpCode::Sm4ed,
            RvOp::Sm4ks => RiscVIntCryptoOpCode::Sm4ks,
            RvOp::Aes32esi => RiscVIntCryptoOpCode::Aes32esi,
            RvOp::Aes32esmi => RiscVIntCryptoOpCode::Aes32esmi,
            RvOp::Aes32dsi => RiscVIntCryptoOpCode::Aes32dsi,
            RvOp::Aes32dsmi => RiscVIntCryptoOpCode::Aes32dsmi,
            RvOp::Aes64es => RiscVIntCryptoOpCode::Aes64es,
            RvOp::Aes64esm => RiscVIntCryptoOpCode::Aes64esm,
            RvOp::Aes64ds => RiscVIntCryptoOpCode::Aes64ds,
            RvOp::Aes64dsm => RiscVIntCryptoOpCode::Aes64dsm,
            RvOp::Aes64im => RiscVIntCryptoOpCode::Aes64im,
            RvOp::Aes64ks1i => RiscVIntCryptoOpCode::Aes64ks1i,
            RvOp::Aes64ks2 => RiscVIntCryptoOpCode::Aes64ks2,
            other => {
                return Err(LowerError::UnsupportedOp {
                    op: format!("RISC-V integer-crypto helper operation {other:?}"),
                });
            }
        };
        Ok(code as i64)
    }

    fn lower_int_crypto(
        &mut self,
        dst: VReg,
        src1: VReg,
        src2: VReg,
        op: RvOp,
        imm: u8,
        xlen: u8,
    ) -> Result<(), LowerError> {
        if !matches!(xlen, 32 | 64) {
            return Err(LowerError::UnsupportedOp {
                op: format!("RISC-V integer-crypto XLEN {xlen}"),
            });
        }
        let op_code = Self::int_crypto_op_code(op)?;
        self.load_vreg_to(src1, ADDR, OpWidth::W64)?;
        self.load_vreg_to(src2, HI, OpWidth::W64)?;
        {
            let mut e = X86Emitter::new(&mut self.code);
            e.emit_mov_ri(RHS, i64::from(imm), OpWidth::W64);
            e.emit_mov_ri(TMP0, i64::from(xlen), OpWidth::W64);
            e.emit_mov_rm(TARGET, STATE, RV_INT_CRYPTO_FN_OFFSET, OpWidth::W64);
            // Preserve the state pointer and retain 16-byte stack alignment at
            // the SysV call boundary.  RDI is then free for helper argument 0.
            e.emit_push(STATE);
            e.emit_sub_ri(PhysReg::Rsp, 8, OpWidth::W64);
            e.emit_mov_ri(STATE, op_code, OpWidth::W64);
            e.emit_call_reg(TARGET);
            e.emit_add_ri(PhysReg::Rsp, 8, OpWidth::W64);
            e.emit_pop(STATE);
        }
        self.store_reg_to(dst, ACC, OpWidth::W64)
    }

    #[allow(clippy::too_many_arguments)]
    fn lower_rv_fp(
        &mut self,
        dst: VReg,
        fcsr_dst: VReg,
        src1: VReg,
        src2: VReg,
        src3: VReg,
        fcsr_src: VReg,
        op: RvOp,
        rm_field: u8,
        xlen: u8,
        guest_pc: u64,
    ) -> Result<(), LowerError> {
        if !matches!(xlen, 32 | 64) {
            return Err(LowerError::UnsupportedOp {
                op: format!("RISC-V scalar-FP XLEN {xlen}"),
            });
        }
        if rm_field > 7 {
            return Err(LowerError::UnsupportedOp {
                op: format!("RISC-V scalar-FP rounding field {rm_field}"),
            });
        }
        if xlen == 32 && crate::isa::riscv::float::fp_requires_rv64(op) {
            return Err(LowerError::UnsupportedOp {
                op: format!("RISC-V scalar-FP RV64-only operation {op:?} with XLEN 32"),
            });
        }
        let op_code = RiscVFpOpCode::from_op(op).ok_or_else(|| LowerError::UnsupportedOp {
            op: format!("RISC-V scalar-FP helper operation {op:?}"),
        })? as i64;

        // SysV argument order: op, rm, fcsr, a, b, c. RDI remains the state
        // pointer until every state-backed source and the helper address load.
        self.load_vreg_to(src1, RHS, OpWidth::W64)?;
        self.load_vreg_to(src2, TMP0, OpWidth::W64)?;
        self.load_vreg_to(src3, TMP1, OpWidth::W64)?;
        self.load_vreg_to(fcsr_src, HI, OpWidth::W64)?;
        {
            let mut e = X86Emitter::new(&mut self.code);
            e.emit_mov_ri(ADDR, i64::from(rm_field), OpWidth::W64);
            e.emit_mov_rm(TARGET, STATE, RV_FP_FN_OFFSET, OpWidth::W64);
            e.emit_push(STATE);
            e.emit_sub_ri(PhysReg::Rsp, 8, OpWidth::W64);
            e.emit_mov_ri(STATE, op_code, OpWidth::W64);
            e.emit_call_reg(TARGET);
            e.emit_add_ri(PhysReg::Rsp, 8, OpWidth::W64);
            e.emit_pop(STATE);
            // The two-register result is RAX=value, RDX=fcsr/status. An
            // invalid status must trap before either destination is written.
            e.emit_cmp_ri(HI, RISCV_FP_RESULT_INVALID as i64, OpWidth::W64);
        }
        let valid = self.emit_jcc_placeholder(X86Cond::Ne);
        self.emit_arch_exit(guest_pc, EXIT_TRAP);
        self.patch_rel32_to_current(valid)?;
        if xlen == 32 && crate::isa::riscv::float::fp_writes_int_dst(op) {
            let mut e = X86Emitter::new(&mut self.code);
            e.emit_mov_rr(ACC, ACC, OpWidth::W32);
        }
        self.store_reg_to(dst, ACC, OpWidth::W64)?;
        self.store_reg_to(fcsr_dst, HI, OpWidth::W64)
    }

    fn lower_rv_vector(
        &mut self,
        insn: u32,
        xlen: u8,
        state: &crate::smir::ir::ops::RvVectorState,
        guest_pc: u64,
    ) -> Result<(), LowerError> {
        if !matches!(xlen, 32 | 64) {
            return Err(LowerError::UnsupportedOp {
                op: format!("RISC-V Vector XLEN {xlen}"),
            });
        }

        // Materialize the complete scalar/CSR SSA snapshot before the opaque
        // helper observes it. This is required when optimization has kept a
        // newer value in a virtual register rather than its architectural slot.
        for (index, src) in state.x_srcs.iter().copied().enumerate() {
            self.load_vreg_to(src, ACC, OpWidth::W64)?;
            let mut e = X86Emitter::new(&mut self.code);
            e.emit_mov_mr(
                STATE,
                RV_X_OFFSET + i32::try_from(index).expect("32 registers") * 8,
                ACC,
                OpWidth::W64,
            );
        }
        for (index, src) in state.f_srcs.iter().copied().enumerate() {
            self.load_vreg_to(src, ACC, OpWidth::W64)?;
            let mut e = X86Emitter::new(&mut self.code);
            e.emit_mov_mr(
                STATE,
                RV_F_OFFSET + i32::try_from(index).expect("32 registers") * 8,
                ACC,
                OpWidth::W64,
            );
        }
        for (src, offset) in [
            (state.fcsr_src, RV_FCSR_OFFSET),
            (state.vl_src, RV_VL_OFFSET),
            (state.vtype_src, RV_VTYPE_OFFSET),
            (state.vstart_src, RV_VSTART_OFFSET),
            (state.vcsr_src, RV_VCSR_OFFSET),
        ] {
            self.load_vreg_to(src, ACC, OpWidth::W64)?;
            let mut e = X86Emitter::new(&mut self.code);
            e.emit_mov_mr(STATE, offset, ACC, OpWidth::W64);
        }

        // SysV arguments are (state, insn, xlen). RDI already contains state.
        // Preserve it across the Rust helper and retain 16-byte call alignment.
        {
            let mut e = X86Emitter::new(&mut self.code);
            e.emit_mov_rm(TARGET, STATE, RV_VECTOR_FN_OFFSET, OpWidth::W64);
            e.emit_mov_ri_imm64(ADDR, i64::from(insn));
            e.emit_mov_ri(HI, i64::from(xlen), OpWidth::W64);
            e.emit_push(STATE);
            e.emit_sub_ri(PhysReg::Rsp, 8, OpWidth::W64);
            e.emit_call_reg(TARGET);
            e.emit_add_ri(PhysReg::Rsp, 8, OpWidth::W64);
            e.emit_pop(STATE);
        }
        self.emit_trap_unless_one(ACC, guest_pc)?;

        // The helper owns the external vector-file mutation. Re-import every
        // scalar/CSR result into its recorded SSA destination only after exact
        // success, so malformed/fault statuses cannot partially commit results.
        for (index, dst) in state.x_dsts.iter().copied().enumerate().skip(1) {
            {
                let mut e = X86Emitter::new(&mut self.code);
                e.emit_mov_rm(
                    ACC,
                    STATE,
                    RV_X_OFFSET + i32::try_from(index).expect("32 registers") * 8,
                    OpWidth::W64,
                );
            }
            self.store_reg_to(dst, ACC, OpWidth::W64)?;
        }
        for (index, dst) in state.f_dsts.iter().copied().enumerate() {
            {
                let mut e = X86Emitter::new(&mut self.code);
                e.emit_mov_rm(
                    ACC,
                    STATE,
                    RV_F_OFFSET + i32::try_from(index).expect("32 registers") * 8,
                    OpWidth::W64,
                );
            }
            self.store_reg_to(dst, ACC, OpWidth::W64)?;
        }
        for (dst, offset) in [
            (state.fcsr_dst, RV_FCSR_OFFSET),
            (state.vl_dst, RV_VL_OFFSET),
            (state.vtype_dst, RV_VTYPE_OFFSET),
            (state.vstart_dst, RV_VSTART_OFFSET),
            (state.vcsr_dst, RV_VCSR_OFFSET),
        ] {
            {
                let mut e = X86Emitter::new(&mut self.code);
                e.emit_mov_rm(ACC, STATE, offset, OpWidth::W64);
            }
            self.store_reg_to(dst, ACC, OpWidth::W64)?;
        }
        Ok(())
    }

    fn lower_atomic_rmw(
        &mut self,
        dst: VReg,
        addr: &Address,
        src: VReg,
        op: AtomicOp,
        width: MemWidth,
        order: MemoryOrder,
        guest_pc: u64,
    ) -> Result<(), LowerError> {
        let (op_width, size) = Self::scalar_mem_width(width)?;
        self.load_addr_to(addr, ADDR)?;
        self.load_vreg_to(src, HI, op_width)?;
        {
            let mut e = X86Emitter::new(&mut self.code);
            e.emit_mov_ri(RHS, size, OpWidth::W64);
            e.emit_mov_ri(TMP0, Self::atomic_op_code(op), OpWidth::W64);
            e.emit_mov_ri(TMP1, Self::memory_order_code(order), OpWidth::W64);
            e.emit_mov_rm(TARGET, STATE, RV_ATOMIC_RMW_FN_OFFSET, OpWidth::W64);
        }
        self.emit_mem_helper_call(TARGET);
        self.emit_trap_unless_one(HI, guest_pc)?;
        self.store_reg_to(dst, ACC, OpWidth::W64)
    }

    fn lower_cas(
        &mut self,
        dst: VReg,
        success: VReg,
        addr: &Address,
        expected: VReg,
        new_val: VReg,
        width: MemWidth,
        order: MemoryOrder,
        guest_pc: u64,
    ) -> Result<(), LowerError> {
        let (op_width, size) = Self::scalar_mem_width(width)?;
        self.load_addr_to(addr, ADDR)?;
        self.load_vreg_to(expected, HI, op_width)?;
        self.load_vreg_to(new_val, RHS, op_width)?;
        {
            let mut e = X86Emitter::new(&mut self.code);
            e.emit_mov_ri(TMP0, size, OpWidth::W64);
            e.emit_mov_ri(TMP1, Self::memory_order_code(order), OpWidth::W64);
            e.emit_mov_rm(TARGET, STATE, RV_CAS_FN_OFFSET, OpWidth::W64);
        }
        self.emit_mem_helper_call(TARGET);
        // RDX is 0=fault, 1=compare failed, 2=swapped. Subtracting one maps
        // the two completed outcomes to the SMIR Boolean; the unsigned range
        // check also rejects zero and non-canonical helper statuses.
        {
            let mut e = X86Emitter::new(&mut self.code);
            e.emit_sub_ri(HI, 1, OpWidth::W64);
            e.emit_cmp_ri(HI, 1, OpWidth::W64);
        }
        let valid = self.emit_jcc_placeholder(X86Cond::Be);
        self.emit_arch_exit(guest_pc, EXIT_TRAP);
        self.patch_rel32_to_current(valid)?;
        self.store_reg_to(dst, ACC, OpWidth::W64)?;
        self.store_reg_to(success, HI, OpWidth::W64)
    }

    fn lower_load_exclusive(
        &mut self,
        dst: VReg,
        addr: &Address,
        width: MemWidth,
        guest_pc: u64,
    ) -> Result<(), LowerError> {
        let (_, size) = Self::scalar_mem_width(width)?;
        self.load_addr_to(addr, ADDR)?;
        {
            let mut e = X86Emitter::new(&mut self.code);
            e.emit_mov_ri(HI, size, OpWidth::W64);
            e.emit_mov_rm(TARGET, STATE, RV_LOAD_EXCLUSIVE_FN_OFFSET, OpWidth::W64);
        }
        self.emit_mem_helper_call(TARGET);
        self.emit_trap_unless_one(HI, guest_pc)?;
        self.store_reg_to(dst, ACC, OpWidth::W64)
    }

    fn lower_store_exclusive(
        &mut self,
        status: VReg,
        src: VReg,
        addr: &Address,
        width: MemWidth,
        guest_pc: u64,
    ) -> Result<(), LowerError> {
        let (op_width, size) = Self::scalar_mem_width(width)?;
        self.load_addr_to(addr, ADDR)?;
        self.load_vreg_to(src, HI, op_width)?;
        {
            let mut e = X86Emitter::new(&mut self.code);
            e.emit_mov_ri(RHS, size, OpWidth::W64);
            e.emit_mov_rm(TARGET, STATE, RV_STORE_EXCLUSIVE_FN_OFFSET, OpWidth::W64);
        }
        self.emit_mem_helper_call(TARGET);
        self.emit_trap_unless_one(HI, guest_pc)?;
        {
            let mut e = X86Emitter::new(&mut self.code);
            e.emit_test_rr(ACC, ACC, OpWidth::W64);
            e.emit_setcc(X86Cond::E, ACC);
            e.emit_movzx(ACC, ACC, OpWidth::W8, OpWidth::W64);
        }
        self.store_reg_to(status, ACC, OpWidth::W64)
    }

    fn lower_clear_exclusive(&mut self) {
        {
            let mut e = X86Emitter::new(&mut self.code);
            e.emit_mov_rm(TARGET, STATE, RV_CLEAR_EXCLUSIVE_FN_OFFSET, OpWidth::W64);
        }
        self.emit_mem_helper_call(TARGET);
    }

    fn lower_op(&mut self, op: &SmirOp) -> Result<(), LowerError> {
        match &op.kind {
            OpKind::Nop | OpKind::Fence { .. } => {
                let mut e = X86Emitter::new(&mut self.code);
                if matches!(op.kind, OpKind::Fence { .. }) {
                    e.emit_mfence();
                } else {
                    e.emit_nop();
                }
            }
            OpKind::Mov { dst, src, width } => {
                self.load_src_to(src, ACC, *width)?;
                self.store_reg_to(*dst, ACC, *width)?;
            }
            OpKind::Add {
                dst,
                src1,
                src2,
                width,
                flags,
            } => self.lower_binop(*dst, *src1, src2, *width, *flags, BinOp::Add)?,
            OpKind::Sub {
                dst,
                src1,
                src2,
                width,
                flags,
            } => self.lower_binop(*dst, *src1, src2, *width, *flags, BinOp::Sub)?,
            OpKind::And {
                dst,
                src1,
                src2,
                width,
                flags,
            } => self.lower_binop(*dst, *src1, src2, *width, *flags, BinOp::And)?,
            OpKind::Or {
                dst,
                src1,
                src2,
                width,
                flags,
            } => self.lower_binop(*dst, *src1, src2, *width, *flags, BinOp::Or)?,
            OpKind::Xor {
                dst,
                src1,
                src2,
                width,
                flags,
            } => self.lower_binop(*dst, *src1, src2, *width, *flags, BinOp::Xor)?,
            OpKind::AndNot {
                dst,
                src1,
                src2,
                width,
                flags,
            } => {
                Self::require_no_flags(*flags, "AndNot")?;
                self.load_vreg_to(*src1, ACC, *width)?;
                self.load_src_to(src2, RHS, *width)?;
                {
                    let mut e = X86Emitter::new(&mut self.code);
                    e.emit_not(RHS, *width);
                    e.emit_and_rr(ACC, RHS, *width);
                }
                self.store_reg_to(*dst, ACC, *width)?;
            }
            OpKind::Not { dst, src, width } => {
                self.load_vreg_to(*src, ACC, *width)?;
                {
                    let mut e = X86Emitter::new(&mut self.code);
                    e.emit_not(ACC, *width);
                }
                self.store_reg_to(*dst, ACC, *width)?;
            }
            OpKind::Shl {
                dst,
                src,
                amount,
                width,
                flags,
            } => self.lower_shift(*dst, *src, amount, *width, *flags, ShiftOp::Lsl, false)?,
            OpKind::Shr {
                dst,
                src,
                amount,
                width,
                flags,
            } => self.lower_shift(*dst, *src, amount, *width, *flags, ShiftOp::Lsr, false)?,
            OpKind::Sar {
                dst,
                src,
                amount,
                width,
                flags,
            } => self.lower_shift(*dst, *src, amount, *width, *flags, ShiftOp::Asr, false)?,
            OpKind::Ror {
                dst,
                src,
                amount,
                width,
                flags,
            } => self.lower_shift(*dst, *src, amount, *width, *flags, ShiftOp::Ror, false)?,
            OpKind::Rol {
                dst,
                src,
                amount,
                width,
                flags,
            } => self.lower_shift(*dst, *src, amount, *width, *flags, ShiftOp::Ror, true)?,
            OpKind::MulU {
                dst_lo,
                dst_hi,
                src1,
                src2,
                width,
                flags,
            } => self.lower_mul(*dst_lo, *dst_hi, *src1, src2, *width, *flags, false)?,
            OpKind::MulS {
                dst_lo,
                dst_hi,
                src1,
                src2,
                width,
                flags,
            } => self.lower_mul(*dst_lo, *dst_hi, *src1, src2, *width, *flags, true)?,
            OpKind::DivU {
                quot,
                rem,
                src1,
                src2,
                width,
                flags,
            } => self.lower_div(*quot, *rem, *src1, src2, *width, *flags, false)?,
            OpKind::DivS {
                quot,
                rem,
                src1,
                src2,
                width,
                flags,
            } => self.lower_div(*quot, *rem, *src1, src2, *width, *flags, true)?,
            OpKind::Cmp { src1, src2, width } => {
                self.load_vreg_to(*src1, ACC, *width)?;
                self.load_src_to(src2, RHS, *width)?;
                let mut e = X86Emitter::new(&mut self.code);
                e.emit_cmp_rr(ACC, RHS, *width);
            }
            OpKind::SetCC { dst, cond, .. } | OpKind::TestCondition { dst, cond } => {
                let mut e = X86Emitter::new(&mut self.code);
                if *cond == Condition::Always {
                    e.emit_mov_ri(ACC, 1, OpWidth::W32);
                } else {
                    e.emit_setcc(X86Cond::from_condition(*cond), ACC);
                    e.emit_movzx(ACC, ACC, OpWidth::W8, OpWidth::W64);
                }
                self.store_reg_to(*dst, ACC, OpWidth::W64)?;
            }
            OpKind::Select {
                dst,
                cond,
                src_true,
                src_false,
                width,
            } => {
                self.load_vreg_to(*src_false, ACC, *width)?;
                self.load_vreg_to(*src_true, RHS, *width)?;
                self.load_vreg_to(*cond, TMP0, OpWidth::W64)?;
                let cmov_width = if *width == OpWidth::W8 {
                    OpWidth::W16
                } else {
                    *width
                };
                {
                    let mut e = X86Emitter::new(&mut self.code);
                    e.emit_test_rr(TMP0, TMP0, OpWidth::W64);
                    e.emit_cmovcc(X86Cond::Ne, ACC, RHS, cmov_width);
                }
                self.store_reg_to(*dst, ACC, *width)?;
            }
            OpKind::ZeroExtend {
                dst,
                src,
                from_width,
                to_width,
            } => {
                self.load_vreg_to(*src, ACC, *to_width)?;
                {
                    let mut e = X86Emitter::new(&mut self.code);
                    match from_width {
                        OpWidth::W8 | OpWidth::W16 => {
                            e.emit_movzx(ACC, ACC, *from_width, *to_width)
                        }
                        OpWidth::W32 => e.emit_mov_rr(ACC, ACC, OpWidth::W32),
                        OpWidth::W64 => {}
                        OpWidth::W128 => {
                            return Err(LowerError::UnsupportedOp {
                                op: "RISC-V ZeroExtend from W128".into(),
                            });
                        }
                    }
                }
                self.store_reg_to(*dst, ACC, *to_width)?;
            }
            OpKind::SignExtend {
                dst,
                src,
                from_width,
                to_width,
            } => {
                self.load_vreg_to(*src, ACC, *to_width)?;
                {
                    let mut e = X86Emitter::new(&mut self.code);
                    match from_width {
                        OpWidth::W8 | OpWidth::W16 | OpWidth::W32 => {
                            e.emit_movsx(ACC, ACC, *from_width, *to_width)
                        }
                        OpWidth::W64 => {}
                        OpWidth::W128 => {
                            return Err(LowerError::UnsupportedOp {
                                op: "RISC-V SignExtend from W128".into(),
                            });
                        }
                    }
                }
                self.store_reg_to(*dst, ACC, *to_width)?;
            }
            OpKind::Clz { dst, src, width } => self.lower_clz(*dst, *src, *width)?,
            OpKind::Ctz { dst, src, width } => self.lower_ctz(*dst, *src, *width)?,
            OpKind::Popcnt { dst, src, width } => self.lower_popcnt(*dst, *src, *width)?,
            OpKind::Bswap { dst, src, width } => {
                self.load_vreg_to(*src, ACC, *width)?;
                {
                    let mut e = X86Emitter::new(&mut self.code);
                    e.emit_bswap(ACC, *width);
                }
                self.store_reg_to(*dst, ACC, *width)?;
            }
            OpKind::Rbit { dst, src, width } => self.lower_rbit(*dst, *src, *width)?,
            OpKind::Load {
                dst,
                addr,
                width,
                sign,
            } => self.lower_load(*dst, addr, *width, *sign, op.guest_pc)?,
            OpKind::Store { src, addr, width } => {
                self.lower_store(*src, addr, *width, op.guest_pc)?
            }
            OpKind::AtomicRmw {
                dst,
                addr,
                src,
                op: atomic_op,
                width,
                order,
            } => {
                self.lower_atomic_rmw(*dst, addr, *src, *atomic_op, *width, *order, op.guest_pc)?
            }
            OpKind::Cas {
                dst,
                success,
                addr,
                expected,
                new_val,
                width,
                order,
            } => self.lower_cas(
                *dst,
                *success,
                addr,
                *expected,
                *new_val,
                *width,
                *order,
                op.guest_pc,
            )?,
            OpKind::LoadExclusive { dst, addr, width } => {
                self.lower_load_exclusive(*dst, addr, *width, op.guest_pc)?
            }
            OpKind::StoreExclusive {
                status,
                src,
                addr,
                width,
            } => self.lower_store_exclusive(*status, *src, addr, *width, op.guest_pc)?,
            OpKind::ClearExclusive => self.lower_clear_exclusive(),
            OpKind::RvIntCrypto {
                dst,
                src1,
                src2,
                op,
                imm,
                xlen,
            } => self.lower_int_crypto(*dst, *src1, *src2, *op, *imm, *xlen)?,
            OpKind::RvFp {
                dst,
                fcsr_dst,
                src1,
                src2,
                src3,
                fcsr_src,
                op: fp_op,
                rm_field,
                xlen,
            } => self.lower_rv_fp(
                *dst,
                *fcsr_dst,
                *src1,
                *src2,
                *src3,
                *fcsr_src,
                *fp_op,
                *rm_field,
                *xlen,
                op.guest_pc,
            )?,
            OpKind::RvVector {
                insn, xlen, state, ..
            } => self.lower_rv_vector(*insn, *xlen, state, op.guest_pc)?,
            OpKind::Breakpoint => self.emit_arch_exit(op.guest_pc, EXIT_BREAKPOINT),
            OpKind::Syscall { .. } => self.emit_arch_exit(op.guest_pc, EXIT_SYSCALL),
            other => {
                return Err(LowerError::UnsupportedOp {
                    op: format!("RISC-V state-backed lowering for {other:?}"),
                });
            }
        }
        Ok(())
    }

    fn emit_arch_exit(&mut self, pc: u64, reason: i64) {
        self.emit_mov_imm(ACC, pc as i64, OpWidth::W64);
        self.emit_mov_imm(RHS, reason, OpWidth::W64);
        let mut e = X86Emitter::new(&mut self.code);
        e.emit_mov_mr(STATE, RV_PC_OFFSET, ACC, OpWidth::W64);
        e.emit_mov_mr(STATE, RV_EXIT_REASON_OFFSET, RHS, OpWidth::W64);
        drop(e);
        self.emit_epilogue();
    }

    fn return_pc(&self, block: &SmirBlock) -> u64 {
        self.return_pcs.get(&block.id).copied().unwrap_or_else(|| {
            block
                .ops
                .last()
                .map(|op| op.guest_pc.wrapping_add(4))
                .unwrap_or(block.guest_pc)
        })
    }

    fn lower_terminator(&mut self, block: &SmirBlock) -> Result<(), LowerError> {
        match &block.terminator {
            Terminator::Branch { target } => {
                let offset = self.code.position();
                let mut e = X86Emitter::new(&mut self.code);
                e.emit_jmp_rel32(0);
                self.pending_jumps
                    .push((offset + 1, *target, RelocKind::PcRel32));
            }
            Terminator::CondBranch {
                cond,
                true_target,
                false_target,
            } => {
                self.load_vreg_to(*cond, ACC, OpWidth::W64)?;
                let jcc = self.code.position();
                {
                    let mut e = X86Emitter::new(&mut self.code);
                    e.emit_test_rr(ACC, ACC, OpWidth::W64);
                    e.emit_jcc_rel32(X86Cond::Ne, 0);
                }
                self.pending_jumps
                    .push((jcc + 5, *true_target, RelocKind::PcRel32));
                let jmp = self.code.position();
                {
                    let mut e = X86Emitter::new(&mut self.code);
                    e.emit_jmp_rel32(0);
                }
                self.pending_jumps
                    .push((jmp + 1, *false_target, RelocKind::PcRel32));
            }
            Terminator::IndirectBranch { target, .. } => {
                self.load_vreg_to(*target, ACC, OpWidth::W64)?;
                self.emit_mov_imm(RHS, EXIT_RETURN, OpWidth::W64);
                {
                    let mut e = X86Emitter::new(&mut self.code);
                    e.emit_mov_mr(STATE, RV_PC_OFFSET, ACC, OpWidth::W64);
                    e.emit_mov_mr(STATE, RV_EXIT_REASON_OFFSET, RHS, OpWidth::W64);
                }
                self.emit_epilogue();
            }
            Terminator::Return { .. } => {
                self.emit_arch_exit(self.return_pc(block), EXIT_RETURN);
            }
            Terminator::Trap { kind } => {
                let reason = match kind {
                    TrapKind::SystemCall => EXIT_SYSCALL,
                    TrapKind::Breakpoint => EXIT_BREAKPOINT,
                    _ => EXIT_TRAP,
                };
                let pc = block
                    .ops
                    .last()
                    .map(|op| op.guest_pc)
                    .unwrap_or(block.guest_pc);
                self.emit_arch_exit(pc, reason);
            }
            Terminator::Unreachable => self.emit_arch_exit(block.guest_pc, EXIT_TRAP),
            other => {
                return Err(LowerError::UnsupportedOp {
                    op: format!("RISC-V state-backed terminator {other:?}"),
                });
            }
        }
        Ok(())
    }

    fn lower_block(&mut self, block: &SmirBlock) -> Result<(), LowerError> {
        if !block.phis.is_empty() {
            return Err(LowerError::UnsupportedOp {
                op: format!("RISC-V block {:?} contains phi nodes", block.id),
            });
        }
        self.block_offsets.insert(block.id, self.code.position());
        for op in &block.ops {
            self.lower_op(op)?;
        }
        self.lower_terminator(block)
    }

    fn fixup_jumps(&mut self) -> Result<(), LowerError> {
        for (offset, target, kind) in self.pending_jumps.drain(..).collect::<Vec<_>>() {
            let Some(&target_offset) = self.block_offsets.get(&target) else {
                return Err(LowerError::UndefinedLabel {
                    label: format!("block_{}", target.0),
                });
            };
            match kind {
                RelocKind::PcRel32 => {
                    let rel = target_offset as i64 - offset as i64 - 4;
                    if !(i32::MIN as i64..=i32::MAX as i64).contains(&rel) {
                        return Err(LowerError::RelocationOutOfRange {
                            offset,
                            target: target_offset,
                        });
                    }
                    self.code.patch_i32(offset, rel as i32);
                }
                other => {
                    return Err(LowerError::UnsupportedOp {
                        op: format!("RISC-V internal relocation {other:?}"),
                    });
                }
            }
        }
        Ok(())
    }
}

impl Default for RiscVX86_64Lowerer {
    fn default() -> Self {
        Self::new()
    }
}

impl SmirLowerer for RiscVX86_64Lowerer {
    fn target_arch(&self) -> &'static str {
        "x86_64"
    }

    fn lower_function(&mut self, func: &SmirFunction) -> Result<LowerResult, LowerError> {
        self.code.clear();
        self.block_offsets.clear();
        self.pending_jumps.clear();
        self.relocations.clear();
        self.collect_virtuals(func);

        let entry_offset = self.code.position();
        self.emit_prologue();
        if let Some(entry) = func.get_block(func.entry) {
            self.lower_block(entry)?;
        } else {
            return Err(LowerError::UndefinedLabel {
                label: format!("entry block {}", func.entry.0),
            });
        }
        for block in &func.blocks {
            if block.id != func.entry {
                self.lower_block(block)?;
            }
        }
        self.fixup_jumps()?;

        Ok(LowerResult {
            code_size: self.code.len(),
            entry_offset,
            block_offsets: self.block_offsets.clone(),
            relocations: self.relocations.clone(),
            stack_size: self.frame_size,
        })
    }

    fn code_buffer(&self) -> &CodeBuffer {
        &self.code
    }

    fn finalize(&mut self) -> Result<Vec<u8>, LowerError> {
        Ok(self.code.data().to_vec())
    }
}

fn align16(value: usize) -> usize {
    (value + 15) & !15
}
