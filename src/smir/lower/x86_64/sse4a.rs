//! State-backed native lowering for AMD SSE4A EXTRQ/INSERTQ.

use crate::smir::ir::ops::{OpKind, SmirOp, X86Sse4aBitfieldKind};
use crate::smir::ir::types::{ArchReg, OpWidth, VReg, X86Reg};
use crate::smir::lower::regalloc::PhysReg;
use crate::smir::lower::{LowerError, X86_GUEST_ZMM_OFFSET};

use super::{X86_64Lowerer, X86Emitter};

fn low_xmm_index(reg: VReg) -> Option<u8> {
    match reg {
        VReg::Arch(ArchReg::X86(X86Reg::Xmm(index @ 0..=15))) => Some(index),
        _ => None,
    }
}

pub(crate) fn x86_sse4a_bitfield_shape_valid(op: &SmirOp) -> bool {
    let OpKind::X86Sse4aBitfield {
        dst,
        source,
        kind,
        length,
        index,
    } = &op.kind
    else {
        return false;
    };
    let (Some(dst_index), Some(_)) = (low_xmm_index(*dst), low_xmm_index(*source)) else {
        return false;
    };
    if op.x86_hint.is_some()
        || !matches!((length, index), (Some(0..=63), Some(0..=63)) | (None, None))
    {
        return false;
    }
    *kind != X86Sse4aBitfieldKind::Extract
        || length.is_none()
        || low_xmm_index(*source) == Some(dst_index)
}

impl X86_64Lowerer {
    /// Compute in the marshalled ZMM slot with GPR-only host instructions.
    /// Every scratch GPR and the complete native flags image are pushed before
    /// use, so the identity-mapped architectural GPR file remains unchanged.
    pub(crate) fn emit_x86_sse4a_bitfield(&mut self, op: &SmirOp) -> Result<(), LowerError> {
        if !x86_sse4a_bitfield_shape_valid(op) {
            return Err(LowerError::InvalidOperand {
                op: "X86Sse4aBitfield".to_string(),
                operand: "requires exact unhinted low-XMM operands and paired controls".to_string(),
            });
        }
        let OpKind::X86Sse4aBitfield {
            dst,
            source,
            kind,
            length,
            index,
        } = &op.kind
        else {
            unreachable!("validated SSE4A bitfield operation changed kind");
        };
        let dst_offset = X86_GUEST_ZMM_OFFSET + i32::from(low_xmm_index(*dst).unwrap()) * 64;
        let source_offset = X86_GUEST_ZMM_OFFSET + i32::from(low_xmm_index(*source).unwrap()) * 64;

        self.code.emit_u8(0x9C); // pushfq
        self.code.emit_bytes(&[
            0x50, // push rax: state pointer
            0x51, // push rcx: variable shift count
            0x52, // push rdx: destination/result
            0x57, // push rdi: source payload/control
            0x41, 0x50, // push r8: effective mask
            0x41, 0x51, // push r9: index/control
        ]);
        self.emit_load_state_ptr_rax();
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_rm(PhysReg::Rdx, PhysReg::Rax, dst_offset, OpWidth::W64);

            if *kind == X86Sse4aBitfieldKind::Insert || length.is_none() {
                emitter.emit_mov_rm(PhysReg::Rdi, PhysReg::Rax, source_offset, OpWidth::W64);
            }
            match (*length, *index) {
                (Some(length), Some(index)) => {
                    emitter.emit_mov_ri(PhysReg::Rcx, i64::from(length), OpWidth::W64);
                    emitter.emit_mov_ri(PhysReg::R9, i64::from(index), OpWidth::W64);
                }
                (None, None) => {
                    if *kind == X86Sse4aBitfieldKind::Insert {
                        emitter.emit_mov_rm(
                            PhysReg::R9,
                            PhysReg::Rax,
                            source_offset + 8,
                            OpWidth::W64,
                        );
                    } else {
                        emitter.emit_mov_rr(PhysReg::R9, PhysReg::Rdi, OpWidth::W64);
                    }
                    emitter.emit_mov_rr(PhysReg::Rcx, PhysReg::R9, OpWidth::W64);
                    emitter.emit_and_ri(PhysReg::Rcx, 0x3F, OpWidth::W64);
                    emitter.emit_shr_ri(PhysReg::R9, 8, OpWidth::W64);
                    emitter.emit_and_ri(PhysReg::R9, 0x3F, OpWidth::W64);
                }
                _ => unreachable!("shape validator requires paired SSE4A controls"),
            }

            // mask = u64::MAX >> ((-length) & 63). This maps encoded length 0
            // to 64 bits and every 1..63 length to its exact low-bit mask.
            emitter.emit_mov_ri_imm64(PhysReg::R8, -1);
            emitter.emit_neg(PhysReg::Rcx, OpWidth::W64);
            emitter.emit_shr_cl(PhysReg::R8, OpWidth::W64);
            emitter.emit_mov_rr(PhysReg::Rcx, PhysReg::R9, OpWidth::W64);

            match kind {
                X86Sse4aBitfieldKind::Extract => {
                    emitter.emit_shr_cl(PhysReg::Rdx, OpWidth::W64);
                    emitter.emit_and_rr(PhysReg::Rdx, PhysReg::R8, OpWidth::W64);
                }
                X86Sse4aBitfieldKind::Insert => {
                    emitter.emit_and_rr(PhysReg::Rdi, PhysReg::R8, OpWidth::W64);
                    emitter.emit_shl_cl(PhysReg::Rdi, OpWidth::W64);
                    emitter.emit_shl_cl(PhysReg::R8, OpWidth::W64);
                    emitter.emit_not(PhysReg::R8, OpWidth::W64);
                    emitter.emit_and_rr(PhysReg::Rdx, PhysReg::R8, OpWidth::W64);
                    emitter.emit_or_rr(PhysReg::Rdx, PhysReg::Rdi, OpWidth::W64);
                }
            }
            emitter.emit_mov_mr(PhysReg::Rax, dst_offset, PhysReg::Rdx, OpWidth::W64);
        }
        self.code.emit_bytes(&[
            0x41, 0x59, // pop r9
            0x41, 0x58, // pop r8
            0x5F, // pop rdi
            0x5A, // pop rdx
            0x59, // pop rcx
            0x58, // pop rax
            0x9D, // popfq
        ]);
        Ok(())
    }
}
