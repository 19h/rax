//! Intel APX CCMP/CTEST lifting.

use super::*;

impl X86_64Lifter {
    pub(super) fn apx_conditional_opcode_fields_valid(prefix: ApxEvexPrefix, opcode: u8) -> bool {
        let pp_valid = match opcode {
            0x38 | 0x3A | 0x80 | 0x84 | 0xF6 => prefix.pp == 0,
            0x39 | 0x3B | 0x81 | 0x83 | 0x85 | 0xF7 => prefix.pp <= 1,
            _ => false,
        };
        pp_valid && !prefix.z && prefix.ll == 0 && !prefix.nd
    }

    fn apx_conditional_encoding_valid(prefix: ApxEvexPrefix, opcode: u8, modrm: u8) -> bool {
        Self::apx_conditional_opcode_fields_valid(prefix, opcode) && (modrm >> 6 != 3 || prefix.x4)
    }

    pub(crate) fn apx_ccmp_default_rflags(dfv: u8) -> i64 {
        let mut flags = 0x02;
        if dfv & 0x1 != 0 {
            flags |= 0x005; // CF and PF
        }
        if dfv & 0x2 != 0 {
            flags |= 0x040;
        }
        if dfv & 0x4 != 0 {
            flags |= 0x080;
        }
        if dfv & 0x8 != 0 {
            flags |= 0x800;
        }
        flags
    }

    fn push_apx_scc_value(
        &self,
        ops: &mut Vec<SmirOp>,
        pc: u64,
        ctx: &mut LiftContext,
        scc: u8,
    ) -> VReg {
        let dst = ctx.alloc_vreg();
        let kind = match scc & 0x0F {
            0x0A => OpKind::Mov {
                dst,
                src: SrcOperand::Imm(1),
                width: OpWidth::W64,
            },
            0x0B => OpKind::Mov {
                dst,
                src: SrcOperand::Imm(0),
                width: OpWidth::W64,
            },
            cc => OpKind::SetCC {
                dst,
                cond: self.x86_cond(cc),
                width: OpWidth::W64,
            },
        };
        ops.push(SmirOp::new(OpId(ops.len() as u16), pc, kind));
        dst
    }

    pub(crate) fn push_apx_conditional_flags_with(
        &self,
        ops: &mut Vec<SmirOp>,
        pc: u64,
        ctx: &mut LiftContext,
        scc: u8,
        dfv: u8,
        push_true_ops: impl FnOnce(&mut Vec<SmirOp>),
    ) {
        let old_flags = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::ReadFlags { dst: old_flags },
        ));

        let cond_reg = self.push_apx_scc_value(ops, pc, ctx, scc);

        let false_flags = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::And {
                dst: false_flags,
                src1: old_flags,
                src2: SrcOperand::Imm(!APX_CCMP_FLAGS_MASK),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        ));
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::Or {
                dst: false_flags,
                src1: false_flags,
                src2: SrcOperand::Imm(Self::apx_ccmp_default_rflags(dfv)),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        ));

        push_true_ops(ops);

        let true_flags = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::ReadFlags { dst: true_flags },
        ));

        let selected_flags = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::Select {
                dst: selected_flags,
                cond: cond_reg,
                src_true: true_flags,
                src_false: false_flags,
                width: OpWidth::W64,
            },
        ));
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::WriteFlags {
                src: selected_flags,
            },
        ));
    }

    fn push_apx_conditional_load(
        &self,
        ops: &mut Vec<SmirOp>,
        pc: u64,
        ctx: &mut LiftContext,
        modrm: &ModRm,
        next_pc: u64,
        mem_width: MemWidth,
    ) -> VReg {
        if !modrm.is_memory {
            return self.gpr(modrm.rm);
        }

        let x86_addr = modrm.addr.as_ref().unwrap();
        let (addr, pre_ops) = self.x86_addr_to_smir(x86_addr, next_pc, ctx);
        ops.extend(pre_ops);
        let dst = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::Load {
                dst,
                addr,
                width: mem_width,
                sign: SignExtend::Zero,
            },
        ));
        dst
    }

    pub(crate) fn lift_apx_ccmp(
        &self,
        prefix: ApxEvexPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if !Self::apx_conditional_opcode_fields_valid(prefix, opcode) {
            return Ok(Self::apx_invalid_opcode(prefix.bytes + 1));
        }
        let modrm_byte = Self::apx_operand_modrm_byte(prefix, bytes, pc)?;
        if !Self::apx_conditional_encoding_valid(prefix, opcode, modrm_byte) {
            return Ok(Self::apx_modrm_invalid_opcode(prefix));
        }

        let is_byte = opcode & 1 == 0;
        let op_size = prefix.op_size(is_byte);
        let width = self.size_to_width(op_size);
        let mem_width = self.size_to_memwidth(op_size);
        let modrm_prefix = prefix.as_modrm_prefix(prefix.bytes + 1);
        let modrm = decode_modrm(bytes, &modrm_prefix, pc)?;
        let next_pc = pc + prefix.bytes as u64 + 1 + modrm.bytes_consumed as u64;

        let mut ops = Vec::new();
        let rm = self.push_apx_conditional_load(&mut ops, pc, ctx, &modrm, next_pc, mem_width);
        let reg_is_src = opcode & 2 == 0;
        let (src1, src2, hint) = if reg_is_src {
            (rm, self.gpr(modrm.reg), X86AluEncoding::RmReg)
        } else {
            (self.gpr(modrm.reg), rm, X86AluEncoding::RegRm)
        };
        self.push_apx_conditional_flags_with(
            &mut ops,
            pc,
            ctx,
            prefix.ccmp_cond(),
            prefix.ccmp_default_flags(),
            |ops| {
                ops.push(SmirOp::with_hint(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Cmp {
                        src1,
                        src2: SrcOperand::Reg(src2),
                        width,
                    },
                    X86OpHint::AluEncoding(hint),
                ));
            },
        );

        Ok(LiftResult::fallthrough(
            ops,
            prefix.bytes + 1 + modrm.bytes_consumed,
        ))
    }

    pub(crate) fn lift_apx_ctest_reg(
        &self,
        prefix: ApxEvexPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if !Self::apx_conditional_opcode_fields_valid(prefix, opcode) {
            return Ok(Self::apx_invalid_opcode(prefix.bytes + 1));
        }
        let modrm_byte = Self::apx_operand_modrm_byte(prefix, bytes, pc)?;
        if !Self::apx_conditional_encoding_valid(prefix, opcode, modrm_byte) {
            return Ok(Self::apx_modrm_invalid_opcode(prefix));
        }

        let op_size = prefix.op_size(opcode == 0x84);
        let width = self.size_to_width(op_size);
        let mem_width = self.size_to_memwidth(op_size);
        let modrm_prefix = prefix.as_modrm_prefix(prefix.bytes + 1);
        let modrm = decode_modrm(bytes, &modrm_prefix, pc)?;
        let next_pc = pc + prefix.bytes as u64 + 1 + modrm.bytes_consumed as u64;

        let mut ops = Vec::new();
        let src1 = self.push_apx_conditional_load(&mut ops, pc, ctx, &modrm, next_pc, mem_width);
        self.push_apx_conditional_flags_with(
            &mut ops,
            pc,
            ctx,
            prefix.ccmp_cond(),
            prefix.ccmp_default_flags(),
            |ops| {
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Test {
                        src1,
                        src2: SrcOperand::Reg(self.gpr(modrm.reg)),
                        width,
                    },
                ));
            },
        );

        Ok(LiftResult::fallthrough(
            ops,
            prefix.bytes + 1 + modrm.bytes_consumed,
        ))
    }

    pub(crate) fn lift_apx_ccmp_imm(
        &self,
        prefix: ApxEvexPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        self.lift_apx_conditional_imm(prefix, opcode, bytes, pc, ctx, true)
    }

    pub(crate) fn lift_apx_ctest_imm(
        &self,
        prefix: ApxEvexPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        self.lift_apx_conditional_imm(prefix, opcode, bytes, pc, ctx, false)
    }

    fn lift_apx_conditional_imm(
        &self,
        prefix: ApxEvexPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
        is_compare: bool,
    ) -> Result<LiftResult, LiftError> {
        let modrm_byte = Self::apx_operand_modrm_byte(prefix, bytes, pc)?;
        if !Self::apx_conditional_encoding_valid(prefix, opcode, modrm_byte) {
            return Ok(Self::apx_modrm_invalid_opcode(prefix));
        }

        let is_byte = matches!(opcode, 0x80 | 0xF6);
        let op_size = prefix.op_size(is_byte);
        let width = self.size_to_width(op_size);
        let mem_width = self.size_to_memwidth(op_size);
        let modrm_prefix = prefix.as_modrm_prefix(prefix.bytes + 1);
        let modrm = decode_modrm(bytes, &modrm_prefix, pc)?;
        let imm_offset = modrm.bytes_consumed;
        let imm_size = match opcode {
            0x80 | 0x83 | 0xF6 => 1,
            0x81 | 0xF7 if op_size == 2 => 2,
            0x81 | 0xF7 => 4,
            _ => unreachable!("conditional immediate dispatch opcode"),
        };
        if bytes.len() < imm_offset + imm_size {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: bytes.len(),
                need: imm_offset + imm_size,
            });
        }
        let imm = match imm_size {
            1 if matches!(opcode, 0x83) => bytes[imm_offset] as i8 as i64,
            1 => bytes[imm_offset] as i64,
            2 => i16::from_le_bytes([bytes[imm_offset], bytes[imm_offset + 1]]) as i64,
            4 => i32::from_le_bytes([
                bytes[imm_offset],
                bytes[imm_offset + 1],
                bytes[imm_offset + 2],
                bytes[imm_offset + 3],
            ]) as i64,
            _ => unreachable!(),
        };
        let next_pc = pc + prefix.bytes as u64 + 1 + modrm.bytes_consumed as u64 + imm_size as u64;

        let mut ops = Vec::new();
        let src1 = self.push_apx_conditional_load(&mut ops, pc, ctx, &modrm, next_pc, mem_width);
        self.push_apx_conditional_flags_with(
            &mut ops,
            pc,
            ctx,
            prefix.ccmp_cond(),
            prefix.ccmp_default_flags(),
            |ops| {
                let kind = if is_compare {
                    OpKind::Cmp {
                        src1,
                        src2: SrcOperand::Imm(imm),
                        width,
                    }
                } else {
                    OpKind::Test {
                        src1,
                        src2: SrcOperand::Imm(imm),
                        width,
                    }
                };
                ops.push(SmirOp::new(OpId(ops.len() as u16), pc, kind));
            },
        );

        Ok(LiftResult::fallthrough(
            ops,
            prefix.bytes + 1 + modrm.bytes_consumed + imm_size,
        ))
    }
}
