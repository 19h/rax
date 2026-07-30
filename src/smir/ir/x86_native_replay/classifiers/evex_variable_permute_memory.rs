//! Exact EVEX variable VPERMILPS/PD memory-source classification.

use super::X86InstructionBytes;
use super::evex_memory::{memory_operand_end, vector_legacy_prefix_len};
use crate::smir::ir::types::{VecElementType, VecWidth};

/// One byte-validated EVEX variable VPERMILPS/PD memory encoding rewritten to
/// consume an equivalent nonarchitectural vector scratch register.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86EvexVariablePermuteMemoryEncoding {
    pub(crate) width: VecWidth,
    pub(crate) elem: VecElementType,
    pub(crate) destination: u8,
    pub(crate) source1: u8,
    pub(crate) scratch: u8,
    pub(crate) writemask: u8,
    pub(crate) zeroing: bool,
    pub(crate) broadcast: bool,
    pub(crate) memory_size: u32,
    pub(crate) register_instruction: X86InstructionBytes,
    pub(crate) needs_avx512vl: bool,
}

impl X86InstructionBytes {
    /// Validate one EVEX variable-control VPERMILPS/PD whose third operand is
    /// memory, and rewrite only that operand to a nonaliasing vector register.
    ///
    /// Both instructions are exception class E4NF and therefore always read
    /// the memory operand, irrespective of the destination writemask. EVEX.b
    /// changes the transfer from a full 128/256/512-bit control vector to a
    /// scalar 32/64-bit control broadcast. The register rewrite clears EVEX.b;
    /// the lowerer materializes an equivalent repeated control vector first.
    pub(crate) fn evex_variable_permute_memory_encoding(
        &self,
    ) -> Option<X86EvexVariablePermuteMemoryEncoding> {
        let bytes = self.as_slice();
        let start = vector_legacy_prefix_len(bytes);
        if bytes.get(start) != Some(&0x62) {
            return None;
        }

        let p0 = *bytes.get(start + 1)?;
        let p1 = *bytes.get(start + 2)?;
        let p2 = *bytes.get(start + 3)?;
        let opcode = *bytes.get(start + 4)?;
        let modrm_index = start + 5;
        let modrm = *bytes.get(modrm_index)?;
        let mask = p2 & 0x07;
        if p0 & 0x0F != 2
            || p1 & 0x07 != 0x05
            || p2 & 0x60 == 0x60
            || (p2 & 0x80 != 0 && mask == 0)
            || modrm >> 6 == 3
            || memory_operand_end(bytes, modrm_index)? != bytes.len()
        {
            return None;
        }

        let w = p1 & 0x80 != 0;
        let elem = match (opcode, w) {
            (0x0C, false) => VecElementType::F32,
            (0x0D, true) => VecElementType::F64,
            _ => return None,
        };
        let width = match (p2 >> 5) & 3 {
            0 => VecWidth::V128,
            1 => VecWidth::V256,
            2 => VecWidth::V512,
            _ => unreachable!("reserved vector length rejected"),
        };
        let destination =
            (u8::from(p0 & 0x80 == 0) << 3) | (u8::from(p0 & 0x10 == 0) << 4) | ((modrm >> 3) & 7);
        let source1 = ((!p1 >> 3) & 0x0F) | (u8::from(p2 & 0x08 == 0) << 4);
        let scratch = (0..32u8)
            .find(|candidate| *candidate != destination && *candidate != source1)
            .expect("two EVEX operands leave at least thirty scratch registers");
        let broadcast = p2 & 0x10 != 0;
        let memory_size = if broadcast {
            elem.bytes()
        } else {
            width.bytes()
        };

        let register_bytes = [
            0x62,
            (p0 & 0x97)
                | (u8::from(scratch & 0x10 == 0) << 6)
                | (u8::from(scratch & 0x08 == 0) << 5),
            p1 | 0x04,
            p2 & !0x10,
            opcode,
            0xC0 | (modrm & 0x38) | (scratch & 7),
        ];
        let register_instruction = X86InstructionBytes::new(&register_bytes).unwrap();
        let needs_avx512vl = width != VecWidth::V512;
        if register_instruction.evex_register_avx512f_permute_needs_vl() != Some(needs_avx512vl) {
            return None;
        }

        Some(X86EvexVariablePermuteMemoryEncoding {
            width,
            elem,
            destination,
            source1,
            scratch,
            writemask: mask,
            zeroing: p2 & 0x80 != 0,
            broadcast,
            memory_size,
            register_instruction,
            needs_avx512vl,
        })
    }
}
