//! Intel APX paired-stack instruction lifting.

use super::*;

impl X86_64Lifter {
    /// Construct the terminal #UD known from an APX prefix, opcode, and ModR/M
    /// byte. Reserved Group 4/5 cells and invalid paired-stack encodings do not
    /// decode a SIB/displacement or observe an apparent operand.
    pub(super) fn apx_modrm_invalid_opcode(prefix: ApxEvexPrefix) -> LiftResult {
        LiftResult {
            ops: Vec::new(),
            bytes_consumed: prefix.bytes + 2,
            control_flow: ControlFlow::Trap {
                kind: TrapKind::InvalidOpcode,
            },
            branch_targets: Vec::new(),
        }
    }

    pub(crate) fn lift_apx_push2(
        &self,
        prefix: ApxEvexPrefix,
        modrm: u8,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if !prefix.nd
            || prefix.nf
            || prefix.z
            || prefix.ll != 0
            || prefix.aaa != 0
            || prefix.pp != 0
            || !prefix.x4
        {
            return Ok(Self::apx_modrm_invalid_opcode(prefix));
        }
        if (modrm >> 6) != 3 {
            return Ok(Self::apx_modrm_invalid_opcode(prefix));
        }
        let group = (modrm >> 3) & 0x07;
        if group != 6 {
            return Ok(Self::apx_modrm_invalid_opcode(prefix));
        }

        let reg1 = (modrm & 0x07) | prefix.rm_ext();
        let reg2 = prefix.vvvv_reg();
        if reg1 == 4 || reg2 == 4 {
            return Ok(Self::apx_modrm_invalid_opcode(prefix));
        }
        let tmp1 = ctx.alloc_vreg();
        let tmp2 = ctx.alloc_vreg();
        let rsp = self.rsp();
        let ops = vec![
            SmirOp::new(
                OpId(0),
                pc,
                OpKind::Mov {
                    dst: tmp1,
                    src: SrcOperand::Reg(self.gpr(reg1)),
                    width: OpWidth::W64,
                },
            ),
            SmirOp::new(
                OpId(1),
                pc,
                OpKind::Mov {
                    dst: tmp2,
                    src: SrcOperand::Reg(self.gpr(reg2)),
                    width: OpWidth::W64,
                },
            ),
            SmirOp::new(
                OpId(2),
                pc,
                OpKind::Sub {
                    dst: rsp,
                    src1: rsp,
                    src2: SrcOperand::Imm(16),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
            ),
            SmirOp::new(
                OpId(3),
                pc,
                OpKind::Store {
                    src: tmp1,
                    addr: Address::Direct(rsp),
                    width: MemWidth::B8,
                },
            ),
            SmirOp::new(
                OpId(4),
                pc,
                OpKind::Store {
                    src: tmp2,
                    addr: Address::base_off(rsp, 8),
                    width: MemWidth::B8,
                },
            ),
        ];

        Ok(LiftResult::fallthrough(ops, prefix.bytes + 2))
    }

    pub(crate) fn lift_apx_pop2(
        &self,
        prefix: ApxEvexPrefix,
        modrm: u8,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if !prefix.nd
            || prefix.nf
            || prefix.z
            || prefix.ll != 0
            || prefix.aaa != 0
            || prefix.pp != 0
            || !prefix.x4
        {
            return Ok(Self::apx_modrm_invalid_opcode(prefix));
        }
        if (modrm >> 6) != 3 {
            return Ok(Self::apx_modrm_invalid_opcode(prefix));
        }
        let group = (modrm >> 3) & 0x07;
        if group != 0 {
            return Ok(Self::apx_modrm_invalid_opcode(prefix));
        }

        let reg1 = (modrm & 0x07) | prefix.rm_ext();
        let reg2 = prefix.vvvv_reg();
        if reg1 == 4 || reg2 == 4 || reg1 == reg2 {
            return Ok(Self::apx_modrm_invalid_opcode(prefix));
        }
        let tmp1 = ctx.alloc_vreg();
        let tmp2 = ctx.alloc_vreg();
        let rsp = self.rsp();
        let ops = vec![
            SmirOp::new(
                OpId(0),
                pc,
                OpKind::Load {
                    dst: tmp1,
                    addr: Address::Direct(rsp),
                    width: MemWidth::B8,
                    sign: SignExtend::Zero,
                },
            ),
            SmirOp::new(
                OpId(1),
                pc,
                OpKind::Load {
                    dst: tmp2,
                    addr: Address::base_off(rsp, 8),
                    width: MemWidth::B8,
                    sign: SignExtend::Zero,
                },
            ),
            SmirOp::new(
                OpId(2),
                pc,
                OpKind::Add {
                    dst: rsp,
                    src1: rsp,
                    src2: SrcOperand::Imm(16),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
            ),
            SmirOp::new(
                OpId(3),
                pc,
                OpKind::Mov {
                    dst: self.gpr(reg2),
                    src: SrcOperand::Reg(tmp1),
                    width: OpWidth::W64,
                },
            ),
            SmirOp::new(
                OpId(4),
                pc,
                OpKind::Mov {
                    dst: self.gpr(reg1),
                    src: SrcOperand::Reg(tmp2),
                    width: OpWidth::W64,
                },
            ),
        ];

        Ok(LiftResult::fallthrough(ops, prefix.bytes + 2))
    }
}
