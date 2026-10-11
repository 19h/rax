//! Encoding shapes shared by integer scalar and vector AdvSIMD families.

use crate::smir::ir::ops::{OpKind, SmirOp};
use crate::smir::ir::types::{VLaneOp, VecElementType, VecReduceOp, VecUnaryOp, VecWidth};
use crate::smir::lower::LowerError;

use super::Aarch64Lowerer;

pub(super) struct IntegerSimdShape {
    q: u32,
    pub(super) size: u32,
    scalar: bool,
}

impl IntegerSimdShape {
    pub(super) fn encoding_bits(&self) -> u32 {
        (self.q << 30) | (u32::from(self.scalar) << 28) | (self.size << 22)
    }

    pub(super) fn result_width(&self) -> VecWidth {
        if self.scalar || self.q == 0 {
            VecWidth::V64
        } else {
            VecWidth::V128
        }
    }
}

/// Newly supported scalar shapes must retain the AArch64 vector-state ABI.
pub(crate) fn integer_scalar_provenance_valid(operation: &SmirOp) -> bool {
    let scalar_shape = match operation.kind {
        OpKind::VAdd {
            elem: VecElementType::I64,
            lanes: 1,
            ..
        }
        | OpKind::VSub {
            elem: VecElementType::I64,
            lanes: 1,
            ..
        }
        | OpKind::VShift {
            elem: VecElementType::I64,
            lanes: 1,
            ..
        }
        | OpKind::VLane {
            elem: VecElementType::I64,
            lanes: 1,
            op: VLaneOp::Add | VLaneOp::Sub,
            ..
        }
        | OpKind::VUnary {
            elem: VecElementType::I64,
            lanes: 1,
            op: VecUnaryOp::Abs | VecUnaryOp::Neg,
            ..
        }
        | OpKind::VReduce {
            elem: VecElementType::I64,
            lanes: 2,
            op: VecReduceOp::Add,
            ..
        } => true,
        _ => false,
    };
    !scalar_shape || operation.x86_hint.is_none()
}

impl Aarch64Lowerer {
    pub(super) fn simd_scalar_or_vector_integer_shape(
        elem: VecElementType,
        lanes: u8,
    ) -> Result<IntegerSimdShape, LowerError> {
        if elem == VecElementType::I64 && lanes == 1 {
            return Ok(IntegerSimdShape {
                q: 1,
                size: 3,
                scalar: true,
            });
        }
        let (q, size) = Self::simd_integer_shape(elem, lanes)?;
        Ok(IntegerSimdShape {
            q,
            size,
            scalar: false,
        })
    }

    pub(super) fn emit_integer_simd_three_same(
        &mut self,
        rd: u8,
        rn: u8,
        rm: u8,
        shape: &IntegerSimdShape,
        u: u32,
        opcode: u32,
    ) {
        self.emit(
            0x0e20_0400
                | shape.encoding_bits()
                | (u << 29)
                | (u32::from(rm) << 16)
                | (opcode << 11)
                | (u32::from(rn) << 5)
                | u32::from(rd),
        );
    }

    pub(super) fn emit_integer_simd_two_misc(
        &mut self,
        rd: u8,
        rn: u8,
        shape: &IntegerSimdShape,
        u: u32,
        opcode: u32,
    ) {
        self.emit(
            0x0e20_0800
                | shape.encoding_bits()
                | (u << 29)
                | (opcode << 12)
                | (u32::from(rn) << 5)
                | u32::from(rd),
        );
    }

    pub(super) fn emit_integer_simd_shift_immediate(
        &mut self,
        rd: u8,
        rn: u8,
        shape: &IntegerSimdShape,
        u: u32,
        encoded_amount: u32,
        opcode: u32,
    ) {
        // Shift-immediate instructions encode the element size in immh:immb.
        // Only Q and the scalar bit come from the register shape here.
        let register_form = shape.encoding_bits() & !(3 << 22);
        self.emit(
            0x0f00_0400
                | register_form
                | (u << 29)
                | (encoded_amount << 16)
                | (opcode << 11)
                | (u32::from(rn) << 5)
                | u32::from(rd),
        );
    }
}
