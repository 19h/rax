//! State-backed RISC-V-to-AArch64 SMIR lowerer.
//!
//! Lowered code uses the AAPCS64 `extern "C" fn(*mut RiscVGuestRegs)` ABI.
//! X19 is the persistent guest-state pointer, architectural registers are
//! loaded/stored through that state, and SSA temporaries occupy stack slots.
//! The initial backend deliberately admits the scalar I/M/C/Zb-shaped SMIR
//! primitives plus precise helper-backed loads and stores. Operations outside
//! that boundary return [`LowerError`] and remain interpreter-exact.

use std::collections::{HashMap, HashSet};

use crate::isa::riscv::Op as RvOp;
use crate::smir::ir::flags::FlagUpdate;
use crate::smir::ir::ops::{OpKind, SmirOp};
use crate::smir::ir::types::{
    Address, ArchReg, AtomicOp, BlockId, Condition, MemWidth, MemoryOrder, OpWidth, RiscVReg,
    SignExtend, SrcOperand, VReg, VirtualId,
};
use crate::smir::ir::{CallTarget, SmirBlock, SmirFunction, Terminator, TrapKind};
use crate::smir::lower::cross::riscv_x86_64_abi::{
    RISCV_FP_RESULT_INVALID, RiscVAtomicOpCode, RiscVFpOpCode, RiscVIntCryptoOpCode,
    RiscVMemoryOrderCode,
};
use crate::smir::lower::{CodeBuffer, LowerError, LowerResult, Relocation, SmirLowerer};

// AAPCS64: X19 is callee-saved. All arithmetic registers below are caller-saved
// and may be destroyed by helper calls. Virtual values are therefore always
// materialized in the frame rather than retained across operations.
const STATE: u8 = 19;
const ACC: u8 = 9;
const RHS: u8 = 10;
const TMP0: u8 = 11;
const TMP1: u8 = 12;
const HI: u8 = 13;
const TARGET: u8 = 16;
const ADDR: u8 = 1;

const RV_X_OFFSET: u32 = 0;
const RV_F_OFFSET: u32 = 32 * 8;
const RV_PC_OFFSET: u32 = RV_F_OFFSET + 32 * 8;
const RV_FCSR_OFFSET: u32 = RV_PC_OFFSET + 8;
const RV_EXIT_REASON_OFFSET: u32 = RV_FCSR_OFFSET + 8;
const RV_CTX_OFFSET: u32 = RV_EXIT_REASON_OFFSET + 8;
const RV_LOAD_FN_OFFSET: u32 = RV_CTX_OFFSET + 8;
const RV_STORE_FN_OFFSET: u32 = RV_LOAD_FN_OFFSET + 8;
const RV_ATOMIC_RMW_FN_OFFSET: u32 = RV_STORE_FN_OFFSET + 8;
const RV_CAS_FN_OFFSET: u32 = RV_ATOMIC_RMW_FN_OFFSET + 8;
const RV_CAS_PAIR_FN_OFFSET: u32 = RV_CAS_FN_OFFSET + 8;
const RV_LOAD_EXCLUSIVE_FN_OFFSET: u32 = RV_CAS_PAIR_FN_OFFSET + 8;
const RV_STORE_EXCLUSIVE_FN_OFFSET: u32 = RV_LOAD_EXCLUSIVE_FN_OFFSET + 8;
const RV_CLEAR_EXCLUSIVE_FN_OFFSET: u32 = RV_STORE_EXCLUSIVE_FN_OFFSET + 8;
const RV_INT_CRYPTO_FN_OFFSET: u32 = RV_CLEAR_EXCLUSIVE_FN_OFFSET + 8;
const RV_FP_FN_OFFSET: u32 = RV_INT_CRYPTO_FN_OFFSET + 8;
const RV_V_OFFSET: u32 = RV_FP_FN_OFFSET + 8;
const RV_VL_OFFSET: u32 = RV_V_OFFSET + 32 * 16;
const RV_VTYPE_OFFSET: u32 = RV_VL_OFFSET + 8;
const RV_VSTART_OFFSET: u32 = RV_VTYPE_OFFSET + 8;
const RV_VCSR_OFFSET: u32 = RV_VSTART_OFFSET + 8;
const RV_VECTOR_FN_OFFSET: u32 = RV_VCSR_OFFSET + 8;
const RV_JVT_OFFSET: u32 = RV_VECTOR_FN_OFFSET + 8;
const RV_VLENB: i64 = 16;

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

#[derive(Clone, Copy, PartialEq, Eq)]
enum Shift {
    Lsl,
    Lsr,
    Asr,
    Ror,
    Rol,
}

#[derive(Clone, Copy)]
struct BranchFixup {
    offset: usize,
    target: BlockId,
    nonzero_reg: Option<u8>,
}

/// Lower scalar RISC-V SMIR to AArch64 using an explicit state pointer.
pub struct RiscVAarch64Lowerer {
    code: CodeBuffer,
    block_offsets: HashMap<BlockId, usize>,
    branch_fixups: Vec<BranchFixup>,
    virtual_slots: HashMap<VirtualId, u32>,
    frame_size: usize,
    relocations: Vec<Relocation>,
    return_pcs: HashMap<BlockId, u64>,
}

impl RiscVAarch64Lowerer {
    pub fn new() -> Self {
        Self {
            code: CodeBuffer::with_capacity(4096),
            block_offsets: HashMap::new(),
            branch_fixups: Vec::new(),
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

    fn emit(&mut self, word: u32) {
        self.code.emit_u32(word);
    }

    fn sf(width: OpWidth) -> Result<u32, LowerError> {
        match width {
            OpWidth::W8 | OpWidth::W16 | OpWidth::W32 => Ok(0),
            OpWidth::W64 => Ok(1),
            OpWidth::W128 => Err(LowerError::UnsupportedOp {
                op: "RISC-V AArch64 scalar width W128".into(),
            }),
        }
    }

    fn emit_mov_reg(&mut self, dst: u8, src: u8, width: OpWidth) -> Result<(), LowerError> {
        let sf = Self::sf(width)?;
        self.emit(
            (sf << 31)
                | (0b01 << 29)
                | (0b01010 << 24)
                | (u32::from(src) << 16)
                | (31 << 5)
                | u32::from(dst),
        );
        Ok(())
    }

    fn emit_mov_imm(&mut self, dst: u8, imm: i64, width: OpWidth) -> Result<(), LowerError> {
        let emit_width = if width == OpWidth::W64 {
            OpWidth::W64
        } else {
            OpWidth::W32
        };
        let sf = Self::sf(emit_width)?;
        let bits = if emit_width == OpWidth::W32 {
            u64::from(imm as u32)
        } else {
            imm as u64
        };
        let chunks = if sf == 0 { 2 } else { 4 };
        let mut emitted = false;
        for index in 0..chunks {
            let chunk = ((bits >> (index * 16)) & 0xffff) as u32;
            if !emitted || chunk != 0 {
                let opc = if emitted { 0b11 } else { 0b10 }; // MOVK / MOVZ
                self.emit(
                    (sf << 31)
                        | (opc << 29)
                        | (0b100101 << 23)
                        | ((index as u32) << 21)
                        | (chunk << 5)
                        | u32::from(dst),
                );
                emitted = true;
            }
        }
        Ok(())
    }

    fn emit_addsub_reg(
        &mut self,
        dst: u8,
        left: u8,
        right: u8,
        subtract: bool,
        set_flags: bool,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        let sf = Self::sf(width)?;
        self.emit(
            (sf << 31)
                | (u32::from(subtract) << 30)
                | (u32::from(set_flags) << 29)
                | (0b01011 << 24)
                | (u32::from(right) << 16)
                | (u32::from(left) << 5)
                | u32::from(dst),
        );
        Ok(())
    }

    fn emit_addsub_imm(
        &mut self,
        dst: u8,
        src: u8,
        imm: u32,
        subtract: bool,
        set_flags: bool,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        if imm > 0xfff {
            return Err(LowerError::InvalidOperand {
                op: "RISC-V AArch64 add/sub immediate".into(),
                operand: format!("{imm:#x}"),
            });
        }
        let sf = Self::sf(width)?;
        self.emit(
            (sf << 31)
                | (u32::from(subtract) << 30)
                | (u32::from(set_flags) << 29)
                | (0b10001 << 24)
                | (imm << 10)
                | (u32::from(src) << 5)
                | u32::from(dst),
        );
        Ok(())
    }

    fn emit_logic_reg(
        &mut self,
        dst: u8,
        left: u8,
        right: u8,
        opc: u32,
        invert_right: bool,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        let sf = Self::sf(width)?;
        self.emit(
            (sf << 31)
                | (opc << 29)
                | (0b01010 << 24)
                | (u32::from(invert_right) << 21)
                | (u32::from(right) << 16)
                | (u32::from(left) << 5)
                | u32::from(dst),
        );
        Ok(())
    }

    fn emit_bitfield(
        &mut self,
        dst: u8,
        src: u8,
        signed: bool,
        immr: u32,
        imms: u32,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        let sf = Self::sf(width)?;
        let opc = if signed { 0b00 } else { 0b10 };
        self.emit(
            (sf << 31)
                | (opc << 29)
                | (0b100110 << 23)
                | (sf << 22)
                | (immr << 16)
                | (imms << 10)
                | (u32::from(src) << 5)
                | u32::from(dst),
        );
        Ok(())
    }

    fn emit_extract(
        &mut self,
        dst: u8,
        src: u8,
        amount: u32,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        let sf = Self::sf(width)?;
        self.emit(
            (sf << 31)
                | (0b100111 << 23)
                | (sf << 22)
                | (u32::from(src) << 16)
                | (amount << 10)
                | (u32::from(src) << 5)
                | u32::from(dst),
        );
        Ok(())
    }

    fn emit_dp1(
        &mut self,
        dst: u8,
        src: u8,
        opcode: u32,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        let sf = Self::sf(width)?;
        self.emit(
            (sf << 31)
                | (0b1011010110 << 21)
                | (opcode << 10)
                | (u32::from(src) << 5)
                | u32::from(dst),
        );
        Ok(())
    }

    fn emit_dp2(
        &mut self,
        dst: u8,
        left: u8,
        right: u8,
        opcode: u32,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        let sf = Self::sf(width)?;
        self.emit(
            (sf << 31)
                | (0b0011010110 << 21)
                | (u32::from(right) << 16)
                | (opcode << 10)
                | (u32::from(left) << 5)
                | u32::from(dst),
        );
        Ok(())
    }

    fn emit_dp3(
        &mut self,
        dst: u8,
        left: u8,
        right: u8,
        addend: u8,
        op31: u32,
        subtract: bool,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        let sf = Self::sf(width)?;
        self.emit(
            (sf << 31)
                | (0b11011 << 24)
                | (op31 << 21)
                | (u32::from(right) << 16)
                | (u32::from(subtract) << 15)
                | (u32::from(addend) << 10)
                | (u32::from(left) << 5)
                | u32::from(dst),
        );
        Ok(())
    }

    fn emit_cond_select(
        &mut self,
        dst: u8,
        if_true: u8,
        if_false: u8,
        cond: u32,
        increment_false: bool,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        let sf = Self::sf(width)?;
        self.emit(
            (sf << 31)
                | (0b11010100 << 21)
                | (u32::from(if_false) << 16)
                | (cond << 12)
                | (u32::from(increment_false) << 10)
                | (u32::from(if_true) << 5)
                | u32::from(dst),
        );
        Ok(())
    }

    fn emit_ldst_unsigned(&mut self, rt: u8, rn: u8, load: bool, offset: u32) {
        debug_assert_eq!(offset & 7, 0);
        self.emit(
            (3 << 30)
                | (0b111 << 27)
                | (0b01 << 24)
                | (u32::from(load) << 22)
                | ((offset / 8) << 10)
                | (u32::from(rn) << 5)
                | u32::from(rt),
        );
    }

    fn emit_ldst_pair(&mut self, rt: u8, rt2: u8, rn: u8, load: bool, imm7: i32, mode: u32) {
        self.emit(
            (0b10 << 30)
                | (0b101 << 27)
                | (mode << 23)
                | (u32::from(load) << 22)
                | (((imm7 as u32) & 0x7f) << 15)
                | (u32::from(rt2) << 10)
                | (u32::from(rn) << 5)
                | u32::from(rt),
        );
    }

    fn emit_blr(&mut self, reg: u8) {
        self.emit(0xd63f_0000 | (u32::from(reg) << 5));
    }

    fn emit_prologue(&mut self) -> Result<(), LowerError> {
        self.emit_ldst_pair(29, 30, 31, false, -2, 0b11);
        self.emit_addsub_imm(29, 31, 0, false, false, OpWidth::W64)?;
        self.emit_ldst_pair(19, 20, 31, false, -2, 0b11);
        self.emit_mov_reg(STATE, 0, OpWidth::W64)?;
        if self.frame_size != 0 {
            self.emit_addsub_imm(31, 31, self.frame_size as u32, true, false, OpWidth::W64)?;
        }
        // Canonicalize the externally visible x0 slot on every native entry.
        self.emit_mov_imm(ACC, 0, OpWidth::W64)?;
        self.emit_ldst_unsigned(ACC, STATE, false, RV_X_OFFSET);
        Ok(())
    }

    fn emit_epilogue(&mut self) -> Result<(), LowerError> {
        if self.frame_size != 0 {
            self.emit_addsub_imm(31, 31, self.frame_size as u32, false, false, OpWidth::W64)?;
        }
        self.emit_ldst_pair(19, 20, 31, true, 2, 0b01);
        self.emit_ldst_pair(29, 30, 31, true, 2, 0b01);
        self.emit(0xd65f_03c0); // ret
        Ok(())
    }

    fn collect_virtuals(&mut self, func: &SmirFunction) -> Result<(), LowerError> {
        let mut ids = HashSet::new();
        for block in &func.blocks {
            for phi in &block.phis {
                if let VReg::Virtual(id) = phi.dst {
                    ids.insert(id);
                }
                for (_, source) in &phi.sources {
                    if let VReg::Virtual(id) = *source {
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
                    for reg in target.regs() {
                        if let VReg::Virtual(id) = reg {
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
            let offset = u32::try_from(index * 8).map_err(|_| LowerError::StackOverflow {
                required: usize::MAX,
                limit: 0xfff * 8,
            })?;
            self.virtual_slots.insert(id, offset);
        }
        self.frame_size = align16(self.virtual_slots.len() * 8);
        if self.frame_size > 0xfff {
            return Err(LowerError::StackOverflow {
                required: self.frame_size,
                limit: 0xfff,
            });
        }
        Ok(())
    }

    fn virtual_slot(&self, id: VirtualId) -> Result<u32, LowerError> {
        self.virtual_slots
            .get(&id)
            .copied()
            .ok_or_else(|| LowerError::RegisterAllocationFailed {
                reason: format!("missing AArch64 stack slot for virtual {id:?}"),
            })
    }

    fn arch_offset(reg: RiscVReg) -> Result<Option<u32>, LowerError> {
        match reg {
            RiscVReg::X(0) => Ok(None),
            RiscVReg::X(n @ 1..=31) => Ok(Some(RV_X_OFFSET + u32::from(n) * 8)),
            RiscVReg::F(n @ 0..=31) => Ok(Some(RV_F_OFFSET + u32::from(n) * 8)),
            RiscVReg::Pc => Ok(Some(RV_PC_OFFSET)),
            RiscVReg::Csr(0x003) => Ok(Some(RV_FCSR_OFFSET)),
            RiscVReg::Csr(0xc20) => Ok(Some(RV_VL_OFFSET)),
            RiscVReg::Csr(0xc21) => Ok(Some(RV_VTYPE_OFFSET)),
            RiscVReg::Csr(0x008) => Ok(Some(RV_VSTART_OFFSET)),
            RiscVReg::Csr(0x00f) => Ok(Some(RV_VCSR_OFFSET)),
            RiscVReg::Csr(0x017) => Ok(Some(RV_JVT_OFFSET)),
            other => Err(LowerError::InvalidRegister(format!(
                "unsupported state-backed RISC-V register {other:?}"
            ))),
        }
    }

    fn load_vreg_to(&mut self, reg: VReg, dst: u8, width: OpWidth) -> Result<(), LowerError> {
        match reg {
            VReg::Imm(value) => self.emit_mov_imm(dst, value, width)?,
            VReg::Virtual(id) => {
                self.emit_ldst_unsigned(dst, 31, true, self.virtual_slot(id)?);
            }
            VReg::Arch(ArchReg::RiscV(rv)) => {
                if rv == RiscVReg::Csr(0xc22) {
                    self.emit_mov_imm(dst, RV_VLENB, width)?;
                } else if let Some(offset) = Self::arch_offset(rv)? {
                    self.emit_ldst_unsigned(dst, STATE, true, offset);
                } else {
                    self.emit_mov_imm(dst, 0, width)?;
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

    fn load_src_to(&mut self, src: &SrcOperand, dst: u8, width: OpWidth) -> Result<(), LowerError> {
        match src {
            SrcOperand::Reg(reg) => self.load_vreg_to(*reg, dst, width),
            SrcOperand::Imm(value) | SrcOperand::Imm64(value) => {
                self.emit_mov_imm(dst, *value, width)
            }
            other => Err(LowerError::UnsupportedOp {
                op: format!("RISC-V AArch64 source {other:?}"),
            }),
        }
    }

    fn normalize_scalar(&mut self, reg: u8, width: OpWidth) -> Result<(), LowerError> {
        match width {
            OpWidth::W8 | OpWidth::W16 => {
                self.emit_bitfield(reg, reg, false, 0, width.bits() - 1, OpWidth::W32)
            }
            OpWidth::W32 => self.emit_mov_reg(reg, reg, OpWidth::W32),
            OpWidth::W64 => Ok(()),
            OpWidth::W128 => Err(LowerError::UnsupportedOp {
                op: "RISC-V AArch64 normalize W128".into(),
            }),
        }
    }

    fn store_reg_to(&mut self, dst: VReg, src: u8, width: OpWidth) -> Result<(), LowerError> {
        self.normalize_scalar(src, width)?;
        match dst {
            VReg::Imm(_) => {}
            VReg::Virtual(id) => {
                self.emit_ldst_unsigned(src, 31, false, self.virtual_slot(id)?);
            }
            VReg::Arch(ArchReg::RiscV(rv)) => {
                if let Some(offset) = Self::arch_offset(rv)? {
                    self.emit_ldst_unsigned(src, STATE, false, offset);
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

    fn require_no_flags(flags: FlagUpdate, name: &'static str) -> Result<(), LowerError> {
        if flags == FlagUpdate::None {
            Ok(())
        } else {
            Err(LowerError::UnsupportedOp {
                op: format!("RISC-V {name} requests non-architectural flags {flags:?}"),
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
        match op {
            BinOp::Add => self.emit_addsub_reg(ACC, ACC, RHS, false, false, width)?,
            BinOp::Sub => self.emit_addsub_reg(ACC, ACC, RHS, true, false, width)?,
            BinOp::And => self.emit_logic_reg(ACC, ACC, RHS, 0b00, false, width)?,
            BinOp::Or => self.emit_logic_reg(ACC, ACC, RHS, 0b01, false, width)?,
            BinOp::Xor => self.emit_logic_reg(ACC, ACC, RHS, 0b10, false, width)?,
        }
        self.store_reg_to(dst, ACC, width)
    }

    fn lower_shift(
        &mut self,
        dst: VReg,
        src: VReg,
        amount: &SrcOperand,
        width: OpWidth,
        flags: FlagUpdate,
        kind: Shift,
    ) -> Result<(), LowerError> {
        Self::require_no_flags(flags, "shift")?;
        if !matches!(width, OpWidth::W32 | OpWidth::W64) {
            return Err(LowerError::UnsupportedOp {
                op: format!("RISC-V AArch64 shift width {width:?}"),
            });
        }
        self.load_vreg_to(src, ACC, width)?;
        match amount {
            SrcOperand::Imm(value) | SrcOperand::Imm64(value) => {
                let bits = width.bits();
                let mut amount = (*value as u64 & u64::from(bits - 1)) as u32;
                if kind == Shift::Rol {
                    amount = (bits - amount) & (bits - 1);
                }
                match kind {
                    Shift::Lsl if amount != 0 => self.emit_bitfield(
                        ACC,
                        ACC,
                        false,
                        bits - amount,
                        bits - 1 - amount,
                        width,
                    )?,
                    Shift::Lsr if amount != 0 => {
                        self.emit_bitfield(ACC, ACC, false, amount, bits - 1, width)?
                    }
                    Shift::Asr if amount != 0 => {
                        self.emit_bitfield(ACC, ACC, true, amount, bits - 1, width)?
                    }
                    Shift::Ror | Shift::Rol if amount != 0 => {
                        self.emit_extract(ACC, ACC, amount, width)?
                    }
                    _ => {}
                }
            }
            SrcOperand::Reg(reg) => {
                self.load_vreg_to(*reg, RHS, width)?;
                let opcode = match kind {
                    Shift::Lsl => 0b1000,
                    Shift::Lsr => 0b1001,
                    Shift::Asr => 0b1010,
                    Shift::Ror => 0b1011,
                    Shift::Rol => {
                        self.emit_addsub_reg(TMP0, 31, RHS, true, false, width)?;
                        self.emit_dp2(ACC, ACC, TMP0, 0b1011, width)?;
                        return self.store_reg_to(dst, ACC, width);
                    }
                };
                self.emit_dp2(ACC, ACC, RHS, opcode, width)?;
            }
            other => {
                return Err(LowerError::UnsupportedOp {
                    op: format!("RISC-V AArch64 shift amount {other:?}"),
                });
            }
        }
        self.store_reg_to(dst, ACC, width)
    }

    #[allow(clippy::too_many_arguments)]
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
        Self::require_no_flags(flags, "multiply")?;
        if !matches!(width, OpWidth::W32 | OpWidth::W64) {
            return Err(LowerError::UnsupportedOp {
                op: format!("RISC-V AArch64 multiply width {width:?}"),
            });
        }
        self.load_vreg_to(src1, ACC, width)?;
        self.load_src_to(src2, RHS, width)?;
        if width == OpWidth::W64 {
            if let Some(dst_hi) = dst_hi {
                self.emit_dp3(
                    HI,
                    ACC,
                    RHS,
                    31,
                    if signed { 0b010 } else { 0b110 },
                    false,
                    width,
                )?;
                self.emit_dp3(ACC, ACC, RHS, 31, 0, false, width)?;
                self.store_reg_to(dst_lo, ACC, width)?;
                self.store_reg_to(dst_hi, HI, width)?;
            } else {
                self.emit_dp3(ACC, ACC, RHS, 31, 0, false, width)?;
                self.store_reg_to(dst_lo, ACC, width)?;
            }
        } else {
            if signed {
                self.emit_bitfield(ACC, ACC, true, 0, 31, OpWidth::W64)?;
                self.emit_bitfield(RHS, RHS, true, 0, 31, OpWidth::W64)?;
            }
            self.emit_dp3(ACC, ACC, RHS, 31, 0, false, OpWidth::W64)?;
            if let Some(dst_hi) = dst_hi {
                self.emit_bitfield(HI, ACC, false, 32, 63, OpWidth::W64)?;
                self.store_reg_to(dst_lo, ACC, OpWidth::W32)?;
                self.store_reg_to(dst_hi, HI, OpWidth::W32)?;
            } else {
                self.store_reg_to(dst_lo, ACC, OpWidth::W32)?;
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
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
        Self::require_no_flags(flags, "divide")?;
        if !matches!(width, OpWidth::W32 | OpWidth::W64) {
            return Err(LowerError::UnsupportedOp {
                op: format!("RISC-V AArch64 divide width {width:?}"),
            });
        }
        self.load_vreg_to(src1, ACC, width)?;
        self.load_src_to(src2, RHS, width)?;
        if rem.is_some() {
            self.emit_mov_reg(TMP0, ACC, width)?;
        }
        self.emit_dp2(ACC, ACC, RHS, if signed { 0b0011 } else { 0b0010 }, width)?;
        if let Some(rem) = rem {
            self.emit_dp3(HI, ACC, RHS, TMP0, 0, true, width)?; // MSUB
            self.store_reg_to(quot, ACC, width)?;
            self.store_reg_to(rem, HI, width)?;
        } else {
            self.store_reg_to(quot, ACC, width)?;
        }
        Ok(())
    }

    fn lower_popcnt(&mut self, dst: VReg, src: VReg, width: OpWidth) -> Result<(), LowerError> {
        if !matches!(width, OpWidth::W32 | OpWidth::W64) {
            return Err(LowerError::UnsupportedOp {
                op: format!("RISC-V AArch64 POPCNT width {width:?}"),
            });
        }
        self.load_vreg_to(src, ACC, width)?;
        self.emit_bitfield(TMP0, ACC, false, 1, width.bits() - 1, width)?;
        self.emit_mov_imm(TMP1, 0x5555_5555_5555_5555, width)?;
        self.emit_logic_reg(TMP0, TMP0, TMP1, 0, false, width)?;
        self.emit_addsub_reg(ACC, ACC, TMP0, true, false, width)?;
        self.emit_bitfield(TMP0, ACC, false, 2, width.bits() - 1, width)?;
        self.emit_mov_imm(TMP1, 0x3333_3333_3333_3333, width)?;
        self.emit_logic_reg(ACC, ACC, TMP1, 0, false, width)?;
        self.emit_logic_reg(TMP0, TMP0, TMP1, 0, false, width)?;
        self.emit_addsub_reg(ACC, ACC, TMP0, false, false, width)?;
        self.emit_bitfield(TMP0, ACC, false, 4, width.bits() - 1, width)?;
        self.emit_addsub_reg(ACC, ACC, TMP0, false, false, width)?;
        self.emit_mov_imm(TMP1, 0x0f0f_0f0f_0f0f_0f0f, width)?;
        self.emit_logic_reg(ACC, ACC, TMP1, 0, false, width)?;
        for shift in [8, 16, 32] {
            if shift < width.bits() {
                self.emit_bitfield(TMP0, ACC, false, shift, width.bits() - 1, width)?;
                self.emit_addsub_reg(ACC, ACC, TMP0, false, false, width)?;
            }
        }
        self.emit_mov_imm(TMP1, if width == OpWidth::W32 { 0x3f } else { 0x7f }, width)?;
        self.emit_logic_reg(ACC, ACC, TMP1, 0, false, width)?;
        self.store_reg_to(dst, ACC, width)
    }

    fn condition_code(cond: Condition) -> Result<u32, LowerError> {
        match cond {
            Condition::Eq => Ok(0),
            Condition::Ne => Ok(1),
            Condition::Uge => Ok(2),
            Condition::Ult => Ok(3),
            Condition::Negative => Ok(4),
            Condition::Positive => Ok(5),
            Condition::Overflow => Ok(6),
            Condition::NoOverflow => Ok(7),
            Condition::Ugt => Ok(8),
            Condition::Ule => Ok(9),
            Condition::Sge => Ok(10),
            Condition::Slt => Ok(11),
            Condition::Sgt => Ok(12),
            Condition::Sle => Ok(13),
            Condition::Always => Ok(14),
            Condition::Parity | Condition::NoParity => Err(LowerError::UnsupportedOp {
                op: format!("RISC-V AArch64 condition {cond:?}"),
            }),
        }
    }

    fn add_i64_to_reg(&mut self, reg: u8, value: i64) -> Result<(), LowerError> {
        if value == 0 {
            return Ok(());
        }
        self.emit_mov_imm(TMP1, value, OpWidth::W64)?;
        self.emit_addsub_reg(reg, reg, TMP1, false, false, OpWidth::W64)
    }

    fn load_addr_to(&mut self, addr: &Address, dst: u8) -> Result<(), LowerError> {
        match addr {
            Address::Direct(base) => self.load_vreg_to(*base, dst, OpWidth::W64)?,
            Address::BaseOffset { base, offset, .. } => {
                self.load_vreg_to(*base, dst, OpWidth::W64)?;
                self.add_i64_to_reg(dst, *offset)?;
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
                    self.emit_mov_imm(dst, 0, OpWidth::W64)?;
                }
                self.load_vreg_to(*index, TMP0, OpWidth::W64)?;
                match scale {
                    1 => {}
                    2 | 4 | 8 => self.emit_bitfield(
                        TMP0,
                        TMP0,
                        false,
                        64 - scale.trailing_zeros(),
                        63 - scale.trailing_zeros(),
                        OpWidth::W64,
                    )?,
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: format!("RISC-V AArch64 memory scale {scale}"),
                        });
                    }
                }
                self.emit_addsub_reg(dst, dst, TMP0, false, false, OpWidth::W64)?;
                self.add_i64_to_reg(dst, i64::from(*disp))?;
            }
            Address::Absolute(value) => self.emit_mov_imm(dst, *value as i64, OpWidth::W64)?,
            Address::PcRel { offset, base, .. } => self.emit_mov_imm(
                dst,
                base.unwrap_or(0).wrapping_add(*offset as u64) as i64,
                OpWidth::W64,
            )?,
            other => {
                return Err(LowerError::UnsupportedOp {
                    op: format!("RISC-V AArch64 memory address {other:?}"),
                });
            }
        }
        Ok(())
    }

    fn emit_cond_branch_placeholder(&mut self, cond: u32) -> usize {
        let offset = self.code.position();
        self.emit(0x5400_0000 | cond);
        offset
    }

    fn patch_cond_branch_to_current(&mut self, offset: usize, cond: u32) -> Result<(), LowerError> {
        let target = self.code.position();
        let delta = target as i64 - offset as i64;
        if delta % 4 != 0 || !(-(1 << 20)..(1 << 20)).contains(&delta) {
            return Err(LowerError::RelocationOutOfRange { offset, target });
        }
        let imm19 = ((delta / 4) as u32) & 0x7ffff;
        self.code
            .patch_i32(offset, (0x5400_0000 | (imm19 << 5) | cond) as i32);
        Ok(())
    }

    fn emit_arch_exit(&mut self, pc: u64, reason: i64) -> Result<(), LowerError> {
        self.emit_mov_imm(ACC, pc as i64, OpWidth::W64)?;
        self.emit_mov_imm(RHS, reason, OpWidth::W64)?;
        self.emit_ldst_unsigned(ACC, STATE, false, RV_PC_OFFSET);
        self.emit_ldst_unsigned(RHS, STATE, false, RV_EXIT_REASON_OFFSET);
        self.emit_epilogue()
    }

    fn emit_trap_unless_one(&mut self, status: u8, pc: u64) -> Result<(), LowerError> {
        self.emit_addsub_imm(31, status, 1, true, true, OpWidth::W64)?; // cmp status,#1
        let success = self.emit_cond_branch_placeholder(0); // b.eq
        self.emit_arch_exit(pc, EXIT_TRAP)?;
        self.patch_cond_branch_to_current(success, 0)
    }

    fn lower_load(
        &mut self,
        dst: VReg,
        addr: &Address,
        width: MemWidth,
        sign: SignExtend,
        pc: u64,
    ) -> Result<(), LowerError> {
        let size = match width {
            MemWidth::B1 | MemWidth::B2 | MemWidth::B4 | MemWidth::B8 => width.bytes(),
            _ => {
                return Err(LowerError::UnsupportedOp {
                    op: format!("RISC-V AArch64 scalar load width {width:?}"),
                });
            }
        };
        self.load_addr_to(addr, ADDR)?;
        self.emit_ldst_unsigned(0, STATE, true, RV_CTX_OFFSET);
        self.emit_mov_imm(2, i64::from(size), OpWidth::W64)?;
        self.emit_mov_imm(3, i64::from(matches!(sign, SignExtend::Sign)), OpWidth::W64)?;
        self.emit_ldst_unsigned(TARGET, STATE, true, RV_LOAD_FN_OFFSET);
        self.emit_blr(TARGET);
        self.emit_trap_unless_one(1, pc)?;
        self.store_reg_to(dst, 0, OpWidth::W64)
    }

    fn lower_store(
        &mut self,
        src: VReg,
        addr: &Address,
        width: MemWidth,
        pc: u64,
    ) -> Result<(), LowerError> {
        let op_width = width
            .to_op_width()
            .ok_or_else(|| LowerError::UnsupportedOp {
                op: format!("RISC-V AArch64 scalar store width {width:?}"),
            })?;
        if op_width == OpWidth::W128 {
            return Err(LowerError::UnsupportedOp {
                op: "RISC-V AArch64 scalar store width B16".into(),
            });
        }
        self.load_addr_to(addr, ADDR)?;
        self.load_vreg_to(src, 2, op_width)?;
        self.emit_ldst_unsigned(0, STATE, true, RV_CTX_OFFSET);
        self.emit_mov_imm(3, i64::from(width.bytes()), OpWidth::W64)?;
        self.emit_ldst_unsigned(TARGET, STATE, true, RV_STORE_FN_OFFSET);
        self.emit_blr(TARGET);
        self.emit_trap_unless_one(0, pc)
    }

    fn scalar_atomic_width(width: MemWidth) -> Result<(OpWidth, u32), LowerError> {
        match width {
            MemWidth::B4 => Ok((OpWidth::W32, 4)),
            MemWidth::B8 => Ok((OpWidth::W64, 8)),
            _ => Err(LowerError::UnsupportedOp {
                op: format!("RISC-V AArch64 atomic width {width:?}"),
            }),
        }
    }

    fn atomic_op_code(op: AtomicOp) -> i64 {
        (match op {
            AtomicOp::Add => RiscVAtomicOpCode::Add,
            AtomicOp::Sub => RiscVAtomicOpCode::Sub,
            AtomicOp::Neg => RiscVAtomicOpCode::Neg,
            AtomicOp::And => RiscVAtomicOpCode::And,
            AtomicOp::Or => RiscVAtomicOpCode::Or,
            AtomicOp::Xor => RiscVAtomicOpCode::Xor,
            AtomicOp::Nand => RiscVAtomicOpCode::Nand,
            AtomicOp::Max => RiscVAtomicOpCode::Max,
            AtomicOp::Min => RiscVAtomicOpCode::Min,
            AtomicOp::Umax => RiscVAtomicOpCode::Umax,
            AtomicOp::Umin => RiscVAtomicOpCode::Umin,
            AtomicOp::Swap => RiscVAtomicOpCode::Swap,
        }) as i64
    }

    fn memory_order_code(order: MemoryOrder) -> i64 {
        (match order {
            MemoryOrder::Relaxed => RiscVMemoryOrderCode::Relaxed,
            MemoryOrder::Acquire => RiscVMemoryOrderCode::Acquire,
            MemoryOrder::Release => RiscVMemoryOrderCode::Release,
            MemoryOrder::AcqRel => RiscVMemoryOrderCode::AcqRel,
            MemoryOrder::SeqCst => RiscVMemoryOrderCode::SeqCst,
        }) as i64
    }

    #[allow(clippy::too_many_arguments)]
    fn lower_atomic_rmw(
        &mut self,
        dst: VReg,
        addr: &Address,
        src: VReg,
        op: AtomicOp,
        width: MemWidth,
        order: MemoryOrder,
        pc: u64,
    ) -> Result<(), LowerError> {
        let (op_width, size) = Self::scalar_atomic_width(width)?;
        self.load_addr_to(addr, ADDR)?;
        self.load_vreg_to(src, 2, op_width)?;
        self.emit_ldst_unsigned(0, STATE, true, RV_CTX_OFFSET);
        self.emit_mov_imm(3, i64::from(size), OpWidth::W64)?;
        self.emit_mov_imm(4, Self::atomic_op_code(op), OpWidth::W64)?;
        self.emit_mov_imm(5, Self::memory_order_code(order), OpWidth::W64)?;
        self.emit_ldst_unsigned(TARGET, STATE, true, RV_ATOMIC_RMW_FN_OFFSET);
        self.emit_blr(TARGET);
        self.emit_trap_unless_one(1, pc)?;
        self.store_reg_to(dst, 0, OpWidth::W64)
    }

    #[allow(clippy::too_many_arguments)]
    fn lower_cas(
        &mut self,
        dst: VReg,
        success: VReg,
        addr: &Address,
        expected: VReg,
        new_val: VReg,
        width: MemWidth,
        order: MemoryOrder,
        pc: u64,
    ) -> Result<(), LowerError> {
        let (op_width, size) = Self::scalar_atomic_width(width)?;
        self.load_addr_to(addr, ADDR)?;
        self.load_vreg_to(expected, 2, op_width)?;
        self.load_vreg_to(new_val, 3, op_width)?;
        self.emit_ldst_unsigned(0, STATE, true, RV_CTX_OFFSET);
        self.emit_mov_imm(4, i64::from(size), OpWidth::W64)?;
        self.emit_mov_imm(5, Self::memory_order_code(order), OpWidth::W64)?;
        self.emit_ldst_unsigned(TARGET, STATE, true, RV_CAS_FN_OFFSET);
        self.emit_blr(TARGET);

        // Status is 0=fault, 1=compare failed, 2=swapped. Subtract one to
        // obtain the SMIR Boolean and accept exactly the closed interval 0..1.
        self.emit_addsub_imm(1, 1, 1, true, false, OpWidth::W64)?;
        self.emit_addsub_imm(31, 1, 1, true, true, OpWidth::W64)?;
        let valid = self.emit_cond_branch_placeholder(Self::condition_code(Condition::Ule)?);
        self.emit_arch_exit(pc, EXIT_TRAP)?;
        self.patch_cond_branch_to_current(valid, Self::condition_code(Condition::Ule)?)?;
        self.store_reg_to(dst, 0, OpWidth::W64)?;
        self.store_reg_to(success, 1, OpWidth::W64)
    }

    #[allow(clippy::too_many_arguments)]
    fn lower_cas_pair(
        &mut self,
        dst_lo: VReg,
        dst_hi: VReg,
        success: VReg,
        addr: &Address,
        expected_lo: VReg,
        expected_hi: VReg,
        new_lo: VReg,
        new_hi: VReg,
        order: MemoryOrder,
        failure_order: MemoryOrder,
        pc: u64,
    ) -> Result<(), LowerError> {
        let required_failure_order = match order {
            MemoryOrder::Relaxed | MemoryOrder::Release => MemoryOrder::Relaxed,
            MemoryOrder::Acquire | MemoryOrder::AcqRel | MemoryOrder::SeqCst => {
                MemoryOrder::Acquire
            }
        };
        if failure_order != required_failure_order {
            return Err(LowerError::UnsupportedOp {
                op: format!(
                    "RISC-V AArch64 pair CAS failure order {failure_order:?} for {order:?}"
                ),
            });
        }

        self.load_addr_to(addr, ADDR)?;
        self.load_vreg_to(expected_lo, 2, OpWidth::W64)?;
        self.load_vreg_to(expected_hi, 3, OpWidth::W64)?;
        self.load_vreg_to(new_lo, 4, OpWidth::W64)?;
        self.load_vreg_to(new_hi, 5, OpWidth::W64)?;
        self.emit_mov_imm(6, Self::memory_order_code(order), OpWidth::W64)?;
        self.emit_mov_imm(7, Self::memory_order_code(failure_order), OpWidth::W64)?;
        self.emit_ldst_unsigned(0, STATE, true, RV_CTX_OFFSET);
        self.emit_ldst_unsigned(TARGET, STATE, true, RV_CAS_PAIR_FN_OFFSET);

        // AAPCS64 passes argument nine on the stack. Reserve one pointer slot
        // plus an independent high-result slot while retaining 16-byte SP
        // alignment. The helper returns old_lo/status in X0/X1.
        self.emit_addsub_imm(31, 31, 16, true, false, OpWidth::W64)?;
        self.emit_addsub_imm(TMP0, 31, 8, false, false, OpWidth::W64)?;
        self.emit_ldst_unsigned(TMP0, 31, false, 0);
        self.emit_mov_imm(TMP1, 0, OpWidth::W64)?;
        self.emit_ldst_unsigned(TMP1, 31, false, 8);
        self.emit_blr(TARGET);
        self.emit_ldst_unsigned(RHS, 31, true, 8);
        self.emit_addsub_imm(31, 31, 16, false, false, OpWidth::W64)?;

        self.emit_addsub_imm(1, 1, 1, true, false, OpWidth::W64)?;
        self.emit_addsub_imm(31, 1, 1, true, true, OpWidth::W64)?;
        let valid = self.emit_cond_branch_placeholder(Self::condition_code(Condition::Ule)?);
        self.emit_arch_exit(pc, EXIT_TRAP)?;
        self.patch_cond_branch_to_current(valid, Self::condition_code(Condition::Ule)?)?;
        self.store_reg_to(dst_lo, 0, OpWidth::W64)?;
        self.store_reg_to(dst_hi, RHS, OpWidth::W64)?;
        self.store_reg_to(success, 1, OpWidth::W64)
    }

    fn lower_load_exclusive(
        &mut self,
        dst: VReg,
        addr: &Address,
        width: MemWidth,
        pc: u64,
    ) -> Result<(), LowerError> {
        let (_, size) = Self::scalar_atomic_width(width)?;
        self.load_addr_to(addr, ADDR)?;
        self.emit_ldst_unsigned(0, STATE, true, RV_CTX_OFFSET);
        self.emit_mov_imm(2, i64::from(size), OpWidth::W64)?;
        self.emit_ldst_unsigned(TARGET, STATE, true, RV_LOAD_EXCLUSIVE_FN_OFFSET);
        self.emit_blr(TARGET);
        self.emit_trap_unless_one(1, pc)?;
        self.store_reg_to(dst, 0, OpWidth::W64)
    }

    fn lower_store_exclusive(
        &mut self,
        status: VReg,
        src: VReg,
        addr: &Address,
        width: MemWidth,
        pc: u64,
    ) -> Result<(), LowerError> {
        let (op_width, size) = Self::scalar_atomic_width(width)?;
        self.load_addr_to(addr, ADDR)?;
        self.load_vreg_to(src, 2, op_width)?;
        self.emit_ldst_unsigned(0, STATE, true, RV_CTX_OFFSET);
        self.emit_mov_imm(3, i64::from(size), OpWidth::W64)?;
        self.emit_ldst_unsigned(TARGET, STATE, true, RV_STORE_EXCLUSIVE_FN_OFFSET);
        self.emit_blr(TARGET);
        self.emit_trap_unless_one(1, pc)?;
        self.emit_addsub_imm(31, 0, 0, true, true, OpWidth::W64)?;
        self.emit_cond_select(
            ACC,
            31,
            31,
            Self::condition_code(Condition::Eq)? ^ 1,
            true,
            OpWidth::W64,
        )?;
        self.store_reg_to(status, ACC, OpWidth::W64)
    }

    fn lower_clear_exclusive(&mut self) -> Result<(), LowerError> {
        self.emit_ldst_unsigned(0, STATE, true, RV_CTX_OFFSET);
        self.emit_ldst_unsigned(TARGET, STATE, true, RV_CLEAR_EXCLUSIVE_FN_OFFSET);
        self.emit_blr(TARGET);
        Ok(())
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
                    op: format!("RISC-V AArch64 integer-crypto operation {other:?}"),
                });
            }
        };
        Ok(code as i64)
    }

    #[allow(clippy::too_many_arguments)]
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
                op: format!("RISC-V AArch64 integer-crypto XLEN {xlen}"),
            });
        }
        self.load_vreg_to(src1, 1, OpWidth::W64)?;
        self.load_vreg_to(src2, 2, OpWidth::W64)?;
        self.emit_mov_imm(0, Self::int_crypto_op_code(op)?, OpWidth::W64)?;
        self.emit_mov_imm(3, i64::from(imm), OpWidth::W64)?;
        self.emit_mov_imm(4, i64::from(xlen), OpWidth::W64)?;
        self.emit_ldst_unsigned(TARGET, STATE, true, RV_INT_CRYPTO_FN_OFFSET);
        self.emit_blr(TARGET);
        self.store_reg_to(dst, 0, OpWidth::W64)
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
        pc: u64,
    ) -> Result<(), LowerError> {
        if !matches!(xlen, 32 | 64) || rm_field > 7 {
            return Err(LowerError::UnsupportedOp {
                op: format!("RISC-V AArch64 scalar-FP parameters XLEN={xlen}, rm={rm_field}"),
            });
        }
        if xlen == 32 && crate::isa::riscv::float::fp_requires_rv64(op) {
            return Err(LowerError::UnsupportedOp {
                op: format!("RISC-V AArch64 RV64-only scalar-FP operation {op:?}"),
            });
        }
        let op_code = RiscVFpOpCode::from_op(op).ok_or_else(|| LowerError::UnsupportedOp {
            op: format!("RISC-V AArch64 scalar-FP operation {op:?}"),
        })? as i64;

        self.load_vreg_to(src1, 3, OpWidth::W64)?;
        self.load_vreg_to(src2, 4, OpWidth::W64)?;
        self.load_vreg_to(src3, 5, OpWidth::W64)?;
        self.load_vreg_to(fcsr_src, 2, OpWidth::W64)?;
        self.emit_mov_imm(0, op_code, OpWidth::W64)?;
        self.emit_mov_imm(1, i64::from(rm_field), OpWidth::W64)?;
        self.emit_ldst_unsigned(TARGET, STATE, true, RV_FP_FN_OFFSET);
        self.emit_blr(TARGET);

        self.emit_mov_imm(TMP0, RISCV_FP_RESULT_INVALID as i64, OpWidth::W64)?;
        self.emit_addsub_reg(31, 1, TMP0, true, true, OpWidth::W64)?;
        let valid = self.emit_cond_branch_placeholder(Self::condition_code(Condition::Ne)?);
        self.emit_arch_exit(pc, EXIT_TRAP)?;
        self.patch_cond_branch_to_current(valid, Self::condition_code(Condition::Ne)?)?;
        if xlen == 32 && crate::isa::riscv::float::fp_writes_int_dst(op) {
            self.emit_mov_reg(0, 0, OpWidth::W32)?;
        }
        self.store_reg_to(dst, 0, OpWidth::W64)?;
        self.store_reg_to(fcsr_dst, 1, OpWidth::W64)
    }

    fn lower_rv_vector(
        &mut self,
        insn: u32,
        xlen: u8,
        state: &crate::smir::ir::ops::RvVectorState,
        pc: u64,
    ) -> Result<(), LowerError> {
        if !matches!(xlen, 32 | 64) {
            return Err(LowerError::UnsupportedOp {
                op: format!("RISC-V AArch64 Vector XLEN {xlen}"),
            });
        }

        // Publish the exact SSA snapshot observed by the opaque helper.
        for (index, src) in state.x_srcs.iter().copied().enumerate() {
            self.load_vreg_to(src, ACC, OpWidth::W64)?;
            self.emit_ldst_unsigned(ACC, STATE, false, RV_X_OFFSET + index as u32 * 8);
        }
        for (index, src) in state.f_srcs.iter().copied().enumerate() {
            self.load_vreg_to(src, ACC, OpWidth::W64)?;
            self.emit_ldst_unsigned(ACC, STATE, false, RV_F_OFFSET + index as u32 * 8);
        }
        for (src, offset) in [
            (state.fcsr_src, RV_FCSR_OFFSET),
            (state.vl_src, RV_VL_OFFSET),
            (state.vtype_src, RV_VTYPE_OFFSET),
            (state.vstart_src, RV_VSTART_OFFSET),
            (state.vcsr_src, RV_VCSR_OFFSET),
        ] {
            self.load_vreg_to(src, ACC, OpWidth::W64)?;
            self.emit_ldst_unsigned(ACC, STATE, false, offset);
        }

        self.emit_mov_reg(0, STATE, OpWidth::W64)?;
        self.emit_mov_imm(1, i64::from(insn), OpWidth::W64)?;
        self.emit_mov_imm(2, i64::from(xlen), OpWidth::W64)?;
        self.emit_ldst_unsigned(TARGET, STATE, true, RV_VECTOR_FN_OFFSET);
        self.emit_blr(TARGET);
        self.emit_trap_unless_one(0, pc)?;

        // Import results only after exact success, preserving transactional
        // failure semantics for both architectural and virtual destinations.
        for (index, dst) in state.x_dsts.iter().copied().enumerate().skip(1) {
            self.emit_ldst_unsigned(ACC, STATE, true, RV_X_OFFSET + index as u32 * 8);
            self.store_reg_to(dst, ACC, OpWidth::W64)?;
        }
        for (index, dst) in state.f_dsts.iter().copied().enumerate() {
            self.emit_ldst_unsigned(ACC, STATE, true, RV_F_OFFSET + index as u32 * 8);
            self.store_reg_to(dst, ACC, OpWidth::W64)?;
        }
        for (dst, offset) in [
            (state.fcsr_dst, RV_FCSR_OFFSET),
            (state.vl_dst, RV_VL_OFFSET),
            (state.vtype_dst, RV_VTYPE_OFFSET),
            (state.vstart_dst, RV_VSTART_OFFSET),
            (state.vcsr_dst, RV_VCSR_OFFSET),
        ] {
            self.emit_ldst_unsigned(ACC, STATE, true, offset);
            self.store_reg_to(dst, ACC, OpWidth::W64)?;
        }
        Ok(())
    }

    fn lower_op(&mut self, op: &SmirOp) -> Result<(), LowerError> {
        match &op.kind {
            OpKind::Nop => self.emit(0xd503_201f),
            OpKind::Fence { .. } => self.emit(0xd503_3bbf), // dmb ish
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
                self.emit_logic_reg(ACC, ACC, RHS, 0, true, *width)?;
                self.store_reg_to(*dst, ACC, *width)?;
            }
            OpKind::Not { dst, src, width } => {
                self.load_vreg_to(*src, ACC, *width)?;
                self.emit_logic_reg(ACC, 31, ACC, 0b01, true, *width)?;
                self.store_reg_to(*dst, ACC, *width)?;
            }
            OpKind::Shl {
                dst,
                src,
                amount,
                width,
                flags,
            } => self.lower_shift(*dst, *src, amount, *width, *flags, Shift::Lsl)?,
            OpKind::Shr {
                dst,
                src,
                amount,
                width,
                flags,
            } => self.lower_shift(*dst, *src, amount, *width, *flags, Shift::Lsr)?,
            OpKind::Sar {
                dst,
                src,
                amount,
                width,
                flags,
            } => self.lower_shift(*dst, *src, amount, *width, *flags, Shift::Asr)?,
            OpKind::Ror {
                dst,
                src,
                amount,
                width,
                flags,
            } => self.lower_shift(*dst, *src, amount, *width, *flags, Shift::Ror)?,
            OpKind::Rol {
                dst,
                src,
                amount,
                width,
                flags,
            } => self.lower_shift(*dst, *src, amount, *width, *flags, Shift::Rol)?,
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
                self.emit_addsub_reg(31, ACC, RHS, true, true, *width)?;
            }
            OpKind::SetCC { dst, cond, .. } | OpKind::TestCondition { dst, cond } => {
                if *cond == Condition::Always {
                    self.emit_mov_imm(ACC, 1, OpWidth::W64)?;
                } else {
                    let code = Self::condition_code(*cond)?;
                    self.emit_cond_select(ACC, 31, 31, code ^ 1, true, OpWidth::W64)?;
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
                self.emit_addsub_imm(31, TMP0, 0, true, true, OpWidth::W64)?;
                self.emit_cond_select(ACC, RHS, ACC, 1, false, *width)?;
                self.store_reg_to(*dst, ACC, *width)?;
            }
            OpKind::ZeroExtend {
                dst,
                src,
                from_width,
                to_width,
            } => {
                self.load_vreg_to(*src, ACC, *to_width)?;
                if *from_width != OpWidth::W64 {
                    self.emit_bitfield(ACC, ACC, false, 0, from_width.bits() - 1, *to_width)?;
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
                if *from_width != OpWidth::W64 {
                    self.emit_bitfield(ACC, ACC, true, 0, from_width.bits() - 1, *to_width)?;
                }
                self.store_reg_to(*dst, ACC, *to_width)?;
            }
            OpKind::Clz { dst, src, width } => {
                self.load_vreg_to(*src, ACC, *width)?;
                self.emit_dp1(ACC, ACC, 0b000100, *width)?;
                self.store_reg_to(*dst, ACC, *width)?;
            }
            OpKind::Ctz { dst, src, width } => {
                self.load_vreg_to(*src, ACC, *width)?;
                self.emit_dp1(ACC, ACC, 0b000000, *width)?;
                self.emit_dp1(ACC, ACC, 0b000100, *width)?;
                self.store_reg_to(*dst, ACC, *width)?;
            }
            OpKind::Popcnt { dst, src, width } => self.lower_popcnt(*dst, *src, *width)?,
            OpKind::Bswap { dst, src, width } => {
                if !matches!(width, OpWidth::W32 | OpWidth::W64) {
                    return Err(LowerError::UnsupportedOp {
                        op: format!("RISC-V AArch64 BSWAP width {width:?}"),
                    });
                }
                self.load_vreg_to(*src, ACC, *width)?;
                self.emit_dp1(
                    ACC,
                    ACC,
                    if *width == OpWidth::W32 {
                        0b000010
                    } else {
                        0b000011
                    },
                    *width,
                )?;
                self.store_reg_to(*dst, ACC, *width)?;
            }
            OpKind::Rbit { dst, src, width } => {
                self.load_vreg_to(*src, ACC, *width)?;
                self.emit_dp1(ACC, ACC, 0b000000, *width)?;
                self.store_reg_to(*dst, ACC, *width)?;
            }
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
            OpKind::CasPair {
                dst_lo,
                dst_hi,
                success,
                addr,
                expected_lo,
                expected_hi,
                new_lo,
                new_hi,
                order,
                failure_order,
            } => self.lower_cas_pair(
                *dst_lo,
                *dst_hi,
                *success,
                addr,
                *expected_lo,
                *expected_hi,
                *new_lo,
                *new_hi,
                *order,
                *failure_order,
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
            OpKind::ClearExclusive => self.lower_clear_exclusive()?,
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
            OpKind::Breakpoint => self.emit_arch_exit(op.guest_pc, EXIT_BREAKPOINT)?,
            OpKind::Syscall { .. } => self.emit_arch_exit(op.guest_pc, EXIT_SYSCALL)?,
            other => {
                return Err(LowerError::UnsupportedOp {
                    op: format!("RISC-V state-backed AArch64 lowering for {other:?}"),
                });
            }
        }
        Ok(())
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
                self.emit(0x1400_0000);
                self.branch_fixups.push(BranchFixup {
                    offset,
                    target: *target,
                    nonzero_reg: None,
                });
            }
            Terminator::CondBranch {
                cond,
                true_target,
                false_target,
            } => {
                self.load_vreg_to(*cond, ACC, OpWidth::W64)?;
                let cbnz = self.code.position();
                self.emit(0xb500_0000 | u32::from(ACC));
                self.branch_fixups.push(BranchFixup {
                    offset: cbnz,
                    target: *true_target,
                    nonzero_reg: Some(ACC),
                });
                let branch = self.code.position();
                self.emit(0x1400_0000);
                self.branch_fixups.push(BranchFixup {
                    offset: branch,
                    target: *false_target,
                    nonzero_reg: None,
                });
            }
            Terminator::IndirectBranch { target, .. } => {
                self.load_vreg_to(*target, ACC, OpWidth::W64)?;
                self.emit_mov_imm(RHS, EXIT_RETURN, OpWidth::W64)?;
                self.emit_ldst_unsigned(ACC, STATE, false, RV_PC_OFFSET);
                self.emit_ldst_unsigned(RHS, STATE, false, RV_EXIT_REASON_OFFSET);
                self.emit_epilogue()?;
            }
            Terminator::Return { .. } => {
                self.emit_arch_exit(self.return_pc(block), EXIT_RETURN)?;
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
                self.emit_arch_exit(pc, reason)?;
            }
            Terminator::Unreachable => self.emit_arch_exit(block.guest_pc, EXIT_TRAP)?,
            other => {
                return Err(LowerError::UnsupportedOp {
                    op: format!("RISC-V state-backed AArch64 terminator {other:?}"),
                });
            }
        }
        Ok(())
    }

    fn lower_block(&mut self, block: &SmirBlock) -> Result<(), LowerError> {
        if !block.phis.is_empty() {
            return Err(LowerError::UnsupportedOp {
                op: format!("RISC-V AArch64 block {:?} contains phi nodes", block.id),
            });
        }
        self.block_offsets.insert(block.id, self.code.position());
        for op in &block.ops {
            self.lower_op(op)?;
        }
        self.lower_terminator(block)
    }

    fn fixup_branches(&mut self) -> Result<(), LowerError> {
        for fixup in self.branch_fixups.drain(..).collect::<Vec<_>>() {
            let Some(&target) = self.block_offsets.get(&fixup.target) else {
                return Err(LowerError::UndefinedLabel {
                    label: format!("block_{}", fixup.target.0),
                });
            };
            let delta = target as i64 - fixup.offset as i64;
            if delta % 4 != 0 {
                return Err(LowerError::RelocationOutOfRange {
                    offset: fixup.offset,
                    target,
                });
            }
            let word = if let Some(reg) = fixup.nonzero_reg {
                let imm = delta / 4;
                if !(-(1 << 18)..(1 << 18)).contains(&imm) {
                    return Err(LowerError::RelocationOutOfRange {
                        offset: fixup.offset,
                        target,
                    });
                }
                0xb500_0000 | (((imm as u32) & 0x7ffff) << 5) | u32::from(reg)
            } else {
                let imm = delta / 4;
                if !(-(1 << 25)..(1 << 25)).contains(&imm) {
                    return Err(LowerError::RelocationOutOfRange {
                        offset: fixup.offset,
                        target,
                    });
                }
                0x1400_0000 | ((imm as u32) & 0x03ff_ffff)
            };
            self.code.patch_i32(fixup.offset, word as i32);
        }
        Ok(())
    }
}

impl Default for RiscVAarch64Lowerer {
    fn default() -> Self {
        Self::new()
    }
}

impl SmirLowerer for RiscVAarch64Lowerer {
    fn target_arch(&self) -> &'static str {
        "aarch64"
    }

    fn lower_function(&mut self, func: &SmirFunction) -> Result<LowerResult, LowerError> {
        self.code.clear();
        self.block_offsets.clear();
        self.branch_fixups.clear();
        self.relocations.clear();
        self.collect_virtuals(func)?;

        let entry_offset = self.code.position();
        self.emit_prologue()?;
        let Some(entry) = func.get_block(func.entry) else {
            return Err(LowerError::UndefinedLabel {
                label: format!("entry block {}", func.entry.0),
            });
        };
        self.lower_block(entry)?;
        for block in &func.blocks {
            if block.id != func.entry {
                self.lower_block(block)?;
            }
        }
        self.fixup_branches()?;

        Ok(LowerResult {
            code_size: self.code.len(),
            entry_offset,
            block_offsets: self.block_offsets.clone(),
            relocations: self.relocations.clone(),
            stack_size: self.frame_size + 32,
        })
    }

    fn code_buffer(&self) -> &CodeBuffer {
        &self.code
    }

    fn finalize(&mut self) -> Result<Vec<u8>, LowerError> {
        Ok(self.code.as_slice().to_vec())
    }
}

fn align16(value: usize) -> usize {
    (value + 15) & !15
}
