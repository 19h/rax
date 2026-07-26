//! Exact VEX BMI1/BMI2 opcode admission and reserved-encoding frontiers.

use crate::smir::lift::x86_64::*;
use std::collections::{HashMap, HashSet};

use crate::smir::ir::flags::{FlagSet, FlagUpdate};
use crate::smir::ir::memory::MemoryError;
use crate::smir::ir::ops::{
    OpKind, SmirOp, X86AdxKind, X86AluEncoding, X86BlsKind, X86CacheControlKind, X86CountKind,
    X86OpHint, X86RepMode, X86SsePrefix, X86StringKind, X86ThreeDNowKind, X86VecAlign, X86VecMap,
    X86X87ArithmeticDestination, X86X87ArithmeticSource, X86X87CompareSource, X86X87Constant,
    X86X87ControlKind, X86X87DataKind, X86X87EnvWidth, X86X87FloatWidth, X86X87IntWidth,
    X86XSaveKind,
};
use crate::smir::ir::types::*;
use crate::smir::ir::{
    CallTarget, CallingConv, FunctionAttrs, SmirBlock, SmirFunction, Terminator, TrapKind,
    X86InstructionBytes,
};
use crate::smir::lift::{
    ControlFlow, LiftContext, LiftError, LiftResult, MemoryReader, SmirLifter,
};

impl X86_64Lifter {
    fn vex_bmi_invalid_opcode(bytes_consumed: usize) -> LiftResult {
        LiftResult {
            ops: Vec::new(),
            bytes_consumed,
            control_flow: ControlFlow::Trap {
                kind: TrapKind::InvalidOpcode,
            },
            branch_targets: Vec::new(),
        }
    }

    fn vex_bmi_modrm(&self, prefix: VecPrefix, bytes: &[u8], pc: u64) -> Result<ModRm, LiftError> {
        let modrm_offset = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            rex: prefix.rex,
            address_size_override: prefix.address_size_override,
            segment_override: prefix.segment_override,
            cursor: modrm_offset,
            ..X86Prefix::default()
        };
        decode_modrm(&bytes[modrm_offset.min(bytes.len())..], &modrm_prefix, pc).map_err(|error| {
            match error {
                LiftError::Incomplete { addr, have, need } => LiftError::Incomplete {
                    addr,
                    have: modrm_offset + have,
                    need: modrm_offset + need,
                },
                error => error,
            }
        })
    }

    /// Dispatch the complete VEX Map 0F38 BMI1/BMI2 opcode cluster. Intel SDM
    /// Vol. 2A Table 2-28 and the individual opcode tables assign only L=0 and
    /// the mandatory-prefix combinations below. Once prefix plus opcode proves
    /// a reservation, no ModR/M or address byte is fetched.
    pub(crate) fn lift_vex_bmi_0f38(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.encoding != VecEncodingKind::Vex || prefix.map != X86VecMap::Map0F38 {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let opcode_end = prefix.bytes + 1;
        if prefix.l_bits != 0 {
            return Ok(Self::vex_bmi_invalid_opcode(opcode_end));
        }

        match (opcode, prefix.pp) {
            (0xF2, X86SsePrefix::None) => {
                let modrm = self.vex_bmi_modrm(prefix, bytes, pc)?;
                self.lift_vex_andn_0f38(prefix, modrm, bytes, pc, ctx)
            }
            (0xF3, X86SsePrefix::None) => {
                let Some(&modrm_byte) = bytes.get(opcode_end) else {
                    return Err(LiftError::Incomplete {
                        addr: pc,
                        have: bytes.len(),
                        need: opcode_end + 1,
                    });
                };
                if !matches!((modrm_byte >> 3) & 7, 1..=3) {
                    return Ok(Self::vex_bmi_invalid_opcode(opcode_end + 1));
                }
                let modrm = self.vex_bmi_modrm(prefix, bytes, pc)?;
                self.lift_vex_bls_0f38(prefix, modrm, bytes, pc, ctx)
            }
            (0xF5, X86SsePrefix::None) | (0xF7, X86SsePrefix::None) => {
                let modrm = self.vex_bmi_modrm(prefix, bytes, pc)?;
                self.lift_vex_bzhi_bextr_0f38(prefix, modrm, opcode, bytes, pc, ctx)
            }
            (0xF5, X86SsePrefix::Rep | X86SsePrefix::Repne) => {
                let modrm = self.vex_bmi_modrm(prefix, bytes, pc)?;
                self.lift_vex_pdep_pext_0f38(prefix, modrm, bytes, pc, ctx)
            }
            (0xF6, X86SsePrefix::Repne) => {
                let modrm = self.vex_bmi_modrm(prefix, bytes, pc)?;
                self.lift_vex_mulx_0f38(prefix, modrm, bytes, pc, ctx)
            }
            (0xF7, X86SsePrefix::OpSize | X86SsePrefix::Rep | X86SsePrefix::Repne) => {
                let modrm = self.vex_bmi_modrm(prefix, bytes, pc)?;
                self.lift_vex_bmi2_shift_0f38(prefix, modrm, bytes, pc, ctx)
            }
            (0xF2 | 0xF3 | 0xF5 | 0xF6 | 0xF7, _) => Ok(Self::vex_bmi_invalid_opcode(opcode_end)),
            _ => Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            }),
        }
    }

    /// Admit only VEX.L0.F2.0F3A.F0 with reserved `vvvv=1111b`. Every other
    /// prefix cell is #UD at the opcode frontier; an assigned cell continues to
    /// fetch ModR/M and imm8 so truncated instructions remain precise.
    pub(crate) fn lift_vex_bmi2_rorx_dispatch(
        &self,
        prefix: VecPrefix,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.encoding != VecEncodingKind::Vex || prefix.map != X86VecMap::Map0F3A {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        if prefix.l_bits != 0 || prefix.pp != X86SsePrefix::Repne || prefix.vvvv != 0 {
            return Ok(Self::vex_bmi_invalid_opcode(prefix.bytes + 1));
        }
        let modrm = self.vex_bmi_modrm(prefix, bytes, pc)?;
        let imm_offset = prefix.bytes + 1 + modrm.bytes_consumed;
        if bytes.len() <= imm_offset {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: bytes.len(),
                need: imm_offset + 1,
            });
        }
        self.lift_vex_bmi2_rorx_0f3a(prefix, modrm, bytes, pc, ctx)
    }
}
