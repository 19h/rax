//! Sign/zero-extension lowering

use crate::smir::lower::x86_64::*;
use std::collections::HashMap;

use crate::smir::ir::flags::{FlagSet, FlagUpdate};
use crate::smir::ir::ops::{
    OpKind, SmirOp, X86AdxKind, X86AluEncoding, X86BlsKind, X86CacheControlKind, X86CountKind,
    X86OpHint, X86RepMode, X86SsePrefix, X86StringKind, X86VecAlign, X86VecMap,
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
    X86_GUEST_XCR0_OFFSET, X86_GUEST_XGETBV1_OFFSET, X86_GUEST_ZMM_OFFSET, X86_HOST_MXCSR_OFFSET,
};

impl X86_64Lowerer {
    pub(crate) fn lower_op_extensions(
        &mut self,
        op: &crate::smir::ir::ops::SmirOp,
    ) -> Result<(), LowerError> {
        let is_non_accumulating_madd = matches!(
            &op.kind,
            OpKind::VDotProduct {
                acc: VReg::Imm(0),
                mask: None,
                src_elem: VecElementType::I8,
                acc_elem: VecElementType::I16,
                src1_unsigned: true,
                saturate: true,
                zeroing: false,
                ..
            } | OpKind::VDotProduct {
                acc: VReg::Imm(0),
                mask: None,
                src_elem: VecElementType::I16,
                acc_elem: VecElementType::I32,
                src1_unsigned: false,
                saturate: false,
                zeroing: false,
                ..
            }
        );
        let is_classic_mpsadbw = matches!(
            (&op.kind, op.x86_hint),
            (
                OpKind::VMpsadbw { .. },
                Some(X86OpHint::SseOp { .. } | X86OpHint::VexOp { .. })
            )
        );
        if !is_non_accumulating_madd && !is_classic_mpsadbw {
            if let Some(result) = avx10::Avx10Lowerer::new().try_lower(&op.kind, &mut self.code) {
                return result;
            }
        }

        let alu_hint = match op.x86_hint {
            Some(X86OpHint::AluEncoding(enc)) => Some(enc),
            _ => None,
        };

        match &op.kind {
            // ================================================================
            // Extensions
            // ================================================================
            OpKind::ZeroExtend {
                dst,
                src,
                from_width,
                to_width,
            } => {
                if x86_state_backed_gpr_extend_candidate(op) {
                    if !x86_state_backed_gpr_extend_valid(op) {
                        return Err(LowerError::InvalidOperand {
                            op: "state-backed MOVZX".to_string(),
                            operand: format!(
                                "invalid x86 GPR extension {from_width:?}->{to_width:?}"
                            ),
                        });
                    }
                    return self.lower_state_backed_gpr_extend(
                        *dst,
                        *src,
                        *from_width,
                        *to_width,
                        false,
                        matches!(op.x86_hint, Some(X86OpHint::LegacyHighByteReg)),
                    );
                }
                let dst_reg = self.get_dst_reg(*dst)?;
                let src_reg = self.get_reg(*src)?;

                if matches!(op.x86_hint, Some(X86OpHint::LegacyHighByteReg)) {
                    Self::ensure_legacy_high_byte_movx_shape(
                        "MOVZX",
                        src_reg,
                        *from_width,
                        *to_width,
                    )?;
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_push(src_reg);
                    emitter.emit_movzx_rm_disp(
                        dst_reg,
                        PhysReg::Rsp,
                        1,
                        DispSize::Auto,
                        *from_width,
                        *to_width,
                    );
                    emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, 8);
                } else {
                    let mut emitter = X86Emitter::new(&mut self.code);
                    if *from_width == OpWidth::W32 && *to_width == OpWidth::W64 {
                        // 32-bit mov automatically zero-extends
                        emitter.emit_mov_rr(dst_reg, src_reg, OpWidth::W32);
                    } else {
                        emitter.emit_movzx(dst_reg, src_reg, *from_width, *to_width);
                    }
                }
            }

            OpKind::SignExtend {
                dst,
                src,
                from_width,
                to_width,
            } => {
                if x86_state_backed_gpr_extend_candidate(op) {
                    if !x86_state_backed_gpr_extend_valid(op) {
                        return Err(LowerError::InvalidOperand {
                            op: "state-backed MOVSX".to_string(),
                            operand: format!(
                                "invalid x86 GPR extension {from_width:?}->{to_width:?}"
                            ),
                        });
                    }
                    return self.lower_state_backed_gpr_extend(
                        *dst,
                        *src,
                        *from_width,
                        *to_width,
                        true,
                        matches!(op.x86_hint, Some(X86OpHint::LegacyHighByteReg)),
                    );
                }
                let dst_reg = self.get_dst_reg(*dst)?;
                let src_reg = self.get_reg(*src)?;

                if matches!(op.x86_hint, Some(X86OpHint::LegacyHighByteReg)) {
                    Self::ensure_legacy_high_byte_movx_shape(
                        "MOVSX",
                        src_reg,
                        *from_width,
                        *to_width,
                    )?;
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_push(src_reg);
                    emitter.emit_movsx_rm_disp(
                        dst_reg,
                        PhysReg::Rsp,
                        1,
                        DispSize::Auto,
                        *from_width,
                        *to_width,
                    );
                    emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, 8);
                } else {
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_movsx(dst_reg, src_reg, *from_width, *to_width);
                }
            }

            OpKind::Cwd { dst, src, width } => {
                if !matches!(src, VReg::Arch(ArchReg::X86(X86Reg::Rax)))
                    || !matches!(dst, VReg::Arch(ArchReg::X86(X86Reg::Rdx)))
                {
                    return Err(LowerError::InvalidOperand {
                        op: "Cwd".to_string(),
                        operand: "requires RAX/RDX".to_string(),
                    });
                }

                let mut emitter = X86Emitter::new(&mut self.code);
                match width {
                    OpWidth::W16 => emitter.emit_cwd(),
                    OpWidth::W32 => emitter.emit_cdq(),
                    OpWidth::W64 => emitter.emit_cqo(),
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: format!("Cwd width {:?}", width),
                        });
                    }
                }
            }

            _ => return self.lower_op_x87(op),
        }

        Ok(())
    }
}
