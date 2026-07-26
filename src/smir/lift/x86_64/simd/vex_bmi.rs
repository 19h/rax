//! VEX-encoded BMI1/BMI2 semantic lifting after exact opcode admission.

use crate::smir::ir::flags::{FlagSet, FlagUpdate};
use crate::smir::ir::ops::{OpKind, SmirOp, X86BlsKind, X86SsePrefix};
use crate::smir::ir::types::{MemWidth, OpId, OpWidth, SignExtend, SrcOperand, VReg, VecWidth};
use crate::smir::lift::x86_64::{
    ModRm, VecEncodingKind, VecPrefix, X86_64Lifter, x86_bextr_flags, x86_bls_flags, x86_bzhi_flags,
};
use crate::smir::lift::{LiftContext, LiftError, LiftResult};

impl X86_64Lifter {
    fn vex_bmi_source(
        &self,
        modrm: &ModRm,
        mem_width: MemWidth,
        next_pc: u64,
        pc: u64,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) -> VReg {
        if !modrm.is_memory {
            return self.gpr(modrm.rm);
        }

        let (addr, pre_ops) = self.x86_addr_to_smir(
            modrm.addr.as_ref().expect("decoded memory address"),
            next_pc,
            ctx,
        );
        ops.extend(pre_ops);
        let tmp = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::Load {
                dst: tmp,
                addr,
                width: mem_width,
                sign: SignExtend::Zero,
            },
        ));
        tmp
    }

    pub(crate) fn lift_vex_andn_0f38(
        &self,
        prefix: VecPrefix,
        modrm: ModRm,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.encoding != VecEncodingKind::Vex
            || prefix.width != VecWidth::V128
            || prefix.pp != X86SsePrefix::None
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let width = if prefix.w { OpWidth::W64 } else { OpWidth::W32 };
        let mem_width = if prefix.w { MemWidth::B8 } else { MemWidth::B4 };
        let next_pc = pc + prefix.bytes as u64 + 1 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let src2 = self.vex_bmi_source(&modrm, mem_width, next_pc, pc, ctx, &mut ops);

        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::AndNot {
                dst: self.gpr(modrm.reg),
                src1: src2,
                src2: SrcOperand::Reg(self.gpr(prefix.vvvv)),
                width,
                flags: FlagUpdate::Specific(
                    FlagSet::CF
                        .union(FlagSet::ZF)
                        .union(FlagSet::SF)
                        .union(FlagSet::OF),
                ),
            },
        ));

        Ok(LiftResult::fallthrough(
            ops,
            prefix.bytes + 1 + modrm.bytes_consumed,
        ))
    }

    pub(crate) fn lift_vex_bls_0f38(
        &self,
        prefix: VecPrefix,
        modrm: ModRm,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.encoding != VecEncodingKind::Vex
            || prefix.width != VecWidth::V128
            || prefix.pp != X86SsePrefix::None
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let width = if prefix.w { OpWidth::W64 } else { OpWidth::W32 };
        let mem_width = if prefix.w { MemWidth::B8 } else { MemWidth::B4 };
        let kind = match (modrm.byte >> 3) & 0x07 {
            1 => X86BlsKind::Blsr,
            2 => X86BlsKind::Blsmsk,
            3 => X86BlsKind::Blsi,
            _ => {
                return Err(LiftError::InvalidEncoding {
                    addr: pc,
                    bytes: bytes[..prefix.bytes + 1 + modrm.bytes_consumed].to_vec(),
                });
            }
        };
        let next_pc = pc + prefix.bytes as u64 + 1 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let src = self.vex_bmi_source(&modrm, mem_width, next_pc, pc, ctx, &mut ops);

        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::X86Bls {
                dst: self.gpr(prefix.vvvv),
                src,
                width,
                kind,
                flags: x86_bls_flags(),
            },
        ));

        Ok(LiftResult::fallthrough(
            ops,
            prefix.bytes + 1 + modrm.bytes_consumed,
        ))
    }

    pub(crate) fn lift_vex_bzhi_bextr_0f38(
        &self,
        prefix: VecPrefix,
        modrm: ModRm,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.encoding != VecEncodingKind::Vex
            || prefix.width != VecWidth::V128
            || prefix.pp != X86SsePrefix::None
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let width = if prefix.w { OpWidth::W64 } else { OpWidth::W32 };
        let mem_width = if prefix.w { MemWidth::B8 } else { MemWidth::B4 };
        let next_pc = pc + prefix.bytes as u64 + 1 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let src = self.vex_bmi_source(&modrm, mem_width, next_pc, pc, ctx, &mut ops);

        let dst = self.gpr(modrm.reg);
        let control = self.gpr(prefix.vvvv);
        let kind = match opcode {
            0xF5 => OpKind::Bzhi {
                dst,
                src,
                index: control,
                width,
                flags: x86_bzhi_flags(),
            },
            0xF7 => OpKind::Bextr {
                dst,
                src,
                control,
                width,
                flags: x86_bextr_flags(),
            },
            _ => unreachable!("VEX BZHI/BEXTR only dispatches F5/F7"),
        };
        ops.push(SmirOp::new(OpId(ops.len() as u16), pc, kind));

        Ok(LiftResult::fallthrough(
            ops,
            prefix.bytes + 1 + modrm.bytes_consumed,
        ))
    }

    pub(crate) fn lift_vex_pdep_pext_0f38(
        &self,
        prefix: VecPrefix,
        modrm: ModRm,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.encoding != VecEncodingKind::Vex
            || prefix.width != VecWidth::V128
            || !matches!(prefix.pp, X86SsePrefix::Rep | X86SsePrefix::Repne)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let width = if prefix.w { OpWidth::W64 } else { OpWidth::W32 };
        let mem_width = if prefix.w { MemWidth::B8 } else { MemWidth::B4 };
        let next_pc = pc + prefix.bytes as u64 + 1 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let mask = self.vex_bmi_source(&modrm, mem_width, next_pc, pc, ctx, &mut ops);

        let dst = self.gpr(modrm.reg);
        let src = self.gpr(prefix.vvvv);
        let op = match prefix.pp {
            X86SsePrefix::Rep => OpKind::Pext {
                dst,
                src,
                mask,
                width,
            },
            X86SsePrefix::Repne => OpKind::Pdep {
                dst,
                src,
                mask,
                width,
            },
            _ => unreachable!("PDEP/PEXT are only dispatched for F2/F3 VEX prefixes"),
        };
        ops.push(SmirOp::new(OpId(ops.len() as u16), pc, op));

        Ok(LiftResult::fallthrough(
            ops,
            prefix.bytes + 1 + modrm.bytes_consumed,
        ))
    }

    pub(crate) fn lift_vex_bmi2_shift_0f38(
        &self,
        prefix: VecPrefix,
        modrm: ModRm,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.encoding != VecEncodingKind::Vex
            || prefix.width != VecWidth::V128
            || !matches!(
                prefix.pp,
                X86SsePrefix::OpSize | X86SsePrefix::Rep | X86SsePrefix::Repne
            )
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let width = if prefix.w { OpWidth::W64 } else { OpWidth::W32 };
        let mem_width = if prefix.w { MemWidth::B8 } else { MemWidth::B4 };
        let next_pc = pc + prefix.bytes as u64 + 1 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let src = self.vex_bmi_source(&modrm, mem_width, next_pc, pc, ctx, &mut ops);

        let dst = self.gpr(modrm.reg);
        let count = self.gpr(prefix.vvvv);
        // Scalar SMIR shifts apply the source-architecture count mask before
        // shifting. Keep the architectural count operand intact so the
        // state-backed native path can stage it directly without a redundant
        // virtual-register mask.
        let amount = SrcOperand::Reg(count);
        let op = match prefix.pp {
            X86SsePrefix::Rep => OpKind::Sar {
                dst,
                src,
                amount,
                width,
                flags: FlagUpdate::None,
            },
            X86SsePrefix::Repne => OpKind::Shr {
                dst,
                src,
                amount,
                width,
                flags: FlagUpdate::None,
            },
            X86SsePrefix::OpSize => OpKind::Shl {
                dst,
                src,
                amount,
                width,
                flags: FlagUpdate::None,
            },
            _ => unreachable!("BMI2 VEX shifts require 66/F2/F3 prefix encodings"),
        };
        ops.push(SmirOp::new(OpId(ops.len() as u16), pc, op));

        Ok(LiftResult::fallthrough(
            ops,
            prefix.bytes + 1 + modrm.bytes_consumed,
        ))
    }

    pub(crate) fn lift_vex_bmi2_rorx_0f3a(
        &self,
        prefix: VecPrefix,
        modrm: ModRm,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.encoding != VecEncodingKind::Vex
            || prefix.width != VecWidth::V128
            || prefix.pp != X86SsePrefix::Repne
            || prefix.vvvv != 0
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let width = if prefix.w { OpWidth::W64 } else { OpWidth::W32 };
        let mem_width = if prefix.w { MemWidth::B8 } else { MemWidth::B4 };
        let imm_offset = prefix.bytes + 1 + modrm.bytes_consumed;
        if bytes.len() <= imm_offset {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: bytes.len(),
                need: imm_offset + 1,
            });
        }

        let next_pc = pc + imm_offset as u64 + 1;
        let mut ops = Vec::new();
        let src = self.vex_bmi_source(&modrm, mem_width, next_pc, pc, ctx, &mut ops);

        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::Ror {
                dst: self.gpr(modrm.reg),
                src,
                amount: SrcOperand::Imm(bytes[imm_offset] as i64),
                width,
                flags: FlagUpdate::None,
            },
        ));

        Ok(LiftResult::fallthrough(
            ops,
            prefix.bytes + 1 + modrm.bytes_consumed + 1,
        ))
    }
}
