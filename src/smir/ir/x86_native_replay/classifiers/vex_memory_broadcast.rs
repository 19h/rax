//! AVX/AVX2 VEX memory-broadcast replay classification.

use super::X86InstructionBytes;
use crate::smir::ir::types::{VecElementType, VecWidth};

/// Architectural fields for one complete VEX memory-broadcast instruction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86VexMemoryBroadcastFields {
    pub(crate) destination: u8,
    pub(crate) elem: VecElementType,
    pub(crate) source_lanes: u8,
    pub(crate) width: VecWidth,
    pub(crate) opcode: u8,
    pub(crate) memory_size: u32,
    pub(crate) needs_avx2: bool,
}

impl X86InstructionBytes {
    /// Validate one complete AVX/AVX2 VEX memory-broadcast instruction.
    ///
    /// This covers `VBROADCASTSS`, `VBROADCASTSD`, `VBROADCASTF128`,
    /// `VPBROADCASTB/W/D/Q`, and `VBROADCASTI128`. Every form uses map 0F38,
    /// mandatory prefix 66H, W=0, and reserved VEX.vvvv=`1111b`.
    /// `VBROADCASTSD`, `VBROADCASTF128`, and `VBROADCASTI128` require VEX.256.
    /// The floating memory forms require AVX; the integer forms require AVX2.
    /// The shared parser validates the complete ModR/M/SIB/displacement shape
    /// and permits only segment/address-size legacy prefixes.
    pub(crate) fn vex_memory_broadcast_fields(&self) -> Option<X86VexMemoryBroadcastFields> {
        let fields = self.vex_memory_fields()?;
        if fields.source1 != 0 || fields.map != 2 || fields.pp != 1 || fields.w {
            return None;
        }
        let width = if fields.width_256 {
            VecWidth::V256
        } else {
            VecWidth::V128
        };
        let (elem, source_lanes, needs_avx2) = match (fields.opcode, fields.width_256) {
            (0x18, _) => (VecElementType::F32, 1, false),
            (0x19, true) => (VecElementType::F64, 1, false),
            (0x1A, true) => (VecElementType::F32, 4, false),
            (0x58, _) => (VecElementType::I32, 1, true),
            (0x59, _) => (VecElementType::I64, 1, true),
            (0x5A, true) => (VecElementType::I32, 4, true),
            (0x78, _) => (VecElementType::I8, 1, true),
            (0x79, _) => (VecElementType::I16, 1, true),
            _ => return None,
        };
        Some(X86VexMemoryBroadcastFields {
            destination: fields.destination,
            elem,
            source_lanes,
            width,
            opcode: fields.opcode,
            memory_size: u32::from(source_lanes) * elem.bytes(),
            needs_avx2,
        })
    }
}
