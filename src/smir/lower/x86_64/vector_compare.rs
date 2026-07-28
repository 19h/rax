//! State-backed native lowering for strict-lifted AMD XOP VPCOM.

use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint};
use crate::smir::ir::types::{
    ArchReg, DispSize, OpWidth, VReg, VecCmpCond, VecElementType, X86Reg,
};
use crate::smir::lower::regalloc::PhysReg;
use crate::smir::lower::{LowerError, X86_GUEST_ZMM_OFFSET};

use super::{X86_64Lowerer, X86Cond, X86Emitter};

pub(crate) fn x86_state_vcmp_reg_index(reg: VReg) -> Option<u8> {
    match reg {
        VReg::Arch(ArchReg::X86(X86Reg::Xmm(index @ 0..=15))) => Some(index),
        _ => None,
    }
}

pub(crate) fn x86_state_vcmp_element_width(elem: VecElementType, lanes: u8) -> Option<OpWidth> {
    let width = match elem {
        VecElementType::I8 => OpWidth::W8,
        VecElementType::I16 => OpWidth::W16,
        VecElementType::I32 => OpWidth::W32,
        VecElementType::I64 => OpWidth::W64,
        _ => return None,
    };
    (u32::from(lanes) * elem.bytes() == 16).then_some(width)
}

pub(crate) fn x86_state_vcmp_candidate(op: &SmirOp) -> bool {
    matches!(op.x86_hint, Some(X86OpHint::XopVpcom)) && matches!(op.kind, OpKind::VCmp { .. })
}

pub(crate) fn x86_state_vcmp_shape_valid(op: &SmirOp) -> bool {
    let OpKind::VCmp {
        dst,
        src1,
        src2,
        elem,
        lanes,
        ..
    } = op.kind
    else {
        return false;
    };
    x86_state_vcmp_candidate(op)
        && x86_state_vcmp_element_width(elem, lanes).is_some()
        && [dst, src1, src2]
            .into_iter()
            .all(|reg| x86_state_vcmp_reg_index(reg).is_some())
}

fn host_condition(cond: VecCmpCond) -> Option<X86Cond> {
    match cond {
        VecCmpCond::Eq => Some(X86Cond::E),
        VecCmpCond::Ne => Some(X86Cond::Ne),
        VecCmpCond::Lt => Some(X86Cond::L),
        VecCmpCond::Le => Some(X86Cond::Le),
        VecCmpCond::Gt => Some(X86Cond::G),
        VecCmpCond::Ge => Some(X86Cond::Ge),
        VecCmpCond::Ltu => Some(X86Cond::B),
        VecCmpCond::Leu => Some(X86Cond::Be),
        VecCmpCond::Gtu => Some(X86Cond::A),
        VecCmpCond::Geu => Some(X86Cond::Ae),
        VecCmpCond::False | VecCmpCond::True => None,
    }
}

impl X86_64Lowerer {
    fn emit_state_vcmp_lane_load(
        &mut self,
        destination: PhysReg,
        state_offset: i32,
        lane_offset: i32,
        width: OpWidth,
    ) {
        let mut emitter = X86Emitter::new(&mut self.code);
        if width == OpWidth::W64 {
            emitter.emit_mov_rm(destination, PhysReg::Rax, state_offset + lane_offset, width);
        } else {
            emitter.emit_movzx_rm_disp(
                destination,
                PhysReg::Rax,
                state_offset + lane_offset,
                DispSize::Auto,
                width,
                OpWidth::W64,
            );
        }
    }

    /// Compare two explicit vector-slot images lane by lane and commit a
    /// canonical 128-bit all-zero/all-one result.
    ///
    /// Each lane reads both operands before its same-position destination lane
    /// commits, so every destination/source alias is exact. PUSHFQ/POPFQ makes
    /// the scalar host comparisons transparent to guest status flags.
    pub(crate) fn emit_x86_state_vcmp(
        &mut self,
        dst_index: u8,
        src1_offset: i32,
        src2_offset: i32,
        elem: VecElementType,
        lanes: u8,
        cond: VecCmpCond,
        physical_input_indices: &[u8],
    ) -> Result<(), LowerError> {
        let Some(width) = x86_state_vcmp_element_width(elem, lanes) else {
            return Err(LowerError::InvalidOperand {
                op: "state-backed VCmp".to_string(),
                operand: format!("requires a 128-bit integer lane shape, got {elem:?}x{lanes}"),
            });
        };
        if dst_index > 15 || physical_input_indices.iter().any(|index| *index > 15) {
            return Err(LowerError::InvalidOperand {
                op: "state-backed VCmp".to_string(),
                operand: "requires low XMM architectural operands".to_string(),
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
        let element_bytes = elem.bytes() as i32;
        self.code.emit_u8(0x9C); // pushfq
        self.code.emit_bytes(&[
            0x50, // push rax: state pointer
            0x53, // push rbx: left lane
            0x51, // push rcx: right lane
            0x52, // push rdx: Boolean/result lane
        ]);
        self.emit_load_state_ptr_rax();

        for lane_offset in (0..16).step_by(element_bytes as usize) {
            let lane_offset = lane_offset as i32;
            match cond {
                VecCmpCond::False | VecCmpCond::True => {
                    let value = if cond == VecCmpCond::True { -1 } else { 0 };
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_mov_mi_disp(
                        PhysReg::Rax,
                        destination_offset + lane_offset,
                        DispSize::Auto,
                        value,
                        width,
                    );
                }
                _ => {
                    self.emit_state_vcmp_lane_load(PhysReg::Rbx, src1_offset, lane_offset, width);
                    self.emit_state_vcmp_lane_load(PhysReg::Rcx, src2_offset, lane_offset, width);
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_xor_rr(PhysReg::Rdx, PhysReg::Rdx, OpWidth::W32);
                    emitter.emit_cmp_rr(PhysReg::Rbx, PhysReg::Rcx, width);
                    emitter.emit_setcc(
                        host_condition(cond).expect("non-constant VCmp condition"),
                        PhysReg::Rdx,
                    );
                    emitter.emit_neg(PhysReg::Rdx, width);
                    emitter.emit_mov_mr(
                        PhysReg::Rax,
                        destination_offset + lane_offset,
                        PhysReg::Rdx,
                        width,
                    );
                }
            }
        }
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            for offset in (16..64).step_by(8) {
                emitter.emit_mov_mi_disp(
                    PhysReg::Rax,
                    destination_offset + offset,
                    DispSize::Auto,
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

    pub(crate) fn emit_x86_state_vcmp_op(&mut self, op: &SmirOp) -> Result<(), LowerError> {
        if !x86_state_vcmp_shape_valid(op) {
            return Err(LowerError::InvalidOperand {
                op: "state-backed VCmp".to_string(),
                operand: "requires exact XOP VPCOM provenance and low XMM integer operands"
                    .to_string(),
            });
        }
        let OpKind::VCmp {
            dst,
            src1,
            src2,
            cond,
            elem,
            lanes,
        } = op.kind
        else {
            unreachable!("validated VCmp operation changed kind");
        };
        let dst_index = x86_state_vcmp_reg_index(dst).unwrap();
        let src1_index = x86_state_vcmp_reg_index(src1).unwrap();
        let src2_index = x86_state_vcmp_reg_index(src2).unwrap();
        let slot = |index| X86_GUEST_ZMM_OFFSET + i32::from(index) * 64;
        self.emit_x86_state_vcmp(
            dst_index,
            slot(src1_index),
            slot(src2_index),
            elem,
            lanes,
            cond,
            &[src1_index, src2_index],
        )
    }
}
