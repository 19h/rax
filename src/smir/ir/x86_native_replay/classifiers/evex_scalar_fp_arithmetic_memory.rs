//! EVEX scalar floating-point arithmetic memory-source classification.

use super::X86InstructionBytes;
use super::evex_memory::{memory_operand_end, vector_legacy_prefix_len};
use crate::smir::ir::types::{MemWidth, VecElementType};

const OPCODES: [u8; 7] = [0x51, 0x58, 0x59, 0x5C, 0x5D, 0x5E, 0x5F];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ScalarFpArithmeticMemoryFields {
    elem: VecElementType,
    destination: u8,
    source1: u8,
    writemask: Option<u8>,
    zeroing: bool,
    opcode: u8,
    memory_width: MemWidth,
    ll: u8,
}

/// Exact EVEX scalar binary16/binary32/binary64 arithmetic or square-root
/// memory encoding and its byte-validated host-stack replay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86EvexScalarFpArithmeticMemoryEncoding {
    pub(crate) elem: VecElementType,
    pub(crate) destination: u8,
    pub(crate) source1: u8,
    pub(crate) writemask: Option<u8>,
    pub(crate) zeroing: bool,
    pub(crate) opcode: u8,
    pub(crate) memory_width: MemWidth,
    pub(crate) stack_instruction: X86InstructionBytes,
    pub(crate) needs_avx512fp16: bool,
}

fn scalar_fp_arithmetic_memory_fields(
    bytes: &[u8],
) -> Option<(usize, u8, u8, u8, u8, ScalarFpArithmeticMemoryFields)> {
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
    let map = p0 & 0x07;
    let elem = match (map, p1 & 0x83) {
        (1, 0x02) => VecElementType::F32,
        (1, 0x83) => VecElementType::F64,
        (5, 0x02) => VecElementType::F16,
        _ => return None,
    };
    let mask = p2 & 0x07;
    let zeroing = p2 & 0x80 != 0;
    let ll = (p2 >> 5) & 3;
    if !OPCODES.contains(&opcode)
        || p2 & 0x10 != 0
        || ll == 3
        || modrm >> 6 == 3
        || (zeroing && mask == 0)
        || memory_operand_end(bytes, modrm_index)? != bytes.len()
    {
        return None;
    }

    let destination =
        (u8::from(p0 & 0x80 == 0) << 3) | (u8::from(p0 & 0x10 == 0) << 4) | ((modrm >> 3) & 7);
    let source1 = ((!p1 >> 3) & 0x0F) | (u8::from(p2 & 0x08 == 0) << 4);
    let memory_width = match elem {
        VecElementType::F16 => MemWidth::B2,
        VecElementType::F32 => MemWidth::B4,
        VecElementType::F64 => MemWidth::B8,
        _ => unreachable!("validated scalar floating-point element"),
    };
    Some((
        start,
        p0,
        p1,
        p2,
        modrm,
        ScalarFpArithmeticMemoryFields {
            elem,
            destination,
            source1,
            writemask: (mask != 0).then_some(mask),
            zeroing,
            opcode,
            memory_width,
            ll,
        },
    ))
}

impl X86InstructionBytes {
    /// Validate one EVEX scalar `VADD`/`VMUL`/`VSUB`/`VMIN`/`VDIV`/`VMAX`/
    /// `VSQRT` binary16, binary32, or binary64 memory source and synthesize an
    /// exact `[rsp]` replay.
    ///
    /// Intel assigns these forms a Tuple1 Scalar operand and Type E3
    /// exceptions. A memory source always uses dynamic MXCSR control;
    /// `EVEX.b=1` is reserved. Only writemask bit 0 controls the 2/4/8-byte
    /// access. The three defined LLIG images are retained byte-for-byte and do
    /// not require AVX-512VL. Segment/address-size prefixes and APX B4/X4
    /// address extensions are consumed exclusively by helper address
    /// evaluation, so the native rewrite removes them and selects `[rsp]`.
    pub(crate) fn evex_scalar_fp_arithmetic_memory_encoding(
        &self,
    ) -> Option<X86EvexScalarFpArithmeticMemoryEncoding> {
        let bytes = self.as_slice();
        let (_start, p0, p1, p2, modrm, fields) = scalar_fp_arithmetic_memory_fields(bytes)?;
        let stack_instruction = X86InstructionBytes::new(&[
            0x62,
            // Preserve R/R' and the opcode map, select ordinary unextended
            // SIB index/base, and clear APX B4 for the RSP rewrite.
            (p0 & 0x97) | 0x60,
            // Preserve W/vvvv/pp and restore ordinary EVEX.U.
            p1 | 0x04,
            // Preserve z, LLIG, V', and aaa; b was validated clear.
            p2,
            fields.opcode,
            (modrm & 0x38) | 0x04,
            0x24,
        ])?;
        let (_, _, _, _, _, rewritten_fields) =
            scalar_fp_arithmetic_memory_fields(stack_instruction.as_slice())?;
        if rewritten_fields != fields {
            return None;
        }

        Some(X86EvexScalarFpArithmeticMemoryEncoding {
            elem: fields.elem,
            destination: fields.destination,
            source1: fields.source1,
            writemask: fields.writemask,
            zeroing: fields.zeroing,
            opcode: fields.opcode,
            memory_width: fields.memory_width,
            stack_instruction,
            needs_avx512fp16: fields.elem == VecElementType::F16,
        })
    }
}
