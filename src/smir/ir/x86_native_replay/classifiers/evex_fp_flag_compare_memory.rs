//! EVEX scalar floating-point flag-comparison memory-source classification.

use super::X86InstructionBytes;
use super::evex_memory::{memory_operand_end, vector_legacy_prefix_len};
use crate::smir::ir::types::{MemWidth, VecElementType};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FpFlagCompareMemoryFields {
    source1: u8,
    elem: VecElementType,
    signaling: bool,
    ll: u8,
    memory_width: MemWidth,
}

/// Exact EVEX `VCOMISS`/`VCOMISD`/`VCOMISH` or
/// `VUCOMISS`/`VUCOMISD`/`VUCOMISH` memory encoding and its byte-validated
/// host-stack replay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86EvexFpFlagCompareMemoryEncoding {
    pub(crate) source1: u8,
    pub(crate) elem: VecElementType,
    pub(crate) signaling: bool,
    pub(crate) ll: u8,
    pub(crate) memory_width: MemWidth,
    pub(crate) stack_instruction: X86InstructionBytes,
    pub(crate) needs_avx512fp16: bool,
}

fn fp_flag_compare_memory_fields(
    bytes: &[u8],
) -> Option<(u8, u8, u8, u8, FpFlagCompareMemoryFields)> {
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
    let map = p0 & 7;
    let elem = match (map, p1 & 0x83) {
        (1, 0x00) => VecElementType::F32,
        (1, 0x81) => VecElementType::F64,
        (5, 0x00) => VecElementType::F16,
        _ => return None,
    };
    let ll = (p2 >> 5) & 3;
    if !matches!(opcode, 0x2E | 0x2F)
        || modrm >> 6 == 3
        || p2 & 0x9F != 0x08
        || ll == 3
        || memory_operand_end(bytes, modrm_index)? != bytes.len()
    {
        return None;
    }

    let source1 =
        ((modrm >> 3) & 7) | (u8::from(p0 & 0x80 == 0) << 3) | (u8::from(p0 & 0x10 == 0) << 4);
    let memory_width = match elem {
        VecElementType::F16 => MemWidth::B2,
        VecElementType::F32 => MemWidth::B4,
        VecElementType::F64 => MemWidth::B8,
        _ => unreachable!("validated scalar floating-point element"),
    };
    Some((
        p0,
        p1,
        p2,
        modrm,
        FpFlagCompareMemoryFields {
            source1,
            elem,
            signaling: opcode == 0x2F,
            ll,
            memory_width,
        },
    ))
}

impl X86InstructionBytes {
    /// Validate one EVEX scalar floating-point flag comparison with a memory
    /// source and synthesize its exact `[rsp]` replay.
    ///
    /// Intel SDM revision 092 defines these LLIG Tuple1 Scalar forms with one
    /// unconditional 2/4/8-byte Type-E3NF memory access. Memory operands use
    /// dynamic MXCSR exception control and therefore require `EVEX.b=0`; the
    /// three defined LLIG images are preserved and do not require AVX-512VL.
    /// EVEX.vvvv/V', z, b, and aaa are reserved. Segment/address-size prefixes
    /// and APX B4/X4 address extensions remain confined to helper evaluation.
    ///
    /// Classification is O(1) time and O(1) space because an x86 instruction
    /// is at most 15 bytes.
    pub(crate) fn evex_fp_flag_compare_memory_encoding(
        &self,
    ) -> Option<X86EvexFpFlagCompareMemoryEncoding> {
        let (p0, p1, p2, modrm, fields) = fp_flag_compare_memory_fields(self.as_slice())?;
        let opcode = if fields.signaling { 0x2F } else { 0x2E };

        // Independently validate all non-address controls through the existing
        // register-only replay classifier after selecting ordinary XMM0 as the
        // second operand and removing helper-owned APX address extensions.
        let register_probe = X86InstructionBytes::new(&[
            0x62,
            (p0 & 0x97) | 0x60,
            p1 | 0x04,
            p2,
            opcode,
            0xC0 | (modrm & 0x38),
        ])?;
        let requirements = match fields.elem {
            VecElementType::F16 => register_probe.evex_register_fp16_flag_compare_requirements(),
            VecElementType::F32 | VecElementType::F64 => {
                register_probe.evex_register_fp32_fp64_flag_compare_requirements()
            }
            _ => None,
        };
        if requirements != Some((false, fields.elem == VecElementType::F16)) {
            return None;
        }

        let stack_instruction = X86InstructionBytes::new(&[
            0x62,
            // Preserve R/R' and map, select ordinary unextended SIB
            // index/base, and remove APX B4 from the helper-owned address.
            (p0 & 0x97) | 0x60,
            // Preserve W/reserved vvvv/pp and restore ordinary EVEX.U after
            // removing APX X4 from the helper-owned address.
            p1 | 0x04,
            // Preserve LLIG and reserved V'; z/b/aaa were validated clear.
            p2,
            opcode,
            (modrm & 0x38) | 0x04,
            0x24,
        ])?;
        let (_, _, _, _, rewritten) = fp_flag_compare_memory_fields(stack_instruction.as_slice())?;
        if rewritten != fields {
            return None;
        }

        Some(X86EvexFpFlagCompareMemoryEncoding {
            source1: fields.source1,
            elem: fields.elem,
            signaling: fields.signaling,
            ll: fields.ll,
            memory_width: fields.memory_width,
            stack_instruction,
            needs_avx512fp16: fields.elem == VecElementType::F16,
        })
    }
}
