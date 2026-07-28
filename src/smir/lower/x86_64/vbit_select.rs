//! State-backed native lowering for width-bounded vector bit selection.

use crate::smir::ir::ops::{OpKind, SmirOp};
use crate::smir::ir::types::{ArchReg, OpWidth, VReg, VecWidth, X86Reg};
use crate::smir::lower::regalloc::PhysReg;
use crate::smir::lower::{LowerError, X86_GUEST_ZMM_OFFSET};

use super::{X86_64Lowerer, X86Emitter};

pub(crate) fn x86_vbit_select_reg_index(reg: VReg, width: VecWidth) -> Option<u8> {
    match (reg, width) {
        (VReg::Arch(ArchReg::X86(X86Reg::Xmm(index @ 0..=15))), VecWidth::V128)
        | (VReg::Arch(ArchReg::X86(X86Reg::Ymm(index @ 0..=15))), VecWidth::V256) => Some(index),
        _ => None,
    }
}

pub(crate) fn x86_vbit_select_shape_valid(op: &SmirOp) -> bool {
    let OpKind::VBitSelect {
        dst,
        mask,
        src_true,
        src_false,
        width,
    } = &op.kind
    else {
        return false;
    };
    op.x86_hint.is_none()
        && matches!(width, VecWidth::V128 | VecWidth::V256)
        && [dst, mask, src_true, src_false]
            .into_iter()
            .all(|reg| x86_vbit_select_reg_index(*reg, *width).is_some())
}

impl X86_64Lowerer {
    /// Compute a VBitSelect against explicit GuestRegs vector-slot offsets.
    ///
    /// Each 64-bit word snapshots all three inputs before the corresponding
    /// destination word commits, so every same-register alias is exact. The
    /// operation clears every destination byte above `width`, matching the
    /// canonical interpreter's width-bounded write.
    pub(crate) fn emit_x86_vbit_select_state(
        &mut self,
        dst_index: u8,
        mask_offset: i32,
        true_offset: i32,
        false_offset: i32,
        width: VecWidth,
        physical_input_indices: &[u8],
    ) -> Result<(), LowerError> {
        if dst_index > 15
            || !matches!(width, VecWidth::V128 | VecWidth::V256)
            || physical_input_indices.iter().any(|index| *index > 15)
        {
            return Err(LowerError::InvalidOperand {
                op: "VBitSelect".to_string(),
                operand: "requires low XMM/V128 or YMM/V256 architectural operands".to_string(),
            });
        }

        if self.native_vector_state_active {
            self.code.emit_u8(0x50); // push guest rax
            self.emit_load_state_ptr_rax();
            let mut synchronized = [false; 16];
            for index in physical_input_indices {
                if !synchronized[usize::from(*index)] {
                    self.emit_state_backed_xmm_sync(*index, true);
                    synchronized[usize::from(*index)] = true;
                }
            }
            self.code.emit_u8(0x58); // pop guest rax
        }

        let destination_offset = X86_GUEST_ZMM_OFFSET + i32::from(dst_index) * 64;
        let active_bytes = width.bytes() as i32;
        self.code.emit_u8(0x9C); // pushfq
        self.code.emit_bytes(&[
            0x50, // push rax: state pointer
            0x53, // push rbx: mask / inverted mask
            0x51, // push rcx: true contribution / result
            0x52, // push rdx: false contribution
        ]);
        self.emit_load_state_ptr_rax();

        for offset in (0..active_bytes).step_by(8) {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_rm(
                PhysReg::Rbx,
                PhysReg::Rax,
                mask_offset + offset,
                OpWidth::W64,
            );
            emitter.emit_mov_rm(
                PhysReg::Rcx,
                PhysReg::Rax,
                true_offset + offset,
                OpWidth::W64,
            );
            emitter.emit_mov_rm(
                PhysReg::Rdx,
                PhysReg::Rax,
                false_offset + offset,
                OpWidth::W64,
            );
            emitter.emit_and_rr(PhysReg::Rcx, PhysReg::Rbx, OpWidth::W64);
            emitter.emit_not(PhysReg::Rbx, OpWidth::W64);
            emitter.emit_and_rr(PhysReg::Rdx, PhysReg::Rbx, OpWidth::W64);
            emitter.emit_or_rr(PhysReg::Rcx, PhysReg::Rdx, OpWidth::W64);
            emitter.emit_mov_mr(
                PhysReg::Rax,
                destination_offset + offset,
                PhysReg::Rcx,
                OpWidth::W64,
            );
        }
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            for offset in (active_bytes..64).step_by(8) {
                emitter.emit_mov_mi_disp(
                    PhysReg::Rax,
                    destination_offset + offset,
                    crate::smir::ir::types::DispSize::Auto,
                    0,
                    OpWidth::W64,
                );
            }
        }
        self.code.emit_bytes(&[
            0x5A, // pop rdx
            0x59, // pop rcx
            0x5B, // pop rbx
            0x58, // pop rax
            0x9D, // popfq
        ]);

        if self.native_vector_state_active {
            self.code.emit_u8(0x50); // push guest rax
            self.emit_load_state_ptr_rax();
            self.emit_state_backed_xmm_sync(dst_index, false);
            self.code.emit_u8(0x58); // pop guest rax
        }
        Ok(())
    }

    pub(crate) fn emit_x86_vbit_select(&mut self, op: &SmirOp) -> Result<(), LowerError> {
        if !x86_vbit_select_shape_valid(op) {
            return Err(LowerError::InvalidOperand {
                op: "VBitSelect".to_string(),
                operand: "requires exact unhinted low XMM/V128 or YMM/V256 operands".to_string(),
            });
        }
        let OpKind::VBitSelect {
            dst,
            mask,
            src_true,
            src_false,
            width,
        } = &op.kind
        else {
            unreachable!("validated VBitSelect operation changed kind");
        };
        let dst_index = x86_vbit_select_reg_index(*dst, *width).unwrap();
        let mask_index = x86_vbit_select_reg_index(*mask, *width).unwrap();
        let true_index = x86_vbit_select_reg_index(*src_true, *width).unwrap();
        let false_index = x86_vbit_select_reg_index(*src_false, *width).unwrap();
        let slot = |index| X86_GUEST_ZMM_OFFSET + i32::from(index) * 64;
        self.emit_x86_vbit_select_state(
            dst_index,
            slot(mask_index),
            slot(true_index),
            slot(false_index),
            *width,
            &[mask_index, true_index, false_index],
        )
    }
}
