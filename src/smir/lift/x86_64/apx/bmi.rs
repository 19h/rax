//! Intel APX promotion of VEX-encoded BMI1 and BMI2 instructions.

use super::*;

#[derive(Clone, Copy, Debug)]
enum ApxBmiShiftKind {
    Sarx,
    Shlx,
    Shrx,
}

#[derive(Clone, Copy, Debug)]
enum ApxBmi0f38Kind {
    Andn,
    Bls,
    Bzhi,
    Bextr,
    Pdep,
    Pext,
    Mulx,
    Shift(ApxBmiShiftKind),
}

impl ApxBmi0f38Kind {
    fn supports_nf(self) -> bool {
        matches!(self, Self::Andn | Self::Bls | Self::Bzhi | Self::Bextr)
    }
}

impl X86_64Lifter {
    fn retain_apx_bmi_requirement(pc: u64, mut result: LiftResult) -> LiftResult {
        result
            .ops
            .insert(0, SmirOp::new(OpId(0), pc, OpKind::X86RequireApx));
        for (index, op) in result.ops.iter_mut().enumerate() {
            op.id = OpId(index as u16);
        }
        result
    }

    fn apx_bmi_payload_is_valid(prefix: ApxEvexPrefix, bytes: &[u8], supports_nf: bool) -> bool {
        // Intel APX revision 5.0 Figure 3.4 permits only V4, L, and NF in
        // payload byte 2, while exception class APX-EVEX-BMI separately makes
        // L=1 invalid. NF is permitted only for the six flag-suppressible BMI
        // operations. Consequently V4 is the only other variable payload bit.
        let allowed_p2 = 0x08 | if supports_nf { 0x04 } else { 0 };
        bytes[prefix.bytes - 1] & !allowed_p2 == 0
    }

    fn apx_bmi_modrm_byte(prefix: ApxEvexPrefix, bytes: &[u8], pc: u64) -> Result<u8, LiftError> {
        bytes
            .get(prefix.bytes + 1)
            .copied()
            .ok_or(LiftError::Incomplete {
                addr: pc,
                have: bytes.len(),
                need: prefix.bytes + 2,
            })
    }

    pub(crate) fn lift_apx_bmi_0f38(
        &self,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let prefix = decode_apx_evex_prefix_for_map(bytes, pc, 2)?;
        let kind = match (opcode, prefix.pp) {
            (0xF2, 0x00) => ApxBmi0f38Kind::Andn,
            (0xF3, 0x00) => ApxBmi0f38Kind::Bls,
            (0xF5, 0x00) => ApxBmi0f38Kind::Bzhi,
            (0xF7, 0x00) => ApxBmi0f38Kind::Bextr,
            (0xF5, 0x03) => ApxBmi0f38Kind::Pdep,
            (0xF5, 0x02) => ApxBmi0f38Kind::Pext,
            (0xF6, 0x03) => ApxBmi0f38Kind::Mulx,
            (0xF7, 0x02) => ApxBmi0f38Kind::Shift(ApxBmiShiftKind::Sarx),
            (0xF7, 0x01) => ApxBmi0f38Kind::Shift(ApxBmiShiftKind::Shlx),
            (0xF7, 0x03) => ApxBmi0f38Kind::Shift(ApxBmiShiftKind::Shrx),
            _ => return Ok(Self::apx_invalid_opcode(prefix.bytes + 1)),
        };

        if !Self::apx_bmi_payload_is_valid(prefix, bytes, kind.supports_nf()) {
            return Ok(Self::apx_invalid_opcode(prefix.bytes + 1));
        }

        let modrm_byte = Self::apx_bmi_modrm_byte(prefix, bytes, pc)?;
        if modrm_byte >> 6 == 3 && !prefix.x4 {
            return Ok(Self::apx_modrm_invalid_opcode(prefix));
        }
        let bls_kind = if matches!(kind, ApxBmi0f38Kind::Bls) {
            match (modrm_byte >> 3) & 0x07 {
                1 => Some(X86BlsKind::Blsr),
                2 => Some(X86BlsKind::Blsmsk),
                3 => Some(X86BlsKind::Blsi),
                _ => return Ok(Self::apx_modrm_invalid_opcode(prefix)),
            }
        } else {
            None
        };

        let width = if prefix.w { OpWidth::W64 } else { OpWidth::W32 };
        let mem_width = if prefix.w { MemWidth::B8 } else { MemWidth::B4 };
        let modrm_prefix = prefix.as_modrm_prefix(prefix.bytes + 1);
        let modrm = decode_modrm(&bytes[prefix.bytes + 1..], &modrm_prefix, pc)?;
        let next_pc = pc + prefix.bytes as u64 + 1 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();

        let rm_src = if modrm.is_memory {
            let x86_addr = modrm.addr.as_ref().unwrap();
            let (addr, pre_ops) = self.x86_addr_to_smir(x86_addr, next_pc, ctx);
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
        } else {
            self.gpr(modrm.rm)
        };

        match kind {
            ApxBmi0f38Kind::Andn => {
                let flags = if prefix.nf {
                    FlagUpdate::None
                } else {
                    FlagUpdate::Specific(
                        FlagSet::CF
                            .union(FlagSet::ZF)
                            .union(FlagSet::SF)
                            .union(FlagSet::OF),
                    )
                };
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::AndNot {
                        dst: self.gpr(modrm.reg),
                        src1: rm_src,
                        src2: SrcOperand::Reg(self.gpr(prefix.vvvv_reg())),
                        width,
                        flags,
                    },
                ));
            }
            ApxBmi0f38Kind::Bls => {
                let flags = if prefix.nf {
                    FlagUpdate::None
                } else {
                    x86_bls_flags()
                };
                let Some(kind) = bls_kind else {
                    return Ok(Self::apx_modrm_invalid_opcode(prefix));
                };
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::X86Bls {
                        dst: self.gpr(prefix.vvvv_reg()),
                        src: rm_src,
                        width,
                        kind,
                        flags,
                    },
                ));
            }
            ApxBmi0f38Kind::Bzhi => {
                let flags = if prefix.nf {
                    FlagUpdate::None
                } else {
                    x86_bzhi_flags()
                };
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Bzhi {
                        dst: self.gpr(modrm.reg),
                        src: rm_src,
                        index: self.gpr(prefix.vvvv_reg()),
                        width,
                        flags,
                    },
                ));
            }
            ApxBmi0f38Kind::Bextr => {
                let flags = if prefix.nf {
                    FlagUpdate::None
                } else {
                    x86_bextr_flags()
                };
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Bextr {
                        dst: self.gpr(modrm.reg),
                        src: rm_src,
                        control: self.gpr(prefix.vvvv_reg()),
                        width,
                        flags,
                    },
                ));
            }
            ApxBmi0f38Kind::Pdep => ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Pdep {
                    dst: self.gpr(modrm.reg),
                    src: self.gpr(prefix.vvvv_reg()),
                    mask: rm_src,
                    width,
                },
            )),
            ApxBmi0f38Kind::Pext => ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Pext {
                    dst: self.gpr(modrm.reg),
                    src: self.gpr(prefix.vvvv_reg()),
                    mask: rm_src,
                    width,
                },
            )),
            ApxBmi0f38Kind::Mulx => ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                OpKind::MulU {
                    dst_lo: self.gpr(prefix.vvvv_reg()),
                    dst_hi: Some(self.gpr(modrm.reg)),
                    src1: self.gpr(2),
                    src2: SrcOperand::Reg(rm_src),
                    width,
                    flags: FlagUpdate::None,
                },
                X86OpHint::Mulx,
            )),
            ApxBmi0f38Kind::Shift(kind) => {
                let count = self.gpr(prefix.vvvv_reg());
                // Scalar SMIR shifts apply the source architecture's count
                // mask. Preserve the architectural count operand directly,
                // matching the VEX BMI2 lift shape and avoiding a redundant
                // virtual-register definition at the native frontier.
                let amount = SrcOperand::Reg(count);
                let op_kind = match kind {
                    ApxBmiShiftKind::Sarx => OpKind::Sar {
                        dst: self.gpr(modrm.reg),
                        src: rm_src,
                        amount,
                        width,
                        flags: FlagUpdate::None,
                    },
                    ApxBmiShiftKind::Shlx => OpKind::Shl {
                        dst: self.gpr(modrm.reg),
                        src: rm_src,
                        amount,
                        width,
                        flags: FlagUpdate::None,
                    },
                    ApxBmiShiftKind::Shrx => OpKind::Shr {
                        dst: self.gpr(modrm.reg),
                        src: rm_src,
                        amount,
                        width,
                        flags: FlagUpdate::None,
                    },
                };
                ops.push(SmirOp::new(OpId(ops.len() as u16), pc, op_kind));
            }
        }

        Ok(Self::retain_apx_bmi_requirement(
            pc,
            LiftResult::fallthrough(ops, prefix.bytes + 1 + modrm.bytes_consumed),
        ))
    }

    pub(crate) fn lift_apx_bmi_rorx(
        &self,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let prefix = decode_apx_evex_prefix_for_map(bytes, pc, 3)?;
        if prefix.pp != 0x03
            || !Self::apx_bmi_payload_is_valid(prefix, bytes, false)
            || prefix.vvvv != 0x0F
            || !prefix.v_prime
        {
            return Ok(Self::apx_invalid_opcode(prefix.bytes + 1));
        }

        let modrm_byte = Self::apx_bmi_modrm_byte(prefix, bytes, pc)?;
        if modrm_byte >> 6 == 3 && !prefix.x4 {
            return Ok(Self::apx_modrm_invalid_opcode(prefix));
        }

        let width = if prefix.w { OpWidth::W64 } else { OpWidth::W32 };
        let mem_width = if prefix.w { MemWidth::B8 } else { MemWidth::B4 };
        let modrm_prefix = prefix.as_modrm_prefix(prefix.bytes + 1);
        let modrm = decode_modrm(&bytes[prefix.bytes + 1..], &modrm_prefix, pc)?;
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
        let src = if modrm.is_memory {
            let x86_addr = modrm.addr.as_ref().unwrap();
            let (addr, pre_ops) = self.x86_addr_to_smir(x86_addr, next_pc, ctx);
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
        } else {
            self.gpr(modrm.rm)
        };

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

        Ok(Self::retain_apx_bmi_requirement(
            pc,
            LiftResult::fallthrough(ops, prefix.bytes + 1 + modrm.bytes_consumed + 1),
        ))
    }
}
