//! EVEX packed floating-point comparison memory-source classification.

use super::X86InstructionBytes;
use super::evex_memory::{memory_operand_end, vector_legacy_prefix_len};
use crate::smir::ir::types::{VecElementType, VecWidth};

/// Native replay strategy for one exact packed floating-point comparison
/// memory encoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum X86EvexPackedFpCompareMemoryReplay {
    /// A complete vector helper load followed by a register-source rewrite
    /// using one nonarchitectural low vector register.
    Vector {
        scratch: u8,
        register_instruction: X86InstructionBytes,
    },
    /// A scalar helper load followed by the original broadcast comparison
    /// rewritten to consume the staged value from `[rsp]`.
    Broadcast {
        stack_instruction: X86InstructionBytes,
    },
    /// Per-active-lane scalar helper loads accumulated in a nonarchitectural
    /// stack vector, followed by the original writemasked comparison rewritten
    /// to consume that vector from `[rsp]`.
    MaskedVector {
        stack_instruction: X86InstructionBytes,
    },
}

/// Exact EVEX packed floating-point comparison memory encoding and its
/// byte-validated native replay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86EvexPackedFpCompareMemoryEncoding {
    pub(crate) width: VecWidth,
    pub(crate) elem: VecElementType,
    pub(crate) destination: u8,
    pub(crate) source1: u8,
    pub(crate) writemask: Option<u8>,
    pub(crate) predicate: u8,
    pub(crate) replay: X86EvexPackedFpCompareMemoryReplay,
    pub(crate) needs_avx512vl: bool,
    pub(crate) needs_avx512fp16: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RegisterFields {
    width: VecWidth,
    elem: VecElementType,
    destination: u8,
    source1: u8,
    source2: u8,
    writemask: Option<u8>,
    predicate: u8,
}

fn packed_element(map: u8, p1: u8) -> Option<VecElementType> {
    match (map, p1 & 0x83) {
        (1, 0x00) => Some(VecElementType::F32),
        (1, 0x81) => Some(VecElementType::F64),
        (3, 0x00) => Some(VecElementType::F16),
        _ => None,
    }
}

impl X86InstructionBytes {
    /// Validate the non-SAE register-source shape used as an independent
    /// semantic probe for helper-backed packed comparison rewrites.
    fn evex_register_packed_fp_compare_fields(&self) -> Option<RegisterFields> {
        let [0x62, p0, p1, p2, 0xC2, modrm, predicate] = self.as_slice() else {
            return None;
        };
        let mask = p2 & 0x07;
        let ll = (p2 >> 5) & 3;
        let elem = packed_element(p0 & 0x07, *p1)?;
        if p0 & 0x90 != 0x90
            || p1 & 0x04 == 0
            || p2 & 0x90 != 0
            || ll == 3
            || modrm >> 6 != 3
            || predicate & !0x1F != 0
        {
            return None;
        }
        let width = match ll {
            0 => VecWidth::V128,
            1 => VecWidth::V256,
            2 => VecWidth::V512,
            _ => unreachable!("reserved packed vector length rejected"),
        };
        Some(RegisterFields {
            width,
            elem,
            destination: (modrm >> 3) & 7,
            source1: ((!p1 >> 3) & 0x0F) | (u8::from(p2 & 0x08 == 0) << 4),
            source2: (modrm & 7)
                | (u8::from(p0 & 0x20 == 0) << 3)
                | (u8::from(p0 & 0x40 == 0) << 4),
            writemask: (mask != 0).then_some(mask),
            predicate: *predicate,
        })
    }

    /// Validate one packed AVX-512 `VCMPPH`/`VCMPPS`/`VCMPPD` memory source
    /// and select an exact helper-backed native replay.
    ///
    /// Intel SDM revision 092 assigns these operations to Type E2. Map 0F,
    /// NP.W0 and 66.W1 select binary32 and binary64; map 0F3A, NP.W0 selects
    /// binary16. `L'L` selects 128/256/512 bits. For a memory source,
    /// `EVEX.b=1` selects m16/m32/m64 broadcast, never SAE. `EVEX.z` and
    /// immediate bits 7:5 are reserved because the destination is an opmask.
    ///
    /// Segment/address-size prefixes and APX B4/X4 address extensions remain
    /// confined to helper address evaluation. Every rewrite preserves the K
    /// destination and writemask, source vector, width, precision, and exact
    /// five-bit comparison predicate.
    pub(crate) fn evex_packed_fp_compare_memory_encoding(
        &self,
    ) -> Option<X86EvexPackedFpCompareMemoryEncoding> {
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
        let operand_end = memory_operand_end(bytes, modrm_index)?;
        let predicate = *bytes.get(operand_end)?;
        let mask = p2 & 0x07;
        let ll = (p2 >> 5) & 3;
        let elem = packed_element(p0 & 0x07, p1)?;
        if p0 & 0x90 != 0x90
            || opcode != 0xC2
            || modrm >> 6 == 3
            || p2 & 0x80 != 0
            || ll == 3
            || predicate & !0x1F != 0
            || operand_end + 1 != bytes.len()
        {
            return None;
        }

        let width = match ll {
            0 => VecWidth::V128,
            1 => VecWidth::V256,
            2 => VecWidth::V512,
            _ => unreachable!("reserved packed vector length rejected"),
        };
        let destination = (modrm >> 3) & 7;
        let source1 = ((!p1 >> 3) & 0x0F) | (u8::from(p2 & 0x08 == 0) << 4);
        let writemask = (mask != 0).then_some(mask);
        let broadcast = p2 & 0x10 != 0;
        let needs_avx512vl = width != VecWidth::V512;
        let needs_avx512fp16 = elem == VecElementType::F16;

        // Clear memory-broadcast selection and independently decode every
        // non-address semantic field as a legal dynamic-MXCSR register form.
        let register_probe = X86InstructionBytes::new(&[
            0x62,
            (p0 & 0x97) | 0x60,
            p1 | 0x04,
            p2 & !0x10,
            opcode,
            0xC0 | (modrm & 0x38),
            predicate,
        ])
        .unwrap();
        let expected_probe = RegisterFields {
            width,
            elem,
            destination,
            source1,
            source2: 0,
            writemask,
            predicate,
        };
        if register_probe.evex_register_packed_fp_compare_fields() != Some(expected_probe) {
            return None;
        }

        let stack_instruction = || {
            X86InstructionBytes::new(&[
                0x62,
                // Preserve canonical K destination/map fields, select an
                // unextended SIB index/base, and clear APX B4 for RSP.
                (p0 & 0x97) | 0x60,
                // Preserve W/vvvv/pp and restore ordinary EVEX.U.
                p1 | 0x04,
                // Preserve L'L, broadcast, V', and aaa exactly; z is zero.
                p2,
                opcode,
                (modrm & 0x38) | 0x04,
                0x24,
                predicate,
            ])
            .unwrap()
        };

        let replay = if broadcast {
            X86EvexPackedFpCompareMemoryReplay::Broadcast {
                stack_instruction: stack_instruction(),
            }
        } else if writemask.is_some() {
            X86EvexPackedFpCompareMemoryReplay::MaskedVector {
                stack_instruction: stack_instruction(),
            }
        } else {
            let scratch = (0..16u8)
                .find(|candidate| *candidate != source1)
                .expect("one source cannot consume every low vector register");
            let register_instruction = X86InstructionBytes::new(&[
                0x62,
                // Register EVEX.X/B encode scratch bits 4/3 with inverted
                // polarity. Clear APX B4 and retain the K destination/map.
                (p0 & 0x97) | 0x40 | if scratch & 8 == 0 { 0x20 } else { 0 },
                p1 | 0x04,
                p2,
                opcode,
                0xC0 | (modrm & 0x38) | (scratch & 7),
                predicate,
            ])
            .unwrap();
            let expected = RegisterFields {
                source2: scratch,
                ..expected_probe
            };
            if register_instruction.evex_register_packed_fp_compare_fields() != Some(expected) {
                return None;
            }
            X86EvexPackedFpCompareMemoryReplay::Vector {
                scratch,
                register_instruction,
            }
        };

        Some(X86EvexPackedFpCompareMemoryEncoding {
            width,
            elem,
            destination,
            source1,
            writemask,
            predicate,
            replay,
            needs_avx512vl,
            needs_avx512fp16,
        })
    }
}
