//! Miscellaneous lowering

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
    pub(crate) fn lower_op_misc(
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
            // Misc
            // ================================================================
            OpKind::Fence { kind } => match kind {
                FenceKind::LoadLoad => self.code.emit_bytes(&[0x0F, 0xAE, 0xE8]),
                FenceKind::Full => self.code.emit_bytes(&[0x0F, 0xAE, 0xF0]),
                FenceKind::StoreStore => self.code.emit_bytes(&[0x0F, 0xAE, 0xF8]),
                FenceKind::InstructionSerialize => self.emit_x86_serialize(),
                other => {
                    return Err(LowerError::UnsupportedOp {
                        op: format!("x86 native fence {other:?}"),
                    });
                }
            },

            OpKind::Nop => {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_nop();
            }

            OpKind::X86LoadMxcsr { .. } => self.emit_x86_load_mxcsr(op)?,
            OpKind::X86RequireApx => self.emit_x86_require_apx(op)?,
            OpKind::X86RequireSse4a => self.emit_x86_require_sse4a(op)?,
            OpKind::X86RequireTbm => self.emit_x86_require_tbm(op)?,
            OpKind::X86RequireXop => self.emit_x86_require_xop(op)?,
            OpKind::X86Sse4aBitfield { .. } => self.emit_x86_sse4a_bitfield(op)?,
            OpKind::X86Sse4aMovntStore { .. } => self.emit_x86_sse4a_movnt_store(op)?,

            OpKind::Breakpoint => {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_int3();
            }

            OpKind::X86Leave(..) => {
                return Err(LowerError::UnsupportedOp {
                    op: "x86 LEAVE requires exact helper-backed lowering".into(),
                });
            }

            OpKind::X86XTest => {
                // Preserve all non-status RFLAGS without consuming a GPR:
                // pushfq; and qword [rsp], !0x08D5; or qword [rsp], 0x40;
                // popfq.
                self.code.emit_bytes(&[
                    0x9C, 0x48, 0x81, 0x24, 0x24, 0x2A, 0xF7, 0xFF, 0xFF, 0x48, 0x83, 0x0C, 0x24,
                    0x40, 0x9D,
                ]);
            }

            OpKind::X86Cpuid { .. } => self.emit_x86_cpuid(op)?,

            OpKind::X86Clts => self.emit_x86_clts(op)?,

            OpKind::X86StoreMxcsr { .. } => self.emit_x86_store_mxcsr(op)?,

            OpKind::X86Msr(..) => self.emit_x86_msr(op)?,

            OpKind::X86WaitPkg(..) => self.emit_x86_waitpkg(op)?,

            OpKind::X86ReadControl { .. } => self.emit_x86_read_control(op)?,

            OpKind::X86Smsw(..) => self.emit_x86_smsw(op)?,

            OpKind::X86SystemSelectorStore(..) => self.emit_x86_system_selector_store(op)?,

            OpKind::X86SystemSelectorLoad(..) => self.emit_x86_system_selector_load(op)?,

            OpKind::X86SelectorVerify(..) => self.emit_x86_selector_verify(op)?,
            OpKind::X86SelectorQuery(..) => self.emit_x86_selector_query(op)?,

            OpKind::X86FarJump(..) => self.emit_x86_far_jump(op)?,
            OpKind::X86FarCall(..) => self.emit_x86_far_call(op)?,
            OpKind::X86FarReturn(..) => self.emit_x86_far_return(op)?,
            OpKind::X86FastSystemTransfer(..) => self.emit_x86_fast_system_transfer(op)?,

            OpKind::X86Lmsw(..) => self.emit_x86_lmsw(op)?,

            OpKind::X86DescriptorTableStore(..) => self.emit_x86_descriptor_table_store(op)?,

            OpKind::X86DescriptorTableLoad(..) => self.emit_x86_descriptor_table_load(op)?,

            OpKind::X86Invlpg(..) => self.emit_x86_invlpg(op)?,

            OpKind::X86Invpcid(..) => self.emit_x86_invpcid(op)?,

            OpKind::X86ReadDebug { .. } => self.emit_x86_read_debug(op)?,

            OpKind::X86WriteDebug { .. } => self.emit_x86_write_debug(op)?,
            OpKind::X86WriteControl { .. } => self.emit_x86_write_control(op)?,

            OpKind::X86FsGsBase { .. } => self.emit_x86_fsgsbase(op)?,

            OpKind::X86SwapGs { .. } => self.emit_x86_swapgs(op)?,

            OpKind::X86MonitorMwait(..) => self.emit_x86_monitor_mwait(op)?,

            OpKind::X86Pkru { .. } => self.emit_x86_pkru(op)?,

            OpKind::Undefined { .. } => {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_ud2();
            }

            // Unimplemented ops
            _ => {
                return Err(LowerError::UnsupportedOp {
                    op: format!("{:?}", op.kind),
                });
            }
            _ => unreachable!("lower_op: unhandled OpKind"),
        }

        Ok(())
    }
}
