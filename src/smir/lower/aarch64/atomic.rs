//! Atomic / LSE memory-ordering lowering

use crate::smir::lower::aarch64::*;
use std::collections::HashMap;

use crate::smir::ir::flags::{FlagSet, FlagUpdate};
use crate::smir::ir::ops::{
    ArmDpRegShiftKind, OpKind, SmirOp, X86AdxKind, X86BlsKind, X86CountKind,
};
use crate::smir::ir::types::{
    Address, ArchReg, ArmReg, AtomicOp, Avx10FP16Op, BlockId, Condition, ExtendOp, FenceKind,
    FpPrecision, FpRoundMode, MemWidth, MemoryOrder, OpWidth, ShiftOp, SignExtend, SrcOperand,
    VLaneOp, VReg, VecElementType, VecPermuteKind, VecReduceOp, VecUnaryOp, VecWidth,
};
use crate::smir::ir::{CallTarget, SmirBlock, SmirFunction, Terminator, TrapKind};

use super::{CodeBuffer, LowerError, LowerResult, Relocation, SmirLowerer};

impl Aarch64Lowerer {

    pub(crate) fn emit_atomic_load(&mut self, rt: u8, rn: u8, size: u32) {
        self.emit(
            (size << 30)
                | (0b001000 << 24)
                | (1 << 23)
                | (1 << 22)
                | (0b11111 << 16)
                | (1 << 15)
                | (0b11111 << 10)
                | ((rn as u32) << 5)
                | (rt as u32),
        );
    }


    pub(crate) fn emit_atomic_store(&mut self, rt: u8, rn: u8, size: u32) {
        self.emit(
            (size << 30)
                | (0b001000 << 24)
                | (1 << 23)
                | (0b11111 << 16)
                | (1 << 15)
                | (0b11111 << 10)
                | ((rn as u32) << 5)
                | (rt as u32),
        );
    }


    pub(crate) fn emit_atomic_rmw(
        &mut self,
        rt: u8,
        rn: u8,
        rs: u8,
        size: u32,
        acquire: u32,
        release: u32,
        o3: u32,
        opc: u32,
    ) {
        self.emit(
            (size << 30)
                | (0b111 << 27)
                | (acquire << 23)
                | (release << 22)
                | (1 << 21)
                | ((rs as u32) << 16)
                | (o3 << 15)
                | (opc << 12)
                | ((rn as u32) << 5)
                | (rt as u32),
        );
    }


    pub(crate) fn emit_cas(&mut self, rs: u8, rt: u8, rn: u8, size: u32, acquire: u32, release: u32) {
        self.emit(
            (size << 30)
                | (0b001000 << 24)
                | (1 << 23)
                | (acquire << 22)
                | (1 << 21)
                | ((rs as u32) << 16)
                | (release << 15)
                | (0b11111 << 10)
                | ((rn as u32) << 5)
                | (rt as u32),
        );
    }


    pub(crate) fn lower_atomic_addr_to_base(
        &mut self,
        avoid: &[u8],
        addr: &Address,
    ) -> Result<(Vec<u8>, u8), LowerError> {
        match addr {
            Address::Direct(base) => {
                let base_reg = Self::base_gpr(*base)?;
                if base_reg == 31 {
                    self.lower_base_offset_to_scratch(avoid, *base, 0)
                } else {
                    Ok((Vec::new(), base_reg))
                }
            }
            Address::BaseOffset { base, offset, .. } if *offset == 0 => {
                let base_reg = Self::base_gpr(*base)?;
                if base_reg == 31 {
                    self.lower_base_offset_to_scratch(avoid, *base, 0)
                } else {
                    Ok((Vec::new(), base_reg))
                }
            }
            Address::BaseOffset { base, offset, .. } => {
                self.lower_base_offset_to_scratch(avoid, *base, *offset)
            }
            Address::BaseIndexScale {
                base,
                index,
                scale,
                disp,
                ..
            } => self.lower_base_index_scale_to_scratch(avoid, *base, *index, *scale, *disp),
            other => Err(LowerError::UnsupportedOp {
                op: format!("AArch64 native atomic memory address {other:?}"),
            }),
        }
    }


    pub(crate) fn atomic_order_bits(order: MemoryOrder) -> (u32, u32) {
        match order {
            MemoryOrder::Relaxed => (0, 0),
            MemoryOrder::Acquire => (1, 0),
            MemoryOrder::Release => (0, 1),
            MemoryOrder::AcqRel | MemoryOrder::SeqCst => (1, 1),
        }
    }


    pub(crate) fn lower_atomic_load(
        &mut self,
        dst: VReg,
        addr: &Address,
        width: MemWidth,
        order: MemoryOrder,
    ) -> Result<(), LowerError> {
        let rt = Self::dst_gpr_arm_or_x86(dst)?;
        let size = Self::mem_size(width)?;
        match order {
            MemoryOrder::Relaxed => self.lower_mem_access(rt, addr, size, 0b01),
            MemoryOrder::Acquire | MemoryOrder::SeqCst => {
                let (scratches, rn) = self.lower_atomic_addr_to_base(&[rt], addr)?;
                self.emit_atomic_load(rt, rn, size);
                self.emit_scratch_restore(&scratches);
                Ok(())
            }
            MemoryOrder::Release | MemoryOrder::AcqRel => Err(LowerError::UnsupportedOp {
                op: format!("AArch64 native atomic load order {order:?}"),
            }),
        }
    }


    pub(crate) fn lower_atomic_store(
        &mut self,
        src: VReg,
        addr: &Address,
        width: MemWidth,
        order: MemoryOrder,
    ) -> Result<(), LowerError> {
        let rt = Self::gpr_arm_or_x86(src)?;
        let size = Self::mem_size(width)?;
        match order {
            MemoryOrder::Relaxed => self.lower_mem_access(rt, addr, size, 0b00),
            MemoryOrder::Release | MemoryOrder::SeqCst => {
                let (scratches, rn) = self.lower_atomic_addr_to_base(&[rt], addr)?;
                self.emit_atomic_store(rt, rn, size);
                self.emit_scratch_restore(&scratches);
                Ok(())
            }
            MemoryOrder::Acquire | MemoryOrder::AcqRel => Err(LowerError::UnsupportedOp {
                op: format!("AArch64 native atomic store order {order:?}"),
            }),
        }
    }


    pub(crate) fn atomic_rmw_op_encoding(op: AtomicOp, src: VReg) -> Result<(u32, u32), LowerError> {
        match op {
            AtomicOp::Add => Ok((0, 0b000)),
            AtomicOp::Xor => Ok((0, 0b010)),
            AtomicOp::Or => Ok((0, 0b011)),
            AtomicOp::Max => Ok((0, 0b100)),
            AtomicOp::Min => Ok((0, 0b101)),
            AtomicOp::Umax => Ok((0, 0b110)),
            AtomicOp::Umin => Ok((0, 0b111)),
            AtomicOp::Swap => Ok((1, 0b000)),
            AtomicOp::And if src == VReg::Imm(0) => Ok((1, 0b000)),
            AtomicOp::And if src == VReg::Imm(-1) => Ok((0, 0b001)),
            AtomicOp::Sub if src == VReg::Imm(0) => Ok((0, 0b000)),
            AtomicOp::And | AtomicOp::Sub | AtomicOp::Neg | AtomicOp::Nand => {
                Err(LowerError::UnsupportedOp {
                    op: format!("AArch64 native atomic RMW op {op:?}"),
                })
            }
        }
    }


    pub(crate) fn atomic_rmw_source_gpr(op: AtomicOp, src: VReg) -> Result<u8, LowerError> {
        if op == AtomicOp::And && src == VReg::Imm(-1) {
            return Ok(31);
        }

        Self::gpr_arm_or_x86(src)
    }


    pub(crate) fn atomic_rmw_source_avoid(src: VReg) -> Result<Option<u8>, LowerError> {
        match src {
            VReg::Imm(_) => Ok(None),
            other => Ok(Some(Self::gpr_arm_or_x86(other)?)),
        }
    }


    pub(crate) fn lower_atomic_rmw(
        &mut self,
        dst: VReg,
        addr: &Address,
        src: VReg,
        op: AtomicOp,
        width: MemWidth,
        order: MemoryOrder,
    ) -> Result<(), LowerError> {
        let rt = Self::dst_gpr_arm_or_x86(dst)?;
        let size = Self::mem_size(width)?;
        let (acquire, release) = Self::atomic_order_bits(order);
        let mut addr_avoid = vec![rt];
        if let Some(src_reg) = Self::atomic_rmw_source_avoid(src)? {
            addr_avoid.push(src_reg);
        }
        let (addr_scratches, rn) = self.lower_atomic_addr_to_base(&addr_avoid, addr)?;
        if let Ok((o3, opc)) = Self::atomic_rmw_op_encoding(op, src) {
            let rs = match Self::atomic_rmw_source_gpr(op, src) {
                Ok(rs) => rs,
                Err(err) => {
                    let VReg::Imm(value) = src else {
                        return Err(err);
                    };
                    let op_width = match width {
                        MemWidth::B1 | MemWidth::B2 | MemWidth::B4 => OpWidth::W32,
                        MemWidth::B8 => OpWidth::W64,
                        other => {
                            return Err(LowerError::UnsupportedOp {
                                op: format!("AArch64 native atomic RMW width {other:?}"),
                            });
                        }
                    };
                    let scratches = Self::scratch_regs(&[rt, rn], 1)?;
                    let scratch = scratches[0];
                    self.emit_scratch_save(&scratches);
                    self.emit_mov_imm(scratch, value, op_width)?;
                    self.emit_atomic_rmw(rt, rn, scratch, size, acquire, release, o3, opc);
                    self.emit_scratch_restore(&scratches);
                    self.emit_scratch_restore(&addr_scratches);
                    return Ok(());
                }
            };
            self.emit_atomic_rmw(rt, rn, rs, size, acquire, release, o3, opc);
            self.emit_scratch_restore(&addr_scratches);
            return Ok(());
        }

        self.lower_atomic_rmw_exclusive_loop(rt, rn, src, op, width, size, acquire, release)?;
        self.emit_scratch_restore(&addr_scratches);
        Ok(())
    }


    pub(crate) fn lower_atomic_rmw_exclusive_loop(
        &mut self,
        rt: u8,
        rn: u8,
        src: VReg,
        op: AtomicOp,
        width: MemWidth,
        size: u32,
        acquire: u32,
        release: u32,
    ) -> Result<(), LowerError> {
        let op_width = match width {
            MemWidth::B1 | MemWidth::B2 | MemWidth::B4 => OpWidth::W32,
            MemWidth::B8 => OpWidth::W64,
            other => {
                return Err(LowerError::UnsupportedOp {
                    op: format!("AArch64 native atomic RMW width {other:?}"),
                });
            }
        };

        match op {
            AtomicOp::And | AtomicOp::Sub | AtomicOp::Neg | AtomicOp::Nand => {}
            other => {
                return Err(LowerError::UnsupportedOp {
                    op: format!("AArch64 native atomic RMW op {other:?}"),
                });
            }
        }

        let src_reg = match src {
            VReg::Imm(0) => Some(31),
            VReg::Imm(_) => None,
            other => Some(Self::gpr_arm_or_x86(other)?),
        };

        let need_base = rt == rn;
        let need_operand = src_reg.is_none() || src_reg == Some(rt);
        let scratch_count = 2 + usize::from(need_base) + usize::from(need_operand);
        let mut avoid = vec![rt, rn];
        if let Some(src_reg) = src_reg {
            avoid.push(src_reg);
        }
        let scratches = Self::scratch_regs(&avoid, scratch_count)?;
        let mut scratch_index = 0;
        let work = scratches[scratch_index];
        scratch_index += 1;
        let status = scratches[scratch_index];
        scratch_index += 1;
        let base = if need_base {
            let reg = scratches[scratch_index];
            scratch_index += 1;
            reg
        } else {
            rn
        };
        let operand = if need_operand {
            Some(scratches[scratch_index])
        } else {
            None
        };

        self.emit_scratch_save(&scratches);
        if need_base {
            self.emit_mov_reg(base, rn, OpWidth::W64)?;
        }
        let operand = if let Some(operand) = operand {
            match src {
                VReg::Imm(value) => self.emit_mov_imm(operand, value, op_width)?,
                _ => self.emit_mov_reg(operand, src_reg.unwrap(), op_width)?,
            }
            operand
        } else {
            src_reg.unwrap()
        };

        let loop_start = self.code.position();
        self.emit_load_exclusive_ordered(rt, base, size, acquire);
        match op {
            AtomicOp::And => {
                self.emit_logic_shifted(work, rt, operand, 0b00, false, 0, 0, op_width)?;
            }
            AtomicOp::Sub => {
                self.emit_addsub_reg(work, rt, operand, true, false, op_width)?;
            }
            AtomicOp::Neg => {
                self.emit_addsub_reg(work, 31, rt, true, false, op_width)?;
            }
            AtomicOp::Nand => {
                self.emit_logic_shifted(work, rt, operand, 0b00, false, 0, 0, op_width)?;
                self.emit_logic_shifted(work, 31, work, 0b01, true, 0, 0, op_width)?;
            }
            _ => unreachable!(),
        }
        self.emit_store_exclusive_ordered(status, work, base, size, release);
        self.emit_compare_branch_to_offset(status, true, loop_start)?;
        self.emit_scratch_restore(&scratches);
        Ok(())
    }


    pub(crate) fn atomic_cmpxadd_flags_width(width: MemWidth) -> Result<OpWidth, LowerError> {
        match width {
            MemWidth::B1 => Ok(OpWidth::W8),
            MemWidth::B2 => Ok(OpWidth::W16),
            MemWidth::B4 => Ok(OpWidth::W32),
            MemWidth::B8 => Ok(OpWidth::W64),
            other => Err(LowerError::UnsupportedOp {
                op: format!("AArch64 native AtomicCmpXadd width {other:?}"),
            }),
        }
    }


    pub(crate) fn emit_mask_cas_compare_value(&mut self, reg: u8, width: MemWidth) -> Result<(), LowerError> {
        match width {
            MemWidth::B1 => self.emit_bitfield(reg, reg, 0b10, 0, 7, OpWidth::W32),
            MemWidth::B2 => self.emit_bitfield(reg, reg, 0b10, 0, 15, OpWidth::W32),
            MemWidth::B4 | MemWidth::B8 => Ok(()),
            other => Err(LowerError::UnsupportedOp {
                op: format!("AArch64 native CAS width {other:?}"),
            }),
        }
    }


    pub(crate) fn lower_cas(
        &mut self,
        dst: VReg,
        success: VReg,
        addr: &Address,
        expected: VReg,
        new_val: VReg,
        width: MemWidth,
        order: MemoryOrder,
    ) -> Result<(), LowerError> {
        let dst_reg = Self::dst_gpr_arm_or_x86(dst)?;
        let expected_reg = Self::gpr_arm_or_x86(expected)?;
        let new_reg = Self::gpr_arm_or_x86(new_val)?;
        let size = Self::mem_size(width)?;
        let compare_width = Self::cas_compare_width(width)?;
        let (acquire, release) = Self::atomic_order_bits(order);
        let success_reg = match success {
            VReg::Virtual(_) => None,
            other => Some(Self::dst_gpr_arm_or_x86(other)?),
        };

        let mut addr_avoid = vec![dst_reg, expected_reg, new_reg];
        if let Some(success_reg) = success_reg {
            addr_avoid.push(success_reg);
        }
        let (addr_scratches, rn) = self.lower_atomic_addr_to_base(&addr_avoid, addr)?;

        if dst == expected && success_reg.is_none() {
            self.emit_cas(dst_reg, new_reg, rn, size, acquire, release);
            self.emit_scratch_restore(&addr_scratches);
            return Ok(());
        }

        let need_compare = dst != expected;
        let need_saved_expected = success_reg.is_some() && dst == expected;
        let need_masked_expected = success_reg.is_some()
            && dst != expected
            && matches!(width, MemWidth::B1 | MemWidth::B2);
        let need_saved_flags = success_reg.is_some();
        let scratch_count = usize::from(need_compare)
            + usize::from(need_saved_expected)
            + usize::from(need_masked_expected)
            + usize::from(need_saved_flags);

        let mut avoid = vec![dst_reg, expected_reg, new_reg, rn];
        if let Some(success_reg) = success_reg {
            avoid.push(success_reg);
        }
        let scratches = Self::scratch_regs(&avoid, scratch_count)?;
        let mut scratch_index = 0;
        let compare_reg = if need_compare {
            let reg = scratches[scratch_index];
            scratch_index += 1;
            reg
        } else {
            dst_reg
        };
        let saved_expected = if need_saved_expected {
            let reg = scratches[scratch_index];
            scratch_index += 1;
            Some(reg)
        } else {
            None
        };
        let masked_expected = if need_masked_expected {
            let reg = scratches[scratch_index];
            scratch_index += 1;
            Some(reg)
        } else {
            None
        };
        let saved_flags = if need_saved_flags {
            Some(scratches[scratch_index])
        } else {
            None
        };

        self.emit_scratch_save(&scratches);
        if need_compare {
            self.emit_mov_reg(compare_reg, expected_reg, compare_width)?;
        }
        if let Some(saved_expected) = saved_expected {
            self.emit_mov_reg(saved_expected, expected_reg, compare_width)?;
            self.emit_mask_cas_compare_value(saved_expected, width)?;
        }

        self.emit_cas(compare_reg, new_reg, rn, size, acquire, release);
        if need_compare {
            self.emit_mov_reg(dst_reg, compare_reg, compare_width)?;
        }
        if let Some(success_reg) = success_reg {
            let expected_for_compare = if let Some(saved_expected) = saved_expected {
                saved_expected
            } else if let Some(masked_expected) = masked_expected {
                self.emit_mov_reg(masked_expected, expected_reg, OpWidth::W32)?;
                self.emit_mask_cas_compare_value(masked_expected, width)?;
                masked_expected
            } else {
                expected_reg
            };
            let saved_flags = saved_flags.expect("observable CAS success saves flags");
            self.emit_sysreg(saved_flags, ArmReg::Nzcv, true)?;
            self.emit_addsub_shifted(
                31,
                compare_reg,
                expected_for_compare,
                true,
                true,
                0,
                0,
                compare_width,
            )?;
            self.lower_test_condition(Self::arm_x_reg(success_reg), Condition::Eq)?;
            self.emit_sysreg(saved_flags, ArmReg::Nzcv, false)?;
        }
        self.emit_scratch_restore(&scratches);
        self.emit_scratch_restore(&addr_scratches);
        Ok(())
    }


    pub(crate) fn lower_atomic_cmpxadd(
        &mut self,
        dst_old: VReg,
        addr: &Address,
        cmp: VReg,
        add: VReg,
        cond: Condition,
        width: MemWidth,
        order: MemoryOrder,
    ) -> Result<(), LowerError> {
        let dst_reg = Self::dst_gpr_arm_or_x86(dst_old)?;
        let cmp_reg = Self::gpr_arm_or_x86(cmp)?;
        let add_reg = Self::gpr_arm_or_x86(add)?;
        let size = Self::mem_size(width)?;
        let flags_width = Self::atomic_cmpxadd_flags_width(width)?;
        let emit_width = Self::cas_compare_width(width)?;
        let cond_code = Self::arm_cond_code(cond)?;
        let (acquire, release) = Self::atomic_order_bits(order);

        let (addr_scratches, rn) =
            self.lower_atomic_addr_to_base(&[dst_reg, cmp_reg, add_reg], addr)?;
        let need_base = rn == dst_reg;
        let need_saved_cmp = cmp_reg == dst_reg;
        let need_saved_add = add_reg == dst_reg;
        let need_subword_compare = matches!(flags_width, OpWidth::W8 | OpWidth::W16);

        let scratch_count = 2
            + usize::from(need_base)
            + usize::from(need_saved_cmp)
            + usize::from(need_saved_add)
            + if need_subword_compare { 2 } else { 0 };
        let scratches = Self::scratch_regs(&[dst_reg, rn, cmp_reg, add_reg], scratch_count)?;
        let mut scratch_index = 0;
        let work = scratches[scratch_index];
        scratch_index += 1;
        let status = scratches[scratch_index];
        scratch_index += 1;
        let base = if need_base {
            let reg = scratches[scratch_index];
            scratch_index += 1;
            reg
        } else {
            rn
        };
        let cmp_operand = if need_saved_cmp {
            let reg = scratches[scratch_index];
            scratch_index += 1;
            reg
        } else {
            cmp_reg
        };
        let add_operand = if need_saved_add {
            let reg = scratches[scratch_index];
            scratch_index += 1;
            reg
        } else {
            add_reg
        };
        let compare_lhs = if need_subword_compare {
            let reg = scratches[scratch_index];
            scratch_index += 1;
            Some(reg)
        } else {
            None
        };
        let compare_rhs = if need_subword_compare {
            Some(scratches[scratch_index])
        } else {
            None
        };

        self.emit_scratch_save(&scratches);
        if need_base {
            self.emit_mov_reg(base, rn, OpWidth::W64)?;
        }
        if need_saved_cmp {
            self.emit_mov_reg(cmp_operand, cmp_reg, emit_width)?;
        }
        if need_saved_add {
            self.emit_mov_reg(add_operand, add_reg, emit_width)?;
        }

        let loop_start = self.code.position();
        self.emit_load_exclusive_ordered(dst_reg, base, size, acquire);
        if let (Some(lhs), Some(rhs)) = (compare_lhs, compare_rhs) {
            self.emit_shifted_subword_addsub_operand(lhs, dst_reg, flags_width)?;
            self.emit_shifted_subword_addsub_operand(rhs, cmp_operand, flags_width)?;
            self.emit_addsub_reg(31, lhs, rhs, true, true, OpWidth::W32)?;
        } else {
            self.emit_addsub_reg(31, dst_reg, cmp_operand, true, true, emit_width)?;
        }
        self.emit_addsub_reg(work, dst_reg, add_operand, false, false, emit_width)?;
        self.emit_cond_select(work, work, dst_reg, cond_code, 0, 0, emit_width)?;
        self.emit_store_exclusive_ordered(status, work, base, size, release);
        self.emit_compare_branch_to_offset(status, true, loop_start)?;

        self.emit_scratch_restore(&scratches);
        self.emit_scratch_restore(&addr_scratches);
        Ok(())
    }
}
