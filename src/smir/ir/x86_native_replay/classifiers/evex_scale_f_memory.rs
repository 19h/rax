//! EVEX VSCALEFPD/PS/PH/SD/SS/SH memory-source classification.

use super::X86InstructionBytes;
use super::evex_memory::{memory_operand_end, vector_legacy_prefix_len};
use crate::smir::ir::types::{VecElementType, VecWidth};

/// Native replay strategy for one exact VSCALEF memory encoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum X86EvexScaleFMemoryReplay {
    /// A complete vector helper load followed by a register-source rewrite
    /// using one nonarchitectural low vector register.
    Vector {
        scratch: u8,
        register_instruction: X86InstructionBytes,
    },
    /// A scalar helper load followed by the original packed broadcast
    /// operation rewritten to consume the staged value from `[rsp]`.
    Broadcast {
        stack_instruction: X86InstructionBytes,
    },
    /// Per-active-lane scalar helper loads accumulated in a nonarchitectural
    /// stack vector, followed by the original writemasked packed operation
    /// rewritten to consume that vector from `[rsp]`.
    MaskedVector {
        stack_instruction: X86InstructionBytes,
    },
    /// A scalar helper load followed by the original scalar operation
    /// rewritten to consume the staged value from `[rsp]`.
    Scalar {
        stack_instruction: X86InstructionBytes,
    },
}

/// Exact VSCALEF memory encoding and its byte-validated native replay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86EvexScaleFMemoryEncoding {
    pub(crate) width: VecWidth,
    pub(crate) elem: VecElementType,
    pub(crate) destination: u8,
    pub(crate) source1: u8,
    pub(crate) writemask: Option<u8>,
    pub(crate) zeroing: bool,
    pub(crate) scalar: bool,
    pub(crate) replay: X86EvexScaleFMemoryReplay,
    pub(crate) needs_avx512vl: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RegisterFields {
    width: VecWidth,
    elem: VecElementType,
    destination: u8,
    source1: u8,
    source2: u8,
    writemask: Option<u8>,
    zeroing: bool,
    scalar: bool,
}

fn scale_f_element(map: u8, w: bool) -> Option<VecElementType> {
    match (map, w) {
        (2, false) => Some(VecElementType::F32),
        (2, true) => Some(VecElementType::F64),
        (6, false) => Some(VecElementType::F16),
        _ => None,
    }
}

impl X86InstructionBytes {
    /// Validate the dynamic-rounding register-source shape used as an
    /// independent semantic probe for helper-backed VSCALEF rewrites.
    fn evex_register_scale_f_fields(&self) -> Option<RegisterFields> {
        let bytes = self.as_slice();
        if bytes.len() != 6 || bytes[0] != 0x62 {
            return None;
        }
        let p0 = bytes[1];
        let p1 = bytes[2];
        let p2 = bytes[3];
        let opcode = bytes[4];
        let modrm = bytes[5];
        let scalar = opcode == 0x2D;
        let mask = p2 & 0x07;
        let ll = (p2 >> 5) & 3;
        let elem = scale_f_element(p0 & 0x07, p1 & 0x80 != 0)?;
        if p1 & 0x07 != 5
            || !matches!(opcode, 0x2C | 0x2D)
            || modrm >> 6 != 3
            || p2 & 0x10 != 0
            || (!scalar && ll == 3)
            || (p2 & 0x80 != 0 && mask == 0)
        {
            return None;
        }
        let width = if scalar {
            VecWidth::V128
        } else {
            match ll {
                0 => VecWidth::V128,
                1 => VecWidth::V256,
                2 => VecWidth::V512,
                _ => unreachable!("reserved packed vector length rejected"),
            }
        };
        Some(RegisterFields {
            width,
            elem,
            destination: (u8::from(p0 & 0x80 == 0) << 3)
                | (u8::from(p0 & 0x10 == 0) << 4)
                | ((modrm >> 3) & 7),
            source1: ((!p1 >> 3) & 0x0F) | (u8::from(p2 & 0x08 == 0) << 4),
            source2: (modrm & 7)
                | (u8::from(p0 & 0x20 == 0) << 3)
                | (u8::from(p0 & 0x40 == 0) << 4),
            writemask: (mask != 0).then_some(mask),
            zeroing: p2 & 0x80 != 0,
            scalar,
        })
    }

    /// Validate one packed or scalar AVX-512 VSCALEF memory source and select
    /// an exact helper-backed native replay.
    ///
    /// VSCALEFPS/PD use map 0F38 opcode 2CH/2DH with W selecting
    /// binary32/binary64. VSCALEFPH/SH use MAP6 with W=0. Packed L'L selects
    /// 128/256/512 bits and memory EVEX.b selects m16/m32/m64 broadcast.
    /// Scalar L'L is ignored and scalar memory EVEX.b is reserved.
    ///
    /// Segment/address-size prefixes and APX B4/X4 extensions remain confined
    /// to helper address evaluation. Rewrites preserve every architectural
    /// vector operand, opmask, zeroing policy, vector length, and precision.
    /// Scalar helper replay canonicalizes architecturally ignored L'L to 00B
    /// so the newly emitted host instruction is deterministic across CPUs.
    pub(crate) fn evex_scale_f_memory_encoding(&self) -> Option<X86EvexScaleFMemoryEncoding> {
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
        let scalar = opcode == 0x2D;
        let mask = p2 & 0x07;
        let ll = (p2 >> 5) & 3;
        let elem = scale_f_element(p0 & 0x07, p1 & 0x80 != 0)?;
        if p1 & 0x03 != 1
            || !matches!(opcode, 0x2C | 0x2D)
            || modrm >> 6 == 3
            || (scalar && p2 & 0x10 != 0)
            || (!scalar && ll == 3)
            || (p2 & 0x80 != 0 && mask == 0)
            || operand_end != bytes.len()
        {
            return None;
        }

        let width = if scalar {
            VecWidth::V128
        } else {
            match ll {
                0 => VecWidth::V128,
                1 => VecWidth::V256,
                2 => VecWidth::V512,
                _ => unreachable!("reserved packed vector length rejected"),
            }
        };
        let destination =
            (u8::from(p0 & 0x80 == 0) << 3) | (u8::from(p0 & 0x10 == 0) << 4) | ((modrm >> 3) & 7);
        let source1 = ((!p1 >> 3) & 0x0F) | (u8::from(p2 & 0x08 == 0) << 4);
        let writemask = (mask != 0).then_some(mask);
        let zeroing = p2 & 0x80 != 0;
        let broadcast = !scalar && p2 & 0x10 != 0;
        let needs_avx512vl = !scalar && width != VecWidth::V512;

        // Reconstruct a dynamic-rounding register form and independently
        // decode every non-address semantic field before selecting a rewrite.
        let register_probe = X86InstructionBytes::new(&[
            0x62,
            (p0 & 0x97) | 0x60,
            p1 | 0x04,
            p2 & !0x10,
            opcode,
            0xC0 | (modrm & 0x38),
        ])
        .unwrap();
        let expected_probe = RegisterFields {
            width,
            elem,
            destination,
            source1,
            source2: 0,
            writemask,
            zeroing,
            scalar,
        };
        if register_probe.evex_register_scale_f_fields() != Some(expected_probe) {
            return None;
        }

        let stack_instruction = || {
            X86InstructionBytes::new(&[
                0x62,
                // Preserve R/R' and the opcode map, select unextended SIB
                // index/base, and clear APX B4 for the rewritten RSP base.
                (p0 & 0x97) | 0x60,
                // Preserve W/vvvv/pp and restore the ordinary EVEX.U bit.
                p1 | 0x04,
                // Preserve every meaningful control. Scalar L'L is ignored by
                // the guest ISA, so canonicalize it for hosted replay; packed
                // L'L still selects the architectural vector width.
                if scalar { p2 & !0x60 } else { p2 },
                opcode,
                (modrm & 0x38) | 0x04,
                0x24,
            ])
            .unwrap()
        };

        let replay = if scalar {
            X86EvexScaleFMemoryReplay::Scalar {
                stack_instruction: stack_instruction(),
            }
        } else if broadcast {
            X86EvexScaleFMemoryReplay::Broadcast {
                stack_instruction: stack_instruction(),
            }
        } else if writemask.is_some() {
            X86EvexScaleFMemoryReplay::MaskedVector {
                stack_instruction: stack_instruction(),
            }
        } else {
            let scratch = (0..16u8)
                .find(|candidate| *candidate != destination && *candidate != source1)
                .expect("two operands cannot consume every low vector register");
            let register_instruction = X86InstructionBytes::new(&[
                0x62,
                // Register EVEX.X/B encode scratch bits 4/3 with inverted
                // polarity. Clear APX B4 and retain destination extensions.
                (p0 & 0x97) | 0x40 | if scratch & 8 == 0 { 0x20 } else { 0 },
                p1 | 0x04,
                p2,
                opcode,
                0xC0 | (modrm & 0x38) | (scratch & 7),
            ])
            .unwrap();
            let expected = RegisterFields {
                source2: scratch,
                ..expected_probe
            };
            if register_instruction.evex_register_scale_f_fields() != Some(expected) {
                return None;
            }
            X86EvexScaleFMemoryReplay::Vector {
                scratch,
                register_instruction,
            }
        };

        Some(X86EvexScaleFMemoryEncoding {
            width,
            elem,
            destination,
            source1,
            writemask,
            zeroing,
            scalar,
            replay,
            needs_avx512vl,
        })
    }
}
