//! Register helpers, configuration setters, and encoding predicates

use crate::smir::lower::x86_64::*;
use std::collections::HashMap;

use crate::smir::ir::flags::{FlagSet, FlagUpdate};
use crate::smir::ir::ops::{
    OpKind, SmirOp, X86AdxKind, X86AluEncoding, X86BlsKind, X86CacheControlKind, X86CountKind,
    X86OpHint, X86RepMode, X86SsePrefix, X86StringKind, X86VecAlign, X86VecMap, X86X87ControlKind,
};
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, Condition, DispSize, FenceKind, FpRoundMode, GuestAddr, MemWidth,
    OpWidth, ShiftOp, SignExtend, SrcOperand, VLaneOp, VReg, VecCmpCond, VecElementType,
    VecUnaryOp, VecWidth, X86Reg,
};
use crate::smir::ir::{
    CallTarget, SmirBlock, SmirFunction, Terminator, X86InstructionBytes,
    x86_evex_native_replay_spans,
};

use crate::smir::lower::regalloc::{PhysReg, RegAlloc, RegLocation};
use crate::smir::lower::{
    CodeBuffer, LowerError, LowerResult, RelocKind, RelocTarget, Relocation, SmirLowerer,
    X86_GUEST_APX_ENABLED_OFFSET, X86_GUEST_CALL_FN_OFFSET, X86_GUEST_CPL_OFFSET,
    X86_GUEST_CR0_OFFSET, X86_GUEST_CR4_OFFSET, X86_GUEST_CTX_OFFSET, X86_GUEST_EXIT_PC_OFFSET,
    X86_GUEST_FS_BASE_OFFSET, X86_GUEST_GS_BASE_OFFSET, X86_GUEST_K_OFFSET,
    X86_GUEST_LOAD_FN_OFFSET, X86_GUEST_MXCSR_OFFSET, X86_GUEST_PAIR_LOAD_FN_OFFSET,
    X86_GUEST_PAIR_STORE_FN_OFFSET, X86_GUEST_RFLAGS_OFFSET, X86_GUEST_STORE_FN_OFFSET,
    X86_GUEST_TSC_AUX_OFFSET, X86_GUEST_VEC_LOAD_FN_OFFSET, X86_GUEST_VEC_STORE_FN_OFFSET,
    X86_GUEST_X87_TAG_WORD_OFFSET, X86_GUEST_XCR0_OFFSET, X86_GUEST_XGETBV1_OFFSET,
    X86_GUEST_ZMM_OFFSET, X86_HOST_MXCSR_OFFSET, X86_STATE_PTR_AT_RBP,
};

impl X86_64Lowerer {
    /// Enable lowering `Terminator::Call` as a runtime call-out (see `call_helpers`).
    pub fn set_call_helpers(&mut self, on: bool) {
        self.call_helpers = on;
    }

    pub fn set_pcrel_adjust(&mut self, adjust: bool) {
        self.pcrel_adjust = adjust;
    }

    /// Materialize guest-anchored PC-relative `LEA` results as immediates. This
    /// is the relocation-free form required by independently allocated JIT code.
    pub fn set_guest_pcrel_lea_immediates(&mut self, on: bool) {
        self.guest_pcrel_lea_immediates = on;
    }

    /// Get a physical register for a VReg, loading from stack if needed
    pub(crate) fn get_reg(&mut self, vreg: VReg) -> Result<PhysReg, LowerError> {
        let loc = self.regalloc.alloc_vreg(vreg)?;
        match loc {
            RegLocation::Register(r) => Ok(r),
            RegLocation::Stack(offset) => {
                // Load from stack into a temp register
                let temp = self.regalloc.get_scratch()?;
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_mov_rm(temp, PhysReg::Rbp, offset, OpWidth::W64);
                Ok(temp)
            }
            RegLocation::Constant(val) => {
                // Load constant into a register
                let temp = self.regalloc.get_scratch()?;
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_mov_ri(temp, val, OpWidth::W64);
                Ok(temp)
            }
            RegLocation::Unallocated => Err(LowerError::RegisterAllocationFailed {
                reason: "vreg not allocated".to_string(),
            }),
        }
    }

    /// Get the destination register for a VReg
    pub(crate) fn get_dst_reg(&mut self, vreg: VReg) -> Result<PhysReg, LowerError> {
        // Reject non-state-backed guest writes to architectural RSP/RBP: a
        // lowered block runs on the HOST stack, so mapping either destination
        // directly would let the guest pivot the host return path.
        Self::ensure_native_stack_dst_safe(vreg)?;
        let loc = self.regalloc.alloc_vreg(vreg)?;
        match loc {
            RegLocation::Register(r) => Ok(r),
            RegLocation::Stack(_) | RegLocation::Constant(_) | RegLocation::Unallocated => {
                Err(LowerError::RegisterAllocationFailed {
                    reason: "destination must be a register".to_string(),
                })
            }
        }
    }

    pub(crate) fn ensure_legacy_high_byte_movx_shape(
        op: &'static str,
        src: PhysReg,
        from_width: OpWidth,
        to_width: OpWidth,
    ) -> Result<(), LowerError> {
        if from_width != OpWidth::W8
            || !matches!(to_width, OpWidth::W16 | OpWidth::W32 | OpWidth::W64)
            || !matches!(
                src,
                PhysReg::Rax | PhysReg::Rcx | PhysReg::Rdx | PhysReg::Rbx
            )
        {
            return Err(LowerError::InvalidOperand {
                op: op.to_string(),
                operand: format!(
                    "legacy high-byte extension requires AH/CH/DH/BH and W16/W32/W64 destination; got {src:?} {from_width:?}->{to_width:?}"
                ),
            });
        }
        Ok(())
    }

    pub(crate) fn ensure_flag_stack_operands_safe(
        op: &'static str,
        regs: &[PhysReg],
    ) -> Result<(), LowerError> {
        if regs
            .iter()
            .any(|reg| matches!(reg, PhysReg::Rsp | PhysReg::Rbp))
        {
            return Err(LowerError::InvalidOperand {
                op: op.to_string(),
                operand: "RSP/RBP operands are not safe with flag-stack lowering".to_string(),
            });
        }

        Ok(())
    }

    pub(crate) fn pred_store_src_to_vreg(src: &SrcOperand) -> Result<VReg, LowerError> {
        match src {
            SrcOperand::Reg(reg) => Ok(*reg),
            SrcOperand::Imm(imm) | SrcOperand::Imm64(imm) => Ok(VReg::Imm(*imm)),
            other => Err(LowerError::UnsupportedOp {
                op: format!("PredStore source {other:?}"),
            }),
        }
    }

    pub(crate) fn is_rsp(&self, vreg: VReg) -> bool {
        matches!(vreg, VReg::Arch(ArchReg::X86(X86Reg::Rsp)))
    }

    pub(crate) fn sse_prefix(&self, hint: Option<X86OpHint>) -> Option<u8> {
        match hint {
            Some(X86OpHint::SseMov { prefix, .. }) | Some(X86OpHint::SseOp { prefix, .. }) => {
                match prefix {
                    X86SsePrefix::None => None,
                    X86SsePrefix::OpSize => Some(0x66),
                    X86SsePrefix::Rep => Some(0xF3),
                    X86SsePrefix::Repne => Some(0xF2),
                }
            }
            _ => None,
        }
    }

    pub(crate) fn sse_opcode(&self, hint: Option<X86OpHint>, default: u8) -> u8 {
        match hint {
            Some(X86OpHint::SseMov { opcode, .. }) | Some(X86OpHint::SseOp { opcode, .. }) => {
                opcode
            }
            _ => default,
        }
    }

    pub(crate) fn vec_hint(&self, hint: Option<X86OpHint>) -> Option<VecEncoding> {
        match hint {
            Some(X86OpHint::VexOp {
                map,
                pp,
                opcode,
                width,
                w,
            }) => Some(VecEncoding {
                kind: VecEncodingKind::Vex,
                map,
                pp,
                opcode,
                width,
                w,
            }),
            Some(X86OpHint::EvexOp {
                map,
                pp,
                opcode,
                width,
                w,
            }) => Some(VecEncoding {
                kind: VecEncodingKind::Evex,
                map,
                pp,
                opcode,
                width,
                w,
            }),
            _ => None,
        }
    }

    pub(crate) fn vec_requires_vex(&self, regs: &[PhysReg]) -> bool {
        regs.iter()
            .any(|reg| reg.is_ymm() || reg.is_zmm() || reg.vec_ext2() != 0)
    }

    pub(crate) fn vec_requires_evex(&self, width: VecWidth, regs: &[PhysReg]) -> bool {
        width == VecWidth::V512 || regs.iter().any(|reg| reg.is_zmm() || reg.vec_ext2() != 0)
    }

    pub(crate) fn vec_move_pp(&self, hint: Option<X86OpHint>) -> X86SsePrefix {
        match hint {
            Some(X86OpHint::VecAlign(X86VecAlign::Aligned)) => X86SsePrefix::OpSize,
            Some(X86OpHint::VecAlign(X86VecAlign::Unaligned)) => X86SsePrefix::Rep,
            _ => X86SsePrefix::Rep,
        }
    }

    pub(crate) fn vec_move_prefix(&self, hint: Option<X86OpHint>) -> Option<u8> {
        match self.vec_move_pp(hint) {
            X86SsePrefix::OpSize => Some(0x66),
            X86SsePrefix::Rep => Some(0xF3),
            X86SsePrefix::Repne => Some(0xF2),
            X86SsePrefix::None => None,
        }
    }

    pub(crate) fn vec_width_from_lanes(&self, elem: VecElementType, lanes: u8) -> Option<VecWidth> {
        if lanes == VecWidth::V128.lanes(elem) as u8 {
            Some(VecWidth::V128)
        } else if lanes == VecWidth::V256.lanes(elem) as u8 {
            Some(VecWidth::V256)
        } else if lanes == VecWidth::V512.lanes(elem) as u8 {
            Some(VecWidth::V512)
        } else {
            None
        }
    }

    pub(crate) fn x86_gpr_index(v: VReg) -> Option<u8> {
        match v {
            VReg::Arch(ArchReg::X86(r)) => r.gpr_index(),
            _ => None,
        }
    }
}
