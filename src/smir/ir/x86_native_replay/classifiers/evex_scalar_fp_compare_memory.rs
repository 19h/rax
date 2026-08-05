//! EVEX scalar floating-point comparison memory-source classification.

use super::X86InstructionBytes;
use super::evex_memory::{memory_operand_end, vector_legacy_prefix_len};
use crate::smir::ir::types::{MemWidth, VecElementType};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ScalarFpCompareMemoryFields {
    elem: VecElementType,
    destination: u8,
    source1: u8,
    writemask: Option<u8>,
    predicate: u8,
    memory_width: MemWidth,
    ll: u8,
}

/// Exact EVEX scalar binary16/binary32/binary64 comparison memory encoding
/// and its byte-validated host-stack replay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86EvexScalarFpCompareMemoryEncoding {
    pub(crate) elem: VecElementType,
    pub(crate) destination: u8,
    pub(crate) source1: u8,
    pub(crate) writemask: Option<u8>,
    pub(crate) predicate: u8,
    pub(crate) ll: u8,
    pub(crate) memory_width: MemWidth,
    pub(crate) stack_instruction: X86InstructionBytes,
    pub(crate) needs_avx512fp16: bool,
}

fn scalar_fp_compare_memory_fields(
    bytes: &[u8],
) -> Option<(u8, u8, u8, u8, ScalarFpCompareMemoryFields)> {
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
    let operand_end = memory_operand_end(bytes, modrm_index)?;
    let predicate = *bytes.get(operand_end)?;
    let map = p0 & 0x07;
    let elem = match (map, p1 & 0x83) {
        (1, 0x02) => VecElementType::F32,
        (1, 0x83) => VecElementType::F64,
        (3, 0x02) => VecElementType::F16,
        _ => return None,
    };
    let mask = p2 & 0x07;
    let ll = (p2 >> 5) & 3;
    if p0 & 0x90 != 0x90
        || opcode != 0xC2
        || modrm >> 6 == 3
        || p2 & 0x90 != 0
        || ll == 3
        || predicate & !0x1F != 0
        || operand_end + 1 != bytes.len()
    {
        return None;
    }

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
        ScalarFpCompareMemoryFields {
            elem,
            destination: (modrm >> 3) & 7,
            source1: ((!p1 >> 3) & 0x0F) | (u8::from(p2 & 0x08 == 0) << 4),
            writemask: (mask != 0).then_some(mask),
            predicate,
            memory_width,
            ll,
        },
    ))
}

impl X86InstructionBytes {
    /// Validate one EVEX scalar `VCMPSH`/`VCMPSS`/`VCMPSD` memory source and
    /// synthesize an exact `[rsp]` replay.
    ///
    /// Intel SDM revision 092 assigns these Tuple1 Scalar operations Type E3
    /// exceptions. A memory source uses dynamic MXCSR control and requires
    /// `EVEX.b=0`; only writemask bit 0 controls the 2/4/8-byte access. The
    /// three defined LLIG images are retained byte-for-byte and do not require
    /// AVX-512VL. EVEX.z and immediate bits 7:5 are reserved because the
    /// destination is an opmask. Segment/address-size prefixes and APX B4/X4
    /// address extensions remain confined to helper address evaluation.
    pub(crate) fn evex_scalar_fp_compare_memory_encoding(
        &self,
    ) -> Option<X86EvexScalarFpCompareMemoryEncoding> {
        let bytes = self.as_slice();
        let (p0, p1, p2, modrm, fields) = scalar_fp_compare_memory_fields(bytes)?;

        // Independently validate the non-address semantic controls through
        // the register-only classifier after selecting an ordinary XMM0
        // source and clearing helper-owned APX address extensions.
        let register_probe = X86InstructionBytes::new(&[
            0x62,
            (p0 & 0x97) | 0x60,
            p1 | 0x04,
            p2,
            0xC2,
            0xC0 | (modrm & 0x38),
            fields.predicate,
        ])?;
        if register_probe.evex_register_fp_compare_requirements()
            != Some((false, fields.elem == VecElementType::F16))
        {
            return None;
        }

        let stack_instruction = X86InstructionBytes::new(&[
            0x62,
            // Preserve canonical K destination/map fields, select ordinary
            // unextended SIB index/base, and clear APX B4 for RSP.
            (p0 & 0x97) | 0x60,
            // Preserve W/vvvv/pp and restore ordinary EVEX.U.
            p1 | 0x04,
            // Preserve LLIG, V', and aaa; z/b were validated clear.
            p2,
            0xC2,
            (modrm & 0x38) | 0x04,
            0x24,
            fields.predicate,
        ])?;
        let (_, _, _, _, rewritten_fields) =
            scalar_fp_compare_memory_fields(stack_instruction.as_slice())?;
        if rewritten_fields != fields {
            return None;
        }

        Some(X86EvexScalarFpCompareMemoryEncoding {
            elem: fields.elem,
            destination: fields.destination,
            source1: fields.source1,
            writemask: fields.writemask,
            predicate: fields.predicate,
            ll: fields.ll,
            memory_width: fields.memory_width,
            stack_instruction,
            needs_avx512fp16: fields.elem == VecElementType::F16,
        })
    }
}
