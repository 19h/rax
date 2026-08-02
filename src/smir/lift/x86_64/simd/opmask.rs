//! VEX-encoded AVX-512 opmask instruction lifting.

use crate::smir::ir::ops::{
    OpKind, SmirOp, X86OpmaskBinaryKind, X86OpmaskMoveDestination, X86OpmaskMoveSource,
    X86OpmaskOp, X86OpmaskShiftKind, X86OpmaskTestKind, X86SsePrefix, X86VecMap,
};
use crate::smir::ir::types::{ArchReg, OpId, OpWidth, VReg, X86Reg};
use crate::smir::lift::x86_64::{
    VecEncodingKind, VecPrefix, X86_64Lifter, X86Prefix, decode_modrm,
};
use crate::smir::lift::{LiftContext, LiftError, LiftResult};

impl X86_64Lifter {
    pub(crate) fn lift_vex_opmask(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.encoding != VecEncodingKind::Vex {
            return Self::invalid_opmask(bytes, pc);
        }

        match prefix.map {
            X86VecMap::Map0F => self.lift_vex_opmask_map_0f(prefix, opcode, bytes, pc, ctx),
            X86VecMap::Map0F3A => self.lift_vex_opmask_map_0f3a(prefix, opcode, bytes, pc, ctx),
            _ => Self::invalid_opmask(bytes, pc),
        }
    }

    fn lift_vex_opmask_map_0f(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let cursor = prefix.bytes + 1;
        let modrm_prefix = Self::opmask_modrm_prefix(prefix, cursor);
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        let instruction_len = cursor + modrm.bytes_consumed;
        let next_pc = pc + instruction_len as u64;

        let op = match opcode {
            0x41 | 0x42 | 0x45 | 0x46 | 0x47 | 0x4A => {
                if prefix.l_bits != 1
                    || modrm.is_memory
                    || modrm.reg >= 8
                    || modrm.rm >= 8
                    || prefix.vvvv >= 8
                {
                    return Self::invalid_opmask(bytes, pc);
                }
                let width = Self::opmask_logic_width(prefix, bytes, pc)?;
                let kind = match opcode {
                    0x41 => X86OpmaskBinaryKind::And,
                    0x42 => X86OpmaskBinaryKind::AndNot,
                    0x45 => X86OpmaskBinaryKind::Or,
                    0x46 => X86OpmaskBinaryKind::Xnor,
                    0x47 => X86OpmaskBinaryKind::Xor,
                    0x4A => X86OpmaskBinaryKind::Add,
                    _ => unreachable!(),
                };
                X86OpmaskOp::Binary {
                    kind,
                    dst: Self::opmask_reg(modrm.reg),
                    src1: Self::opmask_reg(prefix.vvvv),
                    src2: Self::opmask_reg(modrm.rm),
                    width,
                }
            }
            0x4B => {
                if prefix.l_bits != 1
                    || modrm.is_memory
                    || modrm.reg >= 8
                    || modrm.rm >= 8
                    || prefix.vvvv >= 8
                {
                    return Self::invalid_opmask(bytes, pc);
                }
                let width = match (prefix.pp, prefix.w) {
                    (X86SsePrefix::OpSize, false) => OpWidth::W16,
                    (X86SsePrefix::None, false) => OpWidth::W32,
                    (X86SsePrefix::None, true) => OpWidth::W64,
                    _ => return Self::invalid_opmask(bytes, pc),
                };
                X86OpmaskOp::Unpack {
                    dst: Self::opmask_reg(modrm.reg),
                    src1: Self::opmask_reg(prefix.vvvv),
                    src2: Self::opmask_reg(modrm.rm),
                    width,
                }
            }
            0x44 => {
                if prefix.l_bits != 0
                    || prefix.vvvv != 0
                    || modrm.is_memory
                    || modrm.reg >= 8
                    || modrm.rm >= 8
                {
                    return Self::invalid_opmask(bytes, pc);
                }
                X86OpmaskOp::Not {
                    dst: Self::opmask_reg(modrm.reg),
                    src: Self::opmask_reg(modrm.rm),
                    width: Self::opmask_logic_width(prefix, bytes, pc)?,
                }
            }
            0x90 | 0x91 => {
                if prefix.l_bits != 0
                    || prefix.vvvv != 0
                    || modrm.reg >= 8
                    || (opcode == 0x91 && !modrm.is_memory)
                {
                    return Self::invalid_opmask(bytes, pc);
                }
                let width = Self::opmask_logic_width(prefix, bytes, pc)?;
                if modrm.is_memory {
                    let (addr, mut ops) = self.x86_addr_to_smir(
                        modrm.addr.as_ref().expect("memory ModR/M has an address"),
                        next_pc,
                        ctx,
                    );
                    let op = if opcode == 0x90 {
                        X86OpmaskOp::MoveToMask {
                            dst: Self::opmask_reg(modrm.reg),
                            src: X86OpmaskMoveSource::Memory(addr),
                            width,
                        }
                    } else {
                        X86OpmaskOp::MoveFromMask {
                            dst: X86OpmaskMoveDestination::Memory(addr),
                            src: Self::opmask_reg(modrm.reg),
                            width,
                        }
                    };
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::X86Opmask(op),
                    ));
                    return Ok(LiftResult::fallthrough(ops, instruction_len));
                }
                if modrm.rm >= 8 {
                    return Self::invalid_opmask(bytes, pc);
                }
                if opcode == 0x90 {
                    X86OpmaskOp::MoveToMask {
                        dst: Self::opmask_reg(modrm.reg),
                        src: X86OpmaskMoveSource::Mask(Self::opmask_reg(modrm.rm)),
                        width,
                    }
                } else {
                    unreachable!("opcode 91 register form rejected above")
                }
            }
            0x92 => {
                if prefix.l_bits != 0 || prefix.vvvv != 0 || modrm.is_memory || modrm.reg >= 8 {
                    return Self::invalid_opmask(bytes, pc);
                }
                let width = Self::opmask_gpr_move_width(prefix, bytes, pc)?;
                X86OpmaskOp::MoveToMask {
                    dst: Self::opmask_reg(modrm.reg),
                    src: X86OpmaskMoveSource::Gpr(self.gpr(modrm.rm)),
                    width,
                }
            }
            0x93 => {
                if prefix.l_bits != 0 || prefix.vvvv != 0 || modrm.is_memory || modrm.rm >= 8 {
                    return Self::invalid_opmask(bytes, pc);
                }
                let width = Self::opmask_gpr_move_width(prefix, bytes, pc)?;
                // Opcode 93 encodes the GPR destination in ModR/M.reg and the
                // K source in ModR/M.r/m. VEX.R therefore extends the GPR (for
                // example C4 61 FB 93 CD is KMOVQ r9,k5).
                X86OpmaskOp::MoveFromMask {
                    dst: X86OpmaskMoveDestination::Gpr(self.gpr(modrm.reg)),
                    src: Self::opmask_reg(modrm.rm),
                    width,
                }
            }
            0x98 | 0x99 => {
                if prefix.l_bits != 0
                    || prefix.vvvv != 0
                    || modrm.is_memory
                    || modrm.reg >= 8
                    || modrm.rm >= 8
                {
                    return Self::invalid_opmask(bytes, pc);
                }
                X86OpmaskOp::Test {
                    kind: if opcode == 0x99 {
                        X86OpmaskTestKind::And
                    } else {
                        X86OpmaskTestKind::Or
                    },
                    src1: Self::opmask_reg(modrm.reg),
                    src2: Self::opmask_reg(modrm.rm),
                    width: Self::opmask_logic_width(prefix, bytes, pc)?,
                }
            }
            _ => return Self::invalid_opmask(bytes, pc),
        };

        Ok(LiftResult::fallthrough(
            vec![SmirOp::new(OpId(0), pc, OpKind::X86Opmask(op))],
            instruction_len,
        ))
    }

    fn lift_vex_opmask_map_0f3a(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        _ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.pp != X86SsePrefix::OpSize || prefix.l_bits != 0 || prefix.vvvv != 0 {
            return Self::invalid_opmask(bytes, pc);
        }
        let width = match (opcode, prefix.w) {
            (0x30 | 0x32, false) => OpWidth::W8,
            (0x30 | 0x32, true) => OpWidth::W16,
            (0x31 | 0x33, false) => OpWidth::W32,
            (0x31 | 0x33, true) => OpWidth::W64,
            _ => return Self::invalid_opmask(bytes, pc),
        };
        let cursor = prefix.bytes + 1;
        let modrm = decode_modrm(
            &bytes[cursor..],
            &Self::opmask_modrm_prefix(prefix, cursor),
            pc,
        )?;
        if modrm.is_memory || modrm.reg >= 8 || modrm.rm >= 8 {
            return Self::invalid_opmask(bytes, pc);
        }
        let imm_offset = cursor + modrm.bytes_consumed;
        let Some(&count) = bytes.get(imm_offset) else {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: bytes.len(),
                need: imm_offset + 1,
            });
        };
        let op = X86OpmaskOp::Shift {
            kind: if matches!(opcode, 0x32 | 0x33) {
                X86OpmaskShiftKind::Left
            } else {
                X86OpmaskShiftKind::Right
            },
            dst: Self::opmask_reg(modrm.reg),
            src: Self::opmask_reg(modrm.rm),
            width,
            count,
        };
        Ok(LiftResult::fallthrough(
            vec![SmirOp::new(OpId(0), pc, OpKind::X86Opmask(op))],
            imm_offset + 1,
        ))
    }

    fn opmask_logic_width(prefix: VecPrefix, bytes: &[u8], pc: u64) -> Result<OpWidth, LiftError> {
        match (prefix.pp, prefix.w) {
            (X86SsePrefix::None, false) => Ok(OpWidth::W16),
            (X86SsePrefix::OpSize, false) => Ok(OpWidth::W8),
            (X86SsePrefix::None, true) => Ok(OpWidth::W64),
            (X86SsePrefix::OpSize, true) => Ok(OpWidth::W32),
            _ => Self::invalid_opmask(bytes, pc),
        }
    }

    fn opmask_gpr_move_width(
        prefix: VecPrefix,
        bytes: &[u8],
        pc: u64,
    ) -> Result<OpWidth, LiftError> {
        match (prefix.pp, prefix.w) {
            (X86SsePrefix::None, false) => Ok(OpWidth::W16),
            (X86SsePrefix::OpSize, false) => Ok(OpWidth::W8),
            (X86SsePrefix::Repne, false) => Ok(OpWidth::W32),
            (X86SsePrefix::Repne, true) => Ok(OpWidth::W64),
            _ => Self::invalid_opmask(bytes, pc),
        }
    }

    fn opmask_modrm_prefix(prefix: VecPrefix, cursor: usize) -> X86Prefix {
        X86Prefix {
            ..prefix.modrm_prefix(cursor)
        }
    }

    fn opmask_reg(index: u8) -> VReg {
        VReg::Arch(ArchReg::X86(X86Reg::K(index)))
    }

    fn invalid_opmask<T>(bytes: &[u8], pc: u64) -> Result<T, LiftError> {
        Err(LiftError::InvalidEncoding {
            addr: pc,
            bytes: bytes.to_vec(),
        })
    }
}
