//! Fixed-predicate VEX packed-integer memory-source replay classification.

use super::X86InstructionBytes;
use crate::smir::ir::types::{VecCmpCond, VecElementType, VecWidth};

impl X86InstructionBytes {
    /// Verify that the complete encoded instruction is the fixed-predicate
    /// VEX packed-integer memory-source comparison represented by the supplied
    /// architectural operands and hint fields.
    pub(crate) fn is_vex_memory_fixed_integer_compare(
        &self,
        destination: u8,
        source1: u8,
        elem: VecElementType,
        cond: VecCmpCond,
        width: VecWidth,
        w: bool,
    ) -> bool {
        let (expected_map, expected_opcode) = match (elem, cond) {
            (VecElementType::I8, VecCmpCond::Gt) => (1, 0x64),
            (VecElementType::I16, VecCmpCond::Gt) => (1, 0x65),
            (VecElementType::I32, VecCmpCond::Gt) => (1, 0x66),
            (VecElementType::I8, VecCmpCond::Eq) => (1, 0x74),
            (VecElementType::I16, VecCmpCond::Eq) => (1, 0x75),
            (VecElementType::I32, VecCmpCond::Eq) => (1, 0x76),
            (VecElementType::I64, VecCmpCond::Eq) => (2, 0x29),
            (VecElementType::I64, VecCmpCond::Gt) => (2, 0x37),
            _ => return false,
        };
        let Some(fields) = self.vex_memory_fields() else {
            return false;
        };
        fields.destination == destination
            && fields.source1 == source1
            && fields.map == expected_map
            && fields.pp == 1
            && fields.opcode == expected_opcode
            && fields.width_256 == (width == VecWidth::V256)
            && matches!(width, VecWidth::V128 | VecWidth::V256)
            && fields.w == w
    }
}
