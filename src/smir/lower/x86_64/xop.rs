//! State-backed native lowering for AMD XOP packed rotate/shift operations.

use crate::smir::ir::ops::{OpKind, SmirOp, X86XopPackedBitKind};
use crate::smir::ir::types::{ArchReg, OpWidth, SrcOperand, VReg, VecElementType, X86Reg};
use crate::smir::lower::regalloc::PhysReg;
use crate::smir::lower::{LowerError, X86_GUEST_ZMM_OFFSET};

use super::{X86_64Lowerer, X86Cond, X86Emitter};

#[derive(Clone, Copy)]
pub(crate) enum X86XopStateCount {
    Memory(i32),
    Immediate(u8),
}

pub(crate) fn x86_low_xmm_index(reg: VReg) -> Option<u8> {
    match reg {
        VReg::Arch(ArchReg::X86(X86Reg::Xmm(index @ 0..=15))) => Some(index),
        _ => None,
    }
}

pub(crate) fn x86_xop_packed_bit_shape_valid(op: &SmirOp) -> bool {
    let OpKind::X86XopPackedBit {
        dst,
        src,
        count,
        elem,
        ..
    } = &op.kind
    else {
        return false;
    };
    let operands_valid = op.x86_hint.is_none()
        && x86_low_xmm_index(*dst).is_some()
        && x86_low_xmm_index(*src).is_some();
    let count_valid = matches!(
        count,
        SrcOperand::Reg(reg) if x86_low_xmm_index(*reg).is_some()
    ) || matches!(count, SrcOperand::Imm(value) if (0..=255).contains(value));
    operands_valid
        && count_valid
        && matches!(
            elem,
            VecElementType::I8 | VecElementType::I16 | VecElementType::I32 | VecElementType::I64
        )
}

impl X86_64Lowerer {
    /// Compute one XOP packed-bit operation against explicit GuestRegs offsets.
    /// The source or count may name the nonarchitectural vector-memory scratch
    /// slot. Corresponding-lane evaluation is alias-safe: each lane reads both
    /// inputs before writing that lane, and no later lane reads an earlier lane.
    pub(crate) fn emit_x86_xop_packed_bit_state(
        &mut self,
        dst_index: u8,
        src_offset: i32,
        count: X86XopStateCount,
        elem: VecElementType,
        kind: X86XopPackedBitKind,
    ) -> Result<(), LowerError> {
        if dst_index > 15
            || !matches!(
                elem,
                VecElementType::I8
                    | VecElementType::I16
                    | VecElementType::I32
                    | VecElementType::I64
            )
        {
            return Err(LowerError::InvalidOperand {
                op: "X86XopPackedBit".to_string(),
                operand: "requires a low-XMM destination and integer element width".to_string(),
            });
        }
        let element_bytes = elem.bytes() as i32;
        let width = match elem {
            VecElementType::I8 => OpWidth::W8,
            VecElementType::I16 => OpWidth::W16,
            VecElementType::I32 => OpWidth::W32,
            VecElementType::I64 => OpWidth::W64,
            _ => unreachable!("validated XOP element type"),
        };
        let lanes = 16 / element_bytes;
        let dst_offset = X86_GUEST_ZMM_OFFSET + i32::from(dst_index) * 64;

        self.code.emit_u8(0x9C); // pushfq
        self.code.emit_bytes(&[
            0x50, // push rax: state pointer
            0x51, // push rcx: signed count / CL
            0x52, // push rdx: lane payload/result
        ]);
        self.emit_load_state_ptr_rax();

        for lane in 0..lanes {
            let lane_offset = lane * element_bytes;
            {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_mov_rm(PhysReg::Rdx, PhysReg::Rax, src_offset + lane_offset, width);
                match count {
                    X86XopStateCount::Memory(offset) => emitter.emit_mov_rm(
                        PhysReg::Rcx,
                        PhysReg::Rax,
                        offset + lane_offset,
                        OpWidth::W8,
                    ),
                    X86XopStateCount::Immediate(value) => {
                        emitter.emit_mov_ri(PhysReg::Rcx, i64::from(value), OpWidth::W8)
                    }
                }
                emitter.emit_test_rr(PhysReg::Rcx, PhysReg::Rcx, OpWidth::W8);
            }
            let negative = self.emit_jcc_placeholder(X86Cond::S);
            {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_and_ri(PhysReg::Rcx, i64::from(element_bytes * 8 - 1), OpWidth::W8);
                match kind {
                    X86XopPackedBitKind::Rotate => emitter.emit_rol_cl(PhysReg::Rdx, width),
                    X86XopPackedBitKind::LogicalShift | X86XopPackedBitKind::ArithmeticShift => {
                        emitter.emit_shl_cl(PhysReg::Rdx, width)
                    }
                }
            }
            self.code.emit_u8(0xE9);
            let lane_done = self.code.position();
            self.code.emit_u32(0);

            self.patch_rel32_to_current(negative)?;
            {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_neg(PhysReg::Rcx, OpWidth::W8);
                emitter.emit_and_ri(PhysReg::Rcx, i64::from(element_bytes * 8 - 1), OpWidth::W8);
                match kind {
                    X86XopPackedBitKind::Rotate => emitter.emit_ror_cl(PhysReg::Rdx, width),
                    X86XopPackedBitKind::LogicalShift => emitter.emit_shr_cl(PhysReg::Rdx, width),
                    X86XopPackedBitKind::ArithmeticShift => {
                        emitter.emit_sar_cl(PhysReg::Rdx, width)
                    }
                }
            }
            self.patch_rel32_to_current(lane_done)?;
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_mr(PhysReg::Rax, dst_offset + lane_offset, PhysReg::Rdx, width);
        }

        // XOP is VEX-like: every form clears destination bits 511:128.
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            for offset in (16..64).step_by(8) {
                emitter.emit_mov_mi_disp(
                    PhysReg::Rax,
                    dst_offset + offset,
                    crate::smir::ir::types::DispSize::Auto,
                    0,
                    OpWidth::W64,
                );
            }
        }
        self.code.emit_bytes(&[
            0x5A, // pop rdx
            0x59, // pop rcx
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

    pub(crate) fn emit_x86_xop_packed_bit(&mut self, op: &SmirOp) -> Result<(), LowerError> {
        if !x86_xop_packed_bit_shape_valid(op) {
            return Err(LowerError::InvalidOperand {
                op: "X86XopPackedBit".to_string(),
                operand: "requires exact unhinted low-XMM operands and an 8-bit count".to_string(),
            });
        }
        let OpKind::X86XopPackedBit {
            dst,
            src,
            count,
            elem,
            kind,
        } = &op.kind
        else {
            unreachable!("validated X86XopPackedBit operation changed kind");
        };
        let source_index = x86_low_xmm_index(*src).unwrap();
        if self.native_vector_state_active {
            let count_index = match count {
                SrcOperand::Reg(reg) => Some(x86_low_xmm_index(*reg).unwrap()),
                SrcOperand::Imm(_) => None,
                _ => unreachable!("validated XOP count source"),
            };
            self.code.emit_u8(0x50); // push guest rax
            self.emit_load_state_ptr_rax();
            self.emit_state_backed_xmm_sync(source_index, true);
            if let Some(index) = count_index.filter(|index| *index != source_index) {
                self.emit_state_backed_xmm_sync(index, true);
            }
            self.code.emit_u8(0x58); // pop guest rax
        }
        let source = X86_GUEST_ZMM_OFFSET + i32::from(source_index) * 64;
        let count = match count {
            SrcOperand::Reg(reg) => X86XopStateCount::Memory(
                X86_GUEST_ZMM_OFFSET + i32::from(x86_low_xmm_index(*reg).unwrap()) * 64,
            ),
            SrcOperand::Imm(value) => X86XopStateCount::Immediate(*value as u8),
            _ => unreachable!("validated XOP count source"),
        };
        self.emit_x86_xop_packed_bit_state(
            x86_low_xmm_index(*dst).unwrap(),
            source,
            count,
            *elem,
            *kind,
        )
    }
}
